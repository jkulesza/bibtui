//! Library-level settings: the YAML/library configuration layers, writing
//! settings into the `.bib` metadata, and routing settings-editor changes.

use std::collections::BTreeSet;

use super::*;
use crate::bib::jabref::{meta_comment_key, parse_meta};
use crate::config::library::{
    apply_overrides, changes_for_paths, config_value, flatten, is_jabref_save_action,
    plan_library_clear, plan_library_export, set_path, LibrarySettings, MetaChange,
};
use crate::tui::components::settings::SettingRow;

impl App {
    /// Recompute the effective configuration from the YAML layer and the
    /// library's metadata. Returns warnings about library settings.
    pub(super) fn recompute_config(&mut self) -> Vec<String> {
        let library = LibrarySettings::from_meta(&self.database.jabref_meta);
        let (config, mut warnings) = apply_overrides(&self.base_config, &library.overrides());
        warnings.extend(library.warnings.iter().cloned());
        self.library = library;
        self.config = config;
        self.user_bindings = build_user_bindings(&self.config.keybindings);
        self.sync_runtime_from_config();
        if self.settings_state.is_some() {
            self.rebuild_settings_state();
        }
        self.view_dirty = true;
        warnings
    }

    /// A settings editor reflecting the effective configuration, with
    /// library-sourced settings marked.
    pub(super) fn new_settings_state(&self) -> SettingsState {
        let mut state = SettingsState::new(&self.config);
        state.library_paths = self.library.overrides().into_keys().collect();
        state
    }

    /// Rebuild the open settings editor in place, keeping the cursor.
    fn rebuild_settings_state(&mut self) {
        let mut state = self.new_settings_state();
        if let Some(old) = &self.settings_state {
            state.scroll_offset = old.scroll_offset;
            state.cursor = old.cursor.min(state.rows.len().saturating_sub(1));
            if matches!(state.rows.get(state.cursor), Some(SettingRow::Section(_))) {
                state.move_down();
            }
        }
        self.settings_state = Some(state);
    }

    /// Store a settings-editor change. `self.config` already holds the edited
    /// values; each changed setting goes to the layer it currently comes
    /// from: the library if the library overrides it, otherwise the YAML
    /// layer (exported with `E`).
    pub(super) fn commit_settings_edit(&mut self) {
        let overrides = self.library.overrides();
        let (previous, _) = apply_overrides(&self.base_config, &overrides);
        let before = flatten(&previous);
        let after = flatten(&self.config);
        let changed: BTreeSet<String> = before
            .keys()
            .chain(after.keys())
            .filter(|path| before.get(*path) != after.get(*path))
            .cloned()
            .collect();
        if changed.is_empty() {
            self.sync_runtime_from_config();
            return;
        }

        let jabref_actions_locked = self.library.save_actions.as_ref().is_some_and(|s| !s.exact);
        let mut library_paths = BTreeSet::new();
        let mut base_root = config_value(&self.base_config);
        let mut refused = false;
        for path in &changed {
            if overrides.contains_key(path) {
                if jabref_actions_locked && is_jabref_save_action(path) {
                    refused = true;
                } else {
                    library_paths.insert(path.clone());
                }
            } else if let Some(value) = after.get(path) {
                set_path(&mut base_root, path, value.clone());
            }
        }
        if let Ok(base) = serde_json::from_value(base_root) {
            self.base_config = base;
        }

        let mut messages = Vec::new();
        if !library_paths.is_empty() {
            let (changes, notes) = changes_for_paths(&self.config, &self.library, &library_paths);
            messages.extend(notes);
            if !changes.is_empty() {
                let count = changes.len();
                self.apply_library_changes(changes);
                messages.insert(
                    0,
                    format!(
                        "Updated {} library setting{} (save with :w)",
                        count,
                        if count == 1 { "" } else { "s" }
                    ),
                );
            }
        }
        if refused {
            messages.push(
                "This library's JabRef saveActions use formatters bibtui does not model; change those save actions in JabRef"
                    .into(),
            );
        }
        // Rebuild from the layers so refused or invalid edits are not shown
        // as if they had taken effect.
        self.recompute_config();
        if !messages.is_empty() {
            self.status_message = Some(messages.join("; "));
        }
    }

    /// The current `@Comment` text of a metadata key, if present.
    fn meta_comment_text(&self, key: &str) -> Option<String> {
        self.database
            .raw_file
            .items
            .iter()
            .find_map(|item| match item {
                RawItem::Comment { raw_text } if meta_comment_key(raw_text) == Some(key) => {
                    Some(raw_text.clone())
                }
                _ => None,
            })
    }

    /// Apply metadata changes as one undoable edit and recompute settings.
    pub(super) fn apply_library_changes(&mut self, changes: Vec<MetaChange>) -> Vec<String> {
        if changes.is_empty() {
            return Vec::new();
        }
        let previous: Vec<(String, Option<String>)> = changes
            .iter()
            .map(|change| (change.key.clone(), self.meta_comment_text(&change.key)))
            .collect();
        for change in changes {
            self.set_meta_comment(&change.key, change.comment);
        }
        self.database.jabref_meta = parse_meta(&self.database.raw_file);
        self.push_undo(UndoItem::LibraryMetaChanged { previous });
        self.recompute_config()
    }

    /// Undo [`apply_library_changes`] by restoring each key's previous comment.
    pub(super) fn undo_library_changes(&mut self, previous: Vec<(String, Option<String>)>) {
        for (key, comment) in previous.into_iter().rev() {
            self.set_meta_comment(&key, comment);
        }
        self.database.jabref_meta = parse_meta(&self.database.raw_file);
        self.recompute_config();
        self.status_message = Some("Undo: library settings".into());
    }

