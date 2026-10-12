//! The find widget (M2-W6 / T1) — bottom bar, match scope, counter, highlight.
//!
//! A faithful port of `web/findWidget.ts` to a terminal UI. The reference walks
//! its rendered DOM to highlight matches; here the same search runs over the
//! commit data and the renderer applies the resulting ranges to the text it
//! draws. Everything else — the scope, the counter, the zero-length rule, the
//! wrap-around next/previous, the auto-open-CDV toggle — is the reference's.
//!
//! Reference anchors (read-only `../vscode-git-graph`):
//!
//! * `web/findWidget.ts:224-240` — the match scope. A row matches when the
//!   pattern hits the **author** (only while the author column is visible), the
//!   **commit hash** (`hash.search(pattern) === 0`, i.e. an anchored match on the
//!   full hash, or anywhere in the 7-char abbreviation), the **message**, a
//!   **branch/remote/tag name**, the **date** (only while the date column is
//!   visible) or the **stash selector**. The uncommitted row is skipped.
//! * `web/findWidget.ts:222` — flags are `'u'` plus `'i'` unless match-case is
//!   on; in literal mode the metacharacters are escaped first.
//! * `web/findWidget.ts:299-302` — a pattern that can match zero characters is
//!   rejected with *"Cannot use a regular expression which has zero length
//!   matches"* and the match list is cleared.
//! * `web/findWidget.ts:355` — the counter is `"N of M"`, or `"No Results"`.
//! * `web/findWidget.ts:362-378` — `prev`/`next` **wrap**; `next` moves forward
//!   and `prev` backward, both from the current position.
//! * `web/findWidget.ts:368-378` — when the open-CDV toggle is on, moving to a
//!   match loads the commit-details view for it.
//! * `web/findWidget.ts:139-158` — `show`/`close`/`restoreState`; closing clears
//!   the text, the matches and the position.
//!
//! Deviations, all deliberate and documented:
//!
//! 1. The reference debounces typing by 200 ms (`findWidget.ts:38-50`). A
//!    terminal redraws per keypress, so [`FindWidget::set_text`] re-scans
//!    immediately — the debounce exists to avoid DOM churn, which has no
//!    equivalent here.
//! 2. Highlighting is applied to the rendered column text rather than to DOM
//!    text nodes, so [`FindWidget::ranges_in`] returns **character** ranges.
//! 3. The zero-length rejection is decided from the pattern alone
//!    ([`regex::Regex::matches_empty`]) instead of being discovered mid-scan, so
//!    the error appears on the first keystroke rather than on the first
//!    zero-length hit. The message and the "clear the matches" behaviour are
//!    identical.

pub mod regex;

use crate::app::ColumnState;
use crate::data::Commit;
use crate::menu::classify_ref;
use crate::menu::MenuSurface;
use crate::render::columns::short_date;
use regex::{literal_find_all, Regex};

/// The reference's rejection text, verbatim (`web/findWidget.ts:300`).
pub const ZERO_LENGTH_MESSAGE: &str = "Cannot use a regular expression which has zero length matches";

/// One matched commit row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FindMatch {
    /// Index into [`crate::app::AppState::rows`].
    pub row: usize,
    pub hash: String,
}

/// A compiled search pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Pattern {
    /// Literal mode: the reference escapes the metacharacters and builds a
    /// regex (`findWidget.ts:222`); a substring scan is equivalent and cannot
    /// fail to compile.
    Literal {
        needle: Vec<char>,
        case_sensitive: bool,
    },
    /// Regex mode.
    Regex(Regex),
}

impl Pattern {
    /// Compile `text` the way `findWidget.ts:222` does.
    fn compile(text: &str, is_regex: bool, case_sensitive: bool) -> Result<Self, String> {
        if is_regex {
            let regex = Regex::new_with_flags(text, !case_sensitive).map_err(|e| e.message)?;
            Ok(Pattern::Regex(regex))
        } else {
            Ok(Pattern::Literal {
                needle: text.chars().collect(),
                case_sensitive,
            })
        }
    }

