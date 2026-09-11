use aho_corasick::AhoCorasick;
use rust_i18n::t;
use std::{ops::Range, rc::Rc};

use gpui::{
    App, AppContext as _, Context, Empty, Entity, FocusHandle, Focusable, Half,
    InteractiveElement as _, IntoElement, KeyBinding, ParentElement as _, Pixels, Render, Styled,
    Subscription, Window, actions, canvas, div, prelude::FluentBuilder as _,
};
use ropey::Rope;

use crate::{
    ActiveTheme, Disableable, IconName, Selectable, Sizable,
    actions::SelectUp,
    button::{Button, ButtonVariants},
    h_flex,
    input::{
        Enter, Escape, IndentInline, Input, InputEvent, InputState, RopeExt as _, Search,
        movement::MoveDirection,
    },
    label::Label,
    v_flex,
};

const CONTEXT: &'static str = "SearchPanel";

actions!(input, [Tab]);

pub(super) fn init(cx: &mut App) {
    cx.bind_keys(vec![KeyBinding::new(
        "shift-enter",
        SelectUp,
        Some(CONTEXT),
    )]);
}

#[derive(Debug, Clone)]
pub struct SearchMatcher {
    text: Rope,
    pub query: Option<AhoCorasick>,

    pub(super) matched_ranges: Rc<Vec<Range<usize>>>,
    pub(super) current_match_ix: usize,
    /// Is in replacing mode, if true, the next update will not reset the current match index.
    replacing: bool,
}

impl SearchMatcher {
    pub fn new() -> Self {
        Self {
            text: "".into(),
            query: None,
            matched_ranges: Rc::new(Vec::new()),
            current_match_ix: 0,
            replacing: false,
        }
    }

    /// Update source text and re-match
    pub(crate) fn update(&mut self, text: &Rope) {
        if self.text.eq(text) {
            return;
        }

        self.text = text.clone();
        self.update_matches();
    }

    fn update_matches(&mut self) {
        let mut new_ranges = Vec::new();
        if let Some(query) = &self.query {
            let text = self.text.to_string();
            // FIXME: Use stream find
            let matches = query.stream_find_iter(text.as_bytes());

            for query_match in matches.into_iter() {
                let query_match = query_match.expect("query match for select all action");
                new_ranges.push(query_match.range());
            }
        }
        self.matched_ranges = Rc::new(new_ranges);
        self.current_match_ix = if self.replacing {
            self.current_match_ix
                .min(self.matched_ranges.len().saturating_sub(1))
        } else {
            0
        };
        self.replacing = false;
    }

    /// Update the search query and reset the current match index.
    pub fn update_query(&mut self, query: &str, case_insensitive: bool) {
        self.replacing = false;
        if query.len() > 0 {
            self.query = Some(
                AhoCorasick::builder()
                    .ascii_case_insensitive(case_insensitive)
                    .build(&[query.to_string()])
                    .expect("failed to build AhoCorasick query in SearchMatcher"),
            );
        } else {
            self.query = None;
        }
        self.update_matches();
    }

    /// Returns the number of matches found.
    #[allow(unused)]
    #[inline]
    fn len(&self) -> usize {
        self.matched_ranges.len()
    }

    fn label(&self) -> String {
        if self.len() == 0 {
            return "0/0".to_string();
        }
        format!("{}/{}", self.current_match_ix + 1, self.len())
    }

    /// Update the current match index based on the given offset.
    fn update_cursor_by_offset(&mut self, offset: usize) {
        self.current_match_ix = self
            .matched_ranges
            .iter()
            .position(|range| range.end > offset)
            .unwrap_or(0);
    }
}

impl Iterator for SearchMatcher {
    type Item = Range<usize>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.matched_ranges.is_empty() {
            return None;
        }

        if self.current_match_ix < self.matched_ranges.len().saturating_sub(1) {
            self.current_match_ix += 1;
        } else {
            self.current_match_ix = 0;
        }

        self.matched_ranges.get(self.current_match_ix).cloned()
    }
}

impl DoubleEndedIterator for SearchMatcher {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.matched_ranges.is_empty() {
            return None;
        }

        if self.current_match_ix == 0 {
            self.current_match_ix = self.matched_ranges.len();
        }

        self.current_match_ix -= 1;
        let item = self.matched_ranges[self.current_match_ix].clone();

        Some(item)
    }
}

pub(super) struct SearchPanel {
    editor: Entity<InputState>,
    search_input: Entity<InputState>,
    replace_input: Entity<InputState>,
    case_insensitive: bool,
    replace_mode: bool,
    matcher: SearchMatcher,
    input_width: Pixels,

