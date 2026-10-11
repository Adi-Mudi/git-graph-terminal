//! W5 (T1): the **context-menu model**.
//!
//! The reference `git-graph` opens a different right-click menu depending on
//! what the pointer is over. W5 keeps that idea but makes it explicit and
//! testable: a [`MenuSurface`] is one of the nine surfaces from the inventory,
//! and [`items`] is a pure builder returning the exact `A11..A73` rows in
//! inventory order, each carrying its `DSP-nnn` citation and the
//! [`RefAction`](crate::actions::RefAction) it dispatches.
//!
//! Nothing in this module touches git or the terminal: it is data plus two
//! small state machines ([`ContextMenu`] navigation and [`MenuAction`]).
//! `app.rs` owns the open/close lifecycle, `main.rs` owns key routing, and
//! `render` draws the popup.

use crate::actions::RefAction;

/// Which right-click surface the menu belongs to (inventory rows A11..A73).
///
/// All nine surfaces are built by [`items`]; `Uncommitted`, `Link` and `FileRow`
/// are only reachable once the detail pane and the uncommitted row land, so they
/// are constructed by tests rather than by the running binary.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuSurface {
    /// A local branch label in the graph / table.
    Branch,
    /// A commit row itself.
    CommitRow,
    /// A remote-tracking branch label (`origin/main`).
    RemoteBranch,
    /// A `refs/stash` label.
    Stash,
    /// A tag label.
    Tag,
    /// The uncommitted-changes row (mock fixture only).
    Uncommitted,
    /// The table header (column visibility + ordering).
    ColumnHeader,
    /// A URL / internal link decoration.
    Link,
    /// A file row inside a commit's expanded detail list.
    FileRow,
}

impl MenuSurface {
    /// Border title for the popup.
    pub fn title(self) -> &'static str {
        match self {
            MenuSurface::Branch => "Branch",
            MenuSurface::CommitRow => "Commit",
            MenuSurface::RemoteBranch => "Remote Branch",
            MenuSurface::Stash => "Stash",
            MenuSurface::Tag => "Tag",
            MenuSurface::Uncommitted => "Uncommitted Changes",
            MenuSurface::ColumnHeader => "Table Columns",
            MenuSurface::Link => "Link",
            MenuSurface::FileRow => "File",
        }
    }

    /// All nine surfaces, in inventory order.
    #[allow(dead_code)] // read by the surface-coverage test
    pub const ALL: [MenuSurface; 9] = [
        MenuSurface::Branch,
        MenuSurface::CommitRow,
        MenuSurface::RemoteBranch,
        MenuSurface::Stash,
        MenuSurface::Tag,
        MenuSurface::Uncommitted,
        MenuSurface::ColumnHeader,
        MenuSurface::Link,
        MenuSurface::FileRow,
    ];
}

/// A table column the header menu can show or hide.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnId {
    Date,
    Author,
    Commit,
}

/// Commit ordering, mirroring the reference's radio group in the header menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CommitOrder {
    #[default]
    Date,
    AuthorDate,
    Topological,
}

impl CommitOrder {
    /// The `git log` flag this ordering implies. W5 records the choice; wiring
    /// the flag into `data.rs` is a documented follow-up (see the W5 report).
    pub fn git_flag(self) -> &'static str {
        match self {
            CommitOrder::Date => "--date-order",
            CommitOrder::AuthorDate => "--author-date-order",
            CommitOrder::Topological => "--topo-order",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            CommitOrder::Date => "Commit Timestamp Order",
            CommitOrder::AuthorDate => "Author Timestamp Order",
            CommitOrder::Topological => "Topological Order",
        }
    }
}

/// What activating a menu item does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuAction {
    /// Run the W5 dispatch map.
    Dispatch(RefAction),
    /// Show/hide a table column (column-header surface).
    ToggleColumn(ColumnId),
    /// Switch commit ordering (column-header surface).
    SetColumnOrder(CommitOrder),
    /// Reference behaviour we keep as a documented local stub.
    SelectInDropdown(String),
    UnselectInDropdown(String),
    FollowInternalLink(String),
}

/// One row of a context menu, matching an inventory `A..` row exactly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuItem {
    /// Inventory `id` (`A11`..`A73`).
    pub id: &'static str,
    /// Inventory `label`, verbatim.
    pub label: &'static str,
    /// Inventory `DSP` citation when the row has one.
    pub dsp: Option<&'static str>,
    pub action: MenuAction,
    pub enabled: bool,
    /// Single-key shortcut inside the open menu (ours; inventory has none).
    pub shortcut: Option<char>,
    /// Tick state for the toggle/radio rows of the header menu.
    pub checked: bool,
}

impl MenuItem {
    fn new(
        id: &'static str,
        label: &'static str,
        dsp: Option<&'static str>,
        action: MenuAction,
    ) -> Self {
        MenuItem {
            id,
            label,
            dsp,
            action,
            enabled: true,
            shortcut: None,
            checked: false,
        }
    }

