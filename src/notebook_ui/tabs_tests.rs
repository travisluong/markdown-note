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

fn assert_content_fills_below_tabs(selector: &str, cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.refresh().unwrap();
    let pane = cx.debug_bounds("file-drop-content").unwrap();
    let tabs = cx.debug_bounds("document-tabs").unwrap();
    let content = cx
        .debug_bounds(Box::leak(selector.to_owned().into_boxed_str()))
        .unwrap_or_else(|| panic!("missing {selector}"));
    let tolerance = gpui::px(1.);
    assert!(pane.size.height > gpui::px(200.));
    assert!(tabs.size.height > gpui::px(0.));
    assert!((tabs.top() - pane.top()).abs() <= tolerance);
    assert!(
        (content.top() - tabs.bottom()).abs() <= tolerance,
        "{selector} must begin below tabs: content={content:?}, tabs={tabs:?}"
    );
    assert!(
        (content.bottom() - pane.bottom()).abs() <= tolerance,
        "{selector} must fill remaining pane height: content={content:?}, pane={pane:?}"
    );
    assert!((content.left() - pane.left()).abs() <= tolerance);
    assert!((content.right() - pane.right()).abs() <= tolerance);
}

#[gpui::test]
fn tab_content_fills_remaining_height_across_modes_resizes_and_selection(cx: &mut TestAppContext) {
    let (fixture, notes, cx) = fixture(cx);
    let second = fixture.root.join("second.md");
    fs::write(&second, "# Second\n\nA multiline note.\n\nMore text.").unwrap();
    let (first, second) = cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            let first = this.tabs.active.id;
            this.new_tab(window, cx);
            this.install_document(Some(Document::open(second).unwrap()), window, cx);
            (first, this.tabs.active.id)
        })
    });
    for (width, height) in [(1000., 800.), (720., 500.)] {
        cx.simulate_resize(gpui::size(gpui::px(width), gpui::px(height)));
        for collapsed in [false, true] {
            for tab in [first, second, first] {
                for mode in [ViewMode::Source, ViewMode::LivePreview, ViewMode::Reading] {
                    let selector = cx.update(|window, cx| {
                        notes.update(cx, |this, cx| {
                            this.sidebar_collapsed = collapsed;
                            this.select_tab(tab, window, cx);
                            this.set_mode(mode, window, cx);
                            if mode == ViewMode::Reading {
                                "reading-content".to_owned()
                            } else {
                                format!("input-{}", this.tabs.active.editor.id())
                            }
                        })
                    });
                    assert_content_fills_below_tabs(&selector, cx);
                }
            }
            let empty = cx.update(|window, cx| {
                notes.update(cx, |this, cx| {
                    this.new_tab(window, cx);
                    this.tabs.active.id
                })
            });
            assert_content_fills_below_tabs("empty-content", cx);
            cx.update(|window, cx| notes.update(cx, |this, cx| this.close_tab(empty, window, cx)));
        }
    }
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
            this.select_tab(first, window, cx);
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
        assert_ne!(n.tabs.active.id, first);
        assert_eq!(n.tabs.ids().len(), 3);
        assert_eq!(
            n.tabs.active.document().unwrap().id,
            n.tabs.get(first).unwrap().document().unwrap().id
        );
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
fn history_opens_existing_document_in_current_tab(cx: &mut TestAppContext) {
    let (fixture, notes, cx) = fixture(cx);
    let second = fixture.root.join("second.md");
    fs::write(&second, "second").unwrap();
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.request(Next::File(second.clone()), window, cx)
        })
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            let first = this.tabs.active.id;
            this.new_tab(window, cx);
            this.request(Next::File(fixture.note.clone()), window, cx);
            first
        })
    });
    cx.run_until_parked();
    let first = cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            let first = this.tabs.ids()[0];
            this.select_tab(first, window, cx);
            this.navigate_history(Direction::Back, window, cx);
            first
        })
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            assert_eq!(this.tabs.active.id, first);
            assert_eq!(this.tabs.active.path(), Some(fixture.note.as_path()));
            assert_eq!(
                this.tabs
                    .active
                    .navigation
                    .target(Direction::Forward)
                    .unwrap()
                    .path,
                second
            );
            assert!(
                this.tabs
                    .get(this.tabs.ids()[1])
                    .unwrap()
                    .navigation
                    .target(Direction::Back)
                    .is_none()
            );
            this.navigate_history(Direction::Forward, window, cx);
        })
    });
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(notes.read(cx).tabs.active.path(), Some(second.as_path())));
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
            this.install_document(None, window, cx);
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
        assert!(n.save_state(first).error.is_some());
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
#[gpui::test]
fn shared_notes_keep_live_text_history_modes_and_independent_selection(cx: &mut TestAppContext) {
    let (fixture, notes, cx) = fixture(cx);
    let (first, second) = cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            let first = this.tabs.active.id;
            this.tabs.active.editor.select(0..8, window, cx);
            this.tabs
                .active
                .editor
                .replace_selection("draft", window, cx);
            this.tabs.active.editor.select(6..7, window, cx);
            this.set_mode(ViewMode::LivePreview, window, cx);
            this.new_tab(window, cx);
            let second = this.tabs.active.id;
            this.request(Next::TreeFile(fixture.note.clone()), window, cx);
            assert_eq!(this.tabs.active.id, second);
            assert_eq!(this.tabs.active.editor.text(cx).as_ref(), "draft A");
            assert_ne!(
                this.tabs.active.editor.id(),
                this.tabs.get(first).unwrap().editor.id()
            );
            assert_eq!(this.tabs.active.editor.selection(cx), 0..0);
            assert_eq!(this.tabs.get(first).unwrap().editor.selection(cx), 6..7);
            assert_eq!(this.documents.len(), 1);
            this.tabs
                .active
                .editor
                .replace_selection("你好 ", window, cx);
            assert_eq!(
                this.tabs.get(first).unwrap().editor.text(cx).as_ref(),
                "你好 draft A"
            );
            assert_eq!(this.tabs.get(first).unwrap().editor.selection(cx), 13..14);
            this.set_mode(ViewMode::Source, window, cx);
            this.select_tab(first, window, cx);
            assert_eq!(this.tabs.active.mode, ViewMode::LivePreview);
            (first, second)
        })
    });
    cx.run_until_parked();
    assert!(!cx.has_pending_prompt());
    cx.dispatch_action(Undo);
    cx.run_until_parked();
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            assert_eq!(this.tabs.active.editor.text(cx).as_ref(), "draft A");
            assert_eq!(
                this.tabs.get(second).unwrap().editor.text(cx).as_ref(),
                "draft A"
            );
            this.select_tab(second, window, cx);
        })
    });
    cx.dispatch_action(Redo);
    cx.run_until_parked();
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            assert_eq!(
                this.tabs.get(first).unwrap().editor.text(cx).as_ref(),
                "你好 draft A"
            );
            this.set_mode(ViewMode::Reading, window, cx);
            this.select_tab(first, window, cx);
            this.tabs.active.editor.select(0..0, window, cx);
            this.tabs.active.editor.replace_selection("# ", window, cx);
        })
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        let n = notes.read(cx);
        assert_eq!(
            n.tabs.get(second).unwrap().reading_text.as_ref(),
            "# 你好 draft A"
        );
        assert!(n.tabs.iter().all(|tab| tab.dirty_cached));
    });
}

