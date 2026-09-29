//! In-memory GPUI windows only; terminal launches use the existing test fixture.
use super::Zoom;
use crate::{CommandPalette, Document, Notes, NotesWindow, ViewMenu, ViewMode, notebook};
use gpui::{Entity, TestAppContext, VisualTestContext, prelude::*, px, size};
use gpui_component::{ActiveTheme, Root, WindowExt};
use markdown_note::commands::Command;

fn fixture(cx: &mut TestAppContext) -> (Entity<Notes>, &mut VisualTestContext) {
    cx.update(|cx| {
        gpui_component::init(cx);
        cx.bind_keys(
            crate::help_ui::app_shortcuts()
                .into_iter()
                .map(|s| s.binding),
        );
    });
    let mut notes = None;
    let (_, view) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| Notes::new(window, cx));
        entity.update(cx, |this, cx| {
            this.session_autosave.enabled = false;
            this.apply_appearance(window, cx);
            this.focus.focus(window);
        });
        notes = Some(entity.clone());
        let root = cx.new(|cx| NotesWindow {
            _subscription: cx.observe(&entity, |_, _, cx| cx.notify()),
            notes: entity,
        });
        Root::new(root, window, cx)
    });
    view.simulate_resize(size(px(1200.), px(900.)));
    view.run_until_parked();
    view.refresh().unwrap();
    (notes.unwrap(), view)
}

fn level(notes: &Entity<Notes>, cx: &mut VisualTestContext) -> u16 {
    cx.update(|_, cx| notes.read(cx).appearance().zoom_percent)
}

fn keys(cx: &mut VisualTestContext, key: &str) {
    let key = if cfg!(target_os = "macos") {
        key.replace("ctrl-", "cmd-")
    } else {
        key.into()
    };
    cx.simulate_keystrokes(&key);
    cx.run_until_parked();
}

#[gpui::test]
fn zoom_shortcuts_work_without_a_document_and_in_terminal(cx: &mut TestAppContext) {
    let (notes, cx) = fixture(cx);
    for key in ["ctrl-+", "ctrl-=", "ctrl-shift-+", "ctrl-shift-="] {
        keys(cx, key);
        assert_eq!(level(&notes, cx), 110, "{key}");
        keys(cx, "ctrl-0");
        assert_eq!(level(&notes, cx), 100);
    }
    keys(cx, "ctrl--");
    assert_eq!(level(&notes, cx), 90);
    keys(cx, "ctrl-0");
    let (session, view, input) = cx.update(|window, cx| {
        notes.update(cx, |this, cx| this.toggle_right_panel(window, cx));
        let session = notes.read(cx).terminals.active_session().unwrap().clone();
        let view = session.read(cx).view.clone().unwrap();
        let input = session.read(cx).fixture_input.clone();
        (session, view, input)
    });
    cx.run_until_parked();
    let initial_view = view.clone();
    for (key, expected) in [
        ("ctrl-=", 110),
        ("ctrl-+", 120),
        ("ctrl-shift-+", 130),
        ("ctrl-shift-=", 140),
        ("ctrl--", 130),
        ("ctrl-0", 100),
    ] {
        input.lock().unwrap().clear();
        keys(cx, key);
        assert_eq!(level(&notes, cx), expected, "terminal {key}");
        assert!(
            input.lock().unwrap().is_empty(),
            "Zoom must not reach the shell"
        );
        cx.update(|window, cx| {
            assert!(view.read(cx).focus_handle().is_focused(window));
            assert_eq!(session.read(cx).view.as_ref(), Some(&initial_view));
            assert_eq!(
                view.read(cx).config().font_size,
                px(16. * f32::from(expected) / 100.)
            );
        });
    }
}

