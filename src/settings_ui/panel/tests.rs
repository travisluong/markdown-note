//! Form and application event boundaries on TestPlatform, including modal rendering.
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
    EditorWidth,
    Delay,
    Date,
    Extensions,
    Exclusions,
}
fn input(panel: &Entity<SettingsPanel>, field: Field, value: &str, cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        let panel = panel.read(cx);
        let input = match field {
            Field::Delay => &panel.delay,
            Field::EditorWidth => &panel.editor_width,
            Field::Date => &panel.date,
            Field::Extensions => &panel.hidden_extensions,
            Field::Exclusions => &panel.excluded_patterns,
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

#[gpui::test]
fn date_input_paints_typed_text(cx: &mut TestAppContext) {
    let (_fixture, notes, cx) = fixture(cx, false, true);
    let panel = panel(&notes, cx);
    let date = cx.update(|window, cx| {
        panel.update(cx, |panel, cx| {
            panel.select_section(super::Section::Templates, cx);
            panel.date.update(cx, |date, cx| date.focus(window, cx));
            panel.date.clone()
        })
    });
    let assert_painted = |expected: &str, cx: &mut VisualTestContext| {
        cx.run_until_parked();
        cx.refresh().unwrap();
        cx.update(|_, cx| {
            let date = date.read(cx);
            let (bounds, painted, scroll) = date.diagnostic_input_paint();
            assert_eq!(date.value().as_ref(), expected);
            assert_eq!(
                painted,
                if expected.is_empty() {
                    "YYYY-MM-DD"
                } else {
                    expected
                }
            );
            assert!(bounds.size.width > gpui::px(200.), "{bounds:?}, {scroll:?}");
            assert!(
                bounds.size.height >= gpui::px(16.),
                "{bounds:?}, {scroll:?}"
            );
            assert_eq!(scroll.y, gpui::px(0.));
        });
    };
    assert_painted("YYYY-MM-DD", cx);
    for theme in [notebook::ThemeChoice::Light, notebook::ThemeChoice::Dark] {
        intent(&panel, SettingsIntent::Theme(theme), cx);
        for width in [800., 480.] {
            cx.simulate_resize(gpui::size(gpui::px(width), gpui::px(800.)));
            cx.refresh().unwrap();
            cx.simulate_keystrokes("ctrl-a");
            cx.simulate_input("MM/DD/YYYY");
            assert_painted("MM/DD/YYYY", cx);
            let content = cx.debug_bounds("settings-content").unwrap();
            let reference = cx.debug_bounds("date-format-reference").unwrap();
            assert!(reference.left() >= content.left() && reference.right() <= content.right());
            cx.update(|_, cx| assert_eq!(notes.read(cx).date_format(), "MM/DD/YYYY"));
            cx.simulate_keystrokes("ctrl-a backspace");
            assert_painted("", cx);
            cx.update(|_, cx| assert!(panel.read(cx).date_example(cx).is_none()));
            cx.update(|_, cx| assert_eq!(notes.read(cx).date_format(), "MM/DD/YYYY"));
            cx.simulate_input("YYYY[");
            assert_painted("YYYY[", cx);
            cx.update(|_, cx| assert!(panel.read(cx).date_example(cx).is_none()));
            for section in [super::Section::General, super::Section::Templates] {
                cx.update(|_, cx| panel.update(cx, |panel, cx| panel.select_section(section, cx)));
                cx.refresh().unwrap();
            }
            assert_painted("YYYY[", cx);
            cx.simulate_input("date]");
            assert_painted("YYYY[date]", cx);
            cx.update(|_, cx| assert!(panel.read(cx).date_example(cx).is_some()));
            cx.update(|_, cx| assert_eq!(notes.read(cx).date_format(), "YYYY[date]"));
            let long = format!("[{}] YYYY-MM-DD", "long literal ".repeat(12));
            cx.simulate_keystrokes("ctrl-a");
            cx.simulate_input(&long);
            assert_painted(&long, cx);
            cx.update(|_, cx| assert!(date.read(cx).diagnostic_input_paint().2.x < gpui::px(0.)));
            cx.simulate_keystrokes("ctrl-a backspace");
            assert_painted("", cx);
            cx.update(|_, cx| assert_eq!(date.read(cx).diagnostic_input_paint().2.x, gpui::px(0.)));
        }
    }
}
#[gpui::test]
fn settings_content_keeps_usable_width_and_stacks_on_small_windows(cx: &mut TestAppContext) {
    let (_fixture, notes, cx) = fixture(cx, true, false);
    let panel = panel(&notes, cx);
    for width in [800., 480.] {
        cx.simulate_resize(gpui::size(gpui::px(width), gpui::px(800.)));
        input(&panel, Field::Delay, "invalid", cx);
        cx.refresh().unwrap();
        cx.run_until_parked();
        let content = cx.debug_bounds("settings-content").unwrap();
        let navigation = cx.debug_bounds("settings-navigation").unwrap();
        assert!(content.size.width >= gpui::px(240.), "{content:?}");
        assert!(content.right() <= gpui::px(width));
        if width < 560. {
            assert!(content.top() >= navigation.bottom());
        } else {
            assert!(content.left() >= navigation.right());
        }
    }
}
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
                .diagnostic_input()
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
    cx.update(|window, cx| {
        window.close_dialog(cx);
    });
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

#[gpui::test]
fn settings_fields_fill_the_content_column(cx: &mut TestAppContext) {
    let (_fixture, notes, cx) = fixture(cx, true, false);
    let panel = panel(&notes, cx);
    let fields = cx.update(|_, cx| {
        let panel = panel.read(cx);
        [
            (
                super::Section::General,
                vec![
                    format!("input-{}", panel.delay.entity_id()),
                    format!("input-{}", panel.hidden_extensions.entity_id()),
                ],
            ),
            (
                super::Section::Templates,
                vec![format!("input-{}", panel.date.entity_id())],
            ),
            (
                super::Section::Font,
                panel
                    .fonts
                    .iter()
                    .map(|font| format!("select-{}", font.entity_id()))
                    .collect(),
            ),
        ]
        .map(|(section, selectors)| {
            (
                section,
                selectors
                    .into_iter()
                    .map(|s| &*Box::leak(s.into_boxed_str()))
                    .collect::<Vec<&'static str>>(),
            )
        })
    });
    for width in [800., 560., 480.] {
        cx.simulate_resize(gpui::size(gpui::px(width), gpui::px(800.)));
        for (section, selectors) in &fields {
            cx.update(|_, cx| panel.update(cx, |panel, cx| panel.select_section(*section, cx)));
            cx.refresh().unwrap();
            cx.run_until_parked();
            cx.refresh().unwrap();
            let content = cx.debug_bounds("settings-content").unwrap();
            for selector in selectors {
                let field = cx.debug_bounds(selector).unwrap();
                assert!(
                    (field.left() - content.left()).abs() < gpui::px(1.),
                    "{selector}: {field:?}, content: {content:?}"
                );
                assert!(
                    (field.right() - content.right()).abs() < gpui::px(1.),
                    "{selector}: {field:?}, content: {content:?}"
                );
            }
        }
    }
}

#[gpui::test]
fn border_picker_switches_palettes_resets_and_ignores_stale_events(cx: &mut TestAppContext) {
    use gpui_component::{ActiveTheme, color_picker::ColorPickerEvent};
    let (_fixture, notes, cx) = fixture(cx, false, true);
    let old = panel(&notes, cx);
    let (events, _subscription) = watch(&old, cx);
    for (mode, color) in [
        (notebook::ThemeChoice::Light, 0x123456),
        (notebook::ThemeChoice::Dark, 0xabcdef),
    ] {
        intent(&old, SettingsIntent::Theme(mode), cx);
        // Exercise the actual picker subscription, not only the app intent.
        cx.update(|_, cx| {
            let picker = old.read(cx).colors[3].clone();
            picker.update(cx, |_, cx| {
                cx.emit(ColorPickerEvent::Change(Some(gpui::rgb(color).into())));
            });
        });
        cx.run_until_parked();
        cx.update(|_, cx| {
            let this = notes.read(cx);
            assert_eq!(
                this.session_appearance.palette(this.startup_dark).border,
                color
            );
            assert_eq!(cx.theme().border, gpui::rgb(color).into());
            assert_eq!(
                old.read(cx).colors[3].read(cx).value(),
                Some(gpui::rgb(color).into())
            );
        });
    }
    intent(
        &old,
        SettingsIntent::Theme(notebook::ThemeChoice::Light),
        cx,
    );
    cx.update(|_, cx| {
        assert_eq!(
            old.read(cx).colors[3].read(cx).value(),
            Some(gpui::rgb(0x123456).into())
        );
        assert_eq!(
            events.read(cx).0.len(),
            5,
            "picker acknowledgement must not emit feedback"
        );
    });
    intent(&old, SettingsIntent::ResetPalettes, cx);
    cx.update(|_, cx| {
        let appearance = notes.read(cx).appearance().resolved(false);
        assert_eq!(appearance.light, Some(notebook::ThemePalette::LIGHT));
        assert_eq!(appearance.dark, Some(notebook::ThemePalette::DARK));
        assert_eq!(
            old.read(cx).colors[3].read(cx).value(),
            Some(gpui::rgb(notebook::ThemePalette::LIGHT.border).into())
        );
        assert_eq!(events.read(cx).0.len(), 6);
    });
    intent(&old, SettingsIntent::Close, cx);
    cx.update(|window, cx| notes.update(cx, |this, cx| this.open_settings(window, cx)));
    intent(&old, SettingsIntent::Color(ColorRole::Border, 0x010203), cx);
    cx.update(|_, cx| {
        let this = notes.read(cx);
        assert_eq!(
            this.appearance().palette(this.startup_dark).border,
            notebook::ThemePalette::LIGHT.border
        );
    });
}

#[gpui::test]
fn border_preference_targets_notebook_and_rejects_changed_scope(cx: &mut TestAppContext) {
    let (fixture, notes, cx) = fixture(cx, true, true);
    let panel = panel(&notes, cx);
    let session_before = cx.update(|_, cx| notes.read(cx).session_appearance.clone());
    intent(
        &panel,
        SettingsIntent::Theme(notebook::ThemeChoice::Dark),
        cx,
    );
    intent(
        &panel,
        SettingsIntent::Color(ColorRole::Border, 0x345678),
        cx,
    );
    cx.update(|_, cx| {
        let this = notes.read(cx);
        assert_eq!(
            this.notebooks
                .active
                .as_ref()
                .unwrap()
                .settings
                .appearance
                .palette(this.startup_dark)
                .border,
            0x345678
        );
        assert_eq!(this.session_appearance, session_before);
    });
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.notebooks.active = Some(fixture.second.clone());
            this.handle_settings_intent(
                panel.entity_id(),
                SettingsIntent::Color(ColorRole::Border, 0x111111),
                window,
                cx,
            );
            assert_eq!(
                this.notebooks.active.as_ref().unwrap().settings.appearance,
                fixture.second.settings.appearance
            );
            assert!(this.settings_panel.is_none());
        });
    });
}

#[gpui::test]
fn exclusions_validate_and_reject_stale_dialog_edits(cx: &mut TestAppContext) {
    let (fixture, notes, cx) = fixture(cx, true, false);
    let old = panel(&notes, cx);
    input(
        &old,
        Field::Exclusions,
        r"^archive$, a{1,3}, report\,old",
        cx,
    );
    let expected = notebook::parse_excluded_patterns(r"^archive$, a{1,3}, report\,old").unwrap();
    input(&old, Field::Exclusions, "valid, [broken", cx);
    intent(&old, SettingsIntent::Theme(notebook::ThemeChoice::Dark), cx);
    cx.update(|_, cx| {
        assert_eq!(
            notes
                .read(cx)
                .notebooks
                .active
                .as_ref()
                .unwrap()
                .settings
                .excluded_patterns,
            expected
        );
        assert_eq!(
            old.read(cx).excluded_patterns.read(cx).value().as_ref(),
            "valid, [broken"
        );
        assert!(
            old.read(cx)
                .excluded_patterns_error
                .as_ref()
                .unwrap()
                .contains("pattern 2")
        );
    });
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.notebooks.active = Some(fixture.second.clone());
            this.notebooks.reload_exclusions();
            this.handle_settings_intent(
                old.entity_id(),
                SettingsIntent::ExcludedPatterns(vec![".*".into()]),
                window,
                cx,
            );
            assert!(
                this.notebooks
                    .active
                    .as_ref()
                    .unwrap()
                    .settings
                    .excluded_patterns
                    .is_empty()
            );
            assert!(this.settings_panel.is_none());
            this.open_settings(window, cx);
        })
    });
    let current = panel(&notes, cx);
    input(&current, Field::Exclusions, "^note", cx);
    input(&current, Field::Exclusions, " , , ", cx);
    cx.update(|_, cx| {
        assert!(
            notes
                .read(cx)
                .notebooks
                .active
                .as_ref()
                .unwrap()
                .settings
                .excluded_patterns
                .is_empty()
        )
    });
}

#[gpui::test]
fn exclusions_are_disabled_without_a_notebook_and_while_busy(cx: &mut TestAppContext) {
    let (_fixture, notes, cx) = fixture(cx, false, true);
    let panel = panel(&notes, cx);
    let (events, _subscription) = watch(&panel, cx);
    input(&panel, Field::Exclusions, ".*", cx);
    cx.update(|_, cx| {
        assert!(panel.read(cx).values.excluded_patterns.is_none());
        assert!(events.read(cx).0.is_empty());
        assert!(notes.read(cx).notebooks.active.is_none());
        panel.update(cx, |panel, cx| {
            panel.values.excluded_patterns = Some(Vec::new());
            panel.values.busy = true;
            panel.edit_excluded_patterns(".*", cx);
        });
        assert!(events.read(cx).0.is_empty());
    });
}

