use gpui::SharedString;
use markdown::{
    ParseOptions,
    mdast::{self, Node},
};

use crate::{
    highlighter::HighlightTheme,
    text::{
        TextViewStyle,
        node::{
            self, CodeBlock, ImageNode, InlineNode, LinkMark, NodeContext, Paragraph, Span, Table,
            TableRow, TextMark,
        },
    },
};

/// Parse Markdown into a tree of nodes.
pub(crate) fn parse(
    raw: &str,
    style: &TextViewStyle,
    cx: &mut NodeContext,
    highlight_theme: &HighlightTheme,
) -> Result<node::Node, SharedString> {
    cx.source = raw.to_owned().into();
    markdown::to_mdast(&raw, &ParseOptions::gfm())
        .map(|n| ast_to_node(n, style, cx, highlight_theme))
        .map_err(|e| e.to_string().into())
}

fn parse_table_row(table: &mut Table, node: &mdast::TableRow, cx: &mut NodeContext) {
    let mut row = TableRow::default();
    node.children.iter().for_each(|c| {
        match c {
            Node::TableCell(cell) => {
                parse_table_cell(&mut row, cell, cx);
            }
            _ => {}
        };
    });
    table.children.push(row);
}

fn parse_table_cell(row: &mut node::TableRow, node: &mdast::TableCell, cx: &mut NodeContext) {
    let mut paragraph = Paragraph::default();
    node.children.iter().for_each(|c| {
        parse_paragraph(&mut paragraph, c, cx);
    });
    let table_cell = node::TableCell {
        children: paragraph,
        ..Default::default()
    };
    row.children.push(table_cell);
}

fn parse_paragraph(paragraph: &mut Paragraph, node: &mdast::Node, cx: &mut NodeContext) -> String {
    let span = node.position().map(|pos| Span {
        start: pos.start.offset,
        end: pos.end.offset,
    });
    if let Some(span) = span {
        paragraph.set_span(span);
    }

    let first_child = paragraph.children.len();
    let mut text = String::new();

    match node {
        Node::Paragraph(val) => {
            val.children.iter().for_each(|c| {
                text.push_str(&parse_paragraph(paragraph, c, cx));
            });
        }
        Node::Text(val) => {
            text = val.value.clone();
            paragraph.push_str(&val.value)
        }
        Node::Emphasis(val) => {
            let mut child_paragraph = Paragraph::default();
            for child in val.children.iter() {
                text.push_str(&parse_paragraph(&mut child_paragraph, &child, cx));
            }
            for child in &mut child_paragraph.children {
                child
                    .marks
                    .push((0..child.text.len(), TextMark::default().italic()));
            }
            paragraph.merge(child_paragraph);
        }
        Node::Strong(val) => {
            let mut child_paragraph = Paragraph::default();
            for child in val.children.iter() {
                text.push_str(&parse_paragraph(&mut child_paragraph, &child, cx));
            }
            for child in &mut child_paragraph.children {
                child
                    .marks
                    .push((0..child.text.len(), TextMark::default().bold()));
            }
            paragraph.merge(child_paragraph);
        }
        Node::Delete(val) => {
            let mut child_paragraph = Paragraph::default();
            for child in val.children.iter() {
                text.push_str(&parse_paragraph(&mut child_paragraph, &child, cx));
            }
            for child in &mut child_paragraph.children {
                child
                    .marks
                    .push((0..child.text.len(), TextMark::default().strikethrough()));
            }
            paragraph.merge(child_paragraph);
        }
        Node::InlineCode(val) => {
            text = val.value.clone();
            paragraph.push(
                InlineNode::new(&text).marks(vec![(0..text.len(), TextMark::default().code())]),
            );
        }
        Node::Link(val) => {
            let link_mark = Some(LinkMark {
                url: val.url.clone().into(),
                title: val.title.clone().map(|s| s.into()),
                ..Default::default()
            });

            let mut child_paragraph = Paragraph::default();
            for child in val.children.iter() {
                text.push_str(&parse_paragraph(&mut child_paragraph, &child, cx));
            }

            // FIXME: GPUI InteractiveText does not support inline images yet.
            // So here we push images to the paragraph directly.
            for child in child_paragraph.children.iter_mut() {
                if let Some(image) = child.image.as_mut() {
                    image.link = link_mark.clone();
                }

                child.marks.push((
                    0..child.text.len(),
                    TextMark {
                        link: link_mark.clone(),
                        ..Default::default()
                    },
                ));
            }

            paragraph.merge(child_paragraph);
        }
        Node::Image(raw) => {
            paragraph.push_image(ImageNode {
                url: raw.url.clone().into(),
                title: raw.title.clone().map(|t| t.into()),
                alt: Some(raw.alt.clone().into()),
                ..Default::default()
            });
        }
        Node::InlineMath(raw) => {
            text = raw.value.clone();
            paragraph.push(
                InlineNode::new(&text).marks(vec![(0..text.len(), TextMark::default().code())]),
            );
        }
        Node::MdxTextExpression(raw) => {
            text = raw.value.clone();
            paragraph
                .push(InlineNode::new(&text).marks(vec![(0..text.len(), TextMark::default())]));
        }
        Node::Html(val) => match super::html::parse(&val.value, cx) {
            Ok(el) => {
                if el.is_break() {
                    text = "\n".to_owned();
                    paragraph.push(InlineNode::new(&text));
                } else {
                    if cfg!(debug_assertions) {
                        tracing::warn!("unsupported inline html tag: {:#?}", el);
                    }
                }
            }
            Err(err) => {
                if cfg!(debug_assertions) {
                    tracing::warn!("failed parsing html: {:#?}", err);
                }

                text.push_str(&val.value);
            }
        },
        Node::FootnoteReference(foot) => {
            let prefix = format!("[{}]", foot.identifier);
            paragraph.push(InlineNode::new(&prefix).marks(vec![(
                0..prefix.len(),
                TextMark {
                    italic: true,
                    ..Default::default()
                },
            )]));
        }
        Node::LinkReference(link) => {
            let mut child_paragraph = Paragraph::default();
            let mut child_text = String::new();
            for child in link.children.iter() {
                child_text.push_str(&parse_paragraph(&mut child_paragraph, child, cx));
            }

            let link_mark = LinkMark {
                url: "".into(),
                title: link.label.clone().map(Into::into),
                identifier: Some(link.identifier.clone().into()),
            };

            for child in &mut child_paragraph.children {
                child.marks.push((
                    0..child.text.len(),
                    TextMark {
                        link: Some(link_mark.clone()),
                        ..Default::default()
                    },
                ));
            }
            paragraph.merge(child_paragraph);
            text = child_text;
        }
        _ => {
            if cfg!(debug_assertions) {
                tracing::warn!("unsupported inline node: {:#?}", node);
            }
        }
    }

    if let Some(span) = span {
        // Containers retain the exact mappings from their parsed children.
        if node.children().is_none() {
            let mut range = span.start..span.end;
            if matches!(node, Node::InlineCode(_)) {
                let raw = &cx.source[range.clone()];
                let delimiter = raw.bytes().take_while(|b| *b == b'`').count();
                range.start += delimiter;
                range.end = range.end.saturating_sub(delimiter).max(range.start);
            }
            for child in paragraph.children.iter_mut().skip(first_child) {
                child.source_runs =
                    super::super::source_map::map_leaf(&cx.source, range.clone(), &child.text);
            }
        }
    }
    text
}