#[gpui::test]
fn shared_note_save_and_closing_one_view_retain_the_document(cx: &mut TestAppContext) {
    let (fixture, notes, cx) = fixture(cx);
    let first = cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            let first = this.tabs.active.id;
            this.new_tab(window, cx);
            this.request(Next::File(fixture.root.join("./note.md")), window, cx);
            this.tabs.active.editor.select(0..10, window, cx);
            this.tabs
                .active
                .editor
                .replace_selection("shared draft", window, cx);
            this.close_tab(first, window, cx);
            assert_eq!(this.tabs.ids().len(), 1);
            first
        })
    });
    cx.run_until_parked();
    assert!(!cx.has_pending_prompt());
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            assert!(this.tabs.get(first).is_none());
            assert!(this.dirty(cx));
            this.save(Next::Nothing, window, cx);
        })
    });
    cx.run_until_parked();
    assert_eq!(fs::read_to_string(&fixture.note).unwrap(), "shared draft");
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            assert!(!this.dirty(cx));
            this.tabs.active.editor.replace_selection("!", window, cx);
            this.close_tab(this.tabs.active.id, window, cx);
        })
    });
    cx.run_until_parked();
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    cx.update(|_, cx| assert!(notes.read(cx).dirty(cx)));
}

#[gpui::test]
fn shared_note_navigation_retains_draft_without_prompt_and_picker_stays_in_tab(
    cx: &mut TestAppContext,
) {
    let (fixture, notes, cx) = fixture(cx);
    let other = fixture.root.join("other.md");
    fs::write(&other, "other").unwrap();
    let second = cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.tabs
                .active
                .editor
                .replace_selection("draft ", window, cx);
            this.new_tab(window, cx);
            let second = this.tabs.active.id;
            this.request(Next::File(fixture.note.clone()), window, cx);
            this.request(Next::File(other.clone()), window, cx);
            second
        })
    });
    cx.run_until_parked();
    assert!(!cx.has_pending_prompt());
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            assert_eq!(this.tabs.active.id, second);
            assert_eq!(this.tabs.active.path(), Some(other.as_path()));
            let _ = this.tabs.active.session.pick();
            let operation = this.tabs.active.session.foreground().unwrap().0;
            this.finish_file_picker(operation, Ok(Some(fixture.note.clone())), window, cx);
            assert_eq!(this.tabs.active.id, second);
            assert_eq!(
                this.tabs.active.editor.text(cx).as_ref(),
                "draft original A"
            );
            this.save(Next::Nothing, window, cx);
        })
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert!(
            notes
                .read(cx)
                .tabs
                .iter()
                .all(|tab| !tab.observed(cx).dirty && !tab.dirty_cached)
        )
    });
    assert_eq!(
        fs::read_to_string(&fixture.note).unwrap(),
        "draft original A"
    );
}
#[gpui::test]
fn shared_pending_save_accepts_captured_text_and_closing_origin_preserves_late_edits(
    cx: &mut TestAppContext,
) {
    let (fixture, notes, cx) = fixture(cx);
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            let first = this.tabs.active.id;
            this.new_tab(window, cx);
            this.request(Next::File(fixture.note.clone()), window, cx);
            this.select_tab(first, window, cx);
            this.tabs
                .active
                .editor
                .replace_selection("saved ", window, cx);
            this.start_write(None, window, cx);
            let ticket = this.tabs.active.session.write().unwrap();
            let second = this.tabs.ids()[1];
            this.select_tab(second, window, cx);
            assert_eq!(this.tabs.active.session.write(), Some(ticket));
            this.tabs.active.editor.select(0..0, window, cx);
            this.tabs
                .active
                .editor
                .replace_selection("late ", window, cx);
            this.close_tab(first, window, cx);
        })
    });
    cx.run_until_parked();
    assert!(!cx.has_pending_prompt());
    assert_eq!(
        fs::read_to_string(&fixture.note).unwrap(),
        "saved original A"
    );
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            assert_eq!(this.tabs.ids().len(), 1);
            assert_eq!(
                this.tabs.active.editor.text(cx).as_ref(),
                "late saved original A"
            );
            assert!(this.dirty(cx));
            assert!(this.tabs.active.session.write().is_none());
            this.save(Next::Nothing, window, cx);
        })
    });
    cx.run_until_parked();
    assert_eq!(
        fs::read_to_string(&fixture.note).unwrap(),
        "late saved original A"
    );
}

