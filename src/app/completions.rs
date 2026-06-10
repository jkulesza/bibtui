//! Tab completion for the field editor, sort prompt, and path editors.

use super::*;

impl App {
    /// Recompute tab-completion candidates for the field editor based on the
    /// current field name and value prefix.  Called on every char/backspace.
    pub(super) fn update_field_completions(&mut self) {
        let editor = match &self.field_editor_state {
            Some(e) if !e.is_path => e,
            _ => return,
        };

        // Month field: completions are always the 12 standard abbreviations,
        // filtered by whatever the user has typed so far.
        if editor.is_month {
            const MONTHS: [&str; 12] = [
                "jan", "feb", "mar", "apr", "may", "jun",
                "jul", "aug", "sep", "oct", "nov", "dec",
            ];
            let prefix = editor.value.to_lowercase();
            let completions: Vec<String> = MONTHS
                .iter()
                .filter(|&&m| m.starts_with(prefix.as_str()))
                .map(|&m| m.to_string())
                .collect();
            let e = self.field_editor_state.as_mut().unwrap();
            e.completions = completions;
            e.completion_idx = 0;
            return;
        }

        let current_key = self.detail_entry_key.clone();

        if editor.editing_name {
            let prefix = editor.field_name.to_lowercase();
            let candidates = field_name_candidates(&self.database);
            let completions: Vec<String> = candidates
                .into_iter()
                .filter(|c| c.to_lowercase().starts_with(&prefix) && !prefix.is_empty())
                .collect();
            let e = self.field_editor_state.as_mut().unwrap();
            e.completions = completions;
            e.completion_idx = 0;
        } else {
            let field = editor.field_name.clone();
            let value_lower = editor.value.to_lowercase();
            let all = field_value_candidates(&self.database, &field, current_key.as_deref());
            let completions: Vec<String> = all
                .into_iter()
                .filter(|c| c.to_lowercase().starts_with(&value_lower))
                .collect();
            let e = self.field_editor_state.as_mut().unwrap();
            e.completions = completions;
            e.completion_idx = 0;
        }
    }

    /// Tab-complete for the field editor (name phase or value phase).
    /// Delegates to path completion when `editor.is_path` is set.
    /// `forward`: true = Tab (next), false = Shift-Tab (previous).
    pub(super) fn do_field_tab_complete_dir(&mut self, forward: bool) {
        if self.field_editor_state.as_ref().map(|e| e.is_path).unwrap_or(false) {
            self.do_path_tab_complete_dir(forward);
            return;
        }

        let editor = match &self.field_editor_state {
            Some(e) => e,
            None => return,
        };
        let completions = editor.completions.clone();
        if completions.is_empty() {
            return;
        }
        let idx = editor.completion_idx;
        let editing_name = editor.editing_name;
        let current = if editing_name {
            editor.field_name.clone()
        } else {
            editor.value.clone()
        };

        // If the active text already equals completions[idx], cycle.
        if current.to_lowercase() == completions[idx].to_lowercase() {
            let next_idx = if forward {
                (idx + 1) % completions.len()
            } else {
                (idx + completions.len() - 1) % completions.len()
            };
            let e = self.field_editor_state.as_mut().unwrap();
            e.completion_idx = next_idx;
            if editing_name {
                e.field_name = completions[next_idx].clone();
                e.name_cursor = e.field_name.len();
            } else {
                e.value = completions[next_idx].clone();
                e.cursor = e.value.len();
            }
            return; // Preserve the completion set for continued cycling.
        }

        // First Tab on a partial: fill common prefix, then first match.
        let start_idx = if forward { 0 } else { completions.len() - 1 };
        match completions.len() {
            1 => {
                let e = self.field_editor_state.as_mut().unwrap();
                if editing_name {
                    e.field_name = completions[0].clone();
                    e.name_cursor = e.field_name.len();
                } else {
                    e.value = completions[0].clone();
                    e.cursor = e.value.len();
                }
                e.completion_idx = 0;
            }
            _ => {
                let common = longest_common_prefix(&completions);
                let e = self.field_editor_state.as_mut().unwrap();
                let active_lower = if editing_name {
                    e.field_name.to_lowercase()
                } else {
                    e.value.to_lowercase()
                };
                if active_lower != common.to_lowercase() {
                    // Advance to the common prefix.
                    if editing_name {
                        e.field_name = common;
                        e.name_cursor = e.field_name.len();
                    } else {
                        e.value = common;
                        e.cursor = e.value.len();
                    }
                    e.completion_idx = 0;
                } else {
                    // Already at common prefix — fill in first/last completion.
                    if editing_name {
                        e.field_name = completions[start_idx].clone();
                        e.name_cursor = e.field_name.len();
                    } else {
                        e.value = completions[start_idx].clone();
                        e.cursor = e.value.len();
                    }
                    e.completion_idx = start_idx;
                }
            }
        }
        // DON'T update completions here — preserve the set for cycling.
    }

