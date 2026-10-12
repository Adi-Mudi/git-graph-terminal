//! Keyboard → [`Action`] mapping (M2-W4, extended in W6).
//!
//! One pure function, [`action_for`], so the binding table is easy to read,
//! easy to test, and independent of the terminal. It only looks at the key
//! code and modifiers; [`crate::main`] filters non-press events first.
//!
//! Reference anchors (read-only `../vscode-git-graph`):
//!
//! * `web/main.ts:2025-2111` — the global key handler and its ad-hoc keys.
//! * Plain Up/Down is **table order** and clamps (`2040-2068`, inventory C03);
//!   `Ctrl+Up`/`Ctrl+Down` follow the branch (`2042-2054`, inventory C04) and do
//!   nothing when the relation is absent.
//! * **R1.2** (`web/main.ts:2078-2080`): `s` with **no Shift = next** stash,
//!   with Shift = previous. pi's `INT-009` (`s` = previous) is inverted and not ported.
//! * Esc is not bound by the reference; in a terminal UI it is the conventional
//!   "quit/close". W4 has no overlays, so Esc quits. W5/W6 overlays can claim it
//!   by handling it before this map.
//!
//! ## W6 additions
//!
//! The reference reaches the find widget and the CDV through **click targets**,
//! not keybindings: `#findBtn` / `#cdvFileViewTypeTree` / `#cdvFileViewTypeList`
//! (`web/main.ts:2577-2580`). A terminal needs keys, so the W6 bindings below
//! are the terminal equivalents of those controls. They are deliberately
//! conventional (`Ctrl+F` for find, `F3`/`Shift+F3` for next/previous) and every
//! one of them has a unit test naming the control it stands in for.
//!
//! | key | action | reference control |
//! | --- | ------ | ----------------- |
//! | `Ctrl+F` | open/close find | `#findBtn` |
//! | `F3` / `Ctrl+G` | next match | `#findNext` |
//! | `Shift+F3` | previous match | `#findPrev` |
//! | `Alt+C` | match case | `#findCaseSensitive` |
//! | `Alt+R` | regex mode | `#findIsRegex` |
//! | `Alt+D` | open CDV per match | `#findOpenCommitDetailsView` |
//! | `d` | open CDV | click a commit |
//! | `Tab` | focus the CDV | — |
//! | `v` | Tree/List file view | `#cdvFileViewTypeTree` / `#cdvFileViewTypeList` |
//! | `+` / `-` | resize the CDV | `.cdvHeightResize` |
//! | `Ctrl+W` | close the CDV | `#cdvClose` |
//!
//! `Alt`-modified keys were reserved and unbound in W4/W5; W6 claims exactly
//! the three find toggles and nothing else.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::{Action, MenuNav, PagerNav};
use crate::dialog::DialogKey;

/// Does this key open the context menu for the selected row?
///
/// `Shift+F10` is the Windows/GTK "menu key" convention and `m` is the
/// terminal-friendly alias; `Esc` stays "quit" when no menu is open.
pub fn opens_menu(key: KeyEvent) -> bool {
    if key.modifiers.contains(KeyModifiers::ALT) {
        return false;
    }
    key.code == KeyCode::F(10)
        || (key.code == KeyCode::Char('m') && !key.modifiers.contains(KeyModifiers::CONTROL))
}

/// Translate a key for the **open** context menu.
///
/// `Esc` closes it (W5/W6 overlays claim `Esc` before [`action_for`] sees it),
/// arrows/`j`/`k` move, `Enter` activates, and a bare letter is a shortcut.
pub fn menu_nav_for(key: KeyEvent) -> Option<MenuNav> {
    if key.modifiers.contains(KeyModifiers::ALT) || key.modifiers.contains(KeyModifiers::CONTROL) {
        return None;
    }
    match key.code {
        KeyCode::Esc => Some(MenuNav::Close),
        KeyCode::Up | KeyCode::Char('k') => Some(MenuNav::Up),
        KeyCode::Down | KeyCode::Char('j') => Some(MenuNav::Down),
        KeyCode::Enter => Some(MenuNav::Activate),
        KeyCode::Char(key) if key.is_ascii_alphabetic() => {
            Some(MenuNav::Shortcut(key.to_ascii_lowercase()))
        }
        _ => None,
    }
}

