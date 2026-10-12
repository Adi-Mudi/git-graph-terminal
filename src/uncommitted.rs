//! The **Uncommitted Changes** section (M4-W12 / T4).
//!
//! The reference extension models the working tree as a synthetic first row
//! (`UNCOMMITTED`, `src/dataSource.ts:961`) whose children are the changed
//! files. The terminal design (`design/mockup.html`) puts the same information
//! in a section **above** the history:
//!
//! ```text
//! ▼ Uncommitted Changes (3)
//!     A  M  D  R   ·   staged 1 · unstaged 2   ·   +412 / −93
//!     ├─ A  src/feature/diff_renderer.rs       +412 −0   unstaged
//!     ├─ M  src/render/body.rs                 +0   −93  staged+unstaged
//!     └─ D  src/legacy/old_graph.rs            +0   −0   unstaged
//! ```
//!
//! Data comes from `git status --porcelain=v2 -z --find-renames` (machine
//! readable, NUL-terminated so paths with spaces or newlines survive) plus
//! `git diff --numstat` / `git diff --cached --numstat` for the per-file
//! `+add / −del` counts. Nothing here touches the graph.

use std::path::Path;

use crate::data::{self, GitError};
use crate::files::{self, GitFileStatus};

/// One row of the uncommitted-changes block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UncommittedFile {
    /// The display status (`A`/`M`/`D`/`R`).
    pub status: GitFileStatus,
    /// The identity path (new path for a rename).
    pub path: String,
    /// The pre-change path (`Renamed`/`Deleted` only).
    pub old_path: String,
    /// The index (staged) side changed (`X != '.'`).
    pub staged: bool,
    /// The worktree (unstaged) side changed (`Y != '.'`).
    pub unstaged: bool,
    /// Added lines across staged + unstaged.
    pub additions: u32,
    /// Deleted lines across staged + unstaged.
    pub deletions: u32,
}

impl UncommittedFile {
    /// The one-letter badge, matching the legend (`A`/`M`/`D`/`R`).
    ///
    /// An untracked file shows `A` — it is a file that would be added, and the
    /// legend has no separate letter for it (design rule: A/M/D/R only).
    pub fn letter(&self) -> char {
        display_letter(self.status)
    }

    /// `staged` / `unstaged` / `staged+unstaged`, for the row's right edge.
    pub fn stage_label(&self) -> &'static str {
        match (self.staged, self.unstaged) {
            (true, true) => "staged+unstaged",
            (true, false) => "staged",
            _ => "unstaged",
        }
    }
}

/// The design's single-letter status code.
pub fn display_letter(status: GitFileStatus) -> char {
    match status {
        GitFileStatus::Untracked | GitFileStatus::Added => 'A',
        GitFileStatus::Modified => 'M',
        GitFileStatus::Deleted => 'D',
        GitFileStatus::Renamed => 'R',
    }
}

/// The whole working-tree section.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UncommittedState {
    /// The changed files, in `git status` order.
    pub files: Vec<UncommittedFile>,
}

impl UncommittedState {
    /// Read the working tree of `repo`.
    ///
    /// The `status` call is the source of truth (a missing/unreadable index is
    /// a real error); the two `--numstat` calls are best-effort, so a repository
    /// without a HEAD (unborn branch) still shows its file list.
    pub fn load(repo: &Path) -> Result<Self, GitError> {
        let porcelain = data::run_git(repo, &["status", "--porcelain=v2", "-z", "--find-renames"])?;
        let mut files = parse_porcelain_v2(&porcelain);
        let staged = data::run_git(
            repo,
            &["diff", "--cached", "--numstat", "-z", "--no-renames"],
        )
        .unwrap_or_default();
        let unstaged =
            data::run_git(repo, &["diff", "--numstat", "-z", "--no-renames"]).unwrap_or_default();
        apply_numstat(&mut files, &staged, &unstaged);
        Ok(Self { files })
    }

    /// The number of changed files.
    pub fn len(&self) -> usize {
        self.files.len()
    }

    /// Is there nothing to show?
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// The `[A, M, D, R]` badge counts for the summary line.
    pub fn counts(&self) -> [usize; 4] {
        let mut counts = [0usize; 4];
        for file in &self.files {
            let slot = match file.status {
                GitFileStatus::Added | GitFileStatus::Untracked => 0,
                GitFileStatus::Modified => 1,
                GitFileStatus::Deleted => 2,
                GitFileStatus::Renamed => 3,
            };
            counts[slot] += 1;
        }
        counts
    }

    /// Files with a staged (index) change.
    pub fn staged_count(&self) -> usize {
        self.files.iter().filter(|file| file.staged).count()
    }

    /// Files with an unstaged (worktree) change.
    pub fn unstaged_count(&self) -> usize {
        self.files.iter().filter(|file| file.unstaged).count()
    }

