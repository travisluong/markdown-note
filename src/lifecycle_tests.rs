//! Application lifecycle tests on GPUI's in-memory TestPlatform.
//! Picker outcomes enter the production completion handlers because GPUI's
//! TestPlatform does not implement prompt_for_paths. No native UI is opened.
use super::{Document, Next, Notes, notebook};
use gpui::{Entity, TestAppContext, VisualTestContext, prelude::*};
use gpui_component::Root;
use std::{fs, path::Path, time::Duration};

fn fixture<'a>(
    cx: &'a mut TestAppContext,
    path: &Path,
) -> (Entity<Notes>, &'a mut VisualTestContext) {
    cx.update(gpui_component::init);
    let mut notes = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| {
            let mut notes = Notes::new(window, cx);
            notes.saves.session = notebook::Autosave {
                enabled: true,
                delay_ms: 100,
            };
            notes
        });
        entity.update(cx, |this, cx| {
            this.install_document(
                Some(Document::open(path.to_path_buf()).unwrap()),
                window,
                cx,
            );
        });
        notes = Some(entity.clone());
        Root::new(entity, window, cx)
    });
    (notes.unwrap(), cx)
}

fn edit(notes: &Entity<Notes>, text: &str, cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.editor.update(cx, |editor, cx| {
                editor.select_source_range(0..editor.value().len(), window, cx);
                editor.replace_selection_isolated(text, window, cx);
            });
        })
    });
    cx.run_until_parked();
}

fn elapse(cx: &mut VisualTestContext) {
    cx.executor().advance_clock(Duration::from_millis(150));
    cx.run_until_parked();
}

#[gpui::test]
fn cancelled_file_picker_resumes_retained_dirty_document(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("note.md");
    fs::write(&path, "original").unwrap();
    let (notes, cx) = fixture(cx, &path);
    edit(&notes, "retained", cx);
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            // request() cancels this timer before opening a picker.
            this.saves.timer = None;
            let operation = this.foreground.begin().unwrap();
            this.finish_open(operation, Ok(None), window, cx);
        })
    });
    cx.run_until_parked();
    elapse(cx);
    assert_eq!(fs::read_to_string(path).unwrap(), "retained");
    cx.update(|_, cx| assert!(!notes.read(cx).dirty(cx)));
}

#[gpui::test]
fn folder_picker_outcomes_resume_an_expired_autosave(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("note.md");
    fs::write(&path, "original").unwrap();
    let (notes, cx) = fixture(cx, &path);
    for (text, result) in [
        ("cancelled", Ok(None)),
        ("failed", Err("picker failed".into())),
        ("selected location", Ok(Some(dir.path().to_path_buf()))),
    ] {
        edit(&notes, text, cx);
        let operation =
            cx.update(|_, cx| notes.update(cx, |this, _| this.foreground.begin().unwrap()));
        elapse(cx);
        assert_ne!(fs::read_to_string(&path).unwrap(), text);
        cx.update(|window, cx| {
            notes.update(cx, |this, cx| {
                this.finish_notebook_picker(operation, result, true, window, cx);
            })
        });
        cx.run_until_parked();
        elapse(cx);
        assert_eq!(fs::read_to_string(&path).unwrap(), text);
    }
}

#[gpui::test]
fn failed_notebook_activation_resumes_autosave(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("note.md");
    fs::write(&path, "original").unwrap();
    let (notes, cx) = fixture(cx, &path);
    edit(&notes, "retained", cx);
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.saves.timer = None;
            this.activate_notebook(dir.path().join("missing"), false, window, cx);
        })
    });
    cx.run_until_parked();
    elapse(cx);
    assert_eq!(fs::read_to_string(path).unwrap(), "retained");
    cx.update(|_, cx| assert!(notes.read(cx).notebooks.notice.is_some()));
}

#[gpui::test]
fn failed_file_open_resumes_only_after_error_dismissal(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("note.md");
    fs::write(&path, "original").unwrap();
    let (notes, cx) = fixture(cx, &path);
    edit(&notes, "retained", cx);
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.saves.timer = None;
            let operation = this.foreground.begin().unwrap();
            this.finish_open(
                operation,
                Err(std::io::Error::other("unreadable")),
                window,
                cx,
            );
        })
    });
    cx.run_until_parked();
    elapse(cx);
    assert_eq!(fs::read_to_string(&path).unwrap(), "original");
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer("Ok");
    cx.run_until_parked();
    elapse(cx);
    assert_eq!(fs::read_to_string(&path).unwrap(), "retained");
}

