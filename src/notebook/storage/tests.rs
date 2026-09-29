use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    time::{Duration, UNIX_EPOCH},
};

use super::{
    Notebook, create_untitled, index_path_for, load_index, move_entry, read_directory, save_state,
    settings_path, write_json,
};
use crate::notebook::{
    paths::{markdown, name, resolve},
    settings::{
        Appearance, Autosave, Index, Settings, normalize_sidebar_width, parse_hidden_extensions,
    },
    tree::FileSort,
};

#[test]
fn platform_paths_do_not_require_windows_environment_on_unix() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    for (os, variable, suffix) in [
        ("windows", "APPDATA", "markdown-note/notebooks.json"),
        (
            "macos",
            "HOME",
            "Library/Application Support/markdown-note/notebooks.json",
        ),
        ("linux", "HOME", ".config/markdown-note/notebooks.json"),
        ("linux", "XDG_CONFIG_HOME", "markdown-note/notebooks.json"),
    ] {
        let path = index_path_for(os, |key| {
            (key == variable).then(|| root.as_os_str().to_owned())
        })
        .unwrap();
        assert_eq!(path, root.join(suffix));
        let mut index = load_index(&path).unwrap();
        index.register(root.join("Notes"));
        write_json(&path, &index).unwrap();
        assert_eq!(load_index(&path).unwrap().active, index.active);
    }
}

#[test]
fn xdg_override_must_be_absolute_and_takes_precedence() {
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    let config = temp.path().join("config");
    for override_path in [Path::new(""), Path::new("relative"), config.as_path()] {
        let path = index_path_for("linux", |key| match key {
            "HOME" => Some(home.clone().into_os_string()),
            "XDG_CONFIG_HOME" => Some(override_path.as_os_str().to_owned()),
            _ => None,
        })
        .unwrap();
        let base = if override_path.is_absolute() {
            config.clone()
        } else {
            home.join(".config")
        };
        assert_eq!(path, base.join("markdown-note/notebooks.json"));
    }
    for os in ["windows", "macos", "linux"] {
        assert!(index_path_for(os, |_| None).is_err());
        assert!(index_path_for(os, |_| Some("".into())).is_err());
        assert!(index_path_for(os, |_| Some("relative".into())).is_err());
    }
}

#[test]
fn invalid_view_mode_does_not_overwrite_settings() {
    let fixture = tempfile::tempdir().unwrap();
    let root = fixture.path().join("book");
    let mut book = Notebook::open(&root, true).unwrap();
    let path = settings_path(&root);
    let mut json = serde_json::to_value(&book.settings).unwrap();
    json["view_mode"] = "future_mode".into();
    let saved = serde_json::to_string(&json).unwrap();
    fs::write(&path, &saved).unwrap();
    book.settings.view_mode = crate::ViewMode::Source;
    assert!(
        save_state(
            &fixture.path().join("index.json"),
            &Index::default(),
            Some(&book)
        )
        .is_err()
    );
    assert_eq!(fs::read_to_string(path).unwrap(), saved);
}

