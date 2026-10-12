//! git-graph-terminal — TUI scaffold with a graph renderer (W1 / lane b1).
//!
//! Event loop + terminal setup/teardown (alternate screen, raw mode, mouse
//! capture), the graph renderer in [`render`], and a status bar over the commit
//! data from [`data`].
//!
//! W4 replaced the old ad-hoc `App` + `ratatui::ListState` handling with
//! [`app::AppState`] (owned selection / scroll / expand state) and a single,
//! testable [`keymap::action_for`] → [`app::reduce`] pipeline.
//!
//! W5 adds the **context menu**: `m` / `Shift+F10` / right-click opens the menu
//! for the selected row (or, on the table header, the column menu); arrows and
//! `j`/`k` move inside it, `Enter` or a row's shortcut letter activates, `Esc`
//! closes without activating. Copy rows hand their text to the OSC 52 bridge,
//! which only writes to stdout when `GGT_OSC52=1`.
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

mod actions;
mod app;
mod branches;
mod cdv;
mod data;
mod dialog;
mod diff;
mod editor;
mod files;
mod find;
mod keymap;
mod layout;
mod menu;
mod pager;
mod render;
mod term;
mod uncommitted;
mod version;

use std::io::{self, Stdout, Write};
use std::path::PathBuf;
use std::time::Duration;

use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyEventKind, MouseButton, MouseEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::app::{
    dialog_key, find_key, pager_nav, reduce, Action, AppState, ClickTarget, MenuKey, MenuNav,
    Outcome,
};
use crate::branches::RefKind;
use crate::dialog::DialogResult;
use crate::menu::MenuSurface;
use crate::render::ViewOptions;
use crate::term::ladder::Ladder;
use crate::term::TermCaps;

/// Restores the terminal on every exit path, including panics.
///
/// W11/T1: the guard carries the probe result so it only *leaves* the modes
/// [`setup_terminal`] actually entered. A Dumb host never got `smcup`, so it is
/// never sent `rmcup` either.
struct TerminalGuard {
    caps: TermCaps,
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = restore_terminal(&self.caps);
    }
}

/// Does this probe result allow the alternate screen?
///
/// W8 kept `EnterAlternateScreen` unconditional; W11/T1 (the recorded backlog)
/// skips it on the Dumb tier, where the full-screen TUI is not drawable and the
/// app falls back to the plain commit list instead. Every other tier keeps the
/// W8 policy: always alt-screen, always restore.
fn enters_alt_screen(caps: &TermCaps) -> bool {
    !caps.dumb
}

/// Write the enter escapes for `caps` to `out` (raw mode is separate).
///
/// Split from [`setup_terminal`] so a test can read the exact byte stream and
/// assert the Dumb tier emits no `smcup` (`ESC[?1049h`).
fn write_enter_sequences<W: Write>(out: &mut W, caps: &TermCaps) -> io::Result<()> {
    if enters_alt_screen(caps) {
        execute!(out, EnterAlternateScreen)?;
    }
    // W8/T3: only ask for SGR mouse reporting when the probe proved the terminal
    // speaks it. A `dumb`/`vt100` host must never receive the escape — an
    // unrecognised sequence is printed as literal text on some terminals.
    if caps.mouse {
        execute!(out, EnableMouseCapture)?;
    }
    Ok(())
}

/// Write the leave escapes for `caps` to `out` (raw mode is separate).
fn write_leave_sequences<W: Write>(out: &mut W, caps: &TermCaps) -> io::Result<()> {
    if enters_alt_screen(caps) {
        execute!(out, LeaveAlternateScreen)?;
    }
    // Disabling a mode that was never enabled is a no-op escape, so the guard
    // always restores the terminal.
    execute!(out, DisableMouseCapture)?;
    Ok(())
}

fn setup_terminal(caps: &TermCaps) -> io::Result<Terminal<CrosstermBackend<Stdout>>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    write_enter_sequences(&mut stdout, caps)?;
    Terminal::new(CrosstermBackend::new(stdout))
}

fn restore_terminal(caps: &TermCaps) -> io::Result<()> {
    disable_raw_mode()?;
    write_leave_sequences(&mut io::stdout(), caps)?;
    Ok(())
}

