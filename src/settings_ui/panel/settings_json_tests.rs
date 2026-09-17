//! Settings JSON navigation through the real modal and document adapters.
use super::SettingsIntent;
use super::{fixture, intent, panel};
use crate::{Next, ViewMode, notebook};
use gpui::TestAppContext;
use gpui_component::WindowExt;
use std::{fs, time::Duration};

#[gpui::test]
fn settings_json_button_opens_source_and_preserves_notebook_preferences(cx: &mut TestAppContext) {
    let (fixture, notes, cx) = fixture(cx, true, false);
    let path = notebook::settings_path(&fixture.first.root);
    intent(&panel(&notes, cx), SettingsIntent::Close, cx);
    cx.update(|_, cx| {
        notes.update(cx, |this, _| {
            this.notebooks
                .set_test_persistence_target(fixture._directory.path().join("index.json"));
        })
    });
    for mode in [ViewMode::LivePreview, ViewMode::Reading] {
        cx.update(|window, cx| {
            notes.update(cx, |this, cx| {
                this.request(Next::File(fixture.first.root.join("note.md")), window, cx);
            })
        });
        cx.run_until_parked();
        let tab = cx.update(|window, cx| {
            notes.update(cx, |this, cx| {
                this.set_mode(mode, window, cx);
                this.open_settings(window, cx);
                this.tabs.active.id
            })
        });
        cx.run_until_parked();
        let before = fs::read(&path).unwrap();
        let before_identity = mithril::document::open(&path).unwrap().version;
        cx.refresh().unwrap();
        let button = cx.debug_bounds("open-settings-json").unwrap();
        cx.simulate_click(button.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        cx.update(|window, cx| {
            let this = notes.read(cx);
            assert_eq!(this.tabs.active.path(), Some(path.as_path()));
            assert_eq!(this.tabs.active.id, tab);
            assert_eq!(this.tabs.active.mode, ViewMode::Source);
            assert!(this.tabs.active.editor.is_focused(window, cx));
            assert_eq!(
                this.tabs.active.editor.text(cx).as_ref(),
                String::from_utf8(before.clone()).unwrap()
            );
            assert!(this.settings_panel.is_none());

            let settings = &this.notebooks.active.as_ref().unwrap().settings;
            assert_eq!(settings.view_mode, mode);
            assert_eq!(
                settings.last_file.as_deref(),
                Some(std::path::Path::new("note.md"))
            );
        });
        assert_eq!(fs::read(&path).unwrap(), before);
        assert_eq!(
            mithril::document::open(&path).unwrap().version,
            before_identity
        );
    }
}

#[gpui::test]
fn settings_json_disabled_without_notebook_and_rejects_busy_or_stale_forms(
    cx: &mut TestAppContext,
) {
    let (fixture, notes, cx) = fixture(cx, false, true);
    let form = panel(&notes, cx);
    cx.update(|_, cx| assert!(!form.read(cx).values.has_settings_file));
    intent(&form, SettingsIntent::OpenSettingsJson, cx);
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.handle_settings_intent(
                form.entity_id(),
                SettingsIntent::OpenSettingsJson,
                window,
                cx,
            );
            assert_eq!(
                this.tabs.active.path(),
                Some(fs::canonicalize(&fixture.standalone).unwrap().as_path())
            );
            assert!(this.settings_panel.is_some());
            this.close_settings_panel(form.entity_id(), window, cx);
            this.notebooks.active = Some(fixture.first.clone());
            this.open_settings(window, cx);
            // A different form's intent has no authority over the new dialog.
            this.handle_settings_intent(
                form.entity_id(),
                SettingsIntent::OpenSettingsJson,
                window,
                cx,
            );
            assert!(this.settings_panel.is_some());
        })
    });
    let form = panel(&notes, cx);
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.notebooks.active = Some(fixture.second.clone());
            this.handle_settings_intent(
                form.entity_id(),
                SettingsIntent::OpenSettingsJson,
                window,
                cx,
            );
            assert!(this.settings_panel.is_none());
            assert_eq!(
                this.tabs.active.path(),
                Some(fs::canonicalize(&fixture.standalone).unwrap().as_path())
            );
            this.open_settings(window, cx);
        })
    });
    let form = panel(&notes, cx);
    cx.update(|window, cx| {
        window.close_dialog(cx);
        window.open_dialog(cx, |dialog, _, _| dialog.title("Unrelated"));
        notes.update(cx, |this, cx| {
            this.handle_settings_intent(
                form.entity_id(),
                SettingsIntent::OpenSettingsJson,
                window,
                cx,
            );
            assert!(this.settings_panel.is_none());
            assert!(window.has_active_dialog(cx));
        });
        window.close_dialog(cx);
        notes.update(cx, |this, cx| this.open_settings(window, cx));
    });
    let form = panel(&notes, cx);
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            let _ = this.session.pick();
            this.handle_settings_intent(
                form.entity_id(),
                SettingsIntent::OpenSettingsJson,
                window,
                cx,
            );
            assert!(this.settings_panel.is_some());
            assert!(form.read(cx).values.busy);
            assert!(!this.settings_panel.as_ref().unwrap().opening_file);
            assert_eq!(
                this.tabs.active.path(),
                Some(fs::canonicalize(&fixture.standalone).unwrap().as_path())
            );
        })
    });
}

