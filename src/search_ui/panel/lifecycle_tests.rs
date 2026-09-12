//! Search lifecycle checks use GPUI's in-memory TestPlatform and disposable files.
use crate::{Document, Next, Notes, notebook};
use gpui::{Entity, ScrollStrategy, TestAppContext, VisualTestContext, prelude::*};
use gpui_component::{Root, scroll::ScrollbarHandle};
use std::{
    fs,
    path::Path,
    sync::{Arc, atomic::Ordering},
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
            book.settings.autosave.enabled = false;
            notes.notebooks.active = Some(book);
            notes.saves.session.enabled = false;
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

fn search(notes: &Entity<Notes>, cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.open_search_all(window, cx);
            this.search
                .as_ref()
                .unwrap()
                .read(cx)
                .query
                .clone()
                .update(cx, |query, cx| query.set_value("hit", window, cx));
        })
    });
    cx.run_until_parked();
}

fn finish_search(cx: &mut VisualTestContext) {
    cx.executor().advance_clock(Duration::from_millis(250));
    cx.run_until_parked();
}

fn navigate(notes: &Entity<Notes>, path: &Path, cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.request(Next::File(path.to_path_buf()), window, cx);
        })
    });
}

#[gpui::test]
fn discarded_published_overlay_is_replaced_with_disk_results(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("a.md"), "original").unwrap();
    fs::write(dir.path().join("b.md"), "other").unwrap();
    let (notes, cx) = fixture(cx, dir.path());
    edit(&notes, "discarded hit", cx);
    search(&notes, cx);
    finish_search(cx);
    let (cancelled, query) = cx.update(|_, cx| {
        let panel = notes.read(cx).search.as_ref().unwrap().read(cx);
        assert_eq!(panel.hits.len(), 1);
        assert_eq!(panel.hits[0].relative, Path::new("a.md"));
        (panel.cancelled.clone(), panel.query.clone())
    });

    navigate(&notes, &dir.path().join("b.md"), cx);
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer("Discard");
    cx.run_until_parked();
    assert!(cancelled.load(Ordering::Relaxed));
    finish_search(cx);

    cx.update(|_, cx| {
        let this = notes.read(cx);
        let panel = this.search.as_ref().unwrap().read(cx);
        assert_eq!(panel.query, query);
        assert_eq!(panel.query.read(cx).value().as_ref(), "hit");
        assert!(panel.hits.is_empty());
        assert_eq!(panel.status, "0 matches in 0 files");
        assert_eq!(this.document().unwrap().path.file_name().unwrap(), "b.md");
    });
    assert_eq!(
        fs::read_to_string(dir.path().join("a.md")).unwrap(),
        "original"
    );
}

#[gpui::test]
fn discarded_pending_overlay_cannot_publish_after_navigation(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("a.md"), "original").unwrap();
    fs::write(dir.path().join("b.md"), "other").unwrap();
    let (notes, cx) = fixture(cx, dir.path());
    edit(&notes, "discarded hit", cx);
    search(&notes, cx);
    let cancelled = cx.update(|_, cx| {
        let panel = notes.read(cx).search.as_ref().unwrap().read(cx);
        assert_eq!(panel.status, "Searching…");
        assert!(panel.hits.is_empty());
        panel.cancelled.clone()
    });

    navigate(&notes, &dir.path().join("b.md"), cx);
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer("Discard");
    cx.run_until_parked();
    assert!(cancelled.load(Ordering::Relaxed));
    finish_search(cx);

    cx.update(|_, cx| {
        let panel = notes.read(cx).search.as_ref().unwrap().read(cx);
        assert!(!Arc::ptr_eq(&panel.cancelled, &cancelled));
        assert!(panel.hits.is_empty());
        assert_eq!(panel.status, "0 matches in 0 files");
    });
    assert_eq!(
        fs::read_to_string(dir.path().join("a.md")).unwrap(),
        "original"
    );
}

