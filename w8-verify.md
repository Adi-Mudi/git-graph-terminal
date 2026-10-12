# M3-W8 — Wave verification (independent review; reviewer ≠ author)

Worktree: `../git-graph-terminal-w8` · Branch `ggt-m3-w8` (2 commits) · Base `development` @ `bdcb0d4`
Report under review: `.IDE_Plans/w8-report.md`
Note: design wave (not reference-derived) — judged against its own plan (`w8-plan_v1.0.md`) + report design-decision records.

Toolchain: cargo located at `/home/divakaran/.cargo/bin` (not on default PATH).

## Verdict table

| # | Item | Result | Evidence (file:line / command) |
|---|---|---|---|
| 1 | Gate re-run: cargo test / fmt --check / clippy -D warnings | **PASS** | `cargo test` → 261+267+15+63+61 = **667 passed, 0 failed**; `cargo fmt --check` → clean (exit 0); `cargo clippy --all-targets -- -D warnings` → 0 warnings (exit 0). Matches expected 667/0. |
| 2 | Probe env detection + conservative defaults | **PASS** | `src/term/mod.rs:132-133` reads TERM/TERM_PROGRAM; `family_of` `mod.rs:195-233` handles TMUX(203)/ZELLIJ(206)/KITTY_WINDOW_ID(216)/WEZTERM_*(219-221)/WT_SESSION(226)/TERM. Unknown ⇒ lowest safe: test `unset_term_is_unknown_and_lowest_safe` `mod.rs:325-332` (all axes false). No false claims: `herdr_falls_back_to_env_evidence_only` `mod.rs:407-429` (no COLORTERM ⇒ 256 not truecolor); `vt100_without_colorterm_is_16_colour_ascii` `mod.rs:446-451`; tmux requires 256 TERM for truecolor `mod.rs:250`. Struct doc `mod.rs:94-95`: `false` = "not proven". |
| 3 | Ladder: tiers, ASCII fallback, D-4 tier-preserve, unicode-width guard, renderer-only-ladder | **PASS** | Colour rungs `ColourTier::from_caps` `ladder.rs:54-64` + `quantise` `67-74` (truecolor→256→16→mono); unicode box→ASCII arrows/frames test `ladder.rs:642-661`. **D-4 fix present**: `from_tier` sets `ladder.tier = tier;` after match `ladder.rs:308-315`, proven by `every_colour_tier_cross_every_glyph_tier_is_drawable` `ladder.rs:607-639` (asserts `glyph_tier()==tier` for Dumb). unicode-width guard: `is_ambiguous_width`/`display_width`/`char_width`/`truncate_cells`/`pad_cells` `ladder.rs:460-490+`, `guarded_glyphs` `546-553`, CJK downgrade test `from_caps_downgrades_a_cjk_locale_to_ascii` `713-725`; `pad_cells` wired into branch-label column `render/mod.rs:405`. **Renderer consumes only the ladder**: `grep -E 'env::var|std::env|TERM|COLORTERM|NO_COLOR|TMUX|ZELLIJ|KITTY|WEZTERM|WT_SESSION' src/render/` → only doc-comment prose (`glyphs.rs:19,119`), zero env reads. `graph_widget.rs:72-107` + `render/mod.rs` use `ladder.*` exclusively. |
| 4 | Frozen files byte-identical to base | **PASS** | `git diff development..HEAD -- src/layout src/data.rs` → empty. `git merge-base development HEAD` = `bdcb0d4` (base confirmed). |
| 5 | Degraded simulation: vt100 + no-COLORTERM fixtures + pty spot-check | **PASS** | `vt100_without_colorterm_is_16_colour_ascii` `mod.rs:446-451` covers `TERM=vt100`, `LANG=C`, **no COLORTERM** ⇒ asserts `colour && !truecolor && !ansi256` (16-colour), `!unicode && !mouse`. pty spot-check `target/pty/w8-vt100.frame`: `caps: plain/16/ascii`, ASCII glyphs (`o`, `*-+`, `\|`, `@`), `lanes: 2 graph: 4 cols` — lane geometry preserved. |
| 6 | Report integrity: design decisions + D-1..D-5 + smcup/rmcup policy | **PASS** | Design decisions recorded: §5 rows 10-11 "W8 performs no terminal queries at all" (also `mod.rs:11-14`). D-1..D-5 self-found section present: §6.1 layout untouched, §6.2 unicode-width not-new-dep, §6.3 caps placement, §6.4 **D-4 from_tier(Dumb) self-found bug**, §6.5 D-5 alt screen; plus §11 retrospective items 5-7. smcup/rmcup policy = always-alt-screen documented: §6.5 + §10 item 1 recommends (a) "always alt-screen, always restore"; code confirms `EnterAlternateScreen` unconditional `main.rs:78`, mouse gated `main.rs:82-84`. |

## Notes / non-defects observed

1. Two env reads exist outside the probe but are **not** terminal-capability claims and are **not** in `render/`: `main.rs:315` (`GGT_OSC52`, an OSC-52 clipboard feature toggle) and `actions.rs:1216` (`GGT_CLIPBOARD_FILE`/`XDG_RUNTIME_DIR`, clipboard file path). Neither affects the ladder or the renderer-only-ladder invariant.
2. The report does not contain the literal phrase "non-reference-derived"; it satisfies the plan's actual requirement (`w8-plan_v1.0.md:25-26` "mark design decisions in the report") via §5, §6 (D-1..D-5) and §9. Not a defect.

## Defects

N = 0.

VERDICT: ACCEPT
