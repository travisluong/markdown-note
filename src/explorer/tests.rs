use super::Explorer;
use crate::notebook::{Entry, Settings};
use std::path::{Path, PathBuf};

fn image(path: &Path) -> bool {
    path.extension().is_some_and(|extension| extension == "png")
}
fn entry(path: &str, directory: bool) -> Entry {
    Entry {
        relative: PathBuf::from(path),
        directory,
        link: false,
        modified: None,
        created: None,
    }
}

#[test]
fn old_directory_results_cannot_restore_a_previous_notebook() {
    let settings = Settings::default();
    let mut explorer = Explorer::new(image);
    let old = explorer.begin_directory(PathBuf::new(), &settings);
    explorer.clear();
    let current = explorer.begin_directory(PathBuf::new(), &settings);
    assert!(
        explorer
            .complete_directory(
                old,
                PathBuf::new(),
                Ok(vec![entry("old.md", false)]),
                &settings
            )
            .is_none()
    );
    assert!(
        explorer
            .complete_directory(
                current,
                PathBuf::new(),
                Ok(vec![entry("new.md", false)]),
                &settings
            )
            .is_some()
    );
    assert_eq!(explorer.rows()[0].path, Path::new("new.md"));
}

#[test]
fn refreshed_metadata_retains_late_expansion_and_selection_by_path() {
    let mut settings = Settings::default();
    let mut explorer = Explorer::new(image);
    let initial = explorer.begin_directory(PathBuf::new(), &settings);
    explorer
        .complete_directory(
            initial,
            PathBuf::new(),
            Ok(vec![entry("folder", true)]),
            &settings,
        )
        .unwrap();
    let refresh = explorer.begin_refresh();
    settings.expanded.insert("folder".into());
    let expanded = explorer.begin_directory("folder".into(), &settings);
    explorer
        .complete_directory(
            expanded,
            "folder".into(),
            Ok(vec![entry("folder/note.md", false)]),
            &settings,
        )
        .unwrap();
    assert_eq!(
        explorer.select_path(Path::new("folder/note.md"), false),
        Some(1)
    );
    let missing = explorer
        .complete_refresh(
            refresh,
            vec![(PathBuf::new(), Ok(vec![entry("folder", true)]))],
            &settings,
        )
        .unwrap();
    assert!(missing.is_empty());
    assert_eq!(
        explorer.selected().unwrap().path,
        Path::new("folder/note.md")
    );
    settings.show_file_extensions = false;
    explorer.rebuild(Some(&settings));
    assert_eq!(explorer.selected().unwrap().label, "note");
    assert_eq!(
        explorer.selected().unwrap().filename.as_deref(),
        Some("note.md")
    );
}

#[test]
fn rows_keep_loading_empty_error_link_and_file_capabilities() {
    let mut settings = Settings::default();
    settings
        .expanded
        .extend([PathBuf::from("empty"), "pending".into(), "failed".into()]);
    let mut explorer = Explorer::new(image);
    let generation = explorer.begin_directory(PathBuf::new(), &settings);
    let mut linked = entry("linked.md", false);
    linked.link = true;
    explorer
        .complete_directory(
            generation,
            PathBuf::new(),
            Ok(vec![
                entry("empty", true),
                entry("pending", true),
                entry("failed", true),
                entry("note.md", false),
                entry("note.png", false),
                entry("data.txt", false),
                linked,
            ]),
            &settings,
        )
        .unwrap();
    explorer
        .complete_directory(generation, "empty".into(), Ok(vec![]), &settings)
        .unwrap();
    explorer
        .complete_directory(
            generation,
            "failed".into(),
            Err("unavailable".into()),
            &settings,
        )
        .unwrap();
    assert!(
        explorer
            .rows()
            .iter()
            .any(|row| row.label == "Empty folder" && row.disabled)
    );
    assert!(
        explorer
            .rows()
            .iter()
            .any(|row| row.label == "Loading…" && row.disabled)
    );
    assert!(
        explorer
            .rows()
            .iter()
            .any(|row| row.label == "Retry: unavailable" && row.retry)
    );
    assert!(
        explorer
            .rows()
            .iter()
            .any(|row| row.path == Path::new("linked.md") && row.disabled && !row.draggable)
    );
    assert!(
        explorer
            .rows()
            .iter()
            .any(|row| row.path == Path::new("data.txt") && row.disabled)
    );
    assert!(
        explorer
            .rows()
            .iter()
            .any(|row| row.path == Path::new("note.png") && !row.disabled)
    );
    assert!(!explorer.select(Some(explorer.rows().len())));
}

#[test]
fn refresh_releases_obsolete_loads_for_reexpanded_directories() {
    let mut settings = Settings::default();
    let mut explorer = Explorer::new(image);
    let initial = explorer.begin_directory(PathBuf::new(), &settings);
    explorer
        .complete_directory(
            initial,
            PathBuf::new(),
            Ok(vec![entry("folder", true)]),
            &settings,
        )
        .unwrap();
    settings.expanded.insert("folder".into());
    let obsolete = explorer.begin_directory("folder".into(), &settings);
    settings.expanded.remove(Path::new("folder"));
    let refresh = explorer.begin_refresh();
    settings.expanded.insert("folder".into());
    assert!(
        !explorer.contains_directory(Path::new("folder")),
        "a rejected worker cannot own a loading marker"
    );
    let current = explorer.begin_directory("folder".into(), &settings);
    let missing = explorer
        .complete_refresh(
            refresh,
            vec![(PathBuf::new(), Ok(vec![entry("folder", true)]))],
            &settings,
        )
        .unwrap();
    assert!(
        missing.is_empty(),
        "the current worker retains its loading marker"
    );
    assert!(
        explorer
            .complete_directory(
                obsolete,
                "folder".into(),
                Ok(vec![entry("folder/old.md", false)]),
                &settings
            )
            .is_none()
    );
    explorer
        .complete_directory(
            current,
            "folder".into(),
            Ok(vec![entry("folder/current.md", false)]),
            &settings,
        )
        .unwrap();
    assert!(
        explorer
            .rows()
            .iter()
            .any(|row| row.path == Path::new("folder/current.md"))
    );
    assert!(!explorer.rows().iter().any(|row| row.label == "Loading…"));
}
