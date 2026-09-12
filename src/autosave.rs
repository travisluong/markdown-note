use super::*;
use std::time::Duration;

pub(super) enum Pending {
    Request(OperationId, Next),
    Save(OperationId, Next),
}

#[derive(Default)]
pub(super) struct SaveState {
    pub timer: Option<Task<()>>,
    pub writing: bool,
    pub pending: Option<Pending>,
    pub identity: u64,
    pub error: Option<String>,
    /// A disk conflict requires explicit Save or a newly opened baseline.
    pub conflict: bool,
    pub session: notebook::Autosave,
}

impl Notes {
    pub(super) fn record_save_error(&mut self, error: document::SaveError) {
        self.saves.conflict |= matches!(error, document::SaveError::Conflict(_));
        if self.saves.conflict {
            self.saves.timer = None;
        }
        let recovery = if self.saves.conflict {
            "Your changes are still in the editor. Copy them before reopening the file to resolve the conflict."
        } else {
            "Your changes are still in the editor. Edit again or use Save to retry."
        };
        self.saves.error = Some(format!("Could not save file: {error}. {recovery}"));
    }

    fn notebook_autosave(&self) -> bool {
        self.notebooks.active.as_ref().is_some_and(|book| {
            self.document
                .as_ref()
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
        if self.notebook_autosave()
            && let Some(book) = &mut self.notebooks.active
        {
            book.settings.autosave = settings;
            self.persist_notebooks(cx);
        } else {
            self.saves.session = settings;
        }
        self.schedule_autosave(window, cx);
        cx.notify();
    }

    pub(super) fn schedule_autosave(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.saves.timer = None;
        let settings = self.autosave_settings();
        if !settings.enabled || self.is_busy() || self.document.is_none() || self.saves.conflict {
            return;
        }
        let identity = self.saves.identity;
        let delay = cx
            .background_executor()
            .timer(Duration::from_millis(settings.delay_ms));
        self.saves.timer = Some(cx.spawn_in(window, async move |this, cx| {
            delay.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.saves.identity == identity
                    && !this.saves.writing
                    && !this.saves.conflict
                    && !this.is_busy()
                    && this.autosave_settings().enabled
                    && this.dirty(cx)
                {
                    this.start_write(None, window, cx);
                }
            });
        }));
    }

    pub(super) fn save(&mut self, next: Next, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_busy() || self.document.is_none() {
            return;
        }
        self.saves.timer = None;
        if self.saves.writing {
            let Some(operation) = self.foreground.begin() else {
                return;
            };
            self.saves.pending = Some(Pending::Save(operation, next));
            cx.notify();
            return;
        }
        self.start_write(Some(next), window, cx);
    }

    pub(super) fn start_write(
        &mut self,
        next: Option<Next>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        debug_assert!(!self.saves.writing);
        if self.is_busy() || (next.is_none() && self.saves.conflict) {
            return;
        }
        let Some(document) = &self.document else {
            return;
        };
        let identity = self.saves.identity;
        let (path, format, version) = (
            document.path.clone(),
            document.format,
            document.version.clone(),
        );
        let snapshot = self.editor.read(cx).snapshot();
        let operation = if next.is_some() {
            let Some(operation) = self.foreground.begin() else {
                return;
            };
            Some(operation)
        } else {
            None
        };
        self.saves.writing = true;
        self.saves.error = None;
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let text = snapshot.to_string();
                    document::save_file(&path, &text, format, &version)
                        .map(|receipt| (path, text, snapshot, receipt))
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.saves.writing = false;
                let success = result.is_ok();
                let same_document = this.saves.identity == identity;
                let pending = this.saves.pending.take();
                let owner = match &pending {
                    Some(Pending::Request(owner, _) | Pending::Save(owner, _)) => Some(*owner),
                    None => operation,
                };
                let can_continue = owner.is_none_or(|owner| this.foreground.finish(owner));
                // The write owns its captured baseline independently of any
                // foreground continuation. Never release an unrelated picker/load.
                if same_document {
                    match result {
                        Ok((path, text, snapshot, receipt)) => {
                            this.saves.conflict = false;
                            if let Some(document) = &mut this.document
                                && document.path == path
                            {
                                document.saved = text;
                                document.saved_rope = snapshot;
                                document.version = receipt.version;
                            }
                            this.check_dirty(window, cx);
                            if this
                                .notebooks
                                .active
                                .as_ref()
                                .is_some_and(|book| path.starts_with(&book.root))
                            {
                                this.refresh_sorted_tree(window, cx);
                            }
                        }
                        Err(error) => this.record_save_error(error),
                    }
                }
                if can_continue && same_document {
                    if let Some(pending) = pending {
                        match pending {
                            Pending::Request(_, next) => this.request(next, window, cx),
                            Pending::Save(_, next) => this.save(next, window, cx),
                        }
                    } else if success {
                        if let Some(next) = next {
                            this.continue_with(next, window, cx);
                        }
                    } else if !this.is_busy() {
                        this.finish_search_navigation(false, window, cx);
                    }
                }
                if (success || !same_document) && this.dirty(cx) {
                    this.schedule_autosave(window, cx);
                }
                this.refresh(window, cx);
            });
        })
        .detach();
        cx.notify();
    }
}

#[cfg(feature = "editor-diagnostics")]
pub(super) mod diagnostics;
