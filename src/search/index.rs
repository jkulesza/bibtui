use crate::bib::model::Entry;

/// Build a search index string for an entry (for pre-computing search targets).
#[allow(dead_code)]
pub fn build_search_index(entry: &Entry) -> String {
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
