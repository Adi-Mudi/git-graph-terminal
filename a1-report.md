# M1-W1 a1 report — layout core

Plan: `git-graph-terminal/.IDE_Plans/w1-plan_v1.0.md` (lane `a1`, branch `ggt-m1-w1-a1`)
Worktree: `git-graph-terminal-a1` · Worker: pi · Verifier: cline · PM: Hermes

## 0. Status against the plan's items

| # | Plan item (lane a1) | Status | Evidence |
| - | ------------------- | ------ | -------- |
| 1 | T1 — `src/layout/types.rs` exactly per contract | DONE | §2 |
| 2 | T2 — port `determinePath`, `getAvailableColour`, point/connection bookkeeping, the 4 nav queries, palette, settings | DONE | §3 |
| 3 | T3 — `tests/golden_layout.rs`, exact equality on the golden JSONs | DONE | §4 |
| 4 | T4 — fixture that exercises `lane:-1`/`on_branch:false`, regenerate goldens, cover in the Rust test | DONE **with a documented substitution** | §5 |
| 5 | T5 — unit tests: stash vertex, `onlyFollowFirstParent`, parent outside range | DONE | §6 |
| — | Gate: `cargo test` green incl. golden equality; fmt + clippy clean; report | DONE | §7 |

Stage-6 acceptance (after merge, PM-owned) — a1's contribution:

| # | Acceptance item | a1 status | Evidence |
| - | --------------- | --------- | -------- |
| 1 | FULL TEST green on `development` | a1 half green locally; merge test still owed | §7 |
| 2 | G8 gate: golden fixtures pass against the Rust layout (exact) | DONE — 3/3 fixtures | §4 |
| 3 | TTY demo renders graph + text + status bar | b1's half | — |
| 4 | D4 sentinel covered; worktrees clean; reference repo untouched | DONE (§5 covers the reachable sentinel; see §9 note 1) | §1, §5 |

## 1. Environment (exact)

| tool | version |
| ---- | ------- |
| rustc / cargo | 1.99.0 (b940084d7 2026-09-28) / cargo 1.99.0 (5f94df478 2026-08-27) |
| rustfmt / clippy | 1.10.0-stable (b940084d7e 2026-09-28) / 0.1.99 (b940084d7e 2026-09-28) |
| node / npm | v22.22.1 / 11.19.1 |
| harness deps | typescript 5.9.3, jsdom 30.1.2 (unchanged by this lane) |
| reference checkout | `../vscode-git-graph` @ `d7f43f429a9e024e896bac9fc65fdc530935c812`, `web/graph.ts` sha256 `86a667151fcef285213e3059bcf460df5cfb8b77111f0f9c414f57c25850e4e9` |

The reference checkout's `git status --porcelain` is **empty** (0 entries) — read only.

Base: `development` @ `226516c` (P0 merged). Worktree `../git-graph-terminal-a1` is its own
checkout, with `tools/g8-dump/node_modules` symlinked to the base worktree's install
(worktrees do not carry `node_modules`; nothing else is shared).

## 2. T1 — `src/layout/types.rs` (the a1↔b1 contract)

Created exactly as the plan's contract block specifies — same names, same field names, same
types, same order:

`Lane`, `RowIdx`, `VertexRef`, `Segment`, `SegKind`, `RowLayout`, `GraphLayout`.

Two things the contract left implicit and a1 pinned down in doc comments (both fixed by
evidence, not invented):

1. **`SegKind` is derived from geometry only.** The golden JSON records coordinates, not a
   style variant, so the parity test can only re-derive the expected variant from
   `(col_from, col_to)`: equal → `Vertical`, one lane apart → `RoundedCorner(x2 < x1)`,
   anything wider → `Horizontal`. `Diagonal` and `MergeArc` are kept (the enum is the
   contract) but are never emitted; they are reserved for M3's angular styling.
2. **`VertexRef::on_branch` is always `true`** for anything `layout()` produces — see §5.

`src/layout/mod.rs` re-exports the contract types (`pub use types::{...}`) and keeps the
P0-scaffold items b1 depends on **unchanged**: `CELLS_PER_LANE` and
`struct Cell { glyph, lane, pers }`. Only the P0 `struct Layout` placeholder was replaced
(by `LayoutOpts` + the real API) — see §9 note 3.

## 3. T2 — the port (`src/layout/mod.rs`, +808/−20)

