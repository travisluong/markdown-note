# Mithril terminal integration patches

Upstream: gpui-terminal 0.1.0, downloaded from crates.io.
Repository: https://github.com/zortax/gpui-terminal
Keep LICENSE-MIT, LICENSE-APACHE and this log when upgrading.

Mithril pins GPUI 0.2.2 and selects this copy with [patch.crates-io].
The upstream example and unused clipboard helper are retained as references;
Mithril uses GPUI clipboard access and never launches the example application.

Local changes:
- Add from_receiver for Mithril-owned PTY I/O. Bound the standalone stream
  constructor's queue; process at most 64 KiB per UI turn and yield between
  batches. Drain protocol events while hidden rather than only during paint.
- Forward Alacritty PtyWrite replies (including cursor-position queries).
  Title/bell/OSC clipboard requests are discarded by the embedded byte path;
  they cannot mutate the host clipboard or accumulate while hidden.
- Isolate terminal keys with a Terminal key context and stop propagation.
  Reserve Ctrl+backtick for the host; check a host input guard at key dispatch.
- Add explicit paste with newline normalization, bracketed-paste wrapping,
  removal of embedded ESC, Ctrl+Shift+V / Cmd+V, and input error presentation.
- Ignore zero-size geometry, clip to the panel, report actual grid dimensions,
  and notify resize callbacks for explicit resize. Use the configured background.
- Honor inverse/hidden cells, combining characters and cursor visibility; draw
  an outline cursor so the cell glyph remains visible.

The application owns shell selection, launch errors, stop/restart/exit status,
background reader/writer/controller threads, bounds on I/O and process reaping.
The view never quits the application on shell exit.

Deferred upstream gaps: mouse selection/copy, scrollback navigation, mouse
reporting, terminal search and advanced keyboard/IME protocols. This remains a
single-terminal trial, not a claim of VS Code feature parity.

Verification: root tests/terminal.rs exercises both the component and actual
headless PTYs. Application TestPlatform tests cover focus, shortcuts, modal
authority, retention, settings and stale session completion. Re-run those tests
and all architecture verification commands when upgrading; never patch Cargo's
registry cache.
