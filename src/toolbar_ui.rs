//! Region-based toolbar presentation; actions retain their existing owners.
use crate::Notes;
use gpui::{
    AppContext, Context, EntityId, IntoElement, Render, WeakEntity, Window, div, prelude::*, px,
};
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
        let mut items: Vec<_> = settings
            .order
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
            .collect();
        if region == ToolbarRegion::ContentLeft {
            // The collapsed opener is fixed before navigation, regardless of its saved sidebar order.
            items.sort_by_key(|item| *item != ToolbarItem::Sidebar);
        }
        items
    }

    pub(super) fn toolbar_region_visible(&self, region: ToolbarRegion) -> bool {
        !self.toolbar_items(region).is_empty()
    }

    pub(super) fn render_sidebar_toolbar(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
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
        let per_row = ((f32::from(self.content_toolbar_width) - 4.) / (button + 4.))
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
        let settings = self.toolbar_settings();
        let has_notebook = self.notebooks.active.is_some();
        let disabled = self.document_interaction_blocked() || !has_notebook;
        let sidebar_visible = has_notebook && !self.sidebar_collapsed;
        let searching = self.search_visible(cx);
        let owner = cx.weak_entity();
        let can_reorder = !self.document_interaction_blocked() && !window.has_active_dialog(cx);

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
                let can_reorder =
                    can_reorder && !(item == ToolbarItem::Sidebar && self.sidebar_collapsed);
                let drag = can_reorder.then(|| WindowToolbarDrag {
                    owner: cx.entity_id(),
                    scope: self.notebooks.active.as_ref().map(|book| book.root.clone()),
                    item,
                    size: settings.icon_size,
                });
                let button = match item {
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
                        toolbar_button(
                            id,
                            icon,
                            settings.icon_size,
                            self.document_interaction_blocked()
                                || window.has_active_dialog(cx)
                                || self.tabs.active.navigation.target(direction).is_none(),
                            drag,
                        )
                        .tooltip(item.label())
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.navigate_history(direction, window, cx)
                        }))
                        .into_any_element()
                    }
                    ToolbarItem::Terminal => toolbar_icon_button(
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
                    ToolbarItem::Sidebar => toolbar_button(
                        "toolbar-sidebar-toggle",
                        IconName::PanelLeft,
                        settings.icon_size,
                        self.document_interaction_blocked()
                            || window.has_active_dialog(cx)
                            || !has_notebook,
                        drag,
                    )
                    .tooltip(if sidebar_visible {
                        "Collapse sidebar"
                    } else {
                        "Show sidebar"
                    })
                    .on_click(cx.listener(|this, _, window, cx| this.toggle_sidebar(window, cx)))
                    .into_any_element(),
                    ToolbarItem::Explorer => toolbar_button(
                        "sidebar-explorer",
                        IconName::Folder,
                        settings.icon_size,
                        disabled,
                        drag,
                    )
                    .tooltip(if cfg!(target_os = "macos") {
                        "Explorer (Cmd+Shift+E)"
                    } else {
                        "Explorer (Ctrl+Shift+E)"
                    })
                    .selected(sidebar_visible && !searching)
                    .on_click(cx.listener(|this, _, window, cx| this.show_explorer(window, cx)))
                    .into_any_element(),
                    ToolbarItem::Search => toolbar_button(
                        "sidebar-search",
                        IconName::Search,
                        settings.icon_size,
                        disabled,
                        drag,
                    )
                    .tooltip(if cfg!(target_os = "macos") {
                        "Search all files (Cmd+Shift+F)"
                    } else {
                        "Search all files (Ctrl+Shift+F)"
                    })
                    .selected(sidebar_visible && searching)
                    .on_click(cx.listener(|this, _, window, cx| this.open_search_all(window, cx)))
                    .into_any_element(),
                    ToolbarItem::Sort => toolbar_button(
                        "sidebar-sort",
                        Icon::default().path("toolbar/sort.svg"),
                        settings.icon_size,
                        disabled,
                        drag,
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
                        let button = toolbar_button(
                            id,
                            Icon::default().path(path),
                            settings.icon_size,
                            !self.can_edit_markdown(),
                            drag,
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
                };
                self.toolbar_drop_target(item, settings.icon_size, button, can_reorder, cx)
            }))
    }
}

/// A window-toolbar gesture has separate authority from Settings list drags.
#[derive(Clone)]
struct WindowToolbarDrag {
    owner: EntityId,
    scope: Option<std::path::PathBuf>,
    item: ToolbarItem,
    size: ToolbarIconSize,
}

