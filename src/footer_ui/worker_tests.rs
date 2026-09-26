//! Delayed completions exercise the production queue on GPUI TestPlatform.
use super::*;
use crate::{Document, NotesWindow};
use gpui::{Entity, TestAppContext, VisualTestContext};
use gpui_component::Root;

fn fixture(cx: &mut TestAppContext) -> (tempfile::TempDir, Entity<Notes>, &mut VisualTestContext) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("note.md");
    std::fs::write(&path, "original").unwrap();
    cx.update(gpui_component::init);
    let mut notes = None;
    let (_, view) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| Notes::new(window, cx));
        entity.update(cx, |this, cx| {
            this.session_autosave.enabled = false;
            this.install_document(Some(Document::open(path).unwrap()), window, cx);
        });
        notes = Some(entity.clone());
        Root::new(
            cx.new(|cx| NotesWindow {
                _subscription: cx.observe(&entity, |_, _, cx| cx.notify()),
                notes: entity,
            }),
            window,
            cx,
        )
    });
    view.run_until_parked();
    view.refresh().unwrap();
    (directory, notes.unwrap(), view)
}

fn replace(notes: &Entity<Notes>, text: &str, cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            let editor = &this.tabs.active.editor;
            editor.select(0..editor.snapshot(cx).len(), window, cx);
            editor.replace_selection(text, window, cx);
        })
    });
    cx.run_until_parked();
}

#[gpui::test]
fn latest_edit_wins_with_one_running_and_one_pending_job(cx: &mut TestAppContext) {
    let (_directory, notes, cx) = fixture(cx);
    cx.update(|_, cx| {
        notes.update(cx, |this, _| {
            this.documents
                .values_mut()
                .next()
                .unwrap()
                .counts
                .completion_delay = Duration::from_secs(2);
        })
    });
    replace(&notes, "stale work", cx);
    cx.executor().advance_clock(Duration::from_millis(100));
    cx.run_until_parked();
    let running_jobs = cx.update(|_, cx| {
        notes
            .read(cx)
            .documents
            .values()
            .next()
            .unwrap()
            .counts
            .jobs_started
    });
    for text in ["second", "third", "latest accepted text"] {
        replace(&notes, text, cx);
    }
    cx.update(|_, cx| {
        let this = notes.read(cx);
        let state = &this.documents.values().next().unwrap().counts;
        assert_eq!(state.jobs_started, running_jobs);
        assert_eq!(
            state.pending.as_ref().unwrap().snapshot.to_string(),
            "latest accepted text"
        );
        assert_eq!(
            this.active_document_counts(FooterCountMode::Source)
                .unwrap()
                .characters,
            8
        );
    });
    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(
            notes
                .read(cx)
                .active_document_counts(FooterCountMode::Source)
                .unwrap()
                .characters,
            8,
            "stale completion must not replace the previous displayed count"
        );
    });
    cx.executor().advance_clock(Duration::from_millis(100));
    cx.run_until_parked();
    cx.update(|_, cx| {
        let this = notes.read(cx);
        let state = &this.documents.values().next().unwrap().counts;
        assert_eq!(state.jobs_started, running_jobs + 1);
        assert!(state.pending.is_none());
        assert!(state.task.is_none());
        assert_eq!(
            this.active_document_counts(FooterCountMode::Source),
            Some(DocumentCounts {
                words: 3,
                characters: 20
            })
        );
    });
}

#[gpui::test]
fn closed_document_completion_cannot_replace_reopened_counts(cx: &mut TestAppContext) {
    let (directory, notes, cx) = fixture(cx);
    cx.update(|_, cx| {
        notes.update(cx, |this, _| {
            this.documents
                .values_mut()
                .next()
                .unwrap()
                .counts
                .completion_delay = Duration::from_secs(2);
        })
    });
    replace(&notes, "abandoned count", cx);
    cx.executor().advance_clock(Duration::from_millis(100));
    cx.run_until_parked();
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.install_document(None, window, cx);
            assert!(this.documents.is_empty());
            this.install_document(
                Some(Document::open(directory.path().join("note.md")).unwrap()),
                window,
                cx,
            );
        })
    });
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_secs(3));
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(
            notes
                .read(cx)
                .active_document_counts(FooterCountMode::Source),
            Some(DocumentCounts {
                words: 1,
                characters: 8
            })
        );
    });
}

#[gpui::test]
fn readable_counts_are_lazy_and_cached_across_mode_switches(cx: &mut TestAppContext) {
    let (_directory, notes, cx) = fixture(cx);
    replace(&notes, "# Hello **world**", cx);
    cx.executor().advance_clock(Duration::from_millis(100));
    cx.run_until_parked();
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            assert!(
                this.active_document_counts(FooterCountMode::ReadableText)
                    .is_none()
            );
            this.session_footer.count_mode = FooterCountMode::ReadableText;
            this.sync_footer_counts(window, cx);
            assert!(
                this.active_document_counts(FooterCountMode::ReadableText)
                    .is_none()
            );
        })
    });
    cx.run_until_parked();
    let jobs = cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            assert_eq!(
                this.active_document_counts(FooterCountMode::ReadableText),
                Some(DocumentCounts {
                    words: 2,
                    characters: 11
                })
            );
            let jobs = this.documents.values().next().unwrap().counts.jobs_started;
            for mode in [FooterCountMode::Source, FooterCountMode::ReadableText] {
                this.session_footer.count_mode = mode;
                this.sync_footer_counts(window, cx);
            }
            jobs
        })
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(
            notes
                .read(cx)
                .documents
                .values()
                .next()
                .unwrap()
                .counts
                .jobs_started,
            jobs
        );
    });
}
