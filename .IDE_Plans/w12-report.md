# M4-W12 — Report: UI upgrade (tabs, branches, inline details, uncommitted, diff, editor)

Base: `development` @ cf266d4 (released v0.3.0) · Branch `ggt-m4-w12` · Worktree `../git-graph-terminal-w12`
Plan: `.IDE_Plans/w12-plan_v3.0.md` · Design contract: `.IDE_Plans/mockups/w12_ui_mockup_v3.html`
Artifacts: `src/app.rs`, `src/branches.rs`, `src/cdv.rs`, `src/diff.rs`, `src/editor.rs`,
`src/keymap.rs`, `src/main.rs`, `src/render/{mod,columns,graph_widget}.rs`, `src/term/ladder.rs`,
`src/uncommitted.rs`, `tests/*`, `.github/workflows/ci.yml`, `.IDE_Plans/parity-ledger.md` §11

14 commits on the branch (13 before this report), 18 files, +5378 / −434.

## Reply: DONE — the eight tasks

| Task | What shipped | Where |
|---|---|---|
| **T1** Two tabs | `Overview` / `Branches` only, `1` / `2` / `Tab` or a click on the label | `src/keymap.rs`, `src/render/mod.rs` `tab_bar` |
| **T2** Branches tab | Local / remote / tag groups, HEAD marker, commit counts; select filters the history; right-click keeps the ref menu | `src/branches.rs`, `src/app.rs` `apply_branch_filter` |
| **T3** Inline details | The details expand under the commit's own row, split into the mockup's two columns; following rows shift down | `src/cdv.rs` `left_column` / `right_column`, `src/app.rs` `block_parts` |
| **T4** Uncommitted section | Header with caret, status counts, staged/unstaged split, per-file `+add / −del`, tree glyphs | `src/uncommitted.rs`, `src/render/mod.rs` `inline_blocks` |
| **T5** In-terminal diff | Hunks, both line-number gutters, green adds / red removes / dim context, scrollable, `Esc` back | `src/diff.rs` |
| **T6** Editor fallback | `e` stages both revisions and runs `$GGT_EDITOR -d old new`, then resumes at the same view/row/scroll | `src/editor.rs`, `src/main.rs` `run_editor` |
| **T7** R1 items kept | Refs on their own line above the commit, expansion caret column, mouse across every surface, ladder respected | `src/render/mod.rs`, `src/term/ladder.rs` `disclosure` |
| **T8** Tests, evidence, ledger | 73 new tests, pty KEYS evidence per item, `UD-001…UD-008`, this report | `src/**`, `.IDE_Plans/parity-ledger.md` §11 |

## Phase 1 — T1 + T2: tabs and the branch pane: DONE

Two tabs, and only two. The tab bar is drawn on every frame, so the body geometry
shifts down one row — which is why `surface_at` checks the tab bar *before* the body
(`surface_at_reads_the_tab_bar_before_the_body`) and the branch pane *before* the body
(`surface_at_routes_the_branch_pane_before_the_body`).

`Tab` used to cycle the docked panel's focus. The panel is inline now, so there is no
dock to cycle: `Tab` switches tabs and `Shift+Tab` (`BackTab`) keeps the old
`CdvFocusToggle`. That is a deliberate re-bind, recorded here because it changes a
keybinding the W7 docs still listed.

The pane needs both the Branches tab *and* a wide terminal
(`the_branch_pane_needs_the_branches_tab_and_a_wide_terminal`); a narrow terminal drops
it rather than crushing the history (`a_narrow_terminal_drops_the_branch_pane`). Click
(`a_branch_click_selects_and_filters`), `Enter` (`the_enter_key_filters_the_history_to_a_branch`)
and tags (`a_tag_row_filters_the_history_too`) all filter. A refname that does not resolve
reports in the status line instead of blanking the list
(`an_unresolvable_refname_reports_instead_of_filtering`).

## Phase 2 — T3: the details pane moves inline: DONE

