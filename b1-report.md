# B1 report — W1 renderer

Plan: `.IDE_Plans/w1-plan_v1.0.md` (lane `b1`, branch `ggt-m1-w1-b1`)
Worktree: `git-graph-terminal-b1` · Base: `development` @ `226516c` (P0 merged)
Worker: pi · Verifier: cline after both lanes merge (per master plan pipeline)

## 0. Status against the plan's acceptance list

| # | Acceptance item (w1-plan §Lane b1 gate + Stage 6) | Status | Evidence |
| - | ------------------------------------------------- | ------ | -------- |
| 1 | `cargo test` green (widget unit tests) | DONE | §5 (42/42; 35 new, 7 P0 data tests untouched) |
| 2 | `cargo fmt --check` + `cargo clippy -- -D warnings` clean | DONE | §5 |
| 3 | PTY capture shows a coherent mock graph | DONE | §4 (`tools/pty-shot.sh`, 4 artifacts, no panic) |
| 4 | `b1-report.md` | DONE | this file |
| 5 | Stage 6 #3 "TTY demo: mock **or** real repo renders graph + text + status bar" | DONE via `--mock` | §4; the real repo has no graph cells yet — that is a1's `layout()` (see §8 Q2) |
| 6 | b1 tasks 1–3 file coverage (`cell.rs`, `glyphs.rs`, `graph_widget.rs`, `columns.rs`, `main.rs` wiring, mock fixtures) | DONE | §2 |
| 7 | Never touch the other lane's files | DONE with one disclosed exception | §7 deviation 1 (`src/layout/mod.rs`, one line) |
| 8 | No pushes, no `.IDE_Plans/` edits, no reference-repo mutation | DONE | §7 items 9–10 |

## 1. Environment (exact)

| tool | version |
| ---- | ------- |
| rustc / cargo | 1.99.0 (b940084d7 2026-09-28) / cargo 1.99.0 (5f94df478 2026-08-27) |
| git | 2.53.0 |
| util-linux `script` | 2.41.3 |
| `stty` | uutils coreutils 0.8.0 |
| reference checkout | `../vscode-git-graph` @ `d7f43f429a9e024e896bac9fc65fdc530935c812` — **read only**, `git status --porcelain` still empty |
| reference files read | `src/config.ts:270-285` (the 12-colour palette default + `GraphStyle.Rounded` default) |

No new dependency was added: `Cargo.toml` still declares only `ratatui 0.30.2` + `crossterm 0.29.0`.

## 2. What lane b1 added

| File | Lines | Role |
| ---- | ----- | ---- |
| `src/render/cell.rs` | 398 | Cell grid builder: `Segment`s → per-row cells with connection bits, glyph choice and the `pers` z-order. Public: `CELLS_PER_LANE`, `Cell{glyph,lane,pers}`, `BuiltRow{cells,colours}`, `cell_of(lane)`, `graph_cells(max_lane)`, `build_row(row,max_lane,tier,uncommitted)`, `vertex_glyph(vertex,glyphs,uncommitted)` |
| `src/render/glyphs.rs` | 323 | Three glyph tables (`UNICODE`/`ASCII`/`DUMB`) + `GlyphTier::{detect,detect_from_os}` from `TERM`/`NO_COLOR`/locale, and `colour_enabled{,_os}` (G2 minimal ladder; M3 does full detection) |
| `src/render/graph_widget.rs` | 229 | `GraphWidget` — the ratatui `Widget`: 2 cells per lane, rounded corners, HEAD/stash/uncommitted glyphs, selection, scroll, palette colours |
| `src/render/columns.rs` | 164 | Text columns `idx, hash, date, subject, refs` as a `Table`, starting at `text_offset(max_lane) = 2*max_lane+2` |
| `src/render/mock.rs` | 216 | Hand-built `demo_layout()` / `demo_commits()` / `frozen_palette()` fixture (§6) |
| `src/render/mod.rs` | 252 | `ViewOptions{graph,tier,colour,uncommitted}` seam, `ui()`, `graph_view()`, `commit_list()` (P0, unchanged), `status_bar()` (+ lane width) |
| `src/main.rs` | +~80 | `--mock` + `--dump --mock` wiring, tier/colour detection, graph frame dumped at 140 cols vs P0's 100; key handling unchanged |
| `src/layout/types.rs` | 97 | a1's contract file, stood in because it had not landed in this worktree (§7 deviation 2) |
| `tools/pty-shot.sh` | 67 | The repeatable PTY screenshot command (§4) |

