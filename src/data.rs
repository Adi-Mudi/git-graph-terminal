//! Git data access (decision **D9**: shell out to the `git` binary, parse
//! machine-readable output).
//!
//! Mirrors the reference extension's back end (`src/dataSource.ts`, private
//! `getLog()`): one `git log` call with a `%x1f`-separated `--format`, records
//! terminated by NUL (`-z`) so subjects containing newlines cannot break parsing.
//!
//! Field order is the P0 contract from `p0-plan_v1.0.md` T2:
//! hash, abbrev, parents, author, date, subject, refs.

use std::fmt;
use std::path::Path;
use std::process::Command;

/// Unit Separator (`%x1f`) — field separator inside one log record.
const FS: char = '\u{1f}';
/// NUL — record separator produced by `git log -z`.
const RS: char = '\u{0}';

/// `git log --format` field list (7 fields, FS-separated).
const LOG_FORMAT: &str = "--format=%H\u{1f}%h\u{1f}%P\u{1f}%an\u{1f}%at\u{1f}%s\u{1f}%D";

/// A failure of the `git` subprocess (never a panic).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitError {
    /// The command line that was run, for the error message.
    pub command: String,
    /// Process exit code, `None` when the process could not be spawned/killed.
    pub code: Option<i32>,
    /// Captured stderr (trimmed).
    pub stderr: String,
}

impl fmt::Display for GitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "`{}` failed", self.command)?;
        if let Some(code) = self.code {
            write!(f, " (exit {code})")?;
        }
        if !self.stderr.is_empty() {
            write!(f, ": {}", self.stderr)?;
        }
        Ok(())
    }
}

impl std::error::Error for GitError {}

/// One commit, as needed by the graph. Mirrors the reference `GG.GitCommit`
/// fields that the layout consumes (`hash`, `parents`) plus display fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commit {
    /// Full object name.
    pub hash: String,
    /// Abbreviated object name (`git`'s default abbreviation).
    pub abbrev: String,
    /// Full object names of the parents, in `git` order (first parent first).
    pub parents: Vec<String>,
    /// Author name.
    pub author: String,
    /// Author UNIX timestamp (seconds).
    pub date: i64,
    /// Subject line (first line of the message).
    pub subject: String,
    /// Decorated refs (`%D`) split on `, `, empty when the commit has none.
    pub refs: Vec<String>,
}

/// One local branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Branch {
    /// Short ref name, e.g. `main`.
    pub name: String,
    /// Abbreviated tip object name.
    pub head: String,
    /// Whether this is the currently checked-out branch.
    pub is_current: bool,
}

/// HEAD state: `branch` is `None` on a detached HEAD, `hash` is `None` when the
/// repository has no commits yet (unborn branch).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Head {
    /// Current branch name (`symbolic-ref --short`), `None` when detached.
    pub branch: Option<String>,
    /// Abbreviated HEAD object name, `None` for an unborn branch.
    pub hash: Option<String>,
}

