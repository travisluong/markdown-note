//! Palette adapter contracts using TestPlatform, without native UI/input.
use super::{Command, PaletteEntry};
use crate::{Document, Notes, ViewMode, notebook};
use gpui::{Entity, Focusable, TestAppContext, VisualTestContext, prelude::*};
use gpui_component::{
    Root, WindowExt,
    input::{Redo, Undo},
};
use std::{fs, path::Path};

fn fixture<'a>(
    cx: &'a mut TestAppContext,
    root: &Path,
) -> (Entity<Notes>, &'a mut VisualTestContext) {
    cx.update(gpui_component::init);
    let mut notes = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| {
            let mut notes = Notes::new(window, cx);
            let mut book = notebook::Notebook::open(root, false).unwrap();
            book.settings.autosave.enabled = false;
            notes.notebooks.active = Some(book);
            notes
        });
        entity.update(cx, |this, cx| {
            this.install_document(
                Some(Document::open(root.join("note.md")).unwrap()),
                window,
                cx,
            )
        });
        notes = Some(entity.clone());
        let view = cx.new(|cx| crate::NotesWindow {
            _subscription: cx.observe(&entity, |_, _, cx| cx.notify()),
            notes: entity,
        });
        Root::new(view, window, cx)
    });
    cx.refresh().unwrap();
    cx.run_until_parked();
    (notes.unwrap(), cx)
}

#[gpui::test]
fn palette_overlays_preserve_colors_and_outside_click_focus(cx: &mut TestAppContext) {
    use super::PaletteMode;
    use gpui::{Modifiers, MouseButton, point, px, transparent_black};
    use gpui_component::{ActiveTheme, Theme, ThemeMode, dialog::Dialog};

    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("note.md"), "text").unwrap();
    let (notes, cx) = fixture(cx, directory.path());
    for theme in [ThemeMode::Light, ThemeMode::Dark] {
        cx.update(|window, cx| Theme::change(theme, Some(window), cx));
        for mode in [
            PaletteMode::Commands,
            PaletteMode::Files,
            PaletteMode::Notebooks,
            PaletteMode::Sort,
        ] {
            cx.update(|window, cx| {
                let default_dialog = Dialog::new(window, cx);
                assert_eq!(
                    default_dialog.diagnostic_overlay_color(cx),
                    cx.theme().overlay
                );
                assert_eq!(
                    default_dialog.overlay(false).diagnostic_overlay_color(cx),
                    transparent_black()
                );
                let background = cx.theme().background;
                let overlay = cx.theme().overlay;
                notes.update(cx, |this, cx| {
                    this.editor.focus(window, cx);
                    this.open_palette(mode, Vec::new(), None, window, cx);
                    let dialog = this.palette_dialog(Dialog::new(window, cx), window, cx);
                    assert_eq!(dialog.diagnostic_overlay_color(cx), transparent_black());
                    assert!(window.has_active_dialog(cx));
                    assert!(
                        this.palette
                            .as_ref()
                            .unwrap()
                            .query
                            .focus_handle(cx)
                            .is_focused(window)
                    );
                });
                assert_eq!(cx.theme().background, background);
                assert_eq!(cx.theme().overlay, overlay);
            });
            cx.run_until_parked();
            cx.refresh().unwrap();
            // The transparent backdrop still captures non-dismissing mouse presses.
            cx.simulate_mouse_down(
                point(px(1.), px(1.)),
                MouseButton::Right,
                Modifiers::default(),
            );
            cx.update(|window, cx| {
                assert!(window.has_active_dialog(cx));
                assert!(
                    notes
                        .read(cx)
                        .palette
                        .as_ref()
                        .unwrap()
                        .query
                        .focus_handle(cx)
                        .is_focused(window)
                );
            });
            // A left press outside the surface dismisses and restores editor focus.
            cx.simulate_click(point(px(1.), px(1.)), Modifiers::default());
            cx.run_until_parked();
            cx.update(|window, cx| {
                assert!(!window.has_active_dialog(cx));
                assert!(notes.read(cx).palette.is_none());
                assert!(notes.read(cx).editor.is_focused(window, cx));
            });
        }
    }
}

#[gpui::test]
fn palette_shortcut_badges_preserve_focus_and_fit_narrow_rows(cx: &mut TestAppContext) {
    use gpui::{px, size};

    cx.update(|cx| {
        cx.bind_keys(
            crate::help_ui::app_shortcuts()
                .into_iter()
                .map(|s| s.binding),
        )
    });
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("note.md"), "text").unwrap();
    let (notes, cx) = fixture(cx, directory.path());
    cx.update(|window, cx| notes.update(cx, |this, cx| this.command_palette(window, cx)));
    for width in [1000., 360.] {
        cx.simulate_resize(size(px(width), px(700.)));
        for command in [Command::Bold, Command::SearchAllFiles, Command::InsertDate] {
            cx.update(|window, cx| {
                notes.update(cx, |this, cx| {
                    let palette = this.palette.as_ref().unwrap();
                    palette
                        .query
                        .update(cx, |query, cx| query.set_value(command.label(), window, cx));
                });
            });
            cx.run_until_parked();
            cx.refresh().unwrap();
            cx.executor()
                .advance_clock(std::time::Duration::from_millis(300));
            cx.refresh().unwrap();
            cx.run_until_parked();
            cx.update(|window, cx| {
                let palette = notes.read(cx).palette.as_ref().unwrap();
                assert!(palette.query.focus_handle(cx).is_focused(window));
                assert!(crate::help_ui::command_kbd(command, window).is_some());
            });
            let row = cx.debug_bounds("palette-row-0").unwrap();
            let text = cx.debug_bounds("palette-text-0").unwrap();
            let badge = cx.debug_bounds("palette-shortcut-0").unwrap();
            assert_eq!(row.size.height, px(48.));
            assert!(text.size.width > px(0.));
            assert!(badge.size.width > px(0.));
            assert!(text.right() < badge.left(), "{text:?} {badge:?}");
            assert!(badge.right() <= row.right() && row.right() <= px(width));
            assert!(badge.top() >= row.top() && badge.bottom() <= row.bottom());
            assert!((badge.center().y - row.center().y).abs() <= px(1.));
        }
    }
    // Reading disables editing but keeps the shortcut discoverable.
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.set_mode(ViewMode::Reading, window, cx);
            this.palette
                .as_ref()
                .unwrap()
                .query
                .update(cx, |query, cx| {
                    query.set_value(Command::Bold.label(), window, cx);
                });
        });
    });
    cx.run_until_parked();
    cx.refresh().unwrap();
    assert!(cx.debug_bounds("palette-shortcut-0").is_some());
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            assert!(this.command_unavailable(Command::Bold).is_some());
            this.activate_palette(window, cx);
            assert!(this.palette.is_some());
            assert_eq!(this.editor.text(cx).as_ref(), "text");
        });
    });
    for command in [
        Command::OpenFile,
        Command::SourceMode,
        Command::LivePreview,
        Command::ReadingView,
        Command::TaskList,
    ] {
        cx.update(|window, cx| {
            notes.update(cx, |this, cx| {
                this.palette
                    .as_ref()
                    .unwrap()
                    .query
                    .update(cx, |query, cx| {
                        query.set_value(command.label(), window, cx);
                    });
                assert!(crate::help_ui::command_kbd(command, window).is_none());
            });
        });
        cx.run_until_parked();
        cx.refresh().unwrap();
        // GPUI retains old debug selectors, so verify that the text now occupies
        // the entire padded row rather than checking a stale badge selector.
        let row = cx.debug_bounds("palette-row-0").unwrap();
        let text = cx.debug_bounds("palette-text-0").unwrap();
        assert_eq!(
            row.right() - text.right(),
            text.left() - row.left(),
            "{command:?}"
        );
    }
}

