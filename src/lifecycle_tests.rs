//! Application lifecycle tests on GPUI's in-memory TestPlatform.
//! Picker outcomes enter the production completion handlers because GPUI's
//! TestPlatform does not implement prompt_for_paths. No native UI is opened.
use super::{Document, Next, Notes, notebook};
use crate::app::session::{Effect, OperationId, PhaseKind};
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

fn begin_picker(
    this: &mut Notes,
    window: &mut gpui::Window,
    cx: &mut gpui::Context<Notes>,
) -> OperationId {
    let effects = this.session.pick();
    let operation = this.session.foreground().unwrap().0;
    for effect in effects {
        if !matches!(effect, Effect::Pick(_)) {
            this.apply_session_effects(vec![effect], window, cx);
        }
    }
    operation
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
            let operation = begin_picker(this, window, cx);
            this.finish_file_picker(operation, Ok(None), window, cx);
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
            cx.update(|window, cx| notes.update(cx, |this, cx| begin_picker(this, window, cx)));
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
            this.diagnostic_answer_request(
                Some(1),
                Next::Notebook {
                    path: dir.path().join("missing"),
                    create: false,
                },
                window,
                cx,
            );
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
            let operation = begin_picker(this, window, cx);
            this.finish_file_picker(
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
            notes.read(cx).document().unwrap().path,
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
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            let mut settings = this.autosave_settings();
            settings.enabled = false;
            this.update_autosave(settings, window, cx);
        })
    });
    edit(&notes, "retained", cx);
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            let operation = begin_picker(this, window, cx);
            this.finish_file_picker(operation, Ok(None), window, cx);
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
            assert!(this.session.write().is_some());
            assert!(!this.is_busy());
            begin_picker(this, window, cx)
        })
    });
    cx.run_until_parked();
    assert_eq!(fs::read_to_string(&path).unwrap(), "saved snapshot");
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            assert!(!this.session.write().is_some());
            assert!(!this.dirty(cx));
            assert!(
                this.session
                    .foreground()
                    .is_some_and(|(id, _)| id == picker)
            );
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
            assert!(matches!(
                this.session.foreground(),
                Some((_, PhaseKind::WaitingForWrite))
            ));
        })
    });
    cx.run_until_parked();
    assert_eq!(fs::read_to_string(&path).unwrap(), "newest snapshot");
    cx.update(|_, cx| {
        let state = notes.read(cx);
        assert!(!state.dirty(cx));
        assert!(!state.session.write().is_some());
        assert!(state.session.foreground().is_none());
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
            assert!(matches!(
                this.session.foreground(),
                Some((_, PhaseKind::WaitingForWrite))
            ));
        })
    });
    cx.run_until_parked();
    assert_eq!(fs::read_to_string(&path).unwrap(), "save before navigating");
    cx.update(|_, cx| {
        let state = notes.read(cx);
        assert_eq!(
            state.document().unwrap().path,
            fs::canonicalize(target).unwrap()
        );
        assert_eq!(state.editor.read(cx).value().as_ref(), "target document");
        assert!(!state.is_busy());
        assert!(state.session.foreground().is_none());
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
            let old = this.session.foreground().unwrap().0;
            let observed = this.observed_document(cx);
            let effects = this.session.cancel(old, observed);
            this.apply_session_effects(effects, window, cx);
            begin_picker(this, window, cx)
        })
    });
    cx.run_until_parked();
    assert_eq!(fs::read_to_string(&path).unwrap(), "saved snapshot");
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            assert_eq!(
                this.document().unwrap().path,
                fs::canonicalize(&path).unwrap()
            );
            assert_eq!(this.document().unwrap().saved, "saved snapshot");
            assert!(!this.dirty(cx));
            assert!(this.session.foreground().is_some_and(|(id, _)| id == newer));
            assert!(this.session.write().is_none());
            this.finish_file_picker(newer, Ok(None), window, cx);
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
            let old = begin_picker(this, window, cx);
            this.finish_file_picker(old, Ok(None), window, cx);
            let newer = begin_picker(this, window, cx);
            this.finish_file_picker(old, Ok(Some(target.clone())), window, cx);
            this.answer_request(old, Some(1), window, cx);
            this.finish_notebook_picker(old, Ok(Some(dir.path().to_path_buf())), false, window, cx);
            assert!(this.session.foreground().is_some_and(|(id, _)| id == newer));
            assert_eq!(
                this.document().unwrap().path,
                fs::canonicalize(&path).unwrap()
            );
            this.finish_file_picker(newer, Ok(None), window, cx);
            this.finish_file_picker(newer, Ok(Some(target)), window, cx);
            assert!(!this.is_busy());
            assert_eq!(
                this.document().unwrap().path,
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
        assert_eq!(notes.document().unwrap().path, root.join("Archive/note.md"));
        assert_eq!(notes.mode, super::ViewMode::Source);
        assert_eq!(
            notes.notebooks.active.as_ref().unwrap().settings.view_mode,
            notes.mode
        );
    });
}

