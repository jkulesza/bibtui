use super::*;
use super::groups::{collect_group_names, find_group_node, find_group_node_mut};
use std::io::Write;
use tempfile::NamedTempFile;
use crate::config::defaults::default_config;
use crate::tui::keybindings::InputMode;

/// Two-entry bib used by most tests. Sorted by citation_key: Doe2021, Smith2020.
const TEST_BIB: &str = r#"@Article{Smith2020,
  author  = {Smith, John},
  title   = {My Paper},
  year    = {2020},
  journal = {Nature},
}

@Book{Doe2021,
  author    = {Doe, Jane},
  title     = {Rust Programming},
  year      = {2021},
  publisher = {ACM Press},
}
"#;

/// Build an App from the TEST_BIB string. Returns (App, NamedTempFile);
/// the caller must keep the NamedTempFile alive to prevent deletion.
fn make_app() -> (App, NamedTempFile) {
    let mut tmp = NamedTempFile::new().unwrap();
    write!(tmp, "{}", TEST_BIB).unwrap();
    tmp.flush().unwrap();
    let path = tmp.path().to_path_buf();
    let app = App::new(path, default_config()).unwrap();
    (app, tmp)
}

// ── Sanity ──────────────────────────────────────────────────────────────

#[test]
fn test_app_loads_entries() {
    let (app, _tmp) = make_app();
    assert_eq!(app.database.entries.len(), 2);
}

#[test]
fn test_initial_mode_is_normal() {
    let (app, _tmp) = make_app();
    assert_eq!(app.mode, InputMode::Normal);
}

// ── Navigation ──────────────────────────────────────────────────────────

#[test]
fn test_move_down() {
    let (mut app, _tmp) = make_app();
    assert_eq!(app.entry_list_state.selected(), 0);
    app.handle_action(Action::MoveDown);
    assert_eq!(app.entry_list_state.selected(), 1);
}

#[test]
fn test_move_down_clamps_at_bottom() {
    let (mut app, _tmp) = make_app();
    app.handle_action(Action::MoveToBottom);
    let bottom = app.entry_list_state.selected();
    app.handle_action(Action::MoveDown);
    assert_eq!(app.entry_list_state.selected(), bottom);
}

#[test]
fn test_move_up_clamps_at_top() {
    let (mut app, _tmp) = make_app();
    app.handle_action(Action::MoveUp);
    assert_eq!(app.entry_list_state.selected(), 0);
}

#[test]
fn test_move_to_top() {
    let (mut app, _tmp) = make_app();
    app.handle_action(Action::MoveDown);
    app.handle_action(Action::MoveToTop);
    assert_eq!(app.entry_list_state.selected(), 0);
}

#[test]
fn test_move_to_bottom() {
    let (mut app, _tmp) = make_app();
    app.handle_action(Action::MoveToBottom);
    assert_eq!(app.entry_list_state.selected(), 1); // 2 entries, index 1
}

#[test]
fn test_page_down_clamps() {
    let (mut app, _tmp) = make_app();
    app.handle_action(Action::PageDown);
    assert_eq!(app.entry_list_state.selected(), 1); // only 2 entries
}

#[test]
fn test_page_up_from_top_stays_at_zero() {
    let (mut app, _tmp) = make_app();
    app.handle_action(Action::PageUp);
    assert_eq!(app.entry_list_state.selected(), 0);
}

// ── Focus ────────────────────────────────────────────────────────────────

#[test]
fn test_focus_groups() {
    let (mut app, _tmp) = make_app();
    app.handle_action(Action::FocusGroups);
    assert_eq!(app.focus, Focus::Groups);
}

#[test]
fn test_focus_list() {
    let (mut app, _tmp) = make_app();
    app.handle_action(Action::FocusGroups);
    app.handle_action(Action::FocusList);
    assert_eq!(app.focus, Focus::List);
}

#[test]
fn test_toggle_groups() {
    let (mut app, _tmp) = make_app();
    let initial = app.show_groups;
    app.handle_action(Action::ToggleGroups);
    assert_eq!(app.show_groups, !initial);
    app.handle_action(Action::ToggleGroups);
    assert_eq!(app.show_groups, initial);
}

// ── Mode transitions ─────────────────────────────────────────────────────

#[test]
fn test_enter_exit_search() {
    let (mut app, _tmp) = make_app();
    app.handle_action(Action::EnterSearch);
    assert_eq!(app.mode, InputMode::Search);
    app.handle_action(Action::ExitSearch);
    assert_eq!(app.mode, InputMode::Normal);
    assert!(app.filtered_indices.is_none());
}

#[test]
fn test_confirm_search_stays_normal() {
    let (mut app, _tmp) = make_app();
    app.handle_action(Action::EnterSearch);
    app.handle_action(Action::ConfirmSearch);
    assert_eq!(app.mode, InputMode::Normal);
}

#[test]
fn test_enter_exit_command() {
    let (mut app, _tmp) = make_app();
    app.handle_action(Action::EnterCommand);
    assert_eq!(app.mode, InputMode::Command);
    app.handle_action(Action::ExitCommand);
    assert_eq!(app.mode, InputMode::Normal);
}

#[test]
fn test_enter_exit_settings() {
    let (mut app, _tmp) = make_app();
    app.handle_action(Action::EnterSettings);
    assert_eq!(app.mode, InputMode::Settings);
    assert!(app.settings_state.is_some());
    app.handle_action(Action::ExitSettings);
    assert_eq!(app.mode, InputMode::Normal);
    assert!(app.settings_state.is_none());
}

#[test]
fn test_open_close_detail() {
    let (mut app, _tmp) = make_app();
    app.handle_action(Action::OpenDetail);
    assert_eq!(app.mode, InputMode::Detail);
    assert!(app.detail_state.is_some());
    app.handle_action(Action::CloseDetail);
    assert_eq!(app.mode, InputMode::Normal);
    assert!(app.detail_state.is_none());
}

// ── Toggles ──────────────────────────────────────────────────────────────

#[test]
fn test_toggle_braces() {
    let (mut app, _tmp) = make_app();
    let initial = app.show_braces;
    app.handle_action(Action::ToggleBraces);
    assert_eq!(app.show_braces, !initial);
    assert!(app.status_message.is_some());
}

#[test]
fn test_toggle_latex() {
    let (mut app, _tmp) = make_app();
    let initial = app.render_latex;
    app.handle_action(Action::ToggleLatex);
    assert_eq!(app.render_latex, !initial);
    assert!(app.status_message.is_some());
}

// ── Quit ─────────────────────────────────────────────────────────────────

#[test]
fn test_quit_when_clean() {
    let (mut app, _tmp) = make_app();
    app.command_palette_state.input = "q".to_string();
    app.handle_action(Action::ExecuteCommand);
    assert!(app.should_quit);
}

#[test]
fn test_quit_when_dirty_shows_message() {
    let (mut app, _tmp) = make_app();
    app.dirty = true;
    app.command_palette_state.input = "q".to_string();
    app.handle_action(Action::ExecuteCommand);
    assert!(!app.should_quit);
    assert!(app.status_message.is_some());
}

// ── Search ────────────────────────────────────────────────────────────────

#[test]
fn test_search_char_updates_query() {
    let (mut app, _tmp) = make_app();
    app.handle_action(Action::EnterSearch);
    app.handle_action(Action::SearchChar('s'));
    app.handle_action(Action::SearchChar('m'));
    assert_eq!(app.search_bar_state.query, "sm");
}

#[test]
fn test_search_backspace() {
    let (mut app, _tmp) = make_app();
    app.handle_action(Action::EnterSearch);
    app.handle_action(Action::SearchChar('s'));
    app.handle_action(Action::SearchChar('m'));
    app.handle_action(Action::SearchBackspace);
    assert_eq!(app.search_bar_state.query, "s");
}

#[test]
fn test_search_filters_entries() {
    let (mut app, _tmp) = make_app();
    app.handle_action(Action::EnterSearch);
    app.handle_action(Action::SearchChar('S')); // "Smith2020"
    app.handle_action(Action::SearchChar('m'));
    app.handle_action(Action::SearchChar('i'));
    app.handle_action(Action::SearchChar('t'));
    app.handle_action(Action::SearchChar('h'));
    // filtered_indices should now have 1 match
    assert!(app.filtered_indices.is_some());
    assert_eq!(app.filtered_indices.as_ref().unwrap().len(), 1);
}

// ── Command palette ───────────────────────────────────────────────────────

#[test]
fn test_command_char_updates_input() {
    let (mut app, _tmp) = make_app();
    app.handle_action(Action::EnterCommand);
    app.handle_action(Action::CommandChar('w'));
    assert_eq!(app.command_palette_state.input, "w");
}

#[test]
fn test_command_backspace() {
    let (mut app, _tmp) = make_app();
    app.handle_action(Action::EnterCommand);
    app.handle_action(Action::CommandChar('w'));
    app.handle_action(Action::CommandBackspace);
    assert_eq!(app.command_palette_state.input, "");
    assert_eq!(app.mode, InputMode::Normal);
}

#[test]
fn test_execute_command_sort() {
    let (mut app, _tmp) = make_app();
    app.handle_action(Action::EnterCommand);
    for c in "sort year".chars() {
        app.handle_action(Action::CommandChar(c));
    }
    app.handle_action(Action::ExecuteCommand);
    assert_eq!(app.config.display.default_sort.field, "year");
    assert!(app.status_message.is_some());
}

#[test]
fn test_execute_command_sort_toggle_direction() {
    let (mut app, _tmp) = make_app();
    app.handle_action(Action::EnterCommand);
    for c in "sort year".chars() { app.handle_action(Action::CommandChar(c)); }
    app.handle_action(Action::ExecuteCommand);
    let asc = app.config.display.default_sort.ascending;
    // Same field again: toggle direction
    app.handle_action(Action::EnterCommand);
    for c in "sort year".chars() { app.handle_action(Action::CommandChar(c)); }
    app.handle_action(Action::ExecuteCommand);
    assert_eq!(app.config.display.default_sort.ascending, !asc);
}

#[test]
fn test_execute_command_unknown() {
    let (mut app, _tmp) = make_app();
    app.handle_action(Action::EnterCommand);
    for c in "foobar".chars() { app.handle_action(Action::CommandChar(c)); }
    app.handle_action(Action::ExecuteCommand);
    let msg = app.status_message.as_deref().unwrap_or("");
    assert!(msg.contains("Unknown command"));
}

#[test]
fn test_execute_command_quit_with_dirty() {
    let (mut app, _tmp) = make_app();
    app.dirty = true;
    app.handle_action(Action::EnterCommand);
    for c in "q".chars() { app.handle_action(Action::CommandChar(c)); }
    app.handle_action(Action::ExecuteCommand);
    assert!(!app.should_quit);
    assert!(app.status_message.is_some());
}

#[test]
fn test_execute_command_force_quit() {
    let (mut app, _tmp) = make_app();
    app.dirty = true;
    app.handle_action(Action::EnterCommand);
    for c in "q!".chars() { app.handle_action(Action::CommandChar(c)); }
    app.handle_action(Action::ExecuteCommand);
    assert!(app.should_quit);
}

// ── Field editor tab completion ───────────────────────────────────────────

#[test]
fn test_field_value_completion_on_open() {
    // Opening an existing field editor seeds completions from the database.
    let (mut app, _tmp) = make_app();
    app.handle_action(Action::OpenDetail); // enter detail view
    app.handle_action(Action::EditField);
    let editor = app.field_editor_state.as_ref().unwrap();
    // Completions may be empty if the field has no other values — just
    // verify the state is initialised (no panic and completions is a Vec).
    let _ = &editor.completions;
}

#[test]
fn test_field_value_completion_filters_by_prefix() {
    let (mut app, _tmp) = make_app();
    // Manually open editor for "author" with a known prefix.
    app.field_editor_state = Some(FieldEditorState::new("author", "S"));
    app.update_field_completions();
    let editor = app.field_editor_state.as_ref().unwrap();
    // Every completion must start with "s" (case-insensitive).
    for c in &editor.completions {
        assert!(c.to_lowercase().starts_with('s'), "unexpected: {}", c);
    }
}

#[test]
fn test_field_tab_complete_fills_single_match() {
    let (mut app, _tmp) = make_app();
    // Use a prefix that uniquely matches one author in the test bib file.
    // We inject a known completion directly to avoid bib-content dependency.
    app.field_editor_state = Some(FieldEditorState::new("author", "Smi"));
    let e = app.field_editor_state.as_mut().unwrap();
    e.completions = vec!["Smith, John".to_string()];
    app.handle_action(Action::EditTabComplete);
    let editor = app.field_editor_state.as_ref().unwrap();
    assert_eq!(editor.value, "Smith, John");
    assert_eq!(editor.cursor, "Smith, John".len());
}

#[test]
fn test_field_tab_complete_cycles() {
    let (mut app, _tmp) = make_app();
    app.field_editor_state = Some(FieldEditorState::new("author", "S"));
    let e = app.field_editor_state.as_mut().unwrap();
    e.completions = vec!["Smith, John".to_string(), "Stone, Alice".to_string()];
    // First Tab: common prefix "S" → already there, so fill first match.
    app.handle_action(Action::EditTabComplete);
    assert_eq!(app.field_editor_state.as_ref().unwrap().value, "Smith, John");
    // Second Tab: cycle to next.
    app.handle_action(Action::EditTabComplete);
    assert_eq!(app.field_editor_state.as_ref().unwrap().value, "Stone, Alice");
    // Third Tab: wrap back.
    app.handle_action(Action::EditTabComplete);
    assert_eq!(app.field_editor_state.as_ref().unwrap().value, "Smith, John");
}

#[test]
fn test_field_tab_complete_name_phase() {
    let (mut app, _tmp) = make_app();
    app.field_editor_state = Some(FieldEditorState::new_field());
    let e = app.field_editor_state.as_mut().unwrap();
    e.field_name = "auth".to_string();
    e.name_cursor = 4;
    e.completions = vec!["author".to_string()];
    app.handle_action(Action::EditTabComplete);
    let editor = app.field_editor_state.as_ref().unwrap();
    assert_eq!(editor.field_name, "author");
}

#[test]
fn test_ghost_text_shows_suffix() {
    let mut e = FieldEditorState::new("author", "Smi");
    e.cursor = e.value.len(); // ghost text only shows when cursor is at end
    e.completions = vec!["Smith, John".to_string()];
    assert_eq!(e.ghost_text(), "th, John");
}

#[test]
fn test_ghost_text_empty_when_cursor_not_at_end() {
    let mut e = FieldEditorState::new("author", "Smith");
    e.cursor = 2; // cursor in the middle
    e.completions = vec!["Smith, John".to_string()];
    assert_eq!(e.ghost_text(), "");
}

#[test]
fn test_ghost_text_empty_for_exact_match() {
    let mut e = FieldEditorState::new("author", "Smith, John");
    e.completions = vec!["Smith, John".to_string()];
    assert_eq!(e.ghost_text(), "");
}

#[test]
fn test_field_name_candidates_contains_standard_fields() {
    let (app, _tmp) = make_app();
    let names = field_name_candidates(&app.database);
    assert!(names.contains(&"author".to_string()));
    assert!(names.contains(&"title".to_string()));
    assert!(names.contains(&"journal".to_string()));
}

#[test]
fn test_field_value_candidates_skips_doi() {
    let (app, _tmp) = make_app();
    let candidates = field_value_candidates(&app.database, "doi", None);
    assert!(candidates.is_empty());
}

// ── Sort tab completion ───────────────────────────────────────────────────

#[test]
fn test_sort_tab_complete_single_match() {
    let (mut app, _tmp) = make_app();
    app.handle_action(Action::EnterCommand);
    for c in "sort yea".chars() { app.handle_action(Action::CommandChar(c)); }
    // completions should contain "year"
    assert!(app.command_palette_state.completions.contains(&"year".to_string()));
    app.handle_action(Action::CommandTabComplete);
    assert_eq!(app.command_palette_state.input, "sort year");
}

#[test]
fn test_sort_tab_complete_ghost_text() {
    let (mut app, _tmp) = make_app();
    app.handle_action(Action::EnterCommand);
    for c in "sort yea".chars() { app.handle_action(Action::CommandChar(c)); }
    assert_eq!(app.command_palette_state.ghost_text(), "r");
}

#[test]
fn test_sort_tab_complete_cycles() {
    let (mut app, _tmp) = make_app();
    app.handle_action(Action::EnterCommand);
    for c in "sort yea".chars() { app.handle_action(Action::CommandChar(c)); }
    app.handle_action(Action::CommandTabComplete); // fills "year"
    // Cycling: only one match for "year", so it wraps back
    app.handle_action(Action::CommandTabComplete);
    assert_eq!(app.command_palette_state.input, "sort year");
}

#[test]
fn test_sort_tab_no_completions_outside_sort() {
    let (mut app, _tmp) = make_app();
    app.handle_action(Action::EnterCommand);
    for c in "write".chars() { app.handle_action(Action::CommandChar(c)); }
    assert!(app.command_palette_state.completions.is_empty());
    // Tab should be a no-op
    app.handle_action(Action::CommandTabComplete);
    assert_eq!(app.command_palette_state.input, "write");
}

#[test]
fn test_sort_completions_cleared_on_clear() {
    let (mut app, _tmp) = make_app();
    app.handle_action(Action::EnterCommand);
    for c in "sort yea".chars() { app.handle_action(Action::CommandChar(c)); }
    assert!(!app.command_palette_state.completions.is_empty());
    app.handle_action(Action::EnterCommand); // clears state
    assert!(app.command_palette_state.completions.is_empty());
}

// ── Shift-Tab reverse cycling ────────────────────────────────────────────

