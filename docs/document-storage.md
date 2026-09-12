# Document storage contract

`src/document` owns decoding, disk-version evidence and replacement. It has no
GPUI dependencies. `open` returns canonical path, normalized text, original
BOM/newline format and an opaque `DiskVersion`. The application retains the
saved rope separately from the current editor buffer.

## Save and conflict decisions

A version contains exact raw bytes, including BOM and newline spelling, and a
file identity when the filesystem provides one. Bytes and identity come from
the same opened handle. Opening a selected symlink follows its canonical target;
subsequent saves use that canonical path. Each save compares the current file against that version
before staging and immediately before replacement. Same-length edits, format-only
changes and a replacement with identical bytes are detected when identity is
available. Missing files are not recreated. Read-only files, directories, and
new filesystem links or Windows reparse points are refused.

The parent path is also checked for links, canonical location and available
identity. Staging is beside the destination, the complete encoded file is synced,
and one atomic replacement commits it. When a parent change is detected,
automatic cleanup is disabled; a staged file may remain in its old directory.

`WriteReceipt` contains the bytes actually written and identity obtained from
the committed file handle. It never adopts a reread of the destination: another
process may already have edited or replaced that pathname. Failure to observe
identity after the commit still reports a successful write with known bytes.
The next save continues to compare bytes. When an expected identity is known,
an unavailable current identity is a conflict rather than permission to overwrite.

These are optimistic checks, not a filesystem compare-and-swap. Another process
can race the final check and path-based replacement, or the parent check and
path-based temporary-file cleanup. Windows comparison handles
deny ordinary in-place writes while bytes are read; the final handle must close
before MoveFileExW can replace that file. Unix checks cannot prevent a writer
from changing bytes during a read and restoring the observed metadata. No file
watcher, force-overwrite action or automatic conflict merge is introduced.

## Application behavior

A successful save advances only the matching document's captured saved text,
rope and disk version. Typing during a save remains dirty. A conflict retains
text and history, cancels the debounce timer and pauses automatic retries.
Further edits do not remove that pause; explicit Save checks the same baseline.
An ordinary failure during that explicit retry preserves the conflict pause.
A successful explicit save or installation of a newly opened document clears it.

An app-owned move remaps the active path and revalidates the original saved bytes
with `version_after_move`; some filesystems may change identity during rename.
This operation never adopts changed bytes. If revalidation fails after the move,
the application keeps the actual moved path and buffer and pauses saving for
explicit recovery. Only this known successful move authorizes identity rebinding.

## Metadata and durability

Unix saves copy the current destination's permission bits, including changes made
during staging. They do not promise preservation of ownership, ACLs or extended
attributes. Windows retains tempfile's MoveFileExW replacement behavior; destination
ACLs, alternate data streams and other attributes are not copied. Hard links keep
the previous inode after replacement. Existing UTF-8 BOM and newline conventions
remain; mixed endings normalize to the first observed convention.

The staged file is synced before replacement. Parent-directory synchronization,
arbitrary remote-filesystem failures and full crash durability are outside this
contract. Those guarantees require a separate platform policy, not an inference
from atomic replacement.

## Regression entry points

- `tests/document.rs`: format round trips, invalid inputs and Windows sharing locks.
- `tests/document_conflicts.rs`: bytes, identity, missing/read-only/link targets,
  consecutive receipts, app-owned moves and Unix permission behavior.
- `src/document/storage/tests.rs`: changes during staging, parent replacement,
  Windows junctions and receipt identity after external interference.
- `src/lifecycle_tests.rs`: actual editor conflict pause/retry, clean manual Save,
  cancellation of queued autosave and moved-file save/history behavior.

Tests use disposable files and GPUI TestPlatform. Platform execution and complete
suite results are recorded in [architecture progress](architecture-progress.md).