    fn disabled(mut self) -> Self {
        self.enabled = false;
        self
    }

    fn checked(mut self, checked: bool) -> Self {
        self.checked = checked;
        self
    }
}

/// A dispatch row scoped to one surface, with a shorthand `dsp` citation.
fn dispatch(
    id: &'static str,
    label: &'static str,
    dsp: &'static str,
    action: RefAction,
) -> MenuItem {
    MenuItem::new(id, label, Some(dsp), MenuAction::Dispatch(action))
}

/// Context the builder needs to enable/disable rows and to seed the target.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
/// The values a menu opens with: which ref/commit it is about, what the repo
/// offers, and the live column state.
pub struct MenuContext {
    pub ref_name: String,
    pub hash: String,
    pub subject: String,
    pub remote: String,
    pub branch: String,
    pub selector: String,
    pub path: String,
    pub url: String,
    /// True when HEAD sits on this row, so the menu can hide "Checkout".
    pub is_current_branch: bool,
    pub has_remotes: bool,
    /// Stash selector currently selected (defaults to `stash@{0}`).
    pub stash_index: usize,
    pub columns: MenuColumns,
}

/// Column visibility/order snapshot the header menu renders.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MenuColumns {
    pub date: bool,
    pub author: bool,
    pub commit: bool,
    pub order: CommitOrder,
}

impl Default for MenuColumns {
    fn default() -> Self {
        // W5 shipping default: the three optional columns all visible, date order
        // (matches `data::commits`, which logs with `--date-order`).
        MenuColumns {
            date: true,
            author: true,
            commit: true,
            order: CommitOrder::Date,
        }
    }
}

impl MenuContext {
    pub fn from_ref(surface: MenuSurface, ref_name: &str) -> Self {
        let mut ctx = MenuContext::default();
        match surface {
            MenuSurface::RemoteBranch => {
                let (remote, branch) = split_remote_ref(ref_name);
                ctx.remote = remote;
                ctx.branch = branch;
                ctx.ref_name = ref_name.to_string();
            }
            MenuSurface::Stash => {
                ctx.selector = ref_name.to_string();
                ctx.ref_name = ref_name.to_string();
            }
            _ => ctx.ref_name = ref_name.to_string(),
        }
        ctx
    }

    /// The [`ActionTarget`](crate::actions::ActionTarget) a dispatch should use.
    pub fn action_target(&self) -> crate::actions::ActionTarget {
        let mut target = crate::actions::ActionTarget {
            ref_name: self.ref_name.clone(),
            hash: self.hash.clone(),
            subject: self.subject.clone(),
            remote: self.remote.clone(),
            branch: self.branch.clone(),
            selector: self.selector.clone(),
            path: self.path.clone(),
            url: self.url.clone(),
            message: String::new(),
            name: self.ref_name.clone(),
            ..Default::default()
        };
        // Stash labels may be `stash@{n}` with no `refs/stash` prefix.
        if target.selector.is_empty() && target.ref_name.starts_with("stash@{") {
            target.selector = target.ref_name.clone();
        }
        // Branch surface: the ref name is the branch, and it is the local branch
        // for checkout/push purposes.
        if target.branch.is_empty() {
            target.branch = self.ref_name.clone();
        }
        target.local_branch = String::new();
        target.commit = self.hash.clone();
        target
    }
}

/// Split `origin/feature/x` into (`origin`, `feature/x`).
pub fn split_remote_ref(ref_name: &str) -> (String, String) {
    match ref_name.split_once('/') {
        Some((remote, branch)) => (remote.to_string(), branch.to_string()),
        None => (String::new(), ref_name.to_string()),
    }
}

/// Classify a graph ref decoration into a menu surface plus its name.
///
/// Handles the `layout::refs` decorations (`HEAD -> main`, `tag: v1`,
/// `origin/main`, `refs/stash`) as well as bare names.
pub fn classify_ref(ref_label: &str) -> (MenuSurface, String) {
    let trimmed = ref_label.trim();
    if let Some(tag) = trimmed.strip_prefix("tag: ") {
        return (MenuSurface::Tag, tag.trim().to_string());
    }
    if let Some(branch) = trimmed.strip_prefix("HEAD -> ") {
        return (MenuSurface::Branch, branch.trim().to_string());
    }
    if trimmed == "HEAD" {
        return (MenuSurface::Branch, trimmed.to_string());
    }
    if trimmed.starts_with("stash") || trimmed.starts_with("refs/stash") {
        return (MenuSurface::Stash, trimmed.to_string());
    }
    if let Some(rest) = trimmed.strip_prefix("refs/heads/") {
        return (MenuSurface::Branch, rest.to_string());
    }
    if let Some(rest) = trimmed.strip_prefix("refs/remotes/") {
        return (MenuSurface::RemoteBranch, rest.to_string());
    }
    if let Some(rest) = trimmed.strip_prefix("refs/tags/") {
        return (MenuSurface::Tag, rest.to_string());
    }
    if trimmed.contains('/') {
        return (MenuSurface::RemoteBranch, trimmed.to_string());
    }
    (MenuSurface::Branch, trimmed.to_string())
}