#[gpui::test]
fn externally_invalid_exclusions_are_visible_and_correctable(cx: &mut TestAppContext) {
    let (_fixture, notes, cx) = fixture(cx, true, false);
    let old = panel(&notes, cx);
    intent(&old, SettingsIntent::Close, cx);
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.notebooks
                .active
                .as_mut()
                .unwrap()
                .settings
                .excluded_patterns = vec!["^note".into(), "[broken".into()];
            this.notebooks.reload_exclusions();
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
            this.open_settings(window, cx);
        })
    });
    let current = panel(&notes, cx);
    cx.update(|_, cx| {
        assert!(
            current
                .read(cx)
                .excluded_patterns_error
                .as_ref()
                .unwrap()
                .contains("pattern 2")
        );
        assert_eq!(
            current.read(cx).excluded_patterns.read(cx).value().as_ref(),
            "^note, [broken"
        );
    });
    input(&current, Field::Exclusions, "^note", cx);
    cx.update(|_, cx| {
        assert!(
            notes
                .read(cx)
                .notebooks
                .exclusions
                .excludes(std::path::Path::new("note.md"))
        );
        assert!(notes.read(cx).notebooks.notice.is_none());
        assert!(current.read(cx).excluded_patterns_error.is_none());
    });
}

#[gpui::test]
fn toolbar_switches_update_rendering_and_restore_an_empty_toolbar(cx: &mut TestAppContext) {
    let (_fixture, notes, cx) = fixture(cx, false, true);
    let form = panel(&notes, cx);
    cx.update(|_, cx| {
        form.update(cx, |form, cx| {
            form.select_section(super::Section::Toolbar, cx)
        })
    });
    let selectors = [
        "toolbar-sidebar-toggle",
        "sidebar-explorer",
        "sidebar-search",
        "sidebar-sort",
    ];
    for mask in 0..16 {
        for (index, (item, _)) in notebook::ToolbarItem::OPTIONS.into_iter().enumerate() {
            intent(
                &form,
                SettingsIntent::ToolbarItemEnabled(item, mask & (1 << index) != 0),
                cx,
            );
        }
        cx.refresh().unwrap();
        cx.update(|_, cx| {
            let settings = notes.read(cx).toolbar_settings();
            assert_eq!(settings.any_enabled(), mask != 0);
            for (index, (item, _)) in notebook::ToolbarItem::OPTIONS.into_iter().enumerate() {
                assert_eq!(settings.enabled(item), mask & (1 << index) != 0);
            }
        });
        // Sidebar tools remain configurable without a notebook, but are not rendered.
        for selector in selectors {
            assert!(cx.debug_bounds(selector).is_none());
        }
        cx.update(|_, cx| {
            assert_eq!(form.read(cx).values.toolbar, notes.read(cx).session_toolbar);
        });
    }
    for (item, _) in notebook::ToolbarItem::OPTIONS {
        intent(&form, SettingsIntent::ToolbarItemEnabled(item, true), cx);
    }
    for width in [800., 480.] {
        cx.simulate_resize(gpui::size(gpui::px(width), gpui::px(800.)));
        cx.refresh().unwrap();
        assert!(cx.debug_bounds("settings-content").unwrap().size.width > gpui::px(240.));
    }
    intent(&form, SettingsIntent::Close, cx);
    cx.update(|window, cx| notes.update(cx, |this, cx| this.open_settings(window, cx)));
    let reopened = panel(&notes, cx);
    intent(
        &form,
        SettingsIntent::ToolbarItemEnabled(notebook::ToolbarItem::Search, false),
        cx,
    );
    cx.update(|_, cx| {
        assert_eq!(
            reopened.read(cx).values.toolbar,
            notebook::ToolbarSettings::default()
        );
        assert_eq!(
            notes.read(cx).session_toolbar,
            notebook::ToolbarSettings::default()
        );
    });
}

#[gpui::test]
fn toolbar_preferences_persist_for_notebook_and_reject_stale_scope(cx: &mut TestAppContext) {
    let (fixture, notes, cx) = fixture(cx, true, true);
    cx.update(|_, cx| {
        notes.update(cx, |this, _| {
            this.notebooks
                .set_test_persistence_target(fixture._directory.path().join("index.json"));
        })
    });
    let form = panel(&notes, cx);
    intent(
        &form,
        SettingsIntent::ToolbarItemEnabled(notebook::ToolbarItem::Search, false),
        cx,
    );
    intent(
        &form,
        SettingsIntent::ToolbarItemEnabled(notebook::ToolbarItem::Sort, false),
        cx,
    );
    intent(&form, SettingsIntent::TitleBarSidebar(true), cx);
    intent(
        &form,
        SettingsIntent::ToolbarItemEnabled(notebook::ToolbarItem::Terminal, false),
        cx,
    );
    intent(&form, SettingsIntent::TitleBarTerminal(true), cx);
    let expected = notebook::ToolbarSettings {
        title_bar_sidebar: true,
        title_bar_terminal: true,
        terminal: false,
        search: false,
        sort: false,
        ..notebook::ToolbarSettings::default()
    };
    assert_eq!(
        notebook::Notebook::open(&fixture.first.root, false)
            .unwrap()
            .settings
            .toolbar,
        expected
    );
    assert_eq!(
        notebook::Notebook::open(&fixture.second.root, false)
            .unwrap()
            .settings
            .toolbar,
        notebook::ToolbarSettings::default()
    );
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            assert_eq!(this.toolbar_settings(), expected);
            assert_eq!(this.session_toolbar, notebook::ToolbarSettings::default());
            this.notebooks.active = Some(fixture.second.clone());
            this.handle_settings_intent(
                form.entity_id(),
                SettingsIntent::TitleBarTerminal(true),
                window,
                cx,
            );
            assert_eq!(
                this.toolbar_settings(),
                notebook::ToolbarSettings::default()
            );
            assert!(this.settings_panel.is_none());
            this.notebooks.active =
                Some(notebook::Notebook::open(&fixture.first.root, false).unwrap());
            this.open_settings(window, cx);
        });
    });
    let reopened = panel(&notes, cx);
    cx.update(|_, cx| assert_eq!(reopened.read(cx).values.toolbar, expected));
}

#[gpui::test]
fn toolbar_form_ignores_changes_while_busy(cx: &mut TestAppContext) {
    let (_fixture, notes, cx) = fixture(cx, false, true);
    let form = panel(&notes, cx);
    let (events, _subscription) = watch(&form, cx);
    cx.update(|_, cx| form.update(cx, |form, _| form.values.busy = true));
    intent(
        &form,
        SettingsIntent::ToolbarItemEnabled(notebook::ToolbarItem::Search, false),
        cx,
    );
    intent(&form, SettingsIntent::TitleBarSidebar(false), cx);
    intent(&form, SettingsIntent::TitleBarTerminal(true), cx);
    cx.update(|_, cx| {
        assert!(events.read(cx).0.is_empty());
        assert_eq!(
            notes.read(cx).toolbar_settings(),
            notebook::ToolbarSettings::default()
        );
    });
}

#[gpui::test]
fn toolbar_renders_only_enabled_buttons_in_order(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let dir = tempfile::tempdir().unwrap();
    let book = notebook::Notebook::open(&dir.path().join("book"), true).unwrap();
    for mask in 0..16 {
        // GPUI retains old debug bounds, so each snapshot needs a fresh window.
        let (_, view) = cx.add_window_view(|window, cx| {
            let notes = cx.new(|cx| {
                let mut notes = Notes::new(window, cx);
                for (index, (item, _)) in notebook::ToolbarItem::OPTIONS.into_iter().enumerate() {
                    let mut book = book.clone();
                    book.settings.toolbar = notes.session_toolbar;
                    notes.notebooks.active = Some(book);
                    notes
                        .session_toolbar
                        .set_enabled(item, mask & (1 << index) != 0);
                }
                let mut book = book.clone();
                book.settings.toolbar = notes.session_toolbar;
                notes.notebooks.active = Some(book);
                notes
            });
            let root = cx.new(|cx| NotesWindow {
                _subscription: cx.observe(&notes, |_, _, cx| cx.notify()),
                notes,
            });
            Root::new(root, window, cx)
        });
        view.refresh().unwrap();
        assert_eq!(view.debug_bounds("sidebar-toolbar").is_some(), mask != 0);
        let mut previous_x = None;
        for (index, selector) in [
            (0, "toolbar-sidebar-toggle"),
            (1, "sidebar-explorer"),
            (2, "sidebar-search"),
            (3, "sidebar-sort"),
        ]
        .into_iter()
        {
            let bounds = view.debug_bounds(selector);
            assert_eq!(
                bounds.is_some(),
                mask & (1 << index) != 0,
                "mask {mask}, {selector}"
            );
            if let Some(bounds) = bounds {
                if let Some(x) = previous_x {
                    assert!(bounds.origin.x > x);
                }
                previous_x = Some(bounds.origin.x);
            }
        }
    }
}

fn toolbar_section(form: &Entity<SettingsPanel>, cx: &mut VisualTestContext) {
    cx.update(|_, cx| {
        form.update(cx, |form, cx| {
            form.select_section(super::Section::Toolbar, cx)
        })
    });
    cx.refresh().unwrap();
}

#[gpui::test]
fn toolbar_settings_rows_have_fixed_order_no_grips_and_do_not_drag(cx: &mut TestAppContext) {
    use gpui::{Modifiers, MouseButton, point, px};
    use notebook::ToolbarItem::*;
    let (_fixture, notes, cx) = fixture(cx, false, true);
    cx.simulate_resize(gpui::size(px(1000.), px(1400.)));
    let form = panel(&notes, cx);
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            let mut reversed = notebook::ToolbarItem::DEFAULT_ORDER;
            reversed.reverse();
            this.session_toolbar =
                serde_json::from_value(serde_json::json!({"order": reversed})).unwrap();
            this.sync_settings_panel(window, cx);
        })
    });
    toolbar_section(&form, cx);
    for (region, _) in notebook::ToolbarRegion::OPTIONS {
        let mut previous_y = None;
        for item in notebook::ToolbarItem::DEFAULT_ORDER
            .into_iter()
            .filter(|item| item.region() == region)
        {
            let id = item as usize;
            assert!(
                cx.debug_bounds(format!("toolbar-grip-{id}").leak())
                    .is_none()
            );
            let row = cx.debug_bounds(format!("toolbar-row-{id}").leak()).unwrap();
            if let Some(y) = previous_y {
                assert!(row.top() > y);
            }
            previous_y = Some(row.top());
        }
    }
    for item in [Sidebar, Explorer, Search, Sort] {
        let id = item as usize;
        let row = cx.debug_bounds(format!("toolbar-row-{id}").leak()).unwrap();
        // Drag blank row space, away from the still-interactive visibility switch.
        let start = point(row.right() - px(30.), row.center().y);
        let end = point(start.x + px(12.), start.y);
        cx.simulate_mouse_move(start, None, Modifiers::default());
        cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_move(end, MouseButton::Left, Modifiers::default());
        cx.refresh().unwrap();
        cx.update(|_, cx| assert!(!cx.has_active_drag()));
        for after in [false, true] {
            assert!(
                cx.debug_bounds(format!("toolbar-drop-{id}-{after}").leak())
                    .is_none()
            );
        }
        cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::default());
        cx.run_until_parked();
        cx.refresh().unwrap();
        assert_eq!(
            cx.debug_bounds(format!("toolbar-row-{id}").leak()).unwrap(),
            row
        );
    }
    cx.update(|_, cx| {
        assert_eq!(
            notes.read(cx).toolbar_settings(),
            notebook::ToolbarSettings::default()
        )
    });
    let switch = cx.debug_bounds("toolbar-switch-3").unwrap();
    cx.simulate_click(
        point(switch.left() + px(8.), switch.center().y),
        Modifiers::default(),
    );
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert!(!notes.read(cx).toolbar_settings().sort);
        assert!(!cx.has_active_drag());
    });
    intent(&form, SettingsIntent::Close, cx);
    cx.update(|window, cx| notes.update(cx, |this, cx| this.open_settings(window, cx)));
    let reopened = panel(&notes, cx);
    toolbar_section(&reopened, cx);
    cx.update(|_, cx| assert!(!reopened.read(cx).values.toolbar.sort));
    intent(
        &reopened,
        SettingsIntent::ToolbarItemEnabled(Sort, true),
        cx,
    );
    assert!(
        cx.debug_bounds("toolbar-row-0").unwrap().top()
            < cx.debug_bounds("toolbar-row-3").unwrap().top()
    );
    cx.update(|_, cx| assert!(notes.read(cx).toolbar_settings().sort));
}