#[gpui::test]
fn shared_move_and_trash_prepare_once_and_preserve_views_on_failure(cx: &mut TestAppContext) {
    let (fixture, notes, cx) = fixture(cx);
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.tabs
                .active
                .editor
                .replace_selection("draft ", window, cx);
            this.new_tab(window, cx);
            this.request(Next::File(fixture.note.clone()), window, cx);
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
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            assert!(
                this.tabs
                    .iter()
                    .all(|tab| tab.path() == Some(fixture.root.join("renamed.md").as_path()))
            );
            assert_eq!(this.documents.len(), 1);
            assert!(
                this.documents
                    .contains_key(&fixture.root.join("renamed.md"))
            );
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
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer("Discard");
    cx.run_until_parked();
    assert!(!cx.has_pending_prompt());
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            assert!(this.notebooks.notice.is_some());
            assert!(this.tabs.iter().all(|tab| tab.observed(cx).dirty));
            // In-memory fixture deletion backend, never OS Trash.
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
        let n = notes.read(cx);
        assert!(n.tabs.iter().all(|tab| tab.path().is_none()));
        assert!(n.documents.is_empty());
    });
}

#[gpui::test]
fn shared_close_prompts_once_per_document_and_cancel_keeps_every_view(cx: &mut TestAppContext) {
    let (fixture, notes, cx) = fixture(cx);
    let other = fixture.root.join("other.md");
    fs::write(&other, "other").unwrap();
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.tabs
                .active
                .editor
                .replace_selection("draft ", window, cx);
            this.new_tab(window, cx);
            this.request(Next::File(fixture.note.clone()), window, cx);
            this.new_tab(window, cx);
            this.install_document(Some(Document::open(other).unwrap()), window, cx);
            this.tabs
                .active
                .editor
                .replace_selection("other draft ", window, cx);
            this.request(Next::Close, window, cx);
        })
    });
    cx.run_until_parked();
    cx.simulate_prompt_answer("Save");
    cx.run_until_parked();
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    cx.update(|_, cx| {
        let n = notes.read(cx);
        assert!(!n.is_busy());
        assert_eq!(n.tabs.ids().len(), 3);
        assert!(!n.tabs.get(n.tabs.ids()[0]).unwrap().observed(cx).dirty);
        assert!(!n.tabs.get(n.tabs.ids()[1]).unwrap().observed(cx).dirty);
        assert!(n.tabs.active.observed(cx).dirty);
    });
}
#[gpui::test]
fn shared_edit_notification_survives_origin_navigation_and_conflict_is_shared(
    cx: &mut TestAppContext,
) {
    let (fixture, notes, cx) = fixture(cx);
    let other = fixture.root.join("other.md");
    fs::write(&other, "other").unwrap();
    let first = cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            let first = this.tabs.active.id;
            this.set_mode(ViewMode::Reading, window, cx);
            this.new_tab(window, cx);
            this.request(Next::File(fixture.note.clone()), window, cx);
            this.tabs
                .active
                .editor
                .replace_selection("queued ", window, cx);
            this.request(Next::File(other), window, cx);
            first
        })
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            assert!(this.tabs.get(first).unwrap().dirty_cached);
            assert_eq!(
                this.tabs.get(first).unwrap().reading_text.as_ref(),
                "queued original A"
            );
            let current = this.tabs.active.id;
            this.open_link("note.md", window, cx);
            assert_eq!(this.tabs.active.id, current);
            assert_eq!(
                this.tabs.active.editor.text(cx).as_ref(),
                "queued original A"
            );
        })
    });
    fs::write(&fixture.note, "external change").unwrap();
    cx.update(|window, cx| notes.update(cx, |this, cx| this.save(Next::Nothing, window, cx)));
    cx.run_until_parked();
    cx.update(|_, cx| {
        let n = notes.read(cx);
        assert!(
            n.tabs
                .iter()
                .all(|tab| tab.session.save_conflicted() && tab.observed(cx).dirty)
        );
        assert!(n.save_state(first).error.is_some());
        assert!(std::ptr::eq(
            n.save_state(first),
            n.save_state(n.tabs.active.id)
        ));
    });
    assert_eq!(
        fs::read_to_string(&fixture.note).unwrap(),
        "external change"
    );
}

