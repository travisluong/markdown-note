use super::*;
use gpui_component::{
    Colorize,
    color_picker::{ColorPicker, ColorPickerEvent, ColorPickerState},
};
use gpui_component::{Disableable, Theme, ThemeMode, dialog::Dialog, switch::Switch};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum SettingsSection {
    General,
    Templates,
    Theme,
}

impl Notes {
    pub(super) fn nested_list_letters(&self) -> bool {
        self.notebooks
            .active
            .as_ref()
            .map_or(self.session_nested_list_letters, |book| {
                book.settings.nested_list_letters
            })
    }
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
        self.editor.update(cx, |editor, _| {
            editor.set_nested_list_letters(self.nested_list_letters())
        });
        Theme::change(
            if appearance.theme_choice(self.startup_dark) != notebook::ThemeChoice::Light {
                ThemeMode::Dark
            } else {
                ThemeMode::Light
            },
            Some(window),
            cx,
        );
        let theme = Theme::global_mut(cx);
        apply_theme_colors(theme, appearance, self.startup_dark);
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
        let colors = [cx.theme().background, cx.theme().foreground];
        for (picker, color) in self.theme_pickers.iter().zip(colors) {
            picker.update(cx, |picker, cx| picker.set_value(color, window, cx));
        }
    }

    pub(super) fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || window.has_active_dialog(cx) {
            return;
        }
        self.menu = None;
        self.settings_section = SettingsSection::General;
        self.theme_pickers.clear();
        self.theme_subscriptions.clear();
        for (index, color) in [cx.theme().background, cx.theme().foreground]
            .into_iter()
            .enumerate()
        {
            let picker = cx.new(|cx| ColorPickerState::new(window, cx).default_value(color));
            self.theme_subscriptions.push(cx.subscribe_in(
                &picker,
                window,
                move |this, _, event: &ColorPickerEvent, window, cx| {
                    let ColorPickerEvent::Change(color) = event;
                    let value =
                        color.and_then(|color| u32::from_str_radix(&color.to_hex()[1..7], 16).ok());
                    let mut appearance = this.appearance();
                    if index == 0 {
                        appearance.background_color = value;
                    } else {
                        appearance.font_color = value;
                    }
                    this.update_appearance(appearance, window, cx);
                },
            ));
            self.theme_pickers.push(picker);
        }
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
                Switch::new("nested-list-letters")
                    .label("Use letters for nested ordered lists (a., b., …)")
                    .checked(self.nested_list_letters())
                    .disabled(self.busy)
                    .on_click(cx.listener(|this, letters: &bool, _, cx| {
                        if let Some(book) = &mut this.notebooks.active {
                            book.settings.nested_list_letters = *letters;
                            this.persist_notebooks(cx);
                        } else {
                            this.session_nested_list_letters = *letters;
                        }
                        this.editor
                            .update(cx, |editor, _| editor.set_nested_list_letters(*letters));
                        cx.notify();
                    })),
            )
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
        let mut theme_body = div().flex().flex_col().gap_3().child("Theme");
        let mut selector = div().flex().flex_wrap().gap_2();
        for (index, choice, label) in [
            (0usize, notebook::ThemeChoice::Light, "Light"),
            (1, notebook::ThemeChoice::Dark, "Dark"),
            (2, notebook::ThemeChoice::DarkGray, "Dark Gray"),
        ] {
            selector = selector.child(
                Button::new(("theme-choice", index))
                    .label(label)
                    .when(
                        appearance.theme_choice(self.startup_dark) == choice,
                        |button| button.primary(),
                    )
                    .disabled(self.busy)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        let mut settings = this.appearance();
                        settings.theme = Some(choice);
                        this.update_appearance(settings, window, cx);
                    })),
            );
        }
        theme_body = theme_body.child(selector);
        for (picker, label) in self
            .theme_pickers
            .iter()
            .zip(["Default background color", "Default font color"])
        {
            theme_body = theme_body.child(ColorPicker::new(picker).label(label));
        }
        theme_body = theme_body.child(
            Button::new("reset-theme-colors")
                .label("Reset colors to theme defaults")
                .on_click(cx.listener(|this, _, window, cx| {
                    let mut settings = this.appearance();
                    settings.background_color = None;
                    settings.font_color = None;
                    this.update_appearance(settings, window, cx);
                })),
        );
        let sidebar = div()
            .w(px(104.))
            .flex_shrink_0()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                Button::new("settings-general")
                    .label("General")
                    .when(self.settings_section == SettingsSection::General, |v| {
                        v.primary()
                    })
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.settings_section = SettingsSection::General;
                        cx.notify();
                    })),
            )
            .child(
                Button::new("settings-templates")
                    .label("Templates")
                    .when(self.settings_section == SettingsSection::Templates, |v| {
                        v.primary()
                    })
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.settings_section = SettingsSection::Templates;
                        cx.notify();
                    })),
            );
        let sidebar = sidebar.child(
            Button::new("settings-theme")
                .label("Theme")
                .when(self.settings_section == SettingsSection::Theme, |button| {
                    button.primary()
                })
                .on_click(cx.listener(|this, _, _, cx| {
                    this.settings_section = SettingsSection::Theme;
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
                    .child(div().flex_1().min_w_0().child(
                        if self.settings_section == SettingsSection::Theme {
                            theme_body.into_any_element()
                        } else if self.settings_section == SettingsSection::Templates {
                            templates_body.into_any_element()
                        } else {
                            body.w_full().min_w_0().into_any_element()
                        },
                    )),
            )
    }
}

fn apply_theme_colors(theme: &mut Theme, appearance: notebook::Appearance, startup_dark: bool) {
    if appearance.theme_choice(startup_dark) == notebook::ThemeChoice::DarkGray {
        theme.background = rgb(0x303030).into();
        theme.foreground = rgb(0xe6e6e6).into();
        theme.popover = rgb(0x383838).into();
        theme.muted = rgb(0x404040).into();
        theme.muted_foreground = rgb(0xb8b8b8).into();
        theme.border = rgb(0x555555).into();
        theme.accent = rgb(0x4b4b4b).into();
        theme.secondary = rgb(0x414141).into();
        theme.list_hover = theme.accent;
        theme.list_active = theme.accent;
        theme.tab = theme.background;
        theme.tab_bar = theme.background;
        theme.tab_active = theme.accent;
        theme.table = theme.background;
        theme.table_even = rgb(0x353535).into();
        theme.table_head = theme.muted;
        theme.selection = rgb(0x506680).into();
    }
    if let Some(color) = appearance.background_color {
        theme.background = rgb(color).into();
    }
    if let Some(color) = appearance.font_color {
        theme.foreground = rgb(color).into();
    }
    theme.sidebar = theme.background;
    theme.list = theme.background;
    theme.accordion = theme.background;
    theme.group_box = theme.background;
    theme.popover_foreground = theme.foreground;
    theme.sidebar_foreground = theme.foreground;
    theme.group_box_foreground = theme.foreground;
    theme.secondary_foreground = theme.foreground;
    theme.accent_foreground = theme.foreground;
    theme.caret = theme.foreground;
    let background = theme.background;
    let foreground = theme.foreground;
    let highlight = std::sync::Arc::make_mut(&mut theme.highlight_theme);
    highlight.style.editor_background = Some(background);
    highlight.style.editor_foreground = Some(foreground);
}

#[cfg(test)]
mod theme_tests {
    use super::{Theme, apply_theme_colors, notebook};
    use gpui::rgb;
    use notebook::{Appearance, ThemeChoice};
    #[test]
    fn themes_legacy_preferences_and_custom_colors() {
        let legacy: Appearance = serde_json::from_str(r#"{"dark_mode":true}"#).unwrap();
        assert_eq!(legacy.theme_choice(false), ThemeChoice::Dark);
        assert_eq!(
            Appearance::default().theme_choice(false),
            ThemeChoice::Light
        );
        let appearance = Appearance {
            theme: Some(ThemeChoice::DarkGray),
            ..legacy
        };
        let mut theme = Theme::default();
        apply_theme_colors(&mut theme, appearance, false);
        assert_eq!(theme.background, rgb(0x303030).into());
        assert_eq!(theme.foreground, rgb(0xe6e6e6).into());
        let custom = Appearance {
            background_color: Some(0x123456),
            font_color: Some(0xabcdef),
            ..appearance
        };
        let encoded = serde_json::to_string(&custom).unwrap();
        assert_eq!(
            serde_json::from_str::<Appearance>(&encoded).unwrap(),
            custom
        );
        apply_theme_colors(&mut theme, custom, false);
        assert_eq!(theme.background, rgb(0x123456).into());
        assert_eq!(theme.foreground, rgb(0xabcdef).into());
        assert_eq!(
            theme.highlight_theme.style.editor_background,
            Some(theme.background)
        );
        assert_eq!(
            theme.highlight_theme.style.editor_foreground,
            Some(theme.foreground)
        );
        apply_theme_colors(&mut theme, appearance, false);
        assert_eq!(theme.background, rgb(0x303030).into());
        assert_eq!(
            Appearance {
                font_color: Some(u32::MAX),
                ..appearance
            }
            .normalized()
            .font_color,
            None
        );
    }
}
