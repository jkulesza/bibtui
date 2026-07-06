//! Field editing: the field editor lifecycle, citekey regeneration,
//! and the vim-modal action handler.

use super::*;

impl App {
    pub(super) fn start_edit_field(&mut self) {
        // When a FileEntry row is selected, edit just that file's path.
        if let Some(idx) = self.detail_state.as_ref().and_then(|d| d.selected_file_index()) {
            let key = match self.detail_entry_key.clone() { Some(k) => k, None => return };
            let file_value = self.database.entries.get(&key)
                .and_then(|e| e.fields.get("file").cloned())
                .unwrap_or_default();
            let files = parse_file_field(&file_value);
            let current_path = files.get(idx).map(|f| f.path.as_str()).unwrap_or("");
            self.field_editor_state = Some(FieldEditorState::for_path("File path", current_path));
            self.pending_action = Some(PendingAction::EditFileAttachment { entry_key: key, index: idx });
            self.mode = InputMode::Editing;
            return;
        }
        if let Some(ref detail) = self.detail_state {
            if let Some((field_name, field_value)) = detail.selected_field() {
                self.field_editor_state =
                    Some(FieldEditorState::new(field_name, field_value));
                self.mode = InputMode::Editing;
                self.update_field_completions();
            }
        }
    }

    pub(super) fn start_add_file_attachment(&mut self) {
        let key = match self.detail_entry_key.clone() { Some(k) => k, None => return };
        self.field_editor_state = Some(FieldEditorState::for_path("New file path", ""));
        self.pending_action = Some(PendingAction::AddFileAttachment { entry_key: key });
        self.mode = InputMode::Editing;
    }

    pub(super) fn confirm_edit(&mut self) {
        // Dispatch on the pending action.  Each arm owns its own field-editor
        // cleanup and target mode; anything without a dedicated arm (including
        // `None`) falls through to the ordinary field-edit path.
        match self.pending_action.take() {
            Some(PendingAction::NewFile) => self.confirm_new_file(),
            Some(PendingAction::ExportSettings) => self.confirm_export_settings(),
            Some(PendingAction::ExportJson) => self.confirm_export_json(),
            Some(PendingAction::ExportRis) => self.confirm_export_ris(),
            Some(PendingAction::ImportUrl) => self.confirm_import_url(),
            Some(PendingAction::ImportSettings) => self.confirm_import_settings(),
            Some(PendingAction::EditSetting { setting_id }) => {
                self.confirm_edit_setting(setting_id)
            }
            Some(PendingAction::AddFieldGroup) => self.confirm_add_field_group(),
            Some(PendingAction::EditFieldGroupFields { index }) => {
                self.confirm_edit_field_group_fields(index)
            }
            Some(PendingAction::RenameFieldGroup { index }) => {
                self.confirm_rename_field_group(index)
            }
            Some(PendingAction::AddColumn) => self.confirm_add_column(),
            Some(PendingAction::EditColumnWidth { index }) => {
                self.confirm_edit_column_width(index)
            }
            Some(PendingAction::RenameColumn { index }) => self.confirm_rename_column(index),
            Some(PendingAction::AddFileAttachment { entry_key }) => {
                self.confirm_add_file_attachment(entry_key)
            }
            Some(PendingAction::EditFileAttachment { entry_key, index }) => {
                self.confirm_edit_file_attachment(entry_key, index)
            }
            Some(PendingAction::AddGroup { parent_path }) => {
                self.confirm_add_group(parent_path)
            }
            _ => self.confirm_field_edit(),
        }
    }

    /// New file: the user just entered a path for a brand-new library.
    fn confirm_new_file(&mut self) {
        let path_str = self
            .field_editor_state
            .as_ref()
            .map(|e| e.value.trim().to_string())
            .unwrap_or_default();
        self.field_editor_state = None;

        if path_str.is_empty() {
            // Re-prompt: the user must provide a path.
            self.status_message = Some("Please enter a path for the new library.".to_string());
            self.field_editor_state = Some(FieldEditorState::for_path("Save new library as", ""));
            self.pending_action = Some(PendingAction::NewFile);
            self.mode = InputMode::Editing;
            return;
        }

        // Append .bib if the user omitted it.
        let path_str = if path_str.ends_with(".bib") {
            path_str
        } else {
            format!("{}.bib", path_str)
        };

        self.bib_path = PathBuf::from(expand_tilde(&path_str));
        self.mode = InputMode::Normal;
        self.save(); // writes the (empty) file to disk
    }

