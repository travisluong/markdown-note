//! Window-wide toolbar presentation; sidebar state and actions retain their owners.
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
    notebook::{ToolbarIconSize, ToolbarItem},
};

impl Notes {
    pub(super) fn render_toolbar(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let settings = self.toolbar_settings();
        let has_notebook = self.notebooks.active.is_some();
        let disabled = self.document_interaction_blocked() || !has_notebook;
        let sidebar_visible = has_notebook && !self.sidebar_collapsed;
        let searching = self.search_visible(cx);
        let owner = cx.weak_entity();
        let can_reorder = !self.document_interaction_blocked() && !window.has_active_dialog(cx);

        div()
            .id("window-toolbar")
            .debug_selector(|| "window-toolbar".into())
            .flex()
            .flex_wrap()
            .items_center()
            .w_full()
            .flex_shrink_0()
            .gap_1()
            .p_1()
            .border_b_1()
            .border_color(cx.theme().border)
            .children(
                settings
                    .order
                    .into_iter()
                    .filter(|item| settings.enabled(*item))
                    .map(|item| {
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
                            ToolbarItem::Sidebar => toolbar_button(
                                "toolbar-sidebar-toggle",
                                IconName::PanelLeft,
                                settings.icon_size,
                                self.is_busy() || !has_notebook,
                                drag,
                            )
                            .tooltip(if sidebar_visible {
                                "Collapse sidebar"
                            } else {
                                "Show sidebar"
                            })
                            .on_click(
                                cx.listener(|this, _, window, cx| this.toggle_sidebar(window, cx)),
                            )
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
                            .on_click(
                                cx.listener(|this, _, window, cx| this.show_explorer(window, cx)),
                            )
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
                            .on_click(
                                cx.listener(|this, _, window, cx| this.open_search_all(window, cx)),
                            )
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
                                                    entity.read(cx).heading_menu(
                                                        menu,
                                                        owner.clone(),
                                                        cx,
                                                    )
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
                    }),
            )
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

/// Keep icon geometry independent of Button's built-in icon-to-target ratio.
fn toolbar_button(
    id: &'static str,
    icon: impl Into<Icon>,
    size: ToolbarIconSize,
    disabled: bool,
    drag: Option<WindowToolbarDrag>,
) -> Button {
    Button::new(id)
        .debug_selector(move || id.into())
        .ghost()
        .when_some(drag, |mut button, drag| {
            button.interactivity().on_drag(drag, |drag, _, _, cx| {
                cx.stop_propagation();
                cx.new(|_| drag.clone())
            });
            button
        })
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
