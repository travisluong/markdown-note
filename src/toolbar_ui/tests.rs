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
fn formatting_shortcuts_only_edit_focused_documents(cx: &mut TestAppContext) {
    cx.update(|cx| {
        cx.bind_keys(
            crate::help_ui::app_shortcuts()
                .into_iter()
                .map(|s| s.binding),
        )
    });
    let (_dir, notes, cx) = fixture(cx);
    for mode in [ViewMode::Source, ViewMode::LivePreview] {
        for (key, expected, selection) in [("b", "**café**\n茶", 2..7), ("i", "*café*\n茶", 1..6)]
        {
            let key = format!(
                "{}-{key}",
                if cfg!(target_os = "macos") {
                    "cmd"
                } else {
                    "ctrl"
                }
            );
            cx.update(|window, cx| {
                notes.update(cx, |this, cx| {
                    this.mode = mode;
                    this.editor.set_mode(mode, cx);
                    this.editor.select(0..5, window, cx);
                    this.editor.focus(window, cx);
                    cx.notify();
                })
            });
            cx.refresh().unwrap();
            cx.simulate_keystrokes(&key);
            cx.run_until_parked();
            cx.update(|_, cx| {
                assert_eq!(notes.read(cx).editor.text(cx).as_ref(), expected);
                assert_eq!(notes.read(cx).editor.selection(cx), selection);
            });
            cx.dispatch_action(Undo);
            cx.run_until_parked();
            cx.update(|window, cx| notes.update(cx, |this, cx| this.command_palette(window, cx)));
            cx.refresh().unwrap();
            cx.simulate_keystrokes(&key);
            cx.run_until_parked();
            cx.update(|window, cx| {
                notes.update(cx, |this, cx| {
                    assert_eq!(this.editor.text(cx).as_ref(), "café\n茶");
                    this.apply_markdown_format(Format::Bold, window, cx);
                    assert_eq!(
                        this.editor.text(cx).as_ref(),
                        "café\n茶",
                        "modal rejects direct dispatch too"
                    );
                    window.close_dialog(cx);
                })
            });
            cx.run_until_parked();
        }
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
                this.apply_markdown_format(Format::Bold, window, cx);
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
        let toolbar = cx.debug_bounds("content-toolbar").unwrap();
        let mut previous = None;
        for item in ToolbarItem::DEFAULT_ORDER
            .into_iter()
            .filter(|item| item.region() == ToolbarRegion::ContentCenter)
        {
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
    for item in ToolbarItem::DEFAULT_ORDER
        .into_iter()
        .filter(|item| item.region() == ToolbarRegion::ContentCenter)
    {
        let (_, path, _) = formatting_item(item);
        let bytes = assets.load(path).unwrap().unwrap();
        let svg = std::str::from_utf8(&bytes).unwrap();
        assert!(
            svg.contains("<svg")
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
    assert!(assets.load("toolbar/sort.svg").unwrap().is_some());
    assert_eq!(assets.list("toolbar/").unwrap().len(), 12);
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
                this.apply_markdown_format(Format::Bold, window, cx);
                assert_eq!(this.editor.text(cx), before);
                assert!(!window.has_active_dialog(cx));
            })
        });
    }
}

#[gpui::test]
fn history_buttons_navigate_standalone_files_and_branch(cx: &mut TestAppContext) {
    use crate::app::navigation::Direction;
    let (dir, notes, cx) = fixture(cx);
    let first = std::fs::canonicalize(dir.path().join("standalone.md")).unwrap();
    let second = dir.path().join("second.md");
    let third = dir.path().join("third.md");
    std::fs::write(&second, "second").unwrap();
    std::fs::write(&third, "third").unwrap();
    click("toolbar-back", cx); // Disabled at the first visit.
    for path in [&second, &second, &third] {
        cx.update(|window, cx| {
            notes.update(cx, |this, cx| {
                this.open_path(path.clone(), false, window, cx)
            })
        });
        cx.run_until_parked();
    }
    cx.refresh().unwrap();
    click("toolbar-back", cx);
    cx.update(|_, cx| assert_eq!(notes.read(cx).editor.text(cx).as_ref(), "second"));
    click("toolbar-back", cx);
    cx.update(|_, cx| assert_eq!(notes.read(cx).document().unwrap().path, first));
    click("toolbar-forward", cx);
    cx.update(|_, cx| assert_eq!(notes.read(cx).editor.text(cx).as_ref(), "second"));
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.open_path(first.clone(), false, window, cx)
        })
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert!(
            notes
                .read(cx)
                .navigation
                .target(Direction::Forward)
                .is_none()
        )
    });
}

#[gpui::test]
fn history_clicks_save_discard_cancel_and_failed_load_keep_cursor(cx: &mut TestAppContext) {
    use crate::app::navigation::Direction;
    let (dir, notes, cx) = fixture(cx);
    let first = std::fs::canonicalize(dir.path().join("standalone.md")).unwrap();
    let second = dir.path().join("second.md");
    std::fs::write(&second, "second").unwrap();
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.open_path(second.clone(), false, window, cx)
        })
    });
    cx.run_until_parked();
    for answer in ["Cancel", "Discard", "Save"] {
        cx.update(|window, cx| {
            notes.update(cx, |this, cx| {
                this.editor
                    .select(0..this.editor.snapshot(cx).len(), window, cx);
                this.editor.replace_selection("draft", window, cx);
            })
        });
        cx.run_until_parked();
        cx.refresh().unwrap();
        click("toolbar-back", cx);
        assert!(cx.has_pending_prompt());
        cx.update(|_, cx| {
            assert_eq!(
                notes
                    .read(cx)
                    .navigation
                    .target(Direction::Back)
                    .unwrap()
                    .path,
                first
            )
        });
        cx.simulate_prompt_answer(answer);
        cx.run_until_parked();
        cx.refresh().unwrap();
        cx.update(|_, cx| {
            let this = notes.read(cx);
            if answer == "Cancel" {
                assert!(this.dirty(cx));
                assert_eq!(this.navigation.target(Direction::Back).unwrap().path, first);
            } else {
                assert_eq!(this.document().unwrap().path, first);
                assert!(this.navigation.target(Direction::Back).is_none());
            }
        });
        if answer != "Cancel" {
            click("toolbar-forward", cx);
        }
    }
    assert_eq!(std::fs::read_to_string(&second).unwrap(), "draft");
    std::fs::remove_file(&first).unwrap();
    click("toolbar-back", cx);
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer("Ok");
    cx.run_until_parked();
    cx.update(|_, cx| {
        let this = notes.read(cx);
        assert_eq!(this.navigation.target(Direction::Back).unwrap().path, first);
        assert_eq!(this.editor.text(cx).as_ref(), "draft");
        assert!(this.navigation_pending.is_none());
    });
}

