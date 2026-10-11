# P0 verification report — independent verification of T1–T4

Verifier: cline (independent) · PM: Hermes · Worker: pi
Date: 2026-10-11 · Basis: measured by re-running everything, no claim taken on trust.

Objects verified

| Object | Path |
| ------ | ---- |
| Worktree / branch | `/mnt/Just_Do_It/02_Devp_Soft/01_Adi-Mudi/11_Git_Graph/git-graph-terminal-p0` @ `ggt-p0-setup` = `cc9417aab1b58d9d6ba61a59929c0126e5b2627e` |
| Plan (read-only) | `git-graph-terminal/.IDE_Plans/p0-plan_v1.0.md` |
| Worker report | `git-graph-terminal-p0/p0-report.md` |
| Reference (read-only) | `../vscode-git-graph` @ `d7f43f429a9e024e896bac9fc65fdc530935c812` |

**Verdict: ACCEPT** — all 5 acceptance items PASS. 4 defects, none blocking
(3 × Low, 1 × Info). Details and raw evidence below.

Environment measured: cargo/rustc 1.99.0, rustup 1.29.1, rustfmt 1.10.0-stable,
clippy 0.1.99, node v22.22.1, npm 11.19.1, git 2.53.0.

## Acceptance items

### Item 1 — `cargo build && cargo test` green — PASS

```
$ export PATH="$HOME/.cargo/bin:$PATH"; cargo --version; rustc --version
cargo 1.99.0 (5f94df478 2026-08-27)      rustc 1.99.0 (b940084d7 2026-09-28)

$ cargo build ; echo "BUILD_EXIT=$?"
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.11s
BUILD_EXIT=0

$ cargo test ; echo "TEST_EXIT=$?"
running 5 tests
test data::tests::parses_two_commits_with_parents_and_refs ... ok
test data::tests::skips_malformed_and_accepts_empty_payload ... ok
test data::tests::subject_with_newline_cannot_break_records ... ok
test data::tests::non_repository_is_an_error_not_a_panic ... ok
test data::tests::unborn_branch_yields_no_commits ... ok
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
TEST_EXIT=0

$ cargo fmt --check; echo $?   -> FMT_EXIT=0
$ cargo clippy --all-targets -- -D warnings; echo $?   -> CLIPPY_EXIT=0
```

