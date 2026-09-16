# Command surface inventory and port priorities

Original audit date: 2026-09-15. Baseline: `57704a2`, plus the working-tree changes present during that source audit. The original audit was documentation-only. Implementation status is recorded separately below.

## Current implementation after ports 1-10 (2026-09-16)

All ten ranked ports below are implemented. This section supersedes the historical
coverage matrices and proposals that follow it. Counts checked against the source:
**44 palette commands, 17 configurable toolbar tools, 35 application bindings,
7 File menu actions, and 28 static top-level menu actions** plus registered
notebook entries. File, notebook and six-choice sort picker results are not
additional commands. Inherited editor/component bindings and Settings Escape
remain outside the application-binding count.

| Completed rank | Current access |
| --- | --- |
| 1 | All 16 previously missing formatting operations are in the palette: Bold, Italic, Strikethrough, Inline code, Code block, Blockquote, Ordered list, Unordered list, Task list, Heading 1-6 and Paragraph. Bold/Italic bind Mod+B/I in DocumentEditor. Link keeps its existing command and Mod+K. |
| 2 | File has New note, New tab and Close tab. New/Close tab are also palette commands. Mod+N/T/W invoke their existing lifecycle owners. |
| 3 | Palette has Source mode, Live preview and Reading view. Mod+Shift+M cycles Source → Live preview → Reading. |
| 4 | Settings is in the palette and binds Mod+comma. Replace binds Ctrl+H on Windows/Linux and Cmd+Option+F on macOS. |
| 5 | Back/Forward are in palette and View, with Alt+Left/Right on Windows/Linux and Cmd+Option+Left/Right on macOS. |
| 6 | Toggle sidebar and Show Explorer are in the palette, with Mod+Shift+B/E. View has Show Explorer alongside Collapse sidebar. |
| 7 | File has Go to a note; View has Command palette. Palette adds Open file, Save, Manage notebooks and Keyboard shortcuts. Open file always uses the system picker; Go to retains notebook discovery and its standalone picker fallback. |
| 8 | Palette has Text: Toggle selected tasks (distinct from Format: Task list); Mod+Shift+D inserts the configured current date. |
| 9 | F2 in NotebookTree opens Rename. Palette has Explorer: Copy selected relative path and Explorer: Reveal selected entry, showing the captured Explorer target. No active-document fallback. |
| 10 | Explorer: Sort files opens a searchable six-choice palette picker with the current order marked and initially selected. Escape returns to commands. View has Toggle light/dark mode, sharing the existing palette theme adapter. |

Current top-level menu order (separators omitted):

| Menu | Current actions |
| --- | --- |
| File (7) | Open; Save; New note; New tab; Close tab; Go to a note; Settings |
| View (9) | Source mode; Live preview; Reading view; Back; Forward; Collapse sidebar; Show Explorer; Command palette; Toggle light/dark mode |
| Search (3) | Search in file; Search in all files; Search and replace in file |
| Sort (6) | File name A to Z / Z to A; Modified time new to old / old to new; Created time new to old / old to new |
| Notebooks (1 static) | Manage notebooks; then one entry per registered notebook |
| Help (2) | Keyboard shortcuts; About |

Current application shortcuts (all 35 registrations): `Mod` is Ctrl on
Windows/Linux and Cmd on macOS. Scope is the focused key context, with fresh
capability and modal checks applied by the owning adapter.

