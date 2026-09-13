# Local editor patch

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

Live-preview links use GPUI's secondary modifier: Command-click on macOS and
Control-click on Windows/Linux, matching each platform's editing conventions.

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
