//! Duplicate-entry detection.
//!
//! Two entries are reported as possible duplicates when they share a
//! normalized identifier (DOI, ISBN, eprint, or URL), or when their
//! normalized titles are at least `title_threshold` similar and — unless the
//! settings relax it — their year and first-author surname agree.
//!
//! Fields that identify a *distinct* work (different DOIs, ISBNs, eprints,
//! editions, volumes, or report numbers) veto a title match, so a second
//! edition or a different volume is not flagged.
//!
//! Matching pairs are grouped into clusters transitively (A~B and B~C put
//! A, B, and C in one cluster), strongest reason first, unless that would
//! put two entries the user marked "not duplicates", or two evidently
//! different works (different DOIs, ISBNs, eprints, editions, volumes,
//! numbers, or parts of one book), into one cluster. Every cluster can
//! therefore be merged as a whole; a match kept out is reported alongside
//! as a related candidate.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

use indexmap::IndexMap;

use super::model::Entry;
use crate::util::latex::render_latex;
use crate::util::titlecase::strip_case_braces;

/// A URL shared by more than this many entries is treated as a generic
/// landing page (e.g. a publisher's home page), not an identifier.
const MAX_URL_SHARERS: usize = 4;

/// Matching settings (mirrors `config.duplicates`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DedupSettings {
    /// Minimum normalized title similarity (0.0–1.0) for a title match.
    pub title_threshold: f64,
    /// A title match requires equal years (when both entries have one).
    pub require_same_year: bool,
    /// A title match requires equal first-author surnames (when both have one).
    pub require_same_author: bool,
}

impl Default for DedupSettings {
    fn default() -> Self {
        DedupSettings {
            title_threshold: 0.9,
            require_same_year: true,
            require_same_author: true,
        }
    }
}

/// Why two entries were matched.
#[derive(Debug, Clone, PartialEq)]
pub enum MatchReason {
    Doi,
    Isbn,
    Eprint,
    Url,
    /// Similar titles; `similarity` is in 0.0–1.0.
    Title {
        similarity: f64,
    },
}

impl MatchReason {
    pub fn describe(&self) -> String {
        match self {
            MatchReason::Doi => "same DOI".to_string(),
            MatchReason::Isbn => "same ISBN".to_string(),
            MatchReason::Eprint => "same eprint".to_string(),
            MatchReason::Url => "same URL".to_string(),
            MatchReason::Title { similarity } => {
                format!("title {:.0}% similar", similarity * 100.0)
            }
        }
    }
}

/// One matched pair of citation keys.
#[derive(Debug, Clone, PartialEq)]
pub struct DuplicatePair {
    pub a: String,
    pub b: String,
    pub reason: MatchReason,
}

/// A group of entries that are (transitively) possible duplicates.
#[derive(Debug, Clone, PartialEq)]
pub struct DuplicateCluster {
    /// Member citation keys, in library order.
    pub keys: Vec<String>,
    /// The pairwise matches between members.
    pub pairs: Vec<DuplicatePair>,
    /// Matches between a member and an entry outside the cluster that were
    /// found but not joined, with what kept them apart. These are candidates
    /// to look at, not part of the all-members merge.
    pub related: Vec<RelatedMatch>,
}

/// Why a match was not joined into a cluster: joining it would have put
/// `a` and `b` (in library order) into one cluster.
#[derive(Debug, Clone, PartialEq)]
pub enum Separation {
    /// The user marked `a` and `b` "not duplicates".
    MarkedDistinct { a: String, b: String },
    /// `a` and `b` are evidently different works; `why` completes
    /// "a and b …", e.g. "have different DOIs".
    Conflict {
        a: String,
        b: String,
        why: &'static str,
    },
}

impl Separation {
    pub fn describe(&self) -> String {
        match self {
            Separation::MarkedDistinct { a, b } => {
                format!("{a} and {b} are marked not duplicates")
            }
            Separation::Conflict { a, b, why } => format!("{a} and {b} {why}"),
        }
    }
}

/// A match found but kept out of a cluster.
#[derive(Debug, Clone, PartialEq)]
pub struct RelatedMatch {
    pub pair: DuplicatePair,
    pub separation: Separation,
}

/// Comparison data extracted once per entry.
#[derive(Debug, Clone, Default)]
struct Fingerprint {
    doi: Option<String>,
    isbns: Vec<String>,
    eprint: Option<String>,
    url: Option<String>,
    title: Vec<char>,
    year: Option<String>,
    author: Option<String>,
    edition: Option<String>,
    volume: Option<String>,
    number: Option<String>,
    chapter: Option<String>,
    pages: Option<String>,
}

fn field<'a>(entry: &'a Entry, name: &str) -> Option<&'a str> {
    entry
        .fields
        .get(name)
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
}

/// Identifier text without braces, LaTeX underscore escapes, or surrounding space.
fn clean_identifier(value: &str) -> String {
    strip_case_braces(value)
        .replace("\\_", "_")
        .trim()
        .to_lowercase()
}

/// LaTeX rendered to Unicode and braces removed. Rendering is skipped when
/// there is no command or math to render: dashes and `~` only matter as
/// separators, and callers drop punctuation anyway.
fn plain_text(value: &str) -> String {
    if value.contains(['\\', '$']) {
        strip_case_braces(&render_latex(value))
    } else {
        strip_case_braces(value)
    }
}

/// Lowercase alphanumerics only (for short comparison fields).
fn alnum(value: &str) -> Option<String> {
    let rendered = plain_text(value);
    let s: String = rendered
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect();
    (!s.is_empty()).then_some(s)
}

/// Normalize a DOI: lowercase, without a resolver prefix or `doi:` scheme.
pub fn normalize_doi(value: &str) -> Option<String> {
    let mut s = clean_identifier(value);
    for prefix in [
        "https://doi.org/",
        "http://doi.org/",
        "https://dx.doi.org/",
        "http://dx.doi.org/",
        "doi.org/",
        "dx.doi.org/",
        "doi:",
    ] {
        if let Some(rest) = s.strip_prefix(prefix) {
            s = rest.trim().to_string();
            break;
        }
    }
    s.starts_with("10.").then_some(s)
}

/// Normalize every ISBN in a field value to ISBN-13 digits.
pub fn normalize_isbns(value: &str) -> Vec<String> {
    let mut out = Vec::new();
    for token in value.split([',', ';', '/', ' ']) {
        let compact: String = token
            .chars()
            .filter(|c| c.is_ascii_digit() || *c == 'X' || *c == 'x')
            .map(|c| c.to_ascii_uppercase())
            .collect();
        let isbn = match compact.len() {
            13 if compact.chars().all(|c| c.is_ascii_digit()) => Some(compact),
            10 => isbn10_to_13(&compact),
            _ => None,
        };
        if let Some(isbn) = isbn {
            if !out.contains(&isbn) {
                out.push(isbn);
            }
        }
    }
    out
}

