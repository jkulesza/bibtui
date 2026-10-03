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
    assert!(std::fs::read_to_string(dir.path().join("library.bib"))
        .unwrap()
        .contains("Original"));
    std::fs::remove_dir(blocker).unwrap();
    app.request_save(true);
    assert!(app.should_quit);
    assert!(!app.dirty);
    assert!(std::fs::read_to_string(&app.bib_path)
        .unwrap()
        .contains("Unsaved"));
}

#[test]
fn failed_write_and_quit_stays_open() {
    let (mut app, _dir) = review_app("@Misc{A, title={Original}}\n");
    review_edit(&mut app, "A", "title", "Unsaved");
    app.save_io = Box::new(FailingSaveIo);
    app.request_save(true);
    assert!(!app.should_quit);
    assert!(app.dirty);
    assert!(app
        .status_message
        .as_deref()
        .unwrap()
        .contains("Save failed"));
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
    for _ in 0..MAX_UNDO {
        app.undo();
    }
    assert!(!app.dirty);
    assert_eq!(app.database.entries["A"].fields["title"], "Saved");
    for i in 0..=MAX_UNDO {
        review_edit(&mut app, "A", "title", &i.to_string());
    }
    for _ in 0..MAX_UNDO {
        app.undo();
    }
    assert!(app.dirty);
    assert_eq!(app.save_generation, None);
}
#[test]
fn review_delete_save_undo_save_must_restore_entry() {
    let (mut app, _dir) = review_app("@Misc{A, title={Alpha}}\n@Misc{B, title={Beta}}\n");
    app.delete_entry("A");
    app.save();
    app.undo();
    assert!(app.database.entries.contains_key("A"));
    app.save();
    let reloaded = App::new(app.bib_path.clone(), app.config.clone()).unwrap();
    assert_eq!(
        reloaded.database.entries.len(),
        2,
        "restored entry must survive save/reload"
    );
}

#[test]
fn review_key_change_save_undo_save_must_restore_key() {
    let (mut app, _dir) = review_app("@Misc{A, title={Alpha}, year={2020}}\n");
    app.config
        .citekey
        .templates
        .insert("misc".into(), "New[year]".into());
    app.detail_entry_key = Some("A".into());
    app.regen_citekey();
    app.save();
    app.undo();
    assert!(app.database.entries.contains_key("A"));
    app.save();
    let reloaded = App::new(app.bib_path.clone(), app.config.clone()).unwrap();
    assert!(
        reloaded.database.entries.contains_key("A"),
        "undo must restore key on disk"
    );
}

#[test]
fn deleted_entries_restore_after_sorted_save_and_remain_editable() {
    for order in ["none", "citation_key"] {
        let (mut app, _dir) = review_app(
            "@Misc{C, title={Gamma}}\n@Misc{A, title={Alpha}}\n@Misc{B, title={Beta}}\n",
        );
        app.config.save.entry_sort_order = order.into();
        app.delete_entry("A");
        app.delete_entry("C");
        assert!(app.save());
        app.undo();
        app.undo();
        review_edit(&mut app, "A", "title", "Restored Alpha");
        assert!(app.save());
        let reloaded = App::new(app.bib_path.clone(), app.config.clone()).unwrap();
        assert_eq!(reloaded.database.entries.len(), 3);
        assert_eq!(
            reloaded.database.entries["A"].fields["title"],
            "Restored Alpha"
        );
        assert_eq!(reloaded.database.entries["B"].fields["title"], "Beta");
        assert_eq!(reloaded.database.entries["C"].fields["title"], "Gamma");
    }
}

#[test]
fn automatic_save_renames_unwind_before_older_field_undo() {
    let (mut app, _dir) = review_app("@Misc{A, title={Alpha}, year={2020}}\n");
    review_edit(&mut app, "A", "title", "Changed");
    app.config
        .citekey
        .templates
        .insert("misc".into(), "New[year]".into());
    app.config.save.save_action_regenerate_citekeys = true;
    assert!(app.save());
    app.undo(); // automatic rename
    assert!(app.database.entries.contains_key("A"));
    app.undo(); // preceding field edit
    assert_eq!(app.database.entries["A"].fields["title"], "Alpha");
    assert!(app.dirty);
    app.config.save.save_action_regenerate_citekeys = false;
    assert!(app.save());
    let reloaded = App::new(app.bib_path.clone(), app.config.clone()).unwrap();
    assert_eq!(reloaded.database.entries["A"].fields["title"], "Alpha");
}

pub(super) struct FailingSaveIo;
impl crate::util::persistence::SaveIo for FailingSaveIo {
    fn persist(&self, _path: &std::path::Path, _contents: &[u8]) -> std::io::Result<()> {
        Err(std::io::Error::other("injected write/replacement failure"))
    }
}

#[test]
fn failed_save_restores_raw_document_deletions_and_can_retry() {
    let (mut app, _dir) = review_app("@Misc{A, title={Alpha}}\n@Misc{B, title={Beta}}\n");
    app.delete_entry("A");
    review_edit(&mut app, "B", "title", "Changed");
    let before = write_bib_file(&app.database.raw_file);
    let queued = app.deleted_raw_indices.clone();
    app.save_io = Box::new(FailingSaveIo);
    assert!(!app.save());
    assert_eq!(write_bib_file(&app.database.raw_file), before);
    assert_eq!(std::fs::read_to_string(&app.bib_path).unwrap(), before);
    assert_eq!(app.deleted_raw_indices, queued);
    assert_eq!(app.database.entries["B"].fields["title"], "Changed");
    app.save_io = Box::new(crate::util::persistence::FileSaveIo);
    assert!(app.save());
    let reloaded = App::new(app.bib_path.clone(), app.config.clone()).unwrap();
    assert_eq!(reloaded.database.entries.len(), 1);
    assert_eq!(reloaded.database.entries["B"].fields["title"], "Changed");
}