#[gpui::test]
fn toolbar_size_buttons_apply_immediately_and_preserve_session_scope(cx: &mut TestAppContext) {
    use gpui::{Modifiers, MouseButton, px};
    let (fixture, notes, cx) = fixture(cx, false, true);
    cx.simulate_resize(gpui::size(px(1800.), px(900.)));
    let form = panel(&notes, cx);
    intent(
        &form,
        SettingsIntent::ToolbarItemEnabled(notebook::ToolbarItem::Sidebar, true),
        cx,
    );
    toolbar_section(&form, cx);
    for (index, (size, _)) in notebook::ToolbarIconSize::OPTIONS.into_iter().enumerate() {
        let bounds = cx
            .debug_bounds(
                [
                    "toolbar-icon-size-0",
                    "toolbar-icon-size-1",
                    "toolbar-icon-size-2",
                ][index],
            )
            .unwrap();
        cx.simulate_mouse_down(bounds.center(), MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_up(bounds.center(), MouseButton::Left, Modifiers::default());
        cx.run_until_parked();
        cx.refresh().unwrap();
        cx.update(|_, cx| {
            assert_eq!(notes.read(cx).session_toolbar.icon_size, size);
            assert_eq!(form.read(cx).values.toolbar.icon_size, size);
        });
        let mut first = None;
        for selector in [
            "toolbar-bold",
            "toolbar-italic",
            "toolbar-strikethrough",
            "toolbar-heading",
        ] {
            let button = cx.debug_bounds(selector).unwrap();
            let icon = cx
                .debug_bounds(match selector {
                    "toolbar-bold" => "toolbar-bold-icon",
                    "toolbar-italic" => "toolbar-italic-icon",
                    "toolbar-strikethrough" => "toolbar-strikethrough-icon",
                    _ => "toolbar-heading-icon",
                })
                .unwrap();
            assert_eq!(
                button.size,
                gpui::size(px(size.button_pixels()), px(size.button_pixels()))
            );
            assert_eq!(
                icon.size,
                gpui::size(px(size.icon_pixels()), px(size.icon_pixels()))
            );
            if let Some(top) = first {
                assert_eq!(button.top(), top);
            } else {
                first = Some(button.top());
            }
        }
        let toolbar = cx.debug_bounds("content-toolbar").unwrap();
        // Four-pixel padding on both sides and one border pixel at the default rem size.
        assert_eq!(toolbar.size.height, px(size.button_pixels() + 9.));
    }
    cx.update(|window, cx| {
        window.close_dialog(cx);
        notes.update(cx, |this, cx| {
            this.notebooks.active = Some(fixture.first.clone());
            assert_eq!(
                this.toolbar_settings().icon_size,
                notebook::ToolbarIconSize::Medium
            );
            this.notebooks.active = None;
            assert_eq!(
                this.toolbar_settings().icon_size,
                notebook::ToolbarIconSize::Large
            );
            this.open_settings(window, cx);
        });
    });
    intent(
        &form,
        SettingsIntent::ToolbarIconSize(notebook::ToolbarIconSize::Small),
        cx,
    );
    cx.update(|_, cx| {
        assert_eq!(
            notes.read(cx).session_toolbar.icon_size,
            notebook::ToolbarIconSize::Large
        )
    });
}

#[gpui::test]
fn toolbar_size_persists_for_notebook_and_rejects_scope_and_busy_events(cx: &mut TestAppContext) {
    use notebook::ToolbarIconSize::{Large, Medium};
    let (fixture, notes, cx) = fixture(cx, true, true);
    cx.update(|_, cx| {
        notes.update(cx, |this, _| {
            this.notebooks
                .set_test_persistence_target(fixture._directory.path().join("index.json"))
        })
    });
    let form = panel(&notes, cx);
    intent(&form, SettingsIntent::ToolbarIconSize(Large), cx);
    assert_eq!(
        notebook::Notebook::open(&fixture.first.root, false)
            .unwrap()
            .settings
            .toolbar
            .icon_size,
        Large
    );
    assert_eq!(
        notebook::Notebook::open(&fixture.second.root, false)
            .unwrap()
            .settings
            .toolbar
            .icon_size,
        Medium
    );
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            assert_eq!(this.session_toolbar.icon_size, Medium);
            this.notebooks.active = Some(fixture.second.clone());
            this.handle_settings_intent(
                form.entity_id(),
                SettingsIntent::ToolbarIconSize(Medium),
                window,
                cx,
            );
            assert_eq!(this.toolbar_settings().icon_size, Medium);
            assert!(this.settings_panel.is_none());
            this.open_settings(window, cx);
        })
    });
    let form = panel(&notes, cx);
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            let _ = this.session.pick();
            this.handle_settings_intent(
                form.entity_id(),
                SettingsIntent::ToolbarIconSize(Large),
                window,
                cx,
            );
            assert_eq!(this.toolbar_settings().icon_size, Medium);
            assert!(form.read(cx).values.busy);
        })
    });
    let (events, _subscription) = watch(&form, cx);
    intent(&form, SettingsIntent::ToolbarIconSize(Medium), cx);
    cx.update(|_, cx| assert!(events.read(cx).0.is_empty()));
}

#[gpui::test]
fn toolbar_wraps_and_derives_height_for_every_icon_size(cx: &mut TestAppContext) {
    use gpui::{px, size};
    cx.update(gpui_component::init);
    for (icon_size, _) in notebook::ToolbarIconSize::OPTIONS {
        let (_, view) = cx.add_window_view(|window, cx| {
            let notes = cx.new(|cx| {
                let mut notes = Notes::new(window, cx);
                notes.session_toolbar.icon_size = icon_size;
                notes.session_toolbar.sidebar = true;
                for item in notebook::ToolbarItem::DEFAULT_ORDER
                    .into_iter()
                    .filter(|item| {
                        ![
                            notebook::ToolbarItem::Bold,
                            notebook::ToolbarItem::Italic,
                            notebook::ToolbarItem::Strikethrough,
                            notebook::ToolbarItem::Heading,
                        ]
                        .contains(item)
                    })
                {
                    notes.session_toolbar.set_enabled(item, false);
                }
                notes
            });
            let root = cx.new(|cx| NotesWindow {
                _subscription: cx.observe(&notes, |_, _, cx| cx.notify()),
                notes,
            });
            Root::new(root, window, cx)
        });
        // Fit exactly two targets and their gap on each row.
        let width = icon_size.button_pixels() * 2. + 12.;
        view.simulate_resize(size(px(width), px(800.)));
        view.refresh().unwrap();
        view.run_until_parked();
        view.refresh().unwrap();
        let toolbar = view.debug_bounds("content-toolbar").unwrap();
        let first = view.debug_bounds("toolbar-bold").unwrap();
        let second = view.debug_bounds("toolbar-italic").unwrap();
        let third = view.debug_bounds("toolbar-strikethrough").unwrap();
        let fourth = view.debug_bounds("toolbar-heading").unwrap();
        assert_eq!(first.top(), second.top());
        assert_eq!(third.top(), fourth.top());
        assert_eq!(third.top(), first.bottom() + px(4.));
        assert_eq!(first.left(), third.left());
        assert!(fourth.right() <= toolbar.right());
        assert_eq!(
            toolbar.size.height,
            px(icon_size.button_pixels() * 2. + 13.),
            "toolbar={toolbar:?} first={first:?} second={second:?} third={third:?} fourth={fourth:?}"
        );
    }
}

#[gpui::test]
fn formatting_toolbar_items_toggle_and_keep_their_fixed_position(cx: &mut TestAppContext) {
    use notebook::ToolbarItem::{Bold, Italic};
    let (_fixture, notes, cx) = fixture(cx, false, true);
    cx.simulate_resize(gpui::size(gpui::px(1000.), gpui::px(1400.)));
    let form = panel(&notes, cx);
    intent(&form, SettingsIntent::ToolbarItemEnabled(Italic, true), cx);
    toolbar_section(&form, cx);
    cx.update(|_, cx| {
        form.update(cx, |form, cx| {
            form.content_scroll
                .set_offset(gpui::point(gpui::px(0.), gpui::px(-100.)));
            cx.notify();
        })
    });
    cx.refresh().unwrap();
    assert!(
        cx.debug_bounds("toolbar-bold").unwrap().left()
            < cx.debug_bounds("toolbar-italic").unwrap().left()
    );
    let switch = cx.debug_bounds("toolbar-switch-4").unwrap();
    cx.simulate_click(
        gpui::point(switch.left() + gpui::px(8.), switch.center().y),
        gpui::Modifiers::default(),
    );
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert!(!notes.read(cx).toolbar_settings().bold);
    });
    intent(&form, SettingsIntent::Close, cx);
    cx.update(|window, cx| notes.update(cx, |this, cx| this.open_settings(window, cx)));
    let reopened = panel(&notes, cx);
    cx.update(|_, cx| {
        assert!(!reopened.read(cx).values.toolbar.bold);
    });
    intent(
        &reopened,
        SettingsIntent::ToolbarItemEnabled(Bold, true),
        cx,
    );
    cx.update(|_, cx| assert!(notes.read(cx).toolbar_settings().bold));
}

#[gpui::test]
fn toolbar_reset_button_restores_every_setting_and_rejects_stale_form(cx: &mut TestAppContext) {
    use gpui::{Modifiers, MouseButton};
    let (fixture, notes, cx) = fixture(cx, false, true);
    let form = panel(&notes, cx);
    assert!(cx.update(|_, cx| notes.read(cx).toolbar_settings().sidebar));
    intent(
        &form,
        SettingsIntent::ToolbarItemEnabled(notebook::ToolbarItem::Sidebar, false),
        cx,
    );
    intent(
        &form,
        SettingsIntent::ToolbarIconSize(notebook::ToolbarIconSize::Large),
        cx,
    );
    intent(
        &form,
        SettingsIntent::ToolbarItemEnabled(notebook::ToolbarItem::Search, false),
        cx,
    );
    toolbar_section(&form, cx);
    let bounds = cx.debug_bounds("reset-toolbar").unwrap();
    cx.simulate_mouse_down(bounds.center(), MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_up(bounds.center(), MouseButton::Left, Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| {
        assert_eq!(
            form.read(cx).values.toolbar,
            notebook::ToolbarSettings::default()
        );
        assert!(form.read(cx).values.toolbar.sidebar);
        notes.update(cx, |this, cx| {
            assert_eq!(this.session_toolbar, notebook::ToolbarSettings::default());
            assert_eq!(
                fixture.first.settings.toolbar,
                notebook::ToolbarSettings::default()
            );
            this.close_settings_panel(form.entity_id(), window, cx);
            this.open_settings(window, cx);
            this.session_toolbar.icon_size = notebook::ToolbarIconSize::Small;
            this.handle_settings_intent(form.entity_id(), SettingsIntent::ResetToolbar, window, cx);
            assert_eq!(
                this.session_toolbar.icon_size,
                notebook::ToolbarIconSize::Small
            );
        });
    });
}

#[gpui::test]
fn toolbar_reset_persists_atomically_skips_defaults_and_rejects_scope_and_busy(
    cx: &mut TestAppContext,
) {
    let (fixture, notes, cx) = fixture(cx, true, true);
    cx.update(|_, cx| {
        notes.update(cx, |this, _| {
            this.notebooks
                .set_test_persistence_target(fixture._directory.path().join("index.json"));
        })
    });
    let form = panel(&notes, cx);
    intent(
        &form,
        SettingsIntent::ToolbarIconSize(notebook::ToolbarIconSize::Small),
        cx,
    );
    intent(
        &form,
        SettingsIntent::ToolbarItemEnabled(notebook::ToolbarItem::Bold, false),
        cx,
    );
    intent(&form, SettingsIntent::TitleBarTerminal(true), cx);
    intent(&form, SettingsIntent::TitleBarSidebar(true), cx);
    intent(&form, SettingsIntent::ResetToolbar, cx);
    assert_eq!(
        notebook::Notebook::open(&fixture.first.root, false)
            .unwrap()
            .settings
            .toolbar,
        notebook::ToolbarSettings::default()
    );
    // Removing this fixture file makes an unnecessary write observable, without timing assumptions.
    let settings_path = fixture.first.root.join(".mithril/settings.json");
    fs::remove_file(&settings_path).unwrap();
    intent(&form, SettingsIntent::ResetToolbar, cx);
    assert!(!settings_path.exists());
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            assert_eq!(this.session_toolbar, notebook::ToolbarSettings::default());
            let mut second = fixture.second.clone();
            second.settings.toolbar.icon_size = notebook::ToolbarIconSize::Large;
            this.notebooks.active = Some(second);
            this.handle_settings_intent(form.entity_id(), SettingsIntent::ResetToolbar, window, cx);
            assert_eq!(
                this.toolbar_settings().icon_size,
                notebook::ToolbarIconSize::Large
            );
            assert!(this.settings_panel.is_none());
            this.open_settings(window, cx);
        })
    });
    let form = panel(&notes, cx);
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            let _ = this.session.pick();
            this.handle_settings_intent(form.entity_id(), SettingsIntent::ResetToolbar, window, cx);
            assert_eq!(
                this.toolbar_settings().icon_size,
                notebook::ToolbarIconSize::Large
            );
            assert!(form.read(cx).values.busy);
        })
    });
    let (events, _subscription) = watch(&form, cx);
    intent(&form, SettingsIntent::ResetToolbar, cx);
    cx.update(|_, cx| assert!(events.read(cx).0.is_empty()));
}

#[gpui::test]
fn modal_settings_bounds_scrolling_resize_and_close(cx: &mut TestAppContext) {
    use gpui::{point, px, size};
    let (_fixture, notes, cx) = fixture(cx, true, false);
    let form = panel(&notes, cx);
    cx.simulate_resize(size(px(1000.), px(800.)));
    toolbar_section(&form, cx);
    cx.refresh().unwrap();
    cx.run_until_parked();
    let surface = cx.debug_bounds("dialog-surface").unwrap();
    assert_eq!(surface.size, size(px(800.), px(640.)));
    assert_eq!(surface.origin, point(px(100.), px(80.)));
    cx.update(|window, cx| assert!(window.has_active_dialog(cx)));
    let header = cx.debug_bounds("dialog-title").unwrap();
    let navigation = cx.debug_bounds("settings-navigation").unwrap();
    let content = cx.debug_bounds("settings-content").unwrap();
    cx.update(|_, cx| {
        form.update(cx, |form, cx| {
            form.content_scroll.set_offset(point(px(0.), px(-10000.)));
            cx.notify();
        })
    });
    cx.refresh().unwrap();
    let last = notebook::ToolbarItem::Terminal;
    let last_bounds = cx
        .debug_bounds(format!("toolbar-row-{}", last as usize).leak())
        .unwrap();
    assert!(last_bounds.top() >= content.top() && last_bounds.bottom() <= content.bottom());
    assert_eq!(cx.debug_bounds("dialog-title").unwrap(), header);
    assert_eq!(cx.debug_bounds("settings-navigation").unwrap(), navigation);
    cx.update(|_, cx| {
        form.update(cx, |form, cx| {
            form.select_section(super::Section::Theme, cx)
        })
    });
    cx.refresh().unwrap();
    assert_eq!(cx.debug_bounds("dialog-surface").unwrap(), surface);
    cx.update(|_, cx| assert_eq!(form.read(cx).content_scroll.offset(), point(px(0.), px(0.))));
    // The modal is independent of the sidebar allocation.
    cx.update(|window, cx| notes.update(cx, |this, cx| this.toggle_sidebar(window, cx)));
    cx.refresh().unwrap();
    assert_eq!(cx.debug_bounds("dialog-surface").unwrap(), surface);
    for viewport in [size(px(480.), px(360.)), size(px(320.), px(300.))] {
        cx.simulate_resize(viewport);
        cx.refresh().unwrap();
        cx.run_until_parked();
        cx.refresh().unwrap();
        let surface = cx.debug_bounds("dialog-surface").unwrap();
        let content = cx.debug_bounds("settings-content").unwrap();
        let navigation = cx.debug_bounds("settings-navigation").unwrap();
        assert!(content.top() >= navigation.bottom());
        assert!(
            content.bottom() <= surface.bottom(),
            "surface={surface:?} content={content:?} navigation={navigation:?}"
        );
        assert!(content.size.height > px(0.));
        assert_eq!(
            surface.size,
            size(viewport.width * 0.8, viewport.height * 0.8)
        );
        assert!(surface.left() >= px(24.) && surface.top() >= px(24.));
        assert!(surface.right() <= viewport.width - px(24.));
        assert!(surface.bottom() <= viewport.height - px(24.));
        toolbar_section(&form, cx);
        cx.update(|_, cx| {
            form.update(cx, |form, cx| {
                form.content_scroll.set_offset(point(px(0.), px(-10000.)));
                cx.notify();
            })
        });
        cx.refresh().unwrap();
        let content = cx.debug_bounds("settings-content").unwrap();
        let last_bounds = cx
            .debug_bounds(format!("toolbar-row-{}", last as usize).leak())
            .unwrap();
        assert!(last_bounds.top() >= content.top() && last_bounds.bottom() <= content.bottom());
    }
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    cx.update(|window, cx| {
        assert!(notes.read(cx).settings_panel.is_none());
        assert!(notes.read(cx).tabs.active.editor.is_focused(window, cx));
    });
}

