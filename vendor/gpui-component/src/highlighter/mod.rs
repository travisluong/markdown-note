#[cfg(feature = "editor-diagnostics")]
#[doc(hidden)]
pub mod code;
#[cfg(not(feature = "editor-diagnostics"))]
pub(crate) mod code;
mod diagnostics;
mod highlighter;
mod languages;
mod registry;

pub use diagnostics::*;
pub use highlighter::*;
pub use languages::*;
pub use registry::*;
