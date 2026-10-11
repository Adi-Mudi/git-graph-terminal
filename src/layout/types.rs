//! Shared layout types — the frozen W1 contract (`w1-plan_v1.0.md` §"THE CONTRACT").
//!
//! This module is the one file both W1 lanes code against. It is created here by
//! lane **b1** as a temporary stand-in because lane **a1** (which owns
//! `src/layout/`) had not landed it yet; the content is byte-for-byte the
//! contract's "SHARED TYPES" block, so a1's canonical copy is identical and the
//! merge is a no-op. Renderer code (lane b1) imports these types and never
//! re-implements the layout.
//!
//! Layout invariants (from the same contract):
//! * `row` is the commit index, newest = 0 (Y axis).
//! * `lane` is the branch column index (X axis); `cell = 2 * lane` on screen.
//! * a `Segment` only ever spans **adjacent** rows: `row_to == row_from + 1`.
//! * `pers` is the z-order value deciding which lane owns the colour of a cell
//!   where two lanes cross (the terminal can only show one colour per cell).

/// Branch column index (X axis). `lane: -1` in the reference JSON maps to a
/// `VertexRef` with `on_branch: false`; a `Lane` is only meaningful when
/// `on_branch` is `true`.
pub type Lane = u8;

/// Commit index (Y axis), newest commit = row 0.
pub type RowIdx = u32;

/// A commit node in the graph grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VertexRef {
    /// Commit index (Y).
    pub row: RowIdx,
    /// Branch column index (X).
    pub lane: Lane,
    /// Index into [`GraphLayout::palette`].
    pub colour_idx: u8,
    /// Is this the checked-out commit (HEAD)?
    pub is_head: bool,
    /// Is this a stash entry?
    pub is_stash: bool,
    /// `false` for the reference's `lane: -1` / null-vertex case (D4 sentinel).
    pub on_branch: bool,
}

/// One drawn connection between two adjacent rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Segment {
    /// Row the segment starts at (top).
    pub row_from: RowIdx,
    /// Row the segment ends at (bottom); always `row_from + 1`.
    pub row_to: RowIdx,
    /// Lane at `row_from`.
    pub col_from: Lane,
    /// Lane at `row_to`.
    pub col_to: Lane,
    /// Shape family of the segment (drives the corner glyph choice).
    pub kind: SegKind,
    /// Index into [`GraphLayout::palette`].
    pub colour_idx: u8,
    /// z-order value; wins the colour of a crossing cell.
    pub pers: u8,
}

/// Shape family of a [`Segment`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegKind {
    /// Straight vertical run (same lane on both rows).
    Vertical,
    /// Slanted run between two lanes (angular/sharp corners).
    Diagonal,
    /// Horizontal connector at one row (no vertical anchors).
    Horizontal,
    /// Run with a rounded corner; the payload names the corner (renderer
    /// derives the exact glyph from segment geometry — see `render::cell`).
    RoundedCorner(u8),
    /// Merge arc between two lanes (drawn like a rounded corner).
    MergeArc,
}

/// One terminal row of the graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowLayout {
    /// Commit index (Y).
    pub row: RowIdx,
    /// The commit node on this row, if any.
    pub vertex: Option<VertexRef>,
    /// Connections leaving/entering this row.
    pub segments: Vec<Segment>,
}

/// The whole graph: rows newest-first, the widest lane, and the frozen palette.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphLayout {
    /// One entry per commit row, newest first.
    pub rows: Vec<RowLayout>,
    /// Highest lane index used anywhere in the graph.
    pub max_lane: Lane,
    /// The frozen 12-entry palette (RGB), indexed by `colour_idx % 12`.
    pub palette: [(u8, u8, u8); 12],
}