#[gpui::test]
fn modal_settings_scrollbar_pointer_and_wheel_follow_viewport(cx: &mut TestAppContext) {
    use gpui::{Modifiers, MouseButton, ScrollDelta, ScrollWheelEvent, point, px, size};
    let (_fixture, notes, cx) = fixture(cx, true, false);
    let form = panel(&notes, cx);
    cx.update(|_, cx| {
        gpui_component::Theme::global_mut(cx).scrollbar_show =
            gpui_component::scroll::ScrollbarShow::Always;
    });
    let last = notebook::ToolbarItem::Terminal;
    let last_selector: &str = format!("toolbar-row-{}", last as usize).leak();
    for viewport in [
        size(px(1000.), px(800.)),
        size(px(480.), px(360.)),
        size(px(320.), px(300.)),
        size(px(1000.), px(800.)),
    ] {
        cx.simulate_resize(viewport);
        toolbar_section(&form, cx);
        cx.run_until_parked();
        cx.refresh().unwrap();
        let surface = cx.debug_bounds("dialog-surface").unwrap();
        let header = cx.debug_bounds("dialog-title").unwrap();
        let navigation = cx.debug_bounds("settings-navigation").unwrap();
        let content = cx.debug_bounds("settings-content").unwrap();
        let scroll = cx.update(|_, cx| form.read(cx).content_scroll.clone());
        assert!(scroll.max_offset().height > px(0.));
        assert_eq!(scroll.offset().y, px(0.));

        cx.simulate_event(ScrollWheelEvent {
            position: content.center(),
            delta: ScrollDelta::Pixels(point(px(0.), px(-80.))),
            ..Default::default()
        });
        cx.refresh().unwrap();
        assert!(
            scroll.offset().y < px(0.),
            "wheel did not scroll at {viewport:?}"
        );

        // Hit the track at the viewport edge after wheel scrolling. A scrollbar
        // inside the scrolled element moves away from these fixed coordinates.
        let bottom = point(content.right() - px(8.), content.bottom() - px(8.));
        cx.simulate_click(bottom, Modifiers::default());
        cx.refresh().unwrap();
        assert_eq!(
            scroll.offset().y,
            -scroll.max_offset().height,
            "track click at {viewport:?}"
        );
        let last_bounds = cx.debug_bounds(last_selector).unwrap();
        assert!(last_bounds.top() >= content.top() && last_bounds.bottom() <= content.bottom());

        let top = point(content.right() - px(8.), content.top() + px(12.));
        cx.simulate_click(top, Modifiers::default());
        cx.refresh().unwrap();
        assert_eq!(scroll.offset().y, px(0.), "track return at {viewport:?}");

        cx.simulate_mouse_move(top, None, Modifiers::default());
        cx.simulate_mouse_down(top, MouseButton::Left, Modifiers::default());
        // The component throttles thumb updates with a wall-clock Instant.
        std::thread::sleep(Duration::from_millis(20));
        cx.simulate_mouse_move(bottom, MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_up(bottom, MouseButton::Left, Modifiers::default());
        cx.refresh().unwrap();
        assert_eq!(
            scroll.offset().y,
            -scroll.max_offset().height,
            "thumb drag at {viewport:?}"
        );
        let last_bounds = cx.debug_bounds(last_selector).unwrap();
        assert!(last_bounds.top() >= content.top() && last_bounds.bottom() <= content.bottom());
        assert_eq!(cx.debug_bounds("settings-content").unwrap(), content);
        assert_eq!(cx.debug_bounds("dialog-title").unwrap(), header);
        assert_eq!(cx.debug_bounds("settings-navigation").unwrap(), navigation);

        cx.update(|_, cx| {
            form.update(cx, |form, cx| {
                form.select_section(super::Section::Theme, cx)
            })
        });
        cx.refresh().unwrap();
        assert_eq!(scroll.offset().y, px(0.));
        assert_eq!(cx.debug_bounds("dialog-surface").unwrap(), surface);
        if viewport.width == px(1000.) {
            assert_eq!(scroll.max_offset().height, px(0.));
            cx.simulate_click(bottom, Modifiers::default());
            cx.refresh().unwrap();
            assert_eq!(scroll.offset().y, px(0.));
        }
    }
}

#[gpui::test]
fn modal_settings_close_icon_restores_focus_after_drag_resize_and_reopen(cx: &mut TestAppContext) {
    use gpui::{Modifiers, MouseButton, point, px, size};
    let (_fixture, notes, cx) = fixture(cx, false, true);
    for moved in [false, true] {
        let form = panel(&notes, cx);
        cx.simulate_resize(size(px(1000.), px(800.)));
        cx.refresh().unwrap();
        if moved {
            let before = cx.debug_bounds("dialog-surface").unwrap();
            let title = cx.debug_bounds("dialog-title").unwrap().center();
            let destination = title + point(px(40.), px(30.));
            cx.simulate_mouse_down(title, MouseButton::Left, Modifiers::default());
            cx.simulate_mouse_move(destination, MouseButton::Left, Modifiers::default());
            cx.refresh().unwrap();
            cx.simulate_mouse_up(destination, MouseButton::Left, Modifiers::default());
            cx.refresh().unwrap();
            assert_ne!(
                cx.debug_bounds("dialog-surface").unwrap().origin,
                before.origin
            );
            cx.simulate_resize(size(px(480.), px(360.)));
            cx.refresh().unwrap();
        }
        let close = cx.debug_bounds("dialog-close").unwrap().center();
        cx.simulate_click(close, Modifiers::default());
        cx.run_until_parked();
        cx.update(|window, cx| {
            assert!(
                !window.has_active_dialog(cx),
                "close icon did not dismiss Settings"
            );
            assert!(notes.read(cx).settings_panel.is_none());
            assert!(notes.read(cx).tabs.active.editor.is_focused(window, cx));
            notes.update(cx, |this, cx| this.open_settings(window, cx));
        });
        cx.refresh().unwrap();
        assert_ne!(panel(&notes, cx), form);
    }
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    cx.update(|window, cx| {
        assert!(!window.has_active_dialog(cx));
        assert!(notes.read(cx).settings_panel.is_none());
        assert!(notes.read(cx).tabs.active.editor.is_focused(window, cx));
    });
}

#[gpui::test]
fn modal_settings_reopen_preserves_drafts_and_escape_closes(cx: &mut TestAppContext) {
    let (_fixture, notes, cx) = fixture(cx, false, true);
    let form = panel(&notes, cx);
    cx.update(|_, cx| {
        form.update(cx, |form, cx| {
            form.select_section(super::Section::Templates, cx)
        })
    });
    input(&form, Field::Date, "YYYY[", cx);
    cx.update(|window, cx| notes.update(cx, |this, cx| this.open_settings(window, cx)));
    assert_eq!(panel(&notes, cx), form);
    cx.update(|window, cx| {
        assert!(form.read(cx).section == super::Section::Templates);
        assert_eq!(form.read(cx).date.read(cx).value().as_ref(), "YYYY[");
        form.read(cx)
            .date
            .clone()
            .update(cx, |date, cx| date.focus(window, cx));
    });
    cx.refresh().unwrap();
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    cx.update(|window, cx| {
        assert!(notes.read(cx).settings_panel.is_none());
        notes.update(cx, |this, cx| this.open_settings(window, cx));
    });
    let fresh = panel(&notes, cx);
    assert_ne!(fresh, form);
    cx.update(|_, cx| assert!(fresh.read(cx).section == super::Section::General));
}

#[gpui::test]
fn modal_settings_preserves_document_modes_selection_and_undo(cx: &mut TestAppContext) {
    use crate::ViewMode;
    let (_fixture, notes, cx) = fixture(cx, false, true);
    let form = panel(&notes, cx);
    intent(&form, SettingsIntent::Close, cx);
    for mode in [ViewMode::Source, ViewMode::LivePreview, ViewMode::Reading] {
        let (tab, editor, selection) = cx.update(|window, cx| {
            notes.update(cx, |this, cx| {
                this.set_mode(ViewMode::Source, window, cx);
                this.tabs.active.editor.select(0..8, window, cx);
                this.tabs
                    .active
                    .editor
                    .replace_selection("draft", window, cx);
                this.tabs.active.editor.select(1..3, window, cx);
                this.set_mode(mode, window, cx);
                (
                    this.tabs.active.id,
                    this.tabs.active.editor.id(),
                    this.tabs.active.editor.selection(cx),
                )
            })
        });
        cx.refresh().unwrap();
        cx.run_until_parked();
        cx.update(|window, cx| notes.update(cx, |this, cx| this.open_settings(window, cx)));
        cx.refresh().unwrap();
        assert!(cx.debug_bounds("dialog-surface").is_some());
        cx.simulate_input("must not edit document");
        intent(&panel(&notes, cx), SettingsIntent::CenteredEditor(true), cx);
        input(&panel(&notes, cx), Field::EditorWidth, "750", cx);
        intent(
            &panel(&notes, cx),
            SettingsIntent::CenteredEditor(false),
            cx,
        );
        let form = panel(&notes, cx);
        intent(&form, SettingsIntent::Close, cx);
        cx.refresh().unwrap();
        cx.update(|window, cx| {
            notes.update(cx, |this, cx| {
                assert_eq!(this.tabs.active.id, tab);
                assert_eq!(this.tabs.active.editor.id(), editor);
                assert_eq!(this.tabs.active.mode, mode);
                assert_eq!(this.tabs.active.editor.text(cx).as_ref(), "draft");
                assert_eq!(this.tabs.active.editor.selection(cx), selection);
                assert!(this.dirty(cx));
                if mode == ViewMode::Reading {
                    assert!(this.focus.is_focused(window));
                } else {
                    assert!(this.tabs.active.editor.is_focused(window, cx));
                }
                this.set_mode(ViewMode::Source, window, cx);
            })
        });
        cx.refresh().unwrap();
        cx.dispatch_action(gpui_component::input::Undo);
        cx.run_until_parked();
        cx.update(|_, cx| {
            assert_eq!(
                notes.read(cx).tabs.active.editor.text(cx).as_ref(),
                "original"
            )
        });
    }
}

#[gpui::test]
fn modal_settings_blocks_document_navigation_and_drops(cx: &mut TestAppContext) {
    let (fixture, notes, cx) = fixture(cx, true, false);
    let form = panel(&notes, cx);
    let (tab, editor) = cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            let tab = this.tabs.active.id;
            let editor = this.tabs.active.editor.id();
            this.new_tab(window, cx);
            this.close_tab(tab, window, cx);
            this.request(crate::Next::Open, window, cx);
            this.request(crate::Next::File(fixture.standalone.clone()), window, cx);
            this.request(
                crate::Next::Notebook {
                    path: fixture.second.root.clone(),
                    create: false,
                },
                window,
                cx,
            );
            this.set_mode(crate::ViewMode::Reading, window, cx);
            this.search_current_file(window, cx);
            this.open_search_all(window, cx);
            this.drop_files(vec![fixture.standalone.clone()], window, cx);
            this.save(crate::Next::Nothing, window, cx);
            assert!(!this.is_busy());
            assert!(!this.can_edit_document());
            assert!(this.search.is_none());
            (tab, editor)
        })
    });
    cx.dispatch_action(crate::Open);
    cx.dispatch_action(crate::SearchAllFiles);
    cx.dispatch_action(crate::ReadingView);
    cx.dispatch_action(crate::ReplaceFile);
    cx.run_until_parked();
    assert!(!cx.has_pending_prompt());
    cx.update(|_, cx| {
        let this = notes.read(cx);
        assert_eq!(this.tabs.active.id, tab);
        assert_eq!(this.tabs.ids().len(), 1);
        assert_eq!(this.tabs.active.editor.id(), editor);
        assert_eq!(this.tabs.active.editor.text(cx).as_ref(), "original");
        assert!(this.tabs.active.mode != crate::ViewMode::Reading);
        assert_eq!(
            this.notebooks.active.as_ref().unwrap().root,
            fixture.first.root
        );
    });
    intent(&form, SettingsIntent::Close, cx);
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.new_tab(window, cx);
            assert_ne!(this.tabs.active.id, tab);
        })
    });
}

