//! Entry import: DOI/URL/PDF import and background fetch handling.

use super::*;

/// Fields a DOI lookup searches with (title, author, year) or may change
/// (doi, url).
const DOI_LOOKUP_FIELDS: [&str; 5] = ["title", "author", "year", "doi", "url"];

/// The entry a background DOI lookup was started for, as it was then.
///
/// Entries have no identity beyond their citation key, so a result is
/// applied only when the entry under the same key still has exactly these
/// field values. A renamed, deleted or edited entry, or a different entry
/// that later took the key, gets nothing, so a late answer can never
/// overwrite newer data.
#[derive(Debug, Clone)]
pub(super) struct DoiLookup {
    pub(super) entry_key: String,
    fields: Vec<(&'static str, Option<String>)>,
}

impl DoiLookup {
    pub(super) fn of(entry: &Entry) -> Self {
        Self {
            entry_key: entry.citation_key.clone(),
            fields: DOI_LOOKUP_FIELDS
                .iter()
                .map(|&f| (f, entry.fields.get(f).cloned()))
                .collect(),
        }
    }

    /// The first looked-up field whose value has changed since the lookup.
    fn changed_field(&self, entry: &Entry) -> Option<&'static str> {
        self.fields
            .iter()
            .find(|(f, old)| entry.fields.get(*f) != old.as_ref())
            .map(|(f, _)| *f)
    }
}

impl App {
    /// Whether the user is between interactions: in the entry list or an
    /// entry's detail view with no editor, prompt or dialog open. Finished
    /// background work is applied only then, so it never replaces a draft,
    /// a pending confirmation, or the target of an open dialog.
    pub(super) fn is_idle_for_background(&self) -> bool {
        matches!(self.mode, InputMode::Normal | InputMode::Detail)
            && self.field_editor_state.is_none()
            && self.dialog_state.is_none()
            && self.settings_state.is_none()
    }

    /// Collect finished background imports and DOI lookups, and apply them
    /// once [`is_idle_for_background`](Self::is_idle_for_background).
    /// Returns whether anything visible changed.
    pub(super) fn poll_background_tasks(&mut self) -> bool {
        let mut changed = false;
        if let Some(rx) = &self.pending_import {
            match rx.try_recv() {
                Ok(result) => {
                    self.pending_import = None;
                    self.finished_import = Some(result);
                }
                Err(mpsc::TryRecvError::Empty) => {
                    // Still working. The "Fetching…" status was already drawn
                    // when the fetch started, so avoid a full redraw on every
                    // 100 ms poll tick; wait for completion or a real event.
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.pending_import = None;
                    self.status_message =
                        Some("Import failed: fetcher thread disconnected".to_string());
                    changed = true;
                }
            }
        }
        if let Some((_, rx)) = &self.pending_doi_fetch {
            match rx.try_recv() {
                Ok(result) => {
                    if let Some((lookup, _)) = self.pending_doi_fetch.take() {
                        self.finished_doi_fetch = Some((lookup, result));
                    }
                }
                Err(mpsc::TryRecvError::Empty) => {
                    // Still working; see above.
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.pending_doi_fetch = None;
                    self.status_message =
                        Some("DOI lookup failed: thread disconnected".to_string());
                    changed = true;
                }
            }
        }
        if self.poll_duplicate_scan() {
            changed = true;
        }
        // Showing an import can itself open a dialog (a duplicate offer), so
        // check again before applying a DOI result.
        if self.is_idle_for_background() {
            if let Some(result) = self.finished_import.take() {
                self.handle_import_result(result);
                changed = true;
            }
        }
        if self.is_idle_for_background() {
            if let Some((lookup, result)) = self.finished_doi_fetch.take() {
                self.handle_doi_fetch_result(lookup, result);
                changed = true;
            }
        }
        changed
    }

    /// Open a field-editor prompt asking the user to enter a DOI, URL, or local PDF path.
    pub(super) fn start_import_entry(&mut self) {
        self.field_editor_state = Some(
            crate::tui::components::field_editor::FieldEditorState::for_path(
                "DOI, ISBN, URL, or PDF file path",
                "",
            ),
        );
        self.pending_action = Some(PendingAction::ImportUrl);
        self.mode = InputMode::Editing;
    }

