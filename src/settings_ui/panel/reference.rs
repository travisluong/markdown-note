//! Compact, read-only help for the formatter's existing Moment-style syntax.
use gpui::{App, IntoElement, div, prelude::*, px};
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
    let rows = |values: &'static [(&'static str, &'static str)]| {
        div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap_1()
            .children(values.iter().map(|(pattern, output)| {
                div()
                    .w_full()
                    .min_w_0()
                    .flex()
                    .flex_wrap()
                    .gap_x_3()
                    .child(
                        div()
                            .w(px(172.))
                            .font_family(cx.theme().mono_font_family.clone())
                            .child(*pattern),
                    )
                    .child(div().min_w_0().child(*output))
            }))
    };
    div()
        .debug_selector(|| "date-format-reference".into())
        .w_full().min_w_0().flex().flex_col().gap_2().text_sm()
        .child(div().font_semibold().child("Quick reference"))
        .child("Examples for September 3, 2026 at 14:05:06.")
        .child(rows(EXAMPLES))
        .child(div().font_semibold().child("Common tokens"))
        .child(rows(TOKENS))
        .child("Tokens are case-sensitive: MM is the month; mm is minutes.")
        .child("Uses local time and English names. Put literal text in [brackets], for example [Today] YYYY-MM-DD.")
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
