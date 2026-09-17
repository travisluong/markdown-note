use super::*;
use app::session::{AutosaveTicket, SaveFailure, WriteTicket};
use std::time::Duration;

/// Executor handle and user-facing configuration/messages only. Session owns
/// the active write, queued action, identity and automatic retry policy.
#[derive(Default)]
pub(super) struct SaveState {
    pub timer: Option<Task<()>>,
    pub error: Option<String>,
}

impl Notes {
    pub(crate) fn autosave_root(&self) -> Option<PathBuf> {
        self.tabs
            .active
            .notebook
            .as_ref()
            .map(|(root, _)| root.clone())
            .or_else(|| {
                self.notebooks
                    .active
                    .as_ref()
                    .filter(|book| {
                        self.tabs
                            .active
                            .path()
                            .is_none_or(|path| path.starts_with(&book.root))
                    })
                    .map(|book| book.root.clone())
            })
    }
    pub(super) fn autosave_settings(&self) -> notebook::Autosave {
        let mut settings = self.scoped_autosave_settings(self.tabs.active.id);
        if self.autosave_root().is_none() {
            settings.enabled = self.external_autosave_enabled();
        }
        settings
    }
    pub(crate) fn tab_autosave_settings(&self, id: tabs::TabId) -> notebook::Autosave {
        let mut settings = self.scoped_autosave_settings(id);
        let root = self
            .notebooks
            .active
            .as_ref()
            .map(|book| book.root.as_path());
        if self.tabs.get(id).expect("live tab").is_external_to(root) {
            settings.enabled = self.external_autosave_enabled();
        }
        settings
    }
    fn scoped_autosave_settings(&self, id: tabs::TabId) -> notebook::Autosave {
        let tab = self.tabs.get(id).expect("live tab");
        tab.notebook
            .as_ref()
            .map_or_else(
                || {
                    self.notebooks
                        .active
                        .as_ref()
                        .filter(|book| tab.path().is_none_or(|p| p.starts_with(&book.root)))
                        .map_or(self.session_autosave, |book| book.settings.autosave)
                },
                |(root, cached)| {
                    self.notebooks
                        .active
                        .as_ref()
                        .filter(|book| &book.root == root)
                        .map_or(*cached, |book| book.settings.autosave)
                },
            )
            .normalized()
    }
    pub(super) fn update_autosave(
        &mut self,
        settings: notebook::Autosave,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let root = self.autosave_root();
        if let Some(root) = &root {
            if self
                .notebooks
                .active
                .as_ref()
                .is_some_and(|book| &book.root == root)
            {
                self.record_activation_preference(notebook_ui::PreferenceIntent::Autosave(
                    settings,
                ));
                self.notebooks.active.as_mut().unwrap().settings.autosave = settings;
                self.persist_notebooks(cx);
            } else if let Some(book) = self.notebook_cache.get_mut(root) {
                book.settings.autosave = settings;
                let book = book.clone();
                self.persist_cached_notebook(book, cx);
            }
            for id in self.tabs.ids() {
                if let Some((tab_root, cached)) = &mut self.tabs.get_mut(id).unwrap().notebook
                    && tab_root == root
                {
                    *cached = settings;
                }
            }
        } else {
            self.session_autosave.delay_ms = settings.delay_ms;
            self.set_external_autosave_enabled(settings.enabled, cx);
        }
        for id in self.tabs.ids() {
            self.schedule_tab_autosave(id, window, cx);
        }
        cx.notify();
    }