The biggest single change. The panel used to be docked across the bottom of the area;
it is now an inline block whose rows the commit row reserves, so following rows shift
down and scroll/selection stay put (`expanding_a_row_leaves_scroll_and_selection_alone`,
`collapsing_a_row_restores_the_geometry`, `an_expansion_taller_than_the_viewport_scrolls_just_enough`).

The mockup lays the block out as `.pane{grid-template-columns:1fr 1fr}` — commit
information left, modified files right. The first pass drew the two stacked in one
column; `left_column` / `right_column` / `file_lines` split them, and `inline_height` is
now the taller column rather than their sum (`the_inline_height_is_the_taller_column_not_their_sum`).
One layout path at every width keeps `inline_height` a pure function of the content.

The left column gained the mockup's `Branch` / `Tags` rows, read from the commit's `%D`
decorations and split by `branches_label` / `tags_label`, with a dash when a commit is on
no branch or has no tag (`the_summary_carries_the_mockup_branch_and_tags_rows`,
`a_commit_with_no_refs_shows_a_dash_for_branch_and_tags`). The right column ends with the
mockup's totals line and the diff-summary sentence; a binary file counts towards the file
total but adds nothing to the line counts (`the_totals_sum_text_files_and_skip_binaries`).

Two readers were stale and are fixed here rather than left to rot:
`cdv_file_viewport` scrolled the file cursor against the summary's header depth and now
asks the right column, and the old docked-height API is gone (see UD-001 below).

## Phase 3 — T4 + T7: the uncommitted block and the refs line: DONE

`src/uncommitted.rs` parses `git status --porcelain=v2 -z --find-renames` and the two
`--numstat` runs. `-z` means NUL-terminated fields, so a rename consumes the *next* NUL
field for its destination path (`a_rename_keeps_both_paths_and_reads_the_next_nul_field`)
and a path with a space survives (`paths_with_spaces_survive`). A file that is both staged
and unstaged says so (`a_staged_and_unstaged_file_says_both`), `--numstat`'s `-` for a
binary counts as zero (`numstat_binary_dashes_count_as_zero`), and an empty working tree
is an empty section, not an error (`an_empty_section_is_empty`).

The block paints above the history (`the_uncommitted_section_paints_above_the_history`)
with the mockup's collapse caret, and each file row opens with the mockup's tree glyph —
`┌─` first, `├─` between, `└─` last (`an_uncommitted_file_row_reads_tree_letter_path_stage_counts`).

T7 put each commit's refs on their own display row above it and dropped the table's `refs`
column, which had become a duplicate. The graph widget and the commit table both gained a
per-row `refs` offset so the two agree on where the commit line sits
(`the_refs_line_sits_above_its_commit_row`, `leading_rows_push_the_graph_down`). Every
commit row now ends with the mockup's caret column
(`the_commit_row_ends_with_an_expansion_caret`).

## Phase 4 — T5 + T6: diff and editor: DONE

`src/diff.rs` parses a unified diff itself: header and hunk lines classified
(`the_header_and_hunk_lines_are_classified`), both `@@` ranges parsed
(`a_hunk_header_parses_both_ranges`), old/new line numbers walked per side
(`line_numbers_walk_old_and_new_sides`, `the_gutter_shows_the_right_number_per_side`), a
binary diff is header-only (`a_binary_diff_is_header_only`). The render side paints it
under the details panel (`the_file_diff_paints_under_the_details_panel`).

`Enter` on a file row now grows the diff in place; the old full-screen pager moved to `p`
so both stay reachable, and the keymap doc table was updated in the same commit.

T6 is deliberately opt-in. `program_from_env` reads `$GGT_EDITOR`; unset *or* blank is off,
and the value is trimmed (`the_editor_env_var_turns_the_fallback_on`). With it unset, `e`
only hints in the status line and nothing is launched (`without_an_editor_program_the_key_only_hints`).
With it set, `stage` writes both revisions to `ggt-diff-<pid>-<label>.old/.new` in the OS
temp directory, `plan_argv` builds `<program> -d <old> <new>`, and the launch is *staged*,
never performed, so the test asserts the exact argv without spawning anything
(`the_argv_is_editor_dash_d_then_the_two_files`, `the_editor_key_stages_the_argv_without_launching`).
`cleanup` removes both temp files and ignores "already gone" (`staging_writes_both_sides_and_cleans_up`),
and the label is sanitised so a path with a space or a slash cannot escape the prefix
(`the_temp_name_sanitises_the_label`). A spawn failure lands in the status bar, never a crash.

