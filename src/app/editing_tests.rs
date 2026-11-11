//! App-level tests for `app/editing.rs`: the `confirm_edit` dispatch arms,
//! file-attachment add/edit/delete, field deletion, title-casing, name
//! normalization, and the vim action handler as routed through `App`.

use super::*;
use crate::config::defaults::default_config;
use crate::tui::components::entry_detail::DisplayItem;
use crate::tui::components::settings::SettingsState;
use crate::tui::keybindings::InputMode;
use std::io::Write;

const BIB: &str = r#"@Article{Article_2020_N_SmithDoe,
  author  = {John Smith and Jane Doe},
  title   = {a study of things},
  year    = {2020},
  journal = {Nature},
  note    = {draft},
  file    = {:PDF/one.pdf:PDF;:PDF/two.pdf:PDF},
}
"#;

/// The citation key matches what the default template generates, so field
/// edits do not trigger a key rename (which would add an extra undo step).
/// Build an App in its own temp directory (so relative file paths resolve
/// predictably) with auto-saving side effects disabled.
fn make_app() -> (App, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    // Canonicalize so paths match `make_relative` (macOS: /var → /private/var).
    let path = dir.path().canonicalize().unwrap().join("lib.bib");
    let mut f = std::fs::File::create(&path).unwrap();
    write!(f, "{}", BIB).unwrap();
    let mut cfg = default_config();
    cfg.general.backup_on_save = false;
    cfg.save.save_action_regenerate_citekeys = false;
    let app = App::new(path, cfg).unwrap();
    (app, dir)
}

/// Open the detail view on the only entry.
fn open_detail(app: &mut App) {
    app.handle_action(Action::OpenDetail);
    assert_eq!(app.mode, InputMode::Detail);
}

/// Select the detail row for field `name`.
fn select_field(app: &mut App, name: &str) {
    let detail = app.detail_state.as_mut().unwrap();
    let idx = detail
        .display_fields
        .iter()
        .position(|d| matches!(d, DisplayItem::Field { name: n, .. } if n == name))
        .unwrap_or_else(|| panic!("no field row {name}"));
    detail.select(idx);
}

/// Select the `file_idx`-th file attachment row.
fn select_file(app: &mut App, file_idx: usize) {
    let detail = app.detail_state.as_mut().unwrap();
    let idx = detail
        .display_fields
        .iter()
        .position(|d| matches!(d, DisplayItem::FileEntry { index, .. } if *index == file_idx))
        .unwrap_or_else(|| panic!("no file row {file_idx}"));
    detail.select(idx);
}

fn entry_field(app: &App, name: &str) -> Option<String> {
    let key = app.detail_entry_key.as_ref().unwrap();
    app.database.entries[key].fields.get(name).cloned()
}

fn set_editor_value(app: &mut App, value: &str) {
    let e = app.field_editor_state.as_mut().unwrap();
    e.value = value.to_string();
    e.cursor = value.len();
}

/// Put `value` in a fresh editor tied to `pending` and confirm it.
fn confirm_with(app: &mut App, pending: PendingAction, value: &str) {
    app.field_editor_state = Some(FieldEditorState::for_path("x", value));
    app.pending_action = Some(pending);
    app.mode = InputMode::Editing;
    app.handle_action(Action::ConfirmEdit);
}

// ── Field edit / delete ─────────────────────────────────────────────────

#[test]
fn edit_field_commits_value_and_undo_restores_it() {
    let (mut app, _d) = make_app();
    open_detail(&mut app);
    select_field(&mut app, "note");
    app.handle_action(Action::EditField);
    assert_eq!(app.mode, InputMode::Editing);
    set_editor_value(&mut app, "final");
    app.handle_action(Action::ConfirmEdit);
    assert_eq!(app.mode, InputMode::Detail);
    assert_eq!(entry_field(&app, "note").as_deref(), Some("final"));

    app.handle_action(Action::Undo);
    assert_eq!(entry_field(&app, "note").as_deref(), Some("draft"));
}