| Action | Windows/Linux | macOS | Scope |
| --- | --- | --- | --- |
| Rename selected item | F2 | F2 | NotebookTree |
| Selected item context menu | Shift+F10 | Shift+F10 | NotebookTree |
| Move lines up / down (2) | Ctrl+Up / Down | Cmd+Up / Down | DocumentEditor; original macOS collision remains below |
| Bold / Italic (2) | Ctrl+B / I | Cmd+B / I | DocumentEditor |
| Go to a note | Ctrl+O | Cmd+O | Notes |
| Command palette | Ctrl+P | Cmd+P | Notes |
| Save | Ctrl+S | Cmd+S | Notes |
| Back / Forward (2) | Alt+Left / Right | Cmd+Option+Left / Right | Notes |
| Settings | Ctrl+comma | Cmd+comma | Notes |
| Toggle sidebar | Ctrl+Shift+B | Cmd+Shift+B | Notes |
| Show Explorer | Ctrl+Shift+E | Cmd+Shift+E | Notes |
| Cycle view mode | Ctrl+Shift+M | Cmd+Shift+M | Notes |
| New note | Ctrl+N | Cmd+N | Notes |
| New tab | Ctrl+T | Cmd+T | Notes |
| Close tab | Ctrl+W | Cmd+W | Notes |
| File / View / Search / Notebooks / Help menus (5) | Alt+F / V / S / N / H | Ctrl+Option+F / V / S / N / H | Notes |
| Create or toggle selected tasks | Ctrl+L | Cmd+L | DocumentEditor |
| Insert current date | Ctrl+Shift+D | Cmd+Shift+D | DocumentEditor |
| Insert or edit link | Ctrl+K | Cmd+K | DocumentEditor |
| Find in current file | Ctrl+F | Cmd+F | Notes |
| Search notebook | Ctrl+Shift+F | Cmd+Shift+F | Notes |
| Find and replace | Ctrl+H | Cmd+Option+F | Notes |
| Close Reading find | Escape | Escape | ReadingContent |
| Previous / next / collapse / expand / open tree item (5) | Up / Down / Left / Right / Enter | Same | NotebookTree |

Toolbar composition and its preferences are unchanged. All 17 tools now have
palette access (Heading through seven explicit commands and Sort through its
picker); Sort retains its top-level and toolbar menus. This does not add a
shortcut for every formatting operation or a new toolbar icon for every command.

Verification uses Windows TestPlatform, pure unit tests and static checks; no
native app was launched or desktop input injected. Native Windows/macOS/Linux
presentation and physical keyboard behavior remain manual validation. Detailed
results are recorded in [architecture progress](architecture-progress.md).
The historical macOS line-move collision and Reading-find Help mismatch were
not part of the ten ranked ports and remain follow-ups. Search/Explorer toolbar
shortcut labels now use platform-aware modifiers, and F2 is implemented.

## Historical audit baseline (2026-09-15)

**Everything below describes the original baseline and original recommendations,
not current coverage.** Old counts, missing-surface claims and proposed keys are
retained to explain the audit and implementation choices; use the current
implementation section above for present behavior.

## Scope and counting

The inventory covers the command palette, the full-width window toolbar, application and editor keyboard bindings, and all six top-level menus. File/Explorer context menus and nearby controls are included separately because they contain valuable port candidates. Platform-owned window menus, native file-picker bindings, ordinary text entry, and every individual Settings form control are outside the command inventory.

| Surface | Historical baseline inventory |
| --- | --- |
| Command palette | **10 commands**; file and notebook picker results are separate palette modes, not additional commands |
| Window toolbar | **17 configurable tools**, all enabled by default; Heading expands to seven choices, Sort to six |
| Application shortcuts | **21 bindings** in `app_shortcuts`, plus Settings Escape and the inherited input/component bindings below |
| Literal File menu | **3 actions**: Open, Save, Settings |
| All top-level menus | **19 static leaf actions**, plus one dynamic entry per registered notebook; six menus: File, View, Search, Sort, Notebooks, Help |

“Missing” means no direct entry/binding on that surface. Opening a menu or palette and navigating with arrows is indirect keyboard access, not a dedicated shortcut. Toolbar means the configurable window toolbar; tab-strip and find-panel buttons are recorded separately. Disabled commands still count as present.

The highest-return direction is **existing actions into the palette and selected shortcuts**. Toolbar expansion has a higher cost: it changes persisted item ordering, visibility defaults, Settings controls, icons and crowded-window layout.

## 1. Every palette command and its coverage

Sources: [catalogue](../src/commands/catalogue.rs), [dispatch and picker modes](../src/goto.rs), [capabilities](../src/capabilities.rs), [shortcut registry](../src/help_ui.rs).

`Mod` means Ctrl on Windows/Linux and Cmd on macOS. Menu paths below refer to the top menu bar, not context menus.

