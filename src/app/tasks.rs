//! The app's background network work (imports and metadata DOI lookups),
//! behind a trait so tests decide when, and in what order, tasks finish.
//!
//! A task reports by sending its result on the channel it is given; the
//! event loop polls the receivers (see `App::poll_background_tasks`).

use std::path::PathBuf;
use std::sync::mpsc::Sender;

use super::DoiFetchResult;
use crate::util::import::ImportResult;

/// An import (`i` / `:import`): what to fetch and where PDFs go.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportRequest {
    /// The DOI, ISBN, URL or PDF path the user entered.
    pub query: String,
    /// Directory downloaded PDFs are saved in.
    pub pdf_dir: PathBuf,
    /// Largest PDF to download (`import.max_pdf_size_mb`).
    pub max_pdf_bytes: u64,
}

/// A metadata DOI search (`w` on an entry without a DOI).
#[derive(Debug, Clone, PartialEq)]
pub struct DoiQuery {
    pub title: String,
    pub author: String,
    pub year: String,
}

/// Runs background tasks, reporting each on its `done` channel.
pub trait BackgroundTasks {
    fn import(&self, request: ImportRequest, done: Sender<ImportResult>);
    fn lookup_doi(&self, query: DoiQuery, done: Sender<DoiFetchResult>);
}

/// The real tasks, each on its own thread with network access. (Test
/// builds use [`ManualTasks`] instead, so nothing there reaches the network.)
#[cfg_attr(test, allow(dead_code))]
pub struct ThreadTasks;

impl BackgroundTasks for ThreadTasks {
    fn import(&self, request: ImportRequest, done: Sender<ImportResult>) {
        std::thread::spawn(move || {
            let _ = done.send(run_import(&request));
        });
    }

    fn lookup_doi(&self, query: DoiQuery, done: Sender<DoiFetchResult>) {
        std::thread::spawn(move || {
            let result = crate::util::import::crossref::search_by_metadata(
                &query.title,
                &query.author,
                &query.year,
            );
            let _ = done.send(result);
        });
    }
}

/// Fetch the entry, then try its PDF links until one downloads.
#[cfg_attr(test, allow(dead_code))]
fn run_import(request: &ImportRequest) -> ImportResult {
    let http = crate::util::import::http::HttpClient::new()?;
    let mut entry = crate::util::import::pipeline::run_with(&request.query, &http)?;
    if entry.pdf_path.is_none() && !entry.pdf_urls.is_empty() {
        let doi = entry
            .fields
            .get("doi")
            .cloned()
            .unwrap_or_else(|| "import".to_string());
        for pdf_url in &entry.pdf_urls {
            match crate::util::import::download_pdf_with(
                pdf_url,
                &request.pdf_dir,
                &doi,
                &http,
                request.max_pdf_bytes,
            ) {
                Ok(path) => {
                    entry.pdf_path = Some(path);
                    entry.pdf_error = None;
                    break;
                }
                Err(error) => entry.pdf_error = Some(error.to_string()),
            }
        }
    }
    Ok(entry)
}

/// Tasks that run only when a test finishes them. Clones share one queue,
/// so a test keeps a clone after installing one in the app.
#[cfg(test)]
#[derive(Clone, Default)]
pub struct ManualTasks {
    queue: std::rc::Rc<std::cell::RefCell<Queue>>,
}

#[cfg(test)]
#[derive(Default)]
struct Queue {
    imports: Vec<(ImportRequest, Sender<ImportResult>)>,
    lookups: Vec<(DoiQuery, Sender<DoiFetchResult>)>,
}

#[cfg(test)]
impl BackgroundTasks for ManualTasks {
    fn import(&self, request: ImportRequest, done: Sender<ImportResult>) {
        self.queue.borrow_mut().imports.push((request, done));
    }

    fn lookup_doi(&self, query: DoiQuery, done: Sender<DoiFetchResult>) {
        self.queue.borrow_mut().lookups.push((query, done));
    }
}

#[cfg(test)]
impl ManualTasks {
    /// Replace `app`'s tasks with a fresh queue and return a handle to it.
    pub(super) fn install(app: &mut super::App) -> Self {
        let tasks = ManualTasks::default();
        app.tasks = Box::new(tasks.clone());
        tasks
    }

    /// The imports started and not yet finished, oldest first.
    pub fn imports(&self) -> Vec<ImportRequest> {
        self.queue
            .borrow()
            .imports
            .iter()
            .map(|(r, _)| r.clone())
            .collect()
    }

    /// The DOI lookups started and not yet finished, oldest first.
    pub fn lookups(&self) -> Vec<DoiQuery> {
        self.queue
            .borrow()
            .lookups
            .iter()
            .map(|(q, _)| q.clone())
            .collect()
    }

    /// Finish the oldest unfinished import with `result`.
    pub fn finish_import(&self, result: ImportResult) {
        let (_, done) = self.queue.borrow_mut().imports.remove(0);
        done.send(result)
            .expect("the app still waits for this import");
    }

    /// Finish the oldest unfinished DOI lookup with `result`.
    pub fn finish_lookup(&self, result: DoiFetchResult) {
        let (_, done) = self.queue.borrow_mut().lookups.remove(0);
        done.send(result)
            .expect("the app still waits for this lookup");
    }

    /// End the oldest import without a result, as when its thread dies.
    pub fn abandon_import(&self) {
        drop(self.queue.borrow_mut().imports.remove(0));
    }
}