#[test]
fn edit_field_back_to_original_clears_dirty() {
    let (mut app, _d) = make_app();
    open_detail(&mut app);
    select_field(&mut app, "note");
    app.handle_action(Action::EditField);
    set_editor_value(&mut app, "final");
    app.handle_action(Action::ConfirmEdit);
    let key = app.detail_entry_key.clone().unwrap();
    assert!(app.database.entries[&key].dirty);

    select_field(&mut app, "note");
    app.handle_action(Action::EditField);
    set_editor_value(&mut app, "draft");
    app.handle_action(Action::ConfirmEdit);
    let key = app.detail_entry_key.clone().unwrap();
    assert!(
        !app.database.entries[&key].dirty,
        "{:?}",
        app.database.entries[&key]
    );
}

#[test]
fn new_field_two_phase_month_is_normalized() {
    let (mut app, _d) = make_app();
    open_detail(&mut app);
    app.field_editor_state = Some(FieldEditorState::new_field());
    app.mode = InputMode::Editing;
    {
        let e = app.field_editor_state.as_mut().unwrap();
        e.field_name = "month".to_string();
        e.name_cursor = 5;
    }
    // Phase 1: confirm the name; stays in the editor with month mode on.
    app.handle_action(Action::ConfirmEdit);
    assert_eq!(app.mode, InputMode::Editing);
    assert!(app.field_editor_state.as_ref().unwrap().is_month);
    // Phase 2: commit the value.
    set_editor_value(&mut app, "April");
    app.handle_action(Action::ConfirmEdit);
    assert_eq!(entry_field(&app, "month").as_deref(), Some("apr"));
}

#[test]
fn new_field_with_empty_name_is_aborted() {
    let (mut app, _d) = make_app();
    open_detail(&mut app);
    let before = app.database.entries.values().next().unwrap().fields.len();
    let mut e = FieldEditorState::new_field();
    // Skip the name phase so the empty name reaches the commit path.
    e.advance_phase();
    app.field_editor_state = Some(e);
    app.mode = InputMode::Editing;
    app.handle_action(Action::ConfirmEdit);
    assert_eq!(app.mode, InputMode::Detail);
    let after = app.database.entries.values().next().unwrap().fields.len();
    assert_eq!(before, after);
}

// ── N03: syntax validation of edits ─────────────────────────────────────

#[test]
fn edit_with_unbalanced_brace_keeps_editor_open_and_entry_unchanged() {
    let (mut app, _d) = make_app();
    open_detail(&mut app);
    select_field(&mut app, "note");
    app.handle_action(Action::EditField);
    for bad in ["Unclosed {", "stray }", r"trailing \"] {
        set_editor_value(&mut app, bad);
        app.handle_action(Action::ConfirmEdit);
        assert_eq!(app.mode, InputMode::Editing, "{bad:?}");
        assert_eq!(app.field_editor_state.as_ref().unwrap().value, bad);
        assert_eq!(entry_field(&app, "note").as_deref(), Some("draft"));
        assert!(
            app.status_message
                .as_deref()
                .unwrap()
                .starts_with("Not saved"),
            "{:?}",
            app.status_message
        );
    }
    // The draft can then be fixed and committed.
    set_editor_value(&mut app, r"{Fixed} \{literal\}");
    app.handle_action(Action::ConfirmEdit);
    assert_eq!(app.mode, InputMode::Detail);
    assert_eq!(
        entry_field(&app, "note").as_deref(),
        Some(r"{Fixed} \{literal\}")
    );
}

#[test]
fn new_field_name_is_validated_and_lowercased() {
    let (mut app, _d) = make_app();
    open_detail(&mut app);
    app.field_editor_state = Some(FieldEditorState::new_field());
    app.mode = InputMode::Editing;
    for bad in ["two words", "a=b", "x,y", "{t}"] {
        {
            let e = app.field_editor_state.as_mut().unwrap();
            e.field_name = bad.to_string();
            e.name_cursor = bad.len();
        }
        app.handle_action(Action::ConfirmEdit);
        let e = app.field_editor_state.as_ref().unwrap();
        assert!(e.editing_name, "{bad:?} must stay in the name phase");
        assert!(app
            .status_message
            .as_deref()
            .unwrap()
            .starts_with("Invalid field name"));
    }
    {
        let e = app.field_editor_state.as_mut().unwrap();
        e.field_name = "Annote".to_string();
        e.name_cursor = 6;
    }
    app.handle_action(Action::ConfirmEdit);
    set_editor_value(&mut app, "text");
    app.handle_action(Action::ConfirmEdit);
    assert_eq!(entry_field(&app, "annote").as_deref(), Some("text"));
}