/// Pick the surface for a commit's ref decorations: the richest ref wins, so a
/// row carrying both `main` and `origin/main` opens the branch menu.
pub fn surface_for_refs(refs: &[String]) -> Option<(MenuSurface, String)> {
    let mut best: Option<(usize, MenuSurface, String)> = None;
    for label in refs {
        let (surface, name) = classify_ref(label);
        let rank = match surface {
            MenuSurface::Stash => 0,
            MenuSurface::Tag => 1,
            MenuSurface::RemoteBranch => 2,
            MenuSurface::Branch => 3,
            _ => 4,
        };
        if best.as_ref().map(|(r, _, _)| rank > *r).unwrap_or(true) {
            best = Some((rank, surface, name));
        }
    }
    best.map(|(_, surface, name)| (surface, name))
}

/// Build the exact item list for `surface`, in inventory order.
pub fn items(surface: MenuSurface, ctx: &MenuContext) -> Vec<MenuItem> {
    let rows = match surface {
        MenuSurface::Branch => branch_items(ctx),
        MenuSurface::CommitRow => commit_items(ctx),
        MenuSurface::RemoteBranch => remote_items(ctx),
        MenuSurface::Stash => stash_items(ctx),
        MenuSurface::Tag => tag_items(ctx),
        MenuSurface::Uncommitted => uncommitted_items(ctx),
        MenuSurface::ColumnHeader => column_items(ctx),
        MenuSurface::Link => link_items(ctx),
        MenuSurface::FileRow => file_items(ctx),
    };
    assign_shortcuts(rows)
}

/// Keys the popup's navigation claims for itself, so no row may advertise them as
/// a shortcut: `j`/`k` are the vim-style down/up inside the menu.
const RESERVED_SHORTCUTS: [char; 2] = ['j', 'k'];

/// Give every item a unique `a`, `b`, `c`… shortcut, in display order.
///
/// `j` and `k` are skipped because they move the highlight (see
/// [`crate::keymap::menu_nav_for`]), so a printed letter always activates its row.
fn assign_shortcuts(items: Vec<MenuItem>) -> Vec<MenuItem> {
    let mut alphabet = (b'a'..=b'z')
        .map(|byte| byte as char)
        .filter(|key| !RESERVED_SHORTCUTS.contains(key));
    items
        .into_iter()
        .map(|mut item| {
            item.shortcut = alphabet.next();
            item
        })
        .collect()
}

fn branch_items(ctx: &MenuContext) -> Vec<MenuItem> {
    // The current branch cannot be checked out, deleted, merged into itself or
    // rebased onto itself: those rows render greyed-out (reference behaviour).
    let current = ctx.is_current_branch;
    vec![
        dispatch(
            "A11",
            "Checkout Branch",
            "DSP-005",
            RefAction::CheckoutBranch,
        )
        .apply_if(current, MenuItem::disabled),
        dispatch("A12", "Rename Branch…", "DSP-047", RefAction::RenameBranch),
        dispatch("A13", "Delete Branch…", "DSP-016", RefAction::DeleteBranch)
            .apply_if(current, MenuItem::disabled),
        dispatch(
            "A14",
            "Merge into current branch…",
            "DSP-034",
            RefAction::Merge,
        )
        .apply_if(current, MenuItem::disabled),
        dispatch(
            "A15",
            "Rebase current branch on Branch…",
            "DSP-046",
            RefAction::Rebase,
        )
        .apply_if(current, MenuItem::disabled),
        dispatch("A16", "Push Branch…", "DSP-043", RefAction::PushBranch)
            .apply_if(!ctx.has_remotes, MenuItem::disabled),
        dispatch(
            "A17",
            "Create Pull Request…",
            "DSP-015",
            RefAction::CreatePullRequest,
        ),
        dispatch("A18", "Create Archive", "DSP-013", RefAction::CreateArchive),
        MenuItem::new(
            "A19",
            "Select in Branches Dropdown",
            None,
            MenuAction::SelectInDropdown(ctx.ref_name.clone()),
        ),
        MenuItem::new(
            "A20",
            "Unselect in Branches Dropdown",
            None,
            MenuAction::UnselectInDropdown(ctx.ref_name.clone()),
        ),
        dispatch(
            "A21",
            "Copy Branch Name to Clipboard",
            "DSP-012",
            RefAction::CopyToClipboard,
        ),
        dispatch("A22", "View Issue…", "DSP-037", RefAction::OpenExternalUrl)
            .apply_if(ctx.url.is_empty(), MenuItem::disabled),
    ]
}

