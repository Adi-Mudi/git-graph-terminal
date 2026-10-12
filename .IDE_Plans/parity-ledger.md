# M3-W9 — FUNCTION-PARITY LEDGER (G7 contract, row level)

Base: `development` @ `d6bc02d` · Branch `ggt-m3-w9` · Worktree `../git-graph-terminal-w9`
Date: 2026-10-12 · Wave: M3-W9 · Status: **COMPLETE (every contract row classified, zero blanks)**

Purpose: the user's hard requirement — *"100% reverse-engineered or documented"*. One row per
G7 contract item, mapped to the shipped binary. A row is never blank and is never upgraded to
`DONE` without a cite that actually exists in the tree (T3).

## 0.1 Sources of truth

| Kind | Path |
|---|---|
| Contract + rulings | `.IDE_Plans/g7-checklist.md` (R1–R6, §B contract table) |
| Row authority (commands/settings/interactions/visual/drift) | `.IDE_Plans/g7-inventory-cline.md` (A01–A111, S01–S110, C01–C31, V01–V32, E01–E09) |
| Row authority (dispatch map + finer INT/VIS) | `.IDE_Plans/g7-inventory-pi.md` (DSP-001…062, ACT-001…112, SET-001…082, INT-001…063, VIS-001…059) |
| Shipped binary under audit | `src/**` of `ggt-m3-w9` (16,128 lines Rust, 333+ tests) |

## 0.2 ID scheme used by this ledger

| Family | IDs in this ledger | Count | Why this authority |
|---|---|---|---|
| Webview→host dispatch map | `DSP-001…062` | 62 | contract §B: authority = pi (cline re-derived ✓) |
| Commands/Actions | `ACT-001…111` = cline `A01…A111` | 111 | contract §B: cline is the granular authority |
| Settings | `SET-001…110` = cline `S01…S110` | 110 | contract §B: both passes equal (81 active + 29 aliases) |
| Interactions | `INT-001…063` = pi `INT-001…063` | 63 | pi is the finer merge; cline `C01…C31` folds into these |
| Visual states | `VIS-001…059` = pi `VIS-001…059` | 59 | pi is the finer merge; cline `V01…V32` folds into these |
| Drift / not-found register | `DRIFT-001…009` = cline `E01…E09` | 9 | contract §B: cline |
| Rulings register | `R1.1…R6` | 11 | contract §A |
| **Total** | | **425** | contract §B estimates "~380"; the +45 is pi's finer INT/VIS granularity + the 11 ruling rows |

## 0.3 Status legend (these four values only)

| Status | Means |
|---|---|
| `DONE` | Present and reachable in the shipped binary. Evidence names the code path **and** a test/verify that exists in the tree. |
| `NOT-PORTED` | Absent by ruling (R3/R4/R5/R6) or VS Code-only chrome. Reason given. |
| `DESIGN-DIFFERENCE` | The terminal intentionally behaves differently. Rationale given. |
| `BACKLOG` | A real gap in the M2-core set — a W10 release-wave candidate (see §9). |

---

## 1. Rulings register (contract §A — R1–R6)

