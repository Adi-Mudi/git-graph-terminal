# W11 RELEASE-WAVE VERIFICATION — independent review

Reviewer: independent (not the author) · Worktree `git-graph-terminal-w11` · Branch `ggt-m3-w11` @ `a66b801` · Base `development` @ `0d52cc4`
Report under review: `.IDE_Plans/w11-report.md` (in the worktree). Note: the path given in the brief, `git-graph-terminal/.IDE_Plans/w11-report.md`, does not exist — the report lives in the worktree only.
Method: gates re-run locally from the worktree; no source edited. Cargo is not on the default `PATH` in this shell — `~/.cargo/bin` was prepended for all runs.

## Summary

| # | Item | Result |
|---|------|--------|
| 1 | Gate re-run (test / fmt / clippy) | **PASS** |
| 2 | Dumb fallback: no smcup, list, notice; other tiers keep alt-screen | **PASS** |
| 3 | `--version` = semver + build-time hash; a66b801 worktree fix real | **PASS** (code) / **DEFECT D3** (report row) |
| 4 | README: generated keybindings + sync test, ladder, env toggles, links | **PASS** (code) / **DEFECT D5** (row count) |
| 5 | CI: jobs, TERM matrix, pins, no secrets | **DEFECT D1** (blocking) / D2 reclassified NOT a defect |
| 6 | Frozen files `src/data.rs`, `src/layout/` unchanged | **PASS** |
| 7 | Honesty audit: no unbacked "CI passed" / "tested on N terminals" | **PASS** |

## 1. Gate re-run — PASS

| Check | Command | Evidence | Result |
|-------|---------|----------|--------|
| Tests | `cargo test` | 282/0 (bin) · 282/0 (app_state) · 15/0 (golden) · 63/0 (integration) · 61/0 (perf) = **703 passed, 0 failed**; exit `0` | **PASS** — matches report's 703 |
| Format | `cargo fmt --check` | exit `0`, no diff | **PASS** |
| Clippy | `cargo clippy --all-targets -- -D warnings` | exit `0`, **0 warnings**. Re-verified cold via `CARGO_TARGET_DIR=/tmp/w11-clippy-fresh` → `Finished dev profile`, exit `0`, no warnings | **PASS** |
| Release | `cargo build --release` | exit `0` | **PASS** |

The report's gate numbers are accurate.

## 2. Dumb-terminal fallback — PASS

| Requirement | Evidence | Result |
|-------------|----------|--------|
| No smcup on Dumb | `dumb_tier_never_emits_smcup` — `src/main.rs:472`; asserts `!enter.contains("\u{1b}[?1049h")` (`:481`), no mouse `?1000h` (`:485`), no `?1049l` on leave (`:493`) | **PASS** |
| One rule, both paths | `enters_alt_screen()` = `!caps.dumb` (`src/main.rs:88-90`); used by `write_enter_sequences` (`:96-107`) and `write_leave_sequences` (`:110-118`) | **PASS** |
| Other tiers keep W8 always-alt-screen | `proven_terminal_enters_and_leaves_the_alt_screen` — `src/main.rs:500`, asserts `?1049h` … `?1049l` for `xterm-256color` | **PASS** |
| Commit list still prints | `dumb_fallback_still_renders_the_commit_list` — `src/main.rs:515`; asserts header `commits (`, a row, `caps: plain/mono/dumb`, and **no** `lanes:` | **PASS** |
| One-line notice | `src/main.rs:179-182` `eprintln!("git-graph-terminal: TERM={} is a dumb terminal — falling back to the plain commit list (no alternate screen).")` | **PASS** |
| **Live pty confirmation** | `script(1)` 80x24, `TERM=dumb --mock`, clean env: capture has **no `ESC[?1049h`**, **no `ESC[?1049l`**, **no `ESC[?1000h`**; notice string present; `commits`/`(8)`/`branches`/`(2)` + rows (`Add`, `graph`, `renderer`, `widget`, `Merge branch 'feature/graph'`) all drawn | **PASS** |

