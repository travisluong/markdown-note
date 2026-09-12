//! Retained-buffer autosave checks on GPUI's in-memory TestPlatform.
use super::DirectoryState;
use crate::{Document, Next, Notes, notebook};
use gpui::{Entity, TestAppContext, VisualTestContext, prelude::*};
use gpui_component::Root;
use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
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
            book.settings.autosave = notebook::Autosave {
                enabled: true,
                delay_ms: 100,
            };
            let mut entries = notebook::read_directory(&book.root, Path::new("")).unwrap();
            book.settings.sort_entries(&mut entries);
            notes.notebooks.active = Some(book);
            notes
                .notebooks
                .directories
                .insert(PathBuf::new(), DirectoryState::Ready(entries));
            notes.notebooks.rebuild_rows();
            // Notes::new leaves index persistence disabled; all document writes
            // and the notebook's initial settings stay inside this temp root.
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

fn retained_document_autosaves_after_discarded_navigation(
    cx: &mut TestAppContext,
    select_folder: bool,
) {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    fs::write(root.join("a.md"), "original A").unwrap();
    fs::write(root.join("b.md"), "B").unwrap();
    fs::create_dir(root.join("folder")).unwrap();
    let (notes, cx) = fixture(cx, root);

    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.editor.update(cx, |editor, cx| {
                editor.select_source_range(0..editor.value().len(), window, cx);
                editor.replace_selection_isolated("retained A", window, cx);
            });
        })
    });
    cx.run_until_parked();
    let identity = cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            assert!(this.dirty(cx));
            assert!(this.saves.timer.is_some());
            let book_root = this.notebooks.active.as_ref().unwrap().root.clone();
            this.notebooks.cursor = this
                .notebooks
                .rows
                .iter()
                .position(|row| row.path == Path::new("b.md"));
            this.request(Next::TreeFile(book_root.join("b.md")), window, cx);
            assert!(
                this.saves.timer.is_none(),
                "navigation cancels the old debounce"
            );
            this.saves.identity
        })
    });
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer("Discard");

    // Run the real prompt continuation, stopping before any pending file-open
    // task can install B. One executor tick runs exactly one task and no clock
    // advance is needed to reach the tree-loading state.
    let mut loading = false;
    for _ in 0..1_000 {
        if cx.update(|_, cx| notes.read(cx).notebooks.tree_loading) {
            loading = true;
            break;
        }
        assert!(cx.executor().tick(), "discard continuation did not start B");
    }
    assert!(loading, "discard continuation never entered tree loading");

    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            assert!(this.is_busy());
            assert_eq!(this.saves.identity, identity);
            assert_eq!(this.editor.read(cx).value().as_ref(), "retained A");
            if select_folder {
                this.notebooks.cursor = this.notebooks.rows.iter().position(|row| row.directory);
                this.open_selected_row(window, cx);
                assert!(this.notebooks.tree_target.is_none());
            } else {
                this.tree_move(false, window, cx);
                assert_eq!(
                    this.notebooks.tree_target.as_ref(),
                    this.document.as_ref().map(|document| &document.path)
                );
            }
        })
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        let this = notes.read(cx);
        assert!(!this.is_busy());
        assert!(!this.notebooks.tree_loading);
        assert!(this.notebooks.tree_target.is_none());
        assert_eq!(
            this.saves.identity, identity,
            "abandoned B must never install"
        );
        assert_eq!(
            this.document.as_ref().unwrap().path.file_name().unwrap(),
            "a.md"
        );
        assert!(this.dirty(cx));
    });
    assert_eq!(fs::read_to_string(root.join("a.md")).unwrap(), "original A");

    cx.executor().advance_clock(Duration::from_millis(150));
    cx.run_until_parked();
    assert_eq!(fs::read_to_string(root.join("a.md")).unwrap(), "retained A");
    assert_eq!(fs::read_to_string(root.join("b.md")).unwrap(), "B");
    assert!(!cx.has_pending_prompt());
    cx.update(|_, cx| assert!(!notes.read(cx).dirty(cx)));
}

#[gpui::test]
fn discarded_tree_load_retargeted_to_folder_resumes_retained_autosave(cx: &mut TestAppContext) {
    retained_document_autosaves_after_discarded_navigation(cx, true);
}

#[gpui::test]
fn discarded_tree_load_retargeted_to_current_note_resumes_retained_autosave(
    cx: &mut TestAppContext,
) {
    retained_document_autosaves_after_discarded_navigation(cx, false);
}