Reference functions, read line by line and ported keeping the reference's own names and
control flow so a reviewer can diff the two side by side:

| reference (`web/graph.ts`) | port |
| -------------------------- | ---- |
| `loadCommits()` (392-425) — parent/child links, `NULL_VERTEX_ID` for out-of-range parents | `engine_for()` / `Engine::load_commits` |
| `determinePath()` (705-763), both paths, incl. `registerParentProcessed()` | `Engine::determine_path` |
| `getAvailableColour()` (765-773) | `Engine::available_colour` |
| `getNextPoint()` (257-259), `getPointConnectingTo()` (261-267), `registerUnavailablePoint()` (269-274) | same-named methods on the ported `Vertex` |
| `addToBranch()` (231-236), `addLine()` | `Vertex::add_to_branch`, `Branch::add_line` |
| the 4 queries (600-672) | `nav()` + `derive_adjacency()` |
| palette (`src/config.ts:275-277`) | `PALETTE` (verified exactly equal in the golden test) |
| graph config defaults (`src/config.ts:278-283`) | `defaultGraphConfig()` in the harness; `LayoutOpts` mirrors the settings the harness can vary |

Notes worth a reviewer's attention:

- **`onlyFollowFirstParent` is deliberately not a `LayoutOpts` field.** The plan's contract
  fixes `LayoutOpts` to `{ expand_first_parent }`. `layout()` therefore always uses the
  extension default (`false`), which is what the goldens were produced with; the *other*
  value is driven through the private engine builder by a unit test (§6).
- **`nav()` must recover adjacency from segments.** The reference queries read the live
  `Vertex` graph, but the contract's `GraphLayout` carries only vertices + segments, so the
  port traces each segment path forward (`col_from == lane`, following same-colour
  segments) to rebuild parent/child links. "Same branch" is decided by equal `colour_idx`,
  which is what the reference's `isOnThisBranch()` compares.
- **`nav()` returns `NO_ROW` (= `RowIdx::MAX`)** where the reference returns `-1`
  (`graph.ts:598-672`), because `RowIdx` is unsigned. Documented on the constant.
- **`parent_rows()` is additive, not contract.** The contract's `RowLayout` has no parent
  link, so the parity test needs a way to assert the golden `rows[].parents` field. See §8.

## 4. T3 — G8 golden equality

`tests/golden_layout.rs` discovers every `tests/fixtures/golden/*.json`, rebuilds the exact
`Commit[]` the harness fed the reference, runs `layout()`, and asserts **exact** equality
(no tolerances) against the reference's own output:

- per row: `parents` (through `parent_rows()`), `lane`, `colour`, `isCurrent`→`is_head`,
  `onBranch`→`on_branch`, `RowLayout.row`;
- per row: **every segment** — `col_from`, `col_to`, `kind`, `colour_idx`, and the `pers`
  winner recomputed from the golden's `lockedFirst`/`x1`/`x2` (`pers = lockedFirst ? x1 : x2`);
- `max_lane` recomputed from the golden (vertex lanes **and** both segment ends — the
  highest column in use, per D8 §3.1);
- the 12-entry `palette` element by element;
- golden self-consistency: every line spans exactly one row (`y2 == y1 + 1`), and
  `rows[].lockedFirst` equals "any outgoing line locked to `p1`".

Fixtures: `linear` (5 rows, 1 lane), `branch-merge-rebase` (9 rows, 3 lanes, 1 merge row,
colour reuse) and the new `branch-merge-rebase-truncated` (6 rows, 3 lanes, 3× `parents: [-1]`).

The test walks the JSON with a small hand-written parser (≈200 lines, no `serde_json`) —
deliberate: the project adds no dependency it can avoid (Rule 7 / Dependencies).

Determinism: the harness pins every fixture commit's author+committer date, so
`--date-order` is total; the committed goldens are byte-stable. Re-verified for this lane:
`node g8-dump.mjs --all --check` → 3/3 ok, and the two pre-existing golden files came back
**byte-identical** (empty `git diff`) after the harness change.

## 5. T4 — the D4 sentinel

### 5.1 What the plan expected

> extend tools/g8-dump with a fixture that exercises `lane:-1`/`on_branch:false`
> (followed-only-parent or filtered commit), regenerate goldens, cover it in the Rust test.

The plan's contract repeated the expectation: "`lane:-1` sentinel ⇒ `on_branch:false` —
this is D4".

