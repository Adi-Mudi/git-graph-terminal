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

use crate::actions::{self, DispatchOutcome, RefAction};
use crate::branches::{self, BranchEntry};
use crate::cdv::{CdvKind, CdvState};
use crate::data::{self, Commit};
use crate::dialog::{self, Dialog, DialogKey, DialogResult};
use crate::diff::{self, DiffLine};
use crate::editor::{self, EditorPlan};
use crate::files;
use crate::find::FindWidget;
use crate::layout::types::{GraphLayout, RowIdx};
use crate::layout::{self, LayoutOpts, NavDir, NO_ROW};
use crate::menu::{
    classify_ref, surface_for_refs, ColumnId, CommitOrder, ContextMenu, MenuAction, MenuColumns,
    MenuContext, MenuSurface,
};
use crate::uncommitted::{self, UncommittedState};

/// Extra terminal rows an expanded commit row occupies in W4.
///
/// The reference stretches the graph below an expanded row by `expandY`
/// (`src/config.ts:279`, default 250 px). W4 renders the gap only, as one
/// constant, so W5 can replace it with the measured commit-details pane without
/// touching the navigation code.
pub const EXPANDED_EXTRA_ROWS: u16 = 4;

/// The two top-level views of the body (M4-W12 / T1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    /// The uncommitted section plus the commit history.
    Overview,
    /// The branch list on the left, the filtered history on the right.
    Branches,
}

impl Tab {
    /// Both tabs, in bar order.
    pub const ALL: [Tab; 2] = [Tab::Overview, Tab::Branches];

    /// The label on the tab bar.
    pub fn label(self) -> &'static str {
        match self {
            Tab::Overview => "Overview",
            Tab::Branches => "Branches",
        }
    }

    /// The 1-based position on the bar (the `1` / `2` key).
    pub fn index(self) -> usize {
        match self {
            Tab::Overview => 0,
            Tab::Branches => 1,
        }
    }

    /// The tab for a `0`-based index, or `None`.
    pub fn from_index(index: usize) -> Option<Self> {
        Tab::ALL.get(index).copied()
    }
}

/// Which surface an open inline diff belongs to (M4-W12 / T4, T5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffSource {
    /// A file row of the uncommitted section.
    Uncommitted(usize),
    /// A file row inside an expanded commit's details.
    CommitFile {
        /// The commit row the file belongs to.
        row: RowIdx,
        /// The file's path.
        path: String,
    },
}

/// The diff currently shown inline, under its file row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineDiff {
    /// The pane title (`git diff <hash> -- <path>`).
    pub title: String,
    /// The parsed diff lines.
    pub lines: Vec<DiffLine>,
    /// Where the diff came from.
    pub source: DiffSource,
}

impl InlineDiff {
    /// The commit row this diff is attached to, if any.
    pub fn row(&self) -> Option<RowIdx> {
        match &self.source {
            DiffSource::CommitFile { row, .. } => Some(*row),
            DiffSource::Uncommitted(_) => None,
        }
    }
}

/// The vertical breakdown of one commit block, in display rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockParts {
    /// Rows taken by the refs line (`0` or `1`).
    pub refs: u16,
    /// Rows taken by the commit row itself (always `1`).
    pub commit: u16,
    /// Rows taken by the inline commit-details panel.
    pub cdv: u16,
    /// Rows taken by the inline diff.
    pub diff: u16,
    /// `refs + commit + cdv + diff`.
    pub total: u16,
}

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
    /// Result of the last context-menu action (W5 feedback, Rule 10-safe text).
    pub last_action: Option<String>,
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
    /// Which optional text columns are visible + the commit ordering (W5).
    pub columns: ColumnState,
    /// The open context menu, if any (W5).
    pub menu: Option<ContextMenu>,
    /// Text the last menu action asked to copy (W5/T4, emitted as OSC 52 by
    /// `main.rs` only when `GGT_OSC52=1`).
    pub pending_clipboard: Option<String>,
    /// Status-bar / viewport snapshot.
    pub status: StatusBarState,
    /// The open commit details view, if any (W6/T3).
    pub cdv: Option<CdvState>,
    /// Does the CDV own the keyboard? (`Tab` toggles it.)
    pub cdv_focused: bool,
    /// The find widget's state (W6/T1); `visible` drives the bottom bar.
    pub find: FindWidget,
    /// The open modal dialog, if any (W6/T2). While `Some`, it owns the keyboard.
    pub dialog: Option<Dialog>,
    /// The open read-only pager, if any (W7/T3). While `Some`, it owns the keyboard.
    pub pager: Option<crate::pager::Pager>,
    /// The commit row marked as the comparison base (`c`, VIS-040, W7/T4).
    pub compare_base: Option<RowIdx>,
    /// The row selected before the find widget opened, restored on `Esc`
    /// (W7/T2: `findWidget.ts:139-158` `restoreState`).
    pub selection_before_find: Option<RowIdx>,

    // ---- M4-W12 ----
    /// Which body tab is showing (T1).
    pub tab: Tab,
    /// Every local / remote / tag ref, with its reachable-commit count (T2).
    pub branches: Vec<BranchEntry>,
    /// Cursor into [`AppState::branches`].
    pub branch_selected: usize,
    /// The active branch filter (a full refname), or `None` for all history.
    pub branch_filter: Option<String>,
    /// The hashes reachable from [`AppState::branch_filter`] (T2).
    pub branch_reachable: Option<HashSet<String>>,
    /// The working-tree section (T4).
    pub uncommitted: UncommittedState,
    /// Is the uncommitted section expanded?
    pub uncommitted_open: bool,
    /// The selected uncommitted file, when the cursor is in that section.
    pub uncommitted_selected: Option<usize>,
    /// The inline diff currently open, if any (T4/T5).
    pub inline_diff: Option<InlineDiff>,
    /// The editor to hand `e` to, from `$GGT_EDITOR` (T6).
    pub editor_program: Option<String>,
    /// A staged editor launch for `main.rs` to run after the frame (T6).
    pub pending_editor: Option<EditorPlan>,
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
    /// Open the commit details view for the selected row — `d` (W6/T3).
    CdvOpen,
    /// Close the commit details view — `Esc` while it is focused (W6/T3).
    CdvClose,
    /// Move the keyboard focus between the commit list and the CDV — `Tab`.
    CdvFocusToggle,
    /// Switch the CDV file list between Tree and List — `v` (VIS-041/VIS-042).
    CdvToggleView,
    /// Open the find widget — `Ctrl+F` (W6/T1).
    FindOpen,
    /// Close the find widget and drop the highlight — `Ctrl+F` again.
    FindClose,
    /// Jump to the next match, wrapping — `F3` / `Ctrl+G`.
    FindNext,
    /// Jump to the previous match, wrapping — `Shift+F3`.
    FindPrev,
    /// Toggle match case (`findIsCaseSensitive`) — `Alt+C`.
    FindToggleCase,
    /// Toggle regular-expression mode (`findIsRegex`) — `Alt+R`.
    FindToggleRegex,
    /// Toggle "open the CDV for each match" (`findOpenCommitDetailsView`) — `Alt+D`.
    FindToggleCdv,
    /// Submit the open dialog — `Enter` (C02/C12).
    DialogConfirm,
    /// Dismiss the open dialog — `Esc`.
    DialogCancel,
    /// Open the uncommitted-changes CDV — `u` (VIS-039, W7/T4).
    CdvOpenUncommitted,
    /// Mark/unmark the selected commit as the comparison base — `c` (VIS-040).
    /// Pressing `c` on the already-marked row opens the comparison CDV.
    CompareMark,
    /// Move the CDV file-row cursor down — `j` / Down while the CDV is focused
    /// (W7/T3).
    CdvFileNext,
    /// Move the CDV file-row cursor up — `k` / Up while the CDV is focused.
    CdvFilePrev,
    /// Open the pager with `git diff` for the selected file row — `Enter` (A65).
    CdvFileViewDiff,
    /// Open the pager with `git show <hash>:<path>` — `o` (A66).
    CdvFileViewRevision,
    /// Mark/unmark the selected file row reviewed — `Space` (VIS-049).
    CdvFileToggleReviewed,
    /// Collapse the folder under the cursor — `Left`.
    CdvFolderCollapse,
    /// Expand the folder under the cursor — `Right`.
    CdvFolderExpand,
    /// Page `git diff <hash> -- <path>` for the selected file row — `w` (A67).
    /// The working-tree half of `viewDiffWithWorkingFile`.
    CdvFileViewDiffWithWorkingFile,
    /// Page `git show :<path>` for the selected file row — `e` (A68).
    CdvFileOpen,
    /// Ask before `git checkout <hash> -- <path>` — `R` (A71, gated).
    CdvFileResetToRevision,
    /// Copy the absolute path of the selected file row — `y` (A72).
    CdvFileCopyAbsolutePath,
    /// Copy the repo-relative path of the selected file row — `Y` (A73).
    CdvFileCopyRelativePath,

    // M4-W12 — the tabs, the uncommitted section and the inline diff.
    /// Switch to the Overview tab — `1`.
    TabOverview,
    /// Switch to the Branches tab — `2`.
    TabBranches,
    /// Cycle to the other tab — `Ctrl+Tab`.
    TabToggle,
    /// Move the branch-list cursor down — `]` (also `j` on the Branches tab).
    BranchNext,
    /// Move the branch-list cursor up — `[` (also `k` on the Branches tab).
    BranchPrev,
    /// Filter the history on the selected branch — `Enter` on the branch list.
    BranchFilter,
    /// Toggle the uncommitted section open/closed and jump to it — `u`.
    UncommittedToggle,
    /// Open the diff for the selected inline file row — `Enter`.
    InlineFileOpenDiff,
    /// Open the selected file's two revisions in `$GGT_EDITOR` — `e`.
    EditorOpen,
}

impl Action {
    /// Every variant, for the W7 regression harness.
    ///
    /// The harness asserts each one is reachable from a key, so adding a
    /// variant without a binding fails a test instead of a pty capture. The
    /// list is test-only API: the shipped binary never enumerates actions.
    #[cfg(test)]
    pub const ALL: &'static [Action] = &[
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
        Action::Refresh,
        Action::Quit,
        Action::CdvOpen,
        Action::CdvClose,
        Action::CdvFocusToggle,
        Action::CdvToggleView,
        Action::FindOpen,
        Action::FindClose,
        Action::FindNext,
        Action::FindPrev,
        Action::FindToggleCase,
        Action::FindToggleRegex,
        Action::FindToggleCdv,
        Action::DialogConfirm,
        Action::DialogCancel,
        Action::CdvOpenUncommitted,
        Action::CompareMark,
        Action::CdvFileNext,
        Action::CdvFilePrev,
        Action::CdvFileViewDiff,
        Action::CdvFileViewRevision,
        Action::CdvFileToggleReviewed,
        Action::CdvFolderCollapse,
        Action::CdvFolderExpand,
        Action::CdvFileViewDiffWithWorkingFile,
        Action::CdvFileOpen,
        Action::CdvFileResetToRevision,
        Action::CdvFileCopyAbsolutePath,
        Action::CdvFileCopyRelativePath,
        Action::TabOverview,
        Action::TabBranches,
        Action::TabToggle,
        Action::BranchNext,
        Action::BranchPrev,
        Action::BranchFilter,
        Action::UncommittedToggle,
        Action::InlineFileOpenDiff,
        Action::EditorOpen,
    ];
}

/// What [`reduce`] wants the caller to do next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Redraw the frame.
    Redraw,
    /// The caller should tear down and exit.
    Quit,
}

/// Which optional text columns the table shows, plus the commit ordering (W5/T3).
///
/// The default is the pre-W5 table (`idx`, hash, date, author, subject, refs)
/// with `git`'s `--date-order`, so the P0/P3 dumps stay reproducible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColumnState {
    pub date: bool,
    pub author: bool,
    /// The commit subject column (the reference calls it `Commit`).
    pub commit: bool,
    pub order: CommitOrder,
}

impl Default for ColumnState {
    fn default() -> Self {
        ColumnState {
            date: true,
            author: true,
            commit: true,
            order: CommitOrder::Date,
        }
    }
}

impl ColumnState {
    pub fn is_visible(&self, column: ColumnId) -> bool {
        match column {
            ColumnId::Date => self.date,
            ColumnId::Author => self.author,
            ColumnId::Commit => self.commit,
        }
    }

    pub fn set_visible(&mut self, column: ColumnId, visible: bool) {
        match column {
            ColumnId::Date => self.date = visible,
            ColumnId::Author => self.author = visible,
            ColumnId::Commit => self.commit = visible,
        }
    }

    /// Flip a column; returns its new visibility.
    pub fn toggle(&mut self, column: ColumnId) -> bool {
        let next = !self.is_visible(column);
        self.set_visible(column, next);
        next
    }

    /// The snapshot the header menu renders (checkmarks + radio).
    pub fn menu_columns(&self) -> MenuColumns {
        MenuColumns {
            date: self.date,
            author: self.author,
            commit: self.commit,
            order: self.order,
        }
    }
}

/// The name a column shows in the header and in status messages.
pub fn column_label(column: ColumnId) -> &'static str {
    match column {
        ColumnId::Date => "Date",
        ColumnId::Author => "Author",
        ColumnId::Commit => "Commit",
    }
}

/// A keypress translated for the open context menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuNav {
    Up,
    Down,
    Activate,
    Close,
    /// Single-key shortcut inside the menu (`a`..`z`).
    Shortcut(char),
}

/// A keypress translated for the open pager (W7/T3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PagerNav {
    LineUp,
    LineDown,
    PageUp,
    PageDown,
    Top,
    Bottom,
    Close,
}

/// What [`AppState::menu_nav`] did, so the caller can dispatch on activation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuKey {
    /// The highlight moved (redraw only).
    Moved,
    /// A row was activated; the caller runs [`AppState::activate_menu`].
    Activated,
    /// The menu was closed without activating anything.
    Closed,
    /// Nothing happened (a disabled row, or an unmatched key).
    Ignored,
}

