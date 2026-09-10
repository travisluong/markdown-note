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
- **File → Settings**: adjust content font size (10–40 px), UI font size (10–24 px), and dark mode. Changes apply immediately and are saved per notebook. Without an active notebook, settings apply for the session only.
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
navigate the sidebar and immediately load selected Markdown files in the current view mode, keeping keyboard focus in the sidebar. Left/Right collapse/expand and Enter activates a row.
Other files and filesystem links are displayed but cannot be opened. Empty folders
remain visible. Use **Refresh** to pick up external changes; there is no file watcher.

Each notebook stores appearance preferences and its last selected file and expanded
folders in `.gpui-note/settings.json`; file and folder paths are notebook-relative. This internal folder is hidden from the sidebar.
`%APPDATA%\gpui-note\notebooks.json` stores only the notebook paths and active
notebook, so the menu and active notebook can be restored on launch. Settings use
atomic writes. Missing notebooks remain in the menu; corrupt or unsupported
settings are reported and preserved. Manage notebooks lists notebook names and full folder paths;
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
Appearance preferences default to 16 px for both font sizes and the startup system
theme. Switching notebooks restores their preferences; leaving a notebook restores
the separate session preferences. The smoke check also exercises appearance
restoration, font limits, and removal without deleting files.

Native folder-picker interaction and the save prompt's actual buttons still need
a manual smoke test.

Live preview recognizes one to six leading `#` characters immediately, even
without a space. Heading hashes and separating whitespace hide off the cursor source line and reappear on that line. `- ` becomes a bullet and `1. ` starts
an ordered item. Task lists also support indentation and quote prefixes. Matched `**bold**`,
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

Ctrl+O searches all Markdown files and folders in the active notebook, including collapsed folders. Results show relative paths; Up/Down selects, Enter opens, and Escape returns focus. File > Open still opens external files; without a notebook Ctrl+O uses that picker.

Task checkboxes are clickable in Live preview and Reading view, including [X]. Toggles are undoable and preserve the rest of the Markdown. Enter continues a task unchecked in either editable mode; Enter on an empty task removes its marker and moves to the next line.

Autosave is enabled by default after 1,000 ms without edits. Settings offers an enable switch and a delay from 100 to 60,000 ms; invalid input keeps the previous setting. Preferences persist per notebook, while external files use session preferences. Saves preserve BOM and newline conventions, allow continued typing, and serialize with manual saves and navigation. Failures leave changes dirty with a nonmodal error; edit again or Save to retry.

The opt-in GPUI_NOTE_AUTOSAVE_CHECK diagnostic (editor-diagnostics build) exercises timers, save races, failure recovery, task autosave, and validation against disposable files.

Live preview renders explicitly marked blockquotes with muted text, indentation, and one vertical bar per nesting level. Quote prefixes reappear on the cursor source line. Emphasis and clickable tasks work inside quotes; unmarked continuation lines remain literal under the line-local renderer.

Ctrl+P opens the command palette. **Templates: Insert current date** replaces the
remembered editor selection as one undoable edit in Source or Live preview.
Settings has General and Templates sections; the date format is saved per
notebook (or for the session without a notebook). Invalid edits show an inline
error and keep the last valid format. The default is `YYYY-MM-DD`.

Date templates use local time and English names. Supported Moment-style tokens:
`YY YYYY M MM MMM MMMM D DD Do d dd ddd dddd H HH h hh m mm s ss SSS A a Z ZZ`.
Use `[literal text]` or a backslash before a character to escape it. Empty formats,
unsupported alphabetic tokens, unmatched brackets, nested brackets, and trailing
backslashes are rejected. This is a supported subset of
[Moment formatting](https://momentjs.com/docs/#/displaying/format/), translated to
[Chrono formatting](https://docs.rs/chrono/latest/chrono/format/strftime/index.html).

Ctrl+L in the focused Source or Live preview document editor creates or toggles
tasks on the current line or every selected line. A final line touched only at
its starting boundary is excluded. Existing tasks toggle independently; other
list items gain `[ ] `, and plain or empty lines gain `- [ ] `. Indentation,
quote prefixes, ordered/unordered markers, Unicode, and selection direction are
preserved. The entire selection changes in one undoable edit.


Live preview renders top-level GFM pipe tables with editable header and body
cells. Click a cell to edit its inline Markdown; inactive cells show formatting.
Tab/Shift+Tab move between cells, Enter moves down a column, and Tab or Enter
from the last row adds a body row when needed. Arrow keys cross cell boundaries,
Home/End stay within a cell, and Escape exits the table. At the document edges,
exiting creates a blank line as an undoable edit.

Right-click a table cell to insert/delete rows or columns, set column alignment,
or delete the table. The header row and final column cannot be removed through
the row/column commands. Horizontal scrolling (or Shift+wheel) moves wide tables
without scrolling the surrounding document. Cell edits and structural commands
use the same undo history and autosave as ordinary text. Literal pipes are
escaped, and pasted line breaks within a cell become spaces. Missing cells in
uneven rows are added to the Markdown when entered. Nested/container tables,
merged cells, formulas, and rectangular spreadsheet paste are not supported by
the live table editor; Source mode remains available for literal Markdown.
