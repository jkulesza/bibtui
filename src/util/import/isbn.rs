use indexmap::IndexMap;

use super::fetcher::Fetcher;
use super::{ImportedEntry, ImportError};

/// Fetches BibTeX metadata for books from the OpenLibrary Books API using an ISBN.
///
/// Accepts ISBN-10 and ISBN-13 in any common notation: bare digits, digits with
/// hyphens (`978-0-374-52837-9`), digits with spaces, or digits mixed with hyphens
/// and spaces.  The check digit for ISBN-10 may be `X` (uppercase or lowercase).
pub struct IsbnFetcher;

impl IsbnFetcher {
    /// Strip whitespace and hyphens and validate that the result is a well-formed
    /// ISBN-10 or ISBN-13.  Returns the normalised all-uppercase digits-only string
    /// (e.g. `"9780374528379"`) or `None` if the input is not an ISBN.
    pub fn normalize(input: &str) -> Option<String> {
        // Keep only alphanumeric characters, convert to upper-case.
        let s: String = input
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .collect::<String>()
            .to_uppercase();

        match s.len() {
            10 => {
                // ISBN-10: first 9 chars must be digits; last char digit or 'X'.
                let (body, check) = s.split_at(9);
                if body.chars().all(|c| c.is_ascii_digit())
                    && matches!(check, "X" | "0" | "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
                {
                    Some(s)
                } else {
                    None
                }
            }
            13 => {
                // ISBN-13: all digits, must start with 978 or 979.
                if s.chars().all(|c| c.is_ascii_digit())
                    && (s.starts_with("978") || s.starts_with("979"))
                {
                    Some(s)
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

impl Fetcher for IsbnFetcher {
    fn can_handle(&self, input: &str) -> bool {
        Self::normalize(input).is_some()
    }

    fn fetch(&self, input: &str) -> Result<ImportedEntry, ImportError> {
        let isbn = Self::normalize(input)
            .ok_or_else(|| ImportError::NoMatch(input.to_string()))?;

        let url = format!(
            "https://openlibrary.org/api/books?bibkeys=ISBN:{}&format=json&jscmd=data",
            isbn
        );

        let response = ureq::get(&url)
            .set("User-Agent", "bibtui/0.1 (https://github.com/jkulesza/bibtui)")
            .call()
            .map_err(|e| ImportError::Network(e.to_string()))?;

        let json: serde_json::Value = response
            .into_json()
            .map_err(|e| ImportError::Parse(e.to_string()))?;

        let key = format!("ISBN:{}", isbn);
        let book = json
            .get(&key)
            .ok_or_else(|| ImportError::Parse(format!("No OpenLibrary record found for ISBN {}", isbn)))?;

        parse_openlibrary_book(book, &isbn)
    }
}

fn parse_openlibrary_book(book: &serde_json::Value, isbn: &str) -> Result<ImportedEntry, ImportError> {
    let mut fields: IndexMap<String, String> = IndexMap::new();

    // Title
    let title = book["title"]
        .as_str()
        .ok_or_else(|| ImportError::Parse("OpenLibrary response missing 'title'".to_string()))?;
    fields.insert("title".to_string(), title.to_string());

    // Authors: [{"name": "...", "url": "..."}, ...]
    let author_str = build_author_string(book);
    if !author_str.is_empty() {
        fields.insert("author".to_string(), author_str);
    }

    // Year: publish_date may be "2011", "October 25, 2011", "2011-10-25", etc.
    if let Some(year) = extract_year(book) {
        fields.insert("year".to_string(), year);
    }

    // Publisher: publishers[0].name
    if let Some(publisher) = book["publishers"]
        .as_array()
        .and_then(|a| a.first())
        .and_then(|p| p["name"].as_str())
    {
        fields.insert("publisher".to_string(), publisher.to_string());
    }

    // Place of publication: publish_places[0].name (used as "address" in BibTeX)
    if let Some(place) = book["publish_places"]
        .as_array()
        .and_then(|a| a.first())
        .and_then(|p| p["name"].as_str())
    {
        if !place.is_empty() {
            fields.insert("address".to_string(), place.to_string());
        }
    }

    // Number of pages
    if let Some(pages) = book["number_of_pages"].as_u64() {
        fields.insert("pages".to_string(), pages.to_string());
    }

    // ISBN: store the canonical ISBN-13 when available, else ISBN-10.
    // Pull from identifiers first (more reliable than the query key).
    let isbn_stored = book["identifiers"]["isbn_13"]
        .as_array()
        .and_then(|a| a.first())
        .and_then(|v| v.as_str())
        .or_else(|| {
            book["identifiers"]["isbn_10"]
                .as_array()
                .and_then(|a| a.first())
                .and_then(|v| v.as_str())
        })
        .unwrap_or(isbn);
    fields.insert("isbn".to_string(), isbn_stored.to_string());

    // LCCN
    if let Some(lccn) = book["identifiers"]["lccn"]
        .as_array()
        .and_then(|a| a.first())
        .and_then(|v| v.as_str())
    {
        fields.insert("lccn".to_string(), lccn.to_string());
    }

    // Edition
    if let Some(ed) = book["edition_name"].as_str() {
        if !ed.is_empty() {
            fields.insert("edition".to_string(), ed.to_string());
        }
    }

    // OpenLibrary URL as the book URL
    if let Some(ol_id) = book["identifiers"]["openlibrary"]
        .as_array()
        .and_then(|a| a.first())
        .and_then(|v| v.as_str())
    {
        fields.insert(
            "url".to_string(),
            format!("https://openlibrary.org/books/{}", ol_id),
        );
    }

    Ok(ImportedEntry::new("book", fields))
}

fn build_author_string(book: &serde_json::Value) -> String {
    let Some(authors) = book["authors"].as_array() else {
        return String::new();
    };
    authors
        .iter()
        .filter_map(|a| a["name"].as_str())
        .map(|name| {
            // OpenLibrary gives "First Last"; try to convert to "Last, First" for BibTeX.
            // Simple heuristic: last whitespace-delimited token is the family name.
            // Handles compound surnames poorly, but is consistent with what Crossref does.
            let parts: Vec<&str> = name.splitn(2, ' ').collect();
            if parts.len() == 2 {
                format!("{}, {}", parts[1], parts[0])
            } else {
                name.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" and ")
}

fn extract_year(book: &serde_json::Value) -> Option<String> {
    let date = book["publish_date"].as_str()?;
    // Find the first run of 4 consecutive digits — handles all common formats.
    let bytes = date.as_bytes();
    for i in 0..bytes.len().saturating_sub(3) {
        if bytes[i..i + 4].iter().all(|b| b.is_ascii_digit()) {
            let year: u32 = std::str::from_utf8(&bytes[i..i + 4])
                .ok()?
                .parse()
                .ok()?;
            if (1000..=2999).contains(&year) {
                return Some(year.to_string());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── normalize ─────────────────────────────────────────────────────────────

    #[test]
    fn test_normalize_isbn13_bare() {
        assert_eq!(
            IsbnFetcher::normalize("9780374528379"),
            Some("9780374528379".to_string())
        );
    }

    #[test]
    fn test_normalize_isbn13_with_hyphens() {
        assert_eq!(
            IsbnFetcher::normalize("978-0-374-52837-9"),
            Some("9780374528379".to_string())
        );
    }

    #[test]
    fn test_normalize_isbn13_with_spaces() {
        assert_eq!(
            IsbnFetcher::normalize("978 0 374 52837 9"),
            Some("9780374528379".to_string())
        );
    }

    #[test]
    fn test_normalize_isbn13_mixed_dashes_spaces() {
        assert_eq!(
            IsbnFetcher::normalize("978-0 374-52837 9"),
            Some("9780374528379".to_string())
        );
    }

    #[test]
    fn test_normalize_isbn13_979_prefix() {
        assert_eq!(
            IsbnFetcher::normalize("9791032309421"),
            Some("9791032309421".to_string())
        );
    }

    #[test]
    fn test_normalize_isbn13_invalid_prefix() {
        // 13 digits but doesn't start with 978 or 979
        assert_eq!(IsbnFetcher::normalize("1234567890123"), None);
    }

    #[test]
    fn test_normalize_isbn10_bare() {
        assert_eq!(
            IsbnFetcher::normalize("0374528373"),
            Some("0374528373".to_string())
        );
    }

    #[test]
    fn test_normalize_isbn10_with_hyphens() {
        assert_eq!(
            IsbnFetcher::normalize("0-374-52837-3"),
            Some("0374528373".to_string())
        );
    }

    #[test]
    fn test_normalize_isbn10_check_x_uppercase() {
        assert_eq!(
            IsbnFetcher::normalize("019853453X"),
            Some("019853453X".to_string())
        );
    }

    #[test]
    fn test_normalize_isbn10_check_x_lowercase() {
        // Lowercase 'x' should be normalised to 'X'
        assert_eq!(
            IsbnFetcher::normalize("019853453x"),
            Some("019853453X".to_string())
        );
    }

    #[test]
    fn test_normalize_isbn10_with_hyphens_x() {
        assert_eq!(
            IsbnFetcher::normalize("0-19-853453-X"),
            Some("019853453X".to_string())
        );
    }

    #[test]
    fn test_normalize_too_short() {
        assert_eq!(IsbnFetcher::normalize("123456789"), None);
    }

    #[test]
    fn test_normalize_too_long() {
        assert_eq!(IsbnFetcher::normalize("97803745283790"), None);
    }

    #[test]
    fn test_normalize_empty() {
        assert_eq!(IsbnFetcher::normalize(""), None);
    }

    #[test]
    fn test_normalize_non_isbn_string() {
        assert_eq!(IsbnFetcher::normalize("not-an-isbn"), None);
    }

    #[test]
    fn test_normalize_doi_not_matched() {
        assert_eq!(IsbnFetcher::normalize("10.1234/foo"), None);
    }

    #[test]
    fn test_normalize_url_not_matched() {
        assert_eq!(IsbnFetcher::normalize("https://example.com"), None);
    }

    #[test]
    fn test_normalize_isbn10_non_digit_body() {
        // 'X' only valid as the check digit (last position), not elsewhere
        assert_eq!(IsbnFetcher::normalize("019X53453X"), None);
    }

    // ── can_handle ────────────────────────────────────────────────────────────

    #[test]
    fn test_can_handle_isbn13() {
        assert!(IsbnFetcher.can_handle("9780374528379"));
    }

    #[test]
    fn test_can_handle_isbn13_hyphens() {
        assert!(IsbnFetcher.can_handle("978-0-374-52837-9"));
    }

    #[test]
    fn test_can_handle_isbn10() {
        assert!(IsbnFetcher.can_handle("0374528373"));
    }

    #[test]
    fn test_can_handle_doi_not_matched() {
        assert!(!IsbnFetcher.can_handle("10.1234/foo"));
    }

    #[test]
    fn test_can_handle_url_not_matched() {
        assert!(!IsbnFetcher.can_handle("https://www.ans.org/article/123"));
    }

    #[test]
    fn test_can_handle_random_string_not_matched() {
        assert!(!IsbnFetcher.can_handle("Smith2020"));
    }

    // ── extract_year ─────────────────────────────────────────────────────────

    #[test]
    fn test_extract_year_bare_year() {
        let book = serde_json::json!({"publish_date": "2011"});
        assert_eq!(extract_year(&book), Some("2011".to_string()));
    }

    #[test]
    fn test_extract_year_full_date() {
        let book = serde_json::json!({"publish_date": "October 25, 2011"});
        assert_eq!(extract_year(&book), Some("2011".to_string()));
    }

    #[test]
    fn test_extract_year_iso_date() {
        let book = serde_json::json!({"publish_date": "2011-10-25"});
        assert_eq!(extract_year(&book), Some("2011".to_string()));
    }

    #[test]
    fn test_extract_year_missing() {
        let book = serde_json::json!({});
        assert_eq!(extract_year(&book), None);
    }

    #[test]
    fn test_extract_year_no_valid_year() {
        let book = serde_json::json!({"publish_date": "Unknown"});
        assert_eq!(extract_year(&book), None);
    }

    // ── build_author_string ───────────────────────────────────────────────────

    #[test]
    fn test_build_author_two_names() {
        let book = serde_json::json!({
            "authors": [
                {"name": "Daniel Kahneman"},
                {"name": "Amos Tversky"}
            ]
        });
        assert_eq!(build_author_string(&book), "Kahneman, Daniel and Tversky, Amos");
    }

    #[test]
    fn test_build_author_single_name_only() {
        let book = serde_json::json!({"authors": [{"name": "Aristotle"}]});
        assert_eq!(build_author_string(&book), "Aristotle");
    }

    #[test]
    fn test_build_author_empty_array() {
        let book = serde_json::json!({"authors": []});
        assert_eq!(build_author_string(&book), "");
    }

    #[test]
    fn test_build_author_missing_field() {
        let book = serde_json::json!({});
        assert_eq!(build_author_string(&book), "");
    }

    // ── parse_openlibrary_book ────────────────────────────────────────────────

    #[test]
    fn test_parse_full_record() {
        let book = serde_json::json!({
            "title": "Thinking, Fast and Slow",
            "authors": [{"name": "Daniel Kahneman"}],
            "publish_date": "2011",
            "publishers": [{"name": "Farrar, Straus and Giroux"}],
            "publish_places": [{"name": "New York"}],
            "number_of_pages": 499,
            "identifiers": {
                "isbn_13": ["9780374528379"],
                "isbn_10": ["0374528373"],
                "lccn": ["2011010169"],
                "openlibrary": ["OL24916873M"]
            }
        });
        let entry = parse_openlibrary_book(&book, "9780374528379").unwrap();
        assert_eq!(entry.entry_type, "book");
        assert_eq!(entry.fields["title"], "Thinking, Fast and Slow");
        assert_eq!(entry.fields["author"], "Kahneman, Daniel");
        assert_eq!(entry.fields["year"], "2011");
        assert_eq!(entry.fields["publisher"], "Farrar, Straus and Giroux");
        assert_eq!(entry.fields["address"], "New York");
        assert_eq!(entry.fields["pages"], "499");
        assert_eq!(entry.fields["isbn"], "9780374528379");
        assert_eq!(entry.fields["lccn"], "2011010169");
        assert_eq!(entry.fields["url"], "https://openlibrary.org/books/OL24916873M");
    }

    #[test]
    fn test_parse_isbn10_fallback_when_no_isbn13() {
        let book = serde_json::json!({
            "title": "Some Book",
            "identifiers": {"isbn_10": ["0374528373"]}
        });
        let entry = parse_openlibrary_book(&book, "0374528373").unwrap();
        assert_eq!(entry.fields["isbn"], "0374528373");
    }

    #[test]
    fn test_parse_uses_query_isbn_when_no_identifiers() {
        let book = serde_json::json!({"title": "Some Book"});
        let entry = parse_openlibrary_book(&book, "9780374528379").unwrap();
        assert_eq!(entry.fields["isbn"], "9780374528379");
    }

    #[test]
    fn test_parse_missing_title_returns_error() {
        let book = serde_json::json!({"authors": [{"name": "Someone"}]});
        assert!(parse_openlibrary_book(&book, "9780000000000").is_err());
    }

    #[test]
    fn test_parse_optional_fields_absent_no_panic() {
        let book = serde_json::json!({"title": "Minimal"});
        let entry = parse_openlibrary_book(&book, "9780000000000").unwrap();
        assert!(!entry.fields.contains_key("author"));
        assert!(!entry.fields.contains_key("year"));
        assert!(!entry.fields.contains_key("publisher"));
        assert!(!entry.fields.contains_key("address"));
        assert!(!entry.fields.contains_key("pages"));
    }

    #[test]
    fn test_parse_edition_field() {
        let book = serde_json::json!({
            "title": "Some Book",
            "edition_name": "3rd edition",
            "identifiers": {}
        });
        let entry = parse_openlibrary_book(&book, "9780000000001").unwrap();
        assert_eq!(entry.fields["edition"], "3rd edition");
    }

    #[test]
    fn test_parse_empty_edition_not_stored() {
        let book = serde_json::json!({
            "title": "Some Book",
            "edition_name": "",
            "identifiers": {}
        });
        let entry = parse_openlibrary_book(&book, "9780000000001").unwrap();
        assert!(!entry.fields.contains_key("edition"));
    }
}
