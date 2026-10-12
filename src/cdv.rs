//! The commit details view (M2-W6 / T3).
//!
//! Ports `web/main.ts:2524-2600` (the panel's HTML) and the pieces of
//! `web/main.ts:3528-3600` that build its file list, onto a terminal panel.
//!
//! Reference anchors (read-only `../vscode-git-graph`):
//!
//! * `web/main.ts:2536-2537` — **VIS-037** the loading label is
//!   `Loading Commit Details ...`, `Loading Uncommitted Changes ...` or
//!   `Loading Commit Comparison ...` depending on what is being expanded.
//! * `web/main.ts:2539-2570` — **VIS-038** the summary block: `Commit`,
//!   `Parents` (comma-joined, `None` when there are none), `Author` plus
//!   `<email>`, a separate `Author Date` row **only when it differs from the
//!   committer date**, `Committer` plus `<email>`, the signature badge, and
//!   `Committer Date` (labelled plain `Date` when the two dates agree). The
//!   commit body follows after a blank line.
//! * `web/main.ts:2572` — **VIS-039** an uncommitted expansion shows
//!   `Displaying all uncommitted changes.` instead of a summary.
//! * `web/main.ts:2575` — **VIS-040** a comparison shows
//!   `Displaying all changes from <from> to <to>.`, with `Uncommitted Changes`
//!   standing in for the `to` side.
//! * `web/main.ts:2576` + `2851-2868` — **VIS-041 / VIS-042** the file view is
//!   either a `File Tree` or a `File List`, and the control toggles between them.
//! * `web/main.ts:3586-3597` — **VIS-047** one row per changed file: the file
//!   name, a status badge, `(+N|-M)` counts for modified text files, a
//!   "last viewed" eye, and a `Unable to View Diff (this is a binary file)`
//!   hint when the file cannot be diffed. **VIS-048** a submodule gets its own
//!   repository row. **VIS-052** an untracked file is always diffable.
//!
//! ## Deliberate deviations
//!
//! 1. **Docked, not inline.** The reference defaults to
//!    `commitDetailsView.location = Inline` (`src/config.ts:72-74`) and also
//!    supports `Docked to Bottom`. An inline panel needs the table to grow a
//!    variable-height row, which the ratatui `Table` cannot do; the terminal
//!    port therefore docks the CDV to the bottom of the screen — a first-class
//!    reference layout, chosen because it is the only one the widget set can
//!    express. The panel is resizable ([`CdvState::resize`]) like the
//!    reference's `cdvHeightResize` handle.
//! 2. **`(Review)` markers are text.** The reference draws SVG eye/tick icons;
//!    here they are short text badges.
//! 3. **Signature shows as a badge + tooltip text.** `generateSignatureHtml`
//!    (`web/main.ts:3949-3959`) uses an icon with a `title`; the terminal shows
//!    the icon glyph and, on the same line, the description.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::files::{self, FileRow, FileRowKind, FileTree, FileView, GitFileChange, GitFileStatus};
use crate::layout::types::RowIdx;
use crate::render::columns::long_date;

/// The reference's `cdvHeightResize` bounds, in terminal rows.
pub const MIN_HEIGHT: u16 = 6;
pub const MAX_HEIGHT: u16 = 40;
/// Height a freshly opened CDV gets.
pub const DEFAULT_HEIGHT: u16 = 14;

/// What the panel is showing (`web/main.ts:2536`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CdvKind {
    /// A commit's details.
    #[default]
    Commit,
    /// The uncommitted-changes entry (VIS-039).
    Uncommitted,
    /// A two-commit comparison (VIS-040).
    Comparison,
}

impl CdvKind {
    /// The `Loading …` label (VIS-037).
    pub fn loading_label(self) -> &'static str {
        match self {
            CdvKind::Commit => "Commit Details",
            CdvKind::Uncommitted => "Uncommitted Changes",
            CdvKind::Comparison => "Commit Comparison",
        }
    }

    /// The panel title.
    pub fn title(self) -> &'static str {
        self.loading_label()
    }
}