#[test]
fn edit_save_reload_preserves_entries_and_values() {
    let (mut app, _d) = make_app();
    open_detail(&mut app);
    select_field(&mut app, "note");
    app.handle_action(Action::EditField);
    set_editor_value(&mut app, r"{DNA} {a {nested} group} 50\% \{x\}");
    app.handle_action(Action::ConfirmEdit);
    assert!(app.save(), "{:?}", app.status_message);
    let expected = app.database.entries.clone();
    let reloaded = App::new(app.bib_path.clone(), app.config.clone()).unwrap();
    assert_eq!(
        reloaded.database.entries.keys().collect::<Vec<_>>(),
        expected.keys().collect::<Vec<_>>()
    );
    for (key, entry) in &expected {
        assert_eq!(reloaded.database.entries[key].fields, entry.fields, "{key}");
    }
}

#[test]
fn save_refuses_output_that_would_not_reload() {
    let (mut app, _d) = make_app();
    let before = std::fs::read(&app.bib_path).unwrap();
    // A value that bypasses the editor (e.g. a programmatic change).
    let key = app.database.entries.keys().next().unwrap().clone();
    let entry = app.database.entries.get_mut(&key).unwrap();
    entry.fields.insert("note".into(), "Unclosed {".into());
    entry.dirty = true;
    app.dirty = true;
    assert!(!app.save());
    assert!(
        app.status_message
            .as_deref()
            .unwrap()
            .contains("nothing was written"),
        "{:?}",
        app.status_message
    );
    assert_eq!(std::fs::read(&app.bib_path).unwrap(), before);
    assert!(app.dirty);
    assert!(app.database.entries[&key].dirty);
    assert_eq!(app.database.entries[&key].fields["note"], "Unclosed {");
}

#[test]
fn delete_field_removes_it_and_undo_restores() {
    let (mut app, _d) = make_app();
    open_detail(&mut app);
    select_field(&mut app, "journal");
    app.handle_action(Action::DeleteField);
    assert_eq!(entry_field(&app, "journal"), None);
    app.handle_action(Action::Undo);
    assert_eq!(entry_field(&app, "journal").as_deref(), Some("Nature"));
}

// ── File attachments ────────────────────────────────────────────────────

#[test]
#[cfg(unix)] // asserts '/'-separated stored paths
fn edit_file_row_opens_path_editor_and_updates_one_file() {
    let (mut app, d) = make_app();
    open_detail(&mut app);
    select_file(&mut app, 1);
    app.handle_action(Action::EditField);
    assert!(matches!(
        app.pending_action,
        Some(PendingAction::EditFileAttachment { index: 1, .. })
    ));
    assert_eq!(
        app.field_editor_state.as_ref().unwrap().value,
        "PDF/two.pdf"
    );

    let new_path = d
        .path()
        .canonicalize()
        .unwrap()
        .join("PDF")
        .join("renamed.pdf");
    set_editor_value(&mut app, new_path.to_str().unwrap());
    app.handle_action(Action::ConfirmEdit);
    assert_eq!(app.mode, InputMode::Detail);
    let file = entry_field(&app, "file").unwrap();
    assert!(file.contains("PDF/one.pdf"), "{file}");
    assert!(file.contains("PDF/renamed.pdf"), "{file}");
    assert!(!file.contains("two.pdf"), "{file}");
}

#[test]
#[cfg(unix)] // asserts '/'-separated stored paths
fn edit_file_row_with_unchanged_path_is_noop() {
    let (mut app, d) = make_app();
    open_detail(&mut app);
    select_file(&mut app, 0);
    app.handle_action(Action::EditField);
    let same = d.path().canonicalize().unwrap().join("PDF").join("one.pdf");
    set_editor_value(&mut app, same.to_str().unwrap());
    let undo_before = app.undo_stack.len();
    app.handle_action(Action::ConfirmEdit);
    assert_eq!(app.undo_stack.len(), undo_before);
}

