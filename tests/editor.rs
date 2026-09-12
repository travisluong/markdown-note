#![allow(dead_code)]
// Exercise the vendored editor modules without enabling unrelated component test dependencies.
extern crate gpui_component as ropey;
pub use gpui_component::{ActiveTheme, highlighter, markdown_images};
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

#[path = "../vendor/gpui-component/src/input/fenced_code.rs"]
mod fenced_code;

#[cfg(feature = "editor-diagnostics")]
#[test]
fn production_preview_parser_counts_gfm_work_once() {
    use gpui_component::input::{PreviewParseWork, diagnostic_preview_parse_work};
    assert_eq!(
        diagnostic_preview_parse_work("plain café 你好\r\ntext"),
        PreviewParseWork::default()
    );
    let mixed = "[café](note.md)\r\n\r\n| a | b |\r\n| - | - |\r\n| c | d |\r\n\r\n![image](image.png)\r\n\r\n```json\r\n{}\r\n```\r\n";
    assert_eq!(
        diagnostic_preview_parse_work(mixed),
        PreviewParseWork {
            gfm_calls: 1,
            gfm_bytes: mixed.len()
        }
    );
}
