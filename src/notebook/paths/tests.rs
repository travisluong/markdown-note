use std::path::PathBuf;

use super::{display_path, validate_name};

#[test]
fn folder_names() {
    for value in [
        "",
        " ",
        ".",
        "..",
        "a/b",
        "a\\b",
        "a:",
        "nul",
        "COM1.md",
        "LPT²",
        "trailing.",
        "trailing ",
    ] {
        assert!(validate_name(value).is_err(), "{value}");
    }
    for value in ["Work", "My notes", "日本語", "COM10", "notes.v2"] {
        validate_name(value).unwrap();
    }
}

#[test]
fn only_normalize_verbatim_drive_and_unc_for_display() {
    for (input, expected) in [
        (r"\\?\C:\Notes\你好", r"C:\Notes\你好"),
        (r"\\?\z:\", r"z:\"),
        (r"\\?\UNC\server\share\Notes", r"\\server\share\Notes"),
        (r"\\server\share\Notes", r"\\server\share\Notes"),
        (r"C:\Notes", r"C:\Notes"),
        (r"relative\Notes", r"relative\Notes"),
        (r"\\?\Volume{abc}\Notes", r"\\?\Volume{abc}\Notes"),
        (r"\\.\C:\Notes", r"\\.\C:\Notes"),
        (r"\\?\C:relative", r"\\?\C:relative"),
        (r"\\?\UNC\server", r"\\?\UNC\server"),
        ("/home/notes", "/home/notes"),
    ] {
        let path = PathBuf::from(input);
        assert_eq!(display_path(&path), expected);
        assert_eq!(path.to_string_lossy(), input);
    }
}

#[test]
fn exact_file_remap_preserves_a_usable_filename_without_a_trailing_separator() {
    let directory = tempfile::tempdir().unwrap();
    let from = directory.path().join("note.md");
    let to = directory.path().join("moved.md");
    std::fs::write(&to, "note").unwrap();
    let remapped = super::remap_path(&from, &from, &to);
    // Path equality ignores a trailing separator; the OS spelling must not add one.
    assert_eq!(remapped.as_os_str(), to.as_os_str());
    assert_eq!(std::fs::read_to_string(remapped).unwrap(), "note");
    assert_eq!(
        super::remap_path(&from.join("child.md"), &from, &to),
        to.join("child.md")
    );
    let unrelated = directory.path().join("other.md");
    assert_eq!(super::remap_path(&unrelated, &from, &to), unrelated);
}

#[test]
fn current_and_legacy_metadata_names_are_reserved_at_every_depth() {
    use super::{is_metadata_directory, safe_relative};
    use std::{ffi::OsStr, path::Path};

    for name in [".markdown-note", ".MARKDOWN-NOTE", ".mithril", ".MiThRiL"] {
        assert!(is_metadata_directory(OsStr::new(name)));
        assert!(validate_name(name).is_err());
        for path in [
            PathBuf::from(name),
            Path::new(name).join("settings.json"),
            Path::new("nested").join(name).join("note.md"),
        ] {
            assert!(!safe_relative(&path), "{}", path.display());
        }
    }
    for name in [
        "mithril",
        "markdown-note",
        ".mithril-notes",
        ".markdown-note.md",
    ] {
        assert!(!is_metadata_directory(OsStr::new(name)));
        validate_name(name).unwrap();
        assert!(safe_relative(Path::new(name)));
    }
}
