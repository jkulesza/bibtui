use super::tests::unique_doi;
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
    fn persist_new(&self, _path: &std::path::Path, _contents: &[u8]) -> std::io::Result<()> {
        Err(std::io::Error::other("injected write failure"))
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
    // A shared attachment is not renamed (and is named in the status), but
    // it no longer blocks saving the library.
    assert!(app.save());
    let status = app.status_message.clone().unwrap();
    assert!(status.contains("not renamed"), "{status}");
    assert!(status.contains("also used by"), "{status}");
    assert!(dir.path().join("old.pdf").exists());
    assert!(!dir.path().join("A.pdf").exists());
    assert!(!dir.path().join("B.pdf").exists());
    // Detach B's copy (marked dirty, as a real field deletion is).
    let b = app.database.entries.get_mut("B").unwrap();
    b.fields.shift_remove("file");
    b.dirty = true;
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
        finish_review_search(&mut app);
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

#[test]
fn imported_attachment_paths_escape_jabref_separators() {
    let (mut app, dir) = review_app("@Misc{A}\n");
    let path = dir.path().join("paper;part.pdf");
    std::fs::write(&path, "%PDF").unwrap();
    let mut imported = crate::util::import::ImportedEntry::new("misc", IndexMap::new());
    imported.pdf_path = Some(path.clone());
    app.handle_import_result(Ok(imported));
    let key = app.detail_entry_key.clone().unwrap();
    let files = parse_file_field(&app.database.entries[&key].fields["file"]);
    assert_eq!(files.len(), 1);
    assert_eq!(
        crate::util::open::resolve_file_path(&files[0].path, dir.path()),
        path
    );
    assert!(app.save());
    let reload = App::new(app.bib_path.clone(), app.config.clone()).unwrap();
    assert_eq!(
        parse_file_field(&reload.database.entries[&key].fields["file"])[0].path,
        "paper;part.pdf"
    );
}

#[test]
fn grouped_attachment_undo_retains_errors_after_other_moves_succeed() {
    let (mut app, dir) =
        review_app("@Misc{A,file={:old-a.pdf:PDF}}\n@Misc{B,file={:old-b.pdf:PDF}}\n");
    std::fs::write(dir.path().join("old-a.pdf"), "A").unwrap();
    std::fs::write(dir.path().join("old-b.pdf"), "B").unwrap();
    app.sync_filenames(true);
    std::fs::write(dir.path().join("old-b.pdf"), "unrelated").unwrap();
    app.undo();
    assert!(app
        .status_message
        .as_deref()
        .unwrap()
        .contains("Undo errors"));
    assert_eq!(
        parse_file_field(&app.database.entries["A"].fields["file"])[0].path,
        "old-a.pdf"
    );
    assert_eq!(
        parse_file_field(&app.database.entries["B"].fields["file"])[0].path,
        "B.pdf"
    );
    assert_eq!(
        std::fs::read_to_string(dir.path().join("old-b.pdf")).unwrap(),
        "unrelated"
    );
    assert!(app.dirty);
}

fn run_command(app: &mut App, command: &str) {
    app.command_palette_state.input = command.into();
    app.execute_command();
}

#[test]
fn missing_attachment_is_reported_without_blocking_save() {
    let (mut app, _dir) = review_app("@Misc{A, title={T}, file={:missing.pdf:PDF}}\n");
    app.config.save.sync_filenames = true;
    review_edit(&mut app, "A", "title", "Changed");
    assert!(app.save(), "{:?}", app.status_message);
    let status = app.status_message.clone().unwrap();
    assert!(status.contains("1 attachment not renamed"), "{status}");
    assert!(status.contains("missing.pdf (file not found)"), "{status}");
    assert_eq!(app.database.entries["A"].fields["file"], ":missing.pdf:PDF");
    assert!(!app.dirty);
}

#[test]
fn remote_attachment_links_are_left_alone_on_save() {
    let (mut app, _dir) =
        review_app("@Misc{A, title={T}, file={Online:https\\://example.com/x.pdf:URL}}\n");
    app.config.save.sync_filenames = true;
    review_edit(&mut app, "A", "title", "Changed");
    assert!(app.save(), "{:?}", app.status_message);
    assert_eq!(
        app.database.entries["A"].fields["file"],
        "Online:https\\://example.com/x.pdf:URL"
    );
    assert!(app.compute_sync_renames(true).is_empty());
}

#[test]
fn case_only_attachment_rename_succeeds_on_save() {
    let (mut app, dir) = review_app("@Misc{Smith2020, title={T}, file={:smith2020.pdf:PDF}}\n");
    std::fs::write(dir.path().join("smith2020.pdf"), b"%PDF").unwrap();
    app.config.save.sync_filenames = true;
    review_edit(&mut app, "Smith2020", "title", "Changed");
    assert!(app.save(), "{:?}", app.status_message);
    let mut names: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|n| n.ends_with(".pdf"))
        .collect();
    names.sort();
    assert_eq!(names, ["Smith2020.pdf"]);
    assert_eq!(
        app.database.entries["Smith2020"].fields["file"],
        ":Smith2020.pdf:PDF"
    );
}

#[test]
fn manual_sync_reports_missing_attachment_as_not_renamed() {
    let (mut app, _dir) = review_app("@Misc{A, title={T}, file={:missing.pdf:PDF}}\n");
    app.detail_entry_key = Some("A".into());
    app.sync_entry_filename();
    let status = app.status_message.clone().unwrap();
    assert!(status.contains("not renamed"), "{status}");
    assert!(app.undo_stack.is_empty());
}

