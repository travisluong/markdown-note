use super::*;
use gpui::ClipboardItem;
use gpui_component::input::Undo;
use markdown_note::markdown_edit::{self, Format};

fn select(notes: &Entity<Notes>, cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.editor.select(0..5, window, cx);
        })
    });
    cx.refresh().unwrap();
}

fn choose(row: usize, cx: &mut VisualTestContext) {
    click(MenuKind::Edit, cx);
    let keys = std::iter::repeat_n("down", row)
        .collect::<Vec<_>>()
        .join(" ");
    cx.simulate_keystrokes(&keys);
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    cx.refresh().unwrap();
}

#[gpui::test]
fn edit_clipboard_selection_and_history_preserve_document_focus(cx: &mut TestAppContext) {
    let (_dir, notes, cx) = fixture(cx);
    select(&notes, cx);
    choose(4, cx); // Copy
    cx.update(|_, cx| assert_eq!(cx.read_from_clipboard().unwrap().text().unwrap(), "title"));
    choose(3, cx); // Cut
    cx.update(|window, cx| {
        let this = notes.read(cx);
        assert_eq!(this.editor.text(cx).as_ref(), " bar test");
        assert!(this.dirty(cx));
        assert!(this.editor.is_focused(window, cx));
    });
    choose(1, cx); // Undo
    cx.update(|_, cx| assert!(!notes.read(cx).dirty(cx)));
    choose(2, cx); // Redo
    cx.update(|_, cx| assert_eq!(notes.read(cx).editor.text(cx).as_ref(), " bar test"));
    choose(1, cx);
    select(&notes, cx);
    cx.update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string("new".into())));
    choose(5, cx); // Paste
    cx.update(|_, cx| assert_eq!(notes.read(cx).editor.text(cx).as_ref(), "new bar test"));
    choose(6, cx); // Select all
    cx.update(|_, cx| assert_eq!(notes.read(cx).editor.selection(cx), 0..12));
    choose(1, cx);
    cx.update(|_, cx| assert!(!notes.read(cx).dirty(cx)));
}

#[gpui::test]
fn edit_format_submenus_route_all_formats_and_share_undo(cx: &mut TestAppContext) {
    let (_dir, notes, cx) = fixture(cx);
    for mode in [ViewMode::Source, ViewMode::LivePreview] {
        cx.update(|window, cx| {
            notes.update(cx, |this, cx| this.change_view_mode(mode, window, cx))
        });
        for (keys, format) in [
            ("enter", Format::Bold),
            ("down enter", Format::Italic),
            ("down down enter", Format::Strikethrough),
            ("down down down enter", Format::InlineCode),
            ("down down down down right enter", Format::Heading(1)),
            ("down down down down right down enter", Format::Heading(2)),
            (
                "down down down down right down down enter",
                Format::Heading(3),
            ),
            (
                "down down down down right down down down enter",
                Format::Heading(4),
            ),
            (
                "down down down down right down down down down enter",
                Format::Heading(5),
            ),
            (
                "down down down down right down down down down down enter",
                Format::Heading(6),
            ),
            (
                "down down down down right down down down down down down enter",
                Format::Heading(0),
            ),
            ("down down down down down right enter", Format::OrderedList),
            (
                "down down down down down right down enter",
                Format::UnorderedList,
            ),
            (
                "down down down down down right down down enter",
                Format::TaskList,
            ),
            ("down down down down down down enter", Format::Blockquote),
            (
                "down down down down down down down enter",
                Format::CodeBlock,
            ),
        ] {
            select(&notes, cx);
            click(MenuKind::Edit, cx);
            cx.simulate_keystrokes("down down down down down down down right");
            cx.refresh().unwrap();
            cx.simulate_keystrokes(keys);
            cx.run_until_parked();
            let edit = markdown_edit::transform("title bar test", 0..5, format).unwrap();
            let mut expected = "title bar test".to_owned();
            expected.replace_range(edit.range, &edit.text);
            cx.update(|window, cx| {
                let this = notes.read(cx);
                assert_eq!(
                    this.editor.text(cx).as_ref(),
                    expected,
                    "{mode:?} {format:?}"
                );
                assert!(this.menu.is_none(), "{format:?}");
                assert!(this.editor.is_focused(window, cx), "{format:?}");
            });
            // Paragraph on a plain paragraph is a no-op, so has nothing to undo.
            if expected != "title bar test" {
                cx.dispatch_action(Undo);
                cx.run_until_parked();
            }
            cx.update(|_, cx| assert!(!notes.read(cx).dirty(cx), "{format:?}"));
        }
    }
}

#[gpui::test]
fn edit_date_tasks_and_keyboard_entry_use_existing_commands(cx: &mut TestAppContext) {
    cx.update(|cx| {
        cx.bind_keys(
            crate::help_ui::app_shortcuts()
                .into_iter()
                .map(|s| s.binding),
        )
    });
    let (_dir, notes, cx) = fixture(cx);
    select(&notes, cx);
    cx.simulate_keystrokes("alt-e");
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(notes.read(cx).menu.as_ref().unwrap().0, MenuKind::Edit));
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    choose(8, cx);
    cx.update(|window, cx| {
        let this = notes.read(cx);
        assert_ne!(this.editor.text(cx).as_ref(), "title bar test");
        assert!(this.editor.text(cx).ends_with(" bar test"));
        assert!(this.editor.is_focused(window, cx));
    });
    choose(1, cx);
    select(&notes, cx);
    choose(9, cx);
    cx.update(|_, cx| {
        assert_eq!(
            notes.read(cx).editor.text(cx).as_ref(),
            "- [ ] title bar test"
        )
    });
    choose(1, cx);
    cx.update(|_, cx| assert!(!notes.read(cx).dirty(cx)));
}

