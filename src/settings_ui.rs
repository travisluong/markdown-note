use crate::{Notes, ViewMode, notebook, templates};
use gpui::{
    App, Context, EntityId, SharedString, Subscription, WeakEntity, Window, prelude::*, px, rgb,
};
use gpui_component::{Theme, ThemeMode, WindowExt, dialog::Dialog};
use std::{
    path::PathBuf,
    rc::{Rc, Weak},
};

mod panel;
use panel::{ColorRole, FontRole, FontSizeRole, SettingsIntent, SettingsPanel, SettingsValues};

// Match gpui-component's platform defaults for source text and code blocks.
const DEFAULT_MONOSPACE_FONT: &str = if cfg!(target_os = "macos") {
    "Menlo"
} else if cfg!(target_os = "windows") {
    "Consolas"
} else {
    "DejaVu Sans Mono"
};
impl Notes {
    fn show_file_extensions(&self) -> bool {
        self.notebooks
            .active
            .as_ref()
            .map_or(self.session_show_file_extensions, |book| {
                book.settings.show_file_extensions
            })
    }

    fn hidden_extensions(&self) -> &[String] {
        self.notebooks
            .active
            .as_ref()
            .map_or(&self.session_always_hide_extensions, |book| {
                &book.settings.always_hide_extensions
            })
    }

    fn update_sidebar_extensions(
        &mut self,
        show: bool,
        extensions: Vec<String>,
        cx: &mut Context<Self>,
    ) {
        if self.is_busy() {
            return;
        }
        if let Some(book) = &mut self.notebooks.active {
            book.settings.show_file_extensions = show;
            book.settings.always_hide_extensions = extensions;
            self.notebooks.rebuild_rows();
            self.persist_notebooks(cx);
        } else {
            self.session_show_file_extensions = show;
            self.session_always_hide_extensions = extensions;
        }
        cx.notify();
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
            .map_or_else(
                || self.session_appearance.clone(),
                |book| book.settings.appearance.clone(),
            )
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
        theme.font_family = resolve_font(
            appearance.interface_font.as_deref(),
            &self.installed_fonts,
            ".SystemUIFont",
        );
        theme.mono_font_family = resolve_font(
            appearance.monospace_font.as_deref(),
            &self.installed_fonts,
            DEFAULT_MONOSPACE_FONT,
        );
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
        if self.is_busy() {
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
        self.sync_settings_panel(window, cx);
    }
}

#[derive(Clone, PartialEq, Eq)]
enum PreferenceTarget {
    Session,
    Notebook(PathBuf),
}

#[derive(Clone, PartialEq, Eq)]
struct SettingsScope {
    preferences: PreferenceTarget,
    autosave: PreferenceTarget,
}

/// The dialog builder owns the form; this handle only observes its authority.
pub(super) struct SettingsDialog {
    panel: WeakEntity<SettingsPanel>,
    lifetime: Weak<()>,
    scope: SettingsScope,
    _subscription: Subscription,
}

impl Notes {
    fn settings_scope(&self) -> SettingsScope {
        let preferences = self
            .notebooks
            .active
            .as_ref()
            .map_or(PreferenceTarget::Session, |book| {
                PreferenceTarget::Notebook(book.root.clone())
            });
        let autosave = self
            .notebooks
            .active
            .as_ref()
            .filter(|book| {
                self.document()
                    .is_none_or(|document| document.path.starts_with(&book.root))
            })
            .map_or(PreferenceTarget::Session, |book| {
                PreferenceTarget::Notebook(book.root.clone())
            });
        SettingsScope {
            preferences,
            autosave,
        }
    }

    fn settings_values(&self) -> SettingsValues {
        SettingsValues {
            appearance: self.appearance(),
            autosave: self.autosave_settings(),
            date_format: self.date_format().to_owned(),
            show_extensions: self.show_file_extensions(),
            hidden_extensions: self.hidden_extensions().to_vec(),
            startup_dark: self.startup_dark,
            busy: self.is_busy(),
            subtitle: self.notebooks.active.as_ref().map_or_else(
                || "Changes apply immediately for this session.".to_owned(),
                |book| format!("Changes are saved for {}.", notebook::name(&book.root)),
            ),
            notice: self.notebooks.notice.clone(),
        }
    }

