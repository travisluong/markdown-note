use super::*;
use gpui_component::resizable::{ResizableState, h_resizable, resizable_panel};
use gpui_component::{IconName, PixelsExt, Selectable};

impl Notes {
    pub(super) fn reveal_sidebar(&mut self, cx: &mut Context<Self>) {
        if self.sidebar_collapsed {
            self.sidebar_collapsed = false;
            self.reset_sidebar_resize(cx);
        }
    }

    pub(super) fn toggle_sidebar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || window.has_active_dialog(cx) || self.notebooks.active.is_none() {
            return;
        }
        if self.sidebar_collapsed {
            self.reveal_sidebar(cx);
            if self.search.as_ref().is_some_and(|panel| panel.visible) {
                self.open_search_all(window, cx);
            } else {
                self.show_explorer(window, cx);
            }
        } else {
            self.sidebar_collapsed = true;
            if self.mode == ViewMode::Reading || self.document.is_none() {
                self.focus.focus(window);
            } else {
                self.editor
                    .update(cx, |editor, cx| editor.focus(window, cx));
            }
        }
        cx.notify();
    }

    pub(super) fn show_explorer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || window.has_active_dialog(cx) || self.notebooks.active.is_none() {
            return;
        }
        self.reveal_sidebar(cx);
        if let Some(panel) = &mut self.search {
            panel.visible = false;
        }
        self.notebooks.tree_focus.focus(window);
        cx.notify();
    }

    fn render_sidebar_toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let searching = self.search.as_ref().is_some_and(|panel| panel.visible);
        div()
            .id("sidebar-toolbar")
            .drag_over::<notebook_ui::EntryDrag>(|view, _, _, cx| view.bg(cx.theme().accent))
            .on_drop(
                cx.listener(|this, drag: &notebook_ui::EntryDrag, window, cx| {
                    cx.stop_propagation();
                    this.drop_entry(drag, PathBuf::new(), None, window, cx);
                }),
            )
            .flex()
            .items_center()
            .flex_shrink_0()
            .gap_1()
            .p_1()
            .child(
                Button::new("sidebar-explorer")
                    .small()
                    .ghost()
                    .icon(IconName::Folder)
                    .tooltip("Explorer")
                    .selected(!searching)
                    .on_click(cx.listener(|this, _, window, cx| this.show_explorer(window, cx))),
            )
            .child(
                Button::new("sidebar-search")
                    .small()
                    .ghost()
                    .icon(IconName::Search)
                    .tooltip("Search all files (Ctrl+Shift+F)")
                    .selected(searching)
                    .on_click(cx.listener(|this, _, window, cx| this.open_search_all(window, cx))),
            )
    }
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
        if self.sidebar_collapsed {
            return editor.into_any_element();
        }
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
        let sidebar = div()
            .flex()
            .flex_col()
            .size_full()
            .min_w_0()
            .overflow_hidden()
            .border_r_1()
            .border_color(cx.theme().border)
            .child(self.render_sidebar_toolbar(cx))
            .child(div().flex_1().min_h_0().overflow_hidden().child(sidebar));
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
