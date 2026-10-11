//! Graph layout — the W1 port of the reference `Graph` layout algorithm
//! (`../vscode-git-graph/web/graph.ts`, read-only).
//!
//! The shape is locked by the D8 binding rulings (`.IDE_Plans/d8-recommendation.md`)
//! and the a1↔b1 contract in `w1-plan_v1.0.md`:
//!
//! * `§3.1` — **2 cells per lane**: `cell = 2 * lane`, first cell = 0, the commit
//!   text column starts at `2 * max_lane + 2`.
//! * `§3.2` — crossing cells use a z-order model, **one lane wins the colour**:
//!   `Cell { glyph, lane, pers }`.
//! * `§1` — port `Vertex`, `Branch`, `Graph::determinePath()`,
//!   `getAvailableColour()`, `connections`/`registerUnavailablePoint`/`getNextPoint`
//!   and the four navigation queries **verbatim in behaviour**.
//! * `§3.4.2` — **colour index is not column index**.
//!
//! The port keeps the reference's own names and control flow so it can be diffed
//! against `web/graph.ts` line by line. Reference line anchors are quoted in the
//! comments below (`graph.ts:NNN`). Only the *layout* half of the reference is
//! ported; nothing here renders.
//!
//! # Reachability of the `lane: -1` / `on_branch: false` sentinel
//!
//! The G8 golden schema documents `lane: -1` for a row where the reference draws no
//! node (`Vertex.draw()` returns early when `onBranch === null`, `graph.ts:301`;
//! `isNotOnBranch()` is the same test, `graph.ts:238-240`).
//! That state is **unreachable** for the commit list the golden harness feeds in:
//! `loadCommits()` walks every vertex with
//! `while (i < vertices.length) if (getNextParent() !== null || isNotOnBranch()) determinePath(i);`
//! (`graph.ts:433-439`). A vertex with no parents has `getNextParent() === null`, so
//! `isNotOnBranch()` decides: unless the vertex already got a branch (which sets
//! `onBranch`), `determinePath()` runs. Every one of its two paths ends with the
//! start vertex on a branch — the normal path calls `vertex.addToBranch(branch, x)`
//! unconditionally (`graph.ts:733`), and the merge path is guarded by
//! `!vertex.isNotOnBranch()` (`graph.ts:709`), i.e. that vertex is already on one. A
//! vertex with an out-of-range parent has `getNextParent() === nullVertex`
//! (`!== null`) and therefore always runs `determinePath()`. So every vertex ends up
//! with a branch and `on_branch` is always `true`.
//!
//! The reachable "sentinel" case is therefore the **parent link**
//! `parents: [-1]` (`NULL_VERTEX_ID`, `graph.ts:2`): the golden
//! `branch-merge-rebase-truncated` fixture exercises it by dumping a real repository
//! through a smaller commit window (a boundary commit's parent falls outside the
//! window). See `tools/g8-dump/README.md` and `tests/golden_layout.rs`.

#![allow(dead_code)]

pub mod types;

pub use types::{GraphLayout, Lane, RowIdx, RowLayout, SegKind, Segment, VertexRef};

use crate::data::Commit;
use std::collections::HashMap;

/// Number of terminal cells per lane — D8 ruling `§3.1`.
pub const CELLS_PER_LANE: usize = 2;

/// One terminal cell of the graph grid — D8 ruling `§3.2`.
///
/// Kept from the P0 scaffold: b1's renderer builds these from the [`Segment`]s.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cell {
    /// Glyph drawn in this cell (depends on the G2 capability tier: Unicode / ASCII / dumb).
    pub glyph: char,
    /// Lane (grid column) this cell belongs to.
    pub lane: usize,
    /// z-order personality: which lane owns the colour when two lanes cross here.
    pub pers: usize,
}

/// [`nav`] returns this when the requested relation does not exist.
///
/// `RowIdx` is unsigned, so `-1` from the reference (`graph.ts:606` etc.) becomes
/// `RowIdx::MAX`.
pub const NO_ROW: RowIdx = RowIdx::MAX;

