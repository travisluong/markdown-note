# Markdown Note terminal integration patches

Upstream: gpui-terminal 0.1.0, downloaded from crates.io.
Repository: https://github.com/zortax/gpui-terminal
Keep LICENSE-MIT, LICENSE-APACHE and this log when upgrading.

Markdown Note pins GPUI 0.2.2 and selects this copy with [patch.crates-io].
The upstream example and unused clipboard helper are retained as references;
Markdown Note uses GPUI clipboard access and never launches the example application.

Local changes:
- Share normalized grid dimensions across construction, explicit resize and canvas
  layout, enforcing Alacritty's two-column/one-row minimum. One-column grids can
  panic on wide Unicode output or fail to finish reflowing existing wide cells.
  Hit testing and PTY resize callbacks report the same safe dimensions; tiny
  panels clip the grid and zero-size view resizes remain ignored. Root
  tests/terminal_resize.rs and application sidebar tests cover these limits.
- Add from_receiver for Markdown Note-owned PTY I/O. Bound the standalone stream
  constructor's queue; process at most 64 KiB per UI turn and yield between
  batches. Drain protocol events while hidden rather than only during paint.
- Forward Alacritty PtyWrite replies (including cursor-position queries).
  Bell/OSC clipboard requests are discarded by the embedded byte path;
  they cannot mutate the host clipboard or accumulate while hidden.
- Emit TerminalTitleChanged for OSC 0/2 and resets during output processing,
  including hidden views. Retain only the latest pending title for the existing
  window-dependent callback. Markdown Note subscribes with session/view/generation
  guards and falls back to the shell label for empty titles.
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

Deferred upstream gaps: scrollback navigation, mouse
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
  underline. The component keeps Box as its compatibility default; Markdown Note
  explicitly selects Line unless its notebook/session preference says otherwise.
- Keep SHOW_CURSOR visibility control; application shape requests do not override
  host configuration. cursor_quad exposes the same paint primitive used by paint
  for headless geometry/visibility checks in the root terminal integration suite.

Terminal completion keys:
- Register Tab/Shift+Tab NoAction bindings once per app in the Terminal context.
  This suppresses ancestor focus traversal before raw key dispatch, allowing the
  existing guarded key handler to send HT/backtab to the shell. Other contexts
  retain normal focus navigation; disabled or modal-blocked terminals send nothing.

Multiline prompt key:
- Encode Shift+Enter as CSI 13;2u instead of collapsing it into Enter's CR.
  This is an explicit mapping for prompt editors that recognize CSI-u; it neither
  negotiates nor advertises full kitty keyboard support. Unmodified Enter and
  Ctrl+J retain CR/LF; other modifier combinations retain their existing behavior.
- Encoding follows https://sw.kovidgoyal.net/kitty/keyboard-protocol/ (Enter=13,
  Shift modifier=2). TestPlatform verifies exact bytes, focus, unchanged document
  contents, and modal/disabled-input guards.

Terminal selection and clipboard:
- Use Alacritty simple selections and selection_to_string for visible-grid drag
  selection, wrapped-line extraction, wide characters and combining characters.
  Hit testing shares the last painted cell geometry, including padding and font
  changes. Window pointer listeners finish drags outside the clipped grid.
- Paint selection backgrounds and contrasting glyphs. Copy clears selection only
  after writing text through GPUI; empty selections preserve the clipboard.
- Windows/Linux Ctrl+C copies a selection or sends interrupt, Ctrl+V pastes, and
  Ctrl+Shift+C/V are explicit alternatives. macOS uses Cmd+C/V and retains Ctrl+C.
  Terminal-scoped NoAction bindings prevent ancestor shortcuts taking these keys.
- Expose guarded clipboard methods and selection availability for host menus.
  The host interaction guard remains effective after exit, independently of the
  input-enabled flag, so retained output supports copy while writes are disabled.
  Markdown Note owns popup presentation and stale session/menu validation. Existing
  paste normalization, bracketed-paste framing, limits and error handling remain.
- Root terminal integration and application TestPlatform tests exercise pointer
  selection, clipboard shortcuts, geometry, menus and stale/modal authority.

Window zoom shortcuts join Ctrl+backtick as host-reserved keys. Mod+Plus/Equals
(including shifted variants), Mod+Minus and Mod+0 bubble to the application before
terminal encoding; Mod is Cmd on macOS and Ctrl elsewhere. Other modifiers and
shell keys retain their existing behavior. Markdown Note passes scaled font/padding
configuration while preserving saved terminal font overrides. The application
`zoom_ui` tests verify focus, unchanged view identity and no shell bytes for zoom.
