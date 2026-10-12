//! Commit file changes and the CDV file tree (W6 / lane w6).
//!
//! Two git calls per commit, exactly like the reference's `dataSource.ts`:
//!
//! * `git diff-tree --raw -r --root --find-renames --diff-filter=AMDR -z <hash>`
//!   — one record per changed file, carrying both blob modes (so a gitlink,
//!   mode `160000`, is detected as a **submodule**) and the status letter;
//! * `git diff-tree --numstat -r --root --find-renames --diff-filter=AMDR -z <hash>`
//!   — the `+additions` / `-deletions` counts (`-` → `None`, i.e. binary).
//!
//! `--raw -z` record shape (verified against git 2.4x):
//!
//! ```text
//! <commit>\0:100644 100644 <sha> <sha> M\0path\0
//! <commit>\0:100644 100644 <sha> <sha> R083\0old\0new\0
//! ```
//!
//! `--numstat -z` record shape:
//!
//! ```text
//! <commit>\0<add>\t<del>\t<path>\0
//! <commit>\0<add>\t<del>\t\0old\0new\0     (rename: empty third field)
//! ```
//!
//! The tree mirrors the reference's `generateFileTree` (`web/main.ts:3530-3560`):
//! folders sort before files, `localeCompare` on the name (byte order here), and a
//! chain of single-child folders collapses into one `a / b / c` row
//! (`getCurrentFolderInfo`, `web/main.ts:3553-3558`).

use std::collections::HashSet;
use std::path::Path;

use crate::data::{run_git, GitError};

/// Field separator for the commit-details `--format` (`%x1f`).
const FS: char = '\u{1f}';

/// The reference's `GitFileStatus` (`src/types.ts:76-83`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum GitFileStatus {
    Added,
    Modified,
    Deleted,
    Renamed,
    Untracked,
}

impl GitFileStatus {
    /// The single-letter code used in the row badge and the `.gitFileName` class.
    pub fn code(self) -> char {
        match self {
            GitFileStatus::Added => 'A',
            GitFileStatus::Modified => 'M',
            GitFileStatus::Deleted => 'D',
            GitFileStatus::Renamed => 'R',
            GitFileStatus::Untracked => 'U',
        }
    }

    /// Parse a `--raw` / `--name-status` status letter (`R083` → [`Renamed`]).
    pub fn from_code(code: char) -> Option<Self> {
        match code.to_ascii_uppercase() {
            'A' => Some(GitFileStatus::Added),
            'M' => Some(GitFileStatus::Modified),
            'D' => Some(GitFileStatus::Deleted),
            'R' => Some(GitFileStatus::Renamed),
            'U' => Some(GitFileStatus::Untracked),
            _ => None,
        }
    }
}

/// One changed file, mirroring `GG.GitFileChange` (`src/types.ts:65-71`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitFileChange {
    /// Path before the change (`Renamed`/`Deleted`).
    pub old_path: String,
    /// Path after the change (the identity used for lookups).
    pub new_path: String,
    pub status: GitFileStatus,
    /// `None` when git reports `-` (binary) or the file has no numstat record.
    pub additions: Option<u32>,
    pub deletions: Option<u32>,
    /// Blob mode `160000` on either side → gitlink / submodule entry.
    pub submodule: bool,
}

impl GitFileChange {
    /// The path the row shows (the old path for a deleted file).
    pub fn path(&self) -> &str {
        if self.status == GitFileStatus::Deleted {
            &self.old_path
        } else {
            &self.new_path
        }
    }

    /// Does git report both an additions and a deletions count?
    ///
    /// This is the reference's `textFile` test
    /// (`web/main.ts:3584`: `additions !== null && deletions !== null`).
    pub fn text_file(&self) -> bool {
        self.additions.is_some() && self.deletions.is_some()
    }

    /// `gitDiffPossible`: an untracked file is always viewable, otherwise the
    /// file must have text numstat (`web/main.ts:3585`).
    pub fn diff_possible(&self) -> bool {
        self.status == GitFileStatus::Untracked || self.text_file()
    }

    /// `+N|-M` text, or `None` for a file git gives no counts for.
    pub fn counts_label(&self) -> Option<String> {
        match (self.additions, self.deletions) {
            (Some(additions), Some(deletions)) => Some(format!("+{additions}|-{deletions}")),
            _ => None,
        }
    }
}

