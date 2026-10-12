# M3-W10 — POST-MERGE VERIFICATION (independent review of the MERGED result)

Reviewer stance: **NOT the author.** The worker merged (`bfdb94f`) before requesting review, so this
is a release-candidate check of the merged `development` tip.

| Field | Value |
|---|---|
| Repo (main worktree) | `/mnt/Just_Do_It/02_Devp_Soft/01_Adi-Mudi/11_Git_Graph/git-graph-terminal` |
| Branch / HEAD reviewed | `development` @ `445858f9615d7abc1634ac2eb97df1297365866a` |
| Source under review | `3eb6102` (W10 source) · `3c94eb9` (ledger flips) · `bfdb94f` (merge) |
| Working tree | clean (`git status --porcelain` → empty) |
| Worker report | `.IDE_Plans/w10-report.md` |
| Ledger | `.IDE_Plans/parity-ledger.md` |
| Toolchain | cargo/rustc 1.99.0 |
| Files touched by this review | **none** (read-only; this file is the only new artifact) |

## 1. Gate re-run on development — PASS

Re-run from scratch on the merged tip. Nothing below is quoted from the worker's report; each line
is this reviewer's own command output.

| Gate | Command | Result |
|---|---|---|
| Unit + integration tests | `cargo test` | **PASS** — exit 0, `274 + 280 + 15 + 63 + 61` = **693 passed, 0 failed**, 0 ignored, 0 warnings |
| Format | `cargo fmt --check` | **PASS** — exit 0, zero diff |
| Lints | `cargo clippy --all-targets -- -D warnings` | **PASS** — exit 0, zero diagnostics |
| Binary smoke | `--dump --mock` | **PASS** — exit 0, 12 lines of rendered mock graph |

Expected 693 / 0 failed: **matches exactly.** Observed 693 / 0 failed.

Every gate test the worker named as a regression gate was also re-run individually against the merged
tip; all 16 pass:

`clean_untracked_honours_directories_and_ignored_files`,
`the_new_cdv_keys_map_to_their_file_row_actions`, `the_new_cdv_keys_build_the_inventory_argv`,
`the_reset_key_stops_at_the_confirmation_dialog`,
`cdv_copy_text_is_absolute_for_y_and_relative_for_shift_y`,
`a_folder_click_toggles_it_open_and_closed`,
`surface_at_reads_the_docked_cdv_as_file_rows`, `surface_at_ignores_a_closed_cdv`,
`hit_geometry_docks_the_cdv_under_the_body`, `the_copy_keys_put_the_path_on_the_clipboard`,
`the_new_cdv_keys_are_no_ops_without_a_file_row`, `the_nine_surfaces_cover_a11_through_a73_once`,
`destructive_ops_build_real_argv_but_are_gated`,
`surface_at_routes_rows_refs_and_empty_space`, `shortcuts_are_unique_and_lowercase_per_surface`,
`every_pager_nav_is_reachable_from_a_key` — **16/16 passed, 0 failed.**

## 2. AUDIT-001 — the `-x` checkbox is REAL — PASS

The claim was that `plan()` for `CleanUntrackedFiles` read only `directories`, so the dialog checkbox
labelled *"Also remove ignored files (-x)"* was a live no-op. Verified it is now a real four-way match.

- **Four-way match exists** — `src/actions.rs:608-613`:
  `match (target.directories, target.include_untracked) { (false,false) => "-f", (true,false) => "-fd", (false,true) => "-fx", (true,true) => "-fdx" }`.
  The old single-ternary (`if target.directories { "-fd" } else { "-f" }`) is **gone**.
- **Test covering all four combinations** — **`clean_untracked_honours_directories_and_ignored_files`,
  `src/actions.rs:1467`** (assertions at `:1471-1488`). It iterates a 4-entry table
  `[(false,false,"-f"), (true,false,"-fd"), (false,true,"-fx"), (true,true,"-fdx")]` and asserts the
  full argv for each. All four clusters are genuinely pinned, not just two.
- **No second meaning for the field** — `include_untracked` is still consumed by `PushStash`
  (`src/actions.rs:962`), and the new match does not disturb it.
- **Not a re-label dodge** — the fix is behavioural (the flag is emitted), not a label change, which
  is what §10 Q1 asked the user to decide.

## 3. CDV file-row menu — PASS

All four required mechanisms are present and independently reachable.

