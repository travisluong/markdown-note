# Architecture implementation progress

Historical verification records describe development snapshots and do not replace
current verification of the source.

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

## Windows installer — 2026-09-12

Added `packaging/windows/mithril.iss` for per-user Windows installation, Start
menu and optional desktop shortcuts, and Installed apps registration/uninstall.
The stable AppId lets newer installers reuse the existing installation. Installer
ownership excludes notebook files and application preferences. Automatic editor
shutdown/restart is disabled so users can save and close before an upgrade.

The release workflow compiles the installer alongside the portable ZIP. A
separate job with `contents: write` attaches all platform downloads to published
GitHub Releases after successful builds; manual runs retain Actions artifacts
only. No release was created or uploaded during local verification.

Validation on Windows/MSVC with the pinned Rust 1.98.1:

- `cargo build --release --locked --offline --bin mithril`: passed.
- Inno Setup 6.7.3 compiled the actual script with the freshly built executable:
  passed. Local output is
  `target/installer/mithril-0.1.0-windows-x86_64-setup.exe` (9,275,958 bytes).
- Installer PE product version and publisher match the package/configuration.
- actionlint 1.7.12 on `.github/workflows/build.yml`: passed.
- PowerShell AST parsing of the installer workflow step: passed.
- `git diff --check`: passed.

The existing `proc-macro-error2` future-compatibility notice remains. No Rust
source changed, so the application test suites were not repeated for packaging.
The compiler was prepared in silent portable mode under ignored `target/`;
neither Mithril nor its installer was launched. Native install, upgrade,
file-in-use, uninstall and shortcut behavior remain manual verification in a
clean Windows account or VM, as listed in `packaging/windows/README.md`.
GitHub Actions execution/upload was not tested locally. App and installer remain
unsigned; automatic updates and custom Windows icon branding are not included.

## Mithril rename — 2026-09-12

Renamed the Rust package/library/executable, app titles and Help, diagnostic
variables, notebook metadata and user configuration paths, macOS bundle,
Windows installer script and shortcuts, release artifacts, repository links,
and documentation to Mithril. The Windows installer AppId remains stable.
Earlier entries above use the current names; the checks below verify this rename.

Preferences now live in `.mithril/settings.json` and the platform configuration
folder's `mithril/notebooks.json`. Existing installations must rename their
previous settings directories before opening notebooks; README documents this
step. On this workstation, the two registered notebooks and user configuration
were renamed with SHA-256 verification that every existing settings file stayed
unchanged. No note contents were changed.

Verification on Windows/MSVC with pinned Rust 1.98.1:

- `cargo check --locked --offline`: passed.
- `cargo test --locked --offline --features editor-diagnostics`: 301 passed.
- `cargo fmt --check`: passed.
- `cargo clippy --locked --offline --all-targets --features editor-diagnostics -- -D warnings`: passed.
- `cargo build --release --locked --offline --bin mithril`: passed.
- Inno Setup 6.7.3 compiled `packaging/windows/mithril.iss` with the renamed
  release executable into `target/installer/mithril-0.1.0-windows-x86_64-setup.exe`.
- actionlint on `.github/workflows/build.yml`, macOS plist XML parsing, and
  `git diff --check`: passed.
- Tracked source/content inspection found no previous project-name references.

The existing `proc-macro-error2` future-compatibility notice remains. No desktop
application or installer was launched. Native installation/upgrade behavior and
other platform builds remain manual/CI validation. The GitHub repository was
renamed to `travisluong/mithril`, its repository identity was verified, and the
local origin was updated. Source changes were not pushed.

## Icon-free Windows title bar (2026-09-12)

Windows startup now requests a transparent native title bar and Notes renders
`gpui_component::TitleBar` with only the current title and window controls.
One title helper supplies the visible text and native title, preserving file
names and the unsaved marker. Fullscreen omits the client title bar.

Windows/MSVC verification: `cargo check --offline --locked --bin mithril`,
`cargo test --offline --locked --features editor-diagnostics` (301 passed),
`cargo fmt --check`, strict all-target clippy with editor diagnostics, and
`cargo build --offline --locked --bin mithril` passed. No desktop app was
launched. Native appearance, dragging, minimize/maximize and close behavior
remain manual validation.

## Marketing logo across application icons (2026-09-12)

Checked-in SVG, seven-size ICO, 512px PNG and Retina ICNS assets now derive from
the marketing site's exact silver M mark. `scripts/build-icons.mjs` documents
regeneration through the marketing repository's sharp dependency. Normal builds
need only the generated files. `build.rs` embeds resource 1 into the Windows
binary (the ID GPUI loads), and Inno Setup uses that same ICO. macOS packaging
copies the ICNS into Resources and declares it in Info.plist. Linux archives
include a PNG and desktop entry matching the window's application ID.

Verification on Windows/MSVC:

- Default Cargo check/build, formatting and strict all-target diagnostics lint
  passed; the combined changes passed all 301 headless Rust tests.
- A non-executing PE data-resource inspection compared all seven embedded icon
  images byte-for-byte against the ICO. Pillow decoded every ICO size and the
  macOS ICNS; the plist binding and exact marketing SVG copies were checked.
- Inno Setup 6.7.3 compiled the branded installer using the checked debug binary
  into `target/installer-icon-check/`; no installer or desktop app was launched.
- actionlint validated the release workflow; `git diff --check` passed.
- In the marketing repository, the favicon SVG now copies the source mark and
  its ICO is regenerated from it. Static build and all existing headless website
  checks passed; favicon entries decode at 16/32/48px.

Native shell/taskbar/title-bar appearance and macOS/Linux builds remain manual
or CI validation. The pre-existing proc-macro-error2 compatibility notice remains.

## Restore native Windows window controls (2026-09-12)

The user reported inability to close the app and hangs after the two most recent
commits. Inspection identified `c2ab535` as the behavioral change: removing the
icon also hid the native title bar and installed `gpui_component::TitleBar`.
Its Windows controls rely on GPUI non-client hit testing and mouse messages,
which the existing TestPlatform close tests do not exercise. The later branding
commit adds resources and an app ID without changing session shutdown.

Rolled back the custom title bar and explicitly disabled transparent title-bar
mode, restoring native controls and dragging. The native small icon returns;
the branded executable resources and title/unsaved-marker helper remain. The
save/discard/cancel and preference-flush barriers are unchanged. This is a
rollback of the suspected regression, not a reproduced diagnosis of the exact
native event failure. Native close, dragging and hang behavior still require
manual validation; no desktop application or native harness was launched.

Windows/MSVC validation passed: `cargo check --offline --locked`,
`cargo test --offline --locked --features editor-diagnostics` (complete suite,
including session close and all four GPUI preference-close tests),
`cargo fmt --check`,
`cargo clippy --offline --locked --all-targets --features editor-diagnostics -- -D warnings`,
and `cargo build --offline --locked --bin mithril`.

## Link ordinary files dropped into notes (2026-09-12)

Both sidebar and external file drops now use a shared file-drop handler.
Ordinary files are linked at their existing locations, including nested notes,
while supported images retain their import/embed behavior. Mixed batches retain
input order and image rollback until the captured document accepts insertion.
Link labels escape literal Markdown and destinations encode ampersands to avoid
Markdown character-reference decoding changing the target filename.

Windows/MSVC validation passed: `cargo check --offline --locked`,
`cargo test --offline --locked --lib --bins --features editor-diagnostics`
(62 library and 111 application tests), `cargo fmt --check`, strict all-target
diagnostics Clippy, and `git diff --check`. Regression coverage exercises mixed
file/image drops, nested relative links, Unicode and special filenames, and
rollback preserving linked files. The full integration command could not finish:
Windows denied replacement of `target/debug/mithril.exe`; a Mithril process was
running and was left untouched. No desktop validation was performed.

## Publish Windows downloads in a public release repository (2026-09-12)

The build workflow now accepts stable `vMAJOR.MINOR.PATCH` tag pushes, requires
the Cargo package version to match, and runs Windows headless tests and release
packaging using the pinned Rust toolchain. Manual runs retain Actions artifacts
without publishing. Cross-platform validation remains in the separate check
workflow. The Windows installer updates link and main download documentation
now point to `travisluong/mithril-releases`.

The release helper owns tag/version validation, versioned filenames, SHA-256
checksums, and draft creation/recovery. The publisher's separate credential is
exposed only to its publishing step; all built-in job tokens remain read-only.
It verifies the three uploaded assets by downloading and comparing them before
publishing. Published releases are never overwritten, mismatched drafts fail
for inspection, and runs for a source ref are serialized. The destination tag
targets its own `main`; source identity is recorded in release notes without
copying source or binaries into public Git history.

Local verification passed:

- `node --test .github/scripts/release.test.mjs`: 10 tests, using mocked GitHub
  CLI calls for creation, interrupted upload recovery, existing publication,
  corrupt/unexpected assets, credential/network failure, and source mismatch.
- `actionlint .github/workflows/build.yml` and `git diff --check`.
- Actual eligibility and Cargo metadata CLI commands produced `build=true`
  and `version=0.1.0` for `v0.1.0`.
- Inno Setup 6.7.3 compiled the updated script using the existing local release
  executable into `target/public-release-check/dist`. The portable ZIP contains
  only `mithril.exe`; checksum generation and verification passed. This checks
  packaging against an existing binary, not a fresh CI release build.
- `cargo test --offline --locked --features editor-diagnostics`: full headless
  suite passed. The existing proc-macro-error2 future-compatibility warning remains.

No application or installer was launched. Live GitHub publication and manual
workflow dispatch were not performed: no GitHub CLI, connector or token is
available in this session, and public API lookup was blocked by local network
restrictions. The user must provision/confirm the public repository and set
`MITHRIL_RELEASES_TOKEN` using the packaging guide before pushing a release tag.
Native Windows installation checks remain manual. The pre-existing Cargo.lock
changes are outside this work and remain uncommitted.

## Fix the first release's Windows file-drop test fixture (2026-09-12)

The first release pipeline reported a failure in
`mixed_drops_link_files_in_place_and_roll_back_only_imported_images`. The test
passed locally with the normal TEMP path, but failed deterministically when
TEMP/TMP used uppercase directory names: imported files had canonical paths,
while the nonexistent fixture note retained the temporary directory's raw
spelling. Relative URLs consequently traversed back through the differently
spelled ancestors instead of matching the expected short links. Windows 8.3
temporary-directory aliases can cause the same mismatch.

The fixture now creates the note and opens it through `document::open`, just
as production does, before generating links. No production path or file-drop
behavior changed, and all link, mixed-order and rollback assertions remain.

Verification passed: all nine image-import tests with uppercase TEMP/TMP,
`cargo test --offline --locked --features editor-diagnostics` (full suite,
including all 111 application tests), `cargo fmt --check`, and
`git diff --check`. The linker initially could not replace cached test
executables inside the sandbox; the headless checks succeeded with elevated
tool permissions. No application or installer was launched. The existing
proc-macro-error2 future-compatibility warning remains.

## Settings field layout (2026-09-12)
Settings controls use explicit available widths and nonshrinking text fields. Below 560 px, navigation stacks above the form. Placeholders document delay, extensions and date syntax. Headless settings suite: 14 passed, including 800/480 px layout with invalid input. Native UI validation remains user-run.

## Logical line movement (2026-09-12)
Added document-only Ctrl+Up/Down (Command on macOS), palette commands and Help entries through Editor. Pure tests cover UTF-8, selected blocks, CRLF and EOF; TestPlatform covers Source/Live, undo and redo.

## Inline code styling (2026-09-12)
Source-preserving inline-code spans now share the existing GFM worker and exact-revision acceptance. Both rendered modes use the fenced-code muted background. Three focused tests passed for Unicode, multi-backticks, multiline content, literal markers, active-line reveal and stale parsing.

## Live-preview list dragging (2026-09-12)
Implemented cross-list subtree moves with indentation/bullet adaptation, preserved task state, guarded source transactions, insertion preview and edge scrolling. Pure tests cover subtrees/nesting/EOF/blockquote rejection; three TestPlatform transaction tests pass including real marker geometry, drag-versus-click, undo/redo, reversed Unicode selection, Escape and stale edits. Native pointer validation remains user-run.

List-drag final review: four pure reorder tests passed, including lazy continuation paragraphs moved into nested lists and CRLF files without a final newline. Accepted edits also cancel pending drag feedback immediately. Strict all-target Clippy passes.

## Sidebar context menu and final editor verification (2026-09-12)

Files expose Open, Rename, Duplicate, Copy relative path, system Reveal and Trash.
Folders expose New note/folder, Rename, path copy, Reveal and Trash; background
space exposes creation and Refresh. Right-click selects without navigating;
Shift+F10 opens the selected row's menu. Targets retain notebook root and explorer
generation, and filesystem actions revalidate relative paths and links.

New notes target the selected directory. Duplicating the active note copies its
unsaved snapshot with retained encoding/newline conventions. Rename reuses move
baseline/history/settings remapping. Session-owned EntryOperation keeps the
editor until I/O succeeds; active-content deletion uses Save/Discard/Cancel,
while unrelated operations retain drafts across write barriers. Successful Trash
clears affected content and persisted paths; failure retains them. Trash 5.2.8 is
isolated behind the storage adapter; tests use temporary-file backends, never OS
Trash or system Reveal.

Verification on Windows, without desktop interaction:

- `cargo check --locked --offline`: passed.
- `cargo test --locked --offline --features editor-diagnostics`: 323 passed, none
  failed or ignored, including TestPlatform right-click/keyboard context menus.
- Focused coverage includes collision-safe storage, unsaved duplication, nested
  note creation, stale targets, parent rename/history, failed/successful Trash,
  write barriers and retained drafts; list gestures, source mappings and layout
  checks are recorded in the sections above.
- `cargo fmt --check` and strict all-target diagnostic Clippy: passed.
- The pre-existing Cargo.lock package-order change remains uncommitted; only the
  new Trash dependency changes are included in this feature.

Native visual/pointer checks and system Recycle Bin/Finder/Explorer behavior
remain user-run under AGENTS.md. No desktop apps or native harnesses were launched.

Final boundary regression: moving upward from EOF after a trailing LF/CRLF is a no-op, preserving an empty caret selection and the final-newline invariant. Focused line-movement and editor transaction tests pass.

## Stable code-block presentation (2026-09-12)

Live preview now reserves fence rows, including wrapped info strings, and hides
only their glyphs outside the active block. Code font, padding, background and
source mappings persist during selection changes. Nested fence prefixes use the
same inset as their content; empty blocks retain real source positions.

Headless verification: all 3 code-highlighting tests and all 75 editor module
tests pass, including source/history, cursor and selection positions, themes,
Unicode/CRLF, empty/unfinished/nested blocks and viewport work bounds. Tests use
`--target-dir target/agent-validation` because the default output's mithril.exe
is locked. No running application or desktop state was changed.
## Windows Reveal repair (2026-09-12)

Sidebar Reveal now launches Explorer through an app-owned background adapter.
The adapter preserves Unicode and native path strings and converts canonical
verbatim drive/UNC paths for the shell. Launch errors appear in the notebook
notice, with stale completion guards. The headless tests cover arguments,
recorded dispatch, stale/missing targets and injected failures; no Explorer
window is launched by tests. Native Explorer confirmation remains user-run.
## Settings field width and integrated verification (2026-09-12)

The actual autosave input measured 26 px inside a 454 px content column before
this fix. The content column now propagates width through flex layout, and font
selectors retain their full width through their component containers. Navigation
stays on the left at normal widths and stacks above the content on narrow windows.
Debug-only control selectors let tests measure the inputs themselves. All 15
settings tests pass, including field bounds at 800, 560 and 480 px.

All five requested improvements are implemented: file hover tooltips removed,
code geometry/styling retained during selection, Windows Reveal repaired,
Untitled numbering starts at 1 after the unnumbered note, and settings fields
fill their content area. Existing files and the user's Cargo.lock change remain
untouched.

Final noninteractive verification on Windows:

- `cargo check --locked --offline`: passed.
- `cargo test --locked --offline --target-dir target/agent-validation --features editor-diagnostics`: 327 passed, zero failed or ignored.
- `cargo fmt --check`: passed.
- `cargo clippy --locked --offline --all-targets --features editor-diagnostics -- -D warnings`: passed.

The separate test target avoids a locked default-output mithril.exe. The only
reported toolchain notice is the existing dependency future-compatibility notice
for proc-macro-error2. No native application, Explorer window, or desktop input
was used. Native visual and Explorer confirmation remain user-run.
## Live checkbox border alignment (2026-09-12)

Checkbox squares and strokes now align to physical pixels after layout/scroll
translation. Logical layout and interaction bounds remain unchanged. The new
geometry test covers fractional positions, 100/125/150/200% scale, and interface
font sizes 10 through 24. The focused editor and editor_transactions suites pass:
79 tests, zero failures. Native visual confirmation remains user-run.

## Theme border color (2026-09-12)

Settings > Theme now includes Border color, independently persisted for Light
and Dark through the existing appearance preference owner. Missing/invalid
border values resolve to the containing mode's default (#E5E5E5 / #262626).
The setting updates shared theme borders and the current sidebar entry outline.
The picker adapter rounds HSL-to-RGB channel conversion so selected RGB bytes
are not truncated by floating-point noise.

Headless tests cover old/invalid settings, JSON round trips, independent palettes,
reset, actual picker events and acknowledgement without feedback, session versus
notebook scope, stale dialogs/notebooks, theme propagation, and disk persistence
through SettingsWriter. Checkbox paint geometry and existing editor interactions
also pass. Final noninteractive Windows verification:

