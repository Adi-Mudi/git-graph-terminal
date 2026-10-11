# M1-W2 lane report — integration + parity gate + perf smoke

Lane: `w2` · Branch: `ggt-m1-w2` · Worktree: `../git-graph-terminal-w2`
Base: `development` @ `b651826` · Plan: `.IDE_Plans/w2-plan_v1.0.md`
Date: 2026-10-11

This lane closes the F3 gap: the real `data::commits → layout::layout() → render` path
now runs in the TUI, the whole path is pinned by an end-to-end golden test, and the
layout engine got a quadratic hot-spot removed after a 30k-commit perf smoke tripped
the budget (see **Deviation 4**).

---

## 1. Deliverables

| # | Item | File |
|---|------|------|
| 1 | Real path wired into the TUI (`--dump`/`--text-list` keep the P0 list) | `src/main.rs` |
| 2 | End-to-end test: real repo → layout → golden JSON + `TestBackend` render smoke | `tests/integration.rs` |
| 3 | Shared golden parse/parity helpers (used by both test crates) | `tests/common/mod.rs` |
| 4 | `golden_layout.rs` refactored onto the shared helpers | `tests/golden_layout.rs` |
| 5 | 30k perf smoke + budget trip-wires + peak RSS | `tests/perf.rs` |
| 6 | Deterministic ≥30k-commit repo generator | `tools/mkbig-repo.sh` |
| 7 | pty captures (real / text-list / mock / 30k) | `tools/pty-shot.sh` → `target/pty/*` |
| 8 | O(n²) fix in `to_graph_layout()` (approved before edit) | `src/layout/mod.rs` |

## 2. Commands

```bash
cargo test                                   # 49 unit + 15 golden + 45 integration + 43 perf
cargo test --release --test perf -- --nocapture
cargo clippy --all-targets && cargo fmt --check

tools/mkbig-repo.sh                          # /tmp/ggt-big, 30 600 commits, 301 branches

MOCK=0             ROWS=40 COLS=120 tools/pty-shot.sh w2-real-graph
MOCK=0 ARGS=--text-list ROWS=40 COLS=120 tools/pty-shot.sh w2-text-list
MOCK=1             ROWS=24 COLS=100 tools/pty-shot.sh w2-mock
( cd /tmp/ggt-big && MOCK=0 ROWS=40 COLS=120 <worktree>/tools/pty-shot.sh w2-big-repo )
```

## 3. Test results

`cargo test` — **all green**, 152 tests:

| Crate | Tests | What it pins |
|-------|-------|--------------|
| unit | 49 | data / layout / render / glyphs / mock |
| `golden_layout` | 15 | `layout()` alone vs the G8 reference goldens |
| `integration` | 45 | **`data::commits → layout()`** vs the same goldens + `TestBackend` glyphs |
| `perf` | 43 | 30k budget trip-wires (42 = included module tests) |

`cargo fmt --check` clean; `cargo clippy --all-targets` clean.

The two parity suites re-derive each expected `Segment` only from the golden's own
geometry + `lockedFirst`, so they cannot just re-assert the port's output. After the
Deviation-4 fix both parity suites stayed green — that is the behaviour-preservation
proof for the refactor.

## 4. Perf smoke (T3)

Repo: `tools/mkbig-repo.sh` → **30 600 commits, 301 branches, 2 lanes**, peak RSS ≈ 38 MB.
`git log` (data layer): ≈ 196–240 ms.

| Measurement | Before fix (release) | Before fix (debug) | **After fix (release)** | **After fix (debug)** | Budget |
|-------------|---------------------:|-------------------:|------------------------:|----------------------:|-------:|
| `layout()` 30k | 473 ms | 4 049 ms | **29 ms** | **52 ms** | ≤ 1.0 s |
| data + layout + first frame | — | — | **225 ms** | **292 ms** | ≤ 2.0 s |

