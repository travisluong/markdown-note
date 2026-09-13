//! Whole-item Markdown moves, derived from the shared GFM tree.
use super::live_preview::TaskEdit;
use markdown::mdast::Node;
use std::ops::Range;

#[derive(Clone, Debug)]
pub(super) struct Item {
    pub source: Range<usize>,
    pub marker: usize,
    pub quote: Option<usize>,
    pub prefix: String,
    pub indent: usize,
    pub bullet: char,
}
fn prefix_end(line: &str) -> usize {
    let bytes = line.as_bytes();
    let mut end = 0;
    loop {
        let spaces = bytes[end..]
            .iter()
            .take_while(|c| matches!(c, b' ' | b'\t'))
            .count();
        if bytes.get(end + spaces) != Some(&b'>') {
            return end;
        }
        end += spaces + 1;
        if bytes.get(end) == Some(&b' ') {
            end += 1;
        }
    }
}
fn columns(s: &str) -> usize {
    s.chars()
        .fold(0, |n, c| if c == '\t' { (n / 4 + 1) * 4 } else { n + 1 })
}
pub(super) fn from_ast(source: &str, root: &Node) -> Vec<Item> {
    fn visit(
        source: &str,
        node: &Node,
        quote: Option<usize>,
        unordered: bool,
        out: &mut Vec<Item>,
    ) {
        let quote = if matches!(node, Node::Blockquote(_)) {
            node.position().map(|p| p.start.offset)
        } else {
            quote
        };
        let unordered = if let Node::List(list) = node {
            !list.ordered
        } else {
            unordered
        };
        if let Node::ListItem(item) = node
            && unordered
            && let Some(pos) = &item.position
        {
            let marker = pos.start.offset;
            let start = source[..marker].rfind('\n').map_or(0, |at| at + 1);
            let end = if source[..pos.end.offset].ends_with('\n') {
                pos.end.offset
            } else {
                source[pos.end.offset..]
                    .find('\n')
                    .map_or(source.len(), |at| pos.end.offset + at + 1)
            };
            let prefix = &source[start..marker];
            if let Some(bullet @ ('-' | '*' | '+')) = source[marker..].chars().next() {
                let quote_end = prefix_end(prefix);
                out.push(Item {
                    source: start..end,
                    marker,
                    quote,
                    prefix: prefix[..quote_end].into(),
                    indent: columns(&prefix[quote_end..]),
                    bullet,
                });
            }
        }
        if let Some(children) = node.children() {
            for child in children {
                visit(source, child, quote, unordered, out);
            }
        }
    }
    let mut out = Vec::new();
    visit(source, root, None, false, &mut out);
    out.sort_by_key(|item| item.marker);
    out
}
pub(super) fn can_drop(item: &Item, target: &Item) -> bool {
    item.quote == target.quote && !item.source.contains(&target.marker)
}
#[cfg(test)]
pub(super) fn reorder(
    source: &str,
    item: &Item,
    target: &Item,
    after: bool,
    selection: Range<usize>,
) -> Option<TaskEdit> {
    reorder_anchored(source, item, target, after, selection, 0).map(|(mut edit, _)| {
        let mut result = source.to_owned();
        result.replace_range(edit.range, &edit.text);
        edit.range = 0..source.len();
        edit.text = result;
        edit
    })
}

