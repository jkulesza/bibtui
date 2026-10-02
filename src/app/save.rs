//! Save pipeline: filename sync, save actions, violations, and
//! raw-file synchronization.

use super::*;

#[derive(Debug, thiserror::Error)]
pub(super) enum SaveError {
    #[error("Save refused: bibliography changed outside bibtui; reload it before saving")]
    ExternalChange,

    #[error("Backup failed: {0}")]
    Backup(std::io::Error),
    #[error("Save failed: {0}")]
    Write(std::io::Error),
}


impl App {
    /// Synchronize all attachments, retaining successful changes and reporting
    /// failures individually. A manual bulk sync is a single undo operation.
    pub(super) fn sync_filenames(&mut self, force: bool) {
        if !force && !self.config.save.sync_filenames { return; }
        let keys: Vec<String> = self.database.entries.keys().cloned().collect();
        let mut undo = Vec::new();
        let mut errors = Vec::new();
        for key in keys {
            let (item, entry_errors) = self.rename_entry_files(&key);
            if let Some(item) = item { undo.push(item); }
            errors.extend(entry_errors);
        }
        if !undo.is_empty() { self.push_undo(UndoItem::Batch(undo)); }
        self.status_message = Some(if errors.is_empty() {
            "Filenames synced to citation keys".to_string()
        } else { format!("File rename errors: {}", errors.join("; ")) });
    }

    pub(super) fn sync_entry_filename(&mut self) {
        let Some(key) = self.detail_entry_key.clone() else { return };
        if !self.database.entries.get(&key).is_some_and(|e| e.fields.contains_key("file")) {
            self.status_message = Some("No file attachment to sync".into());
            return;
        }
        let (item, errors) = self.rename_entry_files(&key);
        let changed = item.is_some();
        if let Some(item) = item { self.push_undo(item); }
        self.status_message = Some(if !errors.is_empty() {
            format!("File rename errors: {}", errors.join("; "))
        } else if changed {
            "File renamed to match citation key".into()
        } else {
            "File already matches citation key".into()
        });
    }

    fn rename_entry_files(&mut self, key: &str) -> (Option<UndoItem>, Vec<String>) {
        let Some(entry) = self.database.entries.get(key) else { return (None, vec![]) };
        let Some(old_file_value) = entry.fields.get("file").cloned() else { return (None, vec![]) };
        let file_dir = effective_file_dir(&self.bib_path, self.database.jabref_meta.file_directory.as_deref());
        let mut files = parse_file_field(&old_file_value);
        let plans = plan_filename_renames(&entry.citation_key, &files, &file_dir);
        let mut renames = Vec::new();
        let mut errors = Vec::new();
        for plan in plans {
            match self.save_io.rename_attachment(&plan.old_abs, &plan.new_abs) {
                Ok(()) => {
                    files[plan.index].path = plan.new_rel_path;
                    renames.push((plan.new_abs, plan.old_abs));
                }
                Err(error) => errors.push(format!("rename {} to {}: {}", plan.old_abs.display(), plan.new_abs.display(), error)),
            }
        }
        if renames.is_empty() { return (None, errors); }
        if let Some(entry) = self.database.entries.get_mut(key) {
            entry.fields.insert("file".into(), serialize_file_field(&files));
            entry.dirty = true;
            if self.detail_entry_key.as_deref() == Some(key) {
                if let Some(detail) = self.detail_state.as_mut() { detail.refresh(entry); }
            }
        }
        (Some(UndoItem::FilenamesSynced { entry_key: key.into(), old_file_value, renames }), errors)
    }