fn ast_to_node(
    value: mdast::Node,
    style: &TextViewStyle,
    cx: &mut NodeContext,
    highlight_theme: &HighlightTheme,
) -> node::Node {
    match value {
        Node::Root(val) => {
            cx.blocks = val
                .children
                .iter()
                .map(|child| {
                    child
                        .position()
                        .map_or(0..0, |p| p.start.offset..p.end.offset)
                })
                .collect();
            let children = val
                .children
                .into_iter()
                .map(|c| ast_to_node(c, style, cx, highlight_theme))
                .collect();
            node::Node::Root { children }
        }
        Node::Paragraph(val) => {
            let mut paragraph = Paragraph::default();
            val.children.iter().for_each(|c| {
                parse_paragraph(&mut paragraph, c, cx);
            });

            node::Node::Paragraph(paragraph)
        }
        Node::Blockquote(val) => {
            let children = val
                .children
                .into_iter()
                .map(|c| ast_to_node(c, style, cx, highlight_theme))
                .collect();
            node::Node::Blockquote { children }
        }
        Node::List(list) => {
            let children = list
                .children
                .into_iter()
                .map(|c| ast_to_node(c, style, cx, highlight_theme))
                .collect();
            node::Node::List {
                ordered: list.ordered,
                children,
            }
        }
        Node::ListItem(val) => {
            let children = val
                .children
                .into_iter()
                .map(|c| ast_to_node(c, style, cx, highlight_theme))
                .collect();
            node::Node::ListItem {
                children,
                spread: val.spread,
                checked: val.checked,
                task_offset: val.position.as_ref().and_then(|pos| {
                    let tail = cx.source.get(pos.start.offset..pos.end.offset)?;
                    let line = tail.lines().next()?;
                    let bracket = line.find('[')?;
                    let bytes = line.as_bytes();
                    (bytes.get(bracket + 2) == Some(&b']')
                        && bytes
                            .get(bracket + 1)
                            .is_some_and(|c| matches!(c, b' ' | b'x' | b'X')))
                    .then_some(pos.start.offset + bracket + 1)
                }),
            }
        }
        Node::Break(_) => node::Node::Break { html: false },
        Node::Code(raw) => {
            let mut block = CodeBlock::new(
                raw.value.clone().into(),
                raw.lang.map(Into::into),
                style,
                highlight_theme,
            );
            if let Some(position) = raw.position {
                let range = position.start.offset..position.end.offset;
                let source = &cx.source[range.clone()];
                if source.starts_with("```") || source.starts_with("~~~") {
                    for line in crate::highlighter::code::code_lines(&cx.source, range, &raw.value) {
                        let source_end = line.source.end;
                        let display_end = line.display.end;
                        block.source_runs.push(super::super::source_map::SourceRun {
                            source: line.source, display: line.display,
                        });
                        if raw.value.as_bytes().get(display_end) == Some(&b'\n')
                            && cx.source.as_bytes().get(source_end) == Some(&b'\n') {
                            block.source_runs.push(super::super::source_map::SourceRun {
                                source: source_end..source_end + 1,
                                display: display_end..display_end + 1,
                            });
                        }
                    }
                } else {
                    block.source_runs = super::super::source_map::map_leaf(&cx.source, range, &raw.value);
                }
            }
            node::Node::CodeBlock(block)
        }
        Node::Heading(val) => {
            let mut paragraph = Paragraph::default();
            val.children.iter().for_each(|c| {
                parse_paragraph(&mut paragraph, c, cx);
            });

            node::Node::Heading {
                level: val.depth,
                children: paragraph,
            }
        }
        Node::Math(val) => node::Node::CodeBlock(CodeBlock::new(
            val.value.into(),
            None,
            style,
            highlight_theme,
        )),
        Node::Html(val) => match super::html::parse(&val.value, cx) {
            Ok(el) => el,
            Err(err) => {
                if cfg!(debug_assertions) {
                    tracing::warn!("error parsing html: {:#?}", err);
                }

                node::Node::Paragraph(Paragraph::new(val.value))
            }
        },
        Node::MdxFlowExpression(val) => node::Node::CodeBlock(CodeBlock::new(
            val.value.into(),
            Some("mdx".into()),
            style,
            highlight_theme,
        )),
        Node::Yaml(val) => node::Node::CodeBlock(CodeBlock::new(
            val.value.into(),
            Some("yml".into()),
            style,
            highlight_theme,
        )),
        Node::Toml(val) => node::Node::CodeBlock(CodeBlock::new(
            val.value.into(),
            Some("toml".into()),
            style,
            highlight_theme,
        )),
        Node::MdxJsxTextElement(val) => {
            let mut paragraph = Paragraph::default();
            val.children.iter().for_each(|c| {
                parse_paragraph(&mut paragraph, c, cx);
            });
            node::Node::Paragraph(paragraph)
        }
        Node::MdxJsxFlowElement(val) => {
            let mut paragraph = Paragraph::default();
            val.children.iter().for_each(|c| {
                parse_paragraph(&mut paragraph, c, cx);
            });
            node::Node::Paragraph(paragraph)
        }
        Node::ThematicBreak(_) => node::Node::Divider,
        Node::Table(val) => {
            let mut table = Table::default();
            table.source_start = val.position.as_ref().map_or(0, |pos| pos.start.offset);
            table.column_aligns = val
                .align
                .clone()
                .into_iter()
                .map(|align| align.into())
                .collect();
            val.children.iter().for_each(|c| {
                if let Node::TableRow(row) = c {
                    parse_table_row(&mut table, row, cx);
                }
            });

            node::Node::Table(table)
        }
        Node::FootnoteDefinition(def) => {
            let mut paragraph = Paragraph::default();
            let prefix = format!("[{}]: ", def.identifier);
            paragraph.push(InlineNode::new(&prefix).marks(vec![(
                0..prefix.len(),
                TextMark {
                    italic: true,
                    ..Default::default()
                },
            )]));

            def.children.iter().for_each(|c| {
                parse_paragraph(&mut paragraph, c, cx);
            });
            node::Node::Paragraph(paragraph)
        }
        Node::Definition(def) => {
            cx.add_ref(
                def.identifier.clone().into(),
                LinkMark {
                    url: def.url.clone().into(),
                    identifier: Some(def.identifier.clone().into()),
                    title: def.title.clone().map(Into::into),
                },
            );

            node::Node::Definition {
                identifier: def.identifier.clone().into(),
                url: def.url.clone().into(),
                title: def.title.clone().map(|s| s.into()),
            }
        }
        _ => {
            if cfg!(debug_assertions) {
                tracing::warn!("unsupported node: {:#?}", value);
            }
            node::Node::Unknown
        }
    }
}
