//! Save pipeline: filename sync, save actions, violations, and
//! raw-file synchronization.

use super::*;

#[derive(Debug, thiserror::Error)]
pub(super) enum SaveError {
    #[error(
        "Save refused: bibliography changed outside bibtui; reload it, or use :w! to overwrite (the external version is kept in .bib.bak)"
    )]
    ExternalChange,
    #[error("Save failed: {0}")]
    Plan(String),

    #[error("Backup failed: {0}")]
    Backup(std::io::Error),
    #[error("Save failed: {0}")]
    Write(std::io::Error),
}

#[derive(Clone)]
struct SaveState {
    database: Database,
    undo: Vec<UndoItem>,
    generation: Option<usize>,
    deleted: Vec<usize>,
    sorted: Vec<String>,
    detail_key: Option<String>,
    dirty: bool,
    view_dirty: bool,
}

impl SaveState {
    fn capture(app: &App) -> Self {
        Self {
            database: app.database.clone(),
            undo: app.undo_stack.clone(),
            generation: app.save_generation,
            deleted: app.deleted_raw_indices.clone(),
            sorted: app.sorted_keys.clone(),
            detail_key: app.detail_entry_key.clone(),
            dirty: app.dirty,
            view_dirty: app.view_dirty,
        }
    }
    /// Install `self` into `app` and return the state it replaces, moving
    /// rather than cloning the database and undo stack.
    fn swap_into(self, app: &mut App) -> Self {
        Self {
            database: std::mem::replace(&mut app.database, self.database),
            undo: std::mem::replace(&mut app.undo_stack, self.undo),
            generation: std::mem::replace(&mut app.save_generation, self.generation),
            deleted: std::mem::replace(&mut app.deleted_raw_indices, self.deleted),
            sorted: std::mem::replace(&mut app.sorted_keys, self.sorted),
            detail_key: std::mem::replace(&mut app.detail_entry_key, self.detail_key),
            dirty: std::mem::replace(&mut app.dirty, self.dirty),
            view_dirty: std::mem::replace(&mut app.view_dirty, self.view_dirty),
        }
    }

    fn restore(self, app: &mut App) {
        app.database = self.database;
        app.undo_stack = self.undo;
        app.save_generation = self.generation;
        app.deleted_raw_indices = self.deleted;
        app.sorted_keys = self.sorted;
        app.detail_entry_key = self.detail_key;
        app.dirty = self.dirty;
        app.view_dirty = self.view_dirty;
    }
}

pub(super) struct SavePlan {
    original: Database,
    path: PathBuf,
    staged: SaveState,
    output: String,
    renames: Vec<PlannedRename>,
    /// Attachments that need renaming but cannot be (missing from disk).
    skipped: Vec<String>,
    /// File contents the plan expects on disk until it is persisted.
    baseline: Option<Vec<u8>>,
    /// The plan deliberately overwrites changes made outside bibtui (`:w!`).
    overrides_external: bool,
}

impl App {
    /// Synchronize all attachments, retaining successful changes and reporting
    /// failures individually. A manual bulk sync is a single undo operation.
    pub(super) fn sync_filenames(&mut self, force: bool) {
        if !force && !self.config.save.sync_filenames {
            return;
        }
        let keys: Vec<String> = self.database.entries.keys().cloned().collect();
        let mut undo = Vec::new();
        let mut errors = Vec::new();
        for key in keys {
            let (item, entry_errors) = self.rename_entry_files(&key);
            if let Some(item) = item {
                undo.push(item);
            }
            errors.extend(entry_errors);
        }
        if !undo.is_empty() {
            self.push_undo(UndoItem::Batch(undo));
        }
        self.status_message = Some(if errors.is_empty() {
            "Filenames synced to citation keys".to_string()
        } else {
            format!("File rename errors: {}", errors.join("; "))
        });
    }

    pub(super) fn sync_entry_filename(&mut self) {
        let Some(key) = self.detail_entry_key.clone() else {
            return;
        };
        if !self
            .database
            .entries
            .get(&key)
            .is_some_and(|e| e.fields.contains_key("file"))
        {
            self.status_message = Some("No file attachment to sync".into());
            return;
        }
        let (item, errors) = self.rename_entry_files(&key);
        let changed = item.is_some();
        if let Some(item) = item {
            self.push_undo(item);
        }
        self.status_message = Some(if !errors.is_empty() {
            format!("File rename errors: {}", errors.join("; "))
        } else if changed {
            "File renamed to match citation key".into()
        } else {
            "File already matches citation key".into()
        });
    }

