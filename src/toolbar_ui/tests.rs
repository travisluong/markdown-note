//! Real toolbar/menu interactions on GPUI TestPlatform only.
use super::*;
use crate::{NotesWindow, ViewMode, notebook};
use gpui::{AssetSource, Entity, Modifiers, TestAppContext, VisualTestContext};
use gpui_component::{
    Root,
    input::{Redo, Undo},
};

fn fixture(cx: &mut TestAppContext) -> (tempfile::TempDir, Entity<Notes>, &mut VisualTestContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("standalone.md");
    std::fs::write(&path, "café\n茶").unwrap();
    cx.update(gpui_component::init);
    let mut notes = None;
    let (_, view) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| Notes::new(window, cx));
        entity.update(cx, |this, cx| this.open_path(path, false, window, cx));
        notes = Some(entity.clone());
        let root = cx.new(|cx| NotesWindow {
            _subscription: cx.observe(&entity, |_, _, cx| cx.notify()),
            notes: entity,
        });
        Root::new(root, window, cx)
    });
    view.run_until_parked();
    let notes = notes.unwrap();
    view.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.update_autosave(
                notebook::Autosave {
                    enabled: false,
                    delay_ms: 1_000,
                },
                window,
                cx,
            );
            this.mode = ViewMode::Source;
            this.editor.set_mode(ViewMode::Source, cx);
            this.editor.select(0..5, window, cx);
            cx.notify();
        })
    });
    view.refresh().unwrap();
    (dir, notes, view)
}

fn click(selector: &'static str, cx: &mut VisualTestContext) {
    let bounds = cx.debug_bounds(selector).unwrap();
    cx.simulate_click(bounds.center(), Modifiers::default());
    cx.run_until_parked();
    cx.refresh().unwrap();
}

#[gpui::test]
fn toolbar_clicks_preserve_selection_focus_dirty_tracking_and_history(cx: &mut TestAppContext) {
    let (_dir, notes, cx) = fixture(cx);
    for mode in [ViewMode::Source, ViewMode::LivePreview] {
        cx.update(|window, cx| {
            notes.update(cx, |this, cx| {
                this.mode = mode;
                this.editor.set_mode(mode, cx);
                this.editor.select(0..5, window, cx);
                cx.notify();
            })
        });
        cx.refresh().unwrap();
        click("toolbar-bold", cx);
        cx.update(|window, cx| {
            let this = notes.read(cx);
            assert!(this.notebooks.active.is_none());
            assert_eq!(this.editor.text(cx).as_ref(), "**café**\n茶");
            assert_eq!(this.editor.selection(cx), 2..7);
            assert!(this.editor.is_focused(window, cx));
            assert!(this.dirty(cx));
        });
        cx.dispatch_action(Undo);
        cx.run_until_parked();
        cx.update(|_, cx| {
            assert!(!notes.read(cx).dirty(cx));
        });
        cx.dispatch_action(Redo);
        cx.run_until_parked();
        cx.update(|_, cx| assert_eq!(notes.read(cx).editor.text(cx).as_ref(), "**café**\n茶"));
        cx.dispatch_action(Undo);
        cx.run_until_parked();
    }
}

#[gpui::test]
fn heading_menu_sets_levels_paragraph_and_rejects_stale_source(cx: &mut TestAppContext) {
    let (_dir, notes, cx) = fixture(cx);
    for level in 1..=6 {
        click("toolbar-heading", cx);
        let keys = std::iter::repeat_n("down", level)
            .chain(["enter"])
            .collect::<Vec<_>>()
            .join(" ");
        cx.simulate_keystrokes(&keys);
        cx.run_until_parked();
        cx.refresh().unwrap();
        cx.update(|window, cx| {
            let this = notes.read(cx);
            assert_eq!(
                this.editor.text(cx).as_ref(),
                format!("{} café\n茶", "#".repeat(level))
            );
            assert!(this.editor.is_focused(window, cx));
            assert_eq!(&this.editor.text(cx)[this.editor.selection(cx)], "café");
        });
    }
    click("toolbar-heading", cx);
    cx.simulate_keystrokes("down down down down down down down enter");
    cx.run_until_parked();
    cx.refresh().unwrap();
    cx.update(|_, cx| assert_eq!(notes.read(cx).editor.text(cx).as_ref(), "café\n茶"));
    click("toolbar-heading", cx);
    cx.update(|window, cx| {
        let editor = notes.read(cx).editor.clone();
        editor.replace_selection("changed", window, cx);
    });
    cx.simulate_keystrokes("down enter");
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(notes.read(cx).editor.text(cx).as_ref(), "changed\n茶"));
}