#[gpui::test]
fn history_rejects_dialog_busy_and_stale_completion(cx: &mut TestAppContext) {
    use crate::app::{
        navigation::Direction,
        session::{Loaded, Stage},
    };
    let (dir, notes, cx) = fixture(cx);
    let second = dir.path().join("second.md");
    std::fs::write(&second, "second").unwrap();
    cx.update(|window, cx| notes.update(cx, |this, cx| this.open_path(second, false, window, cx)));
    cx.run_until_parked();
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            let visit = this.navigation.target(Direction::Back).unwrap();
            this.open_settings(window, cx);
            this.navigate_history(Direction::Back, window, cx);
            assert_eq!(this.editor.text(cx).as_ref(), "second");
            window.close_dialog(cx);
            let observed = this.observed_document(cx);
            let _ = this.session.request(crate::Next::Nothing, observed);
            let operation = this.session.foreground().unwrap().0;
            let _ = this.session.dispatch(operation, Stage::Load);
            this.navigate_history(Direction::Back, window, cx);
            assert_eq!(this.navigation.target(Direction::Back), Some(visit.clone()));
            let effects = this.session.cancel(operation, observed);
            this.apply_session_effects(effects, window, cx);
            assert!(!this.complete_content_load(operation, Ok(Loaded::Empty), window, cx));
            assert_eq!(this.navigation.target(Direction::Back), Some(visit));
            this.install_document(None, window, cx);
            assert!(this.navigation.target(Direction::Back).is_none());
            assert!(this.navigation.target(Direction::Forward).is_none());
        })
    });
}

#[gpui::test]
fn history_revisits_images_after_notebook_switch(cx: &mut TestAppContext) {
    let (dir, notes, cx) = fixture(cx);
    let book = notebook::Notebook::open(&dir.path().join("book"), true).unwrap();
    let other = notebook::Notebook::open(&dir.path().join("other"), true).unwrap();
    let image = book.root.join("image.png");
    std::fs::write(&image, b"fixture").unwrap(); // Decoding is independent of accepted path installation.
    let second = other.root.join("second.md");
    std::fs::write(&second, "second").unwrap();
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.notebooks.active = Some(book);
            this.open_path(image.clone(), false, window, cx);
            assert_eq!(this.image_path(), Some(image.as_path()));
            this.notebooks.active = Some(other);
            this.open_path(second, false, window, cx);
        })
    });
    cx.run_until_parked();
    cx.refresh().unwrap();
    click("toolbar-back", cx);
    cx.update(|_, cx| assert_eq!(notes.read(cx).image_path(), Some(image.as_path())));
    click("toolbar-forward", cx);
    cx.update(|_, cx| assert_eq!(notes.read(cx).editor.text(cx).as_ref(), "second"));
}

#[gpui::test]
fn history_shortcuts_and_view_menu_navigate_standalone_files(cx: &mut TestAppContext) {
    cx.update(|cx| {
        cx.bind_keys(
            crate::help_ui::app_shortcuts()
                .into_iter()
                .map(|s| s.binding),
        )
    });
    let (dir, notes, cx) = fixture(cx);
    let second = dir.path().join("second.md");
    std::fs::write(&second, "second").unwrap();
    cx.update(|window, cx| notes.update(cx, |this, cx| this.open_path(second, false, window, cx)));
    cx.run_until_parked();
    let (back, forward) = if cfg!(target_os = "macos") {
        ("cmd-alt-left", "cmd-alt-right")
    } else {
        ("alt-left", "alt-right")
    };
    cx.refresh().unwrap();
    cx.simulate_keystrokes(back);
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(notes.read(cx).editor.text(cx).as_ref(), "café\n茶"));
    cx.refresh().unwrap();
    cx.simulate_keystrokes(forward);
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(notes.read(cx).editor.text(cx).as_ref(), "second"));
    // Mode entries and Centered editor stay first; Back follows them and Forward is disabled at the end.
    cx.refresh().unwrap();
    click("view-menu", cx);
    cx.simulate_keystrokes("down down down down down enter");
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(notes.read(cx).editor.text(cx).as_ref(), "café\n茶"));
    cx.refresh().unwrap();
    click("view-menu", cx);
    // Back is now disabled, so the fifth enabled item is Forward.
    cx.simulate_keystrokes("down down down down down enter");
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(notes.read(cx).editor.text(cx).as_ref(), "second"));
    cx.update(|window, cx| notes.update(cx, |this, cx| this.new_tab(window, cx)));
    cx.run_until_parked();
    cx.refresh().unwrap();
    cx.simulate_keystrokes(back);
    cx.run_until_parked();
    cx.update(|_, cx| {
        let this = notes.read(cx);
        assert!(this.document().is_none());
        assert!(!this.capabilities().has_back_history);
    });
}

#[gpui::test]
fn toolbar_icon_color_keeps_disabled_formatting_distinct(cx: &mut TestAppContext) {
    use gpui_component::{ActiveTheme, Icon, Theme};
    let (_dir, notes, cx) = fixture(cx);
    for mode in [ViewMode::Source, ViewMode::Reading] {
        cx.update(|_, cx| {
            notes.update(cx, |this, cx| {
                this.mode = mode;
                this.editor.set_mode(mode, cx);
                cx.notify();
            });
            Theme::global_mut(cx).icon_color = Some(gpui::rgb(0x345678).into());
            Icon::diagnostic_begin_color_capture(cx);
        });
        cx.update(|window, cx| {
            window.refresh();
            window.draw(cx).clear();
        });
        cx.update(|_, cx| {
            let colors = Icon::diagnostic_colors(cx);
            let bold = colors
                .iter()
                .rev()
                .find(|(path, _)| path.as_ref() == "toolbar/bold.svg")
                .unwrap()
                .1;
            assert_eq!(
                bold,
                if mode == ViewMode::Source {
                    gpui::rgb(0x345678).into()
                } else {
                    cx.theme().muted_foreground.opacity(0.5)
                }
            );
        });
    }
}

fn drag_defaults() -> notebook::ToolbarSettings {
    notebook::ToolbarSettings {
        sidebar: true,
        ..Default::default()
    }
}

fn drag_fixture(
    cx: &mut TestAppContext,
) -> (tempfile::TempDir, Entity<Notes>, &mut VisualTestContext) {
    let (dir, notes, cx) = fixture(cx);
    cx.update(|_, cx| {
        notes.update(cx, |this, cx| {
            this.session_toolbar = drag_defaults();
            cx.notify();
        })
    });
    cx.refresh().unwrap();
    (dir, notes, cx)
}

fn icon_bounds(item: ToolbarItem, cx: &mut VisualTestContext) -> gpui::Bounds<gpui::Pixels> {
    cx.debug_bounds(format!("window-toolbar-item-{}", item as usize).leak())
        .unwrap()
}

