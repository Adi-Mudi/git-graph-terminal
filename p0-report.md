# P0 report — T1–T4

Plan: `git-graph-terminal/.IDE_Plans/p0-plan_v1.0.md` (lane `p0`, branch `ggt-p0-setup`)
Worktree: `git-graph-terminal-p0` · Worker: pi · Verifier: cline · PM: Hermes

## 0. Status against the plan's acceptance list

| # | Acceptance item | Status | Evidence |
| - | --------------- | ------ | -------- |
| 1 | `cargo build && cargo test` green | DONE | §6 (5/5 unit tests pass) |
| 2 | Binary inside the worktree's git history renders real commits + status bar | DONE | §3 (real PTY run at `e6330a8` + `--dump` frame at the final commit) |
| 3 | Two golden fixtures produced by the documented harness command | DONE | §4, `tools/g8-dump/README.md` |
| 4 | `g8-dump` re-run works from a clean shell | DONE locally; NOT verified by PM yet | §10 |
| 5 | All commits on `ggt-p0-setup`, worktree clean, `development` untouched | DONE | §7 item 9, `git log development..HEAD` |

Task status: T1 DONE · T2 DONE · T3 DONE · T4 DONE (activates only with a GitHub remote).

## 1. Environment (exact)

| tool | version |
| ---- | ------- |
| rustc / cargo | 1.99.0 (b940084d7 2026-09-28) / cargo 1.99.0 (5f94df478 2026-08-27) |
| rustup | 1.29.1 (d95a37b6a 2026-08-13); components: cargo, rust-std, rustc, rustfmt, clippy |
| rustfmt / clippy | 1.10.0-stable / 0.1.99 |
| node / npm | v22.22.1 / 11.19.1 |
| git | 2.53.0 |
| harness deps | typescript 5.9.3, jsdom 30.1.2 |
| reference checkout | `../vscode-git-graph` @ `d7f43f429a9e024e896bac9fc65fdc530935c812` ("#557 Improved context menu click event handling."), `web/graph.ts` sha256 `86a667151fcef285213e3059bcf460df5cfb8b77111f0f9c414f57c25850e4e9` |

The reference checkout's `git status --porcelain` is **empty** (0 entries) — only read, never written.

## 2. T1 — toolchain

```
$ rustup --version
rustup 1.29.1 (d95a37b6a 2026-08-13)
$ cargo --version && rustc --version
cargo 1.99.0 (5f94df478 2026-08-27)
rustc 1.99.0 (b940084d7 2026-09-28)
```

Installed with the plan's official rustup script (`--profile minimal`); `rustfmt` and `clippy`
were additionally added as components because T4's CI runs them and they are absent from the
minimal profile. Nothing was installed system-wide (rustup home only).

## 3. T2 — cargo scaffold

`cargo init --name git-graph-terminal`, edition 2021, deps `ratatui 0.30.2` + `crossterm 0.29.0`
(the only two in `Cargo.toml`), `Cargo.lock` committed.

- `src/main.rs` — terminal setup/teardown (alternate screen + raw mode), event loop (`q` quit,
  `j/k/↑/↓` scroll, `r` reload), status bar (branch, HEAD short hash, commit count, panes hint),
  and `--dump [REPO]` for headless verification (see deviation 1).
- `src/data.rs` — D9 git shell-out: `run_git`, `commits` (NUL/newline-safe `--format`:
  hash, abbrev, parents, author, date, subject, refs), `branches`, `is_repo`; unborn branch and
  non-repository states degrade to an empty list / status-bar message (never a panic).
- `src/layout/mod.rs` — stub with the D8 comment (2 cells/lane, `Cell{glyph,lane,pers}`).
- `src/render/mod.rs` — commit-list rows + status bar wiring.

Smoke behavior — acceptance item 2, verified twice.

(a) Real PTY run (headless pty harness: `python3 pty_capture.py target/debug/git-graph-terminal .`,
script kept outside the repo at `/tmp/ggt-p0/pty_capture.py`, evidence `/tmp/ggt-p0/pty-evidence.txt`)
at commit `e6330a8` (the branch had 3 commits then): initial frame at 100x24 rendered
`commits (3) · branches (3)` plus `branch: ggt-p0-setup  head: e6330a8  commits: 3  panes: j/k/↑/↓ scroll · r reload · q quit`,
then scroll keys `jjjkk`, then two resizes (120x40 and 60x10) re-rendered the frame —
`child exit status: 0`, `panic in output: False` (clean alternate-screen teardown).

