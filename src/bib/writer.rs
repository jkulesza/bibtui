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
fn format_field_value(field_name: &str, value: &str) -> String {
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