#[gpui::test]
fn cancelling_save_prompt_retains_document_and_resumes_autosave(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("note.md");
    fs::write(&path, "original").unwrap();
    let (notes, cx) = fixture(cx, &path);
    edit(&notes, "retained", cx);
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.request(Next::File(dir.path().join("other.md")), window, cx);
        })
    });
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    elapse(cx);
    assert_eq!(fs::read_to_string(&path).unwrap(), "retained");
    cx.update(|_, cx| {
        assert_eq!(
            notes.read(cx).document.as_ref().unwrap().path,
            fs::canonicalize(&path).unwrap()
        )
    });
}

#[gpui::test]
fn retained_document_keeps_autosave_disabled(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("note.md");
    fs::write(&path, "original").unwrap();
    let (notes, cx) = fixture(cx, &path);
    cx.update(|_, cx| notes.update(cx, |this, _| this.saves.session.enabled = false));
    edit(&notes, "retained", cx);
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            let operation = this.foreground.begin().unwrap();
            this.finish_open(operation, Ok(None), window, cx);
        })
    });
    cx.run_until_parked();
    elapse(cx);
    assert_eq!(fs::read_to_string(&path).unwrap(), "original");
    cx.update(|_, cx| assert!(notes.read(cx).dirty(cx)));
}

#[gpui::test]
fn background_save_finishing_does_not_release_a_picker(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("note.md");
    fs::write(&path, "original").unwrap();
    let (notes, cx) = fixture(cx, &path);
    edit(&notes, "saved snapshot", cx);
    let picker = cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.saves.timer = None;
            this.start_write(None, window, cx);
            assert!(this.saves.writing);
            assert!(!this.is_busy());
            this.foreground.begin().unwrap()
        })
    });
    cx.run_until_parked();
    assert_eq!(fs::read_to_string(&path).unwrap(), "saved snapshot");
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            assert!(!this.saves.writing);
            assert!(!this.dirty(cx));
            assert!(this.foreground.owns(picker));
            assert!(this.is_busy());
            this.finish_notebook_picker(picker, Ok(None), true, window, cx);
            assert!(!this.is_busy());
        })
    });
}

#[gpui::test]
fn queued_manual_save_writes_the_newest_snapshot(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("note.md");
    fs::write(&path, "original").unwrap();
    let (notes, cx) = fixture(cx, &path);
    edit(&notes, "older snapshot", cx);
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.saves.timer = None;
            this.start_write(None, window, cx);
            this.editor.update(cx, |editor, cx| {
                editor.select_source_range(0..editor.value().len(), window, cx);
                editor.replace_selection_isolated("newest snapshot", window, cx);
            });
            this.save(Next::Nothing, window, cx);
            let Some(super::autosave::Pending::Save(operation, _)) = this.saves.pending.as_ref()
            else {
                panic!("manual save must wait for the background writer");
            };
            assert!(this.foreground.owns(*operation));
        })
    });
    cx.run_until_parked();
    assert_eq!(fs::read_to_string(&path).unwrap(), "newest snapshot");
    cx.update(|_, cx| {
        let state = notes.read(cx);
        assert!(!state.dirty(cx));
        assert!(!state.saves.writing);
        assert!(state.saves.pending.is_none());
        assert!(!state.is_busy());
    });
}

#[gpui::test]
fn queued_navigation_releases_its_wait_owner_before_loading(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("note.md");
    let target = dir.path().join("target.md");
    fs::write(&path, "original").unwrap();
    fs::write(&target, "target document").unwrap();
    let (notes, cx) = fixture(cx, &path);
    edit(&notes, "save before navigating", cx);
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.saves.timer = None;
            this.start_write(None, window, cx);
            this.request(Next::File(target.clone()), window, cx);
            let Some(super::autosave::Pending::Request(operation, _)) = this.saves.pending.as_ref()
            else {
                panic!("navigation must wait for the background writer");
            };
            assert!(this.foreground.owns(*operation));
        })
    });
    cx.run_until_parked();
    assert_eq!(fs::read_to_string(&path).unwrap(), "save before navigating");
    cx.update(|_, cx| {
        let state = notes.read(cx);
        assert_eq!(
            state.document.as_ref().unwrap().path,
            fs::canonicalize(target).unwrap()
        );
        assert_eq!(state.editor.read(cx).value().as_ref(), "target document");
        assert!(!state.is_busy());
        assert!(state.saves.pending.is_none());
    });
}

