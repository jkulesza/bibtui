use std::path::Path;

use super::crossref::CrossrefFetcher;
use super::fetcher::Fetcher;
use super::{ImportedEntry, ImportError};

/// Fetches BibTeX metadata by reading a local PDF file.
///
/// Extracts the DOI from the PDF (metadata fields, XMP, labeled `doi:` text,
/// or raw DOI patterns in the first 200 KB), then looks it up via Crossref.
/// Sets `pdf_path` directly so no download is needed.
pub struct PdfFetcher;

impl PdfFetcher {
    /// Extract a DOI from the raw bytes of a PDF file.
    pub fn extract_doi_from_path(path: &Path) -> Result<String, ImportError> {
        let bytes =
            std::fs::read(path).map_err(|e| ImportError::Parse(format!("Cannot read file: {}", e)))?;

        if !bytes.starts_with(b"%PDF") {
            return Err(ImportError::Parse("File is not a valid PDF".to_string()));
        }

        // Search the first 200 KB — covers uncompressed DocInfo, XMP, and early page headers.
        let head_end = bytes.len().min(200_000);
        let head = String::from_utf8_lossy(&bytes[..head_end]);

        if let Some(doi) = find_doi_in_text(&head) {
            return Ok(doi);
        }

        // Also check the last 50 KB — xref/trailer sometimes repeats metadata.
        if bytes.len() > head_end {
            let tail = String::from_utf8_lossy(&bytes[bytes.len().saturating_sub(50_000)..]);
            if let Some(doi) = find_doi_in_text(&tail) {
                return Ok(doi);
            }
        }

        Err(ImportError::Parse("No DOI found in PDF".to_string()))
    }
}

impl Fetcher for PdfFetcher {
    fn can_handle(&self, input: &str) -> bool {
        input.to_lowercase().ends_with(".pdf") && Path::new(input).exists()
    }

    fn fetch(&self, input: &str) -> Result<ImportedEntry, ImportError> {
        let path = Path::new(input);
        let doi = Self::extract_doi_from_path(path)?;

        let mut entry = CrossrefFetcher.fetch(&doi)?;

        // The PDF is already local — no download needed; set path directly.
        entry.pdf_path = Some(
            path.canonicalize()
                .unwrap_or_else(|_| path.to_path_buf()),
        );

        Ok(entry)
    }
}

/// Search `text` for a DOI, trying labeled patterns first then bare patterns.
fn find_doi_in_text(text: &str) -> Option<String> {
    labeled_doi(text).or_else(|| bare_doi(text))
}

/// Try common labeled DOI patterns (case-insensitive).
///
/// Handles:
/// - `doi:10.xxx/...`  / `DOI: 10.xxx/...`
/// - `https://doi.org/10.xxx/...`
/// - `http://dx.doi.org/10.xxx/...`
/// - XML/XMP: `<prism:doi>10.xxx/...</prism:doi>`
/// - PDF /Subject or /Keywords entries containing the DOI
fn labeled_doi(text: &str) -> Option<String> {
    let lower = text.to_lowercase();

    // Ordered from most-specific to least-specific
    let prefixes: &[&str] = &[
        "doi.org/",
        "doi:",
        "doi :",
        "doi\t",
        "prism:doi>",
        "/subject (doi:",
        "/subject (doi ",
        "/keywords (doi:",
        "/keywords (doi ",
    ];

    for prefix in prefixes {
        let Some(pos) = lower.find(prefix) else { continue };
        let after = text[pos + prefix.len()..].trim_start_matches(|c: char| c == ' ' || c == '\t');
        // Skip the literal "doi:" that may still prefix the number
        let after = after
            .strip_prefix("doi:")
            .map(|s| s.trim_start_matches(|c: char| c == ' ' || c == '\t'))
            .unwrap_or(after);
        if after.starts_with("10.") {
            if let Some(doi) = extract_doi_at(after) {
                return Some(doi);
            }
        }
    }
    None
}

/// Scan for a bare `10.XXXX/suffix` pattern not preceded by a digit or dot
/// (to avoid matching things like `110.xxx`).
fn bare_doi(text: &str) -> Option<String> {
    let mut pos = 0;
    while let Some(rel) = text[pos..].find("10.") {
        let start = pos + rel;
        // Must not be preceded by a digit or '.' (would make it part of another number)
        if start > 0 {
            let prev = text.as_bytes()[start - 1];
            if prev.is_ascii_digit() || prev == b'.' {
                pos = start + 1;
                continue;
            }
        }
        if let Some(doi) = extract_doi_at(&text[start..]) {
            return Some(doi);
        }
        pos = start + 1;
    }
    None
}

