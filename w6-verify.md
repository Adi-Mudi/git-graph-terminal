# W6 WAVE VERIFICATION — independent verifier report

Wave: M2-W6 (find widget + dialogs + commit-details view) — M2 completion
Branch: `ggt-m2-w6` · Range: `development`..`b8dfc74` · 11 commits, single wave
Verifier: cline (headless, independent) · PM: Hermes · Date: 2026-10-12
Report under review: `.IDE_Plans/w6-report.md` (reference repo, read-only — not edited)
Truth docs: `.IDE_Plans/g7-checklist.md` R1–R6, `g7-inventory-pi.md` §3/§4, reference `../vscode-git-graph`.

Method: re-ran every gate from a clean shell (`PATH` includes `$HOME/.cargo/bin`); recounted the
clippy findings by hand and isolated the 2 "extra" errors cargo reports; cross-checked the find scope,
CDV argv and dialog rules against the live reference TypeScript. No source file was edited.

---

## 1. Per-item results

### 1.1 FULL TEST / fmt / clippy — PASS
| Gate | Command | Result |
|---|---|---|
| fmt | `cargo fmt --check` | exit **0**, clean |
| tests | `cargo test` | exit **0**, **583 passed / 0 failed** across 5 targets |
| clippy | `cargo clippy --all-targets -- -D warnings` | exit **101** (expected under `-D`); **12 distinct dead-code errors** in the bin lib target — see §2 reconciliation |

Test breakdown (matches task's expected 583):
| Target | Passed |
|---|---|
| bin unit tests (src/main.rs) | 236 |
| tests/app_state.rs | 242 |
| tests/golden_layout.rs | 15 |
| tests/integration.rs | 46 |
| tests/perf.rs | 44 |
| **Total** | **583**, 0 failed, 0 ignored |

### 1.2 Contract spot-checks — PASS

1. **Find scope = author / hash+abbrev / message(subject) / branch+tag+remote / date / stash — NOT body.**
   `row_matches` (`src/find/mod.rs:207-226`) matches on `commit.author` (gated on `columns.author`),
   `commit.hash` (anchored) / abbrev (gated on `columns.commit`), **`commit.subject`** (the message —
   no body field exists on `Commit`), head/remote/tag names, `short_date` (gated on `columns.date`),
   stash selector. Uncommitted row skipped. Verified **byte-for-byte** against reference
   `web/findWidget.ts:236-245` (`findPattern.test(commit.message)` — the reference searches `message`, not body).
   Tests: `literal_search_covers_every_scope_branch`, `hidden_columns_leave_the_scope`,
   `the_uncommitted_row_is_never_a_match`. **PASS.**
2. **`N of M` counter + No Results wording.** `counter()` (`find/mod.rs:388-394`): `"No Results"` when
   empty, else `format!("{} of {}", position+1, matches.len())`. Reference `findWidget.ts:367`
   (`(position+1)+' of '+length : 'No Results'`). Test `the_counter_reads_n_of_m_and_no_results`. **PASS.**
3. **Enter/Shift+Enter = next/prev match.** Reference binds `Enter`→`next`, `Shift+Enter`→`prev`
   (`findWidget.ts:37` titles + `:43-47` handler). The port's `next()`/`prev()` (`find/mod.rs:397/411`,
   forward/backward with wrap) implement the semantics and are unit-tested
   (`next_and_prev_wrap_in_both_directions`). **Runtime wiring of Enter→find is a declared W7-2 gap**
   (find query not typeable from the event loop; report §3 T1). Model correct, wiring deferred — matches report. **PASS (with declared gap).**
4. **Regex zero-length rejected.** `ZERO_LENGTH_MESSAGE` = the reference string verbatim
   (`find/mod.rs:54`), enforced via `Regex::matches_empty`; tests
   `a_zero_length_pattern_is_rejected_with_the_reference_message` + `…_on_an_empty_repository`. **PASS.**
5. **Esc closes and restores.** `close()` clears text/matches/position and `show()` restores
   (`find/mod.rs:254-271`; doc cites `findWidget.ts:139-158`); reference `findClose` title = "Close (Escape)". **PASS.**
6. **Dialog: Enter submits from anywhere; focus order; submit runs the action.** `handle()`
   (`dialog.rs:353`): `Enter`→Submit (C12, from any field incl. inputs), `Escape`→Close (C02),
   `Tab`/`Shift+Tab` walk the generated `tabindex` order and wrap. `submit()` runs the action, not cancel
   (`dialog.ts:393-400`). **PASS.**
7. **25 destructive actions execute ONLY via a confirmed dialog plan.** Two-step safety:
   `dispatch` refuses `PlanKind::Confirm` (returns `NeedsConfirm`, `actions.rs:1263`);
   `execute_confirmed` (`actions.rs:1288`) **accepts only a gated (`Confirm`) plan** — anything else
   returns `Skipped` — and is called only after a dialog `Submit`. A cancelled dialog provably never
   reaches git. Test `dispatch_refuses_gated_plans_without_running_git`. **PASS.**
8. **CDV file list argv matches `dataSource.ts:1779-1781`.** `commit_file_changes` (`files.rs:230-257`)
   builds `diff-tree --raw|-numstat -r --root --find-renames --diff-filter=AMDR -z <hash>` — identical
   flags and order to reference `['diff-tree', arg, '-r', '--root', '--find-renames', '--diff-filter='+filter, '-z', fromHash]`. **PASS.**
9. **Tree/List toggle.** `FileView::Tree`/`List` (`cdv.rs:108`), `v` toggles (VIS-041/042); tests
   `vis_041_and_042_the_view_toggles_and_is_named`, `…_the_tree_nests_and_the_list_does_not`. **PASS.**
