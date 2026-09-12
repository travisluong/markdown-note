# Architecture implementation progress

The full objective is the staged plan in [the audit](architecture-audit.md):
complete the architectural improvements, improve performance, and preserve
behavior, prioritizing Windows. Completion requires current implementation and
verification evidence for every milestone; passing a subset is not completion.
Native desktop tests remain user-run under AGENTS.md.

| Milestone | Status | Evidence / remaining work |
| --- | --- | --- |
| 1. Editor redo policy | Implemented and verified | `tests/editor_history.rs`: 11 headless tests passed, including divergent typing/commands/IME, no-op replacements and generic History compatibility; complete Windows/Linux suites passed. |
| 2. Reliable baseline | Windows/Linux verified; macOS execution pending | Feature docs corrected; dedicated formatting commit; strict Clippy passes without added suppressions; Rust 1.98.1 pinned; separate three-platform validation CI configured. Release packaging unchanged. |
| 3. Lifecycle/edit fixes and integration coverage | Implemented and verified | F2 picker/error/tree-retarget cancellation, F3 revalidation, F4 inline image insertion and F5 discarded overlays repaired. F6 foreground ownership and table/list/save/navigation headless coverage passed. A separately reproduced close-time input loss was repaired; all 91 application tests passed afterward. |
| 4. Data/storage boundaries and file policy | Implemented and verified on Windows/Linux | Library exposes format/I/O, notebook domain owners and shared values. Disk versions guard saves; conflict pause retains editor/history; app-owned moves revalidate saved bytes. Metadata and optimistic race limits are explicit in `document-storage.md`. Both full headless suites passed. |
| 5. Document-session transition owner | Pending | Explicit operation identities/effects, dependent-state notifications and delayed/failure/cancel/move/close tests. |
| 6. Bounded settings writer | Implemented and verified on Windows/Linux | Pure one-running/latest-per-destination model and one GPUI worker loop replace the task chain. Ten model/disk tests and six actual lifecycle tests pass, including 20,000 submissions, target-book flush, unrelated failures, close retry/cancel/explicit discard and late preference changes. |
| 7. UI and command ownership | Pending | Settings/explorer/palette/search owners, command catalogue and explicit capabilities/imports. |
| 8. Editor boundary and analysis performance | In progress | Complete vendor patch inventory verified against the published 0.5.1 package. Shared GFM analysis verified: mixed queued requests use one parse, plain requests use zero; 67 editor tests passed, preserving incremental/cache/work bounds. Initial Reading parse moved off layout; 8 controlled lifecycle tests passed. Thin editor adapter remains. |

## Verification rules

Initial clean architecture baseline, Rust/Cargo 1.98.1 (later results below):

| Platform | Verification |
| --- | --- |
| Windows/MSVC | Default `cargo check --locked --offline`, `cargo fmt --check`, strict all-target diagnostics Clippy and full diagnostics-enabled suite passed. **188 tests**, zero failures or ignored tests. |
| Ubuntu 26.04 WSL, x86_64 | Default `cargo check --locked` and full diagnostics-enabled suite passed. **185 tests**, zero failures or ignored tests; display variables unset. |
| macOS | Validation CI configured, not executed locally. No macOS result is inferred from another platform. |

Windows coverage: 90 application, 2 code highlighting, 63 editor, 11 history,
2 editor transactions, 5 image editor, 3 images, 3 Reading search and 9 tables.
Linux has the same 98 integration tests and 87 application tests; the three
Windows-only cases cover sharing-locked saves, junctions and canonical drive/UNC
notebook paths. No source was pushed and no native application was launched.

Windows compilation encountered generated-file linker access errors (LNK1104/
LNK1114); rebuilding the affected headless targets under the approved user
context succeeded, followed by the complete suite with `-j 1`. The compiler's
existing `proc-macro-error2 2.0.1` future-compatibility warning remains.

Lifecycle coverage includes picker outcomes, failed activation/open, prompt
cancellation, disabled autosave, stale/duplicate foreground completions, queued
save/navigation, tree retargeting and discarded search overlays. Picker outcomes
enter production completion handlers because TestPlatform does not implement
native folder/file picking. List/task/table diagnostics run actual InputState
transactions on TestPlatform without benchmark progress-file side effects.

After that full baseline, `notebook_ui::close_tests` reproduced accepted input
being lost while a clean close awaited settings persistence. The test uses the
registered close callback and ordinary text-input handler, and verifies actual
window removal. Notifying the view when close acquires ownership rejects that
input. The focused test and the complete Windows application target passed
(**91 tests**) after this separate repair; the baseline integration code is
unchanged.

Two additional TestPlatform tests reproduced the retained-dirty-buffer autosave
failure after actual Discard navigation retargeted to the current note or a
folder. Both failed before the repair; the subsequent notebook UI test filter
passed all seven tests after routing retained-buffer exits through
`resume_document`. Virtual time, disposable notebooks and production prompt/tree
handlers are used; the old note is saved and the abandoned target stays intact.

