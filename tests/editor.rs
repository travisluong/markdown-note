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
#[path = "../vendor/gpui-component/src/input/scroll.rs"]
mod scroll;
#[path = "../vendor/gpui-component/src/input/text_wrapper.rs"]
mod text_wrapper;