/// The frozen 12-entry palette — `vscode-git-graph/src/config.ts:275-277`, as RGB.
pub const PALETTE: [(u8, u8, u8); 12] = [
    (0x00, 0x85, 0xd9),
    (0xd9, 0x00, 0x8f),
    (0x00, 0xd9, 0x0a),
    (0xd9, 0x85, 0x00),
    (0xa3, 0x00, 0xd9),
    (0xff, 0x00, 0x00),
    (0x00, 0xd9, 0xcc),
    (0xe1, 0x38, 0xe8),
    (0x85, 0xd9, 0x00),
    (0xdc, 0x5b, 0x23),
    (0x6f, 0x24, 0xd6),
    (0xff, 0xcc, 0x00),
];

/// Settings that change how commits are laid out.
///
/// Mirrors the reference settings the G8 harness can vary. The reference
/// `onlyFollowFirstParent` flag (`graph.ts:390`) is intentionally **not** a field
/// here — [`layout`] always uses the extension's default (`false`), which is what the
/// golden fixtures were produced with. The edge-case tests drive the other value
/// through the private engine builder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LayoutOpts {
    /// Reserved: reference "expand the first parent" toggle. No effect in W1.
    pub expand_first_parent: bool,
}

/// Navigation direction for [`nav`] — the four reference queries
/// (`graph.ts:600-672`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavDir {
    /// First child of the row (`getFirstChildIndex`).
    Up,
    /// First parent of the row (`getFirstParentIndex`).
    Down,
    /// Alternative child of the row (`getAlternativeChildIndex`).
    Expand,
    /// Alternative parent of the row (`getAlternativeParentIndex`).
    Collapse,
}

/// Lay out `commits` (newest first) into rows, lanes and segments.
///
/// This is the whole reference algorithm: build the parent/child links
/// (`loadCommits`, `graph.ts:392-425`), assign a branch colour column
/// (`determinePath` + `getAvailableColour`, `graph.ts:705-773`) and project the
/// result into the a1↔b1 contract types.
pub fn layout(commits: &[Commit], opts: &LayoutOpts) -> GraphLayout {
    // `expand_first_parent` is reserved for a later wave; read it so the parameter
    // is genuinely used and the option is not silently dropped by refactors.
    let _reserved = opts.expand_first_parent;
    if commits.is_empty() {
        return GraphLayout {
            rows: Vec::new(),
            max_lane: 0,
            palette: PALETTE,
        };
    }
    let engine = engine_for(commits, false);
    to_graph_layout(&engine)
}

/// **Additive, non-contract helper.** Parent rows per commit, in `git` order;
/// `None` is the reference's `NULL_VERTEX_ID` (`parents: [-1]` in the golden JSON).
///
/// The a1↔b1 contract's [`GraphLayout`] does not carry parent links (its
/// [`RowLayout`] has only `vertex` and `segments`), so the G8 parity test uses this
/// helper to assert the golden `rows[].parents` field directly.
pub fn parent_rows(commits: &[Commit]) -> Vec<Vec<Option<RowIdx>>> {
    let engine = engine_for(commits, false);
    engine
        .vertices
        .iter()
        .map(|vertex| {
            vertex
                .parents
                .iter()
                .map(|parent| (*parent != NULL_VERTEX_ID).then_some(*parent as RowIdx))
                .collect()
        })
        .collect()
}

/// The four navigation queries (`graph.ts:600-672`), over a finished [`GraphLayout`].
///
/// Returns [`NO_ROW`] when the requested relation does not exist.
///
/// The reference queries read the live `Vertex` graph; the contract's
/// [`GraphLayout`] only carries vertices and segments, so the parent/child adjacency
/// is recovered by tracing each segment path from a vertex (`col_from == lane`)
/// forward, following same-colour segments, until it reaches a row whose vertex sits
/// on the path lane. A vertex is "on the same branch" as a child when their
/// `colour_idx` match (palette colours are unique among live branches, which is what
/// the reference's `isOnThisBranch` compares).
pub fn nav(g: &GraphLayout, from: RowIdx, dir: NavDir) -> RowIdx {
    let index = from as usize;
    if index >= g.rows.len() {
        return NO_ROW;
    }
    let (parents, children) = derive_adjacency(g);
    let parent_colour = g.rows[index].vertex.map(|vertex| vertex.colour_idx);
    match dir {
        NavDir::Down => parents[index].first().copied().unwrap_or(NO_ROW),
        NavDir::Collapse => match parents[index].as_slice() {
            [_, second, ..] => *second,
            [only] => *only,
            [] => NO_ROW,
        },
        NavDir::Up => first_child(g, parent_colour, &children[index]),
        NavDir::Expand => alternative_child(g, parent_colour, &children[index]),
    }
}

