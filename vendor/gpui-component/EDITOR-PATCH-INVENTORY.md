# Local patch inventory

## Independent interface icon colors (2026-09-17)

| File | Local contract and verification |
| --- | --- |
| src/theme/mod.rs | Optional icon_color, defaulting to None for compatibility. |
| src/icon.rs | Opt-in themed color with explicit-color precedence, clone preservation and matching element/entity render paths. Opt-in diagnostics trace resolved colors for headless tests. |
| src/button/button.rs | Enabled neutral icons/carets use the override; disabled, primary, semantic and loading variants retain foreground inheritance. |
| src/menu/popup_menu.rs | Neutral icons, checks and submenu carets honor the override and retain disabled/explicit colors. |
| src/select.rs | Select arrows and empty-state icons honor the override without recoloring labels. |
| src/list/list.rs, src/list/delegate.rs, src/list/list_item.rs | Search, empty-state and selection-check icons use the override; existing muted colors remain fallbacks. |

Reconcile these changes on upstream upgrades. Render regressions live in
tests/theme_icons.rs; application toolbar and Settings tests cover palette
propagation, disabled formatting, edits, switching, reset and stale events.

## Dialog close-button pointer isolation

`src/dialog.rs` isolates close-control left-button presses from title dragging
and exposes the `dialog-close` debug selector. The headless Settings regression
`modal_settings_close_icon_restores_focus_after_drag_resize_and_reopen` covers
pointer dismissal, focus restoration, dragging/resizing, reopening and Escape.
Reconcile this event isolation on upstream upgrades.

## Title-bar overflow submenu builders

`src/menu/popup_menu.rs` accepts borrowed `FnOnce` builders in `submenu` and
`submenu_with_icon`, matching their immediate, single call through `PopupMenu::build`.
No builder is retained. This lets the application populate both ordinary menus
and title-bar overflow submenus from the same current Notes snapshot without
recursively borrowing its entity or copying command definitions. Parent-menu
focus and dismissal wiring remains owned by PopupMenu. Coverage:
`src/title_bar_ui/tests.rs`, including a keyboard-invoked overflow submenu command.
Reconcile this signature relaxation on upstream upgrades.

## Responsive settings modal placement

`src/dialog.rs` adds `DialogPlacement::with_margin` (default remains 16 logical
pixels) and `set_preferred_size`. Responsive size updates recenter an unmoved
dialog, preserve a dragged origin where possible, and cancel an in-progress drag
when dimensions change. Layout clamps size and position within the configured
margin. Settings opts into 24 pixels and 80% of the content viewport; other
dialogs retain their existing defaults. Reconcile these APIs and drag state on
upgrades. Coverage: `tests/dialog_placement.rs` and headless
`settings_ui::panel::tests::modal_settings_*` tests in the application.

This is the file-level companion to [EDITOR-PATCH.md](EDITOR-PATCH.md). Use it to
find the implementation and regression checks for a local editor feature before
extending it or reconciling a new upstream release. Paths in the inventory tables
are relative to `vendor/gpui-component`; test paths are relative to the repository
root. Test references identify coverage entry points, not a claim that every
behavior in a file has a dedicated test.

## Movable dialog extension (2026-09-15)

| File | Local contract and verification |
| --- | --- |
| `src/dialog.rs` | Optional `DialogPlacement` stores per-open preferred dimensions and drag position. Movable mode clamps placement/size, captures title drags and fills a caller-scrolled body without positional animation; ordinary dialogs retain existing behavior. `src/settings_ui/panel/tests.rs` verifies bounds, scrolling, drag/release, resize, section/open resets and preference retention on TestPlatform. Reconcile this opt-in extension when upgrading. |

## Baseline and completeness