#[cfg(unix)]
#[test]
fn saving_through_symlink_preserves_link_and_target_permissions() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let (mut app, dir) = review_app("@Misc{A, title={Alpha}}\n");
    let original = app.bib_path.clone();
    std::fs::set_permissions(&original, std::fs::Permissions::from_mode(0o640)).unwrap();
    let link = dir.path().join("link.bib");
    symlink(&original, &link).unwrap();
    app.bib_path = link.clone();
    review_edit(&mut app, "A", "title", "Changed");
    assert!(app.save());
    assert!(std::fs::symlink_metadata(&link)
        .unwrap()
        .file_type()
        .is_symlink());
    assert_eq!(
        std::fs::metadata(&original).unwrap().permissions().mode() & 0o777,
        0o640
    );
    assert!(std::fs::read_to_string(&original)
        .unwrap()
        .contains("Changed"));
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);
}
#[test]
fn review_temp_path_must_not_clobber_existing_file() {
    let (mut app, _dir) = review_app("@Misc{A, title={Alpha}}\n");
    let other = app.bib_path.with_extension("bib.tmp");
    std::fs::write(&other, "unrelated temporary data").unwrap();
    app.save();
    assert!(
        other.exists(),
        "save removed an unrelated pre-existing temporary file"
    );
    assert_eq!(
        std::fs::read_to_string(other).unwrap(),
        "unrelated temporary data"
    );
}

#[test]
fn external_changes_preserve_both_external_file_and_existing_backup() {
    for external in [Some("external update"), None] {
        let (mut app, _dir) = review_app("@Misc{A, title={Alpha}}\n");
        app.config.general.backup_on_save = true;
        let backup = app.bib_path.with_extension("bib.bak");
        std::fs::write(&backup, "previous backup").unwrap();
        review_edit(&mut app, "A", "title", "Local edit");
        if let Some(text) = external {
            std::fs::write(&app.bib_path, text).unwrap();
        } else {
            std::fs::remove_file(&app.bib_path).unwrap();
        }
        assert!(!app.save());
        assert!(app.dirty);
        assert!(app
            .status_message
            .as_deref()
            .unwrap()
            .contains("changed outside"));
        assert_eq!(std::fs::read_to_string(&backup).unwrap(), "previous backup");
        assert_eq!(
            std::fs::read_to_string(&app.bib_path).ok().as_deref(),
            external
        );
        assert_eq!(app.database.entries["A"].fields["title"], "Local edit");
    }
}

#[test]
fn new_library_does_not_overwrite_file_created_since_open() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("new.bib");
    let mut app = App::new(path.clone(), default_config()).unwrap();
    std::fs::write(&path, "created by another process").unwrap();
    assert!(!app.save());
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "created by another process"
    );
}

#[test]
fn unchanged_external_rewrite_and_repeated_saves_are_allowed() {
    let (mut app, _dir) = review_app("@Misc{A, title={Alpha}}\n");
    std::fs::write(&app.bib_path, app.saved_contents.as_ref().unwrap()).unwrap();
    assert!(app.save());
    review_edit(&mut app, "A", "title", "Next");
    assert!(app.save());
    assert_eq!(
        app.saved_contents,
        Some(std::fs::read(&app.bib_path).unwrap())
    );
}
#[test]
fn review_partial_attachment_failure_must_keep_paths_consistent() {
    let (mut app, dir) = review_app("@Misc{A, file={:one.pdf:PDF;:two.pdf:PDF}}\n");
    std::fs::write(dir.path().join("one.pdf"), "one").unwrap();
    std::fs::write(dir.path().join("two.pdf"), "two").unwrap();
    std::fs::write(dir.path().join("A_2.pdf"), "occupied").unwrap();
    app.detail_entry_key = Some("A".into());
    app.sync_entry_filename();
    for file in parse_file_field(&app.database.entries["A"].fields["file"]) {
        assert!(
            dir.path().join(&file.path).exists(),
            "attachment now missing: {}",
            file.path
        );
    }
}

#[test]
fn review_attachment_undo_must_not_overwrite_new_file() {
    let (mut app, dir) = review_app("@Misc{A, file={:old.pdf:PDF}}\n");
    std::fs::write(dir.path().join("old.pdf"), "attachment").unwrap();
    app.detail_entry_key = Some("A".into());
    app.sync_entry_filename();
    std::fs::write(dir.path().join("old.pdf"), "new unrelated contents").unwrap();
    app.undo();
    assert_eq!(
        std::fs::read_to_string(dir.path().join("old.pdf")).unwrap(),
        "new unrelated contents"
    );
}

#[test]
fn failed_attachment_undo_keeps_current_path_and_unsaved_indicator() {
    let (mut app, dir) = review_app("@Misc{A, file={:old.pdf:PDF}}\n");
    std::fs::write(dir.path().join("old.pdf"), "attachment").unwrap();
    app.detail_entry_key = Some("A".into());
    app.sync_entry_filename();
    std::fs::write(dir.path().join("old.pdf"), "unrelated").unwrap();
    app.undo();
    assert_eq!(
        parse_file_field(&app.database.entries["A"].fields["file"])[0].path,
        "A.pdf"
    );
    assert!(app.dirty);
    assert!(app
        .status_message
        .as_deref()
        .unwrap()
        .contains("Undo errors"));
}