    pub(super) fn undo_filename_sync(&mut self, key: &str, old_value: String, renames: Vec<(PathBuf, PathBuf)>) -> bool {
        let file_dir = effective_file_dir(&self.bib_path, self.database.jabref_meta.file_directory.as_deref());
        let old_files = parse_file_field(&old_value);
        let mut files = self.database.entries.get(key).and_then(|e| e.fields.get("file")).map(|v| parse_file_field(v)).unwrap_or_default();
        let mut errors = Vec::new();
        for (new_abs, old_abs) in renames.into_iter().rev() {
            match self.save_io.rename_attachment(&new_abs, &old_abs) {
                Ok(()) => {
                    for file in &mut files {
                        if crate::util::open::resolve_file_path(&file.path, &file_dir) == new_abs {
                            if let Some(original) = old_files.iter().find(|f| crate::util::open::resolve_file_path(&f.path, &file_dir) == old_abs) {
                                *file = original.clone();
                            }
                        }
                    }
                }
                Err(error) => errors.push(format!("rename {}: {}", new_abs.display(), error)),
            }
        }
        if let Some(entry) = self.database.entries.get_mut(key) {
            entry.fields.insert("file".into(), if errors.is_empty() { old_value } else { serialize_file_field(&files) });
            entry.dirty = true;
            if self.detail_entry_key.as_deref() == Some(key) {
                if let Some(detail) = self.detail_state.as_mut() { detail.refresh(entry); }
            }
        }
        self.status_message = Some(if errors.is_empty() { "Undo: filename sync".into() } else { format!("Undo errors: {}", errors.join("; ")) });
        errors.is_empty()
    }

    /// Compute the (old_filename, new_filename) pairs that `sync_filenames`
    /// would rename, without touching the filesystem.  Returns an empty vec
    /// when nothing would change.  When `force` is false the config guard is
    /// respected and an empty vec is returned if sync is disabled.
    pub(super) fn compute_sync_renames(&self, force: bool) -> Vec<(String, String)> {
        if !force && !self.config.save.sync_filenames {
            return Vec::new();
        }

        let file_dir = effective_file_dir(
            &self.bib_path,
            self.database.jabref_meta.file_directory.as_deref(),
        );

        let mut renames = Vec::new();

        for (_, entry) in &self.database.entries {
            let file_val = match entry.fields.get("file") {
                Some(v) => v,
                None => continue,
            };
            let citekey = &entry.citation_key;
            let parsed = parse_file_field(file_val);
            if parsed.is_empty() {
                continue;
            }
            let multi = parsed.len() > 1;
            for (i, pf) in parsed.iter().enumerate() {
                let old_rel = PathBuf::from(&pf.path);
                let ext = old_rel
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("pdf")
                    .to_string();
                let safe_stem = crate::util::import::sanitize_filename_stem(citekey);
                let new_filename = if multi {
                    format!("{}_{}.{}", safe_stem, i + 1, ext)
                } else {
                    format!("{}.{}", safe_stem, ext)
                };
                if old_rel.file_name().and_then(|n| n.to_str()) == Some(&new_filename) {
                    continue; // Already correctly named.
                }
                let old_display = old_rel
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(&pf.path)
                    .to_string();
                // Show the directory-relative context for absolute paths.
                let old_display = if old_rel.is_absolute() {
                    let rel = old_rel.strip_prefix(&file_dir).unwrap_or(&old_rel);
                    rel.to_string_lossy().into_owned()
                } else {
                    old_display
                };
                renames.push((old_display, new_filename));
            }
        }

        renames.sort();
        renames
    }

    /// Trigger a manual filename-sync: show the preview dialog if any files
    /// would be renamed, or display a status message if everything is already
    /// in sync.  Works regardless of the `sync_filenames` config setting.
    pub(super) fn request_sync_filenames(&mut self) {
        let renames = self.compute_sync_renames(true);
        if renames.is_empty() {
            self.status_message = Some("All filenames already match their citation keys".to_string());
        } else {
            self.dialog_state = Some(DialogState::file_sync_preview(renames));
            self.pending_action = Some(PendingAction::SyncFilenamesOnly);
            self.mode = InputMode::Dialog;
        }
    }

    /// Begin a save, showing a filename-sync preview dialog first if any files
    /// would be renamed.  `and_quit` causes the app to exit after saving.
    pub(super) fn request_save(&mut self, and_quit: bool) {
        let renames = self.compute_sync_renames(false);
        if renames.is_empty() {
            if self.save() && and_quit {
                self.should_quit = true;
            }
        } else {
            self.dialog_state = Some(DialogState::file_sync_preview(renames));
            self.pending_action = Some(if and_quit {
                PendingAction::SaveAndQuit
            } else {
                PendingAction::Save
            });
            self.mode = InputMode::Dialog;
        }
    }