#[gpui::test]
fn shared_explorer_keyboard_navigation_keeps_current_tab_and_tree_focus(cx: &mut TestAppContext) {
    let (fixture, notes, cx) = fixture(cx);
    fs::write(fixture.root.join("other.md"), "other").unwrap();
    cx.update(|window, cx| notes.update(cx, |this, cx| this.refresh_tree(window, cx)));
    cx.run_until_parked();
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.new_tab(window, cx);
            let current = this.tabs.active.id;
            let row = this
                .notebooks
                .explorer
                .rows()
                .iter()
                .position(|row| row.path == std::path::Path::new("note.md"))
                .unwrap();
            let down = row > 0;
            this.notebooks
                .explorer
                .select(Some(if down { row - 1 } else { row + 1 }));
            this.notebooks.tree_focus.focus(window);
            this.tree_move(down, window, cx);
            assert_eq!(this.tabs.active.id, current);
            assert_eq!(this.tabs.active.path(), Some(fixture.note.as_path()));
            assert_eq!(this.tabs.ids().len(), 2);
            assert!(this.notebooks.tree_focus.is_focused(window));
        })
    });
}

#[gpui::test]
fn note_and_tab_shortcuts_keep_creation_and_close_lifecycles_distinct(cx: &mut TestAppContext) {
    cx.update(|cx| {
        cx.bind_keys(
            crate::help_ui::app_shortcuts()
                .into_iter()
                .map(|s| s.binding),
        )
    });
    let (fixture, notes, cx) = fixture(cx);
    let modifier = if cfg!(target_os = "macos") {
        "cmd"
    } else {
        "ctrl"
    };
    let first = cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.editor.replace_selection(" draft", window, cx);
            this.editor.focus(window, cx);
            this.tabs.active.id
        })
    });
    cx.refresh().unwrap();
    let files = fs::read_dir(&fixture.root).unwrap().count();
    cx.simulate_keystrokes(&format!("{modifier}-t"));
    cx.run_until_parked();
    cx.update(|_, cx| {
        let this = notes.read(cx);
        assert_eq!(this.tabs.ids().len(), 2);
        assert!(this.document().is_none());
    });
    assert_eq!(fs::read_dir(&fixture.root).unwrap().count(), files);
    assert!(!cx.has_pending_prompt());
    cx.simulate_keystrokes(&format!("{modifier}-w"));
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(notes.read(cx).tabs.active.id, first));
    cx.simulate_keystrokes(&format!("{modifier}-w"));
    cx.run_until_parked();
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    cx.update(|_, cx| assert!(notes.read(cx).dirty(cx)));
    cx.simulate_keystrokes(&format!("{modifier}-n"));
    cx.run_until_parked();
    assert!(cx.has_pending_prompt());
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.new_note(window, cx);
            this.new_tab(window, cx);
            this.close_tab(first, window, cx);
            assert_eq!(this.tabs.ids(), vec![first]);
        })
    });
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    assert_eq!(fs::read_dir(&fixture.root).unwrap().count(), files);
    cx.simulate_keystrokes(&format!("{modifier}-n"));
    cx.run_until_parked();
    cx.simulate_prompt_answer("Discard");
    cx.run_until_parked();
    cx.update(|_, cx| {
        let this = notes.read(cx);
        let path = &this.document().unwrap().path;
        assert_eq!(path.parent(), Some(fixture.root.as_path()));
        assert_ne!(path, &fixture.note);
        assert!(path.is_file());
        assert_eq!(this.tabs.ids(), vec![first]);
        assert!(!this.dirty(cx));
    });
    assert_eq!(fs::read_dir(&fixture.root).unwrap().count(), files + 1);
    assert_eq!(fs::read_to_string(&fixture.note).unwrap(), "original A");
}

