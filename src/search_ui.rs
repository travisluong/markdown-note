use super::*;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

pub(super) struct SearchPanel {
    pub visible: bool,
    root: PathBuf,
    query: Entity<InputState>,
    _subscription: Subscription,
    case_sensitive: bool,
    hits: Vec<search::Hit>,
    selected: usize,
    status: String,
    scroll: UniformListScrollHandle,
    cancelled: Arc<AtomicBool>,
    task: Option<Task<()>>,
}

impl Drop for SearchPanel {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

impl Notes {
    pub(super) fn open_search_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || window.has_active_dialog(cx) {
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
            let query =
                cx.new(|cx| InputState::new(window, cx).placeholder("Search notebook text"));
            let subscription = cx.subscribe_in(&query, window, |this, _, event, window, cx| {
                if matches!(event, InputEvent::Change) {
                    this.schedule_search(window, cx);
                }
            });
            self.search = Some(SearchPanel {
                visible: true,
                root,
                query,
                _subscription: subscription,
                case_sensitive: false,
                hits: Vec::new(),
                selected: 0,
                status: String::new(),
                scroll: UniformListScrollHandle::new(),
                cancelled: Arc::new(AtomicBool::new(false)),
                task: None,
            });
        }
        let panel = self.search.as_mut().unwrap();
        panel.visible = true;
        panel.query.update(cx, |query, cx| query.focus(window, cx));
        cx.notify();
    }