fn isbn10_to_13(isbn10: &str) -> Option<String> {
    let body = &isbn10[..9];
    if !body.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let digits = format!("978{}", body);
    let sum: u32 = digits
        .chars()
        .filter_map(|c| c.to_digit(10))
        .enumerate()
        .map(|(i, d)| if i % 2 == 0 { d } else { d * 3 })
        .sum();
    let check = (10 - sum % 10) % 10;
    Some(format!("{}{}", digits, check))
}

/// Normalize an eprint id: lowercase, no `arXiv:` prefix or version suffix.
pub fn normalize_eprint(value: &str) -> Option<String> {
    let s = clean_identifier(value);
    let s = s.strip_prefix("arxiv:").unwrap_or(&s).trim().to_string();
    let s = match s.rfind('v') {
        Some(pos)
            if pos > 0 && pos + 1 < s.len() && s[pos + 1..].chars().all(|c| c.is_ascii_digit()) =>
        {
            s[..pos].to_string()
        }
        _ => s,
    };
    (!s.is_empty()).then_some(s)
}

/// Normalize a URL for comparison: no scheme, `www.`, or trailing slash.
/// Returns `None` for a bare host with no path, which identifies a site
/// rather than a work.
pub fn normalize_url(value: &str) -> Option<String> {
    let s = clean_identifier(value);
    let s = s
        .strip_prefix("https://")
        .or_else(|| s.strip_prefix("http://"))
        .unwrap_or(&s);
    let s = s.strip_prefix("www.").unwrap_or(s);
    let s = s.trim_end_matches('/');
    let (_, path) = s.split_once('/')?;
    (!path.is_empty()).then(|| s.to_string())
}

/// Normalize a title for fuzzy comparison: LaTeX rendered, braces removed,
/// lowercased, punctuation dropped, and whitespace collapsed.
pub fn normalize_title(value: &str) -> String {
    let rendered = plain_text(value);
    let spaced: String = rendered
        .chars()
        .flat_map(|c| c.to_lowercase())
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect();
    spaced.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Lowercase surname of the first author (or editor).
fn first_surname(entry: &Entry) -> Option<String> {
    first_surname_in(field(entry, "author").or_else(|| field(entry, "editor"))?)
}

/// Lowercase alphanumeric comparison key for one surname or organization
/// name, with LaTeX rendered (`G{\"o}del` and `Gödel` agree).
pub fn surname_key(name: &str) -> Option<String> {
    alnum(name)
}

/// Comparison key ([`surname_key`]) of the first name in a BibTeX name list;
/// a brace-protected organization counts as one name.
pub fn first_surname_in(names: &str) -> Option<String> {
    let first = *crate::util::author::split_name_list(names).first()?;
    if let Some(literal) = crate::util::author::literal_name(first) {
        return alnum(literal);
    }
    let surname = match first.split_once(',') {
        Some((last, _)) => last.trim(),
        None => first.split_whitespace().next_back().unwrap_or(first),
    };
    alnum(surname)
}

fn year_of(entry: &Entry) -> Option<String> {
    let raw = field(entry, "year").or_else(|| field(entry, "date"))?;
    let digits: String = raw.chars().filter(|c| c.is_ascii_digit()).take(4).collect();
    (digits.len() == 4).then_some(digits)
}

/// Normalize an edition: "2nd", "Second", and "2" all compare equal.
fn normalize_edition(value: &str) -> Option<String> {
    const ORDINALS: &[&str] = &[
        "first", "second", "third", "fourth", "fifth", "sixth", "seventh", "eighth", "ninth",
        "tenth",
    ];
    let s = alnum(value)?;
    let digits: String = s.chars().take_while(|c| c.is_ascii_digit()).collect();
    if !digits.is_empty() {
        return Some(digits);
    }
    for (i, word) in ORDINALS.iter().enumerate() {
        if s.starts_with(word) {
            return Some((i + 1).to_string());
        }
    }
    Some(s)
}

fn fingerprint(entry: &Entry) -> Fingerprint {
    let doi = field(entry, "doi").and_then(normalize_doi).or_else(|| {
        // A doi.org link in `url` identifies the DOI too.
        field(entry, "url")
            .and_then(normalize_doi)
            .filter(|_| field(entry, "url").is_some_and(|u| u.contains("doi.org")))
    });
    Fingerprint {
        doi,
        isbns: field(entry, "isbn")
            .map(normalize_isbns)
            .unwrap_or_default(),
        eprint: field(entry, "eprint").and_then(normalize_eprint),
        url: field(entry, "url").and_then(normalize_url),
        title: field(entry, "title")
            .map(normalize_title)
            .unwrap_or_default()
            .chars()
            .collect(),
        year: year_of(entry),
        author: first_surname(entry),
        edition: field(entry, "edition").and_then(normalize_edition),
        volume: field(entry, "volume").and_then(alnum),
        number: field(entry, "number").and_then(alnum),
        chapter: field(entry, "chapter").and_then(alnum),
        pages: field(entry, "pages").and_then(alnum),
    }
}

/// Both present and different.
fn differ(a: &Option<String>, b: &Option<String>) -> bool {
    matches!((a, b), (Some(x), Some(y)) if x != y)
}

/// Both present and equal.
fn agree(a: &Option<String>, b: &Option<String>) -> bool {
    matches!((a, b), (Some(x), Some(y)) if x == y)
}

/// Fields that, when both present and different, mark distinct works.
fn distinct_works(a: &Fingerprint, b: &Fingerprint) -> bool {
    differ(&a.doi, &b.doi)
        || differ(&a.eprint, &b.eprint)
        || (!a.isbns.is_empty()
            && !b.isbns.is_empty()
            && !a.isbns.iter().any(|i| b.isbns.contains(i)))
        || differ(&a.edition, &b.edition)
        || differ(&a.volume, &b.volume)
        || differ(&a.number, &b.number)
}

/// Parts of one book (chapters, volumes) share its ISBN.
fn different_parts(a: &Fingerprint, b: &Fingerprint) -> bool {
    differ(&a.volume, &b.volume) || differ(&a.chapter, &b.chapter) || differ(&a.pages, &b.pages)
}

fn match_pair(a: &Fingerprint, b: &Fingerprint, settings: &DedupSettings) -> Option<MatchReason> {
    if agree(&a.doi, &b.doi) {
        return Some(MatchReason::Doi);
    }
    if distinct_works(a, b) {
        return None;
    }
    if agree(&a.eprint, &b.eprint) {
        return Some(MatchReason::Eprint);
    }
    if a.isbns.iter().any(|i| b.isbns.contains(i)) && !different_parts(a, b) {
        return Some(MatchReason::Isbn);
    }
    if agree(&a.url, &b.url) {
        return Some(MatchReason::Url);
    }
    if a.title.is_empty() || b.title.is_empty() {
        return None;
    }
    if settings.require_same_year && differ(&a.year, &b.year) {
        return None;
    }
    if settings.require_same_author && differ(&a.author, &b.author) {
        return None;
    }
    let similarity = title_similarity(&a.title, &b.title, settings.title_threshold)?;
    Some(MatchReason::Title { similarity })
}

/// Normalized Levenshtein similarity of two [`normalize_title`]d titles, or
/// `None` when it is below `threshold`.
pub fn normalized_title_similarity(a: &str, b: &str, threshold: f64) -> Option<f64> {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    title_similarity(&a, &b, threshold)
}

/// Normalized Levenshtein similarity of two titles, or `None` when it is
/// below `threshold` (computed with a band so dissimilar pairs exit early).
fn title_similarity(a: &[char], b: &[char], threshold: f64) -> Option<f64> {
    let longest = a.len().max(b.len());
    if longest == 0 {
        return None;
    }
    let threshold = threshold.clamp(0.0, 1.0);
    let max_distance = ((1.0 - threshold) * longest as f64).floor() as usize;
    let distance = bounded_levenshtein(a, b, max_distance)?;
    let similarity = 1.0 - distance as f64 / longest as f64;
    (similarity >= threshold).then_some(similarity)
}

/// Levenshtein distance, or `None` if it exceeds `max`.
fn bounded_levenshtein(a: &[char], b: &[char], max: usize) -> Option<usize> {
    if a.len().abs_diff(b.len()) > max {
        return None;
    }
    if a == b {
        return Some(0);
    }
    let big = max + 1;
    let mut prev: Vec<usize> = (0..=b.len()).map(|j| j.min(big)).collect();
    let mut cur = vec![big; b.len() + 1];
    for i in 1..=a.len() {
        let lo = i.saturating_sub(max).max(1);
        let hi = (i + max).min(b.len());
        // Only the band (plus one boundary cell each side) is ever read.
        cur[lo - 1] = if lo == 1 { i.min(big) } else { big };
        if hi < b.len() {
            cur[hi + 1] = big;
        }
        let mut row_min = cur[lo - 1];
        for j in lo..=hi {
            let cost = usize::from(a[i - 1] != b[j - 1]);
            let value = (prev[j - 1] + cost).min(prev[j] + 1).min(cur[j - 1] + 1);
            cur[j] = value.min(big);
            row_min = row_min.min(cur[j]);
        }
        if row_min > max {
            return None;
        }
        std::mem::swap(&mut prev, &mut cur);
    }
    let distance = prev[b.len()];
    (distance <= max).then_some(distance)
}

/// True when the user marked a set containing both keys "not duplicates".
pub fn is_ignored(a: &str, b: &str, ignored: &[Vec<String>]) -> bool {
    ignored
        .iter()
        .any(|set| set.iter().any(|k| k == a) && set.iter().any(|k| k == b))
}

/// "Not duplicates" sets indexed by key, for constant-time pair lookups.
struct Marks<'a> {
    sets_of: HashMap<&'a str, Vec<usize>>,
}