/* ------------------------------------------------------------------------- */
/* Adjacency recovery for nav()                                              */
/* ------------------------------------------------------------------------- */

/// Parent and child rows for every row, `git` order (ascending for children).
fn derive_adjacency(g: &GraphLayout) -> (Vec<Vec<RowIdx>>, Vec<Vec<RowIdx>>) {
    let len = g.rows.len();
    let mut parents: Vec<Vec<RowIdx>> = vec![Vec::new(); len];
    let mut children: Vec<Vec<RowIdx>> = vec![Vec::new(); len];

    for (row, layout_row) in g.rows.iter().enumerate() {
        let Some(vertex) = layout_row.vertex else {
            continue;
        };
        for segment in &layout_row.segments {
            // A parent edge starts at the vertex itself.
            if segment.col_from != vertex.lane {
                continue;
            }
            let mut trace_row = segment.row_to as usize;
            let mut trace_col = segment.col_to;
            let colour = segment.colour_idx;
            let mut steps = 0usize;
            while trace_row < len && steps <= len {
                if let Some(target) = g.rows[trace_row]
                    .vertex
                    .filter(|candidate| candidate.lane == trace_col)
                {
                    // The path reaches a node on its lane: that node is the parent.
                    parents[row].push(target.row);
                    children[target.row as usize].push(row as RowIdx);
                    break;
                }
                match g.rows[trace_row]
                    .segments
                    .iter()
                    .find(|next| next.col_from == trace_col && next.colour_idx == colour)
                {
                    Some(next) => {
                        trace_row = next.row_to as usize;
                        trace_col = next.col_to;
                    }
                    None => break,
                }
                steps += 1;
            }
        }
    }
    (parents, children)
}

/// `getFirstChildIndex` (`graph.ts:626-647`).
///
/// Prefers a child on the parent's own branch; when there is none, the highest child
/// row. `parent_colour` is the parent row's `colour_idx` (the branch proxy).
fn first_child(g: &GraphLayout, parent_colour: Option<u8>, children: &[RowIdx]) -> RowIdx {
    match children {
        [single] => *single,
        [] => NO_ROW,
        _ => {
            if let Some(colour) = parent_colour {
                if let Some(child) = children
                    .iter()
                    .find(|child| vertex_colour(g, **child) == Some(colour))
                {
                    return *child;
                }
            }
            *children.iter().max().expect("non-empty")
        }
    }
}

/// `getAlternativeChildIndex` (`graph.ts:654-676`).
fn alternative_child(g: &GraphLayout, parent_colour: Option<u8>, children: &[RowIdx]) -> RowIdx {
    match children {
        [single] => *single,
        [] => NO_ROW,
        _ => {
            if let Some(colour) = parent_colour {
                if let Some(same) = children
                    .iter()
                    .find(|child| vertex_colour(g, **child) == Some(colour))
                {
                    return children
                        .iter()
                        .copied()
                        .filter(|child| child != same)
                        .max()
                        .unwrap_or(NO_ROW);
                }
            }
            let mut sorted: Vec<RowIdx> = children.to_vec();
            sorted.sort_unstable();
            sorted[sorted.len() - 2]
        }
    }
}

/// `colour_idx` of a row's vertex, if it has one.
fn vertex_colour(g: &GraphLayout, row: RowIdx) -> Option<u8> {
    g.rows
        .get(row as usize)
        .and_then(|r| r.vertex)
        .map(|v| v.colour_idx)
}

/* ------------------------------------------------------------------------- */
/* Reference algorithm port                                                 */
/* ------------------------------------------------------------------------- */

/// `NULL_VERTEX_ID` — `web/graph.ts:2`. Parents outside the loaded range point here.
const NULL_VERTEX_ID: usize = usize::MAX;

/// A graph space point (`{x, y}` in the reference).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Point {
    x: usize,
    y: usize,
}

/// One line of a branch (`Branch.lines` entry).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Line {
    p1: Point,
    p2: Point,
    locked_first: bool,
}

/// A branch (`web/graph.ts` `Branch`): a colour plus the lines that draw it.
#[derive(Debug, Clone)]
struct Branch {
    colour: usize,
    end: usize,
    lines: Vec<Line>,
}

/// A registered crossing point (`Vertex.connections[x]`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct UnavailablePoint {
    connects_to: Option<usize>,
    on_branch: usize,
}

