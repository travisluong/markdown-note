//! Background fenced-code classification and bounded snapshot-based lookup.
use crate::highlighter::code::{CodeTokens, code_lines, language};
use markdown::{ParseOptions, mdast::Node};
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

#[derive(Default)]
pub(super) struct Snapshot {
    pub blocks: Vec<Range<usize>>,
    pub lines: BTreeMap<usize, CodeLine>,
    pub cache: Arc<Cache>,
}

impl Snapshot {
    pub fn parse(source: &str, previous: &Cache) -> Self {
        if !source.contains("```") && !source.contains("~~~") {
            return Self::default();
        }
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
                for mapping in code_lines(source, range, &code.value) {
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
        if let Ok(root) = markdown::to_mdast(source, &ParseOptions::gfm()) {
            visit(&root, source, previous, &mut result, &mut cache);
        }
        result.cache = Arc::new(cache);
        result
    }
}

#[derive(Default)]
pub(super) struct Index {
    pub snapshot: Arc<Snapshot>,
    edits: Vec<super::preview_syntax::Delta>,
    overflowed: bool,
}
impl Index {
    pub fn new(snapshot: Arc<Snapshot>) -> Self {
        Self {
            snapshot,
            edits: Vec::new(),
            overflowed: false,
        }
    }
    pub fn edit(&mut self, range: Range<usize>, inserted: usize) {
        if self.snapshot.blocks.is_empty() || self.overflowed {
            return;
        }
        if self.edits.len() == 64 {
            // Bound lookup work during long bursts; fresh analysis restores it.
            self.overflowed = true;
            self.edits.clear();
        } else {
            self.edits
                .push(super::preview_syntax::Delta { range, inserted });
        }
    }
    pub fn line(&self, start: usize) -> Option<CodeLine> {
        if self.overflowed {
            return None;
        }
        let mut original = start;
        for edit in self.edits.iter().rev() {
            let end = edit.range.start + edit.inserted;
            if original >= end {
                original = original
                    .saturating_add_signed(edit.range.len() as isize - edit.inserted as isize);
            } else if original >= edit.range.start {
                original = edit.range.start;
            }
        }
        let at = self.snapshot.blocks.partition_point(|r| r.end <= original);
        let block = self.snapshot.blocks.get(at)?;
        if !block.contains(&original) {
            return None;
        }
        // Never paint classifications from an older document revision. Retain
        // literal source presentation while the current request is running.
        if !self.edits.is_empty() {
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
        assert!(index.line(row + 105).is_none());
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