#[test]
fn manual_bulk_filename_sync_is_dirty_and_undoable() {
    let (mut app, dir) = review_app("@Misc{A, file={:old.pdf:PDF}}\n");
    std::fs::write(dir.path().join("old.pdf"), "attachment").unwrap();
    app.sync_filenames(true);
    assert!(app.dirty);
    assert_eq!(app.undo_stack.len(), 1);
    app.undo();
    assert!(dir.path().join("old.pdf").exists());
    assert!(!app.dirty);
}
#[test]
fn review_save_filename_must_match_final_citekey() {
    let (mut app, dir) = review_app("@Misc{A, year={2020}, file={:old.pdf:PDF}}\n");
    std::fs::write(dir.path().join("old.pdf"), "attachment").unwrap();
    app.config.save.sync_filenames = true;
    app.config.save.save_action_regenerate_citekeys = true;
    app.config
        .citekey
        .templates
        .insert("misc".into(), "New[year]".into());
    app.save();
    let e = app.database.entries.values().next().unwrap();
    assert_eq!(
        parse_file_field(&e.fields["file"])[0].path,
        format!("{}.pdf", e.citation_key)
    );
}

#[test]
fn save_preview_uses_final_keys_without_mutating_live_state() {
    let (mut app, dir) = review_app("@Misc{A, year={2020}, file={:old.pdf:PDF}}\n");
    std::fs::write(dir.path().join("old.pdf"), "attachment").unwrap();
    app.config.save.sync_filenames = true;
    app.config.save.save_action_regenerate_citekeys = true;
    app.config
        .citekey
        .templates
        .insert("misc".into(), "New[year]".into());
    app.request_save(false);
    let DialogKind::FileSyncPreview { renames } = &app.dialog_state.as_ref().unwrap().kind else {
        panic!("expected preview")
    };
    assert!(renames[0].1.ends_with("New2020.pdf"));
    assert!(app.database.entries.contains_key("A"));
    assert!(dir.path().join("old.pdf").exists());
    app.handle_dialog_confirm();
    assert!(dir.path().join("New2020.pdf").exists());
    assert!(app.database.entries.contains_key("New2020"));
    assert_eq!(
        app.saved_contents,
        Some(std::fs::read(&app.bib_path).unwrap())
    );
}

#[test]
fn save_failure_reverses_attachment_moves_and_preserves_database() {
    let (mut app, dir) = review_app("@Misc{A, file={:old.pdf:PDF}}\n");
    std::fs::write(dir.path().join("old.pdf"), "attachment").unwrap();
    app.config.save.sync_filenames = true;
    let before = app.database.clone();
    app.save_io = Box::new(FailingSaveIo);
    assert!(!app.save());
    assert_eq!(app.database, before);
    assert!(dir.path().join("old.pdf").exists());
    assert!(!dir.path().join("A.pdf").exists());
    app.save_io = Box::new(crate::util::persistence::FileSaveIo);
    assert!(app.save());
    assert!(dir.path().join("A.pdf").exists());
}

#[test]
fn backup_failure_precedes_attachment_moves() {
    let (mut app, dir) = review_app("@Misc{A, file={:old.pdf:PDF}}\n");
    std::fs::write(dir.path().join("old.pdf"), "attachment").unwrap();
    std::fs::create_dir(app.bib_path.with_extension("bib.bak")).unwrap();
    app.config.general.backup_on_save = true;
    app.config.save.sync_filenames = true;
    assert!(!app.save());
    assert!(dir.path().join("old.pdf").exists());
    assert!(!dir.path().join("A.pdf").exists());
}

#[test]
fn shared_attachments_and_changed_previews_are_rejected_before_moves() {
    let (mut app, dir) =
        review_app("@Misc{A, file={:old.pdf:PDF}}\n@Misc{B, file={:old.pdf:PDF}}\n");
    std::fs::write(dir.path().join("old.pdf"), "attachment").unwrap();
    app.config.save.sync_filenames = true;
    assert!(!app.save());
    assert!(app
        .status_message
        .as_deref()
        .unwrap()
        .contains("shared attachment"));
    app.database
        .entries
        .get_mut("B")
        .unwrap()
        .fields
        .shift_remove("file");
    app.request_save(false);
    review_edit(&mut app, "A", "title", "Arrived during preview");
    app.handle_dialog_confirm();
    assert!(app
        .status_message
        .as_deref()
        .unwrap()
        .contains("during the preview"));
    assert!(dir.path().join("old.pdf").exists());
    assert_eq!(
        app.database.entries["A"].fields["title"],
        "Arrived during preview"
    );
}

#[test]
fn failed_save_reports_unreversed_attachment_and_keeps_recoverable_path() {
    struct RecoveryFailure;
    impl crate::util::persistence::SaveIo for RecoveryFailure {
        fn persist(&self, path: &std::path::Path, _: &[u8]) -> std::io::Result<()> {
            std::fs::write(path.parent().unwrap().join("old.pdf"), "unrelated")?;
            Err(std::io::Error::other("replacement failed"))
        }
    }
    let (mut app, dir) = review_app("@Misc{A, file={:old.pdf:PDF}}\n");
    std::fs::write(dir.path().join("old.pdf"), "attachment").unwrap();
    app.config.save.sync_filenames = true;
    app.save_io = Box::new(RecoveryFailure);
    assert!(!app.save());
    assert!(app
        .status_message
        .as_deref()
        .unwrap()
        .contains("recovery failed"));
    assert_eq!(
        parse_file_field(&app.database.entries["A"].fields["file"])[0].path,
        "A.pdf"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("old.pdf")).unwrap(),
        "unrelated"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("A.pdf")).unwrap(),
        "attachment"
    );
    assert!(app.dirty);
    app.save_io = Box::new(crate::util::persistence::FileSaveIo);
    assert!(app.save());
    let loaded = App::new(app.bib_path.clone(), app.config.clone()).unwrap();
    assert_eq!(
        loaded.database.entries["A"].fields["file"],
        app.database.entries["A"].fields["file"]
    );
}