## 3. `--version` + worktree hash fix — PASS (code)

| Requirement | Evidence | Result |
|-------------|----------|--------|
| Semver + short hash | `src/version.rs:16-18` `format!("git-graph-terminal {VERSION} ({GIT_HASH})")`; `VERSION` = `env!("CARGO_PKG_VERSION")` (`:10`); `GIT_HASH` = `env!("GGT_GIT_HASH")` (`:13`) | **PASS** |
| Build-time, not runtime | `build.rs:24` `git(&["rev-parse","--short","HEAD"])` → `build.rs:25` `cargo:rustc-env=GGT_GIT_HASH={hash}`. No `git` call at runtime | **PASS** |
| a66b801 fix is real | `build.rs:36-45 watch_git_metadata()` watches `<abs-git-dir>/HEAD` + `<common-dir>/<symbolic-ref HEAD>`; `git_common_dir()` (`:52-62`) uses `--path-format=absolute` + `CARGO_MANIFEST_DIR` fallback. Emitted dep-info: `rerun-if-changed=…/.git/worktrees/git-graph-terminal-w11/HEAD` and `…/.git/refs/heads/ggt-m3-w11` — **not** `.git`. `.git` here is a 119-byte *file*, so the old `rerun-if-changed=.git` would never fire | **PASS** — bug and fix both real |
| Live output | `--version` → `git-graph-terminal 0.3.0 (a66b801)`; matches `git rev-parse --short HEAD`. Run from `/tmp` (outside repo) → same `a66b801`, proving build-time capture. `-V` identical | **PASS** |
| Pinned by tests | `src/version.rs:24-56` (3 tests): semver is 3 numeric parts; hash non-empty, 7–40 hex or `unknown`; line names binary + semver + hash | **PASS** |

**DEFECT D3 (minor, doc)** — `.IDE_Plans/w11-report.md:116` records `| --version | **PASS** — git-graph-terminal 0.3.0 (0d52cc4) |`. `0d52cc4` is the *stale base* hash — exactly the bug the report's own fix note (`:74-90`) describes. The gate row was not refreshed after the fix, so the evidence row contradicts the shipped binary (`a66b801`).

## 4. README — PASS (code)

| Requirement | Evidence | Result |
|-------------|----------|--------|
| Keybindings GENERATED | `src/keymap.rs:674-686 keybindings_markdown()` renders `BINDINGS` (`:316`); `--keybindings` flag at `src/main.rs:151-154` | **PASS** |
| **Sync test exists** | `readme_keybindings_table_is_generated_from_this_map` — `src/keymap.rs:991`. `include_str!("../README.md")`, finds `<!-- KEYBINDINGS:START -->` / `<!-- KEYBINDINGS:END -->` (`:993-1001`), asserts marker order (`:1002-1005`), then `assert_eq!(readme[start..end].trim(), keybindings_markdown().trim())` (`:1006-1010`) — README block must equal generator byte-for-byte | **PASS** |
| Second guard | `binding_table_is_well_formed` — `src/keymap.rs:1016`: every field non-empty, pipe-free, newline-free | **PASS** |
| **No hand-typed drift (proved)** | `--keybindings` output diffed against the README block between the markers → **byte-identical, empty diff** | **PASS** |
| Capability ladder section | README `:147-180`: axis table (colour / glyphs / arrows / frames / mouse / bracketed paste), wide-glyph guard `:166-169`, dumb fallback `:170-173`, `--caps` sample | **PASS** |
| `GGT_OSC52` / `GGT_CLIPBOARD_FILE` | README `:186` and `:187`, defaults `$XDG_RUNTIME_DIR/ggt-clipboard.txt` → `/tmp/ggt-clipboard.txt` | **PASS** |
| Ledger + verify links | Ledger `.IDE_Plans/parity-ledger.md` at README `:16`; wave table `:21-33`; **all 4 relative links resolve** on disk; zero broken | **PASS** |
| Tested-terminal matrix backed | README `:191-207` lists 9 envs; each family's detection has a unit test — `src/term/mod.rs:343` tmux, `:373` zellij, `:384` kitty, `:393` wezterm/WT, `:407` herdr, `:446` vt100, `:335` dumb | **PASS** (probe-level) |

