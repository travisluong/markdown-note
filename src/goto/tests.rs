//! Palette adapter contracts using TestPlatform, without native UI/input.
use super::{Command, PaletteEntry};
use crate::{Document, Notes, ViewMode, notebook};
use gpui::{Entity, TestAppContext, VisualTestContext, prelude::*};
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
        Root::new(entity, window, cx)
    });
    cx.refresh().unwrap();
    cx.run_until_parked();
    (notes.unwrap(), cx)
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
