//! Thin application adapter around the vendored document editor.
//! The editor owns current text/history. Session policy owns document identity,
//! saved baselines and permission to edit; this adapter owns no second buffer.
use crate::ViewMode;
use gpui::{
    App, AppContext, Context, Entity, EntityId, FocusHandle, Focusable, Pixels, Point,
    SharedString, Subscription, Window,
};
use gpui_component::input::{Input, InputEvent, InputState, Rope};
use std::{ops::Range, path::PathBuf};

/// Only events the application handles. Navigation remains an application action.
pub enum Event<'a> {
    Changed,
    OpenLink(&'a str),
}

#[derive(Clone)]
pub struct Editor {
    input: Entity<InputState>,
}

impl Editor {
    pub fn new(text: impl Into<SharedString>, window: &mut Window, cx: &mut App) -> Self {
        let text = text.into();
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .multi_line(true)
                .searchable(true)
                .default_value(text)
        });
        Self { input }
    }

    pub fn has_shared_buffer(&self, cx: &App) -> bool {
        self.input.read(cx).has_shared_buffer()
    }

    pub fn shared_buffer(&self, cx: &mut App) -> Entity<gpui_component::input::InputBuffer> {
        self.input.update(cx, |input, cx| input.shared_buffer(cx))
    }
    pub fn from_shared_buffer(
        buffer: Entity<gpui_component::input::InputBuffer>,
        window: &mut Window,
        cx: &mut App,
    ) -> Self {
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .multi_line(true)
                .searchable(true)
                .with_shared_buffer(buffer, cx)
        });
        Self { input }
    }
    pub fn break_undo_group(&self, cx: &mut App) {
        self.input.update(cx, |input, _| input.break_undo_group());
    }

    pub fn subscribe<T: 'static>(
        &self,
        window: &Window,
        cx: &mut Context<T>,
        handler: impl Fn(&mut T, Event<'_>, &mut Window, &mut Context<T>) + 'static,
    ) -> Subscription {
        cx.subscribe_in(
            &self.input,
            window,
            move |owner, _, event, window, cx| match event {
                InputEvent::Change => handler(owner, Event::Changed, window, cx),
                InputEvent::OpenLink { destination } => {
                    handler(owner, Event::OpenLink(destination), window, cx)
                }
                _ => {}
            },
        )
    }

    /// Cheap immutable rope capture, shared with save and search workers.
    pub fn snapshot(&self, cx: &App) -> Rope {
        self.input.read(cx).snapshot()
    }
    /// Materializes the entire source; use snapshot() for a cheap immutable capture.
    pub fn text(&self, cx: &App) -> SharedString {
        self.input.read(cx).value()
    }
    /// Selection and edit ranges always use UTF-8 source bytes.
    pub fn selection(&self, cx: &App) -> Range<usize> {
        self.input.read(cx).source_selection()
    }
    pub fn id(&self) -> EntityId {
        self.input.entity_id()
    }
    pub fn widget(&self) -> Input {
        Input::new(&self.input)
    }
    pub fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.input.focus_handle(cx)
    }
    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        self.input.update(cx, |input, cx| input.focus(window, cx));
    }
    pub fn is_focused(&self, window: &Window, cx: &App) -> bool {
        self.focus_handle(cx).is_focused(window)
    }
    pub fn set_mode(&self, mode: ViewMode, cx: &mut App) {
        self.input.update(cx, |input, cx| {
            input.set_live_preview(mode == ViewMode::LivePreview, cx)
        });
    }
    pub fn set_document_path(&self, path: Option<PathBuf>, cx: &mut App) {
        self.input
            .update(cx, |input, cx| input.set_image_document(path, cx));
    }
    pub fn select(&self, range: Range<usize>, window: &mut Window, cx: &mut App) -> bool {
        self.input
            .update(cx, |input, cx| input.select_source_range(range, window, cx))
    }
    /// Highlight and reveal a notebook match while keyboard focus stays in search.
    pub fn reveal_match(&self, range: Range<usize>, cx: &mut App) -> bool {
        self.input.update(cx, |input, cx| {
            input.select_source_range_without_focus(range, cx)
        })
    }
    pub fn replace_selection(&self, text: &str, window: &mut Window, cx: &mut App) {
        self.input.update(cx, |input, cx| {
            input.replace_selection_isolated(text, window, cx)
        });
    }
    /// Format the remembered UTF-8 selection in one normal isolated editor edit.
    pub fn format_markdown(
        &self,
        format: crate::markdown_edit::Format,
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        self.input.update(cx, |input, cx| {
            if input.is_disabled() {
                return false;
            }
            let source = input.value();
            let Some(edit) =
                crate::markdown_edit::transform(&source, input.source_selection(), format)
            else {
                return false;
            };
            if !input.select_source_range(edit.range, window, cx) {
                return false;
            }
            input.replace_selection_isolated(&edit.text, window, cx);
            input.select_source_range(edit.selection, window, cx);
            input.focus(window, cx);
            true
        })
    }
    /// Apply one isolated transaction only to the captured source. Failed guards
    /// leave text, selection and history unchanged. The caller checks document ID.
    pub fn replace_range_if_current(
        &self,
        range: Range<usize>,
        expected: &Rope,
        text: &str,
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        self.input.update(cx, |input, cx| {
            if input.is_disabled()
                || input.snapshot() != *expected
                || !input.select_source_range(range, window, cx)
            {
                return false;
            }
            input.replace_selection_isolated(text, window, cx);
            true
        })
    }
    pub fn drop_offset(&self, position: Point<Pixels>, cx: &App) -> Option<usize> {
        self.input.read(cx).drop_offset(position)
    }
    pub fn insert_at_snapshot(
        &self,
        offset: usize,
        expected: &Rope,
        text: &str,
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        self.input.update(cx, |input, cx| {
            let inserted = input.insert_at_snapshot(offset, expected, text, window, cx);
            if inserted {
                input.focus(window, cx);
            }
            inserted
        })
    }
    pub fn toggle_selected_tasks(&self, window: &mut Window, cx: &mut App) {
        self.input
            .update(cx, |input, cx| input.toggle_selected_tasks(window, cx));
    }
    pub fn move_lines(&self, down: bool, window: &mut Window, cx: &mut App) {
        self.input
            .update(cx, |input, cx| input.move_lines(down, window, cx));
    }
    pub fn toggle_task(
        &self,
        offset: usize,
        rendered_source: &str,
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        self.input.update(cx, |input, cx| {
            input.toggle_markdown_task(offset, rendered_source, window, cx)
        })
    }
    pub fn find(&self, window: &mut Window, cx: &mut App) {
        self.input
            .update(cx, |input, cx| input.open_search(window, cx));
    }
    pub fn find_and_replace(&self, window: &mut Window, cx: &mut App) {
        self.input
            .update(cx, |input, cx| input.open_search_replace(window, cx));
    }

    /// Preserve an explicitly configured input used by existing benchmark scripts.
    /// Production installation always constructs a fresh editor with new.
    #[cfg(feature = "editor-diagnostics")]
    #[doc(hidden)]
    pub fn diagnostic_from_input(input: Entity<InputState>) -> Self {
        Self { input }
    }

    /// Escape hatch for TestPlatform and opt-in editor diagnostics. Production
    /// feature edits use the semantic methods above, not unrestricted callbacks.
    #[doc(hidden)]
    pub fn diagnostic_input(&self) -> &Entity<InputState> {
        &self.input
    }
}

#[cfg(test)]
mod tests;