fn start_icon_drag(item: ToolbarItem, cx: &mut VisualTestContext) {
    let start = icon_bounds(item, cx).center();
    cx.simulate_mouse_move(start, None, Modifiers::default());
    cx.simulate_mouse_down(start, gpui::MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(
        gpui::point(start.x + px(12.), start.y),
        gpui::MouseButton::Left,
        Modifiers::default(),
    );
    cx.refresh().unwrap();
    cx.update(|_, cx| assert!(cx.has_active_drag(), "{item:?} did not start dragging"));
}

fn finish_icon_drag(target: ToolbarItem, after: bool, cx: &mut VisualTestContext) {
    let target = icon_bounds(target, cx);
    let destination = gpui::point(
        if after {
            target.right() - px(3.)
        } else {
            target.left() + px(3.)
        },
        target.center().y,
    );
    cx.simulate_mouse_move(destination, gpui::MouseButton::Left, Modifiers::default());
    cx.refresh().unwrap();
    cx.simulate_mouse_up(destination, gpui::MouseButton::Left, Modifiers::default());
    cx.run_until_parked();
    cx.refresh().unwrap();
    cx.update(|_, cx| assert!(!cx.has_active_drag()));
}

#[gpui::test]
fn icon_drags_reorder_across_wrapped_rows_at_every_size(cx: &mut TestAppContext) {
    use ToolbarItem::*;
    let (_dir, notes, cx) = drag_fixture(cx);
    for (size, _) in ToolbarIconSize::OPTIONS {
        cx.update(|_, cx| {
            notes.update(cx, |this, cx| {
                this.session_toolbar = notebook::ToolbarSettings {
                    icon_size: size,
                    ..drag_defaults()
                };
                cx.notify();
            })
        });
        cx.simulate_resize(gpui::size(px(180.), px(800.)));
        cx.refresh().unwrap();
        assert!(icon_bounds(TaskList, cx).top() > icon_bounds(Bold, cx).top());
        let before = icon_bounds(Bold, cx);
        start_icon_drag(TaskList, cx);
        assert_eq!(
            icon_bounds(Bold, cx),
            before,
            "drag overlays must not move icons"
        );
        finish_icon_drag(Bold, false, cx);
        cx.update(|_, cx| assert_eq!(notes.read(cx).toolbar_settings().order[6], TaskList));
        start_icon_drag(TaskList, cx);
        finish_icon_drag(UnorderedList, true, cx);
        cx.update(|window, cx| {
            let this = notes.read(cx);
            assert_eq!(this.toolbar_settings().order, ToolbarItem::DEFAULT_ORDER);
            assert_eq!(this.editor.text(cx).as_ref(), "café\n茶");
            assert_eq!(this.editor.selection(cx), 0..5);
            assert!(this.editor.is_focused(window, cx));
            assert!(!this.dirty(cx));
        });
    }
}

#[gpui::test]
fn disabled_icons_drag_and_short_clicks_still_work(cx: &mut TestAppContext) {
    use ToolbarItem::*;
    let (_dir, notes, cx) = drag_fixture(cx);
    // History actions are disabled in this standalone fixture, but can still be reordered.
    start_icon_drag(Forward, cx);
    finish_icon_drag(Back, false, cx);
    cx.update(|window, cx| {
        assert_eq!(
            notes.read(cx).toolbar_settings().order[1..3],
            [Forward, Back]
        );
        assert!(notes.read(cx).editor.is_focused(window, cx));
        assert!(!window.has_active_dialog(cx));
    });
    cx.update(|_, cx| {
        notes.update(cx, |this, cx| {
            this.mode = ViewMode::Reading;
            cx.notify();
        })
    });
    cx.refresh().unwrap();
    for item in [Bold, Heading, Link] {
        start_icon_drag(item, cx);
        finish_icon_drag(Italic, false, cx);
        cx.update(|window, cx| {
            let this = notes.read(cx);
            assert_eq!(this.editor.text(cx).as_ref(), "café\n茶");
            assert_eq!(this.editor.selection(cx), 0..5);
            assert!(!window.has_active_dialog(cx));
        });
    }
    cx.update(|_, cx| {
        notes.update(cx, |this, cx| {
            this.mode = ViewMode::Source;
            cx.notify();
        })
    });
    cx.refresh().unwrap();
    let start = icon_bounds(Bold, cx).center();
    cx.simulate_mouse_down(start, gpui::MouseButton::Left, Modifiers::default());
    let end = gpui::point(start.x + px(1.), start.y);
    cx.simulate_mouse_move(end, gpui::MouseButton::Left, Modifiers::default());
    cx.update(|_, cx| assert!(!cx.has_active_drag()));
    cx.simulate_mouse_up(end, gpui::MouseButton::Left, Modifiers::default());
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(notes.read(cx).editor.text(cx).as_ref(), "**café**\n茶"));
}

#[gpui::test]
fn dropdown_icon_drags_preserve_focus_and_never_open_menus(cx: &mut TestAppContext) {
    use ToolbarItem::*;
    let (dir, notes, cx) = drag_fixture(cx);
    let mut book = notebook::Notebook::open(&dir.path().join("book"), true).unwrap();
    book.settings.toolbar.sidebar = true;
    cx.update(|_, cx| {
        notes.update(cx, |this, cx| {
            this.notebooks.active = Some(book);
            this.notebooks
                .set_test_persistence_target(dir.path().join("index.json"));
            cx.notify();
        })
    });
    cx.refresh().unwrap();
    for item in [Heading, Sort, Bold, Link] {
        start_icon_drag(item, cx);
        cx.update(|window, cx| assert!(notes.read(cx).editor.is_focused(window, cx)));
        let first = cx.update(|_, cx| {
            notes
                .read(cx)
                .toolbar_settings()
                .order
                .into_iter()
                .find(|candidate| candidate.region() == item.region())
                .unwrap()
        });
        finish_icon_drag(first, false, cx);
        cx.update(|window, cx| {
            let this = notes.read(cx);
            assert_eq!(
                this.toolbar_settings()
                    .order
                    .into_iter()
                    .find(|candidate| candidate.region() == item.region()),
                Some(item)
            );
            assert!(this.editor.is_focused(window, cx));
            assert_eq!(this.editor.text(cx).as_ref(), "café\n茶");
            assert_eq!(this.editor.selection(cx), 0..5);
            assert!(!window.has_active_dialog(cx));
        });
    }
    click("sidebar-sort", cx);
    cx.update(|window, cx| assert!(!notes.read(cx).editor.is_focused(window, cx)));
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    cx.update(|window, cx| {
        let editor = notes.read(cx).editor.clone();
        editor.focus(window, cx);
    });
    click("toolbar-heading", cx);
    // A real press can render the outside-click dismissal before mouse-up.
    let position = icon_bounds(Heading, cx).center();
    cx.simulate_mouse_down(position, gpui::MouseButton::Left, Modifiers::default());
    cx.run_until_parked();
    cx.refresh().unwrap();
    cx.simulate_mouse_up(position, gpui::MouseButton::Left, Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| assert!(notes.read(cx).editor.is_focused(window, cx)));
}