- cargo check --locked --offline: passed.
- cargo test --locked --offline --features editor-diagnostics: passed, no failures.
- cargo fmt --check: passed.
- cargo clippy --locked --offline --all-targets --features editor-diagnostics -- -D warnings: passed.

An initial alternate-target attempt hit a linker output-file error; the full
suite passed using the default target. The existing proc-macro-error2 dependency
future-compatibility notice remains. No desktop apps were launched or controlled;
native visual confirmation remains user-run. The user's Cargo.lock changes were
left untouched and excluded from both commits.

## List gestures, automatic numbering and table borders (2026-09-12)

Live preview tables now paint each shared border once with physical pixel alignment.
Bullet and numeric marker hover shares drag geometry and uses the standard arrow.
Checkbox/list drops replace only the changed UTF-8 span and anchor stationary
visible source content through selection remapping and background layout updates.
Edge scrolling updates hit geometry and stops on release or cancellation.

Source and Live Enter insert and renumber numeric siblings in one undo step.
Numeric dragging moves full subtrees between numeric lists, retaining each list's
original start and adopting its destination delimiter. Shared source transforms
preserve task state, Unicode selections, CRLF/final-newline state and continuation
indentation across digit-width changes. Literal fences and unrelated lists remain
outside numbering transactions. Revision, quote, descendant, disabled and IME
guards remain in force.

Noninteractive Windows verification:

- Full locked offline diagnostics suite: 344 tests passed, zero failures.
- Locked offline cargo check and strict all-targets diagnostics Clippy: passed.
- cargo fmt --check: passed.
- Focused tests cover table border geometry at fractional scales, numeric-only
  analysis, Source/Live Enter undo/redo, nested/cross-list/quoted moves, both numeric
  delimiters, UTF-8/CRLF, rendered marker widths and scrolled drops with an offscreen
  caret, including dragging the first visible item and cancelling edge scrolling.

Windows intermittently denied the linker access to existing test executables;
retrying the same commands succeeded. The existing proc-macro-error2 future
compatibility notice remains. No native application or desktop input was used;
physical pointer and visual confirmation remain user-run. The pre-existing
Cargo.lock changes remain untouched and are excluded from the commits.

## Ordered keyboard movement (2026-09-13)

Ctrl+Up/Down now normalizes affected ordered sibling lists in the same source
transaction, retaining their starting values and mapped selections. Focused
move_line_tests passed (3 tests); the Source/Live TestPlatform history regression
passed. Coverage includes both directions, selected siblings, nested/quoted lists,
Unicode, CRLF, digit widths and literal fences. The pre-existing Cargo.lock edit
is excluded. Headless tests use target/headless with shared dependency caches
because Windows denies replacement of target/debug/mithril.exe; no app was
launched, stopped or controlled.

## Live table continuity during edits (2026-09-13)

TableIndex retains unchanged source projections during pending context analysis.
Unrelated edits rebase offsets; list replacements crossing tables locate unchanged
rope slices within the edited rows. Edited structures retire individually, and
exact background results remain authoritative for Markdown context. No document
GFM parse or source copy was introduced on the input thread.

New wrapper regressions hold analysis pending across typing, punctuation, Enter,
keyboard moves and checkbox drags across two tables in both directions. They
verify rendered rows, hidden separators, source/cell ranges, column widths and
horizontal scroll under LF/CRLF. Delayed results cannot restore stale syntax;
an exact parse removes tables genuinely converted into fenced code.

The full locked offline editor-diagnostics suite passed, including existing
large-table work bounds and InputState drag/history transactions. Locked offline
cargo check, cargo fmt --check, and strict all-targets diagnostics Clippy passed.
Commands used --target-dir target/headless to avoid the locked normal executable.
The existing proc-macro-error2 future-compatibility notice remains. No desktop
interaction or native application testing was performed. Cargo.lock was left
unchanged by this task and excluded from both commits.

### Link rendering during typing (2026-09-13)

Unchanged links keep their mapped presentation during pending context analysis.
Queries spanning edits retain unaffected links, and long bursts compact offsets
instead of clearing the index. Exact analysis applies changed Markdown context.
Validation: `cargo test --locked --offline --test editor link -- --nocapture`
passed all 13 matching headless tests, including new delayed-analysis and burst
regressions. No native application or desktop interaction was used.

### Individual link reveal and direct click (2026-09-13)

Live-preview link projection uses the caret's UTF-8 source offset independently
of line/cell syntax reveal. Entering or leaving a link invalidates cached line
and cell presentation, including the shared boundary of adjacent links. Links
retain their label underline; a direct click emits OpenLink without changing
source or selection. Hover wording and README now describe direct clicks.

Verification: `cargo check --locked --offline`, `cargo fmt --check`, and strict
`cargo clippy --locked --offline --all-targets --features editor-diagnostics -- -D warnings`
passed. `cargo test --locked --offline --features editor-diagnostics` passed all
354 tests. After the adjacent-link boundary adjustment, the focused editor and
editor_transactions link suites passed all 16 checks and strict lint passed again.
Coverage includes Unicode mappings, active-line/cell reveal, source boundaries,
unchanged-link retention, stale layout rejection, and the emitted OpenLink URL.
Two intermediate Windows linker attempts could not reopen test executables;
subsequent builds and tests succeeded. Validation used only headless TestPlatform;
native visual validation remains manual.

### Arrow cursor over links (2026-09-13)

Live-preview Input hover uses the existing current-source link hit to select the
standard Arrow cursor and destination tooltip; other editor text uses IBeam.
Reading inline links also use Arrow. No source, selection, or click semantics
changed. `cargo check --locked --offline`, `cargo fmt --check`, and strict
all-target editor-diagnostics clippy passed. The existing direct-link-click test
could not rebuild because Windows denied removal of target/debug/mithril.exe;
a targeted cargo-rustc attempt encountered the same file lock. No application
was closed or launched and no desktop interaction was used for validation.

### Top Search menu (2026-09-13)

The top bar now includes Search between View and Notebooks, with Search in file,
Search in all files, and Search and replace in file. Menu entries reuse the
existing file/Reading find, notebook search, and editor replace handlers, with
availability supplied by the command catalogue. Alt+S opens the menu and appears
in Keyboard shortcuts through the shared shortcut definitions.

Verification: `cargo fmt --check`, `cargo check --locked --offline`,
`cargo test --locked --offline --features editor-diagnostics` (354 tests), and
`cargo clippy --locked --offline --all-targets --features editor-diagnostics -- -D warnings`
passed. Validation was headless; native menu appearance remains manual.

### Top Sort menu (2026-09-13)

The top bar includes Sort between Search and Notebooks. It shares the sidebar
sort button's menu builder, exposing all six name/modified/created ordering
options with the active notebook's selection checked. Both menus use the existing
sort and persistence handler; entries are disabled while busy or without a notebook.

Verification: `cargo fmt --check`, `cargo check --locked --offline`,
`cargo test --locked --offline --features editor-diagnostics`, and
`cargo clippy --locked --offline --all-targets --features editor-diagnostics -- -D warnings`
passed. Validation was headless; native menu appearance remains manual.

### Keyboard shortcut dialog height (2026-09-13)

Help dialogs reserve a fixed top margin, the component's 30px slide animation,
and a bottom margin when limiting their height. The shortcut list uses the
dialog's built-in scrolling content, with Close in the fixed footer. A headless
GPUI regression verifies that Close stays inside the window while resizing
between 900px, 600px, 320px, and 1200px heights, including a narrow window.

Verification: `cargo check --locked --offline`,
`cargo test --locked --offline --features editor-diagnostics`, `cargo fmt --check`,
and `cargo clippy --locked --offline --all-targets --features editor-diagnostics -- -D warnings`
passed. Native appearance remains manual; no desktop interaction was used.

### View menu sidebar toggle (2026-09-13)

View now includes Collapse sidebar after the view modes, checked from the same
session state used by the sidebar icon. It calls the existing sidebar toggle
after menu dismissal so editor, Explorer, and Search focus behavior is preserved.
The item is disabled while busy or without an active notebook.

Verification: `cargo fmt --check`, `cargo check --locked --offline`,
`cargo test --locked --offline --features editor-diagnostics`, and
`cargo clippy --locked --offline --all-targets --features editor-diagnostics -- -D warnings`
passed. Static review confirmed shared state and the existing toggle guards;
native menu appearance and interaction remain manual. No desktop interaction
was used.

### Notebook search focus and visibility (2026-09-13)

Clicks, Enter and arrow navigation retain notebook-query focus while selecting
and revealing the match. Cancelled/failed navigation restores the previous
result; successful clicks preserve result-list scroll. Reading initializes find
without focusing its query. The editor resolves a pending source range against
current wrapping and preview geometry, including code and table viewports.

Verification: all 367 tests passed with
`cargo test --locked --offline --features editor-diagnostics`.
`cargo check --locked --offline`, `cargo fmt --check`, and
`cargo clippy --locked --offline --all-targets --features editor-diagnostics -- -D warnings`
also passed. New TestPlatform regressions exercise real row clicks followed by
arrow/Enter actions in Source, Live and Reading, cancellation/load failure,
retained list scroll, delayed Reading parsing, initial editor layout, distant
Unicode, wrapped/horizontal matches, code, tables, oversized selections,
superseding/invalid ranges, and undo/redo preservation. No desktop interaction
or native app harness was used; native appearance remains manual validation.
### Search result snippets without line numbers (2026-09-13)

Result rows display the existing before/match/after snippet without the numeric
line prefix. Internal line metadata remains available for revalidation.
Verification: the 16 search UI tests, `cargo check --locked --offline`,
`cargo fmt --check`, and strict Clippy with all targets and editor diagnostics
passed after this presentation change. The full 367-test suite passed for the
preceding focus/reveal change. Native appearance remains manual validation.
### Full-width window toolbar (2026-09-13)

The root layout now renders a dedicated toolbar below the text menus and above
notices and the resizable sidebar/document area. Its fixed buttons toggle the
sidebar, show Explorer, open notebook Search, and sort files. The toolbar stays
visible without a notebook or with the sidebar collapsed; buttons are disabled
without a notebook or while busy. Explorer/Search highlight only a visible panel,
and the toggle icon and tooltip reflect whether the sidebar is visible.

Toolbar presentation lives in `toolbar_ui.rs`; existing sidebar/search handlers
retain focus, panel restoration, width, sorting, and preference ownership. The
old sidebar icon row is removed. Explorer retains notebook-root drops, while the
window toolbar has no drop handler. Customization and preference changes are
deferred.

Verification: `cargo check --locked --offline`, `cargo fmt --check`, and
`cargo clippy --locked --offline --all-targets --features editor-diagnostics -- -D warnings`
passed. `cargo test --locked --offline --features editor-diagnostics --lib --bins`
passed all 199 tests (67 library and 132 application), including notebook width
persistence and search navigation/focus/state regressions. Static review verified
shared toggle/panel/sort dispatch and Explorer's retained root-drop handler.
The full `cargo test --locked --offline --features editor-diagnostics` command
was attempted but could not remove `target/debug/mithril.exe` (Windows access
denied); the integration suite could not complete through that command. No
application process was stopped and no desktop or native harness was used.
Native layout, themes, narrow windows, toolbar interaction, and Explorer drops
remain manual validation. The pre-existing Cargo.lock change is excluded.

### Date input rendering correction (2026-09-14)

A headless SettingsPanel test reproduced a populated date input whose text
paint bounds were 0 px wide while the field itself was 454 px wide. Explicit
single-line flex layout and text-leaf growth restore usable inner geometry.
The example now reads the current raw draft and disappears for invalid input;
only valid drafts emit date preference intents.

Verification: all 18 settings_ui tests passed with --locked --offline and
editor-diagnostics, including actual TestPlatform typing, empty/invalid input,
correction, section changes, Light/Dark themes, 800/480 px windows and long
horizontal scrolling. Tests inspect shaped text and inner geometry. No desktop
interaction or native application harness was used. Full gates follow the
reference addition. Existing Cargo.lock changes remain excluded.

### Date format quick reference and integrated verification (2026-09-14)

Settings Templates now includes six read-only common formats with fixed sample
output and a compact token guide covering years, numeric/named months, days,
weekdays, 12/24-hour time, minutes, seconds and AM/PM. The reference calls out
case sensitivity, MM versus mm, English/local-time behavior and bracketed
literal text. It wraps inside the existing scrollable settings dialog.
Reference data and presentation are private to SettingsPanel; format syntax,
preference schema and ownership are unchanged.

All 19 settings tests passed, including formatter checks for every reference
example and token row, and reference containment at narrow widths. The complete
cargo test --locked --offline --features editor-diagnostics suite passed.
cargo check --locked --offline, cargo fmt --check, and strict all-target clippy
with editor-diagnostics also passed. Earlier transient Windows linker failures
on test executables cleared on retry; no processes were stopped. Verification
used TestPlatform and noninteractive commands only; native visual confirmation
remains for the user. Cargo.lock remains excluded from both task commits.

### Date reference row overlap correction (2026-09-14)

The quick reference now uses two equal grid columns with top-aligned cells
and nonshrinking rows/groups. Tokens and descriptions wrap inside their own
columns instead of moving between flex lines. This keeps wrapped labels from
overlapping the preceding token or following row.

An isolated TestPlatform regression, with all rows onscreen to avoid stale
debug bounds from clipped dialog frames, checks both example and token cells
at narrow/regular widths and UI sizes 14 and 24. Restoring the old wrapping
layout makes the regression fail: a description begins before its preceding
token's bottom. The corrected layout passes. All 20 settings_ui tests passed,
including date input painting and reference formatter checks. cargo check
--locked --offline, cargo fmt --check, and strict all-target clippy with
editor-diagnostics passed. No desktop interaction was used; native visual
confirmation remains with the user. Cargo.lock remains excluded.

### Live-preview horizontal rules (2026-09-14)

The existing worker GFM tree now supplies exact-revision thematic-break ranges.
Live preview paints a 2 px theme.border divider with measured indentation and
available content width; the caret row shows its Markdown. Literal shaping and
source mappings stay fixed across reveal, including wrapped markers. Edits and
mode/document resets invalidate the ranges; stale results cannot reinstall them.
Source and Reading behavior are unchanged.

The 95-test editor suite covers valid markers, quotes/lists, Unicode/CRLF,
heading/table/code exclusions, one shared parse, delayed results and structural
context edits. Two new TestPlatform tests cover Light/Dark themes, indentation,
wrapped geometry, every marker caret position, clicking, edits, mode switches
and undo/redo. The complete cargo test --locked --offline --features
editor-diagnostics suite passed. cargo check --locked --offline, cargo fmt
--check, and strict all-target clippy with editor-diagnostics passed as well.
No desktop interaction or native app harness was used; native visual validation
remains with the user. The pre-existing Cargo.lock change is excluded.

### Horizontal-rule flash while entering new lines (2026-09-14)

Every edit previously cleared all divider metadata, exposing source syntax
until background analysis completed. TextWrapper now retains byte-unchanged
source lines and remaps their ranges across edits. Source-line boundary checks
handle newline insertion at either edge, shifted Unicode/CRLF content and
multiple rules, and immediately retire changed or joined lines. No document
copy or Markdown parse is added to the input thread. Exact analysis still
resolves structural context changes; older results never replace retained ranges.

The delayed-worker regressions failed under the original global invalidation
and pass with retention. They exercise repeated newlines, quoted/list-contained
rules, shifted source ranges, edited markers and heading/code context changes.
TestPlatform also exercises four consecutive real Enter actions after a rule.

The full cargo test --locked --offline --target-dir target/headless --features
editor-diagnostics suite passed, including all 96 editor module tests and both
horizontal-rule TestPlatform tests. cargo check and strict all-target clippy
with editor-diagnostics passed in the same target directory; cargo fmt --check
also passed. The ordinary target build hit a locked mithril.exe, so validation
used the existing separate headless target without stopping or interacting with
the running application. Native visual confirmation remains with the user.
The pre-existing Cargo.lock change remains excluded from the commit.

### Windows Move to Trash crash (2026-09-14)

The Windows trash adapter now runs the shell operation on a dedicated thread.
GPUI schedules background work on Windows Runtime MTA workers, while trash
5.2.8 initializes COM as STA and panics if that fails. The fresh thread lets
trash own its apartment; thread creation failures, ordinary shell errors and
backend panics return as I/O errors through the existing session failure path.
The editor and unsaved text remain available on failure.

A headless Windows regression reproduced the incompatible apartment error
0x80010106 with direct execution and passes with the dedicated thread. A second
regression covers error propagation, panic containment and a subsequent
successful operation. Neither calls the real Recycle Bin. The notebook UI
fixture now uses NotesWindow to render actual modal layers; a dialog regression
covers Escape cancellation, confirmation failure and successful fixture deletion.
Existing coverage verifies preservation of unsaved edits after failed deletion.

The full cargo test --locked --offline --target-dir target/headless --features
editor-diagnostics suite passed. cargo check with the same locked/offline target,
cargo fmt --check, and strict all-target clippy with editor-diagnostics also
passed. An initial parallel test build hit a transient linker lock; the full
rerun with -j 2 passed. No desktop interaction or native application launch was
used. Native Recycle Bin validation remains with the user. The pre-existing
Cargo.lock change is excluded from this fix.

### Notebook regex exclusions (2026-09-14)

