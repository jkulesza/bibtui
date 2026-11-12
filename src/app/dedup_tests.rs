//! Duplicate review, merge, not-duplicate marks, and the import check.

use super::*;
use crate::config::defaults::default_config;
use crate::tui::keybindings::InputMode;
use pretty_assertions::assert_eq;
use std::io::Write;
use tempfile::NamedTempFile;

/// Unusual alignment and a bare year: re-serializing would change both.
const UNTOUCHED: &str = "@Book{Keep1999,\n  author = {Keeper, K.},\n  title  =   {Untouched Book},\n  year   = 1999,\n}";

fn dup_bib() -> String {
    format!(
        r#"@Article{{Smith2020,
  author  = {{Smith, John}},
  title   = {{Neutron Transport in Reactor Cores}},
  journal = {{Phys. Rev. D}},
  year    = {{2020}},
  groups  = {{G1}},
}}

@Article{{Smith2020a,
  author  = {{John Smith}},
  title   = {{{{Neutron}} Transport in Reactor Cores}},
  journal = {{Physical Review D}},
  year    = {{2020}},
  doi     = {{10.1/x}},
  file    = {{:PDF/a.pdf:PDF}},
  groups  = {{G2}},
}}

@InProceedings{{Other2021,
  author   = {{Doe, Jane}},
  title    = {{Something Else Entirely}},
  year     = {{2021}},
  crossref = {{Smith2020}},
}}

{UNTOUCHED}
"#
    )
}

fn make_app_from(bib: &str) -> (App, NamedTempFile) {
    let mut tmp = NamedTempFile::new().unwrap();
    write!(tmp, "{}", bib).unwrap();
    tmp.flush().unwrap();
    let mut cfg = default_config();
    cfg.save.entry_sort_order = "none".to_string();
    cfg.save.save_action_regenerate_citekeys = false;
    cfg.general.backup_on_save = false;
    let app = App::new(tmp.path().to_path_buf(), cfg).unwrap();
    (app, tmp)
}

fn make_app() -> (App, NamedTempFile) {
    make_app_from(&dup_bib())
}

/// An entry's (type, fields, groups).
type EntryShape = (String, Vec<(String, String)>, Vec<String>);

/// The shape of every entry, keyed by citation key.
fn snapshot(app: &App) -> std::collections::BTreeMap<String, EntryShape> {
    app.database
        .entries
        .iter()
        .map(|(k, e)| {
            (
                k.clone(),
                (
                    e.entry_type.display_name().to_string(),
                    e.fields
                        .iter()
                        .map(|(a, b)| (a.clone(), b.clone()))
                        .collect(),
                    e.group_memberships.clone(),
                ),
            )
        })
        .collect()
}

#[test]
fn load_reports_duplicates_in_status_bar() {
    let (mut app, _tmp) = make_app();
    app.wait_for_duplicate_scan();
    let status = app.status_message.as_deref().unwrap_or("");
    assert!(
        status.contains("1 possible duplicate cluster — press U"),
        "{status}"
    );
}

#[test]
fn load_check_can_be_disabled() {
    let mut tmp = NamedTempFile::new().unwrap();
    write!(tmp, "{}", dup_bib()).unwrap();
    let mut cfg = default_config();
    cfg.duplicates.check_on_load = false;
    let app = App::new(tmp.path().to_path_buf(), cfg).unwrap();
    assert!(app.duplicate_scan.is_none());
    assert!(app.status_message.is_none());
}

#[test]
fn review_lists_cluster_with_reasons() {
    let (mut app, _tmp) = make_app();
    app.handle_action(Action::FindDuplicates);
    app.wait_for_duplicate_scan();
    assert_eq!(app.mode, InputMode::DedupReview);
    let review = app.dedup_review_state.as_ref().unwrap();
    assert_eq!(review.clusters.len(), 1);
    assert_eq!(review.clusters[0].keys, vec!["Smith2020", "Smith2020a"]);
    assert_eq!(review.clusters[0].reasons, vec!["title 100% similar"]);
    assert!(review.clusters[0].summaries[0].contains("(2020) [Article]"));

    // `:dups` opens the same list.
    app.handle_action(Action::CloseDedup);
    assert_eq!(app.mode, InputMode::Normal);
    app.command_palette_state.input = "dups".into();
    app.execute_command();
    app.wait_for_duplicate_scan();
    assert_eq!(app.mode, InputMode::DedupReview);
}

