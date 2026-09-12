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

// All pointer events below are dispatched only to GPUI's in-memory TestPlatform.
#[cfg(feature = "editor-diagnostics")]
#[gpui::test]
async fn leaving_image_line_preserves_clicked_text(cx: &mut TestAppContext) {
    use gpui::{Modifiers, point, px};
    use gpui_component::input::Position;
    cx.update(gpui_component::init);
    let fixture = tempfile::tempdir().unwrap();
    image::RgbaImage::from_pixel(200, 150, image::Rgba([255, 0, 0, 255]))
        .save(fixture.path().join("a.png"))
        .unwrap();
    let mut editor = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let input = cx.new(|cx| InputState::new(window, cx).multi_line(true));
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
    for missing in [false, true] {
        for wrapped in [false, true] {
            for scroll_case in 0..3 {
                for below in [false, true] {
                    let alt = if wrapped {
                        "long alt text ".repeat(30)
                    } else {
                        "a".into()
                    };
                    let url = if missing { "missing.png" } else { "a.png" };
                    let image_row = if scroll_case == 2 { 80 } else { 12 };
                    let trailing_rows = if scroll_case == 2 { 4 } else { 100 };
                    let text = format!(
                        "{}![{alt}]({url})\n{}",
                        (0..image_row)
                            .map(|i| if i + 1 == image_row {
                                "\n".into()
                            } else {
                                format!("before {i}\n")
                            })
                            .collect::<String>(),
                        (0..trailing_rows)
                            .map(|i| if i == 0 {
                                "\n".into()
                            } else {
                                format!("after {i}\n")
                            })
                            .collect::<String>()
                    );
                    let target_row = if below { image_row + 2 } else { image_row - 2 };
                    let start = text
                        .split_inclusive('\n')
                        .take(target_row)
                        .map(str::len)
                        .sum::<usize>();
                    let end = start + text.lines().nth(target_row).unwrap().len();
                    cx.update(|window, cx| {
                        editor.update(cx, |state, cx| {
                            state.set_value(&text, window, cx);
                            state.set_image_document(Some(fixture.path().join("note.md")), cx);
                            state.set_live_preview(true, cx);
                            state.diagnostic_settle_preview();
                            state.set_cursor_position(
                                Position::new(image_row as u32, 0),
                                window,
                                cx,
                            );
                        })
                    });
                    cx.run_until_parked();
                    cx.refresh().unwrap();
                    cx.run_until_parked();
                    cx.update(|_, cx| {
                        editor.update(cx, |state, cx| state.diagnostic_scroll(100000., cx))
                    });
                    cx.run_until_parked();
                    if scroll_case != 0 {
                        cx.update(|_, cx| {
                            editor.update(cx, |state, cx| {
                                state.diagnostic_scroll(
                                    if scroll_case == 2 { -100000. } else { -100. },
                                    cx,
                                )
                            })
                        });
                        cx.run_until_parked();
                    }
                    let locate = |state: &InputState| -> Option<i32> {
                        (0..1000).find(|y| {
                            state
                                .drop_offset(point(px(50.), px(*y as f32)))
                                .is_some_and(|offset| offset >= start && offset < end)
                        })
                    };
                    let before = cx
                        .update(|_, cx| locate(editor.read(cx)))
                        .expect("target visible");
                    let click = point(px(50.), px(before as f32 + 4.));
                    let expected = cx.update(|_, cx| editor.read(cx).drop_offset(click).unwrap());
                    cx.simulate_click(click, Modifiers::default());
                    cx.run_until_parked();
                    cx.update(|_, cx| {
                        let state = editor.read(cx);
                        assert_eq!(state.cursor(), expected);
                        let after = locate(state).unwrap();
                        if scroll_case == 1 || (scroll_case == 0 && !below) || (scroll_case == 2 && below) {
                            assert!((after - before).abs() <= 1,
                                "jump: {before} -> {after}, missing={missing}, wrapped={wrapped}, below={below}, scroll_case={scroll_case}");
                        } else {
                            // Scroll limits allow only the movement caused by collapsing source.
                            if scroll_case == 0 { assert!(after < before); } else { assert!(after >= before); }
                        }
                    });
                    let settled = cx.update(|_, cx| locate(editor.read(cx)));
                    cx.simulate_click(
                        point(px(50.), px(settled.unwrap() as f32 + 4.)),
                        Modifiers::default(),
                    );
                    cx.run_until_parked();
                    cx.update(|_, cx| assert_eq!(locate(editor.read(cx)), settled));
                    for _ in 0..3 {
                        cx.refresh().unwrap();
                        cx.run_until_parked();
                        cx.update(|_, cx| assert_eq!(locate(editor.read(cx)), settled));
                    }
                }
            }
        }
    }
}
