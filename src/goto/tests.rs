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
