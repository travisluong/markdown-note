//! Explorer directory results, row projection and keyboard selection.
//! This owner performs no filesystem IO and imports no application or GPUI state.
use crate::notebook;
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

#[derive(Clone)]
enum DirectoryState {
    Loading,
    Ready(Vec<notebook::Entry>),
    Failed(String),
}

#[derive(Clone)]
pub struct Row {
    pub path: PathBuf,
    pub depth: usize,
    pub directory: bool,
    pub disabled: bool,
    pub label: String,
    pub filename: Option<String>,
    pub retry: bool,
    pub draggable: bool,
}

/// A refresh identity, private to an Explorer instance. Retain the same owner
/// across notebook changes so delayed directory results cannot reuse identities.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Generation(u64);

/// Directory cache, rendered row projection and cursor form one owner.
/// GPUI adapters own focus, scroll and worker handles; notebook Settings owns
/// persisted expansion/sort choices. The predicate adds application image types
/// without coupling this policy to an image widget or importer.
pub struct Explorer {
    directories: BTreeMap<PathBuf, DirectoryState>,
    rows: Vec<Row>,
    cursor: Option<usize>,
    generation: Generation,
    supported_image: fn(&Path) -> bool,
}

impl Explorer {
    pub fn new(supported_image: fn(&Path) -> bool) -> Self {
        Self {
            directories: BTreeMap::new(),
            rows: Vec::new(),
            cursor: None,
            generation: Generation(0),
            supported_image,
        }
    }
    pub fn rows(&self) -> &[Row] {
        &self.rows
    }
    pub fn row(&self, index: usize) -> Option<&Row> {
        self.rows.get(index)
    }
    pub fn cursor(&self) -> Option<usize> {
        self.cursor
    }
    pub fn selected(&self) -> Option<&Row> {
        self.cursor.and_then(|index| self.row(index))
    }
    pub fn contains_directory(&self, path: &Path) -> bool {
        self.directories.contains_key(path)
    }
    pub fn is_current(&self, generation: Generation) -> bool {
        self.generation == generation
    }

    /// Invalidates workers and drops the previous notebook's projection.
    pub fn clear(&mut self) {
        self.begin_refresh();
        self.directories.clear();
        self.rows.clear();
        self.cursor = None;
    }

    /// Refresh metadata while keeping existing rows and keyboard selection visible.
    pub fn begin_refresh(&mut self) -> Generation {
        self.generation.0 = self
            .generation
            .0
            .checked_add(1)
            .expect("explorer generation overflow");
        // Workers from the prior generation can no longer finish these loads.
        // Keep completed caches and visible rows, but allow re-expansion to
        // request a current worker instead of preserving a permanent spinner.
        self.directories
            .retain(|_, state| !matches!(state, DirectoryState::Loading));
        self.generation
    }

    pub fn begin_directory(&mut self, path: PathBuf, settings: &notebook::Settings) -> Generation {
        self.directories.insert(path, DirectoryState::Loading);
        self.rebuild(Some(settings));
        self.generation
    }

