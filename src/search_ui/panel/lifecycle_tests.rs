//! Search lifecycle checks use GPUI's in-memory TestPlatform and disposable files.
use crate::{Document, Next, Notes, notebook};
use gpui::{Entity, Focusable, ScrollStrategy, TestAppContext, VisualTestContext, prelude::*};
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
            notes.session_autosave.enabled = false;
            notes
        });
        entity.update(cx, |this, cx| {
            this.install_document(Some(Document::open(root.join("a.md")).unwrap()), window, cx);
        });
        notes = Some(entity.clone());
        let view = cx.new(|cx| crate::NotesWindow {
            _subscription: cx.observe(&entity, |_, _, cx| cx.notify()),
            notes: entity,
        });
        Root::new(view, window, cx)
    });
    (notes.unwrap(), cx)
}

fn edit(notes: &Entity<Notes>, text: &str, cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.editor.diagnostic_input().update(cx, |editor, cx| {
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
        assert_eq!(
            panel
                .navigation
                .as_ref()
                .map(|navigation| navigation.previous),
            Some(1)
        );
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

fn click_then_arrows(cx: &mut TestAppContext, mode: crate::ViewMode) {
    use gpui::Focusable;
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("a.md"), "hit\n\nhit\n\nhit").unwrap();
    fs::write(dir.path().join("b.md"), "hit\n\nhit").unwrap();
    let (notes, cx) = fixture(cx, dir.path());
    cx.update(|window, cx| notes.update(cx, |this, cx| this.set_mode(mode, window, cx)));
    search(&notes, cx);
    finish_search(cx);
    // TestPlatform dispatches a real row click without any OS input. Start
    // outside search to require the click to establish query focus.
    cx.update(|window, cx| notes.read(cx).editor.clone().focus(window, cx));
    cx.refresh().unwrap();
    cx.run_until_parked();
    let row = cx
        .debug_bounds("notebook-search-result-1")
        .expect("second search row");
    cx.simulate_click(row.center(), gpui::Modifiers::default());
    cx.run_until_parked();
    for (action, expected) in [
        (None, 1),
        (Some(true), 2),
        (Some(false), 1),
        (Some(true), 2),
        (Some(true), 3),
    ] {
        if let Some(down) = action {
            if down {
                cx.dispatch_action(gpui_component::input::MoveDown);
            } else {
                cx.dispatch_action(gpui_component::input::MoveUp);
            }
            cx.run_until_parked();
        }
        cx.update(|window, cx| {
            let this = notes.read(cx);
            let panel = this.search.as_ref().unwrap().read(cx);
            assert_eq!(panel.selected, expected);
            assert!(panel.query.focus_handle(cx).is_focused(window));
            assert!(!this.editor.is_focused(window, cx));
            let hit = &panel.hits[expected];
            assert_eq!(
                this.document().unwrap().path,
                fs::canonicalize(dir.path().join(&hit.relative)).unwrap()
            );
            if mode == crate::ViewMode::Reading {
                let find = this.reading_search.as_ref().unwrap();
                assert!(find.matches.contains(&hit.range));
                assert!(!find.query.focus_handle(cx).is_focused(window));
            } else {
                assert_eq!(this.editor.selection(cx), hit.range);
            }
            assert_eq!(
                this.editor.text(cx).as_ref(),
                fs::read_to_string(&this.document().unwrap().path).unwrap()
            );
        });
    }
    cx.dispatch_action(gpui_component::input::Enter { secondary: false });
    cx.run_until_parked();
    cx.update(|window, cx| {
        let panel = notes.read(cx).search.as_ref().unwrap().read(cx);
        assert_eq!(panel.selected, 3);
        assert!(panel.query.focus_handle(cx).is_focused(window));
    });
}

#[gpui::test]
fn source_click_then_arrows_keep_search_focus(cx: &mut TestAppContext) {
    click_then_arrows(cx, crate::ViewMode::Source);
}

#[gpui::test]
fn live_click_then_arrows_keep_search_focus(cx: &mut TestAppContext) {
    click_then_arrows(cx, crate::ViewMode::LivePreview);
}

#[gpui::test]
fn reading_click_then_arrows_keep_search_focus(cx: &mut TestAppContext) {
    click_then_arrows(cx, crate::ViewMode::Reading);
}

#[gpui::test]
fn cancelled_and_failed_clicks_restore_selection_and_search_focus(cx: &mut TestAppContext) {
    use gpui::Focusable;
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("a.md"), "hit").unwrap();
    fs::write(dir.path().join("b.md"), "hit").unwrap();
    let (notes, cx) = fixture(cx, dir.path());
    edit(&notes, "hit changed", cx);
    search(&notes, cx);
    finish_search(cx);
    let panel = cx.update(|_, cx| notes.read(cx).search.as_ref().unwrap().clone());
    cx.update(|_, cx| panel.update(cx, |panel, cx| panel.request_open(1, false, cx)));
    cx.run_until_parked();
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    cx.update(|window, cx| {
        assert_eq!(panel.read(cx).selected, 0);
        assert!(panel.read(cx).query.focus_handle(cx).is_focused(window));
    });
    // A scanned file disappearing must follow the same failure restoration.
    fs::remove_file(dir.path().join("b.md")).unwrap();
    cx.update(|_, cx| panel.update(cx, |panel, cx| panel.request_open(1, false, cx)));
    cx.run_until_parked();
    cx.simulate_prompt_answer("Discard");
    cx.run_until_parked();
    cx.update(|window, cx| {
        assert_eq!(panel.read(cx).selected, 0);
        assert!(panel.read(cx).query.focus_handle(cx).is_focused(window));
        assert_eq!(
            notes.read(cx).document().unwrap().path,
            fs::canonicalize(dir.path().join("a.md")).unwrap()
        );
    });
}

#[gpui::test]
fn successful_click_preserves_result_list_scroll(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("a.md"), "hit\n".repeat(80)).unwrap();
    let (notes, cx) = fixture(cx, dir.path());
    search(&notes, cx);
    finish_search(cx);
    let panel = cx.update(|_, cx| notes.read(cx).search.as_ref().unwrap().clone());
    cx.update(|_, cx| {
        panel.update(cx, |panel, cx| {
            panel.scroll.scroll_to_item(20, ScrollStrategy::Top);
            cx.notify();
        })
    });
    cx.run_until_parked();
    cx.refresh().unwrap();
    let before = cx.update(|_, cx| panel.read(cx).scroll.offset());
    let row = cx
        .debug_bounds("notebook-search-result-22")
        .expect("visible row");
    cx.simulate_click(row.center(), gpui::Modifiers::default());
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(panel.read(cx).selected, 22);
        assert_eq!(panel.read(cx).scroll.offset(), before);
    });
}

