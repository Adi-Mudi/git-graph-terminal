# P0.1 delta verification — D1/D2/D3 fixes (independent, evidence-based)

Verifier: cline (independent) · PM: Hermes · Worker: pi
Date: 2026-10-11 · Scope: **only the P0.1 delta** (D1–D3); round-1 results stay in `p0-verify.md`.

Objects

| Object | Value |
| ------ | ----- |
| Worktree / branch | `git-graph-terminal-p0` @ `ggt-p0-setup` = **`fe3e6fc`** |
| Delta commits | `9233abc` fix(data) · `2ec4a79` docs(g8) · `a8335b8` docs(report) · `fe3e6fc` docs(verify, my report) |
| Reference (read-only) | `../vscode-git-graph` @ `d7f43f4` |
| Delta files | `src/data.rs` (+122), `tools/g8-dump/README.md` (24 ±), `tools/g8-dump/package.json`, `tools/g8-dump/package-lock.json` (+3), `p0-report.md` §11 |

**Verdict: ACCEPT** — D3 parity achieved for the extension's default settings; D1 and D2
fixed and reproduced; all gates green. 3 new findings, all **Info** (documentation /
out-of-P0-contract divergence), 0 blocking.

## Item 1 — D3: ref selection parity with reference `getLog()` — PASS

Reference `src/dataSource.ts:1509-1529` ("Show All" arm), verbatim:

```
1509:            // Show All
1510:            args.push('--branches');
1511:            if (includeTags) args.push('--tags');
1512:            if (includeCommitsMentionedByReflogs) args.push('--reflog');
1513:            if (includeRemotes) {
1514:                if (hideRemotes.length === 0) {
1515:                    args.push('--remotes');
1516:                } else {
1517:                    remotes.filter((remote) => !hideRemotes.includes(remote)).forEach((remote) => {
1518:                        args.push('--glob=refs/remotes/' + remote);
1519:                    });
1520:                }
1521:            }
1522:
1523:            // Add the unique list of base hashes of stashes, so that commits only referenced by stashes are displayed
1524:            const stashBaseHashes = stashes.map((stash) => stash.baseHash);
1525:            stashBaseHashes.filter((hash, index) => stashBaseHashes.indexOf(hash) === index).forEach((hash) => args.push(hash));
1527:            args.push('HEAD');
1529:        args.push('--');
```
(1500 prefix: `['-c','log.showSignature=false','log','--max-count='+num,'--format='+gitFormatLog,'--'+order+'-order']`,
1501-1502 optional `--first-parent`; `--tags` is gated by
`showTags && config.showCommitsOnlyReferencedByTags` at the call site `dataSource.ts:167`.)

Ours, `src/data.rs:199-215`, verbatim:

```
199:     let out = run_git(
200:         repo,
201:         &[
202:             "-c",
203:             "log.showSignature=false",
204:             "log",
205:             "--date-order",
206:             "-z",
207:             LOG_FORMAT,
208:             "--branches",
209:             "--tags",
210:             "--remotes",
211:             "HEAD",
212:             // Options must precede this separator; everything after it is a path.
213:             "--",
214:         ],
215:     )?;
```

Which flags are conditional, and the defaults I measured in the reference:

| reference flag | condition | default (`src/config.ts`) | ours | parity |
| -------------- | --------- | ------------------------- | ---- | ------ |
| `--branches` | always in the Show All arm | n/a | present (`:208`) | **✓** |
| `--tags` | `showTags && showCommitsOnlyReferencedByTags` | `true` (`:485`) && `true` (`:457`) | present (`:209`) | **✓** default |
| `--remotes` | `showRemoteBranches` | `true` (`:464`) | present (`:210`) | **✓** default |
| `--reflog` | `includeCommitsMentionedByReflogs` | `false` (`:429`) | absent | **✓** default |
| `--first-parent` | `onlyFollowFirstParent` | `false` (`:450`) | absent | **✓** default |
| `HEAD --` | always | n/a | present (`:211`,`:213`) | **✓** |
| `--date-order` | `--<commitOrdering>-order` | `date` (`:396`) | `--date-order` (`:205`) | **✓** default |
| `-c log.showSignature=false` | always | n/a | present (`:202-203`) | **✓** |
| `--all` | **never** | — | removed | **✓** (the defect) |

