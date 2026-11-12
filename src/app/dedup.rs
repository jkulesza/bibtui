//! Duplicate detection and merging: the review list (`U` / `:dups`), the
//! field-by-field merge view, "not a duplicate" marks stored in the library,
//! and the duplicate check after importing or adding an entry.

use super::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::bib::dedup::{
    find_duplicates_cancellable, find_matches_for, DedupSettings, DuplicateCluster,
};
use crate::bib::jabref::{escape_meta, split_meta_list};
use crate::config::library::MetaChange;
use crate::tui::components::dedup::{
    union_files, union_groups, DedupMergeState, DedupReviewState, ReviewCluster, TYPE_FIELD,
};
use crate::util::latex::render_latex;
use crate::util::titlecase::strip_case_braces;

/// Metadata key of the "not duplicates" marks:
/// `@Comment{jabref-meta: bibtuiNotDuplicates:\nkeyA,keyB;\nkeyC,keyD,keyE;\n}`.
/// Each `;`-separated item is a set of keys never to be reported together,
/// one per line like JabRef's own `grouping` block.
/// It is not a `bibtui.*` key because it is library data, not a setting.
pub(super) const NOT_DUPLICATES_META: &str = "bibtuiNotDuplicates";

/// The metadata comment holding `sets`, or `None` (remove it) when empty.
fn not_duplicates_comment(sets: &[Vec<String>]) -> Option<String> {
    if sets.is_empty() {
        return None;
    }
    let body: String = sets
        .iter()
        .map(|s| format!("{};\n", escape_meta(&s.join(","))))
        .collect();
    Some(format!(
        "@Comment{{jabref-meta: {NOT_DUPLICATES_META}:\n{body}}}"
    ))
}

/// What a background duplicate scan is for.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum ScanPurpose {
    /// The status-bar count when a file is opened (`duplicates.check_on_load`).
    OnLoad,
    /// The review list (`U` / `:dups`).
    Review,
}

/// A duplicate scan running on a worker thread over a snapshot of the
/// library. Its result is used only if the library, marks and settings still
/// equal the snapshot. Dropping it (replacing or cancelling the scan) tells
/// the worker to stop.
pub(super) struct DuplicateScan {
    purpose: ScanPurpose,
    entries: Arc<IndexMap<String, Entry>>,
    ignored: Vec<Vec<String>>,
    settings: DedupSettings,
    cancel: Arc<AtomicBool>,
    /// `None` when the worker was cancelled.
    rx: mpsc::Receiver<Option<Vec<DuplicateCluster>>>,
}

impl Drop for DuplicateScan {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

impl DuplicateScan {
    /// Whether the library has changed since the scan's snapshot.
    fn is_stale(&self, app: &App) -> bool {
        *self.entries != app.database.entries
            || self.ignored != app.not_duplicate_sets()
            || self.settings != app.config.duplicates.settings()
    }
}

impl App {
    // ── "Not duplicates" marks ──────────────────────────────────────────────

    /// Key sets the user marked as not duplicates.
    pub(super) fn not_duplicate_sets(&self) -> Vec<Vec<String>> {
        let Some(raw) = self
            .database
            .jabref_meta
            .unknown_meta
            .get(NOT_DUPLICATES_META)
        else {
            return Vec::new();
        };
        split_meta_list(raw)
            .into_iter()
            .map(|item| {
                item.split(',')
                    .map(|k| k.trim().to_string())
                    .filter(|k| !k.is_empty())
                    .collect::<Vec<_>>()
            })
            .filter(|set| set.len() >= 2)
            .collect()
    }

    /// Record `keys` as not duplicates of each other (undoable; saved with
    /// the file).
    pub(super) fn mark_not_duplicates(&mut self, keys: &[String]) {
        let mut set = keys.to_vec();
        set.sort();
        set.dedup();
        let mut sets = self.not_duplicate_sets();
        if set.len() < 2 || sets.contains(&set) {
            return;
        }
        sets.push(set);
        self.apply_library_changes(vec![MetaChange {
            key: NOT_DUPLICATES_META.to_string(),
            comment: not_duplicates_comment(&sets),
            summary: "not-duplicate marks".to_string(),
        }]);
    }