#[gpui::test]
fn settings_json_unsaved_cancel_preserves_document(cx: &mut TestAppContext) {
    let (fixture, notes, cx) = fixture(cx, true, false);
    intent(&panel(&notes, cx), SettingsIntent::Close, cx);
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.tabs.active.editor.select(0..8, window, cx);
            this.tabs
                .active
                .editor
                .replace_selection("draft", window, cx);
            this.open_settings(window, cx);
        })
    });
    intent(&panel(&notes, cx), SettingsIntent::OpenSettingsJson, cx);
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    cx.update(|_, cx| {
        let this = notes.read(cx);
        assert_eq!(
            this.tabs.active.path(),
            Some(fixture.first.root.join("note.md").as_path())
        );
        assert_eq!(this.tabs.active.editor.text(cx).as_ref(), "draft");
        assert!(this.dirty(cx));
        assert!(!this.is_busy());
        assert!(this.settings_panel.is_none());
    });
}

#[gpui::test]
fn settings_json_waits_for_latest_preferences_and_ignores_duplicate_requests(
    cx: &mut TestAppContext,
) {
    let (fixture, notes, cx) = fixture(cx, true, false);
    let form = panel(&notes, cx);
    cx.update(|_, cx| {
        notes.update(cx, |this, _| {
            this.notebooks
                .set_test_persistence_target(fixture._directory.path().join("index.json"));
            this.notebooks
                .set_test_persistence_delay(Duration::from_secs(10));
        })
    });
    intent(&form, SettingsIntent::DateFormat("YYYY".into()), cx);
    intent(&form, SettingsIntent::OpenSettingsJson, cx);
    intent(&form, SettingsIntent::OpenSettingsJson, cx);
    cx.update(|window, cx| {
        assert!(window.has_active_dialog(cx));
        assert!(form.read(cx).values.opening_settings_file);
        assert_eq!(
            notes.read(cx).tabs.active.path(),
            Some(fixture.first.root.join("note.md").as_path())
        );
    });
    intent(&form, SettingsIntent::DateFormat("YYYY-MM".into()), cx);
    cx.executor().advance_clock(Duration::from_secs(10));
    cx.run_until_parked();
    cx.update(|_, cx| {
        let this = notes.read(cx);
        assert!(this.settings_panel.is_none());
        let json: serde_json::Value =
            serde_json::from_str(this.tabs.active.editor.text(cx).as_ref()).unwrap();
        assert_eq!(json["date_format"], "YYYY-MM");
        assert_eq!(
            this.tabs.active.path(),
            Some(notebook::settings_path(&fixture.first.root).as_path())
        );
    });
}

#[gpui::test]
fn settings_json_pending_open_expires_when_dialog_closes(cx: &mut TestAppContext) {
    let (fixture, notes, cx) = fixture(cx, true, false);
    let form = panel(&notes, cx);
    cx.update(|_, cx| {
        notes.update(cx, |this, _| {
            this.notebooks
                .set_test_persistence_target(fixture._directory.path().join("index.json"));
            this.notebooks
                .set_test_persistence_delay(Duration::from_secs(10));
        })
    });
    intent(&form, SettingsIntent::DateFormat("YYYY".into()), cx);
    intent(&form, SettingsIntent::OpenSettingsJson, cx);
    cx.update(|window, cx| {
        window.close_dialog(cx);
        window.open_dialog(cx, |dialog, _, _| dialog.title("Unrelated"));
    });
    cx.executor().advance_clock(Duration::from_secs(10));
    cx.run_until_parked();
    cx.update(|window, cx| {
        assert!(window.has_active_dialog(cx));
        assert!(notes.read(cx).settings_panel.is_none());
        assert_eq!(
            notes.read(cx).tabs.active.path(),
            Some(fixture.first.root.join("note.md").as_path())
        );
    });
}