    pub(super) fn schedule_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(panel) = self.search.as_mut() else {
            return;
        };
        panel.cancelled.store(true, Ordering::Relaxed);
        panel.task = None;
        panel.cancelled = Arc::new(AtomicBool::new(false));
        let cancelled = panel.cancelled.clone();
        let root = panel.root.clone();
        let query = panel.query.read(cx).value().to_string();
        let case_sensitive = panel.case_sensitive;
        let previous = panel
            .hits
            .get(panel.selected)
            .map(|hit| (hit.relative.clone(), hit.range.clone()));
        panel.hits.clear();
        panel.selected = 0;
        panel.scroll.scroll_to_item(0, ScrollStrategy::Top);
        if query.is_empty() {
            panel.status.clear();
            cx.notify();
            return;
        }
        panel.status = "Searching…".into();
        let buffer = self
            .document
            .as_ref()
            .map(|doc| (doc.path.clone(), self.editor.read(cx).snapshot()));
        panel.task = Some(cx.spawn_in(window, async move |this, cx| {
            Timer::after(Duration::from_millis(200)).await;
            let worker_cancel = cancelled.clone();
            let result = cx
                .background_executor()
                .spawn(async move {
                    search::scan(&root, &query, case_sensitive, buffer, &worker_cancel)
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                let Some(panel) = this.search.as_mut() else {
                    return;
                };
                if cancelled.load(Ordering::Relaxed) || !Arc::ptr_eq(&cancelled, &panel.cancelled) {
                    return;
                }
                let files = result
                    .hits
                    .iter()
                    .map(|hit| &hit.relative)
                    .collect::<std::collections::BTreeSet<_>>()
                    .len();
                panel.status = format!(
                    "{} matches in {} files{}{}",
                    result.hits.len(),
                    files,
                    if result.limited {
                        " • first 2,000 shown"
                    } else {
                        ""
                    },
                    if result.skipped > 0 {
                        format!(
                            " • {} files/folders skipped (unreadable or over 8 MiB)",
                            result.skipped
                        )
                    } else {
                        String::new()
                    }
                );
                panel.hits = result.hits;
                panel.selected = previous
                    .and_then(|(path, range)| {
                        panel
                            .hits
                            .iter()
                            .position(|hit| hit.relative == path && hit.range == range)
                    })
                    .unwrap_or(0);
                panel
                    .scroll
                    .scroll_to_item(panel.selected, ScrollStrategy::Top);
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn move_search_result(&mut self, down: bool, cx: &mut Context<Self>) {
        if let Some(panel) = &mut self.search {
            if panel.hits.is_empty() {
                return;
            }
            panel.selected = if down {
                (panel.selected + 1).min(panel.hits.len() - 1)
            } else {
                panel.selected.saturating_sub(1)
            };
            panel
                .scroll
                .scroll_to_item(panel.selected, ScrollStrategy::Top);
            cx.notify();
        }
    }

    fn activate_search_result(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || window.has_active_dialog(cx) {
            return;
        }
        let Some(panel) = &self.search else {
            return;
        };
        let Some(hit) = panel.hits.get(panel.selected).cloned() else {
            return;
        };
        let destination = search::Destination {
            path: panel.root.join(&hit.relative),
            hit,
            query: panel.query.read(cx).value().to_string(),
            case_sensitive: panel.case_sensitive,
        };
        if self
            .document
            .as_ref()
            .is_some_and(|doc| doc.path == destination.path)
        {
            self.reveal_search_result(destination, window, cx);
        } else {
            self.request(Next::SearchResult(destination), window, cx);
        }
    }

    pub(super) fn reveal_search_result(
        &mut self,
        destination: search::Destination,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self
            .document
            .as_ref()
            .is_some_and(|doc| doc.path == destination.path)
        {
            return;
        }
        let text = self.editor.read(cx).value();
        let Some(range) = search::revalidate(&text, &destination) else {
            self.notebooks.notice =
                Some("The search match no longer exists. Results have been refreshed.".into());
            self.schedule_search(window, cx);
            return;
        };
        if self.mode == ViewMode::Reading {
            self.set_mode(ViewMode::Source, window, cx);
        }
        self.editor.update(cx, |editor, cx| {
            editor.select_source_range(range, window, cx);
        });
        cx.notify();
    }

    pub(super) fn render_search_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let panel = self.search.as_ref().unwrap();
        div()
            .id("notebook-search")
            .flex()
            .flex_col()
            .w_full()
            .min_w_0()
            .h_full()
            .min_h_0()
            .gap_2()
            .p_2()
            .capture_action(
                cx.listener(|this, _: &gpui_component::input::MoveDown, _, cx| {
                    this.move_search_result(true, cx);
                    cx.stop_propagation();
                }),
            )
            .capture_action(
                cx.listener(|this, _: &gpui_component::input::MoveUp, _, cx| {
                    this.move_search_result(false, cx);
                    cx.stop_propagation();
                }),
            )
            .capture_action(
                cx.listener(|this, _: &gpui_component::input::Enter, window, cx| {
                    this.activate_search_result(window, cx);
                    cx.stop_propagation();
                }),
            )
            .capture_action(
                cx.listener(|this, _: &gpui_component::input::Escape, window, cx| {
                    this.editor
                        .update(cx, |editor, cx| editor.focus(window, cx));
                    cx.stop_propagation();
                }),
            )
            .child(Input::new(&panel.query))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_1()
                    .child(
                        Button::new("search-case")
                            .small()
                            .ghost()
                            .label(if panel.case_sensitive {
                                "Match case: on"
                            } else {
                                "Match case: off"
                            })
                            .on_click(cx.listener(|this, _, window, cx| {
                                if let Some(panel) = &mut this.search {
                                    panel.case_sensitive = !panel.case_sensitive;
                                }
                                this.schedule_search(window, cx);
                            })),
                    )
                    .child(
                        Button::new("search-refresh")
                            .small()
                            .ghost()
                            .label("Refresh")
                            .on_click(
                                cx.listener(|this, _, window, cx| this.schedule_search(window, cx)),
                            ),
                    ),
            )
            .when(!panel.status.is_empty(), |view| {
                view.child(div().text_xs().child(panel.status.clone()))
            })
            .child(
                uniform_list(
                    "search-results",
                    panel.hits.len(),
                    cx.processor(|this, range: std::ops::Range<usize>, _, cx| {
                        let panel = this.search.as_ref().unwrap();
                        range
                            .map(|i| {
                                let hit = &panel.hits[i];
                                let first = i == 0 || panel.hits[i - 1].relative != hit.relative;
                                div()
                                    .id(i)
                                    .h(px(54.))
                                    .w_full()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .px_1()
                                    .when(i == panel.selected, |view| view.bg(cx.theme().accent))
                                    .child(div().text_xs().truncate().child(if first {
                                        hit.relative.display().to_string()
                                    } else {
                                        String::new()
                                    }))
                                    .child(
                                        div()
                                            .flex()
                                            .overflow_hidden()
                                            .text_sm()
                                            .child(
                                                div()
                                                    .text_color(cx.theme().muted_foreground)
                                                    .child(format!("{}: {}", hit.line, hit.before)),
                                            )
                                            .child(
                                                div()
                                                    .bg(cx.theme().secondary)
                                                    .child(hit.matched.clone()),
                                            )
                                            .child(div().child(hit.after.clone())),
                                    )
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        if let Some(panel) = &mut this.search {
                                            panel.selected = i;
                                        }
                                        this.activate_search_result(window, cx);
                                    }))
                            })
                            .collect::<Vec<_>>()
                    }),
                )
                .track_scroll(panel.scroll.clone())
                .flex_1()
                .min_h_0()
                .w_full(),
            )
    }
}

