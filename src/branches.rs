//! The **Branches** tab data (M4-W12 / T2).
//!
//! The reference extension has a branch/tag/remote dropdown
//! (`src/dataSource.ts`, `getBranches`). W12 promotes that to a persistent left
//! pane in the second tab: every local branch, remote-tracking branch and tag
//! with the number of commits it reaches, and clicking one filters the history
//! pane on the right to exactly the commits reachable from that ref.
//!
//! The reachable set comes from `git rev-list <ref>`; the history pane then
//! keeps only the layout rows whose hash is in that set, which is what makes the
//! filtered count match the badge on the branch row.

use std::collections::HashSet;
use std::path::Path;

use crate::data::{self, GitError};

/// The kind of ref a [`BranchEntry`] describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RefKind {
    /// `refs/heads/*`.
    Local,
    /// `refs/remotes/*`.
    Remote,
    /// `refs/tags/*`.
    Tag,
}

impl RefKind {
    /// The label the left pane groups by.
    pub fn label(self) -> &'static str {
        match self {
            RefKind::Local => "LOCAL BRANCHES",
            RefKind::Remote => "REMOTES",
            RefKind::Tag => "TAGS",
        }
    }

    /// The design's one-letter badge (`L`/`R`/`T`).
    pub fn badge(self) -> char {
        match self {
            RefKind::Local => 'L',
            RefKind::Remote => 'R',
            RefKind::Tag => 'T',
        }
    }

    /// The kind for a full refname, or `None` for anything else.
    pub fn from_refname(refname: &str) -> Option<Self> {
        if refname.starts_with("refs/heads/") {
            Some(RefKind::Local)
        } else if refname.starts_with("refs/remotes/") {
            Some(RefKind::Remote)
        } else if refname.starts_with("refs/tags/") {
            Some(RefKind::Tag)
        } else {
            None
        }
    }
}

/// One row in the branch pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchEntry {
    /// The short name (`main`, `origin/main`, `v1.0`).
    pub name: String,
    /// The full refname (`refs/heads/main`).
    pub refname: String,
    /// Local / remote / tag.
    pub kind: RefKind,
    /// Is this the checked-out branch?
    pub is_head: bool,
    /// Commits reachable from the tip.
    pub count: usize,
}

impl BranchEntry {
    /// The row's badge, with the HEAD marker the design uses.
    pub fn badge(&self) -> String {
        if self.is_head {
            format!("{}*", self.kind.badge())
        } else {
            self.kind.badge().to_string()
        }
    }

    /// `main   L*   42 commits`.
    pub fn label(&self) -> String {
        let suffix = if self.count == 1 { "commit" } else { "commits" };
        format!("{}  {}  {} {}", self.name, self.badge(), self.count, suffix)
    }
}

/// Parse `git for-each-ref --format=%(refname:short)%x1f%(objectname)%x1f%(refname)`.
///
/// `counts` is looked up by full refname. A symbolic `refs/remotes/*/HEAD` is
/// skipped — it duplicates the branch it points at.
pub fn parse_for_each_ref(
    output: &str,
    head_branch: Option<&str>,
    counts: &std::collections::HashMap<String, usize>,
) -> Vec<BranchEntry> {
    let mut out = Vec::new();
    for line in output.lines() {
        let mut fields = line.split('\u{1f}');
        let (Some(name), Some(_object), Some(refname)) =
            (fields.next(), fields.next(), fields.next())
        else {
            continue;
        };
        if name.is_empty() || refname.ends_with("/HEAD") {
            continue;
        }
        let Some(kind) = RefKind::from_refname(refname) else {
            continue;
        };
        let is_head = kind == RefKind::Local && head_branch == Some(name);
        out.push(BranchEntry {
            name: name.to_string(),
            refname: refname.to_string(),
            kind,
            is_head,
            count: counts.get(refname).copied().unwrap_or(0),
        });
    }
    // Local first, then remotes, then tags; the checked-out branch leads its
    // group, the rest are alphabetical.
    out.sort_by(|a, b| {
        (a.kind as u8, !a.is_head, &a.name).cmp(&(b.kind as u8, !b.is_head, &b.name))
    });
    out
}

