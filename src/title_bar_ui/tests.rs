//! TestPlatform only: these tests never create or control a native window.
use super::*;
use crate::{Document, FileMenu, HelpMenu, NotesWindow};
use gpui::{Entity, Modifiers, TestAppContext, VisualTestContext, size};
use gpui_component::Root;
use std::fs;

fn fixture(cx: &mut TestAppContext) -> (tempfile::TempDir, Entity<Notes>, &mut VisualTestContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("note.md");
    fs::write(&path, "title bar test").unwrap();
    cx.update(gpui_component::init);
    let mut notes = None;
    let (_, view) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| Notes::new(window, cx));
        entity.update(cx, |this, cx| {
            this.session_autosave.enabled = false;
            this.install_document(Some(Document::open(path).unwrap()), window, cx);
        });
        notes = Some(entity.clone());
        let root = cx.new(|cx| NotesWindow {
            _subscription: cx.observe(&entity, |_, _, cx| cx.notify()),
            notes: entity,
        });
        Root::new(root, window, cx)
    });
    view.simulate_resize(size(px(1000.), px(700.)));
    view.run_until_parked();
    view.refresh().unwrap();
    (dir, notes.unwrap(), view)
}

fn click(kind: MenuKind, cx: &mut VisualTestContext) {
    let bounds = cx.debug_bounds(kind.selector()).unwrap();
    // Movement inside a menu trigger must not enter a native drag loop.
    // TestPlatform deliberately does not implement start_window_move.
    cx.simulate_mouse_down(bounds.center(), MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(
        bounds.center() + gpui::point(px(1.), px(0.)),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    cx.simulate_mouse_up(bounds.center(), MouseButton::Left, Modifiers::default());
    cx.run_until_parked();
    cx.refresh().unwrap();
}

#[cfg(target_os = "windows")]
#[gpui::test]
fn windows_caption_regions_dispatch_client_input(cx: &mut TestAppContext) {
    use windows::CaptionCommand;
    let (_dir, notes, cx) = fixture(cx);
    for (selector, expected) in [
        ("title-bar-drag", CaptionCommand::Move),
        ("window-minimize", CaptionCommand::Minimize),
        ("window-maximize", CaptionCommand::Maximize),
    ] {
        let bounds = cx.debug_bounds(selector).unwrap();
        let bar = cx.debug_bounds("window-title-bar").unwrap();
        assert!(bounds.size.width > px(0.) && bounds.size.height > px(0.));
        assert!(bounds.top() >= bar.top() && bounds.bottom() <= bar.bottom());
        cx.simulate_click(bounds.center(), Modifiers::default());
        cx.update(|_, cx| {
            notes.update(cx, |this, _| {
                assert_eq!(std::mem::take(&mut this.title_bar.commands), vec![expected]);
            })
        });
    }
    let drag = cx.debug_bounds("title-bar-drag").unwrap();
    cx.simulate_event(gpui::MouseDownEvent {
        position: drag.center(),
        button: MouseButton::Left,
        click_count: 2,
        ..Default::default()
    });
    cx.simulate_mouse_up(drag.center(), MouseButton::Left, Modifiers::default());
    cx.update(|_, cx| {
        notes.update(cx, |this, _| {
            assert_eq!(
                std::mem::take(&mut this.title_bar.commands),
                vec![CaptionCommand::Maximize]
            );
        })
    });
    assert_eq!(CaptionCommand::zoom(true), CaptionCommand::Restore);

    // A press released outside the button must not execute a window command.
    let minimize = cx.debug_bounds("window-minimize").unwrap();
    cx.simulate_mouse_down(minimize.center(), MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_up(drag.center(), MouseButton::Left, Modifiers::default());
    click(MenuKind::File, cx);
    cx.simulate_keystrokes("escape");
    cx.update(|_, cx| assert!(notes.read(cx).title_bar.commands.is_empty()));
}

#[cfg(target_os = "windows")]
#[gpui::test]
fn windows_close_button_prompts_and_cancel_preserves_document(cx: &mut TestAppContext) {
    let (_dir, notes, cx) = fixture(cx);
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.editor.replace_selection("changed", window, cx);
        })
    });
    cx.run_until_parked();
    cx.refresh().unwrap();
    let close = cx.debug_bounds("window-close").unwrap();
    cx.simulate_click(close.center(), Modifiers::default());
    cx.run_until_parked();
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    assert_eq!(cx.windows().len(), 1);
    cx.update(|_, cx| assert!(notes.read(cx).dirty(cx)));
}

