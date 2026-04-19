//! Export serialisation for non-BibTeX formats.
//!
//! Currently supported:
//!   * CSL-JSON — Citation Style Language JSON (used by Pandoc, Zotero, etc.)
//!   * RIS      — Research Information Systems plain-text format
//!
//! BibTeX export is handled by `bib::writer`.

use crate::bib::model::{Database, Entry, EntryType};

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Strip outer case-protecting braces from a BibTeX field value.
/// `{MCNP}` → `MCNP`, `{{Foo Bar}}` → `Foo Bar`.
fn strip_braces(s: &str) -> String {
    let s = s.trim();
    // Repeatedly remove matching outer `{...}`.
    let mut result = s.to_string();
    loop {
        let t = result.trim();
        if t.starts_with('{') && t.ends_with('}') {
            // Verify the opening brace actually matches the closing brace.
            let inner = &t[1..t.len() - 1];
            let mut depth = 0i32;
            let mut balanced = true;
            for ch in inner.chars() {
                if ch == '{' {
                    depth += 1;
                } else if ch == '}' {
                    depth -= 1;
                    if depth < 0 {
                        balanced = false;
                        break;
                    }
                }
            }
            if balanced && depth == 0 {
                result = inner.to_string();
                continue;
            }
        }
        break;
    }
    result
}

/// Split a BibTeX `pages` field into (first, last).
/// `100--115`, `100-115`, `100` all handled.
fn split_pages(pages: &str) -> (String, String) {
    let stripped = strip_braces(pages);
    // Double-hyphen: "100--115"
    if let Some(idx) = stripped.find("--") {
        return (stripped[..idx].to_string(), stripped[idx + 2..].to_string());
    }
    // Single hyphen: "100-115" (only when there are digits on both sides)
    if let Some(idx) = stripped.find('-') {
        let left = &stripped[..idx];
        let right = &stripped[idx + 1..];
        if !left.is_empty() && !right.is_empty() {
            return (left.to_string(), right.to_string());
        }
    }
    (stripped.clone(), stripped)
}

/// Parse a BibTeX author string into `(family, given)` pairs.
fn parse_authors(author_str: &str) -> Vec<(String, String)> {
    author_str
        .split(" and ")
        .map(|a| {
            let a = a.trim();
            if let Some(comma) = a.find(',') {
                let family = strip_braces(a[..comma].trim());
                let given = strip_braces(a[comma + 1..].trim());
                (family, given)
            } else {
                // "First Last" form — last token is family name
                let parts: Vec<&str> = a.split_whitespace().collect();
                match parts.len() {
                    0 => (String::new(), String::new()),
                    1 => (strip_braces(parts[0]), String::new()),
                    _ => {
                        let family = strip_braces(parts[parts.len() - 1]);
                        let given = parts[..parts.len() - 1].join(" ");
                        (family, strip_braces(&given))
                    }
                }
            }
        })
        .filter(|(f, _)| !f.is_empty())
        .collect()
}

// ── CSL-JSON ──────────────────────────────────────────────────────────────────

/// Map a BibTeX `EntryType` to a CSL item type string.
fn csl_type(entry_type: &EntryType) -> &'static str {
    match entry_type {
        EntryType::Article => "article-journal",
        EntryType::Book | EntryType::Booklet => "book",
        EntryType::InBook | EntryType::InCollection => "chapter",
        EntryType::InProceedings | EntryType::Proceedings => "paper-conference",
        EntryType::MastersThesis | EntryType::PhdThesis => "thesis",
        EntryType::TechReport => "report",
        EntryType::Manual => "document",
        EntryType::Unpublished => "manuscript",
        EntryType::Misc | EntryType::Other(_) => "article",
    }
}