    pub(super) fn save(&mut self) -> bool {
        // Save transformations are staged in memory. On failure restore every
        // persistence-related field, including raw indices and deletion queues.
        let previous_database = self.database.clone();
        let previous_undo = self.undo_stack.clone();
        let previous_generation = self.save_generation;
        let previous_deleted = self.deleted_raw_indices.clone();
        let previous_sorted = self.sorted_keys.clone();
        let previous_detail = self.detail_entry_key.clone();
        match self.try_save() {
            Ok(()) => {
                self.status_message = Some(format!("Saved to {}", self.bib_path.display()));
                true
            }
            Err(error) => {
                self.database = previous_database;
                self.undo_stack = previous_undo;
                self.save_generation = previous_generation;
                self.deleted_raw_indices = previous_deleted;
                self.sorted_keys = previous_sorted;
                self.detail_entry_key = previous_detail;
                self.dirty = true;
                self.status_message = Some(error.to_string());
                false
            }
        }
    }

    fn verify_saved_contents(&self) -> std::result::Result<(), SaveError> {
        let current = self.save_io.read(&self.bib_path).map_err(SaveError::Write)?;
        if current != self.saved_contents {
            return Err(SaveError::ExternalChange);
        }
        Ok(())
    }

    fn try_save(&mut self) -> std::result::Result<(), SaveError> {
        self.verify_saved_contents()?;
        // Rename attached files to match citation keys before serialising.
        self.sync_filenames(false);

        // Apply save actions (field normalisations) to all entries.
        self.apply_save_actions();

        // Regenerate all citation keys from templates (after field normalisations
        // so the keys are based on the final, normalised field values).
        if self.config.save.save_action_regenerate_citekeys {
            // Record automatic renames as a batch so older undo records are
            // reached only after their original keys have been restored.
            let n = self.regen_all_citekeys_impl(true);
            if n > 0 {
                self.sorted_keys = sort_entries(&self.database.entries, &self.config);
            }
        }

        // Backup — only when the file already exists (skip for brand-new libraries).
        if self.config.general.backup_on_save && self.bib_path.exists() {
            let backup_path = self.bib_path.with_extension("bib.bak");
            if let Err(e) = self.save_io.backup(&self.bib_path, &backup_path) {
                return Err(SaveError::Backup(e));
            }
        }

        // Update raw file for dirty entries
        self.sync_dirty_entries();

        // Re-order entries in the raw file if configured.
        self.sort_entries_for_save();

        // Write (normalise blank lines so no more than one blank line appears anywhere).
        // Write atomically: write to a sibling temp file then rename over the
        // target, so a crash or disk-full mid-write cannot truncate the original.
        let output = normalize_blank_lines(write_bib_file(&self.database.raw_file));
        // Recheck after preparation/backup too. This detects external changes,
        // but is not an interprocess lock against a writer racing the rename.
        self.verify_saved_contents()?;
        self.save_io.persist(&self.bib_path, output.as_bytes()).map_err(SaveError::Write)?;
        self.saved_contents = Some(output.into_bytes());
        self.save_generation = Some(self.undo_stack.len());
        self.dirty = false;
        for entry in self.database.entries.values_mut() {
            entry.dirty = false;
        }
        Ok(())
    }

    /// Dry-run of [`apply_save_actions`]: returns every field that *would* change
    /// without mutating the database.  Violations are listed in the same stable
    /// order as the real save actions, and — because both paths share
    /// [`compute_save_transforms`] — the predicted new values match exactly what
    /// a subsequent save produces.
    pub(super) fn compute_violations(&self) -> Vec<Violation> {
        let cfg = &self.config.save;
        let mut violations: Vec<Violation> = Vec::new();

        for (key, entry) in &self.database.entries {
            for t in compute_save_transforms(cfg, entry) {
                violations.push(Violation {
                    entry_key: key.clone(),
                    field: t.field,
                    old_value: t.old_value,
                    new_value: t.new_value,
                    action_name: t.action_name,
                });
            }
        }

        violations
    }

