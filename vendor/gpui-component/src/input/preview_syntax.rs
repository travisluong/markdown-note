//! Background-only document analysis and a bounded, non-debounced request queue.
use super::{markdown_links::MarkdownLink, table::TableIndex};
use ropey::Rope;
use std::{ops::Range, sync::Arc};

pub(super) struct Request {
    pub revision: u64,
    pub text: Rope,
}
pub(super) struct Parsed {
    pub revision: u64,
    pub links: Vec<MarkdownLink>,
    pub tables: TableIndex,
}
impl Request {
    pub fn parse(self) -> Parsed {
        let source = self.text.to_string();
        let links = super::markdown_links::parse(&source);
        let mut tables = TableIndex::default();
        tables.rebuild_source(&source);
        Parsed {
            revision: self.revision,
            links,
            tables,
        }
    }
}

#[derive(Default)]
pub(super) struct Queue {
    pub revision: u64,
    running: Option<u64>,
    pending: Option<Request>,
    edits: Vec<Delta>,
    rebasable: bool,
}
impl Queue {
    pub fn request(&mut self, text: &Rope) {
        self.revision += 1;
        self.pending = Some(Request {
            revision: self.revision,
            text: text.clone(),
        });
    }
    pub fn edit(&mut self, range: Range<usize>, inserted: usize, safe: bool) {
        if self.running.is_none() {
            return;
        }
        if !safe || self.edits.len() == 64 {
            self.rebasable = false;
            self.edits.clear();
        } else if self.rebasable {
            self.edits.push(Delta { range, inserted });
        }
    }
    pub fn reset(&mut self) {
        self.rebasable = false;
        self.edits.clear();
        self.revision += 1;
        self.pending = None;
        // Keep the running slot until completion, even across mode changes.
    }
    pub fn start(&mut self) -> Option<Request> {
        if self.running.is_some() {
            return None;
        }
        let request = self.pending.take()?;
        self.running = Some(request.revision);
        self.rebasable = true;
        self.edits.clear();
        Some(request)
    }
    pub fn rebasing(&self, revision: u64) -> Option<Vec<Delta>> {
        if self.running == Some(revision) && (revision == self.revision || self.rebasable) {
            Some(self.edits.clone())
        } else {
            None
        }
    }
    pub fn finish(&mut self, revision: u64) -> bool {
        if self.running != Some(revision) {
            return false;
        }
        self.running = None;
        revision == self.revision
    }
    #[cfg(any(test, feature = "editor-diagnostics"))]
    pub fn jobs(&self) -> usize {
        usize::from(self.running.is_some()) + usize::from(self.pending.is_some())
    }
}

#[derive(Clone, Debug)]
pub(super) struct Delta {
    pub range: Range<usize>,
    pub inserted: usize,
}

/// Keep document offsets in the parsed snapshot. Edits translate range queries,
/// rather than rewriting every subsequent link on the input thread.
#[derive(Default)]
pub(super) struct Links {
    items: Arc<Vec<MarkdownLink>>,
    edits: Vec<Delta>,
}
impl Links {
    #[cfg(test)]
    pub fn new(items: Vec<MarkdownLink>) -> Self {
        Self {
            items: Arc::new(items),
            edits: Vec::new(),
        }
    }
    pub fn with_edits(items: Vec<MarkdownLink>, edits: Vec<Delta>) -> Self {
        Self {
            items: Arc::new(items),
            edits,
        }
    }
    pub fn edit(&mut self, range: Range<usize>, inserted: usize) {
        // Bound query work during arbitrarily long input bursts.
        if self.edits.len() == 64 {
            *self = Self::default();
        }
        if !self.items.is_empty() {
            self.edits.push(Delta { range, inserted });
        }
    }
    pub fn in_range(&self, mut range: Range<usize>) -> Vec<MarkdownLink> {
        for edit in self.edits.iter().rev() {
            let end = edit.range.start + edit.inserted;
            let delta = edit.inserted as isize - edit.range.len() as isize;
            if range.end <= edit.range.start {
                continue;
            }
            if range.start >= end {
                range = range.start.saturating_add_signed(-delta)
                    ..range.end.saturating_add_signed(-delta);
            } else {
                return Vec::new();
            }
        }
        let start = self
            .items
            .partition_point(|link| link.source.end <= range.start);
        self.items[start..]
            .iter()
            .take_while(|link| link.source.start < range.end)
            .filter_map(|link| {
                let mut link = link.clone();
                for edit in &self.edits {
                    if link.source.end <= edit.range.start {
                        continue;
                    }
                    if link.source.start < edit.range.end || link.source.start < edit.range.start {
                        return None;
                    }
                    let delta = edit.inserted as isize - edit.range.len() as isize;
                    for r in [&mut link.source, &mut link.label] {
                        *r = r.start.saturating_add_signed(delta)
                            ..r.end.saturating_add_signed(delta);
                    }
                }
                Some(link)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn queue_coalesces_without_cancelling_running_work() {
        let mut queue = Queue::default();
        queue.request(&Rope::from("first"));
        let first = queue.start().unwrap();
        for _ in 0..600 {
            queue.request(&Rope::from("latest"));
            assert!(queue.start().is_none());
            assert_eq!(queue.jobs(), 2);
        }
        assert!(!queue.finish(first.revision));
        let latest = queue.start().unwrap();
        assert_eq!(latest.text.to_string(), "latest");
        assert!(queue.finish(latest.revision));
        assert_eq!(queue.jobs(), 0);
    }
    #[test]
    fn mode_or_document_reset_rejects_old_results() {
        let mut queue = Queue::default();
        queue.request(&Rope::from("old"));
        let old = queue.start().unwrap();
        queue.reset();
        assert!(!queue.finish(old.revision));
        assert!(queue.start().is_none());
    }
    #[test]
    fn offset_only_edits_reuse_links_and_reject_overlaps() {
        let source = "before\n\n[café](note.md)\n";
        let mut links = Links::new(super::super::markdown_links::parse(source));
        links.edit(0..0, 2);
        let found = links.in_range(10..source.len() + 2);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].label, 11..16);
        links.edit(12..14, 1);
        assert!(links.in_range(10..source.len() + 1).is_empty());
    }
}
