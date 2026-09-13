//! Search reveal geometry and focus on TestPlatform; no desktop interaction.
#![cfg(feature = "editor-diagnostics")]
use gpui::{
    Context, Entity, Focusable, IntoElement, Render, TestAppContext, Window, div, prelude::*, px,
};
use gpui_component::{
    Root,
    input::{Input, InputState, Redo, Undo},
};
use std::ops::Range;

struct View {
    editor: Entity<InputState>,
    query: Entity<InputState>,
}
impl Render for View {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .w(px(420.))
            .h(px(280.))
            .child(Input::new(&self.query))
            .child(Input::new(&self.editor).h_full())
    }
}

fn reveal(cx: &mut TestAppContext, live: bool, wrap: bool, content: Option<String>) {
    cx.update(gpui_component::init);
    let content = content.unwrap_or_else(|| format!("{}target 你好😀 end", "café ".repeat(350)));
    let source = format!(
        "near café\n{}\n{}\n{}",
        "paragraph\n".repeat(160),
        content,
        "tail\n".repeat(80)
    );
    let start = source.find("target").unwrap();
    let distant = start..start + "target 你好😀".len();
    let near = 0.."near café".len();
    let mut entities = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let editor = cx.new(|cx| {
            let mut state = InputState::new(window, cx)
                .multi_line(true)
                .soft_wrap(wrap)
                .default_value(source.clone());
            state.set_live_preview(live, cx);
            // The pending target must survive the absence of any previous layout.
            assert!(state.select_source_range_without_focus(distant.clone(), cx));
            state
        });
        let query = cx.new(|cx| InputState::new(window, cx));
        query.update(cx, |query, cx| query.focus(window, cx));
        entities = Some((editor.clone(), query.clone()));
        let view = cx.new(|_| View { editor, query });
        Root::new(view, window, cx)
    });
    let (editor, query) = entities.unwrap();
    let settle = |cx: &mut gpui::VisualTestContext| {
        cx.run_until_parked();
        for _ in 0..3 {
            cx.refresh().unwrap();
            cx.run_until_parked();
        }
    };
    let assert_match = |range: Range<usize>, cx: &mut gpui::VisualTestContext| {
        cx.update(|window, cx| {
            let state = editor.read(cx);
            assert_eq!(state.source_selection(), range);
            assert!(
                state.diagnostic_range_visible(range.clone()),
                "range {range:?} must be visible; live={live}, wrap={wrap}"
            );
            assert!(state.diagnostic_pending_reveal().is_none());
            assert!(query.focus_handle(cx).is_focused(window));
            assert!(!editor.focus_handle(cx).is_focused(window));
            assert_eq!(state.value().as_ref(), source);
        });
    };
    settle(cx);
    assert_match(distant.clone(), cx);
    // Repeated navigation in both directions, including the same destination.
    for range in [near.clone(), distant.clone(), distant.clone(), near.clone()] {
        cx.update(|_, cx| {
            editor.update(cx, |state, cx| {
                assert!(state.select_source_range_without_focus(range.clone(), cx))
            })
        });
        settle(cx);
        assert_match(range, cx);
    }
    // A superseding request wins; invalid UTF-8 must leave that request intact.
    cx.update(|_, cx| {
        editor.update(cx, |state, cx| {
            state.select_source_range_without_focus(distant.clone(), cx);
            state.select_source_range_without_focus(near.clone(), cx);
            assert!(!state.select_source_range_without_focus(0.."near caf".len() + 1, cx));
            assert_eq!(state.diagnostic_pending_reveal(), Some(near.clone()));
        })
    });
    settle(cx);
    assert_match(near, cx);

    // Oversized selections reveal the beginning, even with an offscreen caret.
    cx.update(|_, cx| {
        editor.update(cx, |state, cx| {
            state.select_source_range_without_focus(0..distant.end, cx);
        })
    });
    settle(cx);
    cx.update(|_, cx| assert!(editor.read(cx).diagnostic_range_visible(0..4)));

    // Search selection preserves redo and creates no undo transaction.
    cx.update(|window, cx| {
        editor.update(cx, |state, cx| {
            state.select_source_range(0..4, window, cx);
            state.replace_selection_isolated("NEAR", window, cx);
        })
    });
    cx.dispatch_action(Undo);
    cx.run_until_parked();
    cx.update(|window, cx| {
        query.update(cx, |query, cx| query.focus(window, cx));
        editor.update(cx, |state, cx| {
            state.select_source_range_without_focus(distant.clone(), cx);
        });
    });
    settle(cx);
    cx.update(|window, cx| editor.update(cx, |state, cx| state.focus(window, cx)));
    cx.dispatch_action(Redo);
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(
            editor.read(cx).value().as_ref(),
            source.replacen("near", "NEAR", 1)
        )
    });
    cx.dispatch_action(Undo);
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(editor.read(cx).value().as_ref(), source));
}

#[gpui::test]
fn source_reveals_wrapped_unicode_without_focus(cx: &mut TestAppContext) {
    reveal(cx, false, true, None);
}
#[gpui::test]
fn live_reveals_wrapped_unicode_without_focus(cx: &mut TestAppContext) {
    reveal(cx, true, true, None);
}
#[gpui::test]
fn source_reveals_horizontal_matches_without_focus(cx: &mut TestAppContext) {
    reveal(cx, false, false, None);
}
#[gpui::test]
fn live_reveals_code_matches_without_focus(cx: &mut TestAppContext) {
    reveal(
        cx,
        true,
        true,
        Some(format!(
            "```rust\n{}target 你好😀\n```",
            "let x = 1; ".repeat(160)
        )),
    );
}
#[gpui::test]
fn live_reveals_table_matches_without_focus(cx: &mut TestAppContext) {
    reveal(cx, true, true, Some("| First | Second | Third | Fourth | Fifth | Last |\n| --- | --- | --- | --- | --- | --- |\n| one | two | three | four | five | target 你好😀 |".into()));
}
