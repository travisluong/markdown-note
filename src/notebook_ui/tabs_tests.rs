//! Retained tabs, real controls and asynchronous ownership on TestPlatform.
use super::persistence_tests::{fail_index_write, fixture};
use crate::{Document, Next, ViewMode, app::navigation::Direction, notebook, tabs::WorkspaceKind};
use gpui::{Modifiers, TestAppContext, VisualTestContext};
use gpui_component::input::{Redo, Undo};
use std::{fs, time::Duration};

fn click(selector: &str, cx: &mut VisualTestContext) {
    cx.refresh().unwrap();
    let bounds = cx
        .debug_bounds(Box::leak(selector.to_owned().into_boxed_str()))
        .unwrap_or_else(|| panic!("missing {selector}"));
    cx.simulate_click(bounds.center(), Modifiers::default());
    cx.run_until_parked();
    cx.refresh().unwrap();
}

#[gpui::test]
fn tab_buttons_route_files_retain_editor_selection_mode_and_history(cx: &mut TestAppContext) {
    let (fixture, notes, cx) = fixture(cx);
    let second = fixture.root.join("second.md");
    fs::write(&second, "second").unwrap();
    let (first, editor, selection) = cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.tabs.active.editor.select(0..8, window, cx);
            this.tabs
                .active
                .editor
                .replace_selection("draft", window, cx);
            this.set_mode(ViewMode::LivePreview, window, cx);
            (
                this.tabs.active.id,
                this.tabs.active.editor.id(),
                this.tabs.active.editor.selection(cx),
            )
        })
    });
    cx.run_until_parked();
    click("tab-add", cx);
    let next = cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            assert!(this.document().is_none());
            assert_eq!(this.tabs.ids().len(), 2);
            let id = this.tabs.active.id;
            this.request(Next::File(second.clone()), window, cx);
            id
        })
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            assert_eq!(this.document().unwrap().path, second);
            assert_eq!(this.tabs.active.id, next);
            this.tabs
                .active
                .editor
                .replace_selection(" unsaved", window, cx);
            this.request(Next::File(fixture.note.clone()), window, cx);
            assert_eq!(this.tabs.active.id, first);
            assert_eq!(this.tabs.active.editor.id(), editor);
            assert_eq!(this.tabs.active.editor.selection(cx), selection);
            assert_eq!(this.tabs.active.mode, ViewMode::LivePreview);
            assert_eq!(this.tabs.active.editor.text(cx).as_ref(), "draft A");
        })
    });
    cx.run_until_parked();
    assert!(!cx.has_pending_prompt());
    cx.dispatch_action(Undo);
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(
            notes.read(cx).tabs.active.editor.text(cx).as_ref(),
            "original A"
        )
    });
    cx.dispatch_action(Redo);
    cx.run_until_parked();
    click(&format!("tab-select-{}", next.0), cx);
    cx.update(|_, cx| {
        let n = notes.read(cx);
        assert_eq!(n.tabs.active.id, next);
        assert!(n.dirty(cx));
    });
    assert_eq!(
        fs::read_dir(&fixture.root)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.path().extension().is_some_and(|x| x == "md"))
            .count(),
        2
    );
}

#[gpui::test]
fn inactive_autosave_uses_tab_identity_even_when_document_and_write_ids_collide(
    cx: &mut TestAppContext,
) {
    let (fixture, notes, cx) = fixture(cx);
    let second = fixture.root.join("second.md");
    fs::write(&second, "second").unwrap();
    let (first, ticket) = cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.tabs
                .active
                .editor
                .replace_selection(" saved first", window, cx);
            this.start_write(None, window, cx);
            let ticket = this.tabs.active.session.write().unwrap();
            let first = this.tabs.active.id;
            this.new_tab(window, cx);
            this.install_document(Some(Document::open(second.clone()).unwrap()), window, cx);
            this.tabs
                .active
                .editor
                .replace_selection(" saved second", window, cx);
            this.start_write(None, window, cx);
            assert_eq!(ticket, this.tabs.active.session.write().unwrap());
            (first, ticket)
        })
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        let n = notes.read(cx);
        assert_ne!(n.tabs.active.id, first);
        assert!(n.tabs.get(first).unwrap().session.write().is_none());
        assert!(n.tabs.active.session.write().is_none());
        assert!(!n.tabs.get(first).unwrap().observed(cx).dirty);
        assert!(!n.dirty(cx));
        assert_eq!(
            n.tabs.active.session.content().document_id(),
            Some(ticket.document)
        );
    });
    assert!(
        fs::read_to_string(&fixture.note)
            .unwrap()
            .contains("saved first")
    );
    assert!(
        fs::read_to_string(&second)
            .unwrap()
            .contains("saved second")
    );
}