#[gpui::test]
fn stale_pending_navigation_keeps_saved_baseline_and_new_owner(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("note.md");
    let target = dir.path().join("target.md");
    fs::write(&path, "original").unwrap();
    fs::write(&target, "target document").unwrap();
    let (notes, cx) = fixture(cx, &path);
    edit(&notes, "saved snapshot", cx);
    let newer = cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.saves.timer = None;
            this.start_write(None, window, cx);
            this.request(Next::File(target), window, cx);
            let Some(super::autosave::Pending::Request(old, _)) = this.saves.pending.as_ref()
            else {
                panic!("expected queued navigation");
            };
            // Simulate a superseded foreground continuation while the actual write runs.
            assert!(this.foreground.finish(*old));
            this.foreground.begin().unwrap()
        })
    });
    cx.run_until_parked();
    assert_eq!(fs::read_to_string(&path).unwrap(), "saved snapshot");
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            assert_eq!(
                this.document.as_ref().unwrap().path,
                fs::canonicalize(&path).unwrap()
            );
            assert_eq!(this.document.as_ref().unwrap().saved, "saved snapshot");
            assert!(!this.dirty(cx));
            assert!(this.foreground.owns(newer));
            assert!(this.saves.pending.is_none());
            this.finish_open(newer, Ok(None), window, cx);
        })
    });
}

#[gpui::test]
fn stale_and_duplicate_foreground_completions_cannot_install_documents(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("note.md");
    let target = dir.path().join("target.md");
    fs::write(&path, "original").unwrap();
    fs::write(&target, "target document").unwrap();
    let (notes, cx) = fixture(cx, &path);
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            let old = this.foreground.begin().unwrap();
            this.finish_open(old, Ok(None), window, cx);
            let newer = this.foreground.begin().unwrap();
            this.finish_open(
                old,
                Ok(Some(Document::open(target.clone()).unwrap())),
                window,
                cx,
            );
            this.answer_request(old, Some(1), Next::File(target.clone()), window, cx);
            this.finish_notebook_picker(old, Ok(Some(dir.path().to_path_buf())), false, window, cx);
            assert!(this.foreground.owns(newer));
            assert_eq!(
                this.document.as_ref().unwrap().path,
                fs::canonicalize(&path).unwrap()
            );
            this.finish_open(newer, Ok(None), window, cx);
            this.finish_open(newer, Ok(Some(Document::open(target).unwrap())), window, cx);
            assert!(!this.is_busy());
            assert_eq!(
                this.document.as_ref().unwrap().path,
                fs::canonicalize(&path).unwrap()
            );
        })
    });
    cx.run_until_parked();
    assert!(!cx.has_pending_prompt());
}

#[gpui::test]
fn moving_a_note_preserves_preferences_changed_while_the_move_is_pending(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    let mut book = notebook::Notebook::open(&directory.path().join("Book"), true).unwrap();
    book.settings.autosave.enabled = false;
    let root = book.root.clone();
    let path = root.join("note.md");
    fs::write(&path, "original").unwrap();
    fs::create_dir(root.join("Archive")).unwrap();
    let (notes, cx) = fixture(cx, &path);
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.notebooks.active = Some(book);
            this.drop_entry(
                &super::notebook_ui::EntryDrag {
                    root: root.clone(),
                    path: "note.md".into(),
                },
                "Archive".into(),
                window,
                cx,
            );
            assert!(this.is_busy());
            // The View menu remains usable while the filesystem worker runs.
            this.set_mode(super::ViewMode::Source, window, cx);
            assert_eq!(
                this.notebooks.active.as_ref().unwrap().settings.view_mode,
                super::ViewMode::Source
            );
        });
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        let notes = notes.read(cx);
        assert!(!notes.is_busy());
        assert_eq!(
            notes.document.as_ref().unwrap().path,
            root.join("Archive/note.md")
        );
        assert_eq!(notes.mode, super::ViewMode::Source);
        assert_eq!(
            notes.notebooks.active.as_ref().unwrap().settings.view_mode,
            notes.mode
        );
    });
}
