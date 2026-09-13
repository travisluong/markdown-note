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