/// Translate a key for the **open** dialog (W7/T1).
///
/// The reference routes these from a window-level listener
/// (`web/main.ts:2032-2040`): `Escape` closes the dialog and `Enter` submits it
/// from anywhere, including inside an input. `Tab`/`Shift+Tab` (and the arrow
/// aliases) walk the generated `tabindex` order, and a plain character edits the
/// focused text field.
///
/// This runs **before** the menu and the global [`action_for`] map, so the
/// dialog provably owns the keyboard while it is open.
pub fn dialog_key_for(key: KeyEvent) -> Option<DialogKey> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    if ctrl || alt {
        return None;
    }
    match key.code {
        KeyCode::Esc => Some(DialogKey::Escape),
        KeyCode::Enter => Some(DialogKey::Submit),
        KeyCode::Tab | KeyCode::Down => Some(DialogKey::Next),
        KeyCode::BackTab | KeyCode::Up => Some(DialogKey::Prev),
        KeyCode::Backspace => Some(DialogKey::Backspace),
        KeyCode::Delete => Some(DialogKey::Delete),
        KeyCode::Left => Some(DialogKey::Left),
        KeyCode::Right => Some(DialogKey::Right),
        KeyCode::Home => Some(DialogKey::Home),
        KeyCode::End => Some(DialogKey::End),
        KeyCode::Char(' ') => Some(DialogKey::Space),
        KeyCode::Char(c) => Some(DialogKey::Char(c)),
        _ => None,
    }
}

/// Translate a key for the **open** pager (W7/T3).
///
/// The pager is read-only, so the map is small: `Esc`/`q` close it, `j`/`k` and
/// the arrows scroll by one line, `PageUp`/`PageDown` by a screenful, and
/// `Home`/`End` jump to the ends. This runs before [`action_for`], so `q` closes
/// the pager instead of quitting the app.
pub fn pager_nav_for(key: KeyEvent) -> Option<PagerNav> {
    if key.modifiers.contains(KeyModifiers::ALT) || key.modifiers.contains(KeyModifiers::CONTROL) {
        return None;
    }
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('Q') => Some(PagerNav::Close),
        KeyCode::Up | KeyCode::Char('k') => Some(PagerNav::LineUp),
        KeyCode::Down | KeyCode::Char('j') => Some(PagerNav::LineDown),
        KeyCode::PageUp => Some(PagerNav::PageUp),
        KeyCode::PageDown => Some(PagerNav::PageDown),
        KeyCode::Home => Some(PagerNav::Top),
        KeyCode::End => Some(PagerNav::Bottom),
        _ => None,
    }
}

/// Translate a key for the **open** find widget (W7/T2).
///
/// While the widget owns the keyboard, a plain character is query text
/// (`push_char`), `Backspace` edits it, `Enter`/`Shift+Enter` step through the
/// matches, and `Esc` closes the widget and restores the pre-find selection.
/// The W6 chords (`Ctrl+F`, `F3`, `Alt+C`…) still fall through to
/// [`action_for`] so the existing toggles keep working while the bar is up.
pub fn find_key_for(key: KeyEvent) -> Option<FindKey> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let alt = key.modifiers.contains(KeyModifiers::ALT);
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);
    if ctrl || alt {
        return None;
    }
    match key.code {
        KeyCode::Esc => Some(FindKey::Close),
        KeyCode::Backspace => Some(FindKey::Backspace),
        KeyCode::Enter if shift => Some(FindKey::Prev),
        KeyCode::Enter => Some(FindKey::Next),
        KeyCode::Char(c) if !shift => Some(FindKey::Char(c)),
        _ => None,
    }
}

