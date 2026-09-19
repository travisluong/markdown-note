//! Settings persistence transitions on TestPlatform; fixtures stay in temporary notebooks.
use crate::{Document, Next, Notes, ViewMode, notebook};
use gpui::{Entity, TestAppContext, VisualTestContext, prelude::*};
use gpui_component::Root;
use std::{fs, path::PathBuf, time::Duration};

pub(super) struct Fixture {
    pub(super) root: PathBuf,
    pub(super) other: PathBuf,
    pub(super) note: PathBuf,
    pub(super) index: PathBuf,
    _directory: tempfile::TempDir,
}

pub(super) fn fixture(cx: &mut TestAppContext) -> (Fixture, Entity<Notes>, &mut VisualTestContext) {
    let directory = tempfile::tempdir().unwrap();
    let mut first = notebook::Notebook::open(&directory.path().join("A"), true).unwrap();
    let mut second = notebook::Notebook::open(&directory.path().join("B"), true).unwrap();
    for (book, text) in [(&mut first, "original A"), (&mut second, "original B")] {
        fs::write(book.root.join("note.md"), text).unwrap();
        book.settings.last_file = Some("note.md".into());
        book.settings.autosave.enabled = false;
        book.settings.view_mode = ViewMode::Source;
        notebook::save_book(book).unwrap();
    }
    let fixture = Fixture {
        note: first.root.join("note.md"),
        root: first.root.clone(),
        other: second.root.clone(),
        index: directory.path().join("index.json"),
        _directory: directory,
    };
    let mut index = notebook::Index::default();
    index.register(first.root.clone());
    notebook::save_index(&fixture.index, &index).unwrap();
    cx.update(gpui_component::init);
    let mut notes = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| {
            let mut notes = Notes::new(window, cx);
            notes.tabs.set_initial_notebook(first.root.clone());
            notes.notebooks.active = Some(first);
            notes.notebooks.index = index;
            notes.notebooks.index_path = Some(fixture.index.clone());
            notes.notebooks.index_writable = true;
            notes.session_autosave.enabled = false;
            notes.mode = ViewMode::Source;
            notes
        });
        entity.update(cx, |this, cx| {
            this.install_document(
                Some(Document::open(fixture.note.clone()).unwrap()),
                window,
                cx,
            );
        });
        notes = Some(entity.clone());
        let view = cx.new(|cx| crate::NotesWindow {
            _subscription: cx.observe(&entity, |_, _, cx| cx.notify()),
            notes: entity,
        });
        Root::new(view, window, cx)
    });
    cx.refresh().unwrap();
    cx.run_until_parked();
    (fixture, notes.unwrap(), cx)
}

pub(super) fn fail_index_write(
    fixture: &Fixture,
    notes: &Entity<Notes>,
    cx: &mut VisualTestContext,
) {
    fs::write(&fixture.index, "corrupt index").unwrap();
    cx.update(|_, cx| {
        notes.update(cx, |this, cx| {
            this.notebooks.active.as_mut().unwrap().settings.date_format = "MMMM Do, YYYY".into();
            this.persist_notebooks(cx);
        });
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        let writer = notes.read(cx).notebooks.writer.as_ref().unwrap();
        assert!(!writer.is_running());
        assert!(matches!(
            writer.flush_status(&writer.flush_all()),
            notebook::FlushStatus::Failed(_)
        ));
    });
}

#[gpui::test]
fn activation_waits_for_the_target_book_latest_settings(cx: &mut TestAppContext) {
    let (fixture, notes, cx) = fixture(cx);
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.request(
                Next::Notebook {
                    path: fixture.other.clone(),
                    create: false,
                },
                window,
                cx,
            );
        });
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            assert_eq!(this.notebooks.active.as_ref().unwrap().root, fixture.other);
            this.notebooks.write_delay = Duration::from_secs(10);
            this.notebooks.active.as_mut().unwrap().settings.date_format = "MMMM Do, YYYY".into();
            this.set_mode(ViewMode::Reading, window, cx);
            this.request(
                Next::Notebook {
                    path: fixture.root.clone(),
                    create: false,
                },
                window,
                cx,
            );
        });
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            assert_eq!(this.notebooks.active.as_ref().unwrap().root, fixture.root);
            assert!(!this.is_busy(), "B persistence must not block loading A");
            this.request(
                Next::Notebook {
                    path: fixture.other.clone(),
                    create: false,
                },
                window,
                cx,
            );
        });
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        let notes = notes.read(cx);
        assert!(notes.is_busy());
        assert_eq!(notes.notebooks.active.as_ref().unwrap().root, fixture.root);
        assert_eq!(notes.editor.text(cx).as_ref(), "original A");
    });
    cx.executor().advance_clock(Duration::from_secs(10));
    cx.run_until_parked();
    cx.update(|_, cx| {
        let notes = notes.read(cx);
        assert!(!notes.is_busy());
        let active = notes.notebooks.active.as_ref().unwrap();
        assert_eq!(active.root, fixture.other);
        assert_eq!(active.settings.date_format, "MMMM Do, YYYY");
        assert_eq!(notes.mode, ViewMode::Reading);
        assert_eq!(notes.editor.text(cx).as_ref(), "original B");
    });
    assert_eq!(
        notebook::load_index(&fixture.index).unwrap().active,
        Some(fixture.other)
    );
}