#[gpui::test]
fn icon_order_persists_and_noop_or_outside_drops_do_not_write(cx: &mut TestAppContext) {
    use ToolbarItem::*;
    let (dir, notes, cx) = drag_fixture(cx);
    let mut book = notebook::Notebook::open(&dir.path().join("book"), true).unwrap();
    book.settings.toolbar.sidebar = true;
    book.settings.toolbar.set_enabled(Back, false);
    book.settings.toolbar.set_enabled(Forward, false);
    let root = book.root.clone();
    cx.update(|_, cx| {
        notes.update(cx, |this, cx| {
            this.notebooks.active = Some(book);
            this.notebooks
                .set_test_persistence_target(dir.path().join("index.json"));
            cx.notify();
        })
    });
    cx.refresh().unwrap();
    start_icon_drag(TaskList, cx);
    finish_icon_drag(Bold, false, cx);
    let saved = notebook::Notebook::open(&root, false)
        .unwrap()
        .settings
        .toolbar;
    assert_eq!(saved.order[6], TaskList);
    assert_eq!(saved.order[1..3], [Back, Forward]);
    assert!(!saved.back && !saved.forward);
    cx.update(|_, cx| assert_eq!(notes.read(cx).session_toolbar, drag_defaults()));
    let settings_path = root.join(".mithril/settings.json");
    std::fs::remove_file(&settings_path).unwrap();
    for target in [TaskList, Bold] {
        start_icon_drag(TaskList, cx);
        finish_icon_drag(target, false, cx);
        assert!(!settings_path.exists(), "no-op drop submitted preferences");
    }
    start_icon_drag(Bold, cx);
    let outside = gpui::point(px(2.), px(2.));
    cx.simulate_mouse_move(outside, gpui::MouseButton::Left, Modifiers::default());
    cx.refresh().unwrap();
    cx.simulate_mouse_up(outside, gpui::MouseButton::Left, Modifiers::default());
    cx.run_until_parked();
    assert!(!settings_path.exists());
    cx.update(|_, cx| assert_eq!(notes.read(cx).toolbar_settings(), saved));
}

#[gpui::test]
fn toolbar_drop_rechecks_owner_scope_visibility_modal_and_busy(cx: &mut TestAppContext) {
    use ToolbarItem::*;
    let (dir, notes, cx) = drag_fixture(cx);
    let foreign_owner = cx.update(|_, cx| cx.new(|_| ()).entity_id());
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            let valid = WindowToolbarDrag {
                owner: cx.entity_id(),
                scope: None,
                item: Bold,
                size: ToolbarIconSize::default(),
            };
            let before = this.toolbar_settings();
            for rejected in [
                WindowToolbarDrag {
                    owner: foreign_owner,
                    ..valid.clone()
                },
                WindowToolbarDrag {
                    scope: Some(dir.path().to_path_buf()),
                    ..valid.clone()
                },
            ] {
                this.drop_window_toolbar_item(&rejected, Italic, false, window, cx);
                assert_eq!(this.toolbar_settings(), before);
            }
            for hidden in [Bold, Italic] {
                this.session_toolbar.set_enabled(hidden, false);
                let hidden_settings = this.toolbar_settings();
                this.drop_window_toolbar_item(&valid, Italic, false, window, cx);
                assert_eq!(this.toolbar_settings(), hidden_settings);
                this.session_toolbar.set_enabled(hidden, true);
            }
            this.session_toolbar.visible = false;
            let hidden_toolbar = this.toolbar_settings();
            this.drop_window_toolbar_item(&valid, Italic, false, window, cx);
            assert_eq!(this.toolbar_settings(), hidden_toolbar);
            this.session_toolbar.visible = true;
            window.open_dialog(cx, |dialog, _, _| dialog.title("Blocked"));
            this.drop_window_toolbar_item(&valid, Italic, false, window, cx);
            assert_eq!(this.toolbar_settings(), before);
            window.close_dialog(cx);
            let _ = this.session.pick();
            this.drop_window_toolbar_item(&valid, Italic, false, window, cx);
            assert_eq!(this.toolbar_settings(), before);
        })
    });
    cx.refresh().unwrap();
    let start = icon_bounds(Bold, cx).center();
    cx.simulate_mouse_down(start, gpui::MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(
        gpui::point(start.x + px(12.), start.y),
        gpui::MouseButton::Left,
        Modifiers::default(),
    );
    cx.update(|_, cx| assert!(!cx.has_active_drag()));
    cx.simulate_mouse_up(start, gpui::MouseButton::Left, Modifiers::default());
}

#[gpui::test]
fn changing_notebooks_during_icon_drag_rejects_the_drop(cx: &mut TestAppContext) {
    let (dir, notes, cx) = drag_fixture(cx);
    start_icon_drag(ToolbarItem::Bold, cx);
    let mut book = notebook::Notebook::open(&dir.path().join("other"), true).unwrap();
    book.settings.toolbar.sidebar = true;
    cx.update(|_, cx| {
        notes.update(cx, |this, cx| {
            this.notebooks.active = Some(book);
            this.notebooks
                .set_test_persistence_target(dir.path().join("index.json"));
            cx.notify();
        })
    });
    cx.refresh().unwrap();
    finish_icon_drag(ToolbarItem::Sidebar, false, cx);
    cx.update(|_, cx| {
        assert_eq!(notes.read(cx).toolbar_settings(), drag_defaults());
        assert_eq!(notes.read(cx).session_toolbar, drag_defaults());
    });
}

#[gpui::test]
fn dragging_available_back_icon_does_not_navigate_but_clicking_does(cx: &mut TestAppContext) {
    let (dir, notes, cx) = drag_fixture(cx);
    let first = dir.path().join("standalone.md").canonicalize().unwrap();
    let second = dir.path().join("second.md");
    std::fs::write(&second, "second").unwrap();
    let second = second.canonicalize().unwrap();
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.open_path(second.clone(), false, window, cx)
        })
    });
    cx.run_until_parked();
    cx.refresh().unwrap();
    let before = cx.update(|_, cx| {
        notes
            .read(cx)
            .tabs
            .active
            .navigation
            .target(crate::app::navigation::Direction::Back)
    });
    start_icon_drag(ToolbarItem::Back, cx);
    finish_icon_drag(ToolbarItem::TaskList, true, cx);
    cx.update(|window, cx| {
        let this = notes.read(cx);
        assert_eq!(this.path(), Some(second.as_path()));
        assert_eq!(
            this.tabs
                .active
                .navigation
                .target(crate::app::navigation::Direction::Back),
            before
        );
        assert!(this.editor.is_focused(window, cx));
    });
    click("toolbar-back", cx);
    cx.update(|_, cx| assert_eq!(notes.read(cx).path(), Some(first.as_path())));
}

