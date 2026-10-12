# M3-W11 — Report: release wave (dumb fallback, docs, version, CI)

Base: `development` @ 0d52cc4 · Branch `ggt-m3-w11` · Worktree `../git-graph-terminal-w11`
Artifacts: `README.md`, `build.rs`, `src/version.rs`, `src/keymap.rs` (binding table),
`src/main.rs` (fallback + flags), `.github/workflows/ci.yml`

## Reply: DONE — the four tasks

| Task | What shipped | Where |
|---|---|---|
| **T1** Dumb-terminal fallback | No `smcup`/alt-screen on the Dumb tier; render the plain commit list instead; restore nothing | `src/main.rs` |
| **T2** Release README | What it is, parity position, install, generated keybindings, capability ladder, env toggles, tested terminals | `README.md` |
| **T3** Release build + version | `0.3.0` + git short hash via `build.rs` → `--version` | `Cargo.toml`, `build.rs`, `src/version.rs` |
| **T4** CI | fmt + clippy + test, plus a separate remote pty E2E matrix | `.github/workflows/ci.yml` |

## Phase 1 — T1: Dumb-terminal fallback: DONE

W8 accepted "always alt-screen" for every tier. W11/T1 keeps that for every tier
**except Dumb**, where the full-screen graph is not drawable:

- `TerminalGuard` now carries the probe result (`caps: TermCaps`). It only *leaves*
  the modes `setup_terminal` actually *entered*.
- `enters_alt_screen(&caps)` is the single rule: `!caps.dumb`.
- `write_enter_sequences` / `write_leave_sequences` are split out so a test reads the
  exact byte stream. The Dumb tier emits no `ESC[?1049h` (and no mouse escape).
- The Dumb tier prints a one-line stderr notice and renders the P0 plain commit list
  (`show_graph = false`). `--dump` stays headless and keeps rendering the requested
  view for reproducible evidence.

Three tests pin it (`src/main.rs`):

| Test | Asserts |
|---|---|
| `dumb_tier_never_emits_smcup` | Dumb ⇒ no `ESC[?1049h`, no mouse escape, no `ESC[?1049l` |
| `proven_terminal_enters_and_leaves_the_alt_screen` | non-Dumb ⇒ `ESC[?1049h` … `ESC[?1049l` |
| `dumb_fallback_still_renders_the_commit_list` | fallback frame has the header, a row, `caps: plain/mono/dumb`, and **no** `lanes:` |

## Phase 2 — T2: README: DONE