#[gpui::test]
fn disk_conflict_retains_text_and_pauses_autosave_until_explicit_save(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("note.md");
    fs::write(&path, "original").unwrap();
    let (notes, cx) = fixture(cx, &path);
    fs::write(&path, "external").unwrap();
    edit(&notes, "retained edit", cx);
    elapse(cx);
    assert_eq!(fs::read_to_string(&path).unwrap(), "external");
    cx.update(|_, cx| {
        let notes = notes.read(cx);
        assert!(notes.session.save_conflicted());
        assert!(notes.saves.error.is_some());
        assert!(notes.dirty(cx));
        assert_eq!(notes.document().unwrap().saved, "original");
        assert_eq!(notes.editor.read(cx).value().as_ref(), "retained edit");
    });
    // Restoring the matching disk bytes does not silently resume a paused save.
    fs::write(&path, "original").unwrap();
    edit(&notes, "newer retained edit", cx);
    elapse(cx);
    assert_eq!(fs::read_to_string(&path).unwrap(), "original");
    cx.update(|window, cx| notes.update(cx, |this, cx| this.save(Next::Nothing, window, cx)));
    cx.run_until_parked();
    assert_eq!(fs::read_to_string(&path).unwrap(), "newer retained edit");
    cx.update(|_, cx| {
        let notes = notes.read(cx);
        assert!(!notes.session.save_conflicted());
        assert!(notes.saves.error.is_none());
        assert!(!notes.dirty(cx));
    });
    edit(&notes, "autosave after receipt", cx);
    elapse(cx);
    assert_eq!(fs::read_to_string(&path).unwrap(), "autosave after receipt");
}

#[gpui::test]
fn manual_save_checks_disk_version_even_when_editor_is_clean(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("note.md");
    fs::write(&path, "original").unwrap();
    let (notes, cx) = fixture(cx, &path);
    fs::write(&path, "external").unwrap();
    cx.update(|window, cx| notes.update(cx, |this, cx| this.save(Next::Nothing, window, cx)));
    cx.run_until_parked();
    assert_eq!(fs::read_to_string(&path).unwrap(), "external");
    cx.update(|_, cx| {
        let notes = notes.read(cx);
        assert!(notes.session.save_conflicted());
        assert!(!notes.dirty(cx));
        assert_eq!(notes.editor.read(cx).value().as_ref(), "original");
    });
}

#[gpui::test]
fn conflict_completion_cancels_autosave_queued_while_the_write_was_running(
    cx: &mut TestAppContext,
) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("note.md");
    fs::write(&path, "original").unwrap();
    let (notes, cx) = fixture(cx, &path);
    edit(&notes, "retained edit", cx);
    fs::write(&path, "external").unwrap();
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.start_write(None, window, cx);
            this.schedule_autosave(window, cx);
            assert!(this.session.write().is_some());
        })
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        let notes = notes.read(cx);
        assert!(notes.session.save_conflicted());
        assert!(notes.saves.timer.is_none());
    });
    fs::write(&path, "original").unwrap();
    elapse(cx);
    assert_eq!(fs::read_to_string(&path).unwrap(), "original");
    cx.update(|_, cx| assert!(notes.read(cx).dirty(cx)));
}