#[test]
fn canceled_save_preview_leaves_files_and_history_untouched() {
    let (mut app, dir) = review_app("@Misc{A, file={:old.pdf:PDF}}\n");
    std::fs::write(dir.path().join("old.pdf"), "attachment").unwrap();
    app.config.save.sync_filenames = true;
    let before = app.database.clone();
    app.request_save(true);
    app.handle_action(Action::DialogCancel);
    assert!(app.pending_save.is_none());
    assert_eq!(app.database, before);
    assert!(app.undo_stack.is_empty());
    assert!(!app.should_quit);
    assert!(dir.path().join("old.pdf").exists());
}

#[test]
fn key_changes_preserve_original_field_expression_variants_across_saves() {
    for mode in ["manual", "automatic", "duplicate"] {
        let entry = "@Misc{A, journal=j, title={Hello} # {World}, note=\"quoted\", year={2020}}\n";
        let input = format!(
            "@String{{j = {{Journal}}}}\n{}{}",
            entry,
            if mode == "duplicate" { entry } else { "" }
        );
        let (mut app, _dir) = review_app(&input);
        app.config
            .citekey
            .templates
            .insert("misc".into(), "New[year]".into());
        if mode == "manual" {
            app.detail_entry_key = Some("A".into());
            app.regen_citekey();
        } else if mode == "automatic" {
            app.config.save.save_action_regenerate_citekeys = true;
        }
        for _ in 0..2 {
            assert!(app.save());
            let raw = parse_bib_file(&std::fs::read_to_string(&app.bib_path).unwrap()).unwrap();
            for item in raw.items {
                if let RawItem::Entry(entry) = item {
                    let value = |name| &entry.fields.iter().find(|f| f.name == name).unwrap().value;
                    assert!(matches!(value("journal"), RawFieldValue::Bare(_)), "{mode}");
                    assert!(matches!(value("title"), RawFieldValue::Concat(_)), "{mode}");
                    assert!(matches!(value("note"), RawFieldValue::Quoted(_)), "{mode}");
                }
            }
        }
    }
}

#[test]
fn single_key_change_updates_exact_crossrefs_and_undoes_as_one_step() {
    let (mut app, _dir) = review_app("@Misc{Parent, year={2020}}\n@Article{Child, crossref={Parent}, note={Parent}}\n@Misc{Other, crossref={ParentSuffix}}\n");
    app.config
        .citekey
        .templates
        .insert("misc".into(), "New[year]".into());
    app.detail_entry_key = Some("Parent".into());
    app.regen_citekey();
    assert_eq!(app.database.entries["Child"].fields["crossref"], "New2020");
    assert_eq!(app.database.entries["Child"].fields["note"], "Parent");
    assert_eq!(
        app.database.entries["Other"].fields["crossref"],
        "ParentSuffix"
    );
    assert_eq!(app.undo_stack.len(), 1);
    assert!(app.save());
    app.undo();
    assert!(app.database.entries.contains_key("Parent"));
    assert_eq!(app.database.entries["Child"].fields["crossref"], "Parent");
    assert!(app.save());
    let reload = App::new(app.bib_path.clone(), app.config.clone()).unwrap();
    assert_eq!(
        reload.database.entries["Child"].fields["crossref"],
        "Parent"
    );
}

#[test]
fn bulk_crossrefs_use_simultaneous_mapping_with_collisions() {
    let (mut app, _dir) = review_app("@Misc{B, title={C}}\n@Misc{A, title={B}}\n@Misc{D, title={C}}\n@Misc{Stable, title={Stable}}\n@Misc{Child, title={Child}, crossref={A}}\n@Misc{Second, title={Second}, crossref={B}}\n@Misc{Third, title={Third}, crossref={D}}\n@Misc{Fourth, title={Fourth}, crossref={Stable}}\n");
    app.config
        .citekey
        .templates
        .insert("misc".into(), "[title]".into());
    assert_eq!(app.regen_all_citekeys_impl(true), 3);
    for (key, target) in [
        ("Child", "B"),
        ("Second", "C"),
        ("Third", "C_2"),
        ("Fourth", "Stable"),
    ] {
        assert_eq!(app.database.entries[key].fields["crossref"], target);
        assert!(app.database.entries.contains_key(target));
    }
    assert!(app.save());
    app.undo();
    for (key, target) in [
        ("Child", "A"),
        ("Second", "B"),
        ("Third", "D"),
        ("Fourth", "Stable"),
    ] {
        assert_eq!(app.database.entries[key].fields["crossref"], target);
        assert!(app.database.entries.contains_key(target));
    }
    assert!(app.save());
}

#[test]
fn save_preserves_blank_lines_inside_every_opaque_item() {
    for newline in ["\n", "\r\n"] {
        let input = "@String{j={First\n\n\nSecond}}\n@Preamble{\"First\n\n\nSecond\"}\n@Comment{First\n\n\nSecond}\n% First\n\n\n% Second\n@Misc{A, abstract={First\n\n\nSecond}, note=\"First\n\n\nSecond\"}\n".replace('\n', newline);
        let (mut app, _dir) = review_app(&input);
        assert!(app.save());
        assert_eq!(std::fs::read_to_string(&app.bib_path).unwrap(), input);
    }
}

