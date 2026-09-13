//! Source ranges from the shared GFM tree; no parser runs during layout.
use markdown::mdast::Node;
use std::ops::Range;

#[derive(Clone, Debug, PartialEq)]
pub(super) struct CodeSpan {
    pub source: Range<usize>,
    pub content: Range<usize>,
}
pub(super) fn from_ast(source: &str, root: &Node) -> Vec<CodeSpan> {
    fn visit(source: &str, node: &Node, spans: &mut Vec<CodeSpan>) {
        if let Node::InlineCode(code) = node {
            if let Some(pos) = &code.position {
                let range = pos.start.offset..pos.end.offset;
                let raw = &source[range.clone()];
                let ticks = raw.bytes().take_while(|c| *c == b'`').count();
                if ticks > 0 && raw.len() >= ticks * 2 {
                    spans.push(CodeSpan {
                        content: range.start + ticks..range.end - ticks,
                        source: range,
                    });
                }
            }
        } else if let Some(children) = node.children() {
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
    fn gfm_ranges_cover_backticks_unicode_and_multiline_code() {
        let source = "`*café*` and ``a ` b`` and `x\ny`\n\n```\n`literal`\n```";
        let root = markdown::to_mdast(source, &markdown::ParseOptions::gfm()).unwrap();
        let spans = from_ast(source, &root);
        assert_eq!(
            spans
                .iter()
                .map(|s| &source[s.content.clone()])
                .collect::<Vec<_>>(),
            ["*café*", "a ` b", "x\ny"]
        );
        assert!(
            from_ast(
                "unmatched `code",
                &markdown::to_mdast("unmatched `code", &markdown::ParseOptions::gfm()).unwrap()
            )
            .is_empty()
        );
    }
}
#[cfg(test)]
mod presentation_tests {
    #[test]
    fn inline_code_preserves_literal_markers_and_source_mapping() {
        let source = "text ``*café* ` _茶_`` tail";
        let ast = markdown::to_mdast(source, &markdown::ParseOptions::gfm()).unwrap();
        let spans = super::from_ast(source, &ast);
        let p = super::super::live_preview::Presentation::parse(source).with_inline_code(
            &spans,
            0,
            source.len(),
        );
        let mapped = p.display(source, false);
        assert_eq!(mapped.text, "text *café* ` _茶_ tail");
        assert_eq!(p.display(source, true).text, source);
        for offset in spans[0]
            .content
            .clone()
            .filter(|offset| source.is_char_boundary(*offset))
        {
            assert_eq!(
                mapped.display_to_source[mapped.source_to_display[offset]],
                offset
            );
        }
    }
}
