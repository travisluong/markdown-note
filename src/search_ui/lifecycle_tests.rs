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
                .query
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
        let panel = notes.read(cx).search.as_ref().unwrap();
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
        let panel = this.search.as_ref().unwrap();
        assert_eq!(panel.query, query);
        assert_eq!(panel.query.read(cx).value().as_ref(), "hit");
        assert!(panel.hits.is_empty());
        assert_eq!(panel.status, "0 matches in 0 files");
        assert_eq!(
            this.document.as_ref().unwrap().path.file_name().unwrap(),
            "b.md"
        );
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
        let panel = notes.read(cx).search.as_ref().unwrap();
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
        let panel = notes.read(cx).search.as_ref().unwrap();
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
            let panel = this.search.as_mut().unwrap();
            panel.selected = 20;
            panel.scroll.scroll_to_item(20, ScrollStrategy::Top);
            cx.notify();
        })
    });
    cx.run_until_parked();
    let (cancelled, query, status, hits, scroll) = cx.update(|_, cx| {
        let panel = notes.read(cx).search.as_ref().unwrap();
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
        let panel = this.search.as_ref().unwrap();
        assert_eq!(
            this.document.as_ref().unwrap().path.file_name().unwrap(),
            "b.md"
        );
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
