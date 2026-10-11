//! Rendering: the P0 text list, the graph view, and the status bar.
//!
//! Since P3 this module also hosts the **non-interactive graph renderer**
//! (lane b1): [`cell`] (cell grid + z-order), [`glyphs`] (Unicode/ASCII/Dumb
//! tables), [`graph_widget`] (the ratatui widget), [`columns`] (text columns)
//! and [`mock`] (the hand-built demo layout, used until a1's `layout()` lands).
//!
//! [`ViewOptions`] is the seam the graph renderer uses to plug a [`GraphLayout`]
//! in without touching the `data` contract:
//!
//! * `graph: None` → the P0 text list (`commit_list`), unchanged byte-for-byte.
//! * `graph: Some(layout)` → graph on the left, text columns on the right,
//!   both sharing display-row offsets so they stay aligned.
//!
//! W4 moves selection/scroll/expand ownership into [`crate::app::AppState`], so
//! the render functions read that instead of a `ratatui::ListState`. Expanded
//! rows reserve `EXPANDED_EXTRA_ROWS` blank terminal rows in **both** panes so
//! the graph row and the text row keep their indices.

pub mod cell;
pub mod columns;
pub mod glyphs;
pub mod graph_widget;
pub mod mock;

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use crate::app::{AppState, ColumnState, HitGeometry, EXPANDED_EXTRA_ROWS};
use crate::layout::types::{GraphLayout, RowIdx};
use crate::menu::ContextMenu;
use crate::render::columns::{commits_table_window, text_offset};
use crate::render::glyphs::GlyphTier;
use crate::render::graph_widget::GraphWidget;

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

/// Draw the whole frame: main area + status bar + any open context menu.
pub fn ui(frame: &mut Frame, state: &AppState, view: &ViewOptions<'_>) {
    let chunks = Layout::vertical([Constraint::Min(1), Constraint::Length(STATUS_HEIGHT)])
        .split(frame.area());
    match view.graph {
        Some(layout) => graph_view(frame, chunks[0], state, layout, view),
        None => commit_list(frame, chunks[0], state),
    }
    status_bar(frame, chunks[1], state, view);
    // The popup is drawn last so it covers both panes.
    if let Some(menu) = &state.menu {
        context_menu(frame, menu, frame.area());
    }
}

/// Geometry the mouse router needs, derived from the same numbers `ui` used.
pub fn hit_geometry(state: &AppState, view: &ViewOptions<'_>) -> Option<HitGeometry> {
    let layout = view.graph?;
    let width = state.status.width;
    if width == 0 {
        return None;
    }
    // `status.height` is the graph body: the screen minus the status bar and the
    // one-row table header. Add the header row back to get the main area.
    let main_height = state.status.height.saturating_add(1);
    Some(HitGeometry {
        body: Rect {
            x: 0,
            y: 0,
            width,
            height: main_height,
        },
        graph_width: text_offset(layout.max_lane).min(width),
        has_header: main_height > 1,
    })
}

/// Draw an open context menu as a bordered popup with a highlighted row.
///
/// Check/radio rows render a leading `✓`, disabled rows render dim, and every row
/// shows its inventory id (`A11`..`A73`) so a capture of the popup can be grepped
/// against the W5 inventory (see `the_popup_draws_the_surface_title_and_every_inventory_id`).
pub fn context_menu(frame: &mut Frame, menu: &ContextMenu, screen: Rect) {
    let rect = menu.rect(screen);
    frame.render_widget(Clear, rect);

    let chosen = Style::default()
        .bg(Color::Blue)
        .add_modifier(Modifier::BOLD);
    let off = Style::default().fg(Color::DarkGray);

    let lines: Vec<Line> = menu
        .items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let marker = if item.checked { "✓" } else { " " };
            let shortcut = item
                .shortcut
                .map(|key| format!("({key})"))
                .unwrap_or_default();
            let text = format!(" {marker} {} {:<34} {:>4} ", item.id, item.label, shortcut);
            let style = if !item.enabled {
                off
            } else if index == menu.selected {
                chosen
            } else {
                Style::default()
            };
            Line::styled(text, style)
        })
        .collect();

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan))
        .title(format!(" {} ", menu.surface.title()));
    frame.render_widget(Paragraph::new(lines).block(block), rect);
}

