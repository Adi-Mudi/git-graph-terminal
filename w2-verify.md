# M1-W2 wave verification (independent) — cline-w2

| | |
|---|---|
| Initiative | `git-graph-terminal` (M1-W2: integration + parity gate + perf smoke) |
| Verifier | cline-w2 (independent) |
| PM | Hermes |
| Worktree | `git-graph-terminal-w2` |
| Branch | `ggt-m1-w2` @ `9d9bdd9` |
| Base | `development` @ `b651826` (W1 merged & verified) |
| Plan (read-only) | `.IDE_Plans/w2-plan_v1.0.md` (PM-owned, reference dir) |
| Worker report | `w2-report.md` (this worktree) |
| Date | 2026-10-11 |
| Mode | read-only verify — no fixes, no edits to `.IDE_Plans` / reference repo / worker files |

## VERDICT: **ACCEPT**

All acceptance criteria hold: full suite **152/152**, the borderless graph renders **real**
commits with lane glyphs + refs + the full status bar, both parity suites pass on the
approved layout fix, perf is inside the W2 budget, and every boundary is intact
(tree clean, no remotes/push, no `.IDE_Plans` or reference-repo mutations).

**Defects: 4 — all non-blocking** (1 process deviation, 1 CI-coverage gap, 2 reporting
imprecisions). See the defects table and deviation judgments below.

---

## Per-item results

### Item 1 — Commits / rules — **PASS**

1. **Exactly 4 conventional commits**, correct types and order (newest first):
   `9d9bdd9 docs(report): w2 lane report` · `70f2c9a test: end-to-end integration + perf smoke`
   · `7f8f0d3 feat(render): wire real data->layout->render path`
   · `1777100 fix(layout): O(n) row-bucketing in to_graph_layout`.
   `git rev-list --count development..HEAD` = **4**.
2. **Tree clean** — `git status --porcelain --untracked-files=all` = empty.
3. **No push / no remotes** — `git remote -v` = empty (no remote exists, so no push possible).
4. **File scope matches the plan** (`git log --name-status development..HEAD`):
   `1777100` → `src/layout/mod.rs`; `7f8f0d3` → `src/main.rs`;
   `70f2c9a` → `tests/{common/mod.rs,integration.rs,perf.rs}`, `tests/golden_layout.rs`,
   `tools/{mkbig-repo.sh,pty-shot.sh}`; `9d9bdd9` → `w2-report.md`. No stray files.
