//! Keyboard → [`Action`] mapping (M2-W4).
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

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::Action;

/// Map one key event to an [`Action`], or `None` when the key is unbound.
///
/// `Alt`-modified keys are reserved (W5/W6 context menus) and always `None`.
pub fn action_for(key: KeyEvent) -> Option<Action> {
    let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
    let shift = key.modifiers.contains(KeyModifiers::SHIFT);

    if key.modifiers.contains(KeyModifiers::ALT) {
        return None;
    }

    match key.code {
        KeyCode::Esc => Some(Action::Quit),
        KeyCode::Char('q') | KeyCode::Char('Q') if !ctrl => Some(Action::Quit),

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
            Some(Action::Refresh)
        );
    }

    #[test]
    fn unbound_keys_are_none() {
        for event in [
            key(KeyCode::Char('z')),
            key(KeyCode::Tab),
            modified(KeyCode::Char('j'), KeyModifiers::ALT),
            modified(KeyCode::Char('j'), KeyModifiers::CONTROL),
        ] {
            assert_eq!(action_for(event), None);
        }
    }
}
