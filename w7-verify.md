# W7 VERIFICATION — runtime wiring lane (M2 completion)

Independent review. Reviewer is NOT the author.

- Worktree: `../git-graph-terminal-w7` · Branch `ggt-m2-w7` · Base `development` @ `57ef293`
- HEAD: `fa215e6` · **6 commits** (`e8def58, cca56a6, 1258308, 194587a, 9dddd6a, fa215e6`)
- Report under review: `.IDE_Plans/w7-report.md` (§W7)
- Contract: `.IDE_Plans/g7-checklist.md` + `g7-inventory-{pi,cline}.md` + reference clone `../vscode-git-graph`
- Toolchain: `/home/divakaran/.cargo/bin/cargo`

---

## 1. Gate re-run

| Gate | Command | Result | Expected | Verdict |
|---|---|---|---|---|
| G1 | `cargo test` | **599 passed, 0 failed, 0 ignored** (244 bin + 250 app_state + 15 golden_layout + 46 integration + 44 perf) | 599 pass / 0 failed | **PASS** |
| G2 | `cargo fmt --check` | exit **0**, clean | clean | **PASS** |
| G3 | `cargo clippy --all-targets -- -D warnings` | exit **0**, 0 errors (forced full re-check, no cache) | EXACTLY 0 | **PASS** |

Test math: 244+250+15+46+44 = **599** ✓ (matches report §4).
G3 was re-run after `touch src/main.rs` to defeat the compile cache → real `Checking` pass → 0 diagnostics.

---

## 2. The W7 promises

| # | Item | Evidence (file:line) | Verdict |
|---|---|---|---|
| T1 | Dialog keys live at runtime | `FindClose`/`DialogConfirm`/`DialogCancel` **constructed**: declared `app.rs:171,183,185`; enumerated in the exhaustive harness `app.rs:236,242,243`; **reduced** in the dispatch match `app.rs:897,915,916`; **driven** from the event loop `main.rs:165,171`. `dialog_key` wired `app.rs:1299` ← `main.rs:163`. Regression harness `keymap.rs:553` `every_action_is_reachable_from_a_key` over `Action::ALL` with exhaustive `representative()` `keymap.rs:464-524`. | **PASS** |
| T1b | No `#[allow]` added by W7 | `git diff 57ef293..HEAD \| grep -c '^+.*#\[allow'` = **0** (0 added, 0 removed). The 17 `#[allow]` in the tree are W5/W6 legacy, byte-identical to base. `src/pager.rs` carries **0**; `mod pager;` at `tests/app_state.rs:55-56` carries **0** (the D-2 promise held). | **PASS** |
| T2 | Find text entry: typed query → N of M + highlight; Esc restores | `push_char`/`pop_char` `find/mod.rs:340,345` wired `app.rs:1247,1251`. `N of M` readout `find/mod.rs:374` `format!("{} of {}", position+1, matches.len())` + `status_line` `find/mod.rs:429-431`. Match highlight = `row_is_current` (`render/columns.rs:151`, `findCurrentCommit` tint) + `ranges_in` inline spans (`render/mod.rs:316`). **Esc restores selection**: `close_find` `app.rs:1220-1222` `state.find.close()` then restores `selection_before_find.take()`. | **PASS** |
| T3 | CDV file-row cursor + A65–A68 through `src/pager.rs` | Cursor wired: `cdv_move_file`/`selected_change` (`app.rs`), keys `keymap.rs:184-188` (`j/k/Enter/o/space/←/→`). A65 `ViewDiff` `actions.rs:1082-1097`, A66 `ViewFileAtRevision` `actions.rs:1118-1130`, A67/A68 same block — all `PlanKind::Execute`, opened into the pager via `cdv_open_pager` (`app.rs:921-922`). | **PASS** |
| T3b | Pager is read-only: no shell, direct exec, no injection | **`src/pager.rs` contains no `Command`/`spawn`/`exec`/`sh`** (grep clean; only doc comments and `Pager::new("git show …", …)` test literals). Pager is a passive `Vec<String>` + `scroll`. Execution is via `data::run_git` `data.rs:99-120` = `Command::new("git").arg("-C").arg(repo).args(&[&str])` — **direct exec, argv slice, no shell string, no interpolation**. Renders with `Line::raw(line.clone())` (`pager.rs:131`) = **literal text, no ANSI/escape interpretation** → captured output cannot inject terminal codes. | **PASS** |
| T4 | `u` Uncommitted + `c` mark-then-open Comparison kinds | `CdvKind::Uncommitted` constructed `app.rs:1022` (from `working_tree_changes`); `CdvKind::Comparison` `app.rs:1064` (from `comparison_changes`); two-step `compare_mark` `app.rs:1036-1050`. Both openers set `cdv_focused=true` (`app.rs:1026,1071`). | **PASS** |
| T4b | VIS-051 external-diff detection | `external_diff_tool` `files.rs:438-446`: reads `gui.difftool`, falls back to `diff.tool`. **Exact match** to reference `web/main.ts:3123-3125` (`guiDiffTool` wins else `diffTool`). Read into CDV at `app.rs:1024,1069` → `cdv.rs:117` `external_diff`. | **PASS** |
| T4c | `tab` token in `tools/pty-shot.sh` | `tools/pty-shot.sh:63` `tab) printf '\t' ;;`. Added in commit `fa215e6` — **resolves** report D-4 / §11 item 5. | **PASS** |

