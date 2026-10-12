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
//! * [`crate::cdv::draw`] — the commit details view, drawn inline under its own
//!   commit row (`#cdv.inline`, `web/styles/main.css:299-302`) rather than
//!   docked.
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

use crate::app::{AppState, ColumnState, DiffSource, HitGeometry, Tab};
use crate::branches::{BranchEntry, RefKind};
use crate::cdv::{self, CdvState};
use crate::dialog::Dialog;
use crate::diff;
use crate::find::FindWidget;
use crate::layout::types::{GraphLayout, RowIdx};
use crate::menu::ContextMenu;
use crate::pager;
use crate::render::columns::{commits_table_window, text_offset, TableWindow};
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

/// Height of the tab bar: one row of `Overview | Branches` (M4-W12/T1).
pub const TAB_BAR_HEIGHT: u16 = 1;

/// Width of the Branches tab's left pane, in columns (M4-W12/T2).
///
/// The design gives the branch list a fixed left column (`.split`'s `270px`).
/// Narrow terminals drop the pane rather than squeeze the history — the filter
/// keys (`[` / `]` / `b`) still work, so no capability is lost.
pub const BRANCH_PANE_WIDTH: u16 = 30;

/// Below this body width the Branches tab shows the history alone.
const BRANCH_PANE_MIN_BODY: u16 = 56;

/// The x-ranges of the tab bar's clickable labels, in `Tab::ALL` order.
///
/// [`tab_bar`] renders `│ 1 Overview │ 2 Branches `; this walks the same format
/// strings so the mouse router and the renderer cannot drift apart (T1/T7).
/// The leading `│ ` prefix is included, so the ranges are screen columns.
pub fn tab_spans() -> Vec<(Tab, std::ops::Range<u16>)> {
    let mut spans = Vec::new();
    // The leading `│ ` separator the bar starts with.
    let mut x = 2u16;
    for index in 0.. {
        let Some(tab) = Tab::from_index(index) else {
            break;
        };
        let width = format!(" {} {} ", tab.index() + 1, tab.label())
            .chars()
            .count() as u16;
        spans.push((tab, x..x + width));
        // The ` │ ` separator before the next label.
        x += width + 3;
    }
    spans
}

/// The tab whose label covers screen column `x`, or `None` (T7).
pub fn tab_at_x(x: u16) -> Option<Tab> {
    tab_spans()
        .into_iter()
        .find(|(_, span)| span.contains(&x))
        .map(|(tab, _)| tab)
}

/// The width the Branches tab's left pane takes out of `body_width` (T2).
///
/// [`ui`], [`graph_view`] and [`hit_geometry`] all call this, so the drawn pane
/// and the mouse router can never disagree.
pub fn branch_pane_width(tab: Tab, body_width: u16) -> u16 {
    if tab != Tab::Branches || body_width < BRANCH_PANE_MIN_BODY {
        return 0;
    }
    BRANCH_PANE_WIDTH.min(body_width / 2)
}

/// What an [`InlineBlock`] is, and how to draw it.
pub enum BlockKind<'a> {
    /// Plain lines, no frame — the refs line and the uncommitted section.
    Plain,
    /// A framed panel with a title — the inline diff (M4-W12/T5).
    Framed(String),
    /// The commit-details panel, drawn by [`cdv::draw`] so the panel's own
    /// border, title and scroll logic stays the single implementation.
    Cdv(&'a CdvState),
}

/// One block of content drawn **inline** in the body, over the commit table.
///
/// M4-W12 replaces the docked bottom panel with blocks that sit directly under
/// the row they belong to: the refs line above a commit (T7), the commit-details
/// panel (T3) and the diff of the selected file (T5), plus the uncommitted
/// section above the history (T4).
///
/// `commits_table_window` reserves the display rows each block occupies (see
/// [`crate::app::AppState::block_parts`]); this type carries the content that
/// paints them.
pub struct InlineBlock<'a> {
    /// First display row of the block, in body-relative coordinates.
    pub top: u16,
    /// The content lines (`Plain` / `Framed` only).
    pub lines: Vec<Line<'static>>,
    /// How to draw the block.
    pub kind: BlockKind<'a>,
    /// Highlight the block's first line as part of the selected row.
    pub selected: bool,
}

