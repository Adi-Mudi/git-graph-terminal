//! A small backtracking regex engine for the find widget.
//!
//! The reference's find widget compiles the pattern with JavaScript's `RegExp`
//! (`web/findWidget.ts:222`). The terminal port deliberately adds **no new
//! dependency** to reproduce that, so this module is a self-contained engine
//! over `Vec<char>`.
//!
//! ## Supported syntax
//!
//! * literals, `.`, `^`, `$`
//! * character classes: `[abc]`, `[^abc]`, ranges `[a-z]`, escapes inside
//! * escapes: `\d \D \w \W \s \S \n \r \t \f \v \0`, `\b` / `\B` word boundaries,
//!   and `\.`-style metacharacter escaping
//! * groups `(...)` and `(?:...)`, alternation `|`
//! * quantifiers `*`, `+`, `?`, `{n}`, `{n,}`, `{n,m}` and their lazy forms
//!   (`*?`, `+?`, `??`, `{n,m}?`)
//! * the `i` (case-insensitive) flag
//!
//! ## Deliberate limitations
//!
//! * **No back-references and no look-around.** `\1` and `(?=…)` are rejected at
//!   compile time with a clear message rather than silently mis-matching. The
//!   find widget's scope (`web/findWidget.ts:224-240`) is short single-line text
//!   — authors, hashes, subjects, ref names and dates — where they are not
//!   useful.
//! * **No named groups / sticky / multiline flags.** The reference always
//!   compiles with `'u'` plus `'i'` unless match-case is on
//!   (`findWidget.ts:222`), and never uses `m`, so `^`/`$` anchor to the whole
//!   string, which is what the reference gets for single-line inputs.
//! * **Case folding is per-character `to_lowercase`.** Exact for ASCII and for
//!   the common Unicode cases; it does not implement full case folding (e.g.
//!   `ß` ↔ `ss`), which JavaScript's `i` flag does not do either.
//! * **Matching is leftmost-first (backtracking), like JavaScript** — not
//!   leftmost-longest like POSIX. `find_all` returns non-overlapping matches and
//!   advances one character past a zero-length match, mirroring `String.matchAll`.

use std::fmt;

/// A pattern that failed to compile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegexError {
    /// The reason, phrased like the messages the reference surfaces
    /// (`web/findWidget.ts:339`).
    pub message: String,
    /// Character offset in the pattern where the problem was found.
    pub at: usize,
}

impl fmt::Display for RegexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for RegexError {}

fn err(message: impl Into<String>, at: usize) -> RegexError {
    RegexError {
        message: message.into(),
        at,
    }
}

/* ------------------------------------------------------------------ */
/* AST                                                                 */
/* ------------------------------------------------------------------ */

/// One item inside a character class.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ClassItem {
    Char(char),
    Range(char, char),
    /// `\d`, `\w`, `\s` and their negations.
    Perl(char),
}

/// A parsed node.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Node {
    /// Matches the empty string.
    Empty,
    Char(char),
    /// `.`
    Any,
    Class {
        negated: bool,
        items: Vec<ClassItem>,
    },
    /// `^` — the start of the input.
    Start,
    /// `$` — the end of the input.
    End,
    /// `\b`
    WordBoundary,
    /// `\B`
    NotWordBoundary,
    Concat(Vec<Node>),
    Alt(Vec<Node>),
    Repeat {
        node: Box<Node>,
        min: u32,
        max: Option<u32>,
        greedy: bool,
    },
}

/* ------------------------------------------------------------------ */
/* Parser                                                              */
/* ------------------------------------------------------------------ */