#[test]
fn force_write_overwrites_external_change_and_backs_it_up() {
    let (mut app, _dir) = review_app("@Misc{A, title={Original}}\n");
    assert!(!app.config.general.backup_on_save);
    std::fs::write(&app.bib_path, "@Misc{External, title={Theirs}}\n").unwrap();
    review_edit(&mut app, "A", "title", "Mine");

    run_command(&mut app, "w");
    assert!(app.dirty);
    let status = app.status_message.clone().unwrap();
    assert!(status.contains(":w!"), "{status}");

    run_command(&mut app, "w!");
    assert!(!app.dirty, "{:?}", app.status_message);
    let saved = std::fs::read_to_string(&app.bib_path).unwrap();
    assert!(saved.contains("Mine") && !saved.contains("Theirs"));
    // The external version is kept even though backups are disabled.
    let backup = std::fs::read_to_string(app.bib_path.with_extension("bib.bak")).unwrap();
    assert!(backup.contains("Theirs"));

    // The forced save becomes the new baseline for ordinary saves.
    review_edit(&mut app, "A", "title", "Again");
    run_command(&mut app, "w");
    assert!(!app.dirty, "{:?}", app.status_message);
}

#[test]
fn force_write_and_quit_exits_after_saving() {
    let (mut app, _dir) = review_app("@Misc{A, title={Original}}\n");
    std::fs::write(&app.bib_path, "@Misc{External, title={Theirs}}\n").unwrap();
    review_edit(&mut app, "A", "title", "Mine");
    run_command(&mut app, "wq");
    assert!(!app.should_quit);
    run_command(&mut app, "wq!");
    assert!(app.should_quit, "{:?}", app.status_message);
}

#[test]
fn background_search_keeps_previous_results_until_new_ones_arrive() {
    let input: String = (0..1_000)
        .map(|i| format!("@Misc{{Key{i:05}, title={{Row {i:05}}}}}\n"))
        .collect();
    let (mut app, _dir) = review_app(&input);
    app.search_bar_state.query = "Key0001".into();
    app.update_search();
    finish_review_search(&mut app);
    let before = app.visible_entry_count();
    assert!(before > 0);
    // An edit re-runs the search in the background; the list must not blank.
    review_edit(&mut app, "Key00010", "title", "Edited");
    app.refresh_view();
    assert!(app.search_bar_state.searching);
    assert_eq!(app.visible_entry_count(), before);
    finish_review_search(&mut app);
    assert_eq!(app.visible_entry_count(), before);
}

#[test]
fn background_search_sees_edits_made_without_the_undo_stack() {
    let input: String = (0..1_000)
        .map(|i| format!("@Misc{{Key{i:05}, title={{Row {i:05}}}}}\n"))
        .collect();
    let (mut app, _dir) = review_app(&input);
    app.search_bar_state.query = "title:zebra".into();
    app.update_search();
    finish_review_search(&mut app);
    assert_eq!(app.visible_entry_count(), 0);
    // Mutate directly (no push_undo) and refresh: the cache must revalidate.
    app.database
        .entries
        .get_mut("Key00500")
        .unwrap()
        .fields
        .insert("title".into(), "Zebra".into());
    app.refresh_view();
    finish_review_search(&mut app);
    assert_eq!(app.selected_entry_key().as_deref(), Some("Key00500"));
}

// ── N02: export destinations ─────────────────────────────────────────────

const EXPORT_BIB: &str = "@Misc{A, title={Alpha}}\n";

fn export_to(app: &mut App, kind: ExportKind, path: &std::path::Path) {
    let path = path.to_str().unwrap();
    match kind {
        ExportKind::Settings => app.export_settings(path),
        ExportKind::Json => app.do_export_json(path),
        ExportKind::Ris => app.do_export_ris(path),
    }
}

const ALL_EXPORTS: [ExportKind; 3] = [ExportKind::Settings, ExportKind::Json, ExportKind::Ris];

#[test]
fn export_refuses_the_open_bibliography() {
    let (mut app, dir) = review_app(EXPORT_BIB);
    let bib = app.bib_path.clone();
    // The ordinary path, and the same file spelled through `.`.
    let dotted = dir.path().join(".").join("library.bib");
    for kind in ALL_EXPORTS {
        for target in [&bib, &dotted] {
            export_to(&mut app, kind, target);
            assert!(
                app.status_message
                    .as_deref()
                    .unwrap()
                    .contains("open bibliography"),
                "{kind:?} {target:?}: {:?}",
                app.status_message
            );
            assert!(app.dialog_state.is_none(), "no overwrite prompt is offered");
        }
    }
    assert_eq!(std::fs::read_to_string(&bib).unwrap(), EXPORT_BIB);
}

#[cfg(unix)]
#[test]
fn export_refuses_symlink_and_hard_link_aliases_of_the_bibliography() {
    let (mut app, dir) = review_app(EXPORT_BIB);
    let bib = app.bib_path.clone();
    let soft = dir.path().join("soft.json");
    let hard = dir.path().join("hard.json");
    std::os::unix::fs::symlink(&bib, &soft).unwrap();
    std::fs::hard_link(&bib, &hard).unwrap();
    for kind in ALL_EXPORTS {
        for alias in [&soft, &hard] {
            export_to(&mut app, kind, alias);
            assert!(
                app.status_message
                    .as_deref()
                    .unwrap()
                    .contains("open bibliography"),
                "{kind:?} {alias:?}: {:?}",
                app.status_message
            );
        }
    }
    assert_eq!(std::fs::read_to_string(&bib).unwrap(), EXPORT_BIB);
}

#[test]
fn export_refuses_a_new_bibliography_path_before_it_exists() {
    let dir = tempfile::tempdir().unwrap();
    let bib = dir.path().join("new.bib");
    let mut app = App::new(bib.clone(), default_config()).unwrap();
    app.do_export_json(bib.to_str().unwrap());
    assert!(app
        .status_message
        .as_deref()
        .unwrap()
        .contains("open bibliography"));
    assert!(!bib.exists());
}

