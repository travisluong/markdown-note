//! Deliberately line-local Markdown presentation. All offsets are UTF-8 bytes.
use std::ops::Range;

#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Presentation {
    pub code: Option<crate::highlighter::code::CodeTokens>,
    pub links: Vec<super::markdown_links::MarkdownLink>,
    pub heading: usize,
    pub spans: Vec<(Range<usize>, bool)>,
    pub hidden: Vec<Range<usize>>,
    pub bullet: bool,
    pub task: Option<usize>,
    pub quote_prefixes: Vec<Range<usize>>,
    pub quote_depth: usize,
}

fn quote_prefixes(line: &str) -> Vec<Range<usize>> {
    let mut quote_prefixes = Vec::new();
    let mut prefix = 0;
    loop {
        let spaces = line[prefix..]
            .bytes()
            .take_while(|b| matches!(b, b' ' | b'\t'))
            .count();
        if !line[prefix + spaces..].starts_with('>') {
            break;
        }
        let start = prefix;
        prefix += spaces + 1;
        if line[prefix..].starts_with(' ') {
            prefix += 1;
        }
        quote_prefixes.push(start..prefix);
    }
    quote_prefixes
}

fn content_start(line: &str) -> usize {
    let start = quote_prefixes(line).last().map_or(0, |r| r.end);
    start
        + line[start..]
            .bytes()
            .take_while(|b| matches!(b, b' ' | b'\t'))
            .count()
}

fn list_content_start(line: &str, start: usize) -> Option<usize> {
    let body = &line[start..];
    let marker = if body.starts_with(['-', '*', '+']) {
        1
    } else {
        let digits = ordered_marker(body)?.0;
        if digits == 0 || !body[digits..].starts_with(['.', ')']) {
            return None;
        }
        digits + 1
    };
    let spaces = body[marker..]
        .bytes()
        .take_while(|b| matches!(b, b' ' | b'\t'))
        .count();
    (spaces > 0).then_some(start + marker + spaces)
}

fn ordered_marker(body: &str) -> Option<(usize, usize, bool)> {
    let letters = body.as_bytes().first()?.is_ascii_lowercase();
    let len = body
        .bytes()
        .take_while(|b| {
            if letters {
                b.is_ascii_lowercase()
            } else {
                b.is_ascii_digit()
            }
        })
        .count();
    if len == 0 || !body[len..].starts_with(['.', ')']) {
        return None;
    }
    let mut value = 0usize;
    for byte in body[..len].bytes() {
        value = value
            .checked_mul(if letters { 26 } else { 10 })?
            .checked_add(if letters {
                (byte - b'a' + 1) as usize
            } else {
                (byte - b'0') as usize
            })?;
    }
    Some((len, value, letters))
}

fn ordinal(mut value: usize, letters: bool) -> String {
    if !letters {
        return value.to_string();
    }
    let mut result = Vec::new();
    while value > 0 {
        value -= 1;
        result.push(b'a' + (value % 26) as u8);
        value /= 26;
    }
    result.reverse();
    String::from_utf8(result).unwrap()
}

// Byte offset of the state character, including any explicit quote prefixes.
pub(super) fn task_state(line: &str) -> Option<usize> {
    let body = line;
    let prefix = list_content_start(line, content_start(line))?;
    let marker = body.as_bytes().get(prefix..prefix + 3)?;
    if marker[0] == b'['
        && matches!(marker[1], b' ' | b'x' | b'X')
        && marker[2] == b']'
        && body
            .as_bytes()
            .get(prefix + 3)
            .is_none_or(|c| c.is_ascii_whitespace())
    {
        Some(prefix + 1)
    } else {
        None
    }
}

pub(super) fn marker(line: &str) -> Option<(usize, String)> {
    let start = content_start(line);
    let end = list_content_start(line, start)?;
    let body = &line[start..];
    let digits = body.bytes().take_while(u8::is_ascii_digit).count();
    let next_marker = if digits > 0 {
        let mut next = body.as_bytes()[..digits].to_vec();
        let mut carry = true;
        for digit in next.iter_mut().rev() {
            if *digit == b'9' {
                *digit = b'0';
            } else {
                *digit += 1;
                carry = false;
                break;
            }
        }
        if carry {
            next.insert(0, b'1');
        }
        let next = String::from_utf8(next).ok()?;
        let next = next.trim_start_matches('0');
        format!("{next}{}", &body[digits..end - start])
    } else if let Some((len, value, true)) = ordered_marker(body) {
        format!(
            "{}{}",
            ordinal(value.saturating_add(1), true),
            &body[len..end - start]
        )
    } else {
        line[start..end].to_owned()
    };
    Some((end, format!("{}{next_marker}", &line[..start])))
}

