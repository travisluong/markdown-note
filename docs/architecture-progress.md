# Architecture implementation progress

The full objective is the staged plan in [the audit](architecture-audit.md):
complete the architectural improvements, improve performance, and preserve
behavior, prioritizing Windows. Completion requires current implementation and
verification evidence for every milestone; passing a subset is not completion.
Native desktop tests remain user-run under AGENTS.md.

All eight implementation milestones are complete. Final source gates pass on
Windows and Linux: **301 tests per platform**, no failures or ignored tests,
default builds, strict all-target diagnostics Clippy, and repository formatting.
macOS validation is configured in CI but has not been executed in this workspace.
These checks do not claim physical-input or native frame-latency results.

| Milestone | Status | Evidence / remaining work |
| --- | --- | --- |
| 1. Editor redo policy | Implemented and verified | `tests/editor_history.rs`: 11 headless tests passed, including divergent typing/commands/IME, no-op replacements and generic History compatibility; complete Windows/Linux suites passed. |
| 2. Reliable baseline | Implemented; Windows/Linux verified | Rust 1.98.1 pinned; default builds, headless suites, format and strict lint pass. Three-platform validation CI is configured; macOS execution remains unverified. Release packaging is unchanged. |
| 3. Lifecycle/edit fixes and integration coverage | Implemented and verified | F2 picker/error/tree-retarget cancellation, F3 revalidation, F4 inline image insertion and F5 discarded overlays repaired. F6 foreground ownership and table/list/save/navigation headless coverage passed. A separately reproduced close-time input loss was repaired; all 91 application tests passed afterward. |
| 4. Data/storage boundaries and file policy | Implemented and verified on Windows/Linux | Library exposes format/I/O, notebook domain owners and shared values. Disk versions guard saves; conflict pause retains editor/history; app-owned moves revalidate saved bytes. Metadata and optimistic race limits are explicit in `document-storage.md`. Both full headless suites passed. |
| 5. Document-session transition owner | Implemented and verified on Windows/Linux | Private Session owns content, foreground phases, writes, continuations and autosave identities. Twenty-nine pure tests plus actual delayed/failure/cancel/move/close cases pass. See the session contract and final integrated gates below. |
| 6. Bounded settings writer | Implemented and verified on Windows/Linux | Pure one-running/latest-per-destination model and one GPUI worker loop replace the task chain. Ten model/disk tests and six actual lifecycle tests pass, including 20,000 submissions, target-book flush, unrelated failures, close retry/cancel/explicit discard and late preference changes. |
| 7. UI and command ownership | Implemented and verified on Windows/Linux | SettingsPanel, SearchPanel, Explorer and PaletteModel own private feature state. Explicit catalogue/capability policies and typed preference/navigation intents connect them to the application. Existing tests were retained, with actual adapter, stale-result and dialog-lifetime regressions added. |
| 8. Editor boundary and analysis performance | Implemented and verified on Windows/Linux | Production features use a thin Editor adapter; four actual adapter tests cover transactions, events and guarded selection. Vendor inventory is complete. Shared GFM analysis uses one parse for mixed queued work and zero for plain requests; initial Reading parsing runs off layout with bounded pending work. |

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
Windows and Linux as part of their full 240-test suites. The separate same-book
reload preference race is repaired below.

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

## Preferences changed during notebook reload

A deterministic after-read delay reproduced two overwrite failures: a captured
Source setting replaced a newer Reading choice, and an external Reading setting
replaced an explicit re-selection of Source. Activation now retains typed mode,
normalized sidebar-width and notebook-autosave intents keyed by operation and
canonical root. Only explicit fields replay into the same notebook. Other loaded
preferences remain intact; changes on A cannot replace B, and autosave for a
standalone document stays in session scope. Four real TestPlatform regressions
and two pure owner tests passed on Windows, followed by all 19 notebook lifecycle
tests. The regression hook delays installation after real filesystem reads.

## Command policy boundary

The library now exposes `commands::Command`, `PaletteEntry`, `PaletteModel` and
recursive notebook discovery behind an explicit facade. Command labels and
availability, discovery, and palette ranking/selection have separate modules.
`capabilities::Capabilities` uses named state fields and semantic edit/save/search
queries. Nine existing command/ranking/discovery tests were retained in the new
owner, with a new filter/replacement/selection regression; all ten passed on
Linux, including a complete 56-test library run with existing storage and newly
prepared owners. The live palette adapter is integrated in a following change.

## Settings form ownership

