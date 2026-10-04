//! Library-level settings: layering, writing to the `.bib`, settings-editor
//! routing, undo, and compatibility with files saved by JabRef.

use super::*;
use crate::config::defaults::default_config;
use crate::config::library::{decode_value, LibrarySettings};
use pretty_assertions::assert_eq;
use serde_json::json;

const LIBRARY: &str = "@Article{Smith2020,
  author = {Smith, John},
  title  = {First Paper},
  year   = {2020},
}

@Comment{jabref-meta: databaseType:bibtex;}

@Comment{jabref-meta: keypattern_article:[auth][year];}
";

fn yaml_config() -> Config {
    let mut config = default_config();
    config.general.backup_on_save = false;
    config
}

fn open(contents: &str, config: Config) -> (App, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.bib");
    std::fs::write(&path, contents).unwrap();
    (App::new(path, config).unwrap(), dir)
}

fn reload(app: &App) -> App {
    App::new(app.bib_path.clone(), app.base_config.clone()).unwrap()
}

fn saved(app: &App) -> String {
    std::fs::read_to_string(&app.bib_path).unwrap()
}

fn run_command(app: &mut App, command: &str) {
    app.command_palette_state.input = command.into();
    app.execute_command();
}

fn open_settings_at(app: &mut App, id: &str) {
    app.handle_action(Action::EnterSettings);
    let state = app.settings_state.as_mut().unwrap();
    let row = state
        .rows
        .iter()
        .position(|row| matches!(row, SettingRow::Item(i) if state.items[*i].id == id))
        .unwrap();
    state.cursor = row;
}

use crate::tui::components::settings::SettingRow;

#[test]
fn library_settings_override_yaml_and_are_marked() {
    let contents = LIBRARY.replace(
        "@Comment{jabref-meta: databaseType",
        "@Comment{jabref-meta: bibtui.save.sync_filenames:\ntrue;\n}\n\n@Comment{jabref-meta: databaseType",
    );
    let (mut app, _dir) = open(&contents, yaml_config());
    assert!(app.config.save.sync_filenames);
    assert!(!app.base_config.save.sync_filenames);
    assert_eq!(app.config.citekey.templates["article"], "[auth][year]");
    app.handle_action(Action::EnterSettings);
    let state = app.settings_state.as_ref().unwrap();
    assert!(state.library_paths.contains("save.sync_filenames"));
    assert!(state.library_paths.contains("citekey.templates.article"));
    assert!(!state.library_paths.contains("save.align_fields"));
}

#[test]
fn invalid_library_settings_are_reported_at_startup() {
    let contents =
        format!("{LIBRARY}\n@Comment{{jabref-meta: bibtui.save.sync_filenames:maybe;}}\n");
    let (app, _dir) = open(&contents, yaml_config());
    let status = app.status_message.clone().unwrap();
    assert!(status.contains("Library settings:"), "{status}");
    assert!(status.contains("save.sync_filenames"), "{status}");
    assert!(!app.config.save.sync_filenames);
}

#[test]
fn library_keybindings_and_theme_take_effect() {
    let contents = format!(
        "{LIBRARY}\n@Comment{{jabref-meta: bibtui.keybindings:\n%7B\"normal\":%7B\"ctrl-d\":\"DeleteEntry\"%7D%7D;\n}}\n"
    );
    // Hand-written braces would also work if balanced; the encoder escapes them.
    let (app, _dir) = open(&contents, yaml_config());
    assert_eq!(app.config.keybindings["normal"]["ctrl-d"], "DeleteEntry");
    assert!(!app.user_bindings.is_empty());
}