    fn rename_entry_files(&mut self, key: &str) -> (Option<UndoItem>, Vec<String>) {
        let Some(entry) = self.database.entries.get(key) else {
            return (None, vec![]);
        };
        let Some(old_file_value) = entry.fields.get("file").cloned() else {
            return (None, vec![]);
        };
        let file_dir = effective_file_dir(
            &self.bib_path,
            self.database.jabref_meta.file_directory.as_deref(),
        );
        let mut files = parse_file_field(&old_file_value);
        let (plans, skipped) = plan_filename_renames(&entry.citation_key, &files, &file_dir);
        let mut renames = Vec::new();
        let mut errors: Vec<String> = skipped
            .into_iter()
            .map(|note| format!("not renamed: {note}"))
            .collect();
        for plan in plans {
            match self.save_io.rename_attachment(&plan.old_abs, &plan.new_abs) {
                Ok(()) => {
                    files[plan.index].path = plan.new_rel_path;
                    renames.push((plan.new_abs, plan.old_abs));
                }
                Err(error) => errors.push(format!(
                    "rename {} to {}: {}",
                    plan.old_abs.display(),
                    plan.new_abs.display(),
                    error
                )),
            }
        }
        if renames.is_empty() {
            return (None, errors);
        }
        if let Some(entry) = self.database.entries.get_mut(key) {
            entry
                .fields
                .insert("file".into(), serialize_file_field(&files));
            entry.dirty = true;
            if self.detail_entry_key.as_deref() == Some(key) {
                if let Some(detail) = self.detail_state.as_mut() {
                    detail.refresh(entry);
                }
            }
        }
        (
            Some(UndoItem::FilenamesSynced {
                entry_key: key.into(),
                old_file_value,
                renames,
            }),
            errors,
        )
    }

    pub(super) fn undo_filename_sync(
        &mut self,
        key: &str,
        old_value: String,
        renames: Vec<(PathBuf, PathBuf)>,
    ) -> bool {
        let file_dir = effective_file_dir(
            &self.bib_path,
            self.database.jabref_meta.file_directory.as_deref(),
        );
        let old_files = parse_file_field(&old_value);
        let mut files = self
            .database
            .entries
            .get(key)
            .and_then(|e| e.fields.get("file"))
            .map(|v| parse_file_field(v))
            .unwrap_or_default();
        let mut errors = Vec::new();
        for (new_abs, old_abs) in renames.into_iter().rev() {
            match self.save_io.rename_attachment(&new_abs, &old_abs) {
                Ok(()) => {
                    for file in &mut files {
                        if crate::util::open::resolve_file_path(&file.path, &file_dir) == new_abs {
                            if let Some(original) = old_files.iter().find(|f| {
                                crate::util::open::resolve_file_path(&f.path, &file_dir) == old_abs
                            }) {
                                *file = original.clone();
                            }
                        }
                    }
                }
                Err(error) => errors.push(format!("rename {}: {}", new_abs.display(), error)),
            }
        }
        if let Some(entry) = self.database.entries.get_mut(key) {
            entry.fields.insert(
                "file".into(),
                if errors.is_empty() {
                    old_value
                } else {
                    serialize_file_field(&files)
                },
            );
            entry.dirty = true;
            if self.detail_entry_key.as_deref() == Some(key) {
                if let Some(detail) = self.detail_state.as_mut() {
                    detail.refresh(entry);
                }
            }
        }
        self.status_message = Some(if errors.is_empty() {
            "Undo: filename sync".into()
        } else {
            format!("Undo errors: {}", errors.join("; "))
        });
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
            let parsed = parse_file_field(file_val);
            // Share planning with the actual sync so the preview omits the
            // same remote and missing attachments.
            let (plans, _) = plan_filename_renames(&entry.citation_key, &parsed, &file_dir);
            for plan in plans {
                let pf = &parsed[plan.index];
                let old_rel = PathBuf::from(&pf.path);
                let new_filename = plan
                    .new_abs
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
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
            self.status_message =
                Some("All filenames already match their citation keys".to_string());
        } else {
            self.dialog_state = Some(DialogState::file_sync_preview(renames));
            self.pending_action = Some(PendingAction::SyncFilenamesOnly);
            self.mode = InputMode::Dialog;
        }
    }

