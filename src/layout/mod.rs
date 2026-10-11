//! Graph layout — **stub (P0 scaffold only, nothing runs here yet).**
//!
//! W1 fills this module. The shape is already locked by the D8 binding rulings
//! (`.IDE_Plans/d8-recommendation.md`):
//!
//! * `§3.1` — **2 cells per lane**: `cell = 2 * lane`, first cell = 0, the commit
//!   text column starts at `2 * max_lane + 2` (same `+2` pad as `git`); the
//!   webview's `offsetX = 16` focus inset folds into that text offset.
//! * `§3.2` — crossing cells use a z-order model, **one lane wins the colour**:
//!   `Cell { glyph, lane, pers }` (there is no way to show two colours in one
//!   terminal cell; documented and tested deviation).
//! * `§1` — port `Vertex`, `Branch`, `Graph::determinePath()`,
//!   `getAvailableColour()`, `connections`/`registerUnavailablePoint`/`getNextPoint`
//!   and the four navigation queries **verbatim in behaviour** from
//!   `../vscode-git-graph/web/graph.ts`.
//! * `§3.4.2` — **colour index is not column index**: `getAvailableColour()`
//!   indexes the fixed 12-entry palette, columns come from
//!   `Vertex::next_x`/`getNextPoint`/`registerUnavailablePoint`. Two independent
//!   counters; tying them together breaks column compaction (port risk #1).
//!
//! Golden-fixture parity for this module is produced by `tools/g8-dump`
//! (see `tools/g8-dump/README.md`, P0 T3).

// These are deliberate P0 stubs: W1 consumes them. Kept warning-free so
// `cargo clippy -- -D warnings` (CI) stays green in the scaffold.
#![allow(dead_code)]

/// Number of terminal cells per lane — D8 ruling `§3.1`.
pub const CELLS_PER_LANE: usize = 2;

/// One terminal cell of the graph grid — D8 ruling `§3.2`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    /// Glyph drawn in this cell (depends on the G2 capability tier: Unicode / ASCII / dumb).
    pub glyph: char,
    /// Lane (grid column) this cell belongs to.
    pub lane: usize,
    /// z-order personality: which lane owns the colour when two lanes cross here.
    pub pers: usize,
}

/// Placeholder for the W1 port of the reference `Graph` class. No behaviour in P0.
#[derive(Debug, Default)]
pub struct Layout;