    /// Add, replace, or remove one metadata comment in the raw file. New
    /// comments are placed among the existing metadata comments in JabRef's
    /// key order. Raw indices of entries and pending deletions are kept valid.
    fn set_meta_comment(&mut self, key: &str, comment: Option<String>) {
        let items = &self.database.raw_file.items;
        let existing = items.iter().position(
            |item| matches!(item, RawItem::Comment { raw_text } if meta_comment_key(raw_text) == Some(key)),
        );
        match (existing, comment) {
            (Some(index), Some(text)) => {
                self.database.raw_file.items[index] = RawItem::Comment { raw_text: text };
            }
            (Some(index), None) => {
                self.remove_raw_item(index);
                // Drop the blank-line separator that belonged to it.
                let separator = index
                    .checked_sub(1)
                    .filter(|&i| is_blank_separator(&self.database.raw_file.items[i]))
                    .or_else(|| {
                        (index < self.database.raw_file.items.len()
                            && is_blank_separator(&self.database.raw_file.items[index]))
                        .then_some(index)
                    });
                if let Some(separator) = separator {
                    self.remove_raw_item(separator);
                }
            }
            (None, Some(text)) => {
                let meta_positions: Vec<(usize, &str)> = items
                    .iter()
                    .enumerate()
                    .filter_map(|(i, item)| match item {
                        RawItem::Comment { raw_text } => meta_comment_key(raw_text).map(|k| (i, k)),
                        _ => None,
                    })
                    .collect();
                let comment = RawItem::Comment { raw_text: text };
                let separator = || RawItem::Preamble("\n\n".into());
                if let Some(&(before, _)) = meta_positions.iter().find(|(_, k)| *k > key) {
                    self.insert_raw_items(before, vec![comment, separator()]);
                } else if let Some(&(last, _)) = meta_positions.last() {
                    self.insert_raw_items(last + 1, vec![separator(), comment]);
                } else {
                    let end = self.database.raw_file.items.len();
                    self.insert_raw_items(
                        end,
                        vec![separator(), comment, RawItem::Preamble("\n".into())],
                    );
                }
            }
            (None, None) => {}
        }
    }

    fn insert_raw_items(&mut self, at: usize, new_items: Vec<RawItem>) {
        let count = new_items.len();
        self.database.raw_file.items.splice(at..at, new_items);
        for entry in self.database.entries.values_mut() {
            if entry.raw_index != usize::MAX && entry.raw_index >= at {
                entry.raw_index += count;
            }
        }
        for index in &mut self.deleted_raw_indices {
            if *index >= at {
                *index += count;
            }
        }
    }

    fn remove_raw_item(&mut self, at: usize) {
        self.database.raw_file.items.remove(at);
        for entry in self.database.entries.values_mut() {
            if entry.raw_index != usize::MAX && entry.raw_index > at {
                entry.raw_index -= 1;
            }
        }
        for index in &mut self.deleted_raw_indices {
            if *index > at {
                *index -= 1;
            }
        }
    }

    /// Preview writing the effective settings into the library (`B`,
    /// `:settings-export bib`); confirming applies the changes.
    pub(super) fn request_library_export(&mut self) {
        let (changes, notes) = plan_library_export(&self.config, &self.base_config, &self.library);
        if changes.is_empty() {
            let mut message = "Library settings are already up to date".to_string();
            for note in notes {
                message.push_str("; ");
                message.push_str(&note);
            }
            self.status_message = Some(message);
            return;
        }
        let mut lines: Vec<String> = changes
            .iter()
            .map(|change| {
                let marker = match (&change.comment, self.meta_comment_text(&change.key)) {
                    (None, _) => '-',
                    (Some(_), Some(_)) => '~',
                    (Some(_), None) => '+',
                };
                format!("{marker} {}", change.summary)
            })
            .collect();
        lines.extend(notes.into_iter().map(|note| format!("! {note}")));
        self.dialog_state = Some(DialogState::change_preview(
            " Write settings to library ",
            lines,
        ));
        self.pending_action = Some(PendingAction::ApplyLibrarySettings { changes });
        self.mode = InputMode::Dialog;
    }

    /// Apply previewed library changes after the user confirmed them.
    pub(super) fn confirm_library_changes(&mut self, changes: Vec<MetaChange>) {
        let count = changes.len();
        let warnings = self.apply_library_changes(changes);
        let mut message = format!(
            "Wrote {} setting change{} to the library; save with :w (u to undo)",
            count,
            if count == 1 { "" } else { "s" }
        );
        if !warnings.is_empty() {
            message.push_str(&format!(" | Library settings: {}", warnings.join("; ")));
        }
        self.status_message = Some(message);
    }

    /// Remove every `bibtui.*` setting from the library (`:settings-clear-bib`).
    pub(super) fn clear_library_settings(&mut self) {
        let changes = plan_library_clear(&self.library);
        if changes.is_empty() {
            self.status_message = Some("This library stores no bibtui settings".into());
            return;
        }
        let count = changes.len();
        self.apply_library_changes(changes);
        self.status_message = Some(format!(
            "Removed {} bibtui setting{} from the library; JabRef keys kept (u to undo, :w to save)",
            count,
            if count == 1 { "" } else { "s" }
        ));
    }
}

fn is_blank_separator(item: &RawItem) -> bool {
    matches!(item, RawItem::Preamble(text) if text.trim().is_empty())
}
