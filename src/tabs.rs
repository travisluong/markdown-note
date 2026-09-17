//! Retained document ownership. Tab identities qualify session-local tickets.
use crate::*;
use app::session::{Effect, Observed};
use gpui_component::{Disableable, IconName};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TabId(pub(crate) u64);

pub(crate) struct Tab {
    pub(crate) id: TabId,
    pub(crate) session: Session<Document, PathBuf, Next>,
    pub(crate) navigation: app::navigation::NavigationHistory,
    pub(crate) navigation_pending: Option<(OperationId, app::navigation::Visit)>,
    pub(crate) editor: Editor,
    pub(crate) editor_subscription: Subscription,
    pub(crate) mode: ViewMode,
    markdown_mode: Option<ViewMode>,
    pub(crate) reading_text: SharedString,
    pub(crate) reading_search: Option<reading_search::ReadingSearch>,
    pub(crate) reading_view: Option<TextView>,
    pub(crate) dirty_cached: bool,
    pub(crate) dirty_revision: u64,
    pub(crate) dirty_task: Option<Task<()>>,
    pub(crate) saves: autosave::SaveState,
    pub(crate) notebook: Option<(PathBuf, notebook::Autosave)>,
}

impl Tab {
    fn new(id: TabId, window: &mut Window, cx: &mut Context<Notes>) -> Self {
        let editor = Editor::new(SharedString::default(), window, cx);
        let editor_subscription = Notes::subscribe_tab_editor(id, &editor, window, cx);
        Self {
            id,
            session: Session::default(),
            navigation: Default::default(),
            navigation_pending: None,
            editor,
            editor_subscription,
            mode: ViewMode::default(),
            markdown_mode: None,
            reading_text: SharedString::default(),
            reading_search: None,
            reading_view: None,
            dirty_cached: false,
            dirty_revision: 0,
            dirty_task: None,
            saves: Default::default(),
            notebook: None,
        }
    }
    pub(crate) fn document(&self) -> Option<&Document> {
        match self.session.content() {
            Content::Document { value, .. } => Some(value),
            _ => None,
        }
    }
    pub(crate) fn source_only(&self) -> bool {
        self.document()
            .is_some_and(|doc| !notebook::markdown(&doc.path))
    }
    /// Forcing Source is temporary and never changes notebook preferences.
    pub(crate) fn apply_document_mode(&mut self) {
        if self.source_only() {
            self.markdown_mode.get_or_insert(self.mode);
            self.mode = ViewMode::Source;
        } else if let Some(mode) = self.markdown_mode.take() {
            self.mode = mode;
        }
    }
    pub(crate) fn path(&self) -> Option<&Path> {
        match self.session.content() {
            Content::Document { value, .. } => Some(&value.path),
            Content::Image { value, .. } => Some(value),
            Content::Empty => None,
        }
    }
    pub(crate) fn observed(&self, cx: &App) -> Observed {
        Observed {
            document: self.session.content().document_id(),
            dirty: self
                .document()
                .is_some_and(|d| d.saved_rope() != self.editor.snapshot(cx)),
        }
    }
}

pub(crate) struct TabSet {
    pub(crate) active: Tab,
    inactive: Vec<Tab>,
    order: Vec<TabId>,
    serial: u64,
    scroll: ScrollHandle,
}
impl TabSet {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Notes>) -> Self {
        Self {
            active: Tab::new(TabId(1), window, cx),
            inactive: vec![],
            order: vec![TabId(1)],
            serial: 1,
            scroll: ScrollHandle::new(),
        }
    }
    pub(crate) fn get(&self, id: TabId) -> Option<&Tab> {
        if self.active.id == id {
            Some(&self.active)
        } else {
            self.inactive.iter().find(|t| t.id == id)
        }
    }
    pub(crate) fn get_mut(&mut self, id: TabId) -> Option<&mut Tab> {
        if self.active.id == id {
            Some(&mut self.active)
        } else {
            self.inactive.iter_mut().find(|t| t.id == id)
        }
    }
    pub(crate) fn ids(&self) -> Vec<TabId> {
        self.order.clone()
    }
    pub(crate) fn iter(&self) -> impl Iterator<Item = &Tab> {
        self.order.iter().filter_map(|id| self.get(*id))
    }
    fn select(&mut self, id: TabId) {
        if let Some(index) = self.inactive.iter().position(|t| t.id == id) {
            std::mem::swap(&mut self.active, &mut self.inactive[index]);
        }
    }
    fn add(&mut self, window: &mut Window, cx: &mut Context<Notes>) -> TabId {
        self.serial = self.serial.checked_add(1).expect("tab identity exhausted");
        let id = TabId(self.serial);
        self.inactive.push(Tab::new(id, window, cx));
        self.order.push(id);
        self.select(id);
        id
    }
}

