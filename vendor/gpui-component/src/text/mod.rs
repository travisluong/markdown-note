mod format;
mod inline;
mod node;
mod style;
mod text_view;
mod utils;

use gpui::App;
pub use style::*;
pub use text_view::*;

pub(crate) fn init(cx: &mut App) {
    text_view::init(cx);
}

mod source_highlights;
pub use source_highlights::SourceHighlights;
mod source_map;
