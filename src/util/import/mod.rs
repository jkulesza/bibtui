pub mod ans;
pub mod crossref;
pub mod fetcher;
pub mod http;
pub mod isbn;
pub mod pdf;
pub mod pipeline;
pub mod tandfonline;

use indexmap::IndexMap;
use std::io::{Read, Write};
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
    download_pdf_with(
        pdf_url,
        dest_dir,
        doi,
        &http::HttpClient::new()?,
        100 * 1024 * 1024,
    )
}

pub fn download_pdf_with(
    pdf_url: &str,
    dest_dir: &Path,
    doi: &str,
    http: &dyn http::HttpTransport,
    max_bytes: u64,
) -> Result<PathBuf, ImportError> {
    let response = http.download(pdf_url)?;
    if max_bytes < 4
        || response
            .content_length
            .is_some_and(|length| length > max_bytes)
    {
        return Err(ImportError::Parse(format!(
            "PDF exceeds {} byte limit",
            max_bytes
        )));
    }
    let mut reader = response.body;
    let mut header = [0; 4];
    reader
        .read_exact(&mut header)
        .map_err(|error| ImportError::Network(error.to_string()))?;
    if &header != b"%PDF" {
        return Err(ImportError::Parse(
            "Downloaded content is not a PDF (missing %PDF header)".into(),
        ));
    }
    let mut temporary = crate::util::persistence::new_file_builder(".bibtui-download-")
        .tempfile_in(dest_dir)
        .map_err(|error| ImportError::Parse(error.to_string()))?;
    temporary
        .write_all(&header)
        .map_err(|error| ImportError::Parse(error.to_string()))?;
    let copied = std::io::copy(&mut reader.take(max_bytes - 4 + 1), &mut temporary)
        .map_err(|error| ImportError::Network(error.to_string()))?;
    if copied + 4 > max_bytes {
        return Err(ImportError::Parse(format!(
            "PDF exceeds {} byte limit",
            max_bytes
        )));
    }
    temporary
        .flush()
        .and_then(|_| temporary.as_file().sync_all())
        .map_err(|error| ImportError::Parse(error.to_string()))?;
    // Re-importing the same DOI reuses an identical earlier download; a
    // different existing file is never overwritten, so pick `<doi>_2.pdf`, …
    let stem = sanitize_filename_stem(doi);
    for n in 1..=1000 {
        let dest = if n == 1 {
            dest_dir.join(doi_to_filename(doi))
        } else {
            dest_dir.join(format!("{stem}_{n}.pdf"))
        };
        if dest.exists() {
            if files_equal(temporary.path(), &dest) {
                return Ok(dest);
            }
            continue;
        }
        match temporary.persist_noclobber(&dest) {
            Ok(_) => return Ok(dest),
            Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {
                temporary = error.file;
            }
            Err(error) => {
                return Err(ImportError::Parse(format!(
                    "Cannot create {}: {}",
                    dest.display(),
                    error.error
                )))
            }
        }
    }
    Err(ImportError::Parse(format!(
        "No free filename for {stem}.pdf in {}",
        dest_dir.display()
    )))
}

fn doi_to_filename(doi: &str) -> String {
    format!("{}.pdf", sanitize_filename_stem(doi))
}

fn files_equal(a: &Path, b: &Path) -> bool {
    let same_length = match (std::fs::metadata(a), std::fs::metadata(b)) {
        (Ok(a), Ok(b)) => a.len() == b.len(),
        _ => false,
    };
    same_length && matches!((std::fs::read(a), std::fs::read(b)), (Ok(a), Ok(b)) if a == b)
}