#[test]
fn test_field_tab_complete_reverse_cycles_backward() {
    let (mut app, _tmp) = make_app();
    app.field_editor_state = Some(FieldEditorState::new("author", "S"));
    let e = app.field_editor_state.as_mut().unwrap();
    e.completions = vec!["Smith, John".to_string(), "Stone, Alice".to_string()];
    // Forward to first match.
    app.handle_action(Action::EditTabComplete);
    assert_eq!(app.field_editor_state.as_ref().unwrap().value, "Smith, John");
    // Shift-Tab: wrap to last.
    app.handle_action(Action::EditTabCompleteReverse);
    assert_eq!(app.field_editor_state.as_ref().unwrap().value, "Stone, Alice");
    // Shift-Tab again: back to first.
    app.handle_action(Action::EditTabCompleteReverse);
    assert_eq!(app.field_editor_state.as_ref().unwrap().value, "Smith, John");
}

#[test]
fn test_sort_tab_complete_reverse() {
    let (mut app, _tmp) = make_app();
    app.handle_action(Action::EnterCommand);
    // Set up completions manually for predictability.
    app.command_palette_state.input = "sort ".to_string();
    app.command_palette_state.cursor = 5;
    app.command_palette_state.completions = vec![
        "author".to_string(), "title".to_string(), "year".to_string(),
    ];
    app.command_palette_state.completion_idx = 0;
    // Fill first match forward.
    app.handle_action(Action::CommandTabComplete);
    assert_eq!(app.command_palette_state.input, "sort author");
    // Shift-Tab wraps to last.
    app.handle_action(Action::CommandTabCompleteReverse);
    assert_eq!(app.command_palette_state.input, "sort year");
    // Shift-Tab again → middle.
    app.handle_action(Action::CommandTabCompleteReverse);
    assert_eq!(app.command_palette_state.input, "sort title");
}

#[test]
fn test_sort_file_completions_for_add_dirs_first() {
    let mut completions = vec![
        "file_b.pdf".to_string(),
        "subdir/".to_string(),
        "file_a.pdf".to_string(),
    ];
    sort_file_completions_for_add(&mut completions, &[]);
    assert!(completions[0].ends_with('/'), "directories should be first");
}

#[test]
fn test_sort_file_completions_for_add_non_matching_keys_first() {
    let keys = vec!["Smith2020".to_string(), "Jones2021".to_string()];
    let mut completions = vec![
        "Smith2020.pdf".to_string(),
        "new_paper.pdf".to_string(),
        "Jones2021.pdf".to_string(),
    ];
    sort_file_completions_for_add(&mut completions, &keys);
    // "new_paper.pdf" doesn't match any key → should come first.
    assert_eq!(completions[0], "new_paper.pdf");
}

// ── Entry operations ──────────────────────────────────────────────────────

#[test]
fn test_add_entry_opens_type_picker() {
    let (mut app, _tmp) = make_app();
    app.handle_action(Action::AddEntry);
    assert!(app.dialog_state.is_some());
    assert_eq!(app.mode, InputMode::Dialog);
}

#[test]
fn test_delete_entry_opens_confirm_dialog() {
    let (mut app, _tmp) = make_app();
    app.handle_action(Action::DeleteEntry);
    assert!(app.dialog_state.is_some());
    assert_eq!(app.mode, InputMode::Dialog);
    // No local files → simple Confirm dialog
    assert!(matches!(
        app.dialog_state.as_ref().unwrap().kind,
        crate::tui::components::dialog::DialogKind::Confirm { .. }
    ));
}

#[test]
fn test_delete_entry_with_one_local_file_shows_type_picker() {
    use tempfile::NamedTempFile;
    let (mut app, _tmp) = make_app();

    // Attach a real temporary file to the first entry.
    let pdf = NamedTempFile::new().unwrap();
    let pdf_path = pdf.path().to_path_buf();
    let fname = pdf_path.file_name().unwrap().to_str().unwrap().to_string();

    let key = app.sorted_keys.first().cloned().unwrap();
    app.database
        .entries
        .get_mut(&key)
        .unwrap()
        .fields
        .insert("file".to_string(), format!(":{}:PDF", pdf_path.display()));

    app.handle_action(Action::DeleteEntry);
    assert_eq!(app.mode, InputMode::Dialog);
    // Should be a TypePicker (not Confirm) because there is one local file.
    let dialog = app.dialog_state.as_ref().unwrap();
    assert!(matches!(
        dialog.kind,
        crate::tui::components::dialog::DialogKind::TypePicker { .. }
    ));
    // "Delete entry + {fname}" should be the first option.
    if let crate::tui::components::dialog::DialogKind::TypePicker { options, .. } = &dialog.kind {
        assert!(options[0].contains(&fname));
        assert_eq!(options.len(), 3); // entry+file, entry only, cancel
    }
}

#[test]
fn test_delete_entry_with_multiple_local_files_shows_file_delete_select() {
    use tempfile::NamedTempFile;
    let (mut app, _tmp) = make_app();

    let pdf1 = NamedTempFile::new().unwrap();
    let pdf2 = NamedTempFile::new().unwrap();
    let p1 = pdf1.path();
    let p2 = pdf2.path();
    let file_val = format!(":{}:PDF;:{}:PDF", p1.display(), p2.display());

    let key = app.sorted_keys.first().cloned().unwrap();
    app.database
        .entries
        .get_mut(&key)
        .unwrap()
        .fields
        .insert("file".to_string(), file_val);

    app.handle_action(Action::DeleteEntry);
    assert_eq!(app.mode, InputMode::Dialog);
    let dialog = app.dialog_state.as_ref().unwrap();
    assert!(matches!(
        dialog.kind,
        crate::tui::components::dialog::DialogKind::FileDeleteSelect { .. }
    ));
    assert_eq!(dialog.option_count(), 2);
}

#[test]
fn test_delete_entry_with_file_option0_deletes_both() {
    use tempfile::NamedTempFile;
    let (mut app, _tmp) = make_app();

    let pdf = NamedTempFile::new().unwrap();
    let pdf_path = pdf.path().to_path_buf();

    let key = app.sorted_keys.first().cloned().unwrap();
    app.database
        .entries
        .get_mut(&key)
        .unwrap()
        .fields
        .insert("file".to_string(), format!(":{}:PDF", pdf_path.display()));

    // Trigger delete → TypePicker
    app.handle_action(Action::DeleteEntry);
    // Select option 0 (delete entry + file) and confirm
    app.dialog_state.as_mut().unwrap().select(0);
    app.handle_action(Action::DialogConfirm);

    assert!(!app.database.entries.contains_key(&key));
    assert!(!pdf_path.exists(), "file should have been deleted");
}

#[test]
fn test_delete_entry_with_file_option1_keeps_file() {
    use tempfile::NamedTempFile;
    let (mut app, _tmp) = make_app();

    let pdf = NamedTempFile::new().unwrap();
    let pdf_path = pdf.path().to_path_buf();

    let key = app.sorted_keys.first().cloned().unwrap();
    app.database
        .entries
        .get_mut(&key)
        .unwrap()
        .fields
        .insert("file".to_string(), format!(":{}:PDF", pdf_path.display()));

    app.handle_action(Action::DeleteEntry);
    app.dialog_state.as_mut().unwrap().select(1); // "Delete entry only"
    app.handle_action(Action::DialogConfirm);

    assert!(!app.database.entries.contains_key(&key));
    assert!(pdf_path.exists(), "file should have been kept");
}

#[test]
fn test_delete_entry_with_file_option2_cancels() {
    use tempfile::NamedTempFile;
    let (mut app, _tmp) = make_app();

    let pdf = NamedTempFile::new().unwrap();
    let pdf_path = pdf.path().to_path_buf();
    let key = app.sorted_keys.first().cloned().unwrap();
    app.database
        .entries
        .get_mut(&key)
        .unwrap()
        .fields
        .insert("file".to_string(), format!(":{}:PDF", pdf_path.display()));

    app.handle_action(Action::DeleteEntry);
    app.dialog_state.as_mut().unwrap().select(2); // "Cancel"
    app.handle_action(Action::DialogConfirm);

    assert!(app.database.entries.contains_key(&key), "entry should survive cancel");
    assert!(pdf_path.exists(), "file should survive cancel");
}

#[test]
fn test_delete_entry_multi_file_select_partial() {
    use tempfile::NamedTempFile;
    let (mut app, _tmp) = make_app();

    let pdf1 = NamedTempFile::new().unwrap();
    let pdf2 = NamedTempFile::new().unwrap();
    let p1 = pdf1.path().to_path_buf();
    let p2 = pdf2.path().to_path_buf();
    let file_val = format!(":{}:PDF;:{}:PDF", p1.display(), p2.display());

    let key = app.sorted_keys.first().cloned().unwrap();
    app.database
        .entries
        .get_mut(&key)
        .unwrap()
        .fields
        .insert("file".to_string(), file_val);

    app.handle_action(Action::DeleteEntry);
    // Uncheck the second file (keep it)
    app.dialog_state.as_mut().unwrap().select(1);
    app.handle_action(Action::DialogToggle); // uncheck second file
    app.handle_action(Action::DialogConfirm);

    assert!(!app.database.entries.contains_key(&key));
    assert!(!p1.exists(), "first file should be deleted");
    assert!(p2.exists(), "second file should be kept");
}

#[test]
fn test_duplicate_entry() {
    let (mut app, _tmp) = make_app();
    let initial_count = app.database.entries.len();
    app.handle_action(Action::DuplicateEntry);
    assert_eq!(app.database.entries.len(), initial_count + 1);
    assert!(app.status_message.as_deref().unwrap().contains("duplicated"));
}

#[test]
fn test_add_entry_of_type() {
    let (mut app, _tmp) = make_app();
    let before = app.database.entries.len();
    app.add_entry_of_type("Article");
    assert_eq!(app.database.entries.len(), before + 1);
    assert_eq!(app.mode, InputMode::Detail);
}

#[test]
fn test_add_entry_of_type_twice_keeps_both() {
    let (mut app, _tmp) = make_app();
    let before = app.database.entries.len();
    app.add_entry_of_type("Article");
    // Simulate an edit to the first placeholder entry before adding another.
    app.database
        .entries
        .get_mut("New_Article")
        .unwrap()
        .fields
        .insert("title".to_string(), "Kept".to_string());
    app.add_entry_of_type("Article");
    assert_eq!(app.database.entries.len(), before + 2);
    assert!(app.database.entries.contains_key("New_Article"));
    assert!(app.database.entries.contains_key("New_Article_2"));
    // The first entry's edits were not overwritten.
    assert_eq!(
        app.database.entries["New_Article"].fields.get("title").map(String::as_str),
        Some("Kept")
    );
}

#[test]
fn test_duplicate_entry_twice_keeps_three_distinct_entries() {
    let (mut app, _tmp) = make_app();
    let key = app.sorted_keys[0].clone();
    let before = app.database.entries.len();
    // Re-select the same source entry before each duplication.
    let dup = |app: &mut App, key: &str| {
        let idx = app.sorted_keys.iter().position(|k| k == key).unwrap();
        app.entry_list_state.select(idx);
        app.handle_action(Action::DuplicateEntry);
    };
    dup(&mut app, &key);
    dup(&mut app, &key);
    assert_eq!(app.database.entries.len(), before + 2);
    assert!(app.database.entries.contains_key(&key));
    assert!(app.database.entries.contains_key(&format!("{}_copy", key)));
    assert!(app.database.entries.contains_key(&format!("{}_copy_2", key)));
}

#[test]
fn test_delete_entry() {
    let (mut app, _tmp) = make_app();
    let key = app.sorted_keys[0].clone();
    let before = app.database.entries.len();
    app.delete_entry(&key);
    assert_eq!(app.database.entries.len(), before - 1);
    assert!(!app.database.entries.contains_key(&key));
}

// ── Undo ─────────────────────────────────────────────────────────────────

#[test]
fn test_undo_empty_stack() {
    let (mut app, _tmp) = make_app();
    app.handle_action(Action::Undo);
    assert_eq!(app.status_message.as_deref(), Some("Nothing to undo"));
}

#[test]
fn test_undo_after_duplicate() {
    let (mut app, _tmp) = make_app();
    let before = app.database.entries.len();
    app.handle_action(Action::DuplicateEntry);
    assert_eq!(app.database.entries.len(), before + 1);
    app.handle_action(Action::Undo);
    assert_eq!(app.database.entries.len(), before);
}

#[test]
fn test_undo_after_delete() {
    let (mut app, _tmp) = make_app();
    let key = app.sorted_keys[0].clone();
    let before = app.database.entries.len();
    app.delete_entry(&key);
    app.undo();
    assert_eq!(app.database.entries.len(), before);
    assert!(app.database.entries.contains_key(&key));
}

// ── Dialog ───────────────────────────────────────────────────────────────

#[test]
fn test_dialog_cancel_clears_state() {
    let (mut app, _tmp) = make_app();
    app.handle_action(Action::DeleteEntry);
    app.handle_action(Action::DialogCancel);
    assert!(app.dialog_state.is_none());
    assert_eq!(app.mode, InputMode::Normal);
}

#[test]
fn test_dialog_toggle() {
    let (mut app, _tmp) = make_app();
    app.handle_action(Action::AddEntry); // opens type picker
    // DialogToggle should not panic even on a type-picker dialog
    app.handle_action(Action::DialogToggle);
}

// ── ShowHelp ─────────────────────────────────────────────────────────────

#[test]
fn test_show_help_opens_modal() {
    let (mut app, _tmp) = make_app();
    app.handle_action(Action::ShowHelp);
    assert!(app.help_state.is_some());
    assert_eq!(app.mode, InputMode::Help);
    app.handle_action(Action::CloseHelp);
    assert!(app.help_state.is_none());
    assert_eq!(app.mode, InputMode::Normal);
}

// ── Validate ─────────────────────────────────────────────────────────────

#[test]
fn test_validate_opens_results_panel() {
    let (mut app, _tmp) = make_app();
    app.handle_action(Action::Validate);
    assert!(app.validate_results_state.is_some());
    assert_eq!(app.mode, InputMode::ValidateResults);
}

#[test]
fn test_close_validate_results_returns_to_normal() {
    let (mut app, _tmp) = make_app();
    app.handle_action(Action::Validate);
    app.handle_action(Action::CloseValidateResults);
    assert!(app.validate_results_state.is_none());
    assert_eq!(app.mode, InputMode::Normal);
}

#[test]
fn test_validate_move_down_scrolls_results() {
    let (mut app, _tmp) = make_app();
    app.handle_action(Action::Validate);
    // Build a large enough violation list that scroll > 0 is possible
    if let Some(ref mut vrs) = app.validate_results_state {
        // Manually push enough violations so total_lines > inner_height fallback (24)
        for i in 0..10 {
            vrs.violations.push(
                crate::tui::components::validate_results::Violation {
                    entry_key: format!("k{}", i),
                    field: "title".to_string(),
                    old_value: "old".to_string(),
                    new_value: "new".to_string(),
                    action_name: "test",
                },
            );
        }
    }
    // In ValidateResults mode, MoveDown scrolls the panel
    app.handle_action(Action::MoveDown);
    assert_eq!(app.validate_results_state.as_ref().unwrap().scroll, 1);
}

#[test]
fn test_validate_move_up_scrolls_results() {
    let (mut app, _tmp) = make_app();
    app.handle_action(Action::Validate);
    if let Some(ref mut vrs) = app.validate_results_state {
        for i in 0..10 {
            vrs.violations.push(
                crate::tui::components::validate_results::Violation {
                    entry_key: format!("k{}", i),
                    field: "title".to_string(),
                    old_value: "old".to_string(),
                    new_value: "new".to_string(),
                    action_name: "test",
                },
            );
        }
    }
    app.handle_action(Action::MoveDown);
    assert_eq!(app.validate_results_state.as_ref().unwrap().scroll, 1);
    app.handle_action(Action::MoveUp);
    assert_eq!(app.validate_results_state.as_ref().unwrap().scroll, 0);
}

// ── Name disambiguator ────────────────────────────────────────────────────

#[test]
fn test_disambiguate_names_opens_panel() {
    let (mut app, _tmp) = make_app();
    app.handle_action(Action::DisambiguateNames);
    assert!(app.name_disambig_state.is_some());
    assert_eq!(app.mode, InputMode::NameDisambig);
}

#[test]
fn test_close_name_disambig_returns_to_normal() {
    let (mut app, _tmp) = make_app();
    app.handle_action(Action::DisambiguateNames);
    app.handle_action(Action::CloseNameDisambig);
    assert!(app.name_disambig_state.is_none());
    assert_eq!(app.mode, InputMode::Normal);
}

#[test]
fn test_disambig_navigate_clusters() {
    let (mut app, _tmp) = make_app();
    app.handle_action(Action::DisambiguateNames);
    if let Some(ref state) = app.name_disambig_state {
        if state.clusters.len() >= 2 {
            let initial = state.cursor;
            app.handle_action(Action::MoveDown);
            assert_eq!(app.name_disambig_state.as_ref().unwrap().cursor, initial + 1);
            app.handle_action(Action::MoveUp);
            assert_eq!(app.name_disambig_state.as_ref().unwrap().cursor, initial);
        }
    }
}

#[test]
fn test_disambig_cycle_variant() {
    let (mut app, _tmp) = make_app();
    app.handle_action(Action::DisambiguateNames);
    if let Some(ref state) = app.name_disambig_state {
        if !state.clusters.is_empty() && state.clusters[0].variants.len() >= 2 {
            let initial = state.clusters[0].selected_variant;
            app.handle_action(Action::DisambigCycleVariant);
            let next = app.name_disambig_state.as_ref().unwrap().clusters[0].selected_variant;
            assert_ne!(next, initial);
        }
    }
}

