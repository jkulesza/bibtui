pub mod ans;
pub mod crossref;
pub mod fetcher;
pub mod pdf;
pub mod pipeline;
pub mod tandfonline;

use indexmap::IndexMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// A successfully parsed BibTeX entry imported from a remote source.
#[derive(Debug, Clone)]
pub struct ImportedEntry {
    /// BibTeX entry type in lowercase (e.g. `"article"`, `"book"`).
    pub entry_type: String,
    /// Field name → value map.
    pub fields: IndexMap<String, String>,
    /// PDF download URL candidates to try in order (first success wins).
    pub pdf_urls: Vec<String>,
    /// Local path to a downloaded PDF, populated by the import thread.
    pub pdf_path: Option<PathBuf>,
    /// Error message from PDF download failure (import itself succeeded).
    pub pdf_error: Option<String>,
}

impl ImportedEntry {
    pub fn new(entry_type: impl Into<String>, fields: IndexMap<String, String>) -> Self {
        ImportedEntry {
            entry_type: entry_type.into(),
            fields,
            pdf_urls: Vec::new(),
            pdf_path: None,
            pdf_error: None,
        }
    }
}

#[derive(Debug, Error)]
pub enum ImportError {
    #[error("Network error: {0}")]
    Network(String),
    #[error("Parse error: {0}")]
    Parse(String),
    #[error("No fetcher matched: {0}")]
    NoMatch(String),
}

pub type ImportResult = Result<ImportedEntry, ImportError>;

/// Attempt to import a BibTeX entry from a DOI or URL.
/// Tries fetchers in priority order: publisher-specific scrapers first,
/// then Crossref as the general fallback.
pub fn fetch(doi_or_url: &str) -> ImportResult {
    pipeline::run(doi_or_url)
}

/// Download a PDF from `pdf_url` and save it to `dest_dir`.
/// The filename is derived from the DOI (sanitized for the filesystem).
/// Returns the path of the saved file on success.
pub fn download_pdf(pdf_url: &str, dest_dir: &Path, doi: &str) -> Result<PathBuf, ImportError> {
    let filename = doi_to_filename(doi);
    let dest = dest_dir.join(&filename);

    let response = ureq::get(pdf_url)
        .set("User-Agent", "bibtui/0.1 (https://github.com/jkulesza/bibtui)")
        .call()
        .map_err(|e| ImportError::Network(e.to_string()))?;

    let mut reader = response.into_reader();
    let mut buf = Vec::new();
    reader
        .read_to_end(&mut buf)
        .map_err(|e| ImportError::Parse(format!("Read error: {}", e)))?;

    // Verify PDF magic bytes — more reliable than Content-Type (which can be wrong after redirects)
    if !buf.starts_with(b"%PDF") {
        return Err(ImportError::Parse(
            "Downloaded content is not a PDF (missing %PDF header)".to_string(),
        ));
    }

    std::fs::write(&dest, &buf)
        .map_err(|e| ImportError::Parse(format!("Write error: {}", e)))?;

    Ok(dest)
}

fn doi_to_filename(doi: &str) -> String {
    let slug: String = doi
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' { c } else { '_' })
        .collect();
    format!("{}.pdf", slug)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_doi_to_filename_simple() {
        assert_eq!(doi_to_filename("10.1234/foo"), "10_1234_foo.pdf");
    }

    #[test]
    fn test_doi_to_filename_slashes_become_underscores() {
        assert_eq!(
            doi_to_filename("10.1080/00295639.2025.2483123"),
            "10_1080_00295639_2025_2483123.pdf"
        );
    }

    #[test]
    fn test_doi_to_filename_hyphens_preserved() {
        assert_eq!(doi_to_filename("10.13182/NSE20-1234"), "10_13182_NSE20-1234.pdf");
    }

    #[test]
    fn test_doi_to_filename_special_chars_replaced() {
        assert_eq!(doi_to_filename("10.1234/foo:bar(baz)"), "10_1234_foo_bar_baz_.pdf");
    }

    #[test]
    fn test_imported_entry_new() {
        use indexmap::IndexMap;
        let mut fields = IndexMap::new();
        fields.insert("title".to_string(), "My Paper".to_string());
        fields.insert("year".to_string(), "2023".to_string());

        let entry = ImportedEntry::new("article", fields.clone());
        assert_eq!(entry.entry_type, "article");
        assert_eq!(entry.fields["title"], "My Paper");
        assert_eq!(entry.fields["year"], "2023");
        assert!(entry.pdf_urls.is_empty());
        assert!(entry.pdf_path.is_none());
        assert!(entry.pdf_error.is_none());
    }

    #[test]
    fn test_import_error_display_network() {
        let e = ImportError::Network("timeout".to_string());
        assert_eq!(e.to_string(), "Network error: timeout");
    }

    #[test]
    fn test_import_error_display_parse() {
        let e = ImportError::Parse("bad json".to_string());
        assert_eq!(e.to_string(), "Parse error: bad json");
    }

    #[test]
    fn test_import_error_display_no_match() {
        let e = ImportError::NoMatch("https://example.com".to_string());
        assert_eq!(e.to_string(), "No fetcher matched: https://example.com");
    }
}