    pub(super) fn update_sort_completions(&mut self) {
        let input = self.command_palette_state.input.clone();
        let partial = match input.strip_prefix("sort ") {
            Some(p) => p.to_string(),
            None => {
                self.command_palette_state.completions.clear();
                self.command_palette_state.completion_idx = 0;
                return;
            }
        };
        let candidates = sort_field_candidates(&self.database);
        self.command_palette_state.completions = candidates
            .into_iter()
            .filter(|c| c.starts_with(partial.as_str()))
            .collect();
        self.command_palette_state.completion_idx = 0;
    }

    pub(super) fn do_sort_tab_complete_dir(&mut self, forward: bool) {
        let completions = self.command_palette_state.completions.clone();
        if completions.is_empty() {
            return;
        }
        let input = self.command_palette_state.input.clone();
        let partial = match input.strip_prefix("sort ") {
            Some(p) => p.to_string(),
            None => return,
        };
        let idx = self.command_palette_state.completion_idx;

        // Already filled this completion — cycle.
        if partial == completions[idx].as_str() {
            let next_idx = if forward {
                (idx + 1) % completions.len()
            } else {
                (idx + completions.len() - 1) % completions.len()
            };
            self.command_palette_state.completion_idx = next_idx;
            let new_input = format!("sort {}", completions[next_idx]);
            self.command_palette_state.cursor = new_input.len();
            self.command_palette_state.input = new_input;
            return; // Keep the same completion set for continued cycling.
        }

        // First Tab on a partial: complete to common prefix, then first match.
        let start_idx = if forward { 0 } else { completions.len() - 1 };
        match completions.len() {
            1 => {
                let new_input = format!("sort {}", completions[0]);
                self.command_palette_state.cursor = new_input.len();
                self.command_palette_state.input = new_input;
                self.command_palette_state.completion_idx = 0;
            }
            _ => {
                let common = longest_common_prefix(&completions);
                if partial.as_str() != common.as_str() {
                    // Advance to the longest common prefix without cycling yet.
                    let new_input = format!("sort {}", common);
                    self.command_palette_state.cursor = new_input.len();
                    self.command_palette_state.input = new_input;
                    self.command_palette_state.completion_idx = 0;
                } else {
                    // Already at common prefix — start cycling.
                    let new_input = format!("sort {}", completions[start_idx]);
                    self.command_palette_state.cursor = new_input.len();
                    self.command_palette_state.input = new_input;
                    self.command_palette_state.completion_idx = start_idx;
                }
            }
        }
        // DON'T update completions here — preserve the full set for cycling.
    }

