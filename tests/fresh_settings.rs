//! Markdown Note starts with its own preferences while preserving legacy metadata.
use markdown_note::notebook::{self, Notebook, Settings};
use std::{fs, path::Path};

fn assert_fresh_settings(book: &Notebook) {
    assert_eq!(
        serde_json::to_value(&book.settings).unwrap(),
        serde_json::to_value(Settings::default()).unwrap()
    );
}

#[test]
fn legacy_preferences_are_ignored_and_preserved_when_new_settings_are_missing() {
    for legacy in [
        r#"{"version":1,"last_file":"old.md","expanded":["old"],"view_mode":"source"}"#,
        "malformed legacy JSON",
        r#"{"version":999}"#,
    ] {
        let directory = tempfile::tempdir().unwrap();
        let legacy_path = directory.path().join(".mithril/settings.json");
        fs::create_dir(legacy_path.parent().unwrap()).unwrap();
        fs::write(&legacy_path, legacy).unwrap();
        let book = Notebook::open(directory.path(), false).unwrap();
        assert_fresh_settings(&book);
        assert!(notebook::settings_path(&book.root).is_file());
        notebook::save_book(&book).unwrap();
        assert_eq!(fs::read_to_string(&legacy_path).unwrap(), legacy);
    }
    let directory = tempfile::tempdir().unwrap();
    let legacy_path = directory.path().join(".mithril");
    let bytes = "legacy metadata is not a directory";
    fs::write(&legacy_path, bytes).unwrap();
    let book = Notebook::open(directory.path(), false).unwrap();
    notebook::save_book(&book).unwrap();
    assert_eq!(fs::read_to_string(legacy_path).unwrap(), bytes);
}

#[test]
fn only_new_preferences_are_loaded_validated_and_saved() {
    let directory = tempfile::tempdir().unwrap();
    let legacy = directory.path().join(".mithril/settings.json");
    fs::create_dir(legacy.parent().unwrap()).unwrap();
    fs::write(&legacy, "broken legacy settings").unwrap();
    let mut book = Notebook::open(directory.path(), false).unwrap();
    book.settings.appearance.zoom_percent = 120;
    notebook::save_book(&book).unwrap();
    let reopened = Notebook::open(directory.path(), false).unwrap();
    assert_eq!(reopened.settings.appearance.zoom_percent, 120);
    let path = notebook::settings_path(&book.root);
    fs::write(&path, "broken current settings").unwrap();
    assert!(Notebook::open(directory.path(), false).is_err());
    assert!(notebook::save_book(&book).is_err());
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "broken current settings"
    );
    assert_eq!(
        fs::read_to_string(&legacy).unwrap(),
        "broken legacy settings"
    );
    for relative in [
        ".mithril/note.md",
        "nested/.MITHRIL/note.md",
        "nested/.MARKDOWN-NOTE/note.md",
    ] {
        let mut settings = Settings {
            last_file: Some(relative.into()),
            ..Default::default()
        };
        notebook::write_json(&path, &settings).unwrap();
        assert!(Notebook::open(directory.path(), false).is_err());
        settings.last_file = None;
        settings.expanded.insert(relative.into());
        notebook::write_json(&path, &settings).unwrap();
        assert!(Notebook::open(directory.path(), false).is_err());
    }
}

#[test]
fn metadata_cannot_be_created_renamed_moved_duplicated_or_trashed() {
    let directory = tempfile::tempdir().unwrap();
    let book = Notebook::open(directory.path(), false).unwrap();
    fs::create_dir(book.root.join("nested")).unwrap();
    let note = book.root.join("note.md");
    fs::write(&note, "preserved note").unwrap();
    fs::create_dir(book.root.join(".mithril")).unwrap();
    let legacy = book.root.join(".mithril/hidden.md");
    fs::write(&legacy, "preserved legacy data").unwrap();
    for name in [".mithril", ".MITHRIL", ".markdown-note", ".MARKDOWN-NOTE"] {
        assert!(notebook::create_folder(&book.root, Path::new(""), name).is_err());
        assert!(notebook::create_folder(&book.root, Path::new("nested"), name).is_err());
        assert!(notebook::rename_entry(&book.root, Path::new("nested"), name).is_err());
        assert!(notebook::rename_entry(&book.root, Path::new("note.md"), name).is_err());
        let protected = Path::new(name);
        assert!(notebook::entry_path(&book.root, protected).is_err());
        assert!(notebook::entry_directory(&book.root, protected).is_err());
        assert!(notebook::rename_entry(&book.root, protected, "renamed").is_err());
        assert!(notebook::move_entry(&book.root, protected, Path::new("nested")).is_err());
        assert!(notebook::move_entry(&book.root, Path::new("note.md"), protected).is_err());
        assert!(notebook::duplicate_entry(&book.root, &protected.join("hidden.md"), None).is_err());
        // Guards reject metadata before the native Trash backend is reached.
        assert!(notebook::trash_entry(&book.root, protected).is_err());
    }
    assert_eq!(fs::read_to_string(note).unwrap(), "preserved note");
    assert_eq!(fs::read_to_string(legacy).unwrap(), "preserved legacy data");
    assert!(notebook::settings_path(&book.root).is_file());
}

#[cfg(any(unix, windows))]
#[test]
fn linked_legacy_metadata_is_ignored_and_its_target_is_preserved() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("book");
    let target = directory.path().join("legacy-target");
    fs::create_dir(&root).unwrap();
    fs::create_dir(&target).unwrap();
    let bytes = "broken linked legacy settings";
    fs::write(target.join("settings.json"), bytes).unwrap();
    let link = root.join(".mithril");
    #[cfg(unix)]
    std::os::unix::fs::symlink(&target, &link).unwrap();
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let result = std::process::Command::new("powershell.exe")
            .creation_flags(0x08000000)
            .args(["-NoProfile", "-NonInteractive", "-Command",
                "New-Item -ItemType Junction -Path $env:MARKDOWN_NOTE_TEST_LINK -Target $env:MARKDOWN_NOTE_TEST_TARGET | Out-Null"])
            .env("MARKDOWN_NOTE_TEST_LINK", &link)
            .env("MARKDOWN_NOTE_TEST_TARGET", &target)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
    let opened = Notebook::open(&root, false);
    let saved = opened.as_ref().ok().map(notebook::save_book);
    let metadata = fs::symlink_metadata(&link);
    #[cfg(windows)]
    fs::remove_dir(&link).unwrap();
    #[cfg(unix)]
    fs::remove_file(&link).unwrap();
    // Remove the fixture link itself before any assertions or TempDir cleanup.
    let book = opened.unwrap();
    saved.unwrap().unwrap();
    assert!(notebook::is_link(&metadata.unwrap()));
    assert_fresh_settings(&book);
    assert_eq!(
        fs::read_to_string(target.join("settings.json")).unwrap(),
        bytes
    );
}
