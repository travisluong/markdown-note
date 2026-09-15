//! Window-wide toolbar presentation; sidebar state and actions retain their owners.
use crate::Notes;
use gpui::{Context, IntoElement, div, prelude::*};
use gpui_component::{
    ActiveTheme, Disableable, IconName, Selectable, Sizable,
    button::{Button, ButtonVariants},
    menu::DropdownMenu,
};

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
            .items_center()
            .w_full()
            .flex_shrink_0()
            .gap_1()
            .p_1()
            .border_b_1()
            .border_color(cx.theme().border)
            .when(settings.sidebar, |view| {
                view.child(
                    Button::new("toolbar-sidebar-toggle")
                        .debug_selector(|| "toolbar-sidebar-toggle".into())
                        .small()
                        .ghost()
                        .icon(if sidebar_visible {
                            IconName::PanelLeftClose
                        } else {
                            IconName::PanelLeftOpen
                        })
                        .tooltip(if sidebar_visible {
                            "Collapse sidebar"
                        } else {
                            "Show sidebar"
                        })
                        .disabled(disabled)
                        .on_click(
                            cx.listener(|this, _, window, cx| this.toggle_sidebar(window, cx)),
                        ),
                )
            })
            .when(settings.explorer, |view| {
                view.child(
                    Button::new("sidebar-explorer")
                        .debug_selector(|| "sidebar-explorer".into())
                        .small()
                        .ghost()
                        .icon(IconName::Folder)
                        .tooltip("Explorer")
                        .selected(sidebar_visible && !searching)
                        .disabled(disabled)
                        .on_click(
                            cx.listener(|this, _, window, cx| this.show_explorer(window, cx)),
                        ),
                )
            })
            .when(settings.search, |view| {
                view.child(
                    Button::new("sidebar-search")
                        .debug_selector(|| "sidebar-search".into())
                        .small()
                        .ghost()
                        .icon(IconName::Search)
                        .tooltip("Search all files (Ctrl+Shift+F)")
                        .selected(sidebar_visible && searching)
                        .disabled(disabled)
                        .on_click(
                            cx.listener(|this, _, window, cx| this.open_search_all(window, cx)),
                        ),
                )
            })
            .when(settings.sort, |view| {
                view.child(
                    Button::new("sidebar-sort")
                        .debug_selector(|| "sidebar-sort".into())
                        .small()
                        .ghost()
                        .icon(IconName::SortAscending)
                        .tooltip("Sort files")
                        .disabled(disabled)
                        .dropdown_menu(move |menu, _, cx| {
                            if let Some(entity) = owner.upgrade() {
                                entity.read(cx).file_sort_menu(menu, owner.clone())
                            } else {
                                menu
                            }
                        }),
                )
            })
    }
}