Nothing was added under `tests/`: b1's tests are unit tests next to the code, per the plan.

## 3. The D8 rules, as implemented

1. **Lane pitch — 2 cells per lane (D8 §3.1).** `cell_of(lane) = 2*lane`; the graph area is
   `graph_cells(max_lane) = 2*(max_lane+1)` wide; the text columns start at
   `2*max_lane+2` (same `+2` pad as the reference). Asserted by `cell::tests::pitch_and_width_follow_d8`,
   `columns::tests::text_offset_matches_the_contract`, `tests::graph_width_uses_the_contract_pitch`.
2. **Crossing cell — `Cell{glyph,lane,pers}`, one lane wins the colour (D8 §3.2).** Each cell
   accumulates connection bits (`UP|DOWN|LEFT|RIGHT`) and every contributing segment votes with
   its `pers`; the highest `pers` owns `lane` and the colour. Glyph by tier: `┼` (Unicode),
   `+` (ASCII), one-lane-wins i.e. `|` for the vertical (Dumb). Asserted by
   `crossing_uses_the_cross_glyph_per_tier`, `higher_pers_wins_the_crossing_colour`.
   Deliberate, documented deviation: a terminal cell cannot show two colours.
3. **Glyph from capability tier (G2).** `TERM=dumb` → `DUMB`; no `TERM` → `DUMB`; `NO_COLOR` →
   `ASCII` + colour off; non-UTF-8 locale → `ASCII`; UTF-8 locale or a modern `COLORTERM` hint →
   `UNICODE`. Unknown `TERM` falls back to `ASCII` (never to Unicode: a wrong guess renders tofu).
4. **Vertex glyphs.** HEAD `◯`, stash `◎`, ordinary `●`, uncommitted `┄` (grey, `DarkGray`), and
   the dashed `┆` for a vertical passing through the uncommitted row.
5. **Rounded by default.** `RoundedCorner`/`MergeArc` → curved glyphs (`╰╯╭╮`); `SegKind::Diagonal`
   → sharp glyphs (`└┘┌┐`), reserved for M3's angular style (G9).
6. **Palette.** `mock::frozen_palette()` is the reference's 12-entry array verbatim
   (`config.ts:275`), indexed by `colour_idx % 12`; asserted by `mock::tests::palette_is_the_frozen_twelve`
   and `graph_widget::tests::colour_follows_the_frozen_palette`.

### 3.1 One rule that needs a1's written agreement at merge

a1's canonical `types.rs` doc says a jump **wider than one lane** is `SegKind::Horizontal`.
Drawing that as a bare horizontal bar would leave the line disconnected from the next row
(`Horizontal` is documented in my stand-in as "no vertical anchors"). b1 therefore draws
**every** lane change as a connected step — corner at `col_from` (`UP`+toward), horizontal run,
corner at `col_to` (`DOWN`+back) — whatever the `SegKind` is, and however far the lane moves.
Asserted by `a_wide_lane_jump_stays_connected` and `adjacent_lane_transition_is_a_rounded_step`.
If a1 really means "a row with no vertical anchors", b1 needs one sentence from a1 confirming
that `Horizontal` never appears for a wider jump; otherwise no change is needed.

## 4. PTY smoke (b1 task 3)

`tools/pty-shot.sh` is the documented, repeatable command (executable, no dependencies beyond
`script`/`stty`/`sed`/`strings`). It runs the binary under a real PTY at 80×24, quits after
`WAIT` seconds (default 1) and writes four artifacts to `target/pty/` (git-ignored):

