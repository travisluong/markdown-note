//! Strikethrough ranges derived from the shared worker's GFM tree.
use markdown::mdast::Node;
use std::ops::Range;

#[derive(Clone, Debug, PartialEq)]
pub(super) struct StrikeSpan {
    pub source: Range<usize>,
    pub content: Range<usize>,
}

pub(super) fn from_ast(source: &str, root: &Node) -> Vec<StrikeSpan> {
    fn visit(source: &str, node: &Node, spans: &mut Vec<StrikeSpan>) {
        if let Node::Delete(delete) = node
            && let Some(pos) = &delete.position
        {
            let range = pos.start.offset..pos.end.offset;
            let markers = source[range.clone()]
                .bytes()
                .take_while(|b| *b == b'~')
                .count();
            if markers > 0 && range.len() > markers * 2 {
                spans.push(StrikeSpan {
                    content: range.start + markers..range.end - markers,
                    source: range,
                });
            }
        }
        if let Some(children) = node.children() {
            for child in children {
                visit(source, child, spans);
            }
        }
    }
    let mut spans = Vec::new();
    visit(source, root, &mut spans);
    spans.sort_by_key(|span| span.source.start);
    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gfm_strikethrough_excludes_literal_and_invalid_markers() {
        let source = "~~café **茶**~~ and ~single~ and ~~first\r\nsecond~~\n\n`~~code~~` \\~\\~escaped\\~\\~ ~~unclosed\n\n```\n~~fenced~~\n```\n\n    ~~indented~~\n\n[label](https://example.com/~~path~~)";
        let ast = markdown::to_mdast(source, &markdown::ParseOptions::gfm()).unwrap();
        let spans = from_ast(source, &ast);
        assert_eq!(
            spans
                .iter()
                .map(|s| &source[s.content.clone()])
                .collect::<Vec<_>>(),
            ["café **茶**", "single", "first\r\nsecond"]
        );
    }

    #[test]
    fn strike_presentation_hides_delimiters_and_preserves_utf8_mapping() {
        let source = "> ~~café **茶** and `code`~~ tail";
        let ast = markdown::to_mdast(source, &markdown::ParseOptions::gfm()).unwrap();
        let spans = from_ast(source, &ast);
        let p = super::super::live_preview::Presentation::parse(source)
            .with_inline_code(
                &super::super::inline_code::from_ast(source, &ast),
                0,
                source.len(),
            )
            .with_strikethrough(&spans, 0, source.len());
        let mapped = p.display(source, false);
        assert_eq!(mapped.text, "café 茶 and code tail");
        assert_eq!(p.display(source, true).text, source);
        for (offset, ch) in mapped.text.char_indices() {
            let original = mapped.display_to_source[offset];
            assert!(source[original..].starts_with(ch));
            assert_eq!(mapped.source_to_display[original], offset);
        }
        assert!(p.strikethrough[0].contains(&source.find("code").unwrap()));
    }
}