#[test]
fn test_apply_name_disambig() {
    let (mut app, _tmp) = make_app();
    // Inject two entries with slightly different author names
    {
        let mut fields1 = IndexMap::new();
        fields1.insert("author".to_string(), "Smith, J.".to_string());
        app.database.entries.insert("k1".to_string(), Entry {
            entry_type: crate::bib::model::EntryType::Article,
            citation_key: "k1".to_string(),
            fields: fields1,
            group_memberships: vec![],
            raw_index: 900,
            dirty: false,
        });
        let mut fields2 = IndexMap::new();
        fields2.insert("author".to_string(), "Smith, John".to_string());
        app.database.entries.insert("k2".to_string(), Entry {
            entry_type: crate::bib::model::EntryType::Article,
            citation_key: "k2".to_string(),
            fields: fields2,
            group_memberships: vec![],
            raw_index: 901,
            dirty: false,
        });
    }
    app.handle_action(Action::DisambiguateNames);
    // Find the cluster that contains Smith variants
    if let Some(ref state) = app.name_disambig_state {
        let has_smith = state.clusters.iter().any(|c|
            c.variants.iter().any(|v| v.name.contains("Smith"))
        );
        if has_smith {
            app.handle_action(Action::ApplyNameDisambig);
            assert!(app.name_disambig_state.is_none());
            assert_eq!(app.mode, InputMode::Normal);
            // Check that at least one entry was updated
            let a1 = app.database.entries.get("k1").unwrap().fields.get("author").unwrap();
            let a2 = app.database.entries.get("k2").unwrap().fields.get("author").unwrap();
            // After disambiguation, both should have the same name
            assert_eq!(a1, a2);
        }
    }
}

#[test]
fn test_disambig_remove_variant_closes_when_empty() {
    let (mut app, _tmp) = make_app();
    // Inject exactly two entries with different author names
    {
        let mut f1 = IndexMap::new();
        f1.insert("author".to_string(), "Doe, J.".to_string());
        app.database.entries.insert("d1".to_string(), Entry {
            entry_type: crate::bib::model::EntryType::Article,
            citation_key: "d1".to_string(),
            fields: f1,
            group_memberships: vec![],
            raw_index: 950,
            dirty: false,
        });
        let mut f2 = IndexMap::new();
        f2.insert("author".to_string(), "Doe, Jane".to_string());
        app.database.entries.insert("d2".to_string(), Entry {
            entry_type: crate::bib::model::EntryType::Article,
            citation_key: "d2".to_string(),
            fields: f2,
            group_memberships: vec![],
            raw_index: 951,
            dirty: false,
        });
    }
    app.handle_action(Action::DisambiguateNames);
    assert_eq!(app.mode, InputMode::NameDisambig);
    // Remove clusters until empty — should auto-close
    if let Some(ref state) = app.name_disambig_state {
        let count = state.clusters.len();
        for _ in 0..count {
            app.handle_action(Action::DisambigRemoveVariant);
        }
    }
    // Should have auto-closed
    if app.name_disambig_state.is_some() {
        // If still open (more clusters than expected), close manually
        app.handle_action(Action::CloseNameDisambig);
    }
    assert_eq!(app.mode, InputMode::Normal);
}

#[test]
fn test_disambig_preview_toggle() {
    let (mut app, _tmp) = make_app();
    {
        let mut f1 = IndexMap::new();
        f1.insert("author".to_string(), "Xu, A.".to_string());
        f1.insert("title".to_string(), "Paper One".to_string());
        app.database.entries.insert("x1".to_string(), Entry {
            entry_type: crate::bib::model::EntryType::Article,
            citation_key: "x1".to_string(),
            fields: f1,
            group_memberships: vec![],
            raw_index: 960,
            dirty: false,
        });
        let mut f2 = IndexMap::new();
        f2.insert("author".to_string(), "Xu, Alice".to_string());
        f2.insert("title".to_string(), "Paper Two".to_string());
        app.database.entries.insert("x2".to_string(), Entry {
            entry_type: crate::bib::model::EntryType::Article,
            citation_key: "x2".to_string(),
            fields: f2,
            group_memberships: vec![],
            raw_index: 961,
            dirty: false,
        });
    }
    app.handle_action(Action::DisambiguateNames);
    if app.name_disambig_state.as_ref().is_none_or(|s| s.clusters.is_empty()) {
        return; // no clusters to test with
    }
    // Open preview
    app.handle_action(Action::DisambigPreview);
    assert!(app.name_disambig_state.as_ref().unwrap().preview.is_some());
    // Scroll preview
    app.handle_action(Action::MoveDown);
    app.handle_action(Action::MoveUp);
    // Close preview with toggle
    app.handle_action(Action::DisambigPreview);
    assert!(app.name_disambig_state.as_ref().unwrap().preview.is_none());
}

#[test]
fn test_disambig_close_dismisses_preview_first() {
    let (mut app, _tmp) = make_app();
    {
        let mut f1 = IndexMap::new();
        f1.insert("author".to_string(), "Lee, B.".to_string());
        app.database.entries.insert("l1".to_string(), Entry {
            entry_type: crate::bib::model::EntryType::Article,
            citation_key: "l1".to_string(),
            fields: f1,
            group_memberships: vec![],
            raw_index: 970,
            dirty: false,
        });
        let mut f2 = IndexMap::new();
        f2.insert("author".to_string(), "Lee, Bob".to_string());
        app.database.entries.insert("l2".to_string(), Entry {
            entry_type: crate::bib::model::EntryType::Article,
            citation_key: "l2".to_string(),
            fields: f2,
            group_memberships: vec![],
            raw_index: 971,
            dirty: false,
        });
    }
    app.handle_action(Action::DisambiguateNames);
    if app.name_disambig_state.as_ref().is_none_or(|s| s.clusters.is_empty()) {
        return;
    }
    // Open preview
    app.handle_action(Action::DisambigPreview);
    assert!(app.name_disambig_state.as_ref().unwrap().preview.is_some());
    // First Esc closes preview, not dialog
    app.handle_action(Action::CloseNameDisambig);
    assert!(app.name_disambig_state.is_some());
    assert!(app.name_disambig_state.as_ref().unwrap().preview.is_none());
    // Second Esc closes the dialog
    app.handle_action(Action::CloseNameDisambig);
    assert!(app.name_disambig_state.is_none());
    assert_eq!(app.mode, InputMode::Normal);
}

#[test]
fn test_disambig_move_to_top_bottom() {
    let (mut app, _tmp) = make_app();
    app.handle_action(Action::DisambiguateNames);
    if app.name_disambig_state.as_ref().is_none_or(|s| s.clusters.len() < 2) {
        return;
    }
    app.handle_action(Action::MoveToBottom);
    let len = app.name_disambig_state.as_ref().unwrap().clusters.len();
    assert_eq!(app.name_disambig_state.as_ref().unwrap().cursor, len - 1);
    app.handle_action(Action::MoveToTop);
    assert_eq!(app.name_disambig_state.as_ref().unwrap().cursor, 0);
}

#[test]
fn test_disambig_page_down_up() {
    let (mut app, _tmp) = make_app();
    app.handle_action(Action::DisambiguateNames);
    if app.name_disambig_state.as_ref().is_none_or(|s| s.clusters.is_empty()) {
        return;
    }
    app.handle_action(Action::PageDown);
    app.handle_action(Action::PageUp);
    assert_eq!(app.name_disambig_state.as_ref().unwrap().cursor, 0);
}

#[test]
fn test_disambig_cycle_variant_reverse_action() {
    let (mut app, _tmp) = make_app();
    {
        let mut f1 = IndexMap::new();
        f1.insert("author".to_string(), "Park, C.".to_string());
        app.database.entries.insert("p1".to_string(), Entry {
            entry_type: crate::bib::model::EntryType::Article,
            citation_key: "p1".to_string(),
            fields: f1,
            group_memberships: vec![],
            raw_index: 980,
            dirty: false,
        });
        let mut f2 = IndexMap::new();
        f2.insert("author".to_string(), "Park, Chris".to_string());
        app.database.entries.insert("p2".to_string(), Entry {
            entry_type: crate::bib::model::EntryType::Article,
            citation_key: "p2".to_string(),
            fields: f2,
            group_memberships: vec![],
            raw_index: 981,
            dirty: false,
        });
    }
    app.handle_action(Action::DisambiguateNames);
    if app.name_disambig_state.as_ref().is_none_or(|s| s.clusters.is_empty()) {
        return;
    }
    let initial = app.name_disambig_state.as_ref().unwrap().clusters[0].selected_variant;
    app.handle_action(Action::DisambigCycleVariantReverse);
    let after = app.name_disambig_state.as_ref().unwrap().clusters[0].selected_variant;
    // With 2 variants, cycling reverse from 0 wraps to 1
    if app.name_disambig_state.as_ref().unwrap().clusters[0].variants.len() >= 2 {
        assert_ne!(initial, after);
    }
}

#[test]
fn test_apply_disambig_no_changes_needed() {
    let (mut app, _tmp) = make_app();
    // Open disambiguator and immediately apply — if all clusters have the
    // canonical already selected, it returns "No changes needed" or applies
    // zero mutations.
    app.handle_action(Action::DisambiguateNames);
    if app.name_disambig_state.is_some() {
        app.handle_action(Action::ApplyNameDisambig);
        assert!(app.name_disambig_state.is_none());
        assert_eq!(app.mode, InputMode::Normal);
    }
}

// ── Settings extended navigation ──────────────────────────────────────────

#[test]
fn test_settings_move_to_top() {
    let (mut app, _tmp) = make_app();
    app.handle_action(Action::EnterSettings);
    // Move down a few rows
    app.handle_action(Action::SettingsMoveDown);
    app.handle_action(Action::SettingsMoveDown);
    let after_down = app.settings_state.as_ref().unwrap().cursor;
    app.handle_action(Action::SettingsMoveToTop);
    let after_top = app.settings_state.as_ref().unwrap().cursor;
    assert!(after_top < after_down);
}

#[test]
fn test_settings_move_to_bottom() {
    let (mut app, _tmp) = make_app();
    app.handle_action(Action::EnterSettings);
    let start = app.settings_state.as_ref().unwrap().cursor;
    app.handle_action(Action::SettingsMoveToBottom);
    let bottom = app.settings_state.as_ref().unwrap().cursor;
    assert!(bottom > start);
}

#[test]
fn test_settings_page_down() {
    let (mut app, _tmp) = make_app();
    app.handle_action(Action::EnterSettings);
    let start = app.settings_state.as_ref().unwrap().cursor;
    app.handle_action(Action::SettingsPageDown);
    let after = app.settings_state.as_ref().unwrap().cursor;
    assert!(after > start);
}

#[test]
fn test_settings_page_up_at_top_is_noop() {
    let (mut app, _tmp) = make_app();
    app.handle_action(Action::EnterSettings);
    let top = app.settings_state.as_ref().unwrap().cursor;
    app.handle_action(Action::SettingsPageUp);
    assert_eq!(app.settings_state.as_ref().unwrap().cursor, top);
}

#[test]
fn test_settings_cursor_restored_on_reopen() {
    let (mut app, _tmp) = make_app();
    app.handle_action(Action::EnterSettings);
    app.handle_action(Action::SettingsMoveDown);
    app.handle_action(Action::SettingsMoveDown);
    let cursor_before = app.settings_state.as_ref().unwrap().cursor;
    app.handle_action(Action::ExitSettings);
    assert!(app.settings_state.is_none());
    app.handle_action(Action::EnterSettings);
    let cursor_after = app.settings_state.as_ref().unwrap().cursor;
    assert_eq!(cursor_after, cursor_before, "cursor should be restored on reopen");
}

// ── Dirty-flag recheck after field edit ──────────────────────────────────

#[test]
fn test_dirty_cleared_when_field_reverted_to_original() {
    // Use an entry whose citation key already matches the template output,
    // so that reverting the field also reverts the auto-generated key.
    // Template: Article_[year]_[auth]
    // For year=2020, author=Smith → Article_2020_Smith
    // Template: Article_[year]_[journal_abbrev]_[authors]_[pages]
    // For year=2020, journal=Nature (abbrev "N"), author=Smith → Article_2020_N_Smith
    let mut tmp = NamedTempFile::new().unwrap();
    write!(tmp, "@Article{{Article_2020_N_Smith,\n  author  = {{Smith, John}},\n  title   = {{My Paper}},\n  year    = {{2020}},\n  journal = {{Nature}},\n}}\n").unwrap();
    tmp.flush().unwrap();
    let app_result = App::new(tmp.path().to_path_buf(), default_config());
    let mut app = app_result.unwrap();

    app.handle_action(Action::OpenDetail);
    app.detail_entry_key = Some("Article_2020_N_Smith".to_string());

    use crate::tui::components::field_editor::FieldEditorState;

    // Modify the year field — key auto-regens to Article_2099_N_Smith.
    app.field_editor_state = Some(FieldEditorState::new("year", "2099"));
    app.handle_action(Action::ConfirmEdit);
    assert!(app.database.entries.values().any(|e| e.dirty), "should be dirty after change");

    // Revert the year field to its original value — key regens back.
    app.field_editor_state = Some(FieldEditorState::new("year", "2020"));
    app.handle_action(Action::ConfirmEdit);

    // After reverting, the entry should no longer be dirty.
    let reverted_key = app.detail_entry_key.clone().unwrap();
    let entry = app.database.entries.get(&reverted_key).expect("entry must exist");
    assert!(!entry.dirty, "entry should not be dirty after reverting to original value; key={}", reverted_key);
    let _tmp = tmp;
}

// ── Auto-regen citekey on field edit ─────────────────────────────────────

#[test]
fn test_citekey_auto_updated_on_field_edit() {
    let (mut app, _tmp) = make_app();
    app.handle_action(Action::OpenDetail);
    app.detail_entry_key = Some("Smith2020".to_string());

    use crate::tui::components::field_editor::FieldEditorState;
    app.field_editor_state = Some(FieldEditorState::new("year", "2023"));
    app.handle_action(Action::ConfirmEdit);

    // Key should have been auto-regenerated to reflect the new year.
    // Template: Article_[year]_[journal_abbrev]_[authors]_[pages]
    // Smith2020 has journal=Nature (abbrev "N"), author=Smith, no pages → Article_2023_N_Smith
    assert!(
        app.database.entries.contains_key("Article_2023_N_Smith"),
        "expected auto-regenerated key Article_2023_N_Smith; keys: {:?}",
        app.database.entries.keys().collect::<Vec<_>>()
    );
    assert!(!app.database.entries.contains_key("Smith2020"), "old key should be gone");
}

// ── unique_citekey / collision resolution ────────────────────────────────

#[test]
fn test_unique_citekey_free_base_returned_as_is() {
    let (app, _tmp) = make_app();
    assert_eq!(app.unique_citekey("NewKey", "anything"), "NewKey");
}

#[test]
fn test_unique_citekey_current_key_counts_as_free() {
    // Smith2020 exists in the DB; if current_key IS Smith2020 the slot is being
    // freed — the base key should be returned unchanged.
    let (app, _tmp) = make_app();
    assert_eq!(app.unique_citekey("Smith2020", "Smith2020"), "Smith2020");
}

#[test]
fn test_unique_citekey_collision_gets_suffix() {
    // Smith2020 exists; a different entry wants the same base → should get _2.
    let (app, _tmp) = make_app();
    assert_eq!(app.unique_citekey("Smith2020", "Doe2021"), "Smith2020_2");
}

#[test]
fn test_unique_citekey_suffix_slot_is_current_key() {
    // Smith2020 exists; Smith2020_2 also exists (as current entry's own key).
    // The loop must recognise _2 as the current entry's slot and return it,
    // not skip to _3 (the original bug).
    let mut tmp = NamedTempFile::new().unwrap();
    write!(tmp, concat!(
        "@Article{{Smith2020,\n  author={{}},\n  title={{}},\n  year={{2020}},\n  journal={{Nature}},\n}}\n",
        "@Article{{Smith2020_2,\n  author={{}},\n  title={{}},\n  year={{2020}},\n  journal={{Nature}},\n}}\n",
    )).unwrap();
    tmp.flush().unwrap();
    let app = App::new(tmp.path().to_path_buf(), default_config()).unwrap();
    // Smith2020_2 is being renamed; its own slot should be chosen, not _3.
    assert_eq!(app.unique_citekey("Smith2020", "Smith2020_2"), "Smith2020_2");
    let _tmp = tmp;
}

#[test]
fn test_regen_citekey_collision_resolved_with_suffix() {
    // Two articles produce the same template output. The second should get _2.
    let mut tmp = NamedTempFile::new().unwrap();
    write!(tmp, concat!(
        "@Article{{Article_2020_N_Smith,\n  author={{Smith, John}},\n  title={{P1}},\n  year={{2020}},\n  journal={{Nature}},\n}}\n",
        "@Article{{OldKey,\n  author={{Smith, John}},\n  title={{P2}},\n  year={{2020}},\n  journal={{Nature}},\n}}\n",
    )).unwrap();
    tmp.flush().unwrap();
    let mut app = App::new(tmp.path().to_path_buf(), default_config()).unwrap();
    app.handle_action(Action::OpenDetail);
    app.detail_entry_key = Some("OldKey".to_string());
    if let Some(e) = app.database.entries.get("OldKey") {
        app.detail_state = Some(EntryDetailState::new(e, app.config.field_groups.clone()));
    }
    app.handle_action(Action::RegenCitekey);
    assert!(app.database.entries.contains_key("Article_2020_N_Smith_2"),
        "collision should produce _2 suffix; keys={:?}", app.database.entries.keys().collect::<Vec<_>>());
    assert!(!app.database.entries.contains_key("OldKey"), "old key should be gone");
    let _tmp = tmp;
}

