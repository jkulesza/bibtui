//! Syntax checks for values about to be written, and a reload check for
//! serialized output, so an edit can never be "saved" into a file that does
//! not parse back to the same entries.

use std::collections::HashSet;

use super::model::Database;
use super::parser::{build_database, parse_bib_file};

/// Why a field value cannot be written inside `{...}`, or `None` if it can.
///
/// Mirrors the parser: braces must balance, and a backslash escapes the next
/// character (so `\{` and `\}` are literal and a trailing `\` would escape
/// the closing brace).
pub fn value_syntax_error(value: &str) -> Option<String> {
    let mut depth = 0usize;
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => {
                if chars.next().is_none() {
                    return Some("ends with an unescaped backslash".into());
                }
            }
            '{' => depth += 1,
            '}' => {
                if depth == 0 {
                    return Some("has a '}' without a matching '{'".into());
                }
                depth -= 1;
            }
            _ => {}
        }
    }
    match depth {
        0 => None,
        1 => Some("has a '{' that is never closed".into()),
        n => Some(format!("has {n} '{{' that are never closed")),
    }
}

/// Why `name` cannot be a field name, or `None` if it can. Field names are
/// letters, digits, `_` and `-`, as the parser reads them.
pub fn field_name_error(name: &str) -> Option<String> {
    if name.is_empty() {
        return Some("is empty".into());
    }
    match name
        .chars()
        .find(|&c| !(c.is_alphanumeric() || c == '_' || c == '-'))
    {
        Some(c) if c.is_whitespace() => Some("contains whitespace".into()),
        Some(c) => Some(format!("contains '{c}'")),
        None => None,
    }
}

/// Check that `output` parses back to exactly the entries of `expected`
/// (same keys, same fields and values). Malformed text the parser keeps as
/// opaque passthrough is not an entry on either side, so it stays
/// preservable; anything an edit newly breaks is reported.
pub fn verify_reload(output: &str, expected: &Database) -> Result<(), String> {
    let raw = parse_bib_file(output).map_err(|e| format!("output does not parse: {e}"))?;
    let reloaded = build_database(raw);
    for (key, entry) in &expected.entries {
        let Some(again) = reloaded.entries.get(key) else {
            return Err(format!("entry '{key}' would be missing after reload"));
        };
        for (name, value) in &entry.fields {
            match again.fields.get(name) {
                Some(v) if v == value => {}
                // Month abbreviations are written as bare lowercase macros.
                Some(v) if name == "month" && v.eq_ignore_ascii_case(value) => {}
                Some(_) => {
                    return Err(format!(
                        "field '{name}' of '{key}' would reload with a different value"
                    ))
                }
                None => {
                    return Err(format!(
                        "field '{name}' of '{key}' would be missing after reload"
                    ))
                }
            }
        }
        if let Some(extra) = again
            .fields
            .keys()
            .find(|name| !entry.fields.contains_key(*name))
        {
            return Err(format!(
                "entry '{key}' would reload with an unexpected field '{extra}'"
            ));
        }
    }
    let expected_keys: HashSet<&String> = expected.entries.keys().collect();
    if let Some(extra) = reloaded
        .entries
        .keys()
        .find(|key| !expected_keys.contains(key))
    {
        return Err(format!(
            "output would contain an unexpected entry '{extra}'"
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn balanced_and_escaped_values_are_accepted() {
        for ok in [
            "",
            "Plain",
            "{Protected} Title",
            "{A {nested} group}",
            r"50\% of \{literal\} braces",
            r#"G{\"o}del"#,
            r"\\",
        ] {
            assert_eq!(value_syntax_error(ok), None, "{ok:?}");
        }
    }

    #[test]
    fn unbalanced_values_and_trailing_escape_are_rejected() {
        assert!(value_syntax_error("Unclosed {")
            .unwrap()
            .contains("never closed"));
        assert!(value_syntax_error("{{a}").unwrap().contains("never closed"));
        assert!(value_syntax_error("a}")
            .unwrap()
            .contains("without a matching"));
        assert!(value_syntax_error("}{")
            .unwrap()
            .contains("without a matching"));
        assert!(value_syntax_error(r"ends\").unwrap().contains("backslash"));
        assert!(value_syntax_error(r"{\}").unwrap().contains("never closed"));
    }

    #[test]
    fn field_names_follow_the_parser_grammar() {
        for ok in ["title", "Journal-Abbrev", "my_field2", "année"] {
            assert_eq!(field_name_error(ok), None, "{ok:?}");
        }
        assert_eq!(field_name_error(""), Some("is empty".into()));
        assert_eq!(
            field_name_error("two words"),
            Some("contains whitespace".into())
        );
        assert_eq!(field_name_error("a=b"), Some("contains '='".into()));
        assert_eq!(field_name_error("x,y"), Some("contains ','".into()));
        assert_eq!(field_name_error("{t}"), Some("contains '{'".into()));
    }

    fn db(src: &str) -> Database {
        build_database(parse_bib_file(src).unwrap())
    }

    #[test]
    fn verify_reload_accepts_identical_output_and_opaque_text() {
        let src = "@Misc{A, title={T}, month=apr}\n@Misc(broken\n@Misc{B, title={U}}\n";
        let expected = db(src);
        assert_eq!(verify_reload(src, &expected), Ok(()));
    }

    #[test]
    fn verify_reload_reports_missing_and_changed_entries() {
        let expected = db("@Misc{A, title={T}}\n@Misc{B, title={U}}\n");
        let err = verify_reload("@Misc{A, title={T}}\n", &expected).unwrap_err();
        assert!(err.contains("'B' would be missing"), "{err}");
        let err =
            verify_reload("@Misc{A, title={X}}\n@Misc{B, title={U}}\n", &expected).unwrap_err();
        assert!(err.contains("field 'title' of 'A'"), "{err}");
        let err = verify_reload(
            "@Misc{A, title={T}}\n@Misc{B, title={U}}\n@Misc{C, title={V}}\n",
            &expected,
        )
        .unwrap_err();
        assert!(err.contains("unexpected entry 'C'"), "{err}");
    }

    #[test]
    fn verify_reload_tolerates_bare_month_case() {
        let mut expected = db("@Misc{A, month=apr}\n");
        expected
            .entries
            .get_mut("A")
            .unwrap()
            .fields
            .insert("month".into(), "Apr".into());
        assert_eq!(verify_reload("@Misc{A, month=apr}\n", &expected), Ok(()));
    }
}
