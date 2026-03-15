use super::ans::AnsFetcher;
use super::crossref::CrossrefFetcher;
use super::fetcher::Fetcher;
use super::pdf::PdfFetcher;
use super::tandfonline::TandFOnlineFetcher;
use super::{ImportedEntry, ImportError};

/// The ordered list of fetchers tried in sequence.
/// PdfFetcher first (local file paths), then publisher-specific scrapers,
/// then Crossref as the general DOI/URL fallback.
fn fetchers() -> Vec<Box<dyn Fetcher>> {
    vec![
        Box::new(PdfFetcher),
        Box::new(AnsFetcher),
        Box::new(TandFOnlineFetcher),
        Box::new(CrossrefFetcher),
    ]
}

/// Run the fetcher pipeline: find the first fetcher that can handle the input,
/// fetch the metadata, apply publisher corrections, then prepend any Unpaywall
/// open-access PDF URL to the candidate list so it is tried before paywalled
/// publisher URLs.
pub fn run(doi_or_url: &str) -> Result<ImportedEntry, ImportError> {
    for fetcher in fetchers() {
        if fetcher.can_handle(doi_or_url) {
            let mut entry = fetcher.fetch(doi_or_url)?;
            // Fix publisher field when Crossref reports a distributor instead of the
            // society publisher (e.g. T&F listed instead of ANS).
            apply_publisher_corrections(&mut entry);
            // Prepend Unpaywall OA PDF URL if available — free, legal, no auth required.
            if let Some(doi) = entry.fields.get("doi").cloned() {
                if let Some(oa_url) = unpaywall_pdf_url(&doi) {
                    entry.pdf_urls.insert(0, oa_url);
                }
            }
            return Ok(entry);
        }
    }
    Err(ImportError::NoMatch(doi_or_url.to_string()))
}

/// Correct the `publisher` field when Crossref reports a distributor/aggregator
/// rather than the originating society publisher.
///
/// Checks (in order): DOI prefix, ISSN, journal name.
fn apply_publisher_corrections(entry: &mut ImportedEntry) {
    let doi = entry.fields.get("doi").cloned().unwrap_or_default();
    let issn = entry.fields.get("issn").cloned().unwrap_or_default();
    let journal = entry.fields.get("journal").cloned().unwrap_or_default();
    let journal_lower = journal.to_lowercase();

    // ── American Nuclear Society ─────────────────────────────────────────────
    // ANS-owned DOI prefix
    if doi.starts_with("10.13182/") {
        entry.fields.insert("publisher".to_string(), "American Nuclear Society".to_string());
        return;
    }
    // ANS journals distributed by Taylor & Francis — identified by ISSN
    const ANS_ISSNS: &[&str] = &[
        "0029-5639", "1943-748X",  // Nuclear Science and Engineering
        "0029-5450", "1943-7471",  // Nuclear Technology
        "1536-1055", "1943-7641",  // Fusion Science and Technology
        "0003-018X", "1943-7714",  // Transactions of the American Nuclear Society
    ];
    if ANS_ISSNS.iter().any(|&i| issn.contains(i)) {
        entry.fields.insert("publisher".to_string(), "American Nuclear Society".to_string());
        return;
    }
    // ANS journals by name (fallback)
    const ANS_JOURNALS: &[&str] = &[
        "nuclear science and engineering",
        "nuclear technology",
        "fusion science and technology",
        "transactions of the american nuclear society",
    ];
    if ANS_JOURNALS.iter().any(|&j| journal_lower.contains(j)) {
        entry.fields.insert("publisher".to_string(), "American Nuclear Society".to_string());
    }
}