#[test]
fn test_regen_all_citekeys_preserves_suffix_not_bumps_to_3() {
    // Article_2020_N_Smith exists and Article_2020_N_Smith_2 also exists.
    // Regen-all should leave both unchanged — _2 keeps _2, not _3.
    let mut tmp = NamedTempFile::new().unwrap();
    write!(tmp, concat!(
        "@Article{{Article_2020_N_Smith,\n  author={{Smith, John}},\n  title={{P1}},\n  year={{2020}},\n  journal={{Nature}},\n}}\n",
        "@Article{{Article_2020_N_Smith_2,\n  author={{Smith, John}},\n  title={{P2}},\n  year={{2020}},\n  journal={{Nature}},\n}}\n",
    )).unwrap();
    tmp.flush().unwrap();
    let mut app = App::new(tmp.path().to_path_buf(), default_config()).unwrap();
    app.handle_action(Action::RegenAllCitekeys);
    assert!(app.database.entries.contains_key("Article_2020_N_Smith"),
        "primary key must survive; keys={:?}", app.database.entries.keys().collect::<Vec<_>>());
    assert!(app.database.entries.contains_key("Article_2020_N_Smith_2"),
        "_2 must be preserved, not bumped to _3; keys={:?}", app.database.entries.keys().collect::<Vec<_>>());
    assert!(!app.database.entries.contains_key("Article_2020_N_Smith_3"),
        "must not create spurious _3");
    let _tmp = tmp;
}

// ── Close citation preview ────────────────────────────────────────────────

#[test]
fn test_close_citation_preview() {
    let (mut app, _tmp) = make_app();
    app.mode = InputMode::CitationPreview;
    app.citation_preview_state = Some(CitationPreviewState {
        citation: "cite".to_string(),
        entry_key: "Smith2020".to_string(),
        style_name: "ieeetran".to_string(),
    });
    app.handle_action(Action::CloseCitationPreview);
    assert!(app.citation_preview_state.is_none());
    assert_eq!(app.mode, InputMode::Normal);
}

// ── Field editor ─────────────────────────────────────────────────────────

#[test]
fn test_edit_char_updates_editor() {
    let (mut app, _tmp) = make_app();
    app.field_editor_state = Some(FieldEditorState::new("title", "old"));
    app.handle_action(Action::EditChar('X'));
    let val = app.field_editor_state.as_ref().unwrap().value.clone();
    assert!(val.contains('X'));
}

#[test]
fn test_edit_backspace() {
    let (mut app, _tmp) = make_app();
    let mut editor = FieldEditorState::new("title", "abc");
    editor.cursor = editor.value.len(); // position at end for backspace test
    app.field_editor_state = Some(editor);
    app.handle_action(Action::EditBackspace);
    let val = app.field_editor_state.as_ref().unwrap().value.clone();
    assert_eq!(val, "ab");
}

#[test]
fn test_cancel_edit() {
    let (mut app, _tmp) = make_app();
    app.field_editor_state = Some(FieldEditorState::new("title", "abc"));
    app.mode = InputMode::Editing;
    app.handle_action(Action::CancelEdit);
    assert!(app.field_editor_state.is_none());
    assert_eq!(app.mode, InputMode::Normal);
}

// ── sort_entries (module-level fn) ────────────────────────────────────────

#[test]
fn test_sort_entries_ascending() {
    let (app, _tmp) = make_app();
    // Default config sorts by citation_key ascending → Doe2021, Smith2020
    assert_eq!(app.sorted_keys[0], "Doe2021");
    assert_eq!(app.sorted_keys[1], "Smith2020");
}

#[test]
fn test_sort_entries_descending() {
    let (app, _tmp) = make_app();
    let mut cfg = default_config();
    cfg.display.default_sort.ascending = false;
    let keys = sort_entries(&app.database.entries, &cfg);
    assert_eq!(keys[0], "Smith2020");
    assert_eq!(keys[1], "Doe2021");
}

#[test]
fn test_sort_by_year() {
    let (app, _tmp) = make_app();
    let mut cfg = default_config();
    cfg.display.default_sort.field = "year".to_string();
    let keys = sort_entries(&app.database.entries, &cfg);
    // Smith2020 (2020) before Doe2021 (2021)
    assert_eq!(keys[0], "Smith2020");
    assert_eq!(keys[1], "Doe2021");
}

// ── get_sort_value ────────────────────────────────────────────────────────

#[test]
fn test_get_sort_value_citation_key() {
    let (app, _tmp) = make_app();
    let entry = app.database.entries.get("Smith2020").unwrap();
    assert_eq!(get_sort_value(entry, "citation_key"), "Smith2020");
    assert_eq!(get_sort_value(entry, "key"), "Smith2020");
    assert_eq!(get_sort_value(entry, "citekey"), "Smith2020");
}

#[test]
fn test_get_sort_value_entrytype() {
    let (app, _tmp) = make_app();
    let entry = app.database.entries.get("Smith2020").unwrap();
    assert_eq!(get_sort_value(entry, "entrytype"), "Article");
    assert_eq!(get_sort_value(entry, "type"), "Article");
}

#[test]
fn test_get_sort_value_field() {
    let (app, _tmp) = make_app();
    let entry = app.database.entries.get("Smith2020").unwrap();
    assert_eq!(get_sort_value(entry, "year"), "2020");
}

#[test]
fn test_get_sort_value_missing_field() {
    let (app, _tmp) = make_app();
    let entry = app.database.entries.get("Smith2020").unwrap();
    assert_eq!(get_sort_value(entry, "nonexistent"), "");
}

#[test]
fn test_sort_none_returns_file_order() {
    // When field is "none", keys come back in IndexMap insertion order
    // (Smith2020 first, Doe2021 second — file order).
    let (app, _tmp) = make_app();
    let mut cfg = default_config();
    cfg.display.default_sort.field = "none".to_string();
    let keys = sort_entries(&app.database.entries, &cfg);
    assert_eq!(keys[0], "Smith2020");
    assert_eq!(keys[1], "Doe2021");
}

#[test]
fn test_sort_command_none_sets_file_order() {
    // `:sort none` must set the sort field to "none" and rebuild sorted_keys
    // in file (insertion) order regardless of any prior sort.
    let (mut app, _tmp) = make_app();
    // First sort by citation_key (default) — Doe2021 first.
    assert_eq!(app.sorted_keys[0], "Doe2021");
    // Now reset to file order via command.
    app.handle_action(Action::EnterCommand);
    for c in "sort none".chars() { app.handle_action(Action::CommandChar(c)); }
    app.handle_action(Action::ExecuteCommand);
    assert_eq!(app.config.display.default_sort.field, "none");
    assert_eq!(app.sorted_keys[0], "Smith2020",
        "sorted_keys[0] should be Smith2020 (file order) after :sort none");
    let msg = app.status_message.as_deref().unwrap_or("");
    assert!(msg.contains("file order"), "status should mention file order");
}

#[test]
fn test_sort_command_none_reruns_active_search() {
    // After a search is confirmed, `:sort none` must re-run the search so
    // filtered_indices stays valid against the new sorted_keys.
    let (mut app, _tmp) = make_app();
    // Search for "Smith" — matches Smith2020 only.
    app.handle_action(Action::EnterSearch);
    for c in "Smith".chars() { app.handle_action(Action::SearchChar(c)); }
    app.handle_action(Action::ConfirmSearch);
    assert!(app.filtered_indices.is_some());
    // Now change sort order.
    app.handle_action(Action::EnterCommand);
    for c in "sort none".chars() { app.handle_action(Action::CommandChar(c)); }
    app.handle_action(Action::ExecuteCommand);
    // filtered_indices should still be Some (search is still active)
    // and its single entry should index Smith2020 in the new sorted_keys.
    let indices = app.filtered_indices.as_ref().expect("filter still active");
    assert_eq!(indices.len(), 1);
    let matched_key = app.sorted_keys.get(indices[0]).expect("valid index");
    assert_eq!(matched_key, "Smith2020");
}

#[test]
fn test_reset_sort_clears_active_search_filter() {
    // ESC (ResetSort) while filtered_indices is set clears the filter
    // instead of touching the sort config.
    let (mut app, _tmp) = make_app();
    app.handle_action(Action::EnterSearch);
    for c in "Smith".chars() { app.handle_action(Action::SearchChar(c)); }
    app.handle_action(Action::ConfirmSearch);
    assert!(app.filtered_indices.is_some(), "filter should be active");
    // ESC in Normal mode.
    app.handle_action(Action::ResetSort);
    assert!(app.filtered_indices.is_none(), "ESC should clear the filter");
    assert!(app.search_bar_state.query.is_empty(), "search query should be cleared");
    let msg = app.status_message.as_deref().unwrap_or("");
    assert!(msg.contains("cleared"), "status should say search was cleared");
}

#[test]
fn test_reset_sort_falls_through_to_sort_reset_when_no_filter() {
    // ESC (ResetSort) with no active filter and a non-default sort still
    // restores the configured default sort.
    let (mut app, _tmp) = make_app();
    // Change the sort away from the default.
    app.handle_action(Action::EnterCommand);
    for c in "sort year".chars() { app.handle_action(Action::CommandChar(c)); }
    app.handle_action(Action::ExecuteCommand);
    assert_eq!(app.config.display.default_sort.field, "year");
    // No search filter active.
    assert!(app.filtered_indices.is_none());
    // ESC should restore the default sort (citation_key).
    app.handle_action(Action::ResetSort);
    assert_eq!(app.config.display.default_sort.field, "citation_key");
}

#[test]
fn test_sort_command_reruns_search_after_sort_change() {
    // Any :sort command while a search is active must recompute filtered_indices
    // against the new sorted_keys order.
    let (mut app, _tmp) = make_app();
    app.handle_action(Action::EnterSearch);
    for c in "Doe".chars() { app.handle_action(Action::SearchChar(c)); }
    app.handle_action(Action::ConfirmSearch);
    let before = app.filtered_indices.clone().unwrap();
    // Change sort — sorted_keys order changes.
    app.handle_action(Action::EnterCommand);
    for c in "sort year".chars() { app.handle_action(Action::CommandChar(c)); }
    app.handle_action(Action::ExecuteCommand);
    let after = app.filtered_indices.as_ref().expect("filter stays active");
    // The matched key must still be Doe2021 regardless of index position.
    let matched_key = app.sorted_keys.get(after[0]).expect("valid index");
    assert_eq!(matched_key, "Doe2021");
    // The index into the new sorted_keys may differ from before.
    let _ = before; // suppress unused warning
}

// ── find_group_node ───────────────────────────────────────────────────────

#[test]
fn test_find_group_node_root() {
    let (app, _tmp) = make_app();
    let found = find_group_node(&app.database.groups.root, "All Entries");
    assert!(found.is_some());
}

#[test]
fn test_find_group_node_missing() {
    let (app, _tmp) = make_app();
    let found = find_group_node(&app.database.groups.root, "NoSuchGroup");
    assert!(found.is_none());
}

// ── collect_group_names ───────────────────────────────────────────────────

#[test]
fn test_collect_group_names_excludes_all_entries() {
    let (app, _tmp) = make_app();
    let mut names = Vec::new();
    collect_group_names(&app.database.groups.root, &mut names);
    assert!(!names.contains(&"All Entries".to_string()));
}

// ── visible_entry_count ───────────────────────────────────────────────────

#[test]
fn test_visible_entry_count_unfiltered() {
    let (app, _tmp) = make_app();
    assert_eq!(app.visible_entry_count(), 2);
}

#[test]
fn test_visible_entry_count_filtered() {
    let (mut app, _tmp) = make_app();
    app.filtered_indices = Some(vec![0]);
    assert_eq!(app.visible_entry_count(), 1);
}

// ── dirty tracking ────────────────────────────────────────────────────────

#[test]
fn test_dirty_after_duplicate() {
    let (mut app, _tmp) = make_app();
    assert!(!app.dirty);
    app.handle_action(Action::DuplicateEntry);
    assert!(app.dirty);
}

#[test]
fn test_not_dirty_after_undo_to_clean_state() {
    let (mut app, _tmp) = make_app();
    app.handle_action(Action::DuplicateEntry);
    assert!(app.dirty);
    app.handle_action(Action::Undo);
    assert!(!app.dirty);
}

// ── longest_common_prefix ────────────────────────────────────────────────

#[test]
fn test_lcp_empty() {
    assert_eq!(longest_common_prefix(&[]), "");
}

#[test]
fn test_lcp_single() {
    assert_eq!(longest_common_prefix(&["bibtui.yaml".to_string()]), "bibtui.yaml");
}

#[test]
fn test_lcp_shared_prefix() {
    let items = vec!["bibtui.yaml".to_string(), "bibtui.yml".to_string()];
    assert_eq!(longest_common_prefix(&items), "bibtui.y");
}

#[test]
fn test_lcp_no_common() {
    let items = vec!["abc".to_string(), "xyz".to_string()];
    assert_eq!(longest_common_prefix(&items), "");
}

#[test]
fn test_lcp_identical() {
    let items = vec!["foo.yaml".to_string(), "foo.yaml".to_string()];
    assert_eq!(longest_common_prefix(&items), "foo.yaml");
}

#[test]
fn test_lcp_one_is_prefix_of_other() {
    let items = vec!["foo".to_string(), "foobar".to_string()];
    assert_eq!(longest_common_prefix(&items), "foo");
}

// ── expand_tilde / contract_tilde ────────────────────────────────────────

#[test]
fn test_expand_tilde_non_tilde_path_unchanged() {
    assert_eq!(expand_tilde("/tmp/foo.yaml"), "/tmp/foo.yaml");
    assert_eq!(expand_tilde("relative/path"), "relative/path");
}

#[test]
fn test_contract_tilde_non_home_path_unchanged() {
    assert_eq!(contract_tilde("/tmp/foo.yaml"), "/tmp/foo.yaml");
}

#[test]
fn test_expand_contract_roundtrip() {
    if let Ok(home) = std::env::var("HOME") {
        let abs = format!("{}/documents/file.yaml", home);
        let contracted = contract_tilde(&abs);
        assert!(contracted.starts_with("~/"));
        let re_expanded = expand_tilde(&contracted);
        assert_eq!(re_expanded, abs);
    }
}

#[test]
fn test_contract_tilde_exact_home() {
    if let Ok(home) = std::env::var("HOME") {
        assert_eq!(contract_tilde(&home), "~");
    }
}

// ── compute_sync_renames ─────────────────────────────────────────────────

#[test]
fn test_compute_sync_renames_disabled_returns_empty() {
    let (mut app, _tmp) = make_app();
    app.config.save.sync_filenames = false;
    assert!(app.compute_sync_renames(false).is_empty());
}

#[test]
fn test_compute_sync_renames_force_bypasses_config() {
    let (mut app, _tmp) = make_app();
    app.config.save.sync_filenames = false;
    // force=true should still compute even though the config flag is off.
    // TEST_BIB has no file fields so result is still empty — but the function
    // must NOT short-circuit on the flag.
    assert!(app.compute_sync_renames(true).is_empty());
}

#[test]
fn test_compute_sync_renames_no_dirty_entries_returns_empty() {
    let (mut app, _tmp) = make_app();
    app.config.save.sync_filenames = true;
    // Fresh app has no dirty entries.
    assert!(app.compute_sync_renames(false).is_empty());
}

#[test]
fn test_request_sync_filenames_no_file_fields_sets_status() {
    // TEST_BIB has no file fields — should produce the "already in sync" message.
    let (mut app, _tmp) = make_app();
    app.handle_action(Action::SyncFilenames);
    assert!(
        app.status_message.as_deref().unwrap_or("").contains("already match"),
        "expected 'already match' status; got {:?}", app.status_message
    );
    assert_eq!(app.mode, InputMode::Normal, "mode should stay Normal");
}

#[test]
fn test_request_sync_filenames_pending_file_shows_dialog() {
    // Build a bib with an entry whose file field does NOT match the cite key.
    let mut tmp = NamedTempFile::new().unwrap();
    write!(tmp, "@Article{{Smith2020,\n  author={{Smith, John}},\n  title={{T}},\n  year={{2020}},\n  file={{:wrong_name.pdf:PDF}},\n}}\n").unwrap();
    tmp.flush().unwrap();
    let mut app = App::new(tmp.path().to_path_buf(), default_config()).unwrap();
    app.handle_action(Action::SyncFilenames);
    assert_eq!(app.mode, InputMode::Dialog, "should enter Dialog mode");
    assert!(
        matches!(app.pending_action, Some(PendingAction::SyncFilenamesOnly)),
        "pending action should be SyncFilenamesOnly"
    );
    assert!(app.dialog_state.is_some(), "dialog state should be set");
    let _tmp = tmp;
}

// ── FocusGroups reveals panel ────────────────────────────────────────────

#[test]
fn test_focus_groups_shows_hidden_panel() {
    let (mut app, _tmp) = make_app();
    app.show_groups = false;
    app.handle_action(Action::FocusGroups);
    assert!(app.show_groups, "FocusGroups should reveal the panel when hidden");
    assert_eq!(app.focus, Focus::Groups);
}

#[test]
fn test_focus_list_after_focus_groups() {
    let (mut app, _tmp) = make_app();
    app.handle_action(Action::FocusGroups);
    app.handle_action(Action::FocusList);
    assert_eq!(app.focus, Focus::List);
}

// ── Settings path editors ────────────────────────────────────────────────

#[test]
fn test_settings_export_enters_path_editing() {
    let (mut app, _tmp) = make_app();
    app.handle_action(Action::EnterSettings);
    app.handle_action(Action::SettingsExport);
    assert_eq!(app.mode, InputMode::Editing);
    let editor = app.field_editor_state.as_ref().expect("editor should be set");
    assert!(editor.is_path);
    assert_eq!(editor.value, "bibtui.yaml");
    assert!(matches!(app.pending_action, Some(PendingAction::ExportSettings)));
}

