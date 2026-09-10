use std::ops::Range;

use gpui::{App, Font, LineFragment, Pixels, Point, ShapedLine, Size, Window, point, px, size};
use ropey::Rope;
use smallvec::SmallVec;

use crate::input::RopeExt;

#[derive(Debug, Clone)]
pub(super) struct TableCellLayout {
    pub source: Range<usize>,
    pub x: Pixels,
    pub width: Pixels,
    pub alignment: super::table::Alignment,
    pub styles: Vec<(Range<usize>, u8)>,
    pub mapped: std::rc::Rc<super::live_preview::MappedText>,
    pub wraps: Vec<Range<usize>>,
}
#[derive(Debug, Clone)]
pub(super) struct TableRowLayout {
    pub scroll: std::rc::Rc<std::cell::Cell<Pixels>>,
    pub viewport: Pixels,
    pub cells: Vec<TableCellLayout>,
    pub height: usize,
    pub header: bool,
    pub active: Option<usize>,
    pub widths: Vec<Pixels>,
}

/// A line with soft wrapped lines info.
#[derive(Debug, Clone)]
pub(super) struct LineItem {
    pub(super) table: Option<std::rc::Rc<TableRowLayout>>,
    /// The original line text, without end `\n`.
    pub(super) line: Rope,
    measured: bool,
    pub(super) cache:
        std::cell::RefCell<std::collections::HashMap<usize, (Vec<gpui::TextRun>, ShapedLine)>>,
    pub(super) presentation: Option<super::live_preview::Presentation>,
    pub(super) mapped: Option<std::rc::Rc<super::live_preview::MappedText>>,
    pub(super) scale: f32,
    /// The soft wrapped lines relative byte range (0..line.len) of this line (Include first line).
    ///
    /// Not contains the line end `\n`.
    pub(super) wrapped_lines: std::rc::Rc<Vec<Range<usize>>>,
}

impl LineItem {
    /// Get the bytes length of this line.
    #[inline]
    pub(super) fn len(&self) -> usize {
        self.line.len()
    }

    /// Get number of soft wrapped lines of this line (include the first line).
    #[inline]
    pub(super) fn lines_len(&self) -> usize {
        self.table
            .as_ref()
            .map_or(self.wrapped_lines.len(), |t| t.height)
    }

    /// Get the height of this line item with given line height.
    pub(super) fn height(&self, line_height: Pixels) -> Pixels {
        self.lines_len() as f32 * line_height * self.scale
    }
}

#[derive(Debug, Default)]
pub(super) struct LongestRow {
    /// The 0-based row index.
    pub row: usize,
    /// The bytes length of the longest line.
    pub len: usize,
}

/// Used to prepare the text with soft wrap to be get lines to displayed in the Editor.
///
/// After use lines to calculate the scroll size of the Editor.
pub(super) struct TextWrapper {
    pub(super) tables: super::table::TableIndex,
    active_offset: usize,
    table_rows: std::collections::HashSet<usize>,
    table_scroll: std::collections::HashMap<usize, std::rc::Rc<std::cell::Cell<Pixels>>>,
    table_widths: std::collections::HashMap<usize, Vec<Pixels>>,
    text: Rope,
    /// Total wrapped lines (Inlucde the first line), value is start and end index of the line.
    soft_lines: usize,
    font: Font,
    font_size: Pixels,
    task_size: Pixels,
    /// If is none, it means the text is not wrapped
    wrap_width: Option<Pixels>,
    /// The longest (row, bytes len) in characters, used to calculate the horizontal scroll width.
    pub(super) longest_row: LongestRow,
    /// The lines by split \n
    pub(super) lines: super::line_index::Lines,

    _initialized: bool,
    live_preview: bool,
    estimating: bool,
    reveal_row: usize,
}

#[allow(unused)]
impl TextWrapper {
    pub(super) fn new(font: Font, font_size: Pixels, wrap_width: Option<Pixels>) -> Self {
        Self {
            tables: Default::default(),
            active_offset: usize::MAX,
            table_rows: Default::default(),
            table_scroll: Default::default(),
            table_widths: Default::default(),
            text: Rope::new(),
            font,
            font_size,
            task_size: px(14.),
            wrap_width,
            soft_lines: 0,
            longest_row: LongestRow::default(),
            lines: Default::default(),
            _initialized: false,
            live_preview: false,
            estimating: false,
            reveal_row: 0,
        }
    }

    pub(super) fn set_live_preview(&mut self, enabled: bool, cx: &mut App) {
        self.live_preview = enabled;
        if enabled {
            self.tables.rebuild(&self.text);
        }
        self.table_widths.clear();
        self.update_all(&self.text.clone(), cx);
    }

    pub(super) fn measure(&mut self, rows: Range<usize>, cx: &mut App) {
        let text = self.text.clone();
        for row in rows {
            if self.lines.get(row).is_some_and(|l| !l.measured) {
                let offset = text.line_start_offset(row);
                self.update(&text, &(offset..offset), &Rope::new(), cx);
            }
        }
    }

    pub(super) fn reveal(&mut self, row: usize, cx: &mut App) {
        if !self.live_preview || self.reveal_row == row {
            return;
        }
        let old = self.reveal_row;
        self.reveal_row = row;
        let text = self.text.clone();
        for row in [old, row] {
            if row < text.lines_len() {
                let offset = text.line_start_offset(row);
                self.update(&text, &(offset..offset), &Rope::new(), cx);
            }
        }
    }

    pub(super) fn reveal_cell(&mut self, offset: usize, cx: &mut App) {
        let previous = self
            .tables
            .at(self.active_offset)
            .map(|(t, r, c)| (t.source.start, r, c));
        let next = self
            .tables
            .at(offset)
            .map(|(t, r, c)| (t.source.start, r, c));
        self.active_offset = offset;
        if self.live_preview && previous != next {
            let text = self.text.clone();
            self.update(&text, &(offset..offset), &Rope::new(), cx);
        }
    }

    #[inline]
    pub(super) fn set_default_text(&mut self, text: &Rope) {
        self.text = text.clone();
    }

    /// Get the total number of lines including wrapped lines.
    #[inline]
    pub(super) fn len(&self) -> usize {
        self.soft_lines
    }

    /// Get the line item by row index.
    #[inline]
    pub(super) fn line(&self, row: usize) -> Option<&LineItem> {
        self.lines.get(row)
    }

    pub(super) fn set_wrap_width(&mut self, wrap_width: Option<Pixels>, cx: &mut App) {
        if wrap_width == self.wrap_width {
            return;
        }

        self.wrap_width = wrap_width;
        self.table_widths.clear();
        self.update_all(&self.text.clone(), cx);
    }

    pub(super) fn set_font(&mut self, font: Font, font_size: Pixels, cx: &mut App) {
        if self.font.eq(&font) && self.font_size == font_size {
            return;
        }

        self.font = font;
        self.font_size = font_size;
        self.table_widths.clear();
        self.update_all(&self.text.clone(), cx);
    }

