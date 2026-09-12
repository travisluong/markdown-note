//! Notebook domain facade: schemas, path rules, tree values, and filesystem operations.
//! Callers use these exports; child modules name the owners and keep dependencies explicit.

mod paths;
mod settings;
mod storage;
mod tree;

pub use paths::{
    display_path, is_link, markdown, name, remap_path, resolve, safe_relative, validate_name,
};
pub use settings::{
    Appearance, Autosave, Index, Settings, ThemeChoice, ThemePalette,
    default_always_hide_extensions, default_show_file_extensions, default_sidebar_width,
    normalize_sidebar_width, parse_hidden_extensions,
};
pub use storage::{
    Notebook, create_untitled, index_path, load_index, move_entry, read_directory, save_state,
    settings_path, write_json,
};
pub use tree::{Entry, FileSort, sidebar_file_name};