#[test]
#[cfg(unix)] // asserts '/'-separated stored paths
fn add_file_attachment_appends_relative_path_with_type() {
    let (mut app, d) = make_app();
    open_detail(&mut app);
    app.handle_action(Action::AddFileAttachment);
    assert_eq!(app.mode, InputMode::Editing);
    let p = d.path().canonicalize().unwrap().join("notes.djvu");
    set_editor_value(&mut app, p.to_str().unwrap());
    app.handle_action(Action::ConfirmEdit);
    let file = entry_field(&app, "file").unwrap();
    assert!(file.contains("notes.djvu:DJVU"), "{file}");
    assert!(file.contains("PDF/one.pdf"), "{file}");
}

#[test]
#[cfg(unix)] // asserts '/'-separated stored paths
fn add_file_attachment_to_entry_without_file_field() {
    let (mut app, d) = make_app();
    open_detail(&mut app);
    select_field(&mut app, "journal");
    let key = app.detail_entry_key.clone().unwrap();
    app.database
        .entries
        .get_mut(&key)
        .unwrap()
        .fields
        .shift_remove("file");
    app.handle_action(Action::AddFileAttachment);
    let p = d.path().canonicalize().unwrap().join("a.pdf");
    set_editor_value(&mut app, p.to_str().unwrap());
    app.handle_action(Action::ConfirmEdit);
    assert_eq!(entry_field(&app, "file").as_deref(), Some(":a.pdf:PDF"));
    // Undo removes the field again (old value was None).
    app.handle_action(Action::Undo);
    assert_eq!(entry_field(&app, "file"), None);
}

#[test]
fn add_file_attachment_with_empty_path_does_nothing() {
    let (mut app, _d) = make_app();
    open_detail(&mut app);
    let before = entry_field(&app, "file");
    app.handle_action(Action::AddFileAttachment);
    set_editor_value(&mut app, "   ");
    app.handle_action(Action::ConfirmEdit);
    assert_eq!(app.mode, InputMode::Detail);
    assert_eq!(entry_field(&app, "file"), before);
}

#[test]
fn delete_file_row_removes_only_that_file_then_whole_field() {
    let (mut app, _d) = make_app();
    open_detail(&mut app);
    select_file(&mut app, 0);
    app.handle_action(Action::DeleteField);
    let file = entry_field(&app, "file").unwrap();
    assert!(!file.contains("one.pdf"), "{file}");
    assert!(file.contains("two.pdf"), "{file}");

    select_file(&mut app, 0);
    app.handle_action(Action::DeleteField);
    assert_eq!(entry_field(&app, "file"), None);

    // Out-of-range / missing-field deletions are ignored.
    app.delete_file_attachment(5);
    assert_eq!(entry_field(&app, "file"), None);
}

// ── Title case / name normalization ─────────────────────────────────────

#[test]
fn titlecase_field_converts_then_reports_already_done() {
    let (mut app, _d) = make_app();
    open_detail(&mut app);
    select_field(&mut app, "title");
    app.handle_action(Action::TitlecaseField);
    assert_eq!(
        entry_field(&app, "title").as_deref(),
        Some("A Study of Things")
    );
    assert!(app
        .status_message
        .as_deref()
        .unwrap()
        .starts_with("Title-cased"));

    app.handle_action(Action::TitlecaseField);
    assert!(app
        .status_message
        .as_deref()
        .unwrap()
        .contains("already in title case"));
}

#[test]
fn normalize_names_converts_then_reports_already_done() {
    let (mut app, _d) = make_app();
    open_detail(&mut app);
    select_field(&mut app, "author");
    app.handle_action(Action::NormalizeNames);
    assert_eq!(
        entry_field(&app, "author").as_deref(),
        Some("Smith, John and Doe, Jane")
    );
    app.handle_action(Action::NormalizeNames);
    assert!(app
        .status_message
        .as_deref()
        .unwrap()
        .contains("already in 'Last, First'"));
}