    /// Rewrite the "not duplicates" marks after citation keys change, using
    /// the same simultaneous `old → new` mapping as crossrefs (so A→B with
    /// B→C moves a mark on A to B, not C). A mark whose keys collapse into
    /// one entry, as when marked entries are merged, is dropped. Marks on
    /// deleted entries are kept, so undoing the deletion restores them.
    /// Returns the undo record, or `None` when no mark changed.
    pub(super) fn retarget_not_duplicates(
        &mut self,
        mapping: &std::collections::HashMap<String, String>,
    ) -> Option<UndoItem> {
        if mapping.is_empty() {
            return None;
        }
        let sets = self.not_duplicate_sets();
        let mut retargeted: Vec<Vec<String>> = Vec::with_capacity(sets.len());
        for set in &sets {
            let mut new_set: Vec<String> = set
                .iter()
                .map(|k| mapping.get(k).unwrap_or(k).clone())
                .collect();
            new_set.sort();
            new_set.dedup();
            if new_set.len() >= 2 && !retargeted.contains(&new_set) {
                retargeted.push(new_set);
            }
        }
        if retargeted == sets {
            return None;
        }
        let previous = self.meta_comment_text(NOT_DUPLICATES_META);
        self.set_meta_comment(NOT_DUPLICATES_META, not_duplicates_comment(&retargeted));
        self.database.jabref_meta = crate::bib::jabref::parse_meta(&self.database.raw_file);
        Some(UndoItem::LibraryMetaChanged {
            previous: vec![(NOT_DUPLICATES_META.to_string(), previous)],
        })
    }

    // ── Detection ───────────────────────────────────────────────────────────

    /// Scan for duplicates on a worker thread, replacing (and so
    /// cancelling) any scan already running.
    fn start_duplicate_scan(&mut self, purpose: ScanPurpose) {
        let entries = Arc::new(self.database.entries.clone());
        let ignored = self.not_duplicate_sets();
        let settings = self.config.duplicates.settings();
        let cancel = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel();
        {
            let (entries, ignored, cancel) = (entries.clone(), ignored.clone(), cancel.clone());
            std::thread::spawn(move || {
                let clusters = find_duplicates_cancellable(&entries, &settings, &ignored, &cancel);
                // The receiver is gone if the scan was cancelled or replaced.
                let _ = tx.send(clusters);
            });
        }
        self.duplicate_scan = Some(DuplicateScan {
            purpose,
            entries,
            ignored,
            settings,
            cancel,
            rx,
        });
    }

    /// Collect a finished duplicate scan. A review list opens only once
    /// [`is_idle_for_background`](Self::is_idle_for_background). Returns
    /// whether anything visible changed.
    pub(super) fn poll_duplicate_scan(&mut self) -> bool {
        let Some(scan) = &self.duplicate_scan else {
            return false;
        };
        if scan.purpose == ScanPurpose::Review && !self.is_idle_for_background() {
            return false;
        }
        let result = match scan.rx.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return false,
            Err(mpsc::TryRecvError::Disconnected) => None,
        };
        match self.duplicate_scan.take() {
            Some(scan) => self.finish_duplicate_scan(scan, result),
            None => false,
        }
    }

    /// Apply a scan's `result` (`None`: the worker failed), unless the
    /// library changed meanwhile: a stale load check is dropped, a stale
    /// review search starts again on the current library.
    fn finish_duplicate_scan(
        &mut self,
        scan: DuplicateScan,
        result: Option<Vec<DuplicateCluster>>,
    ) -> bool {
        let Some(clusters) = result else {
            if scan.purpose == ScanPurpose::Review {
                self.status_message = Some("Duplicate search failed".to_string());
                return true;
            }
            return false;
        };
        let stale = scan.is_stale(self);
        match (scan.purpose, stale) {
            (ScanPurpose::OnLoad, true) => false,
            (ScanPurpose::OnLoad, false) => {
                self.report_duplicate_count(clusters.len());
                true
            }
            (ScanPurpose::Review, true) => {
                self.start_duplicate_scan(ScanPurpose::Review);
                false
            }
            (ScanPurpose::Review, false) => {
                self.show_duplicate_review(clusters);
                true
            }
        }
    }

