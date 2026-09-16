//! Application-owned menus and platform title-bar regions.
use crate::{
    About, KeyboardShortcuts, LivePreview, MenuKind, Next, Notes, Open, OpenSettings, ReadingView,
    ReplaceFile, Save, SearchAllFiles, SourceMode, ViewMode,
};
use gpui::{
    AnyElement, App, Bounds, Context, Decorations, DismissEvent, Focusable, IntoElement,
    MouseButton, Pixels, Size, TextRun, WeakEntity, Window, WindowControlArea, anchored, canvas,
    deferred, div, font, img, prelude::*, px,
};
use gpui_component::{
    ActiveTheme, Sizable, WindowExt,
    button::{Button, ButtonVariants},
    input::Search,
    menu::{PopupMenu, PopupMenuItem},
};
use gpui_component::{Icon, IconName};
use std::{cell::RefCell, rc::Rc};

impl Notes {
    fn populate_menu(
        &self,
        kind: MenuKind,
        menu: PopupMenu,
        owner: WeakEntity<Self>,
        cx: &mut App,
    ) -> PopupMenu {
        let busy = self.document_interaction_blocked();
        let can_save = self.document().is_some() && !busy;
        let mode = self.tabs.active.mode;
        let capabilities = self.capabilities();
        if kind == MenuKind::File {
            menu.menu_with_disabled("Open…", Box::new(Open), busy)
                .menu_with_disabled("Save", Box::new(Save), !can_save)
                .separator()
                .menu_with_disabled("Settings", Box::new(OpenSettings), self.is_busy())
        } else if kind == MenuKind::View {
            menu.menu_with_check_and_disabled(
                "Source mode",
                mode == ViewMode::Source,
                Box::new(SourceMode),
                busy,
            )
            .menu_with_check_and_disabled(
                "Live preview",
                mode == ViewMode::LivePreview,
                Box::new(LivePreview),
                busy,
            )
            .menu_with_check_and_disabled(
                "Reading view",
                mode == ViewMode::Reading,
                Box::new(ReadingView),
                busy,
            )
            .separator()
            .item(
                PopupMenuItem::new("Collapse sidebar")
                    .checked(self.sidebar_collapsed)
                    .disabled(self.is_busy() || self.notebooks.active.is_none())
                    .on_click(move |_, window, cx| {
                        let _ = owner.update(cx, |_, cx| {
                            // Restore sidebar focus after the menu finishes dismissing.
                            cx.defer_in(window, |this, window, cx| {
                                this.toggle_sidebar(window, cx);
                            });
                        });
                    }),
            )
        } else if kind == MenuKind::Search {
            use mithril::commands::Command;
            menu.menu_with_disabled(
                "Search in file",
                Box::new(Search),
                Command::SearchFile.unavailable(capabilities).is_some(),
            )
            .menu_with_disabled(
                "Search in all files",
                Box::new(SearchAllFiles),
                Command::SearchAllFiles.unavailable(capabilities).is_some(),
            )
            .menu_with_disabled(
                "Search and replace in file",
                Box::new(ReplaceFile),
                Command::ReplaceFile.unavailable(capabilities).is_some(),
            )
        } else if kind == MenuKind::Sort {
            self.file_sort_menu(menu, owner)
        } else if kind == MenuKind::Help {
            menu.menu("Keyboard shortcuts", Box::new(KeyboardShortcuts))
                .menu("About", Box::new(About))
        } else {
            self.notebook_menu(menu, cx)
        }
    }
}

const MENUS: [MenuKind; 6] = [
    MenuKind::File,
    MenuKind::View,
    MenuKind::Search,
    MenuKind::Sort,
    MenuKind::Notebooks,
    MenuKind::Help,
];

