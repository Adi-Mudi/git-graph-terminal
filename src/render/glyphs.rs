//! Glyph tables (G2 tables, detection moved to M3).
//!
//! Three swappable tables — **Unicode**, **ASCII**, **Dumb**. W8 moved the
//! *selection* out of this module: [`crate::term::TermCaps`] probes the
//! environment and [`crate::term::ladder::Ladder`] picks a [`GlyphTier`], so the
//! renderer never reads the environment itself. The table shape and the call
//! sites are unchanged from G2.
//!
//! The tables stay pure data: every glyph is one terminal cell wide in a Western
//! locale (see `term::ladder::guarded_glyphs` for the CJK ambiguous-width guard).

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
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
