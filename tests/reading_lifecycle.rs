//! Real Reading views on GPUI TestPlatform, with controlled background completions.
#![cfg(feature = "editor-diagnostics")]

use gpui::{
    Context, Entity, IntoElement, Render, SharedString, TestAppContext, VisualTestContext, Window,
    div, prelude::*, px, rems,
};
use gpui_component::{
    Root,
    text::{
        ReadingParseControl, ReadingProbe, ReadingSnapshot, SourceHighlights, TextView,
        TextViewStyle,
    },
};
use std::sync::{Arc, Mutex};

struct ReadingView {
    source: SharedString,
    html: bool,
    style: TextViewStyle,
    heading_scale: f32,
    search: Option<SourceHighlights>,
    control: ReadingParseControl,
    probe: Option<ReadingProbe>,
    task_calls: Arc<Mutex<Vec<(String, bool)>>>,
}

impl Render for ReadingView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let scale = self.heading_scale;
        let style = self
            .style
            .clone()
            .heading_font_size(move |level, base| base * scale + px(level as f32));
        let expected = self.source.clone();
        let task_calls = self.task_calls.clone();
        let view = if self.html {
            TextView::html("reading", self.source.clone(), window, cx)
        } else {
            TextView::markdown("reading", self.source.clone(), window, cx)
        }
        .style(style)
        .scrollable(true)
        .source_highlights(self.search.clone())
        .on_task_toggle(move |_, rendered_source, _, _| {
            task_calls.lock().unwrap().push((
                rendered_source.to_owned(),
                rendered_source == expected.as_ref(),
            ));
        })
        // Fresh callbacks on every render must not restart parsing.
        .code_block_actions(|_, _, _| div())
        .diagnostic_parse_control(self.control.clone());
        self.probe = Some(view.diagnostic_probe());
        div().size_full().child(view)
    }
}

fn fixture<'a>(
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
            html: false,
            style: TextViewStyle::default(),
            heading_scale: 1.,
            search: None,
            control: control.clone(),
            probe: None,
            task_calls: Arc::default(),
        });
        view = Some(entity.clone());
        Root::new(entity, window, cx)
    });
    cx.refresh().unwrap();
    cx.run_until_parked();
    (view.unwrap(), control, cx)
}

fn snapshot(view: &Entity<ReadingView>, cx: &mut VisualTestContext) -> ReadingSnapshot {
    cx.update(|_, cx| view.read(cx).probe.as_ref().unwrap().snapshot(cx).unwrap())
}

fn source(view: &Entity<ReadingView>, value: &str, cx: &mut VisualTestContext) {
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.source = value.to_owned().into();
            cx.notify();
        })
    });
    cx.run_until_parked();
}

fn complete(control: &ReadingParseControl, cx: &mut VisualTestContext) {
    control.complete(Ok(()));
    cx.run_until_parked();
}

#[gpui::test]
fn cold_layout_waits_for_worker_and_identical_renders_do_not_parse(cx: &mut TestAppContext) {
    let (view, control, cx) = fixture(cx, "# café 你好");
    let cold = snapshot(&view, cx);
    assert_eq!(cold.generation, 1);
    assert_eq!(cold.running, Some(1));
    assert!(cold.pending.is_none());
    assert!(
        cold.displayed_source.is_none(),
        "layout must not parse before the controlled worker"
    );
    assert_eq!(control.started(), 1);
    complete(&control, cx);
    assert_eq!(
        snapshot(&view, cx)
            .displayed_source
            .as_ref()
            .map(|source| source.as_ref()),
        Some("# café 你好")
    );
    for _ in 0..4 {
        cx.update(|_, cx| view.update(cx, |_, cx| cx.notify()));
        cx.run_until_parked();
    }
    assert_eq!(control.started(), 1);
    assert!(snapshot(&view, cx).running.is_none());
}