#[test]
fn export_to_new_file_writes_readable_output() {
    let (mut app, dir) = review_app(EXPORT_BIB);
    let json = dir.path().join("out.json");
    let ris = dir.path().join("out.ris");
    let yaml = dir.path().join("out.yaml");
    export_to(&mut app, ExportKind::Json, &json);
    assert!(app
        .status_message
        .as_deref()
        .unwrap()
        .starts_with("Exported 1 entries as CSL-JSON"));
    let parsed: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&json).unwrap()).unwrap();
    assert_eq!(parsed[0]["id"], "A");
    export_to(&mut app, ExportKind::Ris, &ris);
    assert!(std::fs::read_to_string(&ris).unwrap().contains("ID  - A"));
    export_to(&mut app, ExportKind::Settings, &yaml);
    let _: crate::config::schema::Config =
        serde_yaml::from_str(&std::fs::read_to_string(&yaml).unwrap()).unwrap();
    // No temporary files are left beside the exports.
    let names: Vec<String> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        names.iter().all(|n| !n.starts_with(".bibtui-")),
        "{names:?}"
    );
}

#[test]
fn export_over_existing_file_asks_first() {
    let (mut app, dir) = review_app(EXPORT_BIB);
    let out = dir.path().join("out.json");
    std::fs::write(&out, "previous").unwrap();

    // Cancelling keeps the previous file.
    app.do_export_json(out.to_str().unwrap());
    assert_eq!(app.mode, InputMode::Dialog);
    assert!(matches!(
        app.pending_action,
        Some(PendingAction::OverwriteExport {
            kind: ExportKind::Json,
            ..
        })
    ));
    app.handle_action(Action::DialogCancel);
    assert_eq!(std::fs::read_to_string(&out).unwrap(), "previous");

    // Confirming replaces it.
    app.do_export_json(out.to_str().unwrap());
    app.handle_dialog_confirm();
    assert!(std::fs::read_to_string(&out)
        .unwrap()
        .contains("\"id\": \"A\""));
    assert!(app
        .status_message
        .as_deref()
        .unwrap()
        .starts_with("Exported 1 entries"));
}

#[test]
fn failed_export_preserves_the_previous_file() {
    let (mut app, dir) = review_app(EXPORT_BIB);
    let out = dir.path().join("out.ris");
    std::fs::write(&out, "previous").unwrap();
    app.save_io = Box::new(FailingSaveIo);
    app.do_export_ris(out.to_str().unwrap());
    app.handle_dialog_confirm();
    assert_eq!(std::fs::read_to_string(&out).unwrap(), "previous");
    assert!(app
        .status_message
        .as_deref()
        .unwrap()
        .starts_with("Export failed"));
}

#[test]
fn export_does_not_replace_a_file_created_after_the_check() {
    let (mut app, dir) = review_app(EXPORT_BIB);
    let out = dir.path().join("late.json");
    // The destination did not exist when chosen, then appeared.
    std::fs::write(&out, "someone else's").unwrap();
    app.write_export(ExportKind::Json, &out, false);
    assert_eq!(std::fs::read_to_string(&out).unwrap(), "someone else's");
    assert!(app
        .status_message
        .as_deref()
        .unwrap()
        .contains("created meanwhile"));
}

#[cfg(unix)]
#[test]
fn confirmed_overwrite_rechecks_for_a_bibliography_alias() {
    let (mut app, dir) = review_app(EXPORT_BIB);
    let out = dir.path().join("out.json");
    std::fs::write(&out, "previous").unwrap();
    app.do_export_json(out.to_str().unwrap());
    // Before confirming, the destination becomes a link to the library.
    std::fs::remove_file(&out).unwrap();
    std::os::unix::fs::symlink(&app.bib_path, &out).unwrap();
    app.handle_dialog_confirm();
    assert!(app
        .status_message
        .as_deref()
        .unwrap()
        .contains("open bibliography"));
    assert_eq!(std::fs::read_to_string(&app.bib_path).unwrap(), EXPORT_BIB);
}

// ── N05: shared attachments during entry deletion ────────────────────────

fn select_entry(app: &mut App, key: &str) {
    let idx = app.sorted_keys.iter().position(|k| k == key).unwrap();
    app.entry_list_state.select(idx);
    assert_eq!(app.selected_entry_key().as_deref(), Some(key));
}

fn delete_dialog_files(app: &App) -> Vec<(String, bool)> {
    match &app.dialog_state.as_ref().unwrap().kind {
        crate::tui::components::dialog::DialogKind::FileDeleteSelect { files, .. } => files.clone(),
        other => panic!("expected a file checklist, got {other:?}"),
    }
}

#[test]
fn deleting_one_owner_preserves_a_shared_attachment_by_default() {
    let (mut app, dir) =
        review_app("@Misc{A, file={:shared.pdf:PDF}}\n@Misc{B, file={:shared.pdf:PDF}}\n");
    let shared = dir.path().join("shared.pdf");
    std::fs::write(&shared, "attachment").unwrap();
    select_entry(&mut app, "A");
    app.handle_action(Action::DeleteEntry);
    assert_eq!(
        delete_dialog_files(&app),
        vec![("shared.pdf (also used by B)".to_string(), false)]
    );
    app.handle_action(Action::DialogConfirm);
    assert!(!app.database.entries.contains_key("A"));
    assert!(shared.exists(), "B still needs the file");
    assert_eq!(app.status_message.as_deref(), Some("Deleted 'A'"));
}

#[test]
fn shared_attachment_is_deleted_only_when_deliberately_checked() {
    let (mut app, dir) =
        review_app("@Misc{A, file={:shared.pdf:PDF}}\n@Misc{B, file={:shared.pdf:PDF}}\n");
    let shared = dir.path().join("shared.pdf");
    std::fs::write(&shared, "attachment").unwrap();
    select_entry(&mut app, "A");
    app.handle_action(Action::DeleteEntry);
    app.handle_action(Action::DialogToggle);
    app.handle_action(Action::DialogConfirm);
    assert!(!shared.exists());
    assert!(app
        .status_message
        .as_deref()
        .unwrap()
        .contains("and 1 file (file deletion cannot be undone)"));
}