    /// Preview and execute the same immutable plan based on final field values
    /// and citation keys. No files or live document state change during planning.
    pub(super) fn request_save(&mut self, and_quit: bool) {
        self.request_save_with(and_quit, false);
    }

    /// `force` (`:w!`) accepts the current on-disk file as the baseline, so a
    /// bibliography changed outside bibtui is overwritten. The external version
    /// is always backed up to `.bib.bak` first.
    pub(super) fn request_save_with(&mut self, and_quit: bool, force: bool) {
        let plan = match self.prepare_save_plan(force) {
            Ok(plan) => plan,
            Err(error) => {
                self.status_message = Some(error.to_string());
                self.dirty = true;
                return;
            }
        };
        let preview: Vec<_> = plan
            .renames
            .iter()
            .map(|p| {
                (
                    p.old_abs.display().to_string(),
                    p.new_abs.display().to_string(),
                )
            })
            .collect();
        self.pending_save = Some(plan);
        if preview.is_empty() {
            if self.save() && and_quit {
                self.should_quit = true;
            }
        } else {
            self.dialog_state = Some(DialogState::file_sync_preview(preview));
            self.pending_action = Some(if and_quit {
                PendingAction::SaveAndQuit
            } else {
                PendingAction::Save
            });
            self.mode = InputMode::Dialog;
        }
    }

    pub(super) fn save(&mut self) -> bool {
        let plan = self
            .pending_save
            .take()
            .map(Ok)
            .unwrap_or_else(|| self.prepare_save_plan(false));
        let mut skipped = Vec::new();
        let result = plan.and_then(|plan| {
            skipped = plan.skipped.clone();
            self.execute_save_plan(plan)
        });
        match result {
            Ok(()) => {
                let mut message = format!("Saved to {}", self.bib_path.display());
                if !skipped.is_empty() {
                    message.push_str(&format!(
                        "; {} attachment{} not renamed: {}",
                        skipped.len(),
                        if skipped.len() == 1 { "" } else { "s" },
                        skipped.join("; ")
                    ));
                }
                self.status_message = Some(message);
                true
            }
            Err(error) => {
                self.dirty = true;
                self.status_message = Some(error.to_string());
                false
            }
        }
    }

    fn verify_disk_contents(
        &self,
        expected: &Option<Vec<u8>>,
    ) -> std::result::Result<(), SaveError> {
        let current = self
            .save_io
            .read(&self.bib_path)
            .map_err(SaveError::Write)?;
        if current != *expected {
            return Err(SaveError::ExternalChange);
        }
        Ok(())
    }

    fn prepare_save_plan(&mut self, force: bool) -> std::result::Result<SavePlan, SaveError> {
        let baseline = if force {
            self.save_io
                .read(&self.bib_path)
                .map_err(SaveError::Write)?
        } else {
            self.verify_disk_contents(&self.saved_contents)?;
            self.saved_contents.clone()
        };
        let overrides_external = baseline != self.saved_contents;
        let before = SaveState::capture(self);
        self.apply_save_actions();
        if self.config.save.save_action_regenerate_citekeys
            && self.regen_all_citekeys_impl(true) > 0
        {
            self.sorted_keys = sort_entries(&self.database.entries, &self.config);
        }
        let file_dir = effective_file_dir(
            &self.bib_path,
            self.database.jabref_meta.file_directory.as_deref(),
        );
        let mut renames = Vec::new();
        let mut skipped = Vec::new();
        let mut undo = Vec::new();
        if self.config.save.sync_filenames {
            for (key, entry) in &mut self.database.entries {
                let Some(old_value) = entry.fields.get("file").cloned() else {
                    continue;
                };
                let mut files = parse_file_field(&old_value);
                let (plans, entry_skipped) =
                    plan_filename_renames(&entry.citation_key, &files, &file_dir);
                skipped.extend(entry_skipped);
                if plans.is_empty() {
                    continue;
                }
                let undo_renames = plans
                    .iter()
                    .map(|p| (p.new_abs.clone(), p.old_abs.clone()))
                    .collect();
                for plan in &plans {
                    files[plan.index].path = plan.new_rel_path.clone();
                }
                entry
                    .fields
                    .insert("file".into(), serialize_file_field(&files));
                entry.dirty = true;
                undo.push(UndoItem::FilenamesSynced {
                    entry_key: key.clone(),
                    old_file_value: old_value,
                    renames: undo_renames,
                });
                renames.extend(plans);
            }
        }
        if !undo.is_empty() {
            self.push_undo(UndoItem::Batch(undo));
        }
        self.sync_dirty_entries();
        self.sort_entries_for_save();
        crate::bib::writer::normalize_separators(&mut self.database.raw_file);
        let output = write_bib_file(&self.database.raw_file);
        // Two database copies per save: the pre-save snapshot (restored into
        // the app) and `original` for detecting changes during the preview.
        let original = before.database.clone();
        let staged = before.swap_into(self);
        let plan = SavePlan {
            original,
            path: self.bib_path.clone(),
            staged,
            output,
            renames,
            skipped,
            baseline,
            overrides_external,
        };
        self.validate_save_renames(&plan)?;
        Ok(plan)
    }

