//! Command catalogue and palette policies exposed without the application root.
mod catalogue;
mod discovery;
mod palette;

pub use catalogue::Command;
pub use discovery::discover;
pub use palette::{PaletteEntry, PaletteModel};

#[cfg(test)]
mod tests;