#[cfg(windows)]
#[gpui::test]
fn io_failure_during_manual_conflict_retry_does_not_resume_automatic_saves(
    cx: &mut TestAppContext,
) {
    use std::os::windows::fs::OpenOptionsExt;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("note.md");
    fs::write(&path, "original").unwrap();
    let (notes, cx) = fixture(cx, &path);
    fs::write(&path, "external").unwrap();
    edit(&notes, "retained edit", cx);
    elapse(cx);
    fs::write(&path, "original").unwrap();
    let lock = fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&path)
        .unwrap();
    cx.update(|window, cx| notes.update(cx, |this, cx| this.save(Next::Nothing, window, cx)));
    cx.run_until_parked();
    cx.update(|_, cx| assert!(notes.read(cx).session.save_conflicted()));
    drop(lock);
    edit(&notes, "later retained edit", cx);
    elapse(cx);
    assert_eq!(fs::read_to_string(&path).unwrap(), "original");
    cx.update(|window, cx| notes.update(cx, |this, cx| this.save(Next::Nothing, window, cx)));
    cx.run_until_parked();
    assert_eq!(fs::read_to_string(&path).unwrap(), "later retained edit");
    cx.update(|_, cx| assert!(!notes.read(cx).session.save_conflicted()));
}

#[gpui::test]
fn move_retains_unsaved_history_and_revalidates_the_saved_file_version(cx: &mut TestAppContext) {
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
            this.set_mode(super::ViewMode::Source, window, cx);
        })
    });
    edit(&notes, "retained edit", cx);
    let identity = cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            let identity = this.document_id();
            this.drop_entry(
                &super::notebook_ui::EntryDrag {
                    root: root.clone(),
                    path: "note.md".into(),
                },
                "Archive".into(),
                window,
                cx,
            );
            identity
        })
    });
    cx.run_until_parked();
    let moved = root.join("Archive/note.md");
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            assert_eq!(this.document_id(), identity);
            assert_eq!(this.document().unwrap().path, moved);
            assert!(this.dirty(cx));
            assert_eq!(this.editor.read(cx).value().as_ref(), "retained edit");
            this.save(Next::Nothing, window, cx);
        })
    });
    cx.run_until_parked();
    assert_eq!(fs::read_to_string(&moved).unwrap(), "retained edit");
    assert!(!path.exists());
    cx.dispatch_action(gpui_component::input::Undo);
    cx.run_until_parked();
    cx.update(|_, cx| {
        let notes = notes.read(cx);
        assert_eq!(notes.editor.read(cx).value().as_ref(), "original");
        assert!(notes.dirty(cx));
        assert_eq!(notes.document_id(), identity);
        assert!(!notes.session.save_conflicted());
    });
}

#[gpui::test]
fn stale_autosave_arm_effect_cannot_replace_the_latest_timer(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("note.md");
    fs::write(&path, "original").unwrap();
    let (notes, cx) = fixture(cx, &path);
    edit(&notes, "first edit", cx);
    let old = cx.update(|_, cx| notes.read(cx).session.autosave().unwrap());
    edit(&notes, "latest edit", cx);
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            let current = this.session.autosave().unwrap();
            assert_ne!(old, current);
            this.arm_session_autosave(old, window, cx);
            assert_eq!(this.session.autosave(), Some(current));
        })
    });
    elapse(cx);
    assert_eq!(fs::read_to_string(&path).unwrap(), "latest edit");
    cx.update(|_, cx| assert!(!notes.read(cx).dirty(cx)));
}
