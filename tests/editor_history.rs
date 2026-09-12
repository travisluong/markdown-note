//! Real editor history on GPUI TestPlatform; no native windows or OS input.
use gpui::{
    Context, Entity, EntityInputHandler, IntoElement, Render, TestAppContext, Window, div,
    prelude::*,
};
use gpui_component::{
    Root,
    history::{History, HistoryItem},
    input::{Input, InputState, Redo, Undo},
};

struct EditorView(Entity<InputState>);

impl Render for EditorView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(Input::new(&self.0).h_full())
    }
}

#[derive(Clone, Copy)]
enum EditKind {
    Isolated,
    Typing,
    Ime,
}

fn divergent_edit(cx: &mut TestAppContext, live: bool, kind: EditKind) {
    cx.update(gpui_component::init);
    let mut editor = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let input = cx.new(|cx| {
            let mut state = InputState::new(window, cx)
                .multi_line(true)
                .default_value("a");
            state.set_live_preview(live, cx);
            state.focus(window, cx);
            state
        });
        editor = Some(input.clone());
        let view = cx.new(|_| EditorView(input));
        Root::new(view, window, cx)
    });
    let editor = editor.unwrap();
    cx.update(|window, cx| {
        editor.update(cx, |state, cx| {
            assert!(state.select_source_range(0..1, window, cx));
            state.replace_selection_isolated("b", window, cx);
            assert_eq!(state.value().as_ref(), "b");
        });
    });
    cx.run_until_parked();
    cx.dispatch_action(Undo);
    cx.update(|_, cx| assert_eq!(editor.read(cx).value().as_ref(), "a"));

    // A selection change alone must not invalidate the original redo branch.
    cx.update(|window, cx| {
        editor.update(cx, |state, cx| {
            assert!(state.select_source_range(0..1, window, cx));
        });
    });
    cx.dispatch_action(Redo);
    cx.update(|_, cx| assert_eq!(editor.read(cx).value().as_ref(), "b"));
    cx.dispatch_action(Undo);
    cx.update(|_, cx| assert_eq!(editor.read(cx).value().as_ref(), "a"));

    let expected = match kind {
        EditKind::Isolated => "c",
        EditKind::Typing => "café",
        EditKind::Ime => "你好😀",
    };
    cx.update(|window, cx| {
        editor.update(cx, |state, cx| {
            assert!(state.select_source_range(0..1, window, cx));
            match kind {
                EditKind::Isolated => state.replace_selection_isolated("c", window, cx),
                EditKind::Typing => {
                    state.replace_text_in_range(None, "c", window, cx);
                    state.replace_text_in_range(None, "afé", window, cx);
                }
                EditKind::Ime => {
                    state.replace_and_mark_text_in_range(None, "n", Some(1..1), window, cx);
                    state.replace_and_mark_text_in_range(None, "你好", Some(2..2), window, cx);
                    state.replace_text_in_range(None, "你好😀", window, cx);
                }
            }
            assert_eq!(state.value().as_ref(), expected);
        });
    });
    cx.run_until_parked();
    cx.dispatch_action(Redo);
    cx.update(|_, cx| {
        assert_eq!(
            editor.read(cx).value().as_ref(),
            expected,
            "redo must not overwrite a divergent edit; live_preview={live}"
        );
    });

    // New typing and composition remain grouped, and redo can replay the new branch.
    cx.dispatch_action(Undo);
    cx.update(|_, cx| assert_eq!(editor.read(cx).value().as_ref(), "a"));
    cx.dispatch_action(Redo);
    cx.update(|_, cx| assert_eq!(editor.read(cx).value().as_ref(), expected));
}

#[gpui::test]
fn source_isolated_edit_discards_redo(cx: &mut TestAppContext) {
    divergent_edit(cx, false, EditKind::Isolated);
}

#[gpui::test]
fn preview_isolated_edit_discards_redo(cx: &mut TestAppContext) {
    divergent_edit(cx, true, EditKind::Isolated);
}