    fn validate_save_renames(&self, plan: &SavePlan) -> std::result::Result<(), SaveError> {
        use std::collections::{HashMap, HashSet};
        if plan.renames.is_empty() {
            return Ok(());
        }
        let file_dir = effective_file_dir(
            &self.bib_path,
            self.database.jabref_meta.file_directory.as_deref(),
        );
        let mut owners: HashMap<PathBuf, usize> = HashMap::new();
        for entry in self.database.entries.values() {
            if let Some(value) = entry.fields.get("file") {
                for file in parse_file_field(value) {
                    let path = crate::util::open::resolve_file_path(&file.path, &file_dir);
                    let path = path.canonicalize().unwrap_or(path);
                    *owners.entry(path).or_default() += 1;
                }
            }
        }
        let mut targets = HashSet::new();
        for rename in &plan.renames {
            let source = rename.old_abs.canonicalize().map_err(|error| {
                SaveError::Plan(format!(
                    "attachment {}: {}",
                    rename.old_abs.display(),
                    error
                ))
            })?;
            if owners.get(&source).copied().unwrap_or(0) > 1 {
                return Err(SaveError::Plan(format!(
                    "shared attachment {} cannot be renamed to multiple citation keys",
                    source.display()
                )));
            }
            // An existing target is acceptable only when it is the source
            // itself (a case-only rename on a case-insensitive filesystem).
            let occupied = std::fs::symlink_metadata(&rename.new_abs).is_ok()
                && !crate::util::persistence::same_file(&rename.old_abs, &rename.new_abs);
            if !targets.insert(rename.new_abs.clone()) || occupied {
                return Err(SaveError::Plan(format!(
                    "attachment target {} already exists or is used twice",
                    rename.new_abs.display()
                )));
            }
        }
        Ok(())
    }

    fn execute_save_plan(&mut self, plan: SavePlan) -> std::result::Result<(), SaveError> {
        if self.database != plan.original || self.bib_path != plan.path {
            return Err(SaveError::Plan(
                "library changed during the preview; save again to review a new plan".into(),
            ));
        }
        self.verify_disk_contents(&plan.baseline)?;
        self.validate_save_renames(&plan)?;
        // Output is fully serialized before either backup or attachment mutation.
        // An overwritten external version is always backed up.
        if (self.config.general.backup_on_save || plan.overrides_external)
            && plan.baseline.is_some()
        {
            self.save_io
                .backup(&self.bib_path, &self.bib_path.with_extension("bib.bak"))
                .map_err(SaveError::Backup)?;
        }
        self.verify_disk_contents(&plan.baseline)?;
        let mut completed = Vec::new();
        let result = (|| {
            for rename in &plan.renames {
                self.save_io
                    .rename_attachment(&rename.old_abs, &rename.new_abs)
                    .map_err(SaveError::Write)?;
                completed.push(rename);
            }
            self.verify_disk_contents(&plan.baseline)?;
            self.save_io
                .persist(&self.bib_path, plan.output.as_bytes())
                .map_err(SaveError::Write)
        })();
        if let Err(error) = result {
            let mut recovery_errors = Vec::new();
            for rename in completed.into_iter().rev() {
                if let Err(recovery) = self
                    .save_io
                    .rename_attachment(&rename.new_abs, &rename.old_abs)
                {
                    self.retain_unreversed_rename(rename);
                    recovery_errors.push(format!(
                        "{} remains at {}: {}",
                        rename.old_abs.display(),
                        rename.new_abs.display(),
                        recovery
                    ));
                }
            }
            return Err(if recovery_errors.is_empty() {
                error
            } else {
                SaveError::Plan(format!(
                    "{}; attachment recovery failed: {}",
                    error,
                    recovery_errors.join("; ")
                ))
            });
        }
        plan.staged.restore(self);
        self.refresh_view();
        self.saved_contents = Some(plan.output.into_bytes());
        self.save_generation = Some(self.undo_stack.len());
        self.dirty = false;
        for entry in self.database.entries.values_mut() {
            entry.dirty = false;
        }
        if let (Some(key), Some(detail)) = (&self.detail_entry_key, &mut self.detail_state) {
            if let Some(entry) = self.database.entries.get(key) {
                detail.refresh(entry);
            }
        }
        Ok(())
    }