#[test]
fn merge_combines_fields_groups_files_and_crossrefs() {
    let (mut app, _tmp) = make_app();
    app.handle_action(Action::FindDuplicates);
    app.wait_for_duplicate_scan();
    app.handle_action(Action::DedupReconcile);
    assert_eq!(app.mode, InputMode::DedupMerge);
    {
        let merge = app.dedup_merge_state.as_ref().unwrap();
        assert_eq!(merge.final_key(), "Smith2020a");
        // "Smith, John" vs "John Smith" is a conflict too, and so is a
        // title that differs only in case-protecting braces.
        assert_eq!(merge.rows[merge.cursor].field, "author");
    }
    app.handle_action(Action::MoveDown);
    app.handle_action(Action::MoveDown);
    assert_eq!(
        app.dedup_merge_state
            .as_ref()
            .map(|m| m.rows[m.cursor].field.clone()),
        Some("journal".to_string())
    );
    // Keep Smith2020's journal abbreviation.
    app.handle_action(Action::DedupPick(0));
    app.handle_action(Action::ApplyMerge);

    assert!(!app.database.entries.contains_key("Smith2020"));
    let merged = &app.database.entries["Smith2020a"];
    assert_eq!(merged.fields["journal"], "Phys. Rev. D");
    assert_eq!(merged.fields["doi"], "10.1/x");
    assert_eq!(merged.fields["file"], ":PDF/a.pdf:PDF");
    assert_eq!(merged.group_memberships, vec!["G2", "G1"]);
    assert_eq!(merged.fields["groups"], "G2,G1");
    assert_eq!(
        app.database.entries["Other2021"].fields["crossref"],
        "Smith2020a"
    );

    // The retired-keys report is shown; dismissing it closes the (now
    // empty) review.
    assert_eq!(app.mode, InputMode::Dialog);
    match &app.dialog_state.as_ref().unwrap().kind {
        DialogKind::Message { message, .. } => assert_eq!(message, "Smith2020 → Smith2020a"),
        other => panic!("unexpected dialog {other:?}"),
    }
    app.handle_action(Action::DialogConfirm);
    assert_eq!(app.mode, InputMode::Normal);
    assert!(app.dedup_review_state.is_none());
}

#[test]
fn merge_is_a_single_undo_step() {
    let (mut app, _tmp) = make_app();
    let before = snapshot(&app);
    app.handle_action(Action::FindDuplicates);
    app.wait_for_duplicate_scan();
    app.handle_action(Action::DedupReconcile);
    app.handle_action(Action::DedupCycleSurvivor); // keep Smith2020's key
    app.handle_action(Action::ApplyMerge);
    app.handle_action(Action::DialogConfirm);
    assert_eq!(app.database.entries.len(), 3);
    assert!(app.dirty);

    app.handle_action(Action::Undo);
    assert_eq!(snapshot(&app), before);
    assert!(!app.dirty);
}

#[test]
fn keeping_other_member_and_regenerating_key() {
    let (mut app, _tmp) = make_app();
    app.handle_action(Action::FindDuplicates);
    app.wait_for_duplicate_scan();
    app.handle_action(Action::DedupReconcile);
    app.handle_action(Action::DedupCycleSurvivor);
    assert_eq!(
        app.dedup_merge_state.as_ref().unwrap().final_key(),
        "Smith2020"
    );
    app.handle_action(Action::DedupRegenKey);
    let key = app
        .dedup_merge_state
        .as_ref()
        .unwrap()
        .final_key()
        .to_string();
    assert!(key.starts_with("Article_2020"), "{key}");
    app.handle_action(Action::ApplyMerge);
    assert!(app.database.entries.contains_key(&key));
    assert!(!app.database.entries.contains_key("Smith2020"));
    assert!(!app.database.entries.contains_key("Smith2020a"));
    match &app.dialog_state.as_ref().unwrap().kind {
        DialogKind::Message { message, .. } => {
            assert_eq!(message, &format!("Smith2020a → {key}\nSmith2020 → {key}"))
        }
        other => panic!("unexpected dialog {other:?}"),
    }
    assert_eq!(app.database.entries["Other2021"].fields["crossref"], key);
}

#[test]
fn escape_from_merge_returns_to_review() {
    let (mut app, _tmp) = make_app();
    app.handle_action(Action::FindDuplicates);
    app.wait_for_duplicate_scan();
    app.handle_action(Action::DedupReconcile);
    app.handle_action(Action::CloseDedup);
    assert_eq!(app.mode, InputMode::DedupReview);
    assert!(app.dedup_merge_state.is_none());
    assert_eq!(app.database.entries.len(), 4);
}