/// Translate a key for the **focused** CDV file list (W7/T3).
///
/// The reference reaches the file-row menu and the reviewed checkbox through
/// **click targets** (`#cdvFileViewTypeTree`, the row context menu, and the
/// `Reviewed` cell at `web/main.ts:2620`), not keys. A terminal needs keys, so
/// this map is the terminal equivalent of those controls. It runs only while
/// `Tab` has given the CDV the keyboard, so the commit table's `j`/`k`/`Enter`
/// stay theirs.
///
/// | key | action | reference control |
/// | --- | ------ | ----------------- |
/// | `j` / `Down` | next file row | — |
/// | `k` / `Up` | previous file row | — |
/// | `Enter` | A65 View Diff | file-row menu |
/// | `o` | A66 View File at Revision | file-row menu |
/// | `Space` | A68 Mark Reviewed (VIS-049) | the row's `Reviewed` cell |
/// | `Left` | collapse folder | `alterFileTreeFolderOpen` |
/// | `Right` | expand folder | `alterFileTreeFolderOpen` |
pub fn cdv_key_for(key: KeyEvent) -> Option<Action> {
    if key.modifiers.contains(KeyModifiers::CONTROL) || key.modifiers.contains(KeyModifiers::ALT) {
        return None;
    }
    match key.code {
        KeyCode::Down | KeyCode::Char('j') => Some(Action::CdvFileNext),
        KeyCode::Up | KeyCode::Char('k') => Some(Action::CdvFilePrev),
        KeyCode::Enter => Some(Action::CdvFileViewDiff),
        KeyCode::Char('o') | KeyCode::Char('O') => Some(Action::CdvFileViewRevision),
        KeyCode::Char(' ') => Some(Action::CdvFileToggleReviewed),
        KeyCode::Left => Some(Action::CdvFolderCollapse),
        KeyCode::Right => Some(Action::CdvFolderExpand),
        _ => None,
    }
}

/// A keypress translated for the open find widget (W7/T2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindKey {
    /// Append `c` to the query.
    Char(char),
    /// Delete the last query character.
    Backspace,
    /// Jump to the next match.
    Next,
    /// Jump to the previous match.
    Prev,
    /// Close the widget and restore the pre-find selection.
    Close,
}