    /// `(additions, deletions)` summed over every file.
    pub fn totals(&self) -> (u32, u32) {
        self.files.iter().fold((0, 0), |(adds, dels), file| {
            (adds + file.additions, dels + file.deletions)
        })
    }

    /// The header line: `▼ Uncommitted Changes (N)  — working directory vs HEAD`.
    ///
    /// `caret` is the caller's disclosure marker — `ladder.disclosure(open)`, so
    /// the header follows the glyph rung like every other drawn surface. The
    /// section itself holds no ladder, which keeps this module data-only.
    pub fn title(&self, caret: char) -> String {
        format!(
            "{caret} Uncommitted Changes ({})  — working directory vs HEAD",
            self.len()
        )
    }

    /// The summary line under the header.
    pub fn summary_line(&self) -> String {
        let [added, modified, deleted, renamed] = self.counts();
        let (adds, dels) = self.totals();
        format!(
            "   A {added}  ·  M {modified}  ·  D {deleted}  ·  R {renamed}   ·   staged {} · unstaged {}   ·   +{adds} / −{dels}",
            self.staged_count(),
            self.unstaged_count(),
        )
    }
}

/// Split `s` into `n` space-separated fields, keeping the last one whole.
///
/// `git status --porcelain=v2` puts the path last and does not quote it under
/// `-z`, so the path may contain spaces.
fn split_n(s: &str, n: usize) -> Vec<&str> {
    let mut parts = Vec::with_capacity(n);
    let mut rest = s;
    for _ in 0..n.saturating_sub(1) {
        match rest.find(' ') {
            Some(pos) => {
                parts.push(&rest[..pos]);
                rest = &rest[pos + 1..];
            }
            None => break,
        }
    }
    parts.push(rest);
    parts
}

/// Parse a `git status --porcelain=v2 -z` payload.
///
/// Record kinds: `1` ordinary, `2` rename/copy (the old path is the next NUL
/// field), `u` unmerged, `?` untracked, `!` ignored (skipped).
pub fn parse_porcelain_v2(output: &str) -> Vec<UncommittedFile> {
    let fields: Vec<&str> = output.split('\0').collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < fields.len() {
        let entry = fields[i];
        i += 1;
        let kind = entry.chars().next().unwrap_or(' ');
        match kind {
            '1' => {
                let parts = split_n(entry, 9);
                if parts.len() < 9 {
                    continue;
                }
                let path = parts[8].to_string();
                out.push(from_xy(parts[1], path.clone(), path));
            }
            '2' => {
                let parts = split_n(entry, 10);
                if parts.len() < 10 {
                    continue;
                }
                let new_path = parts[9].to_string();
                let old_path = fields.get(i).copied().unwrap_or_default().to_string();
                i += 1;
                out.push(from_xy(parts[1], new_path, old_path));
            }
            'u' => {
                let parts = split_n(entry, 11);
                if parts.len() < 11 {
                    continue;
                }
                let path = parts[10].to_string();
                out.push(UncommittedFile {
                    status: GitFileStatus::Modified,
                    path: path.clone(),
                    old_path: path,
                    staged: false,
                    unstaged: true,
                    additions: 0,
                    deletions: 0,
                });
            }
            '?' => {
                let path = entry[1..].trim_start().to_string();
                if path.is_empty() {
                    continue;
                }
                out.push(UncommittedFile {
                    status: GitFileStatus::Untracked,
                    path: path.clone(),
                    old_path: path,
                    staged: false,
                    unstaged: true,
                    additions: 0,
                    deletions: 0,
                });
            }
            _ => {}
        }
    }
    out
}

/// Build one row from a porcelain `XY` field and its paths.
fn from_xy(xy: &str, new_path: String, old_path: String) -> UncommittedFile {
    let mut chars = xy.chars();
    let x = chars.next().unwrap_or('.');
    let y = chars.next().unwrap_or('.');
    let letter = if y != '.' { y } else { x };
    let status = match letter.to_ascii_uppercase() {
        'A' => GitFileStatus::Added,
        'D' => GitFileStatus::Deleted,
        'R' | 'C' => GitFileStatus::Renamed,
        _ => GitFileStatus::Modified,
    };
    UncommittedFile {
        status,
        path: new_path,
        old_path,
        staged: x != '.',
        unstaged: y != '.',
        additions: 0,
        deletions: 0,
    }
}