#[gpui::test]
fn cold_superseded_success_is_discarded_and_only_latest_pending_runs(cx: &mut TestAppContext) {
    let (view, control, cx) = fixture(cx, "A");
    source(&view, "B", cx);
    source(&view, "C", cx);
    let queued = snapshot(&view, cx);
    assert_eq!((queued.running, queued.pending), (Some(1), Some(3)));
    assert_eq!(control.started(), 1);
    complete(&control, cx);
    let superseded = snapshot(&view, cx);
    assert!(superseded.displayed_source.is_none());
    assert_eq!((superseded.running, superseded.pending), (Some(3), None));
    assert_eq!(control.started(), 2, "B must never run");
    complete(&control, cx);
    assert_eq!(
        snapshot(&view, cx)
            .displayed_source
            .as_ref()
            .map(|source| source.as_ref()),
        Some("C")
    );
}

#[gpui::test]
fn stale_errors_retain_the_tree_and_current_errors_recover(cx: &mut TestAppContext) {
    let (view, control, cx) = fixture(cx, "A");
    complete(&control, cx);
    source(&view, "B", cx);
    source(&view, "C", cx);
    source(&view, "D", cx);
    assert_eq!(
        snapshot(&view, cx)
            .displayed_source
            .as_ref()
            .map(|source| source.as_ref()),
        Some("A")
    );
    control.complete(Err("obsolete error".into()));
    cx.run_until_parked();
    let stale = snapshot(&view, cx);
    assert!(stale.error.is_none());
    assert_eq!(
        stale
            .displayed_source
            .as_ref()
            .map(|source| source.as_ref()),
        Some("A")
    );
    assert_eq!((stale.running, stale.pending), (Some(4), None));
    assert_eq!(control.started(), 3);
    complete(&control, cx);
    source(&view, "current failure", cx);
    control.complete(Err("current error".into()));
    cx.run_until_parked();
    assert_eq!(
        snapshot(&view, cx)
            .error
            .as_ref()
            .map(|error| error.as_ref()),
        Some("current error")
    );
    source(&view, "current failure", cx);
    assert_eq!(
        control.started(),
        4,
        "unchanged error render must not retry indefinitely"
    );
    source(&view, "recovered", cx);
    complete(&control, cx);
    assert_eq!(
        snapshot(&view, cx)
            .displayed_source
            .as_ref()
            .map(|source| source.as_ref()),
        Some("recovered")
    );
}

#[gpui::test]
fn pending_parse_uses_latest_style_without_restarting_for_closures(cx: &mut TestAppContext) {
    let (view, control, cx) = fixture(cx, "# heading\n\n```rust\nlet value = 1;\n```");
    let mut block = div().opacity(0.4);
    let block_style = block.style().clone();
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.style.paragraph_gap = rems(2.);
            view.style.heading_base_font_size = px(20.);
            view.style.code_block = block_style.clone();
            view.style.is_dark = true;
            view.heading_scale = 2.;
            cx.notify();
        })
    });
    cx.run_until_parked();
    assert_eq!(control.started(), 1);
    complete(&control, cx);
    let style = snapshot(&view, cx).style.unwrap();
    assert_eq!(style.paragraph_gap, rems(2.));
    assert_eq!(style.code_block, block_style);
    assert!(style.is_dark);
    assert_eq!(
        style.heading_font_size.as_ref().unwrap()(2, style.heading_base_font_size),
        px(42.)
    );
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.heading_scale = 3.;
            cx.notify();
        })
    });
    cx.run_until_parked();
    let style = snapshot(&view, cx).style.unwrap();
    assert_eq!(
        style.heading_font_size.as_ref().unwrap()(2, style.heading_base_font_size),
        px(62.)
    );
    assert_eq!(control.started(), 1);
}

