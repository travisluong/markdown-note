use gpui_component::RopeExt;
use gpui_component::input::{
    Rope,
    table::{Alignment, Command, TableIndex, cell_input, cells},
};

fn index(source: &str) -> (Rope, TableIndex) {
    let rope = Rope::from(source);
    let mut index = TableIndex::default();
    index.rebuild(&rope);
    (rope, index)
}

#[test]
fn gfm_ranges_and_context() {
    for newline in ["\n", "\r\n"] {
        let text = [
            "before",
            "",
            "| **café** | 你好 |",
            "| :--- | ---: |",
            "| a\\|b | `code` |",
            "| | |",
            "",
            "after",
        ]
        .join(newline);
        let (_, index) = index(&text);
        assert_eq!(index.tables.len(), 1);
        let table = &index.tables[0];
        assert_eq!(table.alignments, [Alignment::Left, Alignment::Right]);
        assert_eq!(table.rows.len(), 3);
        assert_eq!(&text[table.rows[1].cells[0].clone()], "a\\|b");
        for row in &table.rows {
            for cell in &row.cells {
                assert!(text.is_char_boundary(cell.start));
                assert!(text.is_char_boundary(cell.end));
            }
        }
    }
    for text in [
        "```md\n| a | b |\n| --- | --- |\n```",
        "    a | b\n    --- | ---",
        "a | b\n---",
        "a | b\n--- | nope",
    ] {
        assert!(index(text).1.tables.is_empty(), "{text}");
    }
    assert_eq!(index("a | b\n--- | ---").1.tables[0].rows.len(), 1);
    let (_, uneven) = index("a | b\n--- | ---\nx\ny | z | extra");
    assert_eq!(uneven.tables[0].rows[1].cells.len(), 2);
    assert_eq!(uneven.tables[0].rows[2].cells.len(), 3);
    assert_eq!(cells("| a\\|b | |", 0).len(), 2);
}

#[test]
fn structural_commands_preserve_document_and_alignment() {
    let original = "before\r\n\r\n| café | 你好 |\r\n| :---: | ---: |\r\n| x | y |\r\n\r\nafter";
    for command in [
        Command::RowAbove,
        Command::RowBelow,
        Command::DeleteRow,
        Command::ColumnLeft,
        Command::ColumnRight,
        Command::DeleteColumn,
        Command::Align(Alignment::Left),
        Command::Align(Alignment::Center),
        Command::Align(Alignment::Right),
        Command::DeleteTable,
    ] {
        let (mut rope, parsed) = index(original);
        let table = &parsed.tables[0];
        let edit = table.edit(&rope, 1, 1, command).unwrap();
        assert_eq!(edit.range, table.source);
        rope.replace(edit.range.clone(), &edit.text);
        let output = rope.to_string();
        assert!(output.starts_with("before\r\n\r\n"));
        assert!(output.ends_with("\r\n\r\nafter"));
        assert!(output.is_char_boundary(edit.caret));
        assert!(!edit.text.replace("\r\n", "").contains('\n'));
        let (_, reparsed) = index(&output);
        assert_eq!(
            reparsed.tables.len(),
            usize::from(command != Command::DeleteTable)
        );
        if command != Command::DeleteTable {
            assert!(reparsed.at(edit.caret).is_some());
        }
    }
    let (rope, parsed) = index("| a |\n| --- |\n| x |");
    let table = &parsed.tables[0];
    assert!(table.edit(&rope, 0, 0, Command::DeleteRow).is_none());
    assert!(table.edit(&rope, 1, 0, Command::DeleteColumn).is_none());
}

#[test]
fn cell_input_preserves_markdown_and_escapes_pipes() {
    assert_eq!(cell_input("a|b\r\nc\rd\ne"), "a\\|b c d e");
    assert_eq!(cell_input("**café** _你好_ \\|"), "**café** _你好_ \\|");
}

#[test]
fn incremental_index_matches_fresh_parser() {
    let initial = "# Title\n\npara\n\n| a | b |\n| --- | --- |\n| x | y |\n\nafter\n\n```\na | b\n--- | ---\n```\n";
    let (mut rope, mut cached) = index(initial);
    let mut seed = 7u64;
    for i in 0..180 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let text = rope.to_string();
        let at = seed as usize % (text.len() + 1);
        let at = text.floor_char_boundary(at);
        let end = if i % 4 == 0 {
            text.ceil_char_boundary((at + 1).min(text.len()))
        } else {
            at
        };
        let insertion = ["x", "|", "\n", " ", "`", "", "你好"][i % 7];
        let old = rope.clone();
        rope.replace(at..end, insertion);
        cached.update(&old, &rope, &(at..end), &Rope::from(insertion));
        let mut fresh = TableIndex::default();
        fresh.rebuild(&rope);
        assert_eq!(
            format!("{:?}", cached.tables),
            format!("{:?}", fresh.tables),
            "iteration {i}: {}",
            rope.to_string()
        );
    }
}

#[test]
fn plain_typing_does_not_parse_document() {
    let (mut rope, mut cached) = index(&format!(
        "{}\n\na | b\n--- | ---\nx | y",
        "ordinary text\n".repeat(10000)
    ));
    let before = cached.parsed_bytes;
    let old = rope.clone();
    rope.replace(2..2, "z");
    cached.update(&old, &rope, &(2..2), &Rope::from("z"));
    assert_eq!(cached.parsed_bytes, before);
    assert_eq!(
        cached.tables[0].source.start,
        old.len() - "a | b\n--- | ---\nx | y".len() + 1
    );
}

#[test]
fn inline_code_links_and_escapes_keep_source_offsets() {
    use gpui_component::input::table::inline;
    for source in [
        "**café** _你好_",
        "[label](https://example.com)",
        "~~old~~",
        "a\\|b",
    ] {
        let inline = inline(source);
        for range in inline
            .hidden
            .iter()
            .chain(inline.styles.iter().map(|(r, _)| r))
        {
            assert!(range.end <= source.len());
            assert!(source.is_char_boundary(range.start));
            assert!(source.is_char_boundary(range.end));
        }
        assert!(!inline.hidden.is_empty());
    }
    assert_eq!(inline("**你好** words").styles, vec![(0..10, 1)]);
    let source = format!("{0}code{0}", char::from(96));
    let code = inline(&source);
    assert_eq!(code.styles[0].1, 3);
    assert_eq!(code.hidden.len(), 2);
}

#[test]
fn typing_in_large_table_does_not_reparse_it() {
    let (mut rope, mut cached) = index(&format!(
        "| a | b |\n| --- | --- |\n{}",
        "| café | value |\n".repeat(10000)
    ));
    let at = cached.tables[0].rows[5000].cells[0].start + 1;
    let old = rope.clone();
    let before = cached.parsed_bytes;
    rope.replace(at..at, "你好");
    cached.update(&old, &rope, &(at..at), &Rope::from("你好"));
    assert_eq!(cached.parsed_bytes, before);
    let mut fresh = TableIndex::default();
    fresh.rebuild(&rope);
    assert_eq!(
        format!("{:?}", cached.tables),
        format!("{:?}", fresh.tables)
    );
}
