use gpui::SharedString;
use std::{
    ops::Range,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

/// Search presentation belongs to an exact source snapshot. Clone to retain a
/// one-shot reveal across renders; construct a new value to navigate again.
#[derive(Clone)]
pub struct SourceHighlights {
    pub(crate) source: SharedString,
    pub(crate) matches: Arc<Vec<Range<usize>>>,
    pub(crate) active: Option<Range<usize>>,
    pub(crate) started: Arc<AtomicBool>,
    pub(crate) revealed: Arc<AtomicBool>,
    pub(crate) cancelled: Arc<AtomicBool>,
    pub(crate) horizontal_started: Arc<AtomicBool>,
    hidden: Arc<AtomicBool>,
}
impl SourceHighlights {
    pub fn new(
        source: SharedString,
        matches: Vec<Range<usize>>,
        active: Option<Range<usize>>,
    ) -> Self {
        let valid =
            |range: &Range<usize>| range.start < range.end && source.get(range.clone()).is_some();
        let mut matches: Vec<Range<usize>> = matches.into_iter().filter(valid).collect();
        matches.sort_by_key(|range| range.start);
        let mut merged: Vec<Range<usize>> = Vec::new();
        for range in matches {
            if let Some(last) = merged.last_mut().filter(|last| last.end > range.start) {
                last.end = last.end.max(range.end);
            } else {
                merged.push(range);
            }
        }
        let matches = merged;
        let active = active.filter(valid);
        Self {
            source,
            matches: Arc::new(matches),
            active,
            started: Arc::new(AtomicBool::new(false)),
            revealed: Arc::new(AtomicBool::new(false)),
            cancelled: Arc::new(AtomicBool::new(false)),
            horizontal_started: Arc::new(AtomicBool::new(false)),
            hidden: Arc::new(AtomicBool::new(false)),
        }
    }
    pub(crate) fn same_request(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.started, &other.started)
    }
    pub(crate) fn matches_snapshot(&self, source: &str) -> bool {
        self.source.as_ref() == source
    }
    pub fn hidden_match(&self) -> bool {
        self.hidden.load(Ordering::Relaxed)
    }
    pub(crate) fn set_hidden(&self, hidden: bool) {
        self.hidden.store(hidden, Ordering::Relaxed);
    }
}

#[derive(Clone)]
pub(crate) struct RevealTarget {
    pub highlights: SourceHighlights,

    pub item: usize,
}