#[gpui::test]
fn view_mode_palette_and_cycle_preserve_document_selection_and_history(cx: &mut TestAppContext) {
    cx.update(|cx| {
        cx.bind_keys(
            crate::help_ui::app_shortcuts()
                .into_iter()
                .map(|s| s.binding),
        )
    });
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("note.md"), "café").unwrap();
    let (notes, cx) = fixture(cx, directory.path());
    let (editor, tab) = cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.editor.select(0..5, window, cx);
            this.editor.replace_selection("draft 茶", window, cx);
            this.editor.select(6..9, window, cx);
            (this.editor.id(), this.tabs.active.id)
        })
    });
    for (command, mode) in [
        (Command::LivePreview, ViewMode::LivePreview),
        (Command::ReadingView, ViewMode::Reading),
        (Command::SourceMode, ViewMode::Source),
    ] {
        cx.update(|window, cx| {
            notes.update(cx, |this, cx| {
                this.command_palette(window, cx);
                this.palette
                    .as_ref()
                    .unwrap()
                    .query
                    .update(cx, |query, cx| {
                        query.set_value(command.label(), window, cx);
                    });
            })
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            notes.update(cx, |this, cx| {
                assert!(
                    matches!(this.palette.as_ref().unwrap().model.selected_entry(),
                Some(PaletteEntry::Command(selected)) if *selected == command)
                );
                this.activate_palette(window, cx);
                assert!(this.palette.is_none());
                assert_eq!(this.mode, mode);
                assert_eq!(
                    this.notebooks.active.as_ref().unwrap().settings.view_mode,
                    mode
                );
                assert_eq!(this.editor.id(), editor);
                assert_eq!(this.tabs.active.id, tab);
                assert_eq!(this.editor.text(cx).as_ref(), "draft 茶");
                assert_eq!(this.editor.selection(cx), 6..9);
                assert!(this.dirty(cx));
            })
        });
        cx.run_until_parked();
    }
    // Standalone documents have the same cycle, including leaving Reading.
    cx.update(|_, cx| notes.update(cx, |this, _| this.notebooks.active = None));
    let key = if cfg!(target_os = "macos") {
        "cmd-shift-m"
    } else {
        "ctrl-shift-m"
    };
    for mode in [ViewMode::LivePreview, ViewMode::Reading, ViewMode::Source] {
        cx.refresh().unwrap();
        cx.simulate_keystrokes(key);
        cx.run_until_parked();
        cx.update(|_, cx| {
            let this = notes.read(cx);
            assert_eq!(this.mode, mode);
            assert_eq!(this.editor.id(), editor);
            assert_eq!(this.editor.selection(cx), 6..9);
            assert_eq!(this.editor.text(cx).as_ref(), "draft 茶");
        });
    }
    cx.dispatch_action(Undo);
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(notes.read(cx).editor.text(cx).as_ref(), "café"));
    cx.dispatch_action(Redo);
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(notes.read(cx).editor.text(cx).as_ref(), "draft 茶"));
}

#[gpui::test]
fn view_mode_actions_reject_dialogs_and_busy_sessions(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("note.md"), "original").unwrap();
    let (notes, cx) = fixture(cx, directory.path());
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.set_mode(ViewMode::Source, window, cx);
            this.open_help(false, window, cx);
            assert!(window.has_active_dialog(cx));
            this.run_command(Command::ReadingView, window, cx);
            this.cycle_view_mode(window, cx);
            this.change_view_mode(ViewMode::LivePreview, window, cx);
            assert_eq!(this.mode, ViewMode::Source);
            window.close_dialog(cx);
            this.editor.replace_selection(" draft", window, cx);
        })
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            assert!(this.dirty(cx));
            this.request(
                crate::Next::File(directory.path().join("next.md")),
                window,
                cx,
            );
        })
    });
    cx.run_until_parked();
    assert!(cx.has_pending_prompt());
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            assert!(this.document_interaction_blocked());
            this.run_command(Command::ReadingView, window, cx);
            this.cycle_view_mode(window, cx);
            this.change_view_mode(ViewMode::LivePreview, window, cx);
            assert_eq!(this.mode, ViewMode::Source);
            assert_eq!(
                this.notebooks.active.as_ref().unwrap().settings.view_mode,
                ViewMode::Source
            );
        })
    });
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
}

#[gpui::test]
fn filtered_command_rechecks_capability_and_keeps_one_undo_step(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("note.md"), "original").unwrap();
    let (notes, cx) = fixture(cx, directory.path());
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.command_palette(window, cx);
            this.palette
                .as_ref()
                .unwrap()
                .query
                .update(cx, |query, cx| query.set_value("current date", window, cx));
        })
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            assert!(matches!(
                this.palette.as_ref().unwrap().model.selected_entry(),
                Some(PaletteEntry::Command(Command::InsertDate))
            ));
            this.set_mode(ViewMode::Reading, window, cx);
            this.activate_palette(window, cx);
            assert!(
                this.palette.is_some(),
                "a now-unavailable command stays in the palette"
            );
            assert_eq!(this.editor.text(cx).as_ref(), "original");
            assert!(!this.dirty(cx));
            this.set_mode(ViewMode::Source, window, cx);
        })
    });
    cx.refresh().unwrap();
    cx.run_until_parked();
    cx.update(|window, cx| notes.update(cx, |this, cx| this.activate_palette(window, cx)));
    cx.run_until_parked();
    let changed = cx.update(|_, cx| {
        let this = notes.read(cx);
        assert!(this.palette.is_none());
        assert!(this.dirty(cx));
        this.editor.text(cx).to_string()
    });
    assert_ne!(changed, "original");
    cx.dispatch_action(Undo);
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(notes.read(cx).editor.text(cx).as_ref(), "original");
        assert!(!notes.read(cx).dirty(cx));
    });
    cx.dispatch_action(Redo);
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(notes.read(cx).editor.text(cx).as_ref(), changed));
}