General settings now accept a comma-separated, notebook-specific regex list.
The parser preserves escaped commas, nested character classes and repetition
counts; invalid drafts retain the last valid settings. The additive
`excluded_patterns` field defaults to empty for existing notebooks. The shared
pure matcher uses slash-separated notebook-relative paths and folder ancestors,
with case-sensitive matching unless a regex opts into `(?i)`.

NotebookState owns the active compiled policy. Explorer reads, notebook search
(including unsaved overlays), and Go to File capture that policy. Preference
changes invalidate directory generations and restart search; expanded folders
cannot bypass excluded ancestors. Explicit file opening, last-file restoration
and document editing/saving keep their existing behavior. Invalid external
patterns preserve the notebook and stored list, disable all custom exclusions,
and expose a notice and settings validation error for recovery.

Twelve new headless tests cover parsing, matching, legacy settings, persistence,
all discovery surfaces, invalid drafts, disabled and stale dialogs, notebook
activation, stale workers, clearing exclusions, and saving an excluded open note.

Verification on Windows: locked/offline `cargo check` in `target/headless`,
`cargo fmt --check`, strict all-target Clippy with
`editor-diagnostics`, and the full locked/offline test suite with
`--target-dir target/headless -j 2 --features editor-diagnostics` passed
(394 tests). Initial linking attempts encountered transient LNK1104
locks in the default and headless build directories; retrying after the locks
cleared passed. No native application launch or desktop interaction was used.
Existing unrelated changes, including the lockfile package reorder and vendor
link-menu work, are excluded from this feature's commit.

## Toolbar customization (2026-09-14)

Settings now includes a Toolbar tab with independent switches for Toggle sidebar,
Explorer, Search and Sort files. Typed intents update the active notebook through
the existing settings writer, or session preferences without a notebook. All four
items default to enabled, including missing fields in older JSON. Buttons retain
their order, actions and availability rules; turning all four off removes the row
and border. Existing menus and shortcuts remain available.

Six new headless tests cover legacy and partial settings, invalid field types,
all 16 visibility combinations, JSON/disk persistence, actual rendered buttons and
order, empty-toolbar restoration, settings acknowledgement, narrow dialog layout,
busy forms, expired dialogs and notebook scope changes. The settings fixture uses
a temporary index through a test-only NotebookState helper. Fresh TestPlatform
windows verify absence because GPUI retains old debug bounds between frames.

Verification on Windows: locked/offline cargo check, cargo fmt --check, strict
all-target Clippy with editor-diagnostics, and the complete locked/offline test
suite with --target-dir target/headless -j 1 --features editor-diagnostics passed
(400 tests). Transient Windows linker/executable file locks cleared on retry; the
final full run passed without concurrent builds. No native application launch or
desktop interaction was used. Native visual validation remains manual. Existing
lockfile and vendor link-menu changes are excluded from this feature's commit.

## Toolbar drag ordering (2026-09-14)

Settings > Toolbar now presents ordered rows with dedicated drag grips and
independent visibility switches. GPUI drag previews show the tool label; upper
and lower insertion targets paint a line without shifting row geometry. A drop
emits a relative move intent, which the settings adapter applies to the current
notebook or session defaults. Hidden items retain their positions and remain
reorderable. Unchanged moves do not enqueue preference writes.

ToolbarSettings stores a four-item order using lowercase serialized item names.
Missing, incomplete, duplicate, unknown or otherwise invalid order values restore
the original default order without changing visibility. The toolbar filters its
buttons from that order while retaining existing actions and availability rules.
Drag payloads identify their originating panel, and the existing adapter guards
reject expired dialogs, changed preference scopes and busy sessions.

Five new tests cover order normalization, every move across all 24 permutations
and 16 visibility masks, headless grip and switch gestures, before/after drop
placement, cancellation, unchanged drops, busy sessions, notebook persistence,
session defaults, hidden-item movement, reopened dialogs and stale drag origins.
Existing persistence and render tests now exercise non-default order as well.

Verification on Windows: locked/offline cargo check, cargo fmt --check, strict
all-target Clippy with editor-diagnostics, all 11 focused toolbar tests, and the
complete locked/offline suite with -j 1 --features editor-diagnostics passed
(405 tests). Initial LNK1104 executable locks cleared after sequential target
retries; the final full suite passed from the default target directory. The
existing target/headless dependency directory is a junction to target/debug/deps.
No native application launch or desktop interaction was used; native visual
validation remains manual. Unrelated lockfile and vendor link-menu changes,
including their architecture-map addition, are excluded from this commit.

## 2026-09-14 — Toolbar icon size presets

Settings → Toolbar now offers Small, Medium and Large presets. ToolbarSettings
stores a typed icon_size with lowercase JSON values; missing and invalid values
fall back to Small. Typed form intents apply immediately through the existing
notebook/session preference scope, dialog-lifetime and busy guards. Unchanged
notebook size selections do not enqueue a write.

The application toolbar owns exact 14/20/28 logical-pixel icons and 24/32/40-pixel
button targets. Explicit Icon children preserve the requested icon-to-target
sizes independently of component defaults. The row wraps at narrow widths and
its height follows button rows, padding and border instead of a fixed height.

Four new tests cover invalid/missing values and persistence, preset clicks and
exact headless icon/button/toolbar geometry, notebook versus session isolation,
stale dialog/scope events, busy form and adapter guards, and two-row wrapping
for all three sizes. Existing toolbar visibility, reorder and persistence tests
continue to pass.
Verification on Windows: cargo check --locked --offline, cargo test --locked
--offline -j 1 --features editor-diagnostics (409 tests), cargo fmt --check, and
cargo clippy --locked --offline --all-targets --features editor-diagnostics --
-D warnings passed. No desktop automation or native application launch was used.
Unrelated lockfile/vendor link-menu changes and their architecture-map addition
remain outside this commit; their tests were present during the full-suite run.
## 2026-09-14 — Markdown formatting toolbar

Added eleven independently configurable formatting controls after the four existing
navigation controls: bold, italic, strikethrough, heading, inline code, code block,
link, blockquote, ordered list, unordered list and task list. The toolbar retains
all three icon presets and wrapping. Preferences preserve known saved order and
visibility, skip duplicate/unknown order entries and append missing items, so old
notebooks gain the enabled formatting controls without losing existing choices.
Application-owned monochrome SVGs are embedded by an asset source that delegates
other icons to the existing component bundle.

The pure markdown_edit module returns UTF-8 replacement ranges, replacement text
and resulting selections. Editor::format_markdown applies one isolated edit with
normal Change events, shared Source/Live history, selection restoration and focus.
Inline syntax toggles, safe backtick delimiters, container-preserving line edits,
list conversion/numbering and fenced code wrap/unwrap have focused regression
coverage. The heading menu supports levels 1–6 and Paragraph with captured document,
editor, source and selection guards. Link uses the existing guarded edit dialog.
Controls and handlers reject Reading, empty/image views and busy sessions, while
standalone Source/Live documents remain editable.

Headless tests exercise toolbar clicks, selection/focus, dirty tracking and history,
all heading levels and stale source, link-dialog cancellation, unavailable states,
all presets and wrapping, formatting settings switches/drag/reopen, preference
migration/persistence and asset lookup/fallback. Pure UTF-8 tests cover Unicode,
empty carets, nested lists/quotes, CRLF, line-end selection boundaries and backticks.

Verification on Windows: cargo check --locked --offline, cargo test --locked
--offline --features editor-diagnostics (427 tests), cargo fmt --check, and cargo
clippy --locked --offline --all-targets --features editor-diagnostics -- -D warnings
passed. Transient Windows linker LNK1104 output-access errors cleared on retry;
the complete suite passed in the default target directory. No native application
launch or desktop interaction was used. Unrelated lockfile/vendor link-menu edits,
their tests and their architecture-map addition remain outside this commit.

## 2026-09-15 — Toolbar reset and Medium default

The Toolbar settings section now provides Reset toolbar to defaults beside the
size presets. One typed ResetToolbar intent restores complete ordering, enabled
states and Medium size in the current preference scope. The adapter retains the
existing notebook/session, dialog-lifetime and foreground-busy guards, and skips
notebook persistence when preferences already equal the defaults. Missing and
invalid icon-size values now use Medium; explicitly saved Small and Large values
remain unchanged until reset.

Headless TestPlatform coverage exercises the actual reset button, stale form and
notebook-scope rejection, form/adapter busy guards, persisted reset and observable
no-op writes. Settings serialization coverage checks Medium defaults and invalid
values alongside round trips for every explicit preset.
Verification on Windows: cargo check --locked --offline, cargo fmt --check, and
cargo clippy --locked --offline --all-targets --features editor-diagnostics --
-D warnings passed. The full cargo test --locked --offline -j 1 --features
editor-diagnostics --target-dir target/headless suite passed all 429 tests.
The default target directory hit Windows LNK1104/output-access errors, including
an inaccessible target/debug/mithril.exe; the existing separate headless cache
completed without changing or stopping any application process. No desktop
interaction or native application launch was used. Unrelated lockfile/vendor
link-menu changes and their architecture-map addition remain outside this commit;
their tests were present in the full-suite run.

## 2026-09-15 — Movable, bounded Settings

Settings uses an opt-in vendored Dialog placement handle owned by the current
form. Opening or changing sections centers the form at 620 logical pixels wide,
with preferred heights of 600 for General/Toolbar, 560 for Templates, 460 for
Font and 420 for Theme. Layout clamps size and placement to 16-pixel viewport
margins and reclamps on resize. Preference edits and selecting the same section
retain placement; section changes reset the content scroll position too.

Only title presses begin a window-wide captured drag; release anywhere ends it
without activating underlying controls. Movable dialogs omit positional entrance
animation and provide a bounded body. Settings keeps title/navigation visible
while the content has its own vertical scrollbar, and uses resolved dialog
width for its narrow layout. Ordinary dialogs keep their original defaults.
No document-session ownership or transition contracts changed.

All 37 Settings tests passed on headless TestPlatform, including existing toolbar
reorder/cancellation/scope coverage. The new geometry regression checks actual
surface/title/navigation/body bounds, final-row scrolling, pointer dragging and
corner clamping, release outside the header/window, resize at 480x360 and 320x240,
all section sizes, preference/same-section retention, reopening and Escape.
Cargo check --locked --offline -j 1 --target-dir target/headless, cargo fmt
--check, and cargo clippy --locked --offline -j 1 --target-dir target/headless
--all-targets --features editor-diagnostics -- -D warnings passed on Windows.
The full cargo test --locked --offline -j 1 --features editor-diagnostics
--target-dir target/headless suite passed all 430 tests. First-link LNK1104
errors were resolved by removing stale .exe build outputs only from the verified
`target/headless/debug/deps` directory, then rebuilding. No application process,
native app output, or desktop interaction was used. Unrelated lockfile and vendor
link-menu changes/tests remained outside this commit; their tests were included
in the integrated run.

## Toolbar Back and Forward — 2026-09-15

Added enabled-by-default Back/Forward controls to toolbar visibility, order, size
and reset settings. Defaults lead with navigation; existing saved orders retain
their entries and append new items. The pure session-only navigation owner uses
revision-tagged proposals, and the session adapter commits only accepted installs.
Sidebar-created notes share that hook. Empty installs clear history, successful
moves remap it, and canonical image visits work after notebook switches.

Verification on Windows used GPUI TestPlatform only: the full diagnostics-enabled
suite passed **436 tests**, including **171 application tests**. New coverage
exercises history boundaries, duplicate visits, branches, move remapping, real
Back/Forward clicks for standalone notes, Save/Discard/Cancel, missing-file errors,
dialog/busy guards, stale loads, Empty installation and cross-notebook image
history. Existing sidebar note creation now asserts its recorded previous visit;
settings migration/persistence tests include all 17 controls.

Passed `cargo check --locked --offline -j 1 --target-dir target/headless`,
`cargo test --locked --offline -j 1 --features editor-diagnostics --target-dir target/headless`,
`cargo fmt --check`, and
`cargo clippy --locked --offline -j 1 --all-targets --features editor-diagnostics --target-dir target/headless -- -D warnings`.
Stale Windows test executables were removed only from the verified headless build
output directory before relinking. Native desktop interaction was not used;
visual confirmation remains user-run. Cargo reports the existing dependency
future-incompatibility notice for proc-macro-error2.
## Retained document tabs — 2026-09-15

Implemented the fourth sequential toolbar/settings/navigation task. TabSet retains
independent Session, editor/subscription, undo/cursor/scroll, Reading view/find,
mode, navigation and save/dirty runtimes. Stable TabId qualifies all captured
session-local identities. New tab, selection, close, duplicate-path focus and
notebook activation preserve occupied tabs; tab selection permits background saves.
Workspace close/move/Trash preparation drains writes, preserves discarded buffers
until success, cancels owned preparation on failure, and resumes eligible timers.
All affected paths and saved disk versions are remapped after successful moves;
Trash clears content only after backend success. Inactive Duplicate uses its source
buffer. Autosave uses each tab's owning canonical notebook or the shared session
default, with explicit General Settings scope. Search receives all retained overlays.

Windows headless verification:

- Full locked/offline diagnostics-enabled suite: **453 passed, 0 failed, 0 ignored**
  (81 library, 188 application, 184 integration tests; doc-tests contain no cases).
- Default `cargo check --locked --offline -j 1 --target-dir target/headless`,
  `cargo fmt --check`, and strict all-target diagnostics Clippy passed.
- New cases cover real tab +/X/selection controls, retained edits/selection/undo,
  Reading scroll, matching session-local write/operation IDs in different tabs,
  inactive write failure and a queued failure during another tab's close prompt,
  multiple Save/Discard/Cancel decisions and the preference barrier, notebook
  switching/autosave scope, both histories on existing-tab focus, inactive Duplicate,
  multi-tab move and failed/successful Trash, and the actual SearchPanel overlay and
  result-focus pipeline. A pure Session test covers stale activation_retained events.
- The full suite log is `target/headless/tabs-full-tests.log`. Existing headless deps
  is a junction into workspace `target/debug/deps`; stale test executables there
  were removed only after checking both exact paths. The running application and
  `target/debug/mithril.exe` were not touched.
- An automatic approval reviewer capacity failure delayed one test launch; retry
  after path inspection succeeded. No native app, desktop input, or visible test
  window was used. Native visual validation remains user-run.

The three initial Toolbar Settings drag regressions came from adding an autosave
scope sentence to every section's shared subtitle. Moving that sentence beside
General's autosave controls restored the bounded Toolbar layout; all existing
settings drag tests pass unchanged.

## Tab content height regression (2026-09-15)

The retained-tab layout added a heightless flex column inside a non-flex,
full-height parent. The editor's percentage height consequently resolved to its
intrinsic height. `file-drop-content` now owns the full-height flex column, with
the non-shrinking tab strip above a flexing, bounded content container. Source,
Live Preview, Reading, images and empty views share that remaining-space slot;
drop handlers and retained tab ownership are unchanged.

The new TestPlatform geometry regression failed before the fix with an editor
height of **40 px** inside a **726 px** pane. It now checks the editor's actual
input bounds, plus Reading and empty-view bounds, against the tab strip and pane
within one pixel. Coverage includes Source/Live/Reading modes, repeated selection
between two document tabs, expanded/collapsed sidebar and 1000x800/720x500 windows.

Windows headless verification:

- All **16 tab tests** passed, including the new regression.
- Full locked/offline diagnostics suite: **454 passed, 0 failed, 0 ignored**
  (81 library, 189 application, 184 integration; no doc-test cases).
- `cargo check --locked --offline --target-dir target/headless -j 1`,
  `cargo fmt --check`, and strict all-target diagnostics Clippy passed.
- Full suite output: `target/headless/tab-layout-tests.log`; check and Clippy
  outputs use the corresponding `tab-layout-check.log` and `tab-layout-clippy.log`.
- A transient Windows LNK1104 on the test executable cleared on retry. No desktop
  interaction or native application harness was used; native visual validation
  remains user-run.

## Inline settings screen (2026-09-15)

Settings now occupies the existing content slot below the retained document tabs.
Notes owns the panel entity and subscription directly; sender identity and captured
preference scope reject stale events. Closing restores the retained view, selection,
scroll position, undo history and focus. Header/Close and section navigation stay
fixed while the body scrolls. Narrow panes use a single scrolling row of section
buttons so navigation does not consume the available body height.

A separate document-interaction guard pauses tabs, notebook navigation, document
commands, search interaction and drops without changing session busy state or
stopping autosave/preferences. Sidebar geometry and application closing remain
available. Escape works from settings fields and from application chrome; an open
font picker consumes the first Escape. Late sidebar-menu dismissal cannot steal
focus, and blocked sidebar clicks cannot change its cursor.

Windows headless verification:

- Full locked/offline diagnostics suite passed: **462 tests** (81 library,
  197 application, 184 integration), zero failures or ignored tests.
- After the final sidebar pointer/focus guard and chrome Escape regression, the
  complete application suite passed again: **198 tests**, zero failures.
- Final `cargo check --locked --offline`, `cargo fmt --check`, and
  `cargo clippy --locked --offline --all-targets --features editor-diagnostics -- -D warnings`
  passed. The existing proc-macro-error2 future-compatibility notice remains.
- TestPlatform coverage includes pane bounds and resize, fixed header/section
  scrolling, repeated opening/drafts, stale panel events, nested Escape, retained
  Source/Live/Reading state and scroll, image/empty views, blocked pointer/actions/
  drops, retained notebook search, autosave and cancelled application closing.