10. **4-row gap shows message + files.** `EXPANDED_EXTRA_ROWS = 4` (`app.rs:50`); `ToggleExpand`
    (`app.rs:755`) calls `open_cdv`, docking the summary + file list in the expanded row. **PASS.**

### 1.3 data.rs deviation (5-line UNCOMMITTED const) — PASS
`git diff --numstat development..HEAD -- src/data.rs` = **`5  0`** (5 additions, **0 deletions**);
deletion-line count = 0. The change is a single doc comment + `pub const UNCOMMITTED: &str = "UNCOMMITTED";`
inserted between two untouched lines. No existing line modified, no existing code path reads it
(`data.rs` never emits it as a commit hash). **Pure-additive, zero behavior change — PASS.** Matches report §6.1.

### 1.4 W7 debt list (§11.1) completeness — PASS
All **12** clippy dead-code findings map 1:1 to the report §11.1 table (13 rows; rows 12+13 are one
`impl` block printed as a single finding). Line anchors verified against source:

| # | Finding (file:line) | Report §11.1 row | W7 |
|---|---|---|---|
| 1 | app.rs:164 `FindClose`,`DialogConfirm`,`DialogCancel` | 1 | W7-1 |
| 2 | app.rs:946 `dialog_key` | 2 | W7-1 |
| 3 | dialog.rs:143 `validate`,`insert`,`backspace`,`delete` | 3 | W7-1 |
| 4 | dialog.rs:192 `char_byte_index` | 4 | W7-1 |
| 5 | dialog.rs:213 `DialogValue` | 5 | W7-1 |
| 6 | dialog.rs:221 `DialogResult` | 6 | W7-1 |
| 7 | dialog.rs:233 `DialogKey` | 7 | W7-1 |
| 8 | dialog.rs:272 `message`,`focusable_count`,`validate`,`handle`,`values` | 8 | W7-1 |
| 9 | find/mod.rs:340 `set_text`,`push_char`,`pop_char`,`clear_text`,`row_is_match`,`status_line` | 9 | W7-2 |
| 10 | find/regex.rs:427 `is_match` | 10 | W7-2 |
| 11 | cdv.rs:69 `Uncommitted`,`Comparison` | 11 | W7-4 |
| 12 | cdv.rs:150 `commit`,`uncommitted`,`comparison`,`folder_open`,`toggle_folder`,`toggle_reviewed` | 12+13 | W7-4 + W7-3 |

**Nothing hidden.** `grep` for `#[allow(dead_code)]` / `#[expect]` / `#[cfg(test)]` gating: the four new
W6 modules (`dialog.rs`, `cdv.rs`, `files.rs`, `find/`) and `app.rs` have **zero** `#[allow(dead_code)]`.
The only suppressions are (a) pre-existing W5 `#[allow(dead_code)]` on coverage-test-only items in
`actions.rs`/`menu.rs`/`layout/mod.rs`, and (b) one legitimate `#[allow(clippy::too_many_arguments)]` on
the live `find/regex.rs:587 match_repeat`. None of the 12 findings is gated, so the gate stays red until
W7 — exactly as the report states. **PASS.**

---

## 2. Clippy-14 reconciliation

The task expects "EXACTLY 14 errors". Measured behaviour of `cargo clippy --all-targets -- -D warnings`:

- bin lib target: `could not compile … due to **12** previous errors` → **12 distinct** dead-code findings.
- bin **test** target: `could not compile … due to **2** previous errors`. Cargo's own summary
  (`--message-format=short`) states: `(bin "git-graph-terminal" test) generated 2 warnings **(2 duplicates)**`.

So the raw error-rollup line count is 12 + 2 = **14**, but the 2 extra are cargo-confirmed **duplicates**
of 2 of the 12 findings, re-emitted for the separately-compiled test harness. **Distinct findings = 12**,
which is what the report §11.1 enumerates and what §8 documents (its own history: "50 → 14 (hygiene
`423e17a`) → 12 (deletion `b8dfc74`)"). Both the task's 14 (rollup lines) and the report's 12 (distinct
findings) are correct; they count different things. **All 14 error kind = dead-code (never used/never
constructed); no other clippy error kind (no style/perf/correctness lint) appears.** No defect.

---

## 3. Defects table

| # | Severity | Where | Description | Blocking? |
|---|---|---|---|---|
| — | none | — | No functional or contract defect found. The clippy-14 vs 12 gap is a counting-method nuance (§2), fully explained by cargo's duplicate re-emission and the report's own documented 14→12 trajectory. | No |

**Functional defects: 0. Contract defects: 0.**

---

## 4. Verdict

fmt clean; 583/583 tests green; the find scope, counter, zero-length rule, next/prev semantics, dialog
submit/focus/cancel and the CDV `diff-tree` argv all match the live reference; the 25 destructive actions
are provably gated behind `dispatch`+`execute_confirmed`; the `data.rs` deviation is pure-additive with
zero behavior change; and the §11.1 W7 debt list enumerates every one of the 12 distinct clippy
dead-code findings with no hidden `#[allow]` gating. The three declared runtime-wiring gaps (find text
entry, dialog submit key, CDV file-row cursor) are honestly reported as W7 scope, not hidden.

VERDICT: ACCEPT (defects: 0)

11. **File-row actions dispatch through existing paths.** A65–A68 (View Diff / View File / Diff with
    Working File / Open File) reuse the existing pager/diff dispatch. **Runtime reachability is the
    declared W7-3 gap** (no file-row cursor; report §3 T3, §10 item 3). Model + argv DONE, wiring deferred — matches report. **PASS (with declared gap).**