impl Render for WindowToolbarDrag {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let icon = match self.item {
            ToolbarItem::Back => Icon::new(IconName::ArrowLeft),
            ToolbarItem::Forward => Icon::new(IconName::ArrowRight),
            ToolbarItem::Terminal => Icon::new(IconName::PanelRight),
            ToolbarItem::Sidebar => Icon::new(IconName::PanelLeft),
            ToolbarItem::Explorer => Icon::new(IconName::Folder),
            ToolbarItem::Search => Icon::new(IconName::Search),
            ToolbarItem::Sort => Icon::default().path("toolbar/sort.svg"),
            item => Icon::default().path(formatting_item(item).1),
        };
        div()
            .size(px(self.size.button_pixels()))
            .flex()
            .items_center()
            .justify_center()
            .rounded_md()
            .bg(cx.theme().accent)
            .child(
                icon.themed_color(true)
                    .with_size(px(self.size.icon_pixels())),
            )
    }
}

impl Notes {
    fn accepts_toolbar_drag(
        &self,
        drag: &WindowToolbarDrag,
        target: ToolbarItem,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let settings = self.toolbar_settings();
        drag.owner == cx.entity_id()
            && drag.scope == self.notebooks.active.as_ref().map(|book| book.root.clone())
            && !self.document_interaction_blocked()
            && !window.has_active_dialog(cx)
            && drag.item.region() == target.region()
            && !(self.sidebar_collapsed
                && (drag.item == ToolbarItem::Sidebar || target == ToolbarItem::Sidebar))
            && settings.visible
            && settings.enabled(drag.item)
            && settings.enabled(target)
    }

    fn drop_window_toolbar_item(
        &mut self,
        drag: &WindowToolbarDrag,
        target: ToolbarItem,
        after: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.accepts_toolbar_drag(drag, target, window, cx) {
            self.move_toolbar_item(drag.item, target, after, window, cx);
        }
    }

    fn toolbar_drop_target(
        &self,
        item: ToolbarItem,
        size: ToolbarIconSize,
        button: gpui::AnyElement,
        can_reorder: bool,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let id = item as usize;
        let owner = cx.weak_entity();
        div()
            .id(("window-toolbar-item", id))
            .debug_selector(move || format!("window-toolbar-item-{id}"))
            .relative()
            .flex_shrink_0()
            .size(px(size.button_pixels()))
            .child(button)
            // Absolute halves preserve wrapping and icon geometry during hover.
            .when(can_reorder && cx.has_active_drag(), |view| {
                view.children([false, true].map(|after| {
                    let can_drop_owner = owner.clone();
                    let hover_owner = owner.clone();
                    div()
                        .id(("window-toolbar-drop", id * 2 + usize::from(after)))
                        .debug_selector(move || format!("window-toolbar-drop-{id}-{after}"))
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .w_1_2()
                        .when(after, |view| view.right_0())
                        .when(!after, |view| view.left_0())
                        .can_drop(move |value, window, cx| {
                            value
                                .downcast_ref::<WindowToolbarDrag>()
                                .is_some_and(|drag| {
                                    can_drop_owner
                                        .update(cx, |this, cx| {
                                            this.accepts_toolbar_drag(drag, item, window, cx)
                                        })
                                        .unwrap_or(false)
                                })
                        })
                        .drag_over::<WindowToolbarDrag>(move |style, drag, window, cx| {
                            let accepted = hover_owner
                                .update(cx, |this, cx| {
                                    this.accepts_toolbar_drag(drag, item, window, cx)
                                })
                                .unwrap_or(false);
                            if !accepted || drag.item == item {
                                return style;
                            }
                            let style = style.border_color(cx.theme().primary);
                            if after {
                                style.border_r_2()
                            } else {
                                style.border_l_2()
                            }
                        })
                        .on_drop(
                            cx.listener(move |this, drag: &WindowToolbarDrag, window, cx| {
                                cx.stop_propagation();
                                this.drop_window_toolbar_item(drag, item, after, window, cx);
                            }),
                        )
                }))
            })
            .into_any_element()
    }
}

fn toolbar_button(
    id: &'static str,
    icon: impl Into<Icon>,
    size: ToolbarIconSize,
    disabled: bool,
    drag: Option<WindowToolbarDrag>,
) -> Button {
    toolbar_icon_button(id, icon, size, disabled).when_some(drag, |mut button, drag| {
        button.interactivity().on_drag(drag, |drag, _, _, cx| {
            cx.stop_propagation();
            cx.new(|_| drag.clone())
        });
        button
    })
}

/// Keep icon geometry independent of Button's built-in icon-to-target ratio.
pub(super) fn toolbar_icon_button(
    id: &'static str,
    icon: impl Into<Icon>,
    size: ToolbarIconSize,
    disabled: bool,
) -> Button {
    Button::new(id)
        .debug_selector(move || id.into())
        .ghost()
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
