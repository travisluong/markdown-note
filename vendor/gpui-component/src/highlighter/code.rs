//! Shared, theme-independent fenced-code highlighting.
use super::{HighlightTheme, LanguageRegistry, SyntaxHighlighter};
use gpui::{HighlightStyle, SharedString};
use ropey::Rope;
use std::{
    collections::{BTreeMap, BTreeSet},
    ops::Range,
};

pub(crate) fn language(info: Option<&str>) -> Option<SharedString> {
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
pub(crate) struct CodeTokens {
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
        let mut active = BTreeSet::new();
        let mut previous = 0;
        let mut spans = Vec::new();
        for (offset, changes) in events {
            if previous < offset && !active.is_empty() {
                spans.push((
                    previous..offset,
                    active
                        .iter()
                        .map(|id: &usize| captures[*id].1.clone())
                        .collect(),
                ));
            }
            for (id, add) in changes {
                if add {
                    active.insert(id);
                } else {
                    active.remove(&id);
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
