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

use std::process::Command;

fn main() {
    // Rebuild when the checkout moves so the stamp tracks the revision.
    println!("cargo:rerun-if-changed=.git");
    println!("cargo:rerun-if-changed=build.rs");

    let hash = git_short_hash().unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=GGT_GIT_HASH={hash}");
}

/// `git rev-parse --short HEAD`, or `None` when git/the repo is unavailable.
fn git_short_hash() -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let hash = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if hash.is_empty() {
        None
    } else {
        Some(hash)
    }
}