#[gpui::test]
fn stale_tree_callbacks_keep_rendered_source_and_find_waits_for_matching_parse(
    cx: &mut TestAppContext,
) {
    let old = "- [ ] old task";
    let (view, control, cx) = fixture(cx, old);
    complete(&control, cx);
    let latest = "- [ ] latest task";
    source(&view, latest, cx);
    cx.update(|window, cx| {
        let probe = view.read(cx).probe.clone().unwrap();
        probe.activate_rendered_task(3, window, cx);
    });
    cx.update(|_, cx| {
        assert_eq!(
            *view.read(cx).task_calls.lock().unwrap(),
            vec![(old.to_owned(), false)]
        );
    });
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.search = Some(SourceHighlights::new(
                latest.into(),
                std::iter::once(6..12).collect(),
                Some(6..12),
            ));
            cx.notify();
        })
    });
    cx.run_until_parked();
    let pending = snapshot(&view, cx);
    assert!(!pending.has_highlights);
    assert!(!pending.search_started);
    complete(&control, cx);
    let current = snapshot(&view, cx);
    assert!(current.has_highlights);
    assert!(current.search_started);
    cx.update(|window, cx| {
        let probe = view.read(cx).probe.clone().unwrap();
        probe.activate_rendered_task(3, window, cx);
    });
    cx.update(|_, cx| {
        assert_eq!(
            view.read(cx).task_calls.lock().unwrap().last().unwrap(),
            &(latest.to_owned(), true)
        )
    });
}

#[gpui::test]
fn initial_find_reveals_once_and_list_scroll_survives_pending_updates(cx: &mut TestAppContext) {
    let text = (0..200)
        .map(|index| format!("paragraph {index}\n\n"))
        .collect::<String>();
    let start = text.find("paragraph 180").unwrap();
    let (view, control, cx) = fixture(cx, &text);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.search = Some(SourceHighlights::new(
                text.clone().into(),
                std::iter::once(start..start + 13).collect(),
                Some(start..start + 13),
            ));
            cx.notify();
        })
    });
    cx.run_until_parked();
    assert!(!snapshot(&view, cx).search_started);
    complete(&control, cx);
    cx.refresh().unwrap();
    cx.run_until_parked();
    let revealed = snapshot(&view, cx);
    assert!(revealed.search_started);
    assert_eq!(revealed.item_count, 200);
    cx.update(|_, cx| {
        assert!(
            view.read(cx)
                .probe
                .as_ref()
                .unwrap()
                .item_is_vertically_visible(180, cx),
            "find must reveal the match, including when the viewport clamps near the end"
        );
    });
    cx.update(|_, cx| {
        view.read(cx).probe.as_ref().unwrap().scroll_to(10, cx);
        view.update(cx, |_, cx| cx.notify());
    });
    cx.run_until_parked();
    assert_eq!(
        snapshot(&view, cx).scroll_item,
        10,
        "an unchanged find must not reveal again"
    );
    source(&view, &text.replace("paragraph 0", "updated 0"), cx);
    let pending = snapshot(&view, cx);
    assert_eq!(pending.scroll_item, 10);
    assert_eq!(pending.item_count, 200);
    assert!(!pending.has_highlights);
    complete(&control, cx);
    assert_eq!(snapshot(&view, cx).scroll_item, 10);
}

#[gpui::test]
fn switching_same_key_between_markdown_and_html_schedules_new_parse(cx: &mut TestAppContext) {
    let text = "<p>HTML café</p>";
    let (view, control, cx) = fixture(cx, text);
    complete(&control, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.html = true;
            cx.notify();
        })
    });
    cx.run_until_parked();
    assert_eq!(control.started(), 2);
    assert_eq!(snapshot(&view, cx).running, Some(2));
    complete(&control, cx);
    assert_eq!(
        snapshot(&view, cx)
            .displayed_source
            .as_ref()
            .map(|source| source.as_ref()),
        Some(text)
    );
    assert!(snapshot(&view, cx).error.is_none());
}

#[gpui::test]
fn closing_view_cancels_delivery_of_its_held_parse(cx: &mut TestAppContext) {
    let (view, control, cx) = fixture(cx, "pending");
    let probe = cx.update(|window, cx| {
        let probe = view.read(cx).probe.clone().unwrap();
        window.remove_window();
        probe
    });
    cx.run_until_parked();
    control.complete(Ok(()));
    cx.run_until_parked();
    TestAppContext::update(cx, |cx| assert!(probe.snapshot(cx).is_none()));
}
