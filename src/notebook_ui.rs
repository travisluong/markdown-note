use super::*;
use gpui_component::{Disableable, dialog::Dialog, menu::PopupMenuItem};
use std::collections::BTreeMap;

#[cfg(feature = "editor-diagnostics")]
pub(super) mod diagnostics;

#[derive(Clone)]
enum DirectoryState {
    Loading,
    Ready(Vec<notebook::Entry>),
    Failed(String),
}

#[derive(Clone)]
struct Row {
    path: PathBuf,
    depth: usize,
    directory: bool,
    disabled: bool,
    label: String,
    retry: bool,
}

pub(super) struct NotebookState {
    pub active: Option<notebook::Notebook>,
    index: notebook::Index,
    index_path: Option<PathBuf>,
    index_writable: bool,
    owner: WeakEntity<Notes>,
    directories: BTreeMap<PathBuf, DirectoryState>,
    generation: u64,
    rows: Vec<Row>,
    cursor: Option<usize>,
    scroll: UniformListScrollHandle,
    tree_focus: FocusHandle,
    persistence: Option<Task<()>>,
    pub notice: Option<String>,
    creating: bool,
    name: Entity<InputState>,
    location: Option<PathBuf>,
    modal_error: Option<String>,
    _name_subscription: Subscription,
}

impl NotebookState {
    pub fn new(window: &mut Window, cx: &mut Context<Notes>) -> Self {
        let name = cx.new(|cx| InputState::new(window, cx).placeholder("Notebook name"));
        let subscription = cx.subscribe(&name, |_, _, _: &InputEvent, cx| cx.notify());
        Self {
            active: None,
            index: notebook::Index::default(),
            index_path: None,
            index_writable: false,
            owner: cx.weak_entity(),
            directories: BTreeMap::new(),
            generation: 0,
            rows: Vec::new(),
            cursor: None,
            scroll: UniformListScrollHandle::new(),
            tree_focus: cx.focus_handle(),
            persistence: None,
            notice: None,
            creating: false,
            name,
            location: None,
            modal_error: None,
            _name_subscription: subscription,
        }
    }

    fn rebuild_rows(&mut self) {
        let mut rows = Vec::new();
        if let Some(book) = &self.active {
            self.append_rows(Path::new(""), 0, &book.settings, &mut rows);
        }
        let old_path = self
            .cursor
            .and_then(|i| self.rows.get(i))
            .map(|r| r.path.clone());
        self.rows = rows;
        self.cursor = old_path.and_then(|path| self.rows.iter().position(|r| r.path == path));
    }

    fn append_rows(
        &self,
        path: &Path,
        depth: usize,
        settings: &notebook::Settings,
        rows: &mut Vec<Row>,
    ) {
        match self.directories.get(path) {
            Some(DirectoryState::Ready(entries)) => {
                if entries.is_empty() {
                    rows.push(Row {
                        path: path.to_path_buf(),
                        depth,
                        directory: false,
                        disabled: true,
                        label: "Empty folder".into(),
                        retry: false,
                    });
                }
                for entry in entries {
                    rows.push(Row {
                        path: entry.relative.clone(),
                        depth,
                        directory: entry.directory,
                        disabled: entry.link
                            || (!entry.directory && !notebook::markdown(&entry.relative)),
                        label: format!(
                            "{}{}",
                            notebook::name(&entry.relative),
                            if entry.link { " (link)" } else { "" }
                        ),
                        retry: false,
                    });
                    if entry.directory && !entry.link && settings.expanded.contains(&entry.relative)
                    {
                        self.append_rows(&entry.relative, depth + 1, settings, rows);
                    }
                }
            }
            Some(DirectoryState::Failed(error)) => rows.push(Row {
                path: path.to_path_buf(),
                depth,
                directory: false,
                disabled: false,
                label: format!("Retry: {error}"),
                retry: true,
            }),
            _ => rows.push(Row {
                path: path.to_path_buf(),
                depth,
                directory: false,
                disabled: true,
                label: "Loading…".into(),
                retry: false,
            }),
        }
    }
}

