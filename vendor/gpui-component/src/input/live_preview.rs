//! Deliberately line-local Markdown presentation. All offsets are UTF-8 bytes.
use std::ops::Range;

#[derive(Clone, Debug, Default)]
pub(super) struct Presentation {
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
        let digits = body.bytes().take_while(u8::is_ascii_digit).count();
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
    if line.starts_with("- ") {
        return Some((2, "- ".into()));
    }
    let digits = line.bytes().take_while(u8::is_ascii_digit).count();
    if digits > 0 && line[digits..].starts_with(". ") {
        let mut next = line[..digits].as_bytes().to_vec();
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
        return Some((digits + 2, format!("{next}. ")));
    }
    None
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
        let mut next = line[..end].to_string();
        next.replace_range(state..state + 1, " ");
        return Some((cursor..cursor, format!("\n{next} ")));
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
    pub fn parse(line: &str) -> Self {
        PARSED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let quote_prefixes = quote_prefixes(line);
        let prefix = quote_prefixes.last().map_or(0, |r| r.end);
        let line = &line[prefix..];
        let hashes = line.bytes().take_while(|b| *b == b'#').count();
        let mut result = Self {
            heading: if hashes <= 6 { hashes } else { 0 },
            bullet: line.starts_with("- "),
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
        let mut i = result.task.map_or(0, |state| state + 2);
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
            for pos in ix..ix + ch.len_utf8() {
                source_to_display[pos] = start;
            }
            if !hidden {
                text.push(if self.task == Some(ix) {
                    '\u{fffc}'
                } else if ix == self.quote_prefixes.last().map_or(0, |r| r.end) && self.bullet {
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

#[derive(Clone, Debug)]
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
        assert!(marker(" 1. x").is_none());
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
}
static PARSED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static SHAPED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
pub fn editor_work() -> EditorWork {
    use std::sync::atomic::Ordering::Relaxed;
    EditorWork {
        parsed_lines: PARSED.load(Relaxed),
        shaped_bytes: SHAPED.load(Relaxed),
    }
}
pub(super) fn record_shaped(bytes: usize) {
    SHAPED.fetch_add(bytes as u64, std::sync::atomic::Ordering::Relaxed);
}

/// A single buffer replacement and the mapped, forward selection range.
pub(super) struct TaskEdit {
    pub range: Range<usize>,
    pub text: String,
    pub selection: Range<usize>,
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
