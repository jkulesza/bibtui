use super::{ImportedEntry, ImportError};
use super::http::{HttpClient, HttpTransport};

/// A source that can fetch BibTeX metadata from a URL or DOI.
pub trait Fetcher: Send + Sync {
    /// Returns true if this fetcher can handle the given input.
    fn can_handle(&self, doi_or_url: &str) -> bool;

    /// Fetch and return the parsed entry.
    fn fetch(&self, doi_or_url: &str) -> Result<ImportedEntry, ImportError> {
        self.fetch_with(doi_or_url, &HttpClient::new()?)
    }

    fn fetch_with(&self, doi_or_url: &str, http: &dyn HttpTransport) -> Result<ImportedEntry, ImportError>;
}
