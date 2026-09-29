//! Session adapter decisions that also change notebook presentation.
use super::persistence_tests::fixture;
use crate::notebook;
use gpui::TestAppContext;
use std::{fs, time::Duration};

#[gpui::test]
fn removing_inactive_notebook_keeps_dirty_document_and_its_autosave(cx: &mut TestAppContext) {
    let (fixture, notes, cx) = fixture(cx);
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.notebooks.index.register(fixture.other.clone());
            this.notebooks.index.register(fixture.root.clone());
            this.update_autosave(
                notebook::Autosave {
                    enabled: true,
                    delay_ms: 100,
                },
                window,
                cx,
            );
            this.editor.diagnostic_input().update(cx, |editor, cx| {
                editor.select_source_range(0..editor.value().len(), window, cx);
                editor.replace_selection_isolated("retained A", window, cx);
            });
        })
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            let identity = this.document_id();
            let autosave = this.session.autosave();
            assert!(autosave.is_some());
            this.remove_notebook(fixture.other.clone(), window, cx);
            assert!(!this.notebooks.registered_paths().contains(&fixture.other));
            assert_eq!(this.notebooks.active.as_ref().unwrap().root, fixture.root);
            assert_eq!(this.document_id(), identity);
            assert_eq!(this.session.autosave(), autosave);
            assert!(this.session.write().is_none());
            assert!(!this.is_busy());
            assert!(this.dirty(cx));
            assert_eq!(this.editor.text(cx).as_ref(), "retained A");
        })
    });
    cx.run_until_parked();
    assert!(!cx.has_pending_prompt());
    assert_eq!(fs::read_to_string(&fixture.note).unwrap(), "original A");
    cx.executor().advance_clock(Duration::from_millis(150));
    cx.run_until_parked();
    assert_eq!(fs::read_to_string(&fixture.note).unwrap(), "retained A");
    cx.update(|_, cx| assert!(!notes.read(cx).dirty(cx)));
}

#[test]
fn settings_json_resolution_admits_only_exact_regular_settings_file() {
    let directory = tempfile::tempdir().unwrap();
    let book = notebook::Notebook::open(&directory.path().join("book"), true).unwrap();
    let path = notebook::settings_path(&book.root);
    // Invalid JSON is still text that can be opened for repair.
    fs::write(&path, "broken JSON").unwrap();
    assert_eq!(
        super::resolve_document_path(&book.root, &path).unwrap(),
        path
    );
    assert!(
        notebook::resolve(
            &book.root,
            std::path::Path::new(".markdown-note/settings.json")
        )
        .is_err()
    );
    for relative in [
        ".markdown-note/other.json",
        "nested/.markdown-note/settings.json",
        ".markdown-note/../note.md",
        ".mithril/settings.json",
        ".MITHRIL/settings.json",
        "nested/.mithril/settings.json",
        "nested/.MARKDOWN-NOTE/settings.json",
    ] {
        assert!(super::resolve_document_path(&book.root, &book.root.join(relative)).is_err());
    }
    fs::remove_file(&path).unwrap();
    assert!(super::resolve_document_path(&book.root, &path).is_err());
    fs::create_dir(&path).unwrap();
    assert!(super::resolve_document_path(&book.root, &path).is_err());
}

#[cfg(any(unix, windows))]
#[test]
fn settings_json_resolution_rejects_linked_metadata_directory() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("book");
    let target = directory.path().join("target");
    fs::create_dir(&root).unwrap();
    fs::create_dir(&target).unwrap();
    fs::write(target.join("settings.json"), "{}").unwrap();
    let link = root.join(".markdown-note");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&target, &link).unwrap();
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let result = std::process::Command::new("powershell.exe")
            .creation_flags(0x08000000) // CREATE_NO_WINDOW: headless diagnostics only.
            .args(["-NoProfile", "-NonInteractive", "-Command",
                "New-Item -ItemType Junction -Path $env:MARKDOWN_NOTE_TEST_LINK -Target $env:MARKDOWN_NOTE_TEST_TARGET | Out-Null"])
            .env("MARKDOWN_NOTE_TEST_LINK", &link)
            .env("MARKDOWN_NOTE_TEST_TARGET", &target)
            .output().unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    assert!(super::resolve_document_path(&root, &notebook::settings_path(&root)).is_err());
    assert!(notebook::Notebook::open(&root, false).is_err());
    #[cfg(windows)]
    fs::remove_dir(&link).unwrap();
    #[cfg(unix)]
    fs::remove_file(&link).unwrap();
    assert!(target.join("settings.json").exists());
}

#[cfg(unix)]
#[test]
fn settings_json_resolution_rejects_linked_file() {
    let directory = tempfile::tempdir().unwrap();
    let book = notebook::Notebook::open(&directory.path().join("book"), true).unwrap();
    let path = notebook::settings_path(&book.root);
    let target = directory.path().join("other.json");
    fs::write(&target, "{}").unwrap();
    fs::remove_file(&path).unwrap();
    std::os::unix::fs::symlink(&target, &path).unwrap();
    assert!(super::resolve_document_path(&book.root, &path).is_err());
    assert!(notebook::Notebook::open(&book.root, false).is_err());
}
