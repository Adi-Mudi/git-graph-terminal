//! Interaction / event layer for the graph view (M2-W4).
//!
//! `AppState` sits between `main.rs`'s terminal loop and [`crate::render`]: it
//! owns the loaded rows, the laid-out graph, the selection / scroll / expand
//! state and a snapshot of the status-bar metadata. [`reduce`] is the only
//! mutator — it is where a keypress becomes a state change.
//!
//! Reference anchors (read-only `../vscode-git-graph`):
//!
//! * `web/main.ts:2025-2111` — the global key handler.
//! * `web/main.ts:1887-1930` — `scrollToStash(next)`; **no Shift = NEXT**
//!   (`scrollToStash(!e.shiftKey)`, `main.ts:2078-2080`) and the stash index
//!   **wraps** (`1899-1904`). This is binding ruling **R1.2** from
//!   `.IDE_Plans/g7-checklist.md`; pi's `INT-009` is inverted and is *not* ported.
//! * `web/main.ts:2040-2068` — plain Up/Down is **table order** and clamps
//!   (inventory C03); `Ctrl+Up`/`Ctrl+Down` follow the branch (`graph.ts:598-644`,
//!   inventory C04) and are a no-op when the query returns nothing (no wrap).
//! * `web/graph.ts:84-105` + `src/config.ts:279` — an expanded row stretches the
//!   graph by `expandY` (inventory C29). W4 reserves that gap only; W5 fills it
//!   with the commit-details view.
//!
//! The row geometry helpers below are additive (the W1 a1↔b1 contract is
//! unchanged): a commit row is one terminal row tall, or
//! `1 + EXPANDED_EXTRA_ROWS` when it is in [`AppState::expanded`]. `scroll` is a
//! **display-row** offset so it keeps working when expanded rows make the row
//! index and the screen row drift apart.

use std::collections::HashSet;
use std::path::Path;

use crate::data::{self, Commit};
use crate::layout::types::{GraphLayout, RowIdx};
use crate::layout::{self, LayoutOpts, NavDir, NO_ROW};

/// Extra terminal rows an expanded commit row occupies in W4.
///
/// The reference stretches the graph below an expanded row by `expandY`
/// (`src/config.ts:279`, default 250 px). W4 renders the gap only, as one
/// constant, so W5 can replace it with the measured commit-details pane without
/// touching the navigation code.
pub const EXPANDED_EXTRA_ROWS: u16 = 4;

/// Everything the status bar needs, snapshotted after each [`reduce`].
///
/// `width` / `height` are the **terminal** size; the event loop refreshes them
/// every frame. `height` is the main-area (body) height in rows — the viewport
/// used by `PageNext`/`PagePrev` and by the follow-the-selection scroll.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusBarState {
    /// Repository path as shown in the UI (never an absolute server path — Rule 10).
    pub repo_label: String,
    /// Current branch, or `(detached HEAD)`.
    pub branch: String,
    /// HEAD short hash, or `(no commits)`.
    pub head: String,
    /// Commit count.
    pub commits: usize,
    /// Local branch count.
    pub branches: usize,
    /// Lane count (`max_lane + 1`).
    pub lanes: usize,
    /// Graph width in terminal columns (`2 * (max_lane + 1)`).
    pub graph_width: u16,
    /// Terminal width in columns (set each frame).
    pub width: u16,
    /// Main-area viewport height in rows (set each frame).
    pub height: u16,
    /// Selected row (selection info for the status bar).
    pub selected: RowIdx,
    /// Startup/refresh error (the TUI stays alive).
    pub error: Option<String>,
}