#[test]
fn export_previews_writes_jabref_layout_and_round_trips() {
    let mut config = yaml_config();
    config.save.sync_filenames = true;
    config.save.save_action_latex_cleanup = false;
    config.titlecase.stop_words = vec!["a".into(), "of".into()];
    let (mut app, _dir) = open(LIBRARY, config);
    app.handle_action(Action::EnterSettings);
    app.handle_action(Action::SettingsExportLibrary);
    let DialogKind::ChangePreview { lines, .. } = &app.dialog_state.as_ref().unwrap().kind else {
        panic!("expected a change preview");
    };
    assert!(
        lines.contains(&"+ bibtui.save.sync_filenames = true".to_string()),
        "{lines:?}"
    );
    assert!(
        lines.iter().any(|l| l.starts_with("+ saveActions = ")),
        "{lines:?}"
    );
    // Nothing is written until confirmed.
    assert!(!app.dirty);
    app.handle_action(Action::DialogConfirm);
    assert_eq!(app.mode, InputMode::Settings);
    assert!(app.dirty);
    assert!(app.save(), "{:?}", app.status_message);

    let text = saved(&app);
    // The entry and JabRef's own comments are untouched; new keys are placed
    // in JabRef's key order with one blank line between comments.
    assert!(text.starts_with(
        "@Article{Smith2020,\n  author = {Smith, John},\n  title  = {First Paper},\n  year   = {2020},\n}\n\n"
    ));
    assert!(text.contains(
        "@Comment{jabref-meta: bibtui.save.sync_filenames:\ntrue;\n}\n\n@Comment{jabref-meta: bibtui.titlecase.stop_words:\n[\"a\",\"of\"];\n}\n\n@Comment{jabref-meta: databaseType:bibtex;}\n\n@Comment{jabref-meta: keypattern_article:[auth][year];}\n\n@Comment{jabref-meta: saveActions:enabled;\n"
    ), "{text}");
    assert!(!text.contains("\n\n\n"), "{text}");
    assert!(text.ends_with(";}\n"), "{text}");

    // A machine with a default YAML config sees the same settings.
    let other = App::new(app.bib_path.clone(), yaml_config()).unwrap();
    assert!(other.config.save.sync_filenames);
    assert!(!other.config.save.save_action_latex_cleanup);
    assert_eq!(other.config.titlecase.stop_words, ["a", "of"]);
    assert_eq!(
        other.status_message.as_deref(),
        Some("Library settings: this library overrides YAML save settings (latex_cleanup off, sync_filenames on); see ◆ in Settings")
    );

    // Exporting again changes nothing.
    let mut again = reload(&app);
    again.request_library_export();
    assert!(again.dialog_state.is_none());
    assert!(again
        .status_message
        .as_deref()
        .unwrap()
        .contains("already up to date"));
}

#[test]
fn export_and_clear_are_undoable_and_restore_the_file_exactly() {
    let mut config = yaml_config();
    config.save.field_order = "alphabetical".into();
    let (mut app, _dir) = open(LIBRARY, config);
    run_command(&mut app, "settings-export bib");
    app.handle_action(Action::DialogConfirm);
    assert_eq!(app.mode, InputMode::Normal);
    assert_eq!(
        app.library.values["save.field_order"],
        json!("alphabetical")
    );
    app.undo();
    assert!(app.library.values.is_empty());
    assert!(!app.dirty);
    assert_eq!(write_bib_file(&app.database.raw_file), LIBRARY);

    run_command(&mut app, "settings-export bib");
    app.handle_action(Action::DialogConfirm);
    assert!(app.save());
    let mut app = reload(&app);
    run_command(&mut app, "settings-clear-bib");
    assert!(app.library.values.is_empty());
    assert_eq!(app.config.save.field_order, "alphabetical"); // still the YAML value
    assert!(app.save());
    let text = saved(&app);
    assert!(!text.contains("bibtui."), "{text}");
    // JabRef's own keys are kept.
    assert!(text.contains("keypattern_article:[auth][year]"), "{text}");
    assert_eq!(text, LIBRARY);
}

