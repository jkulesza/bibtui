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