#[gpui::test]
fn text_files_use_picker_source_editing_and_restore_markdown_mode(cx: &mut TestAppContext) {
    use mithril::commands::Command;
    let (fixture, notes, cx) = fixture(cx);
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| this.set_mode(ViewMode::Reading, window, cx))
    });
    for name in [
        "file.txt", "file.js", "file.ts", "file.sh", "README", "file.xyz",
    ] {
        let path = fixture.other.join(name);
        fs::write(&path, "original").unwrap();
        cx.update(|window, cx| {
            notes.update(cx, |this, cx| {
                this.request(Next::Open, window, cx);
                let operation = this.file_picker_operation.take().unwrap();
                this.finish_file_picker(operation, Ok(Some(path.clone())), window, cx);
            })
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            notes.update(cx, |this, cx| {
                assert_eq!(this.document().unwrap().path, path);
                assert_eq!(this.tabs.active.mode, ViewMode::Source);
                assert!(this.can_edit_document());
                assert!(!this.can_edit_markdown());
                for command in [
                    Command::LivePreview,
                    Command::ReadingView,
                    Command::Bold,
                    Command::ToggleTasks,
                    Command::EditLink,
                ] {
                    assert!(command.unavailable(this.capabilities()).is_some());
                    this.run_command(command, window, cx);
                }
                this.cycle_view_mode(window, cx);
                this.set_mode(ViewMode::Reading, window, cx);
                assert_eq!(this.tabs.active.mode, ViewMode::Source);
                assert_eq!(
                    this.notebooks.active.as_ref().unwrap().settings.view_mode,
                    ViewMode::Reading
                );
                assert_eq!(this.tabs.active.editor.text(cx).as_ref(), "original");
                this.tabs.active.editor.select(0..8, window, cx);
                this.tabs
                    .active
                    .editor
                    .replace_selection("edited", window, cx);
            })
        });
        cx.run_until_parked();
        cx.dispatch_action(Undo);
        cx.run_until_parked();
        cx.update(|_, cx| {
            assert_eq!(
                notes.read(cx).tabs.active.editor.text(cx).as_ref(),
                "original"
            )
        });
        cx.dispatch_action(Redo);
        cx.run_until_parked();
        cx.update(|window, cx| notes.update(cx, |this, cx| this.save(Next::Nothing, window, cx)));
        cx.run_until_parked();
        assert_eq!(fs::read_to_string(&path).unwrap(), "edited");
    }
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.request(Next::File(fixture.note.clone()), window, cx)
        })
    });
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(notes.read(cx).tabs.active.mode, ViewMode::Reading));
}