/// The interactive state of one graph view — the w4 contract.
#[derive(Debug, Clone)]
pub struct AppState {
    /// Commits, newest first (`data::commits`).
    pub rows: Vec<Commit>,
    /// Laid-out graph for [`AppState::rows`] (`layout::layout`).
    pub layout: GraphLayout,
    /// Single selected row, kept visible by [`AppState::ensure_visible`].
    pub selected: RowIdx,
    /// Rows whose details are expanded (W4: a reserved gap, W5: the CDV).
    pub expanded: HashSet<RowIdx>,
    /// Viewport offset in **display rows** (see the module docs).
    pub scroll: u16,
    /// Status-bar / viewport snapshot.
    pub status: StatusBarState,
}

/// Every keyboard-driven state change.
///
/// Context-menu / dialog / find actions arrive in W5/W6; the enum stays
/// extensible (the contract comment says so explicitly).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Move one row down in table order (clamped) — `j` / Down.
    SelectNext,
    /// Move one row up in table order (clamped) — `k` / Up.
    SelectPrev,
    /// Move down by the viewport height minus one — PageDown.
    PageNext,
    /// Move up by the viewport height minus one — PageUp.
    PagePrev,
    /// Jump to the newest commit (row 0) — Home.
    Top,
    /// Jump to the oldest loaded commit — End.
    Bottom,
    /// Toggle the selected row's expanded state — Enter.
    ToggleExpand,
    /// Follow the first parent on the same branch — Right / Ctrl+Down.
    NextBranch,
    /// Follow the first child on the same branch — Left / Ctrl+Up.
    PrevBranch,
    /// Scroll to the next stash (`s` / Ctrl+s; **no Shift**, R1.2).
    StashNext,
    /// Scroll to the previous stash (`S` / Ctrl+Shift+s; Shift, R1.2).
    StashPrev,
    /// Reload commits + layout + repo info — `r`.
    Refresh,
    /// Leave the TUI — `q` / Esc.
    Quit,
}

/// What [`reduce`] wants the caller to do next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Redraw the frame.
    Redraw,
    /// The caller should tear down and exit.
    Quit,
}

impl AppState {
    /// Load (or reload) the state for `repo`; never panics, errors are captured.
    pub fn load(repo: &Path) -> Self {
        let mut error: Option<String> = None;
        let mut rows: Vec<Commit> = Vec::new();
        let mut branches = 0usize;
        let mut branch = "(detached HEAD)".to_string();
        let mut head = "(no commits)".to_string();

        if !data::is_repo(repo) {
            error = Some(format!("not a git repository: {}", repo.display()));
        } else {
            // Error priority mirrors the P0 `App::load`: branches, then commits,
            // then HEAD only when nothing else has failed.
            match data::branches(repo) {
                Ok(list) => branches = list.len(),
                Err(e) => error = Some(e.to_string()),
            }
            match data::commits(repo) {
                Ok(list) => rows = list,
                Err(e) => error = Some(e.to_string()),
            }
            match data::head(repo) {
                Ok(state) => {
                    branch = state
                        .branch
                        .unwrap_or_else(|| "(detached HEAD)".to_string());
                    head = state.hash.unwrap_or_else(|| "(no commits)".to_string());
                }
                Err(e) if error.is_none() => error = Some(e.to_string()),
                Err(_) => {}
            }
        }

        let layout = layout::layout(&rows, &LayoutOpts::default());
        Self::from_parts(
            rows,
            layout,
            branch,
            head,
            branches,
            repo_label(repo),
            error,
        )
    }

    /// The `--mock` state: the hand-built fixture from [`crate::render::mock`],
    /// so the interactive loop can be smoke-tested without a repository.
    pub fn demo() -> Self {
        Self::from_parts(
            crate::render::mock::demo_commits(),
            crate::render::mock::demo_layout(),
            "main".to_string(),
            "a1b2c3d".to_string(),
            2,
            "(mock)".to_string(),
            None,
        )
    }

