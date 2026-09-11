use gpui::{Pixels, px};

/// Reveal the caret's line with the smallest possible viewport movement.
/// A visible line must stay put, including at either edge of the viewport.
pub(super) fn reveal_line(
    offset: Pixels,
    line_top: Pixels,
    line_height: Pixels,
    viewport_height: Pixels,
) -> Pixels {
    let visible_height = line_height.min(viewport_height).max(px(0.));
    let minimum = -line_top;
    let maximum = viewport_height - line_top - visible_height;
    offset.clamp(minimum, maximum).min(px(0.))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_on_visible_lines_keeps_the_viewport_fixed() {
        for top in [100., 105., 120., 250., 460., 480.] {
            let mut offset = px(-100.);
            for _ in 0..20 {
                offset = reveal_line(offset, px(top), px(20.), px(400.));
                assert_eq!(offset, px(-100.));
            }
        }
    }

    #[test]
    fn enter_inside_viewport_does_not_scroll() {
        assert_eq!(
            reveal_line(px(-100.), px(460.), px(20.), px(400.)),
            px(-100.)
        );
    }

    #[test]
    fn enter_at_bottom_scrolls_once_to_reveal_new_line() {
        let offset = reveal_line(px(-100.), px(500.), px(20.), px(400.));
        assert_eq!(offset, px(-120.));
        assert_eq!(reveal_line(offset, px(500.), px(20.), px(400.)), offset);
    }

    #[test]
    fn clipped_lines_scroll_only_by_the_missing_height() {
        assert_eq!(
            reveal_line(px(-100.), px(490.), px(20.), px(400.)),
            px(-110.)
        );
        assert_eq!(reveal_line(px(-100.), px(95.), px(20.), px(400.)), px(-95.));
        assert_eq!(
            reveal_line(px(-100.), px(470.), px(40.), px(400.)),
            px(-110.)
        );
    }

    #[test]
    fn line_taller_than_viewport_does_not_oscillate() {
        let offset = reveal_line(px(-100.), px(120.), px(40.), px(10.));
        assert_eq!(offset, px(-120.));
        assert_eq!(reveal_line(offset, px(120.), px(40.), px(10.)), offset);
    }
}

/// Single-line text never scrolls vertically; empty fields never scroll at all.
/// Normalize deferred caret offsets before painting, not just when saving them.
pub(super) fn text_offset(
    mut offset: gpui::Point<Pixels>,
    empty: bool,
    single_line: bool,
) -> gpui::Point<Pixels> {
    if empty {
        offset.x = px(0.);
    }
    if empty || single_line {
        offset.y = px(0.);
    }
    offset
}

#[cfg(test)]
mod focus_tests {
    use super::*;
    use gpui::point;
    #[test]
    fn placeholder_origin_survives_fractional_focus_scroll() {
        let origin = point(px(8.), px(7.25));
        for deferred in [
            point(px(0.), px(0.)),
            point(px(-1.), px(-0.25)),
            point(px(0.), px(-0.5)),
        ] {
            assert_eq!(origin + text_offset(deferred, true, true), origin);
        }
    }
    #[test]
    fn populated_inputs_keep_horizontal_scrolling_and_editors_keep_both_axes() {
        let offset = point(px(-120.), px(-0.25));
        assert_eq!(text_offset(offset, false, true), point(px(-120.), px(0.)));
        assert_eq!(text_offset(offset, false, false), offset);
    }
}
