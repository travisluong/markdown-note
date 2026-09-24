# Mithril

A desktop Markdown notebook built with GPUI, with Windows as the primary
platform. One window shows one note or image at a time. Notes are ordinary UTF-8
files; Source, Live preview and Reading provide three ways to work with them.

## Run on Windows

Install Rust through rustup, Visual Studio C++ Build Tools, and a Windows SDK.
Use the Windows MSVC toolchain. The repository selects Rust 1.98.1 through
[`rust-toolchain.toml`](rust-toolchain.toml):

```powershell
cargo run --locked
```

## Download a desktop build

Windows releases are tested, built, packaged and published from a local Windows
machine with `./release.sh vMAJOR.MINOR.PATCH` in Git Bash, or
`./release.ps1 -Tag vMAJOR.MINOR.PATCH` in PowerShell. The command builds an isolated
snapshot of that local tag; its version must match Cargo metadata. See the
[release setup and procedure](packaging/windows/README.md) for prerequisites,
credentials and `-BuildOnly`. Publishing a release does not use GitHub Actions.
Other platforms currently have no releases.

For Windows, open [Releases](https://github.com/travisluong/mithril-releases/releases)
and download `mithril-<version>-windows-x86_64-setup.exe` under **Assets** once
the build finishes. Run it and choose **Install**. It installs for your Windows
account without requiring administrator privileges, adds Mithril to the Start
menu and **Settings → Apps → Installed apps**, and offers an optional desktop
shortcut and launch on completion. The default location is
`%LOCALAPPDATA%\Programs\Mithril`.

To upgrade, save your notes, close Mithril, and run the newer installer.
Uninstall through Installed apps; your notebooks and preferences are retained.
The app and installer are currently unsigned, so Windows may display a
SmartScreen warning. Automatic updates are not included yet.

The release also includes `mithril-<version>-windows-x86_64.zip` and
`SHA256SUMS.txt` with SHA-256 checksums for the installer and portable ZIP.
For the portable Windows build, extract its ZIP and run `mithril.exe`.
The local release command creates a draft in the public downloads repository,
verifies every uploaded asset, and publishes only after verification succeeds.
Reruns resume matching drafts and leave published releases unchanged.
Windows installer build instructions and manual
checks are in [`packaging/windows/README.md`](packaging/windows/README.md).
Headless tests and packaging checks do not establish interactive UI or
install/upgrade/uninstall behavior.

## Get started

1. Use **Notebooks → Manage notebooks** to create a notebook or open an existing
   folder. Alternatively, **File → Open…** opens an existing `.md` or `.markdown`
   file without requiring a notebook.
2. Press **Ctrl+O** to find a Markdown note or folder anywhere in the active
   notebook, including collapsed folders. Results show relative paths; Up/Down
   selects, Enter opens, and Escape returns focus. Without an active notebook,
   Ctrl+O opens the file picker instead. **File → Open…** always uses that picker.
3. Press **Ctrl+P** and choose **Notebook: Create new note** to create an empty
   `Untitled.md` in the notebook root. Existing names are preserved by using
   `Untitled 2.md`, `Untitled 3.md`, and so on. Save/Discard/Cancel protection runs
   first; the new note opens for editing, switching Reading view to Source.
4. Choose **View → Source mode**, **Live preview**, or **Reading view**. Autosave
   is enabled by default; **File → Save** or **Ctrl+S** saves immediately.

On macOS, application shortcuts use **Command** in place of **Ctrl** above.
Menu shortcuts are **Control+Option+F / V / N / H**, leaving Option-letter
combinations available for typing. On Windows/Linux, use **Alt+F / V / N / H**
for File, View, Notebooks and Help; arrows, Enter and Escape navigate menus.
**Help → Keyboard shortcuts** lists bindings for the current platform, including
editor-specific differences such as Command+Shift+Z for redo on macOS.

Mermaid code blocks render offline in Live preview and Reading. See
[Mermaid diagrams](docs/mermaid.md) for syntax, editing and compatibility.

## Notebooks and Explorer

Creating a notebook makes a new folder with the chosen name inside its parent
location. Names must be valid Windows folder names, without slashes. Existing
folders are never replaced by Create; use **Open folder as notebook** instead.
An opened folder uses its folder name as the notebook name.

The Notebooks menu switches between registered notebooks and restores the last
selected note. Manage notebooks shows their names and full paths. Remove forgets
a notebook without deleting its folder or files. Removing the active notebook
closes it, with Save/Discard/Cancel protection for unsaved edits. Missing notebooks
remain in the menu; corrupt or unsupported settings are reported and preserved.

Explorer displays folders and files recursively. Click a folder to expand it or
a Markdown file to open it in the current view mode. Supported image files open
in an image viewer. Other file types and filesystem links are displayed but
cannot be opened. Empty folders remain visible. Up/Down moves through rows and
opens supported selected files while retaining sidebar focus; Left/Right
collapses or expands folders, and Enter activates a row. Use **Refresh** to pick
up external changes; there is no file watcher.

The sidebar's top icons switch between Explorer and notebook Search. The toggle
beside File collapses the sidebar or restores its previous panel; Ctrl+Shift+F
also reveals Search. Collapse state lasts for the session, and switching panels
retains search queries and results. Drag the right border to resize both panels.
Each notebook saves the width, defaulting to 280 px with a 160–600 px range; the
layout keeps at least 200 px available for the editor.

The **Sort files** icon offers filename A–Z/Z–A, modified newest/oldest, and created
newest/oldest ordering, saved per notebook. Folders remain first in alphabetical
order; sorting applies to files within each folder. Unavailable timestamps sort
last, and successful saves refresh timestamp ordering. Manual ordering and
insertion targets are not supported; old custom ordering is ignored.

Drag files or folders onto a folder to move them, or onto the sidebar toolbar to
move them to the notebook root. Name collisions and moves into a folder's own
descendants are rejected. Open buffers retain their edits and follow the moved path.

### File extension labels

**Settings → General → Sidebar** controls file-tree labels. **Show file
extensions** is on by default; **Always hide these extensions** defaults to `md`,
so `notes.md` appears as `notes` while `photo.png` retains its extension. Turn the
switch off to hide all final extensions; the exception list is disabled and
preserved. Clear the list with the switch on to show every extension.

Enter comma-separated extensions such as `md, txt`. Matching ignores case,
surrounding whitespace, optional leading dots, duplicates and empty entries.
Invalid paths, spaces within extensions, wildcards and compound suffixes show an
error and retain the last valid setting. Only the final suffix is hidden:
`archive.tar.gz` becomes `archive.tar` when hiding `gz`. Extensionless names,
dotfiles such as `.gitignore`, trailing dots and folder names are preserved.
Hover over a file for its full filename. Search results and actual paths are
unchanged. Preferences apply immediately and persist per notebook; without an
active notebook, they last for the session.

## Editing and view modes

**Source** edits literal Markdown. **Live preview** edits the same text while
rendering supported Markdown around the active line or block. These modes share
the buffer, selection and undo history. **Reading** renders the current text,
including unsaved edits. Source and Live preview copy their source selection;
Reading copies selected rendered text. Saving always uses the Markdown source.

Each notebook remembers its selected view mode. New notebooks and older settings
without a saved mode start in Live preview, as do standalone sessions. Find and
notebook Search keep the selected mode. Use the View menu to change modes.

### Headings, lists, tasks and quotes

Live preview recognizes one to six leading `#` characters immediately, even
without a space. Heading hashes and separating whitespace hide outside the
cursor's source line. Matched `**bold**`, `__bold__`, `*italic*` and `_italic_`
delimiters also hide off the active source line and reappear for editing.
Unsupported syntax remains literal.

List markers render as bullets or ordered items. Enter splits a list item and
continues its marker, incrementing an ordered marker without renumbering other
items. Enter on an empty list item clears that line. Each list Enter is one undo
step; selections and pasted newlines use normal replacement behavior.
Tab/Shift+Tab nests or outdents list items with their descendants and continuation
lines. Table navigation takes precedence when the cursor is in a table.

Task checkboxes are clickable in Live preview and Reading, including `[X]`.
Toggles are undoable and preserve the remaining Markdown. Enter continues a task
unchecked in either editable mode; Enter on an empty task removes its marker and
moves to the next line. Task controls stay visible on the active Live-preview line.

**Ctrl+L**, with the document editor focused in Source or Live preview, creates
or toggles tasks on the current line or selected lines. A final line touched only
at its starting boundary is excluded. Existing tasks toggle independently;
other list items gain `[ ] `, and plain or empty lines gain `- [ ] `. Indentation,
quote prefixes, ordered/unordered markers, Unicode and selection direction are
preserved. The selection changes in one undoable edit.

Live preview renders explicitly marked blockquotes with muted text, indentation
and one vertical bar per nesting level. Quote prefixes reappear on the cursor's
source line. Emphasis and tasks work inside quotes; unmarked continuation lines
remain literal under the line-local renderer.

### Tables and fenced code

Live preview renders top-level GFM pipe tables with editable header and body
cells. Click a cell to edit its inline Markdown; inactive cells show formatting.
Tab/Shift+Tab moves between cells, Enter moves down a column, and Tab or Enter
from the last row adds a body row when needed. Arrow keys cross cell boundaries,
Home/End stays within a cell, and Escape exits the table. At document edges,
exiting creates a blank line as an undoable edit.

Right-click a cell to insert/delete rows or columns, set column alignment, or
delete the table. The header row and final column cannot be removed through the
row/column commands. Horizontal scrolling or Shift+wheel moves wide tables
without scrolling the surrounding document. Cell edits and structural commands
share ordinary undo and autosave. Literal pipes are escaped; pasted cell line
breaks become spaces. Missing cells in uneven rows are added when entered.
Nested/container tables, merged cells, formulas and rectangular spreadsheet
paste are not supported by the live table editor; Source remains available.

Reading and Live preview highlight fenced code using the fence's language tag.
Unknown languages use plain text. In Live preview, inactive fenced blocks hide
their fence rows; moving the caret or selection into a block reveals its source.
Code contents remain literal rather than receiving Markdown emphasis or task styling.

### Links

Markdown links render in Live preview, including reference links and autolinks.
Links stay rendered and underlined while you type elsewhere, including on the
same line or in the same table cell. A link reveals its Markdown only when the
caret touches or enters it. **Click** opens the destination without moving the
selection in Live preview; Reading also uses ordinary clicks.
Hover shows the destination. Code spans and blocks do not activate links.

**Ctrl+K** or **Text: Insert or edit link** in the command palette opens the link
editor. Labels accept inline Markdown; edits preserve existing titles and form
one undo step. Editing a reference link converts that occurrence to an inline
link without changing its shared definition.

HTTP, HTTPS and mailto links open through the system handler. Relative `.md` and
`.markdown` paths resolve against the current document and use normal
save-before-navigation protection. Links from a note inside a notebook stay
inside that notebook; filesystem symlinks/junctions are not traversed. Heading
anchors, wiki links and links to other local file types are not supported.

### Images

Explorer opens PNG, JPEG, GIF, WebP and BMP files in an image viewer that fits the
image within the available space. Source retains image Markdown; Reading and
Live preview resolve relative image paths against the note. Live preview
renders standalone image paragraphs, retains the image while revealing its active
source line, and shows a placeholder when loading fails.

Drop supported image files into a note in Source or Live preview to insert image
Markdown. The note must be inside the active notebook. Files are copied into the
notebook root with available filenames; files already directly in that root are
reused. The insertion is one undoable edit and does not replace previously
selected text. If the document changes before the import finishes, the drop is
cancelled and newly copied files are rolled back. Undo removes inserted Markdown;
files from an accepted import remain in the notebook.

## Find and notebook Search

**Ctrl+F** finds literal text in the current document, including unsaved edits
and table text. In Source and Live preview, selected text seeds the query.
Matches have highlights and an active-match counter; Enter/Shift+Enter or the
arrow buttons move through them and wrap. **Match case** toggles sensitivity:
case-insensitive matching folds ASCII letters, while non-ASCII characters match
exactly. Escape closes Find and returns focus to the document. Ctrl+F also works
from the notebook sidebar; Find state resets when another document loads.

Reading Find highlights rendered text and scrolls to the active match while
keeping Reading mode. Matches in hidden Markdown syntax reveal their containing
block and show an explanation. Source and Live preview also offer **Replace one**
and **Replace all**, using literal text and separate undo steps. The command
palette can open replacement controls; Reading does not offer replacement.

**Ctrl+Shift+F** or the sidebar **Search** icon searches Markdown files across the
active notebook, including unsaved changes in its open note. Results group
matches by file with line numbers and highlighted snippets. Match case uses the
same ASCII rules as Find. Up/Down selects and reveals a result; Enter or a click
opens it with Save/Discard/Cancel protection. Reading reveals the rendered match.
Opening a result preserves the list, query, selection and scroll position.
Escape returns focus to the editor; **Files** returns to Explorer. Reopening
Search retains the query and results until switching notebooks. Without an
active notebook, Search asks you to open one.

Search refreshes after edits to the open note, discarding unsaved search content
when navigating away, or changes to the query or case option. To pick up external
file changes, change the query or toggle Match case.
Scans run in the background and superseded queries are cancelled. Filesystem
links and `.mithril` are excluded. Results are capped at 2,000 matches; files
over 8 MiB and unreadable/non-UTF-8 files are skipped with a visible count.
Changed matches are rechecked against the loaded text before navigation.

## Settings, commands and date templates

**File → Settings** has General, Font, Templates and Theme sections. Font offers
searchable installed-font lists for the interface, text and monospace roles.
Source uses the monospace font; Live preview and Reading use the text font.
Unavailable saved fonts fall back to defaults. Content font size ranges from
10–40 px and UI font size from 10–24 px; both default to 16 px.
Appearance starts with the system theme. Preferences apply immediately and are
saved per active notebook; without a notebook, appearance, date and sidebar
preferences last for the session. Switching notebooks restores their preferences,
and leaving a notebook restores the separate session preferences. Autosave uses
the separate scope described below for files outside the active notebook.

Theme offers independent Light and Dark palettes with Background, Font and
Primary color pickers. Primary controls task checkboxes and links independently
of font color. **Reset Light and Dark themes** restores both palettes without
changing the active mode or font sizes. Older custom colors are preserved in
the previously selected palette; Dark Gray migrates to a customized Dark theme.

**Ctrl+P** opens the searchable command palette. Unavailable commands remain
visible with an explanation:

| Command | Action |
| --- | --- |
| Appearance: Toggle light/dark mode | Switches themes while retaining each palette's custom colors. |
| Notebook: Change notebook | Lists registered notebooks with paths and an active marker; Escape returns to commands. |
| Notebook: Create new note | Creates an available Untitled filename in the active notebook root. |
| Search: Search current file | Opens Find in any document view mode. |
| Search: Search in all files | Searches Markdown files in the active notebook. |
| Search: Search and replace in current file | Opens replacement controls in Source or Live preview. |
| Templates: Insert current date | Replaces the remembered editor selection in one undoable edit in Source or Live preview. |
| Text: Insert or edit link | Opens the link editor in Source or Live preview. |

The date format is saved per notebook, or for the session without a notebook.
Invalid edits show an inline error and retain the last valid format. The default
is `YYYY-MM-DD`. Templates use local time and English names, supporting these
Moment-style tokens:

```text
YY YYYY M MM MMM MMMM D DD Do d dd ddd dddd H HH h hh m mm s ss SSS A a Z ZZ
```

Use `[literal text]` or a backslash before a character to escape it. Empty
formats, unsupported alphabetic tokens, unmatched/nested brackets and trailing
backslashes are rejected. This is a supported subset of
[Moment formatting](https://momentjs.com/docs/#/displaying/format/), translated to
[Chrono formatting](https://docs.rs/chrono/latest/chrono/format/strftime/index.html).

## Saving and on-disk storage

An asterisk in the title means unsaved changes. Autosave is enabled by default
after 1,000 ms without edits. Settings offers an enable switch and a delay from
100 to 60,000 ms; invalid input retains the previous setting. Autosave preferences
persist per notebook, while notes outside the active notebook use session
preferences. Writes allow continued typing and serialize with manual saves and
navigation. Ordinary write failures retain changes with a nonmodal error; edit
again or use **Save** to retry.

If the file changes, is replaced, or disappears on disk, saving reports a conflict
and pauses autosave for that document. Your text and undo history remain in the
editor. Copy any changes you want to keep before reopening the file, then merge
them with the disk version. **Save** explicitly retries the same version check;
it does not force an overwrite. Opening a fresh baseline or a successful explicit
save clears the pause.

Opening another file, switching notebooks or closing prompts to Save, Discard or
Cancel when needed. Failed opens/saves preserve the current buffer. Saves write
a temporary file beside the original and then replace it. UTF-8 BOM and
LF/CRLF/CR conventions are preserved. Mixed line endings normalize to the first
observed convention on save; files without line endings use LF. Unix permission
bits are preserved. Other metadata, including ownership, ACLs, extended attributes
and Windows named streams, is not copied by the replacement policy. The Markdown
file picker shows all files and rejects unsupported extensions. See the
[storage contract](docs/document-storage.md) for conflict checks and their limits.

Each notebook keeps settings in `.mithril/settings.json`, including appearance,
autosave, view mode, sidebar options, the last selected file and expanded folders.
File/folder paths there are notebook-relative; the internal directory is hidden
from Explorer. The user configuration stores registered notebook paths and the
active notebook so they can be restored at startup:

| Platform | User configuration |
| --- | --- |
| Windows | `%APPDATA%\mithril\notebooks.json` |
| macOS | `~/Library/Application Support/mithril/notebooks.json` |
| Linux | `$XDG_CONFIG_HOME/mithril/notebooks.json`, falling back to `~/.config/mithril/notebooks.json` if XDG_CONFIG_HOME is unset or not absolute |

If you used a build from before the Mithril rename, close the app and rename
the previous notebook metadata folder to `.mithril` in each notebook. Rename
the previous app folder in your platform's user configuration directory to
`mithril` as well. Keep the JSON files unchanged; existing settings are not
automatically migrated. If a destination already exists, back up both folders
before reconciling them.

Settings use atomic file replacements. There is no Save As, file watcher,
multiple-document tab interface or database.

## Validation and contributor information

For current ownership, extension rules and headless verification commands, start
with [ARCHITECTURE.md](ARCHITECTURE.md) and [AGENTS.md](AGENTS.md). The
[implementation progress](docs/architecture-progress.md) records completed work
and current verification; the [dated audit](docs/architecture-audit.md) records
the original findings and plan. The component is patched in
`vendor/gpui-component`; its [patch notes](vendor/gpui-component/EDITOR-PATCH.md)
record the upstream source and local changes.

Builds and validation run locally; there are no GitHub Actions workflows.
Use the headless verification commands in [ARCHITECTURE.md](ARCHITECTURE.md)
with the repository's pinned Rust toolchain on each target platform.

### User-run native diagnostics

The in-app diagnostic harnesses create a real application window and are for a
user to run manually. Agents must use headless tests and must not launch these
harnesses, including hidden launches. The notebook harness uses disposable
fixtures and an isolated notebook index:

```powershell
cargo build --locked --features editor-diagnostics
$env:MITHRIL_NOTEBOOK_CHECK = Join-Path (Get-Location) 'target/notebook-smoke.txt'
& ./target/debug/mithril.exe
Remove-Item Env:MITHRIL_NOTEBOOK_CHECK
Get-Content target/notebook-smoke.txt
```

It exits automatically and writes a result, progress file or panic report.
`MITHRIL_AUTOSAVE_CHECK` selects a separate disposable-fixture diagnostic for
timers, save races, failure recovery, task autosave and settings validation.
[Performance validation](benchmarks/README.md) documents the user-run release
benchmark, fixtures, hardware, historical measurements, limitations and commands.
Those recorded timings are not current measurements on every machine.

For a manual UI check, use disposable files to exercise all three views,
Save/Discard/Cancel, notebook switching, menus, native pickers, clipboard,
selection, scrolling and physical IME input. Reopen saved files and check invalid
UTF-8 and locked/read-only files. These native interactions are distinct from
the headless regression suite.

Use Ctrl+Up / Ctrl+Down (Command on macOS) in Source or Live preview to move the current line or selected lines.


Right-click a sidebar file or folder (or press Shift+F10 on its selected row) for
creation, rename, duplicate, relative-path copy, system reveal and Trash actions.
Folder and background menus offer New note and New folder; Duplicate applies to
files and copies the active note's unsaved text. Trash uses the system recycle
bin and resolves unsaved changes before deleting the active note or its parent.

In Live preview, drag a bullet or checkbox to move its item and nested content
before or after another unordered/task item, including in another list. The
item matches the destination indentation and bullet style, preserving checkbox
state. Escape cancels; Undo restores the entire move. Numbered lists and moves
between different blockquotes are excluded.