#[gpui::test]
fn formatting_palette_preserves_unicode_selection_and_one_step_history(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("note.md"), "café\n茶").unwrap();
    let (notes, cx) = fixture(cx, directory.path());
    for mode in [ViewMode::Source, ViewMode::LivePreview] {
        for command in Command::ALL
            .iter()
            .copied()
            .filter(|c| c.markdown_format().is_some())
        {
            // Paragraph needs an existing heading so every command makes a real edit.
            let original = if command == Command::Paragraph {
                "# café\n茶"
            } else {
                "café\n茶"
            };
            let selected = if command == Command::Paragraph {
                2..7
            } else {
                0..5
            };
            cx.update(|window, cx| {
                notes.update(cx, |this, cx| {
                    this.mode = mode;
                    this.editor.set_mode(mode, cx);
                    this.editor
                        .select(0..this.editor.snapshot(cx).len(), window, cx);
                    this.editor.replace_selection(original, window, cx);
                    this.editor.select(selected.clone(), window, cx);
                    this.command_palette(window, cx);
                    this.palette
                        .as_ref()
                        .unwrap()
                        .query
                        .update(cx, |q, cx| q.set_value(command.label(), window, cx));
                })
            });
            cx.run_until_parked();
            cx.update(|window, cx| notes.update(cx, |this, cx| {
                assert!(matches!(this.palette.as_ref().unwrap().model.selected_entry(), Some(PaletteEntry::Command(c)) if *c == command));
                this.activate_palette(window, cx);
                assert!(this.palette.is_none());
                assert!(this.editor.is_focused(window, cx));
                assert_ne!(this.editor.text(cx).as_ref(), original);
                assert_eq!(&this.editor.text(cx)[this.editor.selection(cx)], "café");
            }));
            cx.dispatch_action(Undo);
            cx.run_until_parked();
            cx.update(|_, cx| {
                assert_eq!(
                    notes.read(cx).editor.text(cx).as_ref(),
                    original,
                    "{command:?}"
                )
            });
        }
    }
}

#[gpui::test]
fn old_file_scan_cannot_replace_a_new_command_palette(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("note.md"), "original").unwrap();
    let (notes, cx) = fixture(cx, directory.path());
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.go_to(window, cx);
            window.close_dialog(cx);
            this.close_palette(window, cx);
            this.command_palette(window, cx);
        })
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        let palette = notes.read(cx).palette.as_ref().unwrap();
        assert_eq!(palette.model.results().len(), Command::ALL.len());
        assert!(
            palette
                .model
                .results()
                .iter()
                .all(|index| matches!(index, PaletteEntry::Command(_)))
        );
    });
}

#[gpui::test]
fn go_to_file_uses_current_notebook_exclusions(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("note.md"), "original").unwrap();
    fs::create_dir(directory.path().join("archive")).unwrap();
    fs::write(directory.path().join("archive/hidden.md"), "hidden").unwrap();
    let (notes, cx) = fixture(cx, directory.path());
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.update_excluded_patterns(vec!["^archive$".into()], window, cx);
            this.go_to(window, cx);
        })
    });
    cx.run_until_parked();
    cx.update(|window, cx| notes.update(cx, |this, cx| {
        let entries = this.palette.as_ref().unwrap().model.results();
        assert_eq!(entries.len(), 1);
        assert!(matches!(&entries[0], PaletteEntry::File(entry) if entry.relative == Path::new("note.md")));
        window.close_dialog(cx);
        this.close_palette(window, cx);
        this.update_excluded_patterns(Vec::new(), window, cx);
        this.go_to(window, cx);
    }));
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(
            notes
                .read(cx)
                .palette
                .as_ref()
                .unwrap()
                .model
                .results()
                .len(),
            3
        )
    });
}

#[gpui::test]
fn palette_tab_commands_work_without_notebook_and_respect_modal_guards(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("note.md"), "original").unwrap();
    let (notes, cx) = fixture(cx, directory.path());
    let first = cx.update(|_, cx| {
        notes.update(cx, |this, _| {
            this.notebooks.active = None;
            this.tabs.active.id
        })
    });
    for (command, count) in [(Command::NewTab, 2), (Command::CloseTab, 1)] {
        cx.update(|window, cx| {
            notes.update(cx, |this, cx| {
                this.command_palette(window, cx);
                this.palette
                    .as_ref()
                    .unwrap()
                    .query
                    .update(cx, |query, cx| {
                        query.set_value(command.label(), window, cx);
                    });
            })
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            notes.update(cx, |this, cx| {
                assert!(
                    matches!(this.palette.as_ref().unwrap().model.selected_entry(),
                Some(PaletteEntry::Command(selected)) if *selected == command)
                );
                let ids = this.tabs.ids();
                this.new_note(window, cx);
                this.new_tab(window, cx);
                this.close_tab(this.tabs.active.id, window, cx);
                assert_eq!(this.tabs.ids(), ids, "modal blocks direct dispatch");
                this.activate_palette(window, cx);
            })
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            let this = notes.read(cx);
            assert!(this.palette.is_none());

            assert_eq!(this.tabs.ids().len(), count);
            assert!(!window.has_active_dialog(cx));
        });
    }
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            assert_eq!(this.tabs.active.id, first);
            assert_eq!(this.editor.text(cx).as_ref(), "original");
            this.new_note(window, cx);
            assert_eq!(this.tabs.active.id, first);
            assert!(!this.is_busy());
        })
    });
}

