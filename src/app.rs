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

use crate::actions::{self, RefAction};
use crate::cdv::{self, CdvKind, CdvState};
use crate::data::{self, Commit};
use crate::dialog::{self, Dialog, DialogKey, DialogResult};
use crate::files;
use crate::find::FindWidget;
use crate::layout::types::{GraphLayout, RowIdx};
use crate::layout::{self, LayoutOpts, NavDir, NO_ROW};
use crate::menu::{
    classify_ref, surface_for_refs, ColumnId, CommitOrder, ContextMenu, MenuAction, MenuColumns,
    MenuContext, MenuSurface,
};

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
    /// Grow the CDV panel — `+` (the `cdvHeightResize` handle).
    CdvResizeUp,
    /// Shrink the CDV panel — `-`.
    CdvResizeDown,
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
}

/// What a screen cell belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClickTarget {
    /// The table header row, right of the graph → column menu.
    ColumnHeader,
    /// A commit row, with the surface its refs (or its position) imply.
    Row(RowIdx, MenuSurface, String),
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

    /// Is a context menu open?
    pub fn menu_is_open(&self) -> bool {
        self.menu.is_some()
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
        let mut cursor = 0u16;
        for row in 0..self.rows.len() as RowIdx {
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
        Action::Quit => return Outcome::Quit,
        Action::SelectNext => select_table(state, 1),
        Action::SelectPrev => select_table(state, -1),
        Action::PageNext => select_page(state, 1),
        Action::PagePrev => select_page(state, -1),
        Action::Top => state.selected = 0,
        Action::Bottom => state.selected = state.rows.len().saturating_sub(1) as RowIdx,
        Action::ToggleExpand => {
            toggle_expand(state);
            // W6/T3: expanding a row loads its commit details view; collapsing
            // it (or expanding a different row) closes or replaces the panel.
            if state.expanded.contains(&state.selected) {
                open_cdv(state, repo);
            } else {
                close_cdv_for(state, state.selected);
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
        Action::CdvResizeUp => resize_cdv(state, 2),
        Action::CdvResizeDown => resize_cdv(state, -2),

        // W6/T1 — the find widget.
        Action::FindOpen => {
            state.find.show();
            recompute_find(state);
        }
        Action::FindClose => state.find.close(),
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
    state.cdv = Some(cdv);
}

/// Close the panel when it belongs to `row` (collapsing that row).
fn close_cdv_for(state: &mut AppState, row: RowIdx) {
    if state.cdv.as_ref().map(|cdv| cdv.row) == Some(row) {
        state.cdv = None;
        state.cdv_focused = false;
    }
}

/// Grow/shrink the docked panel (`cdvHeightResize`).
fn resize_cdv(state: &mut AppState, delta: i32) {
    if let Some(cdv) = state.cdv.as_mut() {
        cdv.resize(delta);
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
    state
        .find
        .recompute(&state.rows, &state.columns, uncommitted, previous.as_deref());
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
        assert_eq!(state.row_at_display(0), Some(0));
        assert_eq!(state.row_at_display(2), Some(2));
        assert_eq!(state.row_at_display(3), None);
        state.expanded.insert(0);
        // Row 0 now spans display rows 0..=EXPANDED_EXTRA_ROWS.
        assert_eq!(state.row_at_display(EXPANDED_EXTRA_ROWS), Some(0));
        assert_eq!(state.row_at_display(EXPANDED_EXTRA_ROWS + 1), Some(1));
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
        // Past the last commit, and outside the body entirely.
        assert_eq!(state.surface_at(0, 4, &hit), None);
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
                2,
                MenuSurface::CommitRow,
                "c2".to_string()
            ))
        );
    }

    #[test]
    fn surface_at_on_a_row_with_no_refs_is_the_commit_menu() {
        let state = state_of(linear(3));
        let hit = geometry();
        match state
            .surface_at(95, 2, &hit)
            .expect("row 1 in the refs column")
        {
            ClickTarget::Row(1, surface, name) => {
                assert_eq!(surface, MenuSurface::CommitRow);
                assert_eq!(name, "c1");
            }
            other => panic!("expected the commit menu, got {other:?}"),
        }
    }
}
