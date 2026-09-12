//! Preference choices during activation. The deterministic delay occurs after
//! real filesystem reads, before their captured result installs.

use super::persistence_tests::fixture;
use crate::{Next, ViewMode, notebook};
use gpui::TestAppContext;
use std::{fs, time::Duration};

#[gpui::test]
fn same_book_reload_preserves_choices_made_after_settings_were_read(cx: &mut TestAppContext) {
    let (fixture, notes, cx) = fixture(cx);
    let mut external = notebook::Notebook::open(&fixture.root, false).unwrap();
    external.settings.date_format = "MMMM Do, YYYY".into();
    external.settings.file_sort = notebook::FileSort::ModifiedNewest;
    notebook::save_book(&external).unwrap();
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.notebooks.activation_install_delay = Duration::from_secs(2);
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
            assert!(this.is_busy());
            this.set_mode(ViewMode::Reading, window, cx);
            this.save_sidebar_width(375., cx);
            this.update_autosave(
                notebook::Autosave {
                    enabled: false,
                    delay_ms: 500,
                },
                window,
                cx,
            );
        });
    });
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();
    cx.update(|_, cx| {
        let notes = notes.read(cx);
        assert!(!notes.is_busy());
        assert_eq!(notes.mode, ViewMode::Reading);
        let settings = &notes.notebooks.active.as_ref().unwrap().settings;
        assert_eq!(settings.sidebar_width, 375.);
        assert_eq!(
            settings.autosave,
            notebook::Autosave {
                enabled: false,
                delay_ms: 500
            }
        );
        assert_eq!(settings.date_format, "MMMM Do, YYYY");
        assert_eq!(settings.file_sort, notebook::FileSort::ModifiedNewest);
    });
    let saved = notebook::Notebook::open(&fixture.root, false)
        .unwrap()
        .settings;
    assert_eq!(saved.view_mode, ViewMode::Reading);
    assert_eq!(saved.sidebar_width, 375.);
    assert_eq!(saved.autosave.delay_ms, 500);
    assert_eq!(saved.date_format, "MMMM Do, YYYY");
}

#[gpui::test]
fn reselecting_an_unchanged_value_is_an_intent_against_external_reload(cx: &mut TestAppContext) {
    let (fixture, notes, cx) = fixture(cx);
    let mut external = notebook::Notebook::open(&fixture.root, false).unwrap();
    external.settings.view_mode = ViewMode::Reading;
    external.settings.sidebar_width = 420.;
    notebook::save_book(&external).unwrap();
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.notebooks.activation_install_delay = Duration::from_secs(2);
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
            assert_eq!(this.mode, ViewMode::Source);
            this.set_mode(ViewMode::Source, window, cx);
            this.save_sidebar_width(280., cx);
        });
    });
    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();
    cx.update(|_, cx| {
        let notes = notes.read(cx);
        assert_eq!(notes.mode, ViewMode::Source);
        assert_eq!(
            notes
                .notebooks
                .active
                .as_ref()
                .unwrap()
                .settings
                .sidebar_width,
            280.
        );
    });
}

#[gpui::test]
fn preferences_changed_on_a_do_not_replace_b_or_its_external_file(cx: &mut TestAppContext) {
    let (fixture, notes, cx) = fixture(cx);
    fs::write(fixture.other.join("note.md"), "external B").unwrap();
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.notebooks.activation_install_delay = Duration::from_secs(2);
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
            this.set_mode(ViewMode::Reading, window, cx);
            this.save_sidebar_width(375., cx);
        });
    });
    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();
    cx.update(|_, cx| {
        let notes = notes.read(cx);
        assert_eq!(notes.notebooks.active.as_ref().unwrap().root, fixture.other);
        assert_eq!(notes.mode, ViewMode::Source);
        assert_eq!(
            notes
                .notebooks
                .active
                .as_ref()
                .unwrap()
                .settings
                .sidebar_width,
            280.
        );
        assert_eq!(notes.editor.text(cx).as_ref(), "external B");
    });
    let first = notebook::Notebook::open(&fixture.root, false)
        .unwrap()
        .settings;
    assert_eq!(first.view_mode, ViewMode::Reading);
    assert_eq!(first.sidebar_width, 375.);
}

#[gpui::test]
fn standalone_document_autosave_choices_remain_session_scoped_during_reload(
    cx: &mut TestAppContext,
) {
    let (fixture, notes, cx) = fixture(cx);
    let standalone = fixture.root.parent().unwrap().join("standalone.md");
    fs::write(&standalone, "outside the notebook").unwrap();
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.request(Next::File(standalone), window, cx);
        });
    });
    cx.run_until_parked();
    let original = notebook::Notebook::open(&fixture.root, false)
        .unwrap()
        .settings
        .autosave;
    let session = notebook::Autosave {
        enabled: false,
        delay_ms: 500,
    };
    assert_ne!(original, session);
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.notebooks.activation_install_delay = Duration::from_secs(2);
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
            assert!(this.is_busy());
            this.update_autosave(session, window, cx);
        });
    });
    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();
    cx.update(|_, cx| {
        let notes = notes.read(cx);
        assert!(!notes.is_busy());
        assert_eq!(notes.saves.session, session);
        assert_eq!(
            notes.notebooks.active.as_ref().unwrap().settings.autosave,
            original
        );
        assert_eq!(notes.editor.text(cx).as_ref(), "original A");
    });
    assert_eq!(
        notebook::Notebook::open(&fixture.root, false)
            .unwrap()
            .settings
            .autosave,
        original
    );
}