#[gpui::test]
fn non_markdown_retained_views_share_unsaved_text(cx: &mut TestAppContext) {
    let (fixture, notes, cx) = fixture(cx);
    let path = fixture.root.join("script.ts");
    fs::write(&path, "const count = 1;").unwrap();
    let first = cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.install_document(Some(Document::open(path.clone()).unwrap()), window, cx);
            this.tabs.active.editor.select(14..15, window, cx);
            this.tabs.active.editor.replace_selection("2", window, cx);
            let first = this.tabs.active.id;
            this.new_tab(window, cx);
            this.request(Next::FileFromPicker(path.clone()), window, cx);
            first
        })
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            assert_eq!(this.tabs.active.mode, ViewMode::Source);
            assert_eq!(
                this.tabs.active.editor.text(cx).as_ref(),
                "const count = 2;"
            );
            assert_eq!(
                this.tabs.get(first).unwrap().editor.text(cx),
                this.tabs.active.editor.text(cx)
            );
            this.tabs.active.editor.focus(window, cx);
        })
    });
    cx.dispatch_action(Undo);
    cx.run_until_parked();
    cx.update(|_, cx| {
        let this = notes.read(cx);
        assert_eq!(
            this.tabs.active.editor.text(cx).as_ref(),
            "const count = 1;"
        );
        assert_eq!(
            this.tabs.get(first).unwrap().editor.text(cx),
            this.tabs.active.editor.text(cx)
        );
    });
}