/// Screen geometry the renderer used, so a mouse event can be routed back to a
/// surface without re-deriving the layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HitGeometry {
    /// The whole main area (graph + text columns + header).
    pub body: ratatui::layout::Rect,
    /// Width of the graph pane in columns.
    pub graph_width: u16,
    /// Does the area start with a one-row table header?
    pub has_header: bool,
    /// The inline commit-details panel, when it is open (W6/T3).
    /// `None` means no CDV is drawn, so no cell can belong to it.
    pub cdv: Option<ratatui::layout::Rect>,
    /// The tab bar, always drawn since M4-W12/T1.
    pub tab_bar: ratatui::layout::Rect,
    /// The Branches tab's left pane, when that tab is active and the terminal is
    /// wide enough to show it (M4-W12/T2). `None` means no cell is a branch row.
    pub branch_pane: Option<ratatui::layout::Rect>,
}

/// What a screen cell belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClickTarget {
    /// A tab-bar label → switch to that tab (M4-W12/T1, T7).
    Tab(Tab),
    /// The table header row, right of the graph → column menu.
    ColumnHeader,
    /// A commit row, with the surface its refs (or its position) imply.
    Row(RowIdx, MenuSurface, String),
    /// A file row in the inline commit-details view, with its full path — the
    /// `MenuSurface::FileRow` menu (A65…A73, W10).
    CdvFile(String),
    /// A folder row in the inline commit-details view, with its full path.
    ///
    /// The `FileRow` menu is file-only, so a folder opens no menu: the reference
    /// binds a click on `.fileTreeFolder` to the open/closed flip instead
    /// (`web/main.ts:2968-2978`), which is INT-050.
    CdvFolder(String),
    /// A branch row in the Branches tab's left pane, by index into
    /// [`AppState::branches`] (M4-W12/T2).
    Branch(usize),
}

