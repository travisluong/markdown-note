use std::ops::Range;

/// A run of displayed UTF-8 bytes and the original Markdown bytes it represents.
#[derive(Clone, Debug, PartialEq)]
pub struct SourceRun {
    pub source: Range<usize>,
    pub display: Range<usize>,
}

/// Map a parser leaf back to its source, accounting for removed escapes, container
/// prefixes, and normalized whitespace. Callers exclude code delimiters first.
pub fn map_leaf(source: &str, range: Range<usize>, display: &str) -> Vec<SourceRun> {
    let Some(raw) = source.get(range.clone()) else {
        return Vec::new();
    };
    let mut runs: Vec<SourceRun> = Vec::new();
    let mut cursor = 0;
    let mut index = 0;
    while cursor < raw.len() && index < display.len() {
        // Character references may expand to more than one Unicode character.
        // Let the same Markdown parser decode them; retain the entire token's
        // source extent rather than guessing offsets in its name or digits.
        if raw[cursor..].starts_with('&')
            && let Some(end) = raw.as_bytes()[cursor..]
                .iter()
                .take(33)
                .position(|byte| *byte == b';')
        {
            let token = &raw[cursor..cursor + end + 1];
            if !display[index..].starts_with(token)
                && let Ok(markdown::mdast::Node::Root(root)) =
                    markdown::to_mdast(token, &markdown::ParseOptions::gfm())
                && let Some(markdown::mdast::Node::Paragraph(p)) = root.children.first()
                && let Some(markdown::mdast::Node::Text(text)) = p.children.first()
                && text.value != token
                && display[index..].starts_with(&text.value)
            {
                runs.push(SourceRun {
                    source: range.start + cursor..range.start + cursor + token.len(),
                    display: index..index + text.value.len(),
                });
                cursor += token.len();
                index += text.value.len();
                continue;
            }
        }
        let ch = display[index..].chars().next().unwrap();
        let raw_ch = raw[cursor..].chars().next().unwrap();
        if raw_ch != ch && !(ch == ' ' && matches!(raw_ch, '\n' | '\r')) {
            cursor += raw_ch.len_utf8();
            continue;
        }
        let run = SourceRun {
            source: range.start + cursor..range.start + cursor + raw_ch.len_utf8(),
            display: index..index + ch.len_utf8(),
        };
        if let Some(previous) = runs.last_mut().filter(|previous| {
            previous.source.end == run.source.start
                && previous.display.end == run.display.start
                && previous.source.len() == previous.display.len()
                && run.source.len() == run.display.len()
        }) {
            previous.source.end = run.source.end;
            previous.display.end = run.display.end;
        } else {
            runs.push(run);
        }
        cursor += raw_ch.len_utf8();
        index += ch.len_utf8();
    }
    runs
}

pub fn project(runs: &[SourceRun], range: &Range<usize>) -> Vec<Range<usize>> {
    let mut result: Vec<Range<usize>> = Vec::new();
    for run in runs {
        let start = run.source.start.max(range.start);
        let end = run.source.end.min(range.end);
        if start >= end {
            continue;
        }
        let displayed = if run.source.len() == run.display.len() {
            run.display.start + start - run.source.start..run.display.start + end - run.source.start
        } else {
            run.display.clone()
        };
        if let Some(previous) = result
            .last_mut()
            .filter(|previous| previous.end == displayed.start)
        {
            previous.end = displayed.end;
        } else {
            result.push(displayed);
        }
    }
    result
}

pub fn containing_block(blocks: &[Range<usize>], range: &Range<usize>) -> Option<usize> {
    blocks
        .iter()
        .position(|block| block.start <= range.start && range.start < block.end)
        .or_else(|| blocks.iter().position(|block| block.start >= range.start))
        .or_else(|| blocks.len().checked_sub(1))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unicode_and_removed_escapes_preserve_offsets() {
        let source = "xx 你好 \\*literal\\*";
        let runs = map_leaf(source, 3..source.len(), "你好 *literal*");
        let start = source.find("literal").unwrap();
        assert_eq!(project(&runs, &(start..start + 7)), vec![8..15]);
        assert_eq!(project(&runs, &(3..9)), vec![0..6]);
        assert!(project(&runs, &(10..11)).is_empty());
    }
    #[test]
    fn offscreen_and_hidden_syntax_choose_source_block() {
        let blocks = vec![0..10, 12..40, 45..100];
        assert_eq!(containing_block(&blocks, &(85..89)), Some(2));
        assert_eq!(containing_block(&blocks, &(10..12)), Some(1));
        assert_eq!(containing_block(&[], &(0..1)), None);
    }
}
