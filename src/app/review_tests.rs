use super::*;
use crate::config::defaults::default_config;

fn review_app(input: &str) -> (App, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.bib");
    std::fs::write(&path, input).unwrap();
    let mut cfg = default_config();
    cfg.general.backup_on_save = false;
    cfg.save.entry_sort_order = "none".into();
    cfg.save.save_action_regenerate_citekeys = false;
    cfg.save.save_action_trim_whitespace = false;
    cfg.save.save_action_escape_underscores = false;
    cfg.save.save_action_escape_ampersands = false;
    cfg.save.save_action_cleanup_url = false;
    cfg.save.save_action_latex_cleanup = false;
    cfg.save.save_action_normalize_date = false;
    cfg.save.save_action_normalize_month = false;
    cfg.save.save_action_normalize_names_of_persons = false;
    cfg.save.save_action_normalize_page_numbers = false;
    cfg.save.save_action_normalize_isbn = false;
    cfg.save.save_action_ordinals_to_superscript = false;
    cfg.save.save_action_unicode_to_latex = false;
    (App::new(path, cfg).unwrap(), dir)
}

fn review_edit(app: &mut App, key: &str, field: &str, value: &str) {
    let old = app.database.entries[key].fields.get(field).cloned();
    app.push_undo(UndoItem::FieldChanged {
        entry_key: key.into(),
        field_name: field.into(),
        old_value: old,
    });
    let entry = app.database.entries.get_mut(key).unwrap();
    entry.fields.insert(field.into(), value.into());
    entry.dirty = true;
}

#[test]
fn review_failed_wq_must_stay_open() {
    let (mut app, _dir) = review_app("@Misc{A, title={Original}}\n");
    review_edit(&mut app, "A", "title", "Unsaved");
    app.config.general.backup_on_save = true;
    std::fs::create_dir(app.bib_path.with_extension("bib.bak")).unwrap();
    app.command_palette_state.input = "wq".into();
    app.execute_command();
    assert!(app
        .status_message
        .as_deref()
        .unwrap()
        .contains("Backup failed"));
    assert!(!app.should_quit, "save failure must not exit the app");
}


#[test]
fn failed_save_and_quit_preview_can_retry() {
    let (mut app, dir) = review_app("@Misc{A, title={Original}}\n");
    review_edit(&mut app, "A", "title", "Unsaved");
    app.config.general.backup_on_save = true;
    let blocker = app.bib_path.with_extension("bib.bak");
    std::fs::create_dir(&blocker).unwrap();
    app.pending_action = Some(PendingAction::SaveAndQuit);
    app.handle_dialog_confirm();
    assert!(!app.should_quit);
    assert!(app.dirty);
    assert!(std::fs::read_to_string(dir.path().join("library.bib")).unwrap().contains("Original"));
    std::fs::remove_dir(blocker).unwrap();
    app.request_save(true);
    assert!(app.should_quit);
    assert!(!app.dirty);
    assert!(std::fs::read_to_string(&app.bib_path).unwrap().contains("Unsaved"));
}

#[test]
fn failed_write_and_quit_stays_open() {
    let (mut app, _dir) = review_app("@Misc{A, title={Original}}\n");
    review_edit(&mut app, "A", "title", "Unsaved");
    std::fs::create_dir(app.bib_path.with_extension("bib.tmp")).unwrap();
    app.request_save(true);
    assert!(!app.should_quit);
    assert!(app.dirty);
    assert!(app.status_message.as_deref().unwrap().contains("Save failed"));
}
#[test]
fn review_edit_after_undo_saved_state_must_be_dirty() {
    let (mut app, _dir) = review_app("@Misc{A, title={Original}}\n");
    review_edit(&mut app, "A", "title", "Saved");
    app.save();
    app.undo();
    review_edit(&mut app, "A", "title", "Different");
    assert!(
        app.dirty,
        "equal undo depth does not imply equal document state"
    );
}


#[test]
fn undo_branch_requires_quit_confirmation_and_new_save_point() {
    let (mut app, _dir) = review_app("@Misc{A, title={Original}}\n");
    review_edit(&mut app, "A", "title", "Saved");
    assert!(app.save());
    review_edit(&mut app, "A", "title", "Later");
    app.undo();
    assert!(!app.dirty);
    app.undo();
    review_edit(&mut app, "A", "title", "Branch");
    app.command_palette_state.input = "q".into();
    app.execute_command();
    assert!(!app.should_quit);
    assert!(matches!(app.pending_action, Some(PendingAction::Quit)));
    assert!(app.save());
    review_edit(&mut app, "A", "title", "After branch save");
    app.undo();
    assert!(!app.dirty);
    assert_eq!(app.database.entries["A"].fields["title"], "Branch");
}

#[test]
fn undo_marker_survives_cap_and_batch_until_saved_state_evicted() {
    let (mut app, _dir) = review_app("@Misc{A, title={Original}}\n");
    review_edit(&mut app, "A", "title", "Saved");
    assert!(app.save());
    for i in 0..MAX_UNDO {
        review_edit(&mut app, "A", "title", &i.to_string());
    }
    for _ in 0..MAX_UNDO { app.undo(); }
    assert!(!app.dirty);
    assert_eq!(app.database.entries["A"].fields["title"], "Saved");
    for i in 0..=MAX_UNDO {
        review_edit(&mut app, "A", "title", &i.to_string());
    }
    for _ in 0..MAX_UNDO { app.undo(); }
    assert!(app.dirty);
    assert_eq!(app.save_generation, None);
}