## The three defects the gates caught

All three were found by a gate during the work, not by review afterwards. Each is listed
with the artifact that failed, because a defect that no gate can see is not really fixed.

**1. Zero-width rect — the inline surfaces were built but never reached the screen.**
`draw_inline_blocks` was handed the *graph* column, so every block's width computed to
zero and nothing was drawn. The rows were still reserved, so the list showed blank gaps
under an expanded commit and no uncommitted block at all. Caught by the four new `ui`-level
tests driving a real `TestBackend` draw — all four were red before the fix, which is the
only reason the bug was visible: nothing else in the suite reads the framebuffer. Fixed in
`844ea92` by passing the full body width and letting the blocks subtract the graph width.
Before/after frames: `target/pty/w12-overview-mock.frame` (blank rows) vs
`target/pty/ci-xterm-256color.frame` (populated).

**2. Non-hermetic test — the editor tests shared one temp path.** Remote CI run
`38300733488` (commit `ffb4e01`) went **red** on the job `fmt + clippy + test`:
`app::tests::the_editor_key_stages_the_argv_without_launching` panicked at `src/app.rs:3991`
on `unwrap()` of `Err(Os { code: 2, kind: NotFound })` — the staged temp file was gone.
336 passed, 1 failed; both `pty E2E` jobs passed. `editor::stage` named its files
`ggt-diff-<pid>-<label>.old/.new`, and every test in one binary shares the pid, so
`the_editor_key_stages_the_argv_without_launching` (repo `editor-on`) and
`staging_the_editor_leaves_the_view_untouched` (repo `editor-resume`) — both selecting
`f.txt` — computed the *same* path. The resume test ends with `editor::cleanup`, which
deletes both files, so it could delete the other test's file between that test's `stage`
and its `read_to_string`. It passed locally, and passed on the run before it and the two
runs after, which is what makes it the clearest case in this wave of a result that depended
on the machine rather than on the code. Fixed in `39a453c` by keying the name on the *call* —
a process-wide counter — instead of on the process, so the next caller cannot reintroduce
it; `two_stages_in_one_process_never_share_a_path` forces the failing interleaving.

The same class, milder, in `the_editor_env_var_turns_the_fallback_on`: its first assertion
was "unset is off", true only if the machine has not exported `GGT_EDITOR`, and its
`set_var` leaked into peer tests because Rust test threads share one process. That one was
already self-owned end to end in `1c43a84` — it clears the variable before the first
assertion and again at the end.

**3. Ladder violation — the W12 surfaces spelled their box drawing out.** `┌ │ ├ └ ─ ▼ ▶`
were literals in the new render code, so a terminal below the Unicode rung — `TERM=dumb`, or
a CJK locale whose ambiguous-width glyphs make the graph's lanes drift — received glyphs it
cannot draw. Every other surface in the app asks the ladder for these; these five did not.
Caught by the evidence gallery, which renders the demo frame at *both* rungs, and now pinned
by `the_inline_surfaces_follow_the_glyph_rung`. Fixed in `8c30038`: `Ladder::disclosure`
returns `▼` / `▶` on the Unicode rung and `v` / `>` below it, `inline_blocks` and `tree_glyph`
compose their corners and tees from `GlyphSet`, and the caret column follows the rung
(`the_trailing_caret_follows_the_glyph_rung`). The Unicode rung is byte-identical to before —
this is a fallback, not a restyle. The ASCII frame is now the gallery's default evidence:
`target/pty/ci-vt100.frame`.

Left alone deliberately: `cdv.rs`'s tree-view folder caret predates W12 (it arrived with
W7/T3), and `·` / `−` / `—` are not ladder glyphs anywhere in the app.

## Mockup conformance — `w12_ui_mockup_v3.html`, row by row