/// A commit node (`web/graph.ts` `Vertex`). Indices into the vertex vec replace the
/// reference's object references; `NULL_VERTEX_ID` stands in for `nullVertex` and
/// `None` for JS `null`.
#[derive(Debug, Clone)]
struct Vertex {
    id: usize,
    is_stash: bool,
    x: usize,
    children: Vec<usize>,
    parents: Vec<usize>,
    next_parent: usize,
    on_branch: Option<usize>,
    is_current: bool,
    next_x: usize,
    connections: Vec<Option<UnavailablePoint>>,
}

impl Vertex {
    fn new(id: usize, is_stash: bool) -> Self {
        Self {
            id,
            is_stash,
            x: 0,
            children: Vec::new(),
            parents: Vec::new(),
            next_parent: 0,
            on_branch: None,
            is_current: false,
            next_x: 0,
            connections: Vec::new(),
        }
    }

    /// `getNextParent()` (`graph.ts:204-207`).
    fn get_next_parent(&self) -> Option<usize> {
        self.parents.get(self.next_parent).copied()
    }

    /// `registerParentProcessed()` (`graph.ts:213-215`).
    fn register_parent_processed(&mut self) {
        self.next_parent += 1;
    }

    /// `addToBranch(branch, x)` (`graph.ts:224-229`) — first assignment wins.
    fn add_to_branch(&mut self, branch: usize, x: usize) {
        if self.on_branch.is_none() {
            self.on_branch = Some(branch);
            self.x = x;
        }
    }

    /// `isMerge()` (`graph.ts:217-219`).
    fn is_merge(&self) -> bool {
        self.parents.len() > 1
    }

    /// `getPoint()` (`graph.ts:246-248`).
    fn get_point(&self) -> Point {
        Point {
            x: self.x,
            y: self.id,
        }
    }

    /// `getNextPoint()` (`graph.ts:250-252`).
    fn get_next_point(&self) -> Point {
        Point {
            x: self.next_x,
            y: self.id,
        }
    }

    /// `getPointConnectingTo(vertex, onBranch)` (`graph.ts:254-263`).
    fn get_point_connecting_to(&self, vertex: usize, on_branch: usize) -> Option<Point> {
        for (x, connection) in self.connections.iter().enumerate() {
            if let Some(connection) = connection {
                if connection.connects_to == Some(vertex) && connection.on_branch == on_branch {
                    return Some(Point { x, y: self.id });
                }
            }
        }
        None
    }

    /// `registerUnavailablePoint(x, connectsToVertex, onBranch)` (`graph.ts:264-269`).
    fn register_unavailable_point(
        &mut self,
        x: usize,
        connects_to: Option<usize>,
        on_branch: usize,
    ) {
        if x == self.next_x {
            self.next_x = x + 1;
            let point = Some(UnavailablePoint {
                connects_to,
                on_branch,
            });
            if x < self.connections.len() {
                self.connections[x] = point;
            } else {
                self.connections.push(point);
            }
        }
    }

    /// `getColour()` (`graph.ts:277-279`).
    fn colour(&self, branches: &[Branch]) -> usize {
        match self.on_branch {
            Some(branch) => branches[branch].colour,
            None => 0,
        }
    }
}

/// The mutable layout state for one `layout()` run.
struct Engine {
    vertices: Vec<Vertex>,
    branches: Vec<Branch>,
    available_colours: Vec<usize>,
}