pub(super) fn reorder_anchored(
    source: &str,
    item: &Item,
    target: &Item,
    after: bool,
    selection: Range<usize>,
    anchor: usize,
) -> Option<(TaskEdit, usize)> {
    if !can_drop(item, target) {
        return None;
    }
    let destination = if after {
        target.source.end
    } else {
        target.source.start
    };
    if destination > item.source.start && destination < item.source.end {
        return None;
    }
    let newline = if source.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    let mut block = String::new();
    let mut mapped = Vec::new();
    let mut old_start = item.source.start;
    for (row, line) in source[item.source.clone()]
        .split_inclusive('\n')
        .enumerate()
    {
        let prefix = prefix_end(line);
        let whitespace = line[prefix..]
            .bytes()
            .take_while(|c| matches!(c, b' ' | b'\t'))
            .count();
        let old_content = prefix + whitespace;
        let old_indent = columns(&line[prefix..old_content]);
        let indent = if row == 0 {
            target.indent
        } else {
            (old_indent.saturating_sub(item.indent) + target.indent).max(target.indent + 2)
        };
        let start = block.len();
        if !line.trim().is_empty() {
            block.push_str(&item.prefix);
            block.push_str(&" ".repeat(indent));
        }
        let new_content = block.len() - start;
        let mut content = line[old_content..].to_owned();
        if row == 0 {
            content.replace_range(..1, &target.bullet.to_string());
        }
        block.push_str(&content);
        mapped.push((
            old_start..old_start + line.len(),
            start,
            old_content,
            new_content,
        ));
        old_start += line.len();
    }
    if !block.ends_with('\n') {
        block.push_str(newline);
    }
    let mut result = source.to_owned();
    result.replace_range(item.source.clone(), "");
    let at = if destination >= item.source.end {
        destination - item.source.len()
    } else {
        destination
    };
    let separator = if at > 0 && !result[..at].ends_with('\n') {
        newline
    } else {
        ""
    };
    let insertion_start = at + separator.len();
    result.insert_str(at, &(separator.to_owned() + &block));
    if !source.ends_with('\n') && result.ends_with('\n') {
        result.pop();
        if result.ends_with('\r') {
            result.pop();
        }
    }
    let map = |offset: usize| {
        if offset >= item.source.start && offset <= item.source.end {
            let (range, start, old_content, new_content) = mapped
                .iter()
                .find(|(r, _, _, _)| r.contains(&offset))
                .unwrap_or_else(|| mapped.last().unwrap());
            return (insertion_start
                + start
                + new_content
                + offset.saturating_sub(range.start + old_content))
            .min(result.len());
        }
        let removed = if offset >= item.source.end {
            offset - item.source.len()
        } else {
            offset
        };
        (removed
            + if removed >= at {
                block.len() + separator.len()
            } else {
                0
            })
        .min(result.len())
    };
    let (start, end) = (map(selection.start), map(selection.end));
    if result == source {
        return None;
    }
    let anchor = map(anchor);
    Some((
        minimal_edit(source, &result, start.min(end)..start.max(end)),
        anchor,
    ))
}

/// Preserve untouched layout caches by replacing only the differing UTF-8 span.
pub(super) fn minimal_edit(source: &str, result: &str, selection: Range<usize>) -> TaskEdit {
    let prefix: usize = source
        .chars()
        .zip(result.chars())
        .take_while(|(a, b)| a == b)
        .map(|(c, _)| c.len_utf8())
        .sum();
    let suffix: usize = source[prefix..]
        .chars()
        .rev()
        .zip(result[prefix..].chars().rev())
        .take_while(|(a, b)| a == b)
        .map(|(c, _)| c.len_utf8())
        .sum();
    TaskEdit {
        range: prefix..source.len() - suffix,
        text: result[prefix..result.len() - suffix].into(),
        selection,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn items(s: &str) -> Vec<Item> {
        from_ast(
            s,
            &markdown::to_mdast(s, &markdown::ParseOptions::gfm()).unwrap(),
        )
    }
    #[test]
    fn moves_subtrees_between_lists_and_keeps_task_state() {
        let s = "- [x] café\n  - child\n\nparagraph\n\n* other\n";
        let i = items(s);
        let edit = reorder(s, &i[0], &i[2], true, 6..11).unwrap();
        assert_eq!(edit.text, "\nparagraph\n\n* other\n* [x] café\n  - child\n");
        assert_eq!(&edit.text[edit.selection], "café");
        assert!(!can_drop(&i[0], &i[1]));
    }
    #[test]
    fn nesting_eof_and_quote_boundaries() {
        let s = "- first\n  - child\n- last";
        let i = items(s);
        let edit = reorder(s, &i[2], &i[1], false, 19..23).unwrap();
        assert_eq!(edit.text, "- first\n  - last\n  - child");
        let s = "> - a\n\n> - b\n\n1. numbered\n";
        let i = items(s);
        assert_eq!(i.len(), 2);
        assert!(!can_drop(&i[0], &i[1]));
    }
}

#[cfg(test)]
mod continuation_tests {
    use super::*;
    #[test]
    fn lazy_paragraphs_stay_inside_the_moved_nested_item() {
        let source = "- source\nlazy continuation\n\n- parent\n  - target\n";
        let root = markdown::to_mdast(source, &markdown::ParseOptions::gfm()).unwrap();
        let items = from_ast(source, &root);
        let edit = reorder(source, &items[0], &items[2], false, 2..8).unwrap();
        assert_eq!(
            edit.text,
            "\n- parent\n  - source\n    lazy continuation\n  - target\n"
        );
        let root = markdown::to_mdast(&edit.text, &markdown::ParseOptions::gfm()).unwrap();
        let moved = from_ast(&edit.text, &root);
        assert!(edit.text[moved[1].source.clone()].contains("lazy continuation"));
    }
    #[test]
    fn moving_past_eof_retains_crlf_without_adding_a_final_newline() {
        let source = "- a\r\n- b";
        let root = markdown::to_mdast(source, &markdown::ParseOptions::gfm()).unwrap();
        let items = from_ast(source, &root);
        assert_eq!(
            reorder(source, &items[0], &items[1], true, 2..3)
                .unwrap()
                .text,
            "- b\r\n- a"
        );
    }
}