| Exact palette label | Toolbar | Direct shortcut | Top menu | Availability / semantics |
| --- | --- | --- | --- | --- |
| Templates: Insert current date | — | — | — | Editable document; replaces selection using configured date format |
| Text: Move line up | — | Mod+Up | — | Editable document; current/selected logical lines; macOS binding collision noted below |
| Text: Move line down | — | Mod+Down | — | Same, downward |
| Text: Insert or edit link | Insert / Edit Link | Mod+K | — | Editable document; guarded link dialog |
| Appearance: Toggle light/dark mode | — | — | — | Toggles configured appearance; Settings also offers theme choices |
| Notebook: Change notebook | — | — | Notebooks > registered notebook | Palette opens notebook picker; menu selects a notebook directly |
| Notebook: Create new note | — | — | — | Active notebook required; creates Untitled note in notebook root |
| Search: Search current file | — | Mod+F | Search > Search in file | Open document; supports Reading as well as Source/Live |
| Search: Search in all files | Search | Mod+Shift+F | Search > Search in all files | Active notebook required |
| Search: Search and replace in current file | — | — | Search > Search and replace in file | Editable document; also accessible through current-file find controls |

All catalogue commands reject busy capability state. Palette opening additionally guards modal/dialog state; execution recomputes capabilities. Editing requires a document in Source or Live preview, including standalone files; Reading, image and empty views are excluded.

## 2. Every window toolbar tool and its coverage

Sources: [rendering and dispatch](../src/toolbar_ui.rs), [toolbar preferences](../src/notebook/settings.rs), [format transformations](../src/markdown_edit.rs), [semantic Editor adapter](../src/editor.rs).

Listed in default toolbar order. Saved custom order and visibility can differ.

| Tool | Palette | Direct shortcut | Top menu | Behavior / choices |
| --- | --- | --- | --- | --- |
| Back | — | — | — | Previous history visit in active tab; requires available history, not an active notebook |
| Forward | — | — | — | Next history visit in active tab |
| Toggle sidebar | — | — | View > Collapse sidebar | Show/collapse active notebook sidebar |
| Explorer | — | — | — | Show/focus Explorer, including switching away from Search |
| Search | Search in all files | Mod+Shift+F | Search > Search in all files | Show/focus notebook search |
| Sort files | — | — | Sort > same six choices | Shared sort menu, listed below |
| Bold | — | — | — | Toggle bold formatting |
| Italic | — | — | — | Toggle italic formatting |
| Strikethrough | — | — | — | Toggle strikethrough formatting |
| Heading | — | — | — | Heading 1, Heading 2, Heading 3, Heading 4, Heading 5, Heading 6, Paragraph |
| Inline code | — | — | — | Toggle inline code |
| Code block | — | — | — | Toggle fenced code block |
| Insert / Edit Link | Insert or edit link | Mod+K | — | Reuses guarded link dialog |
| Blockquote | — | — | — | Toggle quote prefix on selected lines |
| Ordered list | — | — | — | Apply/remove ordered-list formatting |
| Unordered list | — | — | — | Apply/remove unordered-list formatting |
| Task list | — | — | — | Apply/remove task-list formatting; **not equivalent to Mod+L** |

There are **10 formatting tools missing from both the palette and dedicated shortcuts**. Expanding Heading into its seven choices gives **16 missing formatting operations**. Link is the eleventh formatting tool and already has both surfaces.

Formatting uses `Editor::format_markdown` through `apply_toolbar_format`; it performs one isolated source edit and preserves normal history/change behavior. Heading additionally captures editor/document identity, source and selection while its menu is open. Ports must retain equivalent target validation.

Sidebar, Explorer, Search and Sort require an active notebook. Formatting requires an editable document. Back/Forward use per-tab history. Availability is state-dependent; a visible button is not necessarily enabled.

## 3. Every top-level menu action

Sources: [menu construction](../src/title_bar_ui.rs), [notebook menu](../src/notebook_ui.rs), [shared Sort menu](../src/sidebar_ui.rs), [sort labels](../src/notebook/tree.rs).