#[test]
fn sort_order_is_transitive_and_antisymmetric_for_mixed_values() {
    let values = [
        "",
        " ",
        "2",
        "10",
        "1a",
        "-5",
        "+5",
        "05",
        "18446744073709551616",
        "é",
        "ê",
        "100--120",
        "-12--1",
    ];
    for field in ["title", "citation_key", "year", "volume", "number", "pages"] {
        for a in values {
            for b in values {
                assert_eq!(
                    compare_sort_values(field, a, b),
                    compare_sort_values(field, b, a).reverse()
                );
                for c in values {
                    if compare_sort_values(field, a, b).is_le()
                        && compare_sort_values(field, b, c).is_le()
                    {
                        assert!(
                            compare_sort_values(field, a, c).is_le(),
                            "{field}: {a}, {b}, {c}"
                        );
                    }
                }
            }
        }
    }
    let (mut app, _dir) =
        review_app("@Misc{A,title={2}}\n@Misc{B,title={10}}\n@Misc{C,title={1a}}\n");
    app.config.display.default_sort.field = "title".into();
    app.config.display.default_sort.ascending = true;
    let mut ascending = sort_entries(&app.database.entries, &app.config);
    assert_eq!(ascending, ["B", "C", "A"]);
    ascending.reverse();
    app.config.display.default_sort.ascending = false;
    assert_eq!(sort_entries(&app.database.entries, &app.config), ascending);
}

#[test]
fn documented_multi_field_search_matches_all_qualifiers() {
    let (mut app, _dir) = review_app("@Article{A, author={Smith, John}, year={2020}}\n@Article{B, author={Smith, John}, year={2021}}\n");
    app.search_bar_state.query = "author:smith year:2020".into();
    app.update_search();
    assert_eq!(app.visible_entry_count(), 1);
    assert_eq!(app.selected_entry_key().as_deref(), Some("A"));
}

#[test]
fn completion_prefix_is_a_prefix_of_every_unicode_candidate() {
    assert_eq!(longest_common_prefix(&["é.pdf".into(), "ê.pdf".into()]), "");
    let parts = ["", "é", "ê", "漢", "😀", "😁", "a", "e\u{301}"];
    for prefix in parts {
        for a in parts {
            for b in parts {
                let candidates = [format!("{prefix}{a}.pdf"), format!("{prefix}{b}.pdf")];
                let common = longest_common_prefix(&candidates);
                assert!(candidates.iter().all(|s| s.starts_with(&common)));
                assert!(common.starts_with(prefix));
            }
        }
    }
}

#[test]
fn key_releases_do_not_dispatch_or_advance_command_history() {
    use crossterm::event::KeyModifiers;
    for c in ['j', 'd', 'g', 'y'] {
        let (mut app, _dir) = review_app("@Misc{A}\n@Misc{B}\n");
        let press = KeyEvent::new(KeyCode::Char(c), KeyModifiers::NONE);
        app.handle_event(Event::Key(press));
        let selection = app.entry_list_state.selected();
        let history = (app.second_last_key, app.last_key);
        app.handle_event(Event::Key(KeyEvent {
            kind: KeyEventKind::Release,
            ..press
        }));
        assert_eq!(app.entry_list_state.selected(), selection);
        assert_eq!((app.second_last_key, app.last_key), history);
        assert!(app.dialog_state.is_none());
    }
    let (mut app, _dir) = review_app("@Misc{A}\n@Misc{B}\n");
    let press = KeyEvent::new(KeyCode::Char('z'), KeyModifiers::NONE);
    app.user_bindings
        .push((InputMode::Normal, press, Action::DeleteEntry));
    for kind in [KeyEventKind::Release, KeyEventKind::Repeat] {
        app.handle_event(Event::Key(KeyEvent { kind, ..press }));
        assert!(app.dialog_state.is_none());
    }
    app.handle_event(Event::Key(press));
    assert!(app.dialog_state.is_some());
}

#[test]
fn repeat_events_allow_navigation_and_text_without_command_chains() {
    use crossterm::event::KeyModifiers;
    let (mut app, _dir) = review_app("@Misc{A}\n@Misc{B}\n");
    let event = |c, kind| {
        Event::Key(KeyEvent::new_with_kind(
            KeyCode::Char(c),
            KeyModifiers::NONE,
            kind,
        ))
    };
    app.handle_event(event('j', KeyEventKind::Repeat));
    assert_eq!(app.entry_list_state.selected(), 1);
    app.handle_event(event('d', KeyEventKind::Press));
    app.handle_event(event('d', KeyEventKind::Repeat));
    assert!(app.dialog_state.is_none());
    assert_eq!(app.second_last_key, None);
    app.mode = InputMode::Search;
    app.handle_event(event('a', KeyEventKind::Press));
    app.handle_event(event('a', KeyEventKind::Release));
    app.handle_event(event('a', KeyEventKind::Repeat));
    assert_eq!(app.search_bar_state.query, "aa");
}

#[test]
fn mutations_rebuild_search_results_and_preserve_selection_identity() {
    let (mut app, _dir) =
        review_app("@Misc{A, title={Alpha}}\n@Misc{B, title={Beta}}\n@Misc{C, title={Gamma}}\n");
    app.search_bar_state.query = "key:B".into();
    app.update_search();
    app.delete_entry("B");
    assert!(app.visible_entries().is_empty());
    assert_eq!(app.selected_entry_key(), None);
    assert_eq!(app.search_bar_state.result_count, 0);
    app.undo();
    assert_eq!(app.selected_entry_key().as_deref(), Some("B"));
    app.duplicate_entry();
    assert_eq!(app.visible_entry_count(), 2);
    assert_eq!(app.selected_entry_key().as_deref(), Some("B"));
    app.undo();
    assert_eq!(app.visible_entry_count(), 1);
    app.add_entry_of_type("Misc");
    assert_eq!(app.visible_entry_count(), 1);
    app.close_detail();
    app.search_bar_state.clear();
    app.update_search();
    let position = app.sorted_keys.iter().position(|key| key == "C").unwrap();
    app.entry_list_state.select(position);
    app.delete_entry("A");
    assert_eq!(app.selected_entry_key().as_deref(), Some("C"));
}

