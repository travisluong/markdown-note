# Local editor patch

## Per-dialog overlay colors

`Dialog::overlay_color` overrides only the backdrop paint color, defaulting to the
theme overlay. A transparent color retains modal mouse interception and outside-click
dismissal; `overlay(false)` retains its existing behavior. Mithril's shared palette
dialog opts into transparency for Commands, Go To, Change Notebook and Sort Files.
Reconcile `src/dialog.rs` on upgrades. The diagnostics-only color accessor and
`goto::tests::palette_overlays_preserve_colors_and_outside_click_focus` cover default
and transparent colors in both themes, dismissal and focus restoration on TestPlatform.

## Independent workspace dividers

ResizablePanelGroup::adjacent_only opts into resizing only the two panes beside
the dragged divider, clamped against both panes' limits using measured sizes.
The default cascading behavior remains available to existing callers.
ResizableState::completed_divider identifies the completed drag so the app saves
only that side's preference. Mouse handlers read current drag state at dispatch.
Reconcile src/resizable/{mod,panel}.rs on upgrade. Root toolbar tests cover
independent dividers, limits, narrow layouts and preserved preferences.

## Toolbar dropdown click triggers

Popover::trigger_on_click and DropdownMenuPopover::trigger_on_click opt into
completed-left-click activation instead of mouse-down. Both default to false,
preserving other popovers. Mithril's Heading and Sort toolbar buttons opt in so
GPUI can distinguish dragging from activation before opening a menu. Disabled
buttons continue blocking activation. The open state captured on mouse-down survives
a repaint after outside-click dismissal, so clicking an open trigger still closes
it. The toolbar registers drag gestures directly
through Button's existing Interactivity; no Button API or click policy changes.

Reconcile src/popover.rs and src/menu/dropdown_menu.rs on upstream upgrades.
Coverage: src/toolbar_ui/tests.rs tests ordinary menu clicks, disabled actions,
reordering without menu activation, and retained editor focus/selection.

## Independent interface icon colors

Theme.icon_color is an optional, serde-compatible application override. Icons opt
in with Icon::themed_color; explicit icon text colors take precedence, while the
default continues to inherit text color (entity icons default to theme.foreground).
Both render paths and clones preserve the option. Neutral Button variants, menu
icons/checks/carets, Select controls and List controls opt in; disabled controls,
primary/status buttons, loading indicators and semantic colors retain their roles.
Application toolbar children pass their disabled state explicitly. The optional
editor-diagnostics color capture records resolved colors only after a test opts in.

Reconcile src/theme/mod.rs, src/icon.rs, src/button/button.rs, src/menu/popup_menu.rs,
src/select.rs and src/list/{list,delegate,list_item}.rs on upstream upgrades.
Coverage: tests/theme_icons.rs and application toolbar/settings headless tests.

## Dialog close-button pointer isolation

`src/dialog.rs` stops left-button presses on the close control from reaching the
overlapping draggable title. Otherwise the title starts a drag and captures the
release before the close click runs. Existing cancellation, close callbacks and
focus restoration are unchanged. The `dialog-close` debug selector supports the
headless `modal_settings_close_icon_restores_focus_after_drag_resize_and_reopen`
regression. Preserve this event isolation when reconciling upstream dialogs.

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

## Movable dialogs

`Dialog::movable(DialogPlacement)` opts into caller-owned per-open placement,
fixed preferred dimensions and 16-pixel viewport clamping. Title presses capture
pointer movement/release across the window; content and control presses keep
normal behavior. Movable dialogs fill their body, with caller-owned scrolling,
and skip the positional entrance animation. Ordinary dialogs preserve upstream
layout, focus, keyboard, close callbacks and animation. Settings tests cover
normal/small viewport bounds, scrolling, dragging/release, section resets and
preference-edit placement retention. Reconcile `src/dialog.rs` on upgrades and
rerun `settings_ui::panel::tests` on headless TestPlatform.

The [file-level inventory](EDITOR-PATCH-INVENTORY.md) maps every modified and
added file against the verified published 0.5.1 package, with regression entry
points and upgrade reconciliation notes. Preserve both documents and the license.

Ordered keyboard line moves reuse list_edit numbering after the source swap,
preserving list starts, delimiters, Unicode selections and digit-width changes
in one isolated undo entry. Tests cover both directions, selected siblings,
nested/quoted lists, literal code, CRLF and Source/Live history.

