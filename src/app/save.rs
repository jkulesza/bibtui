//! Save pipeline: filename sync, save actions, violations, and
//! raw-file synchronization.

use super::*;

impl App {
    /// Rename attached files to match the citation key, updating the `file` field in place.
    ///
    /// - One file  →  `citekey.ext`
    /// - N files   →  `citekey_1.ext`, `citekey_2.ext`, …
    ///
    /// All entries with a `file` field are processed, regardless of dirty state.
    /// The actual file is renamed on disk; if the rename fails the entry is left unchanged.
    pub(super) fn sync_filenames(&mut self, force: bool) {
        if !force && !self.config.save.sync_filenames {
            return;
        }

        let file_dir = effective_file_dir(
            &self.bib_path,
            self.database.jabref_meta.file_directory.as_deref(),
        );

        let keys: Vec<String> = self
            .database
            .entries
            .iter()
            .filter(|(_, e)| e.fields.contains_key("file"))
            .map(|(k, _)| k.clone())
            .collect();

        let mut rename_msgs: Vec<String> = Vec::new();

        for key in keys {
            let (citekey, file_val) = {
                let entry = &self.database.entries[&key];
                (entry.citation_key.clone(), entry.fields["file"].clone())
            };

            let mut parsed = parse_file_field(&file_val);
            if parsed.is_empty() {
                continue;
            }

            let plans = plan_filename_renames(&citekey, &parsed, &file_dir);
            let mut changed = false;

            for plan in plans {
                if plan.old_abs.exists() {
                    if rename_target_conflicts(&plan.old_abs, &plan.new_abs) {
                        rename_msgs.push(format!(
                            "skipped {}: target {} already exists",
                            plan.old_abs.display(),
                            plan.new_abs.display()
                        ));
                        continue;
                    }
                    if let Err(e) = std::fs::rename(&plan.old_abs, &plan.new_abs) {
                        rename_msgs.push(format!("rename {}: {}", plan.old_abs.display(), e));
                        continue;
                    }
                }
                parsed[plan.index].path = plan.new_rel_path;
                changed = true;
            }

            if changed {
                let new_file_val = serialize_file_field(&parsed);
                if let Some(entry) = self.database.entries.get_mut(&key) {
                    entry.fields.insert("file".to_string(), new_file_val);
                    entry.dirty = true;
                }
            }
        }

        if !rename_msgs.is_empty() {
            self.status_message = Some(format!("File rename errors: {}", rename_msgs.join("; ")));
        }
    }