    /// Assemble a state from already-loaded pieces.
    fn from_parts(
        rows: Vec<Commit>,
        layout: GraphLayout,
        branch: String,
        head: String,
        branches: usize,
        repo_label: String,
        error: Option<String>,
    ) -> Self {
        let status = StatusBarState {
            repo_label,
            branch,
            head,
            commits: rows.len(),
            branches,
            lanes: layout.max_lane as usize + 1,
            graph_width: graph_width(layout.max_lane),
            width: 0,
            height: 0,
            selected: 0,
            error,
        };
        Self {
            rows,
            layout,
            selected: 0,
            expanded: HashSet::new(),
            scroll: 0,
            status,
        }
    }

    /// Terminal height of commit row `row`: `1`, or `1 + EXPANDED_EXTRA_ROWS`.
    pub fn row_height(&self, row: RowIdx) -> u16 {
        if self.expanded.contains(&row) {
            1 + EXPANDED_EXTRA_ROWS
        } else {
            1
        }
    }

    /// Display-row offset (top) of commit row `row`.
    pub fn row_top(&self, row: RowIdx) -> u16 {
        let last = row.min(self.rows.len() as RowIdx);
        let mut top = 0u16;
        for r in 0..last {
            top = top.saturating_add(self.row_height(r));
        }
        top
    }

    /// Total display rows the commit list occupies.
    pub fn display_rows(&self) -> u16 {
        let mut total = 0u16;
        for r in 0..self.rows.len() as RowIdx {
            total = total.saturating_add(self.row_height(r));
        }
        total
    }

    /// Refresh the derived status fields after a mutation.
    fn sync_status(&mut self) {
        self.status.commits = self.rows.len();
        self.status.lanes = self.layout.max_lane as usize + 1;
        self.status.graph_width = graph_width(self.layout.max_lane);
        self.status.selected = self.selected;
    }

    /// Keep the selected row inside `[scroll, scroll + height)`.
    ///
    /// Anchors to the bottom edge like the reference scroll container
    /// (`main.ts:1917-1922`): scroll down only when the row falls off the
    /// bottom, and jump the row to the top when it moves above the viewport.
    fn ensure_visible(&mut self) {
        if self.rows.is_empty() {
            self.scroll = 0;
            return;
        }
        let view = self.status.height.max(1);
        let top = self.row_top(self.selected);
        let bottom = top.saturating_add(self.row_height(self.selected));
        if top < self.scroll {
            self.scroll = top;
        } else if bottom > self.scroll.saturating_add(view) {
            self.scroll = bottom.saturating_sub(view);
        }
        let max_scroll = self.display_rows().saturating_sub(view);
        self.scroll = self.scroll.min(max_scroll);
    }
}

/// Apply `action` to `state`; `repo` is only read by [`Action::Refresh`].
///
/// "Pure-ish": every branch is a deterministic state transition, except Refresh,
/// which re-reads the repository through [`AppState::load`].
pub fn reduce(state: &mut AppState, action: Action, repo: &Path) -> Outcome {
    match action {
        Action::Quit => return Outcome::Quit,
        Action::SelectNext => select_table(state, 1),
        Action::SelectPrev => select_table(state, -1),
        Action::PageNext => select_page(state, 1),
        Action::PagePrev => select_page(state, -1),
        Action::Top => state.selected = 0,
        Action::Bottom => state.selected = state.rows.len().saturating_sub(1) as RowIdx,
        Action::ToggleExpand => toggle_expand(state),
        // Right / Ctrl+Down follows the first parent (older, `graph.ts:598-603`).
        Action::NextBranch => select_branch(state, NavDir::Down),
        // Left / Ctrl+Up follows the first child (newer, `graph.ts:626-644`).
        Action::PrevBranch => select_branch(state, NavDir::Up),
        // R1.2: no Shift = NEXT stash; Shift = PREVIOUS. Both wrap.
        Action::StashNext => select_stash(state, true),
        Action::StashPrev => select_stash(state, false),
        Action::Refresh => reload(state, repo),
    }
    state.sync_status();
    state.ensure_visible();
    Outcome::Redraw
}

