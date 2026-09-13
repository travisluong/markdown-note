//! Source-local numeric list transactions. No GFM tree is built on the input thread.
use super::live_preview::TaskEdit;
use std::ops::Range;

#[derive(Clone, Debug)]
pub(super) struct NumericItem {
    pub source: Range<usize>,
    pub digits: Range<usize>,
    pub value: usize,
    pub list: usize,
    content_column: usize,
}

pub(super) fn columns(text: &str) -> usize {
    text.bytes()
        .fold(0, |n, b| if b == b'\t' { (n / 4 + 1) * 4 } else { n + 1 })
}

/// Byte offsets after explicit quote prefixes and indentation, and quote depth.
fn prefix(line: &str) -> (usize, usize, usize) {
    let (mut end, mut depth) = (0, 0);
    loop {
        let spaces = line[end..]
            .bytes()
            .take_while(|b| matches!(b, b' ' | b'\t'))
            .count();
        if line.as_bytes().get(end + spaces) != Some(&b'>') {
            return (end, end + spaces, depth);
        }
        end += spaces + 1;
        if line.as_bytes().get(end) == Some(&b' ') {
            end += 1;
        }
        depth += 1;
    }
}

pub(super) fn numeric_marker(line: &str) -> bool {
    let (_, start, _) = prefix(line);
    marker(&line[start..]).is_some_and(|(_, _, number)| number.is_some())
}

fn marker(body: &str) -> Option<(usize, u8, Option<usize>)> {
    let digits = body.bytes().take_while(u8::is_ascii_digit).count();
    let (end, kind, number) =
        if (1..=9).contains(&digits) && matches!(body.as_bytes().get(digits), Some(b'.' | b')')) {
            (
                digits + 1,
                body.as_bytes()[digits],
                Some(body[..digits].parse().ok()?),
            )
        } else if matches!(body.as_bytes().first(), Some(b'-' | b'*' | b'+')) {
            (1, body.as_bytes()[0], None)
        } else {
            return None;
        };
    let spaces = body[end..]
        .bytes()
        .take_while(|b| matches!(b, b' ' | b'\t'))
        .count();
    (spaces > 0 || body[end..].trim().is_empty()).then_some((end + spaces, kind, number))
}

/// Track list containers and literal fences while scanning source lines. Numeric
/// sibling groups retain separate identities across paragraphs and quote blocks.
pub(super) fn scan(source: &str) -> Vec<NumericItem> {
    struct Frame {
        indent: usize,
        content: usize,
        depth: usize,
        kind: u8,
        id: usize,
        last: Option<usize>,
    }
    let mut frames: Vec<Frame> = Vec::new();
    let mut items: Vec<NumericItem> = Vec::new();
    let (mut offset, mut next_id, mut blank) = (0, 0, true);
    let mut fence: Option<(u8, usize, usize, usize)> = None;
    for line in source.split_inclusive('\n') {
        let text = line.trim_end_matches(['\r', '\n']);
        let (quote, start, depth) = prefix(text);
        let indent = columns(&text[quote..start]);
        let body = &text[start..];
        while frames.last().is_some_and(|f| f.depth > depth) {
            frames.pop();
        }
        if let Some((_, _, fence_depth, fence_indent)) = fence {
            if depth < fence_depth || (!body.is_empty() && indent < fence_indent) {
                fence = None;
            }
        }
        let mut literal = fence.is_some();
        if let Some((ch, len, _, _)) = fence {
            let count = body.bytes().take_while(|b| *b == ch).count();
            if count >= len && body[count..].trim().is_empty() {
                fence = None;
            }
        }
        if !literal && !body.is_empty() {
            if let Some((content, kind, number)) = marker(body) {
                while frames
                    .last()
                    .is_some_and(|f| f.depth == depth && f.indent > indent)
                {
                    frames.pop();
                }
                if frames
                    .last()
                    .is_some_and(|f| f.depth == depth && f.indent == indent && f.kind != kind)
                {
                    frames.pop();
                }
                let same = frames
                    .last()
                    .is_some_and(|f| f.depth == depth && f.indent == indent && f.kind == kind);
                let base = frames
                    .last()
                    .filter(|f| f.depth == depth)
                    .map_or(0, |f| f.content);
                if same || (indent >= base && indent < base + 4) {
                    if !same {
                        frames.push(Frame {
                            indent,
                            content: indent + content,
                            depth,
                            kind,
                            id: next_id,
                            last: None,
                        });
                        next_id += 1;
                    }
                    let frame = frames.last_mut().unwrap();
                    frame.content = indent + content;
                    frame.last = number.map(|value| {
                        let ix = items.len();
                        let digits = body.bytes().take_while(u8::is_ascii_digit).count();
                        items.push(NumericItem {
                            source: offset..offset + line.len(),
                            digits: offset + start..offset + start + digits,
                            value,
                            list: frame.id,
                            content_column: frame.content,
                        });
                        ix
                    });
                    let remainder = &body[content..];
                    if let Some(ch @ (b'`' | b'~')) = remainder.as_bytes().first().copied() {
                        let count = remainder.bytes().take_while(|b| *b == ch).count();
                        if count >= 3 {
                            fence = Some((ch, count, depth, frame.content));
                            literal = true;
                        }
                    }
                }
            } else {
                while frames.last().is_some_and(|f| {
                    f.depth == depth
                        && indent < f.content
                        && (blank || body.starts_with(['#', '>']))
                }) {
                    frames.pop();
                }
                let base = frames
                    .last()
                    .filter(|f| f.depth == depth)
                    .map_or(0, |f| f.content);
                if indent < base + 4 {
                    if let Some(ch @ (b'`' | b'~')) = body.as_bytes().first().copied() {
                        let count = body.bytes().take_while(|b| *b == ch).count();
                        if count >= 3 {
                            fence = Some((ch, count, depth, base));
                            literal = true;
                        }
                    }
                }
            }
        }
        for frame in &frames {
            if let Some(ix) = frame.last {
                items[ix].source.end = offset + line.len();
            }
        }
        blank = body.is_empty() && !literal;
        offset += line.len();
    }
    items
}

