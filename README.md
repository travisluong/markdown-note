# GPUI Note

A small Windows Markdown app built with GPUI. One window, one document.

## Run

Use stable Rust with the Windows MSVC toolchain, Visual Studio C++ Build Tools,
and a Windows SDK installed:

```powershell
cargo run --locked
```

## Use

- **File → Open…** (`Ctrl+O`): open a UTF-8 `.md` or `.markdown` file.
- **File → Save** (`Ctrl+S`): save back to the opened file.
- **Notebooks → Manage notebooks**: create a notebook or open an existing folder.
- **Notebooks → notebook name**: switch notebooks and restore its last selected note.
- **View → Source mode**: edit plain Markdown, with standard copy/paste and undo/redo.
- **View → Live preview**: edit headings, emphasis, and flat lists with reading typography.
- **View → Reading view**: render the current text, including unsaved edits.
- `Alt+F` / `Alt+V` / `Alt+N`: open menus; use arrow keys, Enter, and Escape inside them.

Creating a notebook makes a new folder named after the notebook inside the chosen
parent location. Names must be valid Windows folder names, without slashes.
Opening a folder uses its folder name as the notebook name. Existing folders
are never replaced by Create; use Open folder as notebook instead.

The sidebar displays folders and files recursively. Click a folder to expand it
and a `.md` or `.markdown` file to open it in the current view mode. Arrow keys
navigate the sidebar; Left/Right collapse/expand and Enter activates a row.
Other files and filesystem links are displayed but cannot be opened. Empty folders
remain visible. Use **Refresh** to pick up external changes; there is no file watcher.

Each notebook stores its last selected file and expanded folders as relative paths
in `.gpui-note/settings.json`. This internal folder is hidden from the sidebar.
`%APPDATA%\gpui-note\notebooks.json` stores only the notebook paths and active
notebook, so the menu and active notebook can be restored on launch. Settings use
atomic writes. Missing notebooks remain in the menu; corrupt or unsupported
settings are reported and preserved. Manage notebooks lists registered notebooks;
Remove forgets a notebook without deleting its folder or files. Removing the active
notebook closes it, with Save/Discard/Cancel protection for unsaved edits.

An asterisk in the title means there are unsaved changes. Opening another file
or switching notebooks or closing prompts to Save, Discard, or Cancel. Failed opens/saves preserve the
current buffer. Saves write a temporary file beside the original before replacing it.

UTF-8 BOM and LF/CRLF/CR line endings are preserved. Mixed line endings are
normalized to the first observed convention on save. Files without line endings
use LF. The file picker shows all files; unsupported extensions are rejected.

There is no New, Save As, autosave, file watching, tabs, or database. Reading view
uses GPUI Component's built-in Markdown support. Source and Live preview share a rope buffer, selection, and undo history.

## Check

```powershell
cargo check --locked
cargo test --locked
cargo fmt --check
```

The tests cover Unicode, empty files, BOM/newline round trips, dirty state,
invalid/missing files, and Windows replacement failures preserving the original.

For a UI smoke test, open a disposable Markdown file, edit it, switch both views,
save, and reopen. Check menu selection marks, keyboard navigation, scrolling,
clipboard, undo/redo, and each Save/Discard/Cancel branch on open and close.
Also check invalid UTF-8 and a locked/read-only file.

Notebook tests cover folder names, creation collisions, persistence, duplicate
paths, corrupt settings, missing folders, recursive listing, and Windows junctions.
An opt-in in-app smoke check exercises the modal screens, sidebar, view modes,
save-prompt response handling, notebook switches, and restoration using disposable
fixtures and an isolated index:

```powershell
cargo build --locked --features editor-diagnostics
$env:GPUI_NOTE_NOTEBOOK_CHECK = Join-Path (Get-Location) 'target/notebook-smoke.txt'
Start-Process target/debug/gpui-note.exe -WindowStyle Hidden -Wait
Remove-Item Env:GPUI_NOTE_NOTEBOOK_CHECK
Get-Content target/notebook-smoke.txt
```

The check exits automatically and writes a result, progress file, or panic report.
Native folder-picker interaction and the save prompt's actual buttons still need
a manual smoke test.

Live preview recognizes one to six leading `#` characters immediately, even
without a space. Heading hashes and separating whitespace hide off the cursor source line and reappear on that line. `- ` becomes a bullet and `1. ` starts
an ordered item. Only unindented lists are supported. Matched `**bold**`,
`__bold__`, `*italic*`, and `_italic_` delimiters disappear off the cursor's source
line and reappear on that line. Unsupported syntax stays literal.

Enter splits a list item and continues its marker, incrementing an ordered
marker without renumbering other items. Enter on an empty list item clears that
line. Each list Enter is one undo step. Selections and pasted newlines use normal
replacement behavior. Saving and copying always use the original Markdown.

The pinned component is patched locally in `vendor/gpui-component`; see
[the patch notes](vendor/gpui-component/EDITOR-PATCH.md).
See [performance validation](benchmarks/README.md) for the release benchmark,
fixtures, hardware, limitations, and reproduction commands.
