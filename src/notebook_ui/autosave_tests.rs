//! Retained-buffer autosave checks on GPUI's in-memory TestPlatform.
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
            let selection = this
                .notebooks
                .explorer
                .rows()
                .iter()
                .position(|row| row.path == Path::new("b.md"));
            this.notebooks.explorer.select(selection);
            this.request(Next::TreeFile(book_root.join("b.md")), window, cx);
            assert!(
                this.saves.timer.is_none(),
                "navigation cancels the old debounce"
            );
            this.document_id()
        })
    });
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer("Discard");

    // Run the real prompt continuation, stopping before any pending file-open
    // task can install B. One executor tick runs exactly one task and no clock
    // advance is needed to reach the tree-loading state.
    let mut loading = false;
    for _ in 0..1_000 {
        if cx.update(|_, cx| notes.read(cx).tree_loading()) {
            loading = true;
            break;
        }
        assert!(cx.executor().tick(), "discard continuation did not start B");
    }
    assert!(loading, "discard continuation never entered tree loading");

    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            assert!(this.is_busy());
            assert_eq!(this.document_id(), identity);
            assert_eq!(this.editor.read(cx).value().as_ref(), "retained A");
            if select_folder {
                let selection = this
                    .notebooks
                    .explorer
                    .rows()
                    .iter()
                    .position(|row| row.directory);
                this.notebooks.explorer.select(selection);
                this.open_selected_row(window, cx);
            } else {
                this.tree_move(false, window, cx);
            }
        })
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        let this = notes.read(cx);
        assert!(!this.is_busy());
        assert!(!this.tree_loading());
        assert!(this.notebooks.tree_operation.is_none());
        assert_eq!(
            this.document_id(),
            identity,
            "abandoned B must never install"
        );
        assert_eq!(this.document().unwrap().path.file_name().unwrap(), "a.md");
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

#[gpui::test]
fn delayed_notebook_load_failure_rearms_the_retained_document_once(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("Current");
    let target = directory.path().join("Broken");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("a.md"), "original A").unwrap();
    let broken = notebook::Notebook::open(&target, true).unwrap();
    fs::write(notebook::settings_path(&broken.root), "invalid settings").unwrap();
    let (notes, cx) = fixture(cx, &root);
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.editor.update(cx, |editor, cx| {
                editor.select_source_range(0..editor.value().len(), window, cx);
                editor.replace_selection_isolated("retained A", window, cx);
            });
        })
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.notebooks.activation_install_delay = Duration::from_secs(2);
            this.diagnostic_answer_request(
                Some(1),
                Next::Notebook {
                    path: target,
                    create: false,
                },
                window,
                cx,
            );
        })
    });
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(150));
    cx.run_until_parked();
    assert_eq!(fs::read_to_string(root.join("a.md")).unwrap(), "original A");
    cx.update(|_, cx| assert!(notes.read(cx).is_busy()));
    cx.executor().advance_clock(Duration::from_millis(1_850));
    cx.run_until_parked();
    cx.update(|_, cx| {
        let notes = notes.read(cx);
        assert!(!notes.is_busy());
        assert!(notes.notebooks.notice.is_some());
        assert!(notes.session.autosave().is_some());
        assert!(notes.dirty(cx));
    });
    cx.executor().advance_clock(Duration::from_millis(150));
    cx.run_until_parked();
    assert_eq!(fs::read_to_string(root.join("a.md")).unwrap(), "retained A");
    assert_eq!(
        fs::read_to_string(notebook::settings_path(&broken.root)).unwrap(),
        "invalid settings"
    );
    cx.update(|_, cx| assert!(!notes.read(cx).dirty(cx)));
}