/// Parse the `--raw -z` payload into `(status, old, new, submodule)` records.
///
/// The leading field of the payload is the commit hash, which is skipped — the
/// same `lines.shift()` the reference does (`src/dataSource.ts:1789`).
pub fn parse_raw(output: &str) -> Vec<(GitFileStatus, String, String, bool)> {
    parse_raw_at(output, 1)
}

/// Parse a `git diff --raw -z` payload.
///
/// Unlike `git diff-tree`, a plain `git diff` has no leading commit-hash field,
/// so the reference only shifts the first line for the `diff-tree` form
/// (`src/dataSource.ts:1786`).
pub fn parse_raw_diff(output: &str) -> Vec<(GitFileStatus, String, String, bool)> {
    parse_raw_at(output, 0)
}

fn parse_raw_at(output: &str, start: usize) -> Vec<(GitFileStatus, String, String, bool)> {
    let parts: Vec<&str> = output.split('\0').collect();
    let mut records = Vec::new();
    let mut i = start;
    while i < parts.len() {
        let header = parts[i];
        if !header.starts_with(':') {
            break;
        }
        i += 1;
        let fields: Vec<&str> = header.split_whitespace().collect();
        if fields.len() < 5 {
            break;
        }
        let submodule = fields[1] == "160000" || fields[2] == "160000";
        let Some(status) = fields[4].chars().next().and_then(GitFileStatus::from_code) else {
            break;
        };
        if status == GitFileStatus::Renamed {
            if i + 1 >= parts.len() {
                break;
            }
            records.push((
                status,
                parts[i].to_string(),
                parts[i + 1].to_string(),
                submodule,
            ));
            i += 2;
        } else {
            if i >= parts.len() {
                break;
            }
            records.push((
                status,
                parts[i].to_string(),
                parts[i].to_string(),
                submodule,
            ));
            i += 1;
        }
    }
    records
}

/// Parse the `--numstat -z` payload into `(path, additions, deletions)` records.
pub fn parse_numstat(output: &str) -> Vec<(String, Option<u32>, Option<u32>)> {
    parse_numstat_at(output, 1)
}

/// Parse a `git diff --numstat -z` payload (no leading commit-hash field).
pub fn parse_numstat_diff(output: &str) -> Vec<(String, Option<u32>, Option<u32>)> {
    parse_numstat_at(output, 0)
}

fn parse_numstat_at(output: &str, start: usize) -> Vec<(String, Option<u32>, Option<u32>)> {
    let parts: Vec<&str> = output.split('\0').collect();
    let mut records = Vec::new();
    let mut i = start;
    while i < parts.len() {
        let fields: Vec<&str> = parts[i].split('\t').collect();
        if fields.len() != 3 {
            break;
        }
        let additions = fields[0].parse::<u32>().ok();
        let deletions = fields[1].parse::<u32>().ok();
        if fields[2].is_empty() {
            // Rename: the path follows as two extra NUL-separated fields.
            if i + 2 >= parts.len() {
                break;
            }
            records.push((parts[i + 2].to_string(), additions, deletions));
            i += 3;
        } else {
            records.push((fields[2].to_string(), additions, deletions));
            i += 1;
        }
    }
    records
}

/// Join the two record sets by path, preserving the status order.
///
/// This is `generateFileChanges` (`src/dataSource.ts:1849-1888`): the
/// `--name-status` list defines the rows and the numstat lookup fills the counts.
pub fn merge_changes(
    status_records: &[(GitFileStatus, String, String, bool)],
    numstat: &[(String, Option<u32>, Option<u32>)],
) -> Vec<GitFileChange> {
    let mut changes: Vec<GitFileChange> = status_records
        .iter()
        .map(|(status, old_path, new_path, submodule)| GitFileChange {
            old_path: old_path.clone(),
            new_path: new_path.clone(),
            status: *status,
            additions: None,
            deletions: None,
            submodule: *submodule,
        })
        .collect();

    for (path, additions, deletions) in numstat {
        if let Some(change) = changes.iter_mut().find(|change| &change.new_path == path) {
            change.additions = *additions;
            change.deletions = *deletions;
        }
    }
    changes
}

