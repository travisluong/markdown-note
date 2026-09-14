//! Compact, read-only help for the formatter's existing Moment-style syntax.
use gpui::{App, IntoElement, div, prelude::*};
use gpui_component::{ActiveTheme, StyledExt};

const EXAMPLES: &[(&str, &str)] = &[
    ("YYYY-MM-DD", "2026-09-03"),
    ("MM/DD/YYYY", "09/03/2026"),
    ("DD/MM/YYYY", "03/09/2026"),
    ("MMMM D, YYYY", "September 3, 2026"),
    ("ddd, MMM D, YYYY", "Thu, Sep 3, 2026"),
    ("YYYY-MM-DD HH:mm", "2026-09-03 14:05"),
];

const TOKENS: &[(&str, &str)] = &[
    ("YYYY / YY", "Year: 2026 / 26"),
    ("M / MM", "Month: 9 / 09"),
    ("MMM / MMMM", "Month name: Sep / September"),
    ("D / DD / Do", "Day: 3 / 03 / 3rd"),
    ("ddd / dddd", "Weekday: Thu / Thursday"),
    ("H / HH", "24-hour time: 14 / 14"),
    ("h / hh", "12-hour time: 2 / 02"),
    ("m / mm", "Minutes: 5 / 05"),
    ("s / ss", "Seconds: 6 / 06"),
    ("A / a", "AM or PM: PM / pm"),
];

pub(super) fn render(cx: &App) -> impl IntoElement {
    let rows = |group: &'static str, values: &'static [(&'static str, &'static str)]| {
        div()
            .w_full()
            .min_w_0()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .gap_1()
            .children(values.iter().enumerate().map(|(index, (pattern, output))| {
                div()
                    .debug_selector(move || format!("date-{group}-row-{index}"))
                    .w_full()
                    .min_w_0()
                    .flex_shrink_0()
                    .grid()
                    .items_start()
                    .grid_cols(2)
                    .gap_x_3()
                    .child(
                        div()
                            .debug_selector(move || format!("date-{group}-pattern-{index}"))
                            .min_w_0()
                            .font_family(cx.theme().mono_font_family.clone())
                            .child(*pattern),
                    )
                    .child(
                        div()
                            .debug_selector(move || format!("date-{group}-output-{index}"))
                            .min_w_0()
                            .child(*output),
                    )
            }))
    };
    div()
        .debug_selector(|| "date-format-reference".into())
        .w_full().min_w_0().flex_shrink_0().flex().flex_col().gap_2().text_sm()
        .child(div().font_semibold().child("Quick reference"))
        .child("Examples for September 3, 2026 at 14:05:06.")
        .child(rows("examples", EXAMPLES))
        .child(div().font_semibold().child("Common tokens"))
        .child(rows("tokens", TOKENS))
        .child("Tokens are case-sensitive: MM is the month; mm is minutes.")
        .child("Uses local time and English names. Put literal text in [brackets], for example [Today] YYYY-MM-DD.")
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::TestAppContext;

    #[test]
    fn reference_examples_match_the_formatter() {
        let date = chrono::DateTime::parse_from_rfc3339("2026-09-03T14:05:06-05:00").unwrap();
        for &(pattern, expected) in EXAMPLES {
            assert_eq!(
                mithril::templates::format(pattern, &date).unwrap(),
                expected
            );
        }
        for &(pattern, description) in TOKENS {
            let expected = description.split_once(": ").unwrap().1;
            assert_eq!(
                mithril::templates::format(pattern, &date).unwrap(),
                expected
            );
        }
        assert_eq!(
            mithril::templates::format("[Today] YYYY-MM-DD", &date).unwrap(),
            "Today 2026-09-03"
        );
    }
    #[gpui::test]
    fn date_reference_rows_do_not_overlap(cx: &mut TestAppContext) {
        use gpui::{AppContext, Context, Entity, Render, Window};
        use gpui_component::Root;
        struct ReferenceView;
        impl Render for ReferenceView {
            fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
                div().w_full().p_4().child(super::render(cx))
            }
        }
        cx.update(gpui_component::init);
        let (_, cx) = cx.add_window_view(|window, cx| {
            let view: Entity<ReferenceView> = cx.new(|_| ReferenceView);
            Root::new(view, window, cx)
        });
        let selectors = ["examples", "tokens"].map(|group| {
            let count = if group == "examples" { 6 } else { 10 };
            (0..count)
                .map(|index| {
                    ["row", "pattern", "output"].map(|part| {
                        &*Box::leak(format!("date-{group}-{part}-{index}").into_boxed_str())
                    })
                })
                .collect::<Vec<_>>()
        });
        for font_size in [14, 24] {
            cx.update(|window, _| window.set_rem_size(gpui::px(font_size as f32)));
            // Keep every row onscreen so debug bounds cannot come from an older
            // clipped frame. These widths span the actual settings content widths.
            for width in [486., 418., 272.] {
                let height = 3000.;
                cx.simulate_resize(gpui::size(gpui::px(width), gpui::px(height)));
                cx.refresh().unwrap();
                cx.run_until_parked();
                cx.refresh().unwrap();
                for group in &selectors {
                    let mut previous_bottom = gpui::px(0.);
                    for [row_id, pattern_id, output_id] in group {
                        let row = cx.debug_bounds(row_id).unwrap();
                        let pattern = cx.debug_bounds(pattern_id).unwrap();
                        let output = cx.debug_bounds(output_id).unwrap();
                        assert!(
                            pattern.top().min(output.top()) >= previous_bottom,
                            "{row_id}: previous bottom {previous_bottom:?}, pattern {pattern:?}, output {output:?}"
                        );
                        assert!(
                            pattern.left() >= row.left()
                                && output.right() <= row.right() + gpui::px(0.5),
                            "{row_id}: {row:?}, {pattern:?}, {output:?}"
                        );
                        assert!(
                            output.left() >= pattern.right() || output.top() >= pattern.bottom(),
                            "{output_id}: {output:?} overlaps {pattern:?}"
                        );
                        assert!(
                            output.size.height >= gpui::px(font_size as f32 * 0.875),
                            "{output_id}: {output:?}"
                        );
                        previous_bottom = pattern.bottom().max(output.bottom());
                    }
                }
            }
        }
    }
}