/// Everything the CDV panel draws — `ExpandedCommit` plus the view settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CdvState {
    pub kind: CdvKind,
    /// `expandedCommit.loading` (`web/main.ts:2535`).
    pub loading: bool,
    /// The commit row this panel belongs to.
    pub row: RowIdx,
    /// `expandedCommit.commitHash`.
    pub hash: String,
    /// `expandedCommit.compareWithHash` — `Some` for a comparison (VIS-040).
    pub compare_with: Option<String>,
    /// `expandedCommit.commitDetails` (VIS-038).
    pub detail: Option<files::CommitDetail>,
    /// `expandedCommit.fileChanges` (VIS-047).
    pub changes: Vec<GitFileChange>,
    /// `expandedCommit.fileTree` (VIS-041).
    pub tree: FileTree,
    /// `commitDetailsView.fileView.type` (VIS-041 / VIS-042).
    pub view: FileView,
    /// Folders the user has closed, by full path (`alterFileTreeFolderOpen`).
    pub closed_folders: Vec<String>,
    /// Files marked reviewed (`alterFileTreeFileReviewed`).
    pub reviewed: Vec<String>,
    /// `expandedCommit.lastViewedFile`.
    pub last_viewed_file: Option<String>,
    /// The configured external diff tool (`git config diff.tool` /
    /// `gui.difftool`), or `None` when neither is set (VIS-051, W7/T4).
    pub external_diff: Option<String>,
    /// Cursor row into [`CdvState::file_rows`] (W7/T3) — the file-row menu's
    /// selected row, so A65–A68 can act on it.
    pub selected: usize,
    /// Scroll offset into the rendered lines.
    pub scroll: u16,
    /// Panel height in terminal rows.
    pub height: u16,
    /// `commitDetailsView.autoCenter` — kept for parity; unused while docked.
    pub auto_center: bool,
    /// Load error, shown in place of the summary.
    pub error: Option<String>,
}

impl CdvState {
    /// A panel that is still loading (`expandedCommit.loading = true`).
    pub fn loading(row: RowIdx, hash: &str, kind: CdvKind, compare_with: Option<String>) -> Self {
        CdvState {
            kind,
            loading: true,
            row,
            hash: hash.to_string(),
            compare_with,
            detail: None,
            changes: Vec::new(),
            tree: FileTree::default(),
            view: FileView::Tree,
            closed_folders: Vec::new(),
            reviewed: Vec::new(),
            last_viewed_file: None,
            external_diff: None,
            selected: 0,
            scroll: 0,
            height: DEFAULT_HEIGHT,
            auto_center: true,
            error: None,
        }
    }

    /// A loaded uncommitted-changes panel (VIS-039).
    pub fn uncommitted(row: RowIdx, changes: Vec<GitFileChange>) -> Self {
        let tree = FileTree::build(&changes);
        CdvState {
            kind: CdvKind::Uncommitted,
            loading: false,
            row,
            hash: crate::data::UNCOMMITTED.to_string(),
            compare_with: None,
            detail: None,
            changes,
            tree,
            view: FileView::Tree,
            closed_folders: Vec::new(),
            reviewed: Vec::new(),
            last_viewed_file: None,
            external_diff: None,
            selected: 0,
            scroll: 0,
            height: DEFAULT_HEIGHT,
            auto_center: true,
            error: None,
        }
    }

    /// A loaded comparison panel (VIS-040).
    pub fn comparison(row: RowIdx, from: &str, to: &str, changes: Vec<GitFileChange>) -> Self {
        let tree = FileTree::build(&changes);
        CdvState {
            kind: CdvKind::Comparison,
            loading: false,
            row,
            hash: from.to_string(),
            compare_with: Some(to.to_string()),
            detail: None,
            changes,
            tree,
            view: FileView::Tree,
            closed_folders: Vec::new(),
            reviewed: Vec::new(),
            last_viewed_file: None,
            external_diff: None,
            selected: 0,
            scroll: 0,
            height: DEFAULT_HEIGHT,
            auto_center: true,
            error: None,
        }
    }

    /// Attach the loaded detail, clearing the loading flag.
    pub fn loaded(&mut self, detail: files::CommitDetail, changes: Vec<GitFileChange>) {
        self.tree = FileTree::build(&changes);
        self.detail = Some(detail);
        self.changes = changes;
        self.loading = false;
    }

    /// Record a load failure; the panel shows the message instead of a summary.
    pub fn failed(&mut self, message: &str) {
        self.loading = false;
        self.error = Some(message.to_string());
    }

    /// Record the file list for a panel that has no commit summary
    /// (`CdvKind::Uncommitted` / `CdvKind::Comparison`, W7/T4).
    pub fn loaded_changes(&mut self, changes: Vec<GitFileChange>) {
        self.tree = FileTree::build(&changes);
        self.changes = changes;
        self.loading = false;
        self.error = None;
        self.clamp_selection();
    }

    /// Is the external diff action available? (VIS-051)
    ///
    /// This is `externalDiffPossible` (`web/main.ts:2521`): a loaded panel whose
    /// comparison or commit actually has a left-hand side to diff against.
    pub fn external_diff_available(&self) -> bool {
        let has_left_side = self.compare_with.is_some()
            || self
                .detail
                .as_ref()
                .map(|detail| !detail.parents.is_empty())
                .unwrap_or(false);
        !self.loading && has_left_side
    }

    /// Toggle Tree ↔ List (VIS-041 / VIS-042).
    pub fn toggle_view(&mut self) {
        self.view = self.view.toggle();
    }

    /// Resize the panel, clamped to the reference's bounds.
    pub fn resize(&mut self, delta: i32) {
        let height = self.height as i32 + delta;
        self.height = height.clamp(MIN_HEIGHT as i32, MAX_HEIGHT as i32) as u16;
    }

