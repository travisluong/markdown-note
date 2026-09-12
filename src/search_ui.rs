//! Application adapter for notebook search. The panel owns query, workers,
//! selection, and rendering; this layer supplies captured editor text and handles
//! approved navigation intents through the document/session coordinator.
mod panel;
pub(super) use panel::SearchPanel;
use panel::{Overlay, SearchIntent};

#[cfg(feature = "editor-diagnostics")]
pub(super) use panel::diagnostics::diagnostic_search;

use crate::{Next, Notes, ViewMode, search};
use gpui::{App, AppContext, Context, Entity, Window};
use gpui_component::WindowExt;
use std::path::Path;

impl Notes {
    pub(super) fn search_current_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.document().is_none() || self.is_busy() || window.has_active_dialog(cx) {
            return;
        }
        if self.mode == ViewMode::Reading {
            self.open_reading_search(window, cx);
        } else {
            self.editor
                .update(cx, |editor, cx| editor.open_search(window, cx));
        }
    }

    fn search_overlay(&self, cx: &App) -> Option<Overlay> {
        self.document().map(|document| Overlay {
            path: document.path.clone(),
            source: self.editor.read(cx).snapshot(),
        })
    }

    pub(super) fn search_visible(&self, cx: &App) -> bool {
        self.search
            .as_ref()
            .is_some_and(|panel| panel.read(cx).visible())
    }

    pub(super) fn hide_search(&mut self, cx: &mut Context<Self>) {
        if let Some(panel) = &self.search {
            panel.update(cx, |panel, cx| panel.hide(cx));
        }
    }

    pub(super) fn open_search_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_busy() || window.has_active_dialog(cx) {
            return;
        }
        let Some(book) = &self.notebooks.active else {
            self.notebooks.notice = Some("Open a notebook to search all Markdown files.".into());
            cx.notify();
            return;
        };
        let root = book.root.clone();
        self.reveal_sidebar(cx);
        if self.search.is_none() {
            let overlay = self.search_overlay(cx);
            let panel = cx.new(|cx| SearchPanel::new(root, overlay, window, cx));
            self.search_subscription = Some(cx.subscribe_in(
                &panel,
                window,
                |this, panel, intent, window, cx| {
                    if this.search.as_ref() != Some(panel) {
                        return;
                    }
                    match intent {
                        SearchIntent::Open(destination) => {
                            if this.is_busy() || window.has_active_dialog(cx) {
                                panel.update(cx, |panel, _| panel.reject_open(destination));
                                return;
                            }
                            if !panel.update(cx, |panel, cx| panel.accept_open(destination, cx)) {
                                return;
                            }
                            if this
                                .document()
                                .is_some_and(|doc| doc.path == destination.path)
                            {
                                this.reveal_search_result(destination.clone(), window, cx);
                                this.finish_search_navigation(true, window, cx);
                            } else {
                                this.request(Next::SearchResult(destination.clone()), window, cx);
                            }
                        }
                        SearchIntent::FocusEditor => {
                            this.editor
                                .update(cx, |editor, cx| editor.focus(window, cx));
                        }
                    }
                },
            ));
            self.search = Some(panel);
        }
        self.search
            .as_ref()
            .unwrap()
            .update(cx, |panel, cx| panel.show(window, cx));
        cx.notify();
    }

    /// Every install updates the next scan's overlay. Only an explicitly
    /// discarded captured overlay invalidates the existing presentation.
    pub(super) fn search_document_installed(
        &mut self,
        discarded: Option<&Path>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let overlay = self.search_overlay(cx);
        if let Some(panel) = &self.search {
            panel.update(cx, |panel, cx| {
                panel.document_installed(overlay, discarded, cx)
            });
        }
    }

    pub(super) fn schedule_search(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let overlay = self.search_overlay(cx);
        if let Some(panel) = &self.search {
            panel.update(cx, |panel, cx| panel.rescan(overlay, cx));
        }
    }

    pub(super) fn finish_search_navigation(
        &mut self,
        success: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(panel) = &self.search {
            panel.update(cx, |panel, cx| {
                panel.navigation_finished(success, window, cx)
            });
        }
    }

    pub(super) fn reveal_search_result(
        &mut self,
        destination: search::Destination,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self
            .document()
            .is_some_and(|doc| doc.path == destination.path)
        {
            return;
        }
        let text = self.editor.read(cx).value();
        let Some(range) = search::revalidate(&text, &destination) else {
            self.notebooks.notice = Some("The search match no longer exists in this file.".into());
            cx.notify();
            return;
        };
        if self.mode == ViewMode::Reading {
            self.reveal_reading_match(
                destination.query,
                destination.case_sensitive,
                range,
                window,
                cx,
            );
            return;
        }
        self.editor.update(cx, |editor, cx| {
            editor.select_source_range(range, window, cx);
        });
        cx.notify();
    }

    pub(super) fn render_search_panel(&self, _cx: &mut Context<Self>) -> Entity<SearchPanel> {
        self.search.as_ref().unwrap().clone()
    }
}
