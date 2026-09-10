use super::*;
use gpui_component::PixelsExt;
use gpui_component::resizable::{ResizableState, h_resizable, resizable_panel};

impl Notes {
    pub(super) fn reset_sidebar_resize(&mut self, cx: &mut Context<Self>) {
        self.sidebar_resize = cx.new(|_| ResizableState::default());
    }

    pub(super) fn save_sidebar_width(&mut self, width: f32, cx: &mut Context<Self>) {
        let Some(book) = self.notebooks.active.as_mut() else {
            return;
        };
        let width = notebook::normalize_sidebar_width(width);
        if book.settings.sidebar_width != width {
            book.settings.sidebar_width = width;
            self.persist_notebooks(cx);
        }
    }

    pub(super) fn render_sidebar_layout(
        &mut self,
        content: AnyElement,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let editor = div()
            .flex_1()
            .min_w_0()
            .h_full()
            .overflow_hidden()
            .child(content);
        let Some(book) = self.notebooks.active.as_ref() else {
            return editor.into_any_element();
        };
        let max_width = (window.viewport_size().width.as_f32() - 200.).clamp(160., 600.);
        let width = notebook::normalize_sidebar_width(book.settings.sidebar_width).min(max_width);
        let sidebar = if self.search.as_ref().is_some_and(|panel| panel.visible) {
            self.render_search_panel(cx).into_any_element()
        } else {
            self.render_sidebar(window, cx).into_any_element()
        };
        h_resizable("sidebar-layout")
            .with_state(&self.sidebar_resize)
            .child(
                resizable_panel()
                    .size(px(width))
                    .size_range(px(160.)..px(max_width))
                    .child(sidebar),
            )
            .child(
                resizable_panel()
                    .size_range(px(200.)..Pixels::MAX)
                    .child(editor),
            )
            .on_resize(cx.listener(|this, state: &Entity<ResizableState>, _, cx| {
                if let Some(width) = state.read(cx).sizes().first().copied() {
                    this.save_sidebar_width(width.as_f32(), cx);
                }
            }))
            .into_any_element()
    }
}