    pub(super) fn set_task_size(&mut self, size: Pixels, cx: &mut App) {
        if self.task_size != size {
            self.task_size = size;
            self.update_all(&self.text.clone(), cx);
        }
    }

    pub(super) fn prepare_if_need(&mut self, text: &Rope, cx: &mut App) {
        if self._initialized {
            return;
        }
        self._initialized = true;
        self.update_all(text, cx);
    }

    /// Update the text wrapper and recalculate the wrapped lines.
    ///
    /// If the `text` is the same as the current text, do nothing.
    ///
    /// - `changed_text`: The text [`Rope`] that has changed.
    /// - `range`: The `selected_range` before change.
    /// - `new_text`: The inserted text.
    /// - `force`: Whether to force the update, if false, the update will be skipped if the text is the same.
    /// - `cx`: The application context.
    pub(super) fn update(
        &mut self,
        changed_text: &Rope,
        range: &Range<usize>,
        new_text: &Rope,
        cx: &mut App,
    ) {
        if self.live_preview {
            if self.active_offset != usize::MAX && (!range.is_empty() || new_text.len() > 0) {
                self.active_offset = if self.active_offset < range.start {
                    self.active_offset
                } else if self.active_offset <= range.end {
                    range.start + new_text.len()
                } else {
                    self.active_offset
                        .saturating_add_signed(new_text.len() as isize - range.len() as isize)
                };
            }
            let first = self.text.offset_to_point(range.start).row;
            let last = self.text.offset_to_point(range.end).row;
            let delta = new_text.lines_len() as isize - 1 - (last - first) as isize;
            self.table_rows = self
                .table_rows
                .iter()
                .filter_map(|r| {
                    if *r < first {
                        Some(*r)
                    } else if *r > last {
                        Some(r.saturating_add_signed(delta))
                    } else {
                        None
                    }
                })
                .collect();
            self.tables
                .update(&self.text, changed_text, range, new_text);
        }
        let mut line_wrapper = cx
            .text_system()
            .line_wrapper(self.font.clone(), self.font_size);
        let task_size = self.task_size;
        self._update(
            changed_text,
            range,
            new_text,
            &mut |line_str, wrap_width, task| {
                let fragments = if let Some(offset) = task {
                    vec![
                        LineFragment::text(&line_str[..offset]),
                        LineFragment::element(task_size, 3),
                        LineFragment::text(&line_str[offset + 3..]),
                    ]
                } else {
                    vec![LineFragment::text(line_str)]
                };
                line_wrapper.wrap_line(&fragments, wrap_width).collect()
            },
        );
        if self.live_preview {
            self.layout_tables(&mut |text, width| {
                line_wrapper
                    .wrap_line(&[LineFragment::text(text)], width)
                    .map(|b| b.ix)
                    .collect()
            });
        }
    }

    fn _update<F>(
        &mut self,
        changed_text: &Rope,
        range: &Range<usize>,
        new_text: &Rope,
        wrap_line: &mut F,
    ) where
        F: FnMut(&str, Pixels, Option<usize>) -> Vec<gpui::Boundary>,
    {
        // Remove the old changed lines.
        let start_row = self.text.offset_to_point(range.start).row;
        let start_row = start_row.min(self.lines.len().saturating_sub(1));
        let end_row = self.text.offset_to_point(range.end).row;
        let end_row = end_row.min(self.lines.len().saturating_sub(1));
        let rows_range = start_row..=end_row;

        if rows_range.contains(&self.longest_row.row) {
            self.longest_row = LongestRow::default();
        }

        let mut longest_row_ix = self.longest_row.row;
        let mut longest_row_len = self.longest_row.len;

        // To add the new lines.
        let new_start_row = changed_text.offset_to_point(range.start).row;
        let new_start_offset = changed_text.line_start_offset(new_start_row);
        let new_end_row = changed_text
            .offset_to_point(range.start + new_text.len())
            .row;
        let new_end_offset = changed_text.line_end_offset(new_end_row);
        let new_range = new_start_offset..new_end_offset;

        let mut new_lines = vec![];
        let wrap_width = self.wrap_width;

        // line not contains `\n`.
        for (ix, line) in Rope::from(changed_text.slice(new_range))
            .iter_lines()
            .enumerate()
        {
            if self.estimating {
                let columns =
                    (self.wrap_width.unwrap_or(px(800.)) / (self.font_size * 0.5)).max(1.) as usize;
                let count = line.len().div_ceil(columns).max(1);
                // No text conversion or parsing until this line approaches the viewport.
                let ranges = (0..count)
                    .map(|i| (i * columns).min(line.len())..((i + 1) * columns).min(line.len()))
                    .collect();
                new_lines.push(LineItem {
                    table: None,
                    line: Rope::from(line),
                    measured: false,
                    presentation: None,
                    mapped: None,
                    scale: 1.,
                    cache: Default::default(),
                    wrapped_lines: std::rc::Rc::new(ranges),
                });
                continue;
            }
            let source = line.to_string();
            let presentation = self
                .live_preview
                .then(|| super::live_preview::Presentation::parse(&source));
            let scale = presentation.as_ref().map_or(1., |p| p.scale());
            let mapped = presentation.as_ref().map(|p| {
                std::rc::Rc::new(p.display(&source, new_start_row + ix == self.reveal_row))
            });
            let line_str = mapped.as_ref().map_or(source.as_str(), |m| m.text.as_str());
            let mut wrapped_lines = vec![];
            let mut prev_boundary_ix = 0;

            if line_str.len() > longest_row_len {
                longest_row_ix = new_start_row + ix;
                longest_row_len = line_str.len();
            }

            // If wrap_width is Pixels::MAX, skip wrapping to disable word wrap
            if let Some(wrap_width) = wrap_width {
                // Here only have wrapped line, if there is no wrap meet, the `line_wraps` result will empty.
                for boundary in wrap_line(
                    &line_str,
                    (wrap_width
                        - presentation
                            .as_ref()
                            .map_or(px(0.), |p| px(p.quote_indent(self.font_size.into()))))
                    .max(self.font_size)
                        / scale,
                    presentation
                        .as_ref()
                        .and_then(|p| p.task)
                        .map(|state| mapped.as_ref().unwrap().source_to_display[state]),
                ) {
                    wrapped_lines.push(prev_boundary_ix..boundary.ix);
                    prev_boundary_ix = boundary.ix;
                }
            }

            // Reset of the line
            if !line_str[prev_boundary_ix..].is_empty() || prev_boundary_ix == 0 {
                wrapped_lines.push(prev_boundary_ix..line_str.len());
            }

            new_lines.push(LineItem {
                table: None,
                line: Rope::from(line),
                measured: true,
                cache: Default::default(),
                presentation,
                mapped,
                scale,
                wrapped_lines: std::rc::Rc::new(wrapped_lines),
            });
        }

        if self.lines.len() == 0 {
            self.lines = new_lines.into();
        } else {
            self.lines.splice(rows_range, new_lines);
        }

        self.text = changed_text.clone();
        self.soft_lines = self.lines.wraps();
        self.longest_row = LongestRow {
            row: longest_row_ix,
            len: longest_row_len,
        }
    }

