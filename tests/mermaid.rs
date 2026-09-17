//! Mermaid runs exclusively on the in-memory GPUI TestPlatform.
#![cfg(feature = "editor-diagnostics")]
use gpui::{Context, Entity, IntoElement, Render, TestAppContext, Window, div, prelude::*, px};
use gpui_component::{
    Root, Theme, ThemeMode,
    input::{Input, InputState, Redo, Undo},
    text::{SourceHighlights, TextView},
};

#[test]
fn offline_renderer_families_limits_and_queue() {
    for (kind, dark, rendered) in gpui_component::diagnostic_mermaid_checks() {
        let dimensions = rendered.size(0);
        let mut rgba = rendered.as_bytes(0).unwrap().to_vec();
        for p in rgba.as_chunks_mut::<4>().0 {
            p.swap(0, 2);
        }
        let image =
            image::RgbaImage::from_raw(dimensions.width.into(), dimensions.height.into(), rgba)
                .unwrap();
        image
            .save(format!(
                "target/mermaid-{kind}-{}.png",
                if dark { "dark" } else { "light" }
            ))
            .unwrap();
        // The native code-block surface must show through in both themes.
        for (x, y) in [
            (0, 0),
            (image.width() - 1, 0),
            (0, image.height() - 1),
            (image.width() - 1, image.height() - 1),
        ] {
            assert_eq!(
                image.get_pixel(x, y).0[3],
                0,
                "{kind} dark={dark}: opaque canvas at ({x}, {y})"
            );
        }
        let color = if dark { 32 } else { 245 };
        let mut composite = image::RgbaImage::from_pixel(
            image.width(),
            image.height(),
            image::Rgba([color, color, color, 255]),
        );
        image::imageops::overlay(&mut composite, &image, 0, 0);
        composite
            .save(format!(
                "target/mermaid-{kind}-{}-surface.png",
                if dark { "dark" } else { "light" }
            ))
            .unwrap();
    }
}
struct Live(Entity<InputState>, f32);
impl Render for Live {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(self.1))
            .h_full()
            .child(Input::new(&self.0).h_full())
    }
}
#[gpui::test]
fn live_mermaid_reveal_copy_history_and_modes(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let source = "before\n```MeRmAiD\nflowchart TD\nA[Hello] --> B[World]\n```\nafter\n";
    let mut editor = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let input = cx.new(|cx| {
            let mut state = InputState::new(window, cx)
                .multi_line(true)
                .default_value(source);
            state.set_live_preview(true, cx);
            state.focus(window, cx);
            state.select_source_range(0..0, window, cx);
            state.diagnostic_settle_preview();
            state
        });
        editor = Some(input.clone());
        let view = cx.new(|_| Live(input, 360.));
        Root::new(view, window, cx)
    });
    let editor = editor.unwrap();
    for _ in 0..4 {
        cx.refresh().unwrap();
        cx.run_until_parked();
    }
    cx.update(|_, cx| {
        let state = editor.read(cx);
        assert!(
            state.diagnostic_mermaid_row(1).0.is_some_and(|n| n > 0),
            "{:?}",
            state.diagnostic_mermaid_row(1)
        );
        assert_eq!(state.diagnostic_mermaid_row(2).0, Some(0));
        assert_eq!(state.diagnostic_code_row(3).3, 0.);
        assert_eq!(state.value().as_ref(), source);
    });
    let button = cx.debug_bounds("code-copy").unwrap();
    cx.simulate_click(button.center(), Default::default());
    cx.update(|_, cx| {
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("flowchart TD\nA[Hello] --> B[World]")
        );
        assert_eq!(editor.read(cx).source_selection(), 0..0);
    });
    cx.update(|window, cx| {
        let p = editor.read(cx).diagnostic_code_point(1, 0) + gpui::point(px(20.), px(25.));
        editor.update(cx, |state, cx| state.diagnostic_code_click(p, window, cx));
    });
    cx.refresh().unwrap();
    cx.update(|_, cx| {
        let state = editor.read(cx);
        assert_eq!(
            state.source_selection().start,
            source.find("flowchart").unwrap()
        );
        assert_eq!(state.diagnostic_mermaid_row(1).0, None);
        assert!(state.diagnostic_code_row(2).3 > 0.);
    });
    cx.update(|window, cx| {
        editor.update(cx, |s, cx| {
            let at = source.find("Hello").unwrap();
            s.select_source_range(at..at + 5, window, cx);
            s.replace_selection_isolated("café", window, cx);
            s.select_source_range(0..0, window, cx);
            s.diagnostic_settle_preview();
        })
    });
    for _ in 0..3 {
        cx.refresh().unwrap();
        cx.run_until_parked();
    }
    cx.dispatch_action(Undo);
    cx.update(|_, cx| assert_eq!(editor.read(cx).value().as_ref(), source));
    cx.dispatch_action(Redo);
    cx.update(|_, cx| assert!(editor.read(cx).value().contains("café")));
    for mode in [ThemeMode::Dark, ThemeMode::Light] {
        cx.update(|window, cx| {
            Theme::change(mode, Some(window), cx);
            editor.update(cx, |s, cx| {
                s.select_source_range(0..0, window, cx);
                s.diagnostic_settle_preview();
            });
        });
        for _ in 0..3 {
            cx.refresh().unwrap();
            cx.run_until_parked();
        }
        cx.update(|_, cx| assert!(editor.read(cx).diagnostic_mermaid_row(1).0.is_some()));
    }
    cx.update(|_, cx| editor.update(cx, |s, cx| s.set_live_preview(false, cx)));
    cx.refresh().unwrap();
    cx.update(|_, cx| {
        assert_eq!(editor.read(cx).diagnostic_mermaid_row(1), (None, None));
        assert!(editor.read(cx).diagnostic_code_row(2).3 > 0.);
    });
}

