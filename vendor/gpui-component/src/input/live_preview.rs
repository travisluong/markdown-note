//! Deliberately line-local Markdown presentation. All offsets are UTF-8 bytes.
use std::ops::Range;

#[derive(Clone, Debug, Default)]
pub(super) struct Presentation {
    pub heading: usize,
    pub spans: Vec<(Range<usize>, bool)>,
    pub hidden: Vec<Range<usize>>,
    pub bullet: bool,
    pub task: Option<usize>,
}

// Byte offset of the state character, including any explicit quote prefixes.
pub(super) fn task_state(line: &str) -> Option<usize> {
    let mut start = 0;
    while line[start..].starts_with('>') {
        start += 1;
        if line[start..].starts_with(' ') {
            start += 1;
        }
    }
    let body = &line[start..];
    let prefix = if body.starts_with("- ") || body.starts_with("* ") || body.starts_with("+ ") {
        2
    } else {
        let n = body.bytes().take_while(u8::is_ascii_digit).count();
        if n == 0 || !body[n..].starts_with(". ") {
            return None;
        }
        n + 2
    };
    let marker = body.as_bytes().get(prefix..prefix + 3)?;
    if marker[0] == b'['
        && matches!(marker[1], b' ' | b'x' | b'X')
        && marker[2] == b']'
        && body
            .as_bytes()
            .get(prefix + 3)
            .is_none_or(|c| c.is_ascii_whitespace())
    {
        Some(start + prefix + 1)
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
        let hashes = line.bytes().take_while(|b| *b == b'#').count();
        let mut result = Self {
            heading: if hashes <= 6 { hashes } else { 0 },
            bullet: line.starts_with("- "),
            task: task_state(line),
            ..Self::default()
        };
        if let Some(state) = result.task {
            result.bullet = false;
            // Retain a single mapped checkbox glyph; hide the rest of its marker.
            result.hidden.extend([0..state, state + 1..state + 2]);
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
        let mut i = 0;
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
        result.hidden.sort_by_key(|r| r.start);
        result.spans.sort_by_key(|(r, _)| r.start);
        result
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
        for (ix, ch) in source.char_indices() {
            while self.hidden.get(hidden_index).is_some_and(|r| r.end <= ix) {
                hidden_index += 1;
            }
            let hidden = !reveal
                && self
                    .hidden
                    .get(hidden_index)
                    .is_some_and(|r| r.contains(&ix));
            let start = text.len();
            for pos in ix..ix + ch.len_utf8() {
                source_to_display[pos] = start;
            }
            if !hidden {
                text.push(if !reveal && self.task == Some(ix) {
                    if ch == ' ' { '☐' } else { '☑' }
                } else if ix == 0 && self.bullet {
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
        for marker in ["- [ ]", "- [x]", "- [X]", "1. [X]"] {
            let source = format!("{marker} café");
            let p = Presentation::parse(&source);
            let mapped = p.display(&source, false);
            assert!(mapped.text.starts_with(['☐', '☑']));
            assert_eq!(mapped.display_to_source[0], p.task.unwrap());
            assert_eq!(p.display(&source, true).text, source);
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