#[cfg(feature = "editor-diagnostics")]
#[gpui::test]
fn replace_shortcut_only_opens_for_editable_documents(cx: &mut TestAppContext) {
    use gpui::Focusable;
    cx.update(|cx| {
        cx.bind_keys(
            crate::help_ui::app_shortcuts()
                .into_iter()
                .map(|s| s.binding),
        )
    });
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("note.md"), "original").unwrap();
    let (notes, cx) = fixture(cx, directory.path());
    let key = if cfg!(target_os = "macos") {
        "cmd-alt-f"
    } else {
        "ctrl-h"
    };
    for mode in [ViewMode::Source, ViewMode::LivePreview] {
        cx.update(|window, cx| {
            notes.update(cx, |this, cx| {
                this.set_mode(mode, window, cx);
                this.editor.select(0..8, window, cx);
                this.editor.focus(window, cx);
            })
        });
        cx.refresh().unwrap();
        cx.simulate_keystrokes(key);
        cx.run_until_parked();
        cx.update(|window, cx| {
            let this = notes.read(cx);
            let (query, _, _, _) = this
                .editor
                .diagnostic_input()
                .read(cx)
                .diagnostic_find(cx)
                .unwrap();
            assert!(query.read(cx).focus_handle(cx).is_focused(window));
            assert_eq!(query.read(cx).value().as_ref(), "original");
            assert_eq!(this.editor.text(cx).as_ref(), "original");
        });
        cx.simulate_keystrokes("escape");
        cx.run_until_parked();
    }
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| this.set_mode(ViewMode::Reading, window, cx))
    });
    cx.refresh().unwrap();
    cx.simulate_keystrokes(key);
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert!(
            notes
                .read(cx)
                .editor
                .diagnostic_input()
                .read(cx)
                .diagnostic_find(cx)
                .is_none()
        );
    });
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.set_mode(ViewMode::Source, window, cx);
            this.open_help(false, window, cx);
        })
    });
    cx.refresh().unwrap();
    cx.simulate_keystrokes(key);
    cx.run_until_parked();
    cx.update(|window, cx| {
        assert!(window.has_active_dialog(cx));
        assert!(
            notes
                .read(cx)
                .editor
                .diagnostic_input()
                .read(cx)
                .diagnostic_find(cx)
                .is_none()
        );
        window.close_dialog(cx);
        notes.update(cx, |this, cx| this.install_document(None, window, cx));
    });
    cx.refresh().unwrap();
    cx.simulate_keystrokes(key);
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert!(
            notes
                .read(cx)
                .editor
                .diagnostic_input()
                .read(cx)
                .diagnostic_find(cx)
                .is_none()
        )
    });
}

#[gpui::test]
fn history_palette_navigates_and_rechecks_active_tab_targets(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("note.md"), "first").unwrap();
    fs::write(directory.path().join("second.md"), "second").unwrap();
    let (notes, cx) = fixture(cx, directory.path());
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.notebooks.active = None;
            this.open_path(directory.path().join("second.md"), false, window, cx);
        })
    });
    cx.run_until_parked();
    for (command, expected) in [(Command::Back, "first"), (Command::Forward, "second")] {
        cx.update(|window, cx| {
            notes.update(cx, |this, cx| {
                this.command_palette(window, cx);
                this.palette
                    .as_ref()
                    .unwrap()
                    .query
                    .update(cx, |query, cx| query.set_value(command.label(), window, cx));
            })
        });
        cx.run_until_parked();
        cx.update(|window, cx| notes.update(cx, |this, cx| {
            assert!(matches!(this.palette.as_ref().unwrap().model.selected_entry(), Some(PaletteEntry::Command(selected)) if *selected == command));
            this.activate_palette(window, cx);
            assert!(this.palette.is_none());
        }));
        cx.run_until_parked();
        cx.update(|_, cx| assert_eq!(notes.read(cx).editor.text(cx).as_ref(), expected));
    }
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.command_palette(window, cx);
            this.palette
                .as_ref()
                .unwrap()
                .query
                .update(cx, |query, cx| {
                    query.set_value(Command::Back.label(), window, cx)
                });
        })
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            assert!(this.command_unavailable(Command::Back).is_none());
            this.tabs.active.navigation = Default::default();
            this.activate_palette(window, cx);
            assert!(
                this.palette.is_some(),
                "fresh target guard retains disabled palette"
            );
            assert_eq!(this.editor.text(cx).as_ref(), "second");
            window.close_dialog(cx);
            this.close_palette(window, cx);
            this.run_command(Command::Back, window, cx);
            assert_eq!(this.editor.text(cx).as_ref(), "second");
        })
    });
}

#[gpui::test]
fn sidebar_palette_commands_switch_panels_and_recheck_notebook_and_busy_guards(
    cx: &mut TestAppContext,
) {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("note.md"), "original").unwrap();
    let (notes, cx) = fixture(cx, directory.path());
    for (command, collapsed) in [
        (Command::ToggleSidebar, true),
        (Command::ShowExplorer, false),
    ] {
        cx.update(|window, cx| {
            notes.update(cx, |this, cx| {
                this.command_palette(window, cx);
                this.palette
                    .as_ref()
                    .unwrap()
                    .query
                    .update(cx, |query, cx| query.set_value(command.label(), window, cx));
            })
        });
        cx.run_until_parked();
        cx.update(|window, cx| notes.update(cx, |this, cx| {
            assert!(matches!(this.palette.as_ref().unwrap().model.selected_entry(), Some(PaletteEntry::Command(selected)) if *selected == command));
            this.activate_palette(window, cx);
            assert_eq!(this.sidebar_collapsed, collapsed);
            assert!(this.palette.is_none());
            if collapsed {
                assert!(this.editor.is_focused(window, cx));
            } else {
                assert!(this.notebooks.tree_focus.is_focused(window));
            }
        }));
    }
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.command_palette(window, cx);
            this.palette
                .as_ref()
                .unwrap()
                .query
                .update(cx, |query, cx| {
                    query.set_value(Command::ToggleSidebar.label(), window, cx)
                });
        })
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.notebooks.active = None;
            this.activate_palette(window, cx);
            assert!(this.palette.is_some());
            assert!(!this.sidebar_collapsed);
            window.close_dialog(cx);
            this.close_palette(window, cx);
            this.run_command(Command::ToggleSidebar, window, cx);
            this.run_command(Command::ShowExplorer, window, cx);
            assert!(!this.sidebar_collapsed);
            this.notebooks.active =
                Some(notebook::Notebook::open(directory.path(), false).unwrap());
            this.open_help(false, window, cx);
            this.run_command(Command::ToggleSidebar, window, cx);
            this.sidebar_collapsed = true;
            this.run_command(Command::ShowExplorer, window, cx);
            assert!(this.sidebar_collapsed, "dialog blocks direct invocation");
            window.close_dialog(cx);
            let _ = this.session.pick();
            this.run_command(Command::ToggleSidebar, window, cx);
            this.run_command(Command::ShowExplorer, window, cx);
            assert!(
                this.sidebar_collapsed,
                "busy session blocks direct invocation"
            );
        })
    });
}

fn select_discovery_command(notes: &Entity<Notes>, command: Command, cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.command_palette(window, cx);
            this.palette
                .as_ref()
                .unwrap()
                .query
                .update(cx, |query, cx| {
                    query.set_value(command.label(), window, cx);
                });
        })
    });
    cx.run_until_parked();
    cx.update(|_, cx| assert!(matches!(notes.read(cx).palette.as_ref().unwrap().model.selected_entry(), Some(PaletteEntry::Command(selected)) if *selected == command)));
}