/// Extract a DOI from `text` that starts with `10.`.
/// Returns `None` if the candidate is not a valid DOI shape.
fn extract_doi_at(text: &str) -> Option<String> {
    // DOI ends at whitespace or common PDF/HTML delimiters
    let end = text
        .find(|c: char| {
            c.is_whitespace()
                || matches!(c, '"' | '\'' | '<' | '>' | '{' | '}' | '\\' | ')' | ']' | '\0')
        })
        .unwrap_or(text.len().min(200));

    let candidate = text[..end].trim_end_matches(|c: char| matches!(c, '.' | ',' | ';' | ':'));

    // Validate: 10.XXXX/suffix  (XXXX = 4+ ASCII digits)
    let slash = candidate.find('/')?;
    let registrant = &candidate[3..slash]; // chars after "10."
    if registrant.len() >= 4
        && registrant.chars().all(|c| c.is_ascii_digit())
        && slash + 1 < candidate.len()
    {
        Some(candidate.to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_labeled_doi_colon() {
        let text = "Published article. doi:10.1080/00295639.2025.2483123 more text";
        assert_eq!(
            find_doi_in_text(text),
            Some("10.1080/00295639.2025.2483123".to_string())
        );
    }

    #[test]
    fn test_labeled_doi_url() {
        let text = "See https://doi.org/10.13182/NSE20-1234 for details";
        assert_eq!(
            find_doi_in_text(text),
            Some("10.13182/NSE20-1234".to_string())
        );
    }

    #[test]
    fn test_labeled_doi_case_insensitive() {
        let text = "DOI: 10.1016/j.anucene.2020.107650";
        assert_eq!(
            find_doi_in_text(text),
            Some("10.1016/j.anucene.2020.107650".to_string())
        );
    }

    #[test]
    fn test_xmp_prism_doi() {
        let text = "<prism:doi>10.1103/PhysRevLett.125.012001</prism:doi>";
        assert_eq!(
            find_doi_in_text(text),
            Some("10.1103/PhysRevLett.125.012001".to_string())
        );
    }

    #[test]
    fn test_bare_doi_fallback() {
        // No label — should still find via bare scan
        let text = "some binary-looking text 10.1016/j.foo.2020.1234 end";
        assert_eq!(
            find_doi_in_text(text),
            Some("10.1016/j.foo.2020.1234".to_string())
        );
    }

    #[test]
    fn test_bare_doi_not_preceded_by_digit() {
        // 110.1016 should NOT match (preceded by digit '1')
        let text = "value=110.1016/j.foo.2020 and real doi 10.1016/j.bar.2021.1";
        assert_eq!(
            find_doi_in_text(text),
            Some("10.1016/j.bar.2021.1".to_string())
        );
    }

    #[test]
    fn test_trailing_punctuation_stripped() {
        let text = "doi:10.1080/00295639.2025.2483123.";
        assert_eq!(
            find_doi_in_text(text),
            Some("10.1080/00295639.2025.2483123".to_string())
        );
    }

    #[test]
    fn test_no_doi() {
        let text = "This text has no DOI at all.";
        assert_eq!(find_doi_in_text(text), None);
    }

    #[test]
    fn test_can_handle_rejects_nonexistent() {
        let f = PdfFetcher;
        assert!(!f.can_handle("/nonexistent/path/file.pdf"));
    }

    #[test]
    fn test_can_handle_rejects_non_pdf() {
        let f = PdfFetcher;
        assert!(!f.can_handle("Cargo.toml"));
    }

    #[test]
    fn test_labeled_doi_doi_space_colon() {
        // "doi :" variant
        let text = "doi :10.1234/foo.bar.2020";
        assert_eq!(find_doi_in_text(text), Some("10.1234/foo.bar.2020".to_string()));
    }

    #[test]
    fn test_labeled_doi_tab_after_doi() {
        let text = "doi\t10.1234/baz.2021";
        assert_eq!(find_doi_in_text(text), Some("10.1234/baz.2021".to_string()));
    }

    #[test]
    fn test_labeled_doi_dx_doi_org() {
        let text = "available at http://dx.doi.org/10.1016/j.foo.2020.1234";
        assert_eq!(
            find_doi_in_text(text),
            Some("10.1016/j.foo.2020.1234".to_string())
        );
    }

    #[test]
    fn test_extract_doi_at_short_registrant_rejected() {
        // "10.123/foo" has only 3 digits — must be rejected
        assert_eq!(extract_doi_at("10.123/foo"), None);
    }

    #[test]
    fn test_extract_doi_at_no_slash_rejected() {
        assert_eq!(extract_doi_at("10.12345no-slash"), None);
    }

    #[test]
    fn test_extract_doi_at_trailing_slash_rejected() {
        // "10.1234/" with nothing after the slash is invalid
        assert_eq!(extract_doi_at("10.1234/"), None);
    }

    #[test]
    fn test_extract_doi_at_ends_at_delimiter() {
        assert_eq!(
            extract_doi_at("10.1234/foo.bar\"rest"),
            Some("10.1234/foo.bar".to_string())
        );
    }

    #[test]
    fn test_bare_doi_preceded_by_dot_skipped() {
        // "1.10.1234/foo" — preceded by '.', should skip and not match partial
        // but the standalone "10.1234/foo" at end should match
        let text = "v1.10.1234/foo and 10.9999/real.doi";
        assert_eq!(find_doi_in_text(text), Some("10.9999/real.doi".to_string()));
    }

    #[test]
    fn test_extract_doi_from_pdf_bytes_not_pdf() {
        let _bytes = b"This is not a PDF file at all";
        let result = PdfFetcher::extract_doi_from_path(std::path::Path::new("/nonexistent.pdf"));
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_doi_from_bytes_in_text_region() {
        // Simulate a small in-memory "PDF" with a DOI embedded in the header area.
        // We test the find_doi_in_text function directly since extract_doi_from_path
        // requires a real file.
        let simulated_pdf_text =
            "%PDF-1.4\n/Subject (doi:10.1016/j.anucene.2020.107650)\n/Creator (LaTeX)";
        assert_eq!(
            find_doi_in_text(simulated_pdf_text),
            Some("10.1016/j.anucene.2020.107650".to_string())
        );
    }

    #[test]
    fn test_extract_doi_from_path_header_hit() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        use std::io::Write;
        tmp.write_all(b"%PDF-1.4\n/Subject (doi:10.1016/j.foo.2024.001)\n").unwrap();
        tmp.flush().unwrap();
        let doi = PdfFetcher::extract_doi_from_path(tmp.path()).unwrap();
        assert_eq!(doi, "10.1016/j.foo.2024.001");
    }

    #[test]
    fn test_extract_doi_from_path_tail_hit() {
        // DOI lives only in the tail (last 50 KB); header has no DOI.
        // Pad the middle with > 200 KB of non-DOI bytes so the head scan misses.
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        use std::io::Write;
        tmp.write_all(b"%PDF-1.4\n").unwrap();
        // 250 KB of filler with no DOI — exceeds the 200 KB head window
        tmp.write_all(&vec![b'x'; 250_000]).unwrap();
        tmp.write_all(b"\n/trailer doi:10.9999/tail.found\n").unwrap();
        tmp.flush().unwrap();
        let doi = PdfFetcher::extract_doi_from_path(tmp.path()).unwrap();
        assert_eq!(doi, "10.9999/tail.found");
    }

    #[test]
    fn test_extract_doi_from_path_not_pdf_magic() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        use std::io::Write;
        tmp.write_all(b"NOT A PDF\ndoi:10.1234/foo.bar\n").unwrap();
        tmp.flush().unwrap();
        let err = PdfFetcher::extract_doi_from_path(tmp.path()).unwrap_err();
        assert!(matches!(err, ImportError::Parse(_)));
    }

    #[test]
    fn test_extract_doi_from_path_no_doi_anywhere() {
        let mut tmp = tempfile::NamedTempFile::new().unwrap();
        use std::io::Write;
        tmp.write_all(b"%PDF-1.4\nplain content with no doi at all\n").unwrap();
        tmp.flush().unwrap();
        let err = PdfFetcher::extract_doi_from_path(tmp.path()).unwrap_err();
        assert!(matches!(err, ImportError::Parse(_)));
    }

    #[test]
    fn test_can_handle_accepts_existing_pdf_path() {
        // A real .pdf file on disk should be accepted; case-insensitive extension match.
        let tmp = tempfile::Builder::new().suffix(".PDF").tempfile().unwrap();
        assert!(PdfFetcher.can_handle(tmp.path().to_str().unwrap()));
    }
}