/// Hand the terminal to the staged editor, then resume the TUI (T6).
///
/// The alternate screen and raw mode come off first so the editor owns a normal
/// terminal, and go back on afterwards. `Ok(None)` means the editor exited;
/// `Ok(Some(message))` carries a spawn failure to the status bar, because a
/// missing `nvim` must not take the whole UI down with it.
fn run_editor(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    caps: &TermCaps,
    plan: &editor::EditorPlan,
) -> io::Result<Option<String>> {
    let _ = restore_terminal(caps);
    let outcome = editor::run(plan);
    // The temp dir goes whatever the editor did, so a failed spawn cannot leak
    // the two staged files (T6).
    editor::cleanup(plan);
    *terminal = setup_terminal(caps)?;
    terminal.clear()?;
    match outcome {
        Ok(_) => Ok(None),
        Err(error) => Ok(Some(format!("editor failed: {error}"))),
    }
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

    // T3: the release identity, printed before anything touches the repository
    // (so `--version` works even outside a git checkout).
    if args.iter().any(|a| a == "--version" || a == "-V") {
        println!("{}", version::version_line());
        return Ok(());
    }
    // T2: the README keybindings table, generated from `keymap::BINDINGS`.
    if args.iter().any(|a| a == "--keybindings") {
        print!("{}", keymap::keybindings_markdown());
        return Ok(());
    }

    let mut state = if mock {
        AppState::demo()
    } else {
        AppState::load(&repo)
    };

    // Capability probe (M3-W8): one read of the environment builds the ladder
    // the whole frame degrades through. `--caps` prints the probe and exits.
    let caps = TermCaps::detect_from_os();
    let ladder = Ladder::from_caps(&caps);
    if args.iter().any(|a| a == "--caps") {
        println!("{}", caps.summary());
        println!("ladder={}", ladder.label());
        return Ok(());
    }
    // `--mock` always shows the graph; a real repo hides it for `--text-list`
    // and for `--dump` (so the P0 bordered list frame stays reproducible).
    let graph_requested = mock || !(text_list || dump);
    // W11/T1: a Dumb terminal cannot place the cursor, so the full-screen graph
    // is not drawable. Fall back to the P0 plain list and never enter the
    // alternate screen. `--dump` is headless (it prints one frame and exits), so
    // it keeps rendering the requested view for reproducible evidence.
    if graph_requested && caps.dumb && !dump {
        eprintln!(
            "git-graph-terminal: TERM={} is a dumb terminal — falling back to the plain commit list (no alternate screen).",
            caps.term.as_deref().unwrap_or("-")
        );
    }
    let show_graph = graph_requested && (!caps.dumb || dump);
    // Rows the graph view spends outside the commit body: the status bar, plus
    // Chrome rows around the graph body: the tab bar, the table header the
    // graph pane has no counterpart for, and the status bar.
    let chrome = if show_graph { 3 } else { 2 };

    if dump {
        let view = make_view(&state, ladder, mock, show_graph);
        return dump_frame(&state, &view);
    }

    let mut terminal = setup_terminal(&caps)?;
    let _guard = TerminalGuard { caps: caps.clone() };

    loop {
        // A staged editor launch takes the terminal before the next draw, then
        // the loop resumes with the selection and scroll untouched (T6).
        if let Some(plan) = state.pending_editor.take() {
            if let Some(message) = run_editor(&mut terminal, &caps, &plan)? {
                state.status.error = Some(message);
            }
            continue;
        }
        {
            // Refresh the viewport every frame so paging and the
            // follow-the-selection scroll know the real terminal height.
            let size = terminal.size()?;
            state.status.width = size.width;
            state.status.height = size.height.saturating_sub(chrome).max(1);

            let view = make_view(&state, ladder, mock, show_graph);
            terminal.draw(|frame| render::ui(frame, &state, &view))?;
        }

        // Everything the mouse router needs for this frame: the full screen
        // rect (for menu clamping) and the main-area geometry.
        let screen_height = state.status.height.saturating_add(chrome);
        let screen = ratatui::layout::Rect {
            x: 0,
            y: 0,
            width: state.status.width,
            height: screen_height,
        };
        let view = make_view(&state, ladder, mock, show_graph);
        let hit = render::hit_geometry(&state, &view, screen_height);

        if !event::poll(Duration::from_millis(250))? {
            continue;
        }
        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                // Overlay ownership, outermost first. The pager and the dialog
                // are modal, so they claim the keyboard before anything else
                // (W7/T1, W7/T3); the menu and the find widget come next, and
                // the focused CDV only takes the file-list keys, so the W6
                // chords (`Tab`, `Ctrl+W`, `v`, `+`/`-`) keep working.
                if state.pager_is_open() {
                    if let Some(nav) = keymap::pager_nav_for(key) {
                        pager_nav(&mut state, nav, pager::viewport(screen_height));
                    }
                    continue;
                }
                if state.dialog_is_open() {
                    if let Some(dialog_key_event) = keymap::dialog_key_for(key) {
                        match dialog_key(&mut state, dialog_key_event) {
                            Some(DialogResult::Submit { .. }) => {
                                if reduce(&mut state, Action::DialogConfirm, &repo) == Outcome::Quit
                                {
                                    break;
                                }
                            }
                            Some(DialogResult::Cancel) => {
                                reduce(&mut state, Action::DialogCancel, &repo);
                            }
                            None => {}
                        }
                    }
                    continue;
                }
                if state.menu_is_open() {
                    // The overlay owns the keyboard until it closes (Esc).
                    if let Some(nav) = keymap::menu_nav_for(key) {
                        if state.menu_nav(nav) == MenuKey::Activated {
                            state.activate_menu(&repo);
                            flush_clipboard(&mut state)?;
                        }
                    }
                    continue;
                }
                // The find widget is up: a plain character is query text. The
                // `Ctrl`/`Alt` chords return `None` here and fall through to
                // [`keymap::action_for`], so the W6 toggles still work.
                if state.find.visible {
                    if let Some(find_key_event) = keymap::find_key_for(key) {
                        find_key(&mut state, find_key_event, &repo);
                        continue;
                    }
                }
                if show_graph && keymap::opens_menu(key) {
                    state.open_menu_for_selection(0, 1);
                    continue;
                }
                // `Tab` has given the CDV the keyboard: its file list owns the
                // row-movement and row-action keys (W7/T3).
                if state.cdv_focused && state.cdv.is_some() {
                    if let Some(action) = keymap::cdv_key_for(key) {
                        if reduce(&mut state, action, &repo) == Outcome::Quit {
                            break;
                        }
                        continue;
                    }
                }
                if let Some(action) = keymap::action_for(key) {
                    // Expansion only exists in the graph view; the P0 text list
                    // has no detail pane, so keep the fallback untouched.
                    if action == Action::ToggleExpand && !show_graph {
                        continue;
                    }
                    if reduce(&mut state, action, &repo) == Outcome::Quit {
                        break;
                    }
                }
            }
            // Mouse events only arrive while capture is on (graph mode).
            Event::Mouse(mouse) if show_graph => {
                let inside_menu = state
                    .menu
                    .as_ref()
                    .map(|menu| menu.contains(mouse.column, mouse.row, screen))
                    .unwrap_or(false);
                match mouse.kind {
                    MouseEventKind::Down(MouseButton::Right) => {
                        if inside_menu {
                            state.menu = None;
                        } else {
                            open_menu_at(&mut state, hit.as_ref(), mouse.column, mouse.row);
                        }
                    }
                    MouseEventKind::Down(MouseButton::Left) if inside_menu => {
                        // Click-to-activate: highlight the row under the pointer.
                        let rect = state.menu.as_ref().map(|menu| menu.rect(screen));
                        if let (Some(menu), Some(rect)) = (state.menu.as_mut(), rect) {
                            let index = mouse.row.saturating_sub(rect.y).saturating_sub(1) as usize;
                            if index < menu.items.len() {
                                menu.selected = index;
                            }
                        }
                        if state.menu_nav(MenuNav::Activate) == MenuKey::Activated {
                            state.activate_menu(&repo);
                            flush_clipboard(&mut state)?;
                        }
                    }
                    MouseEventKind::Down(MouseButton::Left) => {
                        if let Some(hit) = hit.as_ref() {
                            match state.surface_at(mouse.column, mouse.row, hit) {
                                Some(ClickTarget::Tab(tab)) => app::set_tab(&mut state, tab),
                                Some(ClickTarget::Row(row, _, _)) => state.selected = row,
                                // A click on a branch row moves the branch cursor
                                // and filters the history, the same gesture as
                                // `Enter` on that row (T2).
                                Some(ClickTarget::Branch(index)) => {
                                    state.branch_selected = index;
                                    app::apply_branch_filter(&mut state, &repo);
                                }
                                // A click inside the inline CDV moves its cursor.
                                Some(ClickTarget::CdvFile(path)) => {
                                    if let Some(cdv) = state.cdv.as_mut() {
                                        cdv.select_path(&path);
                                    }
                                }
                                // A folder click flips it open/closed (INT-050).
                                Some(ClickTarget::CdvFolder(path)) => {
                                    app::toggle_cdv_folder(&mut state, &path);
                                }
                                _ => {}
                            }
                        }
                    }
                    // The wheel scrolls the surface under the pointer (T7).
                    MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                        let over_pane =
                            hit.as_ref()
                                .and_then(|hit| hit.branch_pane)
                                .is_some_and(|pane| {
                                    mouse.column < pane.x.saturating_add(pane.width)
                                        && mouse.row >= pane.y
                                        && mouse.row < pane.y.saturating_add(pane.height)
                                });
                        let delta = if mouse.kind == MouseEventKind::ScrollDown {
                            app::WHEEL_LINES
                        } else {
                            -app::WHEEL_LINES
                        };
                        app::wheel_scroll(&mut state, delta, over_pane);
                    }
                    _ => {}
                }
            }
            // Resize (and any other event): redraw with the new size.
            _ => {}
        }
    }

    // Stop borrowing the terminal before the guard restores the screen.
    drop(terminal);
    let _ = io::stdout().flush();
    Ok(())
}