#[test]
fn normalize_names_rejects_non_name_field() {
    let (mut app, _d) = make_app();
    open_detail(&mut app);
    select_field(&mut app, "journal");
    app.handle_action(Action::NormalizeNames);
    assert!(app
        .status_message
        .as_deref()
        .unwrap()
        .contains("not a person-name field"));
    assert_eq!(entry_field(&app, "journal").as_deref(), Some("Nature"));
}

// ── confirm_edit dispatch: exports / imports ────────────────────────────

#[test]
fn confirm_export_json_and_ris_write_files() {
    let (mut app, d) = make_app();
    let json = d.path().canonicalize().unwrap().join("out.json");
    confirm_with(&mut app, PendingAction::ExportJson, json.to_str().unwrap());
    assert_eq!(app.mode, InputMode::Normal);
    assert!(std::fs::read_to_string(&json).unwrap().contains("Nature"));

    let ris = d.path().canonicalize().unwrap().join("out.ris");
    confirm_with(&mut app, PendingAction::ExportRis, ris.to_str().unwrap());
    assert!(std::fs::read_to_string(&ris).unwrap().contains("TY  -"));
}

#[test]
fn confirm_exports_with_empty_path_write_nothing() {
    let (mut app, _d) = make_app();
    confirm_with(&mut app, PendingAction::ExportJson, "");
    assert_eq!(app.mode, InputMode::Normal);
    confirm_with(&mut app, PendingAction::ExportRis, "");
    assert_eq!(app.mode, InputMode::Normal);
    assert!(app.field_editor_state.is_none());
}

#[test]
fn confirm_export_settings_returns_to_normal_without_settings_screen() {
    let (mut app, d) = make_app();
    let out = d.path().canonicalize().unwrap().join("cfg.yaml");
    confirm_with(
        &mut app,
        PendingAction::ExportSettings,
        out.to_str().unwrap(),
    );
    assert_eq!(app.mode, InputMode::Normal);
    assert!(out.exists());
}

#[test]
fn confirm_import_settings_and_url_with_empty_input() {
    let (mut app, _d) = make_app();
    confirm_with(&mut app, PendingAction::ImportSettings, "");
    assert_eq!(app.mode, InputMode::Settings);
    confirm_with(&mut app, PendingAction::ImportUrl, "");
    assert_eq!(app.mode, InputMode::Normal);
}

#[test]
fn confirm_import_url_while_import_pending_is_refused() {
    let (mut app, _d) = make_app();
    let (_tx, rx) = std::sync::mpsc::channel();
    app.pending_import = Some(rx);
    confirm_with(&mut app, PendingAction::ImportUrl, "10.1000/xyz");
    assert!(app
        .status_message
        .as_deref()
        .unwrap()
        .contains("already in progress"));
}

#[test]
fn confirm_new_file_reprompts_on_empty_then_appends_bib() {
    let d = tempfile::tempdir().unwrap();
    let mut cfg = default_config();
    cfg.general.backup_on_save = false;
    let mut app = App::new_empty(cfg).unwrap();
    set_editor_value(&mut app, "");
    app.handle_action(Action::ConfirmEdit);
    assert!(matches!(app.pending_action, Some(PendingAction::NewFile)));
    assert_eq!(app.mode, InputMode::Editing);

    let target = d.path().join("fresh");
    set_editor_value(&mut app, target.to_str().unwrap());
    app.handle_action(Action::ConfirmEdit);
    assert_eq!(app.bib_path, d.path().join("fresh.bib"));
    assert!(
        d.path().join("fresh.bib").exists(),
        "{:?}",
        app.status_message
    );
}

// ── confirm_edit dispatch: settings screen ──────────────────────────────

fn open_settings(app: &mut App) {
    app.settings_state = Some(SettingsState::new(&app.config));
}

