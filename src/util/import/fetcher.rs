use super::{ImportedEntry, ImportError};

/// A source that can fetch BibTeX metadata from a URL or DOI.
pub trait Fetcher: Send + Sync {
    /// Returns true if this fetcher can handle the given input.
    fn can_handle(&self, doi_or_url: &str) -> bool;

    /// Fetch and return the parsed entry.
    fn fetch(&self, doi_or_url: &str) -> Result<ImportedEntry, ImportError>;
}
