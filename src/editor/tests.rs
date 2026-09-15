//! Real adapter contracts on GPUI TestPlatform; no native desktop interaction.
use super::{Editor, Event};
use crate::ViewMode;
use gpui::{
    Context, Entity, IntoElement, Render, Subscription, TestAppContext, VisualTestContext, Window,
    div, prelude::*,
};
use gpui_component::{
    Root,
    input::{InputEvent, Redo, Undo},
};

struct Host {
    editor: Editor,
    changes: usize,
    disabled: bool,
    link: Option<String>,
    _subscription: Subscription,
}
impl Host {
    fn new(text: &str, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let editor = Editor::new(text.to_owned(), window, cx);
        let subscription = editor.subscribe(window, cx, |this, event, _, _| match event {
            Event::Changed => this.changes += 1,
            Event::OpenLink(destination) => this.link = Some(destination.to_owned()),
        });
        Self {
            editor,
            changes: 0,
            disabled: false,
            link: None,
            _subscription: subscription,
        }
    }
}
impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .child(self.editor.widget().h_full().disabled(self.disabled))
    }
}
fn fixture<'a>(
    cx: &'a mut TestAppContext,
    text: &str,
) -> (Entity<Host>, &'a mut VisualTestContext) {
    cx.update(gpui_component::init);
    let mut host = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| Host::new(text, window, cx));
        host = Some(entity.clone());
        Root::new(entity, window, cx)
    });
    cx.refresh().unwrap();
    cx.run_until_parked();
    (host.unwrap(), cx)
}
fn transactions(cx: &mut TestAppContext, mode: ViewMode) {
    let (host, cx) = fixture(cx, "café");
    let editor = cx.update(|_, cx| host.read(cx).editor.clone());
    let saved = cx.update(|window, cx| {
        editor.set_mode(mode, cx);
        let saved = editor.snapshot(cx);
        assert!(!editor.replace_range_if_current(4..5, &saved, "invalid", window, cx));
        assert_eq!(editor.text(cx).as_ref(), "café");
        assert!(editor.replace_range_if_current(0..5, &saved, "茶", window, cx));
        assert!(!editor.replace_range_if_current(0..3, &saved, "stale", window, cx));
        saved
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(host.read(cx).changes, 1);
        assert_eq!(saved.to_string(), "café");
        assert_eq!(editor.text(cx).as_ref(), "茶");
    });
    let id = editor.id();
    cx.update(|_, cx| {
        editor.set_mode(
            if mode == ViewMode::Source {
                ViewMode::LivePreview
            } else {
                ViewMode::Source
            },
            cx,
        )
    });
    assert_eq!(editor.id(), id);
    cx.dispatch_action(Undo);
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(editor.text(cx).as_ref(), "café"));
    cx.dispatch_action(Redo);
    cx.run_until_parked();
    cx.update(|window, cx| {
        assert_eq!(editor.text(cx).as_ref(), "茶");
        let snapshot = editor.snapshot(cx);
        assert!(!editor.insert_at_snapshot(1, &snapshot, "split", window, cx));
        assert!(editor.insert_at_snapshot(3, &snapshot, " image", window, cx));
        assert!(!editor.insert_at_snapshot(3, &snapshot, "stale", window, cx));
    });
    cx.run_until_parked();
    cx.dispatch_action(Undo);
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(editor.text(cx).as_ref(), "茶"));
}
#[gpui::test]
fn source_transactions_preserve_snapshots_events_and_undo(cx: &mut TestAppContext) {
    transactions(cx, ViewMode::Source);
}

#[gpui::test]
fn line_moves_are_isolated_and_preserve_selection_in_both_modes(cx: &mut TestAppContext) {
    let (host, cx) = fixture(cx, "a\ncafé\n茶");
    let editor = cx.update(|_, cx| host.read(cx).editor.clone());
    for mode in [ViewMode::Source, ViewMode::LivePreview] {
        cx.update(|window, cx| {
            editor.set_mode(mode, cx);
            editor.focus(window, cx);
            assert!(editor.select(3..7, window, cx));
            editor.move_lines(true, window, cx);
            assert_eq!(editor.text(cx).as_ref(), "a\n茶\ncafé");
            assert_eq!(editor.selection(cx), 7..11);
        });
        cx.run_until_parked();
        cx.dispatch_action(Undo);
        cx.run_until_parked();
        cx.update(|_, cx| assert_eq!(editor.text(cx).as_ref(), "a\ncafé\n茶"));
        cx.dispatch_action(Redo);
        cx.run_until_parked();
        cx.update(|_, cx| assert_eq!(editor.text(cx).as_ref(), "a\n茶\ncafé"));
        cx.dispatch_action(Undo);
        cx.run_until_parked();
    }
}
#[gpui::test]
fn ordered_line_moves_renumber_in_one_undo_step(cx: &mut TestAppContext) {
    let original = "9. café\n10. 茶\n11. last";
    let expected = "9. 茶\n10. café\n11. last";
    let (host, cx) = fixture(cx, original);
    let editor = cx.update(|_, cx| host.read(cx).editor.clone());
    for mode in [ViewMode::Source, ViewMode::LivePreview] {
        cx.update(|window, cx| {
            editor.set_mode(mode, cx);
            editor.focus(window, cx);
            assert!(editor.select(3..8, window, cx));
            editor.move_lines(true, window, cx);
            assert_eq!(editor.text(cx).as_ref(), expected);
            assert_eq!(&editor.text(cx)[editor.selection(cx)], "café");
        });
        cx.run_until_parked();
        cx.dispatch_action(Undo);
        cx.run_until_parked();
        cx.update(|_, cx| assert_eq!(editor.text(cx).as_ref(), original));
        cx.dispatch_action(Redo);
        cx.run_until_parked();
        cx.update(|_, cx| assert_eq!(editor.text(cx).as_ref(), expected));
        cx.dispatch_action(Undo);
        cx.run_until_parked();
    }
}

