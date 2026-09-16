//! Geometry-only coverage for the vendored dialog placement contract.
use gpui::{point, px, size};
use gpui_component::dialog::DialogPlacement;

#[test]
fn default_dialogs_keep_the_existing_margin() {
    let placement = DialogPlacement::new(size(px(1000.), px(900.)));
    let bounds = placement.bounds(size(px(400.), px(300.)));
    assert_eq!(bounds.origin, point(px(16.), px(16.)));
    assert_eq!(bounds.size, size(px(368.), px(268.)));
}

#[test]
fn configured_margin_clamps_size_and_survives_updates() {
    let placement = DialogPlacement::new(size(px(1000.), px(900.))).with_margin(px(24.));
    let viewport = size(px(400.), px(300.));
    let bounds = placement.bounds(viewport);
    assert_eq!(bounds.origin, point(px(24.), px(24.)));
    assert_eq!(bounds.size, size(px(352.), px(252.)));
    placement.set_preferred_size(size(px(200.), px(160.)));
    let bounds = placement.bounds(viewport);
    assert_eq!(bounds.origin, point(px(100.), px(70.)));
    assert_eq!(bounds.size, size(px(200.), px(160.)));
    placement.reset(size(px(800.), px(800.)));
    assert_eq!(placement.bounds(viewport).origin, point(px(24.), px(24.)));
}