Document edits invalidate the abandoned redo branch after Undo. InputState clears
redo only for actual text changes, after the replay guard; generic History keeps
its existing opt-in policy. `tests/editor_history.rs` covers isolated edits,
ordinary typing, Unicode IME grouping, selection and identical-text replacements
in Source and Live preview, plus generic history compatibility.

Live-preview links open with a direct click, preserving source and selection.
Only the link touched by the caret (including source boundaries) reveals its
Markdown; other links on the same line or table cell stay rendered and underlined.
Caret movement through a revealed destination keeps it editable. Shift/Alt clicks
retain selection behavior, and destination hits reject stale layout snapshots.

Live-preview checkbox state replacements are recognized as context-independent
edits by the shared syntax classifier, including undo and redo. Unchanged tables
retain their rendered rows, dimensions, and horizontal scroll while background
analysis is pending and after it completes. Headless regression tests cover
repeated toggles, delayed analysis, nested tasks, Unicode, and CRLF; structural
Markdown edits still invalidate background-result reuse conservatively. Table
projections whose source bytes are unchanged survive pending context analysis,
including tables crossed by list drags. Exact parses apply genuine structural
changes; changed tables retire individually. No document GFM parse or source copy
is added to the input thread.

Reading and Live-preview task checkbox squares explicitly use the standard arrow
cursor. Live-preview cursor hitboxes share the checkbox click geometry.
Checkbox presses consume mouse-down before text selection in both views. Live
preview handles every click count (including rapid double/triple clicks) and
Shift-clicks as checkbox interactions, preserving the existing text selection;
disabled checkboxes consume the press without editing.

Upstream: gpui-component 0.5.1, commit
`0f0ab35233212f8f3277028995caf0c41e13ee6c`, `crates/ui`.
The original Apache 2.0 license is retained. Cargo uses this copy via the root
`[patch.crates-io]`; no registry files are modified.

Changes cover input support, Reading-view source mappings, menu cursors, and the `editor-diagnostics` feature:

- Opt-in `find_only()` configuration hides replacement controls, with a public
  `open_search()` entry point for app-level Ctrl+F. Non-searchable inputs propagate
  the search action; the find query handles repeated Ctrl+F locally. Find selects
  and reveals the active source range, wraps from the end, notifies after edits,
  and closes on Escape from either the panel or document. Literal matching retains
  upstream ASCII case folding. Find diagnostics exercise matching and keyboard
  integration in Source, Reading, and Live preview, including tables.

- Opt-in live-preview presentation and cheap immutable rope snapshots.
- Heading markers hide outside the cursor source line, using lossless source mappings.
- Line-local Markdown parser, bidirectional UTF-8 display mappings, and list Enter.
- An implicit treap indexes source rows, wrapped rows, and cumulative heights.
- Unmeasured lines use estimates; viewport measurements preserve the scroll anchor.
- Shaped segments are cached per line revision, wrapping configuration, font, and runs.
  Long lines shape only visible wrapped segments plus a viewport of overscan.
- Mouse, selection, cursor, and IME layout use original source coordinates.
- Cumulative parsed-line and shaped-byte counters; optional in-app diagnostics.

The integration test target in `tests/editor.rs` compiles the patched parser,
height index, and wrapper modules directly, avoiding unrelated upstream component
test dependencies. `tests/editor_transactions.rs` runs the actual InputState
list/task and table diagnostics on headless TestPlatform, including undo/redo,
mode, selection, disabled and IME paths. Table progress is an explicit callback;
only the benchmark caller writes progress files. Additional in-app diagnostics
cover native rendering and remain user-run.

When updating upstream, rebase these input changes and rerun the tests and
release benchmark. Do not silently remove the Cargo patch.

Task checkboxes retain UTF-8 state offsets in the reading AST and expose an opt-in TextView callback with the parsed source snapshot. Live preview maps task glyphs to the source state; InputState toggles one byte as an isolated undo operation and rejects stale snapshots. Task Enter continues unchecked tasks or exits empty tasks onto the next line in both editable modes.

Blockquotes retain mapped prefix ranges and nesting depth. The same font-scaled indentation is used for wrapping, shaped-line painting, selections, caret geometry, and hit testing. Each quote level paints a vertical bar; text uses the theme's muted foreground. Prefixes reveal on the cursor source line. Nested tasks use their actual painted hit bounds so clicks in the quote gutter do not toggle them. Diagnostics cover Unicode reading-task offsets, nested quote checkbox clicks, wrapping, source/display round trips, font changes, scrolling, and both themes.
Task checkbox controls remain visible on the cursor line in Live preview; only Source mode exposes their literal markers. Quote and emphasis prefix reveal remains independent.

