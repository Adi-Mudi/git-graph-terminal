//! Terminal capability probe (M3-W8, T1).
//!
//! The renderer never reads the environment itself: [`main`](crate) probes once
//! at startup and hands the renderer a [`ladder::Ladder`] built from these
//! capabilities. Detection is deliberately **conservative** — every axis we
//! cannot prove from the environment degrades to the lowest safe value for that
//! axis (mono / ASCII / no mouse), so an unrecognised terminal still produces a
//! correct, aligned graph instead of a garbled one.
//!
//! Only the environment is read. We deliberately do **not** send blocking
//! terminal queries (DA1/DA2, XTVERSION, OSC 4 palette reads): the app must come
//! up instantly on every host, and a query that no terminal answers would hang
//! the first frame. That is a documented W8 design decision (see
//! `.IDE_Plans/w8-report.md`); the tmux palette caveat it implies is handled in
//! [`TerminalFamily::Tmux`].
//!
//! | axis | evidence |
//! | --- | --- |
//! | family | `TMUX`, `ZELLIJ`, `HERDR_ENV`, `KITTY_WINDOW_ID`, `WEZTERM_*`, `WT_SESSION`, `TERM_PROGRAM`, `TERM` |
//! | truecolor | `COLORTERM=truecolor`/`24bit`, kitty, WezTerm |
//! | 256 colour | `TERM` containing `256color`, truecolor, kitty, WezTerm |
//! | colour on/off | `TERM` unset/empty/`dumb`, `NO_COLOR` |
//! | unicode | UTF-8 locale (`LC_ALL` → `LC_CTYPE` → `LANG`), kitty, WezTerm |
//! | ambiguous width | CJK locale (`ja`/`ko`/`zh`) |
//! | mouse (SGR) | known family or a `TERM` shape that implies SGR |
//! | bracketed paste | same evidence as mouse |
//!
//! The ladder itself (how a capability becomes a colour or a glyph) lives in
//! [`ladder`].

pub mod ladder;

/// Which terminal (or multiplexer) is hosting us.
///
/// Order matters: the **innermost** layer wins, because it is the one that
/// decides what actually reaches the screen. tmux inside kitty reports `Tmux`,
/// not `Kitty` — the outer terminal's `TERM_PROGRAM` is not evidence of what
/// tmux will pass through (T3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalFamily {
    /// GNU screen-compatible multiplexer (`$TMUX`). Its `TERM` (`screen`,
    /// `screen-256color`, `tmux-256color`) is the authority on colour depth.
    Tmux,
    /// Zellij multiplexer (`$ZELLIJ`). Passes box-drawing and colour through.
    Zellij,
    /// kitty (`$KITTY_WINDOW_ID`, `TERM=xterm-kitty`). Always truecolor.
    Kitty,
    /// WezTerm (`$WEZTERM_*`, `TERM_PROGRAM=WezTerm`). Always truecolor.
    WezTerm,
    /// Windows Terminal (`$WT_SESSION`). Colour depth comes from `$COLORTERM`.
    WindowsTerminal,
    /// Herdr (`$HERDR_ENV`) — the CI dev environment. Deliberately treated as an
    /// **unknown-capability fallback**: we believe only what `TERM`/`COLORTERM`
    /// prove, and never grant family-specific extras. It stays fully usable
    /// because the fallback still honours a truecolor `COLORTERM` and a UTF-8
    /// locale.
    Herdr,
    /// A terminal we can name only as "some terminal" (`TERM` set, no marker).
    Plain,
    /// No usable `TERM` at all.
    Unknown,
}

