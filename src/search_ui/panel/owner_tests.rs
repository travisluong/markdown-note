//! Panel ownership checks use a real query entity and deterministic executor time.
use super::{Overlay, SearchPanel};
use gpui::{AppContext, Entity, TestAppContext, VisualTestContext};
use gpui_component::{Root, input::Rope};
use std::{
    fs,
    path::Path,
    sync::{Arc, atomic::Ordering},
    time::Duration,
};

fn fixture<'a>(
    cx: &'a mut TestAppContext,
    root: &Path,
    overlay: Option<Overlay>,
) -> (Entity<SearchPanel>, &'a mut VisualTestContext) {
    cx.update(gpui_component::init);
    let mut panel = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| {
            SearchPanel::new(
                root.to_path_buf(),
                markdown_note::notebook::Exclusions::default(),
                overlay,
                window,
                cx,
            )
        });
        panel = Some(entity.clone());
        Root::new(entity, window, cx)
    });
    (panel.unwrap(), cx)
}

fn query(panel: &Entity<SearchPanel>, value: &str, cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        let query = panel.read(cx).query.clone();
        query.update(cx, |query, cx| {
            query.set_value(value.to_owned(), window, cx)
        });
    });
    cx.run_until_parked();
}

fn finish(cx: &mut VisualTestContext) {
    cx.executor().advance_clock(Duration::from_millis(250));
    cx.run_until_parked();
}

#[gpui::test]
fn completed_scan_cannot_publish_after_its_query_is_superseded(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    fs::write(root.join("a.md"), "first second").unwrap();
    let (panel, cx) = fixture(cx, &root, None);
    cx.update(|_, cx| {
        panel.update(cx, |panel, _| {
            panel.completion_delay = Duration::from_secs(2);
        })
    });
    query(&panel, "first", cx);
    finish(cx);
    let cancelled = cx.update(|_, cx| {
        let panel = panel.read(cx);
        assert_eq!(panel.status, "Searching…");
        assert!(panel.hits.is_empty());
        panel.cancelled.clone()
    });
    query(&panel, "second", cx);
    finish(cx);
    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();
    cx.update(|_, cx| {
        let panel = panel.read(cx);
        assert!(cancelled.load(Ordering::Relaxed));
        assert!(!Arc::ptr_eq(&cancelled, &panel.cancelled));
        assert_eq!(panel.hits.len(), 1);
        assert_eq!(panel.hits[0].matched, "second");
        assert_eq!(panel.status, "1 matches in 1 files");
    });
}

#[gpui::test]
fn clean_install_updates_future_overlay_without_resetting_published_results(
    cx: &mut TestAppContext,
) {
    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    fs::write(root.join("a.md"), "original").unwrap();
    fs::write(root.join("b.md"), "other").unwrap();
    let (panel, cx) = fixture(
        cx,
        &root,
        Some(Overlay {
            path: root.join("a.md"),
            source: Rope::from("old hit"),
        }),
    );
    query(&panel, "hit", cx);
    finish(cx);
    let cancelled = cx.update(|_, cx| panel.read(cx).cancelled.clone());
    cx.update(|_, cx| {
        panel.update(cx, |panel, cx| {
            panel.document_installed(
                Some(Overlay {
                    path: root.join("b.md"),
                    source: Rope::from("fresh hit"),
                }),
                None,
                cx,
            );
            assert_eq!(panel.hits[0].relative, Path::new("a.md"));
            assert!(Arc::ptr_eq(&cancelled, &panel.cancelled));
        })
    });
    query(&panel, "fresh", cx);
    finish(cx);
    cx.update(|_, cx| {
        let panel = panel.read(cx);
        assert_eq!(panel.hits.len(), 1);
        assert_eq!(panel.hits[0].relative, Path::new("b.md"));
        assert_eq!(panel.hits[0].matched, "fresh");
    });
}

#[gpui::test]
fn navigation_waits_for_approval_and_rejects_superseded_requests(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    fs::write(root.join("a.md"), "hit hit hit").unwrap();
    let (panel, cx) = fixture(cx, &root, None);
    query(&panel, "hit", cx);
    finish(cx);
    let stale = cx.update(|window, cx| {
        panel.update(cx, |panel, cx| {
            panel.request_adjacent(true, cx);
            let destination = panel.pending_opens.front().unwrap().destination.clone();
            assert_eq!(
                panel.selected, 0,
                "the application has not approved navigation"
            );
            panel.reject_open(&destination);
            assert_eq!(panel.selected, 0);
            panel.request_adjacent(true, cx);
            assert!(panel.accept_open(&destination, cx));
            assert_eq!(panel.selected, 1);
            panel.navigation_finished(false, window, cx);
            assert_eq!(
                panel.selected, 0,
                "cancelled arrow navigation restores selection"
            );
            panel.request_adjacent(true, cx);
            destination
        })
    });
    query(&panel, "missing", cx);
    cx.update(|_, cx| {
        panel.update(cx, |panel, cx| {
            assert!(!panel.accept_open(&stale, cx));
            assert_eq!(panel.selected, 0);
            assert!(panel.navigation.is_none());
        })
    });
}

#[gpui::test]
fn closing_the_panel_owner_cancels_its_pending_scan(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    fs::write(root.join("a.md"), "hit").unwrap();
    let (panel, cx) = fixture(cx, &root, None);
    query(&panel, "hit", cx);
    let cancelled = cx.update(|_, cx| panel.read(cx).cancelled.clone());
    drop(panel);
    cx.update(|window, _| window.remove_window());
    cx.run_until_parked();
    assert!(cancelled.load(Ordering::Relaxed));
    finish(cx);
}
