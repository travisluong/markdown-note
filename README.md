# GPUI Note

A small Windows Markdown app built with GPUI. One window, one document.

## Run

Use stable Rust with the Windows MSVC toolchain, Visual Studio C++ Build Tools,
and a Windows SDK installed:

```powershell
cargo run --locked
```

## Download a build from GitHub Actions

The **Build desktop binaries** workflow builds release executables for Windows
x86-64, Linux x86-64, and macOS (separate Apple Silicon and Intel builds).
It runs when a GitHub release is published (including pre-releases), or manually
from **Actions**. Ordinary pushes, tag pushes, and pull requests do not generate
builds. Publish a release whose tag includes this workflow to start a release
build. Push the workflow to GitHub before using it; manual
**Run workflow** becomes available once the workflow is on the default branch.

Open the repository's **Actions** tab, select **Build desktop binaries**, and
open a successful run. Under **Artifacts**, download the build matching your
computer. Extract the downloaded artifact ZIP, then extract the archive inside:

| Artifact | Contents |
| --- | --- |
| `gpui-note-windows-x86_64` | ZIP containing `gpui-note.exe` |
| `gpui-note-linux-x86_64` | tar.gz containing `gpui-note` |
| `gpui-note-macos-apple-silicon` | ZIP containing `GPUI Note.app` for M-series Macs |
| `gpui-note-macos-intel` | ZIP containing `GPUI Note.app` for Intel Macs |

