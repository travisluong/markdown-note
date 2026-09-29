//! Footer presentation and bounded, shared-document count work.
use crate::{Notes, notebook};
use gpui::{Context, IntoElement, Task, Window, div, prelude::*};
use gpui_component::{ActiveTheme, input::Rope};
use markdown_note::document_counts::{self, DocumentCounts};
use notebook::FooterCountMode;
use std::time::Duration;

#[derive(Clone, Copy, PartialEq, Eq)]
struct CountKey {
    revision: u64,
    markdown: bool,
    readable: bool,
}

struct CountRequest {
    key: CountKey,
    snapshot: Rope,
    debounce: bool,
}

#[derive(Clone, Copy)]
struct CachedCounts {
    markdown: bool,
    counts: DocumentCounts,
}

#[derive(Default)]
pub(crate) struct CountState {
    revision: u64,
    readable_requested: bool,
    requested: Option<CountKey>,
    source: Option<CachedCounts>,
    readable: Option<CachedCounts>,
    pending: Option<CountRequest>,
    task: Option<Task<()>>,
    #[cfg(test)]
    pub completion_delay: Duration,
    #[cfg(test)]
    pub jobs_started: usize,
}

impl Notes {
    pub(crate) fn active_document_counts(&self, mode: FooterCountMode) -> Option<DocumentCounts> {
        let document = self.document()?;
        let state = &self.documents.get(&document.path)?.counts;
        let cached = match mode {
            FooterCountMode::Source => state.source,
            FooterCountMode::ReadableText => state.readable,
        }?;
        (cached.markdown == notebook::markdown(&document.path)).then_some(cached.counts)
    }

    pub(crate) fn document_counts_edited(
        &mut self,
        document: u64,
        snapshot: Rope,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(runtime) = self
            .documents
            .values_mut()
            .find(|r| r.document.id == document)
        else {
            return;
        };
        runtime.counts.revision += 1;
        self.queue_document_counts(document, snapshot, true, window, cx);
    }

    /// Synchronize cheap cache keys only. Source materialization and parsing stay
    /// on the worker; ordinary rerenders never rescan or copy document text.
    pub(crate) fn sync_footer_counts(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((id, path)) = self
            .document()
            .map(|document| (document.id, document.path.clone()))
        else {
            return;
        };
        let markdown = notebook::markdown(&path);
        let readable = self.footer_settings().count_mode == FooterCountMode::ReadableText;
        let Some(state) = self.documents.get_mut(&path).map(|r| &mut r.counts) else {
            return;
        };
        state.readable_requested |= readable;
        let key = CountKey {
            revision: state.revision,
            markdown,
            readable: state.readable_requested,
        };
        if state.requested == Some(key) {
            return;
        }
        self.queue_document_counts(id, self.tabs.active.editor.snapshot(cx), false, window, cx);
    }

    pub(crate) fn queue_document_counts(
        &mut self,
        document: u64,
        snapshot: Rope,
        debounce: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(runtime) = self
            .documents
            .values_mut()
            .find(|r| r.document.id == document)
        else {
            return;
        };
        let state = &mut runtime.counts;
        let key = CountKey {
            revision: state.revision,
            markdown: notebook::markdown(&runtime.document.path),
            readable: state.readable_requested,
        };
        if state.requested == Some(key) {
            return;
        }
        state.requested = Some(key);
        state.pending = Some(CountRequest {
            key,
            snapshot,
            debounce,
        });
        if state.task.is_some() {
            return;
        }
        state.task = Some(cx.spawn_in(window, async move |this, cx| {
            loop {
                let debounce = this
                    .update_in(cx, |this, _, _| {
                        this.documents
                            .values()
                            .find(|r| r.document.id == document)
                            .and_then(|r| r.counts.pending.as_ref())
                            .map(|r| r.debounce)
                    })
                    .ok()
                    .flatten();
                let Some(debounce) = debounce else { return };
                if debounce {
                    cx.background_executor()
                        .timer(Duration::from_millis(100))
                        .await;
                }
                let request = this
                    .update_in(cx, |this, _, _| {
                        let state = &mut this
                            .documents
                            .values_mut()
                            .find(|r| r.document.id == document)?
                            .counts;
                        #[cfg(test)]
                        {
                            state.jobs_started += 1;
                        }
                        state.pending.take()
                    })
                    .ok()
                    .flatten();
                let Some(request) = request else { return };
                let key = request.key;
                #[cfg(test)]
                let completion_delay = this
                    .update_in(cx, |this, _, _| {
                        this.documents
                            .values_mut()
                            .find(|r| r.document.id == document)
                            .map(|r| std::mem::take(&mut r.counts.completion_delay))
                            .unwrap_or_default()
                    })
                    .unwrap_or_default();
                let (source, readable) = cx
                    .background_executor()
                    .spawn(async move {
                        let source = request.snapshot.to_string();
                        let raw =
                            document_counts::count(&source, key.markdown, FooterCountMode::Source);
                        let readable = key.readable.then(|| {
                            if key.markdown {
                                document_counts::count(&source, true, FooterCountMode::ReadableText)
                            } else {
                                raw
                            }
                        });
                        (raw, readable)
                    })
                    .await;
                #[cfg(test)]
                cx.background_executor().timer(completion_delay).await;
                let more = this
                    .update_in(cx, |this, _, cx| {
                        let Some(runtime) = this
                            .documents
                            .values_mut()
                            .find(|r| r.document.id == document)
                        else {
                            return false;
                        };
                        let state = &mut runtime.counts;
                        if state.revision == key.revision
                            && notebook::markdown(&runtime.document.path) == key.markdown
                        {
                            state.source = Some(CachedCounts {
                                markdown: key.markdown,
                                counts: source,
                            });
                            if let Some(counts) = readable {
                                state.readable = Some(CachedCounts {
                                    markdown: key.markdown,
                                    counts,
                                });
                            }
                            cx.notify();
                        }
                        let more = state.pending.is_some();
                        if !more {
                            state.task = None;
                        }
                        more
                    })
                    .unwrap_or(false);
                if !more {
                    return;
                }
            }
        }));
    }

    pub(crate) fn render_footer(&self, cx: &Context<Self>) -> impl IntoElement {
        let settings = self.footer_settings();
        let counts = self.active_document_counts(settings.count_mode);
        let has_document = self.document().is_some();
        let show_words = has_document && settings.word_count;
        let show_characters = has_document && settings.character_count;
        div()
            .id("app-footer")
            .debug_selector(|| "app-footer".into())
            .w_full()
            .min_h(crate::zoom_ui::zoom_px(28., cx))
            .flex_shrink_0()
            .flex()
            .flex_wrap()
            .items_center()
            .justify_end()
            .gap_2()
            .px_3()
            .py_1()
            .border_t_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().background)
            .text_sm()
            .text_color(cx.theme().muted_foreground)
            .when(show_words, |view| {
                view.child(
                    div()
                        .debug_selector(|| "footer-word-count".into())
                        .child(count_label(counts.map(|c| c.words), "word")),
                )
            })
            .when(show_words && show_characters, |view| view.child("·"))
            .when(show_characters, |view| {
                view.child(
                    div()
                        .debug_selector(|| "footer-character-count".into())
                        .child(count_label(counts.map(|c| c.characters), "character")),
                )
            })
    }
}

fn count_label(count: Option<usize>, noun: &str) -> String {
    match count {
        Some(1) => format!("1 {noun}"),
        Some(count) => format!("{count} {noun}s"),
        None => format!("— {noun}s"),
    }
}

#[cfg(test)]
mod tests;
#[cfg(test)]
mod worker_tests;