#[gpui::test]
fn discovery_palette_open_uses_system_picker_and_save_persists_reading_document(
    cx: &mut TestAppContext,
) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("note.md");
    fs::write(&path, "original").unwrap();
    let (notes, cx) = fixture(cx, directory.path());
    select_discovery_command(&notes, Command::OpenFile, cx);
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.activate_palette(window, cx);
            assert!(this.palette.is_none());
            assert!(this.notebooks.active.is_some());
            let operation = this
                .file_picker_operation
                .take()
                .expect("Open goes to OS picker even in notebook");
            assert!(this.document_interaction_blocked());
            assert!(this.editor.is_focused(window, cx));
            this.finish_file_picker(operation, Ok(None), window, cx);
            assert!(!this.document_interaction_blocked());
            this.editor.select(0..8, window, cx);
            this.editor.replace_selection("saved draft", window, cx);
            this.set_mode(ViewMode::Reading, window, cx);
        })
    });
    cx.run_until_parked();
    select_discovery_command(&notes, Command::Save, cx);
    cx.update(|window, cx| notes.update(cx, |this, cx| this.activate_palette(window, cx)));
    cx.run_until_parked();
    assert_eq!(fs::read_to_string(path).unwrap(), "saved draft");
    cx.update(|_, cx| assert!(!notes.read(cx).dirty(cx)));
}

#[gpui::test]
fn discovery_palette_manage_help_and_fresh_availability_guards(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("note.md"), "original").unwrap();
    let (notes, cx) = fixture(cx, directory.path());
    select_discovery_command(&notes, Command::ManageNotebooks, cx);
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            assert!(this.command_unavailable(Command::ManageNotebooks).is_some());
            this.activate_palette(window, cx);
            assert!(this.palette.is_some());
            this.notebooks
                .set_test_persistence_target(directory.path().join("index.json"));
            assert!(this.command_unavailable(Command::ManageNotebooks).is_none());
            this.activate_palette(window, cx);
            assert!(this.palette.is_none());
            assert!(window.has_active_dialog(cx));
            this.run_command(Command::OpenFile, window, cx);
            assert!(this.file_picker_operation.is_none());
            this.go_to(window, cx);
            this.command_palette(window, cx);
            assert!(this.palette.is_none());
            window.close_dialog(cx);
        })
    });
    cx.run_until_parked();
    select_discovery_command(&notes, Command::KeyboardShortcuts, cx);
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.activate_palette(window, cx);
            assert!(this.palette.is_none());
            assert!(window.has_active_dialog(cx));
            window.close_dialog(cx);
        })
    });
    cx.run_until_parked();
    select_discovery_command(&notes, Command::Save, cx);
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.install_document(None, window, cx);
            assert!(this.command_unavailable(Command::Save).is_some());
            this.activate_palette(window, cx);
            assert!(this.palette.is_some(), "Save rechecks the current document");
            window.close_dialog(cx);
            this.close_palette(window, cx);
            this.request(crate::Next::Open, window, cx);
            let operation = this.file_picker_operation.take().unwrap();
            for command in [
                Command::OpenFile,
                Command::Save,
                Command::ManageNotebooks,
                Command::KeyboardShortcuts,
            ] {
                assert!(this.command_unavailable(command).is_some());
                this.run_command(command, window, cx);
            }
            this.go_to(window, cx);
            this.command_palette(window, cx);
            assert!(this.palette.is_none());
            assert!(!window.has_active_dialog(cx));
            assert!(this.file_picker_operation.is_none());
            this.finish_file_picker(operation, Ok(None), window, cx);
        })
    });
}

#[gpui::test]
fn discovery_go_to_shortcut_keeps_notebook_search_and_standalone_picker_fallback(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        cx.bind_keys(
            crate::help_ui::app_shortcuts()
                .into_iter()
                .map(|s| s.binding),
        )
    });
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("note.md"), "original").unwrap();
    let (notes, cx) = fixture(cx, directory.path());
    let key = if cfg!(target_os = "macos") {
        "cmd-o"
    } else {
        "ctrl-o"
    };
    cx.simulate_keystrokes(key);
    cx.run_until_parked();
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            let palette = this.palette.as_ref().unwrap();
            assert_eq!(palette.mode, super::PaletteMode::Files);
            assert!(palette.query.read(cx).focus_handle(cx).is_focused(window));
            assert!(this.file_picker_operation.is_none());
            window.close_dialog(cx);
            this.close_palette(window, cx);
            this.notebooks.active = None;
        })
    });
    cx.run_until_parked();
    cx.simulate_keystrokes(key);
    cx.run_until_parked();
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            assert!(this.palette.is_none());
            let operation = this.file_picker_operation.take().unwrap();
            this.finish_file_picker(operation, Ok(None), window, cx);
            assert!(this.editor.is_focused(window, cx));
        })
    });
}

#[gpui::test]
fn tasks_palette_distinguishes_completion_from_formatting_with_one_step_undo(
    cx: &mut TestAppContext,
) {
    let directory = tempfile::tempdir().unwrap();
    let original = "- [ ] café\n- [ ] 茶";
    fs::write(directory.path().join("note.md"), original).unwrap();
    let (notes, cx) = fixture(cx, directory.path());
    for mode in [ViewMode::Source, ViewMode::LivePreview] {
        for (command, expected) in [
            (Command::ToggleTasks, "- [x] café\n- [x] 茶"),
            (Command::TaskList, "café\n茶"),
        ] {
            cx.update(|window, cx| {
                notes.update(cx, |this, cx| {
                    this.set_mode(mode, window, cx);
                    this.editor.select(0..original.len(), window, cx);
                    this.command_palette(window, cx);
                    this.palette
                        .as_ref()
                        .unwrap()
                        .query
                        .update(cx, |query, cx| {
                            query.set_value(command.label(), window, cx);
                        });
                })
            });
            cx.run_until_parked();
            cx.update(|window, cx| {
                notes.update(cx, |this, cx| {
                    assert!(
                        matches!(this.palette.as_ref().unwrap().model.selected_entry(),
                    Some(PaletteEntry::Command(selected)) if *selected == command)
                    );
                    this.activate_palette(window, cx);
                    assert_eq!(this.editor.text(cx).as_ref(), expected);
                    assert!(this.editor.is_focused(window, cx));
                })
            });
            cx.dispatch_action(Undo);
            cx.run_until_parked();
            cx.update(|_, cx| assert_eq!(notes.read(cx).editor.text(cx).as_ref(), original));
        }
    }
}