### 5.2 Finding: `lane:-1` / `on_branch:false` is unreachable for a commit list

Reading the reference, the loop that builds the graph is

```js
i = 0;
while (i < this.vertices.length) {
    if (this.vertices[i].getNextParent() !== null || this.vertices[i].isNotOnBranch()) {
        this.determinePath(i);
    } else {
        i++;
    }
}
```

(`web/graph.ts:433-439`; `isNotOnBranch()` is `onBranch === null`, `:238-240`.)

1. A vertex with **no parents** has `getNextParent() === null`, so `isNotOnBranch()`
   decides. If it is not yet on a branch, `determinePath()` runs.
2. `determinePath()` has exactly two exits, and both leave the start vertex on a branch:
   - the *normal* path calls `vertex.addToBranch(branch, lastPoint.x)` unconditionally
     (`:733`);
   - the *merge* path is guarded by `!vertex.isNotOnBranch()` (`:709`) — that vertex is
     already on a branch.
3. A vertex with an **out-of-range parent** has `getNextParent() === nullVertex`, which is
   `!== null`, so `determinePath()` runs too.

So every vertex ends up with a branch, `on_branch` is always `true`, and `lane: -1` can
never appear. `Vertex.draw()`'s `if (this.onBranch === null) return;` (`:301`) is reachable
only for a `Vertex` that was never fed to `loadCommits()` — not for a commit list.

### 5.3 Empirical confirmation (the reference's own code)

Written proof is one thing; the reference was also **executed**. The probe below drives the
reference's real `loadCommits()` (via the harness's `reference-graph.mjs` loader) over **18**
cases: 2 fixture repositories × 2 `onlyFollowFirstParent` values, 2 truncated windows × 2
values, and 5 synthetic boundary lists × 2 values (out-of-range parent, `*` uncommitted row,
filtered tail, two disjoint roots, single root).

```js
// /tmp/g8-probe.mjs — does the reference ever leave a vertex off-branch (`lane: -1`)?
// Usage: node /tmp/g8-probe.mjs <a1-worktree-root>
import path from 'node:path';

const ROOT = process.argv[2];
if (!ROOT) throw new Error('usage: node /tmp/g8-probe.mjs <a1-worktree-root>');

const lib = (name) => path.join(ROOT, 'tools/g8-dump/lib', name);
const { loadReferenceGraph, defaultGraphConfig } = await import(lib('reference-graph.mjs'));
const { readRepositoryCommits } = await import(lib('git-commits.mjs'));

const REF = path.resolve(ROOT, '..', 'vscode-git-graph');
const reference = loadReferenceGraph(REF);

function report(label, commits, commitHead, lookup, only) {
  const graph = new reference.Graph('graph', reference.document.body, defaultGraphConfig(), {});
  graph.loadCommits(commits, commitHead, lookup, only);
  const onBranch = graph.vertices.map((v) => v.onBranch !== null);
  const lanes = graph.vertices.map((v, i) => (onBranch[i] ? v.getPoint().x : -1));
  const colours = graph.vertices.map((v, i) => (onBranch[i] ? v.getColour() : -1));
  const off = lanes.filter((x) => x < 0).length;
  console.log(
    `${label.padEnd(38)} onlyFirstParent=${String(only).padEnd(5)} rows=${commits.length}` +
      ` offBranch=${off} minLane=${Math.min(...lanes)} lanes=[${lanes.join(',')}] colours=[${colours.join(',')}]`,
  );
}

const mk = (hash, parents) => ({
  hash, parents, stash: null,
  authorName: 'a', authorEmail: 'a@b', authorDate: 0,
  committerName: 'a', committerEmail: 'a@b', committerDate: 0,
  message: hash, refs: [],
});
const lookupOf = (commits) => Object.fromEntries(commits.map((c, i) => [c.hash, i]));
const fixture = (name, win) =>
  readRepositoryCommits(path.resolve(ROOT, 'tests/fixtures/repos', name), win);

console.log('--- 1. real fixture repositories, full window ---');
for (const name of ['linear', 'branch-merge-rebase']) {
  const { commits, commitHead, commitLookup } = fixture(name);
  for (const only of [false, true]) report(`repo:${name}`, commits, commitHead, commitLookup, only);
}

console.log('--- 2. truncated commit window (the new D4 fixture: parents: [-1]) ---');
for (const [name, win] of [['linear', 3], ['branch-merge-rebase', 6]]) {
  const { commits, commitHead, commitLookup } = fixture(name, win);
  for (const only of [false, true]) {
    report(`repo:${name}@window${win}`, commits, commitHead, commitLookup, only);
  }
}

console.log('--- 3. synthetic boundary/edge commit lists ---');
const cases = {
  'out-of-range-parent': [mk('a', ['deadbeef']), mk('b', ['c']), mk('c', [])],
  'uncommitted-row(*)': [mk('*', ['c1']), mk('c1', ['c2']), mk('c2', [])],
  'filtered-tail': [mk('head', ['mid']), mk('mid', ['gone'])],
  'two-roots': [mk('a', ['b']), mk('b', []), mk('x', ['y']), mk('y', [])],
  'single-release': [mk('a', [])],
};
for (const [label, commits] of Object.entries(cases)) {
  const head = commits[0].hash;
  for (const only of [false, true]) report(label, commits, head, lookupOf(commits), only);
}
```

