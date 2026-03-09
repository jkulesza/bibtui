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
            RawItem::BibPreamble(content) => {
                out.push_str("@Preamble{");
                out.push_str(content);
                out.push('}');
            }
            RawItem::StringDef { name, raw_value } => {
                out.push_str(&format!("@String{{{} = {}}}", name, raw_value));
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

/// Serialize a single entry from semantic data (for modified entries).
pub fn serialize_entry(entry: &Entry, align: bool) -> String {
    let mut out = String::new();

    out.push_str(&format!(
        "@{}{{{},\n",
        entry.entry_type.display_name(),
        entry.citation_key
    ));

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

    for (key, value) in entry.fields.iter() {
        let padding = if align && key.len() < align_width {
            " ".repeat(align_width - key.len())
        } else {
            String::new()
        };

        // Determine how to format the value
        let formatted_value = format_field_value(key, value);

        out.push_str(&format!(
            "  {}{} = {},\n",
            key, padding, formatted_value
        ));
    }

    out.push('}');
    out
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
        let result = serialize_entry(&entry, false);
        assert!(result.starts_with("@Article{Smith2020,"), "result: {}", result);
        assert!(result.contains("year = {2020}"), "result: {}", result);
        assert!(result.contains("title = {A Test}"), "result: {}", result);
    }

    #[test]
    fn test_serialize_entry_align() {
        let entry = make_test_entry();
        let result = serialize_entry(&entry, true);
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
                align_width: 0,
                trailing_comma: false,
                raw_text: raw_text.clone(),
            })],
        };
        let result = write_bib_file(&raw);
        assert_eq!(result, raw_text);
    }
}