    /// Is `path` a folder the user has collapsed?
    pub fn folder_open(&self, path: &str) -> bool {
        !self.closed_folders.iter().any(|closed| closed == path)
    }

    /// Collapse/expand a folder (`alterFileTreeFolderOpen`).
    pub fn toggle_folder(&mut self, path: &str) {
        if let Some(index) = self.closed_folders.iter().position(|closed| closed == path) {
            self.closed_folders.remove(index);
        } else {
            self.closed_folders.push(path.to_string());
        }
    }

    /// Mark/unmark a file reviewed (`alterFileTreeFileReviewed`).
    pub fn toggle_reviewed(&mut self, path: &str) {
        if let Some(index) = self.reviewed.iter().position(|mark| mark == path) {
            self.reviewed.remove(index);
        } else {
            self.reviewed.push(path.to_string());
        }
    }

    /// Is `path` marked reviewed?
    pub fn is_reviewed(&self, path: &str) -> bool {
        self.reviewed.iter().any(|mark| mark == path)
    }

    /// The rows the file list draws (VIS-041 / VIS-042 / VIS-047).
    ///
    /// `files::file_rows` takes the set of **closed** folders (its `open`
    /// parameter is the reference's `folder.open` inverted — see
    /// `files::flatten_tree`), which is exactly what [`CdvState::closed_folders`]
    /// stores.
    pub fn file_rows(&self) -> Vec<FileRow> {
        let closed: std::collections::HashSet<String> =
            self.closed_folders.iter().cloned().collect();
        files::file_rows(&self.tree, &self.changes, self.view, &closed)
    }

    /// Number of rendered file rows.
    pub fn file_rows_len(&self) -> usize {
        self.file_rows().len()
    }

    /// Keep the cursor inside the row list after a collapse/expand (W7/T3).
    pub fn clamp_selection(&mut self) {
        let len = self.file_rows_len();
        self.selected = if len == 0 {
            0
        } else {
            self.selected.min(len - 1)
        };
    }

    /// Move the file-row cursor by `delta`, clamped at both ends (W7/T3).
    pub fn move_selection(&mut self, delta: i32) {
        let len = self.file_rows_len();
        if len == 0 {
            self.selected = 0;
            return;
        }
        self.selected = (self.selected as i32 + delta).clamp(0, len as i32 - 1) as usize;
    }

    /// The row under the cursor, if any.
    pub fn selected_row(&self) -> Option<FileRow> {
        self.file_rows().into_iter().nth(self.selected)
    }

    /// Full path of the row under the cursor.
    pub fn selected_path(&self) -> Option<String> {
        self.selected_row().map(|row| row.path)
    }

    /// The change the cursor points at, or `None` on a folder row (A65–A68).
    pub fn selected_change(&self) -> Option<&GitFileChange> {
        match self.selected_row()?.kind {
            FileRowKind::File(index) => self.changes.get(index),
            FileRowKind::Folder { .. } => None,
        }
    }

    /// Collapse (`open = false`) or expand (`open = true`) the folder under the
    /// cursor (W7/T3).
    pub fn set_folder_open_at_cursor(&mut self, open: bool) {
        if let Some(row) = self.selected_row() {
            if matches!(row.kind, FileRowKind::Folder { .. }) && self.folder_open(&row.path) != open
            {
                self.toggle_folder(&row.path);
                self.clamp_selection();
            }
        }
    }

    /// Scroll so the cursor row is inside `viewport` (W7/T3).
    pub fn ensure_file_visible(&mut self, viewport: u16) {
        if viewport == 0 {
            return;
        }
        let header = self.summary_lines().len() as u16 + 2;
        let line = header.saturating_add(self.selected as u16);
        if line < self.scroll {
            self.scroll = line;
        } else if line >= self.scroll.saturating_add(viewport) {
            self.scroll = line + 1 - viewport;
        }
    }

