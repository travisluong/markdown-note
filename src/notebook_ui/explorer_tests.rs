//! Explorer I/O adapter checks on TestPlatform.
use super::persistence_tests::fixture;
use gpui::TestAppContext;
use std::{
    fs,
    path::{Path, PathBuf},
};

#[gpui::test]
fn unfinished_folder_target_does_not_clear_an_existing_cursor(cx: &mut TestAppContext) {
    let (fixture, notes, cx) = fixture(cx);
    fs::create_dir(fixture.root.join("folder")).unwrap();
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.load_directory(PathBuf::new(), window, cx)
        })
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            assert!(
                this.notebooks
                    .explorer
                    .select_path(Path::new("note.md"), false)
                    .is_some()
            );
            this.notebooks.folder_target = Some("not-yet-visible".into());
            this.load_directory("folder".into(), window, cx);
        })
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        let this = notes.read(cx);
        assert_eq!(
            this.notebooks.explorer.selected().unwrap().path,
            Path::new("note.md")
        );
        assert_eq!(
            this.notebooks.folder_target.as_deref(),
            Some(Path::new("not-yet-visible"))
        );
        assert!(!this.notebooks.explorer.is_loading());
    });
}

#[gpui::test]
fn exclusions_invalidate_old_directory_results_and_prune_expanded_folders(cx: &mut TestAppContext) {
    let (fixture, notes, cx) = fixture(cx);
    fs::create_dir_all(fixture.root.join("archive/deep")).unwrap();
    fs::write(fixture.root.join("archive/deep/hidden.md"), "hidden").unwrap();
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            let settings = &mut this.notebooks.active.as_mut().unwrap().settings;
            settings.expanded.insert("archive".into());
            settings.expanded.insert("archive/deep".into());
            this.refresh_tree(window, cx);
        })
    });
    cx.run_until_parked();
    let stale = cx.update(|_, cx| notes.read(cx).notebooks.explorer.generation());
    let old_rows = crate::notebook::read_directory(&fixture.root, Path::new("")).unwrap();
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.update_excluded_patterns(vec!["^archive$".into()], window, cx);
            let settings = &this.notebooks.active.as_ref().unwrap().settings;
            assert!(
                this.notebooks
                    .explorer
                    .complete_directory(stale, PathBuf::new(), Ok(old_rows), settings)
                    .is_none()
            );
        })
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            assert!(
                this.notebooks
                    .explorer
                    .rows()
                    .iter()
                    .all(|row| !row.path.starts_with("archive"))
            );
            this.refresh_sorted_tree(window, cx);
        })
    });
    cx.run_until_parked();
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            assert!(
                !this
                    .notebooks
                    .explorer
                    .contains_directory(Path::new("archive/deep"))
            );
            assert!(!this.notebooks.explorer.is_loading());
            this.update_excluded_patterns(Vec::new(), window, cx);
        })
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert!(
            notes
                .read(cx)
                .notebooks
                .explorer
                .rows()
                .iter()
                .any(|row| row.path == Path::new("archive/deep/hidden.md"))
        );
    });
}