/// Open the menu for whatever the pointer is over, or close it on empty space.
fn open_menu_at(state: &mut AppState, hit: Option<&app::HitGeometry>, x: u16, y: u16) {
    let target = hit.and_then(|hit| state.surface_at(x, y, hit));
    match target {
        Some(ClickTarget::Tab(_)) => state.menu = None,
        Some(ClickTarget::ColumnHeader) => {
            state.open_menu(MenuSurface::ColumnHeader, None, x, y);
        }
        Some(ClickTarget::Row(row, surface, name)) => {
            state.selected = row;
            state.open_menu(surface, Some(&name), x, y);
        }
        // A branch row keeps its checkout / merge / rebase / delete menu (T2).
        Some(ClickTarget::Branch(index)) => {
            state.branch_selected = index;
            let entry = state.branches.get(index);
            let (surface, name) = match entry {
                Some(entry) => (branch_surface(entry.kind), entry.name.clone()),
                None => (MenuSurface::Branch, String::new()),
            };
            state.open_menu(surface, Some(&name), x, y);
        }
        // The `FileRow` menu acts on the row under the cursor, so the click has
        // to move the cursor first (W10).
        Some(ClickTarget::CdvFile(path)) => {
            if let Some(cdv) = state.cdv.as_mut() {
                cdv.select_path(&path);
            }
            state.open_menu(MenuSurface::FileRow, Some(&path), x, y);
        }
        // A folder has no `FileRow` menu, so a right-click opens nothing — but
        // it still must not fall through to the commit-row menu (INT-050).
        Some(ClickTarget::CdvFolder(_)) => state.menu = None,
        None => state.menu = None,
    }
}