`SettingsPanel` owns its form controls, validation, selected section and internal
subscriptions. Named preference intents cross into the application adapter,
which retains notebook/session scope and theme application. The dialog retains a
lifetime token; callbacks after replacement, ordinary close or programmatic
closure cannot apply preferences to another form or notebook. Thirteen focused
Windows tests passed in an isolated pre-Session checkout, including invalid and
valid values, sections, theme/font controls, scope, stale events and both close
paths. Strict diagnostics Clippy also passed for that isolated integration.

## Explorer state boundary

The pure `Explorer` owner keeps directory cache state, projected rows, selection
and generation checks private. It accepts notebook settings and captured scan
results, and exposes rows and missing-directory requests to the UI adapter. Four
Linux owner tests pass. A new collapse/refresh/re-expand regression failed before
invalidating obsolete Loading entries at refresh start; current workers and
completed caches remain valid. Live explorer integration follows separately.

## Completed feature and editor integrations

The Session integration removes the old foreground gate and save-policy flags.
Its isolated Linux application suite passed 114 tests, including the retained old
palette tests before that later extraction; strict lint also passed. Independent
review found and repaired a nested error-effect autosave rearm and unnecessary
save prompting when removing an inactive notebook. Regressions verify eventual
retained-text saving, stale-timer rejection and unchanged active-document policy.

The Search owner passed 11 actual panel/lifecycle tests and strict lint in an
isolated Windows checkout before Session integration. Its FIFO navigation
proposals preserve consecutive key actions while rejecting obsolete queries or
operations. The current overlay updates on every accepted content installation.
SettingsPanel separately passed 13 tests and strict lint, including programmatic
closure rejecting queued preference events.

Explorer keeps cache, row and cursor state behind one persistent generation
owner. Its isolated Linux integration passed five pure owner tests, all 21
notebook adapter tests and strict lint. The former extension-row regression moved
with its policy without losing assertions. Actual directory completion also
checks that an unresolved folder target cannot erase another visible selection.
Palette policy retains ten library tests; two new TestPlatform cases verify
query filtering, fresh command availability, date-command Undo/Redo and rejection
of a stale file scan after opening another palette. Those adapter tests passed
independently on Linux and in the integrated Windows application suite.

The Editor adapter wraps one InputState and exposes cheap rope snapshots,
source-byte selection, isolated edits, task/link events, find and image insertion.
It adds no saved/current buffer or separate view-mode state. Existing native
benchmark inputs keep their original configuration through a feature-gated
constructor; the harnesses were compiled but not launched. Four actual adapter
tests include rejected disabled edits preserving selection and history. The
vendor patch inventory documents the new read-only disabled-state accessor.

Isolated validation uses immutable source snapshots and application-package
cache cleanup before returning to the live checkout; the final integration also
refreshes the vendored component artifacts. Windows occasionally required the
previously authorized rebuild after generated-output LNK1104 errors. These were
build-environment errors; no compiler/linker error was counted as a passing run.

## Final integrated gates

The completed source at `e921c4f` passed the following checks. The subsequent
architecture-guide commit changes documentation only.

| Platform | Default build | Diagnostics-enabled tests | Strict all-target diagnostics Clippy | Formatting |
| --- | --- | --- | --- | --- |
| Windows/MSVC, Rust 1.98.1 | Passed | **301 passed**: 62 library + 109 application + 130 integration | Passed with `-D warnings` | Passed |
| Ubuntu 26.04 WSL x86_64, Rust 1.98.1 | Passed | **301 passed**: 62 library + 107 application + 132 integration | Passed with `-D warnings` | Same checked source |
| macOS | Not run locally | Not run locally | Not run locally | CI configured |

Every executed suite had zero failures and zero ignored tests. Platform-specific
file identity, sharing, junction, permission and path tests account for the
Windows/Linux target-count differences. The dependency's existing
`proc-macro-error2 2.0.1` future-compatibility notice remains; strict project lint
passes without introducing warning suppressions.

Final local logs are retained in ignored `target/`: `editor-integration-tests.log`,
`editor-integration-clippy.log`, `architecture-final-check.log`,
`architecture-final-format.log`, `linux-architecture-final-test.log`,
`linux-architecture-final-check.log` and `linux-architecture-final-clippy.log`.
Independent intermediate snapshot manifests and logs are retained there too.

Performance evidence is structural and test-observed: bounded preference writes,
one GFM parse for mixed queued analysis, plain-text fast paths, bounded Reading
work including initial parsing, and no new per-keystroke source materialization
in the session/editor boundaries. Native rendering latency and physical input
remain manual validation under AGENTS.md. Automated regression checks passed;
they cannot establish the absence of every possible regression.

The current extension map is [ARCHITECTURE.md](../ARCHITECTURE.md), supplemented
by the [session contract](document-session.md), [storage contract](document-storage.md)
and vendored patch inventory. Improvements are committed separately; no source
was pushed and no native application or desktop automation was launched.
