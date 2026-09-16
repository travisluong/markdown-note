//! GPUI TestPlatform only; no native window or desktop input.
use gpui::{Context, Entity, Render, TestAppContext, Window, div, prelude::*, px};
use gpui_component::{
    Root, Theme, ThemeMode,
    input::{Input, InputState, Redo, Undo},
};

struct StrikeView(Entity<InputState>);
impl Render for StrikeView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(220.))
            .h_full()
            .child(Input::new(&self.0).h_full())
    }
}

#[gpui::test]
fn strikethrough_renders_through_reveal_themes_edits_and_history(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let source = "intro\n~~café **茶** and `code` with enough text to wrap~~ tail\n`~~literal~~`\n";
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
        let view = cx.new(|_| StrikeView(input));
        Root::new(view, window, cx)
    });
    let editor = editor.unwrap();
    for mode in [ThemeMode::Light, ThemeMode::Dark] {
        for cursor in [0, 10] {
            cx.update(|window, cx| {
                Theme::change(mode, Some(window), cx);
                editor.update(cx, |state, cx| {
                    state.diagnostic_settle_preview();
                    state.select_source_range(cursor..cursor, window, cx);
                    cx.notify();
                });
            });
            cx.run_until_parked();
            cx.refresh().unwrap();
            cx.update(|_, cx| {
                let state = editor.read(cx);
                let row = source.lines().nth(1).unwrap();
                let runs = state.diagnostic_strikethrough_runs(1);
                for label in ["café", "茶", "code", "wrap"] {
                    assert!(
                        runs.iter()
                            .any(|range| range.contains(&row.find(label).unwrap())),
                        "{label}: {runs:?}"
                    );
                }
                for label in ["~~", "tail"] {
                    assert!(
                        !runs
                            .iter()
                            .any(|range| range.contains(&row.find(label).unwrap()))
                    );
                }
                assert!(state.diagnostic_strikethrough_runs(2).is_empty());
                assert_eq!(state.value().as_ref(), source);
                let text = state.diagnostic_code_row(1).2;
                assert_eq!(text.contains("~~"), cursor == 10);
            });
        }
    }
    cx.update(|window, cx| {
        editor.update(cx, |state, cx| {
            state.select_source_range(6..8, window, cx);
            state.replace_selection_isolated("", window, cx);
        })
    });
    for expected in [false, true, false] {
        cx.update(|_, cx| {
            editor.update(cx, |state, cx| {
                state.diagnostic_settle_preview();
                cx.notify();
            })
        });
        cx.run_until_parked();
        cx.refresh().unwrap();
        cx.update(|_, cx| {
            assert_eq!(
                !editor.read(cx).diagnostic_strikethrough_runs(1).is_empty(),
                expected
            )
        });
        if expected {
            cx.dispatch_action(Redo);
        } else {
            cx.dispatch_action(Undo);
        }
    }
    cx.update(|_, cx| editor.update(cx, |state, cx| state.set_live_preview(false, cx)));
    cx.run_until_parked();
    cx.refresh().unwrap();
    cx.update(|_, cx| assert!(editor.read(cx).diagnostic_strikethrough_runs(1).is_empty()));
}