    /// Update the text wrapper and recalculate the wrapped lines.
    ///
    /// If the `text` is the same as the current text, do nothing.
    fn update_all(&mut self, text: &Rope, cx: &mut App) {
        self.estimating = self.wrap_width.is_some() || self.live_preview;
        self.update(text, &(0..text.len()), &text, cx);
        self.estimating = false;
    }

    fn layout_tables(&mut self, wrap: &mut impl FnMut(&str, Pixels) -> Vec<usize>) {
        use super::table::{CELL_PADDING, MIN_COLUMN};
        let mut valid_rows = std::collections::HashSet::new();
        for table in &self.tables.tables {
            let active = table.cell_at(self.active_offset);
            let widths = if active.is_some() {
                // Keep widths stable while typing, but rebuild them after column edits.
                self.table_widths
                    .get(&table.source.start)
                    .filter(|widths| widths.len() == table.alignments.len())
                    .cloned()
            } else {
                None
            };
            let widths = widths.unwrap_or_else(|| {
                let mut widths = vec![px(MIN_COLUMN); table.alignments.len()];
                for row in &table.rows {
                    for (i, c) in row.cells.iter().take(widths.len()).enumerate() {
                        let chars = self.text.slice(c.clone()).chars().count();
                        widths[i] = widths[i]
                            .max(px(super::table::column_width(chars, self.font_size.into())));
                    }
                }
                let available = self.wrap_width.unwrap_or(px(800.));
                let sum: Pixels = widths.iter().copied().fold(px(0.), |a, b| a + b);
                if sum > available {
                    for w in &mut widths {
                        *w = (*w * (available / sum)).max(px(MIN_COLUMN));
                    }
                }
                widths
            });
            self.table_widths.insert(table.source.start, widths.clone());
            let scroll = self
                .table_scroll
                .entry(table.source.start)
                .or_default()
                .clone();
            let viewport = self.wrap_width.unwrap_or(px(800.));
            let total = widths.iter().copied().fold(px(0.), |a, b| a + b);
            scroll.set(scroll.get().clamp(px(0.), (total - viewport).max(px(0.))));
            for (r, source_row) in table.rows.iter().enumerate() {
                let row = self.text.offset_to_point(source_row.source.start).row;
                valid_rows.insert(row);
                let Some(old) = self.lines.get(row) else {
                    continue;
                };
                let active_col = active.filter(|(ar, _)| *ar == r).map(|(_, c)| c);
                if old.table.as_ref().is_some_and(|t| {
                    t.widths == widths
                        && t.active == active_col
                        && t.header == (r == 0)
                        && t.cells
                            .iter()
                            .zip(&table.alignments)
                            .all(|(cell, align)| cell.alignment == *align)
                }) {
                    continue;
                }
                let line_start = self.text.line_start_offset(row);
                let mut cells = vec![];
                let mut x = px(0.);
                let mut height = 1;
                for (c, range) in source_row.cells.iter().take(widths.len()).enumerate() {
                    let text = self.text.slice(range.clone()).to_string();
                    let inline = super::table::inline(&text);
                    let presentation = super::live_preview::Presentation {
                        hidden: inline.hidden,
                        ..Default::default()
                    };
                    let mapped =
                        std::rc::Rc::new(presentation.display(&text, active_col == Some(c)));
                    let mut starts = vec![0];
                    starts.extend(wrap(&mapped.text, widths[c] - px(CELL_PADDING * 2.)));
                    starts.push(mapped.text.len());
                    let wraps = starts.windows(2).map(|p| p[0]..p[1]).collect::<Vec<_>>();
                    height = height.max(wraps.len());
                    cells.push(TableCellLayout {
                        source: range.start - line_start..range.end - line_start,
                        x,
                        width: widths[c],
                        alignment: table.alignments[c],
                        styles: inline.styles,
                        mapped,
                        wraps,
                    });
                    x += widths[c];
                }
                let mut item = old.clone();
                item.table = Some(std::rc::Rc::new(TableRowLayout {
                    scroll: scroll.clone(),
                    viewport,
                    cells,
                    height,
                    header: r == 0,
                    active: active_col,
                    widths: widths.clone(),
                }));
                item.scale = 1.;
                item.measured = true;
                item.mapped = None;
                item.presentation = None;
                item.wrapped_lines = std::rc::Rc::new(vec![0..item.line.len(); height]);
                item.cache.borrow_mut().clear();
                self.lines.splice(row..=row, vec![item]);
            }
            let row = self.text.offset_to_point(table.separator.start).row;
            valid_rows.insert(row);
            if let Some(old) = self.lines.get(row) {
                let mut item = old.clone();
                item.table = Some(std::rc::Rc::new(TableRowLayout {
                    scroll: scroll.clone(),
                    viewport,
                    cells: vec![],
                    height: 0,
                    header: false,
                    active: None,
                    widths,
                }));
                item.scale = 0.;
                item.measured = true;
                item.mapped = None;
                item.presentation = None;
                item.wrapped_lines = std::rc::Rc::new(vec![0..item.line.len()]);
                self.lines.splice(row..=row, vec![item]);
            }
        }
        // Restore rows of tables that an edit made invalid.
        let stale = self
            .table_rows
            .difference(&valid_rows)
            .copied()
            .collect::<Vec<_>>();
        self.table_rows = valid_rows;
        for row in stale {
            let Some(mut item) = self.lines.get(row).cloned() else {
                continue;
            };
            item.table = None;
            item.measured = false;
            item.scale = 1.;
            self.lines.splice(row..=row, vec![item]);
        }
        self.soft_lines = self.lines.wraps();
        let starts = self
            .tables
            .tables
            .iter()
            .map(|t| t.source.start)
            .collect::<std::collections::HashSet<_>>();
        self.table_widths.retain(|start, _| starts.contains(start));
        self.table_scroll.retain(|start, _| starts.contains(start));
    }