| Menu | Exact item | Palette | Toolbar | Direct shortcut |
| --- | --- | --- | --- | --- |
| File | Open… | — | — | —; Mod+O is Go to a note, with an Open fallback only without a notebook |
| File | Save | — | — | Mod+S |
| File | Settings | — | — | — |
| View | Source mode | — | — | — |
| View | Live preview | — | — | — |
| View | Reading view | — | — | — |
| View | Collapse sidebar | — | Toggle sidebar | — |
| Search | Search in file | Yes | — | Mod+F |
| Search | Search in all files | Yes | Search | Mod+Shift+F |
| Search | Search and replace in file | Yes | — | — |
| Sort | File name (A to Z) | — | Sort files submenu | — |
| Sort | File name (Z to A) | — | Sort files submenu | — |
| Sort | Modified time (new to old) | — | Sort files submenu | — |
| Sort | Modified time (old to new) | — | Sort files submenu | — |
| Sort | Created time (new to old) | — | Sort files submenu | — |
| Sort | Created time (old to new) | — | Sort files submenu | — |
| Notebooks | Manage notebooks | — | — | — |
| Notebooks | Each registered notebook, named dynamically | Change notebook picker | — | — |
| Help | Keyboard shortcuts | — | — | — |
| Help | About | — | — | — |

Notebook names include parent paths when necessary to disambiguate duplicates. Active notebook, view mode and sort choice show selection state. Sort keeps folders alphabetical and ahead of files. Menu overflow under More reuses these same actions and does not add commands.

There is no Edit or Format menu. The File menu currently has no New Note, New Tab, Close Tab, Save As or Quit item. Save As is not an existing action to port; adding it would require new behavior.

## 4. Every application shortcut

Source: [binding registry and Help labels](../src/help_ui.rs), registered by [main](../src/main.rs). These are all 21 `app_shortcuts` entries.

| Windows / Linux | macOS | Action | Context |
| --- | --- | --- | --- |
| Ctrl+O | Cmd+O | Go to a note; file/folder picker within active notebook, OS Open fallback without notebook | Notes |
| Ctrl+P | Cmd+P | Command palette | Notes |
| Ctrl+S | Cmd+S | Save | Notes |
| Alt+F | Ctrl+Option+F | Open File menu | Notes |
| Alt+V | Ctrl+Option+V | Open View menu | Notes |
| Alt+S | Ctrl+Option+S | Open Search menu | Notes |
| Alt+N | Ctrl+Option+N | Open Notebooks menu | Notes |
| Alt+H | Ctrl+Option+H | Open Help menu | Notes |
| Ctrl+Up | Cmd+Up | Move line/selected lines up | DocumentEditor |
| Ctrl+Down | Cmd+Down | Move line/selected lines down | DocumentEditor |
| Ctrl+L | Cmd+L | Create or toggle selected tasks | DocumentEditor |
| Ctrl+K | Cmd+K | Insert or edit link | DocumentEditor |
| Ctrl+F | Cmd+F | Find in current file | Notes |
| Ctrl+Shift+F | Cmd+Shift+F | Search all notebook files | Notes |
| Escape | Escape | Close Reading find | ReadingContent |
| Shift+F10 | Shift+F10 | Open selected Explorer item's context menu | NotebookTree |
| Up | Up | Previous tree item | NotebookTree |
| Down | Down | Next tree item | NotebookTree |
| Left | Left | Collapse folder / move to parent | NotebookTree |
| Right | Right | Expand folder / move to child | NotebookTree |
| Enter | Enter | Open selected tree item | NotebookTree |

One additional application binding is registered in [SettingsPanel](../src/settings_ui/panel.rs): **Escape closes Settings**, in `SettingsPanel` context. Sort has no matching Alt menu-opening binding.

Mod+O and Mod+P are shortcut-only entry points to their respective pickers; neither is listed as a command inside the command palette or exposed on the toolbar/top menus.

### Inherited input/editor shortcuts

Source: [InputState binding initialization](../vendor/gpui-component/src/input/state.rs). These also serve text fields where applicable; Markdown-specific behavior depends on the document editor and mode. Each combined row enumerates both paired bindings.

