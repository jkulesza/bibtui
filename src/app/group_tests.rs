//! Renaming groups from the group pane.

use super::*;
use crate::config::defaults::default_config;
use crossterm::event::{KeyEventKind, KeyEventState, KeyModifiers};
use pretty_assertions::assert_eq;

const LIBRARY: &str = r"@Article{A,
  title  = {Alpha},
  groups = {Reactors, Physics,Other},
}

@Article{B,
  title  = {Beta},
  groups = {Physics},
}

@Article{C,
  title = {Gamma},
}

@Article{D,
  title  = {Delta},
  groups = {Shared},
}

@Comment{jabref-meta: databaseType:bibtex;}

@Comment{jabref-meta: grouping:
0 AllEntriesGroup:;
1 StaticGroup:Physics\;0\;1\;0x8a8a8aff\;\;Physics papers\;;
1 StaticGroup:Reactors\;0\;1\;\;\;\;;
1 KeywordGroup:Alpha papers\;0\;title\;Alpha\;0\;0\;1\;\;\;\;;
2 StaticGroup:Shared\;0\;1\;\;\;\;;
1 StaticGroup:Shared\;0\;1\;\;\;\;;
1 StaticGroup:Twin\;0\;1\;\;\;\;;
1 StaticGroup:Twin\;0\;1\;\;\;\;;
1 KeywordGroup:Reactors\;0\;title\;Reactor\;0\;0\;1\;\;\;\;;
}
";

// Rows in the group pane.
const PHYSICS: usize = 1;
const REACTORS: usize = 2;
const KEYWORD: usize = 3;
const SHARED: usize = 4;
const TWIN: usize = 6;

fn open() -> (App, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("library.bib");
    std::fs::write(&path, LIBRARY).unwrap();
    let mut config = default_config();
    config.general.backup_on_save = false;
    config.save.entry_sort_order = "none".into();
    config.save.save_action_regenerate_citekeys = false;
    (App::new(path, config).unwrap(), dir)
}

fn press(app: &mut App, c: char) {
    app.handle_key(KeyEvent {
        code: KeyCode::Char(c),
        modifiers: KeyModifiers::NONE,
        kind: KeyEventKind::Press,
        state: KeyEventState::NONE,
    });
}

/// Select `row` in the group pane, press `e`, and confirm `name`.
fn rename(app: &mut App, row: usize, name: &str) {
    press(app, 'h');
    app.group_tree_state.select(row);
    press(app, 'e');
    let editor = app.field_editor_state.as_mut().expect("rename prompt");
    editor.value = name.into();
    app.handle_action(Action::ConfirmEdit);
}

fn status(app: &App) -> String {
    app.status_message.clone().unwrap_or_default()
}

#[test]
fn e_opens_a_prompt_prefilled_with_the_group_name() {
    let (mut app, _dir) = open();
    press(&mut app, 'h');
    app.group_tree_state.select(PHYSICS);
    press(&mut app, 'e');
    assert_eq!(app.mode, InputMode::Editing);
    assert_eq!(app.field_editor_state.as_ref().unwrap().value, "Physics");
    // Escape cancels without changes.
    app.handle_action(Action::CancelEdit);
    assert!(!app.dirty);
    assert_eq!(write_bib_file(&app.database.raw_file), LIBRARY);
}

#[test]
fn e_does_nothing_in_the_entry_list() {
    let (mut app, _dir) = open();
    press(&mut app, 'e');
    assert_eq!(app.mode, InputMode::Normal);
    assert!(app.field_editor_state.is_none());
}

#[test]
fn renaming_a_static_group_updates_members_and_keeps_jabref_fields() {
    let (mut app, _dir) = open();
    rename(&mut app, PHYSICS, "Physics & Chemistry");
    assert_eq!(
        status(&app),
        "Renamed group 'Physics' to 'Physics & Chemistry' in 2 entries (u to undo)"
    );
    assert_eq!(app.focus, Focus::Groups);
    assert!(app.dirty);
    let entries = &app.database.entries;
    // Only the name changes; each field keeps its separators and spacing.
    assert_eq!(
        entries["A"].fields["groups"],
        "Reactors, Physics & Chemistry,Other"
    );
    assert_eq!(entries["B"].fields["groups"], "Physics & Chemistry");
    assert!(!entries["C"].fields.contains_key("groups"));
    assert!(!entries["C"].dirty);

    // The renamed group still filters to its members.
    press(&mut app, 'h');
    app.group_tree_state.select(PHYSICS);
    app.handle_action(Action::OpenDetail);
    assert_eq!(app.visible_entry_count(), 2);

    assert!(app.save(), "{:?}", app.status_message);
    let text = std::fs::read_to_string(&app.bib_path).unwrap();
    // JabRef's extra fields (color, description) survive; the block keeps
    // JabRef's layout.
    assert!(
        text.contains("@Comment{jabref-meta: grouping:\n0 AllEntriesGroup:;\n1 StaticGroup:Physics & Chemistry\\;0\\;1\\;0x8a8a8aff\\;\\;Physics papers\\;;\n1 StaticGroup:Reactors"),
        "{text}"
    );
    assert!(text.contains("\\;\\;\\;\\;;\n}\n"), "{text}");
    assert!(
        text.contains("@Article{C,\n  title = {Gamma},\n}"),
        "{text}"
    );
    let reloaded = App::new(app.bib_path.clone(), app.config.clone()).unwrap();
    assert_eq!(
        reloaded.database.entries["A"].group_memberships,
        ["Reactors", "Physics & Chemistry", "Other"]
    );
}

