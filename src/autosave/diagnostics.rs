//! Runs real timers and atomic writes against disposable files.
use super::*;
use std::time::Instant;

pub(crate) fn start(notes: Entity<Notes>, output: PathBuf, window: &mut Window, cx: &mut App) {
    let panic_path = output.with_extension("panic.txt");
    std::panic::set_hook(Box::new(move |info| {
        let _ = fs::write(&panic_path, info.to_string());
    }));
    let fixture = tempfile::tempdir().unwrap();
    let path = fixture.path().join("save.md");
    fs::write(&path, b"\xef\xbb\xbfstart\r\n").unwrap();
    notes.update(cx, |this, cx| {
        this.install_document(Some(Document::open(path.clone()).unwrap()), window, cx);
        this.saves.session = notebook::Autosave {
            enabled: true,
            delay_ms: 100,
        };
    });
    window.spawn(cx, async move |cx| {
        let started = Instant::now();
        for step in 0..=13 {
            fs::write(output.with_extension("progress.txt"), format!("step {step}")).unwrap();
            loop {
                Timer::after(Duration::from_millis(250)).await;
                assert!(started.elapsed().as_secs() < 45, "autosave timeout at {step}");
                if notes.read_with(cx, |this, _| !this.saves.writing && !this.is_busy()).unwrap() { break; }
            }
            cx.update(|window, cx| notes.update(cx, |this, cx| {
                match step {
                    0 => {
                        this.editor.update(cx, |e, cx| e.set_value("cancelled\n", window, cx));
                        this.schedule_autosave(window, cx);
                        let mut settings = this.autosave_settings(); settings.enabled = false;
                        this.update_autosave(settings, window, cx);
                    }
                    1 => {
                        assert_eq!(Document::open(path.clone()).unwrap().saved, "start\n");
                        this.editor.update(cx, |e, cx| e.set_value("snapshot A\n", window, cx));
                        this.start_write(None, window, cx);
                        assert!(!this.is_busy());
                        this.editor.update(cx, |e, cx| e.set_value("newer B\n", window, cx));
                    }
                    2 => {
                        assert_eq!(Document::open(path.clone()).unwrap().saved, "snapshot A\n");
                        assert_eq!(this.document.as_ref().unwrap().saved, "snapshot A\n");
                        assert!(this.dirty(cx));
                        this.update_autosave(notebook::Autosave { enabled: true, delay_ms: 1000 }, window, cx);
                        this.update_autosave(notebook::Autosave { enabled: true, delay_ms: 100 }, window, cx);
                    }
                    3 => {
                        assert_eq!(fs::read(&path).unwrap(), b"\xef\xbb\xbfnewer B\r\n");
                        assert!(!this.dirty(cx));
                        this.editor.update(cx, |e, cx| e.set_value("older\n", window, cx));
                        this.start_write(None, window, cx);
                        this.editor.update(cx, |e, cx| e.set_value("manual newest\n", window, cx));
                        this.save(Next::Nothing, window, cx);
                        assert!(matches!(this.saves.pending, Some(Pending::Save(_, _))));
                    }
                    4 => {
                        assert_eq!(Document::open(path.clone()).unwrap().saved, "manual newest\n");
                        assert!(!this.dirty(cx));
                        this.editor.update(cx, |e, cx| e.set_value("before navigation\n", window, cx));
                        this.start_write(None, window, cx);
                        this.request(Next::File(path.clone()), window, cx);
                        assert!(matches!(this.saves.pending, Some(Pending::Request(_, _))));
                    }
                    5 => {
                        assert_eq!(this.editor.read(cx).value().as_str(), "before navigation\n");
                        assert!(!this.dirty(cx));
                        // A directory at the target makes atomic replacement fail.
                        fs::rename(&path, fixture.path().join("backup.md")).unwrap();
                        fs::create_dir(&path).unwrap();
                        this.editor.update(cx, |e, cx| e.set_value("recover me\n", window, cx));
                        this.schedule_autosave(window, cx);
                    }
                    6 => {
                        assert!(this.saves.error.is_some());
                        assert!(this.dirty(cx));
                        assert!(!this.is_busy());
                        fs::remove_dir(&path).unwrap();
                        fs::rename(fixture.path().join("backup.md"), &path).unwrap();
                        let document = this.document.as_mut().unwrap();
                        document.version = document::version_after_move(&path, &document.version).unwrap();
                        this.save(Next::Nothing, window, cx);
                    }
                    7 => {
                        assert_eq!(Document::open(path.clone()).unwrap().saved, "recover me\n");
                        assert!(!this.dirty(cx));
                        assert!(this.saves.error.is_none());
                        this.open_settings(window, cx);
                        this.autosave_delay.clone().unwrap().update(cx, |e, cx| e.set_value("99", window, cx));
                    }
                    8 => {
                        assert!(this.autosave_validation.is_some());
                        assert_eq!(this.autosave_settings().delay_ms, 100);
                        window.close_dialog(cx);
                        this.editor.update(cx, |e, cx| e.set_value("discarded\n", window, cx));
                        this.schedule_autosave(window, cx);
                        this.diagnostic_answer_request(Some(1), Next::File(path.clone()), window, cx);
                    }
                    9 => {
                        assert_eq!(Document::open(path.clone()).unwrap().saved, "recover me\n");
                        assert_eq!(this.editor.read(cx).value().as_str(), "recover me\n");
                        this.editor.update(cx, |e, cx| e.set_value("- [X] task\n", window, cx));
                        this.set_mode(ViewMode::Reading, window, cx);
                        this.editor.update(cx, |e, cx| { assert!(e.toggle_markdown_task(3, "- [X] task\n", window, cx)); });
                    }
                    10 => {
                        assert_eq!(this.reading_text.as_str(), "- [ ] task\n");
                        assert_eq!(Document::open(path.clone()).unwrap().saved, "- [ ] task\n");
                        this.editor.update(cx, |e, cx| e.set_value("debounce one", window, cx));
                        this.update_autosave(notebook::Autosave { enabled: true, delay_ms: 1000 }, window, cx);
                    }
                    11 => {
                        assert_eq!(Document::open(path.clone()).unwrap().saved, "- [ ] task\n");
                        this.editor.update(cx, |e, cx| e.set_value("debounce two", window, cx));
                        this.update_autosave(notebook::Autosave { enabled: true, delay_ms: 100 }, window, cx);
                    }
                    12 => {
                        assert_eq!(Document::open(path.clone()).unwrap().saved, "debounce two");
                        this.set_mode(ViewMode::LivePreview,window,cx);
                        this.editor.update(cx,|e,cx| e.diagnostic_table_autosave(window,cx));
                    }
                    13 => {
                        let saved = Document::open(path.clone()).unwrap().saved;
                        assert_eq!(saved,this.editor.read(cx).value().as_str());
                        assert!(saved.contains("edited x"));
                        assert!(!this.dirty(cx));
                    },
                    _ => unreachable!(),
                }
            })).unwrap();
        }
        fs::write(output, "passed: cancellation, debounce rescheduling, typing during save, saved snapshots, queued manual saves, navigation serialization, BOM/CRLF, nonmodal failure recovery, validation, discard cancellation, reading task refresh, editable table cells/rows and autosave\n").unwrap();
        drop(fixture);
        cx.update(|_, cx| cx.quit()).unwrap();
    }).detach();
}