struct Reading {
    source: String,
    search: Option<SourceHighlights>,
    probe: Option<gpui_component::text::ReadingProbe>,
}
impl Render for Reading {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let view = TextView::markdown("reading", self.source.clone(), window, cx)
            .source_highlights(self.search.clone());
        self.probe = Some(view.diagnostic_probe());
        div().w(px(360.)).h_full().child(view)
    }
}
#[gpui::test]
fn reading_mermaid_renders_copies_and_reveals_find(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let source = "> ~~~mermaid\n> flowchart TD\n> A --> B\n> ~~~\n";
    let mut view = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let v = cx.new(|_| Reading {
            source: source.into(),
            search: None,
            probe: None,
        });
        view = Some(v.clone());
        Root::new(v, window, cx)
    });
    let view = view.unwrap();
    for _ in 0..5 {
        cx.refresh().unwrap();
        cx.run_until_parked();
    }
    cx.update(|_, cx| {
        assert_eq!(
            view.read(cx).probe.as_ref().unwrap().mermaid_states(cx),
            vec![true]
        )
    });
    let button = cx.debug_bounds("code-copy").unwrap();
    cx.simulate_click(button.center(), Default::default());
    cx.update(|_, cx| {
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("flowchart TD\nA --> B")
        )
    });
    cx.update(|_, cx| {
        view.update(cx, |v, cx| {
            let start = source.find("flowchart").unwrap();
            let range = start..start + 9;
            v.search = Some(SourceHighlights::new(
                source.into(),
                vec![range.clone()],
                Some(range),
            ));
            cx.notify();
        })
    });
    cx.refresh().unwrap();
    cx.update(|_, cx| {
        assert_eq!(
            view.read(cx).probe.as_ref().unwrap().mermaid_states(cx),
            vec![false]
        )
    });
    cx.update(|_, cx| {
        view.update(cx, |v, cx| {
            v.search = None;
            cx.notify();
        })
    });
    for _ in 0..3 {
        cx.refresh().unwrap();
        cx.run_until_parked();
    }
    cx.update(|_, cx| {
        assert_eq!(
            view.read(cx).probe.as_ref().unwrap().mermaid_states(cx),
            vec![true]
        )
    });
}

#[gpui::test]
fn nested_fences_errors_selection_resize_and_stale_jobs(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let mut editor = None;
    let mut view = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let input = cx.new(|cx| InputState::new(window, cx).multi_line(true));
        editor = Some(input.clone());
        let v = cx.new(|_| Live(input, 360.));
        view = Some(v.clone());
        Root::new(v, window, cx)
    });
    let editor = editor.unwrap();
    let view = view.unwrap();
    for (source, row, expected) in [
        (
            "before\r\n> ~~~MeRmAiD\r\n> flowchart TD\r\n> A[你好] --> B[café]\r\n> ~~~\r\nafter",
            1,
            1,
        ),
        (
            "before\n- list\n\n  ```mermaid\n  flowchart LR\n  A --> B\n  ```\nafter",
            3,
            1,
        ),
        ("before\n```mermaid\nflowchart LR\nA --> B", 1, 1),
        ("before\n~~~mermaid\n~~~\nafter", 1, -1),
        ("before\n```mermaid\nflowchart TD\nA[\n```\nafter", 1, -1),
        ("before\n```rust\nflowchart TD\nA --> B\n```\nafter", 1, 0),
    ] {
        cx.update(|window, cx| {
            editor.update(cx, |s, cx| {
                s.set_value(source, window, cx);
                s.set_live_preview(true, cx);
                s.select_source_range(0..0, window, cx);
                s.diagnostic_settle_preview();
                cx.notify();
            })
        });
        for _ in 0..4 {
            cx.refresh().unwrap();
            cx.run_until_parked();
        }
        cx.update(|_, cx| {
            let s = editor.read(cx);
            let (rows, error) = s.diagnostic_mermaid_row(row);
            match expected {
                1 => assert!(rows.is_some_and(|n| n > 0), "{source}: {error:?}"),
                -1 => assert!(rows.is_none() && error.is_some(), "{source}"),
                _ => assert_eq!((rows, error), (None, None)),
            }
            assert_eq!(s.value().as_ref(), source);
        });
        if expected == 1 {
            // A source Find result/selection spanning the diagram reveals all of it.
            cx.update(|window, cx| {
                editor.update(cx, |s, cx| {
                    s.select_source_range(0..source.len(), window, cx)
                })
            });
            cx.refresh().unwrap();
            cx.update(|_, cx| assert_eq!(editor.read(cx).diagnostic_mermaid_row(row).0, None));
            cx.update(|window, cx| {
                editor.update(cx, |s, cx| s.select_source_range(0..0, window, cx))
            });
            cx.update(|_, cx| {
                view.update(cx, |v, cx| {
                    v.1 = 190.;
                    cx.notify();
                })
            });
            for _ in 0..3 {
                cx.refresh().unwrap();
                cx.run_until_parked();
            }
            cx.update(|_, cx| assert!(editor.read(cx).diagnostic_mermaid_row(row).0.is_some()));
        }
    }
    // Replace a queued valid diagram with invalid syntax, then replace the error with prose.
    for source in [
        "before\n```mermaid\nflowchart TD\nA --> B\n```",
        "before\n```mermaid\nflowchart TD\nA[\n```",
        "plain text",
    ] {
        cx.update(|window, cx| {
            editor.update(cx, |s, cx| {
                s.set_value(source, window, cx);
                s.select_source_range(0..0, window, cx);
                s.diagnostic_settle_preview();
                cx.notify();
            })
        });
        cx.refresh().unwrap();
    }
    cx.run_until_parked();
    cx.refresh().unwrap();
    cx.update(|_, cx| {
        assert_eq!(editor.read(cx).value().as_ref(), "plain text");
        assert_eq!(editor.read(cx).diagnostic_mermaid_row(0), (None, None));
    });
}

