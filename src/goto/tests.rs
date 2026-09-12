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