#[cfg(unix)]
#[test]
fn shared_attachment_is_recognized_through_aliases() {
    let (mut app, dir) = review_app(
        "@Misc{A, file={:shared.pdf:PDF}}\n\
         @Misc{B, file={:sub/../shared.pdf:PDF}}\n\
         @Misc{C, file={:hard.pdf:PDF}}\n\
         @Misc{D, file={:soft.pdf:PDF}}\n",
    );
    let shared = dir.path().join("shared.pdf");
    std::fs::write(&shared, "attachment").unwrap();
    std::fs::create_dir(dir.path().join("sub")).unwrap();
    std::fs::hard_link(&shared, dir.path().join("hard.pdf")).unwrap();
    std::os::unix::fs::symlink(&shared, dir.path().join("soft.pdf")).unwrap();
    select_entry(&mut app, "A");
    app.handle_action(Action::DeleteEntry);
    assert_eq!(
        delete_dialog_files(&app),
        vec![(
            "shared.pdf (also used by B, C and 1 more)".to_string(),
            false
        )]
    );
}

#[test]
fn unshared_attachment_deletion_reports_permanence_and_failures() {
    let (mut app, dir) =
        review_app("@Misc{A, file={:a.pdf:PDF;:folder.pdf:PDF}}\n@Misc{B, title={Other}}\n");
    std::fs::write(dir.path().join("a.pdf"), "attachment").unwrap();
    // A directory cannot be removed with remove_file: an injected failure.
    std::fs::create_dir(dir.path().join("folder.pdf")).unwrap();
    select_entry(&mut app, "A");
    app.handle_action(Action::DeleteEntry);
    let title = match &app.dialog_state.as_ref().unwrap().kind {
        crate::tui::components::dialog::DialogKind::FileDeleteSelect { title, .. } => title.clone(),
        _ => unreachable!(),
    };
    assert!(title.contains("deleted permanently"), "{title}");
    app.handle_action(Action::DialogConfirm);
    let status = app.status_message.clone().unwrap();
    assert!(!app.database.entries.contains_key("A"));
    assert!(!dir.path().join("a.pdf").exists());
    assert!(status.starts_with("Deleted 'A' and 1 file"), "{status}");
    assert!(status.contains("could not delete"), "{status}");
    assert!(status.contains("folder.pdf"), "{status}");

    // Undo restores the entry (as the dialog says), not the deleted bytes.
    app.undo();
    assert!(app.database.entries.contains_key("A"));
    assert!(!dir.path().join("a.pdf").exists());
}

#[test]
fn single_unshared_attachment_option_names_permanence() {
    let (mut app, dir) = review_app("@Misc{A, file={:a.pdf:PDF}}\n");
    std::fs::write(dir.path().join("a.pdf"), "attachment").unwrap();
    app.handle_action(Action::DeleteEntry);
    match &app.dialog_state.as_ref().unwrap().kind {
        crate::tui::components::dialog::DialogKind::TypePicker { options, .. } => {
            assert!(options[0].contains("a.pdf"));
            assert!(options[0].contains("undo restores only the entry"));
            assert_eq!(options[1], "Delete entry only");
        }
        other => panic!("{other:?}"),
    }
}

// ── N06: shared attachments and manual / bulk filename sync ──────────────

const SHARED_BIB: &str = "@Misc{A, file={:shared.pdf:PDF}}\n@Misc{B, file={:shared.pdf:PDF}}\n";

#[test]
fn manual_sync_does_not_move_a_shared_attachment() {
    let (mut app, dir) = review_app(SHARED_BIB);
    let shared = dir.path().join("shared.pdf");
    std::fs::write(&shared, "attachment").unwrap();
    app.detail_entry_key = Some("A".into());
    app.sync_entry_filename();
    let status = app.status_message.clone().unwrap();
    assert!(status.contains("not renamed"), "{status}");
    assert!(status.contains("also used by B"), "{status}");
    assert!(shared.exists());
    assert!(!dir.path().join("A.pdf").exists());
    for key in ["A", "B"] {
        assert_eq!(app.database.entries[key].fields["file"], ":shared.pdf:PDF");
        assert!(!app.database.entries[key].dirty);
    }
}

#[test]
fn bulk_sync_and_preview_skip_shared_attachments_but_rename_others() {
    let (mut app, dir) =
        review_app("@Misc{A, file={:shared.pdf:PDF}}\n@Misc{B, file={:shared.pdf:PDF}}\n@Misc{C, file={:own.pdf:PDF}}\n");
    std::fs::write(dir.path().join("shared.pdf"), "attachment").unwrap();
    std::fs::write(dir.path().join("own.pdf"), "attachment").unwrap();
    // The preview lists only the rename that will actually happen.
    assert_eq!(
        app.compute_sync_renames(true),
        vec![("own.pdf".to_string(), "C.pdf".to_string())]
    );
    app.sync_filenames(true);
    assert!(dir.path().join("shared.pdf").exists());
    assert!(dir.path().join("C.pdf").exists());
    assert_eq!(app.database.entries["C"].fields["file"], ":C.pdf:PDF");
    assert_eq!(app.database.entries["B"].fields["file"], ":shared.pdf:PDF");
    let status = app.status_message.clone().unwrap();
    assert!(status.contains("also used by"), "{status}");
}

#[cfg(unix)]
#[test]
fn manual_sync_recognizes_a_hard_linked_shared_attachment() {
    let (mut app, dir) =
        review_app("@Misc{A, file={:shared.pdf:PDF}}\n@Misc{B, file={:link.pdf:PDF}}\n");
    let shared = dir.path().join("shared.pdf");
    std::fs::write(&shared, "attachment").unwrap();
    std::fs::hard_link(&shared, dir.path().join("link.pdf")).unwrap();
    app.detail_entry_key = Some("A".into());
    app.sync_entry_filename();
    assert!(shared.exists());
    assert!(!dir.path().join("A.pdf").exists());
}

