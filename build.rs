//! Build script (W11/T3): stamp the git short hash into the binary.
//!
//! `--version` must report a semver **and** the exact revision the binary was
//! built from. The semver comes from `Cargo.toml` (`CARGO_PKG_VERSION`); the
//! revision is captured here, at build time, with `git rev-parse --short HEAD`
//! and handed to the crate as `GGT_GIT_HASH`.
//!
//! Build-time capture is deliberate: `git-graph-terminal` is installed with
//! `cargo install`, so at runtime the process is very often *not* inside the
//! repository it was built from. A runtime `git` call would report the user's
//! current directory instead of the binary's revision.
//!
//! The hash is best-effort: a source tarball with no `.git` still builds, and
//! `--version` then prints `unknown` rather than failing the build.

use std::path::Path;
use std::process::Command;

fn main() {
    // Rebuild when the checkout moves so the stamp tracks the revision.
    println!("cargo:rerun-if-changed=build.rs");
    watch_git_metadata();

    let hash = git(&["rev-parse", "--short", "HEAD"]).unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=GGT_GIT_HASH={hash}");
}

/// Ask Cargo to rerun this script whenever the commit changes.
///
/// Watching `.git` alone is **not** enough: inside a git worktree `.git` is a
/// *file* (a `gitdir:` pointer), and a commit writes to the real git directory,
/// never to that file — so the stamp would freeze at the revision the worktree
/// was created from. Watch the two files that actually move on commit: the git
/// directory's `HEAD` (checkout) and the branch ref (commit), both resolved
/// through git so a normal checkout and a worktree are covered alike.
fn watch_git_metadata() {
    if let Some(git_dir) = git(&["rev-parse", "--absolute-git-dir"]) {
        println!("cargo:rerun-if-changed={git_dir}/HEAD");
    }
    if let Some(common_dir) = git_common_dir() {
        if let Some(symref) = git(&["symbolic-ref", "-q", "HEAD"]) {
            println!("cargo:rerun-if-changed={common_dir}/{symref}");
        }
    }
}

/// The absolute git common directory (shared by all worktrees).
///
/// `--path-format=absolute` needs git 2.31+; older git returns a path relative
/// to the working directory, which for a build script is the package root, so
/// it is resolved against `CARGO_MANIFEST_DIR` in that case.
fn git_common_dir() -> Option<String> {
    if let Some(dir) = git(&["rev-parse", "--path-format=absolute", "--git-common-dir"]) {
        return Some(dir);
    }
    let dir = git(&["rev-parse", "--git-common-dir"])?;
    if Path::new(&dir).is_absolute() {
        return Some(dir);
    }
    let base = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
    Some(Path::new(&base).join(dir).to_string_lossy().into_owned())
}

/// `git <args>` stdout, trimmed, or `None` when git/the repo is unavailable.
fn git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}