#[gpui::test]
fn notebook_switch_retains_drafts_and_owning_notebook_autosave_preferences(
    cx: &mut TestAppContext,
) {
    let (fixture, notes, cx) = fixture(cx);
    let first = cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.tabs
                .active
                .editor
                .replace_selection(" draft", window, cx);
            let id = this.tabs.active.id;
            this.request(
                Next::Notebook {
                    path: fixture.other.clone(),
                    create: false,
                },
                window,
                cx,
            );
            id
        })
    });
    cx.run_until_parked();
    assert!(!cx.has_pending_prompt());
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            assert_eq!(this.tabs.ids().len(), 2);
            assert_eq!(this.document().unwrap().path, fixture.other.join("note.md"));
            this.select_tab(first, window, cx);
            assert!(this.dirty(cx));
            assert_eq!(this.notebooks.active.as_ref().unwrap().root, fixture.other);
            assert_eq!(this.autosave_root(), Some(fixture.root.clone()));
            this.update_autosave(
                notebook::Autosave {
                    enabled: true,
                    delay_ms: 100,
                },
                window,
                cx,
            );
            assert!(
                !this
                    .notebooks
                    .active
                    .as_ref()
                    .unwrap()
                    .settings
                    .autosave
                    .enabled
            );
            this.request(
                Next::Notebook {
                    path: fixture.root.clone(),
                    create: false,
                },
                window,
                cx,
            );
        })
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        let n = notes.read(cx);
        assert_eq!(n.tabs.active.id, first);
        assert_eq!(n.tabs.ids().len(), 2);
        assert!(n.autosave_settings().enabled);
    });
    cx.executor().advance_clock(Duration::from_millis(150));
    cx.run_until_parked();
    assert!(fs::read_to_string(&fixture.note).unwrap().contains("draft"));
    assert!(
        notebook::Notebook::open(&fixture.root, false)
            .unwrap()
            .settings
            .autosave
            .enabled
    );
    assert!(
        !notebook::Notebook::open(&fixture.other, false)
            .unwrap()
            .settings
            .autosave
            .enabled
    );
}

#[gpui::test]
fn close_tab_cancel_discard_and_last_tab_leave_one_empty_tab(cx: &mut TestAppContext) {
    let (_fixture, notes, cx) = fixture(cx);
    let id = cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.tabs
                .active
                .editor
                .replace_selection(" draft", window, cx);
            this.tabs.active.id
        })
    });
    cx.run_until_parked();
    click(&format!("tab-close-{}", id.0), cx);
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    cx.update(|_, cx| {
        let n = notes.read(cx);
        assert_eq!(n.tabs.ids(), vec![id]);
        assert!(n.dirty(cx));
        assert!(!n.is_busy());
    });
    click(&format!("tab-close-{}", id.0), cx);
    cx.simulate_prompt_answer("Discard");
    cx.run_until_parked();
    cx.update(|_, cx| {
        let n = notes.read(cx);
        assert_eq!(n.tabs.ids().len(), 1);
        assert_ne!(n.tabs.active.id, id);
        assert!(n.document().is_none());
        assert!(!n.is_busy());
    });
}