/// Every inline block the current state wants painted, in draw order.
fn inline_blocks(state: &AppState, width: u16) -> Vec<InlineBlock<'_>> {
    let mut blocks: Vec<InlineBlock<'_>> = Vec::new();

    // T4 — the uncommitted section sits above the first commit.
    if state.tab == Tab::Overview && !state.uncommitted.is_empty() {
        let mut lines = vec![Line::styled(
            format!(" {} ", state.uncommitted.title(state.uncommitted_open)),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )];
        lines.push(Line::styled(
            state.uncommitted.summary_line(),
            Style::default().fg(Color::DarkGray),
        ));
        if state.uncommitted_open {
            let total = state.uncommitted.files.len();
            for (index, file) in state.uncommitted.files.iter().enumerate() {
                lines.push(uncommitted_file_line(
                    file,
                    index,
                    total,
                    state.uncommitted_selected == Some(index),
                ));
            }
        }
        blocks.push(InlineBlock {
            top: 0,
            lines,
            kind: BlockKind::Plain,
            selected: false,
        });
    }

    // One pass over the visible rows: each block's top is the running cursor.
    let mut cursor = state.uncommitted_height();
    for row in state.visible_rows() {
        let parts = state.block_parts(row);
        let top = cursor;
        cursor = cursor.saturating_add(parts.total);

        // T7 — refs get their own line above the commit.
        if parts.refs > 0 {
            let refs = state
                .rows
                .get(row as usize)
                .map(|commit| commit.refs.join(" · "))
                .unwrap_or_default();
            blocks.push(InlineBlock {
                top,
                lines: vec![Line::styled(
                    format!("┌ refs │ {refs}"),
                    Style::default().fg(Color::Cyan),
                )],
                kind: BlockKind::Plain,
                selected: state.selected == row && state.uncommitted_selected.is_none(),
            });
        }

        // T3 — the commit-details panel, inline under its row.
        if parts.cdv > 0 {
            if let Some(panel) = &state.cdv {
                if panel.row == row {
                    blocks.push(InlineBlock {
                        top: top.saturating_add(parts.refs).saturating_add(parts.commit),
                        lines: Vec::new(),
                        kind: BlockKind::Cdv(panel),
                        selected: false,
                    });
                }
            }
        }

        // T5 — the selected file's diff, inline under the details panel.
        if parts.diff > 0 {
            if let Some(diff) = &state.inline_diff {
                if diff.row() == Some(row) {
                    blocks.push(InlineBlock {
                        top: top
                            .saturating_add(parts.refs)
                            .saturating_add(parts.commit)
                            .saturating_add(parts.cdv),
                        lines: diff_lines(&diff.lines, width),
                        kind: BlockKind::Framed(diff.title.clone()),
                        selected: false,
                    });
                }
            }
        }
    }

    // T4 — the uncommitted file's diff, under the uncommitted section.
    if state.tab == Tab::Overview {
        if let Some(diff) = &state.inline_diff {
            if matches!(diff.source, DiffSource::Uncommitted(_)) {
                let top = state
                    .uncommitted_height()
                    .saturating_sub(diff.lines.len() as u16 + 1);
                blocks.push(InlineBlock {
                    top,
                    lines: diff_lines(&diff.lines, width),
                    kind: BlockKind::Framed(diff.title.clone()),
                    selected: false,
                });
            }
        }
    }

    blocks
}

/// One uncommitted file row: badge, path, counts, stage label (T4).
/// The tree glyph that opens an uncommitted file row (`┌─`, `├─`, `└─`).
///
/// The mockup hand-writes one glyph per row: `┌─` on the first, `└─` on the
/// last, `├─` in between. A one-file section is both first and last, so it gets
/// `└─`.
fn tree_glyph(index: usize, total: usize) -> &'static str {
    if total <= 1 || index + 1 >= total {
        "└─"
    } else if index == 0 {
        "┌─"
    } else {
        "├─"
    }
}

/// One uncommitted file row: `  ┌─ A src/main.ts  staged  +12 −0`.
///
/// The mockup's order is tree glyph, status letter, path, then the staging
/// marker and the counts. The counts are printed even when both are `0` — the
/// mockup prints `+0 −0` rather than dropping the field.
fn uncommitted_file_line(
    file: &crate::uncommitted::UncommittedFile,
    index: usize,
    total: usize,
    selected: bool,
) -> Line<'static> {
    let mut style = Style::default();
    if selected {
        style = style.add_modifier(Modifier::BOLD | Modifier::REVERSED);
    }
    let letter = match file.status {
        crate::files::GitFileStatus::Added => Color::Green,
        crate::files::GitFileStatus::Modified => Color::Yellow,
        crate::files::GitFileStatus::Deleted => Color::Red,
        crate::files::GitFileStatus::Renamed => Color::Cyan,
        crate::files::GitFileStatus::Untracked => Color::Magenta,
    };
    let dim = Style::default().fg(Color::DarkGray);
    Line::from(vec![
        Span::styled(format!("  {} ", tree_glyph(index, total)), dim),
        Span::styled(format!("{} ", file.letter()), Style::default().fg(letter)),
        Span::styled(file.path.clone(), style),
        Span::styled(format!("  {}", file.stage_label()), dim),
        Span::styled(
            format!("  +{} \u{2212}{}", file.additions, file.deletions),
            dim,
        ),
    ])
}

/// Turn parsed diff lines into renderable lines.
fn diff_lines(lines: &[diff::DiffLine], width: u16) -> Vec<Line<'static>> {
    lines
        .iter()
        .map(|line| diff::styled_line(line, width))
        .collect()
}

