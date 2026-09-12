//! Form and application event boundaries on TestPlatform, including real dialog rendering.
use super::{ColorRole, FontSizeRole, SettingsIntent, SettingsPanel};
use crate::{Notes, NotesWindow, notebook};
use gpui::{AppContext, Entity, Subscription, TestAppContext, VisualTestContext};
use gpui_component::{Root, WindowExt};
use std::{fs, path::PathBuf, time::Duration};

struct Fixture {
    _directory: tempfile::TempDir,
    first: notebook::Notebook,
    second: notebook::Notebook,
    standalone: PathBuf,
}

fn fixture(
    cx: &mut TestAppContext,
    active: bool,
    standalone: bool,
) -> (Fixture, Entity<Notes>, &mut VisualTestContext) {
    let directory = tempfile::tempdir().unwrap();
    let first = notebook::Notebook::open(&directory.path().join("A"), true).unwrap();
    let second = notebook::Notebook::open(&directory.path().join("B"), true).unwrap();
    fs::write(first.root.join("note.md"), "original").unwrap();
    let outside = directory.path().join("outside.md");
    fs::write(&outside, "original").unwrap();
    let path = if standalone {
        outside.clone()
    } else {
        first.root.join("note.md")
    };
    cx.update(gpui_component::init);
    let mut notes = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| {
            let mut notes = Notes::new(window, cx);
            if active {
                notes.notebooks.active = Some(first.clone());
            }
            notes
        });
        entity.update(cx, |this, cx| this.open_path(path, false, window, cx));
        notes = Some(entity.clone());
        let view = cx.new(|cx| NotesWindow {
            _subscription: cx.observe(&entity, |_, _, cx| cx.notify()),
            notes: entity,
        });
        Root::new(view, window, cx)
    });
    cx.run_until_parked();
    let notes = notes.unwrap();
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.update_autosave(
                notebook::Autosave {
                    enabled: false,
                    delay_ms: 1_000,
                },
                window,
                cx,
            );
            this.open_settings(window, cx);
        })
    });
    cx.refresh().unwrap();
    cx.run_until_parked();
    (
        Fixture {
            _directory: directory,
            first,
            second,
            standalone: outside,
        },
        notes,
        cx,
    )
}

fn panel(notes: &Entity<Notes>, cx: &mut VisualTestContext) -> Entity<SettingsPanel> {
    cx.update(|_, cx| {
        notes
            .read(cx)
            .settings_panel
            .as_ref()
            .unwrap()
            .panel
            .upgrade()
            .unwrap()
    })
}

#[derive(Clone, Copy)]
enum Field {
    Delay,
    Date,
    Extensions,
}
fn input(panel: &Entity<SettingsPanel>, field: Field, value: &str, cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        let panel = panel.read(cx);
        let input = match field {
            Field::Delay => &panel.delay,
            Field::Date => &panel.date,
            Field::Extensions => &panel.hidden_extensions,
        }
        .clone();
        input.update(cx, |input, cx| {
            input.set_value(value.to_owned(), window, cx)
        });
    });
    cx.run_until_parked();
}
fn intent(panel: &Entity<SettingsPanel>, intent: SettingsIntent, cx: &mut VisualTestContext) {
    cx.update(|_, cx| panel.update(cx, |panel, cx| panel.emit_intent(intent, cx)));
    cx.run_until_parked();
}

struct Events(Vec<SettingsIntent>);
fn watch(
    panel: &Entity<SettingsPanel>,
    cx: &mut VisualTestContext,
) -> (Entity<Events>, Subscription) {
    cx.update(|_, cx| {
        let events = cx.new(|_| Events(Vec::new()));
        let subscription = events.update(cx, |_, cx| {
            cx.subscribe(panel, |events, _, intent, _| events.0.push(intent.clone()))
        });
        (events, subscription)
    })
}