pub(super) fn list_enter(line: &str, cursor: usize) -> Option<(Range<usize>, String)> {
    if let Some(state) = task_state(line) {
        let end = state + 2;
        if cursor < end {
            return None;
        }
        if line[end..].trim().is_empty() {
            return Some((0..line.len(), "\n".into()));
        }
        let (_, next) = marker(line)?;
        return Some((cursor..cursor, format!("\n{next}[ ] ")));
    }
    let (prefix, next) = marker(line)?;
    if line[prefix..].trim().is_empty() {
        return Some((0..line.len(), String::new()));
    }
    if cursor < prefix {
        return None;
    }
    Some((cursor..cursor, format!("\n{next}")))
}

impl Presentation {
    pub fn literal(
        source: &str,
        prefix: usize,
        tokens: crate::highlighter::code::CodeTokens,
    ) -> Self {
        let prefix = prefix.min(source.len());
        let quotes = quote_prefixes(&source[..prefix]);
        Self {
            code: Some(tokens),
            hidden: if prefix > 0 {
                std::iter::once(0..prefix).collect()
            } else {
                Vec::new()
            },
            quote_depth: quotes.len(),
            quote_prefixes: quotes,
            ..Default::default()
        }
    }
    pub fn parse(line: &str) -> Self {
        PARSED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let quote_prefixes = quote_prefixes(line);
        let prefix = quote_prefixes.last().map_or(0, |r| r.end);
        let line = &line[prefix..];
        let hashes = line.bytes().take_while(|b| *b == b'#').count();
        let mut result = Self {
            heading: if hashes <= 6 { hashes } else { 0 },
            bullet: list_content_start(line, content_start(line)).is_some()
                && line[content_start(line)..].starts_with(['-', '*', '+']),
            task: task_state(line),
            ..Self::default()
        };
        if let Some(state) = result.task {
            result.bullet = false;
            // Retain a mapped fixed-width object; hide the rest of its marker.
            result
                .hidden
                .extend([content_start(line)..state, state + 1..state + 2]);
        }
        if result.heading > 0 {
            let separator = line[hashes..]
                .bytes()
                .take_while(|b| *b == b' ' || *b == b'\t')
                .count();
            result.hidden.push(0..hashes + separator);
        }
        let bytes = line.as_bytes();
        let mut openings = [None; 4];
        let mut i = result.task.map_or_else(
            || list_content_start(line, content_start(line)).unwrap_or(0),
            |state| state + 2,
        );
        while i < bytes.len() {
            if bytes[i] != b'*' && bytes[i] != b'_' {
                i += 1;
                continue;
            }
            let symbol = bytes[i];
            let n = bytes[i..].iter().take_while(|b| **b == symbol).count();
            if n <= 2 {
                let key = usize::from(symbol == b'_') * 2 + n - 1;
                if let Some(start) = openings[key].take() {
                    if i > start + n {
                        result.spans.push((start..i + n, n == 2));
                        result.hidden.extend([start..start + n, i..i + n]);
                    }
                } else {
                    openings[key] = Some(i);
                }
            }
            i += n;
        }
        for range in &mut result.hidden {
            range.start += prefix;
            range.end += prefix;
        }
        for (range, _) in &mut result.spans {
            range.start += prefix;
            range.end += prefix;
        }
        result.task = result.task.map(|offset| offset + prefix);
        result.quote_depth = quote_prefixes.len();
        result.hidden.extend(quote_prefixes.iter().cloned());
        result.quote_prefixes = quote_prefixes;
        result.hidden.sort_by_key(|r| r.start);
        result.spans.sort_by_key(|(r, _)| r.start);
        result
    }

    pub fn with_links(
        mut self,
        links: &[super::markdown_links::MarkdownLink],
        start: usize,
        len: usize,
    ) -> Self {
        for link in links
            .iter()
            .filter(|link| link.source.start < start + len && link.source.end > start)
        {
            let local = |r: &Range<usize>| {
                r.start.saturating_sub(start).min(len)..r.end.saturating_sub(start).min(len)
            };
            // Formatting markers inside a destination are literal URL characters.
            self.hidden.retain(|r| {
                !local(&link.source).contains(&r.start) || local(&link.label).contains(&r.start)
            });
            self.spans.retain(|(r, _)| {
                !local(&link.source).contains(&r.start) || local(&link.label).contains(&r.start)
            });
            self.hidden
                .push(local(&(link.source.start..link.label.start)));
            self.hidden.push(local(&(link.label.end..link.source.end)));
            let mut link = link.clone();
            link.source = local(&link.source);
            link.label = local(&link.label);
            self.links.push(link);
        }
        self.hidden.sort_by_key(|r| r.start);
        let mut merged: Vec<Range<usize>> = Vec::new();
        for range in self.hidden.drain(..).filter(|r| !r.is_empty()) {
            if let Some(last) = merged.last_mut().filter(|last| last.end >= range.start) {
                last.end = last.end.max(range.end);
            } else {
                merged.push(range);
            }
        }
        self.hidden = merged;
        self
    }

