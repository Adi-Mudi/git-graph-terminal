//! Shared layout types — the **a1 ↔ b1 contract** from `w1-plan_v1.0.md`
//! ("THE CONTRACT (exact)"). Field names and types are fixed; both lanes code
//! against them. a1 owns this file, b1 only imports it.
//!
//! Everything here is plain data: no dependency on `ratatui`, no rendering, no
//! git. `layout()` (in the parent module) fills these in from `Commit[]`.

/// Branch column index (graph X axis). One lane = two terminal cells (D8 §3.1).
pub type Lane = u8;

/// Commit index (graph Y axis), newest commit = row 0.
///
/// Rows are adjacent: a `Segment` always spans `row_from -> row_from + 1`.
pub type RowIdx = u32;

/// One commit node as the renderer sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VertexRef {
    /// Row this vertex lives on (equals the commit index).
    pub row: RowIdx,
    /// Grid column of the node (reference `Vertex.x`).
    pub lane: Lane,
    /// Index into [`GraphLayout::palette`] (reference `Vertex.getColour()`), **not**
    /// a column (D8 §3.4.2). `0` when the vertex has no branch.
    pub colour_idx: u8,
    /// Is this commit the checked-out commit (HEAD)?
    pub is_head: bool,
    /// Is this commit a stash entry (`refs/stash`)?
    pub is_stash: bool,
    /// Does the reference draw a node here (`onBranch !== null`)? Always `true` for
    /// everything `layout()` produces from a real `git log` — see `mod.rs` for the
    /// reachability note on the `lane: -1` sentinel.
    pub on_branch: bool,
}

/// One drawn graph line between two **adjacent** rows.
///
/// `row_to == row_from + 1` always. `col_from`/`col_to` are lane indices. When
/// `col_from != col_to` the line changes lane between the two rows; the renderer
/// turns that into a corner/bevel (evidence: reference `Branch.lines`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Segment {
    /// Row the segment starts at (its top node's row).
    pub row_from: RowIdx,
    /// Row the segment ends at; always `row_from + 1`.
    pub row_to: RowIdx,
    /// Lane at `row_from`.
    pub col_from: Lane,
    /// Lane at `row_to`.
    pub col_to: Lane,
    /// Geometric shape of the segment, derived only from `(col_from, col_to)`.
    pub kind: SegKind,
    /// Palette index of the branch this segment belongs to (same meaning as
    /// [`VertexRef::colour_idx`]). Two segments crossing in one cell: the higher
    /// `pers` wins the colour (D8 §3.2).
    pub colour_idx: u8,
    /// z-order winner / personality lane for a crossing cell: `p1.x` when the line
    /// is locked to its start, else `p2.x` (reference `Line.lockedFirst`).
    pub pers: u8,
}

/// Shape of a [`Segment`].
///
/// a1 derives the variant purely from the geometry of `(col_from, col_to)` so the
/// G8 parity test can re-derive the expected value from the golden JSON:
/// * `col_from == col_to`            -> [`SegKind::Vertical`]
/// * `|col_to - col_from| == 1`      -> [`SegKind::RoundedCorner(0)`] when moving
///   right (`col_to > col_from`), [`SegKind::RoundedCorner(1)`] when moving left
/// * anything wider                  -> [`SegKind::Horizontal`]
///
/// `Diagonal` and `MergeArc` are reserved for later renderer styles (D8: rounded
/// is the default, angular/diagonal is M3). The geometry port never emits them,
/// because the golden JSON records only coordinates, not the style variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SegKind {
    /// Straight up/down within one lane.
    Vertical,
    /// A one-lane-per-row diagonal (reserved, angular styling).
    Diagonal,
    /// A line that jumps more than one lane between two rows.
    Horizontal,
    /// A one-lane shift; the payload is the direction (`0` = right, `1` = left).
    RoundedCorner(u8),
    /// A branch line that merges into an existing branch (reserved).
    MergeArc,
}

/// One rendered row: the node (if any) plus every segment that starts on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowLayout {
    /// Row index (== commit index, newest = 0).
    pub row: RowIdx,
    /// The commit node on this row; `None` when the row has no node.
    pub vertex: Option<VertexRef>,
    /// Segments whose `row_from == row`. Ordered by the branch that owns them.
    pub segments: Vec<Segment>,
}

/// The whole laid-out graph, ready for the renderer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphLayout {
    /// One entry per commit, in log order (newest first).
    pub rows: Vec<RowLayout>,
    /// Highest lane index in use. The commit text column starts at
    /// `2 * max_lane + 2` (D8 §3.1).
    pub max_lane: Lane,
    /// The frozen 12-entry colour palette (reference `config.ts:275-277`), RGB.
    pub palette: [(u8, u8, u8); 12],
}