Output (`node /tmp/g8-probe.mjs <a1-worktree-root>`, run from `tools/g8-dump`):

```
--- 1. real fixture repositories, full window ---
repo:linear                            onlyFirstParent=false rows=5 offBranch=0 minLane=0 lanes=[0,0,0,0,0] colours=[0,0,0,0,0]
repo:linear                            onlyFirstParent=true  rows=5 offBranch=0 minLane=0 lanes=[0,0,0,0,0] colours=[0,0,0,0,0]
repo:branch-merge-rebase               onlyFirstParent=false rows=9 offBranch=0 minLane=0 lanes=[0,0,1,1,2,2,1,1,0] colours=[0,0,1,1,2,2,1,1,0]
repo:branch-merge-rebase               onlyFirstParent=true  rows=9 offBranch=0 minLane=0 lanes=[0,0,1,1,2,2,1,1,0] colours=[0,0,1,1,2,2,1,1,0]
--- 2. truncated commit window (the new D4 fixture: parents: [-1]) ---
repo:linear@window3                    onlyFirstParent=false rows=3 offBranch=0 minLane=0 lanes=[0,0,0] colours=[0,0,0]
repo:linear@window3                    onlyFirstParent=true  rows=3 offBranch=0 minLane=0 lanes=[0,0,0] colours=[0,0,0]
repo:branch-merge-rebase@window6       onlyFirstParent=false rows=6 offBranch=0 minLane=0 lanes=[0,0,1,1,2,2] colours=[0,0,1,1,2,2]
repo:branch-merge-rebase@window6       onlyFirstParent=true  rows=6 offBranch=0 minLane=0 lanes=[0,0,1,1,2,2] colours=[0,0,1,1,2,2]
--- 3. synthetic boundary/edge commit lists ---
out-of-range-parent                    onlyFirstParent=false rows=3 offBranch=0 minLane=0 lanes=[0,1,1] colours=[0,1,1]
out-of-range-parent                    onlyFirstParent=true  rows=3 offBranch=0 minLane=0 lanes=[0,1,1] colours=[0,1,1]
uncommitted-row(*)                     onlyFirstParent=false rows=3 offBranch=0 minLane=0 lanes=[0,0,0] colours=[0,0,0]
uncommitted-row(*)                     onlyFirstParent=true  rows=3 offBranch=0 minLane=0 lanes=[0,0,0] colours=[0,0,0]
filtered-tail                          onlyFirstParent=false rows=2 offBranch=0 minLane=0 lanes=[0,0] colours=[0,0]
filtered-tail                          onlyFirstParent=true  rows=2 offBranch=0 minLane=0 lanes=[0,0] colours=[0,0]
two-roots                              onlyFirstParent=false rows=4 offBranch=0 minLane=0 lanes=[0,0,0,0] colours=[0,0,0,0]
two-roots                              onlyFirstParent=true  rows=4 offBranch=0 minLane=0 lanes=[0,0,0,0] colours=[0,0,0,0]
single-release                         onlyFirstParent=false rows=1 offBranch=0 minLane=0 lanes=[0] colours=[0]
single-release                         onlyFirstParent=true  rows=1 offBranch=0 minLane=0 lanes=[0] colours=[0]
```

**18/18 cases: `offBranch=0`, `minLane=0`.** Both of the plan's suggested mechanisms were
tested and neither produces `lane:-1`:

- "followed-only-parent" (`onlyFollowFirstParent=true`) only *suppresses* the out-of-range
  link for secondary parents (`graph.ts:415-417`); the row still gets a branch.
- "filtered commit" produces exactly the **reachable** sentinel — a commit whose parent is
  not in the list gets `parents: [-1]` and stays on a branch.

### 5.4 The delivered fixture: the reachable sentinel, from a real repository

Since `lane:-1` cannot be exercised, the reachable form of the same frozen sentinel is
covered instead: **`parents: [-1]` (`NULL_VERTEX_ID`)** on a commit at the commit-window
boundary.

`tools/g8-dump/g8-dump.mjs` grew a `TRUNCATED_WINDOWS` table; `branch-merge-rebase-truncated`
is the *same real repository* dumped through a 6-commit window. `lib/git-commits.mjs` grew an
optional `maxCommits` parameter that mirrors the reference exactly: `getCommits()` asks git
for `maxCommits + 1` and drops the extra commit (`src/dataSource.ts:167-172`) — this is how
the graph view pages history, so the window is a real reference behaviour, not a synthetic
input.

The new golden `tests/fixtures/golden/branch-merge-rebase-truncated.json` (6 rows, 3 lanes,
colour reuse) is reference-generated, and cross-checks against the full fixture exactly as
theory says — identical `commitHashes[0..6]`, identical `commitHead`, identical lane /
colour / lockedFirst on every row, with only the out-of-window parents flipped to `-1`:

| row | lane | colour | `parents` (full) | `parents` (window 6) |
| --- | ---- | ------ | ---------------- | -------------------- |
| 0 | 0 | 0 | `[1]` | `[1]` |
| 1 | 0 | 0 | `[8]` | **`[-1]`** |
| 2 | 1 | 1 | `[3, 4]` | `[3, 4]` |
| 3 | 1 | 1 | `[6]` | **`[-1]`** |
| 4 | 2 | 2 | `[5]` | `[5]` |
| 5 | 2 | 2 | `[8]` | **`[-1]`** |

### 5.5 Rust-side coverage

- `parent_rows()` maps `parents: [-1]` to `None`, and the test asserts the golden's `-1`
  entries arrive as `None` — so the sentinel is compared, not skipped.
- The test **requires** at least one golden row with `parents: [-1]` (`null_vertex_parents >= 1`),
  and requires ≥3 fixtures, so the coverage cannot silently disappear.
- The test also asserts the negative: **no** golden row may carry `lane:-1`, `colour:-1` or
  `onBranch:false`. If a future fixture ever produces one, the test fails loudly and §5 must
  be revisited instead of the port silently drifting.

## 6. T5 — unit tests (`src/layout/mod.rs`, `mod.rs::tests`)

| test | covers |
| ---- | ------ |
| `empty_input_yields_empty_layout` | zero commits → zero rows, no panic |
| `linear_chain_is_one_lane_and_one_column` | one lane, one colour, vertical segments |
| `stash_vertex_is_flagged_from_refs` | `refs/stash` → `is_stash` (plan item 5a) |
| `only_follow_first_parent_drops_later_parents` | engine driven with `onlyFollowFirstParent=true`; secondary parents suppressed (plan item 5b) |
| `parent_outside_the_loaded_range_becomes_null_vertex` | `parents: [-1]` → `None` in `parent_rows()` (plan item 5c) |
| `nav_resolves_parents_children_and_alternatives` | `Up`/`Down`/`Expand`/`Collapse` against `graph.ts:598-672` |
| `nav_out_of_range_is_no_row` | `NO_ROW` instead of the reference's `-1` |

## 7. Test gate (Rule 6)

```
$ cargo fmt --check                      # clean, no diff
$ cargo clippy --all-targets -- -D warnings   # clean (CI runs the narrower `cargo clippy -- -D warnings`)
$ cargo test                             # 14 passed + 15 passed, 0 failed
```

`cargo test` detail: 7 `data::` + 7 `layout::` unit tests, plus the `golden_layout`
integration binary (1 test, 3 fixtures) — the integration binary reports 15 (14 + 1 golden).

```
$ cd tools/g8-dump && node g8-dump.mjs --all --check
ok   branch-merge-rebase: matches tests/fixtures/golden/branch-merge-rebase.json
ok   branch-merge-rebase-truncated: matches tests/fixtures/golden/branch-merge-rebase-truncated.json
ok   linear: matches tests/fixtures/golden/linear.json
```