impl Notes {
    pub(super) fn restore_notebooks(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.busy = true;
        let configured_path = self.notebooks.index_path.clone();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx.background_executor().spawn(async move {
                let path = match configured_path { Some(path) => path, None => notebook::index_path()? };
                notebook::load_index(&path).map(|index| (path, index))
            }).await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.busy = false;
                match result {
                    Ok((path, index)) => {
                        let active = index.active.clone();
                        this.notebooks.index = index;
                        this.notebooks.index_path = Some(path);
                        this.notebooks.index_writable = true;
                        if let Some(path) = active { this.activate_notebook(path, false, window, cx); }
                    }
                    Err(error) => this.notebooks.notice = Some(format!("Could not restore notebooks: {error}. Notebook history is disabled to preserve existing settings.")),
                }
                cx.notify();
            });
        }).detach();
    }

    pub(super) fn notebook_menu(&self, mut menu: PopupMenu, _: &mut App) -> PopupMenu {
        menu = menu.menu_with_disabled(
            "Manage notebooks",
            Box::new(ManageNotebooks),
            self.busy || !self.notebooks.index_writable,
        );
        if !self.notebooks.index.notebooks.is_empty() {
            menu = menu.separator();
        }
        for path in &self.notebooks.index.notebooks {
            let name = notebook::name(path);
            let duplicate = self
                .notebooks
                .index
                .notebooks
                .iter()
                .filter(|p| notebook::name(p) == name)
                .count()
                > 1;
            let label = if duplicate {
                format!("{} — {}", name, path.parent().unwrap_or(path).display())
            } else {
                name
            };
            let checked = self
                .notebooks
                .active
                .as_ref()
                .is_some_and(|book| &book.root == path);
            let path = path.clone();
            let owner = self.notebooks.owner.clone();
            menu = menu.item(
                PopupMenuItem::new(label)
                    .checked(checked)
                    .disabled(self.busy)
                    .on_click(move |_, window, cx| {
                        let _ = owner.update(cx, |this, cx| {
                            this.request(
                                Next::Notebook {
                                    path: path.clone(),
                                    create: false,
                                },
                                window,
                                cx,
                            )
                        });
                    }),
            );
        }
        menu.scrollable(true).max_h(px(400.))
    }

    pub(super) fn manage_notebooks(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || !self.notebooks.index_writable || window.has_active_dialog(cx) {
            return;
        }
        self.menu = None;
        self.notebooks.creating = false;
        self.notebooks.modal_error = None;
        let owner = cx.weak_entity();
        window.open_dialog(cx, move |dialog, window, cx| {
            owner
                .update(cx, |this, cx| this.notebook_dialog(dialog, window, cx))
                .unwrap_or_else(|_| Dialog::new(window, cx))
        });
        cx.notify();
    }

    fn notebook_dialog(
        &mut self,
        dialog: Dialog,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Dialog {
        let owner = cx.weak_entity();
        let dialog = dialog
            .title(if self.notebooks.creating {
                "Create new notebook"
            } else {
                "Manage notebooks"
            })
            .w(px(460.))
            .overlay_closable(false)
            .close_button(!self.busy)
            .keyboard(!self.busy)
            .on_ok(move |_, window, cx| {
                let _ = owner.update(cx, |this, cx| {
                    if this.notebooks.creating {
                        this.create_from_form(window, cx);
                    }
                });
                false
            });
        let mut body = div().flex().flex_col().gap_3();
        if self.notebooks.creating {
            let name = self.notebooks.name.read(cx).value().to_string();
            body = body
                .child("Notebook name")
                .child(Input::new(&self.notebooks.name).disabled(self.busy))
                .child("Location (parent folder)")
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div().flex_1().min_w_0().text_sm().child(
                                self.notebooks
                                    .location
                                    .as_ref()
                                    .map(|p| p.display().to_string())
                                    .unwrap_or_else(|| "Choose a location".into()),
                            ),
                        )
                        .child(
                            Button::new("browse-notebook-location")
                                .label("Browse")
                                .disabled(self.busy)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.pick_notebook_folder(true, window, cx)
                                })),
                        ),
                )
                .when_some(self.notebooks.location.clone(), |view, parent| {
                    view.child(
                        div()
                            .text_sm()
                            .child(format!("Folder: {}", parent.join(&name).display())),
                    )
                })
                .child(
                    div()
                        .flex()
                        .gap_2()
                        .child(
                            Button::new("notebook-back")
                                .label("Back")
                                .disabled(self.busy)
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.notebooks.creating = false;
                                    this.notebooks.modal_error = None;
                                    cx.notify();
                                })),
                        )
                        .child(
                            Button::new("notebook-create")
                                .primary()
                                .label("Create")
                                .disabled(self.busy || self.notebooks.location.is_none())
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.create_from_form(window, cx);
                                })),
                        ),
                );
        } else {
            let mut list = div()
                .id("managed-notebooks")
                .max_h(px(300.))
                .overflow_y_scroll()
                .flex()
                .flex_col()
                .gap_2();
            if self.notebooks.index.notebooks.is_empty() {
                list = list.child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("No notebooks yet."),
                );
            }
            for (index, path) in self.notebooks.index.notebooks.iter().enumerate() {
                let active = self
                    .notebooks
                    .active
                    .as_ref()
                    .is_some_and(|book| &book.root == path);
                let label = format!(
                    "{}{}",
                    notebook::name(path),
                    if active { " (active)" } else { "" }
                );
                let path = path.clone();
                list = list.child(
                    div().flex().items_center().gap_2().child(
                        Button::new(("managed-notebook", index))
                            .label(label)
                            .disabled(self.busy)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.request(
                                    Next::Notebook {
                                        path: path.clone(),
                                        create: false,
                                    },
                                    window,
                                    cx,
                                );
                            })),
                    ),
                );
            }
            body = body
                .child(list)
                .child(
                    Button::new("new-notebook")
                        .label("Create new notebook")
                        .disabled(self.busy)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.notebooks.creating = true;
                            this.notebooks.modal_error = None;
                            this.notebooks
                                .name
                                .update(cx, |name, cx| name.focus(window, cx));
                            cx.notify();
                        })),
                )
                .child(
                    Button::new("open-notebook")
                        .label("Open folder as notebook")
                        .disabled(self.busy)
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.pick_notebook_folder(false, window, cx)
                        })),
                );
        }
        dialog.child(
            body.when_some(self.notebooks.modal_error.clone(), |view, error| {
                view.child(div().text_sm().child(error))
            }),
        )
    }

    fn create_from_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let name = self.notebooks.name.read(cx).value().to_string();
        if let Err(error) = notebook::validate_name(&name) {
            self.notebooks.modal_error = Some(error.to_string());
            cx.notify();
            return;
        }
        if let Some(parent) = &self.notebooks.location {
            self.request(
                Next::Notebook {
                    path: parent.join(name),
                    create: true,
                },
                window,
                cx,
            );
        } else {
            self.notebooks.modal_error = Some("Choose a location with Browse.".into());
            cx.notify();
        }
    }

    fn pick_notebook_folder(
        &mut self,
        location: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.notebooks.modal_error = None;
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(
                if location {
                    "Choose location"
                } else {
                    "Open notebook"
                }
                .into(),
            ),
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = match picker.await {
                Ok(Ok(paths)) => Ok(paths.and_then(|paths| paths.into_iter().next())),
                Ok(Err(error)) => Err(error.to_string()),
                Err(error) => Err(error.to_string()),
            };
            let _ = this.update_in(cx, |this, window, cx| {
                this.busy = false;
                match result {
                    Ok(Some(path)) if location => this.notebooks.location = Some(path),
                    Ok(Some(path)) => this.request(
                        Next::Notebook {
                            path,
                            create: false,
                        },
                        window,
                        cx,
                    ),
                    Ok(None) => {}
                    Err(error) => this.notebooks.modal_error = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn activate_notebook(
        &mut self,
        path: PathBuf,
        create: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.busy = true;
        self.notebooks.modal_error = None;
        let pending = self.notebooks.persistence.take();
        cx.spawn_in(window, async move |this, cx| {
            if let Some(pending) = pending {
                pending.await;
            }
            let result = cx
                .background_executor()
                .spawn(async move {
                    let book = notebook::Notebook::open(&path, create)?;
                    let document = match &book.settings.last_file {
                        Some(relative) => {
                            match notebook::resolve(&book.root, relative).and_then(Document::open) {
                                Ok(document) => Some(document),
                                Err(error) if error.kind() == io::ErrorKind::NotFound => None,
                                Err(error) => return Err(error),
                            }
                        }
                        None => None,
                    };
                    Ok::<_, io::Error>((book, document))
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.busy = false;
                match result {
                    Ok((book, document)) => {
                        this.notebooks.index.register(book.root.clone());
                        this.notebooks.active = Some(book);
                        this.install_document(document, window, cx);
                        this.refresh_tree(window, cx);
                        this.remember_file(cx);
                        if window.has_active_dialog(cx) {
                            window.close_dialog(cx);
                        }
                    }
                    Err(error) => {
                        let message = format!("Could not open notebook: {error}");
                        if window.has_active_dialog(cx) {
                            this.notebooks.modal_error = Some(message);
                        } else {
                            this.notebooks.notice = Some(message);
                        }
                    }
                }
                this.refresh(window, cx);
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn install_document(
        &mut self,
        document: Option<Document>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // A fresh editor prevents undo from restoring another document.
        let text = document
            .as_ref()
            .map(|d| d.saved.clone())
            .unwrap_or_default();
        let editor = cx.new(|cx| {
            InputState::new(window, cx)
                .multi_line(true)
                .default_value(text)
        });
        self.editor_subscription = Self::subscribe_editor(&editor, window, cx);
        self.editor = editor;
        self.document = document;
        self.reading_text = if self.mode == ViewMode::Reading {
            self.editor.read(cx).value()
        } else {
            SharedString::default()
        };
        self.dirty_task = None;
        self.dirty_revision += 1;
        self.dirty_cached = false;
        self.editor.update(cx, |editor, cx| {
            editor.set_live_preview(self.mode == ViewMode::LivePreview, cx)
        });
        if self.mode != ViewMode::Reading && self.document.is_some() {
            self.editor
                .update(cx, |editor, cx| editor.focus(window, cx));
        } else {
            self.focus.focus(window);
        }
    }

    pub(super) fn open_path(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.busy = true;
        let root = self.notebooks.active.as_ref().map(|book| book.root.clone());
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    if let Some(root) = root {
                        if let Ok(relative) = path.strip_prefix(&root) {
                            return notebook::resolve(&root, relative).and_then(Document::open);
                        }
                    }
                    Document::open(path)
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.busy = false;
                match result {
                    Ok(document) => {
                        this.install_document(Some(document), window, cx);
                        this.remember_file(cx);
                    }
                    Err(error) => this.error("Could not open file", error, window, cx),
                }
                this.refresh(window, cx);
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn remember_file(&mut self, cx: &mut Context<Self>) {
        if let Some(book) = &mut self.notebooks.active {
            // File -> Open may select a standalone document outside this notebook.
            if let Some(relative) = self
                .document
                .as_ref()
                .and_then(|d| d.path.strip_prefix(&book.root).ok())
            {
                if notebook::safe_relative(relative) {
                    book.settings.last_file = Some(relative.to_path_buf());
                }
            } else if self.document.is_none() {
                book.settings.last_file = None;
            }
        }
        self.persist_notebooks(cx);
    }

    fn persist_notebooks(&mut self, cx: &mut Context<Self>) {
        if !self.notebooks.index_writable {
            return;
        }
        let Some(path) = self.notebooks.index_path.clone() else {
            return;
        };
        let book = self.notebooks.active.clone();
        let index = self.notebooks.index.clone();
        let previous = self.notebooks.persistence.take();
        self.notebooks.persistence = Some(cx.spawn(async move |this, cx| {
            if let Some(previous) = previous {
                previous.await;
            }
            let result = cx
                .background_executor()
                .spawn(async move { notebook::save_state(&path, &index, book.as_ref()) })
                .await;
            if let Err(error) = result {
                let _ = this.update(cx, |this, cx| {
                    this.notebooks.notice =
                        Some(format!("Could not save notebook settings: {error}"));
                    cx.notify();
                });
            }
        }));
    }

    pub(super) fn finish_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.busy = true;
        let pending = self.notebooks.persistence.take();
        cx.spawn_in(window, async move |_, cx| {
            if let Some(pending) = pending {
                pending.await;
            }
            let _ = cx.update(|window, _| window.remove_window());
        })
        .detach();
    }

    fn refresh_tree(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.notebooks.generation += 1;
        self.notebooks.directories.clear();
        self.notebooks.cursor = None;
        self.load_directory(PathBuf::new(), window, cx);
    }

    fn load_directory(&mut self, relative: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let Some(book) = &self.notebooks.active else {
            return;
        };
        let root = book.root.clone();
        let generation = self.notebooks.generation;
        self.notebooks
            .directories
            .insert(relative.clone(), DirectoryState::Loading);
        self.notebooks.rebuild_rows();
        cx.spawn_in(window, async move |this, cx| {
            let path = relative.clone();
            let result = cx
                .background_executor()
                .spawn(async move { notebook::read_directory(&root, &path) })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.notebooks.generation != generation {
                    return;
                }
                let mut children = Vec::new();
                if let (Ok(entries), Some(book)) = (&result, &this.notebooks.active) {
                    children = entries
                        .iter()
                        .filter(|e| {
                            e.directory && !e.link && book.settings.expanded.contains(&e.relative)
                        })
                        .map(|e| e.relative.clone())
                        .collect();
                }
                this.notebooks.directories.insert(
                    relative,
                    match result {
                        Ok(entries) => DirectoryState::Ready(entries),
                        Err(error) => DirectoryState::Failed(error.to_string()),
                    },
                );
                for path in children {
                    if !this.notebooks.directories.contains_key(&path) {
                        this.load_directory(path, window, cx);
                    }
                }
                this.notebooks.rebuild_rows();
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn activate_row(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let Some(row) = self.notebooks.rows.get(index).cloned() else {
            return;
        };
        self.notebooks.cursor = Some(index);
        if row.disabled {
            cx.notify();
            return;
        }
        if row.retry {
            self.load_directory(row.path, window, cx);
            return;
        }
        let Some(book) = &mut self.notebooks.active else {
            return;
        };
        if row.directory {
            if !book.settings.expanded.remove(&row.path) {
                book.settings.expanded.insert(row.path.clone());
            }
            if !self.notebooks.directories.contains_key(&row.path) {
                self.load_directory(row.path, window, cx);
            }
            self.notebooks.rebuild_rows();
            self.persist_notebooks(cx);
        } else {
            let path = book.root.join(row.path);
            if !self.document.as_ref().is_some_and(|d| d.path == path) {
                self.request(Next::File(path), window, cx);
            }
        }
        cx.notify();
    }

    fn tree_move(&mut self, down: bool, cx: &mut Context<Self>) {
        let len = self.notebooks.rows.len();
        if len == 0 || self.busy {
            return;
        }
        let next = match self.notebooks.cursor {
            Some(i) if down => (i + 1).min(len - 1),
            Some(i) => i.saturating_sub(1),
            None => 0,
        };
        self.notebooks.cursor = Some(next);
        self.notebooks.scroll.scroll_to_item(
            next,
            if down {
                ScrollStrategy::Bottom
            } else {
                ScrollStrategy::Top
            },
        );
        cx.notify();
    }

    fn tree_horizontal(&mut self, right: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.notebooks.cursor else {
            return;
        };
        let Some(row) = self.notebooks.rows.get(index) else {
            return;
        };
        let expanded = self
            .notebooks
            .active
            .as_ref()
            .is_some_and(|b| b.settings.expanded.contains(&row.path));
        if row.directory && expanded != right {
            self.activate_row(index, window, cx);
        } else if right && row.directory && expanded {
            self.tree_move(true, cx);
        } else if !right {
            let parent = row.path.parent().map(Path::to_path_buf);
            self.notebooks.cursor = self
                .notebooks
                .rows
                .iter()
                .position(|r| r.directory && Some(&r.path) == parent.as_ref());
            if let Some(i) = self.notebooks.cursor {
                self.notebooks.scroll.scroll_to_item(i, ScrollStrategy::Top);
            }
            cx.notify();
        }
    }

    pub(super) fn render_sidebar(
        &mut self,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let title = self
            .notebooks
            .active
            .as_ref()
            .map(|book| notebook::name(&book.root))
            .unwrap_or_default();
        div()
            .id("notebook-sidebar")
            .key_context("NotebookTree")
            .track_focus(&self.notebooks.tree_focus)
            .on_action(cx.listener(|this, _: &TreeUp, _, cx| this.tree_move(false, cx)))
            .on_action(cx.listener(|this, _: &TreeDown, _, cx| this.tree_move(true, cx)))
            .on_action(
                cx.listener(|this, _: &TreeLeft, window, cx| {
                    this.tree_horizontal(false, window, cx)
                }),
            )
            .on_action(
                cx.listener(|this, _: &TreeRight, window, cx| {
                    this.tree_horizontal(true, window, cx)
                }),
            )
            .on_action(cx.listener(|this, _: &TreeOpen, window, cx| {
                if let Some(i) = this.notebooks.cursor {
                    this.activate_row(i, window, cx);
                }
            }))
            .w(px(240.))
            .min_w(px(160.))
            .flex_shrink_0()
            .h_full()
            .flex()
            .flex_col()
            .border_r_1()
            .border_color(cx.theme().border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .p_2()
                    .gap_1()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_sm()
                            .child(title),
                    )
                    .child(
                        Button::new("refresh-notebook")
                            .ghost()
                            .small()
                            .label("Refresh")
                            .disabled(self.busy)
                            .on_click(
                                cx.listener(|this, _, window, cx| this.refresh_tree(window, cx)),
                            ),
                    ),
            )
            .child(
                uniform_list(
                    "notebook-files",
                    self.notebooks.rows.len(),
                    cx.processor(|this, range: std::ops::Range<usize>, _, cx| {
                        range
                            .map(|index| {
                                let row = &this.notebooks.rows[index];
                                let selected = this
                                    .notebooks
                                    .active
                                    .as_ref()
                                    .zip(this.document.as_ref())
                                    .is_some_and(|(book, document)| {
                                        !row.directory
                                            && !row.disabled
                                            && !row.retry
                                            && book.root.join(&row.path) == document.path
                                    });
                                let expanded =
                                    this.notebooks.active.as_ref().is_some_and(|book| {
                                        book.settings.expanded.contains(&row.path)
                                    });
                                let prefix = if row.directory {
                                    if expanded { "▾ " } else { "▸ " }
                                } else {
                                    "  "
                                };
                                div()
                                    .id(index)
                                    .h(px(28.))
                                    .w_full()
                                    .pl(px(8. + row.depth as f32 * 16.))
                                    .pr_2()
                                    .text_sm()
                                    .flex()
                                    .items_center()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .when(selected, |view| view.bg(cx.theme().accent))
                                    .when(this.notebooks.cursor == Some(index), |view| {
                                        view.border_1().border_color(cx.theme().border)
                                    })
                                    .when(row.disabled, |view| {
                                        view.text_color(cx.theme().muted_foreground)
                                    })
                                    .child(format!("{prefix}{}", row.label))
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.notebooks.tree_focus.focus(window);
                                        this.activate_row(index, window, cx);
                                    }))
                            })
                            .collect::<Vec<_>>()
                    }),
                )
                .track_scroll(self.notebooks.scroll.clone())
                .flex_1()
                .min_h_0(),
            )
    }
}
