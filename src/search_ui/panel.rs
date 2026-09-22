//! Owns notebook-search interaction and background work without the application
//! root. Captured overlay/lifecycle inputs enter; navigation and focus intents
//! leave. Current overlay input is distinct from each job's captured path.
use gpui::prelude::FluentBuilder;
use gpui::{
    AppContext, Context, Entity, EventEmitter, InteractiveElement, IntoElement, ParentElement,
    Render, ScrollStrategy, StatefulInteractiveElement, Styled, Subscription, Task,
    UniformListScrollHandle, Window, div, uniform_list,
};
use gpui_component::{
    ActiveTheme, Disableable, Sizable,
    button::{Button, ButtonVariants},
    input::{Input, InputEvent, InputState, Rope},
};
use mithril::search;
use std::{
    collections::{BTreeSet, VecDeque},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

#[cfg(feature = "editor-diagnostics")]
pub(super) mod diagnostics;
#[cfg(test)]
mod lifecycle_tests;
#[cfg(test)]
mod owner_tests;

/// Immutable editor snapshot supplied by the application for future scans.
#[derive(Clone)]
pub(super) struct Overlay {
    pub(super) path: PathBuf,
    pub(super) source: Rope,
}

pub(super) enum SearchIntent {
    Open(search::Destination),
    FocusEditor,
}

struct OpenRequest {
    destination: search::Destination,
    selected: usize,
    arrow: bool,
}

struct Navigation {
    previous: usize,
    arrow: bool,
}

pub(crate) struct SearchPanel {
    visible: bool,
    interaction_enabled: bool,
    root: PathBuf,
    exclusions: mithril::notebook::Exclusions,
    query: Entity<InputState>,
    _subscription: Subscription,
    case_sensitive: bool,
    hits: Vec<search::Hit>,
    selected: usize,
    navigation: Option<Navigation>,
    pending_opens: VecDeque<OpenRequest>,
    status: String,
    scroll: UniformListScrollHandle,
    cancelled: Arc<AtomicBool>,
    task: Option<Task<()>>,
    overlays: Vec<Overlay>,
    overlay_paths: BTreeSet<PathBuf>,
    #[cfg(test)]
    completion_delay: Duration,
}

impl EventEmitter<SearchIntent> for SearchPanel {}

impl Drop for SearchPanel {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}

impl SearchPanel {
    pub(super) fn new(
        root: PathBuf,
        exclusions: mithril::notebook::Exclusions,
        overlay: impl IntoIterator<Item = Overlay>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let query = cx.new(|cx| InputState::new(window, cx).placeholder("Search notebook text"));
        let subscription = cx.subscribe(&query, |this, _, event, cx| {
            if matches!(event, InputEvent::Change) {
                this.schedule(cx);
            }
        });
        Self {
            visible: true,
            interaction_enabled: true,
            root,
            exclusions,
            query,
            _subscription: subscription,
            case_sensitive: false,
            hits: Vec::new(),
            selected: 0,
            navigation: None,
            pending_opens: VecDeque::new(),
            status: String::new(),
            scroll: UniformListScrollHandle::new(),
            cancelled: Arc::new(AtomicBool::new(false)),
            task: None,
            overlays: overlay.into_iter().collect(),
            overlay_paths: BTreeSet::new(),
            #[cfg(test)]
            completion_delay: Duration::ZERO,
        }
    }

    pub(super) fn visible(&self) -> bool {
        self.visible
    }

    pub(crate) fn set_interaction_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        if self.interaction_enabled != enabled {
            self.interaction_enabled = enabled;
            cx.notify();
        }
    }

    pub(super) fn show(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.visible = true;
        self.query.update(cx, |query, cx| query.focus(window, cx));
        cx.notify();
    }

    pub(super) fn hide(&mut self, cx: &mut Context<Self>) {
        self.visible = false;
        cx.notify();
    }

    pub(super) fn document_installed(
        &mut self,
        overlay: impl IntoIterator<Item = Overlay>,
        discarded: Option<&Path>,
        cx: &mut Context<Self>,
    ) {
        self.overlays = overlay.into_iter().collect();
        if discarded
            .is_some_and(|path| path.starts_with(&self.root) && self.overlay_paths.contains(path))
        {
            self.schedule(cx);
        }
    }

    pub(super) fn rescan(
        &mut self,
        overlay: impl IntoIterator<Item = Overlay>,
        exclusions: mithril::notebook::Exclusions,
        cx: &mut Context<Self>,
    ) {
        self.exclusions = exclusions;
        self.overlays = overlay.into_iter().collect();
        self.schedule(cx);
    }

    fn schedule(&mut self, cx: &mut Context<Self>) {
        self.cancelled.store(true, Ordering::Relaxed);
        self.task = None;
        self.cancelled = Arc::new(AtomicBool::new(false));
        let cancelled = self.cancelled.clone();
        let root = self.root.clone();
        let exclusions = self.exclusions.clone();
        let query = self.query.read(cx).value().to_string();
        let case_sensitive = self.case_sensitive;
        let previous = self
            .hits
            .get(self.selected)
            .map(|hit| (hit.relative.clone(), hit.range.clone()));
        self.hits.clear();
        self.pending_opens.clear();
        self.overlay_paths.clear();
        self.selected = 0;
        self.scroll.scroll_to_item(0, ScrollStrategy::Top);
        if query.is_empty() {
            self.status.clear();
            cx.notify();
            return;
        }
        self.status = "Searching…".into();
        let buffers = self
            .overlays
            .iter()
            .filter(|overlay| overlay.path.starts_with(&root))
            .map(|overlay| (overlay.path.clone(), overlay.source.clone()))
            .collect::<Vec<_>>();
        self.overlay_paths = buffers.iter().map(|(path, _)| path.clone()).collect();
        let debounce = cx.background_executor().timer(Duration::from_millis(200));
        #[cfg(test)]
        let completion_delay = std::mem::take(&mut self.completion_delay);
        self.task = Some(cx.spawn(async move |this, cx| {
            debounce.await;
            let worker_cancel = cancelled.clone();
            let result = cx
                .background_executor()
                .spawn(async move {
                    search::scan_many(
                        &root,
                        &exclusions,
                        &query,
                        case_sensitive,
                        buffers,
                        &worker_cancel,
                    )
                })
                .await;
            #[cfg(test)]
            cx.background_executor().timer(completion_delay).await;
            let _ = this.update(cx, |this, cx| {
                if cancelled.load(Ordering::Relaxed) || !Arc::ptr_eq(&cancelled, &this.cancelled) {
                    return;
                }
                let files = result
                    .hits
                    .iter()
                    .map(|hit| &hit.relative)
                    .collect::<BTreeSet<_>>()
                    .len();
                this.status = format!(
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
                this.hits = result.hits;
                this.selected = previous
                    .and_then(|(path, range)| {
                        this.hits
                            .iter()
                            .position(|hit| hit.relative == path && hit.range == range)
                    })
                    .unwrap_or(0);
                this.scroll
                    .scroll_to_item(this.selected, ScrollStrategy::Top);
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn projected_selection(&self) -> usize {
        self.pending_opens
            .back()
            .map_or(self.selected, |request| request.selected)
    }

    fn request_adjacent(&mut self, down: bool, cx: &mut Context<Self>) {
        let selected = adjacent_result(self.projected_selection(), self.hits.len(), down);
        self.request_open(selected, true, cx);
    }

    fn request_selected(&mut self, cx: &mut Context<Self>) {
        self.request_open(self.projected_selection(), false, cx);
    }

    fn request_open(&mut self, selected: usize, arrow: bool, cx: &mut Context<Self>) {
        if !self.interaction_enabled {
            return;
        }
        let Some(hit) = self.hits.get(selected).cloned() else {
            return;
        };
        let destination = search::Destination {
            path: self.root.join(&hit.relative),
            hit,
            query: self.query.read(cx).value().to_string(),
            case_sensitive: self.case_sensitive,
        };
        self.pending_opens.push_back(OpenRequest {
            destination: destination.clone(),
            selected,
            arrow,
        });
        cx.emit(SearchIntent::Open(destination));
    }

    // GPUI delivers emitted intents in order. Keep one proposal per undelivered
    // input event so repeated arrows advance through the same file, while a
    // foreground load can reject later proposals without changing selection.
    // Query changes discard this transient queue. Identical destinations remain
    // distinct sequential inputs; matching uses their source/query identity.
    pub(super) fn accept_open(
        &mut self,
        destination: &search::Destination,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.pending_matches(destination) {
            return false;
        }
        let request = self.pending_opens.pop_front().unwrap();
        self.navigation = Some(Navigation {
            previous: self.selected,
            arrow: request.arrow,
        });
        self.selected = request.selected;
        if request.arrow {
            self.scroll
                .scroll_to_item(self.selected, ScrollStrategy::Top);
        }
        cx.notify();
        true
    }

    pub(super) fn reject_open(&mut self, destination: &search::Destination) {
        if self.pending_matches(destination) {
            self.pending_opens.pop_front();
        }
    }

    fn pending_matches(&self, destination: &search::Destination) -> bool {
        self.pending_opens.front().is_some_and(|request| {
            let pending = &request.destination;
            pending.path == destination.path
                && pending.query == destination.query
                && pending.case_sensitive == destination.case_sensitive
                && pending.hit.relative == destination.hit.relative
                && pending.hit.range == destination.hit.range
        })
    }

    pub(super) fn navigation_finished(
        &mut self,
        success: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(navigation) = self.navigation.take() {
            if !success {
                self.selected = navigation.previous.min(self.hits.len().saturating_sub(1));
            }
            if navigation.arrow || !success {
                self.scroll
                    .scroll_to_item(self.selected, ScrollStrategy::Top);
            }
            if self.visible && self.interaction_enabled {
                self.query.update(cx, |query, cx| query.focus(window, cx));
            }
            cx.notify();
        }
    }
}

impl Render for SearchPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let panel = &*self;
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
            .capture_action(cx.listener(
                |this, _: &gpui_component::input::MoveDown, _window, cx| {
                    this.request_adjacent(true, cx);
                    cx.stop_propagation();
                },
            ))
            .capture_action(
                cx.listener(|this, _: &gpui_component::input::MoveUp, _window, cx| {
                    this.request_adjacent(false, cx);
                    cx.stop_propagation();
                }),
            )
            .capture_action(
                cx.listener(|this, _: &gpui_component::input::Enter, _window, cx| {
                    this.request_selected(cx);
                    cx.stop_propagation();
                }),
            )
            .capture_action(
                cx.listener(|_this, _: &gpui_component::input::Escape, _window, cx| {
                    cx.emit(SearchIntent::FocusEditor);
                    cx.stop_propagation();
                }),
            )
            .child(Input::new(&panel.query).disabled(!panel.interaction_enabled))
            .child(
                div().flex().flex_wrap().gap_1().child(
                    Button::new("search-case")
                        .disabled(!panel.interaction_enabled)
                        .small()
                        .ghost()
                        .label(if panel.case_sensitive {
                            "Match case: on"
                        } else {
                            "Match case: off"
                        })
                        .on_click(cx.listener(|this, _, _window, cx| {
                            if !this.interaction_enabled {
                                return;
                            }
                            this.case_sensitive = !this.case_sensitive;
                            this.schedule(cx);
                        })),
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
                        let panel = &*this;
                        range
                            .map(|i| {
                                let hit = &panel.hits[i];
                                let first = i == 0 || panel.hits[i - 1].relative != hit.relative;
                                div()
                                    .id(i)
                                    .debug_selector(move || format!("notebook-search-result-{i}"))
                                    .when(!panel.interaction_enabled, |view| view.opacity(0.5))
                                    .h(crate::zoom_ui::zoom_px(54., cx))
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
                                                    .child(hit.before.clone()),
                                            )
                                            .child(
                                                div()
                                                    .bg(cx.theme().secondary)
                                                    .child(hit.matched.clone()),
                                            )
                                            .child(div().child(hit.after.clone())),
                                    )
                                    .on_click(cx.listener(move |this, _, _window, cx| {
                                        this.request_open(i, false, cx);
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

fn adjacent_result(selected: usize, count: usize, down: bool) -> usize {
    if down {
        selected.saturating_add(1).min(count.saturating_sub(1))
    } else {
        selected.saturating_sub(1)
    }
}

#[cfg(test)]
mod navigation_tests {
    use super::adjacent_result;
    #[test]
    fn arrows_clamp_and_repeat() {
        assert_eq!(adjacent_result(0, 0, true), 0);
        assert_eq!(adjacent_result(0, 3, false), 0);
        let next = adjacent_result(0, 3, true);
        assert_eq!(next, 1);
        assert_eq!(adjacent_result(next, 3, true), 2);
        assert_eq!(adjacent_result(2, 3, true), 2);
        assert_eq!(adjacent_result(2, 3, false), 1);
    }
}