    fn confirm_export_settings(&mut self) {
        let path_str = self
            .field_editor_state
            .as_ref()
            .map(|e| e.value.trim().to_string())
            .unwrap_or_else(|| "bibtui.yaml".to_string());
        self.field_editor_state = None;
        self.mode = InputMode::Settings;
        if !path_str.is_empty() {
            self.export_settings(&path_str);
        }
    }

    /// Export as CSL-JSON.
    fn confirm_export_json(&mut self) {
        let path_str = self
            .field_editor_state
            .as_ref()
            .map(|e| e.value.trim().to_string())
            .unwrap_or_default();
        self.field_editor_state = None;
        self.mode = InputMode::Normal;
        if !path_str.is_empty() {
            self.do_export_json(&path_str);
        }
    }

    /// Export as RIS.
    fn confirm_export_ris(&mut self) {
        let path_str = self
            .field_editor_state
            .as_ref()
            .map(|e| e.value.trim().to_string())
            .unwrap_or_default();
        self.field_editor_state = None;
        self.mode = InputMode::Normal;
        if !path_str.is_empty() {
            self.do_export_ris(&path_str);
        }
    }

    /// Import entry from DOI/URL.
    fn confirm_import_url(&mut self) {
        let doi_or_url = self
            .field_editor_state
            .as_ref()
            .map(|e| e.value.trim().to_string())
            .unwrap_or_default();
        self.field_editor_state = None;
        self.mode = InputMode::Normal;
        if !doi_or_url.is_empty() {
            self.spawn_import(doi_or_url);
        }
    }

    fn confirm_import_settings(&mut self) {
        let path_str = self
            .field_editor_state
            .as_ref()
            .map(|e| e.value.trim().to_string())
            .unwrap_or_default();
        self.field_editor_state = None;
        self.mode = InputMode::Settings;
        if !path_str.is_empty() {
            self.import_settings(&path_str);
        }
    }

    /// Edit a string setting.
    fn confirm_edit_setting(&mut self, setting_id: String) {
        let new_val = self
            .field_editor_state
            .as_ref()
            .map(|e| e.value.clone())
            .unwrap_or_default();
        self.field_editor_state = None;
        self.mode = InputMode::Settings;
        if let Some(ref mut s) = self.settings_state {
            s.set_value(&setting_id, SettingValue::Str(new_val));
            s.apply_to_config(&mut self.config);
            self.sync_runtime_from_config();
        }
    }

    /// Add a new field group.
    fn confirm_add_field_group(&mut self) {
        let name = self
            .field_editor_state
            .as_ref()
            .map(|e| e.value.trim().to_string())
            .unwrap_or_default();
        self.field_editor_state = None;
        self.mode = InputMode::Settings;
        if !name.is_empty() {
            if let Some(ref mut s) = self.settings_state {
                s.add_field_group(name);
                s.apply_to_config(&mut self.config);
            }
            self.sync_runtime_from_config();
        }
    }

    /// Edit field group fields.
    fn confirm_edit_field_group_fields(&mut self, index: usize) {
        let fields_csv = self
            .field_editor_state
            .as_ref()
            .map(|e| e.value.clone())
            .unwrap_or_default();
        self.field_editor_state = None;
        self.mode = InputMode::Settings;
        if let Some(ref mut s) = self.settings_state {
            s.set_field_group_fields(index, fields_csv);
            s.apply_to_config(&mut self.config);
        }
        self.sync_runtime_from_config();
    }

    /// Rename a field group.
    fn confirm_rename_field_group(&mut self, index: usize) {
        let name = self
            .field_editor_state
            .as_ref()
            .map(|e| e.value.trim().to_string())
            .unwrap_or_default();
        self.field_editor_state = None;
        self.mode = InputMode::Settings;
        if !name.is_empty() {
            if let Some(ref mut s) = self.settings_state {
                s.set_field_group_name(index, name);
                s.apply_to_config(&mut self.config);
            }
            self.sync_runtime_from_config();
        }
    }

    /// Add a new display column.
    fn confirm_add_column(&mut self) {
        let input = self
            .field_editor_state
            .as_ref()
            .map(|e| e.value.trim().to_string())
            .unwrap_or_default();
        self.field_editor_state = None;
        self.mode = InputMode::Settings;
        if !input.is_empty() {
            let (field, header) = parse_field_header(&input);
            if let Some(ref mut s) = self.settings_state {
                s.add_column(field, header, "flex".to_string());
                s.apply_to_config(&mut self.config);
            }
            self.sync_runtime_from_config();
        }
    }

