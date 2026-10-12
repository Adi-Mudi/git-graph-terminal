//! The ratatui widget that paints a [`GraphLayout`] as a cell grid.
//!
//! Two cells per lane (D8 §3.1), rounded corners, and the special vertex glyphs:
//! HEAD `◯`, stash `◎`, uncommitted `┄` (grey). Colour comes from
//! [`GraphLayout::palette`] quantised through the terminal's [`Ladder`], so a
//! 256-colour or 16-colour terminal still gets a distinguishable graph and a mono
//! terminal gets none.
//!
//! Row `i` of the layout is painted at display row `i` shifted by `scroll`
//! (expanded rows occupy more than one display row), so the text-column table
//! next to it can use the exact same display offsets and stay aligned.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::Widget;

use crate::layout::types::{GraphLayout, RowIdx};
use crate::render::cell::{build_row, cell_of, graph_cells, vertex_glyph};
use crate::term::ladder::Ladder;

/// A view over a [`GraphLayout`] for one terminal area.
#[derive(Debug, Clone, Copy)]
pub struct GraphWidget<'a> {
    /// The layout to draw.
    pub layout: &'a GraphLayout,
    /// Terminal capability ladder: glyph rung, colour rung, wide-glyph guard.
    pub ladder: Ladder,
    /// Highlighted row (the list selection).
    pub selected: Option<RowIdx>,
    /// The uncommitted-changes row, if the repository has any.
    pub uncommitted: Option<RowIdx>,
    /// First **display** row shown at `area.y` (expanded rows are taller than one).
    pub scroll: u16,
    /// Rows shown **before** the first layout row — the inline uncommitted
    /// section, which the Overview tab draws above the history (M4-W12/T4).
    pub leading: u16,
    /// The display-row height of every layout row, indexed by row (M4-W12/T1:
    /// a block is `refs + commit + details + diff`). A missing entry is `1`.
    pub heights: &'a [u16],
    /// The rows each layout row spends on its refs line before its commit
    /// (M4-W12/T7). The vertex and the commit's lane cells belong to the commit
    /// row, which therefore sits this many display rows below the block's top.
    /// A missing entry is `0`.
    pub refs: &'a [u16],
    /// The rows to draw, in order. The Branches tab hides the rows its filter
    /// excludes, so the widget can no longer assume `0..rows.len()`.
    pub visible: &'a [RowIdx],
}

