use super::*;
use gpui_component::{Disableable, dialog::Dialog, menu::PopupMenuItem};
use std::collections::BTreeMap;

#[cfg(feature = "editor-diagnostics")]
pub(super) mod diagnostics;

#[cfg(test)]
mod operation_tests;

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
    filename: Option<String>,
    retry: bool,
    draggable: bool,
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
    pub(super) tree_focus: FocusHandle,
    persistence: Option<Task<()>>,
    tree_loading: bool,
    tree_target: Option<PathBuf>,
    folder_target: Option<PathBuf>,
    pub notice: Option<String>,
    creating: bool,
    name: Entity<InputState>,
    location: Option<PathBuf>,
    modal_error: Option<String>,
    _name_subscription: Subscription,
}

impl NotebookState {
    pub(super) fn registered_paths(&self) -> &[PathBuf] {
        &self.index.notebooks
    }

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
            tree_loading: false,
            tree_target: None,
            folder_target: None,
            notice: None,
            creating: false,
            name,
            location: None,
            modal_error: None,
            _name_subscription: subscription,
        }
    }

    pub(super) fn rebuild_rows(&mut self) {
        let mut rows = Vec::new();
        if let Some(book) = &self.active {
            Self::append_rows(&self.directories, Path::new(""), 0, &book.settings, &mut rows);
        }
        replace_rows(&mut self.rows, &mut self.cursor, rows);
    }

    fn append_rows(
        directories: &BTreeMap<PathBuf, DirectoryState>,
        path: &Path,
        depth: usize,
        settings: &notebook::Settings,
        rows: &mut Vec<Row>,
    ) {
        match directories.get(path) {
            Some(DirectoryState::Ready(entries)) => {
                if entries.is_empty() {
                    rows.push(Row {
                        path: path.to_path_buf(),
                        depth,
                        directory: false,
                        disabled: true,
                        label: "Empty folder".into(),
                        filename: None,
                        retry: false,
                        draggable: false,
                    });
                }
                for entry in entries {
                    rows.push(Row {
                        path: entry.relative.clone(),
                        depth,
                        directory: entry.directory,
                        disabled: entry.link
                            || (!entry.directory && !(notebook::markdown(&entry.relative) || images::supported(&entry.relative))),
                        filename: (!entry.directory).then(|| notebook::name(&entry.relative)),
                        label: format!(
                            "{}{}",
                            if entry.directory {
                                notebook::name(&entry.relative)
                            } else {
                                notebook::sidebar_file_name(&entry.relative, settings.show_file_extensions, &settings.always_hide_extensions)
                            },
                            if entry.link { " (link)" } else { "" }
                        ),
                        retry: false,
                        draggable: !entry.link,
                    });
                    if entry.directory && !entry.link && settings.expanded.contains(&entry.relative)
                    {
                        Self::append_rows(directories, &entry.relative, depth + 1, settings, rows);
                    }
                }
            }
            Some(DirectoryState::Failed(error)) => rows.push(Row {
                path: path.to_path_buf(),
                depth,
                directory: false,
                disabled: false,
                label: format!("Retry: {error}"),
                filename: None,
                retry: true,
                draggable: false,
            }),
            _ => rows.push(Row {
                path: path.to_path_buf(),
                depth,
                directory: false,
                disabled: true,
                label: "Loading…".into(),
                filename: None,
                retry: false,
                draggable: false,
            }),
        }
    }
}

impl Notes {
    pub(super) fn focus_folder(
        &mut self,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(book) = &mut self.notebooks.active else {
            return;
        };
        for ancestor in path.ancestors().filter(|p| !p.as_os_str().is_empty()) {
            book.settings.expanded.insert(ancestor.to_path_buf());
        }
        self.notebooks.folder_target = Some(path);
        self.refresh_tree(window, cx);
        self.notebooks.tree_focus.focus(window);
        self.persist_notebooks(cx);
    }

