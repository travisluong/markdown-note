//! Background fenced-code classification and bounded snapshot-based lookup.
use crate::highlighter::code::{CodeTokens, code_lines, language};
use markdown::mdast::Node;
use std::{
    collections::{BTreeMap, HashMap},
    ops::Range,
    sync::Arc,
};

pub(super) type Cache = HashMap<(String, String), Arc<CodeTokens>>;

#[derive(Clone, Debug, Default)]
pub(super) struct CodeLine {
    pub prefix: usize,
    pub tokens: CodeTokens,
}

#[derive(Clone, Debug)]
pub(super) struct Block {
    #[cfg(feature = "mermaid")]
    pub mermaid: bool,
    pub code: gpui::SharedString,
    pub source: Range<usize>,
    pub opening: Range<usize>,
    pub content: Range<usize>,
    pub closing: Option<Range<usize>>,
    pub first: usize,
    pub last: usize,
}
impl Block {
    pub fn active(&self, selection: &Range<usize>) -> bool {
        if selection.is_empty() {
            self.source.contains(&selection.start) || selection.start == self.source.end
        } else {
            selection.start < self.source.end && selection.end > self.source.start
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Decoration {
    /// Suppress fence glyphs only; source mapping and geometry stay intact.
    pub hidden: bool,
    pub first: bool,
    pub last: bool,
    pub padding: f32,
}
impl Decoration {
    pub fn top(&self) -> f32 {
        if self.first { self.padding } else { 0. }
    }
    pub fn bottom(&self) -> f32 {
        if self.last { self.padding } else { 0. }
    }
}

#[derive(Default)]
pub(super) struct Snapshot {
    pub blocks: Vec<Range<usize>>,
    pub details: Vec<Block>,
    pub lines: BTreeMap<usize, CodeLine>,
    pub cache: Arc<Cache>,
}

impl Snapshot {
    pub fn requires_parse(source: &str) -> bool {
        source.contains("```") || source.contains("~~~")
    }

    #[cfg(test)]
    pub fn parse(source: &str, previous: &Cache) -> Self {
        if !Self::requires_parse(source) {
            return Self::default();
        }
        let Ok(root) = markdown::to_mdast(source, &markdown::ParseOptions::gfm()) else {
            return Self::default();
        };
        Self::from_ast(source, previous, &root)
    }

    /// Classify fences and reuse tokens while borrowing the worker's GFM tree.
    pub fn from_ast(source: &str, previous: &Cache, root: &Node) -> Self {
        fn visit(
            node: &Node,
            source: &str,
            previous: &Cache,
            result: &mut Snapshot,
            cache: &mut Cache,
        ) {
            if let Node::Code(code) = node {
                let Some(pos) = &code.position else { return };
                let range = pos.start.offset..pos.end.offset;
                let Some(raw) = source.get(range.clone()) else {
                    return;
                };
                if !raw.starts_with("```") && !raw.starts_with("~~~") {
                    return;
                }
                let start = source[..range.start].rfind('\n').map_or(0, |n| n + 1);
                result.blocks.push(start..range.end);
                let opening_end = source[range.clone()]
                    .find('\n')
                    .map_or(range.end, |n| range.start + n + 1);
                let last_start = source[..range.end].rfind('\n').map_or(start, |n| n + 1);
                let mappings = code_lines(source, range.clone(), &code.value);
                let marker = raw.as_bytes()[0];
                let count = raw.bytes().take_while(|b| *b == marker).count();
                let tail = source[last_start..range.end]
                    .trim_start_matches(|c: char| c.is_whitespace() || c == '>');
                let fence_len = tail.bytes().take_while(|b| *b == marker).count();
                let closed = last_start >= opening_end
                    && fence_len >= count
                    && tail[fence_len..].trim().is_empty()
                    && (code.value.is_empty()
                        || mappings.last().is_some_and(|m| m.line_start < last_start));
                let closing = closed.then_some(last_start..range.end);
                // Fence prefixes use the same quote/list inset as content, even
                // when only the fence glyphs are hidden.
                result.lines.insert(
                    start,
                    CodeLine {
                        prefix: range.start - start,
                        tokens: CodeTokens::default(),
                    },
                );
                if closed {
                    result.lines.insert(
                        last_start,
                        CodeLine {
                            prefix: source[last_start..range.end]
                                .find(marker as char)
                                .unwrap_or(0),
                            tokens: CodeTokens::default(),
                        },
                    );
                }
                let content_end = closing.as_ref().map_or(range.end, |r| r.start);
                let empty = opening_end >= content_end;
                let first = if empty { last_start } else { opening_end };
                let last = if empty {
                    first
                } else {
                    let end = content_end
                        - usize::from(source.as_bytes().get(content_end - 1) == Some(&b'\n'));
                    source[..end]
                        .rfind('\n')
                        .map_or(first, |n| n + 1)
                        .max(first)
                };
                result.details.push(Block {
                    #[cfg(feature = "mermaid")]
                    mermaid: crate::mermaid::language(code.lang.as_deref()),
                    code: code.value.clone().into(),
                    source: start..range.end,
                    opening: start..opening_end,
                    content: opening_end..content_end,
                    closing,
                    first,
                    last,
                });
                let tokens = if let Some(lang) = language(code.lang.as_deref()) {
                    let key = (lang.to_string(), code.value.clone());
                    cache
                        .entry(key.clone())
                        .or_insert_with(|| {
                            previous.get(&key).cloned().unwrap_or_else(|| {
                                Arc::new(CodeTokens::new(&code.value, Some(&lang)))
                            })
                        })
                        .clone()
                } else {
                    Arc::new(CodeTokens::default())
                };
                for mapping in mappings {
                    let mut spans = Vec::new();
                    let first = tokens
                        .spans
                        .partition_point(|(r, _)| r.end <= mapping.display.start);
                    for (span, names) in tokens.spans[first..]
                        .iter()
                        .take_while(|(r, _)| r.start < mapping.display.end)
                    {
                        let a = span.start.max(mapping.display.start) - mapping.display.start;
                        let b = span.end.min(mapping.display.end) - mapping.display.start;
                        let prefix = mapping.source.start - mapping.line_start;
                        let range = if mapping.source.len() == mapping.display.len() {
                            prefix + a..prefix + b
                        } else {
                            mapping.source.start - mapping.line_start
                                ..mapping.source.end - mapping.line_start
                        };
                        spans.push((range, names.clone()));
                    }
                    let line = result
                        .lines
                        .entry(mapping.line_start)
                        .or_insert_with(|| CodeLine {
                            prefix: mapping.source.start - mapping.line_start,
                            tokens: CodeTokens::default(),
                        });
                    line.tokens.spans.extend(spans);
                }
                return;
            }
            if let Some(children) = node.children() {
                for child in children {
                    visit(child, source, previous, result, cache);
                }
            }
        }
        let mut result = Self::default();
        let mut cache = Cache::new();
        visit(root, source, previous, &mut result, &mut cache);
        result.cache = Arc::new(cache);
        result
    }
}

struct Edit {
    delta: super::preview_syntax::Delta,
    affected: Range<usize>,
    independent: bool,
}

#[derive(Default)]
pub(super) struct Index {
    pub snapshot: Arc<Snapshot>,
    edits: Vec<Edit>,
    overflowed: bool,
    unchanged_prefix: usize,
}
impl Index {
    pub fn new(snapshot: Arc<Snapshot>) -> Self {
        Self {
            snapshot,
            edits: Vec::new(),
            overflowed: false,
            unchanged_prefix: usize::MAX,
        }
    }
    #[cfg(test)]
    pub fn edit(&mut self, range: Range<usize>, inserted: usize) {
        self.edit_context(range.clone(), inserted, range, false);
    }

    pub fn edit_context(
        &mut self,
        range: Range<usize>,
        inserted: usize,
        affected: Range<usize>,
        independent: bool,
    ) {
        if self.snapshot.blocks.is_empty() {
            return;
        }
        self.unchanged_prefix = self
            .unchanged_prefix
            .min(self.original_offset(affected.start));
        if self.overflowed {
            return;
        }
        // A typing burst (including backspace/undo inside the insertion) is one
        // translation, rather than exhausting the lookup budget on every key.
        if let Some(last) = self.edits.last_mut() {
            let end = last.delta.range.start + last.delta.inserted;
            if range.start >= last.delta.range.start && range.end <= end {
                last.delta.inserted = last.delta.inserted - range.len() + inserted;
                last.independent &= independent;
                return;
            }
        }
        if self.edits.len() == 64 {
            self.overflowed = true;
            self.edits.clear();
        } else {
            self.edits.push(Edit {
                delta: super::preview_syntax::Delta { range, inserted },
                affected,
                independent,
            });
        }
    }

    fn original_offset(&self, mut offset: usize) -> usize {
        for edit in self.edits.iter().rev().map(|e| &e.delta) {
            let end = edit.range.start + edit.inserted;
            if offset >= end {
                offset = offset
                    .saturating_add_signed(edit.range.len() as isize - edit.inserted as isize);
            } else if offset >= edit.range.start {
                offset = edit.range.start;
            }
        }
        offset
    }
    pub fn active_blocks(&self, selection: &Range<usize>) -> Range<usize> {
        let selection = self.original_offset(selection.start)..self.original_offset(selection.end);
        let blocks = &self.snapshot.details;
        let start = blocks.partition_point(|b| {
            if selection.is_empty() {
                b.source.end < selection.start
            } else {
                b.source.end <= selection.start
            }
        });
        let end = blocks.partition_point(|b| {
            if selection.is_empty() {
                b.source.start <= selection.end
            } else {
                b.source.start < selection.end
            }
        });
        start..end
    }
    pub fn block(&self, offset: usize) -> Option<Block> {
        if self.overflowed && offset >= self.unchanged_prefix {
            return None;
        }
        let original = self.original_offset(offset);
        let at = self
            .snapshot
            .details
            .partition_point(|b| b.source.end < original);
        let mut block = self.snapshot.details.get(at)?.clone();
        if self.overflowed && block.source.end >= self.unchanged_prefix {
            return None;
        }
        if original < block.source.start || original > block.source.end {
            return None;
        }
        for edit in &self.edits {
            // Structural changes can affect following blocks, but cannot change
            // a completed block earlier in the document. Ordinary prose edits
            // simply translate untouched blocks and their cached code tokens.
            if edit.affected.start <= block.source.end
                && (!edit.independent || edit.affected.end >= block.source.start)
            {
                return None;
            }
            if edit.delta.range.end <= block.source.start {
                let delta = edit.delta.inserted as isize - edit.delta.range.len() as isize;
                let shift = |r: &mut Range<usize>| {
                    r.start = r.start.saturating_add_signed(delta);
                    r.end = r.end.saturating_add_signed(delta);
                };
                shift(&mut block.source);
                shift(&mut block.opening);
                shift(&mut block.content);
                if let Some(closing) = &mut block.closing {
                    shift(closing);
                }
                block.first = block.first.saturating_add_signed(delta);
                block.last = block.last.saturating_add_signed(delta);
            }
        }
        Some(block)
    }
    pub fn decoration(
        &self,
        start: usize,
        selection: &Range<usize>,
        padding: f32,
    ) -> Option<Decoration> {
        let block = self.block(start)?;
        Some(Decoration {
            hidden: (block.opening.contains(&start)
                || block.closing.as_ref().is_some_and(|r| r.contains(&start)))
                && !block.active(selection),
            first: start == block.opening.start,
            last: start == block.closing.as_ref().map_or(block.last, |r| r.start),
            padding,
        })
    }
    pub fn line(&self, start: usize) -> Option<CodeLine> {
        if self.overflowed && start >= self.unchanged_prefix {
            return None;
        }
        let original = self.original_offset(start);
        let at = self.snapshot.blocks.partition_point(|r| r.end <= original);
        let block = self.snapshot.blocks.get(at)?;
        if !block.contains(&original) {
            return None;
        }
        // Only unchanged, safely translated blocks may reuse source mappings
        // and tokens. Uncertain blocks stay literal until fresh analysis.
        if self.block(start).is_none() {
            return Some(CodeLine::default());
        }
        Some(
            self.snapshot
                .lines
                .get(&original)
                .cloned()
                .unwrap_or_default(),
        )
    }
}
#[cfg(test)]
mod tests {
    use super::super::live_preview::Presentation;
    use super::*;
    use crate::highlighter::HighlightTheme;

    #[test]
    fn fenced_tokens_map_nested_unicode_crlf_and_unfinished_blocks() {
        for source in [
            "```RS extra\r\n/* café\r\n你好 */\r\nfn main() {}\r\n```\r\n",
            "> ```rs\n> /* café\n> 你好 */\n> fn main() {}\n> ```\n",
            "- item\n\n  ~~~rust\n  /* café\n  你好 */\n  fn main() {}\n  ~~~\n",
            "```rust\n/* café\n你好 */\nfn main() {}",
            "  ```rust\n  /* café\n  你好 */\n  fn main() {}\n  ```",
            "- item\n\n  ```rust\n\tfn main() {}\n\t/* café\n\t你好 */\n  ```",
        ] {
            let parsed = Snapshot::parse(source, &Cache::new());
            assert_eq!(parsed.blocks.len(), 1, "{source}");
            let index = Index::new(Arc::new(parsed));
            for (needle, capture) in [("你好", "comment"), ("fn", "keyword")] {
                let at = source.find(needle).unwrap();
                let row = source[..at].rfind('\n').map_or(0, |n| n + 1);
                let line = index.line(row).unwrap();
                let theme = HighlightTheme::default_light();
                let styles = line
                    .tokens
                    .styles(at - row..at - row + needle.len(), &theme);
                assert!(
                    styles
                        .iter()
                        .any(|(_, s)| s.color == theme.style(capture).unwrap().color),
                    "{source}: {needle}: {styles:?}"
                );
            }
        }
    }

    #[test]
    fn code_stays_literal_in_active_and_inactive_lines() {
        for tag in ["rust", "", "unknown"] {
            let source =
                format!("> ```{tag}\n> # **bold** [link](url) - [ ] &amp; > literal\n> ```");
            let snapshot = Snapshot::parse(&source, &Cache::new());
            let row = source.find("\n").unwrap() + 1;
            let line = source[row..].split('\n').next().unwrap();
            let code = Index::new(Arc::new(snapshot)).line(row).unwrap();
            let p = Presentation::literal(line, code.prefix, code.tokens);
            assert!(p.task.is_none() && p.links.is_empty() && p.spans.is_empty());
            assert_eq!(p.heading, 0);
            assert_eq!(p.display(line, false).text, &line[2..]);
            assert_eq!(p.display(line, true).text, line);
            for reveal in [false, true] {
                let mapped = p.display(line, reveal);
                for (at, _) in mapped.text.char_indices() {
                    let source_at = mapped.display_to_source[at];
                    assert_eq!(mapped.source_to_display[source_at], at);
                }
            }
        }
        let source = "    fn main() {}\n\n`fn main()`\n";
        assert!(Snapshot::parse(source, &Cache::new()).blocks.is_empty());
    }

    #[test]
    fn cache_reuses_unchanged_blocks_and_retires_removed_content() {
        let source = "```rs\nfn main() {}\n```\n";
        let first = Snapshot::parse(source, &Cache::new());
        let old = first.cache.values().next().unwrap();
        let moved = Snapshot::parse(&format!("paragraph\n\n{source}{source}"), &first.cache);
        assert_eq!(moved.cache.len(), 1);
        assert!(Arc::ptr_eq(old, moved.cache.values().next().unwrap()));
        let changed = Snapshot::parse(&source.replace("main", "other"), &moved.cache);
        assert_eq!(changed.cache.len(), 1);
        assert!(!Arc::ptr_eq(old, changed.cache.values().next().unwrap()));
        let language = Snapshot::parse(&source.replace("rs", "py"), &changed.cache);
        assert_eq!(language.cache.keys().next().unwrap().0, "python");
        assert!(Snapshot::parse("no code", &language.cache).cache.is_empty());
    }

    #[test]
    fn edits_clear_old_colors_but_keep_literal_source_until_analysis() {
        let source = "text\n\n```rs\nfn main() {}\n```\n";
        let mut index = Index::new(Arc::new(Snapshot::parse(source, &Cache::new())));
        let row = source.find("fn").unwrap();
        assert!(!index.line(row).unwrap().tokens.spans.is_empty());
        index.edit(0..0, 5);
        assert!(index.line(row + 5).unwrap().tokens.spans.is_empty());
        index.edit(row + 8..row + 8, 3);
        assert!(index.line(row + 5).unwrap().tokens.spans.is_empty());
        assert!(index.line(0).is_none());
        let snapshot = index.snapshot.clone();
        for _ in 0..100 {
            index.edit(0..0, 1);
        }
        assert!(index.line(row + 105).unwrap().tokens.spans.is_empty());
        assert!(
            Arc::ptr_eq(&index.snapshot, &snapshot),
            "large snapshots retire on the background executor"
        );
    }

    #[test]
    fn wrapped_ranges_match_whole_block_colors_in_both_themes() {
        let source = "```js\nconst café = \"**hello**\"; // comment\n```";
        let snapshot = Snapshot::parse(source, &Cache::new());
        let row = source.find("const").unwrap();
        let line = &snapshot.lines[&row];
        let code = "const café = \"**hello**\"; // comment";
        let tokens = CodeTokens::new(code, Some("js"));
        for theme in [
            HighlightTheme::default_light(),
            HighlightTheme::default_dark(),
        ] {
            for (a, ch) in code.char_indices() {
                let range = a..a + ch.len_utf8();
                assert_eq!(
                    line.tokens.styles(range.clone(), &theme),
                    tokens.styles(range, &theme)
                );
            }
        }
    }
}

#[cfg(test)]
mod block_tests {
    use super::*;
    #[test]
    fn fences_content_and_selection_are_block_scoped() {
        for source in [
            "before\n```rust\n你好\n\n```\nafter",
            "before\r\n~~~~ rust\r\n你好\r\n\r\n~~~~\r\nafter",
            "before\n> ```rust\n> 你好\n> \n> ```\nafter",
            "before\n- item\n\n  ```rust\n  你好\n  \n  ```\nafter",
        ] {
            let index = Index::new(Arc::new(Snapshot::parse(source, &Cache::new())));
            let block = &index.snapshot.details[0];
            assert!(block.closing.is_some(), "{source}");
            assert!(
                index
                    .decoration(block.opening.start, &(0..0), 0.6)
                    .unwrap()
                    .hidden
            );
            assert!(
                index
                    .decoration(block.closing.as_ref().unwrap().start, &(0..0), 0.6)
                    .unwrap()
                    .hidden
            );
            assert!(
                index
                    .decoration(block.opening.start, &(0..0), 0.6)
                    .unwrap()
                    .first
            );
            assert!(
                index
                    .decoration(block.closing.as_ref().unwrap().start, &(0..0), 0.6)
                    .unwrap()
                    .last
            );
            for caret in [
                block.opening.start,
                source.find("你好").unwrap(),
                block.source.end,
            ] {
                assert!(
                    index
                        .decoration(block.opening.start, &(caret..caret), 0.6)
                        .is_some_and(|code| !code.hidden)
                );
                assert!(
                    index
                        .decoration(block.last, &(caret..caret), 0.6)
                        .is_some_and(|code| !code.hidden)
                );
            }
            assert!(!block.active(&(0..block.source.start)));
            assert!(block.active(&(0..block.content.end)));
            assert_eq!(index.active_blocks(&(0..source.len())), 0..1);
        }
    }
    #[test]
    fn empty_and_unfinished_blocks_have_a_clickable_row() {
        for source in [
            "before\n```\n```",
            "before\n~~~",
            "before\n```\n",
            "before\n```rs\ncode",
        ] {
            let index = Index::new(Arc::new(Snapshot::parse(source, &Cache::new())));
            let b = &index.snapshot.details[0];
            let first = index.decoration(b.opening.start, &(0..0), 0.6).unwrap();
            assert!(first.padding > 0., "{source}: {b:?}");
            assert!(first.first);
            assert!(index.decoration(b.last, &(0..0), 0.6).unwrap().last);
        }
    }
    #[test]
    fn fence_like_content_in_unclosed_blocks_stays_visible() {
        for source in [
            "before\n```\n> ```",
            "before\n```\n    ```",
            "before\n````\n```",
            "before\n```\n~~~",
        ] {
            let index = Index::new(Arc::new(Snapshot::parse(source, &Cache::new())));
            let b = &index.snapshot.details[0];
            assert!(b.closing.is_none(), "{source}");
            assert!(!index.decoration(b.last, &(0..0), 0.6).unwrap().hidden);
        }
    }

    #[test]
    fn translated_blocks_keep_tokens_and_active_source_coordinates() {
        let source = "before\n```js\nconst café = 1;\n```\nafter";
        let mut index = Index::new(Arc::new(Snapshot::parse(source, &Cache::new())));
        let code = source.find("const").unwrap();
        let colors = index.line(code).unwrap().tokens;
        index.edit_context(1..1, 2, 0..6, true);
        let b = index.block(code + 2).unwrap();
        assert_eq!(b.source.start, 9);
        assert_eq!(index.line(code + 2).unwrap().tokens, colors);
        assert!(index.decoration(9, &(0..0), 0.6).unwrap().hidden);
        assert!(
            index
                .decoration(9, &(code + 2..code + 2), 0.6)
                .is_some_and(|code| !code.hidden)
        );
        // Editing the closing fence cannot retain its old hidden geometry.
        let closing = b.closing.unwrap();
        index.edit_context(closing.start..closing.start + 1, 0, closing, false);
        assert!(index.block(code + 2).is_none());
        assert!(index.line(code + 2).unwrap().tokens.spans.is_empty());
    }

    #[test]
    fn exhausted_mapping_log_keeps_the_unchanged_prefix_rendered() {
        let source = "```js\nconst x = 1;\n```\nafter";
        let mut index = Index::new(Arc::new(Snapshot::parse(source, &Cache::new())));
        // Disjoint structural edits cannot coalesce. The bounded fallback must
        // still retain blocks preceding every edit, even after the 64th edit.
        for i in 0..100 {
            let at = source.len() + i * 3;
            index.edit_context(at..at, 1, at..at, false);
        }
        assert!(index.overflowed);
        assert!(
            index
                .decoration(0, &(source.len()..source.len()), 0.6)
                .unwrap()
                .hidden
        );
        assert!(!index.line(6).unwrap().tokens.spans.is_empty());

        let mut changed = Index::new(Arc::new(Snapshot::parse(source, &Cache::new())));
        changed.edit_context(10..11, 1, 6..18, false);
        for i in 0..100 {
            let at = source.len() + i * 3;
            changed.edit_context(at..at, 1, at..at, false);
        }
        assert!(changed.overflowed);
        assert!(
            changed.block(0).is_none(),
            "overflow must not restore a partly edited block"
        );
    }

    #[test]
    fn stale_analysis_never_hides_fences() {
        let mut index = Index::new(Arc::new(Snapshot::parse(
            "before\n```\ncode\n```",
            &Cache::new(),
        )));
        assert!(index.decoration(7, &(0..0), 0.6).is_some());
        index.edit(0..0, 1);
        assert!(index.decoration(8, &(0..0), 0.6).is_none());
    }
}
