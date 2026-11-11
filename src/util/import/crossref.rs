use indexmap::IndexMap;

use super::fetcher::Fetcher;
use super::http::HttpTransport;
use super::{ImportError, ImportedEntry};

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

    fn fetch_with(
        &self,
        doi_or_url: &str,
        http: &dyn HttpTransport,
    ) -> Result<ImportedEntry, ImportError> {
        let doi = Self::extract_doi(doi_or_url)
            .ok_or_else(|| ImportError::NoMatch(doi_or_url.to_string()))?;

        let url = format!("https://api.crossref.org/works/{}", doi);
        let json = http.get(&url, super::http::REQUEST_TIMEOUT)?.json()?;

        let work = json
            .get("message")
            .ok_or_else(|| ImportError::Parse("missing 'message' key".to_string()))?;

        parse_crossref_work(work, &doi)
    }
}

/// A Crossref work returned by a metadata search, with what is needed to
/// show it to the user and to compare it with the entry.
#[derive(Debug, Clone, PartialEq)]
pub struct DoiCandidate {
    pub doi: String,
    pub url: String,
    pub title: String,
    /// First author's family name (or organization name), as returned.
    pub first_author: Option<String>,
    pub year: Option<String>,
    surname_keys: Vec<String>,
    years: Vec<String>,
}

impl DoiCandidate {
    /// One line for a picker: title, first author, year and DOI.
    pub fn label(&self) -> String {
        let mut label = if self.title.is_empty() {
            "(untitled)".to_string()
        } else {
            self.title.clone()
        };
        match (&self.first_author, &self.year) {
            (Some(a), Some(y)) => label.push_str(&format!(" — {a} ({y})")),
            (Some(a), None) => label.push_str(&format!(" — {a}")),
            (None, Some(y)) => label.push_str(&format!(" ({y})")),
            (None, None) => {}
        }
        label.push_str(&format!(" — {}", self.doi));
        label
    }

    #[cfg(test)]
    pub fn for_test(doi: &str, url: &str) -> Self {
        Self {
            doi: doi.into(),
            url: url.into(),
            title: String::new(),
            first_author: None,
            year: None,
            surname_keys: Vec::new(),
            years: Vec::new(),
        }
    }
}

/// What a metadata search found.
#[derive(Debug, Clone, PartialEq)]
pub enum MetadataMatch {
    /// Exactly one work agrees with the entry strongly enough to assign its
    /// DOI without asking.
    Unique(DoiCandidate),
    /// Possible matches the user must choose between (or reject):
    /// `ambiguous` means several works match equally well (for example two
    /// editions), otherwise each is only a weak match.
    Choose {
        candidates: Vec<DoiCandidate>,
        ambiguous: bool,
    },
}

/// Title similarity at or above which two titles are taken to name the
/// same work; the same default the duplicate detector uses.
const TITLE_MATCH_THRESHOLD: f64 = 0.9;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Agreement {
    Agree,
    Disagree,
    Unknown,
}

/// How a candidate compares with the entry's title, first author and year.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Comparison {
    title_matches: bool,
    author: Agreement,
    year: Agreement,
}

impl Comparison {
    /// Strong enough to assign without asking: the title names the same
    /// work, the first author or the year confirms it, and neither
    /// contradicts it. Crossref's relevance score is deliberately not used:
    /// it ranks results but does not say whether any of them is the work.
    fn is_strong(self) -> bool {
        self.title_matches
            && (self.author == Agreement::Agree || self.year == Agreement::Agree)
            && self.author != Agreement::Disagree
            && self.year != Agreement::Disagree
    }

    /// Worth offering to the user: the title matches, or (when the entry
    /// has no title) both author and year agree.
    fn is_plausible(self, query_has_title: bool) -> bool {
        self.title_matches
            || (!query_has_title
                && self.author == Agreement::Agree
                && self.year == Agreement::Agree)
    }
}

struct Query {
    title: String,
    surname: Option<String>,
    year: Option<String>,
}

impl Query {
    fn new(title: &str, author: &str, year: &str) -> Self {
        let digits: String = year
            .chars()
            .filter(|c| c.is_ascii_digit())
            .take(4)
            .collect();
        Self {
            title: crate::bib::dedup::normalize_title(title),
            surname: crate::bib::dedup::first_surname_in(author),
            year: (digits.len() == 4).then_some(digits),
        }
    }