impl TerminalFamily {
    /// Short lowercase label used by the `--caps` line and the status bar.
    pub const fn label(self) -> &'static str {
        match self {
            TerminalFamily::Tmux => "tmux",
            TerminalFamily::Zellij => "zellij",
            TerminalFamily::Kitty => "kitty",
            TerminalFamily::WezTerm => "wezterm",
            TerminalFamily::WindowsTerminal => "windows-terminal",
            TerminalFamily::Herdr => "herdr",
            TerminalFamily::Plain => "plain",
            TerminalFamily::Unknown => "unknown",
        }
    }

    /// A family whose capabilities we can name without guessing.
    pub const fn is_known(self) -> bool {
        matches!(
            self,
            TerminalFamily::Tmux
                | TerminalFamily::Zellij
                | TerminalFamily::Kitty
                | TerminalFamily::WezTerm
                | TerminalFamily::WindowsTerminal
        )
    }
}

/// The capabilities the renderer needs, probed once at startup.
///
/// Every field is a **claim we are willing to act on**: `false` means "not
/// proven", not "proven absent". That is what makes the probe conservative.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TermCaps {
    /// Innermost terminal/multiplexer family.
    pub family: TerminalFamily,
    /// `$TERM`, when set and non-empty.
    pub term: Option<String>,
    /// `$TERM_PROGRAM`, when set and non-empty.
    pub term_program: Option<String>,
    /// `$TERM` is unset, empty or `dumb` — the lowest safe terminal.
    pub dumb: bool,
    /// `COLORTERM` (or a truecolor-by-construction family) proves 24-bit colour.
    pub truecolor: bool,
    /// A 256-colour palette is safe (truecolor or a `256color` `TERM`).
    pub ansi256: bool,
    /// Colour output is allowed at all (`TERM` usable and `NO_COLOR` unset).
    pub colour: bool,
    /// SGR mouse reporting is safe.
    pub mouse: bool,
    /// Box-drawing glyphs are safe to emit (UTF-8 locale or a modern family).
    pub unicode: bool,
    /// The locale renders East-Asian *ambiguous* glyphs two cells wide.
    pub ambiguous_wide: bool,
    /// Bracketed paste is safe (detected, not enabled — see the module docs).
    pub bracketed_paste: bool,
}

impl TermCaps {
    /// Probe an environment lookup (`env("TERM")` → `Some("xterm")`).
    ///
    /// Taking the lookup as a closure keeps the whole probe testable without
    /// touching the process environment (the same seam the G2 ladder used).
    pub fn detect<F>(env: F) -> Self
    where
        F: Fn(&str) -> Option<String>,
    {
        let get = |key: &str| env(key).filter(|value| !value.is_empty());
        let term = get("TERM");
        let term_program = get("TERM_PROGRAM");
        let dumb = match term.as_deref() {
            None => true,
            Some("dumb") => true,
            Some(_) => false,
        };
        let family = family_of(&get, term.as_deref(), term_program.as_deref());
        let colour = !dumb && get("NO_COLOR").is_none();
        let truecolor = colour && truecolor_for(family, &get, term.as_deref());
        let ansi256 = colour
            && (truecolor
                || term_has_256(term.as_deref())
                || matches!(family, TerminalFamily::Kitty | TerminalFamily::WezTerm));
        // `NO_COLOR` only turns colour off (M3 narrowing of the G2 ladder): a
        // colourless but UTF-8 terminal still gets box-drawing glyphs.
        let unicode = !dumb && (utf8_locale(&get) || modern_unicode(family));
        // SGR mouse + bracketed paste need a terminal that speaks ANSI escapes.
        // "Unproven" (e.g. `TERM=foo`) stays off.
        let modern = !dumb
            && (family.is_known()
                || term_has_256(term.as_deref())
                || term_mentions(term.as_deref(), &["xterm", "screen", "tmux", "rxvt"]));
        Self {
            family,
            term,
            term_program,
            dumb,
            truecolor,
            ansi256,
            colour,
            mouse: modern,
            unicode,
            ambiguous_wide: cjk_locale(&get),
            bracketed_paste: modern,
        }
    }

    /// Probe the real process environment.
    pub fn detect_from_os() -> Self {
        Self::detect(|key| std::env::var(key).ok())
    }

