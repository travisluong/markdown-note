//! GPUI TestPlatform only: no native windows, OS input, or desktop interaction.
use gpui::{Context, Entity, IntoElement, Render, TestAppContext, Window, div, prelude::*};
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
