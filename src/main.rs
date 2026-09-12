#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod app;
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
mod search;
mod search_ui;
mod settings_ui;
mod sidebar_ui;

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use app::operation::{ForegroundOperation, OperationId};
use gpui::{prelude::*, *};
use gpui_component::{
    ActiveTheme, Root, Sizable, WindowExt,
    button::{Button, ButtonVariants},
    input::{Input, InputEvent, InputState, Rope, Search},
    menu::PopupMenu,
    text::TextView,
};
use gpui_note::{ViewMode, document, notebook, templates};

actions!(
    notes,
    [
        Open,
        GoTo,
        CommandPalette,
        SearchAllFiles,
        ToggleTasks,
        EditLink,
        Save,
        SourceMode,
        LivePreview,
        ReadingView,
        CloseReadingFind,
        FileMenu,
        ViewMenu,
        NotebooksMenu,
        HelpMenu,
        KeyboardShortcuts,
        About,
        ManageNotebooks,
        OpenSettings,
        TreeUp,
        TreeDown,
        TreeLeft,
        TreeRight,
        TreeOpen
    ]
);

#[derive(Clone)]
enum Next {
    Nothing,
    Open,
    Close,
    File(PathBuf),
    NewNote(PathBuf),
    TreeFile(PathBuf),
    SearchResult(search::Destination),
    RemoveNotebook(PathBuf),
    Notebook { path: PathBuf, create: bool },
}

#[derive(Clone, Copy, PartialEq)]
enum MenuKind {
    File,
    View,
    Notebooks,
    Help,
}

struct Document {
    path: PathBuf,
    saved: String,
    saved_rope: Rope,
    format: document::TextFormat,
    version: document::DiskVersion,
    #[cfg(feature = "editor-diagnostics")]
    _diagnostic_fixture: Option<tempfile::TempDir>,
}

impl Document {
    fn open(path: PathBuf) -> io::Result<Self> {
        let loaded = document::open(&path)?;
        Ok(Self {
            path: loaded.path,
            saved_rope: Rope::from(loaded.text.as_str()),
            saved: loaded.text,
            format: loaded.format,
            version: loaded.version,
            #[cfg(feature = "editor-diagnostics")]
            _diagnostic_fixture: None,
        })
    }
}

struct Notes {
    sidebar_collapsed: bool,
    sidebar_resize: Entity<gpui_component::resizable::ResizableState>,
    session_date_format: String,
    session_show_file_extensions: bool,
    session_always_hide_extensions: Vec<String>,
    settings_panel: Option<settings_ui::SettingsDialog>,
    installed_fonts: Vec<String>,
    saves: autosave::SaveState,
    palette: Option<goto::Palette>,
    search: Option<search_ui::SearchPanel>,
    search_destination: Option<search::Destination>,
    image_path: Option<PathBuf>,
    document: Option<Document>,
    editor: Entity<InputState>,
    editor_subscription: Subscription,
    mode: ViewMode,
    reading_text: SharedString,
    reading_search: Option<reading_search::ReadingSearch>,
    focus: FocusHandle,
    foreground: ForegroundOperation,
    dirty_cached: bool,
    dirty_revision: u64,
    dirty_task: Option<Task<()>>,
    menu: Option<(MenuKind, Entity<PopupMenu>)>,
    menu_previous_focus: Option<FocusHandle>,
    menu_subscription: Option<Subscription>,
    notebooks: notebook_ui::NotebookState,
    session_appearance: notebook::Appearance,
    startup_dark: bool,
}