---

## 3. Deviation audit

### (a) 5 items deleted vs W6 §11.1 "wired" — confirmed duplicate/unreachable

All four symbols confirmed **ABSENT in HEAD** (`git grep … HEAD`):

| Deleted | Present in HEAD? | Behaviour carried by (file:line) | Duplicate/unreachable? |
|---|---|---|---|
| `FindWidget::row_is_match` | **ABSENT** | `row_is_current` `render/columns.rs:151` (current-match tint) + `ranges_in` `render/mod.rs:316` (per-span highlight) | ✓ distinct behaviour already covered |
| `FindWidget::set_text`, `clear_text` | **ABSENT** | `push_char`/`pop_char` `find/mod.rs:340,345` (typing) + `close()` (clearing) | ✓ redundant with the entry API |
| `CdvState::commit` | **ABSENT** | `loading` + `loaded` two-step (`app.rs:983-986`, `cdv.rs` `loaded_commit` test helper) | ✓ `commit` was a one-shot that bypassed the real load path |
| `Regex::is_match` | **ABSENT** | `Pattern::is_match` `find/mod.rs:104` (sole caller; body was `find_at(text,0).is_some()`) | ✓ strict subset, single call site |

Precedent: identical 5-item deletion in `b8dfc74`. Constraint kept: never `#[allow]`. **PASS** — each is a true duplicate or unreachable, and the deletion is the W6-sanctioned alternative to gating.

### (b) 3 updated tests — each has a documented reference-based reason (not make-it-pass)

| Test | Change | Documented reason |
|---|---|---|
| `find/mod.rs` `set_text_rescans_and_clears_when_emptied` → `typing_a_pattern_rescans_and_emptying_it_clears_the_matches` | body rewritten to `push_char`/`pop_char` | D-1: `set_text`/`clear_text` deleted; ref `web/findWidget.ts:366` |
| `find/mod.rs` `row_helpers_answer_match_and_current` → `the_current_row_walks_the_matches` | asserts `matched_rows`/`current_row` instead of `row_is_match` | D-1: `row_is_match` deleted; ref `web/findWidget.ts:366` |
| `actions.rs` coverage count | `execute 10→14`, `oos 15→11` | Inline comment: *"A65–A68 moved from OutOfScope to Execute in W7/T3 (10 -> 14)"*; refs `utils.ts:418-443,453-477,484-490,390-405` — 4 DSP rows genuinely reclassified by wiring, not to force green |

The `find/regex.rs` `is_match`→`find_at` edits are the same D-1 deletion, documented. The `cdv.rs` `commit`→`loaded_commit` swaps are a mechanical adaptation to the deleted constructor (documented helper `loaded_commit`). **PASS** — no assertion was loosened to pass; each edit tracks a deleted API or a real reclassification.