Caret tracking preserves the viewport while the current line is visible, including at the top and bottom edges. It scrolls only the distance needed to reveal an offscreen line, using the live-preview heading scale, instead of moving by whole rows inside an artificial edge margin. Integration tests cover stable repeated typing, Enter at the bottom, partial clipping, and small viewports.

Command insertion exposes an isolated selection replacement API, retaining the editor selection while a palette owns focus. Diagnostics cover Unicode selection replacement and single-step undo/redo.

Live-preview task controls now reserve an exact 0.875 rem object slot during wrapping and shaping. They paint Reading mode's primary border/fill, half-radius corners, and 0.5 rem check icon; both states share the same source mapping and geometry. Mouse hit testing uses the painted square. Geometry diagnostics cover fixed dimensions, state-independent bounds, nested quotes, themes, font changes, and scrolling.

Ctrl+L uses the public isolated multiline task edit API. Shared recognition accepts indentation, nested quote prefixes, unordered markers, and ordered `.`/`)` markers. Insertions map both selection endpoints while preserving direction; a final line touched only at its start is excluded. Parser tests cover mixed lines, Unicode, indentation, empty lines and boundaries; InputState diagnostics cover reversed selections and undo/redo in both editable modes.

Live-preview tables use the installed markdown crate's GFM recognition and a
source-range block index. Plain documents without pipes bypass the GFM parser;
ordinary cell edits update ranges locally, while syntax/context changes reparse
neighboring blocks or rebuild the index conservatively. Header/body cells share
column widths, mapped inline formatting, wrapped heights, and a table-local
horizontal offset. The delimiter line has zero height in the line index.
Visible shaped cell segments are cached; active-table widths stay fixed during
typing. Reading mode shares column sizing and padding with live preview.

Table editing stays in InputState's rope, selection, history, and change-event
pipeline. Tab/Shift+Tab, Enter, arrows, Home/End, and Escape navigate cells;
Tab at the end adds a body row. Context menus insert/delete rows and columns,
set alignment, and delete tables. Commands validate the source revision and
produce isolated undo transactions. Missing cells in uneven rows materialize
on entry. Input preserves escaped pipes and normalizes pasted cell newlines.
IME composition selection is mapped relative to the inserted text, including
normalization. Nested/container tables remain source text in live preview.

The doc-hidden input::table module exposes the parser/model to the application's
integration tests; it is not an upstream public API commitment. Tests cover
GFM recognition, source ranges, Unicode, structural edits, incremental/full
parser equivalence, and layout indexing. Editor diagnostics cover commands,
stale targets, undo/redo, disabled editing, IME, cell hit testing, selection,
and font/theme changes. The release harness includes a 10,000-row table;
autosave diagnostics exercise cell editing and row insertion together.

Active-table width caching validates the current column count after structural
edits. Inserted columns no longer hide the rightmost cells, and column deletion
cannot reuse an oversized width vector. Layout regressions cover insertion on
both sides, deletion, undo/redo snapshots, Unicode content, and LF/CRLF text.

Reading and live-preview table columns have a 120-pixel minimum, including empty
columns. Narrow viewports retain that minimum and scroll horizontally instead
of squeezing cells. Reading cells also declare the minimum in their flex style;
layout tests cover empty headers/body cells at narrow and wide viewport sizes.
Rows in both views reserve at least two text lines of height, including empty
headers and body rows. This scales with the font, while longer content can grow
the row. Live-preview height indexing uses the same minimum and keeps the
Markdown separator hidden; regression checks cover row heights and offsets.

Tab/Shift+Tab nest list items in Source and Live preview, moving their descendants
and indented continuation lines together. Nesting aligns to the preceding sibling's
list content column (before a task checkbox); outdent aligns to the enclosing
parent, with the configured space step as a fallback. Selected siblings retain
their relative level, and each subtree is edited once. Quote prefixes, markers,
checkbox states, line endings, and selection direction are preserved. Markdown
tabs are measured at four-column stops and edited indentation is written as spaces.
Table navigation keeps precedence, and each nesting command is one undo operation.
Shared marker recognition covers indented unordered and ordered lists; Enter
preserves prefixes and continues numbering and unchecked tasks in both modes.
Live preview renders nested bullets with lossless mappings. Parser tests and
in-app diagnostics cover subtree edits, selection boundaries, Unicode, CRLF,
undo/redo, disabled editing, Reading hierarchy, and nested checkbox hit geometry.
`select_source_range()` validates UTF-8 byte boundaries, selects and reveals a source match without changing text or undo history, for notebook-wide search navigation.

