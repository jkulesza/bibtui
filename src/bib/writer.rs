use super::entry_types::fields_for_type;
use super::model::*;

/// Serialize a RawBibFile back to a string.
/// Unmodified entries use their raw_text for byte-perfect round-trip.
pub fn write_bib_file(raw: &RawBibFile) -> String {
    let mut out = String::new();

    for item in &raw.items {
        match item {
            RawItem::Preamble(text) => {
                out.push_str(text);
            }
            RawItem::BibPreamble { raw_text, .. } => {
                out.push_str(raw_text);
            }
            RawItem::StringDef { raw_text, .. } => {
                out.push_str(raw_text);
            }
            RawItem::Comment { raw_text } => {
                out.push_str(raw_text);
            }
            RawItem::Entry(entry) => {
                // Use raw_text for passthrough (unmodified entries)
                out.push_str(&entry.raw_text);
            }
        }
    }

    out
}

/// Replace any sequence of 3 or more consecutive newlines with exactly two
/// (i.e. at most one blank line between items).
pub fn normalize_blank_lines(s: String) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut newline_run = 0usize;

    for &b in bytes {
        if b == b'\n' {
            newline_run += 1;
            if newline_run <= 2 {
                out.push(b);
            }
        } else {
            newline_run = 0;
            out.push(b);
        }
    }

    // Input was valid UTF-8 and we only dropped '\n' bytes, so this never fails.
    String::from_utf8(out).expect("dropping newline bytes preserves UTF-8 validity")
}

/// Serialize a single entry from semantic data (for modified entries).
///
/// When `original` is the raw entry this entry was parsed from, any field whose
/// semantic value is unchanged is written back using its original source text
/// (preserving `#` concatenation, `@String` references, and quoted vs braced
/// style). Only genuinely changed or new fields get the standard braced
/// formatting.
pub fn serialize_entry(
    entry: &Entry,
    align: bool,
    sort_fields: bool,
    original: Option<&RawEntry>,
) -> String {
    let mut out = String::new();

    out.push_str(&format!(
        "@{}{{{},\n",
        entry.entry_type.display_name(),
        entry.citation_key
    ));

    // Optionally sort fields: required (alpha) → optional (alpha) → nonstandard (alpha)
    let sorted_keys: Vec<String>;
    let field_iter: Box<dyn Iterator<Item = (&String, &String)>> = if sort_fields {
        let (required, optional) = fields_for_type(&entry.entry_type);
        let req_set: std::collections::HashSet<&str> = required.iter().copied().collect();
        let opt_set: std::collections::HashSet<&str> = optional.iter().copied().collect();

        let mut req_keys: Vec<&String> = entry.fields.keys()
            .filter(|k| req_set.contains(k.as_str())).collect();
        let mut opt_keys: Vec<&String> = entry.fields.keys()
            .filter(|k| opt_set.contains(k.as_str())).collect();
        let mut other_keys: Vec<&String> = entry.fields.keys()
            .filter(|k| !req_set.contains(k.as_str()) && !opt_set.contains(k.as_str())).collect();

        req_keys.sort_unstable_by_key(|a| a.to_lowercase());
        opt_keys.sort_unstable_by_key(|a| a.to_lowercase());
        other_keys.sort_unstable_by_key(|a| a.to_lowercase());

        sorted_keys = req_keys.into_iter()
            .chain(opt_keys)
            .chain(other_keys).cloned()
            .collect();
        Box::new(sorted_keys.iter().map(|k| (k, &entry.fields[k])))
    } else {
        Box::new(entry.fields.iter())
    };

    // Compute alignment width
    let align_width = if align {
        entry
            .fields
            .keys()
            .map(|k| k.len())
            .max()
            .unwrap_or(0)
    } else {
        0
    };

    for (key, value) in field_iter {
        let padding = if align && key.len() < align_width {
            " ".repeat(align_width - key.len())
        } else {
            String::new()
        };

        // Determine how to format the value. Fields whose semantic value is
        // unchanged from the original raw entry keep their original source
        // text; changed or new fields get standard braced formatting.
        let formatted_value = raw_value_for_field(key, value, original).to_source_text();

        out.push_str(&format!(
            "  {}{} = {},\n",
            key, padding, formatted_value
        ));
    }

    out.push_str("}\n");
    out
}