    open: bool,
    _subscriptions: Vec<Subscription>,
}

impl InputState {
    #[cfg(feature = "editor-diagnostics")]
    pub fn diagnostic_find_matcher() {
        let mut matcher = SearchMatcher::new();
        matcher.update(&Rope::from("你好 Cat cat [.*]\ncat"));
        matcher.update_query("cat", true);
        assert_eq!(matcher.len(), 3);
        matcher.update_query("cat", false);
        assert_eq!(matcher.len(), 2);
        matcher.update_query("你好", false);
        assert_eq!(matcher.matched_ranges.as_ref(), &[0..6]);
        matcher.update_query("[.*]", false);
        assert_eq!(matcher.len(), 1);
        matcher.update_cursor_by_offset(100);
        assert_eq!(matcher.current_match_ix, 0);
        matcher.update_query("", false);
        assert_eq!(matcher.len(), 0);
        assert_eq!(matcher.next(), None);
        assert_eq!(matcher.next_back(), None);
        matcher.update_query("cat", true);
        matcher.update(&Rope::from("cat"));
        assert_eq!(matcher.len(), 1);
        assert_eq!(matcher.next_back(), Some(0..3));
        matcher.update(&Rope::from(""));
        assert_eq!(matcher.label(), "0/0");

        let source = Rope::from("你好 [.*] 😀 [.*] 尾");
        matcher.update(&source);
        matcher.update_query("[.*]", false);
        assert_eq!(
            replacement_text(&source, &matcher.matched_ranges, "é"),
            "你好 é 😀 é 尾"
        );
        assert_eq!(
            replacement_text(&source, &matcher.matched_ranges[..1], ""),
            "你好  😀 [.*] 尾"
        );
        matcher.current_match_ix = 1;
        matcher.replacing = true;
        matcher.update(&Rope::from("你好 [.*] 😀 尾"));
        assert_eq!(matcher.label(), "1/1");
        matcher.update_query("absent", false);
        assert_eq!(matcher.label(), "0/0");
    }

    pub(super) fn close_search(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(panel) = self.search_panel.clone() else {
            return false;
        };
        if !panel.read(cx).open {
            return false;
        }
        panel.update(cx, |panel, cx| {
            panel.open = false;
            cx.notify();
        });
        self.focus(window, cx);
        cx.notify();
        true
    }

    #[cfg(feature = "editor-diagnostics")]
    pub fn diagnostic_find(&self, cx: &App) -> Option<(Entity<InputState>, usize, usize, String)> {
        let panel = self.search_panel.as_ref()?.read(cx);
        panel.open.then(|| {
            (
                panel.search_input.clone(),
                panel.matcher.len(),
                panel.matcher.current_match_ix,
                self.selected_text().to_string(),
            )
        })
    }

    #[cfg(feature = "editor-diagnostics")]
    pub fn diagnostic_replace_step(
        &mut self,
        step: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        const ORIGINAL: &str = "你好 [.*] 😀 [.*] 尾";
        const ONE: &str = "你好 é 😀 [.*] 尾";
        const ALL: &str = "你好 é 😀 é 尾";
        match step {
            0 => {
                self.set_value(ORIGINAL, window, cx);
                self.selected_range = (7..11).into();
                self.open_search_replace(window, cx);
                let panel = self.search_panel.as_ref().unwrap().read(cx);
                assert!(panel.open && panel.replace_mode);
                assert_eq!(panel.search_input.read(cx).value().as_str(), "[.*]");
                assert!(panel.search_input.read(cx).focus_handle.is_focused(window));
            }
            1 => {
                self.search_panel.clone().unwrap().update(cx, |panel, cx| {
                    assert_eq!(panel.matcher.len(), 2);
                    panel
                        .replace_input
                        .update(cx, |input, cx| input.set_value("é", window, cx));
                    panel.replace_next(window, cx);
                });
            }
            2 => {
                assert_eq!(self.value().as_str(), ONE);
                self.undo(&super::Undo, window, cx);
                assert_eq!(self.value().as_str(), ORIGINAL);
                self.redo(&super::Redo, window, cx);
                assert_eq!(self.value().as_str(), ONE);
                self.search_panel
                    .clone()
                    .unwrap()
                    .update(cx, |panel, cx| panel.replace_all(window, cx));
            }
            3 => {
                assert_eq!(self.value().as_str(), ALL);
                self.undo(&super::Undo, window, cx);
                assert_eq!(self.value().as_str(), ONE);
                self.redo(&super::Redo, window, cx);
                assert_eq!(self.value().as_str(), ALL);
                self.close_search(window, cx);
            }
            _ => unreachable!(),
        }
    }