Reading search uses opt-in `TextView::source_highlights(Option<SourceHighlights>)` with
UTF-8 ranges and an exact Markdown snapshot. Inline text, nested formatting,
reference links, table cells, and code blocks retain source mappings. Highlights
compose with existing text styles. A one-shot request reveals the virtualized
block first, then the measured match, and rejects stale source/query requests.
Source-only matches reveal the containing block and expose `hidden_match()` for
an explanatory notice. Parsing stays on the background executor; UI scroll
handles stay on the UI thread. `tests/reading_search.rs` exercises the mappings
and the actual renderer parser without opening a window.

Cold Reading initialization also dispatches parsing to the background executor.
TextView keeps one running request and one latest pending source/type snapshot;
both success and error completions must match the latest generation. A retained
tree continues to supply its own source to callbacks while the next parse waits.
Style, theme and callback props update independently because AST construction
depends only on source and Markdown/HTML type. The same ListState survives
updates, and find highlights/reveal wait for the matching rendered snapshot.
`tests/reading_lifecycle.rs` controls worker completion on TestPlatform to verify
cold layout, coalescing, stale errors, styles, callbacks, find, scroll and teardown.

Menu rows and popup surfaces explicitly use the arrow cursor, and popup hit
areas occlude the editor underneath. Deferred input scrolling is normalized
before painting: empty placeholders stay at their layout origin and single-line
inputs retain horizontal scrolling without fractional vertical movement. The
scroll tests cover both behaviors.
Markdown links have a document-level GFM source index shared with app commands.
Inline, reference and autolinks retain UTF-8 source/label ranges; fences and code
spans stay outside the index. Live-preview labels use theme link styling,
including tables, and syntax reveals with the existing active-line/cell policy.
Ctrl+click emits InputEvent::OpenLink only for an exact painted text hit with a
current source snapshot. Strict hit testing excludes quote/table gutters and
unused row height. Hover shows the destination in both views.

TextView::on_link_open receives the destination and rendered source snapshot;
without a callback the upstream external-opening behavior remains available.
The app validates the snapshot, handles external schemes and relative Markdown
paths, and uses its existing save/navigation flow. The link dialog reuses
select_source_range and replace_selection_isolated, validating document identity
and snapshot before applying a single undo transaction. A read-only
InputState::is_disabled accessor lets the application reject guarded range edits
before selection changes when the rendered input is still disabled. The editor
adapter's TestPlatform regression verifies unchanged text, selection, and history
for that rejected transaction.

Link parsing is cached across viewport measurement and cursor movement. Ordinary
alphanumeric/space edits in standalone paragraphs reparse that
paragraph with the current reference definitions and verify its block type; structural/context edits
conservatively reparse the document. Documents without link trigger characters
skip GFM parsing. A 10,000-link regression checks bounded paragraph parse work;
headless tests cover incremental/full-parser equivalence, definition changes,
Unicode source mappings, table references, and wrapped/gutter hit testing.

- Ordered list nesting starts a fresh sibling count, with an app-configurable alphabetic marker option. Alphabetic markers support Enter continuation and outdent; they are a plain-text extension to standard Markdown.


The command palette can call public `InputState::open_search_replace()` to open
replacement controls with the find query focused. Find-only and disabled inputs
remain protected. Search replacements use isolated undo edits, convert whole-file
UTF-8 byte ranges to UTF-16 before editing, and reject deferred edits when the
source changed. Match indices clamp and reset after replacements. The headless
find diagnostic covers literal Unicode replacement and last-result removal.

Sustained Live-preview editing separates rope/line updates from document analysis.
The InputState-owned queue has one running parse and one latest pending rope
snapshot, starts without an idle timer, and never cancels the running parse on
another keypress. Safe edits translate snapshot link ranges; context changes,
document replacement, mode changes, and exhausted edit logs reject stale results.
GFM parsing and full-document string conversion run on the background executor.
Each queued mixed request now builds at most one GFM tree and lends it to link,
table, fenced-code and image visitors. Each visitor retains its own candidate
guard and standalone entry point; plain requests build no GFM tree. The tree
is dropped on the worker after producing the existing indexes, so no AST cache
is retained across edits. `diagnostic_preview_parse_work` counts actual GFM
calls and input bytes separately from the existing table/index counters.
Line measurement and cursor reveal cannot invoke document parsers. Presentation
invalidations are revision-based and measured lazily in the existing viewport
plus overscan, preserving the scroll anchor and unchanged shaped rows.

