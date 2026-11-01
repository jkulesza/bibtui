//! Tests for `app/completions.rs`: path tab completion (fill, common prefix,
//! cycling, add-file ordering, tilde handling), field-name and month
//! completions, and the free `path_completions` / `contract_tilde` helpers.

#[cfg(unix)]
use super::completions::{contract_tilde, path_completions};
use super::*;
use crate::config::defaults::default_config;
use crate::tui::keybindings::InputMode;
use std::io::Write;

const BIB: &str = r#"@Article{Smith2020,
  author  = {Smith, John},
  title   = {My Paper},
  year    = {2020},
  journal = {Nature},
  note    = {alpha},
}

@Book{Doe2021,
  author    = {Doe, Jane},
  title     = {Rust Programming},
  year      = {2021},
  publisher = {ACM Press},
  note      = {alphabet},
}
"#;

fn make_app() -> (App, tempfile::NamedTempFile) {
    let mut tmp = tempfile::NamedTempFile::new().unwrap();
    write!(tmp, "{}", BIB).unwrap();
    tmp.flush().unwrap();
    let app = App::new(tmp.path().to_path_buf(), default_config()).unwrap();
    (app, tmp)
}

/// A directory containing `apple.pdf`, `apricot.pdf`, `banana.txt`, and a
/// subdirectory `apdir/`. Returned path ends without a separator.
#[cfg(unix)]
fn fixture_dir() -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    for f in ["apple.pdf", "apricot.pdf", "banana.txt"] {
        std::fs::write(d.path().join(f), b"x").unwrap();
    }
    std::fs::create_dir(d.path().join("apdir")).unwrap();
    d
}

fn path_editor(app: &mut App, pending: PendingAction, value: &str) {
    let mut e = FieldEditorState::for_path("Path", value);
    e.cursor = value.len();
    app.field_editor_state = Some(e);
    app.pending_action = Some(pending);
    app.mode = InputMode::Editing;
}

fn value(app: &App) -> String {
    app.field_editor_state.as_ref().unwrap().value.clone()
}

// ── path_completions / contract_tilde ───────────────────────────────────

#[test]
#[cfg(unix)] // '/'-joined candidates and $HOME
fn path_completions_lists_matches_with_dir_suffix() {
    let d = fixture_dir();
    let base = d.path().display().to_string();
    let got = path_completions(&format!("{base}/ap"));
    assert_eq!(
        got,
        vec![
            format!("{base}/apdir/"),
            format!("{base}/apple.pdf"),
            format!("{base}/apricot.pdf"),
        ]
    );
    // Trailing separator lists the whole directory.
    assert_eq!(path_completions(&format!("{base}/")).len(), 4);
    // Missing directory yields nothing.
    assert!(path_completions(&format!("{base}/nope/x")).is_empty());
}

#[test]
#[cfg(unix)] // '/'-joined candidates and $HOME
fn contract_tilde_round_trips_home_paths() {
    let home = std::env::var("HOME").unwrap();
    assert_eq!(contract_tilde(&home), "~");
    assert_eq!(contract_tilde(&format!("{home}/x/y")), "~/x/y");
    assert_eq!(
        contract_tilde("/definitely/not/home"),
        "/definitely/not/home"
    );
}

#[test]
#[cfg(unix)] // '/'-joined candidates and $HOME
fn path_completions_preserve_tilde_prefix() {
    // Every entry under $HOME should come back in `~/` form.
    let got = path_completions("~/");
    assert!(got.iter().all(|c| c.starts_with("~/")), "{got:?}");
}

// ── do_path_tab_complete_dir via EditTabComplete ────────────────────────

#[test]
#[cfg(unix)] // '/'-joined candidates and $HOME
fn path_tab_single_match_fills_it() {
    let (mut app, _t) = make_app();
    let d = fixture_dir();
    let base = d.path().display().to_string();
    path_editor(&mut app, PendingAction::ExportJson, &format!("{base}/ban"));
    app.handle_action(Action::EditTabComplete);
    assert_eq!(value(&app), format!("{base}/banana.txt"));
}

#[test]
#[cfg(unix)] // '/'-joined candidates and $HOME
fn path_tab_no_match_reports_status() {
    let (mut app, _t) = make_app();
    let d = fixture_dir();
    let base = d.path().display().to_string();
    path_editor(&mut app, PendingAction::ExportRis, &format!("{base}/zzz"));
    app.handle_action(Action::EditTabComplete);
    assert_eq!(app.status_message.as_deref(), Some("No completions"));
}

