# M3-W10 — Report: AUDIT-001 fix + CDV file-row menu reachability

Base: `development` @ 6d1d3a3 (W9 docs commit) · Branch `ggt-m3-w9` · Worktree `../git-graph-terminal-w9`
Source commit: `3eb6102` · Merge commit: `bfdb94f`
Artifacts: `.IDE_Plans/parity-ledger.md` (row flips + §8 recompute + §8.1/§9.1/§10 updates)

> **Lane correction.** The plan named branch `ggt-m3-w10` / worktree `../git-graph-terminal-w10`.
> Neither was ever created — the W9 lane (`ggt-m3-w9`) was continued as the W10 lane, which the
> user confirmed in the pre-execution scope check. No file, phase or deliverable changed.

## Reply: DONE — counts

W10-B closed completely: **8 of 8 rows**, plus the AUDIT-001 defect.

| Status | Before | After | Δ |
|---|---|---|---|
| **DONE** | 179 (42%) | **187 (44%)** | **+8** |
| NOT-PORTED | 112 | 112 | — |
| DESIGN-DIFFERENCE | 66 | 66 | — |
| BACKLOG | 68 | **60** | **−8** |
| **Total** | **425** | **425** | — |

Per family:

| Family | Rows | DONE | NOT-PORTED | DESIGN-DIFFERENCE | BACKLOG |
|---|---|---|---|---|---|
| R1–R6 rulings register | 11 | 11 | 0 | 0 | 0 |
| DSP dispatch map | 62 | 49 | 11 | 2 | 0 |
| ACT commands/actions | 111 | **52** | 20 | 5 | **34** |
| SET settings | 110 | 0 | 56 | 48 | 6 |
| INT interactions | 63 | **30** | 8 | 8 | **17** |
| VIS visual states | 59 | 43 | 10 | 3 | 3 |
| DRIFT | 9 | 2 | 7 | 0 | 0 |

## Phase 1 — AUDIT-001: make the `(-x)` checkbox real: DONE

The *Clean Untracked Files* dialog offered a checkbox labelled *"Also remove ignored files (-x)"*
bound to `include_untracked`, and `plan()` read only `directories` — a live no-op control.

`plan()` now matches all four independent clusters (`src/actions.rs:605-613`):

| `directories` | `include_untracked` | flag |
|---|---|---|
| false | false | `-f` |
| true | false | `-fd` |
| false | true | `-fx` |
| true | true | `-fdx` |

Test `clean_untracked_honours_directories_and_ignored_files` pins all four. `PushStash` already
read the same field this way, so the stash meaning is unchanged and one field no longer means two
things. Ledger §8.1 now carries the finding as `FIXED` with the commit and the test.

## Phase 2 — `ClickTarget::CdvFile`: teach the geometry about the CDV: DONE

- `ClickTarget` gained `CdvFile(String)` and `CdvFolder(String)` (`src/app.rs:409`, `:415`).
- `HitGeometry` gained `cdv: Option<Rect>` (`src/app.rs:395`); `hit_geometry` now takes the screen
  height and returns the panel rect, because the panel is the only overlay whose height the mouse
  router cannot recover from `AppState` alone.
- `AppState::cdv_file_at` (`src/app.rs:875`) maps a screen cell to a `FileRow` from `scroll` plus
  the summary height — the same maths `ensure_file_visible` uses, so the cursor and the pointer
  cannot disagree.

### Flag A — found and fixed (real defect, not in the plan)

`hit_geometry` derived the body as `status.height + 1`. That ignores the docked CDV **and** the
find bar entirely, so the commit-row region covered both overlays: a click on a file row was read
as a click on a commit, and a click on the status bar too. It is now
`screen − cdv − find − status`, and is a no-op when no overlay is open — which is why every
pre-existing `surface_at` test still passes unchanged.

## Phase 3 — route the click: open the `FileRow` menu: DONE

- `open_menu_at` (`src/main.rs:296`) routes `CdvFile` → `MenuSurface::FileRow`; the left-click arm
  (`src/main.rs:267-283`) moves the CDV cursor via `CdvState::select_path` (`src/cdv.rs:339`).
- `CdvFolder` left-click → `toggle_cdv_folder` (`src/app.rs:1217`); right-click opens nothing
  (the `FileRow` menu is file-only) but still must not fall through to the commit-row menu.

### Flag C — found and fixed (real defect, not in the plan)

`MenuContext.path` was **never populated for the `FileRow` surface** — the field was only ever set
by a test literal. Without the fix all nine A65…A73 rows would have rendered disabled even once a
click could reach them, i.e. the plan's Phase 3 would have shipped a menu that looks reachable and
is not.

## Phase 4 — the five missing CDV file keys: DONE

| Key | Action | Inventory | Notes |
|---|---|---|---|
| `w` / `W` | `CdvFileViewDiffWithWorkingFile` | A67, DSP-060 | diffs the working tree against the panel's commit |
| `e` / `E` | `CdvFileOpen` | A68, DSP-038 | opens the pager (`show :<path>`) |
| `R` | `CdvFileResetToRevision` | A71, DSP-049 | gated; shifted on purpose — a stray press must not arm a working-tree overwrite |
| `y` | `CdvFileCopyAbsolutePath` | A72, DSP-011 | payload is the repo-joined path |
| `Y` | `CdvFileCopyRelativePath` | A73, DSP-011 | payload is the repo-relative path |

