use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::app::Action;

/// Map a key event to an action based on the current mode.
/// `is_message_dialog` is true when a `DialogKind::Message` popup is active;
/// in that mode `yy` copies the message instead of `y` confirming.
/// `edit_normal` is true when the field editor is in vim Normal mode.
pub fn map_key(
    key: KeyEvent,
    mode: &InputMode,
    second_last_key: Option<char>,
    last_key: Option<char>,
    is_message_dialog: bool,
    edit_normal: bool,
) -> Option<Action> {
    match mode {
        InputMode::Normal => map_normal_key(key, last_key),
        InputMode::Search => map_search_key(key),
        InputMode::Detail => map_detail_key(key, last_key),
        InputMode::DetailSearch => map_detail_search_key(key),
        InputMode::Editing => map_editing_key(key, edit_normal, second_last_key, last_key),
        InputMode::Dialog => map_dialog_key(key, last_key, is_message_dialog),
        InputMode::Command => map_command_key(key),
        InputMode::CitationPreview => map_citation_preview_key(key, last_key),
        InputMode::Settings => map_settings_key(key),
        InputMode::ValidateResults => map_validate_results_key(key),
        InputMode::NameDisambig => map_name_disambig_key(key),
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
    NameDisambig,
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
        KeyCode::Char('C') => Some(Action::RegenAllCitekeys),
        KeyCode::Char('M') => Some(Action::DisambiguateNames),
        KeyCode::Char('F') => Some(Action::SyncFilenames),
        KeyCode::Esc => Some(Action::ResetSort),
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
        KeyCode::Char('a') => Some(Action::NormalizeNames),
        KeyCode::Char('A') => Some(Action::AddField),
        KeyCode::Char('f') => Some(Action::AddFileAttachment),
        KeyCode::Char('d') => Some(Action::DeleteField),
        KeyCode::Char('T') => Some(Action::TitlecaseField),
        KeyCode::Char('o') => Some(Action::OpenFile),
        KeyCode::Char('w') => Some(Action::OpenWeb),
        KeyCode::Tab => Some(Action::EditGroups),
        KeyCode::Char('c') => Some(Action::RegenCitekey),
        KeyCode::Char('t') => Some(Action::ChangeEntryType),
        KeyCode::Char('L') => Some(Action::ToggleLatex),
        KeyCode::Char('B') => Some(Action::ToggleBraces),
        KeyCode::Char('u') => Some(Action::Undo),
        KeyCode::Char('/') => Some(Action::EnterDetailSearch),
        KeyCode::Char('n') => Some(Action::DetailNextMatch),
        KeyCode::Char('N') => Some(Action::DetailPrevMatch),
        KeyCode::Char('?') => Some(Action::ShowHelp),
        KeyCode::Char('F') => Some(Action::SyncEntryFilename),
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

fn map_editing_key(key: KeyEvent, is_normal: bool, second_last_key: Option<char>, last_key: Option<char>) -> Option<Action> {
    if is_normal {
        map_editing_normal_key(key, second_last_key, last_key)
    } else {
        map_editing_insert_key(key)
    }
}

fn map_editing_insert_key(key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Esc => Some(Action::EditEnterNormal),
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
        KeyCode::Char('w') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(Action::EditDeleteWordBack)
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(Action::EditDeleteToHome)
        }
        KeyCode::Delete => Some(Action::EditDelete),
        KeyCode::Tab => Some(Action::EditTabComplete),
        KeyCode::BackTab => Some(Action::EditTabCompleteReverse),
        KeyCode::Char(c) => Some(Action::EditChar(c)),
        _ => None,
    }
}

