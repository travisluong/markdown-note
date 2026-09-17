use super::*;
use gpui_component::{Disableable, dialog::Dialog, menu::PopupMenuItem};
use mithril::explorer::Explorer;
#[cfg(test)]
use std::time::Duration;

/// Only document loading admits the notebook's exact settings file. Tree and
/// persisted relative paths continue to reject all metadata paths.
fn resolve_document_path(root: &Path, path: &Path) -> io::Result<PathBuf> {
    if path == notebook::settings_path(root) {
        for (component, directory) in [
            (root.to_path_buf(), true),
            (root.join(".mithril"), true),
            (path.to_path_buf(), false),
        ] {
            let metadata = std::fs::symlink_metadata(&component)?;
            if notebook::is_link(&metadata)
                || (directory && !metadata.is_dir())
                || (!directory && !metadata.is_file())
            {
                return Err(io::Error::other(
                    "Notebook settings must be a regular file in an unlinked notebook folder.",
                ));
            }
        }
        return Ok(path.to_path_buf());
    }
    notebook::resolve(
        root,
        path.strip_prefix(root)
            .map_err(|_| io::Error::other("File is outside the notebook"))?,
    )
}

impl Notes {
    pub(crate) fn new_note(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.capabilities().can_create_note() || window.has_active_dialog(cx) {
            return;
        }
        if let Some(book) = &self.notebooks.active {
            self.request(Next::NewNote(book.root.clone()), window, cx);
        }
    }
}

mod activation_preferences;
mod entry_menu;
#[cfg(windows)]
mod reveal;
use activation_preferences::ActivationPreferences;
pub(super) use activation_preferences::PreferenceIntent;
pub(super) use entry_menu::{EntryAction, EntryTarget};

#[cfg(feature = "editor-diagnostics")]
pub(super) mod diagnostics;

#[cfg(test)]
mod operation_tests;

#[cfg(test)]
mod explorer_tests;

#[cfg(test)]
mod session_tests;
#[cfg(test)]
mod tabs_tests;

#[cfg(test)]
mod autosave_tests;

#[cfg(test)]
mod close_tests;

#[cfg(test)]
mod persistence_tests;

#[cfg(test)]
mod activation_preference_tests;

enum SettingsAfterFlush {
    Close,
    Activate { path: PathBuf, create: bool },
}

struct SettingsWaiter {
    operation: OperationId,
    barrier: notebook::FlushBarrier,
    after: SettingsAfterFlush,
}

#[cfg(test)]
type RevealBackend = dyn FnMut(&Path) -> io::Result<()>;

pub(super) struct NotebookState {
    pub active: Option<notebook::Notebook>,
    pub(super) exclusions: notebook::Exclusions,
    exclusion_error: Option<String>,
    pub(super) context_menu: Option<(gpui::Point<Pixels>, Entity<PopupMenu>)>,
    context_subscription: Option<Subscription>,
    select_after_refresh: Option<PathBuf>,
    #[cfg(test)]
    trash_backend: fn(&Path, &Path) -> io::Result<()>,
    #[cfg(test)]
    reveal_backend: Box<RevealBackend>,
    index: notebook::Index,
    index_path: Option<PathBuf>,
    index_writable: bool,
    owner: WeakEntity<Notes>,
    explorer: Explorer,
    scroll: UniformListScrollHandle,
    pub(super) tree_focus: FocusHandle,
    persistence: Option<Task<()>>,
    writer: Option<notebook::SettingsWriter>,
    settings_waiter: Option<SettingsWaiter>,
    activation_preferences: Option<ActivationPreferences>,
    window: AnyWindowHandle,
    #[cfg(test)]
    write_delay: Duration,
    #[cfg(test)]
    activation_install_delay: Duration,
    tree_operation: Option<OperationId>,
    folder_target: Option<PathBuf>,
    pub notice: Option<String>,
    creating: bool,
    name: Entity<InputState>,
    location: Option<PathBuf>,
    modal_error: Option<String>,
    _name_subscription: Subscription,
}

impl NotebookState {
    pub(super) fn settings_file_flush_status(&self, root: &Path) -> notebook::FlushStatus {
        self.writer
            .as_ref()
            .map_or(notebook::FlushStatus::Succeeded, |writer| {
                writer.flush_status(&writer.flush_book(root))
            })
    }

    pub(super) fn index_writable(&self) -> bool {
        self.index_writable
    }

    #[cfg(test)]
    pub(crate) fn set_test_persistence_target(&mut self, path: PathBuf) {
        self.index_path = Some(path);
        self.index_writable = true;
    }

    #[cfg(test)]
    pub(crate) fn set_test_persistence_delay(&mut self, delay: Duration) {
        self.write_delay = delay;
    }

    pub(super) fn reload_exclusions(&mut self) {
        if self.notice == self.exclusion_error {
            self.notice = None;
        }
        let patterns = self
            .active
            .as_ref()
            .map_or(&[][..], |book| book.settings.excluded_patterns.as_slice());
        match notebook::Exclusions::compile(patterns) {
            Ok(exclusions) => {
                self.exclusions = exclusions;
                self.exclusion_error = None;
            }
            Err(error) => {
                self.exclusions = notebook::Exclusions::default();
                let message =
                    format!("Custom exclusions are disabled. Fix them in Settings. {error}");
                self.exclusion_error = Some(message.clone());
                self.notice = Some(message);
            }
        }
    }

    pub(super) fn registered_paths(&self) -> &[PathBuf] {
        &self.index.notebooks
    }