5. **No `.IDE_Plans` in the worktree** — `ls .IDE_Plans` → *No such file or directory*.
   The PM plan file in the reference/master dir has mtime **17:56** (PM write, before the
   lane's first commit at 18:43) and was not touched by the lane.
6. **Reference repo unchanged** — `vscode-git-graph` HEAD `d7f43f4` (its clone commit),
   reflog shows only `clone`, and `find -newermt '2026-10-11 18:00'` returns nothing.
   The master project dir (`git-graph-terminal`, `development` @ `b651826`) is clean and
   has no files modified after 18:00.
7. **Reflog** shows only the 4 commits plus a worktree `reset: moving to HEAD` — no amend,
   rebase, or history rewrite.

### Item 2 — FULL TEST — **PASS**

`PATH=$HOME/.cargo/bin`; run from the worktree.

1. `cargo fmt --check` → **exit 0** (clean).
2. `cargo clippy --all-targets -- -D warnings` → **exit 0** (`Finished`, no warnings).
3. `cargo test` → **152/152, 0 failed**:

   | Target | Result |
   |---|---|
   | unit (`src/main.rs`) | 49 passed |
   | `tests/golden_layout.rs` | 15 passed |
   | `tests/integration.rs` | 45 passed |
   | `tests/perf.rs` | 43 passed |
   | **total** | **152 passed, 0 failed** |

   Matches the reported split (49 + 15 + 45 + 43).

### Item 3 — T1/F3 real-path wiring — **PASS**

1. **Borderless graph on THIS repo (real commits)** — interactive pty capture
   (`MOCK=0 ROWS=40 COLS=120 tools/pty-shot.sh w2-verify-real`) shows the graph glyph
   column painting **real** commits with lane glyphs and refs, no surrounding border:
   `◯` (open HEAD) for `9d9bdd9 … HEAD -> ggt-m1-w2`, then `● ● ● ●`, the merge
   `●─╮` at `c8d7609`, the branch lane `● │ … │ ● … ├─●`, with `development`,
   `ggt-m1-w1-a1`, `ggt-m1-w1-b1`, `ggt-p0-setup`, `main` refs — exactly the W1-verified
   25-commit history (21 base + the 4 W2 commits).
2. **Status bar fields all present** — `repo: .  branch: ggt-m1-w2  head: 9d9bdd9
   commits: 25  branches: 6  lanes: 2  graph: 4 cols  panes: …` → **repo / branch / head /
   commits / branches / lanes / width** (`graph: N cols` = lane width; see item-6 code).
3. **`--text-list` fallback still works** — `git-graph-terminal --dump --text-list .`
   exit 0 and prints the P0 bordered list (`┌ commits (25) · branches (6) …`); the
   interactive `ARGS=--text-list` pty capture (`w2-verify-text`) renders the same bordered
   list with refs and status bar.
4. **`--mock` still works** — `--dump --mock` (exit 0) and the pty mock capture render the
   mock graph (`◯/●─╮/│ ●/│ ◎/●─┐/  ●`) plus refs and a `lanes: 2  graph: 4 cols`
   status bar.
5. **Note (by design):** headless `--dump` on a *real* repo prints the **text list**, not
   the graph (`src/main.rs`: `build_graph(mock, text_list || dump, …)` → graph is `None`
   when `dump`). The real borderless graph is provable only through the interactive pty
   path, which is what the report uses (report §5(a)); this matches the plan's "pty
   captures" T4 evidence and is not a defect. No run of the binary mutated the repo
   (head stayed `9d9bdd9`).

### Item 4 — T2 end-to-end parity test — **PASS**

1. **Test names verified** — `tests/integration.rs` declares exactly three explicit tests
   (`cargo test --test integration -- --list`, non-module filter):
   `data_to_layout_matches_every_golden_fixture`, `truncated_fixture_keeps_the_out_of_window_parent_sentinel`,
   `render_smoke_paints_graph_glyphs`. The other **42** of the 45 are the source modules'
   own `#[cfg(test)]` tests (pulled in via `#[path]`): `layout::tests::*`, `mock::tests::*`,
   `cell::*`, `glyphs::*`, `graph_widget::*`.
2. **Runs `data::commits` on the real fixture repos and asserts golden equality.**
   `data_to_layout_matches_every_golden_fixture` iterates `golden_paths()` (all `*.json` in
   `tests/fixtures/golden/`, asserted `== 3`), and for each calls `commits_for_golden()` →
   `data::commits(&repo)` (a real `git log`) then `assert_layout_matches(name, &graph,
   &commits, &golden)`. Live run:
   `ok data→layout parity: branch-merge-rebase-truncated (6 rows, 3 NULL parents)`,
   `branch-merge-rebase (9 rows, 0 NULL parents)`, `linear (5 rows, 0 NULL parents)`.
3. **Fixtures list** — 3 goldens; `repo.path` fields resolve to **2** real repos
   (`tests/fixtures/repos/linear`, `tests/fixtures/repos/branch-merge-rebase`); the
   `-truncated` golden reviews the same `branch-merge-rebase` repo through a smaller
   commit window. So the test runs `data::commits` for **each of the 3 goldens** (over 2
   distinct repos) — the report's "3 golden fixtures" is what is pinned.
4. **EXACT equality** — `assert_layout_matches` asserts exact `assert_eq!` on rows count,
   per-row `lane`/`colour`/`is_head`/`on_branch`/`row`, the full `Vec<Segment>`
   (row_from/row_to/col_from/col_to/kind/colour_idx/pers), `parent_rows` per row, `max_lane`,
   and `palette`; `commits_for_golden` additionally asserts the hash order
   (`assert_eq!(got, want)`). Expected segments are re-derived only from the golden's own
   geometry (`expected_segments` / `expected_kind`), so the test cannot re-assert the port's
   output. Any divergence in `data::commits` (order/content/refs) or `layout()` fails it.
5. **Failed-if-wrong reasoning (no files modified):** a reordered hash list fails
   `commits_for_golden`; a wrong lane/colour/head flag fails the per-row asserts; a moved
   segment coordinate fails the `Vec<Segment>` equality; a changed `max_lane`/palette fails
   its assert; a missing `parents:[-1]` sentinel fails
   `truncated_fixture_keeps_the_out_of_window_parent_sentinel` (`null_parents >= 1`). The
   render smoke separately asserts the real layout paints `●`, `◯` and a box-drawing
   connector, and that the HEAD vertex's own grid cell (`2*lane`) holds `◯`.

### Item 5 — T3 perf — **PASS**

1. **Reproducible numbers** (`cargo test --test perf -- --nocapture`):

   | Profile | git log | layout | frame (data+layout+draw) | peak RSS |
   |---|---:|---:|---:|---:|
   | debug (mine) | 239 ms | 60 ms | 299 ms | 38.8 MB |
   | release (mine) | 207 ms | 37 ms | 244 ms | 38.4 MB |
   | report §4 debug | — | 52 ms | 292 ms | ~38 MB |
   | report §4 release | — | 29 ms | 225 ms | ~38 MB |

   All within reason (same order of magnitude; small box-to-box variance).
2. **Budgets** — release `layout()` **37 ms ≤ 1.0 s** and first frame **244 ms ≤ 2.0 s**
   (debug trip-wires 2.0 s / 3.0 s), enforced by asserts in `perf.rs`
   (`layout_ms <= LAYOUT_BUDGET_MS`, `frame_ms <= FRAME_BUDGET_MS`).
3. **`tools/mkbig-repo.sh` regenerates** — `bash tools/mkbig-repo.sh 500 /tmp/ggt-verify-big`
   → `generated … (510 commits, 6 branches)` (500 + 2×chunks; 5 side branches + main), and
   the binary read it (`commits: 510  branches: 6`).
4. **In the DEFAULT suite** — plain `cargo test` builds and runs `tests/perf.rs`
   (`Running tests/perf.rs … 43 passed`) with a **profile-aware ceiling**
   (`LAYOUT_BUDGET_MS = if cfg!(debug_assertions) { 2_000 } else { 1_000 }`,
   `FRAME_BUDGET_MS = if cfg!(debug_assertions) { 3_000 } else { 2_000 }`) — "run always,
   profile-aware ceiling" as ruled. (Caveat D2 below.)

### Item 6 — Approved layout fix (`1777100`) — **PASS**

1. **Behavior-preserving by construction** — the diff (`git show 1777100`) only adds a
   one-pass bucketing `lines_by_row: Vec<Vec<(Line, usize)>>` keyed on `line.p1.y`, then
   replaces `for branch { for line { if line.p1.y != row { continue } … } }` with
   `for (line, colour) in &lines_by_row[row] { … }`. **No** condition, branch, coordinate,
   `segment_kind`, `pers`, or `max_lane` change; the out-of-range skip is preserved via
   `lines_by_row.get_mut(line.p1.y)`. Iteration order is identical (branch-major,
   line-minor), so each row's `Vec<Segment>` is byte-for-byte the same.
2. **Both parity suites pass on it** — golden **15/15** and integration **45/45** green at
   HEAD, and `src/layout/mod.rs` is **unchanged** between `1777100` and HEAD
   (`git diff --stat 1777100 HEAD -- src/layout/mod.rs` = empty), so the green suites
   validate the fix exactly.
3. **Perf justification reproduced** — release `layout()` 37 ms (vs the pre-fix 473 ms in
   the commit message; both ≫ the 30k threshold) confirms the O(n²)→O(n) effect.

### Item 7 — Deviations judged — **PASS (legitimate, documented)**

See the dedicated section below.

---

## Defects / issues table

| # | Severity | Item | Finding | Blocking? |
|---|---|---|---|---|
| D1 | Medium (process) | Item 6 / report §6.4 | Layout fix landed with a failing **perf** test, not the plan-mandated "failing **golden** test first". Change is behavior-preserving (proven) and developer-approved; literal precondition not met. | No |
| D2 | Medium (CI coverage) | Item 5 / report §6.3 | The plan budget (1.0 s / 2.0 s) is enforced only on the **release** profile; default `cargo test` (and `ci.yml`, which runs debug `cargo test`) uses looser ceilings (2.0 s / 3.0 s). A regression up to 2 s would not fail CI. | No |
| D3 | Low (verification gap) | Item 3 | Headless `--dump` on a real repo prints the text list, so the real borderless graph cannot be asserted headlessly; only the interactive pty capture proves it (as the plan intends). | No |
| D4 | Low (reporting) | Item 4 / report §3 | §3's "integration 45 … data::commits → layout()" reads as if the whole target pins the path; in fact only **3** of 45 do (42 are included module tests). Also "3 G8 fixture **repos**" is 3 goldens over 2 real repos. Numbers themselves are correct. | No |

No correctness, boundary, or acceptance defect found. Tree clean; no reference-repo /
`.IDE_Plans` mutation; no push.

---

## Deviation judgments (report §6)

1. **`tests/perf.rs` added (not named in T3)** → **legitimate.** The plan asked to
   "measure & report … report numbers regardless"; a committed, assertion-bearing test is
   the natural way to make the budget a regression guard, and `tools/mkbig-repo.sh` was
   also delivered. Benign additive scope, documented.
2. **`tools/pty-shot.sh` gained an `ARGS` env var** → **legitimate.** Required to capture
   `--text-list`; small, additive, backward-compatible (default empty), and the `.frame`
   step is skipped only for interactive modes. Documented.
3. **Profile-aware perf ceilings** → **legitimate but weakens CI** (see D2). Sensible to
   keep the default debug suite green, but this means the plan's 1.0 s/2.0 s budget is not
   enforced by the current CI job (which runs debug `cargo test`). Recorded, not blocking.
4. **Layout change without a failing *golden* test** → **legitimate deviation, process
   gate not literally met** (see D1). The plan's literal precondition names a failing
   golden test; the lane hit a failing **perf** test instead, recorded the failing test +
   before/after numbers (§4), proved behavior preservation (both parity suites green), and
   obtained explicit developer approval. The intent (regression-proof-by-a-failing-test +
   explicit report section) is satisfied; the letter of "golden first" is not.
5. **First frame measured as a `TestBackend` proxy, not a pty timestamp** → **legitimate.**
   The plan "suggested" a pty timestamp; the proxy is deterministic, and the real pty
   captures (item 3) confirm the actual first frame renders. Documented.

All five stay inside the lane boundary — none touches the reference repo, the goldens, or
`.IDE_Plans`.

---

## How this was verified (commands, read-only)

- `git log/status/remote/reflog/rev-list`; `git show 1777100`; `git diff 1777100 HEAD`.
- `cargo fmt --check`; `cargo clippy --all-targets -- -D warnings`; `cargo test`.
- `cargo test --test integration -- --list`; `cargo test --test integration data_to_layout -- --nocapture`.
- `cargo test [--release] --test perf -- --nocapture`.
- `tools/pty-shot.sh` (real `MOCK=0`, `ARGS=--text-list`, `MOCK=1`) → `target/pty/w2-verify-*`.
- `./target/debug/git-graph-terminal --dump [--mock|--text-list] .`; `tools/mkbig-repo.sh 500 /tmp/ggt-verify-big`.
- mtime/`find -newermt` on the worktree, the master project dir, and `vscode-git-graph`.

**Final: ACCEPT — 152/152 green, borders intact, 4 non-blocking observations.**