- Logs: `target/settings-full-test.log` and `target/settings-app-test.log`.
  Transient Windows LNK1104 test-executable failures cleared on retry. No native
  application or desktop interaction was used; native visual validation remains
  user-run. The generic vendored dialog implementation was not changed.

## Shared notes in current tabs — 2026-09-15

- All note navigation now installs in the current tab. Canonical-path document
  runtimes retain unsaved text, shared history/baseline, one write/debounce owner,
  and one buffer subscription independently of any view's lifetime.
- InputBuffer broadcasts source edits to independent Source/Live views; selections,
  scroll anchors, find and modes stay local. Reading views receive shared changes.
  Closing one view retains the draft; workspace decisions deduplicate documents.
  Move revalidation, search overlays, save errors and accepted baselines are shared.
- Full `cargo test --locked --offline --features editor-diagnostics -j 1` passed:
  **476 tests** (81 library, 208 application, 187 integration), zero failures or
  ignored tests. Coverage includes shared edits/undo/IME, typing boundaries,
  Source/Live scroll anchoring, Reading updates, Explorer keyboard focus, links,
  picker/history/search routing, pending saves, late edits, external conflicts,
  closing the origin, deduplicated close/Trash decisions and move reindexing.
- `cargo check --locked --offline`, `cargo fmt --check`, and strict
  `cargo clippy --locked --offline --all-targets --features editor-diagnostics -- -D warnings`
  passed. Transient Windows LNK1104 executable locks cleared on retry; the existing
  proc-macro-error2 future-compatibility notice remains.
- Logs are in `target/shared-notes-full-tests.log`, `target/shared-notes-check.log`,
  and `target/shared-notes-clippy.log`. Tests used TestPlatform only; no native
  application, desktop input or OS Trash was used. Native visual validation remains
  user-run. Existing unrelated working-tree changes were preserved.

## Settings modal restoration

- Restored the settings modal over retained editor/image/empty content. The dialog
  builder owns the form and lifetime; weak application handles reject events after
  Root dismissal, including stale Close events targeting a later dialog.
- Kept current form controls, preference scopes, interaction guards and compact
  section navigation. Escape dismisses nested pickers before the settings modal;
  closing restores prior focus and retains document selection, undo and scrolling.
- Verification: all 37 settings tests passed with `cargo test --locked --features
  editor-diagnostics --bin mithril settings_ui::panel::tests -- --test-threads=1`;
  `cargo fmt --check` passed. TestPlatform only, no native desktop interaction.
  Log: `target/settings-modal-restore-tests.log`.

## Responsive settings modal sizing

- Settings now uses 80% of the window content width/height with a minimum
  24-logical-pixel margin. All sections share the same dimensions; title and
  navigation remain fixed while content scrolls, including at 320 x 300.
- DialogPlacement adds opt-in margins and responsive preferred-size updates.
  Unmoved dialogs recenter on resize; dragged positions remain where possible
  and clamp at every edge. Reopening recenters. Other dialogs keep their defaults.
- Added geometry and headless drag/resize/section/scroll tests. Updated the search
  fixture to render NotesWindow's modal layer, and background click tests to
  verify that outside-click dismissal does not activate the covered control.
- All **479 tests passed**: 209 application tests through `cargo test --locked
  --features editor-diagnostics --bin mithril -- --test-threads=1`, plus 81 library
  and 189 integration tests through the current Cargo-built test harnesses.
  The unrestricted cargo test command compiled all 21 harnesses but could not
  replace the running `target/debug/mithril.exe`; Cargo JSON artifacts verified
  all harnesses were current before direct execution. The application harness
  was verified through Cargo separately to capture its Windows-subsystem output.
  Doc tests also passed (zero cases). No application or desktop input was used.
- `cargo check --locked`, `cargo fmt --check`, and strict `cargo clippy --locked
  --all-targets --features editor-diagnostics -- -D warnings` passed. Transient
  test-executable linker locks cleared on retry; the existing proc-macro-error2
  future-compatibility notice remains. Native visual validation stays user-run.
- Evidence: `target/settings-modal-application-tests.log`,
  `target/settings-modal-harness-tests.log`, `target/settings-modal-test-artifacts.jsonl`,
  `target/settings-modal-check.log`, and `target/settings-modal-clippy.log`.

## Combined title/menu bar (2026-09-15)

The application-owned `src/title_bar_ui.rs` replaces the visible window title
with File, View, Search, Sort, Notebooks and Help. It embeds the existing brand
PNG, reserves native macOS traffic-light space, and uses separate Windows
non-client caption/drag regions or Linux client controls. Root already owns the
Linux WindowBorder; no second border is installed. Linux compositor refusal
retains server decorations with the menu row below. Fullscreen retains menus
without logo, drag region or caption controls. OS titles and tab dirty markers
continue to identify the active file.

Small-button label widths are shaped with the current UI font. Menus that do not
fit move into trailing More submenus; File remains visible. Popup positions come
from painted trigger bounds and snap within the viewport. Keyboard shortcuts use
the visible button or More anchor; geometry changes dismiss open menus and
restore focus. Ordinary and overflow menus share the existing command builder.
The vendor submenu builder signatures now accept borrowed FnOnce callbacks,
matching their immediate construction, without changing parent dismissal wiring.

Custom Linux close and native should-close callbacks share the guarded workspace
close request, including unsaved documents and preference-flush failures. The
previous component TitleBar's ancestor mouse-drag callbacks are not reused.

Windows/MSVC verification:

- Seven new title-bar tests passed on TestPlatform, covering menu anchors, Escape
  focus restoration, overflow submenu commands, hidden-menu keyboard access,
  resize dismissal, modal guards, 400/700/1000 px widths with 10/16/24 px UI fonts,
  fullscreen transitions, platform layout reservations, and close Cancel/Discard.
- `cargo check --offline --locked`, `cargo fmt --check`, strict all-target
  diagnostics Clippy and `git diff --check` passed.
- The default executable build passed with `cargo build --offline --locked
  --bin mithril --target-dir target/titlebar-validation`.
- The complete `cargo test --offline --locked --features editor-diagnostics
  --target-dir target/titlebar-validation` suite passed, including existing
  save/close/preference-flush regressions and the pre-existing link-menu work.
  The normal target directory could not replace the running/locked mithril.exe;
  validation used a separate output directory seeded from build caches instead.

Only the Windows Rust target is installed locally. Linux/macOS compilation remains
for the existing validation CI; no push or remote workflow was triggered.
Native close/minimize/maximize/restore, dragging/resizing/snapping, macOS traffic
lights, Linux decorations and high-DPI behavior remain manual platform validation.
No native app, desktop input or visible test harness was launched. These headless
results do not establish resolution of the previously reported native hang.

## Windows title-bar input regression (2026-09-15)

The combined bar's Windows branch previously supplied only native hit-test
regions, with no handlers for client mouse input. It now handles caption presses
and button clicks explicitly while retaining those hit regions. Dragging and
double-click maximize/restore stay confined to the blank caption region; menus
remain ordinary controls. Caption button presses suppress GPUI's native default
handling to avoid executing the same operation twice. Close invokes the shared
workspace close request, preserving unsaved-document and preference barriers.

`title_bar_ui/windows.rs` posts move/minimize/maximize/restore system commands to
the current window. This keeps native modal loops outside the GPUI input callback
and its borrows. GPUI 0.2.2's Windows move method is a no-op and its zoom method
only maximizes, so this adapter also supplies the missing move/restore behavior.
Tests record native-command intents without touching an OS window. Two added
TestPlatform regressions cover actual caption/menu pointer dispatch, double-click,
release outside a button, and dirty Close/Cancel; all nine title-bar tests pass.

`cargo check --offline --locked`, `cargo fmt --check`, strict all-target Clippy
with editor diagnostics, and `git diff --check` pass. The complete suite passes
with 488 tests using `cargo test --offline --locked --features editor-diagnostics
--target-dir target/titlebar-validation`. Full-suite evidence is in
`target/titlebar-controls-tests.log`; compiler/lint logs are
`target/titlebar-controls-check.log` and `target/titlebar-controls-clippy.log`.
Native Windows dragging, snapping, minimize/maximize/restore and close remain
user-run validation. No native window or desktop input was used during testing.

## Settings scrollbar viewport anchoring (2026-09-15)

SettingsPanel now places its vertical scrollbar on a relatively positioned
wrapper beside the tracked scrolling viewport. The track and pointer hit area
stay fixed while section contents move. The existing scroll handle, section
reset behavior, modal dimensions, title and navigation remain unchanged. This
is local to Settings; the shared scrollbar component and preference APIs do not
change.

The added TestPlatform regression reproduced the original failure: after an
80-pixel wheel scroll, clicking the track at the viewport's bottom left the
offset at -80 pixels instead of reaching the -497.5-pixel limit. With the fix,
wheel scrolling, track clicks in both directions and thumb dragging reach the
expected positions at 1000x800, 480x360 and 320x300 window sizes and after growing
the window again. The last toolbar row remains reachable, title/navigation
bounds remain fixed, and switching to Theme resets the offset and leaves no
scroll range when that shorter section fits.

`cargo check --offline --locked`, `cargo fmt --check`, strict all-target Clippy
with editor diagnostics and `git diff --check` pass. The complete
`cargo test --offline --locked --features editor-diagnostics --target-dir
target/titlebar-validation` run passes all 490 tests, including concurrent
close-button and link-menu work that is excluded from this change. Logs are
`target/settings-scrollbar-full-tests.log`, `target/settings-scrollbar-check.log`
and `target/settings-scrollbar-clippy.log`. The initial focused settings run
passed the scrollbar regression but caught the then-in-progress close-button
test; the full run includes its passing fix.

All testing uses TestPlatform without native desktop interaction; visual
confirmation of the native scrollbar remains user-run validation.

## Settings close-button pointer regression (2026-09-15)

The movable dialog title overlapped the close control. A press on the close icon
started a title drag, whose capture handler consumed the mouse release before
the close click could execute. The close control now stops left-button press
propagation while retaining its existing callbacks, cleanup and focus behavior.

The new TestPlatform pointer regression failed before the fix with "close icon
did not dismiss Settings" and passes afterward. It covers initial dismissal,
closing after dragging and resizing, fresh form identity on reopen, focus
restoration and Escape. All 40 Settings tests and the complete 490-test suite
(including dialog placement) pass. Commands used:

- `cargo test --offline --locked --features editor-diagnostics --bin mithril settings_ui::panel::tests --target-dir target/titlebar-validation`
- `cargo test --offline --locked --features editor-diagnostics --target-dir target/titlebar-validation`
- `cargo check --offline --locked`
- `cargo fmt --check`
- `cargo clippy --offline --locked --all-targets --features editor-diagnostics -- -D warnings`

Logs are in `target/settings-close-{regression,focused,tests,check,format,clippy}.log`.
The full suite includes concurrent unrelated workspace work. No native app,
desktop input or visible test harness was launched; native UI validation remains
user-run.

## Input borders inherit the palette border (2026-09-15)

Appearance application now maps theme.input to theme.border, covering sidebar,
Settings and other shared input-border consumers. Focused inputs retain theme.ring.
The existing palette regression covers default and custom Light/Dark borders
and verifies that applying the palette preserves the focus color.

All 490 headless tests pass with `cargo test --offline --locked --features
editor-diagnostics --target-dir target/titlebar-validation`. `cargo check
--offline --locked`, `cargo fmt --check`, strict all-target Clippy with editor
diagnostics, and `git diff --check` also pass. Logs are in
`target/input-border-{tests,check,clippy}.log`. The initial default-target test
build encountered a locked `target/debug/mithril.exe`; the alternate target
completed without interacting with the running application. Full-suite results
include existing unrelated workspace changes, excluded from this commit.

## Live-preview strikethrough (2026-09-15)

Ordinary Live-preview text now renders GFM strikethrough through worker-derived
Delete ranges, mapped delimiter hiding and actual shaped-run decorations.
Code/escaped markers remain literal; nested emphasis, links and inline code
compose with the strike. Exact-revision acceptance and edit/reset invalidation
prevent stale ranges. Source and Reading behavior remain unchanged.

The full headless suite passes with `cargo test --offline --locked --features
editor-diagnostics --target-dir target/titlebar-validation`. Focused regressions
cover Unicode, multiline parsing, code/escape exclusions, mapped presentation,
stale analysis and rendered runs through Light/Dark themes, wrapping, marker
reveal, edits, Undo/Redo and Source mode. `cargo check --offline --locked`,
`cargo fmt --check`, strict all-target Clippy with editor diagnostics and
`git diff --check` pass. Logs: `target/strikethrough-{tests,check,format,clippy}.log`.
The alternate test target avoids a locked default-target mithril.exe. No native
app or desktop input was used. The suite includes unrelated existing workspace
changes, excluded from the strikethrough commit.

## Code-block copy buttons (2026-09-15)

Live preview and Reading now offer a top-right Copy code icon with a tooltip.
The shared control stops pointer presses from changing editor selections. Live
uses parsed block values with source-revision guards, and reserves opening-row
width; Reading reserves trailing space and retains custom action support.

`cargo test --locked --features editor-diagnostics --test code_copy --test
code_highlighting` passes all five headless tests. Coverage includes Unicode,
CRLF, quotes/lists, empty and unfinished fences, unchanged source/selection,
Source-mode clicks, and existing theme/wrapping/history geometry regressions.
`cargo check --locked`, `cargo fmt --check`, and strict all-target Clippy with
editor diagnostics pass. GPUI retains debug bounds across frames, so Source-mode
validation checks clipboard behavior instead of stale selector absence.
No native app or desktop input was used. Existing unrelated changes remain
outside this commit.

## Command surface port 1: Markdown formatting (2026-09-15)

The command palette now exposes all sixteen previously toolbar-only formatting
operations: Bold, Italic, Strikethrough, Inline code, Code block, Blockquote,
Ordered list, Unordered list, Task list, Heading 1–6 and Paragraph. Link keeps
its existing dialog command. Mod+B and Mod+I are documented via the shared
shortcut definitions and bind only in DocumentEditor, with exact editor focus.
Toolbar, palette and shortcuts share apply_markdown_format, which rechecks
editable-document capability and active dialogs before Editor::format_markdown.
Heading popups retain their editor/document/source/selection checks.

Focused headless verification uses target/titlebar-validation to avoid the
user’s running default-target executable. The library commands filter passed
12 tests, and the binary formatting filter passed 7 tests. New coverage checks
all sixteen palette operations in Source/Live with Unicode selection and one-step
Undo, every capability combination, actual Bold/Italic keystrokes and palette
input isolation, and modal rejection of direct formatting. No desktop interaction
or native app harness was used.
The full toolbar module (12), palette module (4), and Help shortcut module (3)
also pass, including stale-heading rejection and platform key documentation.
`cargo fmt --check` and `git diff --check` pass. A transient Windows linker lock
on the just-run test executable cleared on retry.

## Command surface port 2: New notes and tabs (2026-09-15)

File now includes New note, New tab and Close tab. Mod+N creates a disk note in
the active notebook root through the shared guarded new_note adapter and
Next::NewNote; Mod+T creates an empty tab and Mod+W closes the active tab through
existing retained-document lifecycle owners. The palette adds New tab and Close
current tab, including availability without a notebook. Shortcut Help uses the
same registrations, with Command on macOS and Control elsewhere. All new routes
retain busy/modal guards, save/discard/cancel and shared-document semantics.

Headless TestPlatform checks passed: retained tab suite (24), palette suite (5),
title-bar suite (10), Help shortcuts (3), and library commands filter (12).
New regressions exercise actual shortcut dispatch, root note creation versus
empty tabs, cancellation before disk creation, dirty-tab close cancellation,
File menu tab actions, standalone palette actions and modal rejection.
Existing retained-tab tests cover shared views, save failures and last-view
closure. cargo fmt --check and git diff --check pass. Tests used --offline
--locked --target-dir target/titlebar-validation; transient Windows linker locks
cleared on retry. No desktop interaction or native application was used.

## Command surface port 3: View modes (2026-09-15)

The palette now offers Source mode, Live preview and Reading view. Mod+Shift+M
cycles Source -> Live preview -> Reading -> Source, documented by the same Help
registration that binds the shortcut. Menu actions, palette dispatch and cycling
share change_view_mode, which checks busy/modal state before calling set_mode.
The underlying mode owner retains per-tab editor/selection/history, active
notebook preference scope and explicit activation preference replay semantics.

Headless checks passed: palette adapter suite (7), activation preference suite
(4), Help suite (3), and mode capability/discovery matrix (1). New tests cover
actual palette activation and standalone cycle keystrokes, Unicode selection,
editor identity, one-step Undo/Redo, notebook mode persistence, and rejection
during dialogs and dirty-navigation prompts. Commands remain available for empty
and image views as in View menu. Tests used --offline --locked --target-dir
target/titlebar-validation; a transient linker lock cleared on retry.
cargo fmt --all --check and git diff --check pass. No desktop interaction or
native application harness was used.

## Command surface port 4: Settings and Replace shortcuts (2026-09-15)

The palette now includes Application: Settings, sharing open_settings with the
File menu and new Mod+, shortcut. Existing busy/modal guards preserve the active
Settings form and its drafts. Palette activation dismisses the palette before
opening Settings. Replace uses the existing guarded ReplaceFile action via
Ctrl+H on Windows/Linux and Cmd+Option+F on macOS, preserving system Cmd+H Hide.
The shared shortcut generator supplies Help and platform-specific bindings.

