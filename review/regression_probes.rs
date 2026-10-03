// Review regression probes: append inside src/app/tests.rs in a disposable copy.
// Assertions describe desired behavior; these deliberately fail on the reviewed revision.

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
fn review_key_rename_must_preserve_raw_field_expressions() {
    let (mut app, _dir) = review_app(
        "@String{j = {Journal}}\n@Misc{A, journal=j, title={Hello} # {World}, year={2020}}\n",
    );
    app.config
        .citekey
        .templates
        .insert("misc".into(), "New[year]".into());
    app.config.save.save_action_regenerate_citekeys = true;
    app.save();
    let raw = parse_bib_file(&std::fs::read_to_string(&app.bib_path).unwrap()).unwrap();
    let e = raw
        .items
        .iter()
        .find_map(|i| {
            if let RawItem::Entry(e) = i {
                Some(e)
            } else {
                None
            }
        })
        .unwrap();
    let journal = &e.fields.iter().find(|f| f.name == "journal").unwrap().value;
    assert!(
        matches!(journal, RawFieldValue::Bare(_)),
        "macro reference became a literal: {:?}",
        journal
    );
}

#[test]
fn review_blank_lines_inside_fields_must_survive_save() {
    let input = "@Misc{A, abstract={First\n\n\nSecond}}\n";
    let (mut app, _dir) = review_app(input);
    app.save();
    assert_eq!(std::fs::read_to_string(&app.bib_path).unwrap(), input);
}

#[test]
fn review_filter_must_remain_valid_after_delete() {
    let (mut app, _dir) =
        review_app("@Misc{A, title={Alpha}}\n@Misc{B, title={Beta}}\n@Misc{C, title={Gamma}}\n");
    app.search_bar_state.query = "key:B".into();
    app.update_search();
    app.delete_entry("B");
    assert!(
        app.visible_entries().is_empty(),
        "stale index exposed an unrelated entry"
    );
}

#[test]
fn review_documented_multi_field_search_must_match() {
    let (mut app, _dir) = review_app("@Article{A, author={Smith, John}, year={2020}}\n");
    app.search_bar_state.query = "author:smith year:2020".into();
    app.update_search();
    assert_eq!(app.visible_entry_count(), 1);
}

#[test]
fn review_sort_comparator_must_be_transitive() {
    let values = ["2", "10", "1a"];
    for a in values { for b in values { for c in values {
        if compare_sort_values("title", a, b).is_le() && compare_sort_values("title", b, c).is_le() {
            assert!(compare_sort_values("title", a, c).is_le());
        }
    } } }
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
fn review_pdf_doi_scan_must_not_panic_on_unicode() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("unicode.pdf");
    // Long DOI-like data exercises the 200-byte fallback boundary.
    std::fs::write(&path, format!("%PDF-1.4\n10.1234/x{}", "é".repeat(110))).unwrap();
    let _ = crate::util::import::pdf::PdfFetcher::extract_doi_from_path(&path);
}

#[test]
fn review_key_regeneration_must_update_crossref() {
    let (mut app, _dir) = review_app("@Proceedings{Parent, year={2020}}\n@InProceedings{Child, title={Chapter}, crossref={Parent}}\n");
    app.config
        .citekey
        .templates
        .insert("proceedings".into(), "New[year]".into());
    app.detail_entry_key = Some("Parent".into());
    app.regen_citekey();
    assert!(app.database.entries.contains_key("New2020"));
    assert_eq!(app.database.entries["Child"].fields["crossref"], "New2020");
}

#[test]
fn review_pdf_lowercase_offsets_must_not_panic() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("unicode.pdf");
    std::fs::write(&path, format!("%PDF-1.4\n{}doi:10.1234/a", "K".repeat(10))).unwrap();
    let _ = crate::util::import::pdf::PdfFetcher::extract_doi_from_path(&path);
}

#[test]
fn review_completion_must_return_a_prefix_of_every_candidate() {
    assert_eq!(longest_common_prefix(&["é.pdf".into(), "ê.pdf".into()]), "");
}

#[test]
fn review_key_release_must_not_perform_actions() {
    use crossterm::event::{KeyEventKind, KeyModifiers};
    let (mut app, _dir) = review_app("@Misc{A, title={Alpha}}\n@Misc{B, title={Beta}}\n");
    app.handle_event(Event::Key(KeyEvent::new_with_kind(
        KeyCode::Char('j'),
        KeyModifiers::NONE,
        KeyEventKind::Release,
    )));
    assert_eq!(
        app.entry_list_state.selected(),
        0,
        "key release must not move selection"
    );
}
