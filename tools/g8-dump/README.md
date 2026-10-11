# g8-dump — G8 golden-fixture harness (P0 T3 seed)

Executes **the reference's own layout code** on small fixture repositories and writes the
result to `tests/fixtures/golden/<fixture>.json`. Those files are the seed of the D2 parity
gate (G8): W1's Rust port of `Vertex` / `Branch` / `Graph::determinePath()` must reproduce
the same rows, lanes, colours, parent links and line geometry.

```
fixture repo (git)
  └─ reference GitCommit[] model        (mirrors vscode-git-graph src/dataSource.ts getLog()/getRefs())
      └─ reference Graph.loadCommits()  (vscode-git-graph web/graph.ts, executed unmodified in jsdom)
          └─ tests/fixtures/golden/<fixture>.json
```

No file in the reference checkout is written to, and no compiled/shipped JS is involved:
`web/graph.ts` is transpiled in memory (`ts.transpileModule`) and run in a `new Function`
scope with a minimal jsdom document. Only `loadCommits()` runs — `render()`/`draw()` are
never called (they need a real layout engine and are not part of the topology contract).

## Requirements

- node ≥ 20 (developed on v22.22.1), npm, git
- the read-only reference checkout. Default path: `../../../vscode-git-graph`
  (sibling of this project). Override with `--reference <dir>` or `G8_REFERENCE_REPO=<dir>`.

## Exact re-run commands (clean shell)

```bash
cd tools/g8-dump
npm install            # devDeps only: typescript ^5.9.3, jsdom ^30.1.2
npm run fixtures       # creates tests/fixtures/repos/{linear,branch-merge-rebase}
npm run dump           # writes tests/fixtures/golden/*.json
npm run check          # re-runs and fails if the committed golden files differ
```

One-liner from the project root:

```bash
cd tools/g8-dump && npm install && npm run fixtures && npm run dump && npm run check
```

Single fixture / dry runs:

```bash
node g8-dump.mjs --fixture linear --stdout                 # print, do not write
node g8-dump.mjs --all --check                             # verify committed goldens
node g8-dump.mjs --all --repos /tmp/somewhere --out /tmp/g # other fixture set / output dir
node g8-dump.mjs --help
```

## Output contract

`tests/fixtures/golden/<fixture>.json` — `rows` is the binding contract from
`p0-plan_v1.0.md` T3:

| field | meaning |
| ----- | ------- |
| `row` | vertex id == commit index, newest commit is row 0 (`Vertex.id`) |
| `lane` | graph grid column of the commit node (`Vertex.x`); `-1` when the reference draws no node for that row (`Vertex.draw()` returns early when `onBranch === null`) |
| `colour` | index into the 12-entry palette (`Vertex.getColour()` → `Branch.getColour()`); `-1` when the row has no branch. **Palette index, not column** (D8 ruling §3.4.2) |
| `parents` | parent vertex ids in `git` order; `-1` = `NULL_VERTEX_ID` = parent outside the loaded graph |
| `lockedFirst` | row projection of `Line.lockedFirst`: `true` when a branch line leaving this row (`line.p1.y === row`) is locked to `p1`; `false` when the row has no outgoing line |

Additive extras (they do not change `rows`):

- `rowNodes[]` — `isCurrent`, `isCommitted`, `isMerge`, `onBranch`
- `branches[]` — `colour`, `end`, `numUncommitted`, and every `line {p1, p2, lockedFirst}`
  in graph space; this is the data W1 needs to assert line routing, not just node placement
- `reference.sha256` — hash of the exact `web/graph.ts` that produced the file
- `repo.commitHashes` — fixture tip history, so a drift shows up as a readable diff

**Frozen contract:** the PM accepted this schema — including the `lane: -1` sentinel and the
relative `repo.path` — as the D2 parity contract for W1 (2026-10-11, `p0-report.md` §8).

## Fixture repositories

Created by `make-fixtures.sh` under `tests/fixtures/repos/` (git-ignored, generated):

| fixture | shape |
| ------- | ----- |
| `linear` | 5 commits on one branch, annotated tag `v1`, no merges → 1 lane, 1 colour index |
| `branch-merge-rebase` | `main` (m1–m4) + `feature` forked at m1 (f1–f2) + a `--no-ff` merge into `main` + `rebase-like` forked at m1 (r1–r2), annotated tag `v2` → 9 commits, 3 lanes, one merge row, colour reuse |

### Determinism

Every commit has a pinned author/committer name, email and timestamp, pinned default
branch (`main`), GPG signing off and `core.autocrlf=false`, so fixture object hashes —
and therefore the golden JSON — are identical on any machine. `--check` verifies this.

The script is add-only: an existing fixture repository is never deleted or rewritten.
To regenerate from scratch, remove the fixture directories you want rebuilt and re-run:

```bash
rm -rf tests/fixtures/repos/linear tests/fixtures/repos/branch-merge-rebase   # your call
npm run fixtures
```

To cross-check determinism without touching the committed fixtures, generate a second set
elsewhere and diff:

```bash
bash make-fixtures.sh /tmp/g8-fixtures/repos
node g8-dump.mjs --all --repos /tmp/g8-fixtures/repos --out /tmp/g8-golden
diff -r ../../tests/fixtures/golden /tmp/g8-golden   # must be empty
```

## Notes / limitations (P0)

- `node_modules/` and `tests/fixtures/repos/` are git-ignored; the golden JSON files are
  committed and are the reviewable artifact.
- Stashes and the "uncommitted changes" row are not modelled (`stash: null`,
  `--max-count=1000`), because `Graph.loadCommits()` only needs `hash`, `parents`, `stash`
  and the HEAD hash. Larger fixtures for perf work (G5, 30k commits) are M1 scope.
- npm prints `EBADENGINE` for the transitive `w3c-xmlserializer@6.0.0` (wants node ≥
  22.22.2; this machine has 22.22.1). It is a warning only — install and dumps work.
  CI should pin node ≥ 22.22.2.
