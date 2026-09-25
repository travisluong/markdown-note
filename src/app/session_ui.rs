//! GPUI effects for the document session. Session owns transition policy; these
//! adapters own prompts, executor handles, captured I/O data and presentation.

use std::{
    io,
    path::{Path, PathBuf},
};

#[cfg(not(test))]
use gpui::PathPromptOptions;
use gpui::{App, Context, PromptButton, PromptLevel, Window};

use crate::{Document, Next, Notes, search};
use gpui_component::WindowExt;

use super::session::{
    Choice, Content, DocumentId, Effect, Loaded, Observed, OperationId, Rejection, Stage,
};

impl Notes {
    pub(crate) fn image_path(&self) -> Option<&Path> {
        match self.tabs.active.session.content() {
            Content::Image { value, .. } => Some(value.as_path()),
            _ => None,
        }
    }

    pub(crate) fn document_id(&self) -> Option<DocumentId> {
        self.tabs.active.session.content().document_id()
    }

    /// Use at an actual save/navigation decision, never on every edit event.
    pub(crate) fn observed_document(&self, cx: &App) -> Observed {
        Observed {
            document: self.document_id(),
            dirty: self.dirty(cx),
        }
    }

    /// Stage adapters consume effects requiring their captured I/O payload
    /// (picker purpose, load result or preference barrier) before forwarding the
    /// remaining shared effects here. No adapter acquires or releases ownership.
    pub(crate) fn apply_session_effects(
        &mut self,
        effects: Vec<Effect<Next>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for effect in effects {
            match effect {
                Effect::StateChanged => self.refresh(window, cx),
                Effect::CancelAutosave => self.save_state_mut(self.tabs.active.id).timer = None,
                Effect::ArmAutosave(ticket) => self.arm_session_autosave(ticket, window, cx),
                Effect::Prompt(operation) => self.prompt_session_save(operation, window, cx),
                Effect::Write(ticket) => self.write_session_snapshot(ticket, window, cx),
                Effect::BaselineAccepted(_) => self.saved_session_baseline(window, cx),
                Effect::WriteFailed(_) => {
                    // The write adapter attached the matching I/O error before
                    // forwarding this effect; the model owns its retry policy.
                }
                Effect::Continue { operation, action } => {
                    self.continue_operation(operation, action, window, cx);
                }
                Effect::Cancelled { operation, action } => {
                    if self
                        .tabs
                        .active
                        .navigation_pending
                        .as_ref()
                        .is_some_and(|(id, _)| *id == operation)
                    {
                        self.tabs.active.navigation_pending = None;
                    }
                    self.cancel_session_context(operation);
                    self.cancel_workspace(window, cx);
                    self.finish_search_navigation(false, window, cx);
                    if matches!(action, Some(Next::TreeFile(_))) {
                        self.restore_tree_cursor(window, cx);
                    }
                }
                Effect::Close => self.close_window(window, cx),
                Effect::Rejected(Rejection::IdentityExhausted) => {
                    self.notebooks.notice = Some(
                        "The session identity limit was reached. Keep your unsaved text and restart the application."
                            .into(),
                    );
                    cx.notify();
                }
                Effect::Rejected(Rejection::Busy | Rejection::NoDocument) => {}
                Effect::Pick(_)
                | Effect::Load(_)
                | Effect::Move(_)
                | Effect::RestoreIndex(_)
                | Effect::RestoreFailed(_)
                | Effect::ResolveActivation(_)
                | Effect::FlushActivation { .. }
                | Effect::ActivationFlushFailed(_)
                | Effect::ActivationFailed(_)
                | Effect::FlushClose { .. }
                | Effect::CloseFailed(_)
                | Effect::ShowError(_)
                | Effect::MoveFailed(_)
                | Effect::Installed { .. }
                | Effect::Moved(_)
                | Effect::MoveBaselineConflict(_) => {
                    unreachable!("the stage adapter must consume its I/O effects")
                }
            }
        }
    }

