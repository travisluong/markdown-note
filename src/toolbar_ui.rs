//! Region-based toolbar presentation; actions retain their existing owners.
use crate::Notes;
#[cfg(test)]
use gpui::px;
use gpui::{Context, IntoElement, WeakEntity, Window, div, prelude::*};
use gpui_component::WindowExt;
use gpui_component::{
    ActiveTheme, Disableable, Icon, IconName, Selectable, Sizable,
    button::{Button, ButtonVariants},
    menu::{DropdownMenu, PopupMenu, PopupMenuItem},
};
use mithril::{
    markdown_edit::Format,
    notebook::{ToolbarIconSize, ToolbarItem, ToolbarRegion},
};

impl Notes {
    /// Resolve panel-dependent placement once for rendering, sizing and visibility.
    fn toolbar_items(&self, region: ToolbarRegion) -> Vec<ToolbarItem> {
        let settings = self.toolbar_settings();
        if !settings.visible {
            return Vec::new();
        }
        ToolbarItem::DEFAULT_ORDER
            .into_iter()
            .filter(|item| {
                if !settings.enabled(*item) {
                    return false;
                }
                let placement = match item {
                    ToolbarItem::Terminal if self.right_panel_open => return false,
                    ToolbarItem::Sidebar if self.notebooks.active.is_none() => return false,
                    ToolbarItem::Sidebar if self.sidebar_collapsed => ToolbarRegion::ContentLeft,
                    _ => item.region(),
                };
                placement == region
            })
            .collect()
    }

    pub(super) fn toolbar_region_visible(&self, region: ToolbarRegion) -> bool {
        !self.toolbar_items(region).is_empty()
    }

    pub(super) fn render_sidebar_toolbar(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let zoom = self.appearance().zoom_factor();
        let px = |value| gpui::px(value * zoom);
        div()
            .id("sidebar-toolbar")
            .debug_selector(|| "sidebar-toolbar".into())
            .w_full()
            .flex_shrink_0()
            .p(px(4.))
            .border_b_1()
            .border_color(cx.theme().border)
            .child(self.render_toolbar_region(ToolbarRegion::Sidebar, window, cx))
    }