#[test]
fn save_after_merge_keeps_untouched_entries_byte_exact() {
    let (mut app, tmp) = make_app();
    app.handle_action(Action::FindDuplicates);
    app.wait_for_duplicate_scan();
    app.handle_action(Action::DedupReconcile);
    app.handle_action(Action::ApplyMerge);
    app.handle_action(Action::DialogConfirm);
    app.save();
    let out = std::fs::read_to_string(tmp.path()).unwrap();
    assert!(out.contains(UNTOUCHED), "{out}");
    assert!(!out.contains("@Article{Smith2020,"), "{out}");
    assert_eq!(out.matches("@Article{Smith2020a,").count(), 1, "{out}");
    let reparsed = crate::bib::parser::build_database(parse_bib_file(&out).unwrap());
    assert_eq!(reparsed.entries.len(), 3);
    assert_eq!(
        reparsed.entries["Smith2020a"].group_memberships,
        vec!["G2", "G1"]
    );
}

#[test]
fn not_duplicate_marks_persist_and_undo() {
    let (mut app, tmp) = make_app();
    app.handle_action(Action::FindDuplicates);
    app.wait_for_duplicate_scan();
    app.handle_action(Action::DedupIgnore);
    assert_eq!(app.mode, InputMode::DedupReview);
    assert!(app.dedup_review_state.as_ref().unwrap().clusters.is_empty());
    assert_eq!(
        app.not_duplicate_sets(),
        vec![vec!["Smith2020".to_string(), "Smith2020a".to_string()]]
    );
    assert!(app.duplicate_clusters().is_empty());

    app.handle_action(Action::CloseDedup);
    app.handle_action(Action::Undo);
    assert!(app.not_duplicate_sets().is_empty());
    assert_eq!(app.duplicate_clusters().len(), 1);

    // Mark again, save, and reload: the mark survives and nothing is reported.
    app.handle_action(Action::FindDuplicates);
    app.wait_for_duplicate_scan();
    app.handle_action(Action::DedupIgnore);
    app.save();
    let out = std::fs::read_to_string(tmp.path()).unwrap();
    assert!(
        out.contains("@Comment{jabref-meta: bibtuiNotDuplicates:\nSmith2020,Smith2020a;\n}"),
        "{out}"
    );
    assert!(out.contains(UNTOUCHED));
    let reloaded = App::new(tmp.path().to_path_buf(), default_config()).unwrap();
    assert!(reloaded.duplicate_clusters().is_empty());
    assert!(reloaded.status_message.is_none());
}

#[test]
fn marking_a_second_set_keeps_the_first() {
    let (mut app, tmp) = make_app();
    app.mark_not_duplicates(&["B".into(), "A".into()]);
    app.mark_not_duplicates(&["C".into(), "D".into(), "E".into()]);
    app.mark_not_duplicates(&["A".into(), "B".into()]); // already marked
    assert_eq!(
        app.not_duplicate_sets(),
        vec![
            vec!["A".to_string(), "B".to_string()],
            vec!["C".to_string(), "D".to_string(), "E".to_string()],
        ]
    );

    // One set per line, like JabRef's grouping block.
    app.save();
    let out = std::fs::read_to_string(tmp.path()).unwrap();
    assert!(
        out.contains("@Comment{jabref-meta: bibtuiNotDuplicates:\nA,B;\nC,D,E;\n}"),
        "{out}"
    );
}

/// Add an entry that duplicates Other2021 by DOI, as an import would.
fn add_imported_duplicate(app: &mut App) -> String {
    app.database
        .entries
        .get_mut("Other2021")
        .unwrap()
        .fields
        .insert("doi".into(), "10.9/z".into());
    let mut fields = IndexMap::new();
    fields.insert("title".to_string(), "Something else, entirely".to_string());
    fields.insert("doi".to_string(), "https://doi.org/10.9/Z".to_string());
    fields.insert("note".to_string(), "imported".to_string());
    let key = "Imported2021".to_string();
    app.database.entries.insert(
        key.clone(),
        Entry {
            entry_type: EntryType::InProceedings,
            citation_key: key.clone(),
            fields,
            group_memberships: vec![],
            raw_index: usize::MAX,
            dirty: true,
        },
    );
    app.push_undo(UndoItem::EntryAdded {
        entry_key: key.clone(),
    });
    app.refresh_view();
    app.offer_duplicate_merge(&key);
    key
}

#[test]
fn import_duplicate_can_be_merged() {
    let (mut app, _tmp) = make_app();
    let key = add_imported_duplicate(&mut app);
    assert_eq!(app.mode, InputMode::Dialog);
    assert!(matches!(
        app.pending_action,
        Some(PendingAction::ImportDuplicate { ref existing, .. }) if existing == &vec!["Other2021".to_string()]
    ));
    app.handle_action(Action::DialogConfirm); // option 0: merge
    assert_eq!(app.mode, InputMode::DedupMerge);
    let merge = app.dedup_merge_state.as_ref().unwrap();
    assert_eq!(merge.keys, vec!["Other2021".to_string(), key.clone()]);
    assert!(!merge.from_review);
    app.handle_action(Action::ApplyMerge);
    // Other2021 has more fields, so it survives and gains the import's note.
    assert!(!app.database.entries.contains_key(&key));
    assert_eq!(app.database.entries["Other2021"].fields["note"], "imported");
    assert_eq!(app.database.entries.len(), 4);
    app.handle_action(Action::DialogConfirm);
    assert_eq!(app.mode, InputMode::Normal);
}