/// Choose the raw value to write for a field: when the semantic value is
/// unchanged from the original raw entry, the original `RawFieldValue` is
/// reused verbatim (preserving `#` concatenation, `@String` references, and
/// quoted style); otherwise the value is braced (bare month tokens stay bare).
pub fn raw_value_for_field(
    field_name: &str,
    value: &str,
    original: Option<&RawEntry>,
) -> RawFieldValue {
    if let Some(orig_field) = original
        .and_then(|raw| raw.fields.iter().find(|f| f.name == field_name))
        .filter(|f| f.value.to_string_value() == value)
    {
        return orig_field.value.clone();
    }

    // Month values stay bare if they're standard month abbreviations
    if field_name == "month" {
        let lower = value.to_lowercase();
        if matches!(
            lower.as_str(),
            "jan" | "feb" | "mar" | "apr" | "may" | "jun" | "jul" | "aug" | "sep" | "oct"
                | "nov" | "dec"
        ) {
            return RawFieldValue::Bare(lower);
        }
    }

    RawFieldValue::Braced(value.to_string())
}

/// Build the raw field list for a re-serialized entry so that a later save can
/// still recognize (and preserve) fields the user never changed. Unchanged
/// fields keep their original `RawFieldValue`; changed or new fields carry the
/// standard braced value.
pub fn merged_raw_fields(entry: &Entry, original: Option<&RawEntry>) -> Vec<RawField> {
    entry
        .fields
        .iter()
        .map(|(key, value)| RawField {
            name: key.clone(),
            value: raw_value_for_field(key, value, original),
        })
        .collect()
}

