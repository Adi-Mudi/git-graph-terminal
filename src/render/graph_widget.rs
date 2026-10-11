//! The ratatui widget that paints a [`GraphLayout`] as a cell grid.
//!
//! Two cells per lane (D8 §3.1), rounded corners, and the special vertex glyphs:
//! HEAD `◯`, stash `◎`, uncommitted `┄` (grey). Colour comes from
//! [`GraphLayout::palette`] unless colour output is disabled (`NO_COLOR` /
//! `TERM=dumb`) or the row is the uncommitted row (always grey).
//!
//! Row `i` of the layout is painted at display row `i` shifted by `scroll`
//! (expanded rows occupy more than one display row), so the text-column table
//! next to it can use the exact same display offsets and stay aligned.

use std::collections::HashSet;

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::Widget;

use crate::layout::types::{GraphLayout, RowIdx};
use crate::render::cell::{build_row, cell_of, graph_cells, vertex_glyph};
use crate::render::glyphs::GlyphTier;

/// Style applied to the uncommitted-changes row and its path.
pub const UNCOMMITTED_COLOUR: Color = Color::DarkGray;

/// A view over a [`GraphLayout`] for one terminal area.
#[derive(Debug, Clone, Copy)]
pub struct GraphWidget<'a> {
    /// The layout to draw.
    pub layout: &'a GraphLayout,
    /// Glyph capability tier.
    pub tier: GlyphTier,
    /// Highlighted row (the list selection).
    pub selected: Option<RowIdx>,
    /// The uncommitted-changes row, if the repository has any.
    pub uncommitted: Option<RowIdx>,
    /// First **display** row shown at `area.y` (expanded rows are taller than one).
    pub scroll: u16,
    /// Emit colours? (`false` for `NO_COLOR` / `TERM=dumb`.)
    pub colour: bool,
    /// Rows whose details are expanded (they reserve `detail_rows` extra rows).
    pub expanded: &'a HashSet<RowIdx>,
    /// Extra terminal rows a single expanded row occupies (W4 gap; W5 CDV).
    pub detail_rows: u16,
}

impl GraphWidget<'_> {
    /// Palette colour for a layout colour index.
    fn colour_of(&self, idx: u8) -> Color {
        let (r, g, b) = self.layout.palette[(idx as usize) % self.layout.palette.len()];
        Color::Rgb(r, g, b)
    }
}