#[gpui::test]
fn modal_settings_window_close_cancel_retains_modal_and_draft(cx: &mut TestAppContext) {
    let (_fixture, notes, cx) = fixture(cx, false, true);
    let form = panel(&notes, cx);
    intent(&form, SettingsIntent::Close, cx);
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.tabs
                .active
                .editor
                .replace_selection("draft", window, cx);
            this.open_settings(window, cx);
        })
    });
    let form = panel(&notes, cx);
    cx.run_until_parked();
    assert!(!cx.simulate_close());
    cx.run_until_parked();
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    assert_eq!(panel(&notes, cx), form);
    cx.update(|_, cx| {
        assert!(!notes.read(cx).is_busy());
        assert!(notes.read(cx).tabs.active.editor.text(cx).contains("draft"));
    });
}

#[gpui::test]
fn modal_settings_escape_dismisses_font_picker_before_modal(cx: &mut TestAppContext) {
    use gpui::{Modifiers, MouseButton};
    let (_fixture, notes, cx) = fixture(cx, false, true);
    let form = panel(&notes, cx);
    let picker = cx.update(|_, cx| {
        form.update(cx, |form, cx| {
            form.select_section(super::Section::Font, cx);
            form.fonts[0].clone()
        })
    });
    cx.refresh().unwrap();
    let bounds = cx
        .debug_bounds(format!("select-{}", picker.entity_id()).leak())
        .unwrap();
    cx.simulate_mouse_down(bounds.center(), MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_up(bounds.center(), MouseButton::Left, Modifiers::default());
    cx.run_until_parked();
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert_eq!(panel(&notes, cx), form);
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    cx.update(|_, cx| assert!(notes.read(cx).settings_panel.is_none()));
}

#[gpui::test]
fn modal_settings_restores_empty_and_image_content(cx: &mut TestAppContext) {
    let (fixture, notes, cx) = fixture(cx, true, false);
    let form = panel(&notes, cx);
    intent(&form, SettingsIntent::Close, cx);
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.new_tab(window, cx);
            this.open_settings(window, cx);
        })
    });
    let form = panel(&notes, cx);
    let tab = cx.update(|_, cx| notes.read(cx).tabs.active.id);
    intent(&form, SettingsIntent::Close, cx);
    cx.update(|window, cx| {
        let this = notes.read(cx);
        assert_eq!(this.tabs.active.id, tab);
        assert!(matches!(
            this.tabs.active.session.content(),
            crate::Content::Empty
        ));
        assert!(this.focus.is_focused(window));
    });
    // Content ownership is independent of whether the image decoder succeeds.
    let path = fixture.first.root.join("image.png");
    fs::write(&path, b"image fixture").unwrap();
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.request(crate::Next::File(path.clone()), window, cx)
        })
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            assert_eq!(this.image_path(), Some(path.as_path()));
            this.open_settings(window, cx);
        })
    });
    let form = panel(&notes, cx);
    let tab = cx.update(|_, cx| notes.read(cx).tabs.active.id);
    intent(&form, SettingsIntent::Close, cx);
    cx.update(|window, cx| {
        let this = notes.read(cx);
        assert_eq!(this.tabs.active.id, tab);
        assert_eq!(this.image_path(), Some(path.as_path()));
        assert!(this.focus.is_focused(window));
    });
}

#[cfg(feature = "editor-diagnostics")]
#[gpui::test]
fn modal_settings_retains_editor_and_reading_scroll(cx: &mut TestAppContext) {
    let (_fixture, notes, cx) = fixture(cx, false, true);
    intent(&panel(&notes, cx), SettingsIntent::Close, cx);
    let editor = cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.set_mode(crate::ViewMode::Source, window, cx);
            this.tabs.active.editor.select(0..8, window, cx);
            this.tabs
                .active
                .editor
                .replace_selection(&"Paragraph\n\n".repeat(150), window, cx);
            this.tabs.active.editor.diagnostic_input().clone()
        })
    });
    cx.refresh().unwrap();
    cx.run_until_parked();
    cx.update(|_, cx| editor.update(cx, |input, cx| input.diagnostic_scroll(-300., cx)));
    cx.refresh().unwrap();
    let offset = cx.update(|_, cx| editor.read(cx).diagnostic_input_paint().2);
    assert!(offset.y < gpui::px(0.));
    cx.update(|window, cx| notes.update(cx, |this, cx| this.open_settings(window, cx)));
    cx.refresh().unwrap();
    intent(&panel(&notes, cx), SettingsIntent::Close, cx);
    cx.refresh().unwrap();
    cx.update(|_, cx| assert_eq!(editor.read(cx).diagnostic_input_paint().2, offset));
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.set_mode(crate::ViewMode::Reading, window, cx)
        })
    });
    cx.refresh().unwrap();
    cx.run_until_parked();
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
    cx.update(|_, cx| probe.scroll_to(60, cx));
    cx.refresh().unwrap();
    let before = cx.update(|_, cx| probe.snapshot(cx).unwrap().scroll_item);
    assert!(before > 0);
    cx.update(|window, cx| notes.update(cx, |this, cx| this.open_settings(window, cx)));
    cx.refresh().unwrap();
    intent(&panel(&notes, cx), SettingsIntent::Close, cx);
    cx.refresh().unwrap();
    cx.update(|_, cx| assert_eq!(probe.snapshot(cx).unwrap().scroll_item, before));
}

#[gpui::test]
fn modal_settings_drag_resize_and_reopen_keep_clearance(cx: &mut TestAppContext) {
    use gpui::{Modifiers, MouseButton, point, px, size};
    let (_fixture, notes, cx) = fixture(cx, true, false);
    cx.simulate_resize(size(px(1200.), px(900.)));
    cx.refresh().unwrap();
    let centered = cx.debug_bounds("dialog-surface").unwrap();
    assert_eq!(centered.origin, point(px(120.), px(90.)));
    assert_eq!(centered.size, size(px(960.), px(720.)));
    for (destination, expected) in [
        (point(px(-1000.), px(-1000.)), point(px(24.), px(24.))),
        (point(px(3000.), px(3000.)), point(px(216.), px(156.))),
    ] {
        let title = cx.debug_bounds("dialog-title").unwrap().center();
        cx.simulate_mouse_down(title, MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_move(destination, MouseButton::Left, Modifiers::default());
        cx.refresh().unwrap();
        cx.simulate_mouse_up(destination, MouseButton::Left, Modifiers::default());
        cx.refresh().unwrap();
        let bounds = cx.debug_bounds("dialog-surface").unwrap();
        assert_eq!(bounds.origin, expected);
        assert_eq!(bounds.size, centered.size);
    }
    let form = panel(&notes, cx);
    for section in [
        super::Section::General,
        super::Section::Terminal,
        super::Section::Toolbar,
        super::Section::Font,
        super::Section::Templates,
        super::Section::Theme,
    ] {
        cx.update(|_, cx| form.update(cx, |form, cx| form.select_section(section, cx)));
        cx.refresh().unwrap();
        let bounds = cx.debug_bounds("dialog-surface").unwrap();
        assert_eq!(bounds.origin, point(px(216.), px(156.)));
        assert_eq!(bounds.size, centered.size);
    }
    // Increasing the window retains the dragged origin; shrinking clamps it.
    for (viewport, origin) in [
        (size(px(1600.), px(1200.)), point(px(216.), px(156.))),
        (size(px(320.), px(300.)), point(px(40.), px(36.))),
    ] {
        cx.simulate_resize(viewport);
        cx.refresh().unwrap();
        let bounds = cx.debug_bounds("dialog-surface").unwrap();
        assert_eq!(bounds.origin, origin);
        assert_eq!(
            bounds.size,
            size(viewport.width * 0.8, viewport.height * 0.8)
        );
        assert!(bounds.left() >= px(24.) && bounds.top() >= px(24.));
        assert!(bounds.right() <= viewport.width - px(24.));
        assert!(bounds.bottom() <= viewport.height - px(24.));
    }
    intent(&form, SettingsIntent::Close, cx);
    cx.update(|window, cx| notes.update(cx, |this, cx| this.open_settings(window, cx)));
    cx.refresh().unwrap();
    assert_eq!(
        cx.debug_bounds("dialog-surface").unwrap().origin,
        point(px(32.), px(30.))
    );
}

#[gpui::test]
fn settings_palette_and_shortcut_share_modal_owner(cx: &mut TestAppContext) {
    cx.update(|cx| {
        cx.bind_keys(
            crate::help_ui::app_shortcuts()
                .into_iter()
                .map(|s| s.binding),
        )
    });
    let (_fixture, notes, cx) = fixture(cx, false, true);
    let initial = panel(&notes, cx);
    intent(&initial, SettingsIntent::Close, cx);
    cx.refresh().unwrap();
    let (palette_key, settings_key) = if cfg!(target_os = "macos") {
        ("cmd-p", "cmd-,")
    } else {
        ("ctrl-p", "ctrl-,")
    };
    cx.simulate_keystrokes(palette_key);
    cx.run_until_parked();
    cx.simulate_input("Settings");
    cx.run_until_parked();
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    cx.refresh().unwrap();
    cx.update(|window, cx| {
        assert!(notes.read(cx).palette.is_none());
        assert!(window.has_active_dialog(cx));
    });
    let form = panel(&notes, cx);
    assert_ne!(initial, form);
    cx.update(|_, cx| {
        form.update(cx, |form, cx| {
            form.select_section(super::Section::Templates, cx)
        })
    });
    input(&form, Field::Date, "YYYY[", cx);
    cx.refresh().unwrap();
    cx.simulate_keystrokes(settings_key);
    cx.run_until_parked();
    assert_eq!(panel(&notes, cx), form);
    cx.update(|_, cx| {
        assert!(form.read(cx).section == super::Section::Templates);
        assert_eq!(form.read(cx).date.read(cx).value().as_ref(), "YYYY[");
    });
    intent(&form, SettingsIntent::Close, cx);
    cx.refresh().unwrap();
    cx.simulate_keystrokes(settings_key);
    cx.run_until_parked();
    let reopened = panel(&notes, cx);
    assert_ne!(reopened, form);
    intent(&reopened, SettingsIntent::Close, cx);
    cx.update(|window, cx| notes.update(cx, |this, cx| this.open_help(false, window, cx)));
    cx.refresh().unwrap();
    cx.simulate_keystrokes(settings_key);
    cx.run_until_parked();
    cx.update(|window, cx| {
        assert!(window.has_active_dialog(cx));
        assert!(notes.read(cx).settings_panel.is_none());
    });
}

#[gpui::test]
fn centered_editor_form_preserves_width_and_invalid_drafts(cx: &mut TestAppContext) {
    let (_fixture, notes, cx) = fixture(cx, false, true);
    let form = panel(&notes, cx);
    intent(&form, SettingsIntent::CenteredEditor(true), cx);
    input(&form, Field::EditorWidth, "900", cx);
    cx.update(|_, cx| assert_eq!(notes.read(cx).appearance().centered_editor_width, 900));
    for value in ["", "199", "4001", "abc", "700px", "700.5"] {
        input(&form, Field::EditorWidth, value, cx);
        cx.update(|_, cx| {
            assert_eq!(notes.read(cx).appearance().centered_editor_width, 900);
            assert!(form.read(cx).editor_width_error.is_some());
            assert_eq!(form.read(cx).editor_width.read(cx).value().as_ref(), value);
        });
    }
    input(&form, Field::EditorWidth, "800", cx);
    intent(&form, SettingsIntent::CenteredEditor(false), cx);
    cx.update(|_, cx| {
        assert!(!notes.read(cx).appearance().centered_editor);
        assert!(form.read(cx).editor_width_error.is_none());
        assert_eq!(form.read(cx).editor_width.read(cx).value().as_ref(), "800");
    });
    intent(&form, SettingsIntent::CenteredEditor(true), cx);
    intent(&form, SettingsIntent::Close, cx);
    cx.update(|window, cx| notes.update(cx, |this, cx| this.open_settings(window, cx)));
    let fresh = panel(&notes, cx);
    intent(&form, SettingsIntent::CenteredEditorWidth(1000), cx);
    cx.update(|_, cx| {
        assert_eq!(notes.read(cx).session_appearance.centered_editor_width, 800);
        assert_eq!(fresh.read(cx).editor_width.read(cx).value().as_ref(), "800");
    });
}

#[gpui::test]
fn centered_editor_persists_and_rejects_wrong_scope_and_busy_events(cx: &mut TestAppContext) {
    let (fixture, notes, cx) = fixture(cx, true, false);
    cx.update(|_, cx| {
        notes.update(cx, |this, _| {
            this.notebooks
                .set_test_persistence_target(fixture._directory.path().join("index.json"));
        })
    });
    let form = panel(&notes, cx);
    intent(&form, SettingsIntent::CenteredEditor(true), cx);
    input(&form, Field::EditorWidth, "850", cx);
    let saved = notebook::Notebook::open(&fixture.first.root, false)
        .unwrap()
        .settings
        .appearance;
    assert!(saved.centered_editor);
    assert_eq!(saved.centered_editor_width, 850);
    assert!(
        !notebook::Notebook::open(&fixture.second.root, false)
            .unwrap()
            .settings
            .appearance
            .centered_editor
    );
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            assert!(!this.session_appearance.centered_editor);
            this.notebooks.active = Some(fixture.second.clone());
            this.handle_settings_intent(
                form.entity_id(),
                SettingsIntent::CenteredEditor(true),
                window,
                cx,
            );
            assert!(!this.appearance().centered_editor);
            assert!(this.settings_panel.is_none());
            this.open_settings(window, cx);
        })
    });
    let form = panel(&notes, cx);
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            let _ = this.session.pick();
            this.handle_settings_intent(
                form.entity_id(),
                SettingsIntent::CenteredEditorWidth(900),
                window,
                cx,
            );
            this.handle_settings_intent(
                form.entity_id(),
                SettingsIntent::CenteredEditor(true),
                window,
                cx,
            );
            assert_eq!(this.appearance().centered_editor_width, 700);
            assert!(!this.appearance().centered_editor);
            assert!(form.read(cx).values.busy);
        })
    });
}