    pub(super) fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_busy() || window.has_active_dialog(cx) {
            return;
        }
        self.menu = None;
        let scope = self.settings_scope();
        let values = self.settings_values();
        let panel = cx.new(|cx| SettingsPanel::new(values, &self.installed_fonts, window, cx));
        let subscription = cx.subscribe_in(
            &panel,
            window,
            |this, sender, intent: &SettingsIntent, window, cx| {
                this.handle_settings_intent(sender.entity_id(), intent.clone(), window, cx);
            },
        );
        let id = panel.entity_id();
        let lifetime = Rc::new(());
        self.settings_panel = Some(SettingsDialog {
            panel: panel.downgrade(),
            lifetime: Rc::downgrade(&lifetime),
            scope,
            _subscription: subscription,
        });
        let owner = cx.weak_entity();
        window.open_dialog(cx, move |dialog, window, cx| {
            // The dialog builder owns the form and its authority, including when
            // Root::close_dialog bypasses the optional on_close callback.
            let _keep_alive = (&panel, &lifetime);
            owner
                .update(cx, |this, cx| this.settings_dialog(id, dialog, window, cx))
                .unwrap_or_else(|_| Dialog::new(window, cx))
        });
        cx.notify();
    }

    fn settings_dialog(
        &mut self,
        id: EntityId,
        dialog: Dialog,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Dialog {
        let Some(open) = &self.settings_panel else {
            return dialog;
        };
        if open.panel.entity_id() != id {
            return dialog;
        }
        if open.scope != self.settings_scope() {
            cx.defer_in(window, move |this, window, cx| {
                this.close_settings_panel(id, window, cx)
            });
            return dialog;
        }
        let Some(panel) = open.panel.upgrade() else {
            return dialog;
        };
        let values = self.settings_values();
        let owner = cx.weak_entity();
        let dialog = dialog.on_close(move |_, _, cx| {
            let _ = owner.update(cx, |this, cx| {
                if this
                    .settings_panel
                    .as_ref()
                    .is_some_and(|open| open.panel.entity_id() == id)
                {
                    this.settings_panel = None;
                    cx.notify();
                }
            });
        });
        panel.update(cx, |panel, cx| panel.sync_effective(values, window, cx));
        SettingsPanel::dialog(panel, dialog, window)
    }

    fn close_settings_panel(&mut self, id: EntityId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(open) = &self.settings_panel else {
            return;
        };
        if open.panel.entity_id() != id {
            return;
        }
        let active = open.lifetime.upgrade().is_some();
        self.settings_panel = None;
        // An expired form must not close a later, unrelated dialog.
        if active {
            window.close_dialog(cx);
        }
        cx.notify();
    }
    fn sync_settings_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(open) = &self.settings_panel else {
            return;
        };
        if open.lifetime.upgrade().is_none() {
            self.settings_panel = None;
            cx.notify();
            return;
        }
        let id = open.panel.entity_id();
        if open.scope != self.settings_scope() {
            self.close_settings_panel(id, window, cx);
            return;
        }
        let Some(panel) = open.panel.upgrade() else {
            return;
        };
        let values = self.settings_values();
        panel.update(cx, |panel, cx| panel.sync_effective(values, window, cx));
    }