On macOS, extract `GPUI Note.app`, move it to Applications if desired, and open it
from Finder or the Dock; Terminal is not needed. The app is not Developer ID-signed
or notarized. If macOS blocks the first launch, follow
[Apple's instructions to open an app from an unknown developer](https://support.apple.com/guide/mac-help/mh40616/mac)
using System Settings → Privacy & Security → Open Anyway.

Run `gpui-note.exe` on Windows or `./gpui-note` from a terminal on Linux.
Artifacts are retained for 14 days. Downloads do not include installers or `.dmg` images.
The Linux build uses Ubuntu 22.04 and needs compatible system libraries and a
working graphics driver; it is not a self-contained AppImage. The workflow
checks compilation and the macOS bundle's property list and executable permissions,
not interactive UI behavior.

## Use

On macOS, application shortcuts use **Command** in place of **Ctrl** below.
Menu shortcuts use **Control+Option+F / V / N / H**, leaving Option-letter
combinations available for typing. Help → Keyboard shortcuts shows platform-specific bindings.

- **File → Open…** (`Ctrl+O`): open a UTF-8 `.md` or `.markdown` file.
- **File → Settings**: adjust content font size (10–40 px), UI font size (10–24 px), and themes. The Theme section offers independently customizable Light and Dark palettes, background/font/primary color pickers, and a button to reset both palettes. Changes apply immediately and are saved per notebook. Without an active notebook, settings apply for the session only.
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
The user configuration file stores only the notebook paths and active
notebook, so the menu and active notebook can be restored on launch. Settings use
atomic writes. Missing notebooks remain in the menu; corrupt or unsupported
settings are reported and preserved. Manage notebooks lists notebook names and full folder paths;
Remove forgets a notebook without deleting its folder or files. Removing the active
notebook closes it, with Save/Discard/Cancel protection for unsaved edits.

The configuration file is `%APPDATA%\gpui-note\notebooks.json` on Windows,
`~/Library/Application Support/gpui-note/notebooks.json` on macOS, and
`$XDG_CONFIG_HOME/gpui-note/notebooks.json` on Linux (falling back to
`~/.config/gpui-note/notebooks.json` when XDG_CONFIG_HOME is unset or not absolute).

An asterisk in the title means there are unsaved changes. Opening another file
or switching notebooks or closing prompts to Save, Discard, or Cancel. Failed opens/saves preserve the
current buffer. Saves write a temporary file beside the original before replacing it.

UTF-8 BOM and LF/CRLF/CR line endings are preserved. Mixed line endings are
normalized to the first observed convention on save. Files without line endings
use LF. The file picker shows all files; unsupported extensions are rejected.

There is no New, Save As, autosave, file watching, tabs, or database. Reading view
uses GPUI Component's built-in Markdown support. Source and Live preview share a rope buffer, selection, and undo history.

`Ctrl+F` finds literal text in the current document, including unsaved edits.
In Source and Live preview, selected text seeds the query. Matches are highlighted with an active-match
counter; Enter/Shift+Enter or the arrow buttons move between matches and wrap.
The **Match case** button toggles case sensitivity (case-insensitive matching
folds ASCII letters; non-ASCII characters match exactly). Escape closes find
and returns focus to the document. All three view modes support find, including
table text. Reading view highlights rendered text and scrolls to the active
match while keeping the selected view mode. Matches in hidden Markdown syntax
reveal their containing block and show a short explanation.
Ctrl+F also works from the notebook sidebar. Find state resets when another
document is loaded. In Source and Live preview, the find panel also offers
Replace one and Replace all. The command palette can open these controls directly;
replacement is unavailable in Reading view. Replacements use literal text and
form separate undo steps.

`Ctrl+Shift+F` or the **Search** toolbar button searches literal text across all
`.md` and `.markdown` files in the active notebook, including unsaved changes in
the open note. The sidebar search panel groups matches by file and shows line
numbers and highlighted snippets. **Match case** uses the same ASCII case rules
as Ctrl+F. Up/Down selects a result; Enter or a click opens it and selects the
match, with the usual Save/Discard/Cancel protection. Reading view reveals the
match in the rendered document. Opening a result preserves the search list,
query, selection, and scroll position without restarting the search. Escape leaves the sidebar query;
**Files** returns to
the notebook tree. Reopening Search retains its query and results until switching
notebooks. Without an active notebook, Search asks you to open one.

Search refreshes after edits to the open note or changes to the query or case
option. To pick up external file changes, change the query or toggle Match case.
Scans run in the background and superseded queries are cancelled.
Filesystem links and `.gpui-note` are excluded. Searches show up to 2,000 matches;
files over 8 MiB and unreadable/non-UTF-8 files are skipped with a visible count.
A changed match is rechecked against the loaded text before navigation.

Drag the sidebar’s right border to resize Explorer and Search together. Each
notebook saves its width in `.gpui-note/settings.json` (default 280 px, range
160–600 px). The layout keeps at least 200 px available for the editor.

The sidebar’s top icons open Explorer and notebook search. The toggle beside
File stays in the menu bar to collapse the sidebar or restore the previous
panel. Ctrl+Shift+F also reveals Search. Collapse state lasts for the session;
search queries and results remain available when switching panels.

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

Ctrl+P opens the searchable command palette. It includes:

- **Appearance: Toggle light/dark mode**, preserving each theme's custom colors.
- **Notebook: Change notebook**, opening a searchable list of registered notebooks
  with their paths and an active marker. Escape returns to the command list.
- **Notebook: Create new note**, creating an empty `Untitled.md` in the notebook
  root, or `Untitled 2.md`, `Untitled 3.md`, and so on when names are taken.
  Save/Discard/Cancel protection runs first. The new note opens for editing;
  Reading view switches to Source.
- **Search: Search current file**, available in all three view modes.
- **Search: Search in all files**, searching Markdown files in the active notebook.
- **Search: Search and replace in current file**, available in Source and Live preview.

Unavailable commands remain visible with an explanation. Existing date and link
commands are retained. **Templates: Insert current date** replaces the
remembered editor selection as one undoable edit in Source or Live preview.
Settings has General, Templates, and Theme sections; the date format is saved per
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

Each notebook remembers its selected view mode and restores it when reopened.
New notebooks and older settings without a saved mode start in Live preview.
Standalone sessions also start in Live preview. Search and Find never switch
view modes automatically. Mode selections are saved through the View menu
or the corresponding commands.

Markdown links render in Live preview, including reference links and autolinks.
Syntax reveals on the active line (or active table cell); Ctrl+click (Command+click on macOS) opens a
link without moving the selection. Reading view uses ordinary clicks. Hover
shows the destination. Code spans and code blocks do not activate links.
`Ctrl+K` or **Text: Insert or edit link** in the command palette opens the link
editor. Labels accept inline Markdown; edits preserve existing titles and form
one undo step. Editing a reference link converts that occurrence to an inline
link without changing its shared definition.

HTTP, HTTPS, and mailto links open through the system handler. Relative `.md`
and `.markdown` paths resolve against the current document and use the normal
save-before-navigation flow. Notebook links stay inside the current notebook;
filesystem symlinks/junctions are not traversed. Heading anchors, wiki links,
and links to other local file types are not included.

Explorer drag and drop moves files and folders into a destination folder or, when dropped onto the sidebar toolbar, into the notebook root. Name collisions and moves into a folder’s own descendants are rejected; open buffers keep their edits and follow the moved path.

Settings > Theme offers independent Light and Dark palettes with Background color, Font color, and Primary color pickers. Primary controls task checkboxes and links and remains independent when the font color changes. Reset Light and Dark themes restores both palettes without changing the active mode or font sizes. Existing custom colors are preserved in the previously selected palette; Dark Gray becomes a customized Dark theme.

The sidebar Sort files icon offers filename A–Z/Z–A, modified newest/oldest, and created newest/oldest ordering, saved per notebook. Folders always remain first in alphabetical order; sorting applies only to files within each folder. Unavailable timestamps sort last. Drag files or folders onto a folder to move them, or onto the sidebar toolbar to move them to the notebook root. Manual ordering and insertion targets are no longer supported; old custom ordering is ignored. Successful saves refresh timestamp ordering.

### Sidebar file extensions

Settings > General > Sidebar controls file-tree labels per notebook. **Show file extensions** is on by default; **Always hide these extensions** defaults to `md`, so `notes.md` appears as `notes` while `photo.png` keeps its extension. Turn the switch off to hide all final extensions; the exception list is disabled and preserved. Clear the list with the switch on to show every extension.

Enter comma-separated extensions such as `md, txt`. Matching ignores case, surrounding whitespace, optional leading dots, duplicates, and empty entries. Invalid paths, spaces within extensions, wildcards, and compound suffixes show an error and keep the last valid setting. Only the final suffix is hidden (`archive.tar.gz` becomes `archive.tar` when hiding `gz`). Extensionless names, dotfiles such as `.gitignore`, trailing dots, and folder names are preserved. Hover over a file for its full filename. Search results and actual file paths are unchanged. Changes apply immediately; without an active notebook, preferences last for the session only.
