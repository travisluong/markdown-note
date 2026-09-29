# Markdown Note user guide

Markdown Note keeps your notes as ordinary UTF-8 files. One window contains
document tabs, a notebook sidebar, and an optional terminal. Markdown files have
Source, Live preview, and Reading views.

## Install and start

Packaged downloads have not been published for Markdown Note yet. Windows is the
primary platform. Follow the [build instructions](../README.md#build-from-source-windows)
to build from source. Linux and macOS do not currently have packaged downloads.

Building and editing notes require no website checkout, database, account, or
hosted service. The optional feedback form is described in [Feedback](feedback.md).

## Get started

1. Open **Notebooks → Manage notebooks**. Create a notebook, or choose **Open folder as notebook** for an existing folder.
2. Choose **File → New note** or press **Ctrl+N** to create `Untitled.md` in the notebook root. Existing names are preserved with numbered filenames. Rename it from its sidebar context menu.
3. Write in Live preview, or select **View → Source mode** or **Reading view**.
4. Press **Ctrl+O** to find a Markdown note or folder in the active notebook. Up/Down selects a result, Enter opens it, and Escape returns focus. Without a notebook, Ctrl+O opens the file picker.
5. Use **Ctrl+S** to save immediately. Notebook autosave is enabled by default.

**File → Open…** always opens the file picker and works without a notebook. It
accepts Markdown and other UTF-8 text files, as well as supported images.

Use **Ctrl+P** for the command palette. On macOS, use **Command** instead of
**Ctrl** for these shortcuts. **Help → Keyboard shortcuts** lists the bindings
for your platform, including editor and menu shortcuts.

## Notebooks and tabs

A notebook is a folder you choose. Creating one makes a new folder inside the
selected parent; opening one uses an existing folder without moving its files.
**Manage notebooks → Remove** forgets a notebook without deleting its folder or
notes. Missing folders and damaged settings are reported rather than replaced.

**Ctrl+T** opens an empty tab without creating a file. Open a file in that tab,
or use **Ctrl+N** to create a note in the active notebook. **Ctrl+W** closes the
current tab. Opening a file from Explorer, a link, Search, or the file picker
uses the current tab, so create a new tab first to keep the previous file open.

Switching tabs retains edits, selection, view mode, and navigation history.
Opening the same file in multiple tabs shares its text and undo history. Closing
its last view prompts to Save, Discard, or Cancel when needed.

Each notebook keeps its own group of tabs during the app session. Switching
notebooks restores that group's tabs without discarding drafts in another group.
Files opened without a notebook have a separate group. After an app restart,
the active notebook restores its last selected note or empty tab, not its entire
previous tab set.

### Explorer

Click a folder to expand it, a Markdown file to open it, or a supported image to
view it. Other file types are displayed but cannot be opened from Explorer;
use **File → Open…** for other UTF-8 text files. Filesystem links are not opened
or traversed. Empty folders remain visible.

Up/Down moves between rows, Left/Right collapses or expands folders, and Enter
activates a row. Right-click a row, or press **Shift+F10**, for creation, rename,
duplicate, relative-path copy, system reveal, and Trash actions. Folder and
background menus offer New note and New folder. Duplicate copies a file,
including retained unsaved text when applicable. Trash uses the system recycle
bin and resolves affected unsaved changes first.

Drag files or folders onto another folder to move them, or onto the sidebar
toolbar to move them to the notebook root. Name collisions and moves inside a
folder's own descendants are rejected. Open documents keep their edits and
follow successfully moved paths.

**Sort files** offers filename, modified-date, and created-date ordering.
Folders stay first in alphabetical order. **Refresh** picks up external changes;
there is no automatic file watcher. The sidebar controls switch between Explorer
and Search, and its edge can be dragged to resize it.

### File extension labels

**Settings → General → Sidebar** controls whether extensions appear. **Show file
extensions** is on by default, while **Always hide these extensions** starts with
`md`: `notes.md` appears as `notes`, but `photo.png` keeps its extension.

Enter comma-separated extensions such as `md, txt`. Matching ignores case and
optional leading dots. These settings change labels only; filenames and search
results remain unchanged. Hover over a file to see its full filename.

## Editing and view modes

**Source** edits literal Markdown. **Live preview** edits the same text while
rendering formatting around the active line or block. Both use the same text and
undo history. **Reading** renders the current text, including unsaved edits.
Source and Live preview copy the source selection; Reading copies selected
rendered text. Saving always writes the underlying source.

Use the View menu or **Ctrl+Shift+M** to cycle through the three views. Tabs keep
their own view modes during the session; notebooks remember the selected
Markdown mode for later use. The default is Live preview.

Other UTF-8 files, such as `.txt`, `.json`, and source code, open in Source mode.
Known languages receive syntax highlighting; unknown extensions use plain text.
Find, replace, undo, and saving remain available. Markdown formatting controls,
Live preview, and Reading are unavailable for those files. Binary data and
invalid UTF-8 are rejected.

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

In Live preview, drag a bullet or checkbox to move its item and nested content
before or after another unordered/task item. Escape cancels; Undo restores the
move. Use **Ctrl+Up / Ctrl+Down** in the editor to move lines or selected lines.

Mermaid code blocks render offline in Live preview and Reading. See
[Mermaid diagrams](mermaid.md) for syntax and limits. The optional
[terminal panel](terminal.md) can keep shells beside your notes.

## Find and notebook Search

**Ctrl+F** finds literal text in the current document, including unsaved changes.
In Source and Live preview, the current selection seeds the query. Matches are
highlighted and show a counter; use the Find controls to move between them.
**Match case** is optional. Case-insensitive matching folds ASCII letters;
non-ASCII characters match exactly. Escape closes Find.

Reading Find scrolls to the rendered match while keeping Reading mode. A match
in hidden Markdown syntax reveals its containing block with an explanation.
Source and Live preview also support literal **Replace one** and **Replace all**
as undoable edits; Reading does not offer replacement.

**Ctrl+Shift+F** searches Markdown files across the active notebook, including
unsaved text in retained open documents. Results group highlighted snippets by
file. Up/Down selects a result; Enter or a click opens it in the current tab with
the usual unsaved-change protection. The query and result list remain available
when you return to Search.

Search refreshes after relevant edits or query changes. To rescan external file
changes, change the query or toggle Match case. Scans skip filesystem links,
internal metadata, unreadable/non-UTF-8 files, and files over 8 MiB. Results are
capped at 2,000 matches, with skipped or limited results indicated. **Ctrl+O**
and notebook Search are Markdown-specific; use File → Open for other text files.

## Customize the workspace

**File → Settings** contains General, Terminal, Toolbar, Templates, Font, and
Theme sections. Most preferences belong to the active notebook. Without one,
appearance and similar preferences last for the current app session.

- **General:** autosave, external-file autosave, sidebar labels/exclusions, centered editor width, and footer counts. The footer can count source text or readable Markdown.
- **Terminal:** shell, cursor, and independent terminal font settings. New shell choices apply to new terminal tabs.
- **Toolbar:** visibility of buttons and regions, plus icon size. Hiding a panel button does not close the panel's documents or shells.
- **Font:** interface, text, and monospace fonts and sizes. Source uses the monospace font; Live preview and Reading use the text font.
- **Theme:** independent light/dark palettes for background, text, primary, border, icon, and secondary colors. Reset restores the palettes.
- **Templates:** the format used by **Templates: Insert current date**, also available with **Ctrl+Shift+D**.

The default date format is `YYYY-MM-DD`. Supported tokens include years, months,
days, weekdays, hours, minutes, seconds, and time zones:

```text
YY YYYY M MM MMM MMMM D DD Do d dd ddd dddd H HH h hh m mm s ss SSS A a Z ZZ
```

Use `[literal text]` to include text without interpreting it as a token. Invalid
formats show an error and retain the last valid setting. Dates use local time
and English names.

**View → Centered editor** toggles the centered text column. **View → Zoom**
changes notebook zoom from 50% to 200%; Reset returns to 100%. Sidebar and
terminal widths are independently resizable.

## Saving and conflicts

An asterisk in the window title means unsaved changes. Notebook autosave starts
enabled with a 1,000 ms delay after edits. Settings offers an enable switch and
a delay from 100 to 60,000 ms. **Ctrl+S** saves immediately.

Files outside the active notebook show an **External file** badge. Their autosave
has a separate **Autosave external files** switch, off by default.
Without a notebook, that separate preference lasts for the session. Save these
files explicitly unless you choose to enable their autosave.

If a file changes, is replaced, or disappears on disk, saving reports a conflict
and pauses autosave for that document. Your text and undo history stay available.
Copy any changes you need before reopening the file, then reconcile them with the
disk version. Save retries the version check; it does not force an overwrite.

Opening a replacement file in the current tab, closing the last view of a dirty
file, or closing the app prompts to Save, Discard, or Cancel when needed. Failed
opens and saves preserve your work. Writes use a temporary file beside the
original before replacing it, retaining UTF-8 BOM and newline conventions.
See the [storage contract](document-storage.md) for detailed guarantees and limits.

## Files and preferences

Notes stay in the folders you choose. Each notebook stores preferences in
`.markdown-note/settings.json`; this internal folder is hidden from Explorer.
Registered notebook locations and window placement are stored per user:

| Platform | Configuration directory |
| --- | --- |
| Windows | `%APPDATA%\markdown-note` |
| macOS | `~/Library/Application Support/markdown-note` |
| Linux | `$XDG_CONFIG_HOME/markdown-note`, or `~/.config/markdown-note` when XDG_CONFIG_HOME is unset or not absolute |

Markdown Note starts with its own preferences. To use an existing notes folder,
open it through Manage notebooks; your note files do not need conversion.
Settings from the previous application are not imported. Its `.mithril` metadata
is kept hidden and protected without being read or changed.

There is no built-in cloud sync, automatic file watcher, or Save As command.
You can manage and back up the ordinary note files using your own tools.

For code changes and verification, see [Contributing](../CONTRIBUTING.md).
