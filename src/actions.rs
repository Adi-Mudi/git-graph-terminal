//! W5 (T2): the context-menu **dispatch map**.
//!
//! Every row of the W5 inventory's `DSP-001..DSP-062` table becomes one
//! [`RefAction`] variant, and [`plan`] is a pure, exhaustive match that turns a
//! `(action, target)` pair into the exact `git` argv the reference
//! implementation runs (transcribed from
//! `vscode-git-graph/src/dataSource.ts`, method by method — the inventory's
//! `command` column names the method).
//!
//! Two properties matter for W5:
//!
//! * **Pure planning.** [`plan`] never touches the filesystem or the process
//!   table. Everything the user could be about to run is visible as a
//!   `Vec<String>` argv, which is what the argv tests assert on.
//! * **Safe execution only.** [`dispatch`] runs git for [`PlanKind::Execute`]
//!   plans (read-only queries plus the non-destructive sync/checkout set the
//!   plan whitelists) and *refuses* to run [`PlanKind::Confirm`] plans: those
//!   carry the real argv but stop at the W5/W6 boundary, so the W6 confirmation
//!   dialog has a single place to hook into.
//!
//! `PlanKind::Local` covers actions that are our own UI state (refresh the
//! model, flip a column, copy to the clipboard) and `PlanKind::OutOfScope`
//! covers the M3 / out-of-scope rows that the inventory still lists — the enum
//! stays exhaustive, the behaviour is documented rather than silently absent.

use std::io::Write;
use std::path::{Path, PathBuf};

use crate::data::{self, GitError};

/// `git reset` mode, mirroring `GitResetMode` in the reference implementation.
///
/// W5 only ever plans `Mixed`; `Soft`/`Hard` are published so the W6 dialog can
/// offer the full ladder without changing this enum's shape.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ResetMode {
    Soft,
    #[default]
    Mixed,
    Hard,
}

impl ResetMode {
    #[allow(dead_code)] // the W6 reset dialog picks the mode; W5 plans Mixed only
    pub fn as_flag(self) -> &'static str {
        match self {
            ResetMode::Soft => "soft",
            ResetMode::Mixed => "mixed",
            ResetMode::Hard => "hard",
        }
    }
}

/// `git push` mode, mirroring `GitPushBranchMode`.
///
/// The force modes are only reachable from the W6 force-push confirmation, so
/// W5 plans `Normal` and publishes the other two.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PushMode {
    #[default]
    Normal,
    ForceWithLease,
    Force,
}

impl PushMode {
    #[allow(dead_code)] // the W6 force-push confirmation reads this
    pub fn as_flag(self) -> Option<&'static str> {
        match self {
            PushMode::Normal => None,
            PushMode::ForceWithLease => Some("--force-with-lease"),
            PushMode::Force => Some("--force"),
        }
    }

    /// A force push can drop commits on the remote, so it needs the W6 gate.
    pub fn is_force(self) -> bool {
        !matches!(self, PushMode::Normal)
    }
}

/// The 62 dispatch rows from the W5 inventory: `(DSP id, command, source method)`.
///
/// The first column is the inventory's `id`, the second its `command`, and the
/// third the `dataSource.ts` method the argv was transcribed from. Keeping the
/// table here lets the coverage test prove the enum really is exhaustive.
#[allow(dead_code)] // read by the coverage test and quoted in `w5-report.md`
pub const DSP_TABLE: [(&str, &str, &str); 62] = [
    ("DSP-001", "addRemote", "addRemote"),
    ("DSP-002", "addTag", "addTag"),
    ("DSP-003", "applyStash", "applyStash"),
    ("DSP-004", "branchFromStash", "branchFromStash"),
    ("DSP-005", "checkoutBranch", "checkoutBranch"),
    ("DSP-006", "checkoutCommit", "checkoutCommit"),
    ("DSP-007", "cherrypickCommit", "cherrypickCommit"),
    ("DSP-008", "cleanUntrackedFiles", "cleanUntrackedFiles"),
    ("DSP-009", "commitDetails", "commitDetails"),
    ("DSP-010", "compareCommits", "compareCommits"),
    ("DSP-011", "copyFilePath", "copyFilePath"),
    ("DSP-012", "copyToClipboard", "copyToClipboard"),
    ("DSP-013", "createArchive", "archive"),
    ("DSP-014", "createBranch", "createBranch"),
    ("DSP-015", "createPullRequest", "createPullRequest"),
    ("DSP-016", "deleteBranch", "deleteBranch"),
    ("DSP-017", "deleteRemote", "deleteRemote"),
    ("DSP-018", "deleteRemoteBranch", "deleteRemoteBranch"),
    ("DSP-019", "deleteTag", "deleteTag"),
    ("DSP-020", "deleteUserDetails", "unsetConfigValue"),
    ("DSP-021", "dropCommit", "dropCommit"),
    ("DSP-022", "dropStash", "dropStash"),
    ("DSP-023", "editRemote", "editRemote"),
    ("DSP-024", "editUserDetails", "setConfigValue"),
    ("DSP-025", "endCodeReview", "endCodeReview"),
    ("DSP-026", "exportRepoConfig", "exportRepoConfig"),
    ("DSP-027", "fetch", "fetch"),
    ("DSP-028", "fetchAvatar", "fetchAvatar"),
    ("DSP-029", "fetchIntoLocalBranch", "fetchIntoLocalBranch"),
    ("DSP-030", "loadCommits", "loadCommits"),
    ("DSP-031", "loadConfig", "loadConfig"),
    ("DSP-032", "loadRepoInfo", "loadRepoInfo"),
    ("DSP-033", "loadRepos", "loadRepos"),
    ("DSP-034", "merge", "merge"),
    ("DSP-035", "openExtensionSettings", "openExtensionSettings"),
    ("DSP-036", "openExternalDirDiff", "openExternalDirDiff"),
    ("DSP-037", "openExternalUrl", "openExternalUrl"),
    ("DSP-038", "openFile", "openFile"),
    ("DSP-039", "openTerminal", "openTerminal"),
    ("DSP-040", "popStash", "popStash"),
    ("DSP-041", "pruneRemote", "pruneRemote"),
    ("DSP-042", "pullBranch", "pullBranch"),
    ("DSP-043", "pushBranch", "pushBranch"),
    ("DSP-044", "pushStash", "pushStash"),
    ("DSP-045", "pushTag", "pushTag"),
    ("DSP-046", "rebase", "rebase"),
    ("DSP-047", "renameBranch", "renameBranch"),
    ("DSP-048", "rescanForRepos", "rescanForRepos"),
    ("DSP-049", "resetFileToRevision", "resetFileToRevision"),
    ("DSP-050", "resetToCommit", "resetToCommit"),
    ("DSP-051", "revertCommit", "revertCommit"),
    ("DSP-052", "setGlobalViewState", "setGlobalViewState"),
    ("DSP-053", "setRepoState", "setRepoState"),
    ("DSP-054", "setWorkspaceViewState", "setWorkspaceViewState"),
    ("DSP-055", "showErrorMessage", "showErrorMessage"),
    ("DSP-056", "startCodeReview", "startCodeReview"),
    ("DSP-057", "tagDetails", "getTagDetails"),
    ("DSP-058", "updateCodeReview", "updateCodeReview"),
    ("DSP-059", "viewDiff", "viewDiff"),
    (
        "DSP-060",
        "viewDiffWithWorkingFile",
        "viewDiffWithWorkingFile",
    ),
    ("DSP-061", "viewFileAtRevision", "viewFileAtRevision"),
    ("DSP-062", "viewScm", "viewScm"),
];

