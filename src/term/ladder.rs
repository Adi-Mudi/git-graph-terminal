//! Capability ladder (M3-W8, T2) — the ordered degradation the renderer draws
//! from.
//!
//! [`super::TermCaps`] says what the terminal *proved*; this module turns that
//! into the two things a drawing routine actually needs:
//!
//! * **colour** — [`ColourTier`], `truecolor → 256 → 16 → mono`. A frozen palette
//!   entry (`u8,u8,u8`) is quantised down to the best colour the tier can name.
//! * **glyphs** — the Unicode table degrades to the ASCII table, arrows
//!   (`↑↓←→`) to `^v<>`, and frames (`┌─┐│`) to `+-|`.
//!
//! It also carries the **wide-glyph alignment guard**: two cells per lane is the
//! whole geometry contract (D8 §3.1), so a glyph that renders two cells wide
//! would shift every row after it. [`guarded_glyphs`] falls back to ASCII when a
//! CJK locale makes the East-Asian *ambiguous* glyphs double-width, and
//! [`display_width`]/[`truncate_cells`]/[`pad_cells`] give the text side the same
//! guarantee (a branch label with CJK or emoji must not break the column maths).
//!
//! Only [`crate::render`] consumes this module. Nothing here reads the
//! environment: the renderer must stay a pure function of the ladder it is given.

use ratatui::style::Color;
use ratatui::symbols::border;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::render::glyphs::{GlyphSet, GlyphTier, ASCII};
use crate::term::TerminalFamily;

/// How many colours the terminal can name, from most to least capable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColourTier {
    /// 24-bit RGB (`COLORTERM=truecolor`/`24bit`, kitty, WezTerm).
    TrueColor,
    /// The 256-entry xterm palette (`TERM=…256color`).
    Ansi256,
    /// The 16 ANSI colours — the conservative fallback for an unproven terminal.
    Ansi16,
    /// No colour at all (`TERM=dumb`/unset or `NO_COLOR`).
    Mono,
}

impl ColourTier {
    /// Short label for `--caps` and the status bar.
    pub const fn label(self) -> &'static str {
        match self {
            ColourTier::TrueColor => "truecolor",
            ColourTier::Ansi256 => "256",
            ColourTier::Ansi16 => "16",
            ColourTier::Mono => "mono",
        }
    }

    /// The ladder's colour rung for a probe result.
    pub fn from_caps(caps: &super::TermCaps) -> Self {
        if !caps.colour {
            ColourTier::Mono
        } else if caps.truecolor {
            ColourTier::TrueColor
        } else if caps.ansi256 {
            ColourTier::Ansi256
        } else {
            ColourTier::Ansi16
        }
    }

    /// Quantise one RGB palette entry to this tier; `None` means "draw plain".
    pub fn quantise(self, r: u8, g: u8, b: u8) -> Option<Color> {
        match self {
            ColourTier::Mono => None,
            ColourTier::TrueColor => Some(Color::Rgb(r, g, b)),
            ColourTier::Ansi256 => Some(Color::Indexed(rgb_to_ansi256(r, g, b))),
            ColourTier::Ansi16 => Some(ansi16_colour(rgb_to_ansi16(r, g, b))),
        }
    }
}

/// The 16 ANSI colours as RGB, in palette order (normal, then bright).
const ANSI16_RGB: [(u8, u8, u8); 16] = [
    (0, 0, 0),
    (205, 49, 49),
    (13, 188, 121),
    (229, 229, 16),
    (36, 114, 200),
    (188, 63, 188),
    (17, 168, 205),
    (229, 229, 229),
    (102, 102, 102),
    (241, 76, 76),
    (35, 209, 139),
    (245, 245, 67),
    (59, 142, 234),
    (214, 112, 214),
    (41, 184, 219),
    (255, 255, 255),
];

