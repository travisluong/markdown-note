# Live preview validation

Measured on 2026-09-09 in an optimized release build, Windows 11 Home
10.0.26200, AMD Ryzen AI 9 465, 32 GB installed RAM (31.3 GiB OS-visible).
Available GPUs: Radeon 880M and NVIDIA GeForce RTX 5070 Laptop GPU.
The benchmark does not identify which adapter GPUI selected.
Window: 900 × 700 logical pixels.

The large fixture is 10,700,000 UTF-8 bytes: 100,000 newline-terminated Markdown
lines containing Latin text, Chinese, bold, and italic syntax (plus the trailing
empty source line). The long-line fixture is 1,179,648 bytes without newlines.
Both are generated in memory; no user file is changed.

| Fixture position | Edit p95 | Scroll p95 |
| --- | ---: | ---: |
| Beginning | 7.339 ms | 7.529 ms |
| Middle | 7.383 ms | 7.194 ms |
| End region | 7.010 ms | 6.850 ms |
| Long line | 27.758 ms | 6.853 ms |

Both requested warm-interaction targets pass: edits below 50 ms and scrolling
below 33 ms at p95. Raw counters and timings are in [live-preview.csv](live-preview.csv).
Ordinary edits parse one source line and shape at most 166 bytes. Scrolling the
large fixture parses at most four newly encountered lines and shapes at most
400 bytes per frame. Long-line edits parse one line and shape at most 16,040
bytes; long-line scrolling parses no lines and shapes at most 648 bytes.

Method: 60 frame-driven operations per group, discard six settling frames,
report p95 of the remaining 54. Edit time starts immediately before InputState
insertion and ends at the next GPUI frame callback, including synchronous edit,
layout, and paint work. Scrolling changes the input scroll offset by 80 logical
pixels per frame, reversing periodically. The ordinary app dirty-state
subscription remains enabled. Samples include frame scheduling; they are not
OS keyboard-event latency or GPU presentation timestamps. Cold opening, resizing,
and initial mode-switch costs are not represented by these warm samples.

## Reproduce

From the repository root in an interactive Windows desktop session:

```powershell
cargo build --release --locked --features editor-diagnostics
$env:MARKDOWN_NOTE_BENCH = Join-Path $pwd 'benchmarks/live-preview.csv'
& ./target/release/markdown-note.exe
# The benchmark closes its own window when done.
Remove-Item Env:MARKDOWN_NOTE_BENCH
```

The environment variable is ignored without `editor-diagnostics`. Diagnostic
progress and panic files use the same output basename. The harness first checks
actual InputState list splitting/incrementing/empty exit, undo/redo, mode-change
selection/history retention, ordinary selection replacement, pasted newlines,
disabled input, and IME marked-text replacement. It also checks rendered click
round trips at every UTF-8 character boundary in a fixture containing hidden
emphasis markers, bullets, Unicode, and wrapped headings, plus selection paths
and composition bounds. Those checks passed.

`cargo test --locked` exercises encoding/save failures and the actual patched
parser, wrapper, and height index via a small integration target. `cargo check
--locked`, `cargo fmt --check`, and rustfmt checks on the edited vendor modules
also pass.

Native OS candidate-window placement, real clipboard interaction, drag selection,
and visual typography still warrant a manual desktop smoke test. The automated
IME checks exercise buffer/composition paths rather than a physical IME session.
Long-line parsing and wrapping remain proportional to the edited source line;
only shaping and painting are segmented. Timing claims apply to these recorded
fixtures and hardware, not arbitrary line lengths or machines.

## Five-improvement validation

The follow-up release run is recorded in [improvements.csv](improvements.csv).
It uses the same fixtures and timing method. Editor diagnostics now also check
Reading-view task offsets after Unicode text, task Enter in both editable modes,
checkbox click/undo in a nested quote, and quote geometry across 16/20 px fonts,
light/dark themes, scrolling, and source-line marker reveal. Autosave is disabled
for the synthetic benchmark documents so they never write to disk.

The separate notebook smoke check dispatches actual GPUI Ctrl+O, Up/Down, Enter,
and Escape keystrokes, checking file activation, folder expansion, and restored
focus. The autosave smoke check exercises actual debounce timers and asynchronous
atomic writes, typing during a save, queued manual saves and navigation, discard,
BOM/CRLF preservation, invalid settings, nonmodal failure recovery, and Reading
view task autosave. All fixtures and notebook indexes are disposable.

Run the autosave check with an editor-diagnostics build:

```powershell
$env:MARKDOWN_NOTE_AUTOSAVE_CHECK = Join-Path $pwd 'target/autosave-smoke.txt'
& ./target/release/markdown-note.exe
Remove-Item Env:MARKDOWN_NOTE_AUTOSAVE_CHECK
```