#[gpui::test]
fn workspace_close_cancel_keeps_earlier_discard_and_successful_save_buffers(
    cx: &mut TestAppContext,
) {
    let (fixture, notes, cx) = fixture(cx);
    let paths = [
        fixture.note.clone(),
        fixture.root.join("b.md"),
        fixture.root.join("c.md"),
    ];
    for path in &paths[1..] {
        fs::write(path, "original").unwrap();
    }
    let ids = cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            let mut ids = vec![];
            for (index, path) in paths.iter().enumerate() {
                if index > 0 {
                    this.new_tab(window, cx);
                    this.install_document(Some(Document::open(path.clone()).unwrap()), window, cx);
                }
                this.tabs
                    .active
                    .editor
                    .replace_selection(" draft", window, cx);
                ids.push(this.tabs.active.id);
            }
            ids
        })
    });
    cx.run_until_parked();
    assert!(!cx.simulate_close());
    cx.run_until_parked();
    cx.simulate_prompt_answer("Save");
    cx.run_until_parked();
    cx.simulate_prompt_answer("Discard");
    cx.run_until_parked();
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    cx.update(|_, cx| {
        let n = notes.read(cx);
        assert!(!n.is_busy());
        assert_eq!(n.tabs.ids(), ids);
        for id in &ids {
            assert!(n.tabs.get(*id).unwrap().editor.text(cx).contains("draft"));
        }
        assert!(!n.tabs.get(ids[0]).unwrap().observed(cx).dirty);
        assert!(n.tabs.get(ids[1]).unwrap().observed(cx).dirty);
    });
    assert!(fs::read_to_string(&paths[0]).unwrap().contains("draft"));
    assert_eq!(fs::read_to_string(&paths[1]).unwrap(), "original");
}

#[gpui::test]
fn workspace_preference_failure_after_multiple_discards_preserves_every_tab(
    cx: &mut TestAppContext,
) {
    let (fixture, notes, cx) = fixture(cx);
    fail_index_write(&fixture, &notes, cx);
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.tabs
                .active
                .editor
                .replace_selection(" first draft", window, cx);
            this.new_tab(window, cx);
            this.install_document(
                Some(Document::open(fixture.other.join("note.md")).unwrap()),
                window,
                cx,
            );
            this.tabs
                .active
                .editor
                .replace_selection(" second draft", window, cx);
        })
    });
    cx.run_until_parked();
    assert!(!cx.simulate_close());
    cx.run_until_parked();
    cx.simulate_prompt_answer("Discard");
    cx.run_until_parked();
    cx.simulate_prompt_answer("Discard");
    cx.run_until_parked();
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    cx.update(|_, cx| {
        let n = notes.read(cx);
        assert!(!n.is_busy());
        assert_eq!(n.tabs.ids().len(), 2);
        assert!(n.tabs.iter().all(|t| t.observed(cx).dirty));
    });
}

#[gpui::test]
fn history_focuses_existing_tab_without_changing_either_history(cx: &mut TestAppContext) {
    let (fixture, notes, cx) = fixture(cx);
    let second = fixture.root.join("second.md");
    fs::write(&second, "second").unwrap();
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.request(Next::File(second.clone()), window, cx)
        })
    });
    cx.run_until_parked();
    let first = cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            let id = this.tabs.active.id;
            this.new_tab(window, cx);
            this.request(Next::File(fixture.note.clone()), window, cx);
            id
        })
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            let other = this.tabs.active.id;
            this.select_tab(first, window, cx);
            let back = this.tabs.active.navigation.target(Direction::Back).unwrap();
            this.navigate_history(Direction::Back, window, cx);
            assert_eq!(this.tabs.active.id, other);
            assert!(
                this.tabs
                    .active
                    .navigation
                    .target(Direction::Back)
                    .is_none()
            );
            assert_eq!(
                this.tabs
                    .get(first)
                    .unwrap()
                    .navigation
                    .target(Direction::Back)
                    .unwrap()
                    .path,
                back.path
            );
            assert_eq!(this.tabs.get(first).unwrap().path(), Some(second.as_path()));
        })
    });
    assert!(!cx.has_pending_prompt());
}