/// Draw the whole frame: tab bar + body + status bar + any open overlay.
pub fn ui(frame: &mut Frame, state: &AppState, view: &ViewOptions<'_>) {
    let rows = overlay_rows(state, frame.area().height);
    let chunks = Layout::vertical([
        Constraint::Length(TAB_BAR_HEIGHT),
        Constraint::Min(1),
        Constraint::Length(rows.find),
        Constraint::Length(STATUS_HEIGHT),
    ])
    .split(frame.area());
    tab_bar(frame, chunks[0], state.tab, view.ladder);
    match view.graph {
        Some(layout) => {
            let pane = branch_pane_width(state.tab, chunks[1].width);
            if pane == 0 {
                graph_view(frame, chunks[1], state, layout, view);
            } else {
                let split = Layout::horizontal([Constraint::Length(pane), Constraint::Min(0)])
                    .split(chunks[1]);
                branch_pane(frame, split[0], state, view.ladder);
                graph_view(frame, split[1], state, layout, view);
            }
        }
        None => commit_list(frame, chunks[1], state),
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

/// The tab bar: `Overview | Branches`, with the active tab highlighted (T1).
pub fn tab_bar(frame: &mut Frame, area: Rect, active: Tab, ladder: Ladder) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    let bar = ladder.glyphs().vertical;
    let mut spans: Vec<Span<'static>> = vec![Span::styled(
        format!("{bar} "),
        Style::default().fg(Color::DarkGray),
    )];
    for (index, (tab, _)) in tab_spans().into_iter().enumerate() {
        if index > 0 {
            spans.push(Span::styled(
                format!(" {bar} "),
                Style::default().fg(Color::DarkGray),
            ));
        }
        let label = format!(" {} {} ", tab.index() + 1, tab.label());
        let style = if tab == active {
            Style::default()
                .bg(Color::Blue)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        spans.push(Span::styled(label, style));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// One row of the Branches tab's left pane (T2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BranchRow {
    /// A group heading, drawn when the ref kind changes.
    Header(RefKind),
    /// A branch row, by index into [`AppState::branches`].
    Entry(usize),
}

/// The pane's rows: a heading per ref kind, then that kind's entries.
///
/// `branches` is already sorted local → remote → tag, so a heading is emitted
/// whenever the kind changes rather than by grouping a second time.
pub fn branch_rows(branches: &[BranchEntry]) -> Vec<BranchRow> {
    let mut rows = Vec::new();
    let mut last: Option<RefKind> = None;
    for (index, entry) in branches.iter().enumerate() {
        if last != Some(entry.kind) {
            rows.push(BranchRow::Header(entry.kind));
            last = Some(entry.kind);
        }
        rows.push(BranchRow::Entry(index));
    }
    rows
}

/// The first pane row the branch list draws, so `selected` stays visible (T2).
///
/// [`branch_pane`] and [`crate::app::AppState::surface_at`] both call this with
/// the same [`branch_rows`] list, so a draw and a click can never disagree.
pub fn branch_pane_offset(rows: &[BranchRow], selected: usize, height: u16) -> usize {
    let selected_row = rows
        .iter()
        .position(|row| *row == BranchRow::Entry(selected))
        .unwrap_or(0);
    let view = height as usize;
    selected_row
        .saturating_add(1)
        .saturating_sub(view)
        .min(rows.len().saturating_sub(view))
}

/// One branch row: `name  HEAD  N commits`, coloured by ref kind (T2).
fn branch_line(entry: &BranchEntry, selected: bool, filtered: bool) -> Line<'static> {
    let name_style = match entry.kind {
        RefKind::Local if entry.is_head => Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
        RefKind::Local => Style::default().fg(Color::Cyan),
        RefKind::Remote => Style::default().fg(Color::Magenta),
        RefKind::Tag => Style::default().fg(Color::Yellow),
    };
    let mut spans = vec![Span::styled(
        if filtered { "* " } else { "  " },
        Style::default().fg(Color::Green),
    )];
    spans.push(Span::styled(entry.name.clone(), name_style));
    if entry.is_head {
        spans.push(Span::styled(
            "  HEAD",
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ));
    }
    let suffix = if entry.count == 1 {
        "commit"
    } else {
        "commits"
    };
    spans.push(Span::styled(
        format!("  {} {suffix}", entry.count),
        Style::default().fg(Color::DarkGray),
    ));
    let line = Line::from(spans);
    if selected {
        line.style(Style::default().bg(Color::Indexed(236)))
    } else {
        line
    }
}

/// The Branches tab's left pane: refs grouped local → remote → tag (T2).
///
/// The window follows [`AppState::branch_selected`], so the cursor is always
/// visible however long the ref list grows.
pub fn branch_pane(frame: &mut Frame, area: Rect, state: &AppState, ladder: Ladder) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    // The last column is the pane's right border, like `.branches`'s
    // `border-right` in the design.
    let list = Rect {
        width: area.width.saturating_sub(1),
        ..area
    };
    let rows = branch_rows(&state.branches);
    let view = area.height as usize;
    let offset = branch_pane_offset(&rows, state.branch_selected, area.height);

    let mut lines: Vec<Line<'static>> = Vec::new();
    for row in rows.iter().skip(offset).take(view) {
        match row {
            BranchRow::Header(kind) => lines.push(Line::styled(
                format!(" {}", kind.label()),
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::BOLD),
            )),
            BranchRow::Entry(index) => {
                let entry = &state.branches[*index];
                let filtered = state.branch_filter.as_deref() == Some(entry.refname.as_str());
                lines.push(branch_line(
                    entry,
                    *index == state.branch_selected,
                    filtered,
                ));
            }
        }
    }
    frame.render_widget(Paragraph::new(lines), list);

    // The separator column, drawn with the ladder's vertical glyph.
    if area.width > list.width {
        let border = Rect {
            x: area.x + list.width,
            width: 1,
            ..area
        };
        let glyph = ladder.glyphs().vertical;
        let text: Vec<Line<'static>> = (0..border.height)
            .map(|_| Line::styled(glyph.to_string(), Style::default().fg(Color::DarkGray)))
            .collect();
        frame.render_widget(Paragraph::new(text), border);
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
    /// The find bar, if visible.
    pub find: u16,
    /// The tab bar — always present since M4-W12/T1.
    pub tabs: u16,
}

