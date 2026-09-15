//! Window-wide toolbar presentation; sidebar state and actions retain their owners.
use crate::Notes;
use gpui::{Context, IntoElement, div, prelude::*, px};
use gpui_component::{
    ActiveTheme, Disableable, Icon, IconName, Selectable, Sizable,
    button::{Button, ButtonVariants},
    menu::DropdownMenu,
};
use mithril::notebook::{ToolbarIconSize, ToolbarItem};

impl Notes {
    pub(super) fn render_toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let settings = self.toolbar_settings();
        let has_notebook = self.notebooks.active.is_some();
        let disabled = self.is_busy() || !has_notebook;
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
                        ToolbarItem::Sidebar => toolbar_button(
                            "toolbar-sidebar-toggle",
                            if sidebar_visible {
                                IconName::PanelLeftClose
                            } else {
                                IconName::PanelLeftOpen
                            },
                            settings.icon_size,
                        )
                        .tooltip(if sidebar_visible {
                            "Collapse sidebar"
                        } else {
                            "Show sidebar"
                        })
                        .disabled(disabled)
                        .on_click(
                            cx.listener(|this, _, window, cx| this.toggle_sidebar(window, cx)),
                        )
                        .into_any_element(),
                        ToolbarItem::Explorer => {
                            toolbar_button("sidebar-explorer", IconName::Folder, settings.icon_size)
                                .tooltip("Explorer")
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
                                .tooltip("Search all files (Ctrl+Shift+F)")
                                .selected(sidebar_visible && searching)
                                .disabled(disabled)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.open_search_all(window, cx)
                                }))
                                .into_any_element()
                        }
                        ToolbarItem::Sort => toolbar_button(
                            "sidebar-sort",
                            IconName::SortAscending,
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
                    }),
            )
    }
}

/// Keep icon geometry independent of Button's built-in icon-to-target ratio.
fn toolbar_button(id: &'static str, icon: IconName, size: ToolbarIconSize) -> Button {
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
