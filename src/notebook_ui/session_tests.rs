//! Session adapter decisions that also change notebook presentation.
use super::persistence_tests::fixture;
use crate::notebook;
use gpui::TestAppContext;
use std::{fs, time::Duration};

#[gpui::test]
fn removing_inactive_notebook_keeps_dirty_document_and_its_autosave(cx: &mut TestAppContext) {
    let (fixture, notes, cx) = fixture(cx);
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.notebooks.index.register(fixture.other.clone());
            this.notebooks.index.register(fixture.root.clone());
            this.update_autosave(
                notebook::Autosave {
                    enabled: true,
                    delay_ms: 100,
                },
                window,
                cx,
            );
            this.editor.update(cx, |editor, cx| {
                editor.select_source_range(0..editor.value().len(), window, cx);
                editor.replace_selection_isolated("retained A", window, cx);
            });
        })
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            let identity = this.document_id();
            let autosave = this.session.autosave();
            assert!(autosave.is_some());
            this.remove_notebook(fixture.other.clone(), window, cx);
            assert!(!this.notebooks.registered_paths().contains(&fixture.other));
            assert_eq!(this.notebooks.active.as_ref().unwrap().root, fixture.root);
            assert_eq!(this.document_id(), identity);
            assert_eq!(this.session.autosave(), autosave);
            assert!(this.session.write().is_none());
            assert!(!this.is_busy());
            assert!(this.dirty(cx));
            assert_eq!(this.editor.read(cx).value().as_ref(), "retained A");
        })
    });
    cx.run_until_parked();
    assert!(!cx.has_pending_prompt());
    assert_eq!(fs::read_to_string(&fixture.note).unwrap(), "original A");
    cx.executor().advance_clock(Duration::from_millis(150));
    cx.run_until_parked();
    assert_eq!(fs::read_to_string(&fixture.note).unwrap(), "retained A");
    cx.update(|_, cx| assert!(!notes.read(cx).dirty(cx)));
}