Table column hints are computed with the background parse. Width and horizontal
scroll caches survive offset-only edits through stable table keys. Table layout
queries only requested rows; a per-table prefix-sum index translates subsequent
row offsets after cell edits without visiting every row. Cell ranges remain
exact for input normalization, keyboard navigation, Unicode, and IME. The
internal/doc-hidden table test interface exposes row_range/cell_range for current
source coordinates; the stored rows are snapshot-relative. Adding a table row
uses a small source insertion and local metadata update rather than rewriting
and reparsing the whole table. Structural/context changes display current source
while awaiting valid analysis. Large retired link/table indexes are freed on the
background executor.

Headless regression tests cover sustained Enter/Backspace-style replacements in
100,000-line, 10,000-link, and 10,000-row-table fixtures; bounded copied bytes,
viewport row visits and parser queues; delayed and superseded completions; cache
identity; reference/fence changes; shifted table commands; Unicode and CRLF.
Existing synchronous behavior diagnostics explicitly settle synthetic fixtures;
the cadence benchmark awaits the real parser and records painted revisions.
No desktop interaction or visible app launch is required for the automated tests.
Known whole-table commands rebuild their generated row/column metadata directly,
without GFM parsing, so Tab/arrow navigation remains available immediately after
column, alignment and missing-cell commands. Cached widths resume content-based
sizing when the caret leaves the active table.

Standalone Markdown image paragraphs use background GFM recognition. InputState's
opt-in image document context resolves relative paths through the same resolver as
Reading TextView. Live preview loads assets asynchronously, reserves image height
in the line index, maps image hits to source boundaries, and retains an image
while revealing its active source line. Failed assets paint a stable placeholder;
images preserve aspect ratio and do not upscale. Source mode keeps Markdown text.
Parser, path, height-index and hit-mapping regressions run in the headless tests.

Image drops use public drop_offset (last painted UTF-8 hit testing) and
insert_at_snapshot (snapshot-validated isolated insertion). The insertion bypasses
cell-input normalization while retaining normal history, change events and search
updates; previously selected text is not replaced by a drop. In-memory GPUI
TestPlatform tests cover isolated undo/redo and stale snapshot rejection without
creating native windows or sending OS input.

Image-line caret changes retain measured image geometry while rewrapping source.
Scroll anchors are captured before reveal changes the height index. A click away
from an image preserves the clicked source position for the next paint, bounded
by the document's scroll limits; Markdown source still collapses normally.
Headless image-editor regressions cover loaded and unavailable assets, wrapped
source, clicks above/below images, scrolled views and both scroll boundaries, and repeated frames
and clicks without delayed movement.

## Fenced-code language support

The application enables the bundled Tree-sitter grammars. Reading code blocks
share normalized language resolution (including aliases and plain-text fallback)
and retain theme-independent token classifications. Rendering resolves the current
highlight theme, preserving syntax colors under search backgrounds. Headless
reading-search diagnostics exercise the language bundle, aliases, multiline and
Unicode tokens, plain fallbacks, and light/dark recoloring. The lockfile selects
cc 1.2.67 to satisfy the bundled SQL grammar's ~1.2.1 build dependency.

## Live-preview fenced-code highlighting

Fenced-code analysis runs in the existing bounded preview queue. GFM code nodes
provide block boundaries, including unfinished fences and nested containers;
literal line mappings preserve UTF-8, CRLF, escapes, entities and partial tabs.
The same mappings also keep reading-search results aligned with literal code.
Only language/content entries present in the latest document are retained in the
background token cache. Token results require an exact document revision, unlike
rebasable link results. Edits clear old colors while retaining bounded literal
source presentation; retired indexes are released on the background executor.
Documents without fence markers skip code parsing.

Live-preview shaped runs resolve cached captures against the current theme and
preserve source/display coordinates, search painting, selection and IME decoration.
Code contents do not receive Markdown heading, emphasis, link or task styling.
The code helpers are exposed only with editor-diagnostics so root integration tests
can exercise the vendored implementation; production has no new public API.
Tests cover cache reuse/pruning, delayed results, nested/unfinished fences,
Unicode/CRLF/tab mappings, literal presentation, and actual TestPlatform shaped
runs across themes, wrapping, mode changes and undo/redo. No native UI is used.

