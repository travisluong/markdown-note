//! Pure Markdown toolbar edits. All ranges and selections are UTF-8 source bytes.
use std::{collections::HashMap, ops::Range};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Bold,
    Italic,
    Strikethrough,
    InlineCode,
    Heading(u8),
    Blockquote,
    OrderedList,
    UnorderedList,
    TaskList,
    CodeBlock,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Edit {
    pub range: Range<usize>,
    pub text: String,
    pub selection: Range<usize>,
}

pub fn transform(source: &str, selection: Range<usize>, format: Format) -> Option<Edit> {
    if selection.start > selection.end
        || selection.end > source.len()
        || !source.is_char_boundary(selection.start)
        || !source.is_char_boundary(selection.end)
    {
        return None;
    }
    Some(match format {
        Format::Bold => inline(source, selection, "**"),
        Format::Italic => inline(source, selection, "*"),
        Format::Strikethrough => inline(source, selection, "~~"),
        Format::InlineCode => code_span(source, selection),
        Format::CodeBlock => code_block(source, selection),
        Format::Heading(level) if level > 6 => return None,
        _ => block(source, selection, format),
    })
}

fn inline(source: &str, selection: Range<usize>, delimiter: &str) -> Edit {
    let selected = &source[selection.clone()];
    let n = delimiter.len();
    if selected.len() >= n * 2
        && selected.starts_with(delimiter)
        && selected.ends_with(delimiter)
        && (delimiter != "*" || star_emphasis(selected, false) && star_emphasis(selected, true))
    {
        let text = selected[n..selected.len() - n].to_owned();
        return Edit {
            selection: selection.start..selection.start + text.len(),
            range: selection,
            text,
        };
    }
    if source[..selection.start].ends_with(delimiter)
        && source[selection.end..].starts_with(delimiter)
        // A single star must not strip half of a bold delimiter.
        && (delimiter != "*"
            || (star_emphasis(&source[..selection.start], true)
                && star_emphasis(&source[selection.end..], false)))
    {
        return Edit {
            range: selection.start - n..selection.end + n,
            text: selected.to_owned(),
            selection: selection.start - n..selection.end - n,
        };
    }
    Edit {
        range: selection.clone(),
        text: format!("{delimiter}{selected}{delimiter}"),
        selection: selection.start + n..selection.end + n,
    }
}

fn star_emphasis(text: &str, reverse: bool) -> bool {
    let count = if reverse {
        text.bytes().rev().take_while(|b| *b == b'*').count()
    } else {
        text.bytes().take_while(|b| *b == b'*').count()
    };
    count % 2 == 1
}

fn longest_run(text: &str, byte: u8) -> usize {
    text.bytes()
        .fold((0, 0), |(max, run), b| {
            let run = if b == byte { run + 1 } else { 0 };
            (max.max(run), run)
        })
        .0
}

fn code_span(source: &str, selection: Range<usize>) -> Edit {
    let selected = &source[selection.clone()];
    // Accept either a selected complete span or the content selection retained by
    // this operation, including the padding required by Markdown around backticks.
    let opening = selected.bytes().take_while(|b| *b == b'`').count();
    let closing = selected.bytes().rev().take_while(|b| *b == b'`').count();
    if opening > 0
        && opening == closing
        && selected.len() >= opening * 2
        && !selected[opening..selected.len() - closing]
            .split(|ch| ch != '`')
            .any(|run| run.len() == opening)
    {
        let inner = &selected[opening..selected.len() - closing];
        let inner = unpad_code(inner);
        return Edit {
            range: selection.clone(),
            text: inner.to_owned(),
            selection: selection.start..selection.start + inner.len(),
        };
    }
    for pad in [0, 1] {
        if pad == 1
            && (!source[..selection.start].ends_with(' ')
                || !source[selection.end..].starts_with(' '))
        {
            continue;
        }
        let before = &source[..selection.start - pad];
        let after = &source[selection.end + pad..];
        let left = before.bytes().rev().take_while(|b| *b == b'`').count();
        let right = after.bytes().take_while(|b| *b == b'`').count();
        if left > 0 && left == right && longest_run(selected, b'`') < left {
            return Edit {
                range: selection.start - pad - left..selection.end + pad + right,
                text: selected.to_owned(),
                selection: selection.start - pad - left..selection.end - pad - left,
            };
        }
    }
    let delimiter = "`".repeat(longest_run(selected, b'`') + 1);
    let padding = if selected.starts_with('`')
        || selected.ends_with('`')
        || (selected.starts_with(' ') && selected.ends_with(' ') && !selected.trim().is_empty())
    {
        " "
    } else {
        ""
    };
    let offset = delimiter.len() + padding.len();
    Edit {
        range: selection.clone(),
        text: format!("{delimiter}{padding}{selected}{padding}{delimiter}"),
        selection: selection.start + offset..selection.end + offset,
    }
}

