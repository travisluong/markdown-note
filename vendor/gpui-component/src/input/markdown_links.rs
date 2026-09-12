//! Source-based Markdown links shared by editing and presentation.
use markdown::{ParseOptions, mdast::Node};
use std::{collections::HashMap, ops::Range};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarkdownLink {
    pub source: Range<usize>,
    pub label: Range<usize>,
    pub destination: String,
    pub title: Option<String>,
}

/// Parse the whole document so definitions, code fences and containers have
/// the same meaning as Reading view. Offsets always refer to original UTF-8.
#[derive(Default)]
pub struct LinkIndex {
    pub links: Vec<MarkdownLink>,
    paragraphs: Vec<Range<usize>>,
    definitions: String,
    simple_definitions: bool,
    pub parsed_bytes: usize,
}

impl LinkIndex {
    pub fn update(&mut self, old: &str, source: &str, range: Range<usize>, inserted: &str) {
        let ordinary = |s: &str| s.chars().all(|c| c.is_alphanumeric() || c == ' ');
        let paragraph = self
            .paragraphs
            .iter()
            .find(|p| p.start < range.start && range.end < p.end)
            .cloned();
        if self.simple_definitions
            && ordinary(inserted)
            && ordinary(&old[range.clone()])
            && let Some(block) = paragraph
        {
            let delta = inserted.len() as isize - range.len() as isize;
            let end = block.end.saturating_add_signed(delta);
            let fragment = format!("{}\n\n{}", &source[block.start..end], self.definitions);
            let parsed = parse_document_inner(&fragment, false);
            // A paragraph becoming a list, HTML block, heading or definition
            // can change neighboring blocks, so use the document parser.
            if parsed.paragraphs.first() != Some(&(0..end - block.start)) {
                let mut next = parse_document(source);
                next.parsed_bytes += self.parsed_bytes + fragment.len();
                *self = next;
                return;
            }
            let mut links = parsed.links;
            links.retain(|link| link.source.end <= end - block.start);
            for link in &mut links {
                shift(link, block.start as isize);
            }
            self.links
                .retain(|link| link.source.end <= block.start || link.source.start >= block.end);
            for link in &mut self.links {
                if link.source.start >= block.end {
                    shift(link, delta);
                }
            }
            self.links.extend(links);
            self.links.sort_by_key(|link| link.source.start);
            for p in &mut self.paragraphs {
                if p.start >= block.end {
                    p.start = p.start.saturating_add_signed(delta);
                }
                if p.end >= block.end {
                    p.end = p.end.saturating_add_signed(delta);
                }
            }
            self.parsed_bytes += fragment.len();
            return;
        }
        let mut next = parse_document(source);
        next.parsed_bytes += self.parsed_bytes;
        *self = next;
    }
}

fn shift(link: &mut MarkdownLink, delta: isize) {
    link.source = link.source.start.saturating_add_signed(delta)
        ..link.source.end.saturating_add_signed(delta);
    link.label =
        link.label.start.saturating_add_signed(delta)..link.label.end.saturating_add_signed(delta);
}

pub fn parse(source: &str) -> Vec<MarkdownLink> {
    parse_document(source).links
}

fn parse_document(source: &str) -> LinkIndex {
    parse_document_inner(source, true)
}

pub(super) fn requires_parse(source: &str) -> bool {
    source
        .bytes()
        .any(|b| matches!(b, b'[' | b'<' | b':' | b'@'))
}

fn parse_document_inner(source: &str, allow_skip: bool) -> LinkIndex {
    if allow_skip && !requires_parse(source) {
        return LinkIndex::default();
    }
    let Ok(root) = markdown::to_mdast(source, &ParseOptions::gfm()) else {
        return LinkIndex::default();
    };
    from_ast(source, &root)
}

