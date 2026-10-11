//! Text columns for the commit list — `idx`, short hash, date, subject, refs.
//!
//! The graph occupies the first [`text_offset`] cells (`2 * max_lane + 2`, D8
//! §3.1); this table is rendered in the area to the right of it by
//! [`crate::render::ui`], so its first column starts exactly at that offset.
//! The table carries no block/border of its own — row `i` must line up with the
//! graph widget's row `i`.

use ratatui::layout::Alignment;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Cell as TuiCell, Row as TuiRow, Table};

use crate::data::Commit;
use crate::layout::types::Lane;
use crate::render::cell::CELLS_PER_LANE;

/// Screen column where the text columns start — `2 * max_lane + 2`.
pub fn text_offset(max_lane: Lane) -> u16 {
    (CELLS_PER_LANE * (max_lane as usize + 1)) as u16
}

/// Column headers, top to bottom of the text area.
pub const HEADERS: [&str; 5] = ["idx", "hash", "date", "subject", "refs"];

/// Header row for the table.
///
/// The `idx` label is right-aligned so it sits over the right-aligned commit
/// numbers instead of drifting to the left edge of the 5-cell column.
pub fn header() -> TuiRow<'static> {
    let style = Style::default()
        .fg(Color::DarkGray)
        .add_modifier(Modifier::BOLD);
    TuiRow::new(HEADERS.iter().enumerate().map(|(i, h)| {
        let line = Line::styled(*h, style);
        // Right-align only the label of the right-aligned `idx` column.
        TuiCell::from(if i == 0 {
            line.alignment(Alignment::Right)
        } else {
            line
        })
    }))
}

/// Build the commit table for `rows`, where `start_index` is the absolute index
/// of `rows[0]` in the full commit list (used for the `idx` column and to decide
/// which row is selected). `selected` is an absolute commit index.
pub fn commits_table(
    rows: &[Commit],
    start_index: usize,
    selected: Option<usize>,
) -> Table<'static> {
    let body: Vec<TuiRow<'static>> = rows
        .iter()
        .enumerate()
        .map(|(i, commit)| {
            let idx = start_index + i;
            let is_selected = selected == Some(idx);
            let mut style = Style::default();
            if is_selected {
                style = style.add_modifier(Modifier::BOLD | Modifier::REVERSED);
            }
            let dim = Style::default().fg(Color::DarkGray);
            let hash = Style::default().fg(Color::Yellow);
            let date = Style::default().fg(Color::Green);
            let refs = Style::default().fg(Color::Cyan);

            TuiRow::new(vec![
                TuiCell::from(Line::styled(format!("{:>5}", idx + 1), dim)),
                TuiCell::from(Line::styled(commit.abbrev.clone(), hash)),
                TuiCell::from(Line::styled(short_date(commit.date), date)),
                TuiCell::from(Line::styled(commit.subject.clone(), style)),
                TuiCell::from(Line::styled(commit.refs.join(", "), refs)),
            ])
        })
        .collect();

    Table::new(
        body,
        [
            ratatui::layout::Constraint::Length(5),
            ratatui::layout::Constraint::Length(8),
            ratatui::layout::Constraint::Length(10),
            ratatui::layout::Constraint::Min(12),
            ratatui::layout::Constraint::Length(20),
        ],
    )
    .header(header())
    .column_spacing(1)
}

/// Format a UNIX timestamp as an ISO-8601 UTC date (`YYYY-MM-DD`).
///
/// Civil-from-days (Howard Hinnant's algorithm) so no date crate is needed —
/// keeping the dependency set to ratatui + crossterm.
pub fn short_date(timestamp: i64) -> String {
    let days = timestamp.div_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}")
}

/// Days since 1970-01-01 → `(year, month, day)`.
fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let day = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let month = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    (if month <= 2 { year + 1 } else { year }, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commit(abbrev: &str, date: i64, subject: &str, refs: &[&str]) -> Commit {
        Commit {
            hash: abbrev.repeat(5),
            abbrev: abbrev.to_string(),
            parents: Vec::new(),
            author: "Ada".to_string(),
            date,
            subject: subject.to_string(),
            refs: refs.iter().map(|r| r.to_string()).collect(),
        }
    }

    #[test]
    fn text_offset_matches_the_contract() {
        assert_eq!(text_offset(0), 2);
        assert_eq!(text_offset(1), 4);
        assert_eq!(text_offset(3), 8);
    }

    #[test]
    fn dates_render_in_utc() {
        assert_eq!(short_date(0), "1970-01-01");
        assert_eq!(short_date(1_700_000_000), "2023-11-14");
        assert_eq!(short_date(1_000_000_000), "2001-09-09");
        assert_eq!(short_date(86_399), "1970-01-01");
        assert_eq!(short_date(86_400), "1970-01-02");
    }

    #[test]
    fn header_names_all_five_columns() {
        assert_eq!(HEADERS.len(), 5);
        assert_eq!(HEADERS[4], "refs");
    }

    #[test]
    fn table_builds_for_a_slab_of_commits() {
        let rows = vec![
            commit("aaaaaaa", 0, "root", &[]),
            commit("bbbbbbb", 86_400, "tip", &["HEAD -> main"]),
        ];
        let _ = commits_table(&rows, 0, Some(1));
        let _ = commits_table(&rows, 5, None);
        let _ = commits_table(&[], 0, None);
    }
}