    fn compare(&self, c: &DoiCandidate, candidate_titles: &[String]) -> Comparison {
        let title_matches = !self.title.is_empty()
            && candidate_titles.iter().any(|t| {
                *t == self.title
                    || crate::bib::dedup::normalized_title_similarity(
                        t,
                        &self.title,
                        TITLE_MATCH_THRESHOLD,
                    )
                    .is_some()
            });
        let agreement = |mine: &Option<String>, theirs: &[String]| match mine {
            Some(_) if theirs.is_empty() => Agreement::Unknown,
            Some(v) if theirs.contains(v) => Agreement::Agree,
            Some(_) => Agreement::Disagree,
            None => Agreement::Unknown,
        };
        Comparison {
            title_matches,
            author: agreement(&self.surname, &c.surname_keys),
            year: agreement(&self.year, &c.years),
        }
    }
}

/// A search result as a candidate, plus its normalized titles (the title
/// alone and with its subtitle, since entries often store "Title: Subtitle").
fn candidate_from(item: &serde_json::Value) -> Option<(DoiCandidate, Vec<String>)> {
    let doi = item["DOI"].as_str()?.to_string();
    let first_str = |key: &str| {
        item[key]
            .as_array()
            .and_then(|a| a.first())
            .and_then(|v| v.as_str())
            .map(str::to_string)
    };
    let title = first_str("title").unwrap_or_default();
    let subtitle = first_str("subtitle");
    let mut titles = vec![crate::bib::dedup::normalize_title(&title)];
    if let Some(sub) = &subtitle {
        titles.push(crate::bib::dedup::normalize_title(&format!(
            "{title} {sub}"
        )));
    }
    titles.retain(|t| !t.is_empty());

    let names: Vec<&str> = item["author"]
        .as_array()
        .map(|authors| {
            authors
                .iter()
                .filter_map(|a| a["family"].as_str().or_else(|| a["name"].as_str()))
                .collect()
        })
        .unwrap_or_default();
    // Every date Crossref has: print and online years often differ by one.
    let mut years: Vec<String> = ["published-print", "published-online", "issued"]
        .iter()
        .filter_map(|key| {
            item[*key]["date-parts"]
                .as_array()
                .and_then(|a| a.first())
                .and_then(|a| a.as_array())
                .and_then(|a| a.first())
                .and_then(|v| v.as_u64())
                .map(|y| y.to_string())
        })
        .collect();
    years.dedup();
    let candidate = DoiCandidate {
        doi,
        url: item["URL"].as_str().unwrap_or("").to_string(),
        title,
        first_author: names.first().map(|n| n.to_string()),
        year: extract_year(item),
        surname_keys: names
            .iter()
            .filter_map(|n| crate::bib::dedup::surname_key(n))
            .collect(),
        years,
    };
    Some((candidate, titles))
}

/// Search Crossref for the work described by an entry's metadata.
pub fn search_by_metadata(title: &str, author: &str, year: &str) -> Result<MetadataMatch, String> {
    let client = super::http::HttpClient::new().map_err(|error| error.to_string())?;
    search_by_metadata_with(title, author, year, &client)
}

/// [`search_by_metadata`] over a given transport. Each result is compared
/// with the entry (see `Comparison::is_strong`); a lone strong match is
/// returned as [`MetadataMatch::Unique`], several strong or only weak
/// matches as [`MetadataMatch::Choose`], and no credible match as an error.
pub fn search_by_metadata_with(
    title: &str,
    author: &str,
    year: &str,
    http: &dyn HttpTransport,
) -> Result<MetadataMatch, String> {
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
        "https://api.crossref.org/works?query.bibliographic={}&rows=5&select=DOI,URL,score,title,subtitle,author,published-print,published-online,issued",
        urlencoding_simple(&bib_query)
    );

    // Add structured filters when we have them — improves precision
    if !author.trim().is_empty() {
        url.push_str(&format!("&query.author={}", urlencoding_simple(author)));
    }

    let json = http
        .get(&url, super::http::REQUEST_TIMEOUT)
        .and_then(|response| response.json())
        .map_err(|error| error.to_string())?;

    let items = json["message"]["items"]
        .as_array()
        .ok_or_else(|| "Unexpected response format".to_string())?;

    if items.is_empty() {
        return Err("No results found".to_string());
    }

    let query = Query::new(title, author, year);
    let query_has_title = !query.title.is_empty();
    let mut strong: Vec<DoiCandidate> = Vec::new();
    let mut plausible: Vec<DoiCandidate> = Vec::new();
    let mut closest: Option<DoiCandidate> = None;
    for item in items {
        let Some((candidate, titles)) = candidate_from(item) else {
            continue;
        };
        let comparison = query.compare(&candidate, &titles);
        let seen = |list: &[DoiCandidate]| {
            list.iter()
                .any(|c| c.doi.eq_ignore_ascii_case(&candidate.doi))
        };
        if comparison.is_strong() {
            if !seen(&strong) {
                strong.push(candidate);
            }
        } else if comparison.is_plausible(query_has_title) {
            if !seen(&plausible) {
                plausible.push(candidate);
            }
        } else if closest.is_none() {
            closest = Some(candidate);
        }
    }

    match strong.len() {
        1 => Ok(MetadataMatch::Unique(strong.remove(0))),
        0 if !plausible.is_empty() => Ok(MetadataMatch::Choose {
            candidates: plausible,
            ambiguous: false,
        }),
        0 => Err(match closest {
            Some(c) => format!("no matching work found (closest result: {})", c.label()),
            None => "no matching work found".to_string(),
        }),
        _ => Ok(MetadataMatch::Choose {
            candidates: strong,
            ambiguous: true,
        }),
    }
}

