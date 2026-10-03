use nucleo_matcher::pattern::{Atom, AtomKind, CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

use crate::bib::model::Entry;

pub struct SearchEngine {
    matcher: Matcher,
}

impl Default for SearchEngine {
    fn default() -> Self {
        Self::new()
    }
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

        let terms = parse_query(query);
        let patterns: Vec<_> = terms.iter().map(|term| {
            if term.quoted {
                let mut pattern = Pattern::default();
                pattern.atoms.push(Atom::new(&term.text, CaseMatching::Ignore, Normalization::Smart, AtomKind::Substring, false));
                pattern
            } else {
                Pattern::new(&term.text, CaseMatching::Ignore, Normalization::Smart, AtomKind::Fuzzy)
            }
        }).collect();
        let mut results = Vec::new();
        let mut buf = Vec::new();
        for (idx, entry) in entries.iter().enumerate() {
            let mut total = 0u32;
            let matches = terms.iter().zip(&patterns).all(|(term, pattern)| {
                if term.text.is_empty() { return false; }
                let haystack = build_search_string(entry, term.field.as_deref());
                if haystack.is_empty() { return false; }
                match pattern.score(Utf32Str::new(&haystack, &mut buf), &mut self.matcher) {
                    Some(score) => { total = total.saturating_add(score); true }
                    None => false,
                }
            });
            if matches { results.push((idx, total)); }
        }

        results.sort_by_key(|&(_, score)| std::cmp::Reverse(score));
        results
    }
}

#[derive(Debug, PartialEq, Eq)]
struct QueryTerm {
    field: Option<String>,
    text: String,
    quoted: bool,
}

/// Whitespace separates AND terms; double quotes retain a contiguous phrase.
/// An unfinished quote consumes the rest, permitting incremental typing.
/// Unknown qualifiers are custom field names. URLs remain unqualified text.
fn parse_query(query: &str) -> Vec<QueryTerm> {
    let mut tokens = Vec::new();
    let mut token = String::new();
    let mut in_quote = false;
    let mut quoted = false;
    let mut chars = query.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\"' => { in_quote = !in_quote; quoted = true; }
            '\\' if in_quote && matches!(chars.peek(), Some('\"' | '\\')) => {
                if let Some(escaped) = chars.next() { token.push(escaped); }
            }
            c if c.is_whitespace() && !in_quote => {
                if !token.is_empty() { tokens.push((std::mem::take(&mut token), quoted)); }
                quoted = false;
            }
            c => token.push(c),
        }
    }
    if !token.is_empty() { tokens.push((token, quoted)); }
    tokens.into_iter().map(|(text, quoted)| {
        if let Some((field, value)) = text.split_once(':') {
            if !field.is_empty() && !value.starts_with("//")
                && field.chars().all(|c| c.is_alphanumeric() || c == '_') {
                return QueryTerm { field: Some(field.to_lowercase()), text: value.into(), quoted };
            }
        }
        QueryTerm { field: None, text, quoted }
    }).collect()
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bib::model::{Entry, EntryType};
    use indexmap::IndexMap;

    fn make_entry(key: &str, fields: &[(&str, &str)]) -> Entry {
        let mut f = IndexMap::new();
        for (k, v) in fields {
            f.insert(k.to_string(), v.to_string());
        }
        Entry {
            entry_type: EntryType::Article,
            citation_key: key.to_string(),
            fields: f,
            group_memberships: vec![],
            raw_index: 0,
            dirty: false,
        }
    }

    #[test]
    fn test_empty_query_returns_all() {
        let e1 = make_entry("Smith2020", &[]);
        let e2 = make_entry("Doe2021", &[]);
        let entries = vec![&e1, &e2];
        let mut engine = SearchEngine::new();
        let results = engine.search(&entries, "");
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_parse_query_no_colon() {
        assert_eq!(parse_query("smith"), vec![QueryTerm { field: None, text: "smith".into(), quoted: false }]);
    }

    #[test]
    fn test_parse_query_with_field() {
        assert_eq!(parse_query("author:smith"), vec![QueryTerm { field: Some("author".into()), text: "smith".into(), quoted: false }]);
    }

    #[test]
    fn test_parse_query_empty_field() {
        // colon at start — not a valid field filter
        assert_eq!(parse_query(":smith"), vec![QueryTerm { field: None, text: ":smith".into(), quoted: false }]);
    }

    #[test]
    fn test_build_search_string_no_filter() {
        let e = make_entry("Smith2020", &[("author", "Smith, J."), ("year", "2020")]);
        let s = build_search_string(&e, None);
        assert!(s.contains("Smith2020"));
        assert!(s.contains("Smith, J."));
        assert!(s.contains("2020"));
    }

    #[test]
    fn test_build_search_string_field_filter() {
        let e = make_entry("Smith2020", &[("author", "Smith, J."), ("year", "2020")]);
        let s = build_search_string(&e, Some("author"));
        assert_eq!(s, "Smith, J.");
    }

    #[test]
    fn test_build_search_string_citekey_filter() {
        let e = make_entry("Smith2020", &[("author", "Smith, J.")]);
        let s = build_search_string(&e, Some("citation_key"));
        assert_eq!(s, "Smith2020");
    }

    #[test]
    fn test_build_search_string_type_filter() {
        let e = make_entry("Smith2020", &[]);
        let s = build_search_string(&e, Some("entrytype"));
        assert_eq!(s, "Article");
    }

    #[test]
    fn test_search_field_specific() {
        let e1 = make_entry("Smith2020", &[("author", "Smith, John")]);
        let e2 = make_entry("Doe2021", &[("author", "Doe, Jane")]);
        let entries = vec![&e1, &e2];
        let mut engine = SearchEngine::new();
        let results = engine.search(&entries, "author:Smith");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, 0);
    }
    #[test]
    fn multi_term_queries_and_quoted_phrases() {
        let a = make_entry("A", &[("author", "Smith, John"), ("year", "2020"), ("title", "Fast neural methods"), ("url", "https://doi.org/10.1234/a:b")]);
        let b = make_entry("B", &[("author", "Smith, John"), ("year", "2021"), ("title", "Fast useful neural methods")]);
        let c = make_entry("C", &[("author", "Jones, Jane"), ("year", "2020")]);
        let entries = [&a, &b, &c];
        let mut engine = SearchEngine::new();
        for query in ["author:smith year:2020", "AUTHOR:smith 2020", "title:\"fast neural\"", "title:\"fast neural", "https://doi.org/10.1234/a:b", "url:https://doi.org/10.1234/a:b", "key:A", "citekey:A", "citation_key:A", "10.1234/a:b"] {
            let results = engine.search(&entries, query);
            assert_eq!(results.iter().map(|r| r.0).collect::<Vec<_>>(), [0], "{query}");
        }
        for query in ["missing:smith", "author:", "author:smith year:1990", "\"fast methods\""] {
            assert!(engine.search(&entries, query).is_empty(), "{query}");
        }
        assert_eq!(engine.search(&entries, " \"\" ").len(), 3);
        assert_eq!(engine.search(&entries, "type:article").len(), 3);
        assert_eq!(engine.search(&entries, "smith").len(), 2);
        assert_eq!(engine.search(&entries, "smith 2020").len(), 1);
    }

}