    /// Every match in `text`, as character ranges.
    fn ranges_in(&self, text: &str) -> Vec<(usize, usize)> {
        match self {
            Pattern::Literal {
                needle,
                case_sensitive,
            } => literal_find_all(&chars(text), needle, !case_sensitive),
            Pattern::Regex(regex) => regex.find_all(&chars(text)),
        }
    }

    fn is_match(&self, text: &str) -> bool {
        !self.ranges_in(text).is_empty()
    }

    /// Does the pattern match at character offset 0? (`hash.search(p) === 0`.)
    fn is_anchored_match(&self, text: &str) -> bool {
        match self {
            Pattern::Literal {
                needle,
                case_sensitive,
            } => {
                if needle.is_empty() {
                    return false;
                }
                let text = chars(text);
                text.len() >= needle.len()
                    && needle.iter().enumerate().all(|(i, want)| {
                        let got = text[i];
                        got == *want
                            || (!case_sensitive && got.to_lowercase().eq(want.to_lowercase()))
                    })
            }
            Pattern::Regex(regex) => regex.match_at(&chars(text), 0).is_some(),
        }
    }

    /// Can the pattern match zero characters? (`findWidget.ts:299-302`.)
    fn matches_empty(&self) -> bool {
        match self {
            Pattern::Literal { needle, .. } => needle.is_empty(),
            Pattern::Regex(regex) => regex.matches_empty(),
        }
    }
}

fn chars(text: &str) -> Vec<char> {
    text.chars().collect()
}

/// The reference's `abbrevCommit`: the first seven characters of the hash.
pub fn abbrev_commit(hash: &str) -> String {
    hash.chars().take(7).collect()
}

/// A branch decoration plus the remote-tracking refs that point at the same row.
///
/// Mirrors `getBranchLabels(heads, remotes)`'s `heads[].remotes` nesting
/// (`findWidget.ts:229`), flattened because `data.rs` only loads `%D`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefName {
    pub name: String,
    pub remotes: Vec<String>,
}

/// Split a commit's `%D` decorations into `(heads, remotes, tags, stash)`.
///
/// The reference reads `commit.heads` / `commit.remotes` / `commit.tags` /
/// `commit.stash` from the log format; the terminal port only has `%D`, so the
/// same partition is derived with [`crate::menu::classify_ref`] — the one
/// classification the W5 menu already uses, so the find scope and the menu can
/// never disagree about what a decoration means.
pub fn branch_labels(refs: &[String]) -> (Vec<RefName>, Vec<RefName>, Vec<String>, Option<String>) {
    let mut heads: Vec<RefName> = Vec::new();
    let mut remotes: Vec<RefName> = Vec::new();
    let mut tags: Vec<String> = Vec::new();
    let mut stash: Option<String> = None;

    for label in refs {
        let (surface, name) = classify_ref(label);
        match surface {
            MenuSurface::Tag => tags.push(name),
            MenuSurface::RemoteBranch => remotes.push(RefName {
                name,
                remotes: Vec::new(),
            }),
            MenuSurface::Stash => {
                stash = Some(stash_selector(label));
            }
            _ => heads.push(RefName {
                name,
                remotes: Vec::new(),
            }),
        }
    }
    (heads, remotes, tags, stash)
}

/// `refs/stash` → `stash@{0}`; anything already a selector is passed through.
///
/// The reference's `commit.stash.selector` is the `stash@{n}` string the user
/// sees, so `refs/stash` (the only stash decoration `%D` emits for the current
/// entry) is rendered in that form.
pub fn stash_selector(label: &str) -> String {
    let trimmed = label.trim();
    if trimmed.starts_with("stash@{") {
        return trimmed.to_string();
    }
    "stash@{0}".to_string()
}