    pub(super) fn do_path_tab_complete_dir(&mut self, forward: bool) {
        // Only active for path-editing pending actions.
        let is_path_edit = matches!(
            self.pending_action,
            Some(PendingAction::ExportSettings)
                | Some(PendingAction::ImportSettings)
                | Some(PendingAction::NewFile)
                | Some(PendingAction::ImportUrl)
                | Some(PendingAction::AddFileAttachment { .. })
                | Some(PendingAction::EditFileAttachment { .. })
                | Some(PendingAction::ExportJson)
                | Some(PendingAction::ExportRis)
        );
        if !is_path_edit {
            return;
        }
        let editor = match self.field_editor_state.as_mut() {
            Some(e) => e,
            None => return,
        };

        // If we already have completions and the current value matches the
        // last-inserted candidate, cycle forward or backward.
        if !self.path_completions.is_empty()
            && self.path_completion_idx < self.path_completions.len()
            && editor.value == self.path_completions[self.path_completion_idx]
        {
            let len = self.path_completions.len();
            self.path_completion_idx = if forward {
                (self.path_completion_idx + 1) % len
            } else {
                (self.path_completion_idx + len - 1) % len
            };
            let next = self.path_completions[self.path_completion_idx].clone();
            editor.value = next;
            editor.cursor = editor.value.len();
            return;
        }

        // Compute fresh completions from the current value.
        let is_add_file = matches!(
            self.pending_action,
            Some(PendingAction::AddFileAttachment { .. })
        );
        let mut completions = path_completions(&editor.value);
        if is_add_file {
            let keys: Vec<String> = self.database.entries.keys().cloned().collect();
            sort_file_completions_for_add(&mut completions, &keys);
        }
        self.path_completions = completions;
        self.path_completion_idx = 0;

        let start_idx = if forward { 0 } else { self.path_completions.len().saturating_sub(1) };
        match self.path_completions.len() {
            0 => {
                self.status_message = Some("No completions".to_string());
            }
            1 => {
                editor.value = self.path_completions[0].clone();
                editor.cursor = editor.value.len();
            }
            _ => {
                // Complete to the longest common prefix.
                let common = longest_common_prefix(&self.path_completions);
                if common != editor.value {
                    // Advance to the common prefix without cycling yet.
                    editor.value = common;
                    editor.cursor = editor.value.len();
                    // Reset completions so the next Tab starts cycling.
                    self.path_completion_idx = 0;
                } else {
                    // Already at the common prefix — start cycling.
                    let pick = self.path_completions[start_idx].clone();
                    editor.value = pick;
                    editor.cursor = editor.value.len();
                    self.path_completion_idx = start_idx;
                }
            }
        }
    }
}

pub(super) fn expand_tilde(s: &str) -> String {
    if s == "~" || s.starts_with("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            let home = home.to_string_lossy();
            return format!("{}{}", home, &s[1..]);
        }
    }
    s.to_string()
}

/// Contract an absolute path back to a `~`-prefixed form when the path falls
/// under the user's home directory.  Returns the input unchanged otherwise.
pub(super) fn contract_tilde(s: &str) -> String {
    if let Some(home) = std::env::var_os("HOME") {
        let home = home.to_string_lossy();
        let home_slash = format!("{}/", home);
        if s == home.as_ref() {
            return "~".to_string();
        }
        if let Some(rest) = s.strip_prefix(home_slash.as_str()) {
            return format!("~/{}", rest);
        }
    }
    s.to_string()
}

/// All BibTeX field names the user might want to type (for new-field name completion).
pub(super) fn field_name_candidates(database: &Database) -> Vec<String> {
    let mut names: std::collections::BTreeSet<String> = [
        "abstract", "address", "annote", "author", "booktitle", "chapter",
        "crossref", "doi", "edition", "editor", "howpublished", "institution",
        "isbn", "issn", "journal", "keywords", "language", "lccn", "month",
        "note", "number", "organization", "pages", "publisher", "school",
        "series", "title", "type", "url", "volume", "year",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    for entry in database.entries.values() {
        for key in entry.fields.keys() {
            names.insert(key.clone());
        }
    }
    names.into_iter().collect()
}

/// Values used for `field` across the database, sorted by frequency then
/// alphabetically.  Skips `current_entry_key` so the entry being edited
/// doesn't seed its own suggestions.  Returns an empty vec for fields where
/// completion would be unhelpful (identifiers, page ranges, etc.).
pub(super) fn field_value_candidates(
    database: &Database,
    field: &str,
    current_entry_key: Option<&str>,
) -> Vec<String> {
    // These fields hold unique identifiers or numeric ranges — skip them.
    if matches!(
        field,
        "doi" | "eprint" | "isbn" | "issn" | "lccn" | "pages" | "url"
            | "volume" | "number"
    ) {
        return Vec::new();
    }
    let mut freq: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for (key, entry) in &database.entries {
        if current_entry_key.map(|k| k == key.as_str()).unwrap_or(false) {
            continue;
        }
        if let Some(v) = entry.fields.get(field) {
            let v = v.trim().to_string();
            if !v.is_empty() {
                *freq.entry(v).or_insert(0) += 1;
            }
        }
    }
    let mut candidates: Vec<String> = freq.keys().cloned().collect();
    candidates.sort_by(|a, b| freq[b].cmp(&freq[a]).then(a.cmp(b)));
    candidates.truncate(50);
    candidates
}

/// All field names that are valid `:sort` targets.
/// Includes the standard virtual fields plus every field key present in the database.
pub(super) fn sort_field_candidates(database: &Database) -> Vec<String> {
    let mut fields: std::collections::BTreeSet<String> = [
        "author", "citation_key", "entrytype", "journal", "title", "year",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    for entry in database.entries.values() {
        for key in entry.fields.keys() {
            fields.insert(key.clone());
        }
    }
    fields.into_iter().collect()
}

pub(super) fn path_completions(prefix: &str) -> Vec<String> {
    use std::path::Path;

    let tilde = prefix.starts_with('~');
    // Work with the expanded form for all filesystem operations.
    let expanded = expand_tilde(prefix);
    let expanded = expanded.as_str();

    let path = Path::new(expanded);
    let (dir, stem) = if expanded.ends_with('/') || expanded.ends_with(std::path::MAIN_SEPARATOR) {
        (path, "")
    } else {
        let stem = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        let parent = path.parent().unwrap_or(Path::new("."));
        let parent = if parent == Path::new("") { Path::new(".") } else { parent };
        (parent, stem)
    };

    let mut matches = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if !name_str.starts_with(stem) {
                continue;
            }
            let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
            let candidate = if dir == std::path::Path::new(".") && !expanded.contains('/') {
                if is_dir {
                    format!("{}/", name_str)
                } else {
                    name_str.to_string()
                }
            } else {
                let base = dir.display().to_string();
                let sep = if base.ends_with('/') { "" } else { "/" };
                if is_dir {
                    format!("{}{}{}/", base, sep, name_str)
                } else {
                    format!("{}{}{}", base, sep, name_str)
                }
            };
            // Re-apply `~` contraction so the editor shows the tilde form.
            let candidate = if tilde { contract_tilde(&candidate) } else { candidate };
            matches.push(candidate);
        }
    }
    matches.sort();
    matches
}

