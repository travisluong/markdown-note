use super::*;
use gpui_component::{Disableable, Theme, ThemeMode, dialog::Dialog, switch::Switch};

impl Notes {
    pub(super) fn can_insert_date(&self) -> bool {
        self.document.is_some() && !self.busy && self.mode != ViewMode::Reading
    }

    pub(super) fn date_format(&self) -> &str {
        let value = self
            .notebooks
            .active
            .as_ref()
            .map_or(self.session_date_format.as_str(), |book| {
                book.settings.date_format.as_str()
            });
        if templates::current(value).is_ok() {
            value
        } else {
            "YYYY-MM-DD"
        }
    }

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
        self.settings_templates = false;
        self.date_validation = None;
        let date = self.date_format().to_owned();
        let date_input = cx.new(|cx| InputState::new(window, cx).default_value(date));
        self.date_subscription = Some(cx.subscribe(&date_input, |this, input, event, cx| {
            if !matches!(event, InputEvent::Change) {
                return;
            }
            let value = input.read(cx).value().to_string();
            match templates::current(&value) {
                Ok(_) if !this.busy => {
                    this.date_validation = None;
                    if let Some(book) = &mut this.notebooks.active {
                        book.settings.date_format = value;
                        this.persist_notebooks(cx);
                    } else {
                        this.session_date_format = value;
                    }
                }
                Err(error) => this.date_validation = Some(error),
                _ => {}
            }
            cx.notify();
        }));
        self.date_input = Some(date_input);
        let delay = self.autosave_settings().delay_ms.to_string();
        let input = cx.new(|cx| InputState::new(window, cx).default_value(delay));
        self.autosave_validation = None;
        self.autosave_subscription =
            Some(
                cx.subscribe_in(&input, window, |this, input, event, window, cx| {
                    if !matches!(event, InputEvent::Change) {
                        return;
                    }
                    let value = input.read(cx).value();
                    if let Some(delay_ms) = notebook::Autosave::parse_delay(&value) {
                        this.autosave_validation = None;
                        let mut settings = this.autosave_settings();
                        settings.delay_ms = delay_ms;
                        this.update_autosave(settings, window, cx);
                    } else {
                        this.autosave_validation =
                            Some("Enter a whole number from 100 to 60,000 ms.".into());
                    }
                    cx.notify();
                }),
            );
        self.autosave_delay = Some(input);
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
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .when(window.viewport_size().width < px(560.), |v| v.w_full())
                            .when(window.viewport_size().width >= px(560.), |v| v.flex_1())
                            .min_w_0()
                            .child(label),
                    )
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
                Switch::new("autosave-enabled")
                    .label("Autosave")
                    .checked(self.autosave_settings().enabled)
                    .on_click(cx.listener(|this, enabled: &bool, window, cx| {
                        let mut settings = this.autosave_settings();
                        settings.enabled = *enabled;
                        this.update_autosave(settings, window, cx);
                    })),
            )
            .child("Autosave delay (milliseconds)")
            .when_some(self.autosave_delay.clone(), |view, input| {
                view.child(Input::new(&input))
            })
            .when_some(self.autosave_validation.clone(), |view, message| {
                view.child(div().text_sm().text_color(cx.theme().danger).child(message))
            })
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
        let templates_body = div().w_full().min_w_0().flex().flex_col().gap_3()
            .child("Current date format")
            .when_some(self.date_input.clone(), |view, input| view.child(Input::new(&input)))
            .when_some(self.date_validation.clone(), |view, error| view.child(div().text_sm().text_color(cx.theme().danger).child(error)))
            .child(format!("Example: {}", templates::current(self.date_format()).unwrap_or_default()))
            .child(div().text_sm().child("Moment-style tokens, local time, English names. Use [brackets] for literal text. See README for supported tokens."));
        let sidebar = div()
            .w(px(104.))
            .flex_shrink_0()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                Button::new("settings-general")
                    .label("General")
                    .when(!self.settings_templates, |v| v.primary())
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.settings_templates = false;
                        cx.notify();
                    })),
            )
            .child(
                Button::new("settings-templates")
                    .label("Templates")
                    .when(self.settings_templates, |v| v.primary())
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.settings_templates = true;
                        cx.notify();
                    })),
            );
        dialog
            .title("Settings")
            .w(px(620.).min(window.viewport_size().width - px(32.)))
            .max_h((window.viewport_size().height - px(100.)).max(px(120.)))
            .child(
                div()
                    .w_full()
                    .min_w_0()
                    .flex()
                    .gap_3()
                    .child(sidebar)
                    .child(div().flex_1().min_w_0().child(if self.settings_templates {
                        templates_body.into_any_element()
                    } else {
                        body.w_full().min_w_0().into_any_element()
                    })),
            )
    }
}