    pub fn new(window: &mut Window, cx: &mut Context<Notes>) -> Self {
        let name = cx.new(|cx| InputState::new(window, cx).placeholder("Notebook name"));
        let subscription = cx.subscribe(&name, |_, _, _: &InputEvent, cx| cx.notify());
        Self {
            active: None,
            exclusions: notebook::Exclusions::default(),
            exclusion_error: None,
            context_menu: None,
            context_subscription: None,
            select_after_refresh: None,
            #[cfg(test)]
            trash_backend: |_, _| Err(io::Error::other("OS Trash is disabled in tests")),
            #[cfg(test)]
            reveal_backend: Box::new(|_| Err(io::Error::other("OS reveal is disabled in tests"))),
            index: notebook::Index::default(),
            index_path: None,
            index_writable: false,
            owner: cx.weak_entity(),
            explorer: Explorer::new(images::supported),
            scroll: UniformListScrollHandle::new(),
            tree_focus: cx.focus_handle(),
            persistence: None,
            writer: None,
            settings_waiter: None,
            activation_preferences: None,
            window: window.window_handle(),
            #[cfg(test)]
            write_delay: Duration::ZERO,
            #[cfg(test)]
            activation_install_delay: Duration::ZERO,
            tree_operation: None,
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
        self.explorer
            .rebuild(self.active.as_ref().map(|book| &book.settings));
    }
}

impl Notes {
    pub(super) fn update_excluded_patterns(
        &mut self,
        patterns: Vec<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.is_busy() || notebook::Exclusions::compile(&patterns).is_err() {
            return;
        }
        let Some(book) = &mut self.notebooks.active else {
            return;
        };
        if book.settings.excluded_patterns == patterns {
            return;
        }
        book.settings.excluded_patterns = patterns;
        self.notebooks.reload_exclusions();
        self.persist_notebooks(cx);
        self.refresh_tree(window, cx);
        self.schedule_search(window, cx);
        cx.notify();
    }

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
        let effects = self.tabs.active.session.restore();
        for effect in effects {
            if let app::session::Effect::RestoreIndex(operation) = effect {
                let configured_path = self.notebooks.index_path.clone();
                let tab_id = self.tabs.active.id;
                cx.spawn_in(window, async move |this, cx| {
                    let result = cx.background_executor().spawn(async move {
                        let path = match configured_path { Some(path) => path, None => notebook::index_path()? };
                        notebook::load_index(&path).map(|index| (path, index))
                    }).await;
                    let _ = this.update_in(cx, |this, window, cx| {
                        if this.tabs.active.id != tab_id { return; }
                        match result {
                            Ok((path, index)) => {
                                let next = index.active.clone().map(|path| Next::Notebook { path, create: false });
                                let effects = this.tabs.active.session.restored(operation, Ok(next));
                                if effects.is_empty() { return; }
                                this.notebooks.index = index;
                                this.notebooks.index_path = Some(path);
                                this.notebooks.index_writable = true;
                                this.apply_session_effects(effects, window, cx);
                            }
                            Err(error) => {
                                let effects = this.tabs.active.session.restored(operation, Err(()));
                                for effect in effects {
                                    if matches!(effect, app::session::Effect::RestoreFailed(_)) {
                                        this.notebooks.notice = Some(format!("Could not restore notebooks: {error}. Notebook history is disabled to preserve existing settings."));
                                    } else {
                                        this.apply_session_effects(vec![effect], window, cx);
                                    }
                                }
                            }
                        }
                    });
                }).detach();
            } else {
                self.apply_session_effects(vec![effect], window, cx);
            }
        }
    }
    pub(super) fn notebook_menu(&self, mut menu: PopupMenu, _: &mut App) -> PopupMenu {
        menu = menu.menu_with_disabled(
            "Manage notebooks",
            Box::new(ManageNotebooks),
            self.document_interaction_blocked() || !self.notebooks.index_writable,
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
                    .disabled(self.document_interaction_blocked())
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
        if self.document_interaction_blocked()
            || !self.notebooks.index_writable
            || window.has_active_dialog(cx)
        {
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
        if self.document_interaction_blocked() || !self.notebooks.index_writable {
            return;
        }
        if self
            .notebooks
            .active
            .as_ref()
            .is_some_and(|book| book.root == path)
        {
            self.request(Next::RemoveNotebook(path), window, cx);
        } else {
            // Removing an inactive index entry does not navigate or save the
            // retained document. Its existing autosave remains independent.
            self.notebooks.index.forget(&path);
            self.persist_notebooks(cx);
            self.refresh(window, cx);
        }
    }

    pub(crate) fn remove_notebook_with_operation(
        &mut self,
        operation: OperationId,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(book) = &self.notebooks.active {
            self.notebook_cache.insert(book.root.clone(), book.clone());
        }
        if !self.notebooks.index_writable {
            let observed = self.observed_document(cx);
            let effects = self.tabs.active.session.continued(operation, observed);
            self.apply_session_effects(effects, window, cx);
            return;
        }
        if self
            .notebooks
            .active
            .as_ref()
            .is_some_and(|book| book.root == path)
        {
            let effects = self
                .tabs
                .active
                .session
                .dispatch(operation, app::session::Stage::Load);
            if effects.is_empty() {
                return;
            }
            for effect in effects {
                if let app::session::Effect::Load(operation) = effect {
                    self.notebooks.active = None;
                    self.notebooks.reload_exclusions();
                    self.search = None;
                    self.search_subscription = None;
                    self.apply_appearance(window, cx);
                    self.notebooks.explorer.clear();
                    self.complete_content_load(
                        operation,
                        Ok(app::session::Loaded::Empty),
                        window,
                        cx,
                    );
                } else {
                    self.apply_session_effects(vec![effect], window, cx);
                }
            }
        } else {
            let observed = self.observed_document(cx);
            let effects = self.tabs.active.session.continued(operation, observed);
            if effects.is_empty() {
                return;
            }
            self.apply_session_effects(effects, window, cx);
        }
        self.notebooks.index.forget(&path);
        self.persist_notebooks(cx);
        self.refresh(window, cx);
    }
    fn create_from_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.document_interaction_blocked() {
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
        let effects = self.tabs.active.session.pick();
        for effect in effects {
            if let app::session::Effect::Pick(operation) = effect {
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
                let tab_id = self.tabs.active.id;
                cx.spawn_in(window, async move |this, cx| {
                    let result = match picker.await {
                        Ok(Ok(paths)) => Ok(paths.and_then(|paths| paths.into_iter().next())),
                        Ok(Err(error)) => Err(error.to_string()),
                        Err(error) => Err(error.to_string()),
                    };
                    let _ = this.update_in(cx, |this, window, cx| {
                        if this.tabs.active.id != tab_id {
                            return;
                        }
                        this.finish_notebook_picker(operation, result, location, window, cx);
                    });
                })
                .detach();
            } else {
                self.apply_session_effects(vec![effect], window, cx);
            }
        }
    }

    pub(super) fn finish_notebook_picker(
        &mut self,
        operation: OperationId,
        result: Result<Option<PathBuf>, String>,
        location: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let observed = self.observed_document(cx);
        match result {
            Ok(Some(path)) if !location => {
                let effects = self.tabs.active.session.picked(
                    operation,
                    Some(Next::Notebook {
                        path,
                        create: false,
                    }),
                    observed,
                );
                self.apply_session_effects(effects, window, cx);
            }
            result => {
                let effects = self.tabs.active.session.picked(operation, None, observed);
                if effects.is_empty() {
                    return;
                }
                match result {
                    Ok(Some(path)) => self.notebooks.location = Some(path),
                    Err(error) => self.notebooks.modal_error = Some(error),
                    Ok(None) => {}
                }
                self.apply_session_effects(effects, window, cx);
            }
        }
    }
    pub(crate) fn activate_notebook_with_operation(
        &mut self,
        operation: OperationId,
        path: PathBuf,
        create: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let effects = self
            .tabs
            .active
            .session
            .dispatch(operation, app::session::Stage::ActivateNotebook);
        for effect in effects {
            if let app::session::Effect::ResolveActivation(operation) = effect {
                self.notebooks.activation_preferences = self
                    .notebooks
                    .active
                    .as_ref()
                    .map(|book| ActivationPreferences::new(operation, book.root.clone()));
                self.notebooks.modal_error = None;
                let path = path.clone();
                let tab_id = self.tabs.active.id;
                cx.spawn_in(window, async move |this, cx| {
                    // Resolve identity before reading settings, so pending target writes
                    // reach disk before Notebook::open reloads that same destination.
                    let result = cx
                        .background_executor()
                        .spawn(async move {
                            if create {
                                Ok(path)
                            } else {
                                fs::canonicalize(path)
                            }
                        })
                        .await;
                    let _ = this.update_in(cx, |this, window, cx| {
                        if this.tabs.active.id != tab_id {
                            return;
                        }
                        let observed = this.observed_document(cx);
                        let effects = this.tabs.active.session.activation_resolved(
                            operation,
                            result.is_ok(),
                            observed,
                        );
                        for effect in effects {
                            match effect {
                                app::session::Effect::FlushActivation { operation, .. } => {
                                    let path = result
                                        .as_ref()
                                        .expect("accepted successful resolution")
                                        .clone();
                                    let barrier = this
                                        .notebooks
                                        .writer
                                        .as_ref()
                                        .map(|writer| writer.flush_book(&path))
                                        .unwrap_or_default();
                                    this.notebooks.settings_waiter = Some(SettingsWaiter {
                                        operation,
                                        barrier,
                                        after: SettingsAfterFlush::Activate { path, create },
                                    });
                                    this.resolve_settings_waiter(window, cx);
                                }
                                app::session::Effect::ActivationFailed(_) => {
                                    this.take_activation_preferences(operation);
                                    this.notebook_open_failed(
                                        io::Error::other(result.as_ref().unwrap_err().to_string()),
                                        window,
                                        cx,
                                    );
                                }
                                other => this.apply_session_effects(vec![other], window, cx),
                            }
                        }
                    });
                })
                .detach();
            } else {
                self.apply_session_effects(vec![effect], window, cx);
            }
        }
    }

    fn load_notebook_after_flush(
        &mut self,
        path: PathBuf,
        create: bool,
        operation: OperationId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        #[cfg(test)]
        let install_delay = std::mem::take(&mut self.notebooks.activation_install_delay);
        let retained = self
            .documents
            .iter()
            .map(|(path, runtime)| (path.clone(), runtime.document.clone()))
            .collect::<std::collections::HashMap<_, _>>();
        let tab_id = self.tabs.active.id;
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let book = notebook::Notebook::open(&path, create)?;
                    let document = match &book.settings.last_file {
                        Some(relative) => {
                            match notebook::resolve(&book.root, relative).and_then(|path| {
                                let canonical = fs::canonicalize(&path)?;
                                retained
                                    .get(&canonical)
                                    .cloned()
                                    .map_or_else(|| Document::open(path), Ok)
                            }) {
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
            #[cfg(test)]
            cx.background_executor().timer(install_delay).await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.tabs.active.id != tab_id {
                    return;
                }
                if this.tabs.active.session.foreground()
                    != Some((operation, app::session::PhaseKind::LoadingNotebook))
                {
                    return;
                }
                let preferences = this.take_activation_preferences(operation);
                match result {
                    Ok((mut book, document)) => {
                        if let Some(preferences) = preferences {
                            preferences.replay(operation, &mut book);
                        }
                        this.search = None;
                        this.search_subscription = None;
                        this.reset_sidebar_resize(cx);
                        this.notebooks.index.register(book.root.clone());
                        let mode = book.settings.view_mode;
                        this.notebook_cache.insert(book.root.clone(), book.clone());
                        for id in this.tabs.ids() {
                            if let Some((root, settings)) =
                                &mut this.tabs.get_mut(id).unwrap().notebook
                                && *root == book.root
                            {
                                *settings = book.settings.autosave;
                            }
                        }
                        this.notebooks.active = Some(book);
                        this.notebooks.reload_exclusions();
                        this.apply_appearance(window, cx);
                        this.install_notebook_tab(operation, document, mode, window, cx);
                        this.refresh_tree(window, cx);
                        this.remember_file(cx);
                        if window.has_active_dialog(cx) {
                            window.close_dialog(cx);
                        }
                    }
                    Err(error) => {
                        this.complete_content_load(operation, Err(error), window, cx);
                    }
                }
                this.refresh(window, cx);
            });
        })
        .detach();
        cx.notify();
    }

    /// The active notebook is canonical. A pending activation can only replay
    /// explicit choices onto that same notebook, never onto a different target.
    pub(super) fn record_activation_preference(&mut self, intent: PreferenceIntent) {
        let Some(preferences) = &mut self.notebooks.activation_preferences else {
            return;
        };
        let operation = preferences.operation();
        if self
            .tabs
            .active
            .session
            .foreground()
            .is_some_and(|(id, _)| id == operation)
            && let Some(book) = &self.notebooks.active
        {
            preferences.record(operation, &book.root, intent);
        }
    }

    fn take_activation_preferences(
        &mut self,
        operation: OperationId,
    ) -> Option<ActivationPreferences> {
        if self
            .notebooks
            .activation_preferences
            .as_ref()
            .is_some_and(|preferences| preferences.operation() == operation)
        {
            self.notebooks.activation_preferences.take()
        } else {
            None
        }
    }

    pub(crate) fn notebook_open_failed(
        &mut self,
        error: io::Error,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let message = format!("Could not open notebook: {error}");
        if window.has_active_dialog(cx) {
            self.notebooks.modal_error = Some(message);
        } else {
            self.notebooks.notice = Some(message);
        }
        self.refresh(window, cx);
    }

    pub(crate) fn create_note_with_operation(
        &mut self,
        operation: OperationId,
        root: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self
            .notebooks
            .active
            .as_ref()
            .is_some_and(|book| book.root == root)
        {
            let observed = self.observed_document(cx);
            let effects = self.tabs.active.session.continued(operation, observed);
            self.apply_session_effects(effects, window, cx);
            return;
        }
        let effects = self
            .tabs
            .active
            .session
            .dispatch(operation, app::session::Stage::Load);
        for effect in effects {
            if let app::session::Effect::Load(operation) = effect {
                let root = root.clone();
                let tab_id = self.tabs.active.id;
                cx.spawn_in(window, async move |this, cx| {
                    let result =
                        cx.background_executor()
                            .spawn(async move {
                                notebook::create_untitled(&root).and_then(Document::open)
                            })
                            .await;
                    let _ = this.update_in(cx, |this, window, cx| {
                        if this.tabs.active.id != tab_id {
                            return;
                        }
                        if this.tabs.active.session.foreground()
                            != Some((operation, app::session::PhaseKind::Loading))
                        {
                            return;
                        }
                        if result.is_ok() && this.tabs.active.mode == ViewMode::Reading {
                            this.set_mode(ViewMode::Source, window, cx);
                        }
                        this.search_destination = None;
                        if this.complete_content_load(
                            operation,
                            result.map(app::session::Loaded::Document),
                            window,
                            cx,
                        ) {
                            this.remember_file(cx);
                            this.reveal_sidebar(cx);
                            this.refresh_tree(window, cx);
                        }
                        this.refresh(window, cx);
                    });
                })
                .detach();
            } else {
                self.apply_session_effects(vec![effect], window, cx);
            }
        }
    }
    pub(crate) fn install_session_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.install_tab_editor(self.tabs.active.id, window, cx);
    }
    pub(crate) fn install_tab_editor(
        &mut self,
        id: crate::tabs::TabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(book) = &self.notebooks.active {
            self.notebook_cache.insert(book.root.clone(), book.clone());
        }
        let path = self
            .tabs
            .get(id)
            .and_then(|tab| tab.path())
            .map(Path::to_path_buf);
        let owning = path.as_ref().and_then(|path| {
            self.notebook_cache
                .values()
                .filter(|book| path.starts_with(&book.root))
                .max_by_key(|book| book.root.components().count())
                .map(|book| (book.root.clone(), book.settings.autosave))
        });
        let owning = owning.or_else(|| {
            self.notebooks
                .active
                .as_ref()
                .filter(|book| {
                    path.as_ref()
                        .is_some_and(|path| path.starts_with(&book.root))
                })
                .map(|book| (book.root.clone(), book.settings.autosave))
        });
        let editor = self.document_editor(id, window, cx);
        let tab = self.tabs.get_mut(id).unwrap();
        tab.apply_document_mode();
        tab.reading_search = None;
        tab.reading_view = None;
        tab.saves.timer = None;
        tab.saves.error = None;
        tab.notebook = owning;
        tab.editor_subscription = Self::subscribe_tab_editor(id, &editor, window, cx);
        tab.editor = editor;
        tab.reading_text = if tab.mode == ViewMode::Reading {
            tab.editor.text(cx)
        } else {
            SharedString::default()
        };
        tab.dirty_task = None;
        tab.dirty_revision += 1;
        tab.dirty_cached = tab.observed(cx).dirty;
        if tab.path().is_none() {
            tab.navigation = Default::default();
            tab.navigation_pending = None;
        }
        tab.editor.set_document_path(path, cx);
        tab.editor.set_mode(tab.mode, cx);
        if id == self.tabs.active.id {
            if self.tabs.active.mode != ViewMode::Reading && self.document().is_some() {
                self.tabs.active.editor.focus(window, cx);
            } else {
                self.focus.focus(window);
            }
        }
        self.reconcile_documents();
        for tab_id in self.tabs.ids() {
            self.schedule_tab_autosave(tab_id, window, cx);
        }
    }