#[test]
fn group_and_search_filters_compose_and_keep_duplicate_name_identity() {
    let (mut app, _dir) = review_app("@Misc{A, title={Alpha}, year={2020}}\n@Misc{B, title={Beta}, year={2021}}\n@Misc{C, title={Beta}, year={2020}}\n");
    let group = |year: &str| GroupNode {
        group: Group {
            name: "Same".into(),
            group_type: GroupType::Keyword {
                field: "year".into(),
                search_term: year.into(),
                case_sensitive: false,
                regex: false,
            },
        },
        children: vec![],
        expanded: true,
        original_fields: None,
    };
    app.database.groups.root.children = vec![group("2020"), group("2021")];
    app.group_tree_state.refresh(&app.database.groups);
    app.group_tree_state.select(1);
    app.select_group();
    assert_eq!(app.visible_entry_count(), 2);
    app.group_tree_state.select(2);
    app.select_group(); // same name, different identity
    assert_eq!(app.selected_entry_key().as_deref(), Some("B"));
    app.config.display.default_sort.ascending = false;
    app.refresh_view();
    assert_eq!(app.group_tree_state.active_path, Some(vec![1]));
    assert_eq!(app.selected_entry_key().as_deref(), Some("B"));
    app.search_bar_state.query = "title:Alpha".into();
    app.update_search();
    assert_eq!(app.visible_entry_count(), 0);
    app.search_bar_state.query = "title:Beta".into();
    app.update_search();
    assert_eq!(app.visible_entry_count(), 1);
    app.finish_delete_group(vec![0]); // shift the active sibling's path
    assert_eq!(app.group_tree_state.active_path, Some(vec![0]));
    assert_eq!(app.selected_entry_key().as_deref(), Some("B"));
    app.undo();
    assert_eq!(app.group_tree_state.active_path, Some(vec![1]));
    assert_eq!(app.selected_entry_key().as_deref(), Some("B"));
    app.detail_entry_key = Some("B".into());
    app.field_editor_state = Some(FieldEditorState::new("year", "2020"));
    app.config
        .citekey
        .templates
        .insert("misc".into(), "[title]".into());
    app.confirm_edit();
    assert_eq!(app.visible_entry_count(), 0);
    app.undo(); // rename
    app.undo(); // year
    assert_eq!(app.selected_entry_key().as_deref(), Some("B"));
    assert_eq!(app.search_bar_state.result_count, 1);
}

#[test]
fn importing_and_rekeying_reapply_active_search() {
    use crate::util::import::ImportedEntry;
    let (mut app, _dir) = review_app("@Misc{A, title={Original}}\n");
    app.config
        .citekey
        .templates
        .insert("misc".into(), "[title]".into());
    app.search_bar_state.query = "title:Imported".into();
    app.update_search();
    let fields = IndexMap::from([("title".into(), "Imported".into())]);
    app.handle_import_result(Ok(ImportedEntry::new("misc", fields)));
    assert_eq!(app.visible_entry_count(), 1);
    assert_eq!(app.selected_entry_key().as_deref(), Some("Imported"));
    app.search_bar_state.query = "key:A".into();
    app.update_search();
    app.regen_all_citekeys();
    assert!(app
        .visible_entries()
        .iter()
        .all(|e| e.citation_key != "Imported"));
    assert_eq!(
        app.search_bar_state.result_count,
        app.visible_entries().len()
    );
}

#[test]
fn bulk_rekey_preserves_file_order_and_first_available_suffix() {
    let (mut app, _dir) = review_app("@Misc{Target, title={Target}}\n@Misc{Old, title={Target}}\n@Misc{Target_2, title={Target}}\n@Misc{Other, title={Target}}\n");
    app.config
        .citekey
        .templates
        .insert("misc".into(), "[title]".into());
    app.config.display.default_sort.field = "none".into();
    app.refresh_view();
    app.entry_list_state.select(1);
    app.regen_all_citekeys();
    assert_eq!(
        app.database
            .entries
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["Target", "Target_3", "Target_2", "Target_4"]
    );
    assert_eq!(app.selected_entry_key().as_deref(), Some("Target_3"));
    assert_eq!(app.regen_all_citekeys_impl(true), 0);
    assert!(app.save());
    let reload = App::new(app.bib_path.clone(), app.config.clone()).unwrap();
    assert_eq!(
        reload.database.entries.keys().collect::<Vec<_>>(),
        app.database.entries.keys().collect::<Vec<_>>()
    );
    app.undo();
    assert_eq!(
        app.database
            .entries
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["Target", "Old", "Target_2", "Other"]
    );
}

#[test]
fn rendering_builds_only_viewport_rows_and_tracks_global_navigation() {
    use ratatui::{backend::TestBackend, Terminal};
    for count in [100, 10_000] {
        let input: String = (0..count)
            .map(|i| format!("@Misc{{Key{i:05}, title={{Row {i:05}}}}}\n"))
            .collect();
        let (mut app, _dir) = review_app(&input);
        app.show_groups = false;
        app.focus = Focus::List;
        let mut terminal = Terminal::new(TestBackend::new(100, 20)).unwrap();
        for action in [
            Action::MoveToBottom,
            Action::MoveToTop,
            Action::PageDown,
            Action::PageUp,
        ] {
            app.handle_action(action);
            terminal.draw(|frame| app.render(frame)).unwrap();
            assert_eq!(app.entry_list_state.rendered_rows, 15);
            let selected = app.selected_entry_key().unwrap();
            let screen: String = terminal
                .backend()
                .buffer()
                .content
                .iter()
                .map(|cell| cell.symbol())
                .collect();
            assert!(
                screen.contains(&selected),
                "selected row {selected} must be rendered"
            );
        }
        app.handle_action(Action::MoveToBottom);
        terminal.draw(|frame| app.render(frame)).unwrap();
        terminal.backend_mut().resize(100, 12);
        terminal.autoresize().unwrap();
        terminal.draw(|frame| app.render(frame)).unwrap();
        assert_eq!(app.entry_list_state.rendered_rows, 7);
        assert_eq!(app.entry_list_state.table_state.offset(), count - 7);
        app.search_bar_state.query = "key:missing".into();
        app.update_search();
        terminal.draw(|frame| app.render(frame)).unwrap();
        assert_eq!(app.entry_list_state.rendered_rows, 0);
        assert_eq!(app.selected_entry_key(), None);
    }
}

