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

use crate::app::ColumnState;
use crate::data::Commit;
use crate::find::FindWidget;
use crate::layout::types::{Lane, RowIdx};
use crate::render::cell::CELLS_PER_LANE;
use crate::render::{highlighted, FIND_CURRENT_BG};

/// Screen column where the text columns start — `2 * max_lane + 2`.
pub fn text_offset(max_lane: Lane) -> u16 {
    (CELLS_PER_LANE * (max_lane as usize + 1)) as u16
}

/// The table's fixed columns: the row number and the short hash. Everything
/// else is toggled from the column-header menu for W5/T3.
pub const FIXED_HEADERS: [&str; 2] = ["idx", "hash"];

/// The header labels for the current [`ColumnState`].
///
/// Always `idx`, `hash`, `refs`; `date`, `author` and `subject` only when the
/// corresponding column is visible.
pub fn headers(columns: &ColumnState) -> Vec<&'static str> {
    let mut out = vec![FIXED_HEADERS[0], FIXED_HEADERS[1]];
    if columns.date {
        out.push("date");
    }
    if columns.author {
        out.push("author");
    }
    if columns.commit {
        out.push("subject");
    }
    out.push("refs");
    out
}

/// Column widths matching [`headers`], in the same order.
pub fn constraints(columns: &ColumnState) -> Vec<ratatui::layout::Constraint> {
    use ratatui::layout::Constraint::{Length, Min};
    let mut out = vec![Length(5), Length(8)];
    if columns.date {
        out.push(Length(10));
    }
    if columns.author {
        out.push(Length(12));
    }
    if columns.commit {
        out.push(Min(12));
    }
    out.push(Length(20));
    out
}

/// Header row for the table.
///
/// The `idx` label is right-aligned so it sits over the right-aligned commit
/// numbers instead of drifting to the left edge of the 5-cell column.
pub fn header(columns: &ColumnState) -> TuiRow<'static> {
    let style = Style::default()
        .fg(Color::DarkGray)
        .add_modifier(Modifier::BOLD);
    TuiRow::new(headers(columns).into_iter().enumerate().map(|(i, h)| {
        let line = Line::styled(h, style);
        // Right-align only the label of the right-aligned `idx` column.
        TuiCell::from(if i == 0 {
            line.alignment(Alignment::Right)
        } else {
            line
        })
    }))
}

/// Build the commit table for a **display-row window**.
///
/// `scroll` is the first display row shown at the top of the body. `heights`
/// gives the display-row height of every commit row (M4-W12/T1: a block is
/// `refs + commit + details + diff`), and `leading` reserves the rows the inline
/// uncommitted section claims above the first commit (T4). `visible` is the row
/// order the active tab shows — the Branches filter drops rows it excludes.
///
/// The inline content itself is painted **over** these reserved rows by
/// [`crate::render::ui`], so this function only has to leave the right number of
/// blank spacer rows and keep table body line `d` aligned with graph display row
/// `d`. `selected` is an absolute commit index (not a display row). `columns`
/// carries the W5 column visibility toggles. `find` carries the W6 find widget
/// while it is visible, so matched characters are tinted and the current match's
/// row is filled.
pub fn commits_table_window(
    commits: &[Commit],
    heights: &[u16],
    visible: &[RowIdx],
    leading: u16,
    scroll: u16,
    selected: Option<usize>,
    columns: &ColumnState,
    find: Option<&FindWidget>,
) -> Table<'static> {
    let mut body: Vec<TuiRow<'static>> = Vec::new();
    let scroll = scroll as u32;
    let mut cursor: u32 = leading as u32;

    // The leading block is drawn over the table, so it only needs spacers.
    if cursor > scroll {
        for _ in 0..(cursor - scroll) {
            body.push(spacer_row());
        }
    }

    for &i in visible {
        let Some(commit) = commits.get(i as usize) else {
            continue;
        };
        let height = heights.get(i as usize).copied().unwrap_or(1).max(1) as u32;
        let top = cursor;
        cursor += height;
        if cursor <= scroll {
            continue; // entirely above the viewport
        }

        // The row's own line is only shown when its top is in view; if it is
        // scrolled halfway off, the visible part is spacer.
        if top >= scroll {
            body.push(commit_row(i as usize, commit, selected, columns, find));
        }
        if height > 1 {
            let first_gap = top + 1;
            let visible_from = first_gap.max(scroll);
            for _ in 0..(cursor - visible_from) {
                body.push(spacer_row());
            }
        }
    }

    Table::new(body, constraints(columns))
        .header(header(columns))
        .column_spacing(1)
}