impl Engine {
    /// `Graph::determinePath(startAt)` (`graph.ts:705-764`).
    fn determine_path(&mut self, start_at: usize) {
        let mut vertex = start_at;
        let mut parent_vertex = self.vertices[start_at].get_next_parent();
        let mut last_point = if self.vertices[start_at].on_branch.is_none() {
            self.vertices[start_at].get_next_point()
        } else {
            self.vertices[start_at].get_point()
        };

        // Merge between two vertices that are already on branches.
        let merge_case = parent_vertex
            .filter(|&parent| parent != NULL_VERTEX_ID)
            .map(|parent| {
                self.vertices[vertex].is_merge()
                    && self.vertices[vertex].on_branch.is_some()
                    && self.vertices[parent].on_branch.is_some()
            })
            .unwrap_or(false);

        if merge_case {
            let parent = parent_vertex.expect("merge case implies a parent");
            let parent_branch = self.vertices[parent]
                .on_branch
                .expect("merge case implies the parent has a branch");
            let mut found_point_to_parent = false;
            let mut i = start_at + 1;
            while i < self.vertices.len() {
                let cur_point =
                    match self.vertices[i].get_point_connecting_to(parent, parent_branch) {
                        Some(point) => {
                            found_point_to_parent = true;
                            point
                        }
                        None => self.vertices[i].get_next_point(),
                    };
                let locked_first = if !found_point_to_parent && i != parent {
                    last_point.x < cur_point.x
                } else {
                    true
                };
                self.branches[parent_branch].lines.push(Line {
                    p1: last_point,
                    p2: cur_point,
                    locked_first,
                });
                self.vertices[i].register_unavailable_point(
                    cur_point.x,
                    Some(parent),
                    parent_branch,
                );
                last_point = cur_point;
                if found_point_to_parent {
                    self.vertices[vertex].register_parent_processed();
                    break;
                }
                i += 1;
            }
        } else {
            // Normal branch.
            let colour = self.get_available_colour(start_at);
            let branch_index = self.branches.len();
            self.branches.push(Branch {
                colour,
                end: 0,
                lines: Vec::new(),
            });
            self.vertices[vertex].add_to_branch(branch_index, last_point.x);
            self.vertices[vertex].register_unavailable_point(
                last_point.x,
                Some(vertex),
                branch_index,
            );

            let mut i = start_at + 1;
            while i < self.vertices.len() {
                let cur_point = if parent_vertex == Some(i) && self.vertices[i].on_branch.is_some()
                {
                    self.vertices[i].get_point()
                } else {
                    self.vertices[i].get_next_point()
                };
                let locked_first = last_point.x < cur_point.x;
                self.branches[branch_index].lines.push(Line {
                    p1: last_point,
                    p2: cur_point,
                    locked_first,
                });
                self.vertices[i].register_unavailable_point(
                    cur_point.x,
                    parent_vertex,
                    branch_index,
                );
                last_point = cur_point;

                if parent_vertex == Some(i) {
                    // The parent has been reached: continue building the branch from it.
                    self.vertices[vertex].register_parent_processed();
                    let parent_vertex_on_branch = self.vertices[i].on_branch.is_some();
                    self.vertices[i].add_to_branch(branch_index, cur_point.x);
                    vertex = i;
                    parent_vertex = self.vertices[vertex].get_next_parent();
                    if parent_vertex.is_none() || parent_vertex_on_branch {
                        break;
                    }
                }
                i += 1;
            }
            if i == self.vertices.len() && parent_vertex == Some(NULL_VERTEX_ID) {
                // Vertex is the last in the graph, so no more branch can be formed.
                self.vertices[vertex].register_parent_processed();
            }
            self.branches[branch_index].end = i;
            self.available_colours[colour] = i;
        }
    }

    /// `Graph::getAvailableColour(startAt)` (`graph.ts:766-773`).
    fn get_available_colour(&mut self, start_at: usize) -> usize {
        for (colour, end) in self.available_colours.iter().enumerate() {
            if start_at > *end {
                return colour;
            }
        }
        self.available_colours.push(0);
        self.available_colours.len() - 1
    }
}

/// Build and run the reference engine for `commits`.
///
/// `only_follow_first_parent` is the reference flag (`graph.ts:390`): when set, merge
/// parents after the first are dropped instead of being linked to `NULL_VERTEX_ID`.
fn engine_for(commits: &[Commit], only_follow_first_parent: bool) -> Engine {
    let mut lookup: HashMap<&str, usize> = HashMap::with_capacity(commits.len());
    for (index, commit) in commits.iter().enumerate() {
        lookup.insert(commit.hash.as_str(), index);
    }

    let mut vertices: Vec<Vertex> = commits
        .iter()
        .enumerate()
        .map(|(index, commit)| Vertex::new(index, is_stash_commit(&commit.refs)))
        .collect();

    // `loadCommits()` parent/child wiring (`graph.ts:397-412`).
    for (index, commit) in commits.iter().enumerate() {
        for (parent_position, parent_hash) in commit.parents.iter().enumerate() {
            match lookup.get(parent_hash.as_str()) {
                Some(&parent_index) => {
                    vertices[index].parents.push(parent_index);
                    vertices[parent_index].children.push(index);
                }
                None => {
                    if !only_follow_first_parent || parent_position == 0 {
                        vertices[index].parents.push(NULL_VERTEX_ID);
                    }
                }
            }
        }
    }

    // `setCurrent()` (`graph.ts:419-421`): the checked-out commit is the HEAD ref.
    for (index, commit) in commits.iter().enumerate() {
        if commit.refs.iter().any(|reference| is_head_ref(reference)) {
            vertices[index].is_current = true;
        }
    }

    let mut engine = Engine {
        vertices,
        branches: Vec::new(),
        available_colours: Vec::new(),
    };
    let mut i = 0;
    while i < engine.vertices.len() {
        if engine.vertices[i].get_next_parent().is_some() || engine.vertices[i].on_branch.is_none()
        {
            engine.determine_path(i);
        } else {
            i += 1;
        }
    }
    engine
}

