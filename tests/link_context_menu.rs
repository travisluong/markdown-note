//! Production pointer/menu/clipboard paths on GPUI's headless TestPlatform.
#![cfg(feature = "editor-diagnostics")]

use gpui::{
    App, Context, Entity, Focusable, IntoElement, Modifiers, MouseButton, Pixels, Point, Render,
    SharedString, TestAppContext, VisualTestContext, Window, div, point, prelude::*, px,
};
use gpui_component::{
    Root,
    input::{Input, InputEvent, InputState},
    text::{ReadingParseControl, ReadingProbe, TextView},
};
use std::sync::{Arc, Mutex};

fn settle(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.refresh().unwrap();
    cx.run_until_parked();
}

fn right_click(cx: &mut VisualTestContext, position: Point<Pixels>) {
    cx.simulate_mouse_move(position, None, Modifiers::default());
    cx.simulate_mouse_down(position, MouseButton::Right, Modifiers::default());
    cx.simulate_mouse_up(position, MouseButton::Right, Modifiers::default());
    settle(cx);
}

fn copy_first_item(cx: &mut VisualTestContext, expected: &str) {
    cx.simulate_keystrokes("down enter");
    cx.update(|_, cx| {
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some(expected)
        )
    });
}

struct LiveView(Entity<InputState>);
impl Render for LiveView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(360.))
            .h_full()
            .child(Input::new(&self.0).h_full())
    }
}

fn live_fixture<'a>(
    cx: &'a mut TestAppContext,
    source: &str,
) -> (Entity<InputState>, &'a mut VisualTestContext) {
    cx.update(gpui_component::init);
    let mut editor = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let input = cx.new(|cx| {
            let mut state = InputState::new(window, cx)
                .multi_line(true)
                .default_value(source.to_owned());
            state.set_live_preview(true, cx);
            state.select_source_range(0..3, window, cx);
            state
        });
        editor = Some(input.clone());
        let view = cx.new(|_| LiveView(input));
        Root::new(view, window, cx)
    });
    settle(cx);
    (editor.unwrap(), cx)
}

fn live_unchanged(editor: &Entity<InputState>, source: &str, cx: &mut VisualTestContext) {
    cx.update(|_, cx| {
        let editor = editor.read(cx);
        assert_eq!(editor.text().to_string(), source);
        assert_eq!(editor.source_selection(), 0..3);
    });
}

#[gpui::test]
fn live_copy_link_uses_the_clicked_destination_and_preserves_selection(cx: &mut TestAppContext) {
    let source = "before [café](https://example.com/a?q=1#part) and [relative](../notes/caf%C3%A9.md#heading)\n\n[reference][ref]\n\n[ref]: ../reference.md#part\n\n[This long link label wraps across multiple visual lines and keeps its destination](#wrapped)\n\n| Name | Other |\n| --- | --- |\n| [table link](../table.md#cell) | text |\n";
    let (editor, cx) = live_fixture(cx, source);
    let opened = Arc::new(Mutex::new(Vec::new()));
    let calls = opened.clone();
    let _subscription = cx.update(|_, cx| {
        cx.subscribe(&editor, move |_, event, _| {
            if let InputEvent::OpenLink { destination } = event {
                calls.lock().unwrap().push(destination.clone());
            }
        })
    });
    let positions = cx.update(|_, cx| editor.read(cx).diagnostic_link_positions());
    for target in [
        "https://example.com/a?q=1#part",
        "../notes/caf%C3%A9.md#heading",
        "../reference.md#part",
        "#wrapped",
        "../table.md#cell",
    ] {
        assert!(
            positions.iter().any(|(url, _)| url == target),
            "missing painted {target}: {positions:?}"
        );
    }
    let wrapped: Vec<_> = positions
        .iter()
        .filter(|(url, _)| url == "#wrapped")
        .collect();
    assert!(wrapped.len() > 1 && wrapped[0].1.y != wrapped[1].1.y);
    for (destination, position) in positions {
        right_click(cx, position);
        cx.update(|window, cx| {
            let menu = editor.read(cx).diagnostic_mouse_menu(cx).unwrap();
            assert!(
                menu.focus_handle(cx).is_focused(window),
                "Live popup should hold focus; editor focused: {}",
                editor.focus_handle(cx).is_focused(window)
            );
        });
        cx.update(|_, cx| assert!(editor.read(cx).diagnostic_mouse_menu_has_copy_link(cx)));
        live_unchanged(&editor, source, cx);
        copy_first_item(cx, &destination);
        live_unchanged(&editor, source, cx);
        cx.update(|_, cx| assert!(editor.read(cx).diagnostic_mouse_menu(cx).is_none()));
    }
    assert!(opened.lock().unwrap().is_empty());
    cx.simulate_keystrokes(if cfg!(target_os = "macos") {
        "cmd-c"
    } else {
        "ctrl-c"
    });
    cx.update(|_, cx| {
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("bef")
        )
    });
}