impl<'a> Marks<'a> {
    fn new(ignored: &'a [Vec<String>]) -> Self {
        let mut sets_of: HashMap<&str, Vec<usize>> = HashMap::new();
        for (n, set) in ignored.iter().enumerate() {
            for key in set {
                let sets = sets_of.entry(key.as_str()).or_default();
                if sets.last() != Some(&n) {
                    sets.push(n);
                }
            }
        }
        Marks { sets_of }
    }

    /// True when one marked set contains both keys.
    fn contains(&self, a: &str, b: &str) -> bool {
        match (self.sets_of.get(a), self.sets_of.get(b)) {
            (Some(x), Some(y)) => x.iter().any(|n| y.contains(n)),
            _ => false,
        }
    }
}

/// Fingerprints for every entry, with generic (widely shared) URLs dropped.
fn fingerprints(entries: &IndexMap<String, Entry>) -> Vec<Fingerprint> {
    let mut prints: Vec<Fingerprint> = entries.values().map(fingerprint).collect();
    let mut url_counts: HashMap<String, usize> = HashMap::new();
    for print in &prints {
        if let Some(url) = &print.url {
            *url_counts.entry(url.clone()).or_default() += 1;
        }
    }
    for print in &mut prints {
        if print
            .url
            .as_ref()
            .is_some_and(|u| url_counts.get(u).copied().unwrap_or(0) > MAX_URL_SHARERS)
        {
            print.url = None;
        }
    }
    prints
}

/// Most deletion variants a title may generate for the deletion index.
const DELETION_CAP: usize = 256;

/// Call `emit(i, j)` for every pair of entries that [`match_pair`] could
/// accept (and possibly others; a pair may be emitted more than once, but
/// always with `i < j`): pairs sharing a DOI, eprint, ISBN or URL, and pairs
/// whose titles could be `title_threshold` similar. The comparison itself
/// (including the year, author and distinct-work checks) is left to
/// `match_pair`, so a missing year or author still allows a title match.
/// Nothing is collected, so memory stays linear in the library size.
/// Stops early once `cancel` is set.
fn for_each_candidate(
    prints: &[Fingerprint],
    settings: &DedupSettings,
    cancel: &AtomicBool,
    emit: &mut dyn FnMut(usize, usize),
) {
    let mut by_identifier: HashMap<(u8, &str), Vec<usize>> = HashMap::new();
    for (i, print) in prints.iter().enumerate() {
        let ids = [(0, &print.doi), (1, &print.eprint), (2, &print.url)];
        for (kind, id) in ids {
            if let Some(id) = id {
                by_identifier
                    .entry((kind, id.as_str()))
                    .or_default()
                    .push(i);
            }
        }
        for isbn in &print.isbns {
            by_identifier.entry((3, isbn.as_str())).or_default().push(i);
        }
    }
    for bucket in by_identifier.values() {
        if cancelled(cancel) {
            return;
        }
        for (n, &i) in bucket.iter().enumerate() {
            for &j in &bucket[n + 1..] {
                emit(i, j);
            }
        }
    }

    title_candidates(prints, settings.title_threshold, cancel, emit);
}

fn cancelled(cancel: &AtomicBool) -> bool {
    cancel.load(Ordering::Relaxed)
}

/// Most edits a title of `len` characters can be from a `threshold`-similar
/// title (whose length is at most `len / threshold`).
fn max_title_edits(len: usize, threshold: f64) -> usize {
    ((1.0 - threshold) * len as f64 / threshold + 1e-9).floor() as usize
}

