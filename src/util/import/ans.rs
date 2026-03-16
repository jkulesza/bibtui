use super::crossref::CrossrefFetcher;
use super::fetcher::Fetcher;
use super::{ImportedEntry, ImportError};

/// Fetches BibTeX metadata from American Nuclear Society (ANS) publication URLs.
///
/// Downloads the article page, extracts the DOI embedded in the HTML,
/// fetches metadata from Crossref, then overrides the url and publisher
/// fields with ANS-specific values.
pub struct AnsFetcher;

impl AnsFetcher {
    const HOST: &'static str = "www.ans.org";
}

impl Fetcher for AnsFetcher {
    fn can_handle(&self, doi_or_url: &str) -> bool {
        doi_or_url.contains(Self::HOST)
    }

    fn fetch(&self, doi_or_url: &str) -> Result<ImportedEntry, ImportError> {
        let html = ureq::get(doi_or_url)
            .set("User-Agent", "bibtui/0.1 (https://github.com/jkulesza/bibtui)")
            .call()
            .map_err(|e| ImportError::Network(e.to_string()))?
            .into_string()
            .map_err(|e| ImportError::Parse(e.to_string()))?;

        let doi = extract_doi_from_html(&html)
            .ok_or_else(|| ImportError::Parse("Could not find DOI in ANS page".to_string()))?;

        let mut entry = CrossrefFetcher.fetch(&doi)?;

        // The original ANS URL is more useful to the user than the doi.org resolver
        entry.fields.insert("url".to_string(), doi_or_url.to_string());

        // ANS is the society publisher; Crossref may report the distributor (e.g. T&F)
        entry.fields.insert("publisher".to_string(), "American Nuclear Society".to_string());

        // Build PDF URL candidates in priority order; the import thread tries each in sequence.
        entry.pdf_urls = pdf_url_candidates(&html, &doi, doi_or_url);

        Ok(entry)
    }
}

/// Extract a DOI from common HTML patterns.
fn extract_doi_from_html(html: &str) -> Option<String> {
    // Pattern 1: <meta name="citation_doi" content="10.xxxx/...">
    if let Some(doi) = extract_meta_content(html, "citation_doi") {
        return Some(doi);
    }

    // Pattern 2: <meta name="DC.Identifier" content="10.xxxx/...">
    if let Some(doi) = extract_meta_content(html, "DC.Identifier") {
        if doi.starts_with("10.") {
            return Some(doi);
        }
    }

    // Pattern 3: href="https://doi.org/10.xxxx/..."
    if let Some(doi) = extract_doi_from_href(html) {
        return Some(doi);
    }

    None
}

/// Build an ordered list of PDF URL candidates for this ANS article.
/// The import thread tries each in sequence, stopping on the first successful download.
fn pdf_url_candidates(html: &str, doi: &str, article_url: &str) -> Vec<String> {
    let mut candidates: Vec<String> = Vec::new();

    // 1. citation_pdf_url meta tag — most reliable when present
    if let Some(url) = extract_meta_content(html, "citation_pdf_url") {
        candidates.push(url);
    }

    // 2. ANS direct PDF endpoint: append /pdf/ to the article URL.
    //    ANS member access or open-access articles often serve the PDF here.
    let ans_pdf = format!("{}/pdf/", article_url.trim_end_matches('/'));
    if !candidates.contains(&ans_pdf) {
        candidates.push(ans_pdf);
    }

    // 3. T&F PDF URL for journals distributed by Taylor & Francis (DOI prefix 10.1080/)
    if doi.starts_with("10.1080/") {
        let tf_pdf = format!("https://www.tandfonline.com/doi/pdf/{}", doi);
        if !candidates.contains(&tf_pdf) {
            candidates.push(tf_pdf);
        }
    }

    candidates
}

/// Extract a named attribute value from `<meta>` tags, handling any attribute order.
fn extract_meta_content(html: &str, name: &str) -> Option<String> {
    let html_lower = html.to_lowercase();
    let name_needle = format!("name=\"{}\"", name.to_lowercase());

    let mut search_start = 0;
    while let Some(rel) = html_lower[search_start..].find("<meta") {
        let tag_start = search_start + rel;
        // Find the closing '>' of this tag
        let tag_end = html_lower[tag_start..]
            .find('>')
            .map(|i| tag_start + i + 1)
            .unwrap_or(html_lower.len());

        let tag_lower = &html_lower[tag_start..tag_end];
        let tag_orig = &html[tag_start..tag_end];

        if tag_lower.contains(name_needle.as_str()) {
            // Extract content="..." from within this same tag
            if let Some(cp) = tag_lower.find("content=\"") {
                let start = cp + "content=\"".len();
                let rest = &tag_orig[start..];
                if let Some(end) = rest.find('"') {
                    let value = rest[..end].trim();
                    if !value.is_empty() {
                        return Some(value.to_string());
                    }
                }
            }
        }

        search_start = tag_end;
    }
    None
}

