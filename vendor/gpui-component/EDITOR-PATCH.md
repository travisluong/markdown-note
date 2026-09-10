# Local editor patch

Upstream: gpui-component 0.5.1, commit
`0f0ab35233212f8f3277028995caf0c41e13ee6c`, `crates/ui`.
The original Apache 2.0 license is retained. Cargo uses this copy via the root
`[patch.crates-io]`; no registry files are modified.

Changes are confined to input support and the `editor-diagnostics` feature:

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
test dependencies. The optional in-app diagnostics additionally exercise the
actual InputState edit, undo, redo, mode, selection, disabled, and IME paths.

When updating upstream, rebase these input changes and rerun the tests and
release benchmark. Do not silently remove the Cargo patch.

Task checkboxes retain UTF-8 state offsets in the reading AST and expose an opt-in TextView callback with the parsed source snapshot. Live preview maps task glyphs to the source state; InputState toggles one byte as an isolated undo operation and rejects stale snapshots. Task Enter continues unchecked tasks or exits empty tasks onto the next line in both editable modes.

Blockquotes retain mapped prefix ranges and nesting depth. The same font-scaled indentation is used for wrapping, shaped-line painting, selections, caret geometry, and hit testing. Each quote level paints a vertical bar; text uses the theme's muted foreground. Prefixes reveal on the cursor source line. Nested tasks use their actual painted hit bounds so clicks in the quote gutter do not toggle them. Diagnostics cover Unicode reading-task offsets, nested quote checkbox clicks, wrapping, source/display round trips, font changes, scrolling, and both themes.
Task checkbox controls remain visible on the cursor line in Live preview; only Source mode exposes their literal markers. Quote and emphasis prefix reveal remains independent.

Caret tracking preserves the viewport while the current line is visible, including at the top and bottom edges. It scrolls only the distance needed to reveal an offscreen line, using the live-preview heading scale, instead of moving by whole rows inside an artificial edge margin. Integration tests cover stable repeated typing, Enter at the bottom, partial clipping, and small viewports.

Command insertion exposes an isolated selection replacement API, retaining the editor selection while a palette owns focus. Diagnostics cover Unicode selection replacement and single-step undo/redo.

Live-preview task controls now reserve an exact 0.875 rem object slot during wrapping and shaping. They paint Reading mode's primary border/fill, half-radius corners, and 0.5 rem check icon; both states share the same source mapping and geometry. Mouse hit testing uses the painted square. Geometry diagnostics cover fixed dimensions, state-independent bounds, nested quotes, themes, font changes, and scrolling.

Ctrl+L uses the public isolated multiline task edit API. Shared recognition accepts indentation, nested quote prefixes, unordered markers, and ordered `.`/`)` markers. Insertions map both selection endpoints while preserving direction; a final line touched only at its start is excluded. Parser tests cover mixed lines, Unicode, indentation, empty lines and boundaries; InputState diagnostics cover reversed selections and undo/redo in both editable modes.