/// Number of ways to delete at most `k` (≤ 2) of `len` characters.
fn deletion_variants(len: usize, k: usize) -> usize {
    match k {
        0 => 1,
        1 => 1 + len,
        _ => 1 + len + len * len.saturating_sub(1) / 2,
    }
}

/// Hashes of `title` with every choice of at most `k` (≤ 2) characters
/// deleted, tagged with `entry`. Hash collisions only add candidates.
fn push_deletion_hashes(title: &[char], k: usize, entry: u32, out: &mut Vec<(u64, u32)>) {
    let hash = |skip: [usize; 2]| {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for (n, &c) in title.iter().enumerate() {
            if n != skip[0] && n != skip[1] {
                h = (h ^ c as u64).wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
        h
    };
    let none = usize::MAX;
    out.push((hash([none, none]), entry));
    if k >= 1 {
        for p in 0..title.len() {
            out.push((hash([p, none]), entry));
            if k >= 2 {
                out.extend((p + 1..title.len()).map(|q| (hash([p, q]), entry)));
            }
        }
    }
}

/// Emit the title pairs that pass the title filters (see
/// [`for_each_candidate`]). Pairs of entries that both have a DOI are
/// skipped: equal DOIs are already paired, and different ones never match.
///
/// Two titles may match only within `e` edits, where `e` is at most each
/// title's own bound `k` ([`max_title_edits`]), and are then found without
/// false negatives:
///
/// - Titles with `k ≤ 2` and few variants go in a deletion index: titles
///   within `e` edits share a variant with at most `e` characters deleted
///   from each. Other titles close enough in length to one of those add
///   variants too (up to that title's `k`), so those pairs meet there.
/// - Other titles are cut into `k + 1` segments (segment partitioning, as in
///   PassJoin). `e ≤ k` edits leave at least one segment untouched, and it
///   appears in the other title shifted by at most `k` places (and, from the
///   end, by at most `k` from the length difference). Each title looks up
///   those substrings in the segment index of every compatible length.
/// - A title too short for `k + 1` non-empty segments (very low thresholds)
///   is compared with every title.
fn title_candidates(
    prints: &[Fingerprint],
    threshold: f64,
    cancel: &AtomicBool,
    emit: &mut dyn FnMut(usize, usize),
) {
    let titled: Vec<usize> = (0..prints.len())
        .filter(|&i| !prints[i].title.is_empty())
        .collect();
    let threshold = threshold.clamp(0.0, 1.0);
    let edits = |len: usize| {
        if threshold > 0.0 {
            max_title_edits(len, threshold)
        } else {
            len
        }
    };
    let has_doi = |i: usize| prints[i].doi.is_some();
    let mut emit = |i: usize, j: usize| {
        if i != j && !(has_doi(i) && has_doi(j)) {
            emit(i.min(j), i.max(j));
        }
    };

    let mut variants: Vec<(u64, u32)> = Vec::new();
    // Largest deletion budget of an indexed title, by title length.
    let mut indexed_k: HashMap<usize, usize> = HashMap::new();
    let mut others: Vec<usize> = Vec::new();
    for &i in &titled {
        let len = prints[i].title.len();
        let k = edits(len);
        if k <= 2 && deletion_variants(len, k) <= DELETION_CAP {
            push_deletion_hashes(&prints[i].title, k, i as u32, &mut variants);
            let best = indexed_k.entry(len).or_default();
            *best = (*best).max(k);
        } else {
            others.push(i);
        }
    }

    let mut segmented: Vec<usize> = Vec::new();
    let mut unfiltered: Vec<usize> = Vec::new();
    for &i in &others {
        let title = &prints[i].title;
        let len = title.len();
        // A partner in the deletion index has length within its own budget.
        let partner_k = (len.saturating_sub(2)..=len + 2)
            .filter_map(|l| indexed_k.get(&l).filter(|&&k| l.abs_diff(len) <= k))
            .max();
        if let Some(&k) = partner_k {
            push_deletion_hashes(title, k, i as u32, &mut variants);
        }
        if edits(len) < len {
            segmented.push(i);
        } else {
            unfiltered.push(i);
        }
    }

    variants.sort_unstable();
    variants.dedup();
    for group in variants.chunk_by(|a, b| a.0 == b.0) {
        if cancelled(cancel) {
            return;
        }
        for (n, &(_, i)) in group.iter().enumerate() {
            for &(_, j) in &group[n + 1..] {
                emit(i as usize, j as usize);
            }
        }
    }

    // Segment index of titles without a DOI, keyed by (title length,
    // segment number, segment hash). Every segmented title probes it, so
    // each pair with at most one DOI is found: when neither has one, by the
    // later entry probing the earlier one's segments.
    let hashes: HashMap<usize, PrefixHash> = segmented
        .iter()
        .map(|&i| (i, PrefixHash::new(&prints[i].title)))
        .collect();
    let mut index: HashMap<(usize, usize, u64), Vec<usize>> = HashMap::new();
    let mut lengths: Vec<usize> = Vec::new();
    for &i in &segmented {
        if has_doi(i) {
            continue;
        }
        let len = prints[i].title.len();
        lengths.push(len);
        for (n, (start, seg_len)) in segments(len, edits(len)).enumerate() {
            let hash = hashes[&i].substring(start, seg_len);
            index.entry((len, n, hash)).or_default().push(i);
        }
    }
    lengths.sort_unstable();
    lengths.dedup();

    let mut last_paired = vec![usize::MAX; prints.len()];
    for &i in &segmented {
        if cancelled(cancel) {
            return;
        }
        let probe = &hashes[&i];
        let len = prints[i].title.len();
        // Indexed lengths `l` whose own budget allows the length difference.
        let low = lengths.partition_point(|&l| l + edits(l) < len);
        for &l in &lengths[low..] {
            if l > len + edits(l) {
                // `l - edits(l)` grows with `l`, so no longer length fits.
                break;
            }
            let k = edits(l) as isize;
            let delta = len as isize - l as isize;
            for (n, (start, seg_len)) in segments(l, edits(l)).enumerate() {
                let shift_lo = (-k).max(delta - k);
                let shift_hi = k.min(delta + k);
                for shift in shift_lo..=shift_hi {
                    let at = start as isize + shift;
                    if at < 0 || at as usize + seg_len > len {
                        continue;
                    }
                    let hash = probe.substring(at as usize, seg_len);
                    let Some(found) = index.get(&(l, n, hash)) else {
                        continue;
                    };
                    for &j in found {
                        if (has_doi(i) || j < i) && last_paired[j] != i {
                            last_paired[j] = i;
                            emit(j, i);
                        }
                    }
                }
            }
        }
    }

    for &i in &unfiltered {
        if cancelled(cancel) {
            return;
        }
        for &j in &titled {
            emit(i, j);
        }
    }
}

/// `(start, length)` of the `k + 1` segments of a title of `len` (> `k`)
/// characters; the last `len % (k + 1)` segments are one longer.
fn segments(len: usize, k: usize) -> impl Iterator<Item = (usize, usize)> {
    let count = k + 1;
    let base = len / count;
    let longer_from = count - len % count;
    (0..count).map(move |n| {
        let start = n * base + n.saturating_sub(longer_from);
        (start, base + usize::from(n >= longer_from))
    })
}

/// Polynomial prefix hashes of a title, for O(1) substring hashes.
struct PrefixHash {
    prefix: Vec<u64>,
    power: Vec<u64>,
}

impl PrefixHash {
    const BASE: u64 = 0x0000_0100_0000_01b3;

    fn new(title: &[char]) -> Self {
        let mut prefix = Vec::with_capacity(title.len() + 1);
        let mut power = Vec::with_capacity(title.len() + 1);
        prefix.push(0u64);
        power.push(1u64);
        for &c in title {
            let last = prefix[prefix.len() - 1];
            prefix.push(last.wrapping_mul(Self::BASE).wrapping_add(c as u64 + 1));
            power.push(power[power.len() - 1].wrapping_mul(Self::BASE));
        }
        PrefixHash { prefix, power }
    }

    fn substring(&self, start: usize, len: usize) -> u64 {
        self.prefix[start + len].wrapping_sub(self.prefix[start].wrapping_mul(self.power[len]))
    }
}

/// Why two entries that did not match each other are evidently different
/// works, so no cluster may contain both, or `None`. Only identifiers and
/// numbering count: a different year or author alone does not, since
/// preprints and corrected records legitimately differ there.
fn conflict(a: &Fingerprint, b: &Fingerprint) -> Option<&'static str> {
    if agree(&a.doi, &b.doi) {
        return None;
    }
    if differ(&a.doi, &b.doi) {
        Some("have different DOIs")
    } else if differ(&a.eprint, &b.eprint) {
        Some("have different eprints")
    } else if !a.isbns.is_empty()
        && !b.isbns.is_empty()
        && !a.isbns.iter().any(|i| b.isbns.contains(i))
    {
        Some("have different ISBNs")
    } else if differ(&a.edition, &b.edition) {
        Some("have different editions")
    } else if differ(&a.volume, &b.volume) {
        Some("have different volumes")
    } else if differ(&a.number, &b.number) {
        Some("have different numbers")
    } else if a.isbns.iter().any(|i| b.isbns.contains(i)) && different_parts(a, b) {
        Some("are different parts of one book")
    } else {
        None
    }
}

/// Order in which matches are joined into clusters: the most reliable
/// reasons first, so a weak link cannot claim an entry that a strong one
/// would have put elsewhere.
fn strength(reason: &MatchReason) -> (u8, std::cmp::Reverse<u64>) {
    match reason {
        MatchReason::Doi => (0, std::cmp::Reverse(0)),
        MatchReason::Eprint => (1, std::cmp::Reverse(0)),
        MatchReason::Isbn => (2, std::cmp::Reverse(0)),
        MatchReason::Url => (3, std::cmp::Reverse(0)),
        MatchReason::Title { similarity } => (
            4,
            std::cmp::Reverse((similarity.clamp(0.0, 1.0) * 1e6).round() as u64),
        ),
    }
}

/// Find all clusters of possible duplicates in `entries`.
///
/// Matches are joined strongest first. A join is refused when it would put
/// two entries the user marked "not duplicates", or two evidently different
/// works (see [`conflict`]), into one cluster, so every cluster is safe to
/// merge as a whole. A refused match is reported in
/// [`DuplicateCluster::related`] of the cluster(s) of its entries.
///
/// Only plausible pairs are compared (see [`for_each_candidate`]), so this
/// is not quadratic in the library size for ordinary libraries.
pub fn find_duplicates(
    entries: &IndexMap<String, Entry>,
    settings: &DedupSettings,
    ignored: &[Vec<String>],
) -> Vec<DuplicateCluster> {
    find_duplicates_cancellable(entries, settings, ignored, &AtomicBool::new(false))
        .unwrap_or_default()
}

/// [`find_duplicates`], giving up with `None` soon after `cancel` is set
/// (from another thread).
pub fn find_duplicates_cancellable(
    entries: &IndexMap<String, Entry>,
    settings: &DedupSettings,
    ignored: &[Vec<String>],
    cancel: &AtomicBool,
) -> Option<Vec<DuplicateCluster>> {
    let keys: Vec<&String> = entries.keys().collect();
    let prints = fingerprints(entries);
    let n = keys.len();

    let marks = Marks::new(ignored);
    let mut matches: Vec<(usize, usize, MatchReason)> = Vec::new();
    for_each_candidate(&prints, settings, cancel, &mut |i, j| {
        if !marks.contains(keys[i], keys[j]) {
            if let Some(reason) = match_pair(&prints[i], &prints[j], settings) {
                matches.push((i, j, reason));
            }
        }
    });
    if cancelled(cancel) {
        return None;
    }
    // A pair found by several indexes is the same match.
    matches.sort_by_key(|(i, j, _)| (*i, *j));
    matches.dedup_by_key(|(i, j, _)| (*i, *j));
    matches.sort_by_key(|(i, j, reason)| (strength(reason), *i, *j));

    // Why `x` and `y` may not share a cluster, or `None`.
    let separation = |x: usize, y: usize| -> Option<Separation> {
        let (a, b) = if x < y { (x, y) } else { (y, x) };
        if marks.contains(keys[a], keys[b]) {
            return Some(Separation::MarkedDistinct {
                a: keys[a].clone(),
                b: keys[b].clone(),
            });
        }
        let why = conflict(&prints[a], &prints[b])?;
        match_pair(&prints[a], &prints[b], settings)
            .is_none()
            .then(|| Separation::Conflict {
                a: keys[a].clone(),
                b: keys[b].clone(),
                why,
            })
    };

    let mut cluster_of: Vec<usize> = (0..n).collect();
    let mut members: Vec<Vec<usize>> = (0..n).map(|i| vec![i]).collect();
    let mut linked: Vec<(usize, usize, MatchReason)> = Vec::new();
    let mut refused: Vec<(usize, usize, MatchReason, Separation)> = Vec::new();
    for (i, j, reason) in matches {
        if cancelled(cancel) {
            return None;
        }
        let (ci, cj) = (cluster_of[i], cluster_of[j]);
        if ci != cj {
            let blocker = members[ci]
                .iter()
                .find_map(|&x| members[cj].iter().find_map(|&y| separation(x, y)));
            if let Some(blocker) = blocker {
                refused.push((i, j, reason, blocker));
                continue;
            }
            let (keep, absorb) = if members[ci].len() >= members[cj].len() {
                (ci, cj)
            } else {
                (cj, ci)
            };
            let moved = std::mem::take(&mut members[absorb]);
            for &m in &moved {
                cluster_of[m] = keep;
            }
            members[keep].extend(moved);
        }
        linked.push((i, j, reason));
    }
    linked.sort_by_key(|(i, j, _)| (*i, *j));
    refused.sort_by_key(|(i, j, _, _)| (*i, *j));

    let pair = |i: usize, j: usize, reason: &MatchReason| DuplicatePair {
        a: keys[i].clone(),
        b: keys[j].clone(),
        reason: reason.clone(),
    };
    let mut clusters: Vec<DuplicateCluster> = members
        .iter()
        .enumerate()
        .filter(|(_, m)| m.len() >= 2)
        .map(|(c, m)| {
            let mut indices = m.clone();
            indices.sort_unstable();
            DuplicateCluster {
                keys: indices.iter().map(|&i| keys[i].clone()).collect(),
                pairs: linked
                    .iter()
                    .filter(|(i, _, _)| cluster_of[*i] == c)
                    .map(|(i, j, reason)| pair(*i, *j, reason))
                    .collect(),
                related: refused
                    .iter()
                    .filter(|(i, j, _, _)| cluster_of[*i] == c || cluster_of[*j] == c)
                    .map(|(i, j, reason, separation)| RelatedMatch {
                        pair: pair(*i, *j, reason),
                        separation: separation.clone(),
                    })
                    .collect(),
            }
        })
        .collect();
    clusters.sort_by_key(|c| c.keys[0].to_lowercase());
    Some(clusters)
}

/// Existing entries that look like duplicates of the entry stored at `key`
/// (e.g. one just imported), with the reason for each match.
pub fn find_matches_for(
    key: &str,
    entries: &IndexMap<String, Entry>,
    settings: &DedupSettings,
    ignored: &[Vec<String>],
) -> Vec<(String, MatchReason)> {
    let Some(target) = entries.get_index_of(key) else {
        return Vec::new();
    };
    let prints = fingerprints(entries);
    entries
        .keys()
        .enumerate()
        .filter(|(i, other)| *i != target && !is_ignored(key, other, ignored))
        .filter_map(|(i, other)| {
            match_pair(&prints[target], &prints[i], settings).map(|r| (other.clone(), r))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bib::model::EntryType;
    use pretty_assertions::assert_eq;

    fn entry(key: &str, fields: &[(&str, &str)]) -> Entry {
        Entry {
            entry_type: EntryType::Article,
            citation_key: key.to_string(),
            fields: fields
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            group_memberships: Vec::new(),
            raw_index: 0,
            dirty: false,
        }
    }

    fn library(entries: Vec<Entry>) -> IndexMap<String, Entry> {
        entries
            .into_iter()
            .map(|e| (e.citation_key.clone(), e))
            .collect()
    }

    fn cluster_keys(entries: Vec<Entry>) -> Vec<Vec<String>> {
        find_duplicates(&library(entries), &DedupSettings::default(), &[])
            .into_iter()
            .map(|c| c.keys)
            .collect()
    }

    #[test]
    fn doi_normalization_strips_resolvers_and_case() {
        assert_eq!(
            normalize_doi("https://doi.org/10.1000/ABC"),
            Some("10.1000/abc".into())
        );
        assert_eq!(
            normalize_doi("doi:10.1000/x\\_y"),
            Some("10.1000/x_y".into())
        );
        assert_eq!(normalize_doi("{10.1000/abc}"), Some("10.1000/abc".into()));
        assert_eq!(normalize_doi("not a doi"), None);
    }

    #[test]
    fn isbn10_and_isbn13_compare_equal() {
        assert_eq!(normalize_isbns("0-306-40615-2"), vec!["9780306406157"]);
        assert_eq!(normalize_isbns("978-0-306-40615-7"), vec!["9780306406157"]);
        assert_eq!(
            normalize_isbns("0-306-40615-2, 978-1-4028-9462-6"),
            vec!["9780306406157", "9781402894626"]
        );
        assert!(normalize_isbns("12345").is_empty());
    }

    #[test]
    fn eprint_and_url_normalization() {
        assert_eq!(
            normalize_eprint("arXiv:2101.00001v3"),
            Some("2101.00001".into())
        );
        assert_eq!(
            normalize_eprint("hep-th/9901001"),
            Some("hep-th/9901001".into())
        );
        assert_eq!(
            normalize_url("https://www.example.com/paper/1/"),
            Some("example.com/paper/1".into())
        );
        assert_eq!(normalize_url("https://www.osti.gov/"), None);
    }

    #[test]
    fn title_normalization_renders_latex_and_drops_punctuation() {
        assert_eq!(
            normalize_title("{Monte Carlo} Transport in {\\'E}cole: A {Study}."),
            "monte carlo transport in école a study"
        );
    }

    #[test]
    fn bounded_levenshtein_matches_plain_distance() {
        let a: Vec<char> = "kitten".chars().collect();
        let b: Vec<char> = "sitting".chars().collect();
        assert_eq!(bounded_levenshtein(&a, &b, 3), Some(3));
        assert_eq!(bounded_levenshtein(&a, &b, 2), None);
        assert_eq!(bounded_levenshtein(&a, &a, 0), Some(0));
    }

    #[test]
    fn same_doi_matches_regardless_of_title() {
        let keys = cluster_keys(vec![
            entry("A", &[("title", "Foo"), ("doi", "10.1/x")]),
            entry("B", &[("title", "Bar"), ("doi", "https://doi.org/10.1/X")]),
            entry("C", &[("title", "Baz"), ("doi", "10.1/y")]),
        ]);
        assert_eq!(keys, vec![vec!["A".to_string(), "B".to_string()]]);
    }

    #[test]
    fn similar_titles_with_same_year_and_author_match() {
        let lib = library(vec![
            entry(
                "Smith2020",
                &[
                    ("author", "Smith, John"),
                    ("title", "Neutron Transport in Reactor Cores"),
                    ("year", "2020"),
                ],
            ),
            entry(
                "Smith2020a",
                &[
                    ("author", "John Smith and Doe, Jane"),
                    ("title", "{Neutron} transport in reactor cores."),
                    ("year", "2020"),
                ],
            ),
        ]);
        let clusters = find_duplicates(&lib, &DedupSettings::default(), &[]);
        assert_eq!(clusters.len(), 1);
        assert!(matches!(
            clusters[0].pairs[0].reason,
            MatchReason::Title { similarity } if similarity > 0.99
        ));
    }

    #[test]
    fn different_year_or_author_blocks_title_match_unless_relaxed() {
        let lib = library(vec![
            entry(
                "A",
                &[
                    ("author", "Smith, J."),
                    ("title", "A Long Shared Title"),
                    ("year", "2020"),
                ],
            ),
            entry(
                "B",
                &[
                    ("author", "Jones, K."),
                    ("title", "A Long Shared Title"),
                    ("year", "2021"),
                ],
            ),
        ]);
        assert!(find_duplicates(&lib, &DedupSettings::default(), &[]).is_empty());
        let relaxed = DedupSettings {
            require_same_year: false,
            require_same_author: false,
            ..Default::default()
        };
        assert_eq!(find_duplicates(&lib, &relaxed, &[]).len(), 1);
    }

    #[test]
    fn missing_year_does_not_block_a_title_match() {
        let keys = cluster_keys(vec![
            entry(
                "A",
                &[("title", "Some Unique Title Here"), ("year", "2020")],
            ),
            entry("B", &[("title", "Some Unique Title Here")]),
        ]);
        assert_eq!(keys.len(), 1);
    }

    #[test]
    fn distinct_editions_volumes_and_dois_veto_title_matches() {
        for (field, a, b) in [
            ("edition", "2nd", "Third"),
            ("volume", "1", "2"),
            ("doi", "10.1/a", "10.1/b"),
            ("number", "LA-UR-1", "LA-UR-2"),
        ] {
            let keys = cluster_keys(vec![
                entry("A", &[("title", "Handbook of Things"), (field, a)]),
                entry("B", &[("title", "Handbook of Things"), (field, b)]),
            ]);
            assert!(keys.is_empty(), "{field} should veto");
        }
        // Equivalent edition spellings do not veto.
        let keys = cluster_keys(vec![
            entry("A", &[("title", "Handbook of Things"), ("edition", "2nd")]),
            entry(
                "B",
                &[("title", "Handbook of Things"), ("edition", "Second")],
            ),
        ]);
        assert_eq!(keys.len(), 1);
    }

    #[test]
    fn shared_isbn_with_different_chapters_is_not_a_match() {
        let keys = cluster_keys(vec![
            entry(
                "A",
                &[
                    ("title", "Ch One"),
                    ("isbn", "0306406152"),
                    ("chapter", "1"),
                ],
            ),
            entry(
                "B",
                &[
                    ("title", "Ch Two"),
                    ("isbn", "9780306406157"),
                    ("chapter", "2"),
                ],
            ),
        ]);
        assert!(keys.is_empty());
        let keys = cluster_keys(vec![
            entry("A", &[("title", "Book"), ("isbn", "0306406152")]),
            entry("B", &[("title", "The Book"), ("isbn", "9780306406157")]),
        ]);
        assert_eq!(keys.len(), 1);
    }

    #[test]
    fn widely_shared_urls_are_not_identifiers() {
        let titles = ["Alpha", "Bravo", "Charlie", "Delta", "Echo", "Foxtrot"];
        let mut entries: Vec<Entry> = titles
            .iter()
            .map(|t| entry(t, &[("title", t), ("url", "https://example.org/reports")]))
            .collect();
        assert!(cluster_keys(entries.clone()).is_empty());
        entries.truncate(2);
        assert_eq!(cluster_keys(entries).len(), 1);
    }

    #[test]
    fn clusters_are_transitive() {
        let keys = cluster_keys(vec![
            entry("A", &[("title", "Alpha"), ("doi", "10.1/a")]),
            entry(
                "B",
                &[
                    ("title", "Beta"),
                    ("doi", "10.1/a"),
                    ("url", "https://x.org/p"),
                ],
            ),
            entry("C", &[("title", "Gamma"), ("url", "https://x.org/p")]),
            entry("D", &[("title", "Delta")]),
        ]);
        assert_eq!(
            keys,
            vec![vec!["A".to_string(), "B".to_string(), "C".to_string()]]
        );
    }

    /// Distinct candidate pairs, sorted.
    fn candidate_pairs(prints: &[Fingerprint], settings: &DedupSettings) -> Vec<(usize, usize)> {
        let mut pairs = Vec::new();
        for_each_candidate(prints, settings, &AtomicBool::new(false), &mut |i, j| {
            assert!(i < j, "candidates are emitted in order");
            pairs.push((i, j));
        });
        pairs.sort_unstable();
        pairs.dedup();
        pairs
    }

    #[test]
    fn a_cancelled_scan_returns_nothing() {
        let lib = library(vec![
            entry("A", &[("title", "T"), ("doi", "10.1/a")]),
            entry("B", &[("title", "T"), ("doi", "10.1/a")]),
        ]);
        let settings = DedupSettings::default();
        let cancel = AtomicBool::new(false);
        assert_eq!(
            find_duplicates_cancellable(&lib, &settings, &[], &cancel).map(|c| c.len()),
            Some(1)
        );
        cancel.store(true, Ordering::Relaxed);
        assert_eq!(
            find_duplicates_cancellable(&lib, &settings, &[], &cancel),
            None
        );
    }

    fn keys(list: &[&str]) -> Vec<String> {
        list.iter().map(|k| k.to_string()).collect()
    }

    /// Pairs `match_pair` accepts, by comparing every pair.
    fn brute_force_matches(
        prints: &[Fingerprint],
        settings: &DedupSettings,
    ) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        for i in 0..prints.len() {
            for j in (i + 1)..prints.len() {
                if match_pair(&prints[i], &prints[j], settings).is_some() {
                    out.push((i, j));
                }
            }
        }
        out
    }

    #[test]
    fn candidate_index_finds_every_match() {
        // Deterministic pseudo-random titles: variants of a few base titles
        // with random edits, short titles, repeats and shared identifiers.
        let mut seed: u64 = 0x2545_f491_4f6c_dd1d;
        let mut next = move |n: u64| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed % n
        };
        let bases = [
            "Monte Carlo methods for neutron transport",
            "aaaa aaaa aaaa aaaa",
            "Ab",
            "A Primer",
            "On the theory of the the the the",
            "Variance reduction",
        ];
        let alphabet: Vec<char> = "abc ae".chars().collect();
        let mut entries = Vec::new();
        for n in 0..300 {
            let mut title: Vec<char> = bases[next(bases.len() as u64) as usize].chars().collect();
            for _ in 0..next(5) {
                let at = next(title.len() as u64 + 1) as usize;
                let c = alphabet[next(alphabet.len() as u64) as usize];
                match next(3) {
                    0 if at < title.len() => title[at] = c,
                    1 if at < title.len() => {
                        title.remove(at);
                    }
                    _ => title.insert(at, c),
                }
            }
            let title: String = title.into_iter().collect();
            let key = format!("K{n}");
            let mut fields = vec![("title", title)];
            if next(4) == 0 {
                fields.push(("doi", format!("10.1/{}", next(20))));
            }
            if next(6) == 0 {
                fields.push(("year", format!("{}", 2000 + next(2))));
            }
            let fields: Vec<(&str, &str)> = fields.iter().map(|(k, v)| (*k, v.as_str())).collect();
            entries.push(entry(&key, &fields));
        }
        let prints = fingerprints(&library(entries));
        for threshold in [0.0, 0.3, 0.6, 0.75, 0.85, 0.9, 0.95, 1.0] {
            let settings = DedupSettings {
                title_threshold: threshold,
                ..DedupSettings::default()
            };
            let indexed: Vec<(usize, usize)> = candidate_pairs(&prints, &settings)
                .into_iter()
                .filter(|&(i, j)| match_pair(&prints[i], &prints[j], &settings).is_some())
                .collect();
            let expected = brute_force_matches(&prints, &settings);
            assert!(
                !expected.is_empty(),
                "threshold {threshold}: fixture has matches"
            );
            assert_eq!(indexed, expected, "threshold {threshold}");
        }
    }

    #[test]
    fn candidate_index_skips_dissimilar_titles() {
        let entries: Vec<Entry> = (0..200)
            .map(|i| entry(&format!("K{i}"), &[("title", &format!("Row {i:05}"))]))
            .collect();
        let prints = fingerprints(&library(entries));
        let candidates = candidate_pairs(&prints, &DedupSettings::default());
        // Rows one digit apart are one edit apart and stay candidates; the
        // rest of the 19,900 pairs must not be compared.
        assert!(
            candidates.len() < 200 * 199 / 2 / 4,
            "{} candidate pairs",
            candidates.len()
        );
    }

    #[test]
    fn a_bridge_never_joins_a_marked_pair() {
        let lib = library(vec![
            entry("A", &[("title", "One"), ("doi", "10.1/x")]),
            entry("B", &[("title", "Two"), ("doi", "10.1/x")]),
            entry("C", &[("title", "Three"), ("doi", "10.1/x")]),
        ]);
        let ignored = vec![keys(&["C", "A"])];
        let clusters = find_duplicates(&lib, &DedupSettings::default(), &ignored);
        assert_eq!(clusters.len(), 1);
        assert_eq!(clusters[0].keys, keys(&["A", "B"]));
        assert_eq!(
            clusters[0].related,
            vec![RelatedMatch {
                pair: DuplicatePair {
                    a: "B".into(),
                    b: "C".into(),
                    reason: MatchReason::Doi,
                },
                separation: Separation::MarkedDistinct {
                    a: "A".into(),
                    b: "C".into(),
                },
            }]
        );
    }

    #[test]
    fn a_bridge_never_joins_conflicting_identifiers() {
        let same = [("title", "Neutron Transport Methods"), ("year", "2020")];
        let with = |extra: (&'static str, &'static str)| {
            let mut fields = same.to_vec();
            fields.push(extra);
            fields
        };
        let lib = library(vec![
            entry("A", &with(("doi", "10.1/a"))),
            entry("B", &same),
            entry("C", &with(("doi", "10.1/c"))),
        ]);
        let clusters = find_duplicates(&lib, &DedupSettings::default(), &[]);
        assert_eq!(clusters.len(), 1);
        assert_eq!(clusters[0].keys, keys(&["A", "B"]));
        assert_eq!(clusters[0].related.len(), 1);
        assert_eq!(
            clusters[0].related[0].separation,
            Separation::Conflict {
                a: "A".into(),
                b: "C".into(),
                why: "have different DOIs",
            }
        );
    }

    #[test]
    fn every_cluster_member_pair_is_compatible() {
        // Chapters of one book: each matches the book record, not each other.
        let lib = library(vec![
            entry(
                "Book",
                &[("title", "Handbook"), ("isbn", "978-0-306-40615-7")],
            ),
            entry(
                "Ch1",
                &[
                    ("title", "Ch1"),
                    ("isbn", "9780306406157"),
                    ("chapter", "1"),
                ],
            ),
            entry(
                "Ch2",
                &[
                    ("title", "Ch2"),
                    ("isbn", "9780306406157"),
                    ("chapter", "2"),
                ],
            ),
        ]);
        let clusters = find_duplicates(&lib, &DedupSettings::default(), &[]);
        assert_eq!(clusters.len(), 1);
        assert_eq!(clusters[0].keys, keys(&["Book", "Ch1"]));
        assert_eq!(
            clusters[0].related[0].separation.describe(),
            "Ch1 and Ch2 are different parts of one book"
        );
    }

    #[test]
    fn stronger_matches_are_joined_first() {
        // C~B by title comes first in library order, but B~A share a URL;
        // A and C have different eprints, so only one join can stand.
        let lib = library(vec![
            entry(
                "C",
                &[
                    ("title", "Neutron Transport Methods"),
                    ("eprint", "1111.1111"),
                ],
            ),
            entry(
                "B",
                &[
                    ("title", "Neutron Transport Methods"),
                    ("url", "https://x.org/paper"),
                ],
            ),
            entry(
                "A",
                &[
                    ("title", "Unrelated"),
                    ("url", "https://x.org/paper"),
                    ("eprint", "2222.2222"),
                ],
            ),
        ]);
        let clusters = find_duplicates(&lib, &DedupSettings::default(), &[]);
        assert_eq!(clusters.len(), 1);
        assert_eq!(clusters[0].keys, keys(&["B", "A"]));
        assert_eq!(clusters[0].pairs[0].reason, MatchReason::Url);
        assert_eq!(clusters[0].related[0].pair.a, "C");
    }

    #[test]
    fn ignored_sets_suppress_matches() {
        let lib = library(vec![
            entry("A", &[("title", "T"), ("doi", "10.1/a")]),
            entry("B", &[("title", "T"), ("doi", "10.1/a")]),
        ]);
        let ignored = vec![vec!["B".to_string(), "A".to_string()]];
        assert!(find_duplicates(&lib, &DedupSettings::default(), &ignored).is_empty());
        assert!(find_matches_for("A", &lib, &DedupSettings::default(), &ignored).is_empty());
    }

    #[test]
    fn find_matches_for_reports_reasons() {
        let lib = library(vec![
            entry("Old", &[("title", "T"), ("doi", "10.1/a")]),
            entry("Other", &[("title", "Unrelated")]),
            entry("New", &[("title", "T2"), ("doi", "10.1/A")]),
        ]);
        let matches = find_matches_for("New", &lib, &DedupSettings::default(), &[]);
        assert_eq!(matches, vec![("Old".to_string(), MatchReason::Doi)]);
    }

    #[test]
    fn short_dissimilar_titles_do_not_match() {
        let keys = cluster_keys(vec![
            entry("A", &[("title", "MCNP Manual")]),
            entry("B", &[("title", "MCNP Primer")]),
        ]);
        assert!(keys.is_empty());
    }
}