impl Widget for GraphWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let width = graph_cells(self.layout.max_lane).min(area.width as usize);

        // `cursor` is the display row at the top of the next layout row, relative
        // to the viewport top (`scroll`), so it can be negative.
        let mut cursor: i32 = -(self.scroll as i32);
        for row in self.layout.rows.iter() {
            let height = if self.expanded.contains(&row.row) {
                1 + self.detail_rows as i32
            } else {
                1
            };
            let top = cursor;
            cursor += height;
            // A row whose top is above the viewport (`top < 0`) is scrolled off;
            // a row whose top is below the bottom ends the window.
            if top < 0 {
                continue;
            }
            if top >= area.height as i32 {
                break;
            }
            let y = area.y + top as u16;
            let uncommitted = self.uncommitted == Some(row.row);
            let selected = self.selected == Some(row.row);
            let built = build_row(row, self.layout.max_lane, self.tier, uncommitted);

            for (col, cell) in built.cells.iter().enumerate() {
                if col >= width {
                    break;
                }
                let mut style = Style::default();
                if self.colour {
                    style = match built.colours[col] {
                        Some(idx) => style.fg(self.colour_of(idx)),
                        None => style.fg(UNCOMMITTED_COLOUR),
                    };
                }
                if selected {
                    style = style.add_modifier(Modifier::REVERSED);
                }
                buf[(area.x + col as u16, y)]
                    .set_char(cell.glyph)
                    .set_style(style);
            }

            // The vertex is a node, so it is painted last and wins its cell.
            if let Some(vertex) = &row.vertex {
                let col = cell_of(vertex.lane);
                if col < width {
                    let glyph = vertex_glyph(vertex, self.tier.glyphs(), uncommitted);
                    let mut style = Style::default();
                    if self.colour {
                        style = if uncommitted {
                            style.fg(UNCOMMITTED_COLOUR)
                        } else {
                            style
                                .fg(self.colour_of(vertex.colour_idx))
                                .add_modifier(Modifier::BOLD)
                        };
                    }
                    if selected {
                        style = style.add_modifier(Modifier::REVERSED);
                    }
                    buf[(area.x + col as u16, y)]
                        .set_char(glyph)
                        .set_style(style);
                }
            }

            // Keep the highlight a full-width bar even where the graph is empty.
            if selected {
                for col in built.cells.len()..width {
                    buf[(area.x + col as u16, y)]
                        .set_style(Style::default().add_modifier(Modifier::REVERSED));
                }
            }

            // Reserved expansion gap: repeat the down-going lane verticals so the
            // graph stays visually connected across the taller row (a `Segment`'s
            // `col_to` is the lane that continues into the next row).
            if height > 1 {
                for step in 1..height {
                    let gap_y = area.y + (top + step) as u16;
                    if gap_y >= area.bottom() {
                        break;
                    }
                    for segment in &row.segments {
                        let col = cell_of(segment.col_to);
                        if col >= width {
                            break;
                        }
                        let mut style = Style::default();
                        if self.colour {
                            style = style.fg(self.colour_of(segment.colour_idx));
                        }
                        if selected {
                            style = style.add_modifier(Modifier::REVERSED);
                        }
                        buf[(area.x + col as u16, gap_y)]
                            .set_char(self.tier.glyphs().vertical)
                            .set_style(style);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::glyphs::GlyphTier;
    use crate::render::mock::demo_layout;

    fn lines(widget: GraphWidget<'_>, width: u16, height: u16) -> Vec<String> {
        let area = Rect::new(0, 0, width, height);
        let mut buf = Buffer::empty(area);
        widget.render(area, &mut buf);
        (0..height)
            .map(|y| {
                (0..width)
                    .map(|x| buf[(x, y)].symbol().to_string())
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect()
    }

    #[test]
    fn paints_the_mock_graph_vertically_connected() {
        let layout = demo_layout();
        let expanded = HashSet::new();
        let widget = GraphWidget {
            layout: &layout,
            tier: GlyphTier::Unicode,
            selected: None,
            uncommitted: Some(0),
            scroll: 0,
            colour: true,
            expanded: &expanded,
            detail_rows: 0,
        };
        let out = lines(widget, 4, 5);
        assert_eq!(out[0], "┄"); // uncommitted stub
        assert_eq!(out[1], "◯"); // HEAD open circle
        assert_eq!(out[2], "●─╮"); // merge: vertex + rounded arc down to lane 1
        assert_eq!(out[3], "│ ●"); // feature-branch tip
        assert_eq!(out[4], "│ ◎"); // stash nested circle on lane 1
    }

    #[test]
    fn scroll_shifts_the_visible_window() {
        let layout = demo_layout();
        let expanded = HashSet::new();
        let widget = GraphWidget {
            layout: &layout,
            tier: GlyphTier::Ascii,
            selected: None,
            uncommitted: None,
            scroll: 1,
            colour: false,
            expanded: &expanded,
            detail_rows: 0,
        };
        let out = lines(widget, 4, 2);
        assert_eq!(out[0], "o"); // HEAD rendered as ascii 'o'
        assert_eq!(out[1], "*-+"); // merge vertex + ascii horizontal
    }

    #[test]
    fn selection_marks_the_whole_row() {
        let layout = demo_layout();
        let expanded = HashSet::new();
        let widget = GraphWidget {
            layout: &layout,
            tier: GlyphTier::Unicode,
            selected: Some(1),
            uncommitted: None,
            scroll: 0,
            colour: false,
            expanded: &expanded,
            detail_rows: 0,
        };
        let area = Rect::new(0, 0, 4, 2);
        let mut buf = Buffer::empty(area);
        widget.render(area, &mut buf);
        for x in 0..4 {
            assert!(buf[(x, 1)]
                .style()
                .add_modifier
                .contains(Modifier::REVERSED));
        }
    }

    #[test]
    fn colour_follows_the_frozen_palette() {
        let layout = demo_layout();
        let expanded = HashSet::new();
        let widget = GraphWidget {
            layout: &layout,
            tier: GlyphTier::Unicode,
            selected: None,
            uncommitted: None,
            scroll: 1,
            colour: true,
            expanded: &expanded,
            detail_rows: 0,
        };
        let area = Rect::new(0, 0, 4, 1);
        let mut buf = Buffer::empty(area);
        widget.render(area, &mut buf);
        // Row 1 is the HEAD vertex, palette entry 0.
        assert_eq!(buf[(0, 0)].fg, Color::Rgb(0, 133, 217));
    }

    #[test]
    fn clips_to_the_area_width() {
        let layout = demo_layout();
        let expanded = HashSet::new();
        let widget = GraphWidget {
            layout: &layout,
            tier: GlyphTier::Unicode,
            selected: None,
            uncommitted: None,
            scroll: 0,
            colour: false,
            expanded: &expanded,
            detail_rows: 0,
        };
        // One cell wide: no panic, only the first column is written.
        let out = lines(widget, 1, 3);
        assert_eq!(out[0].chars().count(), 1);
    }

    #[test]
    fn expanded_row_reserves_the_gap_and_keeps_verticals() {
        let layout = demo_layout();
        // Layout row 2 is the merge `●─╮` (row 0 uncommitted, row 1 HEAD).
        let mut expanded = HashSet::new();
        expanded.insert(2);
        let widget = GraphWidget {
            layout: &layout,
            tier: GlyphTier::Unicode,
            selected: None,
            uncommitted: Some(0),
            scroll: 0,
            colour: true,
            expanded: &expanded,
            detail_rows: 4,
        };
        let out = lines(widget, 4, 8);
        assert_eq!(out[2], "●─╮", "the expanded row's own line is unchanged");
        for (offset, line) in out.iter().enumerate().take(7).skip(3) {
            assert!(
                line.contains('│'),
                "gap row {offset} must carry through-lines, got {line:?}"
            );
        }
        assert_eq!(out[7], "│ ●", "the next commit is pushed down by four rows");
    }
}