    pub(crate) fn request(&mut self, next: Next, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings_panel.is_some() && !matches!(next, Next::Close) {
            return;
        }
        if matches!(next, Next::Close) && self.workspace.is_none() {
            if !self.is_busy() {
                self.begin_workspace(
                    crate::tabs::WorkspaceKind::Close,
                    self.tabs.ids(),
                    window,
                    cx,
                );
            }
            return;
        }
        if self.workspace.is_some() {
            return;
        }
        if matches!(next, Next::Open) {
            let effects = self.tabs.active.session.pick();
            for effect in effects {
                if let Effect::Pick(operation) = effect {
                    self.show_file_picker(operation, window, cx);
                } else {
                    self.apply_session_effects(vec![effect], window, cx);
                }
            }
            return;
        }
        if let Some(path) = match &next {
            Next::File(path) | Next::TreeFile(path) | Next::FileFromPicker(path) => Some(path),
            _ => None,
        } && self.tabs.active.path()
            == Some(
                std::fs::canonicalize(path)
                    .unwrap_or_else(|_| path.clone())
                    .as_path(),
            )
        {
            return;
        }
        let observed = self.observed_document(cx);
        let effects = if matches!(next, Next::Notebook { .. } | Next::RemoveNotebook(_))
            || self.retains_active_document()
        {
            self.tabs.active.session.request_retaining(next, observed)
        } else {
            self.tabs.active.session.request(next, observed)
        };
        self.apply_session_effects(effects, window, cx);
    }

    pub(crate) fn save(&mut self, next: Next, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings_panel.is_some() {
            return;
        }
        if self.workspace.is_some() {
            return;
        }
        let next = (!matches!(next, Next::Nothing)).then_some(next);
        let effects = self.tabs.active.session.save(next);
        self.apply_session_effects(effects, window, cx);
    }

    fn prompt_session_save(
        &mut self,
        operation: OperationId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
        let tab_id = self.tabs.active.id;
        cx.spawn_in(window, async move |this, cx| {
            let answer = prompt.await.ok();
            let _ = this.update_in(cx, |this, window, cx| {
                if this.tabs.active.id != tab_id {
                    return;
                }
                this.answer_request(operation, answer, window, cx);
            });
        })
        .detach();
    }

    pub(crate) fn answer_request(
        &mut self,
        operation: OperationId,
        answer: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let choice = match answer {
            Some(0) => Choice::Save,
            Some(1) => Choice::Discard,
            _ => Choice::Cancel,
        };
        let observed = self.observed_document(cx);
        let effects = self.tabs.active.session.answer(operation, choice, observed);
        self.apply_session_effects(effects, window, cx);
    }

    fn continue_operation(
        &mut self,
        operation: OperationId,
        next: Next,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match next {
            Next::Prepared => unreachable!("workspace continuation uses tab-qualified effects"),
            Next::History(visit) => {
                if self.tabs.active.navigation.accepts(&visit) {
                    let path = visit.path.clone();
                    self.tabs.active.navigation_pending = Some((operation, visit));
                    self.open_path_with_operation(operation, path, false, window, cx);
                } else {
                    let observed = self.observed_document(cx);
                    let effects = self.tabs.active.session.cancel(operation, observed);
                    self.apply_session_effects(effects, window, cx);
                }
            }
            Next::Entry { target, action } => {
                self.entry_with_operation(operation, target, action, window, cx)
            }
            Next::Nothing => {
                let observed = self.observed_document(cx);
                let effects = self.tabs.active.session.continued(operation, observed);
                self.apply_session_effects(effects, window, cx);
            }
            Next::Open => self.open_with_operation(operation, window, cx),
            Next::Close => self.close_with_operation(operation, window, cx),
            Next::File(path) => self.open_path_with_operation(operation, path, false, window, cx),
            Next::FileFromPicker(path) => {
                self.load_file_with_operation(operation, path, false, window, cx)
            }
            Next::TreeFile(path) => {
                self.open_path_with_operation(operation, path, true, window, cx)
            }
            Next::NewNote(root) => self.create_note_with_operation(operation, root, window, cx),
            Next::RemoveNotebook(path) => {
                self.remove_notebook_with_operation(operation, path, window, cx)
            }
            Next::Notebook { path, create } => {
                self.activate_notebook_with_operation(operation, path, create, window, cx);
            }
            Next::SearchResult(destination) => {
                let search::Destination { path, .. } = &destination;
                let path = path.clone();
                self.search_destination = Some(destination);
                self.open_path_with_operation(operation, path, false, window, cx);
            }
        }
    }

    pub(crate) fn error(
        &mut self,
        message: &str,
        error: impl std::fmt::Display,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let effects = self.tabs.active.session.show_error();
        for effect in effects {
            if let Effect::ShowError(operation) = effect {
                self.show_session_error(operation, message, &error.to_string(), window, cx);
            } else {
                self.apply_session_effects(vec![effect], window, cx);
            }
        }
    }

