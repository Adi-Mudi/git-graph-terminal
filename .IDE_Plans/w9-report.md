# M3-W9 — Report: full parity ledger + remaining-gaps audit

Base: `development` @ d6bc02d · Branch `ggt-m3-w9` · Worktree `../git-graph-terminal-w9`
Artifact: `.IDE_Plans/parity-ledger.md` (630 lines) — the write exception named in the plan.

## Reply: DONE — counts

| Status | Rows | Share |
|---|---|---|
| **DONE** | **179** | 42% |
| NOT-PORTED | 112 | 26% |
| DESIGN-DIFFERENCE | 66 | 16% |
| BACKLOG | 68 | 16% |
| **Total** | **425** | 100% |

Per family:

| Family | Rows | DONE | NOT-PORTED | DESIGN-DIFFERENCE | BACKLOG |
|---|---|---|---|---|---|
| R1–R6 rulings register | 11 | 11 | 0 | 0 | 0 |
| DSP dispatch map | 62 | 49 | 11 | 2 | 0 |
| ACT commands/actions | 111 | 47 | 20 | 5 | 39 |
| SET settings | 110 | 0 | 56 | 48 | 6 |
| INT interactions | 63 | 27 | 8 | 8 | 20 |
| VIS visual states | 59 | 43 | 10 | 3 | 3 |
| DRIFT | 9 | 2 | 7 | 0 | 0 |

## T1 — Ledger build: DONE

- Every row of `.IDE_Plans/g7-checklist.md` (R1–R6, 11 rulings) and every inventory
  family (DSP 62 / ACT 111 / SET 110 / INT 63 / VIS 59 / DRIFT 9) is present: **425 rows,
  zero blanks**, one row per contract item.
- Status vocabulary is exactly the four allowed values; no other value appears anywhere.
- §1 records all 11 user rulings verbatim with the test that pins each one.

## T2 — Gap audit: DONE

- §9.1 splits the 68 `BACKLOG` rows into 10 waves (W10-A…W10-J) that cover **all 68
  exactly once** — verified mechanically: the set of ids named in §9.1 equals the set of
  `BACKLOG` rows, with no `DONE` row claimed by a wave.
- §9.2 lists the promotion candidates (rows that stop being differences if W10-A lands).
- §9.3 lists design-limited rows (no wave, recorded with a rationale) and the
  out-of-scope set that carries its ruling id.
- Biggest gap by far: **no settings subsystem** (48 design-differences + 6 backlog rows
  all trace back to it). Cheapest wave by value/effort: **W10-B**, 12 rows for two
  `ClickTarget` arms plus four CDV keys.

## T3 — Evidence cross-check: DONE

Machine-checked against the real sources, not against prose:

| Check | Result |
|---|---|
| Family row counts vs the contract (62/111/110/63/59/9 + 11 rulings) | PASS |
| No blank or ambiguous status cell | PASS (0) |
| §8 totals table equals the counts recomputed from the rows | PASS (179/112/66/68) |
| Every `snake_case` test name cited in the ledger exists as `fn` in `src/` | PASS — 111 distinct cites, 0 missing |
| Every `src/*.rs:line[-line]` anchor is inside the file's real length | PASS (12/12) |
| §9.1 wave ids == the `BACKLOG` set | PASS |

Mis-citations found and fixed by this check (rows demoted, never upgraded):
1. `render_smoke_paints_graph_glyphs` → `mock_layout_builds_every_row_without_panicking` (ACT-001)
2. `column_state_starts_at_the_p0_default` reused as a repo-state cite → `column_order_rows_switch_the_git_flag` (DSP-053)
3. `every_dispatch_row_cites_its_dsp_id` cited as if it pinned the DSP table → `dsp_table_matches_the_inventory`
4. `app_state_loads_the_merge_fixture_and_matches_the_layout` cited for the merge glyph → `demo_covers_every_special_vertex` (VIS-027)
5. `vis_047_a_binary_file_says_it_cannot_be_diffed` cited for a non-diff command → `every_pager_nav_is_reachable_from_a_key` (DSP-061)
6. `push_mode_select_maps_to_the_enum` reused for pull/fetch rows → `remote_surface_matches_the_inventory_rows` / `force_flags_flip_safe_ops_into_the_gate` (ACT-036/038)
7. `a_gated_row_stops_at_the_confirmation_stub` reused for merge/cherry-pick → `the_nine_surfaces_cover_a11_through_a73_once` / `destructive_ops_build_real_argv_but_are_gated` (ACT-014/027)
8. `reset_mode_and_flags_round_trip_into_the_target` cited for drop-commit → `destructive_ops_build_real_argv_but_are_gated` (ACT-028)
9. `keymap.rs:275-277` anchor off by two lines → `src/keymap.rs:273-276` (R1.2)

