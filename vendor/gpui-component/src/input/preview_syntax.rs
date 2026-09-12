//! Background-only document analysis and a bounded, non-debounced request queue.
use super::{markdown_links::MarkdownLink, table::TableIndex};
use ropey::Rope;
use std::{ops::Range, sync::Arc};

/// Large immutable indexes are released on the background executor as well.
pub(super) struct Retired {
    pub code: Vec<super::fenced_code::Index>,
    pub links: Links,
    pub tables: Vec<super::table::Table>,
}
impl Retired {
    pub fn dispose(self) {
        drop(self.code);
        drop(self.links);
        drop(self.tables);
    }
}

pub(super) struct Request {
    pub code_cache: Arc<super::fenced_code::Cache>,
    pub revision: u64,
    pub text: Rope,
}
pub(super) struct Parsed {
    pub code: Arc<super::fenced_code::Snapshot>,
    pub images: std::collections::BTreeMap<usize, String>,
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
            code: Arc::new(super::fenced_code::Snapshot::parse(
                &source,
                &self.code_cache,
            )),
            images: crate::markdown_images::paragraphs(&source),
            revision: self.revision,
            links,
            tables,
        }
    }
}

#[derive(Default)]
pub(super) struct Queue {
    pub code_cache: Arc<super::fenced_code::Cache>,
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
            code_cache: Arc::default(),
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
        self.code_cache = Arc::default();
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
        let mut request = self.pending.take()?;
        request.code_cache = self.code_cache.clone();
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
    pub fn has_items(&self) -> bool {
        !self.items.is_empty()
    }
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

/// Context-independent edits may reuse syntax outside their changed source range.
/// Inspect at most a small line neighborhood, never the whole document.
pub(super) fn context_independent(old: &Rope, range: &Range<usize>, inserted: &Rope) -> bool {
    use crate::input::RopeExt;
    if range.start == 0 && range.end == old.len() {
        return false;
    }
    let first = old.offset_to_point(range.start).row;
    let last = old.offset_to_point(range.end).row;
    // Changing only a task's state cannot change surrounding block syntax.
    // Read just its prefix and closing marker, even for a very long task line.
    let line_start = old.line_start_offset(first);
    if range.len() == 1
        && inserted.len() == 1
        && first == last
        && range.end - line_start <= 8192
        && matches!(
            old.slice(range.clone()).chars().next(),
            Some(' ' | 'x' | 'X')
        )
        && matches!(inserted.chars().next(), Some(' ' | 'x' | 'X'))
    {
        let mut prefix = old.slice(line_start..range.end).to_string();
        prefix.extend(
            old.slice(range.end..old.line_end_offset(first))
                .chars()
                .take(2),
        );
        if super::live_preview::task_state(&prefix) == Some(range.start - line_start) {
            return true;
        }
    }
    let touched = old.line_start_offset(first)..old.line_end_offset(last);
    if touched.len() <= 8192
        && inserted.len() <= 8192
        && old
            .slice(touched)
            .chars()
            .chain(inserted.chars())
            .all(char::is_whitespace)
    {
        return true;
    }
    let from = old.line_start_offset(first.saturating_sub(1));
    let to = old.line_end_offset((last + 1).min(old.lines_len() - 1));
    to - from <= 8192
        && inserted.len() <= 8192
        && old
            .slice(from..to)
            .chars()
            .chain(inserted.chars())
            .all(|c| c.is_alphanumeric() || c.is_whitespace() || matches!(c, '.' | ',' | '!' | '?'))
}

#[cfg(test)]
mod completion_tests {
    use super::*;
    use crate::input::RopeExt;
    #[test]
    fn task_state_edits_are_safe_but_marker_and_non_task_edits_are_not() {
        for prefix in ["- [", "  > > - [", "1. [", "2) [", "* [", "+ ["] {
            for before in [' ', 'x', 'X'] {
                let source = format!("你好\r\n{prefix}{before}] café {}\r\n", "a".repeat(10_000));
                let offset = "你好\r\n".len() + prefix.len();
                for after in [" ", "x", "X"] {
                    assert!(context_independent(
                        &Rope::from(source.as_str()),
                        &(offset..offset + 1),
                        &Rope::from(after),
                    ));
                }
            }
        }
        for (source, offset, replacement) in [
            ("- [ ] task", 3, "y"),
            ("- [ ] task", 3, "xx"),
            ("- [ ] task", 2, "x"),
            ("- [ ] task", 4, "x"),
            ("- [ ]task", 3, "x"),
            ("- [ ]你好", 3, "x"),
            ("[ ] label", 1, "x"),
            ("text [x]", 6, " "),
            ("- [y] task", 3, "x"),
        ] {
            assert!(
                !context_independent(
                    &Rope::from(source),
                    &(offset..offset + 1),
                    &Rope::from(replacement),
                ),
                "{source:?} at {offset}",
            );
        }
    }
    #[test]
    fn safe_results_rebase_while_latest_snapshot_is_pending() {
        let source = "before\n\n\n[label](note.md)\n";
        let mut queue = Queue::default();
        queue.request(&Rope::from(source));
        let request = queue.start().unwrap();
        let mut current = Rope::from(source);
        for _ in 0..8 {
            queue.edit(0..0, 1, true);
            current.replace(0..0, "\n");
            queue.request(&current);
        }
        let parsed = request.parse();
        let edits = queue.rebasing(parsed.revision).unwrap();
        assert!(!queue.finish(parsed.revision));
        let links = Links::with_edits(parsed.links, edits);
        assert_eq!(
            links.in_range(8..current.len()),
            super::super::markdown_links::parse(&current.to_string())
        );
        assert!(
            queue.start().is_some(),
            "pending analysis starts without a quiet-period timer"
        );
    }
}

#[cfg(test)]
mod code_queue_tests {
    use super::*;
    #[test]
    fn requests_use_current_cache_and_reset_drops_document_cache() {
        let source = Rope::from("```rs\nfn main() {}\n```");
        let mut queue = Queue::default();
        queue.request(&source);
        let first = queue.start().unwrap();
        queue.request(&source);
        let parsed = first.parse();
        assert!(!queue.finish(parsed.revision));
        queue.code_cache = parsed.code.cache.clone();
        let next = queue.start().unwrap();
        assert!(Arc::ptr_eq(&next.code_cache, &parsed.code.cache));
        queue.reset();
        assert!(queue.code_cache.is_empty());
        assert!(!queue.finish(next.revision));
    }
}