/// What kind of thing the user is acting on. One flat parameter bag, so
/// [`plan`] can stay a pure function while every DSP row still finds the inputs
/// it needs. Unused fields are simply left at their default.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ActionTarget {
    /// Branch / tag / remote-ref name, e.g. `main` or `origin/main`.
    pub ref_name: String,
    pub hash: String,
    pub subject: String,
    pub remote: String,
    pub branch: String,
    pub local_branch: String,
    pub selector: String,
    pub path: String,
    pub message: String,
    pub new_name: String,
    pub name: String,
    pub url: String,
    pub commit: String,
    pub key: String,
    pub value: String,
    pub location: String,
    pub from: String,
    pub to: String,
    /// Text a `Copy …` action should place on the clipboard.
    pub clipboard: String,
    pub reset_mode: ResetMode,
    pub push_mode: PushMode,
    pub force: bool,
    pub directories: bool,
    pub include_untracked: bool,
    pub reinstate_index: bool,
    pub record_origin: bool,
    pub no_commit: bool,
    pub set_upstream: bool,
    pub prune: bool,
    pub prune_tags: bool,
    pub checkout: bool,
    pub no_ff: bool,
    pub squash: bool,
    pub ignore_date: bool,
    pub annotated: bool,
}

impl ActionTarget {
    /// The first non-empty value among `values`, or `""`.
    fn first(values: &[&str]) -> String {
        values
            .iter()
            .find(|value| !value.is_empty())
            .map(|value| (*value).to_string())
            .unwrap_or_default()
    }
}

/// How a plan is allowed to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanKind {
    /// Safe to run immediately (read-only queries, checkout, non-destructive sync).
    Execute,
    /// Real argv, but gated: W5 stops here and W6 adds the confirmation dialog.
    Confirm(&'static str),
    /// UI/state-only work (refresh, column toggle, clipboard write); no git argv.
    Local,
    /// Listed in the inventory but explicitly out of scope for W5.
    OutOfScope,
}

impl PlanKind {
    #[allow(dead_code)] // quoted by the W5 coverage report, not by the status bar
    pub fn label(self) -> &'static str {
        match self {
            PlanKind::Execute => "execute",
            PlanKind::Confirm(_) => "confirm",
            PlanKind::Local => "local",
            PlanKind::OutOfScope => "out-of-scope",
        }
    }

    /// Is this plan waiting on the W6 confirmation dialog?
    #[allow(dead_code)] // the W6 dialog calls this; `activate_menu` matches instead
    pub fn is_gated(self) -> bool {
        matches!(self, PlanKind::Confirm(_))
    }
}

/// A fully-resolved intention: what it is, which inventory rows cite it, the
/// exact `git` argv it would run, and whether W5 may run it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionPlan {
    /// `DSP-nnn` id from the inventory.
    pub dsp: &'static str,
    /// Menu/action id from the inventory (`A11`..`A73`), or `—` when the row is
    /// not reachable from a context-menu surface.
    pub act: &'static str,
    /// Inventory `command` name.
    pub command: &'static str,
    /// Human label used by the status bar and the confirm stub.
    pub label: &'static str,
    /// The argv passed to `git -C <repo> …`.
    pub argv: Vec<String>,
    pub kind: PlanKind,
    /// Data a `Local` copy action hands to the clipboard.
    pub payload: Option<String>,
}

impl ActionPlan {
    fn new(
        dsp: &'static str,
        act: &'static str,
        command: &'static str,
        label: &'static str,
        kind: PlanKind,
    ) -> Self {
        ActionPlan {
            dsp,
            act,
            command,
            label,
            argv: Vec::new(),
            kind,
            payload: None,
        }
    }

    fn argv(mut self, argv: &[&str]) -> Self {
        self.argv = argv.iter().map(|part| (*part).to_string()).collect();
        self
    }

    fn owned_argv(mut self, argv: Vec<String>) -> Self {
        self.argv = argv;
        self
    }

    fn payload(mut self, text: &str) -> Self {
        self.payload = Some(text.to_string());
        self
    }

    /// `true` when the plan mutates the repository (used by the tests and by
    /// the status bar to decide whether a refresh is needed).
    pub fn mutates(&self) -> bool {
        matches!(self.kind, PlanKind::Execute | PlanKind::Confirm(_)) && !self.argv.is_empty()
    }
}

/// Every dispatch row in the inventory, as an exhaustive enum.
#[allow(dead_code)] // the parts no menu row reaches yet are the W6 dialog's remit
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefAction {
    AddRemote,
    AddTag,
    ApplyStash,
    BranchFromStash,
    CheckoutBranch,
    CheckoutCommit,
    CherrypickCommit,
    CleanUntrackedFiles,
    CommitDetails,
    CompareCommits,
    CopyFilePath,
    CopyToClipboard,
    CreateArchive,
    CreateBranch,
    CreatePullRequest,
    DeleteBranch,
    DeleteRemote,
    DeleteRemoteBranch,
    DeleteTag,
    DeleteUserDetails,
    DropCommit,
    DropStash,
    EditRemote,
    EditUserDetails,
    EndCodeReview,
    ExportRepoConfig,
    Fetch,
    FetchAvatar,
    FetchIntoLocalBranch,
    LoadCommits,
    LoadConfig,
    LoadRepoInfo,
    LoadRepos,
    Merge,
    OpenExtensionSettings,
    OpenExternalDirDiff,
    OpenExternalUrl,
    OpenFile,
    OpenTerminal,
    PopStash,
    PruneRemote,
    PullBranch,
    PushBranch,
    PushStash,
    PushTag,
    Rebase,
    RenameBranch,
    RescanForRepos,
    ResetFileToRevision,
    ResetToCommit,
    RevertCommit,
    SetGlobalViewState,
    SetRepoState,
    SetWorkspaceViewState,
    ShowErrorMessage,
    StartCodeReview,
    TagDetails,
    UpdateCodeReview,
    ViewDiff,
    ViewDiffWithWorkingFile,
    ViewFileAtRevision,
    ViewScm,
}