#[gpui::test]
fn zoom_menu_palette_and_guards_share_the_actions(cx: &mut TestAppContext) {
    let (notes, cx) = fixture(cx);
    for (selection, expected) in [
        ("enter", 110),
        ("down enter", 100),
        ("down down enter", 100),
    ] {
        cx.dispatch_action(ViewMenu);
        // Zoom is last in View. Right opens its submenu at Zoom in.
        keys(cx, "up right");
        keys(cx, selection);
        assert_eq!(level(&notes, cx), expected);
        cx.update(|_, cx| assert!(notes.read(cx).menu.is_none()));
    }
    for (query, expected) in [
        ("View: Zoom in", 110),
        ("View: Zoom out", 100),
        ("View: Reset zoom", 100),
    ] {
        cx.dispatch_action(CommandPalette);
        cx.run_until_parked();
        cx.simulate_input(query);
        cx.run_until_parked();
        keys(cx, "enter");
        assert_eq!(level(&notes, cx), expected);
        cx.update(|_, cx| assert!(notes.read(cx).palette.is_none()));
    }
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            window.open_dialog(cx, |dialog, _, _| dialog.title("Modal guard"));
            this.change_zoom(Zoom::In, window, cx);
            this.run_command(Command::ZoomOut, window, cx);
            assert_eq!(this.appearance().zoom_percent, 100);
            window.close_dialog(cx);
            let _ = this.session.pick();
            this.change_zoom(Zoom::In, window, cx);
            this.run_command(Command::ZoomOut, window, cx);
            assert_eq!(this.appearance().zoom_percent, 100);
        });
    });
}

#[gpui::test]
fn zoom_notebook_persistence_and_standalone_scale_are_independent(cx: &mut TestAppContext) {
    let (notes, cx) = fixture(cx);
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.notebooks
                .set_test_persistence_target(first.path().join("index.json"));
            this.change_zoom(Zoom::Out, window, cx);
            this.notebooks.active = Some(notebook::Notebook::open(first.path(), false).unwrap());
            this.apply_appearance(window, cx);
            assert_eq!(window.rem_size(), px(16.));
            this.change_zoom(Zoom::In, window, cx);
        });
    });
    cx.run_until_parked();
    assert_eq!(
        notebook::Notebook::open(first.path(), false)
            .unwrap()
            .settings
            .appearance
            .zoom_percent,
        110
    );
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.notebooks.active = Some(notebook::Notebook::open(second.path(), false).unwrap());
            this.apply_appearance(window, cx);
            assert_eq!(this.appearance().zoom_percent, 100);
            this.change_zoom(Zoom::Out, window, cx);
        });
    });
    cx.run_until_parked();
    assert_eq!(
        notebook::Notebook::open(second.path(), false)
            .unwrap()
            .settings
            .appearance
            .zoom_percent,
        90
    );
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.notebooks.active = Some(notebook::Notebook::open(first.path(), false).unwrap());
            this.apply_appearance(window, cx);
            assert_eq!(window.rem_size(), px(16. * 1.1));
            this.change_zoom(Zoom::Reset, window, cx);
            this.notebooks.active = None;
            this.apply_appearance(window, cx);
            assert_eq!(this.appearance().zoom_percent, 90);
            assert_eq!(window.rem_size(), px(16. * 0.9));
        });
    });
    cx.run_until_parked();
    assert_eq!(
        notebook::Notebook::open(first.path(), false)
            .unwrap()
            .settings
            .appearance
            .zoom_percent,
        100
    );
}

