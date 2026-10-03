use crate::bib::model::Entry;
use indexmap::IndexMap;
use std::ops::Range;

/// One copy of searchable text, with field ranges for qualified queries.
/// It includes every field; long abstracts are never truncated.
pub(super) struct SearchDocument {
    pub key: String,
    text: String,
    fields: IndexMap<String, Range<usize>>,
    kind: Range<usize>,
    key_range: Range<usize>,
}

impl SearchDocument {
    pub fn new(entry: &Entry) -> Self {
        let mut text = entry.entry_type.display_name().to_string();
        let kind = 0..text.len();
        text.push(' ');
        let start = text.len();
        text.push_str(&entry.citation_key);
        let key_range = start..text.len();
        let mut fields = IndexMap::with_capacity(entry.fields.len());
        for (name, value) in &entry.fields {
            text.push(' ');
            let start = text.len();
            text.push_str(value);
            fields.insert(name.clone(), start..text.len());
        }
        Self {
            key: entry.citation_key.clone(),
            text,
            fields,
            kind,
            key_range,
        }
    }

    pub fn get(&self, field: Option<&str>) -> &str {
        let range = match field {
            None => return &self.text,
            Some("entrytype" | "type") => &self.kind,
            Some("citation_key" | "key" | "citekey") => &self.key_range,
            Some(name) => match self.fields.get(name) {
                Some(range) => range,
                None => return "",
            },
        };
        &self.text[range.clone()]
    }

    /// Validate only when the document changes; query-only updates reuse the
    /// existing snapshot. Equality avoids cache correctness depending on hashes.
    pub fn matches(&self, entry: &Entry) -> bool {
        self.key == entry.citation_key
            && self.get(Some("type")) == entry.entry_type.display_name()
            && self.fields.len() == entry.fields.len()
            && self
                .fields
                .iter()
                .zip(&entry.fields)
                .all(|((name, range), (other, value))| {
                    name == other && self.text[range.clone()] == *value
                })
    }
}

/// Build a search index string for an entry (also used by the benchmark).
pub fn build_search_index(entry: &Entry) -> String {
    SearchDocument::new(entry).text
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bib::model::{Entry, EntryType};
    use indexmap::IndexMap;

    fn make_entry(key: &str, entry_type: EntryType, fields: &[(&str, &str)]) -> Entry {
        let mut f = IndexMap::new();
        for (k, v) in fields {
            f.insert(k.to_string(), v.to_string());
        }
        Entry {
            entry_type,
            citation_key: key.to_string(),
            fields: f,
            group_memberships: vec![],
            raw_index: 0,
            dirty: false,
        }
    }

    #[test]
    fn test_index_contains_key_and_type() {
        let e = make_entry("Smith2020", EntryType::Article, &[]);
        let idx = build_search_index(&e);
        assert!(idx.contains("Smith2020"));
        assert!(idx.contains("Article"));
    }

    #[test]
    fn test_index_contains_field_values() {
        let e = make_entry(
            "Doe2021",
            EntryType::Book,
            &[("title", "Rust Programming"), ("author", "Doe, John")],
        );
        let idx = build_search_index(&e);
        assert!(idx.contains("Rust Programming"));
        assert!(idx.contains("Doe, John"));
    }
}
