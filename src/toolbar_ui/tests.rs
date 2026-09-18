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
        for item in ToolbarItem::DEFAULT_ORDER.into_iter().skip(6) {
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
    for item in ToolbarItem::DEFAULT_ORDER.into_iter().skip(6) {
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