#[gpui::test]
fn all_menus_anchor_to_buttons_and_escape_restores_editor_focus(cx: &mut TestAppContext) {
    let (_dir, notes, cx) = fixture(cx);
    cx.update(|window, cx| notes.update(cx, |this, cx| this.editor.focus(window, cx)));
    for kind in MENUS {
        click(kind, cx);
        let button = cx.debug_bounds(kind.selector()).unwrap();
        let popup = cx.debug_bounds("title-menu-popup").unwrap();
        assert!((popup.left() - button.left()).abs() <= px(1.));
        assert!((popup.top() - button.bottom()).abs() <= px(1.));
        cx.update(|_, cx| assert_eq!(notes.read(cx).menu.as_ref().unwrap().0, kind));
        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
        cx.update(|window, cx| {
            assert!(notes.read(cx).menu.is_none());
            assert!(notes.read(cx).editor.is_focused(window, cx));
        });
    }
}

#[gpui::test]
fn narrow_header_overflow_keyboard_commands_and_resize_dismissal(cx: &mut TestAppContext) {
    let (_dir, notes, cx) = fixture(cx);
    cx.simulate_resize(size(px(400.), px(700.)));
    cx.refresh().unwrap();
    let visible = cx.update(|_, cx| notes.read(cx).title_bar.layout.as_ref().unwrap().visible);
    assert!((1..6).contains(&visible));
    let more = cx.debug_bounds("more-menu").unwrap();
    let bar = cx.debug_bounds("window-title-bar").unwrap();
    assert!(more.right() <= bar.right());
    if let Some(controls) = cx.debug_bounds("window-minimize") {
        assert!(more.right() + px(32.) <= controls.left());
    }
    cx.dispatch_action(HelpMenu);
    cx.run_until_parked();
    cx.refresh().unwrap();
    assert_eq!(
        cx.update(|_, cx| notes.read(cx).menu.as_ref().unwrap().0),
        MenuKind::Help
    );
    let popup = cx.debug_bounds("title-menu-popup").unwrap();
    // The popup can shift left to fit; its top still follows the More button.
    assert!((popup.top() - more.bottom()).abs() <= px(1.));
    assert!(popup.left() >= px(4.) && popup.right() <= px(396.));
    cx.simulate_resize(size(px(1000.), px(700.)));
    cx.refresh().unwrap();
    cx.update(|_, cx| assert!(notes.read(cx).menu.is_none()));
    assert!(cx.debug_bounds("help-menu").is_some());
    cx.update(|_, cx| assert_eq!(notes.read(cx).title_bar.layout.as_ref().unwrap().visible, 6));
}

#[gpui::test]
fn more_submenus_share_commands_and_dismiss_the_parent(cx: &mut TestAppContext) {
    let (_dir, notes, cx) = fixture(cx);
    cx.simulate_resize(size(px(400.), px(700.)));
    cx.refresh().unwrap();
    click(MenuKind::More, cx);
    // Help is the last overflow entry; enter its submenu and invoke About.
    let hidden = cx.update(|_, cx| 6 - notes.read(cx).title_bar.layout.as_ref().unwrap().visible);
    let keys = std::iter::repeat_n("down", hidden)
        .collect::<Vec<_>>()
        .join(" ");
    cx.simulate_keystrokes(&keys);
    cx.refresh().unwrap();
    cx.simulate_keystrokes("left down enter");
    cx.run_until_parked();
    cx.update(|window, cx| {
        assert!(window.has_active_dialog(cx));
        assert!(notes.read(cx).menu.is_none());
    });
}

#[gpui::test]
fn header_respects_modal_guard_and_large_interface_text(cx: &mut TestAppContext) {
    let (_dir, notes, cx) = fixture(cx);
    for width in [400., 700., 1000.] {
        for font_size in [10., 16., 24.] {
            cx.update(|window, cx| {
                gpui_component::Theme::global_mut(cx).font_size = px(font_size);
                window.set_rem_size(px(font_size));
            });
            cx.simulate_resize(size(px(width), px(700.)));
            cx.refresh().unwrap();
            let bar = cx.debug_bounds("window-title-bar").unwrap();
            let visible =
                cx.update(|_, cx| notes.read(cx).title_bar.layout.as_ref().unwrap().visible);
            assert!(bar.size.height >= px(34.).max(px(font_size * 1.5 + 8.)));
            for kind in MENUS[..visible]
                .iter()
                .copied()
                .chain((visible < 6).then_some(MenuKind::More))
            {
                if let Some(button) = cx.debug_bounds(kind.selector()) {
                    assert!(button.top() >= bar.top() && button.bottom() <= bar.bottom());
                    assert!(button.left() >= bar.left() && button.right() <= bar.right());
                    if let Some(controls) = cx.debug_bounds("window-minimize") {
                        assert!(
                            button.right() + px(32.) <= controls.left(),
                            "width {width} font {font_size} kind {kind:?}: button {button:?} controls {controls:?}"
                        );
                    }
                }
            }
        }
    }
    cx.update(|window, cx| window.open_dialog(cx, |dialog, _, _| dialog.title("Modal")));
    cx.dispatch_action(FileMenu);
    cx.run_until_parked();
    cx.update(|_, cx| assert!(notes.read(cx).menu.is_none()));
}

