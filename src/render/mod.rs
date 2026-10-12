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
//!
//! W6 adds three overlays, each drawn after the pane it covers:
//!
//! * [`find_bar`] — the find widget, pinned to the **top right** of the window
//!   exactly like `#findWidget` (`web/styles/findWidget.css:1-5`: `position:
//!   fixed; top: 0; right: 28px`). It also tints the matched characters of every
//!   visible cell ([`highlighted`]) and the current match's row.
//! * [`crate::cdv::draw`] — the commit details view, docked to the **bottom**
//!   (`#cdv.docked`, `web/styles/main.css:309-320`).
//! * [`dialog_popup`] — the modal confirmation dialog, centred
//!   (`.dialog`, `web/styles/dialog.css:1-16`).
//!
//! Overlays are drawn in the order the reference stacks them: CDV (z-index 20),
//! find widget (21), dialog (41), context menu (51).
//!
//! W8 adds the **capability ladder**: [`ViewOptions::ladder`] carries the
//! terminal's proven colour/glyph/frame rungs (built by [`crate::term`]), and
//! every drawing routine degrades through it. This module never reads the
//! environment.

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
use crate::cdv;
use crate::dialog::Dialog;
use crate::find::FindWidget;
use crate::layout::types::{GraphLayout, RowIdx};
use crate::menu::ContextMenu;
use crate::pager;
use crate::render::columns::{commits_table_window, text_offset};
use crate::render::graph_widget::GraphWidget;
use crate::term::ladder::{self, Dir, Ladder};

/// One `│` row for the uncommitted entry and one for the status bar.
const STATUS_HEIGHT: u16 = 1;

/// Height of the find bar: a bordered box with one content row.
///
/// `#findWidget` is 34 px tall (`web/styles/findWidget.css:5`), which is two
/// terminal rows; the border costs one more.
pub const FIND_BAR_HEIGHT: u16 = 3;

/// `--vscode-editor-findMatchHighlightBackground`: the tint behind a matched
/// character (`findWidget.ts:1959`, applied to the `findMatch` spans).
const FIND_MATCH_BG: Color = Color::Indexed(226);
/// The foreground that stays readable on [`FIND_MATCH_BG`].
const FIND_MATCH_FG: Color = Color::Black;
/// `--git-graph-findMatchCommit`: the tint of the **current** match's row
/// (`web/styles/main.css:665-668`).
pub(crate) const FIND_CURRENT_BG: Color = Color::Indexed(58);

/// Extra columns the text side needs before the graph is worth showing.
const MIN_TEXT_WIDTH: u16 = 24;

/// How to draw the main area.
#[derive(Debug, Clone, Copy)]
pub struct ViewOptions<'a> {
    /// The graph layout to draw on the left, or `None` for the text-only list.
    pub graph: Option<&'a GraphLayout>,
    /// Terminal capability ladder (colour + glyph + frame rungs) from
    /// [`crate::term`]. The renderer consumes this and never raw env.
    pub ladder: Ladder,
    /// Row index of the uncommitted-changes entry, if any.
    pub uncommitted: Option<RowIdx>,
}

impl ViewOptions<'_> {
    /// The P0 text-only view (used when no layout is available).
    pub const TEXT_ONLY: ViewOptions<'static> = ViewOptions {
        graph: None,
        ladder: Ladder::unlimited(),
        uncommitted: None,
    };
}

/// Draw the whole frame: main area + status bar + any open context menu.
pub fn ui(frame: &mut Frame, state: &AppState, view: &ViewOptions<'_>) {
    let rows = overlay_rows(state, frame.area().height);
    let chunks = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(rows.cdv),
        Constraint::Length(rows.find),
        Constraint::Length(STATUS_HEIGHT),
    ])
    .split(frame.area());
    match view.graph {
        Some(layout) => graph_view(frame, chunks[0], state, layout, view),
        None => commit_list(frame, chunks[0], state),
    }
    // The docked CDV sits directly above the status bar (`#cdv.docked`).
    if let Some(panel) = &state.cdv {
        cdv::draw(frame, chunks[1], panel, state.cdv_focused, view.ladder);
    }
    if state.find.visible {
        find_bar(frame, chunks[2], &state.find, view.ladder);
    }
    status_bar(frame, chunks[3], state, view);
    // A modal dialog blocks the whole window; the context menu still stacks on
    // top of it so the two never fight over the same rows.
    if let Some(dialog) = &state.dialog {
        dialog_popup(frame, dialog, frame.area(), view.ladder);
    }
    if let Some(menu) = &state.menu {
        context_menu(frame, menu, frame.area(), view.ladder);
    }
    // The pager is the top-most layer: it covers the whole window, including the
    // status bar, and the event loop routes every key to it while it is open.
    if let Some(pager) = &state.pager {
        pager::draw(frame, frame.area(), pager, view.ladder);
    }
}

/// Rows the W6 overlays take out of the commit body.
///
/// `ui` and the event loop must agree on this number: `ui` lays the overlays out
/// with it and [`crate::app::AppState::overlay_rows`] reports it so
/// `status.height` (the viewport used by paging and by the follow-the-selection
/// scroll) is the rows that are actually visible.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct OverlayRows {
    /// The docked commit details view, if open.
    pub cdv: u16,
    /// The find bar, if visible.
    pub find: u16,
}