    /// Apply the enabled save actions to every entry in the database.
    ///
    /// Entries whose fields change are marked dirty so they are re-serialised.
    /// The transform ordering is defined by [`compute_save_transforms`], which is
    /// also what the dry-run validator uses, so validate and save never drift.
    pub(super) fn apply_save_actions(&mut self) {
        let cfg = self.config.save.clone();
        let keys: Vec<String> = self.database.entries.keys().cloned().collect();

        for key in &keys {
            let transforms = match self.database.entries.get(key) {
                Some(e) => compute_save_transforms(&cfg, e),
                None => continue,
            };
            if transforms.is_empty() {
                continue;
            }
            if let Some(entry) = self.database.entries.get_mut(key) {
                for t in transforms {
                    entry.fields.insert(t.field, t.new_value);
                }
                entry.dirty = true;
            }
        }
    }

    pub(super) fn sync_dirty_entries(&mut self) {
        let sort_fields = self.config.save.field_order == "alphabetical";
        // When alphabetical ordering is enabled, re-serialize all entries (not just
        // dirty ones) so that existing entries also get their fields reordered.
        let keys_to_sync: Vec<String> = self
            .database
            .entries
            .iter()
            .filter(|(_, e)| e.dirty || sort_fields)
            .map(|(k, _)| k.clone())
            .collect();

        // Phase 1: update existing raw items in place. These don't change the
        // item count, so every recorded raw_index (including the queued deletion
        // indices) stays valid throughout this phase.
        let mut new_keys: Vec<String> = Vec::new();
        for key in keys_to_sync {
            if let Some(entry) = self.database.entries.get(&key) {
                if entry.raw_index < self.database.raw_file.items.len() {
                    // Reuse the original raw values for fields the user did not
                    // change (preserves `#` concatenation and @String references).
                    let original = match &self.database.raw_file.items[entry.raw_index] {
                        RawItem::Entry(re) if re.citation_key == entry.citation_key => {
                            Some(re)
                        }
                        _ => None,
                    };
                    let serialized = serialize_entry(
                        entry,
                        self.config.save.align_fields,
                        sort_fields,
                        original,
                    );
                    let merged_fields = merged_raw_fields(entry, original);
                    self.database.raw_file.items[entry.raw_index] =
                        RawItem::Entry(RawEntry {
                            entry_type: entry.entry_type.display_name().to_string(),
                            citation_key: entry.citation_key.clone(),
                            // Keep raw values so a later save can still tell
                            // which fields are unchanged.
                            fields: merged_fields,
                            raw_text: serialized,
                        });
                } else {
                    new_keys.push(key);
                }
            }
        }

        // Phase 2: remove raw items for deleted entries. The queued indices are
        // relative to the pre-save layout, so this must run before any insertion.
        // Process in reverse index order so earlier removals don't shift later ones.
        if !self.deleted_raw_indices.is_empty() {
            let mut to_remove = self.deleted_raw_indices.drain(..).collect::<Vec<_>>();
            to_remove.sort_unstable_by(|a, b| b.cmp(a)); // descending
            to_remove.dedup();
            for idx in to_remove {
                if idx < self.database.raw_file.items.len() {
                    self.database.raw_file.items.remove(idx);
                    // Also remove the preceding blank Preamble separator if present
                    if idx > 0 {
                        if let Some(RawItem::Preamble(s)) =
                            self.database.raw_file.items.get(idx - 1)
                        {
                            if s.trim().is_empty() {
                                self.database.raw_file.items.remove(idx - 1);
                            }
                        }
                    }
                }
            }
        }

        // Phase 3: insert new entries (raw_index == usize::MAX) before the JabRef
        // @Comment blocks.
        for key in new_keys {
            if let Some(entry) = self.database.entries.get(&key) {
                let serialized =
                    serialize_entry(entry, self.config.save.align_fields, sort_fields, None);
                let insert_pos = self
                    .database
                    .raw_file
                    .items
                    .iter()
                    .position(|item| matches!(item, RawItem::Comment { .. }))
                    .unwrap_or(self.database.raw_file.items.len());

                // Insert: blank line, entry, blank line (before @Comment).
                // normalize_blank_lines will collapse any excess, ensuring exactly
                // one blank line on each side of the new entry.
                self.database.raw_file.items.insert(
                    insert_pos,
                    RawItem::Preamble("\n".to_string()),
                );
                self.database.raw_file.items.insert(
                    insert_pos + 1,
                    RawItem::Entry(RawEntry {
                        entry_type: entry.entry_type.display_name().to_string(),
                        citation_key: entry.citation_key.clone(),
                        fields: merged_raw_fields(entry, None),
                        raw_text: serialized,
                    }),
                );
                self.database.raw_file.items.insert(
                    insert_pos + 2,
                    RawItem::Preamble("\n".to_string()),
                );
            }
        }

        // Phase 4: rebuild raw_index for every entry. The removals and insertions
        // above shift item positions, and this must hold even when entry sorting
        // is disabled (sort_entries_for_save only rebuilds when it actually sorts).
        for (i, item) in self.database.raw_file.items.iter().enumerate() {
            if let RawItem::Entry(re) = item {
                if let Some(db_entry) = self.database.entries.get_mut(&re.citation_key) {
                    db_entry.raw_index = i;
                }
            }
        }
    }