Fenced code blocks in Live preview retain Reading mode's theme colors, monospace
font/size, padding and rounded surfaces while editing. Fence rows retain their
full wrapped height; only their glyphs hide outside the selection's block.
Find reveals fence text too, and losing focus alone preserves this state.
Empty blocks retain clickable fence rows at their actual source positions.
Block geometry uses the existing height index, lazy viewport measurement, and
background syntax snapshots; stale classifications never hide source. Headless
tests cover nested/unfinished/empty fences, Unicode/CRLF, keyboard and pointer
entry, selection, search, themes, history, and bounded large-block invalidation.

Code previews retain unaffected block geometry and cached tokens while background
analysis is pending. Bounded edit translations preserve earlier blocks during
new-fence typing and shift untouched blocks during plain-text edits. Structural
edits invalidate the affected block and potentially dependent following blocks.
Typing/backspace bursts coalesce; log overflow still preserves the unchanged
prefix. Regression tests cover per-keystroke and delayed analysis, Unicode/CRLF,
long edit/undo bursts, source-coordinate reveal, and closing-fence invalidation.

Line movement uses an isolated source transaction, retaining selection direction and final-newline state. Source and Live preview share the same implementation; IME and disabled input reject moves. Coverage: move_line_tests and editor::tests::line_moves_are_isolated_and_preserve_selection_in_both_modes.


Inline code is indexed from the shared GFM analysis and accepted only at the exact source revision. Live preview preserves literal markup inside code, reveals backticks on the active line, and paints muted surfaces below selection/find highlights; Reading uses the same muted background. inline_code and sustained_edit_tests cover parsing, byte mappings and stale results.


Live-preview bullets and tasks initiate list-item dragging after 4 px; checkbox clicks toggle on release. Shared GFM item ranges include nested content. Moves preserve task status, adapt indentation/bullet style, reject descendant/other-quote and cross numbered/bullet destinations, and use an isolated source edit. Drags retain editor identity, source revision and snapshot; Escape, edits and mode changes invalidate them. Marker hit testing is indexed by source row; insertion feedback, drag text and edge scrolling use the editor layout. TestPlatform verifies click-versus-drag, undo/redo, reversed Unicode selection and stale/cancelled gestures.


List drag follow-up coverage preserves CRLF without a final newline and normalizes lazy continuation indentation when nesting, so continuation text remains inside the moved item. Accepted text/IME edits clear a pending drag immediately.

Line movement treats the terminal empty line after a final newline as a document boundary and keeps empty selections empty; LF/CRLF regression coverage lives in move_line_tests.

Live-preview fenced blocks reserve the full wrapped geometry of both fence lines.
Selection changes hide or reveal fence glyphs only; font, padding, source mapping,
background and downstream positions stay fixed. Outer fence rows own rounded
corners. `tests/code_highlighting.rs` covers caret/selection movement, themes,
long info strings, empty/unfinished/nested blocks, Unicode, CRLF and history.
Settings fields inherit their complete content-column width through flex
containers. Select's outer/state containers also use flex layout so percentage
widths reach the trigger on narrow windows. Input and Select expose debug-only
bounds selectors for the actual controls; the SettingsPanel headless tests check
text inputs and all three font selectors at 800, 560 and 480 pixel viewports.
Live-preview task checkbox painting snaps its final screen origin, square size,
and uniform border width to physical pixels. Layout and pointer hit targets stay
unchanged; the checkmark is centered within the snapped square. The geometry test
in text_wrapper.rs covers fractional positions, 100/125/150/200% display scales,
and every supported interface font size. Existing editor transaction tests cover
checkbox state changes and undo. Reading rendering is unchanged.

Live tables paint each shared edge once, with physical-pixel-aligned borders matching the outer edge. Reading retains its existing single-edge layout.

Live bullet hover uses Arrow over the same source-mapped marker bounds as drag initiation. Text retains the editor cursor; visible-row indexing bounds hitbox work.

List drops preserve the viewport across selection remapping and background layout, replace only the changed source span, and refresh hit geometry after edge scrolling. tests/editor_transactions.rs covers a scrolled nested checkbox drop with an offscreen caret and post-analysis stability.