#[cfg(feature = "editor-diagnostics")]
pub(super) async fn diagnostic_search(notes: &Entity<Notes>, cx: &mut AsyncWindowContext) {
    for step in 0..=18 {
        Timer::after(Duration::from_millis(400)).await;
        // Wait for file loads and debounced scans, with a bounded deadline.
        for attempt in 0..100 {
            let ready = notes
                .update(cx, |this, _| {
                    !this.busy
                        && !this.saves.writing
                        && this
                            .search
                            .as_ref()
                            .is_none_or(|p| p.status != "Searching…")
                })
                .unwrap();
            if ready {
                break;
            }
            assert!(attempt < 99, "search smoke timed out at {step}");
            Timer::after(Duration::from_millis(100)).await;
        }
        cx.update(|window, cx| {
            if step == 1 || step == 6 {
                window.dispatch_keystroke(Keystroke::parse("ctrl-shift-f").unwrap(), cx);
                return;
            }
            if step == 4 || step == 7 {
                if step == 7 {
                    window.dispatch_keystroke(Keystroke::parse("down").unwrap(), cx);
                }
                window.dispatch_keystroke(Keystroke::parse("enter").unwrap(), cx);
                return;
            }
            notes.update(cx, |this, cx| {
                let root = this.notebooks.active.as_ref().unwrap().root.clone();
                let a = root.join("search-smoke-a.md");
                let b = root.join("search-smoke-b.md");
                match step {
                    0 => {
                        fs::write(&a, "café needle\nneedle").unwrap();
                        fs::write(&b, "prefix needle").unwrap();
                        this.install_document(Some(Document::open(a).unwrap()), window, cx);
                        this.set_mode(ViewMode::Reading, window, cx);
                        this.toggle_sidebar(window, cx);
                        assert!(this.sidebar_collapsed);
                    }
                    2 => {
                        let query = this
                            .search
                            .as_ref()
                            .expect("shortcut opens search")
                            .query
                            .clone();
                        assert!(query.focus_handle(cx).is_focused(window));
                        assert!(!this.sidebar_collapsed);
                        assert!(this.search.as_ref().unwrap().status.is_empty());
                        query.update(cx, |q, cx| q.set_value("superseded", window, cx));
                        query.update(cx, |q, cx| q.set_value("needle", window, cx));
                    }
                    3 => {
                        assert_eq!(this.search.as_ref().unwrap().hits.len(), 3);
                        let query = this.search.as_ref().unwrap().query.clone();
                        let width = this
                            .notebooks
                            .active
                            .as_ref()
                            .unwrap()
                            .settings
                            .sidebar_width;
                        this.search.as_mut().unwrap().selected = 1;
                        this.toggle_sidebar(window, cx);
                        assert!(this.sidebar_collapsed);
                        assert!(this.focus.is_focused(window));
                        this.toggle_sidebar(window, cx);
                        assert!(!this.sidebar_collapsed);
                        assert!(query.focus_handle(cx).is_focused(window));
                        this.show_explorer(window, cx);
                        assert!(!this.search.as_ref().unwrap().visible);
                        assert!(this.notebooks.tree_focus.is_focused(window));
                        this.toggle_sidebar(window, cx);
                        this.toggle_sidebar(window, cx);
                        assert!(!this.search.as_ref().unwrap().visible);
                        assert!(this.notebooks.tree_focus.is_focused(window));
                        this.open_search_all(window, cx);
                        let panel = this.search.as_ref().unwrap();
                        assert!(panel.visible);
                        assert_eq!(panel.query, query);
                        assert_eq!(query.read(cx).value().as_str(), "needle");
                        assert_eq!(panel.hits.len(), 3);
                        assert_eq!(panel.selected, 1);
                        assert_eq!(
                            this.notebooks
                                .active
                                .as_ref()
                                .unwrap()
                                .settings
                                .sidebar_width,
                            width
                        );
                        this.search.as_mut().unwrap().selected = 0;
                    }
                    5 | 8 => {
                        let panel = this.search.as_ref().unwrap();
                        assert_eq!(panel.selected, if step == 5 { 0 } else { 1 });
                        assert_eq!(
                            this.editor.read(cx).cursor(),
                            panel.hits[panel.selected].range.end
                        );
                        assert!(this.mode == ViewMode::Source);
                        assert!(this.editor.focus_handle(cx).is_focused(window));
                        if step == 5 {
                            this.toggle_sidebar(window, cx);
                            assert!(this.sidebar_collapsed);
                        }
                    }
                    9 => {
                        this.editor
                            .update(cx, |e, cx| e.set_value("unsaved needle", window, cx));
                        this.search
                            .as_ref()
                            .unwrap()
                            .query
                            .clone()
                            .update(cx, |q, cx| q.set_value("unsaved", window, cx));
                    }
                    10 => {
                        assert_eq!(this.search.as_ref().unwrap().hits.len(), 1);
                        this.search
                            .as_ref()
                            .unwrap()
                            .query
                            .clone()
                            .update(cx, |q, cx| q.set_value("needle", window, cx));
                    }
                    11 => {
                        let panel = this.search.as_ref().unwrap();
                        assert_eq!(panel.hits.len(), 2);
                        let destination = search::Destination {
                            path: b.clone(),
                            hit: panel.hits[1].clone(),
                            query: "needle".into(),
                            case_sensitive: false,
                        };
                        this.answer_request(
                            None,
                            Next::SearchResult(destination.clone()),
                            window,
                            cx,
                        );
                        assert_eq!(this.document.as_ref().unwrap().path, a);
                        assert_eq!(this.editor.read(cx).value().as_str(), "unsaved needle");
                        this.answer_request(Some(0), Next::SearchResult(destination), window, cx);
                    }
                    12 => {
                        assert_eq!(this.document.as_ref().unwrap().path, b);
                        assert_eq!(this.editor.read(cx).cursor(), 13);
                        assert_eq!(fs::read_to_string(a).unwrap(), "unsaved needle");
                        this.editor
                            .update(cx, |e, cx| e.set_value("discard me", window, cx));
                    }
                    13 => {
                        let hit = this.search.as_ref().unwrap().hits[0].clone();
                        this.answer_request(
                            Some(1),
                            Next::SearchResult(search::Destination {
                                path: a,
                                hit,
                                query: "needle".into(),
                                case_sensitive: false,
                            }),
                            window,
                            cx,
                        );
                    }
                    14 => {
                        assert_eq!(this.document.as_ref().unwrap().path, a);
                        assert_eq!(this.editor.read(cx).cursor(), 14);
                        assert_eq!(fs::read_to_string(b).unwrap(), "prefix needle");
                        this.set_mode(ViewMode::LivePreview, window, cx);
                        this.editor.update(cx, |e, cx| {
                            assert!(e.select_source_range(8..14, window, cx));
                            assert!(!e.select_source_range(0..999, window, cx));
                            assert_eq!(e.value().as_str(), "unsaved needle");
                        });
                    }
                    15 => {
                        fs::remove_file(b).unwrap();
                        this.schedule_search(window, cx);
                    }
                    16 => assert_eq!(this.search.as_ref().unwrap().hits.len(), 1),
                    17 => {
                        assert_eq!(this.document.as_ref().unwrap().path, a);
                        assert!(this.search_destination.is_none());
                        window.close_dialog(cx);
                        this.request(
                            Next::Notebook {
                                path: root,
                                create: false,
                            },
                            window,
                            cx,
                        );
                    }
                    18 => assert!(this.search.is_none(), "notebook switch clears search"),
                    _ => unreachable!(),
                }
            });
        })
        .unwrap();
    }
}
