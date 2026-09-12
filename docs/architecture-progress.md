# Architecture implementation progress

The full objective is the staged plan in [the audit](architecture-audit.md):
complete the architectural improvements, improve performance, and preserve
behavior, prioritizing Windows. Completion requires current implementation and
verification evidence for every milestone; passing a subset is not completion.
Native desktop tests remain user-run under AGENTS.md.

| Milestone | Status | Evidence / remaining work |
| --- | --- | --- |
| 1. Editor redo policy | Implemented and verified | `tests/editor_history.rs`: 11 headless tests passed, including divergent typing/commands/IME, no-op replacements and generic History compatibility; complete 168-test suite passed. |
| 2. Reliable baseline | Pending | Correct feature docs, format, lint, separate validation CI, explicit toolchain and platform checks. |
| 3. Lifecycle/edit fixes and integration coverage | In progress | F2 cancellation, F3 revalidation, F4 inline image insertion and F5 discarded overlays repaired and verified. F6 operation ownership and broader table/save integration coverage remain. |
| 4. Data/storage boundaries and file policy | Pending | Library entry point; document and notebook modules; conflict detection and metadata policy with platform tests. |
| 5. Document-session transition owner | Pending | Explicit operation identities/effects, dependent-state notifications and delayed/failure/cancel/move/close tests. |
| 6. Bounded settings writer | Pending | Coalescing per destination, flush/error semantics and multi-notebook stress tests. |
| 7. UI and command ownership | Pending | Settings/explorer/palette/search owners, command catalogue and explicit capabilities/imports. |
| 8. Editor boundary and analysis performance | Pending | Thin editor adapter, vendor patch inventory, off-layout initial Reading parse, shared analysis investigation and measured work bounds. |

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

Run the default application check and diagnostics-enabled headless suite after
behavioral changes; use focused tests while developing each repair. Establish
passing formatting and strict lint gates before broad extraction. Track test
counts and scope, not just a green command. Do not claim native rendering, real
input, platform execution or latency results from static analysis alone.

Preserve the dated audit as historical evidence. Update ARCHITECTURE.md when
current ownership/contracts change, and update this table as milestones are
implemented and verified. Keep each improvement in a separate commit.