#[gpui::test]
fn centered_editor_layout_fits_all_modes_and_sidebar_sizes(cx: &mut TestAppContext) {
    use crate::ViewMode;
    use gpui::{px, size};
    let (_fixture, notes, cx) = fixture(cx, true, false);
    intent(&panel(&notes, cx), SettingsIntent::CenteredEditor(true), cx);
    intent(&panel(&notes, cx), SettingsIntent::Close, cx);
    for mode in [ViewMode::Source, ViewMode::LivePreview, ViewMode::Reading] {
        cx.update(|window, cx| notes.update(cx, |this, cx| this.set_mode(mode, window, cx)));
        for (window_width, sidebar_width, collapsed) in [
            (1400., 280., false),
            (1400., 440., false),
            (650., 280., false),
            (650., 280., true),
        ] {
            cx.simulate_resize(size(px(window_width), px(800.)));
            cx.update(|_, cx| {
                notes.update(cx, |this, cx| {
                    this.sidebar_collapsed = collapsed;
                    this.notebooks
                        .active
                        .as_mut()
                        .unwrap()
                        .settings
                        .sidebar_width = sidebar_width;
                    this.reset_sidebar_resize(cx);
                    cx.notify();
                })
            });
            cx.run_until_parked();
            cx.refresh().unwrap();
            let viewport = cx.debug_bounds("document-viewport").unwrap();
            let pane = cx
                .debug_bounds(if mode == ViewMode::Reading {
                    "reading-text-column-0"
                } else {
                    "editor-text-column"
                })
                .unwrap();
            let scrollbar = cx
                .debug_bounds(if mode == ViewMode::Reading {
                    "reading-scrollbar-viewport"
                } else {
                    "editor-scrollbar-viewport"
                })
                .unwrap();
            assert_eq!(scrollbar.right(), viewport.right(), "{mode:?}");
            // Input's existing horizontal padding is outside its text leaf.
            let padding = if mode == ViewMode::Reading {
                px(0.)
            } else {
                gpui_component::Size::default().input_px() * 2.
            };
            assert_eq!(
                pane.size.width + padding,
                viewport.size.width.min(px(700.)),
                "{mode:?}"
            );
            assert!(
                (pane.origin.x - viewport.origin.x - (viewport.size.width - pane.size.width) / 2.)
                    .abs()
                    < px(1.)
            );
        }
    }
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            let mut appearance = this.appearance();
            appearance.centered_editor = false;
            this.update_appearance(appearance, window, cx);
        })
    });
    cx.simulate_resize(size(px(1400.), px(800.)));
    cx.run_until_parked();
    cx.refresh().unwrap();
    assert_eq!(
        cx.debug_bounds("reading-text-column-0").unwrap().size.width,
        cx.debug_bounds("document-viewport").unwrap().size.width
    );
}

#[gpui::test]
fn centered_editor_scrollbars_and_gutter_wheel_use_full_viewport(cx: &mut TestAppContext) {
    use crate::ViewMode;
    use gpui::{Modifiers, MouseButton, ScrollDelta, ScrollWheelEvent, point, px, size};
    let (_fixture, notes, cx) = fixture(cx, false, true);
    intent(&panel(&notes, cx), SettingsIntent::CenteredEditor(true), cx);
    intent(&panel(&notes, cx), SettingsIntent::Close, cx);
    cx.update(|window, cx| {
        gpui_component::Theme::global_mut(cx).scrollbar_show =
            gpui_component::scroll::ScrollbarShow::Always;
        notes.update(cx, |this, cx| {
            this.editor.select(0..8, window, cx);
            let text = (0..150)
                .map(|i| {
                    format!(
                        "Paragraph {i} with enough text to exercise wrapping and scrolling.\n\n"
                    )
                })
                .collect::<String>();
            this.editor.replace_selection(&text, window, cx);
            this.editor.select(0..0, window, cx);
        });
    });
    cx.simulate_resize(size(px(1400.), px(800.)));
    for mode in [ViewMode::Source, ViewMode::LivePreview, ViewMode::Reading] {
        cx.update(|window, cx| notes.update(cx, |this, cx| this.set_mode(mode, window, cx)));
        cx.run_until_parked();
        cx.refresh().unwrap();
        let viewport = cx.debug_bounds("document-viewport").unwrap();
        let scroll = cx
            .debug_bounds(if mode == ViewMode::Reading {
                "reading-scrollbar-viewport"
            } else {
                "editor-scrollbar-viewport"
            })
            .unwrap();
        assert_eq!(scroll.right(), viewport.right());
        let position = |cx: &mut VisualTestContext| {
            cx.update(|_, cx| {
                let this = notes.read(cx);
                if mode == ViewMode::Reading {
                    this.reading_view
                        .as_ref()
                        .unwrap()
                        .diagnostic_probe()
                        .snapshot(cx)
                        .unwrap()
                        .scroll_item as f32
                } else {
                    -f32::from(
                        this.editor
                            .diagnostic_input()
                            .read(cx)
                            .diagnostic_input_paint()
                            .2
                            .y,
                    )
                }
            })
        };
        let before = position(cx);
        cx.simulate_event(ScrollWheelEvent {
            position: point(scroll.right() - px(50.), scroll.center().y),
            delta: ScrollDelta::Pixels(point(px(0.), px(-180.))),
            ..Default::default()
        });
        cx.refresh().unwrap();
        assert!(position(cx) > before, "gutter wheel in {mode:?}");
        let before = position(cx);
        let bottom = point(scroll.right() - px(8.), scroll.bottom() - px(8.));
        cx.simulate_click(bottom, Modifiers::default());
        cx.refresh().unwrap();
        assert!(position(cx) > before, "right edge track in {mode:?}");
        let top = point(scroll.right() - px(8.), scroll.top() + px(12.));
        cx.simulate_click(top, Modifiers::default());
        cx.refresh().unwrap();
        assert_eq!(position(cx), 0., "track return in {mode:?}");
        cx.simulate_mouse_move(top, None, Modifiers::default());
        cx.simulate_mouse_down(top, MouseButton::Left, Modifiers::default());
        std::thread::sleep(Duration::from_millis(20));
        cx.simulate_mouse_move(bottom, MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_up(bottom, MouseButton::Left, Modifiers::default());
        cx.refresh().unwrap();
        assert!(position(cx) > 0., "right edge thumb drag in {mode:?}");
        // Changing width at a fixed viewport must remeasure Reading's cached
        // item heights without throwing away its logical scroll anchor.
        if mode == ViewMode::Reading {
            let anchor = position(cx);
            cx.update(|window, cx| {
                notes.update(cx, |this, cx| {
                    let mut appearance = this.appearance();
                    appearance.centered_editor_width = 400;
                    this.update_appearance(appearance, window, cx);
                })
            });
            cx.refresh().unwrap();
            assert_eq!(position(cx), anchor);
        }
    }
}

#[gpui::test]
fn external_autosave_setting_applies_with_and_without_a_notebook(cx: &mut TestAppContext) {
    for active in [false, true] {
        let (fixture, notes, cx) = fixture(cx, active, true);
        let form = panel(&notes, cx);
        cx.update(|window, cx| {
            notes.update(cx, |this, cx| {
                assert!(!this.external_autosave_enabled());
                assert!(!form.read(cx).values.autosave_external_files);
                assert!(!form.read(cx).values.has_autosave_notebook);
                this.editor.select(0..8, window, cx);
                this.editor.replace_selection("edited", window, cx);
            })
        });
        cx.run_until_parked();
        cx.executor().advance_clock(Duration::from_secs(2));
        cx.run_until_parked();
        assert_eq!(fs::read_to_string(&fixture.standalone).unwrap(), "original");
        intent(&form, SettingsIntent::ExternalAutosaveEnabled(true), cx);
        cx.update(|_, cx| {
            assert!(notes.read(cx).external_autosave_enabled());
            assert!(form.read(cx).values.autosave_external_files);
        });
        cx.executor().advance_clock(Duration::from_secs(2));
        cx.run_until_parked();
        assert_eq!(fs::read_to_string(&fixture.standalone).unwrap(), "edited");
        intent(&form, SettingsIntent::ExternalAutosaveEnabled(false), cx);
        intent(&form, SettingsIntent::Close, cx);
        intent(&form, SettingsIntent::ExternalAutosaveEnabled(true), cx);
        cx.update(|_, cx| assert!(!notes.read(cx).external_autosave_enabled()));
    }
}

#[gpui::test]
fn external_autosave_form_rejects_a_changed_notebook_scope(cx: &mut TestAppContext) {
    let (fixture, notes, cx) = fixture(cx, true, true);
    let form = panel(&notes, cx);
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            assert!(
                form.read(cx)
                    .values
                    .external_autosave_scope
                    .contains("Saved for A")
            );
            this.notebooks.active = Some(fixture.second.clone());
            this.handle_settings_intent(
                form.entity_id(),
                SettingsIntent::ExternalAutosaveEnabled(true),
                window,
                cx,
            );
            assert!(!this.external_autosave_enabled());
            assert!(this.settings_panel.is_none());
            this.open_settings(window, cx);
        })
    });
    let next = panel(&notes, cx);
    cx.update(|_, cx| {
        assert!(
            next.read(cx)
                .values
                .external_autosave_scope
                .contains("Saved for B")
        )
    });
    intent(&next, SettingsIntent::ExternalAutosaveEnabled(true), cx);
    cx.update(|_, cx| {
        assert!(
            notes
                .read(cx)
                .notebooks
                .active
                .as_ref()
                .unwrap()
                .settings
                .autosave_external_files
        );
        assert!(!notes.read(cx).session_autosave.enabled);
    });
}

#[path = "settings_json_tests.rs"]
mod settings_json;

#[gpui::test]
fn icon_and_secondary_pickers_sync_independently_reset_and_reject_stale_events(
    cx: &mut TestAppContext,
) {
    use gpui_component::{ActiveTheme, color_picker::ColorPickerEvent};
    let (_fixture, notes, cx) = fixture(cx, false, true);
    let old = panel(&notes, cx);
    let (events, _subscription) = watch(&old, cx);
    for (mode, icon, secondary) in [
        (notebook::ThemeChoice::Light, 0x123456, 0x654321),
        (notebook::ThemeChoice::Dark, 0xabcdef, 0xfedcba),
    ] {
        intent(&old, SettingsIntent::Theme(mode), cx);
        for (index, color) in [(4, icon), (5, secondary)] {
            cx.update(|_, cx| {
                old.read(cx).colors[index].clone().update(cx, |_, cx| {
                    cx.emit(ColorPickerEvent::Change(Some(gpui::rgb(color).into())));
                });
            });
            cx.run_until_parked();
        }
        intent(
            &old,
            SettingsIntent::Color(ColorRole::Primary, 0xff0000),
            cx,
        );
        intent(&old, SettingsIntent::Color(ColorRole::Font, 0x00ff00), cx);
        cx.update(|_, cx| {
            let appearance = notes.read(cx).appearance();
            let palette = appearance.palette(false);
            assert_eq!(palette.icon, icon);
            assert_eq!(palette.secondary, secondary);
            assert_eq!(cx.theme().icon_color, Some(gpui::rgb(icon).into()));
            assert_eq!(cx.theme().secondary, gpui::rgb(secondary).into());
            assert_eq!(cx.theme().foreground, gpui::rgb(0x00ff00).into());
        });
    }
    intent(
        &old,
        SettingsIntent::Theme(notebook::ThemeChoice::Light),
        cx,
    );
    cx.update(|_, cx| {
        assert_eq!(
            old.read(cx).colors[4].read(cx).value(),
            Some(gpui::rgb(0x123456).into())
        );
        assert_eq!(
            old.read(cx).colors[5].read(cx).value(),
            Some(gpui::rgb(0x654321).into())
        );
        assert_eq!(events.read(cx).0.len(), 11, "sync must not emit feedback");
    });
    intent(&old, SettingsIntent::ResetPalettes, cx);
    cx.update(|_, cx| {
        let appearance = notes.read(cx).appearance().resolved(false);
        assert_eq!(appearance.light, Some(notebook::ThemePalette::LIGHT));
        assert_eq!(appearance.dark, Some(notebook::ThemePalette::DARK));
        assert_eq!(
            old.read(cx).colors[4].read(cx).value(),
            Some(gpui::rgb(notebook::ThemePalette::LIGHT.icon).into())
        );
        assert_eq!(
            old.read(cx).colors[5].read(cx).value(),
            Some(gpui::rgb(notebook::ThemePalette::LIGHT.secondary).into())
        );
    });
    intent(&old, SettingsIntent::Close, cx);
    for role in [ColorRole::Icon, ColorRole::Secondary] {
        intent(&old, SettingsIntent::Color(role, 0x456789), cx);
    }
    cx.update(|_, cx| {
        assert_eq!(
            notes.read(cx).appearance().palette(false),
            notebook::ThemePalette::LIGHT
        )
    });
}

#[gpui::test]
fn toolbar_visibility_toggle_preserves_customization_and_reset_restores_it(
    cx: &mut TestAppContext,
) {
    let (_fixture, notes, cx) = fixture(cx, false, true);
    let form = panel(&notes, cx);
    // Modal settings blocks the menu adapter.
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.toggle_toolbar(window, cx);
            assert!(this.toolbar_settings().visible);
        })
    });
    intent(&form, SettingsIntent::Close, cx);
    let original = cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.session_toolbar.search = false;
            this.session_toolbar.icon_size = notebook::ToolbarIconSize::Large;
            this.session_toolbar.alignment = notebook::ToolbarAlignment::Center;
            let original = this.session_toolbar;
            this.toggle_toolbar(window, cx);
            assert_eq!(
                this.session_toolbar,
                notebook::ToolbarSettings {
                    visible: false,
                    ..original
                }
            );
            original
        })
    });
    cx.refresh().unwrap();
    // GPUI retains old debug selectors; measure reclaimed content space instead.
    let hidden_top = cx.debug_bounds("document-viewport").unwrap().top();
    cx.update(|window, cx| notes.update(cx, |this, cx| this.toggle_toolbar(window, cx)));
    cx.refresh().unwrap();
    assert!(cx.debug_bounds("document-viewport").unwrap().top() > hidden_top);
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            assert_eq!(this.session_toolbar, original);
            this.toggle_toolbar(window, cx);
            this.open_settings(window, cx);
        })
    });
    let form = panel(&notes, cx);
    intent(&form, SettingsIntent::ResetToolbar, cx);
    cx.refresh().unwrap();
    assert!(cx.debug_bounds("document-viewport").unwrap().top() > hidden_top);
    cx.update(|_, cx| {
        assert_eq!(
            notes.read(cx).session_toolbar,
            notebook::ToolbarSettings::default()
        )
    });
    intent(&form, SettingsIntent::Close, cx);
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            for item in notebook::ToolbarItem::DEFAULT_ORDER {
                this.session_toolbar.set_enabled(item, false);
            }
            this.toggle_toolbar(window, cx);
            this.toggle_toolbar(window, cx);
            assert!(this.session_toolbar.visible);
            let _ = this.session.pick();
            this.toggle_toolbar(window, cx);
            assert!(this.session_toolbar.visible);
        })
    });
    cx.refresh().unwrap();
    assert_eq!(
        cx.debug_bounds("document-viewport").unwrap().top(),
        hidden_top
    );
}

