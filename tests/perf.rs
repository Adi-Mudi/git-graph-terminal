//! Perf smoke (W2, T3): a ≥30k-commit history must stay inside the frame budget.
//!
//! Generates a deterministic repo with `git fast-import` (one process, no node, no
//! network) — a long trunk with a short-lived side branch merged back every
//! [`CHUNK`] commits — then measures, in one run:
//!
//! * `data::commits` wall time (the real `git log` call),
//! * `layout::layout` wall time over the whole history,
//! * one viewport render into a ratatui `TestBackend` (the first-frame draw),
//! * the process peak RSS (`VmHWM`, Linux).
//!
//! Budgets match the W2 plan on the **release** profile: `layout()` ≤ **1.0 s**, whole
//! data→layout→frame path ≤ **2.0 s**. Debug builds run several times slower, so the
//! trip-wire ceiling is raised there; a quadratic (or worse) regression blows past both.
//!
//! The same repo can be regenerated for manual/pty runs with `tools/mkbig-repo.sh`.

#[allow(dead_code)] // `data.rs` is FROZEN and shared; the test only uses part of it.
#[path = "../src/data.rs"]
mod data;
#[path = "../src/layout/mod.rs"]
mod layout;

#[path = "../src/render/cell.rs"]
pub mod cell;
/// Render modules for the viewport draw; re-exported under `render` so the source
/// files' `crate::render::<module>` paths keep working.
#[path = "../src/render/glyphs.rs"]
pub mod glyphs;
#[path = "../src/render/graph_widget.rs"]
pub mod graph_widget;
#[path = "../src/render/mock.rs"]
pub mod mock;
/// W8: `graph_widget` now degrades through `term::ladder`.
#[path = "../src/term/mod.rs"]
pub mod term;

mod render {
    pub use crate::{cell, glyphs, graph_widget, mock};
}

use std::collections::HashSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

use layout::{layout, LayoutOpts};
use ratatui::backend::TestBackend;
use ratatui::Terminal;
use render::graph_widget::GraphWidget;
use term::ladder::{ColourTier, Ladder};

/// Trunk commits to generate (must be divisible by [`CHUNK`]).
const COMMITS: usize = 30_000;
/// Trunk commits between two side-branch/merge cycles.
const CHUNK: usize = 100;
/// W2 budget for `layout()` alone (release profile; debug trip-wire is looser).
const LAYOUT_BUDGET_MS: u128 = if cfg!(debug_assertions) { 2_000 } else { 1_000 };
/// W2 budget for the whole data → layout → first frame path (release profile).
const FRAME_BUDGET_MS: u128 = if cfg!(debug_assertions) { 3_000 } else { 2_000 };
/// Viewport used for the first-frame draw.
const VIEW_ROWS: u16 = 40;
const VIEW_COLS: u16 = 120;

#[test]
fn large_history_stays_inside_the_frame_budget() {
    assert_eq!(COMMITS % CHUNK, 0, "COMMITS must be divisible by CHUNK");
    let chunks = COMMITS / CHUNK;

    let repo = temp_repo();
    make_big_repo(&repo, COMMITS, CHUNK);

    let started = Instant::now();
    let commits = data::commits(&repo).expect("data::commits on the generated repo");
    let data_ms = started.elapsed().as_millis();

    let expected = COMMITS + 2 * chunks;
    assert_eq!(
        commits.len(),
        expected,
        "generated repo must expose every trunk/merge/side commit"
    );

    let started = Instant::now();
    let graph = layout(&commits, &LayoutOpts::default());
    let layout_ms = started.elapsed().as_millis();

    let render_ms = render_viewport_ms(&graph);
    let frame_ms = data_ms + layout_ms + render_ms;
    let peak_rss_kb = peak_rss_kb();

    eprintln!(
        "perf: {} commits, {} lanes | git log {data_ms} ms, layout {layout_ms} ms, \
         frame(mock) {frame_ms} ms | peak RSS {} MB",
        commits.len(),
        graph.max_lane + 1,
        peak_rss_kb
            .map(|kb| format!("{:.1}", kb as f64 / 1024.0))
            .unwrap_or_else(|| "?".to_string())
    );

    assert_eq!(graph.rows.len(), commits.len(), "one layout row per commit");
    assert!(
        graph.max_lane >= 1,
        "the merge cycles must produce more than one lane"
    );
    assert!(
        layout_ms <= LAYOUT_BUDGET_MS,
        "layout() over {} commits took {layout_ms} ms (budget {LAYOUT_BUDGET_MS} ms)",
        commits.len()
    );
    assert!(
        frame_ms <= FRAME_BUDGET_MS,
        "data + layout + frame over {} commits took {frame_ms} ms (budget {FRAME_BUDGET_MS} ms)",
        commits.len()
    );

    cleanup(repo);
}

