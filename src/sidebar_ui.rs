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
    use crate::{Document, Notes, NotesWindow, ViewMode, notebook};
    use gpui::{TestAppContext, prelude::*, px, size};
    use gpui_component::Root;

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

    #[gpui::test]
    fn terminal_divider_resizes_beside_document_modes(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("resize.md");
        let text = "# Terminal resizing\n\nA paragraph with **bold text**, *emphasis*, and a [link](https://example.com) that wraps as the terminal gets wider.\n\n## A list\n\n- First item with enough text to wrap while resizing the content panel.\n- Second item\n  - Nested item\n\n> A quoted paragraph that also needs to be laid out repeatedly.\n\n| Name | Value |\n| --- | --- |\n| terminal | retained |\n\n```rust\nfn main() {\n    println!(\"terminal width changes\");\n}\n```\n".repeat(4);
        std::fs::write(&path, &text).unwrap();
        let mut notes = None;
        let (_, cx) = cx.add_window_view(|window, cx| {
            let entity = cx.new(|cx| Notes::new(window, cx));
            entity.update(cx, |this, cx| {
                this.session_autosave.enabled = false;
                this.notebooks
                    .set_test_persistence_target(directory.path().join("index.json"));
                this.apply_appearance(window, cx);
            });
            notes = Some(entity.clone());
            let root = cx.new(|cx| NotesWindow {
                _subscription: cx.observe(&entity, |_, _, cx| cx.notify()),
                notes: entity,
            });
            Root::new(root, window, cx)
        });
        let notes = notes.unwrap();
        cx.update(|window, cx| {
            notes.update(cx, |this, cx| {
                this.install_document(Some(Document::open(path).unwrap()), window, cx);
                this.toggle_right_panel(window, cx);
            });
        });
        cx.simulate_resize(size(px(1400.), px(900.)));
        cx.run_until_parked();
        cx.refresh().unwrap();
        let (editor, terminal) = cx.update(|_, cx| {
            let this = notes.read(cx);
            (
                this.editor.id(),
                this.terminals
                    .active_session()
                    .unwrap()
                    .read(cx)
                    .view
                    .clone()
                    .unwrap(),
            )
        });
        for notebook_open in [false, true] {
            if notebook_open {
                cx.update(|window, cx| {
                    notes.update(cx, |this, cx| {
                        this.notebooks.active =
                            Some(notebook::Notebook::open(directory.path(), false).unwrap());
                        this.apply_appearance(window, cx);
                        cx.notify();
                    });
                });
            }
            for mode in [ViewMode::Source, ViewMode::LivePreview, ViewMode::Reading] {
                cx.update(|window, cx| {
                    notes.update(cx, |this, cx| {
                        this.change_view_mode(mode, window, cx);
                        if mode != ViewMode::Reading {
                            this.editor.select(text.len()..text.len(), window, cx);
                        }
                    });
                });
                cx.run_until_parked();
                cx.refresh().unwrap();
                if mode == ViewMode::Reading {
                    let position = cx.debug_bounds("workspace-content").unwrap().center();
                    cx.simulate_event(gpui::ScrollWheelEvent {
                        position,
                        delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(-1200.))),
                        ..Default::default()
                    });
                    cx.run_until_parked();
                    cx.refresh().unwrap();
                }
                for delta in [-80., 120., -100., 60.] {
                    let panel = cx.debug_bounds("right-panel").unwrap();
                    let start = gpui::point(panel.left(), panel.center().y);
                    let before = cx.update(|_, cx| terminal.read(cx).dimensions());
                    cx.simulate_mouse_move(start, None, gpui::Modifiers::default());
                    cx.simulate_mouse_down(
                        start,
                        gpui::MouseButton::Left,
                        gpui::Modifiers::default(),
                    );
                    // Exercise successive rendered drag frames, not only the release.
                    for step in 1..=8 {
                        let position = gpui::point(start.x + px(delta * step as f32 / 8.), start.y);
                        cx.simulate_mouse_move(
                            position,
                            gpui::MouseButton::Left,
                            gpui::Modifiers::default(),
                        );
                        cx.run_until_parked();
                        cx.refresh().unwrap();
                    }
                    cx.simulate_mouse_up(
                        gpui::point(start.x + px(delta), start.y),
                        gpui::MouseButton::Left,
                        gpui::Modifiers::default(),
                    );
                    cx.run_until_parked();
                    cx.refresh().unwrap();
                    let displayed = cx.debug_bounds("right-panel").unwrap().size.width;
                    cx.update(|_, cx| {
                        let this = notes.read(cx);
                        assert_eq!(this.editor.id(), editor);
                        assert_eq!(this.editor.text(cx), text);
                        assert_eq!(
                            this.terminals.active_session().unwrap().read(cx).view.as_ref(),
                            Some(&terminal)
                        );
                        assert!((f32::from(displayed) - this.terminal_width()).abs() < 1.);
                        let after = terminal.read(cx).dimensions();
                        assert_eq!(before.1, after.1);
                        assert!(
                            if delta < 0. { after.0 > before.0 } else { after.0 < before.0 },
                            "{mode:?}, notebook={notebook_open}, delta={delta}: {before:?} -> {after:?}"
                        );
                    });
                }
            }
        }
    }

    #[gpui::test]
    fn narrow_zoomed_workspace_keeps_terminal_minimum_columns(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let directory = tempfile::tempdir().unwrap();
        let mut notes = None;
        let (_, cx) = cx.add_window_view(|window, cx| {
            let entity = cx.new(|cx| Notes::new(window, cx));
            notes = Some(entity.clone());
            let root = cx.new(|cx| NotesWindow {
                _subscription: cx.observe(&entity, |_, _, cx| cx.notify()),
                notes: entity,
            });
            Root::new(root, window, cx)
        });
        let notes = notes.unwrap();
        cx.update(|window, cx| {
            notes.update(cx, |this, cx| {
                this.notebooks
                    .set_test_persistence_target(directory.path().join("index.json"));
                let mut book = notebook::Notebook::open(directory.path(), false).unwrap();
                book.settings.terminal.font_size = Some(40);
                book.settings.appearance.zoom_percent = 200;
                book.settings.terminal_width = 1000.;
                this.notebooks.active = Some(book);
                this.apply_appearance(window, cx);
                this.toggle_right_panel(window, cx);
            });
        });
        cx.simulate_resize(size(px(800.), px(700.)));
        cx.run_until_parked();
        cx.refresh().unwrap();
        let terminal = cx.update(|_, cx| {
            let this = notes.read(cx);
            this.terminals
                .active_session()
                .unwrap()
                .read(cx)
                .view
                .clone()
                .unwrap()
        });
        cx.update(|_, cx| assert!(terminal.read(cx).dimensions().0 > 2));
        let panel = cx.debug_bounds("right-panel").unwrap();
        let start = gpui::point(panel.left(), panel.center().y);
        cx.simulate_mouse_move(start, None, gpui::Modifiers::default());
        cx.simulate_mouse_down(start, gpui::MouseButton::Left, gpui::Modifiers::default());
        // Unequal saved panel widths project to an 87.5px sidebar and a 312.5px
        // terminal. Dragging the divider can shrink the terminal below two cells.
        let mut reached_minimum_grid = false;
        for step in 1..=8 {
            let position = gpui::point(start.x + px(225. * step as f32 / 8.), start.y);
            cx.simulate_mouse_move(
                position,
                gpui::MouseButton::Left,
                gpui::Modifiers::default(),
            );
            cx.run_until_parked();
            cx.refresh().unwrap();
            let panel_width = cx.debug_bounds("right-panel").unwrap().size.width;
            reached_minimum_grid |= cx.update(|_, cx| {
                let terminal = terminal.read(cx);
                let columns = terminal.dimensions().0;
                assert!(columns >= 2, "drag frame {step}: only {columns} column(s)");
                let geometry = terminal.grid_geometry().unwrap();
                assert_eq!(geometry.columns, columns);
                let padding = terminal.config().padding;
                let available_width = panel_width - padding.left - padding.right;
                if available_width < geometry.cell_width * 2. {
                    assert_eq!(columns, 2);
                    true
                } else {
                    false
                }
            });
        }
        assert!(
            reached_minimum_grid,
            "drag must project fewer than two cells"
        );
        cx.simulate_mouse_up(
            gpui::point(start.x + px(225.), start.y),
            gpui::MouseButton::Left,
            gpui::Modifiers::default(),
        );
        cx.run_until_parked();
        cx.refresh().unwrap();
        // Persisting the completed drag normalizes the saved width and may widen
        // the panel again; the terminal must remain valid after that projection.
        cx.update(|_, cx| assert!(terminal.read(cx).dimensions().0 >= 2));
        // Both panels compress around the editor's minimum width. With a large
        // terminal font, the projected panel can fit fewer than two cells.
        for width in [1000., 600., 500., 440., 500., 1000.] {
            cx.simulate_resize(size(px(width), px(700.)));
            cx.run_until_parked();
            cx.refresh().unwrap();
            let panel_width = cx.debug_bounds("right-panel").unwrap().size.width;
            cx.update(|_, cx| {
                let this = notes.read(cx);
                let session = this.terminals.active_session().unwrap().read(cx);
                let terminal = session.view.as_ref().unwrap().read(cx);
                let columns = terminal.dimensions().0;
                assert!(
                    columns >= 2,
                    "viewport={width}, terminal={panel_width:?}: only {columns} column(s)"
                );
            });
        }
    }
}