    pub(super) fn restore_notebooks(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(operation) = self.foreground.begin() else { return; };
        let configured_path = self.notebooks.index_path.clone();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx.background_executor().spawn(async move {
                let path = match configured_path { Some(path) => path, None => notebook::index_path()? };
                notebook::load_index(&path).map(|index| (path, index))
            }).await;
            let _ = this.update_in(cx, |this, window, cx| {
                if !this.foreground.finish(operation) { return; }
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
            self.is_busy() || !self.notebooks.index_writable,
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
                    .disabled(self.is_busy())
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
        if self.is_busy() || !self.notebooks.index_writable || window.has_active_dialog(cx) {
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
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Dialog {
        let owner = cx.weak_entity();
        let dialog = dialog
            .title(if self.notebooks.creating {
                "Create new notebook"
            } else {
                "Manage notebooks"
            })
            .w(px(460.).min(window.viewport_size().width - px(32.)))
            .max_h((window.viewport_size().height - px(100.)).max(px(120.)))
            .overlay_closable(false)
            .close_button(!self.is_busy())
            .keyboard(!self.is_busy())
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
                .child(Input::new(&self.notebooks.name).disabled(self.is_busy()))
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
                                .disabled(self.is_busy())
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
                                .disabled(self.is_busy())
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
                                .disabled(self.is_busy() || self.notebooks.location.is_none())
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
                let remove_path = path.clone();
                let path_label = notebook::display_path(&path);
                list = list.child(
                    div()
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .gap_1()
                                .child(
                                    Button::new(("managed-notebook", index))
                                        .label(label)
                                        .disabled(self.is_busy())
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
                                )
                                .child(
                                    div()
                                        .text_sm()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(path_label),
                                ),
                        )
                        .child(
                            Button::new(("remove-notebook", index))
                                .label("Remove")
                                .disabled(self.is_busy())
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    if this
                                        .notebooks
                                        .active
                                        .as_ref()
                                        .is_some_and(|book| book.root == remove_path)
                                    {
                                        this.request(
                                            Next::RemoveNotebook(remove_path.clone()),
                                            window,
                                            cx,
                                        );
                                    } else {
                                        this.remove_notebook(remove_path.clone(), window, cx);
                                    }
                                })),
                        ),
                );
            }
            body = body
                .child(list)
                .child(
                    Button::new("new-notebook")
                        .label("Create new notebook")
                        .disabled(self.is_busy())
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
                        .disabled(self.is_busy())
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

    pub(super) fn remove_notebook(
        &mut self,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.is_busy() || !self.notebooks.index_writable {
            return;
        }
        if self
            .notebooks
            .active
            .as_ref()
            .is_some_and(|book| book.root == path)
        {
            self.notebooks.active = None;
            self.search = None;
            self.apply_appearance(window, cx);
            self.notebooks.generation += 1;
            self.notebooks.directories.clear();
            self.notebooks.rows.clear();
            self.notebooks.cursor = None;
            self.install_document(None, window, cx);
        }
        self.notebooks.index.forget(&path);
        self.persist_notebooks(cx);
        self.refresh(window, cx);
    }

    fn create_from_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_busy() {
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
        if self.is_busy() {
            return;
        }
        let Some(operation) = self.foreground.begin() else { return; };
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
                this.finish_notebook_picker(operation, result, location, window, cx);
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn finish_notebook_picker(
        &mut self,
        operation: OperationId,
        result: Result<Option<PathBuf>, String>,
        location: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.foreground.finish(operation) { return; }
        match result {
            Ok(Some(path)) if !location => {
                self.request(Next::Notebook { path, create: false }, window, cx);
            }
            result => {
                match result {
                    Ok(Some(path)) => self.notebooks.location = Some(path),
                    Err(error) => self.notebooks.modal_error = Some(error),
                    Ok(None) => {}
                }
                self.resume_document(window, cx);
            }
        }
    }

    pub(super) fn activate_notebook(
        &mut self,
        path: PathBuf,
        create: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(operation) = self.foreground.begin() else { return; };
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
                if !this.foreground.finish(operation) { return; }
                match result {
                    Ok((book, document)) => {
                        this.search = None;
                        this.reset_sidebar_resize(cx);
                        this.notebooks.index.register(book.root.clone());
                        this.mode = book.settings.view_mode;
                        this.notebooks.active = Some(book);
                        this.apply_appearance(window, cx);
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
                        this.resume_document(window, cx);
                    }
                }
                this.refresh(window, cx);
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn create_note(
        &mut self,
        root: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.is_busy()
            || !self
                .notebooks
                .active
                .as_ref()
                .is_some_and(|book| book.root == root)
        {
            return;
        }
        let Some(operation) = self.foreground.begin() else { return; };
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { notebook::create_untitled(&root).and_then(Document::open) })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                if !this.foreground.finish(operation) { return; }
                match result {
                    Ok(document) => {
                        if this.mode == ViewMode::Reading {
                            this.set_mode(ViewMode::Source, window, cx);
                        }
                        this.search_destination = None;
                        this.install_document(Some(document), window, cx);
                        this.remember_file(cx);
                        this.reveal_sidebar(cx);
                        this.refresh_tree(window, cx);
                    }
                    Err(error) => this.error("Could not create note", error, window, cx),
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
        let discarded_path = if self.search.is_some() && self.dirty(cx) {
            self.document.as_ref().map(|document| document.path.clone())
        } else {
            None
        };
        self.image_path = None;
        self.reading_search = None;
        self.saves.timer = None;
        self.saves.identity += 1;
        self.saves.error = None;
        // A fresh editor prevents undo from restoring another document.
        let text = document
            .as_ref()
            .map(|d| d.saved.clone())
            .unwrap_or_default();
        let editor = cx.new(|cx| {
            InputState::new(window, cx)
                .multi_line(true)
                .searchable(true)
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
            editor.set_image_document(self.document.as_ref().map(|d| d.path.clone()), cx);
            editor.set_live_preview(self.mode == ViewMode::LivePreview, cx)
        });
        if self.mode != ViewMode::Reading && self.document.is_some() {
            self.editor
                .update(cx, |editor, cx| editor.focus(window, cx));
        } else {
            self.focus.focus(window);
        }
        if let Some(path) = discarded_path {
            self.invalidate_discarded_search_overlay(&path, window, cx);
        }
    }

    pub(super) fn open_path(
        &mut self,
        path: PathBuf,
        keep_tree_focus: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if images::supported(&path) {
            self.open_image(path, keep_tree_focus, window, cx);
            return;
        }
        let Some(operation) = self.foreground.begin() else { return; };
        self.notebooks.tree_loading = keep_tree_focus;
        self.notebooks.tree_target = keep_tree_focus.then(|| path.clone());
        let loading_path = path.clone();
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
                if !this.foreground.finish(operation) { return; }
                this.notebooks.tree_loading = false;
                if keep_tree_focus && this.notebooks.tree_target.as_ref() != Some(&loading_path) {
                    if let Some(target) = this.notebooks.tree_target.take() {
                        if !this.document.as_ref().is_some_and(|doc| doc.path == target) {
                            this.open_path(target, true, window, cx);
                        }
                    }
                    cx.notify();
                    return;
                }
                this.notebooks.tree_target = None;
                match result {
                    Ok(document) => {
                        this.install_document(Some(document), window, cx);
                        this.remember_file(cx);
                        if keep_tree_focus {
                            this.notebooks.tree_focus.focus(window);
                        }
                        if let Some(destination) = this.search_destination.take() {
                            this.reveal_search_result(destination, window, cx);
                            this.finish_search_navigation(true, window, cx);
                        }
                    }
                    Err(error) => {
                        this.search_destination = None;
                        this.finish_search_navigation(false, window, cx);
                        if keep_tree_focus {
                            this.restore_tree_cursor(window, cx);
                        }
                        this.error("Could not open file", error, window, cx);
                    }
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

    pub(super) fn persist_notebooks(&mut self, cx: &mut Context<Self>) {
        if let Some(book) = &mut self.notebooks.active {
            book.settings.appearance = book.settings.appearance.clone().resolved(self.startup_dark);
        }
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
        let Some(operation) = self.foreground.begin() else { return; };
        let pending = self.notebooks.persistence.take();
        cx.spawn_in(window, async move |this, cx| {
            if let Some(pending) = pending {
                pending.await;
            }
            let _ = this.update_in(cx, |this, window, _| {
                if this.foreground.finish(operation) {
                    window.remove_window();
                }
            });
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
                        Ok(mut entries) => {
                            if let Some(book) = &this.notebooks.active {
                                book.settings.sort_entries(&mut entries);
                            }
                            DirectoryState::Ready(entries)
                        }
                        Err(error) => DirectoryState::Failed(error.to_string()),
                    },
                );
                for path in children {
                    if !this.notebooks.directories.contains_key(&path) {
                        this.load_directory(path, window, cx);
                    }
                }
                this.notebooks.rebuild_rows();
                if let Some(target) = &this.notebooks.folder_target {
                    if let Some(index) = this
                        .notebooks
                        .rows
                        .iter()
                        .position(|row| row.directory && &row.path == target)
                    {
                        this.notebooks.cursor = Some(index);
                        this.notebooks
                            .scroll
                            .scroll_to_item(index, ScrollStrategy::Top);
                        this.notebooks.folder_target = None;
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn activate_row(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_busy() {
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
                self.request(Next::TreeFile(path), window, cx);
            }
        }
        cx.notify();
    }

    fn tree_move(&mut self, down: bool, window: &mut Window, cx: &mut Context<Self>) {
        let len = self.notebooks.rows.len();
        if len == 0 || (self.is_busy() && !self.notebooks.tree_loading) {
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
        self.open_selected_row(window, cx);
        cx.notify();
    }

    pub(super) fn restore_tree_cursor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.notebooks.cursor = self
            .notebooks
            .active
            .as_ref()
            .zip(self.document.as_ref().map(|d| d.path.as_path()).or(self.image_path.as_deref()))
            .and_then(|(book, path)| {
                self.notebooks
                    .rows
                    .iter()
                    .position(|row| !row.directory && book.root.join(&row.path) == path)
            });
        if let Some(index) = self.notebooks.cursor {
            self.notebooks
                .scroll
                .scroll_to_item(index, ScrollStrategy::Top);
        }
        self.notebooks.tree_focus.focus(window);
        cx.notify();
    }

    fn open_selected_row(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let target = self
            .notebooks
            .cursor
            .and_then(|index| self.notebooks.rows.get(index))
            .filter(|row| !row.directory && !row.disabled && !row.retry)
            .and_then(|row| {
                self.notebooks
                    .active
                    .as_ref()
                    .map(|book| book.root.join(&row.path))
            });
        if self.notebooks.tree_loading {
            self.notebooks.tree_target = target;
        } else if let Some(path) = target {
            if !self.document.as_ref().is_some_and(|doc| doc.path == path) {
                self.request(Next::TreeFile(path), window, cx);
            }
        }
    }

    fn tree_horizontal(&mut self, right: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_busy() && !self.notebooks.tree_loading {
            return;
        }
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
            self.tree_move(true, window, cx);
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
            self.open_selected_row(window, cx);
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
            .on_drop(cx.listener(|this, drag: &EntryDrag, window, cx| {
                cx.stop_propagation();
                this.drop_entry(drag, PathBuf::new(), window, cx);
            }))
            .key_context("NotebookTree")
            .track_focus(&self.notebooks.tree_focus)
            .on_action(
                cx.listener(|this, _: &TreeUp, window, cx| this.tree_move(false, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &TreeDown, window, cx| this.tree_move(true, window, cx)),
            )
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
            .w_full()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .child(
                div()
                    .id("notebook-root-drop")
                    .drag_over::<EntryDrag>(|view, _, _, cx| view.bg(cx.theme().accent))
                    .flex()
                    .items_center()
                    .p_2()
                    .min_h(px(40.))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .text_sm()
                            .child(title),
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
                                    .zip(this.document.as_ref().map(|d| d.path.as_path()).or(this.image_path.as_deref()))
                                    .is_some_and(|(book, document_path)| {
                                        !row.directory
                                            && !row.disabled
                                            && !row.retry
                                            && book.root.join(&row.path) == document_path
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
                                let path = row.path.clone();
                                let directory = row.directory;
                                let drag = EntryDrag {
                                    root: this.notebooks.active.as_ref().unwrap().root.clone(),
                                    path: path.clone(),
                                };
                                div()
                                    .id(index)
                                    .h(px(28. * this.appearance().ui_font_size as f32 / 16.))
                                    .w_full()
                                    .pl(px(8. + row.depth as f32 * 16.))
                                    .pr_2()
                                    .text_sm()
                                    .flex()
                                    .items_center()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .border_1()
                                    .border_color(gpui::transparent_black())
                                    .when(selected, |view| view.bg(cx.theme().accent))
                                    .when(this.notebooks.cursor == Some(index), |view| {
                                        view.border_color(cx.theme().foreground.opacity(0.55))
                                    })
                                    .when(row.disabled, |view| {
                                        view.text_color(cx.theme().muted_foreground)
                                    })
                                    .relative()
                                    // Rows consume drops so only unoccupied space falls back to root.
                                    .when(!row.draggable || !directory || this.is_busy(), |view| {
                                        view.on_drop(|_: &EntryDrag, _, cx| cx.stop_propagation())
                                    })
                                    .when(row.draggable && !this.is_busy(), |view| {
                                        view.on_drag(drag, |drag, _, _, cx| {
                                            cx.stop_propagation();
                                            cx.new(|_| drag.clone())
                                        })
                                    })
                                    .when(row.draggable && directory && !this.is_busy(), |view| {
                                        view.drag_over::<EntryDrag>(|view, _, _, cx| {
                                            view.bg(cx.theme().accent)
                                        })
                                        .on_drop(
                                            cx.listener(
                                                move |this, drag: &EntryDrag, window, cx| {
                                                    cx.stop_propagation();
                                                    this.drop_entry(drag, path.clone(), window, cx);
                                                },
                                            ),
                                        )
                                    })
                                    .child(format!("{prefix}{}", row.label))
                                    .when_some(row.filename.clone(), |view, filename| {
                                        view.tooltip(move |window, cx| {
                                            gpui_component::tooltip::Tooltip::new(filename.clone()).build(window, cx)
                                        })
                                    })
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

#[derive(Clone)]
pub(super) struct EntryDrag {
    pub(super) root: PathBuf,
    pub(super) path: PathBuf,
}
impl Render for EntryDrag {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_3()
            .py_1()
            .bg(cx.theme().accent)
            .text_color(cx.theme().foreground)
            .child(notebook::name(&self.path))
    }
}

impl Notes {
    pub(super) fn drop_entry(
        &mut self,
        drag: &EntryDrag,
        parent: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.is_busy() || window.has_active_dialog(cx) {
            return;
        }
        if self.saves.writing {
            self.notebooks.notice =
                Some("A save is finishing. Please drop the entry again in a moment.".into());
            cx.notify();
            return;
        }
        let Some(book) = &self.notebooks.active else {
            return;
        };
        if book.root != drag.root || drag.path.parent() == Some(parent.as_path()) {
            return;
        }
        let root = book.root.clone();
        let from = drag.path.clone();
        let mut settings = book.settings.clone();
        self.saves.timer = None;
        let Some(operation) = self.foreground.begin() else { return; };
        cx.notify();
        cx.spawn_in(window, async move |this, cx| {
            let worker_root = root.clone();
            let worker_from = from.clone();
            let result = cx
                .background_executor()
                .spawn(async move {
                    let to = notebook::move_entry(&worker_root, &worker_from, &parent)?;
                    settings.remap_entry(&worker_from, &to);
                    Ok::<_, io::Error>((to, settings))
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                if !this.foreground.finish(operation) { return; }
                match result {
                    Ok((to, settings)) => {
                        if let Some(document) = &mut this.document {
                            document.path = notebook::remap_path(
                                &document.path,
                                &root.join(&from),
                                &root.join(&to),
                            );
                        }
                        if let Some(path) = &mut this.image_path {
                            *path = notebook::remap_path(path, &root.join(&from), &root.join(&to));
                        }
                        this.editor.update(cx, |editor, cx| {
                            editor.set_image_document(this.document.as_ref().map(|doc| doc.path.clone()), cx);
                        });
                        if let Some(book) = &mut this.notebooks.active {
                            book.settings = settings;
                        }
                        this.notebooks.notice = None;
                        this.persist_notebooks(cx);
                        for row in &mut this.notebooks.rows {
                            row.path = notebook::remap_path(&row.path, &from, &to);
                        }
                        this.refresh_sorted_tree(window, cx);
                        this.schedule_search(window, cx);
                    }
                    Err(error) => {
                        this.notebooks.notice = Some(format!("Could not move entry: {error}"))
                    }
                }
                this.schedule_autosave(window, cx);
                cx.notify();
            });
        })
        .detach();
    }
}

impl Notes {
    pub(super) fn set_file_sort(
        &mut self,
        sort: notebook::FileSort,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.is_busy() || window.has_active_dialog(cx) {
            return;
        }
        let Some(book) = &mut self.notebooks.active else {
            return;
        };
        if book.settings.file_sort == sort {
            return;
        }
        book.settings.file_sort = sort;
        for state in self.notebooks.directories.values_mut() {
            if let DirectoryState::Ready(entries) = state {
                sort.sort_entries(entries);
            }
        }
        self.notebooks.rebuild_rows();
        self.persist_notebooks(cx);
        cx.notify();
    }

    // Keep current rows visible until fresh metadata arrives, so keyboard selection
    // survives reordering. Generation checks discard reads from older saves/notebooks.
    pub(super) fn refresh_sorted_tree(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(book) = &self.notebooks.active else {
            return;
        };
        let root = book.root.clone();
        let mut paths = book.settings.expanded.clone();
        paths.insert(PathBuf::new());
        self.notebooks.generation += 1;
        let generation = self.notebooks.generation;
        cx.spawn_in(window, async move |this, cx| {
            let directories = cx
                .background_executor()
                .spawn(async move {
                    paths
                        .into_iter()
                        .map(|path| {
                            let result = notebook::read_directory(&root, &path);
                            (path, result)
                        })
                        .collect::<Vec<_>>()
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.notebooks.generation != generation {
                    return;
                }
                let Some(book) = &this.notebooks.active else {
                    return;
                };
                let mut refreshed: BTreeMap<_, _> = directories
                    .into_iter()
                    .map(|(path, result)| {
                        let state = match result {
                            Ok(mut entries) => {
                                book.settings.sort_entries(&mut entries);
                                DirectoryState::Ready(entries)
                            }
                            Err(error) => DirectoryState::Failed(error.to_string()),
                        };
                        (path, state)
                    })
                    .collect();
                // Preserve directories loaded after this refresh started, including
                // their selected rows, rather than replacing them with loading rows.
                for path in &book.settings.expanded {
                    if !refreshed.contains_key(path)
                        && let Some(state) = this.notebooks.directories.remove(path)
                    {
                        refreshed.insert(path.clone(), state);
                    }
                }
                this.notebooks.directories = refreshed;
                // A user may have expanded another folder while metadata was read.
                let missing: Vec<_> = book
                    .settings
                    .expanded
                    .iter()
                    .filter(|path| !this.notebooks.directories.contains_key(*path))
                    .cloned()
                    .collect();
                this.notebooks.rebuild_rows();
                for path in missing {
                    this.load_directory(path, window, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }
}

fn replace_rows(rows: &mut Vec<Row>, cursor: &mut Option<usize>, next: Vec<Row>) {
    let old_path = cursor.and_then(|i| rows.get(i)).map(|row| row.path.clone());
    *rows = next;
    *cursor = old_path.and_then(|path| rows.iter().position(|row| row.path == path));
}

#[cfg(test)]
mod extension_row_tests {
    use super::{DirectoryState, NotebookState, notebook, replace_rows};
    use std::{
        collections::BTreeMap,
        path::{Path, PathBuf},
    };

    #[test]
    fn extension_changes_preserve_row_identity_and_status() {
        let mut settings = notebook::Settings::default();
        let entries = [
            ("note.md", false, false),
            ("note.png", false, false),
            ("data.txt", false, false),
            ("folder.md", true, false),
            ("linked.md", false, true),
            ("empty", true, false),
            ("pending", true, false),
            ("failed", true, false),
        ];
        let mut directories = BTreeMap::new();
        directories.insert(
            PathBuf::new(),
            DirectoryState::Ready(
                entries
                    .iter()
                    .map(|(path, directory, link)| notebook::Entry {
                        relative: PathBuf::from(path),
                        directory: *directory,
                        link: *link,
                        modified: None,
                        created: None,
                    })
                    .collect(),
            ),
        );
        directories.insert("empty".into(), DirectoryState::Ready(vec![]));
        directories.insert("pending".into(), DirectoryState::Loading);
        directories.insert(
            "failed".into(),
            DirectoryState::Failed("unavailable".into()),
        );
        settings
            .expanded
            .extend([PathBuf::from("empty"), "pending".into(), "failed".into()]);
        let mut rows = Vec::new();
        NotebookState::append_rows(&directories, Path::new(""), 0, &settings, &mut rows);
        let before = rows.clone();
        let mut cursor = Some(1);
        settings.show_file_extensions = false;
        let mut next = Vec::new();
        NotebookState::append_rows(&directories, Path::new(""), 0, &settings, &mut next);
        replace_rows(&mut rows, &mut cursor, next);
        assert_eq!(cursor, Some(1));
        assert_eq!(rows[1].path, Path::new("note.png"));
        assert_eq!(rows[0].label, "note");
        assert_eq!(rows[1].label, "note");
        assert_eq!(rows[1].filename.as_deref(), Some("note.png"));
        assert_eq!(rows[2].label, "data");
        assert!(rows[2].disabled);
        assert_eq!(rows[3].label, "folder.md");
        assert_eq!(rows[4].label, "linked (link)");
        for (old, new) in before.iter().zip(&rows) {
            assert_eq!(old.path, new.path);
            assert_eq!(old.directory, new.directory);
            assert_eq!(old.draggable, new.draggable);
            assert_eq!(old.disabled, new.disabled);
            assert_eq!(old.retry, new.retry);
            if old.filename.is_none() {
                assert_eq!(old.label, new.label);
            }
        }
        assert!(rows.iter().any(|row| row.label == "Empty folder"));
        assert!(rows.iter().any(|row| row.label == "Loading…"));
        assert!(rows.iter().any(|row| row.label == "Retry: unavailable"));
    }
}