#[gpui::test]
fn exclusions_cancel_old_searches_and_keep_the_open_document_editable(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("a.md"), "original").unwrap();
    fs::write(dir.path().join("b.md"), "disk hit").unwrap();
    let (notes, cx) = fixture(cx, dir.path());
    edit(&notes, "unsaved hit", cx);
    search(&notes, cx);
    finish_search(cx);
    cx.update(|_, cx| {
        assert_eq!(
            notes.read(cx).search.as_ref().unwrap().read(cx).hits.len(),
            2
        )
    });
    // Retain an old worker after it has scanned the now-excluded overlay.
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.search.as_ref().unwrap().update(cx, |panel, _| {
                panel.completion_delay = Duration::from_secs(2)
            });
            this.schedule_search(window, cx);
        })
    });
    finish_search(cx);
    let old_cancelled = cx.update(|_, cx| {
        notes
            .read(cx)
            .search
            .as_ref()
            .unwrap()
            .read(cx)
            .cancelled
            .clone()
    });
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.update_excluded_patterns(vec![r"^a\.md$".into()], window, cx);
            assert!(this.dirty(cx));
            assert_eq!(this.editor.text(cx).as_ref(), "unsaved hit");
        })
    });
    finish_search(cx);
    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert!(old_cancelled.load(Ordering::Relaxed));
        let panel = notes.read(cx).search.as_ref().unwrap().read(cx);
        assert_eq!(panel.hits.len(), 1);
        assert_eq!(panel.hits[0].relative, Path::new("b.md"));
    });
    edit(&notes, "edited excluded hit", cx);
    cx.update(|window, cx| notes.update(cx, |this, cx| this.save(Next::Nothing, window, cx)));
    cx.run_until_parked();
    assert_eq!(
        fs::read_to_string(dir.path().join("a.md")).unwrap(),
        "edited excluded hit"
    );
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            assert!(!this.dirty(cx));
            this.update_excluded_patterns(Vec::new(), window, cx);
        })
    });
    finish_search(cx);
    cx.update(|_, cx| {
        assert_eq!(
            notes.read(cx).search.as_ref().unwrap().read(cx).hits.len(),
            2
        )
    });
}

