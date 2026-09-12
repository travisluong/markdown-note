//! Directory entry values, deterministic sorting, and sidebar filename presentation.
//! Sorting keeps folders alphabetical and ahead of files, with missing timestamps last.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::paths::name;

#[derive(Clone, Debug)]
pub struct Entry {
    pub modified: Option<std::time::SystemTime>,
    pub created: Option<std::time::SystemTime>,
    pub relative: PathBuf,
    pub directory: bool,
    pub link: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileSort {
    #[default]
    NameAscending,
    NameDescending,
    ModifiedNewest,
    ModifiedOldest,
    CreatedNewest,
    CreatedOldest,
}

impl FileSort {
    pub const OPTIONS: [(Self, &'static str); 6] = [
        (Self::NameAscending, "File name (A to Z)"),
        (Self::NameDescending, "File name (Z to A)"),
        (Self::ModifiedNewest, "Modified time (new to old)"),
        (Self::ModifiedOldest, "Modified time (old to new)"),
        (Self::CreatedNewest, "Created time (new to old)"),
        (Self::CreatedOldest, "Created time (old to new)"),
    ];

    pub fn sort_entries(self, entries: &mut [Entry]) {
        use std::cmp::Ordering;
        let by_name = |a: &Entry, b: &Entry| {
            name(&a.relative)
                .to_lowercase()
                .cmp(&name(&b.relative).to_lowercase())
                .then_with(|| a.relative.cmp(&b.relative))
        };
        entries.sort_by(|a, b| {
            b.directory.cmp(&a.directory).then_with(|| {
                if a.directory {
                    return by_name(a, b);
                }
                let (left, right, newest) = match self {
                    Self::NameAscending => return by_name(a, b),
                    Self::NameDescending => return by_name(b, a),
                    Self::ModifiedNewest => (a.modified, b.modified, true),
                    Self::ModifiedOldest => (a.modified, b.modified, false),
                    Self::CreatedNewest => (a.created, b.created, true),
                    Self::CreatedOldest => (a.created, b.created, false),
                };
                match (left, right) {
                    (Some(left), Some(right)) => {
                        if newest {
                            right.cmp(&left)
                        } else {
                            left.cmp(&right)
                        }
                    }
                    (Some(_), None) => Ordering::Less,
                    (None, Some(_)) => Ordering::Greater,
                    (None, None) => Ordering::Equal,
                }
                .then_with(|| by_name(a, b))
            })
        });
    }
}

pub fn sidebar_file_name(path: &Path, show: bool, always_hide: &[String]) -> String {
    let full_name = name(path);
    let Some(extension) = path.extension().and_then(|extension| extension.to_str()) else {
        return full_name;
    };
    if extension.is_empty()
        || (show
            && !always_hide
                .iter()
                .any(|hidden| hidden.to_lowercase() == extension.to_lowercase()))
    {
        return full_name;
    }
    path.file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or(full_name)
}

#[cfg(test)]
mod tests;
