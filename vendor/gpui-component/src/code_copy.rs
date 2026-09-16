//! Shared code-block copy control for Reading and Live preview.
use gpui::{
    App, ClipboardItem, ElementId, IntoElement, MouseButton, SharedString, div, prelude::*, px,
};

use crate::{
    ActiveTheme, IconName, Sizable,
    button::{Button, ButtonVariants},
};

pub(crate) fn button(
    id: impl Into<ElementId>,
    value: impl Fn(&App) -> Option<SharedString> + 'static,
    cx: &App,
) -> impl IntoElement {
    div()
        .id(id)
        .debug_selector(|| "code-copy".into())
        .size(px(24.))
        .rounded(cx.theme().radius)
        .bg(cx.theme().muted)
        .on_mouse_down(MouseButton::Left, |_, window, cx| {
            window.prevent_default();
            cx.stop_propagation();
        })
        .child(
            Button::new("copy")
                .icon(IconName::Copy)
                .ghost()
                .xsmall()
                .size(px(24.))
                .tooltip("Copy code")
                .on_click(move |_, _, cx| {
                    cx.stop_propagation();
                    if let Some(value) = value(cx) {
                        cx.write_to_clipboard(ClipboardItem::new_string(value.to_string()));
                    }
                }),
        )
}
