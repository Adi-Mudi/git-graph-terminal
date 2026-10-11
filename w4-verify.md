# M2-W4 wave verification — interactions core (keyboard nav, selection, scroll, expand)

Verifier: cline-w4 (independent) · PM: Hermes · Date: 2026-10-12
Worktree: `git-graph-terminal-w4` · Branch: `ggt-m2-w4` · Commit `ff4e192`
Plan: `.IDE_Plans/w4-plan_v1.0.md` · Contract: `.IDE_Plans/g7-checklist.md` (R1–R6)
Worker report: `.IDE_Plans/w4-report.md` (lives in the master checkout, outside this worktree)
Toolchain: cargo 1.99.0 / rustc 1.99.0 · PATH=`$HOME/.cargo/bin`

> Method: read-only verification. Every claim below carries a command/evidence. No fixes
> were made. `.IDE_Plans/`, the reference repo and worker files were not edited.
>
> Approval pointer (PM): `.IDE_Plans/pm-autoanswers-log.md` (master checkout) now records
> the W4 dialog answers durably — round A (6) + round B (3). Round B fixes the `g`/`G` ->
> **Home/End-only** decision and the `action_for(key)` signature; the original TUI transcript
> pane was closed, so that log is the authoritative record (see "Approval cross-check" below).

## Verdict summary

| # | Item | Result |
| --- | --- | --- |
| 1 | Full test gate (fmt / clippy / test 217) | **PASS** |
| 2 | Contract compliance (AppState/Action/reduce/keymap, R1.2, C04, aliases, Home/End, gap=4, scroll-follow) | **PASS** |
| 3 | Behavior evidence (pty/tmux per-key) | **PASS** |
| 4 | Reference fidelity spot-check (C03/C04/C06) | **PASS** |
| 5 | Scope guard (no menus/dialogs/find, data.rs untouched) | **PASS** |
| 6 | Deviations judged | **PASS (low-severity only)** |

**Verdict: ACCEPT** · Functional defects: 0 · Low/cosmetic defects: 2 (documentation only; D-2 resolved by the PM approval log)

---

## Item 1 — Full test gate: PASS

Evidence (all run in the worktree, `PATH=$HOME/.cargo/bin`):

```
cargo fmt --check                              -> FMT_EXIT=0 (clean)
cargo clippy --all-targets -- -D warnings      -> CLIPPY_EXIT=0 (0 warnings)
cargo test                                     -> TEST_EXIT=0
```

`cargo test` result lines:

| Target | Result |
| --- | --- |
| unittests src/main.rs (app+keymap+render+data+layout) | 69 passed; 0 failed |
| tests/app_state.rs | 43 passed; 0 failed |
| tests/golden_layout.rs | 15 passed; 0 failed |
| tests/integration.rs | 46 passed; 0 failed |
| tests/perf.rs | 44 passed; 0 failed |
| **Total** | **217 passed; 0 failed** (`grep 'test result:' \| awk` sum = 217) |

Matches the expected 217 = 69+43+15+46+44. No `FAILED`/`panicked` lines in the log.

---

## Item 2 — Contract compliance: PASS