struct Parser<'a> {
    pattern: &'a [char],
    pos: usize,
    case_insensitive: bool,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<char> {
        self.pattern.get(self.pos).copied()
    }

    fn next(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += 1;
        Some(c)
    }

    fn eat(&mut self, want: char) -> bool {
        if self.peek() == Some(want) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    /// `alt := concat ('|' concat)*`
    fn parse_alt(&mut self) -> Result<Node, RegexError> {
        let mut branches = vec![self.parse_concat()?];
        while self.eat('|') {
            branches.push(self.parse_concat()?);
        }
        Ok(if branches.len() == 1 {
            branches.pop().unwrap_or(Node::Empty)
        } else {
            Node::Alt(branches)
        })
    }

    /// `concat := repeat*`
    fn parse_concat(&mut self) -> Result<Node, RegexError> {
        let mut items = Vec::new();
        while let Some(c) = self.peek() {
            if c == '|' || c == ')' {
                break;
            }
            items.push(self.parse_repeat()?);
        }
        Ok(match items.len() {
            0 => Node::Empty,
            1 => items.pop().unwrap_or(Node::Empty),
            _ => Node::Concat(items),
        })
    }

    /// `repeat := atom quantifier?`
    fn parse_repeat(&mut self) -> Result<Node, RegexError> {
        let atom = self.parse_atom()?;
        let Some(c) = self.peek() else {
            return Ok(atom);
        };
        let (min, max) = match c {
            '*' => {
                self.pos += 1;
                (0, None)
            }
            '+' => {
                self.pos += 1;
                (1, None)
            }
            '?' => {
                self.pos += 1;
                (0, Some(1))
            }
            '{' => match self.try_parse_braces()? {
                Some(bounds) => bounds,
                // An unparsable `{` is a literal in JavaScript.
                None => return Ok(atom),
            },
            _ => return Ok(atom),
        };
        // A trailing `?` makes the quantifier lazy.
        let greedy = !self.eat('?');
        if let Some(max) = max {
            if max < min {
                return Err(err("numbers out of order in {} quantifier", self.pos));
            }
        }
        Ok(Node::Repeat {
            node: Box::new(atom),
            min,
            max,
            greedy,
        })
    }

    /// Parse `{n}`, `{n,}` or `{n,m}` starting at a `{`. `Ok(None)` means "not a
    /// quantifier", leaving the cursor untouched so `{` stays a literal.
    fn try_parse_braces(&mut self) -> Result<Option<(u32, Option<u32>)>, RegexError> {
        let start = self.pos;
        self.pos += 1; // the `{`
        let Some(min) = self.parse_number() else {
            self.pos = start;
            return Ok(None);
        };
        let bounds = if self.eat(',') {
            match self.parse_number() {
                Some(max) => (min, Some(max)),
                None => (min, None),
            }
        } else {
            (min, Some(min))
        };
        if !self.eat('}') {
            self.pos = start;
            return Ok(None);
        }
        Ok(Some(bounds))
    }

    fn parse_number(&mut self) -> Option<u32> {
        let start = self.pos;
        let mut value: u64 = 0;
        while let Some(c) = self.peek() {
            if let Some(digit) = c.to_digit(10) {
                value = value * 10 + u64::from(digit);
                if value > u64::from(u32::MAX) {
                    return None;
                }
                self.pos += 1;
            } else {
                break;
            }
        }
        if self.pos == start {
            None
        } else {
            Some(value as u32)
        }
    }

    /// `atom := group | class | escape | '.' | '^' | '$' | literal`
    fn parse_atom(&mut self) -> Result<Node, RegexError> {
        let at = self.pos;
        let Some(c) = self.next() else {
            return Ok(Node::Empty);
        };
        match c {
            '(' => {
                // `(?:` is a plain group; `(?=`/`(?!`/`(?<` are unsupported.
                if self.peek() == Some('?') {
                    match self.pattern.get(self.pos + 1) {
                        Some(':') => {
                            self.pos += 2;
                        }
                        Some(mark @ ('=' | '!' | '<')) => {
                            return Err(err(
                                format!(
                                    "look-around and named groups are not supported ('(?{mark}')"
                                ),
                                at,
                            ))
                        }
                        _ => return Err(err("invalid group", at)),
                    }
                }
                let inner = self.parse_alt()?;
                if !self.eat(')') {
                    return Err(err("unclosed group '(' — missing ')'", at));
                }
                Ok(inner)
            }
            '[' => self.parse_class(at),
            '\\' => self.parse_escape(at),
            '.' => Ok(Node::Any),
            '^' => Ok(Node::Start),
            '$' => Ok(Node::End),
            ')' => Err(err("unmatched ')'", at)),
            '*' | '+' | '?' => Err(err(format!("nothing to repeat before '{c}'"), at)),
            other => Ok(Node::Char(other)),
        }
    }

    /// Parse a `[...]` class; the opening `[` is already consumed.
    fn parse_class(&mut self, at: usize) -> Result<Node, RegexError> {
        let negated = self.eat('^');
        let mut items = Vec::new();
        let mut first = true;
        loop {
            let Some(c) = self.peek() else {
                return Err(err("unclosed character class '[' — missing ']'", at));
            };
            // A `]` in the first position is a literal, as in JavaScript.
            if c == ']' && !first {
                self.pos += 1;
                break;
            }
            first = false;
            let item = if c == '\\' {
                self.pos += 1;
                let esc_at = self.pos;
                let Some(escaped) = self.next() else {
                    return Err(err("trailing backslash", esc_at));
                };
                if is_perl_class(escaped) {
                    items.push(ClassItem::Perl(escaped));
                    continue;
                }
                ClassItem::Char(class_escape(escaped))
            } else {
                self.pos += 1;
                ClassItem::Char(c)
            };

            // `a-z`, but not `-]` or a trailing `-`.
            if self.peek() == Some('-') && self.pattern.get(self.pos + 1).copied() != Some(']') {
                if let ClassItem::Char(lo) = item {
                    self.pos += 1; // the `-`
                    let Some(hi_c) = self.next() else {
                        return Err(err("unclosed character class '[' — missing ']'", at));
                    };
                    let hi = if hi_c == '\\' {
                        match self.next() {
                            Some(escaped) => class_escape(escaped),
                            None => return Err(err("trailing backslash", self.pos)),
                        }
                    } else {
                        hi_c
                    };
                    if hi < lo {
                        return Err(err("range out of order in character class", at));
                    }
                    items.push(ClassItem::Range(lo, hi));
                    continue;
                }
            }
            items.push(item);
        }
        Ok(Node::Class { negated, items })
    }

    /// Parse an escape outside a class; the `\` is already consumed.
    fn parse_escape(&mut self, at: usize) -> Result<Node, RegexError> {
        let Some(c) = self.next() else {
            return Err(err("trailing backslash", at));
        };
        match c {
            'd' | 'D' | 'w' | 'W' | 's' | 'S' => Ok(Node::Class {
                negated: c.is_ascii_uppercase(),
                items: vec![ClassItem::Perl(c.to_ascii_lowercase())],
            }),
            'b' => Ok(Node::WordBoundary),
            'B' => Ok(Node::NotWordBoundary),
            '1'..='9' => Err(err(
                format!("back-references are not supported ('\\{c}')"),
                at,
            )),
            other => Ok(Node::Char(class_escape(other))),
        }
    }
}

