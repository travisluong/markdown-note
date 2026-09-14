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
    pub dividers: Arc<Vec<Range<usize>>>,
    pub list_items: Arc<Vec<super::list_reorder::Item>>,
    pub inline_code: Arc<Vec<super::inline_code::CodeSpan>>,
    pub code: Arc<super::fenced_code::Snapshot>,
    pub images: std::collections::BTreeMap<usize, String>,
    pub revision: u64,
    pub links: Vec<MarkdownLink>,
    pub tables: TableIndex,
}
/// Actual full-document GFM work for one request, separate from index byte accounting.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PreviewParseWork {
    pub gfm_calls: usize,
    pub gfm_bytes: usize,
}

impl Request {
    pub fn parse(self) -> Parsed {
        self.parse_with_work().0
    }

    fn parse_with_work(self) -> (Parsed, PreviewParseWork) {
        let source = self.text.to_string();
        let links_needed = super::markdown_links::requires_parse(&source);
        let tables_needed = TableIndex::requires_parse(&source);
        let code_needed = super::fenced_code::Snapshot::requires_parse(&source);
        let images_needed = crate::markdown_images::requires_parse(&source);
        let mut work = PreviewParseWork::default();
        let root = if links_needed
            || tables_needed
            || code_needed
            || images_needed
            || source.contains('`')
            || source
                .lines()
                .any(|line| line.contains(['-', '*', '+', '_']))
            || source.lines().any(super::list_edit::numeric_marker)
        {
            work.gfm_calls += 1;
            work.gfm_bytes += source.len();
            markdown::to_mdast(&source, &markdown::ParseOptions::gfm()).ok()
        } else {
            None
        };
        let links = root
            .as_ref()
            .filter(|_| links_needed)
            .map(|root| super::markdown_links::from_ast(&source, root).links)
            .unwrap_or_default();
        let code = root
            .as_ref()
            .filter(|_| code_needed)
            .map(|root| super::fenced_code::Snapshot::from_ast(&source, &self.code_cache, root))
            .unwrap_or_default();
        let images = root
            .as_ref()
            .filter(|_| images_needed)
            .map(crate::markdown_images::paragraphs_from_ast)
            .unwrap_or_default();
        let mut tables = TableIndex::default();
        tables.rebuild_from_ast(&source, root.as_ref());
        // The tree stays on this worker and is dropped before returning derived indexes.
        (
            Parsed {
                dividers: Arc::new(
                    root.as_ref()
                        .map(|root| thematic_breaks(&source, root))
                        .unwrap_or_default(),
                ),
                list_items: Arc::new(
                    root.as_ref()
                        .map(|root| super::list_reorder::from_ast(&source, root))
                        .unwrap_or_default(),
                ),
                inline_code: Arc::new(
                    root.as_ref()
                        .map(|root| super::inline_code::from_ast(&source, root))
                        .unwrap_or_default(),
                ),
                code: Arc::new(code),
                images,
                revision: self.revision,
                links,
                tables,
            },
            work,
        )
    }
}

/// Reuse the worker's GFM tree so setext headings, tables and code stay literal.
fn thematic_breaks(source: &str, root: &markdown::mdast::Node) -> Vec<Range<usize>> {
    fn visit(source: &str, node: &markdown::mdast::Node, ranges: &mut Vec<Range<usize>>) {
        if let markdown::mdast::Node::ThematicBreak(rule) = node {
            if let Some(position) = &rule.position {
                let mut range = position.start.offset..position.end.offset;
                range.start += source[range.clone()].find(['-', '*', '_']).unwrap_or(0);
                ranges.push(range);
            }
        } else if let Some(children) = node.children() {
            for child in children {
                visit(source, child, ranges);
            }
        }
    }
    let mut ranges = Vec::new();
    visit(source, root, &mut ranges);
    ranges
}