(b) Deterministic one-frame dump, run just now on the final commit (5 commits in the branch):

```
$ cargo run -q -- --dump .
┌ commits (5) · branches (3) — . ──────────────────────────────────────────────────────────────────┐
│>     1 6400450 test(g8): add reference golden-fixture harness + linear & branch-merge-rebase fixt│
│      2 59d8688 feat(tui): scaffold cargo binary crate with live commit list and status bar       │
│      3 e6330a8 Add .gitignore: plans (.IDE_Plans) and build output stay out of git  (main, develo│
│      4 0262f36 Record Q1 decisions: Rust stack, local unit tests, remote-only E2E on GitHub Actio│
│      5 e4448e3 Initial commit: project scaffold for terminal Git Graph reverse engineering       │
└──────────────────────────────────────────────────────────────────────────────────────────────────┘
 branch: ggt-p0-setup  head: 6400450  commits: 5  panes: j/k/↑/↓ scroll · r reload · q quit
```

(c) Degenerate inputs (evidence `/tmp/ggt-p0/edge-cases.txt`): unborn branch →
`branch: main  head: (no commits)  commits: 0`, no panic; detached HEAD →
`branch: (detached HEAD)  head: d99539a  commits: 1`; non-repository → status bar
`not a git repository: /tmp/ggt-p0/plain`; run from a sub-directory (`src/render`) works.

Second frame on the fixture repo (`tests/fixtures/repos/branch-merge-rebase`, generated by T3) —
proves the data layer handles a real merge/fork topology: `commits: 9 · branches: 3`,
`branch: main`, merge commit `7009c4e` with ref `HEAD -> main, tag: v2`.

## 4. T3 — G8 harness (`tools/g8-dump/`)

What it does, in one line: fixture repo → reference `GitCommit[]` model (mirrors
`src/dataSource.ts getLog()/getRefs()`) → **reference `Graph.loadCommits()` executed unmodified**
(`web/graph.ts` transpiled in memory with `ts.transpileModule`, run in a jsdom document) →
`tests/fixtures/golden/<fixture>.json`.

Toolchain choice (the plan asked to verify availability and report the choice): **typescript +
jsdom**, not jest+jsdom — no test framework is needed because the harness is a dumper, and
`transpileModule` + `new Function` avoids a bundler entirely. Only `loadCommits()` runs;
`render()`/`draw()` are never called (they need a real layout engine and are not part of the
topology contract). `Graph.determinePath()` runs implicitly inside `loadCommits()`.

Commands and outputs:

```
$ cd tools/g8-dump && npm ci
(npm warn EBADENGINE ×5 on node 22.22.1 — jsdom@30.1.2, @asamuzakjp/css-color@7.1.5,
 @asamuzakjp/dom-selector@9.2.4, w3c-xmlserializer@6.0.0 and g8-dump@0.1.0 itself;
 all want ^22.22.2 || ^24.15.0 || >=26.0.0 — see deviation 6 and §11.2)
$ npm ls --depth=0
g8-dump@0.1.0 .../tools/g8-dump
├── jsdom@30.1.2
└── typescript@5.9.3
(37 packages in package-lock.json, node_modules/ git-ignored)
$ npm run fixtures              # bash make-fixtures.sh
fixture repositories under .../tests/fixtures/repos: branch-merge-rebase, linear
$ npm run dump                  # node g8-dump.mjs --all
wrote tests/fixtures/golden/branch-merge-rebase.json (9 rows, 3 branches)
wrote tests/fixtures/golden/linear.json (5 rows, 1 branches)
$ npm run check                 # node g8-dump.mjs --all --check
ok   branch-merge-rebase: matches tests/fixtures/golden/branch-merge-rebase.json
ok   linear: matches tests/fixtures/golden/linear.json
```

### 4.1 Golden data (the D2 parity target)

`linear` — 5 rows, 1 branch, head `65e070e`, 1 lane, colour index 0 everywhere:

| row | lane | colour | lockedFirst | parents | flags |
| --- | ---- | ------ | ----------- | ------- | ----- |
| 0 | 0 | 0 | false | [1] | isCurrent, isCommitted |
| 1..3 | 0 | 0 | false | [2] [3] [4] | isCommitted |
| 4 | 0 | 0 | false | [] | isCommitted |

`branch-merge-rebase` — 9 rows, 3 branches, head `7009c4e`:

| row | lane | colour | lockedFirst | parents | note |
| --- | ---- | ------ | ----------- | ------- | ---- |
| 0 | 0 | 0 | false | [1] | r2 (fork tip) |
| 1 | 0 | 0 | false | [8] | r1 |
| 2 | 1 | 1 | **true** | [3, 4] | merge commit (HEAD), first parent on lane 1 |
| 3 | 1 | 1 | false | [6] | m4 |
| 4 | 2 | 2 | false | [5] | f2 |
| 5 | 2 | 2 | false | [8] | f1 |
| 6 | 1 | 1 | false | [7] | m3 |
| 7 | 1 | 1 | false | [8] | m2 |
| 8 | 0 | 0 | false | [] | m1 (root) |

Branch line geometry is in the same file (`branches[].lines`, graph space), e.g. branch 1 ends
with `(1,7)->(0,8)` and branch 2 starts with the lock `(1,2)->(2,3)` marked `lockedFirst`.

### 4.2 Determinism proof (fixtures regenerate to byte-identical goldens)

```
$ bash make-fixtures.sh /tmp/ggt-p0/regen2/repos
$ node g8-dump.mjs --all --repos /tmp/ggt-p0/regen2/repos --out /tmp/ggt-p0/regen2/golden
$ diff -r ../../tests/fixtures/golden /tmp/ggt-p0/regen2/golden     # empty
DETERMINISM_OK byte-identical
2a96d1a8...  branch-merge-rebase.json   (identical in both trees)
e518a5c0...  linear.json                (identical in both trees)
```

Cause: every fixture commit pins author/committer name, email and timestamp, pins the default
branch to `main`, disables GPG signing and `core.autocrlf`. `--check` re-derives the goldens and
fails on drift, so the committed files are reproducible.

## 5. T4 — CI skeleton

`.github/workflows/ci.yml`: `on: push (all branches) / pull_request / workflow_dispatch`,
`ubuntu-latest`, steps = checkout → `dtolnay/rust-toolchain@stable` (rustfmt + clippy) → cargo
cache → the plan's exact three commands `cargo fmt --check`, `cargo clippy -- -D warnings`,
`cargo test`. YAML parses (1 job, 6 steps).

**Not active yet**: there is no GitHub remote/repository for this project. Per the plan this is an
escalation item, not ours. The G8 harness is deliberately not in CI (it needs the read-only
reference checkout, which is not a submodule); the file has a comment recording the M1 hook.

## 6. Test results (test gate)

```
$ cargo fmt --check                     -> OK (no diff)
$ cargo clippy --all-targets -- -D warnings -> OK, exit 0
$ cargo build                           -> OK
$ cargo test                            -> 5 passed; 0 failed; 0 ignored
  data::tests::parses_two_commits_with_parents_and_refs ... ok
  data::tests::skips_malformed_and_accepts_empty_payload ... ok
  data::tests::subject_with_newline_cannot_break_records ... ok
  data::tests::non_repository_is_an_error_not_a_panic ... ok
  data::tests::unborn_branch_yields_no_commits ... ok
```

Harness-side check: `node --check` passes on all 4 `.mjs` files, `bash -n` on `make-fixtures.sh`,
`json.load` on `package.json`, `package-lock.json` and both golden files, plus the `--check`
idempotency run in §4. On the Rust side the TUI was additionally driven through a real PTY
(scroll + two resizes + clean exit, no panic) and against degenerate inputs (unborn branch,
detached HEAD, non-repository, sub-directory) — see §3. There is no Rust integration-test
directory yet (`tests/` holds fixtures).

## 7. Deviations, with reasons

1. **`--dump` flag added to the binary** (T2). The plan's smoke behavior assumes a TTY. Both paths
   were executed: a real headless-PTY run (initial frame + scroll + two resizes + clean exit, no
   panic) and `--dump` for a deterministic, greppable frame. The PTY harness script that produced
   the first evidence lives outside the repo (`/tmp/ggt-p0/pty_capture.py`), so by itself it is not
   reproducible from the worktree — see open question 6.