    /// Rename attached files for the currently open detail entry to match its
    /// citation key, regardless of the `sync_filenames` config setting.
    pub(super) fn sync_entry_filename(&mut self) {
        let key = match self.detail_entry_key.clone() {
            Some(k) => k,
            None => return,
        };
        let entry = match self.database.entries.get(&key) {
            Some(e) => e,
            None => return,
        };
        let old_file_value = match entry.fields.get("file") {
            Some(v) => v.clone(),
            None => {
                self.status_message = Some("No file attachment to sync".to_string());
                return;
            }
        };
        let citekey = entry.citation_key.clone();

        let file_dir = effective_file_dir(
            &self.bib_path,
            self.database.jabref_meta.file_directory.as_deref(),
        );

        let mut parsed = parse_file_field(&old_file_value);
        if parsed.is_empty() {
            self.status_message = Some("No file attachment to sync".to_string());
            return;
        }

        let mut changed = false;
        let mut rename_msgs: Vec<String> = Vec::new();
        // Collect (new_abs, old_abs) pairs for undo.
        let mut undo_renames: Vec<(PathBuf, PathBuf)> = Vec::new();

        let plans = plan_filename_renames(&citekey, &parsed, &file_dir);
        for plan in plans {
            if plan.old_abs.exists() {
                if rename_target_conflicts(&plan.old_abs, &plan.new_abs) {
                    rename_msgs.push(format!(
                        "skipped {}: target {} already exists",
                        plan.old_abs.display(),
                        plan.new_abs.display()
                    ));
                    continue;
                }
                if let Err(e) = std::fs::rename(&plan.old_abs, &plan.new_abs) {
                    rename_msgs.push(format!("rename {}: {}", plan.old_abs.display(), e));
                    continue;
                }
                undo_renames.push((plan.new_abs.clone(), plan.old_abs));
            }

            parsed[plan.index].path = plan.new_rel_path;
            changed = true;
        }

        if !rename_msgs.is_empty() {
            self.status_message = Some(format!("File rename errors: {}", rename_msgs.join("; ")));
            return;
        }

        if changed {
            self.push_undo(UndoItem::FilenamesSynced {
                entry_key: key.clone(),
                old_file_value,
                renames: undo_renames,
            });
            let new_file_val = serialize_file_field(&parsed);
            if let Some(entry) = self.database.entries.get_mut(&key) {
                entry.fields.insert("file".to_string(), new_file_val);
                entry.dirty = true;
            }
            if let Some(ref mut detail) = self.detail_state {
                if let Some(entry) = self.database.entries.get(&key) {
                    detail.refresh(entry);
                }
            }
            self.status_message = Some("File renamed to match citation key".to_string());
        } else {
            self.status_message = Some("File already matches citation key".to_string());
        }
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
            self.save();
            if and_quit {
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

    pub(super) fn save(&mut self) {
        // Rename attached files to match citation keys before serialising.
        self.sync_filenames(false);

        // Apply save actions (field normalisations) to all entries.
        self.apply_save_actions();

        // Regenerate all citation keys from templates (after field normalisations
        // so the keys are based on the final, normalised field values).
        if self.config.save.save_action_regenerate_citekeys {
            let n = self.regen_all_citekeys_impl(false);
            if n > 0 {
                self.sorted_keys = sort_entries(&self.database.entries, &self.config);
            }
        }

        // Backup — only when the file already exists (skip for brand-new libraries).
        if self.config.general.backup_on_save && self.bib_path.exists() {
            let backup_path = self.bib_path.with_extension("bib.bak");
            if let Err(e) = std::fs::copy(&self.bib_path, &backup_path) {
                self.status_message = Some(format!("Backup failed: {}", e));
                return;
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
        let tmp_path = self.bib_path.with_extension("bib.tmp");
        let write_result = std::fs::write(&tmp_path, &output)
            .and_then(|()| std::fs::rename(&tmp_path, &self.bib_path));
        match write_result {
            Ok(()) => {
                self.save_generation = Some(self.undo_stack.len());
                self.dirty = false;
                // Mark all entries clean
                for entry in self.database.entries.values_mut() {
                    entry.dirty = false;
                }
                self.status_message = Some(format!("Saved to {}", self.bib_path.display()));
            }
            Err(e) => {
                // Clean up the temp file so a failed save leaves no debris and
                // the original file is untouched.
                let _ = std::fs::remove_file(&tmp_path);
                self.status_message = Some(format!("Save failed: {}", e));
            }
        }
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

    // Emit one transform per field whose net value differs from the original.
    let mut transforms = Vec::new();
    for (field, new_val) in &field_state {
        let orig = entry.fields.get(*field).cloned().unwrap_or_default();
        if *new_val != orig {
            transforms.push(FieldTransform {
                field: field.to_string(),
                old_value: orig,
                new_value: new_val.clone(),
                action_name: action_label_for_field(field, cfg),
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
/// target conflicts at rename time via [`rename_target_conflicts`]) and keep
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

/// Returns true when renaming `src` to `dest` would clobber an existing,
/// unrelated file.  A destination that resolves (via canonicalization) to the
/// same file as `src` is not a conflict — this permits case-only renames on
/// case-insensitive filesystems.
fn rename_target_conflicts(src: &std::path::Path, dest: &std::path::Path) -> bool {
    if !dest.exists() {
        return false;
    }
    match (src.canonicalize(), dest.canonicalize()) {
        (Ok(a), Ok(b)) => a != b,
        _ => true,
    }
}
