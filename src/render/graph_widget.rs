//! The ratatui widget that paints a [`GraphLayout`] as a cell grid.
//!
//! Two cells per lane (D8 §3.1), rounded corners, and the special vertex glyphs:
//! HEAD `◯`, stash `◎`, uncommitted `┄` (grey). Colour comes from
//! [`GraphLayout::palette`] unless colour output is disabled (`NO_COLOR` /
//! `TERM=dumb`) or the row is the uncommitted row (always grey).
//!
//! Row `i` of the layout is painted on terminal row `area.y + i - scroll`, so
//! the text-column table next to it can use the exact same row indices.

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
    /// First layout row shown at `area.y` (W1: simple offset, W2 virtualises).
    pub scroll: usize,
    /// Emit colours? (`false` for `NO_COLOR` / `TERM=dumb`.)
    pub colour: bool,
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

        for (i, row) in self.layout.rows.iter().enumerate().skip(self.scroll) {
            let y = area.y + (i - self.scroll) as u16;
            if y >= area.bottom() {
                break;
            }
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
        let widget = GraphWidget {
            layout: &layout,
            tier: GlyphTier::Unicode,
            selected: None,
            uncommitted: Some(0),
            scroll: 0,
            colour: true,
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
        let widget = GraphWidget {
            layout: &layout,
            tier: GlyphTier::Ascii,
            selected: None,
            uncommitted: None,
            scroll: 1,
            colour: false,
        };
        let out = lines(widget, 4, 2);
        assert_eq!(out[0], "o"); // HEAD rendered as ascii 'o'
        assert_eq!(out[1], "*-+"); // merge vertex + ascii horizontal
    }

    #[test]
    fn selection_marks_the_whole_row() {
        let layout = demo_layout();
        let widget = GraphWidget {
            layout: &layout,
            tier: GlyphTier::Unicode,
            selected: Some(1),
            uncommitted: None,
            scroll: 0,
            colour: false,
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
        let widget = GraphWidget {
            layout: &layout,
            tier: GlyphTier::Unicode,
            selected: None,
            uncommitted: None,
            scroll: 1,
            colour: true,
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
        let widget = GraphWidget {
            layout: &layout,
            tier: GlyphTier::Unicode,
            selected: None,
            uncommitted: None,
            scroll: 0,
            colour: false,
        };
        // One cell wide: no panic, only the first column is written.
        let out = lines(widget, 1, 3);
        assert_eq!(out[0].chars().count(), 1);
    }
}