#[test]
fn undo_restores_the_tree_entries_and_file_exactly() {
    let (mut app, _dir) = open();
    rename(&mut app, PHYSICS, "Renamed");
    app.undo();
    assert!(!app.dirty);
    assert_eq!(
        app.database.entries["A"].group_memberships,
        ["Reactors", "Physics", "Other"]
    );
    assert_eq!(write_bib_file(&app.database.raw_file), LIBRARY);
    assert!(app.save());
    assert_eq!(std::fs::read_to_string(&app.bib_path).unwrap(), LIBRARY);
}

#[test]
fn renaming_a_keyword_group_changes_only_its_name() {
    let (mut app, _dir) = open();
    rename(&mut app, KEYWORD, "Alpha-titled");
    assert_eq!(
        status(&app),
        "Renamed group 'Alpha papers' to 'Alpha-titled' (u to undo)"
    );
    assert!(app.database.entries.values().all(|e| !e.dirty));
    let raw = write_bib_file(&app.database.raw_file);
    assert!(
        raw.contains("1 KeywordGroup:Alpha-titled\\;0\\;title\\;Alpha\\;0\\;0\\;1\\;\\;\\;\\;;"),
        "{raw}"
    );
}

#[test]
fn invalid_and_ambiguous_renames_are_refused() {
    let (mut app, _dir) = open();
    for (row, name, expected) in [
        (PHYSICS, "", "cannot be empty"),
        (PHYSICS, "A, B", "cannot contain ','"),
        (PHYSICS, "a;b", "cannot contain ';'"),
        (PHYSICS, "C:\\x", "cannot contain '\\'"),
        (
            PHYSICS,
            "Reactors",
            "another group is already named 'Reactors'",
        ),
        (
            SHARED,
            "Unique",
            "another static group is also named 'Shared'",
        ),
    ] {
        rename(&mut app, row, name);
        let message = status(&app);
        assert!(message.starts_with("Group not renamed: "), "{message}");
        assert!(message.contains(expected), "{name:?}: {message}");
        assert!(!app.dirty, "{name:?}");
    }
    // The All Entries row cannot be renamed at all.
    press(&mut app, 'h');
    app.group_tree_state.select(0);
    press(&mut app, 'e');
    assert!(app.field_editor_state.is_none());
    assert!(status(&app).contains("cannot be renamed"));
    // An unchanged name is a no-op.
    rename(&mut app, REACTORS, "Reactors");
    assert!(!app.dirty);
    assert_eq!(write_bib_file(&app.database.raw_file), LIBRARY);
}

#[test]
fn new_groups_with_separator_characters_are_refused() {
    let (mut app, _dir) = open();
    press(&mut app, 'h');
    app.group_tree_state.select(0);
    press(&mut app, 'a');
    app.field_editor_state.as_mut().unwrap().value = "x,y".into();
    app.handle_action(Action::ConfirmEdit);
    assert!(
        status(&app).starts_with("Group not added:"),
        "{}",
        status(&app)
    );
    assert!(!app.dirty);
}

#[test]
fn rename_is_listed_in_help_and_bindable() {
    assert_eq!(
        crate::tui::keybindings::action_from_name("RenameGroup"),
        Some(Action::RenameGroup)
    );
}

#[test]
fn shared_names_without_members_and_keyword_namesakes_do_not_block_renaming() {
    let (mut app, _dir) = open();
    // Two static groups called `Twin`, but no entry names them: unambiguous.
    rename(&mut app, TWIN, "Twin A");
    assert_eq!(
        status(&app),
        "Renamed group 'Twin' to 'Twin A' in 0 entries (u to undo)"
    );
    // `Reactors` is also the name of a keyword group, whose membership is
    // computed from its search, so the static group's entries are unambiguous.
    rename(&mut app, REACTORS, "Reactor physics");
    assert!(status(&app).contains("in 1 entry"), "{}", status(&app));
    assert_eq!(
        app.database.entries["A"].fields["groups"],
        "Reactor physics, Physics,Other"
    );
}
