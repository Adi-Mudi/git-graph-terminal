//! git-graph-terminal — TUI scaffold with a graph renderer (W1 / lane b1).
//!
//! Event loop + terminal setup/teardown (alternate screen, raw mode), the graph
//! renderer in [`render`], and a status bar over the commit data from [`data`].
//! W2 / F3 wires the real path: `data::commits` → `layout::layout()` → [`render`];
//! `--mock` draws the hand-built fixture from [`render::mock`] so tests stay
//! independent of git, and `--text-list` keeps the P0 bordered text list.
//!
//! Usage:
//! * `git-graph-terminal [REPO]` (default `.`) — borderless graph view over the
//!   real `data → layout → render` path.
//! * `git-graph-terminal --text-list [REPO]` — P0's bordered text list (fallback).
//! * `git-graph-terminal --mock [REPO]` — graph view on the hand-built mock fixture.
//! * `git-graph-terminal --dump [REPO]` — print one frame to stdout and exit
//!   (headless verification; a real repo dumps the text list, `--mock` the mock graph).

mod data;
mod layout;
mod render;
use std::io::{self, Stdout, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::widgets::ListState;
use ratatui::Terminal;

use crate::data::Commit;
use crate::layout::types::GraphLayout;
use crate::layout::LayoutOpts;
use crate::render::glyphs::GlyphTier;
use crate::render::ViewOptions;

/// Application state for the P0 view.
pub struct App {
    /// Repository the view is showing.
    pub repo: PathBuf,
    /// Current branch name, `None` on a detached HEAD.
    pub branch: Option<String>,
    /// Abbreviated HEAD object name, `None` for an unborn branch.
    pub head: Option<String>,
    /// Local branches (`refs/heads`), used for the branch count in the list title.
    pub branches: Vec<data::Branch>,
    /// Commits, newest first.
    pub commits: Vec<Commit>,
    /// Startup/refresh error shown in the status bar (the TUI stays alive).
    pub error: Option<String>,
}

impl App {
    /// Load (or reload) the view state for `repo`; never panics, errors are captured.
    pub fn load(repo: &Path) -> Self {
        let mut app = Self {
            repo: repo.to_path_buf(),
            branch: None,
            head: None,
            branches: Vec::new(),
            commits: Vec::new(),
            error: None,
        };
        if !data::is_repo(repo) {
            app.error = Some(format!("not a git repository: {}", repo.display()));
            return app;
        }
        match data::branches(repo) {
            Ok(branches) => app.branches = branches,
            Err(e) => app.error = Some(e.to_string()),
        }
        match data::commits(repo) {
            Ok(commits) => app.commits = commits,
            Err(e) => app.error = Some(e.to_string()),
        }
        match data::head(repo) {
            Ok(head) => {
                app.branch = head.branch;
                app.head = head.hash;
            }
            Err(e) if app.error.is_none() => app.error = Some(e.to_string()),
            Err(_) => {}
        }
        app
    }

    /// The `--mock` view: the hand-built fixture from [`render::mock`], so the
    /// renderer can be smoke-tested before lane a1's `layout()` lands.
    pub fn demo() -> Self {
        Self {
            repo: PathBuf::from("(mock)"),
            branch: Some("main".to_string()),
            head: Some("a1b2c3d".to_string()),
            branches: vec![
                data::Branch {
                    name: "main".to_string(),
                    head: "a1b2c3d".to_string(),
                    is_current: true,
                },
                data::Branch {
                    name: "feature/graph".to_string(),
                    head: "c3d4e5f".to_string(),
                    is_current: false,
                },
            ],
            commits: render::mock::demo_commits(),
            error: None,
        }
    }

    /// Repository path as shown in the UI (never an absolute server path — Rule 10).
    pub fn repo_label(&self) -> String {
        let repo = self
            .repo
            .canonicalize()
            .unwrap_or_else(|_| self.repo.clone());
        let label = std::env::current_dir()
            .ok()
            .and_then(|cwd| repo.strip_prefix(&cwd).ok().map(Path::to_path_buf))
            .unwrap_or_else(|| repo.clone());
        let text = label.to_string_lossy().to_string();
        if text.is_empty() {
            ".".to_string()
        } else {
            text
        }
    }

    /// Branch name for the status bar.
    pub fn branch_name(&self) -> &str {
        match &self.branch {
            Some(b) => b.as_str(),
            None => "(detached HEAD)",
        }
    }

    /// HEAD short hash for the status bar.
    pub fn head_short(&self) -> &str {
        self.head.as_deref().unwrap_or("(no commits)")
    }

    /// Move the selection down by one, clamped to the last commit.
    pub fn select_next(&self, state: &mut ListState) {
        let last = self.commits.len().saturating_sub(1);
        let next = state.selected().map(|i| (i + 1).min(last)).unwrap_or(0);
        if !self.commits.is_empty() {
            state.select(Some(next));
        }
    }

    /// Move the selection up by one, clamped to the first commit.
    pub fn select_previous(&self, state: &mut ListState) {
        let prev = state.selected().map(|i| i.saturating_sub(1)).unwrap_or(0);
        if !self.commits.is_empty() {
            state.select(Some(prev));
        }
    }
}

/// Restores the terminal on every exit path, including panics.
struct TerminalGuard;

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = restore_terminal();
    }
}

