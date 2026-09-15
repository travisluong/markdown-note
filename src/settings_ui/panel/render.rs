use super::{FontSizeRole, Section, SettingsIntent, SettingsPanel};
use gpui::{Context, Entity, IntoElement, Render, Window, div, prelude::*, px};
use gpui_component::{
    ActiveTheme, Disableable,
    button::{Button, ButtonVariants},
    color_picker::ColorPicker,
    dialog::Dialog,
    input::Input,
    select::Select,
    switch::Switch,
};
use mithril::notebook;

impl Render for SettingsPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let appearance = &self.values.appearance;
        let narrow = window.viewport_size().width < px(560.);
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
                            .when(window.viewport_size().width < px(560.), |view| {
                                view.w_full()
                            })
                            .when(window.viewport_size().width >= px(560.), |view| {
                                view.flex_1()
                            })
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
            .child(Switch::new("autosave-enabled").label("Autosave").checked(self.values.autosave.enabled)
                .on_click(cx.listener(|this, enabled: &bool, _, cx| this.emit_intent(SettingsIntent::AutosaveEnabled(*enabled), cx))))
            .child("Autosave delay (milliseconds)")
            .child(Input::new(&self.delay).w_full().flex_shrink_0())
            .when_some(self.delay_error.clone(), |view, message| view.child(div().text_sm().text_color(cx.theme().danger).child(message)))
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
            .when_some(self.values.notice.clone(), |view, notice| view.child(div().text_sm().child(notice)))
            .child(Button::new("close-settings").label("Close")
                .on_click(cx.listener(|this, _, _, cx| this.emit_intent(SettingsIntent::Close, cx))));
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
            .child(toolbar_sizes)
            .child("Choose which items appear in the toolbar. Drag the grips to reorder.");
        for item in self.values.toolbar.order {
            toolbar_body = toolbar_body.child(self.render_toolbar_item(item, cx));
        }
        let mut sidebar = div()
            .debug_selector(|| "settings-navigation".into())
            .w(px(104.))
            .flex_shrink_0()
            .flex()
            .flex_col()
            .gap_2();
        if narrow {
            sidebar = sidebar.w_full().flex_row().flex_wrap();
        }
        for (id, section, label) in [
            ("settings-general", Section::General, "General"),
            ("settings-toolbar", Section::Toolbar, "Toolbar"),
            ("settings-templates", Section::Templates, "Templates"),
            ("settings-font", Section::Font, "Font"),
            ("settings-theme", Section::Theme, "Theme"),
        ] {
            sidebar = sidebar.child(
                Button::new(id)
                    .label(label)
                    .when(self.section == section, |button| button.primary())
                    .on_click(cx.listener(move |this, _, _, cx| this.select_section(section, cx))),
            );
        }
        let content = match self.section {
            Section::General => body.w_full().min_w_0().into_any_element(),
            Section::Toolbar => toolbar_body.into_any_element(),
            Section::Font => font_body.into_any_element(),
            Section::Templates => templates_body.into_any_element(),
            Section::Theme => theme_body.into_any_element(),
        };
        div()
            .w_full()
            .min_w_0()
            .flex()
            .when(narrow, |view| view.flex_col())
            .gap_3()
            .child(sidebar)
            .child(
                div()
                    .debug_selector(|| "settings-content".into())
                    .flex()
                    .flex_col()
                    .w_full()
                    .min_w_0()
                    .when(!narrow, |view| view.flex_1().min_w(px(240.)))
                    .child(content),
            )
    }
}

impl SettingsPanel {
    pub(in crate::settings_ui) fn dialog(
        panel: Entity<Self>,
        dialog: Dialog,
        window: &Window,
    ) -> Dialog {
        dialog
            .title("Settings")
            .w(px(620.).min(window.viewport_size().width - px(32.)))
            .max_h((window.viewport_size().height - px(100.)).max(px(120.)))
            .child(panel)
    }
}
