# W12 WAVE VERIFICATION — independent verifier report

Range verified: `cf266d4..b6e3637` (15 commits, single wave) · Base: `development` @ `cf266d4`
Branch `ggt-m4-w12` · Worktree `../git-graph-terminal-w12`
Verifier: cline (headless, independent — NOT the author) · Date: 2026-10-13

Contracts read:
1. Tasks — `.IDE_Plans/w12-plan_v3.0.md` (in the main worktree; `.IDE_Plans/` is gitignored, so it is
   absent from this worktree's tree — see Item 0 below).
2. Design — `.IDE_Plans/mockups/w12_ui_mockup_v3.html` (HTML source read; same gitignore note).
Report under test — `.IDE_Plans/w12-report.md`.

Testing policy honoured: no `cargo test`, no test binaries run locally. Only `cargo fmt --check` was run
(PASS, exit 0). All pass/fail/panic evidence comes from remote CI via `gh`.

---

## Item 0 — Contract availability (read-only note)

`.IDE_Plans/` is `.gitignore`d (`.gitignore` line: `.IDE_Plans/`). In this w12 worktree only
`w12-report.md` and `parity-ledger.md` are present (force-added/tracked). `w12-plan_v3.0.md` and
`mockups/w12_ui_mockup_v3.html` are NOT in the worktree tree — they live in the main worktree
(`../git-graph-terminal/.IDE_Plans/`). I read both from there. **PASS** (contracts exist and were
read; the report's references are correct paths). This is a documentation-hygiene note, not a defect
in the shipped code.

---

## Per-item verdict table

| # | Item | Result | Evidence |
|---|---|---|---|
| 1 | Conformance spot-check (rows 1,2,5,9,11,13 + extras) | **PASS** | see §1 |
| 2a | Zero-width-rect fix: inline surfaces actually paint | **PASS** | see §2a |
| 2b | Editor test hermetic (temp files keyed per call) | **PASS** | see §2b |
| 2c | Ladder/glyph capability: no hardcoded Unicode on ASCII rung | **PASS** | see §2c |
| 3 | Remote CI matches the report (green tip + honest failure) | **PASS** | see §3 |
| 4 | No-regression audit (upgrade, remove nothing) | **PASS** (UD-001 story complete) | see §4 |
| 5 | UD-001…UD-008 present in ledger §11 and match report | **PASS** | see §5 |
| 6 | Frozen files: `data.rs` / `layout/` additive-only | **PASS** (untouched) | see §6 |
| 7 | Verdict written to `w12-verify.md` | **PASS** | this file |


---

## §1 — Conformance spot-check (report's 17-row table)

Every named test below was confirmed to exist in the tree at the cited `file:line` AND to assert what
the row claims. Six required rows (1, 2, 5, 9, 11, 13) plus extras are shown.

| Row | Report claim (abbrev) | Named test(s) | file:line | Asserts | Verdict |
|---|---|---|---|---|---|
| **1** | caret column: `▼` open, `▶` closed | `the_commit_row_ends_with_an_expansion_caret` | `src/render/mod.rs:1443` | closed row `trim_end().ends_with('▶')`, open row `ends_with('▼')` (L1449-1462) | **PASS** |
| 1 | same, ASCII rung | `the_trailing_caret_follows_the_glyph_rung` | `src/render/columns.rs:537` | `unlimited`→`▶`/`▼`; `mono_ascii`→`>`/`v`, asserts `!contains('▶')`/`!contains('▼')` (L557-565) | **PASS** |
| **2** | split-pane, taller column | `the_details_pane_paints_as_two_columns` | `src/render/mod.rs:1727` | left col `Commit: a1b2c3d`, right col `Files (2) — Tree View` same row, commit-info left of files (L1734-1746) | **PASS** |
| 2 | inline height = taller col | `the_inline_height_is_the_taller_column_not_their_sum` | `src/cdv.rs:1470` | `inline_height == left.max(right)+2` AND `< left+right+2` (L1483-1484) | **PASS** |
| **5** | uncommitted header above history | `the_uncommitted_section_paints_above_the_history` | `src/render/mod.rs:1633` | header + summary(`staged`/`unstaged`) + per-file rows + `header_at < commit_at` (L1637-1657) | **PASS** |
| 5 | Overview-only | `the_uncommitted_section_belongs_to_the_overview_tab` | `src/app.rs:3691` | `uncommitted_height()>0` on Overview, `==0` on Branches (L3699-3702) | **PASS** |
| **9** | diff paints under details | `the_file_diff_paints_under_the_details_panel` | `src/render/mod.rs:1773` | diff row `> commit_at` AND below the panel's `└` bottom border (L1788-1808) | **PASS** |
| 9 | hunk ranges | `a_hunk_header_parses_both_ranges` | `src/diff.rs:268` | `@@ -131,6 +131,20 @@`→`(131,131)`; `@@ -0,0 +1 @@`→`(0,1)`; non-header→`None` (L269-274) | **PASS** |
| 9 | gutters | `the_gutter_shows_the_right_number_per_side` | `src/diff.rs:257` | add `.gutter()=="+     2 "`, del `"-     2 "` (L260-264) | **PASS** |
| **11** | two tabs, `1`/`2`/`Tab` | `the_tab_keys_select_and_cycle` | `src/app.rs:3635` | `TabBranches`→Branches, `TabToggle` cycles, `TabOverview`→Overview (L3636-3646) | **PASS** |
| 11 | branch click filters | `a_branch_click_selects_and_filters` | `src/app.rs:3002` | present; real repo, ref resolves | **PASS** |
| 11 | tag row filters | `a_tag_row_filters_the_history_too` | `src/app.rs:3752` | builds `refs/tags/v1`, `visible_rows()==vec![1]` (L3766-3767) | **PASS** |
| **13** | diff classes + editor | `the_header_and_hunk_lines_are_classified` | `src/diff.rs:224` | lines[0..3] Header, lines[4] Hunk (L226-230) | **PASS** |
| 13 | editor argv | `the_argv_is_editor_dash_d_then_the_two_files` | `src/editor.rs:144` | `plan_argv == ["nvim","-d",old,new]` (L145-146) | **PASS** |
| 13 | resume state | `staging_the_editor_leaves_the_view_untouched` | `src/app.rs:4017` | view tuple identical before/after `EditorOpen` (L4044-4048) | **PASS** |
| 14 | mouse + ladder (extra) | `the_wheel_scrolls_the_surface_under_the_pointer`, `surface_at_routes_the_branch_pane_before_the_body`, `the_inline_surfaces_follow_the_glyph_rung` | `src/app.rs:3051`, `src/app.rs:2948`, `src/render/mod.rs:1470` | all present | **PASS** |
| 16/17 | tab-bar + status-bar (extra) | `every_column_of_a_tab_label_maps_back_to_that_tab`, `a_tab_separator_is_not_a_tab`, `the_drawn_tab_bar_matches_the_click_ranges`, `summary_is_one_deterministic_line` | `src/render/mod.rs:1819/1828/1842`, `src/term/mod.rs:470` | all present | **PASS** |

Cross-checked all 17 rows against the actual `w12_ui_mockup_v3.html`: tab bar labels + sub-labels
(HTML L162-163), status bar `commits/uncommitted/branches/sel` (L171), caret `▼`/`▶` (L383), refs line
`┌ refs │` (L325), split-pane `.pane .col` two-column grid (L342-356), legend `A/M/D/R` incl. `R`

---

## §2 — The three gate-caught defects (fixed in code, not just described)

### 2a. Zero-width rect — inline surfaces actually paint. **PASS**
- The fix commit `844ea92` changed the call site: `draw_inline_blocks` used to receive the narrow
  graph column; it now receives `Rect { width: area.width, ..body }` (git show `844ea92`,
  `src/render/mod.rs` graph_view hunk: `-draw_inline_blocks(frame, body, graph_width, …)` →
  `+draw_inline_blocks(frame, Rect { width: area.width, ..body }, …)`).
- The callee subtracts the graph width itself and guards zero: `src/render/mod.rs:1078-1082`
  `let x = body.x + graph_width; let width = body.width.saturating_sub(graph_width); if width == 0 || body.height == 0 { return; }`.
- Four `ui`-level `TestBackend` draw tests exist and read the framebuffer (the only things that can
  see this bug): `the_uncommitted_section_paints_above_the_history`,
  `the_details_panel_paints_under_its_row` (`mod.rs:1663`), `the_details_pane_paints_as_two_columns`,
  `the_file_diff_paints_under_the_details_panel`. Genuinely fixed — the surfaces paint.

### 2b. Hermetic editor test — temp files keyed per call. **PASS**
- Fix commit `39a453c` (git show `src/editor.rs`): added `static NEXT_STAGE: AtomicU64` and
  `let seq = NEXT_STAGE.fetch_add(1, Ordering::Relaxed);` — temp name is now
  `ggt-diff-{pid}-{seq}-{safe}.old/.new`, keyed on the **call**, not the process.
- Regression test forces the interleaving: `two_stages_in_one_process_never_share_a_path`
  (`src/editor.rs`, added in `39a453c`) — stages twice, asserts the two paths differ, reads first pair last.
- No unwrap on environment paths in the editor flow: `program_from_env` (`src/editor.rs:62`) uses
  `std::env::var(ENV_VAR).ok().map(trim).filter(non-empty)`; the env test
  `the_editor_env_var_turns_the_fallback_on` (`src/app.rs:3944`) clears the var first
  (`remove_var` L3946) and again at the end (L3961). The old flake site `app.rs:3990`
  `read_to_string(&plan.old_file).unwrap()` is a *staged temp file* whose path is now per-call unique,
  so a peer test's `cleanup` can no longer delete it mid-read. Hermetic.

### 2c. Ladder/glyph capability — no hardcoded Unicode on the ASCII rung. **PASS**
- `Ladder::disclosure` (`src/term/ladder.rs:423-430`) returns `▼`/`▶` on `GlyphTier::Unicode` and
  `v`/`>` below it — a single rung-aware source, not a literal.
- `inline_blocks` composes from `ladder.glyphs()` (`src/render/mod.rs:201`); `tree_glyph`
  (`src/render/mod.rs:328-330`) uses `glyphs.sharp_corner(Corner::UpRight)` etc; the framed block uses
  `view.ladder.border_set()` (`src/render/mod.rs:1120`).
- Pinned by `the_inline_surfaces_follow_the_glyph_rung` (`src/render/mod.rs:1470`): Unicode rung keeps
  `┌ refs │` / `┌─ M `; ASCII rung yields `+ refs |` / `+- M `, and asserts NO box-drawing char
  survives — `for row in &ascii { assert!(!row.contains(['┌','│','├','└','─'])) }` (L1492-1497); also
  `├─ Commit Details` → `+- Commit Details` (L1508-1515). No hardcoded Unicode on the ASCII rung.


---

## §3 — Remote CI matches the report (no overclaim)

`gh run list --repo Adi-Mudi/git-graph-terminal --branch ggt-m4-w12`:

| Run id | Commit | Report says | `gh` shows | Match |
|---|---|---|---|---|
| `38315859919` | `b6e3637` (tip) | awaiting its run | **success** — 3/3 jobs ✓ | **PASS** (conservative, not overclaimed) |
| `38311439228` | `6f5b2be` | GREEN, incl. 2 new pty inline assertions | **success** — 3/3 jobs ✓ | **PASS** |
| `38308943519` | `e7337fb` | GREEN (40s/28s/26s) | **success** — 3/3 jobs ✓ | **PASS** |
| `38300733488` | `ffb4e01` | FAILED — `app::tests::the_editor_key_stages_the_argv_without_launching` panicked `src/app.rs:3991` `unwrap()` `Os{code:2,NotFound}`; 336 passed/1 failed; both pty jobs passed | **failure** — `Test` step red; log: `panicked at src/app.rs:3991:53 … NotFound`; `test result: FAILED. 336 passed; 1 failed`; `pty E2E` both green | **PASS** — honest, exact |
| `38297846663` | `e5af02a` | GREEN | **success** — 3/3 jobs ✓ | **PASS** |

Test-count reconciliation (report's "703 → 847, W12 adds 73"):
- Green run `38311439228` (`6f5b2be`): 353+353 bin/app_state + 15 golden + 64 integration + 62 perf =
  **847**. Matches the report exactly.
- Failure run `38300733488` (`ffb4e01`): `running 337 tests` → `336 passed; 1 failed`. Matches.
- Source-level `#[test]`/`#[tokio::test]` attrs: `git grep` HEAD = **366**, `development` = **293** →
  **+73** new tests. Matches "73 new tests". (CI's 847 double-counts `app.rs` across the bin and
  app_state harnesses; both figures are internally consistent.)
- Tip run `38315859919` (`b6e3637`) totals: 354+354 + 15 + 64 + 62 = 850 (the editor regression test
  `two_stages…` added in `39a453c` is present here). Green. The editor-fix commit `39a453c` has no
  dedicated run, but it is the tip's parent and is thus CI-validated on `b6e3637`.

The report does **not** overclaim CI: it explicitly records the one red run and its exact cause, and
says `39a453c`/tip were "awaiting their run" — conservative, and the tip is now green.

---

## §4 — No-regression audit ("upgrade, remove nothing")

Intact / still present (git grep on HEAD):
- **DSP_TABLE 62 rows**: `src/actions.rs:90` `pub const DSP_TABLE: [(&str,&str,&str); 62]`, rows
  DSP-001…DSP-062 (L91-156). **PASS.**
- **`src/actions.rs` untouched**: `git diff development..HEAD -- src/actions.rs` = empty. **PASS.**
- **find**: `Action::FindOpen/Close/Next/Prev/ToggleCase/ToggleRegex/ToggleCdv` (`src/app.rs:278, 1352-1359`). **PASS.**
- **context menus**: `ContextMenu`, `MenuSurface`, `menu_context` (`src/app.rs:190, 921-951`). **PASS.**
- **mouse**: `MouseEventKind::ScrollUp|ScrollDown` routing (`src/main.rs:392-401`), `surface_at`
  (`src/render/mod.rs`, `src/app.rs`), wheel test present. **PASS.**
- **uncommitted**: `uncommitted_height` (`src/app.rs`, `src/render/mod.rs`), `src/uncommitted.rs`. **PASS.**
- **capability ladder**: `GlyphTier`, `mono_ascii`, `border_set` (`src/term/ladder.rs`, `src/render/…`). **PASS.**
- **dumb fallback**: `GlyphTier::Ascii`/`Dumb` → `mono_ascii` (`src/term/ladder.rs:290-350`). **PASS.**
- **pager**: `mod pager;` (`src/main.rs:41`), `Pager::new` (`src/pager.rs`), `p` key preserved. **PASS.**

Things removed, and whether superseded:
1. **`+`/`-` resize + `Action::CdvResizeUp`/`CdvResizeDown` + `CdvState::height` / `MIN_HEIGHT` /
   `MAX_HEIGHT` / `DEFAULT_HEIGHT` / `CdvState::resize` / `resize_cdv` (UD-001).**
   - Diff confirms removal: `src/app.rs` removes `CdvResizeUp/Down` variants (L154/156), the
     `CdvResizeUp => resize_cdv(state, 2)` / `Down => -2` arms (L664-665), `fn resize_cdv` (L966), and
     the `+`/`=`→`CdvResizeUp`, `-`/`_`→`CdvResizeDown` keymap entries (L2100-2101, 2170-2171).
   - `git grep CdvResize|MIN_HEIGHT|MAX_HEIGHT|DEFAULT_HEIGHT` on HEAD = **no hits** (only
     `inline_height` remains). API genuinely gone, not merely re-pointed.
   - **Is that the whole story?** Yes. The docked panel it resized no longer exists (T3 made it an
     inline block sized per frame by `cdv::inline_height` at `src/cdv.rs:894`, reserved by
     `AppState::inline_cdv_height` at `src/app.rs:781`). The one live reader, `cdv_file_viewport`
     (`src/app.rs:1793`), was re-pointed to ask `inline_height`. The capability has no target left
     because the panel is inline; it is superseded, not dropped from the product. Ledger UD-001
     records this and corrects the two citing parity rows (ACT-110, INT-046) to keep `NOT-PORTED`
     with updated reason text. **Judged: legitimately superseded.**
2. **Commit-table `refs` column (UD-002).** `FIXED_HEADERS` unchanged at `["idx","hash"]`
   (`src/render/columns.rs:29`); refs render on their own line above the commit. Superseded, not
   removed from the product. **Legitimately superseded.**
3. **`Tab` re-bind (UD-001-adjacent).** `Tab` moved from `CdvFocusToggle` to `TabToggle`; `Shift+Tab`
   (`BackTab`) keeps `CdvFocusToggle` (`src/keymap.rs:265`, `src/app.rs:1344`). Both retained. **Superseded.**
4. **`Enter`/`p` swap on file rows.** `Enter` opens inline diff; `p` keeps the full-screen pager. Both
   reachable. **Superseded.**

No feature was removed without a superseding path. Nothing else was deleted. **PASS.**


---

## §5 — UD-001…UD-008 in `parity-ledger.md` §11

`## 11. W12 design differences — UD-001…` at `parity-ledger.md:652`; `<!-- LEDGER-END -->` at L671.
All eight rows present (L662-669) and each matches the report's §Ledger table:

| ID | Ledger §11 | Report | Match |
|---|---|---|---|
| UD-001 | resize API gone, `cdv_file_viewport` fixed | same | **PASS** |
| UD-002 | no `refs` column; refs on own line | same | **PASS** |
| UD-003 | keep field order/labels; add Branch/Tags; right header `Files (N) — Tree View` | same | **PASS** |
| UD-004 | right column no wrap; dim summary clips < ~126 cols | same | **PASS** |
| UD-005 | branch filter is a tab, not a dropdown; capability present | same | **PASS** |
| UD-006 | dedicated uncommitted block vs `UNCOMMITTED` pseudo-row | same | **PASS** |
| UD-007 | diff parsed/rendered in-terminal vs VS Code diff editor | same | **PASS** |
| UD-008 | `e` → `$GGT_EDITOR` diff mode, inert when unset | same | **PASS** |

§8 totals unchanged: 425 rows (187 DONE / 112 absent by ruling / 66 design differences / 60 real gaps);
W12 adds no parity row and closes none. **PASS.**

---

## §6 — Frozen files: `src/data.rs` and `src/layout/`

`git diff development..HEAD -- src/data.rs src/layout/mod.rs src/layout/types.rs | wc -l` = **0**.
Both frozen files are **byte-identical to base** — i.e. untouched, which trivially satisfies the
"additive-only" rule (there are no changes at all, additive or otherwise). Stronger than the report's
claim ("`src/data.rs` is not in this branch's diff"). **PASS.** Exactly what changed: nothing.

---

## Retrospective

**What went well**
- The four `ui`-level `TestBackend` tests are real framebuffer assertions — the only guard that can
  see the zero-width-rect class of bug, and they exist (§2a).
- The editor fix is at the source (per-call atomic counter), not in the two tests, so the next caller
  cannot reintroduce the collision; a dedicated test forces the interleaving (§2b).
- The ladder fix is a genuine rung-aware source (`Ladder::disclosure`) with an ASCII-rung assertion
  that greps for surviving box-drawing chars (§2c).
- The report is honest about CI: the one red run and its exact panic are recorded, not hidden (§3).
- Frozen-file discipline held with margin — `data.rs`/`layout/` are byte-identical, not merely
  additive (§6).

**What failed / limits of this verification**
- `.IDE_Plans/` is gitignored, so the plan and mockup contracts are not in the worktree tree; I had to
  read them from the main worktree. Documentation-availability friction, not a code defect.
- I could not run the test suite locally (thermal policy); every pass/fail claim rests on remote CI
  logs via `gh`, which I verified directly rather than trusting the report's summary.

**What to improve**
- The report's phrase "703 → 847, W12 adds 73" mixes two units (CI harness total vs source attr
  count). Both numbers are individually correct and I reconciled them (847 = CI harness sum; 73 =
  366−293 source attrs), but the sentence could state both units so a reader does not expect
  847−703 to equal 73.

---

VERDICT: ACCEPT (defects: 0)

renamed (L193), `e` Neovim fallback note (L335). No row's evidence fails to hold.