/// Draw the P0 text-only commit list (the original P0 view, block and all).
///
/// Kept byte-for-byte as P0 rendered it so the P0 `--dump` evidence still
/// reproduces; the graph view below replaces it as soon as a layout is available.
pub fn commit_list(frame: &mut Frame, area: Rect, state: &AppState) {
    let items: Vec<ListItem> = state
        .rows
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
        state.status.commits, state.status.branches, state.status.repo_label
    ));
    let list = List::new(items)
        .block(block)
        .highlight_style(
            Style::default()
                .bg(Color::Blue)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("> ");

    // The P0 view always highlights the first row; the pre-W4 loop did the same.
    let mut list_state = ListState::default();
    if !state.rows.is_empty() {
        list_state.select(Some(state.selected as usize));
    }
    frame.render_stateful_widget(list, area, &mut list_state);
}

/// Draw the graph on the left and the text columns on the right, sharing the
/// same **display-row** offset (`state.scroll`).
fn graph_view(
    frame: &mut Frame,
    area: Rect,
    state: &AppState,
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

    let selected = if state.rows.is_empty() {
        None
    } else {
        Some(state.selected)
    };

    frame.render_widget(
        GraphWidget {
            layout,
            tier: view.tier,
            selected,
            uncommitted: view.uncommitted,
            scroll: state.scroll,
            colour: view.colour,
            expanded: &state.expanded,
            detail_rows: EXPANDED_EXTRA_ROWS,
        },
        body,
    );

    frame.render_widget(
        commits_table_window(
            &state.rows,
            &state.expanded,
            EXPANDED_EXTRA_ROWS,
            state.scroll,
            selected.map(|row| row as usize),
            &state.columns,
        ),
        columns[1],
    );
}

/// The status bar, including graph lane width (feature b1-W1-3) and, in graph
/// mode, the W4 selection/expansion info.
///
/// The text-list variant is kept byte-for-byte so the P0 `--dump` reproduces.
pub fn status_bar(frame: &mut Frame, area: Rect, state: &AppState, view: &ViewOptions<'_>) {
    let dim = Style::default().fg(Color::DarkGray);
    let green = Style::default()
        .fg(Color::Green)
        .add_modifier(Modifier::BOLD);
    let yellow = Style::default().fg(Color::Yellow);
    let alert = Style::default().fg(Color::White).bg(Color::Red);

    let mut spans = Vec::new();
    if let Some(error) = &state.status.error {
        spans.push(Span::styled(
            format!(" git-graph-terminal — {error} "),
            alert,
        ));
    }
    spans.push(Span::styled(" repo: ", dim));
    spans.push(Span::raw(state.status.repo_label.clone()));
    spans.push(Span::styled("  branch: ", dim));
    spans.push(Span::styled(state.status.branch.clone(), green));
    spans.push(Span::styled("  head: ", dim));
    spans.push(Span::styled(state.status.head.clone(), yellow));
    spans.push(Span::styled("  commits: ", dim));
    spans.push(Span::styled(state.status.commits.to_string(), yellow));
    spans.push(Span::styled("  branches: ", dim));
    spans.push(Span::raw(state.status.branches.to_string()));

    if let Some(layout) = view.graph {
        // The last action comes first inside the graph block: the trailing
        // `panes:` hint is long enough to push anything after it off a normal
        // terminal, and a copy/confirm result must never be the thing that is cut.
        if let Some(last) = &state.status.last_action {
            spans.push(Span::styled("  last: ", dim));
            spans.push(Span::styled(last.clone(), yellow));
        }
        // W4 selection info: `sel: <1-based>/<total>` and the expanded count.
        let position = if state.rows.is_empty() {
            0
        } else {
            state.selected as usize + 1
        };
        spans.push(Span::styled("  sel: ", dim));
        spans.push(Span::raw(format!("{position}/{}", state.rows.len())));
        spans.push(Span::styled("  expanded: ", dim));
        spans.push(Span::raw(state.expanded.len().to_string()));
        spans.push(Span::styled("  lanes: ", dim));
        spans.push(Span::raw((layout.max_lane as usize + 1).to_string()));
        spans.push(Span::styled("  graph: ", dim));
        spans.push(Span::raw(format!("{} cols", graph_width(layout))));
        spans.push(Span::styled("  columns: ", dim));
        spans.push(Span::raw(column_summary(&state.columns)));
        if area.width < graph_width(layout) as u16 + MIN_TEXT_WIDTH {
            spans.push(Span::styled("  (narrow: text clipped)", dim));
        }
        spans.push(Span::styled(
            "  panes: j/k/↑/↓ · ←/→ branch · Enter expand · PgUp/PgDn · s/S stash · m/F10 menu · right-click menu · r reload · q quit",
            dim,
        ));
    } else {
        spans.push(Span::styled(
            "  panes: j/k/↑/↓ scroll · r reload · q quit",
            dim,
        ));
    }

    frame.render_widget(Paragraph::new(ratatui::text::Line::from(spans)), area);
}

