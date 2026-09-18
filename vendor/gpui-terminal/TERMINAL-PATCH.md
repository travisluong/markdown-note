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

Cursor configuration extension:
- Expose CursorStyle (Line, Box, Underline) in TerminalConfig and the renderer;
  construction and update_config both apply it without resetting terminal state.
- Paint a thin vertical line, the existing transparent box outline, or a thin
  underline. The component keeps Box as its compatibility default; Mithril
  explicitly selects Line unless its notebook/session preference says otherwise.
- Keep SHOW_CURSOR visibility control; application shape requests do not override
  host configuration. cursor_quad exposes the same paint primitive used by paint
  for headless geometry/visibility checks in the root terminal integration suite.

Terminal completion keys:
- Register Tab/Shift+Tab NoAction bindings once per app in the Terminal context.
  This suppresses ancestor focus traversal before raw key dispatch, allowing the
  existing guarded key handler to send HT/backtab to the shell. Other contexts
  retain normal focus navigation; disabled or modal-blocked terminals send nothing.