2. **Extra files beyond the literal T3 file list**: `tools/g8-dump/README.md` (the plan demands it),
   `package-lock.json` (npm install artifact, committed for reproducibility), and 5 new
   `.gitignore` entries (`tests/fixtures/repos/`, `tools/g8-dump/node_modules/`).
3. **Golden schema is additive**: `rows` matches the plan contract field-for-field; `rowNodes[]`,
   `branches[]`, `reference{}` and `repo{}` were added so W1 can assert line routing and so a drift
   is readable in review. `rows` never changes shape.
4. **Two documented sentinels in `rows`**: `lane: -1` when the reference draws no node for a row
   (`Vertex.draw()` returns early when `onBranch === null`), and `lockedFirst = (line.p1.y === row &&
   line.lockedFirst)` widened to `false` for rows without an outgoing line. Both are documented in
   the README output contract; the plan only said `int`/`bool`.
5. **`repo.path` in the golden file is the fixed label `tests/fixtures/repos/<name>`**, not the
   absolute path — otherwise the golden file differs per machine and `--check` is useless.
6. **npm emits `EBADENGINE`** for **4 packages** on node 22.22.1 — the direct devDependency
   `jsdom@30.1.2`, its helpers `@asamuzakjp/css-color@7.1.5` and
   `@asamuzakjp/dom-selector@9.2.4`, and the transitive `w3c-xmlserializer@6.0.0`; all want
   `^22.22.2 || ^24.15.0 || >=26.0.0`. (This deviation originally named only
   `w3c-xmlserializer`; corrected in §11.2.) Warning only: install and dumps work. CI/node
   should be ≥ 22.22.2.
7. **Scratch files were written under `/tmp/ggt-p0/`** (regenerated fixture copies, PTY harness +
   captured frames, edge-case and gate logs). This is outside the worktree and the plan says to work
   only inside it — disclosed deliberately. Nothing under `/tmp` is referenced by the committed
   harness; every command in §10 works without it.
8. **No stash support / no "uncommitted changes" row** in the harness (the reference's row for them
   is not modelled: `stash: null`, `--max-count=1000`). Not required by the plan; it must be closed
   before the D2 parity gate can claim full parity.
9. **`development` was not modified** and no push happened: `git log --oneline development..HEAD`
   lists only this branch's commits (T2 scaffold, T3 harness, T4 CI + report, decisions record);
   `development` still points at `e6330a8`. Worktree is clean after the final commit.
10. **`.IDE_Plans/` was not edited** — the plan's own rules say "never edit `.IDE_Plans/`", which
    conflicts with the global rule that the worker updates plan status. Status therefore lives here.

## 8. Open questions / escalations

**Resolved by PM, 2026-10-11:**

1. **Plan status file** — the PM/verifier owns `.IDE_Plans/p0-plan_v1.0.md`; the worker does not
   edit it (deviation 10 stands). Status stays in this report.
2. **Golden schema** — deviations 3–5 (additive fields, `lane: -1` sentinel, `lockedFirst` row
   projection, relative `repo.path`) are **accepted and frozen** as the D2 contract W1 ports
   against; recorded in `tools/g8-dump/README.md`.
3. **GitHub repository + remote** — stays dormant; escalation item 2 (money/account). T4's CI stays
   committed but inactive until the user creates the repo, and remote-only E2E waits with it.

**Still open:**

4. **Fixture scope for M1** — is extending the harness to stashes, an unborn-branch repo and a
   ~30k-commit perf fixture part of the G8/D2 task, or a separate M1 task?
5. **CI reference pinning** — for M1 should the reference checkout be vendored as a pinned submodule
   (so the parity job can run in CI), or stay a local-only harness?
6. **PTY smoke harness** — should I commit the headless-PTY script (currently `/tmp/ggt-p0/
   pty_capture.py`, ~100 lines, no deps) as e.g. `tools/smoke/pty_capture.py` so acceptance item 2 is
   reproducible from the worktree? It is outside the T1–T4 contract, so I did not add it.

## 9. REA statement