    /// Reorder `Entry` slots in `raw_file.items` according to `config.save.entry_sort_order`,
    /// then rebuild `raw_index` in all database entries so future saves remain correct.
    pub(super) fn sort_entries_for_save(&mut self) {
        if self.config.save.entry_sort_order == "none" {
            return;
        }

        // Positions of every Entry item in the raw list.
        let entry_indices: Vec<usize> = self
            .database
            .raw_file
            .items
            .iter()
            .enumerate()
            .filter_map(|(i, item)| if matches!(item, RawItem::Entry(_)) { Some(i) } else { None })
            .collect();

        if entry_indices.len() < 2 {
            return;
        }

        // Extract those items, sort them by citation key (case-insensitive).
        let mut sorted: Vec<RawItem> = entry_indices
            .iter()
            .map(|&i| self.database.raw_file.items[i].clone())
            .collect();
        sorted.sort_by(|a, b| {
            let ka = if let RawItem::Entry(e) = a { e.citation_key.to_lowercase() } else { String::new() };
            let kb = if let RawItem::Entry(e) = b { e.citation_key.to_lowercase() } else { String::new() };
            ka.cmp(&kb)
        });

        // Write them back into the same index slots (non-entry items stay put).
        for (&idx, item) in entry_indices.iter().zip(sorted) {
            self.database.raw_file.items[idx] = item;
        }

        // Rebuild raw_index for every database entry so future saves are correct.
        for (i, item) in self.database.raw_file.items.iter().enumerate() {
            if let RawItem::Entry(re) = item {
                if let Some(db_entry) = self.database.entries.get_mut(&re.citation_key) {
                    db_entry.raw_index = i;
                }
            }
        }
    }
}

/// Text fields that contain natural-language prose / titles.
const TEXT: &[&str] = &[
    "abstract", "addendum", "address", "annote",
    "booktitle", "chapter", "edition", "institution", "journal",
    "keywords", "language", "note", "organization", "publisher",
    "school", "series", "subtitle", "title", "titleaddon", "type",
    "venue",
];

/// Person-name (name-list) fields.
const NAMES: &[&str] = &[
    "author", "editor", "editora", "editorb", "editorc",
    "bookauthor", "afterword", "translator",
];

/// A single net field change produced by the enabled save actions.
pub(super) struct FieldTransform {
    pub field: String,
    pub old_value: String,
    pub new_value: String,
    /// Short label for the save action responsible for this change.
    pub action_name: &'static str,
}

