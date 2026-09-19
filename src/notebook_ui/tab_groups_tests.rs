//! Notebook groups retain views while lifecycle work still sees every tab.
use super::persistence_tests::fixture;
use crate::{Document, Next, Notes, ViewMode, app::navigation::Direction, tabs::WorkspaceKind};
use gpui::{Entity, TestAppContext, VisualTestContext};
use gpui_component::input::{Redo, Undo};
use std::{fs, path::Path};

fn switch(notes: &Entity<Notes>, root: &Path, cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.request(
                Next::Notebook {
                    path: root.into(),
                    create: false,
                },
                window,
                cx,
            );
        })
    });
    cx.run_until_parked();
}

#[gpui::test]
fn round_trip_restores_order_selection_editors_history_and_empty_tabs(cx: &mut TestAppContext) {
    let (fixture, notes, cx) = fixture(cx);
    let second = fixture.root.join("second.md");
    fs::write(&second, "second").unwrap();
    // Build actual navigation history before editing the original note.
    for path in [&second, &fixture.note] {
        cx.update(|window, cx| {
            notes.update(cx, |this, cx| {
                this.open_path(path.clone(), false, window, cx)
            })
        });
        cx.run_until_parked();
    }
    let (ids, editor, selection, back) = cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            let first = this.tabs.active.id;
            this.editor.select(0..10, window, cx);
            this.editor.replace_selection("draft A", window, cx);
            this.editor.select(2..5, window, cx);
            this.set_mode(ViewMode::LivePreview, window, cx);
            let editor = this.editor.id();
            let selection = this.editor.selection(cx);
            let back = this.navigation.target(Direction::Back);
            this.new_tab(window, cx);
            this.install_document(Some(Document::open(second.clone()).unwrap()), window, cx);
            this.set_mode(ViewMode::Reading, window, cx);
            this.new_tab(window, cx);
            this.select_tab(first, window, cx);
            (this.tabs.visible_ids().to_vec(), editor, selection, back)
        })
    });
    switch(&notes, &fixture.other, cx);
    let b = cx.update(|_, cx| {
        let this = notes.read(cx);
        assert_eq!(this.tabs.visible_ids(), &[this.tabs.active.id]);
        assert!(ids.iter().all(|id| !this.tabs.visible_ids().contains(id)));
        this.tabs.active.id
    });
    for _ in 0..3 {
        switch(&notes, &fixture.root, cx);
        switch(&notes, &fixture.root, cx); // Reselecting must not replace any view.
        cx.update(|_, cx| {
            let this = notes.read(cx);
            assert_eq!(this.tabs.visible_ids(), ids);
            assert_eq!(this.tabs.ids().len(), ids.len() + 1);
            assert_eq!(this.tabs.active.id, ids[0]);
            assert_eq!(this.editor.id(), editor);
            assert_eq!(this.editor.selection(cx), selection);
            assert_eq!(this.editor.text(cx).as_ref(), "draft A");
            assert_eq!(this.mode, ViewMode::LivePreview);
            assert_eq!(this.navigation.target(Direction::Back), back);
            assert_eq!(this.tabs.get(ids[1]).unwrap().mode, ViewMode::Reading);
            assert!(this.tabs.get(ids[2]).unwrap().path().is_none());
        });
        switch(&notes, &fixture.other, cx);
        cx.update(|_, cx| assert_eq!(notes.read(cx).tabs.active.id, b));
    }
    switch(&notes, &fixture.root, cx);
    assert!(!cx.has_pending_prompt());
    cx.dispatch_action(Undo);
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(notes.read(cx).editor.text(cx).as_ref(), "original A"));
    cx.dispatch_action(Redo);
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(notes.read(cx).editor.text(cx).as_ref(), "draft A"));
    // An explicitly selected empty tab is restored, even though no last file exists.
    cx.update(|window, cx| notes.update(cx, |this, cx| this.select_tab(ids[2], window, cx)));
    switch(&notes, &fixture.other, cx);
    switch(&notes, &fixture.root, cx);
    cx.update(|_, cx| {
        let this = notes.read(cx);
        assert_eq!(this.tabs.active.id, ids[2]);
        assert!(this.document().is_none());
    });
}

#[gpui::test]
fn retained_group_ignores_unreadable_last_file_and_failed_activation_keeps_current_group(
    cx: &mut TestAppContext,
) {
    let (fixture, notes, cx) = fixture(cx);
    let first = cx.update(|_, cx| notes.read(cx).tabs.active.id);
    switch(&notes, &fixture.other, cx);
    // A directory causes a real document-read failure if restoration tries to reload it.
    let mut book = crate::notebook::Notebook::open(&fixture.root, false).unwrap();
    book.settings.last_file = Some("unreadable.md".into());
    fs::create_dir(fixture.root.join("unreadable.md")).unwrap();
    crate::notebook::save_book(&book).unwrap();
    switch(&notes, &fixture.root, cx);
    cx.update(|_, cx| {
        let this = notes.read(cx);
        assert_eq!(this.notebooks.active.as_ref().unwrap().root, fixture.root);
        assert_eq!(this.tabs.active.id, first);
        assert_eq!(this.editor.text(cx).as_ref(), "original A");
    });
    let missing = fixture.root.parent().unwrap().join("missing");
    switch(&notes, &missing, cx);
    cx.update(|_, cx| {
        let this = notes.read(cx);
        assert_eq!(this.notebooks.active.as_ref().unwrap().root, fixture.root);
        assert_eq!(this.tabs.visible_ids(), &[first]);
        assert_eq!(this.tabs.active.id, first);
        assert!(!this.tabs.has_group(&missing));
        assert!(!this.is_busy());
    });
}