    /// The summary lines (`web/main.ts:2539-2575`) — VIS-038/039/040.
    pub fn summary_lines(&self) -> Vec<Line<'static>> {
        if let Some(error) = &self.error {
            return vec![Line::styled(
                format!("Unable to load commit details: {error}"),
                Style::default().fg(Color::Red),
            )];
        }
        let key = Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::BOLD);
        let value = Style::default();
        let dim = Style::default().fg(Color::DarkGray);

        match self.kind {
            CdvKind::Uncommitted => {
                return vec![Line::from(vec![
                    Span::styled("Displaying all ", value),
                    Span::styled(
                        "uncommitted changes",
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(".", value),
                ])]
            }
            CdvKind::Comparison => {
                let to = self
                    .compare_with
                    .as_deref()
                    .unwrap_or("Uncommitted Changes");
                let to = if to == crate::data::UNCOMMITTED {
                    "Uncommitted Changes"
                } else {
                    to
                };
                return vec![Line::from(vec![
                    Span::styled("Displaying all changes from ", value),
                    Span::styled(
                        self.hash.clone(),
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(" to ", value),
                    Span::styled(
                        to.to_string(),
                        Style::default()
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(".", value),
                ])];
            }
            CdvKind::Commit => {}
        }

        let Some(detail) = &self.detail else {
            return vec![Line::styled("No details loaded.", dim)];
        };

        let mut lines = Vec::new();
        lines.push(kv(&key, "Commit: ", &detail.hash, value));
        let parents = if detail.parents.is_empty() {
            "None".to_string()
        } else {
            detail.parents.join(", ")
        };
        lines.push(kv(&key, "Parents: ", &parents, value));
        lines.push(Line::from(vec![
            Span::styled("Author: ", key),
            Span::styled(detail.author.clone(), value),
            Span::styled(email_suffix(&detail.author_email), dim),
        ]));
        if detail.author_date != detail.committer_date {
            lines.push(kv(
                &key,
                "Author Date: ",
                &long_date(detail.author_date),
                value,
            ));
        }
        let mut committer = vec![
            Span::styled("Committer: ", key),
            Span::styled(detail.committer.clone(), value),
            Span::styled(email_suffix(&detail.committer_email), dim),
        ];
        if detail.has_signature() {
            committer.push(Span::styled(
                format!("  {}", signature_badge(detail)),
                signature_style(detail.signature_status),
            ));
        }
        lines.push(Line::from(committer));
        lines.push(kv(
            &key,
            if detail.author_date != detail.committer_date {
                "Committer Date: "
            } else {
                "Date: "
            },
            &long_date(detail.committer_date),
            value,
        ));

        let body = detail.body_after_subject();
        if !body.trim().is_empty() {
            lines.push(Line::default());
            for line in body.lines() {
                lines.push(Line::raw(line.to_string()));
            }
        }
        lines
    }

    /// One file-list row (VIS-047 / VIS-048 / VIS-052).
    ///
    /// `selected` draws the W7/T3 cursor: a `›` gutter and a highlight, so the
    /// file-row menu's target is always visible.
    pub fn file_line(&self, row: &FileRow, selected: bool) -> Line<'static> {
        let indent = "  ".repeat(row.depth as usize);
        let mut spans: Vec<Span<'static>> = vec![Span::styled(
            if selected { "› " } else { "  " },
            Style::default().fg(Color::Cyan),
        )];
        match row.kind {
            FileRowKind::Folder { open } => {
                spans.push(Span::raw(indent));
                spans.push(Span::styled(
                    if open { "▼ " } else { "▶ " },
                    Style::default().fg(Color::DarkGray),
                ));
                spans.push(Span::styled(
                    row.label.clone(),
                    Style::default()
                        .fg(Color::Blue)
                        .add_modifier(Modifier::BOLD),
                ));
            }
            FileRowKind::File(index) => {
                let Some(change) = self.changes.get(index) else {
                    spans.push(Span::raw(row.label.clone()));
                    return Line::from(spans);
                };
                spans.push(Span::raw(indent));
                spans.push(Span::raw("  "));
                spans.push(Span::styled(row.label.clone(), status_style(change.status)));
                // VIS-047: the status badge, then the counts. The reference only
                // renders the `+N|-M` span for a changed *text* file that is not
                // added, untracked or deleted (`web/main.ts:3591`).
                spans.push(Span::styled(
                    format!(" [{}]", change.status.code()),
                    status_style(change.status).add_modifier(Modifier::BOLD),
                ));
                if !matches!(
                    change.status,
                    GitFileStatus::Added | GitFileStatus::Untracked | GitFileStatus::Deleted
                ) {
                    if let Some(counts) = change.counts_label() {
                        spans.push(Span::styled(
                            format!(" ({counts})"),
                            Style::default().fg(Color::Green),
                        ));
                    }
                }
                if change.status == GitFileStatus::Renamed {
                    spans.push(Span::styled(
                        format!("  ({} → {})", change.old_path, change.new_path),
                        Style::default().fg(Color::DarkGray),
                    ));
                }
                if self.is_reviewed(&row.path) {
                    spans.push(Span::styled(
                        "  (Reviewed)",
                        Style::default().fg(Color::Green),
                    ));
                }
                if self.last_viewed_file.as_deref() == Some(row.path.as_str()) {
                    spans.push(Span::styled(
                        "  (Last Viewed)",
                        Style::default().fg(Color::Cyan),
                    ));
                }
                if change.submodule {
                    spans.push(Span::styled(
                        "  (submodule)",
                        Style::default().fg(Color::Magenta),
                    ));
                } else if !change.diff_possible() {
                    spans.push(Span::styled(
                        if change.status == GitFileStatus::Deleted {
                            "  (Unable to View Diff)"
                        } else {
                            "  (Unable to View Diff (this is a binary file))"
                        },
                        Style::default().fg(Color::DarkGray),
                    ));
                }
            }
        }
        let line = Line::from(spans);
        if selected {
            line.style(Style::default().bg(Color::DarkGray))
        } else {
            line
        }
    }

    /// Every line the panel draws, in order.
    pub fn lines(&self) -> Vec<Line<'static>> {
        if self.loading {
            return vec![Line::styled(
                format!("⏳ Loading {} ...", self.kind.loading_label()),
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            )];
        }
        let mut lines = self.summary_lines();
        lines.push(Line::default());
        // VIS-041 / VIS-042: the active file view is named in the panel.
        lines.push(Line::styled(
            format!(
                "Files ({}) — {} View",
                self.changes.len(),
                self.view.label()
            ),
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        ));
        if self.changes.is_empty() {
            lines.push(Line::styled(
                "No file changes.",
                Style::default().fg(Color::DarkGray),
            ));
        } else {
            for (index, row) in self.file_rows().iter().enumerate() {
                lines.push(self.file_line(row, index == self.selected));
            }
        }
        lines
    }

    /// The one-line panel state, for `--dump` evidence.
    pub fn status_line(&self) -> String {
        let state = if self.loading {
            "loading".to_string()
        } else if self.error.is_some() {
            "error".to_string()
        } else {
            format!("{} files", self.changes.len())
        };
        let mut line = format!(
            "cdv: {} {} [{} view] row {}",
            self.kind.title(),
            state,
            self.view.label(),
            self.row
        );
        if let Some(compare) = &self.compare_with {
            line.push_str(&format!(" compare-with {compare}"));
        }
        // VIS-051: the reference enables `#cdvExternalDiff` when a tool is
        // configured; a terminal has no GUI editor, so the port shows the tool
        // name it would have launched.
        if self.external_diff_available() {
            if let Some(tool) = &self.external_diff {
                line.push_str(&format!(" · external diff: {tool}"));
            }
        }
        line
    }
}