#[gpui::test]
fn an_unrelated_failed_book_does_not_block_activation_or_revert_the_index(cx: &mut TestAppContext) {
    let (fixture, notes, cx) = fixture(cx);
    let damaged = notebook::settings_path(&fixture.root);
    fs::write(&damaged, "corrupt A").unwrap();
    cx.update(|_, cx| notes.update(cx, |this, cx| this.persist_notebooks(cx)));
    cx.run_until_parked();
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.request(
                Next::Notebook {
                    path: fixture.other.clone(),
                    create: false,
                },
                window,
                cx,
            );
        });
    });
    cx.run_until_parked();
    assert!(!cx.has_pending_prompt());
    cx.update(|_, cx| {
        let notes = notes.read(cx);
        assert!(!notes.is_busy());
        assert_eq!(notes.notebooks.active.as_ref().unwrap().root, fixture.other);
        let writer = notes.notebooks.writer.as_ref().unwrap();
        let notebook::FlushStatus::Failed(errors) = writer.flush_status(&writer.flush_all()) else {
            panic!("failed A must remain available for close/retry reporting");
        };
        assert_eq!(errors.len(), 1);
        assert_eq!(errors[0].path, damaged);
    });
    assert_eq!(fs::read_to_string(damaged).unwrap(), "corrupt A");
    assert_eq!(
        notebook::load_index(&fixture.index).unwrap().active,
        Some(fixture.other)
    );
}

#[gpui::test]
fn palette_colors_persist_through_the_settings_writer(cx: &mut TestAppContext) {
    let (fixture, notes, cx) = fixture(cx);
    for (mode, border) in [
        (notebook::ThemeChoice::Light, 0x123456),
        (notebook::ThemeChoice::Dark, 0xabcdef),
    ] {
        cx.update(|window, cx| {
            notes.update(cx, |this, cx| {
                let mut appearance = this.appearance();
                appearance.theme = Some(mode);
                let mut palette = appearance.palette(this.startup_dark);
                palette.border = border;
                palette.icon = border;
                palette.secondary = border;
                appearance.set_palette(this.startup_dark, palette);
                this.update_appearance(appearance, window, cx);
            });
        });
        cx.run_until_parked();
    }
    cx.update(|_, cx| {
        let writer = notes.read(cx).notebooks.writer.as_ref().unwrap();
        assert_eq!(
            writer.flush_status(&writer.flush_all()),
            notebook::FlushStatus::Succeeded
        );
    });
    let loaded = notebook::Notebook::open(&fixture.root, false).unwrap();
    assert_eq!(loaded.settings.appearance.light.unwrap().border, 0x123456);
    assert_eq!(loaded.settings.appearance.dark.unwrap().border, 0xabcdef);
    assert_eq!(loaded.settings.appearance.light.unwrap().icon, 0x123456);
    assert_eq!(loaded.settings.appearance.dark.unwrap().icon, 0xabcdef);
    assert_eq!(
        loaded.settings.appearance.light.unwrap().secondary,
        0x123456
    );
    assert_eq!(loaded.settings.appearance.dark.unwrap().secondary, 0xabcdef);
}

#[gpui::test]
fn activation_loads_exclusions_without_blocking_last_file_restoration(cx: &mut TestAppContext) {
    let (fixture, notes, cx) = fixture(cx);
    let mut other = notebook::Notebook::open(&fixture.other, false).unwrap();
    other.settings.excluded_patterns = vec![r"^note\.md$".into()];
    notebook::save_book(&other).unwrap();
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.request(
                Next::Notebook {
                    path: fixture.other.clone(),
                    create: false,
                },
                window,
                cx,
            )
        })
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        let this = notes.read(cx);
        assert!(
            this.notebooks
                .exclusions
                .excludes(std::path::Path::new("note.md"))
        );
        assert!(
            this.notebooks
                .explorer
                .rows()
                .iter()
                .all(|row| row.path != std::path::Path::new("note.md"))
        );
        assert_eq!(this.document().unwrap().path, fixture.other.join("note.md"));
    });
    // Return to a notebook with invalid external rules; all custom exclusions are disabled.
    let mut first = notebook::Notebook::open(&fixture.root, false).unwrap();
    first.settings.excluded_patterns = vec!["^note".into(), "[broken".into()];
    notebook::save_book(&first).unwrap();
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.request(
                Next::Notebook {
                    path: fixture.root.clone(),
                    create: false,
                },
                window,
                cx,
            )
        })
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        let this = notes.read(cx);
        assert!(
            !this
                .notebooks
                .exclusions
                .excludes(std::path::Path::new("note.md"))
        );
        assert!(
            this.notebooks
                .notice
                .as_ref()
                .unwrap()
                .contains("Custom exclusions are disabled")
        );
        assert_eq!(this.document().unwrap().path, fixture.note);
        assert_eq!(
            this.notebooks
                .active
                .as_ref()
                .unwrap()
                .settings
                .excluded_patterns,
            first.settings.excluded_patterns
        );
    });
}