/// Compute the net field changes the enabled save actions would produce for a
/// single entry, in stable action order (unicode→latex first, then escapes,
/// cleanup, normalisations, person-name normalisation, and journal abbreviation
/// last).  This is the single shared pipeline used by both the dry-run
/// validator ([`App::compute_violations`]) and the real save
/// ([`App::apply_save_actions`]), so the predicted and applied values agree.
pub(super) fn compute_save_transforms(
    cfg: &crate::config::schema::SaveConfig,
    entry: &Entry,
) -> Vec<FieldTransform> {
    // Simulate each save action on a per-field working value, accumulating
    // changes so the final value reflects the net effect of all actions.
    let mut field_state: IndexMap<&str, String> = IndexMap::new();
    let relevant: Vec<&str> = TEXT
        .iter()
        .copied()
        .chain(NAMES.iter().copied())
        .chain(["url", "date", "month", "pages", "isbn"])
        .collect();

    for &f in &relevant {
        if let Some(v) = entry.fields.get(f) {
            field_state.insert(f, v.clone());
        }
    }

    macro_rules! transform {
        ($field:expr, $fn:expr) => {{
            if let Some(val) = field_state.get_mut($field) {
                *val = $fn(val.as_str());
            }
        }};
    }

    // 1. Unicode → LaTeX (run first so later escaping sees LaTeX text)
    if cfg.save_action_unicode_to_latex {
        for f in TEXT.iter().copied().chain(NAMES.iter().copied()) {
            transform!(f, unicode_to_latex);
        }
    }
    // 2. Escape underscores
    if cfg.save_action_escape_underscores {
        for f in TEXT.iter().copied() {
            transform!(f, escape_underscores);
        }
    }
    // 3. Escape ampersands
    if cfg.save_action_escape_ampersands {
        for f in TEXT.iter().copied().chain(NAMES.iter().copied()) {
            transform!(f, escape_ampersands);
        }
    }
    // 4. LaTeX cleanup (% escaping, space collapsing)
    if cfg.save_action_latex_cleanup {
        for f in TEXT.iter().copied() {
            transform!(f, latex_cleanup);
        }
    }
    // 5. URL cleanup
    if cfg.save_action_cleanup_url {
        transform!("url", cleanup_url);
    }
    // 6. Ordinals to superscript
    if cfg.save_action_ordinals_to_superscript {
        for f in TEXT.iter().copied() {
            transform!(f, ordinals_to_superscript);
        }
    }
    // 7. Normalise date
    if cfg.save_action_normalize_date {
        transform!("date", normalize_date);
    }
    // 8. Normalise month
    if cfg.save_action_normalize_month {
        transform!("month", normalize_month);
    }
    // 9. Normalise page numbers
    if cfg.save_action_normalize_page_numbers {
        transform!("pages", normalize_page_numbers);
    }
    // 10. Normalise ISBN
    if cfg.save_action_normalize_isbn {
        transform!("isbn", normalize_isbn);
    }
    // 11. Normalise person names
    if cfg.save_action_normalize_names_of_persons {
        for f in NAMES.iter().copied() {
            transform!(f, crate::util::author::normalize_author_names);
        }
    }

    // 12. Abbreviate journal — sync journal, journal_full, journal_abbrev.
    //     Read the post-transform journal value so validate and save agree.
    if cfg.save_action_abbreviate_journal {
        let journal_current = field_state
            .get("journal")
            .cloned()
            .or_else(|| entry.fields.get("journal").cloned())
            .unwrap_or_default();

        if !journal_current.is_empty() {
            // journal_full is the source of truth; fall back to the current
            // (post-transform) journal value on first run.
            let full = entry
                .fields
                .get("journal_full")
                .filter(|v| !v.is_empty())
                .cloned()
                .unwrap_or_else(|| journal_current.clone());

            let abbrev =
                crate::util::journal::abbreviate_journal(&full, &cfg.journal_abbreviations);
            let preferred = if cfg.journal_field_content == "abbreviated" {
                abbrev.clone()
            } else {
                full.clone()
            };

            // Write results back into the working state so each field is emitted
            // exactly once carrying its final value (`journal` overwrites any
            // earlier text-action result in place).
            field_state.insert("journal_full", full);
            field_state.insert("journal_abbrev", abbrev);
            field_state.insert("journal", preferred);
        }
    }

    // 13. Trim padding whitespace.  Unlike every action above this one applies
    //     to *all* fields, and it runs last so it also removes padding the
    //     other actions leave behind (e.g. latex_cleanup collapsing the "  " of
    //     "  Foo" down to a single leading space).
    let mut trim_only: std::collections::HashSet<&str> = std::collections::HashSet::new();
    if cfg.save_action_trim_whitespace {
        // Fields outside the lists above have not been touched yet; pull them
        // into the working state so the trim reaches every field.
        for (field, val) in &entry.fields {
            if !field_state.contains_key(field.as_str()) {
                field_state.insert(field.as_str(), val.clone());
            }
        }
        for (field, val) in field_state.iter_mut() {
            let trimmed = trim_field_whitespace(val);
            if trimmed != *val {
                // When the pipeline above left the value alone, padding is the
                // only thing that changed — attribute it to the trim.
                let orig = entry.fields.get(*field).map(String::as_str).unwrap_or("");
                if val.as_str() == orig {
                    trim_only.insert(*field);
                }
                *val = trimmed;
            }
        }
    }

    // Emit one transform per field whose net value differs from the original.
    let mut transforms = Vec::new();
    for (field, new_val) in &field_state {
        let orig = entry.fields.get(*field).cloned().unwrap_or_default();
        if *new_val != orig {
            transforms.push(FieldTransform {
                field: field.to_string(),
                old_value: orig,
                new_value: new_val.clone(),
                action_name: if trim_only.contains(*field) {
                    "trim_whitespace"
                } else {
                    action_label_for_field(field, cfg)
                },
            });
        }
    }

    transforms
}

