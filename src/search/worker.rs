//! A single cancellable worker. Query, sort, group and document changes all
//! advance one generation; a stale result can never become an active view.
use std::collections::HashMap;
use std::sync::{Arc, atomic::{AtomicU64, Ordering}, mpsc};
use crate::bib::model::Entry;
use super::{engine::CompiledQuery, index::SearchDocument};

pub(crate) const BACKGROUND_THRESHOLD: usize = 500;

struct Request {
    generation: u64,
    documents: Arc<Vec<Arc<SearchDocument>>>,
    query: String,
}

#[derive(Default)]
pub(crate) struct SearchWorker {
    generation: Arc<AtomicU64>,
    sender: Option<mpsc::Sender<Request>>,
    receiver: Option<mpsc::Receiver<(u64, Vec<String>)>>,
    cache: HashMap<String, Arc<SearchDocument>>,
    documents: Arc<Vec<Arc<SearchDocument>>>,
    #[cfg(test)]
    submitted: usize,
}

impl SearchWorker {
    pub fn cancel(&self) { self.generation.fetch_add(1, Ordering::Relaxed); }

    pub fn submit(&mut self, entries: &[&Entry], query: &str, changed: bool) {
        self.cancel();
        #[cfg(test)] { self.submitted += 1; }
        if changed || self.documents.len() != entries.len()
            || self.documents.iter().zip(entries).any(|(doc, entry)| doc.key != entry.citation_key) {
            let mut old = std::mem::take(&mut self.cache);
            let mut documents = Vec::with_capacity(entries.len());
            for entry in entries {
                let doc = old.remove(&entry.citation_key).filter(|doc| doc.matches(entry))
                    .unwrap_or_else(|| Arc::new(SearchDocument::new(entry)));
                self.cache.insert(entry.citation_key.clone(), Arc::clone(&doc));
                documents.push(doc);
            }
            self.documents = Arc::new(documents);
        }
        if self.sender.is_none() {
            let (sender, requests) = mpsc::channel::<Request>();
            let (results, receiver) = mpsc::channel();
            let generation = Arc::clone(&self.generation);
            std::thread::spawn(move || {
                let mut matcher = nucleo_matcher::Matcher::new(nucleo_matcher::Config::DEFAULT.match_paths());
                let mut buffer = Vec::new();
                while let Ok(mut request) = requests.recv() {
                    while let Ok(newer) = requests.try_recv() { request = newer; }
                    let query = CompiledQuery::new(&request.query);
                    let mut matches = Vec::new();
                    for document in request.documents.iter() {
                        if generation.load(Ordering::Relaxed) != request.generation { break; }
                        if let Some(score) = query.score(document, &mut matcher, &mut buffer) {
                            matches.push((document.key.clone(), score));
                        }
                    }
                    if generation.load(Ordering::Relaxed) != request.generation { continue; }
                    matches.sort_by_key(|(_, score)| std::cmp::Reverse(*score));
                    if results.send((request.generation, matches.into_iter().map(|(key, _)| key).collect())).is_err() { break; }
                }
            });
            self.sender = Some(sender);
            self.receiver = Some(receiver);
        }
        if let Some(sender) = &self.sender {
            let _ = sender.send(Request {
                generation: self.generation.load(Ordering::Relaxed),
                documents: Arc::clone(&self.documents), query: query.into(),
            });
        }
    }

    pub fn poll(&self) -> Option<Vec<String>> {
        let mut latest = None;
        if let Some(receiver) = &self.receiver {
            while let Ok((generation, result)) = receiver.try_recv() {
                if generation == self.generation.load(Ordering::Relaxed) { latest = Some(result); }
            }
        }
        latest
    }

    #[cfg(test)]
    pub fn submission_count(&self) -> usize { self.submitted }
}

impl Drop for SearchWorker {
    fn drop(&mut self) { self.cancel(); self.sender.take(); }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bib::model::EntryType;
    use indexmap::IndexMap;

    #[test]
    fn snapshots_reuse_unchanged_entries_and_reject_stale_results() {
        let mut entry = Entry { citation_key: "A".into(), entry_type: EntryType::Misc,
            fields: IndexMap::from([("title".into(), "old".into())]),
            group_memberships: vec![], raw_index: 0, dirty: false };
        let mut worker = SearchWorker::default();
        worker.submit(&[&entry], "old", true);
        let original = Arc::clone(&worker.documents[0]);
        worker.submit(&[&entry], "other", false);
        assert!(Arc::ptr_eq(&original, &worker.documents[0]));
        entry.fields.insert("title".into(), "new".into());
        worker.submit(&[&entry], "new", true);
        assert!(!Arc::ptr_eq(&original, &worker.documents[0]));
        assert_eq!(worker.documents[0].get(Some("title")), "new");
        let (sender, receiver) = mpsc::channel();
        worker.receiver = Some(receiver);
        let current = worker.generation.load(Ordering::Relaxed);
        sender.send((current - 1, vec!["stale".into()])).unwrap();
        assert!(worker.poll().is_none());
        sender.send((current, vec!["A".into()])).unwrap();
        assert_eq!(worker.poll(), Some(vec!["A".into()]));
        worker.cancel();
        sender.send((current, vec!["A".into()])).unwrap();
        assert!(worker.poll().is_none());
    }
}