Numeric Enter renumbers siblings in the insertion undo step. input/list_edit.rs owns literal-aware container scanning, replacement mapping and continuation indentation. Tests cover non-1 starts, nesting, quotes, code, UTF-8, CRLF, caret mapping and Source/Live undo/redo.

Numbered list drag support extends the shared GFM item index and numeric-only parse detection. Numeric drops preserve each affected list's original start, adopt the destination delimiter, move descendants and remap selections through renumbering. Shared numeric edits adjust continuation columns at digit-width boundaries and exclude fenced literals. Marker hover uses rendered numeric widths; numbered tasks accept either number or checkbox. Regression coverage includes Source/Live Enter history, cross-list/nested drops, CRLF/UTF-8, task state, invalid destinations, stationary viewport anchoring and edge-scroll cancellation.

Unchanged Live-preview links retain their source-mapped projection during pending
context analysis. Range queries can cross edits; only overlapping links retire.
Long edit bursts compact the offset log instead of clearing the index. Regression
tests in tests/editor.rs cover same-line edits, Unicode, CRLF, delayed/stale
analysis, and exact parsing of changed fence context.

Live-preview and Reading links use the standard arrow cursor on hover. Live
preview shares the current-source destination hit lookup with its tooltip and
click handling; ordinary editor text keeps the I-beam cursor.

Notebook search uses select_source_range_without_focus for selection plus a
pending source-range reveal. Layout measures the endpoint rows before choosing
the viewport, then refines scrolling with current wrapped/code/table geometry.
The target survives pending preview analysis and is replaced by later search
navigation or cleared by editor movement, pointer input, scrolling or edits.
The existing select_source_range keeps its focusing behavior. Search selection
is painted while focus remains outside the editor. tests/search_reveal.rs covers
initial layout, distant Unicode, wrapped/horizontal matches, code, tables,
oversized selections, invalid ranges, superseding targets and undo/redo.
Single-line InputState now uses a flex container and its text leaf can grow to
the available width. A percentage-width leaf inside the previous block layout
could paint at zero width even while the outer field had a usable width.
SettingsPanel TestPlatform coverage types into the real input and checks shaped
text, inner geometry, deletion, invalid drafts, section changes, both themes,
narrow layouts, and horizontal scrolling. The diagnostic_input_paint accessor is
available only with editor-diagnostics; document input layout retains its mode.

Live-preview thematic breaks use the existing background GFM tree, including
spaced markers, underscores, quotes and list containers. Exact-revision parses
supply authoritative ranges. Edits retain and rebase byte-unchanged divider
lines, including newline insertion at either edge, until current context
analysis arrives. Changed/joined lines retire immediately; mode/document resets
clear all ranges. Stale results cannot replace the retained projection.
Literal shaping and UTF-8 mappings are identical on and off the caret row:
painting substitutes a 2 px theme.border rule across the available content
width, starting at the measured marker indentation. The caret row paints source
syntax. Wrapping, selection, copy and history remain source-based. Reading's
existing divider renderer is unchanged. tests/horizontal_rules.rs exercises
headless reveal, geometry, clicking, themes, modes and undo/redo; tests/editor.rs
covers GFM contexts, single-parse accounting and stale-result rejection.

Live and Reading links offer Copy link on right-click. InputState resolves the
painted destination before caret movement, preserves selection, and prepends the
item to the existing editor/table menu. Reopening resets the menu's selection;
explicit editor notification also renders menus when the caret does not move.
TextViewState owns Reading's menu and dismissal subscription. Inline text and
linked images supply destinations from the displayed parse, including retained
content during a newer parse. Reading selection starts only on left-click and
survives menu interaction. Both menus capture plain-text destinations, use
PopupMenu keyboard/pointer dismissal and restore the originating view's focus.
No production public API or new input-thread parse is introduced. Feature-gated
layout/menu probes support tests/link_context_menu.rs on headless TestPlatform.

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

`input/strikethrough.rs` derives GFM Delete ranges on the existing preview worker.
`preview_syntax.rs` includes tilde-only documents in analysis; `text_wrapper.rs`
accepts only current ranges and clears them after edits or resets.
`live_preview.rs` hides delimiters using existing source mapping and active-row
reveal; `element.rs` applies theme-colored strikes to content runs, composing
with links, emphasis and inline code. Literal and escaped tildes stay unchanged.
Reconcile these paths and run tests/editor.rs and tests/strikethrough.rs when
upgrading. The latter inspects actual headless shaped runs across themes,
wrapping, reveal, edits, undo/redo and Source mode.