- "First frame" is `data::commits` + `layout()` + one `TestBackend` viewport draw (a
  deterministic proxy for the TUI's first paint; the pty captures below confirm the real
  first frame renders — the 30k capture's status bar reads `commits: 30600`).
- Both paths are inside the W2 budget on the release profile after the fix.

## 5. Evidence captures (`target/pty/`)

**(a) Real repo — this project's own history, borderless graph, 120×40 pty**

Graph column (all 21 rows; ◯ = open HEAD node, `●─╮` / `├─●` = merge steps, two lanes):

```text
◯       1
●─╮     2
● │     3
● │     4
● │     5
│ ●     6
│ ●     7
│ ●     8
├─●     9
●      10
```

Status bar: `repo: .  branch: ggt-m1-w2  head: b651826  commits: 21  branches: 6  lanes: 2  graph: 4 cols`

**(b) `--text-list` fallback (P0 bordered list)** — `target/pty/w2-text-list.txt`, and the
deterministic no-pty frame `target/pty/w2-real-graph.frame`:

```text
┌ commits (21) · branches (6) — . ─────────────────────────────────────────────────────────────────┐
│>     1 b651826 docs(verify): W1 wave verification report (cline-w1, verdict ACCEPT)  (HEAD -> ggt│
│      2 c8d7609 merge: lane b1 (renderer) into development; layout/* resolved to a1 per contract  │
│      3 2d24fbe docs(report): M1-W1 a1 lane report  (ggt-m1-w1-a1)                                │
└──────────────────────────────────────────────────────────────────────────────────────────────────┘
 repo: .  branch: ggt-m1-w2  head: b651826  commits: 21  branches: 6  panes: j/k/↑/↓ scroll · r relo
```

**(c) Mock graph** — deterministic frame `target/pty/w2-mock.frame` (graph + refs + status):

```text
      idx hash     date       subject                                            refs
┄       1 -------  2023-11-14 Uncommitted changes
◯       2 a1b2c3d  2023-11-14 Add graph renderer widget                          HEAD -> main
●─╮     3 b2c3d4e  2023-11-14 Merge branch 'feature/graph'
│ ●     4 c3d4e5f  2023-11-14 Draw rounded corners
│ ◎     5 d4e5f6a  2023-11-14 WIP stash demo                                      stash@{0}
●       6 e5f6a7b  2023-11-14 Base commit                                         tag: v0.1
  …     (rows 7–8: Second root, Orphan root omitted)
 repo: (mock)  branch: main  head: a1b2c3d  commits: 8  branches: 2  lanes: 2  graph: 4 cols
```

**(d) 30k repo (first-frame sanity)** — `target/pty/w2-big-repo.*`:
`repo: .  branch: main  head: b5cb830c  commits: 30600  branches: 301  lanes: 2  graph: 4 cols`.

Every capture leaves four artifacts: `.raw` (pty bytes), `.txt` (readable), `.strings.txt`,
and — for non-interactive modes — `.frame` (deterministic, CI-diffable).

## 6. Deviations

1. **`tests/perf.rs` added (not named in plan T3).** The plan named only
   `tools/mkbig-repo.sh`. A committed, assertion-bearing test was the only way to make
   the budget a regression guard *and* measure `layout()` time + peak RSS without extra
   binary flags. `tools/mkbig-repo.sh` was delivered too (shared shape).
2. **`tools/pty-shot.sh` gained an `ARGS` env var.** Needed to capture `--text-list`;
   the deterministic `.frame` step is skipped for interactive modes.
3. **Perf ceilings are profile-aware.** The plan budget (1.0 s / 2.0 s) is enforced on
   the release profile; debug uses a looser trip-wire (2.0 s / 3.0 s) so the default
   debug `cargo test` stays green while still catching order-of-magnitude blow-ups.
4. **Layout change: O(n²) → O(n) in `to_graph_layout()`.** The plan allows layout edits
   "ONLY with a failing golden test first + explicit report section". No *golden* test
   failed; the **perf smoke failed** (debug `layout()` 4 049 ms > 1 000 ms) first and is
   recorded above. The failing test + measured before/after numbers are in §4. The fix
   buckets branch lines by start row in one pass instead of re-scanning every line for
   every row; each line still lands in exactly one row's segment list in the same order,
   so output is unchanged (both parity suites stayed green). Explicitly approved by the
   developer before the edit.
5. **First-frame is a `TestBackend` proxy**, not a pty wall-clock timestamp (the plan
   suggested "pty timestamp"); the pty captures confirm the real first frame renders.

## 7. Acceptance mapping

1. **FULL TEST green** → `cargo test`: 152/152 (§3).
2. **Real-repo TUI shows the borderless graph** → §5(a) pty capture + `graph: 4 cols` status.
3. **Perf recorded against budget** → §4, inside budget after the fix.
4. **F3 closed; boundaries intact** → `src/layout` port logic otherwise untouched; no
   `tools/g8-dump` goldens changed; no reference-repo mutation; no push.