/// `" <email>"` when the address is known, else nothing (`web/main.ts:2560`).
fn email_suffix(email: &str) -> String {
    if email.is_empty() {
        String::new()
    } else {
        format!(" <{email}>")
    }
}

/// The signature glyph + description (`generateSignatureHtml`).
fn signature_badge(detail: &files::CommitDetail) -> String {
    format!(
        "{} {}",
        detail.signature_status,
        detail.signature_description()
    )
}

fn signature_style(status: char) -> Style {
    match status {
        'G' => Style::default().fg(Color::Green),
        'B' => Style::default().fg(Color::Red),
        _ => Style::default().fg(Color::Yellow),
    }
}

/// The colour for a file status, matching the reference's CSS classes.
fn status_style(status: GitFileStatus) -> Style {
    let colour = match status {
        GitFileStatus::Added => Color::Green,
        GitFileStatus::Modified => Color::Yellow,
        GitFileStatus::Deleted => Color::Red,
        GitFileStatus::Renamed => Color::Cyan,
        GitFileStatus::Untracked => Color::Magenta,
    };
    Style::default().fg(colour)
}

/// One `Key: value` line.
fn kv(key: &Style, label: &str, text: &str, value: Style) -> Line<'static> {
    Line::from(vec![
        Span::styled(label.to_string(), *key),
        Span::styled(text.to_string(), value),
    ])
}

/// The panel's height for a screen of `screen_height` rows.
///
/// The commit list keeps at least [`MIN_LIST_ROWS`] rows so the panel can never
/// swallow the table.
pub const MIN_LIST_ROWS: u16 = 4;

pub fn panel_height(state: &CdvState, screen_height: u16, status_height: u16) -> u16 {
    let available = screen_height
        .saturating_sub(status_height)
        .saturating_sub(MIN_LIST_ROWS);
    state.height.min(available).max(1)
}