    /// Start a background import of `doi_or_url`, storing the receiver for polling.
    pub(super) fn spawn_import(&mut self, doi_or_url: String) {
        if self.pending_import.is_some() || self.finished_import.is_some() {
            self.status_message = Some("Import already in progress — please wait".to_string());
            return;
        }
        let bib_dir = effective_file_dir(
            &self.bib_path,
            self.database.jabref_meta.file_directory.as_deref(),
        );
        let max_pdf_bytes = self
            .config
            .import
            .max_pdf_size_mb
            .saturating_mul(1024 * 1024);
        let (tx, rx) = mpsc::channel();
        self.tasks.import(
            super::tasks::ImportRequest {
                query: doi_or_url,
                pdf_dir: bib_dir,
                max_pdf_bytes,
            },
            tx,
        );
        self.pending_import = Some(rx);
        self.status_message = Some("Fetching…".to_string());
    }

    /// Handle the result of a completed background import fetch.
    pub(super) fn handle_import_result(&mut self, result: crate::util::import::ImportResult) {
        match result {
            Ok(imported) => {
                let entry_type = EntryType::parse(&imported.entry_type);

                // Generate a citation key from the configured template.
                let display_name = entry_type.display_name();
                let type_name = display_name.to_lowercase();
                let template = self.resolve_citekey_template(&type_name, display_name);
                let temp_key = {
                    let mut gen_fields = imported.fields.clone();
                    gen_fields
                        .entry("entrytype".to_string())
                        .or_insert_with(|| display_name.to_string());
                    let key = generate_citekey(&template, &gen_fields);
                    if key.is_empty() {
                        "imported_entry".to_string()
                    } else {
                        key
                    }
                };

                let mut fields = imported.fields;

                // Titlecase the title and wrap in braces to protect case in BibTeX
                if let Some(raw_title) = fields.get("title").cloned() {
                    let titled = crate::util::titlecase::apply_titlecase(
                        &raw_title,
                        &self.config.titlecase.ignore_words,
                        &self.config.titlecase.stop_words,
                    );
                    fields.insert("title".to_string(), format!("{{{}}}", titled));
                }

                // Normalise the imported URL (strip trailing slash). A DOI is
                // an identifier, not a URL — a trailing slash can be part of
                // it, so only trim surrounding whitespace there.
                if let Some(v) = fields.get("url").cloned() {
                    let cleaned = cleanup_url(&v);
                    if cleaned != v {
                        fields.insert("url".to_string(), cleaned);
                    }
                }
                if let Some(v) = fields.get("doi").cloned() {
                    let trimmed = v.trim();
                    if trimmed != v {
                        fields.insert("doi".to_string(), trimmed.to_string());
                    }
                }

                // Set the `file` field using a path relative to the effective file
                // directory (JabRef fileDirectory if set, otherwise the bib parent).
                if let Some(ref pdf_path) = imported.pdf_path {
                    let file_dir = effective_file_dir(
                        &self.bib_path,
                        self.database.jabref_meta.file_directory.as_deref(),
                    );
                    let rel = crate::util::open::make_relative(&file_dir, pdf_path);
                    fields.insert(
                        "file".to_string(),
                        serialize_file_field(&[crate::util::open::ParsedFile {
                            description: String::new(),
                            path: rel.to_string_lossy().into_owned(),
                            file_type: "PDF".into(),
                        }]),
                    );
                }

                let entry = Entry {
                    entry_type,
                    citation_key: temp_key.clone(),
                    fields,
                    group_memberships: Vec::new(),
                    raw_index: usize::MAX,
                    dirty: true,
                };

                // If the key already exists, make it unique
                let key = if self.database.entries.contains_key(&temp_key) {
                    let mut n = 2;
                    loop {
                        let k = format!("{}_{}", temp_key, n);
                        if !self.database.entries.contains_key(&k) {
                            break k;
                        }
                        n += 1;
                    }
                } else {
                    temp_key
                };

                let mut entry = entry;
                entry.citation_key = key.clone();

                self.database.entries.insert(key.clone(), entry);
                self.push_undo(UndoItem::EntryAdded {
                    entry_key: key.clone(),
                });
                self.refresh_view();
                self.dirty = true;

                // Open detail view
                self.detail_entry_key = Some(key.clone());
                if let Some(e) = self.database.entries.get(&key) {
                    self.detail_state =
                        Some(EntryDetailState::new(e, self.config.field_groups.clone()));
                }
                self.mode = InputMode::Detail;
                self.status_message = if let Some(ref pdf_err) = imported.pdf_error {
                    Some(format!(
                        "Imported entry (PDF download failed: {}) — press 'c' to regenerate citation key",
                        pdf_err
                    ))
                } else if imported.pdf_path.is_some() {
                    Some(
                        "Imported entry with PDF — press 'c' to regenerate citation key"
                            .to_string(),
                    )
                } else {
                    Some("Imported entry — press 'c' to regenerate citation key".to_string())
                };
                self.offer_duplicate_merge(&key);
            }
            Err(e) => {
                self.dialog_state = Some(DialogState::message(
                    "Import Error",
                    &format!("Import failed: {}", e),
                ));
                self.pending_action = Some(PendingAction::DismissMessage);
                self.mode = InputMode::Dialog;
            }
        }
    }