#[gpui::test]
fn external_file_badges_follow_current_notebook_membership(cx: &mut TestAppContext) {
    let (fixture, notes, cx) = fixture(cx);
    let path = fixture.other.join("outside.txt");
    fs::write(&path, "external").unwrap();
    let (inside, outside) = cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            let inside = this.tabs.active.id;
            this.new_tab(window, cx);
            this.request(Next::Open, window, cx);
            let operation = this.file_picker_operation.take().unwrap();
            this.finish_file_picker(operation, Ok(Some(path.clone())), window, cx);
            (inside, this.tabs.active.id)
        })
    });
    cx.run_until_parked();
    cx.refresh().unwrap();
    let outside_selector: &'static str =
        Box::leak(format!("tab-external-{}", outside.0).into_boxed_str());
    let inside_selector: &'static str =
        Box::leak(format!("tab-external-{}", inside.0).into_boxed_str());
    assert!(cx.debug_bounds(outside_selector).is_some());
    assert!(cx.debug_bounds(inside_selector).is_none());
    let select_selector: &'static str =
        Box::leak(format!("tab-select-{}", outside.0).into_boxed_str());
    let external_width = cx.debug_bounds(select_selector).unwrap().size.width;
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            let root = Some(fixture.root.as_path());
            assert!(
                this.tabs
                    .active
                    .file_tooltip(root)
                    .contains("This file is outside the current notebook.")
            );
            assert!(
                this.tabs
                    .active
                    .file_tooltip(root)
                    .contains(&path.display().to_string())
            );
            assert!(!this.tabs.get(inside).unwrap().is_external_to(root));
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
    cx.refresh().unwrap();
    cx.update(|_, cx| {
        let this = notes.read(cx);
        assert_eq!(this.notebooks.active.as_ref().unwrap().root, fixture.other);
        assert!(
            !this
                .tabs
                .get(outside)
                .unwrap()
                .is_external_to(Some(&fixture.other))
        );
    });
    // GPUI retains old debug selectors; measure the live button to prove badge removal.
    assert!(cx.debug_bounds(select_selector).unwrap().size.width < external_width);
    assert!(cx.debug_bounds(inside_selector).is_some());
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.notebooks.active = None;
            this.select_tab(outside, window, cx);
            assert!(
                this.tabs
                    .active
                    .file_tooltip(None)
                    .contains("No notebook is open.")
            );
            this.new_tab(window, cx);
            assert!(!this.tabs.active.is_external_to(None));
            assert_eq!(this.tabs.active.file_tooltip(None), "New tab");
            cx.notify();
        })
    });
    cx.run_until_parked();
    cx.refresh().unwrap();
    assert!(cx.debug_bounds(outside_selector).is_some());
}

#[gpui::test]
fn external_autosave_opt_in_persists_and_cancels_pending_writes(cx: &mut TestAppContext) {
    let (fixture, notes, cx) = fixture(cx);
    let paths = [
        fixture.other.join("external.md"),
        fixture.other.join("external.ts"),
    ];
    let mut external_tabs = vec![];
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            assert!(!this.external_autosave_enabled());
            this.update_autosave(
                notebook::Autosave {
                    enabled: true,
                    delay_ms: 100,
                },
                window,
                cx,
            );
            this.editor.replace_selection("notebook edit ", window, cx);
        })
    });
    for path in &paths {
        fs::write(path, "original").unwrap();
        let id = cx.update(|window, cx| {
            notes.update(cx, |this, cx| {
                this.new_tab(window, cx);
                this.install_document(Some(Document::open(path.clone()).unwrap()), window, cx);
                this.editor.select(0..8, window, cx);
                this.editor.replace_selection("external edit", window, cx);
                assert!(!this.tab_autosave_settings(this.tabs.active.id).enabled);
                this.tabs.active.id
            })
        });
        external_tabs.push(id);
    }
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();
    assert!(
        fs::read_to_string(&fixture.note)
            .unwrap()
            .contains("notebook edit")
    );
    for path in &paths {
        assert_eq!(fs::read_to_string(path).unwrap(), "original");
    }
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.update_external_autosave(true, window, cx)
        })
    });
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();
    for path in &paths {
        assert_eq!(fs::read_to_string(path).unwrap(), "external edit");
    }
    assert!(
        notebook::Notebook::open(&fixture.root, false)
            .unwrap()
            .settings
            .autosave_external_files
    );
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.editor.replace_selection(" pending", window, cx);
        })
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            assert!(this.save_state(this.tabs.active.id).timer.is_some());
            this.update_external_autosave(false, window, cx);
            for id in &external_tabs {
                assert!(!this.tab_autosave_settings(*id).enabled);
            }
            assert!(
                this.notebooks
                    .active
                    .as_ref()
                    .unwrap()
                    .settings
                    .autosave
                    .enabled
            );
        })
    });
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();
    assert_eq!(fs::read_to_string(&paths[1]).unwrap(), "external edit");
    assert!(
        !notebook::Notebook::open(&fixture.root, false)
            .unwrap()
            .settings
            .autosave_external_files
    );
    cx.update(|window, cx| notes.update(cx, |this, cx| this.save(Next::Nothing, window, cx)));
    cx.run_until_parked();
    assert_eq!(
        fs::read_to_string(&paths[1]).unwrap(),
        "external edit pending"
    );
}

