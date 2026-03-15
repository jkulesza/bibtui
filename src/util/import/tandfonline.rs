use super::crossref::CrossrefFetcher;
use super::fetcher::Fetcher;
use super::{ImportedEntry, ImportError};

/// Fetches BibTeX metadata from Taylor & Francis Online (tandfonline.com) URLs.
///
/// T&F URLs commonly embed the DOI directly in the path:
///   `https://www.tandfonline.com/doi/abs/10.1080/...`
///   `https://www.tandfonline.com/doi/full/10.1080/...`
pub struct TandFOnlineFetcher;

impl TandFOnlineFetcher {
    const HOST: &'static str = "tandfonline.com";

    /// Try to extract a DOI directly from a T&F URL path.
    fn doi_from_url(url: &str) -> Option<String> {
        // Patterns: /doi/abs/10.xxx, /doi/full/10.xxx, /doi/pdf/10.xxx, /doi/10.xxx
        let segments = ["/doi/abs/", "/doi/full/", "/doi/pdf/", "/doi/epdf/", "/doi/"];
        for seg in &segments {
            if let Some(pos) = url.find(seg) {
                let after = &url[pos + seg.len()..];
                // DOI starts with 10.
                if after.starts_with("10.") {
                    // Strip query string or fragment
                    let end = after
                        .find('?')
                        .or_else(|| after.find('#'))
                        .unwrap_or(after.len());
                    return Some(after[..end].to_string());
                }
            }
        }
        None
    }
}

impl Fetcher for TandFOnlineFetcher {
    fn can_handle(&self, doi_or_url: &str) -> bool {
        doi_or_url.contains(Self::HOST)
    }

    fn fetch(&self, doi_or_url: &str) -> Result<ImportedEntry, ImportError> {
        // Try to extract the DOI directly from the URL path
        let doi = if let Some(d) = Self::doi_from_url(doi_or_url) {
            d
        } else {
            // Fall back to fetching the page and scraping for a DOI
            let html = ureq::get(doi_or_url)
                .set("User-Agent", "bibtui/0.1 (https://github.com/jkulesza/bibtui)")
                .call()
                .map_err(|e| ImportError::Network(e.to_string()))?
                .into_string()
                .map_err(|e| ImportError::Parse(e.to_string()))?;

            extract_doi_from_html(&html)
                .ok_or_else(|| ImportError::Parse("Could not find DOI in T&F page".to_string()))?
        };

        let mut entry = CrossrefFetcher.fetch(&doi)?;

        // Use the original publisher URL rather than the doi.org resolver
        entry.fields.insert("url".to_string(), doi_or_url.to_string());

        Ok(entry)
    }
}