/// How many rows the overlays claim for a screen of `screen_height` rows.
pub fn overlay_rows(state: &AppState, screen_height: u16) -> OverlayRows {
    OverlayRows {
        cdv: state
            .cdv
            .as_ref()
            .map(|panel| cdv::panel_height(panel, screen_height, STATUS_HEIGHT))
            .unwrap_or(0),
        find: if state.find.visible {
            FIND_BAR_HEIGHT
        } else {
            0
        },
    }
}

/// The find widget's bar, pinned to the top right of `area`.
///
/// Width is clamped so a narrow terminal still shows the box rather than
/// clipping it off the left edge. The frame and the cursor block follow the
/// ladder, so an ASCII-only terminal gets `+-|` and `_`.
pub fn find_bar(frame: &mut Frame, area: Rect, find: &FindWidget, ladder: Ladder) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    let width = area.width.min(64).max(area.width.min(24));
    let rect = Rect {
        x: area.width.saturating_sub(width),
        y: 0,
        width,
        height: area.height.min(FIND_BAR_HEIGHT),
    };
    frame.render_widget(Clear, rect);

    let border = if find.error.is_some() {
        Style::default().fg(Color::Red)
    } else {
        Style::default().fg(Color::Cyan)
    };
    let title = format!(" {} ", find.status_line());
    let block = Block::default()
        .borders(Borders::ALL)
        .border_set(ladder.border_set())
        .border_style(border)
        .title(title);
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    if inner.height == 0 {
        return;
    }

    // The reference draws the toggles as `Aa` / `.*` / `cdv` buttons
    // (`#findCaseSensitive`, `#findIsRegex`, `#findOpenCommitDetailsView`); a
    // terminal shows the same flags as bracketed markers plus a caret.
    let mut spans = vec![Span::raw(format!("{}{}", find.text, ladder.caret()))];
    if find.case_sensitive {
        spans.push(Span::styled(" [Aa]", Style::default().fg(Color::Yellow)));
    }
    if find.is_regex {
        spans.push(Span::styled(" [.*]", Style::default().fg(Color::Yellow)));
    }
    if find.open_cdv {
        spans.push(Span::styled(" [cdv]", Style::default().fg(Color::Yellow)));
    }
    if let Some(error) = &find.error {
        spans.push(Span::styled(
            format!(" {error}"),
            Style::default().fg(Color::Red),
        ));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), inner);
}

/// The modal dialog, centred like `.dialog` (`web/styles/dialog.css:1-16`).
pub fn dialog_popup(frame: &mut Frame, dialog: &Dialog, screen: Rect, ladder: Ladder) {
    if screen.height == 0 || screen.width == 0 {
        return;
    }
    let rows = dialog_rows(dialog, ladder) as u16;
    let height = rows.min(screen.height);
    let width = screen.width.min(64).max(screen.width.min(30));
    let rect = Rect {
        x: screen.width.saturating_sub(width) / 2,
        y: screen.height.saturating_sub(height) / 2,
        width,
        height,
    };
    frame.render_widget(Clear, rect);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_set(ladder.border_set())
        .border_style(Style::default().fg(Color::Cyan))
        .title(format!(" {} ", dialog.title));
    let inner = block.inner(rect);
    frame.render_widget(block, rect);

    let lines = dialog_lines(dialog, ladder);
    frame.render_widget(Paragraph::new(lines), inner);
}

/// How many terminal rows [`dialog_popup`] needs (body + fields + buttons + border).
fn dialog_rows(dialog: &Dialog, ladder: Ladder) -> usize {
    dialog_lines(dialog, ladder).len().saturating_add(2).max(3)
}

/// The dialog's body, field list and button strip, in draw order.
///
/// Kept separate from the widget so the layout maths and the text can both be
/// asserted in tests without a terminal. The focused-field pointer follows the
/// ladder (`▸` → `>`).
fn dialog_lines(dialog: &Dialog, ladder: Ladder) -> Vec<Line<'static>> {
    let dim = Style::default().fg(Color::DarkGray);
    let mut lines: Vec<Line<'static>> = Vec::new();
    for body in &dialog.body {
        lines.push(Line::raw(body.clone()));
    }
    for (index, field) in dialog.fields.iter().enumerate() {
        let focusable = dialog.focused_field().map(|f| f.name.as_str()) == Some(&field.name);
        let marker = if focusable { ladder.pointer() } else { ' ' };
        let style = if focusable {
            Style::default().fg(Color::Yellow)
        } else {
            Style::default()
        };
        lines.push(Line::from(vec![
            Span::raw(format!(" {marker} {}: ", field.label)),
            Span::styled(field.display_value(), style),
        ]));
        let _ = index;
    }
    if !dialog.fields.is_empty() {
        lines.push(Line::raw(String::new()));
    }
    let buttons: Vec<Span> = dialog
        .buttons
        .iter()
        .enumerate()
        .map(|(index, label)| {
            let text = format!(" [ {label} ] ");
            if dialog.focused_button() == Some(index) {
                Span::styled(
                    text,
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                Span::styled(text, dim)
            }
        })
        .collect();
    lines.push(Line::from(buttons));
    if let Some(error) = &dialog.error {
        lines.push(Line::styled(error.clone(), Style::default().fg(Color::Red)));
    }
    lines
}