`README.md` covers what it is (terminal port of VS Code Git Graph), the parity
position (links the ledger and every wave's verify file), install
(`cargo install --path .` / release binary), usage, the keybindings table, the
capability ladder, the env toggles (`GGT_OSC52`, `GGT_CLIPBOARD_FILE`, `NO_COLOR`,
`TERM`/`COLORTERM`), the tested-terminal matrix, and the dev/CI commands.

**No hand-typed drift.** The keybindings table is generated from
`keymap::BINDINGS` (65 rows, grouped by context) via `keybindings_markdown()`, which
`--keybindings` prints. Two tests guard it:

| Test | Asserts |
|---|---|
| `readme_keybindings_table_is_generated_from_this_map` | the README block between the `KEYBINDINGS:START/END` markers **equals** the generated table |
| `binding_table_is_well_formed` | every row is non-empty and pipe-free; no key repeats inside a context |

## Phase 3 — T3: Release build + version: DONE

- `Cargo.toml` `version = "0.3.0"` (the M3 wave tag).
- `build.rs` captures `git rev-parse --short HEAD` at build time and passes it as
  `GGT_GIT_HASH`. Build-time capture is deliberate: `cargo install` binaries are
  usually run outside the repo they were built from, so a runtime `git` call would
  report the user's cwd, not the binary's revision. The hash is best-effort — a
  source tarball with no `.git` still builds and prints `unknown`.
- `src/version.rs` exposes `VERSION`, `GIT_HASH`, `version_line()`; `main.rs` prints
  it for `--version`/`-V` **before** anything touches the repository.

Three tests pin the shape: semver is three numeric parts, the hash is a real short
hash (or `unknown`), and the line names the binary + semver + hash on one line.

```
$ git-graph-terminal --version
git-graph-terminal 0.3.0 (0d52cc4)
```

(The hash is the revision at build time; it becomes the W11 commit hash once this
branch is committed and rebuilt.)

## Phase 4 — T4: CI: DONE

`.github/workflows/ci.yml` has two jobs:

1. **`test`** — `ubuntu-latest`: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`
   (upgraded from `--all-targets` off), `cargo test`, with a cargo cache.
2. **`pty-e2e`** — a separate remote job (D4: all PTY/visual E2E runs are remote-only).
   Matrix: `TERM=xterm-256color` (`plain/256/unicode`) and `TERM=vt100` (`plain/16/ascii`).
   It installs `util-linux`/`binutils`, builds, drives `tools/pty-shot.sh`, then asserts
   the deterministic `--dump` frame contains `commits (`, the promised `caps:` ladder, and
   `graph:`, and uploads `target/pty/` as an artifact.

The G8 parity harness is left as a commented hook — it needs the read-only reference
checkout, which is not a submodule here.

## Gate

| Check | Result |
|---|---|
| `cargo fmt --check` | **PASS** |
| `cargo clippy --all-targets -- -D warnings` | **PASS** — 0 warnings |
| `cargo test` | **PASS** — **703** tests, 0 failed (282 bin + 282 app_state + 15 golden + 63 integration + 61 perf) |
| Tests added this wave | **8** — 3 (T1, `main.rs`) + 3 (T3, `version.rs`) + 2 (T2, `keymap.rs`) |
| `cargo build --release` | **PASS** |
| `--version` | **PASS** — `git-graph-terminal 0.3.0 (0d52cc4)` |
| `--keybindings` ↔ README sync test | **PASS** |
| CI YAML validates (`yaml.safe_load`) | **PASS** — 2 jobs (`test`, `pty-e2e`) |
| Dumb smoke (`TERM=dumb --dump --mock`) | **PASS** (exit 0, list frame) |
| Dumb probe (`TERM=dumb --caps`) | **PASS** — `ladder=herdr/mono/dumb` |

## Rule 12 — plan verification

Each plan item read back and checked against the sources; every `TO` present and every
`FROM` gone.

| Item | Result |
|---|---|
| T1 `TerminalGuard { caps }` present; unit `TerminalGuard;` FROM gone | PASS |
| T1 `enters_alt_screen` / `write_enter_sequences` / `write_leave_sequences` present | PASS |
| T1 `show_graph = graph_requested && (!caps.dumb || dump)` present; old `mock \|\| !(text_list \|\| dump)` FROM gone | PASS |
| T2 `keymap::BINDINGS` + `keybindings_markdown` present | PASS |
| T2 `README.md` KEYBINDINGS markers + table present, matches generator | PASS |
| T3 `Cargo.toml` `version = "0.3.0"`; `0.1.0` FROM gone | PASS |
| T3 `build.rs`, `src/version.rs`, `mod version;`, `--version` arm present | PASS |
| T4 `ci.yml` `--all-targets` clippy + `pty-e2e` matrix present | PASS |

## Notes / deviations

- **No self-merge.** Per the wave's deviation note, the branch is committed and pushed
  only on the user's word; the merge to `development` is *requested*, not performed.
- No dependency added; no install run; no secret in any diff; reference tree read-only.
- Layout/`layout.rs` and `data.rs` untouched (frozen).

## Retrospective

**What went well**
- The Dumb fallback fell out of one rule (`enters_alt_screen`) reused by both the enter
  and leave paths, so "restore only what you entered" is structural, not a special case.
- Generating the keybindings table from the code turned "docs drift" into a failing test.
- Build-time hash capture avoided the classic `cargo install` + runtime-git mistake.

**What failed / friction**
- The README table had to be pasted by hand once, because the sync test reads the file
  it is generated into — a bootstrap chicken-and-egg. Regeneration is mechanical
  thereafter (`cargo run -- --keybindings`), but the first write is manual.
- The pty E2E could not be exercised locally (D4 keeps it remote-only), so T1's
  fallback is covered locally by the render test + byte-stream test, not by a live pty.

**What to improve**
- Add a tiny `tools/gen-keybindings.sh` (or a `--write-readme` flag) so the bootstrap
  paste is scripted too.
- Consider a `GGT_FORCE_DUMB=1` debug toggle to drive the fallback through a real pty in
  the remote job, not only in unit tests.
