//! Existing editor transaction diagnostics on GPUI TestPlatform.
//! No native window, desktop input, benchmark driver or user document is used.
use gpui::{Context, Entity, IntoElement, Render, TestAppContext, Window, div, prelude::*};
use gpui_component::{
    Root,
    input::{Input, InputState},
};

struct EditorView(Entity<InputState>);

impl Render for EditorView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(Input::new(&self.0).h_full())
    }
}

fn run_diagnostics(
    cx: &mut TestAppContext,
    check: fn(&mut InputState, &mut Window, &mut Context<InputState>),
) {
    cx.update(gpui_component::init);
    let mut editor = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let input = cx.new(|cx| InputState::new(window, cx).multi_line(true));
        editor = Some(input.clone());
        let view = cx.new(|_| EditorView(input));
        Root::new(view, window, cx)
    });
    let editor = editor.unwrap();
    cx.update(|window, cx| editor.update(cx, |state, cx| check(state, window, cx)));
    cx.run_until_parked();
}

#[gpui::test]
fn list_task_and_selection_transactions(cx: &mut TestAppContext) {
    // Source/Live nesting and outdenting, list/task Enter, reversed Unicode
    // selections, isolated replacement, mode/history retention, stale tasks,
    // IME replacement, disabled edits, and undo/redo.
    run_diagnostics(cx, InputState::diagnostic_checks);
}

#[gpui::test]
fn table_commands_navigation_and_input_transactions(cx: &mut TestAppContext) {
    // Row/column/alignment/table commands, stale targets, isolated undo/redo,
    // escaped pipes and pasted newlines, Unicode IME, Tab navigation/row creation,
    // selection retention across modes, and disabled commands/input.
    run_diagnostics(cx, |editor, window, cx| {
        editor.diagnostic_table_checks(window, cx, |_| {});
    });
}

#[cfg(feature = "editor-diagnostics")]
#[gpui::test]
fn list_drag_uses_rendered_markers_and_one_undo_transaction(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let mut editor = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let input = cx.new(|cx| {
            let mut state = InputState::new(window, cx)
                .multi_line(true)
                .default_value("- [ ] café\n\n* other\n");
            state.set_live_preview(true, cx);
            state.focus(window, cx);
            state
        });
        editor = Some(input.clone());
        let view = cx.new(|_| EditorView(input));
        Root::new(view, window, cx)
    });
    cx.run_until_parked();
    cx.refresh().unwrap();
    cx.run_until_parked();
    cx.update(|window, cx| {
        editor.unwrap().update(cx, |state, cx| {
            state.diagnostic_list_drag_checks(window, cx)
        })
    });
}

#[cfg(feature = "editor-diagnostics")]
#[gpui::test]
fn list_drop_keeps_scrolled_viewport_with_offscreen_caret(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let mut editor = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let input = cx.new(|cx| {
            let source: String = (0..150)
                .map(|row| format!("{}- [ ] item {row}\n", if row == 44 { "  " } else { "" }))
                .collect();
            let mut state = InputState::new(window, cx)
                .multi_line(true)
                .default_value(source);
            state.set_live_preview(true, cx);
            state.focus(window, cx);
            state
        });
        editor = Some(input.clone());
        let view = cx.new(|_| EditorView(input));
        Root::new(view, window, cx)
    });
    cx.run_until_parked();
    for phase in 0..3 {
        cx.refresh().unwrap();
        cx.run_until_parked();
        cx.update(|window, cx| {
            editor.as_ref().unwrap().update(cx, |state, cx| {
                state.diagnostic_scrolled_list_drag(phase, window, cx)
            })
        });
        cx.run_until_parked();
    }
}