/// Draw the panel (docked to the bottom of `area`).
pub fn draw(frame: &mut Frame, area: Rect, state: &CdvState, focused: bool) {
    if area.height == 0 || area.width == 0 {
        return;
    }
    frame.render_widget(Clear, area);
    let border = if focused {
        Style::default().fg(Color::Cyan)
    } else {
        Style::default().fg(Color::DarkGray)
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border)
        .title(format!(" {} ", state.status_line()));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let lines = state.lines();
    let paragraph = Paragraph::new(lines)
        .scroll((state.scroll, 0))
        .style(Style::default());
    frame.render_widget(paragraph, inner);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::UNCOMMITTED;

    fn change(
        status: GitFileStatus,
        path: &str,
        adds: Option<u32>,
        dels: Option<u32>,
    ) -> GitFileChange {
        GitFileChange {
            old_path: path.to_string(),
            new_path: path.to_string(),
            status,
            additions: adds,
            deletions: dels,
            submodule: false,
        }
    }

    fn detail() -> files::CommitDetail {
        files::CommitDetail {
            hash: "abcdef1234567890".to_string(),
            parents: vec!["1111111".to_string(), "2222222".to_string()],
            author: "Ada Lovelace".to_string(),
            author_email: "ada@example.invalid".to_string(),
            author_date: 1_700_000_000,
            committer: "Grace Hopper".to_string(),
            committer_email: "grace@example.invalid".to_string(),
            committer_date: 1_700_000_000,
            signature_status: ' ',
            signature_signer: String::new(),
            signature_key: String::new(),
            body: "subject line\n\nbody line one\nbody line two".to_string(),
        }
    }

    /// The two-step load the app performs: a loading panel, then the details.
    ///
    /// The runtime never builds a loaded commit panel in one shot — it starts in
    /// the loading state (`CdvState::loading`) and fills it via `loaded`, which
    /// is what `open_cdv` does. The tests drive that same transition so they
    /// cannot drift from the real path.
    fn loaded_commit(
        row: RowIdx,
        detail: files::CommitDetail,
        changes: Vec<GitFileChange>,
    ) -> CdvState {
        let hash = detail.hash.clone();
        let mut state = CdvState::loading(row, &hash, CdvKind::Commit, None);
        state.loaded(detail, changes);
        state
    }

    fn text_of(lines: &[Line<'static>]) -> String {
        lines
            .iter()
            .map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn the_loading_label_names_what_is_loading() {
        assert_eq!(CdvKind::Commit.loading_label(), "Commit Details");
        assert_eq!(CdvKind::Uncommitted.loading_label(), "Uncommitted Changes");
        assert_eq!(CdvKind::Comparison.loading_label(), "Commit Comparison");

        let state = CdvState::loading(0, "abc", CdvKind::Commit, None);
        let text = text_of(&state.lines());
        assert!(text.contains("Loading Commit Details ..."), "{text}");
    }

    #[test]
    fn vis_038_the_summary_shows_every_key_value_row() {
        let state = loaded_commit(0, detail(), vec![]);
        let text = text_of(&state.summary_lines());
        assert!(text.contains("Commit: abcdef1234567890"), "{text}");
        assert!(text.contains("Parents: 1111111, 2222222"), "{text}");
        assert!(
            text.contains("Author: Ada Lovelace <ada@example.invalid>"),
            "{text}"
        );
        assert!(
            text.contains("Committer: Grace Hopper <grace@example.invalid>"),
            "{text}"
        );
        assert!(
            !text.contains("Author Date:"),
            "equal dates collapse to one row:\n{text}"
        );
        assert!(text.contains("Date: 2023-11-14 22:13:20Z"), "{text}");
        assert!(text.contains("body line one"), "{text}");
    }

    #[test]
    fn vis_038_author_date_appears_only_when_it_differs() {
        let mut detail = detail();
        detail.author_date = 1_600_000_000;
        let state = loaded_commit(0, detail, vec![]);
        let text = text_of(&state.summary_lines());
        assert!(text.contains("Author Date: 2020-09-13 12:26:40Z"), "{text}");
        assert!(
            text.contains("Committer Date: 2023-11-14 22:13:20Z"),
            "{text}"
        );
    }

    #[test]
    fn vis_038_parents_read_none_for_a_root_commit() {
        let mut detail = detail();
        detail.parents.clear();
        let state = loaded_commit(0, detail, vec![]);
        assert!(text_of(&state.summary_lines()).contains("Parents: None"));
    }

    #[test]
    fn vis_038_an_absent_email_leaves_no_angle_brackets() {
        let mut detail = detail();
        detail.author_email.clear();
        let state = loaded_commit(0, detail, vec![]);
        let text = text_of(&state.summary_lines());
        assert!(text.contains("Author: Ada Lovelace\n"), "{text}");
        assert!(!text.contains("Ada Lovelace <>"), "{text}");
    }

    #[test]
    fn vis_038_a_signature_is_badged_with_its_description() {
        let mut detail = detail();
        detail.signature_status = 'G';
        detail.signature_signer = "Ada".to_string();
        let state = loaded_commit(0, detail, vec![]);
        let text = text_of(&state.summary_lines());
        assert!(text.contains('G'), "{text}");
        // `GIT_SIGNATURE_STATUS_DESCRIPTIONS['G']` is "Valid Signature"
        // (`web/utils.ts:48`), not the word "good".
        assert!(text.contains("Valid Signature"), "{text}");
    }

    #[test]
    fn vis_039_uncommitted_shows_the_reference_sentence() {
        let state = CdvState::uncommitted(3, vec![]);
        assert_eq!(
            text_of(&state.summary_lines()),
            "Displaying all uncommitted changes."
        );
        assert_eq!(state.hash, UNCOMMITTED);
    }

    #[test]
    fn vis_040_comparison_shows_the_reference_sentence() {
        let state = CdvState::comparison(0, "aaaaaaa", "bbbbbbb", vec![]);
        assert_eq!(
            text_of(&state.summary_lines()),
            "Displaying all changes from aaaaaaa to bbbbbbb."
        );

        let uncommitted = CdvState::comparison(0, "aaaaaaa", UNCOMMITTED, vec![]);
        assert_eq!(
            text_of(&uncommitted.summary_lines()),
            "Displaying all changes from aaaaaaa to Uncommitted Changes."
        );
    }

    #[test]
    fn vis_041_and_042_the_view_toggles_and_is_named() {
        let changes = vec![change(
            GitFileStatus::Modified,
            "src/main.rs",
            Some(3),
            Some(1),
        )];
        let mut state = loaded_commit(0, detail(), changes);
        assert_eq!(state.view, FileView::Tree);
        assert!(text_of(&state.lines()).contains("Tree View"));

        state.toggle_view();
        assert_eq!(state.view, FileView::List);
        assert!(text_of(&state.lines()).contains("List View"));
        state.toggle_view();
        assert_eq!(state.view, FileView::Tree);
    }

    #[test]
    fn vis_041_the_tree_nests_and_the_list_does_not() {
        let changes = vec![
            change(GitFileStatus::Modified, "src/main.rs", Some(1), Some(1)),
            change(GitFileStatus::Added, "src/render/mod.rs", Some(9), Some(0)),
            change(GitFileStatus::Deleted, "README.md", Some(0), Some(4)),
        ];
        let mut state = loaded_commit(0, detail(), changes);

        let tree = state.file_rows();
        assert!(
            tree.iter().any(|row| row.label == "src"),
            "the tree has a folder row: {tree:?}"
        );
        assert!(
            tree.iter()
                .any(|row| row.depth == 1 && row.label == "main.rs"),
            "the file is nested: {tree:?}"
        );

        state.toggle_view();
        let list = state.file_rows();
        assert!(
            list.iter().all(|row| row.depth == 0),
            "the list is flat: {list:?}"
        );
        assert!(list.iter().any(|row| row.label == "src/main.rs"));
    }

    #[test]
    fn vis_041_a_collapsed_folder_hides_its_files() {
        let changes = vec![change(
            GitFileStatus::Modified,
            "src/main.rs",
            Some(1),
            Some(1),
        )];
        let mut state = loaded_commit(0, detail(), changes);
        assert!(state.file_rows().iter().any(|row| row.label == "main.rs"));

        state.toggle_folder("src");
        assert!(!state.folder_open("src"));
        assert!(
            !state.file_rows().iter().any(|row| row.label == "main.rs"),
            "a closed folder hides its children"
        );
        state.toggle_folder("src");
        assert!(state.folder_open("src"));
        assert!(state.file_rows().iter().any(|row| row.label == "main.rs"));
    }

    #[test]
    fn vis_047_a_modified_text_file_shows_its_counts() {
        let changes = vec![change(
            GitFileStatus::Modified,
            "src/main.rs",
            Some(3),
            Some(1),
        )];
        let state = loaded_commit(0, detail(), changes);
        let text = text_of(&state.lines());
        assert!(text.contains("[M]"), "{text}");
        assert!(text.contains("(+3|-1)"), "{text}");
    }

    #[test]
    fn vis_047_added_and_deleted_files_show_no_counts() {
        let changes = vec![
            change(GitFileStatus::Added, "new.rs", Some(5), Some(0)),
            change(GitFileStatus::Deleted, "old.rs", Some(0), Some(5)),
        ];
        let state = loaded_commit(0, detail(), changes);
        let text = text_of(&state.lines());
        assert!(!text.contains("(+5|-0)"), "{text}");
        assert!(!text.contains("(+0|-5)"), "{text}");
    }

    #[test]
    fn vis_047_a_binary_file_says_it_cannot_be_diffed() {
        let changes = vec![change(GitFileStatus::Modified, "logo.png", None, None)];
        let state = loaded_commit(0, detail(), changes);
        let text = text_of(&state.lines());
        assert!(text.contains("Unable to View Diff"), "{text}");
        assert!(text.contains("this is a binary file"), "{text}");
    }

    #[test]
    fn vis_047_a_deleted_file_says_only_that_it_cannot_be_diffed() {
        let changes = vec![change(GitFileStatus::Deleted, "gone.bin", None, None)];
        let state = loaded_commit(0, detail(), changes);
        let text = text_of(&state.lines());
        assert!(text.contains("(Unable to View Diff)"), "{text}");
        assert!(!text.contains("binary"), "{text}");
    }

    #[test]
    fn vis_048_a_submodule_gets_its_own_marker() {
        let mut submodule = change(GitFileStatus::Modified, "vendor/lib", None, None);
        submodule.submodule = true;
        let state = loaded_commit(0, detail(), vec![submodule]);
        assert!(text_of(&state.lines()).contains("(submodule)"));
    }

    #[test]
    fn vis_052_an_untracked_file_is_diffable() {
        let changes = vec![change(GitFileStatus::Untracked, "scratch.txt", None, None)];
        assert!(changes[0].diff_possible(), "untracked files can be diffed");
        let state = loaded_commit(0, detail(), changes);
        let text = text_of(&state.lines());
        assert!(text.contains("[U]"), "{text}");
        assert!(!text.contains("Unable to View Diff"), "{text}");
    }

    #[test]
    fn a_renamed_file_shows_both_paths() {
        let mut renamed = change(GitFileStatus::Renamed, "src/new.rs", Some(1), Some(1));
        renamed.old_path = "src/old.rs".to_string();
        let state = loaded_commit(0, detail(), vec![renamed]);
        assert!(text_of(&state.lines()).contains("src/old.rs → src/new.rs"));
    }

    #[test]
    fn a_reviewed_file_is_marked() {
        let changes = vec![change(
            GitFileStatus::Modified,
            "src/main.rs",
            Some(1),
            Some(1),
        )];
        let mut state = loaded_commit(0, detail(), changes);
        assert!(!text_of(&state.lines()).contains("Reviewed"));
        state.toggle_reviewed("src/main.rs");
        assert!(state.is_reviewed("src/main.rs"));
        assert!(text_of(&state.lines()).contains("(Reviewed)"));
        state.toggle_reviewed("src/main.rs");
        assert!(!state.is_reviewed("src/main.rs"));
    }

    #[test]
    fn the_last_viewed_file_is_marked() {
        let changes = vec![change(
            GitFileStatus::Modified,
            "src/main.rs",
            Some(1),
            Some(1),
        )];
        let mut state = loaded_commit(0, detail(), changes);
        state.last_viewed_file = Some("src/main.rs".to_string());
        assert!(text_of(&state.lines()).contains("(Last Viewed)"));
    }

    #[test]
    fn an_empty_commit_says_so() {
        let state = loaded_commit(0, detail(), vec![]);
        let text = text_of(&state.lines());
        assert!(text.contains("Files (0)"), "{text}");
        assert!(text.contains("No file changes."), "{text}");
    }

    #[test]
    fn a_load_failure_replaces_the_summary() {
        let mut state = CdvState::loading(0, "abc", CdvKind::Commit, None);
        state.failed("bad object");
        assert!(!state.loading);
        let text = text_of(&state.lines());
        assert!(
            text.contains("Unable to load commit details: bad object"),
            "{text}"
        );
        assert!(state.status_line().contains("error"));
    }

    #[test]
    fn loaded_clears_the_loading_flag_and_fills_the_tree() {
        let mut state = CdvState::loading(0, "abc", CdvKind::Commit, None);
        assert!(state.lines()[0].spans[0].content.contains("Loading"));
        state.loaded(
            detail(),
            vec![change(GitFileStatus::Added, "src/lib.rs", Some(2), Some(0))],
        );
        assert!(!state.loading);
        assert!(state.lines()[0].spans[0].content.contains("Commit:"));
        let text = text_of(&state.lines());
        assert!(text.contains("Files (1)"), "{text}");
        // Tree view lists the file under its folder, so the row label is the
        // file *name* and the full path only appears in the list view.
        assert!(text.contains("src"), "{text}");
        assert!(text.contains("lib.rs"), "{text}");
        state.toggle_view();
        assert!(text_of(&state.lines()).contains("src/lib.rs"));
    }

    #[test]
    fn resizing_clamps_to_the_reference_bounds() {
        let mut state = loaded_commit(0, detail(), vec![]);
        assert_eq!(state.height, DEFAULT_HEIGHT);
        state.resize(100);
        assert_eq!(state.height, MAX_HEIGHT);
        state.resize(-100);
        assert_eq!(state.height, MIN_HEIGHT);
    }

    #[test]
    fn the_panel_never_swallows_the_commit_list() {
        let mut state = loaded_commit(0, detail(), vec![]);
        state.height = MAX_HEIGHT;
        let height = panel_height(&state, 24, 1);
        assert_eq!(height, 24 - 1 - MIN_LIST_ROWS);
        // A short screen still yields at least one row rather than zero.
        assert_eq!(panel_height(&state, 3, 1), 1);
    }

    #[test]
    fn the_status_line_names_the_kind_the_view_and_the_file_count() {
        let changes = vec![change(GitFileStatus::Added, "a.rs", Some(1), Some(0))];
        let mut state = loaded_commit(7, detail(), changes);
        state.toggle_view();
        let line = state.status_line();
        assert!(line.contains("Commit Details"), "{line}");
        assert!(line.contains("1 files"), "{line}");
        assert!(line.contains("List view"), "{line}");
        assert!(line.contains("row 7"), "{line}");
    }
}
