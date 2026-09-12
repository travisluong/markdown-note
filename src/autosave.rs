use super::*;
use std::time::Duration;

pub(super) enum Pending {
    Request(Next),
    Save(Next),
}

#[derive(Default)]
pub(super) struct SaveState {
    pub timer: Option<Task<()>>,
    pub writing: bool,
    pub pending: Option<Pending>,
    pub identity: u64,
    pub error: Option<String>,
    pub session: notebook::Autosave,
}

impl Notes {
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
        if !settings.enabled || self.busy || self.document.is_none() {
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
                    && !this.busy
                    && this.autosave_settings().enabled
                    && this.dirty(cx)
                {
                    this.start_write(None, window, cx);
                }
            });
        }));
    }

    pub(super) fn save(&mut self, next: Next, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || self.document.is_none() {
            return;
        }
        self.saves.timer = None;
        if self.saves.writing {
            self.saves.pending = Some(Pending::Save(next));
            self.busy = true;
            cx.notify();
            return;
        }
        self.start_write(Some(next), window, cx);
    }

    fn start_write(&mut self, next: Option<Next>, window: &mut Window, cx: &mut Context<Self>) {
        debug_assert!(!self.saves.writing);
        let Some(document) = &self.document else {
            return;
        };
        let identity = self.saves.identity;
        let (path, bom, newline) = (document.path.clone(), document.bom, document.newline);
        let snapshot = self.editor.read(cx).snapshot();
        self.saves.writing = true;
        self.saves.error = None;
        self.busy = next.is_some();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx.background_executor().spawn(async move {
                let text = snapshot.to_string();
                save_file(&path, &text, bom, newline).map(|()| (path, text, snapshot))
            }).await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.saves.writing = false;
                this.busy = false;
                let success = result.is_ok();
                if this.saves.identity == identity {
                    match result {
                        Ok((path, text, snapshot)) => {
                            if let Some(document) = &mut this.document {
                                if document.path == path {
                                    document.saved = text;
                                    document.saved_rope = snapshot;
                                }
                            }
                            this.check_dirty(window, cx);
                            if this.notebooks.active.as_ref().is_some_and(|book| path.starts_with(&book.root)) {
                                this.refresh_sorted_tree(window, cx);
                            }
                        }
                        Err(error) => this.saves.error = Some(format!("Could not save file: {error}. Your changes are still in the editor. Edit again or use Save to retry.")),
                    }
                    if let Some(pending) = this.saves.pending.take() {
                        match pending {
                            Pending::Request(next) => this.request(next, window, cx),
                            Pending::Save(next) => this.save(next, window, cx),
                        }
                    } else if success {
                        if let Some(next) = next { this.continue_with(next, window, cx); }
                        if this.dirty(cx) { this.schedule_autosave(window, cx); }
                    } else {
                        this.finish_search_navigation(false, window, cx);
                    }
                }
                this.refresh(window, cx);
            });
        }).detach();
        cx.notify();
    }
}

#[cfg(feature = "editor-diagnostics")]
pub(super) mod diagnostics;
