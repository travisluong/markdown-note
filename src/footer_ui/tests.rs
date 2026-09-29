//! Footer behavior on GPUI's in-memory TestPlatform; no desktop interaction.
use crate::{Document, Next, Notes, NotesWindow, ViewMode, notebook};
use gpui::{Entity, TestAppContext, VisualTestContext, div, point, prelude::*, px, size};
use gpui_component::{
    Root,
    input::{Redo, Undo},
};
use markdown_note::document_counts::DocumentCounts;
use notebook::{FooterCountMode, FooterSettings};
use std::{fs, time::Duration};

fn fixture<'a>(
    cx: &'a mut TestAppContext,
    name: &str,
    text: &str,
) -> (tempfile::TempDir, Entity<Notes>, &'a mut VisualTestContext) {
    let dir = tempfile::tempdir().unwrap();
    let mut book = notebook::Notebook::open(dir.path(), false).unwrap();
    book.settings.autosave.enabled = false;
    book.settings.view_mode = ViewMode::Source;
    let path = book.root.join(name);
    fs::write(&path, text).unwrap();
    cx.update(gpui_component::init);
    let mut notes = None;
    let (_, view) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| {
            let mut notes = Notes::new(window, cx);
            notes.tabs.set_initial_notebook(book.root.clone());
            notes.notebooks.active = Some(book);
            notes
                .notebooks
                .set_test_persistence_target(dir.path().join("index.json"));
            notes.session_autosave.enabled = false;
            notes.mode = ViewMode::Source;
            notes
        });
        entity.update(cx, |this, cx| {
            this.install_document(Some(Document::open(path).unwrap()), window, cx);
            this.apply_appearance(window, cx);
        });
        notes = Some(entity.clone());
        let root = cx.new(|cx| NotesWindow {
            _subscription: cx.observe(&entity, |_, _, cx| cx.notify()),
            notes: entity,
        });
        Root::new(root, window, cx)
    });
    view.simulate_resize(size(px(1200.), px(800.)));
    settle(view);
    (dir, notes.unwrap(), view)
}

fn settle(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.refresh().unwrap();
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(150));
    cx.run_until_parked();
    // GPUI 0.2.2 retains removed selectors in its alternating debug-bounds maps.
    // Seed zero-size bounds before drawing the real root so only elements in
    // this frame can replace them with visible bounds.
    for _ in 0..2 {
        cx.draw(point(px(0.), px(0.)), size(px(0.), px(0.)), |_, _| {
            div().size_0().children(
                ["app-footer", "footer-word-count", "footer-character-count"]
                    .map(|selector| div().size_0().debug_selector(move || selector.into())),
            )
        });
    }
}

fn footer_visible(selector: &'static str, cx: &mut VisualTestContext) -> bool {
    cx.debug_bounds(selector)
        .is_some_and(|bounds| bounds.size.width > px(0.) && bounds.size.height > px(0.))
}

fn assert_counts(
    notes: &Entity<Notes>,
    mode: FooterCountMode,
    words: usize,
    characters: usize,
    cx: &mut VisualTestContext,
) {
    cx.update(|_, cx| {
        assert_eq!(
            notes.read(cx).active_document_counts(mode),
            Some(DocumentCounts { words, characters }),
        );
    });
}

fn footer_settings(
    notes: &Entity<Notes>,
    update: impl FnOnce(&mut FooterSettings),
    cx: &mut VisualTestContext,
) {
    cx.update(|_, cx| {
        notes.update(cx, |this, cx| {
            let settings = if let Some(book) = &mut this.notebooks.active {
                &mut book.settings.footer
            } else {
                &mut this.session_footer
            };
            update(settings);
            cx.notify();
        });
    });
    settle(cx);
}

fn replace_text(notes: &Entity<Notes>, text: &str, cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            let length = this.editor.text(cx).len();
            this.editor.select(0..length, window, cx);
            this.editor.replace_selection(text, window, cx);
            this.editor.focus(window, cx);
        });
    });
}

#[test]
fn count_labels_handle_zero_one_many_and_pending() {
    assert_eq!(super::count_label(Some(0), "word"), "0 words");
    assert_eq!(super::count_label(Some(1), "word"), "1 word");
    assert_eq!(super::count_label(Some(2), "character"), "2 characters");
    assert_eq!(super::count_label(None, "character"), "— characters");
}

#[gpui::test]
fn document_count_mode_is_independent_of_view_mode_and_selection(cx: &mut TestAppContext) {
    let (_dir, notes, cx) = fixture(cx, "note.md", "# Hello **world**");
    for (count_mode, characters) in [
        (FooterCountMode::Source, 17),
        (FooterCountMode::ReadableText, 11),
    ] {
        footer_settings(&notes, |settings| settings.count_mode = count_mode, cx);
        for view_mode in [ViewMode::Source, ViewMode::LivePreview, ViewMode::Reading] {
            cx.update(|window, cx| {
                notes.update(cx, |this, cx| {
                    this.change_view_mode(view_mode, window, cx);
                    this.editor.select(2..7, window, cx);
                });
            });
            settle(cx);
            assert_counts(&notes, count_mode, 2, characters, cx);
            assert!(footer_visible("footer-word-count", cx));
            assert!(footer_visible("footer-character-count", cx));
        }
    }
    // A view change and readable-text request preserve the source cache.
    assert_counts(&notes, FooterCountMode::Source, 2, 17, cx);
}