/// Does `c` name a Perl character class (`\d`, `\w`, `\s`)?
fn is_perl_class(c: char) -> bool {
    matches!(c, 'd' | 'D' | 'w' | 'W' | 's' | 'S')
}

/// The literal a class escape stands for (`\n` → a newline, `\.` → `.`).
fn class_escape(c: char) -> char {
    match c {
        'n' => '\n',
        'r' => '\r',
        't' => '\t',
        'f' => '\u{c}',
        'v' => '\u{b}',
        '0' => '\0',
        other => other,
    }
}

/* ------------------------------------------------------------------ */
/* Matcher                                                             */
/* ------------------------------------------------------------------ */

/// A compiled pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Regex {
    root: Node,
    case_insensitive: bool,
    /// The original text, kept for `Display` and diagnostics.
    source: String,
}

impl Regex {
    /// Compile `pattern`, treating it literally if it is not valid regex syntax.
    ///
    /// `case_insensitive` mirrors the reference's `i` flag
    /// (`web/findWidget.ts:222`).
    pub fn new_with_flags(pattern: &str, case_insensitive: bool) -> Result<Self, RegexError> {
        let chars: Vec<char> = pattern.chars().collect();
        let mut parser = Parser {
            pattern: &chars,
            pos: 0,
            case_insensitive,
        };
        let root = parser.parse_alt()?;
        if parser.pos < chars.len() {
            // The only way to stop early is an unmatched `)`.
            return Err(err("unmatched ')'", parser.pos));
        }
        Ok(Regex {
            root,
            case_insensitive,
            source: pattern.to_string(),
        })
    }