#[test]
fn editing_a_library_setting_updates_the_library_not_yaml() {
    let contents = LIBRARY.replace(
        "@Comment{jabref-meta: databaseType",
        "@Comment{jabref-meta: bibtui.save.sync_filenames:\ntrue;\n}\n\n@Comment{jabref-meta: databaseType",
    );
    let (mut app, dir) = open(&contents, yaml_config());
    open_settings_at(&mut app, "save.sync_filenames");
    app.handle_action(Action::SettingsToggle);
    assert!(!app.config.save.sync_filenames);
    assert_eq!(app.library.values["save.sync_filenames"], json!(false));
    assert!(!app.base_config.save.sync_filenames);
    assert!(app.dirty);

    // A setting the library does not override changes the YAML layer only.
    open_settings_at(&mut app, "save.align_fields");
    let before = app.config.save.align_fields;
    app.handle_action(Action::SettingsToggle);
    assert_eq!(app.base_config.save.align_fields, !before);
    assert!(!app.library.values.contains_key("save.align_fields"));

    // YAML export contains the YAML layer, never the library's values.
    let yaml_path = dir.path().join("exported.yaml");
    run_command(
        &mut app,
        &format!("settings-export yaml {}", yaml_path.display()),
    );
    let exported: Config =
        serde_yaml::from_str(&std::fs::read_to_string(&yaml_path).unwrap()).unwrap();
    assert!(!exported.save.sync_filenames);
    assert_eq!(exported.save.align_fields, !before);

    app.undo(); // the library toggle is the only undo record
    assert!(app.config.save.sync_filenames);
}

#[test]
fn editing_a_library_citekey_template_rewrites_the_jabref_keypattern() {
    let (mut app, _dir) = open(LIBRARY, yaml_config());
    app.handle_action(Action::EnterSettings);
    let state = app.settings_state.as_mut().unwrap();
    state.set_value(
        "citekey.template.article",
        SettingValue::Str("[auth:lower][year]".into()),
    );
    state.apply_to_config(&mut app.config);
    app.commit_settings_edit();
    assert!(app.save());
    let text = saved(&app);
    assert!(
        text.contains("@Comment{jabref-meta: keypattern_article:[auth:lower][year];}"),
        "{text}"
    );
    assert!(!text.contains("bibtui."), "{text}");
}

#[test]
fn unrepresentable_jabref_save_actions_are_not_rewritten_by_toggles() {
    let contents = format!(
        "{LIBRARY}\n@Comment{{jabref-meta: saveActions:enabled;\ntitle[latex_cleanup,title_case]\n;}}\n"
    );
    let (mut app, _dir) = open(&contents, yaml_config());
    assert!(app
        .status_message
        .as_deref()
        .unwrap()
        .contains("saveActions"));
    open_settings_at(&mut app, "save_actions.latex_cleanup");
    app.handle_action(Action::SettingsToggle);
    assert!(app
        .status_message
        .as_deref()
        .unwrap()
        .contains("change those save actions in JabRef"));
    assert!(!app.dirty);
    assert_eq!(write_bib_file(&app.database.raw_file), contents);
}

#[test]
fn metadata_inserted_before_entries_keeps_entry_bindings_valid() {
    // Metadata comments at the top of the file: inserting a new one shifts
    // every entry's raw position.
    let contents = "@Comment{jabref-meta: databaseType:bibtex;}\n\n@Article{A,\n  title = {Alpha},\n}\n\n@Article{B,\n  title = {Beta},\n}\n";
    let mut config = yaml_config();
    config.save.entry_sort_order = "none".into();
    config.save.sync_filenames = true;
    config.save.save_action_regenerate_citekeys = false;
    let (mut app, _dir) = open(contents, config);
    app.delete_entry("A");
    run_command(&mut app, "settings-export bib");
    app.handle_action(Action::DialogConfirm);
    let entry = app.database.entries.get_mut("B").unwrap();
    entry.fields.insert("title".into(), "Beta edited".into());
    entry.dirty = true;
    assert!(app.save(), "{:?}", app.status_message);
    assert!(!saved(&app).contains("\n\n\n"), "{}", saved(&app));
    let reloaded = reload(&app);
    assert_eq!(reloaded.database.entries.len(), 1);
    assert_eq!(
        reloaded.database.entries["B"].fields["title"],
        "Beta edited"
    );
    assert!(reloaded.config.save.sync_filenames);
    assert_eq!(reloaded.config.save.entry_sort_order, "none");
}

