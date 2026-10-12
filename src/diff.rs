//! Unified-diff parsing and in-terminal rendering (M4-W12 / T5).
//!
//! The pager used to show the raw text `git diff` printed. W12 keeps that text
//! as the source of truth but renders it the way the design mockup does:
//! green/red tints, a line-number gutter, and one `Line` per diff line. The
//! parser is the same for every diff source (working tree, a commit file, or
//! the pager), so `View Diff` and the inline expansion agree by construction.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

/// What kind of unified-diff line this is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffKind {
    /// `diff --git`, `index`, `---`, `+++`, rename/mode lines, `Binary files`.
    Header,
    /// `@@ -a,b +c,d @@`.
    Hunk,
    /// `+added`.
    Add,
    /// `-removed`.
    Del,
    /// ` context` (and `\ No newline at end of file`).
    Context,
}

/// One parsed diff line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffLine {
    pub kind: DiffKind,
    /// The old-file line number (`None` for adds and headers).
    pub old_no: Option<u32>,
    /// The new-file line number (`None` for deletes and headers).
    pub new_no: Option<u32>,
    /// The raw line, prefix included.
    pub text: String,
}

impl DiffLine {
    /// The line-number gutter shown to the left of the text.
    pub fn gutter(&self) -> String {
        let (marker, no) = match self.kind {
            DiffKind::Add => ('+', self.new_no),
            DiffKind::Del => ('-', self.old_no),
            DiffKind::Hunk => ('@', None),
            DiffKind::Header => (' ', None),
            DiffKind::Context => (' ', self.new_no.or(self.old_no)),
        };
        let number = no.map(|n| n.to_string()).unwrap_or_default();
        format!("{marker} {number:>5} ")
    }

    /// The text without its diff prefix (the `+`/`-`/space the gutter shows).
    pub fn body(&self) -> &str {
        match self.kind {
            DiffKind::Add | DiffKind::Del => self.text.get(1..).unwrap_or(""),
            DiffKind::Context => self.text.get(1..).unwrap_or(&self.text),
            _ => &self.text,
        }
    }
}

/// Parse unified-diff text into lines, tracking old/new numbers through hunks.
pub fn parse(text: &str) -> Vec<DiffLine> {
    let mut lines = Vec::new();
    let mut old_no = 0u32;
    let mut new_no = 0u32;
    let mut in_hunk = false;

    for raw in text.lines() {
        if raw.starts_with("@@") {
            if let Some((old_start, new_start)) = parse_hunk_header(raw) {
                old_no = old_start;
                new_no = new_start;
            }
            in_hunk = true;
            lines.push(DiffLine {
                kind: DiffKind::Hunk,
                old_no: None,
                new_no: None,
                text: raw.to_string(),
            });
            continue;
        }
        if is_header(raw) {
            in_hunk = false;
            lines.push(DiffLine {
                kind: DiffKind::Header,
                old_no: None,
                new_no: None,
                text: raw.to_string(),
            });
            continue;
        }
        if !in_hunk {
            lines.push(DiffLine {
                kind: DiffKind::Header,
                old_no: None,
                new_no: None,
                text: raw.to_string(),
            });
            continue;
        }
        let (kind, old, new) = match raw.chars().next() {
            Some('+') => {
                let line = (DiffKind::Add, None, Some(new_no));
                new_no += 1;
                line
            }
            Some('-') => {
                let line = (DiffKind::Del, Some(old_no), None);
                old_no += 1;
                line
            }
            Some('\\') => (DiffKind::Context, None, None),
            _ => {
                let line = (DiffKind::Context, Some(old_no), Some(new_no));
                old_no += 1;
                new_no += 1;
                line
            }
        };
        lines.push(DiffLine {
            kind,
            old_no: old,
            new_no: new,
            text: raw.to_string(),
        });
    }
    lines
}

/// Does this line start a new file/section (rather than continue a hunk)?
fn is_header(line: &str) -> bool {
    const PREFIXES: [&str; 12] = [
        "diff --git",
        "index ",
        "--- ",
        "+++ ",
        "new file",
        "deleted file",
        "similarity index",
        "rename ",
        "old mode",
        "new mode",
        "Binary files",
        "GIT binary patch",
    ];
    PREFIXES.iter().any(|prefix| line.starts_with(prefix))
}

/// Parse `@@ -a,b +c,d @@ tail` into `(a, c)`.
pub fn parse_hunk_header(line: &str) -> Option<(u32, u32)> {
    let rest = line.strip_prefix("@@ ")?;
    let (range, _tail) = rest.split_once(" @@")?;
    let mut parts = range.split(' ');
    let old = parts.next()?.strip_prefix('-')?;
    let new = parts.next()?.strip_prefix('+')?;
    let old_start = old.split(',').next()?.parse().ok()?;
    let new_start = new.split(',').next()?.parse().ok()?;
    Some((old_start, new_start))
}