/// Map one key event to an [`Action`], or `None` when the key is unbound.
///
/// `Alt`-modified keys are reserved: W6 claims only `Alt+C`, `Alt+R` and `Alt+D`
/// (the find toggles); every other `Alt` chord stays unbound.
pub fn action_for(key: KeyEvent) -> Option<Action> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);
    let alt = key.modifiers.contains(KeyModifiers::ALT);

    // W6 find toggles, checked before the blanket `Alt` rejection below.
    if alt && !ctrl {
        return match key.code {
            KeyCode::Char('c') | KeyCode::Char('C') => Some(Action::FindToggleCase),
            KeyCode::Char('r') | KeyCode::Char('R') => Some(Action::FindToggleRegex),
            KeyCode::Char('d') | KeyCode::Char('D') => Some(Action::FindToggleCdv),
            _ => None,
        };
    }
    if alt {
        return None;
    }

    match key.code {
        KeyCode::Esc => Some(Action::Quit),
        KeyCode::Char('q') | KeyCode::Char('Q') if !ctrl => Some(Action::Quit),

        // W6/T1 — find (the `#findBtn` / `#findNext` / `#findPrev` controls).
        KeyCode::Char('f') if ctrl => Some(Action::FindOpen),
        KeyCode::F(3) if shift => Some(Action::FindPrev),
        KeyCode::F(3) => Some(Action::FindNext),
        KeyCode::Char('g') if ctrl => Some(Action::FindNext),

        // W6/T3 — the commit details view.
        KeyCode::Char('w') if ctrl => Some(Action::CdvClose),
        KeyCode::Char('d') if !shift => Some(Action::CdvOpen),
        KeyCode::Tab => Some(Action::CdvFocusToggle),
        KeyCode::Char('v') if !ctrl => Some(Action::CdvToggleView),
        KeyCode::Char('+') | KeyCode::Char('=') => Some(Action::CdvResizeUp),
        KeyCode::Char('-') | KeyCode::Char('_') => Some(Action::CdvResizeDown),

        // W7/T4 — the uncommitted and comparison CDV kinds. The reference opens
        // both from the table (the virtual uncommitted row, and `compareWithHash`
        // set from the commit menu), so a terminal needs keys for them.
        KeyCode::Char('u') if !ctrl => Some(Action::CdvOpenUncommitted),
        KeyCode::Char('c') => Some(Action::CompareMark),

        // Branch nav: Right = first parent, Left = first child; Ctrl+Down/Up are aliases.
        KeyCode::Right => Some(Action::NextBranch),
        KeyCode::Left => Some(Action::PrevBranch),
        KeyCode::Down if ctrl => Some(Action::NextBranch),
        KeyCode::Up if ctrl => Some(Action::PrevBranch),

        // Table order (C03): clamped, one row at a time.
        KeyCode::Down | KeyCode::Char('j') if !ctrl => Some(Action::SelectNext),
        KeyCode::Up | KeyCode::Char('k') if !ctrl => Some(Action::SelectPrev),

        KeyCode::PageDown => Some(Action::PageNext),
        KeyCode::PageUp => Some(Action::PagePrev),
        KeyCode::Home => Some(Action::Top),
        KeyCode::End => Some(Action::Bottom),

        KeyCode::Enter => Some(Action::ToggleExpand),

        // Stash (R1.2): plain `s` / Ctrl+s = NEXT, Shift+s = PREVIOUS.
        // Shift is checked first so Ctrl+Shift+s is PREVIOUS, not NEXT.
        KeyCode::Char('s') if shift => Some(Action::StashPrev),
        KeyCode::Char('S') => Some(Action::StashPrev),
        KeyCode::Char('s') => Some(Action::StashNext),

        KeyCode::Char('r') if !shift => Some(Action::Refresh),

        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn modified(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent::new(code, modifiers)
    }

    #[test]
    fn quit_keys() {
        assert_eq!(action_for(key(KeyCode::Esc)), Some(Action::Quit));
        assert_eq!(action_for(key(KeyCode::Char('q'))), Some(Action::Quit));
        assert_eq!(
            action_for(modified(KeyCode::Char('q'), KeyModifiers::SHIFT)),
            Some(Action::Quit)
        );
        assert_eq!(
            action_for(modified(KeyCode::Char('q'), KeyModifiers::CONTROL)),
            None,
            "Ctrl+q is reserved"
        );
    }

    #[test]
    fn table_order_keys() {
        for event in [key(KeyCode::Down), key(KeyCode::Char('j'))] {
            assert_eq!(action_for(event), Some(Action::SelectNext));
        }
        for event in [key(KeyCode::Up), key(KeyCode::Char('k'))] {
            assert_eq!(action_for(event), Some(Action::SelectPrev));
        }
        assert_eq!(action_for(key(KeyCode::PageDown)), Some(Action::PageNext));
        assert_eq!(action_for(key(KeyCode::PageUp)), Some(Action::PagePrev));
        assert_eq!(action_for(key(KeyCode::Home)), Some(Action::Top));
        assert_eq!(action_for(key(KeyCode::End)), Some(Action::Bottom));
    }

    #[test]
    fn branch_nav_keys() {
        assert_eq!(action_for(key(KeyCode::Right)), Some(Action::NextBranch));
        assert_eq!(action_for(key(KeyCode::Left)), Some(Action::PrevBranch));
        assert_eq!(
            action_for(modified(KeyCode::Down, KeyModifiers::CONTROL)),
            Some(Action::NextBranch)
        );
        assert_eq!(
            action_for(modified(KeyCode::Up, KeyModifiers::CONTROL)),
            Some(Action::PrevBranch)
        );
        // Aliases are not affected by an extra Shift.
        assert_eq!(
            action_for(modified(
                KeyCode::Down,
                KeyModifiers::CONTROL | KeyModifiers::SHIFT
            )),
            Some(Action::NextBranch)
        );
    }

    #[test]
    fn stash_keys_follow_r1_2() {
        // No Shift = NEXT.
        assert_eq!(action_for(key(KeyCode::Char('s'))), Some(Action::StashNext));
        assert_eq!(
            action_for(modified(KeyCode::Char('s'), KeyModifiers::CONTROL)),
            Some(Action::StashNext)
        );
        // Shift = PREVIOUS, however the terminal reports it.
        assert_eq!(
            action_for(modified(KeyCode::Char('s'), KeyModifiers::SHIFT)),
            Some(Action::StashPrev)
        );
        assert_eq!(action_for(key(KeyCode::Char('S'))), Some(Action::StashPrev));
        assert_eq!(
            action_for(modified(
                KeyCode::Char('s'),
                KeyModifiers::CONTROL | KeyModifiers::SHIFT
            )),
            Some(Action::StashPrev),
            "Ctrl+Shift+s stays PREVIOUS"
        );
    }

    #[test]
    fn expand_and_refresh_keys() {
        assert_eq!(action_for(key(KeyCode::Enter)), Some(Action::ToggleExpand));
        assert_eq!(action_for(key(KeyCode::Char('r'))), Some(Action::Refresh));
        assert_eq!(
            action_for(modified(KeyCode::Char('r'), KeyModifiers::CONTROL)),
            Some(Action::Refresh),
            "W4 behaviour is kept: Ctrl+r is still a refresh"
        );
    }

    #[test]
    fn find_keys_stand_in_for_the_find_widget_controls() {
        assert_eq!(
            action_for(modified(KeyCode::Char('f'), KeyModifiers::CONTROL)),
            Some(Action::FindOpen),
            "Ctrl+F opens the find widget (#findBtn)"
        );
        assert_eq!(action_for(key(KeyCode::F(3))), Some(Action::FindNext));
        assert_eq!(
            action_for(modified(KeyCode::F(3), KeyModifiers::SHIFT)),
            Some(Action::FindPrev)
        );
        assert_eq!(
            action_for(modified(KeyCode::Char('g'), KeyModifiers::CONTROL)),
            Some(Action::FindNext)
        );
        assert_eq!(
            action_for(modified(KeyCode::Char('c'), KeyModifiers::ALT)),
            Some(Action::FindToggleCase)
        );
        assert_eq!(
            action_for(modified(KeyCode::Char('r'), KeyModifiers::ALT)),
            Some(Action::FindToggleRegex)
        );
        assert_eq!(
            action_for(modified(KeyCode::Char('d'), KeyModifiers::ALT)),
            Some(Action::FindToggleCdv)
        );
    }

    #[test]
    fn cdv_keys_stand_in_for_the_cdv_controls() {
        assert_eq!(action_for(key(KeyCode::Char('d'))), Some(Action::CdvOpen));
        assert_eq!(action_for(key(KeyCode::Tab)), Some(Action::CdvFocusToggle));
        assert_eq!(
            action_for(key(KeyCode::Char('v'))),
            Some(Action::CdvToggleView),
            "#cdvFileViewTypeTree / #cdvFileViewTypeList"
        );
        assert_eq!(
            action_for(key(KeyCode::Char('+'))),
            Some(Action::CdvResizeUp)
        );
        assert_eq!(
            action_for(key(KeyCode::Char('-'))),
            Some(Action::CdvResizeDown)
        );
        assert_eq!(
            action_for(modified(KeyCode::Char('w'), KeyModifiers::CONTROL)),
            Some(Action::CdvClose),
            "#cdvClose"
        );
    }

    #[test]
    fn unbound_keys_are_none() {
        for event in [
            key(KeyCode::Char('z')),
            key(KeyCode::Char('y')),
            modified(KeyCode::Char('j'), KeyModifiers::ALT),
            modified(KeyCode::Char('j'), KeyModifiers::CONTROL),
            modified(KeyCode::Char('f'), KeyModifiers::ALT),
        ] {
            assert_eq!(action_for(event), None);
        }
    }

    /// The key map that must claim a representative key (W7/T1 harness).
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Route {
        Global,
        Cdv,
        Find,
        Dialog,
    }

    /// The key that must produce `action`, and the map that must claim it.
    ///
    /// This is the W7/T1 regression harness: every [`Action`] variant needs a
    /// row here, so an action that is never wired to a key fails a test instead
    /// of only showing up when a pty session presses the key.
    fn representative(action: Action) -> (Route, KeyEvent) {
        use Action::*;
        match action {
            // The find widget owns these while it is open (W7/T2).
            FindClose => (Route::Find, key(KeyCode::Esc)),
            FindNext => (Route::Find, key(KeyCode::Enter)),
            FindPrev => (Route::Find, modified(KeyCode::Enter, KeyModifiers::SHIFT)),
            // The dialog owns these while it is open (W7/T1).
            DialogConfirm => (Route::Dialog, key(KeyCode::Enter)),
            DialogCancel => (Route::Dialog, key(KeyCode::Esc)),
            // The focused CDV file list owns these (W7/T3).
            CdvFileNext => (Route::Cdv, key(KeyCode::Char('j'))),
            CdvFilePrev => (Route::Cdv, key(KeyCode::Char('k'))),
            CdvFileViewDiff => (Route::Cdv, key(KeyCode::Enter)),
            CdvFileViewRevision => (Route::Cdv, key(KeyCode::Char('o'))),
            CdvFileToggleReviewed => (Route::Cdv, key(KeyCode::Char(' '))),
            CdvFolderCollapse => (Route::Cdv, key(KeyCode::Left)),
            CdvFolderExpand => (Route::Cdv, key(KeyCode::Right)),
            // Everything else comes from the global map.
            SelectNext => (Route::Global, key(KeyCode::Char('j'))),
            SelectPrev => (Route::Global, key(KeyCode::Char('k'))),
            PageNext => (Route::Global, key(KeyCode::PageDown)),
            PagePrev => (Route::Global, key(KeyCode::PageUp)),
            Top => (Route::Global, key(KeyCode::Home)),
            Bottom => (Route::Global, key(KeyCode::End)),
            ToggleExpand => (Route::Global, key(KeyCode::Enter)),
            NextBranch => (Route::Global, key(KeyCode::Right)),
            PrevBranch => (Route::Global, key(KeyCode::Left)),
            StashNext => (Route::Global, key(KeyCode::Char('s'))),
            StashPrev => (Route::Global, key(KeyCode::Char('S'))),
            Refresh => (Route::Global, key(KeyCode::Char('r'))),
            Quit => (Route::Global, key(KeyCode::Char('q'))),
            CdvOpen => (Route::Global, key(KeyCode::Char('d'))),
            CdvClose => (
                Route::Global,
                modified(KeyCode::Char('w'), KeyModifiers::CONTROL),
            ),
            CdvFocusToggle => (Route::Global, key(KeyCode::Tab)),
            CdvToggleView => (Route::Global, key(KeyCode::Char('v'))),
            CdvResizeUp => (Route::Global, key(KeyCode::Char('+'))),
            CdvResizeDown => (Route::Global, key(KeyCode::Char('-'))),
            FindOpen => (
                Route::Global,
                modified(KeyCode::Char('f'), KeyModifiers::CONTROL),
            ),
            FindToggleCase => (
                Route::Global,
                modified(KeyCode::Char('c'), KeyModifiers::ALT),
            ),
            FindToggleRegex => (
                Route::Global,
                modified(KeyCode::Char('r'), KeyModifiers::ALT),
            ),
            FindToggleCdv => (
                Route::Global,
                modified(KeyCode::Char('d'), KeyModifiers::ALT),
            ),
            CdvOpenUncommitted => (Route::Global, key(KeyCode::Char('u'))),
            CompareMark => (Route::Global, key(KeyCode::Char('c'))),
        }
    }

    /// Did `route` claim `key` for exactly `action`?
    fn claimed(route: Route, key: KeyEvent, action: Action) -> bool {
        match route {
            Route::Global => action_for(key) == Some(action),
            Route::Cdv => cdv_key_for(key) == Some(action),
            Route::Find => {
                let expected = match action {
                    Action::FindClose => FindKey::Close,
                    Action::FindNext => FindKey::Next,
                    Action::FindPrev => FindKey::Prev,
                    _ => return false,
                };
                find_key_for(key) == Some(expected)
            }
            Route::Dialog => {
                let expected = if action == Action::DialogCancel {
                    DialogKey::Escape
                } else {
                    DialogKey::Submit
                };
                dialog_key_for(key) == Some(expected)
            }
        }
    }

    /// W7/T1 harness: every [`Action`] must be reachable from a key.
    #[test]
    fn every_action_is_reachable_from_a_key() {
        for &action in Action::ALL {
            let (route, event) = representative(action);
            assert!(
                claimed(route, event, action),
                "{action:?} is unreachable: {event:?} on the {route:?} map did not produce it"
            );
        }
    }

    /// The pager's own map has the same guarantee (W7/T3).
    #[test]
    fn every_pager_nav_is_reachable_from_a_key() {
        let table = [
            (key(KeyCode::Char('j')), PagerNav::LineDown),
            (key(KeyCode::Char('k')), PagerNav::LineUp),
            (key(KeyCode::PageDown), PagerNav::PageDown),
            (key(KeyCode::PageUp), PagerNav::PageUp),
            (key(KeyCode::Home), PagerNav::Top),
            (key(KeyCode::End), PagerNav::Bottom),
            (key(KeyCode::Char('q')), PagerNav::Close),
            (key(KeyCode::Esc), PagerNav::Close),
        ];
        for (event, expected) in table {
            assert_eq!(pager_nav_for(event), Some(expected));
        }
    }
}