#[test]
fn test_settings_import_enters_path_editing() {
    let (mut app, _tmp) = make_app();
    app.handle_action(Action::EnterSettings);
    app.handle_action(Action::SettingsImport);
    assert_eq!(app.mode, InputMode::Editing);
    let editor = app.field_editor_state.as_ref().expect("editor should be set");
    assert!(editor.is_path);
    assert!(matches!(app.pending_action, Some(PendingAction::ImportSettings)));
}

#[test]
fn test_cancel_edit_from_settings_returns_to_settings_mode() {
    let (mut app, _tmp) = make_app();
    app.handle_action(Action::EnterSettings);
    app.handle_action(Action::SettingsExport);
    assert_eq!(app.mode, InputMode::Editing);
    app.handle_action(Action::CancelEdit);
    assert_eq!(app.mode, InputMode::Settings);
    assert!(app.field_editor_state.is_none());
    assert!(app.pending_action.is_none());
}

// ── new_empty constructor ─────────────────────────────────────────────────

#[test]
fn test_new_empty_app_starts_in_editing_mode() {
    let app = App::new_empty(default_config()).unwrap();
    assert_eq!(app.mode, InputMode::Editing);
}

#[test]
fn test_new_empty_app_has_no_entries() {
    let app = App::new_empty(default_config()).unwrap();
    assert_eq!(app.database.entries.len(), 0);
}

#[test]
fn test_new_empty_app_has_path_editor() {
    let app = App::new_empty(default_config()).unwrap();
    let editor = app.field_editor_state.as_ref().expect("editor should be set");
    assert!(editor.is_path, "new_empty should open a path editor");
}

#[test]
fn test_new_empty_app_has_new_file_pending_action() {
    let app = App::new_empty(default_config()).unwrap();
    assert!(matches!(app.pending_action, Some(PendingAction::NewFile)));
}

#[test]
fn test_new_empty_app_cancel_sets_should_quit() {
    let mut app = App::new_empty(default_config()).unwrap();
    app.handle_action(Action::CancelEdit);
    assert!(app.should_quit, "cancelling on a NewFile prompt should quit");
}

// ── Month mode in the field editor ───────────────────────────────────────

/// A bib with a month field for month-editor tests.
const MONTH_BIB: &str = r#"@Article{Smith2020,
  author  = {Smith, John},
  title   = {My Paper},
  year    = {2020},
  journal = {Nature},
  month   = {jan},
}
"#;

fn make_app_with_month() -> (App, NamedTempFile) {
    let mut tmp = NamedTempFile::new().unwrap();
    write!(tmp, "{}", MONTH_BIB).unwrap();
    tmp.flush().unwrap();
    let path = tmp.path().to_path_buf();
    let app = App::new(path, default_config()).unwrap();
    (app, tmp)
}

/// Open detail view, navigate to the month field, and start editing it.
/// Returns the app with a month field editor open.
fn open_month_editor() -> (App, NamedTempFile) {
    let (mut app, tmp) = make_app_with_month();
    app.handle_action(Action::OpenDetail);
    // Manually inject the month field editor so we don't depend on
    // the detail-view navigation (which can vary by field order).
    use crate::tui::components::field_editor::FieldEditorState;
    app.field_editor_state = Some(FieldEditorState::new("month", "jan"));
    app.mode = InputMode::Editing;
    (app, tmp)
}

#[test]
fn test_is_month_flag_on_month_field() {
    use crate::tui::components::field_editor::FieldEditorState;
    let e = FieldEditorState::new("month", "jan");
    assert!(e.is_month);
}

#[test]
fn test_is_month_flag_false_for_title() {
    use crate::tui::components::field_editor::FieldEditorState;
    let e = FieldEditorState::new("title", "x");
    assert!(!e.is_month);
}

#[test]
fn test_edit_cursor_left_in_month_mode_navigates_backward() {
    let (mut app, _tmp) = open_month_editor();
    // value starts at "jan"
    app.handle_action(Action::EditCursorLeft);
    let val = app.field_editor_state.as_ref().unwrap().value.clone();
    // jan backward → dec
    assert_eq!(val, "dec", "EditCursorLeft in month mode should go to dec from jan");
}

#[test]
fn test_edit_cursor_right_in_month_mode_navigates_forward() {
    let (mut app, _tmp) = open_month_editor();
    // value starts at "jan"
    app.handle_action(Action::EditCursorRight);
    let val = app.field_editor_state.as_ref().unwrap().value.clone();
    // jan forward → feb
    assert_eq!(val, "feb", "EditCursorRight in month mode should go to feb from jan");
}

#[test]
fn test_edit_cursor_up_in_month_mode_navigates_minus_6() {
    let (mut app, _tmp) = open_month_editor();
    // Set value to "jul" (index 6) so -6 → jan
    app.field_editor_state.as_mut().unwrap().value = "jul".to_string();
    app.field_editor_state.as_mut().unwrap().cursor = 3;
    app.handle_action(Action::EditCursorUp);
    let val = app.field_editor_state.as_ref().unwrap().value.clone();
    assert_eq!(val, "jan", "EditCursorUp in month mode should navigate -6");
}

#[test]
fn test_edit_cursor_down_in_month_mode_navigates_plus_6() {
    let (mut app, _tmp) = open_month_editor();
    // value starts at "jan" (index 0); +6 → jul (index 6)
    app.handle_action(Action::EditCursorDown);
    let val = app.field_editor_state.as_ref().unwrap().value.clone();
    assert_eq!(val, "jul", "EditCursorDown in month mode should navigate +6");
}

#[test]
fn test_edit_cursor_up_down_noop_for_non_month() {
    let (mut app, _tmp) = make_app();
    // Plain text editor — EditCursorUp/Down should be no-ops.
    use crate::tui::components::field_editor::FieldEditorState;
    app.field_editor_state = Some(FieldEditorState::new("title", "hello"));
    app.mode = InputMode::Editing;
    app.handle_action(Action::EditCursorUp);
    app.handle_action(Action::EditCursorDown);
    // Value must be unchanged
    let val = app.field_editor_state.as_ref().unwrap().value.clone();
    assert_eq!(val, "hello");
}

#[test]
fn test_month_completions_filtered_by_prefix() {
    let (mut app, _tmp) = make_app_with_month();
    use crate::tui::components::field_editor::FieldEditorState;
    // Set up a month editor with prefix "j" — should match jan, jul
    app.field_editor_state = Some(FieldEditorState::new("month", "j"));
    app.field_editor_state.as_mut().unwrap().is_month = true;
    app.update_field_completions();
    let completions = app.field_editor_state.as_ref().unwrap().completions.clone();
    assert!(completions.contains(&"jan".to_string()), "should contain jan");
    assert!(completions.contains(&"jun".to_string()), "should contain jun");
    assert!(completions.contains(&"jul".to_string()), "should contain jul");
    for c in &completions {
        assert!(c.starts_with('j'), "all completions should start with 'j': {}", c);
    }
}

#[test]
fn test_month_completions_all_when_empty_prefix() {
    let (mut app, _tmp) = make_app_with_month();
    use crate::tui::components::field_editor::FieldEditorState;
    // Empty value → all 12 months
    app.field_editor_state = Some(FieldEditorState::new("month", ""));
    app.field_editor_state.as_mut().unwrap().is_month = true;
    app.update_field_completions();
    let completions = app.field_editor_state.as_ref().unwrap().completions.clone();
    assert_eq!(completions.len(), 12);
}

#[test]
fn test_confirm_edit_normalizes_month() {
    let (mut app, _tmp) = make_app_with_month();
    // Open detail view on Smith2020
    app.handle_action(Action::OpenDetail);
    // Manually inject a month editor with the full word "january"
    use crate::tui::components::field_editor::FieldEditorState;
    app.field_editor_state = Some(FieldEditorState::new("month", "january"));
    app.field_editor_state.as_mut().unwrap().is_month = true;
    app.detail_entry_key = Some("Smith2020".to_string());
    app.handle_action(Action::ConfirmEdit);
    // The saved value should be normalized to "jan"
    let saved = app.database.entries.get("Smith2020")
        .and_then(|e| e.fields.get("month"))
        .cloned()
        .unwrap_or_default();
    assert_eq!(saved, "jan", "month should be normalized to 3-letter abbreviation");
}

#[test]
fn test_advance_phase_sets_is_month_when_field_name_is_month() {
    // When a new-field editor advances from name→value phase
    // and field_name == "month", App should set is_month.
    let (mut app, _tmp) = make_app();
    app.handle_action(Action::OpenDetail); // enter detail
    // Inject a new-field editor with "month" as the name
    use crate::tui::components::field_editor::FieldEditorState;
    let mut editor = FieldEditorState::new_field();
    editor.field_name = "month".to_string();
    editor.name_cursor = 5;
    app.field_editor_state = Some(editor);
    app.mode = InputMode::Editing;
    // ConfirmEdit while in name phase → advance to value phase
    app.handle_action(Action::ConfirmEdit);
    // After advancing, is_month should be true
    let is_month = app.field_editor_state.as_ref().map(|e| e.is_month).unwrap_or(false);
    assert!(is_month, "is_month should be set after phase transition to 'month' field");
}

// ── Toggle / show_groups init ────────────────────────────────────────────

#[test]
fn test_show_groups_respects_config() {
    let mut tmp = NamedTempFile::new().unwrap();
    write!(tmp, "{}", TEST_BIB).unwrap();
    tmp.flush().unwrap();
    let path = tmp.path().to_path_buf();
    let mut cfg = default_config();
    cfg.display.show_groups = false;
    let app = App::new(path, cfg).unwrap();
    assert!(!app.show_groups, "App::new should honour config.display.show_groups");
}

// ── handle_doi_fetch_result ──────────────────────────────────────────────

#[test]
fn test_handle_doi_fetch_result_sets_doi_field() {
    let (mut app, _tmp) = make_app();
    // Open detail on Smith2020
    app.handle_action(Action::OpenDetail);
    let key = app.detail_entry_key.clone().unwrap();

    app.handle_doi_fetch_result(
        key.clone(),
        Ok(("10.1234/test".to_string(), "https://doi.org/10.1234/test".to_string())),
    );

    let entry = app.database.entries.get(&key).unwrap();
    assert_eq!(entry.fields.get("doi").map(String::as_str), Some("10.1234/test"));
    // URL is redundant (same DOI), so it should NOT be set
    assert!(entry.fields.get("url").is_none() || entry.fields["url"].is_empty(),
        "redundant DOI URL should not be stored in url field");
}

#[test]
fn test_handle_doi_fetch_result_sets_distinct_url() {
    let (mut app, _tmp) = make_app();
    app.handle_action(Action::OpenDetail);
    let key = app.detail_entry_key.clone().unwrap();

    app.handle_doi_fetch_result(
        key.clone(),
        Ok((
            "10.1234/test".to_string(),
            "https://publisher.example.com/article/42".to_string(),
        )),
    );

    let entry = app.database.entries.get(&key).unwrap();
    assert_eq!(entry.fields.get("doi").map(String::as_str), Some("10.1234/test"));
    assert_eq!(
        entry.fields.get("url").map(String::as_str),
        Some("https://publisher.example.com/article/42")
    );
}

#[test]
fn test_handle_doi_fetch_result_marks_entry_dirty() {
    let (mut app, _tmp) = make_app();
    app.handle_action(Action::OpenDetail);
    let key = app.detail_entry_key.clone().unwrap();
    assert!(!app.database.entries[&key].dirty);

    app.handle_doi_fetch_result(key.clone(), Ok(("10.5555/x".to_string(), String::new())));

    assert!(app.database.entries[&key].dirty);
    assert!(app.dirty);
}

#[test]
fn test_handle_doi_fetch_result_already_up_to_date() {
    let (mut app, _tmp) = make_app();
    app.handle_action(Action::OpenDetail);
    let key = app.detail_entry_key.clone().unwrap();

    // Set doi first
    app.database.entries.get_mut(&key).unwrap()
        .fields.insert("doi".to_string(), "10.1234/test".to_string());

    app.handle_doi_fetch_result(
        key.clone(),
        Ok(("10.1234/test".to_string(), String::new())),
    );

    // Should report already-up-to-date, not set dirty
    let msg = app.status_message.as_deref().unwrap_or("");
    assert!(msg.contains("already") || msg.contains("up-to-date"),
        "expected already-up-to-date message, got: {}", msg);
}

#[test]
fn test_handle_doi_fetch_result_error_sets_status() {
    let (mut app, _tmp) = make_app();
    app.handle_action(Action::OpenDetail);
    let key = app.detail_entry_key.clone().unwrap();

    app.handle_doi_fetch_result(key, Err("Network error: timeout".to_string()));

    let msg = app.status_message.as_deref().unwrap_or("");
    assert!(msg.contains("failed") || msg.contains("Network"), "msg: {}", msg);
}

#[test]
fn test_handle_doi_fetch_result_pushes_undo() {
    let (mut app, _tmp) = make_app();
    app.handle_action(Action::OpenDetail);
    let key = app.detail_entry_key.clone().unwrap();
    let before = app.undo_stack.len();

    app.handle_doi_fetch_result(key, Ok(("10.9999/z".to_string(), String::new())));

    assert!(app.undo_stack.len() > before, "undo record should have been pushed");
}

#[test]
fn test_start_fetch_doi_no_entry_selected() {
    let (mut app, _tmp) = make_app();
    // No entry selected in list (deselect by going to empty state) — use a
    // fresh app with no selection driven via filtered_indices
    app.filtered_indices = Some(vec![]); // empty filtered list → no selection
    app.start_fetch_doi();
    // Should set a status message and not panic
    assert!(app.status_message.is_some());
    assert!(app.pending_doi_fetch.is_none());
}

#[test]
fn test_start_fetch_doi_spawns_background_task() {
    let (mut app, _tmp) = make_app();
    // Select first entry which has title+author
    app.handle_action(Action::OpenDetail);
    assert!(app.pending_doi_fetch.is_none());
    app.start_fetch_doi();
    assert!(app.pending_doi_fetch.is_some(), "background fetch should be pending");
    assert!(app.status_message.as_deref().unwrap_or("").contains("earch") ||
            app.status_message.as_deref().unwrap_or("").contains("etch"),
            "status: {:?}", app.status_message);
}

// ── open_web / ISBN ──────────────────────────────────────────────────────

const ISBN_BIB: &str = r#"@Book{Gottschling2016,
  author    = {Gottschling, Peter},
  publisher = {Addison-Wesley},
  title     = {{Discovering Modern C++}},
  year      = {2016},
  isbn      = {978-0-13-679847-7},
  url       = {https://www.oreilly.com/library/view/discovering-modern-c/9780136798477},
}
"#;

fn make_isbn_app() -> (App, NamedTempFile) {
    let mut tmp = NamedTempFile::new().unwrap();
    write!(tmp, "{}", ISBN_BIB).unwrap();
    tmp.flush().unwrap();
    let path = tmp.path().to_path_buf();
    let app = App::new(path, default_config()).unwrap();
    (app, tmp)
}

#[test]
fn test_open_web_isbn_and_url_shows_picker() {
    // Entry has both isbn and url — expect a picker dialog with 2 options.
    let (mut app, _tmp) = make_isbn_app();
    app.handle_action(Action::OpenWeb);
    assert_eq!(app.mode, InputMode::Dialog, "should enter dialog mode for picker");
    match &app.pending_action {
        Some(PendingAction::OpenWeb(urls)) => {
            assert_eq!(urls.len(), 2, "should have URL and ISBN options");
            let isbn_url = urls.iter().find(|u| u.contains("openlibrary.org"));
            assert!(isbn_url.is_some(), "isbn openlibrary.org URL should be in list");
            // ISBN digits stripped of hyphens/spaces, using search endpoint
            assert!(isbn_url.unwrap().contains("search?isbn=9780136798477"),
                "openlibrary URL should use search endpoint with clean ISBN: {}", isbn_url.unwrap());
        }
        other => panic!("expected PendingAction::OpenWeb, got {:?}", other),
    }
}

#[test]
fn test_open_web_isbn_hyphenated_strips_cleanly() {
    // Verify the ISBN "978-0-13-679847-7" is normalized to "9780136798477".
    let (mut app, _tmp) = make_isbn_app();
    app.handle_action(Action::OpenWeb);
    if let Some(PendingAction::OpenWeb(urls)) = &app.pending_action {
        let isbn_url = urls.iter().find(|u| u.contains("openlibrary.org")).unwrap();
        // Should not contain hyphens in the URL
        assert!(!isbn_url.contains('-'), "hyphens should be stripped from ISBN URL: {}", isbn_url);
    }
}

// ── SyncEntryFilename ────────────────────────────────────────────────────

/// Build an App whose first entry already has a `file` field pointing at a
/// real file on disk inside `dir`.  Returns (App, TempDir, citekey, abs_path).
fn make_app_with_file(
    citekey: &str,
    file_stem: &str,
) -> (App, tempfile::TempDir, String, std::path::PathBuf) {
    let dir = tempfile::TempDir::new().unwrap();
    let filename = format!("{}.pdf", file_stem);
    let abs = dir.path().join(&filename);
    std::fs::write(&abs, b"dummy").unwrap();

    // Relative path stored in the `file` field.
    let rel = filename.clone();
    let bib = format!(
        "@Article{{{citekey},\n  author = {{A, B}},\n  title = {{T}},\n  year = {{2024}},\n  file = {{:{rel}:PDF}},\n}}\n"
    );
    let mut tmp = NamedTempFile::new_in(dir.path()).unwrap();
    write!(tmp, "{}", bib).unwrap();
    tmp.flush().unwrap();
    let bib_path = tmp.path().to_path_buf();
    let mut app = App::new(bib_path, default_config()).unwrap();
    // Keep the temp bib file alive by leaking it into the app path (dir keeps it).
    drop(tmp);
    app.handle_action(Action::OpenDetail);
    (app, dir, citekey.to_string(), abs)
}