| Mechanism | Location | Verified |
|---|---|---|
| `ClickTarget::CdvFile(String)` | `src/app.rs:409` | variant exists, carries the full path |
| `ClickTarget::CdvFolder(String)` | `src/app.rs:415` | variant exists (INT-050) |
| `HitGeometry` publishes the CDV rect | `src/app.rs:397` (`pub cdv: Option<Rect>`) + built at `src/render/mod.rs:358-379` | rect is `{x:0, y:main_height, width, …}`, docked directly under the body |
| Cell → file row mapping | **`cdv_file_at`**, `src/app.rs:875-901` | insets the 1-cell border, `display = panel.scroll + (y - inner_y)`, skips `summary_lines().len() + 2` header lines, then resolves `file_rows()[index]` → `CdvFile` for `File`, `CdvFolder` for `Folder` |
| Right-click → the A65–A73 menu | `open_menu_at`, `src/main.rs:300`, arm **`src/main.rs:312-317`** | `CdvFile(path)` → `select_path` then `open_menu(MenuSurface::FileRow, Some(&path), …)` |
| Left-click moves the CDV cursor | left-click arm `src/main.rs:267-283`, case **`:272-276`** | `CdvFile(path)` → `CdvState::select_path(&path)` |
| The menu is actually enabled | `MenuSurface::FileRow` arm in `menu_context`, `src/app.rs:643-650` sets `ctx.path`; items gated on `ctx.path.is_empty()` at `src/menu.rs:681-731` | **Flag C confirmed fixed** — without `ctx.path` all nine A65…A73 rows render disabled |

Test evidence: `surface_at_reads_the_docked_cdv_as_file_rows` (`src/app.rs:2371`) asserts a file row
resolves to its own path, a folder row resolves to its own path, the border column and past-the-end
resolve to `None`, and the first body row is still a commit row (the panel did not swallow it).
`a_cdv_file_click_moves_the_cursor_and_opens_the_file_menu` (`src/app.rs:2509`) asserts the cursor
moves, `menu.context.path == path`, the nine ids `A65…A73` are exactly the item list, and
**`menu.items.iter().all(|item| item.enabled)`** — so the menu is reachable *and* enabled, not merely
openable.

## 4. The five keys (`w` `e` `R` `y`/`Y`) — PASS

| Key | Action | Bind site | Inventory |
|---|---|---|---|
| `w` / `W` | `CdvFileViewDiffWithWorkingFile` | `src/keymap.rs:193` | A67 / DSP-060 |
| `e` / `E` | `CdvFileOpen` | `src/keymap.rs:194` | A68 / DSP-038 |
| `R` (shift only) | `CdvFileResetToRevision` | `src/keymap.rs:197` | A71 / DSP-049 |
| `y` | `CdvFileCopyAbsolutePath` | `src/keymap.rs:199` | A72 / DSP-011 |
| `Y` | `CdvFileCopyRelativePath` | `src/keymap.rs:200` | A73 / DSP-011 |

Handler arms: `src/app.rs:1008-1014`. Keymap tests: `src/keymap.rs:495-499`.
Mapping test: `the_new_cdv_keys_map_to_their_file_row_actions` (`src/app.rs:2580`) — also proves
unshifted `r` stays unbound, so a stray press cannot arm the destructive reset.

**No fresh argv construction.** All five route through the pre-existing
`cdv_file_target` (`src/app.rs:1240`) → `actions::plan` / `actions::dispatch`
(call sites `src/app.rs:1267`, `:1269`, `:1288`, `:1309`). A grep for `vec![`, `argv =`, `Command::new`
and bare `arg(` across the entire added diff returns **only four hits, all inside test assertions**
(`src/actions.rs:1474`, `src/app.rs:2540`, `:2618`, `:2635`) — the shipped handlers build no argv.
`the_new_cdv_keys_build_the_inventory_argv` (`src/app.rs:2611`) asserts the argv comes out of the
shared planner: `diff --no-color <hash> -- src/lib.rs`, `show :src/lib.rs`, `checkout <hash> -- src/lib.rs`.

`y`/`Y` genuinely differ: `cdv_copy_text` (`src/app.rs:1319-1325`) repo-joins for absolute and passes
through for relative, pinned by `cdv_copy_text_is_absolute_for_y_and_relative_for_shift_y`
(`src/app.rs:2682`). **Flag B confirmed fixed** — A72/A73 no longer share one payload.

## 5. Folder click-to-toggle (INT-050) works in the CDV tree — PASS

- `CdvState::select_path` skips folder rows (`src/cdv.rs:339-350`).
- `toggle_cdv_folder` (`src/app.rs:1217-1223`) flips `closed_folders` then re-runs
  `ensure_file_visible`, so the viewport follows the cursor.
