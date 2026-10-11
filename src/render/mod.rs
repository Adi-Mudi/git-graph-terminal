//! Rendering: the P0 text list, the graph view, and the status bar.
//!
//! Since P3 this module also hosts the **non-interactive graph renderer**
//! (lane b1): [`cell`] (cell grid + z-order), [`glyphs`] (Unicode/ASCII/Dumb
//! tables), [`graph_widget`] (the ratatui widget), [`columns`] (text columns)
//! and [`mock`] (the hand-built demo layout, used until a1's `layout()` lands).
//!
//! [`ViewOptions`] is the seam lane b1 uses to plug a [`GraphLayout`] in
//! without touching a1's `App`/`data` contract:
//!
//! * `graph: None` → the P0 text list (a1's `commit_list`), unchanged.
//! * `graph: Some(layout)` → graph on the left, text columns on the right,
//!   both sharing row indices so they stay aligned.

pub mod cell;
pub mod columns;
pub mod glyphs;
pub mod graph_widget;
pub mod mock;

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use crate::layout::types::{GraphLayout, RowIdx};
use crate::render::columns::{commits_table, text_offset};
use crate::render::glyphs::GlyphTier;
use crate::render::graph_widget::GraphWidget;
use crate::App;

/// One `│` row for the uncommitted entry and one for the status bar.
const STATUS_HEIGHT: u16 = 1;

/// Extra columns the text side needs before the graph is worth showing.
const MIN_TEXT_WIDTH: u16 = 24;

/// How to draw the main area.
#[derive(Debug, Clone, Copy)]
pub struct ViewOptions<'a> {
    /// The graph layout to draw on the left, or `None` for the text-only list.
    pub graph: Option<&'a GraphLayout>,
    /// Glyph capability tier (from `TERM`/locale/`NO_COLOR`).
    pub tier: GlyphTier,
    /// Emit colours?
    pub colour: bool,
    /// Row index of the uncommitted-changes entry, if any.
    pub uncommitted: Option<RowIdx>,
}

impl ViewOptions<'_> {
    /// The P0 text-only view (used when no layout is available).
    pub const TEXT_ONLY: ViewOptions<'static> = ViewOptions {
        graph: None,
        tier: GlyphTier::Unicode,
        colour: true,
        uncommitted: None,
    };
}

/// Draw the whole frame: main area + status bar.
pub fn ui(frame: &mut Frame, app: &App, list_state: &mut ListState, view: &ViewOptions<'_>) {
    let chunks = Layout::vertical([Constraint::Min(1), Constraint::Length(STATUS_HEIGHT)])
        .split(frame.area());
    match view.graph {
        Some(layout) => graph_view(frame, chunks[0], app, list_state, layout, view),
        None => commit_list(frame, chunks[0], app, list_state),
    }
    status_bar(frame, chunks[1], app, view);
}

/// Draw the P0 text-only commit list (the original P0 view, block and all).
///
/// Kept byte-for-byte as P0 rendered it so the P0 `--dump` evidence still
/// reproduces; the graph view below replaces it as soon as a layout is available.
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

/// Draw the graph on the left and the text columns on the right, sharing the
/// same row indices (graph row `i` ↔ table data row `i`).
fn graph_view(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    list_state: &mut ListState,
    layout: &GraphLayout,
    view: &ViewOptions<'_>,
) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    let graph_width = text_offset(layout.max_lane).min(area.width);
    let columns =
        Layout::horizontal([Constraint::Length(graph_width), Constraint::Min(0)]).split(area);

    // The table draws its own one-row header; the graph has no header, so it
    // starts one row lower to keep row i aligned with table data row i.
    let header = u16::from(area.height > 1);
    let body = Rect {
        x: columns[0].x,
        y: columns[0].y + header,
        width: columns[0].width,
        height: columns[0].height.saturating_sub(header),
    };

    let selected = list_state.selected();
    let scroll = scroll_for(selected, body.height as usize);

    frame.render_widget(
        GraphWidget {
            layout,
            tier: view.tier,
            selected: selected.map(|s| s as RowIdx),
            uncommitted: view.uncommitted,
            scroll,
            colour: view.colour,
        },
        body,
    );

    let start = scroll.min(app.commits.len());
    frame.render_widget(
        commits_table(&app.commits[start..], start, selected),
        columns[1],
    );
}

/// Keep the selected row visible, anchored to the bottom of the window.
fn scroll_for(selected: Option<usize>, visible: usize) -> usize {
    match selected {
        Some(row) if visible > 0 && row >= visible => row + 1 - visible,
        _ => 0,
    }
}

/// The status bar, including graph lane width (feature b1-W1-3).
pub fn status_bar(frame: &mut Frame, area: Rect, app: &App, view: &ViewOptions<'_>) {
    let dim = Style::default().fg(Color::DarkGray);
    let green = Style::default()
        .fg(Color::Green)
        .add_modifier(Modifier::BOLD);
    let yellow = Style::default().fg(Color::Yellow);
    let alert = Style::default().fg(Color::White).bg(Color::Red);

    let mut spans = Vec::new();
    if let Some(error) = &app.error {
        spans.push(Span::styled(
            format!(" git-graph-terminal — {error} "),
            alert,
        ));
    }
    spans.push(Span::styled(" repo: ", dim));
    spans.push(Span::raw(app.repo_label()));
    spans.push(Span::styled("  branch: ", dim));
    spans.push(Span::styled(app.branch_name().to_string(), green));
    spans.push(Span::styled("  head: ", dim));
    spans.push(Span::styled(app.head_short().to_string(), yellow));
    spans.push(Span::styled("  commits: ", dim));
    spans.push(Span::styled(app.commits.len().to_string(), yellow));
    spans.push(Span::styled("  branches: ", dim));
    spans.push(Span::raw(app.branches.len().to_string()));

    if let Some(layout) = view.graph {
        spans.push(Span::styled("  lanes: ", dim));
        spans.push(Span::raw((layout.max_lane as usize + 1).to_string()));
        spans.push(Span::styled("  graph: ", dim));
        spans.push(Span::raw(format!("{} cols", graph_width(layout))));
        if area.width < graph_width(layout) as u16 + MIN_TEXT_WIDTH {
            spans.push(Span::styled("  (narrow: text clipped)", dim));
        }
    }
    spans.push(Span::styled(
        "  panes: j/k/↑/↓ scroll · r reload · q quit",
        dim,
    ));

    frame.render_widget(Paragraph::new(ratatui::text::Line::from(spans)), area);
}

/// Width of the graph area in columns for a layout.
fn graph_width(layout: &GraphLayout) -> usize {
    text_offset(layout.max_lane) as usize
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::mock::demo_layout;

    #[test]
    fn scroll_keeps_the_selection_visible() {
        assert_eq!(scroll_for(None, 10), 0);
        assert_eq!(scroll_for(Some(0), 10), 0);
        assert_eq!(scroll_for(Some(9), 10), 0);
        assert_eq!(scroll_for(Some(10), 10), 1);
        assert_eq!(scroll_for(Some(25), 10), 16);
        assert_eq!(scroll_for(Some(3), 0), 0);
    }

    #[test]
    fn graph_width_uses_the_contract_pitch() {
        assert_eq!(graph_width(&demo_layout()), 4);
    }

    #[test]
    fn text_only_view_is_the_default_fallback() {
        assert!(ViewOptions::TEXT_ONLY.graph.is_none());
    }
}
