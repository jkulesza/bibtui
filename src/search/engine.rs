use nucleo_matcher::pattern::{AtomKind, CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

use crate::bib::model::Entry;

pub struct SearchEngine {
    matcher: Matcher,
}

impl SearchEngine {
    pub fn new() -> Self {
        SearchEngine {
            matcher: Matcher::new(Config::DEFAULT.match_paths()),
        }
    }

    /// Search entries with a query string. Returns indices of matching entries
    /// along with their scores, sorted by score descending.
    ///
    /// Supports field-specific syntax: "author:Kulesza" searches only the author field.
    pub fn search(&mut self, entries: &[&Entry], query: &str) -> Vec<(usize, u32)> {
        if query.is_empty() {
            return entries.iter().enumerate().map(|(i, _)| (i, 0)).collect();
        }

        let (field_filter, search_term) = parse_query(query);

        let pattern = Pattern::new(
            search_term,
            CaseMatching::Ignore,
            Normalization::Smart,
            AtomKind::Fuzzy,
        );

        let mut results: Vec<(usize, u32)> = Vec::new();
        let mut buf = Vec::new();

        for (idx, entry) in entries.iter().enumerate() {
            let haystack = build_search_string(entry, field_filter);
            if haystack.is_empty() {
                continue;
            }

            let haystack_utf32 = Utf32Str::new(&haystack, &mut buf);
            if let Some(score) = pattern.score(haystack_utf32, &mut self.matcher) {
                results.push((idx, score));
            }
        }

        results.sort_by(|a, b| b.1.cmp(&a.1));
        results
    }
}

/// Parse a query for field-specific syntax (e.g., "author:Kulesza").
fn parse_query(query: &str) -> (Option<&str>, &str) {
    if let Some(colon_pos) = query.find(':') {
        let field = &query[..colon_pos];
        // Only treat as field filter if the field name looks valid
        if !field.is_empty()
            && field
                .chars()
                .all(|c| c.is_alphanumeric() || c == '_')
        {
            return (Some(field), &query[colon_pos + 1..]);
        }
    }
    (None, query)
}

/// Build a search string from an entry, optionally filtering to a specific field.
fn build_search_string(entry: &Entry, field_filter: Option<&str>) -> String {
    if let Some(field) = field_filter {
        if field == "entrytype" || field == "type" {
            return entry.entry_type.display_name().to_string();
        }
        if field == "key" || field == "citation_key" || field == "citekey" {
            return entry.citation_key.clone();
        }
        return entry.fields.get(field).cloned().unwrap_or_default();
    }

    // Default: concatenate all searchable fields
    let mut parts = Vec::new();
    parts.push(entry.entry_type.display_name().to_string());
    parts.push(entry.citation_key.clone());
    for value in entry.fields.values() {
        parts.push(value.clone());
    }
    parts.join(" ")
}