#[gpui::test]
fn moves_and_failed_trash_preserve_inactive_descendant_buffers(cx: &mut TestAppContext) {
    let (fixture, notes, cx) = fixture(cx);
    fs::create_dir(fixture.root.join("folder")).unwrap();
    let first = fixture.root.join("folder/a.md");
    let second = fixture.root.join("folder/b.md");
    fs::write(&first, "one").unwrap();
    fs::write(&second, "two").unwrap();
    let ids = cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.install_document(Some(Document::open(first.clone()).unwrap()), window, cx);
            this.tabs
                .active
                .editor
                .replace_selection(" draft A", window, cx);
            let a = this.tabs.active.id;
            this.new_tab(window, cx);
            this.install_document(Some(Document::open(second.clone()).unwrap()), window, cx);
            this.tabs
                .active
                .editor
                .replace_selection(" draft B", window, cx);
            let b = this.tabs.active.id;
            this.begin_workspace(
                WorkspaceKind::Move {
                    from: "folder".into(),
                    parent: "".into(),
                    name: Some("renamed".into()),
                },
                this.tabs.ids(),
                window,
                cx,
            );
            vec![a, b]
        })
    });
    cx.run_until_parked();
    assert!(!cx.has_pending_prompt());
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            assert!(!this.is_busy());
            for id in &ids {
                let tab = this.tabs.get(*id).unwrap();
                assert!(
                    tab.path()
                        .unwrap()
                        .starts_with(fixture.root.join("renamed"))
                );
                assert!(tab.observed(cx).dirty);
            }
            let target = super::EntryTarget {
                root: fixture.root.clone(),
                path: "renamed".into(),
                generation: this.notebooks.explorer.generation(),
                directory: true,
            };
            this.begin_workspace(WorkspaceKind::Trash { target }, this.tabs.ids(), window, cx);
        })
    });
    cx.run_until_parked();
    cx.simulate_prompt_answer("Discard");
    cx.run_until_parked();
    cx.simulate_prompt_answer("Discard");
    cx.run_until_parked();
    cx.update(|_, cx| {
        let n = notes.read(cx);
        assert!(!n.is_busy());
        assert!(n.notebooks.notice.as_ref().unwrap().contains("Could not"));
        for id in &ids {
            assert!(n.tabs.get(*id).unwrap().observed(cx).dirty);
        }
    });
}

#[cfg(feature = "editor-diagnostics")]
#[gpui::test]
fn reading_state_survives_switching_tabs(cx: &mut TestAppContext) {
    let (fixture, notes, cx) = fixture(cx);
    let text = (0..100)
        .map(|i| format!("Paragraph {i}\n\n"))
        .collect::<String>();
    fs::write(&fixture.note, &text).unwrap();
    let first = cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.install_document(
                Some(Document::open(fixture.note.clone()).unwrap()),
                window,
                cx,
            );
            this.set_mode(ViewMode::Reading, window, cx);
            this.tabs.active.id
        })
    });
    cx.run_until_parked();
    cx.refresh().unwrap();
    let probe = cx.update(|_, cx| {
        notes
            .read(cx)
            .tabs
            .active
            .reading_view
            .as_ref()
            .unwrap()
            .diagnostic_probe()
    });
    cx.update(|_, cx| probe.scroll_to(30, cx));
    cx.refresh().unwrap();
    let before = cx.update(|_, cx| probe.snapshot(cx).unwrap().scroll_item);
    assert!(before > 0);
    cx.update(|window, cx| notes.update(cx, |this, cx| this.new_tab(window, cx)));
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(probe.snapshot(cx).unwrap().scroll_item, before));
    cx.update(|window, cx| notes.update(cx, |this, cx| this.select_tab(first, window, cx)));
    cx.run_until_parked();
    cx.refresh().unwrap();
    cx.update(|_, cx| {
        assert_eq!(probe.snapshot(cx).unwrap().scroll_item, before);
        assert_eq!(notes.read(cx).tabs.active.mode, ViewMode::Reading);
    });
}