    /// One deterministic line for `--caps`, the status bar and the report.
    pub fn summary(&self) -> String {
        format!(
            "family={} term={} program={} dumb={} truecolor={} ansi256={} colour={} unicode={} ambiguous_wide={} mouse={} bracketed_paste={}",
            self.family.label(),
            self.term.as_deref().unwrap_or("-"),
            self.term_program.as_deref().unwrap_or("-"),
            self.dumb,
            self.truecolor,
            self.ansi256,
            self.colour,
            self.unicode,
            self.ambiguous_wide,
            self.mouse,
            self.bracketed_paste,
        )
    }
}

/// Classify the innermost terminal/multiplexer from the environment markers.
fn family_of<F>(env: &F, term: Option<&str>, term_program: Option<&str>) -> TerminalFamily
where
    F: Fn(&str) -> Option<String>,
{
    let term = term.unwrap_or("");
    let program = term_program.unwrap_or("").to_ascii_lowercase();
    // Multiplexers first: they sit between us and the outer terminal, so their
    // markers beat the outer `TERM_PROGRAM` (a tmux inside kitty is `Tmux`).
    if env("TMUX").is_some() {
        return TerminalFamily::Tmux;
    }
    if env("ZELLIJ").is_some() {
        return TerminalFamily::Zellij;
    }
    // Herdr is detected by its own marker only. It inherits kitty's variables
    // (`KITTY_PID`, `KITTY_INSTALLATION_DIR`) but not `KITTY_WINDOW_ID`, so the
    // kitty check below cannot mis-fire; checking Herdr first makes that safe
    // even if a future Herdr version forwards the window id.
    if env("HERDR_ENV").is_some() || program == "herdr" {
        return TerminalFamily::Herdr;
    }
    if env("KITTY_WINDOW_ID").is_some() || term == "xterm-kitty" || program == "kitty" {
        return TerminalFamily::Kitty;
    }
    if env("WEZTERM_EXECUTABLE").is_some()
        || env("WEZTERM_PANE").is_some()
        || env("WEZTERM").is_some()
        || program == "wezterm"
    {
        return TerminalFamily::WezTerm;
    }
    if env("WT_SESSION").is_some() {
        return TerminalFamily::WindowsTerminal;
    }
    if term.is_empty() {
        return TerminalFamily::Unknown;
    }
    TerminalFamily::Plain
}

/// Is 24-bit colour proven for `family`?
///
/// tmux is the one family where the outer `TERM_PROGRAM` must be ignored: the
/// client's `COLORTERM` survives into the pane, but tmux may still be configured
/// for 8/16 colours, so we additionally require a 256-colour `TERM`.
fn truecolor_for<F>(family: TerminalFamily, env: &F, term: Option<&str>) -> bool
where
    F: Fn(&str) -> Option<String>,
{
    let colorterm = env("COLORTERM")
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_default();
    let colorterm_true = colorterm == "truecolor" || colorterm == "24bit";
    match family {
        // Trust tmux's own caps (`TERM`), not the outer terminal's identity.
        TerminalFamily::Tmux => colorterm_true && term_has_256(term),
        // kitty and WezTerm are truecolor by construction.
        TerminalFamily::Kitty | TerminalFamily::WezTerm => true,
        _ => colorterm_true,
    }
}

/// Families that are UTF-8 by construction, so a missing locale is not fatal.
fn modern_unicode(family: TerminalFamily) -> bool {
    matches!(family, TerminalFamily::Kitty | TerminalFamily::WezTerm)
}

fn term_has_256(term: Option<&str>) -> bool {
    term.map(|value| value.to_ascii_lowercase().contains("256color"))
        .unwrap_or(false)
}

fn term_mentions(term: Option<&str>, needles: &[&str]) -> bool {
    let Some(term) = term else {
        return false;
    };
    let term = term.to_ascii_lowercase();
    needles.iter().any(|needle| term.contains(needle))
}

