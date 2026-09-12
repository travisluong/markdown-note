//! Existing opt-in search diagnostic script; never run as native UI automation.
use crate::{Document, Next, Notes, ViewMode, search};
use gpui::{AsyncWindowContext, Entity, Focusable, Keystroke, Timer};
use gpui_component::WindowExt;
use std::{
    fs,
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

pub(crate) async fn diagnostic_search(notes: &Entity<Notes>, cx: &mut AsyncWindowContext) {
    let mut navigation_snapshot: Option<(Arc<AtomicBool>, String)> = None;
    for step in 0..=18 {
        Timer::after(Duration::from_millis(400)).await;
        // Wait for file loads and debounced scans, with a bounded deadline.
        for attempt in 0..100 {
            let ready = notes
                .update(cx, |this, cx| {
                    !this.is_busy()
                        && this.session.write().is_none()
                        && this
                            .search
                            .as_ref()
                            .is_none_or(|p| p.read(cx).status != "Searching…")
                })
                .unwrap();
            if ready {
                break;
            }
            assert!(attempt < 99, "search smoke timed out at {step}");
            Timer::after(Duration::from_millis(100)).await;
        }
        cx.update(|window, cx| {
            if step == 1 || step == 6 {
                window.dispatch_keystroke(Keystroke::parse("ctrl-shift-f").unwrap(), cx);
                return;
            }
            if step == 4 || step == 7 {
                if step == 7 {
                    window.dispatch_keystroke(Keystroke::parse("down").unwrap(), cx);
                }
                navigation_snapshot = notes.update(cx, |this, cx| {
                    this.search.as_ref().map(|panel| {
                        let panel = panel.read(cx);
                        (panel.cancelled.clone(), panel.status.clone())
                    })
                });
                window.dispatch_keystroke(Keystroke::parse("enter").unwrap(), cx);
                return;
            }
            notes.update(cx, |this, cx| {
                let root = this.notebooks.active.as_ref().unwrap().root.clone();
                let a = root.join("search-smoke-a.md");
                let b = root.join("search-smoke-b.md");
                match step {
                    0 => {
                        fs::write(&a, "café needle\nneedle").unwrap();
                        fs::write(&b, "prefix needle").unwrap();
                        this.install_document(Some(Document::open(a).unwrap()), window, cx);
                        this.set_mode(ViewMode::Reading, window, cx);
                        this.toggle_sidebar(window, cx);
                        assert!(this.sidebar_collapsed);
                    }
                    2 => {
                        let query = this
                            .search
                            .as_ref()
                            .expect("shortcut opens search")
                            .read(cx)
                            .query
                            .clone();
                        assert!(query.focus_handle(cx).is_focused(window));
                        assert!(!this.sidebar_collapsed);
                        assert!(this.search.as_ref().unwrap().read(cx).status.is_empty());
                        query.update(cx, |q, cx| q.set_value("superseded", window, cx));
                        query.update(cx, |q, cx| q.set_value("needle", window, cx));
                    }
                    3 => {
                        assert_eq!(this.search.as_ref().unwrap().read(cx).hits.len(), 3);
                        let query = this.search.as_ref().unwrap().read(cx).query.clone();
                        let width = this
                            .notebooks
                            .active
                            .as_ref()
                            .unwrap()
                            .settings
                            .sidebar_width;
                        this.search
                            .as_ref()
                            .unwrap()
                            .update(cx, |panel, _| panel.selected = 1);
                        this.toggle_sidebar(window, cx);
                        assert!(this.sidebar_collapsed);
                        assert!(this.focus.is_focused(window));
                        this.toggle_sidebar(window, cx);
                        assert!(!this.sidebar_collapsed);
                        assert!(query.focus_handle(cx).is_focused(window));
                        this.show_explorer(window, cx);
                        assert!(!this.search.as_ref().unwrap().read(cx).visible);
                        assert!(this.notebooks.tree_focus.is_focused(window));
                        this.toggle_sidebar(window, cx);
                        this.toggle_sidebar(window, cx);
                        assert!(!this.search.as_ref().unwrap().read(cx).visible);
                        assert!(this.notebooks.tree_focus.is_focused(window));
                        this.open_search_all(window, cx);
                        let panel = this.search.as_ref().unwrap().read(cx);
                        assert!(panel.visible);
                        assert_eq!(panel.query, query);
                        assert_eq!(query.read(cx).value().as_str(), "needle");
                        assert_eq!(panel.hits.len(), 3);
                        assert_eq!(panel.selected, 1);
                        assert_eq!(
                            this.notebooks
                                .active
                                .as_ref()
                                .unwrap()
                                .settings
                                .sidebar_width,
                            width
                        );
                        this.search
                            .as_ref()
                            .unwrap()
                            .update(cx, |panel, _| panel.selected = 0);
                    }
                    5 | 8 => {
                        let panel = this.search.as_ref().unwrap().read(cx);
                        assert_eq!(panel.selected, if step == 5 { 0 } else { 1 });
                        let (scan, status) = navigation_snapshot.as_ref().unwrap();
                        assert!(
                            Arc::ptr_eq(scan, &panel.cancelled),
                            "navigation must not schedule a scan"
                        );
                        assert_eq!(&panel.status, status);
                        assert_eq!(this.mode, ViewMode::Reading);
                        assert!(this.reading_search.is_some());
                        assert_eq!(
                            panel.hits.len(),
                            3,
                            "opening a result preserves the result list"
                        );
                        if step == 5 {
                            this.toggle_sidebar(window, cx);
                            assert!(this.sidebar_collapsed);
                        }
                    }
                    9 => {
                        this.set_mode(ViewMode::Source, window, cx);
                        this.editor
                            .update(cx, |e, cx| e.set_value("unsaved needle", window, cx));
                        this.search
                            .as_ref()
                            .unwrap()
                            .read(cx)
                            .query
                            .clone()
                            .update(cx, |q, cx| q.set_value("unsaved", window, cx));
                    }
                    10 => {
                        assert_eq!(this.search.as_ref().unwrap().read(cx).hits.len(), 1);
                        this.search
                            .as_ref()
                            .unwrap()
                            .read(cx)
                            .query
                            .clone()
                            .update(cx, |q, cx| q.set_value("needle", window, cx));
                    }
                    11 => {
                        let panel = this.search.as_ref().unwrap().read(cx);
                        assert_eq!(panel.hits.len(), 2);
                        let destination = search::Destination {
                            path: b.clone(),
                            hit: panel.hits[1].clone(),
                            query: "needle".into(),
                            case_sensitive: false,
                        };
                        this.diagnostic_answer_request(
                            None,
                            Next::SearchResult(destination.clone()),
                            window,
                            cx,
                        );
                        assert_eq!(this.document().unwrap().path, a);
                        assert_eq!(this.editor.read(cx).value().as_str(), "unsaved needle");
                        this.diagnostic_answer_request(
                            Some(0),
                            Next::SearchResult(destination),
                            window,
                            cx,
                        );
                    }
                    12 => {
                        assert_eq!(this.document().unwrap().path, b);
                        assert_eq!(this.editor.read(cx).cursor(), 13);
                        assert_eq!(fs::read_to_string(a).unwrap(), "unsaved needle");
                        this.editor
                            .update(cx, |e, cx| e.set_value("discard me", window, cx));
                    }
                    13 => {
                        let hit = this.search.as_ref().unwrap().read(cx).hits[0].clone();
                        this.diagnostic_answer_request(
                            Some(1),
                            Next::SearchResult(search::Destination {
                                path: a,
                                hit,
                                query: "needle".into(),
                                case_sensitive: false,
                            }),
                            window,
                            cx,
                        );
                    }
                    14 => {
                        assert_eq!(this.document().unwrap().path, a);
                        assert_eq!(this.editor.read(cx).cursor(), 14);
                        assert_eq!(fs::read_to_string(b).unwrap(), "prefix needle");
                        this.set_mode(ViewMode::LivePreview, window, cx);
                        this.editor.update(cx, |e, cx| {
                            assert!(e.select_source_range(8..14, window, cx));
                            assert!(!e.select_source_range(0..999, window, cx));
                            assert_eq!(e.value().as_str(), "unsaved needle");
                        });
                    }
                    15 => {
                        fs::remove_file(b).unwrap();
                        this.schedule_search(window, cx);
                    }
                    16 => assert_eq!(this.search.as_ref().unwrap().read(cx).hits.len(), 1),
                    17 => {
                        assert_eq!(this.document().unwrap().path, a);
                        assert!(this.search_destination.is_none());
                        window.close_dialog(cx);
                        this.request(
                            Next::Notebook {
                                path: root,
                                create: false,
                            },
                            window,
                            cx,
                        );
                    }
                    18 => assert!(this.search.is_none(), "notebook switch clears search"),
                    _ => unreachable!(),
                }
            });
        })
        .unwrap();
    }
}
