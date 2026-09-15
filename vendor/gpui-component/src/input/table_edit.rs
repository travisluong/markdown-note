use super::{
    InputState, RopeExt,
    table::{Command, Table},
};
use gpui::{Context, Window};

impl InputState {
    pub(super) fn table_input(
        &self,
        range: &std::ops::Range<usize>,
        input: &str,
    ) -> Option<String> {
        if !self.live_preview || self.table_editing || self.history.ignoring() {
            return Some(input.into());
        }
        let Some((t, r, c)) = self.text_wrapper.tables.at(self.cursor()) else {
            return Some(input.into());
        };
        let cell = &t.cell_range(r, c);
        if range.start < cell.start || range.end > cell.end {
            if self.selected_range.is_empty() && input.is_empty() {
                return None;
            }
            return Some(input.into());
        }
        let prefix = self.text.slice(cell.start..range.start).to_string();
        let slashes = prefix
            .chars()
            .rev()
            .take_while(|c| *c == char::from(92))
            .count();
        let normalized = super::table::cell_input(&format!("{}{}", "\\".repeat(slashes), input));
        let mut result = normalized[slashes..].to_string();
        let suffix = self.text.slice(range.end..cell.end).to_string();
        let suffix_slashes = suffix.chars().take_while(|c| *c == char::from(92)).count();
        if suffix[suffix_slashes..].starts_with('|') {
            let combined = format!("{prefix}{result}");
            let trailing = combined
                .chars()
                .rev()
                .take_while(|c| *c == char::from(92))
                .count();
            if (trailing + suffix_slashes) % 2 == 0 {
                result.push(char::from(92));
            }
        }
        Some(result)
    }
    pub(super) fn active_table(&self) -> Option<(Table, usize, usize)> {
        if !self.live_preview {
            return None;
        }
        self.text_wrapper
            .tables
            .at(self.cursor())
            .map(|(t, r, c)| (t.clone(), r, c))
    }

    pub(super) fn table_command(
        &mut self,
        command: Command,
        offset: usize,
        revision: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.disabled || !self.live_preview || revision != self.text_wrapper.tables.revision {
            return false;
        }
        let Some((table, row, col)) = self.text_wrapper.tables.at(offset) else {
            return false;
        };
        let Some(edit) = table.edit(&self.text, row, col, command) else {
            return false;
        };
        self.apply_table_edit(edit, window, cx);
        true
    }

    fn apply_table_edit(
        &mut self,
        edit: super::table::Edit,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.selected_range = edit.range.into();
        self.table_editing = true;
        self.text_wrapper.trusted_table_edit = true;
        self.replace_selection_isolated(&edit.text, window, cx);
        self.table_editing = false;
        self.text_wrapper.trusted_table_edit = false;
        self.move_to(edit.caret, None, cx);
    }

    pub(super) fn focus_table_cell(
        &mut self,
        table: &Table,
        row: usize,
        col: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if col >= table.rows[row].present_cells && !self.disabled {
            if let Some(edit) = table.edit(&self.text, row, col, Command::PadRow) {
                self.apply_table_edit(edit, window, cx);
            }
        } else {
            self.move_to(table.cell_range(row, col).start, None, cx);
        }
    }

    pub(super) fn table_tab(
        &mut self,
        backwards: bool,
        vertical: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.selected_range.is_empty() {
            return false;
        }
        let Some((table, row, col)) = self.active_table() else {
            return false;
        };
        let columns = table.alignments.len();
        let at = row * columns + col;
        let next = if backwards {
            at.checked_sub(1)
        } else {
            Some(at + if vertical { columns } else { 1 })
        };
        match next {
            None => self.table_exit(&table, true, window, cx),
            Some(next) if next < table.rows.len() * columns => {
                self.focus_table_cell(&table, next / columns, next % columns, window, cx)
            }
            Some(_) => {
                let revision = self.text_wrapper.tables.revision;
                if self.table_command(Command::RowBelow, self.cursor(), revision, window, cx) {
                    if let Some((t, r, _)) = self.active_table() {
                        self.move_to(
                            t.cell_range(r, if vertical { col } else { 0 }).start,
                            None,
                            cx,
                        );
                    }
                }
            }
        }
        true
    }

