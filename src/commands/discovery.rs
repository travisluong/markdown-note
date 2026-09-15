//! Recursive notebook discovery uses the shared path and link policy.
use crate::notebook;
use std::{
    fs, io,
    path::{Path, PathBuf},
};

pub fn discover(
    root: &Path,
    exclusions: &notebook::Exclusions,
) -> io::Result<Vec<notebook::Entry>> {
    if notebook::is_link(&fs::symlink_metadata(root)?) {
        return Err(io::Error::other("Notebook links are not traversed."));
    }
    let mut pending = vec![PathBuf::new()];
    let mut found = Vec::new();
    while let Some(path) = pending.pop() {
        for entry in notebook::read_directory_filtered(root, &path, exclusions)? {
            if entry.link {
                continue;
            }
            if entry.directory {
                pending.push(entry.relative.clone());
            }
            if entry.directory || notebook::markdown(&entry.relative) {
                found.push(entry);
            }
        }
    }
    Ok(found)
}