#[gpui::test]
fn date_shortcut_and_palette_reuse_scoped_format_unicode_selection_and_undo(
    cx: &mut TestAppContext,
) {
    cx.update(|cx| {
        cx.bind_keys(
            crate::help_ui::app_shortcuts()
                .into_iter()
                .map(|s| s.binding),
        )
    });
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("note.md"), "café 茶 fin").unwrap();
    let (notes, cx) = fixture(cx, directory.path());
    let key = if cfg!(target_os = "macos") {
        "cmd-shift-d"
    } else {
        "ctrl-shift-d"
    };
    for mode in [ViewMode::Source, ViewMode::LivePreview] {
        for palette in [false, true] {
            cx.update(|window, cx| {
                notes.update(cx, |this, cx| {
                    this.notebooks.active.as_mut().unwrap().settings.date_format = "[日期]".into();
                    this.session_date_format = "[session]".into();
                    this.set_mode(mode, window, cx);
                    this.editor.select(6..9, window, cx);
                    this.editor.focus(window, cx);
                    if palette {
                        this.command_palette(window, cx);
                        this.palette
                            .as_ref()
                            .unwrap()
                            .query
                            .update(cx, |query, cx| {
                                query.set_value("current date", window, cx);
                            });
                    }
                })
            });
            cx.refresh().unwrap();
            cx.run_until_parked();
            if palette {
                cx.update(|window, cx| {
                    notes.update(cx, |this, cx| this.activate_palette(window, cx))
                });
            } else {
                cx.simulate_keystrokes(key);
            }
            cx.run_until_parked();
            cx.update(|_, cx| assert_eq!(notes.read(cx).editor.text(cx).as_ref(), "café 日期 fin"));
            cx.dispatch_action(Undo);
            cx.run_until_parked();
            cx.update(|_, cx| assert_eq!(notes.read(cx).editor.text(cx).as_ref(), "café 茶 fin"));
        }
    }
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.notebooks.active = None;
            this.editor.select(6..9, window, cx);
            this.editor.focus(window, cx);
        })
    });
    cx.refresh().unwrap();
    cx.simulate_keystrokes(key);
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(notes.read(cx).editor.text(cx).as_ref(), "café session fin"));
}

#[gpui::test]
fn task_and_date_adapters_reject_reading_dialogs_busy_and_empty(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("note.md"), "- [ ] café").unwrap();
    for guard in ["reading", "dialog", "busy", "empty"] {
        let (notes, cx) = fixture(cx, directory.path());
        cx.update(|window, cx| {
            notes.update(cx, |this, cx| {
                this.set_mode(ViewMode::Source, window, cx);
                match guard {
                    "reading" => this.set_mode(ViewMode::Reading, window, cx),
                    "dialog" => this.open_help(false, window, cx),
                    "busy" => {
                        let _ = this.session.pick();
                    }
                    "empty" => this.install_document(None, window, cx),
                    _ => unreachable!(),
                }
                let before = this.editor.text(cx);
                this.insert_current_date(window, cx);
                this.toggle_selected_tasks(window, cx);
                this.run_command(Command::InsertDate, window, cx);
                this.run_command(Command::ToggleTasks, window, cx);
                assert_eq!(this.editor.text(cx), before, "{guard}");
                if guard == "dialog" {
                    window.close_dialog(cx);
                }
            })
        });
        cx.run_until_parked();
    }
}

#[gpui::test]
fn date_shortcut_ignores_search_and_modal_inputs(cx: &mut TestAppContext) {
    cx.update(|cx| {
        cx.bind_keys(
            crate::help_ui::app_shortcuts()
                .into_iter()
                .map(|s| s.binding),
        )
    });
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("note.md"), "café 茶").unwrap();
    let (notes, cx) = fixture(cx, directory.path());
    let key = if cfg!(target_os = "macos") {
        "cmd-shift-d"
    } else {
        "ctrl-shift-d"
    };
    // The editor's own find field is within DocumentEditor; the explicit focus
    // guard must prevent the bubbling action from inserting into the document.
    cx.update(|window, cx| notes.update(cx, |this, cx| this.editor.find(window, cx)));
    cx.refresh().unwrap();
    cx.simulate_keystrokes(key);
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(notes.read(cx).editor.text(cx).as_ref(), "café 茶"));
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.command_palette(window, cx);
            this.palette
                .as_ref()
                .unwrap()
                .query
                .update(cx, |query, cx| query.set_value("draft", window, cx));
        })
    });
    cx.refresh().unwrap();
    cx.simulate_keystrokes(key);
    cx.run_until_parked();
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            assert_eq!(
                this.palette
                    .as_ref()
                    .unwrap()
                    .query
                    .read(cx)
                    .value()
                    .as_ref(),
                "draft"
            );
            assert_eq!(this.editor.text(cx).as_ref(), "café 茶");
            window.close_dialog(cx);
            this.close_palette(window, cx);
            this.open_settings(window, cx);
        })
    });
    cx.refresh().unwrap();
    cx.simulate_keystrokes(key);
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(notes.read(cx).editor.text(cx).as_ref(), "café 茶"));
}

#[gpui::test]
fn explorer_palette_captures_selected_file_or_folder_across_dismissal(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("note.md"), "active document").unwrap();
    fs::write(directory.path().join("other.md"), "selected file").unwrap();
    fs::create_dir(directory.path().join("folder")).unwrap();
    let (notes, cx) = fixture(cx, directory.path());
    let calls = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let recorded = calls.clone();
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.notebooks
                .test_set_reveal_backend(Box::new(move |path| {
                    recorded.borrow_mut().push(path.to_path_buf());
                    Ok(())
                }));
            this.test_refresh_tree(window, cx);
        })
    });
    cx.run_until_parked();
    for path in ["other.md", "folder"] {
        for command in [Command::CopySelectedPath, Command::RevealSelectedEntry] {
            cx.update(|window, cx| {
                notes.update(cx, |this, cx| {
                    this.notebooks
                        .test_explorer()
                        .select_path(Path::new(path), path == "folder");
                    this.command_palette(window, cx);
                    let palette = this.palette.as_mut().unwrap();
                    palette
                        .model
                        .replace_entries(vec![PaletteEntry::Command(command)], "");
                    assert!(
                        this.palette_entry_detail(&PaletteEntry::Command(command))
                            .contains(path)
                    );
                    // A changed tree selection must not replace the captured target.
                    this.notebooks
                        .test_explorer()
                        .select_path(Path::new("note.md"), false);
                    assert!(this.command_unavailable(command).is_none());
                    this.activate_palette(window, cx);
                    assert!(this.palette.is_none());
                    assert!(!window.has_active_dialog(cx));
                    assert_eq!(
                        this.document().unwrap().path,
                        directory.path().join("note.md").canonicalize().unwrap()
                    );
                    if command == Command::CopySelectedPath {
                        assert_eq!(
                            cx.read_from_clipboard().unwrap().text().as_deref(),
                            Some(path)
                        );
                    }
                })
            });
            cx.run_until_parked();
        }
    }
    assert_eq!(
        *calls.borrow(),
        [
            directory.path().join("other.md").canonicalize().unwrap(),
            directory.path().join("folder").canonicalize().unwrap()
        ]
    );
    // Selection commands also work in an empty tab.
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.new_tab(window, cx);
            assert!(this.document().is_none());
            this.notebooks
                .test_explorer()
                .select_path(Path::new("other.md"), false);
            this.command_palette(window, cx);
            this.palette
                .as_mut()
                .unwrap()
                .model
                .replace_entries(vec![PaletteEntry::Command(Command::CopySelectedPath)], "");
            this.activate_palette(window, cx);
            assert_eq!(
                cx.read_from_clipboard().unwrap().text().as_deref(),
                Some("other.md")
            );
        })
    });
}

