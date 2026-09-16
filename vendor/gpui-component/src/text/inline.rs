use gpui::{InteractiveElement, ParentElement, StatefulInteractiveElement, prelude::FluentBuilder};
use std::{
    ops::Range,
    rc::Rc,
    sync::{Arc, Mutex},
};

use gpui::{
    App, BorderStyle, Bounds, CursorStyle, Edges, Element, ElementId, GlobalElementId, Half,
    HighlightStyle, Hitbox, HitboxBehavior, InspectorElementId, IntoElement, LayoutId,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point, SharedString, StyledText,
    TextLayout, Window, point, px, quad,
};

use crate::{ActiveTheme, global_state::GlobalState, input::Selection, text::node::LinkMark};

/// A inline element used to render a inline text and support selectable.
///
/// All text in TextView (including the CodeBlock) used this for text rendering.
pub(super) struct Inline {
    id: ElementId,
    text: SharedString,
    link_open: Option<Arc<super::text_view::LinkOpenFn>>,
    source: SharedString,
    links: Rc<Vec<(Range<usize>, LinkMark)>>,
    highlights: Vec<(Range<usize>, HighlightStyle)>,
    styled_text: StyledText,
    active_match: Option<Range<usize>>,
    horizontal_scroll: Option<gpui::ScrollHandle>,
    reveal: Option<super::source_highlights::RevealTarget>,

    state: Arc<Mutex<InlineState>>,
}

/// The inline text state, used RefCell to keep the selection state.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct InlineState {
    hovered_index: Option<usize>,
    /// The text that actually rendering, matched with selection.
    pub(super) text: SharedString,
    pub(super) selection: Option<Selection>,
}

impl InlineState {
    /// Save actually rendered text for selected text to use.
    pub(crate) fn set_text(&mut self, text: SharedString) {
        self.text = text;
    }
}

impl Inline {
    pub(super) fn new(
        id: impl Into<ElementId>,
        state: Arc<Mutex<InlineState>>,
        links: Vec<(Range<usize>, LinkMark)>,
        highlights: Vec<(Range<usize>, HighlightStyle)>,
    ) -> Self {
        let text = state.lock().unwrap().text.clone();
        Self {
            id: id.into(),
            link_open: None,
            source: SharedString::default(),
            links: Rc::new(links),
            highlights,
            text: text.clone(),
            styled_text: StyledText::new(text),
            active_match: None,
            horizontal_scroll: None,
            reveal: None,
            state,
        }
    }

    pub(super) fn with_link_tooltip(self) -> gpui::AnyElement {
        if self.links.is_empty() {
            return self.into_any_element();
        }
        let hovered = self.state.lock().unwrap().hovered_index;
        let target = hovered
            .and_then(|i| self.links.iter().find(|(range, _)| range.contains(&i)))
            .map(|(_, link)| link.url.clone());
        gpui::div()
            .id(self.id.clone())
            .when_some(target, |view, target| {
                view.tooltip(move |window, cx| {
                    crate::tooltip::Tooltip::new(target.clone()).build(window, cx)
                })
            })
            .child(self)
            .into_any_element()
    }

    pub(super) fn horizontal_scroll(mut self, scroll: Option<gpui::ScrollHandle>) -> Self {
        self.horizontal_scroll = scroll;
        self
    }

