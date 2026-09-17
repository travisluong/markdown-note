#![cfg(feature = "editor-diagnostics")]

use gpui::{
    App, AppContext, Context, Hsla, IntoElement, ParentElement, Render, RenderOnce, Styled,
    TestAppContext, Window, div, rgb,
};
use gpui_component::{
    ActiveTheme, Disableable, Icon, IconName, IconNamed, Root, Theme,
    button::{Button, ButtonVariants},
};
use std::{cell::Cell, rc::Rc};

#[derive(IntoElement)]
struct TextProbe(Rc<Cell<Hsla>>);
impl RenderOnce for TextProbe {
    fn render(self, window: &mut Window, _: &mut App) -> impl IntoElement {
        self.0.set(window.text_style().color);
        div().size_1()
    }
}

struct Controls {
    neutral_text: Rc<Cell<Hsla>>,
    primary_text: Rc<Cell<Hsla>>,
    entity_icon: gpui::Entity<Icon>,
}
impl Render for Controls {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .text_color(cx.theme().foreground)
            .child(
                Button::new("neutral")
                    .ghost()
                    .icon(IconName::Search)
                    .label("Neutral")
                    .child(TextProbe(self.neutral_text.clone())),
            )
            .child(
                Button::new("disabled")
                    .ghost()
                    .disabled(true)
                    .icon(IconName::ArrowLeft),
            )
            .child(
                Button::new("primary")
                    .primary()
                    .icon(IconName::Plus)
                    .label("Primary")
                    .child(TextProbe(self.primary_text.clone())),
            )
            .child(Button::new("danger").danger().icon(IconName::Info))
            .child(
                Button::new("explicit")
                    .ghost()
                    .icon(Icon::new(IconName::Close).text_color(rgb(0x123456))),
            )
            .child(Icon::new(IconName::Check).themed_color(true))
            .child(Icon::new(IconName::ArrowRight))
            .child(self.entity_icon.clone())
    }
}

fn icon_color(colors: &[(gpui::SharedString, Hsla)], name: IconName) -> Hsla {
    let expected_path = name.path();
    colors
        .iter()
        .rev()
        .find(|(path, _)| *path == expected_path)
        .unwrap_or_else(|| panic!("{expected_path} missing from {colors:?}"))
        .1
}

#[gpui::test]
fn interface_icons_keep_text_disabled_semantic_and_explicit_colors(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let neutral_text = Rc::new(Cell::new(Hsla::default()));
    let primary_text = Rc::new(Cell::new(Hsla::default()));
    let (_, cx) = cx.add_window_view(|window, cx| {
        let entity_icon = Icon::new(IconName::Folder).themed_color(true).view(cx);
        let controls = cx.new(|_| Controls {
            neutral_text: neutral_text.clone(),
            primary_text: primary_text.clone(),
            entity_icon,
        });
        Root::new(controls, window, cx)
    });
    for custom in [None, Some(rgb(0x987654).into()), Some(rgb(0x456789).into())] {
        cx.update(|_, cx| {
            Theme::global_mut(cx).icon_color = custom;
            Icon::diagnostic_begin_color_capture(cx);
        });
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear();
        });
        cx.update(|_, cx| {
            let theme = cx.theme();
            let colors = Icon::diagnostic_colors(cx);
            assert_eq!(
                icon_color(&colors, IconName::Search),
                custom.unwrap_or(theme.secondary_foreground)
            );
            assert_eq!(
                icon_color(&colors, IconName::ArrowLeft),
                theme.muted_foreground.opacity(0.5)
            );
            assert_eq!(
                icon_color(&colors, IconName::Plus),
                theme.primary_foreground
            );
            assert_eq!(icon_color(&colors, IconName::Info), theme.danger_foreground);
            assert_eq!(icon_color(&colors, IconName::Close), rgb(0x123456).into());
            assert_eq!(
                icon_color(&colors, IconName::Check),
                custom.unwrap_or(theme.foreground)
            );
            assert_eq!(
                icon_color(&colors, IconName::Folder),
                custom.unwrap_or(theme.foreground)
            );
            assert_eq!(icon_color(&colors, IconName::ArrowRight), theme.foreground);
            assert_eq!(neutral_text.get(), theme.secondary_foreground);
            assert_eq!(primary_text.get(), theme.primary_foreground);
        });
    }
}

gpui::actions!(theme_icons, [Choose]);

#[gpui::test]
fn menu_icons_keep_disabled_and_explicit_colors(cx: &mut TestAppContext) {
    use gpui_component::menu::PopupMenu;
    cx.update(gpui_component::init);
    let (_, cx) = cx.add_window_view(|window, cx| {
        let menu = PopupMenu::build(window, cx, |menu, _, _| {
            menu.menu_with_icon("Enabled", IconName::Search, Box::new(Choose))
                .menu_with_icon_and_disabled(
                    "Disabled",
                    IconName::ArrowLeft,
                    Box::new(Choose),
                    true,
                )
                .menu_with_icon(
                    "Explicit",
                    Icon::new(IconName::Info).text_color(rgb(0x123456)),
                    Box::new(Choose),
                )
        });
        Root::new(menu, window, cx)
    });
    cx.update(|_, cx| {
        Theme::global_mut(cx).icon_color = Some(rgb(0x987654).into());
        Icon::diagnostic_begin_color_capture(cx);
    });
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear();
    });
    cx.update(|_, cx| {
        let colors = Icon::diagnostic_colors(cx);
        assert_eq!(icon_color(&colors, IconName::Search), rgb(0x987654).into());
        assert_eq!(
            icon_color(&colors, IconName::ArrowLeft),
            cx.theme().muted_foreground
        );
        assert_eq!(icon_color(&colors, IconName::Info), rgb(0x123456).into());
    });
}
