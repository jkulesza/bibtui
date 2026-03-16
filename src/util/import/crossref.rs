use indexmap::IndexMap;

use super::fetcher::Fetcher;
use super::{ImportedEntry, ImportError};

/// Fetches BibTeX metadata from the Crossref public API.
///
/// Handles bare DOIs (`10.xxxx/...`) and `https://doi.org/...` URLs.
pub struct CrossrefFetcher;

impl CrossrefFetcher {
    /// Extract a bare DOI from input that may be a URL or raw DOI.
    pub fn extract_doi(input: &str) -> Option<String> {
        let input = input.trim();
        // https://doi.org/10.xxx or http://dx.doi.org/10.xxx
        if let Some(rest) = input
            .strip_prefix("https://doi.org/")
            .or_else(|| input.strip_prefix("http://doi.org/"))
            .or_else(|| input.strip_prefix("https://dx.doi.org/"))
            .or_else(|| input.strip_prefix("http://dx.doi.org/"))
        {
            return Some(rest.to_string());
        }
        // Bare DOI: starts with "10."
        if input.starts_with("10.") {
            return Some(input.to_string());
        }
        None
    }
}

impl Fetcher for CrossrefFetcher {
    fn can_handle(&self, doi_or_url: &str) -> bool {
        Self::extract_doi(doi_or_url).is_some()
    }

    fn fetch(&self, doi_or_url: &str) -> Result<ImportedEntry, ImportError> {
        let doi = Self::extract_doi(doi_or_url)
            .ok_or_else(|| ImportError::NoMatch(doi_or_url.to_string()))?;

        let url = format!("https://api.crossref.org/works/{}", doi);
        let response = ureq::get(&url)
            .set(
                "User-Agent",
                "bibtui/0.1 (https://github.com/jkulesza/bibtui; mailto:user@example.com)",
            )
            .call()
            .map_err(|e| ImportError::Network(e.to_string()))?;

        let json: serde_json::Value = response
            .into_json()
            .map_err(|e| ImportError::Parse(e.to_string()))?;

        let work = json
            .get("message")
            .ok_or_else(|| ImportError::Parse("missing 'message' key".to_string()))?;

        parse_crossref_work(work, &doi)
    }
}

/// Search Crossref for a DOI matching the given metadata.
///
/// Returns `(doi, url)` for the best-scoring result, or an error string if
/// nothing useful was found.
pub fn search_by_metadata(
    title: &str,
    author: &str,
    year: &str,
) -> Result<(String, String), String> {
    if title.trim().is_empty() && author.trim().is_empty() {
        return Err("Need at least a title or author to search".to_string());
    }

    // Build bibliographic query: combine all available metadata
    let bib_query = [title, author, year]
        .iter()
        .filter(|s| !s.trim().is_empty())
        .cloned()
        .collect::<Vec<_>>()
        .join(" ");

    let mut url = format!(
        "https://api.crossref.org/works?query.bibliographic={}&rows=3&select=DOI,URL,score,title,author,published-print,published-online,issued",
        urlencoding_simple(&bib_query)
    );

    // Add structured filters when we have them — improves precision
    if !author.trim().is_empty() {
        url.push_str(&format!("&query.author={}", urlencoding_simple(author)));
    }

    let response = ureq::get(&url)
        .set(
            "User-Agent",
            "bibtui/0.1 (https://github.com/jkulesza/bibtui; mailto:user@example.com)",
        )
        .call()
        .map_err(|e| format!("Network error: {}", e))?;

    let json: serde_json::Value = response
        .into_json()
        .map_err(|e| format!("Parse error: {}", e))?;

    let items = json["message"]["items"]
        .as_array()
        .ok_or_else(|| "Unexpected response format".to_string())?;

    if items.is_empty() {
        return Err("No results found".to_string());
    }

    // Pick the highest-scoring result
    let best = items
        .iter()
        .max_by(|a, b| {
            let sa = a["score"].as_f64().unwrap_or(0.0);
            let sb = b["score"].as_f64().unwrap_or(0.0);
            sa.partial_cmp(&sb).unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap(); // items is non-empty

    let doi = best["DOI"]
        .as_str()
        .ok_or_else(|| "Result has no DOI".to_string())?
        .to_string();

    let url_field = best["URL"]
        .as_str()
        .unwrap_or("")
        .to_string();

    Ok((doi, url_field))
}

/// Minimal percent-encoding for URL query parameter values.
/// Encodes spaces as `+` and escapes characters not safe in query strings.
fn urlencoding_simple(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9'
            | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push('+'),
            _ => {
                out.push('%');
                out.push_str(&format!("{:02X}", b));
            }
        }
    }
    out
}