/// Project the engine state into the a1↔b1 contract types.
fn to_graph_layout(engine: &Engine) -> GraphLayout {
    let mut max_lane = 0usize;
    for vertex in &engine.vertices {
        max_lane = max_lane.max(vertex.x);
    }

    // Bucket every branch line by the row it starts on in a single pass. The old
    // shape re-scanned *all* branch lines for *every* row (O(rows × lines) — quadratic
    // on a linear history). Each line still lands in exactly one row's segment list and
    // branch/line order is preserved, so the output is byte-for-byte unchanged.
    let mut lines_by_row: Vec<Vec<(Line, usize)>> = vec![Vec::new(); engine.vertices.len()];
    for branch in &engine.branches {
        for line in &branch.lines {
            // Out-of-range rows were skipped by the old `continue`; keep that guard.
            if let Some(bucket) = lines_by_row.get_mut(line.p1.y) {
                bucket.push((*line, branch.colour));
            }
        }
    }

    let mut rows = Vec::with_capacity(engine.vertices.len());
    for (row, vertex) in engine.vertices.iter().enumerate() {
        let vertex_ref = Some(VertexRef {
            row: row as RowIdx,
            lane: vertex.x as Lane,
            colour_idx: vertex.colour(&engine.branches) as u8,
            is_head: vertex.is_current,
            is_stash: vertex.is_stash,
            on_branch: vertex.on_branch.is_some(),
        });

        let mut segments = Vec::new();
        for (line, colour) in &lines_by_row[row] {
            let pers = if line.locked_first {
                line.p1.x
            } else {
                line.p2.x
            };
            max_lane = max_lane.max(line.p1.x).max(line.p2.x).max(pers);
            segments.push(Segment {
                row_from: line.p1.y as RowIdx,
                row_to: line.p2.y as RowIdx,
                col_from: line.p1.x as Lane,
                col_to: line.p2.x as Lane,
                kind: segment_kind(line.p1, line.p2),
                colour_idx: *colour as u8,
                pers: pers as u8,
            });
        }

        rows.push(RowLayout {
            row: row as RowIdx,
            vertex: vertex_ref,
            segments,
        });
    }

    GraphLayout {
        rows,
        max_lane: max_lane as Lane,
        palette: PALETTE,
    }
}

/// Geometric [`SegKind`] for one segment (see the [`SegKind`] doc comment).
fn segment_kind(p1: Point, p2: Point) -> SegKind {
    if p1.x == p2.x {
        SegKind::Vertical
    } else if p1.x.abs_diff(p2.x) == 1 {
        SegKind::RoundedCorner(u8::from(p2.x < p1.x))
    } else {
        SegKind::Horizontal
    }
}

/// Is this `%D` decoration the HEAD ref?
fn is_head_ref(reference: &str) -> bool {
    reference == "HEAD" || reference.starts_with("HEAD ->") || reference.starts_with("HEAD->")
}

