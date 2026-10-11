//! G8 golden-fixture parity test (W1, lane a1 — task 3 + task 4).
//!
//! Loads every committed golden JSON under `tests/fixtures/golden/` (produced by
//! `tools/g8-dump` from the **reference** `web/graph.ts`), rebuilds the same commit
//! list, runs this crate's [`layout`] on it and asserts the output is identical:
//!
//! * rows: `lane`, `colour`, `parents` (incl. the `-1 == NULL_VERTEX_ID` sentinel),
//!   `lockedFirst`
//! * row nodes: `isCurrent`, `onBranch`, `isStash`
//! * segments: exact line geometry `(row_from, row_to, col_from, col_to)`, the
//!   geometric `kind`, and the crossing `pers` winner
//! * `max_lane` and the frozen `palette`
//!
//! The expected [`Segment`](layout::Segment) values are re-derived **only** from the
//! golden's own geometry and `lockedFirst`, so the test cannot simply re-assert what
//! the port did. The shared parsing/assertion helpers live in [`common`]; W2's
//! `tests/integration.rs` uses the same helpers to pin the `data → layout` path.
//!
//! The crate has no library target (the whole app is a binary), so the two modules
//! under test are pulled in directly. Both are a1-owned; b1's `src/main.rs` and
//! `src/render/` are not touched.

mod common;
#[allow(dead_code)] // `data.rs` is FROZEN and shared; the test only uses part of it.
#[path = "../src/data.rs"]
mod data;
#[path = "../src/layout/mod.rs"]
mod layout;

use std::fs;
use std::path::{Path, PathBuf};

use common::{assert_layout_matches, build_commits, golden_from, read_golden};
use layout::{layout, LayoutOpts};

#[test]
fn golden_fixtures_match_the_reference_layout() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/golden");
    let mut paths: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", dir.display()))
        .map(|entry| entry.expect("dir entry").path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("json"))
        .collect();
    paths.sort();

    assert!(
        paths.len() >= 3,
        "expected at least three committed golden fixtures in {}, found {}",
        dir.display(),
        paths.len()
    );

    let mut null_vertex_parents = 0usize;
    for path in &paths {
        null_vertex_parents += check_fixture(path);
        eprintln!(
            "ok   golden parity: {}",
            path.file_name().unwrap_or_default().to_string_lossy()
        );
    }

    // D4: the `parents: [-1]` (`NULL_VERTEX_ID`) sentinel must be Golden-covered. The
    // `branch-merge-rebase-truncated` fixture exists for exactly this: a commit at the
    // commit-window boundary has a parent outside the loaded graph.
    assert!(
        null_vertex_parents >= 1,
        "expected at least one golden row with parents: [-1] (NULL_VERTEX_ID); the truncated-window fixture should provide it"
    );
}

/// Assert exact parity for one golden file.
///
/// Returns how many `parents: [-1]` (`NULL_VERTEX_ID`) entries the fixture contains, so
/// the caller can prove the reachable sentinel is actually exercised.
fn check_fixture(path: &Path) -> usize {
    let json = read_golden(path);
    let name = json.get("fixture").as_str().to_string();
    let commits = build_commits(&json);
    let graph = layout(&commits, &LayoutOpts::default());
    let golden = golden_from(&json);
    assert_layout_matches(&name, &graph, &commits, &golden)
}