/// `generateFileChanges` (`src/dataSource.ts:1849-1881`) with the working-tree
/// status overlay.
///
/// The name-status rows define the list and the numstat lookup fills the counts
/// (that part is [`merge_changes`]); the `git status` overlay then re-types the
/// paths it reports as deleted and appends the untracked ones.
pub fn merge_changes_with_status(
    status_records: &[(GitFileStatus, String, String, bool)],
    numstat: &[(String, Option<u32>, Option<u32>)],
    deleted: &[String],
    untracked: &[String],
) -> Vec<GitFileChange> {
    let mut changes = merge_changes(status_records, numstat);
    let mut lookup: std::collections::HashMap<String, usize> = changes
        .iter()
        .enumerate()
        .map(|(index, change)| (change.new_path.clone(), index))
        .collect();

    for path in deleted {
        match lookup.get(path) {
            Some(&index) => changes[index].status = GitFileStatus::Deleted,
            None => {
                lookup.insert(path.clone(), changes.len());
                changes.push(GitFileChange {
                    old_path: path.clone(),
                    new_path: path.clone(),
                    status: GitFileStatus::Deleted,
                    additions: None,
                    deletions: None,
                    submodule: false,
                });
            }
        }
    }
    for path in untracked {
        lookup.insert(path.clone(), changes.len());
        changes.push(GitFileChange {
            old_path: path.clone(),
            new_path: path.clone(),
            status: GitFileStatus::Untracked,
            additions: None,
            deletions: None,
            submodule: false,
        });
    }
    changes
}

/// `getStatus` (`src/dataSource.ts:1712-1737`): the deleted and untracked paths
/// in a `git status -s --porcelain -z` payload.
///
/// A rename/copy entry carries the destination in the following NUL field, so
/// the index steps by two for `R`/`C` just like the reference does.
pub fn parse_status(output: &str) -> (Vec<String>, Vec<String>) {
    let mut deleted = Vec::new();
    let mut untracked = Vec::new();
    let parts: Vec<&str> = output.split('\0').collect();
    let mut i = 0;
    while i < parts.len() && !parts[i].is_empty() {
        let entry = parts[i];
        if entry.len() < 4 {
            break;
        }
        let path = entry[3..].to_string();
        let mut chars = entry.chars();
        let c1 = chars.next().unwrap_or(' ');
        let c2 = chars.next().unwrap_or(' ');
        if c1 == 'D' || c2 == 'D' {
            deleted.push(path);
        } else if c1 == '?' || c2 == '?' {
            untracked.push(path);
        }
        if matches!(c1, 'R' | 'C') || matches!(c2, 'R' | 'C') {
            i += 2;
        } else {
            i += 1;
        }
    }
    (deleted, untracked)
}

/// The reference's `--diff-filter` default (`src/dataSource.ts:1465`).
pub const DIFF_FILTER: &str = "AMDR";

/// Changed files of one commit, statuses plus numstat.
pub fn commit_file_changes(repo: &Path, hash: &str) -> Result<Vec<GitFileChange>, GitError> {
    let filter = format!("--diff-filter={DIFF_FILTER}");
    let raw = run_git(
        repo,
        &[
            "diff-tree",
            "--raw",
            "-r",
            "--root",
            "--find-renames",
            &filter,
            "-z",
            hash,
        ],
    )?;
    let numstat = run_git(
        repo,
        &[
            "diff-tree",
            "--numstat",
            "-r",
            "--root",
            "--find-renames",
            &filter,
            "-z",
            hash,
        ],
    )?;
    Ok(merge_changes(&parse_raw(&raw), &parse_numstat(&numstat)))
}

/// The uncommitted working-tree changes for the CDV.
///
/// This is `getUncommittedDetails` (`src/dataSource.ts:396-414`): `git diff
/// HEAD` for the tracked rows plus `git status --porcelain` for the
/// deleted/untracked overlay. `--untracked-files=all` matches the reference's
/// default `showUntrackedFiles: true` (`src/config.ts`).
pub fn working_tree_changes(repo: &Path) -> Result<Vec<GitFileChange>, GitError> {
    let filter = format!("--diff-filter={DIFF_FILTER}");
    let raw = run_git(
        repo,
        &["diff", "--raw", "--find-renames", &filter, "-z", "HEAD"],
    )?;
    let numstat = run_git(
        repo,
        &["diff", "--numstat", "--find-renames", &filter, "-z", "HEAD"],
    )?;
    let status = run_git(
        repo,
        &["status", "-s", "--untracked-files=all", "--porcelain", "-z"],
    )?;
    let (deleted, untracked) = parse_status(&status);
    Ok(merge_changes_with_status(
        &parse_raw_diff(&raw),
        &parse_numstat_diff(&numstat),
        &deleted,
        &untracked,
    ))
}

