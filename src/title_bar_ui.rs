//! Application-owned menus and platform title-bar regions.
use crate::{
    About, CloseTab, CommandPalette, GoTo, KeyboardShortcuts, LivePreview, MenuKind,
    NavigateBack, NavigateForward, NewNote, NewTab, Next, Notes, Open,
    OpenSettings, ReadingView, ReplaceFile, ResetZoom, Save, SearchAllFiles, ShowExplorer,
    SourceMode, ToggleSidebar, ViewMode, ZoomIn, ZoomOut,
};
use gpui::{
    AnyElement, Bounds, Context, Decorations, DismissEvent, Focusable, IntoElement, MouseButton,
    Pixels, Size, TextRun, WeakEntity, Window, WindowControlArea, anchored, canvas, deferred, div,
    font, prelude::*, px,
};
use gpui_component::{
    ActiveTheme, Disableable, Selectable, Sizable, WindowExt,
    button::{Button, ButtonVariants},
    input::Search,
    menu::{PopupMenu, PopupMenuItem},
};
use gpui_component::{Icon, IconName};
use std::{cell::RefCell, rc::Rc};

#[cfg(target_os = "windows")]
mod windows;

mod edit;

impl Notes {
    fn populate_menu(
        &self,
        kind: MenuKind,
        menu: PopupMenu,
        owner: WeakEntity<Self>,
        window: &mut Window,
        cx: &mut Context<PopupMenu>,
    ) -> PopupMenu {
        let busy = self.document_interaction_blocked();
        let can_save = self.document().is_some() && !busy;
        let mode = self.tabs.active.mode;
        let capabilities = self.capabilities();
        if kind == MenuKind::File {
            menu.menu_with_disabled("Open…", Box::new(Open), busy)
                .menu_with_disabled("Save", Box::new(Save), !can_save)
                .separator()
                .menu_with_disabled(
                    "New note",
                    Box::new(NewNote),
                    !capabilities.can_create_note(),
                )
                .menu_with_disabled("New tab", Box::new(NewTab), busy)
                .menu_with_disabled("Close tab", Box::new(CloseTab), busy)
                .separator()
                .menu_with_disabled("Settings", Box::new(OpenSettings), self.is_busy())
        } else if kind == MenuKind::Edit {
            self.edit_menu(menu, owner, window, cx)
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
                busy || capabilities.source_only,
            )
            .menu_with_check_and_disabled(
                "Reading view",
                mode == ViewMode::Reading,
                Box::new(ReadingView),
                busy || capabilities.source_only,
            )
            .separator()
            .item(
                PopupMenuItem::new("Centered editor")
                    .checked(self.appearance().centered_editor)
                    .disabled(busy)
                    .on_click({
                        let owner = owner.clone();
                        move |_, window, cx| {
                            let _ = owner.update(cx, |this, cx| {
                                this.toggle_centered_editor(window, cx);
                            });
                        }
                    }),
            )
            .separator()
            .menu_with_disabled(
                "Back",
                Box::new(NavigateBack),
                mithril::commands::Command::Back
                    .unavailable(capabilities)
                    .is_some(),
            )
            .menu_with_disabled(
                "Forward",
                Box::new(NavigateForward),
                mithril::commands::Command::Forward
                    .unavailable(capabilities)
                    .is_some(),
            )
            .separator()
            .menu_with_check_and_disabled(
                "Collapse sidebar",
                self.sidebar_collapsed,
                Box::new(ToggleSidebar),
                busy || self.notebooks.active.is_none(),
            )
            .item(
                PopupMenuItem::new("Hide toolbars")
                    .checked(!self.toolbar_settings().visible)
                    .disabled(busy)
                    .on_click({
                        let owner = owner.clone();
                        move |_, window, cx| {
                            let _ = owner.update(cx, |this, cx| this.toggle_toolbar(window, cx));
                        }
                    }),
            )
            .menu_with_disabled(
                "Show Explorer",
                Box::new(ShowExplorer),
                busy || self.notebooks.active.is_none(),
            )
            .submenu("Sort files", window, cx, |submenu, _, _| {
                self.file_sort_menu(submenu, owner.clone())
            })
            .menu_with_disabled("Command palette…", Box::new(CommandPalette), busy)
            .separator()
            .item(
                PopupMenuItem::new("Toggle light/dark mode")
                    .disabled(busy)
                    .on_click(move |_, window, cx| {
                        let _ = owner.update(cx, |this, cx| this.toggle_theme(window, cx));
                    }),
            )
            .submenu("Zoom", window, cx, |submenu, _, _| {
                submenu
                    .menu_with_disabled("Zoom in", Box::new(ZoomIn), busy)
                    .menu_with_disabled("Zoom out", Box::new(ZoomOut), busy)
                    .menu_with_disabled("Reset zoom", Box::new(ResetZoom), busy)
            })
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
            .separator()
            .menu_with_disabled("Go to file…", Box::new(GoTo), busy)
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
    MenuKind::Edit,
    MenuKind::View,
    MenuKind::Search,
    MenuKind::Notebooks,
    MenuKind::Help,
];

impl MenuKind {
    fn label(self) -> &'static str {
        match self {
            Self::File => "File",
            Self::Edit => "Edit",
            Self::View => "View",
            Self::Search => "Search",
            Self::Notebooks => "Notebooks",
            Self::Help => "Help",
            Self::More => "More",
        }
    }
    fn selector(self) -> &'static str {
        match self {
            Self::File => "file-menu",
            Self::Edit => "edit-menu",
            Self::View => "view-menu",
            Self::Search => "search-menu",
            Self::Notebooks => "notebooks-menu",
            Self::Help => "help-menu",
            Self::More => "more-menu",
        }
    }
    fn index(self) -> usize {
        MENUS
            .iter()
            .position(|kind| *kind == self)
            .unwrap_or(MENUS.len())
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
    widths: [Pixels; MENUS.len() + 1],
    gap: Pixels,
    terminal_gap: Pixels,
    visible: usize,
    panel_buttons: [bool; 2],
}
impl HeaderLayout {
    fn new(
        mode: HeaderMode,
        viewport: Size<Pixels>,
        width: Pixels,
        text_scale: (Pixels, f32),
        widths: [Pixels; MENUS.len() + 1],
        panel_buttons: [bool; 2],
    ) -> Self {
        let (rem, zoom) = text_scale;
        let gap = rem * 0.25;
        let terminal_gap = if mode.controls() && panel_buttons[1] {
            px(4.)
        } else {
            px(0.)
        };
        let reserved = px(if mode.controls() { 4. } else { 8. })
            + terminal_gap
            + px(28. * zoom) * panel_buttons.into_iter().filter(|visible| *visible).count() as f32
            + if mode.combined() { px(32.) } else { px(0.) }
            + if mode == HeaderMode::Mac {
                px(80.)
            } else {
                px(0.)
            }
            + if mode.controls() { px(102.) } else { px(0.) };
        let available = width - reserved;
        let total = widths[..MENUS.len()]
            .iter()
            .fold(px(0.), |total, width| total + *width)
            + gap * (MENUS.len() - 1) as f32;
        let visible = if total <= available {
            MENUS.len()
        } else {
            let mut count = 0;
            let mut used = widths[MENUS.len()];
            while count < MENUS.len() && used + widths[count] + gap <= available {
                used += widths[count] + gap;
                count += 1;
            }
            count
        };
        Self {
            mode,
            viewport,
            width,
            height: px(34. * zoom).max(rem * 1.5 + px(8. * zoom)),
            widths,
            gap,
            terminal_gap,
            visible,
            panel_buttons,
        }
    }
    fn anchor_index(&self, kind: MenuKind) -> usize {
        if kind.index() < self.visible {
            kind.index()
        } else {
            MENUS.len()
        }
    }
}