**Verdict: parity achieved** for the extension's default settings — identical flag set,
same order, options before the trailing `--`, no `--all`. Remaining divergences are
settings-dependent or documented P0 scope limits (below), none of them reachable with
default settings except the stash/max-count ones.

### Remaining divergences (with line refs) — all deliberate, none is a regression

| # | `getLog()` (ref) | ours (`src/data.rs`) | impact | severity |
| - | ---------------- | -------------------- | ------ | -------- |
| 1 | `--max-count=<num>` always, view passes `initialLoadCommits + 1` = **301** (`config.ts:364` default 300, `dataSource.ts:1500`, popped at `:170`) | none | unbounded load; a 30k-commit repo loads everything (G5/M1 perf work) | Info |
| 2 | 6 fields on `GIT_LOG_SEPARATOR`, stdout split on `EOL_REGEX` (`:1521+`) | `-z` NUL records, 7 fields incl. `%D` (`:206-207`) | deliberate: subjects with newlines cannot break records (reference's own split would) | Info |
| 3 | hidden remotes: one `--glob=refs/remotes/<r>` per visible remote (`:1517-1518`) | always `--remotes` (`:210`) | only when the user configures `hideRemotes`; no remote-filter UI in P0 | Info |
| 4 | distinct stash `baseHash`es appended before `HEAD` (`:1523-1525`) | none | commits reachable *only* from a stash base are not shown; documented (README Notes, addendum §11.1) | Info |
| 5 | `branches !== null` arm — explicit branch list (`:1504-1506`) | always the Show All arm | no branch-selection UI in P0 | Info |
| 6 | `--first-parent` / `--reflog` under non-default settings (`:1501-1502`, `:1512`) | absent | only with non-default settings | Info |

`p0-report.md` §11.1 already documents 8 of these rows (including #1, #3, #4) — I
re-derived them independently and they match, except the two nits N2/N3 below.

### `cargo test` — 7/7, exit 0

```
$ export PATH="$HOME/.cargo/bin:$PATH"; cargo test
running 7 tests
test data::tests::parses_two_commits_with_parents_and_refs ... ok
test data::tests::skips_malformed_and_accepts_empty_payload ... ok
test data::tests::subject_with_newline_cannot_break_records ... ok
test data::tests::non_repository_is_an_error_not_a_panic ... ok
test data::tests::unborn_branch_yields_no_commits ... ok
test data::tests::stray_refs_and_stashes_are_not_shown ... ok     <- new
test data::tests::tag_and_remote_only_commits_are_shown ... ok    <- new
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
TEST_EXIT=0
```
(5 pre-existing + exactly the 2 expected new tests.) No scratch dirs left behind:
`ls -d /tmp/ggt-p0-*` -> nothing.

### Would the regression test fail against the old `--all` behaviour? — YES (reasoned + measured)

From the test code (`src/data.rs:356-387`): `temp_repo("stray")` builds a 2-commit repo
and `commits_expected = [tip, root]` from plain `git log --format=%H`. Then two
`commit-tree` dangling commits are attached to `refs/cline/checkpoints/s1/1` and
`refs/stash`. The test asserts as a **precondition** that `git log --all` reaches both
(`:367-375`) — i.e. old code would return 4 hashes, not 2 — and then requires
`assert_eq!(got, commits_expected)` plus `!got.contains(stray)` / `!got.contains(stashed)`.
With `--all` both the length and the content assertions fail, so the test is a genuine
guard, not a tautology.

Measured with the identical construction (`/tmp/p01-stray`, same `commit-tree` +
`update-ref` sequence as the test):

```
$ git log --format='%h %s' --all                       # OLD behaviour
303d923 checkpoint, not on any branch
2ac0484 c2 tip
071a1bb remote branch only
dcc1966 stash-only commit
ca62305 tag only
2a4bd63 c1 root
$ git log --format='%h %s' --branches --tags --remotes HEAD --     # NEW behaviour
2ac0484 c2 tip
ca62305 tag only                    <- tag-only still shown
071a1bb remote branch only          <- remote-only still shown
2a4bd63 c1 root
counts: old(--all)=6  new=4

checkpoint   old(--all)=yes  new=NO     <- the D3 leak, gone
stash        old(--all)=yes  new=NO     <- refs/stash, gone
tag-only     old(--all)=yes  new=yes    <- not over-fixed
remote-only  old(--all)=yes  new=yes    <- not over-fixed
```

### End-to-end: the real worktree repo (which *has* a `refs/cline/checkpoints/…` ref)

```
$ git for-each-ref --format='%(refname)'
refs/cline/checkpoints/1791711470014_g0jhr/1
refs/heads/development   refs/heads/ggt-p0-setup   refs/heads/main

$ git log --format=%H --all | wc -l                              -> 13
$ git log --format=%H --branches --tags --remotes HEAD -- | wc -l -> 11
$ comm -23 <(--all) <(new)     -> exactly the 2 checkpoint commits (74edbf8, 7877e12)
$ comm -13 <(--all) <(new)     -> 0   (nothing lost)

$ ./target/debug/git-graph-terminal --dump .
┌ commits (11) · branches (3) — . ─────────────────────────────────────────────────────────────────┐
│>     1 fe3e6fc docs(verify): independent P0 verification report (cline-p0, verdict ACCEPT)  (HEAD│
│      2 a8335b8 docs(report): P0.1 addendum for D1-D3 (D3 ref selection, D1 npm ci, D2 engine fact│
...
 branch: ggt-p0-setup  head: fe3e6fc  commits: 11  panes: j/k/↑/↓ scroll · r reload · q quit
```

`commits: 11`, the two `cline checkpoint` / `index on …` rows are gone (round 1 showed
`commits (9)` = 7 branch + 2 checkpoint). **D3 is fixed end-to-end, not just in unit tests.**

## Item 2 — D1: rewritten one-liner leaves the tree clean — PASS

Ran the README's new one-liner (now `npm ci`) verbatim:

```
$ cd tools/g8-dump && npm ci && npm run fixtures && npm run dump && npm run check
added 37 packages, and audited 38 packages in 4s                (npm ci, exit 0)
> fixtures: keep linear / keep branch-merge-rebase (already exist, untouched)
> dump:     wrote tests/fixtures/golden/branch-merge-rebase.json (9 rows, 3 branches)
            wrote tests/fixtures/golden/linear.json (5 rows, 1 branches)
> check:    ok   branch-merge-rebase: matches tests/fixtures/golden/branch-merge-rebase.json
            ok   linear: matches tests/fixtures/golden/linear.json
ONELINER_EXIT=0

$ git status --short   -> (no output)          porcelain_lines=0   <- CLEAN
$ git diff --stat -- tools/g8-dump/package-lock.json -> (empty)     <- lock untouched
package-lock.json sha256 bbc5dc3fe380f40b481b5cd18000937a0b3de63749274836086661dc0ee45145
goldens unchanged vs round 1: branch-merge-rebase 2a96d1a8…, linear e518a5c0…
```

D1 closed: the post-run tree is clean (round 1 left ` M package-lock.json`). The lock now
carries the root `engines` block that matches `package.json`
(`^22.22.2 || ^24.15.0 || >=26.0.0`), so `npm ci` has nothing to rewrite — and `npm ci`
itself fails fast if lock and manifest disagree, so its exit 0 is also evidence of sync.
I deliberately did **not** run `npm install` (the documented cause of D1).

## Item 3 — D2: EBADENGINE documentation now accurate — PASS

```
$ grep -c 'EBADENGINE Unsupported engine' /tmp/p01-oneliner.log   -> 5
packages named: jsdom@30.1.2, @asamuzakjp/css-color@7.1.5,
                @asamuzakjp/dom-selector@9.2.4, w3c-xmlserializer@6.0.0, g8-dump@0.1.0
```

| Source | says | measured | match |
| ------ | ---- | -------- | ----- |
| `tools/g8-dump/README.md:118-122` | the 4 packages (jsdom *direct* + 2 helpers + transitive w3c-xmlserializer), all `^22.22.2 \|\| ^24.15.0 \|\| >=26.0.0`, **plus `g8-dump@0.1.0` itself → 5 warnings in total** | exactly those 4 + `g8-dump@0.1.0`, identical range, 5 warnings | **PASS** |
| `README.md:22-24` | node `^22.22.2 \|\| ^24.15.0 \|\| >=26.0.0`; "developed on v22.22.1 — one patch below the required 22.22.2" | machine v22.22.1; requirement strings identical to npm's `required` | **PASS** |
| `p0-report.md` deviation 6 (rewritten) + §11.2 | 4 packages named, same range | matches | **PASS** |

D2 closed — the docs are exact now, not understated. The "remedy: node ≥ 22.22.2" line
follows directly from the `required` strings npm printed.

## Item 4 — gates — PASS

```
$ cargo fmt --check                                   -> FMT_EXIT=0
$ cargo clippy -- -D warnings                         -> CLIPPY_EXIT=0   (the CI invocation)
$ cargo clippy --all-targets -- -D warnings           -> CLIPPY_ALL_TARGETS_EXIT=0
$ cargo test                                          -> TEST_EXIT=0 (7/7)
```

## Report addendum accuracy (`p0-report.md` §11)

Spot-checked, all reproduced: the before/after invocation table (§11.1), the
`--tags`/`--remotes` default justification, the new test names and their purpose, the
`npm ci` transcript and "tree clean" line (§11.2), the 5-warning EBADENGINE count
(§11.2), and the 8-row divergence table (I re-derived every row from
`dataSource.ts:1500-1529` + `config.ts` defaults — all correct, incl. the `301`
`--max-count` figure: `config.ts:364` default 300 + the `+ 1` at `dataSource.ts:167`).

Two imprecisions (N2, N3 below) — neither is a false claim about behaviour.

## New findings

| # | Severity | Finding |
| - | -------- | ------- |
| N1 | Info | **Remaining divergence, not fixed:** no `--max-count`, so the view loads every commit; the reference passes `initialLoadCommits + 1` = **301** by default (`src/dataSource.ts:1500` + `:167`, `src/config.ts:364`) vs `src/data.rs:199-215`. Documented by the worker in §11.1 row 1 and in the README; matters only for large repos (G5/M1 perf scope). |
| N2 | Info | Doc slip in `src/data.rs:184-185`: "the reflog flag and the stash base hashes are omitted because P0 does not model stashes". The `--reflog` omission is actually driven by the default `includeCommitsMentionedByReflogs = false` (`src/config.ts:429`), not by the stash model. Cosmetic comment error; the code is right. |
| N3 | Info | `p0-report.md:331-333` says "at the two fix commits (`9233abc`, `2ec4a79`) the branch has 9 commits, `--all` still 11". True at `2ec4a79`, but at `9233abc` (the D3 fix) it is 8 branch / 10 with `--all`. Stale-numbers wording in a historical evidence block; the pattern I measured at the tip (11 branch / 13 `--all` / TUI `commits: 11`) is consistent with the claim. |

No new defect affects the D1–D3 fixes. Round-1 defect **D4** (`lane: -1` sentinel has no
fixture) is untouched **by design** — it changes the frozen golden set and is W1 scope;
the addendum states this openly (§11, opening paragraph).

## Verdict

**ACCEPT.** D3 parity with the reference's ref selection is achieved for the extension's
default settings and proven end-to-end (the real checkpoint ref in this worktree no longer
appears); the regression test genuinely fails against the old `--all` invocation (measured,
not assumed); D1 and D2 are fixed and reproduced. 3 new findings, all Info, 0 blocking.

Not re-verified (outside the delta): the unborn-HEAD-with-other-branches edge case in
`commits()` (pre-existing guard, unchanged by this patch) and the historical PTY numbers.

— cline, independent verifier, 2026-10-11
