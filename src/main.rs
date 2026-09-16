#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod app;
mod assets;
mod autosave;
#[cfg(feature = "editor-diagnostics")]
mod benchmark;
#[cfg(feature = "editor-diagnostics")]
mod find_diagnostics;
mod goto;
mod help_ui;
mod image_import;
mod images;
#[cfg(test)]
mod lifecycle_tests;
mod links;
mod notebook_ui;
mod reading_search;
mod shared_documents;

mod search_ui;
mod settings_ui;
mod sidebar_ui;
mod tabs;
mod toolbar_ui;

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use app::session::{Content, OperationId, Session};
use gpui::{prelude::*, *};
use gpui_component::{
    ActiveTheme, Root, Sizable, WindowExt,
    button::{Button, ButtonVariants},
    input::{Input, InputEvent, InputState, Rope, Search},
    menu::{PopupMenu, PopupMenuItem},
    text::TextView,
};
use mithril::editor::{Editor, Event as EditorEvent};
use mithril::{ViewMode, document, notebook, search, templates};

actions!(
    notes,
    [
        Open,
        GoTo,
        CommandPalette,
        SearchAllFiles,
        ReplaceFile,
        ToggleTasks,
        MoveLineUp,
        MoveLineDown,
        EditLink,
        Save,
        SourceMode,
        LivePreview,
        ReadingView,
        CloseReadingFind,
        FileMenu,
        ViewMenu,
        SearchMenu,
        NotebooksMenu,
        HelpMenu,
        KeyboardShortcuts,
        About,
        ManageNotebooks,
        OpenSettings,
        CloseSettings,
        TreeUp,
        TreeDown,
        TreeLeft,
        TreeRight,
        TreeContextMenu,
        TreeOpen
    ]
);

#[derive(Clone)]
enum Next {
    Prepared,
    History(app::navigation::Visit),
    Nothing,
    Open,
    Close,
    File(PathBuf),
    FileFromPicker(PathBuf),
    NewNote(PathBuf),
    Entry {
        target: notebook_ui::EntryTarget,
        action: notebook_ui::EntryAction,
    },
    TreeFile(PathBuf),
    SearchResult(search::Destination),
    RemoveNotebook(PathBuf),
    Notebook {
        path: PathBuf,
        create: bool,
    },
}

#[derive(Clone, Copy, PartialEq)]
enum MenuKind {
    File,
    View,
    Search,
    Sort,
    Notebooks,
    Help,
}

#[derive(Clone)]
struct Document {
    id: u64,
    path: PathBuf,
    baseline: std::sync::Arc<std::sync::Mutex<DocumentBaseline>>,
    #[cfg(feature = "editor-diagnostics")]
    _diagnostic_fixture: Option<std::sync::Arc<tempfile::TempDir>>,
}

struct DocumentBaseline {
    saved_rope: Rope,
    format: document::TextFormat,
    version: document::DiskVersion,
}

impl Document {
    fn open(path: PathBuf) -> io::Result<Self> {
        static SERIAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        let loaded = document::open(&path)?;
        let id = SERIAL
            .fetch_update(
                std::sync::atomic::Ordering::Relaxed,
                std::sync::atomic::Ordering::Relaxed,
                |id| id.checked_add(1),
            )
            .map_err(|_| io::Error::other("Document identity exhausted"))?;
        Ok(Self {
            id,
            path: loaded.path,
            baseline: std::sync::Arc::new(std::sync::Mutex::new(DocumentBaseline {
                saved_rope: Rope::from(loaded.text.as_str()),
                format: loaded.format,
                version: loaded.version,
            })),
            #[cfg(feature = "editor-diagnostics")]
            _diagnostic_fixture: None,
        })
    }
    fn saved_rope(&self) -> Rope {
        self.baseline.lock().unwrap().saved_rope.clone()
    }
    fn saved(&self) -> String {
        self.saved_rope().to_string()
    }
    fn format(&self) -> document::TextFormat {
        self.baseline.lock().unwrap().format
    }
    fn version(&self) -> document::DiskVersion {
        self.baseline.lock().unwrap().version.clone()
    }
    fn set_version(&self, version: document::DiskVersion) {
        self.baseline.lock().unwrap().version = version;
    }
    fn accept_save(&self, snapshot: Rope, version: document::DiskVersion) {
        let mut baseline = self.baseline.lock().unwrap();
        baseline.saved_rope = snapshot;
        baseline.version = version;
    }
}