#[gpui::test]
fn zoom_scales_retained_document_modes_and_toolbar_without_changing_text(cx: &mut TestAppContext) {
    let (notes, cx) = fixture(cx);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("zoom.md");
    std::fs::write(&path, "# Heading\n\nZoom preserves **text**.").unwrap();
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.install_document(Some(Document::open(path).unwrap()), window, cx)
        });
    });
    cx.run_until_parked();
    let baseline = cx.debug_bounds("toolbar-back").unwrap().size;
    let (editor, text, selection) = cx.update(|_, cx| {
        let this = notes.read(cx);
        (
            this.editor.id(),
            this.editor.text(cx),
            this.editor.selection(cx),
        )
    });
    for mode in [ViewMode::Source, ViewMode::LivePreview, ViewMode::Reading] {
        cx.update(|window, cx| {
            notes.update(cx, |this, cx| {
                this.change_view_mode(mode, window, cx);
                let mut appearance = this.appearance();
                appearance.zoom_percent = 150;
                this.update_appearance(appearance, window, cx);
            });
        });
        cx.run_until_parked();
        cx.refresh().unwrap();
        let zoomed = cx.debug_bounds("toolbar-back").unwrap().size;
        assert_eq!(zoomed.width, baseline.width * 1.5);
        assert_eq!(zoomed.height, baseline.height * 1.5);
        cx.update(|window, cx| {
            notes.update(cx, |this, cx| {
                assert_eq!(this.editor.id(), editor);
                assert_eq!(this.editor.text(cx), text);
                assert_eq!(this.editor.selection(cx), selection);
                assert_eq!(cx.theme().font_size, px(24.));
                assert_eq!(this.appearance().content_font_size, 16);
                this.change_zoom(Zoom::Reset, window, cx);
            });
        });
        cx.run_until_parked();
        assert_eq!(cx.debug_bounds("toolbar-back").unwrap().size, baseline);
    }
}

#[gpui::test]
fn zoom_panel_projection_and_resize_keep_preferences_unscaled(cx: &mut TestAppContext) {
    let (notes, cx) = fixture(cx);
    let dir = tempfile::tempdir().unwrap();
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.notebooks
                .set_test_persistence_target(dir.path().join("index.json"));
            this.notebooks.active = Some(notebook::Notebook::open(dir.path(), false).unwrap());
            this.toggle_right_panel(window, cx);
            let mut appearance = this.appearance();
            appearance.zoom_percent = 150;
            this.update_appearance(appearance, window, cx);
        });
    });
    cx.simulate_resize(size(px(1800.), px(900.)));
    cx.run_until_parked();
    cx.refresh().unwrap();
    assert_eq!(
        cx.debug_bounds("workspace-sidebar").unwrap().size.width,
        px(420.)
    );
    let panel = cx.debug_bounds("right-panel").unwrap();
    assert_eq!(panel.size.width, px(420.));
    // These events are delivered only to TestPlatform's in-memory window.
    let start = gpui::point(panel.left(), panel.center().y);
    cx.simulate_mouse_move(start, None, gpui::Modifiers::default());
    cx.simulate_mouse_down(start, gpui::MouseButton::Left, gpui::Modifiers::default());
    let end = gpui::point(start.x - px(90.), start.y);
    cx.simulate_mouse_move(
        gpui::point(start.x - px(12.), start.y),
        gpui::MouseButton::Left,
        gpui::Modifiers::default(),
    );
    cx.run_until_parked();
    cx.simulate_mouse_move(end, gpui::MouseButton::Left, gpui::Modifiers::default());
    cx.run_until_parked();
    cx.simulate_mouse_up(end, gpui::MouseButton::Left, gpui::Modifiers::default());
    cx.run_until_parked();
    let displayed = cx.debug_bounds("right-panel").unwrap().size.width;
    let preferred = cx.update(|_, cx| {
        let this = notes.read(cx);
        assert_eq!(
            this.notebooks
                .active
                .as_ref()
                .unwrap()
                .settings
                .sidebar_width,
            280.
        );
        this.terminal_width()
    });
    assert!(preferred > 280.);
    assert!((f32::from(displayed) / 1.5 - preferred).abs() < 1.);
    for (percent, width) in [(200, 500.), (50, 500.), (100, 1800.)] {
        cx.update(|window, cx| {
            notes.update(cx, |this, cx| {
                let mut appearance = this.appearance();
                appearance.zoom_percent = percent;
                this.update_appearance(appearance, window, cx);
            });
        });
        cx.simulate_resize(size(px(width), px(900.)));
        cx.run_until_parked();
        cx.refresh().unwrap();
        let panel = cx.debug_bounds("right-panel").unwrap();
        assert!(panel.right() <= px(width + 1.));
        cx.update(|_, cx| assert_eq!(notes.read(cx).terminal_width(), preferred));
    }
    assert!((f32::from(cx.debug_bounds("right-panel").unwrap().size.width) - preferred).abs() < 1.);
}