    /// The pattern text.
    pub fn as_str(&self) -> &str {
        &self.source
    }

    /// Does the pattern match anywhere in `text`?
    pub fn is_match(&self, text: &[char]) -> bool {
        self.find_at(text, 0).is_some()
    }

    /// Does a match start exactly at character offset `start`?
    ///
    /// The reference's `hash.search(pattern) === 0` test
    /// (`web/findWidget.ts:226`).
    pub fn match_at(&self, text: &[char], start: usize) -> Option<usize> {
        if start > text.len() {
            return None;
        }
        self.match_end(text, start)
    }

    /// The leftmost match in `text`, as `(start, end)` character offsets.
    pub fn find_at(&self, text: &[char], from: usize) -> Option<(usize, usize)> {
        for start in from..=text.len() {
            if let Some(end) = self.match_end(text, start) {
                return Some((start, end));
            }
        }
        None
    }

    /// Every non-overlapping match, leftmost-first.
    ///
    /// Mirrors `String.matchAll`: after a zero-length match the scan advances one
    /// character so it cannot loop forever.
    pub fn find_all(&self, text: &[char]) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        let mut from = 0;
        while from <= text.len() {
            let Some((start, end)) = self.find_at(text, from) else {
                break;
            };
            out.push((start, end));
            from = if end > start { end } else { start + 1 };
        }
        out
    }

    /// Can the pattern match zero characters? (`web/findWidget.ts:299-302`.)
    pub fn matches_empty(&self) -> bool {
        nullable(&self.root)
    }

    /// Try to match `root` starting at `start`; `Some(end)` on success.
    fn match_end(&self, text: &[char], start: usize) -> Option<usize> {
        let mut result = None;
        self.match_node(&self.root, text, start, &mut |end| {
            result = Some(end);
            true // stop at the first (leftmost-first, greedy) success
        });
        result
    }

    /// Continuation-passing backtracking matcher.
    ///
    /// `k` is called with the position after this node matched; returning `true`
    /// from `k` means "this is the answer", which unwinds the search. Returning
    /// `false` makes the node try its next alternative — that is what gives
    /// greedy/lazy quantifiers and alternation their backtracking order.
    fn match_node(
        &self,
        node: &Node,
        text: &[char],
        pos: usize,
        k: &mut dyn FnMut(usize) -> bool,
    ) -> bool {
        match node {
            Node::Empty => k(pos),
            Node::Char(want) => {
                if pos < text.len() && self.chars_eq(text[pos], *want) {
                    k(pos + 1)
                } else {
                    false
                }
            }
            Node::Any => {
                // `.` does not match a line terminator in JavaScript.
                if pos < text.len() && !matches!(text[pos], '\n' | '\r' | '\u{2028}' | '\u{2029}') {
                    k(pos + 1)
                } else {
                    false
                }
            }
            Node::Class { negated, items } => {
                if pos >= text.len() {
                    return false;
                }
                let hit = items.iter().any(|item| self.class_matches(item, text[pos]));
                if hit != *negated {
                    k(pos + 1)
                } else {
                    false
                }
            }
            Node::Start => {
                if pos == 0 {
                    k(pos)
                } else {
                    false
                }
            }
            Node::End => {
                if pos == text.len() {
                    k(pos)
                } else {
                    false
                }
            }
            Node::WordBoundary | Node::NotWordBoundary => {
                let before = pos > 0 && is_word(text[pos - 1]);
                let after = pos < text.len() && is_word(text[pos]);
                let boundary = before != after;
                let want = matches!(node, Node::WordBoundary);
                if boundary == want {
                    k(pos)
                } else {
                    false
                }
            }
            Node::Concat(items) => self.match_concat(items, 0, text, pos, k),
            Node::Alt(branches) => {
                for branch in branches {
                    if self.match_node(branch, text, pos, k) {
                        return true;
                    }
                }
                false
            }
            Node::Repeat {
                node,
                min,
                max,
                greedy,
            } => self.match_repeat(node, *min, *max, *greedy, text, pos, k),
        }
    }

    fn match_concat(
        &self,
        items: &[Node],
        index: usize,
        text: &[char],
        pos: usize,
        k: &mut dyn FnMut(usize) -> bool,
    ) -> bool {
        match items.split_first() {
            None => k(pos),
            Some((head, rest)) => {
                let mut rest_result = false;
                self.match_node(head, text, pos, &mut |next| {
                    rest_result = self.match_concat(rest, index + 1, text, next, k);
                    rest_result
                })
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn match_repeat(
        &self,
        node: &Node,
        min: u32,
        max: Option<u32>,
        greedy: bool,
        text: &[char],
        pos: usize,
        k: &mut dyn FnMut(usize) -> bool,
    ) -> bool {
        // One more iteration than the maximum is never allowed.
        if max == Some(0) {
            return if min == 0 { k(pos) } else { false };
        }
        let next_max = max.map(|m| m - 1);
        let try_more = |this: &Self, pos: usize, k: &mut dyn FnMut(usize) -> bool| {
            this.match_node(node, text, pos, &mut |next| {
                // A repeat whose body matched nothing cannot progress; stop so
                // `(a*)*` cannot loop forever.
                if next == pos {
                    false
                } else {
                    this.match_repeat(node, min.saturating_sub(1), next_max, greedy, text, next, k)
                }
            })
        };
        let try_stop = |k: &mut dyn FnMut(usize) -> bool| {
            if min == 0 {
                k(pos)
            } else {
                false
            }
        };
        if greedy {
            if try_more(self, pos, k) {
                return true;
            }
            try_stop(k)
        } else {
            if try_stop(k) {
                return true;
            }
            try_more(self, pos, k)
        }
    }

    /// Does one class item match `c`?
    fn class_matches(&self, item: &ClassItem, c: char) -> bool {
        let eq = |a: char, b: char| self.chars_eq(a, b);
        match item {
            ClassItem::Char(want) => eq(*want, c),
            ClassItem::Range(lo, hi) => {
                (*lo <= c && c <= *hi)
                    // Case-insensitive ranges must also catch the other case.
                    || (self.case_insensitive
                        && c.to_lowercase().any(|lower| *lo <= lower && lower <= *hi))
            }
            ClassItem::Perl(kind) => match kind {
                'd' => c.is_ascii_digit(),
                'w' => is_word(c),
                's' => is_space(c),
                _ => false,
            },
        }
    }

    fn chars_eq(&self, a: char, b: char) -> bool {
        if a == b {
            return true;
        }
        if !self.case_insensitive {
            return false;
        }
        a.to_lowercase().eq(b.to_lowercase())
    }
}

impl fmt::Display for Regex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "/{}/{}",
            self.source,
            if self.case_insensitive { "i" } else { "" }
        )
    }
}

