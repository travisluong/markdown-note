use super::*;
use gpui_component::resizable::{ResizableState, h_resizable, resizable_panel};
use gpui_component::{PixelsExt, menu::PopupMenuItem};
use notebook::normalize_terminal_width;

impl Notes {
    pub(super) fn file_sort_menu(&self, mut menu: PopupMenu, owner: WeakEntity<Self>) -> PopupMenu {
        let selected = self
            .notebooks
            .active
            .as_ref()
            .map(|book| book.settings.file_sort);
        let disabled = self.document_interaction_blocked() || selected.is_none();
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
        if self.settings_panel.is_some() {
            self.sidebar_collapsed = !self.sidebar_collapsed;
            self.reset_sidebar_resize(cx);
            self.focus_settings(window, cx);
            cx.notify();
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
            if self.tabs.active.mode == ViewMode::Reading || self.document().is_none() {
                self.focus.focus(window);
            } else {
                self.tabs.active.editor.focus(window, cx);
            }
        }
        cx.notify();
    }

    pub(super) fn show_explorer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings_panel.is_some() {
            return;
        }
        if self.is_busy() || window.has_active_dialog(cx) || self.notebooks.active.is_none() {
            return;
        }
        self.reveal_sidebar(cx);
        self.hide_search(cx);
        self.notebooks.tree_focus.focus(window);
        cx.notify();
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

    pub(super) fn terminal_width(&self) -> f32 {
        self.notebooks
            .active
            .as_ref()
            .map_or(self.right_panel_width, |book| book.settings.terminal_width)
    }

    pub(super) fn save_terminal_width(&mut self, width: f32, cx: &mut Context<Self>) {
        let width = normalize_terminal_width(width);
        self.record_activation_preference(notebook_ui::PreferenceIntent::TerminalWidth(width));
        if let Some(book) = self.notebooks.active.as_mut() {
            if book.settings.terminal_width != width {
                book.settings.terminal_width = width;
                self.persist_notebooks(cx);
            }
        } else {
            self.right_panel_width = width;
        }
    }

    pub(super) fn render_sidebar_layout(
        &mut self,
        content: AnyElement,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let left_visible = self.notebooks.active.is_some() && !self.sidebar_collapsed;
        let right_visible = self.right_panel_open;
        let viewport_width = window.viewport_size().width;
        let layout = (left_visible, right_visible, viewport_width);
        if self.workspace_layout != Some(layout) {
            self.reset_sidebar_resize(cx);
            self.workspace_layout = Some(layout);
        }
        let editor = div()
            .id("workspace-content")
            .debug_selector(|| "workspace-content".into())
            .flex_1()
            .min_w_0()
            .h_full()
            .overflow_hidden()
            .child(content);
        if !left_visible && !right_visible {
            return editor.into_any_element();
        }
        let zoom = self.appearance().zoom_factor();
        let geometry = WorkspaceGeometry::new(
            viewport_width.as_f32() / zoom,
            self.notebooks
                .active
                .as_ref()
                .filter(|_| left_visible)
                .map(|book| book.settings.sidebar_width),
            right_visible.then_some(self.terminal_width()),
        );
        let mut panels = h_resizable("sidebar-layout")
            .with_state(&self.sidebar_resize)
            .adjacent_only(true);
        if let Some(width) = geometry.left {
            let sidebar = if self.search_visible(cx) {
                self.render_search_panel(cx).into_any_element()
            } else {
                self.render_sidebar(window, cx).into_any_element()
            };
            let sidebar = div()
                .id("workspace-sidebar")
                .debug_selector(|| "workspace-sidebar".into())
                .flex()
                .flex_col()
                .size_full()
                .min_w_0()
                .overflow_hidden()
                .when(
                    self.toolbar_region_visible(notebook::ToolbarRegion::Sidebar),
                    |view| view.child(self.render_sidebar_toolbar(window, cx)),
                )
                .child(div().flex_1().min_h_0().overflow_hidden().child(sidebar));
            panels = panels.child(
                resizable_panel()
                    .size(px(width * zoom))
                    .size_range(px(geometry.min_side * zoom)..px(geometry.max_left * zoom))
                    .child(sidebar),
            );
        }
        panels = panels.child(
            resizable_panel()
                .size_range(px(200. * zoom).min(viewport_width)..Pixels::MAX)
                .child(editor),
        );
        if let Some(width) = geometry.right {
            panels = panels.child(
                resizable_panel()
                    .size(px(width * zoom))
                    .size_range(px(geometry.min_side * zoom)..px(geometry.max_right * zoom))
                    .child(self.render_right_panel(window, cx)),
            );
        }
        panels
            .on_resize(
                cx.listener(move |this, state: &Entity<ResizableState>, window, cx| {
                    // Layout changes are projections, not preference changes. Only a completed
                    // user resize from the current arrangement may save widths.
                    if state != &this.sidebar_resize
                        || this.document_interaction_blocked()
                        || window.has_active_dialog(cx)
                    {
                        return;
                    }
                    let sizes = state.read(cx).sizes().clone();
                    let divider = state.read(cx).completed_divider();
                    if left_visible
                        && divider == Some(0)
                        && let Some(width) = sizes.first()
                    {
                        this.save_sidebar_width(width.as_f32() / zoom, cx);
                    }
                    if right_visible
                        && divider == Some(usize::from(left_visible))
                        && let Some(width) = sizes.last()
                    {
                        this.save_terminal_width(width.as_f32() / zoom, cx);
                    }
                }),
            )
            .into_any_element()
    }
}