#[gpui::test]
fn shared_mermaid_views_preserve_source_history_and_scroll(cx: &mut TestAppContext) {
    cx.update(gpui_component::init);
    let source = format!(
        "before\n```mermaid\nflowchart TD\nA --> B\n```\n{}",
        (0..100)
            .map(|i| format!("line {i:03}\n"))
            .collect::<String>()
    );
    let mut pair = None;
    let mut view = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let first = cx.new(|cx| {
            let mut state = InputState::new(window, cx)
                .multi_line(true)
                .default_value(source.clone());
            state.set_live_preview(true, cx);
            state.focus(window, cx);
            state.select_source_range(0..0, window, cx);
            state.diagnostic_settle_preview();
            state
        });
        let buffer = first.update(cx, |s, cx| s.shared_buffer(cx));
        let second = cx.new(|cx| {
            InputState::new(window, cx)
                .multi_line(true)
                .with_shared_buffer(buffer, cx)
        });
        pair = Some((first.clone(), second));
        let v = cx.new(|_| Live(first, 360.));
        view = Some(v.clone());
        Root::new(v, window, cx)
    });
    let (first, second) = pair.unwrap();
    let view = view.unwrap();
    for _ in 0..4 {
        cx.refresh().unwrap();
        cx.run_until_parked();
    }
    let at = source.find("line 060").unwrap();
    let row = source[..at].bytes().filter(|b| *b == b'\n').count();
    cx.update(|window, cx| {
        first.update(cx, |s, cx| {
            s.select_source_range(at..at, window, cx);
        })
    });
    cx.refresh().unwrap();
    cx.run_until_parked();
    cx.refresh().unwrap();
    let before = cx.update(|_, cx| first.read(cx).diagnostic_code_point(row, 0).y);
    cx.update(|window, cx| {
        Theme::change(ThemeMode::Dark, Some(window), cx);
        view.update(cx, |v, cx| {
            v.1 = 240.;
            cx.notify();
        });
    });
    for _ in 0..3 {
        cx.refresh().unwrap();
        cx.run_until_parked();
    }
    let after = cx.update(|_, cx| first.read(cx).diagnostic_code_point(row, 0).y);
    assert!(
        (before - after).abs() < px(2.),
        "scroll anchor moved: {before:?} -> {after:?}"
    );
    let b = source.find("B\n").unwrap();
    cx.update(|window, cx| {
        second.update(cx, |s, cx| {
            s.select_source_range(b..b + 1, window, cx);
            s.replace_selection_isolated("C", window, cx);
        })
    });
    cx.update(|window, cx| {
        first.update(cx, |s, cx| {
            s.select_source_range(0..0, window, cx);
            s.diagnostic_settle_preview();
            s.focus(window, cx);
        })
    });
    for _ in 0..4 {
        cx.refresh().unwrap();
        cx.run_until_parked();
    }
    cx.update(|_, cx| {
        assert!(first.read(cx).value().contains("A --> C"));
        assert_eq!(first.read(cx).value(), second.read(cx).value());
        assert!(first.read(cx).diagnostic_mermaid_row(1).0.is_some());
    });
    cx.dispatch_action(Undo);
    cx.update(|_, cx| {
        assert_eq!(first.read(cx).value().as_ref(), source);
        assert_eq!(first.read(cx).value(), second.read(cx).value());
    });
}