/// Does one commit row match the pattern? (`findWidget.ts:224-240`.)
pub fn row_matches(pattern: &Pattern, commit: &Commit, columns: &ColumnState) -> bool {
    let (heads, remotes, tags, stash) = branch_labels(&commit.refs);

    (columns.author && pattern.is_match(&commit.author))
        || (columns.commit
            && (pattern.is_anchored_match(&commit.hash)
                || pattern.is_match(&abbrev_commit(&commit.hash))))
        || pattern.is_match(&commit.subject)
        || heads.iter().any(|head| {
            pattern.is_match(&head.name)
                || head.remotes.iter().any(|remote| pattern.is_match(remote))
        })
        || remotes.iter().any(|remote| pattern.is_match(&remote.name))
        || tags.iter().any(|tag| pattern.is_match(tag))
        || (columns.date && pattern.is_match(&short_date(commit.date)))
        || stash
            .as_deref()
            .map(|selector| pattern.is_match(selector))
            .unwrap_or(false)
}

/// The find widget's whole state — `FindWidget`'s private fields.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FindWidget {
    /// The pattern text (`this.text`).
    pub text: String,
    pub visible: bool,
    /// `workspaceState.findIsCaseSensitive`.
    pub case_sensitive: bool,
    /// `workspaceState.findIsRegex`.
    pub is_regex: bool,
    /// `workspaceState.findOpenCommitDetailsView`.
    pub open_cdv: bool,
    /// Matched rows, in table order.
    pub matches: Vec<FindMatch>,
    /// Index into `matches`, or `-1` for "no current match".
    pub position: i32,
    /// Compile / zero-length error shown in the widget (`ATTR_ERROR`).
    pub error: Option<String>,
}

impl FindWidget {
    pub fn new() -> Self {
        Self::default()
    }

    /// `show()` (`findWidget.ts:139-150`).
    pub fn show(&mut self) {
        self.visible = true;
    }

    /// `close()` (`findWidget.ts:155-173`): clears text, matches and position.
    pub fn close(&mut self) {
        if !self.visible {
            return;
        }
        self.visible = false;
        self.clear_matches();
        self.text.clear();
        self.matches.clear();
        self.position = -1;
        self.error = None;
    }

    fn clear_matches(&mut self) {
        self.matches.clear();
        self.position = -1;
    }

    /// `findMatches()` — rescan every row and reset the position.
    ///
    /// `go_to_hash` is the reference's first argument: when the current match's
    /// hash still matches after a rescan, the position follows it instead of
    /// jumping back to the first match (`findWidget.ts:344-351`).
    pub fn recompute(
        &mut self,
        commits: &[Commit],
        columns: &ColumnState,
        uncommitted: Option<usize>,
        go_to_hash: Option<&str>,
    ) {
        self.matches.clear();
        self.position = -1;

        if self.text.is_empty() {
            self.error = None;
            return;
        }
        let pattern = match Pattern::compile(&self.text, self.is_regex, self.case_sensitive) {
            Ok(pattern) => {
                self.error = None;
                pattern
            }
            Err(message) => {
                self.error = Some(message);
                return;
            }
        };

        for (row, commit) in commits.iter().enumerate() {
            // `commit.hash !== UNCOMMITTED` — the uncommitted row is never a match.
            if uncommitted == Some(row) {
                continue;
            }
            if row_matches(&pattern, commit, columns) {
                self.matches.push(FindMatch {
                    row,
                    hash: commit.hash.clone(),
                });
            }
        }

        // `findWidget.ts:299-302`: a zero-length pattern clears the matches and
        // reports the error instead of highlighting nothing useful.
        if pattern.matches_empty() && !commits.is_empty() {
            self.error = Some(ZERO_LENGTH_MESSAGE.to_string());
            self.matches.clear();
            self.position = -1;
            return;
        }

        if self.matches.is_empty() {
            return;
        }
        self.position = 0;
        if let Some(hash) = go_to_hash {
            if let Some(index) = self.matches.iter().position(|found| found.hash == hash) {
                self.position = index as i32;
            }
        }
    }

    /// Set the pattern text and rescan, keeping the current match if it survives.
    pub fn set_text(
        &mut self,
        text: String,
        commits: &[Commit],
        columns: &ColumnState,
        uncommitted: Option<usize>,
    ) {
        let previous = self.current_hash().map(str::to_string);
        self.text = text;
        self.recompute(commits, columns, uncommitted, previous.as_deref());
    }

