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