struct Notes {
    tabs: tabs::TabSet,
    documents: std::collections::HashMap<PathBuf, shared_documents::DocumentRuntime>,
    workspace: Option<tabs::WorkspaceOperation>,
    session_autosave: notebook::Autosave,
    notebook_cache: std::collections::HashMap<PathBuf, notebook::Notebook>,
    sidebar_collapsed: bool,
    sidebar_resize: Entity<gpui_component::resizable::ResizableState>,
    session_date_format: String,
    session_show_file_extensions: bool,
    session_toolbar: notebook::ToolbarSettings,
    session_always_hide_extensions: Vec<String>,
    settings_panel: Option<settings_ui::SettingsDialog>,
    installed_fonts: Vec<String>,
    palette: Option<goto::Palette>,
    search: Option<Entity<search_ui::SearchPanel>>,
    search_subscription: Option<Subscription>,
    search_destination: Option<search::Destination>,
    focus: FocusHandle,
    menu: Option<(MenuKind, Entity<PopupMenu>)>,
    menu_previous_focus: Option<FocusHandle>,
    menu_subscription: Option<Subscription>,
    notebooks: notebook_ui::NotebookState,
    session_appearance: notebook::Appearance,
    startup_dark: bool,
}

impl Notes {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let tabs = tabs::TabSet::new(window, cx);
        let weak = cx.weak_entity();
        window.on_window_should_close(cx, move |window, cx| {
            weak.update(cx, |this, cx| {
                if this.is_busy() {
                    return false;
                }
                this.request(Next::Close, window, cx);
                false
            })
            .unwrap_or(true)
        });
        let focus = cx.focus_handle();
        focus.focus(window);
        let notebooks = notebook_ui::NotebookState::new(window, cx);
        let startup_dark = gpui_component::ThemeMode::from(window.appearance()).is_dark();
        Self {
            tabs,
            documents: Default::default(),
            workspace: None,
            session_autosave: Default::default(),
            notebook_cache: Default::default(),
            sidebar_collapsed: false,
            sidebar_resize: cx.new(|_| gpui_component::resizable::ResizableState::default()),
            session_date_format: templates::default_format(),
            session_show_file_extensions: notebook::default_show_file_extensions(),
            session_toolbar: notebook::ToolbarSettings::default(),
            session_always_hide_extensions: notebook::default_always_hide_extensions(),
            settings_panel: None,
            installed_fonts: cx.text_system().all_font_names(),
            palette: None,
            search: None,
            search_subscription: None,
            search_destination: None,
            focus,
            menu: None,
            menu_previous_focus: None,
            menu_subscription: None,
            notebooks,
            session_appearance: notebook::Appearance {
                dark_mode: Some(startup_dark),
                ..Default::default()
            },
            startup_dark,
        }
    }

    fn subscribe_tab_editor(
        id: tabs::TabId,
        editor: &Editor,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Subscription {
        let editor_id = editor.id();
        editor.subscribe(window, cx, move |this, event, window, cx| {
            if this
                .tabs
                .get(id)
                .is_none_or(|tab| tab.editor.id() != editor_id)
            {
                return;
            }
            match event {
                EditorEvent::OpenLink(destination) => {
                    if this.tabs.active.id == id {
                        this.open_link(destination, window, cx);
                    }
                }
                EditorEvent::Changed => {
                    // Standalone diagnostic inputs can bypass document installation.
                    // Production documents notify through the runtime's buffer subscription.
                    if !this.tabs.get(id).unwrap().editor.has_shared_buffer(cx)
                        && let Some(document) = this
                            .tabs
                            .get(id)
                            .and_then(|tab| tab.document())
                            .map(|doc| doc.id)
                    {
                        this.document_edited(document, window, cx);
                    }
                }
            }
        })
    }
    fn dirty(&self, cx: &App) -> bool {
        self.tabs.active.observed(cx).dirty
    }
    fn check_dirty(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.check_tab_dirty(self.tabs.active.id, window, cx);
    }
    fn check_tab_dirty(&mut self, id: tabs::TabId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tab) = self.tabs.get(id) else {
            return;
        };
        let Some(document) = tab.document().cloned() else {
            return;
        };
        let saved = document.saved_rope();
        let current = tab.editor.snapshot(cx);
        for id in self.document_tabs(document.id) {
            let tab = self.tabs.get_mut(id).unwrap();
            tab.dirty_revision += 1;
            tab.dirty_cached = true;
        }
        let Some(runtime) = self.documents.get_mut(&document.path) else {
            return;
        };
        runtime.dirty_revision += 1;
        let revision = runtime.dirty_revision;
        runtime.dirty_task = Some(cx.spawn_in(window, async move |this, cx| {
            let dirty = cx
                .background_executor()
                .spawn(async move { saved != current })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                if !this.documents.values().any(|runtime| {
                    runtime.document.id == document.id && runtime.dirty_revision == revision
                }) {
                    return;
                }
                for id in this.document_tabs(document.id) {
                    this.tabs.get_mut(id).unwrap().dirty_cached = dirty;
                }
                this.refresh(window, cx);
            });
        }));
        self.refresh(window, cx);
    }

    fn window_title(&self) -> String {
        let name = self
            .document()
            .map(|d| d.path.as_path())
            .or(self.image_path())
            .and_then(|p| p.file_name())
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "Mithril".into());
        format!(
            "{}{} — Mithril",
            if self.tabs.active.dirty_cached {
                "* "
            } else {
                ""
            },
            name
        )
    }

    fn refresh(&self, window: &mut Window, cx: &mut Context<Self>) {
        window.set_window_title(&self.window_title());
        cx.notify();
    }

    fn is_busy(&self) -> bool {
        self.workspace.is_some() || self.tabs.iter().any(|tab| tab.session.is_busy())
    }

    /// User interaction is paused independently of session I/O and autosave.
    fn document_interaction_blocked(&self) -> bool {
        self.is_busy() || self.settings_panel.is_some()
    }

    fn document(&self) -> Option<&Document> {
        match self.tabs.active.session.content() {
            Content::Document { value, .. } => Some(value),
            _ => None,
        }
    }

    fn capabilities(&self) -> mithril::capabilities::Capabilities {
        mithril::capabilities::Capabilities {
            has_document: self.document().is_some(),
            mode: self.tabs.active.mode,
            busy: self.document_interaction_blocked(),
            has_notebook: self.notebooks.active.is_some(),
        }
    }

    fn can_edit_document(&self) -> bool {
        self.capabilities().can_edit_document()
    }

    fn set_mode(&mut self, mode: ViewMode, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings_panel.is_some() {
            return;
        }
        self.record_activation_preference(notebook_ui::PreferenceIntent::ViewMode(mode));
        self.tabs.active.reading_text = if mode == ViewMode::Reading {
            self.tabs.active.editor.text(cx)
        } else {
            SharedString::default()
        };
        self.tabs.active.mode = mode;
        if mode == ViewMode::Reading {
            self.update_reading_search(cx);
        }
        if let Some(book) = &mut self.notebooks.active
            && book.settings.view_mode != mode
        {
            book.settings.view_mode = mode;
            self.persist_notebooks(cx);
        }
        self.tabs.active.editor.set_mode(mode, cx);
        if mode != ViewMode::Reading && self.document().is_some() {
            self.tabs.active.editor.focus(window, cx);
        } else {
            self.focus.focus(window);
        }
        cx.notify();
    }

    fn show_menu(&mut self, kind: MenuKind, window: &mut Window, cx: &mut Context<Self>) {
        if window.has_active_dialog(cx) {
            return;
        }
        self.menu_previous_focus = window.focused(cx);
        let busy = self.document_interaction_blocked();
        let can_save = self.document().is_some() && !busy;
        let mode = self.tabs.active.mode;
        let capabilities = self.capabilities();
        let focus = self.focus.clone();
        let owner = cx.weak_entity();
        let menu = PopupMenu::build(window, cx, |menu, _, cx| {
            let menu = menu.action_context(focus);
            if kind == MenuKind::File {
                menu.menu_with_disabled("Open…", Box::new(Open), busy)
                    .menu_with_disabled("Save", Box::new(Save), !can_save)
                    .separator()
                    .menu_with_disabled("Settings", Box::new(OpenSettings), self.is_busy())
            } else if kind == MenuKind::View {
                menu.menu_with_check_and_disabled(
                    "Source mode",
                    mode == ViewMode::Source,
                    Box::new(SourceMode),
                    busy,
                )
                .menu_with_check_and_disabled(
                    "Live preview",
                    mode == ViewMode::LivePreview,
                    Box::new(LivePreview),
                    busy,
                )
                .menu_with_check_and_disabled(
                    "Reading view",
                    mode == ViewMode::Reading,
                    Box::new(ReadingView),
                    busy,
                )
                .separator()
                .item(
                    PopupMenuItem::new("Collapse sidebar")
                        .checked(self.sidebar_collapsed)
                        .disabled(self.is_busy() || self.notebooks.active.is_none())
                        .on_click(move |_, window, cx| {
                            let _ = owner.update(cx, |_, cx| {
                                // Restore sidebar focus after the menu finishes dismissing.
                                cx.defer_in(window, |this, window, cx| {
                                    this.toggle_sidebar(window, cx);
                                });
                            });
                        }),
                )
            } else if kind == MenuKind::Search {
                use mithril::commands::Command;
                menu.menu_with_disabled(
                    "Search in file",
                    Box::new(Search),
                    Command::SearchFile.unavailable(capabilities).is_some(),
                )
                .menu_with_disabled(
                    "Search in all files",
                    Box::new(SearchAllFiles),
                    Command::SearchAllFiles.unavailable(capabilities).is_some(),
                )
                .menu_with_disabled(
                    "Search and replace in file",
                    Box::new(ReplaceFile),
                    Command::ReplaceFile.unavailable(capabilities).is_some(),
                )
            } else if kind == MenuKind::Sort {
                self.file_sort_menu(menu, owner)
            } else if kind == MenuKind::Help {
                menu.menu("Keyboard shortcuts", Box::new(KeyboardShortcuts))
                    .menu("About", Box::new(About))
            } else {
                self.notebook_menu(menu, cx)
            }
        });
        self.menu_subscription =
            Some(
                cx.subscribe_in(&menu, window, |this, _, _: &DismissEvent, _, cx| {
                    this.menu = None;
                    cx.notify();
                }),
            );
        menu.focus_handle(cx).focus(window);
        self.menu = Some((kind, menu));
        cx.notify();
    }
}

