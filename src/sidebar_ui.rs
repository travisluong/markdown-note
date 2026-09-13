use super::*;
use gpui_component::resizable::{ResizableState, h_resizable, resizable_panel};
use gpui_component::{
    Disableable, IconName, PixelsExt, Selectable,
    menu::{DropdownMenu, PopupMenuItem},
};

impl Notes {
    pub(super) fn file_sort_menu(&self, mut menu: PopupMenu, owner: WeakEntity<Self>) -> PopupMenu {
        let selected = self
            .notebooks
            .active
            .as_ref()
            .map(|book| book.settings.file_sort);
        let disabled = self.is_busy() || selected.is_none();
        for (sort, label) in notebook::FileSort::OPTIONS {
            let owner = owner.clone();
            menu = menu.item(
                PopupMenuItem::new(label)
                    .checked(selected == Some(sort))
                    .disabled(disabled)
                    .on_click(move |_, window, cx| {
                        let _ = owner.update(cx, |this, cx| this.set_file_sort(sort, window, cx));
                    }),
            );
        }
        menu
    }

    pub(super) fn reveal_sidebar(&mut self, cx: &mut Context<Self>) {
        if self.sidebar_collapsed {
            self.sidebar_collapsed = false;
            self.reset_sidebar_resize(cx);
        }
    }

    pub(super) fn toggle_sidebar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_busy() || window.has_active_dialog(cx) || self.notebooks.active.is_none() {
            return;
        }
        if self.sidebar_collapsed {
            self.reveal_sidebar(cx);
            if self.search_visible(cx) {
                self.open_search_all(window, cx);
            } else {
                self.show_explorer(window, cx);
            }
        } else {
            self.sidebar_collapsed = true;
            if self.mode == ViewMode::Reading || self.document().is_none() {
                self.focus.focus(window);
            } else {
                self.editor.focus(window, cx);
            }
        }
        cx.notify();
    }

    pub(super) fn show_explorer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_busy() || window.has_active_dialog(cx) || self.notebooks.active.is_none() {
            return;
        }
        self.reveal_sidebar(cx);
        self.hide_search(cx);
        self.notebooks.tree_focus.focus(window);
        cx.notify();
    }

    fn render_sidebar_toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let searching = self.search_visible(cx);
        let owner = cx.weak_entity();
        div()
            .id("sidebar-toolbar")
            .drag_over::<notebook_ui::EntryDrag>(|view, _, _, cx| view.bg(cx.theme().accent))
            .on_drop(
                cx.listener(|this, drag: &notebook_ui::EntryDrag, window, cx| {
                    cx.stop_propagation();
                    this.drop_entry(drag, PathBuf::new(), window, cx);
                }),
            )
            .flex()
            .items_center()
            .flex_shrink_0()
            .gap_1()
            .p_1()
            .child(
                Button::new("sidebar-explorer")
                    .small()
                    .ghost()
                    .icon(IconName::Folder)
                    .tooltip("Explorer")
                    .selected(!searching)
                    .on_click(cx.listener(|this, _, window, cx| this.show_explorer(window, cx))),
            )
            .child(
                Button::new("sidebar-search")
                    .small()
                    .ghost()
                    .icon(IconName::Search)
                    .tooltip("Search all files (Ctrl+Shift+F)")
                    .selected(searching)
                    .on_click(cx.listener(|this, _, window, cx| this.open_search_all(window, cx))),
            )
            .child(
                Button::new("sidebar-sort")
                    .small()
                    .ghost()
                    .icon(IconName::SortAscending)
                    .tooltip("Sort files")
                    .disabled(self.is_busy())
                    .dropdown_menu(move |menu, _, cx| {
                        if let Some(entity) = owner.upgrade() {
                            entity.read(cx).file_sort_menu(menu, owner.clone())
                        } else {
                            menu
                        }
                    }),
            )
    }
    pub(super) fn reset_sidebar_resize(&mut self, cx: &mut Context<Self>) {
        self.sidebar_resize = cx.new(|_| ResizableState::default());
    }

    pub(super) fn save_sidebar_width(&mut self, width: f32, cx: &mut Context<Self>) {
        let width = notebook::normalize_sidebar_width(width);
        self.record_activation_preference(notebook_ui::PreferenceIntent::SidebarWidth(width));
        let Some(book) = self.notebooks.active.as_mut() else {
            return;
        };
        if book.settings.sidebar_width != width {
            book.settings.sidebar_width = width;
            self.persist_notebooks(cx);
        }
    }

    pub(super) fn render_sidebar_layout(
        &mut self,
        content: AnyElement,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let editor = div()
            .flex_1()
            .min_w_0()
            .h_full()
            .overflow_hidden()
            .child(content);
        if self.sidebar_collapsed {
            return editor.into_any_element();
        }
        let Some(book) = self.notebooks.active.as_ref() else {
            return editor.into_any_element();
        };
        let max_width = (window.viewport_size().width.as_f32() - 200.).clamp(160., 600.);
        let width = notebook::normalize_sidebar_width(book.settings.sidebar_width).min(max_width);
        let sidebar = if self.search_visible(cx) {
            self.render_search_panel(cx).into_any_element()
        } else {
            self.render_sidebar(window, cx).into_any_element()
        };
        let sidebar = div()
            .flex()
            .flex_col()
            .size_full()
            .min_w_0()
            .overflow_hidden()
            // The resize handle supplies the single-pixel divider.
            .child(self.render_sidebar_toolbar(cx))
            .child(div().flex_1().min_h_0().overflow_hidden().child(sidebar));
        h_resizable("sidebar-layout")
            .with_state(&self.sidebar_resize)
            .child(
                resizable_panel()
                    .size(px(width))
                    .size_range(px(160.)..px(max_width))
                    .child(sidebar),
            )
            .child(
                resizable_panel()
                    .size_range(px(200.)..Pixels::MAX)
                    .child(editor),
            )
            .on_resize(cx.listener(|this, state: &Entity<ResizableState>, _, cx| {
                if let Some(width) = state.read(cx).sizes().first().copied() {
                    this.save_sidebar_width(width.as_f32(), cx);
                }
            }))
            .into_any_element()
    }
}
