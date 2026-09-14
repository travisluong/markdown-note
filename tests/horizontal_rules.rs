//! TestPlatform only; no native windows or desktop input.
use gpui::{Context, Entity, Render, TestAppContext, Window, div, prelude::*, px};
use gpui_component::{
    Root, Theme, ThemeMode,
    input::{Enter, Input, InputState, Redo, Undo},
};

struct RuleView(Entity<InputState>);
impl Render for RuleView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(220.))
            .h_full()
            .child(Input::new(&self.0).h_full())
    }
}

#[gpui::test]
fn rules_reveal_without_geometry_changes_and_keep_source_mapping(cx: &mut TestAppContext) {
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
        let view = cx.new(|_| RuleView(input));
        Root::new(view, window, cx)
    });
    let editor = editor.unwrap();
    for marker in [
        "---",
        "***",
        "___",
        "  - - -",
        "> > ---",
        "  ***",
        "- - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - - -",
    ] {
        for theme in [ThemeMode::Light, ThemeMode::Dark] {
            let prefix = if marker == "  ***" {
                "- café\r\n\r\n"
            } else {
                "café 你好\r\n\r\n"
            };
            let source = format!("{prefix}{marker}\r\n\r\nafter");
            let at = prefix.len();
            cx.update(|window, cx| {
                Theme::change(theme, Some(window), cx);
                editor.update(cx, |state, cx| {
                    state.set_value(&source, window, cx);
                    state.select_source_range(0..0, window, cx);
                    state.diagnostic_settle_preview();
                    cx.notify();
                });
            });
            cx.run_until_parked();
            cx.refresh().unwrap();
            let baseline = cx.update(|_, cx| {
                let state = editor.read(cx);
                let (hidden, bounds) = state.diagnostic_divider_row(2).unwrap();
                assert!(hidden);
                let bounds = bounds.unwrap();
                assert_eq!(bounds.size.height, px(2.));
                assert!(bounds.size.width > px(100.));
                if marker.starts_with([' ', '>']) {
                    assert!(bounds.origin.x > px(0.));
                }
                (
                    state.diagnostic_code_row(2),
                    state.diagnostic_code_point(4, 0),
                )
            });
            for column in 0..=marker.len() {
                cx.update(|window, cx| {
                    editor.update(cx, |state, cx| {
                        state.select_source_range(at + column..at + column, window, cx);
                    })
                });
                cx.refresh().unwrap();
                cx.update(|_, cx| {
                    let state = editor.read(cx);
                    assert_eq!(state.diagnostic_divider_row(2), Some((false, None)));
                    assert_eq!(state.diagnostic_code_row(2), baseline.0);
                    assert!((state.diagnostic_code_point(4, 0).y - baseline.1.y).abs() < px(0.001));
                    assert_eq!(state.value().as_ref(), source);
                    assert_eq!(state.cursor(), at + column);
                });
            }
            cx.update(|window, cx| {
                editor.update(cx, |state, cx| state.select_source_range(0..0, window, cx))
            });
            cx.refresh().unwrap();
            cx.update(|window, cx| {
                let state = editor.read(cx);
                let (_, bounds) = state.diagnostic_divider_row(2).unwrap();
                let bounds = bounds.unwrap();
                let position = state.diagnostic_code_point(2, 0)
                    + gpui::point(bounds.size.width * 0.7, bounds.origin.y);
                editor.update(cx, |state, cx| {
                    state.diagnostic_code_click(position, window, cx)
                });
            });
            cx.refresh().unwrap();
            cx.update(|_, cx| {
                let state = editor.read(cx);
                assert!((at..=at + marker.len() + 1).contains(&state.cursor()));
                assert_eq!(state.diagnostic_divider_row(2), Some((false, None)));
            });
        }
    }
}

#[gpui::test]
fn rule_edits_history_and_modes_preserve_the_markdown(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let source = "before\n\n---\n\nafter";
    let mut editor = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let input = cx.new(|cx| {
            let mut state = InputState::new(window, cx)
                .multi_line(true)
                .default_value(source);
            state.set_live_preview(true, cx);
            state.focus(window, cx);
            state
        });
        editor = Some(input.clone());
        let view = cx.new(|_| RuleView(input));
        Root::new(view, window, cx)
    });
    let editor = editor.unwrap();
    cx.update(|window, cx| {
        editor.update(cx, |state, cx| {
            state.select_source_range(8..11, window, cx);
            state.replace_selection_isolated("--", window, cx);
            state.diagnostic_settle_preview();
        })
    });
    cx.run_until_parked();
    cx.refresh().unwrap();
    cx.update(|_, cx| assert!(editor.read(cx).diagnostic_divider_row(2).is_none()));
    cx.dispatch_action(Undo);
    cx.update(|_, cx| editor.update(cx, |state, _| state.diagnostic_settle_preview()));
    cx.refresh().unwrap();
    cx.update(|_, cx| {
        assert_eq!(editor.read(cx).value().as_ref(), source);
        assert!(editor.read(cx).diagnostic_divider_row(2).is_some());
    });
    cx.dispatch_action(Redo);
    cx.update(|_, cx| editor.update(cx, |state, _| state.diagnostic_settle_preview()));
    cx.refresh().unwrap();
    cx.update(|_, cx| {
        assert_eq!(editor.read(cx).value().as_ref(), "before\n\n--\n\nafter");
        assert!(editor.read(cx).diagnostic_divider_row(2).is_none());
    });
    cx.dispatch_action(Undo);
    for live in [false, true] {
        cx.update(|_, cx| {
            editor.update(cx, |state, cx| {
                state.set_live_preview(live, cx);
                state.diagnostic_settle_preview();
            })
        });
        cx.refresh().unwrap();
        cx.update(|_, cx| {
            let state = editor.read(cx);
            assert_eq!(state.value().as_ref(), source);
            assert_eq!(state.diagnostic_divider_row(2).is_some(), live);
        });
    }
    cx.update(|window, cx| {
        editor.update(cx, |state, cx| {
            state.select_source_range(11..11, window, cx);
            state.diagnostic_settle_preview();
        })
    });
    for count in 1..=4 {
        cx.dispatch_action(Enter { secondary: false });
        cx.refresh().unwrap();
        cx.update(|_, cx| {
            let state = editor.read(cx);
            assert!(state.diagnostic_divider_row(2).unwrap().0);
            assert_eq!(
                state.value().as_ref(),
                format!("before\n\n---{}\n\nafter", "\n".repeat(count))
            );
        });
    }
}