/// Fold the staged/unstaged `--numstat` payloads into `files`.
///
/// The counts are additive: a file with both a staged and an unstaged hunk
/// shows the sum, which is what `+N / −M` on the row means.
pub fn apply_numstat(files: &mut [UncommittedFile], staged: &str, unstaged: &str) {
    let staged = files::parse_numstat_diff(staged);
    let unstaged = files::parse_numstat_diff(unstaged);
    for file in files.iter_mut() {
        let mut adds = 0u32;
        let mut dels = 0u32;
        for (path, a, d) in staged.iter().chain(unstaged.iter()) {
            if path == &file.path || path == &file.old_path {
                adds += a.unwrap_or(0);
                dels += d.unwrap_or(0);
            }
        }
        file.additions = adds;
        file.deletions = dels;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_modified_and_untracked_entries_parse() {
        // `1 .M ...` unstaged modify, `? new.rs` untracked.
        let payload = "1 .M N... 100644 100644 100644 aaa bbb src/render/body.rs\0\
                       ? src/feature/diff_renderer.rs\0";
        let files = parse_porcelain_v2(payload);
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].path, "src/render/body.rs");
        assert_eq!(files[0].status, GitFileStatus::Modified);
        assert!(!files[0].staged);
        assert!(files[0].unstaged);
        assert_eq!(files[0].stage_label(), "unstaged");
        assert_eq!(files[1].status, GitFileStatus::Untracked);
        assert_eq!(files[1].letter(), 'A', "untracked shows the A badge");
    }

    #[test]
    fn a_rename_keeps_both_paths_and_reads_the_next_nul_field() {
        let payload =
            "2 R. N... 100644 100644 100644 aaa bbb R100 src/new/name.rs\0src/old/name.rs\0";
        let files = parse_porcelain_v2(payload);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].status, GitFileStatus::Renamed);
        assert_eq!(files[0].path, "src/new/name.rs");
        assert_eq!(files[0].old_path, "src/old/name.rs");
        assert!(files[0].staged);
        assert!(!files[0].unstaged);
        assert_eq!(files[0].stage_label(), "staged");
    }

    #[test]
    fn a_staged_and_unstaged_file_says_both() {
        let payload = "1 MM N... 100644 100644 100644 aaa bbb src/lib.rs\0";
        let files = parse_porcelain_v2(payload);
        assert!(files[0].staged && files[0].unstaged);
        assert_eq!(files[0].stage_label(), "staged+unstaged");
    }

    #[test]
    fn paths_with_spaces_survive() {
        let payload = "1 .M N... 100644 100644 100644 aaa bbb src/a file with spaces.rs\0";
        let files = parse_porcelain_v2(payload);
        assert_eq!(files[0].path, "src/a file with spaces.rs");
    }

    #[test]
    fn numstat_adds_staged_and_unstaged_counts() {
        let mut files = parse_porcelain_v2("1 MM N... 100644 100644 100644 aaa bbb src/lib.rs\0");
        apply_numstat(&mut files, "3\t1\tsrc/lib.rs\0", "2\t5\tsrc/lib.rs\0");
        assert_eq!(files[0].additions, 5);
        assert_eq!(files[0].deletions, 6);
    }

    #[test]
    fn numstat_binary_dashes_count_as_zero() {
        let mut files = parse_porcelain_v2("1 .M N... 100644 100644 100644 aaa bbb img.png\0");
        apply_numstat(&mut files, "", "-\t-\timg.png\0");
        assert_eq!(files[0].additions, 0);
        assert_eq!(files[0].deletions, 0);
    }

    #[test]
    fn the_summary_line_counts_badges_staging_and_totals() {
        let state = UncommittedState {
            files: vec![
                UncommittedFile {
                    status: GitFileStatus::Added,
                    path: "a".into(),
                    old_path: "a".into(),
                    staged: false,
                    unstaged: true,
                    additions: 412,
                    deletions: 0,
                },
                UncommittedFile {
                    status: GitFileStatus::Modified,
                    path: "b".into(),
                    old_path: "b".into(),
                    staged: true,
                    unstaged: true,
                    additions: 0,
                    deletions: 93,
                },
                UncommittedFile {
                    status: GitFileStatus::Deleted,
                    path: "c".into(),
                    old_path: "c".into(),
                    staged: false,
                    unstaged: true,
                    additions: 0,
                    deletions: 0,
                },
            ],
        };
        assert_eq!(
            state.title('▼'),
            "▼ Uncommitted Changes (3)  — working directory vs HEAD"
        );
        assert_eq!(
            state.title('v'),
            "v Uncommitted Changes (3)  — working directory vs HEAD"
        );
        assert_eq!(state.counts(), [1, 1, 1, 0]);
        assert_eq!(state.staged_count(), 1);
        assert_eq!(state.unstaged_count(), 3);
        assert_eq!(state.totals(), (412, 93));
        assert_eq!(
            state.summary_line(),
            "   A 1  ·  M 1  ·  D 1  ·  R 0   ·   staged 1 · unstaged 3   ·   +412 / −93"
        );
    }

    #[test]
    fn an_empty_section_is_empty() {
        let state = UncommittedState::default();
        assert!(state.is_empty());
        assert_eq!(
            state.title('▼'),
            "▼ Uncommitted Changes (0)  — working directory vs HEAD"
        );
        assert_eq!(state.totals(), (0, 0));
    }
}
