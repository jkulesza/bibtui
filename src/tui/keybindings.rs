use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::Action;

/// Map a key event to an action based on the current mode.
/// `is_message_dialog` is true when a `DialogKind::Message` popup is active;
/// in that mode `yy` copies the message instead of `y` confirming.
pub fn map_key(
    key: KeyEvent,
    mode: &InputMode,
    last_key: Option<char>,
    is_message_dialog: bool,
) -> Option<Action> {
    match mode {
        InputMode::Normal => map_normal_key(key, last_key),
        InputMode::Search => map_search_key(key),
        InputMode::Detail => map_detail_key(key, last_key),
        InputMode::DetailSearch => map_detail_search_key(key),
        InputMode::Editing => map_editing_key(key),
        InputMode::Dialog => map_dialog_key(key, last_key, is_message_dialog),
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
    DetailSearch,
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

fn map_detail_key(key: KeyEvent, last_key: Option<char>) -> Option<Action> {
    match key.code {
        KeyCode::Esc => Some(Action::CloseDetail),
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
        KeyCode::Char('e') | KeyCode::Char('i') | KeyCode::Enter => Some(Action::EditField),
        KeyCode::Char('a') => Some(Action::NormalizeAuthor),
        KeyCode::Char('A') => Some(Action::AddField),
        KeyCode::Char('f') => Some(Action::AddFileAttachment),
        KeyCode::Char('d') => Some(Action::DeleteField),
        KeyCode::Char('T') => Some(Action::TitlecaseField),
        KeyCode::Char('o') => Some(Action::OpenFile),
        KeyCode::Char('w') => Some(Action::OpenWeb),
        KeyCode::Tab => Some(Action::EditGroups),
        KeyCode::Char('c') => Some(Action::RegenCitekey),
        KeyCode::Char('L') => Some(Action::ToggleLatex),
        KeyCode::Char('B') => Some(Action::ToggleBraces),
        KeyCode::Char('u') => Some(Action::Undo),
        KeyCode::Char('/') => Some(Action::EnterDetailSearch),
        KeyCode::Char('n') => Some(Action::DetailNextMatch),
        KeyCode::Char('N') => Some(Action::DetailPrevMatch),
        _ => None,
    }
}

fn map_detail_search_key(key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Esc => Some(Action::ExitDetailSearch),
        KeyCode::Enter => Some(Action::ExitDetailSearch),
        KeyCode::Backspace => Some(Action::DetailSearchBackspace),
        KeyCode::Char(c) => Some(Action::DetailSearchChar(c)),
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

fn map_dialog_key(key: KeyEvent, last_key: Option<char>, is_message: bool) -> Option<Action> {
    match key.code {
        KeyCode::Esc | KeyCode::Char('n') => Some(Action::DialogCancel),
        KeyCode::Enter => Some(Action::DialogConfirm),
        KeyCode::Char('y') => {
            if is_message {
                // 'yy' copies the message; single 'y' is pending (do nothing)
                if last_key == Some('y') {
                    Some(Action::DialogYank)
                } else {
                    None
                }
            } else {
                Some(Action::DialogConfirm)
            }
        }
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
        assert_eq!(map_key(key(KeyCode::Char('j')), &InputMode::Normal, None, false), Some(Action::MoveDown));
        assert_eq!(map_key(key(KeyCode::Down), &InputMode::Normal, None, false), Some(Action::MoveDown));
        assert_eq!(map_key(key(KeyCode::Char('k')), &InputMode::Normal, None, false), Some(Action::MoveUp));
        assert_eq!(map_key(key(KeyCode::Up), &InputMode::Normal, None, false), Some(Action::MoveUp));
        assert_eq!(map_key(key(KeyCode::Char('G')), &InputMode::Normal, None, false), Some(Action::MoveToBottom));
    }

    #[test]
    fn test_normal_gg() {
        // First 'g' returns None
        assert_eq!(map_key(key(KeyCode::Char('g')), &InputMode::Normal, None, false), None);
        // Second 'g' returns MoveToTop
        assert_eq!(map_key(key(KeyCode::Char('g')), &InputMode::Normal, Some('g'), false), Some(Action::MoveToTop));
    }

    #[test]
    fn test_normal_page_nav() {
        assert_eq!(map_key(ctrl('f'), &InputMode::Normal, None, false), Some(Action::PageDown));
        assert_eq!(map_key(ctrl('b'), &InputMode::Normal, None, false), Some(Action::PageUp));
    }

    #[test]
    fn test_normal_misc() {
        assert_eq!(map_key(key(KeyCode::Char('u')), &InputMode::Normal, None, false), Some(Action::Undo));
        assert_eq!(map_key(key(KeyCode::Char('/')), &InputMode::Normal, None, false), Some(Action::EnterSearch));
        assert_eq!(map_key(key(KeyCode::Enter), &InputMode::Normal, None, false), Some(Action::OpenDetail));
        assert_eq!(map_key(key(KeyCode::Char('a')), &InputMode::Normal, None, false), Some(Action::AddEntry));
        assert_eq!(map_key(key(KeyCode::Char('D')), &InputMode::Normal, None, false), Some(Action::DuplicateEntry));
        assert_eq!(map_key(key(KeyCode::Tab), &InputMode::Normal, None, false), Some(Action::ToggleGroups));
        assert_eq!(map_key(key(KeyCode::Char('B')), &InputMode::Normal, None, false), Some(Action::ToggleBraces));
        assert_eq!(map_key(key(KeyCode::Char('L')), &InputMode::Normal, None, false), Some(Action::ToggleLatex));
        assert_eq!(map_key(key(KeyCode::Char('S')), &InputMode::Normal, None, false), Some(Action::EnterSettings));
        assert_eq!(map_key(key(KeyCode::Char('?')), &InputMode::Normal, None, false), Some(Action::ShowHelp));
        assert_eq!(map_key(key(KeyCode::Char(':')), &InputMode::Normal, None, false), Some(Action::EnterCommand));
        assert_eq!(map_key(key(KeyCode::Char('I')), &InputMode::Normal, None, false), Some(Action::ImportEntry));
    }

    #[test]
    fn test_normal_dd() {
        assert_eq!(map_key(key(KeyCode::Char('d')), &InputMode::Normal, None, false), None);
        assert_eq!(map_key(key(KeyCode::Char('d')), &InputMode::Normal, Some('d'), false), Some(Action::DeleteEntry));
    }

    #[test]
    fn test_normal_yy() {
        assert_eq!(map_key(key(KeyCode::Char('y')), &InputMode::Normal, None, false), None);
        assert_eq!(map_key(key(KeyCode::Char('y')), &InputMode::Normal, Some('y'), false), Some(Action::YankCitekey));
    }

    #[test]
    fn test_normal_focus() {
        assert_eq!(map_key(key(KeyCode::Char('h')), &InputMode::Normal, None, false), Some(Action::FocusGroups));
        assert_eq!(map_key(key(KeyCode::Left), &InputMode::Normal, None, false), Some(Action::FocusGroups));
        assert_eq!(map_key(key(KeyCode::Char('l')), &InputMode::Normal, None, false), Some(Action::FocusList));
        assert_eq!(map_key(key(KeyCode::Right), &InputMode::Normal, None, false), Some(Action::FocusList));
    }

    // ── Search mode ──

    #[test]
    fn test_search_mode() {
        assert_eq!(map_key(key(KeyCode::Esc), &InputMode::Search, None, false), Some(Action::ExitSearch));
        assert_eq!(map_key(key(KeyCode::Enter), &InputMode::Search, None, false), Some(Action::ConfirmSearch));
        assert_eq!(map_key(key(KeyCode::Backspace), &InputMode::Search, None, false), Some(Action::SearchBackspace));
        assert_eq!(map_key(key(KeyCode::Char('x')), &InputMode::Search, None, false), Some(Action::SearchChar('x')));
    }

    // ── Detail mode ──

    #[test]
    fn test_detail_mode() {
        assert_eq!(map_key(key(KeyCode::Esc), &InputMode::Detail, None, false), Some(Action::CloseDetail));
        assert_eq!(map_key(key(KeyCode::Char('q')), &InputMode::Detail, None, false), None);
        assert_eq!(map_key(key(KeyCode::Char('e')), &InputMode::Detail, None, false), Some(Action::EditField));
        assert_eq!(map_key(key(KeyCode::Char('i')), &InputMode::Detail, None, false), Some(Action::EditField));
        assert_eq!(map_key(key(KeyCode::Enter), &InputMode::Detail, None, false), Some(Action::EditField));
        assert_eq!(map_key(key(KeyCode::Char('a')), &InputMode::Detail, None, false), Some(Action::NormalizeAuthor));
        assert_eq!(map_key(key(KeyCode::Char('A')), &InputMode::Detail, None, false), Some(Action::AddField));
        assert_eq!(map_key(key(KeyCode::Char('f')), &InputMode::Detail, None, false), Some(Action::AddFileAttachment));
        assert_eq!(map_key(key(KeyCode::Char('d')), &InputMode::Detail, None, false), Some(Action::DeleteField));
        assert_eq!(map_key(key(KeyCode::Char('T')), &InputMode::Detail, None, false), Some(Action::TitlecaseField));
        assert_eq!(map_key(key(KeyCode::Char('N')), &InputMode::Detail, None, false), Some(Action::DetailPrevMatch));
        assert_eq!(map_key(key(KeyCode::Char('n')), &InputMode::Detail, None, false), Some(Action::DetailNextMatch));
        assert_eq!(map_key(key(KeyCode::Char('/')), &InputMode::Detail, None, false), Some(Action::EnterDetailSearch));
        assert_eq!(map_key(key(KeyCode::Char('c')), &InputMode::Detail, None, false), Some(Action::RegenCitekey));
        assert_eq!(map_key(key(KeyCode::Char('u')), &InputMode::Detail, None, false), Some(Action::Undo));
        // Navigation
        assert_eq!(map_key(key(KeyCode::Char('G')), &InputMode::Detail, None, false), Some(Action::MoveToBottom));
        assert_eq!(map_key(key(KeyCode::Char('g')), &InputMode::Detail, Some('g'), false), Some(Action::MoveToTop));
        assert_eq!(map_key(key(KeyCode::Char('g')), &InputMode::Detail, None, false), None);
        assert_eq!(map_key(ctrl('f'), &InputMode::Detail, None, false), Some(Action::PageDown));
        assert_eq!(map_key(ctrl('b'), &InputMode::Detail, None, false), Some(Action::PageUp));
        // Group editing moved to Tab
        assert_eq!(map_key(key(KeyCode::Tab), &InputMode::Detail, None, false), Some(Action::EditGroups));
        assert_eq!(map_key(key(KeyCode::Char('g')), &InputMode::Detail, Some('x'), false), None);
    }

    // ── Editing mode ──

    #[test]
    fn test_editing_mode() {
        assert_eq!(map_key(key(KeyCode::Esc), &InputMode::Editing, None, false), Some(Action::CancelEdit));
        assert_eq!(map_key(key(KeyCode::Enter), &InputMode::Editing, None, false), Some(Action::ConfirmEdit));
        assert_eq!(map_key(key(KeyCode::Backspace), &InputMode::Editing, None, false), Some(Action::EditBackspace));
        assert_eq!(map_key(key(KeyCode::Left), &InputMode::Editing, None, false), Some(Action::EditCursorLeft));
        assert_eq!(map_key(key(KeyCode::Right), &InputMode::Editing, None, false), Some(Action::EditCursorRight));
        assert_eq!(map_key(key(KeyCode::Delete), &InputMode::Editing, None, false), Some(Action::EditDelete));
        assert_eq!(map_key(ctrl('a'), &InputMode::Editing, None, false), Some(Action::EditCursorHome));
        assert_eq!(map_key(ctrl('e'), &InputMode::Editing, None, false), Some(Action::EditCursorEnd));
        assert_eq!(map_key(key(KeyCode::Char('z')), &InputMode::Editing, None, false), Some(Action::EditChar('z')));
    }

    #[test]
    fn test_editing_mode_up_down() {
        // Up/Down arrow keys in editing mode map to EditCursorUp/Down (used by month navigation).
        assert_eq!(
            map_key(key(KeyCode::Up), &InputMode::Editing, None, false),
            Some(Action::EditCursorUp)
        );
        assert_eq!(
            map_key(key(KeyCode::Down), &InputMode::Editing, None, false),
            Some(Action::EditCursorDown)
        );
    }

    #[test]
    fn test_editing_mode_home_end() {
        // Home and End with CONTROL modifier map to cursor-home/end.
        assert_eq!(
            map_key(
                KeyEvent::new(KeyCode::Home, KeyModifiers::CONTROL),
                &InputMode::Editing, None, false
            ),
            Some(Action::EditCursorHome)
        );
        assert_eq!(
            map_key(
                KeyEvent::new(KeyCode::End, KeyModifiers::CONTROL),
                &InputMode::Editing, None, false
            ),
            Some(Action::EditCursorEnd)
        );
        // Without CONTROL modifier they fall through to None.
        assert_eq!(
            map_key(key(KeyCode::Home), &InputMode::Editing, None, false),
            None
        );
    }

    #[test]
    fn test_editing_mode_tab() {
        assert_eq!(
            map_key(key(KeyCode::Tab), &InputMode::Editing, None, false),
            Some(Action::EditTabComplete)
        );
    }

    // ── Dialog mode ──

    #[test]
    fn test_dialog_mode() {
        // Non-message dialogs: 'y' confirms, 'n'/Esc cancels
        assert_eq!(map_key(key(KeyCode::Esc), &InputMode::Dialog, None, false), Some(Action::DialogCancel));
        assert_eq!(map_key(key(KeyCode::Char('n')), &InputMode::Dialog, None, false), Some(Action::DialogCancel));
        assert_eq!(map_key(key(KeyCode::Enter), &InputMode::Dialog, None, false), Some(Action::DialogConfirm));
        assert_eq!(map_key(key(KeyCode::Char('y')), &InputMode::Dialog, None, false), Some(Action::DialogConfirm));
        assert_eq!(map_key(key(KeyCode::Char('j')), &InputMode::Dialog, None, false), Some(Action::MoveDown));
        assert_eq!(map_key(key(KeyCode::Char('k')), &InputMode::Dialog, None, false), Some(Action::MoveUp));
        assert_eq!(map_key(key(KeyCode::Char(' ')), &InputMode::Dialog, None, false), Some(Action::DialogToggle));
        assert_eq!(map_key(ctrl('f'), &InputMode::Dialog, None, false), Some(Action::PageDown));
        assert_eq!(map_key(ctrl('b'), &InputMode::Dialog, None, false), Some(Action::PageUp));
        assert_eq!(map_key(key(KeyCode::Char('g')), &InputMode::Dialog, None, false), Some(Action::MoveToTop));
        assert_eq!(map_key(key(KeyCode::Char('G')), &InputMode::Dialog, None, false), Some(Action::MoveToBottom));
    }

    #[test]
    fn test_message_dialog_yy() {
        // In a Message dialog: first 'y' does nothing (pending chord)
        assert_eq!(map_key(key(KeyCode::Char('y')), &InputMode::Dialog, None, true), None);
        // Second 'y' (yy) yanks the message
        assert_eq!(map_key(key(KeyCode::Char('y')), &InputMode::Dialog, Some('y'), true), Some(Action::DialogYank));
        // Enter still dismisses
        assert_eq!(map_key(key(KeyCode::Enter), &InputMode::Dialog, None, true), Some(Action::DialogConfirm));
        // Esc still cancels
        assert_eq!(map_key(key(KeyCode::Esc), &InputMode::Dialog, None, true), Some(Action::DialogCancel));
    }

    // ── Command mode ──

    #[test]
    fn test_command_mode() {
        assert_eq!(map_key(key(KeyCode::Esc), &InputMode::Command, None, false), Some(Action::ExitCommand));
        assert_eq!(map_key(key(KeyCode::Enter), &InputMode::Command, None, false), Some(Action::ExecuteCommand));
        assert_eq!(map_key(key(KeyCode::Backspace), &InputMode::Command, None, false), Some(Action::CommandBackspace));
        assert_eq!(map_key(key(KeyCode::Char('w')), &InputMode::Command, None, false), Some(Action::CommandChar('w')));
    }

    // ── Settings mode ──

    #[test]
    fn test_settings_mode() {
        assert_eq!(map_key(key(KeyCode::Esc), &InputMode::Settings, None, false), Some(Action::ExitSettings));
        assert_eq!(map_key(key(KeyCode::Char('q')), &InputMode::Settings, None, false), None);
        assert_eq!(map_key(key(KeyCode::Char('j')), &InputMode::Settings, None, false), Some(Action::SettingsMoveDown));
        assert_eq!(map_key(key(KeyCode::Down), &InputMode::Settings, None, false), Some(Action::SettingsMoveDown));
        assert_eq!(map_key(key(KeyCode::Char('k')), &InputMode::Settings, None, false), Some(Action::SettingsMoveUp));
        assert_eq!(map_key(key(KeyCode::Char('g')), &InputMode::Settings, None, false), Some(Action::SettingsMoveToTop));
        assert_eq!(map_key(key(KeyCode::Char('G')), &InputMode::Settings, None, false), Some(Action::SettingsMoveToBottom));
        assert_eq!(map_key(ctrl('f'), &InputMode::Settings, None, false), Some(Action::SettingsPageDown));
        assert_eq!(map_key(ctrl('b'), &InputMode::Settings, None, false), Some(Action::SettingsPageUp));
        assert_eq!(map_key(key(KeyCode::Enter), &InputMode::Settings, None, false), Some(Action::SettingsToggle));
        assert_eq!(map_key(key(KeyCode::Char(' ')), &InputMode::Settings, None, false), Some(Action::SettingsToggle));
        assert_eq!(map_key(key(KeyCode::Char('e')), &InputMode::Settings, None, false), Some(Action::SettingsEdit));
        assert_eq!(map_key(key(KeyCode::Char('E')), &InputMode::Settings, None, false), Some(Action::SettingsExport));
        assert_eq!(map_key(key(KeyCode::Char('I')), &InputMode::Settings, None, false), Some(Action::SettingsImport));
    }

    #[test]
    fn test_normal_mode_extra_keys() {
        assert_eq!(map_key(key(KeyCode::Char(' ')), &InputMode::Normal, None, false), Some(Action::ShowCitationPreview));
        assert_eq!(map_key(key(KeyCode::Char('o')), &InputMode::Normal, None, false), Some(Action::OpenFile));
        assert_eq!(map_key(key(KeyCode::Char('w')), &InputMode::Normal, None, false), Some(Action::OpenWeb));
        assert_eq!(map_key(key(KeyCode::Char('v')), &InputMode::Normal, None, false), Some(Action::Validate));
        // Unknown key → None
        assert_eq!(map_key(key(KeyCode::F(1)), &InputMode::Normal, None, false), None);
    }

    #[test]
    fn test_search_mode_unknown_key() {
        assert_eq!(map_key(key(KeyCode::F(1)), &InputMode::Search, None, false), None);
    }

    #[test]
    fn test_detail_mode_extra_keys() {
        assert_eq!(map_key(key(KeyCode::Char('o')), &InputMode::Detail, None, false), Some(Action::OpenFile));
        assert_eq!(map_key(key(KeyCode::Char('w')), &InputMode::Detail, None, false), Some(Action::OpenWeb));
        assert_eq!(map_key(key(KeyCode::Char('L')), &InputMode::Detail, None, false), Some(Action::ToggleLatex));
        assert_eq!(map_key(key(KeyCode::Char('B')), &InputMode::Detail, None, false), Some(Action::ToggleBraces));
    }

    // ── DetailSearch mode ──

    #[test]
    fn test_detail_search_mode() {
        assert_eq!(map_key(key(KeyCode::Esc),       &InputMode::DetailSearch, None, false), Some(Action::ExitDetailSearch));
        assert_eq!(map_key(key(KeyCode::Enter),     &InputMode::DetailSearch, None, false), Some(Action::ExitDetailSearch));
        assert_eq!(map_key(key(KeyCode::Backspace), &InputMode::DetailSearch, None, false), Some(Action::DetailSearchBackspace));
        assert_eq!(map_key(key(KeyCode::Char('a')), &InputMode::DetailSearch, None, false), Some(Action::DetailSearchChar('a')));
        assert_eq!(map_key(key(KeyCode::F(1)),      &InputMode::DetailSearch, None, false), None);
    }

    // ── Help mode ──

    #[test]
    fn test_help_mode() {
        // Any key in Help mode closes help
        assert_eq!(map_key(key(KeyCode::Esc),       &InputMode::Help, None, false), Some(Action::CloseHelp));
        assert_eq!(map_key(key(KeyCode::Char('q')), &InputMode::Help, None, false), Some(Action::CloseHelp));
    }

    // ── Dialog mode (catch-all) ──

    #[test]
    fn test_dialog_unknown_key() {
        assert_eq!(map_key(key(KeyCode::F(1)), &InputMode::Dialog, None, false), None);
    }

    // ── Command mode ──

    #[test]
    fn test_command_mode_extra_keys() {
        assert_eq!(map_key(key(KeyCode::Tab),   &InputMode::Command, None, false), Some(Action::CommandTabComplete));
        assert_eq!(map_key(key(KeyCode::F(1)),  &InputMode::Command, None, false), None);
    }

    // ── Settings mode (field group operations) ──

    #[test]
    fn test_settings_mode_field_group_ops() {
        assert_eq!(map_key(key(KeyCode::Char('a')), &InputMode::Settings, None, false), Some(Action::SettingsAddFieldGroup));
        assert_eq!(map_key(key(KeyCode::Char('x')), &InputMode::Settings, None, false), Some(Action::SettingsDeleteFieldGroup));
        assert_eq!(map_key(key(KeyCode::Char('r')), &InputMode::Settings, None, false), Some(Action::SettingsRenameFieldGroup));
    }

    // ── ValidateResults mode ──

    #[test]
    fn test_validate_results_mode() {
        assert_eq!(map_key(key(KeyCode::Char('j')), &InputMode::ValidateResults, None, false), Some(Action::MoveDown));
        assert_eq!(map_key(key(KeyCode::Down),      &InputMode::ValidateResults, None, false), Some(Action::MoveDown));
        assert_eq!(map_key(key(KeyCode::Char('k')), &InputMode::ValidateResults, None, false), Some(Action::MoveUp));
        assert_eq!(map_key(key(KeyCode::Up),        &InputMode::ValidateResults, None, false), Some(Action::MoveUp));
        assert_eq!(map_key(key(KeyCode::Esc),       &InputMode::ValidateResults, None, false), Some(Action::CloseValidateResults));
        assert_eq!(map_key(key(KeyCode::Char('q')), &InputMode::ValidateResults, None, false), Some(Action::CloseValidateResults));
        assert_eq!(map_key(key(KeyCode::F(1)),      &InputMode::ValidateResults, None, false), None);
    }

    // ── CitationPreview mode ──

    #[test]
    fn test_citation_preview_mode() {
        assert_eq!(
            map_key(key(KeyCode::Esc), &InputMode::CitationPreview, None, false),
            Some(Action::CloseCitationPreview)
        );
        assert_eq!(
            map_key(key(KeyCode::Char('q')), &InputMode::CitationPreview, None, false),
            Some(Action::CloseCitationPreview)
        );
        assert_eq!(
            map_key(key(KeyCode::Char('j')), &InputMode::CitationPreview, None, false),
            Some(Action::MoveDown)
        );
        assert_eq!(
            map_key(key(KeyCode::Char('k')), &InputMode::CitationPreview, None, false),
            Some(Action::MoveUp)
        );
    }

    #[test]
    fn test_citation_preview_yy() {
        // First 'y' returns None (waiting for second)
        assert_eq!(
            map_key(key(KeyCode::Char('y')), &InputMode::CitationPreview, None, false),
            None
        );
        // Second 'y' returns YankCitationPreview
        assert_eq!(
            map_key(key(KeyCode::Char('y')), &InputMode::CitationPreview, Some('y'), false),
            Some(Action::YankCitationPreview)
        );
    }
}
