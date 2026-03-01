use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::Action;

/// Map a key event to an action based on the current mode.
pub fn map_key(key: KeyEvent, mode: &InputMode, last_key: Option<char>) -> Option<Action> {
    match mode {
        InputMode::Normal => map_normal_key(key, last_key),
        InputMode::Search => map_search_key(key),
        InputMode::Detail => map_detail_key(key),
        InputMode::Editing => map_editing_key(key),
        InputMode::Dialog => map_dialog_key(key),
        InputMode::Command => map_command_key(key),
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum InputMode {
    Normal,
    Search,
    Detail,
    Editing,
    Dialog,
    Command,
}

fn map_normal_key(key: KeyEvent, last_key: Option<char>) -> Option<Action> {
    match key.code {
        KeyCode::Char('q') => Some(Action::Quit),
        KeyCode::Char('j') | KeyCode::Down => Some(Action::MoveDown),
        KeyCode::Char('k') | KeyCode::Up => Some(Action::MoveUp),
        KeyCode::Char('G') => Some(Action::MoveToBottom),
        KeyCode::Char('g') => {
            if last_key == Some('g') {
                Some(Action::MoveToTop)
            } else {
                None // Wait for second 'g'
            }
        }
        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(Action::PageDown)
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(Action::PageUp)
        }
        KeyCode::Char('/') => Some(Action::EnterSearch),
        KeyCode::Enter => Some(Action::OpenDetail),
        KeyCode::Char('a') => Some(Action::AddEntry),
        KeyCode::Char('d') => {
            if last_key == Some('d') {
                Some(Action::DeleteEntry)
            } else {
                None // Wait for second 'd'
            }
        }
        KeyCode::Char('D') => Some(Action::DuplicateEntry),
        KeyCode::Char('y') => {
            if last_key == Some('y') {
                Some(Action::YankCitekey)
            } else {
                None
            }
        }
        KeyCode::Tab => Some(Action::ToggleGroups),
        KeyCode::Char('h') | KeyCode::Left => Some(Action::FocusGroups),
        KeyCode::Char('l') | KeyCode::Right => Some(Action::FocusList),
        KeyCode::Char(' ') => Some(Action::SelectGroup),
        KeyCode::Char('B') => Some(Action::ToggleBraces),
        KeyCode::Char('o') => Some(Action::OpenFile),
        KeyCode::Char('w') => Some(Action::OpenWeb),
        KeyCode::Char(':') => Some(Action::EnterCommand),
        KeyCode::Char('?') => Some(Action::ShowHelp),
        _ => None,
    }
}

fn map_search_key(key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Esc => Some(Action::ExitSearch),
        KeyCode::Enter => Some(Action::ConfirmSearch),
        KeyCode::Backspace => Some(Action::SearchBackspace),
        KeyCode::Char(c) => Some(Action::SearchChar(c)),
        _ => None,
    }
}

fn map_detail_key(key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => Some(Action::CloseDetail),
        KeyCode::Char('j') | KeyCode::Down => Some(Action::MoveDown),
        KeyCode::Char('k') | KeyCode::Up => Some(Action::MoveUp),
        KeyCode::Char('e') | KeyCode::Enter => Some(Action::EditField),
        KeyCode::Char('a') => Some(Action::AddField),
        KeyCode::Char('d') => Some(Action::DeleteField),
        KeyCode::Char('T') => Some(Action::TitlecaseField),
        KeyCode::Char('o') => Some(Action::OpenFile),
        KeyCode::Char('w') => Some(Action::OpenWeb),
        KeyCode::Char('g') => Some(Action::EditGroups),
        KeyCode::Char('c') => Some(Action::RegenCitekey),
        _ => None,
    }
}

fn map_editing_key(key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Esc => Some(Action::CancelEdit),
        KeyCode::Enter => Some(Action::ConfirmEdit),
        KeyCode::Backspace => Some(Action::EditBackspace),
        KeyCode::Left => Some(Action::EditCursorLeft),
        KeyCode::Right => Some(Action::EditCursorRight),
        KeyCode::Home | KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(Action::EditCursorHome)
        }
        KeyCode::End | KeyCode::Char('e') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(Action::EditCursorEnd)
        }
        KeyCode::Delete => Some(Action::EditDelete),
        KeyCode::Char(c) => Some(Action::EditChar(c)),
        _ => None,
    }
}

fn map_dialog_key(key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Esc | KeyCode::Char('n') => Some(Action::DialogCancel),
        KeyCode::Enter | KeyCode::Char('y') => Some(Action::DialogConfirm),
        KeyCode::Char('j') | KeyCode::Down => Some(Action::MoveDown),
        KeyCode::Char('k') | KeyCode::Up => Some(Action::MoveUp),
        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(Action::PageDown)
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(Action::PageUp)
        }
        KeyCode::Char('g') => Some(Action::MoveToTop),
        KeyCode::Char('G') => Some(Action::MoveToBottom),
        _ => None,
    }
}

fn map_command_key(key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Esc => Some(Action::ExitCommand),
        KeyCode::Enter => Some(Action::ExecuteCommand),
        KeyCode::Backspace => Some(Action::CommandBackspace),
        KeyCode::Char(c) => Some(Action::CommandChar(c)),
        _ => None,
    }
}
