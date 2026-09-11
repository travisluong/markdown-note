use super::*;
use gpui_component::text::SourceHighlights;
use std::ops::Range;

pub(super) struct ReadingSearch {
    pub(super) query: Entity<InputState>,
    _subscription: Subscription,
    case_sensitive: bool,
    pub(super) matches: Vec<Range<usize>>,
    active: usize,
    pub(super) highlights: SourceHighlights,
}

impl Notes {
    pub(super) fn open_reading_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.reading_search.is_none() {
            let query = cx.new(|cx| InputState::new(window, cx).placeholder("Find in file"));
            let subscription = cx.subscribe(&query, |this, _, event, cx| {
                if matches!(event, InputEvent::Change) {
                    this.update_reading_search(cx);
                }
            });
            self.reading_search = Some(ReadingSearch {
                query,
                _subscription: subscription,
                case_sensitive: false,
                matches: Vec::new(),
                active: 0,
                highlights: SourceHighlights::new(self.reading_text.clone(), Vec::new(), None),
            });
        }
        self.reading_search
            .as_ref()
            .unwrap()
            .query
            .update(cx, |query, cx| query.focus(window, cx));
        cx.notify();
    }

    pub(super) fn update_reading_search(&mut self, cx: &mut Context<Self>) {
        let Some(find) = &mut self.reading_search else {
            return;
        };
        let previous = find.matches.get(find.active).cloned();
        find.matches = search::find_ranges(
            &self.reading_text,
            &find.query.read(cx).value(),
            find.case_sensitive,
            usize::MAX,
        );
        find.active = previous
            .and_then(|range| find.matches.iter().position(|item| *item == range))
            .unwrap_or(0);
        find.highlights = SourceHighlights::new(
            self.reading_text.clone(),
            find.matches.clone(),
            find.matches.get(find.active).cloned(),
        );
        cx.notify();
    }

    pub(super) fn reveal_reading_match(
        &mut self,
        query: String,
        case_sensitive: bool,
        range: Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_reading_search(window, cx);
        let find = self.reading_search.as_mut().unwrap();
        find.case_sensitive = case_sensitive;
        find.query
            .update(cx, |input, cx| input.set_value(query, window, cx));
        self.update_reading_search(cx);
        let find = self.reading_search.as_mut().unwrap();
        find.active = find
            .matches
            .iter()
            .position(|item| *item == range)
            .unwrap_or(0);
        find.highlights =
            SourceHighlights::new(self.reading_text.clone(), find.matches.clone(), Some(range));
        cx.notify();
    }

    fn move_reading_match(&mut self, forward: bool, cx: &mut Context<Self>) {
        let Some(find) = &mut self.reading_search else {
            return;
        };
        if find.matches.is_empty() {
            return;
        }
        find.active = next_match(find.active, find.matches.len(), forward);
        find.highlights = SourceHighlights::new(
            self.reading_text.clone(),
            find.matches.clone(),
            find.matches.get(find.active).cloned(),
        );
        cx.notify();
    }

    pub(super) fn render_reading_find(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let find = self.reading_search.as_ref().unwrap();
        div()
            .id("reading-find")
            .flex()
            .flex_col()
            .gap_1()
            .p_2()
            .capture_action(
                cx.listener(|this, event: &gpui_component::input::Enter, _, cx| {
                    this.move_reading_match(!event.secondary, cx);
                    cx.stop_propagation();
                }),
            )
            .capture_action(
                cx.listener(|this, _: &gpui_component::input::Escape, window, cx| {
                    this.reading_search = None;
                    this.focus.focus(window);
                    cx.stop_propagation();
                    cx.notify();
                }),
            )
            .child(Input::new(&find.query))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_1()
                    .child(
                        Button::new("reading-find-case")
                            .small()
                            .ghost()
                            .label(if find.case_sensitive {
                                "Match case: on"
                            } else {
                                "Match case: off"
                            })
                            .on_click(cx.listener(|this, _, _, cx| {
                                if let Some(find) = &mut this.reading_search {
                                    find.case_sensitive = !find.case_sensitive;
                                }
                                this.update_reading_search(cx);
                            })),
                    )
                    .child(
                        Button::new("reading-find-previous")
                            .small()
                            .ghost()
                            .label("Previous")
                            .on_click(
                                cx.listener(|this, _, _, cx| this.move_reading_match(false, cx)),
                            ),
                    )
                    .child(
                        Button::new("reading-find-next")
                            .small()
                            .ghost()
                            .label("Next")
                            .on_click(
                                cx.listener(|this, _, _, cx| this.move_reading_match(true, cx)),
                            ),
                    )
                    .child(div().text_sm().child(format!(
                        "{} / {}",
                        if find.matches.is_empty() {
                            0
                        } else {
                            find.active + 1
                        },
                        find.matches.len()
                    )))
                    .child(
                        Button::new("reading-find-close")
                            .small()
                            .ghost()
                            .label("Close")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.reading_search = None;
                                this.focus.focus(window);
                                cx.notify();
                            })),
                    ),
            )
            .when(find.highlights.hidden_match(), |view| {
                view.child(
                    div()
                        .text_sm()
                        .child("This match is in Markdown syntax; showing its containing block."),
                )
            })
    }
}

fn next_match(active: usize, count: usize, forward: bool) -> usize {
    if count == 0 {
        0
    } else if forward {
        (active + 1) % count
    } else {
        (active + count - 1) % count
    }
}

#[cfg(test)]
mod tests {
    use super::next_match;
    #[test]
    fn match_navigation_wraps_in_both_directions() {
        assert_eq!(next_match(0, 0, true), 0);
        assert_eq!(next_match(0, 1, false), 0);
        assert_eq!(next_match(0, 3, true), 1);
        assert_eq!(next_match(2, 3, true), 0);
        assert_eq!(next_match(0, 3, false), 2);
        assert_eq!(next_match(2, 3, false), 1);
    }
}