#[gpui::test]
fn clean_navigation_preserves_search_results_selection_and_scroll(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("a.md"), "hit\n".repeat(80)).unwrap();
    fs::write(dir.path().join("b.md"), "other").unwrap();
    let (notes, cx) = fixture(cx, dir.path());
    search(&notes, cx);
    finish_search(cx);
    cx.update(|_, cx| {
        notes.update(cx, |this, cx| {
            this.search.as_ref().unwrap().update(cx, |panel, cx| {
                panel.selected = 20;
                panel.scroll.scroll_to_item(20, ScrollStrategy::Top);
                cx.notify();
            });
        })
    });
    cx.run_until_parked();
    let (cancelled, query, status, hits, scroll) = cx.update(|_, cx| {
        let panel = notes.read(cx).search.as_ref().unwrap().read(cx);
        (
            panel.cancelled.clone(),
            panel.query.clone(),
            panel.status.clone(),
            panel
                .hits
                .iter()
                .map(|hit| (hit.relative.clone(), hit.range.clone()))
                .collect::<Vec<_>>(),
            panel.scroll.offset(),
        )
    });

    navigate(&notes, &dir.path().join("b.md"), cx);
    assert!(!cx.has_pending_prompt());
    cx.run_until_parked();
    finish_search(cx);

    cx.update(|_, cx| {
        let this = notes.read(cx);
        let panel = this.search.as_ref().unwrap().read(cx);
        assert_eq!(this.document().unwrap().path.file_name().unwrap(), "b.md");
        assert!(Arc::ptr_eq(&panel.cancelled, &cancelled));
        assert!(!cancelled.load(Ordering::Relaxed));
        assert_eq!(panel.query, query);
        assert_eq!(panel.status, status);
        assert_eq!(panel.selected, 20);
        assert_eq!(panel.scroll.offset(), scroll);
        assert_eq!(
            panel
                .hits
                .iter()
                .map(|hit| (hit.relative.clone(), hit.range.clone()))
                .collect::<Vec<_>>(),
            hits
        );
    });
}

#[gpui::test]
fn panel_open_intents_use_application_navigation_and_restore_cancelled_arrows(
    cx: &mut TestAppContext,
) {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("a.md"), "hit").unwrap();
    fs::write(dir.path().join("b.md"), "hit").unwrap();
    let (notes, cx) = fixture(cx, dir.path());
    search(&notes, cx);
    finish_search(cx);
    cx.update(|_, cx| {
        let panel = notes.read(cx).search.as_ref().unwrap().clone();
        panel.update(cx, |panel, cx| panel.request_adjacent(true, cx));
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        let notes = notes.read(cx);
        assert_eq!(notes.document().unwrap().path.file_name().unwrap(), "b.md");
        let panel = notes.search.as_ref().unwrap().read(cx);
        assert_eq!(panel.selected, 1);
        assert!(panel.navigation.is_none());
    });
    edit(&notes, "hit changed", cx);
    finish_search(cx);
    cx.update(|_, cx| {
        let panel = notes.read(cx).search.as_ref().unwrap().clone();
        panel.update(cx, |panel, cx| panel.request_adjacent(false, cx));
    });
    cx.run_until_parked();
    assert!(cx.has_pending_prompt());
    // The pending save decision prevents another panel action from moving the
    // selection, even though the panel has no dependency on the session owner.
    cx.update(|_, cx| {
        let panel = notes.read(cx).search.as_ref().unwrap().clone();
        panel.update(cx, |panel, cx| panel.request_adjacent(true, cx));
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        let notes = notes.read(cx);
        let panel = notes.search.as_ref().unwrap().read(cx);
        assert_eq!(panel.selected, 0);
        assert_eq!(panel.navigation, Some(1));
        assert!(panel.pending_opens.is_empty());
    });
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    cx.update(|_, cx| {
        let notes = notes.read(cx);
        assert_eq!(notes.document().unwrap().path.file_name().unwrap(), "b.md");
        let panel = notes.search.as_ref().unwrap().read(cx);
        assert_eq!(panel.selected, 1);
        assert!(panel.navigation.is_none());
    });
}

#[gpui::test]
fn queued_arrows_advance_each_same_file_result(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("a.md"), "hit hit hit").unwrap();
    let (notes, cx) = fixture(cx, dir.path());
    search(&notes, cx);
    finish_search(cx);
    cx.update(|_, cx| {
        let panel = notes.read(cx).search.as_ref().unwrap().clone();
        panel.update(cx, |panel, cx| {
            panel.request_adjacent(true, cx);
            panel.request_adjacent(true, cx);
        });
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        let notes = notes.read(cx);
        let panel = notes.search.as_ref().unwrap().read(cx);
        assert_eq!(panel.selected, 2);
        assert!(panel.navigation.is_none());
        assert!(panel.pending_opens.is_empty());
        assert_eq!(notes.document().unwrap().path.file_name().unwrap(), "a.md");
    });
}

#[gpui::test]
fn queued_arrows_do_not_skip_the_first_pending_file_navigation(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    for name in ["a.md", "b.md", "c.md"] {
        fs::write(dir.path().join(name), "hit").unwrap();
    }
    let (notes, cx) = fixture(cx, dir.path());
    search(&notes, cx);
    finish_search(cx);
    cx.update(|_, cx| {
        let panel = notes.read(cx).search.as_ref().unwrap().clone();
        panel.update(cx, |panel, cx| {
            panel.request_adjacent(true, cx);
            panel.request_adjacent(true, cx);
        });
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        let notes = notes.read(cx);
        let panel = notes.search.as_ref().unwrap().read(cx);
        assert_eq!(panel.selected, 1);
        assert!(panel.navigation.is_none());
        assert!(panel.pending_opens.is_empty());
        assert_eq!(notes.document().unwrap().path.file_name().unwrap(), "b.md");
    });
}