    /// Return display point (with soft wrap) from the given byte offset in the text.
    ///
    /// Panics if the `offset` is out of bounds.
    pub(crate) fn offset_to_display_point(&self, offset: usize) -> DisplayPoint {
        let row = self.text.offset_to_point(offset).row;
        let start = self.text.line_start_offset(row);
        let line = &self.lines[row];

        let wrapped_row = self.lines.prefix_wraps(row);
        let local_offset = offset.saturating_sub(start);
        if let Some(table) = &line.table {
            if let Some(cell) = table
                .cells
                .iter()
                .find(|c| local_offset <= c.source.end)
                .or_else(|| table.cells.last())
            {
                let display = cell.mapped.source_to_display[local_offset
                    .saturating_sub(cell.source.start)
                    .min(cell.source.len())];
                let wrap = cell
                    .wraps
                    .partition_point(|r| r.end <= display)
                    .min(cell.wraps.len() - 1);
                return DisplayPoint::new(wrapped_row + wrap, wrap, local_offset);
            }
        }
        let local_offset = line
            .mapped
            .as_ref()
            .map_or(local_offset, |m| m.source_to_display[local_offset]);
        for (ix, range) in line.wrapped_lines.iter().enumerate() {
            if range.contains(&local_offset) {
                return DisplayPoint::new(
                    wrapped_row + ix,
                    ix,
                    local_offset.saturating_sub(range.start),
                );
            }
        }

        // Otherwise return the eof of the line.
        let last_range = line.wrapped_lines.last().unwrap_or(&(0..0));
        let ix = line.lines_len().saturating_sub(1);
        return DisplayPoint::new(wrapped_row + ix, ix, last_range.len());
    }

    /// Return byte offset in the text from the given display point (with soft wrap).
    ///
    /// Panics if the `point.row` is out of bounds.
    pub(crate) fn display_point_to_offset(&self, point: DisplayPoint) -> usize {
        if point.row >= self.lines.wraps() {
            return self.text.len();
        }
        let row = self.lines.row_at_wrap(point.row);
        let line = &self.lines[row];
        if line.table.is_some() {
            return self.text.floor_char_boundary(
                self.text.line_start_offset(row) + point.column.min(line.len()),
            );
        }
        let local_row = point.row - self.lines.prefix_wraps(row);
        let range = &line.wrapped_lines[local_row];
        let display = (range.start + point.column).min(range.end);
        let source = line
            .mapped
            .as_ref()
            .map_or(display, |m| m.display_to_source[display]);
        let offset = self.text.line_start_offset(row) + source;
        if line.measured {
            offset
        } else {
            self.text.floor_char_boundary(offset)
        }
    }
    pub(crate) fn display_point_to_point(&self, point: DisplayPoint) -> tree_sitter::Point {
        let offset = self.display_point_to_offset(point);
        self.text.offset_to_point(offset)
    }

    pub(crate) fn point_to_display_point(&self, point: tree_sitter::Point) -> DisplayPoint {
        let offset = self.text.point_to_offset(point);
        self.offset_to_display_point(offset)
    }
}

/// The actually display point in the text.
///
/// This is usually used to describe the
/// position in the text with `soft-wrap` mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisplayPoint {
    /// The 0-based soft wrapped row index in the text.
    pub row: usize,
    /// The 0-based row index in local line (include first line).
    ///
    /// This value only valid when return from [`TextWrapper::offset_to_display_point`], otherwise it will be ignored.
    pub local_row: usize,
    /// The 0-based column byte index in the display line (with soft wrap).
    pub column: usize,
}

impl DisplayPoint {
    pub fn new(row: usize, local_row: usize, column: usize) -> Self {
        Self {
            row,
            local_row,
            column,
        }
    }
}

/// The layout info of a line with soft wrapped lines.
pub(crate) struct LineLayout {
    pub(super) table: Option<TablePaintLayout>,
    pub(super) task: Option<(usize, bool, Pixels)>,
    /// Total bytes length of this line.
    len: usize,
    pub(super) mapped: Option<std::rc::Rc<super::live_preview::MappedText>>,
    pub(super) scale: f32,
    /// The soft wrapped lines of this line (Include the first line).
    pub(crate) wrapped_lines: SmallVec<[ShapedLine; 1]>,
    pub(crate) longest_width: Pixels,
    pub(super) indent: Pixels,
    pub(super) quote_depth: usize,
    pub(super) segment_start: usize,
    pub(super) segment_ranges: Option<std::rc::Rc<Vec<Range<usize>>>>,
}

pub(super) struct TablePaintLayout {
    pub scroll: std::rc::Rc<std::cell::Cell<Pixels>>,
    pub viewport: Pixels,
    pub cells: Vec<(Range<usize>, Pixels, Pixels, LineLayout)>,
    pub height: usize,
}

impl LineLayout {
    pub(super) fn task_bounds(&self, line_height: Pixels) -> Option<gpui::Bounds<Pixels>> {
        let (state, _, width) = self.task?;
        let origin =
            self.position_for_index(state, line_height)? + point(px(0.), width * (0.4 / 0.875));
        Some(gpui::Bounds::new(origin, size(width, width)))
    }
    pub(crate) fn new() -> Self {
        Self {
            table: None,
            task: None,
            len: 0,
            mapped: None,
            scale: 1.,
            longest_width: px(0.),
            indent: px(0.),
            quote_depth: 0,
            segment_start: 0,
            segment_ranges: None,
            wrapped_lines: SmallVec::new(),
        }
    }

    pub(crate) fn lines(mut self, wrapped_lines: SmallVec<[ShapedLine; 1]>) -> Self {
        self.set_wrapped_lines(wrapped_lines);
        self
    }

    pub(crate) fn set_wrapped_lines(&mut self, wrapped_lines: SmallVec<[ShapedLine; 1]>) {
        self.len = self
            .segment_ranges
            .as_ref()
            .and_then(|r| r.last())
            .map_or_else(|| wrapped_lines.iter().map(|l| l.len).sum(), |r| r.end);
        let width = wrapped_lines
            .iter()
            .map(|l| l.width)
            .max()
            .unwrap_or_default();
        self.longest_width = width + self.indent;
        self.wrapped_lines = wrapped_lines;
    }

    #[inline]
    pub(super) fn len(&self) -> usize {
        self.mapped
            .as_ref()
            .map_or(self.len, |m| m.source_to_display.len() - 1)
    }