#[gpui::test]
fn edit_menu_rejects_stale_selection_document_and_reading_changes(cx: &mut TestAppContext) {
    let (dir, notes, cx) = fixture(cx);
    for change in 0..5 {
        cx.update(|window, cx| {
            notes.update(cx, |this, cx| {
                this.install_document(
                    Some(Document::open(dir.path().join("note.md")).unwrap()),
                    window,
                    cx,
                );
                this.change_view_mode(ViewMode::Source, window, cx);
                this.editor.select(0..5, window, cx);
            })
        });
        cx.refresh().unwrap();
        click(MenuKind::Edit, cx);
        cx.simulate_keystrokes("down down down down down down down right");
        cx.update(|window, cx| {
            notes.update(cx, |this, cx| {
                match change {
                    0 => {
                        this.editor.reveal_match(6..9, cx);
                    }
                    1 => this.editor.replace_selection("changed", window, cx),
                    2 => this.change_view_mode(ViewMode::Reading, window, cx),
                    3 => this.install_document(None, window, cx),
                    _ => this.install_document(
                        Some(Document::open(dir.path().join("note.md")).unwrap()),
                        window,
                        cx,
                    ),
                }
                if let Some((_, menu)) = &this.menu {
                    menu.focus_handle(cx).focus(window);
                }
            })
        });
        let before = cx.update(|_, cx| notes.read(cx).editor.text(cx));
        // Re-enter the retained Format submenu and try Bold.
        cx.simulate_keystrokes("right enter");
        cx.run_until_parked();
        cx.update(|_, cx| assert_eq!(notes.read(cx).editor.text(cx), before));
        cx.update(|window, cx| notes.update(cx, |this, cx| this.dismiss_title_menu(window, cx)));
    }
}

#[gpui::test]
fn edit_reading_copy_preserves_rendered_selection_and_disables_changes(cx: &mut TestAppContext) {
    let (_dir, notes, cx) = fixture(cx);
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.editor.select(0..14, window, cx);
            this.editor
                .replace_selection("**title** bar test", window, cx);
            this.change_view_mode(ViewMode::Reading, window, cx);
        })
    });
    cx.run_until_parked();
    cx.refresh().unwrap();
    cx.run_until_parked();
    cx.refresh().unwrap();
    let bounds = cx.debug_bounds("reading-content").unwrap();
    let start = gpui::point(bounds.left() + px(24.), bounds.top() + px(34.));
    let end = gpui::point(bounds.right() - px(24.), start.y);
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(end, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::default());
    cx.run_until_parked();
    cx.refresh().unwrap();
    let probe = cx.update(|_, cx| {
        notes
            .read(cx)
            .reading_view
            .as_ref()
            .unwrap()
            .diagnostic_probe()
    });
    cx.update(|_, cx| {
        assert_eq!(
            probe
                .snapshot(cx)
                .unwrap()
                .displayed_source
                .as_ref()
                .map(|text| text.as_ref()),
            Some("**title** bar test")
        )
    });
    let selection = cx.update(|_, cx| probe.selection(cx));
    assert_ne!(selection.0, selection.1);
    choose(2, cx); // Initial Down selects Undo; the next skips disabled Redo/Cut to Copy.
    cx.update(|window, cx| {
        assert!(notes.read(cx).menu.is_none());
        assert!(probe.is_focused(window, cx));
        assert_eq!(probe.selection(cx), selection);
    });
    cx.update(|window, cx| {
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("title bar test")
        );
        assert_eq!(probe.selection(cx), selection);
        assert!(probe.is_focused(window, cx));
    });
    click(MenuKind::Edit, cx);
    cx.simulate_keystrokes("down down down right enter"); // Format > disabled Bold
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(
            notes.read(cx).editor.text(cx).as_ref(),
            "**title** bar test"
        )
    });
}

#[gpui::test]
fn edit_link_dialog_keeps_its_focus_after_nested_menu_dismissal(cx: &mut TestAppContext) {
    let (_dir, notes, cx) = fixture(cx);
    select(&notes, cx);
    click(MenuKind::Edit, cx);
    cx.simulate_keystrokes("down down down down down down down right");
    cx.simulate_keystrokes("down down down down down down down down enter");
    cx.run_until_parked();
    cx.update(|window, cx| {
        assert!(notes.read(cx).menu.is_none());
        assert!(window.has_active_dialog(cx));
        assert!(!notes.read(cx).editor.is_focused(window, cx));
    });
}

#[gpui::test]
fn empty_edit_menu_escape_restores_visible_application_focus(cx: &mut TestAppContext) {
    let (_dir, notes, cx) = fixture(cx);
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.install_document(None, window, cx);
            this.focus.focus(window);
        })
    });
    cx.refresh().unwrap();
    click(MenuKind::Edit, cx);
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    cx.update(|window, cx| assert!(notes.read(cx).focus.is_focused(window)));
}