/// The changed files between two revisions (`getCommitComparison`,
/// `src/dataSource.ts:423-434`).
///
/// When `to` is `UNCOMMITTED` the right-hand side is the working tree, so the
/// diff carries no right ref and the `git status` overlay is merged in instead —
/// the same two branches the reference takes.
pub fn comparison_changes(
    repo: &Path,
    from: &str,
    to: &str,
) -> Result<Vec<GitFileChange>, GitError> {
    let filter = format!("--diff-filter={DIFF_FILTER}");
    let uncommitted = to == crate::data::UNCOMMITTED;
    let mut raw_args = vec!["diff", "--raw", "--find-renames", &filter, "-z", from];
    let mut numstat_args = vec!["diff", "--numstat", "--find-renames", &filter, "-z", from];
    if !uncommitted {
        raw_args.push(to);
        numstat_args.push(to);
    }
    let raw = run_git(repo, &raw_args)?;
    let numstat = run_git(repo, &numstat_args)?;
    let (deleted, untracked) = if uncommitted {
        let status = run_git(
            repo,
            &["status", "-s", "--untracked-files=all", "--porcelain", "-z"],
        )?;
        parse_status(&status)
    } else {
        (Vec::new(), Vec::new())
    };
    Ok(merge_changes_with_status(
        &parse_raw_diff(&raw),
        &parse_numstat_diff(&numstat),
        &deleted,
        &untracked,
    ))
}

/// The configured external diff tool (VIS-051).
///
/// `gitConfig.diffTool` / `guiDiffTool` come from the consolidated `git config`
/// read (`src/dataSource.ts:311-312`); `gui.difftool` wins when both are set,
/// exactly as `renderCdvExternalDiffBtn` picks its title
/// (`web/main.ts:3123-3125`).
pub fn external_diff_tool(repo: &Path) -> Option<String> {
    let read = |key: &str| {
        run_git(repo, &["config", "--get", key])
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    };
    read("gui.difftool").or_else(|| read("diff.tool"))
}

/// The reference's `--format` for the CDV summary (`src/dataSource.ts:106-110`).
const DETAILS_FORMAT: &str = "--format=%H\u{1f}%P\u{1f}%an\u{1f}%ae\u{1f}%at\u{1f}%cn\u{1f}%ce\u{1f}%ct\u{1f}%G?\u{1f}%GS\u{1f}%GK\u{1f}%B";

/// Everything the CDV summary block shows (VIS-038).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CommitDetail {
    pub hash: String,
    pub parents: Vec<String>,
    pub author: String,
    pub author_email: String,
    pub author_date: i64,
    pub committer: String,
    pub committer_email: String,
    pub committer_date: i64,
    /// `%G?` — `N` means "no signature".
    pub signature_status: char,
    pub signature_signer: String,
    pub signature_key: String,
    /// `%B` — the raw message (subject + body).
    pub body: String,
}

impl CommitDetail {
    /// Does the commit carry a GPG signature the reference would render?
    pub fn has_signature(&self) -> bool {
        !matches!(self.signature_status, 'N' | ' ' | '\0')
    }

