use super::*;
use gpui_component::{Disableable, Theme, ThemeMode, dialog::Dialog, switch::Switch};

impl Notes {
    pub(super) fn appearance(&self) -> notebook::Appearance {
        let mut appearance = self
            .notebooks
            .active
            .as_ref()
            .map_or(self.session_appearance, |book| book.settings.appearance)
            .normalized();
        appearance.dark_mode = Some(appearance.dark_mode.unwrap_or(self.startup_dark));
        appearance
    }

    pub(super) fn apply_appearance(&self, window: &mut Window, cx: &mut Context<Self>) {
        let appearance = self.appearance();
        Theme::change(
            if appearance.dark_mode == Some(true) {
                ThemeMode::Dark
            } else {
                ThemeMode::Light
            },
            Some(window),
            cx,
        );
        let theme = Theme::global_mut(cx);
        theme.font_size = px(appearance.ui_font_size as f32);
        theme.mono_font_size = px(appearance.content_font_size as f32);
        window.set_rem_size(px(appearance.ui_font_size as f32));
        window.refresh();
        cx.notify();
    }

    pub(super) fn update_appearance(
        &mut self,
        appearance: notebook::Appearance,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.busy {
            return;
        }
        let appearance = appearance.normalized();
        if let Some(book) = &mut self.notebooks.active {
            book.settings.appearance = appearance;
            self.persist_notebooks(cx);
        } else {
            self.session_appearance = appearance;
        }
        self.apply_appearance(window, cx);
    }

    pub(super) fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || window.has_active_dialog(cx) {
            return;
        }
        self.menu = None;
        let owner = cx.weak_entity();
        window.open_dialog(cx, move |dialog, window, cx| {
            owner
                .update(cx, |this, cx| this.settings_dialog(dialog, window, cx))
                .unwrap_or_else(|_| Dialog::new(window, cx))
        });
        cx.notify();
    }

    fn settings_dialog(
        &mut self,
        dialog: Dialog,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Dialog {
        let appearance = self.appearance();
        let subtitle = self.notebooks.active.as_ref().map_or_else(
            || "Changes apply immediately for this session.".to_owned(),
            |book| format!("Changes are saved for {}.", notebook::name(&book.root)),
        );
        let mut body = div().flex().flex_col().gap_3().child(
            div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(subtitle),
        );
        for (index, label, value, maximum) in [
            (
                0usize,
                "Content font size",
                appearance.content_font_size,
                40,
            ),
            (1usize, "UI font size", appearance.ui_font_size, 24),
        ] {
            body = body.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(div().flex_1().child(label))
                    .child(
                        Button::new(("font-decrease", index))
                            .label("−")
                            .disabled(self.busy || value <= 10)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                let mut settings = this.appearance();
                                if index == 0 {
                                    settings.content_font_size =
                                        settings.content_font_size.saturating_sub(1);
                                } else {
                                    settings.ui_font_size = settings.ui_font_size.saturating_sub(1);
                                }
                                this.update_appearance(settings, window, cx);
                            })),
                    )
                    .child(
                        div()
                            .min_w(px(48.))
                            .text_center()
                            .child(format!("{value} px")),
                    )
                    .child(
                        Button::new(("font-increase", index))
                            .label("+")
                            .disabled(self.busy || value >= maximum)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                let mut settings = this.appearance();
                                if index == 0 {
                                    settings.content_font_size += 1;
                                } else {
                                    settings.ui_font_size += 1;
                                }
                                this.update_appearance(settings, window, cx);
                            })),
                    ),
            );
        }
        body = body
            .child(
                Switch::new("dark-mode")
                    .label("Dark mode")
                    .checked(appearance.dark_mode == Some(true))
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, checked: &bool, window, cx| {
                        let mut settings = this.appearance();
                        settings.dark_mode = Some(*checked);
                        this.update_appearance(settings, window, cx);
                    })),
            )
            .when_some(self.notebooks.notice.clone(), |view, notice| {
                view.child(div().text_sm().child(notice))
            })
            .child(
                Button::new("close-settings")
                    .label("Close")
                    .on_click(|_, window, cx| window.close_dialog(cx)),
            );
        dialog
            .title("Settings")
            .w(px(460.).min(window.viewport_size().width - px(32.)))
            .max_h((window.viewport_size().height - px(100.)).max(px(120.)))
            .child(body)
    }
}
