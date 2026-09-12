//! Shared, theme-independent fenced-code highlighting.
use super::{HighlightTheme, LanguageRegistry, SyntaxHighlighter};
use gpui::{HighlightStyle, SharedString};
use ropey::Rope;
use std::{
    collections::{BTreeMap, BTreeSet},
    ops::Range,
};

pub fn language(info: Option<&str>) -> Option<SharedString> {
    let tag = info?.split_whitespace().next()?.to_ascii_lowercase();
    let config = LanguageRegistry::singleton().language(&tag)?;
    // Without the optional language bundle the registry falls back to JSON.
    if config.name == "text" || (config.name == "json" && !matches!(tag.as_str(), "json" | "jsonc"))
    {
        return None;
    }
    Some(config.name)
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CodeTokens {
    // Sorted disjoint ranges; capture ordering is retained for overlapping styles.
    pub spans: Vec<(Range<usize>, Vec<SharedString>)>,
}

impl CodeTokens {
    pub fn new(code: &str, lang: Option<&str>) -> Self {
        let Some(lang) = language(lang) else {
            return Self::default();
        };
        let mut highlighter = SyntaxHighlighter::new(&lang);
        highlighter.update(None, &Rope::from_str(code));
        let captures = highlighter.captures(0..code.len());
        let mut events: BTreeMap<usize, Vec<(usize, bool)>> = BTreeMap::new();
        for (id, (range, _)) in captures.iter().enumerate() {
            if range.start < range.end && range.end <= code.len() {
                events.entry(range.start).or_default().push((id, true));
                events.entry(range.end).or_default().push((id, false));
            }
        }
        let mut active: BTreeMap<SharedString, BTreeSet<usize>> = BTreeMap::new();
        let mut previous = 0;
        let mut spans = Vec::new();
        for (offset, changes) in events {
            if previous < offset && !active.is_empty() {
                // Repeated captures need only their last occurrence. Bound each
                // span by the number of capture names, not the size of the block.
                let mut names = active
                    .iter()
                    .map(|(name, ids)| (*ids.last().unwrap(), name.clone()))
                    .collect::<Vec<_>>();
                names.sort_by_key(|(id, _)| *id);
                spans.push((
                    previous..offset,
                    names.into_iter().map(|(_, name)| name).collect(),
                ));
            }
            for (id, add) in changes {
                let name = &captures[id].1;
                if add {
                    active.entry(name.clone()).or_default().insert(id);
                } else if let Some(ids) = active.get_mut(name) {
                    ids.remove(&id);
                    if ids.is_empty() {
                        active.remove(name);
                    }
                }
            }
            previous = offset;
        }
        Self { spans }
    }

    pub fn styles(
        &self,
        range: Range<usize>,
        theme: &HighlightTheme,
    ) -> Vec<(Range<usize>, HighlightStyle)> {
        let start = self
            .spans
            .partition_point(|(span, _)| span.end <= range.start);
        self.spans[start..]
            .iter()
            .take_while(|(span, _)| span.start < range.end)
            .map(|(span, names)| {
                let mut style = HighlightStyle::default();
                for name in names {
                    if let Some(next) = theme.style(name) {
                        super::highlighter::merge_highlight_style(&mut style, &next);
                    }
                }
                (span.start.max(range.start)..span.end.min(range.end), style)
            })
            .collect()
    }
}

/// Exact per-line mapping for literal fenced code. Unlike prose, code never
/// decodes entities or escapes. Markdown has only stripped container prefixes,
/// fence indentation and CRs from the parser's value.
#[derive(Clone, Debug)]
pub struct CodeLineMap {
    pub line_start: usize,
    pub source: Range<usize>,
    pub display: Range<usize>,
}

pub fn code_lines(source: &str, block: Range<usize>, value: &str) -> Vec<CodeLineMap> {
    let Some(raw) = source.get(block.clone()) else {
        return Vec::new();
    };
    let Some(newline) = raw.find('\n') else {
        return Vec::new();
    };
    let mut offset = block.start + newline + 1;
    let mut display = 0;
    let mut result = Vec::new();
    for line in value.split('\n') {
        if offset >= block.end {
            break;
        }
        let end = source[offset..block.end]
            .find('\n')
            .map_or(block.end, |n| offset + n);
        let raw_line = if line.ends_with('\r') {
            &source[offset..end]
        } else {
            source[offset..end].trim_end_matches('\r')
        };
        if raw_line.ends_with(line) {
            result.push(CodeLineMap {
                line_start: offset,
                source: offset + raw_line.len() - line.len()..offset + raw_line.len(),
                display: display..display + line.len(),
            });
        } else {
            // A partially consumed container-indentation tab can become spaces
            // in mdast. Align from the literal suffix so prefix characters can
            // never accidentally consume matching characters inside the code.
            let mut raw_end = raw_line.len();
            let mut value_end = line.len();
            let mut mapped = Vec::new();
            while value_end > 0 && raw_end > 0 {
                let a = raw_line[..raw_end].chars().next_back().unwrap();
                let b = line[..value_end].chars().next_back().unwrap();
                let count = if a == b {
                    b.len_utf8()
                } else if a == '\t' && b == ' ' {
                    line[..value_end]
                        .bytes()
                        .rev()
                        .take(4)
                        .take_while(|b| *b == b' ')
                        .count()
                } else {
                    break;
                };
                mapped.push(CodeLineMap {
                    line_start: offset,
                    source: offset + raw_end - a.len_utf8()..offset + raw_end,
                    display: display + value_end - count..display + value_end,
                });
                raw_end -= a.len_utf8();
                value_end -= count;
            }
            if value_end == 0 {
                mapped.reverse();
                result.extend(mapped);
            }
        }
        offset = end + 1;
        display += line.len() + 1;
    }
    result
}