All five route through the existing `cdv_file_target` + `actions::plan`, so **no new argv was
introduced** — the plans already existed and were already tested; only the entry points were
missing.

### Flag B — found and fixed (real defect, not in the plan)

A72 and A73 both called `CopyFilePath` with the **same** payload, so the two rows were
indistinguishable — "absolute" copied a relative path. `cdv_copy_text` now repo-joins for `y` and
passes the path through for `Y`, reusing the existing `ActionTarget::clipboard` seam. The function
was split out of `cdv_copy_path` so the absolute-vs-relative rule is testable without touching the
real clipboard.

## Phase 5 — verify, record, commit: DONE

- **5.1** 8 rows flipped `BACKLOG` → `DONE` with code + test cites; §8 recomputed; §9.1 heading,
  wave counts and Total updated so the wave set still equals the `BACKLOG` set; a revision note
  records the closure.
- **5.2** §8.1 `AUDIT-001` marked `FIXED` in `3eb6102` with the test that pins it.
- **5.3** Gate PASS — see below.
- **5.4** Source committed as `3eb6102`; merged to `development` as `bfdb94f`; docs committed
  separately, matching the W9 precedent (`6d1d3a3`).
- **§10** (user-approved addition) Q1 and Q2 marked ANSWERED with their resolutions.

### Flag F — Subphase 5.3 named a checker that did not exist

The plan said "rerun the W9 ledger checker". W9 ran its cross-check ad hoc; there is no script in
the tree. It was re-implemented for this gate (see the table below).

## Planning contradiction found and resolved (user decision)

**Subphase 2.5 and Subphase 5.1 contradicted each other on INT-050.**

- 2.5 specified `FileRowKind::Folder { .. } => None` — a folder row resolves to nothing.
- 5.1 listed **INT-050 "click folder (CDV tree)"** among the rows to flip to `DONE`.

The reference *does* bind a folder click: `web/main.ts:2968-2978` listens on `.fileTreeFolder` and
calls `alterFileTreeFolderOpen`. So 5.1 was right and 2.5 was wrong. Per Rule 2 this was raised
rather than resolved silently; the user chose to add the behaviour, so `ClickTarget::CdvFolder` +
`toggle_cdv_folder` were implemented and INT-050 closed **for real** rather than the row being
marked `DONE` without it. This is the only addition to Subphase 2.1's specified surface.

## Gate

| Check | Result |
|---|---|
| `cargo test` (merged base) | **PASS** — 693 tests, 0 failed (274 + 280 + 15 + 63 + 61) |
| `cargo clippy --all-targets -- -D warnings` | **PASS** |
| `cargo fmt --check` | **PASS** |
| Binary smoke (`--dump --mock`) | **PASS** (exit 0) |
| Ledger rows / no blank status | **PASS** — 425 rows, 4 allowed values only |
| §8 table == counts recomputed from the rows | **PASS** — 187/112/66/60 |
| §9.1 wave ids == the `BACKLOG` set | **PASS** (60/60) |
| §9.1 wave counts sum | **PASS** (60) |
| Every cited `snake_case` test exists as `fn` in `src/` | **PASS** — 107 distinct cites, 0 missing |
| Named gates still green | **PASS** — `every_pager_nav_is_reachable_from_a_key`, `shortcuts_are_unique_and_lowercase_per_surface`, `surface_at_routes_rows_refs_and_empty_space`, `the_nine_surfaces_cover_a11_through_a73_once`, `destructive_ops_build_real_argv_but_are_gated` |

## Rule 12 — plan verification

Each item read back from the plan file and checked against the real sources; every `TO` present and
every `FROM` gone.

| Item | Result |
|---|---|
| Phase 1 `CleanUntrackedFiles` FROM gone / TO present | PASS |
| Phase 2 `ClickTarget::CdvFile`, `HitGeometry.cdv`, `hit_geometry` 3-arg, `cdv_file_at` | PASS |
| Phase 2 `body.height` FROM gone | PASS |
| Phase 3 `open_menu_at`, `select_path`, `MenuContext.path` | PASS |
| Phase 4 five `Action` variants, `cdv_key_for` arms, route table, `app.rs` handlers | PASS |
| Phase 4 `cdv_copy_text` split | PASS |
| Phase 5.1/5.2 ledger flips | PASS |

Three apparent failures during this check were **my own grep patterns**, not plan gaps, and are
recorded so the check is not re-litigated:

1. `FileRowKind::Folder { .. } => None` still exists at `src/cdv.rs:355` — that is
   `CdvState::selected_change()`, file-only by design, unrelated to `cdv_file_at`.
2. `status.height` still appears — in a comment, not in the geometry.
3. `pub cdv: Option<…>` is at `src/app.rs:395`; `HitGeometry` lives in `app.rs`, not
   `render/mod.rs`.

## Rules honoured

- All changes scoped to the project directory; reference tree (`vscode-git-graph/`) read-only.
- No dependency added; no install run; no environment or system setting touched.
- No secret in any diff (scanned before each commit).
- Every deletion-free wave: no file was removed, so no delete confirmation was needed.
- Commits made only on the user's word; merges only on the user's confirm.