fn unpad_code(text: &str) -> &str {
    if text.len() >= 2 && text.starts_with(' ') && text.ends_with(' ') && !text.trim().is_empty() {
        &text[1..text.len() - 1]
    } else {
        text
    }
}

fn line_range(source: &str, selection: &Range<usize>) -> Range<usize> {
    let start = source[..selection.start].rfind('\n').map_or(0, |i| i + 1);
    let end = if selection.end > selection.start && source[..selection.end].ends_with('\n') {
        selection.end
    } else {
        source[selection.end..]
            .find('\n')
            .map_or(source.len(), |i| selection.end + i + 1)
    };
    start..end
}

/// Bytes of indentation and enclosing blockquotes before the line's own syntax.
fn container(line: &str) -> usize {
    let bytes = line.as_bytes();
    let mut i = 0;
    loop {
        while bytes.get(i).is_some_and(|b| matches!(b, b' ' | b'\t')) {
            i += 1;
        }
        if bytes.get(i) != Some(&b'>') {
            return i;
        }
        i += 1;
        if bytes.get(i) == Some(&b' ') {
            i += 1;
        }
    }
}

fn heading_prefix(text: &str) -> usize {
    let n = text.bytes().take_while(|b| *b == b'#').count();
    if (1..=6).contains(&n)
        && (n == text.len() || text.as_bytes().get(n).is_some_and(u8::is_ascii_whitespace))
    {
        n + usize::from(n < text.len())
    } else {
        0
    }
}

fn list_prefix(text: &str) -> (usize, Option<Format>) {
    let bytes = text.as_bytes();
    let digits = bytes.iter().take_while(|b| b.is_ascii_digit()).count();
    let (mut end, mut kind) = if matches!(bytes.first(), Some(b'-' | b'+' | b'*')) {
        (1, Format::UnorderedList)
    } else if (1..=9).contains(&digits) && matches!(bytes.get(digits), Some(b'.' | b')')) {
        (digits + 1, Format::OrderedList)
    } else {
        return (0, None);
    };
    if !bytes.get(end).is_some_and(|b| matches!(b, b' ' | b'\t')) {
        return (0, None);
    }
    end += 1;
    if matches!(bytes.get(end..end + 3), Some(b"[ ]" | b"[x]" | b"[X]"))
        && (end + 3 == text.trim_end_matches(['\r', '\n']).len()
            || bytes
                .get(end + 3)
                .is_some_and(|b| matches!(b, b' ' | b'\t')))
    {
        end += 3;
        end += usize::from(bytes.get(end).is_some_and(|b| matches!(b, b' ' | b'\t')));
        kind = Format::TaskList;
    }
    (end, Some(kind))
}

