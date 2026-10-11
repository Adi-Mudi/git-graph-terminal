//! Cell-grid builder — turns a [`RowLayout`]'s [`Segment`]s into terminal cells
//! with a z-order colour winner (D8 §3.1 / §3.2).
//!
//! Geometry contract (D8 §3.1): **2 cells per lane**, `cell = 2 * lane`, the
//! text column starts at `2 * max_lane + 2` — i.e. [`graph_cells`]. Every
//! segment spans adjacent rows (`row_to == row_from + 1`), so one row of cells
//! is built independently.
//!
//! Per row the builder accumulates a 4-bit connection mask (`UP`/`DOWN`/`LEFT`/
//! `RIGHT`) per grid column and then maps the mask to a box-drawing glyph:
//!
//! ```text
//! vertical  UP|DOWN        horizontal  LEFT|RIGHT
//! rounded   UP|RIGHT ╰     sharp       UP|RIGHT └   (SegKind::Diagonal)
//! crossing  all four → ┼ (Unicode) / + (ASCII) / one-lane-wins (Dumb)
//! ```
//!
//! **Lane changes are always drawn as a connected step** — a corner at `col_from`
//! (UP + toward `col_to`), a horizontal run, and a corner at `col_to` (DOWN +
//! back toward `col_from`) — whatever the segment's `SegKind` is, and however far
//! the lane moves. Lane a1's canonical `types.rs` classifies a jump wider than one
//! lane as `SegKind::Horizontal`; drawing that as a bare horizontal bar would
//! leave the line unconnected to the next row, so `Horizontal` gets the same
//! corners. This is the W2 integration point flagged in `b1-report.md`.
//!
//! **z-order / colour winner:** when two lanes share a cell (a crossing, or a
//! corner that meets a through-line) exactly one colour is shown. The
//! contribution with the **higher `pers`** value is on top and therefore wins
//! the cell's colour; ties keep the first contribution. This is the deliberate,
//! documented deviation from the reference SVG (a terminal cell has one colour)
//! and is the one rule lane a1 and lane b1 must agree on — flagged for the W2
//! integration review.
//!
//! [`Cell`] itself is exactly the contract shape `{glyph, lane, pers}`; the
//! winning colour index is returned alongside it in [`BuiltRow::colours`] so the
//! contract struct stays frozen.

use crate::layout::types::{Lane, RowLayout, SegKind, Segment, VertexRef};
use crate::render::glyphs::{Corner, GlyphSet, GlyphTier};

/// Terminal cells per lane (D8 §3.1).
pub const CELLS_PER_LANE: usize = 2;

// Connection bits for one grid cell.
const UP: u8 = 1;
const DOWN: u8 = 2;
const LEFT: u8 = 4;
const RIGHT: u8 = 8;

/// One terminal cell of the graph grid — frozen contract type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    /// Glyph drawn in this cell (depends on the [`GlyphTier`]).
    pub glyph: char,
    /// Grid column (lane) this cell belongs to.
    pub lane: usize,
    /// z-order value of the winning segment (higher wins the colour).
    pub pers: usize,
}

/// One built row: the contract [`Cell`]s plus the winning palette index per
/// cell (`None` = draw without colour, e.g. the uncommitted row).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuiltRow {
    /// One cell per grid column (`graph_cells(max_lane)` entries).
    pub cells: Vec<Cell>,
    /// Palette index for each cell, parallel to `cells`.
    pub colours: Vec<Option<u8>>,
}

/// Screen column of lane `lane` — `2 * lane`.
pub const fn cell_of(lane: Lane) -> usize {
    CELLS_PER_LANE * lane as usize
}

/// Width of the graph area in cells — `2 * max_lane + 2` (the `+2` pad D8 §3.1).
pub const fn graph_cells(max_lane: Lane) -> usize {
    CELLS_PER_LANE * (max_lane as usize + 1)
}

/// Glyph for a vertex, honouring uncommitted → HEAD → stash → commit, and the
/// `on_branch: false` sentinel (D4) which draws nothing.
pub fn vertex_glyph(vertex: &VertexRef, glyphs: &GlyphSet, uncommitted: bool) -> char {
    if uncommitted {
        return glyphs.uncommitted;
    }
    if !vertex.on_branch {
        return ' ';
    }
    if vertex.is_head {
        glyphs.head
    } else if vertex.is_stash {
        glyphs.stash
    } else {
        glyphs.commit
    }
}