    /// Edit column width.
    fn confirm_edit_column_width(&mut self, index: usize) {
        let width_spec = self
            .field_editor_state
            .as_ref()
            .map(|e| e.value.trim().to_string())
            .unwrap_or_default();
        self.field_editor_state = None;
        self.mode = InputMode::Settings;
        if let Some(ref mut s) = self.settings_state {
            s.set_column_width(index, width_spec);
            s.apply_to_config(&mut self.config);
        }
        self.sync_runtime_from_config();
    }

    /// Rename a display column.
    fn confirm_rename_column(&mut self, index: usize) {
        let input = self
            .field_editor_state
            .as_ref()
            .map(|e| e.value.trim().to_string())
            .unwrap_or_default();
        self.field_editor_state = None;
        self.mode = InputMode::Settings;
        if !input.is_empty() {
            let (field, header) = parse_field_header(&input);
            if let Some(ref mut s) = self.settings_state {
                s.set_column_name(index, field, header);
                s.apply_to_config(&mut self.config);
            }
            self.sync_runtime_from_config();
        }
    }

    /// Add a new file attachment.
    fn confirm_add_file_attachment(&mut self, entry_key: String) {
        let path_str = self
            .field_editor_state
            .as_ref()
            .map(|e| e.value.trim().to_string())
            .unwrap_or_default();
        self.field_editor_state = None;
        self.mode = InputMode::Detail;
        if !path_str.is_empty() {
            let abs_path = PathBuf::from(expand_tilde(&path_str));
            let file_type = abs_path
                .extension()
                .and_then(|e| e.to_str())
                .map(|e| e.to_uppercase())
                .unwrap_or_default();
            // Store a path relative to the JabRef fileDirectory (same convention
            // used by the import pipeline).  Falls back to absolute if the file
            // is outside that directory or canonicalization fails.
            let file_dir = effective_file_dir(
                &self.bib_path,
                self.database.jabref_meta.file_directory.as_deref(),
            );
            let stored_path = crate::util::open::make_relative(&file_dir, &abs_path)
                .to_string_lossy()
                .into_owned();
            let current = self
                .database
                .entries
                .get(&entry_key)
                .and_then(|e| e.fields.get("file").cloned())
                .unwrap_or_default();
            let mut files = parse_file_field(&current);
            files.push(crate::util::open::ParsedFile {
                description: String::new(),
                path: stored_path,
                file_type,
            });
            let new_value = serialize_file_field(&files);
            self.push_undo(UndoItem::FieldChanged {
                entry_key: entry_key.clone(),
                field_name: "file".to_string(),
                old_value: if current.is_empty() { None } else { Some(current) },
            });
            if let Some(entry) = self.database.entries.get_mut(&entry_key) {
                entry.fields.insert("file".to_string(), new_value);
                entry.dirty = true;
                let snapshot = entry.clone();
                if let Some(ref mut detail) = self.detail_state {
                    detail.refresh(&snapshot);
                }
            }
        }
    }