#[test]
#[cfg(unix)] // '/'-joined candidates and $HOME
fn path_tab_advances_to_common_prefix_then_cycles_both_ways() {
    let (mut app, _t) = make_app();
    let d = fixture_dir();
    let base = d.path().display().to_string();
    path_editor(
        &mut app,
        PendingAction::ImportSettings,
        &format!("{base}/a"),
    );

    // a → common prefix "ap".
    app.handle_action(Action::EditTabComplete);
    assert_eq!(value(&app), format!("{base}/ap"));

    // At the common prefix: first Tab picks the first candidate…
    app.handle_action(Action::EditTabComplete);
    assert_eq!(value(&app), format!("{base}/apdir/"));
    // …then cycles forward…
    app.handle_action(Action::EditTabComplete);
    assert_eq!(value(&app), format!("{base}/apple.pdf"));
    // …and backward, wrapping.
    app.handle_action(Action::EditTabCompleteReverse);
    assert_eq!(value(&app), format!("{base}/apdir/"));
    app.handle_action(Action::EditTabCompleteReverse);
    assert_eq!(value(&app), format!("{base}/apricot.pdf"));
}

#[test]
#[cfg(unix)] // '/'-joined candidates and $HOME
fn path_tab_reverse_at_common_prefix_starts_from_last() {
    let (mut app, _t) = make_app();
    let d = fixture_dir();
    let base = d.path().display().to_string();
    path_editor(&mut app, PendingAction::NewFile, &format!("{base}/ap"));
    app.handle_action(Action::EditTabCompleteReverse);
    assert_eq!(value(&app), format!("{base}/apricot.pdf"));
}

#[test]
#[cfg(unix)] // '/'-joined candidates and $HOME
fn path_tab_add_file_puts_citekey_named_files_last() {
    let (mut app, _t) = make_app();
    let d = tempfile::tempdir().unwrap();
    std::fs::write(d.path().join("Smith2020.pdf"), b"x").unwrap();
    std::fs::write(d.path().join("Scan.pdf"), b"x").unwrap();
    let base = d.path().display().to_string();
    path_editor(
        &mut app,
        PendingAction::AddFileAttachment {
            entry_key: "Smith2020".to_string(),
        },
        &format!("{base}/S"),
    );
    // Common prefix equals the input → first Tab fills the first candidate,
    // which must be the non-citekey file.
    app.handle_action(Action::EditTabComplete);
    assert_eq!(value(&app), format!("{base}/Scan.pdf"));
}

#[test]
fn path_tab_ignored_for_non_path_pending_action() {
    let (mut app, _t) = make_app();
    path_editor(&mut app, PendingAction::AddColumn, "/tmp/");
    app.handle_action(Action::EditTabComplete);
    assert_eq!(value(&app), "/tmp/");
    assert!(app.path_completions.is_empty());
}

// ── Field-name and month completions ────────────────────────────────────

#[test]
fn name_phase_completions_filter_by_prefix() {
    let (mut app, _t) = make_app();
    let mut e = FieldEditorState::new_field();
    e.field_name = "pub".to_string();
    app.field_editor_state = Some(e);
    app.update_field_completions();
    let c = &app.field_editor_state.as_ref().unwrap().completions;
    assert!(c.contains(&"publisher".to_string()), "{c:?}");
    assert!(c.iter().all(|n| n.starts_with("pub")), "{c:?}");

    // Empty prefix offers nothing.
    app.field_editor_state.as_mut().unwrap().field_name.clear();
    app.update_field_completions();
    assert!(app
        .field_editor_state
        .as_ref()
        .unwrap()
        .completions
        .is_empty());
}

#[test]
fn month_completions_filter_standard_abbreviations() {
    let (mut app, _t) = make_app();
    let mut e = FieldEditorState::new("month", "ju");
    e.is_month = true;
    app.field_editor_state = Some(e);
    app.update_field_completions();
    assert_eq!(
        app.field_editor_state.as_ref().unwrap().completions,
        vec!["jun", "jul"]
    );
}

#[test]
fn value_tab_advances_to_common_prefix_first() {
    let (mut app, _t) = make_app();
    let mut e = FieldEditorState::new("note", "a");
    e.cursor = 1;
    e.completions = vec!["alpha".to_string(), "alphabet".to_string()];
    app.field_editor_state = Some(e);
    app.handle_action(Action::EditTabComplete);
    assert_eq!(value(&app), "alpha");
    // Now equals completions[0] → cycles to the next.
    app.handle_action(Action::EditTabComplete);
    assert_eq!(value(&app), "alphabet");
}

#[test]
fn name_tab_advances_to_common_prefix() {
    let (mut app, _t) = make_app();
    let mut e = FieldEditorState::new_field();
    e.field_name = "e".to_string();
    e.completions = vec!["editor".to_string(), "edition".to_string()];
    app.field_editor_state = Some(e);
    app.handle_action(Action::EditTabComplete);
    assert_eq!(app.field_editor_state.as_ref().unwrap().field_name, "edit");
    app.handle_action(Action::EditTabCompleteReverse);
    assert_eq!(
        app.field_editor_state.as_ref().unwrap().field_name,
        "edition"
    );
}