#[gpui::test]
fn split_toolbars_follow_columns_and_ignore_legacy_alignment(cx: &mut TestAppContext) {
    let (dir, notes, cx) = fixture(cx);
    assert!(cx.debug_bounds("sidebar-toolbar").is_none());
    let book = notebook::Notebook::open(&dir.path().join("book"), true).unwrap();
    cx.update(|_, cx| {
        notes.update(cx, |this, cx| {
            this.notebooks.active = Some(book);
            cx.notify();
        })
    });
    for (icon_size, _) in ToolbarIconSize::OPTIONS {
        for (alignment, _) in notebook::ToolbarAlignment::OPTIONS {
            cx.update(|_, cx| {
                notes.update(cx, |this, cx| {
                    let toolbar = &mut this.notebooks.active.as_mut().unwrap().settings.toolbar;
                    toolbar.icon_size = icon_size;
                    toolbar.alignment = alignment;
                    cx.notify();
                })
            });
            cx.simulate_resize(gpui::size(px(1600.), px(900.)));
            cx.run_until_parked();
            cx.refresh().unwrap();
            cx.run_until_parked();
            cx.refresh().unwrap();
            let sidebar = cx.debug_bounds("sidebar-toolbar").unwrap();
            let content = cx.debug_bounds("content-toolbar").unwrap();
            let first = cx.debug_bounds("toolbar-bold").unwrap();
            let last = cx.debug_bounds("toolbar-task-list").unwrap();
            assert_eq!(sidebar.top(), content.top());
            assert_eq!(sidebar.right(), content.left());
            assert!(
                (first.left() - content.left() - (content.right() - last.right())).abs() <= px(1.)
            );
            assert_eq!(first.top(), cx.debug_bounds("toolbar-back").unwrap().top());
            for selector in ["sidebar-explorer", "sidebar-search", "sidebar-sort"] {
                let icon = cx.debug_bounds(selector).unwrap();
                assert!(icon.left() >= sidebar.left() && icon.right() <= sidebar.right());
            }
            cx.simulate_resize(gpui::size(px(400.), px(900.)));
            cx.run_until_parked();
            cx.refresh().unwrap();
            cx.run_until_parked();
            cx.refresh().unwrap();
            assert!(
                cx.debug_bounds("toolbar-bold").unwrap().top()
                    > cx.debug_bounds("toolbar-back").unwrap().top()
            );
        }
    }
    cx.update(|window, cx| notes.update(cx, |this, cx| this.toggle_sidebar(window, cx)));
    cx.run_until_parked();
    cx.refresh().unwrap();
    assert_eq!(cx.debug_bounds("content-toolbar").unwrap().left(), px(0.));
}

#[gpui::test]
fn icon_drop_rejects_cross_region_without_persisting(cx: &mut TestAppContext) {
    let (_dir, notes, cx) = drag_fixture(cx);
    let before = cx.update(|_, cx| notes.read(cx).toolbar_settings());
    start_icon_drag(ToolbarItem::Bold, cx);
    finish_icon_drag(ToolbarItem::Back, false, cx);
    cx.update(|_, cx| assert_eq!(notes.read(cx).toolbar_settings(), before));
}

fn settle_layout(cx: &mut VisualTestContext) {
    cx.refresh().unwrap();
    cx.run_until_parked();
    cx.refresh().unwrap();
}

#[gpui::test]
fn right_panel_toggle_focuses_terminal_and_retains_document_selection_and_history(
    cx: &mut TestAppContext,
) {
    let (_dir, notes, cx) = fixture(cx);
    let editor = cx.update(|_, cx| {
        let this = notes.read(cx);
        assert!(!this.right_panel_open);
        this.editor.id()
    });
    click("toolbar-bold", cx);
    let selection = cx.update(|_, cx| notes.read(cx).editor.selection(cx));
    click("toolbar-terminal", cx);
    settle_layout(cx);
    cx.update(|window, cx| {
        let this = notes.read(cx);
        assert!(this.right_panel_open);
        assert_eq!(this.editor.id(), editor);
        assert_eq!(this.editor.selection(cx), selection);
        assert!(
            this.terminals
                .active_session()
                .unwrap()
                .read(cx)
                .view
                .as_ref()
                .unwrap()
                .read(cx)
                .focus_handle()
                .is_focused(window)
        );
    });
    let panel = cx.debug_bounds("right-panel").unwrap();
    let toolbar = cx.debug_bounds("content-toolbar").unwrap();
    assert_eq!(panel.size.width, px(280.));
    assert_eq!(panel.top(), toolbar.top());
    assert_eq!(panel.left(), toolbar.right());
    assert_eq!(
        cx.debug_bounds("toolbar-terminal").unwrap().right(),
        toolbar.right() - px(4.)
    );
    click("toolbar-terminal", cx);
    cx.update(|window, cx| {
        assert!(!notes.read(cx).right_panel_open);
        assert!(notes.read(cx).editor.is_focused(window, cx));
        assert_eq!(notes.read(cx).editor.selection(cx), selection);
    });
    cx.dispatch_action(Undo);
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(notes.read(cx).editor.text(cx).as_ref(), "café\n茶"));
    click("toolbar-terminal", cx);
    click("toolbar-terminal", cx);
    cx.update(|_, cx| assert!(!notes.read(cx).right_panel_open));
}

#[gpui::test]
fn right_panel_resize_and_workspace_changes_preserve_preferred_widths(cx: &mut TestAppContext) {
    let (dir, notes, cx) = fixture(cx);
    let book = notebook::Notebook::open(&dir.path().join("book"), true).unwrap();
    let other = notebook::Notebook::open(&dir.path().join("other"), true).unwrap();
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.notebooks.active = Some(book);
            this.notebooks
                .set_test_persistence_target(dir.path().join("index.json"));
            this.toggle_right_panel(window, cx);
        })
    });
    cx.simulate_resize(gpui::size(px(1400.), px(900.)));
    settle_layout(cx);
    let panel = cx.debug_bounds("right-panel").unwrap();
    let start = gpui::point(panel.left(), panel.center().y);
    cx.simulate_mouse_move(start, None, Modifiers::default());
    cx.simulate_mouse_down(start, gpui::MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(
        gpui::point(start.x - px(12.), start.y),
        gpui::MouseButton::Left,
        Modifiers::default(),
    );
    settle_layout(cx);
    let end = gpui::point(start.x - px(100.), start.y);
    cx.simulate_mouse_move(end, gpui::MouseButton::Left, Modifiers::default());
    settle_layout(cx);
    cx.simulate_mouse_up(end, gpui::MouseButton::Left, Modifiers::default());
    settle_layout(cx);
    let preferred = cx.update(|_, cx| {
        let this = notes.read(cx);
        assert!(this.right_panel_width > 350.);
        assert_eq!(
            this.notebooks
                .active
                .as_ref()
                .unwrap()
                .settings
                .sidebar_width,
            280.
        );
        this.right_panel_width
    });
    for width in [400., 600., 1400.] {
        cx.simulate_resize(gpui::size(px(width), px(900.)));
        settle_layout(cx);
        let left = cx.debug_bounds("workspace-sidebar").unwrap();
        let center = cx.debug_bounds("workspace-content").unwrap();
        let right = cx.debug_bounds("right-panel").unwrap();
        assert_eq!(left.right(), center.left());
        assert_eq!(center.right(), right.left());
        assert!(
            center.size.width >= px(199.5),
            "{left:?} {center:?} {right:?}"
        );
        assert!(right.right() <= px(width + 0.5));
        cx.update(|_, cx| {
            let this = notes.read(cx);
            assert_eq!(this.right_panel_width, preferred);
            assert_eq!(
                this.notebooks
                    .active
                    .as_ref()
                    .unwrap()
                    .settings
                    .sidebar_width,
                280.
            );
        });
    }
    assert!((cx.debug_bounds("right-panel").unwrap().size.width - px(preferred)).abs() <= px(1.));
    click("toolbar-terminal", cx);
    click("toolbar-terminal", cx);
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.new_tab(window, cx);
            this.notebooks.active = Some(other);
            this.reset_sidebar_resize(cx);
            this.toggle_sidebar(window, cx);
            assert!(this.right_panel_open);
            assert_eq!(this.right_panel_width, preferred);
        })
    });
    settle_layout(cx);
    assert_eq!(cx.debug_bounds("workspace-content").unwrap().left(), px(0.));
    assert!((cx.debug_bounds("right-panel").unwrap().size.width - px(preferred)).abs() <= px(1.));
}