#[gpui::test]
fn explorer_palette_rejects_stale_missing_and_invalid_targets(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("note.md"), "active").unwrap();
    fs::write(directory.path().join("other.md"), "selected").unwrap();
    let (notes, cx) = fixture(cx, directory.path());
    for stale_kind in 0..3 {
        cx.update(|window, cx| notes.update(cx, |this, cx| this.test_refresh_tree(window, cx)));
        cx.run_until_parked();
        cx.update(|window, cx| {
            notes.update(cx, |this, cx| {
                this.notebooks
                    .test_explorer()
                    .select_path(Path::new("other.md"), false);
                this.command_palette(window, cx);
                this.palette
                    .as_mut()
                    .unwrap()
                    .model
                    .replace_entries(vec![PaletteEntry::Command(Command::CopySelectedPath)], "");
                cx.write_to_clipboard(gpui::ClipboardItem::new_string("unchanged".into()));
                let root = this.notebooks.active.as_ref().unwrap().root.clone();
                match stale_kind {
                    0 => {
                        this.notebooks.test_explorer().begin_refresh();
                    }
                    1 => {
                        this.notebooks.active.as_mut().unwrap().root =
                            directory.path().join("different-notebook");
                    }
                    _ => {
                        fs::remove_file(directory.path().join("other.md")).unwrap();
                    }
                }
                this.activate_palette(window, cx);
                assert_eq!(
                    cx.read_from_clipboard().unwrap().text().as_deref(),
                    Some("unchanged")
                );
                if stale_kind < 2 {
                    assert!(
                        this.command_unavailable(Command::CopySelectedPath)
                            .is_some()
                    );
                } else {
                    assert!(this.notebooks.notice.is_some());
                    this.notebooks.test_set_reveal_backend(Box::new(|_| {
                        panic!("Missing target reached Reveal backend")
                    }));
                    this.command_palette(window, cx);
                    this.palette.as_mut().unwrap().model.replace_entries(
                        vec![PaletteEntry::Command(Command::RevealSelectedEntry)],
                        "",
                    );
                    this.activate_palette(window, cx);
                    assert!(this.palette.is_none());
                }
                this.notebooks.active.as_mut().unwrap().root = root;
                window.close_dialog(cx);
                this.close_palette(window, cx);
            })
        });
        cx.run_until_parked();
    }
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.notebooks.test_explorer().select(None);
            this.command_palette(window, cx);
            assert!(
                this.command_unavailable(Command::RevealSelectedEntry)
                    .is_some()
            );
            window.close_dialog(cx);
            this.close_palette(window, cx);
            let settings = this.notebooks.active.as_ref().unwrap().settings.clone();
            for (path, link) in [("link.md", true), ("unsupported.bin", false)] {
                let generation = this
                    .notebooks
                    .test_explorer()
                    .begin_directory(Default::default(), &settings);
                this.notebooks.test_explorer().complete_directory(
                    generation,
                    Default::default(),
                    Ok(vec![notebook::Entry {
                        relative: path.into(),
                        directory: false,
                        link,
                        modified: None,
                        created: None,
                    }]),
                    &settings,
                );
                this.notebooks.test_explorer().select(Some(0));
                assert!(this.selected_entry_target().is_none());
            }
            this.notebooks
                .test_explorer()
                .begin_directory(Default::default(), &settings);
            this.notebooks.test_explorer().select(Some(0));
            assert!(this.selected_entry_target().is_none());
            this.rename_selected_entry(window, cx);
            assert!(!window.has_active_dialog(cx));
        })
    });
}

#[gpui::test]
fn explorer_f2_renames_selection_and_is_scoped_away_from_editor_and_forms(cx: &mut TestAppContext) {
    cx.update(|cx| {
        cx.bind_keys(
            crate::help_ui::app_shortcuts()
                .into_iter()
                .map(|s| s.binding),
        )
    });
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("note.md"), "active").unwrap();
    fs::write(directory.path().join("other.md"), "selected").unwrap();
    let (notes, cx) = fixture(cx, directory.path());
    cx.update(|window, cx| notes.update(cx, |this, cx| this.test_refresh_tree(window, cx)));
    cx.run_until_parked();
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.notebooks
                .test_explorer()
                .select_path(Path::new("other.md"), false);
            this.editor.focus(window, cx);
        })
    });
    cx.refresh().unwrap();
    cx.simulate_keystrokes("f2");
    cx.run_until_parked();
    cx.update(|window, cx| {
        assert!(!window.has_active_dialog(cx));
        notes.read(cx).notebooks.tree_focus.focus(window);
    });
    cx.refresh().unwrap();
    cx.simulate_keystrokes("f2");
    cx.run_until_parked();
    cx.update(|window, cx| assert!(window.has_active_dialog(cx)));
    // The rename input is focused: F2 cannot create another dialog.
    cx.simulate_keystrokes("f2");
    cx.simulate_keystrokes(if cfg!(target_os = "macos") {
        "cmd-a"
    } else {
        "ctrl-a"
    });
    cx.simulate_input("renamed.md");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(directory.path().join("renamed.md").exists());
    assert!(!directory.path().join("other.md").exists());
    assert_eq!(
        fs::read_to_string(directory.path().join("note.md")).unwrap(),
        "active"
    );
    cx.update(|window, cx| {
        assert!(!window.has_active_dialog(cx));
        assert_eq!(
            notes.read(cx).document().unwrap().path.file_name().unwrap(),
            "note.md"
        );
    });
}