#[gpui::test]
fn empty_documents_have_zero_counts_and_plain_text_is_not_markdown(cx: &mut TestAppContext) {
    let (dir, notes, cx) = fixture(cx, "empty.md", "");
    assert_counts(&notes, FooterCountMode::Source, 0, 0, cx);
    footer_settings(
        &notes,
        |settings| settings.count_mode = FooterCountMode::ReadableText,
        cx,
    );
    assert_counts(&notes, FooterCountMode::ReadableText, 0, 0, cx);
    let path = dir.path().join("plain.txt");
    fs::write(&path, "# literal **text**").unwrap();
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.install_document(Some(Document::open(path).unwrap()), window, cx);
        });
    });
    settle(cx);
    assert_counts(&notes, FooterCountMode::Source, 2, 18, cx);
    assert_counts(&notes, FooterCountMode::ReadableText, 2, 18, cx);
}

#[gpui::test]
fn counts_follow_unsaved_edits_undo_redo_and_retain_last_totals_while_pending(
    cx: &mut TestAppContext,
) {
    let (dir, notes, cx) = fixture(cx, "note.md", "one");
    assert_counts(&notes, FooterCountMode::Source, 1, 3, cx);
    replace_text(&notes, "one two", cx);
    cx.run_until_parked();
    assert_counts(&notes, FooterCountMode::Source, 1, 3, cx);
    settle(cx);
    assert_counts(&notes, FooterCountMode::Source, 2, 7, cx);
    cx.dispatch_action(Undo);
    settle(cx);
    assert_counts(&notes, FooterCountMode::Source, 1, 3, cx);
    cx.dispatch_action(Redo);
    settle(cx);
    assert_counts(&notes, FooterCountMode::Source, 2, 7, cx);
    assert_eq!(
        fs::read_to_string(dir.path().join("note.md")).unwrap(),
        "one"
    );
}

#[gpui::test]
fn shared_reading_peer_receives_counts_and_survives_editing_view_closure(cx: &mut TestAppContext) {
    let (dir, notes, cx) = fixture(cx, "note.md", "one");
    let (source, reading) = cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            let source = this.tabs.active.id;
            this.new_tab(window, cx);
            this.request(Next::File(dir.path().join("note.md")), window, cx);
            this.change_view_mode(ViewMode::Reading, window, cx);
            let reading = this.tabs.active.id;
            this.select_tab(source, window, cx);
            (source, reading)
        })
    });
    replace_text(&notes, "one two", cx);
    settle(cx);
    assert_counts(&notes, FooterCountMode::Source, 2, 7, cx);
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            assert_eq!(this.documents.len(), 1);
            assert_eq!(
                this.tabs.get(reading).unwrap().reading_text.as_ref(),
                "one two"
            );
            this.select_tab(reading, window, cx);
            assert_eq!(this.mode, ViewMode::Reading);
            this.close_tab(source, window, cx);
            assert!(this.tabs.get(source).is_none());
            assert_eq!(this.documents.len(), 1);
        });
    });
    settle(cx);
    assert!(!cx.has_pending_prompt());
    assert_counts(&notes, FooterCountMode::Source, 2, 7, cx);
}

#[gpui::test]
fn selecting_tabs_uses_their_counts_and_empty_or_image_tabs_hide_metrics(cx: &mut TestAppContext) {
    let (dir, notes, cx) = fixture(cx, "first.md", "one");
    let second_path = dir.path().join("second.md");
    fs::write(&second_path, "another two words").unwrap();
    let (first, second) = cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            let first = this.tabs.active.id;
            this.new_tab(window, cx);
            this.install_document(Some(Document::open(second_path).unwrap()), window, cx);
            (first, this.tabs.active.id)
        })
    });
    settle(cx);
    assert_counts(&notes, FooterCountMode::Source, 3, 17, cx);
    cx.update(|window, cx| notes.update(cx, |this, cx| this.select_tab(first, window, cx)));
    settle(cx);
    assert_counts(&notes, FooterCountMode::Source, 1, 3, cx);
    cx.update(|window, cx| notes.update(cx, |this, cx| this.new_tab(window, cx)));
    settle(cx);
    assert!(footer_visible("app-footer", cx));
    assert!(!footer_visible("footer-word-count", cx));
    assert!(!footer_visible("footer-character-count", cx));
    let image_path = dir.path().join("image.png");
    image::RgbaImage::new(1, 1).save(&image_path).unwrap();
    let image_path = fs::canonicalize(image_path).unwrap();
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.request(Next::File(image_path), window, cx)
        })
    });
    settle(cx);
    cx.update(|_, cx| {
        assert!(notes.read(cx).image_path().is_some());
        assert_eq!(
            notes
                .read(cx)
                .active_document_counts(FooterCountMode::Source),
            None
        );
    });
    assert!(footer_visible("app-footer", cx));
    assert!(!footer_visible("footer-word-count", cx));
    assert!(!footer_visible("footer-character-count", cx));
    cx.update(|window, cx| notes.update(cx, |this, cx| this.select_tab(second, window, cx)));
    settle(cx);
    assert_counts(&notes, FooterCountMode::Source, 3, 17, cx);
}