impl Widget for GraphWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if area.width == 0 || area.height == 0 {
            return;
        }
        let width = graph_cells(self.layout.max_lane).min(area.width as usize);

        // `cursor` is the display row at the top of the next layout row, relative
        // to the viewport top (`scroll`), so it can be negative.
        let mut cursor: i32 = self.leading as i32 - self.scroll as i32;
        for &index in self.visible {
            let Some(row) = self.layout.rows.get(index as usize) else {
                continue;
            };
            let height = self
                .heights
                .get(index as usize)
                .copied()
                .unwrap_or(1)
                .max(1) as i32;
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
            // T7: the refs line owns the block's first display row, so the
            // commit's cells and vertex sit `refs` rows lower. When that lands
            // past the bottom, only the refs line and the gap rows show.
            let refs = self.refs.get(index as usize).copied().unwrap_or(0) as i32;
            let commit_top = top + refs;
            let commit_on_screen = commit_top < area.height as i32;
            let y = area.y + commit_top as u16;
            let uncommitted = self.uncommitted == Some(index);
            let selected = self.selected == Some(index);
            let glyphs = self.ladder.glyphs();
            let built = build_row(
                row,
                self.layout.max_lane,
                self.ladder.glyph_tier(),
                uncommitted,
            );

            if commit_on_screen {
                for (col, cell) in built.cells.iter().enumerate() {
                    if col >= width {
                        break;
                    }
                    let mut style = Style::default();
                    let colour = match built.colours[col] {
                        Some(idx) => self.ladder.colour_of(&self.layout.palette, idx),
                        None => self.ladder.uncommitted_colour(),
                    };
                    if let Some(colour) = colour {
                        style = style.fg(colour);
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
                        let glyph = vertex_glyph(vertex, glyphs, uncommitted);
                        let mut style = Style::default();
                        let colour = if uncommitted {
                            self.ladder.uncommitted_colour()
                        } else {
                            self.ladder
                                .colour_of(&self.layout.palette, vertex.colour_idx)
                        };
                        if let Some(colour) = colour {
                            style = style.fg(colour);
                            if !uncommitted {
                                style = style.add_modifier(Modifier::BOLD);
                            }
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

            // Reserved gap rows: the refs line above the commit (T7) and any
            // expansion below it. Repeat the down-going lane verticals so the
            // graph stays visually connected across the taller row (a `Segment`'s
            // `col_to` is the lane that continues into the next row). The
            // commit's own row holds the cells and the vertex, so it is skipped.
            if height > 1 {
                for step in 0..height {
                    if step == refs {
                        continue;
                    }
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
                        if let Some(colour) = self
                            .ladder
                            .colour_of(&self.layout.palette, segment.colour_idx)
                        {
                            style = style.fg(colour);
                        }
                        if selected {
                            style = style.add_modifier(Modifier::REVERSED);
                        }
                        buf[(area.x + col as u16, gap_y)]
                            .set_char(glyphs.vertical)
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
    use crate::render::mock::demo_layout;
    use crate::term::ladder::ColourTier;
    use ratatui::style::Color;

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

    /// One display row per layout row; `expanded` rows get `1 + detail`.
    fn heights(count: usize, expanded: &[usize], detail: u16) -> Vec<u16> {
        (0..count)
            .map(|i| if expanded.contains(&i) { 1 + detail } else { 1 })
            .collect()
    }

    /// Every row index of `demo_layout()`, in order.
    fn all_rows() -> Vec<RowIdx> {
        (0..demo_layout().rows.len() as RowIdx).collect()
    }

    #[test]
    fn paints_the_mock_graph_vertically_connected() {
        let layout = demo_layout();
        let h = heights(layout.rows.len(), &[], 0);
        let v = all_rows();
        let widget = GraphWidget {
            layout: &layout,
            ladder: Ladder::unlimited(),
            selected: None,
            uncommitted: Some(0),
            scroll: 0,
            leading: 0,
            heights: &h,
            refs: &[],
            visible: &v,
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
        let h = heights(layout.rows.len(), &[], 0);
        let v = all_rows();
        let widget = GraphWidget {
            layout: &layout,
            ladder: Ladder::mono_ascii(),
            selected: None,
            uncommitted: None,
            scroll: 1,
            leading: 0,
            heights: &h,
            refs: &[],
            visible: &v,
        };
        let out = lines(widget, 4, 2);
        assert_eq!(out[0], "o"); // HEAD rendered as ascii 'o'
        assert_eq!(out[1], "*-+"); // merge vertex + ascii horizontal
    }

    #[test]
    fn leading_rows_push_the_graph_down() {
        // M4-W12/T4: the uncommitted section sits above the first commit, so
        // the graph must start one row lower instead of at the very top.
        let layout = demo_layout();
        let h = heights(layout.rows.len(), &[], 0);
        let v = all_rows();
        let widget = GraphWidget {
            layout: &layout,
            ladder: Ladder::unlimited(),
            selected: None,
            uncommitted: Some(0),
            scroll: 0,
            leading: 2,
            heights: &h,
            refs: &[],
            visible: &v,
        };
        let out = lines(widget, 4, 4);
        assert_eq!(out[0], "", "the leading rows stay empty");
        assert_eq!(out[1], "", "the leading rows stay empty");
        assert_eq!(out[2], "┄", "row 0 now starts at display row 2");
        assert_eq!(out[3], "◯");
    }

    #[test]
    fn selection_marks_the_whole_row() {
        let layout = demo_layout();
        let h = heights(layout.rows.len(), &[], 0);
        let v = all_rows();
        let widget = GraphWidget {
            layout: &layout,
            ladder: Ladder::unlimited().with_colour(ColourTier::Mono),
            selected: Some(1),
            uncommitted: None,
            scroll: 0,
            leading: 0,
            heights: &h,
            refs: &[],
            visible: &v,
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
        let h = heights(layout.rows.len(), &[], 0);
        let v = all_rows();
        let widget = GraphWidget {
            layout: &layout,
            ladder: Ladder::unlimited(),
            selected: None,
            uncommitted: None,
            scroll: 1,
            leading: 0,
            heights: &h,
            refs: &[],
            visible: &v,
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
        let h = heights(layout.rows.len(), &[], 0);
        let v = all_rows();
        let widget = GraphWidget {
            layout: &layout,
            ladder: Ladder::unlimited().with_colour(ColourTier::Mono),
            selected: None,
            uncommitted: None,
            scroll: 0,
            leading: 0,
            heights: &h,
            refs: &[],
            visible: &v,
        };
        // One cell wide: no panic, only the first column is written.
        let out = lines(widget, 1, 3);
        assert_eq!(out[0].chars().count(), 1);
    }

    #[test]
    fn expanded_row_reserves_the_gap_and_keeps_verticals() {
        let layout = demo_layout();
        // Layout row 2 is the merge `●─╮` (row 0 uncommitted, row 1 HEAD).
        let h = heights(layout.rows.len(), &[2], 4);
        let v = all_rows();
        let widget = GraphWidget {
            layout: &layout,
            ladder: Ladder::unlimited(),
            selected: None,
            uncommitted: Some(0),
            scroll: 0,
            leading: 0,
            heights: &h,
            refs: &[],
            visible: &v,
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