/// Build the cells for one row.
///
/// `uncommitted` marks the uncommitted-changes row (the contract's `VertexRef`
/// has no such flag, so the caller — the app/mock — supplies the row index).
pub fn build_row(row: &RowLayout, max_lane: Lane, tier: GlyphTier, uncommitted: bool) -> BuiltRow {
    let glyphs = tier.glyphs();
    let width = graph_cells(max_lane);
    let mut conns = vec![Conn::default(); width];

    for seg in &row.segments {
        // Contract invariant: segments only join adjacent rows.
        if seg.row_to != seg.row_from + 1 {
            continue;
        }
        let from = cell_of(seg.col_from);
        let to = cell_of(seg.col_to);
        if from >= width || to >= width {
            continue;
        }
        // Diagonal runs use sharp corners; everything else (rounded corners,
        // merge arcs, verticals) is drawn rounded.
        let curved = !matches!(seg.kind, SegKind::Diagonal);

        if from == to {
            add(&mut conns[from], UP | DOWN, curved, seg);
            continue;
        }

        // A one-row lane change: corners at both ends, horizontal in between.
        // This also covers a wide jump (`|col_to - col_from| > 1`), which a1
        // labels `SegKind::Horizontal` but which must still connect to the next
        // row — a bare bar would not.
        let rightward = from < to;
        let (lo, hi) = (from.min(to), from.max(to));
        let start_dir = if rightward { RIGHT } else { LEFT };
        let end_dir = if rightward { LEFT } else { RIGHT };
        add(&mut conns[from], UP | start_dir, curved, seg);
        add(&mut conns[to], DOWN | end_dir, curved, seg);
        for cell in &mut conns[(lo + 1)..hi] {
            add(cell, LEFT | RIGHT, curved, seg);
        }
    }

    let mut cells = Vec::with_capacity(width);
    let mut colours = Vec::with_capacity(width);
    for (col, conn) in conns.iter().enumerate() {
        let lane = col / CELLS_PER_LANE;
        if !conn.set {
            cells.push(Cell {
                glyph: ' ',
                lane,
                pers: 0,
            });
            colours.push(None);
            continue;
        }
        let mut glyph = glyph_for(conn.mask, conn.curved, glyphs, tier);
        if uncommitted && conn.mask == UP | DOWN {
            glyph = glyphs.dashed_vertical;
        }
        cells.push(Cell {
            glyph,
            lane,
            pers: conn.pers as usize,
        });
        colours.push(if uncommitted { None } else { Some(conn.colour) });
    }

    BuiltRow { cells, colours }
}

/// Accumulated connections for one grid column.
#[derive(Clone, Copy, Default)]
struct Conn {
    mask: u8,
    curved: bool,
    pers: u8,
    colour: u8,
    set: bool,
}

/// Merge a segment's contribution into a cell; the higher `pers` wins the colour.
fn add(cell: &mut Conn, mask: u8, curved: bool, seg: &Segment) {
    if !cell.set {
        *cell = Conn {
            mask,
            curved,
            pers: seg.pers,
            colour: seg.colour_idx,
            set: true,
        };
        return;
    }
    cell.mask |= mask;
    cell.curved |= curved;
    if seg.pers > cell.pers {
        cell.pers = seg.pers;
        cell.colour = seg.colour_idx;
    }
}