fn commit_items(_ctx: &MenuContext) -> Vec<MenuItem> {
    vec![
        dispatch("A23", "Add Tag…", "DSP-002", RefAction::AddTag),
        dispatch("A24", "Create Branch…", "DSP-014", RefAction::CreateBranch),
        dispatch("A25", "Checkout", "DSP-006", RefAction::CheckoutCommit),
        dispatch(
            "A26",
            "Cherry Pick…",
            "DSP-007",
            RefAction::CherrypickCommit,
        ),
        dispatch("A27", "Revert…", "DSP-051", RefAction::RevertCommit),
        dispatch("A28", "Drop…", "DSP-021", RefAction::DropCommit),
        dispatch(
            "A29",
            "Merge into current branch…",
            "DSP-034",
            RefAction::Merge,
        ),
        dispatch(
            "A30",
            "Rebase current branch on this Commit…",
            "DSP-046",
            RefAction::Rebase,
        ),
        dispatch(
            "A31",
            "Reset current branch to this Commit…",
            "DSP-050",
            RefAction::ResetToCommit,
        ),
        dispatch(
            "A32",
            "Copy Commit Hash to Clipboard",
            "DSP-012",
            RefAction::CopyToClipboard,
        ),
        dispatch(
            "A33",
            "Copy Commit Subject to Clipboard",
            "DSP-012",
            RefAction::CopyToClipboard,
        ),
    ]
}

fn remote_items(ctx: &MenuContext) -> Vec<MenuItem> {
    vec![
        dispatch(
            "A34",
            "Checkout Branch…",
            "DSP-005",
            RefAction::CheckoutBranch,
        ),
        dispatch(
            "A35",
            "Delete Remote Branch…",
            "DSP-018",
            RefAction::DeleteRemoteBranch,
        ),
        dispatch(
            "A36",
            "Fetch into local branch…",
            "DSP-029",
            RefAction::FetchIntoLocalBranch,
        ),
        dispatch(
            "A37",
            "Merge into current branch…",
            "DSP-034",
            RefAction::Merge,
        ),
        dispatch(
            "A38",
            "Pull into current branch…",
            "DSP-042",
            RefAction::PullBranch,
        ),
        dispatch(
            "A39",
            "Create Pull Request",
            "DSP-015",
            RefAction::CreatePullRequest,
        ),
        dispatch("A40", "Create Archive", "DSP-013", RefAction::CreateArchive),
        MenuItem::new(
            "A41",
            "Select in Branches Dropdown",
            None,
            MenuAction::SelectInDropdown(ctx.ref_name.clone()),
        ),
        MenuItem::new(
            "A42",
            "Unselect in Branches Dropdown",
            None,
            MenuAction::UnselectInDropdown(ctx.ref_name.clone()),
        ),
        dispatch(
            "A43",
            "Copy Branch Name to Clipboard",
            "DSP-012",
            RefAction::CopyToClipboard,
        ),
        dispatch("A44", "View Issue…", "DSP-037", RefAction::OpenExternalUrl)
            .apply_if(ctx.url.is_empty(), MenuItem::disabled),
    ]
}

fn stash_items(_ctx: &MenuContext) -> Vec<MenuItem> {
    vec![
        dispatch("A45", "Apply Stash…", "DSP-003", RefAction::ApplyStash),
        dispatch(
            "A46",
            "Create Branch from Stash…",
            "DSP-004",
            RefAction::BranchFromStash,
        ),
        dispatch("A47", "Pop Stash…", "DSP-040", RefAction::PopStash),
        dispatch("A48", "Drop Stash…", "DSP-022", RefAction::DropStash),
        dispatch(
            "A49",
            "Copy Stash Name to Clipboard",
            "DSP-012",
            RefAction::CopyToClipboard,
        ),
        dispatch(
            "A50",
            "Copy Stash Hash to Clipboard",
            "DSP-012",
            RefAction::CopyToClipboard,
        ),
    ]
}

fn tag_items(_ctx: &MenuContext) -> Vec<MenuItem> {
    vec![
        dispatch("A51", "View Details", "DSP-057", RefAction::TagDetails),
        dispatch("A52", "Delete Tag…", "DSP-019", RefAction::DeleteTag),
        dispatch("A53", "Push Tag…", "DSP-045", RefAction::PushTag),
        dispatch("A54", "Create Archive", "DSP-013", RefAction::CreateArchive),
        dispatch(
            "A55",
            "Copy Tag Name to Clipboard",
            "DSP-012",
            RefAction::CopyToClipboard,
        ),
    ]
}

fn uncommitted_items(_ctx: &MenuContext) -> Vec<MenuItem> {
    vec![
        dispatch(
            "A56",
            "Stash uncommitted changes…",
            "DSP-044",
            RefAction::PushStash,
        ),
        dispatch(
            "A57",
            "Reset uncommitted changes…",
            "DSP-050",
            RefAction::ResetToCommit,
        ),
        dispatch(
            "A58",
            "Clean untracked files…",
            "DSP-008",
            RefAction::CleanUntrackedFiles,
        ),
        dispatch(
            "A59",
            "Open Source Control View",
            "DSP-062",
            RefAction::ViewScm,
        ),
    ]
}

