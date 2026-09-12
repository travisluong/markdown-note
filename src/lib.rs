//! Application data and storage contracts, independent of the GPUI window.
//!
//! UI coordination lives in the executable. These modules can be exercised by
//! integration tests and extended without importing the application root.

pub mod capabilities;
pub mod commands;
pub mod document;
pub mod explorer;
pub mod notebook;
pub mod templates;
mod view_mode;

pub use view_mode::ViewMode;