Golden parity check inside `cargo test`:

```
ok   golden parity: branch-merge-rebase-truncated.json
ok   golden parity: branch-merge-rebase.json
ok   golden parity: linear.json
```

No test in the repository was skipped, weakened, or made conditional.

## 8. Deviations, with reasons

1. **T4's stated goal (`lane:-1` fixture) was not delivered as written** — it is not
   deliverable. Delivered instead: the reachable sentinel `parents: [-1]`, from a real
   repository, plus a written and executed proof of unreachability (§5). Reason: the plan's
   two suggested mechanisms were both tested against the reference and both produce a
   *different* sentinel (§5.3). Escalated in §9 note 1; the README of `tools/g8-dump` and
   `src/layout/mod.rs` document the substitution where a reviewer will look for it.
2. **`parent_rows()` added to `src/layout/mod.rs`** — additive, non-contract, documented as
   such. Reason: the contract's `RowLayout` carries no parent links, but T3 requires
   asserting the golden `rows[].parents` field.
3. **`nav()` recovers adjacency from segments** rather than reading live vertices. Reason:
   the contract's `GraphLayout` has no adjacency. Documented on `nav()`.
4. **`LayoutOpts` has no `onlyFollowFirstParent` field.** Reason: the contract fixes the
   struct's shape; `layout()` uses the extension default (what the goldens were produced
   with) and the other value is covered by a unit test through the engine.
5. **No dependency added.** The golden test parses JSON with a local minimal parser instead
   of `serde_json`. Reason: Rule 7 — never add a dependency that built-ins can avoid.
6. **Harness scope widened by one file's worth of change** (`lib/git-commits.mjs` +
   `g8-dump.mjs` window support + `README.md`). Reason: required by T4 ("extend
   tools/g8-dump, regenerate goldens"); the committed goldens for the two pre-existing
   fixtures are byte-identical, so nothing was re-baselined.
7. **`tools/g8-dump/node_modules` is a symlink** to the base worktree's install and shows as
   untracked (the `.gitignore` pattern has a trailing slash, which git does not apply to
   symlinks). Left alone; not staged. Flagged so the verifier does not mistake it for a stray
   artifact.

## 9. Open questions / escalations

1. **To PM/verifier:** T4's wording says "a fixture that exercises `lane:-1`/`on_branch:false`".
   a1's finding is that this state is unreachable for a commit list, and that the *reachable*
   sentinel is `parents: [-1]`. Please either accept the substitution on the strength of §5,
   or re-scope T4. Restating T4 as "cover the reachable `NULL_VERTEX_ID` sentinel" is a
   one-line plan change; a1 did not edit the plan.
2. **To b1 (cross-lane risk):** `layout::Layout` (the P0 placeholder struct) is **gone** —
   replaced by `LayoutOpts` + `layout()`. `Cell` and `CELLS_PER_LANE` are unchanged. b1 must
   import `layout::types::*` (or the re-exports) and must not reference `layout::Layout`.
3. **To PM:** the golden set is now three files. If any downstream gate hard-codes "two
   goldens", it needs the new count.

## 10. REA statement

No compiled or shipped JavaScript was disassembled for this lane. Everything in §3 and §5
comes from **reading the reference TypeScript source** (`web/graph.ts`, `src/config.ts`,
`src/types.ts`, `src/dataSource.ts`) with line anchors quoted above, and from **executing the
reference's own `loadCommits()`** through the harness loader (§5.3). The reference checkout
was read only — its `git status --porcelain` is empty.

## 11. Reproduce from scratch (clean shell)

```bash
# 1. Rust side — from the a1 worktree root
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test

# 2. G8 harness — needs the read-only reference at ../../../vscode-git-graph
cd tools/g8-dump
npm ci                       # or reuse the symlinked node_modules
npm run fixtures             # rebuild tests/fixtures/repos/{linear,branch-merge-rebase}
npm run dump                 # regenerate all three goldens
npm run check                # must print 3x "ok"

# 3. The D4 reachability probe (throwaway, §5.3)
node /tmp/g8-probe.mjs "$PWD/../.."   # must print offBranch=0 and minLane=0 on all 18 lines
```

Lane commits land on `ggt-m1-w1-a1`; `git log --oneline development..HEAD` lists them.