impl RefAction {
    /// All 62 variants, in `DSP-001..DSP-062` order.
    #[allow(dead_code)] // read by the coverage test
    pub const ALL: [RefAction; 62] = [
        RefAction::AddRemote,
        RefAction::AddTag,
        RefAction::ApplyStash,
        RefAction::BranchFromStash,
        RefAction::CheckoutBranch,
        RefAction::CheckoutCommit,
        RefAction::CherrypickCommit,
        RefAction::CleanUntrackedFiles,
        RefAction::CommitDetails,
        RefAction::CompareCommits,
        RefAction::CopyFilePath,
        RefAction::CopyToClipboard,
        RefAction::CreateArchive,
        RefAction::CreateBranch,
        RefAction::CreatePullRequest,
        RefAction::DeleteBranch,
        RefAction::DeleteRemote,
        RefAction::DeleteRemoteBranch,
        RefAction::DeleteTag,
        RefAction::DeleteUserDetails,
        RefAction::DropCommit,
        RefAction::DropStash,
        RefAction::EditRemote,
        RefAction::EditUserDetails,
        RefAction::EndCodeReview,
        RefAction::ExportRepoConfig,
        RefAction::Fetch,
        RefAction::FetchAvatar,
        RefAction::FetchIntoLocalBranch,
        RefAction::LoadCommits,
        RefAction::LoadConfig,
        RefAction::LoadRepoInfo,
        RefAction::LoadRepos,
        RefAction::Merge,
        RefAction::OpenExtensionSettings,
        RefAction::OpenExternalDirDiff,
        RefAction::OpenExternalUrl,
        RefAction::OpenFile,
        RefAction::OpenTerminal,
        RefAction::PopStash,
        RefAction::PruneRemote,
        RefAction::PullBranch,
        RefAction::PushBranch,
        RefAction::PushStash,
        RefAction::PushTag,
        RefAction::Rebase,
        RefAction::RenameBranch,
        RefAction::RescanForRepos,
        RefAction::ResetFileToRevision,
        RefAction::ResetToCommit,
        RefAction::RevertCommit,
        RefAction::SetGlobalViewState,
        RefAction::SetRepoState,
        RefAction::SetWorkspaceViewState,
        RefAction::ShowErrorMessage,
        RefAction::StartCodeReview,
        RefAction::TagDetails,
        RefAction::UpdateCodeReview,
        RefAction::ViewDiff,
        RefAction::ViewDiffWithWorkingFile,
        RefAction::ViewFileAtRevision,
        RefAction::ViewScm,
    ];

    /// The inventory `DSP-nnn` id.
    #[allow(dead_code)] // read by the coverage test and the W5 report
    pub fn dsp_id(self) -> &'static str {
        DSP_TABLE[self as usize].0
    }

    /// The inventory `command` name.
    #[allow(dead_code)] // read by the coverage test and the W5 report
    pub fn command(self) -> &'static str {
        DSP_TABLE[self as usize].1
    }
}