/// The `ratatui` colour for one of the 16 ANSI indices.
pub const fn ansi16_colour(index: u8) -> Color {
    match index % 16 {
        0 => Color::Black,
        1 => Color::Red,
        2 => Color::Green,
        3 => Color::Yellow,
        4 => Color::Blue,
        5 => Color::Magenta,
        6 => Color::Cyan,
        7 => Color::Gray,
        8 => Color::DarkGray,
        9 => Color::LightRed,
        10 => Color::LightGreen,
        11 => Color::LightYellow,
        12 => Color::LightBlue,
        13 => Color::LightMagenta,
        14 => Color::LightCyan,
        _ => Color::White,
    }
}

/// Nearest 16-colour index for an RGB triple (Euclidean distance).
pub fn rgb_to_ansi16(r: u8, g: u8, b: u8) -> u8 {
    let mut best = 0u8;
    let mut best_distance = u32::MAX;
    for (index, (pr, pg, pb)) in ANSI16_RGB.iter().enumerate() {
        let distance = colour_distance(r, g, b, *pr, *pg, *pb);
        if distance < best_distance {
            best_distance = distance;
            best = index as u8;
        }
    }
    best
}

/// Nearest 256-colour index: the closer of the 6×6×6 cube and the 24-step grey
/// ramp, exactly as the xterm palette is laid out.
pub fn rgb_to_ansi256(r: u8, g: u8, b: u8) -> u8 {
    let to_cube = |value: u8| -> u8 { ((value as u16 * 5 + 127) / 255) as u8 };
    let (cr, cg, cb) = (to_cube(r), to_cube(g), to_cube(b));
    let cube = 16 + 36 * cr + 6 * cg + cb;
    let cube_rgb = (cube_component(cr), cube_component(cg), cube_component(cb));

    let grey_average = (r as u16 + g as u16 + b as u16) / 3;
    let grey_index = if grey_average < 8 {
        0
    } else if grey_average > 238 {
        23
    } else {
        ((grey_average - 8) / 10) as u8
    };
    let grey = 232 + grey_index;
    let grey_level = 8 + 10 * grey_index;

    let cube_distance = colour_distance(r, g, b, cube_rgb.0, cube_rgb.1, cube_rgb.2);
    let grey_distance = colour_distance(r, g, b, grey_level, grey_level, grey_level);
    if grey_distance < cube_distance {
        grey
    } else {
        cube
    }
}

fn cube_component(index: u8) -> u8 {
    if index == 0 {
        0
    } else {
        55 + 40 * index
    }
}

fn colour_distance(r: u8, g: u8, b: u8, pr: u8, pg: u8, pb: u8) -> u32 {
    let d = |a: u8, b: u8| -> u32 {
        let delta = a as i32 - b as i32;
        (delta * delta) as u32
    };
    d(r, pr) + d(g, pg) + d(b, pb)
}

/// A direction an arrow glyph can point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    /// Upward.
    Up,
    /// Downward.
    Down,
    /// Leftward.
    Left,
    /// Rightward.
    Right,
}

/// The arrow set: Unicode arrows, or the seven-bit `^ v < >` fallback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArrowSet {
    /// `↑ ↓ ← →`.
    Unicode,
    /// `^ v < >`.
    Ascii,
}

impl ArrowSet {
    /// The glyph for `dir`.
    pub const fn arrow(self, dir: Dir) -> char {
        match (self, dir) {
            (ArrowSet::Unicode, Dir::Up) => '↑',
            (ArrowSet::Unicode, Dir::Down) => '↓',
            (ArrowSet::Unicode, Dir::Left) => '←',
            (ArrowSet::Unicode, Dir::Right) => '→',
            (ArrowSet::Ascii, Dir::Up) => '^',
            (ArrowSet::Ascii, Dir::Down) => 'v',
            (ArrowSet::Ascii, Dir::Left) => '<',
            (ArrowSet::Ascii, Dir::Right) => '>',
        }
    }
}

/// The frame set: box-drawing, or the seven-bit `+ - |` fallback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BorderSet {
    /// `┌ ─ ┐ │`.
    Unicode,
    /// `+ - |`.
    Ascii,
}

