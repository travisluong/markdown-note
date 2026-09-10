//! Source-preserving GFM table model. Public only for the application's editor test harness.
use super::RopeExt;
use markdown::{ParseOptions, mdast::Node};
use ropey::Rope;
use std::ops::Range;

pub const CELL_PADDING: f32 = 8.;
pub const MIN_COLUMN: f32 = 80.;
pub const MAX_COLUMN: f32 = 320.;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Alignment {
    Left,
    Center,
    Right,
}
#[derive(Clone, Debug)]
pub struct Row {
    pub source: Range<usize>,
    pub present_cells: usize,
    pub cells: Vec<Range<usize>>,
}
#[derive(Clone, Debug)]
pub struct Table {
    pub source: Range<usize>,
    pub separator: Range<usize>,
    pub rows: Vec<Row>,
    pub alignments: Vec<Alignment>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    PadRow,
    RowAbove,
    RowBelow,
    DeleteRow,
    ColumnLeft,
    ColumnRight,
    DeleteColumn,
    Align(Alignment),
    DeleteTable,
}
#[derive(Clone, Debug)]
pub struct Edit {
    pub range: Range<usize>,
    pub text: String,
    pub caret: usize,
}

/// Split only unescaped pipes, including inside code spans (GFM's table rule).
pub fn cells(line: &str, base: usize) -> Vec<Range<usize>> {
    let line = line.trim_end_matches(['\r', '\n']);
    let mut cuts = vec![];
    let mut slashes = 0;
    for (i, c) in line.char_indices() {
        if c == '|' && slashes % 2 == 0 {
            cuts.push(i);
        }
        slashes = if c == '\\' { slashes + 1 } else { 0 };
    }
    let mut ranges = vec![];
    let mut start = 0;
    for end in cuts.iter().copied().chain(std::iter::once(line.len())) {
        ranges.push(start..end);
        start = end + 1;
    }
    if cuts.first().is_some_and(|&i| line[..i].trim().is_empty()) {
        ranges.remove(0);
    }
    if cuts
        .last()
        .is_some_and(|&i| line[i + 1..].trim().is_empty())
    {
        ranges.pop();
    }
    ranges
        .into_iter()
        .map(|r| {
            let raw = &line[r.clone()];
            let start = r.start + raw.len() - raw.trim_start().len();
            let end = (r.end - (raw.len() - raw.trim_end().len())).max(start);
            base + start..base + end
        })
        .collect()
}

fn parse(text: &str, base: usize) -> (Vec<Range<usize>>, Vec<Table>) {
    // GFM requires at least one pipe in a table header. Avoid parsing plain documents.
    if !text.contains('|') {
        return Default::default();
    }
    let Ok(Node::Root(root)) = markdown::to_mdast(text, &ParseOptions::gfm()) else {
        return Default::default();
    };
    let mut blocks = vec![];
    let mut tables = vec![];
    for node in root.children {
        let Some(p) = node.position() else { continue };
        let range = p.start.offset..p.end.offset;
        blocks.push(base + range.start..base + range.end);
        let Node::Table(t) = node else { continue };
        let mut rows = vec![];
        let mut separator = 0..0;
        let mut offset = range.start;
        for (i, line) in text[range.clone()].split_inclusive('\n').enumerate() {
            let end = offset + line.trim_end_matches(['\r', '\n']).len();
            if i == 1 {
                separator = base + offset..base + end;
            } else {
                let cells = cells(line, base + offset);
                rows.push(Row {
                    source: base + offset..base + end,
                    present_cells: cells.len(),
                    cells,
                });
            }
            offset += line.len();
        }
        let alignments = t
            .align
            .iter()
            .map(|a| match a {
                markdown::mdast::AlignKind::Center => Alignment::Center,
                markdown::mdast::AlignKind::Right => Alignment::Right,
                _ => Alignment::Left,
            })
            .collect::<Vec<_>>();
        // Missing body cells are empty insertion points; surplus cells remain in source.
        for row in &mut rows {
            let end = row.source.end;
            row.cells
                .resize_with(alignments.len().max(row.cells.len()), || end..end);
        }
        tables.push(Table {
            source: base + range.start..base + range.end,
            separator,
            rows,
            alignments,
        });
    }
    (blocks, tables)
}