Layer-consistency corrections (the audit's main structural finding):

10. §2 `DSP-` rows were redefined to mean *"does the dispatch entry exist?"* — reachability
    moved wholly to §3 `ACT-`. `DSP-027 Fetch` is therefore `DONE` (a real, tested `fetch`
    plan) while `ACT-008 Fetch` is `BACKLOG` (no key binds it). This removed 10 false
    `BACKLOG`s from §2 and added them where they belong, in §3.
11. `MenuSurface::FileRow` (A65…A73) and `MenuSurface::Link` (A62…A64) are fully defined
    in `src/menu.rs` but `ClickTarget` (`src/app.rs:383-388`) has no arm for either, so no
    click ever reaches them. Six `ACT` rows were demoted to `BACKLOG` on this evidence
    (ACT-064, ACT-067, ACT-068, ACT-071, ACT-072, ACT-073) plus INT-029/INT-031.
    These are the W10-B wave.
    Rows demoted to `BACKLOG` by T3 in total: **8** (ACT-064/067/068/071/072/073,
    INT-029, INT-031); rows reclassified `BACKLOG` → `DESIGN-DIFFERENCE`: **4**
    (ACT-104, SET-053, SET-054, SET-055).
12. Three SET rows were recording a *missing checkbox* as if it were a *matching default*.
    The real defaults come from `ActionTarget::default()` (every bool `false`,
    `ResetMode::Mixed`, `PushMode::Normal`), so SET-024 (`noFastForward` = `true` in the
    reference), SET-031 (`ignoreDate` = `true`) and SET-035 (`includeUntracked` = `true`)
    are genuine behavioural differences, not just absent controls. `fields_for()`
    (`src/dialog.rs:531-580`) builds forms for only 11 actions, so several SET rows now
    say "no form field" instead of implying a control exists.
13. `ACT-104 Load More Commits`, `SET-053` and `SET-054/055` were reclassified from
    `BACKLOG` to `DESIGN-DIFFERENCE`: the terminal loads the whole history in one
    `git log` (`src/data.rs:206-221`) with no `--max-count`, so there is no paging to
    configure — a control would have nothing to do.
14. `INT-032`/`INT-033` were reworded to match the real router: a click on empty space
    falls through to a commit row (`surface_at_routes_rows_refs_and_empty_space`), and a
    graph vertex is never a hit target (M3 pointer-on-canvas ruling).

## Defect found (outside the parity contract — NOT fixed)

**AUDIT-001** — the *Clean Untracked Files* dialog offers a checkbox labelled
*"Also remove ignored files (-x)"* bound to `include_untracked`, but `plan()` for
`CleanUntrackedFiles` reads only `directories`; it never emits `-x`
(`src/dialog.rs:557-565` vs `src/actions.rs:594-603`). Ticking the box silently does
nothing. The same field name *is* honoured by `PushStash`, so one field means two things.
Recorded in ledger §8.1 and raised as Q1 in §10. Not fixed — it is a source change and
this wave is documentation-only.

## Gate

- Ledger complete: 425/425 rows classified, zero blanks. **PASS**
- Gaps list + W10 candidate plan written. **PASS**
- `commit docs(parity) on the branch`: **PENDING — awaiting the user's word.** Note
  `.IDE_Plans/` is in `.gitignore`, so this needs `git add -f` (as `w2-report.md` did).
  Nothing else in the worktree changed: `git status` on `ggt-m3-w9` is clean, and no
  `src/` file was touched.
- Rules honoured: no `src/` changes; only `parity-ledger.md` and `w9-report.md` written;
  reference tree read-only.