    /// Append one character (the terminal's `keyup` handler).
    pub fn push_char(&mut self, c: char) {
        self.text.push(c);
    }

    /// Remove the last character.
    pub fn pop_char(&mut self) {
        self.text.pop();
    }

    /// Clear the pattern.
    pub fn clear_text(&mut self) {
        self.text.clear();
    }

    /// `getCurrentHash()`.
    pub fn current_hash(&self) -> Option<&str> {
        if self.position < 0 {
            return None;
        }
        self.matches
            .get(self.position as usize)
            .map(|found| found.hash.as_str())
    }

    /// The row of the current match.
    pub fn current_row(&self) -> Option<usize> {
        if self.position < 0 {
            return None;
        }
        self.matches.get(self.position as usize).map(|found| found.row)
    }

    /// `updatePosition`'s counter text (`findWidget.ts:355`).
    pub fn counter(&self) -> String {
        if self.matches.is_empty() {
            "No Results".to_string()
        } else {
            format!("{} of {}", self.position + 1, self.matches.len())
        }
    }

    /// `next()` (`findWidget.ts:372-376`): forward with wrap-around.
    pub fn next(&mut self) -> Option<usize> {
        if self.matches.is_empty() {
            return None;
        }
        let len = self.matches.len() as i32;
        self.position = if self.position < len - 1 {
            self.position + 1
        } else {
            0
        };
        self.current_row()
    }

    /// `prev()` (`findWidget.ts:366-370`): backward with wrap-around.
    pub fn prev(&mut self) -> Option<usize> {
        if self.matches.is_empty() {
            return None;
        }
        let len = self.matches.len() as i32;
        self.position = if self.position > 0 {
            self.position - 1
        } else {
            len - 1
        };
        self.current_row()
    }

    /// Highlight ranges for `text`, or nothing when the widget has no pattern.
    ///
    /// The renderer calls this per column so the matched characters are styled
    /// the way the reference styles its `findMatch` spans.
    pub fn ranges_in(&self, text: &str) -> Vec<(usize, usize)> {
        if self.text.is_empty() {
            return Vec::new();
        }
        match Pattern::compile(&self.text, self.is_regex, self.case_sensitive) {
            Ok(pattern) => pattern.ranges_in(text),
            Err(_) => Vec::new(),
        }
    }

    /// Is `row` a match at all? (The row gets the `findMatch` background.)
    pub fn row_is_match(&self, row: usize) -> bool {
        self.matches.iter().any(|found| found.row == row)
    }

    /// Is `row` the **current** match? (The row gets `findCurrentCommit`.)
    pub fn row_is_current(&self, row: usize) -> bool {
        self.current_row() == Some(row)
    }