#[test]
fn export_into_a_library_without_metadata_appends_cleanly() {
    let contents = "@Article{A,\n  title = {Alpha},\n}\n";
    // Only sync_filenames differs from bibtui's defaults.
    let mut config = default_config();
    config.save.sync_filenames = true;
    let (mut app, _dir) = open(contents, config);
    run_command(&mut app, "settings-export bib");
    app.handle_action(Action::DialogConfirm);
    app.config.save.save_action_regenerate_citekeys = false; // keep key `A`
    assert!(app.save());
    assert_eq!(
        saved(&app),
        "@Article{A,\n  title = {Alpha},\n}\n\n@Comment{jabref-meta: bibtui.save.sync_filenames:\ntrue;\n}\n"
    );
}

#[test]
fn settings_exported_by_bibtui_survive_jabref_rewriting_them() {
    // `after` is the same library opened and saved in JabRef 5.15's GUI.
    let before = include_str!("../../tests/fixtures/jabref_meta_roundtrip_before.bib");
    let after = include_str!("../../tests/fixtures/jabref_meta_roundtrip_after_jabref_5.15.bib");
    let read = |text: &str| build_database(parse_bib_file(text).unwrap()).jabref_meta;
    let (before_meta, after_meta) = (read(before), read(after));
    // A hand-written (non-JSON) value is kept verbatim too.
    assert_eq!(
        before_meta.unknown_meta["bibtui.test.regex"],
        after_meta.unknown_meta["bibtui.test.regex"]
    );
    let before = LibrarySettings::from_meta(&before_meta);
    let after = LibrarySettings::from_meta(&after_meta);
    for path in [
        "citation.style",
        "field_groups",
        "save.field_order",
        "save.sync_filenames",
        "test.semicolon",
        "titlecase.stop_words",
    ] {
        assert_eq!(before.values.get(path), after.values.get(path), "{path}");
        assert!(after.values.contains_key(path), "{path}");
    }
    assert_eq!(after.values["test.semicolon"], json!("a;b"));
    assert_eq!(after.key_patterns, before.key_patterns);
    // JabRef strips a level of backslashes, which is why bibtui
    // percent-encodes them: a raw backslash does not survive.
    assert_eq!(
        before_meta.unknown_meta["bibtui.test.backslash"],
        "\"C:\\\\papers\\\\x\""
    );
    assert_eq!(
        after_meta.unknown_meta["bibtui.test.backslash"],
        "\"C:\\papers\\x\""
    );
    // bibtui writes comments in the exact layout JabRef produced.
    let comment = crate::config::library::bibtui_comment("citation.style", &json!("IEEEtranN"));
    assert!(
        include_str!("../../tests/fixtures/jabref_meta_roundtrip_after_jabref_5.15.bib")
            .contains(&comment)
    );
    let encoded = crate::config::library::encode_value(&json!("C:\\papers\\x"));
    assert!(!encoded.contains('\\'));
    assert_eq!(decode_value(&encoded).unwrap(), json!("C:\\papers\\x"));
}

#[test]
fn startup_notes_library_save_actions_that_override_yaml() {
    // JabRef's default save actions only normalize dates, months, and pages.
    let contents = format!(
        "{LIBRARY}\n@Comment{{jabref-meta: saveActions:enabled;\nall-text-fields[identity]\ndate[normalize_date]\nmonth[normalize_month]\npages[normalize_page_numbers]\n;}}\n"
    );
    let (mut app, _dir) = open(&contents, yaml_config());
    let status = app.status_message.clone().unwrap();
    assert!(status.contains("overrides YAML save settings"), "{status}");
    assert!(status.contains("latex_cleanup off"), "{status}");
    assert!(!status.contains("normalize_month"), "{status}");
    assert!(!app.config.save.save_action_latex_cleanup);
    assert!(app.config.save.save_action_normalize_month);

    // Turning an action back on rewrites JabRef's saveActions so JabRef
    // applies it too; the placeholder line is dropped.
    open_settings_at(&mut app, "save_actions.latex_cleanup");
    app.handle_action(Action::SettingsToggle);
    assert!(app.config.save.save_action_latex_cleanup);
    assert!(app.save(), "{:?}", app.status_message);
    let text = saved(&app);
    assert!(text.contains("title[latex_cleanup]\n"), "{text}");
    assert!(text.contains("month[normalize_month]\n"), "{text}");
    assert!(!text.contains("identity"), "{text}");
    assert!(reload(&app).config.save.save_action_latex_cleanup);
}

