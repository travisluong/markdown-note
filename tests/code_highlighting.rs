//! GPUI TestPlatform only: no native windows, OS input, or desktop interaction.
use gpui::{
    Context, Entity, Focusable, IntoElement, Render, TestAppContext, Window, div, prelude::*,
};
use gpui_component::{
    ActiveTheme, Root, Theme, ThemeMode,
    input::{Input, InputState, Redo, Undo},
};
struct CodeView(Entity<InputState>);
impl Render for CodeView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(gpui::px(220.))
            .h_full()
            .child(Input::new(&self.0).h_full())
    }
}
#[gpui::test]
fn code_colors_survive_editing_wrapping_themes_and_history(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let source = "```rust\nfn main() { let café = \"**literal**\"; }\n```\n";
    let mut editor = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let input = cx.new(|cx| {
            let mut state = InputState::new(window, cx)
                .multi_line(true)
                .searchable(true)
                .default_value(source);
            state.set_live_preview(true, cx);
            state.focus(window, cx);
            state
        });
        editor = Some(input.clone());
        let view = cx.new(|_| CodeView(input));
        Root::new(view, window, cx)
    });
    let editor = editor.unwrap();
    for mode in [ThemeMode::Light, ThemeMode::Dark] {
        cx.update(|window, cx| {
            Theme::change(mode, Some(window), cx);
            editor.update(cx, |state, cx| {
                state.diagnostic_settle_preview();
                state.select_source_range(8..10, window, cx);
                cx.notify();
            });
        });
        cx.refresh().unwrap();
        cx.run_until_parked();
        cx.refresh().unwrap();
        cx.update(|_, cx| {
            let runs = editor.read(cx).diagnostic_code_runs(1);
            let expected = cx
                .theme()
                .highlight_theme
                .style("keyword")
                .unwrap()
                .color
                .unwrap();
            assert!(
                runs.iter()
                    .any(|(r, color)| r.contains(&0) && *color == expected),
                "{runs:?}"
            );
            let value = editor.read(cx).value();
            assert_eq!(value.as_ref(), source);
            for (r, _) in runs {
                assert!(source.lines().nth(1).unwrap().is_char_boundary(r.start));
                assert!(source.lines().nth(1).unwrap().is_char_boundary(r.end));
            }
        });
    }
    cx.update(|window, cx| {
        editor.update(cx, |state, cx| {
            state.select_source_range(3..7, window, cx);
            state.replace_selection_isolated("unknown", window, cx);
            state.diagnostic_settle_preview();
            cx.notify();
        });
    });
    cx.run_until_parked();
    cx.refresh().unwrap();
    cx.update(|_, cx| {
        let runs = editor.read(cx).diagnostic_code_runs(1);
        let keyword = cx
            .theme()
            .highlight_theme
            .style("keyword")
            .unwrap()
            .color
            .unwrap();
        assert!(
            runs.iter()
                .any(|(r, color)| r.contains(&0) && *color != keyword),
            "unknown language must clear old colors"
        );
    });
    cx.dispatch_action(Undo);
    cx.update(|_, cx| assert_eq!(editor.read(cx).value().as_ref(), source));
    cx.dispatch_action(Redo);
    cx.update(|_, cx| assert!(editor.read(cx).value().starts_with("```unknown")));
    cx.update(|_, cx| editor.update(cx, |state, cx| state.set_live_preview(false, cx)));
    cx.refresh().unwrap();
    cx.run_until_parked();
    cx.update(|_, cx| assert!(editor.read(cx).diagnostic_code_runs(1).is_empty()));
}

