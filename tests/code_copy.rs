//! Code copy uses production controls on headless GPUI TestPlatform only.
use gpui::{
    Context, Entity, IntoElement, Modifiers, Render, TestAppContext, Window, div, prelude::*, px,
};
use gpui_component::{
    Root,
    input::{Input, InputState},
    text::TextView,
};

struct LiveView(Entity<InputState>);
impl Render for LiveView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(260.))
            .h_full()
            .child(Input::new(&self.0).h_full())
    }
}

#[gpui::test]
fn live_copy_code_preserves_source_and_selection(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let mut editor = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let input = cx.new(|cx| InputState::new(window, cx).multi_line(true));
        editor = Some(input.clone());
        let view = cx.new(|_| LiveView(input));
        Root::new(view, window, cx)
    });
    let editor = editor.unwrap();
    for (source, expected) in [
        (
            "before\n```rust\nlet café = 1;\n```\nafter",
            "let café = 1;",
        ),
        (
            "before\n> ~~~text\n> 你好\n> **literal**\n> ~~~",
            "你好\n**literal**",
        ),
        (
            "before\n- item\n\n  ```\n  one\n    two\n  ```",
            "one\n  two",
        ),
        ("before\r\n```\r\nhello\r\n```", "hello"),
        ("before\n```\n```", ""),
        ("before\n```\nunclosed", "unclosed"),
    ] {
        cx.update(|window, cx| {
            editor.update(cx, |state, cx| {
                state.set_value(source, window, cx);
                state.set_live_preview(true, cx);
                state.select_source_range(0..3, window, cx);
                state.diagnostic_settle_preview();
                cx.notify();
            })
        });
        cx.run_until_parked();
        cx.refresh().unwrap();
        let button = cx.debug_bounds("code-copy").expect(source);
        assert_eq!(button.size.width, px(24.));
        cx.simulate_click(button.center(), Modifiers::default());
        cx.update(|_, cx| {
            assert_eq!(
                cx.read_from_clipboard().unwrap().text().as_deref(),
                Some(expected),
                "{source}"
            );
            assert_eq!(editor.read(cx).value().as_ref(), source);
            assert_eq!(editor.read(cx).source_selection(), 0..3);
        });
    }
    cx.update(|_, cx| editor.update(cx, |state, cx| state.set_live_preview(false, cx)));
    cx.run_until_parked();
    cx.refresh().unwrap();
    // GPUI 0.2.2 retains debug bounds across frames; exercise the old location
    // to prove Source mode has no active copy control.
    let old_button = cx.debug_bounds("code-copy").unwrap();
    cx.update(|_, cx| cx.write_to_clipboard(gpui::ClipboardItem::new_string("sentinel".into())));
    cx.simulate_click(old_button.center(), Modifiers::default());
    cx.update(|_, cx| {
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("sentinel")
        )
    });
}

struct ReadingView;
impl Render for ReadingView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().w(px(260.)).h_full().child(TextView::markdown(
            "reading",
            "> ```rust\n> let café = 1;\n> ```",
            window,
            cx,
        ))
    }
}

#[gpui::test]
fn reading_copy_code_excludes_container_and_fences(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|_| ReadingView);
        Root::new(view, window, cx)
    });
    cx.run_until_parked();
    cx.refresh().unwrap();
    cx.run_until_parked();
    cx.refresh().unwrap();
    let button = cx.debug_bounds("code-copy").unwrap();
    cx.simulate_click(button.center(), Modifiers::default());
    cx.update(|_, cx| {
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("let café = 1;")
        )
    });
}