#[test]
fn import_duplicate_keep_both_marks_and_discard_deletes() {
    let (mut app, _tmp) = make_app();
    let key = add_imported_duplicate(&mut app);
    app.dialog_state.as_mut().unwrap().select(1);
    app.handle_action(Action::DialogConfirm);
    assert!(app.database.entries.contains_key(&key));
    assert!(app
        .not_duplicate_sets()
        .contains(&vec![key.clone(), "Other2021".to_string()]));

    let (mut app, _tmp) = make_app();
    let key = add_imported_duplicate(&mut app);
    app.dialog_state.as_mut().unwrap().select(2);
    app.handle_action(Action::DialogConfirm);
    assert!(!app.database.entries.contains_key(&key));
    assert_eq!(app.mode, InputMode::Normal);
}

#[test]
fn import_check_can_be_disabled() {
    let (mut app, _tmp) = make_app();
    app.config.duplicates.check_on_import = false;
    add_imported_duplicate(&mut app);
    assert!(app.dialog_state.is_none());
}

#[test]
fn manually_added_entry_is_checked_when_its_detail_view_closes() {
    let (mut app, _tmp) = make_app();
    app.add_entry_of_type("Article");
    let key = app.detail_entry_key.clone().unwrap();
    let entry = app.database.entries.get_mut(&key).unwrap();
    entry.fields.insert("doi".into(), "10.1/X".into());
    app.handle_action(Action::CloseDetail);
    assert_eq!(app.mode, InputMode::Dialog);
    assert!(matches!(
        app.pending_action,
        Some(PendingAction::ImportDuplicate { ref new_key, .. }) if *new_key == key
    ));

    // Only once: reopening and closing it again does not ask again.
    app.handle_action(Action::DialogCancel);
    app.open_detail();
    app.handle_action(Action::CloseDetail);
    assert!(app.dialog_state.is_none());
}

// ── Background scans (review N15) ───────────────────────────────────────────

#[test]
fn load_check_result_for_a_changed_library_is_dropped() {
    let (mut app, _tmp) = make_app();
    assert!(
        app.duplicate_scan.is_some(),
        "the load check runs in the background"
    );
    app.delete_entry("Smith2020a");
    app.wait_for_duplicate_scan();
    // The count described the library as opened, not as it is now.
    assert_eq!(app.status_message.as_deref(), Some("Deleted 'Smith2020a'"));
}

#[test]
fn review_search_for_a_changed_library_starts_again() {
    let (mut app, _tmp) = make_app();
    app.wait_for_duplicate_scan();
    app.handle_action(Action::FindDuplicates);
    assert_eq!(
        app.mode,
        InputMode::Normal,
        "the list opens when the search ends"
    );
    app.delete_entry("Smith2020a");
    app.wait_for_duplicate_scan();
    assert_eq!(app.mode, InputMode::DedupReview);
    let review = app.dedup_review_state.as_ref().unwrap();
    assert!(review.clusters.is_empty(), "scanned the current library");
}

#[test]
fn review_search_can_be_cancelled_with_esc() {
    let (mut app, _tmp) = make_app();
    app.wait_for_duplicate_scan();
    app.handle_action(Action::FindDuplicates);
    app.handle_action(Action::FindDuplicates);
    assert_eq!(
        app.status_message.as_deref(),
        Some("Still looking for duplicates… (Esc cancels)")
    );
    app.handle_action(Action::ResetSort);
    assert!(app.duplicate_scan.is_none());
    assert_eq!(
        app.status_message.as_deref(),
        Some("Duplicate search cancelled")
    );
    // Esc does its usual job again once nothing is searching.
    assert!(!app.cancel_duplicate_review());
}

#[test]
fn review_list_waits_until_the_user_is_idle() {
    let (mut app, _tmp) = make_app();
    app.wait_for_duplicate_scan();
    app.handle_action(Action::FindDuplicates);
    app.start_import_entry();
    // However long the worker takes, nothing opens over the prompt.
    for _ in 0..20 {
        assert!(!app.poll_duplicate_scan());
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(app.mode, InputMode::Editing);
    app.handle_action(Action::CancelEdit);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !app.poll_duplicate_scan() {
        assert!(
            std::time::Instant::now() < deadline,
            "search never finished"
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(app.mode, InputMode::DedupReview);
    assert_eq!(app.dedup_review_state.as_ref().unwrap().clusters.len(), 1);
}