/// The tinted background for an added line.
const ADD_BG: Color = Color::Rgb(0x14, 0x2e, 0x1c);
/// The tinted background for a removed line.
const DEL_BG: Color = Color::Rgb(0x3a, 0x1a, 0x1e);

/// Style one parsed diff line for the terminal.
pub fn styled_line(line: &DiffLine, width: u16) -> Line<'static> {
    let gutter_style = match line.kind {
        DiffKind::Add => Style::default().fg(Color::Green),
        DiffKind::Del => Style::default().fg(Color::Red),
        DiffKind::Hunk => Style::default()
            .fg(Color::Magenta)
            .add_modifier(Modifier::BOLD),
        _ => Style::default().fg(Color::DarkGray),
    };
    let (text_style, marker) = match line.kind {
        DiffKind::Add => (
            Style::default().fg(Color::Green).bg(ADD_BG),
            Span::styled("+", Style::default().fg(Color::Green)),
        ),
        DiffKind::Del => (
            Style::default().fg(Color::Red).bg(DEL_BG),
            Span::styled("-", Style::default().fg(Color::Red)),
        ),
        DiffKind::Hunk => (
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
            Span::raw(""),
        ),
        _ => (Style::default().fg(Color::Gray), Span::raw(" ")),
    };
    let _ = width;
    let body = line.body().to_string();
    let mut spans = vec![Span::styled(line.gutter(), gutter_style), marker];
    if !matches!(line.kind, DiffKind::Add | DiffKind::Del) {
        spans.pop();
    }
    spans.push(Span::styled(body, text_style));
    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,3 +1,4 @@ fn main() {
 use std::io;
-old line
+new line
+another new line
 context
";

    #[test]
    fn the_header_and_hunk_lines_are_classified() {
        let lines = parse(SAMPLE);
        assert_eq!(lines[0].kind, DiffKind::Header);
        assert_eq!(lines[1].kind, DiffKind::Header);
        assert_eq!(lines[2].kind, DiffKind::Header);
        assert_eq!(lines[3].kind, DiffKind::Header);
        assert_eq!(lines[4].kind, DiffKind::Hunk);
    }

    #[test]
    fn line_numbers_walk_old_and_new_sides() {
        let lines = parse(SAMPLE);
        let adds: Vec<&DiffLine> = lines.iter().filter(|l| l.kind == DiffKind::Add).collect();
        assert_eq!(adds.len(), 2);
        assert_eq!(adds[0].new_no, Some(2));
        assert_eq!(adds[1].new_no, Some(3));
        let dels: Vec<&DiffLine> = lines.iter().filter(|l| l.kind == DiffKind::Del).collect();
        assert_eq!(dels[0].old_no, Some(2));
        // ` context` is new line 4, old line 3.
        let context = lines.iter().find(|l| l.text == " context").unwrap();
        assert_eq!(context.old_no, Some(3));
        assert_eq!(context.new_no, Some(4));
    }

    #[test]
    fn stats_count_only_added_and_removed_lines() {
        let lines = parse(SAMPLE);
        let adds = lines.iter().filter(|l| l.kind == DiffKind::Add).count();
        let dels = lines.iter().filter(|l| l.kind == DiffKind::Del).count();
        assert_eq!((adds, dels), (2, 1));
    }

    #[test]
    fn the_gutter_shows_the_right_number_per_side() {
        let lines = parse(SAMPLE);
        let add = lines.iter().find(|l| l.kind == DiffKind::Add).unwrap();
        assert_eq!(add.gutter(), "+     2 ");
        assert_eq!(add.body(), "new line");
        let del = lines.iter().find(|l| l.kind == DiffKind::Del).unwrap();
        assert_eq!(del.gutter(), "-     2 ");
        assert_eq!(del.body(), "old line");
    }

    #[test]
    fn a_hunk_header_parses_both_ranges() {
        assert_eq!(
            parse_hunk_header("@@ -131,6 +131,20 @@ fn x()"),
            Some((131, 131))
        );
        assert_eq!(parse_hunk_header("@@ -0,0 +1 @@"), Some((0, 1)));
        assert_eq!(parse_hunk_header("not a header"), None);
    }

    #[test]
    fn a_binary_diff_is_header_only() {
        let lines = parse("diff --git a/x b/x\nBinary files a/x and b/x differ\n");
        assert_eq!(lines.len(), 2);
        assert!(lines.iter().all(|l| l.kind == DiffKind::Header));
    }

    #[test]
    fn an_empty_diff_is_empty() {
        assert!(parse("").is_empty());
    }
}
