//! Opt-in UI smoke check. Uses only disposable notebooks and a disposable index.
use super::*;
use std::time::{Duration, Instant};

pub(crate) fn start(notes: Entity<Notes>, output: PathBuf, window: &mut Window, cx: &mut App) {
    let panic_path = output.with_extension("panic.txt");
    std::panic::set_hook(Box::new(move |info| {
        let _ = fs::write(&panic_path, info.to_string());
    }));
    let fixture = tempfile::tempdir().expect("temporary notebook fixture");
    let first = fixture.path().join("First");
    let second = fixture.path().join("Second");
    fs::create_dir(&second).unwrap();
    let index = fixture.path().join("app/notebooks.json");
    notes.update(cx, |this, cx| {
        this.notebooks.index_path = Some(index.clone());
        this.notebooks.index_writable = true;
        cx.notify();
    });
    window.spawn(cx, async move |cx| {
        let started = Instant::now();
        let mut step = 0;
        while step <= 19 {
            cx.background_executor().timer(Duration::from_millis(60)).await;
            assert!(started.elapsed() < Duration::from_secs(40), "Notebook smoke check timed out at step {step}");
            let ready = notes.update(cx, |this, _| !this.busy && !this.notebooks.directories.values().any(|d| matches!(d, DirectoryState::Loading))).unwrap();
            if !ready { continue; }
            fs::write(output.with_extension("progress.txt"), format!("step {step} of 19")).unwrap();
            cx.update(|window, cx| notes.update(cx, |this, cx| {
                match step {
                    0 => this.manage_notebooks(window, cx),
                    1 => {
                        assert!(window.has_active_dialog(cx));
                        this.notebooks.creating = true;
                        this.notebooks.name.update(cx, |name, cx| name.set_value("First", window, cx));
                        this.notebooks.location = first.parent().map(Path::to_path_buf);
                    }
                    2 => this.create_from_form(window, cx),
                    3 => {
                        assert!(!window.has_active_dialog(cx));
                        assert_eq!(notebook::name(&this.notebooks.active.as_ref().unwrap().root), "First");
                        assert!(this.document.is_none());
                        fs::create_dir_all(first.join("folder/empty")).unwrap();
                        fs::write(first.join("folder/nested.md"), "# Nested").unwrap();
                        fs::write(first.join("root.md"), "# Root").unwrap();
                        fs::write(first.join("image.png"), "not an image").unwrap();
                        this.refresh_tree(window, cx);
                    }
                    4 => {
                        assert!(this.notebooks.rows.iter().any(|r| r.label == "image.png" && r.disabled));
                        assert!(!this.notebooks.rows.iter().any(|r| r.label == ".gpui-note"));
                        let folder = this.notebooks.rows.iter().position(|r| r.label == "folder").unwrap();
                        this.activate_row(folder, window, cx);
                    }
                    5 => {
                        let nested = this.notebooks.rows.iter().position(|r| r.label == "nested.md").unwrap();
                        assert_eq!(this.notebooks.rows[nested].depth, 1);
                        this.activate_row(nested, window, cx);
                    }
                    6 => {
                        assert!(this.mode == ViewMode::Source);
                        assert_eq!(this.editor.read(cx).value().as_str(), "# Nested");
                        this.set_mode(ViewMode::LivePreview, window, cx);
                        this.request(Next::File(first.join("root.md")), window, cx);
                    }
                    7 => {
                        assert!(this.mode == ViewMode::LivePreview);
                        assert_eq!(this.editor.read(cx).value().as_str(), "# Root");
                        this.set_mode(ViewMode::Reading, window, cx);
                        this.request(Next::File(first.join("folder/nested.md")), window, cx);
                    }
                    8 => {
                        assert_eq!(this.reading_text.as_str(), "# Nested");
                        this.editor.update(cx, |editor, cx| editor.set_value("# Saved", window, cx));
                        assert!(this.dirty(cx));
                        // Exercise the same response handler as the native save prompt.
                        this.answer_request(None, Next::File(first.join("root.md")), window, cx);
                        assert_eq!(this.document.as_ref().unwrap().path.file_name().unwrap(), "nested.md");
                        assert!(this.dirty(cx));
                        this.answer_request(Some(0), Next::File(first.join("root.md")), window, cx);
                    }
                    9 => {
                        assert_eq!(fs::read_to_string(first.join("folder/nested.md")).unwrap(), "# Saved");
                        assert_eq!(this.reading_text.as_str(), "# Root");
                        this.editor.update(cx, |editor, cx| editor.set_value("discard me", window, cx));
                        this.answer_request(Some(1), Next::Notebook { path: second.clone(), create: false }, window, cx);
                    }
                    10 => {
                        assert_eq!(notebook::name(&this.notebooks.active.as_ref().unwrap().root), "Second");
                        assert!(this.document.is_none());
                        assert_eq!(fs::read_to_string(first.join("root.md")).unwrap(), "# Root");
                        this.request(Next::Notebook { path: first.clone(), create: false }, window, cx);
                    }
                    11 => {
                        assert_eq!(this.reading_text.as_str(), "# Root");
                        assert!(this.notebooks.active.as_ref().unwrap().settings.expanded.contains(Path::new("folder")));
                        assert_eq!(this.notebooks.index.notebooks.len(), 2);
                        this.request(Next::Notebook { path: first.join("missing"), create: false }, window, cx);
                    }
                    12 => {
                        assert!(this.notebooks.notice.as_ref().unwrap().contains("Could not open notebook"));
                        assert_eq!(this.reading_text.as_str(), "# Root");
                        assert_eq!(notebook::name(&this.notebooks.active.as_ref().unwrap().root), "First");
                        this.notebooks.notice = None;
                        this.notebooks.index = notebook::Index::default();
                        this.notebooks.active = None;
                        this.install_document(None, window, cx);
                        this.restore_notebooks(window, cx);
                    }
                    13 => {
                        assert_eq!(this.notebooks.index.notebooks.len(), 2);
                        assert_eq!(this.reading_text.as_str(), "# Root");
                        assert!(this.mode == ViewMode::Reading);
                        assert!(this.notebooks.rows.iter().any(|r| r.label == "nested.md"));
                        this.show_menu(MenuKind::Notebooks, window, cx);
                    }
                    14 => {
                        assert!(this.menu.is_some());
                        this.menu = None;
                        this.set_mode(ViewMode::Source, window, cx);
                        this.notebooks.tree_focus.focus(window);
                        let nested = this.notebooks.rows.iter().position(|r| r.label == "nested.md").unwrap();
                        this.notebooks.cursor = Some(nested + 1);
                        this.tree_move(false, window, cx);
                    }
                    15 => {
                        assert!(this.mode == ViewMode::Source);
                        assert_eq!(this.editor.read(cx).value().as_str(), "# Saved");
                        assert!(this.notebooks.tree_focus.is_focused(window));
                        this.set_mode(ViewMode::LivePreview, window, cx);
                        this.notebooks.tree_focus.focus(window);
                        let root = this.notebooks.rows.iter().position(|r| r.label == "root.md").unwrap();
                        this.notebooks.cursor = Some(root - 1);
                        this.tree_move(true, window, cx);
                    }
                    16 => {
                        assert!(this.mode == ViewMode::LivePreview);
                        assert_eq!(this.editor.read(cx).value().as_str(), "# Root");
                        assert!(this.notebooks.tree_focus.is_focused(window));
                        this.set_mode(ViewMode::Reading, window, cx);
                        this.notebooks.tree_focus.focus(window);
                        let nested = this.notebooks.rows.iter().position(|r| r.label == "nested.md").unwrap();
                        this.notebooks.cursor = Some(nested + 1);
                        this.tree_move(false, window, cx);
                    }
                    17 => {
                        assert_eq!(this.reading_text.as_str(), "# Saved");
                        assert!(this.notebooks.tree_focus.is_focused(window));
                        let root = this.notebooks.rows.iter().position(|r| r.label == "root.md").unwrap();
                        this.notebooks.cursor = Some(root - 1);
                        this.tree_move(true, window, cx);
                        // Move again before the first asynchronous read can complete.
                        let nested = this.notebooks.rows.iter().position(|r| r.label == "nested.md").unwrap();
                        this.notebooks.cursor = Some(nested + 1);
                        this.tree_move(false, window, cx);
                    }
                    18 => {
                        assert_eq!(this.reading_text.as_str(), "# Saved");
                        assert!(this.notebooks.tree_focus.is_focused(window));
                        this.editor.update(cx, |editor, cx| editor.set_value("unsaved", window, cx));
                        this.answer_request(None, Next::TreeFile(first.join("root.md")), window, cx);
                        assert!(this.dirty(cx));
                        assert_eq!(this.notebooks.rows[this.notebooks.cursor.unwrap()].label, "nested.md");
                        this.answer_request(Some(1), Next::TreeFile(first.join("root.md")), window, cx);
                    }
                    19 => {
                        assert_eq!(this.reading_text.as_str(), "# Root");
                        assert!(this.notebooks.tree_focus.is_focused(window));
                    }
                    _ => unreachable!(),
                }
                cx.notify();
            })).unwrap();
            step += 1;
        }
        let pending = notes.update(cx, |this, _| this.notebooks.persistence.take()).unwrap();
        if let Some(pending) = pending { pending.await; }
        let restored = notebook::load_index(&index).unwrap();
        assert_eq!(restored.notebooks.len(), 2);
        assert_eq!(restored.active, Some(fs::canonicalize(&first).unwrap()));
        fs::write(output, "passed: modal screens, creation, registration, recursive sidebar, arrow loading in all view modes, retained focus, rapid navigation, save/discard/cancel responses, missing notebook preservation, persistence and restoration\n").unwrap();
        drop(fixture);
        cx.update(|_, cx| cx.quit()).unwrap();
    }).detach();
}