#[gpui::test]
fn exclusions_persist_through_the_writer_and_stay_scoped_to_the_notebook(cx: &mut TestAppContext) {
    let (fixture, notes, cx) = fixture(cx);
    let patterns = notebook::parse_excluded_patterns(r"^archive$, a{1,3}, report\,old").unwrap();
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.update_excluded_patterns(patterns.clone(), window, cx)
        })
    });
    cx.run_until_parked();
    assert_eq!(
        notebook::Notebook::open(&fixture.root, false)
            .unwrap()
            .settings
            .excluded_patterns,
        patterns
    );
    assert!(
        notebook::Notebook::open(&fixture.other, false)
            .unwrap()
            .settings
            .excluded_patterns
            .is_empty()
    );
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.update_excluded_patterns(vec!["[broken".into()], window, cx)
        })
    });
    cx.run_until_parked();
    assert_eq!(
        notebook::Notebook::open(&fixture.root, false)
            .unwrap()
            .settings
            .excluded_patterns,
        patterns
    );
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.update_excluded_patterns(Vec::new(), window, cx)
        })
    });
    cx.run_until_parked();
    assert!(
        notebook::Notebook::open(&fixture.root, false)
            .unwrap()
            .settings
            .excluded_patterns
            .is_empty()
    );
}

/// Restore a fresh application owner from the actual files written by the previous session.
fn assert_startup_tab(fixture: &Fixture, expected: Option<&str>, cx: &mut VisualTestContext) {
    cx.run_until_parked();
    let book = notebook::Notebook::open(&fixture.root, false).unwrap();
    assert_eq!(book.settings.last_file, expected.map(PathBuf::from));
    let restored = cx.update(|window, cx| {
        let notes = cx.new(|cx| {
            let mut notes = Notes::new(window, cx);
            notes.notebooks.index_path = Some(fixture.index.clone());
            notes
        });
        notes.update(cx, |this, cx| this.restore_notebooks(window, cx));
        notes
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        let notes = restored.read(cx);
        assert!(!notes.is_busy());
        assert_eq!(notes.notebooks.active.as_ref().unwrap().root, fixture.root);
        assert_eq!(notes.tabs.visible_ids().len(), 1);
        assert_eq!(
            notes.document().map(|document| document.path.clone()),
            expected.map(|relative| fixture.root.join(relative))
        );
        if expected.is_none() {
            assert!(matches!(
                notes.tabs.active.session.content(),
                crate::app::session::Content::Empty
            ));
        }
    });
}

#[gpui::test]
fn startup_remembers_created_and_selected_empty_tabs(cx: &mut TestAppContext) {
    let (fixture, notes, cx) = fixture(cx);
    let (document, empty) = cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            let document = this.tabs.active.id;
            this.new_tab(window, cx);
            (document, this.tabs.active.id)
        })
    });
    assert_startup_tab(&fixture, None, cx);

    cx.update(|window, cx| notes.update(cx, |this, cx| this.select_tab(document, window, cx)));
    assert_startup_tab(&fixture, Some("note.md"), cx);

    cx.update(|window, cx| notes.update(cx, |this, cx| this.select_tab(empty, window, cx)));
    assert_startup_tab(&fixture, None, cx);
}

#[gpui::test]
fn startup_remembers_empty_tab_after_closing_active_or_final_tab(cx: &mut TestAppContext) {
    let (fixture, notes, cx) = fixture(cx);
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            let document = this.tabs.active.id;
            this.new_tab(window, cx);
            this.select_tab(document, window, cx);
            this.close_tab(document, window, cx);
        })
    });
    assert_startup_tab(&fixture, None, cx);

    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.request(Next::File(fixture.note.clone()), window, cx);
        })
    });
    cx.run_until_parked();
    assert_startup_tab(&fixture, Some("note.md"), cx);
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            assert_eq!(this.tabs.ids().len(), 1);
            this.close_tab(this.tabs.active.id, window, cx);
        })
    });
    assert_startup_tab(&fixture, None, cx);
}

#[gpui::test]
fn startup_keeps_empty_tab_after_closing_background_document(cx: &mut TestAppContext) {
    let (fixture, notes, cx) = fixture(cx);
    let empty = cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            let document = this.tabs.active.id;
            this.new_tab(window, cx);
            let empty = this.tabs.active.id;
            this.close_tab(document, window, cx);
            empty
        })
    });
    assert_startup_tab(&fixture, None, cx);
    cx.update(|_, cx| assert_eq!(notes.read(cx).tabs.active.id, empty));
}