#[test]
fn settings_screen_marks_library_settings_and_renders_the_preview() {
    use ratatui::{backend::TestBackend, Terminal};
    let contents = LIBRARY.replace(
        "@Comment{jabref-meta: databaseType",
        "@Comment{jabref-meta: bibtui.save.sync_filenames:\ntrue;\n}\n\n@Comment{jabref-meta: databaseType",
    );
    let mut config = yaml_config();
    config.save.field_order = "alphabetical".into();
    let (mut app, _dir) = open(&contents, config);
    open_settings_at(&mut app, "save.sync_filenames");
    let mut terminal = Terminal::new(TestBackend::new(140, 40)).unwrap();
    let screen = |terminal: &Terminal<TestBackend>| -> String {
        terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect()
    };
    terminal.draw(|frame| app.render(frame)).unwrap();
    let text = screen(&terminal);
    assert!(text.contains('◆'), "library marker missing");
    assert!(
        text.contains("Stored in this library"),
        "description note missing"
    );
    assert!(text.contains("B: write to .bib"));

    app.handle_action(Action::SettingsExportLibrary);
    terminal.draw(|frame| app.render(frame)).unwrap();
    let text = screen(&terminal);
    assert!(text.contains("Write settings to library"));
    assert!(
        text.contains("+ bibtui.save.field_order = alphabetical"),
        "{text}"
    );
    app.handle_action(Action::DialogCancel);
    assert_eq!(app.mode, InputMode::Settings);
    assert!(!app.dirty);
}

#[test]
fn bibtui_written_settings_are_byte_stable_through_jabref() {
    // bibtui exported these settings, then JabRef 5.15 opened and saved the
    // file: JabRef rewrote it byte for byte, including saveActions, the
    // keypattern regex backslash, and percent-encoded values.
    let jabref_saved =
        include_str!("../../tests/fixtures/jabref_meta_bibtui_written_after_jabref_5.15.bib");
    let mut config = yaml_config();
    config.save.sync_filenames = true;
    config.save.save_action_latex_cleanup = false;
    config.save.entry_sort_order = "none".into();
    config.save.save_action_regenerate_citekeys = false;
    config.titlecase.ignore_words = vec![
        "C:\\papers".into(),
        "a;b".into(),
        "{braced}".into(),
        "100%".into(),
    ];
    config.citekey.templates.insert(
        "book".into(),
        "Book_[auth]_[title:regex(\"\\s\", \"\")]".into(),
    );
    let (mut app, _dir) = open(LIBRARY, config);
    app.request_library_export();
    app.handle_action(Action::DialogConfirm);
    assert!(app.save(), "{:?}", app.status_message);
    assert_eq!(saved(&app), jabref_saved);
    let effective = app.config.clone();

    // Reading JabRef's file with a default YAML config restores every setting.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.bib");
    std::fs::write(&path, jabref_saved).unwrap();
    let reread = App::new(path, yaml_config()).unwrap();
    assert!(
        reread.library.warnings.is_empty(),
        "{:?}",
        reread.library.warnings
    );
    assert_eq!(
        crate::config::library::config_value(&reread.config),
        crate::config::library::config_value(&effective)
    );
    assert_eq!(
        reread.config.citekey.templates["book"],
        "Book_[auth]_[title:regex(\"\\s\", \"\")]"
    );
}