/// Single-line ASCII frame, the fallback for `ratatui`'s default box-drawing set.
const ASCII_BORDER: border::Set<'static> = border::Set {
    top_left: "+",
    top_right: "+",
    bottom_left: "+",
    bottom_right: "+",
    vertical_left: "|",
    vertical_right: "|",
    horizontal_top: "-",
    horizontal_bottom: "-",
};

impl BorderSet {
    /// The `ratatui` border symbols for this set.
    pub const fn ratatui(self) -> border::Set<'static> {
        match self {
            BorderSet::Unicode => border::PLAIN,
            BorderSet::Ascii => ASCII_BORDER,
        }
    }
}

/// Everything the renderer needs to draw at the terminal's proven capability.
///
/// `Copy` and `const`-constructible, so it can ride inside
/// [`crate::render::ViewOptions`] (including its `TEXT_ONLY` default) without an
/// allocation or a lifetime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ladder {
    /// Innermost terminal family (status bar + report only).
    pub family: TerminalFamily,
    /// Glyph table rung (Unicode / Ascii / Dumb).
    pub tier: GlyphTier,
    /// Colour rung.
    pub colour: ColourTier,
    /// Arrow set rung.
    pub arrows: ArrowSet,
    /// Frame set rung.
    pub borders: BorderSet,
    /// SGR mouse reporting is safe.
    pub mouse: bool,
    /// Bracketed paste is safe (detected, not enabled — see `term` docs).
    pub bracketed_paste: bool,
    /// East-Asian ambiguous glyphs are double-width here.
    pub ambiguous_wide: bool,
}

impl Ladder {
    /// The top of the ladder — Unicode + truecolor + every feature on.
    ///
    /// Used by tests and by the P0 text-only fallback, which draws no graph and
    /// therefore has no capability to degrade.
    pub const fn unlimited() -> Self {
        Self {
            family: TerminalFamily::Plain,
            tier: GlyphTier::Unicode,
            colour: ColourTier::TrueColor,
            arrows: ArrowSet::Unicode,
            borders: BorderSet::Unicode,
            mouse: true,
            bracketed_paste: true,
            ambiguous_wide: false,
        }
    }

    /// The bottom of the ladder — ASCII glyphs, no colour, no mouse.
    pub const fn mono_ascii() -> Self {
        Self {
            family: TerminalFamily::Plain,
            tier: GlyphTier::Ascii,
            colour: ColourTier::Mono,
            arrows: ArrowSet::Ascii,
            borders: BorderSet::Ascii,
            mouse: false,
            bracketed_paste: false,
            ambiguous_wide: false,
        }
    }

    /// A ladder at `tier`: Unicode gets the full ladder, anything else the floor.
    ///
    /// The tier is preserved even when the rung collapses (ASCII and Dumb share
    /// the mono/ASCII arrows and frames but keep their own glyph table), so a
    /// caller can always ask which tier it actually got.
    pub const fn from_tier(tier: GlyphTier) -> Self {
        let mut ladder = match tier {
            GlyphTier::Unicode => Self::unlimited(),
            GlyphTier::Ascii | GlyphTier::Dumb => Self::mono_ascii(),
        };
        ladder.tier = tier;
        ladder
    }

    /// Same ladder at a different colour rung.
    pub const fn with_colour(mut self, colour: ColourTier) -> Self {
        self.colour = colour;
        self
    }

    /// Same ladder with the ambiguous-width flag overridden.
    ///
    /// Setting it also drops the arrow and frame rungs to ASCII: those are
    /// exactly the glyphs a CJK locale renders two cells wide, so a ladder that
    /// kept them would break the lane geometry even though its glyph *table* is
    /// guarded.
    pub const fn with_ambiguous_wide(mut self, wide: bool) -> Self {
        self.ambiguous_wide = wide;
        if wide {
            self.arrows = ArrowSet::Ascii;
            self.borders = BorderSet::Ascii;
        }
        self
    }