/// Serialise a single entry as a CSL-JSON object (a JSON `{...}` value).
fn entry_to_csl_json(entry: &Entry) -> serde_json::Value {
    let mut obj = serde_json::Map::new();

    obj.insert("id".to_string(), serde_json::Value::String(entry.citation_key.clone()));
    obj.insert("type".to_string(), serde_json::Value::String(csl_type(&entry.entry_type).to_string()));

    // Authors
    if let Some(author_str) = entry.fields.get("author") {
        let authors: Vec<serde_json::Value> = parse_authors(author_str)
            .into_iter()
            .map(|(family, given)| {
                let mut a = serde_json::Map::new();
                a.insert("family".to_string(), serde_json::Value::String(family));
                if !given.is_empty() {
                    a.insert("given".to_string(), serde_json::Value::String(given));
                }
                serde_json::Value::Object(a)
            })
            .collect();
        obj.insert("author".to_string(), serde_json::Value::Array(authors));
    }

    // Editors
    if let Some(editor_str) = entry.fields.get("editor") {
        let editors: Vec<serde_json::Value> = parse_authors(editor_str)
            .into_iter()
            .map(|(family, given)| {
                let mut a = serde_json::Map::new();
                a.insert("family".to_string(), serde_json::Value::String(family));
                if !given.is_empty() {
                    a.insert("given".to_string(), serde_json::Value::String(given));
                }
                serde_json::Value::Object(a)
            })
            .collect();
        obj.insert("editor".to_string(), serde_json::Value::Array(editors));
    }

    // Title
    if let Some(title) = entry.fields.get("title") {
        obj.insert("title".to_string(), serde_json::Value::String(strip_braces(title)));
    }

    // Container title (journal, booktitle)
    let container = entry.fields.get("journal")
        .or_else(|| entry.fields.get("booktitle"));
    if let Some(ct) = container {
        obj.insert("container-title".to_string(), serde_json::Value::String(strip_braces(ct)));
    }

    // Year → issued date-parts
    if let Some(year) = entry.fields.get("year") {
        if let Ok(y) = year.trim().parse::<i64>() {
            let mut issued = serde_json::Map::new();
            issued.insert(
                "date-parts".to_string(),
                serde_json::Value::Array(vec![serde_json::Value::Array(vec![
                    serde_json::Value::Number(y.into()),
                ])]),
            );
            obj.insert("issued".to_string(), serde_json::Value::Object(issued));
        }
    }

    // Simple string fields
    let string_fields: &[(&str, &str)] = &[
        ("volume", "volume"),
        ("number", "issue"),
        ("publisher", "publisher"),
        ("address", "publisher-place"),
        ("edition", "edition"),
        ("series", "collection-title"),
        ("doi", "DOI"),
        ("url", "URL"),
        ("isbn", "ISBN"),
        ("issn", "ISSN"),
        ("note", "note"),
        ("abstract", "abstract"),
    ];
    for (bib_key, csl_key) in string_fields {
        if let Some(val) = entry.fields.get(*bib_key) {
            obj.insert(csl_key.to_string(), serde_json::Value::String(strip_braces(val)));
        }
    }

    // Pages → page + page-first
    if let Some(pages) = entry.fields.get("pages") {
        let clean = strip_braces(pages);
        obj.insert("page".to_string(), serde_json::Value::String(clean.clone()));
        let (first, _) = split_pages(&clean);
        obj.insert("page-first".to_string(), serde_json::Value::String(first));
    }

    serde_json::Value::Object(obj)
}

/// Serialise all entries in `db` to a CSL-JSON string.
pub fn export_csl_json(db: &Database) -> anyhow::Result<String> {
    let items: Vec<serde_json::Value> = db
        .entries
        .values()
        .map(entry_to_csl_json)
        .collect();
    let json = serde_json::to_string_pretty(&serde_json::Value::Array(items))?;
    Ok(json)
}

// ── RIS ───────────────────────────────────────────────────────────────────────

/// Map a BibTeX `EntryType` to a RIS type tag.
fn ris_type(entry_type: &EntryType) -> &'static str {
    match entry_type {
        EntryType::Article => "JOUR",
        EntryType::Book | EntryType::Booklet => "BOOK",
        EntryType::InBook | EntryType::InCollection => "CHAP",
        EntryType::InProceedings | EntryType::Proceedings => "CONF",
        EntryType::MastersThesis => "THES",
        EntryType::PhdThesis => "THES",
        EntryType::TechReport => "RPRT",
        EntryType::Manual => "GEN",
        EntryType::Unpublished => "UNPB",
        EntryType::Misc | EntryType::Other(_) => "GEN",
    }
}

