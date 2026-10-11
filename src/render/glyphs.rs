//! Glyph tables and the capability tier ladder (G2, minimal).
//!
//! Three swappable tables — **Unicode**, **ASCII**, **Dumb** — selected from the
//! environment by [`GlyphTier::detect`]. This is the G2 *minimal* ladder: M3
//! replaces it with the full capability detection module; the table shape and
//! the call sites stay.
//!
//! Ladder (first match wins):
//!
//! 1. `TERM` unset, empty or `dumb` → [`GlyphTier::Dumb`].
//! 2. `NO_COLOR` set (non-empty) → [`GlyphTier::Ascii`] (conservative, colourless
//!    environments; M3 narrows this to colour-only).
//! 3. locale (`LC_ALL` → `LC_CTYPE` → `LANG`) set but not UTF-8 **and** no modern
//!    terminal hint (`COLORTERM` / `TMUX`) → [`GlyphTier::Ascii`].
//! 4. otherwise → [`GlyphTier::Unicode`] (box-drawing + rounded arcs).
//!
//! `TMUX` and `COLORTERM` are *keep-Unicode* hints: they never downgrade on
//! their own (tmux and modern terminals pass box-drawing through).

/// Capability tier for glyph selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlyphTier {
    /// Full box-drawing set with rounded corners (`│ ─ ┼ ╰ ╮ ╭ ╯ ● ◯ ◎ ┄`).
    Unicode,
    /// Seven-bit-safe subset (`| - + * o @ :`).
    Ascii,
    /// `TERM=dumb` minimum: one symbol per vertex, `|`/`-` lines.
    Dumb,
}

/// Which corner a rounded/sharp arc connects (used as an index into
/// [`GlyphSet::rounded`] / [`GlyphSet::sharp`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Corner {
    /// Connects upward and rightward (`╰`).
    UpRight,
    /// Connects upward and leftward (`╯`).
    UpLeft,
    /// Connects downward and rightward (`╭`).
    DownRight,
    /// Connects downward and leftward (`╮`).
    DownLeft,
}

/// One glyph table. Fields are read by `render::cell`; `cross` is the D8 §3.2
/// crossing glyph and the `tee_*` fields cover three-way junctions.
#[derive(Debug, Clone, Copy)]
pub struct GlyphSet {
    /// Straight vertical line.
    pub vertical: char,
    /// Straight horizontal line.
    pub horizontal: char,
    /// Crossing of a vertical and a horizontal line (D8 §3.2).
    pub cross: char,
    /// Rounded corners, indexed by [`Corner`].
    pub rounded: [char; 4],
    /// Sharp (angular) corners, indexed by [`Corner`].
    pub sharp: [char; 4],
    /// Filled commit node.
    pub commit: char,
    /// Open circle — the checked-out commit (HEAD).
    pub head: char,
    /// Nested circle — a stash entry.
    pub stash: char,
    /// Dashed node — the uncommitted-changes row.
    pub uncommitted: char,
    /// Dashed vertical — the uncommitted-changes path.
    pub dashed_vertical: char,
    /// Tee missing its downward arm (`┴`).
    pub tee_missing_down: char,
    /// Tee missing its upward arm (`┬`).
    pub tee_missing_up: char,
    /// Tee missing its leftward arm (`├`).
    pub tee_missing_left: char,
    /// Tee missing its rightward arm (`┤`).
    pub tee_missing_right: char,
}

impl GlyphSet {
    /// Rounded corner glyph for `corner`.
    pub fn rounded_corner(&self, corner: Corner) -> char {
        self.rounded[corner as usize]
    }

    /// Sharp corner glyph for `corner`.
    pub fn sharp_corner(&self, corner: Corner) -> char {
        self.sharp[corner as usize]
    }
}

/// Unicode tier — box-drawing with rounded arcs.
pub const UNICODE: GlyphSet = GlyphSet {
    vertical: '│',
    horizontal: '─',
    cross: '┼',
    rounded: ['╰', '╯', '╭', '╮'],
    sharp: ['└', '┘', '┌', '┐'],
    commit: '●',
    head: '◯',
    stash: '◎',
    uncommitted: '┄',
    dashed_vertical: '┆',
    tee_missing_down: '┴',
    tee_missing_up: '┬',
    tee_missing_left: '├',
    tee_missing_right: '┤',
};

/// ASCII tier — no box-drawing, no rounded arcs.
pub const ASCII: GlyphSet = GlyphSet {
    vertical: '|',
    horizontal: '-',
    cross: '+',
    rounded: ['+'; 4],
    sharp: ['+'; 4],
    commit: '*',
    head: 'o',
    stash: '@',
    uncommitted: ':',
    dashed_vertical: ':',
    tee_missing_down: '+',
    tee_missing_up: '+',
    tee_missing_left: '+',
    tee_missing_right: '+',
};

/// Dumb tier (`TERM=dumb`) — one glyph per vertex, minimal lines.
pub const DUMB: GlyphSet = GlyphSet {
    vertical: '|',
    horizontal: '-',
    cross: '|',
    rounded: ['+'; 4],
    sharp: ['+'; 4],
    commit: 'o',
    head: 'o',
    stash: 'o',
    uncommitted: '.',
    dashed_vertical: '|',
    tee_missing_down: '+',
    tee_missing_up: '+',
    tee_missing_left: '+',
    tee_missing_right: '+',
};