    pub(super) fn render_content_toolbar(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let zoom = self.appearance().zoom_factor();
        let px = |value| gpui::px(value * zoom);
        let settings = self.toolbar_settings();
        let width = |region| {
            let count = self.toolbar_items(region).len();
            count as f32 * settings.icon_size.button_pixels() + count.saturating_sub(1) as f32 * 4.
        };
        let side = width(ToolbarRegion::ContentLeft).max(width(ToolbarRegion::ContentRight));
        let center = width(ToolbarRegion::ContentCenter);
        let wrapped = self.content_toolbar_width < px(8. + side * 2. + center + 8.);
        // Taffy's intrinsic wrapped height can retain the min-content estimate.
        // Derive the toolbar height from the same available width as its icon rows.
        let button = settings.icon_size.button_pixels();
        let per_row = ((f32::from(self.content_toolbar_width) / zoom - 4.) / (button + 4.))
            .floor()
            .max(1.);
        let formatting_count = ((center + 4.) / (button + 4.)).floor();
        let rows = if wrapped {
            (formatting_count / per_row).ceil() as usize + usize::from(side > 0.)
        } else {
            1
        };
        let height = rows as f32 * button + rows.saturating_sub(1) as f32 * 4. + 9.;
        let owner = cx.weak_entity();
        let left = self.render_toolbar_region(ToolbarRegion::ContentLeft, window, cx);
        let middle = self.render_toolbar_region(ToolbarRegion::ContentCenter, window, cx);
        let right = self.render_toolbar_region(ToolbarRegion::ContentRight, window, cx);
        let row = div()
            .flex()
            .items_center()
            .w_full()
            .gap(px(4.))
            .child(div().w(px(side)).flex_shrink_0().child(left));
        let row = if wrapped {
            row.child(div().flex_1())
        } else {
            row.child(div().flex_1().min_w_0().child(middle))
        };
        let row = row.child(
            div()
                .w(px(side))
                .flex_shrink_0()
                .flex()
                .justify_end()
                .child(right),
        );
        div()
            .h(px(height))
            .id("content-toolbar")
            .debug_selector(|| "content-toolbar".into())
            .relative()
            .flex()
            .flex_col()
            .w_full()
            .flex_shrink_0()
            .gap(px(4.))
            .p(px(4.))
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                gpui::canvas(
                    move |bounds, _, cx| {
                        cx.defer(move |cx| {
                            let _ = owner.update(cx, |this, cx| {
                                if this.content_toolbar_width != bounds.size.width {
                                    this.content_toolbar_width = bounds.size.width;
                                    cx.notify();
                                }
                            });
                        });
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .when(side > 0. || !wrapped, |view| view.child(row))
            .when(wrapped && center > 0., |view| {
                view.child(self.render_toolbar_region(ToolbarRegion::ContentCenter, window, cx))
            })
    }

    fn render_toolbar_region(
        &self,
        region: ToolbarRegion,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let zoom = self.appearance().zoom_factor();
        let px = |value| gpui::px(value * zoom);
        let settings = self.toolbar_settings();
        let has_notebook = self.notebooks.active.is_some();
        let disabled = self.document_interaction_blocked() || !has_notebook;
        let sidebar_visible = has_notebook && !self.sidebar_collapsed;
        let searching = self.search_visible(cx);
        let owner = cx.weak_entity();

        div()
            .flex()
            .flex_wrap()
            .items_center()
            .min_w_0()
            .w_full()
            .gap(px(4.))
            .when(region == ToolbarRegion::ContentCenter, |view| {
                view.justify_center()
            })
            .when(region == ToolbarRegion::ContentRight, |view| {
                view.justify_end()
            })
            .children(self.toolbar_items(region).into_iter().map(|item| {
                match item {
                    ToolbarItem::Back | ToolbarItem::Forward => {
                        let (id, icon, direction) = if item == ToolbarItem::Back {
                            (
                                "toolbar-back",
                                IconName::ArrowLeft,
                                crate::app::navigation::Direction::Back,
                            )
                        } else {
                            (
                                "toolbar-forward",
                                IconName::ArrowRight,
                                crate::app::navigation::Direction::Forward,
                            )
                        };
                        toolbar_icon_button(
                            zoom,
                            id,
                            icon,
                            settings.icon_size,
                            self.document_interaction_blocked()
                                || window.has_active_dialog(cx)
                                || self.tabs.active.navigation.target(direction).is_none(),
                        )
                        .tooltip(item.label())
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.navigate_history(direction, window, cx)
                        }))
                        .into_any_element()
                    }
                    ToolbarItem::Terminal => toolbar_icon_button(
                        zoom,
                        "toolbar-terminal",
                        IconName::PanelRight,
                        settings.icon_size,
                        self.document_interaction_blocked() || window.has_active_dialog(cx),
                    )
                    .tooltip("Open terminal panel")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.toggle_right_panel(window, cx);
                    }))
                    .into_any_element(),
                    ToolbarItem::Sidebar => toolbar_icon_button(
                        zoom,
                        "toolbar-sidebar-toggle",
                        IconName::PanelLeft,
                        settings.icon_size,
                        self.document_interaction_blocked()
                            || window.has_active_dialog(cx)
                            || !has_notebook,
                    )
                    .tooltip(if sidebar_visible {
                        "Collapse sidebar"
                    } else {
                        "Show sidebar"
                    })
                    .on_click(cx.listener(|this, _, window, cx| this.toggle_sidebar(window, cx)))
                    .into_any_element(),
                    ToolbarItem::Explorer => toolbar_icon_button(
                        zoom,
                        "sidebar-explorer",
                        IconName::Folder,
                        settings.icon_size,
                        disabled,
                    )
                    .tooltip(if cfg!(target_os = "macos") {
                        "Explorer (Cmd+Shift+E)"
                    } else {
                        "Explorer (Ctrl+Shift+E)"
                    })
                    .selected(sidebar_visible && !searching)
                    .on_click(cx.listener(|this, _, window, cx| this.show_explorer(window, cx)))
                    .into_any_element(),
                    ToolbarItem::Search => toolbar_icon_button(
                        zoom,
                        "sidebar-search",
                        IconName::Search,
                        settings.icon_size,
                        disabled,
                    )
                    .tooltip(if cfg!(target_os = "macos") {
                        "Search all files (Cmd+Shift+F)"
                    } else {
                        "Search all files (Ctrl+Shift+F)"
                    })
                    .selected(sidebar_visible && searching)
                    .on_click(cx.listener(|this, _, window, cx| this.open_search_all(window, cx)))
                    .into_any_element(),
                    ToolbarItem::Sort => toolbar_icon_button(
                        zoom,
                        "sidebar-sort",
                        Icon::default().path("toolbar/sort.svg"),
                        settings.icon_size,
                        disabled,
                    )
                    .tooltip("Sort files")
                    .dropdown_menu({
                        let owner = owner.clone();
                        move |menu, _, cx| {
                            if let Some(entity) = owner.upgrade() {
                                entity.read(cx).file_sort_menu(menu, owner.clone())
                            } else {
                                menu
                            }
                        }
                    })
                    .trigger_on_click(true)
                    .into_any_element(),
                    item => {
                        let (id, path, format) = formatting_item(item);
                        let button = toolbar_icon_button(
                            zoom,
                            id,
                            Icon::default().path(path),
                            settings.icon_size,
                            !self.can_edit_markdown(),
                        )
                        .tooltip(item.label());
                        if item == ToolbarItem::Heading {
                            button
                                .dropdown_menu({
                                    let owner = owner.clone();
                                    move |menu, _, cx| {
                                        if let Some(entity) = owner.upgrade() {
                                            entity.read(cx).heading_menu(menu, owner.clone(), cx)
                                        } else {
                                            menu
                                        }
                                    }
                                })
                                .trigger_on_click(true)
                                .into_any_element()
                        } else {
                            button
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    if item == ToolbarItem::Link {
                                        this.edit_link(window, cx);
                                    } else if let Some(format) = format {
                                        this.apply_markdown_format(format, window, cx);
                                    }
                                }))
                                .into_any_element()
                        }
                    }
                }
            }))
    }
}