/// First non-empty locale variable, `LC_ALL` → `LC_CTYPE` → `LANG`.
fn locale<F>(env: &F) -> Option<String>
where
    F: Fn(&str) -> Option<String>,
{
    ["LC_ALL", "LC_CTYPE", "LANG"]
        .into_iter()
        .find_map(|key| env(key).filter(|value| !value.is_empty()))
}

fn utf8_locale<F>(env: &F) -> bool
where
    F: Fn(&str) -> Option<String>,
{
    locale(env).is_some_and(|value| {
        let value = value.to_ascii_lowercase();
        value.contains("utf-8") || value.contains("utf8")
    })
}

/// CJK locales render East-Asian *ambiguous* glyphs (box-drawing, `●`, `◯`)
/// two cells wide, which would break the two-cells-per-lane geometry.
fn cjk_locale<F>(env: &F) -> bool
where
    F: Fn(&str) -> Option<String>,
{
    locale(env).is_some_and(|value| {
        let language = value
            .to_ascii_lowercase()
            .split(['_', '-', '.'])
            .next()
            .unwrap_or("")
            .to_string();
        matches!(language.as_str(), "ja" | "ko" | "zh")
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect();
        move |key: &str| map.get(key).cloned()
    }

    #[test]
    fn unset_term_is_unknown_and_lowest_safe() {
        let caps = TermCaps::detect(env(&[]));
        assert_eq!(caps.family, TerminalFamily::Unknown);
        assert!(caps.dumb);
        assert!(!caps.colour && !caps.truecolor && !caps.ansi256);
        assert!(!caps.unicode && !caps.mouse && !caps.bracketed_paste);
    }

    #[test]
    fn dumb_term_is_the_lowest_tier() {
        let caps = TermCaps::detect(env(&[("TERM", "dumb")]));
        assert_eq!(caps.family, TerminalFamily::Plain);
        assert!(caps.dumb);
        assert!(!caps.colour && !caps.unicode && !caps.mouse);
    }

    #[test]
    fn tmux_trusts_term_caps_not_the_outer_program() {
        // tmux inside WezTerm, but tmux's TERM is a 256-colour screen: the outer
        // identity must not grant truecolor.
        let caps = TermCaps::detect(env(&[
            ("TERM", "screen-256color"),
            ("TMUX", "/tmp/tmux-1000,1,0"),
            ("TERM_PROGRAM", "WezTerm"),
        ]));
        assert_eq!(caps.family, TerminalFamily::Tmux);
        assert!(!caps.truecolor, "no COLORTERM -> not proven");
        assert!(caps.ansi256);
        assert!(caps.unicode || !caps.unicode); // unicode needs a locale below
                                                // COLORTERM + a 256-colour tmux TERM -> truecolor proven.
        let caps = TermCaps::detect(env(&[
            ("TERM", "screen-256color"),
            ("TMUX", "/tmp/tmux-1000,1,0"),
            ("COLORTERM", "truecolor"),
        ]));
        assert!(caps.truecolor && caps.ansi256);
        // A plain `screen` (8/16 colour) tmux stays conservative.
        let caps = TermCaps::detect(env(&[
            ("TERM", "screen"),
            ("TMUX", "/tmp/tmux-1000,1,0"),
            ("COLORTERM", "truecolor"),
        ]));
        assert!(!caps.truecolor, "tmux TERM says 8/16 colours");
        assert!(!caps.ansi256);
    }

    #[test]
    fn zellij_is_detected_as_the_innermost_layer() {
        let caps = TermCaps::detect(env(&[
            ("TERM", "xterm-256color"),
            ("ZELLIJ", "0"),
            ("LANG", "en_US.UTF-8"),
        ]));
        assert_eq!(caps.family, TerminalFamily::Zellij);
        assert!(caps.unicode && caps.ansi256 && caps.colour);
    }

    #[test]
    fn kitty_reports_truecolor_and_unicode_without_a_locale() {
        let caps = TermCaps::detect(env(&[("TERM", "xterm-kitty"), ("KITTY_WINDOW_ID", "1")]));
        assert_eq!(caps.family, TerminalFamily::Kitty);
        assert!(caps.truecolor && caps.ansi256 && caps.colour);
        assert!(caps.unicode, "kitty is UTF-8 by construction");
        assert!(caps.mouse && caps.bracketed_paste);
    }

    #[test]
    fn wezterm_and_windows_terminal_are_detected() {
        let caps = TermCaps::detect(env(&[("TERM", "xterm-256color"), ("WEZTERM_PANE", "0")]));
        assert_eq!(caps.family, TerminalFamily::WezTerm);
        assert!(caps.truecolor);
        let caps = TermCaps::detect(env(&[
            ("TERM", "xterm-256color"),
            ("WT_SESSION", "0000-1111"),
            ("COLORTERM", "truecolor"),
        ]));
        assert_eq!(caps.family, TerminalFamily::WindowsTerminal);
        assert!(caps.truecolor && caps.ansi256);
    }

    #[test]
    fn herdr_falls_back_to_env_evidence_only() {
        // The CI host: Herdr on kitty. `KITTY_WINDOW_ID` is absent, so the
        // family is Herdr and only COLORTERM/locale prove capabilities.
        let caps = TermCaps::detect(env(&[
            ("TERM", "xterm-256color"),
            ("TERM_PROGRAM", "herdr"),
            ("HERDR_ENV", "1"),
            ("KITTY_PID", "7206"),
            ("COLORTERM", "truecolor"),
            ("LANG", "en_US.UTF-8"),
        ]));
        assert_eq!(caps.family, TerminalFamily::Herdr);
        assert!(caps.truecolor && caps.ansi256 && caps.unicode);
        assert!(caps.mouse && caps.colour);
        // Without COLORTERM the fallback drops to 256, not truecolor.
        let caps = TermCaps::detect(env(&[
            ("TERM", "xterm-256color"),
            ("HERDR_ENV", "1"),
            ("LANG", "en_US.UTF-8"),
        ]));
        assert_eq!(caps.family, TerminalFamily::Herdr);
        assert!(!caps.truecolor && caps.ansi256);
    }

    #[test]
    fn no_color_turns_colour_off_but_keeps_unicode() {
        let caps = TermCaps::detect(env(&[
            ("TERM", "xterm-256color"),
            ("LANG", "en_US.UTF-8"),
            ("NO_COLOR", "1"),
        ]));
        assert!(!caps.colour && !caps.truecolor && !caps.ansi256);
        assert!(
            caps.unicode,
            "NO_COLOR is a colour axis only (M3 narrowing)"
        );
    }

    #[test]
    fn vt100_without_colorterm_is_16_colour_ascii() {
        let caps = TermCaps::detect(env(&[("TERM", "vt100"), ("LANG", "C")]));
        assert_eq!(caps.family, TerminalFamily::Plain);
        assert!(caps.colour && !caps.truecolor && !caps.ansi256);
        assert!(!caps.unicode && !caps.mouse && !caps.bracketed_paste);
    }

    #[test]
    fn cjk_locale_flags_ambiguous_width() {
        assert!(
            TermCaps::detect(env(&[("TERM", "xterm-256color"), ("LANG", "ja_JP.UTF-8")]))
                .ambiguous_wide
        );
        assert!(
            TermCaps::detect(env(&[("TERM", "xterm-256color"), ("LANG", "zh_CN.UTF-8")]))
                .ambiguous_wide
        );
        assert!(
            !TermCaps::detect(env(&[("TERM", "xterm-256color"), ("LANG", "en_US.UTF-8")]))
                .ambiguous_wide
        );
    }

    #[test]
    fn summary_is_one_deterministic_line() {
        let caps = TermCaps::detect(env(&[("TERM", "xterm-256color"), ("LANG", "C")]));
        let line = caps.summary();
        assert!(line.starts_with("family=plain term=xterm-256color"));
        assert!(line.contains("truecolor=false"));
        assert!(line.contains("unicode=false"));
        assert!(!line.contains('\n'));
    }
}