#[gpui::test]
fn notebook_switch_cancels_autosave_for_newly_external_tabs(cx: &mut TestAppContext) {
    let (fixture, notes, cx) = fixture(cx);
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.update_autosave(
                notebook::Autosave {
                    enabled: true,
                    delay_ms: 1_000,
                },
                window,
                cx,
            );
            this.editor.replace_selection("pending ", window, cx);
        })
    });
    cx.run_until_parked();
    let first = cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            let first = this.tabs.active.id;
            assert!(this.save_state(first).timer.is_some());
            this.request(
                Next::Notebook {
                    path: fixture.other.clone(),
                    create: false,
                },
                window,
                cx,
            );
            first
        })
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        let this = notes.read(cx);
        assert!(
            this.tabs
                .get(first)
                .unwrap()
                .is_external_to(Some(&fixture.other))
        );
        assert!(!this.tab_autosave_settings(first).enabled);
    });
    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();
    assert_eq!(fs::read_to_string(&fixture.note).unwrap(), "original A");
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.update_external_autosave(true, window, cx)
        })
    });
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();
    assert_eq!(
        fs::read_to_string(&fixture.note).unwrap(),
        "pending original A"
    );
}

#[gpui::test]
fn external_autosave_notebook_preferences_are_independent_across_switches(cx: &mut TestAppContext) {
    let (fixture, notes, cx) = fixture(cx);
    let external = fixture.root.parent().unwrap().join("external.txt");
    fs::write(&external, "original").unwrap();
    let id = cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.install_document(Some(Document::open(external.clone()).unwrap()), window, cx);
            this.update_external_autosave(true, window, cx);
            // Editing the external delay must not copy this notebook's enable flag into session defaults.
            this.update_autosave(
                notebook::Autosave {
                    enabled: true,
                    delay_ms: 100,
                },
                window,
                cx,
            );
            assert!(!this.session_autosave.enabled);
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
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            assert!(!this.external_autosave_enabled());
            this.select_tab(id, window, cx);
        })
    });
    cx.run_until_parked();
    cx.refresh().unwrap();
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.editor.select(0..8, window, cx);
            this.editor.replace_selection("edited under B", window, cx);
            assert_eq!(this.editor.text(cx).as_ref(), "edited under B");
        })
    });
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();
    assert_eq!(fs::read_to_string(&external).unwrap(), "original");
    assert!(
        notebook::Notebook::open(&fixture.root, false)
            .unwrap()
            .settings
            .autosave_external_files
    );
    assert!(
        !notebook::Notebook::open(&fixture.other, false)
            .unwrap()
            .settings
            .autosave_external_files
    );
    assert!(
        !fs::read_to_string(&fixture.index)
            .unwrap()
            .contains("autosave_external_files")
    );
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
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
        let this = notes.read(cx);
        assert!(this.external_autosave_enabled());
        assert!(this.tab_autosave_settings(id).enabled);
        assert_eq!(
            this.tabs.get(id).unwrap().editor.text(cx).as_ref(),
            "edited under B"
        );
        assert!(
            this.save_state(id).timer.is_some(),
            "timer missing: error={:?}, dirty={}, blocked={}, ticket={:?}",
            this.save_state(id).error,
            this.tabs.get(id).unwrap().observed(cx).dirty,
            this.document_write_blocked(id),
            this.tabs.get(id).unwrap().session.autosave()
        );
    });
    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();
    assert_eq!(fs::read_to_string(&external).unwrap(), "edited under B");
}