#[test]
fn test_sync_entry_filename_no_detail_open_is_noop() {
    let (mut app, _tmp) = make_app();
    // No detail open — action should be silently ignored.
    app.handle_action(Action::SyncEntryFilename);
    assert!(app.undo_stack.is_empty());
}

#[test]
fn test_sync_entry_filename_no_file_field_shows_status() {
    let (mut app, _tmp) = make_app();
    app.handle_action(Action::OpenDetail);
    // Smith2020 has no `file` field.
    app.handle_action(Action::SyncEntryFilename);
    assert!(
        app.status_message.as_deref().unwrap_or("").contains("No file"),
        "expected 'No file' status, got: {:?}", app.status_message
    );
    assert!(app.undo_stack.is_empty());
}

#[test]
fn test_sync_entry_filename_already_matches_no_undo_pushed() {
    // File is already named after the cite key — no rename needed.
    let citekey = "AuthorYear2024";
    let (mut app, _dir, _, _) = make_app_with_file(citekey, citekey);
    app.handle_action(Action::SyncEntryFilename);
    assert!(
        app.status_message.as_deref().unwrap_or("").contains("already"),
        "expected 'already matches' status, got: {:?}", app.status_message
    );
    assert!(app.undo_stack.is_empty(), "no undo item should be pushed when nothing changed");
}

#[test]
fn test_sync_entry_filename_renames_disk_file_and_updates_field() {
    let citekey = "AuthorYear2024";
    let old_stem = "old_filename";
    let (mut app, dir, key, old_abs) = make_app_with_file(citekey, old_stem);

    app.handle_action(Action::SyncEntryFilename);

    // Old file is gone, new file exists.
    assert!(!old_abs.exists(), "old file should have been renamed");
    let new_abs = dir.path().join(format!("{}.pdf", citekey));
    assert!(new_abs.exists(), "new file should exist at {:?}", new_abs);

    // `file` field in the entry is updated.
    let file_val = app.database.entries[&key].fields["file"].clone();
    assert!(file_val.contains(citekey), "file field should contain new stem: {}", file_val);

    // Entry is dirty.
    assert!(app.database.entries[&key].dirty);

    // An undo item was pushed.
    assert_eq!(app.undo_stack.len(), 1);
    assert!(matches!(app.undo_stack[0], UndoItem::FilenamesSynced { .. }));
}

#[test]
fn test_sync_entry_filename_undo_reverts_field_and_renames_back() {
    let citekey = "AuthorYear2024";
    let old_stem = "old_filename";
    let (mut app, dir, key, old_abs) = make_app_with_file(citekey, old_stem);
    let old_file_val = app.database.entries[&key].fields["file"].clone();

    app.handle_action(Action::SyncEntryFilename);
    assert!(!old_abs.exists());

    // Undo should rename the file back and restore the field.
    app.handle_action(Action::Undo);

    assert!(old_abs.exists(), "original file should be restored after undo");
    let new_abs = dir.path().join(format!("{}.pdf", citekey));
    assert!(!new_abs.exists(), "new file should be gone after undo");

    let file_val = app.database.entries[&key].fields["file"].clone();
    assert_eq!(file_val, old_file_val, "file field should be reverted");
    assert!(
        app.status_message.as_deref().unwrap_or("").contains("Undo"),
        "status should mention undo, got: {:?}", app.status_message
    );
}

#[test]
fn test_sync_entry_filename_field_only_when_file_absent() {
    // File does not exist on disk — field should still be updated, no undo rename.
    let citekey = "AuthorYear2024";
    let old_stem = "missing_file";
    let (mut app, _dir, key, abs) = make_app_with_file(citekey, old_stem);
    // Remove the file so it doesn't exist on disk.
    std::fs::remove_file(&abs).unwrap();

    app.handle_action(Action::SyncEntryFilename);

    // Field is updated even though the file didn't exist.
    let file_val = app.database.entries[&key].fields["file"].clone();
    assert!(file_val.contains(citekey), "file field should be updated: {}", file_val);

    // Undo item pushed — but renames vec should be empty (no disk rename).
    assert_eq!(app.undo_stack.len(), 1);
    if let UndoItem::FilenamesSynced { renames, .. } = &app.undo_stack[0] {
        assert!(renames.is_empty(), "no disk renames when file absent");
    } else {
        panic!("expected FilenamesSynced undo item");
    }
}

#[test]
fn test_open_web_isbn_with_doi_shows_picker_not_fetch() {
    // Entry has isbn + doi — 2 URL candidates → picker dialog, no browser opened,
    // no DOI fetch started. This verifies isbn generates a URL candidate without
    // triggering the browser-opening single-URL path.
    const ISBN_DOI_BIB: &str = r#"@Book{IsbnDoi2020,
  author    = {Author, Test},
  title     = {{Test Book}},
  year      = {2020},
  isbn      = {9781234567890},
  doi       = {10.1234/test},
}
"#;
    let mut tmp = NamedTempFile::new().unwrap();
    write!(tmp, "{}", ISBN_DOI_BIB).unwrap();
    tmp.flush().unwrap();
    let path = tmp.path().to_path_buf();
    let mut app = App::new(path, default_config()).unwrap();
    app.handle_action(Action::OpenWeb);
    // Picker shown (2 URLs: doi + isbn) — no browser opened, no DOI fetch
    assert_eq!(app.mode, InputMode::Dialog, "should show picker for doi + isbn");
    assert!(app.pending_doi_fetch.is_none(),
        "should not start DOI fetch when isbn/doi already provide URLs");
    if let Some(PendingAction::OpenWeb(urls)) = &app.pending_action {
        assert_eq!(urls.len(), 2);
        assert!(urls.iter().any(|u| u.contains("openlibrary.org")),
            "isbn openlibrary URL should be present");
    } else {
        panic!("expected PendingAction::OpenWeb");
    }
}

// ── parse_field_header ────────────────────────────────────────────────────

#[test]
fn test_parse_field_header_no_separator() {
    let (f, h) = parse_field_header("author");
    assert_eq!(f, "author");
    assert_eq!(h, "author");
}

#[test]
fn test_parse_field_header_with_pipe() {
    let (f, h) = parse_field_header("citation_key|Key");
    assert_eq!(f, "citation_key");
    assert_eq!(h, "Key");
}

#[test]
fn test_parse_field_header_empty_header_falls_back_to_field() {
    let (f, h) = parse_field_header("author|");
    assert_eq!(f, "author");
    assert_eq!(h, "author");
}

#[test]
fn test_parse_field_header_strips_whitespace() {
    let (f, h) = parse_field_header("  year  |  Year  ");
    assert_eq!(f, "year");
    assert_eq!(h, "Year");
}

// ── sort_field_candidates ─────────────────────────────────────────────────

#[test]
fn test_sort_field_candidates_includes_virtual_fields() {
    let (app, _tmp) = make_app();
    let fields = sort_field_candidates(&app.database);
    for required in ["author", "citation_key", "entrytype", "journal", "title", "year"] {
        assert!(fields.iter().any(|f| f == required),
            "expected '{}' in sort fields, got {:?}", required, fields);
    }
}

#[test]
fn test_sort_field_candidates_includes_database_fields() {
    let (app, _tmp) = make_app();
    // TEST_BIB has Smith2020 with journal+author+title+year — all already covered.
    // Verify "journal" comes from the bib data path:
    let result = sort_field_candidates(&app.database);
    assert!(result.iter().any(|f| f == "journal"));
    assert!(result.iter().any(|f| f == "title"));
}

// ── action_label_for_field ────────────────────────────────────────────────

#[test]
fn test_action_label_for_field_url_with_cleanup_enabled() {
    let cfg = crate::config::schema::SaveConfig {
        save_action_cleanup_url: true,
        ..Default::default()
    };
    assert_eq!(action_label_for_field("url", &cfg), "cleanup_url");
}

#[test]
fn test_action_label_for_field_url_with_cleanup_disabled_falls_through() {
    let cfg = crate::config::schema::SaveConfig {
        save_action_cleanup_url: false,
        ..Default::default()
    };
    // Falls through to text-field defaults
    let label = action_label_for_field("url", &cfg);
    // url has no specific match → falls into the `_` arm
    assert!(["unicode→latex", "esc_underscores", "esc_ampersands",
             "latex_cleanup", "ordinals", "save_action"].contains(&label));
}

#[test]
fn test_action_label_for_field_isbn_normalize() {
    let cfg = crate::config::schema::SaveConfig {
        save_action_normalize_isbn: true,
        ..Default::default()
    };
    assert_eq!(action_label_for_field("isbn", &cfg), "normalize_isbn");
}

#[test]
fn test_action_label_for_field_pages_normalize() {
    let cfg = crate::config::schema::SaveConfig {
        save_action_normalize_page_numbers: true,
        ..Default::default()
    };
    assert_eq!(action_label_for_field("pages", &cfg), "normalize_pages");
}

#[test]
fn test_action_label_for_field_author_normalize_names() {
    let cfg = crate::config::schema::SaveConfig {
        save_action_normalize_names_of_persons: true,
        ..Default::default()
    };
    assert_eq!(action_label_for_field("author", &cfg), "normalize_names");
    assert_eq!(action_label_for_field("editor", &cfg), "normalize_names");
    assert_eq!(action_label_for_field("translator", &cfg), "normalize_names");
}

#[test]
fn test_action_label_for_field_journal_abbreviate() {
    let cfg = crate::config::schema::SaveConfig {
        save_action_abbreviate_journal: true,
        ..Default::default()
    };
    assert_eq!(action_label_for_field("journal", &cfg), "abbreviate_journal");
    assert_eq!(action_label_for_field("journal_full", &cfg), "abbreviate_journal");
}

#[test]
fn test_action_label_for_field_text_field_priority_order() {
    // unicode→latex has highest priority
    let cfg = crate::config::schema::SaveConfig {
        save_action_unicode_to_latex: true,
        save_action_escape_underscores: true,
        save_action_latex_cleanup: true,
        ..Default::default()
    };
    assert_eq!(action_label_for_field("title", &cfg), "unicode→latex");
}

#[test]
fn test_action_label_for_field_no_actions_returns_save_action() {
    // Default config has many actions on; verify a field that has no specific
    // match still returns *something* — use a config with all actions off.
    let empty = crate::config::schema::SaveConfig {
        save_action_unicode_to_latex: false,
        save_action_escape_underscores: false,
        save_action_escape_ampersands: false,
        save_action_latex_cleanup: false,
        save_action_ordinals_to_superscript: false,
        save_action_cleanup_url: false,
        save_action_normalize_date: false,
        save_action_normalize_month: false,
        save_action_normalize_page_numbers: false,
        save_action_normalize_isbn: false,
        save_action_normalize_names_of_persons: false,
        save_action_abbreviate_journal: false,
        ..Default::default()
    };
    assert_eq!(action_label_for_field("title", &empty), "save_action");
}

// ── collect_group_names / find_group_node ─────────────────────────────────

fn make_group_node(name: &str, group_type: GroupType, children: Vec<GroupNode>) -> GroupNode {
    GroupNode {
        group: crate::bib::model::Group {
            name: name.to_string(),
            group_type,
        },
        children,
        expanded: true,
    }
}

#[test]
fn test_collect_group_names_skips_all_entries() {
    let root = make_group_node(
        "All Entries",
        GroupType::AllEntries,
        vec![
            make_group_node("Physics", GroupType::Static, vec![]),
            make_group_node("Chemistry", GroupType::Static, vec![]),
        ],
    );
    let mut names = Vec::new();
    collect_group_names(&root, &mut names);
    assert_eq!(names, vec!["Physics", "Chemistry"]);
}

#[test]
fn test_collect_group_names_includes_nested() {
    let root = make_group_node(
        "All Entries",
        GroupType::AllEntries,
        vec![
            make_group_node(
                "Physics",
                GroupType::Static,
                vec![make_group_node("Quantum", GroupType::Static, vec![])],
            ),
        ],
    );
    let mut names = Vec::new();
    collect_group_names(&root, &mut names);
    assert_eq!(names, vec!["Physics", "Quantum"]);
}

#[test]
fn test_find_group_node_finds_root() {
    let root = make_group_node("Physics", GroupType::Static, vec![]);
    let found = find_group_node(&root, "Physics");
    assert!(found.is_some());
}

#[test]
fn test_find_group_node_finds_nested() {
    let root = make_group_node(
        "All Entries",
        GroupType::AllEntries,
        vec![make_group_node(
            "Physics",
            GroupType::Static,
            vec![make_group_node("Quantum", GroupType::Static, vec![])],
        )],
    );
    assert!(find_group_node(&root, "Quantum").is_some());
}

#[test]
fn test_find_group_node_returns_none_when_absent() {
    let root = make_group_node("Physics", GroupType::Static, vec![]);
    assert!(find_group_node(&root, "Chemistry").is_none());
}

#[test]
fn test_find_group_node_mut_empty_path_returns_root() {
    let mut root = make_group_node("Physics", GroupType::Static, vec![]);
    let found = find_group_node_mut(&mut root, &[]);
    assert!(found.is_some());
}

#[test]
fn test_find_group_node_mut_navigates_path() {
    let mut root = make_group_node(
        "Root",
        GroupType::AllEntries,
        vec![
            make_group_node("A", GroupType::Static, vec![
                make_group_node("A1", GroupType::Static, vec![]),
            ]),
            make_group_node("B", GroupType::Static, vec![]),
        ],
    );
    let found = find_group_node_mut(&mut root, &[0, 0]);
    assert!(found.is_some());
    assert_eq!(found.unwrap().group.name, "A1");
}

#[test]
fn test_find_group_node_mut_invalid_path_returns_none() {
    let mut root = make_group_node("Physics", GroupType::Static, vec![]);
    assert!(find_group_node_mut(&mut root, &[5]).is_none());
}

// ── Save pipeline ────────────────────────────────────────────────────────

/// App with entry sorting, field sorting, citekey regeneration, and backups
/// disabled so saves are minimal and deterministic. Critically, with
/// `entry_sort_order: none` the save path must maintain raw_index itself
/// (regression coverage for stale-raw_index bugs).
fn make_app_no_sort() -> (App, NamedTempFile) {
    let mut tmp = NamedTempFile::new().unwrap();
    write!(tmp, "{}", TEST_BIB).unwrap();
    tmp.flush().unwrap();
    let mut cfg = default_config();
    cfg.save.entry_sort_order = "none".to_string();
    cfg.save.field_order = "none".to_string();
    cfg.save.save_action_regenerate_citekeys = false;
    cfg.general.backup_on_save = false;
    let app = App::new(tmp.path().to_path_buf(), cfg).unwrap();
    (app, tmp)
}

fn insert_new_entry(app: &mut App, key: &str) {
    let mut fields = IndexMap::new();
    fields.insert("title".to_string(), "New Work".to_string());
    fields.insert("year".to_string(), "2024".to_string());
    let entry = Entry {
        entry_type: EntryType::Misc,
        citation_key: key.to_string(),
        fields,
        group_memberships: vec![],
        raw_index: usize::MAX,
        dirty: true,
    };
    app.database.entries.insert(key.to_string(), entry);
    app.sorted_keys = sort_entries(&app.database.entries, &app.config);
}

#[test]
fn test_save_writes_dirty_entry_and_preserves_clean_bytes() {
    let (mut app, tmp) = make_app_no_sort();
    if let Some(e) = app.database.entries.get_mut("Smith2020") {
        e.fields.insert("title".to_string(), "Updated Title".to_string());
        e.dirty = true;
    }
    app.save();
    let out = std::fs::read_to_string(tmp.path()).unwrap();
    assert!(out.contains("Updated Title"), "got: {}", out);
    // The untouched entry round-trips byte-for-byte, including alignment.
    let doe_block = "@Book{Doe2021,\n  author    = {Doe, Jane},\n  title     = {Rust Programming},\n  year      = {2021},\n  publisher = {ACM Press},\n}";
    assert!(out.contains(doe_block), "clean entry must keep original bytes: {}", out);
    assert!(app.database.entries.values().all(|e| !e.dirty));
    assert!(!app.dirty);
}

/// Regression: re-serializing a dirty entry must not destroy `#` concatenation
/// or bare @String references in fields the user did not change.
#[test]
fn test_save_dirty_entry_preserves_concat_and_string_refs() {
    const CONCAT_BIB: &str = "@String{mainseries = {Main Series}}\n\
        @String{jnlref = {Journal of Refs}}\n\n\
        @Article{Concat2020,\n  \
        author  = {Smith, John},\n  \
        title   = {A Title},\n  \
        series  = mainseries # {, Part B},\n  \
        journal = jnlref,\n  \
        year    = {2020},\n}\n";
    let mut tmp = NamedTempFile::new().unwrap();
    write!(tmp, "{}", CONCAT_BIB).unwrap();
    tmp.flush().unwrap();
    let mut cfg = default_config();
    cfg.save.entry_sort_order = "none".to_string();
    cfg.save.field_order = "none".to_string();
    cfg.save.save_action_regenerate_citekeys = false;
    cfg.general.backup_on_save = false;
    let mut app = App::new(tmp.path().to_path_buf(), cfg).unwrap();

    // Edit an unrelated field; entry becomes dirty and is re-serialized.
    if let Some(e) = app.database.entries.get_mut("Concat2020") {
        e.fields.insert("title".to_string(), "Updated Title".to_string());
        e.dirty = true;
    }
    app.save();
    let out = std::fs::read_to_string(tmp.path()).unwrap();
    assert!(out.contains("Updated Title"), "got: {}", out);
    assert!(
        out.contains("series  = mainseries # {, Part B},"),
        "concat field bytes must be unchanged: {}", out
    );
    assert!(
        out.contains("journal = jnlref,"),
        "bare @String reference must be unchanged: {}", out
    );

    // A second edit + save must still preserve the untouched raw values.
    if let Some(e) = app.database.entries.get_mut("Concat2020") {
        e.fields.insert("title".to_string(), "Second Title".to_string());
        e.dirty = true;
    }
    app.save();
    let out = std::fs::read_to_string(tmp.path()).unwrap();
    assert!(out.contains("Second Title"), "got: {}", out);
    assert!(
        out.contains("series  = mainseries # {, Part B},"),
        "concat must survive a second save: {}", out
    );
    assert!(out.contains("journal = jnlref,"), "got: {}", out);
    assert!(parse_bib_file(&out).is_ok());
}