#[gpui::test]
fn inactive_blocks_reveal_by_pointer_selection_and_history(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let source = "before\n```rust\nlet café = 1;\n// second\n```\nbetween\n~~~\n~~~\nafter\ntail\ntail\ntail\ntail\ntail\ntail\ntail\ntail\ntail\ntail\ntail\ntail\ntail\ntail\ntail\ntail\ntail\ntail\ntail\ntail\ntail\ntail\ntail\ntail\ntail\ntail\ntail\ntail\ntail\ntail\ntail\ntail\ntail\ntail\ntail\ntail\ntail\ntail\ntail\ntail\ntail\ntail\ntail\ntail\ntail\nend";
    let mut editor = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let input = cx.new(|cx| {
            let mut state = InputState::new(window, cx)
                .multi_line(true)
                .searchable(true)
                .default_value(source);
            state.set_live_preview(true, cx);
            state.focus(window, cx);
            state.select_source_range(0..0, window, cx);
            state.diagnostic_settle_preview();
            state
        });
        editor = Some(input.clone());
        let view = cx.new(|_| CodeView(input));
        Root::new(view, window, cx)
    });
    let editor = editor.unwrap();
    cx.refresh().unwrap();
    cx.run_until_parked();
    cx.refresh().unwrap();
    for theme in [ThemeMode::Light, ThemeMode::Dark] {
        cx.update(|window, cx| Theme::change(theme, Some(window), cx));
        cx.refresh().unwrap();
        cx.update(|_, cx| {
            let state = editor.read(cx);
            assert!(state.diagnostic_code_row(1).1);
            assert!(state.diagnostic_code_row(1).3 > 1.);
            assert_eq!(state.diagnostic_code_row(2).2, "let café = 1;");
            assert!(state.diagnostic_code_row(2).0);
            assert!(state.diagnostic_code_row(4).1);
            assert!(
                state.diagnostic_code_row(7).1,
                "empty block retains a clickable row"
            );
            assert_eq!(state.diagnostic_code_row(7).2, "~~~");
            assert_eq!(state.value().as_ref(), source);
        });
    }
    // Horizontal keyboard entry reveals a hidden opening fence, and exit hides only its text.
    cx.update(|window, cx| {
        editor.update(cx, |state, cx| state.select_source_range(6..6, window, cx))
    });
    cx.dispatch_action(gpui_component::input::MoveRight);
    cx.refresh().unwrap();
    cx.update(|_, cx| assert!(!editor.read(cx).diagnostic_code_row(1).1));
    cx.dispatch_action(gpui_component::input::MoveLeft);
    cx.refresh().unwrap();
    cx.update(|_, cx| assert!(editor.read(cx).diagnostic_code_row(1).1));
    cx.update(|_, cx| {
        let state = editor.read(cx);
        let p = state.diagnostic_code_point(2, 0);
        assert_eq!(
            state
                .drop_offset(p + gpui::point(gpui::px(-6.), gpui::px(2.)))
                .unwrap(),
            source.find("let").unwrap()
        );
        let p = state.diagnostic_code_point(5, 0);
        assert_eq!(
            state
                .drop_offset(p + gpui::point(gpui::px(0.), gpui::px(2.)))
                .unwrap(),
            source.find("between").unwrap()
        );
    });
    let clicked = source.find("café").unwrap();
    let before = cx.update(|window, cx| {
        editor.update(cx, |state, cx| {
            let p = state.diagnostic_code_point(2, 4);
            state.diagnostic_code_click(p + gpui::point(gpui::px(0.1), gpui::px(2.)), window, cx);
            assert_eq!(state.cursor(), clicked);
            p
        })
    });
    cx.refresh().unwrap();
    cx.run_until_parked();
    cx.refresh().unwrap();
    cx.update(|_, cx| {
        let state = editor.read(cx);
        for row in 1..=4 {
            assert!(state.diagnostic_code_row(row).0);
        }
        assert_eq!(state.diagnostic_code_row(1).2, "```rust");
        assert_eq!(state.diagnostic_code_row(4).2, "```");
        assert!(
            (state.diagnostic_code_point(2, 4).y - before.y).abs() < gpui::px(1.),
            "before {before:?}, after {:?}",
            state.diagnostic_code_point(2, 4)
        );
        assert!(state.diagnostic_code_row(6).1);
    });
    // Moving a selection across both blocks reveals both, including the fences.
    cx.update(|window, cx| {
        editor.update(cx, |state, cx| {
            state.select_source_range(clicked..source.rfind("~~~").unwrap() + 3, window, cx)
        })
    });
    cx.refresh().unwrap();
    cx.update(|_, cx| {
        let state = editor.read(cx);
        assert!(state.diagnostic_code_row(1).0);
        assert!(state.diagnostic_code_row(6).0);
    });
    cx.update(|window, cx| {
        editor.update(cx, |state, cx| state.select_source_range(0..0, window, cx))
    });
    cx.refresh().unwrap();
    cx.update(|window, cx| {
        editor.update(cx, |state, cx| {
            let p = state.diagnostic_code_point(7, 0);
            state.diagnostic_code_click(p + gpui::point(gpui::px(0.), gpui::px(2.)), window, cx);
            assert_eq!(state.cursor(), source.rfind("~~~").unwrap());
        })
    });
    cx.refresh().unwrap();
    cx.update(|window, cx| {
        editor.update(cx, |state, cx| {
            assert!(state.diagnostic_code_row(6).0);
            state.replace_selection_isolated("code\n", window, cx);
            state.diagnostic_settle_preview();
        })
    });
    cx.dispatch_action(Undo);
    cx.update(|_, cx| assert_eq!(editor.read(cx).value().as_ref(), source));
    cx.dispatch_action(Redo);
    cx.update(|_, cx| assert!(editor.read(cx).value().contains("~~~\ncode\n~~~")));
    cx.update(|_, cx| editor.update(cx, |state, cx| state.set_live_preview(false, cx)));
    cx.refresh().unwrap();
    cx.update(|_, cx| assert!(!editor.read(cx).diagnostic_code_row(1).0));
    cx.update(|window, cx| {
        editor.update(cx, |state, cx| {
            state.set_live_preview(true, cx);
            state.diagnostic_settle_preview();
            state.open_search(window, cx);
        })
    });
    cx.update(|window, cx| {
        let query = editor.read(cx).diagnostic_find(cx).unwrap().0;
        query.update(cx, |state, cx| state.set_value("second", window, cx));
    });
    cx.run_until_parked();
    cx.refresh().unwrap();
    cx.update(|window, cx| {
        let state = editor.read(cx);
        let (query, count, _, selected) = state.diagnostic_find(cx).unwrap();
        assert_eq!(count, 1);
        assert_eq!(selected, "second");
        assert!(query.focus_handle(cx).is_focused(window));
        assert!(
            !state.diagnostic_code_row(1).1,
            "search reveals code while the query owns focus"
        );
        assert!(!state.diagnostic_code_row(4).1);
    });
}

