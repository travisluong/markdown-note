//! Window-wide toolbar presentation; sidebar state and actions retain their owners.
use crate::Notes;
use gpui::{Context, IntoElement, WeakEntity, Window, div, prelude::*, px};
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
                    .map(|item| match item {
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
                            toolbar_button(id, icon, settings.icon_size)
                                .tooltip(item.label())
                                .disabled(
                                    self.document_interaction_blocked()
                                        || window.has_active_dialog(cx)
                                        || self.tabs.active.navigation.target(direction).is_none(),
                                )
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.navigate_history(direction, window, cx)
                                }))
                                .into_any_element()
                        }
                        ToolbarItem::Sidebar => toolbar_button(
                            "toolbar-sidebar-toggle",
                            IconName::PanelLeft,
                            settings.icon_size,
                        )
                        .tooltip(if sidebar_visible {
                            "Collapse sidebar"
                        } else {
                            "Show sidebar"
                        })
                        .disabled(self.is_busy() || !has_notebook)
                        .on_click(
                            cx.listener(|this, _, window, cx| this.toggle_sidebar(window, cx)),
                        )
                        .into_any_element(),
                        ToolbarItem::Explorer => {
                            toolbar_button("sidebar-explorer", IconName::Folder, settings.icon_size)
                                .tooltip(if cfg!(target_os = "macos") {
                                    "Explorer (Cmd+Shift+E)"
                                } else {
                                    "Explorer (Ctrl+Shift+E)"
                                })
                                .selected(sidebar_visible && !searching)
                                .disabled(disabled)
                                .on_click(
                                    cx.listener(|this, _, window, cx| {
                                        this.show_explorer(window, cx)
                                    }),
                                )
                                .into_any_element()
                        }
                        ToolbarItem::Search => {
                            toolbar_button("sidebar-search", IconName::Search, settings.icon_size)
                                .tooltip(if cfg!(target_os = "macos") {
                                    "Search all files (Cmd+Shift+F)"
                                } else {
                                    "Search all files (Ctrl+Shift+F)"
                                })
                                .selected(sidebar_visible && searching)
                                .disabled(disabled)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.open_search_all(window, cx)
                                }))
                                .into_any_element()
                        }
                        ToolbarItem::Sort => toolbar_button(
                            "sidebar-sort",
                            Icon::default().path("toolbar/sort.svg"),
                            settings.icon_size,
                        )
                        .tooltip("Sort files")
                        .disabled(disabled)
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
                        .into_any_element(),
                        item => {
                            let (id, path, format) = formatting_item(item);
                            let button =
                                toolbar_button(id, Icon::default().path(path), settings.icon_size)
                                    .tooltip(item.label())
                                    .disabled(!self.can_edit_document());
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
                    }),
            )
    }
}

/// Keep icon geometry independent of Button's built-in icon-to-target ratio.
fn toolbar_button(id: &'static str, icon: impl Into<Icon>, size: ToolbarIconSize) -> Button {
    Button::new(id)
        .debug_selector(move || id.into())
        .ghost()
        .with_size(px(size.button_pixels()))
        .size(px(size.button_pixels()))
        .p_0()
        .child(
            div()
                .debug_selector(move || format!("{id}-icon"))
                .size(px(size.icon_pixels()))
                .child(Icon::new(icon).with_size(px(size.icon_pixels()))),
        )
}

impl Notes {
    pub(super) fn apply_markdown_format(
        &mut self,
        format: Format,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.can_edit_document() && !window.has_active_dialog(cx) {
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
                    .disabled(!self.can_edit_document())
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