| Contract item (plan) | Result | Evidence |
| --- | --- | --- |
| `AppState { rows, layout, selected, expanded, scroll, status }` all `pub` | PASS | `src/app.rs:76-89` |
| `enum Action { … }` | PASS | `src/app.rs:96-123` — 13 variants, identical to the plan's 13-item list (`SelectNext,SelectPrev,PageNext,PagePrev,Top,Bottom,ToggleExpand,NextBranch,PrevBranch,StashNext,StashPrev,Refresh,Quit`) |
| `reduce(state,&mut, action, repo) -> Outcome` | PASS | `src/app.rs:290`; `Refresh` reloads via `AppState::load` |
| single `keymap` table `action_for` | PASS | `src/keymap.rs:26` (see deviation D1: `action_for(key)` not `(key, mods)`) |
| R1.2 stash: **no Shift = NEXT, Shift = PREVIOUS** | PASS | `keymap.rs:57-59`; `app.rs:305-306`; unit test `stash_direction_and_wrap_match_r1_2` (app.rs:582) + keymap test `stash_keys_follow_r1_2` (keymap.rs:131) assert the direction |
| branch nav per C04, **no wrap** | PASS | `select_branch` (app.rs:347-355) uses `layout::nav`, `NO_ROW` -> selection untouched; test `branch_nav_follows_first_parent_and_child_then_no_ops` (app.rs:555) |
| Ctrl+Arrow aliases | PASS | `keymap.rs:41-42`: `Down if ctrl -> NextBranch`, `Up if ctrl -> PrevBranch`; test `branch_nav_keys` |
| Home/End for top/bottom, `g`/`G` dropped | PASS | `keymap.rs:50-51`: `Home -> Top`, `End -> Bottom`; no `g`/`G` arm. **Approved**: `pm-autoanswers-log.md` M2-W4 round B — "g/G keys: **Home/End only** — drop P0's g/G fallbacks" |
| expand gap = 4 rows | PASS | `EXPANDED_EXTRA_ROWS = 4` (app.rs:41); `row_height = 1 + 4`; visible as 4 `│` rows in the pty shot (Item 3) |
| scroll follows selection | PASS | `ensure_visible` (app.rs:268-283), bottom-anchored; tests `viewport_follows_the_selection`, `scrolling_follows_the_selection_over_the_fixture` |

Note: the worker report §2 counts "exact 12 variants"; the plan and the code both have **13** — a report wording slip only (see Defect D-1).

### Approval cross-check (PM evidence pointer)

Against `.IDE_Plans/pm-autoanswers-log.md` §M2-W4, every dialog answer matches the shipped
code (read-only check):

| Log entry | Decision | Code | Match |
| --- | --- | --- | --- |
| Round B | g/G: **Home/End only** | `keymap.rs:50-51`; no `g`/`G` arm | YES |
| Round B | accept `action_for(key: KeyEvent)` | `keymap.rs:26` | YES |
| Round B | Commit: **Yes** | commit `ff4e192` exists | YES |
| Round A Q1 | Right=first parent, Left=first child, **no wrap** | `keymap.rs:39-40` + `layout::nav` NO_ROW no-op | YES |
| Round A Q2 | Ctrl+Up/Down aliases | `keymap.rs:41-42` | YES |
| Round A Q3 | `s`=next, `Shift+s`=prev, +`Ctrl/Cmd+s` | `keymap.rs:55-59`; test `stash_keys_follow_r1_2` | YES |
| Round A Q4 | Expand gap = 4 rows | `EXPANDED_EXTRA_ROWS=4` (app.rs:41) | YES |
| Round A Q5 | synthetic stash tests + documented skip | `stash_direction_and_wrap_match_r1_2`; mock pty `s` -> row 5 | YES |

Deviation adjudication in that log also matches this report: `w4-report.md` outside the
worktree = accepted (D8 precedent); single commit = noted, non-blocking. No new defects.

---

## Item 3 — Behavior evidence: PASS

The worker's `tools/pty-shot.sh` only produces a ratatui **diff stream**, so a changed
status cell is written without a row break and is hard to read. For clean, unambiguous
evidence I additionally drove the real binary inside a real pty (tmux 3.6) at 140x30 and
took full-screen captures (`tmux capture-pane -p`). Both the worker's `KEYS` path and the
tmux capture were exercised.

Worker `KEYS` path (real repo, `MOCK=0 COLS=140`):

```
KEYS=down,down,right,enter,s MOCK=0 COLS=140 ROWS=30 tools/pty-shot.sh w4-verify-realrepo
-> COMMAND_EXIT_CODE="0"; status bar: sel: 1/27  expanded: 0  lanes: 2  graph: 4 cols
```

tmux full-screen captures (real repo, and `--mock` for the stash path):