#[gpui::test]
fn toolbar_visibility_persists_only_for_the_active_notebook(cx: &mut TestAppContext) {
    let (fixture, notes, cx) = fixture(cx, true, true);
    let form = panel(&notes, cx);
    intent(&form, SettingsIntent::Close, cx);
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.notebooks
                .set_test_persistence_target(fixture._directory.path().join("index.json"));
            this.toggle_toolbar(window, cx);
            assert!(this.session_toolbar.visible);
        })
    });
    cx.run_until_parked();
    let saved = notebook::Notebook::open(&fixture.first.root, false).unwrap();
    assert!(!saved.settings.toolbar.visible);
    assert!(
        notebook::Notebook::open(&fixture.second.root, false)
            .unwrap()
            .settings
            .toolbar
            .visible
    );
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.notebooks.active = Some(fixture.second.clone());
            assert!(this.toolbar_settings().visible);
            this.notebooks.active = Some(saved);
            assert!(!this.toolbar_settings().visible);
            this.open_settings(window, cx);
        })
    });
    let form = panel(&notes, cx);
    intent(&form, SettingsIntent::ResetToolbar, cx);
    assert!(
        notebook::Notebook::open(&fixture.first.root, false)
            .unwrap()
            .settings
            .toolbar
            .visible
    );
}

#[gpui::test]
fn terminal_shell_settings_persist_per_notebook_and_reject_stale_forms(cx: &mut TestAppContext) {
    use mithril::terminal::WindowsShell;
    let (fixture, notes, cx) = fixture(cx, true, false);
    cx.update(|_, cx| {
        notes.update(cx, |this, _| {
            this.notebooks
                .set_test_persistence_target(fixture._directory.path().join("index.json"));
        })
    });
    let form = panel(&notes, cx);
    intent(
        &form,
        SettingsIntent::TerminalShell(WindowsShell::GitBash),
        cx,
    );
    assert_eq!(
        notebook::Notebook::open(&fixture.first.root, false)
            .unwrap()
            .settings
            .terminal
            .windows_shell,
        WindowsShell::GitBash
    );
    assert_eq!(
        notebook::Notebook::open(&fixture.second.root, false)
            .unwrap()
            .settings
            .terminal
            .windows_shell,
        WindowsShell::Powershell
    );
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            assert!(
                this.terminals.tabs.is_empty(),
                "changing the shell never launches a process"
            );
            this.notebooks.active = Some(fixture.second.clone());
            this.handle_settings_intent(
                form.entity_id(),
                SettingsIntent::TerminalShell(WindowsShell::GitBash),
                window,
                cx,
            );
            assert_eq!(
                this.terminal_settings().windows_shell,
                WindowsShell::Powershell
            );
            assert!(this.settings_panel.is_none());
        })
    });
}

#[gpui::test]
fn terminal_shell_without_notebook_is_session_only(cx: &mut TestAppContext) {
    use mithril::terminal::WindowsShell;
    let (fixture, notes, cx) = fixture(cx, false, true);
    let form = panel(&notes, cx);
    intent(
        &form,
        SettingsIntent::TerminalShell(WindowsShell::GitBash),
        cx,
    );
    cx.update(|_, cx| {
        notes.update(cx, |this, _| {
            assert_eq!(this.session_terminal.windows_shell, WindowsShell::GitBash);
            assert!(this.terminals.tabs.is_empty());
            this.notebooks.active = Some(fixture.first.clone());
            assert_eq!(
                this.terminal_settings().windows_shell,
                WindowsShell::Powershell
            );
            this.notebooks.active = None;
            assert_eq!(
                this.terminal_settings().windows_shell,
                WindowsShell::GitBash
            );
        })
    });
}

#[gpui::test]
fn terminal_shell_dropdown_confirms_and_syncs_effective_selection(cx: &mut TestAppContext) {
    use gpui_component::select::SelectEvent;
    let (_fixture, notes, cx) = fixture(cx, false, true);
    let form = panel(&notes, cx);
    let picker = cx.update(|_, cx| {
        form.update(cx, |form, cx| {
            form.select_section(super::Section::Terminal, cx)
        });
        form.read(cx).terminal_shell.clone()
    });
    cx.update(|_, cx| {
        assert_eq!(picker.read(cx).selected_value(), Some(&"PowerShell"));
        picker.update(cx, |_, cx| cx.emit(SelectEvent::Confirm(Some("Git Bash"))));
    });
    cx.run_until_parked();
    cx.refresh().unwrap();
    cx.update(|_, cx| {
        assert_eq!(
            notes.read(cx).terminal_settings().windows_shell,
            mithril::terminal::WindowsShell::GitBash
        );
        assert_eq!(picker.read(cx).selected_value(), Some(&"Git Bash"));
        assert!(notes.read(cx).terminals.tabs.is_empty());
    });
    for width in [1000., 400.] {
        cx.simulate_resize(gpui::size(gpui::px(width), gpui::px(650.)));
        cx.refresh().unwrap();
        assert!(cx.debug_bounds("settings-navigation").is_some());
        assert_eq!(
            cx.debug_bounds("terminal-shell-picker").is_some(),
            cfg!(windows)
        );
    }
    cx.update(|window, cx| {
        form.update(cx, |form, cx| {
            let mut values = form.values.clone();
            values.terminal.windows_shell = mithril::terminal::WindowsShell::Powershell;
            form.sync_effective(values, window, cx);
        });
        assert_eq!(picker.read(cx).selected_value(), Some(&"PowerShell"));
    });
}

#[gpui::test]
fn terminal_cursor_dropdown_persists_and_rejects_stale_or_busy_forms(cx: &mut TestAppContext) {
    use gpui_component::select::SelectEvent;
    use mithril::terminal::CursorStyle;
    let (fixture, notes, cx) = fixture(cx, true, false);
    cx.update(|_, cx| {
        notes.update(cx, |this, _| {
            this.notebooks
                .set_test_persistence_target(fixture._directory.path().join("index.json"));
        })
    });
    let form = panel(&notes, cx);
    let picker = cx.update(|_, cx| {
        form.update(cx, |form, cx| {
            form.select_section(super::Section::Terminal, cx)
        });
        form.read(cx).terminal_cursor.clone()
    });
    for (style, label) in super::CURSOR_OPTIONS {
        cx.update(|_, cx| picker.update(cx, |_, cx| cx.emit(SelectEvent::Confirm(Some(label)))));
        cx.run_until_parked();
        cx.refresh().unwrap();
        cx.update(|_, cx| {
            assert_eq!(picker.read(cx).selected_value(), Some(&label));
            assert_eq!(notes.read(cx).terminal_settings().cursor_style, style);
            assert!(notes.read(cx).terminals.tabs.is_empty());
        });
        assert_eq!(
            notebook::Notebook::open(&fixture.first.root, false)
                .unwrap()
                .settings
                .terminal
                .cursor_style,
            style
        );
    }
    assert_eq!(
        notebook::Notebook::open(&fixture.second.root, false)
            .unwrap()
            .settings
            .terminal
            .cursor_style,
        CursorStyle::Line
    );
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.notebooks.active = Some(fixture.second.clone());
            this.handle_settings_intent(
                form.entity_id(),
                SettingsIntent::TerminalCursorStyle(CursorStyle::Box),
                window,
                cx,
            );
            assert_eq!(this.terminal_settings().cursor_style, CursorStyle::Line);
            assert!(this.settings_panel.is_none());
            this.open_settings(window, cx);
        })
    });
    let fresh = panel(&notes, cx);
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            let _ = this.session.pick();
            this.handle_settings_intent(
                fresh.entity_id(),
                SettingsIntent::TerminalCursorStyle(CursorStyle::Box),
                window,
                cx,
            );
            assert_eq!(this.terminal_settings().cursor_style, CursorStyle::Line);
            assert!(fresh.read(cx).values.busy);
        })
    });
    let (events, _subscription) = watch(&fresh, cx);
    intent(
        &fresh,
        SettingsIntent::TerminalCursorStyle(CursorStyle::Box),
        cx,
    );
    cx.update(|_, cx| assert!(events.read(cx).0.is_empty()));
}

#[gpui::test]
fn terminal_cursor_updates_retained_hidden_terminal_and_follows_notebook(cx: &mut TestAppContext) {
    use mithril::terminal::CursorStyle;
    let (fixture, notes, cx) = fixture(cx, false, true);
    let form = panel(&notes, cx);
    intent(&form, SettingsIntent::Close, cx);
    cx.update(|window, cx| notes.update(cx, |this, cx| this.toggle_right_panel(window, cx)));
    cx.refresh().unwrap();
    let (terminal, view, launch) = cx.update(|_, cx| {
        let terminal = notes.read(cx).terminals.active_session().cloned().unwrap();
        let view = terminal.read(cx).view.clone().unwrap();
        let launch = terminal.read(cx).last_launch.clone();
        assert_eq!(
            view.read(cx).config().cursor_style,
            gpui_terminal::CursorStyle::Line
        );
        (terminal, view, launch)
    });
    cx.update(|window, cx| notes.update(cx, |this, cx| this.open_settings(window, cx)));
    let form = panel(&notes, cx);
    intent(
        &form,
        SettingsIntent::TerminalCursorStyle(CursorStyle::Box),
        cx,
    );
    cx.refresh().unwrap();
    cx.update(|_, cx| {
        assert_eq!(
            view.read(cx).config().cursor_style,
            gpui_terminal::CursorStyle::Box
        )
    });
    intent(&form, SettingsIntent::Close, cx);
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.toggle_right_panel(window, cx);
            this.open_settings(window, cx);
        })
    });
    let form = panel(&notes, cx);
    intent(
        &form,
        SettingsIntent::TerminalCursorStyle(CursorStyle::Underline),
        cx,
    );
    cx.refresh().unwrap();
    cx.update(|_, cx| {
        assert_eq!(
            view.read(cx).config().cursor_style,
            gpui_terminal::CursorStyle::Underline
        );
        assert!(!notes.read(cx).right_panel_open);
        assert_eq!(
            notes.read(cx).session_terminal.cursor_style,
            CursorStyle::Underline
        );
        assert_eq!(terminal.read(cx).view.as_ref(), Some(&view));
        assert_eq!(terminal.read(cx).last_launch, launch);
    });
    intent(&form, SettingsIntent::Close, cx);
    cx.update(|_, cx| {
        notes.update(cx, |this, cx| {
            this.notebooks.active = Some(fixture.first.clone());
            cx.notify();
        })
    });
    cx.refresh().unwrap();
    cx.update(|_, cx| {
        assert_eq!(
            view.read(cx).config().cursor_style,
            gpui_terminal::CursorStyle::Line
        )
    });
    cx.update(|_, cx| {
        notes.update(cx, |this, cx| {
            this.notebooks.active = None;
            cx.notify();
        })
    });
    cx.refresh().unwrap();
    cx.update(|_, cx| {
        assert_eq!(
            view.read(cx).config().cursor_style,
            gpui_terminal::CursorStyle::Underline
        )
    });
}

#[gpui::test]
fn panel_button_settings_apply_to_session_without_closing_panels(cx: &mut TestAppContext) {
    let (_fixture, notes, cx) = fixture(cx, false, true);
    let form = panel(&notes, cx);
    intent(&form, SettingsIntent::Close, cx);
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.toggle_right_panel(window, cx);
            this.session_toolbar.visible = false;
            this.open_settings(window, cx);
        })
    });
    let form = panel(&notes, cx);
    toolbar_section(&form, cx);
    intent(&form, SettingsIntent::TitleBarTerminal(true), cx);
    intent(&form, SettingsIntent::TitleBarSidebar(false), cx);
    intent(
        &form,
        SettingsIntent::ToolbarItemEnabled(notebook::ToolbarItem::Terminal, false),
        cx,
    );
    cx.update(|_, cx| {
        let this = notes.read(cx);
        assert!(!this.session_toolbar.title_bar_sidebar);
        assert!(this.session_toolbar.title_bar_terminal);
        assert!(!this.session_toolbar.terminal);
        assert!(!this.session_toolbar.visible);
        assert!(this.right_panel_open);
        assert_eq!(this.terminals.tabs.len(), 1);
        assert_eq!(form.read(cx).values.toolbar, this.session_toolbar);
    });
    intent(&form, SettingsIntent::Close, cx);
    cx.update(|window, cx| notes.update(cx, |this, cx| this.open_settings(window, cx)));
    intent(&form, SettingsIntent::TitleBarTerminal(false), cx);
    cx.update(|_, cx| assert!(notes.read(cx).session_toolbar.title_bar_terminal));
    let reopened = panel(&notes, cx);
    intent(&reopened, SettingsIntent::ResetToolbar, cx);
    cx.update(|_, cx| {
        assert_eq!(
            notes.read(cx).session_toolbar,
            notebook::ToolbarSettings::default()
        )
    });
}

