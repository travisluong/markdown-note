#![allow(dead_code)]
// Exercise the vendored editor modules without enabling unrelated component test dependencies.
extern crate gpui_component as ropey;
pub use gpui_component::ActiveTheme;
mod input {
    pub use gpui_component::input::RopeExt;
}
#[path = "../vendor/gpui-component/src/input/line_index.rs"]
mod line_index;
#[path = "../vendor/gpui-component/src/input/live_preview.rs"]
mod live_preview;
#[path = "../vendor/gpui-component/src/input/markdown_links.rs"]
mod markdown_links;
#[path = "../vendor/gpui-component/src/input/preview_syntax.rs"]
mod preview_syntax;
#[path = "../vendor/gpui-component/src/input/scroll.rs"]
mod scroll;
#[path = "../vendor/gpui-component/src/input/text_wrapper.rs"]
mod text_wrapper;

mod table {
    pub use gpui_component::input::table::*;
}

#[cfg(feature = "editor-diagnostics")]
#[test]
fn find_and_replace_literal_unicode_and_match_navigation() {
    gpui_component::input::InputState::diagnostic_find_matcher();
}