/// Is this `%D` decoration a stash ref?
fn is_stash_commit(refs: &[String]) -> bool {
    refs.iter().any(|reference| {
        reference == "refs/stash"
            || reference.starts_with("refs/stash@")
            || reference == "stash"
            || reference.starts_with("stash@")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commit(hash: &str, parents: &[&str], refs: &[&str]) -> Commit {
        Commit {
            hash: hash.to_string(),
            abbrev: hash.chars().take(7).collect(),
            parents: parents.iter().map(|parent| (*parent).to_string()).collect(),
            author: "Test".to_string(),
            date: 0,
            subject: format!("commit {hash}"),
            refs: refs
                .iter()
                .map(|reference| (*reference).to_string())
                .collect(),
        }
    }

    #[test]
    fn empty_input_yields_empty_layout() {
        let graph = layout(&[], &LayoutOpts::default());
        assert!(graph.rows.is_empty());
        assert_eq!(graph.max_lane, 0);
        assert_eq!(graph.palette, PALETTE);
    }

    #[test]
    fn linear_chain_is_one_lane_and_one_column() {
        let commits = [
            commit("a", &["b"], &["HEAD -> main"]),
            commit("b", &["c"], &[]),
            commit("c", &[], &[]),
        ];
        let graph = layout(&commits, &LayoutOpts::default());
        assert_eq!(graph.rows.len(), 3);
        for row in &graph.rows {
            let vertex = row.vertex.expect("every row has a vertex");
            assert_eq!(vertex.lane, 0);
            assert_eq!(vertex.colour_idx, 0);
            assert!(vertex.on_branch);
        }
        assert!(graph.rows[0].vertex.expect("vertex").is_head);
        assert_eq!(graph.rows[0].segments.len(), 1);
        assert_eq!(graph.rows[0].segments[0].row_from, 0);
        assert_eq!(graph.rows[0].segments[0].row_to, 1);
        assert_eq!(graph.rows[0].segments[0].kind, SegKind::Vertical);
        assert!(graph.rows[2].segments.is_empty());
        assert_eq!(graph.max_lane, 0);
    }

    #[test]
    fn parent_outside_the_loaded_range_becomes_null_vertex() {
        // `parent_rows` (what `layout` uses) maps the reference `NULL_VERTEX_ID` to
        // `None`, i.e. the golden `parents: [-1]`.
        let commits = [
            commit("a", &["missing"], &[]),
            commit("b", &["c"], &[]),
            commit("c", &[], &[]),
        ];
        let rows = parent_rows(&commits);
        assert_eq!(rows[0], vec![None]);
        assert_eq!(rows[1], vec![Some(2)]);
        assert_eq!(rows[2], Vec::<Option<RowIdx>>::new());

        // The laid-out last row still has a node (on_branch is never false).
        let graph = layout(&commits, &LayoutOpts::default());
        assert!(graph.rows[2].vertex.expect("vertex").on_branch);
    }

    #[test]
    fn only_follow_first_parent_drops_later_parents() {
        let commits = [commit("m", &["p", "outside"], &[]), commit("p", &[], &[])];
        let both = engine_for(&commits, false);
        assert_eq!(both.vertices[0].parents, vec![1, NULL_VERTEX_ID]);
        let single = engine_for(&commits, true);
        assert_eq!(single.vertices[0].parents, vec![1]);
    }

    #[test]
    fn stash_vertex_is_flagged_from_refs() {
        let commits = [commit("s", &[], &["refs/stash"])];
        let graph = layout(&commits, &LayoutOpts::default());
        assert!(graph.rows[0].vertex.expect("vertex").is_stash);
    }

    #[test]
    fn nav_resolves_parents_children_and_alternatives() {
        // c -> m, m -> p1 + p2. Rows: 0 = c, 1 = m, 2 = p1, 3 = p2.
        let commits = [
            commit("c", &["m"], &["HEAD -> main"]),
            commit("m", &["p1", "p2"], &[]),
            commit("p1", &[], &[]),
            commit("p2", &[], &[]),
        ];
        let graph = layout(&commits, &LayoutOpts::default());

        assert_eq!(nav(&graph, 0, NavDir::Down), 1, "first parent of c");
        assert_eq!(nav(&graph, 0, NavDir::Up), NO_ROW, "c has no child");
        assert_eq!(nav(&graph, 1, NavDir::Down), 2, "first parent of m");
        assert_eq!(
            nav(&graph, 1, NavDir::Collapse),
            3,
            "alternative parent of m"
        );
        assert_eq!(nav(&graph, 1, NavDir::Up), 0, "child of m is c");
        assert_eq!(nav(&graph, 1, NavDir::Expand), 0, "only child of m is c");
        assert_eq!(nav(&graph, 2, NavDir::Down), NO_ROW, "p1 is a root");
    }

    #[test]
    fn nav_out_of_range_is_no_row() {
        let graph = layout(&[commit("a", &[], &[])], &LayoutOpts::default());
        assert_eq!(nav(&graph, 99, NavDir::Down), NO_ROW);
    }
}