/// The context-menu surface a ref kind opens (T2).
fn branch_surface(kind: RefKind) -> MenuSurface {
    match kind {
        RefKind::Local => MenuSurface::Branch,
        RefKind::Remote => MenuSurface::RemoteBranch,
        RefKind::Tag => MenuSurface::Tag,
    }
}

/// Emit the pending OSC 52 clipboard write (opt-in via `GGT_OSC52=1`).
///
/// The dispatch map in [`actions`] always drops a copy of the text in the
/// fallback clipboard file; this adds the terminal-escape path on top, and only
/// when the user asked for it. Some terminals ignore OSC 52 and the sequence has
/// no business in `--dump` output, so the default is "off".
fn flush_clipboard(state: &mut AppState) -> io::Result<()> {
    let Some(text) = state.pending_clipboard.take() else {
        return Ok(());
    };
    let enabled = std::env::var("GGT_OSC52")
        .map(|value| value == "1")
        .unwrap_or(false);
    if enabled {
        actions::emit_osc52(&text)?;
    }
    Ok(())
}

/// [`ViewOptions`] for one frame.
fn make_view<'a>(
    state: &'a AppState,
    ladder: Ladder,
    mock: bool,
    show_graph: bool,
) -> ViewOptions<'a> {
    let mut view = ViewOptions {
        ladder,
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
    // The panel's border title is the mockup's lead line, so the panel's full
    // state — kind, view, row, file count, compare target, external diff tool —
    // is printed here instead. `--dump` is the headless evidence path, which is
    // what `CdvState::status_line` exists for.
    if let Some(cdv) = &state.cdv {
        writeln!(out, "{}", cdv.status_line())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A probe fixture built from `(key, value)` pairs (no process env).
    fn caps(pairs: &[(&str, &str)]) -> TermCaps {
        let pairs: Vec<(String, String)> = pairs
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect();
        TermCaps::detect(move |key| {
            pairs
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value.clone())
        })
    }

    /// W11/T1: the Dumb tier must never receive `smcup`/`rmcup`.
    #[test]
    fn dumb_tier_never_emits_smcup() {
        let dumb = caps(&[("TERM", "dumb")]);
        assert!(dumb.dumb);
        assert!(!enters_alt_screen(&dumb));

        let mut out = Vec::new();
        write_enter_sequences(&mut out, &dumb).unwrap();
        let enter = String::from_utf8(out).unwrap();
        assert!(
            !enter.contains("\u{1b}[?1049h"),
            "Dumb must not enter the alternate screen: {enter:?}"
        );
        assert!(
            !enter.contains("\u{1b}[?1000h"),
            "Dumb must not enable mouse capture: {enter:?}"
        );

        let mut out = Vec::new();
        write_leave_sequences(&mut out, &dumb).unwrap();
        let leave = String::from_utf8(out).unwrap();
        assert!(
            !leave.contains("\u{1b}[?1049l"),
            "Dumb must not leave an alternate screen it never entered: {leave:?}"
        );
    }

    /// W11/T1: every non-Dumb tier keeps W8's always-alt-screen policy.
    #[test]
    fn proven_terminal_enters_and_leaves_the_alt_screen() {
        let xterm = caps(&[("TERM", "xterm-256color"), ("LANG", "en_US.UTF-8")]);
        assert!(enters_alt_screen(&xterm));

        let mut out = Vec::new();
        write_enter_sequences(&mut out, &xterm).unwrap();
        assert!(String::from_utf8(out).unwrap().contains("\u{1b}[?1049h"));

        let mut out = Vec::new();
        write_leave_sequences(&mut out, &xterm).unwrap();
        assert!(String::from_utf8(out).unwrap().contains("\u{1b}[?1049l"));
    }

    /// W11/T1: the fallback still paints the commit list and the dumb ladder.
    #[test]
    fn dumb_fallback_still_renders_the_commit_list() {
        let dumb = caps(&[("TERM", "dumb")]);
        let ladder = Ladder::from_caps(&dumb);
        let state = AppState::demo();
        // The interactive fallback: `--mock` asked for the graph, but Dumb forces
        // the P0 plain list (`show_graph = false`).
        let view = make_view(&state, ladder, true, false);
        let backend = ratatui::backend::TestBackend::new(100, 12);
        let mut terminal = ratatui::Terminal::new(backend).unwrap();
        terminal
            .draw(|frame| render::ui(frame, &state, &view))
            .unwrap();

        let buffer = terminal.backend().buffer();
        let mut text = String::new();
        for y in 0..buffer.area.height {
            for x in 0..buffer.area.width {
                text.push_str(buffer[(x, y)].symbol());
            }
            text.push('\n');
        }
        assert!(
            text.contains("commits ("),
            "commit-list header missing:\n{text}"
        );
        assert!(
            text.contains("Add graph renderer widget"),
            "commit list missing:\n{text}"
        );
        assert!(
            text.contains("caps: plain/mono/dumb"),
            "status bar must show the dumb ladder:\n{text}"
        );
        assert!(
            !text.contains("lanes:"),
            "the graph view must not be drawn on Dumb:\n{text}"
        );
    }
}