The baseline is the locally cached crates.io **gpui-component 0.5.1 package**,
identified on 2026-09-12 by both its normalized `Cargo.toml` (
ame` and `version`)
and `.cargo_vcs_info.json`:

- Git SHA: `0f0ab35233212f8f3277028995caf0c41e13ee6c`.
- Git package path: `crates/ui`.
- Cached directory: `gpui-component-0.5.1` under Cargo's
  `registry/src/index.crates.io-1949cf8c6b5b557f/` directory.
- The vendored package has the same package identity and VCS metadata.

The comparison enumerated **all files, including hidden files**, in both trees.
It compared content and separated CRLF/LF-only differences from substantive
changes. Including this inventory, the cached tree has **211 files** and the
vendored tree has **225 files**: **22 substantively modified**, **14 added**
(12 Rust files and two patch documents), **2 differing only in line endings**,
**187 identical**, and **0 removed**. Every changed or added path appears below.
`.cargo-ok` is present and identical in both trees; it is not a local feature.

This establishes completeness against that cached **published package**, not
against every file in the upstream Git repository. The comparison is read-only;
the registry cache must never be patched. Repeat it when changing the vendor
tree, since this document is a snapshot rather than an automatically enforced
manifest.

`LICENSE-APACHE` is byte-identical in both trees. Its SHA-256 is
`28ebc47a24f04d01f5c2a55eed537375d71c65ef05f7c3739accca419bb2ea7e`.
The package retains `license = "Apache-2.0"` and the upstream repository metadata.
`Cargo.toml.orig`, the vendored `Cargo.lock`, `.cargo_vcs_info.json`, the upstream
README, and bundled language query files are unchanged. Keep the license and
both patch documents when upgrading. The application's root `Cargo.toml`
selects this copy using `[patch.crates-io]`.

## Regression entry points

The abbreviations in the tables refer to these repository test targets:

| Key | Entry point | What it exercises |
| --- | --- | --- |
| E | [`tests/editor.rs`](../../tests/editor.rs) | Directly includes the patched line index, live-preview parser, Markdown-link parser, preview queue, scroll helpers, wrapper, and fenced-code modules; also calls the find/replacement and production GFM work-count diagnostics with `editor-diagnostics`. |
| EA | [`src/editor/tests.rs`](../../src/editor/tests.rs) | Thin editor adapter transactions, snapshot/disabled guards, source selection, events and isolated history on TestPlatform. |
| H | [`tests/editor_history.rs`](../../tests/editor_history.rs) | Actual InputState divergent edits, typing/IME grouping, same-text replacement, Source/Live preview, and generic History compatibility. |
| TX | [`tests/editor_transactions.rs`](../../tests/editor_transactions.rs) | TestPlatform entry points for list/task/selection transactions and table command/navigation/input diagnostics. |
| T | [`tests/tables.rs`](../../tests/tables.rs) | GFM source ranges, command generation, escaping, incremental/full-parser equivalence, large-table edits, and immediate navigation metadata. |
| R | [`tests/reading_search.rs`](../../tests/reading_search.rs) | Direct source-map tests and the Reading renderer's Markdown/search/highlighting diagnostic. |
| RL | [`tests/reading_lifecycle.rs`](../../tests/reading_lifecycle.rs) | Controlled TestPlatform Reading workers: cold initialization, latest-only queue, stale successes/errors, style refresh, callback source identity, find/reveal, retained scroll, HTML switching and teardown. |
| I | [`tests/images.rs`](../../tests/images.rs) | Standalone-image recognition, document-relative URLs, and source-offset rebasing. |
| IE | [`tests/image_editor.rs`](../../tests/image_editor.rs) | TestPlatform image insertion, disabled/stale edits, history, loaded-image geometry, source hit mapping, and scroll stability. |
| C | [`tests/code_highlighting.rs`](../../tests/code_highlighting.rs) | TestPlatform fenced-code paint/style, theme changes, wrapping, pointer/selection reveal, source coordinates, and history. |

Use the root package to run these tests. Dependency-local `#[cfg(test)]` tests
are not automatically run by `cargo test` in the application; E and R deliberately
include selected dependency source modules. The application and component
`editor-diagnostics` features expose the additional diagnostic entry points.

The core headless command is:

```powershell
cargo test --locked --features editor-diagnostics
```

These TestPlatform checks do not open native windows or inject operating-system
input. Some older application diagnostics use a real GPUI application lifecycle;
do not launch them for automated validation under the repository's desktop-use
restriction. Gaps marked **static/manual** below require source review and, if
visual confirmation is needed, validation by the user. The performance harness
and its execution limits are documented in
[`benchmarks/README.md`](../../benchmarks/README.md).

## Modified upstream files

| File | Local responsibility | Regression entry points | Upstream reconciliation |
| --- | --- | --- | --- |
| `Cargo.toml` | Adds the `editor-diagnostics` feature and pinned URL dependency used by image resource resolution. | Compile the root package with and without its diagnostics feature; I, IE. | Carry feature/dependency requirements into the new normalized manifest. Do not mistake the unchanged `Cargo.toml.orig` for the effective local manifest. |
| `src/highlighter/highlighter.rs` | Exposes theme-independent capture names/ranges and shared highlight-style merging. | C, R; E's fenced-code tests. | Compare upstream capture/style APIs before retaining the small visibility/extraction changes. |
| `src/highlighter/mod.rs` | Exposes the local code-highlighting module internally, or as doc-hidden public API under diagnostics. | C, R; diagnostics and ordinary compilation. | Preserve feature visibility without publishing a broader stable API accidentally. |
| `src/history.rs` | Adds explicit redo-branch clearing while preserving generic History's opt-in behavior. | H. | Reconcile with upstream History policy; InputState owns the decision to discard redo after a divergent text edit. |
| `src/input/element.rs` | Paint/layout/hit-test integration for mapped live preview, tasks, tables, links, images, fenced code, selection, and caret/IME geometry. | E, IE, C, TX. | Reconcile alongside `state.rs`, `text_wrapper.rs`, and `movement.rs`; source coordinates and painted geometry must agree. |
| `src/input/indent.rs` | Routes Tab/Shift+Tab to table navigation and list nesting, with disabled-state guards. | TX; E's list parser tests. | Preserve table precedence and isolated list transactions when merging upstream indentation behavior. |
| `src/input/input.rs` | Adds live-preview destination hover tooltip wiring and debug-only input bounds selectors. | E covers destination parsing; settings panel tests measure real input bounds. Tooltip presentation is static/manual. | Retain current-source hit lookup, direct-click wording and bounds diagnostics. |
| `src/input/mod.rs` | Registers local editor modules and exports Markdown-link helpers, doc-hidden table support, and work counters. | E, T, IE, C; root compilation. | Keep public versus internal exports intentional; direct-source test harness imports depend on these boundaries. |
| `src/input/movement.rs` | Floors movement to UTF-8 boundaries and integrates scaled preview rows and table-aware arrow/Home/End movement. | TX, IE, C; E's layout tests. | Preserve source/display conversion and table navigation before ordinary line movement. |
| `src/input/popovers/context_menu.rs` | Adds table row/column/alignment/delete actions with captured revision checks and enablement. | TX, T; popup visual placement is static/manual. | Retain revision validation and disabled/header/final-column guards when rebuilding upstream menus. |
| `src/input/search.rs` | Exposes find/find-only/replace entry points, literal match handling, source selection/reveal, and isolated UTF-8/UTF-16 replacement transactions. | E's find diagnostic; C exercises reveal from search. | Preserve ASCII-only case folding, non-overlapping literal matches, snapshot checks, and edit-history isolation. |
| `src/input/state.rs` | Integrates opt-in preview state, snapshot edits, the parse queue, source selections, task/list operations, image drops, code diagnostics, redo invalidation, and the read-only `is_disabled` accessor. | EA, H, TX, IE, C; E's find diagnostic. | Reconcile command entry points and every edit path together. Preserve replay guards, snapshot identity, selection, InputEvent behavior, and disabled-state checks before guarded application edits. |
| `src/input/text_wrapper.rs` | Owns incremental source-row wrapping, lazy measurements/caches, mapped preview/table/image/code geometry, and analysis-result application. | E, T, IE, C. | Preserve bounded viewport work, scroll anchors, stable table keys, and stale-analysis rejection; rerun performance validation after structural changes. |
| `src/lib.rs` | Exports the doc-hidden shared Markdown-image helper module. | I, IE; compilation. | Keep image recognition/resolution shared between Reading and Live preview. |
| `src/select.rs` | Uses flex containers to propagate percentage widths to the trigger, with a debug-only bounds selector. | SettingsPanel field-width regressions at normal and narrow viewport sizes. | Preserve full-width triggers and popup anchoring when adopting upstream layout changes. |
| `src/menu/menu_item.rs` | Selects the arrow cursor for menu rows. | Static/manual; no dedicated headless cursor assertion is identified here. | Preserve cursor behavior while adopting upstream menu rendering changes. |
| `src/menu/popup_menu.rs` | Makes popup surfaces occlude the editor and use the arrow cursor. | Static/manual; no dedicated headless occlusion assertion is identified here. | Review hit-test layering as well as visible styles. |
| `src/text/format/html.rs` | Initializes the added task-source-offset field for HTML-derived list nodes. | RL exercises Markdown/HTML type switching; compilation. | Keep non-Markdown nodes compatible with the extended shared node type. |
| `src/text/format/markdown.rs` | Retains task offsets and source mappings through Markdown AST conversion, including inline/code text. | R covers source maps; the separate Reading task-offset diagnostic is not invoked by these headless targets. | Preserve parser-position/UTF-8 contracts and literal-code mappings when adopting AST changes. |
| `src/text/inline.rs` | Projects source highlights into displayed ranges, resolves current-source link hits, and participates in one-shot match reveal. | R; link paint/hover interaction still has static/manual aspects. | Reconcile style composition, byte boundaries, snapshot checks, and horizontal reveal together. |
| `src/text/mod.rs` | Exposes `SourceHighlights` and registers Reading source-map modules. | R; compilation. | Preserve the opt-in highlight API and module visibility. |
| `src/text/node.rs` | Extends Reading nodes for task callbacks, mapped search/link rendering, table dimensions, images, and themed code presentation. | R, C, T; shared parser/layout diagnostics. | Check node constructors, renderer callbacks, table minimum dimensions, and code style against the upstream node model. |
| `src/text/text_view.rs` | Owns background initial/update parsing with one running/latest pending request, document image context, snapshot-aware callbacks, reveal lifecycle and Reading diagnostics. | R, RL, I, IE; native callback presentation has static/manual aspects. | Preserve worker-only initial parsing, source/type generation checks, live presentation props and retained tree source identity; reject stale snapshots before document mutations or reveals. |

## Added local files

| File | Local responsibility | Regression entry points | Upstream reconciliation |
| --- | --- | --- | --- |
| `src/highlighter/code.rs` | Resolves language tags, stores theme-independent code captures, composes styles, and maps literal code lines. | C, R; E's fenced-code cases. | Prefer compatible upstream code-token support if available; preserve aliases, plain fallback, Unicode/CRLF/tab mappings, and theme recoloring. |
| `src/input/fenced_code.rs` | Classifies fenced blocks and tracks bounded code caches/snapshots and literal source presentation. | E, C, R. | Reconcile with the preview queue and wrapper, especially unfinished/nested fences and cache invalidation. |
| `src/input/line_index.rs` | Maintains the implicit-treap index of source rows, wrapped rows, and cumulative heights. | E. | Preserve index invariants and complexity; do not replace it with full-document layout during an upstream merge. |
| `src/input/live_preview.rs` | Parses line-local Markdown/list/task/quote presentation, records lossless mappings, and reports editor work counters. | E, TX. | Keep rendering syntax and editing marker recognition consistent; preserve unsupported syntax literally. |
| `src/input/markdown_links.rs` | Builds GFM source/label/destination indexes while excluding code, and supports shared app link editing. | E; application `src/links.rs` unit tests. | Preserve reference-definition semantics and UTF-8 ranges; audit public app callers when changing the index model. |
| `src/input/preview_syntax.rs` | Owns shared GFM analysis requests/results, one-running/one-pending scheduling, edit translations, and retired index disposal. | E, including shared-visitor equivalence and exact request work counts. | Preserve one shared GFM tree per mixed request, independent visitor guards, bounded queue/edit-log work and worker destruction; geometry queries must not trigger document parsing. |
| `src/input/scroll.rs` | Normalizes input scroll offsets for empty/single-line controls. | E. | Keep placeholder origin and horizontal-only single-line behavior through upstream scroll changes. |
| `src/input/table.rs` | Builds GFM table/source indexes and generates cell/row/column operations with incremental metadata maintenance. | T, E. | Preserve source-relative ranges, escaping, parse equivalence, and current row/cell accessors rather than exposing stale stored offsets. |
| `src/input/table_edit.rs` | Applies table commands and keyboard navigation through InputState, including isolated history and normalized cell input/IME. | TX, T. | Reconcile with `indent.rs`, `movement.rs`, context menus, and all InputState replacement paths. |
| `src/markdown_images.rs` | Recognizes standalone image paragraphs, resolves document-relative resources, and rebases image offsets. | I, IE; application `src/image_import.rs` unit tests. | Keep recognition and URL policy shared across views; URL resolution alone does not impose notebook containment. |
| `src/text/source_highlights.rs` | Binds match ranges and one-shot reveal/cancellation state to an exact source snapshot. | R. | Retain snapshot identity and request lifetime semantics when adopting upstream search presentation. |
| `src/text/source_map.rs` | Maps displayed UTF-8 runs back to source and finds a containing block for hidden syntax. | R. | Verify escapes, entities, normalized text, literal code, and hidden-only matches against the upstream parser. |
| `EDITOR-PATCH.md` | Narrative behavior and invariants for the local fork. | Review against implementation and regression targets. | Reconcile the narrative when upstream implements or replaces a local behavior; do not silently discard it. |
| `EDITOR-PATCH-INVENTORY.md` | Complete path-level patch map and reconciliation checklist. | Repeat the baseline comparison. | Update baseline identity, changed-path lists, coverage gaps, and counts after every vendor upgrade. |

## Line-ending-only differences

`src/input/lsp/completions.rs` and `src/input/popovers/completion_menu.rs` differ
from the cached package only by CRLF/LF representation. They contain no local
algorithm change in this comparison. Treat them as formatting during
reconciliation; do not manufacture behavioral patches for these files.

## Upgrade procedure

1. Establish the exact new release/package identity and retain the old package
   as a read-only comparison baseline. Verify VCS metadata instead of assuming
   that a directory name proves the source revision.
2. Compare complete file sets, including hidden files. Report added, removed,
   substantive, and line-ending-only changes separately. The following read-only
   command gives a useful content diff once the two paths are set; exit code 1
   means differences were found:

   ```powershell
   git -c core.autocrlf=false diff --no-index --ignore-space-at-eol $baselinePath $vendorPath
   ```

   Enumerate and compare bytes separately as well: this Git command intentionally
   ignores end-of-line whitespace and therefore is not a byte-identity check.
3. Reconcile a behavior across its entire row group and callers, not just the
   most obvious file. The critical groups are editing/history, preview
   scheduling/layout, table commands, and Reading/source mappings. Keep changes
   in the checked-in vendor tree, never in Cargo's registry cache.
4. Keep `LICENSE-APACHE`, package metadata, and the application's Cargo patch.
   Review upstream features/dependencies before updating the lockfile. Preserve
   local opt-in defaults and test-only visibility deliberately.
5. Run root compilation and the headless regression targets above. Use the
   documented performance checks when changing parser scheduling, line indexes,
   or geometry. Record remaining static/manual gaps instead of claiming UI
   verification from compilation alone.
6. Update this inventory and the narrative, record the new baseline and
   reconciliation decisions, and commit each completed improvement separately.

Line movement: input/live_preview.rs owns source-line swapping; input/state.rs applies the isolated edit and selection. Reconcile these contracts on vendor upgrades.
Ordered swaps use input/list_edit.rs to normalize affected sibling groups.
Coverage: move_line_tests in tests/editor.rs and ordered_line_moves in src/editor/tests.rs.


Inline code: input/inline_code.rs (new GFM ranges), preview_syntax.rs and text_wrapper.rs (shared analysis), live_preview.rs (mapped presentation), element.rs (monospace and background), text/node.rs (Reading color). Tests: tests/editor.rs inline_code and sustained_edit_tests.


List dragging: new input/list_reorder.rs (GFM item boundaries and source transform), input/list_drag.rs (gesture state and transactions); state.rs/element.rs wire pointer events and feedback; preview_syntax.rs/text_wrapper.rs share revision-guarded analysis. Coverage: list_reorder tests and tests/editor_transactions.rs.


Live-preview fenced blocks reserve the full wrapped geometry of both fence lines.
Selection changes hide or reveal fence glyphs only; font, padding, source mapping,
background and downstream positions stay fixed. Outer fence rows own rounded
corners. `tests/code_highlighting.rs` covers caret/selection movement, themes,
long info strings, empty/unfinished/nested blocks, Unicode, CRLF and history.
Live checkbox border alignment: input/text_wrapper.rs snaps final paint geometry
using the window scale factor. Regression: tests/editor.rs,
text_wrapper::tests::task_borders_align_to_physical_pixels_at_fractional_positions;
existing tests/editor_transactions.rs covers task interactions and undo. Preserve
physical pixel alignment when reconciling upstream paint changes.

Table border ownership: input/text_wrapper.rs; tests/editor.rs exercises single shared edges and fractional display scales.
Table continuity: input/table.rs retains unchanged source ranges through pending
analysis and list-drag replacements; text_wrapper.rs sustained_edit_tests covers
typing, Enter, keyboard moves, cross-table drags, source offsets, width/scroll
stability, delayed results, and exact structural invalidation with LF/CRLF.

Bullet cursor: input/list_drag.rs shares marker bounds with input/element.rs hover hitboxes. Headless editor_transactions list_drag verifies the rendered targets.

Drag viewport: input/list_drag.rs, list_reorder.rs, state.rs and element.rs. Coverage: list_drop_keeps_scrolled_viewport_with_offscreen_caret plus existing reorder and transaction tests.

Numeric Enter: new input/list_edit.rs; input/state.rs and input/mod.rs integration. Coverage: list_edit unit tests through tests/editor.rs and editor_transactions list_task_and_selection_transactions.

Numbered dragging: input/list_reorder.rs metadata and source moves; input/list_edit.rs shared numbering/offset mapping; input/preview_syntax.rs numeric-only detection; input/list_drag.rs marker geometry and transactions. Coverage: list_reorder/list_edit tests in tests/editor.rs and numbered/scrolled/edge-scroll diagnostics in tests/editor_transactions.rs.

Link retention: preview_syntax.rs translates queries crossing edits and compacts
offset logs without losing unchanged links; text_wrapper.rs retains projections
until exact context analysis. Coverage: editor link_queries and
unchanged_links_survive_pending_context_analysis tests.

Link interaction: live_preview.rs reveals individual links from the caret offset;
text_wrapper.rs invalidates same-line/cell projections at link boundaries.
state.rs opens direct clicks without selecting the label; input.rs documents the
click in its tooltip. Coverage: editor link_tests and cached-line/cell regression,
plus editor_transactions direct_link_click with an observed OpenLink event.

Link hover cursor: input/input.rs selects Arrow for a current-source link hit,
otherwise IBeam; text/inline.rs selects Arrow over Reading links. Retain this
cursor policy when reconciling upstream hover rendering.

Search reveal: input/state.rs exposes select_source_range_without_focus and owns
the pending UTF-8 range; input/movement.rs retires it on caret movement.
input/element.rs resolves endpoint wrapping before viewport layout and applies
current range/table geometry; input/scroll.rs supplies interval reveal policy.
input/preview_syntax.rs exposes the existing queue job count to production layout
so the reveal remains pending until analysis settles. Coverage:
tests/search_reveal.rs and tests/editor.rs scroll range tests. Preserve focus
independence, bounded endpoint measurement, and editor-interaction cancellation
when reconciling upstream changes.
Single-line form sizing follow-up: input/state.rs uses flex for single-line
containers and exposes diagnostic_input_paint under editor-diagnostics;
input/element.rs allows the text leaf to grow. Regression entry point:
src/settings_ui/panel/tests.rs::date_input_paints_typed_text.

## Live-preview thematic breaks (2026-09-14)

No additional vendor files. `src/input/preview_syntax.rs` extracts divider ranges
from the shared worker tree. `src/input/live_preview.rs` retains literal divider
presentation; `src/input/text_wrapper.rs` owns revision guards, invalidation,
source geometry and rule painting. `src/input/element.rs` carries presentation
and content width into layout. `src/input/state.rs` adds feature-gated headless
geometry inspection. Regression entry points: `tests/horizontal_rules.rs` and
the shared-analysis and sustained-edit suites compiled by `tests/editor.rs`.

Divider follow-up: TextWrapper rebases unchanged divider source lines through
pending edits instead of clearing them globally. Boundary checks retire changed
or joined lines without copying/parsing document text. The delayed-newline and
exact-context regressions are in tests/editor.rs; tests/horizontal_rules.rs also
exercises repeated Enter through the real action in TestPlatform.

### Link context menus (2026-09-14)

`src/input/popovers/context_menu.rs` adds Copy link before the existing editor
and table actions, captures the pre-selection painted destination and resets
menu navigation on each opening. `src/input/state.rs` adds feature-gated layout
and menu probes. `src/text/inline.rs` uses exact painted text hits for right-click;
`src/text/node.rs` routes linked-image presses through a weak Reading owner.
`src/text/text_view.rs` owns the popup, dismissal subscription and selection
preservation. Reconcile source snapshot ownership, focus restoration, stale Live
layout rejection and captured clipboard values during upgrades. No new vendor
files are added. `tests/link_context_menu.rs` exercises actual headless pointer,
menu and clipboard paths, including Unicode, wrapped/table/reference links,
linked images, selection, menu dismissal and pending Reading parses.

## Shared document views

`input/shared_buffer.rs` adds InputBuffer with shared rope/history and weak
InputState view registrations. Each view keeps its own selection, scroll, find,
mode and layout. Edit/IME/Undo/Redo publish synchronously to peer snapshots,
remapping UTF-8 selections and viewport anchors without taking focus or adding
history entries. InputBufferEvent::Changed lets document owners retain edit
subscriptions independently of any view. View switches break typing groups.
The application opts in through Editor; ordinary form inputs remain independent.

Reconcile `input/shared_buffer.rs`, `input/state.rs`, `input/mod.rs`,
`input/table_edit.rs`, `input/list_drag.rs`, and `history.rs` on upstream upgrades.
The viewport anchor restore also supports Source views. Validate with
`tests/editor_shared.rs`, existing editor/history/transaction tests and
`notebook_ui::tabs_tests` on TestPlatform. No native UI testing is required.

## Live-preview strikethrough

Added `src/input/strikethrough.rs` and registered it in `src/input/mod.rs`.
Reconcile its GFM Delete ranges with `preview_syntax.rs` shared parsing,
`text_wrapper.rs` revision invalidation, `live_preview.rs` source mapping and
`element.rs` shaped-run decorations. Preserve code/escape exclusions and
composable inline formatting. `tests/editor.rs` covers parser/projection and
stale results; `tests/strikethrough.rs` covers the headless render/edit lifecycle.

## Code-block copy controls

- Added `src/code_copy.rs`, registered by `src/lib.rs`: shared icon button,
  tooltip, plain-text clipboard callback and pointer isolation.
- `src/text/node.rs`: default Reading copy action and trailing content inset.
- `src/input/fenced_code.rs`: retain parser-normalized code on each block.
- `src/input/text_wrapper.rs`: reserve width on the opening fence row.
- `src/input/element.rs`: visible Live controls positioned from measured rows;
  reject callbacks after source revision or mode changes.
- Regression entry points: application `tests/code_copy.rs`,
  `tests/code_highlighting.rs`. Reconcile all pieces together on upgrades.

## Mermaid extension (2026-09-17)

| File | Local contract and verification |
| --- | --- |
| `Cargo.toml`, `src/lib.rs` | Optional `mermaid` feature pins Merman 0.7.0, resvg 0.45.1 and image 0.25.10. Root application enables it. Renderer diagnostics require `editor-diagnostics`. |
| `src/mermaid.rs` (added) | Offline source-to-image worker, shared bounded LRU, keyed weak tickets, one running job, bounded pending work, resource denial, transparent root canvas over native themed surfaces, alpha conversion, Reading width measurement and renderer/queue diagnostics. |
| `src/mermaid/svg.rs` (added) | Pinned Merman 0.7.0 compatibility correction centers SVG flowchart edge-label text and backgrounds on the edge anchor; real-renderer checks cover themes, directions, short/long/multiline labels and idempotence. |
| `src/input/fenced_code.rs` | Mermaid language classification during the existing worker AST visit, retaining exact code and source ranges. |
| `src/input/text_wrapper/mermaid.rs` (added), `src/input/text_wrapper.rs` | Multiline anchor/covered/source projections, zero-height source rows, selection reveal, keyed invalidation and weak image retention. |
| `src/input/element.rs`, `src/input/state.rs` | Visible-block requests, viewport recalculation and scroll compensation; diagram source hit testing and headless diagnostics. |
| `src/text/format/markdown.rs`, `src/text/node.rs`, `src/text/text_view.rs` | Fenced-only Reading diagram classification, native images, source/error fallback, preserved code actions and active Find reveal, plus probe diagnostics. |

Verification: `tests/mermaid.rs` uses real native rendering and GPUI TestPlatform;
`tests/editor.rs` also compiles the no-Mermaid wrapper path. Existing code copy,
code highlighting, shared editor and Reading lifecycle tests cover interactions.
Reconcile the added feature and both renderer integrations before upgrading.