**DEFECT D5 (minor, doc)** — `.IDE_Plans/w11-report.md:47` claims `keymap::BINDINGS` has "**65 rows**". Actual: **69** `Binding {` entries (`src/keymap.rs:316-668`) and the generated table / README block have **69** data rows. The report understates by 4.

## 5. CI — DEFECT D1 (blocking)

Structure is fine; the pty-e2e assertion cannot pass.

| Requirement | Evidence | Result |
|-------------|----------|--------|
| Two jobs | `test` (`.github/workflows/ci.yml:15-44`) and `pty-e2e` (`:51-95`) | **PASS** |
| `test` content | fmt `:36`, clippy `--all-targets -- -D warnings` `:41`, `cargo test` `:44`, cargo cache `:27-34` | **PASS** |
| TERM matrix | `:58-63` — `xterm-256color` (`C.UTF-8`, `plain/256/unicode`) and `vt100` (`C`, `plain/16/ascii`) | **PASS** |
| Actions pinned by version | `:19` `actions/checkout@v4`, `:22`/`:68` `dtolnay/rust-toolchain@stable`, `:27` `actions/cache@v4`, `:92` `actions/upload-artifact@v4` | **PASS** |
| No secrets | grep `secret\|token\|password\|api_key\|credential` across ci.yml → **no matches** | **PASS** |

**DEFECT D1 (BLOCKING) — `grep -q "commits ("` at `.github/workflows/ci.yml:87` can never succeed, so the pty-e2e job is red on its first run.**

1. `pty-shot.sh` defaults `MOCK="${MOCK:-1}"` (`tools/pty-shot.sh:31`); CI passes no `MOCK`, so the asserted frame comes from `"$BIN" --dump --mock` (`tools/pty-shot.sh:103`).
2. `--mock` requests the graph and `--dump` keeps it: `src/main.rs:184` `let show_graph = graph_requested && (!caps.dumb || dump);`. The frame is therefore the **graph** view.
3. `commits (` exists in exactly one place — the **text-list** border title, `src/render/mod.rs:475` `" commits ({}) · branches ({}) — {} "`. The graph view's status bar uses a colon instead (`commits: 8`).
4. Reproduced in a clean env (`env -i PATH=… TERM=xterm-256color LANG=C.UTF-8`), which is what CI provides:
   - `--caps` → `ladder=plain/256/unicode/mouse` ✔
   - frame line 12 → `… commits: 8  branches: 2  caps: plain/256/unicode/mouse  sel: 1/8  expanded: 0  lanes: 2  graph:` → `commits (` **MISSING**, `caps:` present, `graph:` present.
   - `TERM=vt100 LANG=C` → same: `commits (` **MISSING**.
5. Ran the exact CI block against a real `pty-shot.sh` capture: `test -s` PASS, then `grep -q "commits ("` **FAILS**. With `set -euo pipefail` (`ci.yml:84`) the job aborts there.
6. The two assertions are **mutually exclusive** under this script: forcing `MOCK=0` (real repo) yields `commits (` PRESENT but `graph:` **MISSING** (text list has no `graph:` field) — so `ci.yml:87` and `ci.yml:89` cannot both hold for any single `pty-shot.sh` invocation.

Minimal fixes: either assert on the graph frame (`commits:`) with `MOCK=1`, or run `MOCK=0` and drop the `graph:` assertion — not both as written.

**D2 — reclassified NOT a defect.** The matrix value `caps: plain/256/unicode` (`ci.yml:60`) is grepped against a real frame reading `caps: plain/256/unicode/mouse`. This passes only because `grep -q` is a substring match. Fragile, but currently green — recorded as a note, not a defect.