    pub(crate) fn show_session_error(
        &mut self,
        operation: OperationId,
        title: &str,
        detail: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let prompt = window.prompt(PromptLevel::Critical, title, Some(detail), &["OK"], cx);
        let tab_id = self.tabs.active.id;
        cx.spawn_in(window, async move |this, cx| {
            let _ = prompt.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.tabs.active.id != tab_id {
                    return;
                }
                let observed = this.observed_document(cx);
                let effects = this.tabs.active.session.cancel(operation, observed);
                this.apply_session_effects(effects, window, cx);
            });
        })
        .detach();
    }

    fn open_with_operation(
        &mut self,
        operation: OperationId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let effects = self.tabs.active.session.dispatch(operation, Stage::Pick);
        for effect in effects {
            if let Effect::Pick(operation) = effect {
                self.show_file_picker(operation, window, cx);
            } else {
                self.apply_session_effects(vec![effect], window, cx);
            }
        }
    }
    // TestPlatform has no path dialog; tests capture the operation and supply
    // outcomes through the production finish_file_picker adapter.
    #[cfg(test)]
    fn show_file_picker(
        &mut self,
        operation: OperationId,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        self.file_picker_operation = Some(operation);
    }

    #[cfg(not(test))]
    fn show_file_picker(
        &mut self,
        operation: OperationId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Open".into()),
        });
        let tab_id = self.tabs.active.id;
        cx.spawn_in(window, async move |this, cx| {
            let result = match picker.await {
                Ok(Ok(paths)) => Ok(paths.and_then(|paths| paths.into_iter().next())),
                Ok(Err(error)) => Err(io::Error::other(error.to_string())),
                Err(error) => Err(io::Error::other(error.to_string())),
            };
            let _ = this.update_in(cx, |this, window, cx| {
                if this.tabs.active.id == tab_id {
                    this.finish_file_picker(operation, result, window, cx);
                }
            });
        })
        .detach();
    }

    pub(crate) fn finish_file_picker(
        &mut self,
        operation: OperationId,
        result: io::Result<Option<PathBuf>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(path) => {
                let observed = self.observed_document(cx);
                let same = path.as_ref().is_some_and(|path| {
                    self.tabs.active.path()
                        == Some(
                            std::fs::canonicalize(path)
                                .unwrap_or_else(|_| path.clone())
                                .as_path(),
                        )
                });
                let action = path.filter(|_| !same).map(Next::FileFromPicker);
                let effects = if self.retains_active_document() {
                    self.tabs
                        .active
                        .session
                        .picked_retaining(operation, action, observed)
                } else {
                    self.tabs.active.session.picked(operation, action, observed)
                };
                self.apply_session_effects(effects, window, cx);
            }
            Err(error) => {
                let effects = self.tabs.active.session.pick_failed(operation);
                for effect in effects {
                    if let Effect::ShowError(operation) = effect {
                        self.show_session_error(
                            operation,
                            "Could not open file",
                            &error.to_string(),
                            window,
                            cx,
                        );
                    } else {
                        self.apply_session_effects(vec![effect], window, cx);
                    }
                }
            }
        }
    }

    /// Capture the old path before the owned installation drops its metadata.
    /// The explicit discarded-document effect controls search overlay invalidation.
    pub(crate) fn complete_content_load(
        &mut self,
        operation: OperationId,
        result: io::Result<Loaded<Document, PathBuf>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let previous = self
            .document()
            .zip(self.document_id())
            .map(|(document, id)| (id, document.path.clone()));
        let observed = self.observed_document(cx);
        let result = result.map(|loaded| match loaded {
            Loaded::Document(document) => {
                Loaded::Document(self.retained_document(&document.path).unwrap_or(document))
            }
            other => other,
        });
        let error = result.as_ref().err().map(ToString::to_string);
        let effects = self
            .tabs
            .active
            .session
            .loaded(operation, result.map_err(|_| ()), observed);
        let mut installed = false;
        for effect in effects {
            match effect {
                Effect::Installed {
                    discarded_document, ..
                } => {
                    let discarded_path = previous
                        .as_ref()
                        .filter(|(id, _)| Some(*id) == discarded_document)
                        .map(|(_, path)| path.as_path());
                    self.record_navigation_install(operation);
                    self.install_session_editor(window, cx);
                    self.search_document_installed(discarded_path, window, cx);
                    installed = true;
                }
                Effect::ShowError(operation) => self.show_session_error(
                    operation,
                    "Could not open file",
                    error.as_deref().unwrap_or("Unknown load error"),
                    window,
                    cx,
                ),
                Effect::ActivationFailed(_) => self.notebook_open_failed(
                    io::Error::other(
                        error
                            .clone()
                            .unwrap_or_else(|| "Unknown notebook load error".into()),
                    ),
                    window,
                    cx,
                ),
                other => self.apply_session_effects(vec![other], window, cx),
            }
        }
        installed
    }

    #[cfg(any(test, feature = "editor-diagnostics"))]
    pub(crate) fn diagnostic_answer_request(
        &mut self,
        answer: Option<usize>,
        next: Next,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(next, Next::Close) && self.workspace.is_none() {
            if !self.is_busy() {
                self.begin_workspace(
                    crate::tabs::WorkspaceKind::Close,
                    self.tabs.ids(),
                    window,
                    cx,
                );
            }
            return;
        }
        if self.workspace.is_some() {
            return;
        }
        if let Some(path) = match &next {
            Next::File(path) | Next::TreeFile(path) | Next::FileFromPicker(path) => Some(path),
            _ => None,
        } && self.tabs.active.path()
            == Some(
                std::fs::canonicalize(path)
                    .unwrap_or_else(|_| path.clone())
                    .as_path(),
            )
        {
            return;
        }
        let observed = self.observed_document(cx);
        let effects = if matches!(next, Next::Notebook { .. } | Next::RemoveNotebook(_))
            || self.retains_active_document()
        {
            self.tabs.active.session.request_retaining(next, observed)
        } else {
            self.tabs.active.session.request(next, observed)
        };
        let prompt = effects.iter().find_map(|effect| {
            if let Effect::Prompt(operation) = effect {
                Some(*operation)
            } else {
                None
            }
        });
        if let Some(operation) = prompt {
            // TestPlatform does not implement native prompts. Drive the actual
            // owned decision without opening a second prompt or inventing a token.
            for effect in effects {
                if !matches!(effect, Effect::Prompt(_)) {
                    self.apply_session_effects(vec![effect], window, cx);
                }
            }
            self.answer_request(operation, answer, window, cx);
        } else {
            self.apply_session_effects(effects, window, cx);
        }
    }

    #[cfg(any(test, feature = "editor-diagnostics"))]
    pub(crate) fn install_document(
        &mut self,
        document: Option<Document>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let observed = self.observed_document(cx);
        let _ = self.tabs.active.session.request(Next::Nothing, observed);
        if let Some((operation, super::session::PhaseKind::Prompt)) =
            self.tabs.active.session.foreground()
        {
            let _ = self
                .tabs
                .active
                .session
                .answer(operation, Choice::Discard, observed);
        }
        let Some((operation, super::session::PhaseKind::Dispatching)) =
            self.tabs.active.session.foreground()
        else {
            panic!("fixture installation needs an idle session")
        };
        let _ = self.tabs.active.session.dispatch(operation, Stage::Load);
        self.complete_content_load(
            operation,
            Ok(document.map_or(Loaded::Empty, Loaded::Document)),
            window,
            cx,
        );
    }
}

impl Notes {
    pub(crate) fn navigate_history(
        &mut self,
        direction: crate::app::navigation::Direction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.document_interaction_blocked() || window.has_active_dialog(cx) {
            return;
        }
        if let Some(visit) = self.tabs.active.navigation.target(direction) {
            self.request(crate::Next::History(visit), window, cx);
        }
    }

    /// Called exclusively for accepted installations, including sidebar-created notes.
    pub(crate) fn record_navigation_install(&mut self, operation: crate::OperationId) {
        let path = self
            .document()
            .map(|doc| doc.path.clone())
            .or_else(|| self.image_path().map(ToOwned::to_owned));
        let pending = self
            .tabs
            .active
            .navigation_pending
            .take()
            .filter(|(id, _)| *id == operation);
        if let Some(path) = path {
            if let Some((_, visit)) = pending {
                self.tabs.active.navigation.commit(&visit, &path);
            } else {
                self.tabs.active.navigation.record(path);
            }
        } else {
            self.tabs.active.navigation = Default::default();
        }
    }
}