/// `text` with every find match tinted, as one line.
///
/// [`FindWidget::ranges_in`] returns **character** offsets (see the module docs
/// of [`crate::find`]), so the split happens on `chars()`, never on byte
/// indices. `find: None` and an empty pattern both return the plain line, so
/// the pre-W6 rendering is byte-for-byte unchanged when the widget is hidden.
pub(crate) fn highlighted(text: &str, style: Style, find: Option<&FindWidget>) -> Line<'static> {
    let ranges = match find {
        Some(find) if !find.text.is_empty() => find.ranges_in(text),
        _ => Vec::new(),
    };
    if ranges.is_empty() {
        return Line::styled(text.to_string(), style);
    }
    let chars: Vec<char> = text.chars().collect();
    let match_style = Style::default().bg(FIND_MATCH_BG).fg(FIND_MATCH_FG);
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut cursor = 0usize;
    for (start, end) in ranges {
        let start = start.min(chars.len());
        let end = end.min(chars.len()).max(start);
        if start > cursor {
            spans.push(Span::styled(
                chars[cursor..start].iter().collect::<String>(),
                style,
            ));
        }
        spans.push(Span::styled(
            chars[start..end].iter().collect::<String>(),
            match_style,
        ));
        cursor = end;
    }
    if cursor < chars.len() {
        spans.push(Span::styled(
            chars[cursor..].iter().collect::<String>(),
            style,
        ));
    }
    Line::from(spans)
}

/// Geometry the mouse router needs, derived from the same numbers `ui` used.
pub fn hit_geometry(
    state: &AppState,
    view: &ViewOptions<'_>,
    screen_height: u16,
) -> Option<HitGeometry> {
    let layout = view.graph?;
    let width = state.status.width;
    if width == 0 {
        return None;
    }
    // `ui` stacks the docked CDV, the find bar and the status bar under the main
    // area, so the main area is the screen minus exactly those rows. Deriving it
    // from `overlay_rows` keeps the router and the renderer on one formula —
    // `status.height` alone does not know about the overlays (W10).
    let overlays = overlay_rows(state, screen_height);
    let main_height = screen_height
        .saturating_sub(overlays.cdv)
        .saturating_sub(overlays.find)
        .saturating_sub(STATUS_HEIGHT);
    let cdv = (overlays.cdv > 0).then_some(Rect {
        x: 0,
        y: main_height,
        width,
        height: overlays.cdv,
    });
    Some(HitGeometry {
        body: Rect {
            x: 0,
            y: 0,
            width,
            height: main_height,
        },
        graph_width: text_offset(layout.max_lane).min(width),
        has_header: main_height > 1,
        cdv,
    })
}

/// Draw an open context menu as a bordered popup with a highlighted row.
///
/// Check/radio rows render a leading `✓` (ASCII `x` on a degraded ladder),
/// disabled rows render dim, and every row shows its inventory id (`A11`..`A73`)
/// so a capture of the popup can be grepped against the W5 inventory (see
/// `the_popup_draws_the_surface_title_and_every_inventory_id`). Labels are padded
/// by **display cells**, so a CJK label cannot shift the id/shortcut columns.
pub fn context_menu(frame: &mut Frame, menu: &ContextMenu, screen: Rect, ladder: Ladder) {
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
            let marker = if item.checked { ladder.checked() } else { ' ' };
            let shortcut = item
                .shortcut
                .map(|key| format!("({key})"))
                .unwrap_or_default();
            let label = ladder::pad_cells(item.label, 34);
            let text = format!(" {marker} {} {label} {:>4} ", item.id, shortcut);
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
        .border_set(ladder.border_set())
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
            ladder: view.ladder,
            selected,
            uncommitted: view.uncommitted,
            scroll: state.scroll,
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
            state.find.visible.then_some(&state.find),
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
    // W8: the capability ladder this frame was drawn with. Kept high in the line
    // because the trailing hints are long enough to run off a normal terminal,
    // and a pty capture must always show which rung the host got.
    spans.push(Span::styled("  caps: ", dim));
    spans.push(Span::raw(view.ladder.label()));

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
        let up = view.ladder.arrow(Dir::Up);
        let down = view.ladder.arrow(Dir::Down);
        let left = view.ladder.arrow(Dir::Left);
        let right = view.ladder.arrow(Dir::Right);
        let dot = view.ladder.separator();
        spans.push(Span::styled(
            format!(
                "  panes: j/k/{up}/{down} {dot} {left}/{right} branch {dot} Enter expand {dot} PgUp/PgDn {dot} s/S stash {dot} m/F10 menu {dot} right-click menu {dot} r reload {dot} q quit"
            ),
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
                context_menu(frame, menu, area, Ladder::unlimited());
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