impl MenuKind {
    fn label(self) -> &'static str {
        match self {
            Self::File => "File",
            Self::View => "View",
            Self::Search => "Search",
            Self::Sort => "Sort",
            Self::Notebooks => "Notebooks",
            Self::Help => "Help",
            Self::More => "More",
        }
    }
    fn selector(self) -> &'static str {
        match self {
            Self::File => "file-menu",
            Self::View => "view-menu",
            Self::Search => "search-menu",
            Self::Sort => "sort-menu",
            Self::Notebooks => "notebooks-menu",
            Self::Help => "help-menu",
            Self::More => "more-menu",
        }
    }
    fn index(self) -> usize {
        MENUS.iter().position(|kind| *kind == self).unwrap_or(6)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HeaderMode {
    Windows,
    Mac,
    Linux,
    Server,
    Fullscreen,
}
impl HeaderMode {
    fn current(window: &Window) -> Self {
        if window.is_fullscreen() {
            Self::Fullscreen
        } else if cfg!(target_os = "windows") {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::Mac
        } else if matches!(window.window_decorations(), Decorations::Client { .. }) {
            Self::Linux
        } else {
            Self::Server
        }
    }
    fn combined(self) -> bool {
        !matches!(self, Self::Server | Self::Fullscreen)
    }
    fn controls(self) -> bool {
        matches!(self, Self::Windows | Self::Linux)
    }
}

#[derive(Clone, Debug, PartialEq)]
struct HeaderLayout {
    mode: HeaderMode,
    viewport: Size<Pixels>,
    width: Pixels,
    height: Pixels,
    widths: [Pixels; 7],
    gap: Pixels,
    visible: usize,
}
impl HeaderLayout {
    fn new(
        mode: HeaderMode,
        viewport: Size<Pixels>,
        width: Pixels,
        rem: Pixels,
        widths: [Pixels; 7],
    ) -> Self {
        let gap = rem * 0.25;
        let reserved = px(if mode.controls() { 4. } else { 8. })
            + if mode.combined() {
                px(28. + 32.)
            } else {
                px(0.)
            }
            + if mode == HeaderMode::Mac {
                px(80.)
            } else {
                px(0.)
            }
            + if mode.controls() { px(102.) } else { px(0.) };
        let available = width - reserved;
        let total = widths[..6]
            .iter()
            .fold(px(0.), |total, width| total + *width)
            + gap * 5.;
        let visible = if total <= available {
            6
        } else {
            let mut count = 1;
            let mut used = widths[0] + gap + widths[6];
            while count < 6 && used + widths[count] + gap <= available {
                used += widths[count] + gap;
                count += 1;
            }
            count
        };
        Self {
            mode,
            viewport,
            width,
            height: px(34.).max(rem * 1.5 + px(8.)),
            widths,
            gap,
            visible,
        }
    }
    fn anchor_index(&self, kind: MenuKind) -> usize {
        if kind.index() < self.visible {
            kind.index()
        } else {
            6
        }
    }
}

#[derive(Default)]
pub(super) struct TitleBarState {
    layout: Option<HeaderLayout>,
    bounds: Rc<RefCell<[Bounds<Pixels>; 7]>>,
}

impl Notes {
    // Both native close and client-decorated Linux close use the same lifecycle owner.
    pub(super) fn request_window_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.is_busy() {
            self.request(Next::Close, window, cx);
        }
    }

    pub(super) fn show_menu(
        &mut self,
        kind: MenuKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if window.has_active_dialog(cx) {
            return;
        }
        if self.menu.is_none() {
            self.menu_previous_focus = window.focused(cx);
        }
        let focus = self
            .menu_previous_focus
            .clone()
            .unwrap_or_else(|| self.focus.clone());
        let owner = cx.weak_entity();
        let visible = self
            .title_bar
            .layout
            .as_ref()
            .map_or(6, |layout| layout.visible);
        let menu = PopupMenu::build(window, cx, |menu, window, cx| {
            let mut menu = menu.action_context(focus.clone());
            if kind == MenuKind::More {
                for kind in &MENUS[visible..] {
                    menu = menu.submenu(kind.label(), window, cx, |submenu, _, cx| {
                        self.populate_menu(
                            *kind,
                            submenu.action_context(focus.clone()),
                            owner.clone(),
                            cx,
                        )
                    });
                }
                menu
            } else {
                self.populate_menu(kind, menu, owner, cx)
            }
        });
        self.menu_subscription =
            Some(
                cx.subscribe_in(&menu, window, |this, dismissed, _: &DismissEvent, _, cx| {
                    if this
                        .menu
                        .as_ref()
                        .is_some_and(|(_, active)| active == dismissed)
                    {
                        this.menu = None;
                        cx.notify();
                    }
                }),
            );
        menu.focus_handle(cx).focus(window);
        self.menu = Some((kind, menu));
        cx.notify();
    }

    fn dismiss_title_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.menu.take().is_some() {
            if let Some(previous) = self.menu_previous_focus.take() {
                previous.focus(window);
            }
            self.menu_subscription = None;
            cx.notify();
        }
    }

    pub(super) fn render_title_menu(&self) -> Option<AnyElement> {
        let (kind, menu) = self.menu.as_ref()?;
        let layout = self.title_bar.layout.as_ref()?;
        let bounds = self.title_bar.bounds.borrow()[layout.anchor_index(*kind)];
        Some(
            deferred(
                anchored()
                    .position(bounds.bottom_left())
                    .snap_to_window_with_margin(px(4.))
                    .child(
                        div()
                            .debug_selector(|| "title-menu-popup".into())
                            .child(menu.clone()),
                    ),
            )
            .into_any_element(),
        )
    }

    pub(super) fn render_title_bar(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mode = HeaderMode::current(window);
        let rem = window.rem_size();
        let font = font(cx.theme().font_family.clone());
        let widths = std::array::from_fn(|ix| {
            let label = if ix == 6 { MenuKind::More } else { MENUS[ix] }.label();
            let line = window.text_system().shape_line(
                label.into(),
                rem * 0.875,
                &[TextRun {
                    len: label.len(),
                    font: font.clone(),
                    color: cx.theme().foreground,
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                }],
                None,
            );
            // Match small Button's px_3 padding and two one-pixel borders.
            line.width.ceil() + rem * 1.5 + px(2.)
        });
        let viewport = window.viewport_size();
        let mut width = viewport.width;
        if cfg!(target_os = "linux") {
            let padding = gpui_component::window_paddings(window);
            width -= padding.left + padding.right;
            if let Decorations::Client { tiling } = window.window_decorations() {
                if !tiling.left {
                    width -= px(1.);
                }
                if !tiling.right {
                    width -= px(1.);
                }
            }
        }
        let layout = HeaderLayout::new(mode, viewport, width, rem, widths);
        if self
            .title_bar
            .layout
            .as_ref()
            .is_some_and(|old| old != &layout)
        {
            self.dismiss_title_menu(window, cx);
        }
        self.title_bar.layout = Some(layout.clone());
        let mut bar = div()
            .id("window-title-bar")
            .debug_selector(|| "window-title-bar".into())
            .flex()
            .items_center()
            .flex_shrink_0()
            .h(layout.height)
            .w_full()
            .border_b_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().background)
            .pl(px(4.))
            .pr(px(if mode.controls() { 0. } else { 4. }));
        if mode == HeaderMode::Mac {
            bar = bar.child(div().w(px(80.)).flex_shrink_0());
        }
        if mode.combined() {
            bar = bar.child(
                div()
                    .debug_selector(|| "title-bar-logo".into())
                    .flex()
                    .items_center()
                    .justify_center()
                    .w(px(28.))
                    .flex_shrink_0()
                    .child(img("brand/mithril.png").size(px(16.))),
            );
        }
        let mut menus = div().flex().items_center().gap(layout.gap).flex_shrink_0();
        for kind in MENUS[..layout.visible]
            .iter()
            .copied()
            .chain((layout.visible < 6).then_some(MenuKind::More))
        {
            let bounds = self.title_bar.bounds.clone();
            menus = menus.child(
                div()
                    .relative()
                    .flex_shrink_0()
                    .w(layout.widths[kind.index()])
                    .child(
                        Button::new(kind.selector())
                            .debug_selector(move || kind.selector().into())
                            .cursor_default()
                            .ghost()
                            .small()
                            .w_full()
                            .label(kind.label())
                            .on_click(cx.listener(move |this, _, window, cx| {
                                cx.stop_propagation();
                                this.show_menu(kind, window, cx);
                            })),
                    )
                    .child(
                        canvas(
                            move |rect, _, _| {
                                bounds.borrow_mut()[kind.index()] = rect;
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .top_0()
                        .left_0()
                        .size_full(),
                    ),
            );
        }
        bar = bar.child(menus);
        if mode.combined() {
            // No ancestor of menus or controls participates in dragging.
            let mut drag = div()
                .id("title-bar-drag")
                .debug_selector(|| "title-bar-drag".into())
                .flex_1()
                .min_w(px(32.))
                .h_full();
            if mode == HeaderMode::Windows {
                drag = drag.window_control_area(WindowControlArea::Drag);
            } else {
                drag = drag.on_mouse_down(MouseButton::Left, move |event, window, cx| {
                    cx.stop_propagation();
                    if event.click_count == 2 {
                        if mode == HeaderMode::Mac {
                            window.titlebar_double_click();
                        } else {
                            window.zoom_window();
                        }
                    } else {
                        window.start_window_move();
                    }
                });
            }
            if mode != HeaderMode::Mac {
                drag = drag.on_mouse_down(MouseButton::Right, |event, window, cx| {
                    cx.stop_propagation();
                    window.show_window_menu(event.position);
                });
            }
            bar = bar.child(drag);
        }
        if mode.controls() {
            for (id, icon, area) in [
                (
                    "window-minimize",
                    IconName::WindowMinimize,
                    WindowControlArea::Min,
                ),
                (
                    "window-maximize",
                    if window.is_maximized() {
                        IconName::WindowRestore
                    } else {
                        IconName::WindowMaximize
                    },
                    WindowControlArea::Max,
                ),
                (
                    "window-close",
                    IconName::WindowClose,
                    WindowControlArea::Close,
                ),
            ] {
                let supported = window.window_controls();
                let enabled = match area {
                    WindowControlArea::Min => supported.minimize,
                    WindowControlArea::Max => supported.maximize,
                    _ => true,
                };
                let mut control = div()
                    .id(id)
                    .debug_selector(move || id.into())
                    .flex()
                    .items_center()
                    .justify_center()
                    .w(px(34.))
                    .h_full()
                    .flex_shrink_0()
                    .hover(|style| {
                        if area == WindowControlArea::Close {
                            style
                                .bg(cx.theme().danger)
                                .text_color(cx.theme().danger_foreground)
                        } else {
                            style.bg(cx.theme().secondary_hover)
                        }
                    })
                    .child(Icon::new(icon).small());
                if enabled && mode == HeaderMode::Windows {
                    // Leave events unhandled so GPUI can finish its non-client press/release pair.
                    control = control.window_control_area(area);
                } else if enabled {
                    control = control.on_click(cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        match area {
                            WindowControlArea::Min => window.minimize_window(),
                            WindowControlArea::Max => window.zoom_window(),
                            WindowControlArea::Close => this.request_window_close(window, cx),
                            WindowControlArea::Drag => unreachable!(),
                        }
                    }));
                }
                bar = bar.child(control);
            }
        }
        bar.into_any_element()
    }
}

#[cfg(test)]
mod tests;
