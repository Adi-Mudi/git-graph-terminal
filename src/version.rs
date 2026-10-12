//! Release identity (W11/T3): the semver and the exact revision the binary was
//! built from.
//!
//! `VERSION` is `Cargo.toml`'s `version` (the wave tag, `0.3.0` for M3).
//! `GIT_HASH` is injected by `build.rs` as `GGT_GIT_HASH` with
//! `git rev-parse --short HEAD`, or `"unknown"` on a source tree with no
//! `.git`. [`version_line`] is what `--version` prints.

/// The crate version from `Cargo.toml`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// The git short hash the binary was built from (`build.rs`), or `"unknown"`.
pub const GIT_HASH: &str = env!("GGT_GIT_HASH");

/// One line for `--version`: name, semver, short hash.
pub fn version_line() -> String {
    format!("git-graph-terminal {VERSION} ({GIT_HASH})")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_a_three_part_semver() {
        let parts: Vec<&str> = VERSION.split('.').collect();
        assert_eq!(parts.len(), 3, "VERSION {VERSION:?} is not semver");
        for part in parts {
            assert!(
                part.parse::<u64>().is_ok(),
                "VERSION {VERSION:?} has a non-numeric part {part:?}"
            );
        }
    }

    #[test]
    fn git_hash_is_present_and_short() {
        assert!(!GIT_HASH.is_empty(), "GIT_HASH must never be empty");
        // A real short hash is 7-40 hex chars; the tarball fallback is "unknown".
        if GIT_HASH != "unknown" {
            assert!(
                (7..=40).contains(&GIT_HASH.len())
                    && GIT_HASH.chars().all(|c| c.is_ascii_hexdigit()),
                "GIT_HASH {GIT_HASH:?} is not a git short hash"
            );
        }
    }

    #[test]
    fn version_line_names_the_binary_semver_and_hash() {
        let line = version_line();
        assert!(line.starts_with("git-graph-terminal "));
        assert!(line.contains(VERSION));
        assert!(line.contains(GIT_HASH));
        assert!(!line.contains('\n'));
    }
}