#[test]
fn platform_layouts_reserve_controls_and_preserve_menu_order() {
    let widths = [px(60.); 7];
    for mode in [
        HeaderMode::Windows,
        HeaderMode::Mac,
        HeaderMode::Linux,
        HeaderMode::Server,
        HeaderMode::Fullscreen,
    ] {
        let narrow = HeaderLayout::new(mode, size(px(400.), px(700.)), px(400.), px(16.), widths);
        assert!(narrow.visible >= 1);
        assert_eq!(narrow.anchor_index(MenuKind::File), 0);
        for kind in MENUS.iter().skip(narrow.visible) {
            assert_eq!(narrow.anchor_index(*kind), 6);
        }
        let wide = HeaderLayout::new(mode, size(px(1000.), px(700.)), px(1000.), px(16.), widths);
        assert_eq!(wide.visible, 6);
    }
    let fullscreen = HeaderLayout::new(
        HeaderMode::Fullscreen,
        size(px(400.), px(700.)),
        px(400.),
        px(16.),
        widths,
    );
    assert_eq!(fullscreen.visible, 6);
    assert!(!fullscreen.mode.combined());
    assert!(!HeaderMode::Server.controls());
    assert!(!HeaderMode::Mac.controls());
}

#[gpui::test]
fn custom_close_request_uses_unsaved_cancel_and_discard_barrier(cx: &mut TestAppContext) {
    let (dir, notes, cx) = fixture(cx);
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.editor.replace_selection("changed", window, cx);
        })
    });
    cx.run_until_parked();
    cx.update(|window, cx| notes.update(cx, |this, cx| this.request_window_close(window, cx)));
    cx.run_until_parked();
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    assert_eq!(cx.windows().len(), 1);
    cx.update(|_, cx| assert!(notes.read(cx).dirty(cx)));
    cx.update(|window, cx| notes.update(cx, |this, cx| this.request_window_close(window, cx)));
    cx.run_until_parked();
    cx.simulate_prompt_answer("Discard");
    cx.run_until_parked();
    assert!(cx.windows().is_empty());
    assert_eq!(
        fs::read_to_string(dir.path().join("note.md")).unwrap(),
        "title bar test"
    );
}

#[gpui::test]
fn fullscreen_keeps_menus_and_restores_windowed_layout(cx: &mut TestAppContext) {
    let (_dir, notes, cx) = fixture(cx);
    cx.simulate_resize(size(px(400.), px(700.)));
    cx.refresh().unwrap();
    let windowed = cx.update(|_, cx| notes.read(cx).title_bar.layout.clone().unwrap());
    click(MenuKind::File, cx);
    cx.update(|window, cx| {
        window.toggle_fullscreen();
        notes.update(cx, |_, cx| cx.notify());
    });
    cx.refresh().unwrap();
    cx.update(|_, cx| {
        let this = notes.read(cx);
        let layout = this.title_bar.layout.as_ref().unwrap();
        assert_eq!(layout.mode, HeaderMode::Fullscreen);
        assert!(layout.visible >= windowed.visible);
        assert!(this.menu.is_none());
    });
    // A menu click with pointer motion stays an ordinary client interaction.
    let button = cx.debug_bounds("view-menu").unwrap();
    cx.simulate_mouse_down(button.center(), MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(
        button.center() + gpui::point(px(1.), px(0.)),
        Some(MouseButton::Left),
        Modifiers::default(),
    );
    cx.simulate_mouse_up(button.center(), MouseButton::Left, Modifiers::default());
    cx.run_until_parked();
    cx.simulate_keystrokes("down down down enter");
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(notes.read(cx).mode, ViewMode::Reading));
    cx.update(|window, cx| {
        window.toggle_fullscreen();
        notes.update(cx, |_, cx| cx.notify());
    });
    cx.refresh().unwrap();
    cx.update(|_, cx| assert_eq!(notes.read(cx).title_bar.layout.as_ref().unwrap(), &windowed));
}