#[gpui::test]
fn failed_inactive_save_cancels_workspace_without_losing_buffers(cx: &mut TestAppContext) {
    let (fixture, notes, cx) = fixture(cx);
    let first = cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.tabs
                .active
                .editor
                .replace_selection("draft", window, cx);
            let id = this.tabs.active.id;
            this.new_tab(window, cx);
            id
        })
    });
    cx.run_until_parked();
    fs::write(&fixture.note, "external change").unwrap();
    cx.update(|window, cx| notes.update(cx, |this, cx| this.close_tab(first, window, cx)));
    cx.run_until_parked();
    cx.simulate_prompt_answer("Save");
    cx.run_until_parked();
    cx.update(|_, cx| {
        let n = notes.read(cx);
        assert!(!n.is_busy());
        assert_eq!(n.tabs.ids().len(), 2);
        let tab = n.tabs.get(first).unwrap();
        assert!(tab.observed(cx).dirty);
        assert!(tab.session.save_conflicted());
        assert!(tab.saves.error.is_some());
        assert!(tab.session.foreground().is_none());
    });
}

#[gpui::test]
fn queued_background_failure_releases_another_tabs_pending_close_prompt(cx: &mut TestAppContext) {
    let (fixture, notes, cx) = fixture(cx);
    let second = fixture.root.join("b.md");
    fs::write(&second, "second").unwrap();
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.tabs
                .active
                .editor
                .replace_selection("draft A", window, cx);
            let first = this.tabs.active.id;
            this.new_tab(window, cx);
            this.install_document(Some(Document::open(second.clone()).unwrap()), window, cx);
            this.tabs
                .active
                .editor
                .replace_selection("draft B", window, cx);
            fs::write(&second, "external").unwrap();
            this.start_write(None, window, cx);
            this.select_tab(first, window, cx);
            this.request(Next::Close, window, cx);
        })
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        let n = notes.read(cx);
        assert!(!n.is_busy());
        assert!(n.workspace.is_none());
        assert!(n.tabs.iter().all(|tab| tab.session.foreground().is_none()));
        assert!(n.tabs.iter().all(|tab| tab.observed(cx).dirty));
    });
    if cx.has_pending_prompt() {
        cx.simulate_prompt_answer("Discard");
        cx.run_until_parked();
    }
    cx.update(|_, cx| {
        let n = notes.read(cx);
        assert_eq!(n.tabs.ids().len(), 2);
        assert!(!n.is_busy());
    });
}

#[gpui::test]
fn cancelled_load_cannot_install_in_another_tab_with_colliding_operation_id(
    cx: &mut TestAppContext,
) {
    let (fixture, notes, cx) = fixture(cx);
    let pending = fixture.root.join("pending.md");
    let target = fixture.root.join("target.md");
    fs::write(&pending, "wrong").unwrap();
    fs::write(&target, "correct").unwrap();
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.request(Next::File(pending.clone()), window, cx);
            let operation = this.tabs.active.session.foreground().unwrap().0;
            let observed = this.observed_document(cx);
            let effects = this.tabs.active.session.cancel(operation, observed);
            this.apply_session_effects(effects, window, cx);
            this.new_tab(window, cx);
            this.install_document(None, window, cx);
            this.request(Next::File(target.clone()), window, cx);
            assert_eq!(operation, this.tabs.active.session.foreground().unwrap().0);
        })
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        let n = notes.read(cx);
        assert_eq!(n.tabs.active.editor.text(cx).as_ref(), "correct");
        assert_eq!(n.document().unwrap().path, target);
        assert_eq!(n.tabs.ids().len(), 2);
    });
}