#[gpui::test]
fn retained_unsaved_tabs_feed_real_search_and_open_existing_result(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    for name in ["a.md", "b.md"] {
        fs::write(dir.path().join(name), "original").unwrap();
    }
    fs::write(outside.path().join("outside.md"), "original").unwrap();
    let (notes, cx) = fixture(cx, dir.path());
    edit(&notes, "first hit", cx);
    let first = cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            let id = this.tabs.active.id;
            this.new_tab(window, cx);
            this.install_document(
                Some(Document::open(dir.path().join("b.md")).unwrap()),
                window,
                cx,
            );
            id
        })
    });
    edit(&notes, "second hit", cx);
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.new_tab(window, cx);
            this.install_document(
                Some(Document::open(outside.path().join("outside.md")).unwrap()),
                window,
                cx,
            );
        })
    });
    edit(&notes, "outside hit", cx);
    let destination_tab = cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.new_tab(window, cx);
            this.tabs.active.id
        })
    });
    search(&notes, cx);
    finish_search(cx);
    let panel = cx.update(|_, cx| notes.read(cx).search.as_ref().unwrap().clone());
    cx.update(|_, cx| {
        panel.update(cx, |panel, cx| {
            assert_eq!(panel.hits.len(), 2);
            assert!(
                panel
                    .hits
                    .iter()
                    .any(|hit| hit.relative == Path::new("b.md"))
            );
            let index = panel
                .hits
                .iter()
                .position(|hit| hit.relative == Path::new("a.md"))
                .unwrap();
            panel.request_open(index, false, cx);
        })
    });
    cx.run_until_parked();
    assert!(!cx.has_pending_prompt());
    cx.update(|window, cx| {
        let n = notes.read(cx);
        assert_eq!(n.tabs.active.id, destination_tab);
        assert_eq!(
            n.tabs.active.document().unwrap().id,
            n.tabs.get(first).unwrap().document().unwrap().id
        );
        assert_eq!(n.tabs.ids().len(), 4);
        assert_eq!(n.tabs.active.editor.selection(cx), 6..9);
        assert!(
            panel
                .read(cx)
                .query
                .read(cx)
                .focus_handle(cx)
                .is_focused(window)
        );
        assert_eq!(panel.read(cx).hits.len(), 2);
    });
}

#[gpui::test]
fn settings_pause_preserves_search_and_blocks_result_navigation(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("a.md"), "hit first").unwrap();
    fs::write(dir.path().join("b.md"), "hit second").unwrap();
    let (notes, cx) = fixture(cx, dir.path());
    search(&notes, cx);
    finish_search(cx);
    let panel = cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.open_settings(window, cx);
            this.search.as_ref().unwrap().clone()
        })
    });
    cx.refresh().unwrap();
    cx.update(|_, cx| {
        panel.update(cx, |panel, cx| {
            assert!(!panel.interaction_enabled);
            assert_eq!(panel.hits.len(), 2);
            assert_eq!(panel.query.read(cx).value().as_ref(), "hit");
            panel.request_open(1, false, cx);
            assert!(panel.pending_opens.is_empty());
        })
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(
            notes.read(cx).document().unwrap().path.file_name().unwrap(),
            "a.md"
        )
    });
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    cx.refresh().unwrap();
    cx.update(|_, cx| {
        panel.update(cx, |panel, cx| {
            assert!(panel.interaction_enabled);
            assert_eq!(panel.hits.len(), 2);
            assert_eq!(panel.query.read(cx).value().as_ref(), "hit");
            panel.request_open(1, false, cx);
        })
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(
            notes.read(cx).document().unwrap().path.file_name().unwrap(),
            "b.md"
        )
    });
}

#[gpui::test]
fn title_sidebar_button_retains_search_results_and_query_focus(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("a.md"), "hit hit").unwrap();
    let (notes, cx) = fixture(cx, dir.path());
    cx.update(|_, cx| {
        notes.update(cx, |this, cx| {
            this.notebooks
                .active
                .as_mut()
                .unwrap()
                .settings
                .toolbar
                .title_bar_sidebar = true;
            cx.notify();
        })
    });
    search(&notes, cx);
    finish_search(cx);
    let (query, cancelled) = cx.update(|_, cx| {
        let panel = notes.read(cx).search.as_ref().unwrap().read(cx);
        assert_eq!(panel.hits.len(), 2);
        (panel.query.clone(), panel.cancelled.clone())
    });
    for collapsed in [true, false] {
        cx.refresh().unwrap();
        let button = cx.debug_bounds("title-bar-sidebar-toggle").unwrap();
        cx.simulate_click(button.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        cx.update(|window, cx| {
            let this = notes.read(cx);
            assert_eq!(this.sidebar_collapsed, collapsed);
            let panel = this.search.as_ref().unwrap().read(cx);
            assert!(panel.visible);
            assert_eq!(panel.query, query);
            assert_eq!(query.read(cx).value().as_ref(), "hit");
            assert_eq!(panel.hits.len(), 2);
            assert!(Arc::ptr_eq(&panel.cancelled, &cancelled));
            if !collapsed {
                assert!(query.focus_handle(cx).is_focused(window));
            }
        });
    }
}