#[test]
fn confirm_settings_field_group_add_edit_rename() {
    let (mut app, _d) = make_app();
    open_settings(&mut app);
    let n = app.config.field_groups.len();

    confirm_with(&mut app, PendingAction::AddFieldGroup, "Mine");
    assert_eq!(app.mode, InputMode::Settings);
    assert_eq!(app.config.field_groups.len(), n + 1);
    let idx = n;
    assert_eq!(app.config.field_groups[idx].name, "Mine");

    confirm_with(
        &mut app,
        PendingAction::EditFieldGroupFields { index: idx },
        "note, abstract",
    );
    assert_eq!(
        app.config.field_groups[idx].fields,
        vec!["note", "abstract"]
    );

    confirm_with(
        &mut app,
        PendingAction::RenameFieldGroup { index: idx },
        "Ours",
    );
    assert_eq!(app.config.field_groups[idx].name, "Ours");

    // Empty names are ignored.
    confirm_with(&mut app, PendingAction::AddFieldGroup, "  ");
    confirm_with(&mut app, PendingAction::RenameFieldGroup { index: idx }, "");
    assert_eq!(app.config.field_groups.len(), n + 1);
    assert_eq!(app.config.field_groups[idx].name, "Ours");
}

#[test]
fn confirm_settings_column_add_width_rename() {
    let (mut app, _d) = make_app();
    open_settings(&mut app);
    let n = app.config.display.columns.len();

    confirm_with(&mut app, PendingAction::AddColumn, "note");
    assert_eq!(app.config.display.columns.len(), n + 1);
    let idx = n;
    assert_eq!(app.config.display.columns[idx].field, "note");

    confirm_with(
        &mut app,
        PendingAction::EditColumnWidth { index: idx },
        "12",
    );
    confirm_with(
        &mut app,
        PendingAction::RenameColumn { index: idx },
        "abstract",
    );
    assert_eq!(app.config.display.columns[idx].field, "abstract");

    // Empty inputs are ignored.
    confirm_with(&mut app, PendingAction::AddColumn, "");
    confirm_with(&mut app, PendingAction::RenameColumn { index: idx }, " ");
    assert_eq!(app.config.display.columns.len(), n + 1);
    assert_eq!(app.config.display.columns[idx].field, "abstract");
    assert_eq!(app.mode, InputMode::Settings);
}

#[test]
fn confirm_edit_setting_updates_config() {
    let (mut app, _d) = make_app();
    open_settings(&mut app);
    confirm_with(
        &mut app,
        PendingAction::EditSetting {
            setting_id: "general.editor".to_string(),
        },
        "nano",
    );
    assert_eq!(app.mode, InputMode::Settings);
    assert!(
        format!("{:?}", app.config.general.editor).contains("nano"),
        "{:?}",
        app.config.general.editor
    );
}

#[test]
fn confirm_edit_setting_rejects_invalid_theme_color() {
    let (mut app, _d) = make_app();
    open_settings(&mut app);
    let before = app.config.theme.selected_bg.clone();
    confirm_with(
        &mut app,
        PendingAction::EditSetting {
            setting_id: "theme.selected_bg".to_string(),
        },
        "#aéabc",
    );
    assert_eq!(app.mode, InputMode::Settings);
    assert_eq!(app.config.theme.selected_bg, before);
    assert!(app
        .status_message
        .as_deref()
        .unwrap()
        .contains("Invalid color"));

    // A valid (uppercase, padded) color is accepted and trimmed.
    confirm_with(
        &mut app,
        PendingAction::EditSetting {
            setting_id: "theme.selected_bg".to_string(),
        },
        " #ABCDEF ",
    );
    assert_eq!(app.config.theme.selected_bg, "#ABCDEF");
}

#[test]
fn confirm_import_settings_with_invalid_color_warns_without_panicking() {
    let (mut app, d) = make_app();
    let path = d.path().join("bad_theme.yaml");
    std::fs::write(&path, "theme:\n  selected_bg: '#aéabc'\n").unwrap();
    confirm_with(
        &mut app,
        PendingAction::ImportSettings,
        path.to_str().unwrap(),
    );
    let status = app.status_message.as_deref().unwrap();
    assert!(status.contains("Settings imported"), "{status}");
    assert!(status.contains("theme.selected_bg"), "{status}");
}

// ── confirm_edit dispatch: groups ───────────────────────────────────────

