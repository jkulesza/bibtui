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
//! A, B, and C in one cluster). Pairs the user marked "not a duplicate" are
//! never linked.

use std::collections::{HashMap, HashSet};

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
    /// The pairwise matches that linked the members.
    pub pairs: Vec<DuplicatePair>,
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

/// Find all clusters of possible duplicates in `entries`.
pub fn find_duplicates(
    entries: &IndexMap<String, Entry>,
    settings: &DedupSettings,
    ignored: &[Vec<String>],
) -> Vec<DuplicateCluster> {
    let keys: Vec<&String> = entries.keys().collect();
    let prints = fingerprints(entries);
    let n = keys.len();

    let mut parent: Vec<usize> = (0..n).collect();
    fn root(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }

    let mut pairs: Vec<(usize, usize, MatchReason)> = Vec::new();
    for i in 0..n {
        for j in (i + 1)..n {
            let Some(reason) = match_pair(&prints[i], &prints[j], settings) else {
                continue;
            };
            if is_ignored(keys[i], keys[j], ignored) {
                continue;
            }
            let (ri, rj) = (root(&mut parent, i), root(&mut parent, j));
            if ri != rj {
                parent[rj] = ri;
            }
            pairs.push((i, j, reason));
        }
    }

    let mut members: IndexMap<usize, Vec<usize>> = IndexMap::new();
    let linked: HashSet<usize> = pairs.iter().flat_map(|(i, j, _)| [*i, *j]).collect();
    for i in 0..n {
        if linked.contains(&i) {
            let r = root(&mut parent, i);
            members.entry(r).or_default().push(i);
        }
    }

    let mut clusters: Vec<DuplicateCluster> = members
        .into_iter()
        .map(|(r, indices)| DuplicateCluster {
            keys: indices.iter().map(|&i| keys[i].clone()).collect(),
            pairs: pairs
                .iter()
                .filter(|(i, _, _)| root(&mut parent, *i) == r)
                .map(|(i, j, reason)| DuplicatePair {
                    a: keys[*i].clone(),
                    b: keys[*j].clone(),
                    reason: reason.clone(),
                })
                .collect(),
        })
        .collect();
    clusters.sort_by_key(|c| c.keys[0].to_lowercase());
    clusters
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