#[gpui::test]
fn live_menu_dismissal_nonlinks_source_and_stale_layout(cx: &mut TestAppContext) {
    let source = "before [café](../target.md#part) after\n";
    let (editor, cx) = live_fixture(cx, source);
    let position = cx.update(|_, cx| editor.read(cx).diagnostic_link_positions()[0].1);
    right_click(cx, position);
    cx.update(|window, cx| {
        let menu = editor.read(cx).diagnostic_mouse_menu(cx).unwrap();
        assert!(
            menu.focus_handle(cx).is_focused(window),
            "Live popup should hold focus; editor focused: {}",
            editor.focus_handle(cx).is_focused(window)
        );
    });
    cx.simulate_keystrokes("escape");
    live_unchanged(&editor, source, cx);
    cx.update(|_, cx| assert!(editor.read(cx).diagnostic_mouse_menu(cx).is_none()));
    right_click(cx, position);
    cx.update(|window, cx| {
        let menu = editor.read(cx).diagnostic_mouse_menu(cx).unwrap();
        assert!(
            menu.focus_handle(cx).is_focused(window),
            "Live popup should hold focus; editor focused: {}",
            editor.focus_handle(cx).is_focused(window)
        );
    });
    cx.simulate_mouse_down(
        point(px(500.), px(300.)),
        MouseButton::Right,
        Modifiers::default(),
    );
    settle(cx);
    cx.update(|_, cx| assert!(editor.read(cx).diagnostic_mouse_menu(cx).is_none()));
    live_unchanged(&editor, source, cx);

    right_click(cx, point(px(8.), position.y));
    cx.update(|_, cx| assert!(!editor.read(cx).diagnostic_mouse_menu_has_copy_link(cx)));
    cx.simulate_keystrokes("escape");
    cx.simulate_mouse_move(position, None, Modifiers::default());
    settle(cx);
    cx.update(|window, cx| {
        editor.update(cx, |state, cx| {
            state.diagnostic_stale_link_menu(position, window, cx)
        })
    });
    settle(cx);
    cx.update(|_, cx| assert!(!editor.read(cx).diagnostic_mouse_menu_has_copy_link(cx)));
    cx.simulate_keystrokes("escape");
    cx.update(|_, cx| editor.update(cx, |state, cx| state.set_live_preview(false, cx)));
    settle(cx);
    right_click(cx, position);
    cx.update(|window, cx| {
        let menu = editor.read(cx).diagnostic_mouse_menu(cx).unwrap();
        assert!(
            menu.focus_handle(cx).is_focused(window),
            "Live popup should hold focus; editor focused: {}",
            editor.focus_handle(cx).is_focused(window)
        );
    });
    cx.update(|_, cx| assert!(!editor.read(cx).diagnostic_mouse_menu_has_copy_link(cx)));
}

struct ReadingView {
    source: SharedString,
    control: ReadingParseControl,
    probe: Option<ReadingProbe>,
    opened: Arc<Mutex<Vec<String>>>,
}
impl Render for ReadingView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let opened = self.opened.clone();
        let reading = TextView::markdown("reading", self.source.clone(), window, cx)
            .selectable(true)
            .diagnostic_parse_control(self.control.clone())
            .on_link_open(move |url, _, _, _| opened.lock().unwrap().push(url.to_owned()));
        self.probe = Some(reading.diagnostic_probe());
        div().w(px(360.)).h_full().child(reading)
    }
}
fn reading_fixture<'a>(
    cx: &'a mut TestAppContext,
    source: &str,
) -> (
    Entity<ReadingView>,
    ReadingParseControl,
    &'a mut VisualTestContext,
) {
    cx.update(gpui_component::init);
    let control = ReadingParseControl::default();
    let mut view = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|_| ReadingView {
            source: source.to_owned().into(),
            control: control.clone(),
            probe: None,
            opened: Arc::default(),
        });
        view = Some(entity.clone());
        Root::new(entity, window, cx)
    });
    settle(cx);
    control.complete(Ok(()));
    settle(cx);
    (view.unwrap(), control, cx)
}
fn probe(view: &Entity<ReadingView>, cx: &App) -> ReadingProbe {
    view.read(cx).probe.clone().unwrap()
}