fn block(source: &str, selection: Range<usize>, format: Format) -> Edit {
    let range = line_range(source, &selection);
    let lines: Vec<_> = if range.is_empty() {
        vec![""]
    } else {
        source[range.clone()].split_inclusive('\n').collect()
    };
    let remove = match format {
        Format::Blockquote => lines
            .iter()
            .all(|line| line[..container(line)].contains('>')),
        Format::OrderedList | Format::UnorderedList | Format::TaskList => {
            lines.iter().any(|line| !line.trim().is_empty())
                && lines
                    .iter()
                    .filter(|line| !line.trim().is_empty())
                    .all(|line| list_prefix(&line[container(line)..]).1 == Some(format))
        }
        _ => false,
    };
    let mut text = String::new();
    let mut replacements = Vec::new();
    let mut source_start = range.start;
    let mut numbers = HashMap::<String, usize>::new();
    for line in lines {
        let prefix = container(line);
        let body = &line[prefix..];
        let (at, old_len, new) = match format {
            Format::Heading(level) => (
                prefix,
                heading_prefix(body),
                if level == 0 {
                    String::new()
                } else {
                    format!("{} ", "#".repeat(level as usize))
                },
            ),
            Format::Blockquote if remove => {
                let at = line[..prefix].rfind('>').unwrap();
                (
                    at,
                    1 + usize::from(line.as_bytes().get(at + 1) == Some(&b' ')),
                    String::new(),
                )
            }
            Format::Blockquote => (prefix, 0, "> ".into()),
            _ => {
                let old = list_prefix(body).0;
                if line.trim().is_empty() && !selection.is_empty() {
                    numbers.clear();
                    text.push_str(line);
                    source_start += line.len();
                    continue;
                }
                let marker = if remove {
                    String::new()
                } else {
                    match format {
                        Format::OrderedList => {
                            let key = line[..prefix].to_owned();
                            // A new parent starts a fresh nested sibling sequence.
                            numbers.retain(|context, _| context.len() <= key.len());
                            let number = numbers.entry(key).or_default();
                            *number += 1;
                            format!("{number}. ")
                        }
                        Format::TaskList => "- [ ] ".into(),
                        _ => "- ".into(),
                    }
                };
                (prefix, old, marker)
            }
        };
        text.push_str(&line[..at]);
        text.push_str(&new);
        text.push_str(&line[at + old_len..]);
        replacements.push((source_start + at..source_start + at + old_len, new.len()));
        source_start += line.len();
    }
    let map = |offset: usize| {
        let mut delta = 0isize;
        for (old, new_len) in &replacements {
            if offset < old.start {
                break;
            }
            if offset <= old.end {
                return old.start.checked_add_signed(delta).unwrap() + new_len;
            }
            delta += *new_len as isize - old.len() as isize;
        }
        offset.checked_add_signed(delta).unwrap()
    };
    Edit {
        range,
        text,
        selection: map(selection.start)..map(selection.end),
    }
}

fn fence(line: &str) -> Option<(usize, u8, usize)> {
    let prefix = container(line);
    let body = &line[prefix..];
    let byte = *body.as_bytes().first()?;
    if !matches!(byte, b'`' | b'~') {
        return None;
    }
    let n = body.bytes().take_while(|b| *b == byte).count();
    (n >= 3).then_some((prefix, byte, n))
}

fn code_block(source: &str, selection: Range<usize>) -> Edit {
    let range = line_range(source, &selection);
    let newline = if source.contains("\r\n") {
        "\r\n"
    } else {
        "\n"
    };
    // Find a paired fence containing the selection, respecting quote containers.
    let mut opening: Option<(usize, usize, u8, usize, String)> = None;
    let mut offset = 0;
    for line in source.split_inclusive('\n') {
        if let Some((prefix, byte, n)) = fence(line) {
            if let Some((start, body_start, open_byte, open_len, context)) = &opening {
                if *open_byte == byte
                    && n >= *open_len
                    && context == &line[..prefix]
                    && line[prefix + n..].trim().is_empty()
                {
                    if selection.start >= *start && selection.end <= offset + line.len() {
                        let text = source[*body_start..offset].to_owned();
                        let map = |at: usize| start + at.clamp(*body_start, offset) - body_start;
                        return Edit {
                            range: *start..offset + line.len(),
                            text,
                            selection: map(selection.start)..map(selection.end),
                        };
                    }
                    opening = None;
                }
            } else {
                opening = Some((
                    offset,
                    offset + line.len(),
                    byte,
                    n,
                    line[..prefix].to_owned(),
                ));
            }
        }
        offset += line.len();
    }
    if selection.is_empty() {
        let line = &source[range.start..range.end];
        let prefix = &line[..container(line)];
        let leading = if selection.start == range.start {
            ""
        } else {
            newline
        };
        let suffix_prefix = if selection.start > range.start && selection.start < range.end {
            prefix
        } else {
            ""
        };
        let text = format!(
            "{leading}{prefix}```{newline}{prefix}{newline}{prefix}```{newline}{suffix_prefix}"
        );
        let caret =
            selection.start + leading.len() + prefix.len() + 3 + newline.len() + prefix.len();
        return Edit {
            range: selection,
            text,
            selection: caret..caret,
        };
    }
    let content = &source[range.clone()];
    let first = content.split('\n').next().unwrap_or("");
    let prefix = &first[..container(first)];
    let delimiter = "`".repeat(3.max(longest_run(content, b'`') + 1));
    let opening = format!("{prefix}{delimiter}{newline}");
    let separator = if content.ends_with('\n') { "" } else { newline };
    let trailing = if content.ends_with('\n') { newline } else { "" };
    Edit {
        text: format!("{opening}{content}{separator}{prefix}{delimiter}{trailing}"),
        selection: selection.start + opening.len()..selection.end + opening.len(),
        range,
    }
}

#[cfg(test)]
mod tests;