/// Keep icon geometry independent of Button's built-in icon-to-target ratio.
pub(super) fn toolbar_icon_button(
    zoom: f32,
    id: &'static str,
    icon: impl Into<Icon>,
    size: ToolbarIconSize,
    disabled: bool,
) -> Button {
    let px = |value| gpui::px(value * zoom);
    Button::new(id)
        .debug_selector(move || id.into())
        .ghost()
        .flex_shrink_0()
        .disabled(disabled)
        .with_size(px(size.button_pixels()))
        .size(px(size.button_pixels()))
        .p_0()
        .child(
            div()
                .debug_selector(move || format!("{id}-icon"))
                .size(px(size.icon_pixels()))
                .child(
                    Icon::new(icon)
                        .themed_color(!disabled)
                        .with_size(px(size.icon_pixels())),
                ),
        )
}

impl Notes {
    pub(super) fn apply_markdown_format(
        &mut self,
        format: Format,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.can_edit_markdown() && !window.has_active_dialog(cx) {
            self.tabs.active.editor.format_markdown(format, window, cx);
        }
    }

    fn heading_menu(
        &self,
        mut menu: PopupMenu,
        owner: WeakEntity<Self>,
        cx: &gpui::App,
    ) -> PopupMenu {
        menu = menu.action_context(self.tabs.active.editor.focus_handle(cx));
        let editor = self.tabs.active.editor.id();
        let document = self.document_id();
        let snapshot = self.tabs.active.editor.snapshot(cx);
        let selection = self.tabs.active.editor.selection(cx);
        for (level, label) in [
            (1, "Heading 1"),
            (2, "Heading 2"),
            (3, "Heading 3"),
            (4, "Heading 4"),
            (5, "Heading 5"),
            (6, "Heading 6"),
            (0, "Paragraph"),
        ] {
            let owner = owner.clone();
            let snapshot = snapshot.clone();
            let selection = selection.clone();
            menu = menu.item(
                PopupMenuItem::new(label)
                    .disabled(!self.can_edit_markdown())
                    .on_click(move |_, window, cx| {
                        let _ = owner.update(cx, |this, cx| {
                            if this.tabs.active.editor.id() == editor
                                && this.document_id() == document
                                && this.tabs.active.editor.snapshot(cx) == snapshot
                                && this.tabs.active.editor.selection(cx) == selection
                            {
                                this.apply_markdown_format(Format::Heading(level), window, cx);
                            }
                        });
                    }),
            );
        }
        menu
    }
}

fn formatting_item(item: ToolbarItem) -> (&'static str, &'static str, Option<Format>) {
    match item {
        ToolbarItem::Bold => ("toolbar-bold", "toolbar/bold.svg", Some(Format::Bold)),
        ToolbarItem::Italic => ("toolbar-italic", "toolbar/italic.svg", Some(Format::Italic)),
        ToolbarItem::Strikethrough => (
            "toolbar-strikethrough",
            "toolbar/strikethrough.svg",
            Some(Format::Strikethrough),
        ),
        ToolbarItem::Heading => ("toolbar-heading", "toolbar/heading.svg", None),
        ToolbarItem::InlineCode => (
            "toolbar-inline-code",
            "toolbar/inlinecode.svg",
            Some(Format::InlineCode),
        ),
        ToolbarItem::CodeBlock => (
            "toolbar-code-block",
            "toolbar/codeblock.svg",
            Some(Format::CodeBlock),
        ),
        ToolbarItem::Link => ("toolbar-link", "toolbar/link.svg", None),
        ToolbarItem::Blockquote => (
            "toolbar-blockquote",
            "toolbar/blockquote.svg",
            Some(Format::Blockquote),
        ),
        ToolbarItem::OrderedList => (
            "toolbar-ordered-list",
            "toolbar/orderedlist.svg",
            Some(Format::OrderedList),
        ),
        ToolbarItem::UnorderedList => (
            "toolbar-unordered-list",
            "toolbar/unorderedlist.svg",
            Some(Format::UnorderedList),
        ),
        ToolbarItem::TaskList => (
            "toolbar-task-list",
            "toolbar/tasklist.svg",
            Some(Format::TaskList),
        ),
        _ => unreachable!("navigation controls use their existing owners"),
    }
}
#[cfg(test)]
mod tests;