fn column_items(ctx: &MenuContext) -> Vec<MenuItem> {
    vec![
        MenuItem::new(
            "A60",
            "Date",
            None,
            MenuAction::ToggleColumn(ColumnId::Date),
        )
        .checked(ctx.columns.date),
        MenuItem::new(
            "A60",
            "Author",
            None,
            MenuAction::ToggleColumn(ColumnId::Author),
        )
        .checked(ctx.columns.author),
        MenuItem::new(
            "A60",
            "Commit",
            None,
            MenuAction::ToggleColumn(ColumnId::Commit),
        )
        .checked(ctx.columns.commit),
        MenuItem::new(
            "A61",
            "Commit Timestamp Order",
            None,
            MenuAction::SetColumnOrder(CommitOrder::Date),
        )
        .checked(ctx.columns.order == CommitOrder::Date),
        MenuItem::new(
            "A61",
            "Author Timestamp Order",
            None,
            MenuAction::SetColumnOrder(CommitOrder::AuthorDate),
        )
        .checked(ctx.columns.order == CommitOrder::AuthorDate),
        MenuItem::new(
            "A61",
            "Topological Order",
            None,
            MenuAction::SetColumnOrder(CommitOrder::Topological),
        )
        .checked(ctx.columns.order == CommitOrder::Topological),
    ]
}

fn link_items(ctx: &MenuContext) -> Vec<MenuItem> {
    vec![
        dispatch("A62", "Open URL", "DSP-037", RefAction::OpenExternalUrl)
            .apply_if(ctx.url.is_empty(), MenuItem::disabled),
        MenuItem::new(
            "A63",
            "Follow Internal Link",
            None,
            MenuAction::FollowInternalLink(ctx.url.clone()),
        ),
        dispatch(
            "A64",
            "Copy URL to Clipboard",
            "DSP-012",
            RefAction::CopyToClipboard,
        ),
    ]
}

fn file_items(ctx: &MenuContext) -> Vec<MenuItem> {
    vec![
        dispatch("A65", "View Diff", "DSP-059", RefAction::ViewDiff)
            .apply_if(ctx.path.is_empty(), MenuItem::disabled),
        dispatch(
            "A66",
            "View File at this Revision",
            "DSP-061",
            RefAction::ViewFileAtRevision,
        )
        .apply_if(ctx.path.is_empty(), MenuItem::disabled),
        dispatch(
            "A67",
            "View Diff with Working File",
            "DSP-060",
            RefAction::ViewDiffWithWorkingFile,
        )
        .apply_if(ctx.path.is_empty(), MenuItem::disabled),
        dispatch("A68", "Open File", "DSP-038", RefAction::OpenFile)
            .apply_if(ctx.path.is_empty(), MenuItem::disabled),
        MenuItem::new(
            "A69",
            "Mark as Reviewed",
            None,
            MenuAction::FollowInternalLink("review:on".into()),
        ),
        MenuItem::new(
            "A70",
            "Mark as Not Reviewed",
            None,
            MenuAction::FollowInternalLink("review:off".into()),
        ),
        dispatch(
            "A71",
            "Reset File to this Revision…",
            "DSP-049",
            RefAction::ResetFileToRevision,
        )
        .apply_if(ctx.path.is_empty(), MenuItem::disabled),
        dispatch(
            "A72",
            "Copy Absolute File Path to Clipboard",
            "DSP-011",
            RefAction::CopyFilePath,
        )
        .apply_if(ctx.path.is_empty(), MenuItem::disabled),
        dispatch(
            "A73",
            "Copy Relative File Path to Clipboard",
            "DSP-011",
            RefAction::CopyFilePath,
        )
        .apply_if(ctx.path.is_empty(), MenuItem::disabled),
    ]
}

impl MenuItem {
    /// Apply `f` when `condition` holds – keeps the builders readable.
    fn apply_if<F: FnOnce(MenuItem) -> MenuItem>(self, condition: bool, f: F) -> MenuItem {
        if condition {
            f(self)
        } else {
            self
        }
    }
}

/// An open context menu: which surface, where, and which row is highlighted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextMenu {
    pub surface: MenuSurface,
    /// Screen position the menu was opened at (mouse or the selected row).
    pub position: (u16, u16),
    pub items: Vec<MenuItem>,
    pub selected: usize,
    pub context: MenuContext,
}

impl ContextMenu {
    /// Build the menu for `surface` at `(x, y)`.
    pub fn open(surface: MenuSurface, x: u16, y: u16, context: MenuContext) -> Self {
        ContextMenu {
            surface,
            position: (x, y),
            items: items(surface, &context),
            selected: 0,
            context,
        }
    }

    pub fn selected_item(&self) -> Option<&MenuItem> {
        self.items.get(self.selected)
    }

    /// Move the highlight by `delta`, wrapping at both ends.
    pub fn move_selection(&mut self, delta: isize) {
        if self.items.is_empty() {
            return;
        }
        let len = self.items.len() as isize;
        let next = (self.selected as isize + delta).rem_euclid(len);
        self.selected = next as usize;
    }