    /// The clusters a scan of the current library would find.
    #[cfg(test)]
    pub(super) fn duplicate_clusters(&self) -> Vec<DuplicateCluster> {
        crate::bib::dedup::find_duplicates(
            &self.database.entries,
            &self.config.duplicates.settings(),
            &self.not_duplicate_sets(),
        )
    }

    /// Wait for the running duplicate scan and apply it, as the event loop
    /// would once idle.
    #[cfg(test)]
    pub(super) fn wait_for_duplicate_scan(&mut self) {
        while let Some(scan) = self.duplicate_scan.take() {
            let result = scan.rx.recv().ok().flatten();
            self.finish_duplicate_scan(scan, result);
        }
    }

    /// Start the status-bar count for a freshly opened file
    /// (`duplicates.check_on_load`) in the background, so a large library
    /// opens without waiting for it.
    pub(super) fn report_duplicates_on_load(&mut self) {
        if !self.config.duplicates.check_on_load || self.database.entries.len() < 2 {
            return;
        }
        self.start_duplicate_scan(ScanPurpose::OnLoad);
    }

    fn report_duplicate_count(&mut self, count: usize) {
        if count == 0 {
            return;
        }
        let note = format!(
            "{} possible duplicate cluster{} — press U to review",
            count,
            if count == 1 { "" } else { "s" }
        );
        self.status_message = Some(match self.status_message.take() {
            Some(msg) => format!("{} | {}", msg, note),
            None => note,
        });
    }

    /// "Title (year) [Type]" for one entry, for the review list.
    fn entry_summary(&self, key: &str) -> String {
        let Some(entry) = self.database.entries.get(key) else {
            return String::new();
        };
        let title = entry
            .fields
            .get("title")
            .map(|t| strip_case_braces(&render_latex(t)))
            .unwrap_or_default();
        let year = entry
            .fields
            .get("year")
            .or_else(|| entry.fields.get("date"))
            .map(|y| format!(" ({})", y.trim()))
            .unwrap_or_default();
        format!("{}{} [{}]", title, year, entry.entry_type.display_name())
    }

    fn review_cluster(&self, cluster: DuplicateCluster) -> ReviewCluster {
        let mut reasons: Vec<String> = Vec::new();
        for pair in &cluster.pairs {
            let reason = pair.reason.describe();
            if !reasons.contains(&reason) {
                reasons.push(reason);
            }
        }
        let related = cluster
            .related
            .iter()
            .map(|r| {
                // Name the outside entry first, then the member it matched.
                let (member, other) = if cluster.keys.contains(&r.pair.a) {
                    (&r.pair.a, &r.pair.b)
                } else {
                    (&r.pair.b, &r.pair.a)
                };
                format!(
                    "{other} ~ {member} ({}) — kept apart: {}",
                    r.pair.reason.describe(),
                    r.separation.describe()
                )
            })
            .collect();
        ReviewCluster {
            summaries: cluster.keys.iter().map(|k| self.entry_summary(k)).collect(),
            keys: cluster.keys,
            reasons,
            related,
        }
    }

    // ── Review list ─────────────────────────────────────────────────────────

    /// `U` / `:dups`: search for duplicates in the background; the review
    /// list opens when the search finishes (Esc cancels it).
    pub(super) fn open_duplicate_review(&mut self) {
        if self
            .duplicate_scan
            .as_ref()
            .is_some_and(|s| s.purpose == ScanPurpose::Review)
        {
            self.status_message = Some("Still looking for duplicates… (Esc cancels)".to_string());
            return;
        }
        self.start_duplicate_scan(ScanPurpose::Review);
        self.status_message = Some("Looking for duplicates… (Esc cancels)".to_string());
    }

    /// Cancel a running `U` search. Returns whether there was one.
    pub(super) fn cancel_duplicate_review(&mut self) -> bool {
        if !self
            .duplicate_scan
            .as_ref()
            .is_some_and(|s| s.purpose == ScanPurpose::Review)
        {
            return false;
        }
        self.duplicate_scan = None;
        self.status_message = Some("Duplicate search cancelled".to_string());
        true
    }