## 6. Frozen files — PASS

`git diff development..HEAD -- src/data.rs src/layout/` → **empty output** (verified twice, including `--stat`). Report `:144` claim confirmed.

## 7. Honesty audit — PASS on false claims, one gap

| Probe | Result |
|-------|--------|
| README: `CI passed` / `CI is green` / `green build` / `verified on` / `tested on <N>` / `all terminals` | **no matches** |
| w11-report: same patterns | **no matches** |
| Report claims CI *ran*? | No — `:118` is scoped to "CI YAML validates (`yaml.safe_load`)", which is true |
| README "Tested terminals" overclaim? | Qualifier at README `:193` ("The probe and the ladder are exercised against this matrix (W8 report, §5 and §7)") is accurate — every listed family has a detection unit test. Live *visual* capture exists only for this host plus the two CI TERMs I ran; README wording stays at probe/ladder level, so no unbacked claim |

No fabricated "CI passed" or "tested on N terminals" claim was found.

**DEFECT D4 (minor, doc)** — the required disclosure is absent from the report. The brief asks the report to state CI is written but **never executed** (no remote, local-only repo). The report never says so; `.IDE_Plans/w11-report.md:118` only claims `CI YAML validates (yaml.safe_load) | PASS`. The honest wording exists only in the artifact header (`.github/workflows/ci.yml:3-4` "Activates once this project has a GitHub remote (the repository does not exist yet…)"), and `git remote -v` is empty, confirming local-only. The report should carry that sentence itself — especially since the pty-e2e job is demonstrably broken, which an "executed" claim would have caught.

---

## Defect register

| ID | Severity | File:line | Defect |
|----|----------|-----------|--------|
| **D1** | **BLOCKING** | `.github/workflows/ci.yml:87` (with `:89`, `tools/pty-shot.sh:31`, `src/main.rs:184`, `src/render/mod.rs:475`) | `grep -q "commits ("` is unsatisfiable — the `MOCK=1` frame is the graph view, which prints `commits:` not `commits (`. Reproduced live in a clean env; job aborts under `set -euo pipefail`. `commits (` and `graph:` are mutually exclusive for any one `pty-shot.sh` mode. |
| D2 | *not a defect* | `.github/workflows/ci.yml:60` | `caps: plain/256/unicode` matches the frame's `caps: plain/256/unicode/mouse` only by `grep -q` substring. Fragile but green — note only. |
| D3 | minor (doc) | `.IDE_Plans/w11-report.md:116` | Gate row records the stale pre-fix hash `0d52cc4` as `--version` PASS evidence; the binary reports `a66b801`. |
| D4 | minor (doc) | `.IDE_Plans/w11-report.md:92-118` | No statement that CI is written but never executed (no remote, local-only). |
| D5 | minor (doc) | `.IDE_Plans/w11-report.md:47` | "65 rows" — actual `BINDINGS` is **69** entries / 69 generated rows. |

Blocking: 1 · Minor (doc): 3

## Verdict

The engineering is sound: all three gates are genuinely green (703/0, fmt clean, clippy 0 warnings verified cold), the dumb fallback is real and correct at both unit and live-pty level, every non-Dumb tier keeps the W8 alt-screen policy, the build-time hash capture and the worktree-staleness fix are correct and proven by the emitted dep-info, the keybindings table is genuinely generated with a real byte-equality sync test (zero drift), the capability ladder / env toggles / links are all present and resolvable, the frozen files are untouched, and no unbacked claim of "CI passed" or "tested on N terminals" appears anywhere.

But the release gate itself does not work: the pty-e2e job's central assertion cannot pass as written, and the report does not disclose that CI has never been executed — the exact omission that let the broken assertion ship. The remaining three defects are report-accuracy only and do not affect the binary.

VERDICT: REJECT (blocking defects: 1, minor: 3)

