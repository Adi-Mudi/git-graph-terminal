//! git-graph-terminal — TUI scaffold with a graph renderer (W1 / lane b1).
//!
//! Event loop + terminal setup/teardown (alternate screen, raw mode), the graph
//! renderer in [`render`], and a status bar over the commit data from [`data`].
//!
//! W4 replaces the old ad-hoc `App` + `ratatui::ListState` handling with
//! [`app::AppState`] (owned selection / scroll / expand state) and a single,
//! testable [`keymap::action_for`] → [`app::reduce`] pipeline.
//!
//! Usage:
//! * `git-graph-terminal [REPO]` (default `.`) — borderless graph view over the
//!   real `data → layout → render` path.
//! * `git-graph-terminal --text-list [REPO]` — P0's bordered text list (fallback).
//! * `git-graph-terminal --mock [REPO]` — graph view on the hand-built mock fixture.
//! * `git-graph-terminal --dump [REPO]` — print one frame to stdout and exit
//!   (headless verification; a real repo dumps the text list, `--mock` the mock graph).
//!
//! Keys: `j`/`k`/↓/↑ move · ←/→ (Ctrl+↓/↑) walk the branch · PgUp/PgDn · Home/End ·
//! Enter expand · `s`/`S` stash next/prev (**no Shift = next**, R1.2) · `r` reload · `q`/Esc quit.

mod app;
mod data;
mod keymap;
mod layout;
mod render;

use std::io::{self, Stdout, Write};
use std::path::PathBuf;
use std::time::Duration;

use crossterm::event::{self, Event, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::app::{reduce, Action, AppState, Outcome};
use crate::render::glyphs::GlyphTier;
use crate::render::ViewOptions;

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

    let mut state = if mock {
        AppState::demo()
    } else {
        AppState::load(&repo)
    };

    // Capability tier + colour come from the environment (G2 minimal ladder).
    let tier = GlyphTier::detect_from_os();
    let colour = render::glyphs::colour_enabled_os();
    // `--mock` always shows the graph; a real repo hides it for `--text-list`
    // and for `--dump` (so the P0 bordered list frame stays reproducible).
    let show_graph = mock || !(text_list || dump);

    if dump {
        let view = make_view(&state, tier, colour, mock, show_graph);
        return dump_frame(&state, &view);
    }

    let mut terminal = setup_terminal()?;
    let _guard = TerminalGuard;

    loop {
        {
            // Refresh the viewport every frame so paging and the
            // follow-the-selection scroll know the real terminal height. The
            // graph body loses one row to the status bar and one to the table
            // header; the text list loses only the status bar.
            let size = terminal.size()?;
            state.status.width = size.width;
            let chrome = if show_graph { 2 } else { 1 };
            state.status.height = size.height.saturating_sub(chrome).max(1);

            let view = make_view(&state, tier, colour, mock, show_graph);
            terminal.draw(|frame| render::ui(frame, &state, &view))?;
        }

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
        if let Some(action) = keymap::action_for(key) {
            // Expansion only exists in the graph view; the P0 text list has no
            // detail pane, so ignore it there and keep the fallback untouched.
            if action == Action::ToggleExpand && !show_graph {
                continue;
            }
            if reduce(&mut state, action, &repo) == Outcome::Quit {
                break;
            }
        }
    }

    // Stop borrowing the terminal before the guard restores the screen.
    drop(terminal);
    let _ = io::stdout().flush();
    Ok(())
}

/// [`ViewOptions`] for one frame.
fn make_view<'a>(
    state: &'a AppState,
    tier: GlyphTier,
    colour: bool,
    mock: bool,
    show_graph: bool,
) -> ViewOptions<'a> {
    let mut view = ViewOptions {
        tier,
        colour,
        ..ViewOptions::TEXT_ONLY
    };
    view.graph = if show_graph {
        Some(&state.layout)
    } else {
        None
    };
    // Only the mock fixture carries an uncommitted row; `data.rs` does not model it.
    view.uncommitted = if mock { Some(0) } else { None };
    view
}

/// Headless verification path: render one frame into a test backend and print it.
///
/// The text view keeps P0's 100-column frame exactly. The graph view is dumped
/// wider so the whole status bar (repo + branch + head + counts + lanes + key
/// hints) fits on one line for review.
fn dump_frame(state: &AppState, view: &ViewOptions<'_>) -> io::Result<()> {
    let width = if view.graph.is_some() { 140 } else { 100 };
    let backend = ratatui::backend::TestBackend::new(width, 12);
    // `TestBackend` cannot fail; its error type is `Infallible`.
    let mut terminal =
        Terminal::new(backend).unwrap_or_else(|e: std::convert::Infallible| match e {});
    terminal
        .draw(|frame| render::ui(frame, state, view))
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