    /// Start a background metadata→DOI lookup for the current entry (detail view or list selection).
    pub(super) fn start_fetch_doi(&mut self) {
        let entry_key = match self.action_entry_key() {
            Some(k) => k,
            None => {
                self.status_message = Some("No entry selected".to_string());
                return;
            }
        };

        if self.pending_doi_fetch.is_some() || self.finished_doi_fetch.is_some() {
            self.status_message = Some("DOI lookup already in progress".to_string());
            return;
        }

        let entry = match self.database.entries.get(&entry_key) {
            Some(e) => e,
            None => return,
        };

        let lookup = DoiLookup::of(entry);
        let title = entry.fields.get("title").cloned().unwrap_or_default();
        let author = entry.fields.get("author").cloned().unwrap_or_default();
        let year = entry.fields.get("year").cloned().unwrap_or_default();

        if title.trim().is_empty() && author.trim().is_empty() {
            self.status_message =
                Some("Entry needs at least a title or author to search".to_string());
            return;
        }

        let (tx, rx) = mpsc::channel();
        self.tasks.lookup_doi(
            super::tasks::DoiQuery {
                title,
                author,
                year,
            },
            tx,
        );

        self.pending_doi_fetch = Some((lookup, rx));
        self.status_message = Some("Searching for DOI…".to_string());
    }

    /// Apply the result of a metadata→DOI lookup to the entry it was
    /// started for, provided that entry is unchanged (see [`DoiLookup`]).
    /// A lone strong match is assigned; weak or ambiguous matches are
    /// offered in a picker, and nothing changes unless one is chosen.
    pub(super) fn handle_doi_fetch_result(&mut self, lookup: DoiLookup, result: DoiFetchResult) {
        let found = match result {
            Ok(found) => found,
            Err(e) => {
                self.status_message = Some(format!("DOI lookup failed: {}", e));
                return;
            }
        };
        let shown_doi = match &found {
            crate::util::import::crossref::MetadataMatch::Unique(c) => c.doi.clone(),
            crate::util::import::crossref::MetadataMatch::Choose { candidates, .. } => candidates
                .iter()
                .map(|c| c.doi.as_str())
                .collect::<Vec<_>>()
                .join(", "),
        };
        if let Some(reason) = self.stale_doi_lookup(&lookup) {
            self.status_message = Some(format!(
                "DOI lookup result not applied: {reason} during the lookup (found {shown_doi})"
            ));
            return;
        }
        match found {
            crate::util::import::crossref::MetadataMatch::Unique(c) => {
                self.apply_found_doi(&lookup.entry_key, &c.doi, &c.url);
            }
            crate::util::import::crossref::MetadataMatch::Choose {
                candidates,
                ambiguous,
            } => {
                let mut options: Vec<String> = candidates.iter().map(|c| c.label()).collect();
                options.push("None of these (assign no DOI)".to_string());
                let title = if ambiguous {
                    format!("Several works match '{}' — choose one", lookup.entry_key)
                } else {
                    format!("Possible matches for '{}' — confirm one", lookup.entry_key)
                };
                self.dialog_state = Some(DialogState::type_picker_titled(&title, options));
                self.pending_action = Some(PendingAction::ChooseDoi { lookup, candidates });
                self.mode = InputMode::Dialog;
                self.status_message = Some(if ambiguous {
                    "Crossref has several equally good matches; none was assigned yet".to_string()
                } else {
                    "No certain match on Crossref; confirm one or choose none".to_string()
                });
            }
        }
    }