fn parse_crossref_work(
    work: &serde_json::Value,
    doi: &str,
) -> Result<ImportedEntry, ImportError> {
    let mut fields: IndexMap<String, String> = IndexMap::new();

    // DOI
    fields.insert("doi".to_string(), doi.to_string());

    // Title
    if let Some(title) = work["title"].as_array().and_then(|a| a.first()).and_then(|v| v.as_str()) {
        fields.insert("title".to_string(), title.to_string());
    }

    // Authors
    let author_str = build_author_string(work);
    if !author_str.is_empty() {
        fields.insert("author".to_string(), author_str);
    }

    // Year
    if let Some(year) = extract_year(work) {
        fields.insert("year".to_string(), year);
    }

    // Journal / container
    if let Some(container) = work["container-title"]
        .as_array()
        .and_then(|a| a.first())
        .and_then(|v| v.as_str())
    {
        if !container.is_empty() {
            fields.insert("journal".to_string(), container.to_string());
        }
    }

    // Volume
    if let Some(v) = work["volume"].as_str() {
        fields.insert("volume".to_string(), v.to_string());
    }

    // Issue / number
    if let Some(n) = work["issue"].as_str() {
        fields.insert("number".to_string(), n.to_string());
    }

    // Pages
    if let Some(p) = work["page"].as_str() {
        // Normalize any run of hyphens/en-dashes to BibTeX "--"
        let mut pages = String::with_capacity(p.len() + 2);
        let mut in_dash = false;
        for c in p.chars() {
            if c == '-' || c == '\u{2013}' {
                if !in_dash {
                    pages.push_str("--");
                    in_dash = true;
                }
            } else {
                in_dash = false;
                pages.push(c);
            }
        }
        fields.insert("pages".to_string(), pages);
    }

    // Publisher
    if let Some(pub_) = work["publisher"].as_str() {
        fields.insert("publisher".to_string(), pub_.to_string());
    }

    // ISSN
    if let Some(issn_arr) = work["ISSN"].as_array() {
        if let Some(issn) = issn_arr.first().and_then(|v| v.as_str()) {
            fields.insert("issn".to_string(), issn.to_string());
        }
    }

    // URL
    if let Some(url) = work["URL"].as_str() {
        fields.insert("url".to_string(), url.to_string());
    }

    // Abstract
    if let Some(abstract_) = work["abstract"].as_str() {
        // Strip jats XML tags if present
        let clean = strip_jats(abstract_);
        if !clean.is_empty() {
            fields.insert("abstract".to_string(), clean);
        }
    }

    // Determine entry type from Crossref type field
    let entry_type = crossref_type_to_bibtex(work["type"].as_str().unwrap_or(""));

    Ok(ImportedEntry::new(entry_type, fields))
}

fn build_author_string(work: &serde_json::Value) -> String {
    let authors = work["author"].as_array();
    let Some(authors) = authors else { return String::new() };

    authors
        .iter()
        .filter_map(|a| {
            let family = a["family"].as_str()?;
            let given = a["given"].as_str().unwrap_or("");
            Some(if given.is_empty() {
                family.to_string()
            } else {
                format!("{}, {}", family, given)
            })
        })
        .collect::<Vec<_>>()
        .join(" and ")
}

fn extract_year(work: &serde_json::Value) -> Option<String> {
    // Try published-print, then published-online, then issued
    for key in &["published-print", "published-online", "issued"] {
        if let Some(year) = work[key]["date-parts"]
            .as_array()
            .and_then(|a| a.first())
            .and_then(|a| a.as_array())
            .and_then(|a| a.first())
            .and_then(|v| v.as_u64())
        {
            return Some(year.to_string());
        }
    }
    None
}

fn crossref_type_to_bibtex(crossref_type: &str) -> String {
    match crossref_type {
        "journal-article" => "article",
        "book" => "book",
        "book-chapter" => "inbook",
        "proceedings-article" => "inproceedings",
        "report" => "techreport",
        "dissertation" => "phdthesis",
        "dataset" | "other" | "" => "misc",
        _ => "misc",
    }
    .to_string()
}