#[gpui::test]
fn cursor_and_selection_preserve_code_geometry(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let mut editor = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let input = cx.new(|cx| {
            let mut state = InputState::new(window, cx).multi_line(true);
            state.set_live_preview(true, cx);
            state.focus(window, cx);
            state
        });
        editor = Some(input.clone());
        let view = cx.new(|_| CodeView(input));
        Root::new(view, window, cx)
    });
    let editor = editor.unwrap();
    for source in [
        "before\n```rust\nlet café = 1;\n```\nafter",
        "before\r\n~~~~ rust\r\n你好\r\n~~~~\r\nafter",
        "before\n> ```rust\n> 你好\n> ```\nafter",
        "before\n- item\n\n  ```rust\n  你好\n  ```\nafter",
        "before\n```\n```\nafter",
        "before\n```rust\n你好",
        "before\n~~~",
        "before\n```rust very long language information with many words that must wrap identically even when hidden\n你好\n```\nafter",
    ] {
        for theme in [ThemeMode::Light, ThemeMode::Dark] {
            cx.update(|window, cx| {
                Theme::change(theme, Some(window), cx);
                editor.update(cx, |state, cx| {
                    state.set_value(source, window, cx);
                    state.select_source_range(0..0, window, cx);
                    state.diagnostic_settle_preview();
                    cx.notify();
                });
            });
            cx.run_until_parked();
            cx.refresh().unwrap();
            cx.run_until_parked();
            cx.refresh().unwrap();
            let geometry = |state: &InputState| {
                (0..source.lines().count())
                    .map(|row| {
                        let (decorated, _, text, height) = state.diagnostic_code_row(row);
                        (decorated, text, height, state.diagnostic_code_point(row, 0))
                    })
                    .collect::<Vec<_>>()
            };
            let baseline = cx.update(|_, cx| geometry(editor.read(cx)));
            // Every source character boundary exercises opening/content/closing
            // fences, navigation out of the block, and Unicode caret positions.
            let mut selections = source
                .char_indices()
                .map(|(at, _)| at..at)
                .collect::<Vec<_>>();
            selections.extend([source.len()..source.len(), 0..source.len(), 0..0]);
            for selection in selections {
                cx.update(|window, cx| {
                    editor.update(cx, |state, cx| {
                        state.select_source_range(selection.clone(), window, cx);
                    })
                });
                cx.refresh().unwrap();
                cx.run_until_parked();
                cx.refresh().unwrap();
                cx.update(|_, cx| {
                    let state = editor.read(cx);
                    for (actual, expected) in geometry(state).iter().zip(&baseline) {
                        assert_eq!(
                            (&actual.0, &actual.1, &actual.2),
                            (&expected.0, &expected.1, &expected.2),
                            "{source:?}, {selection:?}"
                        );
                        // Treap sums can differ by a few floating point ULPs.
                        assert!((actual.3.x - expected.3.x).abs() < gpui::px(0.001));
                        assert!(
                            (actual.3.y - expected.3.y).abs() < gpui::px(0.001),
                            "{source:?}, {selection:?}: {actual:?}, {expected:?}"
                        );
                    }
                    assert_eq!(state.value().as_ref(), source);
                });
            }
        }
    }
}