/// One commit body row (absolute index `idx`).
fn commit_row(
    idx: usize,
    commit: &Commit,
    selected: Option<usize>,
    columns: &ColumnState,
    find: Option<&FindWidget>,
) -> TuiRow<'static> {
    let mut style = Style::default();
    if selected == Some(idx) {
        style = style.add_modifier(Modifier::BOLD | Modifier::REVERSED);
    } else if find.map(|find| find.row_is_current(idx)).unwrap_or(false) {
        // `tr.commit.findCurrentCommit` (`web/styles/main.css:665-668`).
        style = style.bg(FIND_CURRENT_BG);
    }
    let dim = Style::default().fg(Color::DarkGray);
    let hash = Style::default().fg(Color::Yellow);
    let date = Style::default().fg(Color::Green);
    let author = Style::default().fg(Color::Magenta);
    let refs = Style::default().fg(Color::Cyan);

    // The row-level tint above only covers the cells the table paints; every
    // text cell is built through `highlighted` so the match spans win.
    let row_tint = style;
    let mut cells = vec![
        TuiCell::from(highlighted(&format!("{:>5}", idx + 1), dim, None)),
        TuiCell::from(highlighted(&commit.abbrev, hash, find)),
    ];
    if columns.date {
        cells.push(TuiCell::from(highlighted(
            &short_date(commit.date),
            date,
            find,
        )));
    }
    if columns.author {
        cells.push(TuiCell::from(highlighted(&commit.author, author, find)));
    }
    if columns.commit {
        cells.push(TuiCell::from(highlighted(&commit.subject, row_tint, find)));
    }
    cells.push(TuiCell::from(highlighted(
        &commit.refs.join(", "),
        refs,
        find,
    )));
    let mut row = TuiRow::new(cells);
    if row_tint != Style::default() {
        row = row.style(row_tint);
    }
    row
}

/// A blank spacer row for the reserved expansion gap.
fn spacer_row() -> TuiRow<'static> {
    TuiRow::new(vec![TuiCell::from("")])
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

