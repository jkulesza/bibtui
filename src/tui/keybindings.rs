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
        InputMode::CitationPreview => map_citation_preview_key(key, last_key),
        InputMode::Settings => map_settings_key(key),
        InputMode::ValidateResults => map_validate_results_key(key),
        InputMode::Help => Some(Action::CloseHelp),
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
    CitationPreview,
    Settings,
    ValidateResults,
    Help,
}

fn map_normal_key(key: KeyEvent, last_key: Option<char>) -> Option<Action> {
    match key.code {
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
        KeyCode::Char('f') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(Action::PageDown)
        }
        KeyCode::Char('b') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(Action::PageUp)
        }
        KeyCode::Char('u') => Some(Action::Undo),
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
        KeyCode::Char(' ') => Some(Action::ShowCitationPreview),
        KeyCode::Char('B') => Some(Action::ToggleBraces),
        KeyCode::Char('L') => Some(Action::ToggleLatex),
        KeyCode::Char('o') => Some(Action::OpenFile),
        KeyCode::Char('w') => Some(Action::OpenWeb),
        KeyCode::Char(':') => Some(Action::EnterCommand),
        KeyCode::Char('?') => Some(Action::ShowHelp),
        KeyCode::Char('S') => Some(Action::EnterSettings),
        KeyCode::Char('v') => Some(Action::Validate),
        KeyCode::Char('I') => Some(Action::ImportEntry),
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
        KeyCode::Esc => Some(Action::CloseDetail),
        KeyCode::Char('j') | KeyCode::Down => Some(Action::MoveDown),
        KeyCode::Char('k') | KeyCode::Up => Some(Action::MoveUp),
        KeyCode::Char('e') | KeyCode::Char('i') | KeyCode::Enter => Some(Action::EditField),
        KeyCode::Char('a') => Some(Action::AddField),
        KeyCode::Char('d') => Some(Action::DeleteField),
        KeyCode::Char('T') => Some(Action::TitlecaseField),
        KeyCode::Char('N') => Some(Action::NormalizeAuthor),
        KeyCode::Char('o') => Some(Action::OpenFile),
        KeyCode::Char('w') => Some(Action::OpenWeb),
        KeyCode::Char('g') => Some(Action::EditGroups),
        KeyCode::Char('c') => Some(Action::RegenCitekey),
        KeyCode::Char('L') => Some(Action::ToggleLatex),
        KeyCode::Char('B') => Some(Action::ToggleBraces),
        KeyCode::Char('u') => Some(Action::Undo),
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
        KeyCode::Up => Some(Action::EditCursorUp),
        KeyCode::Down => Some(Action::EditCursorDown),
        KeyCode::Home | KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(Action::EditCursorHome)
        }
        KeyCode::End | KeyCode::Char('e') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(Action::EditCursorEnd)
        }
        KeyCode::Delete => Some(Action::EditDelete),
        KeyCode::Tab => Some(Action::EditTabComplete),
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
        KeyCode::Char(' ') => Some(Action::DialogToggle),
        KeyCode::Char('f') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(Action::PageDown)
        }
        KeyCode::Char('b') if key.modifiers.contains(KeyModifiers::CONTROL) => {
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
        KeyCode::Tab => Some(Action::CommandTabComplete),
        KeyCode::Backspace => Some(Action::CommandBackspace),
        KeyCode::Char(c) => Some(Action::CommandChar(c)),
        _ => None,
    }
}

fn map_settings_key(key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Esc => Some(Action::ExitSettings),
        KeyCode::Char('j') | KeyCode::Down => Some(Action::SettingsMoveDown),
        KeyCode::Char('k') | KeyCode::Up => Some(Action::SettingsMoveUp),
        KeyCode::Char('g') => Some(Action::SettingsMoveToTop),
        KeyCode::Char('G') => Some(Action::SettingsMoveToBottom),
        KeyCode::Char('f') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(Action::SettingsPageDown)
        }
        KeyCode::Char('b') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(Action::SettingsPageUp)
        }
        KeyCode::Enter | KeyCode::Char(' ') => Some(Action::SettingsToggle),
        KeyCode::Char('e') => Some(Action::SettingsEdit),
        KeyCode::Char('E') => Some(Action::SettingsExport),
        KeyCode::Char('I') => Some(Action::SettingsImport),
        KeyCode::Char('a') => Some(Action::SettingsAddFieldGroup),
        KeyCode::Char('x') => Some(Action::SettingsDeleteFieldGroup),
        KeyCode::Char('r') => Some(Action::SettingsRenameFieldGroup),
        _ => None,
    }
}

fn map_validate_results_key(key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => Some(Action::MoveDown),
        KeyCode::Char('k') | KeyCode::Up => Some(Action::MoveUp),
        KeyCode::Esc | KeyCode::Char('q') => Some(Action::CloseValidateResults),
        _ => None,
    }
}