/// Build the plan for `action` against `target`. Pure: no I/O, no git.
pub fn plan(action: RefAction, target: &ActionTarget) -> ActionPlan {
    use RefAction::*;

    // Small local helpers keep each arm a single line.
    let name = || ActionTarget::first(&[&target.name, &target.ref_name, &target.branch]);
    let obj = || ActionTarget::first(&[&target.hash, &target.commit, &target.ref_name]);

    match action {
        AddRemote => {
            let mut argv = vec![
                "remote".to_string(),
                "add".to_string(),
                name(),
                target.url.clone(),
            ];
            argv.retain(|part| !part.is_empty());
            ActionPlan::new(
                "DSP-001",
                "—",
                "addRemote",
                "Add Remote",
                gated("adds/edits remote configuration"),
            )
            .owned_argv(argv)
        }
        AddTag => {
            let mut argv = vec!["tag".to_string()];
            if target.force {
                argv.push("-f".to_string());
            }
            if target.annotated {
                argv.push("-a".to_string());
                argv.push(name());
                argv.push("-m".to_string());
                argv.push(target.message.clone());
            } else {
                argv.push(name());
            }
            argv.push(target.hash.clone());
            argv.retain(|part| !part.is_empty());
            ActionPlan::new(
                "DSP-002",
                "A23",
                "addTag",
                "Add Tag",
                gated("creates a ref"),
            )
            .owned_argv(argv)
        }
        ApplyStash => {
            let mut argv = vec!["stash".to_string(), "apply".to_string()];
            if target.reinstate_index {
                argv.push("--index".to_string());
            }
            argv.push(target.selector.clone());
            argv.retain(|part| !part.is_empty());
            ActionPlan::new(
                "DSP-003",
                "A45",
                "applyStash",
                "Apply Stash",
                gated("changes the working tree"),
            )
            .owned_argv(argv)
        }
        BranchFromStash => {
            let argv = vec![
                "stash".to_string(),
                "branch".to_string(),
                target.branch.clone(),
                target.selector.clone(),
            ];
            ActionPlan::new(
                "DSP-004",
                "A46",
                "branchFromStash",
                "Create Branch from Stash",
                gated("creates a ref"),
            )
            .owned_argv(argv.into_iter().filter(|p| !p.is_empty()).collect())
        }
        CheckoutBranch => {
            let (argv, label) = if target.local_branch.is_empty() && target.remote.is_empty() {
                (
                    vec!["checkout".to_string(), target.branch.clone()],
                    "Checkout Branch",
                )
            } else if target.local_branch.is_empty() {
                (
                    vec![
                        "checkout".to_string(),
                        "-b".to_string(),
                        target.branch.clone(),
                        target.ref_name.clone(),
                    ],
                    "Checkout Remote Branch",
                )
            } else {
                (
                    vec![
                        "checkout".to_string(),
                        "-b".to_string(),
                        target.local_branch.clone(),
                        format!("{}/{}", target.remote, target.branch),
                    ],
                    "Checkout Remote Branch",
                )
            };
            ActionPlan::new("DSP-005", "A11", "checkoutBranch", label, PlanKind::Execute)
                .owned_argv(argv.into_iter().filter(|p| !p.is_empty()).collect())
        }
        CheckoutCommit => ActionPlan::new(
            "DSP-006",
            "A25",
            "checkoutCommit",
            "Checkout Commit",
            PlanKind::Execute,
        )
        .argv(&["checkout", &obj()]),
        CherrypickCommit => {
            let mut argv = vec!["cherry-pick".to_string()];
            if target.no_commit {
                argv.push("--no-commit".to_string());
            }
            if target.record_origin {
                argv.push("-x".to_string());
            }
            argv.push(target.hash.clone());
            argv.retain(|part| !part.is_empty());
            ActionPlan::new(
                "DSP-007",
                "A26",
                "cherrypickCommit",
                "Cherry Pick",
                gated("changes the working tree"),
            )
            .owned_argv(argv)
        }
        CleanUntrackedFiles => {
            let flag = if target.directories { "-fd" } else { "-f" };
            ActionPlan::new(
                "DSP-008",
                "A58",
                "cleanUntrackedFiles",
                "Clean Untracked Files",
                gated("deletes untracked files"),
            )
            .argv(&["clean", flag])
        }
        CommitDetails => ActionPlan::new(
            "DSP-009",
            "A51",
            "commitDetails",
            "Commit Details",
            PlanKind::Execute,
        )
        .argv(&["show", "--stat", "--patch", &obj()]),
        CompareCommits => ActionPlan::new(
            "DSP-010",
            "—",
            "compareCommits",
            "Compare Commits",
            PlanKind::Execute,
        )
        .argv(&["diff", "--find-renames", &target.from, &target.to]),
        CopyFilePath => {
            let text = if target.clipboard.is_empty() {
                target.path.clone()
            } else {
                target.clipboard.clone()
            };
            ActionPlan::new(
                "DSP-011",
                "A72",
                "copyFilePath",
                "Copy File Path",
                PlanKind::Local,
            )
            .payload(&text)
        }
        CopyToClipboard => ActionPlan::new(
            "DSP-012",
            "A21",
            "copyToClipboard",
            "Copy to Clipboard",
            PlanKind::Local,
        )
        .payload(&target.clipboard),
        CreateArchive => ActionPlan::new(
            "DSP-013",
            "A18",
            "archive",
            "Create Archive",
            PlanKind::OutOfScope,
        ),
        CreateBranch => {
            let mut argv = if target.checkout {
                vec!["checkout".to_string(), "-b".to_string()]
            } else {
                vec!["branch".to_string()]
            };
            if target.force && !target.checkout {
                argv.push("-f".to_string());
            }
            argv.push(name());
            argv.push(target.hash.clone());
            argv.retain(|part| !part.is_empty());
            ActionPlan::new(
                "DSP-014",
                "A24",
                "createBranch",
                "Create Branch",
                gated("creates a ref"),
            )
            .owned_argv(argv)
        }
        CreatePullRequest => ActionPlan::new(
            "DSP-015",
            "A17",
            "createPullRequest",
            "Create Pull Request",
            PlanKind::OutOfScope,
        ),
        DeleteBranch => ActionPlan::new(
            "DSP-016",
            "A13",
            "deleteBranch",
            "Delete Branch",
            gated("deletes a ref"),
        )
        .argv(&["branch", if target.force { "-D" } else { "-d" }, &name()]),
        DeleteRemote => ActionPlan::new(
            "DSP-017",
            "—",
            "deleteRemote",
            "Delete Remote",
            gated("removes remote configuration"),
        )
        .argv(&["remote", "remove", &name()]),
        DeleteRemoteBranch => ActionPlan::new(
            "DSP-018",
            "A35",
            "deleteRemoteBranch",
            "Delete Remote Branch",
            gated("deletes a ref on the remote"),
        )
        .argv(&["push", &target.remote, "--delete", &target.branch]),
        DeleteTag => ActionPlan::new(
            "DSP-019",
            "A52",
            "deleteTag",
            "Delete Tag",
            gated("deletes a ref"),
        )
        .argv(&["tag", "-d", &name()]),
        DeleteUserDetails => ActionPlan::new(
            "DSP-020",
            "—",
            "unsetConfigValue",
            "Unset Config Value",
            gated("edits configuration"),
        )
        .argv(&["config", "--unset-all", &target.key]),
        DropCommit => ActionPlan::new(
            "DSP-021",
            "A28",
            "dropCommit",
            "Drop Commit",
            gated("rewrites history"),
        )
        .argv(&["rebase", "--onto", &format!("{}^", obj()), &obj()]),
        DropStash => ActionPlan::new(
            "DSP-022",
            "A48",
            "dropStash",
            "Drop Stash",
            gated("drops a stash"),
        )
        .argv(&["stash", "drop", &target.selector]),
        EditRemote => ActionPlan::new(
            "DSP-023",
            "—",
            "editRemote",
            "Edit Remote",
            gated("edits remote configuration"),
        )
        .argv(&["remote", "rename", &target.name, &target.new_name]),
        EditUserDetails => ActionPlan::new(
            "DSP-024",
            "—",
            "setConfigValue",
            "Set Config Value",
            gated("edits configuration"),
        )
        .argv(&["config", &target.key, &target.value]),
        EndCodeReview => ActionPlan::new(
            "DSP-025",
            "—",
            "endCodeReview",
            "End Code Review",
            PlanKind::OutOfScope,
        ),
        ExportRepoConfig => ActionPlan::new(
            "DSP-026",
            "—",
            "exportRepoConfig",
            "Export Repo Config",
            PlanKind::Local,
        )
        .payload(&target.path),
        Fetch => {
            let mut argv = vec!["fetch".to_string()];
            if target.remote.is_empty() {
                argv.push("--all".to_string());
            } else {
                argv.push(target.remote.clone());
            }
            if target.prune {
                argv.push("--prune".to_string());
            }
            if target.prune_tags && target.prune {
                argv.push("--prune-tags".to_string());
            }
            ActionPlan::new("DSP-027", "—", "fetch", "Fetch", PlanKind::Execute).owned_argv(argv)
        }
        FetchAvatar => ActionPlan::new(
            "DSP-028",
            "—",
            "fetchAvatar",
            "Fetch Avatar",
            PlanKind::OutOfScope,
        ),
        FetchIntoLocalBranch => {
            let mut argv = vec!["fetch".to_string()];
            if target.force {
                argv.push("-f".to_string());
            }
            argv.push(target.remote.clone());
            argv.push(format!("{}:{}", target.branch, target.local_branch));
            argv.retain(|part| !part.is_empty());
            let kind = if target.force {
                gated("overwrites a local branch")
            } else {
                PlanKind::Execute
            };
            ActionPlan::new(
                "DSP-029",
                "A36",
                "fetchIntoLocalBranch",
                "Fetch into Local Branch",
                kind,
            )
            .owned_argv(argv)
        }
        LoadCommits => ActionPlan::new(
            "DSP-030",
            "—",
            "loadCommits",
            "Load Commits",
            PlanKind::Local,
        ),
        LoadConfig => ActionPlan::new("DSP-031", "—", "loadConfig", "Load Config", PlanKind::Local),
        LoadRepoInfo => ActionPlan::new(
            "DSP-032",
            "—",
            "loadRepoInfo",
            "Load Repo Info",
            PlanKind::Local,
        ),
        LoadRepos => ActionPlan::new("DSP-033", "—", "loadRepos", "Load Repos", PlanKind::Local),
        Merge => {
            let mut argv = vec!["merge".to_string(), obj()];
            if target.squash {
                argv.push("--squash".to_string());
            } else if target.no_ff {
                argv.push("--no-ff".to_string());
            }
            if target.no_commit {
                argv.push("--no-commit".to_string());
            }
            argv.retain(|part| !part.is_empty());
            ActionPlan::new(
                "DSP-034",
                "A14",
                "merge",
                "Merge",
                gated("changes the working tree"),
            )
            .owned_argv(argv)
        }
        OpenExtensionSettings => ActionPlan::new(
            "DSP-035",
            "—",
            "openExtensionSettings",
            "Open Extension Settings",
            PlanKind::OutOfScope,
        ),
        OpenExternalDirDiff => ActionPlan::new(
            "DSP-036",
            "—",
            "openExternalDirDiff",
            "Open External Dir Diff",
            PlanKind::OutOfScope,
        ),
        OpenExternalUrl => ActionPlan::new(
            "DSP-037",
            "A22",
            "openExternalUrl",
            "Open URL",
            PlanKind::OutOfScope,
        )
        .payload(&target.url),
        OpenFile => ActionPlan::new(
            "DSP-038",
            "A68",
            "openFile",
            "Open File",
            PlanKind::OutOfScope,
        )
        .payload(&target.path),
        OpenTerminal => ActionPlan::new(
            "DSP-039",
            "—",
            "openTerminal",
            "Open Terminal",
            PlanKind::OutOfScope,
        ),
        PopStash => {
            let mut argv = vec!["stash".to_string(), "pop".to_string()];
            if target.reinstate_index {
                argv.push("--index".to_string());
            }
            argv.push(target.selector.clone());
            argv.retain(|part| !part.is_empty());
            ActionPlan::new(
                "DSP-040",
                "A47",
                "popStash",
                "Pop Stash",
                gated("changes the working tree and drops the stash"),
            )
            .owned_argv(argv)
        }
        PruneRemote => ActionPlan::new(
            "DSP-041",
            "—",
            "pruneRemote",
            "Prune Remote",
            gated("deletes remote-tracking refs"),
        )
        .argv(&["remote", "prune", &name()]),
        PullBranch => {
            let mut argv = vec![
                "pull".to_string(),
                target.remote.clone(),
                target.branch.clone(),
            ];
            if target.squash {
                argv.push("--squash".to_string());
            } else if target.no_ff {
                argv.push("--no-ff".to_string());
            }
            argv.retain(|part| !part.is_empty());
            ActionPlan::new("DSP-042", "A38", "pullBranch", "Pull", PlanKind::Execute)
                .owned_argv(argv)
        }
        PushBranch => {
            let mut argv = vec![
                "push".to_string(),
                target.remote.clone(),
                target.branch.clone(),
            ];
            if target.set_upstream {
                argv.push("--set-upstream".to_string());
            }
            if let Some(flag) = target.push_mode.as_flag() {
                argv.push(flag.to_string());
            }
            argv.retain(|part| !part.is_empty());
            let kind = if target.push_mode.is_force() {
                gated("force push overwrites remote history")
            } else {
                PlanKind::Execute
            };
            ActionPlan::new("DSP-043", "A16", "pushBranch", "Push Branch", kind).owned_argv(argv)
        }
        PushStash => {
            let mut argv = vec!["stash".to_string(), "push".to_string()];
            if target.include_untracked {
                argv.push("--include-untracked".to_string());
            }
            if !target.message.is_empty() {
                argv.push("--message".to_string());
                argv.push(target.message.clone());
            }
            ActionPlan::new(
                "DSP-044",
                "A56",
                "pushStash",
                "Stash Uncommitted Changes",
                gated("changes the working tree"),
            )
            .owned_argv(argv)
        }
        PushTag => ActionPlan::new("DSP-045", "A53", "pushTag", "Push Tag", PlanKind::Execute)
            .argv(&["push", &target.remote, &name()]),
        Rebase => {
            let mut argv = vec!["rebase".to_string()];
            if target.ignore_date {
                argv.push("--ignore-date".to_string());
            }
            argv.push(obj());
            argv.retain(|part| !part.is_empty());
            ActionPlan::new(
                "DSP-046",
                "A15",
                "rebase",
                "Rebase",
                gated("rewrites history"),
            )
            .owned_argv(argv)
        }
        RenameBranch => ActionPlan::new(
            "DSP-047",
            "A12",
            "renameBranch",
            "Rename Branch",
            gated("renames a ref"),
        )
        .argv(&["branch", "-m", &target.name, &target.new_name]),
        RescanForRepos => ActionPlan::new(
            "DSP-048",
            "—",
            "rescanForRepos",
            "Rescan for Repos",
            PlanKind::Local,
        ),
        ResetFileToRevision => ActionPlan::new(
            "DSP-049",
            "A71",
            "resetFileToRevision",
            "Reset File to Revision",
            gated("overwrites a working file"),
        )
        .argv(&["checkout", &target.hash, "--", &target.path]),
        ResetToCommit => ActionPlan::new(
            "DSP-050",
            "A31",
            "resetToCommit",
            "Reset to Commit",
            gated("moves the branch pointer"),
        )
        .argv(&[
            "reset",
            &format!("--{}", target.reset_mode.as_flag()),
            &obj(),
        ]),
        RevertCommit => ActionPlan::new(
            "DSP-051",
            "A27",
            "revertCommit",
            "Revert Commit",
            gated("changes the working tree"),
        )
        .argv(&["revert", "--no-edit", &obj()]),
        SetGlobalViewState => ActionPlan::new(
            "DSP-052",
            "—",
            "setGlobalViewState",
            "Set Global View State",
            PlanKind::Local,
        ),
        SetRepoState => ActionPlan::new(
            "DSP-053",
            "—",
            "setRepoState",
            "Set Repo State",
            PlanKind::Local,
        ),
        SetWorkspaceViewState => ActionPlan::new(
            "DSP-054",
            "—",
            "setWorkspaceViewState",
            "Set Workspace View State",
            PlanKind::Local,
        ),
        ShowErrorMessage => ActionPlan::new(
            "DSP-055",
            "—",
            "showErrorMessage",
            "Show Error Message",
            PlanKind::Local,
        ),
        StartCodeReview => ActionPlan::new(
            "DSP-056",
            "—",
            "startCodeReview",
            "Start Code Review",
            PlanKind::OutOfScope,
        ),
        TagDetails => ActionPlan::new(
            "DSP-057",
            "A51",
            "getTagDetails",
            "Tag Details",
            PlanKind::Execute,
        )
        .argv(&["for-each-ref", &name(), "--format=%(contents)"]),
        UpdateCodeReview => ActionPlan::new(
            "DSP-058",
            "—",
            "updateCodeReview",
            "Update Code Review",
            PlanKind::OutOfScope,
        ),
        ViewDiff => ActionPlan::new(
            "DSP-059",
            "A65",
            "viewDiff",
            "View Diff",
            PlanKind::OutOfScope,
        ),
        ViewDiffWithWorkingFile => ActionPlan::new(
            "DSP-060",
            "A67",
            "viewDiffWithWorkingFile",
            "View Diff with Working File",
            PlanKind::OutOfScope,
        ),
        ViewFileAtRevision => ActionPlan::new(
            "DSP-061",
            "A66",
            "viewFileAtRevision",
            "View File at Revision",
            PlanKind::OutOfScope,
        ),
        ViewScm => ActionPlan::new(
            "DSP-062",
            "A59",
            "viewScm",
            "Open Source Control View",
            PlanKind::OutOfScope,
        ),
    }
}

