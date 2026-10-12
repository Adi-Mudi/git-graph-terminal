# W5 WAVE VERIFICATION — independent verifier report

Wave: M2-W5 (context menus + dispatch) · Initiative: git-graph-terminal · Lane branch: `ggt-m2-w5`
Range verified: `28854b1..bf067df` (4 commits, single wave) · Base: `development` @ `d788e9c`
Verifier: cline (headless, independent) · PM: Hermes · Date: 2026-10-12
Truth docs (read-only, not edited): `.IDE_Plans/w5-plan_v1.0.md`, `g7-checklist.md` (R1–R6),
`g7-inventory-pi.md` (§1 DSP-001..062), `g7-inventory-cline.md` (§2 A11–A73) — all in the reference repo.
Worker report under test: `.IDE_Plans/w5-report.md`.

Method: re-ran every gate from a clean shell (`PATH` includes `$HOME/.cargo/bin`), recounted the
dispatch map by hand rather than trusting the worker's tests, and **reproduced the safety captures
myself** in a throwaway `/tmp` git repo so a real destructive run could not touch the lane.

---

## 1. Per-item results

### 1.1 FULL TEST — PASS
| Gate | Command | Result |
|---|---|---|
| fmt | `cargo fmt --check` | exit 0, clean (`FMT_EXIT=0`) |
| clippy | `cargo clippy --all-targets -- -D warnings` | exit 0, **0 warnings** (`CLIPPY_EXIT=0`) |
| tests | `cargo test` | exit 0, 5 targets green (`TEST_EXIT=0`) |

Per-target counts (from the run log):
| Target | Passed |
|---|---|
| unittests src/main.rs | 129 |
| tests/app_state.rs | 99 |
| tests/golden_layout.rs | 15 |
| tests/integration.rs | 46 |
| tests/perf.rs | 44 |
| **Total** | **333** = 129+99+15+46+44 (matches the task's expected number and the worker's claim) |

No `warning:`/`error:` lines in the log (only test-name strings that contain the word "error").

### 1.2 DSP map integrity — PASS
1. **62 variants.** `src/actions.rs:309-372` — `enum RefAction` has exactly 62 variants; `RefAction::ALL` (line 377) is `[RefAction; 62]`.
2. **id parity is really asserted.** `dsp_table_matches_the_inventory` (actions.rs:1296) asserts `DSP_TABLE.len()==62` and that row *i* carries `DSP-{:03}` = `i+1` (sequential id). Not a smoke test — it walks every row.
3. **label/id parity is really asserted.** `every_dsp_row_has_exactly_one_ref_action_variant` (actions.rs:1305) asserts, per variant: `action.dsp_id()==DSP_TABLE[i].0`, `action.command()==DSP_TABLE[i].1`, and `action as usize == i` (discriminant binds variant↔row). `command()` (actions.rs:450) returns the inventory dispatch-name column.
4. **DSP_TABLE matches the pi source of truth.** Cross-checked tricky rows where the two id columns differ: `DSP-013 createArchive→archive`, `DSP-020 deleteUserDetails→unsetConfigValue`, `DSP-024 editUserDetails→setConfigValue`, `DSP-057 tagDetails→getTagDetails` — all identical to `g7-inventory-pi.md`.
5. **Independent plan-kind count (done by hand, not trusting the test).** Inside `plan()` (actions.rs:456-1100): `PlanKind::Execute`=10, `PlanKind::Local`=12, `PlanKind::OutOfScope`=15, textual `gated(`=27. The 2-call overage over 25 is fully explained: actions.rs:797 (`FetchIntoLocalBranch`) and :936 (`PushBranch`) call `gated(...)` **only inside `if target.force` / `if target.push_mode.is_force()` branches whose else-branch is `PlanKind::Execute`**. With the default test target those resolve to Execute, so the runtime mix is **10 execute + 25 gated + 12 local + 15 out-of-scope = 62** — exactly the report's `10/25/12/15` and the passing `plan_kind_mix_has_the_documented_shape` (actions.rs:1619, which asserts those four numbers).
   → **Report's 10/25/12/15 = CONFIRMED correct.**

### 1.3 Menu surfaces — PASS
1. **9 surfaces.** `enum MenuSurface` (menu.rs:24-43) = Branch, CommitRow, RemoteBranch, Stash, Tag, Uncommitted, ColumnHeader, Link, FileRow; `ALL` (menu.rs:63) is `[MenuSurface; 9]`.
2. **Parity tests are genuine `assert_eq!`** against hardcoded inventory rows (menu.rs:853, 874, 894, 914, 948), and `the_nine_surfaces_cover_a11_through_a73_once` (menu.rs:985) pins total drawn rows to 67 (63 ids + 4 header repeats). All ran green.
3. **Spot-checked 8+ rows across 9 different surfaces against `g7-inventory-cline.md` §2 (file:line cited):**
   - Branch: A11 "Checkout Branch…" (cline:101), A13 "Delete Branch…" (:103), A16 "Push Branch…" (:106), A19 "Select in Branches Dropdown" (:109)
   - Commit: A25 "Checkout…" (:123), A31 "Reset current branch to this Commit…" (:129), A32 "Copy Commit Hash to Clipboard" (:130), A33 (:131)
   - Remote: A36 (:142), A38 (:144) · Stash: A47 (:160) · Tag: A53 (:175) · Uncommitted: A58 (:187) · Header: A60 (:196) · Link: A62 (:205) · FileRow: A69 (:219)
   - All labels/ids match the hardcoded test rows byte-for-byte.