Run the default application check and diagnostics-enabled headless suite after
behavioral changes; use focused tests while developing each repair. Establish
passing formatting and strict lint gates before broad extraction. Track test
counts and scope, not just a green command. Do not claim native rendering, real
input, platform execution or latency results from static analysis alone.

Preserve the dated audit as historical evidence. Update ARCHITECTURE.md when
current ownership/contracts change, and update this table as milestones are
implemented and verified. Keep each improvement in a separate commit.

## Data and storage extraction

Notebook schema, path policy, tree values and storage now have separate owners
behind the existing explicit facade. All 23 notebook tests moved with their
owners. `src/lib.rs` exposes these modules, document storage, date templates
and the shared view preference. Public document tests cover BOM/Unicode, all
newline conventions including mixed files, invalid inputs and Windows sharing
locks. The application still owns the saved rope and live editor.

After extraction, Windows default check passed; the focused diagnostics run
passed 25 library tests, 64 application tests, 4 public document tests and 67
editor tests (160 total). The new count moves existing tests between targets;
no tests were ignored. These are headless results; file conflict policy remains
the next separate storage behavior change.

## Shared Live-preview analysis

A queued mixed document previously invoked up to four independent full GFM
parsers. Its link, table, fence and image visitors now borrow one worker-local
tree. Exact per-request diagnostics measured zero GFM calls/bytes for plain
text and one call consuming the mixed source length. Visitor equivalence covers
Unicode, CRLF, reference definitions, nested containers and unfinished fences;
table incremental metadata and moved code-cache identity remain intact. All 67
editor tests passed, including existing large-fixture and queue work bounds.
This is a reduction in parsing work, not a measured native frame-latency result.

## Reading background initialization

Initial Reading parsing no longer runs in request_layout. One running worker
and one latest pending source/type request replace the former unbounded update
channel. Eight controlled TestPlatform tests passed on Windows, covering cold
layout, stale successes/errors, coalescing, style and callback updates, exact
rendered source identity, find/reveal, retained scroll, HTML switching and view
teardown. An unchanged shared source has a pointer fast path before exact text
equality. No native rendering latency or physical interaction result is inferred.

## Settings writer policy

`notebook::SettingsWriter` retains one running job and the latest pending settings
per destination, plus one latest index snapshot with a book-revision dependency.
Captured flush barriers accept newer successful revisions; failed latest snapshots
wait for explicit retry or a new edit. An unrelated failed notebook does not block
a newer active index. Ten Windows tests passed, including 20,000 submissions,
fair scheduling, stale completions, failure/retry paths, both notebooks reaching
disk and corrupt/unsupported JSON preservation.

The application now uses one worker loop rather than a chain of captured tasks.
A typed `SettingsWaiter` retains the foreground token across target-book flush,
close and retry prompts. Close refreshes all revision requirements before removal,
including preferences changed while busy or while its error prompt is open. Six
TestPlatform cases use disposable notebooks, the registered close callback, real
preference writes, simulated prompt answers and executor time. All six passed on
Windows and Linux as part of their full 240-test suites. The independent same-book
reload preference race is being repaired in a separate change.

## Preference changes during a move

A new TestPlatform regression reproduced a mode mismatch: changing to Source
while a note moved left the editor in Source but restored Live-preview settings
from the worker's older snapshot. Move completion now remaps only paths in the
current settings. The regression failed before the fix and passed afterward.

## Exact moved-file paths

The cross-platform move/save regression exposed an empty-suffix path remap that
appended a trailing separator to an exact filename. Path equality ignored the
extra separator, but subsequent file I/O rejected it. The path policy now returns
the destination unchanged for an exact match. A regression comparing the OS
spelling and opening the file failed before the fix on Windows and passed on
Windows and Linux afterward; the actual Linux moved-buffer save/history test
also passed. Nested and unrelated paths keep their existing mapping behavior.

## Conflict-aware document saves

The storage boundary now returns raw-byte/file-identity disk versions and
write receipts. Saves check the captured version before staging and replacement,
refuse missing/read-only/link targets, preserve Unix mode bits, and retain an
accurate committed baseline even when receipt identity observation fails.
Application conflicts retain text/history and suspend automatic retries until
an explicit successful Save or fresh installation. App-owned moves revalidate
saved bytes and preserve the active buffer and undo history. The complete
[storage contract](document-storage.md) documents metadata and race limitations.

After the Windows sharing-handle repair and exact-path fix, full diagnostics
suites passed on **Windows (240 tests)** and **Linux (240 tests)**, with no failures
or ignored tests. Counts are 35 library + 75 application + 130 integration on
Windows, and 35 + 73 + 132 on Linux. Windows storage-specific coverage includes
sharing/readonly failures, actual junction rejection and open-file rename
restrictions; Linux adds Unix mode bits, parent link/rename and concurrent receipt
interference. The suites also include the separately integrated settings driver.
No native desktop was launched; macOS execution remains pending.

After these storage and settings integrations, the Windows default build check,
format check and strict all-target diagnostics Clippy gate passed. Clippy required
one behavior-preserving `Option` early-return simplification in the new worker.
The existing dependency future-compatibility warning remains. The prior Linux
default check and full 240-test run also passed; no macOS execution is claimed.