    pub(super) fn table_exit(
        &mut self,
        table: &Table,
        before: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let at = if before {
            table.source.start
        } else {
            table.source.end
        };
        let row = self.text.offset_to_point(at).row;
        if before && row > 0 {
            self.move_to(self.text.line_end_offset(row - 1), None, cx);
        } else if !before && at < self.text.len() {
            self.move_to(self.text.line_start_offset(row + 1), None, cx);
        } else if !self.disabled {
            self.selected_range = (at..at).into();
            self.table_editing = true;
            self.text_wrapper.trusted_table_edit = true;
            let newline = if self
                .text
                .slice(table.source.clone())
                .to_string()
                .contains("\r\n")
            {
                "\r\n"
            } else {
                "\n"
            };
            self.replace_selection_isolated(newline, window, cx);
            self.table_editing = false;
            self.text_wrapper.trusted_table_edit = false;
            self.move_to(if before { at } else { at + newline.len() }, None, cx);
        }
    }

    pub(super) fn table_horizontal(
        &mut self,
        right: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if !self.selected_range.is_empty() {
            return false;
        }
        let Some((t, r, c)) = self.active_table() else {
            return false;
        };
        let cell = &t.cell_range(r, c);
        if (right && self.cursor() >= cell.end) || (!right && self.cursor() <= cell.start) {
            if right && r + 1 == t.rows.len() && c + 1 == t.alignments.len() {
                self.table_exit(&t, false, window, cx);
            } else {
                self.table_tab(!right, false, window, cx);
                if !right {
                    if let Some((table, row, col)) = self.active_table() {
                        self.move_to(table.cell_range(row, col).end, None, cx);
                    }
                }
            }
            return true;
        }
        false
    }

    pub(super) fn table_vertical(
        &mut self,
        down: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some((t, r, c)) = self.active_table() else {
            return false;
        };
        let source_row = self.text.offset_to_point(self.cursor()).row;
        // Keep normal visual-line movement while the caret is inside a wrapped cell.
        if let Some(layout) = self.last_layout.as_ref().and_then(|l| l.line(source_row)) {
            if let Some((range, _, _, cell)) = layout.table.as_ref().and_then(|t| t.cells.get(c)) {
                let height = self.last_layout.as_ref().unwrap().line_height;
                if let Some(pos) = cell.position_for_index(
                    self.cursor() - self.text.line_start_offset(source_row) - range.start,
                    height,
                ) {
                    let y = pos.y + if down { height } else { -height };
                    if y >= gpui::px(0.) && y < cell.size(height).height {
                        if let Some(local) =
                            cell.closest_index_for_position(gpui::point(pos.x, y), height)
                        {
                            self.move_to(
                                self.text.line_start_offset(source_row) + range.start + local,
                                None,
                                cx,
                            );
                            return true;
                        }
                    }
                }
            }
        }
        let next = if down {
            Some(r + 1).filter(|n| *n < t.rows.len())
        } else {
            r.checked_sub(1)
        };
        if let Some(next) = next {
            let within = self.cursor().saturating_sub(t.cell_range(r, c).start);
            let cell = &t.cell_range(next, c);
            self.move_to(
                self.text
                    .floor_char_boundary(cell.start + within.min(cell.len())),
                None,
                cx,
            );
        } else {
            self.table_exit(&t, !down, window, cx);
        }
        true
    }
}

