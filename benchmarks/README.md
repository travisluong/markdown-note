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
$env:GPUI_NOTE_BENCH = Join-Path $pwd 'benchmarks/live-preview.csv'
& ./target/release/gpui-note.exe
# The benchmark closes its own window when done.
Remove-Item Env:GPUI_NOTE_BENCH
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
