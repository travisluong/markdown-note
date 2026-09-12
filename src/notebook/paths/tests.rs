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