    #[cfg(test)]
    pub(super) fn open_path(
        &mut self,
        path: PathBuf,
        keep_tree_focus: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.request(
            if keep_tree_focus {
                Next::TreeFile(path)
            } else {
                Next::File(path)
            },
            window,
            cx,
        );
    }

    pub(crate) fn open_path_with_operation(
        &mut self,
        operation: OperationId,
        path: PathBuf,
        keep_tree_focus: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if images::supported(&path) {
            self.open_image_with_operation(operation, path, keep_tree_focus, window, cx);
        } else {
            self.load_file_with_operation(operation, path, keep_tree_focus, window, cx);
        }
    }

    pub(crate) fn load_file_with_operation(
        &mut self,
        operation: OperationId,
        path: PathBuf,
        keep_tree_focus: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let effects = self
            .tabs
            .active
            .session
            .dispatch(operation, app::session::Stage::Load);
        for effect in effects {
            if let app::session::Effect::Load(operation) = effect {
                self.notebooks.tree_operation = keep_tree_focus.then_some(operation);
                let notebook_path_valid = self
                    .notebooks
                    .active
                    .as_ref()
                    .and_then(|book| {
                        path.strip_prefix(&book.root)
                            .ok()
                            .map(|_| resolve_document_path(&book.root, &path).is_ok())
                    })
                    .unwrap_or(true);
                if let Some(document) = self
                    .retained_document(&path)
                    .filter(|_| notebook_path_valid)
                {
                    self.notebooks.tree_operation = None;
                    if self.complete_content_load(
                        operation,
                        Ok(app::session::Loaded::Document(document)),
                        window,
                        cx,
                    ) {
                        self.remember_file(cx);
                        if keep_tree_focus {
                            self.notebooks.tree_focus.focus(window);
                        }
                        if let Some(destination) = self.search_destination.take() {
                            self.reveal_search_result(destination, window, cx);
                            self.finish_search_navigation(true, window, cx);
                        }
                    }
                    continue;
                }
                let root = self.notebooks.active.as_ref().map(|book| book.root.clone());
                let path = path.clone();
                let tab_id = self.tabs.active.id;
                cx.spawn_in(window, async move |this, cx| {
                    let result = cx
                        .background_executor()
                        .spawn(async move {
                            if let Some(root) = root
                                && path.starts_with(&root)
                            {
                                return resolve_document_path(&root, &path)
                                    .and_then(Document::open);
                            }
                            Document::open(path)
                        })
                        .await;
                    let _ = this.update_in(cx, |this, window, cx| {
                        if this.tabs.active.id != tab_id {
                            return;
                        }
                        if this.tabs.active.session.foreground()
                            != Some((operation, app::session::PhaseKind::Loading))
                        {
                            return;
                        }
                        if this.notebooks.tree_operation == Some(operation) {
                            this.notebooks.tree_operation = None;
                        }
                        let installed = this.complete_content_load(
                            operation,
                            result.map(app::session::Loaded::Document),
                            window,
                            cx,
                        );
                        if installed {
                            this.remember_file(cx);
                            if keep_tree_focus {
                                this.notebooks.tree_focus.focus(window);
                            }
                            if let Some(destination) = this.search_destination.take() {
                                this.reveal_search_result(destination, window, cx);
                                this.finish_search_navigation(true, window, cx);
                            }
                        } else if this.tabs.active.session.foreground()
                            == Some((operation, app::session::PhaseKind::Error))
                        {
                            this.search_destination = None;
                            this.finish_search_navigation(false, window, cx);
                            if keep_tree_focus {
                                this.restore_tree_cursor(window, cx);
                            }
                        }
                        this.refresh(window, cx);
                    });
                })
                .detach();
            } else {
                self.apply_session_effects(vec![effect], window, cx);
            }
        }
    }
    pub(super) fn remember_file(&mut self, cx: &mut Context<Self>) {
        let document_path = self.document().map(|doc| doc.path.clone());
        if self.notebooks.active.as_ref().is_some_and(|book| {
            document_path.as_ref() == Some(&notebook::settings_path(&book.root))
        }) {
            // Opening or selecting the settings document must not rewrite its
            // freshly loaded baseline, nor replace the notebook's last note.
            return;
        }
        if let Some(book) = &mut self.notebooks.active {
            // File -> Open may select a standalone document outside this notebook.
            if let Some(relative) = document_path
                .as_ref()
                .and_then(|path| path.strip_prefix(&book.root).ok())
            {
                if notebook::safe_relative(relative) {
                    book.settings.last_file = Some(relative.to_path_buf());
                }
            } else if document_path.is_none() {
                book.settings.last_file = None;
            }
        }
        self.persist_notebooks(cx);
    }