fn extract_doi_from_html(html: &str) -> Option<String> {
    // <meta name="dc.identifier" content="10.xxxx/...">
    let html_lower = html.to_lowercase();
    for meta_name in &["dc.identifier", "citation_doi"] {
        let needle = format!("name=\"{}\"", meta_name);
        if let Some(pos) = html_lower.find(&needle) {
            let after = &html[pos..];
            if let Some(content_pos) = after.to_lowercase().find("content=\"") {
                let start = content_pos + "content=\"".len();
                let rest = &after[start..];
                if let Some(end) = rest.find('"') {
                    let val = rest[..end].trim();
                    if val.starts_with("10.") {
                        return Some(val.to_string());
                    }
                }
            }
        }
    }
    // href="https://doi.org/10."
    if let Some(pos) = html.find("href=\"https://doi.org/10.") {
        let start = pos + "href=\"https://doi.org/".len();
        let rest = &html[start..];
        if let Some(end) = rest.find('"') {
            return Some(rest[..end].to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_can_handle() {
        let f = TandFOnlineFetcher;
        assert!(f.can_handle("https://www.tandfonline.com/doi/abs/10.1080/00295639.2020.1234567"));
        assert!(!f.can_handle("https://www.ans.org/pubs/journals/nt/article-1234/"));
        assert!(!f.can_handle("10.1016/j.foo.2020.1234"));
    }

    #[test]
    fn test_doi_from_url_abs() {
        assert_eq!(
            TandFOnlineFetcher::doi_from_url(
                "https://www.tandfonline.com/doi/abs/10.1080/00295639.2020.1234567"
            ),
            Some("10.1080/00295639.2020.1234567".to_string())
        );
    }

    #[test]
    fn test_doi_from_url_full() {
        assert_eq!(
            TandFOnlineFetcher::doi_from_url(
                "https://www.tandfonline.com/doi/full/10.1080/00295639.2020.1234567?abc=1"
            ),
            Some("10.1080/00295639.2020.1234567".to_string())
        );
    }

    #[test]
    fn test_doi_from_url_no_match() {
        assert_eq!(
            TandFOnlineFetcher::doi_from_url("https://www.tandfonline.com/toc/unct20/current"),
            None
        );
    }

    #[test]
    fn test_doi_from_url_pdf_segment() {
        assert_eq!(
            TandFOnlineFetcher::doi_from_url(
                "https://www.tandfonline.com/doi/pdf/10.1080/00295639.2020.1234567"
            ),
            Some("10.1080/00295639.2020.1234567".to_string())
        );
    }

    #[test]
    fn test_doi_from_url_epdf_segment() {
        assert_eq!(
            TandFOnlineFetcher::doi_from_url(
                "https://www.tandfonline.com/doi/epdf/10.1080/00295639.2020.1234567"
            ),
            Some("10.1080/00295639.2020.1234567".to_string())
        );
    }

    #[test]
    fn test_doi_from_url_fragment_stripped() {
        assert_eq!(
            TandFOnlineFetcher::doi_from_url(
                "https://www.tandfonline.com/doi/abs/10.1080/00295639.2020.1234567#section1"
            ),
            Some("10.1080/00295639.2020.1234567".to_string())
        );
    }

    #[test]
    fn test_doi_from_url_bare_doi_segment() {
        assert_eq!(
            TandFOnlineFetcher::doi_from_url(
                "https://www.tandfonline.com/doi/10.1080/00295639.2020.1234567"
            ),
            Some("10.1080/00295639.2020.1234567".to_string())
        );
    }

    // ── extract_doi_from_html ─────────────────────────────────────────────────

    #[test]
    fn test_extract_doi_from_html_dc_identifier() {
        let html = r#"<meta name="dc.identifier" content="10.1080/00295639.2025.1234567">"#;
        assert_eq!(
            extract_doi_from_html(html),
            Some("10.1080/00295639.2025.1234567".to_string())
        );
    }

    #[test]
    fn test_extract_doi_from_html_citation_doi() {
        let html = r#"<meta name="citation_doi" content="10.1080/00295639.2025.9876543">"#;
        assert_eq!(
            extract_doi_from_html(html),
            Some("10.1080/00295639.2025.9876543".to_string())
        );
    }

    #[test]
    fn test_extract_doi_from_html_href_fallback() {
        let html = r#"<a href="https://doi.org/10.1080/00295639.2025.1234567">Link</a>"#;
        assert_eq!(
            extract_doi_from_html(html),
            Some("10.1080/00295639.2025.1234567".to_string())
        );
    }

    #[test]
    fn test_extract_doi_from_html_meta_preferred_over_href() {
        // When both meta and href present, meta wins (listed first in the search order)
        let html = r#"<meta name="citation_doi" content="10.1080/AAA.2025.1">
            <a href="https://doi.org/10.1080/BBB.2025.2">Link</a>"#;
        assert_eq!(
            extract_doi_from_html(html),
            Some("10.1080/AAA.2025.1".to_string())
        );
    }

    #[test]
    fn test_extract_doi_from_html_non_doi_content_skipped() {
        // content doesn't start with "10." — should skip and fall through
        let html = r#"<meta name="dc.identifier" content="urn:issn:0029-5639">
            <a href="https://doi.org/10.1080/00295639.2025.1234567">Link</a>"#;
        assert_eq!(
            extract_doi_from_html(html),
            Some("10.1080/00295639.2025.1234567".to_string())
        );
    }

    #[test]
    fn test_extract_doi_from_html_none() {
        assert_eq!(extract_doi_from_html("<html>No DOI here</html>"), None);
    }
}