| Scenario | Key(s) sent | `sel:` before -> after | `expanded:` | What changed visibly |
| --- | --- | --- | --- | --- |
| nav | `Down Down` | 1/27 -> **3/27** | 0 | selection moved down two rows (SelectNext) |
| branch | `Right` | 1/27 -> **2/27** | 0 | first parent of row 1 (= **NextBranch**, C04) |
| expand | `Enter` | 1/27 -> 1/27 | 0 -> **1** | **4 blank `│` rows** inserted under the selected row (the reserved gap) |
| stash (real) | `s` | 1/27 -> 1/27 | 0 | **no-op** — the real repo has no stash row (documented D5) |
| stash (mock) | `s` | 1/8 -> **5/8** | 0 | jumped to the mock's `stash@{0}` row (no Shift = NEXT, R1.2) |
| stash-prev (mock) | `S` | 1/8 -> **5/8** | 0 | single-stash wrap = same row (Shift = PREVIOUS) |

Raw `Enter` diff (base vs after) shows the gap unambiguously (4 `│` rows added at the top
of the body, later rows pushed down):

```
2a3,6
> │
> │
> │
> │
```

`down/down` visible delta: the only text change is `sel: 1/27` -> `sel: 3/27` (the
selection highlight is reverse-video, which plain text capture does not render). `Right`:
`sel: 1/27` -> `sel: 2/27`. `s` on the real repo: byte-identical frame to the baseline
(`diff` exit 0). All keys exit cleanly (`COMMAND_EXIT_CODE="0"`).


---

## Item 4 — Reference fidelity spot-check: PASS

Three keyboard behaviors taken from inventory **section C** rows, checked against the
cited `web/main.ts` / `web/graph.ts` / `src/config.ts` lines:

| Row | Inventory claim + anchor | Reference (read) | Our impl | Match |
| --- | --- | --- | --- | --- |
| C03 | plain Up/Down = **table order, clamped** — `main.ts:2062-2068` | `2063` comment "order of commits in the table"; `2065` `curHashIndex - 1`; `2067` `curHashIndex + 1` (both guarded, so it clamps) | `SelectNext/SelectPrev` -> `select_table` clamp `0..last` (app.rs:315-322) | YES |
| C04 | Ctrl/Cmd+Up/Down = **branch**: first child / first parent, no wrap — `main.ts:2054-2061`, `graph.ts:624-644`, `598-603` | `2057` `getFirstChildIndex` (ArrowUp); `2059` `getFirstParentIndex` (ArrowDown); `graph.ts:598-603` returns `parents[0].id` or `-1`; `624-644` picks the same-branch child or `-1` | `PrevBranch`=Ctrl+Up->`NavDir::Up`->`first_child`; `NextBranch`=Ctrl+Down->`NavDir::Down`->`parents.first()`; `NO_ROW` -> no-op (app.rs:300-303, layout nav:172-188) | YES |
| C06 | Ctrl/Cmd+S scroll stash; **Shift = previous**, no-shift = next; 5 s memory; wraps — `main.ts:2078-2080`, `1887-1900` | `2079` `this.scrollToStash(!e.shiftKey)`; `1899` `+ (next ? 1 : -1)`; `1900-1903` wrap to `0` / `length-1` | `no Shift -> StashNext`(next=true), `Shift -> StashPrev`(next=false); `select_stash` wraps both ways (app.rs:366-381) | YES (R1.2) |

Bonus anchor check: **C29** expand gap — reference `config.ts:279`
`grid: { …, expandY: 250 }` and `graph.ts:84-105` stretch the graph below the expanded row
by `expandY`. Our W4 reserves a fixed `EXPANDED_EXTRA_ROWS = 4` gap (plan-sanctioned
simplification: W4 shows the gap only; W5 fills the CDV). Direction (stretch *below*) and
row-shift semantics match.


---

## Item 5 — Scope guard: PASS

* **No context menus / dialogs / find** — `grep -rniE 'context_menu|contextmenu|menu|dialog|find_widget|findwidget|overlay|popup' src/` returns **no code matches** (comments aside). W5/W6 surfaces are absent by design.
* **`src/data.rs` untouched** — `git diff cb6fff8 ff4e192 -- src/data.rs` is **empty**.
* **`src/layout/` untouched** — 0 files changed.
* **Touched files beyond `app.rs` / `keymap.rs` / `main.rs`** (all justified as AppState wiring, per plan rule "Layout/render files: minimal, coordinated edits allowed ONLY for AppState wiring"):