impl AppState {
    /// Load (or reload) the state for `repo`; never panics, errors are captured.
    pub fn load(repo: &Path) -> Self {
        let mut error: Option<String> = None;
        let mut rows: Vec<Commit> = Vec::new();
        let mut branch = "(detached HEAD)".to_string();
        let mut head = "(no commits)".to_string();

        if !data::is_repo(repo) {
            error = Some(format!("not a git repository: {}", repo.display()));
        } else {
            // Error priority mirrors the P0 `App::load`: branches, then commits,
            // then HEAD only when nothing else has failed.
            if let Err(e) = data::branches(repo) {
                error = Some(e.to_string());
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
        let uncommitted = uncommitted::UncommittedState::load(repo).unwrap_or_default();
        let branch_list = branches::load(repo).unwrap_or_default();
        let branch_count = branch_list.len();
        Self::from_parts(
            rows,
            layout,
            branch,
            head,
            branch_count,
            repo_label(repo),
            error,
            uncommitted,
            branch_list,
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
            Self::demo_uncommitted(),
            Self::demo_branches(),
        )
    }

    /// The `--mock` uncommitted section (M4-W12/T4): one file per legend letter, so
    /// the section renders every row variant without a working tree.
    fn demo_uncommitted() -> UncommittedState {
        use crate::files::GitFileStatus;
        use crate::uncommitted::UncommittedFile;
        let file = |status, path: &str, staged, unstaged, adds, dels| UncommittedFile {
            status,
            path: path.to_string(),
            old_path: path.to_string(),
            staged,
            unstaged,
            additions: adds,
            deletions: dels,
        };
        UncommittedState {
            files: vec![
                file(
                    GitFileStatus::Modified,
                    "src/render/ui.rs",
                    true,
                    true,
                    42,
                    8,
                ),
                file(
                    GitFileStatus::Added,
                    "src/render/tabs.rs",
                    false,
                    true,
                    120,
                    0,
                ),
                file(
                    GitFileStatus::Deleted,
                    "src/legacy/panel.rs",
                    true,
                    false,
                    0,
                    65,
                ),
            ],
        }
    }

    /// The `--mock` branch list (M4-W12/T2).
    fn demo_branches() -> Vec<BranchEntry> {
        use crate::branches::RefKind;
        let entry = |name: &str, kind, is_head, count| BranchEntry {
            name: name.to_string(),
            refname: match kind {
                RefKind::Tag => format!("refs/tags/{name}"),
                RefKind::Remote => format!("refs/remotes/{name}"),
                RefKind::Local => format!("refs/heads/{name}"),
            },
            kind,
            is_head,
            count,
        };
        vec![
            entry("main", RefKind::Local, true, 8),
            entry("feature/graph", RefKind::Local, false, 4),
            entry("origin/main", RefKind::Remote, false, 8),
            entry("v0.1", RefKind::Tag, false, 5),
        ]
    }

    /// Assemble a state from already-loaded pieces.
    #[allow(clippy::too_many_arguments)]
    fn from_parts(
        rows: Vec<Commit>,
        layout: GraphLayout,
        branch: String,
        head: String,
        branches: usize,
        repo_label: String,
        error: Option<String>,
        uncommitted: UncommittedState,
        branch_list: Vec<BranchEntry>,
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
            last_action: None,
            error,
        };
        Self {
            rows,
            layout,
            selected: 0,
            expanded: HashSet::new(),
            scroll: 0,
            columns: ColumnState::default(),
            menu: None,
            pending_clipboard: None,
            status,
            cdv: None,
            cdv_focused: false,
            find: FindWidget::new(),
            dialog: None,
            pager: None,
            compare_base: None,
            selection_before_find: None,
            tab: Tab::Overview,
            branches: branch_list,
            branch_selected: 0,
            branch_filter: None,
            branch_reachable: None,
            uncommitted,
            uncommitted_open: true,
            uncommitted_selected: None,
            inline_diff: None,
            editor_program: editor::program_from_env(),
            pending_editor: None,
        }
    }

    /// The vertical breakdown of commit row `row` (T1/T3/T7).
    pub fn block_parts(&self, row: RowIdx) -> BlockParts {
        let refs = u16::from(self.has_refs_line(row));
        let cdv = self.inline_cdv_height(row);
        let diff = self.inline_diff_height(row);
        BlockParts {
            refs,
            commit: 1,
            cdv,
            diff,
            total: refs + 1 + cdv + diff,
        }
    }

    /// Does `row` get its own refs line above the commit (T7)?
    pub fn has_refs_line(&self, row: RowIdx) -> bool {
        self.rows
            .get(row as usize)
            .map(|commit| !commit.refs.is_empty())
            .unwrap_or(false)
    }

    /// Rows the inline commit-details panel occupies under `row`.
    ///
    /// An expanded row reserves at least the reference gap even before its
    /// details have loaded — and even when the viewport is not known yet, as it
    /// is not outside the TUI — so the layout never jumps when the panel arrives
    /// (W4 contract).
    pub fn inline_cdv_height(&self, row: RowIdx) -> u16 {
        if !self.expanded.contains(&row) {
            return 0;
        }
        match &self.cdv {
            Some(panel) if panel.row == row => {
                crate::cdv::inline_height(panel, self.status.height.max(1)).max(EXPANDED_EXTRA_ROWS)
            }
            _ => EXPANDED_EXTRA_ROWS,
        }
    }

    /// Rows the inline diff occupies under `row`.
    pub fn inline_diff_height(&self, row: RowIdx) -> u16 {
        match &self.inline_diff {
            Some(diff) if diff.row() == Some(row) => diff.lines.len() as u16 + 1,
            _ => 0,
        }
    }

    /// Rows the uncommitted section occupies at the top of the body (T4).
    ///
    /// The Branches tab has no uncommitted section, so it contributes nothing.
    pub fn uncommitted_height(&self) -> u16 {
        if self.tab != Tab::Overview || self.uncommitted.is_empty() {
            return 0;
        }
        // header + summary
        let mut height = 2u16;
        if self.uncommitted_open {
            height = height.saturating_add(self.uncommitted.len() as u16);
        }
        if let Some(diff) = &self.inline_diff {
            if matches!(diff.source, DiffSource::Uncommitted(_)) {
                height = height.saturating_add(diff.lines.len() as u16 + 1);
            }
        }
        height
    }

    /// The commit rows the current tab shows, in order.
    ///
    /// The Overview tab shows every row; the Branches tab shows only the rows
    /// reachable from the active branch filter (T2).
    pub fn visible_rows(&self) -> Vec<RowIdx> {
        match (self.tab, &self.branch_reachable) {
            (Tab::Branches, Some(set)) => branches::filter_rows(
                self.rows
                    .iter()
                    .enumerate()
                    .map(|(index, commit)| (index, commit.hash.as_str())),
                set,
            )
            .into_iter()
            .map(|index| index as RowIdx)
            .collect(),
            _ => (0..self.rows.len() as RowIdx).collect(),
        }
    }

    /// Terminal height of commit row `row`: its whole block (T1/T3/T7).
    pub fn row_height(&self, row: RowIdx) -> u16 {
        self.block_parts(row).total
    }

    /// Display-row offset (top) of commit row `row`, below the uncommitted block.
    pub fn row_top(&self, row: RowIdx) -> u16 {
        let mut top = self.uncommitted_height();
        for r in self.visible_rows() {
            if r >= row {
                break;
            }
            top = top.saturating_add(self.row_height(r));
        }
        top
    }

    /// Total display rows the body occupies.
    pub fn display_rows(&self) -> u16 {
        let mut total = self.uncommitted_height();
        for r in self.visible_rows() {
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

    /// Is a context menu open?
    pub fn menu_is_open(&self) -> bool {
        self.menu.is_some()
    }

    /// Is a modal dialog open? While `Some`, the dialog owns the keyboard (W7/T1).
    pub fn dialog_is_open(&self) -> bool {
        self.dialog.is_some()
    }

    /// Is the read-only pager open? While `Some`, it owns the keyboard (W7/T3).
    pub fn pager_is_open(&self) -> bool {
        self.pager.is_some()
    }

    /// Does any loaded row carry a remote-tracking decoration?
    ///
    /// `data.rs` does not load the remote list, so the refs are the only signal
    /// the view has; the branch menu greys out `Push` accordingly.
    pub fn has_remotes(&self) -> bool {
        self.rows.iter().any(|commit| {
            commit
                .refs
                .iter()
                .any(|reference| matches!(classify_ref(reference).0, MenuSurface::RemoteBranch))
        })
    }

    /// Seed a [`MenuContext`] from the selected row and the surface.
    fn menu_context(&self, surface: MenuSurface, target_ref: Option<&str>) -> MenuContext {
        let commit = self.rows.get(self.selected as usize);
        let mut ctx = MenuContext {
            columns: self.columns.menu_columns(),
            has_remotes: self.has_remotes(),
            ..MenuContext::default()
        };
        if let Some(commit) = commit {
            ctx.hash = commit.hash.clone();
            ctx.subject = commit.subject.clone();
        }
        let name = target_ref.map(str::to_string).or_else(|| {
            commit
                .and_then(|commit| surface_for_refs(&commit.refs))
                .map(|(_, name)| name)
        });
        if let Some(name) = name {
            let base = MenuContext::from_ref(surface, &name);
            ctx.ref_name = base.ref_name;
            ctx.remote = base.remote;
            ctx.branch = base.branch;
            ctx.selector = base.selector;
        }
        match surface {
            // A row menu is about the commit, not about a ref name.
            MenuSurface::CommitRow | MenuSurface::Uncommitted => {
                ctx.ref_name.clear();
                ctx.branch.clear();
                ctx.remote.clear();
                ctx.selector.clear();
            }
            // A file-row menu is about one path. `from_ref` would otherwise file
            // the path under `ref_name`, and every `A65`..`A73` item is gated on
            // `path` being set (W10).
            MenuSurface::FileRow => {
                ctx.ref_name.clear();
                ctx.branch.clear();
                ctx.remote.clear();
                ctx.selector.clear();
                ctx.path = target_ref.unwrap_or_default().to_string();
            }
            _ => {}
        }
        if matches!(surface, MenuSurface::Uncommitted) {
            ctx.hash = "HEAD".to_string();
        }
        ctx.is_current_branch = !ctx.ref_name.is_empty() && ctx.ref_name == self.status.branch;
        ctx
    }

    /// Open the menu for `surface` at screen `(x, y)`; `target_ref` names the
    /// decoration that was clicked (mouse) or picked from the row (keyboard).
    pub fn open_menu(
        &mut self,
        surface: MenuSurface,
        target_ref: Option<&str>,
        x: u16,
        y: u16,
    ) -> MenuSurface {
        let context = self.menu_context(surface, target_ref);
        let mut menu = ContextMenu::open(surface, x, y, context);
        menu.selected = menu.first_enabled();
        self.menu = Some(menu);
        surface
    }

    /// Keyboard path (Shift+F10): open the menu for the selected row.
    pub fn open_menu_for_selection(&mut self, x: u16, y: u16) -> Option<MenuSurface> {
        let (surface, name) = {
            let commit = self.rows.get(self.selected as usize)?;
            surface_for_refs(&commit.refs).unwrap_or((MenuSurface::CommitRow, commit.hash.clone()))
        };
        Some(self.open_menu(surface, Some(&name), x, y))
    }

    /// Route one key to the open menu.
    pub fn menu_nav(&mut self, nav: MenuNav) -> MenuKey {
        if nav == MenuNav::Close {
            self.menu = None;
            return MenuKey::Closed;
        }
        let Some(menu) = self.menu.as_mut() else {
            return MenuKey::Ignored;
        };
        match nav {
            MenuNav::Up => {
                menu.move_selection(-1);
                MenuKey::Moved
            }
            MenuNav::Down => {
                menu.move_selection(1);
                MenuKey::Moved
            }
            MenuNav::Shortcut(key) => {
                if menu.select_shortcut(key)
                    && menu
                        .selected_item()
                        .map(|item| item.enabled)
                        .unwrap_or(false)
                {
                    MenuKey::Activated
                } else {
                    MenuKey::Moved
                }
            }
            MenuNav::Activate => {
                if menu
                    .selected_item()
                    .map(|item| item.enabled)
                    .unwrap_or(false)
                {
                    MenuKey::Activated
                } else {
                    MenuKey::Ignored
                }
            }
            MenuNav::Close => MenuKey::Closed,
        }
    }

    /// Close the menu and run the highlighted row, returning the status line.
    ///
    /// This is the W5 "menu → dispatch → refresh" seam: safe plans run and the
    /// view reloads, gated plans stop at the W6 confirmation stub, and local
    /// plans (column toggles, clipboard copies) update the view state.
    pub fn activate_menu(&mut self, repo: &Path) -> String {
        let Some(menu) = self.menu.take() else {
            return "no context menu is open".to_string();
        };
        let Some(item) = menu.selected_item().cloned() else {
            return "the context menu has no rows".to_string();
        };
        if !item.enabled {
            self.menu = Some(menu);
            return format!("{}: disabled for this row", item.label);
        }

        let line = match item.action.clone() {
            MenuAction::Dispatch(action) => {
                let mut target = menu.context.action_target();
                target.clipboard = clipboard_text(item.id, &menu.context);
                let outcome = actions::dispatch(repo, action, &target);
                // W6/T2: a gated plan opens the confirmation dialog instead of
                // stopping at the W5 stub. `dispatch` already refused to run it,
                // so the dialog is the only path to `execute_confirmed`.
                if let actions::DispatchOutcome::NeedsConfirm(plan) = &outcome {
                    let plan = plan.clone();
                    if open_dialog_for_plan(self, action, &target, &plan) {
                        self.menu = None;
                        self.status.last_action = Some(format!("confirm: {}", plan.label));
                        return self.status.last_action.clone().unwrap_or_default();
                    }
                }
                if matches!(outcome.plan().kind, actions::PlanKind::Execute)
                    && outcome.plan().mutates()
                {
                    reduce(self, Action::Refresh, repo);
                }
                // Copy rows hand their text to the OSC 52 bridge, but only a
                // plan that actually runs (never a gated confirmation stub).
                if matches!(
                    item.action,
                    MenuAction::Dispatch(RefAction::CopyToClipboard)
                ) && !target.clipboard.is_empty()
                {
                    self.pending_clipboard = Some(target.clipboard.clone());
                }
                outcome.status_line()
            }
            MenuAction::ToggleColumn(column) => {
                let visible = self.columns.toggle(column);
                format!(
                    "{} column: {}",
                    column_label(column),
                    if visible { "shown" } else { "hidden" }
                )
            }
            MenuAction::SetColumnOrder(order) => {
                self.columns.order = order;
                format!("ordering: {} ({})", order.label(), order.git_flag())
            }
            MenuAction::SelectInDropdown(name) => {
                format!("selected {name} in the branches dropdown (local view state)")
            }
            MenuAction::UnselectInDropdown(name) => {
                format!("cleared {name} from the branches dropdown (local view state)")
            }
            MenuAction::FollowInternalLink(link) => {
                format!("followed internal link {link} (local view state)")
            }
        };
        self.status.last_action = Some(line.clone());
        line
    }

    /// The commit row occupying display row `display`, if any.
    pub fn row_at_display(&self, display: u16) -> Option<RowIdx> {
        let mut cursor = self.uncommitted_height();
        for row in self.visible_rows() {
            let height = self.row_height(row);
            if display >= cursor && display < cursor.saturating_add(height) {
                return Some(row);
            }
            cursor = cursor.saturating_add(height);
        }
        None
    }

    /// Route a screen cell back to a click target (mouse right-click support).
    pub fn surface_at(&self, x: u16, y: u16, hit: &HitGeometry) -> Option<ClickTarget> {
        // The tab bar sits above the body, so it is checked first: a cell there
        // must never be read as a commit row (T1/T7).
        let bar = hit.tab_bar;
        if bar.height > 0
            && y >= bar.y
            && y < bar.y.saturating_add(bar.height)
            && x < bar.x.saturating_add(bar.width)
        {
            return crate::render::tab_at_x(x).map(ClickTarget::Tab);
        }
        // The Branches tab's left pane is its own surface, to the left of the
        // body: a cell inside it is a branch row, never a commit row (T2).
        if let Some(pane) = hit.branch_pane {
            if x < pane.x.saturating_add(pane.width)
                && y >= pane.y
                && y < pane.y.saturating_add(pane.height)
            {
                return self.branch_at(x, y, pane);
            }
        }
        // The inline CDV is part of the body flow, so it is checked before the
        // commit rows it is embedded in: a cell inside the panel must never be
        // read as a commit row (W6/T3). A summary line or a cell outside the
        // panel's inner rect falls through to the body checks below.
        if let Some(cdv) = hit.cdv {
            if let Some(target) = self.cdv_file_at(x, y, cdv) {
                return Some(target);
            }
        }
        let body = hit.body;
        if body.width == 0 || body.height == 0 {
            return None;
        }
        if x < body.x || x >= body.x.saturating_add(body.width) {
            return None;
        }
        if y < body.y || y >= body.y.saturating_add(body.height) {
            return None;
        }
        let header_rows = u16::from(hit.has_header && body.height > 1);
        let row_in_body = y - body.y;
        let table_x = body.x.saturating_add(hit.graph_width);
        if header_rows == 1 && row_in_body == 0 {
            // Only the table side has a header; the graph starts one row lower.
            return (x >= table_x).then_some(ClickTarget::ColumnHeader);
        }
        let display = self.scroll + (row_in_body - header_rows);
        let row = self.row_at_display(display)?;
        let commit = self.rows.get(row as usize)?;
        if x < table_x {
            return Some(ClickTarget::Row(
                row,
                MenuSurface::CommitRow,
                commit.hash.clone(),
            ));
        }
        // The refs column is the last 20 cells of the text area (`columns.rs`).
        let refs_left = body.x.saturating_add(body.width).saturating_sub(20);
        if x >= refs_left {
            if let Some((surface, name)) = surface_for_refs(&commit.refs) {
                return Some(ClickTarget::Row(row, surface, name));
            }
        }
        Some(ClickTarget::Row(
            row,
            MenuSurface::CommitRow,
            commit.hash.clone(),
        ))
    }

    /// The branch row under `(x, y)` in the Branches tab's left pane, or `None`.
    ///
    /// The pane draws [`crate::render::branch_rows`] top-down, so the row index
    /// inside the pane maps straight onto that list; a group heading is not a
    /// row and resolves to `None` (T2).
    fn branch_at(&self, _x: u16, y: u16, pane: ratatui::layout::Rect) -> Option<ClickTarget> {
        let rows = crate::render::branch_rows(&self.branches);
        let offset = crate::render::branch_pane_offset(&rows, self.branch_selected, pane.height);
        let index = usize::from(y.saturating_sub(pane.y)).checked_add(offset)?;
        match rows.get(index)? {
            crate::render::BranchRow::Entry(entry) => Some(ClickTarget::Branch(*entry)),
            crate::render::BranchRow::Header(_) => None,
        }
    }

    /// The file row under `(x, y)` inside the inline CDV, or `None` (W6/T3).
    ///
    /// The panel renders [`CdvState::lines`], whose first
    /// `summary_lines().len() + 2` lines are the summary block plus the
    /// `Files (N) — … View` header; the file rows follow in `file_rows()` order.
    /// Folder rows carry no menu surface, so a click on one resolves to `None`.
    fn cdv_file_at(&self, x: u16, y: u16, cdv: ratatui::layout::Rect) -> Option<ClickTarget> {
        let panel = self.cdv.as_ref()?;
        // `cdv::draw` wraps the panel in a one-cell border on every side.
        let inner_x = cdv.x.saturating_add(1);
        let inner_y = cdv.y.saturating_add(1);
        let inner_w = cdv.width.saturating_sub(2);
        let inner_h = cdv.height.saturating_sub(2);
        if inner_w == 0 || inner_h == 0 {
            return None;
        }
        if x < inner_x || x >= inner_x.saturating_add(inner_w) {
            return None;
        }
        if y < inner_y || y >= inner_y.saturating_add(inner_h) {
            return None;
        }
        let display = panel.scroll.saturating_add(y - inner_y);
        let header = panel.summary_lines().len() as u16 + 2;
        let index = usize::from(display.checked_sub(header)?);
        let rows = panel.file_rows();
        match rows.get(index)?.kind {
            files::FileRowKind::File(_) => Some(ClickTarget::CdvFile(rows[index].path.clone())),
            files::FileRowKind::Folder { .. } => {
                Some(ClickTarget::CdvFolder(rows[index].path.clone()))
            }
        }
    }
}

/// Clipboard text for a `Copy …` menu row, keyed by the inventory id.
fn clipboard_text(id: &str, ctx: &MenuContext) -> String {
    match id {
        "A32" | "A50" => ctx.hash.clone(),
        "A33" => ctx.subject.clone(),
        "A49" => {
            if ctx.selector.is_empty() {
                ctx.ref_name.clone()
            } else {
                ctx.selector.clone()
            }
        }
        "A55" | "A21" | "A43" => ctx.ref_name.clone(),
        "A64" => ctx.url.clone(),
        "A72" | "A73" => ctx.path.clone(),
        _ => ctx.ref_name.clone(),
    }
}

/// Apply `action` to `state`; `repo` is only read by [`Action::Refresh`].
///
/// "Pure-ish": every branch is a deterministic state transition, except Refresh,
/// which re-reads the repository through [`AppState::load`].
pub fn reduce(state: &mut AppState, action: Action, repo: &Path) -> Outcome {
    match action {
        Action::Quit => {
            // Esc closes the innermost thing first: an inline diff, then the
            // branch filter, and only then the app.
            if state.inline_diff.is_some() {
                state.inline_diff = None;
            } else if state.tab == Tab::Branches && state.branch_filter.is_some() {
                clear_branch_filter(state);
            } else {
                return Outcome::Quit;
            }
        }
        Action::SelectNext => {
            if state.tab == Tab::Branches {
                select_branch_entry(state, 1);
            } else {
                select_table(state, 1);
            }
        }
        Action::SelectPrev => {
            if state.tab == Tab::Branches {
                select_branch_entry(state, -1);
            } else {
                select_table(state, -1);
            }
        }
        Action::PageNext => select_page(state, 1),
        Action::PagePrev => select_page(state, -1),
        Action::Top => state.selected = state.visible_rows().first().copied().unwrap_or(0),
        Action::Bottom => {
            state.selected = state.visible_rows().last().copied().unwrap_or(0);
        }
        Action::ToggleExpand => {
            if state.tab == Tab::Branches {
                apply_branch_filter(state, repo);
            } else if state.uncommitted_selected.is_some() {
                open_inline_uncommitted_diff(state, repo);
            } else {
                toggle_expand(state);
                // W6/T3: expanding a row loads its commit details view;
                // collapsing it (or expanding a different row) closes or
                // replaces the panel.
                if state.expanded.contains(&state.selected) {
                    open_cdv(state, repo);
                } else {
                    close_cdv_for(state, state.selected);
                }
            }
        }
        // Right / Ctrl+Down follows the first parent (older, `graph.ts:598-603`).
        Action::NextBranch => select_branch(state, NavDir::Down),
        // Left / Ctrl+Up follows the first child (newer, `graph.ts:626-644`).
        Action::PrevBranch => select_branch(state, NavDir::Up),
        // R1.2: no Shift = NEXT stash; Shift = PREVIOUS. Both wrap.
        Action::StashNext => select_stash(state, true),
        Action::StashPrev => select_stash(state, false),
        Action::Refresh => reload(state, repo),

        // W6/T3 — the commit details view.
        Action::CdvOpen => open_cdv(state, repo),
        Action::CdvClose => {
            state.cdv = None;
            state.cdv_focused = false;
        }
        Action::CdvFocusToggle => state.cdv_focused = !state.cdv_focused && state.cdv.is_some(),
        Action::CdvToggleView => {
            if let Some(cdv) = state.cdv.as_mut() {
                cdv.toggle_view();
            }
        }

        // W6/T1 — the find widget.
        Action::FindOpen => {
            if !state.find.visible {
                state.selection_before_find = Some(state.selected);
            }
            state.find.show();
            recompute_find(state);
        }
        Action::FindClose => close_find(state),
        Action::FindNext => find_step(state, true),
        Action::FindPrev => find_step(state, false),
        Action::FindToggleCase => {
            state.find.case_sensitive = !state.find.case_sensitive;
            recompute_find(state);
        }
        Action::FindToggleRegex => {
            state.find.is_regex = !state.find.is_regex;
            recompute_find(state);
        }
        Action::FindToggleCdv => {
            state.find.open_cdv = !state.find.open_cdv;
            follow_find_cdv(state, repo);
        }

        // W6/T2 — the modal dialog. The keyboard text editing lives in
        // [`AppState::dialog_key`]; these two actions are the terminal states.
        Action::DialogConfirm => confirm_dialog(state, repo),
        Action::DialogCancel => state.dialog = None,

        // W7/T3 — the CDV file-row cursor and the A65–A68 file actions.
        Action::CdvFileNext => cdv_move_file(state, 1),
        Action::CdvFilePrev => cdv_move_file(state, -1),
        Action::CdvFileViewDiff => cdv_open_pager(state, repo, RefAction::ViewDiff),
        Action::CdvFileViewRevision => cdv_open_pager(state, repo, RefAction::ViewFileAtRevision),
        Action::CdvFileToggleReviewed => cdv_toggle_reviewed(state),
        Action::CdvFolderCollapse => cdv_set_folder(state, false),
        Action::CdvFolderExpand => cdv_set_folder(state, true),
        // W10 — the rest of the `FileRow` inventory (A67, A68, A71…A73).
        Action::CdvFileViewDiffWithWorkingFile => {
            cdv_open_pager(state, repo, RefAction::ViewDiffWithWorkingFile)
        }
        Action::CdvFileOpen => cdv_open_pager(state, repo, RefAction::OpenFile),
        Action::CdvFileResetToRevision => cdv_reset_file(state, repo),
        Action::CdvFileCopyAbsolutePath => cdv_copy_path(state, repo, true),
        Action::CdvFileCopyRelativePath => cdv_copy_path(state, repo, false),

        // W7/T4 — the uncommitted and comparison CDV kinds.
        Action::CdvOpenUncommitted => open_uncommitted_cdv(state, repo),
        Action::CompareMark => compare_mark(state, repo),

        // M4-W12 — the tabs, the branch list, the uncommitted section and the
        // inline diff (T1, T2, T4, T5, T6).
        Action::TabOverview => set_tab(state, Tab::Overview),
        Action::TabBranches => set_tab(state, Tab::Branches),
        Action::TabToggle => set_tab(
            state,
            if state.tab == Tab::Overview {
                Tab::Branches
            } else {
                Tab::Overview
            },
        ),
        Action::BranchNext => select_branch_entry(state, 1),
        Action::BranchPrev => select_branch_entry(state, -1),
        Action::BranchFilter => apply_branch_filter(state, repo),
        Action::UncommittedToggle => toggle_uncommitted(state),
        Action::InlineFileOpenDiff => open_inline_file_diff(state, repo),
        Action::EditorOpen => stage_editor(state, repo),
    }
    state.sync_status();
    state.ensure_visible();
    Outcome::Redraw
}

/// Table-order movement (inventory C03): clamps at both ends, never wraps.
///
/// M4-W12: the uncommitted section sits above the commit table, so the cursor
/// walks into it on `k` at the first commit and out of it on `j` at the last
/// file — one continuous list, like the reference's virtual first row.
fn select_table(state: &mut AppState, delta: i32) {
    let visible = state.visible_rows();
    if visible.is_empty() {
        return;
    }
    if let Some(index) = state.uncommitted_selected {
        let last = state.uncommitted.len().saturating_sub(1) as i32;
        let next = index as i32 + delta;
        if next < 0 {
            return;
        }
        if next > last {
            state.uncommitted_selected = None;
            state.selected = visible[0];
            return;
        }
        state.uncommitted_selected = Some(next as usize);
        return;
    }
    let position = visible
        .iter()
        .position(|row| *row == state.selected)
        .unwrap_or(0) as i32;
    let next = position + delta;
    if next < 0 {
        if state.uncommitted_open && !state.uncommitted.is_empty() {
            state.uncommitted_selected = Some(state.uncommitted.len() - 1);
        }
        return;
    }
    let last = (visible.len() - 1) as i32;
    state.selected = visible[next.min(last) as usize];
}

/// Page movement: `PageDown = viewport height - 1` (clamped).
fn select_page(state: &mut AppState, dir: i32) {
    let visible = state.visible_rows();
    if visible.is_empty() {
        return;
    }
    let page = state.status.height.max(1).saturating_sub(1).max(1) as i32;
    let position = visible
        .iter()
        .position(|row| *row == state.selected)
        .unwrap_or(0) as i32;
    let last = (visible.len() - 1) as i32;
    let next = (position + dir * page).clamp(0, last);
    state.selected = visible[next as usize];
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

/* ------------------------------------------------------------------ */
/* M4-W12 — tabs, branches, uncommitted, inline diff                   */
/* ------------------------------------------------------------------ */

/// Switch tabs and keep the selection valid for the new tab (T1).
/// Switch to `tab`, moving the cursor into whichever list that tab shows (T1).
///
/// Public because the mouse router switches tabs on a tab-bar click, the same
/// gesture the `1` / `2` keys perform.
pub fn set_tab(state: &mut AppState, tab: Tab) {
    if state.tab == tab {
        return;
    }
    state.tab = tab;
    state.scroll = 0;
    state.inline_diff = None;
    state.uncommitted_selected = None;
    state.selected = state.visible_rows().first().copied().unwrap_or(0);
}

/// Rows one mouse-wheel notch moves.
pub const WHEEL_LINES: i32 = 3;

/// Mouse-wheel scrolling: [`WHEEL_LINES`] per notch, clamped at both ends.
///
/// The wheel scrolls the surface under the pointer (T7). Over the Branches
/// tab's left pane it walks the branch cursor instead, because that pane has no
/// scroll offset of its own — its window follows the cursor (T2).
pub fn wheel_scroll(state: &mut AppState, delta: i32, over_branch_pane: bool) {
    if over_branch_pane && state.tab == Tab::Branches {
        // One entry per notch: a wheel walks a list, it does not page it.
        select_branch_entry(state, delta.signum());
        return;
    }
    let view = state.status.height.max(1);
    let max = state.display_rows().saturating_sub(view) as i32;
    state.scroll = (state.scroll as i32 + delta).clamp(0, max) as u16;
}

/// Move the branch-list cursor (clamped, like the table cursor).
fn select_branch_entry(state: &mut AppState, delta: i32) {
    if state.branches.is_empty() {
        return;
    }
    let last = (state.branches.len() - 1) as i32;
    state.branch_selected = (state.branch_selected as i32 + delta).clamp(0, last) as usize;
}

/// `Enter` on the branch list: filter the history to that ref (T2).
///
/// Public because the mouse router applies the filter on a click, the same
/// gesture the `Enter` key performs.
pub fn apply_branch_filter(state: &mut AppState, repo: &Path) {
    let Some(entry) = state.branches.get(state.branch_selected) else {
        return;
    };
    let refname = entry.refname.clone();
    let name = entry.label();
    match branches::reachable(repo, &refname) {
        Ok(set) => {
            state.branch_filter = Some(refname);
            state.branch_reachable = Some(set);
            state.status.last_action = Some(format!("history filtered to {name}"));
        }
        Err(error) => {
            state.status.error = Some(error.to_string());
            return;
        }
    }
    state.selected = state.visible_rows().first().copied().unwrap_or(0);
    state.scroll = 0;
    state.inline_diff = None;
}

/// `Esc` on the Branches tab: drop the filter and show all history again.
fn clear_branch_filter(state: &mut AppState) {
    state.branch_filter = None;
    state.branch_reachable = None;
    state.selected = 0;
    state.scroll = 0;
    state.inline_diff = None;
}

/// `u` — jump into the uncommitted section, or collapse it (T4).
fn toggle_uncommitted(state: &mut AppState) {
    if state.uncommitted.is_empty() {
        state.status.last_action = Some("No uncommitted changes.".to_string());
        return;
    }
    if state.uncommitted_selected.is_none() {
        state.uncommitted_selected = Some(0);
        state.uncommitted_open = true;
        return;
    }
    if state.uncommitted_open {
        state.uncommitted_open = false;
        state.uncommitted_selected = None;
        state.inline_diff = None;
    } else {
        state.uncommitted_open = true;
    }
}

/// Move the inline file-row cursor (T4): the details file list while `Tab` has
/// focused it, otherwise the uncommitted file list.
/// `Enter` on an uncommitted file row: open its diff inline (T4).
fn open_inline_uncommitted_diff(state: &mut AppState, repo: &Path) {
    let Some(index) = state.uncommitted_selected else {
        return;
    };
    let Some(file) = state.uncommitted.files.get(index) else {
        return;
    };
    let path = file.path.clone();
    let mut text = data::run_git(repo, &["diff", "HEAD", "--", &path]).unwrap_or_default();
    if text.trim().is_empty() {
        // Untracked file: diff the worktree copy against `/dev/null` so a new
        // file still gets a body instead of an empty pane.
        text = data::run_git(repo, &["diff", "--no-index", "--", "/dev/null", &path])
            .unwrap_or_default();
    }
    state.inline_diff = Some(InlineDiff {
        title: format!("git diff HEAD -- {path}"),
        lines: diff::parse(&text),
        source: DiffSource::Uncommitted(index),
    });
}

/// `Enter` on a details file row: open its diff inline under the commit (T5).
fn open_inline_file_diff(state: &mut AppState, repo: &Path) {
    let Some(panel) = state.cdv.as_ref() else {
        return;
    };
    let Some(path) = panel.selected_path() else {
        return;
    };
    let row = panel.row;
    let hash = panel.hash.clone();
    let text = data::run_git(repo, &["show", "--format=", "--patch", &hash, "--", &path])
        .unwrap_or_default();
    state.inline_diff = Some(InlineDiff {
        title: format!("git show {hash} -- {path}"),
        lines: diff::parse(&text),
        source: DiffSource::CommitFile { row, path },
    });
}

/// `e` — stage the selected file's two revisions and hand them to
/// `$GGT_EDITOR` (T6). `main.rs` runs [`AppState::pending_editor`] after the
/// frame, once the terminal is back to its normal mode.
fn stage_editor(state: &mut AppState, repo: &Path) {
    let Some(program) = state.editor_program.clone() else {
        state.status.last_action = Some("Set $GGT_EDITOR to open a diff editor.".to_string());
        return;
    };
    let Some(panel) = state.cdv.as_ref() else {
        state.status.last_action = Some("Open commit details first (d), then press e.".to_string());
        return;
    };
    let Some(path) = panel.selected_path() else {
        return;
    };
    let hash = panel.hash.clone();
    let old = data::run_git(repo, &["show", &format!("{hash}^:{path}")]).unwrap_or_default();
    let new = data::run_git(repo, &["show", &format!("{hash}:{path}")]).unwrap_or_default();
    match editor::stage(&program, &path, &old, &new) {
        Ok(plan) => {
            state.status.last_action = Some(format!("opening {path}: {}", plan.command_line()));
            state.pending_editor = Some(plan);
        }
        Err(error) => state.status.error = Some(error.to_string()),
    }
}

/* ------------------------------------------------------------------ */
/* W6/T3 — the commit details view                                     */
/* ------------------------------------------------------------------ */

/// Load the commit details view for the selected row.
///
/// The reference fetches the details asynchronously and paints a spinner first
/// (`web/main.ts:2536`). The terminal port has no async runtime, so the loading
/// state exists but is filled in before the next frame; the label logic is
/// still exercised because [`CdvState::loading`] is what the panel starts as.
pub fn open_cdv(state: &mut AppState, repo: &Path) {
    let row = state.selected;
    let Some(commit) = state.rows.get(row as usize) else {
        return;
    };
    let hash = commit.hash.clone();
    let mut cdv = CdvState::loading(row, &hash, CdvKind::Commit, None);
    match files::commit_details(repo, &hash) {
        Ok(detail) => match files::commit_file_changes(repo, &hash) {
            Ok(changes) => cdv.loaded(detail, changes),
            Err(error) => cdv.failed(&error.to_string()),
        },
        Err(error) => cdv.failed(&error.to_string()),
    }
    cdv.external_diff = files::external_diff_tool(repo);
    state.cdv = Some(cdv);
}

/* ------------------------------------------------------------------ */
/* W7/T4 — the uncommitted and comparison CDV kinds                    */
/* ------------------------------------------------------------------ */

/// `u` — open the uncommitted-changes CDV (VIS-039).
///
/// The reference inserts a virtual uncommitted row at the top of the table only
/// when the working tree has changes (`src/dataSource.ts:189-192`). `data.rs`
/// does not model that row, so the panel is anchored at row `0` — the position
/// the virtual row would take — and an empty working tree leaves the panel
/// closed and says so, which is the same precondition the reference row has.
pub fn open_uncommitted_cdv(state: &mut AppState, repo: &Path) {
    let changes = match files::working_tree_changes(repo) {
        Ok(changes) => changes,
        Err(error) => {
            state.status.last_action = Some(format!("uncommitted changes: {error}"));
            return;
        }
    };
    if changes.is_empty() {
        state.status.last_action = Some("No uncommitted changes.".to_string());
        state.dialog = Some(Dialog::message(
            "Uncommitted Changes",
            vec!["There are no uncommitted changes to display.".to_string()],
        ));
        return;
    }
    let mut cdv = CdvState::uncommitted(0, Vec::new());
    cdv.loaded_changes(changes);
    cdv.external_diff = files::external_diff_tool(repo);
    state.cdv = Some(cdv);
    state.cdv_focused = true;
}

/// `c` — mark the selected commit as the comparison base, or open the
/// comparison when the base is already marked (VIS-040).
///
/// The reference sets `compareWithHash` from the commit menu and opens the
/// comparison on the second commit; the terminal port keeps the base in
/// [`AppState::compare_base`] so the same two-step gesture works from the
/// keyboard. Pressing `c` twice on the same row clears the mark.
pub fn compare_mark(state: &mut AppState, repo: &Path) {
    let row = state.selected;
    if state.compare_base == Some(row) {
        state.compare_base = None;
        state.status.last_action = Some("comparison cleared".to_string());
        return;
    }
    match state.compare_base.take() {
        None => {
            state.compare_base = Some(row);
            state.status.last_action = Some(format!("marked row {} for comparison", row + 1));
        }
        Some(base) => open_comparison_cdv(state, repo, base, row),
    }
}

/// Load the comparison CDV for two rows.
///
/// `getCommitOrder` (`web/main.ts:2843-2849`) makes the **older** commit the
/// diff's `from` side; in table order the older commit is the larger row index.
fn open_comparison_cdv(state: &mut AppState, repo: &Path, a: RowIdx, b: RowIdx) {
    let (Some(left), Some(right)) = (
        state.rows.get(a as usize).map(|c| c.hash.clone()),
        state.rows.get(b as usize).map(|c| c.hash.clone()),
    ) else {
        return;
    };
    let (from, to) = if a > b { (left, right) } else { (right, left) };
    let mut cdv = CdvState::comparison(state.selected, &from, &to, Vec::new());
    match files::comparison_changes(repo, &from, &to) {
        Ok(changes) => cdv.loaded_changes(changes),
        Err(error) => cdv.failed(&error.to_string()),
    }
    cdv.external_diff = files::external_diff_tool(repo);
    state.cdv = Some(cdv);
    state.cdv_focused = true;
    state.status.last_action = Some(format!("comparing {from} to {to}"));
}

/// Close the panel when it belongs to `row` (collapsing that row).
fn close_cdv_for(state: &mut AppState, row: RowIdx) {
    if state.cdv.as_ref().map(|cdv| cdv.row) == Some(row) {
        state.cdv = None;
        state.cdv_focused = false;
    }
}

/* ------------------------------------------------------------------ */
/* W7/T3 — the CDV file-row cursor and the A65–A68 file actions        */
/* ------------------------------------------------------------------ */

/// The text rows the inline panel can show, for the cursor's scroll window.
///
/// The panel is drawn inline under its commit row (T3), so its viewport is the
/// height [`crate::cdv::inline_height`] gives it for the current body — minus the
/// panel's two border rows.
fn cdv_file_viewport(state: &AppState) -> u16 {
    state
        .cdv
        .as_ref()
        .map(|cdv| crate::cdv::inline_height(cdv, state.status.height.max(1)).saturating_sub(2))
        .unwrap_or(0)
}

/// Move the CDV file-row cursor by `delta` (W7/T3).
fn cdv_move_file(state: &mut AppState, delta: i32) {
    let viewport = cdv_file_viewport(state);
    if let Some(cdv) = state.cdv.as_mut() {
        cdv.move_selection(delta);
        cdv.ensure_file_visible(viewport);
    }
}

/// Collapse (`open = false`) or expand (`open = true`) the folder under the
/// cursor (W7/T3).
fn cdv_set_folder(state: &mut AppState, open: bool) {
    let viewport = cdv_file_viewport(state);
    if let Some(cdv) = state.cdv.as_mut() {
        cdv.set_folder_open_at_cursor(open);
        cdv.ensure_file_visible(viewport);
    }
}

/// Toggle a folder row in the inline CDV by path (INT-050, W10).
///
/// The reference binds a click on `.fileTreeFolder` to the same open/closed flip
/// `Left`/`Right` perform on the cursor (`web/main.ts:2968-2978`). Unlike a file
/// row this does not move the cursor — `CdvState::select_path` is file-only, and
/// the click names the folder directly.
pub fn toggle_cdv_folder(state: &mut AppState, path: &str) {
    let viewport = cdv_file_viewport(state);
    if let Some(cdv) = state.cdv.as_mut() {
        cdv.toggle_folder(path);
        cdv.ensure_file_visible(viewport);
    }
}

/// Toggle the reviewed flag on the row under the cursor (VIS-049, W7/T3).
fn cdv_toggle_reviewed(state: &mut AppState) {
    if let Some(cdv) = state.cdv.as_mut() {
        if let Some(path) = cdv.selected_path() {
            cdv.toggle_reviewed(&path);
        }
    }
}

/// Build the [`actions::ActionTarget`] for the CDV row under the cursor.
///
/// `from` is the panel's commit (or the comparison's left side) and `to` is the
/// comparison's right side, or `UNCOMMITTED` for a plain commit / working tree —
/// exactly the pair `getCommitHashForFile` feeds to `viewDiff`
/// (`web/main.ts:2880-2885`).
fn cdv_file_target(state: &AppState) -> Option<actions::ActionTarget> {
    let cdv = state.cdv.as_ref()?;
    let change = cdv.selected_change()?;
    let from = cdv.hash.clone();
    let to = cdv
        .compare_with
        .clone()
        .unwrap_or_else(|| crate::data::UNCOMMITTED.to_string());
    Some(actions::ActionTarget {
        path: change.new_path.clone(),
        hash: from.clone(),
        commit: from.clone(),
        from,
        to,
        ..actions::ActionTarget::default()
    })
}

/// Run `action`'s plan for the CDV row under the cursor and page the output.
///
/// A terminal has no editor, so the R3 tie-break puts the action + data here and
/// leaves the editor chrome out: the pager shows exactly what git printed, with
/// the argv git ran as its title.
fn cdv_open_pager(state: &mut AppState, repo: &Path, action: RefAction) {
    let Some(target) = cdv_file_target(state) else {
        return;
    };
    let plan = actions::plan(action, &target);
    let title = format!("git {}", plan.argv.join(" "));
    match actions::dispatch(repo, action, &target) {
        DispatchOutcome::Executed { stdout, .. } => {
            state.pager = Some(crate::pager::Pager::new(title, &stdout));
        }
        other => state.status.last_action = Some(other.status_line()),
    }
}

/// Run the gated `Reset File to this Revision` on the CDV row — `R` (A71, W10).
///
/// `git checkout <hash> -- <path>` overwrites a working file, so it takes the
/// same two-step route as the menu row: [`actions::dispatch`] refuses the
/// `Confirm` plan, and the W6 dialog it raises is the only path to
/// `execute_confirmed`.
fn cdv_reset_file(state: &mut AppState, repo: &Path) {
    let Some(target) = cdv_file_target(state) else {
        return;
    };
    let action = RefAction::ResetFileToRevision;
    match actions::dispatch(repo, action, &target) {
        DispatchOutcome::NeedsConfirm(plan) => {
            if open_dialog_for_plan(state, action, &target, &plan) {
                state.status.last_action = Some(format!("confirm: {}", plan.label));
            }
        }
        other => state.status.last_action = Some(other.status_line()),
    }
}

/// Copy the selected CDV file's path — `y` is absolute (A72), `Y` relative
/// (A73, W10).
///
/// The menu distinguishes the two rows only by the payload it hands
/// `copyFilePath`; the keys reuse that same seam, so both still emit DSP-011.
fn cdv_copy_path(state: &mut AppState, repo: &Path, absolute: bool) {
    let Some(mut target) = cdv_file_target(state) else {
        return;
    };
    let text = cdv_copy_text(repo, &target.path, absolute);
    target.clipboard = text.clone();
    let outcome = actions::dispatch(repo, RefAction::CopyFilePath, &target);
    // The same OSC 52 bridge the menu rows feed (`main.rs` emits it).
    state.pending_clipboard = Some(text);
    state.status.last_action = Some(outcome.status_line());
}

/// The text `y`/`Y` put on the clipboard for a CDV file row (A72/A73, W10).
///
/// Absolute is the repo joined to the row's repo-relative path; relative is the
/// row's path untouched. This is the only difference between the two rows.
fn cdv_copy_text(repo: &Path, path: &str, absolute: bool) -> String {
    if absolute {
        repo.join(path).to_string_lossy().into_owned()
    } else {
        path.to_string()
    }
}

/* ------------------------------------------------------------------ */
/* W6/T1 — the find widget                                             */
/* ------------------------------------------------------------------ */

/// Rescan every row, keeping the current match if it survives
/// (`findWidget.ts:344-351`).
pub fn recompute_find(state: &mut AppState) {
    let previous = state.find.current_hash().map(str::to_string);
    let uncommitted = None;
    state.find.recompute(
        &state.rows,
        &state.columns,
        uncommitted,
        previous.as_deref(),
    );
}

/// Move to the next/previous match and follow it with the selection.
fn find_step(state: &mut AppState, forward: bool) {
    let row = if forward {
        state.find.next()
    } else {
        state.find.prev()
    };
    if let Some(row) = row {
        state.selected = row as RowIdx;
    }
}

/// When `findOpenCommitDetailsView` is on, load the CDV for the current match
/// (`findWidget.ts:368-378`). When it is turned off, close the panel the find
/// widget opened.
fn follow_find_cdv(state: &mut AppState, repo: &Path) {
    if !state.find.open_cdv {
        state.cdv = None;
        state.cdv_focused = false;
        return;
    }
    if let Some(row) = state.find.current_row() {
        state.selected = row as RowIdx;
        open_cdv(state, repo);
    }
}

/// Close the widget and put the selection back where it was before `Ctrl+F`
/// (`findWidget.ts:139-158` `restoreState`; W7/T2 acceptance).
fn close_find(state: &mut AppState) {
    state.find.close();
    if let Some(row) = state.selection_before_find.take() {
        if (row as usize) < state.rows.len() {
            state.selected = row;
        }
    }
}

/// Route one keypress to the open find widget (W7/T2).
///
/// Returns `true` when the widget consumed the key. This is the runtime half of
/// [`FindWidget::push_char`] — W6 shipped the model, W7 gives it the keyboard.
///
/// `Escape` goes through [`reduce`] so the action layer stays the single place
/// that closes the widget; the other keys edit the query, which is not an
/// [`Action`].
///
/// Reference: `web/findWidget.ts:300-320` (the `keydown` listener) for the
/// `Enter`/`Shift+Enter` step keys and `Escape` close.
pub fn find_key(state: &mut AppState, key: crate::keymap::FindKey, repo: &Path) -> bool {
    use crate::keymap::FindKey;
    if !state.find.visible {
        return false;
    }
    match key {
        FindKey::Char(c) => {
            state.find.push_char(c);
            recompute_find(state);
        }
        FindKey::Backspace => {
            state.find.pop_char();
            recompute_find(state);
        }
        FindKey::Next => find_step(state, true),
        FindKey::Prev => find_step(state, false),
        FindKey::Close => {
            reduce(state, Action::FindClose, repo);
        }
    }
    state.sync_status();
    state.ensure_visible();
    true
}

/// Route one keypress to the open pager (W7/T3).
///
/// Returns `true` when the pager consumed the key. `viewport` is the number of
/// text rows the overlay draws, so paging and the scroll clamp match the screen.
pub fn pager_nav(state: &mut AppState, nav: PagerNav, viewport: u16) -> bool {
    if nav == PagerNav::Close {
        let was_open = state.pager.is_some();
        state.pager = None;
        return was_open;
    }
    let Some(pager) = state.pager.as_mut() else {
        return false;
    };
    match nav {
        PagerNav::LineUp => pager.scroll_by(-1, viewport),
        PagerNav::LineDown => pager.scroll_by(1, viewport),
        PagerNav::PageUp => pager.page_by(-1, viewport),
        PagerNav::PageDown => pager.page_by(1, viewport),
        PagerNav::Top => pager.top(),
        PagerNav::Bottom => pager.bottom(viewport),
        PagerNav::Close => unreachable!("handled above"),
    }
    true
}

/* ------------------------------------------------------------------ */
/* W6/T2 — the modal dialog                                            */
/* ------------------------------------------------------------------ */

/// Route one keypress to the open dialog, returning what it decided.
///
/// This is the second half of the W5→W6 two-step rule: the dialog never runs
/// git itself. [`Action::DialogConfirm`] does, and only after the user
/// submitted.
pub fn dialog_key(state: &mut AppState, key: DialogKey) -> Option<DialogResult> {
    let dialog = state.dialog.as_mut()?;
    dialog.handle(key)
}

/// Run the plan the user confirmed, then close the dialog.
///
/// Only the dialog's submit button reaches [`actions::execute_confirmed`], and
/// only a `PlanKind::Confirm` plan is accepted — so cancelling is provably a
/// no-op. The result lands in `status.last_action` for the status bar.
fn confirm_dialog(state: &mut AppState, repo: &Path) {
    let Some(dialog) = state.dialog.as_ref() else {
        return;
    };
    // A form that fails validation must not close; the error stays on screen.
    let button = dialog.focused_button().unwrap_or(0);
    let Some(plan) = dialog.plan_for_submit(button) else {
        state.dialog = None;
        state.status.last_action = Some("cancelled".to_string());
        return;
    };
    let outcome = actions::execute_confirmed(repo, &plan);
    let line = outcome.status_line();
    state.dialog = None;
    state.status.last_action = Some(line);
    if outcome.plan().mutates() {
        reduce(state, Action::Refresh, repo);
    }
}

/// Open the dialog a gated plan needs, if the dialog layer knows how to build
/// one for it.
///
/// Returns `true` when a dialog was opened, so the caller knows the two-step
/// gate is now in the user's hands rather than stubbed.
pub fn open_dialog_for_plan(
    state: &mut AppState,
    action: RefAction,
    target: &actions::ActionTarget,
    plan: &actions::ActionPlan,
) -> bool {
    let actions::PlanKind::Confirm(reason) = plan.kind else {
        return false;
    };
    state.dialog = Some(dialog::dialog_for_plan(action, target, plan, reason));
    true
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
    state.menu = None;
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
    use crate::branches::RefKind;
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
            crate::uncommitted::UncommittedState::default(),
            Vec::new(),
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
        state.status.height = 10; // 10 visible rows
        assert_eq!(state.scroll, 0);

        // Row 0 carries a refs line (T7), so the history is 31 display rows and
        // row 10 ends at display row 12 — one row past the window, not two.
        state.selected = 9;
        reduce(&mut state, Action::SelectNext, Path::new("."));
        assert_eq!(state.selected, 10);
        assert_eq!(state.scroll, 2, "row 10 scrolls the window to its bottom");

        reduce(&mut state, Action::Bottom, Path::new("."));
        assert_eq!(state.scroll, 31 - 10, "anchors the last row to the bottom");

        reduce(&mut state, Action::Top, Path::new("."));
        assert_eq!(state.scroll, 0);
    }

    #[test]
    fn expanded_rows_reserve_the_reference_gap() {
        let mut state = state_of(linear(5));
        // Row 0 carries the `HEAD -> main` ref, which now gets its own line (T7).
        assert_eq!(state.row_height(0), 2);
        assert_eq!(state.display_rows(), 6);

        state.expanded.insert(1);
        assert_eq!(state.row_height(1), 1 + EXPANDED_EXTRA_ROWS);
        assert_eq!(state.row_top(0), 0);
        assert_eq!(state.row_top(1), 2, "row 0 is `refs + commit` tall");
        assert_eq!(state.row_top(2), 2 + (1 + EXPANDED_EXTRA_ROWS));
        assert_eq!(state.display_rows(), 6 + EXPANDED_EXTRA_ROWS);
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

    // ---- W5: context menus, column state and mouse routing ----------------

    /// A three-row history with a local branch, a remote branch and a tag, so
    /// every ref surface has something to open on.
    ///
    /// Note `feature` (no slash): [`classify_ref`] has no remote list to consult,
    /// so it reads any `a/b` decoration as a remote-tracking ref.
    fn refs_state() -> AppState {
        let commits = vec![
            commit("c0", &["c1"], &["HEAD -> main", "origin/main"]),
            commit("c1", &["c2"], &["feature"]),
            commit("c2", &[], &["tag: v1.0"]),
        ];
        // `state_of` reports branch `main`, which is also the HEAD decoration.
        state_of(commits)
    }

    /// A 100x9 main area: a 4-column graph pane plus the one-row table header.
    fn geometry() -> HitGeometry {
        HitGeometry {
            body: ratatui::layout::Rect {
                x: 0,
                y: 0,
                width: 100,
                height: 9,
            },
            graph_width: 4,
            has_header: true,
            cdv: None,
            // The hand-made geometry has no tab bar or branch pane, so no cell
            // belongs to either surface.
            tab_bar: ratatui::layout::Rect {
                x: 0,
                y: 0,
                width: 100,
                height: 0,
            },
            branch_pane: None,
        }
    }

    #[test]
    fn column_state_starts_at_the_p0_default() {
        let columns = ColumnState::default();
        assert!(columns.is_visible(ColumnId::Date));
        assert!(columns.is_visible(ColumnId::Author));
        assert!(columns.is_visible(ColumnId::Commit));
        assert_eq!(columns.order, CommitOrder::Date);
        assert_eq!(columns.order.git_flag(), "--date-order");
    }

    #[test]
    fn toggling_a_column_flips_only_that_column() {
        let mut columns = ColumnState::default();
        assert!(!columns.toggle(ColumnId::Author));
        assert!(!columns.author);
        assert!(columns.date && columns.commit);
        assert!(columns.toggle(ColumnId::Author));
        assert_eq!(column_label(ColumnId::Author), "Author");
    }

    #[test]
    fn has_remotes_reads_the_refs() {
        assert!(refs_state().has_remotes());
        assert!(!state_of(linear(2)).has_remotes());
    }

    #[test]
    fn the_column_header_menu_reflects_and_sets_the_column_state() {
        let mut state = refs_state();
        state.columns.author = false;
        state.open_menu(MenuSurface::ColumnHeader, None, 20, 0);
        let menu = state.menu.as_ref().expect("menu open");
        // The checkmark on each toggle row mirrors the current visibility.
        let checked: Vec<(&str, bool)> = menu
            .items
            .iter()
            .filter(|item| matches!(item.action, MenuAction::ToggleColumn(_)))
            .map(|item| (item.label, item.checked))
            .collect();
        assert!(
            checked.contains(&("Author", false)),
            "author unticked: {checked:?}"
        );
        assert!(
            checked.contains(&("Date", true)),
            "date ticked: {checked:?}"
        );
        assert!(
            checked.contains(&("Commit", true)),
            "commit ticked: {checked:?}"
        );

        // Activating the highlighted row is a local state change, not git.
        assert!(matches!(
            menu.items[menu.selected].action,
            MenuAction::ToggleColumn(ColumnId::Date)
        ));
        let line = state.activate_menu(Path::new("."));
        assert!(!state.columns.date, "the first row hid the date column");
        assert!(line.contains("Date column: hidden"), "{line}");
        assert!(state.status.last_action.as_deref() == Some(line.as_str()));
        assert!(state.menu.is_none(), "activating closes the menu");
    }

    #[test]
    fn opening_on_a_branch_row_seeds_the_branch_context() {
        let mut state = refs_state();
        state.selected = 1; // `feature`
        let surface = state.open_menu(MenuSurface::Branch, Some("feature"), 10, 3);
        assert_eq!(surface, MenuSurface::Branch);
        let menu = state.menu.as_ref().expect("menu open");
        assert_eq!(menu.context.ref_name, "feature");
        assert!(!menu.context.is_current_branch);
        assert!(menu.context.has_remotes);
        assert_eq!(menu.selected, menu.first_enabled());
    }

    #[test]
    fn the_current_branch_is_flagged_for_the_menu() {
        let mut state = refs_state();
        state.selected = 0; // `HEAD -> main`
        state.open_menu(MenuSurface::Branch, Some("main"), 10, 0);
        assert!(state.menu.as_ref().unwrap().context.is_current_branch);
    }

    #[test]
    fn a_commit_row_menu_has_no_ref_name() {
        let mut state = refs_state();
        state.selected = 2;
        state.open_menu(MenuSurface::CommitRow, None, 10, 4);
        let ctx = &state.menu.as_ref().unwrap().context;
        assert_eq!(ctx.hash, "c2");
        assert_eq!(ctx.subject, "commit c2");
        assert!(ctx.ref_name.is_empty(), "the row menu is about the commit");
        assert!(ctx.branch.is_empty());
    }

    #[test]
    fn the_uncommitted_menu_targets_head() {
        let mut state = refs_state();
        state.open_menu(MenuSurface::Uncommitted, None, 10, 1);
        let ctx = &state.menu.as_ref().unwrap().context;
        assert_eq!(ctx.hash, "HEAD");
        assert!(ctx.selector.is_empty());
    }

    #[test]
    fn open_menu_for_selection_picks_the_ref_surface() {
        let mut state = refs_state();
        state.selected = 1;
        assert_eq!(
            state.open_menu_for_selection(0, 1),
            Some(MenuSurface::Branch)
        );
        assert_eq!(state.menu.as_ref().unwrap().context.ref_name, "feature");
        state.selected = 2;
        assert_eq!(state.open_menu_for_selection(0, 1), Some(MenuSurface::Tag));
        assert_eq!(state.menu.as_ref().unwrap().context.ref_name, "v1.0");
        state.selected = 0;
        assert_eq!(
            state.open_menu_for_selection(0, 1),
            Some(MenuSurface::Branch)
        );
        assert_eq!(state.menu.as_ref().unwrap().context.ref_name, "main");
    }

    #[test]
    fn open_menu_for_selection_is_none_on_an_empty_repo() {
        let mut state = state_of(Vec::new());
        assert_eq!(state.open_menu_for_selection(0, 1), None);
        assert!(!state.menu_is_open());
    }

    #[test]
    fn menu_navigation_wraps_and_skips_nothing() {
        let mut state = refs_state();
        state.open_menu(MenuSurface::ColumnHeader, None, 20, 0);
        let len = state.menu.as_ref().unwrap().items.len();
        assert_eq!(state.menu_nav(MenuNav::Up), MenuKey::Moved);
        assert_eq!(state.menu.as_ref().unwrap().selected, len - 1, "wraps up");
        assert_eq!(state.menu_nav(MenuNav::Down), MenuKey::Moved);
        assert_eq!(state.menu.as_ref().unwrap().selected, 0, "wraps down");
        assert_eq!(state.menu_nav(MenuNav::Close), MenuKey::Closed);
        assert!(!state.menu_is_open());
    }

    #[test]
    fn menu_navigation_ignores_keys_with_no_menu_open() {
        let mut state = refs_state();
        assert_eq!(state.menu_nav(MenuNav::Down), MenuKey::Ignored);
        assert_eq!(state.menu_nav(MenuNav::Activate), MenuKey::Ignored);
    }

    #[test]
    fn a_disabled_row_never_activates() {
        let mut state = refs_state();
        state.open_menu(MenuSurface::Branch, Some("feature"), 10, 3);
        let disabled = state
            .menu
            .as_ref()
            .unwrap()
            .items
            .iter()
            .position(|item| !item.enabled);
        let Some(index) = disabled else {
            return; // nothing greyed out for this surface; nothing to prove
        };
        state.menu.as_mut().unwrap().selected = index;
        assert_eq!(state.menu_nav(MenuNav::Activate), MenuKey::Ignored);
        assert!(state.menu_is_open(), "a dead row leaves the menu up");
        assert!(state.activate_menu(Path::new(".")).contains("disabled"));
        assert!(state.menu_is_open());
    }

    #[test]
    fn a_shortcut_letter_activates_its_row() {
        let mut state = refs_state();
        state.open_menu(MenuSurface::Tag, Some("v1.0"), 10, 4);
        let (key, id) = {
            let menu = state.menu.as_ref().unwrap();
            let item = menu
                .items
                .iter()
                .find(|item| item.shortcut.is_some() && item.enabled)
                .expect("at least one shortcut row");
            (item.shortcut.unwrap(), item.id)
        };
        assert_eq!(state.menu_nav(MenuNav::Shortcut(key)), MenuKey::Activated);
        let menu = state.menu.as_ref().unwrap();
        assert_eq!(menu.items[menu.selected].id, id);
    }

    #[test]
    fn copy_hash_reaches_the_clipboard_bridge() {
        let mut state = refs_state();
        state.selected = 0;
        state.open_menu(MenuSurface::CommitRow, None, 10, 0);
        let index = state
            .menu
            .as_ref()
            .unwrap()
            .items
            .iter()
            .position(|item| item.id == "A32")
            .expect("Copy Commit Hash row");
        state.menu.as_mut().unwrap().selected = index;
        // Hermetic: point the fallback file at this test's own path while the
        // shared env lock is held (`actions`' clipboard tests mutate it too).
        let _guard = crate::actions::ENV_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let path =
            std::env::temp_dir().join(format!("ggt-w5-menu-clip-{}.txt", std::process::id()));
        std::env::set_var("GGT_CLIPBOARD_FILE", &path);
        let line = state.activate_menu(Path::new("."));
        std::env::remove_var("GGT_CLIPBOARD_FILE");
        assert_eq!(state.pending_clipboard.as_deref(), Some("c0"));
        assert!(line.contains("copied to"), "{line}");
        assert_eq!(
            std::fs::read_to_string(&path).expect("clipboard file"),
            "c0"
        );
        let _ = std::fs::remove_file(&path);
        assert!(!state.menu_is_open());
    }

    #[test]
    fn a_gated_row_stops_at_the_confirmation_stub() {
        let mut state = refs_state();
        state.open_menu(MenuSurface::Branch, Some("feature"), 10, 3);
        let index = state
            .menu
            .as_ref()
            .unwrap()
            .items
            .iter()
            .position(|item| matches!(item.action, MenuAction::Dispatch(RefAction::DeleteBranch)))
            .expect("Delete Branch row");
        state.menu.as_mut().unwrap().selected = index;
        let before = state.rows.clone();
        let line = state.activate_menu(Path::new("/nonexistent/ggt-w5"));
        // W6/T2: the gated row now opens the confirmation dialog instead of
        // stopping at the W5 stub, so nothing runs until the user submits.
        assert!(line.contains("confirm: Delete Branch"), "{line}");
        let dialog = state.dialog.as_ref().expect("the confirmation dialog");
        assert!(dialog.pending.is_some(), "the plan waits for the user");
        assert_eq!(state.rows, before, "a gated plan never runs git");
        assert!(state.pending_clipboard.is_none());
    }

    #[test]
    fn local_rows_stub_through_the_status_line() {
        let mut state = refs_state();
        state.open_menu(MenuSurface::Branch, Some("feature"), 10, 3);
        let (dropdown, link) = {
            let menu = state.menu.as_ref().unwrap();
            let dropdown = menu
                .items
                .iter()
                .position(|item| matches!(item.action, MenuAction::SelectInDropdown(_)));
            let link = menu
                .items
                .iter()
                .position(|item| matches!(item.action, MenuAction::FollowInternalLink(_)));
            (dropdown, link)
        };
        if let Some(index) = dropdown {
            state.menu.as_mut().unwrap().selected = index;
            assert!(state.activate_menu(Path::new(".")).contains("dropdown"));
            state.open_menu(MenuSurface::Branch, Some("feature"), 10, 3);
        }
        if let Some(index) = link {
            state.menu.as_mut().unwrap().selected = index;
            assert!(state
                .activate_menu(Path::new("."))
                .contains("internal link"));
        }
    }

    #[test]
    fn column_order_rows_switch_the_git_flag() {
        let mut state = refs_state();
        state.open_menu(MenuSurface::ColumnHeader, None, 20, 0);
        // Pick a radio row that is *not* the order we are already using.
        let index = state
            .menu
            .as_ref()
            .unwrap()
            .items
            .iter()
            .position(|item| {
                matches!(
                    item.action,
                    MenuAction::SetColumnOrder(order) if order != state.columns.order
                )
            })
            .expect("a different ordering row");
        let expected = match state.menu.as_ref().unwrap().items[index].action {
            MenuAction::SetColumnOrder(order) => order,
            _ => unreachable!(),
        };
        state.menu.as_mut().unwrap().selected = index;
        let line = state.activate_menu(Path::new("."));
        assert_eq!(state.columns.order, expected);
        assert!(line.contains(expected.git_flag()), "{line}");
    }

    #[test]
    fn activate_menu_without_a_menu_is_a_no_op() {
        let mut state = refs_state();
        assert!(state
            .activate_menu(Path::new("."))
            .contains("no context menu"));
    }

    #[test]
    fn row_at_display_maps_every_display_row() {
        let mut state = state_of(linear(3));
        // Row 0 owns a refs line plus its commit line (T7), so its block is two
        // display rows tall and every later row shifts down by one.
        assert_eq!(state.row_height(0), 2);
        assert_eq!(state.row_at_display(0), Some(0));
        assert_eq!(state.row_at_display(1), Some(0));
        assert_eq!(state.row_at_display(2), Some(1));
        assert_eq!(state.row_at_display(3), Some(2));
        assert_eq!(state.row_at_display(4), None);
        state.expanded.insert(0);
        // Row 0 now spans display rows 0..=1 + EXPANDED_EXTRA_ROWS.
        assert_eq!(state.row_at_display(1 + EXPANDED_EXTRA_ROWS), Some(0));
        assert_eq!(state.row_at_display(2 + EXPANDED_EXTRA_ROWS), Some(1));
    }

    #[test]
    fn surface_at_routes_the_header_row() {
        let state = refs_state();
        let hit = geometry();
        // Column 0 is the graph; the table header is only right of it.
        assert_eq!(state.surface_at(0, 0, &hit), None);
        assert_eq!(
            state.surface_at(10, 0, &hit),
            Some(ClickTarget::ColumnHeader)
        );
    }

    /* -------------------------------------------------------------- */
    /* M4-W12 — the tab bar and the branch pane as click surfaces      */
    /* -------------------------------------------------------------- */

    /// A state on the Branches tab with a few refs of every kind, plus the
    /// geometry `main.rs` computes for a 100x24 screen.
    fn branches_geometry() -> (AppState, HitGeometry) {
        let mut state = state_of(linear(4));
        state.tab = Tab::Branches;
        state.branches = vec![
            branch_entry("main", RefKind::Local, true),
            branch_entry("dev", RefKind::Local, false),
            branch_entry("origin/main", RefKind::Remote, false),
            branch_entry("v1.0", RefKind::Tag, false),
        ];
        state.status.width = 100;
        state.status.height = 22;
        let view = view_of(&state);
        let hit = crate::render::hit_geometry(&state, &view, 24).expect("geometry");
        (state, hit)
    }

    fn branch_entry(name: &str, kind: RefKind, is_head: bool) -> BranchEntry {
        BranchEntry {
            name: name.to_string(),
            refname: format!("refs/{name}"),
            kind,
            is_head,
            count: 3,
        }
    }

    /// A click on a tab label switches tabs, and the body no longer answers for
    /// a cell that belongs to the bar (T1/T7).
    #[test]
    fn surface_at_reads_the_tab_bar_before_the_body() {
        let (state, hit) = branches_geometry();
        assert_eq!(hit.tab_bar.height, 1, "the bar is always drawn");
        for (tab, span) in crate::render::tab_spans() {
            let x = span.start;
            assert_eq!(state.surface_at(x, 0, &hit), Some(ClickTarget::Tab(tab)));
        }
        // The separator between the labels is not a tab, and must not fall
        // through to the body either.
        assert_eq!(state.surface_at(0, 0, &hit), None);
        // The body still answers normally one row below the bar. Row 0 is the
        // table header, so the first commit row is `body.y + 1`.
        assert!(matches!(
            state.surface_at(hit.body.x, hit.body.y + 1, &hit),
            Some(ClickTarget::Row(..))
        ));
    }

    /// The left pane is its own surface: a cell inside it is a branch row, and a
    /// cell just right of it is a commit row (T2).
    #[test]
    fn surface_at_routes_the_branch_pane_before_the_body() {
        let (state, hit) = branches_geometry();
        let pane = hit.branch_pane.expect("the Branches tab has a pane");
        assert_eq!(pane.width, crate::render::BRANCH_PANE_WIDTH);
        assert_eq!(hit.body.x, pane.width, "the body starts after the pane");
        // The pane's rows are the group headings and the entries in order:
        // `Local`, main, dev, `Remote`, origin/main, `Tag`, v1.0.
        assert_eq!(state.surface_at(1, pane.y, &hit), None, "the heading");
        assert_eq!(
            state.surface_at(1, pane.y + 1, &hit),
            Some(ClickTarget::Branch(0))
        );
        assert_eq!(
            state.surface_at(1, pane.y + 2, &hit),
            Some(ClickTarget::Branch(1))
        );
        assert_eq!(state.surface_at(1, pane.y + 3, &hit), None, "the heading");
        assert_eq!(
            state.surface_at(1, pane.y + 4, &hit),
            Some(ClickTarget::Branch(2)),
            "the remote entry sits under its own heading"
        );
        // Past the last row.
        assert_eq!(state.surface_at(1, pane.y + 20, &hit), None);
        // Right of the pane, the body answers again.
        assert!(matches!(
            state.surface_at(pane.width + 1, pane.y + 1, &hit),
            Some(ClickTarget::Row(..))
        ));
    }

    /// On the Overview tab there is no pane, so the same cell is a commit row.
    #[test]
    fn the_overview_tab_has_no_branch_pane() {
        let (mut state, _) = branches_geometry();
        state.tab = Tab::Overview;
        let hit = crate::render::hit_geometry(&state, &view_of(&state), 24).expect("geometry");
        assert!(hit.branch_pane.is_none());
        assert_eq!(hit.body.x, 0);
    }

    /// A narrow terminal drops the pane rather than squeezing the history (T2).
    #[test]
    fn a_narrow_terminal_drops_the_branch_pane() {
        let (mut state, _) = branches_geometry();
        state.status.width = 50;
        let hit = crate::render::hit_geometry(&state, &view_of(&state), 24).expect("geometry");
        assert!(hit.branch_pane.is_none());
        assert_eq!(hit.body.x, 0);
    }

    /// A click on a branch row moves the cursor onto it and filters the history
    /// to that ref — the same pair of effects the `Enter` key has (T2).
    #[test]
    fn a_branch_click_selects_and_filters() {
        // A real repo, because the filter asks git which commits the ref
        // reaches; a made-up refname would be refused instead of applied.
        let repo = temp_repo("branch-click");
        data::run_git(&repo, &["branch", "dev"]).expect("git branch dev");
        let mut state = state_of(linear(4));
        state.tab = Tab::Branches;
        state.branches = crate::branches::load(&repo).expect("branch list");
        state.status.width = 100;
        state.status.height = 22;
        let hit = crate::render::hit_geometry(&state, &view_of(&state), 24).expect("geometry");
        let pane = hit.branch_pane.expect("the pane");

        let dev = state
            .branches
            .iter()
            .position(|entry| entry.name == "dev")
            .expect("`dev` is in the list");
        let rows = crate::render::branch_rows(&state.branches);
        let row = rows
            .iter()
            .position(|row| *row == crate::render::BranchRow::Entry(dev))
            .expect("`dev` has a pane row");
        let y = pane.y + row as u16;
        assert_eq!(
            state.surface_at(1, y, &hit),
            Some(ClickTarget::Branch(dev)),
            "the pane row for `dev` resolves back to it"
        );

        state.branch_selected = dev;
        apply_branch_filter(&mut state, &repo);
        assert_eq!(
            state.branch_filter.as_deref(),
            Some("refs/heads/dev"),
            "the filter records the full refname"
        );
        assert_eq!(state.tab, Tab::Branches, "the click does not change tabs");
        assert!(
            state.status.error.is_none(),
            "a real ref filters instead of erroring: {:?}",
            state.status.error
        );
        let _ = fs::remove_dir_all(&repo);
    }

    /// The wheel walks the branch cursor over the pane and scrolls the history
    /// everywhere else, clamped at both ends (T7).
    #[test]
    fn the_wheel_scrolls_the_surface_under_the_pointer() {
        let mut state = state_of(linear(40));
        state.status.height = 10;
        // Over the history: a three-line step, clamped at zero.
        state.scroll = 0;
        wheel_scroll(&mut state, -WHEEL_LINES, false);
        assert_eq!(state.scroll, 0, "the top is a hard stop");
        wheel_scroll(&mut state, WHEEL_LINES, false);
        assert_eq!(state.scroll, WHEEL_LINES as u16);
        // Past the end: pinned to the last full screenful.
        wheel_scroll(&mut state, 10_000, false);
        let max = state.display_rows().saturating_sub(10);
        assert_eq!(state.scroll, max);

        // Over the pane: the cursor walks the branch list instead, one entry
        // per notch, and clamps at both ends.
        let (mut state, _) = branches_geometry();
        state.branch_selected = 0;
        wheel_scroll(&mut state, WHEEL_LINES, true);
        assert_eq!(state.branch_selected, 1);
        wheel_scroll(&mut state, -WHEEL_LINES, true);
        assert_eq!(state.branch_selected, 0);
        wheel_scroll(&mut state, -WHEEL_LINES, true);
        assert_eq!(state.branch_selected, 0, "clamped at the first branch");
        // Already at the end: another notch stays there.
        state.branch_selected = state.branches.len() - 1;
        wheel_scroll(&mut state, WHEEL_LINES, true);
        assert_eq!(
            state.branch_selected,
            state.branches.len() - 1,
            "clamped at the last branch"
        );
        // The same gesture on another tab still scrolls the history, because
        // there is no pane to walk. The viewport has to be shorter than the
        // history, or there is nothing to scroll.
        state.tab = Tab::Overview;
        state.status.height = 2;
        state.scroll = 0;
        wheel_scroll(&mut state, WHEEL_LINES, true);
        assert_eq!(state.scroll, WHEEL_LINES as u16);
    }

    #[test]
    fn surface_at_routes_rows_refs_and_empty_space() {
        let state = refs_state();
        let hit = geometry();
        // The header occupies screen row 0, so commit row `d` sits at row `1 + d`.
        assert_eq!(
            state.surface_at(0, 1, &hit),
            Some(ClickTarget::Row(
                0,
                MenuSurface::CommitRow,
                "c0".to_string()
            )),
            "the graph pane is the commit row"
        );
        // The refs column is the last 20 cells of a 100-wide body.
        let refs = state.surface_at(95, 1, &hit).expect("refs hit");
        match refs {
            ClickTarget::Row(0, surface, name) => {
                assert_eq!(name, "main", "the branch ref outranks the remote ref");
                assert_eq!(surface, MenuSurface::Branch);
            }
            other => panic!("expected a row hit, got {other:?}"),
        }
        // Past the last commit, and outside the body entirely. Row 0 owns two
        // display rows (refs + commit), so rows 1..3 are display rows 0..2.
        // Every row in `refs_state` carries refs, so each owns two display rows.
        assert_eq!(
            state.surface_at(0, 5, &hit),
            Some(ClickTarget::Row(
                2,
                MenuSurface::CommitRow,
                "c2".to_string()
            )),
            "the last commit sits on display row 4"
        );
        assert_eq!(state.surface_at(0, 7, &hit), None, "past the last commit");
        assert_eq!(state.surface_at(101, 1, &hit), None);
        assert_eq!(state.surface_at(0, 99, &hit), None);
    }

    #[test]
    fn surface_at_follows_the_scroll_offet() {
        let mut state = state_of(linear(6));
        let hit = geometry();
        state.scroll = 2;
        assert_eq!(
            state.surface_at(0, 1, &hit),
            Some(ClickTarget::Row(
                1,
                MenuSurface::CommitRow,
                "c1".to_string()
            )),
            "display row 2 is commit row 1, because row 0 owns two display rows"
        );
    }

    #[test]
    fn surface_at_on_a_row_with_no_refs_is_the_commit_menu() {
        let state = state_of(linear(3));
        let hit = geometry();
        // Row 0 owns display rows 0 (refs) and 1 (commit), so row 1 starts at
        // display row 2 — screen row 3 with the header.
        match state
            .surface_at(95, 3, &hit)
            .expect("row 1 in the refs column")
        {
            ClickTarget::Row(1, surface, name) => {
                assert_eq!(surface, MenuSurface::CommitRow);
                assert_eq!(name, "c1");
            }
            other => panic!("expected the commit menu, got {other:?}"),
        }
    }

    /* -------------------------------------------------------------- */
    /* M4-W12 — the inline CDV as a click surface                      */
    /* -------------------------------------------------------------- */

    /// A loaded, uncommitted CDV listing one modified file per path, plus the
    /// geometry that puts the panel inline under row 0 (M4-W12/T3).
    fn cdv_geometry(paths: &[&str]) -> (AppState, HitGeometry) {
        let changes = paths
            .iter()
            .map(|path| files::GitFileChange {
                old_path: (*path).to_string(),
                new_path: (*path).to_string(),
                status: files::GitFileStatus::Modified,
                additions: Some(1),
                deletions: Some(0),
                submodule: false,
            })
            .collect();
        let mut state = state_of(linear(4));
        // `main.rs` sizes the status bar to the screen minus the chrome rows.
        state.status.width = 100;
        state.status.height = 22;
        state.cdv = Some(CdvState::uncommitted(0, changes));
        state.cdv_focused = true;
        // The panel only shows under an expanded row (T3).
        state.expanded.insert(0);
        let view = crate::render::ViewOptions {
            graph: Some(&state.layout),
            ..crate::render::ViewOptions::TEXT_ONLY
        };
        let hit = crate::render::hit_geometry(&state, &view, 24).expect("geometry");
        (state, hit)
    }

    /// A column inside `cdv_geometry`'s inline panel (past the graph gutter).
    fn cdv_x(hit: &HitGeometry) -> u16 {
        hit.cdv.expect("the panel has a rect").x + 1
    }

    /// Screen row of file row `index` inside `cdv_geometry`'s panel.
    fn cdv_row_y(state: &AppState, hit: &HitGeometry, index: usize) -> u16 {
        let panel = state.cdv.as_ref().expect("the panel is open");
        let inner_top = hit.cdv.expect("the panel has a rect").y + 1;
        let header = panel.summary_lines().len() as u16 + 2;
        inner_top + header + index as u16
    }

    #[test]
    fn surface_at_reads_the_inline_cdv_as_file_rows() {
        let (state, hit) = cdv_geometry(&["src/lib.rs", "src/app.rs"]);
        let rows = state.cdv.as_ref().unwrap().file_rows();
        let folder = rows
            .iter()
            .position(|row| matches!(row.kind, files::FileRowKind::Folder { .. }))
            .expect("a tree view folds src/ into a folder row");
        let file = rows
            .iter()
            .position(|row| matches!(row.kind, files::FileRowKind::File(_)))
            .expect("two file rows");
        // A file row is the `FileRow` menu's subject, and the cell carries its
        // full path — the two must agree or the menu would act on another file.
        assert_eq!(
            state.surface_at(cdv_x(&hit), cdv_row_y(&state, &hit, file), &hit),
            Some(ClickTarget::CdvFile(rows[file].path.clone())),
            "a file row resolves to its own path"
        );
        // A folder row is its own target: it has no `FileRow` menu, but the
        // reference flips it open/closed on click, so it must not fall through
        // to the commit-row menu below the panel (INT-050).
        assert_eq!(
            state.surface_at(cdv_x(&hit), cdv_row_y(&state, &hit, folder), &hit),
            Some(ClickTarget::CdvFolder(rows[folder].path.clone())),
            "a folder row resolves to its own path"
        );
        // The border column and the empty space under the last row are not rows.
        assert!(
            !matches!(
                state.surface_at(hit.cdv.unwrap().x, cdv_row_y(&state, &hit, file), &hit),
                Some(ClickTarget::CdvFile(_)) | Some(ClickTarget::CdvFolder(_))
            ),
            "the panel border is not a file row"
        );
        assert!(
            !matches!(
                state.surface_at(cdv_x(&hit), cdv_row_y(&state, &hit, rows.len() + 1), &hit),
                Some(ClickTarget::CdvFile(_)) | Some(ClickTarget::CdvFolder(_))
            ),
            "past the last row is not a file row"
        );
        // The rows around the inline panel are still commit rows: the panel did
        // not swallow the history it is embedded in.
        assert!(matches!(
            state.surface_at(0, 2, &hit),
            Some(ClickTarget::Row(0, _, _))
        ));
        let last = hit.cdv.unwrap().bottom() + 2;
        assert!(matches!(
            state.surface_at(0, last, &hit),
            Some(ClickTarget::Row(_, _, _))
        ));
    }

    #[test]
    fn a_folder_click_toggles_it_open_and_closed() {
        let (mut state, hit) = cdv_geometry(&["src/lib.rs", "src/app.rs"]);
        let rows = state.cdv.as_ref().unwrap().file_rows();
        let folder = rows
            .iter()
            .position(|row| matches!(row.kind, files::FileRowKind::Folder { .. }))
            .expect("a tree view folds src/ into a folder row");
        let path = rows[folder].path.clone();
        let target = state
            .surface_at(cdv_x(&hit), cdv_row_y(&state, &hit, folder), &hit)
            .expect("the folder row is a click target");
        assert_eq!(target, ClickTarget::CdvFolder(path.clone()));

        let files_before = state.cdv.as_ref().unwrap().file_rows().len();
        assert!(
            !state.cdv.as_ref().unwrap().closed_folders.contains(&path),
            "the tree starts expanded"
        );

        // First click closes it: the children stop being listed.
        toggle_cdv_folder(&mut state, &path);
        assert!(state.cdv.as_ref().unwrap().closed_folders.contains(&path));
        assert!(
            state.cdv.as_ref().unwrap().file_rows().len() < files_before,
            "closing a folder hides its children"
        );

        // Second click re-opens it, restoring exactly the original rows.
        toggle_cdv_folder(&mut state, &path);
        assert!(!state.cdv.as_ref().unwrap().closed_folders.contains(&path));
        assert_eq!(state.cdv.as_ref().unwrap().file_rows().len(), files_before);
    }

    #[test]
    fn surface_at_ignores_a_closed_cdv() {
        let (mut state, mut hit) = cdv_geometry(&["src/lib.rs"]);
        state.cdv = None;
        hit.cdv = None;
        // With the panel closed no cell can be a file row, and the body is the
        // only surface left. Row 0 owns two display rows, so screen row 6 (the
        // display row the panel used to start at) is past the four commits.
        let x = hit.graph_width + 1;
        assert_eq!(state.surface_at(x, 12, &hit), None, "past the last commit");
        for y in 0..24 {
            assert!(
                !matches!(
                    state.surface_at(x, y, &hit),
                    Some(ClickTarget::CdvFile(_)) | Some(ClickTarget::CdvFolder(_))
                ),
                "row {y} must not resolve to a closed panel"
            );
        }
    }

    #[test]
    fn hit_geometry_places_the_cdv_inline_under_its_row() {
        let (mut state, hit) = cdv_geometry(&["src/lib.rs"]);
        let cdv = hit.cdv.expect("the panel is open, so it has a rect");
        // `body` is the table pane (header first); the graph body is one row
        // lower, and row 0's block is `refs + commit + panel` (T3/T7).
        assert_eq!(
            cdv.y,
            hit.body.y + 3,
            "the panel sits two display rows under the body top"
        );
        assert!(
            cdv.y + cdv.height <= hit.body.bottom(),
            "the inline panel stays inside the body"
        );
        assert_eq!(
            cdv.width,
            hit.body.width - hit.graph_width,
            "the panel spans the text area"
        );
        // The panel no longer steals rows from the body: it is part of the flow.
        assert_eq!(hit.body.height, 22);
        state.find.visible = true;
        let with_find =
            crate::render::hit_geometry(&state, &view_of(&state), 24).expect("geometry");
        let overlays = crate::render::overlay_rows(&state, 24);
        assert_eq!(
            with_find.body.height + overlays.find + overlays.tabs + 1,
            24,
            "body + find bar + tab bar + status bar fill the screen"
        );
        assert!(
            with_find.body.height < hit.body.height,
            "the find bar is charged to the body too"
        );
    }

    /// The `ViewOptions` the CDV-geometry tests pass to `hit_geometry`.
    fn view_of(state: &AppState) -> crate::render::ViewOptions<'_> {
        crate::render::ViewOptions {
            graph: Some(&state.layout),
            ..crate::render::ViewOptions::TEXT_ONLY
        }
    }

    #[test]
    fn a_cdv_file_click_moves_the_cursor_and_opens_the_file_menu() {
        let (mut state, hit) = cdv_geometry(&["src/lib.rs", "src/app.rs"]);
        let rows = state.cdv.as_ref().unwrap().file_rows();
        let last = rows
            .iter()
            .rposition(|row| matches!(row.kind, files::FileRowKind::File(_)))
            .expect("two file rows");
        let y = cdv_row_y(&state, &hit, last);
        let path = rows[last].path.clone();
        match state
            .surface_at(cdv_x(&hit), y, &hit)
            .expect("the file row")
        {
            ClickTarget::CdvFile(hit_path) => {
                assert_eq!(hit_path, path);
                state.cdv.as_mut().unwrap().select_path(&hit_path);
            }
            other => panic!("expected a CDV file row, got {other:?}"),
        }
        assert_eq!(
            state.cdv.as_ref().unwrap().selected_path().as_deref(),
            Some(path.as_str()),
            "the click moves the cursor onto the clicked file"
        );
        // The menu is the `FileRow` surface and carries the path, which is what
        // enables A65..A73 — `menu_context` never set `path` before W10.
        state.open_menu(MenuSurface::FileRow, Some(&path), 1, y);
        let menu = state.menu.as_ref().expect("a menu is open");
        assert_eq!(menu.surface, MenuSurface::FileRow);
        assert_eq!(menu.context.path, path);
        assert!(menu.context.ref_name.is_empty(), "not a ref menu");
        let ids: Vec<&str> = menu.items.iter().map(|item| item.id).collect();
        assert_eq!(
            ids,
            vec!["A65", "A66", "A67", "A68", "A69", "A70", "A71", "A72", "A73"],
            "the file-row inventory"
        );
        assert!(
            menu.items.iter().all(|item| item.enabled),
            "a non-empty path enables every item"
        );
    }

    #[test]
    fn select_path_ignores_a_folder_row() {
        let (mut state, _) = cdv_geometry(&["src/lib.rs", "src/app.rs"]);
        let panel = state.cdv.as_mut().unwrap();
        let folder = panel
            .file_rows()
            .into_iter()
            .find(|row| matches!(row.kind, files::FileRowKind::Folder { .. }))
            .expect("a folder row");
        assert!(
            !panel.select_path(&folder.path),
            "a folder is not a file row"
        );
        assert_eq!(panel.selected, 0, "the cursor did not move");
    }

    /* -------------------------------------------------------------- */
    /* W10 — the A67/A68/A71/A72/A73 keys                             */
    /* -------------------------------------------------------------- */

    /// A CDV holding one file, with that file row under the cursor.
    fn cdv_with_cursor() -> AppState {
        let (mut state, _) = cdv_geometry(&["src/lib.rs"]);
        assert!(
            state.cdv.as_mut().unwrap().select_path("src/lib.rs"),
            "the fixture lists src/lib.rs"
        );
        state
    }

    #[test]
    fn the_new_cdv_keys_map_to_their_file_row_actions() {
        use crate::keymap::cdv_key_for;
        use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
        let cases = [
            (KeyCode::Char('w'), Action::CdvFileViewDiffWithWorkingFile),
            (KeyCode::Char('e'), Action::CdvFileOpen),
            (KeyCode::Char('R'), Action::CdvFileResetToRevision),
            (KeyCode::Char('y'), Action::CdvFileCopyAbsolutePath),
            (KeyCode::Char('Y'), Action::CdvFileCopyRelativePath),
        ];
        for (code, action) in cases {
            assert_eq!(
                cdv_key_for(KeyEvent::new(code, KeyModifiers::NONE)),
                Some(action),
                "{code:?} must reach {action:?}"
            );
        }
        // Unshifted `r` stays free: only the gated reset takes a key, and it
        // takes the shifted one so a stray press cannot arm it.
        assert_eq!(
            cdv_key_for(KeyEvent::new(KeyCode::Char('r'), KeyModifiers::NONE)),
            None
        );
        // A67 and A68 accept either case; A72 and A73 are told apart by it.
        assert_eq!(
            cdv_key_for(KeyEvent::new(KeyCode::Char('W'), KeyModifiers::NONE)),
            Some(Action::CdvFileViewDiffWithWorkingFile)
        );
    }

    #[test]
    fn the_new_cdv_keys_build_the_inventory_argv() {
        let state = cdv_with_cursor();
        let target = cdv_file_target(&state).expect("a file row is selected");
        let plan = actions::plan(RefAction::ViewDiffWithWorkingFile, &target);
        assert_eq!((plan.dsp, plan.act), ("DSP-060", "A67"));
        assert_eq!(
            plan.argv,
            vec![
                "diff",
                "--no-color",
                crate::data::UNCOMMITTED,
                "--",
                "src/lib.rs"
            ],
            "`w` diffs the working tree against the panel's commit"
        );
        let plan = actions::plan(RefAction::OpenFile, &target);
        assert_eq!((plan.dsp, plan.act), ("DSP-038", "A68"));
        assert_eq!(plan.argv, vec!["show", ":src/lib.rs"]);
        let plan = actions::plan(RefAction::ResetFileToRevision, &target);
        assert_eq!((plan.dsp, plan.act), ("DSP-049", "A71"));
        assert!(plan.kind.is_gated(), "A71 overwrites a working file");
        assert_eq!(
            plan.argv,
            vec!["checkout", crate::data::UNCOMMITTED, "--", "src/lib.rs"]
        );
    }

    #[test]
    fn the_reset_key_stops_at_the_confirmation_dialog() {
        let mut state = cdv_with_cursor();
        // A bogus repo proves nothing ran: a real git call would have failed.
        reduce(
            &mut state,
            Action::CdvFileResetToRevision,
            Path::new("/nonexistent/ggt-w10"),
        );
        assert!(
            state.dialog.is_some(),
            "the gated action opens the W6 dialog"
        );
        assert!(
            state
                .status
                .last_action
                .as_deref()
                .unwrap_or_default()
                .contains("confirm"),
            "the status bar announces the confirmation"
        );
    }

    #[test]
    fn the_new_cdv_keys_are_no_ops_without_a_file_row() {
        let mut state = state_of(linear(3));
        assert!(state.cdv.is_none());
        for action in [
            Action::CdvFileViewDiffWithWorkingFile,
            Action::CdvFileOpen,
            Action::CdvFileResetToRevision,
            Action::CdvFileCopyAbsolutePath,
            Action::CdvFileCopyRelativePath,
        ] {
            reduce(&mut state, action, Path::new("/nonexistent/ggt-w10"));
        }
        assert!(state.dialog.is_none(), "no panel, no dialog");
        assert!(state.pager.is_none(), "no panel, no pager");
        assert!(state.pending_clipboard.is_none(), "no panel, no copy");
    }

    #[test]
    fn cdv_copy_text_is_absolute_for_y_and_relative_for_shift_y() {
        let repo = Path::new("/repos/demo");
        assert_eq!(
            cdv_copy_text(repo, "src/lib.rs", true),
            "/repos/demo/src/lib.rs",
            "A72 copies the absolute path"
        );
        assert_eq!(
            cdv_copy_text(repo, "src/lib.rs", false),
            "src/lib.rs",
            "A73 copies the repo-relative path"
        );
    }

    #[test]
    fn the_copy_keys_put_the_path_on_the_clipboard() {
        let mut state = cdv_with_cursor();
        // Hermetic: point the fallback file at this test's own path while the
        // shared env lock is held (`actions`' clipboard tests mutate it too).
        let _guard = crate::actions::ENV_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let path = std::env::temp_dir().join(format!("ggt-w10-copy-{}.txt", std::process::id()));
        std::env::set_var("GGT_CLIPBOARD_FILE", &path);

        let repo = Path::new("/repos/demo");
        reduce(&mut state, Action::CdvFileCopyRelativePath, repo);
        assert_eq!(state.pending_clipboard.as_deref(), Some("src/lib.rs"));
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "src/lib.rs");

        reduce(&mut state, Action::CdvFileCopyAbsolutePath, repo);
        assert_eq!(
            state.pending_clipboard.as_deref(),
            Some("/repos/demo/src/lib.rs")
        );
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "/repos/demo/src/lib.rs"
        );
        assert!(
            state
                .status
                .last_action
                .as_deref()
                .unwrap_or_default()
                .contains("copied to"),
            "the status bar reports the copy"
        );

        std::env::remove_var("GGT_CLIPBOARD_FILE");
        let _ = std::fs::remove_file(&path);
    }
}