    /// The `GIT_SIGNATURE_STATUS_DESCRIPTIONS` string (`web/utils.ts:47-55`).
    pub fn signature_description(&self) -> &'static str {
        match self.signature_status {
            'G' => "Valid Signature",
            'U' => "Good Signature with Unknown Validity",
            'X' => "Good Signature that has Expired",
            'Y' => "Good Signature made by an Expired Key",
            'R' => "Good Signature made by a Revoked Key",
            'E' => "Signature could not be checked",
            'B' => "Bad Signature",
            _ => "",
        }
    }

    /// Message body without the subject line (the reference formats it as markdown).
    pub fn body_after_subject(&self) -> String {
        match self.body.split_once('\n') {
            Some((_, rest)) => rest.trim_start_matches('\n').trim_end().to_string(),
            None => String::new(),
        }
    }

    /// Parse the `DETAILS_FORMAT` payload.
    pub fn parse(output: &str) -> Option<Self> {
        let mut fields = output.split(FS);
        let mut next = || fields.next().map(str::to_string);
        let detail = CommitDetail {
            hash: next()?,
            parents: next()?
                .split(' ')
                .filter(|p| !p.is_empty())
                .map(str::to_string)
                .collect(),
            author: next()?,
            author_email: next()?,
            author_date: next()?.trim().parse().unwrap_or(0),
            committer: next()?,
            committer_email: next()?,
            committer_date: next()?.trim().parse().unwrap_or(0),
            signature_status: next()?.chars().next().unwrap_or('N'),
            signature_signer: next()?,
            signature_key: next()?,
            body: fields.collect::<Vec<_>>().join(&FS.to_string()),
        };
        Some(detail)
    }
}

/// Read one commit's summary for the CDV.
pub fn commit_details(repo: &Path, hash: &str) -> Result<CommitDetail, GitError> {
    let out = run_git(
        repo,
        &[
            "-c",
            "log.showSignature=false",
            "show",
            "--quiet",
            hash,
            DETAILS_FORMAT,
        ],
    )?;
    CommitDetail::parse(&out).ok_or_else(|| GitError {
        command: format!("git show {hash}"),
        code: None,
        stderr: "unexpected --format output".to_string(),
    })
}

/* ------------------------------------------------------------------ */
/* File tree                                                           */
/* ------------------------------------------------------------------ */

/// Which file layout the CDV shows (VIS-041 / VIS-042).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FileView {
    #[default]
    Tree,
    List,
}

impl FileView {
    pub fn label(self) -> &'static str {
        match self {
            FileView::Tree => "Tree",
            FileView::List => "List",
        }
    }

    pub fn toggle(self) -> Self {
        match self {
            FileView::Tree => FileView::List,
            FileView::List => FileView::Tree,
        }
    }
}

/// A folder in the nested file tree.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Folder {
    pub name: String,
    /// Full path from the repository root (empty for the root itself).
    pub path: String,
    pub children: Vec<Node>,
}

/// One child of a [`Folder`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    Folder(Folder),
    /// Index into the commit's `Vec<GitFileChange>`.
    File(usize),
}

/// The nested tree built from a commit's changed files.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FileTree {
    pub root: Folder,
}

impl FileTree {
    /// Build the tree, inserting each change at its `/`-separated path.
    pub fn build(changes: &[GitFileChange]) -> Self {
        let mut root = Folder::default();
        for (index, change) in changes.iter().enumerate() {
            let path = change.path();
            let mut segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
            if segments.is_empty() {
                segments = vec![path];
            }
            insert(&mut root, &segments, index);
        }
        sort_folder(&mut root, changes);
        FileTree { root }
    }
}

fn insert(folder: &mut Folder, segments: &[&str], index: usize) {
    let (head, rest) = match segments.split_first() {
        Some((head, rest)) => (*head, rest),
        None => return,
    };
    if rest.is_empty() {
        folder.children.push(Node::File(index));
        return;
    }
    let position = folder
        .children
        .iter()
        .position(|child| matches!(child, Node::Folder(f) if f.name == head));
    let position = match position {
        Some(position) => position,
        None => {
            let path = if folder.path.is_empty() {
                head.to_string()
            } else {
                format!("{}/{}", folder.path, head)
            };
            folder.children.push(Node::Folder(Folder {
                name: head.to_string(),
                path,
                children: Vec::new(),
            }));
            folder.children.len() - 1
        }
    };
    if let Node::Folder(child) = &mut folder.children[position] {
        insert(child, rest, index);
    }
}

/// Folders first, then files, each alphabetical (`sortFolderKeys`).
///
/// `sortFolderKeys` compares `contents[a].name` — the *path segment* — so files
/// must be ordered by name, never by their index into `changes`.
fn sort_folder(folder: &mut Folder, changes: &[GitFileChange]) {
    folder.children.sort_by(|a, b| match (a, b) {
        (Node::Folder(_), Node::File(_)) => std::cmp::Ordering::Less,
        (Node::File(_), Node::Folder(_)) => std::cmp::Ordering::Greater,
        (Node::Folder(left), Node::Folder(right)) => left.name.cmp(&right.name),
        (Node::File(left), Node::File(right)) => {
            file_name(&changes[*left]).cmp(file_name(&changes[*right]))
        }
    });
    for child in &mut folder.children {
        if let Node::Folder(folder) = child {
            sort_folder(folder, changes);
        }
    }
}