#[gpui::test]
fn search_scan_uses_all_retained_overlays_and_filters_notebook_root(cx: &mut TestAppContext) {
    let (fixture, notes, cx) = fixture(cx);
    let second = fixture.root.join("b.md");
    fs::write(&second, "on disk").unwrap();
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.tabs
                .active
                .editor
                .replace_selection("needle first", window, cx);
            this.new_tab(window, cx);
            this.install_document(Some(Document::open(second.clone()).unwrap()), window, cx);
            this.tabs
                .active
                .editor
                .replace_selection("needle second", window, cx);
            this.new_tab(window, cx);
            this.install_document(
                Some(Document::open(fixture.other.join("note.md")).unwrap()),
                window,
                cx,
            );
            this.tabs
                .active
                .editor
                .replace_selection("needle outside", window, cx);
        })
    });
    cx.run_until_parked();
    let overlays = cx.update(|_, cx| {
        notes
            .read(cx)
            .tabs
            .iter()
            .filter_map(|tab| {
                tab.document()
                    .map(|doc| (doc.path.clone(), tab.editor.snapshot(cx)))
            })
            .collect()
    });
    let results = crate::search::scan_many(
        &fixture.root,
        &notebook::Exclusions::default(),
        "needle",
        true,
        overlays,
        &std::sync::atomic::AtomicBool::new(false),
    );
    assert_eq!(results.hits.len(), 2);
    assert!(
        results
            .hits
            .iter()
            .all(|hit| hit.relative == std::path::Path::new("note.md")
                || hit.relative == std::path::Path::new("b.md"))
    );
}

#[gpui::test]
fn successful_trash_clears_every_affected_tab_only_after_success(cx: &mut TestAppContext) {
    let (fixture, notes, cx) = fixture(cx);
    fs::create_dir(fixture.root.join("folder")).unwrap();
    let a = fixture.root.join("folder/a.md");
    let b = fixture.root.join("folder/b.md");
    fs::write(&a, "a").unwrap();
    fs::write(&b, "b").unwrap();
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.install_document(Some(Document::open(a.clone()).unwrap()), window, cx);
            this.tabs
                .active
                .editor
                .replace_selection("draft a", window, cx);
            this.new_tab(window, cx);
            this.install_document(Some(Document::open(b.clone()).unwrap()), window, cx);
            this.tabs
                .active
                .editor
                .replace_selection("draft b", window, cx);
            this.notebooks.trash_backend = |root, path| fs::remove_dir_all(root.join(path));
            let target = super::EntryTarget {
                root: fixture.root.clone(),
                path: "folder".into(),
                generation: this.notebooks.explorer.generation(),
                directory: true,
            };
            this.begin_workspace(WorkspaceKind::Trash { target }, this.tabs.ids(), window, cx);
        })
    });
    cx.run_until_parked();
    cx.simulate_prompt_answer("Discard");
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert!(
            notes
                .read(cx)
                .tabs
                .iter()
                .all(|tab| tab.document().is_some())
        )
    });
    cx.simulate_prompt_answer("Discard");
    cx.run_until_parked();
    assert!(!fixture.root.join("folder").exists());
    cx.update(|_, cx| {
        let n = notes.read(cx);
        assert!(!n.is_busy());
        assert_eq!(n.tabs.ids().len(), 2);
        assert!(n.tabs.iter().all(|tab| tab.path().is_none()
            && tab.editor.text(cx).is_empty()
            && tab.navigation.target(Direction::Back).is_none()));
    });
}

#[gpui::test]
fn duplicate_uses_unsaved_source_from_an_inactive_tab(cx: &mut TestAppContext) {
    let (fixture, notes, cx) = fixture(cx);
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.tabs.active.editor.select(0..10, window, cx);
            this.tabs
                .active
                .editor
                .replace_selection("retained source", window, cx);
            this.new_tab(window, cx);
            let target = super::EntryTarget {
                root: fixture.root.clone(),
                path: "note.md".into(),
                generation: this.notebooks.explorer.generation(),
                directory: false,
            };
            this.request(
                Next::Entry {
                    target,
                    action: super::EntryAction::Duplicate,
                },
                window,
                cx,
            );
        })
    });
    cx.run_until_parked();
    let copied = fs::read_dir(&fixture.root)
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .find(|path| path != &fixture.note && path.extension().is_some_and(|ext| ext == "md"))
        .unwrap();
    assert_eq!(fs::read_to_string(copied).unwrap(), "retained source");
    assert_eq!(fs::read_to_string(&fixture.note).unwrap(), "original A");
}