#[gpui::test]
fn removing_notebooks_parks_drafts_and_restores_standalone_tabs(cx: &mut TestAppContext) {
    let (fixture, notes, cx) = fixture(cx);
    let first = cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.editor.replace_selection("draft ", window, cx);
            let first = this.tabs.active.id;
            this.remove_notebook(fixture.root.clone(), window, cx);
            first
        })
    });
    cx.run_until_parked();
    assert!(!cx.has_pending_prompt());
    let standalone = cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            assert!(this.notebooks.active.is_none());
            assert_eq!(this.tabs.visible_ids().len(), 1);
            assert!(this.tabs.active.path().is_none());
            let standalone = this.tabs.active.id;
            this.install_document(
                Some(Document::open(fixture.other.join("note.md")).unwrap()),
                window,
                cx,
            );
            this.editor.replace_selection("standalone ", window, cx);
            standalone
        })
    });
    switch(&notes, &fixture.root, cx);
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            assert_eq!(this.tabs.visible_ids(), &[first]);
            assert_eq!(this.editor.text(cx).as_ref(), "draft original A");
            this.remove_notebook(fixture.root.clone(), window, cx);
        })
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        let this = notes.read(cx);
        assert!(this.notebooks.active.is_none());
        assert_eq!(this.tabs.active.id, standalone);
        assert_eq!(this.editor.text(cx).as_ref(), "standalone original B");
    });
    switch(&notes, &fixture.other, cx);
    // Forget an inactive notebook without destroying its retained group.
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.remove_notebook(fixture.root.clone(), window, cx)
        })
    });
    switch(&notes, &fixture.root, cx);
    cx.update(|_, cx| assert_eq!(notes.read(cx).tabs.active.id, first));
    assert!(!cx.has_pending_prompt());
}

#[gpui::test]
fn closing_last_visible_tab_keeps_other_notebook_views(cx: &mut TestAppContext) {
    let (fixture, notes, cx) = fixture(cx);
    let first = cx.update(|_, cx| notes.read(cx).tabs.active.id);
    switch(&notes, &fixture.other, cx);
    let empty = cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            let closing = this.tabs.active.id;
            this.close_tab(closing, window, cx);
            assert!(this.tabs.get(closing).is_none());
            assert!(this.tabs.get(first).is_some());
            assert_eq!(this.tabs.visible_ids().len(), 1);
            assert!(this.tabs.active.path().is_none());
            this.tabs.active.id
        })
    });
    switch(&notes, &fixture.root, cx);
    cx.update(|_, cx| assert_eq!(notes.read(cx).tabs.active.id, first));
    switch(&notes, &fixture.other, cx);
    cx.update(|_, cx| assert_eq!(notes.read(cx).tabs.active.id, empty));
}

#[gpui::test]
fn close_saves_hidden_draft_and_cancel_preserves_all_groups(cx: &mut TestAppContext) {
    let (fixture, notes, cx) = fixture(cx);
    let a = cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.editor.replace_selection("draft A ", window, cx);
            this.tabs.active.id
        })
    });
    switch(&notes, &fixture.other, cx);
    let b = cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.editor.replace_selection("draft B ", window, cx);
            this.tabs.active.id
        })
    });
    assert!(!cx.simulate_close());
    cx.run_until_parked();
    cx.simulate_prompt_answer("Save");
    cx.run_until_parked();
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    cx.update(|_, cx| {
        let this = notes.read(cx);
        assert_eq!(this.tabs.active.id, b);
        assert_eq!(this.tabs.visible_ids(), &[b]);
        assert!(!this.is_busy());
        assert!(!this.tabs.get(a).unwrap().observed(cx).dirty);
        assert!(this.tabs.get(b).unwrap().observed(cx).dirty);
    });
    assert_eq!(
        fs::read_to_string(&fixture.note).unwrap(),
        "draft A original A"
    );
    switch(&notes, &fixture.root, cx);
    cx.update(|_, cx| assert_eq!(notes.read(cx).tabs.active.id, a));
}