    fn handle_settings_intent(
        &mut self,
        sender: EntityId,
        intent: SettingsIntent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(open) = &self.settings_panel else {
            return;
        };
        if open.panel.entity_id() != sender {
            return;
        }
        if open.lifetime.upgrade().is_none() {
            self.settings_panel = None;
            cx.notify();
            return;
        }
        if matches!(intent, SettingsIntent::Close) {
            self.close_settings_panel(sender, window, cx);
            return;
        }
        let current = self.settings_scope();
        let same_scope = if intent.is_autosave() {
            open.scope.autosave == current.autosave
        } else {
            open.scope.preferences == current.preferences
        };
        if !same_scope {
            self.close_settings_panel(sender, window, cx);
            return;
        }
        // Preserve autosave changes allowed during activation/close; other form
        // fields retain their existing foreground-operation guard.
        if self.is_busy() && !intent.is_autosave() {
            self.sync_settings_panel(window, cx);
            return;
        }
        let mut appearance = self.appearance();
        let appearance_changed = match intent {
            SettingsIntent::AutosaveEnabled(enabled) => {
                let mut settings = self.autosave_settings();
                settings.enabled = enabled;
                self.update_autosave(settings, window, cx);
                false
            }
            SettingsIntent::AutosaveDelay(delay_ms) => {
                let mut settings = self.autosave_settings();
                settings.delay_ms = delay_ms;
                self.update_autosave(settings, window, cx);
                false
            }
            SettingsIntent::ShowExtensions(show) => {
                self.update_sidebar_extensions(show, self.hidden_extensions().to_vec(), cx);
                false
            }
            SettingsIntent::HiddenExtensions(extensions) => {
                if self.show_file_extensions() {
                    self.update_sidebar_extensions(true, extensions, cx);
                }
                false
            }
            SettingsIntent::DateFormat(value) => {
                if let Some(book) = &mut self.notebooks.active {
                    book.settings.date_format = value;
                    self.persist_notebooks(cx);
                } else {
                    self.session_date_format = value;
                }
                false
            }
            SettingsIntent::Font(role, value) => {
                match role {
                    FontRole::Interface => appearance.interface_font = value,
                    FontRole::Text => appearance.text_font = value,
                    FontRole::Monospace => appearance.monospace_font = value,
                }
                true
            }
            SettingsIntent::FontSizeStep(role, delta) => {
                let value = match role {
                    FontSizeRole::Content => &mut appearance.content_font_size,
                    FontSizeRole::Interface => &mut appearance.ui_font_size,
                };
                *value = value.saturating_add_signed(i16::from(delta));
                true
            }
            SettingsIntent::Theme(choice) => {
                appearance.theme = Some(choice);
                true
            }
            SettingsIntent::Color(role, value) => {
                let mut palette = appearance.palette(self.startup_dark);
                match role {
                    ColorRole::Background => palette.background = value,
                    ColorRole::Font => palette.font = value,
                    ColorRole::Primary => palette.primary = value,
                }
                appearance.set_palette(self.startup_dark, palette);
                true
            }
            SettingsIntent::ResetPalettes => {
                appearance.reset_palettes();
                true
            }
            SettingsIntent::Close => {
                self.close_settings_panel(sender, window, cx);
                false
            }
        };
        if appearance_changed {
            self.update_appearance(appearance, window, cx);
        }
        self.sync_settings_panel(window, cx);
        cx.notify();
    }

    #[cfg(feature = "editor-diagnostics")]
    pub(super) fn diagnostic_set_settings_delay(
        &self,
        value: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.settings_panel
            .as_ref()
            .and_then(|open| open.panel.upgrade())
            .expect("settings panel is open")
            .update(cx, |panel, cx| {
                panel.diagnostic_set_delay(value, window, cx)
            });
    }
    #[cfg(feature = "editor-diagnostics")]
    pub(super) fn diagnostic_settings_delay_invalid(&self, cx: &App) -> bool {
        self.settings_panel
            .as_ref()
            .and_then(|open| open.panel.upgrade())
            .is_some_and(|panel| panel.read(cx).diagnostic_delay_invalid())
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
    saved
        .and_then(|name| {
            installed
                .iter()
                .find(|font| font.eq_ignore_ascii_case(name))
        })
        .map_or_else(
            || SharedString::from(fallback.to_owned()),
            |font| font.clone().into(),
        )
}

impl Notes {
    pub(super) fn content_font(&self, _cx: &App) -> SharedString {
        let appearance = self.appearance();
        if self.mode == ViewMode::Source {
            resolve_font(
                appearance.monospace_font.as_deref(),
                &self.installed_fonts,
                DEFAULT_MONOSPACE_FONT,
            )
        } else {
            resolve_font(
                appearance.text_font.as_deref(),
                &self.installed_fonts,
                ".SystemUIFont",
            )
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
        assert_eq!(
            serde_json::from_str::<notebook::Appearance>(&encoded).unwrap(),
            appearance
        );
        let installed = vec!["Text".into()];
        assert_eq!(
            resolve_font(Some("text"), &installed, "Default").as_ref(),
            "Text"
        );
        assert_eq!(
            resolve_font(Some("Missing"), &installed, "Default").as_ref(),
            "Default"
        );
        assert_eq!(appearance.monospace_font.as_deref(), Some("Code"));
    }
}
