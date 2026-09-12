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
            this.busy = true;
            this.finish_open(Ok(None), window, cx);
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
        cx.update(|_, cx| notes.update(cx, |this, _| this.busy = true));
        elapse(cx);
        assert_ne!(fs::read_to_string(&path).unwrap(), text);
        cx.update(|window, cx| {
            notes.update(cx, |this, cx| {
                this.finish_notebook_picker(result, true, window, cx);
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
            this.finish_open(Err(std::io::Error::other("unreadable")), window, cx);
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
    cx.update(|window, cx| notes.update(cx, |this, cx| this.finish_open(Ok(None), window, cx)));
    cx.run_until_parked();
    elapse(cx);
    assert_eq!(fs::read_to_string(&path).unwrap(), "original");
    cx.update(|_, cx| assert!(notes.read(cx).dirty(cx)));
}