    pub(super) fn source_highlights(
        mut self,
        runs: &[super::source_map::SourceRun],
        node_cx: &super::node::NodeContext,
    ) -> Self {
        self.link_open = node_cx.link_open.clone();
        self.source = node_cx.source.clone();
        if let Some(search) = &node_cx.highlights {
            let mut matches = Vec::new();
            let first = runs.first().map_or(0, |run| run.source.start);
            let last = runs.last().map_or(0, |run| run.source.end);
            let start = search.matches.partition_point(|range| range.end <= first);
            for range in search.matches[start..]
                .iter()
                .take_while(|range| range.start < last)
            {
                for display in super::source_map::project(runs, range) {
                    matches.push((
                        display,
                        HighlightStyle {
                            background_color: Some(gpui::rgba(0xe5b84b66).into()),
                            ..Default::default()
                        },
                    ));
                }
            }
            matches.sort_by_key(|(range, _)| range.start);
            self.highlights = gpui::combine_highlights(self.highlights, matches).collect();
            if let Some(active) = &search.active {
                let active_ranges = super::source_map::project(runs, active);
                self.active_match = active_ranges.first().cloned();
                let marks = active_ranges.into_iter().map(|range| {
                    (
                        range,
                        HighlightStyle {
                            background_color: Some(gpui::rgba(0xe5b84baa).into()),
                            ..Default::default()
                        },
                    )
                });
                self.highlights = gpui::combine_highlights(self.highlights, marks).collect();
            }
            self.reveal = node_cx.reveal.clone();
        }
        self
    }

    /// Get link at given mouse position.
    fn link_for_position(
        layout: &TextLayout,
        links: &Vec<(Range<usize>, LinkMark)>,
        position: Point<Pixels>,
    ) -> Option<LinkMark> {
        let offset = layout.index_for_position(position).ok()?;
        for (range, link) in links.iter() {
            if range.contains(&offset) {
                return Some(link.clone());
            }
        }

        None
    }

    /// Paint selected bounds for debug.
    #[allow(unused)]
    fn paint_selected_bounds(&self, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
        window.paint_quad(gpui::PaintQuad {
            bounds,
            background: cx.theme().blue.alpha(0.01).into(),
            corner_radii: gpui::Corners::default(),
            border_color: gpui::transparent_black(),
            border_style: BorderStyle::default(),
            border_widths: gpui::Edges::all(px(0.)),
        });
    }

    fn layout_selections(
        &self,
        text_layout: &TextLayout,
        window: &mut Window,
        cx: &mut App,
    ) -> (bool, bool, Option<Selection>) {
        let Some(text_view_state) = GlobalState::global(cx).text_view_state() else {
            return (false, false, None);
        };

        let text_view_state = text_view_state.read(cx);
        let is_selectable = text_view_state.is_selectable();
        if !text_view_state.has_selection() {
            return (is_selectable, false, None);
        }

        let line_height = window.line_height();
        let selection_bounds = text_view_state.selection_bounds();

        // Use for debug selection bounds
        // self.paint_selected_bounds(selection_bounds, window, cx);

        let mut selection: Option<Selection> = None;
        let mut offset = 0;
        let mut chars = self.text.chars().peekable();
        while let Some(c) = chars.next() {
            let Some(pos) = text_layout.position_for_index(offset) else {
                offset += c.len_utf8();
                continue;
            };

            let mut char_width = line_height.half();
            if let Some(next_pos) = text_layout.position_for_index(offset + 1) {
                if next_pos.y == pos.y {
                    char_width = next_pos.x - pos.x;
                }
            }

            if point_in_text_selection(pos, char_width, &selection_bounds, line_height) {
                if selection.is_none() {
                    selection = Some((offset..offset).into());
                }

                let next_offset = offset + c.len_utf8();
                selection.as_mut().unwrap().end = next_offset;
            }

            offset += c.len_utf8();
        }

        (true, true, selection)
    }