/// Draw the first viewport of `graph` into a `TestBackend` and return the wall time.
fn render_viewport_ms(graph: &layout::GraphLayout) -> u128 {
    let backend = TestBackend::new(VIEW_COLS, VIEW_ROWS);
    let mut terminal = Terminal::new(backend).expect("TestBackend is infallible");
    let started = Instant::now();
    terminal
        .draw(|frame| {
            let area = frame.area();
            frame.render_widget(
                GraphWidget {
                    layout: graph,
                    ladder: Ladder::unlimited().with_colour(ColourTier::Mono),
                    selected: None,
                    uncommitted: None,
                    scroll: 0,
                    expanded: &HashSet::new(),
                    detail_rows: 0,
                },
                area,
            );
        })
        .expect("TestBackend draw is infallible");
    started.elapsed().as_millis()
}

/// Peak resident set size in KiB from `/proc/self/status` (`VmHWM`); `None` off Linux.
fn peak_rss_kb() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    status
        .lines()
        .find(|line| line.starts_with("VmHWM:"))
        .and_then(|line| line.split_whitespace().nth(1))
        .and_then(|value| value.parse().ok())
}

/// A process-unique scratch repository under the system temp directory.
fn temp_repo() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ggt-perf-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create perf scratch dir");
    dir
}

fn cleanup(dir: PathBuf) {
    let _ = std::fs::remove_dir_all(dir);
}

/// `git -C <dir> <args>`; panics with the real stderr on failure.
fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap_or_else(|error| panic!("cannot run git {:?}: {error}", args));
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Append one `commit` command to a fast-import stream.
fn emit_commit(
    out: &mut String,
    refname: &str,
    mark: usize,
    time: u64,
    message: &str,
    from: Option<usize>,
    merges: &[usize],
) {
    out.push_str("commit ");
    out.push_str(refname);
    out.push('\n');
    out.push_str(&format!("mark :{mark}\n"));
    out.push_str(&format!("author Perf <perf@example.com> {time} +0000\n"));
    out.push_str(&format!("committer Perf <perf@example.com> {time} +0000\n"));
    out.push_str(&format!("data {}\n", message.len()));
    out.push_str(message);
    out.push('\n');
    if let Some(from) = from {
        out.push_str(&format!("from :{from}\n"));
    }
    for merge in merges {
        out.push_str(&format!("merge :{merge}\n"));
    }
}

/// Build a deterministic repo: a trunk with a side branch merged back every `chunk`.
fn make_big_repo(dir: &Path, commits: usize, chunk: usize) {
    git(dir, &["init", "-q"]);
    // `git init` may default to `master`; pin HEAD so the repo has a born branch.
    git(dir, &["symbolic-ref", "HEAD", "refs/heads/main"]);

    let mut stream = String::new();
    let mut mark = 0usize;
    let mut main_mark = 0usize;
    let mut time = 1_600_000_000u64;
    let mut done = 0usize;
    let mut chunk_index = 0usize;

    while done < commits {
        for _ in 0..chunk {
            mark += 1;
            time += 1;
            emit_commit(
                &mut stream,
                "refs/heads/main",
                mark,
                time,
                &format!("trunk {mark}"),
                (main_mark > 0).then_some(main_mark),
                &[],
            );
            main_mark = mark;
            done += 1;
        }

        // A one-commit side branch off the current trunk tip, then merged back.
        let side_base = main_mark;
        let side_ref = format!("refs/heads/side-{chunk_index}");
        stream.push_str(&format!("reset {side_ref}\nfrom :{side_base}\n"));
        mark += 1;
        time += 1;
        emit_commit(
            &mut stream,
            &side_ref,
            mark,
            time,
            &format!("side {chunk_index}"),
            Some(side_base),
            &[],
        );
        let side_mark = mark;

        mark += 1;
        time += 1;
        emit_commit(
            &mut stream,
            "refs/heads/main",
            mark,
            time,
            &format!("merge {chunk_index}"),
            Some(main_mark),
            &[side_mark],
        );
        main_mark = mark;

        chunk_index += 1;
    }

    let mut child = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["fast-import", "--quiet", "--force"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn git fast-import");
    child
        .stdin
        .take()
        .expect("fast-import stdin")
        .write_all(stream.as_bytes())
        .expect("write fast-import stream");
    let output = child.wait_with_output().expect("wait for fast-import");
    assert!(
        output.status.success(),
        "git fast-import failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