fn map_citation_preview_key(key: KeyEvent, last_key: Option<char>) -> Option<Action> {
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => Some(Action::MoveDown),
        KeyCode::Char('k') | KeyCode::Up => Some(Action::MoveUp),
        KeyCode::Char('y') => {
            if last_key == Some('y') {
                Some(Action::YankCitationPreview)
            } else {
                None // Wait for second 'y'
            }
        }
        _ => Some(Action::CloseCitationPreview),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    // ── Normal mode ──

    #[test]
    fn test_normal_move() {
        assert_eq!(map_key(key(KeyCode::Char('j')), &InputMode::Normal, None), Some(Action::MoveDown));
        assert_eq!(map_key(key(KeyCode::Down), &InputMode::Normal, None), Some(Action::MoveDown));
        assert_eq!(map_key(key(KeyCode::Char('k')), &InputMode::Normal, None), Some(Action::MoveUp));
        assert_eq!(map_key(key(KeyCode::Up), &InputMode::Normal, None), Some(Action::MoveUp));
        assert_eq!(map_key(key(KeyCode::Char('G')), &InputMode::Normal, None), Some(Action::MoveToBottom));
    }

    #[test]
    fn test_normal_gg() {
        // First 'g' returns None
        assert_eq!(map_key(key(KeyCode::Char('g')), &InputMode::Normal, None), None);
        // Second 'g' returns MoveToTop
        assert_eq!(map_key(key(KeyCode::Char('g')), &InputMode::Normal, Some('g')), Some(Action::MoveToTop));
    }

    #[test]
    fn test_normal_page_nav() {
        assert_eq!(map_key(ctrl('f'), &InputMode::Normal, None), Some(Action::PageDown));
        assert_eq!(map_key(ctrl('b'), &InputMode::Normal, None), Some(Action::PageUp));
    }

    #[test]
    fn test_normal_misc() {
        assert_eq!(map_key(key(KeyCode::Char('u')), &InputMode::Normal, None), Some(Action::Undo));
        assert_eq!(map_key(key(KeyCode::Char('/')), &InputMode::Normal, None), Some(Action::EnterSearch));
        assert_eq!(map_key(key(KeyCode::Enter), &InputMode::Normal, None), Some(Action::OpenDetail));
        assert_eq!(map_key(key(KeyCode::Char('a')), &InputMode::Normal, None), Some(Action::AddEntry));
        assert_eq!(map_key(key(KeyCode::Char('D')), &InputMode::Normal, None), Some(Action::DuplicateEntry));
        assert_eq!(map_key(key(KeyCode::Tab), &InputMode::Normal, None), Some(Action::ToggleGroups));
        assert_eq!(map_key(key(KeyCode::Char('B')), &InputMode::Normal, None), Some(Action::ToggleBraces));
        assert_eq!(map_key(key(KeyCode::Char('L')), &InputMode::Normal, None), Some(Action::ToggleLatex));
        assert_eq!(map_key(key(KeyCode::Char('S')), &InputMode::Normal, None), Some(Action::EnterSettings));
        assert_eq!(map_key(key(KeyCode::Char('?')), &InputMode::Normal, None), Some(Action::ShowHelp));
        assert_eq!(map_key(key(KeyCode::Char(':')), &InputMode::Normal, None), Some(Action::EnterCommand));
        assert_eq!(map_key(key(KeyCode::Char('I')), &InputMode::Normal, None), Some(Action::ImportEntry));
    }

    #[test]
    fn test_normal_dd() {
        assert_eq!(map_key(key(KeyCode::Char('d')), &InputMode::Normal, None), None);
        assert_eq!(map_key(key(KeyCode::Char('d')), &InputMode::Normal, Some('d')), Some(Action::DeleteEntry));
    }

    #[test]
    fn test_normal_yy() {
        assert_eq!(map_key(key(KeyCode::Char('y')), &InputMode::Normal, None), None);
        assert_eq!(map_key(key(KeyCode::Char('y')), &InputMode::Normal, Some('y')), Some(Action::YankCitekey));
    }

    #[test]
    fn test_normal_focus() {
        assert_eq!(map_key(key(KeyCode::Char('h')), &InputMode::Normal, None), Some(Action::FocusGroups));
        assert_eq!(map_key(key(KeyCode::Left), &InputMode::Normal, None), Some(Action::FocusGroups));
        assert_eq!(map_key(key(KeyCode::Char('l')), &InputMode::Normal, None), Some(Action::FocusList));
        assert_eq!(map_key(key(KeyCode::Right), &InputMode::Normal, None), Some(Action::FocusList));
    }

    // ── Search mode ──

    #[test]
    fn test_search_mode() {
        assert_eq!(map_key(key(KeyCode::Esc), &InputMode::Search, None), Some(Action::ExitSearch));
        assert_eq!(map_key(key(KeyCode::Enter), &InputMode::Search, None), Some(Action::ConfirmSearch));
        assert_eq!(map_key(key(KeyCode::Backspace), &InputMode::Search, None), Some(Action::SearchBackspace));
        assert_eq!(map_key(key(KeyCode::Char('x')), &InputMode::Search, None), Some(Action::SearchChar('x')));
    }

    // ── Detail mode ──

    #[test]
    fn test_detail_mode() {
        assert_eq!(map_key(key(KeyCode::Esc), &InputMode::Detail, None), Some(Action::CloseDetail));
        assert_eq!(map_key(key(KeyCode::Char('q')), &InputMode::Detail, None), None);
        assert_eq!(map_key(key(KeyCode::Char('e')), &InputMode::Detail, None), Some(Action::EditField));
        assert_eq!(map_key(key(KeyCode::Char('i')), &InputMode::Detail, None), Some(Action::EditField));
        assert_eq!(map_key(key(KeyCode::Enter), &InputMode::Detail, None), Some(Action::EditField));
        assert_eq!(map_key(key(KeyCode::Char('a')), &InputMode::Detail, None), Some(Action::AddField));
        assert_eq!(map_key(key(KeyCode::Char('d')), &InputMode::Detail, None), Some(Action::DeleteField));
        assert_eq!(map_key(key(KeyCode::Char('T')), &InputMode::Detail, None), Some(Action::TitlecaseField));
        assert_eq!(map_key(key(KeyCode::Char('N')), &InputMode::Detail, None), Some(Action::NormalizeAuthor));
        assert_eq!(map_key(key(KeyCode::Char('c')), &InputMode::Detail, None), Some(Action::RegenCitekey));
        assert_eq!(map_key(key(KeyCode::Char('u')), &InputMode::Detail, None), Some(Action::Undo));
    }

    // ── Editing mode ──

    #[test]
    fn test_editing_mode() {
        assert_eq!(map_key(key(KeyCode::Esc), &InputMode::Editing, None), Some(Action::CancelEdit));
        assert_eq!(map_key(key(KeyCode::Enter), &InputMode::Editing, None), Some(Action::ConfirmEdit));
        assert_eq!(map_key(key(KeyCode::Backspace), &InputMode::Editing, None), Some(Action::EditBackspace));
        assert_eq!(map_key(key(KeyCode::Left), &InputMode::Editing, None), Some(Action::EditCursorLeft));
        assert_eq!(map_key(key(KeyCode::Right), &InputMode::Editing, None), Some(Action::EditCursorRight));
        assert_eq!(map_key(key(KeyCode::Delete), &InputMode::Editing, None), Some(Action::EditDelete));
        assert_eq!(map_key(ctrl('a'), &InputMode::Editing, None), Some(Action::EditCursorHome));
        assert_eq!(map_key(ctrl('e'), &InputMode::Editing, None), Some(Action::EditCursorEnd));
        assert_eq!(map_key(key(KeyCode::Char('z')), &InputMode::Editing, None), Some(Action::EditChar('z')));
    }

    #[test]
    fn test_editing_mode_up_down() {
        // Up/Down arrow keys in editing mode map to EditCursorUp/Down (used by month navigation).
        assert_eq!(
            map_key(key(KeyCode::Up), &InputMode::Editing, None),
            Some(Action::EditCursorUp)
        );
        assert_eq!(
            map_key(key(KeyCode::Down), &InputMode::Editing, None),
            Some(Action::EditCursorDown)
        );
    }

    #[test]
    fn test_editing_mode_home_end() {
        // Home and End with CONTROL modifier map to cursor-home/end.
        assert_eq!(
            map_key(
                KeyEvent::new(KeyCode::Home, KeyModifiers::CONTROL),
                &InputMode::Editing, None
            ),
            Some(Action::EditCursorHome)
        );
        assert_eq!(
            map_key(
                KeyEvent::new(KeyCode::End, KeyModifiers::CONTROL),
                &InputMode::Editing, None
            ),
            Some(Action::EditCursorEnd)
        );
        // Without CONTROL modifier they fall through to None.
        assert_eq!(
            map_key(key(KeyCode::Home), &InputMode::Editing, None),
            None
        );
    }

    #[test]
    fn test_editing_mode_tab() {
        assert_eq!(
            map_key(key(KeyCode::Tab), &InputMode::Editing, None),
            Some(Action::EditTabComplete)
        );
    }

    // ── Dialog mode ──

    #[test]
    fn test_dialog_mode() {
        assert_eq!(map_key(key(KeyCode::Esc), &InputMode::Dialog, None), Some(Action::DialogCancel));
        assert_eq!(map_key(key(KeyCode::Char('n')), &InputMode::Dialog, None), Some(Action::DialogCancel));
        assert_eq!(map_key(key(KeyCode::Enter), &InputMode::Dialog, None), Some(Action::DialogConfirm));
        assert_eq!(map_key(key(KeyCode::Char('y')), &InputMode::Dialog, None), Some(Action::DialogConfirm));
        assert_eq!(map_key(key(KeyCode::Char('j')), &InputMode::Dialog, None), Some(Action::MoveDown));
        assert_eq!(map_key(key(KeyCode::Char('k')), &InputMode::Dialog, None), Some(Action::MoveUp));
        assert_eq!(map_key(key(KeyCode::Char(' ')), &InputMode::Dialog, None), Some(Action::DialogToggle));
        assert_eq!(map_key(ctrl('f'), &InputMode::Dialog, None), Some(Action::PageDown));
        assert_eq!(map_key(ctrl('b'), &InputMode::Dialog, None), Some(Action::PageUp));
        assert_eq!(map_key(key(KeyCode::Char('g')), &InputMode::Dialog, None), Some(Action::MoveToTop));
        assert_eq!(map_key(key(KeyCode::Char('G')), &InputMode::Dialog, None), Some(Action::MoveToBottom));
    }

    // ── Command mode ──

    #[test]
    fn test_command_mode() {
        assert_eq!(map_key(key(KeyCode::Esc), &InputMode::Command, None), Some(Action::ExitCommand));
        assert_eq!(map_key(key(KeyCode::Enter), &InputMode::Command, None), Some(Action::ExecuteCommand));
        assert_eq!(map_key(key(KeyCode::Backspace), &InputMode::Command, None), Some(Action::CommandBackspace));
        assert_eq!(map_key(key(KeyCode::Char('w')), &InputMode::Command, None), Some(Action::CommandChar('w')));
    }

    // ── Settings mode ──

    #[test]
    fn test_settings_mode() {
        assert_eq!(map_key(key(KeyCode::Esc), &InputMode::Settings, None), Some(Action::ExitSettings));
        assert_eq!(map_key(key(KeyCode::Char('q')), &InputMode::Settings, None), None);
        assert_eq!(map_key(key(KeyCode::Char('j')), &InputMode::Settings, None), Some(Action::SettingsMoveDown));
        assert_eq!(map_key(key(KeyCode::Down), &InputMode::Settings, None), Some(Action::SettingsMoveDown));
        assert_eq!(map_key(key(KeyCode::Char('k')), &InputMode::Settings, None), Some(Action::SettingsMoveUp));
        assert_eq!(map_key(key(KeyCode::Char('g')), &InputMode::Settings, None), Some(Action::SettingsMoveToTop));
        assert_eq!(map_key(key(KeyCode::Char('G')), &InputMode::Settings, None), Some(Action::SettingsMoveToBottom));
        assert_eq!(map_key(ctrl('f'), &InputMode::Settings, None), Some(Action::SettingsPageDown));
        assert_eq!(map_key(ctrl('b'), &InputMode::Settings, None), Some(Action::SettingsPageUp));
        assert_eq!(map_key(key(KeyCode::Enter), &InputMode::Settings, None), Some(Action::SettingsToggle));
        assert_eq!(map_key(key(KeyCode::Char(' ')), &InputMode::Settings, None), Some(Action::SettingsToggle));
        assert_eq!(map_key(key(KeyCode::Char('e')), &InputMode::Settings, None), Some(Action::SettingsEdit));
        assert_eq!(map_key(key(KeyCode::Char('E')), &InputMode::Settings, None), Some(Action::SettingsExport));
        assert_eq!(map_key(key(KeyCode::Char('I')), &InputMode::Settings, None), Some(Action::SettingsImport));
    }

    // ── CitationPreview mode ──

    #[test]
    fn test_citation_preview_mode() {
        assert_eq!(
            map_key(key(KeyCode::Esc), &InputMode::CitationPreview, None),
            Some(Action::CloseCitationPreview)
        );
        assert_eq!(
            map_key(key(KeyCode::Char('q')), &InputMode::CitationPreview, None),
            Some(Action::CloseCitationPreview)
        );
    }

    #[test]
    fn test_citation_preview_yy() {
        // First 'y' returns None (waiting for second)
        assert_eq!(
            map_key(key(KeyCode::Char('y')), &InputMode::CitationPreview, None),
            None
        );
        // Second 'y' returns YankCitationPreview
        assert_eq!(
            map_key(key(KeyCode::Char('y')), &InputMode::CitationPreview, Some('y')),
            Some(Action::YankCitationPreview)
        );
    }
}