    pub(super) fn persist_notebooks(&mut self, cx: &mut Context<Self>) {
        if let Some(book) = &self.notebooks.active {
            self.notebook_cache.insert(book.root.clone(), book.clone());
        }
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
        self.notebooks
            .writer
            .get_or_insert_with(|| notebook::SettingsWriter::new(path))
            .submit(index, book);
        self.pump_settings_writer(cx);
    }

    pub(crate) fn persist_cached_notebook(
        &mut self,
        book: notebook::Notebook,
        cx: &mut Context<Self>,
    ) {
        if !self.notebooks.index_writable {
            return;
        }
        let Some(path) = self.notebooks.index_path.clone() else {
            return;
        };
        self.notebooks
            .writer
            .get_or_insert_with(|| notebook::SettingsWriter::new(path))
            .submit(self.notebooks.index.clone(), Some(book));
        self.pump_settings_writer(cx);
    }

    pub(crate) fn external_autosave_enabled(&self) -> bool {
        self.notebooks
            .active
            .as_ref()
            .map_or(self.session_autosave.enabled, |book| {
                book.settings.autosave_external_files
            })
    }

    pub(crate) fn set_external_autosave_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        if self.notebooks.active.is_some() {
            self.record_activation_preference(PreferenceIntent::ExternalAutosave(enabled));
            let book = self.notebooks.active.as_mut().unwrap();
            if book.settings.autosave_external_files != enabled {
                book.settings.autosave_external_files = enabled;
                self.persist_notebooks(cx);
            }
        } else {
            self.session_autosave.enabled = enabled;
        }
    }
    fn pump_settings_writer(&mut self, cx: &mut Context<Self>) {
        let Some(mut job) = self
            .notebooks
            .writer
            .as_mut()
            .and_then(|writer| writer.take_job())
        else {
            return;
        };
        let window = self.notebooks.window;
        #[cfg(test)]
        let delay = std::mem::take(&mut self.notebooks.write_delay);
        // One worker drains eligible destinations. Submissions only replace model
        // snapshots; they never append another task waiting on this task.
        self.notebooks.persistence = Some(cx.spawn(async move |this, cx| {
            #[cfg(test)]
            if !delay.is_zero() {
                cx.background_executor().timer(delay).await;
            }
            loop {
                let id = job.id();
                let path = job.path().to_path_buf();
                let result = cx
                    .background_executor()
                    .spawn(async move { job.write() })
                    .await;
                let next = window
                    .update(cx, |_, window, cx| {
                        this.update(cx, |this, cx| {
                            let message = result.as_ref().err().map(|error| {
                                format!(
                                    "Could not save notebook settings at {}: {error}",
                                    notebook::display_path(&path)
                                )
                            });
                            let writer = this.notebooks.writer.as_mut()?;
                            if !writer.complete(id, result) {
                                return None;
                            }
                            if let Some(message) = message {
                                this.notebooks.notice = Some(message);
                            }
                            this.resolve_settings_waiter(window, cx);
                            this.resolve_settings_file_open(window, cx);
                            cx.notify();
                            this.notebooks
                                .writer
                                .as_mut()
                                .and_then(|writer| writer.take_job())
                        })
                        .ok()
                        .flatten()
                    })
                    .ok()
                    .flatten();
                let Some(next) = next else { break };
                job = next;
            }
        }));
    }

    pub(crate) fn close_with_operation(
        &mut self,
        operation: OperationId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let effects = self
            .tabs
            .active
            .session
            .dispatch(operation, app::session::Stage::Close);
        for effect in effects {
            if let app::session::Effect::FlushClose { operation, .. } = effect {
                self.notebooks.settings_waiter = Some(SettingsWaiter {
                    operation,
                    barrier: self
                        .notebooks
                        .writer
                        .as_ref()
                        .map(|writer| writer.flush_all())
                        .unwrap_or_default(),
                    after: SettingsAfterFlush::Close,
                });
                cx.defer_in(window, |this, window, cx| {
                    this.resolve_settings_waiter(window, cx)
                });
            } else {
                self.apply_session_effects(vec![effect], window, cx);
            }
        }
    }

    pub(crate) fn cancel_session_context(&mut self, operation: OperationId) {
        if self
            .notebooks
            .settings_waiter
            .as_ref()
            .is_some_and(|waiter| waiter.operation == operation)
        {
            self.notebooks.settings_waiter = None;
        }
        if self.notebooks.tree_operation == Some(operation) {
            self.notebooks.tree_operation = None;
        }
        self.take_activation_preferences(operation);
        self.search_destination = None;
    }

    fn resolve_settings_waiter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(mut waiter) = self.notebooks.settings_waiter.take() else {
            return;
        };
        let closing = matches!(waiter.after, SettingsAfterFlush::Close);
        let phase = if closing {
            app::session::PhaseKind::Closing
        } else {
            app::session::PhaseKind::FlushingActivation
        };
        if self.tabs.active.session.foreground() != Some((waiter.operation, phase)) {
            self.take_activation_preferences(waiter.operation);
            return;
        }
        // Include every late preference submission before closing, even after the
        // explicit permission to discard failed preferences has been supplied.
        if closing {
            waiter.barrier = self
                .notebooks
                .writer
                .as_ref()
                .map(|writer| writer.flush_all())
                .unwrap_or_default();
        }
        let status = self
            .notebooks
            .writer
            .as_ref()
            .map(|writer| writer.flush_status(&waiter.barrier))
            .unwrap_or(notebook::FlushStatus::Succeeded);
        if matches!(status, notebook::FlushStatus::Pending) {
            self.notebooks.settings_waiter = Some(waiter);
            return;
        }
        let effects = if closing {
            self.tabs.active.session.close_flushed(
                waiter.operation,
                matches!(status, notebook::FlushStatus::Succeeded),
            )
        } else {
            self.tabs.active.session.activation_flushed(
                waiter.operation,
                matches!(status, notebook::FlushStatus::Succeeded),
            )
        };
        let mut failed = false;
        for effect in effects {
            match effect {
                app::session::Effect::Load(operation) => {
                    if let SettingsAfterFlush::Activate { path, create } = &waiter.after {
                        self.load_notebook_after_flush(
                            path.clone(),
                            *create,
                            operation,
                            window,
                            cx,
                        );
                    }
                }
                app::session::Effect::CloseFailed(_)
                | app::session::Effect::ActivationFlushFailed(_) => failed = true,
                other => self.apply_session_effects(vec![other], window, cx),
            }
        }
        if !failed {
            return;
        }
        let notebook::FlushStatus::Failed(errors) = status else {
            unreachable!()
        };
        let mut buttons = vec![
            PromptButton::Ok("Retry".into()),
            PromptButton::Cancel("Cancel".into()),
        ];
        if closing {
            buttons.push(PromptButton::Other(
                "Close without saving preferences".into(),
            ));
        }
        let detail = errors
            .iter()
            .map(|error| format!("{}: {}", notebook::display_path(&error.path), error.message))
            .collect::<Vec<_>>()
            .join("\n");
        let prompt = window.prompt(
            PromptLevel::Warning,
            "Could not save notebook preferences",
            Some(&detail),
            &buttons,
            cx,
        );
        let tab_id = self.tabs.active.id;
        cx.spawn_in(window, async move |this, cx| {
            let answer = prompt.await.ok();
            let _ = this.update_in(cx, |this, window, cx| {
                if this.tabs.active.id != tab_id {
                    return;
                }
                this.answer_settings_failure(waiter, answer, window, cx);
            });
        })
        .detach();
    }

    fn answer_settings_failure(
        &mut self,
        mut waiter: SettingsWaiter,
        answer: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let closing = matches!(waiter.after, SettingsAfterFlush::Close);
        let effects = match answer {
            Some(0) if closing => self.tabs.active.session.retry_close(waiter.operation),
            Some(0) => self.tabs.active.session.retry_activation(waiter.operation),
            Some(2) if closing => self
                .tabs
                .active
                .session
                .discard_close_preferences(waiter.operation),
            _ => {
                let observed = self.observed_document(cx);
                self.tabs.active.session.cancel(waiter.operation, observed)
            }
        };
        for effect in effects {
            match effect {
                app::session::Effect::FlushClose {
                    operation, retry, ..
                }
                | app::session::Effect::FlushActivation { operation, retry } => {
                    if let Some(preferences) = &mut self.notebooks.activation_preferences {
                        preferences.renew(waiter.operation, operation);
                    }
                    waiter.operation = operation;
                    if closing {
                        waiter.barrier = self
                            .notebooks
                            .writer
                            .as_ref()
                            .map(|writer| writer.flush_all())
                            .unwrap_or_default();
                    }
                    if retry && let Some(writer) = &mut self.notebooks.writer {
                        writer.retry(&waiter.barrier);
                    }
                    self.notebooks.settings_waiter = Some(waiter);
                    self.pump_settings_writer(cx);
                    self.resolve_settings_waiter(window, cx);
                    self.refresh(window, cx);
                    return;
                }
                other => self.apply_session_effects(vec![other], window, cx),
            }
        }
    }

    fn refresh_tree(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.notebooks.context_menu = None;
        self.notebooks.select_after_refresh = None;
        self.notebooks.explorer.clear();
        self.load_directory(PathBuf::new(), window, cx);
    }

    fn load_directory(&mut self, relative: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let Some(book) = &self.notebooks.active else {
            return;
        };
        let root = book.root.clone();
        let exclusions = self.notebooks.exclusions.clone();
        if exclusions.excludes(&relative) {
            return;
        }
        let generation = self
            .notebooks
            .explorer
            .begin_directory(relative.clone(), &book.settings);
        cx.spawn_in(window, async move |this, cx| {
            let path = relative.clone();
            let result = cx
                .background_executor()
                .spawn(async move { notebook::read_directory_filtered(&root, &path, &exclusions) })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                let Some(book) = &this.notebooks.active else {
                    return;
                };
                let Some(children) = this.notebooks.explorer.complete_directory(
                    generation,
                    relative,
                    result.map_err(|error| error.to_string()),
                    &book.settings,
                ) else {
                    return;
                };
                for path in children {
                    this.load_directory(path, window, cx);
                }
                if let Some(target) = &this.notebooks.folder_target
                    && let Some(index) = this
                        .notebooks
                        .explorer
                        .rows()
                        .iter()
                        .position(|row| row.directory && &row.path == target)
                {
                    this.notebooks.explorer.select(Some(index));
                    this.notebooks
                        .scroll
                        .scroll_to_item(index, ScrollStrategy::Top);
                    this.notebooks.folder_target = None;
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    fn activate_row(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.document_interaction_blocked() {
            return;
        }
        let Some(row) = self.notebooks.explorer.rows().get(index).cloned() else {
            return;
        };
        self.notebooks.explorer.select(Some(index));
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
            if !self.notebooks.explorer.contains_directory(&row.path) {
                self.load_directory(row.path, window, cx);
            }
            self.notebooks.rebuild_rows();
            self.persist_notebooks(cx);
        } else {
            let path = book.root.join(row.path);
            if !self.document().is_some_and(|d| d.path == path) {
                self.request(Next::TreeFile(path), window, cx);
            }
        }
        cx.notify();
    }

    fn tree_loading(&self) -> bool {
        self.notebooks.tree_operation.is_some_and(|operation| {
            self.tabs.active.session.foreground()
                == Some((operation, app::session::PhaseKind::Loading))
        })
    }

    fn tree_move(&mut self, down: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings_panel.is_some() || (self.is_busy() && !self.tree_loading()) {
            return;
        }
        let Some(next) = self.notebooks.explorer.move_cursor(down) else {
            return;
        };
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
        let selection = self
            .notebooks
            .active
            .as_ref()
            .zip(
                self.document()
                    .map(|d| d.path.as_path())
                    .or(self.image_path()),
            )
            .and_then(|(book, path)| {
                self.notebooks
                    .explorer
                    .rows()
                    .iter()
                    .position(|row| !row.directory && book.root.join(&row.path) == path)
            });
        self.notebooks.explorer.select(selection);
        if let Some(index) = self.notebooks.explorer.cursor() {
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
            .explorer
            .selected()
            .filter(|row| !row.directory && !row.disabled && !row.retry)
            .and_then(|row| {
                self.notebooks
                    .active
                    .as_ref()
                    .map(|book| book.root.join(&row.path))
            });
        if self.tree_loading() {
            let target =
                target.filter(|path| !self.document().is_some_and(|doc| &doc.path == path));
            let effects = self.tabs.active.session.retarget_load(
                self.notebooks.tree_operation.unwrap(),
                target.map(Next::TreeFile),
            );
            self.apply_session_effects(effects, window, cx);
        } else if let Some(path) = target
            && !self.document().is_some_and(|doc| doc.path == path)
        {
            self.request(Next::TreeFile(path), window, cx);
        }
    }

    fn tree_horizontal(&mut self, right: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings_panel.is_some() || (self.is_busy() && !self.tree_loading()) {
            return;
        }
        let Some(index) = self.notebooks.explorer.cursor() else {
            return;
        };
        let Some(row) = self.notebooks.explorer.rows().get(index) else {
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
            if let Some(parent) = parent {
                self.notebooks.explorer.select_path(&parent, true);
            } else {
                self.notebooks.explorer.select(None);
            }
            if let Some(i) = self.notebooks.explorer.cursor() {
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
            .on_mouse_down(
                gpui::MouseButton::Right,
                cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    this.open_entry_menu(None, event.position, window, cx);
                }),
            )
            .on_action(cx.listener(|this, _: &TreeContextMenu, window, cx| {
                let path = this
                    .notebooks
                    .explorer
                    .selected()
                    .filter(|row| row.draggable)
                    .map(|row| row.path.clone());
                this.open_entry_menu(path, gpui::point(px(32.), px(90.)), window, cx);
            }))
            .on_drop(cx.listener(|this, drag: &EntryDrag, window, cx| {
                cx.stop_propagation();
                this.drop_entry(drag, PathBuf::new(), window, cx);
            }))
            .on_action(cx.listener(|this, _: &RenameEntry, window, cx| {
                this.rename_selected_entry(window, cx);
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
                if let Some(i) = this.notebooks.explorer.cursor() {
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
                    self.notebooks.explorer.rows().len(),
                    cx.processor(|this, range: std::ops::Range<usize>, _, cx| {
                        range
                            .map(|index| {
                                let row = &this.notebooks.explorer.rows()[index];
                                let selected = this
                                    .notebooks
                                    .active
                                    .as_ref()
                                    .zip(
                                        this.document()
                                            .map(|d| d.path.as_path())
                                            .or(this.image_path()),
                                    )
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
                                let context_path = row.path.clone();
                                let context_enabled = row.draggable;
                                let drag = EntryDrag {
                                    root: this.notebooks.active.as_ref().unwrap().root.clone(),
                                    path: path.clone(),
                                };
                                div()
                                    .id(index)
                                    .debug_selector(|| {
                                        format!("notebook-row-{}", row.path.display())
                                    })
                                    .on_mouse_down(
                                        gpui::MouseButton::Right,
                                        cx.listener(
                                            move |this,
                                                  event: &gpui::MouseDownEvent,
                                                  window,
                                                  cx| {
                                                cx.stop_propagation();
                                                if context_enabled
                                                    && !this.document_interaction_blocked()
                                                {
                                                    this.notebooks.explorer.select(Some(index));
                                                    this.open_entry_menu(
                                                        Some(context_path.clone()),
                                                        event.position,
                                                        window,
                                                        cx,
                                                    );
                                                }
                                            },
                                        ),
                                    )
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
                                    .when(this.notebooks.explorer.cursor() == Some(index), |view| {
                                        view.border_color(cx.theme().border)
                                    })
                                    .when(row.disabled, |view| {
                                        view.text_color(cx.theme().muted_foreground)
                                    })
                                    .relative()
                                    .when(this.settings_panel.is_some(), |view| view.opacity(0.5))
                                    // Rows consume drops so only unoccupied space falls back to root.
                                    .when(
                                        !row.draggable
                                            || !directory
                                            || this.document_interaction_blocked(),
                                        |view| {
                                            view.on_drop(|_: &EntryDrag, _, cx| {
                                                cx.stop_propagation()
                                            })
                                        },
                                    )
                                    .when(
                                        row.draggable && !this.document_interaction_blocked(),
                                        |view| {
                                            view.on_drag(drag, |drag, _, _, cx| {
                                                cx.stop_propagation();
                                                cx.new(|_| drag.clone())
                                            })
                                        },
                                    )
                                    .when(
                                        row.draggable
                                            && directory
                                            && !this.document_interaction_blocked(),
                                        |view| {
                                            view.drag_over::<EntryDrag>(|view, _, _, cx| {
                                                view.bg(cx.theme().accent)
                                            })
                                            .on_drop(
                                                cx.listener(
                                                    move |this, drag: &EntryDrag, window, cx| {
                                                        cx.stop_propagation();
                                                        this.drop_entry(
                                                            drag,
                                                            path.clone(),
                                                            window,
                                                            cx,
                                                        );
                                                    },
                                                ),
                                            )
                                        },
                                    )
                                    .child(
                                        gpui::StyledText::new(format!("{prefix}{}", row.label))
                                            .with_highlights(
                                                (row.directory && !row.disabled).then_some((
                                                    0..prefix.len(),
                                                    gpui::HighlightStyle {
                                                        color: cx.theme().icon_color,
                                                        ..Default::default()
                                                    },
                                                )),
                                            ),
                                    )
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        if this.document_interaction_blocked() {
                                            return;
                                        }
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
        if self.settings_panel.is_some() || window.has_active_dialog(cx) {
            return;
        }
        self.move_sidebar_entry(drag, parent, None, window, cx);
    }
    fn move_sidebar_entry(
        &mut self,
        drag: &EntryDrag,
        parent: PathBuf,
        name: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.document_interaction_blocked()
            || self
                .notebooks
                .active
                .as_ref()
                .is_none_or(|book| book.root != drag.root)
            || (name.is_none() && drag.path.parent() == Some(parent.as_path()))
        {
            return;
        }
        self.begin_workspace(
            crate::tabs::WorkspaceKind::Move {
                from: drag.path.clone(),
                parent,
                name,
            },
            self.tabs.ids(),
            window,
            cx,
        );
    }
    pub(crate) fn move_prepared_entry(
        &mut self,
        from: PathBuf,
        parent: PathBuf,
        name: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(book) = &self.notebooks.active else {
            self.cancel_workspace(window, cx);
            return;
        };
        let root = book.root.clone();
        let mut operations = Vec::new();
        let versions = self
            .documents
            .values()
            .map(|runtime| {
                let doc = &runtime.document;
                (doc.id, doc.path.clone(), doc.version())
            })
            .collect::<Vec<_>>();
        for id in self.tabs.ids() {
            let effects = self.tabs.get_mut(id).unwrap().session.move_content();
            for effect in effects {
                if let app::session::Effect::Move(operation) = effect {
                    operations.push((id, operation));
                } else {
                    self.apply_tab_effects(id, vec![effect], window, cx);
                }
            }
        }
        cx.spawn_in(window, async move |this, cx| {
            let worker_root = root.clone(); let worker_from = from.clone();
            let result = cx.background_executor().spawn(async move {
                let to = if let Some(name) = name { notebook::rename_entry(&worker_root, &worker_from, &name)? } else { notebook::move_entry(&worker_root, &worker_from, &parent)? };
                let versions = versions.into_iter().filter_map(|(id, old, version)| {
                    let path = notebook::remap_path(&old, &worker_root.join(&worker_from), &worker_root.join(&to));
                    (path != old).then(|| (id, document::version_after_move(&path, &version)))
                }).collect::<std::collections::HashMap<_,_>>();
                Ok::<_,io::Error>((to, versions))
            }).await;
            let _ = this.update_in(cx, |this, window, cx| {
                match result {
                    Ok((to, mut versions)) => {
                        for (id, operation) in operations {
                            let Some(tab) = this.tabs.get_mut(id) else { continue; };
                            let dirty = tab.observed(cx).dirty; let mut error = None;
                            let effects = tab.session.moved(operation, |content| {
                                match content {
                                    app::session::MovingContent::Document(doc) => {
                                        doc.path = notebook::remap_path(&doc.path, &root.join(&from), &root.join(&to));
                                        if let Some(result) = versions.remove(&doc.id) { match result { Ok(version) => doc.set_version(version), Err(e) => error = Some(e) } }
                                    }
                                    app::session::MovingContent::Image(path) => *path = notebook::remap_path(path, &root.join(&from), &root.join(&to)),
                                    app::session::MovingContent::Empty => {}
                                }
                                app::session::MoveOutcome { dirty, conflict: error.is_some() }
                            });
                            if effects.is_empty() { continue; }
                            tab.navigation.remap(&root.join(&from), &root.join(&to));
                            tab.apply_document_mode();
                            let path = tab.document().map(|d| d.path.clone()); tab.editor.set_document_path(path, cx);
                            tab.editor.set_mode(tab.mode, cx);
                            if let Some(error) = error { this.save_state_mut(id).error = Some(format!("Could not validate moved file: {error}. Your changes remain in this document.")); }
                            let effects = effects.into_iter().filter(|e| !matches!(e, app::session::Effect::Moved(_) | app::session::Effect::MoveBaselineConflict(_))).collect();
                            this.apply_tab_effects(id, effects, window, cx);
                        }
                        if let Some(book) = &mut this.notebooks.active && book.root == root { book.settings.remap_entry(&from, &to); }
                        this.notebooks.notice = None; this.persist_notebooks(cx); this.notebooks.explorer.remap_visible_paths(&from, &to);
                        this.refresh_sorted_tree(window, cx); this.schedule_search(window, cx);
                    }
                    Err(error) => {
                        for (id, operation) in operations {
                            let tab = this.tabs.get_mut(id).unwrap(); let observed = tab.observed(cx);
                            let effects = tab.session.move_failed(operation, observed).into_iter().filter(|e| !matches!(e, app::session::Effect::MoveFailed(_))).collect();
                            this.apply_tab_effects(id, effects, window, cx);
                        }
                        this.notebooks.notice = Some(format!("Could not move entry: {error}"));
                    }
                }
                this.reconcile_documents();
                this.cancel_workspace(window, cx);
            });
        }).detach();
    }
}

impl Notes {
    pub(super) fn set_file_sort(
        &mut self,
        sort: notebook::FileSort,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.document_interaction_blocked() || window.has_active_dialog(cx) {
            return;
        }
        let Some(book) = &mut self.notebooks.active else {
            return;
        };
        if book.settings.file_sort == sort {
            return;
        }
        book.settings.file_sort = sort;
        self.notebooks.explorer.resort(&book.settings);
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
        let exclusions = self.notebooks.exclusions.clone();
        let mut paths = book.settings.expanded.clone();
        paths.retain(|path| !exclusions.excludes(path));
        paths.insert(PathBuf::new());
        let generation = self.notebooks.explorer.begin_refresh();
        cx.spawn_in(window, async move |this, cx| {
            let directories = cx
                .background_executor()
                .spawn(async move {
                    paths
                        .into_iter()
                        .map(|path| {
                            let result =
                                notebook::read_directory_filtered(&root, &path, &exclusions);
                            (path, result.map_err(|error| error.to_string()))
                        })
                        .collect::<Vec<_>>()
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                let Some(book) = &this.notebooks.active else {
                    return;
                };
                let Some(missing) = this.notebooks.explorer.complete_refresh(
                    generation,
                    directories,
                    &book.settings,
                ) else {
                    return;
                };
                if let Some(path) = this.notebooks.select_after_refresh.take() {
                    let row = this
                        .notebooks
                        .explorer
                        .rows()
                        .iter()
                        .position(|row| row.path == path);
                    this.notebooks.explorer.select(row);
                    if let Some(row) = row {
                        this.notebooks
                            .scroll
                            .scroll_to_item(row, ScrollStrategy::Top);
                    }
                }
                for path in missing {
                    this.load_directory(path, window, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }
}

#[cfg(test)]
impl NotebookState {
    pub(crate) fn test_explorer(&mut self) -> &mut Explorer {
        &mut self.explorer
    }
    pub(crate) fn test_set_reveal_backend(&mut self, backend: Box<RevealBackend>) {
        self.reveal_backend = backend;
    }
}

#[cfg(test)]
impl Notes {
    pub(crate) fn test_refresh_tree(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.refresh_tree(window, cx);
    }
}