/// Sanitize a string for use as a filesystem filename stem.
///
/// Replaces any character that is not alphanumeric, `-`, `.`, or `_` with `_`,
/// then collapses consecutive underscores and strips leading/trailing ones.
/// This removes tildes, apostrophes, slashes, spaces, and other characters
/// that are problematic in filenames.
pub fn sanitize_filename_stem(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut prev_underscore = false;
    for c in s.chars() {
        if c.is_alphanumeric() || c == '-' || c == '.' {
            result.push(c);
            prev_underscore = false;
        } else {
            if !prev_underscore {
                result.push('_');
            }
            prev_underscore = true;
        }
    }
    result.trim_matches('_').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_doi_to_filename_simple() {
        assert_eq!(doi_to_filename("10.1234/foo"), "10.1234_foo.pdf");
    }

    #[test]
    fn test_doi_to_filename_slashes_become_underscores() {
        assert_eq!(
            doi_to_filename("10.1080/00295639.2025.2483123"),
            "10.1080_00295639.2025.2483123.pdf"
        );
    }

    #[test]
    fn test_doi_to_filename_hyphens_preserved() {
        assert_eq!(
            doi_to_filename("10.13182/NSE20-1234"),
            "10.13182_NSE20-1234.pdf"
        );
    }

    #[test]
    fn test_doi_to_filename_special_chars_replaced() {
        // Trailing special char is absorbed; no trailing underscore in output.
        assert_eq!(
            doi_to_filename("10.1234/foo:bar(baz)"),
            "10.1234_foo_bar_baz.pdf"
        );
    }

    // ── sanitize_filename_stem ────────────────────────────────────────────────

    #[test]
    fn test_sanitize_tilde_removed() {
        assert_eq!(sanitize_filename_stem("O~Brien2020"), "O_Brien2020");
    }

    #[test]
    fn test_sanitize_apostrophe_removed() {
        assert_eq!(sanitize_filename_stem("O'Brien2020"), "O_Brien2020");
    }

    #[test]
    fn test_sanitize_consecutive_special_collapsed() {
        assert_eq!(sanitize_filename_stem("foo::bar"), "foo_bar");
    }

    #[test]
    fn test_sanitize_leading_trailing_stripped() {
        assert_eq!(sanitize_filename_stem("~foo~"), "foo");
    }

    #[test]
    fn test_sanitize_hyphens_preserved() {
        assert_eq!(sanitize_filename_stem("Smith-Jones2020"), "Smith-Jones2020");
    }

    #[test]
    fn test_sanitize_alphanumeric_unchanged() {
        assert_eq!(sanitize_filename_stem("Smith2020"), "Smith2020");
    }

    #[test]
    fn test_sanitize_periods_preserved() {
        assert_eq!(sanitize_filename_stem("10.1234_foo"), "10.1234_foo");
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

    #[test]
    fn test_fetch_no_match_returns_no_match_error() {
        // A string that no fetcher handles should give NoMatch.
        let result = fetch("not-a-doi-or-url-or-file.bib");
        assert!(matches!(result, Err(ImportError::NoMatch(_))));
    }

    #[test]
    fn test_fetch_bare_doi_routes_to_crossref_not_no_match() {
        use fetcher::Fetcher;
        assert!(crossref::CrossrefFetcher.can_handle("10.9999/test.2099.9999999"));
    }

    #[test]
    fn test_imported_entry_fields_accessible() {
        let mut fields = IndexMap::new();
        fields.insert("doi".to_string(), "10.1234/x".to_string());
        fields.insert("author".to_string(), "Smith, John".to_string());
        let entry = ImportedEntry::new("article", fields);
        assert_eq!(entry.fields.get("doi"), Some(&"10.1234/x".to_string()));
        assert_eq!(entry.fields.get("author"), Some(&"Smith, John".to_string()));
        assert!(entry.pdf_urls.is_empty());
        assert!(entry.pdf_path.is_none());
        assert!(entry.pdf_error.is_none());
    }

    #[test]
    fn test_imported_entry_clone() {
        let mut fields = IndexMap::new();
        fields.insert("title".to_string(), "My Paper".to_string());
        let entry = ImportedEntry::new("book", fields);
        let cloned = entry.clone();
        assert_eq!(cloned.entry_type, "book");
        assert_eq!(cloned.fields["title"], "My Paper");
    }

    #[test]
    fn test_import_error_variants_are_debug() {
        let e1 = ImportError::Network("net error".to_string());
        let e2 = ImportError::Parse("parse error".to_string());
        let e3 = ImportError::NoMatch("no match".to_string());
        // Debug formatting must not panic.
        let _ = format!("{:?}", e1);
        let _ = format!("{:?}", e2);
        let _ = format!("{:?}", e3);
    }
    #[test]
    fn pdf_downloads_validate_limit_and_preserve_existing_files() {
        use http::tests::MockHttp;
        let dir = tempfile::tempdir().unwrap();
        let download = |body, limit| {
            download_pdf_with(
                "https://example.invalid/pdf",
                dir.path(),
                "10.1234/test",
                &MockHttp::new(vec![body]),
                limit,
            )
        };
        for (body, limit) in [
            (Ok("<html>not PDF"), 100),
            (Ok("%PDFtoo big"), 5),
            (Err("timeout"), 100),
        ] {
            assert!(download(body, limit).is_err());
            assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
        }
        let path = download(Ok("%PDFvalid"), 9).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"%PDFvalid");
        // Re-importing identical content reuses the existing file.
        assert_eq!(download(Ok("%PDFvalid"), 100).unwrap(), path);
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
        // Different content gets a new name; the original is untouched.
        let second = download(Ok("%PDFother"), 100).unwrap();
        assert_eq!(second.file_name().unwrap(), "10.1234_test_2.pdf");
        assert_eq!(std::fs::read(&second).unwrap(), b"%PDFother");
        assert_eq!(std::fs::read(&path).unwrap(), b"%PDFvalid");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 2);
    }

    #[test]
    fn interrupted_pdf_stream_cleans_up_only_its_temporary_file() {
        struct BrokenBody(bool);
        impl Read for BrokenBody {
            fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
                if !self.0 {
                    self.0 = true;
                    buffer[..4].copy_from_slice(b"%PDF");
                    Ok(4)
                } else {
                    Err(std::io::Error::new(
                        std::io::ErrorKind::ConnectionReset,
                        "interrupted transfer",
                    ))
                }
            }
        }
        struct BrokenHttp;
        impl http::HttpTransport for BrokenHttp {
            fn get(
                &self,
                _: &str,
                _: std::time::Duration,
            ) -> Result<http::HttpResponse, ImportError> {
                Ok(http::HttpResponse {
                    body: Box::new(BrokenBody(false)),
                    content_length: Some(10),
                })
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let unrelated = dir.path().join("unrelated.tmp");
        std::fs::write(&unrelated, "keep").unwrap();
        assert!(download_pdf_with(
            "https://example.invalid",
            dir.path(),
            "test",
            &BrokenHttp,
            100
        )
        .is_err());
        assert_eq!(std::fs::read_to_string(unrelated).unwrap(), "keep");
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
    }
}