/// Sort file completions for the add-file-attachment context.
///
/// Directories always come first (so the user can navigate into them), then
/// files are sorted by: (1) names that do NOT look like an existing citation
/// key come before names that do, and (2) most recently modified first.
pub(super) fn sort_file_completions_for_add(completions: &mut [String], citation_keys: &[String]) {
    use std::path::Path;

    // Pre-compute modification times.
    let mod_times: std::collections::HashMap<String, std::time::SystemTime> = completions
        .iter()
        .filter_map(|p| {
            let expanded = expand_tilde(p);
            std::fs::metadata(&expanded)
                .and_then(|m| m.modified())
                .ok()
                .map(|t| (p.clone(), t))
        })
        .collect();

    let epoch = std::time::SystemTime::UNIX_EPOCH;

    completions.sort_by(|a, b| {
        let a_is_dir = a.ends_with('/');
        let b_is_dir = b.ends_with('/');

        // Directories first.
        if a_is_dir != b_is_dir {
            return if a_is_dir {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Greater
            };
        }

        // Among files: names that DON'T match a citation key come first.
        let a_stem = Path::new(a.trim_end_matches('/'))
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        let b_stem = Path::new(b.trim_end_matches('/'))
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        let a_matches_key = citation_keys.iter().any(|k| k == a_stem);
        let b_matches_key = citation_keys.iter().any(|k| k == b_stem);
        if a_matches_key != b_matches_key {
            return if a_matches_key {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Less
            };
        }

        // Within same group: most recently modified first.
        let a_time = mod_times.get(a).copied().unwrap_or(epoch);
        let b_time = mod_times.get(b).copied().unwrap_or(epoch);
        b_time.cmp(&a_time)
    });
}

/// Return the longest common byte prefix shared by all strings in `items`.
pub(super) fn longest_common_prefix(items: &[String]) -> String {
    if items.is_empty() {
        return String::new();
    }
    let first = items[0].as_bytes();
    let mut len = first.len();
    for s in &items[1..] {
        let s = s.as_bytes();
        len = len.min(s.len());
        for i in 0..len {
            if first[i] != s[i] {
                len = i;
                break;
            }
        }
    }
    // Walk the first string's chars to find the largest valid UTF-8 boundary <= len.
    let mut boundary = 0;
    for (i, c) in items[0].char_indices() {
        if i >= len {
            break;
        }
        boundary = i + c.len_utf8();
    }
    items[0][..boundary].to_string()
}