/// Reserve content space before fitting side panels. These projected widths are
/// never persisted by a viewport resize or a panel visibility change.
struct WorkspaceGeometry {
    left: Option<f32>,
    right: Option<f32>,
    min_side: f32,
    max_left: f32,
    max_right: f32,
}

impl WorkspaceGeometry {
    fn new(width: f32, left: Option<f32>, right: Option<f32>) -> Self {
        let left = left.map(notebook::normalize_sidebar_width);
        let right = right.map(normalize_terminal_width);
        let count = usize::from(left.is_some()) + usize::from(right.is_some());
        let budget = (width - 200.).max(0.);
        let total = left.unwrap_or_default() + right.unwrap_or_default();
        let scale = if total > 0. {
            (budget / total).min(1.)
        } else {
            1.
        };
        // Keep normal minimums whenever the projected panels fit them. A smaller
        // proportional panel lowers the shared minimum only as far as necessary.
        let min_side = [left, right]
            .into_iter()
            .flatten()
            .map(|width| width * scale)
            .fold(160., f32::min);
        let max_side = (budget - count.saturating_sub(1) as f32 * min_side).max(min_side);
        Self {
            left: left.map(|width| width * scale),
            right: right.map(|width| width * scale),
            min_side,
            max_left: max_side.min(600.),
            max_right: max_side,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::WorkspaceGeometry;

    #[test]
    fn side_panels_fit_proportionally_and_reserve_content() {
        for width in [400., 520., 800., 1600.] {
            for left in [None, Some(160.), Some(280.), Some(600.)] {
                for right in [None, Some(160.), Some(280.), Some(600.), Some(1000.)] {
                    let geometry = WorkspaceGeometry::new(width, left, right);
                    let total =
                        geometry.left.unwrap_or_default() + geometry.right.unwrap_or_default();
                    assert!(total <= width - 200. + 0.001);
                    for side in [geometry.left, geometry.right].into_iter().flatten() {
                        assert!(side >= geometry.min_side - 0.001);
                        assert!(side <= geometry.max_right + 0.001);
                    }
                    if let (Some(left), Some(right)) = (left, right) {
                        assert!(
                            (geometry.left.unwrap() / geometry.right.unwrap() - left / right).abs()
                                < 0.001
                        );
                    }
                }
            }
        }
    }
}