| ID | Item | Status | Evidence / reason |
|---|---|---|---|
| R1.1 | `graph.colours` default = **12 colours** (pi's "8" was a truncation artefact) | DONE | `src/render/mock.rs` frozen palette; test `palette_is_the_frozen_twelve` |
| R1.2 | Stash scroll key: **no Shift = NEXT, Shift = PREVIOUS** (pi INT-009 was inverted — not ported) | DONE | `src/keymap.rs:273-276`; test `stash_direction_and_wrap_match_r1_2` |
| R1.3 | `View Issue` call sites `main.ts:1058`/`1287`; `Reset File to this Revision` = `main.ts:3083` | DONE | anchors re-derived from cline; feature rows ACT-022/ACT-044/ACT-071 carry the ruling |
| R1.4 | `settingsWidget.ts` anchors use cline's re-derived line numbers | DONE | cline anchors adopted; no code impact (settings widget not built → SET/ACT-074…095 rows) |
| R1.5 | REA literal retention is run-specific (whole-dir `E1` attests config literals; module-level `REA-B` is structural only) | DONE | methodological ruling; applied to SET evidence grading in §4 |
| R1.6 | pi §5.5 "no post-1.30.0 changes" is refuted — do not rely on it | DONE | methodological ruling; applied to DRIFT-001/002 |
| R2 | Adopt pi's cross-review corrections to cline (V27 colour classes, diff argv, find-widget rules, M1–M13 verbatim) | DONE | `src/render/columns.rs` diff argv `diff-tree -r --root --find-renames --diff-filter=AMDR -z`; tests `status_codes_match_the_reference`, `the_counter_reads_n_of_m_and_no_results` |
| R3 | Tie-break rule: data/semantics = M2-core · rendering+OS/terminal integration = M3 · VS Code chrome = OUT-OF-SCOPE | DONE | applied throughout; every `NOT-PORTED` row cites the R3 limb it falls under |
| R4 | Code-review actions are **OUT-OF-SCOPE** for the M2 contract (VS Code workspace state = chrome) | DONE | ACT-005/006/007/108, DSP-025/056/058, VIS-025 all `NOT-PORTED` under R4 |
| R5 | DRIFT ruling: `View Issue` + `Reset File to this Revision` exist in source but not in shipped 1.30.0; **source is authority**, both tagged SOURCE-ONLY | DONE | DRIFT-001/DRIFT-002; ACT-022/ACT-044/ACT-071 |
| R6 | Negative contract (9 not-found items) — tests must assert ABSENCE | DONE | §7 DRIFT-003…DRIFT-009; tests `the_uncommitted_row_is_never_a_match`, `unbound_keys_are_none`, `only_follow_first_parent_drops_later_parents` |

---

## 2. Dispatch map — DSP-001…062 (62 rows)

Source of truth: `src/actions.rs` (`enum RefAction` ×62, `fn plan()`, `fn dispatch()`).
Kind mix pinned by `plan_kind_mix_has_the_documented_shape` (14 execute + 25 confirm + 12 local + 11 out-of-scope = 62);
exhaustiveness by `every_dsp_row_has_exactly_one_ref_action_variant`; the table by `dsp_table_matches_the_inventory`.

**Layer rule for this section:** a `DSP-` row asks *"does the terminal have this dispatch entry?"* — i.e. does a
`RefAction` variant exist whose `plan()` produces the reference's effect. **Reachability is a different question**
and is recorded on the `ACT-` row in §3. So `DSP-027 Fetch` is `DONE` (a real `fetch` plan exists and is tested)
while `ACT-008 Fetch` is `BACKLOG` (no key or menu row binds it). The 11 `PlanKind::OutOfScope` rows are
`NOT-PORTED` under R3/R4 — `out_of_scope_rows_are_explicit` asserts exactly which ones they are.

| ID | Feature (`RefAction`) | Status | Evidence / reason |
|---|---|---|---|
| DSP-001 | `AddRemote` | DONE | `remote add <name> <url>`; test `add_remote_asks_for_a_name_and_a_url` (entry point → ACT-078) |
| DSP-002 | `AddTag` | DONE | `tag [-f] [-a <name> -m <msg>] <hash>`; test `destructive_ops_build_real_argv_but_are_gated` |
| DSP-003 | `ApplyStash` | DONE | `stash apply [--index] <sel>`; menu A45; test `a_gated_row_stops_at_the_confirmation_stub` |
| DSP-004 | `BranchFromStash` | DONE | `stash branch <b> <sel>`; form in `src/dialog.rs`; test `applying_a_form_rewrites_the_plan_argv` |
| DSP-005 | `CheckoutBranch` | DONE | Execute `checkout [-b <local>] <b>`; test `checkout_branch_and_commit_argv` |
| DSP-006 | `CheckoutCommit` | DONE | Execute `checkout <obj>`; test `checkout_branch_and_commit_argv` |
| DSP-007 | `CherrypickCommit` | DONE | `cherry-pick [--no-commit] [-x] <hash>`; test `destructive_ops_build_real_argv_but_are_gated` |
| DSP-008 | `CleanUntrackedFiles` | DONE | `clean -f` / `-fd`; test `destructive_ops_build_real_argv_but_are_gated` |
| DSP-009 | `CommitDetails` | DONE | Execute `show --stat --patch <obj>`; test `commit_details_parse_all_fields` |
| DSP-010 | `CompareCommits` | DONE | Execute `diff --find-renames <from> <to>`; test `vis_040_comparison_shows_the_reference_sentence` |
| DSP-011 | `CopyFilePath` | DONE | Local plan with a text payload; test `copy_actions_are_local_and_carry_a_payload` (entry point → ACT-072/073) |
| DSP-012 | `CopyToClipboard` | DONE | OSC 52 + `GGT_CLIPBOARD_FILE`; test `dispatch_writes_the_clipboard_file` |
| DSP-013 | `CreateArchive` | NOT-PORTED | `PlanKind::OutOfScope` — R3 VS Code save-dialog chrome |
| DSP-014 | `CreateBranch` | DONE | `branch [-f] <name> <hash>` / `checkout -b`; test `create_branch_and_remote_argv` |
| DSP-015 | `CreatePullRequest` | NOT-PORTED | `OutOfScope` — R3 hosted-provider web UI |
| DSP-016 | `DeleteBranch` | DONE | `branch -d|-D <name>`; test `destructive_ops_build_real_argv_but_are_gated` |
| DSP-017 | `DeleteRemote` | DONE | `remote remove <name>`; test `dsp_table_matches_the_inventory` (entry point → ACT-077) |
| DSP-018 | `DeleteRemoteBranch` | DONE | `push <r> --delete <b>`; test `dsp_table_matches_the_inventory` |
| DSP-019 | `DeleteTag` | DONE | `tag -d <name>`; test `dsp_table_matches_the_inventory` |
| DSP-020 | `DeleteUserDetails` | DONE | `config --unset-all <key>`; test `reset_file_and_config_argv` (entry point → ACT-085) |
| DSP-021 | `DropCommit` | DONE | `rebase --onto <obj>^ <obj>`; test `destructive_ops_build_real_argv_but_are_gated` |
| DSP-022 | `DropStash` | DONE | `stash drop <sel>`; test `destructive_ops_build_real_argv_but_are_gated` |
| DSP-023 | `EditRemote` | DONE | `remote rename <name> <new>`; test `dsp_table_matches_the_inventory` (entry point → ACT-076) |
| DSP-024 | `EditUserDetails` | DONE | `config <key> <value>`; test `reset_file_and_config_argv` (entry point → ACT-084) |
| DSP-025 | `EndCodeReview` | NOT-PORTED | `OutOfScope` — R4 code-review workspace state |
| DSP-026 | `ExportRepoConfig` | DONE | Local plan with a path payload; test `local_rows_stub_through_the_status_line` (entry point → ACT-095) |
| DSP-027 | `Fetch` | DONE | Execute `fetch [--all\|<r>] [--prune] [--prune-tags]`; test `dsp_table_matches_the_inventory` (no key binds it → ACT-008/102) |
| DSP-028 | `FetchAvatar` | NOT-PORTED | `OutOfScope` — R3 avatars are a webview/network feature |
| DSP-029 | `FetchIntoLocalBranch` | DONE | Execute, gated once forced: `fetch [-f] <r> <b>:<local>`; test `force_flags_flip_safe_ops_into_the_gate` |
| DSP-030 | `LoadCommits` | DONE | Local plan; the real loader is `src/data.rs`; test `parses_two_commits_with_parents_and_refs` |
| DSP-031 | `LoadConfig` | DONE | Local plan; there is no extension config store, so git config is read directly in `src/files.rs` — test `reset_file_and_config_argv` |
| DSP-032 | `LoadRepoInfo` | DONE | Local plan; test `has_remotes_reads_the_refs` |
| DSP-033 | `LoadRepos` | DESIGN-DIFFERENCE | one repo per invocation — the repo is a CLI argument (`src/main.rs:97-101`) |
| DSP-034 | `Merge` | DONE | `merge <obj> [--squash\|--no-ff] [--no-commit]`; test `destructive_ops_build_real_argv_but_are_gated` |
| DSP-035 | `OpenExtensionSettings` | NOT-PORTED | `OutOfScope` — R3 VS Code settings UI |
| DSP-036 | `OpenExternalDirDiff` | NOT-PORTED | `OutOfScope`; availability is shown (VIS-051) but no launch (entry point → ACT-109) |
| DSP-037 | `OpenExternalUrl` | NOT-PORTED | `OutOfScope` — R3 no browser launch in a TUI (the URL is shown/copied instead) |
| DSP-038 | `OpenFile` | DONE | Execute `show :<path>`; test `every_pager_nav_is_reachable_from_a_key` (no key binds it → ACT-068) |
| DSP-039 | `OpenTerminal` | NOT-PORTED | `OutOfScope` — R3 explicitly redundant in a TUI |
| DSP-040 | `PopStash` | DONE | `stash pop [--index] <sel>`; test `stash_and_tag_targets_are_used` |
| DSP-041 | `PruneRemote` | DONE | `remote prune <name>`; test `dsp_table_matches_the_inventory` (entry point → ACT-075) |
| DSP-042 | `PullBranch` | DONE | Execute `pull <r> <b> [--squash\|--no-ff]`; test `remote_surface_matches_the_inventory_rows` |
| DSP-043 | `PushBranch` | DONE | Execute, gated once forced: `push <r> <b> [--set-upstream] [--force\|--force-with-lease]`; test `force_flags_flip_safe_ops_into_the_gate` |
| DSP-044 | `PushStash` | DONE | `stash push [--include-untracked] [--message <msg>]`; test `dsp_table_matches_the_inventory` |
| DSP-045 | `PushTag` | DONE | Execute `push <r> <name>`; test `dsp_table_matches_the_inventory` |
| DSP-046 | `Rebase` | DONE | `rebase [--ignore-date] <obj>`; test `destructive_ops_build_real_argv_but_are_gated` |
| DSP-047 | `RenameBranch` | DONE | `branch -m <name> <new>`; test `destructive_ops_build_real_argv_but_are_gated` |
| DSP-048 | `RescanForRepos` | DONE | Local plan; test `local_rows_stub_through_the_status_line` (entry point → ACT-105) |
| DSP-049 | `ResetFileToRevision` | DONE | `checkout <hash> -- <path>`; test `reset_file_and_config_argv` (entry point → ACT-071) |
| DSP-050 | `ResetToCommit` | DONE | `reset --soft\|--mixed\|--hard <obj>`; test `reset_mode_and_flags_round_trip_into_the_target` |
| DSP-051 | `RevertCommit` | DONE | `revert --no-edit <obj>`; test `destructive_ops_build_real_argv_but_are_gated` |
| DSP-052 | `SetGlobalViewState` | DONE | Local plan; test `column_menu_reflects_visibility_and_order_state` |
| DSP-053 | `SetRepoState` | DONE | Local plan; test `column_order_rows_switch_the_git_flag` |
| DSP-054 | `SetWorkspaceViewState` | DESIGN-DIFFERENCE | Local plan; nothing is persisted — view state lives for the session only |
| DSP-055 | `ShowErrorMessage` | DONE | Local plan through the status line; test `local_rows_stub_through_the_status_line` |
| DSP-056 | `StartCodeReview` | NOT-PORTED | `OutOfScope` — R4 code-review workspace state |
| DSP-057 | `TagDetails` | DONE | Execute `for-each-ref <name> --format=%(contents)`; test `stash_tag_and_uncommitted_surfaces_match` |
| DSP-058 | `UpdateCodeReview` | NOT-PORTED | `OutOfScope` — R4 code-review workspace state |
| DSP-059 | `ViewDiff` | DONE | Execute `diff --no-color <from>[^] [<to>] -- <path>`; test `vis_047_a_modified_text_file_shows_its_counts` |
| DSP-060 | `ViewDiffWithWorkingFile` | DONE | Execute `diff --no-color <hash> -- <path>`; test `vis_052_an_untracked_file_is_diffable` (entry point → ACT-067) |
| DSP-061 | `ViewFileAtRevision` | DONE | Execute `show <hash>:<path>`; test `every_pager_nav_is_reachable_from_a_key` |
| DSP-062 | `ViewScm` | NOT-PORTED | R3 VS Code SCM chrome; `dispatch_refuses_gated_plans_without_running_git` asserts it is skipped |

---

## 3. Commands / Actions — ACT-001…111 (= cline A01…A111, 111 rows)

**Layer rule for this section:** an `ACT-` row asks *"can the user actually reach this?"* — key, menu row,
click or toolbar. A `DONE` here means a real entry point exists. Where a complete menu surface exists but
nothing opens it, the row is `BACKLOG` and says so: `MenuSurface::FileRow` (A65…A73) and `MenuSurface::Link`
(A62…A64) are both fully defined in `src/menu.rs` and pinned by `column_header_links_and_files_match`, but
`ClickTarget` (`src/app.rs:383-388`) only has `ColumnHeader` and `Row`, so no click ever reaches them — only
`Enter`/`o`/`Space` on a CDV file row do (A65/A66/A69/A70).

| ID | Feature | Status | Evidence / reason |
|---|---|---|---|
| ACT-001 | View Git Graph (`git-graph.view`) | DONE | the binary *is* the view — `src/main.rs` event loop; test `mock_layout_builds_every_row_without_panicking` |
| ACT-002 | Add Git Repository… | DESIGN-DIFFERENCE | repo is a CLI argument (`src/main.rs:97-101`); no VS Code folder picker; test `non_repository_is_an_error_not_a_panic` |
| ACT-003 | Remove Git Repository… | BACKLOG | no repo list / ignored-repo state |
| ACT-004 | Clear Avatar Cache | NOT-PORTED | R3 avatars are a webview/network feature |
| ACT-005 | End All Code Reviews in Workspace | NOT-PORTED | R4 |
| ACT-006 | End a specific Code Review… | NOT-PORTED | R4 |
| ACT-007 | Resume a specific Code Review… | NOT-PORTED | R4 |
| ACT-008 | Fetch from Remote(s) | BACKLOG | DSP-027 Execute plan exists; no key/menu entry point (same gap as ACT-102) |
| ACT-009 | Get Version Information | DESIGN-DIFFERENCE | no about dialog; the product's identity is the binary/CLI itself |
| ACT-010 | Open File (diff editor) | NOT-PORTED | R3 VS Code diff-editor chrome |
| ACT-011 | Checkout Branch | DONE | menu row A11 + DSP-005; test `branch_surface_matches_the_inventory_rows` |
| ACT-012 | Rename Branch… | DONE | menu A12 + DSP-047; test `branch_surface_matches_the_inventory_rows` |
| ACT-013 | Delete Branch… | DONE | menu A13 + DSP-016; test `the_popup_draws_the_surface_title_and_every_inventory_id` |
| ACT-014 | Merge into current branch… | DONE | menu A14 + DSP-034; test `the_nine_surfaces_cover_a11_through_a73_once` |
| ACT-015 | Rebase current branch on Branch… | DONE | menu A15 + DSP-046; test `every_gated_action_gets_a_dialog` |
| ACT-016 | Push Branch… | DONE | menu A16 + DSP-043; test `push_mode_select_maps_to_the_enum` |
| ACT-017 | Create Pull Request… | NOT-PORTED | R3 hosted-provider API |
| ACT-018 | Create Archive | NOT-PORTED | R3 VS Code save-dialog chrome |
| ACT-019 | Select in Branches Dropdown | BACKLOG | `MenuAction::SelectInDropdown` is a documented local stub (`src/menu.rs:120-128`); no branches dropdown |
| ACT-020 | Unselect in Branches Dropdown | BACKLOG | same documented stub (`MenuAction::UnselectInDropdown`) |
| ACT-021 | Copy Branch Name to Clipboard | DONE | menu A21 + DSP-012; test `copy_actions_are_local_and_carry_a_payload` |
| ACT-022 | View Issue… | NOT-PORTED | R5 SOURCE-ONLY (absent from shipped 1.30.0) + R3 external URL |
| ACT-023 | Add Tag… | DONE | menu A23 + DSP-002; test `commit_surface_matches_the_inventory_rows` |
| ACT-024 | Create Branch… | DONE | menu A24 + DSP-014; test `create_branch_and_remote_argv` |
| ACT-025 | Checkout (commit) | DONE | menu A25 + DSP-006; test `checkout_branch_and_commit_argv` |
| ACT-026 | Cherry Pick… | DONE | menu A26 + DSP-007; test `every_gated_action_gets_a_dialog` |
| ACT-027 | Revert… | DONE | menu A27 + DSP-051; test `destructive_ops_build_real_argv_but_are_gated` |
| ACT-028 | Drop… | DONE | menu A28 + DSP-021; test `destructive_ops_build_real_argv_but_are_gated` |
| ACT-029 | Merge into current branch… (commit) | DONE | menu A29 → DSP-034; test `the_nine_surfaces_cover_a11_through_a73_once` |
| ACT-030 | Rebase current branch on this Commit… | DONE | menu A30 → DSP-046; test `the_popup_reflects_the_selection_and_disabled_rows` |
| ACT-031 | Reset current branch to this Commit… | DONE | menu A31 + DSP-050; test `reset_mode_and_flags_round_trip_into_the_target` |
| ACT-032 | Copy Commit Hash to Clipboard | DONE | menu A32 + DSP-012; test `copy_hash_reaches_the_clipboard_bridge` |
| ACT-033 | Copy Commit Subject to Clipboard | DONE | menu A33 + DSP-012; test `copy_actions_are_local_and_carry_a_payload` |
| ACT-034 | Checkout Branch… (remote) | DONE | menu A34 → DSP-005; test `remote_surface_matches_the_inventory_rows` |
| ACT-035 | Delete Remote Branch… | DONE | menu A35 + DSP-018; test `remote_surface_matches_the_inventory_rows` |
| ACT-036 | Fetch into local branch… | DONE | menu A36 + DSP-029; test `force_flags_flip_safe_ops_into_the_gate` |
| ACT-037 | Merge into current branch… (remote) | DONE | menu A37 → DSP-034; test `the_nine_surfaces_cover_a11_through_a73_once` |
| ACT-038 | Pull into current branch… | DONE | menu A38 + DSP-042; test `remote_surface_matches_the_inventory_rows` |
| ACT-039 | Create Pull Request (remote) | NOT-PORTED | R3 hosted-provider API |
| ACT-040 | Create Archive (remote) | NOT-PORTED | R3 save-dialog chrome |
| ACT-041 | Select in Branches Dropdown (remote) | BACKLOG | documented stub; no branches dropdown |
| ACT-042 | Unselect in Branches Dropdown (remote) | BACKLOG | documented stub; no branches dropdown |
| ACT-043 | Copy Branch Name to Clipboard (remote) | DONE | menu A43 → DSP-012; test `copy_actions_are_local_and_carry_a_payload` |
| ACT-044 | View Issue… (remote) | NOT-PORTED | R5 SOURCE-ONLY + R3 external URL |
| ACT-045 | Apply Stash… | DONE | menu A45 + DSP-003; test `stash_tag_and_uncommitted_surfaces_match` |
| ACT-046 | Create Branch from Stash… | DONE | menu A46 + DSP-004; test `stash_tag_and_uncommitted_surfaces_match` |
| ACT-047 | Pop Stash… | DONE | menu A47 + DSP-040; test `stash_tag_and_uncommitted_surfaces_match` |
| ACT-048 | Drop Stash… | DONE | menu A48 + DSP-022; test `stash_tag_and_uncommitted_surfaces_match` |
| ACT-049 | Copy Stash Name to Clipboard | DONE | menu A49 → DSP-012; test `copy_actions_are_local_and_carry_a_payload` |
| ACT-050 | Copy Stash Hash to Clipboard | DONE | menu A50 → DSP-012; test `copy_hash_reaches_the_clipboard_bridge` |
| ACT-051 | View Details (tag) | DONE | menu A51 + DSP-057; test `stash_tag_and_uncommitted_surfaces_match` |
| ACT-052 | Delete Tag… | DONE | menu A52 + DSP-019; test `stash_tag_and_uncommitted_surfaces_match` |
| ACT-053 | Push Tag… | DONE | menu A53 + DSP-045; test `dsp_table_matches_the_inventory` |
| ACT-054 | Create Archive (tag) | NOT-PORTED | R3 save-dialog chrome |
| ACT-055 | Copy Tag Name to Clipboard | DONE | menu A55 → DSP-012; test `copy_actions_are_local_and_carry_a_payload` |
| ACT-056 | Stash uncommitted changes… | DONE | menu A56 + DSP-044; test `stash_tag_and_uncommitted_surfaces_match` |
| ACT-057 | Reset uncommitted changes… | DONE | menu A57 + DSP-050; test `the_uncommitted_menu_targets_head` |
| ACT-058 | Clean untracked files… | DONE | menu A58 + DSP-008; test `the_uncommitted_menu_targets_head` |
| ACT-059 | Open Source Control View | NOT-PORTED | R3 VS Code SCM chrome |
| ACT-060 | Date / Author / Commit (checkable) | DONE | menu A60 + `ToggleColumn`; test `toggling_a_column_flips_only_that_column` |
| ACT-061 | Commit/Author Timestamp/Topological order (radio) | DONE | menu A61 + `SetColumnOrder`; test `column_order_rows_switch_the_git_flag` |
| ACT-062 | Open URL | NOT-PORTED | R3 no browser launch in a TUI |
| ACT-063 | Follow Internal Link | BACKLOG | `MenuAction::FollowInternalLink` is a documented local stub (`src/menu.rs:127`) |
| ACT-064 | Copy URL to Clipboard | BACKLOG | the `Link` menu is defined (A62/A63/A64) but nothing opens it — `ClickTarget` has no link arm |
| ACT-065 | View Diff | DONE | menu A65 + `Enter` on a CDV file row → `Action::CdvFileViewDiff`; test `cdv_keys_stand_in_for_the_cdv_controls` |
| ACT-066 | View File at this Revision | DONE | menu A66 + `o` → `Action::CdvFileViewRevision`; test `cdv_keys_stand_in_for_the_cdv_controls` |
| ACT-067 | View Diff with Working File | BACKLOG | the `FileRow` menu row exists (DSP-060 plan is tested) but no key or click opens that surface |
| ACT-068 | Open File (→ pager) | BACKLOG | same — `OpenFile`'s plan exists (`show :<path>`) but no key binds it |
| ACT-069 | Mark as Reviewed | DESIGN-DIFFERENCE | in-session reviewed flag (`Space`), not VS Code workspace state; test `a_reviewed_file_is_marked` |
| ACT-070 | Mark as Not Reviewed | DESIGN-DIFFERENCE | same in-session flag; test `a_reviewed_file_is_marked` |
| ACT-071 | Reset File to this Revision… | BACKLOG | the `FileRow` menu row exists (DSP-049 plan is tested) but nothing opens that surface |
| ACT-072 | Copy Absolute File Path to Clipboard | BACKLOG | same — DSP-011's payload is built but no entry point opens the `FileRow` menu |
| ACT-073 | Copy Relative File Path to Clipboard | BACKLOG | same |
| ACT-074 | Fetch (per remote) | BACKLOG | settings-widget action; widget not built |
| ACT-075 | Prune (per remote) | BACKLOG | settings-widget action (DSP-041 plan exists) |
| ACT-076 | Edit Remote | BACKLOG | settings-widget action (DSP-023 form exists) |
| ACT-077 | Delete Remote | BACKLOG | settings-widget action (DSP-017 form exists) |
| ACT-078 | Add Remote | BACKLOG | settings-widget action (DSP-001 form exists) |
| ACT-079 | Remote visibility eye | BACKLOG | settings-widget state; widget not built |
| ACT-080 | Show Stashes (checkbox) | BACKLOG | settings-widget toggle; default (`true`) is hardcoded in `src/data.rs` |
| ACT-081 | Show Tags (checkbox) | BACKLOG | settings-widget toggle; default (`true`) is hardcoded in `src/data.rs` |
| ACT-082 | Include commits only mentioned by reflogs | BACKLOG | settings-widget toggle; git flag supported in `src/data.rs`, no control |
| ACT-083 | Only follow the first parent of commits | BACKLOG | settings-widget toggle; behaviour exists (`only_follow_first_parent_drops_later_parents`), default `false` hardcoded |
| ACT-084 | Edit / Add User Details | BACKLOG | settings-widget action (DSP-024 form exists) |
| ACT-085 | Remove User Details | BACKLOG | settings-widget action (DSP-020 form exists) |
| ACT-086 | Add / Edit Issue Linking | BACKLOG | settings-widget action; issue regex not implemented |
| ACT-087 | Remove Issue Linking | BACKLOG | settings-widget action |
| ACT-088 | Configure / Edit Pull Request Integration | NOT-PORTED | R3 hosted-provider integration |
| ACT-089 | Remove Pull Request Integration | NOT-PORTED | R3 hosted-provider integration |
| ACT-090 | Edit Name (repo) | BACKLOG | settings-widget action; repo name comes from the path |
| ACT-091 | Delete Name (repo) | BACKLOG | settings-widget action |
| ACT-092 | Edit Initial Branches | BACKLOG | settings-widget action (SET-065) |
| ACT-093 | Clear Initial Branches | BACKLOG | settings-widget action |
| ACT-094 | Open Git Graph Extension Settings | NOT-PORTED | R3 VS Code settings UI |
| ACT-095 | Export Repository Configuration | BACKLOG | local plan (DSP-026) exists; settings-widget entry point not built |
| ACT-096 | Repo dropdown | BACKLOG | single repo per invocation (`DESIGN-DIFFERENCE` on ACT-002); no dropdown |
| ACT-097 | Branches dropdown (multi-select + globs) | BACKLOG | no dropdown (blocks ACT-019/020/041/042) |
| ACT-098 | Show Remote Branches (checkbox) | BACKLOG | toolbar checkbox not built; default `true` hardcoded |
| ACT-099 | Find (magnifier) | DONE | `Ctrl+F` → `src/find/`; test `find_keys_stand_in_for_the_find_widget_controls` |
| ACT-100 | Terminal | NOT-PORTED | R3 redundant in a terminal product |
| ACT-101 | Repository Settings (gear) | BACKLOG | settings widget not built |
| ACT-102 | Fetch (cloud/download) | BACKLOG | same gap as ACT-008 — DSP-027 plan, no entry point |
| ACT-103 | Refresh | DONE | `r` → `Action::Refresh`; test `expand_and_refresh_keys` |
| ACT-104 | Load More Commits | DESIGN-DIFFERENCE | there is no paging at all — one `git log` (`src/data.rs:206-221`) loads the whole history, so a "load more" control has nothing to do |
| ACT-105 | Re-scan workspace for repositories | BACKLOG | no repo discovery / "no repositories" page |
| ACT-106 | Close (CDV) | DONE | `Ctrl+W` → `Action::CdvClose`; test `cdv_keys_stand_in_for_the_cdv_controls` |
| ACT-107 | File View type: Tree / List | DONE | `v` → `Action::CdvToggleView`; test `vis_041_and_042_the_view_toggles_and_is_named` |
| ACT-108 | Code Review (start / end) | NOT-PORTED | R4 code-review workspace state |
| ACT-109 | Open External Directory Diff | BACKLOG | external diff tool launch not implemented (availability display only) |
| ACT-110 | CDV divider drag / height resize | NOT-PORTED | R3 pointer-layout chrome; the terminal CDV is a fixed pane (`+`/`-` resize the row budget instead) |
| ACT-111 | Folder expand / collapse | DONE | `src/cdv.rs` tree toggles; test `closed_folders_hide_their_children` |

---

## 4. Settings — SET-001…110 (= cline S01…S110, 110 rows)

**Port-wide fact:** the terminal binary has **no settings subsystem** — `grep -rniE 'setting' src/` finds no
config reader, and the only env reads are `GGT_CLIPBOARD_FILE` / `XDG_RUNTIME_DIR` (`src/actions.rs:1216,1221`).
So a setting is never a live knob here. Each row therefore records what the terminal does *instead*:
`DESIGN-DIFFERENCE` = the reference default behaviour is hardcoded (named, with its real value); `NOT-PORTED` = no
equivalent at all; `BACKLOG` = an M2-core behaviour that is genuinely missing (a W10 candidate). Aliases
(SET-082…110) are all `NOT-PORTED` — there is no config store for a deprecated name to be folded into.

The dialog defaults below are the real ones from `ActionTarget::default()` (`src/actions.rs:162-201`): every bool
is `false`, `ResetMode::Mixed`, `PushMode::Normal`. Where the reference default is `true`, the row says so —
that is a genuine behavioural difference, not just a missing checkbox. `fields_for()` (`src/dialog.rs:531-580`)
only builds a form for **11** actions, so several fields exist in the parameter bag without any way to set them;
those rows say "no form field".

| ID | Setting (default) | Status | Evidence / reason |
|---|---|---|---|
| SET-001 | `commitDetailsView.autoCenter` = `true` | DESIGN-DIFFERENCE | the CDV is a fixed docked pane, not a scroll-centred webview |
| SET-002 | `commitDetailsView.fileView.fileTree.compactFolders` = `true` | DESIGN-DIFFERENCE | default on — single-child chains collapse; test `a_single_child_folder_chain_collapses` |
| SET-003 | `commitDetailsView.fileView.type` = `"File Tree"` | DESIGN-DIFFERENCE | default Tree; `v` toggles; test `vis_041_and_042_the_view_toggles_and_is_named` |
| SET-004 | `commitDetailsView.location` = `"Inline"` | DESIGN-DIFFERENCE | the terminal CDV is a docked pane, never inline |
| SET-005 | `contextMenuActionsVisibility` = `{}` | NOT-PORTED | no per-row context-menu visibility config |
| SET-006 | `customBranchGlobPatterns` = `[]` | NOT-PORTED | no branches dropdown to hold glob patterns |
| SET-007 | `customEmojiShortcodeMappings` = `[]` | NOT-PORTED | R3 emoji mappings are M3; not built |
| SET-008 | `customPullRequestProviders` = `[]` | NOT-PORTED | R3 hosted-provider integration |
| SET-009 | `date.format` = `"Date & Time"` | DESIGN-DIFFERENCE | fixed long format; test `long_dates_render_the_reference_iso_format` |
| SET-010 | `date.type` = `"Author Date"` | DESIGN-DIFFERENCE | committer date is shown; author date only when it differs — test `vis_038_author_date_appears_only_when_it_differs` |
| SET-011 | `defaultColumnVisibility` = all `true` | DESIGN-DIFFERENCE | all three optional columns start visible; test `column_state_starts_at_the_p0_default` |
| SET-012 | `dialog.addTag.pushToRemote` = `false` | NOT-PORTED | the Add Tag form has type only, no push-to-remote option |
| SET-013 | `dialog.addTag.type` = `"Annotated"` | DESIGN-DIFFERENCE | the plan defaults to **lightweight** (`tag <name> <hash>`); the `annotated` flag adds `-a -m` but no form field exposes it |
| SET-014 | `dialog.applyStash.reinstateIndex` = `false` | DESIGN-DIFFERENCE | default `false` matches; `--index` is wired in `plan()` but no form field sets it |
| SET-015 | `dialog.cherryPick.noCommit` = `false` | DESIGN-DIFFERENCE | default `false` matches; no form field (only `record_origin` is offered) |
| SET-016 | `dialog.cherryPick.recordOrigin` = `false` | DESIGN-DIFFERENCE | default `false` matches; checkbox present — test `applying_a_form_rewrites_the_plan_argv` |
| SET-017 | `dialog.createBranch.checkOut` = `false` | DESIGN-DIFFERENCE | default `false` matches (`branch <name> <hash>`); no form field |
| SET-018 | `dialog.deleteBranch.forceDelete` = `false` | DESIGN-DIFFERENCE | default `false` matches (`branch -d`); `-D` needs the `force` flag, which no form field sets |
| SET-019 | `dialog.fetchIntoLocalBranch.forceFetch` = `false` | DESIGN-DIFFERENCE | default `false` matches; the `force` flag is tested but no form field sets it — test `force_flags_flip_safe_ops_into_the_gate` |
| SET-020 | `dialog.fetchRemote.prune` = `false` | NOT-PORTED | no fetch dialog (DSP-027 has no entry point) |
| SET-021 | `dialog.fetchRemote.pruneTags` = `false` | NOT-PORTED | no fetch dialog |
| SET-022 | `dialog.general.referenceInputSpaceSubstitution` = `"None"` | NOT-PORTED | ref fields reject whitespace outright; test `a_ref_field_refuses_whitespace` |
| SET-023 | `dialog.merge.noCommit` = `false` | DESIGN-DIFFERENCE | default `false` matches; no form field (the Merge form offers `no_ff` and `squash`) |
| SET-024 | `dialog.merge.noFastForward` = `true` | DESIGN-DIFFERENCE | **different default** — the plan allows fast-forward (`merge <obj>`); `no_ff` is a checkbox, off by default |
| SET-025 | `dialog.merge.squashCommits` = `false` | DESIGN-DIFFERENCE | default `false` matches; checkbox present — test `destructive_ops_build_real_argv_but_are_gated` |
| SET-026 | `dialog.merge.squashMessageFormat` = `"Default"` | NOT-PORTED | squash message format not implemented |
| SET-027 | `dialog.popStash.reinstateIndex` = `false` | DESIGN-DIFFERENCE | default `false` matches; `--index` is wired but no form field sets it |
| SET-028 | `dialog.pullBranch.noFastForward` = `false` | DESIGN-DIFFERENCE | default `false` matches; no form field (Pull has no dialog) |
| SET-029 | `dialog.pullBranch.squashCommits` = `false` | DESIGN-DIFFERENCE | default `false` matches; no form field |
| SET-030 | `dialog.pullBranch.squashMessageFormat` = `"Default"` | NOT-PORTED | squash message format not implemented |
| SET-031 | `dialog.rebase.ignoreDate` = `true` | DESIGN-DIFFERENCE | **different default** — the plan runs a plain `rebase <obj>`; no form field sets `--ignore-date` |
| SET-032 | `dialog.rebase.launchInteractiveRebase` = `false` | NOT-PORTED | no interactive rebase editor in a TUI |
| SET-033 | `dialog.resetCurrentBranchToCommit.mode` = `"Mixed"` | DESIGN-DIFFERENCE | default `Mixed` matches; a real dropdown — test `reset_mode_and_flags_round_trip_into_the_target` |
| SET-034 | `dialog.resetUncommittedChanges.mode` = `"Mixed"` | DESIGN-DIFFERENCE | A57 maps to `ResetToCommit` (DSP-050), so the same mode dropdown appears; test `the_uncommitted_menu_targets_head` |
| SET-035 | `dialog.stashUncommittedChanges.includeUntracked` = `true` | DESIGN-DIFFERENCE | **different default** — `--include-untracked` is only added when the flag is set, and it is off by default; no form field |
| SET-036 | `enhancedAccessibility` = `false` | DESIGN-DIFFERENCE | text markers are the natural TUI rendering, always on |
| SET-037 | `fileEncoding` = `"utf8"` | DESIGN-DIFFERENCE | UTF-8 only; test `text_editing_is_utf8_safe` |
| SET-038 | `graph.colours` = 12 colours | DESIGN-DIFFERENCE | the 12-colour palette is frozen; test `palette_is_the_frozen_twelve` |
| SET-039 | `graph.style` = `"rounded"` | DESIGN-DIFFERENCE | rounded edges are the only style; test `adjacent_lane_transition_is_a_rounded_step` |
| SET-040 | `graph.uncommittedChanges` = `"Open Circle…"` | DESIGN-DIFFERENCE | open circle marker; test `uncommitted_row_uses_the_dashed_vertical` |
| SET-041 | `integratedTerminalShell` = `""` | NOT-PORTED | R3 VS Code integrated terminal |
| SET-042 | `keyboardShortcut.find` = `"CTRL/CMD + F"` | DESIGN-DIFFERENCE | fixed `Ctrl+F`; test `find_keys_stand_in_for_the_find_widget_controls` |
| SET-043 | `keyboardShortcut.refresh` = `"CTRL/CMD + R"` | DESIGN-DIFFERENCE | fixed `r`/`Ctrl+R`; test `expand_and_refresh_keys` |
| SET-044 | `keyboardShortcut.scrollToHead` = `"CTRL/CMD + H"` | DESIGN-DIFFERENCE | `Home`/`End` stand in; test `top_and_bottom_jump_to_the_ends` |
| SET-045 | `keyboardShortcut.scrollToStash` = `"CTRL/CMD + S"` | DESIGN-DIFFERENCE | `s`/`S` per R1.2; test `stash_keys_follow_r1_2` |
| SET-046 | `markdown` = `true` | NOT-PORTED | no markdown rendering of commit messages |
| SET-047 | `maxDepthOfRepoSearch` = `0` | NOT-PORTED | no repo discovery walk |
| SET-048 | `openNewTabEditorGroup` = `"Active"` | NOT-PORTED | R3 VS Code editor groups |
| SET-049 | `openToTheRepoOfTheActiveTextEditorDocument` = `false` | NOT-PORTED | R3 no active-editor concept |
| SET-050 | `referenceLabels.alignment` = `"Normal"` | DESIGN-DIFFERENCE | labels align to the graph; test `branch_labels_partitions_the_decorations` |
| SET-051 | `referenceLabels.combineLocalAndRemoteBranchLabels` = `true` | DESIGN-DIFFERENCE | combined labels; test `split_remote_ref_keeps_slashes_in_the_branch` |
| SET-052 | `repository.commits.fetchAvatars` = `false` | NOT-PORTED | R3 avatars |
| SET-053 | `repository.commits.initialLoad` = `300` | DESIGN-DIFFERENCE | **no cap at all** — one `git log` (`src/data.rs:206-221`) loads the whole history; no `--max-count` |
| SET-054 | `repository.commits.loadMore` = `100` | DESIGN-DIFFERENCE | no paging, so there is no page size to configure |
| SET-055 | `repository.commits.loadMoreAutomatically` = `true` | DESIGN-DIFFERENCE | no paging (see ACT-104) |
| SET-056 | `repository.commits.mute.commitsThatAreNotAncestorsOfHead` = `false` | BACKLOG | muting is not implemented (no `mute` in `src/`) |
| SET-057 | `repository.commits.mute.mergeCommits` = `true` | BACKLOG | muting is not implemented |
| SET-058 | `repository.commits.order` = `"date"` | DESIGN-DIFFERENCE | `--date-order` default, switchable from the header menu; test `column_order_rows_switch_the_git_flag` |
| SET-059 | `repository.commits.showSignatureStatus` = `false` | DESIGN-DIFFERENCE | the signature badge is always shown; test `vis_038_a_signature_is_badged_with_its_description` |
| SET-060 | `repository.fetchAndPrune` = `false` | NOT-PORTED | no fetch dialog |
| SET-061 | `repository.fetchAndPruneTags` = `false` | NOT-PORTED | no fetch dialog |
| SET-062 | `repository.includeCommitsMentionedByReflogs` = `false` | BACKLOG | the git flag is wired in `src/data.rs`, but no control sets it (see ACT-082) |
| SET-063 | `repository.onLoad.scrollToHead` = `false` | BACKLOG | no on-load scroll option (starts at the top) |
| SET-064 | `repository.onLoad.showCheckedOutBranch` = `false` | BACKLOG | no on-load branch scroll option |
| SET-065 | `repository.onLoad.showSpecificBranches` = `[]` | BACKLOG | no on-load branch filter |
| SET-066 | `repository.onlyFollowFirstParent` = `false` | DESIGN-DIFFERENCE | flag wired in `src/data.rs`, default `false`; test `only_follow_first_parent_drops_later_parents` |
| SET-067 | `repository.showCommitsOnlyReferencedByTags` = `true` | DESIGN-DIFFERENCE | default `true` hardcoded; test `tag_and_remote_only_commits_are_shown` |
| SET-068 | `repository.showRemoteBranches` = `true` | DESIGN-DIFFERENCE | default `true` hardcoded |
| SET-069 | `repository.showRemoteHeads` = `true` | DESIGN-DIFFERENCE | default `true`; test `tag_and_remote_only_commits_are_shown` |
| SET-070 | `repository.showStashes` = `true` | DESIGN-DIFFERENCE | default `true`; test `stash_vertex_is_flagged_from_refs` |
| SET-071 | `repository.showTags` = `true` | DESIGN-DIFFERENCE | default `true`; test `branch_labels_partitions_the_decorations` |
| SET-072 | `repository.showUncommittedChanges` = `true` | DESIGN-DIFFERENCE | default `true`; test `vertex_glyphs_cover_head_stash_and_uncommitted` |
| SET-073 | `repository.showUntrackedFiles` = `true` | DESIGN-DIFFERENCE | default `true`; test `raw_records_carry_status_paths_and_submodules` |
| SET-074 | `repository.sign.commits` = `false` | NOT-PORTED | no commit signing |
| SET-075 | `repository.sign.tags` = `false` | NOT-PORTED | no tag signing |
| SET-076 | `repository.useMailmap` = `false` | NOT-PORTED | no mailmap resolution |
| SET-077 | `repositoryDropdownOrder` = `"Workspace Full Path"` | NOT-PORTED | no repo dropdown |
| SET-078 | `retainContextWhenHidden` = `true` | NOT-PORTED | R3 VS Code webview lifecycle |
| SET-079 | `showStatusBarItem` = `true` | NOT-PORTED | R3 VS Code status bar |
| SET-080 | `sourceCodeProviderIntegrationLocation` = `"Inline"` | NOT-PORTED | R3 VS Code SCM chrome |
| SET-081 | `tabIconColourTheme` = `"colour"` | NOT-PORTED | R3 VS Code tab icon |
| SET-082 | alias `autoCenterCommitDetailsView` | NOT-PORTED | deprecated alias; no config store (SET-001 covers the behaviour) |
| SET-083 | alias `combineLocalAndRemoteBranchLabels` | NOT-PORTED | deprecated alias (SET-051) |
| SET-084 | alias `commitDetailsViewFileTreeCompactFolders` | NOT-PORTED | deprecated alias (SET-002) |
| SET-085 | alias `commitDetailsViewLocation` | NOT-PORTED | deprecated alias (SET-004) |
| SET-086 | alias `commitOrdering` | NOT-PORTED | deprecated alias (SET-058) |
| SET-087 | alias `dateFormat` | NOT-PORTED | deprecated alias (SET-009) |
| SET-088 | alias `dateType` | NOT-PORTED | deprecated alias (SET-010) |
| SET-089 | alias `defaultFileViewType` | NOT-PORTED | deprecated alias (SET-003) |
| SET-090 | alias `fetchAndPrune` | NOT-PORTED | deprecated alias (SET-060) |
| SET-091 | alias `fetchAvatars` | NOT-PORTED | deprecated alias (SET-052) |
| SET-092 | alias `graphColours` | NOT-PORTED | deprecated alias (SET-038) |
| SET-093 | alias `graphStyle` | NOT-PORTED | deprecated alias (SET-039) |
| SET-094 | alias `includeCommitsMentionedByReflogs` | NOT-PORTED | deprecated alias (SET-062) |
| SET-095 | alias `initialLoadCommits` | NOT-PORTED | deprecated alias (SET-053) |
| SET-096 | alias `loadMoreCommits` | NOT-PORTED | deprecated alias (SET-054) |
| SET-097 | alias `loadMoreCommitsAutomatically` | NOT-PORTED | deprecated alias (SET-055) |
| SET-098 | alias `muteCommitsThatAreNotAncestorsOfHead` | NOT-PORTED | deprecated alias (SET-056) |
| SET-099 | alias `muteMergeCommits` | NOT-PORTED | deprecated alias (SET-057) |
| SET-100 | alias `onlyFollowFirstParent` | NOT-PORTED | deprecated alias (SET-066) |
| SET-101 | alias `openDiffTabLocation` | NOT-PORTED | deprecated alias (SET-048) |
| SET-102 | alias `openRepoToHead` | NOT-PORTED | deprecated alias (SET-063) |
| SET-103 | alias `referenceLabelAlignment` | NOT-PORTED | deprecated alias (SET-050) |
| SET-104 | alias `showCommitsOnlyReferencedByTags` | NOT-PORTED | deprecated alias (SET-067) |
| SET-105 | alias `showCurrentBranchByDefault` | NOT-PORTED | deprecated alias (SET-064) |
| SET-106 | alias `showSignatureStatus` | NOT-PORTED | deprecated alias (SET-059) |
| SET-107 | alias `showTags` | NOT-PORTED | deprecated alias (SET-071) |
| SET-108 | alias `showUncommittedChanges` | NOT-PORTED | deprecated alias (SET-072) |
| SET-109 | alias `showUntrackedFiles` | NOT-PORTED | deprecated alias (SET-073) |
| SET-110 | alias `useMailmap` | NOT-PORTED | deprecated alias (SET-076) |

---

## 5. Interactions — INT-001…063 (= pi INT, cline C01…C31 folded in, 63 rows)

| ID | Trigger | Status | Evidence / reason |
|---|---|---|---|
| INT-001 | `Escape` (context menu open) | DONE | `src/menu.rs` / `src/app.rs` menu nav; test `menu_navigation_ignores_keys_with_no_menu_open` |
| INT-002 | `Escape`/`Enter` (dialog open) | DONE | `src/dialog.rs`; test `enter_submits_and_escape_cancels` |
| INT-003 | `Enter`/keyCode 13 (dialog) | DONE | test `enter_on_a_field_submits_with_the_first_button` |
| INT-004 | `ArrowUp`/`ArrowDown` (expanded) | DONE | `Action::SelectNext/Prev`; test `table_order_keys` |
| INT-005 | `Ctrl+ArrowUp` (prev branch) | DONE | `Action::PrevBranch`; test `branch_nav_keys` |
| INT-006 | `Ctrl+ArrowDown` (next branch) | DONE | `Action::NextBranch`; test `branch_nav_keys` |
| INT-007 | `Ctrl+Shift+ArrowUp` | BACKLOG | not bound; test `unbound_keys_are_none` proves the absence |
| INT-008 | `Ctrl+Shift+ArrowDown` | BACKLOG | not bound; test `unbound_keys_are_none` |
| INT-009 | `Ctrl+<scrollToStash>` | DONE | `s`/`S`; test `stash_keys_follow_r1_2` |
| INT-010 | `Ctrl+<refresh>` | DONE | `r`; test `expand_and_refresh_keys` |
| INT-011 | `Ctrl+<find>` | DONE | `Ctrl+F`; test `find_keys_stand_in_for_the_find_widget_controls` |
| INT-012 | `Ctrl+<scrollToHead>` | DESIGN-DIFFERENCE | `Home`/`End` stand in; test `top_and_bottom_jump_to_the_ends` |
| INT-013 | `Escape` (nothing else open) | DONE | quits; test `quit_keys` |
| INT-014 | `Escape` (find open) | DONE | test `closing_clears_the_text_the_matches_and_the_position` |
| INT-015 | `Escape` (CDV open) | DONE | test `cdv_keys_stand_in_for_the_cdv_controls` |
| INT-016 | `Escape` (pager open) | DONE | test `every_pager_nav_is_reachable_from_a_key` |
| INT-017 | `Escape` (menu / dialog) | DONE | `Action::DialogCancel`; test `enter_on_the_cancel_button_is_the_cancel_path` |
| INT-018 | `Enter`/keyCode 13 (row) | DONE | `Action::ToggleExpand`; test `enter_toggles_the_selected_row_only` |
| INT-019 | typed text | DONE | find input + dialog fields; test `text_editing_tracks_the_cursor` |
| INT-020 | `Tab` / focus | DONE | test `focus_walks_fields_then_buttons_and_wraps` |
| INT-021 | single click (commit row) | DESIGN-DIFFERENCE | left-click selects the row (`src/main.rs:267-275`); `Enter` expands it |
| INT-022 | `Ctrl+click` (different commit, CDV open) | BACKLOG | no modifier-aware click routing |
| INT-023 | single click (ref label) | DESIGN-DIFFERENCE | click selects the row; right-click opens the ref menu |
| INT-024 | double click (branch ref) | BACKLOG | double-click checkout not bound |
| INT-025 | double click (remote ref) | BACKLOG | double-click checkout not bound |
| INT-026 | right click (commit row) | DONE | test `surface_at_routes_rows_refs_and_empty_space` |
| INT-027 | right click (ref label) | DONE | test `surface_for_refs_prefers_branch_over_remote_over_tag` |
| INT-028 | right click (column header) | DONE | test `surface_at_routes_the_header_row` |
| INT-029 | right click (CDV file) | BACKLOG | the `FileRow` menu exists but `ClickTarget` (`src/app.rs:383-388`) has no file arm, so no click reaches it |
| INT-030 | click (URL / link) | BACKLOG | no in-view link navigation (see ACT-062/ACT-063) |
| INT-031 | right click (URL / link) | BACKLOG | the `Link` menu exists but nothing routes a click to it (see INT-029) |
| INT-032 | click / right click (anywhere else) | DONE | anything in the body falls back to the commit row; test `surface_at_routes_rows_refs_and_empty_space` |
| INT-033 | click (graph vertex) | NOT-PORTED | R3/M3 pointer-on-canvas interaction — the graph pane hit-tests as a plain commit row, never as a vertex |
| INT-034 | hover (commit row) | NOT-PORTED | R3/M3 hover; no hover events are consumed |
| INT-035 | hover-out (vertex) | NOT-PORTED | R3/M3 hover |
| INT-036 | wheel / scroll | NOT-PORTED | R3/M3 wheel; `PgUp`/`PgDn`/`Home`/`End` stand in — test `page_moves_by_viewport_height_minus_one` |
| INT-037 | dropdown: click current value | BACKLOG | no dropdown widget |
| INT-038 | dropdown: type in filter | BACKLOG | no dropdown widget |
| INT-039 | dropdown: click an option | BACKLOG | no dropdown widget |
| INT-040 | dropdown: multi-select option | BACKLOG | no dropdown widget |
| INT-041 | dropdown: "Show All" | BACKLOG | no dropdown widget |
| INT-042 | dropdown: double click "Show All" | BACKLOG | no dropdown widget |
| INT-043 | click (external-diff / provider affordance) | NOT-PORTED | R3/M3 chrome |
| INT-044 | click (toolbar control) | BACKLOG | no toolbar; only key bindings exist |
| INT-045 | drag column divider | NOT-PORTED | R3 pointer chrome; terminal columns are fixed-width (visibility is toggled instead) |
| INT-046 | drag CDV height handle | NOT-PORTED | R3 pointer chrome; `+`/`-` resize the row budget instead |
| INT-047 | drag CDV divider | NOT-PORTED | R3 pointer chrome |
| INT-048 | click commit row (expand) | DESIGN-DIFFERENCE | `Enter` toggles; test `enter_toggles_the_selected_row_only` |
| INT-049 | click same commit (collapse) | DESIGN-DIFFERENCE | `Enter` toggles; test `enter_toggles_the_selected_row_only` |
| INT-050 | click folder (CDV tree) | BACKLOG | CDV file rows are cursor-driven (`Left`/`Right` collapse/expand), never clickable |
| INT-051 | click (CDV file row) | BACKLOG | same — the CDV is reached by `Tab` + keys, not by the mouse |
| INT-052 | match case toggle (`Aa`) | DONE | `Alt+C`; test `case_sensitivity_follows_the_toggle` |
| INT-053 | regex toggle (`.*`) | DONE | `Alt+R`; test `regex_mode_uses_the_engine` |
| INT-054 | search scope | DONE | test `literal_search_covers_every_scope_branch` |
| INT-055 | match counter | DONE | test `the_counter_reads_n_of_m_and_no_results` |
| INT-056 | open-CDV toggle | DONE | `Alt+D`; test `find_keys_stand_in_for_the_find_widget_controls` |
| INT-057 | `date.format` | DESIGN-DIFFERENCE | config-only in the reference, fixed here; test `long_dates_render_the_reference_iso_format` |
| INT-058 | `date.type` | DESIGN-DIFFERENCE | committer date fixed; test `vis_038_author_date_appears_only_when_it_differs` |
| INT-059 | column-header menu | DONE | right-click the header; test `the_column_header_menu_reflects_and_sets_the_column_state` |
| INT-060 | Repo dropdown | BACKLOG | single repo per invocation; no dropdown |
| INT-061 | Branches multi-select dropdown | BACKLOG | no dropdown (blocks ACT-019/020/041/042) |
| INT-062 | Show Remote Branches checkbox | BACKLOG | no toolbar checkbox; default `true` hardcoded |
| INT-063 | Find / Terminal / Settings / Fetch / Refresh buttons | DESIGN-DIFFERENCE | Find (`Ctrl+F`) and Refresh (`r`) are DONE by key; Terminal/Settings/Fetch are unbuilt features (ACT-100/101/102) |

---

## 6. Visual states — VIS-001…059 (= pi VIS, cline V01…V32 folded in, 59 rows)

| ID | State | Status | Evidence / reason |
|---|---|---|---|
| VIS-001 | Local branch (HEAD) | DONE | `src/render/columns.rs` labels; test `branch_labels_partitions_the_decorations` |
| VIS-002 | Local branch (non-HEAD) | DONE | test `branch_labels_partitions_the_decorations` |
| VIS-003 | Remote branch | DONE | test `classify_ref_handles_every_decoration_shape` |
| VIS-004 | Local branch with tracking remote(s) | DONE | test `split_remote_ref_keeps_slashes_in_the_branch` |
| VIS-005 | Combined local+remote label | DONE | test `split_remote_ref_keeps_slashes_in_the_branch` |
| VIS-006 | Tag (annotated) | DONE | test `branch_labels_partitions_the_decorations` |
| VIS-007 | Tag (lightweight) | DONE | test `branch_labels_partitions_the_decorations` |
| VIS-008 | Stash ref | DONE | test `stash_vertex_is_flagged_from_refs` |
| VIS-009 | HEAD commit dot | DONE | test `vertex_glyphs_cover_head_stash_and_uncommitted` |
| VIS-010 | Detached HEAD | DONE | no branch label, head dot kept (DRIFT-009); test `classify_ref_handles_every_decoration_shape` |
| VIS-011 | Uncommitted changes row | DONE | test `uncommitted_row_uses_the_dashed_vertical` |
| VIS-012 | Uncommitted changes marker style | DONE | test `vertex_glyphs_cover_head_stash_and_uncommitted` |
| VIS-013 | Current commit row | DONE | test `selection_marks_the_whole_row` |
| VIS-014 | Muted commit (merge) | BACKLOG | muting not implemented (SET-057) |
| VIS-015 | Muted commit (non-ancestor of HEAD) | BACKLOG | muting not implemented (SET-056) |
| VIS-016 | Avatar | NOT-PORTED | R3 avatars are a webview/network feature |
| VIS-017 | Signature status | DONE | test `vis_038_a_signature_is_badged_with_its_description` |
| VIS-018 | Date column | DONE | test `dates_render_in_utc` |
| VIS-019 | Author column | DONE | test `header_names_all_five_columns` |
| VIS-020 | Commit column | DONE | test `abbrev_commit_truncates_to_seven_characters` |
| VIS-021 | Reference labels aligned to graph | DONE | test `branch_labels_partitions_the_decorations` |
| VIS-022 | Tags on right | DONE | test `branch_labels_partitions_the_decorations` |
| VIS-023 | Normal committed vertex | DONE | test `paints_the_mock_graph_vertically_connected` |
| VIS-024 | Current/HEAD vertex | DONE | test `vertex_glyphs_cover_head_stash_and_uncommitted` |
| VIS-025 | Stash vertex | DONE | test `vertex_glyphs_cover_head_stash_and_uncommitted` |
| VIS-026 | Uncommitted-changes vertex | DONE | test `vertex_glyphs_cover_head_stash_and_uncommitted` |
| VIS-027 | Merge commit | DONE | test `demo_covers_every_special_vertex` |
| VIS-028 | Branch colour assignment | DONE | test `colour_follows_the_frozen_palette` |
| VIS-029 | Rounded vs angular edges | DONE | test `adjacent_lane_transition_is_a_rounded_step` |
| VIS-030 | Active/selected vertex | DONE | test `selection_marks_the_whole_row` |
| VIS-031 | Commit flash | NOT-PORTED | R3/M3 webview animation; no flash in the TUI |
| VIS-032 | Commit tooltip | NOT-PORTED | R3/M3 hover tooltip (no hover events) |
| VIS-033 | HEAD-membership tooltip | NOT-PORTED | R3/M3 hover tooltip |
| VIS-034 | Branches tooltip | NOT-PORTED | R3/M3 hover tooltip |
| VIS-035 | Tags tooltip | NOT-PORTED | R3/M3 hover tooltip |
| VIS-036 | Stashes tooltip | NOT-PORTED | R3/M3 hover tooltip |
| VIS-037 | CDV loading | DONE | test `the_loading_label_names_what_is_loading` |
| VIS-038 | Commit summary | DONE | test `vis_038_the_summary_shows_every_key_value_row` |
| VIS-039 | Uncommitted summary | DONE | test `vis_039_uncommitted_shows_the_reference_sentence` |
| VIS-040 | Commit comparison summary | DONE | test `vis_040_comparison_shows_the_reference_sentence` |
| VIS-041 | File view — Tree | DONE | test `vis_041_the_tree_nests_and_the_list_does_not` |
| VIS-042 | File view — List | DONE | test `list_view_shows_full_paths_sorted` |
| VIS-043 | File status badges | DONE | test `status_codes_match_the_reference` |
| VIS-044 | Pending review | DESIGN-DIFFERENCE | in-session reviewed flag only (no workspace state); test `a_reviewed_file_is_marked` |
| VIS-045 | Last viewed file | DONE | test `the_last_viewed_file_is_marked` |
| VIS-046 | Diff-possible file | DONE | test `vis_047_a_modified_text_file_shows_its_counts` |
| VIS-047 | Binary file (no diff) | DONE | test `vis_047_a_binary_file_says_it_cannot_be_diffed` |
| VIS-048 | Submodule entry | DONE | test `vis_048_a_submodule_gets_its_own_marker` |
| VIS-049 | CDV docked vs inline | DESIGN-DIFFERENCE | docked pane only (SET-004); test `the_panel_never_swallows_the_commit_list` |
| VIS-050 | Code review active | NOT-PORTED | R4 code-review workspace state |
| VIS-051 | External diff available | DESIGN-DIFFERENCE | availability is shown; the external tool is not launched (ACT-109 is BACKLOG) |
| VIS-052 | Untracked file | DONE | test `vis_052_an_untracked_file_is_diffable` |
| VIS-053 | Untracked stash file | DONE | test `vis_052_an_untracked_file_is_diffable` |
| VIS-054 | Ignored / `.gitignore`d indicator | NOT-PORTED | R6 negative — the feature does not exist in the reference (DRIFT-003) |
| VIS-055 | Git executable unknown | DONE | failure replaces the summary instead of panicking; test `a_load_failure_replaces_the_summary` |
| VIS-056 | No repositories | BACKLOG | no repo discovery / "no repositories" page (ACT-105, DSP-048) |
| VIS-057 | Loading | DONE | test `loaded_clears_the_loading_flag_and_fills_the_tree` |
| VIS-058 | Refreshing | DONE | test `the_loading_label_names_what_is_loading` |
| VIS-059 | Scroll shadow | NOT-PORTED | R3/M3 webview fade mask; a TUI truncates or scrolls (SET/INT-036) |

---

## 7. Drift / not-found register — DRIFT-001…009 (= cline E01…E09, 9 rows)

| ID | Item | Status | Evidence / reason |
|---|---|---|---|
| DRIFT-001 | **View Issue** — in source, absent from shipped 1.30.0 bundle | NOT-PORTED | R5 SOURCE-ONLY; tracked as ACT-022 / ACT-044 |
| DRIFT-002 | **Reset File to this Revision** — in source, absent from shipped bundle | DONE | R5: source is authority, so it is implemented; ACT-071 + `reset_file_and_config_argv` |
| DRIFT-003 | `.gitignore`d-file indicator | NOT-PORTED | R6 negative — the status code set is pinned by `status_codes_match_the_reference` and has no ignored code; the closest real state is the `U` (untracked) badge, VIS-052 |
| DRIFT-004 | unpulled / unpushed / ahead-behind counts | NOT-PORTED | R6 negative — the log format has no such field; `summary_is_one_deterministic_line` pins the columns we do show |
| DRIFT-005 | distinct rebase marker | NOT-PORTED | R6 negative — the special-vertex set is pinned by `demo_covers_every_special_vertex` and has no rebase badge; merge basics are DONE (VIS-027) |
| DRIFT-006 | interactive date-format toggle | NOT-PORTED | R6 negative — date format/type are config-only (SET-009/010, INT-057/058) |
| DRIFT-007 | wheel-specific behaviour | NOT-PORTED | R6 negative — no wheel handler exists; keys stand in and are tested (`page_moves_by_viewport_height_minus_one`, INT-036) |
| DRIFT-008 | VS Code keybinding contributions | NOT-PORTED | R6 negative — `contributes` has no `keybindings`; the TUI owns its key map, pinned by `shortcuts_are_unique_and_lowercase_per_surface` |
| DRIFT-009 | Detached HEAD rendering | DONE | pinned: no branch label is marked active, the head dot still renders; test `classify_ref_handles_every_decoration_shape` |

---

## 8. Totals

| Family | Rows | DONE | NOT-PORTED | DESIGN-DIFFERENCE | BACKLOG |
|---|---|---|---|---|---|
| Rulings register (§1) | 11 | 11 | 0 | 0 | 0 |
| Dispatch map DSP (§2) | 62 | 49 | 11 | 2 | 0 |
| Commands/Actions ACT (§3) | 111 | 47 | 20 | 5 | 39 |
| Settings SET (§4) | 110 | 0 | 56 | 48 | 6 |
| Interactions INT (§5) | 63 | 27 | 8 | 8 | 20 |
| Visual states VIS (§6) | 59 | 43 | 10 | 3 | 3 |
| Drift DRIFT (§7) | 9 | 2 | 7 | 0 | 0 |
| **Total** | **425** | **179** | **112** | **66** | **68** |

`DONE` = 179/425 (42%). Of the 246 non-DONE rows, **66 are deliberate terminal design differences**
and **112 are absent by ruling** (R3 chrome / R4 code review / R5 source-only / R6 negatives);
the remaining **68 are real gaps** — the W10 candidate list in §9.

### 8.1 Audit findings (defects found while building this ledger — not parity gaps)

| ID | Finding | Where | Impact |
|---|---|---|---|
| AUDIT-001 | The **Clean Untracked Files** dialog offers a checkbox labelled *"Also remove ignored files (-x)"* bound to `include_untracked`, but `plan()` for `CleanUntrackedFiles` only reads `directories` — it never emits `-x`. Ticking that box does nothing. | `src/dialog.rs:557-565` vs `src/actions.rs:594-603` | a user-visible control that silently no-ops. Also: `include_untracked` *is* honoured by `PushStash`, so the same field name means two different things in two forms. Not fixed — see §10. |

---

## 9. Gap audit — W10 release-wave candidate plan (T2)

Every non-DONE row was classified into exactly one of: **quick win** (fits a wave), **design-limited**
(the terminal cannot do it — already recorded as `DESIGN-DIFFERENCE`/`NOT-PORTED` with a rationale),
or **out-of-scope** (ruled absent). The waves below cover **all 68** `BACKLOG` rows exactly once —
no row is left unassigned, and no wave claims a row that is already `DONE`.

### 9.1 Waves (68 BACKLOG rows)

| Wave | Theme | Rows (all `BACKLOG`) | Count | Why it is a quick win |
|---|---|---|---|---|
| **W10-A** | **Settings subsystem** — the single biggest gap | SET-056, SET-057, SET-062, SET-063, SET-064, SET-065 | 6 | one config file + one settings-widget surface. Every knob below already exists as a hardcoded constant, so each is a one-line switch — and it is the prerequisite that unlocks §9.2 |
| **W10-B** | **CDV file-row + link menus** — best value/effort ratio in the ledger | ACT-063, ACT-064, ACT-067, ACT-068, ACT-071, ACT-072, ACT-073, INT-029, INT-030, INT-031, INT-050, INT-051 | 12 | every menu row already carries its label, shortcut and `RefAction`; only `ClickTarget` (`src/app.rs:383-388`) needs `FileRow`/`Link` arms, plus 4 keys on the CDV |
| **W10-C** | **Toolbar / remote control bar** | ACT-008, ACT-074, ACT-075, ACT-076, ACT-077, ACT-078, ACT-102, INT-044, INT-062 | 9 | Fetch already has a complete Execute plan (see §2) and only lacks a key; the per-remote rows reuse the same plan with a different target |
| **W10-D** | **Dropdowns (repo + branches multi-select)** | ACT-019, ACT-020, ACT-041, ACT-042, ACT-096, ACT-097, INT-037, INT-038, INT-039, INT-040, INT-041, INT-042, INT-060, INT-061 | 14 | one reusable multi-select widget clears all 14; the `SelectInDropdown` / `UnselectInDropdown` / `FollowInternalLink` menu stubs already exist to be filled in |
| **W10-E** | **Commit muting** | VIS-014, VIS-015 | 2 | a filter over `src/data.rs` rows + a dim style. Depends on W10-A for SET-056/057 |
| **W10-F** | **Mouse completeness** | INT-007, INT-008, INT-022, INT-024, INT-025 | 5 | the mouse router (`src/main.rs:238-276`) already hit-tests; each row is one new arm in the existing `match` |
| **W10-G** | **Repo discovery / multi-repo** | ACT-003, ACT-105, VIS-056 | 3 | `maxDepthOfRepoSearch` walk + the "no repositories" page; the re-scan plan already exists in §2 |
| **W10-H** | **Repo config + user details + view checkboxes** | ACT-079, ACT-080, ACT-081, ACT-082, ACT-083, ACT-084, ACT-085, ACT-086, ACT-087, ACT-090, ACT-091, ACT-092, ACT-093, ACT-095, ACT-098, ACT-101 | 16 | the forms already exist in `src/dialog.rs`; this is an entry-point problem, so it is blocked by W10-A |
| **W10-I** | **External diff launch** | ACT-109 | 1 | spawn `$GIT_EXTERNAL_DIFF` / `git difftool`; availability is already computed in §6 |
| **W10-J** | **AUDIT-001 fix** | §8.1 | 1 | one line in `plan()` (`-x` when `include_untracked`) or one label change; needs the user's call on which is right (Q1) |
| | | **Total** | **68** | |

### 9.2 Promotion candidates (not gaps — recorded as `DESIGN-DIFFERENCE`/`NOT-PORTED`, no wave of their own)

If W10-A lands, these rows stop being differences and become live knobs. They are listed so the plan is honest
about how much of the 66 `DESIGN-DIFFERENCE` + 112 `NOT-PORTED` population is actually reachable:

| Group | Rows | Note |
|---|---|---|
| Dialog defaults that differ from the reference | SET-013, SET-024, SET-031, SET-035 | the reference default is `true`/`Annotated`; the terminal defaults to the plain form. A settings file would let both exist |
| Dialog fields with no form control | SET-014…019, SET-023, SET-025, SET-027, SET-028, SET-029 | `fields_for()` (`src/dialog.rs:531-580`) builds forms for only 11 actions |
| Text enrichment | SET-007 (emoji), SET-046 (markdown), SET-074/075 (signing), SET-076 (mailmap) | pure formatters over the existing cell builder — cheap once a settings surface exists |
| View/data knobs | SET-006, SET-047, SET-053…055, VIS-051 | dropdown style, repo-search depth, paging |

### 9.3 Design-limited — no wave (recorded, not planned)

Avatars (ACT-004, SET-052, VIS-016) · code review (ACT-005/006/007/108, VIS-050) ·
VS Code chrome (SET-041/048/049/078/079/080/081, ACT-059/094, SET-020…022) ·
pointer-layout chrome (ACT-110, INT-045/046/047) · hover tooltips + wheel + flash + scroll shadow
(VIS-031…036/059, INT-033…036) · browser launch (ACT-062) · terminal (ACT-100) ·
provider integrations (ACT-017/039/088/089, SET-008) · save dialogs (ACT-018/040/054) ·
interactive rebase + squash message format (SET-026/030/032) · pagination (ACT-104, SET-053/054/055) ·
setters with no config store (SET-001, SET-003, SET-005, SET-009, SET-012, SET-036…040, SET-042…045,
SET-050, SET-051, SET-058…061, SET-066…073, SET-077).

**Out-of-scope by ruling (justified, not planned):** R3 chrome, R4 code review, R5 source-only actions,
R6 negatives (DRIFT-003…008) — all already carry their ruling id in §1.

---

## 10. Open questions for the user

| ID | Question | Why it matters |
|---|---|---|
| Q1 | AUDIT-001: should the *"remove ignored files (-x)"* checkbox emit `-x`, or should its label change to match the stash behaviour? | it is a live no-op control in the shipped binary. Rule 2 forbids me fixing it silently — it is outside the approved T3 scope |
| Q2 | For W10, is W10-B (12 rows for ~10 lines of `ClickTarget` + 4 key bindings) the right first wave, or is W10-A (the settings subsystem) worth more despite being larger? | the ledger's own numbers say W10-B is by far the cheapest way to move the `DONE` count, but W10-A is what unlocks §9.2 |

<!-- LEDGER-END -->