#[gpui::test]
fn explorer_actions_recheck_modal_and_write_guards(cx: &mut TestAppContext) {
    use crate::notebook_ui::EntryAction;
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("note.md"), "active").unwrap();
    let (notes, cx) = fixture(cx, directory.path());
    cx.update(|window, cx| notes.update(cx, |this, cx| this.test_refresh_tree(window, cx)));
    cx.run_until_parked();
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.notebooks
                .test_explorer()
                .select_path(Path::new("note.md"), false);
            let target = this.selected_entry_target().unwrap();
            cx.write_to_clipboard(gpui::ClipboardItem::new_string("unchanged".into()));
            this.notebooks.test_set_reveal_backend(Box::new(|_| {
                panic!("Blocked action reached Reveal backend")
            }));
            this.open_help(false, window, cx);
            this.run_selected_entry_action(target.clone(), EntryAction::CopyPath, window, cx);
            this.run_selected_entry_action(target.clone(), EntryAction::Reveal, window, cx);
            this.rename_selected_entry(window, cx);
            assert_eq!(
                cx.read_from_clipboard().unwrap().text().as_deref(),
                Some("unchanged")
            );
            window.close_dialog(cx);
            // Hold a real session write before applying its I/O effects.
            let effects = this.tabs.active.session.save(None);
            assert!(this.tabs.active.session.write().is_some());
            this.run_selected_entry_action(target.clone(), EntryAction::CopyPath, window, cx);
            this.run_selected_entry_action(target, EntryAction::Reveal, window, cx);
            this.rename_selected_entry(window, cx);
            assert!(!window.has_active_dialog(cx));
            assert_eq!(
                cx.read_from_clipboard().unwrap().text().as_deref(),
                Some("unchanged")
            );
            this.apply_session_effects(effects, window, cx);
        })
    });
    cx.run_until_parked();
}

#[gpui::test]
fn sort_palette_searches_applies_persists_and_preserves_document(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    let persistence = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("note.md"), "original").unwrap();
    fs::write(directory.path().join("a.md"), "a").unwrap();
    let (notes, cx) = fixture(cx, directory.path());
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.notebooks
                .set_test_persistence_target(persistence.path().join("test-index.json"));
            this.test_refresh_tree(window, cx);
        })
    });
    cx.run_until_parked();
    let identity = cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.editor.select(0..8, window, cx);
            this.editor.replace_selection("draft text", window, cx);
            this.editor.select(6..10, window, cx);
            (
                this.tabs.active.id,
                this.editor.id(),
                this.document().unwrap().path.clone(),
            )
        })
    });
    for (sort, label) in notebook::FileSort::OPTIONS {
        cx.update(|window, cx| {
            notes.update(cx, |this, cx| {
                this.command_palette(window, cx);
                let palette = this.palette.as_mut().unwrap();
                palette.model.filter(Command::SortFiles.label());
                this.activate_palette(window, cx);
                let palette = this.palette.as_mut().unwrap();
                assert_eq!(palette.mode, super::PaletteMode::Sort);
                assert_eq!(palette.model.results().len(), 6);
                assert_eq!(
                    palette
                        .model
                        .results()
                        .iter()
                        .filter(|entry| matches!(
                            entry,
                            PaletteEntry::FileSort { active: true, .. }
                        ))
                        .count(),
                    1
                );
                assert!(matches!(
                    palette.model.selected_entry(),
                    Some(PaletteEntry::FileSort { active: true, .. })
                ));
                palette
                    .query
                    .update(cx, |query, cx| query.set_value(label, window, cx));
            })
        });
        cx.run_until_parked();
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        cx.update(|window, cx| {
            let this = notes.read(cx);
            assert!(this.palette.is_none());
            assert_eq!(
                this.notebooks.active.as_ref().unwrap().settings.file_sort,
                sort
            );
            assert_eq!(this.tabs.active.id, identity.0);
            assert_eq!(this.editor.id(), identity.1);
            assert_eq!(this.document().unwrap().path, identity.2);
            assert_eq!(this.editor.text(cx).as_ref(), "draft text");
            assert_eq!(this.editor.selection(cx), 6..10);
            assert!(this.editor.is_focused(window, cx));
        });
    }
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.run_command(Command::SortFiles, window, cx);
            this.palette
                .as_mut()
                .unwrap()
                .model
                .filter("File name (Z to A)");
            // Refresh does not invalidate a preference choice for the same notebook.
            this.test_refresh_tree(window, cx);
            this.activate_palette(window, cx);
        })
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        notes.update(cx, |this, _| {
            let rows = this.notebooks.test_explorer().rows();
            assert_eq!(rows.first().unwrap().path, Path::new("note.md"));
            assert_eq!(rows.last().unwrap().path, Path::new("a.md"));
        })
    });
    assert_eq!(
        notebook::Notebook::open(directory.path(), false)
            .unwrap()
            .settings
            .file_sort,
        notebook::FileSort::NameDescending
    );
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.run_command(Command::SortFiles, window, cx)
        })
    });
    cx.run_until_parked();
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(
            notes.read(cx).palette.as_ref().unwrap().mode,
            super::PaletteMode::Commands
        )
    });
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    cx.update(|window, cx| {
        let this = notes.read(cx);
        assert!(this.palette.is_none());
        assert!(this.editor.is_focused(window, cx));
    });
}

#[gpui::test]
fn sort_palette_rejects_stale_notebooks_and_modal_busy_state(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    let other = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("note.md"), "original").unwrap();
    let (notes, cx) = fixture(cx, directory.path());
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.run_command(Command::SortFiles, window, cx);
            this.palette
                .as_mut()
                .unwrap()
                .model
                .filter("File name (Z to A)");
            this.notebooks.active = Some(notebook::Notebook::open(other.path(), false).unwrap());
            this.activate_palette(window, cx);
            assert_eq!(
                this.notebooks.active.as_ref().unwrap().settings.file_sort,
                notebook::FileSort::NameAscending
            );
            this.run_command(Command::SortFiles, window, cx);
            this.notebooks.active = None;
            this.activate_palette(window, cx);
            assert!(this.palette.is_none());
            assert!(this.command_unavailable(Command::SortFiles).is_some());
            this.run_command(Command::SortFiles, window, cx);
            assert!(this.palette.is_none());
            this.notebooks.active =
                Some(notebook::Notebook::open(directory.path(), false).unwrap());
            let appearance = this.appearance();
            this.open_help(false, window, cx);
            this.run_command(Command::SortFiles, window, cx);
            this.toggle_theme(window, cx);
            assert!(this.palette.is_none());
            assert_eq!(this.appearance(), appearance);
            window.close_dialog(cx);
            this.editor.replace_selection("draft", window, cx);
        })
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.request(
                crate::Next::File(directory.path().join("next.md")),
                window,
                cx,
            );
        })
    });
    cx.run_until_parked();
    assert!(cx.has_pending_prompt());
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            let appearance = this.appearance();
            this.run_command(Command::SortFiles, window, cx);
            this.set_file_sort(notebook::FileSort::NameDescending, window, cx);
            this.toggle_theme(window, cx);
            assert!(this.palette.is_none());
            assert_eq!(
                this.notebooks.active.as_ref().unwrap().settings.file_sort,
                notebook::FileSort::NameAscending
            );
            assert_eq!(this.appearance(), appearance);
        })
    });
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
}