impl Table {
    pub fn cell_at(&self, offset: usize) -> Option<(usize, usize)> {
        let row = self
            .rows
            .iter()
            .position(|r| r.source.start <= offset && offset <= r.source.end)?;
        let cells = &self.rows[row].cells[..self.alignments.len()];
        let col = cells
            .partition_point(|c| c.end < offset)
            .min(cells.len() - 1);
        Some((row, col))
    }
    pub fn edit(&self, source: &Rope, row: usize, col: usize, command: Command) -> Option<Edit> {
        if row >= self.rows.len() || col >= self.alignments.len() {
            return None;
        }
        if command == Command::DeleteTable {
            return Some(Edit {
                range: self.source.clone(),
                text: String::new(),
                caret: self.source.start,
            });
        }
        let mut data = self
            .rows
            .iter()
            .map(|r| {
                r.cells
                    .iter()
                    .map(|c| source.slice(c.clone()).to_string())
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        let mut align = self.alignments.clone();
        let mut target = (row, col);
        match command {
            Command::RowAbove | Command::RowBelow => {
                let at = if row == 0 {
                    1
                } else {
                    row + usize::from(command == Command::RowBelow)
                };
                data.insert(at, vec![String::new(); align.len()]);
                target.0 = at;
            }
            Command::DeleteRow if row > 0 => {
                data.remove(row);
                target.0 = row.min(data.len() - 1);
            }
            Command::ColumnLeft | Command::ColumnRight => {
                let at = col + usize::from(command == Command::ColumnRight);
                for r in &mut data {
                    r.insert(at, String::new());
                }
                align.insert(at, Alignment::Left);
                target.1 = at;
            }
            Command::DeleteColumn if align.len() > 1 => {
                for r in &mut data {
                    r.remove(col);
                }
                align.remove(col);
                target.1 = col.min(align.len() - 1);
            }
            Command::Align(a) => align[col] = a,
            Command::PadRow => {}
            _ => return None,
        }
        let newline = if source
            .slice(self.source.clone())
            .to_string()
            .contains("\r\n")
        {
            "\r\n"
        } else {
            "\n"
        };
        let mut text = String::new();
        let mut caret = self.source.start;
        for (i, r) in data.iter().enumerate() {
            if i > 0 {
                text.push_str(newline);
            }
            text.push_str("| ");
            for (j, c) in r.iter().enumerate() {
                if j > 0 {
                    text.push_str(" | ");
                }
                if (i, j) == target {
                    caret = self.source.start + text.len();
                }
                text.push_str(c);
            }
            text.push_str(" |");
            if i == 0 {
                text.push_str(newline);
                text.push('|');
                for a in &align {
                    text.push_str(match a {
                        Alignment::Left => " --- |",
                        Alignment::Center => " :---: |",
                        Alignment::Right => " ---: |",
                    });
                }
            }
        }
        Some(Edit {
            range: self.source.clone(),
            text,
            caret,
        })
    }
    fn shift(&mut self, delta: isize) {
        fn shift(r: &mut Range<usize>, d: isize) {
            r.start = r.start.saturating_add_signed(d);
            r.end = r.end.saturating_add_signed(d);
        }
        shift(&mut self.source, delta);
        shift(&mut self.separator, delta);
        for row in &mut self.rows {
            shift(&mut row.source, delta);
            for cell in &mut row.cells {
                shift(cell, delta);
            }
        }
    }
}

#[derive(Default)]
pub struct TableIndex {
    pub tables: Vec<Table>,
    blocks: Vec<Range<usize>>,
    pub revision: u64,
    pub parsed_bytes: u64,
    initialized: bool,
}
impl TableIndex {
    pub fn at(&self, offset: usize) -> Option<(&Table, usize, usize)> {
        let ix = self.tables.partition_point(|t| t.source.end < offset);
        let t = self.tables.get(ix)?;
        let (r, c) = t.cell_at(offset)?;
        Some((t, r, c))
    }
    pub fn rebuild(&mut self, text: &Rope) {
        let source = text.to_string();
        (self.blocks, self.tables) = parse(&source, 0);
        self.parsed_bytes += source.len() as u64;
        self.revision += 1;
        self.initialized = true;
    }
    pub fn update(&mut self, old: &Rope, text: &Rope, range: &Range<usize>, inserted: &Rope) {
        if !self.initialized {
            self.rebuild(text);
            return;
        }
        if range.is_empty() && inserted.len() == 0 {
            return;
        }
        // Context-changing constructs may invalidate every subsequent block.
        let changed = format!("{}{}", old.slice(range.clone()), inserted);
        if changed.chars().all(|c| c.is_alphanumeric() || c == ' ') {
            if let Some((t, r, c)) = self.at(range.start) {
                if range.end <= t.rows[r].cells[c].end && range.start >= t.rows[r].cells[c].start {
                    let start = t.source.start;
                    let row_start = t.rows[r].source.start;
                    let delta = inserted.len() as isize - range.len() as isize;
                    for b in &mut self.blocks {
                        if b.start > range.start {
                            b.start = b.start.saturating_add_signed(delta);
                        }
                        if b.end >= range.start {
                            b.end = b.end.saturating_add_signed(delta);
                        }
                    }
                    for t in &mut self.tables {
                        if t.source.start > range.start {
                            t.shift(delta);
                        } else if t.source.start == start {
                            t.source.end = t.source.end.saturating_add_signed(delta);
                            if t.separator.start > range.start {
                                t.separator.start = t.separator.start.saturating_add_signed(delta);
                                t.separator.end = t.separator.end.saturating_add_signed(delta);
                            }
                            for row in &mut t.rows {
                                if row.source.start > range.start {
                                    row.source.start =
                                        row.source.start.saturating_add_signed(delta);
                                    row.source.end = row.source.end.saturating_add_signed(delta);
                                    for cell in &mut row.cells {
                                        cell.start = cell.start.saturating_add_signed(delta);
                                        cell.end = cell.end.saturating_add_signed(delta);
                                    }
                                } else if row.source.start == row_start {
                                    row.source.end = row.source.end.saturating_add_signed(delta);
                                    row.cells = cells(
                                        &text.slice(row.source.clone()).to_string(),
                                        row.source.start,
                                    );
                                    row.present_cells = row.cells.len();
                                    let end = row.cells.last().map_or(row.source.end, |c| c.end);
                                    row.cells.resize_with(
                                        row.cells.len().max(t.alignments.len()),
                                        || end..end,
                                    );
                                }
                            }
                        }
                    }
                    self.revision += 1;
                    return;
                }
            }
        }
        if changed.chars().all(|c| c.is_alphanumeric() || c == ' ')
            && self.at(range.start).is_none()
            && !old
                .slice_line(old.offset_to_point(range.start).row)
                .chars()
                .any(|c| c == '|')
        {
            let delta = inserted.len() as isize - range.len() as isize;
            for block in &mut self.blocks {
                if block.start > range.start {
                    block.start = block.start.saturating_add_signed(delta);
                }
                if block.end >= range.start {
                    block.end = block.end.saturating_add_signed(delta);
                }
            }
            for table in &mut self.tables {
                if table.source.start > range.start {
                    table.shift(delta);
                }
            }
            self.revision += 1;
            return;
        }
        if changed.contains(['`', '~', '<', '>', '[', ']']) || range.len() == old.len() {
            self.rebuild(text);
            return;
        }
        let first = self
            .blocks
            .partition_point(|b| b.end < range.start)
            .saturating_sub(1);
        let last =
            (self.blocks.partition_point(|b| b.start <= range.end) + 1).min(self.blocks.len());
        let start = self.blocks.get(first).map_or(0, |b| {
            old.line_start_offset(old.offset_to_point(b.start).row)
        });
        let end = self.blocks.get(last).map_or(old.len(), |b| {
            old.line_start_offset(old.offset_to_point(b.start).row)
        });
        let delta = inserted.len() as isize - range.len() as isize;
        let new_end = end.saturating_add_signed(delta);
        let fragment = text.slice(start..new_end).to_string();
        let (blocks, tables) = parse(&fragment, start);
        self.parsed_bytes += fragment.len() as u64;
        self.blocks.retain(|b| b.end <= start || b.start >= end);
        for b in &mut self.blocks {
            if b.start >= end {
                b.start = b.start.saturating_add_signed(delta);
                b.end = b.end.saturating_add_signed(delta);
            }
        }
        self.blocks.extend(blocks);
        self.blocks.sort_by_key(|b| b.start);
        self.tables
            .retain(|t| t.source.end <= start || t.source.start >= end);
        for t in &mut self.tables {
            if t.source.start >= end {
                t.shift(delta);
            }
        }
        self.tables.extend(tables);
        self.tables.sort_by_key(|t| t.source.start);
        self.revision += 1;
    }
}

pub fn cell_input(input: &str) -> String {
    let mut result = String::new();
    let mut slashes = 0;
    for c in input.replace("\r\n", "\n").chars() {
        if c == '|' && slashes % 2 == 0 {
            result.push('\\');
        }
        result.push(if matches!(c, '\r' | '\n') { ' ' } else { c });
        slashes = if c == '\\' { slashes + 1 } else { 0 };
    }
    result
}

#[derive(Default)]
pub struct Inline {
    pub hidden: Vec<Range<usize>>,
    pub styles: Vec<(Range<usize>, u8)>,
}
pub fn inline(source: &str) -> Inline {
    let mut result = Inline::default();
    if !source
        .chars()
        .any(|c| matches!(c, '*' | '_' | '[' | '~') || c == char::from(96) || c == char::from(92))
    {
        return result;
    }
    fn visit(node: &Node, result: &mut Inline) {
        let Some(p) = node.position() else {
            return;
        };
        let range = p.start.offset.saturating_sub(2)..p.end.offset.saturating_sub(2);
        let style = match node {
            Node::Strong(_) => 1,
            Node::Emphasis(_) => 2,
            Node::InlineCode(_) => 3,
            Node::Delete(_) => 4,
            Node::Link(_) => 5,
            _ => 0,
        };
        if style > 0 {
            result.styles.push((range.clone(), style));
            if let Some(children) = node.children() {
                if let (Some(first), Some(last)) = (
                    children.first().and_then(|n| n.position()),
                    children.last().and_then(|n| n.position()),
                ) {
                    result
                        .hidden
                        .push(range.start..first.start.offset.saturating_sub(2));
                    result
                        .hidden
                        .push(last.end.offset.saturating_sub(2)..range.end);
                }
            }
        }
        if let Some(children) = node.children() {
            for child in children {
                visit(child, result);
            }
        }
    }
    if let Ok(node) = markdown::to_mdast(&format!("x {source}"), &ParseOptions::gfm()) {
        visit(&node, &mut result);
    }
    for (range, style) in &result.styles {
        if *style == 3 {
            let raw = &source[range.clone()];
            let ticks = raw.bytes().take_while(|c| *c == 96).count();
            result.hidden.push(range.start..range.start + ticks);
            result.hidden.push(range.end - ticks..range.end);
        }
    }
    let mut escaped = false;
    for (i, c) in source.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if c == char::from(92)
            && source[i + 1..]
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_punctuation())
        {
            result.hidden.push(i..i + 1);
            escaped = true;
        }
    }
    result.hidden.sort_by_key(|r| r.start);
    result
}
pub fn column_width(characters: usize, font_size: f32) -> f32 {
    (font_size * characters as f32 * 0.55 + CELL_PADDING * 2.).clamp(MIN_COLUMN, MAX_COLUMN)
}
