# Architecture implementation progress

The full objective is the staged plan in [the audit](architecture-audit.md):
complete the architectural improvements, improve performance, and preserve
behavior, prioritizing Windows. Completion requires current implementation and
verification evidence for every milestone; passing a subset is not completion.
Native desktop tests remain user-run under AGENTS.md.

| Milestone | Status | Evidence / remaining work |
| --- | --- | --- |
| 1. Editor redo policy | Implemented and verified | `tests/editor_history.rs`: 11 headless tests passed, including divergent typing/commands/IME, no-op replacements and generic History compatibility; complete 168-test suite passed. |
| 2. Reliable baseline | In progress | Feature documentation corrected; Rust 1.98.1 pinned and separate three-platform validation CI configured. Format, strict lint and platform execution remain. Release packaging workflow unchanged. |
| 3. Lifecycle/edit fixes and integration coverage | In progress | F2 picker/error cancellation, F3 revalidation, F4 inline image insertion and F5 discarded overlays repaired. F6 foreground ownership and table/list/save/navigation headless coverage passed. Additional retained-dirty-buffer tree-retarget autosave regression is being reproduced separately. |
| 4. Data/storage boundaries and file policy | Pending | Library entry point; document and notebook modules; conflict detection and metadata policy with platform tests. |
| 5. Document-session transition owner | Pending | Explicit operation identities/effects, dependent-state notifications and delayed/failure/cancel/move/close tests. |
| 6. Bounded settings writer | Pending | Coalescing per destination, flush/error semantics and multi-notebook stress tests. |
| 7. UI and command ownership | Pending | Settings/explorer/palette/search owners, command catalogue and explicit capabilities/imports. |
| 8. Editor boundary and analysis performance | In progress | Complete vendor patch inventory verified against the published 0.5.1 package. Thin editor adapter, off-layout initial Reading parse, shared analysis and measured work bounds remain. |

## Verification rules

Latest Windows run: `cargo test --locked --offline --features editor-diagnostics
-j 2` passed all 168 tests (72 application, 2 code highlighting, 63 editor, 11
history, 5 image editor, 3 images, 3 Reading search, 9 tables). Application
lifecycle coverage includes file-picker cancellation, folder-picker outcomes,
failed activation/open, save-prompt cancellation and disabled autosave. Picker
outcomes enter the production completion handlers because GPUI TestPlatform
does not implement the native folder/file picker.

The subsequent F5 search lifecycle target passed three additional tests for
published/debounced discarded overlays and clean-navigation state preservation.

Subsequent Windows checks under the pinned compiler: the full application target
passed 88 tests after F6 operation ownership (including gate, save/pending/stale
completion and four tree-retarget cases). The new `editor_transactions` target
passed both list/task/selection and table-command diagnostics on TestPlatform.
The default application check passed. This is not yet a new full-suite result.
Linux/macOS jobs are configured but have not been executed from this Windows
workspace; no source was pushed.

Run the default application check and diagnostics-enabled headless suite after
behavioral changes; use focused tests while developing each repair. Establish
passing formatting and strict lint gates before broad extraction. Track test
counts and scope, not just a green command. Do not claim native rendering, real
input, platform execution or latency results from static analysis alone.

Preserve the dated audit as historical evidence. Update ARCHITECTURE.md when
current ownership/contracts change, and update this table as milestones are
implemented and verified. Keep each improvement in a separate commit.