/// Serialise a single entry in RIS format.
fn entry_to_ris(entry: &Entry) -> String {
    let mut lines: Vec<String> = Vec::new();

    lines.push(format!("TY  - {}", ris_type(&entry.entry_type)));
    lines.push(format!("ID  - {}", entry.citation_key));

    // Authors: one AU line per author
    if let Some(author_str) = entry.fields.get("author") {
        for (family, given) in parse_authors(author_str) {
            if given.is_empty() {
                lines.push(format!("AU  - {}", family));
            } else {
                lines.push(format!("AU  - {}, {}", family, given));
            }
        }
    }

    // Editors
    if let Some(editor_str) = entry.fields.get("editor") {
        for (family, given) in parse_authors(editor_str) {
            if given.is_empty() {
                lines.push(format!("ED  - {}", family));
            } else {
                lines.push(format!("ED  - {}, {}", family, given));
            }
        }
    }

    // Simple field mappings: (bib_key, ris_tag)
    let mappings: &[(&str, &str)] = &[
        ("title",      "TI"),
        ("journal",    "JO"),
        ("booktitle",  "T2"),
        ("year",       "PY"),
        ("volume",     "VL"),
        ("number",     "IS"),
        ("publisher",  "PB"),
        ("address",    "CY"),
        ("edition",    "ET"),
        ("series",     "T3"),
        ("doi",        "DO"),
        ("url",        "UR"),
        ("isbn",       "SN"),
        ("issn",       "SN"),
        ("note",       "N1"),
        ("abstract",   "AB"),
        ("keywords",   "KW"),
    ];
    for (bib_key, ris_tag) in mappings {
        if let Some(val) = entry.fields.get(*bib_key) {
            lines.push(format!("{}  - {}", ris_tag, strip_braces(val)));
        }
    }

    // Pages → SP (start page) + EP (end page)
    if let Some(pages) = entry.fields.get("pages") {
        let (start, end) = split_pages(&strip_braces(pages));
        lines.push(format!("SP  - {}", start));
        if end != start {
            lines.push(format!("EP  - {}", end));
        }
    }

    lines.push("ER  - ".to_string());
    lines.join("\n")
}