/// Query the Unpaywall public API for a legal open-access PDF URL.
/// Returns `None` if the paper has no OA copy or the API is unreachable.
fn unpaywall_pdf_url(doi: &str) -> Option<String> {
    let url = format!(
        "https://api.unpaywall.org/v2/{}?email=bibtui@example.com",
        doi
    );
    let response = ureq::get(&url)
        .set("User-Agent", "bibtui/0.1 (https://github.com/jkulesza/bibtui)")
        .call()
        .ok()?;
    let json: serde_json::Value = response.into_json().ok()?;
    // best_oa_location.url_for_pdf is the most direct downloadable copy
    let pdf_url = json["best_oa_location"]["url_for_pdf"].as_str()?;
    if pdf_url.is_empty() {
        None
    } else {
        Some(pdf_url.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::IndexMap;

    fn make_entry(doi: Option<&str>, issn: Option<&str>, journal: Option<&str>, publisher: Option<&str>) -> ImportedEntry {
        let mut fields = IndexMap::new();
        if let Some(d) = doi     { fields.insert("doi".to_string(),       d.to_string()); }
        if let Some(i) = issn    { fields.insert("issn".to_string(),      i.to_string()); }
        if let Some(j) = journal { fields.insert("journal".to_string(),   j.to_string()); }
        if let Some(p) = publisher { fields.insert("publisher".to_string(), p.to_string()); }
        ImportedEntry::new("article", fields)
    }

    #[test]
    fn test_no_match_returns_error() {
        let result = run("not-a-doi-or-url");
        assert!(matches!(result, Err(ImportError::NoMatch(_))));
    }

    #[test]
    fn test_bare_doi_routes_to_crossref() {
        // CrossrefFetcher::can_handle should match; the actual HTTP call would
        // fail in a unit test environment, but we can at least verify routing.
        // We just check that the NoMatch error is NOT returned for a valid DOI.
        let result = run("10.1016/j.anucene.2020.107650");
        // Either succeeds or fails with Network/Parse, but NOT NoMatch.
        assert!(!matches!(result, Err(ImportError::NoMatch(_))));
    }

    // ── apply_publisher_corrections ───────────────────────────────────────────

    #[test]
    fn test_correction_ans_doi_prefix() {
        let mut e = make_entry(Some("10.13182/NSE20-1234"), None, None, Some("Taylor & Francis"));
        apply_publisher_corrections(&mut e);
        assert_eq!(e.fields["publisher"], "American Nuclear Society");
    }

    #[test]
    fn test_correction_ans_issn_nse_print() {
        let mut e = make_entry(Some("10.1080/00295639.2025.1"), Some("0029-5639"), None, Some("Informa UK Limited"));
        apply_publisher_corrections(&mut e);
        assert_eq!(e.fields["publisher"], "American Nuclear Society");
    }

    #[test]
    fn test_correction_ans_issn_nse_electronic() {
        let mut e = make_entry(Some("10.1080/x"), Some("1943-748X"), None, Some("T&F"));
        apply_publisher_corrections(&mut e);
        assert_eq!(e.fields["publisher"], "American Nuclear Society");
    }

    #[test]
    fn test_correction_ans_issn_nt() {
        let mut e = make_entry(None, Some("0029-5450"), None, Some("T&F"));
        apply_publisher_corrections(&mut e);
        assert_eq!(e.fields["publisher"], "American Nuclear Society");
    }

    #[test]
    fn test_correction_ans_issn_fst() {
        let mut e = make_entry(None, Some("1536-1055"), None, Some("T&F"));
        apply_publisher_corrections(&mut e);
        assert_eq!(e.fields["publisher"], "American Nuclear Society");
    }

    #[test]
    fn test_correction_ans_journal_name() {
        let mut e = make_entry(None, None, Some("Nuclear Science and Engineering"), Some("T&F"));
        apply_publisher_corrections(&mut e);
        assert_eq!(e.fields["publisher"], "American Nuclear Society");
    }

    #[test]
    fn test_correction_ans_journal_name_case_insensitive() {
        let mut e = make_entry(None, None, Some("NUCLEAR TECHNOLOGY"), Some("T&F"));
        apply_publisher_corrections(&mut e);
        assert_eq!(e.fields["publisher"], "American Nuclear Society");
    }

    #[test]
    fn test_correction_non_ans_publisher_unchanged() {
        let mut e = make_entry(Some("10.1016/j.foo.2020.1"), None, Some("Journal of Physics"), Some("Elsevier"));
        apply_publisher_corrections(&mut e);
        assert_eq!(e.fields["publisher"], "Elsevier");
    }

    #[test]
    fn test_correction_empty_fields_no_panic() {
        let mut e = ImportedEntry::new("article", IndexMap::new());
        apply_publisher_corrections(&mut e); // must not panic
        assert!(!e.fields.contains_key("publisher"));
    }

    #[test]
    fn test_correction_doi_prefix_takes_priority_over_issn() {
        // Ensure early return on DOI prefix match even when ISSN is also present
        let mut e = make_entry(Some("10.13182/X"), Some("0029-5639"), None, Some("Wrong"));
        apply_publisher_corrections(&mut e);
        assert_eq!(e.fields["publisher"], "American Nuclear Society");
    }
}