/// Run `git <args...>` in `repo` and return stdout.
///
/// `-c log.showSignature=false` is passed by callers exactly like the reference
/// extension does (`src/dataSource.ts:1500`), so GPG noise never reaches parsing.
pub fn run_git(repo: &Path, args: &[&str]) -> Result<String, GitError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .map_err(|e| GitError {
            command: format!("git -C {} {}", repo.display(), args.join(" ")),
            code: None,
            stderr: e.to_string(),
        })?;

    let command = format!("git -C {} {}", repo.display(), args.join(" "));
    if !output.status.success() {
        return Err(GitError {
            command,
            code: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Parse one `git log -z --format=<LOG_FORMAT>` payload into commits.
///
/// Records that do not carry all 7 fields are skipped rather than guessed at.
pub fn parse_log(output: &str) -> Vec<Commit> {
    let mut commits = Vec::new();
    for record in output.split(RS) {
        if record.is_empty() {
            continue;
        }
        let fields: Vec<&str> = record.split(FS).collect();
        if fields.len() != 7 {
            continue;
        }
        commits.push(Commit {
            hash: fields[0].to_string(),
            abbrev: fields[1].to_string(),
            parents: fields[2]
                .split(' ')
                .filter(|p| !p.is_empty())
                .map(|p| p.to_string())
                .collect(),
            author: fields[3].to_string(),
            date: fields[4].parse::<i64>().unwrap_or(0),
            subject: fields[5].to_string(),
            refs: fields[6]
                .split(", ")
                .map(|r| r.trim().to_string())
                .filter(|r| !r.is_empty())
                .collect(),
        });
    }
    commits
}

/// Is `repo` inside a git work tree?
pub fn is_repo(repo: &Path) -> bool {
    run_git(repo, &["rev-parse", "--git-dir"]).is_ok()
}

/// HEAD state of `repo`; tolerant of detached HEAD and unborn branches.
pub fn head(repo: &Path) -> Result<Head, GitError> {
    let branch = run_git(repo, &["symbolic-ref", "--short", "-q", "HEAD"])
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    let hash = run_git(
        repo,
        &["rev-parse", "--short", "--verify", "--quiet", "HEAD"],
    )
    .ok()
    .map(|s| s.trim().to_string())
    .filter(|s| !s.is_empty());
    Ok(Head { branch, hash })
}

/// All commits reachable from any ref, newest first (`--date-order`, the same
/// default ordering the reference uses for the graph).
///
/// A repository without commits (unborn branch) yields an empty list — not an
/// error, per the T2 contract.
pub fn commits(repo: &Path) -> Result<Vec<Commit>, GitError> {
    // Fails with the real `git` message when `repo` is not a work tree.
    run_git(repo, &["rev-parse", "--git-dir"])?;
    if head(repo)?.hash.is_none() {
        // Unborn branch: `git log` would fail, the graph is simply empty.
        return Ok(Vec::new());
    }
    let out = run_git(
        repo,
        &[
            "-c",
            "log.showSignature=false",
            "log",
            "--all",
            "--date-order",
            "-z",
            LOG_FORMAT,
        ],
    )?;
    Ok(parse_log(&out))
}

/// Local branches (`refs/heads`), plus which one is checked out.
pub fn branches(repo: &Path) -> Result<Vec<Branch>, GitError> {
    // Branch names cannot contain spaces, so a space is a safe field separator here.
    let out = run_git(
        repo,
        &[
            "for-each-ref",
            "--format=%(refname:short) %(objectname:short) %(HEAD)",
            "refs/heads",
        ],
    )?;
    let mut branches = Vec::new();
    for line in out.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split(' ');
        let (Some(name), Some(head)) = (parts.next(), parts.next()) else {
            continue;
        };
        let is_current = parts.next().map(|f| f.starts_with('*')).unwrap_or(false);
        branches.push(Branch {
            name: name.to_string(),
            head: head.to_string(),
            is_current,
        });
    }
    Ok(branches)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn record(hash: &str, abbrev: &str, parents: &str, subject: &str, refs: &str) -> String {
        [hash, abbrev, parents, "Ada", "1700000000", subject, refs].join(&FS.to_string())
    }

    #[test]
    fn parses_two_commits_with_parents_and_refs() {
        let head = record(
            "a".repeat(40).as_str(),
            "aaaaaaa",
            "bbbb",
            "tip",
            "HEAD -> main, tag: v1",
        );
        let root = record("b".repeat(40).as_str(), "bbbbbbb", "", "root", "");
        let out = format!("{head}{RS}{root}{RS}");

        let commits = parse_log(&out);
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].abbrev, "aaaaaaa");
        assert_eq!(commits[0].parents, vec!["bbbb".to_string()]);
        assert_eq!(commits[0].date, 1_700_000_000);
        assert_eq!(commits[0].subject, "tip");
        assert_eq!(commits[0].refs, vec!["HEAD -> main", "tag: v1"]);
        assert!(commits[1].parents.is_empty());
        assert!(commits[1].refs.is_empty());
    }

    #[test]
    fn skips_malformed_and_accepts_empty_payload() {
        assert!(parse_log("").is_empty());
        assert!(parse_log("no-field-separators-here").is_empty());
        assert!(parse_log(&format!("{RS}{RS}")).is_empty());
    }

    #[test]
    fn subject_with_newline_cannot_break_records() {
        // `-z` keeps every record NUL-terminated, so newlines stay inside a subject.
        let out = format!("{}{RS}", record("c", "ccccccc", "", "line1\nline2", ""));
        let commits = parse_log(&out);
        assert_eq!(commits.len(), 1);
        assert!(commits[0].subject.contains('\n'));
    }

    #[test]
    fn non_repository_is_an_error_not_a_panic() {
        let err = commits(Path::new("/nonexistent-ggt-p0-repo")).unwrap_err();
        assert!(err.to_string().contains("failed"));
    }

    #[test]
    fn unborn_branch_yields_no_commits() {
        let dir: PathBuf =
            std::env::temp_dir().join(format!("ggt-p0-unborn-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp dir");
        run_git(&dir, &["init", "-q"]).expect("git init");

        assert_eq!(commits(&dir).expect("no commits"), Vec::new());
        let head = head(&dir).expect("head");
        assert!(head.hash.is_none());
        assert!(branches(&dir).expect("branches").is_empty());

        fs::remove_dir_all(&dir).expect("clean temp dir");
    }
}