// Compatibility for diagnostic fixtures. Production code names the active owner.
#[cfg(any(test, feature = "editor-diagnostics"))]
impl std::ops::Deref for Notes {
    type Target = Tab;
    fn deref(&self) -> &Tab {
        &self.tabs.active
    }
}
#[cfg(any(test, feature = "editor-diagnostics"))]
impl std::ops::DerefMut for Notes {
    fn deref_mut(&mut self) -> &mut Tab {
        &mut self.tabs.active
    }
}

#[derive(Clone)]
pub(crate) enum WorkspaceKind {
    Close,
    CloseTab(TabId),
    Move {
        from: PathBuf,
        parent: PathBuf,
        name: Option<String>,
    },
    Trash {
        target: notebook_ui::EntryTarget,
    },
}
pub(crate) struct WorkspaceOperation {
    kind: WorkspaceKind,
    queue: std::collections::VecDeque<TabId>,
    current: Option<TabId>,
}

impl Notes {
    pub(crate) fn install_notebook_tab(
        &mut self,
        operation: OperationId,
        document: Option<Document>,
        mode: ViewMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.tabs.active.path().is_none() {
            self.tabs.active.mode = mode;
            self.complete_content_load(
                operation,
                Ok(document.map_or(app::session::Loaded::Empty, app::session::Loaded::Document)),
                window,
                cx,
            );
            return;
        }
        let observed = self.observed_document(cx);
        let effects = self
            .tabs
            .active
            .session
            .activation_retained(operation, observed);
        if effects.is_empty() {
            return;
        }
        self.apply_session_effects(effects, window, cx);
        self.tabs.add(window, cx);
        self.tabs.active.mode = mode;
        let observed = self.observed_document(cx);
        let effects = self.tabs.active.session.request(Next::Nothing, observed);
        let operation = effects
            .iter()
            .find_map(|e| {
                if let Effect::Continue { operation, .. } = e {
                    Some(*operation)
                } else {
                    None
                }
            })
            .expect("new tab is idle");
        let _ = self
            .tabs
            .active
            .session
            .dispatch(operation, app::session::Stage::Load);
        self.complete_content_load(
            operation,
            Ok(document.map_or(app::session::Loaded::Empty, app::session::Loaded::Document)),
            window,
            cx,
        );
    }
    pub(crate) fn apply_tab_effects(
        &mut self,
        id: TabId,
        effects: Vec<Effect<Next>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.tabs.get(id).is_none() {
            return;
        }
        if self.workspace.is_some() && effects.iter().any(|e| matches!(e, Effect::WriteFailed(_))) {
            let tab = self.tabs.get_mut(id).unwrap();
            if let Some((operation, _)) = tab.session.foreground() {
                let observed = tab.observed(cx);
                let _ = tab.session.cancel(operation, observed);
            }
            self.cancel_workspace(window, cx);
            return;
        }
        for effect in effects {
            match effect {
                Effect::ArmAutosave(ticket) => self.arm_tab_autosave(id, ticket, window, cx),
                Effect::CancelAutosave => self.save_state_mut(id).timer = None,
                Effect::Write(ticket) => self.write_tab_snapshot(id, ticket, window, cx),
                Effect::BaselineAccepted(_) => {
                    self.check_tab_dirty(id, window, cx);
                    self.refresh_sorted_tree(window, cx);
                }
                Effect::StateChanged => self.refresh(window, cx),
                Effect::WriteFailed(_) => {}
                Effect::Prompt(operation) if self.workspace.is_some() => {
                    self.prompt_prepared_tab(id, operation, window, cx)
                }
                Effect::Continue {
                    operation,
                    action: Next::Prepared,
                } => {
                    let tab = self.tabs.get_mut(id).unwrap();
                    let observed = tab.observed(cx);
                    let effects = tab.session.continued(operation, observed);
                    self.apply_tab_effects(id, effects, window, cx);
                    if let Some(workspace) = &mut self.workspace {
                        workspace.current = None;
                    }
                    self.advance_workspace(window, cx);
                }
                Effect::Cancelled { .. } if self.workspace.is_some() => {
                    self.cancel_workspace(window, cx)
                }
                other if id == self.tabs.active.id => {
                    self.apply_session_effects(vec![other], window, cx)
                }
                _ => unreachable!("inactive sessions only own background writes"),
            }
        }
    }
    pub(crate) fn select_tab(&mut self, id: TabId, window: &mut Window, cx: &mut Context<Self>) {
        if self.document_interaction_blocked() || window.has_active_dialog(cx) {
            return;
        }
        self.tabs.active.editor.break_undo_group(cx);
        self.tabs.select(id);
        self.tabs.active.editor.break_undo_group(cx);
        self.focus_active_tab(window, cx);
    }
    fn focus_active_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.focus_settings(window, cx) {
            return;
        }
        if self.tabs.active.mode != ViewMode::Reading && self.document().is_some() {
            self.tabs.active.editor.focus(window, cx);
        } else {
            self.focus.focus(window);
        }
        if let Some(index) = self
            .tabs
            .order
            .iter()
            .position(|id| *id == self.tabs.active.id)
        {
            self.tabs.scroll.scroll_to_item(index);
        }
        if self.document().is_some() {
            self.remember_file(cx);
        }
        if self.tabs.active.mode == ViewMode::Reading {
            self.update_reading_search(cx);
        }
        self.refresh(window, cx);
    }
    pub(crate) fn new_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.document_interaction_blocked() || window.has_active_dialog(cx) {
            return;
        }
        self.tabs.active.editor.break_undo_group(cx);
        self.tabs.add(window, cx);
        if let Some(book) = &self.notebooks.active {
            self.tabs.active.mode = book.settings.view_mode;
        }
        self.focus_active_tab(window, cx);
    }
    pub(crate) fn render_tabs(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let busy = self.document_interaction_blocked();
        div()
            .id("document-tabs")
            .debug_selector(|| "document-tabs".into())
            .flex()
            .items_center()
            .flex_shrink_0()
            .min_w_0()
            .overflow_x_scroll()
            .track_scroll(&self.tabs.scroll)
            .border_b_1()
            .border_color(cx.theme().border)
            .children(self.tabs.iter().map(|tab| {
                let id = tab.id;
                let title = tab
                    .path()
                    .and_then(Path::file_name)
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "New tab".into());
                let tooltip = tab
                    .path()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| "New tab".into());
                div()
                    .id(("tab", id.0))
                    .flex()
                    .items_center()
                    .flex_shrink_0()
                    .when(id == self.tabs.active.id, |d| d.bg(cx.theme().secondary))
                    .child(
                        Button::new(("tab-select", id.0))
                            .debug_selector(move || format!("tab-select-{}", id.0))
                            .ghost()
                            .label(format!(
                                "{}{}",
                                if tab.dirty_cached { "• " } else { "" },
                                title
                            ))
                            .tooltip(tooltip)
                            .disabled(busy)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.select_tab(id, window, cx)
                            })),
                    )
                    .child(
                        Button::new(("tab-close", id.0))
                            .debug_selector(move || format!("tab-close-{}", id.0))
                            .ghost()
                            .small()
                            .icon(IconName::Close)
                            .tooltip("Close tab")
                            .disabled(busy)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.close_tab(id, window, cx)
                            })),
                    )
            }))
            .child(
                Button::new("tab-add")
                    .debug_selector(|| "tab-add".into())
                    .ghost()
                    .icon(IconName::Plus)
                    .tooltip("New tab")
                    .disabled(busy)
                    .on_click(cx.listener(|this, _, window, cx| this.new_tab(window, cx))),
            )
    }
}