| Artifact | What it is | Why it exists |
| -------- | ---------- | ------------- |
| `<name>.raw` | the raw PTY byte stream (alternate-screen escapes) | ground truth / re-processing |
| `<name>.txt` | ANSI-stripped, one line per cursor move | the **reviewable screenshot** (final frame last) |
| `<name>.strings.txt` | `strings` view (the plan's literal `stty+timeout+strings` step) | fidelity to the plan |
| `<name>.frame` | deterministic `--dump` frame, no PTY | CI-diffable, byte-stable |

```
$ ./tools/pty-shot.sh                     # mock fixture, 80x24, 1s   -> target/pty/mock-graph.*
$ COLS=140 ROWS=24 WAIT=2 ./tools/pty-shot.sh wide   # wider pty: full status bar visible
$ MOCK=0 ./tools/pty-shot.sh real         # the real repo (text-only view until a1 lands)
```

Evidence from the captured run (mock fixture, `script` exit code 0, `grep -c 'panic|thread '` -> 0
in both `.raw` and `.strings.txt`). This is `target/pty/mock-graph.frame` verbatim:

```
      idx hash     date       subject                                                                                   refs
┄       1 -------  2023-11-14 Uncommitted changes
◯       2 a1b2c3d  2023-11-14 Add graph renderer widget                                                                 HEAD -> main
●─╮     3 b2c3d4e  2023-11-14 Merge branch 'feature/graph'
│ ●     4 c3d4e5f  2023-11-14 Draw rounded corners
│ ◎     5 d4e5f6a  2023-11-14 WIP stash demo                                                                            stash@{0}
●       6 e5f6a7b  2023-11-14 Base commit                                                                               tag: v0.1
●─┐     7 f6a7b8c  2023-11-14 Second root
  ●     8 a7b8c9d  2023-11-14 Orphan root


 repo: (mock)  branch: main  head: a1b2c3d  commits: 8  branches: 2  lanes: 2  graph: 4 cols  panes: j/k/↑/↓ scroll · r reload · q quit
```

Frame geometry: the graph view is dumped at 140 columns so the whole status bar fits on one line;
the text view stays at P0's 100 columns, so `target/pty/real.frame` is still P0's bordered frame.
In the pty `.txt` shot a long status bar can arrive split over two lines, one per cursor move — that
is the `.txt` transform's contract, not a rendering bug (the wide shot has the tail on line 55).

The mock covers, in one frame: HEAD open circle on a merge commit, a rounded fork `●─╮` into a
second lane, the merged lane's `│` continuation, a stash vertex with its `stash@{0}` ref, a
`tag:`, a second root joining from the right (`●─┐`), and an orphan root. Vertical connectivity
between every row is asserted by `graph_widget::tests::paints_the_mock_graph_vertically_connected`.

## 5. Test results (test gate)

```
$ cargo fmt --check                                  -> OK (no diff)
$ cargo clippy --all-targets -- -D warnings           -> OK, exit 0
$ cargo test                                          -> 42 passed; 0 failed; 0 ignored
  render::cell            12   render::glyphs          7   render::graph_widget  5
  render::mock             4   render::columns         4   render::tests         3   (35 new)
  data::tests              7   (P0, untouched)
```

Integration-level checks beyond the unit tests:

1. `cargo run -- --dump --mock` — one 140×12 frame on `TestBackend` (written to `.frame`).
2. `./tools/pty-shot.sh` — real PTY run, alternate-screen setup/teardown, exit code 0, no panic.
3. `cargo run -- --dump .` — the real repository still renders the P0 text view (bordered block,
   P0 row format, 100 columns) — i.e. no P0 regression on the fallback path.
4. **Merge-compatibility check with a1's canonical `types.rs`** (temporary file swap, then restore):
   all 42 tests pass against a1's file as well, and the stand-in was restored byte-identically
   (sha256 `0458724f…` before and after). This is the evidence that the W2 merge of `types.rs`
   cannot break b1.

## 6. The mock fixture

`render::mock` is hand-built from the contract types only (no a1 code): 8 rows over 2 lanes, the
frozen palette, all four special vertices, a fork and a re-join. The plan asked for mock fixtures
"constructed by hand in tests"; they are hand-built but live in a module, not in `#[cfg(test)]`,
for two reasons: the PTY smoke and `--dump --mock` are non-test binaries and could not see
test-local fixtures, and `Stage 6 #3` needs a runnable demo before a1 lands. No test depends on
a1's code — every b1 test seeds its own `RowLayout`/`GraphLayout`.

## 7. Deviations, with reasons

1. **`src/layout/mod.rs` gained one declaration** — `pub mod types;`, with a short comment saying
   why b1 touched a1's file. Without it `crate::layout::types` does not resolve, so b1 could not
   compile or test. This is the only cross-lane edit; **W2 merge point**: a1 will add the same
   declaration, and if the wording differs git will report a trivial conflict on that one hunk.
2. **`src/layout/types.rs` was created here** as the plan's documented fallback ("if a1's file is
   absent in your worktree, CREATE ONLY `types.rs` exactly as contracted"). a1 has since landed
   its canonical copy; the two differ only in doc comments and `Hash` derives (§5 item 4).
   **a1's file must win at merge** — and the swap test shows that merges cleanly.
3. **`--mock` flag added to the binary** (an addition to the CLI, not a change of P0 behavior):
   `--dump` alone still prints the P0 text view, and the graph frame is dumped at 140 columns
   instead of 100 so the whole status bar fits on one reviewable line. Needed to exercise the
   renderer before `layout()` lands.
4. **Lane-change geometry**: see §3.1 — a wider jump is drawn as a connected step, not a bar.
5. **P0's text path was restored after a self-caught regression.** My first cut replaced P0's
   `commit_list` (bordered block, `{idx:>5} {abbrev} {subject} ({refs})`) with the new 5-column
   table and moved the title into the status bar. The post-write check caught that this contradicted
   "keep the P0 view unchanged", so the text path is now P0's rendering verbatim and the dated
   table is used only by the graph view. The status bar keeps P0's styles and P0's key hint and
   only adds `repo:`, `branches:`, `lanes:`, `graph cols` (the plan's lane-width awareness) plus a
   `(narrow: text clipped)` hint below the text minimum width.
6. **`Mock` palette duplication is intentional**: `mock::frozen_palette()` duplicates the array
   that a1's `layout()` will fill in, so b1 is testable standalone. At merge, the graph view takes
   the palette from `GraphLayout` (it already does — `mock` only supplies it in this worktree).
7. **PTY artifacts are not committed** (`target/` is git-ignored). They are regenerable in one
   command (§4); committing frame files would freeze evidence that the plan did not ask for.
8. **`tools/pty-shot.sh` uses `script -q -c`** (a real PTY with a controlled window size) around the
   plan's `stty`/`timeout`/`strings` idea. The plan's literal `strings`-only view loses the frame
   structure, so `.txt` (ANSI-stripped, one line per cursor move) is the review artifact and
   `.strings.txt` is kept for the plan's literal wording. Nothing is written outside `target/pty/`.
9. **No pushes; `development` untouched.** All commits are on `ggt-m1-w1-b1`; `git log
   development..HEAD` lists only this lane's commits and the worktree is clean after committing.
10. **No `.IDE_Plans/` edits** (the plan's own rule) — status lives in this report.
11. **Reference repo was only read** (`src/config.ts:270-285`); no mutation, no write, no REA need.
12. **Colour output beyond the palette** (256-colour / RGB detection, `COLORTERM=truecolor`) is
    **not** implemented — it is M3's "full detection" item. The widget takes `colour: bool` +
    `GlyphTier` today, so M3 can extend without touching the grid builder.

## 8. Open questions / escalations

1. **Graph-view chrome — decided by the user during this session: borderless.** The graph view
   has no border; the P0 text fallback keeps P0's bordered title box. Status bar carries
   repo/branches/lanes/graph width.
2. **Real-repo graph cells are empty until a1 merges.** `main.rs` passes `graph: None` for a real
   repo, so today the real repo renders the P0 text list and the graph demo needs `--mock`. When
   a1's `layout()` lands, main should always build `ViewOptions{graph: Some(&layout)}` — a 3-line
   change in `main.rs` that belongs to W2/PM, not to this lane. Who owns that follow-up?
3. **PTY evidence on the merged result.** The PTY frame in §4 is from the mock path at this commit.
   Is a re-capture on `development` after a1 → b1 merge required for Stage 6 #3 (I assume yes, and
   it is one command), or is this frame enough?
4. **Verifier scope.** This lane's tests are all unit tests; there is no committed integration
   test (those are a1's `tests/golden_layout.rs`). Does cline want a b1-specific integration test
   that renders a *golden* layout (from `tests/fixtures/golden/*.json`) once a1 lands? That would
   duplicate a1's assertion, so I did not add it.
5. **`Horizontal` semantics** (§3.1) — needs one confirming sentence from a1 before the W2 review
   closes; not a blocker for either lane's own gate.

## 9. REA statement

REA was **not** triggered: no compiled or shipped JavaScript was analyzed. b1 read one TypeScript
source file for the palette default and coded against D8 + the plan's contract. The reference
checkout is untouched.

## 10. Reproduce from scratch (clean shell)

```bash
cd /mnt/Just_Do_It/02_Devp_Soft/01_Adi-Mudi/11_Git_Graph/git-graph-terminal-b1

export PATH="$HOME/.cargo/bin:$PATH"
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test     # acceptance 1-2

cargo run -q -- --dump --mock      # deterministic frame (140 cols): graph + text + status bar
cargo run -q -- --dump .           # real repo, P0 text view (100 cols, unchanged)

./tools/pty-shot.sh                # acceptance 3 -> target/pty/mock-graph.{raw,txt,strings.txt,frame}
COLS=140 ./tools/pty-shot.sh wide  # wider pty shot; MOCK=0 ./tools/pty-shot.sh real
sed -n '/repo: (mock)/p' target/pty/mock-graph.txt | tail -1
```
