//! Tree navigation runs on GPUI TestPlatform; no native UI or OS input is used.
use super::DirectoryState;
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
            notes.notebooks.active = Some(book);
            notes
                .notebooks
                .directories
                .insert(PathBuf::new(), DirectoryState::Ready(entries));
            notes.notebooks.rebuild_rows();
            notes.saves.session.enabled = false;
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
            this.notebooks.cursor = this
                .notebooks
                .rows
                .iter()
                .position(|row| row.path == Path::new("b.md"));
            let identity = this.saves.identity;
            this.open_path(book_root.join("b.md"), true, window, cx);
            assert!(this.is_busy());
            assert!(this.notebooks.tree_loading);

            // Change the actual tree selection before the executor can finish B.
            // Arrows coalesce pending file targets instead of beginning a second load.
            match target {
                Retarget::NextNote => this.tree_move(true, window, cx),
                Retarget::CurrentNote => this.tree_move(false, window, cx),
                Retarget::Folder => {
                    this.notebooks.cursor =
                        this.notebooks.rows.iter().position(|row| row.directory);
                    this.open_selected_row(window, cx);
                }
                Retarget::Image => {
                    this.tree_move(true, window, cx);
                    this.tree_move(true, window, cx);
                }
            }
            let expected_target = match target {
                Retarget::NextNote => Some(book_root.join("c.md")),
                Retarget::CurrentNote => Some(book_root.join("a.md")),
                Retarget::Folder => None,
                Retarget::Image => Some(book_root.join("image.png")),
            };
            assert_eq!(this.notebooks.tree_target, expected_target);
            assert_eq!(this.editor.read(cx).value().as_ref(), "A");
            assert_eq!(this.saves.identity, identity);
            identity
        })
    });
    cx.run_until_parked();

    assert!(!cx.has_pending_prompt());
    cx.update(|_, cx| {
        let this = notes.read(cx);
        assert!(!this.is_busy(), "retargeted load leaked its operation gate");
        assert!(!this.notebooks.tree_loading);
        assert!(this.notebooks.tree_target.is_none());
        match target {
            Retarget::NextNote => {
                assert_eq!(
                    this.document.as_ref().unwrap().path.file_name().unwrap(),
                    "c.md"
                );
                assert_eq!(this.editor.read(cx).value().as_ref(), "C");
                assert!(this.image_path.is_none());
                // One installation proves the abandoned B result never replaced A.
                assert_eq!(this.saves.identity, original_identity + 1);
            }
            Retarget::Folder | Retarget::CurrentNote => {
                assert_eq!(
                    this.document.as_ref().unwrap().path.file_name().unwrap(),
                    "a.md"
                );
                assert_eq!(this.editor.read(cx).value().as_ref(), "A");
                assert!(this.image_path.is_none());
                assert_eq!(this.saves.identity, original_identity);
            }
            Retarget::Image => {
                assert!(this.document.is_none());
                assert_eq!(
                    this.image_path.as_ref().unwrap().file_name().unwrap(),
                    "image.png"
                );
                assert_eq!(this.saves.identity, original_identity + 1);
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