    fn show_duplicate_review(&mut self, clusters: Vec<DuplicateCluster>) {
        let clusters: Vec<ReviewCluster> = clusters
            .into_iter()
            .map(|c| self.review_cluster(c))
            .collect();
        let count = clusters.len();
        self.dedup_review_state = Some(DedupReviewState::new(clusters));
        self.mode = InputMode::DedupReview;
        self.status_message = Some(if count == 0 {
            "No possible duplicates found".to_string()
        } else {
            format!(
                "{} possible duplicate cluster{}",
                count,
                if count == 1 { "" } else { "s" }
            )
        });
    }

    /// Esc in the review list or merge view.
    pub(super) fn close_dedup(&mut self) {
        if let Some(merge) = self.dedup_merge_state.take() {
            self.mode = if merge.from_review && self.dedup_review_state.is_some() {
                InputMode::DedupReview
            } else {
                InputMode::Normal
            };
            return;
        }
        self.dedup_review_state = None;
        self.mode = InputMode::Normal;
    }

    /// Navigation keys inside the duplicate popups. Returns true if handled.
    pub(super) fn dedup_navigation(&mut self, action: &Action) -> bool {
        match self.mode {
            InputMode::DedupMerge => {
                let Some(state) = self.dedup_merge_state.as_mut() else {
                    return false;
                };
                match action {
                    Action::MoveDown => state.move_conflict(1),
                    Action::MoveUp => state.move_conflict(-1),
                    Action::PageDown => state.move_conflict(10),
                    Action::PageUp => state.move_conflict(-10),
                    Action::MoveToTop => state.move_to_first_conflict(),
                    Action::MoveToBottom => state.move_to_last_conflict(),
                    _ => return false,
                }
                true
            }
            InputMode::DedupReview => {
                let Some(state) = self.dedup_review_state.as_mut() else {
                    return false;
                };
                match action {
                    Action::MoveDown => state.move_down(),
                    Action::MoveUp => state.move_up(),
                    Action::PageDown => state.page_down(),
                    Action::PageUp => state.page_up(),
                    Action::MoveToTop => state.cursor = 0,
                    Action::MoveToBottom => state.move_to_bottom(),
                    _ => return false,
                }
                true
            }
            _ => false,
        }
    }

    /// `x` in the review list: never report the focused cluster again.
    pub(super) fn ignore_duplicate_cluster(&mut self) {
        let Some(keys) = self
            .dedup_review_state
            .as_ref()
            .and_then(|s| s.current())
            .map(|c| c.keys.clone())
        else {
            return;
        };
        self.mark_not_duplicates(&keys);
        if let Some(state) = self.dedup_review_state.as_mut() {
            state.remove_current();
        }
        self.mode = InputMode::DedupReview;
        self.status_message = Some(format!(
            "Marked {} entries as not duplicates (save to keep)",
            keys.len()
        ));
    }

    /// Enter in the review list: open the merge view for the focused cluster.
    pub(super) fn reconcile_duplicate_cluster(&mut self) {
        let Some(cluster) = self
            .dedup_review_state
            .as_ref()
            .and_then(|s| s.current())
            .cloned()
        else {
            return;
        };
        self.open_merge(&cluster.keys, cluster.reasons, true);
    }

    pub(super) fn open_merge(&mut self, keys: &[String], reasons: Vec<String>, from_review: bool) {
        let entries: Vec<&Entry> = keys
            .iter()
            .filter_map(|k| self.database.entries.get(k))
            .collect();
        if entries.len() < 2 {
            self.status_message = Some("Nothing to merge: entries no longer exist".to_string());
            return;
        }
        self.dedup_merge_state = Some(DedupMergeState::new(&entries, reasons, from_review));
        self.mode = InputMode::DedupMerge;
    }

    // ── Merge view ──────────────────────────────────────────────────────────