#[gpui::test]
fn notebook_switch_restores_counts_with_the_notebooks_own_count_mode(cx: &mut TestAppContext) {
    let (first_dir, notes, cx) = fixture(cx, "first.md", "one");
    let second_dir = tempfile::tempdir().unwrap();
    let mut second = notebook::Notebook::open(second_dir.path(), false).unwrap();
    fs::write(second.root.join("second.md"), "# One **two**").unwrap();
    second.settings.last_file = Some("second.md".into());
    second.settings.autosave.enabled = false;
    second.settings.footer.count_mode = FooterCountMode::ReadableText;
    notebook::save_book(&second).unwrap();
    let first_tab = cx.update(|_, cx| notes.read(cx).tabs.active.id);
    for (root, mode, characters) in [
        (second.root.clone(), FooterCountMode::ReadableText, 7),
        (first_dir.path().to_path_buf(), FooterCountMode::Source, 3),
        (second.root, FooterCountMode::ReadableText, 7),
        (first_dir.path().to_path_buf(), FooterCountMode::Source, 3),
    ] {
        cx.update(|window, cx| {
            notes.update(cx, |this, cx| {
                this.request(
                    Next::Notebook {
                        path: root,
                        create: false,
                    },
                    window,
                    cx,
                );
            });
        });
        settle(cx);
        cx.update(|_, cx| {
            let this = notes.read(cx);
            assert_eq!(this.footer_settings().count_mode, mode);
            if mode == FooterCountMode::Source {
                assert_eq!(this.tabs.active.id, first_tab);
            }
        });
        assert_counts(
            &notes,
            mode,
            if characters == 3 { 1 } else { 2 },
            characters,
            cx,
        );
    }
}

#[gpui::test]
fn footer_spans_workspace_scales_with_zoom_and_hidden_footer_reclaims_height(
    cx: &mut TestAppContext,
) {
    let (_dir, notes, cx) = fixture(cx, "note.md", "one");
    cx.update(|window, cx| notes.update(cx, |this, cx| this.toggle_right_panel(window, cx)));
    settle(cx);
    let baseline = cx.debug_bounds("app-footer").unwrap().size.height;
    for zoom in [100, 150] {
        cx.update(|window, cx| {
            notes.update(cx, |this, cx| {
                let mut appearance = this.appearance();
                appearance.zoom_percent = zoom;
                this.update_appearance(appearance, window, cx);
            });
        });
        settle(cx);
        let footer = cx.debug_bounds("app-footer").unwrap();
        let workspace = cx.debug_bounds("app-workspace").unwrap();
        assert_eq!(footer.left(), workspace.left());
        assert_eq!(footer.right(), workspace.right());
        assert!((footer.top() - workspace.bottom()).abs() <= px(1.));
        assert!((footer.size.height - baseline * f32::from(zoom) / 100.).abs() <= px(1.));
        for selector in ["workspace-sidebar", "right-panel", "content-column"] {
            let panel = cx.debug_bounds(selector).unwrap();
            assert!(
                panel.bottom() <= footer.top() + px(1.),
                "{selector} overlaps footer"
            );
        }
        footer_settings(&notes, |settings| settings.visible = false, cx);
        assert!(!footer_visible("app-footer", cx));
        let expanded = cx.debug_bounds("app-workspace").unwrap();
        assert!(
            (expanded.size.height - workspace.size.height - footer.size.height).abs() <= px(1.)
        );
        footer_settings(&notes, |settings| settings.visible = true, cx);
    }
}

#[gpui::test]
fn word_and_character_visibility_are_independent_and_survive_footer_hiding(
    cx: &mut TestAppContext,
) {
    let (_dir, notes, cx) = fixture(cx, "note.md", "one");
    for (words, characters) in [(true, false), (false, true), (false, false), (true, true)] {
        footer_settings(
            &notes,
            |settings| {
                settings.word_count = words;
                settings.character_count = characters;
            },
            cx,
        );
        assert!(footer_visible("app-footer", cx));
        assert_eq!(footer_visible("footer-word-count", cx), words);
        assert_eq!(footer_visible("footer-character-count", cx), characters);
        footer_settings(&notes, |settings| settings.visible = false, cx);
        assert!(!footer_visible("app-footer", cx));
        footer_settings(&notes, |settings| settings.visible = true, cx);
        assert_eq!(footer_visible("footer-word-count", cx), words);
        assert_eq!(footer_visible("footer-character-count", cx), characters);
    }
}
