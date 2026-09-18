//! Session-only terminal placeholder. No terminal process or input is created.
use super::*;
use crate::toolbar_ui::toolbar_icon_button;

impl Notes {
    pub(super) fn toggle_right_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.document_interaction_blocked() || window.has_active_dialog(cx) {
            return;
        }
        self.right_panel_open = !self.right_panel_open;
        self.reset_sidebar_resize(cx);
        if !self.right_panel_open {
            if self.tabs.active.mode == ViewMode::Reading || self.document().is_none() {
                self.focus.focus(window);
            } else {
                self.tabs.active.editor.focus(window, cx);
            }
        }
        cx.notify();
    }

    pub(super) fn render_right_panel(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let blocked = self.document_interaction_blocked() || window.has_active_dialog(cx);
        let icon_size = self.toolbar_settings().icon_size;
        div()
            .id("right-panel")
            .debug_selector(|| "right-panel".into())
            .size_full()
            .min_w_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(
                div()
                    .h(px(icon_size.button_pixels() + 9.))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .p(px(4.))
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_sm()
                            .overflow_hidden()
                            .child("Terminal"),
                    )
                    .child(
                        toolbar_icon_button(
                            "right-panel-close",
                            IconName::Close,
                            icon_size,
                            blocked,
                        )
                        .tooltip("Close terminal panel")
                        .on_click(
                            cx.listener(|this, _, window, cx| this.toggle_right_panel(window, cx)),
                        ),
                    ),
            )
            .child(
                div()
                    .p_3()
                    .text_sm()
                    .whitespace_normal()
                    .text_color(cx.theme().muted_foreground)
                    .child("Terminal is not implemented yet."),
            )
    }
}