    /// Get the position (x, y) for the given index in this line layout.
    ///
    /// - The `offset` is a local byte index in this line layout.
    /// - The return value is relative to the top-left corner of this line layout, start from (0, 0)
    pub(crate) fn position_for_index(
        &self,
        offset: usize,
        line_height: Pixels,
    ) -> Option<Point<Pixels>> {
        if let Some(table) = &self.table {
            if offset > self.len {
                return None;
            }
            let (range, x, _, cell) = table
                .cells
                .iter()
                .find(|(r, _, _, _)| offset <= r.end)
                .or_else(|| table.cells.last())?;
            return cell
                .position_for_index(
                    offset.saturating_sub(range.start).min(range.len()),
                    line_height,
                )
                .map(|p| p + point(*x - table.scroll.get(), px(0.)));
        }
        let line_height = line_height * self.scale;
        let offset = self
            .mapped
            .as_ref()
            .map_or(Some(offset), |m| m.source_to_display.get(offset).copied())?;
        if let Some(ranges) = &self.segment_ranges {
            let end = ranges.last()?.end;
            if offset > end {
                return None;
            }
            let row = ranges
                .partition_point(|r| r.end <= offset)
                .min(ranges.len() - 1);
            let x = row
                .checked_sub(self.segment_start)
                .and_then(|i| self.wrapped_lines.get(i))
                .map_or(px(0.), |line| line.x_for_index(offset - ranges[row].start));
            return Some(point(x + self.indent, row as f32 * line_height));
        }
        let mut acc_len = 0;
        for (i, line) in self.wrapped_lines.iter().enumerate() {
            if offset <= acc_len + line.len {
                return Some(point(
                    self.indent + line.x_for_index(offset - acc_len),
                    i as f32 * line_height,
                ));
            }
            acc_len += line.len;
        }
        None
    }
    /// Get the closest index for the given x in this line layout.
    pub(super) fn closest_index_for_x(&self, x: Pixels) -> usize {
        if self.table.is_some() {
            return self
                .closest_index_for_position(point(x, px(0.)), px(20.))
                .unwrap_or(0);
        }
        let x = x - self.indent;
        let mut acc_len = 0;
        for (i, line) in self.wrapped_lines.iter().enumerate() {
            let is_last = i + 1 == self.wrapped_lines.len();
            if x <= line.width {
                let mut ix = line.closest_index_for_x(x);
                if !is_last && ix == line.len {
                    // For soft wrap line, we can't put the cursor at the end of the line.
                    let c_len = line.text.chars().last().map(|c| c.len_utf8()).unwrap_or(0);
                    ix = ix.saturating_sub(c_len);
                }

                return self.source_index(acc_len + ix);
            }
            acc_len += line.len;
        }

        self.source_index(acc_len)
    }

    /// Get the index for the given position (x, y) in this line layout.
    ///
    /// The `pos` is relative to the top-left corner of this line layout, start from (0, 0)
    /// The return value is a local byte index in this line layout, start from 0.
    pub(super) fn closest_index_for_position(
        &self,
        pos: Point<Pixels>,
        line_height: Pixels,
    ) -> Option<usize> {
        if let Some(table) = &self.table {
            if pos.y < px(0.) || pos.y >= table.height * line_height {
                return None;
            }
            let pos = pos + point(table.scroll.get(), px(0.));
            let (range, x, _, cell) = table
                .cells
                .iter()
                .find(|(_, x, w, _)| pos.x < *x + *w)
                .or_else(|| table.cells.last())?;
            let y = pos
                .y
                .min(cell.size(line_height).height - px(0.01))
                .max(px(0.));
            return cell
                .closest_index_for_position(point(pos.x - *x, y), line_height)
                .map(|i| range.start + i.min(range.len()));
        }
        let mut offset = self
            .segment_ranges
            .as_ref()
            .map_or(0, |r| r[self.segment_start].start);
        let line_height = line_height * self.scale;
        let mut line_top = self.segment_start as f32 * line_height;
        for (i, line) in self.wrapped_lines.iter().enumerate() {
            let is_last = i + 1 == self.wrapped_lines.len();
            let line_bottom = line_top + line_height;
            if pos.y >= line_top && pos.y < line_bottom {
                let mut ix = line.closest_index_for_x(pos.x - self.indent);
                if !is_last && ix == line.len {
                    // For soft wrap line, we can't put the cursor at the end of the line.
                    let c_len = line.text.chars().last().map(|c| c.len_utf8()).unwrap_or(0);
                    ix = ix.saturating_sub(c_len);
                }
                return Some(self.source_index(offset + ix));
            }

            offset += line.len;
            line_top = line_bottom;
        }

        None
    }

    pub(super) fn index_for_position(
        &self,
        pos: Point<Pixels>,
        line_height: Pixels,
    ) -> Option<usize> {
        if self.table.is_some() {
            return self.closest_index_for_position(pos, line_height);
        }
        let mut offset = self
            .segment_ranges
            .as_ref()
            .map_or(0, |r| r[self.segment_start].start);
        let line_height = line_height * self.scale;
        let mut line_top = self.segment_start as f32 * line_height;
        for line in self.wrapped_lines.iter() {
            let line_bottom = line_top + line_height;
            if pos.y >= line_top && pos.y < line_bottom {
                let ix = line.index_for_x(pos.x - self.indent)?;
                return Some(self.source_index(offset + ix));
            }

            offset += line.len;
            line_top = line_bottom;
        }

        None
    }

    fn source_index(&self, display: usize) -> usize {
        self.mapped.as_ref().map_or(display, |m| {
            m.display_to_source[display.min(m.display_to_source.len() - 1)]
        })
    }

    pub(super) fn size(&self, line_height: Pixels) -> Size<Pixels> {
        if let Some(table) = &self.table {
            return size(self.longest_width, table.height * line_height);
        }
        let line_height = line_height * self.scale;
        size(
            self.longest_width,
            self.segment_ranges
                .as_ref()
                .map_or(self.wrapped_lines.len(), |r| r.len())
                * line_height,
        )
    }

    pub(super) fn paint(
        &self,
        pos: Point<Pixels>,
        line_height: Pixels,
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(table) = &self.table {
            use crate::ActiveTheme as _;
            window.with_content_mask(
                Some(gpui::ContentMask {
                    bounds: gpui::Bounds::new(
                        pos,
                        size(table.viewport, table.height * line_height),
                    ),
                }),
                |window| {
                    for (_, x, width, cell) in &table.cells {
                        let origin = pos + point(*x - table.scroll.get(), px(0.));
                        let bounds =
                            gpui::Bounds::new(origin, size(*width, table.height * line_height));
                        // Transparent fill preserves document selection painted underneath.
                        window.paint_quad(gpui::quad(
                            bounds,
                            px(0.),
                            gpui::transparent_black(),
                            px(1.),
                            cx.theme().border,
                            gpui::BorderStyle::Solid,
                        ));
                        cell.paint(origin, line_height, window, cx);
                    }
                },
            );
            return;
        }
        let line_height = line_height * self.scale;
        if self.quote_depth > 0 {
            use crate::ActiveTheme as _;
            let unit = self.indent / self.quote_depth as f32;
            for level in 0..self.quote_depth {
                window.paint_quad(gpui::fill(
                    gpui::Bounds::new(
                        pos + point(
                            unit * (level as f32 + 0.25),
                            self.segment_start as f32 * line_height,
                        ),
                        size(px(2.), self.wrapped_lines.len() as f32 * line_height),
                    ),
                    cx.theme().border,
                ));
            }
        }
        for (ix, line) in self.wrapped_lines.iter().enumerate() {
            _ = line.paint(
                pos + point(self.indent, (ix + self.segment_start) * line_height),
                line_height,
                window,
                cx,
            );
        }
        if let Some(mut bounds) = self.task_bounds(line_height / self.scale) {
            use crate::ActiveTheme as _;
            use gpui::Half as _;
            bounds.origin += pos;
            let checked = self.task.unwrap().1;
            window.paint_quad(gpui::quad(
                bounds,
                cx.theme().radius.half(),
                if checked {
                    cx.theme().primary
                } else {
                    gpui::transparent_black()
                },
                px(1.),
                cx.theme().primary,
                gpui::BorderStyle::Solid,
            ));
            if checked {
                let icon_size = bounds.size.width * (0.5 / 0.875);
                let inset = (bounds.size.width - icon_size) / 2.;
                let _ = window.paint_svg(
                    gpui::Bounds::new(
                        bounds.origin + point(inset, inset),
                        size(icon_size, icon_size),
                    ),
                    "icons/check.svg".into(),
                    Default::default(),
                    cx.theme().primary_foreground,
                    cx,
                );
            }
        }
    }
}