    pub fn quote_indent(&self, font_size: f32) -> f32 {
        self.quote_depth as f32 * font_size
    }

    pub fn scale(&self) -> f32 {
        match self.heading {
            1 => 2.,
            2 => 1.5,
            3 => 1.25,
            4 => 1.125,
            _ => 1.,
        }
    }

    pub fn display(&self, source: &str, reveal: bool) -> MappedText {
        let mut text = String::new();
        let mut source_to_display = vec![0; source.len() + 1];
        let mut display_to_source = vec![0];
        let mut hidden_index = 0;
        let marker_start = content_start(source);
        let task_start = self.task.map(|_| content_start(source));
        for (ix, ch) in source.char_indices() {
            while self.hidden.get(hidden_index).is_some_and(|r| r.end <= ix) {
                hidden_index += 1;
            }
            let task_marker = self
                .task
                .is_some_and(|state| (task_start.unwrap()..state).contains(&ix) || ix == state + 1);
            let hidden = task_marker
                || !reveal
                    && self
                        .hidden
                        .get(hidden_index)
                        .is_some_and(|r| r.contains(&ix));
            let start = text.len();
            source_to_display[ix..ix + ch.len_utf8()].fill(start);
            if !hidden {
                text.push(if self.task == Some(ix) {
                    '\u{fffc}'
                } else if ix == marker_start && self.bullet {
                    '•'
                } else {
                    ch
                });
            }
            for _ in start..text.len() {
                display_to_source.push(ix);
            }
            *display_to_source.last_mut().unwrap() = ix + ch.len_utf8();
        }
        source_to_display[source.len()] = text.len();
        MappedText {
            text,
            source_to_display,
            display_to_source,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct MappedText {
    pub text: String,
    pub source_to_display: Vec<usize>,
    pub display_to_source: Vec<usize>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn heading_markers_and_mapping() {
        for hashes in 1..=6 {
            for suffix in ["", " title", "\t title", "caf\u{e9}", " **bold**"] {
                let source = format!("{}{suffix}", "#".repeat(hashes));
                let p = Presentation::parse(&source);
                assert_eq!(p.display(&source, true).text, source);
                let hidden = p.display(&source, false);
                assert!(!hidden.text.starts_with('#'));
                assert!(!hidden.text.starts_with(' '));
                assert_eq!(hidden.source_to_display.len(), source.len() + 1);
                for (offset, _) in hidden.text.char_indices() {
                    let original = hidden.display_to_source[offset];
                    assert!(source.is_char_boundary(original));
                    assert_eq!(hidden.source_to_display[original], offset);
                }
            }
        }
        for source in ["####### title", "not # heading"] {
            assert_eq!(
                Presentation::parse(source).display(source, false).text,
                source
            );
        }
    }

    #[test]
    fn triggers_and_mapping() {
        for n in 1..=6 {
            assert_eq!(
                Presentation::parse(&format!("{}title", "#".repeat(n))).heading,
                n
            );
        }
        assert_eq!(Presentation::parse("#######title").heading, 0);
        assert!(marker("-x").is_none());
        assert_eq!(marker(" 1. x"), Some((4, " 2. ".into())));
        let p = Presentation::parse("- **café** _你好_");
        let m = p.display("- **café** _你好_", false);
        assert_eq!(m.text, "• café 你好");
        assert_eq!(
            p.display("- **café** _你好_", true).text,
            "• **café** _你好_"
        );
        for (i, _) in m.text.char_indices() {
            assert!("- **café** _你好_".is_char_boundary(m.display_to_source[i]));
        }
        for s in ["*oops", "__oops", "***oops***", "a_", "**"] {
            assert_eq!(Presentation::parse(s).display(s, false).text, s);
        }
    }
    #[test]
    fn list_edits() {
        let source = "> > - [X] **café** _你好_";
        let p = Presentation::parse(source);
        assert_eq!(p.quote_depth, 2);
        assert_eq!(p.quote_prefixes, vec![0..2, 2..4]);
        assert_eq!(p.quote_indent(20.), 40.);
        assert_eq!(p.display(source, false).text, "\u{fffc} café 你好");
        assert_eq!(p.display(source, true).text, "> > \u{fffc} **café** _你好_");
        assert_eq!(p.task, Some(7));
        for (source, depth, text) in [
            (">", 1, ""),
            (">>text", 2, "text"),
            ("   > quoted", 1, "quoted"),
            ("continuation", 0, "continuation"),
        ] {
            let p = Presentation::parse(source);
            assert_eq!(p.quote_depth, depth);
            assert_eq!(p.display(source, false).text, text);
        }
        for marker in ["- [ ]", "- [x]", "- [X]", "1. [X]"] {
            let source = format!("{marker} café");
            let p = Presentation::parse(&source);
            let mapped = p.display(&source, false);
            assert!(mapped.text.starts_with('\u{fffc}'));
            assert_eq!(mapped.display_to_source[0], p.task.unwrap());
            assert_eq!(p.display(&source, true).text, mapped.text);
            assert_eq!(
                list_enter(&format!("{marker} "), marker.len() + 1),
                Some((0..marker.len() + 1, "\n".into()))
            );
        }
        assert_eq!(list_enter("- hello", 4), Some((4..4, "\n- ".into())));
        assert_eq!(list_enter("99. hello", 6), Some((6..6, "\n100. ".into())));
        assert_eq!(list_enter("- \t ", 3), Some((0..4, String::new())));
        assert_eq!(list_enter("12.  ", 5), Some((0..5, String::new())));
        assert_eq!(list_enter("- hello", 0), None);
    }
}

/// Cumulative editor work counters for diagnostics and performance regression tests.
#[derive(Clone, Copy, Debug, Default)]
pub struct EditorWork {
    pub parsed_lines: u64,
    pub shaped_bytes: u64,
    pub table_rows_visited: u64,
    pub ui_copied_bytes: u64,
    pub ui_parsed_bytes: u64,
    pub cache_invalidations: u64,
}
static PARSED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static SHAPED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static TABLE_ROWS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static UI_COPIED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static UI_PARSED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static INVALIDATIONS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub fn editor_work() -> EditorWork {
    use std::sync::atomic::Ordering::Relaxed;
    EditorWork {
        parsed_lines: PARSED.load(Relaxed),
        shaped_bytes: SHAPED.load(Relaxed),
        table_rows_visited: TABLE_ROWS.load(Relaxed),
        ui_copied_bytes: UI_COPIED.load(Relaxed),
        ui_parsed_bytes: UI_PARSED.load(Relaxed),
        cache_invalidations: INVALIDATIONS.load(Relaxed),
    }
}
pub(super) fn record_shaped(bytes: usize) {
    SHAPED.fetch_add(bytes as u64, std::sync::atomic::Ordering::Relaxed);
}
pub(super) fn record_table_row() {
    TABLE_ROWS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}
pub(super) fn record_ui_work(copied: usize, parsed: usize) {
    UI_COPIED.fetch_add(copied as u64, std::sync::atomic::Ordering::Relaxed);
    UI_PARSED.fetch_add(parsed as u64, std::sync::atomic::Ordering::Relaxed);
}
pub(super) fn record_invalidation() {
    INVALIDATIONS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

/// A single buffer replacement and the mapped, forward selection range.
pub(super) struct TaskEdit {
    pub range: Range<usize>,
    pub text: String,
    pub selection: Range<usize>,
}

/// Swap logical source lines, leaving line terminators attached to their slots.
pub(super) fn move_lines(source: &str, selection: Range<usize>, down: bool) -> Option<TaskEdit> {
    if selection.end > source.len()
        || selection.start > selection.end
        || !source.is_char_boundary(selection.start)
        || !source.is_char_boundary(selection.end)
    {
        return None;
    }
    let lines: Vec<&str> = source.split_inclusive('\n').collect();
    if lines.len() < 2 {
        return None;
    }
    let mut starts = vec![0];
    for line in &lines {
        starts.push(starts.last().unwrap() + line.len());
    }
    let row = |offset: usize| {
        starts
            .partition_point(|start| *start <= offset)
            .saturating_sub(1)
            .min(lines.len() - 1)
    };
    let first = row(selection.start);
    let last = row(if selection.is_empty() {
        selection.end
    } else {
        selection.end - 1
    });
    if (!down && first == 0) || (down && last + 1 == lines.len()) {
        return None;
    }
    let lo = if down { first } else { first - 1 };
    let hi = if down { last + 1 } else { last };
    let body = |line: &str| {
        line.trim_end_matches('\n')
            .strip_suffix('\r')
            .unwrap_or_else(|| line.trim_end_matches('\n'))
            .len()
    };
    let mut order: Vec<usize> = (lo..=hi).collect();
    if down {
        order.rotate_right(1);
    } else {
        order.rotate_left(1);
    }
    let mut text = String::new();
    let mut new_starts = vec![0; lines.len()];
    for (slot, original) in (lo..=hi).zip(order) {
        new_starts[original] = starts[lo] + text.len();
        text.push_str(&lines[original][..body(lines[original])]);
        text.push_str(&lines[slot][body(lines[slot])..]);
    }
    let mapped_start =
        new_starts[first] + (selection.start - starts[first]).min(body(lines[first]));
    let end_column = selection.end - starts[last];
    let mapped_end = (new_starts[last] + end_column).min(starts[lo] + text.len());
    Some(TaskEdit {
        range: starts[lo]..starts[hi + 1],
        text,
        selection: mapped_start..mapped_end,
    })
}

#[cfg(test)]
mod move_line_tests {
    use super::*;
    fn moved(source: &str, selection: Range<usize>, down: bool) -> (String, Range<usize>) {
        let edit = move_lines(source, selection, down).unwrap();
        let mut result = source.to_owned();
        result.replace_range(edit.range, &edit.text);
        (result, edit.selection)
    }
    #[test]
    fn unicode_selection_and_final_newline() {
        assert_eq!(
            moved("a\ncafé\n茶", 3..7, true),
            ("a\n茶\ncafé".into(), 7..11)
        );
        assert_eq!(
            moved("a\nb\nc\nd\n", 2..6, false),
            ("b\nc\na\nd\n".into(), 0..4)
        );
        assert_eq!(
            moved("a\r\nb\r\nc", 3..4, true),
            ("a\r\nc\r\nb".into(), 6..7)
        );
        assert!(move_lines("a\nb", 0..0, false).is_none());
        assert!(move_lines("a\nb", 2..2, true).is_none());
        assert!(move_lines("café\nx", 4..4, true).is_none());
    }
}

/// List nesting uses source columns, excluding explicit blockquote prefixes.
/// Markdown tabs advance to the next four-column stop.
fn columns(text: &str) -> usize {
    text.bytes().fold(0, |column, b| {
        if b == b'\t' {
            column + 4 - column % 4
        } else {
            column + 1
        }
    })
}

#[cfg(test)]
pub(super) fn nest_lines(
    source: &str,
    selection: Range<usize>,
    outdent: bool,
    step: usize,
) -> Option<TaskEdit> {
    nest_lines_with_style(source, selection, outdent, step, false)
}

pub(super) fn nest_lines_with_style(
    source: &str,
    selection: Range<usize>,
    outdent: bool,
    step: usize,
    letters: bool,
) -> Option<TaskEdit> {
    struct Line<'a> {
        text: &'a str,
        offset: usize,
        prefix: usize,
        depth: usize,
        start: usize,
        indent: usize,
        content: Option<usize>,
    }
    let mut offset = 0;
    let mut fence: Option<(u8, usize)> = None;
    let lines: Vec<_> = source
        .split('\n')
        .map(|text| {
            let quotes = quote_prefixes(text);
            let prefix = quotes.last().map_or(0, |r| r.end);
            let start = content_start(text);
            let body = text[start..].trim_end_matches('\r');
            let fence_char = body.as_bytes().first().copied().unwrap_or(0);
            let fence_len = body.bytes().take_while(|b| *b == fence_char).count();
            let in_fence = fence.is_some();
            if let Some((ch, len)) = fence {
                if ch == fence_char && fence_len >= len && body[fence_len..].trim().is_empty() {
                    fence = None;
                }
            } else if matches!(fence_char, b'`' | b'~') && fence_len >= 3 {
                fence = Some((fence_char, fence_len));
            }
            let line = Line {
                text,
                offset,
                prefix,
                depth: quotes.len(),
                start,
                indent: columns(&text[prefix..start]),
                content: if in_fence || fence.is_some() {
                    None
                } else {
                    list_content_start(text, start).map(|end| columns(&text[prefix..end]))
                },
            };
            offset += text.len() + 1;
            line
        })
        .collect();
    let first = lines
        .partition_point(|line| line.offset <= selection.start)
        .saturating_sub(1);
    let last_offset =
        if selection.end > selection.start && source.as_bytes()[selection.end - 1] == b'\n' {
            selection.end - 1
        } else {
            selection.end
        };
    let last = lines
        .partition_point(|line| line.offset <= last_offset)
        .saturating_sub(1);
    let mut replacements = Vec::new();
    let mut handled = false;
    let mut targets = vec![None; lines.len()];
    let mut ordinals = vec![None; lines.len()];
    let mut row = first;
    while row <= last {
        let root = &lines[row];
        if root.content.is_none() {
            row += 1;
            continue;
        }
        handled = true;
        let mut target = if outdent {
            root.indent.saturating_sub(step.max(1))
        } else {
            root.indent + step.max(1)
        };
        for (previous_row, previous) in lines[..row].iter().enumerate().rev() {
            if previous.depth != root.depth {
                break;
            }
            if previous.text[previous.prefix..].trim().is_empty() {
                continue;
            }
            if outdent {
                if previous.indent < root.indent {
                    if previous.content.is_some() {
                        target = previous.indent;
                    }
                    break;
                }
            } else if previous.indent <= root.indent {
                if previous.indent == root.indent
                    && let Some(content) = previous.content
                {
                    // Selected siblings stay siblings even when their marker widths differ.
                    target = targets[previous_row].unwrap_or(content);
                }
                break;
            }
            if previous.content.is_none() && previous.indent <= root.indent {
                break;
            }
        }
        targets[row] = Some(target);
        // Include descendants and continuation lines, but not trailing blank lines.
        let mut end = row + 1;
        for (next, line) in lines.iter().enumerate().skip(row + 1) {
            if line.depth != root.depth {
                break;
            }
            if line.text[line.prefix..].trim().is_empty() {
                continue;
            }
            if line.indent <= root.indent {
                break;
            }
            end = next + 1;
        }
        if target != root.indent {
            let marker = ordered_marker(&root.text[root.start..]).map(|(len, _, _)| {
                let mut value = 1;
                for (i, previous) in lines[..row].iter().enumerate().rev() {
                    if previous.depth != root.depth {
                        break;
                    }
                    if previous.text.trim().is_empty() {
                        continue;
                    }
                    let indent = targets[i].unwrap_or(previous.indent);
                    if indent < target {
                        break;
                    }
                    if indent == target {
                        if let Some(n) = ordinals[i].or_else(|| {
                            ordered_marker(&previous.text[previous.start..]).map(|m| m.1)
                        }) {
                            value = n.saturating_add(1);
                        }
                        break;
                    }
                }
                ordinals[row] = Some(value);
                (len, ordinal(value, letters && target > 0))
            });
            for line in &lines[row..end] {
                if line.text[line.prefix..].trim().is_empty() {
                    continue;
                }
                let new_indent = if target > root.indent {
                    line.indent + target - root.indent
                } else {
                    line.indent.saturating_sub(root.indent - target)
                };
                replacements.push((
                    line.offset + line.prefix..line.offset + line.start,
                    " ".repeat(new_indent),
                ));
                if line.offset == root.offset
                    && let Some((len, ref marker)) = marker
                {
                    replacements.push((
                        line.offset + line.start..line.offset + line.start + len,
                        marker.clone(),
                    ));
                }
            }
        }
        row = end;
    }
    if !handled {
        return None;
    }
    let map = |position: usize| {
        let mut shift = 0isize;
        for (range, text) in &replacements {
            if position < range.start {
                break;
            }
            if position < range.end {
                return (range.start as isize + shift) as usize
                    + (position - range.start).min(text.len());
            }
            shift += text.len() as isize - range.len() as isize;
        }
        (position as isize + shift) as usize
    };
    let mapped = map(selection.start)..map(selection.end);
    let start = replacements
        .first()
        .map_or(selection.start, |(range, _)| range.start);
    let end = replacements.last().map_or(start, |(range, _)| range.end);
    let mut text = String::new();
    let mut from = start;
    for (range, replacement) in replacements {
        text.push_str(&source[from..range.start]);
        text.push_str(&replacement);
        from = range.end;
    }
    text.push_str(&source[from..end]);
    Some(TaskEdit {
        range: start..end,
        text,
        selection: mapped,
    })
}

pub(super) fn toggle_lines(source: &str, selection: Range<usize>) -> TaskEdit {
    let start = source[..selection.start].rfind('\n').map_or(0, |i| i + 1);
    let last = if selection.end > selection.start && source.as_bytes()[selection.end - 1] == b'\n' {
        selection.end - 1
    } else {
        selection.end
    };
    let end = source[last..].find('\n').map_or(source.len(), |i| last + i);
    let mut replacements = Vec::new();
    let mut offset = start;
    for line in source[start..end].split('\n') {
        if let Some(state) = task_state(line) {
            replacements.push((
                offset + state..offset + state + 1,
                if line.as_bytes()[state] == b' ' {
                    "x"
                } else {
                    " "
                },
            ));
        } else {
            let content = content_start(line);
            let (at, marker) =
                list_content_start(line, content).map_or((content, "- [ ] "), |at| (at, "[ ] "));
            replacements.push((offset + at..offset + at, marker));
        }
        offset += line.len() + 1;
    }
    let map = |position: usize| {
        position
            + replacements
                .iter()
                .filter(|(range, _)| range.start <= position)
                .map(|(range, text)| text.len() - range.len())
                .sum::<usize>()
    };
    let mapped = map(selection.start)..map(selection.end);
    let mut text = String::new();
    let mut from = start;
    for (range, replacement) in replacements {
        text.push_str(&source[from..range.start]);
        text.push_str(replacement);
        from = range.end;
    }
    text.push_str(&source[from..end]);
    TaskEdit {
        range: start..end,
        text,
        selection: mapped,
    }
}

#[cfg(test)]
mod task_tests {
    use super::*;
    fn apply(source: &str, selection: Range<usize>) -> (String, Range<usize>) {
        let edit = toggle_lines(source, selection);
        let mut result = source.to_owned();
        result.replace_range(edit.range, &edit.text);
        (result, edit.selection)
    }
    #[test]
    fn mixed_lines_and_prefixes() {
        let source = "  plain\n> > - [X] café\n\t12. ordered\n  > + bullet\n\n- [ ] 你好\n";
        let (result, selection) = apply(source, 0..source.len());
        assert_eq!(
            result,
            "  - [ ] plain\n> > - [ ] café\n\t12. [ ] ordered\n  > + [ ] bullet\n- [ ] \n- [x] 你好\n"
        );
        assert_eq!(selection.end, result.len());
        assert!(result.is_char_boundary(selection.start));
        for line in result.lines() {
            assert!(task_state(line).is_some(), "{line}");
        }
    }
    #[test]
    fn caret_empty_lines_and_final_boundary() {
        assert_eq!(apply("", 0..0), ("- [ ] ".into(), 6..6));
        assert_eq!(apply("abc\n", 4..4), ("abc\n- [ ] ".into(), 10..10));
        assert_eq!(apply("abc\ndef", 0..4), ("- [ ] abc\ndef".into(), 6..10));
        assert_eq!(
            apply("abc\ndef", 2..5),
            ("- [ ] abc\n- [ ] def".into(), 8..17)
        );
        assert_eq!(apply("你好 café", 3..3), ("- [ ] 你好 café".into(), 9..9));
        assert_eq!(apply("  > 9) x", 8..8).0, "  > 9) [ ] x");
        assert_eq!(apply("\t> >\t*\ttext", 0..0).0, "\t> >\t*\t[ ] text");
    }
    #[test]
    fn independent_toggle_and_preserved_indentation() {
        let source = "  - [ ] a\n  - [x] b\n>  2) [X] c";
        assert_eq!(
            apply(source, 0..source.len()).0,
            "  - [x] a\n  - [ ] b\n>  2) [ ] c"
        );
        let parsed = Presentation::parse("  - [ ] café");
        assert_eq!(parsed.display("  - [ ] café", true).text, "  \u{fffc} café");
        assert_eq!(task_state("  - [bad]"), None);
    }
}

#[cfg(test)]
mod nesting_tests {
    use super::*;

    fn nest(source: &str, selection: Range<usize>, outdent: bool) -> (String, Range<usize>) {
        let edit = nest_lines(source, selection, outdent, 2).unwrap();
        let mut result = source.to_owned();
        result.replace_range(edit.range, &edit.text);
        (result, edit.selection)
    }

    #[test]
    fn ordered_levels_restart_and_continue() {
        for (source, expected) in [
            ("7. parent\n8. child", "7. parent\n   1. child"),
            (
                "7. parent\n   1. child\n8. next",
                "7. parent\n   1. child\n   2. next",
            ),
            ("7. parent\n   2. child", "7. parent\n   2. child"),
        ] {
            let at = source.len();
            if source.ends_with("2. child") {
                continue;
            }
            assert_eq!(nest(source, at..at, false).0, expected);
        }
        let source = "9. parent\n10. café\n11. next";
        let start = source.find("café").unwrap();
        let edit = nest_lines_with_style(source, start..source.len(), false, 2, true).unwrap();
        let mut result = source.to_owned();
        result.replace_range(edit.range, &edit.text);
        assert_eq!(result, "9. parent\n   a. café\n   b. next");
        assert_eq!(&result[edit.selection], "café\n   b. next");
        assert_eq!(list_enter("   z. end", 9), Some((9..9, "\n   aa. ".into())));
        let at = result.find("café").unwrap();
        assert_eq!(
            nest(&result, at..at, true).0,
            "9. parent\n10. café\n   b. next"
        );
    }
    #[test]
    fn marker_width_and_subtrees() {
        for parent in [
            "- parent",
            "* parent",
            "+ parent",
            "9. parent",
            "10) parent",
            "- [X] parent",
        ] {
            let source = format!("{parent}\n- café\n  1. child\n     continuation\n- sibling");
            let at = source.find("café").unwrap() + "café".len();
            let width = list_content_start(parent, 0).unwrap();
            let spaces = " ".repeat(width);
            let (result, selection) = nest(&source, at..at, false);
            assert_eq!(
                result,
                format!(
                    "{parent}\n{spaces}- café\n{spaces}  1. child\n{spaces}     continuation\n- sibling"
                )
            );
            assert_eq!(selection, at + width..at + width);
            assert_eq!(nest(&result, selection, true).0, source);
        }
    }

    #[test]
    fn repeated_steps_and_top_level() {
        let (one, caret) = nest("- [x] task", 10..10, false);
        assert_eq!(one, "  - [x] task");
        let (two, caret) = nest(&one, caret, false);
        assert_eq!(two, "    - [x] task");
        let (one_again, caret) = nest(&two, caret, true);
        assert_eq!(one_again, one);
        assert_eq!(nest(&one_again, caret, true).0, "- [x] task");
        assert_eq!(nest("- top", 0..0, true), ("- top".into(), 0..0));
        assert_eq!(nest(" - odd", 0..0, true), ("- odd".into(), 0..0));
    }

    #[test]
    fn selections_move_each_subtree_once() {
        let source = "10. first\n11. second\n    - child";
        assert_eq!(
            nest(source, 0..source.len(), false).0,
            "  1. first\n  2. second\n      - child"
        );
        let source = "- parent\n- one\n  - child\n- two\n- untouched";
        let start = source.find("one").unwrap();
        let end = source.find("- untouched").unwrap();
        let (result, selection) = nest(source, start..end, false);
        assert_eq!(
            result,
            "- parent\n  - one\n    - child\n  - two\n- untouched"
        );
        assert_eq!(selection, start + 2..end + 6);
        assert_eq!(nest(&result, selection, true).0, source);
        let source = "plain\n- one\nplain\n- two";
        assert_eq!(
            nest(source, 0..source.len(), false).0,
            "plain\n  - one\nplain\n  - two"
        );
    }

    #[test]
    fn quotes_tabs_crlf_and_block_boundaries() {
        let source = "> > 10. parent\r\n> > - [X] 你好\r\n> >   continuation\r\n> outside\r\n- end";
        let at = source.find("你好").unwrap();
        let (result, caret) = nest(source, at..at, false);
        assert_eq!(
            result,
            "> > 10. parent\r\n> >     - [X] 你好\r\n> >       continuation\r\n> outside\r\n- end"
        );
        assert_eq!(nest(&result, caret, true).0, source);
        assert_eq!(nest("\t- task", 7..7, false).0, "      - task");
        assert_eq!(
            nest("- a\n\n  continuation\n\nparagraph\n- next", 3..3, false).0,
            "  - a\n\n    continuation\n\nparagraph\n- next"
        );
        let source = "```md\n- code\n```\nplain";
        assert!(nest_lines(source, 8..8, false, 2).is_none());
        assert!(nest_lines("plain", 2..2, false, 2).is_none());
    }

    #[test]
    fn nested_enter_and_preview_mappings() {
        let source = "  * *italic*";
        assert_eq!(
            Presentation::parse(source).display(source, false).text,
            "  • italic"
        );
        for (source, next) in [
            ("  * café", "\n  * "),
            ("> >   + 你好", "\n> >   + "),
            ("    99) text", "\n    100) "),
            (">   9. [X] text", "\n>   10. [ ] "),
            ("  - [x] text", "\n  - [ ] "),
        ] {
            assert_eq!(
                list_enter(source, source.len()),
                Some((source.len()..source.len(), next.into()))
            );
        }
        for source in [
            "  - café",
            "    * 你好",
            "> >   + **café**",
            "    - [x] 你好",
        ] {
            let p = Presentation::parse(source);
            let mapped = p.display(source, false);
            assert!(!mapped.text.contains(['-', '*', '+']));
            for (offset, _) in mapped.text.char_indices() {
                let source_offset = mapped.display_to_source[offset];
                assert!(source.is_char_boundary(source_offset));
                assert_eq!(mapped.source_to_display[source_offset], offset);
            }
        }
    }
}

#[cfg(test)]
mod link_tests {
    use super::*;
    #[test]
    fn link_syntax_reveal_and_utf8_mappings() {
        for (source, expected) in [
            ("[café](https://example.com/a_b_c \"title\")", "café"),
            ("> [**你好**](other.md)", "你好"),
            (
                "prefix <https://example.org> suffix",
                "prefix https://example.org suffix",
            ),
            ("[one][id]\n\n[id]: note.md", "one"),
        ] {
            let line = source.lines().next().unwrap();
            let p = Presentation::parse(line).with_links(
                &super::super::markdown_links::parse(source),
                0,
                line.len(),
            );
            let mapped = p.display(line, false);
            assert_eq!(mapped.text, expected);
            assert_eq!(p.display(line, true).text, line);
            for (i, _) in mapped.text.char_indices() {
                let offset = mapped.display_to_source[i];
                assert!(line.is_char_boundary(offset));
                assert_eq!(mapped.source_to_display[offset], i);
            }
        }
    }
}