#[cfg(feature = "editor-diagnostics")]
impl InputState {
    #[doc(hidden)]
    pub fn diagnostic_table_checks(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        mut progress: impl FnMut(Command),
    ) {
        use super::super::input::{Redo, Undo};
        use super::table::Alignment;
        use gpui::EntityInputHandler as _;
        let original = "before\n\n| café | 你好 |\n| :---: | ---: |\n| x | y |\n\nafter";
        self.set_live_preview(true, cx);
        self.diagnostic_settle_preview();
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
            progress(command);
            self.set_value(original, window, cx);
            self.diagnostic_settle_preview();
            self.history.clear();
            let at = self.text_wrapper.tables.tables[0].rows[1].cells[1].start;
            let rev = self.text_wrapper.tables.revision;
            self.move_to(at, None, cx);
            assert!(
                !self.table_command(command, at, rev - 1, window, cx),
                "reject stale target"
            );
            assert!(self.table_command(command, at, rev, window, cx));
            let edited = self.value();
            self.undo(&Undo, window, cx);
            assert_eq!(self.value().as_str(), original);
            self.redo(&Redo, window, cx);
            assert_eq!(self.value(), edited);
        }
        self.set_value(original, window, cx);
        self.diagnostic_settle_preview();
        self.history.clear();
        let at = self.text_wrapper.tables.tables[0].rows[1].cells[0].start;
        self.move_to(at, None, cx);
        self.replace_text_in_range(None, "a|b\r\nc", window, cx);
        assert!(self.value().contains("a\\|b cx"));
        self.undo(&Undo, window, cx);
        assert_eq!(self.value().as_str(), original);
        self.move_to(at, None, cx);
        self.replace_and_mark_text_in_range(None, "您好", Some(2..2), window, cx);
        self.replace_text_in_range(None, "你好", window, cx);
        assert!(self.value().contains("你好x"));
        self.set_value(original, window, cx);
        self.diagnostic_settle_preview();
        self.history.clear();
        self.move_to(at, None, cx);
        self.indent_inline(&super::IndentInline, window, cx);
        assert_eq!(self.active_table().map(|(_, r, c)| (r, c)), Some((1, 1)));
        assert_eq!(self.value().as_str(), original);
        self.outdent_inline(&super::OutdentInline, window, cx);
        assert_eq!(self.active_table().map(|(_, r, c)| (r, c)), Some((1, 0)));
        self.indent_inline(&super::IndentInline, window, cx);
        assert!(self.table_tab(false, false, window, cx));
        assert_eq!(self.text_wrapper.tables.tables[0].rows.len(), 3);
        self.undo(&Undo, window, cx);
        assert_eq!(self.value().as_str(), original);
        self.move_to(at, None, cx);
        let selection = self.selected_range;
        self.set_live_preview(false, cx);
        self.set_live_preview(true, cx);
        self.diagnostic_settle_preview();
        assert_eq!(self.selected_range, selection);
        self.disabled = true;
        let rev = self.text_wrapper.tables.revision;
        assert!(!self.table_command(Command::RowBelow, at, rev, window, cx));
        self.replace_text_in_range(None, "no", window, cx);
        assert_eq!(self.value().as_str(), original);
        self.disabled = false;
    }

    #[doc(hidden)]
    pub fn diagnostic_table_geometry(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let last = self.last_layout.as_ref().expect("paint table fixture");
        let bounds = self.last_bounds.unwrap();
        let mut count = 0;
        let mut wrapped = false;
        for (i, line) in last.lines.iter().enumerate() {
            let Some(table) = &line.table else {
                continue;
            };
            let row = last.visible_range.start + i;
            let start = self.text.line_start_offset(row);
            if table.cells.is_empty() {
                assert_eq!(line.size(last.line_height).height, gpui::px(0.));
                continue;
            }
            count += 1;
            for (range, _, _, cell) in &table.cells {
                wrapped |= cell.segment_ranges.as_ref().unwrap().len() > 1;
                for (offset, _) in self
                    .text
                    .slice(start + range.start..start + range.end)
                    .to_string()
                    .char_indices()
                {
                    let p = line
                        .position_for_index(range.start + offset, last.line_height)
                        .unwrap();
                    if p.x < gpui::px(0.) || p.x >= table.viewport {
                        continue;
                    }
                    let click = bounds.origin
                        + gpui::point(
                            last.line_number_width,
                            self.text_wrapper.lines.prefix(row) * last.line_height,
                        )
                        + p
                        + gpui::point(gpui::px(0.), last.line_height * 0.5);
                    let hit = self.index_for_mouse_position(click);
                    assert_eq!(
                        self.text.offset_to_point(hit).row,
                        row,
                        "table click source row"
                    );
                    let back = line
                        .position_for_index(hit - start, last.line_height)
                        .unwrap();
                    assert!(
                        (back.x - p.x).abs() < gpui::px(1.),
                        "table cell x round trip"
                    );
                    assert!(
                        (back.y - p.y).abs() < gpui::px(1.),
                        "table cell y round trip"
                    );
                }
            }
        }
        assert!(count >= 2 && wrapped, "rendered wrapped table rows");
        assert!(
            super::element::TextElement::layout_match_range(0..self.text.len(), last, &bounds)
                .is_some()
        );
        let _ = (window, cx);
    }
}

#[cfg(feature = "editor-diagnostics")]
impl InputState {
    #[doc(hidden)]
    pub fn diagnostic_table_autosave(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.set_value("| a | b |\n| --- | --- |\n| x | y |\n", window, cx);
        self.set_live_preview(true, cx);
        self.diagnostic_settle_preview();
        let at = self.text_wrapper.tables.tables[0].rows[1].cells[0].start;
        self.move_to(at, None, cx);
        self.insert("edited ", window, cx);
        assert!(self.table_command(
            Command::RowBelow,
            self.cursor(),
            self.text_wrapper.tables.revision,
            window,
            cx
        ));
        assert_eq!(self.text_wrapper.tables.tables[0].rows.len(), 3);
    }
}