fn setup_terminal() -> io::Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    Terminal::new(CrosstermBackend::new(stdout))
}

fn restore_terminal() -> io::Result<()> {
    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen)?;
    Ok(())
}

fn main() -> io::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let dump = args.iter().any(|a| a == "--dump");
    let mock = args.iter().any(|a| a == "--mock");
    let text_list = args.iter().any(|a| a == "--text-list");
    let repo = args
        .iter()
        .find(|a| !a.starts_with('-'))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));

    let mut app = if mock { App::demo() } else { App::load(&repo) };

    // Capability tier + colour come from the environment (G2 minimal ladder).
    let tier = GlyphTier::detect_from_os();
    let colour = render::glyphs::colour_enabled_os();
    // W2 / F3: real repos go `data::commits → layout::layout() → render`. `--mock`
    // keeps the hand-built fixture for tests, `--text-list` keeps P0's bordered
    // list as the fallback, and a real-repo `--dump` prints that list so the P0
    // evidence frames stay reproducible (`--mock --dump` still dumps the mock graph).
    let mut graph: Option<GraphLayout> = build_graph(mock, text_list || dump, &app);

    if dump {
        let view = make_view(graph.as_ref(), tier, colour, mock);
        return dump_frame(&app, &view);
    }

    let mut terminal = setup_terminal()?;
    let _guard = TerminalGuard;
    let mut list_state = ListState::default();
    if !app.commits.is_empty() {
        list_state.select(Some(0));
    }

    loop {
        let view = make_view(graph.as_ref(), tier, colour, mock);
        terminal.draw(|frame| render::ui(frame, &app, &mut list_state, &view))?;

        if !event::poll(Duration::from_millis(250))? {
            continue;
        }
        let Event::Key(key) = event::read()? else {
            // Resize (and any other event): fall through and redraw with the new size.
            continue;
        };
        if key.kind != KeyEventKind::Press {
            continue;
        }
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => break,
            KeyCode::Char('j') | KeyCode::Down => app.select_next(&mut list_state),
            KeyCode::Char('k') | KeyCode::Up => app.select_previous(&mut list_state),
            KeyCode::Char('g') => {
                if !app.commits.is_empty() {
                    list_state.select(Some(0));
                }
            }
            KeyCode::Char('G') => {
                if !app.commits.is_empty() {
                    list_state.select(Some(app.commits.len() - 1));
                }
            }
            KeyCode::Char('r') => {
                let selected = list_state.selected();
                app = if mock { App::demo() } else { App::load(&repo) };
                // Rebuild the layout too, or the graph and the reloaded commits drift apart.
                graph = build_graph(mock, text_list || dump, &app);
                list_state.select(selected.filter(|i| *i < app.commits.len()).or(
                    if app.commits.is_empty() {
                        None
                    } else {
                        Some(0)
                    },
                ));
            }
            _ => {}
        }
    }

    // Stop borrowing the terminal before the guard restores the screen.
    drop(terminal);
    let _ = io::stdout().flush();
    Ok(())
}

/// The graph layout backing the current view, if any.
///
/// `mock` → the hand-built fixture; `text_only` (and not `mock`) → no graph, so
/// the P0 text list is drawn; otherwise the real path: the commits already loaded
/// into `app` go through the W1 layout engine.
fn build_graph(mock: bool, text_only: bool, app: &App) -> Option<GraphLayout> {
    if mock {
        Some(render::mock::demo_layout())
    } else if text_only {
        None
    } else {
        Some(layout::layout(&app.commits, &LayoutOpts::default()))
    }
}

/// [`ViewOptions`] for one frame.
fn make_view<'a>(
    graph: Option<&'a GraphLayout>,
    tier: GlyphTier,
    colour: bool,
    mock: bool,
) -> ViewOptions<'a> {
    let mut view = ViewOptions {
        tier,
        colour,
        ..ViewOptions::TEXT_ONLY
    };
    view.graph = graph;
    // Only the mock fixture carries an uncommitted row; `data.rs` does not model it.
    view.uncommitted = if mock { Some(0) } else { None };
    view
}

/// Headless verification path: render one frame into a test backend and print it.
///
/// The text view keeps P0's 100-column frame exactly. The graph view is dumped
/// wider so the whole status bar (repo + branch + head + counts + lanes + key
/// hints) fits on one line for review.
fn dump_frame(app: &App, view: &ViewOptions<'_>) -> io::Result<()> {
    let width = if view.graph.is_some() { 140 } else { 100 };
    let backend = ratatui::backend::TestBackend::new(width, 12);
    // `TestBackend` cannot fail; its error type is `Infallible`.
    let mut terminal =
        Terminal::new(backend).unwrap_or_else(|e: std::convert::Infallible| match e {});
    let mut list_state = ListState::default();
    if !app.commits.is_empty() {
        list_state.select(Some(0));
    }
    terminal
        .draw(|frame| render::ui(frame, app, &mut list_state, view))
        .unwrap_or_else(|e: std::convert::Infallible| match e {});

    let buffer = terminal.backend().buffer();
    let mut out = io::stdout();
    for y in 0..buffer.area.height {
        let mut row = String::new();
        for x in 0..buffer.area.width {
            row.push_str(buffer[(x, y)].symbol());
        }
        writeln!(out, "{}", row.trim_end())?;
    }
    Ok(())
}