#[test]
fn confirm_add_group_rejects_empty_and_invalid_names() {
    let (mut app, _d) = make_app();
    let before = app.database.groups.root.children.len();
    confirm_with(
        &mut app,
        PendingAction::AddGroup {
            parent_path: vec![],
        },
        "",
    );
    assert_eq!(app.mode, InputMode::Normal);
    confirm_with(
        &mut app,
        PendingAction::AddGroup {
            parent_path: vec![],
        },
        "bad;name",
    );
    assert!(app
        .status_message
        .as_deref()
        .unwrap()
        .starts_with("Group not added"));
    assert_eq!(app.database.groups.root.children.len(), before);

    confirm_with(
        &mut app,
        PendingAction::AddGroup {
            parent_path: vec![],
        },
        "Good",
    );
    assert_eq!(app.database.groups.root.children.len(), before + 1);

    confirm_with(
        &mut app,
        PendingAction::RenameGroup { path: vec![before] },
        "Better",
    );
    assert_eq!(app.focus, Focus::Groups);
    assert_eq!(
        app.database.groups.root.children[before].group.name,
        "Better"
    );
}

// ── Vim action handler routed through App ───────────────────────────────

fn editor(app: &App) -> &FieldEditorState {
    app.field_editor_state.as_ref().unwrap()
}

/// Start a Normal-mode editor on `value` with the cursor at byte `cursor`.
fn vim(app: &mut App, value: &str, cursor: usize) {
    let mut e = FieldEditorState::new("title", value);
    e.editing_mode = EditingMode::Normal;
    e.cursor = cursor;
    app.field_editor_state = Some(e);
    app.mode = InputMode::Editing;
}

#[test]
fn vim_insert_mode_entries_position_cursor() {
    let (mut app, _d) = make_app();
    vim(&mut app, "abc", 1);
    app.handle_action(Action::EditEnterInsert);
    assert_eq!(editor(&app).editing_mode, EditingMode::Insert);
    assert_eq!(editor(&app).cursor, 1);

    vim(&mut app, "abc", 1);
    app.handle_action(Action::EditEnterInsertAfter);
    assert_eq!(editor(&app).cursor, 2);

    vim(&mut app, "abc", 1);
    app.handle_action(Action::EditEnterInsertAtEnd);
    assert_eq!(editor(&app).cursor, 3);

    vim(&mut app, "abc", 2);
    app.handle_action(Action::EditEnterInsertAtHome);
    assert_eq!(editor(&app).cursor, 0);

    vim(&mut app, "abc", 0);
    app.handle_action(Action::EditEnterReplace);
    assert_eq!(editor(&app).editing_mode, EditingMode::Replace);

    app.handle_action(Action::EditEnterNormal);
    assert_eq!(editor(&app).editing_mode, EditingMode::Normal);
}

#[test]
fn vim_word_motions() {
    let (mut app, _d) = make_app();
    vim(&mut app, "foo bar-baz qux", 0);
    app.handle_action(Action::EditMoveWordFwd);
    assert_eq!(editor(&app).cursor, 4);
    app.handle_action(Action::EditMoveWordEnd);
    assert_eq!(editor(&app).cursor, 6);
    app.handle_action(Action::EditMoveWordBwd);
    assert_eq!(editor(&app).cursor, 4);

    app.handle_action(Action::EditMoveBigWordFwd);
    assert_eq!(editor(&app).cursor, 12);
    app.handle_action(Action::EditMoveBigWordBwd);
    assert_eq!(editor(&app).cursor, 4);
    app.handle_action(Action::EditMoveBigWordEnd);
    assert_eq!(editor(&app).cursor, 10);
}

#[test]
fn vim_deletions_and_changes() {
    let (mut app, _d) = make_app();
    vim(&mut app, "foo bar", 0);
    app.handle_action(Action::EditDeleteWordFwd);
    assert_eq!(editor(&app).value, "bar");

    vim(&mut app, "foo bar", 3);
    app.handle_action(Action::EditDeleteToEnd);
    assert_eq!(editor(&app).value, "foo");

    vim(&mut app, "foo bar", 3);
    app.handle_action(Action::EditChangeToEnd);
    assert_eq!(editor(&app).value, "foo");
    assert_eq!(editor(&app).editing_mode, EditingMode::Insert);

    vim(&mut app, "foo", 0);
    app.handle_action(Action::EditSubstituteChar);
    assert_eq!(editor(&app).value, "oo");
    assert_eq!(editor(&app).editing_mode, EditingMode::Insert);

    vim(&mut app, "foo", 1);
    app.handle_action(Action::EditSubstituteLine);
    assert_eq!(editor(&app).value, "");
    assert_eq!(editor(&app).editing_mode, EditingMode::Insert);

    vim(&mut app, "foo", 0);
    app.handle_action(Action::EditToggleCase);
    assert_eq!(editor(&app).value, "Foo");

    vim(&mut app, "foo", 0);
    app.handle_action(Action::EditReplaceChar('g'));
    assert_eq!(editor(&app).value, "goo");

    // Undo reverts the last snapshot.
    app.handle_action(Action::EditUndo);
    assert_eq!(editor(&app).value, "foo");
}