    /// Update the search matcher when text changes.
    pub(super) fn update_search(&mut self, cx: &mut App) {
        let Some(search_panel) = self.search_panel.as_ref() else {
            return;
        };

        let text = self.text.clone();
        search_panel.update(cx, |this, cx| {
            this.matcher.update(&text);
            cx.notify();
        });
    }

    pub(super) fn on_action_search(
        &mut self,
        _: &Search,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.searchable {
            cx.propagate();
            return;
        }
        self.open_search(window, cx);
    }

    /// Open find with replacement controls expanded, focusing the search query.
    /// Find-only and disabled inputs cannot expose replacement controls.
    pub fn open_search_replace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.searchable || !self.search_replaceable || self.disabled {
            return;
        }
        self.open_search(window, cx);
        if let Some(panel) = &self.search_panel {
            panel.update(cx, |panel, cx| {
                panel.replace_mode = true;
                cx.notify();
            });
        }
    }

    /// Open and focus find, using the current selection as the initial query.
    pub fn open_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.searchable || self.disabled {
            return;
        }

        let search_panel = match self.search_panel.as_ref() {
            Some(panel) => panel.clone(),
            None => SearchPanel::new(cx.entity(), window, cx),
        };

        let text = self.text.clone();
        let editor = cx.entity();
        let selected_text = Rope::from(self.selected_text());
        search_panel.update(cx, |this, cx| {
            this.editor = editor;
            this.matcher.update(&text);
            this.show(&selected_text, window, cx);
        });
        self.search_panel = Some(search_panel);
        cx.notify();
    }
}

fn replacement_text(source: &Rope, ranges: &[Range<usize>], replacement: &str) -> String {
    let mut rope = source.clone();
    for range in ranges.iter().rev() {
        rope.replace(range.clone(), replacement);
    }
    rope.to_string()
}

impl SearchPanel {
    pub fn new(editor: Entity<InputState>, window: &mut Window, cx: &mut App) -> Entity<Self> {
        let search_input = cx.new(|cx| InputState::new(window, cx).placeholder("Find"));
        let replace_input = cx.new(|cx| InputState::new(window, cx));

        cx.new(|cx| {
            let _subscriptions = vec![cx.subscribe_in(
                &search_input,
                window,
                |this: &mut Self, _, ev: &InputEvent, window, cx| {
                    // Handle search input changes
                    match ev {
                        InputEvent::Change => {
                            this.update_search_query(window, cx);
                        }
                        _ => {}
                    }
                },
            )];

            Self {
                editor,
                search_input,
                replace_input,
                case_insensitive: true,
                replace_mode: false,
                matcher: SearchMatcher::new(),
                open: true,
                input_width: Pixels::ZERO,
                _subscriptions,
            }
        })
    }

    pub(super) fn show(
        &mut self,
        selected_text: &Rope,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open = true;
        self.search_input.read(cx).focus_handle.focus(window);

        self.search_input.update(cx, |this, cx| {
            if selected_text.len() > 0 {
                // Set value will emit to update_search_query
                this.set_value(selected_text.to_string(), window, cx);
            }
            this.select_all(&super::SelectAll, window, cx);
        });
        cx.notify();
    }

    fn update_search_query(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let query = self.search_input.read(cx).value();
        let offset = self.editor.read(cx).selected_range.start;

        self.matcher
            .update_query(query.as_str(), self.case_insensitive);

        self.matcher.update_cursor_by_offset(offset);
        self.select_current(cx);
        cx.notify();
    }

    pub(super) fn hide(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open = false;
        self.editor.read(cx).focus_handle.focus(window);
        self.editor.update(cx, |_, cx| cx.notify());
        cx.notify();
    }

    fn on_action_prev(&mut self, _: &SelectUp, window: &mut Window, cx: &mut Context<Self>) {
        self.prev(window, cx);
    }

    fn on_action_next(&mut self, _: &Enter, window: &mut Window, cx: &mut Context<Self>) {
        self.next(window, cx);
    }

    fn on_action_escape(&mut self, _: &Escape, window: &mut Window, cx: &mut Context<Self>) {
        self.hide(window, cx);
    }

    fn on_action_tab(&mut self, _: &IndentInline, window: &mut Window, cx: &mut Context<Self>) {
        self.editor.focus_handle(cx).focus(window);
    }

    fn prev(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.matcher.next_back();
        self.select_current(cx);
    }

    fn next(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.matcher.next();
        self.select_current(cx);
    }