/// Read every ref of `repo`, with its reachable-commit count.
pub fn load(repo: &Path) -> Result<Vec<BranchEntry>, GitError> {
    let output = data::run_git(
        repo,
        &[
            "for-each-ref",
            "--format=%(refname:short)\u{1f}%(objectname)\u{1f}%(refname)",
            "refs/heads",
            "refs/remotes",
            "refs/tags",
        ],
    )?;
    let head_branch = data::head(repo).ok().and_then(|head| head.branch);
    let mut counts = std::collections::HashMap::new();
    for line in output.lines() {
        let refname = line.rsplit('\u{1f}').next().unwrap_or_default();
        if refname.is_empty() || refname.ends_with("/HEAD") {
            continue;
        }
        if let Ok(text) = data::run_git(repo, &["rev-list", "--count", refname]) {
            if let Ok(count) = text.trim().parse::<usize>() {
                counts.insert(refname.to_string(), count);
            }
        }
    }
    Ok(parse_for_each_ref(&output, head_branch.as_deref(), &counts))
}

/// The hashes reachable from `refname` (`git rev-list <refname>`).
pub fn reachable(repo: &Path, refname: &str) -> Result<HashSet<String>, GitError> {
    let output = data::run_git(repo, &["rev-list", refname])?;
    Ok(output
        .lines()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect())
}

/// Keep only the rows whose hash is in `reachable`, preserving row order.
pub fn filter_rows<'a, I>(rows: I, reachable: &HashSet<String>) -> Vec<usize>
where
    I: IntoIterator<Item = (usize, &'a str)>,
{
    rows.into_iter()
        .filter(|(_, hash)| reachable.contains(*hash))
        .map(|(index, _)| index)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn counts(pairs: &[(&str, usize)]) -> HashMap<String, usize> {
        pairs
            .iter()
            .map(|(name, count)| (name.to_string(), *count))
            .collect()
    }

    const REFS: &str = "\
main\u{1f}aaa\u{1f}refs/heads/main
feature/ui\u{1f}bbb\u{1f}refs/heads/feature/ui
origin/main\u{1f}aaa\u{1f}refs/remotes/origin/main
origin/HEAD\u{1f}aaa\u{1f}refs/remotes/origin/HEAD
v1.0\u{1f}ccc\u{1f}refs/tags/v1.0
";

    #[test]
    fn refs_split_into_local_remote_and_tag() {
        let counts = counts(&[("refs/heads/main", 12), ("refs/remotes/origin/main", 12)]);
        let entries = parse_for_each_ref(REFS, Some("main"), &counts);
        assert_eq!(entries.len(), 4, "origin/HEAD is skipped");
        assert_eq!(entries[0].kind, RefKind::Local);
        assert_eq!(entries[0].name, "main");
        assert!(entries[0].is_head);
        assert_eq!(entries[0].count, 12);
        assert_eq!(entries[1].name, "feature/ui");
        assert!(!entries[1].is_head);
        assert_eq!(entries[2].kind, RefKind::Remote);
        assert_eq!(entries[3].kind, RefKind::Tag);
        assert_eq!(entries[3].count, 0, "a ref with no count reads 0");
    }

    #[test]
    fn a_head_row_shows_the_star_badge() {
        let counts = counts(&[("refs/heads/main", 1)]);
        let entries = parse_for_each_ref(REFS, Some("main"), &counts);
        assert_eq!(entries[0].badge(), "L*");
        assert_eq!(entries[0].label(), "main  L*  1 commit");
        assert_eq!(entries[1].badge(), "L");
    }

    #[test]
    fn filtering_keeps_only_reachable_rows_in_order() {
        let rows = vec![(0usize, "a"), (1, "b"), (2, "c")];
        let reachable: HashSet<String> = ["a", "c"].iter().map(|s| s.to_string()).collect();
        assert_eq!(filter_rows(rows, &reachable), vec![0, 2]);
    }

    #[test]
    fn an_unknown_refname_has_no_kind() {
        assert_eq!(RefKind::from_refname("refs/stash"), None);
        assert_eq!(RefKind::from_refname("refs/heads/x"), Some(RefKind::Local));
    }
}