- Wired into the left-click arm at `src/main.rs:278-280`, and a right-click on a folder closes the
  menu instead of falling through to the commit-row menu (`src/main.rs:320`).
- Underlying flip: `CdvState::toggle_folder` (`src/cdv.rs:264-271`).
- **Test: `a_folder_click_toggles_it_open_and_closed` (`src/app.rs:2415`)** — asserts the folder row is
  a `CdvFolder` target, the first toggle closes it and hides its children, and the second restores
  exactly the original row count. Genuine round-trip, not a stub.
- Pre-existing `surface_at` tests still pass unchanged, confirming the `body.height` recomputation
  (`src/render/mod.rs:366-379`, now `screen − cdv − find − status`) is a no-op when no overlay is open.
  **Flag A confirmed fixed** — the commit-row region no longer covers the CDV or the find bar.

## 6. Safety re-check — destructive actions still dialog-gated — PASS

Spot-checked the reset path end to end, as required:

1. `cdv_reset_file` (`src/app.rs:1283-1296`) never executes. It calls `actions::dispatch`, which for a
   `PlanKind::Confirm` returns `DispatchOutcome::NeedsConfirm(plan)` **without invoking git**
   (`src/actions.rs:1298`, `:1190`).
2. On `NeedsConfirm` the only side effect is `open_dialog_for_plan` (`src/app.rs:1487-1498`), whose
   body is a `let … = plan.kind else { return false }` guard plus `state.dialog = Some(…)`.
   It cannot write.
3. `ResetFileToRevision`'s plan is genuinely gated: `plan.kind.is_gated()` asserted at
   `src/app.rs:2632`.
4. **Test asserting the gate: `the_reset_key_stops_at_the_confirmation_dialog` (`src/app.rs:2640`)**
   — it points the repo at a **bogus path** (`/nonexistent/ggt-w10`) precisely so that any real git
   call would fail, then asserts `state.dialog.is_some()` and that the status bar contains
   `"confirm"`. A stray `R` therefore opens a `PlanKind::Confirm` dialog and performs **no write**.
5. `destructive_ops_build_real_argv_but_are_gated` (`src/actions.rs:1432`) still covers all 11
   destructive ops including `CleanUntrackedFiles`, asserting real argv + gated + mutating.
6. `the_new_cdv_keys_are_no_ops_without_a_file_row` (`src/app.rs:2664`) proves no key reaches a
   dialog, pager or clipboard when no panel/row is selected.

`R` is deliberately shifted so an unshifted press cannot arm a working-tree overwrite
(`src/keymap.rs:195-197` + assertion at `src/app.rs:2599-2602`). Safety is intact.

## 7. Ledger integrity — PASS

- **Cited commit hashes are real.** The only hashes the ledger cites are `3eb6102` and `d6bc02d`.
  Both resolve in this repo: `3eb6102` → *"fix(w10): make the clean -x checkbox real, and route the
  CDV file-row menu"*; `d6bc02d` → *"docs(verify): W8 wave verification …"*. No dangling or invented
  hashes.
- **Three flipped rows spot-checked** (each re-read against the real source, not the report):
  - **ACT-071** (line 221) → `R` → `CdvFileResetToRevision` → DSP-049 gated. Cites
    `src/keymap.rs:198`; the `R` bind is at `src/keymap.rs:197` (one line off — see observation 2).
    Cites test `the_reset_key_stops_at_the_confirmation_dialog` → **exists, passes**.
  - **INT-050** (line 448) → `ClickTarget::CdvFolder` `src/app.rs:415` ✓,
    `toggle_cdv_folder` `src/app.rs:1217` ✓, left-click arm `src/main.rs:278` ✓. Cites
    `a_folder_click_toggles_it_open_and_closed` → **exists, passes**. Fully accurate.
  - **ACT-073** (line 223) → `Y` → `CdvFileCopyRelativePath`, relative payload, distinguishing A72/A73.
    Cites `cdv_copy_text_is_absolute_for_y_and_relative_for_shift_y` → **exists, passes**.
