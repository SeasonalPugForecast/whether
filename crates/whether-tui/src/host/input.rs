//! Terminal events to reducer keys, and mouse hit-testing on the location tabs.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::text::Line;

use crate::ui_state::Key;

/// The reducer's key for a crossterm key event.
pub(super) fn key(event: KeyEvent) -> Key {
    match event.code {
        KeyCode::Char('c') if event.modifiers.contains(KeyModifiers::CONTROL) => Key::Char('q'),
        KeyCode::Char(c) => Key::Char(c),
        KeyCode::Left => Key::Left,
        KeyCode::Right => Key::Right,
        KeyCode::Up => Key::Up,
        KeyCode::Down => Key::Down,
        KeyCode::Tab => Key::Tab,
        KeyCode::BackTab => Key::BackTab,
        KeyCode::Esc => Key::Esc,
        KeyCode::Enter => Key::Enter,
        KeyCode::Backspace => Key::Backspace,
        _ => Key::Other,
    }
}

/// The tab under column `x`, given the tab titles as drawn by ratatui's `Tabs`
/// (one space of padding on each side, a one-column divider between tabs).
pub(super) fn tab_at(x: u16, titles: &[String]) -> Option<usize> {
    let mut start = 0u16;
    for (i, title) in titles.iter().enumerate() {
        let width = u16::try_from(Line::raw(title.as_str()).width()).unwrap_or(u16::MAX);
        let end = start.saturating_add(width).saturating_add(2);
        if (start..end).contains(&x) {
            return Some(i);
        }
        start = end.saturating_add(1);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::crossterm::event::{KeyEventKind, KeyEventState};

    fn press(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
        KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    #[test]
    fn keys_map() {
        assert_eq!(
            key(press(KeyCode::Char('r'), KeyModifiers::NONE)),
            Key::Char('r')
        );
        assert_eq!(
            key(press(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            Key::Char('q')
        );
        assert_eq!(
            key(press(KeyCode::BackTab, KeyModifiers::SHIFT)),
            Key::BackTab
        );
        assert_eq!(key(press(KeyCode::F(5), KeyModifiers::NONE)), Key::Other);
    }

    #[test]
    fn tabs_hit_test() {
        // " Home 15° │ Cabin 3° "
        let titles = ["Home 15°".to_string(), "Cabin 3°".to_string()];
        assert_eq!(tab_at(0, &titles), Some(0));
        assert_eq!(tab_at(9, &titles), Some(0));
        assert_eq!(tab_at(10, &titles), None, "the divider");
        assert_eq!(tab_at(11, &titles), Some(1));
        assert_eq!(tab_at(30, &titles), None);
    }
}