impl Render for Notes {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let content_size = px(self.appearance().content_font_size as f32);
        self.sync_settings_panel(window, cx);
        if let Some(search) = &self.search {
            search.update(cx, |panel, cx| {
                panel.set_interaction_enabled(self.settings_panel.is_none(), cx);
            });
        }
        let content = if let Some(path) = self.image_path() {
            images::viewer(path).into_any_element()
        } else if let Some(document_path) = self.document().map(|document| document.path.clone()) {
            if self.tabs.active.mode != ViewMode::Reading {
                div()
                    .key_context("DocumentEditor")
                    .on_action(cx.listener(|this, _: &MoveLineUp, window, cx| {
                        if this.can_edit_document()
                            && this.tabs.active.editor.is_focused(window, cx)
                        {
                            this.tabs.active.editor.move_lines(false, window, cx);
                        }
                    }))
                    .on_action(cx.listener(|this, _: &MoveLineDown, window, cx| {
                        if this.can_edit_document()
                            && this.tabs.active.editor.is_focused(window, cx)
                        {
                            this.tabs.active.editor.move_lines(true, window, cx);
                        }
                    }))
                    .on_action(cx.listener(|this, _: &ToggleTasks, window, cx| {
                        if this.can_edit_document()
                            && this.tabs.active.editor.focus_handle(cx).is_focused(window)
                        {
                            this.tabs.active.editor.toggle_selected_tasks(window, cx);
                        }
                    }))
                    .size_full()
                    .font_family(self.content_font(cx))
                    .child(
                        self.tabs
                            .active
                            .editor
                            .widget()
                            .h_full()
                            .appearance(false)
                            .text_size(content_size)
                            .line_height(relative(1.5))
                            .disabled(self.is_busy()),
                    )
                    .into_any_element()
            } else {
                let reading = self
                    .tabs
                    .active
                    .reading_view
                    .get_or_insert_with(|| {
                        TextView::markdown(
                            ("reading", self.tabs.active.editor.id().as_u64() as usize),
                            self.tabs.active.reading_text.clone(),
                            window,
                            cx,
                        )
                    })
                    .clone()
                    .text(self.tabs.active.reading_text.clone());
                div()
                    .debug_selector(|| "reading-content".into())
                    .size_full()
                    .p_6()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .font_family(self.content_font(cx))
                    .key_context("ReadingContent")
                    .on_action(cx.listener(|this, _: &CloseReadingFind, window, cx| {
                        this.tabs.active.reading_search = None;
                        this.focus.focus(window);
                        cx.notify();
                    }))
                    .when(self.tabs.active.reading_search.is_some(), |view| {
                        view.child(self.render_reading_find(cx))
                    })
                    .child(
                        reading
                            .image_document(document_path)
                            .source_highlights(
                                self.tabs
                                    .active
                                    .reading_search
                                    .as_ref()
                                    .map(|find| find.highlights.clone()),
                            )
                            .on_link_open({
                                let owner = cx.weak_entity();
                                let identity = self.document_id();
                                let tab_id = self.tabs.active.id;
                                move |destination, source, window, cx| {
                                    let _ = owner.update(cx, |this, cx| {
                                        if this.tabs.active.id == tab_id
                                            && this.document_id() == identity
                                            && this.tabs.active.editor.text(cx).as_ref() == source
                                        {
                                            this.open_link(destination, window, cx);
                                        }
                                    });
                                }
                            })
                            .on_task_toggle({
                                let owner = cx.weak_entity();
                                let document_path = self.document().map(|d| d.path.clone());
                                let tab_id = self.tabs.active.id;
                                move |offset, source, window, cx| {
                                    let _ = owner.update(cx, |this, cx| {
                                        if this.document_interaction_blocked()
                                            || this.tabs.active.id != tab_id
                                            || this.document().map(|d| &d.path)
                                                != document_path.as_ref()
                                        {
                                            return;
                                        }
                                        this.tabs
                                            .active
                                            .editor
                                            .toggle_task(offset, source, window, cx);
                                    });
                                }
                            })
                            .text_size(content_size)
                            .line_height(relative(1.5))
                            .style(gpui_component::text::TextViewStyle {
                                heading_base_font_size: content_size,
                                highlight_theme: cx.theme().highlight_theme.clone(),
                                is_dark: cx.theme().mode.is_dark(),
                                ..Default::default()
                            })
                            .selectable(true)
                            .scrollable(true)
                            .min_h_0()
                            .flex_1()
                            .size_full(),
                    )
                    .into_any_element()
            }
        } else {
            div()
                .debug_selector(|| "empty-content".into())
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_color(cx.theme().muted_foreground)
                .child(if self.notebooks.active.is_some() {
                    "Select a Markdown file or image from the notebook sidebar."
                } else {
                    "Open a Markdown file with File → Open… (Ctrl+O)"
                })
                .into_any_element()
        };
        div()
            .id("notes")
            .key_context("Notes")
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &EditLink, window, cx| this.edit_link(window, cx)))
            .on_action(cx.listener(|this, _: &Search, window, cx| {
                this.search_current_file(window, cx);
            }))
            .on_action(cx.listener(|this, _: &ReplaceFile, window, cx| {
                if mithril::commands::Command::ReplaceFile
                    .unavailable(this.capabilities())
                    .is_none()
                    && !window.has_active_dialog(cx)
                {
                    this.tabs.active.editor.find_and_replace(window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &GoTo, window, cx| this.go_to(window, cx)))
            .on_action(
                cx.listener(|this, _: &SearchAllFiles, window, cx| {
                    this.open_search_all(window, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &CommandPalette, window, cx| {
                    this.command_palette(window, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &Open, window, cx| this.request(Next::Open, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &Save, window, cx| this.save(Next::Nothing, window, cx)),
            )
            .on_action(cx.listener(|this, _: &SourceMode, window, cx| {
                this.set_mode(ViewMode::Source, window, cx)
            }))
            .on_action(cx.listener(|this, _: &LivePreview, window, cx| {
                this.set_mode(ViewMode::LivePreview, window, cx)
            }))
            .on_action(cx.listener(|this, _: &ReadingView, window, cx| {
                this.set_mode(ViewMode::Reading, window, cx)
            }))
            .on_action(cx.listener(|this, _: &HelpMenu, window, cx| {
                this.show_menu(MenuKind::Help, window, cx)
            }))
            .on_action(cx.listener(|this, _: &KeyboardShortcuts, window, cx| {
                this.open_help(true, window, cx)
            }))
            .on_action(cx.listener(|this, _: &About, window, cx| this.open_help(false, window, cx)))
            .on_action(cx.listener(|this, _: &FileMenu, window, cx| {
                this.show_menu(MenuKind::File, window, cx)
            }))
            .on_action(cx.listener(|this, _: &ViewMenu, window, cx| {
                this.show_menu(MenuKind::View, window, cx)
            }))
            .on_action(cx.listener(|this, _: &SearchMenu, window, cx| {
                this.show_menu(MenuKind::Search, window, cx)
            }))
            .on_action(cx.listener(|this, _: &NotebooksMenu, window, cx| {
                this.show_menu(MenuKind::Notebooks, window, cx)
            }))
            .on_action(cx.listener(|this, _: &ManageNotebooks, window, cx| {
                this.manage_notebooks(window, cx)
            }))
            .on_action(
                cx.listener(|this, _: &OpenSettings, window, cx| this.open_settings(window, cx)),
            )
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(
                div()
                    .flex()
                    .gap_1()
                    .p_1()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(
                        Button::new("file-menu")
                            .cursor_default()
                            .ghost()
                            .small()
                            .label("File")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.show_menu(MenuKind::File, window, cx)
                            })),
                    )
                    .child(
                        Button::new("view-menu")
                            .cursor_default()
                            .ghost()
                            .small()
                            .label("View")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.show_menu(MenuKind::View, window, cx)
                            })),
                    )
                    .child(
                        Button::new("search-menu")
                            .cursor_default()
                            .ghost()
                            .small()
                            .label("Search")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.show_menu(MenuKind::Search, window, cx)
                            })),
                    )
                    .child(
                        Button::new("sort-menu")
                            .cursor_default()
                            .ghost()
                            .small()
                            .label("Sort")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.show_menu(MenuKind::Sort, window, cx)
                            })),
                    )
                    .child(
                        Button::new("notebooks-menu")
                            .cursor_default()
                            .ghost()
                            .small()
                            .label("Notebooks")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.show_menu(MenuKind::Notebooks, window, cx)
                            })),
                    )
                    .child(
                        Button::new("help-menu")
                            .cursor_default()
                            .ghost()
                            .small()
                            .label("Help")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.show_menu(MenuKind::Help, window, cx)
                            })),
                    ),
            )
            .when(self.toolbar_settings().any_enabled(), |view| {
                view.child(self.render_toolbar(window, cx))
            })
            .when_some(
                self.save_state(self.tabs.active.id).error.clone(),
                |view, error| view.child(div().p_2().text_sm().child(error)),
            )
            .when_some(self.notebooks.notice.clone(), |view, notice| {
                view.child(div().p_2().text_sm().child(notice))
            })
            .child(
                div().flex().flex_1().min_h_0().overflow_hidden().child(
                    self.render_sidebar_layout(
                        div()
                            .id("file-drop-content")
                            .debug_selector(|| "file-drop-content".into())
                            .size_full()
                            .flex()
                            .flex_col()
                            .on_drop(cx.listener(|this, paths: &ExternalPaths, window, cx| {
                                cx.stop_propagation();
                                this.drop_files(paths.paths().to_vec(), window, cx);
                            }))
                            .on_drop(cx.listener(
                                |this, drag: &notebook_ui::EntryDrag, window, cx| {
                                    cx.stop_propagation();
                                    this.drop_files(vec![drag.root.join(&drag.path)], window, cx);
                                },
                            ))
                            .child(self.render_tabs(cx))
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .min_h_0()
                                    .overflow_hidden()
                                    .child(content),
                            )
                            .into_any_element(),
                        window,
                        cx,
                    ),
                ),
            )
            .when_some(
                self.notebooks.context_menu.clone(),
                |view, (position, menu)| {
                    view.child(deferred(
                        gpui::anchored()
                            .position(position)
                            .snap_to_window_with_margin(px(8.))
                            .child(menu),
                    ))
                },
            )
            .when_some(self.menu.clone(), |this, (kind, menu)| {
                this.child(deferred(
                    div()
                        .absolute()
                        .top(rems(2.25))
                        .left(rems(
                            match kind {
                                MenuKind::File => 0.25,
                                MenuKind::View => 3.375,
                                MenuKind::Search => 6.75,
                                MenuKind::Sort => 11.,
                                MenuKind::Notebooks => 14.125,
                                MenuKind::Help => 19.875,
                            } + if self.notebooks.active.is_some() {
                                2.
                            } else {
                                0.
                            },
                        ))
                        .child(menu),
                ))
            })
    }
}