fn gated(reason: &'static str) -> PlanKind {
    PlanKind::Confirm(reason)
}

/// What actually happened when a plan was dispatched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DispatchOutcome {
    /// The plan ran; `stdout` is what git printed (possibly empty).
    Executed { plan: ActionPlan, stdout: String },
    /// The plan is gated on the W6 confirmation dialog and was *not* run.
    NeedsConfirm(ActionPlan),
    /// UI/state work (clipboard write, refresh) completed locally.
    Local { plan: ActionPlan, detail: String },
    /// The row is out of scope for W5.
    Skipped(ActionPlan),
    /// git refused the command; the plan's argv is still available.
    Failed { plan: ActionPlan, error: GitError },
}

impl DispatchOutcome {
    pub fn plan(&self) -> &ActionPlan {
        match self {
            DispatchOutcome::Executed { plan, .. }
            | DispatchOutcome::NeedsConfirm(plan)
            | DispatchOutcome::Local { plan, .. }
            | DispatchOutcome::Skipped(plan)
            | DispatchOutcome::Failed { plan, .. } => plan,
        }
    }

    /// One-line summary for the status bar.
    pub fn status_line(&self) -> String {
        let plan = self.plan();
        match self {
            DispatchOutcome::Executed { stdout, .. } => {
                if stdout.is_empty() {
                    format!("{}: ok", plan.label)
                } else {
                    let first = stdout.lines().next().unwrap_or("").trim();
                    format!("{}: {}", plan.label, truncate(first, 60))
                }
            }
            DispatchOutcome::NeedsConfirm(_) => {
                format!("{}: needs confirmation (W6 dialog)", plan.label)
            }
            DispatchOutcome::Local { detail, .. } => format!("{}: {}", plan.label, detail),
            DispatchOutcome::Skipped(_) => format!("{}: out of scope in W5", plan.label),
            DispatchOutcome::Failed { error, .. } => {
                format!(
                    "{}: failed ({})",
                    plan.label,
                    truncate(&error.to_string(), 60)
                )
            }
        }
    }
}

fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let cut: String = text.chars().take(max.saturating_sub(1)).collect();
    format!("{cut}…")
}

/// Serialises tests that mutate the process environment (currently only
/// `GGT_CLIPBOARD_FILE`). Test-only: the environment is global, so a parallel
/// `set_var`/`remove_var` pair from one test can otherwise strip an override out
/// from under another. `app.rs`'s copy-row tests share this lock.
#[cfg(test)]
pub(crate) static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Where a clipboard write lands when OSC 52 is unavailable (or disabled).
pub fn clipboard_path() -> PathBuf {
    if let Ok(path) = std::env::var("GGT_CLIPBOARD_FILE") {
        if !path.is_empty() {
            return PathBuf::from(path);
        }
    }
    if let Ok(dir) = std::env::var("XDG_RUNTIME_DIR") {
        if !dir.is_empty() {
            return PathBuf::from(dir).join("ggt-clipboard.txt");
        }
    }
    PathBuf::from("/tmp/ggt-clipboard.txt")
}

/// Write `text` to the clipboard fallback file. Always succeeds as far as the
/// user is concerned: the terminal OSC 52 escape is best-effort on top.
pub fn write_clipboard(text: &str) -> std::io::Result<PathBuf> {
    let path = clipboard_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    std::fs::write(&path, text)?;
    Ok(path)
}

/// Emit the OSC 52 clipboard escape. Only meaningful on a real terminal, so it
/// is opt-in via `GGT_OSC52=1` (the pty evidence sets it) and never called by
/// the unit tests.
pub fn emit_osc52(text: &str) -> std::io::Result<()> {
    let mut stdout = std::io::stdout();
    write!(stdout, "\x1b]52;c;{}\x07", base64(text))?;
    stdout.flush()
}