    /// The user picked row `selected` of a DOI candidate picker.
    pub(super) fn choose_doi_candidate(
        &mut self,
        lookup: DoiLookup,
        candidates: Vec<crate::util::import::crossref::DoiCandidate>,
        selected: usize,
    ) {
        let Some(c) = candidates.get(selected) else {
            self.status_message = Some("No DOI assigned".to_string());
            return;
        };
        if let Some(reason) = self.stale_doi_lookup(&lookup) {
            self.status_message = Some(format!("DOI not assigned: {reason}"));
            return;
        }
        self.apply_found_doi(&lookup.entry_key, &c.doi, &c.url);
    }

    /// Why a lookup's answer no longer belongs to its entry, if it does not.
    fn stale_doi_lookup(&self, lookup: &DoiLookup) -> Option<String> {
        let key = &lookup.entry_key;
        match self.database.entries.get(key) {
            None => Some(format!("'{key}' was renamed or deleted")),
            Some(entry) => lookup
                .changed_field(entry)
                .map(|field| format!("the {field} of '{key}' changed")),
        }
    }

    /// Set `doi` (and `url`, when it says more than the DOI) on `entry_key`
    /// as one undo step.
    fn apply_found_doi(&mut self, entry_key: &str, doi: &str, url: &str) {
        let Some(entry) = self.database.entries.get(entry_key) else {
            return;
        };
        // Only set url if it carries information beyond the DOI itself
        // (Crossref often returns https://doi.org/<doi> as the URL, which is redundant).
        let url_is_distinct = !url.is_empty()
            && crate::util::import::crossref::CrossrefFetcher::extract_doi(url)
                .is_none_or(|extracted| extracted != doi);
        let effective_url = if url_is_distinct { url } else { "" };

        let changes: Vec<(&str, &str, Option<String>)> = [("doi", doi), ("url", effective_url)]
            .into_iter()
            .filter(|(_, new_val)| !new_val.is_empty())
            .filter_map(|(field, new_val)| {
                let old = entry.fields.get(field).cloned();
                (old.as_deref() != Some(new_val)).then_some((field, new_val, old))
            })
            .collect();
        if changes.is_empty() {
            self.status_message = Some(format!("DOI already up-to-date: {}", doi));
            return;
        }

        let mut undo = Vec::with_capacity(changes.len());
        if let Some(entry) = self.database.entries.get_mut(entry_key) {
            for (field, new_val, old_value) in changes {
                entry.fields.insert(field.to_string(), new_val.to_string());
                undo.push(UndoItem::FieldChanged {
                    entry_key: entry_key.to_string(),
                    field_name: field.to_string(),
                    old_value,
                });
            }
            entry.dirty = true;
        }
        self.push_undo(if undo.len() == 1 {
            undo.remove(0)
        } else {
            UndoItem::Batch(undo)
        });

        // Refresh the detail view if this entry is still open
        if self.detail_entry_key.as_deref() == Some(entry_key) {
            if let Some(entry) = self.database.entries.get(entry_key) {
                let entry_clone = entry.clone();
                if let Some(ref mut detail) = self.detail_state {
                    detail.refresh(&entry_clone);
                }
            }
        }
        self.dirty = true;
        self.refresh_view();
        self.status_message = Some(format!("Found DOI: {}", doi));
    }
}