    /// Build the ladder for a probe result.
    ///
    /// The wide-glyph guard is applied here: a CJK locale makes the Unicode
    /// table's ambiguous glyphs double-width, so the ladder drops straight to the
    /// ASCII table rather than emit a graph whose lanes drift.
    pub fn from_caps(caps: &super::TermCaps) -> Self {
        let unicode = caps.unicode && !caps.ambiguous_wide;
        let tier = if caps.dumb {
            GlyphTier::Dumb
        } else if unicode {
            GlyphTier::Unicode
        } else {
            GlyphTier::Ascii
        };
        let mut ladder = Self::from_tier(tier)
            .with_colour(ColourTier::from_caps(caps))
            .with_ambiguous_wide(caps.ambiguous_wide);
        ladder.family = caps.family;
        ladder.mouse = caps.mouse;
        ladder.bracketed_paste = caps.bracketed_paste;
        ladder
    }

    /// The glyph table rung.
    pub const fn glyph_tier(&self) -> GlyphTier {
        self.tier
    }

    /// The glyph table to draw with, after the wide-glyph guard.
    pub fn glyphs(&self) -> &'static GlyphSet {
        guarded_glyphs(self.tier, self.ambiguous_wide)
    }

    /// Quantise palette entry `idx` for this ladder; `None` means "no colour".
    pub fn colour_of(&self, palette: &[(u8, u8, u8)], idx: u8) -> Option<Color> {
        if palette.is_empty() {
            return None;
        }
        let (r, g, b) = palette[(idx as usize) % palette.len()];
        self.colour.quantise(r, g, b)
    }

    /// The colour for the uncommitted-changes row (`None` on a mono ladder).
    pub fn uncommitted_colour(&self) -> Option<Color> {
        if self.colour == ColourTier::Mono {
            None
        } else {
            Some(Color::DarkGray)
        }
    }

    /// The arrow glyph for `dir`.
    pub const fn arrow(&self, dir: Dir) -> char {
        self.arrows.arrow(dir)
    }

    /// The `ratatui` border symbols for frames (dialogs, menus, CDV, pager).
    pub const fn border_set(&self) -> border::Set<'static> {
        self.borders.ratatui()
    }

    /// The "checked" marker (column toggles).
    pub const fn checked(&self) -> char {
        if matches!(self.tier, GlyphTier::Unicode) {
            '✓'
        } else {
            'x'
        }
    }

    /// The "selected row" pointer (dialog fields).
    pub const fn pointer(&self) -> char {
        if matches!(self.tier, GlyphTier::Unicode) {
            '▸'
        } else {
            '>'
        }
    }

    /// The text-cursor block (find widget).
    pub const fn caret(&self) -> char {
        if matches!(self.tier, GlyphTier::Unicode) {
            '█'
        } else {
            '_'
        }
    }

    /// The separator dot used in status-bar hints.
    pub const fn separator(&self) -> char {
        if matches!(self.tier, GlyphTier::Unicode) {
            '·'
        } else {
            '-'
        }
    }

    /// `family/colour/glyphs` summary for the status bar and the report.
    pub fn label(&self) -> String {
        let glyphs = match self.tier {
            GlyphTier::Unicode => "unicode",
            GlyphTier::Ascii => "ascii",
            GlyphTier::Dumb => "dumb",
        };
        let mouse = if self.mouse { "/mouse" } else { "" };
        format!(
            "{}/{}/{glyphs}{mouse}",
            self.family.label(),
            self.colour.label()
        )
    }
}

/// Glyphs whose East-Asian width is *ambiguous*: one cell in a Western locale,
/// two cells in a CJK locale. Every non-ASCII glyph the graph and frames use is
/// on this list.
const AMBIGUOUS_GLYPHS: &[char] = &[
    '│', '─', '┼', '╰', '╯', '╭', '╮', '└', '┘', '┌', '┐', '┴', '┬', '├', '┤', '●', '◯', '◎', '┄',
    '┆', '↑', '↓', '←', '→', '✓', '▸', '█', '·',
];

/// Does `ch` render two cells wide in a CJK locale?
pub fn is_ambiguous_width(ch: char) -> bool {
    AMBIGUOUS_GLYPHS.contains(&ch)
}