#[gpui::test]
fn terminal_font_controls_persist_reset_and_guard_scope(cx: &mut TestAppContext) {
    use gpui_component::select::SelectEvent;
    let (fixture, notes, cx) = fixture(cx, true, false);
    cx.update(|_, cx| {
        notes.update(cx, |this, _| {
            this.notebooks
                .set_test_persistence_target(fixture._directory.path().join("index.json"));
        })
    });
    let form = panel(&notes, cx);
    let picker = cx.update(|_, cx| {
        form.update(cx, |form, cx| {
            form.select_section(super::Section::Terminal, cx)
        });
        form.read(cx).terminal_font.clone()
    });
    cx.update(|_, cx| {
        picker.update(cx, |_, cx| {
            cx.emit(SelectEvent::Confirm(Some(
                "Saved unavailable font".to_owned(),
            )))
        })
    });
    intent(&form, SettingsIntent::TerminalFontSizeStep(1), cx);
    cx.run_until_parked();
    cx.refresh().unwrap();
    let saved = notebook::Notebook::open(&fixture.first.root, false)
        .unwrap()
        .settings
        .terminal;
    assert_eq!(saved.font_family.as_deref(), Some("Saved unavailable font"));
    assert_eq!(saved.font_size, Some(17));
    cx.update(|_, cx| {
        assert_eq!(
            picker.read(cx).selected_value().map(String::as_str),
            Some("Saved unavailable font")
        );
        assert!(notes.read(cx).terminals.tabs.is_empty());
    });
    intent(&form, SettingsIntent::TerminalFontSizeReset, cx);
    cx.update(|_, cx| {
        let settings = notes.read(cx).terminal_settings();
        assert_eq!(settings.font_size, None);
        assert_eq!(settings.font_family, saved.font_family);
    });
    intent(&form, SettingsIntent::TerminalFontSizeStep(1), cx);
    cx.update(|_, cx| {
        picker.update(cx, |_, cx| {
            cx.emit(SelectEvent::Confirm(Some(
                super::FOLLOW_EDITOR_FONT.to_owned(),
            )))
        })
    });
    cx.update(|_, cx| {
        let settings = notes.read(cx).terminal_settings();
        assert_eq!(settings.font_size, Some(17));
        assert_eq!(settings.font_family, None);
        assert_eq!(
            picker.read(cx).selected_value().map(String::as_str),
            Some(super::FOLLOW_EDITOR_FONT)
        );
    });
    // Steps clamp at both ends, independently of editor content size.
    for _ in 0..50 {
        intent(&form, SettingsIntent::TerminalFontSizeStep(1), cx);
    }
    cx.update(|_, cx| assert_eq!(notes.read(cx).terminal_settings().font_size, Some(40)));
    for _ in 0..50 {
        intent(&form, SettingsIntent::TerminalFontSizeStep(-1), cx);
    }
    cx.update(|_, cx| assert_eq!(notes.read(cx).terminal_settings().font_size, Some(10)));
    cx.run_until_parked();
    assert_eq!(
        notebook::Notebook::open(&fixture.first.root, false)
            .unwrap()
            .settings
            .terminal
            .font_size,
        Some(10)
    );
    assert_eq!(
        notebook::Notebook::open(&fixture.second.root, false)
            .unwrap()
            .settings
            .terminal
            .font_size,
        None
    );
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.notebooks.active = Some(fixture.second.clone());
            this.handle_settings_intent(
                form.entity_id(),
                SettingsIntent::TerminalFontSizeStep(1),
                window,
                cx,
            );
            assert_eq!(this.terminal_settings().font_size, None);
            assert!(this.settings_panel.is_none());
            this.open_settings(window, cx);
        })
    });
    let fresh = panel(&notes, cx);
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            let _ = this.session.pick();
            for intent in [
                SettingsIntent::TerminalFont(Some("Blocked".into())),
                SettingsIntent::TerminalFontSizeStep(1),
                SettingsIntent::TerminalFontSizeReset,
            ] {
                this.handle_settings_intent(fresh.entity_id(), intent, window, cx);
            }
            assert_eq!(this.terminal_settings(), Default::default());
            assert!(fresh.read(cx).values.busy);
        })
    });
    let (events, _subscription) = watch(&fresh, cx);
    intent(&fresh, SettingsIntent::TerminalFontSizeStep(1), cx);
    intent(
        &fresh,
        SettingsIntent::TerminalFont(Some("Blocked".into())),
        cx,
    );
    cx.update(|_, cx| assert!(events.read(cx).0.is_empty()));
}

#[gpui::test]
fn terminal_fonts_update_all_sessions_and_follow_notebook_scope(cx: &mut TestAppContext) {
    use gpui_component::ActiveTheme;
    let (fixture, notes, cx) = fixture(cx, false, true);
    let form = panel(&notes, cx);
    intent(&form, SettingsIntent::Close, cx);
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.installed_fonts = vec!["Terminal Mono".into(), "Editor Mono".into()];
            let mut appearance = this.appearance();
            appearance.monospace_font = Some("Editor Mono".into());
            appearance.content_font_size = 19;
            this.update_appearance(appearance, window, cx);
            this.new_terminal_tab(window, cx);
            this.new_terminal_tab(window, cx);
            this.open_settings(window, cx);
        })
    });
    let form = panel(&notes, cx);
    let sessions = cx.update(|_, cx| {
        notes
            .read(cx)
            .terminals
            .tabs
            .iter()
            .map(|tab| {
                let session = tab.session.clone();
                let view = session.read(cx).view.clone().unwrap();
                let launch = session.read(cx).last_launch.clone();
                assert_eq!(view.read(cx).config().font_family, "Editor Mono");
                assert_eq!(view.read(cx).config().font_size, gpui::px(19.));
                (session, view, launch)
            })
            .collect::<Vec<_>>()
    });
    intent(
        &form,
        SettingsIntent::TerminalFont(Some("terminal mono".into())),
        cx,
    );
    intent(&form, SettingsIntent::TerminalFontSizeStep(1), cx);
    cx.refresh().unwrap();
    cx.update(|_, cx| {
        for (_, view, _) in &sessions {
            assert_eq!(view.read(cx).config().font_family, "Terminal Mono");
            assert_eq!(view.read(cx).config().font_size, gpui::px(20.));
        }
        assert_eq!(notes.read(cx).session_terminal.font_size, Some(20));
        assert_eq!(cx.theme().mono_font_family.as_ref(), "Editor Mono");
    });
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            let mut appearance = this.appearance();
            appearance.content_font_size = 21;
            this.update_appearance(appearance, window, cx);
        })
    });
    cx.refresh().unwrap();
    cx.update(|_, cx| {
        for (_, view, _) in &sessions {
            assert_eq!(view.read(cx).config().font_size, gpui::px(20.));
        }
    });
    intent(&form, SettingsIntent::Close, cx);
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.toggle_right_panel(window, cx);
            this.open_settings(window, cx);
        })
    });
    let form = panel(&notes, cx);
    intent(
        &form,
        SettingsIntent::TerminalFont(Some("Unavailable".into())),
        cx,
    );
    intent(&form, SettingsIntent::TerminalFontSizeReset, cx);
    cx.refresh().unwrap();
    cx.update(|_, cx| {
        assert!(!notes.read(cx).right_panel_open);
        for (session, view, launch) in &sessions {
            assert_eq!(view.read(cx).config().font_family, "Editor Mono");
            assert_eq!(view.read(cx).config().font_size, gpui::px(21.));
            assert_eq!(session.read(cx).view.as_ref(), Some(view));
            assert_eq!(&session.read(cx).last_launch, launch);
        }
        assert_eq!(
            notes.read(cx).session_terminal.font_family.as_deref(),
            Some("Unavailable")
        );
    });
    intent(&form, SettingsIntent::Close, cx);
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.notebooks.active = Some(fixture.first.clone());
            this.apply_appearance(window, cx);
            cx.notify();
        })
    });
    cx.refresh().unwrap();
    cx.update(|_, cx| {
        for (_, view, _) in &sessions {
            assert_eq!(view.read(cx).config().font_size, gpui::px(16.));
            assert_eq!(
                view.read(cx).config().font_family,
                cx.theme().mono_font_family.to_string()
            );
        }
    });
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.notebooks.active = None;
            this.apply_appearance(window, cx);
            cx.notify();
        })
    });
    cx.refresh().unwrap();
    cx.update(|_, cx| {
        for (_, view, _) in &sessions {
            assert_eq!(view.read(cx).config().font_size, gpui::px(21.));
            assert_eq!(view.read(cx).config().font_family, "Editor Mono");
        }
    });
}

#[gpui::test]
fn terminal_font_controls_fit_narrow_and_wide_dialogs(cx: &mut TestAppContext) {
    let (_fixture, notes, cx) = fixture(cx, false, true);
    let form = panel(&notes, cx);
    cx.update(|_, cx| {
        form.update(cx, |form, cx| {
            form.select_section(super::Section::Terminal, cx)
        })
    });
    for width in [500., 1200.] {
        cx.simulate_resize(gpui::size(gpui::px(width), gpui::px(1000.)));
        cx.refresh().unwrap();
        let content = cx.debug_bounds("settings-content").unwrap();
        let font = cx.debug_bounds("terminal-font-picker").unwrap();
        assert!(font.size.width > gpui::px(100.));
        assert!(
            font.left() >= content.left() && font.right() <= content.right(),
            "{font:?} outside {content:?}"
        );
    }
}

#[gpui::test]
fn footer_preferences_preserve_hidden_choices_and_reopened_session_form(cx: &mut TestAppContext) {
    let (_fixture, notes, cx) = fixture(cx, false, true);
    let form = panel(&notes, cx);
    intent(&form, SettingsIntent::FooterWordCount(false), cx);
    intent(
        &form,
        SettingsIntent::FooterCountMode(notebook::FooterCountMode::ReadableText),
        cx,
    );
    intent(&form, SettingsIntent::FooterVisible(false), cx);
    let expected = notebook::FooterSettings {
        visible: false,
        word_count: false,
        character_count: true,
        count_mode: notebook::FooterCountMode::ReadableText,
    };
    // Disabled child controls cannot discard choices while the footer is hidden.
    intent(&form, SettingsIntent::FooterWordCount(true), cx);
    intent(&form, SettingsIntent::FooterCharacterCount(false), cx);
    intent(
        &form,
        SettingsIntent::FooterCountMode(notebook::FooterCountMode::Source),
        cx,
    );
    cx.update(|_, cx| {
        assert_eq!(notes.read(cx).footer_settings(), expected);
        assert_eq!(form.read(cx).values.footer, expected);
    });
    intent(&form, SettingsIntent::Close, cx);
    cx.update(|window, cx| notes.update(cx, |this, cx| this.open_settings(window, cx)));
    let reopened = panel(&notes, cx);
    cx.update(|_, cx| assert_eq!(reopened.read(cx).values.footer, expected));
    intent(&reopened, SettingsIntent::FooterVisible(true), cx);
    intent(&reopened, SettingsIntent::FooterCharacterCount(false), cx);
    // An enabled footer can have no count items without resetting the mode.
    cx.update(|_, cx| {
        assert_eq!(
            notes.read(cx).session_footer,
            notebook::FooterSettings {
                visible: true,
                character_count: false,
                ..expected
            }
        );
    });
    intent(&form, SettingsIntent::FooterVisible(false), cx);
    cx.update(|_, cx| assert!(notes.read(cx).footer_settings().visible));
}

#[gpui::test]
fn footer_preferences_persist_only_for_the_active_notebook_and_reject_stale_scope(
    cx: &mut TestAppContext,
) {
    let (fixture, notes, cx) = fixture(cx, true, true);
    cx.update(|_, cx| {
        notes.update(cx, |this, _| {
            this.notebooks
                .set_test_persistence_target(fixture._directory.path().join("index.json"));
        })
    });
    let form = panel(&notes, cx);
    intent(&form, SettingsIntent::FooterCharacterCount(false), cx);
    intent(
        &form,
        SettingsIntent::FooterCountMode(notebook::FooterCountMode::ReadableText),
        cx,
    );
    intent(&form, SettingsIntent::FooterVisible(false), cx);
    let expected = notebook::FooterSettings {
        visible: false,
        character_count: false,
        count_mode: notebook::FooterCountMode::ReadableText,
        ..notebook::FooterSettings::default()
    };
    assert_eq!(
        notebook::Notebook::open(&fixture.first.root, false)
            .unwrap()
            .settings
            .footer,
        expected
    );
    assert_eq!(
        notebook::Notebook::open(&fixture.second.root, false)
            .unwrap()
            .settings
            .footer,
        notebook::FooterSettings::default()
    );
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            assert_eq!(this.session_footer, notebook::FooterSettings::default());
            this.notebooks.active = Some(fixture.second.clone());
            this.handle_settings_intent(
                form.entity_id(),
                SettingsIntent::FooterVisible(false),
                window,
                cx,
            );
            assert_eq!(this.footer_settings(), notebook::FooterSettings::default());
            assert!(this.settings_panel.is_none());
            this.notebooks.active =
                Some(notebook::Notebook::open(&fixture.first.root, false).unwrap());
            this.open_settings(window, cx);
        });
    });
    let reopened = panel(&notes, cx);
    cx.update(|_, cx| assert_eq!(reopened.read(cx).values.footer, expected));
}

#[gpui::test]
fn footer_form_and_adapter_reject_changes_while_busy(cx: &mut TestAppContext) {
    let (_fixture, notes, cx) = fixture(cx, false, true);
    let form = panel(&notes, cx);
    let choices = [
        SettingsIntent::FooterVisible(false),
        SettingsIntent::FooterWordCount(false),
        SettingsIntent::FooterCharacterCount(false),
        SettingsIntent::FooterCountMode(notebook::FooterCountMode::ReadableText),
    ];
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            let _ = this.session.pick();
            for intent in choices.clone() {
                this.handle_settings_intent(form.entity_id(), intent, window, cx);
            }
            assert_eq!(this.footer_settings(), notebook::FooterSettings::default());
            assert!(form.read(cx).values.busy);
        })
    });
    let (events, _subscription) = watch(&form, cx);
    for choice in choices {
        intent(&form, choice, cx);
    }
    cx.update(|_, cx| assert!(events.read(cx).0.is_empty()));
}