    fn select_current(&self, cx: &mut Context<Self>) {
        let range = self
            .matcher
            .matched_ranges
            .get(self.matcher.current_match_ix)
            .cloned();
        self.editor.update(cx, |state, cx| {
            if let Some(range) = range {
                state.move_to(range.start, None, cx);
                state.selection_reversed = false;
                state.select_to(range.end, cx);
            }
            cx.notify();
        });
        cx.notify();
    }

    pub(super) fn matcher(&self) -> Option<&SearchMatcher> {
        if !self.open {
            return None;
        }

        Some(&self.matcher)
    }

    fn replace_next(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let new_text = self.replace_input.read(cx).value();
        if let Some(range) = self
            .matcher
            .matched_ranges
            .get(self.matcher.current_match_ix)
            .cloned()
        {
            let text_state = self.editor.clone();

            let snapshot = self.matcher.text.clone();
            cx.spawn_in(window, async move |_, cx| {
                cx.update(|window, cx| {
                    text_state.update(cx, |state, cx| {
                        if state.disabled || !state.search_replaceable || state.text != snapshot {
                            return;
                        }
                        if let Some(panel) = &state.search_panel {
                            panel.update(cx, |panel, _| panel.matcher.replacing = true);
                        }
                        let range_utf16 = state.range_to_utf16(&range);
                        state.replace_range_isolated(
                            Some(range_utf16),
                            new_text.as_str(),
                            window,
                            cx,
                        );
                    });
                })
            })
            .detach();
        }
    }

    fn replace_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let new_text = self.replace_input.read(cx).value();
        let snapshot = self.matcher.text.clone();
        let ranges = self.matcher.matched_ranges.clone();
        if ranges.is_empty() {
            return;
        }

        let editor = self.editor.clone();
        cx.spawn_in(window, async move |_, cx| {
            cx.update(|window, cx| {
                editor.update(cx, |state, cx| {
                    if state.disabled || !state.search_replaceable || state.text != snapshot {
                        return;
                    }
                    let text = replacement_text(&state.text, &ranges, new_text.as_str());
                    let full_range = state.range_to_utf16(&(0..state.text.len()));
                    state.replace_range_isolated(Some(full_range), &text, window, cx);
                    state.scroll_to(0, Some(MoveDirection::Down), cx);
                });
            })
        })
        .detach();
    }
}

impl Focusable for SearchPanel {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.search_input.read(cx).focus_handle.clone()
    }
}