/// How many rows the overlays claim for a screen of `screen_height` rows.
///
/// The commit-details panel is **not** here any more: M4-W12/T3 moved it inline,
/// where it is part of the body flow rather than a docked strip.
pub fn overlay_rows(state: &AppState, _screen_height: u16) -> OverlayRows {
    OverlayRows {
        tabs: TAB_BAR_HEIGHT,
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
    // `ui` stacks the tab bar, the find bar and the status bar around the body,
    // so the table area is the screen minus exactly those rows. Deriving it from
    // `overlay_rows` keeps the router and the renderer on one formula —
    // `status.height` alone does not know about the overlays (W10).
    let overlays = overlay_rows(state, screen_height);
    let table_height = screen_height
        .saturating_sub(overlays.tabs)
        .saturating_sub(overlays.find)
        .saturating_sub(STATUS_HEIGHT);
    // `body` is the whole table pane (its header included), because
    // [`AppState::surface_at`] indexes rows relative to it and skips the header.
    // On the Branches tab the left pane takes a column band out of it, exactly
    // as `ui` splits it (T2).
    let pane = branch_pane_width(state.tab, width);
    let branch_pane = (pane > 0).then_some(Rect {
        x: 0,
        y: overlays.tabs,
        width: pane,
        height: table_height,
    });
    let body = Rect {
        x: pane,
        y: overlays.tabs,
        width: width.saturating_sub(pane),
        height: table_height,
    };
    // The graph pane has no header, so the inline blocks sit one row lower.
    let header = u16::from(table_height > 1);
    let graph_body = Rect {
        y: body.y.saturating_add(header),
        height: table_height.saturating_sub(header),
        ..body
    };
    let graph_width = text_offset(layout.max_lane).min(body.width);
    Some(HitGeometry {
        body,
        graph_width,
        has_header: table_height > 1,
        cdv: cdv_rect(state, graph_body, graph_width),
        branch_pane,
        tab_bar: Rect {
            x: 0,
            y: 0,
            width,
            height: overlays.tabs,
        },
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

    // One display-row height per commit row, indexed by row (T1/T3/T7), plus the
    // refs line each row spends above its commit (T7). The table and the graph
    // read both, so display row `d` is the same row in each.
    let heights: Vec<u16> = (0..state.rows.len() as RowIdx)
        .map(|row| state.row_height(row))
        .collect();
    let refs: Vec<u16> = (0..state.rows.len() as RowIdx)
        .map(|row| state.block_parts(row).refs)
        .collect();
    let visible = state.visible_rows();
    let leading = state.uncommitted_height();
    let expanded: Vec<bool> = (0..state.rows.len() as RowIdx)
        .map(|row| state.expanded.contains(&row))
        .collect();

    frame.render_widget(
        GraphWidget {
            layout,
            ladder: view.ladder,
            selected,
            uncommitted: view.uncommitted,
            scroll: state.scroll,
            leading,
            heights: &heights,
            refs: &refs,
            visible: &visible,
        },
        body,
    );

    frame.render_widget(
        commits_table_window(
            &state.rows,
            TableWindow {
                heights: &heights,
                refs: &refs,
                visible: &visible,
                leading,
                expanded: &expanded,
            },
            state.scroll,
            selected.map(|row| row as usize),
            &state.columns,
            state.find.visible.then_some(&state.find),
        ),
        columns[1],
    );

    // T3/T4/T5/T7 — the inline blocks paint over the rows the table reserved.
    // `body` is the graph column, so widen it to the whole area: the blocks
    // start after the graph and subtract `graph_width` themselves to find the
    // text column. Passing the narrow rect makes their width zero, and then
    // nothing is painted at all.
    draw_inline_blocks(
        frame,
        Rect {
            width: area.width,
            ..body
        },
        graph_width,
        state,
        view,
    );
}

/// Draw every inline block over the body, clipped to what is on screen.
///
/// A block whose top has scrolled above the body is skipped: the table already
/// painted blank spacer rows there, and half a panel reads as corruption. A
/// block that runs past the bottom is clipped to the body edge.
fn draw_inline_blocks(
    frame: &mut Frame,
    body: Rect,
    graph_width: u16,
    state: &AppState,
    view: &ViewOptions<'_>,
) {
    let x = body.x.saturating_add(graph_width);
    let width = body.width.saturating_sub(graph_width);
    if width == 0 || body.height == 0 {
        return;
    }

    for block in inline_blocks(state, width) {
        let top = body.y as i32 + block.top as i32 - state.scroll as i32;
        if top < body.y as i32 {
            continue;
        }
        let top = top as u16;
        if top >= body.bottom() {
            continue;
        }
        let max_height = body.bottom() - top;
        let rect = Rect {
            x,
            y: top,
            width,
            height: max_height,
        };

        match &block.kind {
            BlockKind::Cdv(panel) => {
                let height = crate::cdv::inline_height(panel, body.height.max(1)).min(max_height);
                if height == 0 {
                    continue;
                }
                let rect = Rect { height, ..rect };
                frame.render_widget(Clear, rect);
                cdv::draw(frame, rect, panel, state.cdv_focused, view.ladder);
            }
            BlockKind::Framed(title) => {
                let want = block.lines.len() as u16 + 2;
                let height = want.min(max_height);
                if height < 2 {
                    continue;
                }
                let rect = Rect { height, ..rect };
                let frame_block = Block::default()
                    .borders(Borders::ALL)
                    .border_set(view.ladder.border_set())
                    .border_style(Style::default().fg(Color::Cyan))
                    .title(format!(" {title} "));
                frame.render_widget(Clear, rect);
                frame.render_widget(Paragraph::new(block.lines).block(frame_block), rect);
            }
            BlockKind::Plain => {
                if block.lines.is_empty() {
                    continue;
                }
                let height = (block.lines.len() as u16).min(max_height);
                if height == 0 {
                    continue;
                }
                let rect = Rect { height, ..rect };
                let mut lines = block.lines;
                if block.selected {
                    // Keep the selection a full-width bar, like the commit row.
                    for line in &mut lines {
                        for span in &mut line.spans {
                            span.style = span.style.add_modifier(Modifier::REVERSED);
                        }
                    }
                }
                frame.render_widget(Paragraph::new(lines), rect);
            }
        }
    }
}

/// The screen rect of the open commit-details panel, or `None` when it is not
/// on screen. [`hit_geometry`] and [`draw_inline_blocks`] share this so the
/// mouse router can never disagree with what was painted.
pub fn cdv_rect(state: &AppState, body: Rect, graph_width: u16) -> Option<Rect> {
    let panel = state.cdv.as_ref()?;
    let row = panel.row;
    if !state.expanded.contains(&row) {
        return None;
    }
    let parts = state.block_parts(row);
    let top = state
        .row_top(row)
        .saturating_add(parts.refs)
        .saturating_add(parts.commit);
    let y = body.y as i32 + top as i32 - state.scroll as i32;
    if y < body.y as i32 || y >= body.bottom() as i32 {
        return None;
    }
    let width = body.width.saturating_sub(graph_width);
    if width == 0 {
        return None;
    }
    let height = crate::cdv::inline_height(panel, body.height.max(1)).min(body.bottom() - y as u16);
    (height > 0).then_some(Rect {
        x: body.x.saturating_add(graph_width),
        y: y as u16,
        width,
        height,
    })
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
    use crate::app::{AppState, DiffSource, InlineDiff};
    use crate::cdv::{CdvKind, CdvState};
    use crate::menu::{ContextMenu, MenuContext, MenuSurface};
    use crate::render::mock::demo_layout;

    #[test]
    fn graph_width_uses_the_contract_pitch() {
        assert_eq!(graph_width(&demo_layout()), 4);
    }

    /// Render the whole UI into a headless backend and return the screen text.
    ///
    /// The W12 surfaces are inline blocks painted *over* the rows the commit
    /// table reserves, so only a real `ui` draw proves they land where the
    /// mockup puts them — a unit test of the block builder would pass even if
    /// nothing reached the screen.
    fn screen(state: &AppState, width: u16, height: u16) -> Vec<String> {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;
        // The real loop refreshes `status.width`/`status.height` from the
        // terminal before every draw (`main.rs`), and the two disagreeing
        // readers of the panel height are `AppState::inline_cdv_height` (from
        // `status.height`) and `cdv_rect` (from the body rect). Drawing a frame
        // without refreshing the viewport therefore reserves one panel height
        // and paints another, and the panel and its diff overlap. Mirror the
        // loop: the body is the frame minus the tab bar, the table header and
        // the status bar.
        const CHROME: u16 = 3;
        let mut state = state.clone();
        state.status.width = width;
        state.status.height = height.saturating_sub(CHROME);
        let state = &state;
        let view = ViewOptions {
            graph: Some(&state.layout),
            // `make_view` marks the mock fixture's first row as the
            // uncommitted entry; mirror that so the draw matches the app's.
            uncommitted: Some(0),
            ..ViewOptions::TEXT_ONLY
        };
        let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("terminal");
        terminal
            .draw(|frame| ui(frame, state, &view))
            .expect("draw");
        let buffer = terminal.backend().buffer();
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol().to_string())
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect()
    }

    /// The index and text of the single line holding `needle`.
    fn line_with<'a>(rows: &'a [String], needle: &str) -> (usize, &'a str) {
        let hits: Vec<(usize, &String)> = rows
            .iter()
            .enumerate()
            .filter(|(_, line)| line.contains(needle))
            .collect();
        assert_eq!(
            hits.len(),
            1,
            "expected exactly one line with {needle:?}, got {}\n{}",
            hits.len(),
            rows.join("\n")
        );
        (hits[0].0, hits[0].1.as_str())
    }

    /// The mock row carrying the `HEAD -> main` ref.
    fn head_row(state: &AppState) -> RowIdx {
        state
            .rows
            .iter()
            .position(|commit| commit.abbrev == "a1b2c3d")
            .expect("the mock HEAD commit") as RowIdx
    }

    /// T7 — a row with refs spends its first display row on the refs line, so
    /// the refs read as a heading over their commit instead of a column beside
    /// it. The commit text and its graph vertex stay on the commit's own row.
    #[test]
    fn the_refs_line_sits_above_its_commit_row() {
        let state = AppState::demo();
        let rows = screen(&state, 120, 24);

        let (refs_at, refs_line) = line_with(&rows, "HEAD -> main");
        assert!(
            refs_line.contains("┌ refs │ HEAD -> main"),
            "the refs line must carry the mockup's label: {refs_line:?}"
        );
        assert!(
            !refs_line.contains("a1b2c3d"),
            "the refs line must not carry the commit itself: {refs_line:?}"
        );

        let commit = &rows[refs_at + 1];
        assert!(
            commit.contains("a1b2c3d") && commit.contains("Add graph renderer widget"),
            "the commit row must follow its refs line, got {commit:?}"
        );
        // The vertex travels with the commit, not with the refs heading.
        assert!(
            commit.trim_start().starts_with('◯'),
            "the graph vertex must sit on the commit row, got {commit:?}"
        );
    }

    /// Q2 — an uncommitted file row reads `┌─ M src/render/ui.rs  staged  +42 −8`:
    /// the mockup's order is tree glyph, status letter, path, staging marker,
    /// then the counts.
    #[test]
    fn an_uncommitted_file_row_reads_tree_letter_path_stage_counts() {
        let state = AppState::demo();
        let rows = screen(&state, 120, 30);

        let (_, row) = line_with(&rows, "src/render/ui.rs");
        assert!(
            row.contains("┌─ M src/render/ui.rs"),
            "the first file row opens the tree and leads with the letter: {row:?}"
        );
        let stage = row.find("staged").expect("a staging marker");
        let counts = row.find('+').expect("an additions count");
        assert!(
            stage < counts,
            "the staging marker comes before the counts: {row:?}"
        );
        assert!(
            !row.contains('['),
            "the staging marker is no longer bracketed: {row:?}"
        );

        // The last row closes the tree instead of continuing it.
        let (_, last) = line_with(&rows, "src/legacy/panel.rs");
        assert!(
            last.contains("└─ D src/legacy/panel.rs"),
            "the last file row closes the tree: {last:?}"
        );
    }

    /// Q1 — the mockup ends every commit row with a caret: `▼` when the row's
    /// details are open, `▶` when they are closed.
    #[test]
    fn the_commit_row_ends_with_an_expansion_caret() {
        let mut state = AppState::demo();
        let row = head_row(&state);

        let closed = screen(&state, 120, 24);
        let (at, _) = line_with(&closed, "Add graph renderer widget");
        assert!(
            closed[at].trim_end().ends_with('▶'),
            "a closed row ends with the collapsed caret: {:?}",
            closed[at]
        );

        state.expanded.insert(row);
        let open = screen(&state, 120, 30);
        let (at, _) = line_with(&open, "Add graph renderer widget");
        assert!(
            open[at].trim_end().ends_with('▼'),
            "an open row ends with the expanded caret: {:?}",
            open[at]
        );
    }

    /// T8 — the evidence gallery: print one full frame per W12 surface so the
    /// wave report can quote real screens instead of describing them.
    ///
    /// Ignored by default (it only prints), so it costs CI nothing. Run it with
    /// `cargo test --bin git-graph-terminal w12_evidence_gallery -- --ignored --nocapture`
    /// and paste the frames into `.IDE_Plans/w12-report.md`.
    #[test]
    #[ignore = "prints frames for the wave report; run with --ignored --nocapture"]
    fn w12_evidence_gallery() {
        fn frame(label: &str, state: &AppState) {
            println!("\n===== {label} =====\n");
            for line in screen(state, 120, 40) {
                println!("{line}");
            }
        }

        let overview = AppState::demo();
        frame("Overview (key 1) — default", &overview);

        let mut branches = AppState::demo();
        branches.tab = Tab::Branches;
        frame("Branches (key 2) — all branches", &branches);

        let mut filtered = AppState::demo();
        filtered.tab = Tab::Branches;
        filtered.branch_filter = Some("refs/heads/feature/graph".to_string());
        // `branch_filter` is only the label; `branch_reachable` is the set
        // `visible_rows` filters on, so the gallery has to set both.
        let reachable = filtered
            .rows
            .iter()
            .take(4)
            .map(|commit| commit.hash.clone())
            .collect();
        filtered.branch_reachable = Some(reachable);
        frame("Branches (key 2) — feature/graph filtered", &filtered);

        let mut closed = AppState::demo();
        closed.uncommitted_open = false;
        frame("Overview — `u` collapsed the uncommitted section", &closed);

        let mut expanded = AppState::demo();
        let row = head_row(&expanded);
        expanded.expanded.insert(row);
        expanded.cdv = Some(CdvState::loading(row, "a1b2c3d", CdvKind::Commit, None));
        frame(
            "Overview — Enter expanded the HEAD row (loading)",
            &expanded,
        );

        // The same row with the panel loaded and one file's diff open — the two
        // surfaces the mockup's Q1 is really about.
        let change = |status, path: &str, adds: u32, dels: u32| crate::files::GitFileChange {
            old_path: path.to_string(),
            new_path: path.to_string(),
            status,
            additions: Some(adds),
            deletions: Some(dels),
            submodule: false,
        };
        let mut loaded = AppState::demo();
        let row = head_row(&loaded);
        loaded.expanded.insert(row);
        let mut cdv = CdvState::loading(row, "a1b2c3d", CdvKind::Commit, None);
        cdv.loaded(
            crate::files::CommitDetail {
                hash: "a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0".to_string(),
                parents: vec!["9f8e7d6".to_string()],
                author: "Ada Lovelace".to_string(),
                author_email: "ada@example.invalid".to_string(),
                author_date: 1_700_000_000,
                committer: "Ada Lovelace".to_string(),
                committer_email: "ada@example.invalid".to_string(),
                committer_date: 1_700_000_000,
                signature_status: ' ',
                signature_signer: String::new(),
                signature_key: String::new(),
                body: "Add graph renderer widget\n\nThe lane router draws rounded corners."
                    .to_string(),
            },
            vec![
                change(crate::files::GitFileStatus::Modified, "src/lib.rs", 12, 3),
                change(crate::files::GitFileStatus::Added, "src/graph.rs", 48, 0),
            ],
        );
        // `app::open_cdv` hands the panel its commit's decorations; the gallery
        // mirrors that so `Branch:` / `Tags:` show the real values.
        cdv.refs = loaded.rows[row as usize].refs.clone();
        loaded.cdv = Some(cdv);
        loaded.inline_diff = Some(InlineDiff {
            title: "git show a1b2c3d -- src/lib.rs".to_string(),
            lines: crate::diff::parse("@@ -1,2 +1,3 @@\n one\n+two\n"),
            source: DiffSource::CommitFile {
                row,
                path: "src/lib.rs".to_string(),
            },
        });
        frame(
            "Overview — Enter expanded the HEAD row (loaded, one file diff open)",
            &loaded,
        );
    }

    /// T4 — the uncommitted section is the first thing in the Overview body: a
    /// header with the count, a summary line, then one row per file, all above
    /// the history.
    #[test]
    fn the_uncommitted_section_paints_above_the_history() {
        let state = AppState::demo();
        let rows = screen(&state, 120, 30);

        let (header_at, _header) = line_with(&rows, &state.uncommitted.title(true));
        let summary = &rows[header_at + 1];
        assert!(
            summary.contains("staged") && summary.contains("unstaged"),
            "the summary line must split staged from unstaged: {summary:?}"
        );

        // One row per file, each carrying its path and its status letter.
        for path in ["src/render/ui.rs", "src/render/tabs.rs"] {
            assert!(
                rows.iter().any(|line| line.contains(path)),
                "no uncommitted row for {path} in\n{}",
                rows.join("\n")
            );
        }

        let (commit_at, _) = line_with(&rows, "Add graph renderer widget");
        assert!(
            header_at < commit_at,
            "the section belongs above the history: header {header_at}, history {commit_at}"
        );
    }

    /// T3 — expanding a row paints the commit-details panel directly under that
    /// row, keeping the split-pane design inside the list.
    #[test]
    fn the_details_panel_paints_under_its_row() {
        let mut state = AppState::demo();
        let row = head_row(&state);
        state.expanded.insert(row);
        state.cdv = Some(CdvState::loading(row, "a1b2c3d", CdvKind::Commit, None));
        let rows = screen(&state, 120, 40);

        let (commit_at, _) = line_with(&rows, "Add graph renderer widget");
        let panel = &rows[commit_at + 1];
        assert!(
            panel.contains("├─ Commit Details"),
            "the panel must open under its commit row, got {panel:?}"
        );
        // The mockup's own lead text, not the `--dump` status line.
        assert!(
            panel.contains("— loading…"),
            "a loading panel says so in the title, got {panel:?}"
        );
    }

    /// A loaded commit panel on the HEAD row, with two changed files.
    fn loaded_details_panel() -> AppState {
        let mut state = AppState::demo();
        let row = head_row(&state);
        state.expanded.insert(row);
        let change = |status, path: &str, adds: u32, dels: u32| crate::files::GitFileChange {
            old_path: path.to_string(),
            new_path: path.to_string(),
            status,
            additions: Some(adds),
            deletions: Some(dels),
            submodule: false,
        };
        let mut cdv = CdvState::loading(row, "a1b2c3d", CdvKind::Commit, None);
        cdv.loaded(
            crate::files::CommitDetail {
                hash: "a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0".to_string(),
                parents: vec!["9f8e7d6".to_string()],
                author: "Ada Lovelace".to_string(),
                author_email: "ada@example.invalid".to_string(),
                author_date: 1_700_000_000,
                committer: "Ada Lovelace".to_string(),
                committer_email: "ada@example.invalid".to_string(),
                committer_date: 1_700_000_000,
                signature_status: ' ',
                signature_signer: String::new(),
                signature_key: String::new(),
                body: "Add graph renderer widget\n\nThe lane router draws rounded corners."
                    .to_string(),
            },
            vec![
                change(crate::files::GitFileStatus::Modified, "src/lib.rs", 12, 3),
                change(crate::files::GitFileStatus::Added, "src/graph.rs", 48, 0),
            ],
        );
        cdv.refs = vec!["HEAD -> main".to_string(), "tag: v1".to_string()];
        state.cdv = Some(cdv);
        state
    }

    /// T3 — mockup v3's `.pane{grid-template-columns:1fr 1fr}`: the commit
    /// information and the modified-file list paint on the **same** line, side
    /// by side, instead of one above the other.
    #[test]
    fn the_details_pane_paints_as_two_columns() {
        let state = loaded_details_panel();
        let rows = screen(&state, 160, 40);
        let (commit_at, _) = line_with(&rows, "Add graph renderer widget");

        // Row +1 is the block's top border carrying the title; the content
        // starts on row +2, where both columns begin.
        let first = &rows[commit_at + 2];
        assert!(
            first.contains("Commit: a1b2c3d"),
            "the left column starts the panel: {first:?}"
        );
        assert!(
            first.contains("Files (2) — Tree View"),
            "the right column shares the line: {first:?}"
        );
        assert!(
            first.find("Commit: a1b2c3d") < first.find("Files (2)"),
            "commit information is the left column: {first:?}"
        );

        // The mockup's new rows land in the left column, below `Commit:`.
        let second = &rows[commit_at + 3];
        assert!(second.contains("Parents:"), "{second:?}");
    }

    /// T3 — the mockup's `.totals` and diff-summary sentence close the right
    /// column.
    #[test]
    fn the_details_pane_totals_close_the_right_column() {
        let state = loaded_details_panel();
        let rows = screen(&state, 160, 40);

        line_with(
            &rows,
            "Total files changed: 2 · additions: +60 · deletions: −3",
        );
        line_with(
            &rows,
            "Diff summary: 2 files changed, 60 insertions(+), 3 deletions(−)",
        );
    }

    /// T5 — the selected file's diff paints under the details panel, framed by
    /// the `git diff` argv it came from.
    #[test]
    fn the_file_diff_paints_under_the_details_panel() {
        let mut state = AppState::demo();
        let row = head_row(&state);
        state.expanded.insert(row);
        state.cdv = Some(CdvState::loading(row, "a1b2c3d", CdvKind::Commit, None));
        state.inline_diff = Some(InlineDiff {
            title: "git show a1b2c3d -- src/lib.rs".to_string(),
            lines: crate::diff::parse("@@ -1,2 +1,3 @@\n one\n+two\n"),
            source: DiffSource::CommitFile {
                row,
                path: "src/lib.rs".to_string(),
            },
        });
        let rows = screen(&state, 120, 40);

        let (commit_at, _) = line_with(&rows, "Add graph renderer widget");
        let (diff_at, _) = line_with(&rows, "git show a1b2c3d -- src/lib.rs");
        assert!(
            diff_at > commit_at,
            "the diff belongs below its commit row: commit {commit_at}, diff {diff_at}"
        );
        // …and below the panel's *body*, not over it. The panel is sized from
        // `status.height` while the rect comes from the body height, so a draw
        // that skips the per-frame viewport refresh reserves one height and
        // paints another — this assertion is what catches that.
        let (panel_at, _) = line_with(&rows, "├─ Commit Details");
        let bottom = rows[panel_at..diff_at]
            .iter()
            .rposition(|line| line.contains('└'))
            .map(|offset| panel_at + offset)
            .expect("the panel's bottom border");
        assert!(
            bottom < diff_at,
            "the diff must clear the panel's bottom border (row {bottom}), not overwrite\n{}",
            rows.join("\n")
        );
        assert!(
            rows.iter().any(|line| line.contains("two")),
            "the added line must be painted in\n{}",
            rows.join("\n")
        );
    }

    /// The bar's labels and the mouse router walk the same format strings, so a
    /// click on a label's every column must land on that label (T1/T7).
    #[test]
    fn every_column_of_a_tab_label_maps_back_to_that_tab() {
        for (tab, span) in tab_spans() {
            for x in span.clone() {
                assert_eq!(tab_at_x(x), Some(tab), "column {x} of {}", tab.label());
            }
        }
    }

    #[test]
    fn a_tab_separator_is_not_a_tab() {
        let spans = tab_spans();
        // The `│` between the two labels, and the one the bar starts with.
        assert_eq!(tab_at_x(0), None);
        assert_eq!(tab_at_x(1), None);
        let gap = spans[0].1.end;
        assert_eq!(tab_at_x(gap), None);
        // Past the end of the bar.
        assert_eq!(tab_at_x(spans.last().expect("two tabs").1.end), None);
    }

    /// The drawn bar must be exactly as wide as the router thinks it is, or a
    /// click would land on the wrong label.
    #[test]
    fn the_drawn_tab_bar_matches_the_click_ranges() {
        let (text, styles) = render_tab_bar(Tab::Branches, 40);
        let row: Vec<char> = text[0].chars().collect();
        for (tab, span) in tab_spans() {
            let label: String = row
                .iter()
                .skip(span.start as usize)
                .take(span.len())
                .collect();
            assert!(
                label.contains(tab.label()),
                "{span:?} reads {label:?} but should hold {}",
                tab.label()
            );
        }
        // The active tab is highlighted across its whole label, and the other
        // one is not, so the bar shows which tab is current (T1).
        for (tab, span) in tab_spans() {
            let active = tab == Tab::Branches;
            for x in span {
                let style = styles[0][x as usize];
                assert_eq!(
                    style.bg == Some(Color::Blue),
                    active,
                    "column {x} of {}",
                    tab.label()
                );
            }
        }
    }

    /// Render just the tab bar, and return the one row it draws.
    fn render_tab_bar(active: Tab, width: u16) -> (Vec<String>, Vec<Vec<Style>>) {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let backend = TestBackend::new(width, TAB_BAR_HEIGHT);
        let mut terminal =
            Terminal::new(backend).unwrap_or_else(|e: std::convert::Infallible| match e {});
        terminal
            .draw(|frame| tab_bar(frame, frame.area(), active, Ladder::unlimited()))
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

    /// The pane exists only on the Branches tab and only when there is room for
    /// it beside the history (T2).
    #[test]
    fn the_branch_pane_needs_the_branches_tab_and_a_wide_terminal() {
        assert_eq!(
            branch_pane_width(Tab::Overview, 100),
            0,
            "Overview has no pane"
        );
        assert_eq!(branch_pane_width(Tab::Branches, 40), 0, "too narrow");
        assert_eq!(branch_pane_width(Tab::Branches, 100), BRANCH_PANE_WIDTH);
        // A body just wide enough for the pane halves it rather than starving
        // the history, and never exceeds the body.
        let pane = branch_pane_width(Tab::Branches, BRANCH_PANE_MIN_BODY);
        assert!(pane <= BRANCH_PANE_MIN_BODY / 2, "{pane}");
    }

    /// A heading is emitted once per ref kind, and every entry keeps its index
    /// into `AppState::branches` so a click can resolve back to it (T2).
    #[test]
    fn branch_rows_heads_each_ref_kind_once() {
        use crate::branches::{BranchEntry, RefKind};
        let entry = |name: &str, kind, is_head| BranchEntry {
            name: name.to_string(),
            refname: name.to_string(),
            kind,
            is_head,
            count: 1,
        };
        let branches = vec![
            entry("main", RefKind::Local, true),
            entry("dev", RefKind::Local, false),
            entry("origin/main", RefKind::Remote, false),
            entry("v1.0", RefKind::Tag, false),
        ];
        assert_eq!(
            branch_rows(&branches),
            vec![
                BranchRow::Header(RefKind::Local),
                BranchRow::Entry(0),
                BranchRow::Entry(1),
                BranchRow::Header(RefKind::Remote),
                BranchRow::Entry(2),
                BranchRow::Header(RefKind::Tag),
                BranchRow::Entry(3),
            ]
        );
        assert!(branch_rows(&[]).is_empty());
    }

    /// The pane's window scrolls only as far as it must to keep the cursor on
    /// screen, and never past the end of the list (T2).
    #[test]
    fn the_branch_window_follows_the_cursor() {
        let rows: Vec<BranchRow> = (0..10).map(BranchRow::Entry).collect();
        // The first rows need no scrolling at all.
        assert_eq!(branch_pane_offset(&rows, 0, 4), 0);
        assert_eq!(branch_pane_offset(&rows, 3, 4), 0);
        // Row 4 is one past the window, so the window moves by exactly one.
        assert_eq!(branch_pane_offset(&rows, 4, 4), 1);
        // The last row pins the window to the bottom of the list.
        assert_eq!(branch_pane_offset(&rows, 9, 4), 6);
        // A window taller than the list never scrolls.
        assert_eq!(branch_pane_offset(&rows, 9, 40), 0);
        // An empty list is not a panic.
        assert_eq!(branch_pane_offset(&[], 0, 4), 0);
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