#[test]
fn vim_find_and_till_motions() {
    let (mut app, _d) = make_app();
    vim(&mut app, "a-b-c", 0);
    app.handle_action(Action::EditFindCharFwd('-'));
    assert_eq!(editor(&app).cursor, 1);
    app.handle_action(Action::EditFindToCharFwd('c'));
    assert_eq!(editor(&app).cursor, 3);
    app.handle_action(Action::EditFindCharBwd('a'));
    assert_eq!(editor(&app).cursor, 0);

    vim(&mut app, "a-b-c", 4);
    app.handle_action(Action::EditFindToCharBwd('b'));
    assert_eq!(editor(&app).cursor, 3);
}

#[test]
fn vim_delete_to_and_through_char() {
    let (mut app, _d) = make_app();
    vim(&mut app, "abc:def", 0);
    app.handle_action(Action::EditDeleteToChar(':'));
    assert_eq!(editor(&app).value, ":def");

    vim(&mut app, "abc:def", 0);
    app.handle_action(Action::EditDeleteThroughChar(':'));
    assert_eq!(editor(&app).value, "def");

    vim(&mut app, "abc:def", 6);
    app.handle_action(Action::EditDeleteToCharBack(':'));
    assert_eq!(editor(&app).value, "abc:f");

    vim(&mut app, "abc:def", 6);
    app.handle_action(Action::EditDeleteThroughCharBack(':'));
    assert_eq!(editor(&app).value, "abcf");
}

#[test]
fn vim_backward_deletions() {
    let (mut app, _d) = make_app();
    // Normal-mode backspace at end clamps the cursor onto the last char.
    vim(&mut app, "abc", 3);
    app.handle_action(Action::EditDeleteCharBack);
    assert_eq!(editor(&app).value, "ab");
    assert_eq!(editor(&app).cursor, 1);

    vim(&mut app, "foo bar", 7);
    app.handle_action(Action::EditDeleteWordBack);
    assert_eq!(editor(&app).value, "foo ");

    vim(&mut app, "foo bar", 4);
    app.handle_action(Action::EditDeleteToHome);
    assert_eq!(editor(&app).value, "bar");
}

#[test]
fn vim_actions_without_editor_are_ignored() {
    let (mut app, _d) = make_app();
    app.field_editor_state = None;
    for a in [
        Action::EditUndo,
        Action::EditYank,
        Action::EditPut,
        Action::EditEnterInsert,
        Action::EditMoveWordFwd,
        Action::EditDeleteToEnd,
        Action::EditReplaceChar('x'),
    ] {
        app.handle_field_editor_action(a);
    }
    assert!(app.field_editor_state.is_none());
}

#[test]
fn confirm_and_move_down_and_up_reopen_neighbouring_field() {
    let (mut app, _d) = make_app();
    open_detail(&mut app);
    select_field(&mut app, "title");
    app.handle_action(Action::EditField);
    set_editor_value(&mut app, "New Title");
    app.handle_action(Action::EditConfirmAndMoveDown);
    assert_eq!(app.mode, InputMode::Editing);
    let below = editor(&app).field_name.clone();
    assert_ne!(below, "title");
    assert_eq!(entry_field(&app, "title").as_deref(), Some("New Title"));

    app.handle_action(Action::EditConfirmAndMoveUp);
    assert_eq!(app.mode, InputMode::Editing);
    assert_eq!(editor(&app).field_name, "title");
}