#[test]
fn manual_sync_renames_once_the_file_is_no_longer_shared() {
    let (mut app, dir) = review_app(SHARED_BIB);
    std::fs::write(dir.path().join("shared.pdf"), "attachment").unwrap();
    let b = app.database.entries.get_mut("B").unwrap();
    b.fields.shift_remove("file");
    b.dirty = true;
    app.detail_entry_key = Some("A".into());
    app.sync_entry_filename();
    assert_eq!(
        app.status_message.as_deref(),
        Some("File renamed to match citation key")
    );
    assert!(dir.path().join("A.pdf").exists());
}

// ── N07/N08: background DOI lookups and imports ──────────────────────────

const BACKGROUND_BIB: &str =
    "@Article{A, title={First Study}, author={Smith, Ann}, year={2020}, note={n}}\n\
@Article{B, title={Second Study}, author={Jones, Bob}, year={2021}}\n";

/// Start a DOI lookup for `key` whose answer the test controls.
fn start_doi_lookup(app: &mut App, key: &str) -> std::sync::mpsc::Sender<DoiFetchResult> {
    let (tx, rx) = std::sync::mpsc::channel();
    let lookup = super::import::DoiLookup::of(&app.database.entries[key]);
    app.pending_doi_fetch = Some((lookup, rx));
    tx
}

/// Finish a background import with `result` and poll once.
fn finish_import(app: &mut App, result: crate::util::import::ImportResult) {
    let (tx, rx) = std::sync::mpsc::channel();
    tx.send(result).unwrap();
    app.pending_import = Some(rx);
    app.poll_background_tasks();
}

fn background_import() -> crate::util::import::ImportResult {
    let mut fields = IndexMap::new();
    fields.insert("title".to_string(), "imported work".to_string());
    fields.insert("author".to_string(), "Lee, Cy".to_string());
    fields.insert("year".to_string(), "2024".to_string());
    Ok(crate::util::import::ImportedEntry::new("article", fields))
}

fn background_import_error() -> crate::util::import::ImportResult {
    Err(crate::util::import::ImportError::Network(
        "timed out".into(),
    ))
}

fn field(app: &App, key: &str, name: &str) -> Option<String> {
    app.database.entries[key].fields.get(name).cloned()
}

fn status(app: &App) -> &str {
    app.status_message.as_deref().unwrap_or("")
}

/// Confirming an edit regenerates a key that does not match the template,
/// so tests that commit an edit first give every entry its template key.
fn background_app_with_template_keys() -> (App, tempfile::TempDir) {
    let (mut app, dir) = review_app(BACKGROUND_BIB);
    app.regen_all_citekeys_impl(false);
    (app, dir)
}

fn key_with_title(app: &App, title: &str) -> String {
    app.database
        .entries
        .values()
        .find(|e| e.fields.get("title").map(String::as_str) == Some(title))
        .unwrap()
        .citation_key
        .clone()
}

/// Open `key`'s detail view and start editing `name` with draft `draft`.
fn edit_draft(app: &mut App, key: &str, name: &str, draft: &str) {
    use crate::tui::components::entry_detail::DisplayItem;
    select_entry(app, key);
    app.handle_action(Action::OpenDetail);
    let detail = app.detail_state.as_mut().unwrap();
    let idx = detail
        .display_fields
        .iter()
        .position(|d| matches!(d, DisplayItem::Field { name: n, .. } if n == name))
        .unwrap();
    detail.select(idx);
    app.handle_action(Action::EditField);
    assert_eq!(app.mode, InputMode::Editing);
    let editor = app.field_editor_state.as_mut().unwrap();
    editor.value = draft.to_string();
    editor.cursor = draft.len();
}

#[test]
fn doi_result_applies_to_unchanged_entry_as_one_undo_step() {
    let (mut app, _dir) = review_app(BACKGROUND_BIB);
    let tx = start_doi_lookup(&mut app, "A");
    let depth = app.undo_stack.len();
    tx.send(Ok(unique_doi(
        "10.1000/found",
        "https://publisher.example/a",
    )))
    .unwrap();
    assert!(app.poll_background_tasks());
    assert_eq!(field(&app, "A", "doi").as_deref(), Some("10.1000/found"));
    assert_eq!(
        field(&app, "A", "url").as_deref(),
        Some("https://publisher.example/a")
    );
    assert_eq!(status(&app), "Found DOI: 10.1000/found");
    assert_eq!(app.undo_stack.len(), depth + 1, "one undo step");

    app.handle_action(Action::Undo);
    assert_eq!(field(&app, "A", "doi"), None);
    assert_eq!(field(&app, "A", "url"), None);
}

#[test]
fn doi_result_never_overwrites_a_manual_doi() {
    let (mut app, _dir) = review_app(BACKGROUND_BIB);
    let tx = start_doi_lookup(&mut app, "A");
    review_edit(&mut app, "A", "doi", "10.1000/manual");
    let depth = app.undo_stack.len();
    tx.send(Ok(unique_doi("10.1000/stale", ""))).unwrap();
    app.poll_background_tasks();
    assert_eq!(field(&app, "A", "doi").as_deref(), Some("10.1000/manual"));
    assert_eq!(app.undo_stack.len(), depth);
    assert!(status(&app).contains("not applied"), "{}", status(&app));
    assert!(
        status(&app).contains("doi of 'A' changed"),
        "{}",
        status(&app)
    );
    assert!(
        status(&app).contains("found 10.1000/stale"),
        "{}",
        status(&app)
    );
}

#[test]
fn doi_result_discarded_after_title_or_author_change() {
    for (name, value) in [("title", "Revised Study"), ("author", "Smith, Anne")] {
        let (mut app, _dir) = review_app(BACKGROUND_BIB);
        let tx = start_doi_lookup(&mut app, "A");
        review_edit(&mut app, "A", name, value);
        tx.send(Ok(unique_doi("10.1000/stale", ""))).unwrap();
        app.poll_background_tasks();
        assert_eq!(field(&app, "A", "doi"), None, "{name}");
        assert!(
            status(&app).contains(&format!("the {name} of 'A' changed")),
            "{}",
            status(&app)
        );
    }
}