/// Display width of `text` in terminal cells (`unicode-width`).
pub fn display_width(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

/// Display width of one character in terminal cells.
pub fn char_width(ch: char) -> usize {
    UnicodeWidthChar::width(ch).unwrap_or(0)
}

/// `text` clipped to at most `cells` terminal cells (never splits a wide glyph).
pub fn truncate_cells(text: &str, cells: usize) -> String {
    let mut out = String::new();
    let mut used = 0usize;
    for ch in text.chars() {
        let width = char_width(ch);
        if used + width > cells {
            break;
        }
        out.push(ch);
        used += width;
    }
    out
}

/// `text` padded (or clipped) to exactly `cells` terminal cells.
pub fn pad_cells(text: &str, cells: usize) -> String {
    let mut out = truncate_cells(text, cells);
    let used = display_width(&out);
    for _ in used..cells {
        out.push(' ');
    }
    out
}

/// Every glyph in `set`, in a stable order (frames, then corners, then nodes).
pub fn glyph_chars(set: &GlyphSet) -> Vec<char> {
    let mut out = vec![
        set.vertical,
        set.horizontal,
        set.cross,
        set.commit,
        set.head,
        set.stash,
        set.uncommitted,
        set.dashed_vertical,
        set.tee_missing_down,
        set.tee_missing_up,
        set.tee_missing_left,
        set.tee_missing_right,
    ];
    out.extend(set.rounded.iter().copied());
    out.extend(set.sharp.iter().copied());
    out
}

/// True when every glyph in `set` occupies exactly one terminal cell.
pub fn glyphs_are_single_cell(set: &GlyphSet) -> bool {
    glyph_chars(set).into_iter().all(|ch| char_width(ch) <= 1)
}

/// Every glyph in `set` whose East-Asian width is ambiguous.
pub fn ambiguous_glyphs(set: &GlyphSet) -> Vec<char> {
    let mut out: Vec<char> = Vec::new();
    for ch in glyph_chars(set) {
        if is_ambiguous_width(ch) && !out.contains(&ch) {
            out.push(ch);
        }
    }
    out
}

/// Is `set` safe to draw on a host where the East-Asian *ambiguous* glyphs are
/// two cells wide? The ASCII table is (its glyphs are narrow in every locale),
/// the Unicode table is not.
pub fn glyph_set_is_locale_safe(set: &GlyphSet, ambiguous_wide: bool) -> bool {
    !ambiguous_wide || ambiguous_glyphs(set).is_empty()
}

/// The Unicode table, unless a wide glyph would break the two-cells-per-lane
/// geometry — in which case the ASCII table, whose glyphs are one cell wide in
/// every locale.
pub fn guarded_glyphs(tier: GlyphTier, ambiguous_wide: bool) -> &'static GlyphSet {
    let set = tier.glyphs();
    if glyphs_are_single_cell(set) && glyph_set_is_locale_safe(set, ambiguous_wide) {
        set
    } else {
        &ASCII
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::glyphs::UNICODE;
    use crate::term::TermCaps;
    use std::collections::HashMap;

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect();
        move |key: &str| map.get(key).cloned()
    }

    #[test]
    fn colour_ladder_descends_through_every_rung() {
        assert_eq!(
            ColourTier::TrueColor.quantise(0, 133, 217),
            Some(Color::Rgb(0, 133, 217))
        );
        assert_eq!(
            ColourTier::Ansi256.quantise(0, 133, 217),
            Some(Color::Indexed(rgb_to_ansi256(0, 133, 217)))
        );
        assert_eq!(
            ColourTier::Ansi16.quantise(0, 133, 217),
            Some(ansi16_colour(rgb_to_ansi16(0, 133, 217)))
        );
        assert_eq!(ColourTier::Mono.quantise(0, 133, 217), None);
    }

    #[test]
    fn ansi256_matches_the_xterm_palette() {
        assert_eq!(rgb_to_ansi256(0, 0, 0), 16);
        assert_eq!(rgb_to_ansi256(255, 0, 0), 196);
        assert_eq!(rgb_to_ansi256(255, 255, 255), 231);
        // A mid grey is closer to the grey ramp (232..255) than to the cube.
        assert_eq!(rgb_to_ansi256(128, 128, 128), 244);
    }

    #[test]
    fn ansi16_maps_primaries_to_named_colours() {
        assert_eq!(ansi16_colour(rgb_to_ansi16(0, 0, 0)), Color::Black);
        assert_eq!(ansi16_colour(rgb_to_ansi16(255, 0, 0)), Color::Red);
        assert_eq!(ansi16_colour(rgb_to_ansi16(0, 0, 255)), Color::Blue);
        assert_eq!(ansi16_colour(rgb_to_ansi16(255, 255, 255)), Color::White);
    }

    /// The plan's "every capability tier × glyph set" matrix: every colour rung
    /// crossed with every glyph rung must produce a drawable ladder whose glyphs
    /// are all one cell wide (the lane-math invariant).
    #[test]
    fn every_colour_tier_cross_every_glyph_tier_is_drawable() {
        let palette = [(0u8, 133, 217), (255, 255, 255), (13, 188, 121)];
        for tier in [GlyphTier::Unicode, GlyphTier::Ascii, GlyphTier::Dumb] {
            for colour in [
                ColourTier::TrueColor,
                ColourTier::Ansi256,
                ColourTier::Ansi16,
                ColourTier::Mono,
            ] {
                let ladder = Ladder::from_tier(tier).with_colour(colour);
                assert_eq!(ladder.glyph_tier(), tier);
                assert!(
                    glyphs_are_single_cell(ladder.glyphs()),
                    "{tier:?} x {colour:?} keeps one cell per glyph"
                );
                for (index, _) in palette.iter().enumerate() {
                    let expected = if colour == ColourTier::Mono {
                        None
                    } else {
                        Some(true)
                    };
                    assert_eq!(
                        ladder.colour_of(&palette, index as u8).is_some(),
                        expected.is_some(),
                        "{tier:?} x {colour:?} entry {index}"
                    );
                }
            }
        }
        // An empty palette never panics and never invents a colour.
        assert_eq!(Ladder::unlimited().colour_of(&[], 0), None);
    }

    #[test]
    fn arrows_and_frames_follow_the_glyph_rung() {
        let unicode = Ladder::unlimited();
        assert_eq!(unicode.arrow(Dir::Left), '←');
        assert_eq!(unicode.arrow(Dir::Right), '→');
        assert_eq!(unicode.arrow(Dir::Up), '↑');
        assert_eq!(unicode.arrow(Dir::Down), '↓');
        assert_eq!(unicode.border_set().top_left, "┌");
        assert_eq!(unicode.checked(), '✓');
        assert_eq!(unicode.pointer(), '▸');

        let ascii = Ladder::mono_ascii();
        assert_eq!(ascii.arrow(Dir::Left), '<');
        assert_eq!(ascii.arrow(Dir::Right), '>');
        assert_eq!(ascii.arrow(Dir::Up), '^');
        assert_eq!(ascii.arrow(Dir::Down), 'v');
        assert_eq!(ascii.border_set().top_left, "+");
        assert_eq!(ascii.border_set().vertical_left, "|");
        assert_eq!(ascii.checked(), 'x');
        assert_eq!(ascii.pointer(), '>');
    }

    #[test]
    fn wide_glyph_guard_falls_back_to_ascii() {
        assert!(glyphs_are_single_cell(&UNICODE));
        assert!(ambiguous_glyphs(&UNICODE).contains(&'┼'));
        assert!(ambiguous_glyphs(&ASCII).is_empty());
        assert!(glyph_set_is_locale_safe(&UNICODE, false));
        assert!(!glyph_set_is_locale_safe(&UNICODE, true));
        assert_eq!(guarded_glyphs(GlyphTier::Unicode, false).cross, '┼');
        assert_eq!(guarded_glyphs(GlyphTier::Unicode, true).cross, '+');
        // The guarded table is the real ASCII table, not a copy.
        assert_eq!(guarded_glyphs(GlyphTier::Unicode, true).head, 'o');
        // Dumb is already ASCII-width and must survive the guard unchanged.
        assert_eq!(guarded_glyphs(GlyphTier::Dumb, true).commit, 'o');
    }

    #[test]
    fn is_ambiguous_width_covers_the_graph_and_frame_glyphs() {
        for ch in [
            '│', '─', '┼', '╰', '●', '◯', '◎', '┄', '↑', '←', '✓', '▸', '█',
        ] {
            assert!(is_ambiguous_width(ch), "{ch} is East-Asian ambiguous");
        }
        assert!(!is_ambiguous_width('a'));
        assert!(!is_ambiguous_width('+'));
    }

    #[test]
    fn display_width_counts_cjk_as_two_cells() {
        assert_eq!(display_width("ab"), 2);
        assert_eq!(display_width("日本語"), 6);
        assert_eq!(display_width("a日"), 3);
        assert_eq!(char_width('日'), 2);
        assert_eq!(char_width('a'), 1);
    }

    #[test]
    fn truncate_and_pad_respect_display_cells() {
        assert_eq!(truncate_cells("日本語", 4), "日本");
        assert_eq!(truncate_cells("abc", 2), "ab");
        assert_eq!(truncate_cells("ab", 10), "ab");
        assert_eq!(pad_cells("日", 4), "日  ");
        assert_eq!(pad_cells("ab", 4), "ab  ");
        assert_eq!(pad_cells("abcdef", 3), "abc");
        // The `fits` rule the renderer uses, spelled with display cells.
        assert_eq!(display_width("日本"), 4);
        assert!(display_width("ab") <= 2);
        assert!(display_width("日本語") > 4);
    }

    #[test]
    fn from_caps_downgrades_a_cjk_locale_to_ascii() {
        let caps = TermCaps::detect(env(&[
            ("TERM", "xterm-256color"),
            ("LANG", "ja_JP.UTF-8"),
            ("COLORTERM", "truecolor"),
        ]));
        let ladder = Ladder::from_caps(&caps);
        assert!(caps.ambiguous_wide);
        assert_eq!(ladder.glyph_tier(), GlyphTier::Ascii);
        assert_eq!(ladder.arrows, ArrowSet::Ascii);
        assert_eq!(ladder.borders, BorderSet::Ascii);
        assert_eq!(ladder.colour, ColourTier::TrueColor, "colour is untouched");
    }

    #[test]
    fn from_caps_walks_the_full_ladder() {
        let truecolor = Ladder::from_caps(&TermCaps::detect(env(&[
            ("TERM", "xterm-256color"),
            ("COLORTERM", "truecolor"),
            ("LANG", "en_US.UTF-8"),
        ])));
        assert_eq!(truecolor.tier, GlyphTier::Unicode);
        assert_eq!(truecolor.colour, ColourTier::TrueColor);
        assert_eq!(truecolor.arrows, ArrowSet::Unicode);

        let ansi256 = Ladder::from_caps(&TermCaps::detect(env(&[
            ("TERM", "xterm-256color"),
            ("LANG", "en_US.UTF-8"),
        ])));
        assert_eq!(ansi256.colour, ColourTier::Ansi256);

        let ansi16 = Ladder::from_caps(&TermCaps::detect(env(&[("TERM", "vt100"), ("LANG", "C")])));
        assert_eq!(ansi16.colour, ColourTier::Ansi16);
        assert_eq!(ansi16.tier, GlyphTier::Ascii);

        let mono = Ladder::from_caps(&TermCaps::detect(env(&[("TERM", "dumb")])));
        assert_eq!(mono.colour, ColourTier::Mono);
        assert_eq!(mono.tier, GlyphTier::Dumb);
    }

    #[test]
    fn label_is_stable_and_deterministic() {
        assert_eq!(Ladder::unlimited().label(), "plain/truecolor/unicode/mouse");
        assert_eq!(Ladder::mono_ascii().label(), "plain/mono/ascii");
        let caps = TermCaps::detect(env(&[
            ("TERM", "screen-256color"),
            ("TMUX", "/tmp/tmux-1000,1,0"),
            ("LANG", "en_US.UTF-8"),
        ]));
        assert_eq!(Ladder::from_caps(&caps).label(), "tmux/256/unicode/mouse");
    }
}
