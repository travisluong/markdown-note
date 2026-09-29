//! One exclusion policy across notebook discovery, search, and explicit access.
use gpui_component::input::Rope;
use markdown_note::{commands, document, explorer::Explorer, notebook, search};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
};

#[test]
fn exclusions_prune_every_discovery_surface_but_keep_explicit_access() {
    let temp = tempfile::tempdir().unwrap();
    let mut book = notebook::Notebook::open(temp.path(), false).unwrap();
    for dir in ["archive/deep", "nested/drafts", "nested/visible"] {
        fs::create_dir_all(book.root.join(dir)).unwrap();
    }
    for file in [
        "archive/deep/hidden.md",
        "nested/drafts/hidden.md",
        "nested/visible/keep.md",
        "keep.md",
        "scratch.tmp",
        ".markdown-note/hidden.md",
    ] {
        fs::write(book.root.join(file), "hit").unwrap();
    }
    book.settings.excluded_patterns =
        notebook::parse_excluded_patterns(r"^archive$, (^|/)drafts$, \.tmp$").unwrap();
    let policy = notebook::Exclusions::compile(&book.settings.excluded_patterns).unwrap();
    let root_entries =
        notebook::read_directory_filtered(&book.root, Path::new(""), &policy).unwrap();
    assert_eq!(
        root_entries
            .iter()
            .map(|e| e.relative.clone())
            .collect::<Vec<_>>(),
        [PathBuf::from("nested"), PathBuf::from("keep.md")]
    );
    // Explorer consumes filtered snapshots; expanded paths cannot bypass an excluded ancestor.
    let mut explorer = Explorer::new(|_| false);
    let generation = explorer.begin_directory(PathBuf::new(), &book.settings);
    explorer
        .complete_directory(generation, PathBuf::new(), Ok(root_entries), &book.settings)
        .unwrap();
    assert_eq!(explorer.rows().len(), 2);
    assert!(
        notebook::read_directory_filtered(&book.root, Path::new("archive/deep"), &policy)
            .unwrap()
            .is_empty()
    );
    assert!(
        notebook::read_directory_filtered(&book.root, Path::new("archive/deep/missing"), &policy)
            .unwrap()
            .is_empty()
    );
    let entries = commands::discover(&book.root, &policy).unwrap();
    assert_eq!(entries.len(), 4); // Two visible folders and two notes.
    assert!(
        entries
            .iter()
            .all(|entry| !policy.excludes(&entry.relative))
    );
    let results = search::scan(
        &book.root,
        &policy,
        "hit",
        false,
        Some((
            book.root.join("archive/deep/hidden.md"),
            Rope::from("unsaved hit"),
        )),
        &AtomicBool::new(false),
    );
    assert_eq!(
        results
            .hits
            .iter()
            .map(|hit| hit.relative.clone())
            .collect::<Vec<_>>(),
        [
            PathBuf::from("keep.md"),
            PathBuf::from("nested/visible/keep.md")
        ]
    );
    assert_eq!(results.skipped, 0);
    let path = notebook::resolve(&book.root, Path::new("archive/deep/hidden.md")).unwrap();
    assert!(document::open(&path).is_ok());
    let all = commands::discover(&book.root, &notebook::Exclusions::default()).unwrap();
    assert!(
        all.iter()
            .any(|entry| entry.relative == Path::new("archive/deep/hidden.md"))
    );
    assert!(
        !all.iter()
            .any(|entry| entry.relative.starts_with(".markdown-note"))
    );
}

#[test]
fn exclusions_round_trip_and_legacy_settings_default_to_empty() {
    let temp = tempfile::tempdir().unwrap();
    let mut book = notebook::Notebook::open(temp.path(), false).unwrap();
    let mut legacy = serde_json::to_value(&book.settings).unwrap();
    legacy.as_object_mut().unwrap().remove("excluded_patterns");
    let settings: notebook::Settings = serde_json::from_value(legacy).unwrap();
    assert!(settings.excluded_patterns.is_empty());
    book.settings.excluded_patterns =
        notebook::parse_excluded_patterns(r"^archive$, a{1,3}, report\,old").unwrap();
    notebook::save_book(&book).unwrap();
    assert_eq!(
        notebook::Notebook::open(&book.root, false)
            .unwrap()
            .settings
            .excluded_patterns,
        book.settings.excluded_patterns
    );
    // Loading invalid external regexes must preserve the recoverable notebook and raw list.
    book.settings.excluded_patterns = vec!["[broken".into()];
    notebook::save_book(&book).unwrap();
    let reopened = notebook::Notebook::open(&book.root, false).unwrap();
    assert_eq!(reopened.settings.excluded_patterns, ["[broken"]);
    assert!(notebook::Exclusions::compile(&reopened.settings.excluded_patterns).is_err());
}