    /// Paint the selection background.
    fn paint_selection(
        selection: &Selection,
        text_layout: &TextLayout,
        bounds: &Bounds<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let mut start = selection.start;
        let mut end = selection.end;
        if end < start {
            std::mem::swap(&mut start, &mut end);
        }
        let Some(start_position) = text_layout.position_for_index(start) else {
            return;
        };
        let Some(end_position) = text_layout.position_for_index(end) else {
            return;
        };

        let line_height = text_layout.line_height();
        if start_position.y == end_position.y {
            window.paint_quad(quad(
                Bounds::from_corners(
                    start_position,
                    point(end_position.x, end_position.y + line_height),
                ),
                px(0.),
                cx.theme().selection,
                Edges::default(),
                gpui::transparent_black(),
                BorderStyle::default(),
            ));
        } else {
            window.paint_quad(quad(
                Bounds::from_corners(
                    start_position,
                    point(bounds.right(), start_position.y + line_height),
                ),
                px(0.),
                cx.theme().selection,
                Edges::default(),
                gpui::transparent_black(),
                BorderStyle::default(),
            ));

            if end_position.y > start_position.y + line_height {
                window.paint_quad(quad(
                    Bounds::from_corners(
                        point(bounds.left(), start_position.y + line_height),
                        point(bounds.right(), end_position.y),
                    ),
                    px(0.),
                    cx.theme().selection,
                    Edges::default(),
                    gpui::transparent_black(),
                    BorderStyle::default(),
                ));
            }

            window.paint_quad(quad(
                Bounds::from_corners(
                    point(bounds.left(), end_position.y),
                    point(end_position.x, end_position.y + line_height),
                ),
                px(0.),
                cx.theme().selection,
                Edges::default(),
                gpui::transparent_black(),
                BorderStyle::default(),
            ));
        }
    }
}

impl IntoElement for Inline {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for Inline {
    type RequestLayoutState = ();
    type PrepaintState = Hitbox;

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        global_element_id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let text_style = window.text_style();

        let mut runs = Vec::new();
        let mut ix = 0;
        for (range, highlight) in self.highlights.iter() {
            if ix < range.start {
                runs.push(text_style.clone().to_run(range.start - ix));
            }
            runs.push(text_style.clone().highlight(*highlight).to_run(range.len()));
            ix = range.end;
        }
        if ix < self.text.len() {
            runs.push(text_style.to_run(self.text.len() - ix));
        }

        self.styled_text = StyledText::new(self.text.clone()).with_runs(runs);
        let (layout_id, _) =
            self.styled_text
                .request_layout(global_element_id, inspector_id, window, cx);

        (layout_id, ())
    }

    fn prepaint(
        &mut self,
        id: Option<&GlobalElementId>,
        inspector_id: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        self.styled_text
            .prepaint(id, inspector_id, bounds, &mut (), window, cx);

        let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
        hitbox
    }