4. **Shortcut-collision test exists and passes.** `no_shortcut_shadows_a_navigation_key` (menu.rs:1038) + `shortcuts_are_unique_and_lowercase_per_surface` (:1024). The former pins the Commit surface to `['a','b','c','d','e','f','g','h','i','l','m']`, so **A32 = 'l'** (A31='i', A33='m'; `j`/`k` reserved for nav) — matching the task's "A32 = 'l'" requirement. Both tests show `... ok` in the run.

### 1.4 Safety — PASS (reproduced independently)
Destructive action must never execute; safe action must write clipboard fallback. Reproduced in a throwaway
repo `/tmp/w5-safety` (3 commits, HEAD=`4e2a052…`) so the lane worktree could not be mutated.

1. **A31 "Reset current branch to this Commit…" is GATED — no git mutation.**
   - Ran `MOCK=0 KEYS="down,m,i"` (row 2 → Commit menu → `i`=A31) via `tools/pty-shot.sh`.
   - `git rev-parse HEAD` before **and** after: `4e2a052918af40bbd7ddebf2bd351bf5e0bec207` (unchanged).
   - `git status --porcelain`: empty (clean). `git reflog`: exactly 3 entries, all `commit:` — **no reset/checkout entry**.
   - Status line captured: `last: Reset to Commit: needs confirmation (W6 …` — the gated path.
   - Backed by unit test `dispatch_refuses_gated_plans_without_running_git` (actions.rs:1661, green).
2. **A32 "Copy Commit Hash to Clipboard" writes the clipboard fallback — verified by running.**
   - Ran `MOCK=0 KEYS="down,m,l" GGT_CLIPBOARD_FILE=/tmp/w5-verify-clip.txt GGT_OSC52=1` (row 2 → `l`=A32).
   - Fallback file `/tmp/w5-verify-clip.txt` content: `76c98267e05dedcd49f305f1602e7e4f4a6c599d` = **full hash of the selected commit** (commit 2, the row `down` selects). Correct.
   - OSC52 emitted once in the raw stream; decoded payload = the same hash. Status line: `last: Copy to Clipboard: copied to /tmp/…`.
   - Backed by `dispatch_writes_the_clipboard_file` (actions.rs:1673, green).

### 1.5 Scope guard — PASS
1. **`git diff --stat development..HEAD` touches exactly 8 files:** `src/actions.rs`, `src/app.rs`, `src/keymap.rs`, `src/main.rs`, `src/menu.rs`, `src/render/columns.rs`, `src/render/mod.rs`, `tests/app_state.rs`. All are in-plan targets.
2. **`src/data.rs` untouched:** `git diff development..HEAD -- src/data.rs` = 0 lines. **`src/layout/*` untouched:** diff = 0 lines. Both byte-identical to `development`.
3. **No push:** `git rev-parse @{u}` → "no upstream configured for branch 'ggt-m2-w5'". Nothing pushed.
4. **Conventional commits:** all 4 subjects match `^(feat|fix|docs|test|chore|refactor|ci)(\(.+\))?: .+` (`feat(actions|menu|app|render): … (M2-W5)`).
5. **Single wave, 4 commits:** `git rev-list --count development..HEAD` = 4; parent of the first W5 commit `28854b1` is `d788e9c` (the W4 verify baseline). No interleaving with other lanes.

### 1.6 Deviations — as instructed
1. **Report location:** worker wrote `.IDE_Plans/w5-report.md` (inside `.IDE_Plans/`, the reference-repo plan store). The task granted this as an explicit exception, so it is **allowed**, not a defect. No `.IDE_Plans` file was edited by the verifier.
2. **Single-wave 4 commits** `28854b1..bf067df` exactly as instructed — confirmed above.
3. **Self-declared deviations** in report §7 (skip `j`/`k` shortcuts, move `last:` to front, header menu mouse-only, shared `act` id, `dispatch()` refuses Confirm) are all reasonable, in-scope, and backed by tests/captures. None change the contract verdict.

---

## 2. Defects table

| # | Severity | Where | Description | Blocking? |
|---|---|---|---|---|
| 1 | **Cosmetic (doc)** | `.IDE_Plans/w5-report.md` §2 T1/T2 prose | Deliverable prose says `src/menu.rs (new, 1209 lines)` and `src/actions.rs (new, 1737 lines)`, but actual files are **1230** and **1685** lines. The report's own files-changed table (§2) has the **correct** `+1230 / +1685`, so only the T1/T2 sentence drifted. Pure documentation inaccuracy — no code or test impact. | No |

**Functional defects: 0.** The single item above is a non-blocking documentation nit and does not affect any gate, contract row, or safety property.

---

## 3. Verdict

All six verification areas pass with command evidence. 333/333 tests, fmt/clippy clean, the 62-row DSP
map and 9-surface menu model match both inventories with genuine (not smoke) parity assertions, the
destructive A31 path is provably gated with zero git mutation, the A32 clipboard fallback writes the
correct hash, and the scope guard holds (data.rs + layout untouched, no push, conventional single-wave
history). The only finding is one cosmetic line-count typo in the worker report prose.

**VERDICT: ACCEPT** — 0 functional defects, 1 cosmetic doc nit (non-blocking).