#[test]
fn view_modes_default_and_persist_independently() {
    let old: Settings =
        serde_json::from_str(r#"{"version":1,"last_file":null,"expanded":[]}"#).unwrap();
    assert_eq!(old.view_mode, crate::ViewMode::LivePreview);
    let fixture = tempfile::tempdir().unwrap();
    for (name, mode, serialized) in [
        ("source", crate::ViewMode::Source, "source"),
        ("live", crate::ViewMode::LivePreview, "live_preview"),
        ("reading", crate::ViewMode::Reading, "reading"),
    ] {
        let root = fixture.path().join(name);
        let mut book = Notebook::open(&root, true).unwrap();
        assert_eq!(book.settings.view_mode, crate::ViewMode::LivePreview);
        book.settings.view_mode = mode;
        write_json(&settings_path(&root), &book.settings).unwrap();
        assert_eq!(
            Notebook::open(&root, false).unwrap().settings.view_mode,
            mode
        );
        assert_eq!(
            serde_json::to_value(&book.settings).unwrap()["view_mode"],
            serialized
        );
    }
    assert_eq!(
        Notebook::open(&fixture.path().join("source"), false)
            .unwrap()
            .settings
            .view_mode,
        crate::ViewMode::Source
    );
    assert_eq!(
        Notebook::open(&fixture.path().join("reading"), false)
            .unwrap()
            .settings
            .view_mode,
        crate::ViewMode::Reading
    );
}

#[test]
fn sidebar_width_compatibility_normalization_and_persistence() {
    let old: Settings =
        serde_json::from_str(r#"{"version":1,"last_file":null,"expanded":[]}"#).unwrap();
    assert_eq!(old.sidebar_width, 280.);
    assert_eq!(normalize_sidebar_width(-1.), 160.);
    assert_eq!(normalize_sidebar_width(900.), 600.);
    assert_eq!(normalize_sidebar_width(f32::NAN), 280.);
    assert_eq!(normalize_sidebar_width(f32::INFINITY), 280.);
    let fixture = tempfile::tempdir().unwrap();
    for (name, saved, expected) in [
        ("First", 375.5, 375.5),
        ("Second", 210., 210.),
        ("Small", 0., 160.),
        ("Large", 999., 600.),
    ] {
        let root = fixture.path().join(name);
        let mut book = Notebook::open(&root, true).unwrap();
        book.settings.sidebar_width = saved;
        write_json(&settings_path(&root), &book.settings).unwrap();
        assert_eq!(
            Notebook::open(&root, false).unwrap().settings.sidebar_width,
            expected
        );
    }
    assert_eq!(
        Notebook::open(&fixture.path().join("First"), false)
            .unwrap()
            .settings
            .sidebar_width,
        375.5
    );
}

#[test]
fn terminal_width_compatibility_normalization_and_persistence() {
    use crate::notebook::normalize_terminal_width;
    let old: Settings =
        serde_json::from_str(r#"{"version":1,"last_file":null,"expanded":[]}"#).unwrap();
    assert_eq!(old.terminal_width, 280.);
    assert_eq!(normalize_terminal_width(-1.), 160.);
    assert_eq!(normalize_terminal_width(900.), 900.);
    assert_eq!(normalize_terminal_width(f32::NAN), 280.);
    assert_eq!(normalize_terminal_width(f32::INFINITY), 280.);
    let fixture = tempfile::tempdir().unwrap();
    for (name, saved, expected) in [
        ("First", 375.5, 375.5),
        ("Second", 210., 210.),
        ("Small", 0., 160.),
        ("Large", 999., 999.),
    ] {
        let root = fixture.path().join(name);
        let mut book = Notebook::open(&root, true).unwrap();
        book.settings.terminal_width = saved;
        write_json(&settings_path(&root), &book.settings).unwrap();
        assert_eq!(
            Notebook::open(&root, false)
                .unwrap()
                .settings
                .terminal_width,
            expected
        );
    }
}

#[test]
fn appearance_compatibility_and_round_trip() {
    let old: Settings =
        serde_json::from_str(r#"{"version":1,"last_file":null,"expanded":[]}"#).unwrap();
    assert_eq!(old.autosave, Autosave::default());
    assert_eq!(
        serde_json::from_str::<Autosave>(r#"{"enabled":false}"#)
            .unwrap()
            .delay_ms,
        1000
    );
    for value in ["", "99", "60001", "1.5", "-100", "abc"] {
        assert_eq!(Autosave::parse_delay(value), None);
    }
    for value in [100, 1000, 60000] {
        assert_eq!(Autosave::parse_delay(&value.to_string()), Some(value));
    }
    let settings = Autosave {
        enabled: false,
        delay_ms: 2500,
    };
    assert_eq!(
        serde_json::from_str::<Autosave>(&serde_json::to_string(&settings).unwrap()).unwrap(),
        settings
    );
    let old: Settings =
        serde_json::from_str(r#"{"version":1,"last_file":null,"expanded":[]}"#).unwrap();
    assert_eq!(old.appearance, Appearance::default());
    let partial: Appearance = serde_json::from_str(r#"{"dark_mode":true}"#).unwrap();
    assert_eq!(partial.content_font_size, 16);
    assert_eq!(partial.ui_font_size, 16);
    assert_eq!(partial.dark_mode, Some(true));
    let bounds = Appearance {
        content_font_size: 0,
        ui_font_size: 500,
        dark_mode: Some(false),
        ..Default::default()
    }
    .normalized();
    assert_eq!((bounds.content_font_size, bounds.ui_font_size), (10, 24));
    let temp = tempfile::tempdir().unwrap();
    let mut book = Notebook::open(temp.path(), false).unwrap();
    book.settings.appearance = Appearance {
        content_font_size: 40,
        ui_font_size: 24,
        dark_mode: Some(true),
        ..Default::default()
    };
    write_json(&settings_path(temp.path()), &book.settings).unwrap();
    assert_eq!(
        Notebook::open(temp.path(), false)
            .unwrap()
            .settings
            .appearance,
        book.settings.appearance
    );
    let path = settings_path(temp.path());
    fs::write(
        &path,
        r#"{"version":1,"last_file":null,"expanded":[],"appearance":{"dark_mode":"invalid"}}"#,
    )
    .unwrap();
    let original = fs::read(&path).unwrap();
    assert!(
        save_state(
            &temp.path().join("index.json"),
            &Index::default(),
            Some(&book)
        )
        .is_err()
    );
    assert_eq!(fs::read(path).unwrap(), original);
}

#[test]
fn forget_preserves_files_and_other_notebooks() {
    let temp = tempfile::tempdir().unwrap();
    let first = temp.path().join("First");
    let second = temp.path().join("Second");
    Notebook::open(&first, true).unwrap();
    fs::write(first.join("note.md"), "keep me").unwrap();
    let mut index = Index::default();
    index.register(first.clone());
    index.register(second.clone());
    index.forget(&first);
    assert_eq!(index.active, Some(second.clone()));
    assert_eq!(
        fs::read_to_string(first.join("note.md")).unwrap(),
        "keep me"
    );
    index.forget(&second);
    assert!(index.active.is_none());
    assert!(index.notebooks.is_empty());
    let path = temp.path().join("index.json");
    save_state(&path, &index, None).unwrap();
    assert!(load_index(&path).unwrap().notebooks.is_empty());
}

#[test]
fn create_reopen_and_persist() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("Notes");
    let mut book = Notebook::open(&root, true).unwrap();
    assert_eq!(book.settings.date_format, "YYYY-MM-DD");
    book.settings.date_format = "MMMM Do, YYYY".into();
    assert!(Notebook::open(&root, true).is_err());
    fs::create_dir(root.join("folder")).unwrap();
    fs::write(root.join("folder/note.md"), "# Hello").unwrap();
    book.settings.last_file = Some("folder/note.md".into());
    book.settings.expanded.insert("folder".into());
    write_json(&settings_path(&root), &book.settings).unwrap();
    let reopened = Notebook::open(&root, false).unwrap();
    assert_eq!(reopened.settings.date_format, "MMMM Do, YYYY");
    assert_eq!(reopened.settings.last_file, book.settings.last_file);
    assert_eq!(reopened.settings.expanded, book.settings.expanded);
    let mut index = Index::default();
    index.register(book.root.clone());
    index.register(Notebook::open(&root.join("."), false).unwrap().root);
    assert_eq!(index.notebooks.len(), 1);
    let index_file = temp.path().join("app/notebooks.json");
    write_json(&index_file, &index).unwrap();
    assert_eq!(load_index(&index_file).unwrap().active, Some(book.root));
}

#[test]
fn corrupt_and_future_settings_are_preserved() {
    let temp = tempfile::tempdir().unwrap();
    Notebook::open(temp.path(), false).unwrap();
    let path = settings_path(temp.path());
    for bytes in [
        "broken json",
        r#"{"version":999,"last_file":null,"expanded":[]}"#,
        r#"{"version":1,"last_file":"../outside.md","expanded":[]}"#,
    ] {
        fs::write(&path, bytes).unwrap();
        assert!(Notebook::open(temp.path(), false).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), bytes);
    }
    assert!(load_index(&path).is_err());
    assert!(
        load_index(&temp.path().join("missing.json"))
            .unwrap()
            .notebooks
            .is_empty()
    );
}

#[test]
fn directory_listing_and_safe_paths() {
    let temp = tempfile::tempdir().unwrap();
    Notebook::open(temp.path(), false).unwrap();
    fs::create_dir_all(temp.path().join("z-folder/empty")).unwrap();
    fs::write(temp.path().join("b.txt"), "text").unwrap();
    fs::write(temp.path().join("A.MD"), "# Note").unwrap();
    let entries = read_directory(temp.path(), Path::new("")).unwrap();
    assert_eq!(
        entries
            .iter()
            .map(|e| name(&e.relative))
            .collect::<Vec<_>>(),
        ["z-folder", "A.MD", "b.txt"]
    );
    assert!(
        read_directory(temp.path(), Path::new("z-folder/empty"))
            .unwrap()
            .is_empty()
    );
    assert!(read_directory(temp.path(), Path::new("missing")).is_err());
    assert!(resolve(temp.path(), Path::new("../outside.md")).is_err());
    assert!(resolve(temp.path(), Path::new(".markdown-note/settings.json")).is_err());
    assert!(markdown(Path::new("A.MD")));
    assert!(!markdown(Path::new("b.txt")));
}

#[test]
fn persistence_does_not_recreate_missing_notebook_or_overwrite_corruption() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("Notebook");
    let book = Notebook::open(&root, true).unwrap();
    let index_path = temp.path().join("index.json");
    let mut index = Index::default();
    index.register(book.root.clone());
    save_state(&index_path, &index, Some(&book)).unwrap();
    fs::write(settings_path(&root), "damaged").unwrap();
    assert!(save_state(&index_path, &index, Some(&book)).is_err());
    assert_eq!(fs::read_to_string(settings_path(&root)).unwrap(), "damaged");
    fs::rename(&root, temp.path().join("Moved")).unwrap();
    assert!(save_state(&index_path, &index, Some(&book)).is_err());
    assert!(!root.exists());
    fs::write(&index_path, "damaged index").unwrap();
    assert!(save_state(&index_path, &index, None).is_err());
    assert_eq!(fs::read_to_string(index_path).unwrap(), "damaged index");
}

#[cfg(windows)]
#[test]
fn junctions_are_listed_but_never_traversed() {
    use std::process::Command;
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("Notebook");
    let book = Notebook::open(&root, true).unwrap();
    let link = root.join("loop");
    // Junction creation does not require Windows developer mode or elevation.
    let script = "New-Item -ItemType Junction -Path $env:GPUI_TEST_LINK -Target $env:GPUI_TEST_TARGET | Out-Null";
    let result = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .env("GPUI_TEST_LINK", &link)
        .env("GPUI_TEST_TARGET", &root)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let entries = read_directory(&book.root, Path::new("")).unwrap();
    assert_eq!(entries.len(), 1);
    assert!(entries[0].link);
    assert!(read_directory(&book.root, Path::new("loop")).is_err());
    assert!(resolve(&book.root, Path::new("loop/note.md")).is_err());
    // Remove the junction itself, never its target.
    fs::remove_dir(link).unwrap();
    assert!(root.exists());
}

#[test]
fn move_and_preserve_paths() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::create_dir(root.join("folder")).unwrap();
    fs::write(root.join("a.md"), "unsaved lives in the editor").unwrap();
    let destination = move_entry(root, Path::new("a.md"), Path::new("folder")).unwrap();
    assert_eq!(destination, Path::new("folder/a.md"));
    assert!(!root.join("a.md").exists());
    assert_eq!(
        move_entry(root, &destination, Path::new("")).unwrap(),
        Path::new("a.md")
    );
    assert!(move_entry(root, Path::new("folder"), Path::new("folder")).is_err());
    fs::write(root.join("folder/a.md"), "collision").unwrap();
    assert!(move_entry(root, Path::new("a.md"), Path::new("folder")).is_err());
    assert_eq!(
        fs::read_to_string(root.join("folder/a.md")).unwrap(),
        "collision"
    );
    assert!(move_entry(root, Path::new("../outside"), Path::new("")).is_err());
    let mut settings = Settings {
        last_file: Some(PathBuf::from("folder/a.md")),
        ..Default::default()
    };
    settings.expanded.insert(PathBuf::from("folder/sub"));
    settings.remap_entry(Path::new("folder"), Path::new("new/folder"));
    assert_eq!(
        settings.last_file.unwrap(),
        PathBuf::from("new/folder/a.md")
    );
    assert!(settings.expanded.contains(Path::new("new/folder/sub")));
    assert!(settings.expanded.contains(Path::new("new")));
}

#[test]
fn metadata_refresh_changes_modified_order_and_same_parent_move_is_noop() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::write(root.join("a.md"), "a").unwrap();
    fs::write(root.join("b.md"), "b").unwrap();
    let set_time = |path: &str, seconds| {
        fs::OpenOptions::new()
            .write(true)
            .open(root.join(path))
            .unwrap()
            .set_times(fs::FileTimes::new().set_modified(UNIX_EPOCH + Duration::from_secs(seconds)))
            .unwrap();
    };
    set_time("a.md", 100);
    set_time("b.md", 200);
    let mut entries = read_directory(root, Path::new("")).unwrap();
    FileSort::ModifiedNewest.sort_entries(&mut entries);
    assert_eq!(name(&entries[0].relative), "b.md");
    set_time("a.md", 300);
    entries = read_directory(root, Path::new("")).unwrap();
    FileSort::ModifiedNewest.sort_entries(&mut entries);
    assert_eq!(name(&entries[0].relative), "a.md");
    let metadata = fs::metadata(root.join("a.md")).unwrap();
    assert_eq!(entries[0].created, metadata.created().ok());
    assert_eq!(
        move_entry(root, Path::new("a.md"), Path::new("")).unwrap(),
        Path::new("a.md")
    );
    assert_eq!(
        fs::metadata(root.join("a.md")).unwrap().modified().unwrap(),
        metadata.modified().unwrap()
    );
    assert_eq!(fs::read_to_string(root.join("a.md")).unwrap(), "a");
    fs::create_dir_all(root.join("folder/child")).unwrap();
    assert!(move_entry(root, Path::new("folder"), Path::new("folder/child")).is_err());
}

#[test]
fn creates_empty_notes_and_preserves_file_and_directory_collisions() {
    let dir = tempfile::tempdir().unwrap();
    let first = create_untitled(dir.path()).unwrap();
    assert_eq!(first.file_name().unwrap(), "Untitled.md");
    assert_eq!(fs::read(&first).unwrap(), b"");
    fs::write(&first, "keep me").unwrap();
    assert_eq!(
        create_untitled(dir.path()).unwrap().file_name().unwrap(),
        "Untitled 1.md"
    );
    fs::create_dir(dir.path().join("Untitled 2.md")).unwrap();
    assert_eq!(
        create_untitled(dir.path()).unwrap().file_name().unwrap(),
        "Untitled 3.md"
    );
    assert_eq!(fs::read_to_string(first).unwrap(), "keep me");
    fs::remove_file(dir.path().join("Untitled 1.md")).unwrap();
    assert_eq!(
        create_untitled(dir.path()).unwrap().file_name().unwrap(),
        "Untitled 1.md"
    );
    assert!(create_untitled(&dir.path().join("missing")).is_err());
    assert!(create_untitled(&dir.path().join("Untitled.md")).is_err());
}

#[test]
fn concurrent_creation_uses_distinct_names() {
    let dir = tempfile::tempdir().unwrap();
    let paths = std::thread::scope(|scope| {
        let jobs: Vec<_> = (0..8)
            .map(|_| scope.spawn(|| create_untitled(dir.path()).unwrap()))
            .collect();
        jobs.into_iter()
            .map(|job| job.join().unwrap())
            .collect::<BTreeSet<_>>()
    });
    assert_eq!(paths.len(), 8);
    for path in paths {
        assert_eq!(fs::metadata(path).unwrap().len(), 0);
    }
}

#[test]
fn extension_settings_defaults_and_notebook_persistence() {
    let old: Settings =
        serde_json::from_str(r#"{"version":1,"last_file":null,"expanded":[]}"#).unwrap();
    assert!(old.show_file_extensions);
    assert_eq!(old.always_hide_extensions, ["md"]);
    let fixture = tempfile::tempdir().unwrap();
    let first = fixture.path().join("first");
    let second = fixture.path().join("second");
    let mut book = Notebook::open(&first, true).unwrap();
    assert!(book.settings.show_file_extensions);
    assert_eq!(book.settings.always_hide_extensions, ["md"]);
    book.settings.show_file_extensions = false;
    book.settings.always_hide_extensions = parse_hidden_extensions(".md, TXT").unwrap();
    write_json(&settings_path(&first), &book.settings).unwrap();
    let mut reopened = Notebook::open(&first, false).unwrap();
    assert!(!reopened.settings.show_file_extensions);
    assert_eq!(reopened.settings.always_hide_extensions, ["md", "txt"]);
    reopened.settings.show_file_extensions = true;
    reopened.settings.always_hide_extensions.clear();
    write_json(&settings_path(&first), &reopened.settings).unwrap();
    assert!(
        Notebook::open(&first, false)
            .unwrap()
            .settings
            .always_hide_extensions
            .is_empty()
    );
    let other = Notebook::open(&second, true).unwrap();
    assert!(other.settings.show_file_extensions);
    assert_eq!(other.settings.always_hide_extensions, ["md"]);
}

#[test]
fn renamed_index_starts_fresh_and_preserves_legacy_index_bytes() {
    let fixture = tempfile::tempdir().unwrap();
    let legacy = fixture.path().join("mithril/notebooks.json");
    fs::create_dir_all(legacy.parent().unwrap()).unwrap();
    fs::write(&legacy, "damaged legacy index").unwrap();
    let path = index_path_for("windows", |key| {
        (key == "APPDATA").then(|| fixture.path().as_os_str().to_owned())
    })
    .unwrap();
    assert_eq!(path, fixture.path().join("markdown-note/notebooks.json"));
    let index = load_index(&path).unwrap();
    assert!(index.notebooks.is_empty());
    assert!(index.active.is_none());
    super::save_index(&path, &index).unwrap();
    assert_eq!(fs::read_to_string(&legacy).unwrap(), "damaged legacy index");
    assert!(load_index(&path).unwrap().notebooks.is_empty());
}