fn map_editing_normal_key(key: KeyEvent, second_last_key: Option<char>, last_key: Option<char>) -> Option<Action> {
    match key.code {
        KeyCode::Esc => Some(Action::CancelEdit),
        KeyCode::Enter => Some(Action::ConfirmEdit),
        KeyCode::Left => Some(Action::EditCursorLeft),
        KeyCode::Right => Some(Action::EditCursorRight),
        KeyCode::Up => Some(Action::EditConfirmAndMoveUp),
        KeyCode::Down => Some(Action::EditConfirmAndMoveDown),
        KeyCode::Home => Some(Action::EditCursorHome),
        KeyCode::End => Some(Action::EditCursorEnd),
        KeyCode::Char(c) => {
            // 3-key sequences: d+t/T/f/F take priority over 2-key ones.
            match (second_last_key, last_key) {
                (Some('d'), Some('t')) => Some(Action::EditDeleteToChar(c)),
                (Some('d'), Some('T')) => Some(Action::EditDeleteToCharBack(c)),
                (Some('d'), Some('f')) => Some(Action::EditDeleteThroughChar(c)),
                (Some('d'), Some('F')) => Some(Action::EditDeleteThroughCharBack(c)),
                _ => match last_key {
                    // 2-key pending-dispatch: r/f/F/t/T consume the next char.
                    Some('r') => Some(Action::EditReplaceChar(c)),
                    Some('f') => Some(Action::EditFindCharFwd(c)),
                    Some('F') => Some(Action::EditFindCharBwd(c)),
                    Some('t') => Some(Action::EditFindToCharFwd(c)),
                    Some('T') => Some(Action::EditFindToCharBwd(c)),
                    _ => match c {
                        'i' => Some(Action::EditEnterInsert),
                        'a' => Some(Action::EditEnterInsertAfter),
                        'A' => Some(Action::EditEnterInsertAtEnd),
                        'I' => Some(Action::EditEnterInsertAtHome),
                        'R' => Some(Action::EditEnterReplace),
                        'h' => Some(Action::EditCursorLeft),
                        'l' => Some(Action::EditCursorRight),
                        '0' => Some(Action::EditCursorHome),
                        '$' => Some(Action::EditCursorEnd),
                        'j' => Some(Action::EditConfirmAndMoveDown),
                        'k' => Some(Action::EditConfirmAndMoveUp),
                        'x' => Some(Action::EditDelete),
                        'X' => Some(Action::EditDeleteCharBack),
                        'D' => Some(Action::EditDeleteToEnd),
                        'C' => Some(Action::EditChangeToEnd),
                        's' => Some(Action::EditSubstituteChar),
                        'S' => Some(Action::EditSubstituteLine),
                        '~' => Some(Action::EditToggleCase),
                        'w' if last_key == Some('d') => Some(Action::EditDeleteWordFwd),
                        'w' => Some(Action::EditMoveWordFwd),
                        'b' => Some(Action::EditMoveWordBwd),
                        'e' => Some(Action::EditMoveWordEnd),
                        'W' => Some(Action::EditMoveBigWordFwd),
                        'B' => Some(Action::EditMoveBigWordBwd),
                        'E' => Some(Action::EditMoveBigWordEnd),
                        'u' => Some(Action::EditUndo),
                        'p' => Some(Action::EditPut),
                        'y' if last_key == Some('y') => Some(Action::EditYank),
                        // Pending keys — return None; next keypress checks last_key.
                        'r' | 'f' | 'F' | 't' | 'T' | 'd' | 'y' => None,
                        _ => None,
                    },
                },
            }
        }
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
        KeyCode::BackTab => Some(Action::CommandTabCompleteReverse),
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

fn map_name_disambig_key(key: KeyEvent) -> Option<Action> {
    match key.code {
        KeyCode::Char('j') | KeyCode::Down => Some(Action::MoveDown),
        KeyCode::Char('k') | KeyCode::Up => Some(Action::MoveUp),
        KeyCode::Char('g') => Some(Action::MoveToTop),
        KeyCode::Char('G') => Some(Action::MoveToBottom),
        KeyCode::Char('f') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(Action::PageDown)
        }
        KeyCode::Char('b') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            Some(Action::PageUp)
        }
        KeyCode::Tab => Some(Action::DisambigCycleVariant),
        KeyCode::BackTab => Some(Action::DisambigCycleVariantReverse),
        KeyCode::Char('x') => Some(Action::DisambigRemoveVariant),
        KeyCode::Char(' ') => Some(Action::DisambigPreview),
        KeyCode::Enter => Some(Action::ApplyNameDisambig),
        KeyCode::Esc | KeyCode::Char('q') => Some(Action::CloseNameDisambig),
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

// ── User-configurable keybinding helpers ─────────────────────────────────────

/// Parse a key-spec string into a `(KeyCode, KeyModifiers)` pair.
///
/// Supported formats:
/// - Single printable char: `"j"`, `"/"`, `"A"` (uppercase char implies no extra modifier)
/// - Named special keys: `"enter"`, `"esc"`, `"backspace"`, `"delete"`, `"tab"`,
///   `"space"`, `"home"`, `"end"`, `"pageup"`, `"pagedown"`,
///   `"up"`, `"down"`, `"left"`, `"right"`, `"f1"`–`"f12"`
/// - Ctrl combos: `"ctrl-j"`, `"ctrl-f"`, `"ctrl-enter"` (key name after the dash)
/// - Shift combos: `"shift-f1"` (for function keys; regular chars use uppercase directly)
///
/// Returns `None` for unrecognised specs.
pub fn parse_key_spec(spec: &str) -> Option<(KeyCode, KeyModifiers)> {
    let spec = spec.trim().to_lowercase();

    // ctrl- prefix
    if let Some(rest) = spec.strip_prefix("ctrl-") {
        let (code, _) = parse_bare_key(rest)?;
        return Some((code, KeyModifiers::CONTROL));
    }

    // shift- prefix (mainly useful for function keys)
    if let Some(rest) = spec.strip_prefix("shift-") {
        let (code, _) = parse_bare_key(rest)?;
        return Some((code, KeyModifiers::SHIFT));
    }

    // alt- prefix
    if let Some(rest) = spec.strip_prefix("alt-") {
        let (code, _) = parse_bare_key(rest)?;
        return Some((code, KeyModifiers::ALT));
    }

    parse_bare_key(&spec)
}

fn parse_bare_key(s: &str) -> Option<(KeyCode, KeyModifiers)> {
    match s {
        "enter"     => Some((KeyCode::Enter,     KeyModifiers::NONE)),
        "esc"       => Some((KeyCode::Esc,        KeyModifiers::NONE)),
        "backspace" => Some((KeyCode::Backspace,  KeyModifiers::NONE)),
        "delete"    => Some((KeyCode::Delete,     KeyModifiers::NONE)),
        "tab"       => Some((KeyCode::Tab,        KeyModifiers::NONE)),
        "backtab" | "shift-tab" => Some((KeyCode::BackTab, KeyModifiers::SHIFT)),
        "space"     => Some((KeyCode::Char(' '),  KeyModifiers::NONE)),
        "home"      => Some((KeyCode::Home,       KeyModifiers::NONE)),
        "end"       => Some((KeyCode::End,        KeyModifiers::NONE)),
        "pageup"    => Some((KeyCode::PageUp,     KeyModifiers::NONE)),
        "pagedown"  => Some((KeyCode::PageDown,   KeyModifiers::NONE)),
        "up"        => Some((KeyCode::Up,         KeyModifiers::NONE)),
        "down"      => Some((KeyCode::Down,       KeyModifiers::NONE)),
        "left"      => Some((KeyCode::Left,       KeyModifiers::NONE)),
        "right"     => Some((KeyCode::Right,      KeyModifiers::NONE)),
        "f1"        => Some((KeyCode::F(1),       KeyModifiers::NONE)),
        "f2"        => Some((KeyCode::F(2),       KeyModifiers::NONE)),
        "f3"        => Some((KeyCode::F(3),       KeyModifiers::NONE)),
        "f4"        => Some((KeyCode::F(4),       KeyModifiers::NONE)),
        "f5"        => Some((KeyCode::F(5),       KeyModifiers::NONE)),
        "f6"        => Some((KeyCode::F(6),       KeyModifiers::NONE)),
        "f7"        => Some((KeyCode::F(7),       KeyModifiers::NONE)),
        "f8"        => Some((KeyCode::F(8),       KeyModifiers::NONE)),
        "f9"        => Some((KeyCode::F(9),       KeyModifiers::NONE)),
        "f10"       => Some((KeyCode::F(10),      KeyModifiers::NONE)),
        "f11"       => Some((KeyCode::F(11),      KeyModifiers::NONE)),
        "f12"       => Some((KeyCode::F(12),      KeyModifiers::NONE)),
        other => {
            // Must be exactly one char
            let mut chars = other.chars();
            let c = chars.next()?;
            if chars.next().is_some() {
                return None; // more than one char
            }
            Some((KeyCode::Char(c), KeyModifiers::NONE))
        }
    }
}

/// Map an action name string to the corresponding `Action` variant.
///
/// Only no-payload variants are supported (variants that carry a `char` cannot
/// be expressed as a plain name).  Returns `None` for unknown names and for the
/// special sentinel `"None"` (which can be used to intentionally unbind a key).
pub fn action_from_name(name: &str) -> Option<Action> {
    match name {
        "MoveDown"                  => Some(Action::MoveDown),
        "MoveUp"                    => Some(Action::MoveUp),
        "MoveToTop"                 => Some(Action::MoveToTop),
        "MoveToBottom"              => Some(Action::MoveToBottom),
        "PageDown"                  => Some(Action::PageDown),
        "PageUp"                    => Some(Action::PageUp),
        "EnterSearch"               => Some(Action::EnterSearch),
        "ExitSearch"                => Some(Action::ExitSearch),
        "ConfirmSearch"             => Some(Action::ConfirmSearch),
        "SearchBackspace"           => Some(Action::SearchBackspace),
        "OpenDetail"                => Some(Action::OpenDetail),
        "CloseDetail"               => Some(Action::CloseDetail),
        "EnterDetailSearch"         => Some(Action::EnterDetailSearch),
        "ExitDetailSearch"          => Some(Action::ExitDetailSearch),
        "DetailSearchBackspace"     => Some(Action::DetailSearchBackspace),
        "DetailNextMatch"           => Some(Action::DetailNextMatch),
        "DetailPrevMatch"           => Some(Action::DetailPrevMatch),
        "EditField"                 => Some(Action::EditField),
        "AddField"                  => Some(Action::AddField),
        "AddFileAttachment"         => Some(Action::AddFileAttachment),
        "DeleteField"               => Some(Action::DeleteField),
        "EditGroups"                => Some(Action::EditGroups),
        "RegenCitekey"              => Some(Action::RegenCitekey),
        "RegenAllCitekeys"          => Some(Action::RegenAllCitekeys),
        "SyncFilenames"             => Some(Action::SyncFilenames),
        "ConfirmEdit"               => Some(Action::ConfirmEdit),
        "CancelEdit"                => Some(Action::CancelEdit),
        "EditBackspace"             => Some(Action::EditBackspace),
        "EditDelete"                => Some(Action::EditDelete),
        "EditCursorLeft"            => Some(Action::EditCursorLeft),
        "EditCursorRight"           => Some(Action::EditCursorRight),
        "EditCursorUp"              => Some(Action::EditCursorUp),
        "EditCursorDown"            => Some(Action::EditCursorDown),
        "EditCursorHome"            => Some(Action::EditCursorHome),
        "EditCursorEnd"             => Some(Action::EditCursorEnd),
        "EditTabComplete"           => Some(Action::EditTabComplete),
        "EditTabCompleteReverse"    => Some(Action::EditTabCompleteReverse),
        "AddEntry"                  => Some(Action::AddEntry),
        "DeleteEntry"               => Some(Action::DeleteEntry),
        "DuplicateEntry"            => Some(Action::DuplicateEntry),
        "YankCitekey"               => Some(Action::YankCitekey),
        "ToggleGroups"              => Some(Action::ToggleGroups),
        "FocusGroups"               => Some(Action::FocusGroups),
        "FocusList"                 => Some(Action::FocusList),
        "ShowCitationPreview"       => Some(Action::ShowCitationPreview),
        "EnterCommand"              => Some(Action::EnterCommand),
        "ExitCommand"               => Some(Action::ExitCommand),
        "ExecuteCommand"            => Some(Action::ExecuteCommand),
        "CommandBackspace"          => Some(Action::CommandBackspace),
        "CommandTabComplete"        => Some(Action::CommandTabComplete),
        "CommandTabCompleteReverse" => Some(Action::CommandTabCompleteReverse),
        "DialogConfirm"             => Some(Action::DialogConfirm),
        "DialogCancel"              => Some(Action::DialogCancel),
        "DialogToggle"              => Some(Action::DialogToggle),
        "DialogYank"                => Some(Action::DialogYank),
        "ShowHelp"                  => Some(Action::ShowHelp),
        "TitlecaseField"            => Some(Action::TitlecaseField),
        "ChangeEntryType"           => Some(Action::ChangeEntryType),
        "ToggleBraces"              => Some(Action::ToggleBraces),
        "ToggleLatex"               => Some(Action::ToggleLatex),
        "NormalizeNames"            => Some(Action::NormalizeNames),
        "OpenFile"                  => Some(Action::OpenFile),
        "OpenWeb"                   => Some(Action::OpenWeb),
        "CloseCitationPreview"      => Some(Action::CloseCitationPreview),
        "YankCitationPreview"       => Some(Action::YankCitationPreview),
        "Undo"                      => Some(Action::Undo),
        "EnterSettings"             => Some(Action::EnterSettings),
        "ExitSettings"              => Some(Action::ExitSettings),
        "SettingsMoveDown"          => Some(Action::SettingsMoveDown),
        "SettingsMoveUp"            => Some(Action::SettingsMoveUp),
        "SettingsToggle"            => Some(Action::SettingsToggle),
        "SettingsEdit"              => Some(Action::SettingsEdit),
        "SettingsExport"            => Some(Action::SettingsExport),
        "SettingsImport"            => Some(Action::SettingsImport),
        "SettingsAddFieldGroup"     => Some(Action::SettingsAddFieldGroup),
        "SettingsDeleteFieldGroup"  => Some(Action::SettingsDeleteFieldGroup),
        "SettingsRenameFieldGroup"  => Some(Action::SettingsRenameFieldGroup),
        "SettingsMoveToTop"         => Some(Action::SettingsMoveToTop),
        "SettingsMoveToBottom"      => Some(Action::SettingsMoveToBottom),
        "SettingsPageDown"          => Some(Action::SettingsPageDown),
        "SettingsPageUp"            => Some(Action::SettingsPageUp),
        "Validate"                  => Some(Action::Validate),
        "CloseValidateResults"      => Some(Action::CloseValidateResults),
        "DisambiguateNames"         => Some(Action::DisambiguateNames),
        "CloseNameDisambig"         => Some(Action::CloseNameDisambig),
        "ApplyNameDisambig"         => Some(Action::ApplyNameDisambig),
        "DisambigCycleVariant"      => Some(Action::DisambigCycleVariant),
        "DisambigCycleVariantReverse" => Some(Action::DisambigCycleVariantReverse),
        "DisambigRemoveVariant"     => Some(Action::DisambigRemoveVariant),
        "DisambigPreview"           => Some(Action::DisambigPreview),
        "ImportEntry"               => Some(Action::ImportEntry),
        "ExportJson"                => Some(Action::ExportJson),
        "ExportRis"                 => Some(Action::ExportRis),
        "SyncEntryFilename"         => Some(Action::SyncEntryFilename),
        "CloseHelp"                 => Some(Action::CloseHelp),
        "EditUndo"                  => Some(Action::EditUndo),
        "EditPut"                   => Some(Action::EditPut),
        "EditYank"                  => Some(Action::EditYank),
        "EditEnterNormal"           => Some(Action::EditEnterNormal),
        "EditEnterInsert"           => Some(Action::EditEnterInsert),
        "EditEnterInsertAfter"      => Some(Action::EditEnterInsertAfter),
        "EditEnterInsertAtEnd"      => Some(Action::EditEnterInsertAtEnd),
        "EditEnterInsertAtHome"     => Some(Action::EditEnterInsertAtHome),
        "EditMoveWordFwd"           => Some(Action::EditMoveWordFwd),
        "EditMoveWordBwd"           => Some(Action::EditMoveWordBwd),
        "EditMoveWordEnd"           => Some(Action::EditMoveWordEnd),
        "EditMoveBigWordFwd"        => Some(Action::EditMoveBigWordFwd),
        "EditMoveBigWordBwd"        => Some(Action::EditMoveBigWordBwd),
        "EditMoveBigWordEnd"        => Some(Action::EditMoveBigWordEnd),
        "EditDeleteWordFwd"         => Some(Action::EditDeleteWordFwd),
        "EditDeleteToEnd"           => Some(Action::EditDeleteToEnd),
        "EditChangeToEnd"           => Some(Action::EditChangeToEnd),
        "EditSubstituteChar"        => Some(Action::EditSubstituteChar),
        "EditSubstituteLine"        => Some(Action::EditSubstituteLine),
        "EditToggleCase"            => Some(Action::EditToggleCase),
        "EditDeleteCharBack"        => Some(Action::EditDeleteCharBack),
        "EditDeleteWordBack"        => Some(Action::EditDeleteWordBack),
        "EditDeleteToHome"          => Some(Action::EditDeleteToHome),
        "EditConfirmAndMoveDown"    => Some(Action::EditConfirmAndMoveDown),
        "EditConfirmAndMoveUp"      => Some(Action::EditConfirmAndMoveUp),
        _ => None,
    }
}

fn mode_from_name(name: &str) -> Option<InputMode> {
    match name {
        "normal"           => Some(InputMode::Normal),
        "detail"           => Some(InputMode::Detail),
        "search"           => Some(InputMode::Search),
        "editing"          => Some(InputMode::Editing),
        "settings"         => Some(InputMode::Settings),
        "citation_preview" => Some(InputMode::CitationPreview),
        "dialog"           => Some(InputMode::Dialog),
        "command"          => Some(InputMode::Command),
        "name_disambig"    => Some(InputMode::NameDisambig),
        _ => None,
    }
}

/// Build a flat list of `(InputMode, KeyEvent, Action)` from the config's
/// keybindings map.  Invalid entries are silently skipped.
pub fn build_user_bindings(
    config: &indexmap::IndexMap<String, indexmap::IndexMap<String, String>>,
) -> Vec<(InputMode, KeyEvent, Action)> {
    let mut result = Vec::new();
    for (mode_name, bindings) in config {
        let Some(mode) = mode_from_name(mode_name) else { continue };
        for (key_spec, action_name) in bindings {
            // "None" is the intentional-unbind sentinel — skip (caller checks
            // user_bindings first, so an absent entry falls through to defaults).
            if action_name == "None" {
                continue;
            }
            let Some(action) = action_from_name(action_name) else { continue };
            let Some((code, mods)) = parse_key_spec(key_spec) else { continue };
            result.push((mode.clone(), KeyEvent::new(code, mods), action));
        }
    }
    result
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
        assert_eq!(map_key(key(KeyCode::Char('j')), &InputMode::Normal, None, None, false, false), Some(Action::MoveDown));
        assert_eq!(map_key(key(KeyCode::Down), &InputMode::Normal, None, None, false, false), Some(Action::MoveDown));
        assert_eq!(map_key(key(KeyCode::Char('k')), &InputMode::Normal, None, None, false, false), Some(Action::MoveUp));
        assert_eq!(map_key(key(KeyCode::Up), &InputMode::Normal, None, None, false, false), Some(Action::MoveUp));
        assert_eq!(map_key(key(KeyCode::Char('G')), &InputMode::Normal, None, None, false, false), Some(Action::MoveToBottom));
    }

    #[test]
    fn test_normal_gg() {
        // First 'g' returns None
        assert_eq!(map_key(key(KeyCode::Char('g')), &InputMode::Normal, None, None, false, false), None);
        // Second 'g' returns MoveToTop
        assert_eq!(map_key(key(KeyCode::Char('g')), &InputMode::Normal, None, Some('g'), false, false), Some(Action::MoveToTop));
    }

    #[test]
    fn test_normal_page_nav() {
        assert_eq!(map_key(ctrl('f'), &InputMode::Normal, None, None, false, false), Some(Action::PageDown));
        assert_eq!(map_key(ctrl('b'), &InputMode::Normal, None, None, false, false), Some(Action::PageUp));
    }

    #[test]
    fn test_normal_misc() {
        assert_eq!(map_key(key(KeyCode::Char('u')), &InputMode::Normal, None, None, false, false), Some(Action::Undo));
        assert_eq!(map_key(key(KeyCode::Char('/')), &InputMode::Normal, None, None, false, false), Some(Action::EnterSearch));
        assert_eq!(map_key(key(KeyCode::Enter), &InputMode::Normal, None, None, false, false), Some(Action::OpenDetail));
        assert_eq!(map_key(key(KeyCode::Char('a')), &InputMode::Normal, None, None, false, false), Some(Action::AddEntry));
        assert_eq!(map_key(key(KeyCode::Char('D')), &InputMode::Normal, None, None, false, false), Some(Action::DuplicateEntry));
        assert_eq!(map_key(key(KeyCode::Tab), &InputMode::Normal, None, None, false, false), Some(Action::ToggleGroups));
        assert_eq!(map_key(key(KeyCode::Char('B')), &InputMode::Normal, None, None, false, false), Some(Action::ToggleBraces));
        assert_eq!(map_key(key(KeyCode::Char('L')), &InputMode::Normal, None, None, false, false), Some(Action::ToggleLatex));
        assert_eq!(map_key(key(KeyCode::Char('S')), &InputMode::Normal, None, None, false, false), Some(Action::EnterSettings));
        assert_eq!(map_key(key(KeyCode::Char('?')), &InputMode::Normal, None, None, false, false), Some(Action::ShowHelp));
        assert_eq!(map_key(key(KeyCode::Char(':')), &InputMode::Normal, None, None, false, false), Some(Action::EnterCommand));
        assert_eq!(map_key(key(KeyCode::Char('I')), &InputMode::Normal, None, None, false, false), Some(Action::ImportEntry));
    }

    #[test]
    fn test_normal_dd() {
        assert_eq!(map_key(key(KeyCode::Char('d')), &InputMode::Normal, None, None, false, false), None);
        assert_eq!(map_key(key(KeyCode::Char('d')), &InputMode::Normal, None, Some('d'), false, false), Some(Action::DeleteEntry));
    }

    #[test]
    fn test_normal_yy() {
        assert_eq!(map_key(key(KeyCode::Char('y')), &InputMode::Normal, None, None, false, false), None);
        assert_eq!(map_key(key(KeyCode::Char('y')), &InputMode::Normal, None, Some('y'), false, false), Some(Action::YankCitekey));
    }

    #[test]
    fn test_normal_focus() {
        assert_eq!(map_key(key(KeyCode::Char('h')), &InputMode::Normal, None, None, false, false), Some(Action::FocusGroups));
        assert_eq!(map_key(key(KeyCode::Left), &InputMode::Normal, None, None, false, false), Some(Action::FocusGroups));
        assert_eq!(map_key(key(KeyCode::Char('l')), &InputMode::Normal, None, None, false, false), Some(Action::FocusList));
        assert_eq!(map_key(key(KeyCode::Right), &InputMode::Normal, None, None, false, false), Some(Action::FocusList));
    }

    // ── Search mode ──

    #[test]
    fn test_search_mode() {
        assert_eq!(map_key(key(KeyCode::Esc), &InputMode::Search, None, None, false, false), Some(Action::ExitSearch));
        assert_eq!(map_key(key(KeyCode::Enter), &InputMode::Search, None, None, false, false), Some(Action::ConfirmSearch));
        assert_eq!(map_key(key(KeyCode::Backspace), &InputMode::Search, None, None, false, false), Some(Action::SearchBackspace));
        assert_eq!(map_key(key(KeyCode::Char('x')), &InputMode::Search, None, None, false, false), Some(Action::SearchChar('x')));
    }

    // ── Detail mode ──

    #[test]
    fn test_detail_mode() {
        assert_eq!(map_key(key(KeyCode::Esc), &InputMode::Detail, None, None, false, false), Some(Action::CloseDetail));
        assert_eq!(map_key(key(KeyCode::Char('q')), &InputMode::Detail, None, None, false, false), None);
        assert_eq!(map_key(key(KeyCode::Char('e')), &InputMode::Detail, None, None, false, false), Some(Action::EditField));
        assert_eq!(map_key(key(KeyCode::Char('i')), &InputMode::Detail, None, None, false, false), Some(Action::EditField));
        assert_eq!(map_key(key(KeyCode::Enter), &InputMode::Detail, None, None, false, false), Some(Action::EditField));
        assert_eq!(map_key(key(KeyCode::Char('a')), &InputMode::Detail, None, None, false, false), Some(Action::NormalizeNames));
        assert_eq!(map_key(key(KeyCode::Char('A')), &InputMode::Detail, None, None, false, false), Some(Action::AddField));
        assert_eq!(map_key(key(KeyCode::Char('f')), &InputMode::Detail, None, None, false, false), Some(Action::AddFileAttachment));
        assert_eq!(map_key(key(KeyCode::Char('d')), &InputMode::Detail, None, None, false, false), Some(Action::DeleteField));
        assert_eq!(map_key(key(KeyCode::Char('T')), &InputMode::Detail, None, None, false, false), Some(Action::TitlecaseField));
        assert_eq!(map_key(key(KeyCode::Char('N')), &InputMode::Detail, None, None, false, false), Some(Action::DetailPrevMatch));
        assert_eq!(map_key(key(KeyCode::Char('n')), &InputMode::Detail, None, None, false, false), Some(Action::DetailNextMatch));
        assert_eq!(map_key(key(KeyCode::Char('/')), &InputMode::Detail, None, None, false, false), Some(Action::EnterDetailSearch));
        assert_eq!(map_key(key(KeyCode::Char('c')), &InputMode::Detail, None, None, false, false), Some(Action::RegenCitekey));
        assert_eq!(map_key(key(KeyCode::Char('t')), &InputMode::Detail, None, None, false, false), Some(Action::ChangeEntryType));
        assert_eq!(map_key(key(KeyCode::Char('u')), &InputMode::Detail, None, None, false, false), Some(Action::Undo));
        // Navigation
        assert_eq!(map_key(key(KeyCode::Char('G')), &InputMode::Detail, None, None, false, false), Some(Action::MoveToBottom));
        assert_eq!(map_key(key(KeyCode::Char('g')), &InputMode::Detail, None, Some('g'), false, false), Some(Action::MoveToTop));
        assert_eq!(map_key(key(KeyCode::Char('g')), &InputMode::Detail, None, None, false, false), None);
        assert_eq!(map_key(ctrl('f'), &InputMode::Detail, None, None, false, false), Some(Action::PageDown));
        assert_eq!(map_key(ctrl('b'), &InputMode::Detail, None, None, false, false), Some(Action::PageUp));
        // Group editing moved to Tab
        assert_eq!(map_key(key(KeyCode::Tab), &InputMode::Detail, None, None, false, false), Some(Action::EditGroups));
        assert_eq!(map_key(key(KeyCode::Char('g')), &InputMode::Detail, None, Some('x'), false, false), None);
    }

    // ── Editing mode ──

    #[test]
    fn test_editing_mode() {
        // In Insert mode (edit_normal=false), Esc enters Normal mode
        assert_eq!(map_key(key(KeyCode::Esc), &InputMode::Editing, None, None, false, false), Some(Action::EditEnterNormal));
        assert_eq!(map_key(key(KeyCode::Enter), &InputMode::Editing, None, None, false, false), Some(Action::ConfirmEdit));
        assert_eq!(map_key(key(KeyCode::Backspace), &InputMode::Editing, None, None, false, false), Some(Action::EditBackspace));
        assert_eq!(map_key(key(KeyCode::Left), &InputMode::Editing, None, None, false, false), Some(Action::EditCursorLeft));
        assert_eq!(map_key(key(KeyCode::Right), &InputMode::Editing, None, None, false, false), Some(Action::EditCursorRight));
        assert_eq!(map_key(key(KeyCode::Delete), &InputMode::Editing, None, None, false, false), Some(Action::EditDelete));
        assert_eq!(map_key(ctrl('a'), &InputMode::Editing, None, None, false, false), Some(Action::EditCursorHome));
        assert_eq!(map_key(ctrl('e'), &InputMode::Editing, None, None, false, false), Some(Action::EditCursorEnd));
        assert_eq!(map_key(key(KeyCode::Char('z')), &InputMode::Editing, None, None, false, false), Some(Action::EditChar('z')));
    }

    #[test]
    fn test_editing_mode_up_down() {
        // Up/Down arrow keys in editing mode map to EditCursorUp/Down (used by month navigation).
        assert_eq!(
            map_key(key(KeyCode::Up), &InputMode::Editing, None, None, false, false),
            Some(Action::EditCursorUp)
        );
        assert_eq!(
            map_key(key(KeyCode::Down), &InputMode::Editing, None, None, false, false),
            Some(Action::EditCursorDown)
        );
    }

    #[test]
    fn test_editing_mode_home_end() {
        // Home and End with CONTROL modifier map to cursor-home/end.
        assert_eq!(
            map_key(
                KeyEvent::new(KeyCode::Home, KeyModifiers::CONTROL),
                &InputMode::Editing, None, None, false, false
            ),
            Some(Action::EditCursorHome)
        );
        assert_eq!(
            map_key(
                KeyEvent::new(KeyCode::End, KeyModifiers::CONTROL),
                &InputMode::Editing, None, None, false, false
            ),
            Some(Action::EditCursorEnd)
        );
        // Without CONTROL modifier they fall through to None.
        assert_eq!(
            map_key(key(KeyCode::Home), &InputMode::Editing, None, None, false, false),
            None
        );
    }

    #[test]
    fn test_editing_mode_tab() {
        assert_eq!(
            map_key(key(KeyCode::Tab), &InputMode::Editing, None, None, false, false),
            Some(Action::EditTabComplete)
        );
    }

    #[test]
    fn test_editing_mode_backtab() {
        assert_eq!(
            map_key(
                KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT),
                &InputMode::Editing, None, None, false, false,
            ),
            Some(Action::EditTabCompleteReverse)
        );
    }

    #[test]
    fn test_command_mode_backtab() {
        assert_eq!(
            map_key(
                KeyEvent::new(KeyCode::BackTab, KeyModifiers::SHIFT),
                &InputMode::Command, None, None, false, false,
            ),
            Some(Action::CommandTabCompleteReverse)
        );
    }

    // ── Dialog mode ──

    #[test]
    fn test_dialog_mode() {
        // Non-message dialogs: 'y' confirms, 'n'/Esc cancels
        assert_eq!(map_key(key(KeyCode::Esc), &InputMode::Dialog, None, None, false, false), Some(Action::DialogCancel));
        assert_eq!(map_key(key(KeyCode::Char('n')), &InputMode::Dialog, None, None, false, false), Some(Action::DialogCancel));
        assert_eq!(map_key(key(KeyCode::Enter), &InputMode::Dialog, None, None, false, false), Some(Action::DialogConfirm));
        assert_eq!(map_key(key(KeyCode::Char('y')), &InputMode::Dialog, None, None, false, false), Some(Action::DialogConfirm));
        assert_eq!(map_key(key(KeyCode::Char('j')), &InputMode::Dialog, None, None, false, false), Some(Action::MoveDown));
        assert_eq!(map_key(key(KeyCode::Char('k')), &InputMode::Dialog, None, None, false, false), Some(Action::MoveUp));
        assert_eq!(map_key(key(KeyCode::Char(' ')), &InputMode::Dialog, None, None, false, false), Some(Action::DialogToggle));
        assert_eq!(map_key(ctrl('f'), &InputMode::Dialog, None, None, false, false), Some(Action::PageDown));
        assert_eq!(map_key(ctrl('b'), &InputMode::Dialog, None, None, false, false), Some(Action::PageUp));
        assert_eq!(map_key(key(KeyCode::Char('g')), &InputMode::Dialog, None, None, false, false), Some(Action::MoveToTop));
        assert_eq!(map_key(key(KeyCode::Char('G')), &InputMode::Dialog, None, None, false, false), Some(Action::MoveToBottom));
    }

    #[test]
    fn test_message_dialog_yy() {
        // In a Message dialog: first 'y' does nothing (pending chord)
        assert_eq!(map_key(key(KeyCode::Char('y')), &InputMode::Dialog, None, None, true, false), None);
        // Second 'y' (yy) yanks the message
        assert_eq!(map_key(key(KeyCode::Char('y')), &InputMode::Dialog, None, Some('y'), true, false), Some(Action::DialogYank));
        // Enter still dismisses
        assert_eq!(map_key(key(KeyCode::Enter), &InputMode::Dialog, None, None, true, false), Some(Action::DialogConfirm));
        // Esc still cancels
        assert_eq!(map_key(key(KeyCode::Esc), &InputMode::Dialog, None, None, true, false), Some(Action::DialogCancel));
    }

    // ── Command mode ──

    #[test]
    fn test_command_mode() {
        assert_eq!(map_key(key(KeyCode::Esc), &InputMode::Command, None, None, false, false), Some(Action::ExitCommand));
        assert_eq!(map_key(key(KeyCode::Enter), &InputMode::Command, None, None, false, false), Some(Action::ExecuteCommand));
        assert_eq!(map_key(key(KeyCode::Backspace), &InputMode::Command, None, None, false, false), Some(Action::CommandBackspace));
        assert_eq!(map_key(key(KeyCode::Char('w')), &InputMode::Command, None, None, false, false), Some(Action::CommandChar('w')));
    }

    // ── Settings mode ──

    #[test]
    fn test_settings_mode() {
        assert_eq!(map_key(key(KeyCode::Esc), &InputMode::Settings, None, None, false, false), Some(Action::ExitSettings));
        assert_eq!(map_key(key(KeyCode::Char('q')), &InputMode::Settings, None, None, false, false), None);
        assert_eq!(map_key(key(KeyCode::Char('j')), &InputMode::Settings, None, None, false, false), Some(Action::SettingsMoveDown));
        assert_eq!(map_key(key(KeyCode::Down), &InputMode::Settings, None, None, false, false), Some(Action::SettingsMoveDown));
        assert_eq!(map_key(key(KeyCode::Char('k')), &InputMode::Settings, None, None, false, false), Some(Action::SettingsMoveUp));
        assert_eq!(map_key(key(KeyCode::Char('g')), &InputMode::Settings, None, None, false, false), Some(Action::SettingsMoveToTop));
        assert_eq!(map_key(key(KeyCode::Char('G')), &InputMode::Settings, None, None, false, false), Some(Action::SettingsMoveToBottom));
        assert_eq!(map_key(ctrl('f'), &InputMode::Settings, None, None, false, false), Some(Action::SettingsPageDown));
        assert_eq!(map_key(ctrl('b'), &InputMode::Settings, None, None, false, false), Some(Action::SettingsPageUp));
        assert_eq!(map_key(key(KeyCode::Enter), &InputMode::Settings, None, None, false, false), Some(Action::SettingsToggle));
        assert_eq!(map_key(key(KeyCode::Char(' ')), &InputMode::Settings, None, None, false, false), Some(Action::SettingsToggle));
        assert_eq!(map_key(key(KeyCode::Char('e')), &InputMode::Settings, None, None, false, false), Some(Action::SettingsEdit));
        assert_eq!(map_key(key(KeyCode::Char('E')), &InputMode::Settings, None, None, false, false), Some(Action::SettingsExport));
        assert_eq!(map_key(key(KeyCode::Char('I')), &InputMode::Settings, None, None, false, false), Some(Action::SettingsImport));
    }

    #[test]
    fn test_normal_mode_extra_keys() {
        assert_eq!(map_key(key(KeyCode::Char(' ')), &InputMode::Normal, None, None, false, false), Some(Action::ShowCitationPreview));
        assert_eq!(map_key(key(KeyCode::Char('o')), &InputMode::Normal, None, None, false, false), Some(Action::OpenFile));
        assert_eq!(map_key(key(KeyCode::Char('w')), &InputMode::Normal, None, None, false, false), Some(Action::OpenWeb));
        assert_eq!(map_key(key(KeyCode::Char('v')), &InputMode::Normal, None, None, false, false), Some(Action::Validate));
        // Unknown key → None
        assert_eq!(map_key(key(KeyCode::F(1)), &InputMode::Normal, None, None, false, false), None);
    }

    #[test]
    fn test_search_mode_unknown_key() {
        assert_eq!(map_key(key(KeyCode::F(1)), &InputMode::Search, None, None, false, false), None);
    }

    #[test]
    fn test_detail_mode_extra_keys() {
        assert_eq!(map_key(key(KeyCode::Char('o')), &InputMode::Detail, None, None, false, false), Some(Action::OpenFile));
        assert_eq!(map_key(key(KeyCode::Char('w')), &InputMode::Detail, None, None, false, false), Some(Action::OpenWeb));
        assert_eq!(map_key(key(KeyCode::Char('L')), &InputMode::Detail, None, None, false, false), Some(Action::ToggleLatex));
        assert_eq!(map_key(key(KeyCode::Char('B')), &InputMode::Detail, None, None, false, false), Some(Action::ToggleBraces));
        assert_eq!(map_key(key(KeyCode::Char('F')), &InputMode::Detail, None, None, false, false), Some(Action::SyncEntryFilename));
    }

    // ── DetailSearch mode ──

    #[test]
    fn test_detail_search_mode() {
        assert_eq!(map_key(key(KeyCode::Esc),       &InputMode::DetailSearch, None, None, false, false), Some(Action::ExitDetailSearch));
        assert_eq!(map_key(key(KeyCode::Enter),     &InputMode::DetailSearch, None, None, false, false), Some(Action::ExitDetailSearch));
        assert_eq!(map_key(key(KeyCode::Backspace), &InputMode::DetailSearch, None, None, false, false), Some(Action::DetailSearchBackspace));
        assert_eq!(map_key(key(KeyCode::Char('a')), &InputMode::DetailSearch, None, None, false, false), Some(Action::DetailSearchChar('a')));
        assert_eq!(map_key(key(KeyCode::F(1)),      &InputMode::DetailSearch, None, None, false, false), None);
    }

    // ── Help mode ──

    #[test]
    fn test_help_mode() {
        // Any key in Help mode closes help
        assert_eq!(map_key(key(KeyCode::Esc),       &InputMode::Help, None, None, false, false), Some(Action::CloseHelp));
        assert_eq!(map_key(key(KeyCode::Char('q')), &InputMode::Help, None, None, false, false), Some(Action::CloseHelp));
    }

    // ── Dialog mode (catch-all) ──

    #[test]
    fn test_dialog_unknown_key() {
        assert_eq!(map_key(key(KeyCode::F(1)), &InputMode::Dialog, None, None, false, false), None);
    }

    // ── Command mode ──

    #[test]
    fn test_command_mode_extra_keys() {
        assert_eq!(map_key(key(KeyCode::Tab),   &InputMode::Command, None, None, false, false), Some(Action::CommandTabComplete));
        assert_eq!(map_key(key(KeyCode::F(1)),  &InputMode::Command, None, None, false, false), None);
    }

    // ── Settings mode (field group operations) ──

    #[test]
    fn test_settings_mode_field_group_ops() {
        assert_eq!(map_key(key(KeyCode::Char('a')), &InputMode::Settings, None, None, false, false), Some(Action::SettingsAddFieldGroup));
        assert_eq!(map_key(key(KeyCode::Char('x')), &InputMode::Settings, None, None, false, false), Some(Action::SettingsDeleteFieldGroup));
        assert_eq!(map_key(key(KeyCode::Char('r')), &InputMode::Settings, None, None, false, false), Some(Action::SettingsRenameFieldGroup));
    }

    // ── ValidateResults mode ──

    #[test]
    fn test_validate_results_mode() {
        assert_eq!(map_key(key(KeyCode::Char('j')), &InputMode::ValidateResults, None, None, false, false), Some(Action::MoveDown));
        assert_eq!(map_key(key(KeyCode::Down),      &InputMode::ValidateResults, None, None, false, false), Some(Action::MoveDown));
        assert_eq!(map_key(key(KeyCode::Char('k')), &InputMode::ValidateResults, None, None, false, false), Some(Action::MoveUp));
        assert_eq!(map_key(key(KeyCode::Up),        &InputMode::ValidateResults, None, None, false, false), Some(Action::MoveUp));
        assert_eq!(map_key(key(KeyCode::Esc),       &InputMode::ValidateResults, None, None, false, false), Some(Action::CloseValidateResults));
        assert_eq!(map_key(key(KeyCode::Char('q')), &InputMode::ValidateResults, None, None, false, false), Some(Action::CloseValidateResults));
        assert_eq!(map_key(key(KeyCode::F(1)),      &InputMode::ValidateResults, None, None, false, false), None);
    }

    // ── NameDisambig mode ──

    #[test]
    fn test_name_disambig_mode() {
        assert_eq!(map_key(key(KeyCode::Char('j')), &InputMode::NameDisambig, None, None, false, false), Some(Action::MoveDown));
        assert_eq!(map_key(key(KeyCode::Char('k')), &InputMode::NameDisambig, None, None, false, false), Some(Action::MoveUp));
        assert_eq!(map_key(key(KeyCode::Tab),       &InputMode::NameDisambig, None, None, false, false), Some(Action::DisambigCycleVariant));
        assert_eq!(map_key(key(KeyCode::BackTab),   &InputMode::NameDisambig, None, None, false, false), Some(Action::DisambigCycleVariantReverse));
        assert_eq!(map_key(key(KeyCode::Enter),     &InputMode::NameDisambig, None, None, false, false), Some(Action::ApplyNameDisambig));
        assert_eq!(map_key(key(KeyCode::Esc),       &InputMode::NameDisambig, None, None, false, false), Some(Action::CloseNameDisambig));
        assert_eq!(map_key(key(KeyCode::Char('q')), &InputMode::NameDisambig, None, None, false, false), Some(Action::CloseNameDisambig));
        assert_eq!(map_key(key(KeyCode::Char('G')), &InputMode::NameDisambig, None, None, false, false), Some(Action::MoveToBottom));
        assert_eq!(map_key(key(KeyCode::Char('x')), &InputMode::NameDisambig, None, None, false, false), Some(Action::DisambigRemoveVariant));
    }

    // ── CitationPreview mode ──

    #[test]
    fn test_citation_preview_mode() {
        assert_eq!(
            map_key(key(KeyCode::Esc), &InputMode::CitationPreview, None, None, false, false),
            Some(Action::CloseCitationPreview)
        );
        assert_eq!(
            map_key(key(KeyCode::Char('q')), &InputMode::CitationPreview, None, None, false, false),
            Some(Action::CloseCitationPreview)
        );
        assert_eq!(
            map_key(key(KeyCode::Char('j')), &InputMode::CitationPreview, None, None, false, false),
            Some(Action::MoveDown)
        );
        assert_eq!(
            map_key(key(KeyCode::Char('k')), &InputMode::CitationPreview, None, None, false, false),
            Some(Action::MoveUp)
        );
    }

    // ── Editing normal mode ──

    #[test]
    fn test_editing_normal_mode_keys() {
        // Esc in normal mode cancels the edit
        assert_eq!(map_key(key(KeyCode::Esc), &InputMode::Editing, None, None, false, true), Some(Action::CancelEdit));
        assert_eq!(map_key(key(KeyCode::Enter), &InputMode::Editing, None, None, false, true), Some(Action::ConfirmEdit));
        // Mode transitions
        assert_eq!(map_key(key(KeyCode::Char('i')), &InputMode::Editing, None, None, false, true), Some(Action::EditEnterInsert));
        assert_eq!(map_key(key(KeyCode::Char('a')), &InputMode::Editing, None, None, false, true), Some(Action::EditEnterInsertAfter));
        assert_eq!(map_key(key(KeyCode::Char('A')), &InputMode::Editing, None, None, false, true), Some(Action::EditEnterInsertAtEnd));
        assert_eq!(map_key(key(KeyCode::Char('I')), &InputMode::Editing, None, None, false, true), Some(Action::EditEnterInsertAtHome));
        // Navigation
        assert_eq!(map_key(key(KeyCode::Char('h')), &InputMode::Editing, None, None, false, true), Some(Action::EditCursorLeft));
        assert_eq!(map_key(key(KeyCode::Left), &InputMode::Editing, None, None, false, true), Some(Action::EditCursorLeft));
        assert_eq!(map_key(key(KeyCode::Char('l')), &InputMode::Editing, None, None, false, true), Some(Action::EditCursorRight));
        assert_eq!(map_key(key(KeyCode::Right), &InputMode::Editing, None, None, false, true), Some(Action::EditCursorRight));
        assert_eq!(map_key(key(KeyCode::Char('0')), &InputMode::Editing, None, None, false, true), Some(Action::EditCursorHome));
        assert_eq!(map_key(key(KeyCode::Home), &InputMode::Editing, None, None, false, true), Some(Action::EditCursorHome));
        assert_eq!(map_key(key(KeyCode::Char('$')), &InputMode::Editing, None, None, false, true), Some(Action::EditCursorEnd));
        assert_eq!(map_key(key(KeyCode::End), &InputMode::Editing, None, None, false, true), Some(Action::EditCursorEnd));
        // Delete
        assert_eq!(map_key(key(KeyCode::Char('x')), &InputMode::Editing, None, None, false, true), Some(Action::EditDelete));
        // Word motions
        assert_eq!(map_key(key(KeyCode::Char('w')), &InputMode::Editing, None, None, false, true), Some(Action::EditMoveWordFwd));
        assert_eq!(map_key(key(KeyCode::Char('b')), &InputMode::Editing, None, None, false, true), Some(Action::EditMoveWordBwd));
        assert_eq!(map_key(key(KeyCode::Char('e')), &InputMode::Editing, None, None, false, true), Some(Action::EditMoveWordEnd));
        assert_eq!(map_key(key(KeyCode::Char('W')), &InputMode::Editing, None, None, false, true), Some(Action::EditMoveBigWordFwd));
        assert_eq!(map_key(key(KeyCode::Char('B')), &InputMode::Editing, None, None, false, true), Some(Action::EditMoveBigWordBwd));
        assert_eq!(map_key(key(KeyCode::Char('E')), &InputMode::Editing, None, None, false, true), Some(Action::EditMoveBigWordEnd));
        // Undo / put / yank
        assert_eq!(map_key(key(KeyCode::Char('u')), &InputMode::Editing, None, None, false, true), Some(Action::EditUndo));
        assert_eq!(map_key(key(KeyCode::Char('p')), &InputMode::Editing, None, None, false, true), Some(Action::EditPut));
        // First 'y' is pending (no last_key)
        assert_eq!(map_key(key(KeyCode::Char('y')), &InputMode::Editing, None, None, false, true), None);
        // Second 'y' yanks
        assert_eq!(map_key(key(KeyCode::Char('y')), &InputMode::Editing, None, Some('y'), false, true), Some(Action::EditYank));
        // j/k navigate fields
        assert_eq!(map_key(key(KeyCode::Char('j')), &InputMode::Editing, None, None, false, true), Some(Action::EditConfirmAndMoveDown));
        assert_eq!(map_key(key(KeyCode::Char('k')), &InputMode::Editing, None, None, false, true), Some(Action::EditConfirmAndMoveUp));
        assert_eq!(map_key(key(KeyCode::Down), &InputMode::Editing, None, None, false, true), Some(Action::EditConfirmAndMoveDown));
        assert_eq!(map_key(key(KeyCode::Up), &InputMode::Editing, None, None, false, true), Some(Action::EditConfirmAndMoveUp));
        // New operators
        assert_eq!(map_key(key(KeyCode::Char('D')), &InputMode::Editing, None, None, false, true), Some(Action::EditDeleteToEnd));
        assert_eq!(map_key(key(KeyCode::Char('C')), &InputMode::Editing, None, None, false, true), Some(Action::EditChangeToEnd));
        assert_eq!(map_key(key(KeyCode::Char('s')), &InputMode::Editing, None, None, false, true), Some(Action::EditSubstituteChar));
        assert_eq!(map_key(key(KeyCode::Char('S')), &InputMode::Editing, None, None, false, true), Some(Action::EditSubstituteLine));
        assert_eq!(map_key(key(KeyCode::Char('~')), &InputMode::Editing, None, None, false, true), Some(Action::EditToggleCase));
        assert_eq!(map_key(key(KeyCode::Char('X')), &InputMode::Editing, None, None, false, true), Some(Action::EditDeleteCharBack));
        // Pending keys return None alone
        assert_eq!(map_key(key(KeyCode::Char('r')), &InputMode::Editing, None, None, false, true), None);
        assert_eq!(map_key(key(KeyCode::Char('f')), &InputMode::Editing, None, None, false, true), None);
        assert_eq!(map_key(key(KeyCode::Char('F')), &InputMode::Editing, None, None, false, true), None);
        // Pending keys dispatch on next char
        assert_eq!(map_key(key(KeyCode::Char('x')), &InputMode::Editing, None, Some('r'), false, true), Some(Action::EditReplaceChar('x')));
        assert_eq!(map_key(key(KeyCode::Char('.')), &InputMode::Editing, None, Some('f'), false, true), Some(Action::EditFindCharFwd('.')));
        assert_eq!(map_key(key(KeyCode::Char('.')), &InputMode::Editing, None, Some('F'), false, true), Some(Action::EditFindCharBwd('.')));
        // t/T pending keys return None alone, dispatch on next char
        assert_eq!(map_key(key(KeyCode::Char('t')), &InputMode::Editing, None, None, false, true), None);
        assert_eq!(map_key(key(KeyCode::Char('T')), &InputMode::Editing, None, None, false, true), None);
        assert_eq!(map_key(key(KeyCode::Char('.')), &InputMode::Editing, None, Some('t'), false, true), Some(Action::EditFindToCharFwd('.')));
        assert_eq!(map_key(key(KeyCode::Char('.')), &InputMode::Editing, None, Some('T'), false, true), Some(Action::EditFindToCharBwd('.')));
        // dw still works
        assert_eq!(map_key(key(KeyCode::Char('w')), &InputMode::Editing, None, Some('d'), false, true), Some(Action::EditDeleteWordFwd));
        // dt/dT/df/dF 3-key sequences
        assert_eq!(map_key(key(KeyCode::Char('.')), &InputMode::Editing, Some('d'), Some('t'), false, true), Some(Action::EditDeleteToChar('.')));
        assert_eq!(map_key(key(KeyCode::Char('.')), &InputMode::Editing, Some('d'), Some('T'), false, true), Some(Action::EditDeleteToCharBack('.')));
        assert_eq!(map_key(key(KeyCode::Char('.')), &InputMode::Editing, Some('d'), Some('f'), false, true), Some(Action::EditDeleteThroughChar('.')));
        assert_eq!(map_key(key(KeyCode::Char('.')), &InputMode::Editing, Some('d'), Some('F'), false, true), Some(Action::EditDeleteThroughCharBack('.')));
        // Unknown key → None
        assert_eq!(map_key(key(KeyCode::Char('z')), &InputMode::Editing, None, None, false, true), None);
    }

    #[test]
    fn test_editing_insert_ctrl_keys() {
        use crossterm::event::KeyModifiers;
        let ctrl_w = KeyEvent::new(KeyCode::Char('w'), KeyModifiers::CONTROL);
        let ctrl_u = KeyEvent::new(KeyCode::Char('u'), KeyModifiers::CONTROL);
        assert_eq!(map_key(ctrl_w, &InputMode::Editing, None, None, false, false), Some(Action::EditDeleteWordBack));
        assert_eq!(map_key(ctrl_u, &InputMode::Editing, None, None, false, false), Some(Action::EditDeleteToHome));
    }

    #[test]
    fn test_citation_preview_yy() {
        // First 'y' returns None (waiting for second)
        assert_eq!(
            map_key(key(KeyCode::Char('y')), &InputMode::CitationPreview, None, None, false, false),
            None
        );
        // Second 'y' returns YankCitationPreview
        assert_eq!(
            map_key(key(KeyCode::Char('y')), &InputMode::CitationPreview, None, Some('y'), false, false),
            Some(Action::YankCitationPreview)
        );
    }

    #[test]
    fn test_parse_key_spec_chars() {
        assert!(parse_key_spec("j").is_some());
        assert!(parse_key_spec("enter").is_some());
        assert!(parse_key_spec("ctrl-f").is_some());
        assert!(parse_key_spec("esc").is_some());
        assert!(parse_key_spec("f1").is_some());
        assert!(parse_key_spec("pageup").is_some());
        assert!(parse_key_spec("not-a-valid-key---xyz").is_none());
    }

    #[test]
    fn test_parse_key_spec_all_named_keys() {
        let named = [
            "backspace", "delete", "tab", "backtab", "space",
            "home", "end", "pagedown", "up", "down", "left", "right",
            "f2", "f3", "f4", "f5", "f6", "f7", "f8", "f9", "f10", "f11", "f12",
        ];
        for name in named {
            assert!(parse_key_spec(name).is_some(), "parse_key_spec({name:?}) returned None");
        }
    }

    #[test]
    fn test_parse_key_spec_modifier_prefixes() {
        let (code, mods) = parse_key_spec("ctrl-j").unwrap();
        assert_eq!(code, KeyCode::Char('j'));
        assert!(mods.contains(KeyModifiers::CONTROL));

        let (_, mods) = parse_key_spec("shift-f1").unwrap();
        assert!(mods.contains(KeyModifiers::SHIFT));

        let (_, mods) = parse_key_spec("alt-x").unwrap();
        assert!(mods.contains(KeyModifiers::ALT));
    }

    #[test]
    fn test_parse_key_spec_multi_char_returns_none() {
        assert!(parse_key_spec("ab").is_none());
    }

    #[test]
    fn test_parse_key_spec_empty_after_prefix_returns_none() {
        assert!(parse_key_spec("ctrl-").is_none());
        assert!(parse_key_spec("shift-").is_none());
    }

    #[test]
    fn test_action_from_name_roundtrip() {
        assert!(action_from_name("MoveDown").is_some());
        assert!(action_from_name("AddEntry").is_some());
        assert!(action_from_name("Undo").is_some());
        assert!(action_from_name("SyncEntryFilename").is_some());
        assert!(action_from_name("DoesNotExist").is_none());
        assert!(action_from_name("None").is_none()); // intentional unbind
    }

    #[test]
    fn test_action_from_name_extended_coverage() {
        let expected_some = [
            "MoveToTop", "MoveToBottom", "PageDown", "PageUp",
            "EnterSearch", "ExitSearch", "ConfirmSearch", "SearchBackspace",
            "OpenDetail", "CloseDetail", "EnterDetailSearch", "ExitDetailSearch",
            "DetailSearchBackspace", "DetailNextMatch", "DetailPrevMatch",
            "EditField", "AddField", "AddFileAttachment", "DeleteField",
            "EditGroups", "RegenCitekey", "RegenAllCitekeys", "SyncFilenames",
            "ConfirmEdit", "CancelEdit", "EditBackspace", "EditDelete",
            "EditCursorLeft", "EditCursorRight", "EditCursorUp", "EditCursorDown",
            "EditCursorHome", "EditCursorEnd", "EditTabComplete", "EditTabCompleteReverse",
            "DeleteEntry", "DuplicateEntry", "YankCitekey",
            "ToggleGroups", "FocusGroups", "FocusList", "ShowCitationPreview",
            "EnterCommand", "ExitCommand", "ExecuteCommand",
            "CommandBackspace", "CommandTabComplete", "CommandTabCompleteReverse",
            "DialogConfirm", "DialogCancel", "DialogToggle", "DialogYank",
            "ShowHelp", "TitlecaseField", "ChangeEntryType",
            "ToggleBraces", "ToggleLatex", "NormalizeNames",
            "OpenFile", "OpenWeb", "CloseCitationPreview", "YankCitationPreview",
            "EnterSettings", "ExitSettings",
            "SettingsMoveDown", "SettingsMoveUp",
            "SettingsToggle", "SettingsEdit", "SettingsExport", "SettingsImport",
            "SettingsAddFieldGroup", "SettingsDeleteFieldGroup", "SettingsRenameFieldGroup",
            "SettingsMoveToTop", "SettingsMoveToBottom",
            "SettingsPageDown", "SettingsPageUp",
            "Validate", "CloseValidateResults",
            "DisambiguateNames", "CloseNameDisambig", "ApplyNameDisambig",
            "DisambigCycleVariant", "DisambigCycleVariantReverse",
            "DisambigRemoveVariant", "DisambigPreview",
            "ImportEntry", "ExportJson", "ExportRis", "CloseHelp",
            "EditUndo", "EditPut", "EditYank",
            "EditEnterNormal", "EditEnterInsert", "EditEnterInsertAfter",
            "EditEnterInsertAtEnd", "EditEnterInsertAtHome",
            "EditMoveWordFwd", "EditMoveWordBwd", "EditMoveWordEnd",
            "EditMoveBigWordFwd", "EditMoveBigWordBwd", "EditMoveBigWordEnd",
            "EditDeleteWordFwd", "EditDeleteToEnd", "EditChangeToEnd",
            "EditSubstituteChar", "EditSubstituteLine", "EditToggleCase",
            "EditDeleteCharBack", "EditDeleteWordBack", "EditDeleteToHome",
            "EditConfirmAndMoveDown", "EditConfirmAndMoveUp",
        ];
        for name in expected_some {
            assert!(action_from_name(name).is_some(), "action_from_name({name:?}) returned None");
        }
    }

    #[test]
    fn test_build_user_bindings_all_modes() {
        use indexmap::IndexMap;
        let modes = [
            "normal", "detail", "search", "editing",
            "settings", "citation_preview", "dialog", "command", "name_disambig",
        ];
        for mode_name in modes {
            let mut mode_map: IndexMap<String, IndexMap<String, String>> = IndexMap::new();
            let mut bindings: IndexMap<String, String> = IndexMap::new();
            bindings.insert("j".to_string(), "MoveDown".to_string());
            mode_map.insert(mode_name.to_string(), bindings);
            let result = build_user_bindings(&mode_map);
            assert!(!result.is_empty(), "build_user_bindings produced no bindings for mode {mode_name:?}");
        }
    }

    #[test]
    fn test_build_user_bindings_skips_none_sentinel() {
        use indexmap::IndexMap;
        let mut mode_map: IndexMap<String, IndexMap<String, String>> = IndexMap::new();
        let mut bindings: IndexMap<String, String> = IndexMap::new();
        bindings.insert("j".to_string(), "None".to_string()); // intentional unbind
        mode_map.insert("normal".to_string(), bindings);
        let result = build_user_bindings(&mode_map);
        assert!(result.is_empty(), "None sentinel should be skipped");
    }

    #[test]
    fn test_build_user_bindings_skips_unknown_mode() {
        use indexmap::IndexMap;
        let mut mode_map: IndexMap<String, IndexMap<String, String>> = IndexMap::new();
        let mut bindings: IndexMap<String, String> = IndexMap::new();
        bindings.insert("j".to_string(), "MoveDown".to_string());
        mode_map.insert("not_a_real_mode".to_string(), bindings);
        let result = build_user_bindings(&mode_map);
        assert!(result.is_empty(), "unknown mode should be skipped");
    }

    #[test]
    fn test_build_user_bindings() {
        use indexmap::IndexMap;
        let mut mode_map: IndexMap<String, IndexMap<String, String>> = IndexMap::new();
        let mut normal: IndexMap<String, String> = IndexMap::new();
        normal.insert("ctrl-n".to_string(), "AddEntry".to_string());
        normal.insert("o".to_string(), "OpenDetail".to_string());
        mode_map.insert("normal".to_string(), normal);
        let bindings = build_user_bindings(&mode_map);
        assert!(!bindings.is_empty());
        let ctrl_n = crossterm::event::KeyEvent::new(
            KeyCode::Char('n'),
            crossterm::event::KeyModifiers::CONTROL,
        );
        let hit = bindings.iter()
            .find(|(m, k, _)| *m == InputMode::Normal && *k == ctrl_n);
        assert!(hit.is_some());
    }
}