fn finish_review_search(app: &mut App) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while app.search_bar_state.searching {
        app.poll_search();
        assert!(
            std::time::Instant::now() < deadline,
            "background search timed out"
        );
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}

#[test]
fn background_search_discards_old_queries_and_edits_and_batches_paste() {
    let input: String = (0..600)
        .map(|i| {
            format!(
                "@Misc{{Key{i:05}, title={{Common}}, abstract={{{} tailneedle}}}}\n",
                "padding ".repeat(300)
            )
        })
        .collect();
    let (mut app, _dir) = review_app(&input);
    app.handle_action(Action::EnterSearch);
    let before = app.search_worker.submission_count();
    app.handle_event(Event::Paste("abstract:tailneedle title:common".into()));
    assert_eq!(app.search_worker.submission_count() - before, 1);
    assert!(app.search_bar_state.searching);
    // The event loop can accept a command while matching continues.
    app.handle_action(Action::ConfirmSearch);
    app.handle_action(Action::EnterCommand);
    assert_eq!(app.mode, InputMode::Command);
    finish_review_search(&mut app);
    assert_eq!(app.visible_entry_count(), 600);
    app.search_bar_state.query = "key:Key00001".into();
    app.update_search();
    app.search_bar_state.query = "title:Unique".into();
    app.update_search();
    review_edit(&mut app, "Key00002", "title", "Unique");
    app.refresh_view();
    finish_review_search(&mut app);
    assert_eq!(app.selected_entry_key().as_deref(), Some("Key00002"));
    app.delete_entry("Key00002");
    finish_review_search(&mut app);
    assert_eq!(app.visible_entry_count(), 0);
    app.undo();
    finish_review_search(&mut app);
    assert_eq!(app.selected_entry_key().as_deref(), Some("Key00002"));
    app.search_bar_state.query = "common".into();
    app.update_search();
    app.handle_action(Action::ExitSearch);
    assert!(!app.search_bar_state.searching);
    assert_eq!(app.visible_entry_count(), 600);
    // Cancelled results cannot reinstate the old filter.
    app.poll_search();
    assert!(app.filtered_indices.is_none());
}

fn workflow_key(app: &mut App, code: KeyCode) {
    app.handle_event(Event::Key(KeyEvent::new(
        code,
        crossterm::event::KeyModifiers::NONE,
    )));
}
fn workflow_chars(app: &mut App, text: &str) {
    for c in text.chars() {
        workflow_key(app, KeyCode::Char(c));
    }
}
fn workflow_command(app: &mut App, command: &str) {
    workflow_key(app, KeyCode::Char(':'));
    app.handle_event(Event::Paste(command.into()));
    workflow_key(app, KeyCode::Enter);
}

#[test]
fn event_workflow_edit_save_undo_save_and_reload() {
    let (mut app, _dir) = review_app("@Misc{A, title={Original}, year={2020}}\n");
    app.config
        .citekey
        .templates
        .insert("misc".into(), "A".into());
    app.focus = Focus::List;
    workflow_key(&mut app, KeyCode::Enter);
    workflow_chars(&mut app, "/title");
    workflow_key(&mut app, KeyCode::Enter);
    workflow_chars(&mut app, "eS");
    app.handle_event(Event::Paste("Updated\nUnicode é".into()));
    workflow_key(&mut app, KeyCode::Enter);
    assert_eq!(
        app.database.entries["A"].fields["title"],
        "Updated Unicode é"
    );
    assert!(app.dirty);
    // One Escape clears detail search; the next returns to the entry list.
    workflow_key(&mut app, KeyCode::Esc);
    workflow_key(&mut app, KeyCode::Esc);
    workflow_command(&mut app, "w");
    assert!(!app.dirty);
    workflow_chars(&mut app, "u");
    assert!(app.dirty);
    workflow_command(&mut app, "w");
    let reload = App::new(app.bib_path.clone(), app.config.clone()).unwrap();
    assert_eq!(reload.database.entries["A"].fields["title"], "Original");
    assert_eq!(app.selected_entry_key().as_deref(), Some("A"));
    assert!(!app.dirty);
}

#[test]
fn event_workflow_creates_library_and_roundtrips_settings() {
    let dir = tempfile::tempdir().unwrap();
    let mut config = default_config();
    config.general.backup_on_save = false;
    config.save.save_action_regenerate_citekeys = false;
    let mut app = App::new_empty(config).unwrap();
    let path = dir.path().join("new_library");
    app.handle_event(Event::Paste(path.display().to_string()));
    workflow_key(&mut app, KeyCode::Enter);
    assert!(path.with_extension("bib").exists());
    assert_eq!(app.mode, InputMode::Normal);
    app.focus = Focus::List;
    workflow_chars(&mut app, "a");
    workflow_key(&mut app, KeyCode::Enter);
    assert_eq!(app.database.entries.len(), 1);
    workflow_key(&mut app, KeyCode::Esc);
    workflow_command(&mut app, "w");
    assert!(!app.dirty);
    let reload = App::new(app.bib_path.clone(), app.config.clone()).unwrap();
    assert_eq!(reload.database.entries.len(), 1);
    workflow_chars(&mut app, "SE");
    // Path editors begin in Insert mode. Clear their default at the cursor.
    app.handle_event(Event::Key(KeyEvent::new(
        KeyCode::Char('u'),
        crossterm::event::KeyModifiers::CONTROL,
    )));
    let settings_path = dir.path().join("settings.yaml");
    app.handle_event(Event::Paste(settings_path.display().to_string()));
    workflow_key(&mut app, KeyCode::Enter);
    assert!(settings_path.exists());
    let mut imported = app.config.clone();
    imported.display.show_braces = !app.show_braces;
    std::fs::write(&settings_path, serde_yaml::to_string(&imported).unwrap()).unwrap();
    workflow_chars(&mut app, "I");
    app.handle_event(Event::Paste(settings_path.display().to_string()));
    workflow_key(&mut app, KeyCode::Enter);
    assert_eq!(app.show_braces, imported.display.show_braces);
    assert_eq!(app.mode, InputMode::Settings);
}