impl LineLayout {
    pub(super) fn table_row(
        len: usize,
        cells: Vec<(Range<usize>, Pixels, Pixels, LineLayout)>,
        height: usize,
        _header: bool,
        scroll: std::rc::Rc<std::cell::Cell<Pixels>>,
        viewport: Pixels,
    ) -> Self {
        let mut line = Self::new();
        line.len = len;
        line.longest_width = cells
            .iter()
            .map(|(_, _, w, _)| *w)
            .fold(px(0.), |a, b| a + b);
        line.longest_width = line.longest_width.min(viewport);
        line.table = Some(TablePaintLayout {
            cells,
            height,
            scroll,
            viewport,
        });
        line
    }
}

/// Give the mapped object replacement character an exact advance, independent of font or state.
pub(super) fn reserve_task_slot(line: &mut ShapedLine, offset: usize, width: Pixels) {
    let delta = width - (line.x_for_index(offset + 3) - line.x_for_index(offset));
    let mut runs = line.runs.clone();
    for run in &mut runs {
        for glyph in &mut run.glyphs {
            if glyph.index > offset {
                glyph.position.x += delta;
            }
        }
    }
    **line = std::sync::Arc::new(gpui::LineLayout {
        font_size: line.font_size,
        width: line.width + delta,
        ascent: line.ascent,
        descent: line.descent,
        runs,
        len: line.len,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Boundary, FontFeatures, FontStyle, FontWeight, px};

    #[test]
    fn test_update() {
        let font = gpui::Font {
            family: "Arial".into(),
            weight: FontWeight::default(),
            style: FontStyle::Normal,
            features: FontFeatures::default(),
            fallbacks: None,
        };

        let mut wrapper = TextWrapper::new(font, px(14.), None);
        let mut text = Rope::from(
            "Hello, 世界!\r\nThis is second line.\nThis is third line.\n这里是第 4 行。",
        );

        fn fake_wrap_line(_line: &str, _wrap_width: Pixels, _: Option<usize>) -> Vec<Boundary> {
            vec![]
        }

        #[track_caller]
        fn assert_wrapper_lines(text: &Rope, wrapper: &TextWrapper, expected_lines: &[&[&str]]) {
            let mut actual_lines = vec![];
            let mut offset = 0;
            for line in wrapper.lines.iter() {
                actual_lines.push(
                    line.wrapped_lines
                        .iter()
                        .map(|range| text.slice(offset + range.start..offset + range.end))
                        .collect::<Vec<_>>(),
                );
                // +1 \n
                offset += line.len() + 1;
            }
            assert_eq!(actual_lines, expected_lines);
        }

        wrapper._update(&text, &(0..text.len()), &text, &mut fake_wrap_line);
        assert_eq!(wrapper.lines.len(), 4);
        assert_wrapper_lines(
            &text,
            &wrapper,
            &[
                &["Hello, 世界!\r"],
                &["This is second line."],
                &["This is third line."],
                &["这里是第 4 行。"],
            ],
        );

        // Add a new text to end
        let range = text.len()..text.len();
        let new_text = "New text";
        text.replace(range.clone(), new_text);
        wrapper._update(&text, &range, &Rope::from(new_text), &mut fake_wrap_line);
        assert_eq!(
            text.to_string(),
            "Hello, 世界!\r\nThis is second line.\nThis is third line.\n这里是第 4 行。New text"
        );
        assert_eq!(wrapper.lines.len(), 4);
        assert_eq!(wrapper.lines.len(), 4);
        assert_wrapper_lines(
            &text,
            &wrapper,
            &[
                &["Hello, 世界!\r"],
                &["This is second line."],
                &["This is third line."],
                &["这里是第 4 行。New text"],
            ],
        );

        // Replace first line `Hello` to `AAA`
        let range = 0..5;
        let new_text = "AAA";
        text.replace(range.clone(), new_text);
        wrapper._update(&text, &range, &Rope::from(new_text), &mut fake_wrap_line);
        assert_eq!(
            text.to_string(),
            "AAA, 世界!\r\nThis is second line.\nThis is third line.\n这里是第 4 行。New text"
        );
        assert_eq!(wrapper.lines.len(), 4);
        assert_wrapper_lines(
            &text,
            &wrapper,
            &[
                &["AAA, 世界!\r"],
                &["This is second line."],
                &["This is third line."],
                &["这里是第 4 行。New text"],
            ],
        );

        // Remove the second line
        let start_offset = text.line_start_offset(1);
        let end_offset = text.line_end_offset(1);
        let range = start_offset..end_offset + 1;
        text.replace(range.clone(), "");
        wrapper._update(&text, &range, &Rope::from(""), &mut fake_wrap_line);
        assert_eq!(
            text.to_string(),
            "AAA, 世界!\r\nThis is third line.\n这里是第 4 行。New text"
        );
        assert_eq!(wrapper.lines.len(), 3);
        assert_wrapper_lines(
            &text,
            &wrapper,
            &[
                &["AAA, 世界!\r"],
                &["This is third line."],
                &["这里是第 4 行。New text"],
            ],
        );

        // Replace the first 2 lines to "This is a new line."
        let range = text.line_start_offset(0)..text.line_end_offset(1) + 1;
        let new_text = "This is a new line.\nThis is new line 2.\n";
        text.replace(range.clone(), new_text);
        wrapper._update(&text, &range, &Rope::from(new_text), &mut fake_wrap_line);
        assert_eq!(
            text.to_string(),
            "This is a new line.\nThis is new line 2.\n这里是第 4 行。New text"
        );
        assert_eq!(wrapper.lines.len(), 3);
        assert_wrapper_lines(
            &text,
            &wrapper,
            &[
                &["This is a new line."],
                &["This is new line 2."],
                &["这里是第 4 行。New text"],
            ],
        );

        // Add a new line at the end
        let range = text.len()..text.len();
        let new_text = "\nThis is a new line at the end.";
        text.replace(range.clone(), new_text);
        wrapper._update(&text, &range, &Rope::from(new_text), &mut fake_wrap_line);
        assert_eq!(
            text.to_string(),
            "This is a new line.\nThis is new line 2.\n这里是第 4 行。New text\nThis is a new line at the end."
        );
        assert_eq!(wrapper.lines.len(), 4);
        assert_wrapper_lines(
            &text,
            &wrapper,
            &[
                &["This is a new line."],
                &["This is new line 2."],
                &["这里是第 4 行。New text"],
                &["This is a new line at the end."],
            ],
        );

        // Add a new line at the beginning
        let range = 0..0;
        let new_text = "This is a new line at the beginning.\n";
        text.replace(range.clone(), new_text);
        wrapper._update(&text, &range, &Rope::from(new_text), &mut fake_wrap_line);
        assert_eq!(
            text.to_string(),
            "This is a new line at the beginning.\nThis is a new line.\nThis is new line 2.\n这里是第 4 行。New text\nThis is a new line at the end."
        );
        assert_eq!(wrapper.lines.len(), 5);
        assert_wrapper_lines(
            &text,
            &wrapper,
            &[
                &["This is a new line at the beginning."],
                &["This is a new line."],
                &["This is new line 2."],
                &["这里是第 4 行。New text"],
                &["This is a new line at the end."],
            ],
        );

        // Remove all to at least one line in `lines`.
        let range = 0..text.len();
        let new_text = "";
        text.replace(range.clone(), new_text);
        wrapper._update(&text, &range, &Rope::from(new_text), &mut fake_wrap_line);
        assert_eq!(text.to_string(), "");
        assert_eq!(wrapper.lines.len(), 1);
        assert_eq!(*wrapper.lines[0].wrapped_lines, vec![0..0]);

        // Test update_all
        let range = 0..text.len();
        let new_text = "This is a full text.\nThis is a second line.";
        text.replace(range.clone(), new_text);
        wrapper._update(&text, &range, &text, &mut fake_wrap_line);
        assert_eq!(
            text.to_string(),
            "This is a full text.\nThis is a second line."
        );
        assert_eq!(wrapper.lines.len(), 2);
    }

    #[test]
    fn test_line_layout() {
        let mut line_layout = LineLayout::new();

        let line1 = ShapedLine::default().with_len(100);
        let line2 = ShapedLine::default().with_len(50);
        let wrapped_lines = smallvec::smallvec![line1, line2];
        line_layout.set_wrapped_lines(wrapped_lines);
        assert_eq!(line_layout.len(), 150);
        assert_eq!(line_layout.wrapped_lines.len(), 2);
    }

    #[test]
    fn test_offset_to_display_point() {
        let font = gpui::Font {
            family: "Arial".into(),
            weight: FontWeight::default(),
            style: FontStyle::Normal,
            features: FontFeatures::default(),
            fallbacks: None,
        };

        let mut wrapper = TextWrapper::new(font, px(14.), None);
        wrapper.text = Rope::from(
            "Hello, 世界!\r\nThis is second line.\nThis is third line.\n这里是第 4 行。",
        );
        wrapper.lines = vec![
            // range: 0..15
            LineItem {
                table: None,
                line: Rope::from("Hello, 世界!\r"),
                wrapped_lines: std::rc::Rc::new(vec![0..15]),
                presentation: None,
                mapped: None,
                scale: 1.,
                cache: Default::default(),
                measured: true,
            },
            // range: 16..36
            LineItem {
                table: None,
                line: Rope::from("This is second line."),
                wrapped_lines: std::rc::Rc::new(vec![0..10, 10..20]),
                presentation: None,
                mapped: None,
                scale: 1.,
                cache: Default::default(),
                measured: true,
            },
            // range: 37..56
            LineItem {
                table: None,
                line: Rope::from("This is third line."),
                wrapped_lines: std::rc::Rc::new(vec![0..9, 9..15, 15..20]),
                presentation: None,
                mapped: None,
                scale: 1.,
                cache: Default::default(),
                measured: true,
            },
            // range: 57..79
            LineItem {
                table: None,
                line: Rope::from("这里是第 4 行。"),
                wrapped_lines: std::rc::Rc::new(vec![0..22]),
                presentation: None,
                mapped: None,
                scale: 1.,
                cache: Default::default(),
                measured: true,
            },
        ]
        .into();

        assert_eq!(
            wrapper.offset_to_display_point(12),
            DisplayPoint::new(0, 0, 12)
        );
        assert_eq!(
            wrapper.offset_to_display_point(15),
            DisplayPoint::new(0, 0, 15)
        );

        assert_eq!(
            wrapper.offset_to_display_point(16),
            DisplayPoint::new(1, 0, 0)
        );
        assert_eq!(
            wrapper.offset_to_display_point(21),
            DisplayPoint::new(1, 0, 5)
        );
        assert_eq!(
            wrapper.offset_to_display_point(27),
            DisplayPoint::new(2, 1, 1)
        );
        assert_eq!(
            wrapper.offset_to_display_point(37),
            DisplayPoint::new(3, 0, 0)
        );
        assert_eq!(
            wrapper.offset_to_display_point(54),
            DisplayPoint::new(5, 2, 2)
        );
        assert_eq!(
            wrapper.offset_to_display_point(59),
            DisplayPoint::new(6, 0, 2)
        );

        assert_eq!(
            wrapper.display_point_to_offset(DisplayPoint::new(6, 0, 2)),
            59
        );
        assert_eq!(
            wrapper.display_point_to_offset(DisplayPoint::new(5, 2, 2)),
            54
        );
        assert_eq!(
            wrapper.display_point_to_offset(DisplayPoint::new(3, 0, 0)),
            37
        );
        assert_eq!(
            wrapper.display_point_to_offset(DisplayPoint::new(2, 1, 1)),
            27
        );
        assert_eq!(
            wrapper.display_point_to_offset(DisplayPoint::new(1, 0, 5)),
            21
        );
        assert_eq!(
            wrapper.display_point_to_offset(DisplayPoint::new(1, 0, 0)),
            16
        );
        assert_eq!(
            wrapper.display_point_to_offset(DisplayPoint::new(0, 0, 15)),
            15
        );
    }
}

#[cfg(test)]
impl LineItem {
    pub(super) fn diagnostic_line(n: usize) -> Self {
        Self {
            table: None,
            line: Rope::from("x".repeat(n)),
            measured: true,
            presentation: None,
            mapped: None,
            scale: 1.,
            cache: Default::default(),
            wrapped_lines: std::rc::Rc::new((0..n).map(|i| i..i + 1).collect()),
        }
    }
}

#[cfg(test)]
mod preview_tests {
    use super::*;
    #[test]
    fn mapping_survives_wrapping_and_incremental_edits() {
        let mut quotes = TextWrapper::new(gpui::font("Arial"), px(20.), Some(px(160.)));
        quotes.live_preview = true;
        let text = Rope::from("> > **你好 café**");
        quotes._update(&text, &(0..0), &text, &mut |_, width, _| {
            assert_eq!(width, px(120.), "quote indentation reduces wrapping width");
            vec![]
        });
        assert_eq!(
            quotes.lines[0].presentation.as_ref().unwrap().quote_depth,
            2
        );
        let mut wrapper = TextWrapper::new(gpui::font("Arial"), px(14.), None);
        wrapper.live_preview = true;
        wrapper.reveal_row = 1;
        let mut text = Rope::from("#**caf\u{e9}**\n- \u{4f60}\u{597d}\nlast");
        wrapper._update(&text, &(0..0), &text, &mut |_, _, _| vec![]);
        assert_eq!(wrapper.lines[0].mapped.as_ref().unwrap().text, "caf\u{e9}");
        let unchanged = wrapper.lines[0].mapped.clone().unwrap();
        let at = text.line_start_offset(2);
        text.replace(at..at, "x");
        wrapper._update(&text, &(at..at), &Rope::from("x"), &mut |_, _, _| vec![]);
        assert!(std::rc::Rc::ptr_eq(
            &unchanged,
            wrapper.lines[0].mapped.as_ref().unwrap()
        ));
        let point = wrapper.offset_to_display_point(7);
        let offset = wrapper.display_point_to_offset(point);
        assert!(text.to_string().is_char_boundary(offset));
        assert_eq!(wrapper.lines.len(), 3);
    }
}

#[cfg(test)]
mod table_layout_tests {
    use super::*;
    #[test]
    fn column_edits_refresh_active_table_widths_and_preserve_cells() {
        use super::super::table::Command;

        for newline in ["\n", "\r\n"] {
            for command in [
                Command::ColumnLeft,
                Command::ColumnRight,
                Command::DeleteColumn,
            ] {
                let original = Rope::from(
                    ["| name | value |", "| --- | ---: |", "| café | 你好 |"].join(newline),
                );
                let mut wrapper = TextWrapper::new(gpui::font("Arial"), px(16.), Some(px(800.)));
                wrapper.live_preview = true;
                wrapper.tables.rebuild(&original);
                wrapper.active_offset = wrapper.tables.tables[0].rows[1].cells[0].start;
                wrapper._update(&original, &(0..0), &original, &mut |_, _, _| vec![]);
                wrapper.layout_tables(&mut |_, _| vec![]);
                let edit = wrapper.tables.tables[0]
                    .edit(&original, 1, 0, command)
                    .unwrap();
                let mut edited = original.clone();
                edited.replace(edit.range.clone(), &edit.text);

                // Apply the edit, undo it, and redo it while the caret stays in the table.
                for next in [&edited, &original, &edited] {
                    let old = wrapper.text.clone();
                    let range = 0..old.len();
                    wrapper.active_offset = next.len();
                    wrapper.tables.update(&old, next, &range, next);
                    wrapper._update(next, &range, next, &mut |_, _, _| vec![]);
                    wrapper.layout_tables(&mut |_, _| vec![]);
                    let expected: Vec<&str> = if next == &original {
                        vec!["café", "你好"]
                    } else {
                        match command {
                            Command::ColumnLeft => vec!["", "café", "你好"],
                            Command::ColumnRight => vec!["café", "", "你好"],
                            Command::DeleteColumn => vec!["你好"],
                            _ => unreachable!(),
                        }
                    };
                    let table = &wrapper.tables.tables[0];
                    for row in &table.rows {
                        let layout = wrapper.lines[next.offset_to_point(row.source.start).row]
                            .table
                            .as_ref()
                            .unwrap();
                        assert_eq!(layout.widths.len(), expected.len(), "{command:?}");
                        assert_eq!(layout.cells.len(), expected.len(), "{command:?}");
                    }
                    let body = wrapper.lines[2].table.as_ref().unwrap();
                    assert_eq!(
                        body.cells
                            .iter()
                            .map(|c| c.mapped.text.as_str())
                            .collect::<Vec<_>>(),
                        expected
                    );
                    assert_eq!(
                        table.rows[1]
                            .cells
                            .iter()
                            .map(|c| next.slice(c.clone()).to_string())
                            .collect::<Vec<_>>(),
                        expected
                    );
                }
            }
        }
    }

    #[test]
    fn table_rows_share_widths_hide_separator_and_map_unicode() {
        let text = Rope::from(
            "before\n\n| **name** | value |\n| --- | ---: |\n| café | 你好 words words |\n\nafter",
        );
        let mut wrapper = TextWrapper::new(gpui::font("Arial"), px(16.), Some(px(180.)));
        wrapper.live_preview = true;
        wrapper.tables.rebuild(&text);
        let at = wrapper.tables.tables[0].rows[1].cells[1].start;
        wrapper.active_offset = at;
        wrapper._update(&text, &(0..0), &text, &mut |_, _, _| vec![]);
        let mut wrap = |text: &str, _| {
            text.char_indices()
                .enumerate()
                .filter(|(i, _)| *i > 0 && i % 5 == 0)
                .map(|(_, (at, _))| at)
                .collect()
        };
        wrapper.layout_tables(&mut wrap);
        assert_eq!(wrapper.lines[3].lines_len(), 0);
        assert_eq!(wrapper.lines.prefix(4), wrapper.lines.prefix(3));
        let widths = wrapper.lines[2].table.as_ref().unwrap().widths.clone();
        assert_eq!(widths, wrapper.lines[4].table.as_ref().unwrap().widths);
        assert_eq!(
            wrapper.lines[2].table.as_ref().unwrap().cells[0]
                .mapped
                .text,
            "name"
        );
        assert!(wrapper.lines[4].lines_len() > 1);
        for cell in &wrapper.lines[4].table.as_ref().unwrap().cells {
            let source = text
                .slice(
                    text.line_start_offset(4) + cell.source.start
                        ..text.line_start_offset(4) + cell.source.end,
                )
                .to_string();
            for (i, _) in cell.mapped.text.char_indices() {
                assert!(source.is_char_boundary(cell.mapped.display_to_source[i]));
            }
        }
        let mut changed = text.clone();
        changed.replace(at..at, "long inserted text ");
        let inserted = Rope::from("long inserted text ");
        wrapper.tables.update(&text, &changed, &(at..at), &inserted);
        wrapper._update(&changed, &(at..at), &inserted, &mut |_, _, _| vec![]);
        wrapper.layout_tables(&mut wrap);
        assert_eq!(
            widths,
            wrapper.lines[4].table.as_ref().unwrap().widths,
            "columns stay stable during editing"
        );
        for offset in [at, at + 4] {
            assert_eq!(
                wrapper.display_point_to_offset(wrapper.offset_to_display_point(offset)),
                offset
            );
        }
    }
}
