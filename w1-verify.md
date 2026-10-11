# W1 verification — initiative git-graph-terminal (wave W1: lane a1 layout core ∥ lane b1 renderer, merged)

Verifier: **cline-w1** (independent) · PM: Hermes · Base: `development` @ merge `c8d7609`
Worktree: `git-graph-terminal` · Plan (read-only): `.IDE_Plans/w1-plan_v1.0.md`
Method: measure everything, trust nothing — every claim below is backed by command output.
Mode: verification only. Nothing was fixed, no `.IDE_Plans` file was touched, the reference repo was not touched.

## Verdict

**ACCEPT** — all 7 verification items PASS. 3 findings, all non-blocking (2 LOW, 1 INFO; 0 HIGH/CRITICAL).

---

## Item 1 — Contract integrity (merge resolution: layout/* = a1, render/* + main.rs = b1) — **PASS**

Merge topology:

```
$ git log -1 --pretty='%H %P' c8d7609
c8d7609625b0a7e7f144966c5731990a71bb4792 2d24fbe1a7d6973bf6602fc913546691189df0b1 23c54d6e12941603ae262bad4f4e9b599962c22e
#                                                             ^ a1 lane tip   ^ b1 lane tip
```

Layout files are byte-identical to a1 and differ from b1's stand-in (proves "ours" = a1):

```
$ git diff --stat c8d7609 2d24fbe -- src/layout        # merge vs a1
(empty)
$ git diff --stat c8d7609 23c54d6 -- src/layout        # merge vs b1 stand-in
 src/layout/mod.rs   | 837 +----------------------------------
 src/layout/types.rs | 110 +++----
 2 files changed, 72 insertions(+), 875 deletions(-)

$ git rev-parse c8d7609:src/layout/types.rs 2d24fbe:src/layout/types.rs 23c54d6:src/layout/types.rs
081759f666fde2c683bf46decbcdd3f638b07d93   <- merge
081759f666fde2c683bf46decbcdd3f638b07d93   <- a1   (IDENTICAL)
d31020e45c463bdcaca180697f061137cbf8a576   <- b1 stand-in

$ git rev-parse c8d7609:src/layout/mod.rs 2d24fbe:src/layout/mod.rs 23c54d6:src/layout/mod.rs
ed4c65dd8c1276a8b4135bd180be09a0aebc5b13   <- merge
ed4c65dd8c1276a8b4135bd180be09a0aebc5b13   <- a1   (IDENTICAL)
e7a98d8442a3f9c31852ac5599a148396744fed1   <- b1
```