    pub(super) fn update_external_autosave(
        &mut self,
        enabled: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let changed = self.external_autosave_enabled() != enabled;
        self.set_external_autosave_enabled(enabled, cx);
        if !changed {
            return;
        }
        for id in self.tabs.ids() {
            self.schedule_tab_autosave(id, window, cx);
        }
        cx.notify();
    }
    /// Rearm for retained content or a preference change. Edit events use
    /// Session::edit_notice directly, preserving the asynchronous dirty display.
    #[cfg(any(test, feature = "editor-diagnostics"))]
    pub(super) fn schedule_autosave(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.schedule_tab_autosave(self.tabs.active.id, window, cx);
    }
    pub(crate) fn schedule_tab_autosave(
        &mut self,
        id: tabs::TabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let enabled = self.workspace.is_none() && self.tab_autosave_settings(id).enabled;
        let tab = self.tabs.get_mut(id).unwrap();
        let observed = tab.observed(cx);
        let effects = tab.session.set_autosave(enabled, observed);
        self.apply_tab_effects(id, effects, window, cx);
    }
    pub(crate) fn arm_session_autosave(
        &mut self,
        ticket: AutosaveTicket,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.arm_tab_autosave(self.tabs.active.id, ticket, window, cx);
    }
    pub(crate) fn arm_tab_autosave(
        &mut self,
        id: tabs::TabId,
        ticket: AutosaveTicket,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.workspace.is_some()
            || self.document_write_blocked(id)
            || self
                .tabs
                .get(id)
                .is_none_or(|tab| tab.session.autosave() != Some(ticket))
        {
            return;
        }
        let document_id = self
            .tabs
            .get(id)
            .and_then(|tab| tab.document())
            .map(|doc| doc.id);
        let delay = cx.background_executor().timer(Duration::from_millis(
            self.tab_autosave_settings(id).delay_ms,
        ));
        self.save_state_mut(id).timer = Some(cx.spawn_in(window, async move |this, cx| {
            delay.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.workspace.is_some() {
                    return;
                }
                let id = match document_id {
                    Some(document) => {
                        let Some(id) = this.document_tabs(document).first().copied() else {
                            return;
                        };
                        id
                    }
                    None => id,
                };
                if this.document_write_blocked(id) {
                    return;
                }
                let Some(tab) = this.tabs.get_mut(id) else {
                    return;
                };
                let observed = tab.observed(cx);
                let effects = tab.session.autosave_due(ticket, observed);
                this.apply_tab_effects(id, effects, window, cx);
            });
        }));
    }
    pub(crate) fn write_session_snapshot(
        &mut self,
        ticket: WriteTicket,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.write_tab_snapshot(self.tabs.active.id, ticket, window, cx);
    }
    pub(crate) fn write_tab_snapshot(
        &mut self,
        id: tabs::TabId,
        ticket: WriteTicket,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(tab) = self.tabs.get(id) else {
            return;
        };
        if tab.session.write() != Some(ticket) {
            return;
        }
        let document = tab.document().expect("Session writes document").clone();
        let path = document.path.clone();
        let format = document.format();
        let version = document.version();
        let snapshot = tab.editor.snapshot(cx);
        self.save_state_mut(id).error = None;
        self.save_state_mut(id).timer = None;
        cx.spawn_in(window, async move |this, cx| {
            let captured_path = path.clone();
            let result = cx.background_executor().spawn(async move {
                let text = snapshot.to_string();
                document::save_file(&path, &text, format, &version).map(|receipt| (snapshot, receipt))
            }).await;
            let _ = this.update_in(cx, |this, window, cx| {
                let ids = this.document_tabs(document.id);
                let Some(&owner) = ids.first() else { return; };
                let tab = this.tabs.get_mut(owner).unwrap();
                if tab.path() != Some(captured_path.as_path()) || tab.session.write() != Some(ticket) { return; }
                let success = result.is_ok();
                let mut error_message = None;
                let effects = match result {
                    Ok((snapshot, receipt)) => {
                        let current = tab.editor.snapshot(cx);
                        tab.session.write_succeeded(ticket, |document| {
                            document.accept_save(snapshot, receipt.version);
                            document.saved_rope() != current
                        })
                    }
                    Err(error) => {
                        let failure = if matches!(error, document::SaveError::Conflict(_)) { SaveFailure::Conflict } else { SaveFailure::Io };
                        let observed = tab.observed(cx);
                        let effects = tab.session.write_failed(ticket, failure, observed);
                        let recovery = if tab.session.save_conflicted() { "Copy your changes before reopening to resolve the conflict." } else { "Edit again or use Save to retry." };
                        error_message = Some(format!("Could not save file: {error}. Your changes are still in the editor. {recovery}"));
                        effects
                    }
                };
                let mut completions = vec![(owner, effects)];
                for &id in ids.iter().skip(1) {
                    let tab = this.tabs.get_mut(id).unwrap();
                    let observed = tab.observed(cx);
                    completions.push((id, tab.session.shared_write_finished(ticket, success, observed)));
                }
                this.save_state_mut(owner).error = error_message;
                for (id, effects) in completions { this.apply_tab_effects(id, effects, window, cx); }
                if let Some(id) = this.document_tabs(document.id).first().copied() {
                    this.check_tab_dirty(id, window, cx);
                }
            });
        }).detach();
    }

    pub(crate) fn saved_session_baseline(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.check_dirty(window, cx);
        if self.document().is_some_and(|document| {
            self.notebooks
                .active
                .as_ref()
                .is_some_and(|book| document.path.starts_with(&book.root))
        }) {
            self.refresh_sorted_tree(window, cx);
        }
    }

    /// Headless fixtures can drive the actual debounce event without advancing
    /// time; this does not create a second write/continuation authority.
    #[cfg(any(test, feature = "editor-diagnostics"))]
    pub(super) fn start_write(
        &mut self,
        next: Option<Next>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(next) = next {
            self.save(next, window, cx);
            return;
        }
        let observed = self.observed_document(cx);
        let effects = self.tabs.active.session.set_autosave(true, observed);
        self.apply_session_effects(effects, window, cx);
        if let Some(ticket) = self.tabs.active.session.autosave() {
            let effects = self.tabs.active.session.autosave_due(ticket, observed);
            self.apply_session_effects(effects, window, cx);
        }
        let enabled = self.autosave_settings().enabled;
        let effects = self.tabs.active.session.set_autosave(enabled, observed);
        self.apply_session_effects(effects, window, cx);
    }
}

#[cfg(feature = "editor-diagnostics")]
pub(super) mod diagnostics;