/// The last path segment — the name a file is inserted under.
fn file_name(change: &GitFileChange) -> &str {
    change
        .path()
        .rsplit('/')
        .next()
        .unwrap_or_else(|| change.path())
}

/// What one rendered CDV file row is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileRowKind {
    /// A folder row; `open` drives the `closed` class / the disclosure glyph.
    Folder { open: bool },
    /// A file row, indexing the commit's `Vec<GitFileChange>`.
    File(usize),
}

/// One rendered CDV file row, at a given indent depth.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileRow {
    /// Indent level (0 for a top-level row).
    pub depth: u16,
    /// Display label: the file name, or the collapsed `a / b / c` folder chain.
    pub label: String,
    /// Full path — the `FileRow` menu context (`A65`..`A73`).
    pub path: String,
    pub kind: FileRowKind,
}

impl FileRow {
    /// A folder row for `(label, path)`.
    fn folder(depth: u16, label: String, path: String, open: bool) -> Self {
        FileRow {
            depth,
            label,
            path,
            kind: FileRowKind::Folder { open },
        }
    }

    /// A file row for `change`.
    fn file(depth: u16, label: String, path: String, index: usize) -> Self {
        FileRow {
            depth,
            label,
            path,
            kind: FileRowKind::File(index),
        }
    }
}

/// Flatten `tree` for rendering, honouring `open` (folders default to open).
///
/// A chain of single-child folders collapses into one `a / b / c` row, exactly
/// like `getCurrentFolderInfo` (`web/main.ts:3553-3558`).
pub fn flatten_tree(
    tree: &FileTree,
    changes: &[GitFileChange],
    open: &HashSet<String>,
) -> Vec<FileRow> {
    let mut out = Vec::new();
    walk(&tree.root, changes, open, 0, &mut out);
    out
}

fn walk(
    folder: &Folder,
    changes: &[GitFileChange],
    open: &HashSet<String>,
    depth: u16,
    out: &mut Vec<FileRow>,
) {
    for child in &folder.children {
        match child {
            Node::File(index) => {
                let change = &changes[*index];
                let label = file_name(change).to_string();
                out.push(FileRow::file(
                    depth,
                    label,
                    change.path().to_string(),
                    *index,
                ));
            }
            Node::Folder(child_folder) => {
                // Collapse `a` → `b` → `c` while each has exactly one folder child.
                let mut label = child_folder.name.clone();
                let mut path = child_folder.path.clone();
                let mut node = child_folder;
                while node.children.len() == 1 {
                    match &node.children[0] {
                        Node::Folder(inner) => {
                            label = format!("{label} / {}", inner.name);
                            path = inner.path.clone();
                            node = inner;
                        }
                        Node::File(_) => break,
                    }
                }
                let is_open = !open.contains(&path);
                out.push(FileRow::folder(depth, label, path.clone(), is_open));
                if is_open {
                    walk(node, changes, open, depth + 1, out);
                }
            }
        }
    }
}

/// Flatten `changes` into a flat list of full paths (`generateFileListHtml`).
pub fn flatten_list(tree: &FileTree, changes: &[GitFileChange]) -> Vec<FileRow> {
    let mut out = Vec::new();
    walk_list(&tree.root, changes, &mut out);
    out
}

fn walk_list(folder: &Folder, changes: &[GitFileChange], out: &mut Vec<FileRow>) {
    for child in &folder.children {
        match child {
            Node::File(index) => {
                let change = &changes[*index];
                out.push(FileRow::file(
                    0,
                    change.path().to_string(),
                    change.path().to_string(),
                    *index,
                ));
            }
            Node::Folder(inner) => walk_list(inner, changes, out),
        }
    }
}