    /// Jump to the item whose shortcut is `key`.
    pub fn select_shortcut(&mut self, key: char) -> bool {
        let key = key.to_ascii_lowercase();
        if let Some(index) = self
            .items
            .iter()
            .position(|item| item.shortcut == Some(key))
        {
            self.selected = index;
            true
        } else {
            false
        }
    }

    /// The first enabled item, so a fresh menu never opens on a dead row.
    pub fn first_enabled(&self) -> usize {
        self.items.iter().position(|item| item.enabled).unwrap_or(0)
    }

    /// Widest label plus the id, check and shortcut chrome, for the popup size.
    pub fn width_hint(&self) -> u16 {
        let widest = self
            .items
            .iter()
            .map(|item| item.label.chars().count() + 13)
            .max()
            .unwrap_or(16);
        widest.clamp(16, 56) as u16
    }

    /// The popup rectangle, clamped inside `screen`.
    ///
    /// Shared by the renderer and the mouse router so a click and the drawn box
    /// can never disagree.
    pub fn rect(&self, screen: ratatui::layout::Rect) -> ratatui::layout::Rect {
        use ratatui::layout::Rect;
        let width = self.width_hint().min(screen.width).max(1);
        let height = (self.items.len() as u16 + 2).min(screen.height).max(1);
        let x = self.position.0.min(screen.width.saturating_sub(width));
        let y = self.position.1.min(screen.height.saturating_sub(height));
        Rect {
            x: screen.x.saturating_add(x),
            y: screen.y.saturating_add(y),
            width,
            height,
        }
    }