- **All 11 ledger-cited tests resolve as real `fn`s in `src/`** (checked by name against the tree).
- **§8 totals independently recomputed** by this reviewer from the row tables per section — not
  trusted from the file:

  | Family | Rows | DONE | NOT-PORTED | DESIGN-DIFFERENCE | BACKLOG |
  |---|---|---|---|---|---|
  | §1 Rulings | 11 | 11 | 0 | 0 | 0 |
  | §2 DSP | 62 | 49 | 11 | 2 | 0 |
  | §3 ACT | 111 | 52 | 20 | 5 | 34 |
  | §4 SET | 110 | 0 | 56 | 48 | 6 |
  | §5 INT | 63 | 30 | 8 | 8 | 17 |
  | §6 VIS | 59 | 43 | 10 | 3 | 3 |
  | §7 DRIFT | 9 | 2 | 7 | 0 | 0 |
  | **Total** | **425** | **187** | **112** | **66** | **60** |

  Matches the §8 table in the ledger **exactly** (425 rows, 187 DONE, 60 BACKLOG). No blank or
  out-of-legend status anywhere in the row tables; the only non-legend entry is the AUDIT-001 finding
  row in §8.1, which is an audit row by design, not a parity row.

## Item verdict table

| # | Item | Verdict | Evidence |
|---|---|---|---|
| 1 | Gates re-run on merged tip | **PASS** | `cargo test` 693/0 exit 0 · `fmt --check` exit 0 · `clippy -D warnings` exit 0 · smoke exit 0 |
| 2 | AUDIT-001 `-x` is real, four-way, tested | **PASS** | `src/actions.rs:608-613`; test `clean_untracked_honours_directories_and_ignored_files` `src/actions.rs:1467` pins all 4 |
| 3 | CDV file-row menu | **PASS** | `CdvFile` `app.rs:409`; `HitGeometry.cdv` `app.rs:397` + `render/mod.rs:358`; `cdv_file_at` `app.rs:875`; right-click `main.rs:312-317`; left-click `main.rs:272-276`; `ctx.path` `app.rs:643-650`; test `app.rs:2509` |
| 4 | Five keys reuse plan() argv | **PASS** | binds `keymap.rs:193/194/197/199/200`; handlers `app.rs:1008-1014`; zero fresh argv outside tests; reset test `app.rs:2640` |
| 5 | Folder click-to-toggle (INT-050) | **PASS** | `app.rs:1217` + `main.rs:278-280`; test `a_folder_click_toggles_it_open_and_closed` `app.rs:2415` |
| 6 | Destructive actions still gated | **PASS** | `dispatch` `actions.rs:1298` returns `NeedsConfirm` with no git call; `cdv_reset_file` `app.rs:1283`; test `app.rs:2640` |
| 7 | Ledger integrity | **PASS** | hashes `3eb6102`/`d6bc02d` resolve; 3 rows spot-checked accurate; §8 recompute 425/187/112/66/60 exact |

**Blocking defects: 0.**

## Minor, non-blocking observations (recorded for honesty, not counted as defects)

These are documentation line-number imprecisions in the worker's own prose. Every function name,
behaviour, test name and commit hash they point at is correct and verified; only the digit is off.
None affects shipped code, behaviour, or the ledger's meaning.

1. `INT-029` (`.IDE_Plans/parity-ledger.md:427`) cites `src/main.rs:307-312` for the `CdvFile` arm; the
   arm is at `src/main.rs:312-317`. Likewise `.IDE_Plans/w10-report.md:73` cites `open_menu_at` at
   `src/main.rs:296`; it is at `:300`.
2. `ACT-071` cites `src/keymap.rs:198` for the `R` bind; it is at `:197` (`:198` is the following comment).
3. `ACT-072` (`parity-ledger.md:222`) attributes the payload to `cdv_copy_text` at
   `src/app.rs:1217` region; `:1217` is `toggle_cdv_folder` — `cdv_copy_text` is at `:1319`.
4. `AUDIT-001` in §8.1 and the report cite `src/actions.rs:605-613`; the match is at `:608-613`
   (`:605-613` spans the whole `CleanUntrackedFiles` arm, so this one is a fair range).

Suggested follow-up: a one-line correction pass on those four citations in the next docs-only commit.
Not required for release.

## Reviewer's independent conclusion

The merged `development` tip is internally consistent and matches its own claims. The AUDIT-001 fix
is behavioural and four-way tested; the CDV menu is genuinely reachable **and enabled** (the
`MenuContext.path` gap would have shipped a dead menu, and it is closed); the folder toggle is a real
round-trip; the five keys introduce no argv of their own; and the destructive reset path provably
cannot write — the test that proves it uses a bogus repo path precisely so a real git call would fail.
The ledger's own arithmetic recomputes exactly. No source file was modified by this review.

VERDICT: ACCEPT