#[gpui::test]
fn sidebar_shortcuts_retain_search_query_results_and_restore_panel_focus(cx: &mut TestAppContext) {
    cx.update(|cx| {
        cx.bind_keys(
            crate::help_ui::app_shortcuts()
                .into_iter()
                .map(|s| s.binding),
        )
    });
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("a.md"), "hit hit").unwrap();
    let (notes, cx) = fixture(cx, dir.path());
    search(&notes, cx);
    finish_search(cx);
    let (query, cancelled, hit_ranges) = cx.update(|_, cx| {
        let panel = notes.read(cx).search.as_ref().unwrap().read(cx);
        assert_eq!(panel.hits.len(), 2);
        (
            panel.query.clone(),
            panel.cancelled.clone(),
            panel
                .hits
                .iter()
                .map(|h| h.range.clone())
                .collect::<Vec<_>>(),
        )
    });
    let modifier = if cfg!(target_os = "macos") {
        "cmd"
    } else {
        "ctrl"
    };
    let toggle = format!("{modifier}-shift-b");
    let explorer = format!("{modifier}-shift-e");
    for mode in [crate::ViewMode::Source, crate::ViewMode::Reading] {
        cx.update(|window, cx| {
            notes.update(cx, |this, cx| {
                this.set_mode(mode, window, cx);
                this.open_search_all(window, cx);
            })
        });
        cx.refresh().unwrap();
        cx.simulate_keystrokes(&toggle);
        cx.run_until_parked();
        cx.update(|window, cx| {
            let this = notes.read(cx);
            assert!(this.sidebar_collapsed);
            if mode == crate::ViewMode::Reading {
                assert!(this.focus.is_focused(window));
            } else {
                assert!(this.editor.is_focused(window, cx));
            }
        });
        cx.refresh().unwrap();
        cx.simulate_keystrokes(&toggle);
        cx.run_until_parked();
        cx.update(|window, cx| {
            assert!(!notes.read(cx).sidebar_collapsed);
            assert!(query.focus_handle(cx).is_focused(window));
        });
        cx.refresh().unwrap();
        cx.simulate_keystrokes(&explorer);
        cx.run_until_parked();
        cx.update(|window, cx| {
            notes.update(cx, |this, cx| {
                assert!(!this.search_visible(cx));
                assert!(this.notebooks.tree_focus.is_focused(window));
                this.open_search_all(window, cx);
                let panel = this.search.as_ref().unwrap().read(cx);
                assert!(panel.visible);
                assert_eq!(panel.query, query);
                assert_eq!(query.read(cx).value().as_ref(), "hit");
                assert_eq!(
                    panel
                        .hits
                        .iter()
                        .map(|h| h.range.clone())
                        .collect::<Vec<_>>(),
                    hit_ranges
                );
                assert!(
                    Arc::ptr_eq(&panel.cancelled, &cancelled),
                    "panel switch does not schedule a scan"
                );
                assert!(query.focus_handle(cx).is_focused(window));
            })
        });
    }
    // Settings, other dialogs, a missing notebook and a busy session all keep
    // the same collapsed panel even when actions are dispatched directly.
    for guard in 0..4 {
        cx.update(|window, cx| {
            notes.update(cx, |this, cx| {
                this.sidebar_collapsed = true;
                this.focus.focus(window);
                match guard {
                    0 => this.open_settings(window, cx),
                    1 => this.open_help(false, window, cx),
                    2 => this.notebooks.active = None,
                    _ => {
                        this.notebooks.active =
                            Some(notebook::Notebook::open(dir.path(), false).unwrap());
                        let _ = this.session.pick();
                    }
                }
                cx.notify();
            })
        });
        cx.refresh().unwrap();
        cx.simulate_keystrokes(&toggle);
        cx.simulate_keystrokes(&explorer);
        cx.run_until_parked();
        cx.update(|window, cx| {
            notes.update(cx, |this, cx| {
                this.toggle_sidebar(window, cx);
                this.show_explorer(window, cx);
                assert!(
                    this.sidebar_collapsed,
                    "guard {guard} must block sidebar actions"
                );
                if guard < 2 {
                    gpui_component::WindowExt::close_dialog(window, cx);
                }
            })
        });
        cx.run_until_parked();
    }
}
