# Terminal trial

Use the terminal toolbar button, Ctrl+backtick (including macOS), or the
**Terminal: Toggle terminal** command to open the right panel. The first opening
starts one shell. The header contains a horizontally scrollable tab strip and a
fixed **+** button. Each tab has a stable shell-and-number label, such as
PowerShell 1, and an **×** button to close that session. New tabs capture the
current shell preference and starting directory and receive increasing numbers.
Selecting a tab restores its terminal and focus; other shells keep processing
output. Closing an active tab selects its right neighbor, otherwise its left.
Closing an inactive tab preserves the selected session. Closing the last tab hides
the panel; reopening starts a fresh shell.

The toolbar icon, Ctrl+backtick and toggle command hide or show the panel without
stopping any shell. Shell exit or launch failure leaves the output/status visible
until the tab is closed; use **+** to start another session.

The Terminal settings tab offers a Windows shell dropdown with PowerShell and
Git Bash. The choice is saved
per notebook, or retained only for the session without a notebook. It takes
effect on new terminal tabs, never on an existing shell.
PowerShell prefers pwsh on PATH then Windows PowerShell. Git Bash is detected
from Git for Windows installation metadata, standard locations and Git on PATH.
It must be installed separately; missing Git Bash is an error, not a fallback.
Other platforms use $SHELL, falling back to /bin/sh.

The Terminal tab also offers **Line**, **Box**, and **Underline** cursor styles.
Line is a thin vertical bar and is the default, including for existing notebooks
without a cursor preference. Box is the existing outline; Underline sits along
the bottom of the cell. Changes apply immediately without restarting the shell
or clearing output, for all tabs, including while the panel is hidden. Cursor
preferences use the same notebook/session scope as shell selection; switching
notebooks applies
the current notebook's cursor style. Applications can hide the cursor, but their
shape requests do not override the selected style. Cursor blinking is unchanged.

A shell starts in the active notebook, otherwise the current document's folder,
otherwise home. Switching notebooks or tabs preserves the existing shell and
directory. Each new terminal tab uses the current notebook's preference and
directory.
The panel width remains session-only and uses the existing resize geometry.

While terminal focus is active, keys go to the shell. Tab and Shift+Tab stay in
the terminal for shell completion and reverse completion (when supported by the
shell). Shift+Enter sends a distinct modified-Enter sequence (CSI-u) for multiline
prompts in applications that recognize it; ordinary Enter still sends carriage
return. Ctrl+C interrupts a
foreground command. Paste uses Ctrl+Shift+V on Windows/Linux and Cmd+V on macOS,
respecting bracketed-paste mode. Pasting is limited to 1 MiB; a stalled input
queue reports an error. Modal dialogs block input but output continues.
The theme, monospace font and content font size follow notebook appearance.

The trial does not provide selection/copy, scrollback navigation, terminal
search, mouse reporting, shell integration, splits, custom
shell paths or session restoration across app restarts. Full advanced keyboard
protocols and IME composition are not supported by this component.

## Ownership and shutdown

- src/terminal.rs owns shell resolution and PTY I/O. Output is bounded to
  64 x 4 KiB chunks; input is bounded to eight requests of at most 1 MiB.
  Reader, writer and controller workers keep blocking OS operations off GPUI.
- src/terminal_ui.rs owns terminal tab identities, retained per-session views,
  status, launch generations, subscriptions, scrolling and pending cleanup tasks.
  Late launch results cannot install into a closed session or steal focus.
  A disconnected completion channel broadcasts cleanup independently of the
  exit-status consumer.
- src/right_panel_ui.rs coordinates visibility, tab selection, input authority
  and captured launch preferences. Closing a tab stops it and keeps its cleanup
  task alive until even a pending launch has been stopped and reaped. Accepted
  document/window close signals every live shell, then awaits all open-session
  and already-closing-session cleanup before removing the window. Cancelling
  save/close leaves every terminal intact.
- Notebook settings add terminal.windows_shell with powershell as the
  compatibility default. Existing ordered settings writes and form-scope guards
  apply; the preference survives opening the notebook on non-Windows systems.

Windows uses ConPTY. Unix uses cancellable nonblocking descriptors and terminates
the PTY foreground group when its tab closes before reaping the shell. Deliberately
detached jobs are outside the session lifecycle contract.

## Validation

Run the commands in ARCHITECTURE.md. tests/terminal.rs adds headless shell and
component coverage; Git Bash's scenario skips explicitly when it is not installed.
Windows sandbox restrictions can block Ctrl+C even though spawn/read/write work;
run real PTY tests outside that restriction. No test opens the Mithril application,
a terminal window, or injects desktop input. User validation should check native
font rendering, panel resizing, shell profiles and real interactive programs.
