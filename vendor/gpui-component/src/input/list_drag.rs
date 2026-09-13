//! Live-preview pointer gestures; the source transaction lives in list_reorder.
use super::{InputState, Rope, RopeExt};
use gpui::{
    Bounds, Context, EntityId, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, Pixels,
    Point, Window, point, px, size,
};

#[derive(Clone)]
pub(super) struct Drag {
    pub snapshot: Rope,
    pub revision: u64,
    pub editor: EntityId,
    pub item: Option<super::list_reorder::Item>,
    pub task: Option<usize>,
    pub origin: Point<Pixels>,
    pub pointer: Point<Pixels>,
    pub active: bool,
    pub destination: Option<(super::list_reorder::Item, bool)>,
    pub indicator: Option<Bounds<Pixels>>,
}
impl InputState {
    pub(super) fn restore_list_viewport(&mut self, line_height: Pixels) {
        let Some((offset, screen_y, selection)) = self.list_viewport_anchor else {
            return;
        };
        if selection != self.selected_range || offset > self.text.len() || !self.live_preview {
            self.list_viewport_anchor = None;
            return;
        }
        let row = self.text.offset_to_point(offset).row;
        let mut scroll = self.scroll_handle.offset();
        scroll.y = (screen_y - self.text_wrapper.lines.prefix(row) * line_height).min(px(0.));
        self.scroll_handle.set_offset(scroll);
        self.deferred_scroll_offset = Some(scroll);
        self.last_selected_range = Some(selection);
    }
    fn list_item_on_row(&self, row: usize) -> Option<&super::list_reorder::Item> {
        let start = self.text.line_start_offset(row);
        let first = self
            .text_wrapper
            .list_items
            .partition_point(|item| item.marker < start);
        self.text_wrapper
            .list_items
            .get(first)
            .filter(|item| item.marker <= self.text.line_end_offset(row))
    }
    fn list_marker_bounds(&self, item: &super::list_reorder::Item) -> Option<Bounds<Pixels>> {
        self.list_marker_bounds_in(item, self.last_layout.as_ref()?, self.last_bounds?)
    }
    pub(super) fn list_marker_bounds_in(
        &self,
        item: &super::list_reorder::Item,
        layout: &super::state::LastLayout,
        bounds: Bounds<Pixels>,
    ) -> Option<Bounds<Pixels>> {
        let row = self.text.offset_to_point(item.marker).row;
        let start = self.text.line_start_offset(row);
        let line = layout.line(row)?;
        let origin = bounds.origin
            + point(
                layout.line_number_width,
                self.text_wrapper.lines.prefix(row) * layout.line_height,
            );
        if item.start_number.is_none()
            && let Some(mut task) = line.task_bounds(layout.line_height)
        {
            task.origin += origin;
            return Some(task);
        }
        let p = line.position_for_index(item.marker - start, layout.line_height)?;
        let width = if item.start_number.is_some() {
            let end = line.position_for_index(item.marker_end - start, layout.line_height)?;
            (end.x - p.x).max(px(1.))
        } else {
            px(16.)
        };
        Some(Bounds::new(
            origin + p,
            size(width, layout.line_height * line.scale),
        ))
    }
    pub(super) fn begin_list_press(
        &mut self,
        event: &MouseDownEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.live_preview || event.button != MouseButton::Left {
            return false;
        }
        let offset = self.index_for_mouse_position(event.position);
        let row = self.text.offset_to_point(offset).row;
        let start = self.text.line_start_offset(row);
        let task = super::live_preview::task_state(&self.text.slice_line(row).to_string())
            .filter(|state| {
                self.task_hit_bounds(row, *state)
                    .is_some_and(|b| b.contains(&event.position))
            })
            .map(|state| start + state);
        let item = self
            .list_item_on_row(row)
            .filter(|item| {
                task.is_some()
                    || self
                        .list_marker_bounds(item)
                        .is_some_and(|b| b.contains(&event.position))
            })
            .cloned();
        if item.is_none() && task.is_none() {
            return false;
        }
        self.selecting = false;
        if !self.disabled && self.ime_marked_range.is_none() {
            self.list_drag = Some(Drag {
                snapshot: self.text.clone(),
                revision: self.text_wrapper.analysis.revision,
                editor: cx.entity().entity_id(),
                item,
                task,
                origin: event.position,
                pointer: event.position,
                active: false,
                destination: None,
                indicator: None,
            });
        }
        cx.stop_propagation();
        true
    }
    pub(super) fn update_list_drag(
        &mut self,
        event: &MouseMoveEvent,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(mut drag) = self.list_drag.take() else {
            return false;
        };
        if self.disabled
            || self.ime_marked_range.is_some()
            || !self.live_preview
            || drag.revision != self.text_wrapper.analysis.revision
            || drag.editor != cx.entity().entity_id()
            || drag.snapshot != self.text
        {
            cx.notify();
            return true;
        }
        drag.pointer = event.position;
        drag.active |= (drag.pointer.x - drag.origin.x).abs() > px(4.)
            || (drag.pointer.y - drag.origin.y).abs() > px(4.);
        drag.destination = None;
        drag.indicator = None;
        if drag.active && self.input_bounds.contains(&event.position) {
            let offset = self.index_for_mouse_position(event.position);
            let row = self.text.offset_to_point(offset).row;
            if let Some(target) = self.list_item_on_row(row) {
                if drag
                    .item
                    .as_ref()
                    .is_some_and(|item| super::list_reorder::can_drop(item, target))
                {
                    if let Some(bounds) = self.list_marker_bounds(target) {
                        let after = event.position.y >= bounds.center().y;
                        let y = if after {
                            let end_row = self
                                .text
                                .offset_to_point(target.source.end.saturating_sub(1))
                                .row;
                            self.last_bounds.unwrap().top()
                                + self.text_wrapper.lines.prefix(end_row + 1)
                                    * self.last_layout.as_ref().unwrap().line_height
                        } else {
                            bounds.top()
                        };
                        drag.indicator = Some(Bounds::new(
                            point(bounds.left(), y),
                            size(
                                (self.input_bounds.right() - bounds.left() - px(12.)).max(px(1.)),
                                px(2.),
                            ),
                        ));
                        drag.destination = Some((target.clone(), after));
                    }
                }
            }
        }
        self.list_drag = Some(drag);
        cx.notify();
        true
    }
    pub(super) fn scroll_list_drag(&mut self, cx: &mut Context<Self>) {
        if self.disabled || !self.live_preview || self.ime_marked_range.is_some() {
            self.list_drag = None;
            return;
        }
        let Some(drag) = self.list_drag.as_ref().filter(|drag| drag.active) else {
            return;
        };
        let p = drag.pointer;
        let delta = if p.y < self.input_bounds.top() + px(28.) {
            px(10.)
        } else if p.y > self.input_bounds.bottom() - px(28.) {
            px(-10.)
        } else {
            return;
        };
        let old = self.scroll_handle.offset();
        let min = -(self.scroll_size.height - self.input_bounds.size.height).max(px(0.));
        let next = point(old.x, (old.y + delta).clamp(min, px(0.)));
        if next != old {
            self.scroll_handle.set_offset(next);
            if let Some(bounds) = self.last_bounds.as_mut() {
                bounds.origin += next - old;
            }
            self.update_list_drag(
                &MouseMoveEvent {
                    position: p,
                    pressed_button: Some(MouseButton::Left),
                    ..Default::default()
                },
                cx,
            );
        }
    }
    pub(super) fn finish_list_press(
        &mut self,
        event: &MouseUpEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if event.button != MouseButton::Left {
            return false;
        }
        if self.list_drag.as_ref().is_some_and(|drag| drag.active) {
            self.update_list_drag(
                &MouseMoveEvent {
                    position: event.position,
                    pressed_button: Some(MouseButton::Left),
                    ..Default::default()
                },
                cx,
            );
        }
        let Some(drag) = self.list_drag.take() else {
            return false;
        };
        cx.stop_propagation();
        cx.notify();
        if self.disabled
            || self.ime_marked_range.is_some()
            || !self.live_preview
            || drag.snapshot != self.text
            || drag.revision != self.text_wrapper.analysis.revision
            || drag.editor != cx.entity().entity_id()
        {
            return true;
        }
        if drag.active {
            if self.input_bounds.contains(&event.position) {
                if let (Some(item), Some((target, after))) = (drag.item, drag.destination) {
                    let line_height = self.last_layout.as_ref().unwrap().line_height;
                    let mut anchor_row = self
                        .text_wrapper
                        .lines
                        .row_at((-self.scroll_handle.offset().y / line_height).max(0.));
                    // Anchor stationary content, never the item being moved.
                    if item
                        .source
                        .contains(&self.text.line_start_offset(anchor_row))
                    {
                        anchor_row = self.text.offset_to_point(target.marker).row;
                    }
                    let anchor_offset = self.text.line_start_offset(anchor_row);
                    let screen_y = self.text_wrapper.lines.prefix(anchor_row) * line_height
                        + self.scroll_handle.offset().y;
                    if let Some((edit, anchor)) = super::list_reorder::reorder_anchored(
                        &self.text.to_string(),
                        &item,
                        &target,
                        after,
                        self.selected_range.into(),
                        anchor_offset,
                    ) {
                        let reversed = self.selection_reversed;
                        let range = self.range_to_utf16(&edit.range);
                        self.replace_range_isolated(Some(range), &edit.text, window, cx);
                        self.selected_range = edit.selection.into();
                        self.selection_reversed = reversed;
                        self.selected_word_range = None;
                        self.preview_click_anchor = None;
                        self.list_viewport_anchor = Some((anchor, screen_y, self.selected_range));
                        self.restore_list_viewport(line_height);
                        self.pause_blink_cursor(cx);
                    }
                }
            }
        } else if let Some(task) = drag.task {
            let row = self.text.offset_to_point(task).row;
            if self
                .task_hit_bounds(row, task - self.text.line_start_offset(row))
                .is_some_and(|b| b.contains(&event.position))
            {
                self.toggle_markdown_task(task, &drag.snapshot.to_string(), window, cx);
            }
        }
        true
    }
}

