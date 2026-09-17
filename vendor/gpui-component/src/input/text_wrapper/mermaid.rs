//! Source-preserving multiline Mermaid projection. Source rows remain in the height index.
use super::*;
use crate::{
    ActiveTheme,
    mermaid::{self, Key, Status, Ticket},
};

#[derive(Clone)]
pub(in crate::input) enum DiagramLine {
    Image {
        image: std::sync::Weak<gpui::RenderImage>,
        width: f32,
        height: f32,
        rows: usize,
        target: usize,
    },
    Covered,
    Message(gpui::SharedString),
}
impl std::fmt::Debug for DiagramLine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("DiagramLine").field(&self.rows()).finish()
    }
}
impl DiagramLine {
    pub(in crate::input) fn rows(&self) -> Option<usize> {
        match self {
            Self::Image { rows, .. } => Some(*rows),
            Self::Covered => Some(0),
            _ => None,
        }
    }
    pub(in crate::input) fn paint(
        &self,
        pos: Point<Pixels>,
        line_height: Pixels,
        window: &mut Window,
        cx: &mut App,
    ) -> bool {
        match self {
            Self::Covered => true,
            Self::Image {
                image,
                width,
                height,
                ..
            } => {
                if let Some(image) = image.upgrade() {
                    let _ = window.paint_image(
                        gpui::Bounds::new(
                            pos + point(px(12.), px(12.)),
                            size(px(*width), px(*height)),
                        ),
                        gpui::Corners::default(),
                        image,
                        0,
                        false,
                    );
                }
                true
            }
            Self::Message(message) => {
                let style = window.text_style();
                let text = window.text_system().shape_line(
                    message.clone(),
                    style.font_size.to_pixels(window.rem_size()),
                    &[gpui::TextRun {
                        len: message.len(),
                        font: style.font(),
                        color: cx.theme().muted_foreground,
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    }],
                    None,
                );
                let _ = text.paint(pos + point(px(12.), px(0.)), line_height, window, cx);
                false
            }
        }
    }
}
pub(in crate::input) struct Projection {
    rows: Range<usize>,
    key: Key,
    ticket: Option<Ticket>,
}
impl TextWrapper {
    fn remove_diagram(&mut self, start: usize) {
        if let Some(projection) = self.diagrams.remove(&start) {
            for row in projection.rows {
                if let Some(mut line) = self.lines.get(row).cloned() {
                    line.diagram = None;
                    line.syntax_revision = self.syntax_revision.wrapping_sub(1);
                    self.lines.splice(row..=row, vec![line]);
                }
            }
        }
        self.soft_lines = self.lines.wraps();
    }
    pub(in crate::input) fn clear_diagrams(&mut self) {
        let starts: Vec<_> = self.diagrams.keys().copied().collect();
        for start in starts {
            self.remove_diagram(start);
        }
    }
    pub(in crate::input) fn prepare_diagrams(&mut self, window: &Window, cx: &App) {
        let width = f32::from(self.code_width() - px(24.));
        let obsolete: Vec<_> = self
            .diagrams
            .iter()
            .filter_map(|(start, p)| {
                let valid = self.live_preview
                    && self.code.block(*start).is_some_and(|b| {
                        b.mermaid
                            && !b.active(&self.code_selection)
                            && p.key
                                == Key::new(
                                    b.code,
                                    cx.theme().is_dark(),
                                    width,
                                    window.scale_factor(),
                                )
                    });
                (!valid).then_some(*start)
            })
            .collect();
        for start in obsolete {
            self.remove_diagram(start);
        }
    }
    pub(in crate::input) fn measure_diagrams(
        &mut self,
        visible: Range<usize>,
        line_height: Pixels,
        window: &Window,
        cx: &mut App,
    ) {
        if !self.live_preview {
            return;
        }
        let mut row = visible.start;
        let mut requested = std::collections::HashSet::new();
        while row < visible.end {
            let Some(block) = self
                .code
                .block(self.text.line_start_offset(row))
                .filter(|b| b.mermaid)
            else {
                row += 1;
                continue;
            };
            let first = self.text.offset_to_point(block.source.start).row;
            let last = self.text.offset_to_point(block.source.end).row;
            row = last + 1;
            if block.active(&self.code_selection) {
                continue;
            }
            requested.insert(block.source.start);
            let key = Key::new(
                block.code.clone(),
                cx.theme().is_dark(),
                f32::from(self.code_width() - px(24.)),
                window.scale_factor(),
            );
            let p = self
                .diagrams
                .entry(block.source.start)
                .or_insert_with(|| Projection {
                    rows: first..last + 1,
                    key: key.clone(),
                    ticket: None,
                });
            let status = mermaid::request(&mut p.ticket, key, cx);
            let target = block.content.start
                + self.code.line(block.content.start).map_or(0, |l| l.prefix)
                - block.source.start;
            let collapsed = matches!(status, Status::Ready(_));
            let image_rows =
                |height: f32| ((height + 24.) / f32::from(line_height)).ceil().max(1.) as usize;
            let unchanged = self
                .lines
                .get(first)
                .and_then(|l| l.diagram.as_ref())
                .is_some_and(|old| match (old, &status) {
                    (
                        DiagramLine::Image {
                            image,
                            rows,
                            target: old_target,
                            ..
                        },
                        Status::Ready(d),
                    ) => {
                        image.ptr_eq(&std::sync::Arc::downgrade(&d.image))
                            && *rows == image_rows(d.height)
                            && *old_target == target
                    }
                    _ => false,
                });
            if unchanged {
                continue;
            }
            for current in first..=last {
                let Some(mut line) = self.lines.get(current).cloned() else {
                    continue;
                };
                // Fallback displays the full source, including fences.
                if let Some(code) = &mut line.code_block {
                    code.hidden = false;
                }
                line.diagram = if current == first {
                    Some(match &status {
                        Status::Ready(d) => DiagramLine::Image {
                            image: std::sync::Arc::downgrade(&d.image),
                            width: d.width,
                            height: d.height,
                            rows: image_rows(d.height),
                            target,
                        },
                        Status::Pending => DiagramLine::Message("Rendering Mermaid…".into()),
                        Status::Error(e) => DiagramLine::Message(e.clone()),
                    })
                } else if collapsed {
                    Some(DiagramLine::Covered)
                } else {
                    None
                };
                if self.lines[current].height_units() != line.height_units() {
                    cx.refresh_windows();
                }
                self.lines.splice(current..=current, vec![line]);
            }
        }
        // Offscreen geometry can remain, but queued work loses its authority.
        for (start, projection) in &mut self.diagrams {
            if !requested.contains(start) {
                projection.ticket = None;
            }
        }
        self.soft_lines = self.lines.wraps();
    }
}

impl LineLayout {
    pub(in crate::input) fn diagram_projection(item: &LineItem, width: Pixels) -> Option<Self> {
        let diagram = item.diagram.as_ref().filter(|d| d.rows().is_some())?;
        let mut line = Self::new();
        line.len = item.len();
        line.diagram = Some(diagram.clone());
        line.code_block = item.code_block.clone();
        line.code_width = width;
        Some(line)
    }
}

#[cfg(feature = "editor-diagnostics")]
impl LineItem {
    pub(in crate::input) fn diagnostic_mermaid(&self) -> (Option<usize>, Option<String>) {
        (
            self.diagram.as_ref().and_then(|d| d.rows()),
            self.diagram.as_ref().and_then(|d| match d {
                DiagramLine::Message(m) => Some(m.to_string()),
                _ => None,
            }),
        )
    }
}