Headless checks passed: Settings palette/shortcut modal regression (1), palette
adapter suite (8), Help/platform shortcut suite (4), and library commands filter
(14). Coverage includes actual palette selection and shortcut dispatch, repeated
Settings shortcuts preserving section/invalid drafts, another modal blocking
Settings/Replace, Source/Live Replace and Reading/empty rejection, catalogue
availability across busy/document/notebook states, and exact binding/action
metadata for both platforms. Tests used --offline --locked --target-dir
target/titlebar-validation --features editor-diagnostics. cargo fmt --all --check
and git diff --check pass. Native macOS behavior remains manual validation;
no desktop interaction or native application harness was used.

## Command surface port 5: Back/Forward navigation (2026-09-15)

Back and Forward now appear in the palette and View menu, and bind Alt+Left/Right
on Windows/Linux or Cmd+Option+Left/Right on macOS. Help uses those same bindings.
All surfaces share navigate_history, retaining session Save/Discard/Cancel,
revision-guarded cursor installation and failed-load behavior. Capability snapshots
read active-tab history independently of notebook/document presence; palette
activation and the navigation owner both recheck current targets. The first three
View menu entries retain their order.

Headless checks passed: binary history filter (11), library commands filter (15),
View/title-bar suite (10), Help/platform shortcuts suite (5). Coverage includes
actual shortcut, View menu and palette activation; standalone history; empty-tab
isolation; stale palette availability; busy/dialog rejection; cancelled and failed
loads; and image revisits after notebook changes. Exact macOS/Windows binding
actions and Help labels are checked. Commands used --offline --locked --target-dir
target/titlebar-validation --features editor-diagnostics. cargo fmt --all --check
and git diff --check pass. No desktop interaction or native harness was used;
native macOS input behavior remains manual validation.

## Command surface port 6: Sidebar and Explorer (2026-09-15)

Toggle sidebar and Show Explorer now appear in the palette and bind
Ctrl+Shift+B/E on Windows/Linux or Cmd+Shift+B/E on macOS. View gains Show
Explorer; its existing Collapse sidebar entry now uses the shared action, with
both actions dispatched after popup dismissal to preserve destination focus.
Existing sidebar adapters retain notebook, busy and modal checks, Search panel
state and editor/tree/query focus behavior. Help uses the binding catalogue;
Explorer and Search toolbar tooltips show the appropriate platform modifier.
The first three View entries remain Source, Live preview and Reading.

Headless checks passed: sidebar-filter binary tests (5) and library tests (3),
View/title-bar suite (11), command catalogue suite (15), and Help suite (6).
Coverage exercises palette activation and fresh notebook availability, actual
menu dismissal/focus, shortcuts from Search and document/Reading focus,
Search query/results retention without rescanning, and Settings/dialog/busy/
notebookless rejection. The macOS Ctrl+Shift+E editor selection binding does
not collide with the new Cmd+Shift+E shortcut. Commands used --offline --locked
--target-dir target/titlebar-validation --features editor-diagnostics.
cargo fmt --all --check and git diff --check pass. No desktop interaction or
native application harness was used; native macOS input remains manual validation.

## Command surface port 7: Discovery menus and core palette commands (2026-09-15)

File gains Go to a note after Close tab; View gains Command palette after Explorer.
The palette gains Open file, Save, Manage notebooks and Keyboard shortcuts. Open
file explicitly enters the system-picker owner even with an active notebook;
Go to a note and Mod+O retain notebook discovery and standalone picker fallback.
Save uses the existing save capability, including Reading, and Manage notebooks
rechecks index writability before activation. Palette execution rejects any
active dialog in addition to fresh command/busy checks.

Headless checks passed: palette adapter suite (12), title-bar suite (12), and
command catalogue suite (16). Four new TestPlatform tests exercise actual menu
keyboard routing, query focus after dismissal, palette Open/Save/Manage/Help,
Ctrl/Cmd+O routing, Reading save, fresh empty-document/index availability, and
modal/busy rejection. Test-only file-picker capture avoids unsupported native
path dialogs while retaining production session entry/completion. Commands used
--offline --locked --target-dir target/titlebar-validation; cargo fmt --all
--check and git diff --check pass. A transient Windows test-executable linker
lock cleared on retry. No desktop interaction or native harness was used;
native picker presentation remains manual validation.

## Command surface port 8: Task completion palette and date shortcut (2026-09-15)

The palette now exposes Text: Toggle selected tasks, sharing the existing
Mod+L completion/create transaction. Format: Task list retains its separate
apply/remove-syntax behavior. Mod+Shift+D inserts the configured current date
only from focused DocumentEditor content, with the binding also supplying Help.
Both date surfaces and both task surfaces use guarded Notes adapters before
calling existing isolated Editor operations. Reading, non-document content,
busy sessions and active dialogs reject edits; date format scope is unchanged.

Headless checks passed: palette adapter suite (16), command catalogue suite (16),
Help suite (6), and Editor adapter suite (8). Four new TestPlatform regressions
exercise Source/Live task completion versus formatting, Unicode selections,
one-step undo, palette/shortcut date parity and notebook/session format scope,
Reading/dialog/busy/empty guards, and document-find/palette/Settings focus
isolation. Deterministic bracketed Unicode date formats avoid clock-boundary
flakiness; existing templates coverage owns token rendering. Commands used
--offline --locked --target-dir target/titlebar-validation. cargo fmt --all
--check and git diff --check pass. No desktop interaction or native harness was
used; physical macOS shortcut behavior remains manual validation.

## Command surface port 9: selected Explorer actions (2026-09-16)

Added NotebookTree-only F2 Rename and palette Copy selected relative path / Reveal
selected entry. Palette captures the selected row before focus moves, displays
its path, and passes that exact target through dismissal. Shared entry guards
reject stale notebook/generation/row identities, missing/link paths, busy/modal
interaction and Rename during writes. Existing Rename/move and Reveal owners
retain save, history and filesystem behavior; no active-document fallback.

Headless verification: all 19 goto adapter tests passed after switching their
fixture to the real NotesWindow modal host; the final 8 explorer-filter tests
include four new selection-action tests and all pass. Existing entry-menu suite
passes all 9 tests, including ancestor rename history, stale targets and Reveal
backend failures. New tests exercise actual F2/form input and palette activation,
selected versus active document, files/folders and empty tabs, selection changes,
stale notebook/generation, removed paths, invalid/link/loading rows, and modal/write
guards. Reveal is injected in tests; no desktop app or OS file manager launched.
Native keyboard/file-manager presentation remains manual validation.

## Command surface port 10: Sort picker and View theme toggle (2026-09-16)

Explorer: Sort files opens a searchable palette mode with the six FileSort::OPTIONS,
marking and initially selecting the active order. Escape returns to commands.
Activation captures the notebook root across dismissal, rejects a changed/missing
notebook, and calls set_file_sort for Explorer resort and SettingsWriter persistence.
Tree refresh alone does not invalidate a preference choice for the same notebook.
View appends Toggle light/dark mode, sharing a guarded settings adapter with the
existing palette command. Theme scope remains active notebook or session defaults;
no document is required. The first three View entries remain the view modes.

Focused Windows TestPlatform checks pass: goto adapter suite (23), title-bar suite
(13), and command catalogue suite (17). Four new tests exercise all six choices,
current-choice selection, query/Enter/Escape and focus, preserved document/tab/text/
selection, persisted sort plus actual Explorer order, refresh and stale notebook
scope, modal/busy rejection, and theme menu routing on standalone and empty views
with notebook/session persistence. All writes target temporary fixtures. Commands
used --offline --locked --target-dir target/titlebar-validation, with
--features editor-diagnostics for the binary suites. No desktop interaction or
native application harness was used; native platform presentation remains manual.

The inventory now separates current implementation (44 palette commands, 35 app
bindings, 17 toolbar tools, 7 File actions and 28 static top-menu actions) from the
original historical matrices, documenting completion of all ten ranked ports.

Final integrated gates after all ten ports pass on Windows: cargo test --offline
--locked --features editor-diagnostics --target-dir target/titlebar-validation
reports 535 passed, 0 failed, 0 ignored across 24 result groups (including the
empty doc-test group). cargo check --offline --locked --target-dir
target/titlebar-validation and cargo clippy --offline --locked --all-targets
--features editor-diagnostics --target-dir target/titlebar-validation -- -D warnings
both exit successfully. cargo fmt --check and git diff --check also pass. These
runs include the pre-existing working-tree changes; unrelated files and the
existing link-context architecture paragraph are excluded from this port's commit.
No native Windows, macOS or Linux application validation is claimed.

## Command palette shortcut badges (2026-09-16)

Command rows now show right-aligned bordered Kbd badges from registered bindings,
using the help adapter's semantic command/action/context mapping. Lookup uses
Notes or DocumentEditor rather than the focused palette input. Disabled commands
retain their shortcuts; unbound commands retain the full text width. Open file
does not inherit Go to a note's binding, and direct view modes do not inherit the
cycle binding. Rows retain their height, text truncation and execution guards.

New coverage checks all command mappings against both platform catalogues and
uses TestPlatform to verify search focus, editor/application badges, narrow-row
spacing, vertical alignment, disabled execution and unbound-row text width.
GPUI retains removed debug selectors, so absence is checked through current text
bounds and binding lookup rather than stale selector entries.

Windows verification passes: cargo test --offline --locked --features
editor-diagnostics --target-dir target/titlebar-validation (537 passed, 0 failed,
0 ignored across 24 result groups), cargo check --offline --locked --target-dir
target/titlebar-validation, cargo fmt --check, and cargo clippy --offline --locked
--all-targets --features editor-diagnostics --target-dir target/titlebar-validation
-- -D warnings. No desktop interaction was used. Native platform presentation
remains manual validation; the pre-existing Cargo.lock edit is excluded.

## Consistent Lucide toolbar icons (2026-09-16)

Replaced all eleven custom formatting SVGs with unmodified official Lucide
assets. Sort now uses Lucide arrow-up-narrow-wide instead of the component's
modified chevrons icon. Navigation, sidebar, folder and search retain their
existing Lucide assets. assets/toolbar/README.md records the upstream revision,
icon mapping and Lucide-only policy for additions; the upstream license is
included. Toolbar sizing, theme colors and command behavior are unchanged.

Validation: all twelve downloaded SVGs parse as XML and preserve the common
24-by-24 viewBox, 2-unit stroke, currentColor and rounded caps/joins. Windows
cargo check --offline --locked, cargo fmt --check and strict all-target Clippy
with editor-diagnostics pass using target/titlebar-validation. The full headless
suite passes with cargo test --offline --locked --features editor-diagnostics
--target-dir target/titlebar-validation --config
'profile.test.package.mithril.codegen-units=16'. This package-only build override
avoids a locked pre-existing test executable (LNK1104) without touching its
running process. No desktop interaction was used; visual UI validation remains
manual. The pre-existing Cargo.lock edit is excluded.

## Sidebar first in the default toolbar (2026-09-16)

The default toolbar now begins Sidebar, Back, Forward, Explorer, Search, Sort.
New/default layouts and Reset toolbar share this order; saved orders, including
the previous default, retain their positions. Updated headless drag expectations
and extended the existing settings normalization test to cover the saved old order.

Windows verification passes: 31 focused toolbar tests, all 537 headless tests,
cargo check, cargo fmt --check, and strict all-target Clippy with editor-diagnostics.
Cargo checks used --offline --locked and target/titlebar-validation. Tests used
--config 'profile.test.package.mithril.codegen-units=32' to avoid LNK1104 when
overwriting older test executables. No desktop interaction was used, and the
pre-existing Cargo.lock edit is excluded.

## Sort under View (2026-09-16)

Moved the top-level Sort menu to View > Sort files, reusing the same sort menu
builder as the toolbar. Search retains its file, all-files and replace commands.
The top bar now contains File, View, Search, Notebooks and Help. Menu widths,
overflow and popup anchors derive their counts from the menu list. Existing
headless menu tests follow the updated menu positions and overflow count.

Windows verification passes: all 13 focused title-bar tests, the full headless
suite, cargo check, cargo fmt --check and strict all-target Clippy with
editor-diagnostics. Cargo checks used --offline --locked and
target/titlebar-validation. Tests used the package-only override
--config 'profile.test.package.mithril.codegen-units=64' after the previous test
executable could not be overwritten (LNK1104). No desktop interaction was used;
native visual validation remains manual. The pre-existing Cargo.lock edit is
excluded.

## Edit menu (2026-09-17)

Added Edit after File, with Alt+E and shortcut labels from existing action
bindings. Undo/Redo, Cut/Copy/Paste/Select all use component actions. Format
contains inline styles, headings/paragraph, lists, blockquote, code block and
Link; Insert date and Toggle task completion reuse the command adapter.
Callbacks retain document/editor identity, mode, source and selection and
recheck capability, busy and modal guards before dispatch. Deferred execution
preserves destination focus through nested popup dismissal. Empty views retain
application focus when dismissing Edit. Reading disables edits and copies
rendered selections through TextView's Focusable implementation and opt-in
persistent_selection; other TextViews retain their previous selection policy.

Windows verification: the full 543-test headless suite passed, followed by the
20 focused title-bar tests after adding the empty-view focus guard. Cargo check,
cargo fmt --check and strict all-target Clippy with editor-diagnostics passed.
Cargo checks used --offline --locked and target/titlebar-validation; tests used
--config 'profile.test.package.mithril.codegen-units=64'. No desktop interaction
was used. Native visual validation remains manual.

## Mermaid diagrams (2026-09-17)

Implemented native offline diagrams for fenced Mermaid blocks in Live preview
and Reading. Pinned Merman 0.7.0 produces resvg-safe SVG; resvg 0.45.1 rasterizes
on a separate worker with external/embedded image resolution disabled. A shared
keyed queue deduplicates visible requests and abandons obsolete weak tickets;
the cache, pending queue, source bytes and raster dimensions are bounded.
Live preview collapses source rows into an image anchor, reveals the full block
for editing/selection, and preserves source hit testing and viewport anchoring.
Reading retains source mappings and reveals active Find results. Copy code,
shared undo history, native theme changes and source/error fallback are preserved.

Windows verification: `cargo check --locked`, `cargo fmt --check`, and
`cargo clippy --locked --all-targets --features editor-diagnostics -- -D warnings`
passed. `cargo test --locked --features editor-diagnostics` passed 548 tests;
the final five-test Mermaid target additionally passed the subsequently added
shared-buffer/scroll regression. Earlier focused editor/code-copy/highlighting,
shared editor and Reading lifecycle tests also passed. A concurrent focused link
attempt hit Windows LNK1104 while the full suite held the same test executable;
rerunning after it exited passed without configuration changes.
Headlessly generated light/dark sample images were inspected for text and arrow
rendering. No application or desktop interaction was used. Native interactive
visual validation remains manual; Linux/macOS are covered by the existing CI
matrix but were not run locally. The pre-existing Cargo.lock package reorder is
excluded from the feature commit and remains in the working tree.

## Mermaid background correction (2026-09-17)

Reproduced an opaque white canvas in dark mode with a failing native raster pixel
assertion. Merman 0.7.0 applies the dark element palette but retains a white SVG
root background. The shared renderer now adds RootBackgroundPostprocessor to
the existing resvg-safe pipeline, making only the root canvas transparent so
Live preview and Reading inherit their themed native surfaces.

The five-test Mermaid target passes, including new transparent-corner assertions
for flowchart and sequence diagrams in both themes. Light/dark sequence artifacts
were composited onto representative surfaces and inspected headlessly.
`cargo check --locked`, `cargo fmt --all --check`, and
`cargo clippy --locked --all-targets --features editor-diagnostics -- -D warnings`
passed. The focused test command was `cargo test --locked --features
editor-diagnostics --test mermaid --config
'profile.test.package.mithril.codegen-units=64'`; the command-only profile override
avoided a Windows LNK1104 lock on the original test executable. No desktop was
launched or controlled; native visual validation remains manual. The unrelated
Cargo.lock reorder remains excluded.

## Centered editor preference (2026-09-17)

Settings General now groups a Centered editor toggle and Width (px) input under
Editor. Full width remains the compatibility default; centered mode starts at
700 logical pixels and accepts whole numbers from 200 through 4000. Invalid
input drafts stay visible without replacing the last valid preference. Turning
the toggle off disables the input and preserves its value. Existing appearance
scope, stale-form/busy guards and the ordered notebook settings writer apply.

The root renderer centers the retained document pane in Source, Live preview and
Reading, capping its width to the preference and shrinking to the available area
beside the sidebar. Existing pane padding is included; tabs, toolbars, images and
empty content retain their layout. No vendored editor changes were needed.

Windows verification: `cargo check --locked`, `cargo fmt --check`, and
`cargo clippy --locked --all-targets --features editor-diagnostics -- -D warnings`
passed. `cargo test --locked --features editor-diagnostics --lib --bin mithril`
passed all 353 tests (89 library and 264 application), including four new tests
covering schema compatibility/validation, invalid drafts and remembered values,
persistence and stale/busy guards, and all-mode pane geometry across window and
sidebar sizes. The existing selection/undo test now also changes the centered
setting and width while retaining the document.

The broader `cargo test --locked --features editor-diagnostics centered_editor`
invocation failed while replacing `target/debug/mithril.exe` with Windows access
denied; a Mithril process was already running. Integration targets therefore
were not run in this verification. The headless library/application targets
avoid that executable and passed. No desktop interaction or native application
launch was used. Native visual validation remains manual. The pre-existing
Cargo.lock reorder remains excluded from this feature commit.

## Centered columns retain full-width scrollbars (2026-09-17)

