use super::*;

fn apply(source: &str, selection: Range<usize>, format: Format) -> (String, Range<usize>) {
    let edit = transform(source, selection, format).unwrap();
    let mut result = source.to_owned();
    result.replace_range(edit.range, &edit.text);
    assert!(result.is_char_boundary(edit.selection.start));
    assert!(result.is_char_boundary(edit.selection.end));
    (result, edit.selection)
}

#[test]
fn inline_toggles_unicode_selection_and_empty_caret() {
    for (format, delimiter) in [
        (Format::Bold, "**"),
        (Format::Italic, "*"),
        (Format::Strikethrough, "~~"),
        (Format::InlineCode, "`"),
    ] {
        let (text, selection) = apply("café 茶", 0..9, format);
        assert_eq!(text, format!("{delimiter}café 茶{delimiter}"));
        assert_eq!(&text[selection.clone()], "café 茶");
        assert_eq!(apply(&text, selection, format), ("café 茶".into(), 0..9));
        assert_eq!(
            apply(&text, 0..text.len(), format),
            ("café 茶".into(), 0..9)
        );
        let (empty, caret) = apply("", 0..0, format);
        assert_eq!(empty, delimiter.repeat(2));
        assert_eq!(caret, delimiter.len()..delimiter.len());
        assert_eq!(apply(&empty, caret, format), ("".into(), 0..0));
    }
}

#[test]
fn italic_and_bold_delimiters_combine_without_destroying_each_other() {
    for selection in [0..8, 2..6] {
        let (text, selection) = apply("**text**", selection, Format::Italic);
        assert_eq!(text, "***text***");
        assert_eq!(apply(&text, selection, Format::Italic).0, "**text**");
    }
    let (text, selection) = apply("*text*", 1..5, Format::Bold);
    assert_eq!(text, "***text***");
    assert_eq!(apply(&text, selection, Format::Bold).0, "*text*");
}

#[test]
fn inline_code_uses_safe_delimiters_and_padding() {
    for selected in ["a`b", "`code`", "a``b", "`", " café ", "  ", "x\ny"] {
        let (text, selection) = apply(selected, 0..selected.len(), Format::InlineCode);
        // Selected complete code syntax toggles off; other input wraps reversibly.
        if selected == "`code`" {
            assert_eq!(text, "code");
            continue;
        }
        assert_eq!(&text[selection.clone()], selected);
        assert_eq!(apply(&text, selection, Format::InlineCode).0, selected);
    }
    assert_eq!(apply("a`b", 0..3, Format::InlineCode).0, "``a`b``");
    assert_eq!(apply("`", 0..1, Format::InlineCode).0, "`` ` ``");
}

#[test]
fn block_commands_preserve_containers_and_end_boundary() {
    let source = "  > - café\n  > - 茶\nlast\n";
    let end = source.find("last").unwrap();
    let (text, selection) = apply(source, 6..end, Format::OrderedList);
    assert_eq!(text, "  > 1. café\n  > 2. 茶\nlast\n");
    assert_eq!(&text[selection.clone()], "café\n  > 2. 茶\n");
    assert_eq!(
        apply(&text, selection, Format::OrderedList).0,
        "  > café\n  > 茶\nlast\n"
    );
    assert_eq!(
        apply("  > ## café\nlast", 7..7, Format::Heading(3)).0,
        "  > ### café\nlast"
    );
    assert_eq!(apply("  > ## café", 7..7, Format::Heading(0)).0, "  > café");
    assert_eq!(apply("#", 1..1, Format::Heading(2)).0, "## ");
    assert_eq!(apply("  > > café", 8..8, Format::Blockquote).0, "  > café");
    assert_eq!(apply("  café", 3..3, Format::Blockquote).0, "  > café");
}

#[test]
fn list_types_convert_tasks_and_number_nested_siblings() {
    let text = "> - [x] one\n> - two\n>   - child\n>   - child2\n> - three\n>   - child3";
    assert_eq!(
        apply(text, 0..text.len(), Format::OrderedList).0,
        "> 1. one\n> 2. two\n>   1. child\n>   2. child2\n> 3. three\n>   1. child3"
    );
    assert_eq!(
        apply(
            "2) [X] café\n3. 茶",
            0.."2) [X] café\n3. 茶".len(),
            Format::UnorderedList
        )
        .0,
        "- café\n- 茶"
    );
    assert_eq!(apply("- [ ]", 0..5, Format::OrderedList).0, "1. ");
    assert_eq!(
        apply("1. one\n2. two", 0..13, Format::TaskList).0,
        "- [ ] one\n- [ ] two"
    );
    for (format, marker) in [
        (Format::OrderedList, "1. "),
        (Format::UnorderedList, "- "),
        (Format::TaskList, "- [ ] "),
    ] {
        for prefix in ["", "  ", "> ", "  > "] {
            let (text, caret) = apply(prefix, prefix.len()..prefix.len(), format);
            assert_eq!(text, format!("{prefix}{marker}"));
            assert_eq!(caret, text.len()..text.len());
        }
    }
}

#[test]
fn fenced_code_handles_selected_lines_existing_pairs_backticks_and_carets() {
    let (text, selection) = apply("café\n茶\nlast", 0..10, Format::CodeBlock);
    assert_eq!(text, "```\ncafé\n茶\n```\nlast");
    assert_eq!(
        apply(&text, selection, Format::CodeBlock).0,
        "café\n茶\nlast"
    );
    let (text, _) = apply("a```b", 0..5, Format::CodeBlock);
    assert_eq!(text, "````\na```b\n````");
    assert_eq!(
        apply("```rust\ncafé\n```", 9..9, Format::CodeBlock).0,
        "café\n"
    );
    assert_eq!(apply("~~~\nx\n~~~\n", 0..10, Format::CodeBlock).0, "x\n");
    let (text, caret) = apply("", 0..0, Format::CodeBlock);
    assert_eq!(text, "```\n\n```\n");
    assert_eq!(caret, 4..4);
    assert_eq!(
        apply("> hello world", 7..7, Format::CodeBlock).0,
        "> hello\n> ```\n> \n> ```\n>  world"
    );
}

#[test]
fn crlf_and_invalid_utf8_ranges_are_safe() {
    assert_eq!(
        apply("> café\r\n> 茶\r\n", 0..16, Format::Heading(2)).0,
        "> ## café\r\n> ## 茶\r\n"
    );
    assert!(transform("茶", 1..2, Format::Bold).is_none());
    assert!(transform("茶", 0..4, Format::Bold).is_none());
    assert!(transform("x", 0..1, Format::Heading(7)).is_none());
}

#[test]
fn every_valid_selection_keeps_valid_utf8_result_coordinates() {
    for source in [
        "",
        "café\n茶\n",
        "> x\r\n> - [ ] 茶",
        "```rust\nx\n```\n",
        "**é** `茶`",
        "\n\n",
    ] {
        let positions: Vec<_> = (0..=source.len())
            .filter(|i| source.is_char_boundary(*i))
            .collect();
        for &start in &positions {
            for &end in positions.iter().filter(|end| **end >= start) {
                for format in [
                    Format::Bold,
                    Format::Italic,
                    Format::Strikethrough,
                    Format::InlineCode,
                    Format::Heading(0),
                    Format::Heading(6),
                    Format::Blockquote,
                    Format::OrderedList,
                    Format::UnorderedList,
                    Format::TaskList,
                    Format::CodeBlock,
                ] {
                    apply(source, start..end, format);
                }
            }
        }
    }
}
