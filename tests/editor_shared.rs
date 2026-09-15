//! Shared document editing on TestPlatform, with independent rendered views.
use gpui::{
    Context, EntityInputHandler, IntoElement, Render, TestAppContext, Window, div, prelude::*,
};
use gpui_component::{
    Root,
    input::{Redo, Undo},
};
use mithril::{ViewMode, editor::Editor};

struct View(Editor);
impl Render for View {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.0.widget().h_full())
    }
}

#[gpui::test]
fn shared_ime_and_typing_have_one_history_with_view_boundaries(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let mut editors = None;
    let mut view = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let first = Editor::new("start", window, cx);
        let second = Editor::from_shared_buffer(first.shared_buffer(cx), window, cx);
        let content = cx.new(|_| View(first.clone()));
        view = Some(content.clone());
        editors = Some((first, second));
        Root::new(content, window, cx)
    });
    let (first, second) = editors.unwrap();
    let view = view.unwrap();
    cx.update(|window, cx| {
        first.select(0..5, window, cx);
        first.diagnostic_input().update(cx, |input, cx| {
            input.replace_and_mark_text_in_range(None, "n", Some(1..1), window, cx);
            input.replace_and_mark_text_in_range(None, "你好", Some(2..2), window, cx);
            input.replace_text_in_range(None, "你好😀", window, cx);
        });
        assert_eq!(second.text(cx).as_ref(), "你好😀");
        first.break_undo_group(cx);
        second.break_undo_group(cx);
        view.update(cx, |view, cx| {
            view.0 = second.clone();
            cx.notify();
        });
        second.select(10..10, window, cx);
        second.diagnostic_input().update(cx, |input, cx| {
            input.replace_text_in_range(None, "a", window, cx);
            input.replace_text_in_range(None, "b", window, cx);
        });
    });
    cx.run_until_parked();
    cx.dispatch_action(Undo);
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(first.text(cx).as_ref(), "你好😀");
        assert_eq!(second.text(cx).as_ref(), "你好😀");
    });
    cx.dispatch_action(Undo);
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(first.text(cx).as_ref(), "start"));
    cx.dispatch_action(Redo);
    cx.run_until_parked();
    cx.update(|window, cx| {
        first.select(0..0, window, cx);
        view.update(cx, |view, cx| {
            view.0 = first.clone();
            cx.notify();
        });
        first.replace_selection("new ", window, cx);
    });
    cx.dispatch_action(Redo);
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(second.text(cx).as_ref(), "new 你好😀"));
}

#[cfg(feature = "editor-diagnostics")]
fn scroll_case(cx: &mut TestAppContext, mode: ViewMode) {
    cx.update(gpui_component::init);
    let text = (0..150)
        .map(|i| format!("Paragraph {i:03} with text\n\n"))
        .collect::<String>();
    let mut editors = None;
    let mut view = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let first = Editor::new(text, window, cx);
        first.set_mode(mode, cx);
        let second = Editor::from_shared_buffer(first.shared_buffer(cx), window, cx);
        let content = cx.new(|_| View(first.clone()));
        view = Some(content.clone());
        editors = Some((first, second));
        Root::new(content, window, cx)
    });
    let (first, second) = editors.unwrap();
    let view = view.unwrap();
    cx.run_until_parked();
    cx.refresh().unwrap();
    cx.update(|_, cx| {
        first
            .diagnostic_input()
            .update(cx, |input, cx| input.diagnostic_scroll(-1500., cx))
    });
    cx.refresh().unwrap();
    let before = cx.update(|_, cx| first.diagnostic_input().read(cx).diagnostic_input_paint());
    assert!(before.2.y < gpui::px(-1000.));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.0 = second.clone();
            cx.notify();
        });
        second.select(0..0, window, cx);
        second.replace_selection("Inserted above\n\n", window, cx);
        second.replace_selection("And another\n\n", window, cx);
    });
    cx.run_until_parked();
    cx.refresh().unwrap();
    cx.update(|_, cx| {
        let second_scroll = second
            .diagnostic_input()
            .read(cx)
            .diagnostic_input_paint()
            .2;
        assert!(second_scroll.y > before.2.y);
        view.update(cx, |view, cx| {
            view.0 = first.clone();
            cx.notify();
        });
    });
    cx.run_until_parked();
    cx.refresh().unwrap();
    cx.update(|_, cx| {
        let after = first.diagnostic_input().read(cx).diagnostic_input_paint();
        assert!(after.2.y < before.2.y);
        assert_eq!(
            after.1.chars().take(50).collect::<String>(),
            before.1.chars().take(50).collect::<String>()
        );
    });
}

#[cfg(feature = "editor-diagnostics")]
#[gpui::test]
fn shared_source_view_preserves_scroll_through_peer_edits(cx: &mut TestAppContext) {
    scroll_case(cx, ViewMode::Source);
}
#[cfg(feature = "editor-diagnostics")]
#[gpui::test]
fn shared_preview_view_preserves_scroll_through_peer_edits(cx: &mut TestAppContext) {
    scroll_case(cx, ViewMode::LivePreview);
}