#[gpui::test]
fn right_panel_rechecks_modal_and_busy_guards_and_toggles_when_toolbar_hidden(
    cx: &mut TestAppContext,
) {
    let (_dir, notes, cx) = fixture(cx);
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.open_settings(window, cx);
            this.toggle_right_panel(window, cx);
            assert!(!this.right_panel_open);
            window.close_dialog(cx);
        })
    });
    cx.run_until_parked();
    click("toolbar-terminal", cx);
    cx.update(|window, cx| notes.update(cx, |this, cx| this.toggle_toolbar(window, cx)));
    settle_layout(cx);
    cx.dispatch_action(crate::ToggleTerminal);
    cx.run_until_parked();
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            assert!(!this.right_panel_open);
            let _ = this.session.pick();
            this.toggle_right_panel(window, cx);
            assert!(!this.right_panel_open);
        })
    });
}

#[gpui::test]
fn toolbar_geometry_uses_logical_pixels_with_large_interface_text(cx: &mut TestAppContext) {
    let (_dir, notes, cx) = fixture(cx);
    cx.update(|window, cx| {
        gpui_component::Theme::global_mut(cx).font_size = px(28.);
        window.set_rem_size(px(28.));
        notes.update(cx, |this, cx| {
            this.session_toolbar.icon_size = ToolbarIconSize::Large;
            cx.notify();
        });
    });
    cx.simulate_resize(gpui::size(px(400.), px(800.)));
    settle_layout(cx);
    let toolbar = cx.debug_bounds("content-toolbar").unwrap();
    let back = cx.debug_bounds("toolbar-back").unwrap();
    let terminal = cx.debug_bounds("toolbar-terminal").unwrap();
    assert_eq!(back.top(), terminal.top());
    assert_eq!(back.left(), toolbar.left() + px(4.));
    assert_eq!(terminal.right(), toolbar.right() - px(4.));
    for item in ToolbarItem::DEFAULT_ORDER
        .into_iter()
        .filter(|item| item.region() == ToolbarRegion::ContentCenter)
    {
        let bounds = cx.debug_bounds(formatting_item(item).0).unwrap();
        assert!(bounds.top() >= back.bottom() + px(4.));
        assert!(bounds.bottom() <= toolbar.bottom() - px(4.));
        assert_eq!(bounds.size, gpui::size(px(40.), px(40.)));
    }
}

#[gpui::test]
fn right_panel_explicit_narrow_resize_is_retained(cx: &mut TestAppContext) {
    let (_dir, notes, cx) = fixture(cx);
    cx.simulate_resize(gpui::size(px(400.), px(800.)));
    click("toolbar-terminal", cx);
    settle_layout(cx);
    let panel = cx.debug_bounds("right-panel").unwrap();
    assert_eq!(panel.size.width, px(200.));
    cx.update(|_, cx| assert_eq!(notes.read(cx).right_panel_width, 280.));
    let start = gpui::point(panel.left(), panel.center().y);
    cx.simulate_mouse_move(start, None, Modifiers::default());
    cx.simulate_mouse_down(start, gpui::MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(
        gpui::point(start.x + px(12.), start.y),
        gpui::MouseButton::Left,
        Modifiers::default(),
    );
    settle_layout(cx);
    let end = gpui::point(start.x + px(20.), start.y);
    cx.simulate_mouse_move(end, gpui::MouseButton::Left, Modifiers::default());
    settle_layout(cx);
    cx.simulate_mouse_up(end, gpui::MouseButton::Left, Modifiers::default());
    settle_layout(cx);
    cx.update(|_, cx| assert_eq!(notes.read(cx).right_panel_width, 180.));
    click("toolbar-terminal", cx);
    click("toolbar-terminal", cx);
    settle_layout(cx);
    assert_eq!(cx.debug_bounds("right-panel").unwrap().size.width, px(180.));
}

#[gpui::test]
fn terminal_is_lazy_retained_and_consumes_shell_shortcuts(cx: &mut TestAppContext) {
    cx.update(|cx| {
        cx.bind_keys(
            crate::help_ui::app_shortcuts()
                .into_iter()
                .map(|s| s.binding),
        )
    });
    let (_dir, notes, cx) = fixture(cx);
    cx.update(|_, cx| assert!(notes.read(cx).terminals.tabs.is_empty()));
    cx.simulate_keystrokes("ctrl-`");
    cx.run_until_parked();
    let (panel, view) = cx.update(|_, cx| {
        let panel = notes.read(cx).terminals.active_session().cloned().unwrap();
        let view = panel.read(cx).view.clone().unwrap();
        (panel, view)
    });
    cx.simulate_keystrokes("ctrl-c ctrl-s ctrl-w ctrl-b up f5");
    cx.run_until_parked();
    cx.update(|_, cx| {
        let this = notes.read(cx);
        assert_eq!(this.editor.text(cx).as_ref(), "café\n茶");
        assert_eq!(this.tabs.ids().len(), 1);
        assert!(this.right_panel_open);
        let input = panel.read(cx).fixture_input.lock().unwrap().clone();
        assert_eq!(input, b"\x03\x13\x17\x02\x1b[A\x1b[15~");
    });
    cx.simulate_keystrokes("ctrl-`");
    cx.run_until_parked();
    cx.update(|window, cx| {
        assert!(!notes.read(cx).right_panel_open);
        assert!(notes.read(cx).editor.is_focused(window, cx));
    });
    cx.simulate_keystrokes("ctrl-`");
    cx.run_until_parked();
    cx.update(|window, cx| {
        assert_eq!(notes.read(cx).terminals.active_session(), Some(&panel));
        assert_eq!(panel.read(cx).view, Some(view.clone()));
        assert!(view.read(cx).focus_handle().is_focused(window));
    });
}

#[gpui::test]
fn terminal_new_tab_uses_current_notebook_and_preserves_existing_session(cx: &mut TestAppContext) {
    let (dir, notes, cx) = fixture(cx);
    let first = notebook::Notebook::open(&dir.path().join("terminal-first"), true).unwrap();
    let mut second = notebook::Notebook::open(&dir.path().join("terminal-second"), true).unwrap();
    second.settings.terminal.windows_shell = mithril::terminal::WindowsShell::GitBash;
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.notebooks.active = Some(first.clone());
            this.toggle_right_panel(window, cx);
        })
    });
    settle_layout(cx);
    let (panel, view) = cx.update(|_, cx| {
        let panel = notes.read(cx).terminals.active_session().cloned().unwrap();
        (panel.clone(), panel.read(cx).view.clone().unwrap())
    });
    cx.update(|_, cx| {
        notes.update(cx, |this, cx| {
            this.notebooks.active = Some(second.clone());
            cx.notify();
        })
    });
    cx.update(|_, cx| {
        let launch = panel.read(cx).last_launch.as_ref().unwrap();
        assert_eq!(
            launch.0.windows_shell,
            mithril::terminal::WindowsShell::Powershell
        );
        assert_eq!(launch.1.as_ref().unwrap(), &first.root);

        assert_eq!(panel.read(cx).view, Some(view.clone()));
    });
    cx.refresh().unwrap();
    click("terminal-tab-add", cx);
    cx.update(|_, cx| {
        let next = notes.read(cx).terminals.active_session().unwrap();
        assert_ne!(next, &panel);
        assert_eq!(panel.read(cx).view, Some(view.clone()));
        let launch = next.read(cx).last_launch.as_ref().unwrap();
        assert_eq!(
            launch.0.windows_shell,
            mithril::terminal::WindowsShell::GitBash
        );
        assert_eq!(launch.1.as_ref().unwrap(), &second.root);
        assert_ne!(next.read(cx).view, Some(view));
    });
}