Every surface the mockup draws, in document order, numbered 1..17. Rows 5–13 are the Q2 and
KEPT bullets; rows 16–17 are the chrome the mockup draws around them.

| # | Mockup says | Terminal does | Evidence |
|---|---|---|---|
| 1 | row shows a `▼` caret when expanded | caret column on every commit row: `▼` open, `▶` closed | `the_commit_row_ends_with_an_expansion_caret`, `the_trailing_caret_follows_the_glyph_rung` |
| 2 | details keep the split-pane design (left commit info · right modified files + totals) | `left_column` / `right_column` drawn side by side at every width; `inline_height` is the taller column | `the_details_pane_paints_as_two_columns`, `the_details_pane_totals_close_the_right_column`, `the_inline_height_is_the_taller_column_not_their_sum` |
| 3 | rows after it shift down; scroll and selection stay put | per-row display heights; `leading_rows_push_the_graph_down`, `expanded_rows_push_the_next_commit_down_in_the_table` | `expanding_a_row_leaves_scroll_and_selection_alone`, `an_expansion_taller_than_the_viewport_scrolls_just_enough`, `collapsing_a_row_restores_the_geometry` |
| 4 | click the row again (or `Enter` / double-click) to collapse | `Enter` and a click on the row both toggle | `the_details_panel_belongs_to_the_expanded_row` |
| 5 | header with the total: `Uncommitted Changes (N)` | header with a collapse caret and a `u` jump key | `the_uncommitted_section_paints_above_the_history`, `the_uncommitted_section_belongs_to_the_overview_tab` |
| 6 | every file with status letter — A / M / D / R | one row per file, status letter from `--porcelain=v2`; renames keep both paths | `ordinary_modified_and_untracked_entries_parse`, `a_rename_keeps_both_paths_and_reads_the_next_nul_field`, `an_uncommitted_file_row_reads_tree_letter_path_stage_counts` |
| 7 | per-file `+add` / `−del`, plus staged / unstaged marking | per-file counts from the two `--numstat` runs, with `staged` / `unstaged` / `staged+unstaged` | `numstat_adds_staged_and_unstaged_counts`, `a_staged_and_unstaged_file_says_both`, `numstat_binary_dashes_count_as_zero` |
| 8 | summary line: files changed, staged vs unstaged counts, total +/− | `A n · M n · D n · R n · staged n · unstaged n · +n / −n` | `the_summary_line_counts_badges_staging_and_totals` |
| 9 | selecting a file shows its diff inline, in the list | `Enter` grows the diff under the file row; `Esc` returns with the row still selected | `the_file_diff_paints_under_the_details_panel`, `a_hunk_header_parses_both_ranges`, `the_gutter_shows_the_right_number_per_side` |
| 10 | already-backed: the `UNCOMMITTED` sentinel exists — this is the view on top of it | the sentinel and `CdvKind::Uncommitted` are unchanged and still drive the block; `src/uncommitted.rs` is additive | `an_empty_section_is_empty`, `paths_with_spaces_survive` |
| 11 | two tabs only (Overview / Branches); branch selection filters history | exactly two; `1` / `2` / `Tab` or a click; `Enter` / click filters, tags too | `the_tab_keys_select_and_cycle`, `the_drawn_tab_bar_matches_the_click_ranges`, `a_branch_click_selects_and_filters`, `a_tag_row_filters_the_history_too` |
| 12 | refs on their own line above the commit | `┌ refs │ HEAD -> main` one display row above the commit; the table's `refs` column is gone (UD-002) | `the_refs_line_sits_above_its_commit_row`, `leading_rows_push_the_graph_down` |
| 13 | in-terminal diff with green/red highlighting + Neovim fallback (`e`) | adds green, removes red, context dim; `e` runs `$GGT_EDITOR -d old new` and resumes the TUI | `the_header_and_hunk_lines_are_classified`, `the_argv_is_editor_dash_d_then_the_two_files`, `staging_the_editor_leaves_the_view_untouched` |
| 14 | full mouse support, context menus, find, capability ladder, dumb fallback | every new surface is in the router: tab bar, branch pane, file rows, uncommitted rows, diff scroll | `the_wheel_scrolls_the_surface_under_the_pointer`, `surface_at_routes_the_branch_pane_before_the_body`, `the_inline_surfaces_follow_the_glyph_rung` |
| 15 | all 62 dispatch actions and the 703-test suite | `DSP_TABLE` is untouched at 62 rows (`src/actions.rs` is not in this branch's diff); the suite grew 703 → 847 | `DSP_TABLE` coverage test, `cargo test` |
| 16 | tab bar: `Overview — graph, uncommitted, history` / `Branches — pick a branch to filter` | drawn on every frame, with the same sub-labels | `every_column_of_a_tab_label_maps_back_to_that_tab`, `a_tab_separator_is_not_a_tab`, `the_drawn_tab_bar_matches_the_click_ranges` |
| 17 | status bar: key hints left, `commits / uncommitted / branches / sel` right | unchanged, with the new keys added to the hint line | `summary_is_one_deterministic_line` |

Two rows need a word more than a tick:

- **Row 15.** The mockup's number for the suite (703) is W11's; W12 added 73 tests and the
  suite is now 847. The *capability* the bullet names is intact — `DSP_TABLE` still has all
  62 rows and `src/actions.rs` is not touched by this branch. What did change is two
  keybindings that had no target left: `+` / `-` resized the docked panel, and the panel is
  inline, so they are gone (UD-001). `Tab` moved from CDV focus to tab switching, with
  `Shift+Tab` taking over the old binding.
- **Row 6.** The mockup's legend lists `R` renamed; the parser keeps both paths and marks
  the row `R`. A rename is the one status letter whose *display* depends on `--find-renames`
  actually pairing the two sides, which `--porcelain=v2 -z --find-renames` does.

## Ledger — `UD-001…UD-008`

W12 upgrades the design rather than porting new reference surface, so its deliberate changes
are recorded as `UD-` rows in `.IDE_Plans/parity-ledger.md` §11, outside the 425-row parity
population. No `ACT-` / `INT-` / `VIS-` / `SET-` row changes status because of W12; where a
row's *reason* text had gone stale it is corrected in place and the row says so.

| ID | Change |
|---|---|
| UD-001 | `+` / `-` no longer resize the panel; `CdvState::height`, `MIN_HEIGHT`, `MAX_HEIGHT`, `DEFAULT_HEIGHT`, `resize`, `Action::CdvResizeUp` / `CdvResizeDown` are gone. Also fixed the one stale reader (`cdv_file_viewport` scrolled against a hardcoded 14) |
| UD-002 | The commit table has no `refs` column; refs draw only on their own line above the commit |
| UD-003 | The details pane keeps the reference's field order and labels and adds `Branch` / `Tags`; the right header stays `Files (N) — Tree View` |
| UD-004 | The right column does not wrap, so the dim diff-summary sentence clips below ~126 columns. Accepted: it restates the totals line directly above it |
| UD-005 | The branch filter is a tab, not the reference's multi-select dropdown. The *capability* is present; the dropdown rows keep their `BACKLOG` status because they describe the missing widget, not the missing capability |
| UD-006 | Uncommitted changes get a dedicated block instead of the reference's `UNCOMMITTED` pseudo-commit row |
| UD-007 | The diff is parsed and rendered in the terminal instead of being handed to the VS Code diff editor |
| UD-008 | `e` hands the terminal to `$GGT_EDITOR` in diff mode, inert when that variable is unset |

§8 totals are unchanged: **425 rows — 187 `DONE` (44%), 112 absent by ruling, 66 deliberate
design differences, 60 real gaps.** W12 adds no parity row and closes none.

## Gate

| Check | Result |
|---|---|
| `cargo fmt --check` | **PASS** — local (allowed) and in CI |
| `cargo clippy --all-targets -- -D warnings` | **PASS** — local (allowed) and in CI |
| `cargo test` | **PASS** — remote CI run `38311439228`, `fmt + clippy + test` job green. The count (**847**: 353 bin + 353 app_state + 15 golden + 64 integration + 62 perf; W11 was 703, so W12 adds 73) is from a run made before local suite execution was banned on this machine |
| `cargo test` under a hostile env | **PASS** — `GGT_EDITOR=nvim TERM=dumb` green, all five targets. Local, and run before the ban |
| Remote CI on this branch | **PASS** — the repository has a remote (`Adi-Mudi/git-graph-terminal`, over SSH), so the P0 escalation about a missing remote is **closed**. Green through `6f5b2be` (run `38311439228`); `39a453c` and this report are committed and awaiting their run. Four runs are recorded below, including the one that was red |
| pty E2E (remote) | **PASS** — run `38311439228` on `6f5b2be`: `pty E2E (vt100)` and `pty E2E (xterm-256color)` both green, so the two new steps and the ASCII guard run on a real runner |
| pty E2E (local rehearsal) | **PASS** — both matrix rows, driven exactly as the job drives them, *before* the remote job existed |
| Mockup conformance | **COMPLETE** — 17/17 rows above |

### CI history on this branch

| Run | Commit | Result |
|---|---|---|
| `38297846663` | `e5af02a` | **GREEN** — all three jobs (27s / 26s / 27s) |
| `38300733488` | `ffb4e01` | **FAILED** — job `fmt + clippy + test`: `app::tests::the_editor_key_stages_the_argv_without_launching` panicked at `src/app.rs:3991`, `unwrap()` on `Err(Os { code: 2, kind: NotFound })`; 336 passed / 1 failed. Both `pty E2E` jobs passed. See defect 2 below |
| `38308943519` | `e7337fb` | **GREEN** — all three jobs (40s / 28s / 26s) |
| `38311439228` | `6f5b2be` | **GREEN** — all three jobs (31s), including the two new inline-surface pty assertions |

Remote CI has run on every push to this branch. Exactly one run was red, it was red for one
real defect, and that run is why the editor fix in `39a453c` exists.

The local rehearsal preceded the remote job and is not a substitute for it. The remote suite
is the gate, and it is green through `6f5b2be`.

## Rule 12 — plan verification

The v3.0 plan states its tasks as prose, so each `T` was checked as "does the named
behaviour exist and is it pinned". Read against the plan file directly, not against this
report's summary of it.

| Plan item | Verdict | Evidence |
|---|---|---|
| T1 exactly two tabs, `1`/`2`/`Tab`/click, same interface | **PASS** | `Tab` enum, `tab_bar`, `set_tab`; tests above |
| T2 branch pane groups + counts + filter + right-click menu | **PASS** | `src/branches.rs`; ref menu unchanged |
| T3 inline expansion, caret, rows shift, scroll/selection preserved, split-pane kept, no separate view | **PASS** | `block_parts`, `left_column`/`right_column`; geometry tests |
| T4 uncommitted block: header + caret + `u`, summary line, per-file row, inline diff, documented argv | **PASS** | `src/uncommitted.rs` argv is `--porcelain=v2 -z --find-renames` + two `--numstat` + `git diff [--cached] -- <path>` |
| T5 diff in the same view: hunks, numbers, tints, scroll, `Esc` returns with the row selected | **PASS** | `src/diff.rs`, `the_file_diff_paints_under_the_details_panel` |
| T6 `e` → `$GGT_EDITOR` in diff mode, temp files, resume at same view/row/scroll, never a new window | **PASS** | `src/editor.rs`, `run_editor`; no window is ever opened |
| T7 refs own line, full mouse, ladder respected, dumb tier | **PASS** | refs line + caret column; `the_inline_surfaces_follow_the_glyph_rung` |
| T8 unit tests per listed area, golden updates with reasons, pty KEYS evidence, `UD-` rows, conformance table, gate | **PASS** | 73 new tests; no golden file changed (none needed — the goldens cover graph layout, which T7 left byte-identical); captures under `target/pty/`; §11 above |
| Rules: standard lane rules, `data.rs` additive-only, reference tree read-only, `.IDE_Plans` only for §11 + this report, commit incrementally | **PASS** | `src/data.rs` is not in this branch's diff; 13 commits plus the W12b editor fix |

Nothing in the plan is unimplemented. Two things the plan did not name and that are recorded
as deviations rather than silently absorbed:

1. `src/actions.rs` and `src/data.rs` were **not** touched at all — the plan allowed
   `data.rs` to be additive, and it turned out not to need to be. `DSP_TABLE` is intact.
2. The plan's gate says "703+ green"; the suite is at 847, so the gate is met with margin.

## Notes / deviations

- **`REQUEST MERGE` is not acted on.** The plan says do not self-merge; this branch is
  handed over unmerged.
- **`Enter` and `p` swapped.** `Enter` on a file row grew the diff in place and the old
  full-screen pager moved to `p`. Both remain reachable; the change is a re-bind, not a
  removal, and it is recorded in the keymap's own doc table.
- **`Tab` and `Shift+Tab`.** `Tab` cycles tabs; `Shift+Tab` keeps the old CDV focus toggle.
- **UD-004 is a known limitation, not a defect.** The dim diff-summary sentence clips below
  ~126 columns. It restates the totals line immediately above it, so nothing is lost.
- **The report is written into the worktree's `.IDE_Plans/`.** That directory is
  `.gitignore`d; the earlier wave reports are tracked anyway, so this one is force-added to
  match. The W12 plan permits `.IDE_Plans` writes for `parity-ledger.md` §11 and this file
  only, and that is all that was written.
- **The remote exists and CI is green.** `origin` is `Adi-Mudi/git-graph-terminal` over SSH,
  so the P0 escalation about a missing remote is **closed**. CI has run on every push to this
  branch; the history is under Gate. One run was red — `38300733488` on `ffb4e01` — and it is
  recorded as defect 2 below rather than omitted.
- **Local suite execution is banned on this machine.** A local `cargo test` pinned the CPU
  package at 100.6 °C. Local is now `cargo check` / `cargo clippy` / `cargo fmt --check` only,
  and every test result in this report comes from CI.

## Retrospective

**What went well**

- The four `ui`-level `TestBackend` tests paid for themselves immediately — they were the
  only thing that could see the zero-width rect, and all four were red before the fix.
- Rendering the demo frame at *both* ladder rungs as the evidence gallery caught the literal
  glyphs. A single-rung gallery would have shipped them.
- Splitting the ledger work into its own commits (`ffb4e01`, `e7337fb`) kept the design
  differences reviewable separately from the code that caused them.
- Fixing the stale `cdv_file_viewport` reader in the same commit that deleted its field
  meant the removal had no orphaned dependency to leave behind.

**What failed (with reason)**

- The inline surfaces were written against a width that could never be non-zero, and no test
  read the framebuffer, so they shipped blank. Building a surface is not the same as painting
  it; only a draw test tells them apart.
- Two editor tests shared one temp path because `editor::stage` keyed it on the process id
  rather than on the call. It passed locally and passed twice in CI before failing, so the
  suite was flaky rather than broken — the worst kind of red, because it teaches you to
  re-run instead of to read.
- Five new surfaces spelled their box drawing out as literals while the rest of the app
  asked the ladder. The convention existed; the new code did not follow it.
- The first pass at T3 stacked the two columns vertically, which is a different layout from
  the mockup's grid and was only caught by comparing against the mockup again.

**What to improve**

- Any new render surface should land with a `TestBackend` draw test in the same commit. A
  surface with no framebuffer assertion is unverified by construction.
- Any test that touches a process-global — env var, cwd, signal handler — must own it end to
  end: clear before, set per case, clear after. Assume the machine is hostile.
- A shared filesystem path is a process-global too. Key temp files on the *call*, never on the
  pid: a pid is shared by every test in the binary, so two tests that look independent
  silently share one file.
- New render code should reach for `GlyphSet` first and never type a box-drawing character;
  a lint or a grep in CI would make the convention mechanical instead of remembered.
- Diff the render against the mockup's CSS, not against a description of it. The grid versus
  stack mistake came from reading the prose.