/// Return a short label describing which save action is responsible for
/// the change in `field`.  Used in the Validate results popup.
pub(super) fn action_label_for_field(field: &str, cfg: &crate::config::schema::SaveConfig) -> &'static str {
    match field {
        "url" if cfg.save_action_cleanup_url => "cleanup_url",
        "date" if cfg.save_action_normalize_date => "normalize_date",
        "month" if cfg.save_action_normalize_month => "normalize_month",
        "pages" if cfg.save_action_normalize_page_numbers => "normalize_pages",
        "isbn" if cfg.save_action_normalize_isbn => "normalize_isbn",
        "author" | "editor" | "editora" | "editorb" | "editorc"
        | "bookauthor" | "translator"
            if cfg.save_action_normalize_names_of_persons =>
        {
            "normalize_names"
        }
        "journal_full" | "journal_abbrev" if cfg.save_action_abbreviate_journal => {
            "abbreviate_journal"
        }
        "journal" if cfg.save_action_abbreviate_journal => "abbreviate_journal",
        _ => {
            // For text fields the first applicable action wins (same order as apply_save_actions)
            if cfg.save_action_unicode_to_latex {
                "unicode→latex"
            } else if cfg.save_action_escape_underscores {
                "esc_underscores"
            } else if cfg.save_action_escape_ampersands {
                "esc_ampersands"
            } else if cfg.save_action_latex_cleanup {
                "latex_cleanup"
            } else if cfg.save_action_ordinals_to_superscript {
                "ordinals"
            } else {
                "save_action"
            }
        }
    }
}

pub(super) fn sort_entries(entries: &IndexMap<String, Entry>, config: &Config) -> Vec<String> {
    let keys: Vec<String> = entries.keys().cloned().collect();

    let field = &config.display.default_sort.field;
    if field == "none" {
        return keys;
    }

    let ascending = config.display.default_sort.ascending;
    let mut keys = keys;
    keys.sort_by(|a, b| {
        let ea = entries.get(a);
        let eb = entries.get(b);

        let va = ea.map(|e| get_sort_value(e, field)).unwrap_or_default();
        let vb = eb.map(|e| get_sort_value(e, field)).unwrap_or_default();

        let ord = compare_sort_values(field, &va, &vb);
        if ascending {
            ord
        } else {
            ord.reverse()
        }
    });

    keys
}

/// Fields that are compared numerically rather than lexically.
const NUMERIC_SORT_FIELDS: &[&str] = &["year", "volume", "number", "pages"];

/// Compare two sort values in ascending order.
///
/// Numeric-aware behavior applies when the sort field is `year`, `volume`,
/// `number`, or `pages`, or when both values parse fully as integers. In those
/// cases values are compared as numbers so that `"9"` sorts before `"10"`.
///
/// For `pages`, the leading integer of each value is used (e.g. `"123--130"`
/// compares as `123`). Ordering within a numeric field is:
/// 1. empty values first (matching the previous string-based behavior),
/// 2. values with a numeric key, in numeric order,
/// 3. values with no leading integer last, in lexical order.
///
/// When neither trigger applies, the original case-relevant string comparison
/// is used.
pub(super) fn compare_sort_values(field: &str, a: &str, b: &str) -> std::cmp::Ordering {
    if NUMERIC_SORT_FIELDS.contains(&field) {
        return numeric_sort_key(field, a).cmp(&numeric_sort_key(field, b));
    }
    // For any other field, compare numerically only when both values are
    // integers; otherwise fall back to the original string comparison.
    if let (Ok(ia), Ok(ib)) = (a.trim().parse::<i64>(), b.trim().parse::<i64>()) {
        return ia.cmp(&ib);
    }
    a.cmp(b)
}