#[derive(Default)]
pub(super) struct TitleBarState {
    layout: Option<HeaderLayout>,
    bounds: Rc<RefCell<[Bounds<Pixels>; MENUS.len() + 1]>>,
    #[cfg(all(test, target_os = "windows"))]
    commands: Vec<windows::CaptionCommand>,
}

impl Notes {
    #[cfg(target_os = "windows")]
    fn windows_caption_command(&mut self, command: windows::CaptionCommand, _window: &Window) {
        #[cfg(test)]
        self.title_bar.commands.push(command);
        #[cfg(not(test))]
        if let Err(error) = windows::post_command(command, _window) {
            eprintln!("Could not perform window command: {error}");
        }
    }

    // Native and custom caption close use the same lifecycle owner.
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
            .map_or(MENUS.len(), |layout| layout.visible);
        let menu = PopupMenu::build(window, cx, |menu, window, cx| {
            let mut menu = menu.action_context(focus.clone());
            if kind == MenuKind::More {
                for kind in &MENUS[visible..] {
                    menu = menu.submenu(kind.label(), window, cx, |submenu, window, cx| {
                        self.populate_menu(
                            *kind,
                            submenu.action_context(focus.clone()),
                            owner.clone(),
                            window,
                            cx,
                        )
                    });
                }
                menu
            } else {
                self.populate_menu(kind, menu, owner, window, cx)
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
            let label = if ix == MENUS.len() {
                MenuKind::More
            } else {
                MENUS[ix]
            }
            .label();
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
        let settings = self.toolbar_settings();
        let layout = HeaderLayout::new(
            mode,
            viewport,
            width,
            (rem, self.appearance().zoom_factor()),
            widths,
            [settings.title_bar_sidebar, settings.title_bar_terminal],
        );
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
        if settings.title_bar_sidebar {
            bar = bar.child(
                Button::new("title-bar-sidebar-toggle")
                    .debug_selector(|| "title-bar-sidebar-toggle".into())
                    .cursor_default()
                    .ghost()
                    .small()
                    .w(px(28. * self.appearance().zoom_factor()))
                    .flex_shrink_0()
                    .icon(
                        Icon::new(IconName::PanelLeft)
                            .size(px(16. * self.appearance().zoom_factor())),
                    )
                    .tooltip(if self.sidebar_collapsed {
                        "Show sidebar"
                    } else {
                        "Collapse sidebar"
                    })
                    .disabled(
                        self.notebooks.active.is_none()
                            || self.document_interaction_blocked()
                            || window.has_active_dialog(cx),
                    )
                    .on_click(cx.listener(|this, _, window, cx| {
                        cx.stop_propagation();
                        this.toggle_sidebar(window, cx);
                    })),
            );
        }
        let mut menus = div().flex().items_center().gap(layout.gap).flex_shrink_0();
        for kind in MENUS[..layout.visible]
            .iter()
            .copied()
            .chain((layout.visible < MENUS.len()).then_some(MenuKind::More))
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
                #[cfg(target_os = "windows")]
                {
                    drag = drag.on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, event: &gpui::MouseDownEvent, window, cx| {
                            // Handle both client and non-client input. Stop the native
                            // default handler from starting a second move/zoom operation.
                            cx.stop_propagation();
                            let command = if event.click_count == 2 {
                                windows::CaptionCommand::zoom(window.is_maximized())
                            } else {
                                windows::CaptionCommand::Move
                            };
                            this.windows_caption_command(command, window);
                        }),
                    );
                }
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
        } else {
            bar = bar.child(div().flex_1());
        }
        if settings.title_bar_terminal {
            bar = bar.child(
                Button::new("title-bar-terminal-toggle")
                    .debug_selector(|| "title-bar-terminal-toggle".into())
                    .cursor_default()
                    .ghost()
                    .small()
                    .w(px(28. * self.appearance().zoom_factor()))
                    .mr(layout.terminal_gap)
                    .flex_shrink_0()
                    .icon(
                        Icon::new(IconName::PanelRight)
                            .size(px(16. * self.appearance().zoom_factor())),
                    )
                    .tooltip(if self.right_panel_open {
                        "Close terminal panel"
                    } else {
                        "Open terminal panel"
                    })
                    .selected(self.right_panel_open)
                    .disabled(self.document_interaction_blocked() || window.has_active_dialog(cx))
                    .on_click(cx.listener(|this, _, window, cx| {
                        cx.stop_propagation();
                        this.toggle_right_panel(window, cx);
                    })),
            );
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
                    .text_color(if enabled {
                        cx.theme().icon_color.unwrap_or(cx.theme().foreground)
                    } else {
                        cx.theme().muted_foreground
                    })
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
                    // Keep native hit regions (including maximize's Snap hover), but
                    // execute clicks ourselves even when Windows sends client input.
                    control = control.window_control_area(area);
                }
                if enabled {
                    control = control
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_click(cx.listener(move |this, _, window, cx| {
                            cx.stop_propagation();
                            #[cfg(target_os = "windows")]
                            if mode == HeaderMode::Windows && area != WindowControlArea::Close {
                                let command = match area {
                                    WindowControlArea::Min => windows::CaptionCommand::Minimize,
                                    WindowControlArea::Max => {
                                        windows::CaptionCommand::zoom(window.is_maximized())
                                    }
                                    _ => unreachable!(),
                                };
                                this.windows_caption_command(command, window);
                                return;
                            }
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
