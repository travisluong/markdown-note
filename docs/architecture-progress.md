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
| 4. Data/storage boundaries and file policy | In progress | Library entry point exposes document format/I/O, notebook domain owners and shared view preference. Public storage regression tests cover BOM, newline conventions, invalid files and failed replacement. Conflict detection and metadata policy remain. |
| 5. Document-session transition owner | Pending | Explicit operation identities/effects, dependent-state notifications and delayed/failure/cancel/move/close tests. |
| 6. Bounded settings writer | Pending | Coalescing per destination, flush/error semantics and multi-notebook stress tests. |
| 7. UI and command ownership | Pending | Settings/explorer/palette/search owners, command catalogue and explicit capabilities/imports. |
| 8. Editor boundary and analysis performance | In progress | Complete vendor patch inventory verified against the published 0.5.1 package. Shared GFM analysis verified: mixed queued requests use one parse, plain requests use zero; 67 editor tests passed, preserving incremental/cache/work bounds. Thin editor adapter and off-layout initial Reading parse remain. |

## Verification rules

Current baseline, Rust/Cargo 1.98.1:

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
