# git-graph-terminal

A terminal port of the VS Code **Git Graph** extension
([mhutchie/vscode-git-graph](https://github.com/mhutchie/vscode-git-graph)):
the same commit-graph experience — branch/merge topology, commit nodes, refs,
HEAD marker, commit details, find and context actions — as a single, small
terminal binary.

Rust + [ratatui](https://ratatui.rs) / [crossterm](https://github.com/crossterm-rs/crossterm),
no runtime service, no Node.

## Parity position

The port is reverse-engineered against the reference source, not re-imagined.
Every contract row is classified in the **function-parity ledger**
([`.IDE_Plans/parity-ledger.md`](.IDE_Plans/parity-ledger.md)): 479 rows, zero
blanks, each `DONE` row carrying a code cite and a test that exists in the tree.

Verification is per wave and lives beside the code:

| Wave | Scope | Verify |
| --- | --- | --- |
| P0 | setup + text-list fallback | [p0-verify.md](p0-verify.md) · [p0-verify-p01.md](p0-verify-p01.md) |
| M1-W1 | renderer (cell grid, glyphs, lanes) | [w1-verify.md](w1-verify.md) |
| M1-W2 | data → layout → render + perf | [w2-verify.md](w2-verify.md) |
| M2-W4 | interactions core | [w4-verify.md](w4-verify.md) |
| M2-W5 | context menus + dispatch | [w5-verify.md](w5-verify.md) |
| M2-W6 | find widget + dialogs + CDV | [w6-verify.md](w6-verify.md) |
| M2-W7 | runtime wiring | [w7-verify.md](w7-verify.md) |
| M3-W8 | terminal matrix + capability ladder | [w8-verify.md](w8-verify.md) |
| M3-W9 | parity ledger | [`.IDE_Plans/w9-report.md`](.IDE_Plans/w9-report.md) |
| M3-W10 | CDV file-row menu | [w10-verify.md](w10-verify.md) · [`.IDE_Plans/w10-report.md`](.IDE_Plans/w10-report.md) |
| M3-W11 | release wave (dumb fallback, docs, version, CI) | [`.IDE_Plans/w11-report.md`](.IDE_Plans/w11-report.md) |

The remaining gaps (settings widget, dropdowns, multi-repo, muting) are listed
in §9 of the ledger and are deliberate `BACKLOG`, not silent omissions.

## Install

From a checkout:

```sh
cargo install --path .
```

Or build a release binary directly:

```sh
cargo build --release
./target/release/git-graph-terminal --version
```

`--version` prints the semver plus the exact git revision the binary was built
from, e.g. `git-graph-terminal 0.3.0 (abc1234)`.

## Usage

```sh
git-graph-terminal [REPO]            # graph view (default: current directory)
git-graph-terminal --text-list [REPO]  # P0 plain commit list
git-graph-terminal --mock            # demo fixture, no repository needed
git-graph-terminal --dump [REPO]     # print one frame and exit (headless)
git-graph-terminal --caps            # print the capability probe and exit
git-graph-terminal --keybindings     # print the table below and exit
git-graph-terminal --version         # semver + git short hash
```

## Keybindings

The table is **generated from `src/keymap.rs`** (`--keybindings`); a test fails
if this section drifts from the code, so it cannot go stale.

<!-- KEYBINDINGS:START -->
| Keys | Action | Context |
| --- | --- | --- |
| `q` | Quit | Graph view |
| `Esc` | Quit (or close the open overlay) | Graph view |
| `j` | Next commit (table order) | Graph view |
| `k` | Previous commit | Graph view |
| `PgDn` | Page down | Graph view |
| `PgUp` | Page up | Graph view |
| `Home` | First commit | Graph view |
| `End` | Last commit | Graph view |
| `Right` | Follow to the first parent | Graph view |
| `Left` | Follow to the first child | Graph view |
| `Enter` | Expand / collapse the commit | Graph view |
| `s` | Next stash (no Shift = next, R1.2) | Graph view |
| `Shift+s` | Previous stash | Graph view |
| `r` | Refresh | Graph view |
| `m` | Open the context menu for the selection | Graph view |
| `Shift+F10` | Open the context menu (menu key) | Graph view |
| `Ctrl+F` | Open / close the find widget | Graph view |
| `F3` | Find: next match | Graph view |
| `Shift+F3` | Find: previous match | Graph view |
| `Alt+C` | Find: toggle match case | Graph view |
| `Alt+R` | Find: toggle regex mode | Graph view |
| `Alt+D` | Find: open the CDV for each match | Graph view |
| `d` | Open the commit details view (CDV) | Graph view |
| `u` | Open the uncommitted-changes view | Graph view |
| `c` | Mark the commit for comparison | Graph view |
| `Tab` | Focus / unfocus the CDV file list | Graph view |
| `v` | Toggle CDV Tree / List | Graph view |
| `+` | Grow the CDV | Graph view |
| `-` | Shrink the CDV | Graph view |
| `Ctrl+W` | Close the CDV | Graph view |
| `j` | Next item | Context menu |
| `k` | Previous item | Context menu |
| `Enter` | Activate the item | Context menu |
| `letter` | Activate the item by its shortcut | Context menu |
| `Esc` | Close the menu | Context menu |
| `character` | Append to the query | Find widget |
| `Backspace` | Delete the last query character | Find widget |
| `Enter` | Next match | Find widget |
| `Shift+Enter` | Previous match | Find widget |
| `Esc` | Close the find widget | Find widget |
| `Enter` | Submit the dialog | Dialog |
| `Esc` | Cancel the dialog | Dialog |
| `Tab` | Next field | Dialog |
| `Shift+Tab` | Previous field | Dialog |
| `Left` | Move the text cursor | Dialog |
| `Home` | Start of the field | Dialog |
| `End` | End of the field | Dialog |
| `Backspace` | Delete before the cursor | Dialog |
| `Delete` | Delete at the cursor | Dialog |
| `Space` | Toggle / insert a space | Dialog |
| `j` | Scroll down one line | Pager |
| `k` | Scroll up one line | Pager |
| `PgDn` | Scroll down one page | Pager |
| `PgUp` | Scroll up one page | Pager |
| `Home` | Top of the document | Pager |
| `End` | Bottom of the document | Pager |
| `q` | Close the pager | Pager |
| `j` | Next file | CDV file list |
| `k` | Previous file | CDV file list |
| `Enter` | View diff (A65) | CDV file list |
| `o` | View file at revision (A66) | CDV file list |
| `w` | View diff with working file (A67) | CDV file list |
| `e` | Open file (A68) | CDV file list |
| `Space` | Toggle reviewed (A69/A70) | CDV file list |
| `R` | Reset file to this revision (A71, gated) | CDV file list |
| `y` | Copy absolute path (A72) | CDV file list |
| `Y` | Copy relative path (A73) | CDV file list |
| `Left` | Collapse folder | CDV file list |
| `Right` | Expand folder | CDV file list |
<!-- KEYBINDINGS:END -->

## Capability ladder

At startup the binary probes the environment **once** and builds a ladder. The
renderer never reads the environment; it draws from the ladder, so every
degradation is explicit and testable. Detection is conservative: anything not
proven from the environment drops to the lowest safe rung for that axis, so an
unrecognised terminal still gets a correct, aligned graph.

| Axis | Most capable → least capable | Evidence |
| --- | --- | --- |
| Colour | truecolor → 256 → 16 → mono | `COLORTERM=truecolor`/`24bit`, kitty, WezTerm; `TERM=…256color`; `TERM=vt100`; `TERM=dumb`/unset or `NO_COLOR` |
| Glyphs | Unicode (`● ◯ ◎`) → ASCII (`o o @`) → Dumb (`. o o`) | UTF-8 locale or a modern family; unproven; `TERM=dumb` |
| Arrows | `↑ ↓ ← →` → `^ v < >` | same rung as glyphs |
| Frames | `┌ ─ ┐ │` → `+ - |` | same rung as glyphs |
| Mouse | SGR on → off | a known family, a 256-colour `TERM`, or `xterm`/`screen`/`tmux`/`rxvt` |
| Bracketed paste | on → off | same evidence as mouse |

Two guards keep the geometry stable across the ladder:

1. **Wide-glyph guard.** In a CJK locale the East-Asian *ambiguous* glyphs
   (`│ ─ ┼ ● ◯ ↑ ← ✓ …`) render two cells wide, which would shift every row after
   them. The ladder drops those to the ASCII table so a lane is always exactly
   two cells.
2. **Dumb fallback.** On `TERM=dumb` the app does **not** enter the alternate
   screen and falls back to the plain commit list (with a one-line notice on
   stderr). Every other tier always uses the alternate screen, so scrollback is
   restored on exit.

`--caps` prints the machine-readable probe, e.g.:

```
family=plain term=vt100 dumb=false truecolor=false ansi256=false colour=true unicode=false ambiguous_wide=false mouse=false bracketed_paste=false
ladder=plain/16/ascii
```

## Environment toggles

| Variable | Effect |
| --- | --- |
| `GGT_OSC52=1` | Opt in to OSC 52 clipboard writes. Copy rows always write the fallback file; this adds the terminal escape on top. Off by default because some terminals ignore OSC 52 and the sequence has no business in `--dump` output. |
| `GGT_CLIPBOARD_FILE` | Path of the clipboard fallback file. Default: `$XDG_RUNTIME_DIR/ggt-clipboard.txt`, then `/tmp/ggt-clipboard.txt`. |
| `NO_COLOR` | Disables colour only (glyphs still degrade on their own axis). |
| `TERM` / `COLORTERM` | Drive the capability ladder (see above). |

## Tested terminals

The probe and the ladder are exercised against this matrix (W8 report, §5 and
§7); captures are produced with [`tools/pty-shot.sh`](tools/pty-shot.sh).

| Terminal / env | Ladder |
| --- | --- |
| Herdr (real host) | `herdr/truecolor/unicode/mouse` |
| tmux (`screen-256color`, `COLORTERM=truecolor`) | `tmux/truecolor/unicode/mouse` |
| tmux (`screen`, 8/16 colour) | `tmux/16/unicode/mouse` |
| zellij | `zellij/256/unicode/mouse` |
| kitty (`xterm-kitty`) | `kitty/truecolor/unicode/mouse` |
| WezTerm | `wezterm/truecolor/unicode/mouse` |
| Windows Terminal (`WT_SESSION`) | `windows-terminal/…` |
| xterm-256color | `plain/256/unicode` (with a UTF-8 locale) |
| vt100 | `plain/16/ascii` |
| dumb | `plain/mono/dumb` (no alternate screen) |

## Development

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo build --release
```

CI (`.github/workflows/ci.yml`) runs fmt + clippy + the full test suite on
`ubuntu-latest`, plus a separate pty end-to-end job on a `TERM=xterm-256color` /
`TERM=vt100` matrix. Per D4, all visual/PTY end-to-end runs are remote-only; the
local suite stays unit/golden/integration.