/// Minimal base64 (`A-Za-z0-9+/` with `=` padding) so OSC 52 needs no crate.
pub fn base64(input: &str) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(ALPHABET[((triple >> 18) & 0x3f) as usize] as char);
        out.push(ALPHABET[((triple >> 12) & 0x3f) as usize] as char);
        if chunk.len() > 1 {
            out.push(ALPHABET[((triple >> 6) & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(ALPHABET[(triple & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

/// Resolve a plan and, when W5 is allowed to, run it.
///
/// This is the single place where "what would git run" (the pure [`plan`])
/// meets "should we run it": `Execute` runs, `Confirm` stops at the gate,
/// `Local` writes the clipboard / refreshes, `OutOfScope` is a documented skip.
pub fn dispatch(repo: &Path, action: RefAction, target: &ActionTarget) -> DispatchOutcome {
    let plan = plan(action, target);

    match plan.kind {
        PlanKind::Execute => {
            let args: Vec<&str> = plan.argv.iter().map(String::as_str).collect();
            match data::run_git(repo, &args) {
                Ok(stdout) => DispatchOutcome::Executed { plan, stdout },
                Err(error) => DispatchOutcome::Failed { plan, error },
            }
        }
        PlanKind::Confirm(_) => DispatchOutcome::NeedsConfirm(plan),
        PlanKind::Local => {
            let detail = match plan.payload.clone() {
                Some(text) => match write_clipboard(&text) {
                    Ok(path) => format!("copied to {}", path.display()),
                    Err(error) => format!("clipboard failed: {error}"),
                },
                None => "ok".to_string(),
            };
            DispatchOutcome::Local { plan, detail }
        }
        PlanKind::OutOfScope => DispatchOutcome::Skipped(plan),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target() -> ActionTarget {
        ActionTarget {
            ref_name: "origin/main".to_string(),
            hash: "abc1234".to_string(),
            subject: "subject".to_string(),
            remote: "origin".to_string(),
            branch: "main".to_string(),
            local_branch: "main".to_string(),
            selector: "stash@{0}".to_string(),
            path: "src/lib.rs".to_string(),
            message: "msg".to_string(),
            new_name: "renamed".to_string(),
            name: "main".to_string(),
            url: "https://example.test/x".to_string(),
            commit: "abc1234".to_string(),
            key: "user.name".to_string(),
            value: "Ada".to_string(),
            location: "local".to_string(),
            from: "aaa".to_string(),
            to: "bbb".to_string(),
            clipboard: "copied text".to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn dsp_table_matches_the_inventory() {
        assert_eq!(DSP_TABLE.len(), 62);
        for (index, (id, _, _)) in DSP_TABLE.iter().enumerate() {
            let expected = format!("DSP-{:03}", index + 1);
            assert_eq!(*id, expected, "row {index} id");
        }
    }

    #[test]
    fn every_dsp_row_has_exactly_one_ref_action_variant() {
        assert_eq!(RefAction::ALL.len(), 62);
        for (index, action) in RefAction::ALL.iter().enumerate() {
            assert_eq!(action.dsp_id(), DSP_TABLE[index].0);
            assert_eq!(action.command(), DSP_TABLE[index].1);
        }
        // `dsp_id` uses the enum discriminant, so ordering must line up too.
        for (index, action) in RefAction::ALL.iter().enumerate() {
            assert_eq!(*action as usize, index, "{:?} discriminant", action);
        }
    }

    #[test]
    fn checkout_branch_and_commit_argv() {
        // A local branch: no remote, no new local name.
        let local = ActionTarget {
            ref_name: "main".to_string(),
            remote: String::new(),
            local_branch: String::new(),
            branch: "main".to_string(),
            ..target()
        };
        let plan = super::plan(RefAction::CheckoutBranch, &local);
        assert_eq!(plan.argv, vec!["checkout", "main"]);
        assert_eq!(plan.kind, PlanKind::Execute);
        assert_eq!(plan.dsp, "DSP-005");
        assert_eq!(plan.act, "A11");

        // Remote branch checkout creates a local tracking branch.
        let remote = ActionTarget {
            local_branch: String::new(),
            ..target()
        };
        let plan = super::plan(RefAction::CheckoutBranch, &remote);
        assert_eq!(plan.argv, vec!["checkout", "-b", "main", "origin/main"]);

        // Explicit local name.
        let named = ActionTarget {
            local_branch: "mine".to_string(),
            ..target()
        };
        assert_eq!(
            super::plan(RefAction::CheckoutBranch, &named).argv,
            vec!["checkout", "-b", "mine", "origin/main"]
        );

        assert_eq!(
            super::plan(RefAction::CheckoutCommit, &target()).argv,
            vec!["checkout", "abc1234"]
        );
    }

    #[test]
    fn destructive_ops_build_real_argv_but_are_gated() {
        let cases: Vec<(RefAction, Vec<&str>)> = vec![
            (RefAction::DeleteBranch, vec!["branch", "-d", "main"]),
            (
                RefAction::DropCommit,
                vec!["rebase", "--onto", "abc1234^", "abc1234"],
            ),
            (
                RefAction::ResetToCommit,
                vec!["reset", "--mixed", "abc1234"],
            ),
            (RefAction::CleanUntrackedFiles, vec!["clean", "-f"]),
            (RefAction::DropStash, vec!["stash", "drop", "stash@{0}"]),
            (
                RefAction::RenameBranch,
                vec!["branch", "-m", "main", "renamed"],
            ),
            (RefAction::Merge, vec!["merge", "abc1234"]),
            (RefAction::Rebase, vec!["rebase", "abc1234"]),
            (
                RefAction::RevertCommit,
                vec!["revert", "--no-edit", "abc1234"],
            ),
            (RefAction::CherrypickCommit, vec!["cherry-pick", "abc1234"]),
            (RefAction::AddTag, vec!["tag", "main", "abc1234"]),
        ];
        for (action, expected) in cases {
            let plan = super::plan(action, &target());
            assert_eq!(plan.argv, expected, "{action:?} argv");
            assert!(plan.kind.is_gated(), "{action:?} must be gated");
            assert!(plan.mutates(), "{action:?} mutates");
        }
    }

    #[test]
    fn force_flags_flip_safe_ops_into_the_gate() {
        let normal = ActionTarget {
            push_mode: PushMode::Normal,
            ..target()
        };
        let plan = super::plan(RefAction::PushBranch, &normal);
        assert_eq!(plan.argv, vec!["push", "origin", "main"]);
        assert_eq!(plan.kind, PlanKind::Execute);

        let force = ActionTarget {
            push_mode: PushMode::Force,
            ..target()
        };
        let plan = super::plan(RefAction::PushBranch, &force);
        assert_eq!(plan.argv, vec!["push", "origin", "main", "--force"]);
        assert!(plan.kind.is_gated());

        let forced_fetch = ActionTarget {
            force: true,
            local_branch: "mine".to_string(),
            ..target()
        };
        let plan = super::plan(RefAction::FetchIntoLocalBranch, &forced_fetch);
        assert_eq!(plan.argv, vec!["fetch", "-f", "origin", "main:mine"]);
        assert!(plan.kind.is_gated());

        let plain_fetch = ActionTarget {
            local_branch: "mine".to_string(),
            ..target()
        };
        assert_eq!(
            super::plan(RefAction::FetchIntoLocalBranch, &plain_fetch).kind,
            PlanKind::Execute
        );
    }

    #[test]
    fn stash_and_tag_targets_are_used() {
        assert_eq!(
            super::plan(RefAction::ApplyStash, &target()).argv,
            vec!["stash", "apply", "stash@{0}"]
        );
        let with_index = ActionTarget {
            reinstate_index: true,
            ..target()
        };
        assert_eq!(
            super::plan(RefAction::ApplyStash, &with_index).argv,
            vec!["stash", "apply", "--index", "stash@{0}"]
        );
        assert_eq!(
            super::plan(RefAction::PopStash, &target()).argv,
            vec!["stash", "pop", "stash@{0}"]
        );
        assert_eq!(
            super::plan(RefAction::BranchFromStash, &target()).argv,
            vec!["stash", "branch", "main", "stash@{0}"]
        );
        assert_eq!(
            super::plan(RefAction::PushStash, &target()).argv,
            vec!["stash", "push", "--message", "msg"]
        );
        let untracked = ActionTarget {
            message: String::new(),
            include_untracked: true,
            ..target()
        };
        assert_eq!(
            super::plan(RefAction::PushStash, &untracked).argv,
            vec!["stash", "push", "--include-untracked"]
        );
        let tagged = ActionTarget {
            annotated: true,
            force: true,
            ..target()
        };
        assert_eq!(
            super::plan(RefAction::AddTag, &tagged).argv,
            vec!["tag", "-f", "-a", "main", "-m", "msg", "abc1234"]
        );
        assert_eq!(
            super::plan(RefAction::PushTag, &target()).argv,
            vec!["push", "origin", "main"]
        );
        assert_eq!(
            super::plan(RefAction::DeleteTag, &target()).argv,
            vec!["tag", "-d", "main"]
        );
    }

    #[test]
    fn create_branch_and_remote_argv() {
        let checkout = ActionTarget {
            checkout: true,
            ..target()
        };
        assert_eq!(
            super::plan(RefAction::CreateBranch, &checkout).argv,
            vec!["checkout", "-b", "main", "abc1234"]
        );
        let no_checkout = ActionTarget {
            checkout: false,
            ..target()
        };
        assert_eq!(
            super::plan(RefAction::CreateBranch, &no_checkout).argv,
            vec!["branch", "main", "abc1234"]
        );
        assert_eq!(
            super::plan(
                RefAction::DeleteBranch,
                &ActionTarget {
                    force: true,
                    ..target()
                }
            )
            .argv,
            vec!["branch", "-D", "main"]
        );
        assert_eq!(
            super::plan(RefAction::DeleteRemoteBranch, &target()).argv,
            vec!["push", "origin", "--delete", "main"]
        );
        assert_eq!(
            super::plan(RefAction::Fetch, &target()).argv,
            vec!["fetch", "origin"]
        );
        let all = ActionTarget {
            remote: String::new(),
            prune: true,
            prune_tags: true,
            ..target()
        };
        assert_eq!(
            super::plan(RefAction::Fetch, &all).argv,
            vec!["fetch", "--all", "--prune", "--prune-tags"]
        );
        assert_eq!(
            super::plan(RefAction::PullBranch, &target()).argv,
            vec!["pull", "origin", "main"]
        );
        assert_eq!(
            super::plan(RefAction::AddRemote, &target()).argv,
            vec!["remote", "add", "main", "https://example.test/x"]
        );
        assert_eq!(
            super::plan(RefAction::PruneRemote, &target()).argv,
            vec!["remote", "prune", "main"]
        );
        assert_eq!(
            super::plan(RefAction::DeleteRemote, &target()).argv,
            vec!["remote", "remove", "main"]
        );
        assert_eq!(
            super::plan(RefAction::EditRemote, &target()).argv,
            vec!["remote", "rename", "main", "renamed"]
        );
        assert_eq!(
            super::plan(RefAction::EditUserDetails, &target()).argv,
            vec!["config", "user.name", "Ada"]
        );
        assert_eq!(
            super::plan(RefAction::DeleteUserDetails, &target()).argv,
            vec!["config", "--unset-all", "user.name"]
        );
    }

    #[test]
    fn reset_file_and_config_argv() {
        assert_eq!(
            super::plan(RefAction::ResetFileToRevision, &target()).argv,
            vec!["checkout", "abc1234", "--", "src/lib.rs"]
        );
        let soft = ActionTarget {
            reset_mode: ResetMode::Soft,
            ..target()
        };
        assert_eq!(
            super::plan(RefAction::ResetToCommit, &soft).argv,
            vec!["reset", "--soft", "abc1234"]
        );
        let hard = ActionTarget {
            reset_mode: ResetMode::Hard,
            ..target()
        };
        assert_eq!(
            super::plan(RefAction::ResetToCommit, &hard).argv,
            vec!["reset", "--hard", "abc1234"]
        );
    }

    #[test]
    fn copy_actions_are_local_and_carry_a_payload() {
        let plan = super::plan(RefAction::CopyToClipboard, &target());
        assert_eq!(plan.kind, PlanKind::Local);
        assert_eq!(plan.payload.as_deref(), Some("copied text"));
        assert!(plan.argv.is_empty());

        let file = ActionTarget {
            clipboard: String::new(),
            ..target()
        };
        assert_eq!(
            super::plan(RefAction::CopyFilePath, &file)
                .payload
                .as_deref(),
            Some("src/lib.rs")
        );
    }

    #[test]
    fn out_of_scope_rows_are_explicit() {
        for action in [
            RefAction::ViewDiff,
            RefAction::CreateArchive,
            RefAction::OpenFile,
            RefAction::StartCodeReview,
            RefAction::ViewScm,
        ] {
            let plan = super::plan(action, &target());
            assert_eq!(plan.kind, PlanKind::OutOfScope, "{action:?}");
            assert!(plan.argv.is_empty(), "{action:?} must not build argv");
        }
    }

    #[test]
    fn plan_kind_mix_has_the_documented_shape() {
        let mut execute = 0;
        let mut confirm = 0;
        let mut local = 0;
        let mut oos = 0;
        for action in RefAction::ALL {
            match super::plan(action, &target()).kind {
                PlanKind::Execute => execute += 1,
                PlanKind::Confirm(_) => confirm += 1,
                PlanKind::Local => local += 1,
                PlanKind::OutOfScope => oos += 1,
            }
        }
        assert_eq!(execute + confirm + local + oos, 62);
        assert_eq!(execute, 10, "safe executing rows");
        assert_eq!(confirm, 25, "gated rows");
        assert_eq!(local, 12, "local/state rows");
        assert_eq!(oos, 15, "out-of-scope rows");
    }

    #[test]
    fn base64_matches_rfc4648_examples() {
        assert_eq!(base64(""), "");
        assert_eq!(base64("f"), "Zg==");
        assert_eq!(base64("fo"), "Zm8=");
        assert_eq!(base64("foo"), "Zm9v");
        assert_eq!(base64("foob"), "Zm9vYg==");
        assert_eq!(base64("fooba"), "Zm9vYmE=");
        assert_eq!(base64("foobar"), "Zm9vYmFy");
    }

    /// `clipboard_path` reads `GGT_CLIPBOARD_FILE` first, then `XDG_RUNTIME_DIR`.
    #[test]
    fn clipboard_path_honours_the_env_override() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let dir = std::env::temp_dir().join("ggt-w5-clipboard");
        std::env::set_var("GGT_CLIPBOARD_FILE", dir.join("out.txt"));
        assert_eq!(clipboard_path(), dir.join("out.txt"));
        std::env::remove_var("GGT_CLIPBOARD_FILE");
    }

    #[test]
    fn dispatch_refuses_gated_plans_without_running_git() {
        // A bogus repo path proves nothing was executed: a real run would fail.
        let bogus = Path::new("/nonexistent/w5-dispatch-test");
        let outcome = dispatch(bogus, RefAction::DeleteBranch, &target());
        assert!(matches!(outcome, DispatchOutcome::NeedsConfirm(_)));
        assert!(outcome.status_line().contains("needs confirmation"));

        let outcome = dispatch(bogus, RefAction::ViewDiff, &target());
        assert!(matches!(outcome, DispatchOutcome::Skipped(_)));
    }

    #[test]
    fn dispatch_writes_the_clipboard_file() {
        let _guard = ENV_LOCK.lock().unwrap_or_else(|error| error.into_inner());
        let path = std::env::temp_dir().join(format!("ggt-w5-clip-{}.txt", std::process::id()));
        std::env::set_var("GGT_CLIPBOARD_FILE", &path);
        let outcome = dispatch(Path::new("."), RefAction::CopyToClipboard, &target());
        std::env::remove_var("GGT_CLIPBOARD_FILE");
        assert!(matches!(outcome, DispatchOutcome::Local { .. }));
        let written = std::fs::read_to_string(&path).expect("clipboard file written");
        assert_eq!(written, "copied text");
        let _ = std::fs::remove_file(&path);
    }
}
