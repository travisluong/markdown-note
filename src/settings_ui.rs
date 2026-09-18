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
    pub(super) fn toolbar_settings(&self) -> notebook::ToolbarSettings {
        self.notebooks
            .active
            .as_ref()
            .map_or(self.session_toolbar, |book| book.settings.toolbar)
    }

    /// Shared mutation for the Settings list and direct toolbar drops. Callers
    /// validate their own interaction authority before crossing this boundary.
    pub(super) fn move_toolbar_item(
        &mut self,
        item: notebook::ToolbarItem,
        target: notebook::ToolbarItem,
        after: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let changed = if let Some(book) = &mut self.notebooks.active {
            let changed = book.settings.toolbar.move_item(item, target, after);
            if changed {
                self.persist_notebooks(cx);
            }
            changed
        } else {
            self.session_toolbar.move_item(item, target, after)
        };
        if changed {
            self.sync_settings_panel(window, cx);
            cx.notify();
        }
    }

    pub(super) fn toggle_toolbar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.document_interaction_blocked() || window.has_active_dialog(cx) {
            return;
        }
        if let Some(book) = &mut self.notebooks.active {
            book.settings.toolbar.visible = !book.settings.toolbar.visible;
            self.persist_notebooks(cx);
        } else {
            self.session_toolbar.visible = !self.session_toolbar.visible;
        }
        cx.notify();
    }

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

    pub(super) fn toggle_theme(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.document_interaction_blocked() || window.has_active_dialog(cx) {
            return;
        }
        let mut appearance = self.appearance();
        appearance.theme = Some(match appearance.theme_choice(self.startup_dark) {
            notebook::ThemeChoice::Light => notebook::ThemeChoice::Dark,
            _ => notebook::ThemeChoice::Light,
        });
        self.update_appearance(appearance, window, cx);
    }

    pub(super) fn toggle_centered_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.document_interaction_blocked() || window.has_active_dialog(cx) {
            return;
        }
        let mut appearance = self.appearance();
        appearance.centered_editor = !appearance.centered_editor;
        self.update_appearance(appearance, window, cx);
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
    opening_file: bool,
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
            .autosave_root()
            .map_or(PreferenceTarget::Session, PreferenceTarget::Notebook);
        SettingsScope {
            preferences,
            autosave,
        }
    }

    fn settings_values(&self) -> SettingsValues {
        SettingsValues {
            toolbar: self.toolbar_settings(),
            appearance: self.appearance(),
            autosave: self.autosave_settings(),
            autosave_external_files: self.external_autosave_enabled(),
            external_autosave_scope: self.notebooks.active.as_ref().map_or_else(
                || "Applies only to this session while no notebook is open.".to_owned(),
                |book| {
                    format!(
                        "Saved for {}. Applies while this notebook is active.",
                        notebook::name(&book.root)
                    )
                },
            ),
            has_autosave_notebook: self.autosave_root().is_some(),
            date_format: self.date_format().to_owned(),
            show_extensions: self.show_file_extensions(),
            hidden_extensions: self.hidden_extensions().to_vec(),
            excluded_patterns: self
                .notebooks
                .active
                .as_ref()
                .map(|book| book.settings.excluded_patterns.clone()),
            startup_dark: self.startup_dark,
            busy: self.is_busy(),
            has_settings_file: self.notebooks.active.is_some(),
            opening_settings_file: self
                .settings_panel
                .as_ref()
                .is_some_and(|open| open.opening_file),
            subtitle: self.notebooks.active.as_ref().map_or_else(
                || "Changes apply immediately for this session.".to_owned(),
                |book| format!("Changes are saved for {}.", notebook::name(&book.root)),
            ),
            autosave_scope: self.autosave_root().map_or_else(
                || "Autosave delay for external files in this session.".to_owned(),
                |root| format!("Autosave settings apply to {}.", notebook::name(&root)),
            ),
            notice: self.notebooks.notice.clone(),
        }
    }

    pub(super) fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_busy() || window.has_active_dialog(cx) {
            return;
        }
        self.menu = None;
        self.notebooks.context_menu = None;
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
            opening_file: false,
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
        SettingsPanel::dialog(panel, dialog, window, cx)
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
    pub(super) fn sync_settings_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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

    pub(super) fn focus_settings(&self, window: &mut Window, cx: &App) -> bool {
        let Some(open) = &self.settings_panel else {
            return false;
        };
        if open.lifetime.upgrade().is_none() {
            return false;
        }
        let Some(panel) = open.panel.upgrade() else {
            return false;
        };
        panel.read(cx).focus.focus(window);
        true
    }

    /// The dialog retains navigation authority while its notebook writes drain.
    /// Called on the initial intent and after each preference-write completion.
    pub(super) fn resolve_settings_file_open(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(open) = self
            .settings_panel
            .as_ref()
            .filter(|open| open.opening_file)
        else {
            return;
        };
        let id = open.panel.entity_id();
        if open.lifetime.upgrade().is_none() || open.scope != self.settings_scope() {
            self.close_settings_panel(id, window, cx);
            return;
        }
        if self.is_busy() {
            self.settings_panel.as_mut().unwrap().opening_file = false;
            self.sync_settings_panel(window, cx);
            return;
        }
        let PreferenceTarget::Notebook(root) = &open.scope.preferences else {
            return;
        };
        let root = root.clone();
        // Recapture the barrier so edits made while waiting are included too.
        match self.notebooks.settings_file_flush_status(&root) {
            notebook::FlushStatus::Pending => self.sync_settings_panel(window, cx),
            notebook::FlushStatus::Failed(errors) => {
                self.settings_panel.as_mut().unwrap().opening_file = false;
                self.notebooks.notice = Some(format!(
                    "Could not open settings JSON because settings could not be saved: {}",
                    errors
                        .iter()
                        .map(|error| error.message.as_str())
                        .collect::<Vec<_>>()
                        .join("; ")
                ));
                self.sync_settings_panel(window, cx);
            }
            notebook::FlushStatus::Succeeded => {
                let tab = self.tabs.active.id;
                self.close_settings_panel(id, window, cx);
                // Run after modal dismissal restores focus, including the case
                // where this file is already the current document.
                cx.defer_in(window, move |this, window, cx| {
                    if this.is_busy()
                        || window.has_active_dialog(cx)
                        || this.tabs.active.id != tab
                        || this
                            .notebooks
                            .active
                            .as_ref()
                            .is_none_or(|book| book.root != root)
                    {
                        return;
                    }
                    let path = notebook::settings_path(&root);
                    if this.tabs.active.path() == Some(path.as_path()) {
                        this.tabs.active.editor.focus(window, cx);
                    } else {
                        this.request(crate::Next::File(path), window, cx);
                    }
                });
            }
        }
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
        let same_scope = if matches!(intent, SettingsIntent::ExternalAutosaveEnabled(_)) {
            open.scope.preferences == current.preferences
        } else if intent.is_autosave() {
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
            SettingsIntent::OpenSettingsJson => {
                let open = self.settings_panel.as_mut().unwrap();
                if !open.opening_file
                    && matches!(open.scope.preferences, PreferenceTarget::Notebook(_))
                {
                    open.opening_file = true;
                    self.resolve_settings_file_open(window, cx);
                }
                return;
            }
            SettingsIntent::ResetToolbar => {
                let defaults = notebook::ToolbarSettings::default();
                if let Some(book) = &mut self.notebooks.active {
                    if book.settings.toolbar != defaults {
                        book.settings.toolbar = defaults;
                        self.persist_notebooks(cx);
                    }
                } else {
                    self.session_toolbar = defaults;
                }
                false
            }
            SettingsIntent::ToolbarAlignment(alignment) => {
                if let Some(book) = &mut self.notebooks.active {
                    if book.settings.toolbar.alignment != alignment {
                        book.settings.toolbar.alignment = alignment;
                        self.persist_notebooks(cx);
                    }
                } else {
                    self.session_toolbar.alignment = alignment;
                }
                false
            }
            SettingsIntent::ToolbarIconSize(size) => {
                if let Some(book) = &mut self.notebooks.active {
                    if book.settings.toolbar.icon_size != size {
                        book.settings.toolbar.icon_size = size;
                        self.persist_notebooks(cx);
                    }
                } else {
                    self.session_toolbar.icon_size = size;
                }
                false
            }
            SettingsIntent::ToolbarItemMoved {
                item,
                target,
                after,
            } => {
                self.move_toolbar_item(item, target, after, window, cx);
                return;
            }
            SettingsIntent::ToolbarItemEnabled(item, enabled) => {
                if let Some(book) = &mut self.notebooks.active {
                    book.settings.toolbar.set_enabled(item, enabled);
                    self.persist_notebooks(cx);
                } else {
                    self.session_toolbar.set_enabled(item, enabled);
                }
                false
            }
            SettingsIntent::ExternalAutosaveEnabled(enabled) => {
                self.update_external_autosave(enabled, window, cx);
                false
            }
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
            SettingsIntent::ExcludedPatterns(patterns) => {
                self.update_excluded_patterns(patterns, window, cx);
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
            SettingsIntent::CenteredEditor(enabled) => {
                appearance.centered_editor = enabled;
                true
            }
            SettingsIntent::CenteredEditorWidth(width) => {
                appearance.centered_editor_width = width;
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
                    ColorRole::Border => palette.border = value,
                    ColorRole::Icon => palette.icon = value,
                    ColorRole::Secondary => palette.secondary = value,
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
    theme.icon_color = Some(rgb(palette.icon).into());
    theme.secondary = rgb(palette.secondary).into();
    theme.secondary_hover = theme.secondary;
    theme.secondary_active = theme.secondary;
    theme.border = rgb(palette.border).into();
    theme.input = theme.border;
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
            border: u32::MAX,
            icon: u32::MAX,
            secondary: u32::MAX,
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
                    primary: font,
                    ..if choice == ThemeChoice::Light {
                        ThemePalette::LIGHT
                    } else {
                        ThemePalette::DARK
                    }
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
    fn colors_reach_editor_inputs_checkboxes_and_links() {
        let mut theme = Theme::default();
        let focus_color = theme.ring;
        for (choice, palette, checkmark) in [
            (ThemeChoice::Light, ThemePalette::LIGHT, 0xffffff),
            (ThemeChoice::Dark, ThemePalette::DARK, 0x000000),
            (
                ThemeChoice::Light,
                ThemePalette {
                    background: 0x123456,
                    font: 0xabcdef,
                    primary: 0xff0000,
                    border: 0x654321,
                    icon: 0x987654,
                    secondary: 0x456789,
                },
                0x000000,
            ),
            (
                ThemeChoice::Dark,
                ThemePalette {
                    border: 0xabcdef,
                    ..ThemePalette::DARK
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
            assert_eq!(theme.border, rgb(palette.border).into());
            assert_eq!(theme.input, theme.border);
            assert_eq!(theme.icon_color, Some(rgb(palette.icon).into()));
            assert_eq!(theme.secondary_foreground, theme.foreground);
            for color in [
                theme.secondary,
                theme.secondary_hover,
                theme.secondary_active,
            ] {
                assert_eq!(color, rgb(palette.secondary).into());
            }
            assert_eq!(theme.ring, focus_color);
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
        if self.tabs.active.mode == ViewMode::Source {
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
