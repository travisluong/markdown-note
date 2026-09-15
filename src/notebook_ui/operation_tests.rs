//! Tree navigation runs on GPUI TestPlatform; no native UI or OS input is used.
use crate::{Document, Notes, notebook};
use gpui::{Entity, TestAppContext, VisualTestContext, prelude::*};
use gpui_component::Root;
use std::{
    fs,
    path::{Path, PathBuf},
};

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
            let mut entries = notebook::read_directory(&book.root, Path::new("")).unwrap();
            book.settings.sort_entries(&mut entries);
            let generation = notes
                .notebooks
                .explorer
                .begin_directory(PathBuf::new(), &book.settings);
            notes
                .notebooks
                .explorer
                .complete_directory(generation, PathBuf::new(), Ok(entries), &book.settings)
                .unwrap();
            notes.notebooks.active = Some(book);
            notes.session_autosave.enabled = false;
            notes
        });
        entity.update(cx, |this, cx| {
            this.install_document(Some(Document::open(root.join("a.md")).unwrap()), window, cx);
        });
        notes = Some(entity.clone());
        Root::new(entity, window, cx)
    });
    (notes.unwrap(), cx)
}

#[derive(Clone, Copy)]
enum Retarget {
    NextNote,
    Folder,
    CurrentNote,
    Image,
}

fn retarget_pending_tree_open(cx: &mut TestAppContext, target: Retarget) {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    for (file, text) in [("a.md", "A"), ("b.md", "B"), ("c.md", "C")] {
        fs::write(root.join(file), text).unwrap();
    }
    fs::create_dir(root.join("folder")).unwrap();
    image::RgbaImage::from_pixel(2, 2, image::Rgba([255, 0, 0, 255]))
        .save(root.join("image.png"))
        .unwrap();
    let (notes, cx) = fixture(cx, root);

    let original_identity = cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            let book_root = this.notebooks.active.as_ref().unwrap().root.clone();
            let selection = this
                .notebooks
                .explorer
                .rows()
                .iter()
                .position(|row| row.path == Path::new("b.md"));
            this.notebooks.explorer.select(selection);
            let identity = this.document_id();
            this.open_path(book_root.join("b.md"), true, window, cx);
            assert!(this.is_busy());
            assert!(this.tree_loading());

            // Change the actual tree selection before the executor can finish B.
            // Arrows coalesce pending file targets instead of beginning a second load.
            match target {
                Retarget::NextNote => this.tree_move(true, window, cx),
                Retarget::CurrentNote => this.tree_move(false, window, cx),
                Retarget::Folder => {
                    let selection = this
                        .notebooks
                        .explorer
                        .rows()
                        .iter()
                        .position(|row| row.directory);
                    this.notebooks.explorer.select(selection);
                    this.open_selected_row(window, cx);
                }
                Retarget::Image => {
                    this.tree_move(true, window, cx);
                    this.tree_move(true, window, cx);
                }
            }
            assert_eq!(this.editor.text(cx).as_ref(), "A");
            assert_eq!(this.document_id(), identity);
            identity
        })
    });
    cx.run_until_parked();

    assert!(!cx.has_pending_prompt());
    cx.update(|_, cx| {
        let this = notes.read(cx);
        assert!(!this.is_busy(), "retargeted load leaked its operation gate");
        assert!(!this.tree_loading());
        assert!(this.notebooks.tree_operation.is_none());
        match target {
            Retarget::NextNote => {
                assert_eq!(this.document().unwrap().path.file_name().unwrap(), "c.md");
                assert_eq!(this.editor.text(cx).as_ref(), "C");
                assert!(this.image_path().is_none());
                // A new content identity belongs to the final installed target.
                assert_ne!(this.session.content().identity(), original_identity);
            }
            Retarget::Folder | Retarget::CurrentNote => {
                assert_eq!(this.document().unwrap().path.file_name().unwrap(), "a.md");
                assert_eq!(this.editor.text(cx).as_ref(), "A");
                assert!(this.image_path().is_none());
                assert_eq!(this.document_id(), original_identity);
            }
            Retarget::Image => {
                assert!(this.document().is_none());
                assert_eq!(this.image_path().unwrap().file_name().unwrap(), "image.png");
                assert_ne!(this.session.content().identity(), original_identity);
            }
        }
    });
}

#[gpui::test]
fn tree_load_retargets_to_latest_note(cx: &mut TestAppContext) {
    retarget_pending_tree_open(cx, Retarget::NextNote);
}

#[gpui::test]
fn tree_load_abandoned_for_folder_retains_current_note(cx: &mut TestAppContext) {
    retarget_pending_tree_open(cx, Retarget::Folder);
}

#[gpui::test]
fn tree_load_retargeted_to_current_note_releases_operation(cx: &mut TestAppContext) {
    retarget_pending_tree_open(cx, Retarget::CurrentNote);
}

#[gpui::test]
fn tree_load_retargets_to_image_without_leaking_operation(cx: &mut TestAppContext) {
    retarget_pending_tree_open(cx, Retarget::Image);
}
