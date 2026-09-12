use super::*;
use app::session::{AutosaveTicket, Effect, SaveFailure, WriteTicket};
use std::time::Duration;

/// Executor handle and user-facing configuration/messages only. Session owns
/// the active write, queued action, identity and automatic retry policy.
#[derive(Default)]
pub(super) struct SaveState {
    pub timer: Option<Task<()>>,
    pub error: Option<String>,
    pub session: notebook::Autosave,
}

impl Notes {
    pub(super) fn record_save_error(&mut self, error: document::SaveError) {
        let recovery = if self.session.save_conflicted() {
            "Your changes are still in the editor. Copy them before reopening the file to resolve the conflict."
        } else {
            "Your changes are still in the editor. Edit again or use Save to retry."
        };
        self.saves.error = Some(format!("Could not save file: {error}. {recovery}"));
    }
    fn notebook_autosave(&self) -> bool {
        self.notebooks.active.as_ref().is_some_and(|book| {
            self.document()
                .is_none_or(|doc| doc.path.starts_with(&book.root))
        })
    }
    pub(super) fn autosave_settings(&self) -> notebook::Autosave {
        self.notebooks
            .active
            .as_ref()
            .filter(|_| self.notebook_autosave())
            .map_or(self.saves.session, |book| book.settings.autosave)
            .normalized()
    }

    pub(super) fn update_autosave(
        &mut self,
        settings: notebook::Autosave,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let notebook_scoped = self.notebook_autosave();
        if notebook_scoped {
            self.record_activation_preference(notebook_ui::PreferenceIntent::Autosave(settings));
        }
        if notebook_scoped && let Some(book) = &mut self.notebooks.active {
            book.settings.autosave = settings;
            self.persist_notebooks(cx);
        } else {
            self.saves.session = settings;
        }
        self.schedule_autosave(window, cx);
        cx.notify();
    }

    /// Rearm for retained content or a preference change. Edit events use
    /// Session::edit_notice directly, preserving the asynchronous dirty display.
    pub(super) fn schedule_autosave(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let enabled = self.autosave_settings().enabled;
        let observed = self.observed_document(cx);
        let effects = self.session.set_autosave(enabled, observed);
        self.apply_session_effects(effects, window, cx);
    }

    pub(crate) fn arm_session_autosave(
        &mut self,
        ticket: AutosaveTicket,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.session.autosave() != Some(ticket) {
            return;
        }
        let delay = cx
            .background_executor()
            .timer(Duration::from_millis(self.autosave_settings().delay_ms));
        self.saves.timer = Some(cx.spawn_in(window, async move |this, cx| {
            delay.await;
            let _ = this.update_in(cx, |this, window, cx| {
                let observed = this.observed_document(cx);
                let effects = this.session.autosave_due(ticket, observed);
                this.apply_session_effects(effects, window, cx);
            });
        }));
    }

    pub(crate) fn write_session_snapshot(
        &mut self,
        ticket: WriteTicket,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let document = self
            .document()
            .expect("Session only writes an active document");
        debug_assert_eq!(self.document_id(), Some(ticket.document));
        let (path, format, version) = (
            document.path.clone(),
            document.format,
            document.version.clone(),
        );
        let snapshot = self.editor.read(cx).snapshot();
        self.saves.error = None;
        self.saves.timer = None;
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let text = snapshot.to_string();
                    document::save_file(&path, &text, format, &version)
                        .map(|receipt| (text, snapshot, receipt))
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                let effects = match result {
                    Ok((text, snapshot, receipt)) => {
                        let current = this.editor.read(cx).snapshot();
                        this.session.write_succeeded(ticket, |document| {
                            document.saved = text;
                            document.saved_rope = snapshot;
                            document.version = receipt.version;
                            document.saved_rope != current
                        })
                    }
                    Err(error) => {
                        let failure = if matches!(error, document::SaveError::Conflict(_)) {
                            SaveFailure::Conflict
                        } else {
                            SaveFailure::Io
                        };
                        let observed = this.observed_document(cx);
                        let effects = this.session.write_failed(ticket, failure, observed);
                        if effects.iter().any(
                            |effect| matches!(effect, Effect::WriteFailed(id) if *id == ticket),
                        ) {
                            this.record_save_error(error);
                        }
                        effects
                    }
                };
                this.apply_session_effects(effects, window, cx);
            });
        })
        .detach();
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
        let effects = self.session.set_autosave(true, observed);
        self.apply_session_effects(effects, window, cx);
        if let Some(ticket) = self.session.autosave() {
            let effects = self.session.autosave_due(ticket, observed);
            self.apply_session_effects(effects, window, cx);
        }
        let enabled = self.autosave_settings().enabled;
        let effects = self.session.set_autosave(enabled, observed);
        self.apply_session_effects(effects, window, cx);
    }
}

#[cfg(feature = "editor-diagnostics")]
pub(super) mod diagnostics;