#[gpui::test]
fn terminal_input_rechecks_modal_authority(cx: &mut TestAppContext) {
    let (_dir, notes, cx) = fixture(cx);
    click("toolbar-terminal", cx);
    let panel = cx.update(|_, cx| notes.read(cx).terminals.active_session().cloned().unwrap());
    cx.update(|window, cx| {
        window.open_dialog(cx, |dialog, _, _| dialog.title("Test modal"));
        // Force terminal focus under the modal to test the input-time guard,
        // rather than relying only on the dialog's normal focus handling.
        panel.read(cx).focus(window, cx);
    });
    cx.simulate_keystrokes("x ctrl-c tab shift-tab shift-enter");
    cx.run_until_parked();
    cx.update(|_, cx| assert!(panel.read(cx).fixture_input.lock().unwrap().is_empty()));
}

#[gpui::test]
fn terminal_tab_completion_keeps_focus_and_reaches_the_shell(cx: &mut TestAppContext) {
    let (_dir, notes, cx) = fixture(cx);
    click("toolbar-terminal", cx);
    let (panel, view) = cx.update(|_, cx| {
        let panel = notes.read(cx).terminals.active_session().cloned().unwrap();
        let view = panel.read(cx).view.clone().unwrap();
        (panel, view)
    });
    for (key, bytes) in [
        ("tab", b"\t".as_slice()),
        ("shift-tab", b"\x1b[Z".as_slice()),
    ] {
        cx.update(|window, cx| {
            view.read(cx).focus_handle().focus(window);
            panel.read(cx).fixture_input.lock().unwrap().clear();
        });
        cx.refresh().unwrap();
        cx.simulate_keystrokes(key);
        cx.run_until_parked();
        cx.update(|window, cx| {
            assert_eq!(*panel.read(cx).fixture_input.lock().unwrap(), bytes);
            assert!(view.read(cx).focus_handle().is_focused(window));
            assert_eq!(notes.read(cx).editor.text(cx).as_ref(), "café\n茶");
        });
    }
    cx.update(|_, cx| {
        view.update(cx, |view, _| view.set_input_enabled(false));
        panel.read(cx).fixture_input.lock().unwrap().clear();
    });
    cx.simulate_keystrokes("tab shift-tab shift-enter");
    cx.run_until_parked();
    cx.update(|window, cx| {
        assert!(panel.read(cx).fixture_input.lock().unwrap().is_empty());
        assert!(view.read(cx).focus_handle().is_focused(window));
        notes.read(cx).focus.focus(window);
    });
    cx.refresh().unwrap();
    cx.simulate_keystrokes("tab");
    cx.run_until_parked();
    cx.update(|window, cx| {
        assert!(
            !notes.read(cx).focus.is_focused(window),
            "Tab outside the terminal still navigates focus"
        );
        assert!(panel.read(cx).fixture_input.lock().unwrap().is_empty());
    });
}

#[gpui::test]
fn terminal_shift_enter_is_distinct_from_submit(cx: &mut TestAppContext) {
    let (_dir, notes, cx) = fixture(cx);
    click("toolbar-terminal", cx);
    let (panel, view) = cx.update(|_, cx| {
        let panel = notes.read(cx).terminals.active_session().cloned().unwrap();
        let view = panel.read(cx).view.clone().unwrap();
        (panel, view)
    });
    cx.simulate_keystrokes("shift-enter enter ctrl-j tab shift-tab");
    cx.run_until_parked();
    cx.update(|window, cx| {
        assert_eq!(
            *panel.read(cx).fixture_input.lock().unwrap(),
            b"\x1b[13;2u\r\n\t\x1b[Z"
        );
        assert!(view.read(cx).focus_handle().is_focused(window));
        assert_eq!(notes.read(cx).editor.text(cx).as_ref(), "café\n茶");
    });
}

