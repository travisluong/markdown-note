//! Uses GPUI's in-memory TestPlatform, never a native window or OS input.
use gpui::{Context, Entity, IntoElement, Render, TestAppContext, Window, div, prelude::*};
use gpui_component::{
    Root,
    input::{Input, InputState, Redo, SelectAll, Undo},
};
struct EditorView {
    editor: Entity<InputState>,
    disabled: bool,
}
impl Render for EditorView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .child(Input::new(&self.editor).h_full().disabled(self.disabled))
    }
}
#[gpui::test]
fn image_insertion_isolated_history_and_stale_snapshot(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let mut editor = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .multi_line(true)
                .default_value("hello 世界")
        });
        editor = Some(input.clone());
        let view = cx.new(|_| EditorView {
            editor: input,
            disabled: false,
        });
        Root::new(view, window, cx)
    });
    let editor = editor.unwrap();
    for live in [false, true] {
        cx.update(|window, cx| {
            editor.update(cx, |state, cx| {
                state.set_value("hello 世界", window, cx);
                state.set_live_preview(live, cx);
                state.focus(window, cx);
            })
        });
        cx.dispatch_action(SelectAll);
        let snapshot = cx.update(|_, cx| editor.read(cx).snapshot());
        cx.update(|window, cx| {
            editor.update(cx, |state, cx| {
                assert!(!state.insert_at_snapshot(
                    7,
                    &snapshot,
                    "invalid UTF-8 boundary",
                    window,
                    cx
                ));
                assert!(state.insert_at_snapshot(
                    6,
                    &snapshot,
                    "\n\n![a](a.png)\n\n![b](b.png)\n\n",
                    window,
                    cx
                ));
                assert!(state.value().ends_with("世界"));
                assert!(!state.insert_at_snapshot(0, &snapshot, "stale", window, cx));
            })
        });
        cx.run_until_parked();
        cx.dispatch_action(Undo);
        cx.update(|_, cx| assert_eq!(editor.read(cx).value().as_ref(), "hello 世界"));
        cx.dispatch_action(Redo);
        cx.update(|_, cx| assert!(editor.read(cx).value().contains("![b](b.png)")));
    }
}

#[gpui::test]
async fn live_image_height_maps_pointer_to_source(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let fixture = tempfile::tempdir().unwrap();
    image::RgbaImage::from_pixel(200, 300, image::Rgba([255, 0, 0, 255]))
        .save(fixture.path().join("a.png"))
        .unwrap();
    let note = fixture.path().join("note.md");
    let mut editor = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let input = cx.new(|cx| {
            let mut state = InputState::new(window, cx)
                .multi_line(true)
                .default_value("![a](a.png)\n\nafter");
            state.set_image_document(Some(note), cx);
            state.set_live_preview(true, cx);
            state.set_cursor_position(gpui_component::input::Position::new(2, 0), window, cx);
            state
        });
        editor = Some(input.clone());
        let view = cx.new(|_| EditorView {
            editor: input,
            disabled: false,
        });
        Root::new(view, window, cx)
    });
    let editor = editor.unwrap();
    let loaded = cx.update(|_, cx| {
        cx.fetch_asset::<gpui::ImgResourceLoader>(&gpui::Resource::from(
            fixture.path().join("a.png"),
        ))
        .0
    });
    assert!(loaded.await.is_ok());
    cx.update(|_, cx| editor.update(cx, |_, cx| cx.notify()));
    cx.run_until_parked();
    cx.refresh().unwrap();
    cx.run_until_parked();
    cx.update(|_, cx| {
        let state = editor.read(cx);
        let offset = state
            .drop_offset(gpui::point(gpui::px(50.), gpui::px(150.)))
            .unwrap();
        assert!(
            offset <= "![a](a.png)".len(),
            "image hit mapped beyond its source: {offset}"
        );
        assert!(state.value().is_char_boundary(offset));
    });
    cx.update(|window, cx| {
        editor.update(cx, |state, cx| {
            state.set_cursor_position(gpui_component::input::Position::new(0, 0), window, cx);
        })
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        let state = editor.read(cx);
        let source_hit = state
            .drop_offset(gpui::point(gpui::px(50.), gpui::px(19.)))
            .unwrap();
        assert!(source_hit > 0 && source_hit < "![a](a.png)".len());
        let image_hit = state
            .drop_offset(gpui::point(gpui::px(50.), gpui::px(150.)))
            .unwrap();
        assert!(image_hit <= "![a](a.png)".len());
    });
    cx.update(|_, cx| editor.update(cx, |state, cx| state.set_live_preview(false, cx)));
    cx.refresh().unwrap();
    cx.run_until_parked();
    cx.update(|_, cx| {
        let state = editor.read(cx);
        let offset = state
            .drop_offset(gpui::point(gpui::px(50.), gpui::px(150.)))
            .unwrap();
        assert_eq!(offset, state.value().len());
    });
}

#[gpui::test]
fn disabled_editor_rejects_drop_without_editing(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let mut editor = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .multi_line(true)
                .default_value("unchanged")
        });
        editor = Some(input.clone());
        let view = cx.new(|_| EditorView {
            editor: input,
            disabled: true,
        });
        Root::new(view, window, cx)
    });
    let editor = editor.unwrap();
    cx.update(|window, cx| {
        editor.update(cx, |state, cx| {
            let snapshot = state.snapshot();
            assert!(
                state
                    .drop_offset(gpui::point(gpui::px(50.), gpui::px(19.)))
                    .is_none()
            );
            assert!(!state.insert_at_snapshot(0, &snapshot, "![a](a.png)", window, cx));
            assert_eq!(state.value().as_ref(), "unchanged");
        })
    });
}

#[gpui::test]
fn pointer_mapping_after_scroll_stays_in_source_coordinates(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let text = (0..200)
        .map(|i| format!("row {i} 世界 \n"))
        .collect::<String>();
    let mut editor = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let input = cx.new(|cx| {
            let mut state = InputState::new(window, cx)
                .multi_line(true)
                .default_value(text.clone());
            state.set_cursor_position(gpui_component::input::Position::new(199, 0), window, cx);
            state
        });
        editor = Some(input.clone());
        let view = cx.new(|_| EditorView {
            editor: input,
            disabled: false,
        });
        Root::new(view, window, cx)
    });
    let editor = editor.unwrap();
    cx.run_until_parked();
    cx.update(|_, cx| {
        let offset = editor
            .read(cx)
            .drop_offset(gpui::point(gpui::px(50.), gpui::px(40.)))
            .unwrap();
        assert!(offset > text.len() / 2, "scrolled hit returned {offset}");
        assert!(text.is_char_boundary(offset));
    });
}