REA was **not** triggered. The plan makes REA mandatory only if compiled/shipped JS is analyzed
during T3. The harness analyzes the reference's **TypeScript source** (`web/graph.ts`, plus
`src/dataSource.ts` / `src/config.ts` / `src/types.ts` read for the `GitCommit` shape and palette).
No compiled or shipped artifact (e.g. the extension's bundled `media/graph.js`) was read or
executed, so there are no REA evidence ids to report. The reference revision that was executed is
pinned inside every golden file as `reference.sha256`.

## 10. Reproduce from scratch (clean shell)

```bash
cd /mnt/Just_Do_It/02_Devp_Soft/01_Adi-Mudi/11_Git_Graph/git-graph-terminal-p0

# Rust side (acceptance 1, 2)
export PATH="$HOME/.cargo/bin:$PATH"
cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo build && cargo test
cargo run -q -- --dump .

# G8 harness (acceptance 3, 4) — needs network (npm) + the reference at ../../../vscode-git-graph
cd tools/g8-dump
npm ci
npm run fixtures
npm run dump
npm run check          # must print "ok" twice and exit 0
```

Fixture repositories are generated (git-ignored); the golden JSON files are committed and are the
reviewable artifact. Full harness documentation: `tools/g8-dump/README.md`. Use `npm ci`, not
`npm install`: only `npm ci` installs the committed lockfile exactly and leaves the tree clean
(§11.2, defect D1).

## 11. P0.1 addendum — defect fixes after independent verification

The independent verification (`p0-verify.md`, verifier `cline`, verdict ACCEPT with defects
D1–D4) found three defects in this report's scope. They are fixed here; **D4** (no fixture
exercises the `lane: -1` sentinel) is deliberately **not** touched — it needs a new fixture, i.e.
a change to the frozen golden set, and the verifier itself recommended it as a W1 task.

Commits on `ggt-p0-setup`, no push: `9233abc` (D3), `2ec4a79` (D1 + D2).

### 11.1 D3 — `src/data.rs` selected refs the reference never shows (fixed)

**What changed.** `commits()` used `git log --all`. `--all` walks *every* ref, including
`refs/stash` and refs outside `refs/heads`, `refs/tags`, `refs/remotes` — in this worktree the
agent harness's `refs/cline/checkpoints/1791711470014_g0jhr/1` added 2 commits that the branch
does not have. The invocation now mirrors the reference `getLog()` "Show All" branch
(`src/dataSource.ts:1509-1529`) with the extension's default settings
(`showRemoteBranches` = true, `showTags` = true, `includeCommitsMentionedByReflogs` = false,
`onlyFollowFirstParent` = false):

| | invocation |
| --- | --- |
| before | `git -c log.showSignature=false log --all --date-order -z <fmt>` |
| after | `git -c log.showSignature=false log --date-order -z <fmt> --branches --tags --remotes HEAD --` |

`--tags` and `--remotes` are included because those are the reference's **defaults**
(`git-graph.repository.showTags` / `showRemoteBranches` = `true` in `package.json`); the reflog
flag and the stash base hashes are omitted because P0 has no reflog/stash model.

**Evidence (this worktree, branch `ggt-p0-setup`).** Before/after `--dump` at `cc9417a` (the
pre-fix tip, 7 branch commits), then again at the fix commits (9 branch commits):

```
$ git log --oneline ggt-p0-setup | wc -l   -> 7    (at cc9417a, before the fix)
$ git log --all --oneline | wc -l          -> 9    (2 extra from refs/cline/checkpoints/...)
$ cargo run -q -- --dump .   (before)  -> title "commits (9)", rows 1-2 = 74edbf8 / 7877e12
$ cargo run -q -- --dump .   (after)   -> title "commits (7)", row 1 = cc9417a docs: record PM decisions ...
$ cargo run -q -- --dump tests/fixtures/repos/branch-merge-rebase  -> still "commits (9)" (unchanged)

# at the two fix commits (9233abc, 2ec4a79) the branch has 9 commits, --all still 11:
$ git log --oneline ggt-p0-setup | wc -l   -> 9
$ git log --all --oneline | wc -l          -> 11
$ cargo run -q -- --dump .   -> title "commits (9)", rows 1-2 = 2ec4a79 / 9233abc,
                                grep -c 'cline checkpoint|index on ggt-p0-setup' -> 0

$ cargo test                            -> 7 passed; 0 failed (5 existing + 2 new)
$ cargo fmt --check ; cargo clippy --all-targets -- -D warnings   -> exit 0
```

