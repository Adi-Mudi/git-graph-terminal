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
//! 1. **Inline, under the commit row — the reference default.** The reference
//!    defaults to `commitDetailsView.location = Inline` (`src/config.ts:72-74`)
//!    and also supports `Docked to Bottom`. W12/T3 replaced the terminal's
//!    docked bottom pane with the reference's default layout: the panel is a
//!    block directly under its own commit row (`#cdv.inline`,
//!    `web/styles/main.css:299-302`). A ratatui `Table` cannot grow a
//!    variable-height row, so the table reserves the block's display rows and
//!    [`crate::render`] paints over them — [`inline_height`] decides the height
//!    and [`crate::app::AppState::block_parts`] makes the reservation. `Docked to
//!    Bottom` is the layout that is not ported.
//! 2. **`(Review)` markers are text.** The reference draws SVG eye/tick icons;
//!    here they are short text badges.
//! 3. **Signature shows as a badge + tooltip text.** `generateSignatureHtml`
//!    (`web/main.ts:3949-3959`) uses an icon with a `title`; the terminal shows
//!    the icon glyph and, on the same line, the description.
//! 4. **The pane always splits.** Mockup v3 lays the panel out as
//!    `.pane{display:grid;grid-template-columns:1fr 1fr}` — commit information
//!    left, modified files right. The port keeps those two columns at every
//!    width instead of falling back to a stacked column: one layout path keeps
//!    [`inline_height`] a pure function of the content, so it needs no width.
//!    A very narrow terminal therefore gets two tight columns rather than a
//!    stacked pane.

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::render::glyphs::GlyphSet;
use crate::term::ladder::Ladder;

