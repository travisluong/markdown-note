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
        let toolbar = cx.debug_bounds("window-toolbar").unwrap();
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
    // Mode entries stay first; Back follows them and Forward is disabled at the end.
    cx.refresh().unwrap();
    click("view-menu", cx);
    cx.simulate_keystrokes("down down down down enter");
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(notes.read(cx).editor.text(cx).as_ref(), "café\n茶"));
    cx.refresh().unwrap();
    click("view-menu", cx);
    // Back is now disabled, so the fourth enabled item is Forward.
    cx.simulate_keystrokes("down down down down enter");
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