#[test]
fn doi_result_discarded_after_key_change_deletion_or_key_reuse() {
    // Key regenerated: the entry now lives under another key.
    let (mut app, _dir) = review_app(BACKGROUND_BIB);
    let tx = start_doi_lookup(&mut app, "A");
    let mut entry = app.database.entries.shift_remove("A").unwrap();
    entry.citation_key = "Smith2020".into();
    app.database.entries.insert("Smith2020".into(), entry);
    let depth = app.undo_stack.len();
    tx.send(Ok(unique_doi("10.1000/x", ""))).unwrap();
    app.poll_background_tasks();
    assert_eq!(field(&app, "Smith2020", "doi"), None);
    assert_eq!(app.undo_stack.len(), depth);
    assert!(
        status(&app).contains("'A' was renamed or deleted"),
        "{}",
        status(&app)
    );

    // Deleted: no undo record, no success message.
    let (mut app, _dir) = review_app(BACKGROUND_BIB);
    let tx = start_doi_lookup(&mut app, "A");
    app.delete_entry("A");
    let depth = app.undo_stack.len();
    tx.send(Ok(unique_doi("10.1000/x", ""))).unwrap();
    app.poll_background_tasks();
    assert!(!app.database.entries.contains_key("A"));
    assert_eq!(app.undo_stack.len(), depth);
    assert!(!status(&app).contains("Found DOI"), "{}", status(&app));

    // Key reused by a different entry: the answer is not for it.
    let (mut app, _dir) = review_app(BACKGROUND_BIB);
    let tx = start_doi_lookup(&mut app, "A");
    let mut other = app.database.entries.shift_remove("A").unwrap();
    other.fields.insert("title".into(), "Unrelated Work".into());
    app.database.entries.insert("A".into(), other);
    tx.send(Ok(unique_doi("10.1000/x", ""))).unwrap();
    app.poll_background_tasks();
    assert_eq!(field(&app, "A", "doi"), None);
    assert!(status(&app).contains("not applied"), "{}", status(&app));
}

#[test]
fn doi_result_applies_after_change_is_undone() {
    let (mut app, _dir) = review_app(BACKGROUND_BIB);
    let tx = start_doi_lookup(&mut app, "A");
    review_edit(&mut app, "A", "title", "Revised Study");
    app.handle_action(Action::Undo);
    tx.send(Ok(unique_doi("10.1000/found", ""))).unwrap();
    app.poll_background_tasks();
    assert_eq!(field(&app, "A", "doi").as_deref(), Some("10.1000/found"));
}

#[test]
fn doi_result_waits_for_open_editor_on_the_same_entry() {
    let (mut app, _dir) = background_app_with_template_keys();
    let a = key_with_title(&app, "First Study");
    edit_draft(&mut app, &a, "note", "draft text");
    let tx = start_doi_lookup(&mut app, &a);
    tx.send(Ok(unique_doi("10.1000/found", ""))).unwrap();
    app.poll_background_tasks();
    assert_eq!(app.mode, InputMode::Editing);
    assert_eq!(app.field_editor_state.as_ref().unwrap().value, "draft text");
    assert_eq!(field(&app, &a, "doi"), None, "applied only after the edit");

    app.handle_action(Action::ConfirmEdit);
    assert_eq!(field(&app, &a, "note").as_deref(), Some("draft text"));
    app.poll_background_tasks();
    // The note is not a looked-up field, so the answer still applies.
    assert_eq!(field(&app, &a, "doi").as_deref(), Some("10.1000/found"));
}

#[test]
fn second_doi_lookup_refused_while_a_result_waits() {
    let (mut app, _dir) = review_app(BACKGROUND_BIB);
    edit_draft(&mut app, "A", "note", "x");
    let tx = start_doi_lookup(&mut app, "A");
    tx.send(Ok(unique_doi("10.1000/found", ""))).unwrap();
    app.poll_background_tasks();
    assert!(app.finished_doi_fetch.is_some());
    app.handle_action(Action::CancelEdit);
    app.mode = InputMode::Editing; // still busy, e.g. another prompt
    app.start_fetch_doi();
    assert_eq!(status(&app), "DOI lookup already in progress");
}

#[test]
fn import_waits_for_field_edit_then_opens_and_undoes() {
    let (mut app, _dir) = background_app_with_template_keys();
    let a = key_with_title(&app, "First Study");
    edit_draft(&mut app, &a, "note", "draft text");
    finish_import(&mut app, background_import());
    assert_eq!(app.mode, InputMode::Editing);
    assert_eq!(app.detail_entry_key.as_deref(), Some(a.as_str()));
    assert_eq!(app.field_editor_state.as_ref().unwrap().value, "draft text");
    assert_eq!(app.database.entries.len(), 2, "not added yet");

    app.handle_action(Action::ConfirmEdit);
    assert_eq!(field(&app, &a, "note").as_deref(), Some("draft text"));
    assert!(app.poll_background_tasks());
    assert_eq!(app.database.entries.len(), 3);
    let imported = app.detail_entry_key.clone().unwrap();
    assert_ne!(imported, a);
    assert_eq!(app.mode, InputMode::Detail);

    app.handle_action(Action::Undo);
    assert!(!app.database.entries.contains_key(&imported));
    assert_eq!(field(&app, &a, "note").as_deref(), Some("draft text"));
}

#[test]
fn failed_import_waits_for_export_path_prompt() {
    let (mut app, _dir) = review_app(BACKGROUND_BIB);
    app.command_palette_state.input = "settings-export yaml".into();
    app.execute_command();
    assert_eq!(app.mode, InputMode::Editing);
    finish_import(&mut app, background_import_error());
    assert!(matches!(
        app.pending_action,
        Some(PendingAction::ExportSettings)
    ));
    assert!(app.dialog_state.is_none());
    assert_eq!(
        app.field_editor_state.as_ref().unwrap().value,
        "bibtui.yaml"
    );

    app.handle_action(Action::CancelEdit);
    app.poll_background_tasks();
    assert_eq!(app.mode, InputMode::Dialog);
    assert!(matches!(
        app.pending_action,
        Some(PendingAction::DismissMessage)
    ));
}