    fn retain_unreversed_rename(&mut self, rename: &PlannedRename) {
        let file_dir = effective_file_dir(
            &self.bib_path,
            self.database.jabref_meta.file_directory.as_deref(),
        );
        let mut undo = Vec::new();
        for (key, entry) in &mut self.database.entries {
            let Some(old_value) = entry.fields.get("file").cloned() else {
                continue;
            };
            let mut files = parse_file_field(&old_value);
            let mut changed = false;
            for file in &mut files {
                if crate::util::open::resolve_file_path(&file.path, &file_dir) == rename.old_abs {
                    file.path = rename.new_rel_path.clone();
                    changed = true;
                }
            }
            if changed {
                entry
                    .fields
                    .insert("file".into(), serialize_file_field(&files));
                entry.dirty = true;
                undo.push(UndoItem::FilenamesSynced {
                    entry_key: key.clone(),
                    old_file_value: old_value,
                    renames: vec![(rename.new_abs.clone(), rename.old_abs.clone())],
                });
            }
        }
        if !undo.is_empty() {
            self.push_undo(UndoItem::Batch(undo));
        }
        self.save_generation = None;
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
                        RawItem::Entry(re) => Some(re),
                        _ => None,
                    };
                    let serialized = serialize_entry(
                        entry,
                        self.config.save.align_fields,
                        sort_fields,
                        original,
                    );
                    let merged_fields = merged_raw_fields(entry, original);
                    self.database.raw_file.items[entry.raw_index] = RawItem::Entry(RawEntry {
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
            let mut to_remove = std::mem::take(&mut self.deleted_raw_indices);
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
                // normalize_separators will collapse any excess, ensuring exactly
                // one blank line on each side of the new entry.
                self.database
                    .raw_file
                    .items
                    .insert(insert_pos, RawItem::Preamble("\n".to_string()));
                self.database.raw_file.items.insert(
                    insert_pos + 1,
                    RawItem::Entry(RawEntry {
                        entry_type: entry.entry_type.display_name().to_string(),
                        citation_key: entry.citation_key.clone(),
                        fields: merged_raw_fields(entry, None),
                        raw_text: serialized,
                    }),
                );
                self.database
                    .raw_file
                    .items
                    .insert(insert_pos + 2, RawItem::Preamble("\n".to_string()));
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
            .filter_map(|(i, item)| {
                if matches!(item, RawItem::Entry(_)) {
                    Some(i)
                } else {
                    None
                }
            })
            .collect();

        if entry_indices.len() < 2 {
            return;
        }

        // Extract those items, sort them by citation key (case-insensitive).
        let mut sorted: Vec<RawItem> = entry_indices
            .iter()
            .map(|&i| self.database.raw_file.items[i].clone())
            .collect();
        sorted.sort_by_cached_key(|item| match item {
            RawItem::Entry(entry) => entry.citation_key.to_lowercase(),
            _ => String::new(),
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
    "abstract",
    "addendum",
    "address",
    "annote",
    "booktitle",
    "chapter",
    "edition",
    "institution",
    "journal",
    "keywords",
    "language",
    "note",
    "organization",
    "publisher",
    "school",
    "series",
    "subtitle",
    "title",
    "titleaddon",
    "type",
    "venue",
];

/// Person-name (name-list) fields.
const NAMES: &[&str] = &[
    "author",
    "editor",
    "editora",
    "editorb",
    "editorc",
    "bookauthor",
    "afterword",
    "translator",
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
pub(super) fn action_label_for_field(
    field: &str,
    cfg: &crate::config::schema::SaveConfig,
) -> &'static str {
    match field {
        "url" if cfg.save_action_cleanup_url => "cleanup_url",
        "date" if cfg.save_action_normalize_date => "normalize_date",
        "month" if cfg.save_action_normalize_month => "normalize_month",
        "pages" if cfg.save_action_normalize_page_numbers => "normalize_pages",
        "isbn" if cfg.save_action_normalize_isbn => "normalize_isbn",
        "author" | "editor" | "editora" | "editorb" | "editorc" | "bookauthor" | "translator"
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
    let cached_key = |key: &String| sort_value_key(field, &get_sort_value(&entries[key], field));
    if ascending {
        keys.sort_by_cached_key(cached_key);
    } else {
        keys.sort_by_cached_key(|key| std::cmp::Reverse(cached_key(key)));
    }

    keys
}

/// Fields that are compared numerically rather than lexically.
const NUMERIC_SORT_FIELDS: &[&str] = &["year", "volume", "number", "pages"];

/// Choose ordering once per field: numeric for numeric fields, lexical for
/// all others. Pair-dependent numeric fallback would violate transitivity.
#[cfg(test)]
pub(super) fn compare_sort_values(field: &str, a: &str, b: &str) -> std::cmp::Ordering {
    sort_value_key(field, a).cmp(&sort_value_key(field, b))
}

fn sort_value_key(field: &str, value: &str) -> (u8, i64, String) {
    if NUMERIC_SORT_FIELDS.contains(&field) {
        numeric_sort_key(field, value)
    } else {
        (0, 0, value.to_string())
    }
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

/// Parse the leading signed run of ASCII digits as an integer (e.g. `"123--130"` →
/// `Some(123)`). Returns `None` when the value does not start with a digit.
fn leading_integer(v: &str) -> Option<i64> {
    let sign = usize::from(v.starts_with(['-', '+']));
    let end = sign
        + v.as_bytes()[sign..]
            .iter()
            .take_while(|b| b.is_ascii_digit())
            .count();
    v[..end].parse::<i64>().ok()
}

pub(super) fn get_sort_value(entry: &Entry, field: &str) -> String {
    match field {
        "citation_key" | "key" | "citekey" => entry.citation_key.clone(),
        "entrytype" | "type" => entry.entry_type.display_name().to_string(),
        _ => entry.fields.get(field).cloned().unwrap_or_default(),
    }
}

/// Replace the final component of a stored attachment path with `name`,
/// keeping everything before it byte-for-byte.
fn replace_file_name(path: &str, name: &str) -> String {
    let is_separator = |c: char| c == '/' || (cfg!(windows) && c == '\\');
    match path.rfind(is_separator) {
        Some(index) => format!("{}{}", &path[..=index], name),
        None => name.to_string(),
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
///
/// Remote links (`https://…`) and attachments missing from disk are never
/// renamed; they are returned as human-readable notes so that one broken link
/// cannot block a save.
fn plan_filename_renames(
    citekey: &str,
    parsed: &[crate::util::open::ParsedFile],
    file_dir: &std::path::Path,
) -> (Vec<PlannedRename>, Vec<String>) {
    let multi = parsed.len() > 1;
    let safe_stem = crate::util::import::sanitize_filename_stem(citekey);
    let mut plans = Vec::new();
    let mut skipped = Vec::new();

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
        if pf.path.contains("://") {
            continue; // Remote link, not a local file.
        }

        // Resolve to absolute paths.
        let old_abs = if old_rel.is_absolute() {
            old_rel.clone()
        } else {
            file_dir.join(&old_rel)
        };
        if !old_abs.exists() {
            skipped.push(format!("{} (file not found)", old_abs.display()));
            continue;
        }
        let new_abs = old_abs
            .parent()
            .map(|p| p.join(&new_filename))
            .unwrap_or_else(|| file_dir.join(&new_filename));

        // New stored path: replace only the file name, preserving relative vs
        // absolute form, the subdirectory, and the original separator style
        // (JabRef files use `/`, which must not become `\` on Windows).
        let new_rel_path = replace_file_name(&pf.path, &new_filename);

        plans.push(PlannedRename {
            index: i,
            old_abs,
            new_abs,
            new_rel_path,
        });
    }

    (plans, skipped)
}