#[gpui::test]
fn live_transactions_preserve_snapshots_events_and_undo(cx: &mut TestAppContext) {
    transactions(cx, ViewMode::LivePreview);
}

#[gpui::test]
fn rendered_task_guards_and_link_events_use_original_source(cx: &mut TestAppContext) {
    let (host, cx) = fixture(cx, "- [ ] café");
    let editor = cx.update(|_, cx| host.read(cx).editor.clone());
    cx.update(|window, cx| {
        assert!(editor.toggle_task(3, "- [ ] café", window, cx));
        assert!(!editor.toggle_task(3, "- [ ] café", window, cx));
        editor.diagnostic_input().update(cx, |_, cx| {
            cx.emit(InputEvent::OpenLink {
                destination: "note.md".into(),
            })
        });
    });
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert_eq!(editor.text(cx).as_ref(), "- [x] café");
        assert_eq!(host.read(cx).changes, 1);
        assert_eq!(host.read(cx).link.as_deref(), Some("note.md"));
    });
}

#[gpui::test]
fn disabled_range_transaction_preserves_text_selection_and_history(cx: &mut TestAppContext) {
    let (host, cx) = fixture(cx, "café");
    let editor = cx.update(|_, cx| host.read(cx).editor.clone());
    let snapshot = cx.update(|window, cx| {
        editor.select(0..3, window, cx);
        editor.replace_selection("tea", window, cx);
        editor.select(1..3, window, cx);
        host.update(cx, |host, cx| {
            host.disabled = true;
            cx.notify();
        });
        editor.snapshot(cx)
    });
    cx.refresh().unwrap();
    cx.run_until_parked();
    cx.update(|window, cx| {
        assert!(editor.diagnostic_input().read(cx).is_disabled());
        assert!(!editor.replace_range_if_current(0..3, &snapshot, "blocked", window, cx));
        assert_eq!(editor.snapshot(cx), snapshot);
        assert_eq!(editor.selection(cx), 1..3);
        assert_eq!(host.read(cx).changes, 1);
        host.update(cx, |host, cx| {
            host.disabled = false;
            cx.notify();
        });
    });
    cx.refresh().unwrap();
    cx.run_until_parked();
    cx.update(|window, cx| editor.focus(window, cx));
    cx.dispatch_action(Undo);
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(editor.text(cx).as_ref(), "café"));
}

#[gpui::test]
fn markdown_formats_are_single_edits_with_shared_history_and_change_events(
    cx: &mut TestAppContext,
) {
    use crate::markdown_edit::Format;
    let (host, cx) = fixture(cx, "café\n茶");
    let editor = cx.update(|_, cx| host.read(cx).editor.clone());
    for mode in [ViewMode::Source, ViewMode::LivePreview] {
        for format in [
            Format::Bold,
            Format::Italic,
            Format::Strikethrough,
            Format::InlineCode,
            Format::Heading(2),
            Format::Heading(0),
            Format::Blockquote,
            Format::OrderedList,
            Format::UnorderedList,
            Format::TaskList,
            Format::CodeBlock,
        ] {
            let before = cx.update(|window, cx| {
                editor.set_mode(mode, cx);
                editor.select(0..9, window, cx);
                let before = host.read(cx).changes;
                assert!(editor.format_markdown(format, window, cx));
                assert!(editor.is_focused(window, cx));
                before
            });
            cx.run_until_parked();
            let formatted = cx.update(|_, cx| {
                let formatted = editor.text(cx).to_string();
                if formatted != "café\n茶" {
                    assert_eq!(host.read(cx).changes, before + 1);
                }
                formatted
            });
            if formatted == "café\n茶" {
                continue;
            }
            cx.dispatch_action(Undo);
            cx.run_until_parked();
            cx.update(|_, cx| assert_eq!(editor.text(cx).as_ref(), "café\n茶"));
            cx.dispatch_action(Redo);
            cx.run_until_parked();
            cx.update(|_, cx| assert_eq!(editor.text(cx).as_ref(), formatted));
            cx.dispatch_action(Undo);
            cx.run_until_parked();
        }
    }
    cx.update(|window, cx| {
        editor.select(0..5, window, cx);
        editor.format_markdown(Format::Bold, window, cx);
        editor.format_markdown(Format::Italic, window, cx);
        assert_eq!(editor.text(cx).as_ref(), "***café***\n茶");
    });
    cx.run_until_parked();
    cx.dispatch_action(Undo);
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(editor.text(cx).as_ref(), "**café**\n茶"));
    cx.dispatch_action(Undo);
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(editor.text(cx).as_ref(), "café\n茶"));
}

#[gpui::test]
fn markdown_format_rejects_disabled_input(cx: &mut TestAppContext) {
    let (host, cx) = fixture(cx, "café");
    cx.update(|_, cx| {
        host.update(cx, |host, cx| {
            host.disabled = true;
            cx.notify();
        })
    });
    cx.refresh().unwrap();
    cx.update(|window, cx| {
        let editor = host.read(cx).editor.clone();
        assert!(!editor.format_markdown(crate::markdown_edit::Format::Bold, window, cx));
        assert_eq!(editor.text(cx).as_ref(), "café");
    });
}
