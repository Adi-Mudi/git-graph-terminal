//! The read-only pager overlay (M2-W7 / T3).
//!
//! Git Graph hands `View Diff` / `View File at this Revision` to the VS Code
//! editor (`src/utils.ts:418-490`). A terminal has no editor, so R3's tie-break
//! rule puts the **action + data** in M2 and the editor chrome out of scope: the
//! terminal equivalent is a modal, scrollable text pane that shows exactly what
//! `git` printed.
//!
//! The pager owns no git logic. [`crate::app::AppState::activate_menu`] runs the
//! A65–A68 plan through [`crate::actions::dispatch`] and hands the captured
//! stdout to [`Pager::new`], so the argv shown in the title is the argv git ran.
//!
//! Reference anchors (read-only `../vscode-git-graph`):
//!
//! * `src/utils.ts:418-443` — `viewDiff`: `git diff <from> <to> -- <path>` (the
//!   old side is `<from>^` when `from == to`).
//! * `src/utils.ts:476-490` — `viewFileAtRevision`: `git show <hash>:<path>`.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::term::ladder::Ladder;

/// A modal, scrollable view of captured command output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pager {
    /// What the pane is showing (`"git show abc1234:src/lib.rs"`).
    pub title: String,
    /// The captured stdout, one entry per line.
    pub lines: Vec<String>,
    /// First visible line.
    pub scroll: u16,
}

impl Pager {
    /// A pager for `text`, titled with the command that produced it.
    pub fn new(title: impl Into<String>, text: &str) -> Self {
        Pager {
            title: title.into(),
            lines: text.lines().map(str::to_string).collect(),
            scroll: 0,
        }
    }

    /// The number of captured lines.
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    /// The largest valid `scroll` for `viewport` visible rows.
    pub fn scroll_max(&self, viewport: u16) -> u16 {
        let viewport = viewport.max(1) as usize;
        self.lines.len().saturating_sub(viewport) as u16
    }

    /// Move by `delta` lines (positive = down), clamped to the text.
    pub fn scroll_by(&mut self, delta: i32, viewport: u16) {
        let next = (self.scroll as i32 + delta).clamp(0, self.scroll_max(viewport) as i32);
        self.scroll = next as u16;
    }

    /// Move by one viewport minus one row, `dir > 0` = down.
    pub fn page_by(&mut self, dir: i32, viewport: u16) {
        let page = viewport.max(1).saturating_sub(1).max(1) as i32;
        self.scroll_by(dir * page, viewport);
    }

    /// Jump to the first line.
    pub fn top(&mut self) {
        self.scroll = 0;
    }

    /// Jump to the last screenful.
    pub fn bottom(&mut self, viewport: u16) {
        self.scroll = self.scroll_max(viewport);
    }

    /// The one-line pane state, for the border title and `--dump` evidence.
    pub fn status_line(&self) -> String {
        let total = self.line_count();
        if total == 0 {
            return format!("{} — (no output)", self.title);
        }
        let percent = if total == 1 {
            100
        } else {
            let last = self.scroll_max(1).max(1) as usize;
            ((self.scroll as usize * 100) / last).min(100)
        };
        format!("{} — {percent}% ({total} lines)", self.title)
    }
}

/// The number of text rows the overlay shows for a `screen_height`-row screen.
///
/// The border costs two rows; the event loop and [`draw`] must agree on this
/// number so paging matches what is on screen.
pub fn viewport(screen_height: u16) -> u16 {
    screen_height.saturating_sub(2).max(1)
}

/// Draw the pager as a modal overlay covering `area`.
///
/// The caller renders it **last** (after the dialog), so while it is open it is
/// the top-most layer and the event loop routes every key to it. The frame and
/// the hint separator follow the ladder.
pub fn draw(frame: &mut Frame, area: Rect, pager: &Pager, ladder: Ladder) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    frame.render_widget(Clear, area);
    let hint = format!(
        " j/k scroll {} PgUp/PgDn page {} Esc close ",
        ladder.separator(),
        ladder.separator()
    );
    let block = Block::default()
        .borders(Borders::ALL)
        .border_set(ladder.border_set())
        .border_style(Style::default().fg(Color::Cyan))
        .title(format!(" {} ", pager.status_line()))
        .title_bottom(Line::from(hint).centered());
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let body = if pager.lines.is_empty() {
        vec![Line::styled(
            "(no output)",
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
        )]
    } else {
        pager
            .lines
            .iter()
            .map(|line| Line::raw(line.clone()))
            .collect()
    };
    frame.render_widget(Paragraph::new(body).scroll((pager.scroll, 0)), inner);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_splits_the_captured_text() {
        let pager = Pager::new("git show abc:src/lib.rs", "line one\nline two\n");
        assert_eq!(pager.lines, vec!["line one", "line two"]);
        assert_eq!(pager.line_count(), 2);
        assert_eq!(pager.scroll, 0);
    }

    #[test]
    fn scrolling_clamps_at_both_ends() {
        let mut pager = Pager::new("git diff", "1\n2\n3\n4\n5");
        pager.scroll_by(10, 3);
        assert_eq!(pager.scroll, 2, "max = 5 lines - 3 visible");
        pager.scroll_by(-10, 3);
        assert_eq!(pager.scroll, 0);
        pager.bottom(3);
        assert_eq!(pager.scroll, 2);
        pager.top();
        assert_eq!(pager.scroll, 0);
    }

    #[test]
    fn paging_moves_a_screenful_minus_one() {
        let mut pager = Pager::new("git diff", "1\n2\n3\n4\n5\n6\n7\n8");
        pager.page_by(1, 4);
        assert_eq!(pager.scroll, 3);
        pager.page_by(1, 4);
        assert_eq!(pager.scroll, 4, "clamped to 8 - 4");
        pager.page_by(-1, 4);
        assert_eq!(pager.scroll, 1);
    }

    #[test]
    fn an_empty_capture_still_has_a_status_line() {
        let pager = Pager::new("git show abc:gone", "");
        assert_eq!(pager.line_count(), 0);
        assert_eq!(pager.status_line(), "git show abc:gone — (no output)");
    }

    #[test]
    fn the_status_line_reports_the_percent() {
        let mut pager = Pager::new("git diff", "1\n2\n3");
        assert_eq!(pager.status_line(), "git diff — 0% (3 lines)");
        pager.bottom(1);
        assert_eq!(pager.status_line(), "git diff — 100% (3 lines)");
    }

    #[test]
    fn the_viewport_is_the_screen_minus_the_border() {
        assert_eq!(viewport(24), 22);
        assert_eq!(viewport(3), 1);
        assert_eq!(viewport(2), 1, "clamped so the overlay always shows a row");
        assert_eq!(viewport(0), 1);
    }
}