Render/main came from b1 (identical blobs to b1's tip):

```
$ git rev-parse c8d7609:src/render/mod.rs 23c54d6:src/render/mod.rs
4b5780d9be47931288373a175890e635727d105b
4b5780d9be47931288373a175890e635727d105b
$ git rev-parse c8d7609:src/main.rs 23c54d6:src/main.rs
76345615229b82a2690c5336d218a5dc0dda197c
76345615229b82a2690c5336d218a5dc0dda197c
```

No conflict markers anywhere:

```
$ grep -rn '<<<<<<<\|>>>>>>>\|^=======$' src tests
(no output — exit 1)
$ grep -rn '<<<<<<<\|>>>>>>>\|^=======$' . --exclude-dir=.git --exclude-dir=node_modules --exclude-dir=target
(no output)
```

The merge stat shows the merge only introduced b1-owned files (`src/main.rs`, `src/render/*`, `tools/pty-shot.sh`, `b1-report.md`); `src/layout/*` stayed a1's already-merged version.

**Result: PASS.** Merge `src/layout/*` blob IDs == a1 branch; `src/render/*` + `src/main.rs` blob IDs == b1 branch; zero conflict markers.

---

## Item 2 — FULL TEST — **PASS**

```
$ cargo --version && rustc --version && node --version && npm --version
cargo 1.99.0 (5f94df478 2026-08-27)
rustc 1.99.0 (b940084d7 2026-09-28)
v22.22.1
11.19.1

$ cargo fmt --check          -> FMT_OK (no diff)
$ cargo clippy --all-targets -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.09s    (no warnings)

$ cargo test
test result: ok. 49 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out   <- unit binary
     Running tests/golden_layout.rs
test result: ok. 15 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out   <- golden integration binary
$ cargo test -- --list | grep -c ': test$'
64
```

**Result: PASS.** Exactly as the task predicted: 64 total = 49 unit + 15 golden, 0 failed, fmt+clippy clean.

---

## Item 3 — G8 gate (exact equality + harness reproduction) — **PASS**

The test asserts **exact** equality (no tolerances), re-deriving expected values from the golden's own geometry:

- per row: `lane`, `colour`, `isCurrent→is_head`, `onBranch→on_branch`, `RowLayout.row` (`assert_eq!`);
- per row: `parents` incl. `-1 == NULL_VERTEX_ID → None` (`assert_eq!(parents, expected_parents)`);
- per row: **every segment** via `assert_eq!(layout_row.segments, expected)` with `col_from`, `col_to`, `kind`, `colour_idx` and the recomputed `pers` winner:

```rust
let pers = if line.locked_first { line.x1 } else { line.x2 };   // pers winner from golden only
kind: expected_kind(line.x1, line.x2)                            // geometry-only kind
```

- `max_lane` (vertices + both segment ends) and the 12-entry `palette` element-by-element.

Truncated-window fixture included and exercised:

```
$ ls tests/fixtures/golden/
branch-merge-rebase-truncated.json   branch-merge-rebase.json   linear.json
# truncated fixture rows: parents [1], [-1], [3,4], [-1], [5], [-1]  -> 3x NULL_VERTEX_ID
# test requires: paths.len() >= 3  AND  null_vertex_parents >= 1 (fails loudly otherwise)
```

Harness one-liner from `tools/g8-dump/README.md`, re-run from a clean shell:

```
$ cd tools/g8-dump && npm ci && npm run fixtures && npm run dump && npm run check
npmci_exit=0        (found 0 vulnerabilities)
fixtures_exit=0     (branch-merge-rebase, linear)
dump_exit=0         wrote ... branch-merge-rebase.json (9 rows, 3 branches)
                    wrote ... branch-merge-rebase-truncated.json (6 rows, 3 branches)
                    wrote ... linear.json (5 rows, 1 branches)
check_exit=0
ok   branch-merge-rebase: matches tests/fixtures/golden/branch-merge-rebase.json
ok   branch-merge-rebase-truncated: matches tests/fixtures/golden/branch-merge-rebase-truncated.json
ok   linear: matches tests/fixtures/golden/linear.json

$ git status --porcelain -- tests/fixtures ; git status --porcelain
(0 lines — goldens reproduced byte-identical; tree clean)
```

Post-run golden digests (byte-identical to the committed blobs, proven by the empty diff):

```
2a96d1a8a8da76ebdafae1c721a81cb382cc6336a0930629410c98cdf7ad45de  branch-merge-rebase.json
ff0774c5e451aa906d709be0312dd27c684424eef9732e8421d077dc5ca735b9  branch-merge-rebase-truncated.json
e518a5c06862293c45e858d5f5b51167a750096ed2d452811ffc30f41c626722  linear.json
```

**Result: PASS.** Exact-equality assertions for rows/lanes/colours/segments/pers; goldens reproduce byte-identical; tree stays clean.


---

## Item 4 — D4 (unreachability proof documented + reachable `parents:[-1]` golden-covered) — **PASS (with a scope note)**

Written proof lives where a reviewer will look — `src/layout/mod.rs:21-43`:

> `# Reachability of the lane: -1 / on_branch: false sentinel` … "That state is **unreachable** for the commit list the golden harness feeds in" … both `determinePath()` exits leave the start vertex on a branch (`graph.ts:733` unconditional `addToBranch`; `graph.ts:709` guarded by `!vertex.isNotOnBranch()`), therefore `on_branch` is always `true`. The reachable sentinel is the **parent link** `parents: [-1]` (`NULL_VERTEX_ID`).

Mirrored in `tools/g8-dump/README.md` (schema note + "unreachable, so no fixture can exercise it", 18-case probe described) and enforced by the test's two canaries:

- positive: `null_vertex_parents >= 1` and `paths.len() >= 3`;
- negative (D4 canary): every golden row must have `lane >= 0`, `colour >= 0`, `onBranch == true` — if a future fixture ever carries `lane:-1` the test fails loudly instead of the port silently drifting.

**Result: PASS.** Documented + covered + canaried. Note: plan item T4 asked literally for a `lane:-1/on_branch:false` fixture; a1 substituted the reachable `parents:[-1]` sentinel with an executed proof and escalated it to the PM (Finding F2). Plan acceptance #4 ("D4 sentinel covered") is satisfied for the reachable sentinel.

---

## Item 5 — Renderer quality (PTY capture / visible output) — **PASS (scope note: no real-repo wiring yet)**

```
$ bash tools/pty-shot.sh mock-graph-verify
== pty capture: 80x24 (name=mock-graph-verify, mock=1) ==
raw/shot/strings/frame -> target/pty/mock-graph-verify.*
```

`--dump --mock` frame (deterministic, headless) — graph glyphs, refs decoration, status bar all visible:

```
      idx hash     date       subject                                            refs
┄       1 -------  2023-11-14 Uncommitted changes
◯       2 a1b2c3d  2023-11-14 Add graph renderer widget                        HEAD -> main
●─╮     3 b2c3d4e  2023-11-14 Merge branch 'feature/graph'
│ ●     4 c3d4e5f  2023-11-14 Draw rounded corners
│ ◎     5 d4e5f6a  2023-11-14 WIP stash demo                                   stash@{0}
●       6 e5f6a7b  2023-11-14 Base commit                                    tag: v0.1
●─┐     7 f6a7b8c  2023-11-14 Second root
  ●     8 a7b8c9d  2023-11-14 Orphan root

 repo: (mock)  branch: main  head: a1b2c3d  commits: 8  branches: 2  lanes: 2  graph: 4 cols  ...
```

The ANSI-stripped PTY shot (`.txt`) shows the same content from a real 80×24 PTY, exit code 0, no panic; alternate-screen setup/teardown and palette escapes are present in `.strings.txt` (`[?1049h`, `[38;2;0;133;217;49m`).

Real repo (`cargo run -q -- --dump .`) renders **real commits + refs + status bar** on the text path:

```
┌ commits (20) · branches (5) — . ────────────────────────────────────────────┐
│>     1 c8d7609 merge: lane b1 (renderer) into development; ...             │
│      2 2d24fbe docs(report): M1-W1 a1 lane report  (ggt-m1-w1-a1)           │
│      ...
 repo: .  branch: development  head: c8d7609  commits: 20  branches: 5  panes: j/k/↑/↓ scroll · r re
```

**Explicit scope statement:** the borderless graph widget is **NOT yet wired into the real-repo path**. `src/main.rs:197`:

```rust
let graph: Option<GraphLayout> = mock.then(render::mock::demo_layout);
```

So `graph` is `Some(...)` only with `--mock`; a real repo renders `ViewOptions::TEXT_ONLY` (the P0 bordered text list, no graph cells). This matches b1's report (§8 Q2) and the plan ("the real repo has no graph cells yet — that is a1's `layout()`"), i.e. **expected scope for W2 integration, not a W1 defect**. Stage 6 #3 ("mock **or** real repo renders graph + text + status bar") is satisfied via `--mock`.

**Result: PASS.**


---

## Item 6 — Lane rules (cross-lane edits + report claims) — **PASS (with one LOW finding)**

b1's full diff against its base `226516c`:

```
$ git diff --stat 226516c 23c54d6
 b1-report.md               | 239 +++
 src/layout/mod.rs          |   5 +      <- cross-lane edit (disclosed)
 src/layout/types.rs        |  97 +      <- plan's documented stand-in fallback
 src/main.rs                |  85 +-     <- b1's own
 src/render/cell.rs         | 398 +
 src/render/columns.rs      | 164 +
 src/render/glyphs.rs       | 323 +
 src/render/graph_widget.rs | 229 +
 src/render/mock.rs         | 216 +
 src/render/mod.rs          | 238 +-     <- b1's own
 tools/pty-shot.sh          |  67 +
```

The only cross-lane edit is a single hunk in a1's `src/layout/mod.rs` (one `pub mod types;` declaration plus a 3-line comment, `+5/-0`):

```
$ git diff 226516c 23c54d6 -- src/layout/mod.rs
+// W1 shared contract types (`w1-plan_v1.0.md` §THE CONTRACT). Declared here so
+// lane b1 (renderer) can import `crate::layout::types::*`. a1 owns the rest of
+// this module and lands the identical `types.rs` in the same wave.
+pub mod types;
```

a1 touched nothing of b1's (a1 diff = `src/layout/*`, `tests/golden_layout.rs`, the new golden fixture, `tools/g8-dump/*`, `a1-report.md`). The merge kept a1's `mod.rs`, which already declares the same `pub mod types;` — no duplicate and no conflict.

Report claim spot-checks (all confirmed):

1. a1 — reference `web/graph.ts` sha256 `86a66715…`: `sha256sum ../vscode-git-graph/web/graph.ts` → `86a667151fcef285213e3059bcf460df5cfb8b77111f0f9c414f57c25850e4e9` **matches**.
2. b1 — "no new dependency; Cargo.toml still ratatui 0.30.2 + crossterm 0.29.0": `Cargo.toml` shows exactly those two **matches**.
3. b1 — reference checkout `d7f43f4…`, `git status --porcelain` empty: rev-parse → `d7f43f429a9e024e896bac9fc65fdc530935c812`, status 0 lines **matches**.
4. a1 — "3/3 golden ok, pre-existing goldens byte-identical after the harness change": re-run reproduced 3/3 ok with an empty `git diff` **matches**.
5. b1 — line counts (`cell.rs` 398, `glyphs.rs` 323, `graph_widget.rs` 229, `columns.rs` 164, `mock.rs` 216, `render/mod.rs` 252): `wc -l` **matches** exactly.

Note: b1's report states the deviation as "one line" (§0 row 7) and "one declaration … one hunk" (§7 deviation 1) — the actual hunk is 5 lines (declaration + comment). The disclosure is accurate about *scope*; only the literal line count differs. Covered as Finding F1, LOW.

**Result: PASS.**

---

## Item 7 — Boundaries — **PASS**

```
$ ls -la --time-style=full-iso .IDE_Plans/    (newest file)
w1-plan_v1.0.md   2026-10-11 16:03:02        <- before any W1 lane commit
$ git log --format='%ci' -10                   lane commits: 17:28:59 (b1), 17:38:55 (a1), merge 17:42:02
$ git log --all --oneline -- .IDE_Plans | wc -l
0                                              <- .IDE_Plans not tracked in any commit

$ git -C ../vscode-git-graph status --porcelain | wc -l
0
$ git -C ../vscode-git-graph log -1 --format='%H %s'
d7f43f429a9e024e896bac9fc65fdc530935c812 #557 Improved context menu click event handling.
$ git -C ../vscode-git-graph reflog --date=iso | head -5
d7f43f4 HEAD@{2026-10-11 12:27:55 +0530}: clone: from https://github.com/mhutchie/vscode-git-graph.git
                                               <- only the original clone; no checkout/commit/reset

$ find . -name '*.orig' -o -name '*.rej' -o -name '*.bak'   (excl. node_modules)
(no output)                                    <- no conflict/reject debris

main worktree:      git status --porcelain            -> 0 lines (clean, no untracked)
a1 worktree:        ?? tools/g8-dump/node_modules     <- disclosed symlink to the base install
b1 worktree:        (clean)
```

Unmerged leftovers: both lane branches are merge-reachable from `development` (`ggt-m1-w1-a1` = 1st parent, `ggt-m1-w1-b1` = 2nd parent); `ggt-p0-setup`/`main` remain — acceptable.

**Result: PASS.** `.IDE_Plans` untouched (mtime 16:03 < 17:28), reference repo untouched (0 changes, reflog = clone only), no debris, worktrees clean.



---

## Defects / findings

| # | Severity | Finding | Evidence | Impact |
| - | -------- | ------- | -------- | ------ |
| F1 | LOW | b1 made a cross-lane edit to a1-owned `src/layout/mod.rs`: one hunk, `+5/-0` (`pub mod types;` + 3-line comment). The plan allowed b1 to create only `types.rs` if absent; the `mod.rs` declaration was not explicitly allowed. Disclosed in `b1-report.md` (§0 row 7, §7 deviation 1) and required for b1 to compile. | `git diff --stat 226516c 23c54d6` → `src/layout/mod.rs \| 5 +` | None at merge (a1's `mod.rs`, which also declares `pub mod types;`, won). Report's "one line" wording understates it as 5 lines. |
| F2 | LOW | Plan T4 asked literally for a `lane:-1 / on_branch:false` fixture; a1 delivered the reachable `parents:[-1]` sentinel instead (plus a written unreachability proof and an executed 18-case probe). Escalated to PM by a1 (§9 note 1); the plan file was never edited. | `src/layout/mod.rs:21-43`, `tools/g8-dump/README.md`, golden `branch-merge-rebase-truncated.json` | Acceptance #4 met for the reachable sentinel; the literal plan wording still needs a one-line PM ratification (re-scope T4). |
| F3 | INFO | The borderless graph widget is not wired into the real-repo path: `src/main.rs:197` builds `GraphLayout` only under `--mock`, so a real repo shows the P0 text view with empty graph cells. Matches the plan (Stage 6 #3) and b1 (§8 Q2). | `--dump .` → bordered text list, no graph; `--dump --mock` → graph | Expected W2 integration work, not a W1 defect. |

**Defect count: 3 (0 blocker / 0 high / 2 LOW / 1 INFO).**

## Verdict

**ACCEPT** — W1 (a1 layout core + b1 renderer) is contract-correct, fully green (64 tests), golden-exact (3 fixtures, byte-identical re-run), D4-documented-and-covered, renders coherently, and respects the boundaries. F1/F2/F3 are non-blocking and require only (a) acknowledging b1's disclosed one-hunk cross-lane edit, (b) a PM re-scope of T4 to "cover the reachable `NULL_VERTEX_ID` sentinel", (c) scheduling the W2 wiring of `layout()` into `main.rs`.

## Reproduce (verbatim)

```bash
cd /mnt/Just_Do_It/02_Devp_Soft/01_Adi-Mudi/11_Git_Graph/git-graph-terminal
export PATH="$HOME/.cargo/bin:$PATH"
# Item 1
git diff --stat c8d7609 2d24fbe -- src/layout && git rev-parse c8d7609:src/layout/types.rs 2d24fbe:src/layout/types.rs
grep -rn '<<<<<<<' src tests
# Item 2
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test
# Item 3
(cd tools/g8-dump && npm ci && npm run fixtures && npm run dump && npm run check) && git status --porcelain
# Item 5
tools/pty-shot.sh mock-graph-verify && cat target/pty/mock-graph-verify.frame && cargo run -q -- --dump .
# Item 7
ls -la --time-style=full-iso .IDE_Plans/ && git -C ../vscode-git-graph status --porcelain
```