/// Exercise the production request parser without starting a native editor or window.
#[cfg(feature = "editor-diagnostics")]
#[doc(hidden)]
pub fn diagnostic_preview_parse_work(source: &str) -> PreviewParseWork {
    Request {
        code_cache: Arc::default(),
        revision: 0,
        text: Rope::from(source),
    }
    .parse_with_work()
    .1
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
        // Bound query work without dropping rendered links during long input bursts.
        // Compact offsets only; this never copies or parses document source.
        if self.edits.len() == 64 {
            *self = Self::with_edits(self.in_range(0..usize::MAX), Vec::new());
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
            // A viewport/line query may cross an edit even when its links do not.
            // Widen the inverse range across replaced bytes, then reject only
            // individual links that overlap the edit below.
            range.start = if range.start >= end {
                range.start.saturating_add_signed(-delta)
            } else {
                range.start.min(edit.range.start)
            };
            range.end = if range.end >= end {
                range.end.saturating_add_signed(-delta)
            } else {
                edit.range.end
            };
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
    fn link_queries_cross_edits_and_survive_long_bursts() {
        let mut source = "prefix [café](one.md) middle [你好](two.md) suffix".to_string();
        let mut links = Links::new(super::super::markdown_links::parse(&source));
        for i in 0..150 {
            let at = if i % 2 == 0 {
                0
            } else {
                source.find(" middle").unwrap()
            };
            source.insert(at, 'x');
            links.edit(at..at, 1);
            assert_eq!(
                links.in_range(0..source.len()),
                super::super::markdown_links::parse(&source)
            );
        }
        let at = source.find("café").unwrap();
        source.replace_range(at..at + "café".len(), "changed");
        links.edit(at..at + "café".len(), "changed".len());
        let remaining = links.in_range(0..source.len());
        assert_eq!(remaining.len(), 1);
        assert_eq!(&source[remaining[0].label.clone()], "你好");
    }
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

#[cfg(test)]
mod shared_analysis_tests {
    use super::*;
    use crate::input::RopeExt;

    const MIXED: &str = "# café\n\n[**你好**][id] and <https://example.org>.\n\n[id]: notes/café.md \"Title\"\n\n| left | right |\n| :--- | ---: |\n| [cell][id] | `a\\|b` |\n\n![café](images/café.png)\n\n> ```json\n> {\"label\": \"你好\"}\n> ```\n\n```rust\nfn main() {}\n```\n";

    fn request(source: &str, cache: Arc<super::super::fenced_code::Cache>) -> Request {
        Request {
            code_cache: cache,
            revision: 42,
            text: Rope::from(source),
        }
    }

    #[test]
    fn thematic_breaks_use_gfm_context_and_one_shared_parse() {
        for (source, marker) in [
            ("---", "---"),
            ("***", "***"),
            ("___", "___"),
            ("  - - -  ", "- - -"),
            ("café 你好\r\n\r\n> > ***\r\n", "***"),
            ("- item\n\n  ---\n", "---"),
            ("- ***\n", "***"),
        ] {
            let (parsed, work) = request(source, Arc::default()).parse_with_work();
            assert_eq!(parsed.dividers.len(), 1, "{source:?}");
            assert_eq!(source[parsed.dividers[0].clone()].trim(), marker);
            assert_eq!(work.gfm_calls, 1);
            assert_eq!(work.gfm_bytes, source.len());
        }
        for source in [
            "heading\n---",
            "inline --- dashes",
            "--",
            "\\---",
            "- * -",
            "```\n---\n```",
            "~~~\n***\n~~~",
            "    ___",
            "| a | b |\n| --- | --- |\n| c | d |",
            "`---`",
        ] {
            let parsed = request(source, Arc::default()).parse();
            assert!(parsed.dividers.is_empty(), "{source:?}");
        }
    }

    fn assert_tables(actual: &TableIndex, expected: &TableIndex) {
        assert_eq!(actual.tables.len(), expected.tables.len());
        for (actual, expected) in actual.tables.iter().zip(&expected.tables) {
            assert_eq!(actual.source, expected.source);
            assert_eq!(actual.separator, expected.separator);
            assert_eq!(actual.alignments, expected.alignments);
            assert_eq!(actual.column_characters, expected.column_characters);
            assert_eq!(actual.cache_key(), expected.cache_key());
            assert_eq!(actual.rows.len(), expected.rows.len());
            for (row, (actual_row, expected_row)) in
                actual.rows.iter().zip(expected.rows.iter()).enumerate()
            {
                assert_eq!(actual.row_range(row), expected.row_range(row));
                assert_eq!(actual_row.present_cells, expected_row.present_cells);
                assert_eq!(actual_row.cells.len(), expected_row.cells.len());
                for column in 0..actual_row.cells.len() {
                    assert_eq!(
                        actual.cell_range(row, column),
                        expected.cell_range(row, column)
                    );
                }
            }
        }
    }

    fn assert_code(
        actual: &super::super::fenced_code::Snapshot,
        expected: &super::super::fenced_code::Snapshot,
    ) {
        assert_eq!(actual.blocks, expected.blocks);
        assert_eq!(actual.details.len(), expected.details.len());
        for (actual, expected) in actual.details.iter().zip(&expected.details) {
            assert_eq!(actual.source, expected.source);
            assert_eq!(actual.opening, expected.opening);
            assert_eq!(actual.content, expected.content);
            assert_eq!(actual.closing, expected.closing);
            assert_eq!(actual.first, expected.first);
            assert_eq!(actual.last, expected.last);
        }
        assert_eq!(actual.lines.len(), expected.lines.len());
        for (offset, actual) in &actual.lines {
            let expected = &expected.lines[offset];
            assert_eq!(actual.prefix, expected.prefix);
            assert_eq!(actual.tokens, expected.tokens);
        }
        assert_eq!(*actual.cache, *expected.cache);
    }

    #[test]
    fn mixed_requests_match_standalone_indexes_and_byte_accounting() {
        let crlf = MIXED.replace('\n', "\r\n");
        for source in [
            "",
            "ordinary café 你好\r\nnext paragraph",
            MIXED,
            crlf.as_str(),
            "[first][id] [ID][] [id]\n\n[id]: a.md \"first\"\n[id]: b.md\n\n> [nested][inner]\n>\n> [inner]: nested.md\n\n[outer][inner]\n\n![reference][img]\n\n[img]: image.png\n",
            "> | a | b |\n> | - | - |\n> | [nested](a.md) | b |\n>\n> ![nested](a.png)\n\n- ```json\n  {\"key\": \"café\"}\n  ```\n",
            "[broken]( ![broken](\n\n| a | b |\n| --- | --- |\n| short |\n| excess | cells | extra |\n\n~~~rust\nfn main() {\n[code][id]\n![code](x.png)\n",
            "`inline ``` marker` and escaped \\[link](a.md)\n\n    indented | [code](a.md)\n\nhttps://example.org a@example.org\n",
            "| ordinary pipe\n\ntext\n",
            "[link](a.md)\n\nparagraph without table markers\n",
        ] {
            let cache = Arc::default();
            let (actual, work) = request(source, Arc::clone(&cache)).parse_with_work();
            let mut tables = TableIndex::default();
            tables.rebuild_source(source);
            assert_tables(&actual.tables, &tables);
            assert_eq!(actual.tables.revision, tables.revision);
            assert_eq!(actual.tables.parsed_bytes, source.len() as u64);
            assert_eq!(
                actual.links,
                super::super::markdown_links::parse(source),
                "{source:?}"
            );
            assert_eq!(
                actual.images,
                crate::markdown_images::paragraphs(source),
                "{source:?}"
            );
            assert_code(
                &actual.code,
                &super::super::fenced_code::Snapshot::parse(source, &cache),
            );
            assert_eq!(actual.revision, 42);
            let calls =
                usize::from(!source.is_empty() && source != "ordinary café 你好\r\nnext paragraph");
            assert_eq!(work.gfm_calls, calls, "{source:?}");
            assert_eq!(work.gfm_bytes, calls * source.len(), "{source:?}");
        }
        let (mixed, work) = request(MIXED, Arc::default()).parse_with_work();
        assert_eq!(mixed.links.len(), 3);
        assert_eq!(mixed.links[0].destination, "notes/café.md");
        assert_eq!(mixed.links[0].title.as_deref(), Some("Title"));
        assert_eq!(&MIXED[mixed.links[0].label.clone()], "**你好**");
        assert_eq!(mixed.tables.tables.len(), 1);
        assert_eq!(mixed.images.len(), 1);
        assert_eq!(mixed.code.blocks.len(), 2);
        assert_eq!(
            work,
            PreviewParseWork {
                gfm_calls: 1,
                gfm_bytes: MIXED.len()
            }
        );
    }

    #[test]
    fn shared_tables_keep_incremental_block_context_and_fragment_bases() {
        let source = "intro\n\n| first | other |\n| --- | --- |\n| one | two |\n\nbetween\n\n| café | 你好 |\n| :--- | ---: |\n| three | four |\n\ntail\n";
        let mut text = Rope::from(source);
        let mut tables = request(source, Arc::default()).parse().tables;
        let mut expected = TableIndex::default();
        expected.rebuild_source(source);
        for (needle, replacement) in [
            (":---", "---:"),
            ("between", "middle\n\nparagraph"),
            ("---:", "---"),
            ("intro", "préface\r\nintro"),
        ] {
            let old = text.clone();
            let source = old.to_string();
            let at = source.find(needle).unwrap();
            let range = at..at + needle.len();
            text.replace(range.clone(), replacement);
            tables.update(&old, &text, &range, &Rope::from(replacement));
            expected.update(&old, &text, &range, &Rope::from(replacement));
            assert_tables(&tables, &expected);
            let mut rebuilt = TableIndex::default();
            rebuilt.rebuild(&text);
            assert_eq!(
                format!("{:?}", tables.tables),
                format!("{:?}", rebuilt.tables)
            );
        }
    }

    #[test]
    fn shared_code_reuses_unchanged_cache_entries_after_source_moves() {
        let first = request(MIXED, Arc::default()).parse();
        assert!(!first.code.cache.is_empty());
        let moved = format!("préface 你好\r\n\r\n{MIXED}{MIXED}");
        let (next, work) = request(&moved, first.code.cache.clone()).parse_with_work();
        assert_eq!(next.code.blocks.len(), first.code.blocks.len() * 2);
        assert_eq!(next.code.cache.len(), first.code.cache.len());
        for (key, tokens) in first.code.cache.iter() {
            assert!(Arc::ptr_eq(tokens, &next.code.cache[key]));
        }
        assert_eq!(work.gfm_calls, 1);
        assert_eq!(work.gfm_bytes, moved.len());
    }
}