/// Width of the graph area in columns for a layout.
fn graph_width(layout: &GraphLayout) -> usize {
    text_offset(layout.max_lane) as usize
}

/// Compact `date+author+subject · date-order` summary for the status bar.
fn column_summary(columns: &ColumnState) -> String {
    let mut shown = Vec::new();
    for (name, visible) in [
        ("date", columns.date),
        ("author", columns.author),
        ("commit", columns.commit),
    ] {
        if visible {
            shown.push(name);
        }
    }
    format!("{}+ · {}", shown.join("+"), columns.order.label())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::menu::{ContextMenu, MenuContext, MenuSurface};
    use crate::render::mock::demo_layout;

    #[test]
    fn graph_width_uses_the_contract_pitch() {
        assert_eq!(graph_width(&demo_layout()), 4);
    }

    #[test]
    fn text_only_view_is_the_default_fallback() {
        assert!(ViewOptions::TEXT_ONLY.graph.is_none());
    }

    /// Render a popup into a `TestBackend` and hand back both text and styles, so
    /// the assertions can talk about "the highlighted row" rather than pixels.
    fn render_menu(menu: &ContextMenu, width: u16, height: u16) -> (Vec<String>, Vec<Vec<Style>>) {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let backend = TestBackend::new(width, height);
        let mut terminal =
            Terminal::new(backend).unwrap_or_else(|e: std::convert::Infallible| match e {});
        terminal
            .draw(|frame| {
                let area = frame.area();
                context_menu(frame, menu, area);
            })
            .unwrap_or_else(|e: std::convert::Infallible| match e {});
        let buffer = terminal.backend().buffer();
        let text = (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol().to_string())
                    .collect()
            })
            .collect();
        let styles = (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].style())
                    .collect()
            })
            .collect();
        (text, styles)
    }

    #[test]
    fn the_popup_draws_the_surface_title_and_every_inventory_id() {
        let menu = ContextMenu::open(MenuSurface::CommitRow, 4, 1, MenuContext::default());
        let (lines, _) = render_menu(&menu, 80, 24);
        let joined = lines.join("\n");
        assert!(
            joined.contains("Commit"),
            "the title names the surface:\n{joined}"
        );
        for item in &menu.items {
            assert!(
                joined.contains(item.id),
                "row {} is on screen:\n{joined}",
                item.id
            );
        }
        // Shortcut letters are rendered too, so the popup is self-documenting.
        let first = menu.items[0].shortcut.expect("every enabled row has a key");
        assert!(
            joined.contains(&format!("({first})")),
            "shortcut shown:\n{joined}"
        );
    }

    #[test]
    fn the_popup_reflects_the_selection_and_disabled_rows() {
        let mut menu = ContextMenu::open(MenuSurface::ColumnHeader, 2, 1, MenuContext::default());
        menu.selected = 1;
        let (lines, styles) = render_menu(&menu, 80, 24);
        let rect = menu.rect(ratatui::layout::Rect::new(0, 0, 80, 24));
        // Border top + 1 = the first row; +1 again = the selected row.
        let selected_y = (rect.y + 2) as usize;
        let row_style = styles[selected_y][rect.x as usize + 2];
        assert_eq!(
            row_style.bg,
            Some(Color::Blue),
            "line: {}",
            lines[selected_y]
        );
        // The `✓` marker tracks the column state.
        assert!(
            lines[rect.y as usize + 2].contains('✓'),
            "checked row ticked"
        );
        assert!(
            lines[rect.y as usize + 1].contains("A60"),
            "ids are printed"
        );
    }

    #[test]
    fn the_popup_is_clamped_inside_the_screen() {
        let menu = ContextMenu::open(MenuSurface::CommitRow, 70, 20, MenuContext::default());
        let screen = ratatui::layout::Rect::new(0, 0, 80, 24);
        let rect = menu.rect(screen);
        assert!(rect.right() <= screen.right(), "{rect:?} fits horizontally");
        assert!(rect.bottom() <= screen.bottom(), "{rect:?} fits vertically");
        // A click inside the drawn box and the router agree on the bounds.
        assert!(menu.contains(rect.x, rect.y, screen));
        assert!(menu.contains(rect.right() - 1, rect.bottom() - 1, screen));
        assert!(!menu.contains(rect.right(), rect.bottom(), screen));
        // A tiny screen still yields a sane 1x1 minimum rather than a panic.
        let tiny = ratatui::layout::Rect::new(0, 0, 3, 2);
        assert_eq!(menu.rect(tiny).width, 3);
        assert!(menu.rect(tiny).height <= 2);
    }
}