#[cfg(feature = "editor-diagnostics")]
impl InputState {
    #[doc(hidden)]
    pub fn diagnostic_numbered_drag_checks(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let original = self.text.to_string();
        let items = self.text_wrapper.list_items.clone();
        assert_eq!(items.len(), 3, "numeric-only documents must be indexed");
        let start = self.list_marker_bounds(&items[0]).unwrap().center();
        let target = self.list_marker_bounds(&items[2]).unwrap();
        assert!(
            target.size.width > px(16.),
            "multi-digit markers use their rendered width"
        );
        let end = target.center() + point(px(0.), px(3.));
        self.selected_range = (4..9).into();
        self.selection_reversed = true;
        self.on_mouse_down(
            &MouseDownEvent {
                button: MouseButton::Left,
                position: start,
                click_count: 1,
                ..Default::default()
            },
            window,
            cx,
        );
        self.on_drag_move(
            &MouseMoveEvent {
                position: end,
                pressed_button: Some(MouseButton::Left),
                ..Default::default()
            },
            window,
            cx,
        );
        assert!(self.list_drag.as_ref().unwrap().indicator.is_some());
        self.on_mouse_up(
            &MouseUpEvent {
                button: MouseButton::Left,
                position: end,
                ..Default::default()
            },
            window,
            cx,
        );
        let expected = "98. other\n99. last\n100. café\n";
        assert_eq!(self.text.to_string(), expected);
        assert_eq!(self.text.slice(self.selected_range).to_string(), "café");
        assert!(self.selection_reversed);
        self.undo(&super::Undo, window, cx);
        assert_eq!(self.text.to_string(), original);
        self.redo(&super::Redo, window, cx);
        assert_eq!(self.text.to_string(), expected);
    }
    #[doc(hidden)]
    pub fn diagnostic_scrolled_list_drag(
        &mut self,
        phase: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let height = self.last_layout.as_ref().unwrap().line_height;
        if phase % 3 == 0 {
            self.selected_range = (self.text.len()..self.text.len()).into();
            self.last_selected_range = Some(self.selected_range);
            self.scroll_handle
                .set_offset(point(px(0.), -self.text_wrapper.lines.prefix(40) * height));
            cx.notify();
        } else if phase % 3 == 1 {
            let items = self.text_wrapper.list_items.clone();
            let source = if phase >= 3 { 40 } else { 42 };
            let start = self.list_marker_bounds(&items[source]).unwrap().center();
            let end = self.list_marker_bounds(&items[44]).unwrap().center() + point(px(0.), px(3.));
            assert!(self.input_bounds.contains(&start));
            assert!(self.input_bounds.contains(&end));
            let scroll = self.scroll_handle.offset();
            self.on_mouse_down(
                &MouseDownEvent {
                    button: MouseButton::Left,
                    position: start,
                    click_count: 1,
                    ..Default::default()
                },
                window,
                cx,
            );
            self.on_drag_move(
                &MouseMoveEvent {
                    position: end,
                    pressed_button: Some(MouseButton::Left),
                    ..Default::default()
                },
                window,
                cx,
            );
            self.on_mouse_up(
                &MouseUpEvent {
                    button: MouseButton::Left,
                    position: end,
                    ..Default::default()
                },
                window,
                cx,
            );
            assert!(self.list_drag.is_none());
            assert!(self.list_viewport_anchor.is_some());
            if phase < 3 {
                assert_eq!(
                    self.scroll_handle.offset(),
                    scroll,
                    "drop must not reveal the offscreen caret"
                );
            } else {
                let (anchor, screen_y, _) = self.list_viewport_anchor.unwrap();
                assert_eq!(
                    self.text
                        .slice_line(self.text.offset_to_point(anchor).row)
                        .to_string()
                        .trim(),
                    "- [ ] item 44"
                );
                assert_eq!(
                    self.scroll_handle.offset().y
                        + self
                            .text_wrapper
                            .lines
                            .prefix(self.text.offset_to_point(anchor).row)
                            * height,
                    screen_y
                );
            }
            assert_eq!(self.selected_range.start, self.text.len());
        } else {
            let (anchor, screen_y, _) = self
                .list_viewport_anchor
                .expect("anchor survives background analysis");
            let row = self.text.offset_to_point(anchor).row;
            assert!(
                (self.text_wrapper.lines.prefix(row) * height + self.scroll_handle.offset().y
                    - screen_y)
                    .abs()
                    < px(0.1)
            );
            let scroll = self.scroll_handle.offset();
            self.scroll_list_drag(cx);
            assert_eq!(
                self.scroll_handle.offset(),
                scroll,
                "released drags cannot edge-scroll"
            );
            let item = self.text_wrapper.list_items[42].clone();
            let start = self.list_marker_bounds(&item).unwrap().center();
            let end = point(start.x, self.input_bounds.bottom() - px(5.));
            self.on_mouse_down(
                &MouseDownEvent {
                    button: MouseButton::Left,
                    position: start,
                    click_count: 1,
                    ..Default::default()
                },
                window,
                cx,
            );
            self.on_drag_move(
                &MouseMoveEvent {
                    position: end,
                    pressed_button: Some(MouseButton::Left),
                    ..Default::default()
                },
                window,
                cx,
            );
            let origin = self.last_bounds.unwrap().origin;
            self.scroll_list_drag(cx);
            assert_eq!(self.scroll_handle.offset().y, scroll.y - px(10.));
            assert_eq!(self.last_bounds.unwrap().origin.y, origin.y - px(10.));
            assert!(self.list_drag.as_ref().unwrap().destination.is_some());
            self.escape(&super::Escape, window, cx);
            let stopped = self.scroll_handle.offset();
            self.scroll_list_drag(cx);
            assert_eq!(
                self.scroll_handle.offset(),
                stopped,
                "cancelled drags stop scrolling"
            );
        }
    }
    #[doc(hidden)]
    pub fn diagnostic_list_drag_checks(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let original = self.text.to_string();
        let items = self.text_wrapper.list_items.clone();
        assert_eq!(items.len(), 2);
        let start = self.list_marker_bounds(&items[0]).unwrap().center();
        let target = self.list_marker_bounds(&items[1]).unwrap();
        let end = target.center() + point(px(2.), px(3.));
        self.selected_range = (6..11).into();
        self.selection_reversed = true;
        let press = MouseDownEvent {
            button: MouseButton::Left,
            position: start,
            click_count: 1,
            ..Default::default()
        };
        self.on_mouse_down(&press, window, cx);
        assert_eq!(self.text.to_string(), original);
        assert!(self.list_drag.is_some());
        self.on_drag_move(
            &MouseMoveEvent {
                position: end,
                pressed_button: Some(MouseButton::Left),
                ..Default::default()
            },
            window,
            cx,
        );
        assert!(self.list_drag.as_ref().unwrap().indicator.is_some());
        self.on_mouse_up(
            &MouseUpEvent {
                button: MouseButton::Left,
                position: end,
                ..Default::default()
            },
            window,
            cx,
        );
        assert_eq!(self.text.to_string(), "\n* other\n* [ ] café\n");
        assert!(self.selection_reversed);
        assert_eq!(
            self.text
                .slice(self.selected_range.start..self.selected_range.end)
                .to_string(),
            "café"
        );
        self.undo(&super::Undo, window, cx);
        assert_eq!(self.text.to_string(), original);
        self.redo(&super::Redo, window, cx);
        assert_eq!(self.text.to_string(), "\n* other\n* [ ] café\n");
        self.undo(&super::Undo, window, cx);
        self.text_wrapper.diagnostic_settle();
        self.on_mouse_down(&press, window, cx);
        self.escape(&super::Escape, window, cx);
        assert!(self.list_drag.is_none());
        assert_eq!(self.text.to_string(), original);
        self.on_mouse_down(&press, window, cx);
        self.replace_selection_isolated("changed", window, cx);
        let changed = self.text.clone();
        self.on_mouse_up(
            &MouseUpEvent {
                button: MouseButton::Left,
                position: end,
                ..Default::default()
            },
            window,
            cx,
        );
        assert_eq!(self.text, changed);
    }
}