Reproduced independently (the worker's §6 numbers match mine). No warnings, no
skipped/ignored tests. The plan's T2 module layout exists: `src/main.rs`,
`src/data.rs`, `src/layout/mod.rs` (stub + D8 comment), `src/render/mod.rs`.

### Item 2 — TUI renders real commits + status bar — PASS

```
$ timeout 3 script -qec 'stty rows 24 cols 80; ./target/debug/git-graph-terminal' /tmp/verify-tui.raw
SCRIPT_EXIT=124 (killed by timeout, expected for an interactive loop)
3053 bytes captured; alternate screen + raw mode entered, clean teardown
```

Rendered frame (escape codes stripped; box layout re-aligned for readability —
subjects, hashes and the status bar are verbatim from the 3053-byte capture):

```
┌commits(9)·branches(3)—.──────────────────────────────┐
│>     1 74edbf8 On ggt-p0-setup: cline checkpoint session=1791711470014_g0jhr │
│      2 7877e12 index on ggt-p0-setup: cc9417a docs: record PM decisions (froz│
│      3 cc9417a docs: record PM decisions (frozen golden schema, PM-owned plan│
│      4 2aad927 ci: add fmt/clippy/test workflow skeleton and P0 report│
│      5 6400450 test(g8): add reference golden-fixture harness + linear & bran│
│      6 59d8688 feat(tui): scaffold cargo binary crate with live commit lista│
│      7 e6330a8 Add .gitignore: plans (.IDE_Plans) and build output stay outo│
│      8 0262f36 Record Q1 decisions: Rust stack, local unit tests, remote-only│
│      9 e4448e3 Initial commit: project scaffold for terminal Git Graph revers│
└──────────────────────────────────────────────────────────────────────────────┘
 branch: ggt-p0-setup  head: cc9417a  commits: 9  panes: j/k/↑/↓ scroll · r relo
```

Real subjects, real hashes, correct branch/head/status bar. Also re-verified the
headless path at HEAD: `./target/debug/git-graph-terminal --dump .` exit 0, same
frame. Degenerate inputs re-run by me from scratch (`/tmp/verify-ggt/*`):
unborn branch -> `branch: main  head: (no commits)  commits: 0`; detached HEAD ->
`branch: (detached HEAD)  head: 182bce9  commits: 1`; non-repo ->
`git-graph-terminal — not a git repository: /tmp/verify-ggt/plain`; subdir cwd
works. No panics. Matches the worker's claims ($3c).

### Item 3 — golden fixtures + FROZEN schema — PASS

Both files exist: `tests/fixtures/golden/linear.json` (3387 B),
`tests/fixtures/golden/branch-merge-rebase.json` (7940 B).

I wrote an independent validator (408 assertions, outside the worktree) and ran
it against the committed files and against a freshly generated second set:

```
$ node /tmp/verify-schema.mjs tests/fixtures/golden ; echo $?
=== branch-merge-rebase.json ===
  top-level keys: fixture, generatedBy, reference, config, repo, rows, rowNodes, branches
  rows=9 branches=3 lanes={0,1,2} colours={0,1,2} sentinelRows=0 merges=1 lines=20 lockedFirstRows=1
=== linear.json ===
  top-level keys: fixture, generatedBy, reference, config, repo, rows, rowNodes, branches
  rows=5 branches=1 lanes={0} colours={0} sentinelRows=0 merges=0 lines=4 lockedFirstRows=0
SCHEMA OK (408 checks)   SCHEMA_EXIT=0
```

What was checked: `rows[]` has exactly the 5 frozen fields
(`row,lane,colour,parents,lockedFirst`) and no others; `row == index`;
`lane int >= -1` (sentinel allowed); `colour in [-1,11]`; `parents int[]` where
`-1` == `NULL_VERTEX_ID` and every other id is inside the graph;
`lockedFirst` bool and equal to the documented projection of the line state;
`lane == -1 <=> colour == -1 <=> rowNodes[i].onBranch == false`; additive blocks
`rowNodes` (4 bools, one `isCurrent` == the row of `repo.commitHead`),
`branches[]` (`colour/end/numUncommitted/lines` with `p1/p2/lockedFirst`),
`reference{}` (64-hex `sha256`, `file == web/graph.ts`), `repo{}`
(`commitCount == rows.length`, `path` relative, 40-hex `commitHashes`), and
`config.colours` == the reference palette.

Independent cross-checks of the fixture content:

```
$ node -e '<map golden rows/parents to git>'      -> linear: true, branch-merge-rebase: true
$ git --git-dir=tests/fixtures/repos/branch-merge-rebase/.git log --all --date-order --format='%h %p %s'
2dea24e 5eb1c27 r2 replanted again
7009c4e 852a99f 88ab47c merge feature into main      <- row 2, parents [3,4], isMerge true
b95a8a7  m1 base                                    <- row 8, root
$ sha256sum ../vscode-git-graph/web/graph.ts
86a667151fcef285213e3059bcf460df5cfb8b77111f0f9c414f57c25850e4e9   <- == reference.sha256 in both files
```

So: row order == `git` commit order, `parents` == real parent links, the pinned
reference hash is the real file. The 3-lane / 1-merge / colour-reuse shape the
README documents is exactly what is in the file.

### Item 4 — documented one-liner from `tools/g8-dump/README.md` — PASS

Ran the README's one-liner verbatim from the project root:

```
$ cd tools/g8-dump && npm install && npm run fixtures && npm run dump && npm run check
npm warn EBADENGINE (4 packages, see D2) ; up to date, audited 38 packages in 6s
> fixtures: keep  linear (already exists, left untouched)
            keep  branch-merge-rebase (already exists, left untouched)
> dump:     wrote tests/fixtures/golden/branch-merge-rebase.json (9 rows, 3 branches)
            wrote tests/fixtures/golden/linear.json (5 rows, 1 branches)
> check:    ok   branch-merge-rebase: matches tests/fixtures/golden/branch-merge-rebase.json
            ok   linear: matches tests/fixtures/golden/linear.json
ONELINER_EXIT=0
```

All four steps pass, `check` prints "ok" twice and exits 0. Byte-identical
before/after (`branch-merge-rebase` sha256 `2a96d1a8…`, `linear` `e518a5c0…`).
Extra determinism proof (README's own cross-check): regenerated a second fixture
set + goldens under `/tmp/verify-g8` and `diff -r` against the committed ones ->
empty diff; the regenerated set also passes all 408 schema checks. So the goldens
are machine-produced, deterministic and not hand-edited.

Caveat: the re-run is not tree-clean — see defect D1.

### Item 5 — git state — PASS

```
worktree:  git rev-parse HEAD ggt-p0-setup development main
           cc9417a…  cc9417a…  e6330a8…  e6330a8…
commits:   git log --oneline development..HEAD   -> 4 commits (59d8688, 6400450, 2aad927, cc9417a)
           git log --oneline HEAD..development   -> (empty)
tree:      git status --porcelain -> empty (checked before any verification run)
worktrees: git worktree list -> git-graph-terminal e6330a8 [main] / git-graph-terminal-p0 cc9417a [ggt-p0-setup]

master checkout (git-graph-terminal):
  HEAD = e6330a8eae681ea30f58ec284445fb9f01a203be == development == main
  git status --porcelain -> 0 entries ; contents: .git .gitignore README.md .IDE_Plans
  git log --all -- .IDE_Plans inside the worktree -> 0 commits (plan never committed/edited)

reference ../vscode-git-graph:
  HEAD d7f43f4 , branch develop
  git status --porcelain -> 0 lines ; git status --porcelain -uall -> 0 lines
  git reflog -> single entry "d7f43f4 HEAD@{0}: clone: from https://github.com/mhutchie/vscode-git-graph.git"
  git stash list -> 0 ; web/graph.ts mtime 12:27 (clone time, untouched by the 14:45+ work)
  first commit for web/graph.ts (4960650) is an upstream commit, not local: no new commits.
```

Plan file untouched: `.IDE_Plans/*` mtimes are 14:10–14:41, i.e. before the
worker's first commit (14:49), and no commit in the repo touches `.IDE_Plans`.
No code was written outside the worktree except the worker's self-disclosed
scratch dir `/tmp/ggt-p0/` (present, contains the PTY harness and the evidence
files the report cites: `pty-evidence.txt`, `edge-cases.txt`, `env.txt`,
`testgate.txt`, `t2-gate.txt`) — non-repo scratch, disclosed in deviation 7.

Conventional commits: the 4 P0 commits are `feat(tui):`, `test(g8):`, `ci:`,
`docs:` — all conventional. The 3 older commits (`Initial commit: …`,
`Record Q1 decisions…`, `Add .gitignore…`) are pre-P0 and not this lane's work.

## Report accuracy spot-check (`p0-report.md`)

Every factual claim I could reproduce held up. Verified independently: tool
versions (§1), rustup install (§2, `/tmp/ggt-p0/rustup-install.log`), Cargo
deps `crossterm 0.29.0` + `ratatui 0.30.2` and edition 2021 (§3), the PTY
frames incl. `commits (3) · branches (3)`, `head: e6330a8`, two resizes and
`child exit status: 0 / panic in output: False` (§3a — evidence file present and
consistent), the degenerate-input behaviour (§3c — I re-ran all four cases),
`jsdom 30.1.2` / `typescript 5.9.3` (§4), `--max-count=1000` (= `MAX_COMMITS+1`,
`lib/git-commits.mjs:53`) and `stash: null` (§7.8, README), the reference pin
`d7f43f4` + `graph.ts` sha256 `86a6671…` (§1), 0 reference git-status entries
(§1), `development` still `e6330a8` and no push (§7.9), `.IDE_Plans` untouched
(§7.10), and the REA statement (§9): the harness only reads `web/graph.ts`
(TypeScript source) and injects values from `src/config.ts` / `src/types.ts`;
grep finds no reference to `media/`, `dist/` or any bundled JS — so "REA not
triggered" is accurate.

Claims that are *not* reproducible from the worktree (disclosed by the worker
himself, so not counted as false claims): the PTY harness script and the raw
frames live in `/tmp/ggt-p0/` (open question 6). I re-derived the behaviour
independently, so the substance is confirmed even though the artifact is outside
the repo.

## Defects

| # | Severity | Item | Detail |
| - | -------- | ---- | ------ |
| D1 | Low | 4 | The documented one-liner is **not tree-clean**: `npm install` rewrites the committed `tools/g8-dump/package-lock.json` (+3 lines, adds the root `engines` block), so `git status --porcelain` goes from empty to ` M tools/g8-dump/package-lock.json` after the PM's mandated re-run. Committed lock `146d3ac` -> working `c52b5a1`: `"devDependencies": {…}, + "engines": { "node": ">=20" }`. No functional impact (`npm ci --dry-run` -> "up to date", exit 0; goldens byte-identical). Fix later by re-committing the lock (or dropping/aligning the `engines` field). |
| D2 | Low | docs | `tools/g8-dump/README.md` (Notes) and `p0-report.md` deviation 6 say only the **transitive** `w3c-xmlserializer@6.0.0` prints `EBADENGINE`. Actual install prints it for **4** packages: `jsdom@30.1.2` (a *direct* devDependency), `@asamuzakjp/css-color@7.1.5`, `@asamuzakjp/dom-selector@9.2.4`, `w3c-xmlserializer@6.0.0` — all want node `^22.22.2` while this machine has `22.22.1`. Warning only; install/dump/check all pass. Documentation understates the scope. |
| D3 | Low | 2 | The TUI data layer uses `git log --all` (`src/data.rs:190`), which loads refs the reference extension does not: `src/dataSource.ts:1510-1527` uses `--branches [--tags] HEAD --`. Observed effect: rows 1–2 of the list are `74edbf8 On ggt-p0-setup: cline checkpoint session=…` and `7877e12 index on ggt-p0-setup: …` from `refs/cline/checkpoints/1791711470014_g0jhr/1` (agent-harness checkpoints created 15:08:44, i.e. **after** the worker's last commit at 15:03:47 — not a worker artifact), and the title/status bar say `commits (9)` while the branch has 7 (`git log --oneline ggt-p0-setup` = 7, `--all` = 9). `refs/stash` would leak in the same way. Acceptance item 2 still passes (the rows are real commits, the status bar is right), but this must be resolved before W1 claims reference parity. |
| D4 | Info | 3 | The frozen `lane: -1` sentinel is documented and schema-valid, but **no delivered fixture exercises it** (`sentinelRows=0` in both files) — nothing pins the "reference draws no node for this row" path that W1 has to port. Adding a fixture that produces it (e.g. a repo whose HEAD ref is outside the loaded range) would harden the D2 gate. |

Not defects (checked, then withdrawn): my first validator flagged
`branches[2]` not keeping one lane column and `commitHashes[0] != commitHead`.
Both were wrong assumptions of *my* checker — the reference legitimately moves a
branch column (`(1,2)->(2,3)`, lane expansion) and HEAD is `7009c4e` while the
newest row is `2dea24e` (`--date-order` across refs). Re-checked against the
real invariants (endpoint anchoring, `isCurrent` == row of `commitHead`,
`isMerge` == multi-parent rows): all pass.

## Verifier side effects on the worktree (disclosure)

* `tools/g8-dump/package-lock.json` is now ` M` — caused by the mandated re-run
  of the documented one-liner (D1), not by the worker.
* `p0-verify.md` (this file) was added, untracked, as instructed.
* `target/`, `tests/fixtures/repos/`, `tools/g8-dump/node_modules/` were
  rebuilt/touched; all three are git-ignored.
* Scratch outside the repo, verifier-owned: `/tmp/verify-schema.mjs`,
  `/tmp/verify-tui.raw`, `/tmp/verify-oneliner.log`, `/tmp/verify-ggt/`,
  `/tmp/verify-g8/`. Nothing was written to `.IDE_Plans/` or to
  `../vscode-git-graph` (reflog still shows only the clone entry).
* To restore the delivered state exactly: `git checkout -- tools/g8-dump/package-lock.json`
  (not executed — verification only, PM's call).

## Verdict

**ACCEPT.** All 5 acceptance items PASS with reproduced evidence; no HIGH or
CRITICAL defect; the four findings are documentation/hygiene-level and none of
them invalidate T1–T4. Recommended (non-blocking) follow-ups: D1 re-commit the
lock, D2 fix the EBADENGINE wording, D3 decide the ref set before the D2 parity
claim, D4 add a fixture that exercises `lane: -1`.

— cline, independent verifier, 2026-10-11