/// Build an ascending sort key for a numeric field: `(tier, number, text)`.
///
/// Tier 0 is the empty value (sorts first), tier 1 is a value with a numeric
/// key (compared by `number`), and tier 2 is a value with no leading integer
/// (sorts last, compared lexically by `text`).
fn numeric_sort_key(field: &str, value: &str) -> (u8, i64, String) {
    let v = value.trim();
    if v.is_empty() {
        return (0, 0, String::new());
    }
    let parsed = if field == "pages" {
        leading_integer(v)
    } else {
        v.parse::<i64>().ok()
    };
    match parsed {
        Some(n) => (1, n, String::new()),
        None => (2, 0, v.to_string()),
    }
}

/// Parse the leading run of ASCII digits as an integer (e.g. `"123--130"` →
/// `Some(123)`). Returns `None` when the value does not start with a digit.
fn leading_integer(v: &str) -> Option<i64> {
    let digits: String = v.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse::<i64>().ok()
}

pub(super) fn get_sort_value(entry: &Entry, field: &str) -> String {
    match field {
        "citation_key" | "key" | "citekey" => entry.citation_key.clone(),
        "entrytype" | "type" => entry.entry_type.display_name().to_string(),
        _ => entry.fields.get(field).cloned().unwrap_or_default(),
    }
}

/// One attachment rename planned by [`plan_filename_renames`].
struct PlannedRename {
    /// Index of the attachment in the parsed `file` field.
    index: usize,
    /// Absolute path of the file as currently referenced.
    old_abs: PathBuf,
    /// Absolute path the file should be renamed to.
    new_abs: PathBuf,
    /// New value for `ParsedFile::path`, preserving relative vs absolute form.
    new_rel_path: String,
}

/// Plan the renames needed to make an entry's attachments match its citation
/// key: one file becomes `citekey.ext`, N files become `citekey_1.ext` …
/// `citekey_N.ext`.  Attachments already correctly named are omitted.  No
/// filesystem changes are made; callers perform the renames (checking for
/// target conflicts atomically when creating the destination) and keep
/// their own undo/status handling.
fn plan_filename_renames(
    citekey: &str,
    parsed: &[crate::util::open::ParsedFile],
    file_dir: &std::path::Path,
) -> Vec<PlannedRename> {
    let multi = parsed.len() > 1;
    let safe_stem = crate::util::import::sanitize_filename_stem(citekey);
    let mut plans = Vec::new();

    for (i, pf) in parsed.iter().enumerate() {
        let old_rel = PathBuf::from(&pf.path);
        let ext = old_rel
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("pdf")
            .to_string();

        let new_filename = if multi {
            format!("{}_{}.{}", safe_stem, i + 1, ext)
        } else {
            format!("{}.{}", safe_stem, ext)
        };

        // Already correctly named?
        if old_rel.file_name().and_then(|n| n.to_str()) == Some(&new_filename) {
            continue;
        }

        // Resolve to absolute paths.
        let old_abs = if old_rel.is_absolute() {
            old_rel.clone()
        } else {
            file_dir.join(&old_rel)
        };
        let new_abs = old_abs
            .parent()
            .map(|p| p.join(&new_filename))
            .unwrap_or_else(|| file_dir.join(&new_filename));

        // New stored path, preserving relative vs absolute form (and the
        // original subdirectory for relative paths).
        let new_rel_path = if old_rel.is_absolute() {
            new_abs.to_string_lossy().into_owned()
        } else {
            old_rel
                .parent()
                .map(|p| p.join(&new_filename))
                .unwrap_or_else(|| PathBuf::from(&new_filename))
                .to_string_lossy()
                .into_owned()
        };

        plans.push(PlannedRename {
            index: i,
            old_abs,
            new_abs,
            new_rel_path,
        });
    }

    plans
}