    /// Is screen cell `(x, y)` inside the popup?
    pub fn contains(&self, x: u16, y: u16, screen: ratatui::layout::Rect) -> bool {
        let rect = self.rect(screen);
        x >= rect.x
            && x < rect.x.saturating_add(rect.width)
            && y >= rect.y
            && y < rect.y.saturating_add(rect.height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids_and_labels(surface: MenuSurface) -> Vec<(&'static str, &'static str)> {
        super::items(surface, &MenuContext::default())
            .into_iter()
            .map(|item| (item.id, item.label))
            .collect()
    }

    #[test]
    fn branch_surface_matches_the_inventory_rows() {
        assert_eq!(
            ids_and_labels(MenuSurface::Branch),
            vec![
                ("A11", "Checkout Branch"),
                ("A12", "Rename Branch…"),
                ("A13", "Delete Branch…"),
                ("A14", "Merge into current branch…"),
                ("A15", "Rebase current branch on Branch…"),
                ("A16", "Push Branch…"),
                ("A17", "Create Pull Request…"),
                ("A18", "Create Archive"),
                ("A19", "Select in Branches Dropdown"),
                ("A20", "Unselect in Branches Dropdown"),
                ("A21", "Copy Branch Name to Clipboard"),
                ("A22", "View Issue…"),
            ]
        );
    }

    #[test]
    fn commit_surface_matches_the_inventory_rows() {
        assert_eq!(
            ids_and_labels(MenuSurface::CommitRow),
            vec![
                ("A23", "Add Tag…"),
                ("A24", "Create Branch…"),
                ("A25", "Checkout"),
                ("A26", "Cherry Pick…"),
                ("A27", "Revert…"),
                ("A28", "Drop…"),
                ("A29", "Merge into current branch…"),
                ("A30", "Rebase current branch on this Commit…"),
                ("A31", "Reset current branch to this Commit…"),
                ("A32", "Copy Commit Hash to Clipboard"),
                ("A33", "Copy Commit Subject to Clipboard"),
            ]
        );
    }

    #[test]
    fn remote_surface_matches_the_inventory_rows() {
        assert_eq!(
            ids_and_labels(MenuSurface::RemoteBranch),
            vec![
                ("A34", "Checkout Branch…"),
                ("A35", "Delete Remote Branch…"),
                ("A36", "Fetch into local branch…"),
                ("A37", "Merge into current branch…"),
                ("A38", "Pull into current branch…"),
                ("A39", "Create Pull Request"),
                ("A40", "Create Archive"),
                ("A41", "Select in Branches Dropdown"),
                ("A42", "Unselect in Branches Dropdown"),
                ("A43", "Copy Branch Name to Clipboard"),
                ("A44", "View Issue…"),
            ]
        );
    }

    #[test]
    fn stash_tag_and_uncommitted_surfaces_match() {
        assert_eq!(
            ids_and_labels(MenuSurface::Stash),
            vec![
                ("A45", "Apply Stash…"),
                ("A46", "Create Branch from Stash…"),
                ("A47", "Pop Stash…"),
                ("A48", "Drop Stash…"),
                ("A49", "Copy Stash Name to Clipboard"),
                ("A50", "Copy Stash Hash to Clipboard"),
            ]
        );
        assert_eq!(
            ids_and_labels(MenuSurface::Tag),
            vec![
                ("A51", "View Details"),
                ("A52", "Delete Tag…"),
                ("A53", "Push Tag…"),
                ("A54", "Create Archive"),
                ("A55", "Copy Tag Name to Clipboard"),
            ]
        );
        assert_eq!(
            ids_and_labels(MenuSurface::Uncommitted),
            vec![
                ("A56", "Stash uncommitted changes…"),
                ("A57", "Reset uncommitted changes…"),
                ("A58", "Clean untracked files…"),
                ("A59", "Open Source Control View"),
            ]
        );
    }

    #[test]
    fn column_header_links_and_files_match() {
        assert_eq!(
            ids_and_labels(MenuSurface::ColumnHeader),
            vec![
                ("A60", "Date"),
                ("A60", "Author"),
                ("A60", "Commit"),
                ("A61", "Commit Timestamp Order"),
                ("A61", "Author Timestamp Order"),
                ("A61", "Topological Order"),
            ]
        );
        assert_eq!(
            ids_and_labels(MenuSurface::Link),
            vec![
                ("A62", "Open URL"),
                ("A63", "Follow Internal Link"),
                ("A64", "Copy URL to Clipboard"),
            ]
        );
        assert_eq!(
            ids_and_labels(MenuSurface::FileRow),
            vec![
                ("A65", "View Diff"),
                ("A66", "View File at this Revision"),
                ("A67", "View Diff with Working File"),
                ("A68", "Open File"),
                ("A69", "Mark as Reviewed"),
                ("A70", "Mark as Not Reviewed"),
                ("A71", "Reset File to this Revision…"),
                ("A72", "Copy Absolute File Path to Clipboard"),
                ("A73", "Copy Relative File Path to Clipboard"),
            ]
        );
    }

    #[test]
    fn the_nine_surfaces_cover_a11_through_a73_once() {
        let mut count = 0;
        for surface in MenuSurface::ALL {
            count += super::items(surface, &MenuContext::default()).len();
        }
        // The header surface repeats A60/A61 three times each, so 67 drawn rows
        // cover the 63 numbered inventory rows plus those 4 repeats.
        assert_eq!(count, 12 + 11 + 11 + 6 + 5 + 4 + 6 + 3 + 9);
        assert_eq!(count, 67);
    }

    #[test]
    fn every_dispatch_row_cites_its_dsp_id() {
        for surface in MenuSurface::ALL {
            for item in super::items(surface, &MenuContext::default()) {
                let expected = match &item.action {
                    MenuAction::Dispatch(action) => Some(action.dsp_id()),
                    _ => None,
                };
                assert_eq!(item.dsp, expected, "{} {}", item.id, item.label);
                match item.dsp {
                    Some(dsp) => assert!(dsp.starts_with("DSP-"), "{}", item.label),
                    None => {
                        let ok = matches!(
                            item.action,
                            MenuAction::ToggleColumn(_)
                                | MenuAction::SetColumnOrder(_)
                                | MenuAction::SelectInDropdown(_)
                                | MenuAction::UnselectInDropdown(_)
                                | MenuAction::FollowInternalLink(_)
                        );
                        assert!(ok, "{} has no citation but is not local", item.label);
                    }
                }
            }
        }
    }

    #[test]
    fn shortcuts_are_unique_and_lowercase_per_surface() {
        for surface in MenuSurface::ALL {
            let items = super::items(surface, &MenuContext::default());
            let mut seen = Vec::new();
            for item in &items {
                let key = item.shortcut.expect("shortcut assigned");
                assert!(key.is_ascii_lowercase(), "{key} is lowercase");
                assert!(!seen.contains(&key), "duplicate shortcut {key}");
                seen.push(key);
            }
        }
    }

    #[test]
    fn no_shortcut_shadows_a_navigation_key() {
        // `j`/`k` move the highlight, so a row advertising one would be a lie.
        for surface in MenuSurface::ALL {
            for item in super::items(surface, &MenuContext::default()) {
                let key = item.shortcut.expect("shortcut assigned");
                assert!(
                    !RESERVED_SHORTCUTS.contains(&key),
                    "{} ({}) shadows a nav key",
                    item.id,
                    item.label
                );
            }
        }
        // The letters run a, b, c … and simply skip the two reserved ones.
        let keys: Vec<char> = super::items(MenuSurface::CommitRow, &MenuContext::default())
            .iter()
            .map(|item| item.shortcut.unwrap())
            .collect();
        assert_eq!(
            keys,
            vec!['a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'l', 'm']
        );
    }

    #[test]
    fn classify_ref_handles_every_decoration_shape() {
        assert_eq!(
            classify_ref("HEAD -> main"),
            (MenuSurface::Branch, "main".to_string())
        );
        assert_eq!(
            classify_ref("tag: v1.0"),
            (MenuSurface::Tag, "v1.0".to_string())
        );
        assert_eq!(
            classify_ref("origin/main"),
            (MenuSurface::RemoteBranch, "origin/main".to_string())
        );
        assert_eq!(
            classify_ref("refs/stash"),
            (MenuSurface::Stash, "refs/stash".to_string())
        );
        assert_eq!(
            classify_ref("stash@{0}"),
            (MenuSurface::Stash, "stash@{0}".to_string())
        );
        assert_eq!(
            classify_ref("main"),
            (MenuSurface::Branch, "main".to_string())
        );
        assert_eq!(
            classify_ref("refs/heads/dev"),
            (MenuSurface::Branch, "dev".to_string())
        );
        assert_eq!(
            classify_ref("refs/remotes/origin/dev"),
            (MenuSurface::RemoteBranch, "origin/dev".to_string())
        );
        assert_eq!(
            classify_ref("refs/tags/v2"),
            (MenuSurface::Tag, "v2".to_string())
        );
    }

    #[test]
    fn surface_for_refs_prefers_branch_over_remote_over_tag() {
        let refs = vec![
            "tag: v1".to_string(),
            "origin/main".to_string(),
            "HEAD -> main".to_string(),
        ];
        assert_eq!(
            surface_for_refs(&refs),
            Some((MenuSurface::Branch, "main".to_string()))
        );
        assert_eq!(surface_for_refs(&[]), None);
        assert_eq!(
            surface_for_refs(&["refs/stash".to_string()]),
            Some((MenuSurface::Stash, "refs/stash".to_string()))
        );
    }

    #[test]
    fn split_remote_ref_keeps_slashes_in_the_branch() {
        assert_eq!(
            split_remote_ref("origin/feature/x"),
            ("origin".to_string(), "feature/x".to_string())
        );
        assert_eq!(
            split_remote_ref("main"),
            (String::new(), "main".to_string())
        );
    }

    #[test]
    fn menu_navigation_wraps_and_respects_shortcuts() {
        let mut menu = ContextMenu::open(
            MenuSurface::CommitRow,
            4,
            3,
            MenuContext {
                hash: "abc1234".into(),
                subject: "subject".into(),
                ..Default::default()
            },
        );
        assert_eq!(menu.items.len(), 11);
        assert_eq!(menu.selected, 0);
        menu.move_selection(-1);
        assert_eq!(menu.selected, 10, "wraps backwards");
        menu.move_selection(1);
        assert_eq!(menu.selected, 0, "wraps forwards");
        assert!(menu.select_shortcut('c'));
        assert_eq!(menu.items[menu.selected].label, "Checkout");
        assert!(!menu.select_shortcut('z'));
    }

    #[test]
    fn disabled_rows_are_dropped_for_the_current_branch() {
        let ctx = MenuContext {
            ref_name: "main".into(),
            is_current_branch: true,
            ..Default::default()
        };
        let items = super::items(MenuSurface::Branch, &ctx);
        assert!(!items[0].enabled, "Checkout disabled on current branch");
        assert!(!items[2].enabled, "Delete disabled on current branch");
        assert!(items[1].enabled, "Rename stays enabled");
        assert!(!items[5].enabled, "Push disabled without remotes");

        let ctx = MenuContext {
            ref_name: "main".into(),
            has_remotes: true,
            ..Default::default()
        };
        let items = super::items(MenuSurface::Branch, &ctx);
        assert!(items[0].enabled);
        assert!(items[5].enabled);
    }

    #[test]
    fn column_menu_reflects_visibility_and_order_state() {
        let ctx = MenuContext {
            columns: MenuColumns {
                date: false,
                author: true,
                commit: true,
                order: CommitOrder::Topological,
            },
            ..Default::default()
        };
        let items = super::items(MenuSurface::ColumnHeader, &ctx);
        assert!(!items[0].checked, "Date hidden");
        assert!(items[1].checked);
        assert!(!items[4].checked, "Commit Timestamp order not selected");
        assert!(items[5].checked, "Topological order selected");
        assert_eq!(
            items[5].action,
            MenuAction::SetColumnOrder(CommitOrder::Topological)
        );
    }

    #[test]
    fn action_target_carries_the_row_context() {
        let ctx = MenuContext {
            ref_name: "main".into(),
            hash: "abc1234".into(),
            subject: "subject".into(),
            ..Default::default()
        };
        let target = ctx.action_target();
        assert_eq!(target.ref_name, "main");
        assert_eq!(target.branch, "main");
        assert_eq!(target.hash, "abc1234");
        assert_eq!(target.commit, "abc1234");

        let remote = MenuContext::from_ref(MenuSurface::RemoteBranch, "origin/feature/x");
        assert_eq!(remote.remote, "origin");
        assert_eq!(remote.branch, "feature/x");

        let stash = MenuContext::from_ref(MenuSurface::Stash, "stash@{2}");
        assert_eq!(stash.selector, "stash@{2}");
        assert_eq!(stash.action_target().selector, "stash@{2}");
    }

    #[test]
    fn width_hint_is_clamped() {
        let menu = ContextMenu::open(MenuSurface::CommitRow, 0, 0, MenuContext::default());
        let hint = menu.width_hint();
        assert!((16..=56).contains(&hint), "hint {hint}");
    }
}