| Windows / Linux | macOS | Action |
| --- | --- | --- |
| Backspace / Delete | Same | Delete backward / forward |
| Ctrl+Backspace / Ctrl+Delete | Option+Backspace / Option+Delete | Delete to previous word start / next word end |
| — | Cmd+Backspace / Cmd+Delete | Delete to beginning / end of line |
| Enter | Enter | Newline, continue list, exit empty list item; confirm/submit in appropriate single-line contexts |
| Ctrl+Enter | Cmd+Enter | `Enter { secondary: true }`; document editor still uses its Enter path; context-specific meaning below |
| Escape | Escape | Dismiss editor context menu/find/table navigation/drag/completion as applicable; otherwise propagate |
| Up / Down / Left / Right | Same | Move caret |
| Page Up / Page Down | Same | Move caret by page |
| Home / End | Same | Move to line start / end |
| — | Ctrl+A / Ctrl+E | Additional line-start / line-end bindings |
| — | Cmd+Left / Cmd+Right | Additional line-start / line-end bindings |
| Ctrl+Left / Ctrl+Right | Option+Left / Option+Right | Move by word |
| — | Cmd+Up / Cmd+Down | Move to document start / end; overlaps application line-move bindings |
| Shift+Left / Shift+Right / Shift+Up / Shift+Down | Same | Extend selection |
| Shift+Home / Shift+End | Same | Select to line start / end |
| — | Ctrl+Shift+A / Ctrl+Shift+E | Additional select-to-line-start / end bindings |
| — | Cmd+Shift+Left / Cmd+Shift+Right | Additional select-to-line-start / end bindings |
| Ctrl+Shift+Left / Ctrl+Shift+Right | Option+Shift+Left / Option+Shift+Right | Extend selection by word |
| — | Cmd+Shift+Up / Cmd+Shift+Down | Select to document start / end |
| Tab / Shift+Tab | Same | Inline indent/outdent; list nest/outdent; table navigation takes precedence when active |
| Ctrl+] / Ctrl+[ | Cmd+] / Cmd+[ | Indent / outdent lines |
| Ctrl+A | Cmd+A | Select all |
| Ctrl+C / Ctrl+X / Ctrl+V | Cmd+C / Cmd+X / Cmd+V | Copy / cut / paste |
| Ctrl+Z / Ctrl+Y | Cmd+Z / Cmd+Shift+Z | Undo / redo |
| Ctrl+F | Cmd+F | Find in searchable input; overlaps app's document-find route intentionally |
| Ctrl+. | Cmd+. | Component code-actions binding; no application document code-action provider configured |
| — | Ctrl+Cmd+Space | Show system character palette |

There are no Windows/Linux Ctrl+Home, Ctrl+End, Ctrl+Shift+Home, Ctrl+Shift+End or Ctrl+Shift+Z registrations in this input binding list. Those are possible shortcut additions to existing movement/history operations, not claims about undocumented OS behavior.

### Contextual shortcuts and control navigation

Sources: [palette](../src/goto.rs), [notebook search](../src/search_ui/panel.rs), [Reading find](../src/reading_search.rs), [editor find](../vendor/gpui-component/src/input/search.rs), [table navigation](../vendor/gpui-component/src/input/table_edit.rs), [Reading text](../vendor/gpui-component/src/text/text_view.rs).

| Context | Keys | Effect |
| --- | --- | --- |
| Command/file/notebook picker query | Up / Down | Move selected result |
| Command/file/notebook picker query | Enter or Mod+Enter | Execute/open selected result |
| Command/file picker | Escape | Dismiss and restore prior focus |
| Notebook picker | Escape | Return to command palette |
| All-files search query | Up / Down | Select and immediately reveal previous/next result |
| All-files search query | Enter or Mod+Enter | Open selected result |
| All-files search query | Escape | Return focus to editor; retain search panel/results |
| Source/Live file-find panel | Enter or Mod+Enter | Next match |
| Source/Live file-find panel | Shift+Enter | Previous match |
| Source/Live file-find panel | Escape | Close find and focus editor |
| Source/Live file-find panel | Tab | Return focus to editor |
| Source/Live file-find panel | Mod+F | Focus/select search query |
| Reading find query | Enter | Next match |
| Reading find query | Mod+Enter | Previous match (`secondary: true`); **not Shift+Enter in current binding code** |
| Reading find query/content | Escape | Close Reading find |
| Reading text selection | Mod+C | Copy rendered selected text |
| Active Live table | Tab / Shift+Tab | Next / previous cell; advancing at end adds a row |
| Active Live table | Enter (also secondary Enter) | Next row; add row at end |
| Active Live table | Arrows | Move within/cross cells at their boundaries |
| Active Live table | Home / End | Cell start / end |
| Active Live table | Escape | Leave table navigation |

Inherited focus/modal controls, not document commands:

| Component | Keys | Effect / source |
| --- | --- | --- |
| Root focus | Tab / Shift+Tab | Next/previous focus target when not consumed by input; [root](../vendor/gpui-component/src/root.rs) |
| Popup menus, including toolbar dropdowns and context menus | Up / Down, Right / Left, Enter, Escape | Select item, enter/leave submenu, activate, dismiss; [popup menu](../vendor/gpui-component/src/menu/popup_menu.rs) |
| Dialog | Enter / Escape | Confirm / cancel where dialog handlers allow; child handlers can consume keys; [dialog](../vendor/gpui-component/src/dialog.rs) |
| Settings font select | Up / Down, Enter / Mod+Enter, Escape | Navigate, confirm, cancel; [select](../vendor/gpui-component/src/select.rs) |
| List inside component dropdown | Up / Down, Enter / Mod+Enter, Escape | Navigate, confirm, cancel; [list](../vendor/gpui-component/src/list/list.rs) |
| Color picker / popover | Escape | Dismiss; [color picker](../vendor/gpui-component/src/color_picker.rs), [popover](../vendor/gpui-component/src/popover.rs) |
| Debug/inspector-enabled builds only | Ctrl+Shift+I; Cmd+Option+I on macOS | Toggle GPUI inspector; [inspector](../vendor/gpui-component/src/inspector.rs) |

For completeness, component initialization also registers keys for widgets not used as Mithril command surfaces: NumberInput Up/Down (increment/decrement); Sheet Escape; DatePicker Enter/Escape/Delete/Backspace; generic Table Escape/Up/Down/Left/Right; generic Tree arrows; generic AppMenuBar Escape/Left/Right. Mithril's Explorer, Markdown tables and title menu use the dedicated routes above. These inherited registrations are not evidence of additional product features. See [component initialization](../vendor/gpui-component/src/lib.rs).

## 5. File context menus and nearby controls

Source: [Explorer entry menus](../src/notebook_ui/entry_menu.rs). Included to disambiguate “file menu” and identify existing operations suitable for ports.

| Target | Exact menu actions, in order |
| --- | --- |
| File | Open; Rename; Duplicate; Copy relative path; Reveal in Explorer/Finder/file manager; Move to Trash |
| Folder | New note; New folder; Rename; Copy relative path; Reveal in Explorer/Finder/file manager; Move to Trash |
| Explorer background | New note; New folder; Refresh |

Shift+F10 opens the selected item's menu. There are no dedicated Rename, Duplicate, Trash, New Folder, Copy Path, Reveal or Refresh app bindings or palette entries. Tree Enter provides Open. Palette New Note uses the notebook root; folder-context New Note uses its captured folder, so they are related but differently targeted operations. Every context target captures notebook root, Explorer generation and relative path; preserve these guards when porting.

Other existing controls worth keeping in the inventory:

| Surface | Actions | Other access |
| --- | --- | --- |
| [Tab strip](../src/tabs.rs) | New tab; select any retained tab; close a tab | No palette, window-toolbar, top-menu or dedicated shortcut entries |
| [Manage notebooks dialog](../src/notebook_ui.rs) | Create new notebook; open folder as notebook; activate registered notebook; remove notebook registration; creation Browse/Back/Create | Dialog reached through Notebooks menu; activation also in notebook picker/menu |
| [Source/Live find strip](../vendor/gpui-component/src/input/search.rs) | Match case; toggle replace mode; previous match; next match; close; Replace; Replace All | Navigation shortcuts above; opening replace mode also palette/Search menu |
| [Reading find strip](../src/reading_search.rs) | Match case; Previous; Next; Close | Navigation shortcuts above |
| [All-files search panel](../src/search_ui/panel.rs) | Match case; open result | Result navigation shortcuts above |
| [Editor context menu](../vendor/gpui-component/src/input/popovers/context_menu.rs) | Cut; Copy; Paste; Select All | Standard input shortcuts; absent from top menus/palette/window toolbar |
| Live link context menu; Reading link context menu | Copy link destination | Context-only; working-tree implementation included in this audit |
| Live table context menu | Insert row above; Insert row below; Delete row; Insert column left; Insert column right; Delete column; Align left; Align center; Align right; Delete table | Context-only structural commands; Tab/Enter can append rows |
| Live/Reading code blocks | Copy code | Inline icon; no palette/menu/shortcut equivalent |
| Live/Reading task checkboxes | Toggle one task | Mod+L is the related selection-based editable-document operation, with different targeting |
| Window caption | Minimize; maximize/restore; close | Platform controls/lifecycle; not in the app command catalogue |

## 6. Highest-ROI ports

Ranking is a product/engineering judgment based on expected frequency, discoverability, existing handler reuse and implementation risk; there is no usage telemetry behind it. “Small” means mostly registrations and guarded dispatch; “Medium” includes new target selection, preference/schema work or more lifecycle testing. Proposed keys below are **not current shortcuts**.

| Rank | Proposed port | Value | Relative effort and reuse |
| --- | --- | --- | --- |
| **1** | **Toolbar formatting → palette; Bold/Italic → Mod+B / Mod+I** | Removes the largest coverage hole; enables common editing without leaving the keyboard and makes all existing formatting searchable | Small–medium as one family. Reuse `Editor::format_markdown`; add the 16 missing operations, including H1–H6 and Paragraph. Keep keys in `DocumentEditor`, not generic Input. More formatting keys can follow after collision review |
| **2** | **New Note → File menu + Mod+N; New/Close Tab → File menu + palette + Mod+T / Mod+W** | Makes core creation and tab lifecycle discoverable and efficient | Small–medium. Existing `Next::NewNote`, `new_tab`, `close_tab` owners. Distinguish disk note creation from empty tab creation; closing must retain save/cancel behavior |
| **3** | **Source / Live / Reading → palette + one mode-cycle shortcut** | Frequent editing/reading transition is menu-only today | Small for three explicit palette entries via `set_mode`; medium if adding cycle ordering/state logic. Start with palette ports; choose a cycle key after platform review |
| **4** | **Settings → palette + Mod+,; Search and replace → Mod+H on Windows/Linux** | Two familiar operations gain direct access with very little new behavior | Small. Reuse `open_settings` and existing `ReplaceFile` action. Choose a separate macOS Replace binding; Cmd+H is system Hide |
| **5** | **Back/Forward → palette + View menu + shortcuts** | History currently requires pointer access; benefits notebook and standalone-file navigation | Small. Reuse `navigate_history`. Candidate Windows/Linux Alt+Left/Right; macOS needs a different pair because Option+Left/Right already move by word |
| **6** | **Toggle sidebar / Show Explorer → palette + View menu entry for Explorer + selected shortcuts** | Lets keyboard users reclaim space and return from Search to Explorer | Small. Reuse `toggle_sidebar` / `show_explorer`. Candidate sidebar Mod+Shift+B avoids reserving Mod+B, which is better used for Bold |
| **7** | **Go to a note + Command palette → visible menu entries; Open file + Save + Manage notebooks + Keyboard shortcuts → palette** | Makes discovery entry points visible and turns the palette into a more complete index of app actions | Small. Preserve Mod+O behavior; offer explicit Open File as a separate palette entry. No need to put every low-frequency action on the toolbar |
| **8** | **Toggle selected tasks → palette; Insert current date → optional toolbar item or shortcut** | Reuses useful existing operations that are currently hard to discover | Small for palette/task and date shortcut; medium for a persisted toolbar date item. Label task completion distinctly from task-list formatting |
| **9** | **Rename → F2 in NotebookTree; selected-entry Reveal / Copy relative path → palette** | Speeds file organization and removes context-menu hunting | Medium. Must capture the selected Explorer entry explicitly and preserve generation/root/path guards; “selected file” must not silently mean active document |
| **10** | **Sort order → palette picker; theme toggle → View menu** | Broadens discoverability for infrequent configuration actions | Small–medium; reuse `FileSort::OPTIONS`, `set_file_sort`, and current theme update. Lower frequency than editing/navigation |

Recommended first batch: ranks **1–4**, each delivered as a separate improvement/commit. If minimizing engineering effort is the overriding goal, ship Settings and Replace bindings first, then the explicit mode palette entries, then formatting and tab lifecycle ports.

For a toolbar-focused follow-up, prioritize **New Note** and **Insert Date** as optional configurable items. A compact mode selector could follow. Save already has menu/shortcut access and autosave; additional sort/theme/file-management icons offer less value per unit of toolbar space.

Context-only table operations are useful later palette candidates, but require a stable “current table/cell” target and an application-level Editor boundary. Bulk-porting them now is higher effort than reusing existing formatting operations. Destructive file operations likewise need explicit target labels and existing confirmation/lifecycle handling.

## 7. Correctness and discovery issues to resolve with ports

1. **Task formatting and task completion are different.** Toolbar Task list applies/removes `- [ ]` list syntax. Mod+L calls `toggle_selected_tasks`, creating or toggling task state. Keep distinct command identities and labels.
2. **Open is not Go to.** Mod+O uses notebook discovery when a notebook is active, whereas File > Open always requests the OS picker. Do not advertise them as the same action.
3. **macOS line-move bindings collide.** The application maps Ctrl+Up/Down to Cmd+Up/Down, while nested Input already binds those keys to document start/end. Confirm dispatch behavior with headless key tests and select an unambiguous line-move binding; this audit establishes the registration collision, not native runtime precedence.
4. **Reading Previous Find differs from Help.** Help broadly documents Shift+Enter for previous file-find match, but Reading uses the secondary Enter action, currently Cmd+Enter/Ctrl+Enter. Align behavior or document the difference.
5. **Shortcut display is partly duplicated.** Help derives app bindings from `app_shortcuts`, but contextual Help rows are handwritten and the Search toolbar tooltip hardcodes `Ctrl+Shift+F`. Reuse platform-aware labels for new ports and correct that tooltip on macOS.
6. **Useful basic Windows movement bindings are absent.** Consider Ctrl+Home/End and their Shift variants, plus Ctrl+Shift+Z redo alias, as small additions using existing input actions. This would be a separate editor-binding improvement.
7. **Menu access is not direct action access.** Alt+F plus arrows does not count as an Open or Settings shortcut. Similarly, F2 is currently absent despite Shift+F10 making Rename indirectly reachable.

## 8. Implementation approach and verification

The app already has four pieces of command wiring: the pure `Command` catalogue, GPUI actions in `main.rs`, bindings/Help in `help_ui.rs`, and direct toolbar/menu closures. A port should add an entry point to the existing owner rather than duplicate the operation.

For formatting, replace the toolbar-specific dispatch naming with a shared guarded formatting method and have toolbar, palette and document actions call it. Keep `markdown_edit` pure and source edits behind `Editor`. For navigation, notebooks, saving and tab closure, call their existing session/lifecycle owners. Preserve the distinction between an active document, a selected Explorer entry and a menu's captured target.

Add shared metadata only where it removes actual label/shortcut/capability duplication. A full command-framework rewrite is not a prerequisite for the first ports. Expand capabilities when a new command needs history, tabs, index writability or selected-entry state; do not pretend every command is controlled by `can_edit_document`.

Suggested headless checks when implementing:

- Catalogue presence/labels and capability behavior for empty, image, Reading, editable, standalone and notebook states.
- Actual shortcut dispatch in document and form-input contexts, including macOS collisions and dialog blocking.
- Equivalent formatting result, selection and one-step undo from toolbar, palette and shortcut; stale heading/menu targets rejected.
- New Note root targeting versus folder-context creation; empty-tab creation remains a separate operation.
- Dirty last-view tab close, shared-document retained views, save failure and cancellation through the existing close owner.
- Back/Forward history, failed navigation and focus restoration; sidebar/search visibility and focus.
- Platform-aware Help/menu/tooltip labels; preserved toolbar settings migration if new configurable items are added.

This documentation-only audit used source registration/handler tracing, count checks, local link validation and `git diff --check`. No native app was launched and no runtime/test-suite pass is claimed. Existing unrelated working-tree changes were left untouched.