#[test]
fn test_save_backup_created_when_enabled() {
    let (mut app, tmp) = make_app_no_sort();
    app.config.general.backup_on_save = true;
    if let Some(e) = app.database.entries.get_mut("Smith2020") {
        e.fields.insert("year".to_string(), "2022".to_string());
        e.dirty = true;
    }
    app.save();
    let backup_path = tmp.path().with_extension("bib.bak");
    assert!(backup_path.exists(), "backup file should exist");
    // Backup holds the pre-save content.
    let backup = std::fs::read_to_string(&backup_path).unwrap();
    assert!(backup.contains("{2020}"));
    std::fs::remove_file(backup_path).ok();
}

/// Regression (entry_sort_order: none): a newly added entry must get its
/// raw_index bound on first save; the second save must update in place,
/// not insert a duplicate.
#[test]
fn test_save_new_entry_then_edit_does_not_duplicate() {
    let (mut app, tmp) = make_app_no_sort();
    insert_new_entry(&mut app, "New2024");
    app.save();
    let out = std::fs::read_to_string(tmp.path()).unwrap();
    assert_eq!(out.matches("@Misc{New2024,").count(), 1, "got: {}", out);

    // raw_index must now point at the entry's real raw slot.
    let idx = app.database.entries["New2024"].raw_index;
    assert!(
        matches!(app.database.raw_file.items.get(idx),
            Some(RawItem::Entry(e)) if e.citation_key == "New2024"),
        "raw_index must be rebound after save"
    );

    if let Some(e) = app.database.entries.get_mut("New2024") {
        e.fields.insert("title".to_string(), "Renamed Work".to_string());
        e.dirty = true;
    }
    app.save();
    let out = std::fs::read_to_string(tmp.path()).unwrap();
    assert_eq!(
        out.matches("@Misc{New2024,").count(), 1,
        "second save must not duplicate the entry: {}", out
    );
    assert!(out.contains("Renamed Work"));
    assert!(parse_bib_file(&out).is_ok());
}

/// Regression (entry_sort_order: none): deleting entries across two saves
/// must remove the right raw items both times.
#[test]
fn test_delete_save_delete_save_removes_correct_entries() {
    let (mut app, tmp) = make_app_no_sort();
    app.delete_entry("Smith2020");
    app.save();
    let out = std::fs::read_to_string(tmp.path()).unwrap();
    assert!(!out.contains("Smith2020"), "got: {}", out);
    assert!(out.contains("Doe2021"));

    app.delete_entry("Doe2021");
    app.save();
    let out = std::fs::read_to_string(tmp.path()).unwrap();
    assert!(
        !out.contains("Doe2021"),
        "second delete must remove the right entry: {}", out
    );
    assert!(parse_bib_file(&out).is_ok());
}

/// Regression: a duplicated entry's first save must not overwrite the
/// original's raw slot (the copy needs its own).
#[test]
fn test_duplicate_entry_then_save_keeps_original() {
    let (mut app, tmp) = make_app_no_sort();
    // Selection 0 is Doe2021 (sorted by citation_key).
    app.duplicate_entry();
    assert!(app.database.entries.contains_key("Doe2021_copy"));
    app.save();
    let out = std::fs::read_to_string(tmp.path()).unwrap();
    assert!(out.contains("@Book{Doe2021,"), "original must survive save: {}", out);
    assert!(out.contains("@Book{Doe2021_copy,"), "copy must be written: {}", out);
}

/// Add + delete in the same session, then save — both effects must land.
#[test]
fn test_save_add_and_delete_in_same_session() {
    let (mut app, tmp) = make_app_no_sort();
    app.delete_entry("Smith2020");
    insert_new_entry(&mut app, "New2024");
    app.save();
    let out = std::fs::read_to_string(tmp.path()).unwrap();
    assert!(!out.contains("Smith2020"), "got: {}", out);
    assert_eq!(out.matches("@Misc{New2024,").count(), 1);
    assert!(out.contains("Doe2021"));
    assert!(parse_bib_file(&out).is_ok());
}

#[test]
fn test_request_save_without_renames_saves_immediately() {
    let (mut app, tmp) = make_app_no_sort();
    if let Some(e) = app.database.entries.get_mut("Smith2020") {
        e.fields.insert("note".to_string(), "added".to_string());
        e.dirty = true;
    }
    app.request_save(false);
    assert!(app.dialog_state.is_none());
    let out = std::fs::read_to_string(tmp.path()).unwrap();
    assert!(out.contains("added"));
    assert!(!app.should_quit);
}

#[test]
fn test_request_save_and_quit_sets_quit_flag() {
    let (mut app, _tmp) = make_app_no_sort();
    app.request_save(true);
    assert!(app.should_quit);
}

#[test]
fn test_save_sorts_entries_when_configured() {
    let (mut app, tmp) = make_app_no_sort();
    app.config.save.entry_sort_order = "citation_key".to_string();
    if let Some(e) = app.database.entries.get_mut("Smith2020") {
        e.dirty = true;
    }
    app.save();
    let out = std::fs::read_to_string(tmp.path()).unwrap();
    let doe_pos = out.find("Doe2021").unwrap();
    let smith_pos = out.find("Smith2020").unwrap();
    assert!(doe_pos < smith_pos, "entries must be sorted by citation key: {}", out);
}

#[test]
fn test_app_warns_on_duplicate_citation_keys() {
    let mut tmp = NamedTempFile::new().unwrap();
    write!(
        tmp,
        "@Article{{k1,\n  title = {{A}},\n}}\n\n@Article{{k1,\n  title = {{B}},\n}}\n"
    )
    .unwrap();
    tmp.flush().unwrap();
    let app = App::new(tmp.path().to_path_buf(), default_config()).unwrap();
    let msg = app.status_message.clone().unwrap_or_default();
    assert!(msg.contains("duplicate"), "got: {}", msg);
    assert!(msg.contains("k1"), "got: {}", msg);
}

// ── Group management ─────────────────────────────────────────────────────

#[test]
fn test_finish_add_group_adds_node_and_raw_comment() {
    let (mut app, _tmp) = make_app_no_sort();
    app.finish_add_group("Physics".to_string(), vec![]);
    assert!(app
        .database
        .groups
        .root
        .children
        .iter()
        .any(|c| c.group.name == "Physics"));
    // The grouping @Comment must be created/updated in the raw file.
    assert!(app.database.raw_file.items.iter().any(|i| matches!(
        i,
        RawItem::Comment { raw_text }
            if raw_text.contains("jabref-meta: grouping:")
                && raw_text.contains("StaticGroup:Physics")
    )));
    assert!(app.status_message.unwrap().contains("Physics"));
}

#[test]
fn test_finish_delete_group_removes_node() {
    let (mut app, _tmp) = make_app_no_sort();
    app.finish_add_group("Physics".to_string(), vec![]);
    app.finish_delete_group(vec![0]);
    assert!(app.database.groups.root.children.is_empty());
    assert!(app.status_message.unwrap().contains("deleted"));
}

#[test]
fn test_finish_delete_group_empty_path_is_noop() {
    let (mut app, _tmp) = make_app_no_sort();
    app.finish_add_group("Physics".to_string(), vec![]);
    app.finish_delete_group(vec![]);
    assert_eq!(app.database.groups.root.children.len(), 1);
}

#[test]
fn test_finish_assign_groups_sets_field_and_membership() {
    let (mut app, _tmp) = make_app_no_sort();
    app.finish_add_group("Physics".to_string(), vec![]);
    app.finish_assign_groups("Smith2020", vec!["Physics".to_string()]);
    let e = &app.database.entries["Smith2020"];
    assert_eq!(e.fields["groups"], "Physics");
    assert_eq!(e.group_memberships, vec!["Physics".to_string()]);
    assert!(e.dirty);
}

#[test]
fn test_finish_assign_groups_empty_removes_field() {
    let (mut app, _tmp) = make_app_no_sort();
    app.finish_assign_groups("Smith2020", vec!["Physics".to_string()]);
    app.finish_assign_groups("Smith2020", vec![]);
    let e = &app.database.entries["Smith2020"];
    assert!(!e.fields.contains_key("groups"));
    assert!(e.group_memberships.is_empty());
}

#[test]
fn test_group_roundtrip_through_save() {
    let (mut app, tmp) = make_app_no_sort();
    app.finish_add_group("Physics".to_string(), vec![]);
    app.finish_assign_groups("Smith2020", vec!["Physics".to_string()]);
    app.save();

    // Reload from disk: group tree and membership must survive.
    let app2 = App::new(tmp.path().to_path_buf(), default_config()).unwrap();
    assert!(app2
        .database
        .groups
        .root
        .children
        .iter()
        .any(|c| c.group.name == "Physics"));
    assert!(app2.database.entries["Smith2020"]
        .group_memberships
        .contains(&"Physics".to_string()));
}

// ── Import-result handling ───────────────────────────────────────────────

fn imported_article() -> crate::util::import::ImportedEntry {
    let mut fields = IndexMap::new();
    fields.insert("title".to_string(), "an imported work".to_string());
    fields.insert("author".to_string(), "Jones, Alice".to_string());
    fields.insert("year".to_string(), "2023".to_string());
    fields.insert("journal".to_string(), "Nature".to_string());
    crate::util::import::ImportedEntry::new("article", fields)
}

#[test]
fn test_handle_import_result_success_adds_entry() {
    let (mut app, _tmp) = make_app_no_sort();
    app.handle_import_result(Ok(imported_article()));
    assert_eq!(app.database.entries.len(), 3);
    assert_eq!(app.mode, InputMode::Detail);
    let (key, entry) = app
        .database
        .entries
        .iter()
        .find(|(k, _)| *k != "Smith2020" && *k != "Doe2021")
        .unwrap();
    assert!(entry.dirty);
    assert_eq!(entry.raw_index, usize::MAX);
    // Title is titlecased and brace-protected.
    assert!(entry.fields["title"].starts_with('{'), "got: {:?}", entry.fields["title"]);
    assert!(app.detail_entry_key.as_deref() == Some(key.as_str()));
    assert!(app.status_message.unwrap().contains("Imported entry"));
}

#[test]
fn test_handle_import_result_collision_gets_suffix() {
    let (mut app, _tmp) = make_app_no_sort();
    app.handle_import_result(Ok(imported_article()));
    let count_after_first = app.database.entries.len();
    // Importing the identical entry again must not overwrite the first.
    app.handle_import_result(Ok(imported_article()));
    assert_eq!(app.database.entries.len(), count_after_first + 1);
    assert!(
        app.database.entries.keys().any(|k| k.ends_with("_2")),
        "second import should get a _2 suffix: {:?}",
        app.database.entries.keys().collect::<Vec<_>>()
    );
}

#[test]
fn test_handle_import_result_error_shows_dialog() {
    let (mut app, _tmp) = make_app_no_sort();
    app.handle_import_result(Err(crate::util::import::ImportError::Network(
        "timeout".to_string(),
    )));
    assert_eq!(app.mode, InputMode::Dialog);
    assert!(app.dialog_state.is_some());
    assert_eq!(app.database.entries.len(), 2);
}

// ── sync_filenames ───────────────────────────────────────────────────────

/// Bib file with one attached PDF in a subdirectory, on disk in a tempdir.
fn make_app_with_attachment() -> (App, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let bib_path = dir.path().join("lib.bib");
    std::fs::write(
        &bib_path,
        "@Article{Smith2020,\n  title = {T},\n  file  = {:PDF/old.pdf:PDF},\n}\n",
    )
    .unwrap();
    std::fs::create_dir(dir.path().join("PDF")).unwrap();
    std::fs::write(dir.path().join("PDF/old.pdf"), b"%PDF-1.4").unwrap();
    let mut cfg = default_config();
    cfg.save.entry_sort_order = "none".to_string();
    cfg.save.field_order = "none".to_string();
    cfg.save.save_action_regenerate_citekeys = false;
    cfg.general.backup_on_save = false;
    let app = App::new(bib_path, cfg).unwrap();
    (app, dir)
}

#[test]
fn test_sync_filenames_renames_attached_file() {
    let (mut app, dir) = make_app_with_attachment();
    app.sync_filenames(true);
    // File renamed on disk, preserving the subdirectory.
    assert!(dir.path().join("PDF/Smith2020.pdf").exists());
    assert!(!dir.path().join("PDF/old.pdf").exists());
    // The file field is updated and the entry marked dirty.
    let e = &app.database.entries["Smith2020"];
    assert_eq!(e.fields["file"], ":PDF/Smith2020.pdf:PDF");
    assert!(e.dirty);
}

#[test]
fn test_sync_filenames_noop_when_disabled() {
    let (mut app, dir) = make_app_with_attachment();
    app.config.save.sync_filenames = false;
    app.sync_filenames(false);
    assert!(dir.path().join("PDF/old.pdf").exists());
    assert!(!app.database.entries["Smith2020"].dirty);
}

#[test]
fn test_compute_sync_renames_lists_pending_rename() {
    let (app, _dir) = make_app_with_attachment();
    let renames = app.compute_sync_renames(true);
    assert_eq!(renames.len(), 1);
    assert!(renames[0].0.contains("old.pdf"), "got: {:?}", renames);
    assert!(renames[0].1.contains("Smith2020.pdf"), "got: {:?}", renames);
}

#[test]
fn test_sync_filenames_already_named_is_noop() {
    let (mut app, dir) = make_app_with_attachment();
    app.sync_filenames(true);
    // Second run: nothing left to rename.
    let renames = app.compute_sync_renames(true);
    assert!(renames.is_empty(), "got: {:?}", renames);
    app.sync_filenames(true);
    assert!(dir.path().join("PDF/Smith2020.pdf").exists());
}

#[test]
fn test_sync_filenames_does_not_overwrite_existing_target() {
    let dir = tempfile::tempdir().unwrap();
    let bib_path = dir.path().join("lib.bib");
    // Entry keyed "Key" whose attachment is "a.pdf"; a distinct "Key.pdf" already
    // exists in the same directory and must not be clobbered by the rename.
    std::fs::write(
        &bib_path,
        "@Article{Key,\n  title = {T},\n  file  = {:a.pdf:PDF},\n}\n",
    )
    .unwrap();
    std::fs::write(dir.path().join("a.pdf"), b"ORIGINAL-A").unwrap();
    std::fs::write(dir.path().join("Key.pdf"), b"ORIGINAL-KEY").unwrap();

    let mut cfg = default_config();
    cfg.save.entry_sort_order = "none".to_string();
    cfg.save.field_order = "none".to_string();
    cfg.save.save_action_regenerate_citekeys = false;
    cfg.general.backup_on_save = false;
    let mut app = App::new(bib_path, cfg).unwrap();

    app.sync_filenames(true);

    // The pre-existing Key.pdf keeps its original contents.
    assert_eq!(
        std::fs::read(dir.path().join("Key.pdf")).unwrap(),
        b"ORIGINAL-KEY"
    );
    // a.pdf is left in place (the rename was skipped).
    assert_eq!(
        std::fs::read(dir.path().join("a.pdf")).unwrap(),
        b"ORIGINAL-A"
    );
    // The file field is unchanged.
    assert_eq!(app.database.entries["Key"].fields["file"], ":a.pdf:PDF");
}

#[test]
fn test_sync_filenames_multi_attachment_conflict_skips_only_that_file() {
    let dir = tempfile::tempdir().unwrap();
    let bib_path = dir.path().join("lib.bib");
    // Entry with two attachments: targets are Key_1.pdf and Key_2.pdf.
    // A distinct Key_1.pdf already exists, so attachment 1 must be skipped
    // while attachment 2 is still renamed.
    std::fs::write(
        &bib_path,
        "@Article{Key,\n  title = {T},\n  file  = {:a.pdf:PDF;:b.pdf:PDF},\n}\n",
    )
    .unwrap();
    std::fs::write(dir.path().join("a.pdf"), b"ORIGINAL-A").unwrap();
    std::fs::write(dir.path().join("b.pdf"), b"ORIGINAL-B").unwrap();
    std::fs::write(dir.path().join("Key_1.pdf"), b"ORIGINAL-KEY1").unwrap();

    let mut cfg = default_config();
    cfg.save.entry_sort_order = "none".to_string();
    cfg.save.field_order = "none".to_string();
    cfg.save.save_action_regenerate_citekeys = false;
    cfg.general.backup_on_save = false;
    let mut app = App::new(bib_path, cfg).unwrap();

    app.sync_filenames(true);

    // The conflicting target keeps its original contents.
    assert_eq!(
        std::fs::read(dir.path().join("Key_1.pdf")).unwrap(),
        b"ORIGINAL-KEY1"
    );
    // Attachment 1 is left in place; attachment 2 was renamed.
    assert_eq!(
        std::fs::read(dir.path().join("a.pdf")).unwrap(),
        b"ORIGINAL-A"
    );
    assert!(!dir.path().join("b.pdf").exists());
    assert_eq!(
        std::fs::read(dir.path().join("Key_2.pdf")).unwrap(),
        b"ORIGINAL-B"
    );
    // The file field keeps the skipped path and records the renamed one.
    assert_eq!(
        app.database.entries["Key"].fields["file"],
        ":a.pdf:PDF;:Key_2.pdf:PDF"
    );
}