impl Notes {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let editor = cx.new(|cx| {
            InputState::new(window, cx)
                .multi_line(true)
                .searchable(true)
        });
        let editor_subscription = Self::subscribe_editor(&editor, window, cx);
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
            sidebar_collapsed: false,
            sidebar_resize: cx.new(|_| gpui_component::resizable::ResizableState::default()),
            session_date_format: templates::default_format(),
            session_show_file_extensions: notebook::default_show_file_extensions(),
            session_always_hide_extensions: notebook::default_always_hide_extensions(),
            settings_panel: None,
            installed_fonts: cx.text_system().all_font_names(),
            saves: autosave::SaveState::default(),
            palette: None,
            search: None,
            search_destination: None,
            image_path: None,
            document: None,
            editor,
            editor_subscription,
            mode: ViewMode::default(),
            reading_text: SharedString::default(),
            reading_search: None,
            focus,
            foreground: ForegroundOperation::default(),
            dirty_cached: false,
            dirty_revision: 0,
            dirty_task: None,
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

    fn subscribe_editor(
        editor: &Entity<InputState>,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Subscription {
        cx.subscribe_in(editor, window, |this, _, event, window, cx| {
            if let InputEvent::OpenLink { destination } = event {
                this.open_link(destination, window, cx);
            }
            if matches!(event, InputEvent::Change) {
                this.check_dirty(window, cx);
                this.schedule_autosave(window, cx);
                this.schedule_search(window, cx);
                if this.mode == ViewMode::Reading {
                    this.reading_text = this.editor.read(cx).value();
                    this.update_reading_search(cx);
                }
            }
        })
    }

    fn dirty(&self, cx: &App) -> bool {
        self.document
            .as_ref()
            .is_some_and(|d| d.saved_rope != self.editor.read(cx).snapshot())
    }

    fn check_dirty(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(document) = self.document.as_ref() else {
            return;
        };
        let saved = document.saved_rope.clone();
        let current = self.editor.read(cx).snapshot();
        self.dirty_revision += 1;
        let revision = self.dirty_revision;
        self.dirty_cached = true;
        self.refresh(window, cx);
        self.dirty_task = Some(cx.spawn_in(window, async move |this, cx| {
            let dirty = cx
                .background_executor()
                .spawn(async move { saved != current })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.dirty_revision != revision {
                    return;
                }
                this.dirty_cached = dirty;
                this.refresh(window, cx);
            });
        }));
    }

    fn refresh(&self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self
            .document
            .as_ref()
            .map(|d| d.path.as_path())
            .or(self.image_path.as_deref())
            .and_then(|p| p.file_name())
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "GPUI Note".into());
        window.set_window_title(&format!(
            "{}{} — GPUI Note",
            if self.dirty_cached { "* " } else { "" },
            name
        ));
        cx.notify();
    }

    fn is_busy(&self) -> bool {
        self.foreground.is_busy()
    }

    fn error(
        &mut self,
        message: &str,
        error: impl std::fmt::Display,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(operation) = self.foreground.begin() else {
            return;
        };
        let prompt = window.prompt(
            PromptLevel::Critical,
            message,
            Some(&error.to_string()),
            &["OK"],
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            let _ = prompt.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.foreground.finish(operation) {
                    this.resume_document(window, cx);
                }
            });
        })
        .detach();
        cx.notify();
    }

    /// Resume a retained document after a cancelled or failed foreground operation.
    fn resume_document(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.schedule_autosave(window, cx);
        self.refresh(window, cx);
    }

    fn request(&mut self, next: Next, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_busy() {
            return;
        }
        self.saves.timer = None;
        if self.saves.writing {
            let Some(operation) = self.foreground.begin() else {
                return;
            };
            self.saves.pending = Some(autosave::Pending::Request(operation, next));
            cx.notify();
            return;
        }
        if !self.dirty(cx) {
            self.continue_with(next, window, cx);
            return;
        }
        let Some(operation) = self.foreground.begin() else {
            return;
        };
        let prompt = window.prompt(
            PromptLevel::Warning,
            "Save changes?",
            Some("This document has unsaved changes."),
            &[
                PromptButton::Ok("Save".into()),
                PromptButton::Other("Discard".into()),
                PromptButton::Cancel("Cancel".into()),
            ],
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            let answer = prompt.await.ok();
            let _ = this.update_in(cx, |this, window, cx| {
                this.answer_request(operation, answer, next, window, cx);
            });
        })
        .detach();
        cx.notify();
    }

    fn answer_request(
        &mut self,
        operation: OperationId,
        answer: Option<usize>,
        next: Next,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.foreground.finish(operation) {
            return;
        }
        match answer {
            Some(0) => self.save(next, window, cx),
            Some(1) => self.continue_with(next, window, cx),
            _ => {
                self.finish_search_navigation(false, window, cx);
                if matches!(next, Next::TreeFile(_)) {
                    self.restore_tree_cursor(window, cx);
                }
                self.resume_document(window, cx);
            }
        }
    }

    #[cfg(feature = "editor-diagnostics")]
    fn diagnostic_answer_request(
        &mut self,
        answer: Option<usize>,
        next: Next,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let operation = self
            .foreground
            .begin()
            .expect("diagnostic response needs an idle session");
        self.answer_request(operation, answer, next, window, cx);
    }

    fn continue_with(&mut self, next: Next, window: &mut Window, cx: &mut Context<Self>) {
        match next {
            Next::Nothing => self.refresh(window, cx),
            Next::Open => self.open(window, cx),
            Next::NewNote(root) => self.create_note(root, window, cx),
            Next::Close => self.finish_close(window, cx),
            Next::File(path) => self.open_path(path, false, window, cx),
            Next::TreeFile(path) => self.open_path(path, true, window, cx),
            Next::SearchResult(destination) => {
                let path = destination.path.clone();
                self.search_destination = Some(destination);
                self.open_path(path, false, window, cx);
            }
            Next::RemoveNotebook(path) => self.remove_notebook(path, window, cx),
            Next::Notebook { path, create } => self.activate_notebook(path, create, window, cx),
        }
    }

    fn open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(operation) = self.foreground.begin() else {
            return;
        };
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Open".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = match picker.await {
                Ok(Ok(Some(paths))) => match paths.into_iter().next() {
                    Some(path) => {
                        cx.background_executor()
                            .spawn(async move { Document::open(path).map(Some) })
                            .await
                    }
                    None => Ok(None),
                },
                Ok(Ok(None)) => Ok(None),
                Ok(Err(error)) => Err(io::Error::other(error.to_string())),
                Err(error) => Err(io::Error::other(error.to_string())),
            };
            let _ = this.update_in(cx, |this, window, cx| {
                this.finish_open(operation, result, window, cx);
            });
        })
        .detach();
        cx.notify();
    }

    fn finish_open(
        &mut self,
        operation: OperationId,
        result: io::Result<Option<Document>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.foreground.finish(operation) {
            return;
        }
        match result {
            Ok(Some(document)) => {
                self.install_document(Some(document), window, cx);
                self.remember_file(cx);
                self.refresh(window, cx);
            }
            Ok(None) => self.resume_document(window, cx),
            Err(error) => self.error("Could not open file", error, window, cx),
        }
    }

    fn set_mode(&mut self, mode: ViewMode, window: &mut Window, cx: &mut Context<Self>) {
        self.record_activation_preference(notebook_ui::PreferenceIntent::ViewMode(mode));
        self.reading_text = if mode == ViewMode::Reading {
            self.editor.read(cx).value()
        } else {
            SharedString::default()
        };
        self.mode = mode;
        if mode == ViewMode::Reading {
            self.update_reading_search(cx);
        }
        if let Some(book) = &mut self.notebooks.active
            && book.settings.view_mode != mode
        {
            book.settings.view_mode = mode;
            self.persist_notebooks(cx);
        }
        self.editor.update(cx, |editor, cx| {
            editor.set_live_preview(mode == ViewMode::LivePreview, cx)
        });
        if mode != ViewMode::Reading && self.document.is_some() {
            self.editor
                .update(cx, |editor, cx| editor.focus(window, cx));
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
        let busy = self.is_busy();
        let can_save = self.document.is_some() && !busy;
        let mode = self.mode;
        let focus = self.focus.clone();
        let menu = PopupMenu::build(window, cx, |menu, _, cx| {
            let menu = menu.action_context(focus);
            if kind == MenuKind::File {
                menu.menu_with_disabled("Open…", Box::new(Open), busy)
                    .menu_with_disabled("Save", Box::new(Save), !can_save)
                    .separator()
                    .menu_with_disabled("Settings", Box::new(OpenSettings), busy)
            } else if kind == MenuKind::View {
                menu.menu_with_check(
                    "Source mode",
                    mode == ViewMode::Source,
                    Box::new(SourceMode),
                )
                .menu_with_check(
                    "Live preview",
                    mode == ViewMode::LivePreview,
                    Box::new(LivePreview),
                )
                .menu_with_check(
                    "Reading view",
                    mode == ViewMode::Reading,
                    Box::new(ReadingView),
                )
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
        let content = if let Some(path) = &self.image_path {
            images::viewer(path).into_any_element()
        } else if let Some(document) = &self.document {
            if self.mode != ViewMode::Reading {
                div()
                    .key_context("DocumentEditor")
                    .on_action(cx.listener(|this, _: &ToggleTasks, window, cx| {
                        if this.can_edit_document()
                            && this.editor.focus_handle(cx).is_focused(window)
                        {
                            this.editor
                                .update(cx, |editor, cx| editor.toggle_selected_tasks(window, cx));
                        }
                    }))
                    .size_full()
                    .font_family(self.content_font(cx))
                    .child(
                        Input::new(&self.editor)
                            .h_full()
                            .appearance(false)
                            .text_size(content_size)
                            .line_height(relative(1.5))
                            .disabled(self.is_busy()),
                    )
                    .into_any_element()
            } else {
                div()
                    .size_full()
                    .p_6()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .font_family(self.content_font(cx))
                    .key_context("ReadingContent")
                    .on_action(cx.listener(|this, _: &CloseReadingFind, window, cx| {
                        this.reading_search = None;
                        this.focus.focus(window);
                        cx.notify();
                    }))
                    .when(self.reading_search.is_some(), |view| {
                        view.child(self.render_reading_find(cx))
                    })
                    .child(
                        TextView::markdown(
                            ("reading", self.saves.identity as usize),
                            self.reading_text.clone(),
                            window,
                            cx,
                        )
                        .image_document(document.path.clone())
                        .source_highlights(
                            self.reading_search
                                .as_ref()
                                .map(|find| find.highlights.clone()),
                        )
                        .on_link_open({
                            let owner = cx.weak_entity();
                            let identity = self.saves.identity;
                            move |destination, source, window, cx| {
                                let _ = owner.update(cx, |this, cx| {
                                    if this.saves.identity == identity
                                        && this.editor.read(cx).value().as_ref() == source
                                    {
                                        this.open_link(destination, window, cx);
                                    }
                                });
                            }
                        })
                        .on_task_toggle({
                            let owner = cx.weak_entity();
                            let document_path = self.document.as_ref().map(|d| d.path.clone());
                            move |offset, source, window, cx| {
                                let _ = owner.update(cx, |this, cx| {
                                    if this.is_busy()
                                        || this.document.as_ref().map(|d| &d.path)
                                            != document_path.as_ref()
                                    {
                                        return;
                                    }
                                    this.editor.update(cx, |editor, cx| {
                                        editor.toggle_markdown_task(offset, source, window, cx);
                                    });
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
                    .when(self.notebooks.active.is_some(), |view| {
                        view.child(
                            Button::new("toggle-sidebar")
                                .ghost()
                                .small()
                                .w(rems(1.75))
                                .icon(if self.sidebar_collapsed {
                                    gpui_component::IconName::PanelLeftOpen
                                } else {
                                    gpui_component::IconName::PanelLeftClose
                                })
                                .tooltip(if self.sidebar_collapsed {
                                    "Reveal sidebar"
                                } else {
                                    "Collapse sidebar"
                                })
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.toggle_sidebar(window, cx);
                                })),
                        )
                    })
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
            .when_some(self.saves.error.clone(), |view, error| {
                view.child(div().p_2().text_sm().child(error))
            })
            .when_some(self.notebooks.notice.clone(), |view, notice| {
                view.child(div().p_2().text_sm().child(notice))
            })
            .child(
                div().flex().flex_1().min_h_0().overflow_hidden().child(
                    self.render_sidebar_layout(
                        div()
                            .id("image-drop-content")
                            .size_full()
                            .on_drop(cx.listener(|this, paths: &ExternalPaths, window, cx| {
                                cx.stop_propagation();
                                this.drop_images(paths.paths().to_vec(), window, cx);
                            }))
                            .on_drop(cx.listener(
                                |this, drag: &notebook_ui::EntryDrag, window, cx| {
                                    cx.stop_propagation();
                                    this.drop_images(vec![drag.root.join(&drag.path)], window, cx);
                                },
                            ))
                            .child(content)
                            .into_any_element(),
                        window,
                        cx,
                    ),
                ),
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
                                MenuKind::Notebooks => 6.75,
                                MenuKind::Help => 12.5,
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
        std::env::var_os("GPUI_NOTE_BENCH").or_else(|| std::env::var_os("GPUI_NOTE_NOTEBOOK_CHECK"))
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
        .with_assets(gpui_component_assets::Assets)
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
                    titlebar: Some(TitlebarOptions {
                        title: Some("GPUI Note".into()),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                |window, cx| {
                    let notes = cx.new(|cx| Notes::new(window, cx));
                    notes.update(cx, |this, cx| this.apply_appearance(window, cx));
                    let diagnostic_run = cfg!(feature = "editor-diagnostics")
                        && (std::env::var_os("GPUI_NOTE_BENCH").is_some()
                            || std::env::var_os("GPUI_NOTE_NOTEBOOK_CHECK").is_some()
                            || std::env::var_os("GPUI_NOTE_AUTOSAVE_CHECK").is_some());
                    if !diagnostic_run {
                        notes.update(cx, |this, cx| this.restore_notebooks(window, cx));
                    }
                    #[cfg(feature = "editor-diagnostics")]
                    if let Some(output) = std::env::var_os("GPUI_NOTE_BENCH") {
                        let notes = notes.clone();
                        window.on_next_frame(move |window, cx| {
                            benchmark::start(&notes, output.into(), window, cx)
                        });
                    }
                    #[cfg(feature = "editor-diagnostics")]
                    if let Some(output) = std::env::var_os("GPUI_NOTE_NOTEBOOK_CHECK") {
                        notebook_ui::diagnostics::start(notes.clone(), output.into(), window, cx);
                    }
                    #[cfg(feature = "editor-diagnostics")]
                    if let Some(output) = std::env::var_os("GPUI_NOTE_AUTOSAVE_CHECK") {
                        autosave::diagnostics::start(notes.clone(), output.into(), window, cx);
                    }
                    let view = cx.new(|cx| NotesWindow {
                        _subscription: cx.observe(&notes, |_, _, cx| cx.notify()),
                        notes,
                    });
                    cx.new(|cx| Root::new(view, window, cx))
                },
            )
            .expect("Could not create the GPUI Note window");
            cx.activate(true);
        });
}

impl Notes {
    fn document(&self) -> Option<&Document> {
        self.document.as_ref()
    }
    fn capabilities(&self) -> gpui_note::capabilities::Capabilities {
        gpui_note::capabilities::Capabilities {
            has_document: self.document().is_some(),
            mode: self.mode,
            busy: self.is_busy(),
            has_notebook: self.notebooks.active.is_some(),
        }
    }
    fn can_edit_document(&self) -> bool {
        self.capabilities().can_edit_document()
    }
}