#[gpui::test]
fn settings_json_failed_preferences_keep_dialog_and_missing_file_uses_load_error(
    cx: &mut TestAppContext,
) {
    let (fixture, notes, cx) = fixture(cx, true, false);
    let path = notebook::settings_path(&fixture.first.root);
    let form = panel(&notes, cx);
    cx.update(|_, cx| {
        notes.update(cx, |this, _| {
            this.notebooks
                .set_test_persistence_target(fixture._directory.path().join("index.json"));
            this.notebooks
                .set_test_persistence_delay(Duration::from_secs(10));
        })
    });
    intent(&form, SettingsIntent::DateFormat("YYYY".into()), cx);
    intent(&form, SettingsIntent::OpenSettingsJson, cx);
    fs::write(&path, "corrupt settings").unwrap();
    cx.executor().advance_clock(Duration::from_secs(10));
    cx.run_until_parked();
    cx.update(|window, cx| {
        assert!(window.has_active_dialog(cx));
        assert!(!form.read(cx).values.opening_settings_file);
        assert!(
            form.read(cx)
                .values
                .notice
                .as_ref()
                .unwrap()
                .contains("Could not open settings JSON")
        );
        assert_eq!(
            notes.read(cx).tabs.active.path(),
            Some(fixture.first.root.join("note.md").as_path())
        );
    });
    assert_eq!(fs::read_to_string(&path).unwrap(), "corrupt settings");
    // Repair and submit through the normal writer to clear the failed revision.
    notebook::write_json(&path, &fixture.first.settings).unwrap();
    intent(&form, SettingsIntent::DateFormat("YYYY-MM".into()), cx);
    fs::remove_file(&path).unwrap();
    intent(&form, SettingsIntent::OpenSettingsJson, cx);
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer("Ok");
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert!(notes.read(cx).settings_panel.is_none());
        assert_eq!(
            notes.read(cx).tabs.active.path(),
            Some(fixture.first.root.join("note.md").as_path())
        );
    });
    assert!(!path.exists());
}

#[gpui::test]
fn settings_json_reuses_retained_unsaved_text(cx: &mut TestAppContext) {
    let (fixture, notes, cx) = fixture(cx, true, false);
    intent(&panel(&notes, cx), SettingsIntent::OpenSettingsJson, cx);
    let (document, draft) = cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.tabs.active.editor.replace_selection(" ", window, cx);
            let document = this.document_id();
            let draft = this.tabs.active.editor.text(cx);
            this.new_tab(window, cx);
            this.open_settings(window, cx);
            (document, draft)
        })
    });
    intent(&panel(&notes, cx), SettingsIntent::OpenSettingsJson, cx);
    cx.update(|window, cx| {
        let this = notes.read(cx);
        assert_eq!(this.document_id(), document);
        assert_eq!(this.tabs.active.editor.text(cx), draft);
        assert!(this.dirty(cx));
        assert_eq!(this.tabs.active.mode, ViewMode::Source);
        assert_eq!(
            this.tabs.active.path(),
            Some(notebook::settings_path(&fixture.first.root).as_path())
        );
        assert!(this.tabs.active.editor.is_focused(window, cx));
    });
    // Opening again in the same tab must also preserve the buffer and its focus.
    cx.update(|window, cx| notes.update(cx, |this, cx| this.open_settings(window, cx)));
    intent(&panel(&notes, cx), SettingsIntent::OpenSettingsJson, cx);
    assert!(!cx.has_pending_prompt());
    cx.update(|window, cx| {
        assert_eq!(notes.read(cx).tabs.active.editor.text(cx), draft);
        assert!(notes.read(cx).tabs.active.editor.is_focused(window, cx));
    });
}