| File | Delta | Justification |
| --- | --- | --- |
| `src/render/mod.rs` | +/-129 | reads `AppState` (selection/scroll/expanded); status bar gains `sel:` / `expanded:` / `lanes:` / `graph:`; P0 `commit_list` kept byte-for-byte |
| `src/render/graph_widget.rs` | +106 | new `expanded: &HashSet` + `detail_rows: u16`; reserves the 4-row gap and repeats through-verticals (plan explicitly allows passing expanded rows + detail height) |
| `src/render/columns.rs` | +159 | `commits_table` -> `commits_table_window(rows, expanded, detail_rows, scroll, selected)` so graph and text panes share display offsets when a row is expanded |
| `src/main.rs` | -247 net | event loop drives `action_for` -> `reduce`; terminal setup / size refresh / `--dump` preserved |
| `tests/integration.rs`, `tests/perf.rs` | +2 / +3 | mechanically pass the two new `GraphWidget` fields |
| `tests/app_state.rs` | +228 (new) | T4 integration over the real fixture |
| `tools/pty-shot.sh` | +42 | `KEYS` env for scripted captures (plan T3 asks for exactly this) |

No file outside the W4 remit was modified.


---

## Item 6 — Deviations: judged

| # | Deviation | Severity | Judgement |
| --- | --- | --- | --- |
| D-WT | `w4-report.md` lives in the **master** `.IDE_Plans/`, outside this worktree | Info | Expected: Rule 5 keeps plans/reports in the master checkout. Correct placement. |
| D-CM | **ONE commit** (`ff4e192`) instead of a per-task split | Low | Process point, **approved**: log round B — "Commit: **Yes, commit W4**". Single message documents all changes; no functional impact. |
| D1 | `action_for(key)` instead of sketched `action_for(key, mods)` | Low | **Approved**: log round B — "accept `action_for(key: KeyEvent)`, plan's `mods` param unnecessary". Behaviour identical. |
| D2 | `g`/`G` dropped; `Home`/`End` for top/bottom | Low | **Approved**: log round B — "g/G keys: **Home/End only**". (Reference has no top/bottom binding either.) Defect D-2 resolved. |
| D-LR | `Left`/`Right` bound to branch nav | Info | **Approved**: log round A Q1 — Right=first parent, Left=first child, no wrap. The reference binds only `Ctrl+Arrow` for C04; the TUI addition is PM-authorized, not a breach. |
| D3/D4/D5 | `--dump` split, `Enter` ignored in text-list, stash no-op fixture path | Low | All documented in the report and non-observable in the graph view / covered by unit tests. |

---

## Defects (all low / documentation only, non-blocking)

| ID | Severity | Defect |
| --- | --- | --- |
| D-1 | Low (doc) | `w4-report.md` section 2 states the `Action` enum has "exact 12 variants"; the plan and `src/app.rs` have **13**. Report mis-count only; code matches the plan. |
| D-2 | Low (doc) | `w4-report.md` section 8 lists **Q1** (`g`/`G` vs `Home`/`End`) as an **open** question, while the W4 brief states it is user-approved. The approval is not recorded in `.IDE_Plans/`. |
| D-3 | Low (doc) | `w4-report.md` section 1.2 attributes `src/main.rs` "-247 net"; `git show --stat` reports 247 changed lines (insertions + deletions mix). Wording imprecision only. |

No functional, contract, safety, or scope defects were found.

---

## Verdict

**ACCEPT.** All six verification items PASS with command evidence: the full gate is green
(217 tests, clippy/fmt clean), the W4 contract (AppState/Action/reduce, keymap, R1.2 stash
direction, C04 no-wrap branch nav, Ctrl+Arrow aliases, Home/End, 4-row expand gap,
follow-selection scroll) is met, behavior is reproduced on a real pty, three reference
behaviors match the cited source lines, the scope guard holds (no menus/dialogs/find,
`data.rs`/`layout` untouched), and the deviations are documented and low-severity.
Remaining items are documentation wording only (3 low defects) and can be fixed later
without touching code.