#[test]
fn import_waits_for_confirm_dialog_merge_review_and_settings() {
    // A delete confirmation keeps its target and continuation.
    let (mut app, _dir) = review_app(BACKGROUND_BIB);
    app.start_delete_entry();
    assert_eq!(app.mode, InputMode::Dialog);
    let pending = format!("{:?}", app.pending_action);
    finish_import(&mut app, background_import());
    assert_eq!(app.mode, InputMode::Dialog);
    assert_eq!(format!("{:?}", app.pending_action), pending);
    assert_eq!(app.database.entries.len(), 2);
    app.handle_action(Action::DialogCancel);
    app.poll_background_tasks();
    assert_eq!(app.database.entries.len(), 3);

    // Duplicate review stays open.
    let (mut app, _dir) = review_app(BACKGROUND_BIB);
    app.open_duplicate_review();
    finish_import(&mut app, background_import());
    assert_eq!(app.mode, InputMode::DedupReview);
    assert_eq!(app.database.entries.len(), 2);
    app.close_dedup();
    app.poll_background_tasks();
    assert_eq!(app.database.entries.len(), 3);
    assert_eq!(app.mode, InputMode::Detail);

    // Settings stay open, and the error is shown after leaving them.
    let (mut app, _dir) = review_app(BACKGROUND_BIB);
    app.handle_action(Action::EnterSettings);
    assert_eq!(app.mode, InputMode::Settings);
    finish_import(&mut app, background_import_error());
    assert_eq!(app.mode, InputMode::Settings);
    assert!(app.dialog_state.is_none());
    app.handle_action(Action::ExitSettings);
    app.poll_background_tasks();
    assert_eq!(app.mode, InputMode::Dialog);
}

#[test]
fn second_import_refused_while_a_result_waits() {
    let (mut app, _dir) = review_app(BACKGROUND_BIB);
    app.open_duplicate_review();
    finish_import(&mut app, background_import());
    assert!(app.finished_import.is_some());
    app.spawn_import("10.1000/next".into());
    assert_eq!(status(&app), "Import already in progress — please wait");
    assert!(app.pending_import.is_none(), "no second fetch started");
}

// ── N09: metadata DOI matches are checked before they are assigned ───────

fn choose_doi_app() -> (App, tempfile::TempDir) {
    review_app(BACKGROUND_BIB)
}

fn candidate(doi: &str) -> crate::util::import::crossref::DoiCandidate {
    crate::util::import::crossref::DoiCandidate::for_test(doi, "")
}

fn weak_match(dois: &[&str]) -> crate::util::import::crossref::MetadataMatch {
    crate::util::import::crossref::MetadataMatch::Choose {
        candidates: dois.iter().map(|d| candidate(d)).collect(),
        ambiguous: false,
    }
}

#[test]
fn weak_doi_matches_ask_and_assign_only_the_chosen_one() {
    let (mut app, _dir) = choose_doi_app();
    let tx = start_doi_lookup(&mut app, "A");
    tx.send(Ok(weak_match(&["10.1000/one", "10.1000/two"])))
        .unwrap();
    app.poll_background_tasks();
    assert_eq!(app.mode, InputMode::Dialog);
    assert_eq!(
        field(&app, "A", "doi"),
        None,
        "nothing assigned before choosing"
    );
    let options = match &app.dialog_state.as_ref().unwrap().kind {
        DialogKind::TypePicker { options, .. } => options.clone(),
        other => panic!("expected a picker, got {other:?}"),
    };
    assert_eq!(options.len(), 3);
    assert!(options[1].ends_with("10.1000/two"), "{options:?}");
    assert!(options[2].starts_with("None of these"));

    app.dialog_state.as_mut().unwrap().select(1);
    app.handle_action(Action::DialogConfirm);
    assert_eq!(field(&app, "A", "doi").as_deref(), Some("10.1000/two"));
    assert_eq!(status(&app), "Found DOI: 10.1000/two");
}

#[test]
fn rejecting_doi_candidates_leaves_entry_unchanged() {
    for how in ["none", "cancel"] {
        let (mut app, _dir) = choose_doi_app();
        let depth = app.undo_stack.len();
        let tx = start_doi_lookup(&mut app, "A");
        tx.send(Ok(weak_match(&["10.1000/one"]))).unwrap();
        app.poll_background_tasks();
        if how == "none" {
            app.dialog_state.as_mut().unwrap().select(1);
            app.handle_action(Action::DialogConfirm);
            assert_eq!(status(&app), "No DOI assigned");
        } else {
            app.handle_action(Action::DialogCancel);
        }
        assert_eq!(field(&app, "A", "doi"), None, "{how}");
        assert_eq!(app.undo_stack.len(), depth, "{how}");
        assert!(!app.database.entries["A"].dirty, "{how}");
    }
}

#[test]
fn chosen_doi_rechecks_entry_before_assigning() {
    let (mut app, _dir) = choose_doi_app();
    let tx = start_doi_lookup(&mut app, "A");
    tx.send(Ok(weak_match(&["10.1000/one"]))).unwrap();
    app.poll_background_tasks();
    // The entry changes while the picker is open (e.g. by another path).
    review_edit(&mut app, "A", "doi", "10.1000/manual");
    app.handle_action(Action::DialogConfirm);
    assert_eq!(field(&app, "A", "doi").as_deref(), Some("10.1000/manual"));
    assert!(
        status(&app).starts_with("DOI not assigned"),
        "{}",
        status(&app)
    );
}

#[test]
fn unique_doi_match_is_assigned_without_asking() {
    let (mut app, _dir) = choose_doi_app();
    let tx = start_doi_lookup(&mut app, "A");
    tx.send(Ok(unique_doi("10.1000/sure", ""))).unwrap();
    app.poll_background_tasks();
    assert_ne!(app.mode, InputMode::Dialog);
    assert_eq!(field(&app, "A", "doi").as_deref(), Some("10.1000/sure"));
}