## Code-block copy buttons

`src/code_copy.rs` provides the shared Copy code icon and isolates its mouse press
from selection handlers. `src/text/node.rs` supplies it as the default code action
and reserves trailing space; callers can still provide custom actions.
`src/input/fenced_code.rs` retains the parsed code value, `input/text_wrapper.rs`
reserves opening-row width, and `input/element.rs` lays out visible Live controls
with revision-guarded clipboard callbacks. Preserve these contracts on upgrades.
Coverage: `tests/code_copy.rs` and `tests/code_highlighting.rs` in the application.

## Reading selection retention for application menus

`src/text/text_view.rs` adds opt-in `persistent_selection`, default false for
both Markdown and HTML views. When enabled, outside mouse presses preserve the
rendered selection, allowing application menus to copy it after taking focus.
Inside presses still replace the selection. TextView implements Focusable so menu
Copy can target the rendered view even when another control held focus.
Mithril enables persistent selections for Reading;
Copy continues to use rendered text rather than Markdown source. Reconcile the
flag, constructors and outside-click handler on upgrade. Headless coverage lives
in `src/title_bar_ui/tests/edit.rs`; existing `tests/link_context_menu.rs` covers
the default behavior and link-menu selection retention.

## Offline Mermaid diagrams

The optional `mermaid` feature adds pinned Merman 0.7.0 and resvg 0.45.1 rendering
without a webview or JavaScript runtime. `src/mermaid.rs` owns keyed asynchronous
render requests, weak interest tickets, image limits, resource isolation and the
bounded shared cache. Its resvg-safe pipeline applies Merman's root-background
postprocessor with a transparent canvas so the native surface supplies the
light/dark background without replacing diagram element colors. Preserve this
postprocessor on upgrades; the dark palette alone can retain a white canvas.
`input/fenced_code.rs` retains Mermaid identity in the
existing worker parse; `input/text_wrapper/mermaid.rs`, TextWrapper and TextElement
implement multiline source-preserving projections, hit testing and height changes.
CodeBlock and the Markdown visitor render diagrams in Reading while retaining
source mappings, custom actions and Find's active-match reveal. `lib.rs` and the
InputState/ReadingProbe diagnostics expose headless verification only behind
`editor-diagnostics`. Preserve these contracts and rerun `tests/mermaid.rs`, code
copy/highlighting, editor/shared-buffer and Reading lifecycle tests when upgrading.
The component's upstream license remains unchanged; dependencies retain their
own MIT/Apache licensing. No Cargo registry source is edited.

## Centered document columns with full-width scrollbars

`Input::content_max_width` centers the multiline text leaf while retaining the
full-width search panel, wheel target and scrollbar overlay. The configured width
includes the input's existing horizontal padding. `TextView::content_max_width`
and `content_padding` center/pad rendered blocks inside the full-width virtualized
list, keeping the scrollbar and gutter wheel input at the viewport edge. Changing
the column configuration invalidates measured list rows and restores the logical
scroll anchor. Reading clears its pixel-based selection on column reflow, matching
its existing viewport-resize behavior. Defaults preserve component content width
and padding.

Reconcile `input/input.rs`, `text/text_view.rs` and `text/node.rs` when upgrading.
Headless app coverage in `src/settings_ui/panel/tests.rs` checks column and track
geometry in all three modes, narrow windows and sidebar resizing, gutter wheel
scrolling, right-edge track clicks and thumb drags, and Reading anchor retention.

## Document source languages

`InputState::set_document_language` configures a retained document view's source
highlighter using the existing CodeEditor mode. Switching Markdown to Live uses
its original plain-input projection, preserving the buffer, history and selection.
Other languages reject Live and Markdown list Enter/nesting/task operations.
Grammar changes reinitialize diagnostics against the current rope and invalidate
highlighting for the next render. Unconfigured generic inputs keep their behavior.
The diagnostic syntax-range probe supports headless verification of real styles
in Mithril's `src/editor/tests.rs`; retain this contract during upstream upgrades.

## Zoomable menus and dialogs

PopupMenu row heights use the window rem size (20/26 pixels at a 16-pixel rem)
so enlarged interface text remains inside its hit target. Dialog's default
24-pixel padding similarly uses 1.5 rem; explicit padding overrides still win.
Reconcile `src/menu/popup_menu.rs` and `src/dialog.rs` on upgrades. Application
zoom tests exercise nested menu actions and palette dismissal on TestPlatform.