#[gpui::test]
fn invalid_drafts_emit_no_preference_intent_and_survive_other_changes(cx: &mut TestAppContext) {
    let (_fixture, notes, cx) = fixture(cx, false, true);
    let panel = panel(&notes, cx);
    intent(&panel, SettingsIntent::ShowExtensions(true), cx);
    let before = cx.update(|_, cx| notes.read(cx).settings_values());
    let (events, _subscription) = watch(&panel, cx);
    input(&panel, Field::Delay, "99", cx);
    input(&panel, Field::Date, "YYYY[", cx);
    input(&panel, Field::Extensions, "tar.gz", cx);
    cx.update(|_, cx| {
        assert_eq!(notes.read(cx).autosave_settings(), before.autosave);
        assert_eq!(notes.read(cx).date_format(), before.date_format);
        assert_eq!(notes.read(cx).hidden_extensions(), before.hidden_extensions);
        assert!(events.read(cx).0.is_empty());
        let panel = panel.read(cx);
        assert!(
            panel.delay_error.is_some()
                && panel.date_error.is_some()
                && panel.hidden_extensions_error.is_some()
        );
    });
    intent(
        &panel,
        SettingsIntent::Theme(notebook::ThemeChoice::Dark),
        cx,
    );
    cx.refresh().unwrap();
    cx.update(|_, cx| {
        let panel = panel.read(cx);
        assert_eq!(panel.delay.read(cx).value().as_ref(), "99");
        assert_eq!(panel.date.read(cx).value().as_ref(), "YYYY[");
        assert_eq!(panel.hidden_extensions.read(cx).value().as_ref(), "tar.gz");
        assert!(
            panel.delay_error.is_some()
                && panel.date_error.is_some()
                && panel.hidden_extensions_error.is_some()
        );
    });
}

#[gpui::test]
fn valid_delay_input_reschedules_the_actual_document_autosave(cx: &mut TestAppContext) {
    let (fixture, notes, cx) = fixture(cx, false, true);
    let panel = panel(&notes, cx);
    intent(&panel, SettingsIntent::AutosaveEnabled(true), cx);
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.editor
                .update(cx, |input, cx| input.set_value("edited", window, cx));
        })
    });
    cx.run_until_parked();
    input(&panel, Field::Delay, "100", cx);
    cx.executor().advance_clock(Duration::from_millis(150));
    cx.run_until_parked();
    assert_eq!(fs::read_to_string(&fixture.standalone).unwrap(), "edited");
    cx.update(|_, cx| {
        assert_eq!(notes.read(cx).autosave_settings().delay_ms, 100);
        assert!(panel.read(cx).delay_error.is_none());
    });
}

#[gpui::test]
fn rapid_independent_intents_merge_against_current_preferences(cx: &mut TestAppContext) {
    let (_fixture, notes, cx) = fixture(cx, false, true);
    let panel = panel(&notes, cx);
    let original = cx.update(|_, cx| notes.read(cx).appearance());
    cx.update(|_, cx| {
        panel.update(cx, |panel, cx| {
            panel.emit_intent(SettingsIntent::FontSizeStep(FontSizeRole::Content, 1), cx);
            panel.emit_intent(SettingsIntent::FontSizeStep(FontSizeRole::Interface, 1), cx);
        })
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        let appearance = notes.read(cx).appearance();
        assert_eq!(appearance.content_font_size, original.content_font_size + 1);
        assert_eq!(appearance.ui_font_size, original.ui_font_size + 1);
    });
}

#[gpui::test]
fn theme_acknowledgement_and_reset_do_not_emit_feedback_changes(cx: &mut TestAppContext) {
    let (_fixture, notes, cx) = fixture(cx, false, true);
    let panel = panel(&notes, cx);
    let (events, _subscription) = watch(&panel, cx);
    intent(
        &panel,
        SettingsIntent::Theme(notebook::ThemeChoice::Dark),
        cx,
    );
    intent(
        &panel,
        SettingsIntent::Color(ColorRole::Primary, 0x123456),
        cx,
    );
    intent(&panel, SettingsIntent::ResetPalettes, cx);
    cx.refresh().unwrap();
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(events.read(cx).0.len(), 3);
        let this = notes.read(cx);
        assert_eq!(
            this.appearance().palette(this.startup_dark),
            notebook::ThemePalette::DARK
        );
        let panel = panel.read(cx);
        assert_eq!(
            panel.colors[2].read(cx).value(),
            Some(gpui::rgb(notebook::ThemePalette::DARK.primary).into())
        );
    });
}

