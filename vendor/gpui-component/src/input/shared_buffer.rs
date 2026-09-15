//! A document buffer shared by independent input views. View ropes are immutable
//! snapshots; edits are published synchronously before another view can edit.
use std::{cell::RefCell, rc::Rc, time::Duration};

use gpui::{AppContext, Context, Entity, EventEmitter, WeakEntity, Window};

use super::{InputState, Rope, change::Change};
use crate::history::History;

#[derive(Clone)]
pub(super) struct SharedHistory(Rc<RefCell<History<Change>>>);

impl SharedHistory {
    pub(super) fn new() -> Self {
        Self(Rc::new(RefCell::new(History::new())))
    }
    pub(super) fn group_interval(self, interval: Duration) -> Self {
        let history = std::mem::replace(&mut *self.0.borrow_mut(), History::new());
        *self.0.borrow_mut() = history.group_interval(interval);
        self
    }
    pub(super) fn ignoring(&self) -> bool {
        self.0.borrow().ignore
    }
    pub(super) fn set_ignore(&self, ignore: bool) {
        self.0.borrow_mut().ignore = ignore;
    }
    pub(super) fn push(&self, change: Change) {
        self.0.borrow_mut().push(change);
    }
    pub(super) fn clear_redo(&self) {
        self.0.borrow_mut().clear_redo();
    }
    #[cfg(feature = "editor-diagnostics")]
    pub(super) fn clear(&self) {
        self.0.borrow_mut().clear();
    }
    pub(super) fn undo(&self) -> Option<Vec<Change>> {
        self.0.borrow_mut().undo()
    }
    pub(super) fn redo(&self) -> Option<Vec<Change>> {
        self.0.borrow_mut().redo()
    }
    pub(super) fn start_grouping(&self) {
        self.0.borrow_mut().start_grouping();
    }
    pub(super) fn end_grouping(&self) {
        self.0.borrow_mut().end_grouping();
    }
    pub(super) fn boundary(&self) {
        self.0.borrow_mut().break_group();
    }
    #[cfg(feature = "editor-diagnostics")]
    pub(super) fn undos(&self) -> Vec<Change> {
        self.0.borrow().undos().clone()
    }
}

pub enum InputBufferEvent {
    Changed,
}

impl EventEmitter<InputBufferEvent> for InputBuffer {}

pub struct InputBuffer {
    pub(super) text: Rope,
    history: SharedHistory,
    views: Vec<WeakEntity<InputState>>,
}

impl InputState {
    pub fn has_shared_buffer(&self) -> bool {
        self.buffer.is_some()
    }

    /// Attach a new view to the exact in-memory buffer and shared undo history.
    pub fn shared_buffer(&mut self, cx: &mut Context<Self>) -> Entity<InputBuffer> {
        if let Some(buffer) = &self.buffer {
            return buffer.clone();
        }
        let view = cx.entity().downgrade();
        let buffer = cx.new(|_| InputBuffer {
            text: self.text.clone(),
            history: self.history.clone(),
            views: vec![view],
        });
        self.buffer = Some(buffer.clone());
        buffer
    }

    pub fn with_shared_buffer(
        mut self,
        buffer: Entity<InputBuffer>,
        cx: &mut Context<Self>,
    ) -> Self {
        self.text = buffer.read(cx).text.clone();
        self.history = buffer.read(cx).history.clone();
        let view = cx.entity().downgrade();
        buffer.update(cx, |buffer, _| buffer.views.push(view));
        self.text_wrapper.set_default_text(&self.text);
        self._pending_update = true;

        self.buffer = Some(buffer);
        self
    }

    pub fn break_undo_group(&mut self) {
        self.list_enter_boundary = false;
        self.history = self.history.clone().group_interval(Duration::from_secs(1));
        self.history.boundary();
    }

    pub(super) fn publish_buffer_edit(
        &mut self,
        range: std::ops::Range<usize>,
        inserted: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(buffer) = self.buffer.clone() else {
            return;
        };
        let views = buffer.update(cx, |buffer, cx| {
            buffer.text = self.text.clone();
            cx.emit(InputBufferEvent::Changed);
            buffer.views.retain(|view| view.upgrade().is_some());
            buffer.views.clone()
        });
        for view in views {
            if view.entity_id() == cx.entity_id() {
                continue;
            }
            let _ = view.update(cx, |view, cx| {
                view.accept_buffer_edit(self.text.clone(), range.clone(), inserted, window, cx);
            });
        }
    }
}