#[gpui::test]
fn source_typing_discards_redo_and_keeps_grouping(cx: &mut TestAppContext) {
    divergent_edit(cx, false, EditKind::Typing);
}

#[gpui::test]
fn preview_typing_discards_redo_and_keeps_grouping(cx: &mut TestAppContext) {
    divergent_edit(cx, true, EditKind::Typing);
}

#[gpui::test]
fn source_ime_discards_redo_and_keeps_grouping(cx: &mut TestAppContext) {
    divergent_edit(cx, false, EditKind::Ime);
}

#[gpui::test]
fn preview_ime_discards_redo_and_keeps_grouping(cx: &mut TestAppContext) {
    divergent_edit(cx, true, EditKind::Ime);
}

fn same_text_after_undo(cx: &mut TestAppContext, live: bool, isolated: bool) {
    cx.update(gpui_component::init);
    let mut editor = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let input = cx.new(|cx| {
            let mut state = InputState::new(window, cx)
                .multi_line(true)
                .default_value("😀");
            state.set_live_preview(live, cx);
            state.focus(window, cx);
            state
        });
        editor = Some(input.clone());
        let view = cx.new(|_| EditorView(input));
        Root::new(view, window, cx)
    });
    let editor = editor.unwrap();
    cx.update(|window, cx| {
        editor.update(cx, |state, cx| {
            assert!(state.select_source_range(0..4, window, cx));
            state.replace_selection_isolated("你好", window, cx);
            assert_eq!(state.value().as_ref(), "你好");
        });
    });
    cx.run_until_parked();
    cx.dispatch_action(Undo);
    cx.update(|_, cx| assert_eq!(editor.read(cx).value().as_ref(), "😀"));

    cx.update(|window, cx| {
        editor.update(cx, |state, cx| {
            assert!(state.select_source_range(0..4, window, cx));
            if isolated {
                state.replace_selection_isolated("😀", window, cx);
            } else {
                state.replace_text_in_range(None, "😀", window, cx);
            }
            assert_eq!(state.value().as_ref(), "😀");
        });
    });
    cx.run_until_parked();
    cx.dispatch_action(Redo);
    cx.update(|_, cx| {
        assert_eq!(
            editor.read(cx).value().as_ref(),
            "你好",
            "same-text replacement must preserve redo; live_preview={live}, isolated={isolated}"
        );
    });
    cx.dispatch_action(Undo);
    cx.update(|_, cx| assert_eq!(editor.read(cx).value().as_ref(), "😀"));
    cx.dispatch_action(Redo);
    cx.update(|_, cx| assert_eq!(editor.read(cx).value().as_ref(), "你好"));
}

#[gpui::test]
fn source_same_text_isolated_replacement_preserves_redo(cx: &mut TestAppContext) {
    same_text_after_undo(cx, false, true);
}

#[gpui::test]
fn preview_same_text_isolated_replacement_preserves_redo(cx: &mut TestAppContext) {
    same_text_after_undo(cx, true, true);
}

#[gpui::test]
fn source_same_text_ordinary_replacement_preserves_redo(cx: &mut TestAppContext) {
    same_text_after_undo(cx, false, false);
}

#[gpui::test]
fn preview_same_text_ordinary_replacement_preserves_redo(cx: &mut TestAppContext) {
    same_text_after_undo(cx, true, false);
}

#[derive(Clone, PartialEq)]
struct Visit {
    page: usize,
    version: usize,
}

impl HistoryItem for Visit {
    fn version(&self) -> usize {
        self.version
    }

    fn set_version(&mut self, version: usize) {
        self.version = version;
    }
}

#[test]
fn generic_history_keeps_its_existing_redo_policy() {
    let mut history = History::new();
    for page in 0..2 {
        history.push(Visit { page, version: 0 });
    }
    assert_eq!(history.undo().unwrap()[0].page, 1);
    history.push(Visit {
        page: 2,
        version: 0,
    });
    assert_eq!(history.redo().unwrap()[0].page, 1);
}