impl GlyphTier {
    /// The table for this tier.
    pub fn glyphs(self) -> &'static GlyphSet {
        match self {
            GlyphTier::Unicode => &UNICODE,
            GlyphTier::Ascii => &ASCII,
            GlyphTier::Dumb => &DUMB,
        }
    }

    /// Pick a tier from an environment lookup (`env("TERM")` → `Some("xterm")`).
    pub fn detect<F>(env: F) -> Self
    where
        F: Fn(&str) -> Option<String>,
    {
        match env("TERM") {
            None => GlyphTier::Dumb,
            Some(term) if term.is_empty() || term == "dumb" => GlyphTier::Dumb,
            Some(_) => {
                if no_color(&env) {
                    return GlyphTier::Ascii;
                }
                let utf8 = locale(&env).is_some_and(|l| is_utf8_locale(&l));
                let modern = non_empty(&env, "COLORTERM") || non_empty(&env, "TMUX");
                if utf8 || modern {
                    GlyphTier::Unicode
                } else {
                    GlyphTier::Ascii
                }
            }
        }
    }

    /// Pick a tier from the process environment.
    pub fn detect_from_os() -> Self {
        Self::detect(|key| std::env::var(key).ok())
    }
}

/// Is colour output allowed? (`TERM=dumb`/unset and `NO_COLOR` disable it.)
pub fn colour_enabled<F>(env: F) -> bool
where
    F: Fn(&str) -> Option<String>,
{
    match env("TERM") {
        None => false,
        Some(term) if term.is_empty() || term == "dumb" => false,
        Some(_) => !no_color(&env),
    }
}

/// Is colour output allowed in this process environment?
pub fn colour_enabled_os() -> bool {
    colour_enabled(|key| std::env::var(key).ok())
}

fn non_empty<F>(env: &F, key: &str) -> bool
where
    F: Fn(&str) -> Option<String>,
{
    env(key).is_some_and(|value| !value.is_empty())
}

fn no_color<F>(env: &F) -> bool
where
    F: Fn(&str) -> Option<String>,
{
    non_empty(env, "NO_COLOR")
}

fn locale<F>(env: &F) -> Option<String>
where
    F: Fn(&str) -> Option<String>,
{
    ["LC_ALL", "LC_CTYPE", "LANG"]
        .into_iter()
        .find_map(|key| env(key).filter(|value| !value.is_empty()))
}

fn is_utf8_locale(locale: &str) -> bool {
    let locale = locale.to_ascii_lowercase();
    locale.contains("utf-8") || locale.contains("utf8")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |key: &str| map.get(key).cloned()
    }

    #[test]
    fn dumb_term_is_the_lowest_tier() {
        assert_eq!(GlyphTier::detect(env(&[])), GlyphTier::Dumb);
        assert_eq!(GlyphTier::detect(env(&[("TERM", "dumb")])), GlyphTier::Dumb);
        assert_eq!(GlyphTier::detect(env(&[("TERM", "")])), GlyphTier::Dumb);
    }

    #[test]
    fn utf8_locale_selects_unicode() {
        assert_eq!(
            GlyphTier::detect(env(&[("TERM", "xterm-256color"), ("LANG", "en_US.UTF-8")])),
            GlyphTier::Unicode
        );
    }

    #[test]
    fn modern_hint_selects_unicode_without_locale() {
        assert_eq!(
            GlyphTier::detect(env(&[
                ("TERM", "xterm-256color"),
                ("COLORTERM", "truecolor")
            ])),
            GlyphTier::Unicode
        );
        assert_eq!(
            GlyphTier::detect(env(&[("TERM", "screen"), ("TMUX", "/tmp/tmux-1000,1,0")])),
            GlyphTier::Unicode
        );
    }

    #[test]
    fn plain_locale_falls_back_to_ascii() {
        assert_eq!(
            GlyphTier::detect(env(&[("TERM", "vt100"), ("LANG", "C")])),
            GlyphTier::Ascii
        );
    }

    #[test]
    fn no_color_forces_ascii() {
        assert_eq!(
            GlyphTier::detect(env(&[
                ("TERM", "xterm-256color"),
                ("LANG", "en_US.UTF-8"),
                ("NO_COLOR", "1"),
            ])),
            GlyphTier::Ascii
        );
    }

    #[test]
    fn tables_are_distinct_and_complete() {
        assert_eq!(GlyphTier::Unicode.glyphs().cross, '┼');
        assert_eq!(GlyphTier::Ascii.glyphs().cross, '+');
        assert_eq!(
            GlyphTier::Unicode.glyphs().rounded_corner(Corner::UpRight),
            '╰'
        );
        assert_eq!(
            GlyphTier::Unicode.glyphs().sharp_corner(Corner::DownLeft),
            '┐'
        );
        assert_eq!(
            GlyphTier::Ascii.glyphs().rounded_corner(Corner::UpRight),
            '+'
        );
        assert_eq!(GlyphTier::Unicode.glyphs().head, '◯');
        assert_eq!(GlyphTier::Unicode.glyphs().stash, '◎');
        assert_eq!(GlyphTier::Unicode.glyphs().uncommitted, '┄');
    }

    #[test]
    fn colour_follows_no_color_and_dumb() {
        assert!(colour_enabled(env(&[("TERM", "xterm-256color")])));
        assert!(!colour_enabled(env(&[("TERM", "dumb")])));
        assert!(!colour_enabled(env(&[])));
        assert!(!colour_enabled(env(&[
            ("TERM", "xterm-256color"),
            ("NO_COLOR", "1"),
        ])));
    }
}