New unit tests: `data::tests::stray_refs_and_stashes_are_not_shown` (creates
`refs/cline/checkpoints/...` and `refs/stash` in a scratch repo, proves `--all` reaches both, then
asserts `commits()` returns only the branch commits) and
`data::tests::tag_and_remote_only_commits_are_shown` (tag-only and remote-only commits are still
shown). The golden fixtures are untouched: `npm run check` prints `ok` twice.

**Flag differences found against `getLog()` (report only — not changed here).**

| # | `getLog()` (defaults) | ours | note |
| --- | --- | --- | --- |
| 1 | `--max-count=<num>` always passed; the view calls it with `initialLoadCommits + 1` = **301** (default 300), then pops the extra row to set `moreCommitsAvailable` | no `--max-count` | our list is unbounded; a 30k-commit repo loads everything (G5/M1 concern) |
| 2 | splits stdout on `EOL_REGEX`, fields on `GIT_LOG_SEPARATOR` (6 fields) | `-z` + NUL records, 7 fields (`%D` refs added) | deliberate: newlines in a subject cannot break records; the reference's own split would. Keep. |
| 3 | `--first-parent` only when `onlyFollowFirstParent` (default false) | not passed | matches default |
| 4 | `--reflog` only when `includeCommitsMentionedByReflogs` (default false) | not passed | matches default |
| 5 | with hidden remotes: one `--glob=refs/remotes/<remote>` per visible remote instead of `--remotes` | always `--remotes` | no remote filter UI in P0 |
| 6 | appends each distinct stash `baseHash` before `HEAD`, so commits reachable only from a stash still appear | none | P0 has no stash model; documented deviation |
| 7 | trailing `--` | trailing `--` (added) | options now precede it, as `git` requires |
| 8 | `-c log.showSignature=false`, `--date-order` (`--<commitOrdering>-order`, default `date`), `HEAD` | same | parity |

### 11.2 D1 + D2 — `tools/g8-dump/` install docs (fixed)

**D1 (tree not clean).** Every documented command now uses `npm ci`: the exact re-run block, the
one-liner, §10 above and the README. `npm install` had rewritten the committed
`package-lock.json` (it re-added the root `engines` block once `engines` existed in
`package.json`), so the mandated re-run left ` M tools/g8-dump/package-lock.json` behind. The lock
is re-committed and now matches `package.json` exactly: `npm install --package-lock-only` produces
**no** further diff.

```
$ rm -rf tools/g8-dump/node_modules && cd tools/g8-dump && npm ci   -> exit 0, "added 37 packages"
$ npm install --package-lock-only ; git diff --stat                -> only the engines line, no further rewrite
$ cd tools/g8-dump && npm ci && npm run fixtures && npm run dump && npm run check
                                                                   -> exit 0; "ok" for both fixtures
$ git status --porcelain                                           -> empty (tree clean)
```

**D2 (warnings understated).** The README Requirements/Notes and deviation 6 above now name all
**4** packages that warn on node 22.22.1 — `jsdom@30.1.2` (a *direct* devDependency),
`@asamuzakjp/css-color@7.1.5`, `@asamuzakjp/dom-selector@9.2.4`, `w3c-xmlserializer@6.0.0` — all
declaring `node: ^22.22.2 || ^24.15.0 || >=26.0.0`. No version was pinned or downgraded: the
goldens were produced with `jsdom@30.1.2`, so changing it would invalidate the parity evidence,
and no pin removes the warning anyway (the requirement comes from jsdom's own `engines`). Instead
`tools/g8-dump/package.json` now declares the same range as jsdom, which makes the requirement
explicit — and on this machine (22.22.1) `npm ci` therefore prints **5** `EBADENGINE` warnings:
those 4 packages plus `g8-dump@0.1.0` itself. They are warnings only:

```
$ grep -c 'EBADENGINE Unsupported engine' <npm ci log>   -> 5
$ npm ci ; npm run fixtures ; npm run dump ; npm run check  -> all exit 0, goldens byte-identical
```

Remedy documented in the README: node ≥ 22.22.2 (or ≥ 24.15.0, or ≥ 26.0.0) clears every warning;
CI should pin such a version.
