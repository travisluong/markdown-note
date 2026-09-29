# Architecture audit and improvement plan

Reviewed 2026-09-12 against commit `c852b70` with a clean initial worktree.
This is an audit and implementation plan; runtime code is unchanged by this
documentation work. [ARCHITECTURE.md](../ARCHITECTURE.md) maps the current code
and gives future agents an entry point. The target layout below is proposed,
not an existing API.

## Assessment

Retain the current single-process, file-based application and vendored editor.
The strongest existing decisions are the shared rope/history across editable
views, immutable save snapshots, atomic replacement, guarded background results,
incremental viewport work, and meaningful Unicode/parser-equivalence tests.
Replacing these foundations would create more risk than it removes.

The largest architectural weakness is **implicit ownership**. `Notes` in
`src/main.rs:174` owns application, editor, settings-form, navigation and task
state, while eleven files extend it. `notebook_ui.rs` (1,521 lines), `notebook.rs`
(1,499), `main.rs` (1,008), `goto.rs` (936), and `settings_ui.rs` (809) mix several
responsibilities; those sizes include tests and diagnostics. The problem is not
the line count itself. A feature change requires finding unrelated setters,
completion handlers and invariants across these files.

The recommended direction is a **modular application with explicit owners and
testable transitions**, inside the existing crate initially. Separate code by
the decisions it owns, then make GPUI views translate user events into those
decisions. Avoid a rewrite, a general event bus, a plugin framework or new crates
until an actual boundary requires them. Fix the reproduced redo defect first.

## Verification baseline

Windows/MSVC, Rust 1.98.1 and Cargo 1.98.1; cached locked dependencies were used.

| Command | Result |
| --- | --- |
| `cargo check --locked --offline` | Passed for the default application configuration. |
| `cargo test --locked --offline --features editor-diagnostics` | Existing suite passed: 144 tests, zero failures or ignored tests. |
| Targeted temporary TestPlatform redo probe | Two tests failed, reproducing F1 in both Source and Live preview. Probe removed after execution. |
| `cargo fmt --check` | Failed on existing formatting differences in application and included vendor source. |
| `cargo clippy --locked --offline --all-targets --features editor-diagnostics -- -D warnings` | Failed on existing lint violations, including collapsible conditionals, unnecessary casts/unwraps, and items after test modules. This strict gate is a proposed quality target, not an existing repository requirement. |

Existing test counts: application 59; code highlighting 2; editor 63; image
editor 5; images 3; Reading search 3; tables 9. Cargo also reported a future Rust
compatibility warning for `proc-macro-error2 2.0.1`.

No application window, desktop input, native picker, performance harness, release
build, Linux/macOS execution or external dependency-advisory scan was performed.
Passing existing tests does not establish coverage of all lifecycle interleavings.
Findings distinguish reproduced behavior, code-traced behavior and design risks.
Historical benchmark numbers were not revalidated.

## Findings, in repair/review order

### F1 — P1: redo overwrites a newer divergent edit

**Reproduced with the actual InputState on headless TestPlatform, in both
Source and Live preview.** Start with `a`; isolated replace with `b`; Undo
returns `a`; isolated replace with `c`; Redo produces `b`, overwriting `c`.
Both probes asserted that Redo should leave the divergent edit `c` unchanged
and failed with actual `b` versus expected `c`.