/// Format a UNIX timestamp as `YYYY-MM-DD HH:MM:SSZ` — the reference's
/// `formatLongDate` in ISO mode (`web/utils.ts:315-325`).
///
/// The reference renders in the browser's local timezone and appends a numeric
/// offset; a TUI has no local-timezone database without another dependency, so
/// this renders UTC and appends `Z`. The date and time fields are byte-for-byte
/// the reference's ISO format.
pub fn long_date(timestamp: i64) -> String {
    let days = timestamp.div_euclid(86_400);
    let seconds = timestamp.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let (hour, minute, second) = (seconds / 3600, (seconds % 3600) / 60, seconds % 60);
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02}Z")
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
    fn long_dates_render_the_reference_iso_format() {
        assert_eq!(long_date(0), "1970-01-01 00:00:00Z");
        assert_eq!(long_date(1_700_000_000), "2023-11-14 22:13:20Z");
        assert_eq!(long_date(86_399), "1970-01-01 23:59:59Z");
        assert_eq!(long_date(86_400), "1970-01-02 00:00:00Z");
    }

    #[test]
    fn header_names_all_five_columns() {
        let headers = headers(&ColumnState::default());
        assert_eq!(headers.len(), 6, "idx, hash, date, author, subject, refs");
        assert_eq!(headers[0], FIXED_HEADERS[0]);
        assert_eq!(headers[1], FIXED_HEADERS[1]);
        assert_eq!(headers[5], "refs");
    }

    #[test]
    fn hidden_columns_drop_their_header_and_width() {
        let mut columns = ColumnState {
            date: false,
            author: false,
            ..Default::default()
        };
        assert_eq!(headers(&columns), vec!["idx", "hash", "subject", "refs"]);
        assert_eq!(constraints(&columns).len(), 4);
        columns.commit = false;
        assert_eq!(headers(&columns), vec!["idx", "hash", "refs"]);
        assert_eq!(constraints(&columns).len(), 3);
    }

    /// A `heights` slice of `n` rows, all `1` except `expanded` (given `1 + extra`).
    fn flat_heights(n: usize, expanded: &[usize], extra: u16) -> Vec<u16> {
        (0..n)
            .map(|i| if expanded.contains(&i) { 1 + extra } else { 1 })
            .collect()
    }

    /// Every row index of `n` rows, in order.
    fn all_visible(n: usize) -> Vec<RowIdx> {
        (0..n as RowIdx).collect()
    }

    #[test]
    fn table_builds_for_a_slab_of_commits() {
        let rows = vec![
            commit("aaaaaaa", 0, "root", &[]),
            commit("bbbbbbb", 86_400, "tip", &["HEAD -> main"]),
        ];
        let heights = flat_heights(2, &[], 0);
        let visible = all_visible(2);
        let columns = ColumnState::default();
        let _ = commits_table_window(&rows, &heights, &visible, 0, 0, Some(1), &columns, None);
        let _ = commits_table_window(&rows, &heights, &visible, 0, 5, None, &columns, None);
        let _ = commits_table_window(&[], &[], &[], 0, 0, None, &columns, None);
    }

    #[test]
    fn leading_rows_reserve_the_uncommitted_block() {
        let rows = vec![commit("aaaaaaa", 0, "root", &[])];
        let heights = flat_heights(1, &[], 0);
        let visible = all_visible(1);
        let table = commits_table_window(
            &rows,
            &heights,
            &visible,
            3,
            0,
            None,
            &ColumnState::default(),
            None,
        );
        let rendered = render_table(table, 60, 8);
        let line_of = |needle: &str| -> usize {
            rendered
                .iter()
                .position(|line| line.contains(needle))
                .unwrap_or_else(|| panic!("missing {needle:?} in\n{rendered:#?}"))
        };
        // One header row, then three reserved leading rows, then the commit.
        assert_eq!(line_of("aaaaaaa"), 1 + 3);
    }

    /// Render a table into a `width` x `height` test terminal, line by line.
    fn render_table(table: Table<'static>, width: u16, height: u16) -> Vec<String> {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;
        let backend = TestBackend::new(width, height);
        let mut terminal =
            Terminal::new(backend).unwrap_or_else(|e: std::convert::Infallible| match e {});
        terminal
            .draw(|frame| frame.render_widget(table, frame.area()))
            .unwrap_or_else(|e: std::convert::Infallible| match e {});
        let buffer = terminal.backend().buffer();
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect()
    }

    #[test]
    fn expanded_rows_push_the_next_commit_down_in_the_table() {
        let rows = vec![
            commit("aaaaaaa", 0, "root", &[]),
            commit("bbbbbbb", 0, "mid", &[]),
            commit("ccccccc", 0, "tip", &[]),
        ];
        let visible = all_visible(3);

        let render = |expanded: &[usize]| -> Vec<String> {
            let heights = flat_heights(3, expanded, 4);
            let table = commits_table_window(
                &rows,
                &heights,
                &visible,
                0,
                0,
                Some(1),
                &ColumnState::default(),
                None,
            );
            render_table(table, 60, 12)
        };

        let collapsed = render(&[]);
        let with_gap = render(&[1]);

        let line_of = |lines: &[String], needle: &str| -> usize {
            lines
                .iter()
                .position(|line| line.contains(needle))
                .unwrap_or_else(|| panic!("missing {needle:?} in\n{lines:#?}"))
        };

        assert_eq!(line_of(&collapsed, "aaaaaaa"), 1);
        assert_eq!(line_of(&collapsed, "bbbbbbb"), 2);
        assert_eq!(line_of(&collapsed, "ccccccc"), 3);

        assert_eq!(line_of(&with_gap, "aaaaaaa"), 1);
        assert_eq!(line_of(&with_gap, "bbbbbbb"), 2);
        assert_eq!(
            line_of(&with_gap, "ccccccc"),
            2 + 1 + 4,
            "the expanded row reserves its four spacer rows"
        );
    }
}