    /// `c` in the merge view: generate the key from the merged fields.
    pub(super) fn regen_merge_key(&mut self) {
        let Some(state) = self.dedup_merge_state.as_ref() else {
            return;
        };
        let mut fields: IndexMap<String, String> = IndexMap::new();
        let mut type_name = None;
        for (name, value) in state.merged_fields() {
            match (name.as_str(), value) {
                (TYPE_FIELD, Some(v)) => type_name = Some(v),
                (_, Some(v)) => {
                    fields.insert(name, v);
                }
                _ => {}
            }
        }
        let entry_type = EntryType::parse(type_name.as_deref().unwrap_or("Misc"));
        let display_name = entry_type.display_name();
        let template = self.resolve_citekey_template(&display_name.to_lowercase(), display_name);
        fields
            .entry("entrytype".to_string())
            .or_insert_with(|| display_name.to_string());
        let base = generate_citekey(&template, &fields);
        // Members' keys are freed by the merge, so any of them may be reused.
        let key = if state.keys.contains(&base) {
            base
        } else {
            self.unique_citekey(&base, &state.keys[state.survivor])
        };
        if let Some(state) = self.dedup_merge_state.as_mut() {
            self.status_message = Some(format!("Merged key will be '{}'", key));
            state.generated_key = Some(key);
        }
    }

    /// Enter in the merge view: merge every member into the survivor and
    /// delete the others, as one undo step.
    pub(super) fn apply_merge(&mut self) {
        let Some(state) = self.dedup_merge_state.take() else {
            return;
        };
        let back_mode = if state.from_review && self.dedup_review_state.is_some() {
            InputMode::DedupReview
        } else {
            InputMode::Normal
        };
        let entries: Vec<Entry> = state
            .keys
            .iter()
            .filter_map(|k| self.database.entries.get(k).cloned())
            .collect();
        if entries.len() != state.keys.len() {
            self.mode = back_mode;
            self.status_message = Some("Merge cancelled: entries changed".to_string());
            return;
        }
        let refs: Vec<&Entry> = entries.iter().collect();
        let survivor_key = state.keys[state.survivor].clone();
        let mut undo: Vec<UndoItem> = Vec::new();

        // 1. Merged fields, type, attachments, and groups onto the survivor.
        let files = union_files(&refs, state.survivor);
        let groups = union_groups(&refs, state.survivor);
        if let Some(entry) = self.database.entries.get_mut(&survivor_key) {
            let before = undo.len();
            for (name, value) in state.merged_fields() {
                let Some(value) = value else { continue };
                if name == TYPE_FIELD {
                    let new_type = EntryType::parse(&value);
                    if new_type != entry.entry_type {
                        undo.push(UndoItem::EntryTypeChanged {
                            entry_key: survivor_key.clone(),
                            old_type: std::mem::replace(&mut entry.entry_type, new_type),
                        });
                    }
                    continue;
                }
                let old = entry.fields.get(&name).cloned();
                if old.as_deref() != Some(value.as_str()) {
                    undo.push(UndoItem::FieldChanged {
                        entry_key: survivor_key.clone(),
                        field_name: name.clone(),
                        old_value: old,
                    });
                    entry.fields.insert(name, value);
                }
            }
            if let Some(files) = files {
                undo.push(UndoItem::FieldChanged {
                    entry_key: survivor_key.clone(),
                    field_name: "file".to_string(),
                    old_value: entry.fields.get("file").cloned(),
                });
                entry.fields.insert("file".to_string(), files);
            }
            if groups != entry.group_memberships {
                undo.push(UndoItem::GroupMembershipChanged {
                    entry_key: survivor_key.clone(),
                    old_memberships: entry.group_memberships.clone(),
                    old_groups_field: entry.fields.get("groups").cloned(),
                });
                entry.fields.insert("groups".to_string(), groups.join(","));
                entry.group_memberships = groups;
            }
            if undo.len() > before {
                entry.dirty = true;
            }
        }

        // 2. Delete the other members.
        let mut retired: Vec<String> = Vec::new();
        for (i, key) in state.keys.iter().enumerate() {
            if i == state.survivor {
                continue;
            }
            if let Some(entry) = self.database.entries.shift_remove(key) {
                if entry.raw_index != usize::MAX {
                    self.deleted_raw_indices.push(entry.raw_index);
                }
                undo.push(UndoItem::EntryDeleted { entry });
                retired.push(key.clone());
            }
        }

        // 3. Rename the survivor if another key was chosen.
        let mut final_key = state.final_key().to_string();
        if final_key != survivor_key {
            if self.database.entries.contains_key(&final_key) {
                final_key = self.unique_citekey(&final_key, &survivor_key);
            }
            if final_key != survivor_key {
                if let Some(item) = self.rename_entry_key(&survivor_key, &final_key) {
                    undo.push(item);
                    retired.push(survivor_key.clone());
                }
            }
        }
        retired.retain(|k| *k != final_key);

        // 4. Point crossrefs at the merged entry.
        let mapping: std::collections::HashMap<String, String> = retired
            .iter()
            .map(|k| (k.clone(), final_key.clone()))
            .collect();
        undo.extend(self.update_key_references(&mapping));

        self.push_undo(UndoItem::Batch(undo));
        self.refresh_view();
        self.select_entry_key(&final_key);

        if state.from_review {
            if let Some(review) = self.dedup_review_state.as_mut() {
                review.remove_current();
                if review.clusters.is_empty() {
                    self.dedup_review_state = None;
                }
            }
        }
        self.mode = if state.from_review && self.dedup_review_state.is_some() {
            InputMode::DedupReview
        } else {
            InputMode::Normal
        };

        let merged = state.keys.len();
        self.status_message = Some(format!("Merged {} entries into '{}'", merged, final_key));
        if !retired.is_empty() {
            let report: Vec<String> = retired
                .iter()
                .map(|k| format!("{} → {}", k, final_key))
                .collect();
            self.dialog_state = Some(DialogState::message(
                "Retired citation keys (yy: copy)",
                &report.join("\n"),
            ));
            self.pending_action = Some(PendingAction::DismissMergeReport);
            self.mode = InputMode::Dialog;
        }
    }

