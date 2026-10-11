//! Rendering — **stub (P0 scaffold)**.
//!
//! Renders the commit list as plain text rows straight from [`crate::data::commits`]
//! plus the status bar, so the TUI is demonstrably alive in P0. W1 replaces the
//! text rows with the 2-cells-per-lane graph grid (`layout::Cell`, D8 `§3.1/§3.2`);
//! this module owns the grid ↔ text-column split and the glyph tables.

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use crate::App;

/// Draw the whole screen: commit list on top, one-line status bar at the bottom.
pub fn ui(frame: &mut Frame, app: &App, list_state: &mut ListState) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1), Constraint::Length(1)])
        .split(frame.area());

    commit_list(frame, chunks[0], app, list_state);
    status_bar(frame, chunks[1], app);
}

/// Text rows for the commit list (P0 stand-in for the graph grid).
pub fn commit_list(frame: &mut Frame, area: Rect, app: &App, list_state: &mut ListState) {
    let items: Vec<ListItem> = app
        .commits
        .iter()
        .enumerate()
        .map(|(i, commit)| {
            let mut spans = vec![
                Span::styled(
                    format!("{:>5} ", i + 1),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled(
                    format!("{} ", commit.abbrev),
                    Style::default().fg(Color::Yellow),
                ),
                Span::raw(commit.subject.clone()),
            ];
            if !commit.refs.is_empty() {
                spans.push(Span::styled(
                    format!("  ({})", commit.refs.join(", ")),
                    Style::default().fg(Color::Cyan),
                ));
            }
            ListItem::new(Line::from(spans))
        })
        .collect();

    let block = Block::default().borders(Borders::ALL).title(format!(
        " commits ({}) · branches ({}) — {} ",
        app.commits.len(),
        app.branches.len(),
        app.repo_label()
    ));
    let list = List::new(items)
        .block(block)
        .highlight_style(
            Style::default()
                .bg(Color::Blue)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("> ");

    frame.render_stateful_widget(list, area, list_state);
}

/// Status bar: branch, HEAD short hash, commit count, key hints.
pub fn status_bar(frame: &mut Frame, area: Rect, app: &App) {
    let line = if let Some(error) = &app.error {
        Line::from(Span::styled(
            format!(" git-graph-terminal — {error} "),
            Style::default().fg(Color::White).bg(Color::Red),
        ))
    } else {
        Line::from(vec![
            Span::styled(" branch: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                app.branch_name(),
                Style::default()
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("  head: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                app.head_short().to_string(),
                Style::default().fg(Color::Yellow),
            ),
            Span::styled("  commits: ", Style::default().fg(Color::DarkGray)),
            Span::styled(
                app.commits.len().to_string(),
                Style::default().fg(Color::Yellow),
            ),
            Span::styled(
                "  panes: j/k/↑/↓ scroll · r reload · q quit",
                Style::default().fg(Color::DarkGray),
            ),
        ])
    };

    frame.render_widget(
        Paragraph::new(line).style(Style::default().bg(Color::Black)),
        area,
    );
}