#[gpui::test]
fn close_and_reopen_disconnects_old_form_events_and_resets_drafts(cx: &mut TestAppContext) {
    let (_fixture, notes, cx) = fixture(cx, false, true);
    let old = panel(&notes, cx);
    input(&old, Field::Delay, "99", cx);
    intent(&old, SettingsIntent::Close, cx);
    cx.update(|window, cx| {
        assert!(!window.has_active_dialog(cx));
        assert!(notes.read(cx).settings_panel.is_none());
        notes.update(cx, |this, cx| this.open_settings(window, cx));
    });
    let new = panel(&notes, cx);
    input(&old, Field::Delay, "555", cx);
    cx.update(|_, cx| {
        assert_ne!(new.entity_id(), old.entity_id());
        assert_eq!(notes.read(cx).autosave_settings().delay_ms, 1_000);
        assert!(new.read(cx).delay_error.is_none());
        assert_eq!(new.read(cx).delay.read(cx).value().as_ref(), "1000");
    });
}

#[gpui::test]
fn stale_notebook_form_intent_cannot_write_to_the_new_notebook(cx: &mut TestAppContext) {
    let (fixture, notes, cx) = fixture(cx, true, false);
    let panel = panel(&notes, cx);
    let old_id = panel.entity_id();
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.notebooks.active = Some(fixture.second.clone());
            this.handle_settings_intent(
                old_id,
                SettingsIntent::DateFormat("MMMM Do, YYYY".into()),
                window,
                cx,
            );
            assert_eq!(
                this.notebooks.active.as_ref().unwrap().settings.date_format,
                fixture.second.settings.date_format
            );
            assert!(this.settings_panel.is_none());
        })
    });
}

#[gpui::test]
fn standalone_autosave_and_notebook_appearance_keep_separate_targets(cx: &mut TestAppContext) {
    let (fixture, notes, cx) = fixture(cx, true, true);
    let panel = panel(&notes, cx);
    input(&panel, Field::Delay, "500", cx);
    intent(
        &panel,
        SettingsIntent::Theme(notebook::ThemeChoice::Dark),
        cx,
    );
    cx.update(|_, cx| {
        let notes = notes.read(cx);
        assert_eq!(notes.autosave_settings().delay_ms, 500);
        let book = notes.notebooks.active.as_ref().unwrap();
        assert_eq!(book.settings.autosave, fixture.first.settings.autosave);
        assert_eq!(
            book.settings.appearance.theme,
            Some(notebook::ThemeChoice::Dark)
        );
        assert_eq!(notes.session_appearance.theme, None);
    });
}

#[gpui::test]
fn programmatic_close_expires_form_authority_before_a_later_dialog(cx: &mut TestAppContext) {
    let (_fixture, notes, cx) = fixture(cx, false, true);
    let old = panel(&notes, cx);
    cx.update(|window, cx| window.close_dialog(cx));
    input(&old, Field::Delay, "555", cx);
    cx.update(|window, cx| {
        assert_eq!(notes.read(cx).autosave_settings().delay_ms, 1_000);
        assert!(notes.read(cx).settings_panel.is_none());
        notes.update(cx, |this, cx| this.open_settings(window, cx));
    });
    let old = panel(&notes, cx);
    cx.update(|window, cx| {
        window.close_dialog(cx);
        window.open_dialog(cx, |dialog, _, _| dialog.title("Unrelated dialog"));
    });
    intent(&old, SettingsIntent::Close, cx);
    cx.update(|window, cx| {
        assert!(
            window.has_active_dialog(cx),
            "stale Close must not pop the unrelated dialog"
        );
        assert!(notes.read(cx).settings_panel.is_none());
        assert_eq!(notes.read(cx).autosave_settings().delay_ms, 1_000);
    });
}