/// Minimal percent-encoding for URL query parameter values.
/// Encodes spaces as `+` and escapes characters not safe in query strings.
fn urlencoding_simple(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b' ' => out.push('+'),
            _ => {
                out.push('%');
                out.push_str(&format!("{:02X}", b));
            }
        }
    }
    out
}

fn parse_crossref_work(work: &serde_json::Value, doi: &str) -> Result<ImportedEntry, ImportError> {
    let mut fields: IndexMap<String, String> = IndexMap::new();

    // DOI
    fields.insert("doi".to_string(), doi.to_string());

    // Title
    if let Some(title) = work["title"]
        .as_array()
        .and_then(|a| a.first())
        .and_then(|v| v.as_str())
    {
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
    let Some(authors) = authors else {
        return String::new();
    };

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
        assert_eq!(
            crossref_type_to_bibtex("proceedings-article"),
            "inproceedings"
        );
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

    // ── N09: metadata search results are compared with the entry ─────────────

    fn search(
        title: &str,
        author: &str,
        year: &str,
        items: serde_json::Value,
    ) -> Result<MetadataMatch, String> {
        let body = serde_json::json!({"message": {"items": items}}).to_string();
        let http = crate::util::import::http::tests::MockHttp::new(vec![Ok(Box::leak(
            body.into_boxed_str(),
        ))]);
        search_by_metadata_with(title, author, year, &http)
    }

    fn work(doi: &str, title: &str, family: &str, year: u64, score: f64) -> serde_json::Value {
        serde_json::json!({
            "DOI": doi,
            "URL": format!("https://doi.org/{doi}"),
            "score": score,
            "title": [title],
            "author": [{"family": family, "given": "A."}],
            "issued": {"date-parts": [[year]]},
        })
    }

    fn unique_doi(result: Result<MetadataMatch, String>) -> String {
        match result {
            Ok(MetadataMatch::Unique(c)) => c.doi,
            other => panic!("expected a unique match, got {other:?}"),
        }
    }

    fn choices(result: Result<MetadataMatch, String>) -> (Vec<String>, bool) {
        match result {
            Ok(MetadataMatch::Choose {
                candidates,
                ambiguous,
            }) => (candidates.into_iter().map(|c| c.doi).collect(), ambiguous),
            other => panic!("expected a choice, got {other:?}"),
        }
    }

    #[test]
    fn test_metadata_exact_match_is_unique_even_when_ranked_lower() {
        let items = serde_json::json!([
            work(
                "10.1/other",
                "Neutron Transport Methods",
                "Brown",
                2019,
                90.0
            ),
            work(
                "10.1/right",
                "Monte Carlo Variance Reduction",
                "Smith",
                2020,
                40.0
            ),
        ]);
        let result = search(
            "Monte Carlo Variance Reduction",
            "Smith, John",
            "2020",
            items,
        );
        assert_eq!(unique_doi(result), "10.1/right");
    }

    #[test]
    fn test_metadata_punctuation_latex_and_subtitle_variants_match() {
        let items = serde_json::json!([work(
            "10.1/godel",
            "On Gödel’s Theorems — A Survey",
            "Gödel",
            1931,
            10.0
        )]);
        let result = search(
            r#"On {G}{\"o}del's {T}heorems: a survey"#,
            r#"G{\"o}del, Kurt"#,
            "1931",
            items,
        );
        assert_eq!(unique_doi(result), "10.1/godel");

        let mut with_subtitle = work("10.1/sub", "Particle Transport", "Lee", 2001, 5.0);
        with_subtitle["subtitle"] = serde_json::json!(["Theory and Practice"]);
        let result = search(
            "Particle Transport: Theory and Practice",
            "Lee, Ann",
            "2001",
            serde_json::json!([with_subtitle]),
        );
        assert_eq!(unique_doi(result), "10.1/sub");
    }

    #[test]
    fn test_metadata_unrelated_results_are_rejected() {
        // The review's case: one unrelated work with a tiny score.
        let items = serde_json::json!([work("10.1/x", "Cooking With Herbs", "Jones", 1999, 0.01)]);
        let err = search(
            "Monte Carlo Variance Reduction",
            "Smith, John",
            "2020",
            items,
        )
        .unwrap_err();
        assert!(err.starts_with("no matching work found"), "{err}");
        assert!(
            err.contains("Cooking With Herbs — Jones (1999) — 10.1/x"),
            "{err}"
        );

        let err = search(
            "A Title",
            "Smith",
            "2020",
            serde_json::json!([{"title": ["no DOI"]}]),
        )
        .unwrap_err();
        assert_eq!(err, "no matching work found");
    }

    #[test]
    fn test_metadata_same_title_different_author_needs_confirmation() {
        let items = serde_json::json!([work(
            "10.1/b",
            "Monte Carlo Variance Reduction",
            "Brown",
            2020,
            99.0
        )]);
        let (dois, ambiguous) = choices(search(
            "Monte Carlo Variance Reduction",
            "Smith, John",
            "2020",
            items,
        ));
        assert_eq!(dois, vec!["10.1/b"]);
        assert!(!ambiguous);
    }

    #[test]
    fn test_metadata_multiple_editions() {
        let items = || {
            serde_json::json!([
                work("10.1/ed1", "Reactor Physics", "Stacey", 2001, 50.0),
                work("10.1/ed2", "Reactor Physics", "Stacey", 2007, 49.0),
            ])
        };
        // The year picks the edition.
        assert_eq!(
            unique_doi(search("Reactor Physics", "Stacey, W. M.", "2007", items())),
            "10.1/ed2"
        );
        // Without a year both match equally: ask.
        let (dois, ambiguous) = choices(search("Reactor Physics", "Stacey, W. M.", "", items()));
        assert_eq!(dois, vec!["10.1/ed1", "10.1/ed2"]);
        assert!(ambiguous);
    }

    #[test]
    fn test_metadata_missing_information_is_not_assumed() {
        // Title alone confirms nothing else: ask.
        let items = serde_json::json!([work("10.1/t", "Introduction", "Smith", 2020, 80.0)]);
        let (dois, _) = choices(search("Introduction", "", "", items));
        assert_eq!(dois, vec!["10.1/t"]);

        // A result without authors or dates can still be confirmed by the
        // other field, but is never contradicted by what it lacks.
        let bare = serde_json::json!([{"DOI": "10.1/bare", "title": ["Monte Carlo Variance Reduction"], "issued": {"date-parts": [[2020]]}}]);
        assert_eq!(
            unique_doi(search(
                "Monte Carlo Variance Reduction",
                "Smith",
                "2020",
                bare
            )),
            "10.1/bare"
        );

        // Author-only search: author and year must both agree to be offered.
        let items = serde_json::json!([
            work("10.1/a", "Some Paper", "Smith", 2020, 9.0),
            work("10.1/b", "Other Paper", "Smith", 2015, 8.0),
        ]);
        let (dois, ambiguous) = choices(search("", "Smith, J.", "2020", items));
        assert_eq!(dois, vec!["10.1/a"]);
        assert!(!ambiguous);
    }

    #[test]
    fn test_metadata_online_year_counts() {
        let mut w = work("10.1/y", "Fast Methods", "Kim", 2021, 5.0);
        w["published-online"] = serde_json::json!({"date-parts": [[2020, 11]]});
        assert_eq!(
            unique_doi(search(
                "Fast Methods",
                "Kim, Y.",
                "2020",
                serde_json::json!([w])
            )),
            "10.1/y"
        );
    }

    #[test]
    fn test_metadata_duplicate_dois_are_listed_once() {
        let items = serde_json::json!([
            work("10.1/same", "Reactor Physics", "Stacey", 2001, 5.0),
            work("10.1/SAME", "Reactor Physics", "Stacey", 2001, 4.0),
        ]);
        assert_eq!(
            unique_doi(search("Reactor Physics", "Stacey", "2001", items)),
            "10.1/same"
        );
    }

    // ── search_by_metadata early-return (no network) ──────────────────────────

    #[test]
    fn test_search_by_metadata_empty_title_and_author_returns_err() {
        let result = search_by_metadata("", "", "2020");
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(
            msg.contains("title") || msg.contains("author"),
            "msg: {}",
            msg
        );
    }

    #[test]
    fn test_search_by_metadata_whitespace_title_and_author_returns_err() {
        let result = search_by_metadata("   ", "\t", "");
        assert!(result.is_err());
    }
}