    /// The reference's one-line widget state, for `--dump` evidence.
    pub fn status_line(&self) -> String {
        let mut parts = vec![format!("find: {}", self.text)];
        parts.push(format!("[{}]", self.counter()));
        if self.case_sensitive {
            parts.push("Aa".to_string());
        }
        if self.is_regex {
            parts.push(".*".to_string());
        }
        if self.open_cdv {
            parts.push("cdv".to_string());
        }
        if let Some(error) = &self.error {
            parts.push(error.clone());
        }
        parts.join(" ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commit(hash: &str, author: &str, subject: &str, refs: &[&str], date: i64) -> Commit {
        Commit {
            hash: hash.to_string(),
            abbrev: abbrev_commit(hash),
            parents: Vec::new(),
            author: author.to_string(),
            date,
            subject: subject.to_string(),
            refs: refs.iter().map(|r| (*r).to_string()).collect(),
        }
    }

    /// Four rows covering every scope branch of `findWidget.ts:224-240`.
    fn rows() -> Vec<Commit> {
        vec![
            commit(
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                "Ada Lovelace",
                "add find widget",
                &["HEAD -> main", "origin/main"],
                1_700_000_000,
            ),
            commit(
                "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                "Grace Hopper",
                "fix the compiler",
                &["feature/x", "tag: v1.0"],
                1_600_000_000,
            ),
            commit(
                "cccccccccccccccccccccccccccccccccccccccc",
                "Alan Turing",
                "document the machine",
                &["refs/stash"],
                0,
            ),
            commit(
                "dddddddddddddddddddddddddddddddddddddddd",
                "Ada Lovelace",
                "merge branches",
                &[],
                1_500_000_000,
            ),
        ]
    }

    fn scan(text: &str, is_regex: bool, case_sensitive: bool) -> FindWidget {
        let mut widget = FindWidget {
            text: text.to_string(),
            visible: true,
            is_regex,
            case_sensitive,
            ..Default::default()
        };
        widget.recompute(&rows(), &ColumnState::default(), None, None);
        widget
    }

    fn matched_rows(widget: &FindWidget) -> Vec<usize> {
        widget.matches.iter().map(|found| found.row).collect()
    }

    #[test]
    fn literal_search_covers_every_scope_branch() {
        // author
        assert_eq!(matched_rows(&scan("Ada", false, false)), vec![0, 3]);
        // subject
        assert_eq!(matched_rows(&scan("compiler", false, false)), vec![1]);
        // branch name
        assert_eq!(matched_rows(&scan("feature/x", false, false)), vec![1]);
        // tag name (with and without the `tag: ` prefix)
        assert_eq!(matched_rows(&scan("v1.0", false, false)), vec![1]);
        // remote-tracking decoration
        assert_eq!(matched_rows(&scan("origin/main", false, false)), vec![0]);
        // stash selector
        assert_eq!(matched_rows(&scan("stash@{0}", false, false)), vec![2]);
        // date, formatted like the reference's `formatShortDate`
        assert_eq!(matched_rows(&scan("2023-11-14", false, false)), vec![0]);
        // hash prefix, and a substring of the abbreviation
        assert_eq!(matched_rows(&scan("aaaaaaa", false, false)), vec![0]);
        assert_eq!(matched_rows(&scan("bbbbb", false, false)), vec![1]);
    }

    #[test]
    fn hidden_columns_leave_the_scope() {
        let columns = ColumnState {
            date: false,
            author: false,
            ..Default::default()
        };
        let mut widget = FindWidget {
            text: "Ada".to_string(),
            ..Default::default()
        };
        widget.recompute(&rows(), &columns, None, None);
        assert!(widget.matches.is_empty(), "author column is hidden");

        let mut widget = FindWidget {
            text: "2023-11-14".to_string(),
            ..Default::default()
        };
        widget.recompute(&rows(), &columns, None, None);
        assert!(widget.matches.is_empty(), "date column is hidden");

        // The subject is not a column that can be hidden from find.
        let mut widget = FindWidget {
            text: "compiler".to_string(),
            ..Default::default()
        };
        widget.recompute(&rows(), &columns, None, None);
        assert_eq!(matched_rows(&widget), vec![1]);
    }

    #[test]
    fn the_uncommitted_row_is_never_a_match() {
        let mut widget = FindWidget {
            text: "Ada".to_string(),
            ..Default::default()
        };
        widget.recompute(&rows(), &ColumnState::default(), Some(0), None);
        assert_eq!(matched_rows(&widget), vec![3]);
    }

    #[test]
    fn case_sensitivity_follows_the_toggle() {
        assert_eq!(matched_rows(&scan("ada", false, false)), vec![0, 3]);
        assert!(scan("ada", false, true).matches.is_empty());
        assert_eq!(matched_rows(&scan("Ada", false, true)), vec![0, 3]);
    }

    #[test]
    fn literal_mode_escapes_the_metacharacters() {
        // In literal mode `a.*a` is the four characters `a`, `.`, `*`, `a`.
        assert!(scan("a.*a", false, false).matches.is_empty());
        // In regex mode the same text is a pattern and hits every row whose
        // author starts with `A` and later holds an `a` — rows 0 and 3
        // (`Ada Lovelace`) and row 2 (`Alan Turing`).
        assert_eq!(matched_rows(&scan("A.*a", true, true)), vec![0, 2, 3]);
    }

    #[test]
    fn regex_mode_uses_the_engine() {
        assert_eq!(matched_rows(&scan("^aaaa", true, true)), vec![0]);
        assert_eq!(matched_rows(&scan("(compiler|machine)", true, false)), vec![1, 2]);
        assert_eq!(matched_rows(&scan(r"\d{4}-11-14", true, false)), vec![0]);
    }

    #[test]
    fn an_invalid_pattern_reports_the_engine_reason() {
        let widget = scan("(", true, false);
        assert!(widget.matches.is_empty());
        let error = widget.error.clone().unwrap_or_default();
        assert!(error.contains("unclosed"), "{error}");
    }

    #[test]
    fn a_zero_length_pattern_is_rejected_with_the_reference_message() {
        for text in ["a*", "^", "$", r"\b"] {
            let widget = scan(text, true, false);
            assert_eq!(
                widget.error.as_deref(),
                Some(ZERO_LENGTH_MESSAGE),
                "{text} must be rejected"
            );
            assert!(widget.matches.is_empty(), "{text} must clear the matches");
            assert_eq!(widget.position, -1);
            assert_eq!(widget.counter(), "No Results");
        }
    }

    #[test]
    fn a_zero_length_pattern_is_fine_on_an_empty_repository() {
        let mut widget = FindWidget {
            text: "a*".to_string(),
            is_regex: true,
            ..Default::default()
        };
        widget.recompute(&[], &ColumnState::default(), None, None);
        assert!(widget.error.is_none());
        assert_eq!(widget.counter(), "No Results");
    }

    #[test]
    fn the_counter_reads_n_of_m_and_no_results() {
        let mut widget = scan("Ada", false, false);
        assert_eq!(widget.counter(), "1 of 2");
        widget.next();
        assert_eq!(widget.counter(), "2 of 2");
        widget.next();
        assert_eq!(widget.counter(), "1 of 2", "next wraps");

        let none = scan("zzzz", false, false);
        assert_eq!(none.counter(), "No Results");
        assert_eq!(none.position, -1);
    }

    #[test]
    fn next_and_prev_wrap_in_both_directions() {
        let mut widget = scan("a", false, false); // all four subjects/authors hit
        assert_eq!(widget.matches.len(), 4);
        assert_eq!(widget.position, 0);
        assert_eq!(widget.next(), Some(1));
        assert_eq!(widget.prev(), Some(0));
        assert_eq!(widget.prev(), Some(3), "prev wraps to the last match");
        assert_eq!(widget.next(), Some(0), "next wraps back to the first");
    }

    #[test]
    fn next_and_prev_are_noops_without_matches() {
        let mut widget = scan("zzzz", false, false);
        assert_eq!(widget.next(), None);
        assert_eq!(widget.prev(), None);
        assert_eq!(widget.position, -1);
    }

    #[test]
    fn rescanning_keeps_the_current_match_when_it_survives() {
        let mut widget = scan("a", false, false);
        widget.next();
        let current = widget.current_hash().map(str::to_string).expect("a match");
        assert_eq!(widget.position, 1);

        widget.recompute(&rows(), &ColumnState::default(), None, Some(&current));
        assert_eq!(
            widget.current_hash(),
            Some(current.as_str()),
            "the position followed the hash"
        );
    }

    #[test]
    fn rescanning_falls_back_to_the_first_match() {
        let mut widget = scan("a", false, false);
        widget.next();
        widget.recompute(&rows(), &ColumnState::default(), None, Some("nope"));
        assert_eq!(widget.position, 0);
    }

    #[test]
    fn set_text_rescans_and_clears_when_emptied() {
        let mut widget = FindWidget {
            visible: true,
            ..Default::default()
        };
        widget.set_text("compiler".to_string(), &rows(), &ColumnState::default(), None);
        assert_eq!(matched_rows(&widget), vec![1]);
        widget.set_text(String::new(), &rows(), &ColumnState::default(), None);
        assert!(widget.matches.is_empty());
        assert!(widget.error.is_none());
        assert_eq!(widget.counter(), "No Results");
    }

    #[test]
    fn closing_clears_the_text_the_matches_and_the_position() {
        let mut widget = scan("Ada", false, false);
        assert!(widget.visible);
        widget.close();
        assert!(!widget.visible);
        assert!(widget.text.is_empty());
        assert!(widget.matches.is_empty());
        assert_eq!(widget.position, -1);
        assert!(widget.error.is_none());

        // Closing twice is a no-op, not a panic.
        widget.close();
        assert!(!widget.visible);
    }

    #[test]
    fn ranges_in_returns_character_ranges_for_the_renderer() {
        let widget = scan("Ada", false, false);
        assert_eq!(widget.ranges_in("Ada Lovelace"), vec![(0, 3)]);
        assert_eq!(widget.ranges_in("Grace Hopper"), Vec::new());
        // UTF-8: indices are characters, not bytes.
        assert_eq!(widget.ranges_in("héllo Ada"), vec![(6, 9)]);
    }

    #[test]
    fn ranges_in_is_empty_without_a_pattern() {
        let widget = FindWidget::default();
        assert!(widget.ranges_in("anything").is_empty());
    }

    #[test]
    fn row_helpers_answer_match_and_current() {
        let mut widget = scan("Ada", false, false);
        assert!(widget.row_is_match(0) && widget.row_is_match(3));
        assert!(!widget.row_is_match(1));
        assert!(widget.row_is_current(0));
        assert!(!widget.row_is_current(3));
        widget.next();
        assert!(widget.row_is_current(3));
        assert!(!widget.row_is_current(0));
    }

    #[test]
    fn the_status_line_reports_every_modifier() {
        let mut widget = scan("Ada", true, true);
        widget.open_cdv = true;
        let line = widget.status_line();
        assert!(line.contains("find: Ada"), "{line}");
        assert!(line.contains("[1 of 2]"), "{line}");
        assert!(line.contains("Aa"), "{line}");
        assert!(line.contains(".*"), "{line}");
        assert!(line.contains("cdv"), "{line}");
    }

    #[test]
    fn push_and_pop_edit_the_pattern() {
        let mut widget = FindWidget::new();
        widget.push_char('a');
        widget.push_char('b');
        assert_eq!(widget.text, "ab");
        widget.pop_char();
        assert_eq!(widget.text, "a");
        widget.clear_text();
        assert!(widget.text.is_empty());
        // Popping an empty pattern is a no-op.
        widget.pop_char();
        assert!(widget.text.is_empty());
    }

    #[test]
    fn abbrev_commit_truncates_to_seven_characters() {
        assert_eq!(abbrev_commit("0123456789"), "0123456");
        assert_eq!(abbrev_commit("abc"), "abc");
        assert_eq!(abbrev_commit(""), "");
    }

    #[test]
    fn branch_labels_partitions_the_decorations() {
        let refs = vec![
            "HEAD -> main".to_string(),
            "origin/main".to_string(),
            "tag: v1.0".to_string(),
            "refs/stash".to_string(),
        ];
        let (heads, remotes, tags, stash) = branch_labels(&refs);
        assert_eq!(heads.iter().map(|h| h.name.as_str()).collect::<Vec<_>>(), vec!["main"]);
        assert_eq!(
            remotes.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
            vec!["origin/main"]
        );
        assert_eq!(tags, vec!["v1.0".to_string()]);
        assert_eq!(stash.as_deref(), Some("stash@{0}"));
    }

    #[test]
    fn stash_selectors_are_normalised() {
        assert_eq!(stash_selector("refs/stash"), "stash@{0}");
        assert_eq!(stash_selector("stash@{3}"), "stash@{3}");
        assert_eq!(stash_selector("  refs/stash  "), "stash@{0}");
    }
}