/// Remove JATS XML tags (e.g. `<jats:p>`, `</jats:p>`) from abstract text.
fn strip_jats(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_urlencoding_simple_plain() {
        assert_eq!(urlencoding_simple("hello"), "hello");
    }

    #[test]
    fn test_urlencoding_simple_spaces() {
        assert_eq!(urlencoding_simple("hello world"), "hello+world");
    }

    #[test]
    fn test_urlencoding_simple_special_chars() {
        let result = urlencoding_simple("a&b=c");
        assert!(result.contains('%'), "result: {}", result);
        assert!(!result.contains('&'), "result: {}", result);
    }

    #[test]
    fn test_urlencoding_simple_alphanumeric_unchanged() {
        let s = "ABCabc123-_.~";
        assert_eq!(urlencoding_simple(s), s);
    }

    #[test]
    fn test_urlencoding_simple_empty() {
        assert_eq!(urlencoding_simple(""), "");
    }

    #[test]
    fn test_extract_doi_bare() {
        assert_eq!(
            CrossrefFetcher::extract_doi("10.1016/j.anucene.2020.107650"),
            Some("10.1016/j.anucene.2020.107650".to_string())
        );
    }

    #[test]
    fn test_extract_doi_https_doi_org() {
        assert_eq!(
            CrossrefFetcher::extract_doi("https://doi.org/10.1016/j.anucene.2020.107650"),
            Some("10.1016/j.anucene.2020.107650".to_string())
        );
    }

    #[test]
    fn test_extract_doi_dx_doi_org() {
        assert_eq!(
            CrossrefFetcher::extract_doi("http://dx.doi.org/10.1016/j.anucene.2020.107650"),
            Some("10.1016/j.anucene.2020.107650".to_string())
        );
    }

    #[test]
    fn test_extract_doi_non_doi_url() {
        assert_eq!(
            CrossrefFetcher::extract_doi("https://www.ans.org/pubs/journals/nt/article-1234/"),
            None
        );
    }

    #[test]
    fn test_extract_doi_empty() {
        assert_eq!(CrossrefFetcher::extract_doi(""), None);
    }

    #[test]
    fn test_crossref_type_to_bibtex() {
        assert_eq!(crossref_type_to_bibtex("journal-article"), "article");
        assert_eq!(crossref_type_to_bibtex("book"), "book");
        assert_eq!(crossref_type_to_bibtex("proceedings-article"), "inproceedings");
        assert_eq!(crossref_type_to_bibtex("unknown-type"), "misc");
        assert_eq!(crossref_type_to_bibtex(""), "misc");
    }

    #[test]
    fn test_strip_jats() {
        assert_eq!(
            strip_jats("<jats:p>Hello <jats:italic>world</jats:italic>.</jats:p>"),
            "Hello world."
        );
        assert_eq!(strip_jats("Plain text"), "Plain text");
        assert_eq!(strip_jats(""), "");
    }

    #[test]
    fn test_can_handle() {
        let f = CrossrefFetcher;
        assert!(f.can_handle("10.1016/j.anucene.2020.107650"));
        assert!(f.can_handle("https://doi.org/10.1016/j.anucene.2020.107650"));
        assert!(!f.can_handle("https://www.ans.org/pubs/journals/nt/article-1234/"));
    }

    #[test]
    fn test_extract_doi_http_doi_org() {
        assert_eq!(
            CrossrefFetcher::extract_doi("http://doi.org/10.1234/foo"),
            Some("10.1234/foo".to_string())
        );
    }

    #[test]
    fn test_extract_doi_https_dx_doi_org() {
        assert_eq!(
            CrossrefFetcher::extract_doi("https://dx.doi.org/10.1234/foo"),
            Some("10.1234/foo".to_string())
        );
    }

    #[test]
    fn test_extract_doi_trims_whitespace() {
        assert_eq!(
            CrossrefFetcher::extract_doi("  10.1234/foo  "),
            Some("10.1234/foo".to_string())
        );
    }

    #[test]
    fn test_crossref_type_all_variants() {
        assert_eq!(crossref_type_to_bibtex("book-chapter"), "inbook");
        assert_eq!(crossref_type_to_bibtex("report"), "techreport");
        assert_eq!(crossref_type_to_bibtex("dissertation"), "phdthesis");
        assert_eq!(crossref_type_to_bibtex("dataset"), "misc");
        assert_eq!(crossref_type_to_bibtex("other"), "misc");
    }

    #[test]
    fn test_build_author_string_single() {
        let work = serde_json::json!({"author": [{"family": "Smith", "given": "John"}]});
        assert_eq!(build_author_string(&work), "Smith, John");
    }

    #[test]
    fn test_build_author_string_multiple() {
        let work = serde_json::json!({"author": [
            {"family": "Smith", "given": "John"},
            {"family": "Doe", "given": "Jane"}
        ]});
        assert_eq!(build_author_string(&work), "Smith, John and Doe, Jane");
    }

    #[test]
    fn test_build_author_string_no_given_name() {
        let work = serde_json::json!({"author": [{"family": "Collaboration"}]});
        assert_eq!(build_author_string(&work), "Collaboration");
    }

    #[test]
    fn test_build_author_string_empty_given() {
        let work = serde_json::json!({"author": [{"family": "Smith", "given": ""}]});
        assert_eq!(build_author_string(&work), "Smith");
    }

    #[test]
    fn test_build_author_string_no_author_field() {
        let work = serde_json::json!({});
        assert_eq!(build_author_string(&work), "");
    }

    #[test]
    fn test_extract_year_published_print() {
        let work = serde_json::json!({"published-print": {"date-parts": [[2020, 3, 15]]}});
        assert_eq!(extract_year(&work), Some("2020".to_string()));
    }

    #[test]
    fn test_extract_year_prefers_print_over_online() {
        let work = serde_json::json!({
            "published-print":  {"date-parts": [[2020]]},
            "published-online": {"date-parts": [[2019]]}
        });
        assert_eq!(extract_year(&work), Some("2020".to_string()));
    }

    #[test]
    fn test_extract_year_online_fallback() {
        let work = serde_json::json!({"published-online": {"date-parts": [[2021]]}});
        assert_eq!(extract_year(&work), Some("2021".to_string()));
    }

    #[test]
    fn test_extract_year_issued_fallback() {
        let work = serde_json::json!({"issued": {"date-parts": [[2022]]}});
        assert_eq!(extract_year(&work), Some("2022".to_string()));
    }

    #[test]
    fn test_extract_year_missing() {
        let work = serde_json::json!({});
        assert_eq!(extract_year(&work), None);
    }

    #[test]
    fn test_parse_crossref_work_full_article() {
        let work = serde_json::json!({
            "type": "journal-article",
            "title": ["A Test Article"],
            "author": [{"family": "Smith", "given": "J."}],
            "published-print": {"date-parts": [[2020]]},
            "container-title": ["Test Journal"],
            "volume": "10",
            "issue": "2",
            "page": "100-110",
            "publisher": "Test Publisher",
            "ISSN": ["1234-5678"],
            "URL": "https://doi.org/10.1234/test"
        });
        let entry = parse_crossref_work(&work, "10.1234/test").unwrap();
        assert_eq!(entry.entry_type, "article");
        assert_eq!(entry.fields["doi"], "10.1234/test");
        assert_eq!(entry.fields["title"], "A Test Article");
        assert_eq!(entry.fields["author"], "Smith, J.");
        assert_eq!(entry.fields["year"], "2020");
        assert_eq!(entry.fields["journal"], "Test Journal");
        assert_eq!(entry.fields["volume"], "10");
        assert_eq!(entry.fields["number"], "2");
        assert_eq!(entry.fields["pages"], "100--110");
        assert_eq!(entry.fields["publisher"], "Test Publisher");
        assert_eq!(entry.fields["issn"], "1234-5678");
    }

    #[test]
    fn test_parse_crossref_work_pages_en_dash() {
        let work = serde_json::json!({"type": "journal-article", "page": "100\u{2013}110"});
        let entry = parse_crossref_work(&work, "10.1234/x").unwrap();
        assert_eq!(entry.fields["pages"], "100--110");
    }

    #[test]
    fn test_parse_crossref_work_abstract_strips_jats() {
        let work = serde_json::json!({
            "type": "journal-article",
            "abstract": "<jats:p>Hello <jats:italic>world</jats:italic>.</jats:p>"
        });
        let entry = parse_crossref_work(&work, "10.1234/x").unwrap();
        assert_eq!(entry.fields["abstract"], "Hello world.");
    }

    #[test]
    fn test_parse_crossref_work_empty_container_title_skipped() {
        // container-title: [""] should not add a journal field
        let work = serde_json::json!({"type": "journal-article", "container-title": [""]});
        let entry = parse_crossref_work(&work, "10.1234/x").unwrap();
        assert!(!entry.fields.contains_key("journal"));
    }

    #[test]
    fn test_strip_jats_nested() {
        assert_eq!(
            strip_jats("<jats:sec><jats:title>Intro</jats:title><jats:p>Text.</jats:p></jats:sec>"),
            "IntroText."
        );
    }

    // ── search_by_metadata early-return (no network) ──────────────────────────

    #[test]
    fn test_search_by_metadata_empty_title_and_author_returns_err() {
        let result = search_by_metadata("", "", "2020");
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(msg.contains("title") || msg.contains("author"), "msg: {}", msg);
    }

    #[test]
    fn test_search_by_metadata_whitespace_title_and_author_returns_err() {
        let result = search_by_metadata("   ", "\t", "");
        assert!(result.is_err());
    }
}