#[gpui::test]
fn reading_copy_link_preserves_selection_and_uses_displayed_parse(cx: &mut TestAppContext) {
    let source = "before [café](https://example.com/a?q=1#part) and [relative](../notes/caf%C3%A9.md#heading)\n\n[reference][ref]\n\n[ref]: ../reference.md#part\n\n[This long link label wraps across multiple visual lines and keeps its destination](#wrapped)\n\n| Name | Other |\n| --- | --- |\n| [table link](../table.md#cell) | text |\n";
    let (view, control, cx) = reading_fixture(cx, source);
    let probe = cx.update(|_, cx| probe(&view, cx));
    let positions = cx.update(|_, cx| probe.link_positions(cx));
    for target in [
        "https://example.com/a?q=1#part",
        "../notes/caf%C3%A9.md#heading",
        "../reference.md#part",
        "#wrapped",
        "../table.md#cell",
    ] {
        assert!(
            positions.iter().any(|(url, _)| url == target),
            "missing {target}"
        );
    }
    let wrapped: Vec<_> = positions
        .iter()
        .filter(|(url, _)| url == "#wrapped")
        .collect();
    assert!(wrapped.len() > 1 && wrapped[0].1.y != wrapped[1].1.y);
    let y = positions[0].1.y;
    cx.simulate_mouse_down(point(px(1.), y), MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_move(point(px(22.), y), MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_up(point(px(22.), y), MouseButton::Left, Modifiers::default());
    settle(cx);
    let selection = cx.update(|_, cx| probe.selection(cx));
    assert_ne!(selection.0, selection.1);
    for (i, (destination, position)) in positions.iter().enumerate() {
        right_click(cx, *position);
        cx.update(|_, cx| {
            assert!(probe.link_menu(cx).is_some());
            assert_eq!(probe.selection(cx), selection);
        });
        if i == 0 {
            cx.simulate_click(*position + point(px(25.), px(12.)), Modifiers::default());
            settle(cx);
            cx.update(|_, cx| {
                assert_eq!(
                    cx.read_from_clipboard().unwrap().text().as_deref(),
                    Some(destination.as_str())
                )
            });
        } else {
            copy_first_item(cx, destination);
        }
        cx.update(|window, cx| {
            assert!(probe.link_menu(cx).is_none());
            assert_eq!(probe.selection(cx), selection);
            assert!(probe.is_focused(window, cx));
        });
    }
    right_click(cx, positions[0].1);
    cx.simulate_keystrokes("escape");
    cx.update(|window, cx| {
        assert!(probe.link_menu(cx).is_none());
        assert_eq!(probe.selection(cx), selection);
        assert!(probe.is_focused(window, cx));
    });
    right_click(cx, positions[0].1);
    cx.simulate_mouse_down(
        point(px(500.), px(300.)),
        MouseButton::Right,
        Modifiers::default(),
    );
    settle(cx);
    cx.update(|_, cx| {
        assert!(probe.link_menu(cx).is_none());
        assert_eq!(probe.selection(cx), selection);
    });
    right_click(cx, point(px(4.), y));
    cx.update(|_, cx| assert!(probe.link_menu(cx).is_none()));

    // The old tree stays visible while the replacement parse waits.
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.source = "[replacement](https://new.example)".into();
            cx.notify();
        })
    });
    settle(cx);
    right_click(cx, positions[0].1);
    copy_first_item(cx, &positions[0].0);
    cx.update(|_, cx| {
        assert_eq!(
            probe
                .snapshot(cx)
                .unwrap()
                .displayed_source
                .unwrap()
                .as_ref(),
            source
        );
        assert!(view.read(cx).opened.lock().unwrap().is_empty());
    });
    // An open menu keeps its destination after a replacement tree installs.
    right_click(cx, positions[0].1);
    control.complete(Ok(()));
    settle(cx);
    copy_first_item(cx, &positions[0].0);
}

#[gpui::test]
fn reading_linked_image_copies_enclosing_link(cx: &mut TestAppContext) {
    let (view, _, cx) = reading_fixture(cx, "[![alt](missing.png)](../target.md#image)");
    let probe = cx.update(|_, cx| probe(&view, cx));
    right_click(cx, point(px(10.), px(10.)));
    cx.update(|_, cx| assert!(probe.link_menu(cx).is_some()));
    copy_first_item(cx, "../target.md#image");
    cx.update(|_, cx| assert!(view.read(cx).opened.lock().unwrap().is_empty()));
}