impl Notes {
    pub(crate) fn close_tab(&mut self, id: TabId, window: &mut Window, cx: &mut Context<Self>) {
        if self.document_interaction_blocked()
            || window.has_active_dialog(cx)
            || self.tabs.get(id).is_none()
        {
            return;
        }
        self.begin_workspace(WorkspaceKind::CloseTab(id), vec![id], window, cx);
    }
    pub(crate) fn begin_workspace(
        &mut self,
        kind: WorkspaceKind,
        ids: Vec<TabId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.workspace.is_some() {
            return;
        }
        self.workspace = Some(WorkspaceOperation {
            kind,
            queue: {
                let mut documents = std::collections::HashSet::new();
                ids.into_iter()
                    .filter(|id| {
                        self.tabs
                            .get(*id)
                            .and_then(Tab::document)
                            .is_none_or(|doc| documents.insert(doc.id))
                    })
                    .collect()
            },
            current: None,
        });
        for id in self.tabs.ids() {
            self.schedule_tab_autosave(id, window, cx);
        }
        self.advance_workspace(window, cx);
        self.refresh(window, cx);
    }
    pub(crate) fn cancel_workspace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(workspace) = self.workspace.take() else {
            return;
        };
        if let Some(id) = workspace.current
            && let Some(tab) = self.tabs.get_mut(id)
            && let Some((operation, _)) = tab.session.foreground()
        {
            let observed = tab.observed(cx);
            let _ = tab.session.cancel(operation, observed);
        }
        for id in self.tabs.ids() {
            self.schedule_tab_autosave(id, window, cx);
        }
        self.refresh(window, cx);
    }
    fn prompt_prepared_tab(
        &mut self,
        id: TabId,
        operation: OperationId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let path = self
            .tabs
            .get(id)
            .and_then(Tab::path)
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        let prompt = window.prompt(
            PromptLevel::Warning,
            "Save changes?",
            Some(&path),
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
                this.answer_prepared_tab(id, operation, answer, window, cx)
            });
        })
        .detach();
    }
    pub(crate) fn answer_prepared_tab(
        &mut self,
        id: TabId,
        operation: OperationId,
        answer: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .workspace
            .as_ref()
            .is_none_or(|w| w.current != Some(id))
        {
            return;
        }
        let Some(tab) = self.tabs.get_mut(id) else {
            return;
        };
        let choice = match answer {
            Some(0) => app::session::Choice::Save,
            Some(1) => app::session::Choice::Discard,
            _ => app::session::Choice::Cancel,
        };
        let observed = tab.observed(cx);
        let effects = tab.session.answer(operation, choice, observed);
        self.apply_tab_effects(id, effects, window, cx);
    }
    fn advance_workspace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(workspace) = &mut self.workspace else {
            return;
        };
        if workspace.current.is_some() {
            return;
        }
        if let Some(id) = workspace.queue.pop_front() {
            workspace.current = Some(id);
            let retained = self
                .tabs
                .iter()
                .filter_map(|tab| tab.document())
                .filter(|doc| self.document_tabs(doc.id).len() > 1)
                .map(|doc| doc.id)
                .collect::<std::collections::HashSet<_>>();
            let tab = self.tabs.get_mut(id).unwrap();
            let observed = tab.observed(cx);
            let retaining = match &self.workspace.as_ref().unwrap().kind {
                WorkspaceKind::Move { .. } => true,
                WorkspaceKind::CloseTab(_) => {
                    tab.document().is_some_and(|doc| retained.contains(&doc.id))
                }
                WorkspaceKind::Trash { target } => tab
                    .path()
                    .is_none_or(|p| !p.starts_with(target.root.join(&target.path))),
                _ => false,
            };
            let effects = if retaining {
                tab.session.request_retaining(Next::Prepared, observed)
            } else {
                tab.session.request(Next::Prepared, observed)
            };
            self.apply_tab_effects(id, effects, window, cx);
            return;
        }
        match workspace.kind.clone() {
            WorkspaceKind::CloseTab(id) => {
                let previous = self.tabs.get(id).and_then(Tab::path).map(Path::to_path_buf);
                if self.tabs.order.len() == 1 {
                    self.tabs.add(window, cx);
                }
                if self.tabs.active.id == id {
                    let index = self.tabs.order.iter().position(|t| *t == id).unwrap();
                    let other = if index > 0 {
                        self.tabs.order[index - 1]
                    } else {
                        self.tabs.order[1]
                    };
                    self.tabs.select(other);
                }
                self.tabs.inactive.retain(|tab| tab.id != id);
                self.tabs.order.retain(|t| *t != id);
                self.reconcile_documents();
                self.cancel_workspace(window, cx);
                self.search_document_installed(previous.as_deref(), window, cx);
                self.focus_active_tab(window, cx);
            }
            WorkspaceKind::Close => {
                let observed = self.observed_document(cx);
                let effects = self
                    .tabs
                    .active
                    .session
                    .request_retaining(Next::Close, observed);
                self.apply_session_effects(effects, window, cx);
            }
            WorkspaceKind::Move { from, parent, name } => {
                self.move_prepared_entry(from, parent, name, window, cx)
            }
            WorkspaceKind::Trash { target } => self.trash_prepared_entry(target, window, cx),
        }
    }
}