impl Render for SearchPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.open {
            return Empty.into_any_element();
        }

        let has_matches = self.matcher.len() > 0;

        v_flex()
            .id("search-panel")
            .occlude()
            .track_focus(&self.focus_handle(cx))
            .key_context(CONTEXT)
            .on_action(cx.listener(Self::on_action_prev))
            .on_action(cx.listener(Self::on_action_next))
            .on_action(cx.listener(Self::on_action_escape))
            .on_action(cx.listener(Self::on_action_tab))
            .on_action(cx.listener(|this, _: &Search, window, cx| {
                this.search_input.update(cx, |input, cx| {
                    input.focus(window, cx);
                    input.select_all(&super::SelectAll, window, cx);
                });
            }))
            .font_family(cx.theme().font_family.clone())
            .items_center()
            .py_2()
            .px_3()
            .w_full()
            .gap_1()
            .bg(cx.theme().popover)
            .border_b_1()
            .rounded(cx.theme().radius.half())
            .border_color(cx.theme().border)
            .child(
                h_flex()
                    .w_full()
                    .gap_2()
                    .child(
                        div()
                            .flex_1()
                            .gap_1()
                            .child(
                                Input::new(&self.search_input)
                                    .focus_bordered(false)
                                    .suffix(
                                        Button::new("case-insensitive")
                                            .selected(!self.case_insensitive)
                                            .xsmall()
                                            .compact()
                                            .ghost()
                                            .icon(IconName::CaseSensitive)
                                            .tooltip("Match case")
                                            .on_click(cx.listener(|this, _, window, cx| {
                                                this.case_insensitive = !this.case_insensitive;
                                                this.update_search_query(window, cx);
                                                cx.notify();
                                            })),
                                    )
                                    .small()
                                    .w_full()
                                    .shadow_none(),
                            )
                            .child(
                                canvas(
                                    {
                                        let view = cx.entity();
                                        move |bounds, _, cx| {
                                            view.update(cx, |r, _| {
                                                r.input_width = bounds.size.width
                                            })
                                        }
                                    },
                                    |_, _, _, _| {},
                                )
                                .absolute()
                                .size_full(),
                            ),
                    )
                    .when(self.editor.read(cx).search_replaceable, |row| {
                        row.child(
                            Button::new("replace-mode")
                                .xsmall()
                                .ghost()
                                .icon(IconName::Replace)
                                .selected(self.replace_mode)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.replace_mode = !this.replace_mode;
                                    if this.replace_mode {
                                        this.replace_input.read(cx).focus_handle.focus(window);
                                    } else {
                                        this.search_input.read(cx).focus_handle.focus(window);
                                    }
                                    cx.notify();
                                })),
                        )
                    })
                    .child(
                        Button::new("prev")
                            .xsmall()
                            .ghost()
                            .icon(IconName::ChevronLeft)
                            .tooltip("Previous match (Shift+Enter)")
                            .disabled(!has_matches)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.prev(window, cx);
                            })),
                    )
                    .child(
                        Button::new("next")
                            .xsmall()
                            .ghost()
                            .icon(IconName::ChevronRight)
                            .tooltip("Next match (Enter)")
                            .disabled(!has_matches)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.next(window, cx);
                            })),
                    )
                    .child(
                        Label::new(self.matcher.label())
                            .when(!has_matches, |this| {
                                this.text_color(cx.theme().muted_foreground)
                            })
                            .text_left()
                            .min_w_16(),
                    )
                    .child(div().w_7())
                    .child(
                        Button::new("close")
                            .xsmall()
                            .ghost()
                            .icon(IconName::Close)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.on_action_escape(&Escape, window, cx);
                            })),
                    ),
            )
            .when(self.replace_mode, |this| {
                this.child(
                    h_flex()
                        .w_full()
                        .gap_2()
                        .child(
                            Input::new(&self.replace_input)
                                .focus_bordered(false)
                                .small()
                                .w(self.input_width)
                                .shadow_none(),
                        )
                        .child(
                            Button::new("replace-one")
                                .small()
                                .label(t!("Input.Replace"))
                                .disabled(!has_matches)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.replace_next(window, cx);
                                })),
                        )
                        .child(
                            Button::new("replace-all")
                                .small()
                                .label(t!("Input.Replace All"))
                                .disabled(!has_matches)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.replace_all(window, cx);
                                })),
                        ),
                )
            })
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search() {
        let mut matcher = SearchMatcher::new();
        matcher.update(&Rope::from("Hello 世界 this is a Is test string."));
        matcher.update_query("Is", true);

        assert_eq!(matcher.len(), 3);
        let mut matches = matcher.clone();
        assert_eq!(matches.current_match_ix, 0);
        assert_eq!(matches.next(), Some(18..20));
        assert_eq!(matches.next(), Some(23..25));
        assert_eq!(matches.current_match_ix, 2);
        assert_eq!(matches.next(), Some(15..17));
        assert_eq!(matches.current_match_ix, 0);
        assert_eq!(matches.next_back(), Some(23..25));
        assert_eq!(matches.current_match_ix, 2);
        assert_eq!(matches.next_back(), Some(18..20));
        assert_eq!(matches.current_match_ix, 1);
        assert_eq!(matches.next_back(), Some(15..17));
        assert_eq!(matches.current_match_ix, 0);
        assert_eq!(matches.next_back(), Some(23..25));

        matcher.update_query("IS", false);
        assert_eq!(matcher.len(), 0);
        assert_eq!(matcher.next(), None);
        assert_eq!(matcher.next_back(), None);
    }

    #[test]
    fn test_search_label() {
        let mut matcher = SearchMatcher::new();
        matcher.update(&Rope::from("Hello 世界 this is a Is test string."));
        matcher.update_query("Is", true);
        assert_eq!(matcher.label(), "1/3");
        matcher.next();
        assert_eq!(matcher.label(), "2/3");
        matcher.next();
        assert_eq!(matcher.label(), "3/3");
        matcher.next();
        assert_eq!(matcher.label(), "1/3");

        matcher.update_query("IS", false);
        assert_eq!(matcher.label(), "0/0");
    }

    #[test]
    fn test_select_range_start() {
        let mut matcher = SearchMatcher::new();
        matcher.matched_ranges = Rc::new(vec![5..10, 15..20, 25..30]);
        matcher.update_cursor_by_offset(0);
        assert_eq!(matcher.current_match_ix, 0);

        matcher.update_cursor_by_offset(5);
        assert_eq!(matcher.current_match_ix, 0);

        matcher.update_cursor_by_offset(12);
        assert_eq!(matcher.current_match_ix, 1);

        matcher.update_cursor_by_offset(16);
        assert_eq!(matcher.current_match_ix, 1);

        matcher.update_cursor_by_offset(30);
        assert_eq!(matcher.current_match_ix, 0);

        matcher.update_cursor_by_offset(31);
        assert_eq!(matcher.current_match_ix, 0);
    }
}