/// Serialise all entries in `db` to a RIS string.
pub fn export_ris(db: &Database) -> String {
    db.entries
        .values()
        .map(|e| entry_to_ris(e))
        .collect::<Vec<_>>()
        .join("\n\n")
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::IndexMap;

    fn make_article() -> Entry {
        let mut fields = IndexMap::new();
        fields.insert("author".to_string(), "Smith, Jane and Jones, Bob".to_string());
        fields.insert("title".to_string(), "{An Introduction to Testing}".to_string());
        fields.insert("journal".to_string(), "Nuclear Science and Engineering".to_string());
        fields.insert("year".to_string(), "2020".to_string());
        fields.insert("volume".to_string(), "194".to_string());
        fields.insert("pages".to_string(), "1--20".to_string());
        fields.insert("doi".to_string(), "10.1234/test".to_string());
        Entry {
            entry_type: EntryType::Article,
            citation_key: "Smith2020".to_string(),
            fields,
            group_memberships: vec![],
            raw_index: 0,
            dirty: false,
        }
    }

    fn make_db_with(entry: Entry) -> Database {
        let mut entries = IndexMap::new();
        let key = entry.citation_key.clone();
        entries.insert(key, entry);
        Database {
            entries,
            groups: crate::bib::model::GroupTree::default(),
            jabref_meta: crate::bib::model::JabRefMeta::default(),
            raw_file: crate::bib::model::RawBibFile { items: vec![] },
        }
    }

    #[test]
    fn test_strip_braces_single() {
        assert_eq!(strip_braces("{hello}"), "hello");
    }

    #[test]
    fn test_strip_braces_double() {
        assert_eq!(strip_braces("{{MCNP}}"), "MCNP");
    }

    #[test]
    fn test_strip_braces_inner_unbalanced() {
        // Inner `{` without matching `}` → outer braces removed only once.
        assert_eq!(strip_braces("{a{b}c}"), "a{b}c");
    }

    #[test]
    fn test_split_pages_double_hyphen() {
        assert_eq!(split_pages("100--115"), ("100".to_string(), "115".to_string()));
    }

    #[test]
    fn test_split_pages_single_hyphen() {
        assert_eq!(split_pages("100-115"), ("100".to_string(), "115".to_string()));
    }

    #[test]
    fn test_split_pages_single() {
        assert_eq!(split_pages("100"), ("100".to_string(), "100".to_string()));
    }

    #[test]
    fn test_parse_authors_comma_form() {
        let authors = parse_authors("Smith, Jane and Jones, Bob");
        assert_eq!(authors[0], ("Smith".to_string(), "Jane".to_string()));
        assert_eq!(authors[1], ("Jones".to_string(), "Bob".to_string()));
    }

    #[test]
    fn test_parse_authors_natural_form() {
        let authors = parse_authors("Jane Smith and Bob Jones");
        assert_eq!(authors[0], ("Smith".to_string(), "Jane".to_string()));
        assert_eq!(authors[1], ("Jones".to_string(), "Bob".to_string()));
    }

    #[test]
    fn test_csl_json_structure() {
        let entry = make_article();
        let db = make_db_with(entry);
        let json_str = export_csl_json(&db).expect("CSL-JSON export should not fail");
        let parsed: serde_json::Value = serde_json::from_str(&json_str).expect("valid JSON");
        let arr = parsed.as_array().expect("top-level array");
        assert_eq!(arr.len(), 1);
        let obj = &arr[0];
        assert_eq!(obj["id"], "Smith2020");
        assert_eq!(obj["type"], "article-journal");
        assert_eq!(obj["title"], "An Introduction to Testing");
        assert_eq!(obj["container-title"], "Nuclear Science and Engineering");
        assert_eq!(obj["volume"], "194");
        assert_eq!(obj["page"], "1--20");
        assert_eq!(obj["page-first"], "1");
        assert_eq!(obj["DOI"], "10.1234/test");

        // Authors
        let authors = obj["author"].as_array().unwrap();
        assert_eq!(authors.len(), 2);
        assert_eq!(authors[0]["family"], "Smith");
        assert_eq!(authors[0]["given"], "Jane");
    }

    #[test]
    fn test_csl_json_year_date_parts() {
        let entry = make_article();
        let db = make_db_with(entry);
        let json_str = export_csl_json(&db).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        let issued = &parsed[0]["issued"];
        assert_eq!(issued["date-parts"][0][0], 2020);
    }

    #[test]
    fn test_ris_output_contains_required_tags() {
        let entry = make_article();
        let db = make_db_with(entry);
        let ris = export_ris(&db);
        assert!(ris.contains("TY  - JOUR"), "missing TY tag");
        assert!(ris.contains("ID  - Smith2020"), "missing ID tag");
        assert!(ris.contains("AU  - Smith, Jane"), "missing AU tag");
        assert!(ris.contains("TI  - An Introduction to Testing"), "missing TI tag");
        assert!(ris.contains("JO  - Nuclear Science and Engineering"), "missing JO tag");
        assert!(ris.contains("PY  - 2020"), "missing PY tag");
        assert!(ris.contains("SP  - 1"), "missing SP tag");
        assert!(ris.contains("EP  - 20"), "missing EP tag");
        assert!(ris.contains("ER  - "), "missing ER tag");
    }

    #[test]
    fn test_ris_entry_type_book() {
        let mut fields = IndexMap::new();
        fields.insert("author".to_string(), "Knuth, Donald E.".to_string());
        fields.insert("title".to_string(), "The Art of Computer Programming".to_string());
        fields.insert("year".to_string(), "1997".to_string());
        let entry = Entry {
            entry_type: EntryType::Book,
            citation_key: "Knuth1997".to_string(),
            fields,
            group_memberships: vec![],
            raw_index: 0,
            dirty: false,
        };
        let db = make_db_with(entry);
        let ris = export_ris(&db);
        assert!(ris.contains("TY  - BOOK"));
    }

    #[test]
    fn test_csl_json_thesis_type() {
        let mut fields = IndexMap::new();
        fields.insert("author".to_string(), "Doe, Jane".to_string());
        fields.insert("title".to_string(), "Dissertation Title".to_string());
        fields.insert("year".to_string(), "2021".to_string());
        let entry = Entry {
            entry_type: EntryType::PhdThesis,
            citation_key: "Doe2021".to_string(),
            fields,
            group_memberships: vec![],
            raw_index: 0,
            dirty: false,
        };
        let db = make_db_with(entry);
        let json_str = export_csl_json(&db).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(parsed[0]["type"], "thesis");
    }

    // ── csl_type / ris_type coverage for remaining entry types ───────────────

    fn make_entry(entry_type: EntryType, key: &str) -> Entry {
        let mut fields = IndexMap::new();
        fields.insert("title".to_string(), "A Title".to_string());
        fields.insert("year".to_string(), "2000".to_string());
        Entry {
            entry_type,
            citation_key: key.to_string(),
            fields,
            group_memberships: vec![],
            raw_index: 0,
            dirty: false,
        }
    }

    #[test]
    fn test_csl_type_mappings() {
        let cases = [
            (EntryType::Booklet,       "book"),
            (EntryType::InBook,        "chapter"),
            (EntryType::InCollection,  "chapter"),
            (EntryType::InProceedings, "paper-conference"),
            (EntryType::Proceedings,   "paper-conference"),
            (EntryType::MastersThesis, "thesis"),
            (EntryType::TechReport,    "report"),
            (EntryType::Manual,        "document"),
            (EntryType::Unpublished,   "manuscript"),
            (EntryType::Misc,          "article"),
            (EntryType::Other("custom".to_string()), "article"),
        ];
        for (et, expected) in cases {
            assert_eq!(csl_type(&et), expected, "failed for {:?}", et);
        }
    }

    #[test]
    fn test_ris_type_mappings() {
        let cases = [
            (EntryType::Booklet,       "BOOK"),
            (EntryType::InCollection,  "CHAP"),
            (EntryType::InProceedings, "CONF"),
            (EntryType::Proceedings,   "CONF"),
            (EntryType::MastersThesis, "THES"),
            (EntryType::TechReport,    "RPRT"),
            (EntryType::Manual,        "GEN"),
            (EntryType::Unpublished,   "UNPB"),
            (EntryType::Misc,          "GEN"),
            (EntryType::Other("x".to_string()), "GEN"),
        ];
        for (et, expected) in cases {
            assert_eq!(ris_type(&et), expected, "failed for {:?}", et);
        }
    }

    // ── parse_authors edge cases ──────────────────────────────────────────────

    #[test]
    fn test_parse_authors_single_word() {
        let authors = parse_authors("Einstein");
        assert_eq!(authors.len(), 1);
        assert_eq!(authors[0].0, "Einstein");
        assert_eq!(authors[0].1, "");
    }

    #[test]
    fn test_parse_authors_empty_string() {
        let authors = parse_authors("");
        assert!(authors.is_empty());
    }

    #[test]
    fn test_parse_authors_natural_form_single_token() {
        // single name with no comma and no spaces
        let authors = parse_authors("Plato and Socrates");
        assert_eq!(authors.len(), 2);
        assert_eq!(authors[0], ("Plato".to_string(), "".to_string()));
        assert_eq!(authors[1], ("Socrates".to_string(), "".to_string()));
    }

    // ── RIS edge cases ────────────────────────────────────────────────────────

    #[test]
    fn test_ris_with_editor() {
        let mut fields = IndexMap::new();
        fields.insert("editor".to_string(), "Brown, Charlie".to_string());
        fields.insert("title".to_string(), "Handbook".to_string());
        fields.insert("year".to_string(), "2005".to_string());
        let entry = Entry {
            entry_type: EntryType::Book,
            citation_key: "Brown2005".to_string(),
            fields,
            group_memberships: vec![],
            raw_index: 0,
            dirty: false,
        };
        let db = make_db_with(entry);
        let ris = export_ris(&db);
        assert!(ris.contains("ED  - Brown, Charlie"), "missing ED tag");
    }

    #[test]
    fn test_ris_single_page_omits_ep() {
        let mut fields = IndexMap::new();
        fields.insert("pages".to_string(), "42".to_string());
        fields.insert("title".to_string(), "A Note".to_string());
        fields.insert("year".to_string(), "2010".to_string());
        let entry = Entry {
            entry_type: EntryType::Article,
            citation_key: "Note2010".to_string(),
            fields,
            group_memberships: vec![],
            raw_index: 0,
            dirty: false,
        };
        let db = make_db_with(entry);
        let ris = export_ris(&db);
        assert!(ris.contains("SP  - 42"), "missing SP tag");
        assert!(!ris.contains("EP  - "), "should not have EP tag for single page");
    }

    #[test]
    fn test_ris_author_without_given_name() {
        // An author with only a family name should produce "AU  - FamilyName"
        let mut fields = IndexMap::new();
        fields.insert("author".to_string(), "Plato".to_string());
        fields.insert("title".to_string(), "Republic".to_string());
        fields.insert("year".to_string(), "-380".to_string());
        let entry = Entry {
            entry_type: EntryType::Book,
            citation_key: "Plato380".to_string(),
            fields,
            group_memberships: vec![],
            raw_index: 0,
            dirty: false,
        };
        let db = make_db_with(entry);
        let ris = export_ris(&db);
        assert!(ris.contains("AU  - Plato"), "missing AU tag for single-name author");
        assert!(!ris.contains("AU  - Plato,"), "should not have trailing comma");
    }

    // ── CSL-JSON edge cases ───────────────────────────────────────────────────

    #[test]
    fn test_csl_json_booktitle_as_container() {
        let mut fields = IndexMap::new();
        fields.insert("author".to_string(), "Lee, Bob".to_string());
        fields.insert("title".to_string(), "A Chapter".to_string());
        fields.insert("booktitle".to_string(), "The Big Book".to_string());
        fields.insert("year".to_string(), "2015".to_string());
        let entry = Entry {
            entry_type: EntryType::InCollection,
            citation_key: "Lee2015".to_string(),
            fields,
            group_memberships: vec![],
            raw_index: 0,
            dirty: false,
        };
        let db = make_db_with(entry);
        let json_str = export_csl_json(&db).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(parsed[0]["container-title"], "The Big Book");
    }

    #[test]
    fn test_csl_json_with_editor() {
        let mut fields = IndexMap::new();
        fields.insert("editor".to_string(), "Green, Alice".to_string());
        fields.insert("title".to_string(), "Collected Works".to_string());
        fields.insert("year".to_string(), "2000".to_string());
        let entry = Entry {
            entry_type: EntryType::Book,
            citation_key: "Green2000".to_string(),
            fields,
            group_memberships: vec![],
            raw_index: 0,
            dirty: false,
        };
        let db = make_db_with(entry);
        let json_str = export_csl_json(&db).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        let editors = parsed[0]["editor"].as_array().unwrap();
        assert_eq!(editors[0]["family"], "Green");
        assert_eq!(editors[0]["given"], "Alice");
    }

    #[test]
    fn test_csl_json_non_numeric_year_omits_issued() {
        let mut fields = IndexMap::new();
        fields.insert("title".to_string(), "Undated Work".to_string());
        fields.insert("year".to_string(), "forthcoming".to_string());
        let entry = Entry {
            entry_type: EntryType::Misc,
            citation_key: "Undated".to_string(),
            fields,
            group_memberships: vec![],
            raw_index: 0,
            dirty: false,
        };
        let db = make_db_with(entry);
        let json_str = export_csl_json(&db).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        // "forthcoming" cannot be parsed as i64, so "issued" must be absent.
        assert!(parsed[0]["issued"].is_null(), "issued should be absent for non-numeric year");
    }
}