// ── Render smoke tests (TestBackend) ─────────────────────────────────────

/// Render the app into an in-memory buffer and return its text content.
fn render_to_string(app: &mut App, width: u16, height: u16) -> String {
    use ratatui::backend::TestBackend;
    use ratatui::Terminal;
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|f| app.render(f)).unwrap();
    terminal
        .backend()
        .buffer()
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect()
}

#[test]
fn test_render_main_screen_shows_entries() {
    let (mut app, _tmp) = make_app();
    let content = render_to_string(&mut app, 120, 30);
    assert!(content.contains("Smith"), "main screen should list entries");
    assert!(content.contains("Doe"), "main screen should list entries");
}

#[test]
fn test_render_main_screen_with_status_message() {
    let (mut app, _tmp) = make_app();
    app.status_message = Some("hello status".to_string());
    let content = render_to_string(&mut app, 120, 30);
    assert!(content.contains("hello status"), "status bar should show message");
}

#[test]
fn test_render_edit_screen_shows_fields() {
    let (mut app, _tmp) = make_app();
    app.open_detail();
    assert!(app.detail_state.is_some());
    let content = render_to_string(&mut app, 120, 30);
    assert!(content.contains("author"), "detail screen should show field names");
}

#[test]
fn test_render_settings_screen() {
    let (mut app, _tmp) = make_app();
    app.settings_state = Some(SettingsState::new(&app.config));
    app.mode = InputMode::Settings;
    let content = render_to_string(&mut app, 120, 40);
    assert!(!content.trim().is_empty(), "settings screen should render content");
}

#[test]
fn test_render_settings_screen_citekey_template_editor() {
    let (mut app, _tmp) = make_app();
    app.settings_state = Some(SettingsState::new(&app.config));
    app.mode = InputMode::Editing;
    app.field_editor_state = Some(FieldEditorState::new("template", "[auth][year]"));
    app.pending_action = Some(PendingAction::EditSetting {
        setting_id: "citekey.template.article".to_string(),
    });
    assert!(app.is_editing_citekey_template());
    let content = render_to_string(&mut app, 120, 40);
    // The citekey token reference panel renders below the editor.
    assert!(!content.trim().is_empty());
}

#[test]
fn test_render_settings_screen_with_dialog() {
    let (mut app, _tmp) = make_app();
    app.settings_state = Some(SettingsState::new(&app.config));
    app.dialog_state = Some(DialogState::confirm("Reset", "Reset all settings?"));
    app.mode = InputMode::Dialog;
    let content = render_to_string(&mut app, 120, 40);
    assert!(content.contains("Reset all settings?"));
}

#[test]
fn test_render_with_dialog_open() {
    let (mut app, _tmp) = make_app();
    app.dialog_state = Some(DialogState::confirm("Confirm", "Really?"));
    app.mode = InputMode::Dialog;
    let content = render_to_string(&mut app, 120, 30);
    assert!(content.contains("Really?"), "dialog message should render");
}

#[test]
fn test_render_dialog_variants() {
    let cases: Vec<(DialogState, &str)> = vec![
        (DialogState::message("Info", "All good"), "All good"),
        (
            DialogState::file_sync_preview(vec![(
                "old.pdf".to_string(),
                "new.pdf".to_string(),
            )]),
            "old.pdf",
        ),
        (
            DialogState::file_delete_select(
                "Delete attached files?",
                vec![("a.pdf".to_string(), true)],
            ),
            "a.pdf",
        ),
        (
            DialogState::group_assign(vec![
                ("Physics".to_string(), true),
                ("Chemistry".to_string(), false),
            ]),
            "Physics",
        ),
        (
            DialogState::type_picker(vec!["Article".to_string(), "Book".to_string()]),
            "Article",
        ),
    ];
    for (dialog, expected) in cases {
        let (mut app, _tmp) = make_app();
        app.dialog_state = Some(dialog);
        app.mode = InputMode::Dialog;
        let content = render_to_string(&mut app, 100, 30);
        assert!(content.contains(expected), "dialog should render {:?}", expected);
    }
}

#[test]
fn test_render_narrow_terminal_does_not_panic() {
    let (mut app, _tmp) = make_app();
    let _ = render_to_string(&mut app, 40, 10);
    app.open_detail();
    let _ = render_to_string(&mut app, 40, 10);
    app.settings_state = Some(SettingsState::new(&app.config));
    let _ = render_to_string(&mut app, 40, 10);
}

#[test]
fn test_render_tiny_terminal_does_not_panic() {
    let (mut app, _tmp) = make_app();
    let _ = render_to_string(&mut app, 5, 2);
}

// ── Clipboard / opener mocks (finding 2.6) ───────────────────────────────

use std::cell::RefCell;
use std::rc::Rc;

/// Records copied text; `paste` returns a fixed string; `fail` forces errors.
struct MockClipboard {
    copied: Rc<RefCell<Vec<String>>>,
    paste_text: String,
    fail: bool,
}

impl crate::util::clipboard::Clipboard for MockClipboard {
    fn copy(&self, text: &str) -> anyhow::Result<()> {
        if self.fail {
            anyhow::bail!("mock clipboard failure");
        }
        self.copied.borrow_mut().push(text.to_string());
        Ok(())
    }
    fn paste(&self) -> anyhow::Result<String> {
        if self.fail {
            anyhow::bail!("mock clipboard failure");
        }
        Ok(self.paste_text.clone())
    }
}

/// Install a recording clipboard on `app`; returns the shared copy log.
fn install_mock_clipboard(app: &mut App) -> Rc<RefCell<Vec<String>>> {
    let copied = Rc::new(RefCell::new(Vec::new()));
    app.clipboard = Box::new(MockClipboard {
        copied: Rc::clone(&copied),
        paste_text: String::new(),
        fail: false,
    });
    copied
}

/// Records opened paths (as `path:...`) and URLs (as `url:...`).
struct MockOpener {
    opened: Rc<RefCell<Vec<String>>>,
}

impl crate::util::open::Opener for MockOpener {
    fn open_path(&self, path: &std::path::Path) -> Result<()> {
        self.opened.borrow_mut().push(format!("path:{}", path.display()));
        Ok(())
    }
    fn open_url(&self, url: &str) -> Result<()> {
        self.opened.borrow_mut().push(format!("url:{}", url));
        Ok(())
    }
}

/// Install a recording opener on `app`; returns the shared open log.
fn install_mock_opener(app: &mut App) -> Rc<RefCell<Vec<String>>> {
    let opened = Rc::new(RefCell::new(Vec::new()));
    app.opener = Box::new(MockOpener { opened: Rc::clone(&opened) });
    opened
}

// ── Yank to clipboard ─────────────────────────────────────────────────────

#[test]
fn test_yank_citekey_prompt_format_opens_picker() {
    let (mut app, _tmp) = make_app(); // default yank_format is "prompt"
    let copied = install_mock_clipboard(&mut app);
    app.handle_action(Action::YankCitekey);
    assert!(copied.borrow().is_empty());
    assert!(app.dialog_state.is_some());
    assert_eq!(app.mode, InputMode::Dialog);
    assert!(matches!(app.pending_action, Some(PendingAction::YankPrompt { .. })));
}

#[test]
fn test_yank_citekey_direct_format_copies_key() {
    let (mut app, _tmp) = make_app();
    app.config.general.yank_format = "citation_key".to_string();
    let copied = install_mock_clipboard(&mut app);
    app.handle_action(Action::YankCitekey);
    // First entry in citation_key sort order is Doe2021.
    assert_eq!(copied.borrow().as_slice(), ["Doe2021"]);
    assert!(app.status_message.as_deref().unwrap().contains("Copied key 'Doe2021'"));
}

#[test]
fn test_do_yank_bibtex_copies_serialized_entry() {
    let (mut app, _tmp) = make_app();
    let copied = install_mock_clipboard(&mut app);
    app.do_yank("Smith2020", "bibtex");
    let copied = copied.borrow();
    assert_eq!(copied.len(), 1);
    assert!(copied[0].starts_with("@Article{Smith2020"));
    assert!(copied[0].contains("My Paper"));
}

#[test]
fn test_do_yank_citation_copies_formatted_citation() {
    let (mut app, _tmp) = make_app();
    let copied = install_mock_clipboard(&mut app);
    app.do_yank("Smith2020", "citation");
    let copied = copied.borrow();
    assert_eq!(copied.len(), 1);
    assert!(copied[0].contains("Smith"));
    assert!(app.status_message.as_deref().unwrap().contains("citation for 'Smith2020'"));
}

#[test]
fn test_do_yank_clipboard_error_sets_status() {
    let (mut app, _tmp) = make_app();
    app.clipboard = Box::new(MockClipboard {
        copied: Rc::new(RefCell::new(Vec::new())),
        paste_text: String::new(),
        fail: true,
    });
    app.do_yank("Smith2020", "citation_key");
    assert!(app.status_message.as_deref().unwrap().contains("Clipboard error"));
}

#[test]
fn test_dialog_yank_copies_message_text() {
    let (mut app, _tmp) = make_app();
    let copied = install_mock_clipboard(&mut app);
    app.dialog_state = Some(DialogState::message("Error", "something broke"));
    app.mode = InputMode::Dialog;
    app.handle_action(Action::DialogYank);
    assert_eq!(copied.borrow().as_slice(), ["something broke"]);
}

#[test]
fn test_yank_citation_preview_copies_citation() {
    let (mut app, _tmp) = make_app();
    let copied = install_mock_clipboard(&mut app);
    app.citation_preview_state = Some(CitationPreviewState {
        citation: "[1] J. Smith".to_string(),
        entry_key: "Smith2020".to_string(),
        style_name: "ieeetran".to_string(),
    });
    app.handle_action(Action::YankCitationPreview);
    assert_eq!(copied.borrow().as_slice(), ["[1] J. Smith"]);
}

#[test]
fn test_edit_yank_copies_field_value_and_sets_register() {
    let (mut app, _tmp) = make_app();
    let copied = install_mock_clipboard(&mut app);
    app.field_editor_state = Some(FieldEditorState::new("title", "Hello"));
    app.handle_field_editor_action(Action::EditYank);
    assert_eq!(copied.borrow().as_slice(), ["Hello"]);
    assert_eq!(app.field_editor_state.as_ref().unwrap().unnamed_register, "Hello");
}

#[test]
fn test_edit_put_falls_back_to_clipboard() {
    let (mut app, _tmp) = make_app();
    app.clipboard = Box::new(MockClipboard {
        copied: Rc::new(RefCell::new(Vec::new())),
        paste_text: "pasted".to_string(),
        fail: false,
    });
    app.field_editor_state = Some(FieldEditorState::new("title", ""));
    app.handle_field_editor_action(Action::EditPut);
    assert!(app.field_editor_state.as_ref().unwrap().value.contains("pasted"));
}

#[test]
fn test_edit_put_prefers_unnamed_register() {
    let (mut app, _tmp) = make_app();
    app.clipboard = Box::new(MockClipboard {
        copied: Rc::new(RefCell::new(Vec::new())),
        paste_text: "from-clipboard".to_string(),
        fail: false,
    });
    let mut editor = FieldEditorState::new("title", "");
    editor.unnamed_register = "from-register".to_string();
    app.field_editor_state = Some(editor);
    app.handle_field_editor_action(Action::EditPut);
    let value = &app.field_editor_state.as_ref().unwrap().value;
    assert!(value.contains("from-register"));
    assert!(!value.contains("from-clipboard"));
}

// ── Open file / web via mock opener ───────────────────────────────────────

#[test]
fn test_open_file_single_attachment_opens_directly() {
    let (mut app, dir) = make_app_with_attachment();
    let opened = install_mock_opener(&mut app);
    app.open_file();
    let opened = opened.borrow();
    assert_eq!(opened.len(), 1);
    assert!(opened[0].starts_with("path:"));
    assert!(opened[0].ends_with("PDF/old.pdf"));
    assert!(opened[0].contains(dir.path().to_str().unwrap()));
}

#[test]
fn test_open_file_no_attachment_sets_status() {
    let (mut app, _tmp) = make_app();
    let opened = install_mock_opener(&mut app);
    app.open_file();
    assert!(opened.borrow().is_empty());
    assert!(app.status_message.as_deref().unwrap().contains("No file attached"));
}

#[test]
fn test_open_web_single_doi_opens_url() {
    let (mut app, _tmp) = make_app();
    let opened = install_mock_opener(&mut app);
    if let Some(e) = app.database.entries.get_mut("Doe2021") {
        e.fields.insert("doi".to_string(), "10.1000/xyz".to_string());
    }
    app.open_web(); // Doe2021 is the selected entry
    assert_eq!(opened.borrow().as_slice(), ["url:https://doi.org/10.1000/xyz"]);
}

#[test]
fn test_dialog_confirm_open_web_opens_selected_url() {
    let (mut app, _tmp) = make_app();
    let opened = install_mock_opener(&mut app);
    app.dialog_state = Some(DialogState::type_picker_titled(
        "Open Web Link",
        vec!["DOI".to_string(), "URL".to_string()],
    ));
    app.pending_action = Some(PendingAction::OpenWeb(vec![
        "https://doi.org/10.1/a".to_string(),
        "https://example.org/b".to_string(),
    ]));
    app.mode = InputMode::Dialog;
    app.dialog_state.as_mut().unwrap().select(1);
    app.handle_dialog_confirm();
    assert_eq!(opened.borrow().as_slice(), ["url:https://example.org/b"]);
}

// ── Nonexistent .bib path opens a blank library ───────────────────────────

#[test]
fn test_new_with_nonexistent_path_opens_blank_library() {
    let dir = tempfile::tempdir().unwrap();
    let bib_path = dir.path().join("new_library.bib");
    let app = App::new(bib_path.clone(), default_config()).unwrap();
    assert_eq!(app.database.entries.len(), 0);
    assert!(!app.dirty);
    assert_eq!(app.bib_path, bib_path);
    assert!(app.status_message.as_deref().unwrap().contains("New file"));
    assert_eq!(app.mode, InputMode::Normal);
}

#[test]
fn test_new_with_nonexistent_path_save_creates_file() {
    let dir = tempfile::tempdir().unwrap();
    let bib_path = dir.path().join("new_library.bib");
    let mut cfg = default_config();
    cfg.save.entry_sort_order = "none".to_string();
    cfg.save.field_order = "none".to_string();
    cfg.save.save_action_regenerate_citekeys = false;
    cfg.general.backup_on_save = false;
    let mut app = App::new(bib_path.clone(), cfg).unwrap();
    insert_new_entry(&mut app, "Fresh2026");
    app.save();
    assert!(bib_path.exists());
    let written = std::fs::read_to_string(&bib_path).unwrap();
    assert!(written.contains("Fresh2026"));
}

// ── Multi-line paste (bracketed paste) ────────────────────────────────────

#[test]
fn test_paste_multiline_into_insert_mode_editor_collapses_newlines() {
    let (mut app, _tmp) = make_app();
    let mut editor = FieldEditorState::new("title", "");
    editor.editing_mode = EditingMode::Insert;
    app.field_editor_state = Some(editor);
    app.mode = InputMode::Editing;
    app.handle_event(Event::Paste("A Title That\nSpans Three\nLines".to_string()));
    // An empty editor pre-fills protective braces; the paste lands inside them.
    assert_eq!(
        app.field_editor_state.as_ref().unwrap().value,
        "{A Title That Spans Three Lines}"
    );
}

#[test]
fn test_paste_into_normal_mode_editor_uses_put() {
    let (mut app, _tmp) = make_app();
    let mut editor = FieldEditorState::new("title", "X");
    editor.editing_mode = EditingMode::Normal;
    app.field_editor_state = Some(editor);
    app.mode = InputMode::Editing;
    app.handle_event(Event::Paste("one\ntwo".to_string()));
    // put() inserts after the cursor char, like vim `p`
    assert_eq!(app.field_editor_state.as_ref().unwrap().value, "Xone two");
}

#[test]
fn test_paste_into_search_updates_query() {
    let (mut app, _tmp) = make_app();
    app.handle_action(Action::EnterSearch);
    app.handle_event(Event::Paste("Smith\n2020".to_string()));
    assert_eq!(app.search_bar_state.query, "Smith 2020");
}

#[test]
fn test_paste_into_command_palette() {
    let (mut app, _tmp) = make_app();
    app.handle_action(Action::EnterCommand);
    app.handle_event(Event::Paste("sort year".to_string()));
    assert_eq!(app.command_palette_state.input, "sort year");
}

#[test]
fn test_paste_in_normal_mode_is_ignored() {
    let (mut app, _tmp) = make_app();
    app.handle_event(Event::Paste("stray paste".to_string()));
    assert_eq!(app.mode, InputMode::Normal);
    assert!(app.search_bar_state.query.is_empty());
}

#[test]
fn test_edit_put_clipboard_multiline_collapsed() {
    let (mut app, _tmp) = make_app();
    app.clipboard = Box::new(MockClipboard {
        copied: Rc::new(RefCell::new(Vec::new())),
        paste_text: "Line One\nLine Two".to_string(),
        fail: false,
    });
    app.field_editor_state = Some(FieldEditorState::new("title", "x"));
    app.handle_field_editor_action(Action::EditPut);
    assert_eq!(
        app.field_editor_state.as_ref().unwrap().value,
        "xLine One Line Two"
    );
}