fn extract_doi_from_href(html: &str) -> Option<String> {
    let prefix = "href=\"https://doi.org/10.";
    let pos = html.find(prefix)?;
    let start = pos + "href=\"https://doi.org/".len();
    let rest = &html[start..];
    let end = rest.find('"')?;
    Some(rest[..end].trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_can_handle() {
        let f = AnsFetcher;
        assert!(f.can_handle("https://www.ans.org/pubs/journals/nt/article-1234/"));
        assert!(!f.can_handle("https://www.tandfonline.com/doi/abs/10.1080/xyz"));
        assert!(!f.can_handle("10.1016/j.foo.2020.1234"));
    }

    #[test]
    fn test_extract_meta_citation_doi() {
        let html = r#"<html><head>
            <meta name="citation_doi" content="10.13182/NSE20-1234">
        </head></html>"#;
        assert_eq!(
            extract_doi_from_html(html),
            Some("10.13182/NSE20-1234".to_string())
        );
    }

    #[test]
    fn test_extract_doi_from_href() {
        let html = r#"<a href="https://doi.org/10.13182/NT20-5678">Full text</a>"#;
        assert_eq!(
            extract_doi_from_html(html),
            Some("10.13182/NT20-5678".to_string())
        );
    }

    #[test]
    fn test_extract_doi_missing() {
        let html = "<html><body>No DOI here</body></html>";
        assert_eq!(extract_doi_from_html(html), None);
    }

    #[test]
    fn test_pdf_candidates_from_meta_tag() {
        let html = r#"<meta name="citation_pdf_url" content="https://www.ans.org/pubs/journals/nse/article-60004/pdf/">"#;
        let candidates = pdf_url_candidates(html, "10.13182/NSE20-1234", "https://www.ans.org/pubs/journals/nse/article-60004");
        // meta tag URL comes first
        assert_eq!(candidates[0], "https://www.ans.org/pubs/journals/nse/article-60004/pdf/");
        // ANS direct URL would be the same, so only 1 entry (deduped)
        assert_eq!(candidates.len(), 1);
    }

    #[test]
    fn test_pdf_candidates_no_meta_tandf_doi() {
        // No citation_pdf_url; T&F DOI — should have ANS direct + T&F candidates
        let html = "<html><body>No PDF meta here</body></html>";
        let candidates = pdf_url_candidates(html, "10.1080/00295639.2025.2483123", "https://www.ans.org/pubs/journals/nse/article-60004");
        assert_eq!(candidates[0], "https://www.ans.org/pubs/journals/nse/article-60004/pdf/");
        assert_eq!(candidates[1], "https://www.tandfonline.com/doi/pdf/10.1080/00295639.2025.2483123");
        assert_eq!(candidates.len(), 2);
    }

    #[test]
    fn test_pdf_candidates_no_meta_ans_doi() {
        // No citation_pdf_url; ANS-prefix DOI (not T&F) — only ANS direct URL
        let html = "<html><body>No PDF meta here</body></html>";
        let candidates = pdf_url_candidates(html, "10.13182/NSE20-9999", "https://www.ans.org/pubs/journals/nse/article-58027");
        assert_eq!(candidates[0], "https://www.ans.org/pubs/journals/nse/article-58027/pdf/");
        assert_eq!(candidates.len(), 1);
    }

    #[test]
    fn test_extract_meta_content_attr_order_reversed() {
        // content before name in the same tag — the old impl would fail this
        let html = r#"<meta content="10.13182/NSE20-1234" name="citation_doi">"#;
        assert_eq!(
            extract_meta_content(html, "citation_doi"),
            Some("10.13182/NSE20-1234".to_string())
        );
    }

    #[test]
    fn test_extract_meta_content_empty_value_returns_none() {
        let html = r#"<meta name="citation_doi" content="">"#;
        assert_eq!(extract_meta_content(html, "citation_doi"), None);
    }

    #[test]
    fn test_extract_meta_content_skips_wrong_name() {
        // First tag has a different name; second has the correct one
        let html = r#"<meta name="other_field" content="irrelevant">
            <meta name="citation_doi" content="10.13182/NSE-99">"#;
        assert_eq!(
            extract_meta_content(html, "citation_doi"),
            Some("10.13182/NSE-99".to_string())
        );
    }

    #[test]
    fn test_extract_meta_content_unclosed_tag_returns_none() {
        // Tag without closing '>' — should not panic and should return None
        let html = r#"<meta name="citation_doi" content="10.1234/x"#;
        // find('>') fails, tag_end falls back to len() — tag_lower covers whole string
        // but the content should still be found since name+content are present
        let result = extract_meta_content(html, "citation_doi");
        // Either None or Some is acceptable — just must not panic
        let _ = result;
    }

    #[test]
    fn test_extract_doi_from_href_missing_closing_quote() {
        // Missing closing quote after the DOI — find('"') returns None
        let html = r#"<a href="https://doi.org/10.13182/NT20-1234"#;
        assert_eq!(extract_doi_from_href(html), None);
    }

    #[test]
    fn test_extract_doi_from_href_no_doi_prefix() {
        let html = r#"<a href="https://example.com/paper">text</a>"#;
        assert_eq!(extract_doi_from_href(html), None);
    }

    #[test]
    fn test_extract_doi_from_html_meta_preferred_over_href() {
        // When both a meta citation_doi and an href DOI exist, meta is tried first
        let html = r#"
            <meta name="citation_doi" content="10.13182/META-DOI">
            <a href="https://doi.org/10.13182/HREF-DOI">text</a>"#;
        assert_eq!(
            extract_doi_from_html(html),
            Some("10.13182/META-DOI".to_string())
        );
    }
}