// Build modal layers outside Notes::render: their builder updates Notes, and
// GPUI entities cannot be borrowed recursively during their own render.
struct NotesWindow {
    notes: Entity<Notes>,
    _subscription: Subscription,
}

impl Render for NotesWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .child(self.notes.clone())
            .children(Root::render_dialog_layer(window, cx))
            .text_size(cx.theme().font_size)
            .font_family(cx.theme().font_family.clone())
    }
}

fn main() {
    #[cfg(feature = "editor-diagnostics")]
    if let Some(output) =
        std::env::var_os("MITHRIL_BENCH").or_else(|| std::env::var_os("MITHRIL_NOTEBOOK_CHECK"))
    {
        let output = PathBuf::from(output);
        let _ = fs::write(
            output.with_extension("progress.txt"),
            "starting application",
        );
        std::panic::set_hook(Box::new(move |info| {
            let _ = fs::write(output.with_extension("panic.txt"), info.to_string());
        }));
    }
    Application::new()
        .with_assets(assets::Assets)
        .run(|cx: &mut App| {
            gpui_component::init(cx);
            cx.bind_keys(
                help_ui::app_shortcuts()
                    .into_iter()
                    .map(|shortcut| shortcut.binding),
            );
            cx.on_window_closed(|cx| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
            let bounds = Bounds::centered(None, size(px(900.), px(700.)), cx);
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    window_min_size: Some(size(px(400.), px(300.))),
                    app_id: Some("org.mithril.Mithril".into()),
                    titlebar: Some(TitlebarOptions {
                        title: Some("Mithril".into()),
                        // Keep native controls after reported close failures and
                        // hangs with the custom title bar; TestPlatform cannot verify it.
                        appears_transparent: false,
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                |window, cx| {
                    let notes = cx.new(|cx| Notes::new(window, cx));
                    notes.update(cx, |this, cx| this.apply_appearance(window, cx));
                    let diagnostic_run = cfg!(feature = "editor-diagnostics")
                        && (std::env::var_os("MITHRIL_BENCH").is_some()
                            || std::env::var_os("MITHRIL_NOTEBOOK_CHECK").is_some()
                            || std::env::var_os("MITHRIL_AUTOSAVE_CHECK").is_some());
                    if !diagnostic_run {
                        notes.update(cx, |this, cx| this.restore_notebooks(window, cx));
                    }
                    #[cfg(feature = "editor-diagnostics")]
                    if let Some(output) = std::env::var_os("MITHRIL_BENCH") {
                        let notes = notes.clone();
                        window.on_next_frame(move |window, cx| {
                            benchmark::start(&notes, output.into(), window, cx)
                        });
                    }
                    #[cfg(feature = "editor-diagnostics")]
                    if let Some(output) = std::env::var_os("MITHRIL_NOTEBOOK_CHECK") {
                        notebook_ui::diagnostics::start(notes.clone(), output.into(), window, cx);
                    }
                    #[cfg(feature = "editor-diagnostics")]
                    if let Some(output) = std::env::var_os("MITHRIL_AUTOSAVE_CHECK") {
                        autosave::diagnostics::start(notes.clone(), output.into(), window, cx);
                    }
                    let view = cx.new(|cx| NotesWindow {
                        _subscription: cx.observe(&notes, |_, _, cx| cx.notify()),
                        notes,
                    });
                    cx.new(|cx| Root::new(view, window, cx))
                },
            )
            .expect("Could not create the Mithril window");
            cx.activate(true);
        });
}