[`History::push`](../vendor/gpui-component/src/history.rs#L99) retains redo
entries. [`InputState::push_history`](../vendor/gpui-component/src/input/state.rs#L1908)
delegates to it, and `redo` at line 1931 replays old ranges. Generic History has
a test intentionally retaining redo across pushes, so changing its global
semantics could affect unrelated components. Define editor transaction policy
explicitly and invalidate abandoned redo branches for divergent document edits.
Cover ordinary typing, isolated commands, undo/redo and IME grouping. Keep this
repair separate from architecture extraction.

Probe command: `cargo test --locked --offline --features editor-diagnostics
--test architecture_audit_redo`. Result: 0 passed, 2 failed. Its temporary test
file was removed; a permanent regression should accompany the fix.

### F2 — P2: cancelled navigation can leave autosave suspended

**Code-traced defect.** [`request`](../src/main.rs#L386) cancels the autosave
timer at line 390. Edit a note, choose File/Open, choose Discard, then cancel the
file picker. The dirty old document is retained, but cancellation at
`main.rs:492` does not schedule autosave again. A separate path sets `busy` while
the notebook folder picker is open (`notebook_ui.rs:544`); if the one-shot timer
expires then, `autosave.rs:64` declines to save, and cancelling the picker does
not rearm it. Another edit or manual save recovers.

Introduce one retained-document completion path and test timer behavior after
cancel, failed open and failed notebook activation. Do not fix just one picker
callback while leaving equivalent exits inconsistent.

### F3 — P2: search revalidation can ignore an exact surviving hit

**Code-traced defect with a deterministic fixture.**
[`revalidate`](../src/search.rs#L196) uses `ranges`, which returns only the first
2,001 matches (`search.rs:37`). That display/scan cap is inappropriate when
finding the nearest occurrence in a changed document.

Original: 10,000 spaces followed by `hit`, with a result at `10000..10003`.
Changed: `"hit ".repeat(2002)`, then 1,992 spaces, then `hit`. The original
occurrence is still at exactly the same offset, but capped revalidation chooses
`8000..8003`. Add this regression and use an uncapped streaming nearest-match
lookup, retaining the notebook result cap for collection.

### F4 — P2: image insertion can split inline Markdown syntax

**Code-traced defect.** [`image_import::insertion`](../src/image_import.rs#L138)
moves drops outside non-paragraph blocks but allows insertion inside a paragraph,
then inserts blank lines. At offset 10 in `hello **world**`, inserting
`![a](a.png)` separates the emphasis delimiters across paragraphs:
`hello **wo\n\n![a](a.png)\n\nrld**`. The app calls this helper from
`images.rs:119`; current insertion tests omit formatted inline spans.

Choose a safe inline/paragraph boundary before importing files. Add emphasis,
link, code-span and Unicode examples, and retain the current rollback ownership
and isolated undo behavior.

### F5 — P2: discarded search overlays have no lifecycle invalidation

**Code-traced consistency gap within an intentional UI policy.**
[`schedule_search`](../src/search_ui.rs#L108) captures an unsaved document
snapshot. Publication checks query cancellation at line 125, while
[`install_document`](../src/notebook_ui.rs#L694) neither cancels nor refreshes
that overlay. With autosave disabled, search text present only in unsaved A,
discard A while opening B, and A's discarded content can remain in results or
arrive from an in-flight scan.

README intentionally promises a stable result list while navigating. Preserve
ordinary result navigation, selection and scroll; explicitly retire or refresh
the discarded overlay. Test both an already published result and a delayed scan.

### F6 — P2: cross-feature state ownership is not enforced

**Architecture risk with concrete coupling.** `Notes.busy` represents prompts,
pickers, loading and foreground saves. Background autosave starts with
`busy=false`, yet completion clears it unconditionally
([autosave.rs:100](../src/autosave.rs#L100), line 108). An unrelated operation can
own the flag by completion time; specific interleavings need tests.
Link editing also asks `can_insert_date()` (`links.rs:114`, line 179), whose
implementation lives in `settings_ui.rs:65`. Its meaning is general editability,
not date insertion.

Give document/session operations a named state and identity. Let a completion
release only its own operation. Centralize semantic capability queries such as
`can_edit_document`, and move form state out of the root. Merely relocating
`impl Notes` blocks would leave the underlying coupling intact.

### F7 — P2: validation and documentation do not define a reliable baseline

**Observed checks and repository structure.** Formatting and strict lint checks
fail. The sole workflow at the time, `build.yml` (removed later), builds
release binaries on release publication/manual dispatch; it does not run tests
or validate pull requests. Preserve those release trigger choices and add a
separate validation workflow.

Important editor transaction checks, including
[`diagnostic_table_checks`](../vendor/gpui-component/src/input/table_edit.rs#L278),
are called from `src/benchmark.rs:31`, outside the headless test suite. Existing
TestPlatform tests show that a noninteractive integration route is available.

README says there is no New or autosave at line 97, then documents them at lines
202 and 213. Early sidebar and shortcut descriptions also predate images and
quick open. `EDITOR-PATCH.md:127` says parsing stays in the background, while F8
shows an initial exception. Correct these statements rather than appending
another competing description. Keep current architecture, user help, historical
benchmark reports and proposed work clearly separated.

### F8 — P2: Reading initialization and repeated full parses need attention

**Verified execution structure; latency impact unmeasured.**
[`TextView::request_layout`](../vendor/gpui-component/src/text/text_view.rs#L866)
calls `parse_content` synchronously on initial state. Later updates are sent to
background work. `main.rs:638` keys Reading state per document, so first visits
exercise the synchronous path.

Separately, [`preview_syntax::Request::parse`](../vendor/gpui-component/src/input/preview_syntax.rs#L34)
invokes link, table, fenced-code and image analysis independently. On mixed
documents each may run a full GFM parse (`markdown_links.rs:102`, `table.rs:97`,
`fenced_code.rs:174`, `markdown_images.rs:10`). Simple documents benefit from
feature guards and existing fast paths.

Move initial Reading analysis off layout before attempting a parser redesign.
Then investigate a shared immutable AST with feature visitors using measured
mixed-document workloads. Preserve the one-running/one-pending preview queue,
safe-rebase rules, exact code revisions and existing incremental equivalence
tests. Do not replace warm incremental editing with a full parse per edit.

### F9 — P3: settings writes accumulate every intermediate snapshot

**Unbounded queue in source; no backlog measured.**
[`persist_notebooks`](../src/notebook_ui.rs#L826) captures full settings/index
snapshots and awaits the previous task. Repeated settings changes can build a
FIFO chain; switch and close drain it. Theme picker events are one producer.
Sidebar resize persists on mouse-up, not on every drag motion.

Use one writer and a latest pending settings snapshot per destination/notebook,
with explicit flush and error results. Coalescing must retain updates to every
notebook and order the index correctly across switches. Do not replace the queue
with a single global pending value that loses the previous notebook's settings.

### F10 — file conflicts and metadata need an explicit policy

**Data-integrity design gap; platform behavior not executed.**
[`Document`](../src/main.rs#L104) records path, text and encoding, but no disk
version. [`save_file`](../src/main.rs#L156) unconditionally replaces the path.
An external edit made after load can therefore be overwritten by autosave.
This is separate from the documented absence of file watching.

Specify conflict behavior before changing persistence: detect a changed on-disk
baseline, retain the dirty buffer, and report a conflict instead of silently
overwriting it. A metadata check alone is not a race-free compare-and-swap;
document its limits and choose appropriate content/version checks.

The save path also creates a default tempfile without copying existing file
permissions. The pinned tempfile Unix implementation defaults to mode `0600`
and replaces via rename, so an existing `0644` note is expected to change mode.
Add Linux/macOS permission tests and choose which metadata to preserve. Do not
claim full ACL, ownership, hard-link or crash-durability preservation from atomic
replacement alone. Schedule this policy work early because autosave is on by
default; it is not a request to add a file watcher.

## Proposed target responsibilities

Start as modules in the existing crate, with `lib.rs` exposing only the API
needed by the binary and integration tests. Paths are a design sketch, not a
bulk move checklist.

```text
src/
  main.rs                   startup, window creation, dependency wiring
  lib.rs                    module entry points for application/tests
  document/                 format, saved-file facts, source identity/revision
  notebook/                 settings/schema, path policy, tree and file operations
  app/
    session.rs              document/save/navigation transitions and effects
    capabilities.rs         command availability from explicit state
    settings_writer.rs      ordered, coalesced persistence and flush outcomes
  editor/                   narrow adapter around the vendored InputState APIs
  search/                   matcher, scanner, request lifecycle, result navigation
  commands/                 catalogue, ranking and dispatch
  ui/                       shell, explorer, settings, search and palette views
  assets/                   link routing, image import and resource integration
vendor/gpui-component/      generic editor analysis, transactions and presentation
```

Dependencies should flow from UI to application operations to document/notebook
logic and storage. Pure policies must not import `Notes`, `Window`, `Context` or
GPUI entities. GPUI adapters own entities, subscriptions, focus and task handles.
Storage accepts explicit paths/snapshots and returns results without updating UI.
The session model tracks identities/revisions and decisions; it need not own
or duplicate a string buffer. Keep cheap vendor rope snapshots inside the
editor/save adapters rather than introducing per-keystroke string copying.

Use small explicit types where confusion already exists: `DocumentId`,
`Revision`, and source-byte versus IME/display offsets. Represent the active
content as empty/document/image when extracting its owner, instead of preserving
independent optional fields that allow contradictory combinations. Introduce
interfaces for actual testing/ownership needs, not a trait for every function.

## Implementation sequence and acceptance criteria

Each numbered item is a milestone containing independently reviewable commits.
Keep each behavioral fix separate, and keep mechanical extraction separate from
behavior changes. Preserve storage compatibility, Cargo pins and the vendor
patch throughout.

1. **Repair divergent redo first.** Add permanent versions of the two F1 probes,
   then fix editor-specific redo invalidation. Extend coverage to ordinary
   typing, command edits and IME/history grouping. **Done when:** a new edit after
   undo cannot replay the abandoned branch, ordinary undo/redo still works, and
   generic History consumers keep their intended semantics.
2. **Establish a dependable review baseline.** Correct contradictory README and
   patch statements; organize user documentation by current feature. Perform
   formatting in a dedicated commit and resolve or narrowly justify Clippy
   findings without broad suppression. Add a separate pull-request validation
   workflow and record an agreed Rust toolchain. **Done when:** default check,
   headless tests, format and the chosen lint gate pass on a clean checkout;
   release workflow behavior is unchanged. Run checks on Windows, Linux and
   macOS with necessary platform dependencies; do not infer test portability
   from the existing release build matrix.
3. **Lock down and fix lifecycle/edit defects.** Add deterministic regression
   tests before fixes for F2-F5 and operation ownership in F6. Bring the
   existing table/history/save-navigation diagnostics onto TestPlatform or a
   pure transition harness. **Done when:** delayed completion, failure,
   cancellation and discarded overlays preserve text, history and search
   behavior; each fix has its own focused test and commit.
4. **Extract stable data and storage boundaries.** Move document format/I/O out
   of `main.rs`; split notebook settings/schema from filesystem operations;
   move `ViewMode` out of the executable root. Add a library entry point and
   explicit imports. Address F10's conflict/metadata policy in separate behavior
   commits with disposable-file/platform tests. **Done when:** integration tests
   can exercise these APIs without importing `main.rs`; BOM/newline, failed
   replacement, legacy settings and link/path tests remain green.
5. **Give the document session one transition owner.** Extract save/navigation
   decisions into state plus explicit effects: begin write, prompt, open, install,
   notify dependents and resume autosave. Separate foreground-operation identity
   from background save progress. Give search an explicit discarded-overlay
   signal. **Done when:** every completion checks its owner; transition tests
   cover edit-during-save, queued navigation, cancel/failure, stale completion,
   move and close. No feature writes session internals directly.
6. **Bound settings persistence.** Implement F9 after session/persistence
   ownership is clear. **Done when:** stress tests show bounded pending work per
   destination, latest settings for two switched notebooks both reach disk, and
   switch/close await a defined flush outcome including failures.
7. **Extract UI and command owners.** Move settings-form entities/subscriptions,
   explorer state and palette/search controllers behind explicit APIs. Separate
   command metadata/ranking from rendering and use shared capability queries.
   **Done when:** a command or preference can be added through one documented
   owner and its tests; extracted modules do not import root state wholesale.
8. **Refine the editor boundary and analysis.** First expose the app's snapshot,
   isolated-edit, selection and link/task event needs through a thin adapter.
   Inventory local vendor modifications by subsystem and associated tests.
   Fix initial Reading parsing, then measure shared GFM analysis and split
   analysis/layout/input responsibilities as justified. **Done when:** cold
   parsing does not run in layout; parser-equivalence and stale-result tests
   pass; queue, parsed-byte and viewport-work bounds do not regress. Leave
   physical input and user-visible performance validation to the user.

Baseline improvements and focused regression tests can proceed together.
Milestone 4's data extraction and documentation can proceed while independent
fixes land; session and settings-writer changes should be ordered. Defer broad
vendor reorganization until transaction/lifecycle coverage is in place.

## Keeping future AI changes understandable

Keep [ARCHITECTURE.md](../ARCHITECTURE.md) as the current ownership map and this
dated audit as historical findings/plan. Record decisions only for real choices
such as conflict policy, snapshot validity and history semantics, including the
reason and protecting test. Put invariants beside the owning type as well as in
the map. Use consistent capability, identity and operation names.

A feature change should identify its owner, describe its state transition, name
its snapshot/coordinate conventions, and run the relevant headless regression
tests. Update the map when an owner moves. Review success by whether a future
agent can locate the entry point and prove the invariant through a focused test,
not by the number of files created or an arbitrary maximum file length.
