//! Application contracts and explicit feature owners.
//!
//! Data, storage, and policy modules do not import the window; `editor` is the
//! thin GPUI adapter. UI coordination lives in the executable.

pub mod capabilities;
pub mod commands;
pub mod document;
pub mod editor;
pub mod explorer;
pub mod markdown_edit;
pub mod notebook;
pub mod search;
pub mod templates;
pub mod terminal;
mod view_mode;

pub use view_mode::ViewMode;