    /// Return expanded child directories that still need loading. Stale workers
    /// cannot change either cache or row selection.
    pub fn complete_directory(
        &mut self,
        generation: Generation,
        path: PathBuf,
        result: Result<Vec<notebook::Entry>, String>,
        settings: &notebook::Settings,
    ) -> Option<Vec<PathBuf>> {
        if !self.is_current(generation) {
            return None;
        }
        let children = result
            .as_ref()
            .ok()
            .map(|entries| {
                entries
                    .iter()
                    .filter(|entry| {
                        entry.directory
                            && !entry.link
                            && settings.expanded.contains(&entry.relative)
                    })
                    .map(|entry| entry.relative.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        self.directories
            .insert(path, directory_state(result, settings));
        let children = children
            .into_iter()
            .filter(|path| !self.contains_directory(path))
            .collect();
        self.rebuild(Some(settings));
        Some(children)
    }

    /// Preserve directories opened while a batch refresh was running and request
    /// missing newly expanded paths after accepting the batch.
    pub fn complete_refresh(
        &mut self,
        generation: Generation,
        directories: Vec<(PathBuf, Result<Vec<notebook::Entry>, String>)>,
        settings: &notebook::Settings,
    ) -> Option<Vec<PathBuf>> {
        if !self.is_current(generation) {
            return None;
        }
        let mut refreshed: BTreeMap<_, _> = directories
            .into_iter()
            .map(|(path, result)| (path, directory_state(result, settings)))
            .collect();
        for path in &settings.expanded {
            if !refreshed.contains_key(path)
                && let Some(state) = self.directories.remove(path)
            {
                refreshed.insert(path.clone(), state);
            }
        }
        self.directories = refreshed;
        let missing = settings
            .expanded
            .iter()
            .filter(|path| !self.contains_directory(path))
            .cloned()
            .collect();
        self.rebuild(Some(settings));
        Some(missing)
    }

    pub fn rebuild(&mut self, settings: Option<&notebook::Settings>) {
        let selected = self.selected().map(|row| row.path.clone());
        let mut rows = Vec::new();
        if let Some(settings) = settings {
            append_rows(
                &self.directories,
                Path::new(""),
                0,
                settings,
                &mut rows,
                self.supported_image,
            );
        }
        self.rows = rows;
        self.cursor = selected.and_then(|path| self.rows.iter().position(|row| row.path == path));
    }

    pub fn resort(&mut self, settings: &notebook::Settings) {
        for state in self.directories.values_mut() {
            if let DirectoryState::Ready(entries) = state {
                settings.sort_entries(entries);
            }
        }
        self.rebuild(Some(settings));
    }

    pub fn select(&mut self, index: Option<usize>) -> bool {
        if index.is_some_and(|index| index >= self.rows.len()) {
            return false;
        }
        self.cursor = index;
        true
    }

    pub fn select_path(&mut self, path: &Path, directory: bool) -> Option<usize> {
        self.cursor = self
            .rows
            .iter()
            .position(|row| row.directory == directory && row.path == path);
        self.cursor
    }

    pub fn move_cursor(&mut self, down: bool) -> Option<usize> {
        if self.rows.is_empty() {
            return None;
        }
        let next = match self.cursor {
            Some(index) if down => index.saturating_add(1).min(self.rows.len() - 1),
            Some(index) => index.saturating_sub(1),
            None => 0,
        };
        self.cursor = Some(next);
        Some(next)
    }

    pub fn remap_visible_paths(&mut self, from: &Path, to: &Path) {
        for row in &mut self.rows {
            row.path = notebook::remap_path(&row.path, from, to);
        }
    }
}

fn directory_state(
    result: Result<Vec<notebook::Entry>, String>,
    settings: &notebook::Settings,
) -> DirectoryState {
    match result {
        Ok(mut entries) => {
            settings.sort_entries(&mut entries);
            DirectoryState::Ready(entries)
        }
        Err(error) => DirectoryState::Failed(error),
    }
}

fn append_rows(
    directories: &BTreeMap<PathBuf, DirectoryState>,
    path: &Path,
    depth: usize,
    settings: &notebook::Settings,
    rows: &mut Vec<Row>,
    supported_image: fn(&Path) -> bool,
) {
    match directories.get(path) {
        Some(DirectoryState::Ready(entries)) => {
            if entries.is_empty() {
                rows.push(Row {
                    path: path.to_path_buf(),
                    depth,
                    directory: false,
                    disabled: true,
                    label: "Empty folder".into(),
                    filename: None,
                    retry: false,
                    draggable: false,
                });
            }
            for entry in entries {
                rows.push(Row {
                    path: entry.relative.clone(),
                    depth,
                    directory: entry.directory,
                    disabled: entry.link
                        || (!entry.directory
                            && !(notebook::markdown(&entry.relative)
                                || supported_image(&entry.relative))),
                    filename: (!entry.directory).then(|| notebook::name(&entry.relative)),
                    label: format!(
                        "{}{}",
                        if entry.directory {
                            notebook::name(&entry.relative)
                        } else {
                            notebook::sidebar_file_name(
                                &entry.relative,
                                settings.show_file_extensions,
                                &settings.always_hide_extensions,
                            )
                        },
                        if entry.link { " (link)" } else { "" }
                    ),
                    retry: false,
                    draggable: !entry.link,
                });
                if entry.directory && !entry.link && settings.expanded.contains(&entry.relative) {
                    append_rows(
                        directories,
                        &entry.relative,
                        depth + 1,
                        settings,
                        rows,
                        supported_image,
                    );
                }
            }
        }
        Some(DirectoryState::Failed(error)) => rows.push(Row {
            path: path.to_path_buf(),
            depth,
            directory: false,
            disabled: false,
            label: format!("Retry: {error}"),
            filename: None,
            retry: true,
            draggable: false,
        }),
        _ => rows.push(Row {
            path: path.to_path_buf(),
            depth,
            directory: false,
            disabled: true,
            label: "Loading…".into(),
            filename: None,
            retry: false,
            draggable: false,
        }),
    }
}

#[cfg(test)]
mod tests;