/// Table-order movement (inventory C03): clamps at both ends, never wraps.
fn select_table(state: &mut AppState, delta: i32) {
    if state.rows.is_empty() {
        return;
    }
    let last = (state.rows.len() - 1) as i32;
    let next = (state.selected as i32 + delta).clamp(0, last);
    state.selected = next as RowIdx;
}

/// Page movement: `PageDown = viewport height - 1` (clamped).
fn select_page(state: &mut AppState, dir: i32) {
    if state.rows.is_empty() {
        return;
    }
    let page = state.status.height.max(1).saturating_sub(1).max(1) as i32;
    let last = (state.rows.len() - 1) as i32;
    let next = (state.selected as i32 + dir * page).clamp(0, last);
    state.selected = next as RowIdx;
}

fn toggle_expand(state: &mut AppState) {
    if state.rows.is_empty() {
        return;
    }
    let row = state.selected;
    if !state.expanded.remove(&row) {
        state.expanded.insert(row);
    }
}

/// Branch-aware movement via a1's `layout::nav()`. No wrap: an absent relation
/// (`NO_ROW`) leaves the selection untouched (`main.ts:2056-2058`).
fn select_branch(state: &mut AppState, dir: NavDir) {
    if state.rows.is_empty() {
        return;
    }
    let target = layout::nav(&state.layout, state.selected, dir);
    if target != NO_ROW {
        state.selected = target;
    }
}

/// Stash movement, faithful to `scrollToStash` (`main.ts:1887-1908`).
///
/// When the selection is on a stash, step to the adjacent stash and **wrap**.
/// When it is not (the reference's expired 5 s `lastScrollToStash`), `next`
/// jumps to the first stash and `prev` to the last. No stash rows → no-op.
///
/// NOTE: `data.rs` is frozen and does not load `refs/stash`, so a real
/// repository yields no stash rows yet; the direction rule is still proven at
/// this layer with synthetic rows (W4 must-fix, R1.2).
fn select_stash(state: &mut AppState, next: bool) {
    let stashes: Vec<RowIdx> = (0..state.rows.len() as RowIdx)
        .filter(|row| is_stash_row(state, *row))
        .collect();
    if stashes.is_empty() {
        return;
    }
    let current = stashes.iter().position(|row| *row == state.selected);
    let index = match (current, next) {
        (Some(i), true) => (i + 1) % stashes.len(),
        (Some(i), false) => (i + stashes.len() - 1) % stashes.len(),
        (None, true) => 0,
        (None, false) => stashes.len() - 1,
    };
    state.selected = stashes[index];
}

/// Re-read commits, layout and repo metadata; keep a valid selection and prune
/// expanded rows that no longer exist.
fn reload(state: &mut AppState, repo: &Path) {
    let fresh = AppState::load(repo);
    state.rows = fresh.rows;
    state.layout = fresh.layout;
    state.status.branch = fresh.status.branch;
    state.status.head = fresh.status.head;
    state.status.branches = fresh.status.branches;
    state.status.repo_label = fresh.status.repo_label;
    state.status.error = fresh.status.error;

    let len = state.rows.len() as RowIdx;
    state.expanded.retain(|row| *row < len);
    if state.rows.is_empty() {
        state.selected = 0;
        state.expanded.clear();
    } else if state.selected >= len {
        state.selected = 0;
    }
}

/// Is the commit on `row` a stash entry?
///
/// Uses the laid-out vertex flag first (it is set from the `%D` decorations) and
/// falls back to the raw refs, so synthetic rows without a layout still work.
fn is_stash_row(state: &AppState, row: RowIdx) -> bool {
    if let Some(vertex) = state.layout.rows.get(row as usize).and_then(|r| r.vertex) {
        if vertex.is_stash {
            return true;
        }
    }
    state
        .rows
        .get(row as usize)
        .map(|commit| commit.refs.iter().any(|r| is_stash_ref(r)))
        .unwrap_or(false)
}