Moved the width constraint inside the vendored Input and TextView instead of
constraining their outer scroll containers. Source and Live preview keep a centered
text leaf, full-width search and wheel input, and a scrollbar at the document
viewport's right edge. Reading centers/pads virtualized blocks inside a full-width
list, with its scrollbar at the same edge. Column changes invalidate Reading row
measurements and restore the logical scroll anchor. The vendor patch guide records
the new opt-in layout APIs and upgrade obligations.

Headless verification passed all 354 library/application tests (89 + 265), including
updated geometry coverage across all modes, narrow windows/sidebar sizes, gutter
wheel scrolling, right-edge track clicks/thumb dragging, and Reading scroll anchor
retention on width changes. Command: `cargo test --locked --features
editor-diagnostics --lib --bin mithril --config
'profile.test.package.mithril.codegen-units=64'`. The command-only build variant
avoids Windows LNK1104 locks on the original test executable. Check, format and
strict all-targets Clippy gates passed. The complete Cargo test command was also
attempted but could not replace the locked `target/debug/mithril.exe`, so integration
tests remain unrun for this change. No native app or desktop interaction was used;
manual visual validation remains with the user. Unrelated Cargo.lock changes are
excluded.

## Centered editor in the View menu (2026-09-17)

View now exposes a checked Centered editor item after the three view modes.
It uses the existing appearance preference, preserves the configured width, and
rechecks modal/busy guards at execution. The Settings toggle and width input remain
available. Existing View-menu keyboard tests were updated for the new item; new
tests cover toggling in each mode, document/selection retention, standalone and
notebook preference scope/persistence, empty documents, and modal/busy rejection.

Final verification of both the scrollbar correction and menu addition passed all
356 library/application tests (89 + 267), using `cargo test --locked --features
editor-diagnostics --lib --bin mithril --config
'profile.test.package.mithril.codegen-units=32'`. Build variants 64 and 128 also ran
successfully during the work; a subsequent link to an already-used executable
encountered Windows LNK1104, so the final run used a fresh command-only variant.
Check, format and strict all-targets Clippy passed on the final code. The complete
integration suite remains blocked by the locked application executable as recorded
above. No native app/desktop testing was performed, and unrelated Cargo.lock edits
remain excluded from both commits.

## Source editing for text files (2026-09-17)

The file loader now accepts arbitrary UTF-8 text files and rejects binary data.
Non-Markdown documents force Source without persisting that forced mode, and
retain shared text/history, saves and conflict handling. Filename-based syntax
highlighting covers supported grammars with a plain-text fallback. Markdown
formatting and list semantics are unavailable for other file types.

Verification passed: 91 library tests and 269 application tests with
`cargo test --locked --features editor-diagnostics --lib --bin mithril`, plus all
five public storage tests by executing the freshly built `document` test binary.
Check, format and strict all-targets Clippy passed. Full integration testing was
attempted but Cargo could not replace the locked `target/debug/mithril.exe`;
`cargo rustc --test document` produced the updated storage test binary despite the
same application-binary lock. No desktop interaction occurred. Unrelated
Cargo.lock changes are excluded.
## External-file tab indicator (2026-09-17)

Tabs now show an “External file” badge when their canonical path is outside the
active notebook, with an explanatory tooltip and full path. Standalone files use
“No notebook is open.”; empty tabs are unbadged. Membership is recomputed on render
so switching notebooks and moving paths cannot leave a stale origin label.

Headless coverage opens an external text file through the real picker completion
adapter, checks badge geometry and tooltip context, activates the containing
notebook and verifies the badge disappears, then checks standalone/empty contexts.
GPUI retains historical debug selectors, so removal is verified by the current tab
button width and current membership policy. Final verification passed 361 library
and application tests (91 + 270), check, format and strict all-targets Clippy.
The five public storage tests also passed as recorded above. The broader integration
suite remains blocked by the running application's locked executable; no desktop
interaction or native application launch was used.
## External-file autosave opt-in (2026-09-17)

Settings → General → Saving now includes an app-wide “Autosave external files”
switch. `Index::autosave_external_files` defaults to false, including when loading
legacy indexes without this field, and persists through the existing ordered
preference writer. Effective autosave uses the same membership rule as the tab
badge: files outside the active notebook, including retained files from another
notebook and standalone files, use this independent flag. Notebook enable settings
are preserved; delays retain their existing notebook/session scope. Toggling the
flag or switching notebooks recomputes timers for all retained tabs. A disabled
flag cancels pending autosaves; captured writes and manual saves retain their
normal lifecycle. The duplicate generic autosave switch is hidden for standalone
files, while their delay remains editable.

Verification passed 365 library/application tests (92 + 273), `cargo check --locked`,
`cargo fmt --check`, and strict all-targets Clippy with editor-diagnostics. New tests
cover legacy defaults/serialization, persisted opt-in/out, actual Markdown and
TypeScript file writes, cancellation of pending writes, unchanged notebook saves,
manual Save while disabled, notebook-switch policy changes, Settings events with
and without a notebook, and rejected events from a closed form. Existing autosave
harnesses explicitly opt in to retain their lifecycle coverage. The full integration
command was attempted but could not replace the running application's locked
`target/debug/mithril.exe`. No native application or desktop interaction was used.
Unrelated Cargo.lock changes remain untouched.
## Notebook-scoped external autosave (2026-09-17)

Supersedes the app-wide scope described above. `autosave_external_files` now belongs
to notebook Settings and defaults to false when absent. The active notebook's flag
controls external tabs, regardless of their retained owning notebook. Switching
notebooks recomputes their effective policy; a notebook's choice is never copied
into another notebook or into standalone session defaults. With no notebook open,
the toggle is session-only and defaults to off. The previous index-wide flag is
ignored on read and omitted on subsequent index writes.

Settings identifies the active notebook beside the toggle and rejects intents from
a dialog captured for another notebook. Explicit changes during same-notebook
activation, including unchanged re-selection, replay onto that notebook only.
Notebook persistence uses the existing writer and flush contract.

Verification passed 367 library/application tests (92 + 275), check, formatting,
and strict all-targets Clippy. Coverage includes independent persisted notebook
values, actual external-file autosave after switching back to the enabled notebook,
no session-default leakage, legacy/default schema behavior, stale form rejection,
and preference replay during same/different notebook activation. The full test
command was attempted but could not replace the running application's locked
`target/debug/mithril.exe`. No desktop interaction occurred. Unrelated Cargo.lock
changes remain untouched.

## Open settings JSON from Settings (2026-09-17)

General Settings now exposes Open settings JSON for the active notebook, disabled
without a notebook or during foreground work. A pending request remains attached
to the dialog's identity, lifetime and scope while the existing preference writer
drains the notebook's latest submissions. Failures leave Settings open with an
error; success dismisses the modal before normal current-tab navigation, preserving
the unsaved-change flow and editor focus.

Document loading admits only the exact notebook settings path through a separate
regular-file/link check; tree resolution and persisted relative paths retain their
metadata exclusion. JSON uses existing Source-mode and shared-buffer handling.
Opening/selecting the settings file does not rewrite settings or replace last_file.
Manual JSON edits do not automatically update in-memory preferences.

Verification: 376 library/application tests passed (92 library + 284 application),
including nine new Windows-executed regressions for source-mode opening, file
identity preservation, unavailable/busy/stale forms, unsaved cancellation, delayed
and failed writes, missing files, retained edits, and metadata path/link checks.
A Unix-only file-symlink regression is also included but was not executed here.
`cargo check --locked --offline`, `cargo fmt --check`, and strict all-targets Clippy
with `editor-diagnostics` passed. Tests used TestPlatform and temporary files;
no desktop interaction or native application launch occurred. Unrelated Cargo.lock
changes remain untouched.

## Independent icon and secondary theme colors (2026-09-17)

