use super::{FontSizeRole, Section, SettingsIntent, SettingsPanel};
use gpui::{App, Context, Entity, IntoElement, Render, Window, div, prelude::*, px};
use gpui_component::{
    ActiveTheme, Disableable, Sizable,
    button::{Button, ButtonVariants},
    color_picker::ColorPicker,
    dialog::Dialog,
    input::Input,
    scroll::ScrollableElement,
    select::Select,
    switch::Switch,
};
use mithril::notebook;

impl Render for SettingsPanel {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let appearance = &self.values.appearance;
        let narrow = self.placement.size().width - px(48.) < px(512.);
        let mut body = div().w_full().min_w_0().flex().flex_col().gap_3().child(
            div()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(self.values.subtitle.clone()),
        );
        let mut font_body = div().w_full().min_w_0().flex().flex_col().gap_3();
        for (picker, label) in
            self.fonts
                .iter()
                .zip(["Interface font", "Text font", "Monospace font"])
        {
            font_body = font_body.child(label).child(
                div()
                    .w_full()
                    .min_w_0()
                    .h_8()
                    .flex()
                    .flex_shrink_0()
                    .child(Select::new(picker).w_full()),
            );
        }
        for (index, role, label, value, maximum) in [
            (
                0usize,
                FontSizeRole::Content,
                "Content font size",
                appearance.content_font_size,
                40,
            ),
            (
                1usize,
                FontSizeRole::Interface,
                "UI font size",
                appearance.ui_font_size,
                24,
            ),
        ] {
            font_body = font_body.child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .when(narrow, |view| view.w_full())
                            .when(!narrow, |view| view.flex_1())
                            .min_w_0()
                            .child(label),
                    )
                    .child(
                        Button::new(("font-decrease", index))
                            .label("−")
                            .disabled(self.values.busy || value <= 10)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.emit_intent(SettingsIntent::FontSizeStep(role, -1), cx)
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
                            .disabled(self.values.busy || value >= maximum)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.emit_intent(SettingsIntent::FontSizeStep(role, 1), cx)
                            })),
                    ),
            );
        }
        body = body
            .child(
                Button::new("open-settings-json")
                    .debug_selector(|| "open-settings-json".into())
                    .label("Open settings JSON")
                    .disabled(self.values.busy || !self.values.has_settings_file || self.values.opening_settings_file)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.emit_intent(SettingsIntent::OpenSettingsJson, cx)
                    })),
            )
            .when(!self.values.has_settings_file, |body| body.child(
                div().text_sm().text_color(cx.theme().muted_foreground)
                    .child("Open a notebook to edit its settings JSON."),
            ))
            .when(self.values.opening_settings_file, |body| body.child(
                div().text_sm().text_color(cx.theme().muted_foreground)
                    .child("Waiting for settings to finish saving…"),
            ))
            .child("Editor")
            .child(Switch::new("centered-editor").label("Centered editor").checked(appearance.centered_editor)
                .disabled(self.values.busy)
                .on_click(cx.listener(|this, enabled: &bool, _, cx| this.emit_intent(SettingsIntent::CenteredEditor(*enabled), cx))))
            .child("Width (px)")
            .child(Input::new(&self.editor_width).w_full().flex_shrink_0()
                .disabled(self.values.busy || !appearance.centered_editor))
            .child(div().text_sm().text_color(cx.theme().muted_foreground)
                .child("Maximum width for Source, Live preview, and Reading. Shrinks to fit the available space."))
            .when_some(self.editor_width_error.clone(), |view, error| view.child(div().text_sm().text_color(cx.theme().danger).child(error)))
            .child("Saving")
            .child(div().text_sm().text_color(cx.theme().muted_foreground).child(self.values.autosave_scope.clone()))
            .when(self.values.has_autosave_notebook, |body| body.child(Switch::new("autosave-enabled").label("Autosave").checked(self.values.autosave.enabled)
                .on_click(cx.listener(|this, enabled: &bool, _, cx| this.emit_intent(SettingsIntent::AutosaveEnabled(*enabled), cx)))))
            .child("Autosave delay (milliseconds)")
            .child(Input::new(&self.delay).w_full().flex_shrink_0())
            .when_some(self.delay_error.clone(), |view, message| view.child(div().text_sm().text_color(cx.theme().danger).child(message)))
            .child(Switch::new("autosave-external-files").label("Autosave external files").checked(self.values.autosave_external_files)
                .on_click(cx.listener(|this, enabled: &bool, _, cx| this.emit_intent(SettingsIntent::ExternalAutosaveEnabled(*enabled), cx))))
            .child(div().text_sm().text_color(cx.theme().muted_foreground)
                .child("Automatically save files outside the current notebook. Off by default."))
            .child(div().text_sm().text_color(cx.theme().muted_foreground).child(self.values.external_autosave_scope.clone()))
            .child("Sidebar")
            .child(Switch::new("show-file-extensions").label("Show file extensions").checked(self.values.show_extensions)
                .disabled(self.values.busy)
                .on_click(cx.listener(|this, enabled: &bool, _, cx| this.emit_intent(SettingsIntent::ShowExtensions(*enabled), cx))))
            .child("Always hide these extensions")
            .child(Input::new(&self.hidden_extensions).w_full().flex_shrink_0().disabled(!self.values.show_extensions || self.values.busy))
            .child(div().text_sm().text_color(cx.theme().muted_foreground)
                .child("Hide these extensions even when Show file extensions is on. Example: md, txt."))
            .when_some(self.hidden_extensions_error.clone(), |view, error| view.child(div().text_sm().text_color(cx.theme().danger).child(error)))
            .child("Exclude files and folders (regex)")
            .child(Input::new(&self.excluded_patterns).w_full().flex_shrink_0()
                .disabled(self.values.busy || self.values.excluded_patterns.is_none()))
            .child(div().text_sm().text_color(cx.theme().muted_foreground).child(
                if self.values.excluded_patterns.is_none() {
                    "Open a notebook to configure exclusions."
                } else {
                    r"Hide matches from the sidebar, notebook search, and Go to File. Use comma-separated regexes against relative paths with /. Folders exclude their contents. Case-sensitive; use (?i) to ignore case. Example: ^archive$, \.tmp$. Commas inside [a,b], {1,3}, or escaped as \, stay in the pattern."
                }))
            .when_some(self.excluded_patterns_error.clone(), |view, error| view.child(div().text_sm().text_color(cx.theme().danger).child(error)))
            .when_some(self.values.notice.clone(), |view, notice| view.child(div().text_sm().child(notice)));
        let terminal_body = div()
            .w_full().min_w_0().flex().flex_col().gap_3()
            .child(self.values.subtitle.clone())
            .when(cfg!(windows), |body| body
                .child("Terminal shell")
                .child(div().debug_selector(|| "terminal-shell-picker".into()).w_full().min_w_0().h_8().flex().flex_shrink_0()
                    .child(Select::new(&self.terminal_shell).w_full().disabled(self.values.busy)))
                .child(div().text_sm().text_color(cx.theme().muted_foreground)
                    .child("Applies on the next terminal start or Restart. The current shell keeps running.")))
            .when(!cfg!(windows), |body| body.child("Uses your system shell ($SHELL, or /bin/sh)."))
            .child("Cursor style")
            .child(div().debug_selector(|| "terminal-cursor-picker".into()).w_full().min_w_0().h_8().flex().flex_shrink_0()
                .child(Select::new(&self.terminal_cursor).w_full().disabled(self.values.busy)))
            .child(div().text_sm().text_color(cx.theme().muted_foreground)
                .child("Applies immediately, including to the current terminal."));
        let templates_body = div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap_3()
            .child("Current date format")
            .child(Input::new(&self.date).w_full().flex_shrink_0())
            .when_some(self.date_error.clone(), |view, error| {
                view.child(div().text_sm().text_color(cx.theme().danger).child(error))
            })
            .when_some(self.date_example(cx), |view, example| {
                view.child(
                    div()
                        .debug_selector(|| "date-format-example".into())
                        .child(format!("Example: {example}")),
                )
            })
            .child(super::reference::render(cx));
        let mut theme_body = div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap_3()
            .child("Theme");
        let mut selector = div().flex().flex_wrap().gap_2();
        for (index, choice, label) in [
            (0usize, notebook::ThemeChoice::Light, "Light"),
            (1, notebook::ThemeChoice::Dark, "Dark"),
        ] {
            selector = selector.child(
                Button::new(("theme-choice", index))
                    .label(label)
                    .when(
                        appearance.theme_choice(self.values.startup_dark) == choice,
                        |button| button.primary(),
                    )
                    .disabled(self.values.busy)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.emit_intent(SettingsIntent::Theme(choice), cx)
                    })),
            );
        }
        theme_body = theme_body.child(selector);
        for (picker, label) in self.colors.iter().zip([
            "Background color",
            "Font color",
            "Primary color",
            "Border color",
            "Icon color",
            "Secondary color",
        ]) {
            theme_body = theme_body.child(ColorPicker::new(picker).label(label));
        }
        theme_body = theme_body.child(
            Button::new("reset-theme-colors")
                .label("Reset Light and Dark themes")
                .disabled(self.values.busy)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.emit_intent(SettingsIntent::ResetPalettes, cx)
                })),
        );
        let mut toolbar_sizes = div().flex().flex_wrap().gap_2();
        for (index, (size, label)) in notebook::ToolbarIconSize::OPTIONS.into_iter().enumerate() {
            toolbar_sizes = toolbar_sizes.child(
                Button::new(("toolbar-icon-size", index))
                    .debug_selector(move || format!("toolbar-icon-size-{index}"))
                    .label(label)
                    .when(self.values.toolbar.icon_size == size, |button| {
                        button.primary()
                    })
                    .disabled(self.values.busy)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.emit_intent(SettingsIntent::ToolbarIconSize(size), cx)
                    })),
            );
        }
        let mut toolbar_body = div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap_3()
            .child(self.values.subtitle.clone())
            .child("Icon size")
            .child(
                toolbar_sizes.child(
                    Button::new("reset-toolbar")
                        .debug_selector(|| "reset-toolbar".into())
                        .label("Reset toolbar to defaults")
                        .disabled(self.values.busy)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.emit_intent(SettingsIntent::ResetToolbar, cx)
                        })),
                ),
            )
            .child("Choose which items appear. Drag the grips to reorder within each region.");
        for (region, label) in notebook::ToolbarRegion::OPTIONS {
            toolbar_body =
                toolbar_body.child(div().font_weight(gpui::FontWeight::SEMIBOLD).child(label));
            for item in self
                .values
                .toolbar
                .order
                .into_iter()
                .filter(|item| item.region() == region)
            {
                toolbar_body = toolbar_body.child(self.render_toolbar_item(item, cx));
            }
        }
        let mut sidebar = div()
            .id("settings-section-tabs")
            .debug_selector(|| "settings-navigation".into())
            .w(px(104.))
            .flex_shrink_0()
            .flex()
            .flex_col()
            .gap_2();
        if narrow {
            sidebar = sidebar.w_full().flex_row().overflow_x_scroll();
        }
        for (id, section, label) in [
            ("settings-general", Section::General, "General"),
            ("settings-terminal", Section::Terminal, "Terminal"),
            ("settings-toolbar", Section::Toolbar, "Toolbar"),
            ("settings-templates", Section::Templates, "Templates"),
            ("settings-font", Section::Font, "Font"),
            ("settings-theme", Section::Theme, "Theme"),
        ] {
            sidebar = sidebar.child(
                Button::new(id)
                    .label(label)
                    .when(narrow, |button| button.small())
                    .when(self.section == section, |button| button.primary())
                    .on_click(cx.listener(move |this, _, _, cx| this.select_section(section, cx))),
            );
        }
        let content = match self.section {
            Section::General => body.w_full().min_w_0().into_any_element(),
            Section::Terminal => terminal_body.into_any_element(),
            Section::Toolbar => toolbar_body.into_any_element(),
            Section::Font => font_body.into_any_element(),
            Section::Templates => templates_body.into_any_element(),
            Section::Theme => theme_body.into_any_element(),
        };
        let sections = div()
            .w_full()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .flex()
            .when(narrow, |view| view.flex_col())
            .gap_2()
            .child(sidebar)
            .child(
                div()
                    .relative()
                    .w_full()
                    .min_w_0()
                    .min_h_0()
                    .flex_1()
                    .when(!narrow, |view| view.flex_1().min_w(px(240.)))
                    .child(
                        div()
                            .id("settings-scroll")
                            .debug_selector(|| "settings-content".into())
                            .flex()
                            .flex_col()
                            .size_full()
                            .min_w_0()
                            .min_h_0()
                            .overflow_y_scroll()
                            .track_scroll(&self.content_scroll)
                            .child(content),
                    )
                    // Keep the track and its hit area fixed to the viewport.
                    .vertical_scrollbar(&self.content_scroll),
            );
        div()
            .id("settings-panel")
            .key_context("SettingsPanel")
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &crate::CloseSettings, _, cx| {
                this.emit_intent(SettingsIntent::Close, cx);
            }))
            .size_full()
            .min_h_0()
            .min_w_0()
            .flex()
            .flex_col()
            .child(sections)
    }
}

impl SettingsPanel {
    pub(super) fn preferred_size(window: &Window) -> gpui::Size<gpui::Pixels> {
        let padding = gpui_component::window_paddings(window);
        let content = window.viewport_size()
            - gpui::size(padding.left + padding.right, padding.top + padding.bottom);
        gpui::size(content.width * 0.8, content.height * 0.8)
    }

    pub(in crate::settings_ui) fn dialog(
        panel: Entity<Self>,
        dialog: Dialog,
        window: &Window,
        cx: &App,
    ) -> Dialog {
        panel
            .read(cx)
            .placement
            .set_preferred_size(Self::preferred_size(window));
        dialog
            .title("Settings")
            .movable(panel.read(cx).placement.clone())
            .child(panel)
    }
}