/// Is this `%D` decoration a stash ref? (Same predicate as `layout::is_stash_commit`.)
fn is_stash_ref(reference: &str) -> bool {
    reference == "refs/stash"
        || reference.starts_with("refs/stash@")
        || reference == "stash"
        || reference.starts_with("stash@")
}

/// Graph width in terminal columns: `2 cells per lane * (max_lane + 1)` (D8 §3.1).
fn graph_width(max_lane: u8) -> u16 {
    (layout::CELLS_PER_LANE * (max_lane as usize + 1)) as u16
}

/// Repository path as shown in the UI (never an absolute server path — Rule 10).
fn repo_label(repo: &Path) -> String {
    let repo = repo.canonicalize().unwrap_or_else(|_| repo.to_path_buf());
    let label = std::env::current_dir()
        .ok()
        .and_then(|cwd| repo.strip_prefix(&cwd).ok().map(Path::to_path_buf))
        .unwrap_or_else(|| repo.clone());
    let text = label.to_string_lossy().to_string();
    if text.is_empty() {
        ".".to_string()
    } else {
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn commit(hash: &str, parents: &[&str], refs: &[&str]) -> Commit {
        Commit {
            hash: hash.to_string(),
            abbrev: hash.chars().take(7).collect(),
            parents: parents.iter().map(|p| (*p).to_string()).collect(),
            author: "Test".to_string(),
            date: 0,
            subject: format!("commit {hash}"),
            refs: refs.iter().map(|r| (*r).to_string()).collect(),
        }
    }

    /// A linear history in `git log` order: row `i` is `ci` and its parent is
    /// row `i + 1` (the newest row, `c0`, carries the HEAD ref).
    fn linear(count: usize) -> Vec<Commit> {
        (0..count)
            .map(|i| {
                let hash = format!("c{i}");
                let parents: Vec<String> = if i + 1 < count {
                    vec![format!("c{}", i + 1)]
                } else {
                    Vec::new()
                };
                let refs = if i == 0 {
                    &["HEAD -> main"][..]
                } else {
                    &[][..]
                };
                commit(
                    &hash,
                    &parents.iter().map(String::as_str).collect::<Vec<_>>(),
                    refs,
                )
            })
            .collect()
    }

    fn state_of(commits: Vec<Commit>) -> AppState {
        let layout = layout::layout(&commits, &LayoutOpts::default());
        AppState::from_parts(
            commits,
            layout,
            "main".to_string(),
            "abcdef0".to_string(),
            1,
            "(test)".to_string(),
            None,
        )
    }

    #[test]
    fn table_selection_clamps_at_both_ends() {
        let mut state = state_of(linear(3));
        assert_eq!(state.selected, 0);

        for _ in 0..5 {
            reduce(&mut state, Action::SelectPrev, Path::new("."));
        }
        assert_eq!(state.selected, 0, "SelectPrev clamps at the first row");

        for _ in 0..5 {
            reduce(&mut state, Action::SelectNext, Path::new("."));
        }
        assert_eq!(state.selected, 2, "SelectNext clamps at the last row");
    }

    #[test]
    fn top_and_bottom_jump_to_the_ends() {
        let mut state = state_of(linear(10));
        reduce(&mut state, Action::Bottom, Path::new("."));
        assert_eq!(state.selected, 9);
        reduce(&mut state, Action::Top, Path::new("."));
        assert_eq!(state.selected, 0);
    }

    #[test]
    fn page_moves_by_viewport_height_minus_one() {
        let mut state = state_of(linear(50));
        state.status.height = 10; // PageDown = 10 - 1 = 9 rows
        reduce(&mut state, Action::PageNext, Path::new("."));
        assert_eq!(state.selected, 9);
        reduce(&mut state, Action::PageNext, Path::new("."));
        assert_eq!(state.selected, 18);
        reduce(&mut state, Action::PagePrev, Path::new("."));
        assert_eq!(state.selected, 9);
    }

    #[test]
    fn enter_toggles_the_selected_row_only() {
        let mut state = state_of(linear(4));
        state.selected = 2;
        reduce(&mut state, Action::ToggleExpand, Path::new("."));
        assert!(state.expanded.contains(&2));
        reduce(&mut state, Action::ToggleExpand, Path::new("."));
        assert!(!state.expanded.contains(&2));
        assert!(state.expanded.is_empty());
    }

    #[test]
    fn branch_nav_follows_first_parent_and_child_then_no_ops() {
        // c -> m, m -> p1 + p2. Rows: 0 = c, 1 = m, 2 = p1, 3 = p2.
        let mut state = state_of(vec![
            commit("c", &["m"], &["HEAD -> main"]),
            commit("m", &["p1", "p2"], &[]),
            commit("p1", &[], &[]),
            commit("p2", &[], &[]),
        ]);
        // Right / Ctrl+Down = first parent.
        reduce(&mut state, Action::NextBranch, Path::new("."));
        assert_eq!(state.selected, 1, "first parent of c");
        reduce(&mut state, Action::NextBranch, Path::new("."));
        assert_eq!(state.selected, 2, "first parent of m");
        // Left / Ctrl+Up = first child (back toward newer).
        reduce(&mut state, Action::PrevBranch, Path::new("."));
        assert_eq!(state.selected, 1, "first child of p1 is m");
        reduce(&mut state, Action::PrevBranch, Path::new("."));
        assert_eq!(state.selected, 0, "first child of m is c");
        // No relation: unchanged (no wrap, per C04).
        reduce(&mut state, Action::PrevBranch, Path::new("."));
        assert_eq!(state.selected, 0, "c has no child: no-op");
        reduce(&mut state, Action::Bottom, Path::new("."));
        reduce(&mut state, Action::NextBranch, Path::new("."));
        assert_eq!(state.selected, 3, "p2 is a root: no-op");
    }

    #[test]
    fn stash_direction_and_wrap_match_r1_2() {
        // Rows 1 and 3 are stashes. R1.2: NO SHIFT = NEXT, SHIFT = PREVIOUS.
        let mut state = state_of(vec![
            commit("c0", &["c1"], &[]),
            commit("c1", &["c2"], &["refs/stash"]),
            commit("c2", &["c3"], &[]),
            commit("c3", &[], &["refs/stash@{{0}}"]),
        ]);
        state.selected = 0;

        // Not on a stash: next -> first, prev -> last (expired-memory branch).
        reduce(&mut state, Action::StashPrev, Path::new("."));
        assert_eq!(
            state.selected, 3,
            "Shift = previous, wraps to the last stash"
        );
        reduce(&mut state, Action::StashNext, Path::new("."));
        assert_eq!(
            state.selected, 1,
            "no Shift = next, wraps to the first stash"
        );

        // On a stash: step forward / backward with wraparound.
        reduce(&mut state, Action::StashNext, Path::new("."));
        assert_eq!(state.selected, 3);
        reduce(&mut state, Action::StashNext, Path::new("."));
        assert_eq!(state.selected, 1, "next wraps past the last stash");
        reduce(&mut state, Action::StashPrev, Path::new("."));
        assert_eq!(state.selected, 3, "prev wraps before the first stash");
    }

    #[test]
    fn stash_navigation_is_a_noop_without_stash_rows() {
        let mut state = state_of(linear(3));
        state.selected = 1;
        reduce(&mut state, Action::StashNext, Path::new("."));
        reduce(&mut state, Action::StashPrev, Path::new("."));
        assert_eq!(state.selected, 1);
    }

    #[test]
    fn viewport_follows_the_selection() {
        let mut state = state_of(linear(30));
        state.status.height = 10; // 10 visible rows, rows 0..9
        assert_eq!(state.scroll, 0);

        state.selected = 9;
        reduce(&mut state, Action::SelectNext, Path::new("."));
        assert_eq!(state.selected, 10);
        assert_eq!(state.scroll, 1, "row 10 scrolls the window by one");

        reduce(&mut state, Action::Bottom, Path::new("."));
        assert_eq!(
            state.scroll,
            29 + 1 - 10,
            "anchors the last row to the bottom"
        );

        reduce(&mut state, Action::Top, Path::new("."));
        assert_eq!(state.scroll, 0);
    }

    #[test]
    fn expanded_rows_reserve_the_reference_gap() {
        let mut state = state_of(linear(5));
        assert_eq!(state.row_height(0), 1);
        assert_eq!(state.display_rows(), 5);

        state.expanded.insert(1);
        assert_eq!(state.row_height(1), 1 + EXPANDED_EXTRA_ROWS);
        assert_eq!(state.row_top(0), 0);
        assert_eq!(state.row_top(1), 1);
        assert_eq!(state.row_top(2), 1 + (1 + EXPANDED_EXTRA_ROWS));
        assert_eq!(state.display_rows(), 5 + EXPANDED_EXTRA_ROWS);
    }

    #[test]
    fn empty_state_never_panics() {
        let mut state = state_of(Vec::new());
        for action in [
            Action::SelectNext,
            Action::SelectPrev,
            Action::PageNext,
            Action::PagePrev,
            Action::Top,
            Action::Bottom,
            Action::ToggleExpand,
            Action::NextBranch,
            Action::PrevBranch,
            Action::StashNext,
            Action::StashPrev,
        ] {
            assert_eq!(reduce(&mut state, action, Path::new(".")), Outcome::Redraw);
        }
        assert_eq!(state.selected, 0);
        assert_eq!(state.scroll, 0);
    }

    #[test]
    fn quit_returns_quit_without_touching_state() {
        let mut state = state_of(linear(2));
        assert_eq!(
            reduce(&mut state, Action::Quit, Path::new(".")),
            Outcome::Quit
        );
    }

    #[test]
    fn refresh_reloads_rows_and_prunes_expansion() {
        let dir = temp_repo("refresh");
        let mut state = AppState::load(&dir);
        assert_eq!(state.rows.len(), 1);
        state.selected = 0;
        reduce(&mut state, Action::ToggleExpand, Path::new("."));
        assert!(state.expanded.contains(&0));

        // Add a commit, then refresh.
        fs::write(dir.join("f.txt"), "2\n").expect("write file");
        data::run_git(&dir, &["commit", "-q", "-am", "c2"]).expect("commit");
        reduce(&mut state, Action::Refresh, &dir);
        assert_eq!(state.rows.len(), 2, "refresh picks up the new commit");
        assert_eq!(state.status.commits, 2);
        // Expanded row 0 still exists, so it survives; the clamp resets an
        // out-of-range selection to the top.
        state.selected = 99;
        reduce(&mut state, Action::Refresh, &dir);
        assert_eq!(state.selected, 0, "out-of-range selection resets to row 0");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn non_repository_load_is_an_error_not_a_panic() {
        let state = AppState::load(Path::new("/nonexistent-ggt-w4-repo"));
        assert!(state.rows.is_empty());
        assert!(state.status.error.is_some());
    }

    /// One-commit scratch repo with pinned identity (mirrors the `data.rs` tests).
    fn temp_repo(tag: &str) -> PathBuf {
        let dir: PathBuf = std::env::temp_dir().join(format!(
            "ggt-w4-app-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        data::run_git(&dir, &["init", "-q", "-b", "main"]).expect("git init");
        data::run_git(&dir, &["config", "user.name", "Test"]).expect("config name");
        data::run_git(&dir, &["config", "user.email", "test@example.invalid"])
            .expect("config email");
        data::run_git(&dir, &["config", "commit.gpgsign", "false"]).expect("config gpgsign");
        fs::write(dir.join("f.txt"), "1\n").expect("write file");
        data::run_git(&dir, &["add", "f.txt"]).expect("git add");
        data::run_git(&dir, &["commit", "-q", "-m", "c1"]).expect("git commit");
        dir
    }
}
