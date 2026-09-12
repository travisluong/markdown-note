//! Close lifecycle on GPUI TestPlatform; no native windows or OS input.
use super::{Document, Notes, ViewMode};
use gpui::{EntityInputHandler, TestAppContext, prelude::*};
use gpui_component::Root;
use std::{
    fs,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

#[gpui::test]
fn clean_close_blocks_input_while_settings_persistence_is_pending(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("note.md");
    fs::write(&path, "original").unwrap();
    cx.update(gpui_component::init);
    let mut notes = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| {
            let mut notes = Notes::new(window, cx);
            notes.saves.session.enabled = false;
            notes.mode = ViewMode::Source;
            notes
        });
        entity.update(cx, |this, cx| {
            this.install_document(Some(Document::open(path.clone()).unwrap()), window, cx);
        });
        notes = Some(entity.clone());
        Root::new(entity, window, cx)
    });
    let notes = notes.unwrap();
    cx.refresh().unwrap();
    cx.run_until_parked();

    let persisted = Arc::new(AtomicBool::new(false));
    cx.update(|_, cx| {
        notes.update(cx, |this, cx| {
            assert!(!this.dirty(cx));
            assert!(!this.is_busy());
            let delay = cx.background_executor().timer(Duration::from_secs(10));
            let persisted = persisted.clone();
            this.notebooks.persistence = Some(cx.spawn(async move |_, _| {
                delay.await;
                persisted.store(true, Ordering::SeqCst);
            }));
        });
    });

    // Invoke the production on_window_should_close callback. It must defer
    // removal until settings finish and make the already rendered editor inert.
    assert!(!cx.simulate_close());
    cx.run_until_parked();
    assert!(!persisted.load(Ordering::SeqCst));
    assert_eq!(cx.windows().len(), 1);
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            assert!(this.is_busy());
            assert!(!this.saves.writing);
            assert!(!this.dirty(cx));
            this.editor.update(cx, |editor, cx| {
                // This is the normal text-input handler, including its disabled
                // check. Do not use set_value or an isolated diagnostic edit.
                editor.replace_text_in_range(Some(8..8), "unexpected", window, cx);
            });
        });
    });
    cx.run_until_parked();
    let text_during_close = cx.update(|_, cx| notes.read(cx).editor.read(cx).value().to_string());

    cx.executor().advance_clock(Duration::from_secs(10));
    cx.run_until_parked();
    assert!(persisted.load(Ordering::SeqCst));
    assert!(
        cx.windows().is_empty(),
        "settings completion must finish close"
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), "original");
    assert_eq!(
        text_during_close, "original",
        "the closing editor accepted text that was never saved before removal"
    );
}