#[gpui::test]
fn shared_documents_move_and_trash_update_hidden_views(cx: &mut TestAppContext) {
    let (fixture, notes, cx) = fixture(cx);
    let a = cx.update(|_, cx| notes.read(cx).tabs.active.id);
    switch(&notes, &fixture.other, cx);
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.open_path(fixture.note.clone(), false, window, cx)
        })
    });
    cx.run_until_parked();
    let b = cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            let b = this.tabs.active.id;
            this.editor.replace_selection("shared ", window, cx);
            assert_eq!(
                this.tabs.get(a).unwrap().editor.text(cx),
                this.editor.text(cx)
            );
            assert_eq!(this.document_tabs(this.document().unwrap().id).len(), 2);
            b
        })
    });
    switch(&notes, &fixture.root, cx);
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.begin_workspace(
                WorkspaceKind::Move {
                    from: "note.md".into(),
                    parent: "".into(),
                    name: Some("renamed.md".into()),
                },
                this.tabs.ids(),
                window,
                cx,
            );
        })
    });
    cx.run_until_parked();
    assert!(!cx.has_pending_prompt());
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            for id in [a, b] {
                let tab = this.tabs.get(id).unwrap();
                assert_eq!(tab.path(), Some(fixture.root.join("renamed.md").as_path()));
                assert!(tab.observed(cx).dirty);
            }
            this.notebooks.trash_backend = |root, path| fs::remove_file(root.join(path));
            let target = super::EntryTarget {
                root: fixture.root.clone(),
                path: "renamed.md".into(),
                generation: this.notebooks.explorer.generation(),
                directory: false,
            };
            this.begin_workspace(WorkspaceKind::Trash { target }, this.tabs.ids(), window, cx);
        })
    });
    cx.run_until_parked();
    cx.simulate_prompt_answer("Discard");
    cx.run_until_parked();
    assert!(!cx.has_pending_prompt());
    cx.update(|_, cx| {
        let this = notes.read(cx);
        for id in [a, b] {
            let tab = this.tabs.get(id).unwrap();
            assert!(tab.path().is_none());
            assert!(tab.editor.text(cx).is_empty());
        }
    });
    switch(&notes, &fixture.other, cx);
    cx.update(|_, cx| {
        let this = notes.read(cx);
        assert_eq!(this.tabs.active.id, b);
        assert_eq!(this.tabs.visible_ids(), &[b]);
        assert!(this.document().is_none());
    });
}

#[gpui::test]
fn notebook_groups_restore_tab_strip_scroll(cx: &mut TestAppContext) {
    let (fixture, notes, cx) = fixture(cx);
    cx.simulate_resize(gpui::size(gpui::px(700.), gpui::px(600.)));
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            let first = this.tabs.active.id;
            for _ in 0..20 {
                this.new_tab(window, cx);
            }
            this.select_tab(first, window, cx);
        })
    });
    cx.run_until_parked();
    cx.refresh().unwrap();
    cx.update(|_, cx| {
        notes
            .read(cx)
            .tabs
            .scroll_handle()
            .set_offset(gpui::point(gpui::px(-500.), gpui::px(0.)))
    });
    cx.refresh().unwrap();
    let offset = cx.update(|_, cx| notes.read(cx).tabs.scroll_handle().offset());
    assert!(offset.x < gpui::px(0.));
    switch(&notes, &fixture.other, cx);
    cx.refresh().unwrap();
    cx.update(|_, cx| assert_eq!(notes.read(cx).tabs.scroll_handle().offset().x, gpui::px(0.)));
    switch(&notes, &fixture.root, cx);
    cx.refresh().unwrap();
    cx.update(|_, cx| assert_eq!(notes.read(cx).tabs.scroll_handle().offset(), offset));
}

#[gpui::test]
fn stale_notebook_result_cannot_switch_groups_or_append_tabs(cx: &mut TestAppContext) {
    let (fixture, notes, cx) = fixture(cx);
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.notebooks.activation_install_delay = std::time::Duration::from_secs(2);
            this.request(
                Next::Notebook {
                    path: fixture.other.clone(),
                    create: false,
                },
                window,
                cx,
            );
        })
    });
    cx.run_until_parked();
    let a = cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            let (operation, _) = this.session.foreground().unwrap();
            let observed = this.observed_document(cx);
            let effects = this.session.cancel(operation, observed);
            this.apply_session_effects(effects, window, cx);
            this.new_tab(window, cx);
            this.tabs.visible_ids().to_vec()
        })
    });
    switch(&notes, &fixture.other, cx);
    let b = cx.update(|_, cx| notes.read(cx).tabs.active.id);
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(2));
    cx.run_until_parked();
    cx.update(|_, cx| {
        let this = notes.read(cx);
        assert_eq!(this.tabs.active.id, b);
        assert_eq!(this.tabs.visible_ids(), &[b]);
        assert_eq!(this.tabs.ids().len(), a.len() + 1);
        assert!(!this.is_busy());
    });
    switch(&notes, &fixture.root, cx);
    cx.update(|_, cx| {
        let this = notes.read(cx);
        assert_eq!(this.tabs.visible_ids(), a);
        assert_eq!(this.tabs.active.id, *a.last().unwrap());
    });
}