    fn paint(
        &mut self,
        global_id: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        prepaint: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let current_view = window.current_view();
        let hitbox = prepaint;
        let mut state = self.state.lock().unwrap();

        let text_layout = self.styled_text.layout().clone();
        self.styled_text
            .paint(global_id, None, bounds, &mut (), &mut (), window, cx);

        if let (Some(active), Some(target)) = (&self.active_match, &self.reveal) {
            if let Some(position) = text_layout.position_for_index(active.start) {
                if let Some(view) = GlobalState::global(cx).text_view_state().cloned() {
                    if let Some(block_bounds) =
                        view.read(cx).list_state.bounds_for_item(target.item)
                    {
                        if !target
                            .highlights
                            .revealed
                            .swap(true, std::sync::atomic::Ordering::Relaxed)
                        {
                            let offset = (position.y - block_bounds.top() - px(24.)).max(px(0.));
                            let target = target.clone();
                            let horizontal = self.horizontal_scroll.clone();
                            window.on_next_frame(move |_, cx| {
                                if view.read(cx).source_snapshot != target.highlights.source
                                    || !view.read(cx).active_search.as_ref().is_some_and(|search| {
                                        search.same_request(&target.highlights)
                                    })
                                {
                                    return;
                                }
                                if let Some(scroll) = horizontal {
                                    let viewport = scroll.bounds();
                                    let delta = if position.x < viewport.left() {
                                        viewport.left() - position.x + px(12.)
                                    } else if position.x + px(24.) > viewport.right() {
                                        viewport.right() - position.x - px(24.)
                                    } else {
                                        px(0.)
                                    };
                                    let mut offset = scroll.offset();
                                    offset.x += delta;
                                    scroll.set_offset(offset);
                                }
                                view.read(cx).list_state.scroll_to(gpui::ListOffset {
                                    item_ix: target.item,
                                    offset_in_item: offset,
                                });
                                cx.notify(current_view);
                            });
                        }
                    }
                }
            }
        }

        // layout selections
        let (is_selectable, is_selection, selection) =
            self.layout_selections(&text_layout, window, cx);

        state.selection = selection;

        if is_selection || is_selectable {
            window.set_cursor_style(CursorStyle::IBeam, &hitbox);
        }

        // Links use the standard arrow cursor.
        let mouse_position = window.mouse_position();
        if let Some(_) = Self::link_for_position(&text_layout, &self.links, mouse_position) {
            window.set_cursor_style(CursorStyle::Arrow, &hitbox);
        }

        if let Some(selection) = &state.selection {
            Self::paint_selection(selection, &text_layout, &bounds, window, cx);
        }

        #[cfg(feature = "editor-diagnostics")]
        if let Some(owner) = GlobalState::global(cx).text_view_state().cloned() {
            let line_height = window.line_height();
            let positions = self
                .links
                .iter()
                .flat_map(|(range, link)| {
                    let last = self.text[range.clone()]
                        .char_indices()
                        .last()
                        .map_or(range.start, |(i, _)| range.start + i);
                    [range.start, last]
                        .into_iter()
                        .filter_map(|offset| {
                            let position = text_layout.position_for_index(offset)?
                                + point(px(2.), line_height * 0.5);
                            Some((link.url.to_string(), position))
                        })
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>();
            owner.update(cx, |state, _| state.diagnostic_links.extend(positions));
        }

        // Right-click resolves the painted text independently of the selection.
        window.on_mouse_event({
            let owner = GlobalState::global(cx).text_view_state().cloned();
            let links = self.links.clone();
            let text_layout = text_layout.clone();
            let hitbox = hitbox.clone();
            move |event: &MouseDownEvent, phase, window, cx| {
                if event.button != gpui::MouseButton::Right
                    || !phase.bubble()
                    || !hitbox.is_hovered(window)
                {
                    return;
                }
                if let (Some(owner), Some(link)) = (
                    &owner,
                    Self::link_for_position(&text_layout, &links, event.position),
                ) {
                    owner.update(cx, |state, cx| {
                        state.open_link_menu(link.url, event.position, window, cx)
                    });
                    cx.stop_propagation();
                }
            }
        });

        // mouse move, update hovered link
        window.on_mouse_event({
            let hitbox = hitbox.clone();
            let text_layout = text_layout.clone();
            let mut hovered_index = state.hovered_index;
            let hover_state = self.state.clone();
            move |event: &MouseMoveEvent, phase, window, cx| {
                if !phase.bubble() {
                    return;
                }

                let current = hovered_index;
                let updated = hitbox
                    .is_hovered(window)
                    .then(|| text_layout.index_for_position(event.position).ok())
                    .flatten();
                //  notify update when hovering over different links
                if current != updated {
                    hovered_index = updated;
                    hover_state.lock().unwrap().hovered_index = updated;
                    cx.notify(current_view);
                }
            }
        });

        if !is_selection {
            // click to open link
            window.on_mouse_event({
                let links = self.links.clone();
                let callback = self.link_open.clone();
                let source = self.source.clone();
                let text_layout = text_layout.clone();

                move |event: &MouseUpEvent, phase, window, cx| {
                    if event.button != gpui::MouseButton::Left
                        || !bounds.contains(&event.position)
                        || !phase.bubble()
                    {
                        return;
                    }

                    if let Some(link) =
                        Self::link_for_position(&text_layout, &links, event.position)
                    {
                        cx.stop_propagation();
                        if let Some(callback) = &callback {
                            callback(&link.url, &source, window, cx);
                        } else {
                            cx.open_url(&link.url);
                        }
                    }
                }
            });
        }
    }
}

/// Check if a `pos` is within a `bounds`, considering multi-line selections.
fn point_in_text_selection(
    pos: Point<Pixels>,
    char_width: Pixels,
    bounds: &Bounds<Pixels>,
    line_height: Pixels,
) -> bool {
    let top = bounds.top();
    let bottom = bounds.bottom();
    let left = bounds.left();
    let right = bounds.right();

    // Out of the vertical bounds
    if pos.y + line_height < top || pos.y >= bottom {
        return false;
    }

    let single_line = (bottom - top) <= line_height;
    if single_line {
        // If it's a single line selection, just check horizontal bounds
        return pos.x + char_width.half() >= left && pos.x + char_width.half() <= right;
    }

    let is_above = pos.y <= top;
    let is_below = pos.y + line_height >= bottom;

    if is_above {
        return pos.x + char_width.half() >= left;
    } else if is_below {
        return pos.x + char_width.half() <= right;
    } else {
        return true;
    }
}

#[cfg(test)]
mod tests {
    use super::point_in_text_selection;
    use gpui::{Bounds, point, px, size};

    #[test]
    fn test_point_in_text_selection() {
        let line_height = px(20.);
        let char_width = px(10.);
        let bounds = Bounds {
            origin: point(px(50.), px(50.)),
            size: size(px(100.), px(100.)),
        };

        // First line but haft line height, true
        // | p --------|
        // | selection |
        // |-----------|
        assert!(point_in_text_selection(
            point(px(50.), px(40.)),
            char_width,
            &bounds,
            line_height
        ));

        // First line in selection, true
        // | p --------|
        // | selection |
        // |-----------|
        assert!(point_in_text_selection(
            point(px(50.), px(50.)),
            char_width,
            &bounds,
            line_height
        ));
        // First line, but left out of selection, false
        // p |-----------|
        //   | selection |
        //   |-----------|
        assert!(!point_in_text_selection(
            point(px(40.), px(50.)),
            char_width,
            &bounds,
            line_height
        ));
        // First line but right out of selection, true
        // |-----------| p
        // | selection |
        // |-----------|
        assert!(point_in_text_selection(
            point(px(160.), px(50.)),
            char_width,
            &bounds,
            line_height
        ));

        // Middle line in selection, true
        // |-----------|
        // |     p     |
        // |-----------|
        assert!(point_in_text_selection(
            point(px(100.), px(70.)),
            char_width,
            &bounds,
            line_height
        ));
        // Middle line, but left out of selection, true
        //   |-----------|
        // p | selection |
        //   |-----------|
        assert!(point_in_text_selection(
            point(px(40.), px(70.)),
            char_width,
            &bounds,
            line_height
        ));
        // Middle line, but right out of selection, true
        // |-----------|
        // | selection | p
        // |-----------|
        assert!(point_in_text_selection(
            point(px(160.), px(70.)),
            char_width,
            &bounds,
            line_height
        ));

        // Last line in selection, true
        // |-----------|
        // | selection |
        // |------- p -|
        assert!(point_in_text_selection(
            point(px(100.), px(140.)),
            char_width,
            &bounds,
            line_height
        ));
        // Last line, but left out of selection, true
        //
        //   |-----------|
        //   | selection |
        // p |-----------|
        assert!(point_in_text_selection(
            point(px(40.), px(140.)),
            char_width,
            &bounds,
            line_height
        ));
        // Last line, but right out of selection, false
        // |-----------|
        // | selection |
        // |-----------| p
        assert!(!point_in_text_selection(
            point(px(160.), px(140.)),
            char_width,
            &bounds,
            line_height
        ));

        // Out of vertical bounds (top), false
        //       p
        // |-----------|
        // | selection |
        // |-----------|
        assert!(!point_in_text_selection(
            point(px(100.), px(20.)),
            char_width,
            &bounds,
            line_height
        ));
        // Out of vertical bounds (bottom), false
        // |-----------|
        // | selection |
        // |-----------|
        //       p
        assert!(!point_in_text_selection(
            point(px(100.), px(160.)),
            char_width,
            &bounds,
            line_height
        ));
    }
}
