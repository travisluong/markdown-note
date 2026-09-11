use super::*;
use gpui_component::{
    Colorize,
    color_picker::{ColorPicker, ColorPickerEvent, ColorPickerState},
};
use gpui_component::{Disableable, Theme, ThemeMode, dialog::Dialog, switch::Switch};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum SettingsSection {
    General,
    Font,
    Templates,
    Theme,
}

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
            .map_or_else(|| self.session_appearance.clone(), |book| book.settings.appearance.clone())
            .resolved(self.startup_dark);
        appearance.dark_mode = Some(appearance.dark_mode.unwrap_or(self.startup_dark));
        appearance
    }

    pub(super) fn apply_appearance(&self, window: &mut Window, cx: &mut Context<Self>) {
        let appearance = self.appearance();
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
        apply_theme_colors(theme, appearance.clone(), self.startup_dark);
        theme.font_family = resolve_font(appearance.interface_font.as_deref(), &self.installed_fonts, ".SystemUIFont");
        theme.mono_font_family = resolve_font(appearance.monospace_font.as_deref(), &self.installed_fonts, "Consolas");
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
        let appearance = appearance.resolved(self.startup_dark);
        if let Some(book) = &mut self.notebooks.active {
            book.settings.appearance = appearance;
            self.persist_notebooks(cx);
        } else {
            self.session_appearance = appearance;
        }
        self.apply_appearance(window, cx);
        let colors = [
            cx.theme().background,
            cx.theme().foreground,
            cx.theme().primary,
        ];
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
        for (index, color) in [
            cx.theme().background,
            cx.theme().foreground,
            cx.theme().primary,
        ]
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
                    let Some(value) = value else {
                        return;
                    };
                    let mut palette = appearance.palette(this.startup_dark);
                    match index {
                        0 => palette.background = value,
                        1 => palette.font = value,
                        _ => palette.primary = value,
                    }
                    appearance.set_palette(this.startup_dark, palette);
                    this.update_appearance(appearance, window, cx);
                },
            ));
            self.theme_pickers.push(picker);
        }
        self.initialize_font_pickers(window, cx);
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
        let mut font_body = div().flex().flex_col().gap_3();
        for (index, label) in ["Interface font", "Text font", "Monospace font"].into_iter().enumerate() {
            if let Some(picker) = self.font_pickers.get(index) {
                font_body = font_body.child(label).child(gpui_component::select::Select::new(picker));
            }
        }
        for (index, label, value, maximum) in [
            (
                0usize,
                "Content font size",
                appearance.content_font_size,
                40,
            ),
            (1usize, "UI font size", appearance.ui_font_size, 24),
        ] {
            font_body = font_body.child(
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
        for (picker, label) in
            self.theme_pickers
                .iter()
                .zip(["Background color", "Font color", "Primary color"])
        {
            theme_body = theme_body.child(ColorPicker::new(picker).label(label));
        }
        theme_body = theme_body.child(
            Button::new("reset-theme-colors")
                .label("Reset Light and Dark themes")
                .disabled(self.busy)
                .on_click(cx.listener(|this, _, window, cx| {
                    let mut settings = this.appearance();
                    settings.reset_palettes();
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
        let sidebar = sidebar.child(Button::new("settings-font").label("Font").when(self.settings_section == SettingsSection::Font, |b| b.primary()).on_click(cx.listener(|this, _, _, cx| { this.settings_section = SettingsSection::Font; cx.notify(); })));
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
                        } else if self.settings_section == SettingsSection::Font {
                            font_body.into_any_element()
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
    let palette = appearance.palette(startup_dark);
    theme.background = rgb(palette.background).into();
    theme.foreground = rgb(palette.font).into();
    theme.primary = rgb(palette.primary).into();
    theme.primary_hover = theme.primary;
    theme.primary_active = theme.primary;
    theme.link = theme.primary;
    theme.link_hover = theme.primary;
    theme.link_active = theme.primary;
    // Choose the higher-contrast black or white checkmark/button label.
    let linear = |channel: u32| {
        let c = channel as f64 / 255.;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    let luminance = 0.2126 * linear((palette.primary >> 16) & 255)
        + 0.7152 * linear((palette.primary >> 8) & 255)
        + 0.0722 * linear(palette.primary & 255);
    theme.primary_foreground = rgb(if luminance > 0.179 {
        0x000000
    } else {
        0xffffff
    })
    .into();
    theme.tab = theme.background;
    theme.tab_bar = theme.background;
    theme.table = theme.background;
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
        apply_theme_colors(&mut theme, appearance.clone(), false);
        assert_eq!(theme.background, rgb(0x303030).into());
        assert_eq!(theme.foreground, rgb(0xe6e6e6).into());
        let custom = Appearance {
            background_color: Some(0x123456),
            font_color: Some(0xabcdef),
            ..appearance.clone()
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
        apply_theme_colors(&mut theme, appearance.clone(), false);
        assert_eq!(theme.background, rgb(0x303030).into());
        assert_eq!(
            Appearance {
                font_color: Some(u32::MAX),
                ..appearance.clone()
            }
            .normalized()
            .font_color,
            None
        );
    }
}

#[cfg(test)]
mod palette_tests {
    use super::{Theme, apply_theme_colors, notebook};
    use gpui::rgb;
    use notebook::{Appearance, ThemeChoice, ThemePalette};

    #[test]
    fn independent_palettes_reset_and_invalid_colors() {
        let mut appearance = Appearance::default().resolved(false);
        let palette = ThemePalette {
            font: 0xabcdef,
            ..ThemePalette::LIGHT
        };
        appearance.set_palette(false, palette);
        assert_eq!(
            appearance.palette(false).primary,
            ThemePalette::LIGHT.primary
        );
        appearance.theme = Some(ThemeChoice::Dark);
        assert_eq!(appearance.palette(false), ThemePalette::DARK);
        appearance.set_palette(
            false,
            ThemePalette {
                primary: 0xff0000,
                ..ThemePalette::DARK
            },
        );
        appearance.theme = Some(ThemeChoice::Light);
        assert_eq!(appearance.palette(false), palette);
        let encoded = serde_json::to_string(&appearance).unwrap();
        assert_eq!(
            serde_json::from_str::<Appearance>(&encoded).unwrap(),
            appearance
        );
        appearance.ui_font_size = 20;
        appearance.reset_palettes();
        assert_eq!(appearance.theme, Some(ThemeChoice::Light));
        assert_eq!(appearance.ui_font_size, 20);
        assert_eq!(appearance.light, Some(ThemePalette::LIGHT));
        assert_eq!(appearance.dark, Some(ThemePalette::DARK));
        appearance.light = Some(ThemePalette {
            background: u32::MAX,
            font: u32::MAX,
            primary: u32::MAX,
        });
        assert_eq!(appearance.resolved(false).light, Some(ThemePalette::LIGHT));
    }

    #[test]
    fn migrate_legacy_colors_once_using_active_mode() {
        for (json, startup, choice, background, font) in [
            (
                r#"{"dark_mode":true}"#,
                false,
                ThemeChoice::Dark,
                0x202020,
                0xe6e6e6,
            ),
            (
                r#"{"theme":"dark_gray"}"#,
                false,
                ThemeChoice::Dark,
                0x303030,
                0xe6e6e6,
            ),
            (
                r#"{"theme":"light","background_color":1193046,"font_color":11259375}"#,
                true,
                ThemeChoice::Light,
                0x123456,
                0xabcdef,
            ),
            (
                r#"{"font_color":11259375}"#,
                true,
                ThemeChoice::Dark,
                0x202020,
                0xabcdef,
            ),
        ] {
            let appearance = serde_json::from_str::<Appearance>(json)
                .unwrap()
                .resolved(startup);
            assert_eq!(appearance.theme_choice(startup), choice);
            assert_eq!(
                appearance.palette(startup),
                ThemePalette {
                    background,
                    font,
                    primary: font
                }
            );
            let encoded = serde_json::to_string(&appearance).unwrap();
            assert!(!encoded.contains("background_color"));
            assert!(!encoded.contains("dark_gray"));
            assert_eq!(appearance.clone().resolved(startup), appearance);
            assert_eq!(
                serde_json::from_str::<Appearance>(&encoded).unwrap(),
                appearance
            );
        }
    }

    #[test]
    fn colors_reach_editor_checkboxes_and_links() {
        let mut theme = Theme::default();
        for (choice, palette, checkmark) in [
            (ThemeChoice::Light, ThemePalette::LIGHT, 0xffffff),
            (ThemeChoice::Dark, ThemePalette::DARK, 0x000000),
            (
                ThemeChoice::Light,
                ThemePalette {
                    background: 0x123456,
                    font: 0xabcdef,
                    primary: 0xff0000,
                },
                0x000000,
            ),
        ] {
            let mut appearance = Appearance {
                theme: Some(choice),
                ..Default::default()
            };
            appearance.set_palette(false, palette);
            apply_theme_colors(&mut theme, appearance.clone(), false);
            assert_eq!(theme.background, rgb(palette.background).into());
            assert_eq!(theme.foreground, rgb(palette.font).into());
            for color in [
                theme.primary,
                theme.primary_hover,
                theme.primary_active,
                theme.link,
                theme.link_hover,
                theme.link_active,
            ] {
                assert_eq!(color, rgb(palette.primary).into());
            }
            assert_eq!(theme.primary_foreground, rgb(checkmark).into());
            assert_eq!(
                theme.highlight_theme.style.editor_background,
                Some(theme.background)
            );
            assert_eq!(
                theme.highlight_theme.style.editor_foreground,
                Some(theme.foreground)
            );
        }
    }
}

fn resolve_font(saved: Option<&str>, installed: &[String], fallback: &str) -> SharedString {
    saved.and_then(|name| installed.iter().find(|font| font.eq_ignore_ascii_case(name)))
        .map_or_else(|| SharedString::from(fallback.to_owned()), |font| font.clone().into())
}

impl Notes {
    pub(super) fn content_font(&self, _cx: &App) -> SharedString {
        let appearance = self.appearance();
        if self.mode == ViewMode::Source {
            resolve_font(appearance.monospace_font.as_deref(), &self.installed_fonts, "Consolas")
        } else {
            resolve_font(appearance.text_font.as_deref(), &self.installed_fonts, ".SystemUIFont")
        }
    }

    fn initialize_font_pickers(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        use gpui_component::{IndexPath, select::{SearchableVec, SelectEvent, SelectState}};
        self.font_pickers.clear();
        self.font_subscriptions.clear();
        let appearance = self.appearance();
        for (index, selected) in [appearance.interface_font, appearance.text_font, appearance.monospace_font].into_iter().enumerate() {
            let mut fonts = self.installed_fonts.clone();
            fonts.sort_by_key(|font| font.to_lowercase());
            fonts.dedup();
            if let Some(name) = &selected {
                if !fonts.contains(name) { fonts.push(name.clone()); }
            }
            fonts.insert(0, "Default".to_owned());
            let row = selected.as_ref().and_then(|name| fonts.iter().position(|font| font == name)).unwrap_or(0);
            let picker = cx.new(|cx| SelectState::new(SearchableVec::new(fonts), Some(IndexPath::default().row(row)), window, cx).searchable(true));
            self.font_subscriptions.push(cx.subscribe_in(&picker, window, move |this, _, event: &SelectEvent<SearchableVec<String>>, window, cx| {
                let SelectEvent::Confirm(value) = event;
                let value = value.clone().filter(|name| name != "Default");
                let mut appearance = this.appearance();
                match index {
                    0 => appearance.interface_font = value,
                    1 => appearance.text_font = value,
                    _ => appearance.monospace_font = value,
                }
                this.update_appearance(appearance, window, cx);
            }));
            self.font_pickers.push(picker);
        }
    }
}

#[cfg(test)]
mod font_tests {
    use super::{notebook, resolve_font};
    #[test]
    fn font_preferences_round_trip_and_fall_back() {
        let mut appearance: notebook::Appearance = serde_json::from_str("{}").unwrap();
        assert_eq!(appearance.text_font, None);
        appearance.interface_font = Some("Interface".into());
        appearance.text_font = Some("Text".into());
        appearance.monospace_font = Some("Code".into());
        let encoded = serde_json::to_string(&appearance).unwrap();
        assert_eq!(serde_json::from_str::<notebook::Appearance>(&encoded).unwrap(), appearance);
        let installed = vec!["Text".into()];
        assert_eq!(resolve_font(Some("text"), &installed, "Default").as_ref(), "Text");
        assert_eq!(resolve_font(Some("Missing"), &installed, "Default").as_ref(), "Default");
        assert_eq!(appearance.monospace_font.as_deref(), Some("Code"));
    }
}
