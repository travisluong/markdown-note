//! Palette ranking and selection, independent of widgets and task execution.
use super::Command;
use crate::notebook;
use std::path::PathBuf;

#[derive(Clone)]
pub enum PaletteEntry {
    Command(Command),
    File(notebook::Entry),
    FileSort {
        sort: notebook::FileSort,
        label: &'static str,
        active: bool,
    },
    Notebook {
        path: PathBuf,
        active: bool,
    },
}

impl PaletteEntry {
    pub fn label(&self) -> String {
        match self {
            Self::Command(command) => command.label().into(),
            Self::FileSort { label, active, .. } => {
                format!("{label}{}", if *active { " (current)" } else { "" })
            }
            Self::Notebook { path, active } => format!(
                "{}{}",
                notebook::name(path),
                if *active { " (active)" } else { "" }
            ),
            Self::File(entry) => format!(
                "{}{}",
                if entry.directory { "▸ " } else { "" },
                notebook::name(&entry.relative)
            ),
        }
    }

    fn search_name(&self) -> String {
        match self {
            Self::Command(command) => command.label().into(),
            Self::FileSort { label, .. } => (*label).into(),
            Self::Notebook { path, .. } => notebook::name(path),
            Self::File(entry) => notebook::name(&entry.relative),
        }
    }

    pub fn detail(&self) -> String {
        match self {
            Self::Command(command) => command.description().into(),
            Self::FileSort { .. } => "Sort files in the active notebook".into(),
            Self::Notebook { path, .. } => notebook::display_path(path),
            Self::File(entry) => entry.relative.display().to_string(),
        }
    }

    fn rank(&self, query: &str) -> Option<(u8, usize)> {
        score(&self.search_name().to_lowercase(), query).or_else(|| match self {
            Self::File(_) | Self::Notebook { .. } => {
                score(&self.detail().to_lowercase(), query).map(|(_, cost)| (4, cost))
            }
            _ => None,
        })
    }
}

pub(super) fn score(name: &str, query: &str) -> Option<(u8, usize)> {
    if name == query {
        return Some((0, 0));
    }
    if name.starts_with(query) {
        return Some((1, name.len()));
    }
    if let Some(i) = name.find(query) {
        return Some((2, i));
    }
    let mut chars = name.chars().enumerate();
    let mut cost = 0;
    for wanted in query.chars() {
        cost += chars.find(|(_, c)| *c == wanted)?.0;
    }
    Some((3, cost))
}

pub(super) fn ranked_entries(entries: &[PaletteEntry], query: &str) -> Vec<PaletteEntry> {
    let query = query.trim().to_lowercase();
    let mut ranked: Vec<_> = entries
        .iter()
        .filter_map(|entry| entry.rank(&query).map(|rank| (rank, entry.clone())))
        .collect();
    ranked.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| a.1.search_name().cmp(&b.1.search_name()))
            .then_with(|| a.1.detail().cmp(&b.1.detail()))
    });
    ranked.into_iter().map(|(_, entry)| entry).collect()
}

/// Owns ranked entries and selection; the view owns query input, focus and scroll.
/// Changing a filter resets selection against the newly ranked results.
pub struct PaletteModel {
    entries: Vec<PaletteEntry>,
    results: Vec<PaletteEntry>,
    selected: usize,
}

impl PaletteModel {
    pub fn new(entries: Vec<PaletteEntry>) -> Self {
        let mut model = Self {
            entries,
            results: Vec::new(),
            selected: 0,
        };
        model.filter("");
        model
    }
    pub fn results(&self) -> &[PaletteEntry] {
        &self.results
    }
    pub fn selected_index(&self) -> usize {
        self.selected
    }
    pub fn selected_entry(&self) -> Option<&PaletteEntry> {
        self.results.get(self.selected)
    }
    pub fn filter(&mut self, query: &str) {
        self.results = ranked_entries(&self.entries, query);
        self.selected = 0;
    }
    pub fn replace_entries(&mut self, entries: Vec<PaletteEntry>, query: &str) {
        self.entries = entries;
        self.filter(query);
    }
    pub fn move_selection(&mut self, down: bool) -> usize {
        self.selected = if down {
            self.selected
                .saturating_add(1)
                .min(self.results.len().saturating_sub(1))
        } else {
            self.selected.saturating_sub(1)
        };
        self.selected
    }
    /// A stale row index must not activate a different surviving result.
    pub fn select(&mut self, index: usize) -> bool {
        if index >= self.results.len() {
            return false;
        }
        self.selected = index;
        true
    }
}