/// Map a connection mask to a glyph. Dumb tier resolves crossings with
/// one-lane-wins (the vertical survives), per D8 §3.2.
fn glyph_for(mask: u8, curved: bool, glyphs: &GlyphSet, tier: GlyphTier) -> char {
    let corner = |which: Corner| {
        if curved {
            glyphs.rounded_corner(which)
        } else {
            glyphs.sharp_corner(which)
        }
    };
    match mask {
        m if m == UP | DOWN => glyphs.vertical,
        m if m == LEFT | RIGHT => glyphs.horizontal,
        m if m == UP | RIGHT => corner(Corner::UpRight),
        m if m == UP | LEFT => corner(Corner::UpLeft),
        m if m == DOWN | RIGHT => corner(Corner::DownRight),
        m if m == DOWN | LEFT => corner(Corner::DownLeft),
        m if m == UP | LEFT | RIGHT => glyphs.tee_missing_down,
        m if m == DOWN | LEFT | RIGHT => glyphs.tee_missing_up,
        m if m == UP | DOWN | RIGHT => glyphs.tee_missing_left,
        m if m == UP | DOWN | LEFT => glyphs.tee_missing_right,
        m if m == UP | DOWN | LEFT | RIGHT => {
            if tier == GlyphTier::Dumb {
                glyphs.vertical
            } else {
                glyphs.cross
            }
        }
        // Dangling single-direction stubs (defensive; not produced by the port).
        m if m == UP || m == DOWN => glyphs.vertical,
        m if m == LEFT || m == RIGHT => glyphs.horizontal,
        _ => ' ',
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::mock::frozen_palette;

    fn seg(from: Lane, to: Lane, kind: SegKind, colour: u8, pers: u8) -> Segment {
        Segment {
            row_from: 1,
            row_to: 2,
            col_from: from,
            col_to: to,
            kind,
            colour_idx: colour,
            pers,
        }
    }

    fn vertex(lane: Lane, colour: u8) -> VertexRef {
        VertexRef {
            row: 1,
            lane,
            colour_idx: colour,
            is_head: false,
            is_stash: false,
            on_branch: true,
        }
    }

    fn row(row: u32, segments: Vec<Segment>) -> RowLayout {
        RowLayout {
            row,
            vertex: None,
            segments,
        }
    }

    fn glyphs(row: &RowLayout, tier: GlyphTier) -> String {
        build_row(row, 2, tier, false)
            .cells
            .iter()
            .map(|c| c.glyph)
            .collect()
    }

    #[test]
    fn pitch_and_width_follow_d8() {
        assert_eq!(cell_of(0), 0);
        assert_eq!(cell_of(1), 2);
        assert_eq!(cell_of(4), 8);
        assert_eq!(graph_cells(0), 2);
        assert_eq!(graph_cells(2), 6);
        assert_eq!(graph_cells(9), 20);
    }

    #[test]
    fn vertical_is_drawn_at_the_lane_cell() {
        let r = row(1, vec![seg(1, 1, SegKind::Vertical, 0, 0)]);
        assert_eq!(glyphs(&r, GlyphTier::Unicode), "  │   ");
    }

    #[test]
    fn adjacent_lane_transition_is_a_rounded_step() {
        let r = row(1, vec![seg(0, 1, SegKind::RoundedCorner(0), 0, 0)]);
        // lane0 ╰, between-cell ─, lane1 ╮ (the down arm lands on the next row)
        assert_eq!(glyphs(&r, GlyphTier::Unicode), "╰─╮   ");
    }

    #[test]
    fn transition_right_to_left_uses_the_mirrored_corners() {
        let r = row(1, vec![seg(1, 0, SegKind::RoundedCorner(0), 0, 0)]);
        assert_eq!(glyphs(&r, GlyphTier::Unicode), "╭─╯   ");
    }

    #[test]
    fn diagonal_uses_sharp_corners() {
        let r = row(1, vec![seg(0, 1, SegKind::Diagonal, 0, 0)]);
        assert_eq!(glyphs(&r, GlyphTier::Unicode), "└─┐   ");
    }

    #[test]
    fn a_wide_lane_jump_stays_connected() {
        // a1 emits `Horizontal` for a jump wider than one lane; it must still
        // turn down at the target lane, not render as a bare bar.
        let r = row(1, vec![seg(0, 2, SegKind::Horizontal, 4, 0)]);
        assert_eq!(glyphs(&r, GlyphTier::Unicode), "╰───╮ ");
        let built = build_row(&r, 2, GlyphTier::Unicode, false);
        assert_eq!(built.colours[4], Some(4), "the far corner keeps the colour");
    }

    #[test]
    fn crossing_uses_the_cross_glyph_per_tier() {
        // A wide lane jump's horizontal run passes through lane 1, where the
        // lane-1 vertical also runs: that cell carries all four connections.
        let r = row(
            1,
            vec![
                seg(1, 1, SegKind::Vertical, 3, 0),
                seg(0, 2, SegKind::Horizontal, 5, 1),
            ],
        );
        let built = build_row(&r, 2, GlyphTier::Unicode, false);
        assert_eq!(built.cells[2].glyph, '┼');
        assert_eq!(
            build_row(&r, 2, GlyphTier::Ascii, false).cells[2].glyph,
            '+'
        );
        assert_eq!(build_row(&r, 2, GlyphTier::Dumb, false).cells[2].glyph, '|');
    }

    #[test]
    fn higher_pers_wins_the_crossing_colour() {
        let r = row(
            1,
            vec![
                seg(1, 1, SegKind::Vertical, 3, 0),
                seg(0, 2, SegKind::Horizontal, 5, 1),
            ],
        );
        let built = build_row(&r, 2, GlyphTier::Unicode, false);
        assert_eq!(built.colours[2], Some(5), "higher pers is on top");
        assert_eq!(built.cells[2].pers, 1);
    }

    #[test]
    fn vertex_glyphs_cover_head_stash_and_uncommitted() {
        let g = GlyphTier::Unicode.glyphs();
        let mut v = vertex(0, 0);
        v.is_head = true;
        assert_eq!(vertex_glyph(&v, g, false), '◯');
        let mut s = vertex(0, 0);
        s.is_stash = true;
        assert_eq!(vertex_glyph(&s, g, false), '◎');
        assert_eq!(vertex_glyph(&vertex(0, 0), g, false), '●');
        assert_eq!(vertex_glyph(&vertex(0, 0), g, true), '┄');
        let mut off = vertex(0, 0);
        off.on_branch = false;
        assert_eq!(vertex_glyph(&off, g, false), ' ');
    }

    #[test]
    fn uncommitted_row_uses_the_dashed_vertical() {
        let r = row(1, vec![seg(0, 0, SegKind::Vertical, 0, 0)]);
        let built = build_row(&r, 0, GlyphTier::Unicode, true);
        assert_eq!(built.cells[0].glyph, '┆');
        assert_eq!(built.colours[0], None);
    }

    #[test]
    fn segments_outside_the_grid_are_ignored() {
        let r = row(1, vec![seg(0, 5, SegKind::RoundedCorner(0), 0, 0)]);
        assert_eq!(glyphs(&r, GlyphTier::Unicode), "      ");
    }

    #[test]
    fn mock_layout_builds_every_row_without_panicking() {
        let layout = crate::render::mock::demo_layout();
        assert_eq!(layout.palette, frozen_palette());
        for r in &layout.rows {
            let built = build_row(r, layout.max_lane, GlyphTier::Unicode, false);
            assert_eq!(built.cells.len(), graph_cells(layout.max_lane));
            assert_eq!(built.colours.len(), built.cells.len());
        }
    }
}
