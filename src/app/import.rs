//! Entry import: DOI/URL/PDF import and background fetch handling.

use super::*;

impl App {
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

    /// Spawn a background thread to fetch the entry, storing the receiver for polling.
    pub(super) fn spawn_import(&mut self, doi_or_url: String) {
        if self.pending_import.is_some() {
            self.status_message =
                Some("Import already in progress — please wait".to_string());
            return;
        }
        let bib_dir = effective_file_dir(
            &self.bib_path,
            self.database.jabref_meta.file_directory.as_deref(),
        );
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut result = crate::util::import::fetch(&doi_or_url);
            // If the fetcher already resolved a local PDF (e.g. PdfFetcher), skip download.
            // Otherwise try each URL candidate in order; stop on first successful download.
            if let Ok(ref mut entry) = result {
                if entry.pdf_path.is_none() && !entry.pdf_urls.is_empty() {
                    let doi = entry.fields.get("doi").cloned().unwrap_or_else(|| "import".to_string());
                    let mut last_err: Option<String> = None;
                    for pdf_url in &entry.pdf_urls.clone() {
                        match crate::util::import::download_pdf(pdf_url, &bib_dir, &doi) {
                            Ok(path) => {
                                entry.pdf_path = Some(path);
                                last_err = None;
                                break;
                            }
                            Err(e) => {
                                last_err = Some(e.to_string());
                            }
                        }
                    }
                    entry.pdf_error = last_err;
                }
            }
            let _ = tx.send(result);
        });
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
                    gen_fields.entry("entrytype".to_string()).or_insert_with(|| display_name.to_string());
                    let key = generate_citekey(&template, &gen_fields);
                    if key.is_empty() { "imported_entry".to_string() } else { key }
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
                        format!(":{}:PDF", rel.to_string_lossy()),
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
                self.push_undo(UndoItem::EntryAdded { entry_key: key.clone() });
                self.sorted_keys = sort_entries(&self.database.entries, &self.config);
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
                    Some("Imported entry with PDF — press 'c' to regenerate citation key".to_string())
                } else {
                    Some("Imported entry — press 'c' to regenerate citation key".to_string())
                };
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

        if self.pending_doi_fetch.is_some() {
            self.status_message = Some("DOI lookup already in progress".to_string());
            return;
        }

        let entry = match self.database.entries.get(&entry_key) {
            Some(e) => e,
            None => return,
        };

        let title  = entry.fields.get("title").cloned().unwrap_or_default();
        let author = entry.fields.get("author").cloned().unwrap_or_default();
        let year   = entry.fields.get("year").cloned().unwrap_or_default();

        if title.trim().is_empty() && author.trim().is_empty() {
            self.status_message =
                Some("Entry needs at least a title or author to search".to_string());
            return;
        }

        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let result = crate::util::import::crossref::search_by_metadata(
                &title, &author, &year,
            );
            let _ = tx.send(result);
        });

        self.pending_doi_fetch = Some((entry_key, rx));
        self.status_message = Some("Searching for DOI…".to_string());
    }

    /// Apply the result of a metadata→DOI lookup to the entry.
    pub(super) fn handle_doi_fetch_result(
        &mut self,
        entry_key: String,
        result: Result<(String, String), String>,
    ) {
        match result {
            Ok((doi, url)) => {
                let mut changed = false;

                // Only set url if it carries information beyond the DOI itself
                // (Crossref often returns https://doi.org/<doi> as the URL, which is redundant).
                let url_is_distinct = !url.is_empty()
                    && crate::util::import::crossref::CrossrefFetcher::extract_doi(&url)
                        .is_none_or(|extracted| extracted != doi);
                let effective_url = if url_is_distinct { url.as_str() } else { "" };

                // Update doi and url fields, recording undo for each changed field.
                for (field, new_val) in [("doi", doi.as_str()), ("url", effective_url)] {
                    if new_val.is_empty() {
                        continue;
                    }
                    let old = self.database.entries.get(&entry_key)
                        .and_then(|e| e.fields.get(field).cloned());
                    if old.as_deref() == Some(new_val) {
                        continue; // No change
                    }
                    self.push_undo(UndoItem::FieldChanged {
                        entry_key: entry_key.clone(),
                        field_name: field.to_string(),
                        old_value: old,
                    });
                    if let Some(entry) = self.database.entries.get_mut(&entry_key) {
                        entry.fields.insert(field.to_string(), new_val.to_string());
                        entry.dirty = true;
                    }
                    changed = true;
                }

                if changed {
                    // Refresh the detail view if this entry is still open
                    if self.detail_entry_key.as_deref() == Some(entry_key.as_str()) {
                        if let Some(entry) = self.database.entries.get(&entry_key) {
                            let entry_clone = entry.clone();
                            if let Some(ref mut detail) = self.detail_state {
                                detail.refresh(&entry_clone);
                            }
                        }
                    }
                    self.dirty = true;
                    self.status_message = Some(format!("Found DOI: {}", doi));
                } else {
                    self.status_message =
                        Some(format!("DOI already up-to-date: {}", doi));
                }
            }
            Err(e) => {
                self.status_message = Some(format!("DOI lookup failed: {}", e));
            }
        }
    }
}