/// Format a field value for writing. Bare tokens (months) stay bare,
/// everything else gets braces.
pub fn format_field_value(field_name: &str, value: &str) -> String {
    // Month values stay bare if they're standard month abbreviations
    if field_name == "month" {
        let lower = value.to_lowercase();
        if matches!(
            lower.as_str(),
            "jan" | "feb" | "mar" | "apr" | "may" | "jun" | "jul" | "aug" | "sep" | "oct"
                | "nov" | "dec"
        ) {
            return value.to_lowercase();
        }
    }

    format!("{{{}}}", value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bib::model::{Entry, EntryType, RawBibFile, RawEntry, RawItem};
    use indexmap::IndexMap;

    fn make_test_entry() -> Entry {
        let mut fields = IndexMap::new();
        fields.insert("year".to_string(), "2020".to_string());
        fields.insert("title".to_string(), "A Test".to_string());
        Entry {
            entry_type: EntryType::Article,
            citation_key: "Smith2020".to_string(),
            fields,
            group_memberships: vec![],
            raw_index: 0,
            dirty: false,
        }
    }

    #[test]
    fn test_format_field_value_month_bare() {
        assert_eq!(format_field_value("month", "jan"), "jan");
        assert_eq!(format_field_value("month", "dec"), "dec");
        // "January" is not a bare abbreviation — gets braces
        assert_eq!(format_field_value("month", "January"), "{January}");
    }

    #[test]
    fn test_format_field_value_other() {
        assert_eq!(format_field_value("title", "Hello World"), "{Hello World}");
    }

    #[test]
    fn test_serialize_entry_no_align() {
        let entry = make_test_entry();
        let result = serialize_entry(&entry, false, false, None);
        assert!(result.starts_with("@Article{Smith2020,"), "result: {}", result);
        assert!(result.contains("year = {2020}"), "result: {}", result);
        assert!(result.contains("title = {A Test}"), "result: {}", result);
    }

    #[test]
    fn test_serialize_entry_align() {
        let entry = make_test_entry();
        let result = serialize_entry(&entry, true, false, None);
        // max key len is "title" = 5; "year" = 4 gets 1 extra space
        assert!(result.starts_with("@Article{Smith2020,"), "result: {}", result);
        assert!(result.contains("year  ="), "result: {}", result);
    }

    #[test]
    fn test_write_bib_file_preamble_passthrough() {
        let raw = RawBibFile {
            items: vec![RawItem::Preamble("% comment\n".to_string())],
        };
        let result = write_bib_file(&raw);
        assert_eq!(result, "% comment\n");
    }

    #[test]
    fn test_write_bib_file_entry_passthrough() {
        let raw_text = "@Article{k,\n  year = {2020},\n}".to_string();
        let raw = RawBibFile {
            items: vec![RawItem::Entry(RawEntry {
                entry_type: "Article".into(),
                citation_key: "k".into(),
                fields: vec![],
                raw_text: raw_text.clone(),
            })],
        };
        let result = write_bib_file(&raw);
        assert_eq!(result, raw_text);
    }

    #[test]
    fn test_serialize_entry_trailing_newline() {
        let entry = make_test_entry();
        let result = serialize_entry(&entry, false, false, None);
        assert!(result.ends_with("}\n"), "result: {:?}", result);
    }

    #[test]
    fn test_serialize_entry_sort_fields_required_first() {
        let mut fields = IndexMap::new();
        // Insert in reverse order: nonstandard, optional, required
        fields.insert("abstract".to_string(), "Some abstract".to_string()); // nonstandard for Article
        fields.insert("volume".to_string(), "12".to_string());   // optional
        fields.insert("pages".to_string(), "1--10".to_string()); // optional
        fields.insert("year".to_string(), "2020".to_string());   // required
        fields.insert("title".to_string(), "My Title".to_string()); // required
        fields.insert("author".to_string(), "Smith, J".to_string()); // required
        fields.insert("journal".to_string(), "Nature".to_string()); // required
        let entry = Entry {
            entry_type: EntryType::Article,
            citation_key: "Smith2020".to_string(),
            fields,
            group_memberships: vec![],
            raw_index: 0,
            dirty: false,
        };
        let result = serialize_entry(&entry, false, true, None);
        // Required fields (author, journal, title, year) must appear before optional (pages, volume)
        // and optional before nonstandard (abstract)
        let author_pos = result.find("author").unwrap();
        let journal_pos = result.find("journal").unwrap();
        let title_pos = result.find("  title").unwrap();
        let year_pos = result.find("year").unwrap();
        let pages_pos = result.find("pages").unwrap();
        let volume_pos = result.find("volume").unwrap();
        let abstract_pos = result.find("abstract").unwrap();
        // All required before all optional
        assert!(author_pos < pages_pos, "author before pages");
        assert!(journal_pos < pages_pos, "journal before pages");
        assert!(title_pos < pages_pos, "title before pages");
        assert!(year_pos < pages_pos, "year before pages");
        // All optional before nonstandard
        assert!(pages_pos < abstract_pos, "pages before abstract");
        assert!(volume_pos < abstract_pos, "volume before abstract");
    }

    #[test]
    fn test_normalize_blank_lines_no_change() {
        // One blank line (2 newlines) is preserved
        let s = "entry1\n\nentry2\n".to_string();
        assert_eq!(normalize_blank_lines(s.clone()), s);
    }

    #[test]
    fn test_normalize_blank_lines_triple_collapses() {
        let s = "a\n\n\nb".to_string();
        assert_eq!(normalize_blank_lines(s), "a\n\nb");
    }

    #[test]
    fn test_normalize_blank_lines_many_collapses() {
        let s = "a\n\n\n\n\n\nb".to_string();
        assert_eq!(normalize_blank_lines(s), "a\n\nb");
    }

    #[test]
    fn test_normalize_blank_lines_no_newlines_unchanged() {
        let s = "hello world".to_string();
        assert_eq!(normalize_blank_lines(s.clone()), s);
    }

    #[test]
    fn test_normalize_blank_lines_empty_string() {
        assert_eq!(normalize_blank_lines(String::new()), "");
    }

    #[test]
    fn test_write_bib_file_bibpreamble() {
        let raw = RawBibFile {
            items: vec![RawItem::BibPreamble {
                content: "\"hello\"".to_string(),
                raw_text: "@Preamble{\"hello\"}".to_string(),
            }],
        };
        assert_eq!(write_bib_file(&raw), "@Preamble{\"hello\"}");
    }

    #[test]
    fn test_write_bib_file_stringdef() {
        let raw = RawBibFile {
            items: vec![RawItem::StringDef {
                name: "mystr".to_string(),
                raw_value: "{value}".to_string(),
                raw_text: "@string{mystr={value}}".to_string(),
            }],
        };
        // raw_text is written back verbatim (byte-perfect), not re-rendered.
        assert_eq!(write_bib_file(&raw), "@string{mystr={value}}");
    }

    #[test]
    fn test_write_bib_file_comment() {
        let raw = RawBibFile {
            items: vec![RawItem::Comment { raw_text: "@Comment{groups stuff}".to_string() }],
        };
        assert_eq!(write_bib_file(&raw), "@Comment{groups stuff}");
    }

    #[test]
    fn test_write_bib_file_multiple_items() {
        let raw = RawBibFile {
            items: vec![
                RawItem::Preamble("% header\n".to_string()),
                RawItem::Entry(RawEntry {
                    entry_type: "Article".into(),
                    citation_key: "k".into(),
                    fields: vec![],
                    raw_text: "@Article{k,\n}\n".to_string(),
                }),
                RawItem::Comment { raw_text: "@Comment{x}".to_string() },
            ],
        };
        let result = write_bib_file(&raw);
        assert!(result.starts_with("% header\n"));
        assert!(result.contains("@Article{k,"));
        assert!(result.ends_with("@Comment{x}"));
    }

    #[test]
    fn test_serialize_entry_preserves_unchanged_raw_values() {
        use crate::bib::model::{RawField, RawFieldValue};
        let mut fields = IndexMap::new();
        fields.insert("journal".to_string(), "ieee_tps , Part B".to_string());
        fields.insert("series".to_string(), "mainseries".to_string());
        fields.insert("note".to_string(), "quoted note".to_string());
        fields.insert("title".to_string(), "Changed Title".to_string());
        let entry = Entry {
            entry_type: EntryType::Article,
            citation_key: "k".to_string(),
            fields,
            group_memberships: vec![],
            raw_index: 0,
            dirty: true,
        };
        let original = RawEntry {
            entry_type: "Article".into(),
            citation_key: "k".into(),
            fields: vec![
                RawField {
                    name: "journal".to_string(),
                    value: RawFieldValue::Concat(vec![
                        RawFieldValue::Bare("ieee_tps".to_string()),
                        RawFieldValue::Braced(", Part B".to_string()),
                    ]),
                },
                RawField {
                    name: "series".to_string(),
                    value: RawFieldValue::Bare("mainseries".to_string()),
                },
                RawField {
                    name: "note".to_string(),
                    value: RawFieldValue::Quoted("quoted note".to_string()),
                },
                RawField {
                    name: "title".to_string(),
                    value: RawFieldValue::Braced("Old Title".to_string()),
                },
            ],
            raw_text: String::new(),
        };
        let result = serialize_entry(&entry, false, false, Some(&original));
        // Unchanged fields keep their raw source form.
        assert!(
            result.contains("journal = ieee_tps # {, Part B},"),
            "concat must survive: {}", result
        );
        assert!(result.contains("series = mainseries,"), "bare reference must survive: {}", result);
        assert!(result.contains("note = \"quoted note\","), "quoted style must survive: {}", result);
        // The changed field gets standard braced formatting.
        assert!(result.contains("title = {Changed Title},"), "got: {}", result);
    }

    #[test]
    fn test_merged_raw_fields_keeps_unchanged_and_braces_changed() {
        use crate::bib::model::{RawField, RawFieldValue};
        let mut fields = IndexMap::new();
        fields.insert("journal".to_string(), "abc def".to_string());
        fields.insert("year".to_string(), "2021".to_string());
        let entry = Entry {
            entry_type: EntryType::Article,
            citation_key: "k".to_string(),
            fields,
            group_memberships: vec![],
            raw_index: 0,
            dirty: true,
        };
        let original = RawEntry {
            entry_type: "Article".into(),
            citation_key: "k".into(),
            fields: vec![RawField {
                name: "journal".to_string(),
                value: RawFieldValue::Concat(vec![
                    RawFieldValue::Bare("abc".to_string()),
                    RawFieldValue::Braced("def".to_string()),
                ]),
            }],
            raw_text: String::new(),
        };
        let merged = merged_raw_fields(&entry, Some(&original));
        assert_eq!(merged.len(), 2);
        assert!(matches!(merged[0].value, RawFieldValue::Concat(_)));
        assert!(matches!(merged[1].value, RawFieldValue::Braced(ref s) if s == "2021"));
    }

    #[test]
    fn test_format_field_value_all_months() {
        for m in &["jan","feb","mar","apr","may","jun","jul","aug","sep","oct","nov","dec"] {
            assert_eq!(format_field_value("month", m), *m, "month {}", m);
        }
    }
}