#[gpui::test]
fn terminal_tabs_select_close_neighbors_and_reopen_with_stable_ids(cx: &mut TestAppContext) {
    use crate::terminal_ui::TerminalTabId;
    let (_dir, notes, cx) = fixture(cx);
    cx.simulate_resize(gpui::size(px(1600.), px(800.)));
    cx.update(|_, cx| {
        notes.update(cx, |this, cx| {
            this.right_panel_width = 600.;
            cx.notify();
        })
    });
    settle_layout(cx);
    click("toolbar-terminal", cx);
    for selector in ["terminal-restart", "terminal-stop", "right-panel-close"] {
        assert!(cx.debug_bounds(selector).is_none());
    }
    let first = cx.update(|_, cx| notes.read(cx).terminals.active_session().cloned().unwrap());
    cx.simulate_keystrokes("a");
    click("terminal-tab-add", cx);
    let second = cx.update(|_, cx| notes.read(cx).terminals.active_session().cloned().unwrap());
    cx.simulate_keystrokes("b");
    cx.update(|_, cx| {
        assert_ne!(first, second);
        assert_eq!(*first.read(cx).fixture_input.lock().unwrap(), b"a");
        assert_eq!(*second.read(cx).fixture_input.lock().unwrap(), b"b");
        let tabs = &notes.read(cx).terminals.tabs;
        let label = crate::terminal_ui::shell_label(Default::default());
        assert_eq!(tabs[0].title, label);
        assert_eq!(tabs[1].title, label);
    });
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.select_terminal_tab(TerminalTabId(1), window, cx)
        })
    });
    settle_layout(cx);
    cx.simulate_keystrokes("c");
    cx.update(|_, cx| assert_eq!(*first.read(cx).fixture_input.lock().unwrap(), b"ac"));
    // Closing an inactive tab preserves both selection and terminal focus.
    click("terminal-tab-close-2", cx);
    cx.update(|window, cx| {
        assert_eq!(notes.read(cx).terminals.active, Some(TerminalTabId(1)));
        assert!(
            first
                .read(cx)
                .view
                .as_ref()
                .unwrap()
                .read(cx)
                .focus_handle()
                .is_focused(window)
        );
    });
    click("terminal-tab-add", cx);
    click("terminal-tab-add", cx);
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.select_terminal_tab(TerminalTabId(3), window, cx)
        })
    });
    settle_layout(cx);
    click("terminal-tab-close-3", cx);
    cx.update(|_, cx| assert_eq!(notes.read(cx).terminals.active, Some(TerminalTabId(4))));
    click("terminal-tab-close-4", cx);
    cx.update(|_, cx| assert_eq!(notes.read(cx).terminals.active, Some(TerminalTabId(1))));
    click("terminal-tab-close-1", cx);
    cx.update(|window, cx| {
        let notes = notes.read(cx);
        assert!(notes.terminals.tabs.is_empty());
        assert!(notes.terminals.active.is_none());
        assert!(!notes.right_panel_open);
        assert!(notes.editor.is_focused(window, cx));
    });
    cx.update(|_, cx| assert!(notes.read(cx).terminals.cleanup.is_empty()));
    click("toolbar-terminal", cx);
    cx.update(|_, cx| {
        assert_eq!(notes.read(cx).terminals.active, Some(TerminalTabId(5)));
        assert_ne!(notes.read(cx).terminals.active_session(), Some(&first));
    });
}

#[gpui::test]
fn terminal_tabs_overflow_reveals_selection_and_keeps_add_visible(cx: &mut TestAppContext) {
    use crate::terminal_ui::TerminalTabId;
    let (_dir, notes, cx) = fixture(cx);
    click("toolbar-terminal", cx);
    settle_layout(cx);
    let tab = cx.debug_bounds("terminal-tab-1").unwrap();
    let add = cx.debug_bounds("terminal-tab-add").unwrap();
    assert!((add.left() - tab.right() - px(4.)).abs() <= px(1.));
    let document_id = cx.update(|_, cx| notes.read(cx).tabs.active.id.0);
    let document_selector = Box::leak(format!("tab-select-{document_id}").into_boxed_str());
    assert_eq!(
        cx.debug_bounds(document_selector).unwrap().size.height,
        cx.debug_bounds("terminal-tab-select-1")
            .unwrap()
            .size
            .height
    );
    for _ in 0..7 {
        click("terminal-tab-add", cx);
    }
    settle_layout(cx);
    let assert_visible = |selector: &'static str, cx: &mut VisualTestContext| {
        let strip = cx.debug_bounds("terminal-tabs").unwrap();
        let tab = cx.debug_bounds(selector).unwrap();
        let add = cx.debug_bounds("terminal-tab-add").unwrap();
        let panel = cx.debug_bounds("right-panel").unwrap();
        assert!(tab.left() >= strip.left(), "tab starts outside strip");
        assert!(tab.right() <= strip.right(), "tab ends outside strip");
        assert!(add.left() >= strip.right());
        assert!(add.right() <= panel.right());
    };
    assert_visible("terminal-tab-8", cx);
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.select_terminal_tab(TerminalTabId(1), window, cx)
        })
    });
    settle_layout(cx);
    assert_visible("terminal-tab-1", cx);
    for (size, height) in [
        (ToolbarIconSize::Small, 33.),
        (ToolbarIconSize::Medium, 41.),
        (ToolbarIconSize::Large, 49.),
    ] {
        cx.update(|_, cx| {
            notes.update(cx, |this, cx| {
                this.session_toolbar.icon_size = size;
                cx.notify();
            })
        });
        settle_layout(cx);
        assert_eq!(
            cx.debug_bounds("terminal-header").unwrap().size.height,
            px(height)
        );
    }
}

#[gpui::test]
fn terminal_tab_actions_recheck_modal_and_busy_guards(cx: &mut TestAppContext) {
    use crate::terminal_ui::TerminalTabId;
    let (_dir, notes, cx) = fixture(cx);
    click("toolbar-terminal", cx);
    click("terminal-tab-add", cx);
    cx.update(|window, cx| {
        window.open_dialog(cx, |dialog, _, _| dialog.title("Blocked"));
        notes.update(cx, |this, cx| {
            this.new_terminal_tab(window, cx);
            this.select_terminal_tab(TerminalTabId(1), window, cx);
            this.close_terminal_tab(TerminalTabId(2), window, cx);
            assert_eq!(this.terminals.tabs.len(), 2);
            assert_eq!(this.terminals.active, Some(TerminalTabId(2)));
        });
        window.close_dialog(cx);
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            let _ = this.session.pick();
            this.new_terminal_tab(window, cx);
            this.select_terminal_tab(TerminalTabId(1), window, cx);
            this.close_terminal_tab(TerminalTabId(2), window, cx);
            assert_eq!(this.terminals.tabs.len(), 2);
            assert_eq!(this.terminals.active, Some(TerminalTabId(2)));
        })
    });
}

#[gpui::test]
fn terminal_tabs_survive_cancelled_document_window_close(cx: &mut TestAppContext) {
    let (_dir, notes, cx) = fixture(cx);
    click("toolbar-bold", cx);
    click("toolbar-terminal", cx);
    click("terminal-tab-add", cx);
    let sessions = cx.update(|_, cx| {
        notes
            .read(cx)
            .terminals
            .tabs
            .iter()
            .map(|tab| tab.session.clone())
            .collect::<Vec<_>>()
    });
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| this.request(crate::Next::Close, window, cx))
    });
    cx.run_until_parked();
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer("Cancel");
    settle_layout(cx);
    cx.update(|_, cx| {
        let notes = notes.read(cx);
        assert_eq!(notes.terminals.tabs.len(), 2);
        assert!(notes.terminals.cleanup.is_empty());
        for (tab, session) in notes.terminals.tabs.iter().zip(sessions) {
            assert_eq!(tab.session, session);
            assert_eq!(session.read(cx).status, crate::terminal_ui::Status::Running);
        }
    });
}