#[test]
fn event_workflow_attachment_confirmation_failure_retry_and_undo() {
    let (mut app, dir) = review_app("@Misc{A, file={:old.pdf:PDF}}\n");
    app.config.save.sync_filenames = true;
    std::fs::write(dir.path().join("old.pdf"), "%PDF attachment").unwrap();
    app.save_io = Box::new(FailingSaveIo);
    workflow_command(&mut app, "wq");
    assert_eq!(app.mode, InputMode::Dialog);
    workflow_chars(&mut app, "y");
    assert!(!app.should_quit);
    assert!(app.dirty);
    assert!(dir.path().join("old.pdf").exists());
    app.save_io = Box::new(crate::util::persistence::FileSaveIo);
    workflow_command(&mut app, "w");
    workflow_chars(&mut app, "y");
    assert!(!app.dirty);
    assert!(dir.path().join("A.pdf").exists());
    workflow_chars(&mut app, "u");
    assert!(app.dirty);
    assert!(dir.path().join("old.pdf").exists());
    assert_eq!(
        parse_file_field(&app.database.entries["A"].fields["file"])[0].path,
        "old.pdf"
    );
}

#[test]
fn generated_valid_values_preserve_parsing_and_save_reload_semantics() {
    let values = [
        "ASCII",
        "Café",
        "漢字",
        "{Protected}",
        "line\n\nnext",
        r"\LaTeX{}",
        "😀",
        "e\u{301}",
    ];
    for newline in ["\n", "\r\n"] {
        for value in values {
            let input = format!(
                "@Misc{{A, title={{{value}}}, year={{2020}}}}\n@Misc{{B, note={{Unchanged}}}}\n"
            )
            .replace('\n', newline);
            let raw = parse_bib_file(&input).unwrap();
            assert!(raw.warnings.is_empty());
            assert_eq!(write_bib_file(&raw), input);
            let (mut app, _dir) = review_app(&input);
            let expected = app.database.entries["A"].fields["title"].clone();
            review_edit(&mut app, "A", "year", "2021");
            assert!(app.save());
            let reload = App::new(app.bib_path.clone(), app.config.clone()).unwrap();
            assert_eq!(reload.database.entries.len(), 2);
            assert_eq!(reload.database.entries["A"].fields["title"], expected);
            assert_eq!(reload.database.entries["A"].fields["year"], "2021");
            assert_eq!(reload.database.entries["B"].fields["note"], "Unchanged");
        }
    }
}

#[test]
fn event_workflow_type_and_group_changes_save_and_undo() {
    let input = include_str!("../../tests/fixtures/jabref_groups.bib");
    let (mut app, _dir) = review_app(input);
    app.focus = Focus::List;
    let key = app.selected_entry_key().unwrap();
    let original_type = app.database.entries[&key].entry_type.clone();
    let original_groups = app.database.entries[&key].group_memberships.clone();
    workflow_key(&mut app, KeyCode::Enter);
    workflow_chars(&mut app, "t");
    workflow_key(&mut app, KeyCode::Down);
    workflow_key(&mut app, KeyCode::Enter);
    assert_ne!(app.database.entries[&key].entry_type, original_type);
    workflow_key(&mut app, KeyCode::Tab);
    workflow_key(&mut app, KeyCode::Char(' '));
    workflow_key(&mut app, KeyCode::Enter);
    assert_ne!(
        app.database.entries[&key].group_memberships,
        original_groups
    );
    workflow_key(&mut app, KeyCode::Esc);
    workflow_command(&mut app, "w");
    assert!(!app.dirty);
    workflow_chars(&mut app, "uu");
    assert_eq!(app.database.entries[&key].entry_type, original_type);
    assert_eq!(
        app.database.entries[&key].group_memberships,
        original_groups
    );
    workflow_command(&mut app, "w");
    let reload = App::new(app.bib_path.clone(), app.config.clone()).unwrap();
    assert_eq!(reload.database.entries[&key].entry_type, original_type);
    assert_eq!(
        reload.database.entries[&key].group_memberships,
        original_groups
    );
}

#[test]
fn restore_deleted_key_after_another_entry_reused_it_and_was_saved() {
    let (mut app, _dir) =
        review_app("@Misc{A, title={Original A}}\n@Misc{B, title={Original B}}\n");
    app.delete_entry("A");
    app.config
        .citekey
        .templates
        .insert("misc".into(), "A".into());
    app.detail_entry_key = Some("B".into());
    app.regen_citekey();
    assert!(app.save());
    app.undo();
    app.undo();
    assert!(app.save());
    let reload = App::new(app.bib_path.clone(), app.config.clone()).unwrap();
    assert_eq!(reload.database.entries.len(), 2);
    assert_eq!(reload.database.entries["A"].fields["title"], "Original A");
    assert_eq!(reload.database.entries["B"].fields["title"], "Original B");
}