pub(super) fn apply(
    source: &str,
    mut edits: Vec<(Range<usize>, String)>,
    points: &mut [usize],
) -> String {
    edits.sort_by_key(|(range, _)| range.start);
    let mut result = source.to_owned();
    for (range, text) in edits.into_iter().rev() {
        for point in points.iter_mut() {
            if *point >= range.end {
                *point = point.saturating_add_signed(text.len() as isize - range.len() as isize);
            } else if *point > range.start {
                *point = range.start + (*point - range.start).min(text.len());
            }
        }
        result.replace_range(range, &text);
    }
    result
}

pub(super) fn renumber(
    source: &str,
    at: usize,
    start: usize,
    points: &mut [usize],
) -> Option<String> {
    let items = scan(source);
    let group = items
        .iter()
        .rev()
        .find(|i| i.source.contains(&at) || i.digits.start == at)?
        .list;
    let mut edits = Vec::new();
    for (index, item) in items.iter().filter(|i| i.list == group).enumerate() {
        let value = start.checked_add(index)?;
        if value > 999_999_999 {
            return None;
        }
        let digits = value.to_string();
        let delta = digits.len() as isize - item.digits.len() as isize;
        if delta != 0 {
            let mut offset = item.source.start;
            for (row, line) in source[item.source.clone()]
                .split_inclusive('\n')
                .enumerate()
            {
                let (quote, content, _) = prefix(line);
                let indent = columns(&line[quote..content]);
                if row > 0 && !line[content..].trim().is_empty() && indent >= item.content_column {
                    edits.push((
                        offset + quote..offset + content,
                        " ".repeat(indent.saturating_add_signed(delta)),
                    ));
                }
                offset += line.len();
            }
        }
        if source[item.digits.clone()] != digits {
            edits.push((item.digits.clone(), digits));
        }
    }
    Some(apply(source, edits, points))
}

pub(super) fn enter(source: &str, cursor: usize) -> Option<TaskEdit> {
    let items = scan(source);
    let row_start = source[..cursor].rfind('\n').map_or(0, |i| i + 1);
    let item = items.iter().find(|i| i.source.start == row_start)?;
    let first = items.iter().find(|i| i.list == item.list)?;
    let end = source[row_start..]
        .find('\n')
        .map_or(source.len(), |i| row_start + i);
    let line = source[row_start..end].trim_end_matches('\r');
    let (range, insertion) = super::live_preview::list_enter(line, cursor - row_start)?;
    let insertion = if source.contains("\r\n") {
        insertion.replace('\n', "\r\n")
    } else {
        insertion
    };
    let mut result = source.to_owned();
    result.replace_range(row_start + range.start..row_start + range.end, &insertion);
    let mut caret = [row_start + range.start + insertion.len()];
    if !insertion.is_empty() && insertion.contains('\n') {
        result = renumber(&result, item.digits.start, first.value, &mut caret)?;
    }
    Some(super::list_reorder::minimal_edit(
        source,
        &result,
        caret[0]..caret[0],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn inserted(source: &str, cursor: usize) -> (String, usize) {
        let edit = enter(source, cursor).unwrap();
        let mut text = source.to_owned();
        text.replace_range(edit.range, &edit.text);
        (text, edit.selection.start)
    }
    #[test]
    fn enter_renumbers_siblings_and_preserves_caret() {
        let (text, caret) = inserted("5. café word\n6. two\n7. three\n", 8);
        assert_eq!(text, "5. café\n6.  word\n7. two\n8. three\n");
        assert_eq!(&text[..caret], "5. café\n6. ");
    }
    #[test]
    fn independent_lists_nested_content_and_width_changes() {
        let source =
            "8. a\n9. b\n   3. child\n   continuation\n10. c\n\nparagraph\n\n5) separate\n";
        let (text, _) = inserted(source, 4);
        assert_eq!(
            text,
            "8. a\n9. \n10. b\n    3. child\n    continuation\n11. c\n\nparagraph\n\n5) separate\n"
        );
        let (text, _) = inserted("5) a\r\n6) b", 4);
        assert_eq!(text, "5) a\r\n6) \r\n7) b");
    }
    #[test]
    fn literals_and_quote_boundaries() {
        for source in [
            "```\n1. code\n```",
            "    1. code\n",
            "- ```\n  1. code\n  ```\n",
        ] {
            assert!(scan(source).is_empty(), "{source}");
        }
        let source = "> 5. a\n> 6. b\n\n> 2. other\n";
        let (text, _) = inserted(source, 6);
        assert_eq!(text, "> 5. a\n> 6. \n> 7. b\n\n> 2. other\n");
        assert_eq!(inserted("1. \n2. b", 3).0, "\n2. b");
    }
}