#[gpui::test]
fn link_toolbar_opens_existing_dialog_retains_selection_and_cancel_restores_focus(
    cx: &mut TestAppContext,
) {
    let (_dir, notes, cx) = fixture(cx);
    click("toolbar-link", cx);
    cx.update(|window, cx| {
        assert!(window.has_active_dialog(cx));
        assert_eq!(notes.read(cx).editor.selection(cx), 0..5);
        window.close_dialog(cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        assert_eq!(notes.read(cx).editor.text(cx).as_ref(), "café\n茶");
        assert!(notes.read(cx).editor.is_focused(window, cx));
    });
}

#[gpui::test]
fn formatting_buttons_and_handlers_reject_reading_and_busy_states(cx: &mut TestAppContext) {
    let (_dir, notes, cx) = fixture(cx);
    for busy in [false, true] {
        cx.update(|_, cx| {
            notes.update(cx, |this, cx| {
                this.mode = if busy {
                    ViewMode::Source
                } else {
                    ViewMode::Reading
                };
                if busy {
                    let _ = this.session.pick();
                }
                cx.notify();
            })
        });
        cx.refresh().unwrap();
        for selector in [
            "toolbar-bold",
            "toolbar-italic",
            "toolbar-link",
            "toolbar-heading",
            "toolbar-code-block",
        ] {
            click(selector, cx);
        }
        cx.update(|window, cx| {
            notes.update(cx, |this, cx| {
                this.apply_toolbar_format(Format::Bold, window, cx);
                this.edit_link(window, cx);
                assert!(!window.has_active_dialog(cx));
                assert_eq!(this.editor.text(cx).as_ref(), "café\n茶");
            })
        });
    }
}

#[gpui::test]
fn all_formatting_controls_inherit_presets_and_wrap(cx: &mut TestAppContext) {
    let (_dir, notes, cx) = fixture(cx);
    for (icon_size, _) in notebook::ToolbarIconSize::OPTIONS {
        cx.update(|_, cx| {
            notes.update(cx, |this, cx| {
                this.session_toolbar.icon_size = icon_size;
                cx.notify();
            })
        });
        cx.simulate_resize(gpui::size(px(180.), px(800.)));
        cx.refresh().unwrap();
        let toolbar = cx.debug_bounds("window-toolbar").unwrap();
        let mut previous = None;
        for item in ToolbarItem::DEFAULT_ORDER.into_iter().skip(4) {
            let (id, _, _) = formatting_item(item);
            let bounds = cx.debug_bounds(id).unwrap();
            assert_eq!(
                bounds.size,
                gpui::size(px(icon_size.button_pixels()), px(icon_size.button_pixels()))
            );
            assert!(bounds.left() >= toolbar.left() && bounds.right() <= toolbar.right());
            assert!(bounds.bottom() <= toolbar.bottom());
            if let Some(top) = previous {
                assert!(bounds.top() >= top);
            }
            previous = Some(bounds.top());
        }
        assert!(toolbar.size.height > px(icon_size.button_pixels() + 9.));
    }
}

#[test]
fn application_assets_load_all_formatting_icons_and_component_fallback() {
    let assets = crate::assets::Assets;
    for item in ToolbarItem::DEFAULT_ORDER.into_iter().skip(4) {
        let (_, path, _) = formatting_item(item);
        let bytes = assets.load(path).unwrap().unwrap();
        let svg = std::str::from_utf8(&bytes).unwrap();
        assert!(
            svg.contains("<svg ")
                && svg.contains("viewBox=\"0 0 24 24\"")
                && svg.trim_end().ends_with("</svg>")
        );
        assert!(
            assets
                .list("toolbar/")
                .unwrap()
                .iter()
                .any(|name| name.as_ref() == path)
        );
    }
    assert!(assets.load("icons/search.svg").unwrap().is_some());
    assert_eq!(assets.list("toolbar/").unwrap().len(), 11);
}

#[gpui::test]
fn formatting_controls_reject_empty_and_image_views(cx: &mut TestAppContext) {
    let (dir, notes, cx) = fixture(cx);
    for image in [false, true] {
        cx.update(|window, cx| {
            notes.update(cx, |this, cx| {
                this.install_document(None, window, cx);
                if image {
                    let observed = this.observed_document(cx);
                    let _ = this.session.request(crate::Next::Nothing, observed);
                    let operation = this.session.foreground().unwrap().0;
                    let _ = this
                        .session
                        .dispatch(operation, crate::app::session::Stage::Load);
                    this.complete_content_load(
                        operation,
                        Ok(crate::app::session::Loaded::Image(
                            dir.path().join("image.png"),
                        )),
                        window,
                        cx,
                    );
                }
                assert!(!this.can_edit_document());
                cx.notify();
            })
        });
        cx.refresh().unwrap();
        let before = cx.update(|_, cx| notes.read(cx).editor.text(cx));
        for selector in ["toolbar-bold", "toolbar-link", "toolbar-heading"] {
            click(selector, cx);
        }
        cx.update(|window, cx| {
            notes.update(cx, |this, cx| {
                this.apply_toolbar_format(Format::Bold, window, cx);
                assert_eq!(this.editor.text(cx), before);
                assert!(!window.has_active_dialog(cx));
            })
        });
    }
}
