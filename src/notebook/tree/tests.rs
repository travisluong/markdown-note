use std::{
    path::Path,
    time::{Duration, UNIX_EPOCH},
};

use super::{Entry, FileSort, name, sidebar_file_name};
use crate::notebook::settings::default_always_hide_extensions;

fn entry(path: &str, directory: bool, modified: Option<u64>, created: Option<u64>) -> Entry {
    Entry {
        relative: path.into(),
        directory,
        link: false,
        modified: modified.map(|seconds| UNIX_EPOCH + Duration::from_secs(seconds)),
        created: created.map(|seconds| UNIX_EPOCH + Duration::from_secs(seconds)),
    }
}

#[test]
fn six_orders_keep_folders_alphabetical_and_missing_times_last() {
    let entries = vec![
        entry("z-folder", true, Some(1), Some(1)),
        entry("A-folder", true, Some(9), Some(9)),
        entry("b.md", false, Some(3), Some(1)),
        entry("a.md", false, Some(1), Some(3)),
        entry("C.md", false, Some(2), Some(2)),
        entry("unknown.md", false, None, None),
    ];
    for (sort, expected) in [
        (
            FileSort::NameAscending,
            ["a.md", "b.md", "C.md", "unknown.md"],
        ),
        (
            FileSort::NameDescending,
            ["unknown.md", "C.md", "b.md", "a.md"],
        ),
        (
            FileSort::ModifiedNewest,
            ["b.md", "C.md", "a.md", "unknown.md"],
        ),
        (
            FileSort::ModifiedOldest,
            ["a.md", "C.md", "b.md", "unknown.md"],
        ),
        (
            FileSort::CreatedNewest,
            ["a.md", "C.md", "b.md", "unknown.md"],
        ),
        (
            FileSort::CreatedOldest,
            ["b.md", "C.md", "a.md", "unknown.md"],
        ),
    ] {
        let mut sorted = entries.clone();
        sort.sort_entries(&mut sorted);
        let names: Vec<_> = sorted.iter().map(|e| name(&e.relative)).collect();
        assert_eq!(&names[..2], &["A-folder", "z-folder"]);
        assert_eq!(&names[2..], &expected, "{sort:?}");
    }
    for (sort, _) in FileSort::OPTIONS {
        let mut entries = vec![
            entry("b.md", false, Some(1), Some(1)),
            entry("a.md", false, Some(1), Some(1)),
            entry("A.md", false, Some(1), Some(1)),
        ];
        sort.sort_entries(&mut entries);
        let names: Vec<_> = entries.iter().map(|e| name(&e.relative)).collect();
        assert_eq!(
            names,
            if sort == FileSort::NameDescending {
                vec!["b.md", "a.md", "A.md"]
            } else {
                vec!["A.md", "a.md", "b.md"]
            }
        );
    }
}

#[test]
fn sidebar_extension_display() {
    let hidden = default_always_hide_extensions();
    for (name, show, expected) in [
        ("notes.md", true, "notes"),
        ("notes.MD", true, "notes"),
        ("photo.png", true, "photo.png"),
        ("photo.png", false, "photo"),
        ("notes.markdown", true, "notes.markdown"),
        ("archive.tar.gz", false, "archive.tar"),
        (".gitignore", false, ".gitignore"),
        ("README", false, "README"),
        ("name.", false, "name."),
        ("日本語.md", true, "日本語"),
    ] {
        assert_eq!(sidebar_file_name(Path::new(name), show, &hidden), expected);
    }
    assert_eq!(
        sidebar_file_name(Path::new("notes.md"), true, &[]),
        "notes.md"
    );
    assert_eq!(
        sidebar_file_name(Path::new("archive.tar.gz"), true, &["gz".into()]),
        "archive.tar"
    );
}