use crate::files::{self, FileRow, FileRowKind, FileTree, FileView, GitFileChange, GitFileStatus};
use crate::layout::types::RowIdx;
use crate::render::columns::long_date;

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
    /// The commit's refs, as git's `%D` decoration list.
    ///
    /// The reference reads `gitBranch[]` / `tag[]` off the commit object; the
    /// port reuses the decorations the commit table already carries
    /// (`src/data.rs`). The left column splits them into the mockup's `Branch`
    /// and `Tags` rows (mockup v3, `.pane .col` #1). Empty for the uncommitted
    /// and comparison kinds, which have no single commit behind them.
    pub refs: Vec<String>,
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
    /// `commitDetailsView.autoCenter` — kept for parity; unused while inline.
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
            refs: Vec::new(),
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
            refs: Vec::new(),
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
            refs: Vec::new(),
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

    /// Move the cursor to the file row for `path`, if the panel lists it (W10).
    ///
    /// A mouse click is both a selection and the subject of the `FileRow` menu,
    /// and that menu reads the row under the cursor — so the click has to move
    /// the cursor, or the menu would act on a different file. Folder rows are
    /// skipped: they are not a `FileRow` menu target.
    pub fn select_path(&mut self, path: &str) -> bool {
        let Some(index) = self
            .file_rows()
            .iter()
            .position(|row| matches!(row.kind, FileRowKind::File(_)) && row.path == path)
        else {
            return false;
        };
        self.selected = index;
        true
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
        // The file cursor lives in the **right** column, whose own header is the
        // single `Files (N) — <view> View` line — not under the summary.
        let header: u16 = 1;
        let line = header.saturating_add(self.selected as u16);
        if line < self.scroll {
            self.scroll = line;
        } else if line >= self.scroll.saturating_add(viewport) {
            self.scroll = line + 1 - viewport;
        }
    }

    /// The `Branch` row's value (mockup v3, `.pane .col` #1).
    ///
    /// `%D` reports the checked-out branch as `HEAD -> main` and a detached
    /// HEAD as a bare `HEAD`; the mockup lists the branch *name*, so the
    /// decoration prefix comes off and a bare `HEAD` is dropped. A commit on no
    /// branch at all shows the mockup's dash.
    pub fn branches_label(&self) -> String {
        let branches: Vec<String> = self
            .refs
            .iter()
            .filter(|entry| !entry.starts_with("tag: "))
            .map(|entry| entry.strip_prefix("HEAD -> ").unwrap_or(entry))
            .filter(|entry| *entry != "HEAD")
            .map(str::to_string)
            .collect();
        join_or_dash(&branches)
    }

    /// The `Tags` row's value — git's `tag: v1` decorations, prefix stripped.
    pub fn tags_label(&self) -> String {
        let tags: Vec<String> = self
            .refs
            .iter()
            .filter_map(|entry| entry.strip_prefix("tag: "))
            .map(str::to_string)
            .collect();
        join_or_dash(&tags)
    }

    /// The summary lines (`web/main.ts:2539-2575`) — VIS-038/039/040.
    ///
    /// This is the **left** column of the mockup's pane; the right one is
    /// [`CdvState::file_lines`].
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
        // Mockup v3, `.pane .col` #1: `Branch` then `Tags`, between the dates
        // and the message. The reference has no such rows — it draws the refs
        // as graph decorations — so the values come from the decorations this
        // commit already carries.
        lines.push(kv(
            &key,
            "Branch: ",
            &self.branches_label(),
            Style::default().fg(Color::Cyan),
        ));
        lines.push(kv(
            &key,
            "Tags: ",
            &self.tags_label(),
            Style::default().fg(Color::Yellow),
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

    /// The loading line — the whole panel while the details are in flight.
    fn loading_line(&self) -> Line<'static> {
        Line::styled(
            format!("⏳ Loading {} ...", self.kind.loading_label()),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
    }

    /// The **left** column as drawn: the spinner while loading, else the
    /// commit information (mockup v3, `.pane .col` #1).
    fn left_column(&self) -> Vec<Line<'static>> {
        if self.loading {
            return vec![self.loading_line()];
        }
        self.summary_lines()
    }

    /// The **right** column as drawn: empty while loading, so the spinner is
    /// never followed by a file list that has not arrived yet (mockup v3,
    /// `.pane .col` #2).
    fn right_column(&self) -> Vec<Line<'static>> {
        if self.loading {
            return Vec::new();
        }
        self.file_lines()
    }

    /// The **right** column of the mockup's pane: the modified-file list, then
    /// the totals and the diff-summary sentence.
    pub fn file_lines(&self) -> Vec<Line<'static>> {
        let mut lines = Vec::new();
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
        lines.push(Line::default());
        lines.push(self.totals_line());
        lines.push(self.diff_summary_line());
        lines
    }

    /// `(files changed, additions, deletions)` — the mockup's `.totals`.
    ///
    /// A binary file carries no counts, so it counts towards the file total but
    /// adds nothing to the line counts. That is the reference's `textFile` test
    /// (`web/main.ts:3584`), the same one [`GitFileChange::text_file`] ports.
    pub fn totals(&self) -> (usize, u32, u32) {
        let mut additions = 0;
        let mut deletions = 0;
        for change in &self.changes {
            if !change.text_file() {
                continue;
            }
            additions += change.additions.unwrap_or(0);
            deletions += change.deletions.unwrap_or(0);
        }
        (self.changes.len(), additions, deletions)
    }

    /// The mockup's totals line: `Total files changed: 2 · additions: +12 ·
    /// deletions: −3`.
    pub fn totals_line(&self) -> Line<'static> {
        let (files, additions, deletions) = self.totals();
        let dim = Style::default().fg(Color::DarkGray);
        Line::from(vec![
            Span::styled("Total files changed: ", dim),
            Span::styled(files.to_string(), Style::default()),
            Span::styled(" · additions: ", dim),
            Span::styled(format!("+{additions}"), Style::default().fg(Color::Green)),
            Span::styled(" · deletions: ", dim),
            Span::styled(format!("−{deletions}"), Style::default().fg(Color::Red)),
        ])
    }

    /// The mockup's diff-summary sentence: `Diff summary: 2 files changed, 12
    /// insertions(+), 3 deletions(−)`.
    pub fn diff_summary_line(&self) -> Line<'static> {
        let (files, additions, deletions) = self.totals();
        Line::styled(
            format!(
                "Diff summary: {files} files changed, {additions} insertions(+), {deletions} deletions(−)"
            ),
            Style::default().fg(Color::DarkGray),
        )
    }

    /// Every line the panel draws, stacked — the left column, a blank, then the
    /// right column.
    ///
    /// The panel draws the two columns side by side (mockup v3
    /// `.pane{grid-template-columns:1fr 1fr}`), so this stacked form is not what
    /// is painted: it is the test-facing text view of the whole panel, in the
    /// order a stacked terminal would read. `--dump` reports
    /// [`CdvState::status_line`] instead.
    #[cfg(test)]
    pub fn lines(&self) -> Vec<Line<'static>> {
        let mut lines = self.left_column();
        let right = self.right_column();
        if !right.is_empty() {
            lines.push(Line::default());
            lines.extend(right);
        }
        lines
    }

    /// The block title the mockup draws above the pane.
    ///
    /// Mockup v3 (Q1) heads the expanded block with `├─ Commit Details — same
    /// design, expanded in place`. The loading and error states have no mockup
    /// text, so they name themselves instead of claiming the pane is loaded.
    /// [`CdvState::status_line`] still carries the full state for `--dump`.
    ///
    /// The `├─` opener comes from `glyphs`, so an ASCII-rung terminal gets
    /// `+-` instead of a box-drawing tee it cannot render.
    pub fn block_title(&self, glyphs: &GlyphSet) -> String {
        let suffix = if self.loading {
            "loading…"
        } else if self.error.is_some() {
            "error"
        } else {
            "same design, expanded in place"
        };
        format!(
            "{}{} {} — {suffix}",
            glyphs.tee_missing_left,
            glyphs.horizontal,
            self.kind.title()
        )
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

/// The mockup's dash placeholder for an empty `Branch` / `Tags` value.
fn join_or_dash(items: &[String]) -> String {
    if items.is_empty() {
        "—".to_string()
    } else {
        items.join(", ")
    }
}

/// The commit list keeps at least this many rows, so the inline panel can never
/// swallow the whole table (M4-W12).
pub const MIN_LIST_ROWS: u16 = 4;

/// The height the panel needs when it is drawn **inline**, under its commit row
/// (M4-W12).
///
/// The two columns sit side by side, so the panel is as tall as the **taller**
/// column — not their sum. The inline panel is part of the flow, so it asks for
/// exactly what its content needs. It is still capped by [`MIN_LIST_ROWS`] so it
/// can never swallow the whole body.
pub fn inline_height(state: &CdvState, body_height: u16) -> u16 {
    let content = state.left_column().len().max(state.right_column().len()) as u16 + 2;
    let cap = body_height.saturating_sub(MIN_LIST_ROWS).max(1);
    content.min(cap).max(1)
}

/// Draw the panel into `area` — the rect its commit row reserved for it.
///
/// The frame follows the ladder, so an ASCII-only terminal gets `+-|`.
pub fn draw(frame: &mut Frame, area: Rect, state: &CdvState, focused: bool, ladder: Ladder) {
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
        .border_set(ladder.border_set())
        .border_style(border)
        .title(format!(" {} ", state.block_title(ladder.glyphs())));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Mockup v3: `.pane{display:grid;grid-template-columns:1fr 1fr}` — commit
    // information on the left, modified files on the right. Both columns share
    // one scroll offset, so scrolling the file list moves the summary with it.
    let columns =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).split(inner);
    frame.render_widget(
        Paragraph::new(state.left_column())
            .scroll((state.scroll, 0))
            .style(Style::default()),
        columns[0],
    );
    frame.render_widget(
        Paragraph::new(state.right_column())
            .scroll((state.scroll, 0))
            .style(Style::default()),
        columns[1],
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::UNCOMMITTED;
    use crate::render::glyphs::{ASCII, UNICODE};

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
    fn the_panel_never_swallows_the_commit_list() {
        let state = loaded_commit(0, detail(), vec![]);
        // A tall body leaves room for the panel plus `MIN_LIST_ROWS` of table.
        let height = inline_height(&state, 24);
        assert!(height <= 24 - MIN_LIST_ROWS, "{height}");
        // A short body still yields at least one row rather than zero.
        assert_eq!(inline_height(&state, 3), 1);
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

    /// Q1 — the block title is the mockup's lead line, while `status_line` keeps
    /// carrying the full state for `--dump`.
    #[test]
    fn the_block_title_matches_the_mockup() {
        let changes = vec![change(GitFileStatus::Added, "a.rs", Some(1), Some(0))];
        let state = loaded_commit(7, detail(), changes);
        assert_eq!(
            state.block_title(&UNICODE),
            "├─ Commit Details — same design, expanded in place"
        );

        let loading = CdvState::loading(0, "a1b2c3d", CdvKind::Commit, None);
        assert_eq!(
            loading.block_title(&UNICODE),
            "├─ Commit Details — loading…"
        );
    }

    /// The block title's `├─` opener follows the ladder, so an ASCII-rung
    /// terminal gets `+-` rather than a box-drawing tee it cannot render.
    #[test]
    fn the_block_title_opener_follows_the_glyph_rung() {
        let state = loaded_commit(0, detail(), vec![]);
        assert_eq!(
            state.block_title(&ASCII),
            "+- Commit Details — same design, expanded in place"
        );
    }

    /// Mockup v3, `.pane .col` #1: `Branch` and `Tags` sit between the dates
    /// and the message.
    #[test]
    fn the_summary_carries_the_mockup_branch_and_tags_rows() {
        let mut state = loaded_commit(0, detail(), vec![]);
        state.refs = vec!["HEAD -> main".to_string(), "tag: v1".to_string()];

        let text = text_of(&state.summary_lines());
        assert!(text.contains("Branch: main"), "{text}");
        assert!(text.contains("Tags: v1"), "{text}");
        let branch = text.find("Branch: ").expect("a Branch row");
        let body = text.find("body line one").expect("the commit body");
        assert!(branch < body, "the rows precede the message: {text}");
    }

    /// A commit on no branch and with no tag shows the mockup's dash, and a
    /// detached `HEAD` is not a branch name.
    #[test]
    fn a_commit_with_no_refs_shows_a_dash_for_branch_and_tags() {
        let mut state = loaded_commit(0, detail(), vec![]);
        let text = text_of(&state.summary_lines());
        assert!(text.contains("Branch: —"), "{text}");
        assert!(text.contains("Tags: —"), "{text}");

        state.refs = vec!["HEAD".to_string()];
        assert_eq!(
            state.branches_label(),
            "—",
            "a detached HEAD is not a branch"
        );
    }

    /// Several refs of each kind join, and the `HEAD -> ` decoration comes off.
    #[test]
    fn branch_and_tag_labels_split_the_decorations() {
        let mut state = loaded_commit(0, detail(), vec![]);
        state.refs = vec![
            "HEAD -> development".to_string(),
            "origin/development".to_string(),
            "tag: v1".to_string(),
            "tag: v2".to_string(),
        ];
        assert_eq!(state.branches_label(), "development, origin/development");
        assert_eq!(state.tags_label(), "v1, v2");
    }

    /// The mockup's `.totals`: counts sum over text files only — a binary file
    /// counts towards the file total but carries no line counts
    /// (`GitFileChange::text_file`).
    #[test]
    fn the_totals_sum_text_files_and_skip_binaries() {
        let changes = vec![
            change(GitFileStatus::Modified, "src/main.rs", Some(12), Some(3)),
            change(GitFileStatus::Added, "src/lib.rs", Some(4), Some(0)),
            change(GitFileStatus::Modified, "logo.png", None, None),
        ];
        let state = loaded_commit(0, detail(), changes);
        assert_eq!(state.totals(), (3, 16, 3));
    }

    /// The totals line and the diff-summary sentence close the right column.
    #[test]
    fn the_right_column_ends_with_the_totals_and_the_diff_summary() {
        let changes = vec![
            change(GitFileStatus::Modified, "src/main.rs", Some(12), Some(3)),
            change(GitFileStatus::Modified, "src/lib.rs", Some(4), Some(1)),
        ];
        let state = loaded_commit(0, detail(), changes);
        let text = text_of(&state.file_lines());
        assert!(text.contains("Files (2) — Tree View"), "{text}");
        assert!(
            text.contains("Total files changed: 2 · additions: +16 · deletions: −4"),
            "{text}"
        );
        assert!(
            text.contains("Diff summary: 2 files changed, 16 insertions(+), 4 deletions(−)"),
            "{text}"
        );
    }

    /// An empty commit still reports its totals, so the right column has the
    /// same shape everywhere.
    #[test]
    fn an_empty_commit_still_reports_zero_totals() {
        let state = loaded_commit(0, detail(), vec![]);
        let text = text_of(&state.file_lines());
        assert!(text.contains("No file changes."), "{text}");
        assert!(text.contains("Total files changed: 0"), "{text}");
        assert!(
            text.contains("Diff summary: 0 files changed, 0 insertions(+), 0 deletions(−)"),
            "{text}"
        );
    }

    /// The columns sit side by side, so the panel is as tall as the **taller**
    /// column — not their sum (mockup v3 `.pane`).
    #[test]
    fn the_inline_height_is_the_taller_column_not_their_sum() {
        let changes = vec![change(
            GitFileStatus::Modified,
            "src/main.rs",
            Some(1),
            Some(1),
        )];
        let state = loaded_commit(0, detail(), changes);
        let left = state.summary_lines().len() as u16;
        let right = state.file_lines().len() as u16;
        assert!(left > 1 && right > 1, "left {left} right {right}");
        // A body tall enough that neither the max nor the sum is clamped.
        let tall = left + right + MIN_LIST_ROWS + 2;
        assert_eq!(inline_height(&state, tall), left.max(right) + 2);
        assert!(inline_height(&state, tall) < left + right + 2);
    }

    /// While loading there is no file list to draw beside the spinner.
    #[test]
    fn the_loading_panel_draws_one_column() {
        let state = CdvState::loading(0, "abc", CdvKind::Commit, None);
        assert_eq!(state.lines().len(), 1);
        let text = text_of(&state.lines());
        assert!(text.contains("Loading Commit Details ..."), "{text}");
    }
}
