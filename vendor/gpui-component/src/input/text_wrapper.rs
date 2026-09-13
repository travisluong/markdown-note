use std::ops::Range;

use gpui::{App, Font, LineFragment, Pixels, Point, ShapedLine, Size, Window, point, px, size};
use ropey::Rope;
use smallvec::SmallVec;

use crate::input::RopeExt;

#[derive(Debug, Clone, PartialEq)]
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
    revision: std::cell::Cell<u64>,
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
    pub(super) code_block: Option<super::fenced_code::Decoration>,
    pub(super) image: Option<ImageLayout>,
    pub(super) table: Option<std::rc::Rc<TableRowLayout>>,
    /// The original line text, without end `\n`.
    pub(super) line: Rope,
    measured: bool,
    syntax_revision: u64,
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
        if let Some(image) = &self.image {
            return image.rows
                + if image.reveal {
                    self.wrapped_lines.len()
                } else {
                    0
                };
        }
        self.table
            .as_ref()
            .map_or(self.wrapped_lines.len(), |t| t.height)
    }

    pub(super) fn height_units(&self) -> f32 {
        if self.code_block.as_ref().is_some_and(|c| c.hidden) {
            return 0.;
        }
        self.lines_len() as f32 * self.scale
            + self
                .code_block
                .as_ref()
                .map_or(0., |c| c.top() + c.bottom())
    }

    /// Get the height of this line item with given line height.
    pub(super) fn height(&self, line_height: Pixels) -> Pixels {
        self.height_units() * line_height
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
    pub(super) inline_code: std::sync::Arc<Vec<super::inline_code::CodeSpan>>,
    pub(super) code: super::fenced_code::Index,
    retired_code: Vec<super::fenced_code::Index>,
    pub(super) image_document: Option<std::path::PathBuf>,
    pub(super) images: std::collections::BTreeMap<usize, String>,
    pub(super) links: super::preview_syntax::Links,
    retired_links: Vec<super::preview_syntax::Links>,
    pub(super) analysis: super::preview_syntax::Queue,
    pub(super) syntax_revision: u64,
    copied_bytes: usize,
    visited_table_rows: usize,
    pub(super) tables: super::table::TableIndex,
    pub(super) trusted_table_edit: bool,
    active_offset: usize,
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
    code_selection: Range<usize>,
    code_padding: f32,
    code_font: Option<Font>,
    code_font_size: Pixels,
    code_inset: Pixels,
}

#[allow(unused)]
impl TextWrapper {
    pub(super) fn new(font: Font, font_size: Pixels, wrap_width: Option<Pixels>) -> Self {
        Self {
            code: Default::default(),
            inline_code: Default::default(),
            retired_code: Vec::new(),
            image_document: None,
            images: Default::default(),
            links: Default::default(),
            retired_links: Vec::new(),
            analysis: Default::default(),
            syntax_revision: 0,
            copied_bytes: 0,
            visited_table_rows: 0,
            tables: Default::default(),
            trusted_table_edit: false,
            active_offset: usize::MAX,
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
            code_selection: 0..0,
            code_padding: 0.6,
            code_font: None,
            code_font_size: font_size,
            code_inset: px(12.),
        }
    }

    pub(super) fn set_live_preview(&mut self, enabled: bool, cx: &mut App) {
        self.inline_code = Default::default();
        self.images.clear();
        self.live_preview = enabled;
        self.retired_code.push(std::mem::take(&mut self.code));
        self.analysis.reset();
        self.links = Default::default();
        self.tables = Default::default();
        self.syntax_revision += 1;
        if enabled {
            self.analysis.request(&self.text);
        }
        self.table_widths.clear();
        self.update_all(&self.text.clone(), cx);
    }

    pub(super) fn measure(&mut self, rows: Range<usize>, cx: &mut App) {
        let text = self.text.clone();
        for row in rows.clone() {
            if self
                .lines
                .get(row)
                .is_some_and(|l| !l.measured || l.syntax_revision != self.syntax_revision)
            {
                let offset = text.line_start_offset(row);
                self.measure_line(row, cx);
            }
        }
        if self.live_preview {
            let mut wrapper = cx
                .text_system()
                .line_wrapper(self.font.clone(), self.font_size);
            self.layout_tables_in(rows, &mut |text, width| {
                wrapper
                    .wrap_line(&[LineFragment::text(text)], width)
                    .map(|b| b.ix)
                    .collect()
            });
        }
    }