### (c) DIFF_FILTER const — reference-backed and pure-additive

- Anchor confirmed: `vscode-git-graph/src/dataSource.ts:1465` = `filter: string = 'AMDR'`.
- Location: the const is `pub const DIFF_FILTER: &str = "AMDR"` at **`src/files.rs:331`** (the task said "data.rs"; it actually lives in `files.rs`, and the report §1 correctly lists `data.rs` only as a frozen *dependency* — a prompt mislabel, not a mis-wiring).
- Pure-additive: the const **already existed at base `57ef293`** (`files.rs:227`) and served `commit_file_changes`. W7 **did not** change `commit_file_changes`'s argv (`--diff-filter=AMDR` unchanged). W7 only **reused** the existing const in two brand-new functions: `working_tree_changes` (`files.rs:372`) and `comparison_changes` (`files.rs:405`). **PASS.**

### (d) Focus asymmetry (d vs u/c) documented report-only

`d` (A-open CDV) leaves `cdv_focused=false` (needs one `Tab` before `j`/`Enter`); `u`/`c…c` set `cdv_focused=true` immediately (`app.rs:1026,1071`). Documented in report §7 and §11 item 4, **not silently fixed**. **PASS** — reported as a UX ruling, not a wiring gap.


---

## 4. Safety

| Check | Evidence | Verdict |
|---|---|---|
| Pager path must not execute arbitrary content | `src/pager.rs` runs **nothing** — no `Command`/`spawn`. The only exec is `data::run_git` (`data.rs:100-104`): `Command::new("git").arg("-C").arg(repo).args(slice)` — direct exec, **argv array, no shell, no string interpolation → no injection path**. A65–A68 argv is a fixed `git diff`/`git show` (`actions.rs:1089-1129`), never a destructive verb. Pager body rendered via `Line::raw` (`pager.rs:131`) = literal text, **no ANSI/escape interpretation** → captured output can't inject terminal codes. | **PASS** |
| Dialog confirm still gates all destructive actions (spot-check 3) | **DeleteBranch** `actions.rs:690-697` `gated("deletes a ref")`; **CleanUntrackedFiles** `actions.rs:605-614` `gated("deletes untracked files")`; **ResetToCommit** `actions.rs:1012-1023` `gated("moves the branch pointer")`. All → `PlanKind::Confirm` (`gated` = `actions.rs:1141`). `dispatch` **refuses** Confirm (`actions.rs:1291` → `NeedsConfirm`); only `execute_confirmed` (`actions.rs:1316-1319`) runs it and **accepts only gated plans**; reachable only from `confirm_dialog` (`app.rs:1309-1327`) after a dialog submit. Cancelling is provably a no-op. | **PASS** |

---

## 5. Observations (not defects)

1. **Report §10 / §11 item 1 say "nothing is committed."** The actual tree now has **6 commits** (`e8def58…fa215e6`). The report's "ruling needed: commit?" was evidently resolved by committing as a 6-commit series after the report was written. Report-vs-tree timing, not a code defect; the reviewed HEAD is what the gates above ran against.
2. **D-3 scope note:** `src/pager.rs` (195 lines, 6 tests) is genuinely new vs the base (T3 named an "existing" path that did not exist at `57ef293`). It is minimal, read-only, reference-anchored, and necessary for A65/A66 acceptance — acceptable scope growth, already flagged by the author.
3. Prompt said "data.rs DIFF_FILTER"; it is in `files.rs`. Anchor and pure-additive both verified; no impact on the verdict.

---

## VERDICT: ACCEPT

All gates green (test 599/0, fmt 0, clippy 0). T1–T4 promises hold at runtime with file:line evidence. All 5 deletions are true duplicates/unreachable; all 3 test updates carry documented reference-based reasons; DIFF_FILTER is reference-backed and pure-additive; focus asymmetry is report-only. Pager is read-only (direct exec, argv array, no shell, no injection, literal-text render) and every destructive action remains dialog-gated.

**Defects: 0.**