The benchmark and keyboard smoke check need a rendered window; launching them
hidden can prevent frame-driven checks from running. Native picker buttons,
physical mouse interaction, and visual appearance still warrant a manual check.


## Editable tables (2026-09-10)

The release run in [tables.csv](tables.csv) retains the existing fixtures and
adds a 10,000-body-row GFM table with Unicode and inline emphasis. It uses the
same 60-operation groups, six settling frames, and next-frame p95 measurement.
The final run measured large-table edits at 11.306 ms p95 and scrolling at
6.552 ms p95. The existing long-line edit case measured 41.771 ms p95; all edit
and scroll groups remain below the 50 ms / 33 ms targets. Cold table parsing,
mode switches, and structural row/column commands are not timing claims.

The parsed-line column counts the existing line-local presentation parser;
it does not count GFM block-parser bytes. Integration tests separately assert
that ordinary edits outside tables and ordinary cell typing do not reparse the
whole document/table. Context-changing Markdown edits can conservatively rebuild
the block index. Documents without any pipes bypass GFM block parsing.

The frame-driven checks passed for table commands, stale revisions, isolated
undo/redo, disabled editing, IME replacement, wrapped cell hit testing, selections,
and 16/20 px fonts in both themes. Desktop inspection also exercised the table
context menu, row insertion, cell typing with a literal pipe, Tab navigation,
separate undo steps, and horizontal scrolling without moving surrounding text.
Native IME candidate-window placement remains a manual check.

To leave the disposable table fixture open for visual inspection, build with
editor-diagnostics and set both MARKDOWN_NOTE_BENCH to an output path and
MARKDOWN_NOTE_TABLE_PREVIEW=1 before launching. Autosave is disabled for this fixture.
The normal benchmark exits automatically; preview mode stays open. The separate
MARKDOWN_NOTE_AUTOSAVE_CHECK harness includes cell edits followed by row insertion.

## Sustained Enter/Backspace validation

The preview now updates source rows synchronously and performs document-level
link/table analysis on a background executor. Its queue is capped at one running
job and one latest pending snapshot. Viewport measurement does not invoke that
parser. Table rows and link offsets are queried through indexes, and unchanged
row layouts survive source shifts and background completion.

Run the automated, headless regression suite with:

```powershell
cargo test --locked --features editor-diagnostics
```

The suite covers 120 successive inserted lines followed by 120 removed lines at
the beginning, middle and end of 100,000-line notes; alternating edits in notes
with 10,000 links; and viewport/cache behavior in 10,000-row tables, including
blank lines immediately before a table. It also checks safe partial completions,
stale context/document/mode results, bounded queue size and copied bytes, and
incremental table metadata against a fresh GFM parse after Unicode, escaped-pipe,
reference-link, LF/CRLF and structural row edits. These checks do not open a window.

An additional **user-run** release benchmark uses timer-driven input independent
of frame completion. Set both variables before launching the diagnostic build:

```powershell
cargo build --release --locked --features editor-diagnostics
$env:MARKDOWN_NOTE_BENCH = Join-Path $pwd 'benchmarks/sustained-preview.csv'
$env:MARKDOWN_NOTE_BURST = '1'
& ./target/release/markdown-note.exe
Remove-Item Env:MARKDOWN_NOTE_BENCH, Env:MARKDOWN_NOTE_BURST
```

This benchmark opens its own diagnostic window and must be run manually; agents
must not launch it under this repository's desktop-testing restriction. It uses
30 and 60 key events/second, with two seconds of Enter followed by two seconds of
Backspace per group. Fixtures cover 100,000 plain lines, 10,000 links, and a
10,000-row table. For the table fixture, the middle group edits the blank line
immediately before the table; the other groups edit surrounding prose.

Warm-up waits for the actual parser and painted revision. Input does not wait for
a frame or for formatting to finish. CSV output includes synchronous key-handler
p95, scheduled-input-to-editor-paint p95, maximum paint gap, maximum analysis queue
size, unpainted edits, and instrumented UI copy/parse, table-row and cache counters.
The paint timestamp is captured inside the editor's paint method; it is not an
OS keyboard timestamp or a GPU presentation timestamp. The input-to-paint metric
includes missed timer deadlines so a growing event backlog cannot appear fast.

Acceptance targets remain edit-handler p95 below 4 ms and input-to-paint p95 below
16.7 ms, with no unpainted edits or growing backlog. **New display-timing results
have not been recorded by the agent**, and earlier frame-spaced timings above do
not establish these targets. Physical key-repeat behavior and visual stability
remain manual validation. As before, exceptionally long individual source lines
have line-proportional parsing/wrapping costs; document-size stress results are
not a constant-time guarantee for arbitrary single-line input.