    pub(super) fn configure_code(&mut self, font: Font, font_size: Pixels, inset: Pixels) {
        if self.code_font.as_ref() != Some(&font)
            || self.code_font_size != font_size
            || self.code_inset != inset
        {
            self.code_font = Some(font);
            self.code_font_size = font_size;
            self.code_inset = inset;
            if self.live_preview {
                self.syntax_revision += 1;
            }
        }
    }
    pub(super) fn code_width(&self) -> Pixels {
        self.wrap_width.unwrap_or(px(800.))
    }
    pub(super) fn reveal_code(&mut self, selection: Range<usize>, padding: f32) {
        if !self.live_preview {
            return;
        }
        let changed = self.code_padding != padding
            || (self.code_selection != selection
                && self.code.active_blocks(&self.code_selection)
                    != self.code.active_blocks(&selection));
        self.code_selection = selection;
        self.code_padding = padding;
        if changed {
            self.syntax_revision += 1;
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
                self.measure_line(row, cx);
            }
        }
    }

    pub(super) fn reveal_cell(&mut self, offset: usize, cx: &mut App) {
        let previous_table = self
            .tables
            .at(self.active_offset)
            .map(|(t, _, _)| t.cache_key());
        let next_table = self.tables.at(offset).map(|(t, _, _)| t.cache_key());
        if previous_table != next_table
            && let Some(key) = previous_table
        {
            self.table_widths.remove(&key);
        }
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
            self.measure_line(text.offset_to_point(offset).row, cx);
        }
    }

    #[inline]
    pub(super) fn set_default_text(&mut self, text: &Rope) {
        self.retired_code.push(std::mem::take(&mut self.code));
        self.analysis.reset();
        self.links = Default::default();
        self.tables = Default::default();
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
        if !range.is_empty() || new_text.len() > 0 {
            self.edit_syntax(changed_text, range, new_text);
        }
        let mut line_wrapper = cx
            .text_system()
            .line_wrapper(self.font.clone(), self.font_size);
        let task_size = self.task_size;
        self.estimating = new_text.lines_len() > 256;
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
        self.estimating = false;
        if self.live_preview && !self.code.snapshot.blocks.is_empty() {
            let start = changed_text
                .offset_to_point(range.start.min(changed_text.len()))
                .row;
            let end = (start + new_text.lines_len())
                .min(self.lines.len())
                .min(start + 256);
            for row in start..end {
                if let Some(line) = self.lines.get(row) {
                    let mut line = line.clone();
                    line.measured = false;
                    self.lines.splice(row..=row, vec![line]);
                }
            }
        }
    }

    /// Document changes never run a GFM parser on the input thread.
    fn edit_syntax(&mut self, changed: &Rope, range: &Range<usize>, inserted: &Rope) {
        if !self.live_preview {
            return;
        }
        if !self.inline_code.is_empty() {
            self.inline_code = Default::default();
            self.syntax_revision += 1;
        }
        let safe = super::preview_syntax::context_independent(&self.text, range, inserted);
        if safe {
            crate::markdown_images::rebase(&mut self.images, range, inserted.len());
        } else if !self.images.is_empty() {
            self.inline_code = Default::default();
            self.images.clear();
            self.syntax_revision += 1;
        }
        if !self.code.snapshot.blocks.is_empty() {
            let first = self.text.offset_to_point(range.start).row;
            let last = self.text.offset_to_point(range.end).row;
            let affected = self.text.line_start_offset(first)..self.text.line_end_offset(last);
            // Unlike general Markdown links/tables, fences are unaffected by
            // plain text on an adjacent line. Keep this inspection bounded.
            let independent = first == last
                && affected.len() <= 8192
                && inserted.len() <= 8192
                && !inserted.chars().any(|c| matches!(c, '\r' | '\n'))
                && self
                    .text
                    .slice(affected.clone())
                    .chars()
                    .chain(inserted.chars())
                    .all(|c| {
                        c.is_alphanumeric()
                            || matches!(c, ' ' | '\t' | '\r' | '\n' | '.' | ',' | '!' | '?')
                    });
            self.code
                .edit_context(range.clone(), inserted.len(), affected, independent);
            self.syntax_revision += 1;
        }
        self.analysis.edit(range.clone(), inserted.len(), safe);
        self.analysis.request(changed);
        if safe {
            self.links.edit(range.clone(), inserted.len());
        } else {
            if self.links.has_items() {
                self.retired_links.push(std::mem::take(&mut self.links));
            }
            self.syntax_revision += 1;
        }
        if !self.trusted_table_edit || !self.tables.replace_generated(range, inserted) {
            self.tables
                .update_preview(&self.text, changed, range, inserted);
        }
        if self.active_offset != usize::MAX {
            self.active_offset = if self.active_offset < range.start {
                self.active_offset
            } else if self.active_offset <= range.end {
                range.start + inserted.len()
            } else {
                self.active_offset
                    .saturating_add_signed(inserted.len() as isize - range.len() as isize)
            };
        }
    }

    pub(super) fn apply_analysis(&mut self, mut parsed: super::preview_syntax::Parsed) -> bool {
        let current = parsed.revision == self.analysis.revision && self.live_preview;
        let code = super::fenced_code::Index::new(parsed.code);
        if current {
            self.inline_code = parsed.inline_code;
            self.analysis.code_cache = code.snapshot.cache.clone();
            self.retired_code
                .push(std::mem::replace(&mut self.code, code));
        } else {
            self.retired_code.push(code);
        }
        let edits = self.analysis.rebasing(parsed.revision);
        self.analysis.finish(parsed.revision);
        let Some(edits) = edits.filter(|_| self.live_preview) else {
            self.retired_links
                .push(super::preview_syntax::Links::with_edits(
                    parsed.links,
                    Vec::new(),
                ));
            self.tables.retire_from(&mut parsed.tables);
            return false;
        };
        for edit in &edits {
            parsed.tables.rebase_preview(&edit.range, edit.inserted);
            crate::markdown_images::rebase(&mut parsed.images, &edit.range, edit.inserted);
        }
        self.images = parsed.images;
        self.retired_links.push(std::mem::replace(
            &mut self.links,
            super::preview_syntax::Links::with_edits(parsed.links, edits),
        ));
        let old_by_start = self
            .tables
            .tables
            .iter()
            .map(|t| (t.source.start, t))
            .collect::<std::collections::HashMap<_, _>>();
        let mut widths = std::collections::HashMap::new();
        let mut scrolls = std::collections::HashMap::new();
        for table in &parsed.tables.tables {
            if let Some(old) = old_by_start.get(&table.source.start) {
                if (old.column_characters == table.column_characters
                    || table.cell_at(self.active_offset).is_some())
                    && let Some(value) = self
                        .table_widths
                        .remove(&old.cache_key())
                        .filter(|w| w.len() == table.alignments.len())
                {
                    widths.insert(table.cache_key(), value);
                }
                if let Some(value) = self.table_scroll.remove(&old.cache_key()) {
                    scrolls.insert(table.cache_key(), value);
                }
            }
        }
        parsed.tables.revision = self.tables.revision + 1;
        parsed.tables.retire_from(&mut self.tables);
        self.tables = parsed.tables;
        self.table_widths = widths;
        self.table_scroll = scrolls;
        self.syntax_revision += 1;
        true
    }

    pub(super) fn take_retired(&mut self) -> Vec<super::preview_syntax::Retired> {
        let mut retired = self
            .retired_links
            .drain(..)
            .map(|links| super::preview_syntax::Retired {
                links,
                tables: Vec::new(),
                code: Vec::new(),
            })
            .collect::<Vec<_>>();
        let tables = self.tables.take_retired();
        if !tables.is_empty() {
            retired.push(super::preview_syntax::Retired {
                links: Default::default(),
                code: Vec::new(),
                tables,
            });
        }
        if !self.retired_code.is_empty() {
            retired.push(super::preview_syntax::Retired {
                links: Default::default(),
                tables: Vec::new(),
                code: std::mem::take(&mut self.retired_code),
            });
        }
        retired
    }

    fn measure_line(&mut self, row: usize, cx: &mut App) {
        let text = self.text.clone();
        if row >= text.lines_len() {
            return;
        }
        let offset = text.line_start_offset(row);
        let font = if self.live_preview && self.code.line(offset).is_some() {
            self.code_font.clone().unwrap_or_else(|| self.font.clone())
        } else {
            self.font.clone()
        };
        let mut wrapper = cx.text_system().line_wrapper(font, self.font_size);
        let task_size = self.task_size;
        // Remeasuring unchanged source must not temporarily discard image height.
        let image = self.lines.get(row).and_then(|line| line.image.clone());
        self._update(
            &text,
            &(offset..offset),
            &Rope::new(),
            &mut |line, width, task| {
                let fragments = if let Some(offset) = task {
                    vec![
                        LineFragment::text(&line[..offset]),
                        LineFragment::element(task_size, 3),
                        LineFragment::text(&line[offset + 3..]),
                    ]
                } else {
                    vec![LineFragment::text(line)]
                };
                wrapper.wrap_line(&fragments, width).collect()
            },
        );
        if let Some(mut image) = image {
            image.reveal = row == self.reveal_row;
            if let Some(mut line) = self.lines.get(row).cloned() {
                line.image = Some(image);
                self.lines.splice(row..=row, vec![line]);
                self.soft_lines = self.lines.wraps();
            }
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
                    code_block: None,
                    image: None,
                    table: None,
                    line: Rope::from(line),
                    measured: false,
                    syntax_revision: self.syntax_revision,
                    presentation: None,
                    mapped: None,
                    scale: 1.,
                    cache: Default::default(),
                    wrapped_lines: std::rc::Rc::new(ranges),
                });
                continue;
            }
            let source = line.to_string();
            self.copied_bytes += source.len();
            super::live_preview::record_ui_work(source.len(), source.len());
            let code_block = if self.live_preview {
                self.code.decoration(
                    changed_text.line_start_offset(new_start_row + ix),
                    &self.code_selection,
                    self.code_padding,
                )
            } else {
                None
            };
            let mut presentation = self.live_preview.then(|| {
                let start = changed_text.line_start_offset(new_start_row + ix);
                if let Some(code) = self.code.line(start) {
                    super::live_preview::Presentation::literal(&source, code.prefix, code.tokens)
                } else {
                    super::live_preview::Presentation::parse(&source)
                        .with_inline_code(&self.inline_code, start, source.len())
                        .with_links(
                            &self.links.in_range(start..start + source.len()),
                            start,
                            source.len(),
                        )
                }
            });
            if code_block.as_ref().is_some_and(|c| c.empty)
                && let Some(p) = &mut presentation
            {
                p.hidden = std::iter::once(0..source.len()).collect();
            }
            let scale = if code_block.is_some() {
                self.code_font_size / self.font_size
            } else {
                presentation.as_ref().map_or(1., |p| p.scale())
            };
            let mapped = presentation.as_ref().map(|p| {
                let mut mapped = p.display(
                    &source,
                    code_block.is_none()
                        && (p.code.is_some() || new_start_row + ix == self.reveal_row),
                );
                if code_block.as_ref().is_some_and(|c| c.empty) {
                    let start = changed_text.line_start_offset(new_start_row + ix);
                    mapped.display_to_source[0] = self.code.block(start).map_or(0, |b| {
                        b.content.start.saturating_sub(start).min(source.len())
                    });
                }
                std::rc::Rc::new(mapped)
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
                    line_str,
                    (wrap_width
                        - if code_block.is_some() {
                            self.code_inset * 2.
                        } else {
                            px(0.)
                        }
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
                code_block,
                image: None,
                table: None,
                line: Rope::from(line),
                measured: true,
                syntax_revision: self.syntax_revision,
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
            if range.is_empty()
                && new_text.len() == 0
                && new_lines.len() == 1
                && let Some(old) = self.lines.get(start_row)
            {
                let new = &mut new_lines[0];
                let offset = changed_text.line_start_offset(start_row);
                if old.line == new.line && self.tables.at(offset).is_some() {
                    new.table = old.table.clone();
                    new.cache = old.cache.clone();
                }
                if old.scale == new.scale
                    && old.mapped == new.mapped
                    && old.presentation == new.presentation
                    && old.wrapped_lines == new.wrapped_lines
                {
                    new.cache = old.cache.clone();
                }
            }
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
        self._update(text, &(0..self.text.len()), text, &mut |_, _, _| vec![]);
        self.estimating = false;
    }

    #[cfg(test)]
    fn layout_tables(&mut self, wrap: &mut impl FnMut(&str, Pixels) -> Vec<usize>) {
        self.layout_tables_in(0..self.lines.len(), wrap);
    }

    fn layout_tables_in(
        &mut self,
        rows: Range<usize>,
        wrap: &mut impl FnMut(&str, Pixels) -> Vec<usize>,
    ) {
        use super::table::{CELL_PADDING, MIN_COLUMN};
        if rows.is_empty() || self.text.len() == 0 {
            return;
        }
        let start = self
            .text
            .line_start_offset(rows.start.min(self.text.lines_len() - 1));
        let end = self
            .text
            .line_end_offset(rows.end.saturating_sub(1).min(self.text.lines_len() - 1));
        let first = self.tables.tables.partition_point(|t| t.source.end < start);
        for table in self.tables.tables[first..]
            .iter()
            .take_while(|t| t.source.start <= end)
        {
            let active = table.cell_at(self.active_offset);
            let widths = self
                .table_widths
                .get(&table.cache_key())
                .filter(|widths| widths.len() == table.alignments.len())
                .cloned();
            let widths = widths.unwrap_or_else(|| {
                let mut widths = vec![px(MIN_COLUMN); table.alignments.len()];
                for (i, chars) in table.column_characters.iter().enumerate() {
                    widths[i] = px(super::table::column_width(*chars, self.font_size.into()));
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
            self.table_widths.insert(table.cache_key(), widths.clone());
            let scroll = self
                .table_scroll
                .entry(table.cache_key())
                .or_default()
                .clone();
            let viewport = self.wrap_width.unwrap_or(px(800.));
            let total = widths.iter().copied().fold(px(0.), |a, b| a + b);
            scroll.set(scroll.get().clamp(px(0.), (total - viewport).max(px(0.))));
            let first_row = table.row_at_or_after(start);
            for (r, source_row) in table
                .rows
                .iter()
                .enumerate()
                .skip(first_row)
                .take_while(|(r, _)| table.row_range(*r).start <= end)
            {
                self.visited_table_rows += 1;
                super::live_preview::record_table_row();
                let row = self.text.offset_to_point(table.row_range(r).start).row;
                let Some(old) = self.lines.get(row) else {
                    continue;
                };
                let active_col = active.filter(|(ar, _)| *ar == r).map(|(_, c)| c);
                if old.table.as_ref().is_some_and(|t| {
                    t.revision.get() == self.syntax_revision
                        && t.widths == widths
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
                let mut height = super::table::MIN_ROW_LINES;
                for (c, range) in source_row.cells.iter().take(widths.len()).enumerate() {
                    let range = table.absolute_row(r, range);
                    let text = self.text.slice(range.clone()).to_string();
                    self.copied_bytes += text.len();
                    super::live_preview::record_ui_work(text.len(), text.len());
                    let mut inline = super::table::inline(&text);
                    let presentation = super::live_preview::Presentation {
                        hidden: inline.hidden,
                        ..Default::default()
                    }
                    .with_links(
                        &self.links.in_range(range.clone()),
                        range.start,
                        text.len(),
                    );
                    for link in &presentation.links {
                        inline.styles.push((link.label.clone(), 5));
                    }
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
                if let Some(previous) = old.table.as_ref()
                    && previous.cells == cells
                    && previous.widths == widths
                    && previous.height == height
                    && previous.active == active_col
                    && previous.header == (r == 0)
                {
                    previous.revision.set(self.syntax_revision);
                    continue;
                }
                let mut item = old.clone();
                item.table = Some(std::rc::Rc::new(TableRowLayout {
                    revision: std::cell::Cell::new(self.syntax_revision),
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
                super::live_preview::record_invalidation();
                self.lines.splice(row..=row, vec![item]);
            }
            let row = self.text.offset_to_point(table.separator.start).row;
            if rows.contains(&row)
                && let Some(old) = self.lines.get(row)
            {
                let mut item = old.clone();
                item.table = Some(std::rc::Rc::new(TableRowLayout {
                    revision: std::cell::Cell::new(self.syntax_revision),
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
                item.wrapped_lines =
                    std::rc::Rc::new(std::iter::once(0..item.line.len()).collect());
                self.lines.splice(row..=row, vec![item]);
            }
        }
        self.soft_lines = self.lines.wraps();
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
        if let Some(table) = &line.table
            && let Some(cell) = table
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
        let ix = line.wrapped_lines.len().saturating_sub(1);
        DisplayPoint::new(wrapped_row + ix, ix, last_range.len())
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
        let range = &line.wrapped_lines[local_row.min(line.wrapped_lines.len().saturating_sub(1))];
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
    pub(super) code_block: Option<super::fenced_code::Decoration>,
    pub(super) code_width: Pixels,
    pub(super) image: Option<ImageLayout>,
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
    pub(super) fn code_top(&self, line_height: Pixels) -> Pixels {
        self.code_block
            .as_ref()
            .map_or(px(0.), |c| c.top() * line_height)
    }
    pub(super) fn task_bounds(&self, line_height: Pixels) -> Option<gpui::Bounds<Pixels>> {
        let (state, _, width) = self.task?;
        let origin =
            self.position_for_index(state, line_height)? + point(px(0.), width * (0.4 / 0.875));
        Some(gpui::Bounds::new(origin, size(width, width)))
    }
    pub(crate) fn new() -> Self {
        Self {
            code_block: None,
            code_width: px(0.),
            table: None,
            image: None,
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
        if self.code_block.as_ref().is_some_and(|c| c.hidden) {
            return Some(point(px(0.), px(0.)));
        }
        if self.image.as_ref().is_some_and(|i| !i.reveal) {
            return (offset <= self.len()).then(|| point(px(0.), px(0.)));
        }
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
            return Some(point(
                x + self.indent,
                row as f32 * line_height + self.code_top(line_height / self.scale),
            ));
        }
        let mut acc_len = 0;
        for (i, line) in self.wrapped_lines.iter().enumerate() {
            if offset <= acc_len + line.len {
                return Some(point(
                    self.indent + line.x_for_index(offset - acc_len),
                    i as f32 * line_height + self.code_top(line_height / self.scale),
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
        if self.code_block.as_ref().is_some_and(|c| c.hidden) {
            return None;
        }
        if self.code_block.is_some() && (pos.y < px(0.) || pos.y >= self.size(line_height).height) {
            return None;
        }
        let pos = if let Some(code_block) = &self.code_block {
            point(
                pos.x,
                (pos.y - self.code_top(line_height)).max(px(0.)).min(
                    (self.size(line_height).height
                        - self.code_top(line_height)
                        - code_block.bottom() * line_height
                        - px(0.01))
                    .max(px(0.)),
                ),
            )
        } else {
            pos
        };
        if let Some(image) = &self.image {
            let text_height = if image.reveal {
                self.segment_ranges
                    .as_ref()
                    .map_or(self.wrapped_lines.len(), |r| r.len()) as f32
                    * line_height
            } else {
                px(0.)
            };
            if pos.y >= text_height && pos.y < text_height + image.rows as f32 * line_height {
                return Some(
                    if pos.y < text_height + image.rows as f32 * line_height / 2. {
                        0
                    } else {
                        self.len()
                    },
                );
            }
        }
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
        if let Some(table) = &self.table {
            if pos.x < px(0.)
                || pos.x >= table.viewport
                || pos.y < px(0.)
                || pos.y >= table.height * line_height
            {
                return None;
            }
            let pos = pos + point(table.scroll.get(), px(0.));
            let (range, x, _, cell) = table
                .cells
                .iter()
                .find(|(_, x, w, _)| pos.x >= *x && pos.x < *x + *w)?;
            return cell
                .index_for_position(point(pos.x - *x, pos.y), line_height)
                .map(|i| range.start + i);
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
                if pos.x < self.indent {
                    return None;
                }
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
        if self.code_block.as_ref().is_some_and(|c| c.hidden) {
            return size(px(0.), px(0.));
        }
        if let Some(image) = &self.image {
            let text_rows = if image.reveal {
                self.segment_ranges
                    .as_ref()
                    .map_or(self.wrapped_lines.len(), |r| r.len())
            } else {
                0
            };
            return size(
                self.longest_width.max(image.width),
                (text_rows + image.rows) as f32 * line_height,
            );
        }
        if let Some(table) = &self.table {
            return size(self.longest_width, table.height * line_height);
        }
        let line_height = line_height * self.scale;
        size(
            self.longest_width,
            self.segment_ranges
                .as_ref()
                .map_or(self.wrapped_lines.len(), |r| r.len())
                * line_height
                + self.code_block.as_ref().map_or(px(0.), |c| {
                    (c.top() + c.bottom()) * line_height / self.scale
                }),
        )
    }

    pub(super) fn paint(
        &self,
        pos: Point<Pixels>,
        line_height: Pixels,
        window: &mut Window,
        cx: &mut App,
    ) {
        if self.code_block.as_ref().is_some_and(|c| c.hidden) {
            return;
        }
        let pos = pos + point(px(0.), self.code_top(line_height));
        if let Some(image) = &self.image {
            use crate::ActiveTheme as _;
            let text_rows = if image.reveal {
                self.segment_ranges
                    .as_ref()
                    .map_or(self.wrapped_lines.len(), |r| r.len())
            } else {
                0
            };
            let origin = pos + point(px(0.), text_rows as f32 * line_height);
            if let Some(data) = &image.data {
                let _ = window.paint_image(
                    gpui::Bounds::new(origin, size(image.width, image.height)),
                    gpui::Corners::default(),
                    data.clone(),
                    0,
                    false,
                );
            } else {
                window.paint_quad(gpui::fill(
                    gpui::Bounds::new(origin, size(image.width, image.height)),
                    cx.theme().accent,
                ));
                let label = if image.failed {
                    "Image unavailable"
                } else {
                    "Loading image…"
                };
                let style = window.text_style();
                let text = window.text_system().shape_line(
                    label.into(),
                    style.font_size.to_pixels(window.rem_size()),
                    &[gpui::TextRun {
                        len: label.len(),
                        font: style.font(),
                        color: cx.theme().muted_foreground,
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    }],
                    None,
                );
                let _ = text.paint(origin + point(px(8.), px(8.)), line_height, window, cx);
            }
            if !image.reveal {
                return;
            }
        }
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
                code_block: None,
                image: None,
                table: None,
                line: Rope::from("Hello, 世界!\r"),
                wrapped_lines: std::rc::Rc::new(std::iter::once(0..15).collect()),
                presentation: None,
                mapped: None,
                scale: 1.,
                cache: Default::default(),
                measured: true,
                syntax_revision: 0,
            },
            // range: 16..36
            LineItem {
                code_block: None,
                image: None,
                table: None,
                line: Rope::from("This is second line."),
                wrapped_lines: std::rc::Rc::new(vec![0..10, 10..20]),
                presentation: None,
                mapped: None,
                scale: 1.,
                cache: Default::default(),
                measured: true,
                syntax_revision: 0,
            },
            // range: 37..56
            LineItem {
                code_block: None,
                image: None,
                table: None,
                line: Rope::from("This is third line."),
                wrapped_lines: std::rc::Rc::new(vec![0..9, 9..15, 15..20]),
                presentation: None,
                mapped: None,
                scale: 1.,
                cache: Default::default(),
                measured: true,
                syntax_revision: 0,
            },
            // range: 57..79
            LineItem {
                code_block: None,
                image: None,
                table: None,
                line: Rope::from("这里是第 4 行。"),
                wrapped_lines: std::rc::Rc::new(std::iter::once(0..22).collect()),
                presentation: None,
                mapped: None,
                scale: 1.,
                cache: Default::default(),
                measured: true,
                syntax_revision: 0,
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
            code_block: None,
            image: None,
            table: None,
            line: Rope::from("x".repeat(n)),
            measured: true,
            syntax_revision: 0,
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
    fn empty_cells_keep_minimum_dimensions_in_narrow_viewports() {
        let text = Rope::from("| | | |\n| --- | --- | --- |\n| | | |");
        for viewport in [90., 240., 800.] {
            let mut wrapper = TextWrapper::new(gpui::font("Arial"), px(16.), Some(px(viewport)));
            wrapper.live_preview = true;
            wrapper.tables.rebuild(&text);
            wrapper.active_offset = wrapper.tables.tables[0].rows[1].cells[1].start;
            wrapper._update(&text, &(0..0), &text, &mut |_, _, _| vec![]);
            wrapper.layout_tables(&mut |_, _| vec![]);
            for row in [0, 2] {
                let layout = wrapper.lines[row].table.as_ref().unwrap();
                assert_eq!(layout.cells.len(), 3);
                assert_eq!(layout.height, 2);
                assert_eq!(wrapper.lines[row].height(px(20.)), px(40.));
                for (col, cell) in layout.cells.iter().enumerate() {
                    assert!(cell.mapped.text.is_empty());
                    assert!(cell.width >= px(120.));
                    assert_eq!(cell.x, px(120. * col as f32));
                }
                assert_eq!(layout.viewport, px(viewport));
            }
            assert_eq!(wrapper.lines[1].lines_len(), 0, "separator stays hidden");
            assert_eq!(wrapper.lines.prefix(2), 2.);
            assert_eq!(wrapper.lines.prefix(3), 4.);
        }
    }

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
                    for row in table.rows.iter() {
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

#[cfg(test)]
mod link_layout_tests {
    fn settle(wrapper: &mut super::TextWrapper, text: &ropey::Rope) {
        wrapper.analysis.request(text);
        let parsed = wrapper.analysis.start().unwrap().parse();
        assert!(wrapper.apply_analysis(parsed));
    }
    use super::*;
    #[test]
    fn reference_edits_invalidate_distant_presentation() {
        let mut text = Rope::from("[one][id]\n\n[id]: note.md\n");
        let mut wrapper = TextWrapper::new(gpui::font("Arial"), px(14.), None);
        wrapper.live_preview = true;
        wrapper.reveal_row = usize::MAX;
        settle(&mut wrapper, &text);
        wrapper._update(&text, &(0..0), &text, &mut |_, _, _| vec![]);
        assert_eq!(wrapper.lines[0].mapped.as_ref().unwrap().text, "one");
        let at = text.to_string().rfind("[id]").unwrap() + 1;
        text.replace(at..at + 2, "missing");
        wrapper._update(
            &text,
            &(at..at + 2),
            &Rope::from("missing"),
            &mut |_, _, _| vec![],
        );
        settle(&mut wrapper, &text);
        assert_ne!(wrapper.lines[0].syntax_revision, wrapper.syntax_revision);
        wrapper._update(&text, &(0..0), &Rope::new(), &mut |_, _, _| vec![]);
        assert_eq!(wrapper.lines[0].mapped.as_ref().unwrap().text, "[one][id]");
        assert!(wrapper.links.in_range(0..text.len()).is_empty());
    }

    #[test]
    fn table_reference_links_use_document_definitions() {
        let text = Rope::from("| link |\n| --- |\n| [你好][id] |\n\n[id]: note.md\n");
        let mut wrapper = TextWrapper::new(gpui::font("Arial"), px(14.), Some(px(300.)));
        wrapper.live_preview = true;
        wrapper.reveal_row = usize::MAX;
        wrapper.tables.rebuild(&text);
        settle(&mut wrapper, &text);
        wrapper._update(&text, &(0..0), &text, &mut |_, _, _| vec![]);
        wrapper.layout_tables(&mut |_, _| vec![]);
        let cell = &wrapper.lines[2].table.as_ref().unwrap().cells[0];
        assert_eq!(cell.mapped.text, "你好");
        assert!(cell.styles.iter().any(|(_, style)| *style == 5));
    }
}

#[cfg(test)]
mod link_hit_tests {
    use super::*;
    #[test]
    fn strict_hits_respect_wraps_quote_gutters_and_table_scroll() {
        let source = "[你好](note.md)";
        let presentation = super::super::live_preview::Presentation::parse(source).with_links(
            &super::super::markdown_links::parse(source),
            0,
            source.len(),
        );
        let mut shaped = ShapedLine::default();
        *shaped = std::sync::Arc::new(gpui::LineLayout {
            width: px(20.),
            len: 3,
            ..Default::default()
        });
        let mut line = LineLayout::new();
        line.set_wrapped_lines(smallvec::smallvec![shaped.clone(), shaped]);
        line.mapped = Some(std::rc::Rc::new(presentation.display(source, false)));
        line.segment_ranges = Some(std::rc::Rc::new(vec![0..3, 3..6]));
        line.indent = px(14.);
        assert_eq!(
            line.index_for_position(point(px(15.), px(5.)), px(20.)),
            Some(1)
        );
        assert_eq!(
            line.index_for_position(point(px(15.), px(25.)), px(20.)),
            Some(4)
        );
        for position in [
            point(px(13.), px(5.)),
            point(px(34.), px(5.)),
            point(px(15.), px(45.)),
        ] {
            assert_eq!(line.index_for_position(position, px(20.)), None);
        }
        let table = LineLayout::table_row(
            source.len(),
            vec![(0..source.len(), px(0.), px(120.), line)],
            3,
            false,
            std::rc::Rc::new(std::cell::Cell::new(px(5.))),
            px(100.),
        );
        assert_eq!(
            table.index_for_position(point(px(10.), px(25.)), px(20.)),
            Some(4)
        );
        assert_eq!(
            table.index_for_position(point(px(10.), px(45.)), px(20.)),
            None
        );
        assert_eq!(
            table.index_for_position(point(px(-1.), px(5.)), px(20.)),
            None
        );
        assert_eq!(
            table.index_for_position(point(px(100.), px(5.)), px(20.)),
            None
        );
    }
}

#[cfg(test)]
mod sustained_edit_tests {
    use super::*;

    fn make(source: &str) -> TextWrapper {
        let text = Rope::from(source);
        let mut wrapper = TextWrapper::new(gpui::font("Arial"), px(16.), Some(px(800.)));
        wrapper.live_preview = true;
        wrapper.estimating = true;
        wrapper._update(&text, &(0..0), &text, &mut |_, _, _| vec![]);
        wrapper.estimating = false;
        settle(&mut wrapper);
        wrapper
    }
    fn settle(wrapper: &mut TextWrapper) {
        wrapper.analysis.request(&wrapper.text.clone());
        let request = wrapper.analysis.start().unwrap();
        assert!(wrapper.apply_analysis(request.parse()));
    }
    fn edit(wrapper: &mut TextWrapper, range: Range<usize>, inserted: &str) {
        let mut text = wrapper.text.clone();
        text.replace(range.clone(), inserted);
        let inserted = Rope::from(inserted);
        wrapper.edit_syntax(&text, &range, &inserted);
        wrapper._update(&text, &range, &inserted, &mut |_, _, _| vec![]);
        assert_eq!(wrapper.lines.len(), text.lines_len());
        assert!(wrapper.analysis.jobs() <= 2);
        drop(wrapper.take_retired());
    }
    fn measure(wrapper: &mut TextWrapper, rows: Range<usize>) {
        let text = wrapper.text.clone();
        for row in rows.clone() {
            let at = text.line_start_offset(row);
            if wrapper.lines[row].syntax_revision != wrapper.syntax_revision
                || !wrapper.lines[row].measured
            {
                wrapper._update(&text, &(at..at), &Rope::new(), &mut |_, _, _| vec![]);
            }
        }
        wrapper.layout_tables_in(rows, &mut |_, _| vec![]);
    }

    #[test]
    fn unrelated_typing_keeps_code_rendered_while_analysis_is_pending() {
        for newline in ["\n", "\r\n"] {
            let source =
                "before\n\n```js\nconst café = 1;\n```\n\nafter\n\n".replace('\n', newline);
            let mut wrapper = make(&source);
            let rows = wrapper.text.lines_len();
            measure(&mut wrapper, 0..rows);
            let colors = wrapper.lines[3].presentation.as_ref().unwrap().code.clone();
            let height = wrapper.lines[3].height_units();
            let assert_rendered = |wrapper: &TextWrapper, row: usize| {
                assert!(wrapper.lines[row - 1].code_block.as_ref().unwrap().hidden);
                assert!(wrapper.lines[row + 1].code_block.as_ref().unwrap().hidden);
                assert!(wrapper.lines[row].code_block.is_some());
                assert_eq!(wrapper.lines[row].height_units(), height);
                assert_eq!(
                    wrapper.lines[row].presentation.as_ref().unwrap().code,
                    colors
                );
            };
            // Deliberately leave a background result behind while a new fence
            // and its language tag are typed, then deliver that stale result.
            wrapper.analysis.request(&wrapper.text.clone());
            let delayed = wrapper.analysis.start().unwrap();
            for ch in "```js".chars() {
                let at = wrapper.text.len();
                edit(&mut wrapper, at..at, &ch.to_string());
                wrapper.reveal_code(at + 1..at + 1, 0.6);
                measure(&mut wrapper, 0..rows);
                assert_rendered(&wrapper, 3);
            }
            assert!(!wrapper.apply_analysis(delayed.parse()));
            measure(&mut wrapper, 0..rows);
            assert_rendered(&wrapper, 3);
            let latest = wrapper.analysis.start().unwrap();
            assert!(wrapper.apply_analysis(latest.parse()));
            measure(&mut wrapper, 0..rows);
            assert_rendered(&wrapper, 3);
            assert!(
                wrapper.lines[rows - 1].code_block.is_none(),
                "new fence stays editable"
            );

            // Repeat with an already recognized new fence and a fresh result
            // after every language-tag keystroke, as in a fast background worker.
            for ch in "on".chars() {
                let at = wrapper.text.len();
                edit(&mut wrapper, at..at, &ch.to_string());
                wrapper.reveal_code(at + 1..at + 1, 0.6);
                measure(&mut wrapper, 0..rows);
                assert_rendered(&wrapper, 3);
                let latest = wrapper.analysis.start().unwrap();
                assert!(wrapper.apply_analysis(latest.parse()));
                measure(&mut wrapper, 0..rows);
                assert_rendered(&wrapper, 3);
                assert!(wrapper.lines[rows - 1].code_block.is_none());
            }

            // Ordinary text before and after code, including a long input burst,
            // must not expose its fences or temporarily drop syntax colors.
            for _ in 0..200 {
                edit(&mut wrapper, 1..1, "é");
                wrapper.reveal_code(3..3, 0.6);
                measure(&mut wrapper, 0..rows);
                assert_rendered(&wrapper, 3);
                edit(&mut wrapper, 1..3, ""); // undo the insertion
                measure(&mut wrapper, 0..rows);
                assert_rendered(&wrapper, 3);
            }
            let at = wrapper.text.line_start_offset(6) + 2;
            edit(&mut wrapper, at..at, "text");
            wrapper.reveal_code(at + 4..at + 4, 0.6);
            measure(&mut wrapper, 0..rows);
            assert_rendered(&wrapper, 3);
        }
    }

    #[test]
    fn code_reveal_is_viewport_bounded_and_preserves_literal_nested_content() {
        let source = format!(
            "before\n```\n{}```\nafter",
            "literal **café** &amp;\n".repeat(10_000)
        );
        let mut wrapper = make(&source);
        measure(&mut wrapper, 5000..5020);
        let copied = wrapper.copied_bytes;
        let at = wrapper.text.line_start_offset(5005);
        wrapper.reveal_code(at..at, 0.6);
        measure(&mut wrapper, 5000..5020);
        assert!(wrapper.copied_bytes - copied < 2000);
        assert!(wrapper.lines[5005].code_block.is_none());
        let revision = wrapper.syntax_revision;
        wrapper.reveal_code(at + 1..at + 1, 0.6);
        assert_eq!(
            revision, wrapper.syntax_revision,
            "moving within one block doesn't invalidate it"
        );
        wrapper.reveal_code(0..0, 0.6);
        measure(&mut wrapper, 5000..5020);
        assert!(wrapper.lines[5005].code_block.is_some());
        assert_eq!(
            wrapper.lines[5005].mapped.as_ref().unwrap().text,
            "literal **café** &amp;"
        );

        let mut nested = make("before\n> ```\n> literal **你好**\n> ```\nafter");
        measure(&mut nested, 0..5);
        assert_eq!(
            nested.lines[2].mapped.as_ref().unwrap().text,
            "literal **你好**"
        );
        assert_eq!(nested.lines[1].height_units(), 0.);
        let at = nested.text.line_start_offset(2);
        nested.reveal_code(at..at, 0.6);
        measure(&mut nested, 0..5);
        for row in 1..=3 {
            assert!(nested.lines[row].code_block.is_none());
            assert_eq!(
                nested.lines[row].mapped.as_ref().unwrap().text,
                nested.lines[row].line.to_string()
            );
        }
    }

    #[test]
    fn inline_code_analysis_rejects_stale_results() {
        let source = "intro\n`*café*`\n";
        let mut wrapper = make(source);
        assert_eq!(wrapper.inline_code.len(), 1);
        measure(&mut wrapper, 0..3);
        assert_eq!(wrapper.lines[1].mapped.as_ref().unwrap().text, "*café*");
        wrapper.analysis.request(&wrapper.text.clone());
        let stale = wrapper.analysis.start().unwrap();
        edit(&mut wrapper, 6..7, "");
        assert!(wrapper.inline_code.is_empty());
        wrapper.apply_analysis(stale.parse());
        assert!(wrapper.inline_code.is_empty());
        let current = wrapper.analysis.start().unwrap();
        assert!(wrapper.apply_analysis(current.parse()));
        assert!(wrapper.inline_code.is_empty());
    }
    #[test]
    fn code_analysis_rejects_stale_tokens_and_restores_literal_mappings() {
        let source = "prefix\ntext\n\n```rs\nfn main() { let café = \"**literal**\"; }\n```\n";
        let mut wrapper = make(source);
        measure(&mut wrapper, 0..7);
        let cached = wrapper.analysis.code_cache.values().next().unwrap().clone();
        wrapper.analysis.request(&wrapper.text.clone());
        let stale = wrapper.analysis.start().unwrap();
        edit(&mut wrapper, 1..1, "x");
        let row = wrapper.text.to_string().find("fn").unwrap();
        assert!(!wrapper.code.line(row).unwrap().tokens.spans.is_empty());
        // Links can rebase this result; code must still await its exact revision.
        wrapper.apply_analysis(stale.parse());
        assert!(!wrapper.code.line(row).unwrap().tokens.spans.is_empty());
        let current = wrapper.analysis.start().unwrap();
        assert!(wrapper.apply_analysis(current.parse()));
        assert!(std::sync::Arc::ptr_eq(
            &cached,
            wrapper.analysis.code_cache.values().next().unwrap()
        ));
        measure(&mut wrapper, 0..7);
        let line = &wrapper.lines[4];
        assert_eq!(
            line.mapped.as_ref().unwrap().text,
            "fn main() { let café = \"**literal**\"; }"
        );
        assert!(
            !line
                .presentation
                .as_ref()
                .unwrap()
                .code
                .as_ref()
                .unwrap()
                .spans
                .is_empty()
        );
        for offset in [row, row + 3, row + "fn main() { let café".len()] {
            assert_eq!(
                wrapper.display_point_to_offset(wrapper.offset_to_display_point(offset)),
                offset
            );
        }
        let tag = wrapper.text.to_string().find("rs").unwrap();
        edit(&mut wrapper, tag..tag + 2, "unknown");
        let current = wrapper.analysis.start().unwrap();
        assert!(wrapper.apply_analysis(current.parse()));
        let row = wrapper.text.to_string().find("fn").unwrap();
        assert!(wrapper.code.line(row).unwrap().tokens.spans.is_empty());
        assert!(wrapper.analysis.code_cache.is_empty());
    }

    #[test]
    fn enter_and_backspace_update_lines_before_analysis_finishes() {
        for count in [20, 100_000] {
            let mut wrapper = make(&"ordinary café 你好 text\n".repeat(count));
            for row in [0, count / 2, count - 1] {
                let at = wrapper.text.line_start_offset(row);
                let original_rows = wrapper.lines.len();
                let original = wrapper.text.clone();
                wrapper.analysis.request(&original);
                let running = wrapper.analysis.start().unwrap();
                let copied = wrapper.copied_bytes;
                for i in 0..120 {
                    edit(&mut wrapper, at + i..at + i, "\n");
                    assert_eq!(wrapper.lines.len(), original_rows + i + 1);
                }
                for i in (0..120).rev() {
                    edit(&mut wrapper, at + i..at + i + 1, "");
                    assert_eq!(wrapper.lines.len(), original_rows + i);
                }
                assert_eq!(wrapper.text, original);
                assert!(
                    wrapper.copied_bytes - copied < 20_000,
                    "copy work is independent of document size"
                );
                // Long bursts exhaust the bounded mapping log; the latest job still starts immediately.
                assert!(!wrapper.apply_analysis(running.parse()));
                let latest = wrapper.analysis.start().unwrap();
                assert!(wrapper.apply_analysis(latest.parse()));
                assert_eq!(wrapper.analysis.jobs(), 0);
            }
        }
    }

    #[test]
    fn checkbox_toggles_preserve_tables_through_pending_analysis() {
        for (newline, rounds) in [("\n", 2), ("\r\n", 2), ("\n", 8), ("\r\n", 8)] {
            let source = "- [ ] café\n\n| name | value |\n| --- | ---: |\n| 你好 | **one** |\n\n> > 1. [X] nested\n\n| other | value |\n| :--- | --- |\n| two | three |\n\n- [x] after\n".replace('\n', newline);
            let mut wrapper = make(&source);
            wrapper.wrap_width = Some(px(150.));
            let count = wrapper.lines.len();
            measure(&mut wrapper, 0..count);
            assert_eq!(wrapper.tables.tables.len(), 2);
            for scroll in wrapper.table_scroll.values() {
                scroll.set(px(30.));
            }
            let cached = (0..count)
                .filter_map(|row| wrapper.lines[row].table.clone().map(|table| (row, table)))
                .collect::<Vec<_>>();
            assert_eq!(cached.len(), 6, "headers, hidden separators, and body rows");
            let height = wrapper.lines.total_height();
            let widths = wrapper
                .tables
                .tables
                .iter()
                .map(|table| wrapper.table_widths[&table.cache_key()].clone())
                .collect::<Vec<_>>();
            let assert_stable = |wrapper: &TextWrapper| {
                assert_eq!(wrapper.tables.tables.len(), 2);
                assert_eq!(wrapper.lines.total_height(), height);
                for (row, cached) in &cached {
                    let current = wrapper.lines[*row]
                        .table
                        .as_ref()
                        .expect("table must never show source");
                    if cached.height > 0 {
                        assert!(std::rc::Rc::ptr_eq(cached, current));
                    }
                    assert_eq!(current.height, cached.height);
                    assert_eq!(current.widths, cached.widths);
                    assert!(std::rc::Rc::ptr_eq(&cached.scroll, &current.scroll));
                    assert_eq!(current.scroll.get(), px(30.));
                }
                for (table, expected) in wrapper.tables.tables.iter().zip(&widths) {
                    assert_eq!(wrapper.table_widths[&table.cache_key()], *expected);
                }
            };
            let offsets = source
                .match_indices('[')
                .map(|(at, _)| at + 1)
                .collect::<Vec<_>>();
            wrapper.analysis.request(&wrapper.text.clone());
            let delayed = wrapper.analysis.start().unwrap();
            for _ in 0..rounds {
                for &offset in &offsets {
                    let original = wrapper.text.slice(offset..offset + 1).to_string();
                    let next = if original == " " { "x" } else { " " };
                    // The forward, undo, and redo replacements all use this edit path.
                    for replacement in [next, original.as_str(), next] {
                        edit(&mut wrapper, offset..offset + 1, replacement);
                        assert_stable(&wrapper);
                        measure(&mut wrapper, 0..count);
                        assert_stable(&wrapper);
                    }
                }
            }
            // A long burst can exhaust the bounded rebase log; either completion
            // outcome must leave the current table presentation intact.
            assert_eq!(wrapper.apply_analysis(delayed.parse()), rounds == 2);
            measure(&mut wrapper, 0..count);
            assert_stable(&wrapper);
            let latest = wrapper.analysis.start().unwrap();
            assert!(wrapper.apply_analysis(latest.parse()));
            measure(&mut wrapper, 0..count);
            assert_stable(&wrapper);
            let mut fresh = super::super::table::TableIndex::default();
            fresh.rebuild(&wrapper.text);
            assert_eq!(wrapper.tables.tables.len(), fresh.tables.len());
            for (actual, expected) in wrapper.tables.tables.iter().zip(&fresh.tables) {
                assert_eq!(actual.source, expected.source);
                assert_eq!(actual.separator, expected.separator);
                assert_eq!(actual.alignments, expected.alignments);
                assert_eq!(actual.column_characters, expected.column_characters);
                assert_eq!(actual.rows.len(), expected.rows.len());
                for row in 0..actual.rows.len() {
                    assert_eq!(actual.row_range(row), expected.row_range(row));
                    for col in 0..actual.alignments.len() {
                        assert_eq!(actual.cell_range(row, col), expected.cell_range(row, col));
                    }
                }
            }
        }
    }

    #[test]
    fn large_table_layout_visits_only_requested_rows_and_keeps_shifted_caches() {
        let source = format!(
            "paragraph\n\n\n| name | value |\n| --- | --- |\n{}",
            "| café | 你好 |\n".repeat(10_000)
        );
        let mut wrapper = make(&source);
        measure(&mut wrapper, 5000..5020);
        let copied = wrapper.copied_bytes;
        let visited = wrapper.visited_table_rows;
        let cached = wrapper.lines[5005].table.clone().unwrap();
        let identity = wrapper.tables.tables[0].cache_key();
        for _ in 0..60 {
            edit(&mut wrapper, 0..0, "\n");
            edit(&mut wrapper, 0..1, "");
        }
        // Blank line changes immediately above a table keep its stable identity too.
        let before_table = wrapper.text.line_start_offset(2);
        for _ in 0..60 {
            edit(&mut wrapper, before_table..before_table, "\n");
            edit(&mut wrapper, before_table..before_table + 1, "");
        }
        assert_eq!(wrapper.tables.tables[0].cache_key(), identity);
        measure(&mut wrapper, 5000..5020);
        assert_eq!(wrapper.visited_table_rows - visited, 20);
        assert!(wrapper.copied_bytes - copied < 10_000);
        assert!(std::rc::Rc::ptr_eq(
            &cached,
            wrapper.lines[5005].table.as_ref().unwrap()
        ));
        settle(&mut wrapper);
        measure(&mut wrapper, 5000..5020);
        assert!(
            std::rc::Rc::ptr_eq(&cached, wrapper.lines[5005].table.as_ref().unwrap()),
            "unchanged rows survive background completion"
        );
        let current = wrapper.text.clone();
        let offset = wrapper.tables.tables[0].cell_range(5000, 0).start;
        assert_eq!(
            current.slice(offset..offset + "café".len()).to_string(),
            "café"
        );
    }

    #[test]
    fn link_heavy_edits_do_not_copy_or_parse_the_document_on_the_input_thread() {
        let mut wrapper = make(&"[café](note.md)\n\n".repeat(10_000));
        let copied = wrapper.copied_bytes;
        for row in [0, 5000, 19_998] {
            let at = wrapper.text.line_start_offset(row);
            for _ in 0..60 {
                edit(&mut wrapper, at..at, "\n");
                edit(&mut wrapper, at..at + 1, "");
            }
        }
        assert!(wrapper.copied_bytes - copied < 20_000);
        settle(&mut wrapper);
        assert_eq!(wrapper.links.in_range(0..wrapper.text.len()).len(), 10_000);
    }

    #[test]
    fn context_changes_reject_delayed_analysis_and_settle_to_full_parser() {
        for source in [
            "[one][id]\r\n\r\n[id]: note.md\r\n",
            "```\n[code](note.md)\n```\n",
            "| name |\n| --- |\n| [one](note.md) |\n",
        ] {
            let mut wrapper = make(source);
            wrapper.analysis.request(&wrapper.text.clone());
            let old = wrapper.analysis.start().unwrap();
            edit(&mut wrapper, 0..0, "> ");
            assert!(!wrapper.apply_analysis(old.parse()));
            let next = wrapper.analysis.start().unwrap();
            assert!(wrapper.apply_analysis(next.parse()));
            let source = wrapper.text.to_string();
            assert_eq!(
                wrapper.links.in_range(0..source.len()),
                super::super::markdown_links::parse(&source)
            );
        }
    }
}
#[cfg(feature = "editor-diagnostics")]
impl TextWrapper {
    pub(super) fn diagnostic_settle(&mut self) {
        let mut parsed = super::preview_syntax::Request {
            code_cache: self.analysis.code_cache.clone(),
            revision: self.analysis.revision,
            text: self.text.clone(),
        }
        .parse();
        self.analysis.code_cache = parsed.code.cache.clone();
        self.inline_code = parsed.inline_code;
        self.code = super::fenced_code::Index::new(parsed.code);
        self.links = super::preview_syntax::Links::with_edits(parsed.links, Vec::new());
        let revision = self.tables.revision + 1;
        parsed.tables.retire_from(&mut self.tables);
        self.tables = parsed.tables;
        self.tables.revision = revision;
        self.syntax_revision += 1;
    }
}

#[derive(Clone)]
pub(super) struct ImageLayout {
    pub data: Option<std::sync::Arc<gpui::RenderImage>>,
    pub width: Pixels,
    pub height: Pixels,
    pub rows: usize,
    pub reveal: bool,
    pub failed: bool,
}
impl std::fmt::Debug for ImageLayout {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ImageLayout")
            .field("rows", &self.rows)
            .finish()
    }
}
impl TextWrapper {
    pub(super) fn measure_images(
        &mut self,
        rows: Range<usize>,
        line_height: Pixels,
        window: &mut Window,
        cx: &mut App,
    ) {
        if !self.live_preview || self.image_document.is_none() {
            return;
        }
        for row in rows {
            let Some(mut item) = self.lines.get(row).cloned() else {
                continue;
            };
            item.image = None;
            if let Some(url) = self.images.get(&self.text.line_start_offset(row)) {
                let resource =
                    crate::markdown_images::resource(url, self.image_document.as_deref());
                let loaded = resource
                    .as_ref()
                    .and_then(|resource| window.use_asset::<gpui::ImgResourceLoader>(resource, cx));
                let failed = resource.is_none() || loaded.as_ref().is_some_and(|r| r.is_err());
                let data = loaded.and_then(Result::ok);
                let (width, height) = data.as_ref().map_or((px(240.), px(80.)), |image| {
                    let dimensions = image.size(0);
                    let w = px(u32::from(dimensions.width) as f32).max(px(1.));
                    let h = px(u32::from(dimensions.height) as f32).max(px(1.));
                    let ratio = (self.wrap_width.unwrap_or(w) / w).min(1.);
                    (w * ratio, h * ratio)
                });
                item.image = Some(ImageLayout {
                    data,
                    width,
                    height,
                    rows: (height / line_height).ceil() as usize + 1,
                    reveal: row == self.reveal_row,
                    failed,
                });
            }
            self.lines.splice(row..=row, vec![item]);
        }
        self.soft_lines = self.lines.wraps();
    }
}

#[cfg(test)]
mod image_layout_tests {
    use super::*;
    #[test]
    fn image_height_and_hit_mapping() {
        let mut line = LineLayout::new();
        line.len = 12;
        line.image = Some(ImageLayout {
            data: None,
            width: px(200.),
            height: px(90.),
            rows: 6,
            reveal: false,
            failed: true,
        });
        assert_eq!(line.size(px(20.)).height, px(120.));
        assert_eq!(
            line.closest_index_for_position(point(px(70.), px(10.)), px(20.)),
            Some(0)
        );
        assert_eq!(
            line.closest_index_for_position(point(px(70.), px(110.)), px(20.)),
            Some(12)
        );
        assert_eq!(
            line.closest_index_for_position(point(px(70.), px(121.)), px(20.)),
            None
        );
        assert_eq!(
            line.position_for_index(6, px(20.)),
            Some(point(px(0.), px(0.)))
        );
        let mut item = LineItem::diagnostic_line(1);
        item.image = line.image.clone();
        let mut lines = super::super::line_index::Lines::default();
        lines.splice(0..=0, vec![item.clone(), LineItem::diagnostic_line(1)]);
        assert_eq!(lines.prefix(1), 6.);
        item.image.as_mut().unwrap().reveal = true;
        lines.splice(0..=0, vec![item]);
        assert_eq!(lines.prefix(1), 7.);
    }
}