/// `\w`: `[A-Za-z0-9_]`, exactly as JavaScript defines it.
fn is_word(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// `\s`: the JavaScript whitespace set.
fn is_space(c: char) -> bool {
    matches!(
        c,
        ' ' | '\t'
            | '\n'
            | '\r'
            | '\u{b}'
            | '\u{c}'
            | '\u{a0}'
            | '\u{feff}'
            | '\u{1680}'
            | '\u{2000}'
            ..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}'
    )
}

/// Can this node match the empty string? Used for the zero-length rejection.
fn nullable(node: &Node) -> bool {
    match node {
        Node::Empty | Node::Start | Node::End | Node::WordBoundary | Node::NotWordBoundary => true,
        Node::Char(_) | Node::Any | Node::Class { .. } => false,
        Node::Concat(items) => items.iter().all(nullable),
        Node::Alt(branches) => branches.iter().any(nullable),
        Node::Repeat { node, min, .. } => *min == 0 || nullable(node),
    }
}

/// Find every occurrence of `needle` in `haystack`.
///
/// The literal path of [`super::Pattern`]: JavaScript escapes the metacharacters
/// and builds a regex (`web/findWidget.ts:222`); a substring scan is equivalent
/// and cannot fail to compile. `fold_case` implements the `i` flag.
///
/// Returns **character** ranges, and finds overlapping occurrences (each start
/// position is tried), which is what a highlight pass wants.
pub fn literal_find_all(
    haystack: &[char],
    needle: &[char],
    fold_case: bool,
) -> Vec<(usize, usize)> {
    if needle.is_empty() || needle.len() > haystack.len() {
        return Vec::new();
    }
    let eq = |a: char, b: char| a == b || (fold_case && a.to_lowercase().eq(b.to_lowercase()));
    (0..=haystack.len() - needle.len())
        .filter(|start| {
            needle
                .iter()
                .enumerate()
                .all(|(offset, want)| eq(haystack[start + offset], *want))
        })
        .map(|start| (start, start + needle.len()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ch(text: &str) -> Vec<char> {
        text.chars().collect()
    }

    fn compile(pattern: &str) -> Regex {
        Regex::new_with_flags(pattern, false).expect("compiles")
    }

    fn compile_i(pattern: &str) -> Regex {
        Regex::new_with_flags(pattern, true).expect("compiles")
    }

    fn matches(pattern: &str, text: &str) -> Vec<String> {
        compile(pattern)
            .find_all(&ch(text))
            .into_iter()
            .map(|(start, end)| text.chars().take(end).skip(start).collect())
            .collect()
    }

    #[test]
    fn literals_match() {
        assert_eq!(matches("abc", "xxabcxx"), vec!["abc"]);
        assert_eq!(matches("abc", "xxabx"), Vec::<String>::new());
        assert!(compile("abc").is_match(&ch("abc")));
    }

    #[test]
    fn dot_matches_any_character_except_line_terminators() {
        assert_eq!(matches("a.c", "abc"), vec!["abc"]);
        assert_eq!(matches("a.c", "a\nc"), Vec::<String>::new());
        assert_eq!(matches("a.c", "a c"), vec!["a c"]);
    }

    #[test]
    fn anchors_match_the_whole_input() {
        assert_eq!(matches("^abc", "abcdef"), vec!["abc"]);
        assert_eq!(matches("^abc", "xabc"), Vec::<String>::new());
        assert_eq!(matches("abc$", "xabc"), vec!["abc"]);
        assert_eq!(matches("abc$", "abcx"), Vec::<String>::new());
        assert_eq!(matches("^abc$", "abc"), vec!["abc"]);
    }

    #[test]
    fn alternation_picks_the_leftmost_branch_that_matches() {
        assert_eq!(matches("cat|dog", "a dog and a cat"), vec!["dog", "cat"]);
        assert_eq!(
            matches("ab|abc", "abc"),
            vec!["ab"],
            "leftmost-first, not longest"
        );
    }

    #[test]
    fn groups_and_non_capturing_groups_work() {
        assert_eq!(matches("(ab)+", "ababab"), vec!["ababab"]);
        assert_eq!(matches("(?:ab|cd)+", "abcd"), vec!["abcd"]);
    }

    #[test]
    fn character_classes_match_sets_and_ranges() {
        assert_eq!(matches("[abc]", "xaybz"), vec!["a", "b"]);
        assert_eq!(matches("[a-c]+", "xabcx"), vec!["abc"]);
        assert_eq!(matches("[^a-c]+", "abxyzc"), vec!["xyz"]);
        // A `]` first in the class is a literal.
        assert_eq!(matches("[]]", "a]b"), vec!["]"]);
        // A trailing `-` is a literal.
        assert_eq!(matches("[a-]", "-a"), vec!["-", "a"]);
    }

    #[test]
    fn perl_classes_and_their_negations_work() {
        assert_eq!(matches(r"\d+", "a12b"), vec!["12"]);
        assert_eq!(matches(r"\D+", "12ab"), vec!["ab"]);
        assert_eq!(matches(r"\w+", "a_1-b"), vec!["a_1", "b"]);
        assert_eq!(matches(r"\W+", "a -b"), vec![" -"]);
        assert_eq!(matches(r"\s+", "a \t b"), vec![" \t "]);
        assert_eq!(matches(r"\S+", " a b "), vec!["a", "b"]);
    }

    #[test]
    fn escapes_cover_the_control_characters_and_metacharacters() {
        assert_eq!(matches(r"a\.c", "a.c"), vec!["a.c"]);
        assert_eq!(matches(r"a\.c", "abc"), Vec::<String>::new());
        assert_eq!(matches(r"a\nb", "a\nb"), vec!["a\nb"]);
        assert_eq!(matches(r"\t", "a\tb"), vec!["\t"]);
        assert_eq!(matches(r"\[", "a[b"), vec!["["]);
    }

    #[test]
    fn word_boundaries_work() {
        assert_eq!(matches(r"\bcat\b", "a cat sat"), vec!["cat"]);
        assert_eq!(matches(r"\bcat\b", "scatter"), Vec::<String>::new());
        assert_eq!(matches(r"\Bcat", "scatter"), vec!["cat"]);
    }

    #[test]
    fn greedy_and_lazy_quantifiers_differ() {
        assert_eq!(matches("<.*>", "<a><b>"), vec!["<a><b>"]);
        assert_eq!(matches("<.*?>", "<a><b>"), vec!["<a>", "<b>"]);
    }

    #[test]
    fn counted_quantifiers_work() {
        assert_eq!(matches("a{3}", "aaaa"), vec!["aaa"]);
        assert_eq!(matches("a{2,}", "aaaa"), vec!["aaaa"]);
        assert_eq!(matches("a{2,3}", "aaaa"), vec!["aaa"]);
        assert_eq!(matches("a{0,1}", "aa"), vec!["a", "a", ""]);
        // A `{` that is not a quantifier stays a literal.
        assert_eq!(matches("a{x}", "a{x}"), vec!["a{x}"]);
    }

    #[test]
    fn an_out_of_order_quantifier_is_a_compile_error() {
        let error = Regex::new_with_flags("a{3,1}", false).expect_err("must fail");
        assert!(error.message.contains("out of order"), "{}", error.message);
    }

    #[test]
    fn case_insensitivity_follows_the_flag() {
        assert_eq!(matches("abc", "ABC"), Vec::<String>::new());
        assert_eq!(
            compile_i("abc").find_all(&ch("ABC")).len(),
            1,
            "the i flag folds case"
        );
        assert_eq!(compile_i("[a-z]+").find_all(&ch("ABC")).len(), 1);
        assert_eq!(compile_i("É").find_all(&ch("é")).len(), 1);
    }

    #[test]
    fn the_anchored_test_is_what_the_hash_scope_needs() {
        let regex = compile("abc");
        assert_eq!(regex.match_at(&ch("abcdef"), 0), Some(3));
        assert_eq!(regex.match_at(&ch("xabcdef"), 0), None);
        // The substring scope uses `is_match`, which searches anywhere.
        assert!(regex.is_match(&ch("xabcdef")));
    }

    #[test]
    fn find_all_returns_non_overlapping_matches() {
        assert_eq!(matches("aa", "aaaa"), vec!["aa", "aa"]);
        assert_eq!(matches("a", "aaa"), vec!["a", "a", "a"]);
    }

    #[test]
    fn find_all_advances_past_a_zero_length_match() {
        // `a*` matches "" at every position; the scan must still terminate, and
        // it must report the same list `String.matchAll` does. Verified against
        // node: `[..."bab".matchAll(/a*/g)]` yields "", "a", "", "".
        let found = compile("a*").find_all(&ch("bab"));
        assert_eq!(found, vec![(0, 0), (1, 2), (2, 2), (3, 3)]);
    }

    #[test]
    fn matches_empty_identifies_the_zero_length_patterns() {
        for pattern in ["a*", "a?", "(a)*", "|", "^", "$", r"\b", "(?:)"] {
            assert!(
                compile(pattern).matches_empty(),
                "{pattern} can match zero characters"
            );
        }
        for pattern in ["a", "a+", "abc", r"\d", "."] {
            assert!(
                !compile(pattern).matches_empty(),
                "{pattern} cannot match zero characters"
            );
        }
    }

    #[test]
    fn a_nested_repeat_terminates_instead_of_hanging() {
        // `(a*)*` is the classic empty-body loop; it must return, not spin.
        let found = compile("(a*)*").find_all(&ch("aaa"));
        assert!(!found.is_empty());
        assert_eq!(found[0], (0, 3));
    }

    #[test]
    fn unclosed_constructs_report_a_clear_message() {
        let group = Regex::new_with_flags("(abc", false).expect_err("must fail");
        assert!(
            group.message.contains("unclosed group"),
            "{}",
            group.message
        );
        let class = Regex::new_with_flags("[abc", false).expect_err("must fail");
        assert!(
            class.message.contains("unclosed character class"),
            "{}",
            class.message
        );
        let close = Regex::new_with_flags("abc)", false).expect_err("must fail");
        assert!(close.message.contains("unmatched ')'"), "{}", close.message);
        let repeat = Regex::new_with_flags("*abc", false).expect_err("must fail");
        assert!(
            repeat.message.contains("nothing to repeat"),
            "{}",
            repeat.message
        );
    }

    #[test]
    fn unsupported_constructs_say_so_instead_of_mis_matching() {
        let backref = Regex::new_with_flags(r"(a)\1", false).expect_err("must fail");
        assert!(
            backref.message.contains("back-reference"),
            "{}",
            backref.message
        );
        let lookahead = Regex::new_with_flags("a(?=b)", false).expect_err("must fail");
        assert!(
            lookahead.message.contains("look-around"),
            "{}",
            lookahead.message
        );
    }

    #[test]
    fn the_error_carries_the_offset_and_prints_the_message() {
        let error = Regex::new_with_flags("abc(", false).expect_err("must fail");
        assert_eq!(error.at, 3);
        assert!(error.to_string().contains("unclosed group"), "{error}");
    }

    #[test]
    fn display_shows_the_pattern_and_its_flag() {
        assert_eq!(compile("abc").to_string(), "/abc/");
        assert_eq!(compile_i("abc").to_string(), "/abc/i");
        assert_eq!(compile("abc").as_str(), "abc");
    }

    #[test]
    fn literal_find_all_matches_and_folds_case() {
        assert_eq!(literal_find_all(&ch("abcabc"), &ch("abc"), false).len(), 2);
        assert_eq!(
            literal_find_all(&ch("ABC"), &ch("abc"), true),
            vec![(0, 3)],
            "folding is opt-in"
        );
        assert!(literal_find_all(&ch("ABC"), &ch("abc"), false).is_empty());
        // An empty needle never matches, so a cleared box highlights nothing.
        assert!(literal_find_all(&ch("abc"), &ch(""), false).is_empty());
        assert!(literal_find_all(&ch("ab"), &ch("abc"), false).is_empty());
    }

    #[test]
    fn literal_find_all_finds_overlapping_occurrences() {
        assert_eq!(
            literal_find_all(&ch("aaaa"), &ch("aa"), false),
            vec![(0, 2), (1, 3), (2, 4)]
        );
    }

    #[test]
    fn literal_find_all_reports_character_offsets_not_bytes() {
        // "héllo" — the `é` is two bytes but one character.
        assert_eq!(
            literal_find_all(&ch("héllo"), &ch("llo"), false),
            vec![(2, 5)]
        );
    }

    #[test]
    fn is_word_and_is_space_match_javascript() {
        assert!(is_word('a') && is_word('Z') && is_word('0') && is_word('_'));
        assert!(!is_word('-') && !is_word('é'));
        assert!(is_space(' ') && is_space('\t') && is_space('\u{a0}'));
        assert!(!is_space('x'));
    }
}
