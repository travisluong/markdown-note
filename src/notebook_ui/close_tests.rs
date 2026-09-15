//! Close lifecycle on GPUI TestPlatform; no native windows or OS input.
use super::persistence_tests::{fail_index_write, fixture};
use crate::{ViewMode, notebook};
use gpui::{EntityInputHandler, TestAppContext};
use std::{fs, time::Duration};

#[gpui::test]
fn clean_close_blocks_input_and_includes_preferences_changed_while_waiting(
    cx: &mut TestAppContext,
) {
    let (fixture, notes, cx) = fixture(cx);
    cx.update(|_, cx| {
        notes.update(cx, |this, cx| {
            this.notebooks.write_delay = Duration::from_secs(10);
            this.notebooks.active.as_mut().unwrap().settings.date_format = "MMMM Do, YYYY".into();
            this.persist_notebooks(cx);
        });
    });
    assert!(!cx.simulate_close());
    cx.run_until_parked();
    assert_eq!(cx.windows().len(), 1);
    assert_eq!(
        notebook::Notebook::open(&fixture.root, false)
            .unwrap()
            .settings
            .date_format,
        "YYYY-MM-DD"
    );
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            assert!(this.is_busy());
            assert!(!this.dirty(cx));
            this.editor.diagnostic_input().update(cx, |editor, cx| {
                // Exercise ordinary input, including its rendered disabled check.
                editor.replace_text_in_range(Some(10..10), "unexpected", window, cx);
            });
            this.set_mode(ViewMode::Reading, window, cx);
        });
    });
    cx.run_until_parked();
    let text_during_close = cx.update(|_, cx| notes.read(cx).editor.text(cx).to_string());
    cx.executor().advance_clock(Duration::from_secs(10));
    cx.run_until_parked();
    assert!(cx.windows().is_empty());
    assert_eq!(text_during_close, "original A");
    assert_eq!(fs::read_to_string(&fixture.note).unwrap(), "original A");
    let saved = notebook::Notebook::open(&fixture.root, false)
        .unwrap()
        .settings;
    assert_eq!(saved.date_format, "MMMM Do, YYYY");
    assert_eq!(saved.view_mode, ViewMode::Reading);
}

#[gpui::test]
fn close_retries_latest_preferences_after_the_index_is_repaired(cx: &mut TestAppContext) {
    let (fixture, notes, cx) = fixture(cx);
    fail_index_write(&fixture, &notes, cx);
    assert!(!cx.simulate_close());
    cx.run_until_parked();
    assert!(cx.has_pending_prompt());
    assert_eq!(cx.windows().len(), 1);
    fs::write(
        &fixture.index,
        serde_json::to_vec(&notebook::Index::default()).unwrap(),
    )
    .unwrap();
    cx.simulate_prompt_answer("Retry");
    cx.run_until_parked();
    assert!(cx.windows().is_empty());
    assert_eq!(
        notebook::load_index(&fixture.index).unwrap().active,
        Some(fixture.root.clone())
    );
    assert_eq!(
        notebook::Notebook::open(&fixture.root, false)
            .unwrap()
            .settings
            .date_format,
        "MMMM Do, YYYY"
    );
}

#[gpui::test]
fn cancelling_preferences_failure_after_discard_retains_and_autosaves_the_note(
    cx: &mut TestAppContext,
) {
    let (fixture, notes, cx) = fixture(cx);
    fail_index_write(&fixture, &notes, cx);
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.update_autosave(
                notebook::Autosave {
                    enabled: true,
                    delay_ms: 100,
                },
                window,
                cx,
            );
            this.editor.diagnostic_input().update(cx, |editor, cx| {
                editor.select_source_range(0..editor.value().len(), window, cx);
                editor.replace_selection_isolated("retained A", window, cx);
            });
        });
    });
    cx.run_until_parked();
    assert!(!cx.simulate_close());
    cx.run_until_parked();
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer("Discard");
    cx.run_until_parked();
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    cx.update(|_, cx| {
        let notes = notes.read(cx);
        assert!(!notes.is_busy());
        assert!(notes.dirty(cx));
        assert!(notes.active_saves().timer.is_some());
        assert_eq!(notes.editor.text(cx).as_ref(), "retained A");
    });
    cx.executor().advance_clock(Duration::from_millis(150));
    cx.run_until_parked();
    assert_eq!(cx.windows().len(), 1);
    assert_eq!(fs::read_to_string(&fixture.note).unwrap(), "retained A");
    assert_eq!(fs::read_to_string(&fixture.index).unwrap(), "corrupt index");
}

#[gpui::test]
fn close_without_preferences_preserves_corruption_and_drains_late_submissions(
    cx: &mut TestAppContext,
) {
    let (fixture, notes, cx) = fixture(cx);
    fail_index_write(&fixture, &notes, cx);
    assert!(!cx.simulate_close());
    cx.run_until_parked();
    assert!(cx.has_pending_prompt());
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.notebooks.write_delay = Duration::from_secs(2);
            this.set_mode(ViewMode::Reading, window, cx);
        });
    });
    cx.simulate_prompt_answer("Close without saving preferences");
    cx.run_until_parked();
    assert_eq!(
        cx.windows().len(),
        1,
        "a late captured job must finish before removal"
    );
    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();
    assert!(cx.windows().is_empty());
    assert_eq!(fs::read_to_string(&fixture.index).unwrap(), "corrupt index");
    assert_eq!(fs::read_to_string(&fixture.note).unwrap(), "original A");
    assert_eq!(
        notebook::Notebook::open(&fixture.root, false)
            .unwrap()
            .settings
            .view_mode,
        ViewMode::Reading
    );
}