    /// Select `key` in the entry list if it is visible.
    pub(super) fn select_entry_key(&mut self, key: &str) {
        let Some(sorted) = self.sorted_keys.iter().position(|k| k == key) else {
            return;
        };
        let position = match &self.filtered_indices {
            Some(indices) => indices.iter().position(|&i| i == sorted),
            None => Some(sorted),
        };
        if let Some(position) = position {
            self.entry_list_state.select(position);
        }
    }

    // ── Check after import / add ────────────────────────────────────────────

    /// If the entry at `key` (just imported or added) looks like a duplicate
    /// of existing entries, ask whether to merge, keep both, or discard it.
    pub(super) fn offer_duplicate_merge(&mut self, key: &str) {
        if !self.config.duplicates.check_on_import {
            return;
        }
        let matches = find_matches_for(
            key,
            &self.database.entries,
            &self.config.duplicates.settings(),
            &self.not_duplicate_sets(),
        );
        if matches.is_empty() {
            return;
        }
        let mut reasons: Vec<String> = Vec::new();
        for (_, reason) in &matches {
            let r = reason.describe();
            if !reasons.contains(&r) {
                reasons.push(r);
            }
        }
        let existing: Vec<String> = matches.into_iter().map(|(k, _)| k).collect();
        let title = format!(
            "Possible duplicate of {} ({})",
            existing.join(", "),
            reasons.join("; ")
        );
        self.dialog_state = Some(DialogState::type_picker_titled(
            &title,
            vec![
                "Merge with existing entry".to_string(),
                "Keep both (not duplicates)".to_string(),
                format!("Discard '{}'", key),
            ],
        ));
        self.pending_action = Some(PendingAction::ImportDuplicate {
            new_key: key.to_string(),
            existing,
            reasons,
        });
        self.mode = InputMode::Dialog;
    }

    /// The user's answer to [`offer_duplicate_merge`] (0 merge, 1 keep, 2 discard).
    pub(super) fn resolve_import_duplicate(
        &mut self,
        choice: usize,
        new_key: String,
        existing: Vec<String>,
        reasons: Vec<String>,
    ) {
        let back = if self.detail_state.is_some() {
            InputMode::Detail
        } else {
            InputMode::Normal
        };
        match choice {
            0 => {
                if self.detail_state.is_some() {
                    self.close_detail();
                }
                let mut keys = existing;
                keys.push(new_key);
                self.open_merge(&keys, reasons, false);
            }
            1 => {
                let mut keys = existing;
                keys.push(new_key);
                self.mark_not_duplicates(&keys);
                self.mode = back;
                self.status_message =
                    Some("Kept both; marked as not duplicates (save to keep)".to_string());
            }
            _ => {
                if self.detail_entry_key.as_deref() == Some(new_key.as_str()) {
                    self.close_detail();
                }
                self.delete_entry(&new_key);
                self.mode = InputMode::Normal;
            }
        }
    }
}