/// Build source indexes from a GFM tree parsed from this exact source.
pub(super) fn from_ast(source: &str, root: &Node) -> LinkIndex {
    fn definitions(node: &Node, refs: &mut HashMap<String, (String, Option<String>)>) {
        if let Node::Definition(d) = node {
            refs.entry(d.identifier.clone())
                .or_insert((d.url.clone(), d.title.clone()));
        }
        if let Some(children) = node.children() {
            for child in children {
                definitions(child, refs);
            }
        }
    }
    fn visit(
        node: &Node,
        refs: &HashMap<String, (String, Option<String>)>,
        out: &mut Vec<MarkdownLink>,
    ) {
        let target = match node {
            Node::Link(link) => Some((link.url.clone(), link.title.clone())),
            Node::LinkReference(link) => refs.get(&link.identifier).cloned(),
            _ => None,
        };
        if let (Some((destination, title)), Some(p), Some(children)) =
            (target, node.position(), node.children())
        {
            let label = match (
                children.first().and_then(Node::position),
                children.last().and_then(Node::position),
            ) {
                (Some(first), Some(last)) => first.start.offset..last.end.offset,
                _ => p.start.offset + 1..p.start.offset + 1,
            };
            out.push(MarkdownLink {
                source: p.start.offset..p.end.offset,
                label,
                destination,
                title,
            });
        }
        if let Some(children) = node.children() {
            for child in children {
                visit(child, refs, out);
            }
        }
    }
    let mut refs = HashMap::new();
    definitions(root, &mut refs);
    let mut links = Vec::new();
    visit(root, &refs, &mut links);
    let paragraphs = root
        .children()
        .into_iter()
        .flatten()
        .filter(|node| matches!(node, Node::Paragraph(_)))
        .filter_map(|node| node.position().map(|p| p.start.offset..p.end.offset))
        .collect();
    fn definition_source(node: &Node, source: &str, out: &mut String) {
        if matches!(node, Node::Definition(_))
            && let Some(p) = node.position()
        {
            out.push_str(&source[p.start.offset..p.end.offset]);
            out.push('\n');
        }
        if let Some(children) = node.children() {
            for child in children {
                definition_source(child, source, out);
            }
        }
    }
    fn nested_definition(node: &Node) -> bool {
        matches!(node, Node::Definition(_))
            || node
                .children()
                .is_some_and(|children| children.iter().any(nested_definition))
    }
    let simple_definitions = root.children().is_none_or(|children| {
        children
            .iter()
            .all(|node| matches!(node, Node::Definition(_)) || !nested_definition(node))
    });
    let mut definitions = String::new();
    definition_source(root, source, &mut definitions);
    LinkIndex {
        links,
        paragraphs,
        definitions,
        simple_definitions,
        parsed_bytes: source.len(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn references_unicode_and_code_context() {
        let source = "[**café**][ID]\r\n\r\n[id]: <https://example.com/a(b)> \"Title\"\r\n\r\n`[no](https://no)`\r\n\r\n```md\r\n[no](https://no)\r\n```\r\n\r\n<https://example.org>\r\n";
        let links = parse(source);
        assert_eq!(links.len(), 2);
        assert_eq!(&source[links[0].label.clone()], "**café**");
        assert_eq!(links[0].destination, "https://example.com/a(b)");
        assert_eq!(links[0].title.as_deref(), Some("Title"));
        assert_eq!(&source[links[1].label.clone()], "https://example.org");
        assert_eq!(parse(&source.replace("[id]:", "[other]:")).len(), 1);
    }
    #[test]
    fn malformed_escaped_and_tables() {
        assert!(parse(r"\[no](note.md) [broken](").is_empty());
        let source = "| a |\n| - |\n| [你好](../b.md) |\n";
        let links = parse(source);
        assert_eq!(links.len(), 1);
        assert_eq!(&source[links[0].label.clone()], "你好");
        assert_eq!(links[0].destination, "../b.md");
    }
}

#[cfg(test)]
mod incremental_tests {
    use super::*;

    #[test]
    fn incremental_edits_match_full_parser() {
        for initial in [
            "Before [café](https://example.com/a) after\n\n[next][id]\n\n[id]: other.md\n",
            "> [café][id]\n>\n> [id]: other.md\n\n[another][id]\n",
            "[café](note.md)\n\n```\n[code](note.md)\n```\n",
        ] {
            let mut source = initial.to_owned();
            let mut index = LinkIndex::default();
            index.update("", &source, 0..0, &source);
            for inserted in ["a", "é", " ", "[", "\n", "]", "`", ">", "x"] {
                let at = source.find("café").unwrap() + 1;
                let old = source.clone();
                source.insert_str(at, inserted);
                index.update(&old, &source, at..at, inserted);
                assert_eq!(index.links, parse(&source), "insert {inserted:?}");
                let old = source.clone();
                source.replace_range(at..at + inserted.len(), "");
                index.update(&old, &source, at..at + inserted.len(), "");
                assert_eq!(index.links, parse(&source), "remove {inserted:?}");
            }
        }
    }

    #[test]
    fn label_typing_parses_only_its_paragraph() {
        let mut source = "[label](note.md)\n\n".repeat(10_000);
        let mut index = LinkIndex::default();
        index.update("", &source, 0..0, &source);
        assert_eq!(index.links.len(), 10_000);
        let before = index.parsed_bytes;
        let old = source.clone();
        source.insert(3, 'x');
        index.update(&old, &source, 3..3, "x");
        assert!(index.parsed_bytes - before < 100);
        assert_eq!(index.links, parse(&source));
    }
}

#[cfg(test)]
mod paragraph_tests {
    use super::*;
    #[test]
    fn paragraph_fast_path_matches_context_changes() {
        for source in [
            "[one](note.md)\n\nplain text here\n\nend\n",
            "[one][id]\n\n12345x. list?\n\n[id]: note.md\n",
            "[one](note.md)\n\n<scri pt>\n\n[next](note.md)\n",
            "[one](note.md)\n\ntext\nline here\n\nend\n",
        ] {
            for (at, ch) in source.char_indices() {
                if !ch.is_alphanumeric() {
                    continue;
                }
                for inserted in ["", "x", " "] {
                    let mut index = LinkIndex::default();
                    index.update("", source, 0..0, source);
                    let mut updated = source.to_owned();
                    let range = at..at + ch.len_utf8();
                    updated.replace_range(range.clone(), inserted);
                    index.update(source, &updated, range, inserted);
                    assert_eq!(
                        index.links,
                        parse(&updated),
                        "{source:?}: {at} -> {inserted:?}"
                    );
                }
            }
        }
    }
}