    /// Edit the path of a specific file attachment.
    fn confirm_edit_file_attachment(&mut self, entry_key: String, file_idx: usize) {
        let path_str = self
            .field_editor_state
            .as_ref()
            .map(|e| e.value.trim().to_string())
            .unwrap_or_default();
        self.field_editor_state = None;
        self.mode = InputMode::Detail;
        if !path_str.is_empty() {
            let abs_path = PathBuf::from(expand_tilde(&path_str));
            let file_dir = effective_file_dir(
                &self.bib_path,
                self.database.jabref_meta.file_directory.as_deref(),
            );
            let stored_path = crate::util::open::make_relative(&file_dir, &abs_path)
                .to_string_lossy()
                .into_owned();
            let current = self
                .database
                .entries
                .get(&entry_key)
                .and_then(|e| e.fields.get("file").cloned())
                .unwrap_or_default();
            let mut files = parse_file_field(&current);
            if let Some(f) = files.get_mut(file_idx) {
                let ext = abs_path
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|e| e.to_uppercase())
                    .unwrap_or_default();
                f.path = stored_path;
                if !ext.is_empty() && f.file_type.is_empty() {
                    f.file_type = ext;
                }
            }
            let new_value = serialize_file_field(&files);
            if new_value != current {
                self.push_undo(UndoItem::FieldChanged {
                    entry_key: entry_key.clone(),
                    field_name: "file".to_string(),
                    old_value: Some(current),
                });
                if let Some(entry) = self.database.entries.get_mut(&entry_key) {
                    entry.fields.insert("file".to_string(), new_value);
                    entry.dirty = true;
                    let snapshot = entry.clone();
                    if let Some(ref mut detail) = self.detail_state {
                        detail.refresh(&snapshot);
                    }
                }
            }
        }
    }

    /// Group name input — handled separately from field editing.
    fn confirm_add_group(&mut self, parent_path: Vec<usize>) {
        let name = self
            .field_editor_state
            .as_ref()
            .map(|e| e.value.trim().to_string())
            .unwrap_or_default();
        self.field_editor_state = None;
        self.mode = InputMode::Normal;
        if !name.is_empty() {
            self.finish_add_group(name, parent_path);
        }
    }

    /// The ordinary field-editor path: either advance the two-phase new-field
    /// flow (name → value) or commit the edited field value.
    fn confirm_field_edit(&mut self) {
        // Two-phase for new fields: first confirm name, then enter value.
        if let Some(ref mut editor) = self.field_editor_state {
            if editor.advance_phase() {
                // Now that the field name is confirmed, enable month mode if applicable.
                if editor.field_name.eq_ignore_ascii_case("month") {
                    editor.is_month = true;
                }
                // Just switched from name to value editing — seed value completions.
                self.update_field_completions();
                return;
            }
        }

        if let Some(editor) = self.field_editor_state.take() {
            // Skip if field name is empty (aborted new-field)
            if editor.field_name.is_empty() {
                self.mode = InputMode::Detail;
                return;
            }
            if let Some(ref key) = self.detail_entry_key.clone() {
                let existing = self
                    .database
                    .entries
                    .get(key)
                    .and_then(|e| e.fields.get(&editor.field_name).cloned());
                let existing_str = existing.clone().unwrap_or_default();
                // Normalize month values to standard 3-letter abbreviations.
                let save_value = if editor.is_month {
                    normalize_month(&editor.value)
                } else {
                    editor.value.clone()
                };
                if save_value != existing_str {
                    self.push_undo(UndoItem::FieldChanged {
                        entry_key: key.clone(),
                        field_name: editor.field_name.clone(),
                        old_value: existing,
                    });
                    if let Some(entry) = self.database.entries.get_mut(key) {
                        entry.fields.insert(editor.field_name.clone(), save_value);
                        entry.dirty = true;
                        let snapshot = entry.clone();
                        if let Some(ref mut detail) = self.detail_state {
                            detail.refresh(&snapshot);
                        }
                    }
                    self.regen_citekey();
                    let current_key =
                        self.detail_entry_key.clone().unwrap_or_else(|| key.clone());
                    self.recheck_dirty(&current_key);
                }
            }
        }
        self.mode = InputMode::Detail;
    }

    pub(super) fn delete_field(&mut self) {
        // When a FileEntry row is selected, remove just that file from the field.
        if let Some(file_idx) = self.detail_state.as_ref().and_then(|d| d.selected_file_index()) {
            self.delete_file_attachment(file_idx);
            return;
        }

        let field_name_opt = self
            .detail_state
            .as_ref()
            .and_then(|d| d.selected_field())
            .map(|(name, _)| name.to_string());

        if let Some(field_name) = field_name_opt {
            if let Some(ref key) = self.detail_entry_key.clone() {
                let old_value = self.database.entries.get(key)
                    .and_then(|e| e.fields.get(&field_name).cloned());
                if let Some(old_value) = old_value {
                    self.push_undo(UndoItem::FieldChanged {
                        entry_key: key.clone(),
                        field_name: field_name.clone(),
                        old_value: Some(old_value),
                    });
                    if let Some(entry) = self.database.entries.get_mut(key) {
                        entry.fields.shift_remove(&field_name);
                        entry.dirty = true;
                        let entry_clone = entry.clone();
                        if let Some(ref mut detail) = self.detail_state {
                            detail.refresh(&entry_clone);
                        }
                    }
                }
            }
        }
    }

    pub(super) fn delete_file_attachment(&mut self, index: usize) {
        let key = match self.detail_entry_key.clone() { Some(k) => k, None => return };
        let file_value = match self.database.entries.get(&key)
            .and_then(|e| e.fields.get("file")).cloned()
        {
            Some(v) if !v.is_empty() => v,
            _ => return,
        };
        let mut files = parse_file_field(&file_value);
        if index >= files.len() {
            return;
        }
        files.remove(index);
        let new_value = serialize_file_field(&files);
        self.push_undo(UndoItem::FieldChanged {
            entry_key: key.clone(),
            field_name: "file".to_string(),
            old_value: Some(file_value),
        });
        if let Some(entry) = self.database.entries.get_mut(&key) {
            if new_value.is_empty() {
                entry.fields.shift_remove("file");
            } else {
                entry.fields.insert("file".to_string(), new_value);
            }
            entry.dirty = true;
            let snapshot = entry.clone();
            if let Some(ref mut detail) = self.detail_state {
                detail.refresh(&snapshot);
            }
        }
    }

    /// After a field edit, re-evaluate whether the entry is truly dirty by
    /// comparing its current fields and citation key against the original raw
    /// representation in the file.  Clears `dirty` when the entry has been
    /// fully reverted to its on-disk state.
    pub(super) fn recheck_dirty(&mut self, entry_key: &str) {
        use crate::bib::model::RawItem;

        let Some(entry) = self.database.entries.get_mut(entry_key) else { return };

        // A changed citation key is always dirty.
        let original_raw = match self.database.raw_file.items.get(entry.raw_index) {
            Some(RawItem::Entry(re)) => re,
            _ => return,
        };
        if entry.citation_key != original_raw.citation_key {
            return;
        }

        // Build a map of original field values (inner content, no delimiters).
        let original: std::collections::HashMap<String, String> = original_raw
            .fields
            .iter()
            .map(|f| (f.name.to_lowercase(), f.value.to_string_value()))
            .collect();

        // Current non-empty fields must match the original exactly.
        let all_match = entry
            .fields
            .iter()
            .filter(|(_, v)| !v.is_empty())
            .all(|(k, v)| original.get(&k.to_lowercase()).map(|ov| ov == v).unwrap_or(false))
            && original
                .iter()
                .filter(|(_, v)| !v.is_empty())
                .all(|(k, v)| entry.fields.get(k.as_str()).map(|ev| ev == v).unwrap_or(false));

        if all_match {
            entry.dirty = false;
        }
    }

    /// Return a key that does not collide with any existing entry, excluding
    /// `current_key` (the entry being renamed, whose slot is about to be freed).
    pub(super) fn unique_citekey(&self, base: &str, current_key: &str) -> String {
        if !self.database.entries.contains_key(base) || base == current_key {
            return base.to_string();
        }
        let mut n = 2usize;
        loop {
            let candidate = format!("{}_{}", base, n);
            if !self.database.entries.contains_key(&candidate) || candidate == current_key {
                return candidate;
            }
            n += 1;
        }
    }

    /// Resolve the citation key template for a given entry type using precedence:
    /// 1. Per-type pattern from .bib JabRef metadata
    /// 2. Default pattern from .bib JabRef metadata
    /// 3. Per-type pattern from YAML config
    /// 4. Hardcoded default: `{DisplayName}_[year]_[auth]`
    pub(super) fn resolve_citekey_template(&self, type_name: &str, display_name: &str) -> String {
        if let Some(pat) = self.database.jabref_meta.key_patterns.get(type_name) {
            return pat.clone();
        }
        if let Some(ref pat) = self.database.jabref_meta.key_pattern_default {
            return pat.clone();
        }
        if let Some(pat) = self.config.citekey.templates.get(type_name) {
            return pat.clone();
        }
        format!("{}_[year]_[auth]", display_name)
    }

    pub(super) fn regen_citekey(&mut self) {
        if let Some(ref key) = self.detail_entry_key.clone() {
            if let Some(entry) = self.database.entries.get(key) {
                let display_name = entry.entry_type.display_name();
                let type_name = display_name.to_lowercase();
                let template = self.resolve_citekey_template(&type_name, display_name);

                let mut gen_fields = entry.fields.clone();
                gen_fields.entry("entrytype".to_string()).or_insert_with(|| display_name.to_string());
                let base_key = generate_citekey(&template, &gen_fields);
                let new_key = self.unique_citekey(&base_key, key);

                if new_key != *key {
                    // Re-key the entry
                    if let Some(mut entry) = self.database.entries.shift_remove(key) {
                        self.push_undo(UndoItem::CitekeyChanged {
                            old_key: key.clone(),
                            new_key: new_key.clone(),
                            entry_snapshot: entry.clone(),
                        });
                        entry.citation_key = new_key.clone();
                        entry.dirty = true;
                        self.database.entries.insert(new_key.clone(), entry);
                        self.detail_entry_key = Some(new_key);
                        self.sorted_keys = sort_entries(&self.database.entries, &self.config);

                        if let Some(ref mut detail) = self.detail_state {
                            if let Some(entry) = self.database.entries.get(self.detail_entry_key.as_ref().unwrap()) {
                                detail.refresh(entry);
                            }
                        }
                        self.status_message = Some("Citation key regenerated".to_string());
                    }
                }
            }
        }
    }

    /// Regenerate citation keys for all entries using their configured templates.
    /// Returns the number of keys changed.  When `push_undo` is true each rename
    /// is pushed to the undo stack.
    pub(super) fn regen_all_citekeys_impl(&mut self, push_undo: bool) -> usize {
        let keys: Vec<String> = self.database.entries.keys().cloned().collect();
        let mut renamed = 0usize;

        for key in keys {
            let (base_new_key, skip) = {
                let Some(entry) = self.database.entries.get(&key) else { continue };
                let display_name = entry.entry_type.display_name();
                let type_name = display_name.to_lowercase();
                let template = self.resolve_citekey_template(&type_name, display_name);
                let mut gen_fields = entry.fields.clone();
                gen_fields.entry("entrytype".to_string()).or_insert_with(|| display_name.to_string());
                let base = generate_citekey(&template, &gen_fields);
                let skip = base == key;
                (base, skip)
            };

            if skip {
                continue;
            }

            let new_key = self.unique_citekey(&base_new_key, &key);

            if new_key == key {
                continue;
            }

            if let Some(mut entry) = self.database.entries.shift_remove(&key) {
                if push_undo {
                    self.push_undo(UndoItem::CitekeyChanged {
                        old_key: key.clone(),
                        new_key: new_key.clone(),
                        entry_snapshot: entry.clone(),
                    });
                }
                entry.citation_key = new_key.clone();
                entry.dirty = true;
                self.database.entries.insert(new_key.clone(), entry);
                renamed += 1;

                if self.detail_entry_key.as_deref() == Some(key.as_str()) {
                    self.detail_entry_key = Some(new_key);
                }
            }
        }

        renamed
    }

    pub(super) fn regen_all_citekeys(&mut self) {
        let renamed = self.regen_all_citekeys_impl(true);

        if renamed > 0 {
            self.sorted_keys = sort_entries(&self.database.entries, &self.config);
            let new_key = self.detail_entry_key.clone();
            if let (Some(ref key), Some(ref mut detail)) = (new_key, self.detail_state.as_mut()) {
                if let Some(entry) = self.database.entries.get(key) {
                    detail.refresh(entry);
                }
            }
            self.status_message = Some(format!(
                "{} citation key{} regenerated",
                renamed,
                if renamed == 1 { "" } else { "s" }
            ));
        } else {
            self.status_message = Some("All citation keys are up to date".to_string());
        }
    }

    pub(super) fn titlecase_selected_field(&mut self) {
        let field_name = self
            .detail_state
            .as_ref()
            .and_then(|d| d.selected_field())
            .map(|(name, _)| name.to_string());

        if let Some(field_name) = field_name {
            if let Some(key) = self.detail_entry_key.clone() {
                let value = self.database.entries.get(&key)
                    .and_then(|e| e.fields.get(&field_name).cloned());
                if let Some(value) = value {
                    let converted = crate::util::titlecase::apply_titlecase(
                        &value,
                        &self.config.titlecase.ignore_words,
                        &self.config.titlecase.stop_words,
                    );
                    if converted != value {
                        self.push_undo(UndoItem::FieldChanged {
                            entry_key: key.clone(),
                            field_name: field_name.clone(),
                            old_value: Some(value),
                        });
                        if let Some(entry) = self.database.entries.get_mut(&key) {
                            entry.fields.insert(field_name.clone(), converted);
                            entry.dirty = true;
                            let entry_clone = entry.clone();
                            if let Some(ref mut detail) = self.detail_state {
                                detail.refresh(&entry_clone);
                            }
                        }
                        self.status_message = Some(format!("Title-cased '{}'", field_name));
                    } else {
                        self.status_message =
                            Some(format!("'{}' already in title case", field_name));
                    }
                }
            }
        }
    }

    pub(super) fn normalize_names_field(&mut self) {
        const NAME_FIELDS: &[&str] = &[
            "author", "editor", "editora", "editorb", "editorc",
            "bookauthor", "afterword", "translator",
        ];
        let field_name = match self
            .detail_state
            .as_ref()
            .and_then(|d| d.selected_field())
            .map(|(name, _)| name.to_string())
        {
            Some(n) => n,
            None => return,
        };

        if !NAME_FIELDS.iter().any(|&f| f.eq_ignore_ascii_case(&field_name)) {
            self.status_message = Some(format!(
                "'{}' is not a person-name field",
                field_name
            ));
            return;
        }

        if let Some(key) = self.detail_entry_key.clone() {
            let value = self.database.entries.get(&key)
                .and_then(|e| e.fields.get(&field_name).cloned());
            if let Some(value) = value {
                let normalized = crate::util::author::normalize_author_names(&value);
                if normalized != value {
                    self.push_undo(UndoItem::FieldChanged {
                        entry_key: key.clone(),
                        field_name: field_name.clone(),
                        old_value: Some(value),
                    });
                    if let Some(entry) = self.database.entries.get_mut(&key) {
                        entry.fields.insert(field_name.clone(), normalized);
                        entry.dirty = true;
                        let entry_clone = entry.clone();
                        if let Some(ref mut detail) = self.detail_state {
                            detail.refresh(&entry_clone);
                        }
                    }
                    self.status_message =
                        Some(format!("'{}' normalized to 'Last, First' form", field_name));
                } else {
                    self.status_message =
                        Some(format!("'{}' already in 'Last, First' form", field_name));
                }
            }
        }
    }

    pub(super) fn handle_field_editor_action(&mut self, action: Action) {
        match action {
            Action::EditUndo => {
                if let Some(ref mut editor) = self.field_editor_state {
                    editor.undo_edit();
                }
                self.update_field_completions();
            }
            Action::EditPut => {
                // Use unnamed register (set by x/dw/yy) if non-empty, else fall back to clipboard.
                let text_result = self.field_editor_state.as_ref().map(|editor| {
                    if !editor.unnamed_register.is_empty() {
                        Ok(editor.unnamed_register.clone())
                    } else {
                        self.clipboard.paste()
                            .map_err(|e| format!("Clipboard error: {e}"))
                    }
                });
                match text_result {
                    Some(Ok(text)) if !text.is_empty() => {
                        let text = collapse_newlines(&text);
                        if let Some(ref mut editor) = self.field_editor_state {
                            editor.save_undo_snapshot();
                            editor.put(&text);
                        }
                        self.update_field_completions();
                    }
                    Some(Err(e)) => self.status_message = Some(e),
                    _ => {}
                }
            }
            Action::EditYank => {
                if let Some(ref mut editor) = self.field_editor_state {
                    let text = editor.value.clone();
                    editor.unnamed_register = text.clone();
                    match self.clipboard.copy(&text) {
                        Ok(()) => self.status_message = Some(format!("Yanked: {text}")),
                        Err(e) => self.status_message = Some(format!("Clipboard error: {e}")),
                    }
                }
            }
            Action::EditEnterNormal => {
                if let Some(ref mut editor) = self.field_editor_state {
                    editor.enter_normal();
                }
            }
            Action::EditEnterInsert => {
                if let Some(ref mut editor) = self.field_editor_state {
                    editor.save_undo_snapshot();
                    editor.editing_mode = EditingMode::Insert;
                }
            }
            Action::EditEnterInsertAfter => {
                if let Some(ref mut editor) = self.field_editor_state {
                    editor.save_undo_snapshot();
                    // Advance cursor one char past current position (like vim `a`)
                    if editor.cursor < editor.value.len() {
                        let next = editor.value[editor.cursor..]
                            .chars()
                            .next()
                            .map(|c| c.len_utf8())
                            .unwrap_or(0);
                        editor.cursor += next;
                    }
                    editor.editing_mode = EditingMode::Insert;
                }
            }
            Action::EditEnterInsertAtEnd => {
                if let Some(ref mut editor) = self.field_editor_state {
                    editor.save_undo_snapshot();
                    editor.cursor = editor.value.len();
                    editor.editing_mode = EditingMode::Insert;
                }
            }
            Action::EditEnterInsertAtHome => {
                if let Some(ref mut editor) = self.field_editor_state {
                    editor.save_undo_snapshot();
                    editor.cursor = 0;
                    editor.editing_mode = EditingMode::Insert;
                }
            }
            Action::EditEnterReplace => {
                if let Some(ref mut editor) = self.field_editor_state {
                    editor.save_undo_snapshot();
                    editor.replace_undo_stack.clear();
                    editor.editing_mode = EditingMode::Replace;
                }
            }
            Action::EditMoveWordFwd => {
                if let Some(ref mut editor) = self.field_editor_state {
                    editor.move_word_fwd();
                }
            }
            Action::EditMoveWordBwd => {
                if let Some(ref mut editor) = self.field_editor_state {
                    editor.move_word_bwd();
                }
            }
            Action::EditMoveWordEnd => {
                if let Some(ref mut editor) = self.field_editor_state {
                    editor.move_word_end();
                }
            }
            Action::EditMoveBigWordFwd => {
                if let Some(ref mut editor) = self.field_editor_state {
                    editor.move_big_word_fwd();
                }
            }
            Action::EditMoveBigWordBwd => {
                if let Some(ref mut editor) = self.field_editor_state {
                    editor.move_big_word_bwd();
                }
            }
            Action::EditMoveBigWordEnd => {
                if let Some(ref mut editor) = self.field_editor_state {
                    editor.move_big_word_end();
                }
            }
            Action::EditDeleteWordFwd => {
                if let Some(ref mut editor) = self.field_editor_state {
                    editor.save_undo_snapshot();
                    editor.delete_word_fwd();
                }
                self.update_field_completions();
            }
            Action::EditDeleteToEnd => {
                if let Some(ref mut editor) = self.field_editor_state {
                    editor.save_undo_snapshot();
                    editor.delete_to_end();
                }
                self.update_field_completions();
            }
            Action::EditChangeToEnd => {
                if let Some(ref mut editor) = self.field_editor_state {
                    editor.save_undo_snapshot();
                    editor.delete_to_end();
                    editor.editing_mode = EditingMode::Insert;
                }
                self.update_field_completions();
            }
            Action::EditSubstituteChar => {
                if let Some(ref mut editor) = self.field_editor_state {
                    editor.save_undo_snapshot();
                    editor.delete();
                    editor.editing_mode = EditingMode::Insert;
                }
                self.update_field_completions();
            }
            Action::EditSubstituteLine => {
                if let Some(ref mut editor) = self.field_editor_state {
                    editor.save_undo_snapshot();
                    editor.clear_value();
                    editor.editing_mode = EditingMode::Insert;
                }
                self.update_field_completions();
            }
            Action::EditToggleCase => {
                if let Some(ref mut editor) = self.field_editor_state {
                    editor.save_undo_snapshot();
                    editor.toggle_case_at_cursor();
                }
                self.update_field_completions();
            }
            Action::EditReplaceChar(c) => {
                if let Some(ref mut editor) = self.field_editor_state {
                    editor.save_undo_snapshot();
                    editor.replace_char_at_cursor(c);
                }
            }
            Action::EditFindCharFwd(c) => {
                if let Some(ref mut editor) = self.field_editor_state {
                    editor.find_char_fwd(c);
                }
            }
            Action::EditFindCharBwd(c) => {
                if let Some(ref mut editor) = self.field_editor_state {
                    editor.find_char_bwd(c);
                }
            }
            Action::EditFindToCharFwd(c) => {
                if let Some(ref mut editor) = self.field_editor_state {
                    editor.find_to_char_fwd(c);
                }
            }
            Action::EditFindToCharBwd(c) => {
                if let Some(ref mut editor) = self.field_editor_state {
                    editor.find_to_char_bwd(c);
                }
            }
            Action::EditDeleteToChar(c) => {
                if let Some(ref mut editor) = self.field_editor_state {
                    editor.save_undo_snapshot();
                    editor.delete_to_char(c);
                }
                self.update_field_completions();
            }
            Action::EditDeleteThroughChar(c) => {
                if let Some(ref mut editor) = self.field_editor_state {
                    editor.save_undo_snapshot();
                    editor.delete_through_char(c);
                }
                self.update_field_completions();
            }
            Action::EditDeleteToCharBack(c) => {
                if let Some(ref mut editor) = self.field_editor_state {
                    editor.save_undo_snapshot();
                    editor.delete_to_char_back(c);
                }
                self.update_field_completions();
            }
            Action::EditDeleteThroughCharBack(c) => {
                if let Some(ref mut editor) = self.field_editor_state {
                    editor.save_undo_snapshot();
                    editor.delete_through_char_back(c);
                }
                self.update_field_completions();
            }
            Action::EditDeleteCharBack => {
                if let Some(ref mut editor) = self.field_editor_state {
                    editor.save_undo_snapshot();
                    editor.backspace();
                    if editor.editing_mode == EditingMode::Normal
                        && !editor.value.is_empty()
                        && editor.cursor >= editor.value.len()
                    {
                        editor.cursor = editor
                            .value
                            .char_indices()
                            .last()
                            .map(|(i, _)| i)
                            .unwrap_or(0);
                    }
                }
                self.update_field_completions();
            }
            Action::EditDeleteWordBack => {
                if let Some(ref mut editor) = self.field_editor_state {
                    editor.save_undo_snapshot();
                    editor.delete_word_back();
                }
                self.update_field_completions();
            }
            Action::EditDeleteToHome => {
                if let Some(ref mut editor) = self.field_editor_state {
                    editor.save_undo_snapshot();
                    editor.delete_to_home();
                }
                self.update_field_completions();
            }
            Action::EditConfirmAndMoveDown => {
                self.confirm_edit();
                self.move_cursor(1);
                self.start_edit_field();
            }
            Action::EditConfirmAndMoveUp => {
                self.confirm_edit();
                self.move_cursor(-1);
                self.start_edit_field();
            }
            _ => {}
        }
    }
}