Light and Dark palettes now persist six independent RGB colors. Icon defaults to
the built-in font, while secondary defaults to the rounded 12% primary / 88%
background blend (#E4E4E4 Light, #383838 Dark). Missing/invalid new fields resolve
against their containing mode. Settings exposes both pickers through the existing
scoped intents, live synchronization, persistence and two-palette reset.

Secondary drives the shared secondary/hover/active colors while its text retains
the font color. Optional component icon coloring covers neutral buttons, menus,
dialogs, selects and lists, plus application toolbar/caption/disclosure controls.
Explicit semantic colors, disabled styling and primary/status foregrounds remain
independent. The vendor patch documents the opt-in API and render diagnostics.

Verification: 582 tests passed (0 ignored), including 93 library and 286
application tests. The full locked offline suite passed with editor-diagnostics,
one build job and one test thread, using target/theme-validation. The separate
output directory avoided the running application's locked executable; serial
compilation avoided memory exhaustion from concurrent integration-test links.
Check, formatting, whitespace validation and strict all-targets Clippy passed.
Coverage includes compatibility/defaults, RGB boundaries, persistence, independent
palette edits, picker synchronization/reset/stale events, and actual headless icon
rendering with text/disabled/semantic-color preservation. No desktop interaction
or native application launch occurred. Manual visual validation remains with the
user, and the pre-existing Cargo.lock change remains untouched.

## Title-bar sidebar control (2026-09-17)

The menu-bar logo is replaced with a fixed-width sidebar icon before File. The
button uses the existing guarded sidebar owner, preserves Search state and focus,
and remains present in fullscreen and server-decorated layouts. Menu measurement
reserves its width in every mode; it is outside native caption drag regions.

Verification: the two new headless regressions passed, covering click toggling,
missing notebook/modal/busy guards, Search query/results retention and focus.
The title-bar run passed 21 of 24 tests, including fullscreen, overflow and caption
regions; three existing menu-navigation tests encountered the concurrently added
Hide toolbar menu entry and need their navigation counts updated by that change.
No desktop interaction or native app launch was used.

## Toolbar visibility from View (2026-09-17)

View > Hide toolbar toggles a default-true ToolbarSettings::visible preference.
It uses the existing notebook writer, or session defaults without a notebook,
and retains item choices, order and size. Hidden and all-items-disabled toolbars
consume no row or border; resetting defaults restores visibility. Busy/modal
interaction is rejected by the settings adapter.

Verification: two new headless visibility regressions, all 23 title/menu tests,
and all five toolbar-settings tests passed. Coverage includes reclaimed document
space, saved visibility and legacy defaults, notebook/session isolation, reset,
and busy/modal guards. Existing menu navigation tests account for the new entry.
The full locked offline suite, cargo check, cargo fmt --check, and strict
all-targets Clippy with editor-diagnostics passed using one build/test thread.
No desktop interaction or native application launch occurred.

## Optional toolbar sidebar button (2026-09-17)

The customizable toolbar Sidebar item now defaults off, while the title-bar
sidebar button remains available. Missing preferences and Reset toolbar use the
new default; explicit saved true/false choices retain their values. Geometry and
reordering fixtures explicitly enable the optional item when they need it.

Verification: all 37 headless toolbar application tests and all five toolbar
settings integration tests passed with locked offline dependencies. Coverage
includes missing/partial settings, saved visibility round trips, reset, sizing,
ordering and persistence. Formatting and whitespace checks passed. The separate
concurrent visibility change updated the menu-navigation counts; its rerun passed
all 23 title-bar tests, including the new sidebar interaction regression.
No desktop interaction or native application launch occurred.

## Direct toolbar icon reordering (2026-09-17)

Visible toolbar icons now support GPUI drag gestures, including commands whose
normal action is disabled. A floating icon and before/after insertion markers
preserve wrapped layout; only a completed drop updates order. Direct drops and
Settings list moves share the existing settings adapter and ordered preference
writer. Window owner, captured notebook/session scope, toolbar/item visibility,
modal and busy guards reject stale gestures. No reordering schema changes were
needed, and hidden items retain their saved relative order.

Heading and Sort opt into completed-click dropdown activation. The vendored
popover retains the open state from mouse-down across dismissal/repaint, so a
second click closes an open menu. Dragging never invokes a command or opens a
menu; editor text, selection, focus and navigation remain with their existing
owners. The vendor patch documents this opt-in contract.

Verification: the full locked offline editor-diagnostics suite passed 594 tests
(93 library, 298 application, 203 integration; none ignored), using
`target/theme-validation`, one build job and one test thread. After the final
completed-click repaint correction and hidden-toolbar guard, all 37 focused
headless toolbar/settings tests passed again, including eight new regressions.
`cargo check --locked --offline`, `cargo fmt --check`, and strict all-targets
Clippy with editor-diagnostics passed on the final code. No desktop interaction
or native application launch occurred; physical UI validation remains with the
user. The unrelated Cargo.lock change is excluded from this feature commit.

## Restore an empty active tab (2026-09-17)

Tab focus now persists Empty as well as Document content through the existing
notebook preference writer. Creating or selecting an empty tab, or closing a tab
that leaves an empty tab active, clears last_file. Startup keeps the active
notebook and restores one empty tab; selecting a notebook note records it again.
Image selection and the external-file/settings-JSON exceptions remain unchanged.

Verification: three new headless persistence regressions check saved preferences
and fresh-owner startup for new/selected empty tabs, switching back to a note,
closing the active or final note, and closing a background note. All 597 tests
passed (93 library, 301 application, 203 integration), with locked offline
dependencies, one build job and one test thread. Cargo check, cargo fmt --check,
and strict all-targets Clippy with editor-diagnostics also passed. The first full
run exposed the existing Mermaid diagnostic test's assumption that a local
target directory exists; creating that ignored output directory allowed the full
rerun to pass without source changes. No desktop interaction occurred.

## Toolbar alignment preference (2026-09-17)

Settings Toolbar now offers Left, Center and Right above Icon size, applying
changes immediately to the full-width toolbar. Each wrapped row uses the chosen
alignment at every icon size. The typed alignment preference serializes in
lowercase, defaults to Left for missing or invalid values, and uses the existing
notebook/session scope, guarded settings intents and ordered writer. Unchanged
selections skip writes, toolbar visibility retains alignment, and Reset toolbar
restores Left with the other defaults.

Verification: all 601 tests passed (93 library, 304 application, 204 integration;
none ignored) with locked offline dependencies, editor-diagnostics, one build job
and one test thread in target/theme-validation. The focused run passed 40 toolbar
application tests and six toolbar-settings integration tests. New coverage checks
all alignment values, invalid/missing defaults, persistence, unchanged writes,
notebook/session isolation, stale and busy events, immediate layout updates,
reset and hide/show preservation, and wide/wrapped geometry at every icon size.
Cargo check, cargo fmt --check, strict all-targets Clippy with editor-diagnostics,
and whitespace checks passed. Transient Windows LNK1104 test-executable access
errors cleared on retry. No desktop interaction or native application launch
occurred. The unrelated Cargo.lock change is excluded from this feature commit.

## 2026-09-18: workspace toolbar regions

- Moved sidebar controls into the sidebar and navigation/formatting into the content
  column above tabs. Notices now render inside the workspace. Narrow content wraps
  formatting below navigation; equal side reservations center wide layouts.
- Settings and icon drags share fixed region membership and reject cross-region
  moves. Legacy alignment still round-trips but has no UI or layout effect.
- Verification: `cargo check --locked --offline`; 39 headless toolbar tests and
  all 6 toolbar-settings integration tests passed. Formatting applied. Full gates
  follow the right-panel addition. No native application or desktop testing.
- The running debug executable prevents Cargo from replacing the usual final
  binary. Integration tests use `--target-dir target/toolbar-validation --config
  'build.build-dir="target"'` to reuse intermediate artifacts and redirect the final
  binary without touching the running application.

## 2026-09-18: resizable terminal placeholder

- Added the default-enabled Terminal item and backward-compatible 18-item toolbar
  settings. The action opens a session-only right panel with a close control and
  placeholder text; there is no terminal process, command input, or persistence of
  right-panel state. Opening preserves focus and closing returns to active content.
- Workspace geometry now supports either or both side panels. Viewport/visibility
  changes project preferred widths proportionally around 200 px of content and
  rebuild resize state without saving projected widths. Completed user gestures
  save notebook sidebar width and session right-panel width independently of tabs.
- Content errors/notebook notices stay inside the workspace, above the tab strip.
  Toolbars and the right panel remain outside document file-drop hit targets.
  Toolbar padding/gaps use logical pixels so large interface fonts preserve layout.
- Verification: `cargo check --locked --offline`, `cargo fmt --check`, and
  `cargo clippy --locked --offline --all-targets --features editor-diagnostics -- -D warnings`
  passed. The full diagnostic suite passed 606 tests (26 test/doc-test targets),
  using the separate final-artifact directory documented above. The final content
  drop-boundary adjustment passed all 308 application tests. Subsequent explicit
  narrow-resize coverage and the resize callback adjustment passed all 4 focused
  right-panel tests; formatting and strict Clippy were rerun successfully.
- Headless coverage includes region geometry, wrapping/presets, large UI text,
  standalone content, collapse, cross-region rejection, legacy settings, editor
  identity/selection/undo, focus, modal/busy guards, real resize gestures, preferred
  width retention, notebook/tab changes, and both panels at 400 px window width.
  No native application or desktop testing was performed; visual review remains manual.

## 2026-09-18: single-terminal trial

- Replaced the right-panel placeholder with a retained gpui-terminal 0.1.0 view
  over portable-pty 0.9.0. GPUI remains pinned to 0.2.2. Launch, read, write,
  resize, stop, reap and close work stay off the UI thread; bounded channels and
  yielding output batches keep large output from monopolizing rendering.
- Added notebook-scoped PowerShell/Git Bash settings on Windows, session-only
  defaults without a notebook, shell discovery and actionable launch errors.
  Existing settings deserialize with PowerShell; Unix keeps the stored Windows
  preference while using the user's shell. Running sessions retain their launch
  directory and shell when notebooks/preferences change.
- Added Restart/Stop/Retry, retained output and exit status, Ctrl+backtick and
  command-palette discovery, terminal input isolation, input-time modal guards,
  theme/font updates and bracketed paste. Accepted window close waits for cleanup;
  late launches cannot replace newer sessions. Existing document-close cancellation
  still leaves the terminal intact.
- Vendored source includes original MIT/Apache licenses, source revision metadata
  and TERMINAL-PATCH.md. Component patches fix protocol replies, zero-size resize,
  clipping, cursor/inverse/combining-character rendering, paste and output buffering.
  Selection/copy, scrollback navigation, splits, multiple terminals and advanced
  input protocols remain deferred; docs/terminal.md records usage and limits.
- Validation on Windows: cargo check --locked --offline, cargo fmt --check,
  cargo clippy --locked --offline --all-targets --features editor-diagnostics
  -- -D warnings, and the complete cargo test --locked --offline
  --features editor-diagnostics suite passed. The 7 terminal integration tests
  include real PowerShell and installed Git Bash startup, cwd, observed 120x40
  geometry, Ctrl+C, exit/reaping, Stop/Drop and component paste/parser checks.
  TestPlatform coverage includes shell preference persistence/scope, lazy launch,
  retention, restart, focus, shortcuts, modal authority and stale launch results.
- Real PTY signal tests ran outside the Codex sandbox after its restrictions
  prevented Ctrl+C delivery; no native application windows or desktop input were
  used. macOS/Linux code and tests are included in the existing validation matrix,
  but those hosts are unavailable locally and have not been verified in this task.
  Native visual validation remains for the user.

### Terminal settings tab

- Moved Windows shell selection into a dedicated Terminal section after General,
  using a synchronized select control and the existing preference guards.
- Verification: all 3 terminal_shell TestPlatform tests passed, including dropdown
  confirmation, effective-value synchronization, narrow layout, persistence and
  stale-form rejection. No desktop interaction was used.

### Terminal cursor styles

- Added notebook-scoped terminal.cursor_style with Line as the missing-field
  default and Box/Underline alternatives. Standalone preferences are session-only.
  The Terminal section uses synchronized selectors and existing busy/lifetime/scope
  guards; cursor edits never start or restart a shell.
- Notes synchronizes retained terminal appearance while visible or hidden.
  TerminalPanel retains the current style during asynchronous launches; changing
  styles preserves the view, process generation, grid and output. The vendored
  configuration and renderer support all three shapes and preserve SHOW_CURSOR.
- Verification: cargo check --locked --offline, cargo fmt --check, explicit
  rustfmt checks on the changed vendor sources, and strict all-target Clippy with
  editor-diagnostics passed. The complete diagnostics-enabled suite passed all
  636 tests, including 323 application tests and all 9 terminal integration tests.
  New coverage exercises defaults/round trips, cursor geometry and hiding,
  dropdown persistence and guards, hidden terminals, notebook switches, retained
  protocol position and pending-launch configuration.
- Full tests used cargo --config 'build.target-dir="target/terminal-settings-validation"'
  --config 'build.build-dir="target"' test --locked --offline --features
  editor-diagnostics -j 1, preserving the user's running target/debug/mithril.exe.
  The final headless suite ran outside the sandbox because the existing ConPTY
  Ctrl+C scenario cannot complete inside it. No desktop interaction or visible
  application launch was used; native appearance remains manual validation.

### Terminal Tab completion focus fix

- Reproduced Tab being consumed by Root's focus-traversal action before the
  terminal raw key handler. Register terminal-context Tab/Shift+Tab NoAction
  bindings once per app so the existing input guard and byte translation own them.
- The new TestPlatform regression failed before the fix and passes afterward:
  Tab sends HT, Shift+Tab sends backtab, and both retain terminal focus. It also
  covers disabled input, unaffected document text and focus traversal outside
  the terminal. The modal-authority test now exercises both completion keys.
- Verification: all 324 application tests and all 9 terminal integration tests
  passed; cargo check, formatting (including changed vendor sources), and strict
  all-target Clippy with editor-diagnostics passed. Integration used the separate
  terminal-settings-validation output directory and ran outside the sandbox for
  the existing ConPTY Ctrl+C scenario. No desktop interaction was used.

### Terminal Shift+Enter distinction

- Reproduced the terminal collapsing Shift+Enter and Enter into the same CR byte.
  Shift+Enter now emits CSI 13;2u, preserving the key distinction used by multiline
  prompt editors. Enter, Ctrl+J and completion keys retain their existing encodings;
  the terminal does not advertise unsupported full keyboard-protocol negotiation.
- The new TestPlatform regression failed before the fix and passed afterward,
  checking exact input bytes, retained focus and unchanged document contents.
  Existing modal and disabled-input coverage now includes Shift+Enter.
- Verification: all 325 application tests passed with editor-diagnostics, along
  with cargo check --locked --offline, cargo fmt --check, explicit vendor rustfmt,
  and strict all-target Clippy. No desktop interaction or live Claude/Codex prompt
  submission was performed; end-to-end behavior in those CLIs remains manual.

### Multiple terminal tabs

- Replaced the single terminal with stable, monotonically numbered session tabs.
  The header contains a scrollable tab strip, individual close buttons and a fixed
  add button; the former heading, Restart/Retry, Stop and panel-close controls are
  removed. Closing the last tab hides the panel; reopening starts a fresh shell.
- Each tab retains its shell, output and captured launch directory/settings.
  Inactive and hidden sessions continue processing output. Input checks the active
  tab identity, visibility and modal/busy authority; appearance/cursor changes
  synchronize across every retained session.
- Tab closure invalidates pending installation and retains cleanup tasks until
  late launches are stopped and reaped. Accepted window close signals all live
  sessions before joining them and already-closing tabs. Cancelled document close
  preserves all sessions.
- Added TestPlatform coverage for independent input/output, stable labels and
  numbering, neighbor selection, last-tab reopening, overflow and fixed add-button
  geometry, header sizes, hidden sessions/preferences, modal/busy guards, failed
  and exited shells, launch/cleanup barriers and cancelled window close. Added a
  real-PTY scenario proving that stopping one shell leaves another usable.
- Verification: cargo check --locked --offline, cargo fmt --check, and strict
  all-target Clippy with editor-diagnostics passed. All 646 tests passed across
  the full diagnostics-enabled run and targeted completion: 332 application tests,
  10 terminal integration tests, and the remaining library/integration suites.
  Doc-tests also passed (zero cases).
- The initial sandbox run passed the new two-shell scenario but hit the known
  ConPTY Ctrl+C restriction in the existing default-shell test. Rerunning the
  terminal suite outside the sandbox passed all 10 cases; the remaining theme-icon
  and toolbar-settings targets also passed. Full testing used the separate
  terminal-tabs-validation output directory with the shared target build cache,
  preserving the running application binary. No desktop interaction or visible
  app launch was used; native appearance remains manual validation.

## Compact tabs and independent terminal resizing (2026-09-18)

- Matched document tab buttons, padding and corners to the compact terminal tabs.
  The terminal add control follows short tab lists and stays visible on overflow.
- Connected OSC 0/2 titles through a window-independent TerminalTitleChanged event.
  Hidden/inactive sessions update with identity/view/generation guards; empty titles
  restore the shell name. Long labels truncate with full-title tooltips.
- Workspace dividers opt into adjacent-only resizing and persist only the dragged
  side's width. The terminal no longer shares the sidebar's 600 px cap; it keeps a
  160 px normal minimum and reserves 200 px for the editor. Narrow-window fitting
  and session-only preferred-width restoration remain intact.
- Headless tests cover compact heights, adjacent add-button positioning, overflow,
  long titles, split OSC input, hidden updates, resets, stale/closed sessions,
  divider isolation, widths beyond 600 px and narrow-window restoration.
- cargo check --locked --offline, cargo fmt --check and strict all-target Clippy
  with editor-diagnostics passed. The full diagnostics-enabled test suite passed,
  including all 335 application tests and all 10 real-PTY terminal tests.
- Windows sandbox linker output-file failures required rerunning the full suite
  outside the sandbox. Used build.target-dir="target/tab-style-validation" and
  build.build-dir="target" to preserve the running app while reusing build artifacts.
  No desktop interaction or native application launch was used. Native visual
  appearance remains available for user validation. The pre-existing Cargo.lock
  edit was left untouched.

### 2026-09-18 — Top-bar panel controls

- Moved the terminal toggle immediately before the caption controls, using the
  matching right-panel icon and the sidebar button's fixed 28 px target/16 px icon.
  Header overflow reserves only visible controls; terminal actions retain their
  existing focus, lifecycle and modal/busy guards.
- Settings → Toolbar → Top bar independently controls sidebar and terminal button
  visibility. The sidebar preference defaults on; terminal reuses its existing
  saved flag. Neither follows Hide toolbars or workspace icon size, and hiding a
  button leaves its panel/session intact. Legacy toolbar orders remain readable;
  the terminal no longer renders or contributes sizing in workspace toolbars.
- Headless coverage verifies placement, all visibility combinations, narrow menu
  layouts, caption isolation, persistence/session scope, stale/busy guards and
  retaining terminal sessions. Updated terminal and settings scroll assertions to match the relocated control.
- Format, locked offline check and strict all-target Clippy passed. All library,
  application and integration suites plus doc-tests passed across the full run
  and targeted completion. The ConPTY interrupt test required the existing
  outside-sandbox rerun; all 10 terminal integration tests passed there.
- Full validation used target/top-bar-validation with build.build-dir="target"
  to reuse artifacts without replacing the running application. No desktop
  interaction or visible application launch was used. Left Cargo.lock untouched.

### 2026-09-18 — Preserve workspace colors behind palettes

- Added Dialog::overlay_color as a per-dialog backdrop paint override. The shared
  Commands, Go To, Change Notebook and Sort Files palette builder uses transparent
  black while retaining modal mouse handling, outside-click dismissal and focus
  restoration. Ordinary dialogs and sheets retain the theme overlay.
- Added headless coverage for all four palette modes in Light and Dark themes,
  unchanged default dialog colors, outside presses and restored editor focus.
  Updated the architecture map and vendor patch log.
- Locked offline check, format check and strict all-target diagnostics Clippy
  passed. All 25 palette tests passed. The serial full run passed all 99 library
  tests, 348 application tests and remaining integration suites/doc-tests except
  the known sandbox ConPTY interrupt case; all 10 terminal integration tests
  passed when rerun outside the sandbox.
- Full validation used target/palette-overlay-validation
  with build.build-dir="target" after the standard target encountered the running
  application's locked executable. No desktop interaction or visible app launch
  was used. The pre-existing Cargo.lock change and concurrent unrelated edits are
  excluded from this improvement.

### 2026-09-18 — Persisted notebook panel widths

- Notebook settings now store terminal_width alongside sidebar_width. Older
  notebooks default to 280 px; terminal widths retain the 160 px minimum and no
  fixed maximum. Without an active notebook, terminal width remains session-only.
- Completed divider gestures use the ordered settings writer and existing close
  flush. Notebook activation restores both widths and replays only explicit,
  correctly scoped choices. Viewport compression and panel toggles do not write
  projected widths back to settings.
- Headless coverage drags both dividers, closes with a settings write pending,
  restores a fresh application from disk and switches notebooks with distinct
  widths. Storage defaults/normalization, reload replay, stale scope, narrow
  windows and independent divider behavior also pass.
- Locked offline check, strict all-target Clippy and formatting passed. All 100
  library tests, 349 application tests, integration suites and doc-tests passed
  across the full runs and focused completion. The Mermaid renderer passed after creating its output
  directory in the validation snapshot. The known ConPTY sandbox failure passed
  in a headless outside-sandbox terminal-test rerun.
- Validation used target/panel-width-source and target/panel-width-build to
  isolate concurrent source edits and Windows linker conflicts in shared outputs.
  No desktop interaction or native application harness was used. Unrelated
  terminal-font work and the existing Cargo.lock changes are excluded.

### 2026-09-18 — Independent terminal font and size settings

- Added a searchable Terminal font picker and a 10–40 px size stepper in
  Settings → Terminal. Font and size independently inherit editor typography
  until overridden; the follow-editor choice and Use editor size reset them.
- Optional terminal.font_family and terminal.font_size fields preserve old
  notebooks and standalone session scope. Size values clamp on deserialization;
  unavailable font names stay saved while rendering uses the editor fallback.
- Existing guarded intents and SettingsWriter persist changes. All retained
  terminal sessions update in place; pending launches retain their latest full
  configuration, and font changes use the existing grid/PTY resize callback.
- Added headless coverage for persistence, bounds, resets, missing fonts,
  case-insensitive resolution, notebook switching, busy/stale forms, narrow
  layout, multiple/hidden sessions, pending launches, output preservation and
  grid/PTY resizing. All 26 focused application tests and 12 terminal integration
  tests passed, including the real PowerShell and Git Bash PTY tests.
- Locked offline check, formatting and strict all-target diagnostics Clippy
  passed. Validation used target/terminal-settings-validation after the shared
  default target encountered locked executables and stale artifacts. No desktop
  interaction or visible application launch was used; Cargo.lock is excluded.
- The serial full run passed all 100 library tests, 352 application tests and
  all other integration suites/doc-tests. The existing Windows PTY interrupt
  test timed out in the sandbox; rerunning all 12 terminal integration tests
  outside the sandbox passed without code changes.

### 2026-09-19 — Palette arrow-key wraparound

- Shared palette selection now wraps Up from the first filtered result to the last,
  and Down from the last to the first. Commands, Go To, Change Notebook and Sort
  Files retain query focus and use the existing selected-row scroll handling.
- Added regression coverage for both boundaries, ordinary movement, filtered,
  empty and singleton results, plus TestPlatform arrow-key dispatch in all modes.
- All 18 command-model tests passed; the focused palette run passed 3 library and
  22 application tests. Locked offline check, strict all-target diagnostics Clippy
  and formatting of the three changed Rust files passed.
- Full-suite validation was attempted but Windows LNK1104 executable locks blocked
  linking. Repository-wide formatting reported concurrent unrelated session and
  notebook edits. No desktop input or native application harness was used.

### 2026-09-19 — Notebook-scoped document tabs

- TabSet now retains separate canonical-notebook and standalone groups, each with
  tab order, selection and strip scrolling. Rendering and select/close controls
  only expose the current group; stable IDs and global traversal continue to
  serve shared documents, background saves, window close and move/delete.
- Accepted notebook activation restores existing editors and navigation instead
  of appending a last-file view. Only first activation loads the saved last file.
  Failed/stale loads preserve the current group, and removing notebook index
  entries parks their tabs without discarding drafts.
- Added eight TestPlatform regressions for repeated/same-notebook activation,
  editor identity, selection, modes, undo/redo, history, empty tabs, scroll
  restoration, unreadable last files, stale results, removal/standalone restore,
  hidden close/save decisions and shared move/delete. Updated earlier tests
  that intentionally relied on cross-notebook tab selection or last-file reload.
- All 101 library and 361 application tests passed across the full and focused
  runs, as did every integration suite and doc-tests. The full run encountered
  the previously documented Windows PTY interrupt timeout; all 12 terminal
  integration tests passed in a headless outside-sandbox rerun. Transient Windows
  executable linker locks also cleared on retry, without source workarounds.
- Locked offline check, strict all-target diagnostics Clippy, formatting and
  diff whitespace checks passed. No desktop input or native application harness
  was used. Existing autosave policy, terminal tabs and settings schemas remain
  unchanged.

### 2026-09-19 — Terminal selection, copy and paste

- Added visible-grid drag selection using Alacritty text extraction and shared
  paint/hit-test geometry. Copy preserves wrapped and Unicode text, writes plain
  text through GPUI and clears the selection; exited output remains selectable.
- Windows/Linux Ctrl+C copies a selection or interrupts, Ctrl+V pastes, and
  Ctrl+Shift+C/V remain explicit alternatives. macOS uses Cmd+C/V. Existing paste
  normalization, bracketed-paste framing, input limits and errors are retained.
- TerminalSession owns a Copy/Paste popup, checking menu/view/generation identity
  and current host authority at execution. Hidden/blocked sessions discard menus;
  shell exit disables writes immediately without disabling selection or copy.
- Added three terminal integration regressions and an application TestPlatform
  regression for pointer selection, Unicode/wrapping, geometry/font changes,
  clipboard bytes, disabled/exited sessions, menus, stale targets and modal guards.
  Repeated synchronization of an exited session preserves an in-progress drag.
- Locked offline check, strict all-target diagnostics Clippy, formatting and
  whitespace checks passed. Across the full and focused runs, all 101 library
  tests, 362 application tests, integration suites and doc-tests passed; 27 terminal
  application tests were rerun after the final lifecycle adjustment.
- The full run encountered the documented Windows PTY interrupt sandbox timeout;
  all 15 terminal tests passed in a headless outside-sandbox rerun. Validation used
  target/agent-validation to avoid the running application's locked executable;
  a transient test-executable linker lock cleared on retry. No desktop input or
  native application harness was used. Native visual validation remains manual.

### 2026-09-19 — Terminal controls in workspace toolbars

- Moved the terminal opener from the title bar into Content right in the editor
  toolbar. It follows toolbar visibility, the existing terminal preference and
  icon sizing, and disappears while the panel is open. Removed title-bar space
  reserved for the terminal and moved its Settings row into Content right.
- Added the matching, fixed collapse button at the right edge of the terminal
  header, outside scrolling tabs. It remains available when toolbar controls are
  hidden. Both buttons use the existing guarded toggle and retain sessions/focus
  behavior. Long tab titles shrink to keep tab close and header controls reachable.
- Updated headless GPUI regressions for placement, narrow/wrapped layouts, all
  icon sizes, scrolling tabs, settings visibility, retained sessions and caption
  isolation. Updated Settings scrolling tests for the new final terminal row.
- Locked offline check, formatting, strict all-target diagnostics Clippy and
  whitespace checks passed. The serial diagnostics run passed all 101 library
  tests, 363 application tests, other integration suites and doc-tests. The Windows shell integration test hit
  the documented sandbox PTY timeout; all 15 terminal integration tests passed
  in a headless outside-sandbox rerun.
- Validation used target/agent-validation. No desktop input or native application
  harness was used; native visual validation remains manual.

### 2026-09-19 — Optional top-bar panel controls

- Restored the top-bar terminal button with an independent, default-off
  `title_bar_terminal` preference in Settings → Toolbar → Top bar. Retained the
  existing workspace terminal preference, session lifecycle and interaction guards.
- Both top-bar panel buttons now default off; the sidebar toolbar button defaults
  on. Missing settings and reset use these defaults; explicit saved values survive.
- Headless application validation passed all 364 tests, including title-bar
  geometry, retained terminal toggles, modal/busy guards, settings persistence,
  reset and stale-form regressions. Native desktop interaction was not used.

### 2026-09-19 — Sidebar opener follows collapsed state

- The enabled sidebar toolbar button moves to the editor toolbar's fixed left
  edge before navigation when an active notebook sidebar is collapsed. Reopening
  restores its saved sidebar order and Explorer/Search focus without losing the
  query or results. Toolbar visibility and icon sizing still apply.
- Shared effective item selection drives region visibility, width and rendering,
  including when the sidebar opener is the only enabled editor control. Collapsed
  openers cannot start or accept reordering; stale sidebar drags are rejected.
- Headless regressions cover hidden/disabled controls, missing notebooks, an
  otherwise empty editor toolbar, narrow/wide layouts at every icon size, saved
  order, drag rejection and retained search state. Settings help and ownership
  documentation describe the new placement.
- Locked offline check, format check, strict all-target diagnostics Clippy and
  whitespace checks passed. Validation passed 101 library tests, 366 application
  tests, every integration suite and doc-tests. The known Windows PTY sandbox
  timeout affected one terminal integration test; all 15 terminal tests passed
  on the headless outside-sandbox retry, and the remaining suites passed.
- Validation used target/agent-validation. No native application harness or
  desktop input was used; native visual validation remains manual.

### 2026-09-19 — Fixed toolbar icon order

- Removed drag-and-drop reordering from workspace toolbar icons and Settings,
  including grips, previews, insertion targets, move intents and the pure mutation.
  Both surfaces now use the built-in order; visibility switches, icon sizes,
  reset, command clicks and panel-dependent placement retain their behavior.
- Removed the persisted order preference. Legacy order values are ignored without
  a migration write and omitted on the next settings save; other preferences survive.
- Headless regressions cover fixed order after loading legacy preferences, absent
  drag operations, row switches, disabled commands, wrapping at every size,
  sidebar collapse/reopen placement and settings serialization.
- Locked offline check, format check, strict all-target diagnostics Clippy and
  whitespace checks passed. Validation passed 101 library tests, 358 application
  tests, all integration suites and doc-tests. An initial concurrent link failure
  cleared with a single-job build. The known Windows PTY sandbox timeout affected
  one terminal test; all 15 terminal tests passed outside the sandbox, and the
  remaining integration suites and doc-tests passed.
- Validation used target/agent-validation and GPUI TestPlatform. No desktop input
  or native application harness was used; native visual validation remains manual.

## 2026-09-22 — Notebook zoom controls

- Added View → Zoom, three palette commands, and Mod+Plus/Equals, Mod+Minus and
  Mod+0 (Cmd on macOS). Shifted Plus aliases are accepted; terminal focus reserves
  these shortcuts for the application without writing shell bytes. All triggers
  share busy/modal guards and reset only the zoom percentage.
- Notebook appearance persists zoom_percent with a 100% compatibility default,
  50–200% limits and ten-point steps. Standalone use keeps a separate session
  value. Theme/rem sizes, document/terminal typography, explicit control geometry,
  menus and dialogs follow zoom. Base font settings and panel widths stay intact;
  divider gestures save unscaled widths. Retained documents and shells survive.
- Added five TestPlatform scenarios plus preference round-trip/boundary coverage;
  extended the command capability matrix and shortcut mapping checks. Tests cover
  all triggers, terminal focus/input isolation, palette dismissal, modal/busy
  rejection, notebook persistence, view identity/text/selection, toolbar scaling,
  viewport constraints and panel resizing at non-default zoom.
- Validation: cargo check --locked --offline, cargo fmt --check, and strict
  all-target Clippy with editor-diagnostics passed. The serial locked full test
  run passed 107 library and 366 application tests plus the integration suites
  before the documented Windows PTY interrupt sandbox timeout. The other 14
  terminal integration tests passed; the single failing PowerShell test passed
  outside the sandbox. The remaining theme-icons/toolbar-settings suites and
  doc-tests passed separately.
- No native application windows or desktop input were used. Physical keyboard,
  display-DPI transitions and visual inspection remain manual validation.

## 2026-09-22 — Mermaid sequence participant names with spaces

- Root Cargo patch now selects vendored merman-core 0.7.0. The only upstream
  source change is in the sequence lexer: internal whitespace stays inside
  participant IDs; declaration aliases, numeric IDs, comments and central
  connection markers retain their boundaries. Provenance and original licenses
  are included in vendor/merman-core, alongside MITHRIL-PATCH.md.
- The shared renderer accepts existing diagrams without modifying document
  source, selection or history. All new regression inputs use synthetic names
  and messages. No user screenshot or original diagram text is committed.
- Passed all 32 parser tests (28 upstream cases plus four local regression
  cases), and all five headless Mermaid renderer/Live/Reading integration tests.
  The fixtures cover spaces, tabs, Unicode, CRLF, aliases, notes, activation,
  comments, statement separators, malformed input and source/copy/history
  behavior. Renderer diagnostics exercise the longer synthetic diagram in both
  light and dark themes and retain resource/size/queue checks.
- cargo check --locked --offline, cargo fmt --check and strict all-target Clippy
  with editor-diagnostics passed using target/agent-validation. The vendored
  lexer passed rustfmt; other upstream source files remain byte-for-byte intact.
- Validation used GPUI TestPlatform without desktop input or native app windows.
  The open application's executable was left untouched after an initial Windows
  file lock; the temporary initial reproduction executable and symbols were
  removed. The independent validation directory avoids the running executable.
- The serial integrated run passed 107 library tests, 366 application tests and
  229 integration tests before the known Windows PowerShell PTY interrupt timeout.
  The other 14 terminal tests passed, and the single sandbox-sensitive test
  passed on an isolated headless retry outside the sandbox. An initial parallel
  build's transient msvcrt.lib linker error cleared with one build job.
- After preserving Unicode boundary whitespace in the final lexer adjustment,
  all 37 Mermaid/parser checks passed again. No desktop testing was performed.
- The remaining two theme-icon and seven toolbar-settings tests, doc-tests,
  formatting checks and final strict all-target Clippy also passed.

## Local one-command Windows releases — 2026-09-22

- Added `release.ps1 -Tag vMAJOR.MINOR.PATCH` and the Node orchestrator in
  `scripts/release-local.mjs`. It archives the selected local tag, validates its
  Cargo version, runs headless tests, builds with the pinned Rust toolchain,
  packages the installer/portable ZIP/checksums, and reuses the existing verified
  draft publisher for `travisluong/mithril-releases`.
- Builds leave the development checkout untouched and retain source snapshots,
  release metadata and downloads under ignored `target/local-releases`. A local
  lock serializes access to the separate build cache. No source tags are pushed.
  `-BuildOnly` supports local builds without GitHub access. Publishing-token environment
  variables are excluded from compilation/test subprocesses.
- Made the hosted release workflow manual-only and removed its publish job.
  The separate branch/PR validation workflow remains automatic; the setup guide
  calls out its potential charges and the behavior of tags with older workflows.
- Passed all 23 release automation checks, including real Git tag snapshots and
  dirty-checkout preservation, build/package/auth failure handling, lock behavior,
  and the existing draft/upload/download verification regression tests. GitHub
  and Rust compilation were mocked; no release was published.
- The optional smoke check used Inno Setup 6.7.3 and real Windows PowerShell ZIP
  packaging against a synthetic executable, then checked installer headers and
  the ZIP's exact filename/content. This caught a `Compress-Archive` module-load
  failure; ZIP creation now uses the built-in .NET compression API and passes.
- `actionlint` passed for the manual release workflow. PowerShell/Node parsing,
  the wrapper's nonzero exit for an invalid tag, and `git diff --check` passed.
  No application code changed, no full Mithril release build was run, and no
  installer or application UI was launched. Real publishing credentials and
  manual install/upgrade checks remain operator setup.

## Git Bash release entry point — 2026-09-22

- Added `release.sh vMAJOR.MINOR.PATCH` as a thin wrapper over the existing Node
  release command. Remaining options (including `--build-only` and custom Inno
  Setup paths) and Node's exit status pass through unchanged. Script-relative
  path resolution uses Bash builtins and handles spaces and Windows separators.
- Added an LF checkout rule and executable Git mode for the wrapper, and
  documented Git Bash publishing and build-only usage.
- Passed Bash syntax and LF checks plus a temporary stub-based check of argument
  forwarding, paths with spaces, invocation from another directory, exit-code
  preservation, missing-tag usage and help. A real invocation rejected an invalid
  tag through the Node entry point with exit code 1. No build, upload or desktop
  interaction occurred; the existing release implementation was unchanged.

## 2026-09-23 — Mermaid arrow-label background alignment

- Corrected the shared renderer's pinned Merman 0.7.0 SVG output before
  rasterization. Flowchart edge labels emitted centered text beside a left-aligned
  background inside a left-shifted group. Text and background now share the edge
  anchor in Reading and Live preview; other SVG structures are unchanged.
- Added real-renderer regression checks for Yes/No, longer and multiline labels,
  TD/LR directions, both themes and repeated normalization. All five headless
  Mermaid integration tests pass via `cargo test --locked --offline --features
  editor-diagnostics --test mermaid --target-dir target/headless -j 2`. Inspected
  generated light/dark arrow-label raster images without desktop interaction.
- `cargo check --locked --offline`, explicit vendor rustfmt and task-scoped
  `git diff --check` pass. Full tests, root formatting and all-target Clippy were
  attempted but blocked by missing `src/feedback/tests.rs` and/or
  `src/feedback_ui/tests.rs` in concurrent, unrelated feedback-feature edits.
  The ordinary target also encountered an in-use `target/debug/mithril.exe`;
  focused tests passed in the separate headless target without touching that app.

## 2026-09-23 — Feedback and website capture

- Added the pure `feedback` owner for payload validation, bounded JSON, UUID retry
  identity and injectable transport. Production uses the fixed HTTPS endpoint;
  debug/test-only overrides permit loopback HTTP. Requests run off the UI thread
  with a 15-second total timeout, no redirects and bounded JSON acknowledgment.
- Added Help → Send feedback and an app-session modal adapter. Message/contact
  inputs and the in-flight task survive close/reopen; acknowledged success alone
  clears them. Failures preserve the draft and unchanged retry UUID. Completion
  never reopens the modal or steals focus. Root captures workspace focus before
  opening, covering Cancel as well as normal dismissal. Autosave and document
  text, selection, dirty state and identity remain independent.
- Pure tests cover Unicode/byte/email bounds, request metadata, identity/retries,
  acknowledgment, errors, timeout and redirect handling. Three GPUI TestPlatform
  tests exercise actual Help-menu keyboard access, anonymous/contact submissions,
  duplicate protection, close/reopen during send, all retryable errors, focus,
  validation and autosave while the modal remains open.
- `cargo fmt --check`, `cargo check --locked --offline`, and strict all-target
  Clippy with `editor-diagnostics` passed. The ordinary all-tests command could
  not replace the user's running `target/debug/mithril.exe`; it was left untouched.
  Full regression used `--target x86_64-pc-windows-msvc --no-fail-fast --
  --test-threads=1`. Its 731 passing tests plus the one PowerShell-interrupt test
  rerun outside the sandbox cover all 732 regression tests. All 15 terminal tests
  passed in that unsandboxed, noninteractive PTY rerun.
- The one ignored cross-repository library test was run separately by the adjacent
  Next.js harness. It sent the same UUID twice through the real desktop transport
  to local Next.js and verified exactly one matching Postgres row. Both real
  website signup forms persisted/merged independent interests; feedback contact
  did not subscribe the sender. No submitted email/message appeared in server logs.
- Website ESLint, TypeScript, production Webpack build, eight API/Postgres tests
  and 13 headless browser checks passed. The official Postgres 18 container passed
  fresh/repeated migrations, concurrent signup deduplication, feedback idempotency,
  shared limits/expiry and container recreation with preserved data. Test container
  and volume were removed afterward. See `mithril-web/docs/verification.md` for reproducible commands and schema.
- [Feedback contracts and launch handoff](feedback.md) document backend-first
  publication, stored-row checks and an owner review four weeks after the actual
  public launch date. No services were provisioned for production, releases
  published, outreach sent, automation created, or native desktop input injected.

## 2026-09-24: Remove GitHub Actions workflows

- Removed the hosted Windows release build and cross-platform validation workflows.
  Builds, validation and release publishing now use the documented local commands.
- Updated the architecture map, README and Windows packaging guide. Retained
  `.github/scripts/release.mjs` and its tests because the local release command
  imports its validation, checksum and publishing helpers.
- `node --test .github/scripts/release.test.mjs scripts/release-local.test.mjs`:
  22 passed, 1 optional real-installer-compiler test skipped, 0 failed.
- `git diff --check` passed; no workflow files or current documentation links to
  removed workflows remain. Historical audit/progress entries are preserved.

## 2026-09-24: Persist window placement

- Added app-owned versioned `window-state.json` beside `notebooks.json`, separate
  from notebook preferences. It records normal bounds in logical pixels and
  maximization, with validated finite dimensions and negative monitor positions.
  Startup fits bounds onto the display with the greatest overlap (primary when
  disconnected), retains the 400 x 300 minimum and centered 900 x 700 fallback,
  and passes the selected display ID so Windows accepts secondary-monitor bounds.
- Accepted Close captures GPUI's restore bounds, starts an atomic background
  replacement, and awaits both that write and terminal cleanup before removal.
  Cancelled close does not capture or write state; persistence errors log and
  still allow close. Windows reads the minimized restore-to-maximized flag;
  minimized windows never reopen minimized and fullscreen restores normally.
  Diagnostic runs bypass personal reads/writes; headless fixtures default to no
  persistence and explicitly inject temporary paths when testing it.
- Seven focused tests cover JSON round trips and replacement, missing/corrupt
  files and unsupported versions, invalid geometry, negative positions, monitor
  removal, minimum size, normal/maximized/fullscreen/minimized policy, startup
  defaults, diagnostic bypass, cancellation, accepted-close write completion at
  window removal, standalone close and write failure. GPUI TestPlatform does not
  implement native maximization; restoration tests inspect the produced bounds.
- `cargo check --locked --offline`, `cargo fmt --check`, strict all-target Clippy
  with `editor-diagnostics`, and `git diff --check` pass.
- Full regression used `cargo test --locked --offline --features
  editor-diagnostics --target x86_64-pc-windows-msvc --no-fail-fast --
  --test-threads=1` to keep the running app's normal build output untouched:
  739 passed, one existing cross-repository test ignored, and the documented
  sandbox-sensitive PowerShell interrupt test failed. That one noninteractive
  PTY test passed on rerun outside the sandbox, covering all 740 regression tests.
  The seven placement tests also passed again after final display-selection edits.
- No desktop windows were launched or controlled. Native Windows close/relaunch,
  minimized-from-maximized, fullscreen, taskbar/DPI and disconnected-monitor
  visual validation remain manual checks for the user.

## Production v0.1.1 release verification

- Published Windows x86-64 v0.1.1 from source commit
  `f8c2f560dee096a3c81d2a0f2323a49152b8204f`. The source commit and
  annotated tag were pushed to the source repository.
- The recorded desktop test run used `editor-diagnostics`; one existing
  cross-repository test remained ignored.
- Fresh release snapshots exposed a second test setup assumption: Mermaid PNG
  diagnostics wrote into a local `target` folder that external Cargo build caches
  do not create. The test now creates its own output directory.
- All 23 release-tooling checks passed, including actual noninteractive Inno Setup
  and ZIP packaging. Windows native tar was selected explicitly. Transient Windows
  linker file-access failures were resolved by retrying the same source snapshot;
  no test failure was bypassed. The completed snapshot passed the entire suite
  before the optimized build and packaging.
- The installer, portable ZIP and SHA256SUMS.txt were uploaded, downloaded and
  byte-verified before publishing at
  https://github.com/travisluong/mithril-releases/releases/tag/v0.1.1.
  Installer signing, Microsoft Store distribution and desktop UI validation remain
  outside this release. No desktop application was launched for testing.