// ── N11: "not duplicates" marks follow key changes ───────────────────────

/// Two copies of one work that the user has said are different (e.g. a
/// preprint and the article), plus an unrelated entry.
const NOT_DUP_BIB: &str =
    "@Article{A, title={Neutron Transport Methods}, author={Smith, Ann}, year={2020}}\n\
@Article{B, title={Neutron Transport Methods}, author={Smith, Ann}, year={2020}}\n\
@Article{C, title={Unrelated Topic}, author={Jones, Bob}, year={2011}}\n";

fn marked_app() -> (App, tempfile::TempDir) {
    let (mut app, dir) = review_app(NOT_DUP_BIB);
    assert_eq!(app.duplicate_clusters().len(), 1, "A and B look alike");
    app.mark_not_duplicates(&["A".to_string(), "B".to_string()]);
    assert!(app.duplicate_clusters().is_empty());
    (app, dir)
}

fn regen_key_of(app: &mut App, key: &str) {
    select_entry(app, key);
    app.handle_action(Action::OpenDetail);
    app.regen_citekey();
    app.handle_action(Action::CloseDetail);
}

#[test]
fn not_duplicate_mark_follows_single_rekeying_and_undo() {
    let (mut app, _dir) = marked_app();
    regen_key_of(&mut app, "A");
    let a = app
        .database
        .entries
        .keys()
        .find(|k| *k != "B" && *k != "C")
        .unwrap()
        .clone();
    assert_ne!(a, "A");
    assert_eq!(
        app.not_duplicate_sets(),
        vec![{
            let mut s = vec![a.clone(), "B".to_string()];
            s.sort();
            s
        }]
    );
    assert!(app.duplicate_clusters().is_empty(), "decision kept");

    app.handle_action(Action::Undo);
    assert!(app.database.entries.contains_key("A"));
    assert_eq!(
        app.not_duplicate_sets(),
        vec![vec!["A".to_string(), "B".to_string()]]
    );
    assert!(app.duplicate_clusters().is_empty());
}

#[test]
fn not_duplicate_mark_follows_bulk_rekeying_with_collision_suffixes() {
    let (mut app, _dir) = marked_app();
    app.regen_all_citekeys();
    // Same metadata, so the keys collide and one gets a suffix.
    let keys: Vec<String> = app.database.entries.keys().cloned().collect();
    assert!(!keys.contains(&"A".to_string()) && !keys.contains(&"B".to_string()));
    let sets = app.not_duplicate_sets();
    assert_eq!(sets.len(), 1);
    assert!(
        sets[0].iter().all(|k| app.database.entries.contains_key(k)),
        "{sets:?}"
    );
    assert!(app.duplicate_clusters().is_empty(), "decision kept");

    app.handle_action(Action::Undo);
    assert_eq!(
        app.not_duplicate_sets(),
        vec![vec!["A".to_string(), "B".to_string()]]
    );
}

#[test]
fn not_duplicate_mark_survives_save_time_rekeying_and_reload() {
    let (mut app, _dir) = marked_app();
    app.config.save.save_action_regenerate_citekeys = true;
    assert!(app.save(), "{:?}", app.status_message);
    assert!(app.duplicate_clusters().is_empty());
    let reloaded = App::new(app.bib_path.clone(), app.config.clone()).unwrap();
    assert!(
        !reloaded.database.entries.contains_key("A"),
        "keys were regenerated"
    );
    assert_eq!(reloaded.not_duplicate_sets(), app.not_duplicate_sets());
    assert!(
        reloaded.duplicate_clusters().is_empty(),
        "decision kept after reload"
    );
}

#[test]
fn not_duplicate_marks_use_one_simultaneous_mapping() {
    let (mut app, _dir) = marked_app();
    app.mark_not_duplicates(&["B".to_string(), "C".to_string()]);
    // A and B swap keys: references move once, not twice.
    let mapping = std::collections::HashMap::from([
        ("A".to_string(), "B".to_string()),
        ("B".to_string(), "A".to_string()),
        ("C".to_string(), "D".to_string()),
    ]);
    app.update_key_references(&mapping);
    assert_eq!(
        app.not_duplicate_sets(),
        vec![
            vec!["A".to_string(), "B".to_string()],
            vec!["A".to_string(), "D".to_string()],
        ]
    );
}

#[test]
fn merging_retires_keys_in_not_duplicate_marks() {
    // Merging C into A keeps the A/B decision on the surviving key.
    let (mut app, _dir) = marked_app();
    app.mark_not_duplicates(&["B".to_string(), "C".to_string()]);
    app.open_merge(&["A".to_string(), "C".to_string()], vec![], false);
    app.dedup_merge_state.as_mut().unwrap().survivor = 0;
    app.apply_merge();
    assert!(!app.database.entries.contains_key("C"));
    assert_eq!(
        app.not_duplicate_sets(),
        vec![vec!["A".to_string(), "B".to_string()]]
    );

    app.handle_action(Action::Undo);
    assert!(app.database.entries.contains_key("C"));
    assert_eq!(app.not_duplicate_sets().len(), 2);

    // Merging the marked entries themselves drops the now-empty mark.
    let (mut app, _dir) = marked_app();
    app.open_merge(&["A".to_string(), "B".to_string()], vec![], false);
    app.apply_merge();
    assert!(app.not_duplicate_sets().is_empty());
    assert_eq!(
        app.meta_comment_text(super::dedup::NOT_DUPLICATES_META),
        None
    );
}

#[test]
fn deleting_a_marked_entry_keeps_the_mark_for_undo() {
    let (mut app, _dir) = marked_app();
    app.delete_entry("A");
    assert_eq!(app.not_duplicate_sets().len(), 1);
    app.handle_action(Action::Undo);
    assert!(app.duplicate_clusters().is_empty());
}
