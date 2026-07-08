//! Name disambiguation engine: cluster similar author names across the
//! library and rewrite variants to a single canonical form.

use super::*;

impl App {
    // ── Name disambiguator ──────────────────────────────────────────────────

    /// Person-name fields that should be scanned for disambiguation.
    pub(super) const NAME_FIELDS: &'static [&'static str] = &[
        "author", "editor", "editora", "editorb", "editorc",
        "bookauthor", "translator",
    ];

    /// Build clusters of similar author names across all entries.
    pub(super) fn build_name_clusters(&mut self) -> Vec<NameCluster> {
        use std::collections::HashMap;

        // 1. Collect all unique person names and their usage counts.
        let mut name_counts: HashMap<String, usize> = HashMap::new();
        for entry in self.database.entries.values() {
            for &field in Self::NAME_FIELDS {
                if let Some(val) = entry.fields.get(field) {
                    for name in val.split(" and ") {
                        let name = name.trim();
                        if !name.is_empty() {
                            *name_counts.entry(name.to_string()).or_insert(0) += 1;
                        }
                    }
                }
            }
        }

        if name_counts.len() < 2 {
            return Vec::new();
        }

        // 2. Normalize each name to a canonical comparison key: lowercase last
        //    name + first-initial, so "Smith, J." and "Smith, John" compare.
        fn normalize_key(name: &str) -> String {
            let name = name.trim();
            let (last, first) = if let Some(comma) = name.find(',') {
                (name[..comma].trim(), name[comma + 1..].trim())
            } else {
                let parts: Vec<&str> = name.split_whitespace().collect();
                if parts.len() >= 2 {
                    (parts[parts.len() - 1], &name[..name.len() - parts[parts.len() - 1].len()])
                } else {
                    (name, "")
                }
            };
            // Strip braces for comparison
            let last_clean: String = last.chars().filter(|c| *c != '{' && *c != '}').collect();
            let first_clean: String = first.trim().chars().filter(|c| *c != '{' && *c != '}').collect();
            let first_initial = first_clean.chars().next().unwrap_or(' ');
            format!("{}_{}", last_clean.to_lowercase(), first_initial.to_lowercase())
        }

        // 3. Group names by normalized key.
        let mut groups: HashMap<String, Vec<String>> = HashMap::new();
        for name in name_counts.keys() {
            let key = normalize_key(name);
            groups.entry(key).or_default().push(name.clone());
        }

        // 4. Also do fuzzy matching within last-name groups to catch typos.
        //    Group by lowercase last name, then use nucleo within each group.
        let mut last_name_groups: HashMap<String, Vec<String>> = HashMap::new();
        for name in name_counts.keys() {
            let name_trimmed = name.trim();
            let last = if let Some(comma) = name_trimmed.find(',') {
                name_trimmed[..comma].trim()
            } else {
                name_trimmed.split_whitespace().next_back().unwrap_or(name_trimmed)
            };
            let last_clean: String = last.chars().filter(|c| *c != '{' && *c != '}').collect();
            last_name_groups.entry(last_clean.to_lowercase()).or_default().push(name.clone());
        }

        // Use nucleo fuzzy matching to find similar names within the same
        // last-name bucket.
        use nucleo_matcher::pattern::{AtomKind, CaseMatching, Normalization, Pattern};
        use nucleo_matcher::{Config as NucleoConfig, Matcher, Utf32Str};

        let mut matcher = Matcher::new(NucleoConfig::DEFAULT);

        // For each last-name bucket with >1 name, check pairwise fuzzy scores
        for bucket_names in last_name_groups.values() {
            if bucket_names.len() < 2 {
                continue;
            }
            for i in 0..bucket_names.len() {
                for j in (i + 1)..bucket_names.len() {
                    let a = &bucket_names[i];
                    let b = &bucket_names[j];
                    let key_a = normalize_key(a);
                    let key_b = normalize_key(b);
                    if key_a == key_b {
                        continue; // already grouped
                    }
                    let pattern = Pattern::new(
                        a,
                        CaseMatching::Ignore,
                        Normalization::Smart,
                        AtomKind::Fuzzy,
                    );
                    let mut buf = Vec::new();
                    let haystack = Utf32Str::new(b, &mut buf);
                    if let Some(score) = pattern.score(haystack, &mut matcher) {
                        if score > 80 {
                            let merged_key = key_a.clone();
                            if let Some(existing) = groups.get_mut(&key_b) {
                                let taken: Vec<String> = std::mem::take(existing);
                                groups.entry(merged_key).or_default().extend(taken);
                            }
                        }
                    }
                }
            }
        }

        // 5. Remove empty groups and groups with only one unique name.
        let mut clusters: Vec<NameCluster> = Vec::new();
        for names in groups.values() {
            let mut unique: Vec<String> = names.clone();
            unique.sort();
            unique.dedup();
            if unique.len() < 2 {
                continue;
            }
            let mut variants: Vec<NameVariant> = unique
                .iter()
                .map(|n| NameVariant {
                    name: n.clone(),
                    count: *name_counts.get(n).unwrap_or(&0),
                })
                .collect();
            variants.sort_by(|a, b| b.count.cmp(&a.count).then(a.name.cmp(&b.name)));
            let canonical_idx = variants
                .iter()
                .enumerate()
                .max_by_key(|(_, v)| v.name.len())
                .map(|(i, _)| i)
                .unwrap_or(0);
            let canonical = variants[canonical_idx].name.clone();
            clusters.push(NameCluster {
                canonical,
                variants,
                selected_variant: canonical_idx,
            });
        }
        clusters.sort_by_key(|a| a.canonical.to_lowercase());
        clusters
    }

    /// Apply name disambiguation: replace all variant names with the selected
    /// canonical form in each cluster.
    pub(super) fn apply_name_disambiguation(&mut self) {
        let state = match self.name_disambig_state.take() {
            Some(s) => s,
            None => return,
        };
        if state.clusters.is_empty() {
            self.mode = InputMode::Normal;
            return;
        }

        let mut replacements: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        for cluster in &state.clusters {
            for variant in &cluster.variants {
                if variant.name != cluster.canonical {
                    replacements.insert(variant.name.clone(), cluster.canonical.clone());
                }
            }
        }

        if replacements.is_empty() {
            self.mode = InputMode::Normal;
            self.status_message = Some("No changes needed".to_string());
            return;
        }

        // Collect all mutations first (entry_key, field, old_val, new_val).
        let mut mutations: Vec<(String, String, String, String)> = Vec::new();
        for (key, entry) in &self.database.entries {
            for &field in Self::NAME_FIELDS {
                let val = match entry.fields.get(field) {
                    Some(v) => v,
                    None => continue,
                };
                let names: Vec<&str> = val.split(" and ").collect();
                let mut changed = false;
                let new_names: Vec<String> = names
                    .iter()
                    .map(|n| {
                        let trimmed = n.trim();
                        if let Some(canonical) = replacements.get(trimmed) {
                            changed = true;
                            canonical.clone()
                        } else {
                            trimmed.to_string()
                        }
                    })
                    .collect();
                if changed {
                    mutations.push((
                        key.clone(),
                        field.to_string(),
                        val.clone(),
                        new_names.join(" and "),
                    ));
                }
            }
        }

        let changed_count = mutations.len();
        for (key, field, old_val, new_val) in mutations {
            if self.undo_stack.len() >= MAX_UNDO {
                self.undo_stack.remove(0);
                self.save_generation = self.save_generation.and_then(|g: usize| g.checked_sub(1));
            }
            self.undo_stack.push(UndoItem::FieldChanged {
                entry_key: key.clone(),
                field_name: field.clone(),
                old_value: Some(old_val),
            });
            if let Some(entry) = self.database.entries.get_mut(&key) {
                entry.fields.insert(field, new_val);
                entry.dirty = true;
            }
            self.dirty = true;
        }

        self.mode = InputMode::Normal;
        self.status_message = Some(format!(
            "Disambiguated: {} field{} updated",
            changed_count,
            if changed_count == 1 { "" } else { "s" },
        ));
    }
}