/// The rows the CDV shows for one commit.
pub fn file_rows(
    tree: &FileTree,
    changes: &[GitFileChange],
    view: FileView,
    open: &HashSet<String>,
) -> Vec<FileRow> {
    match view {
        FileView::Tree => flatten_tree(tree, changes, open),
        FileView::List => flatten_list(tree, changes),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RAW: &str = "abc123\u{0}:100644 100644 aaa bbb M\u{0}src/main.rs\u{0}\
:100644 000000 aaa 000 D\u{0}src/old.rs\u{0}\
:100644 100644 aaa bbb R083\u{0}src/from.rs\u{0}src/to.rs\u{0}\
:000000 100644 000 bbb A\u{0}src/new.rs\u{0}\
:000000 160000 000 bbb A\u{0}sub/module\u{0}";

    const NUMSTAT: &str = "abc123\u{0}5\t2\tsrc/main.rs\u{0}0\t9\tsrc/old.rs\u{0}\
3\t1\t\u{0}src/from.rs\u{0}src/to.rs\u{0}7\t0\tsrc/new.rs\u{0}-\t-\tsub/module\u{0}";

    fn changes() -> Vec<GitFileChange> {
        merge_changes(&parse_raw(RAW), &parse_numstat(NUMSTAT))
    }

    #[test]
    fn raw_records_carry_status_paths_and_submodules() {
        let records = parse_raw(RAW);
        assert_eq!(records.len(), 5);
        assert_eq!(records[0].0, GitFileStatus::Modified);
        assert_eq!(records[0].1, "src/main.rs");
        assert!(!records[0].3);
        assert_eq!(records[1].0, GitFileStatus::Deleted);
        assert_eq!(records[2].0, GitFileStatus::Renamed);
        assert_eq!(records[2].1, "src/from.rs");
        assert_eq!(records[2].2, "src/to.rs");
        assert!(records[4].3, "mode 160000 is a submodule");
        assert_eq!(records[4].2, "sub/module");
    }

    #[test]
    fn numstat_handles_renames_and_binaries() {
        let records = parse_numstat(NUMSTAT);
        assert_eq!(records.len(), 5);
        assert_eq!(records[0], ("src/main.rs".into(), Some(5), Some(2)));
        assert_eq!(records[2], ("src/to.rs".into(), Some(3), Some(1)));
        assert_eq!(records[4], ("sub/module".into(), None, None));
    }

    #[test]
    fn merged_changes_join_counts_by_path() {
        let changes = changes();
        assert_eq!(changes.len(), 5);
        assert_eq!(changes[0].additions, Some(5));
        assert_eq!(changes[0].deletions, Some(2));
        assert!(changes[0].diff_possible());
        // Deleted file keeps the old path as its identity.
        assert_eq!(changes[1].path(), "src/old.rs");
        assert!(changes[1].diff_possible());
        // Renamed file keeps both paths, so a row can show `old → new`.
        assert_eq!(changes[2].old_path, "src/from.rs");
        assert_eq!(changes[2].new_path, "src/to.rs");
        // Binary: `-`/`-` numstat → not diff-possible.
        assert!(!changes[4].diff_possible());
        assert_eq!(changes[4].counts_label(), None);
    }

    #[test]
    fn status_codes_match_the_reference() {
        for status in [
            GitFileStatus::Added,
            GitFileStatus::Modified,
            GitFileStatus::Deleted,
            GitFileStatus::Renamed,
            GitFileStatus::Untracked,
        ] {
            assert_eq!(GitFileStatus::from_code(status.code()), Some(status));
        }
        assert_eq!(GitFileStatus::from_code('R'), Some(GitFileStatus::Renamed));
        assert_eq!(GitFileStatus::from_code('Z'), None);
    }

    #[test]
    fn tree_groups_folders_before_files_and_collapses_chains() {
        let changes = changes();
        let tree = FileTree::build(&changes);
        let rows = flatten_tree(&tree, &changes, &HashSet::new());
        // `src` has 4 children; `sub` has exactly one child, but it is a *file*
        // (the submodule `sub/module`), and `getCurrentFolderInfo`
        // (`web/main.ts:3551-3557`) only collapses a single-child **folder**.
        let labels: Vec<&str> = rows.iter().map(|row| row.label.as_str()).collect();
        assert_eq!(
            labels,
            vec!["src", "main.rs", "new.rs", "old.rs", "to.rs", "sub", "module",]
        );
        assert_eq!(rows[0].depth, 0);
        assert_eq!(rows[1].depth, 1);
        assert_eq!(rows[5].depth, 0);
        assert_eq!(rows[6].depth, 1);
        assert_eq!(rows[0].path, "src");
        assert_eq!(rows[5].path, "sub");
        assert_eq!(rows[6].path, "sub/module");
    }

    #[test]
    fn a_single_child_folder_chain_collapses() {
        // `a/b/c/only.rs` — three folders each holding one folder, then the file.
        let raw = "abc123\u{0}:000000 100644 000 bbb A\u{0}a/b/c/only.rs\u{0}";
        let numstat = "abc123\u{0}1\t0\ta/b/c/only.rs\u{0}";
        let changes = merge_changes(&parse_raw(raw), &parse_numstat(numstat));
        let tree = FileTree::build(&changes);
        let rows = flatten_tree(&tree, &changes, &HashSet::new());
        let labels: Vec<&str> = rows.iter().map(|row| row.label.as_str()).collect();
        assert_eq!(labels, vec!["a / b / c", "only.rs"]);
        // The collapsed row keeps the deepest path, so the disclosure state and
        // the `FileRow` menu context both point at `a/b/c`.
        assert_eq!(rows[0].path, "a/b/c");
        assert_eq!(rows[0].kind, FileRowKind::Folder { open: true });
        assert_eq!(rows[1].depth, 1);
    }

    #[test]
    fn closed_folders_hide_their_children() {
        let changes = changes();
        let tree = FileTree::build(&changes);
        let mut open = HashSet::new();
        open.insert("src".to_string());
        let rows = flatten_tree(&tree, &changes, &open);
        let labels: Vec<&str> = rows.iter().map(|row| row.label.as_str()).collect();
        assert_eq!(labels, vec!["src", "sub", "module"]);
        assert_eq!(rows[0].kind, FileRowKind::Folder { open: false });
        // `sub` was never toggled, so it keeps its default (open) state.
        assert_eq!(rows[1].kind, FileRowKind::Folder { open: true });
    }

    #[test]
    fn list_view_shows_full_paths_sorted() {
        let changes = changes();
        let tree = FileTree::build(&changes);
        let rows = flatten_list(&tree, &changes);
        let labels: Vec<&str> = rows.iter().map(|row| row.label.as_str()).collect();
        assert_eq!(
            labels,
            vec![
                "src/main.rs",
                "src/new.rs",
                "src/old.rs",
                "src/to.rs",
                "sub/module",
            ]
        );
        assert!(rows.iter().all(|row| row.depth == 0));
        assert_eq!(FileView::Tree.toggle(), FileView::List);
        assert_eq!(FileView::List.toggle(), FileView::Tree);
    }

    #[test]
    fn malformed_payloads_do_not_panic() {
        assert!(parse_raw("").is_empty());
        assert!(parse_raw("onlyhash").is_empty());
        assert!(parse_raw("hash\u{0}:100644 100644 a b M").is_empty());
        assert!(parse_numstat("").is_empty());
        assert!(parse_numstat("hash\u{0}not-a-record").is_empty());
        assert!(CommitDetail::parse("too\u{1f}few").is_none());
    }

    #[test]
    fn commit_details_parse_all_fields() {
        let payload = [
            "abc123",
            "p1 p2",
            "Ada",
            "ada@example.invalid",
            "1700000000",
            "Committer",
            "c@example.invalid",
            "1700000001",
            "G",
            "Signer",
            "KEY",
            "Subject line\n\nBody text\n",
        ]
        .join(&FS.to_string());
        let detail = CommitDetail::parse(&payload).expect("parses");
        assert_eq!(detail.hash, "abc123");
        assert_eq!(detail.parents, vec!["p1".to_string(), "p2".to_string()]);
        assert_eq!(detail.author_date, 1_700_000_000);
        assert!(detail.has_signature());
        assert_eq!(detail.signature_description(), "Valid Signature");
        assert_eq!(detail.body_after_subject(), "Body text");
    }

    #[test]
    fn unsigned_commit_has_no_signature_line() {
        let payload = [
            "abc123", "", "Ada", "", "0", "Ada", "", "0", "N", "", "", "one line",
        ]
        .join(&FS.to_string());
        let detail = CommitDetail::parse(&payload).expect("parses");
        assert!(!detail.has_signature());
        assert!(detail.parents.is_empty());
        assert_eq!(detail.body_after_subject(), "");
    }
}
