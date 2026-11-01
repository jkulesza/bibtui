//! `slidefooter`: a terse citation for presentation slide footers.
//!
//! Always author(s) and year; in between, just enough to retrieve the work:
//!
//! | Type | Middle part | Example |
//! |---|---|---|
//! | Article | ISO 4 journal, `vol(no)` | `Smith et al., Phys. Rev. Lett. 12(3), 2020` |
//! | TechReport | institution acronym, report number | `Sokolowsky and Kulesza, LANL LA-UR-24-28595, 2024` |
//! | Book / Booklet | short title, edition | `Knuth, The Art of Computer Programming, 3rd ed., 1997` |
//! | InProceedings | ISO 4 booktitle | `Doe, Proc. Int. Conf. Mach. Learn., 2020` |
//! | InCollection | `in` short booktitle | `Doe, in Handbook of X, 2019` |
//! | InBook | short title, chapter | `Doe, Big Book, ch. 4, 2019` |
//! | Ph.D. / M.S. thesis | degree, school acronym | `Doe, Ph.D. diss., MIT, 2018` |
//! | Manual | short title, organization acronym | `Manual, LANL, 2020` |
//! | Others | short title (or `howpublished`, or DOI) | `Doe, Dataset X, 2021` |
//!
//! No trailing period and no links: the result is meant to sit in a footer.

use super::{disp, field};
use crate::bib::model::{Entry, EntryType};
use crate::util::journal::abbreviate_journal;
use indexmap::IndexMap;

/// Maximum words kept from a title before it is cut with an ellipsis.
const MAX_TITLE_WORDS: usize = 6;

/// Small words skipped when building an acronym from an institution name.
const ACRONYM_STOP_WORDS: &[&str] = &[
    "of", "the", "and", "for", "in", "at", "on", "de", "der", "du", "la", "le", "&",
];

pub(super) fn format_slidefooter(
    entry: &Entry,
    journal_overrides: &IndexMap<String, String>,
) -> String {
    let mut p: Vec<String> = Vec::new();
    push(&mut p, short_names(entry));

    match &entry.entry_type {
        EntryType::Article => {
            let journal = entry
                .fields
                .get("journal_abbrev")
                .map(|v| disp(v))
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| abbreviate_journal(&field(entry, "journal"), journal_overrides));
            let volno = vol_no(entry);
            push(
                &mut p,
                [journal, volno]
                    .into_iter()
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join(" "),
            );
        }

        EntryType::TechReport => {
            let inst = acronym(&field(entry, "institution"));
            let number = field(entry, "number");
            let label = if !number.is_empty() {
                number.clone()
            } else {
                field(entry, "type")
            };
            push(
                &mut p,
                [inst, label]
                    .into_iter()
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join(" "),
            );
            // Without a report number the title is needed to find it.
            if number.is_empty() {
                push(&mut p, short_title(entry));
            }
        }

        EntryType::Book | EntryType::Booklet => {
            push(&mut p, short_title(entry));
            push(&mut p, edition(&field(entry, "edition")));
        }

        EntryType::InProceedings => {
            let booktitle = field(entry, "booktitle");
            push(&mut p, venue(&booktitle, journal_overrides));
        }

        EntryType::InCollection => {
            let booktitle = shorten(&field(entry, "booktitle"));
            if !booktitle.is_empty() {
                p.push(format!("in {booktitle}"));
            }
        }

        EntryType::InBook => {
            push(&mut p, short_title(entry));
            // Only chapter numbers; a chapter title would dwarf the rest.
            let chapter = field(entry, "chapter");
            if !chapter.is_empty() && chapter.chars().count() <= 4 && !chapter.contains(' ') {
                p.push(format!("ch. {chapter}"));
            }
        }

        EntryType::PhdThesis => {
            p.push("Ph.D. diss.".to_string());
            push(&mut p, acronym(&field(entry, "school")));
        }

        EntryType::MastersThesis => {
            p.push("M.S. thesis".to_string());
            push(&mut p, acronym(&field(entry, "school")));
        }

        EntryType::Manual => {
            push(&mut p, short_title(entry));
            push(&mut p, acronym(&field(entry, "organization")));
        }

        EntryType::Proceedings => {
            let st = field(entry, "shorttitle");
            if st.is_empty() {
                push(&mut p, venue(&field(entry, "title"), journal_overrides));
            } else {
                p.push(st);
            }
        }

        EntryType::Unpublished => {
            push(&mut p, short_title(entry));
            p.push("unpublished".to_string());
        }

        // Misc and Other(...): title first, then how it was published, then
        // a DOI as the last resort for retrievability.
        EntryType::Misc | EntryType::Other(_) => {
            let title = short_title(entry);
            if !title.is_empty() {
                p.push(title);
            } else {
                let how = field(entry, "howpublished");
                let doi = field(entry, "doi");
                if !how.is_empty() {
                    p.push(shorten(&how));
                } else if !doi.is_empty() {
                    p.push(format!("doi:{}", doi.trim()));
                }
            }
        }
    }

    push(&mut p, year(entry));
    p.join(", ")
}

fn push(p: &mut Vec<String>, s: String) {
    if !s.is_empty() {
        p.push(s);
    }
}

/// `year`, falling back to the first four characters of a biblatex `date`.
fn year(entry: &Entry) -> String {
    let y = field(entry, "year");
    if !y.is_empty() {
        return y;
    }
    let date = field(entry, "date");
    date.get(..4)
        .filter(|s| s.chars().all(|c| c.is_ascii_digit()))
        .map(str::to_string)
        .unwrap_or_default()
}

/// `12(3)`, `12`, or `no. 3`.
fn vol_no(entry: &Entry) -> String {
    let vol = field(entry, "volume");
    let num = field(entry, "number");
    match (vol.is_empty(), num.is_empty()) {
        (false, false) => format!("{vol}({num})"),
        (false, true) => vol,
        (true, false) => format!("no. {num}"),
        (true, true) => String::new(),
    }
}

// ── Names ────────────────────────────────────────────────────────────────

/// Surnames: `A`, `A and B`, or `A et al.`; editors get `(ed.)`/`(eds.)`
/// when there is no author.
fn short_names(entry: &Entry) -> String {
    let author = entry.fields.get("author").map(String::as_str).unwrap_or("");
    if !author.trim().is_empty() {
        return surnames(author).0;
    }
    let editor = entry.fields.get("editor").map(String::as_str).unwrap_or("");
    if !editor.trim().is_empty() {
        let (names, count) = surnames(editor);
        return format!("{names} ({})", if count == 1 { "ed." } else { "eds." });
    }
    String::new()
}

/// Returns the formatted surname list and the number of names.
fn surnames(raw: &str) -> (String, usize) {
    let names: Vec<&str> = raw
        .split(" and ")
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .collect();
    let has_others = names.iter().any(|n| n.eq_ignore_ascii_case("others"));
    let real: Vec<String> = names
        .iter()
        .filter(|n| !n.eq_ignore_ascii_case("others"))
        .map(|n| surname(n))
        .collect();
    let s = match (real.len(), has_others) {
        (0, _) => String::new(),
        (1, false) => real[0].clone(),
        (2, false) => format!("{} and {}", real[0], real[1]),
        _ => format!("{} et al.", real[0]),
    };
    (s, names.len())
}

/// Surname of one BibTeX name. A name that is one brace group is a
/// corporate author and is shortened to an acronym when it is long.
fn surname(name: &str) -> String {
    if is_single_brace_group(name) {
        return acronym(&disp(name));
    }
    let last = if let Some((before, _)) = name.split_once(',') {
        before.trim().to_string()
    } else {
        // "First von Last": the surname is the last token plus any
        // lowercase particles immediately before it.
        let tokens: Vec<&str> = name.split_whitespace().collect();
        let mut start = tokens.len().saturating_sub(1);
        while start > 0
            && tokens[start - 1]
                .chars()
                .next()
                .is_some_and(|c| c.is_lowercase())
        {
            start -= 1;
        }
        tokens[start..].join(" ")
    };
    disp(&last)
}

fn is_single_brace_group(s: &str) -> bool {
    let s = s.trim();
    if !(s.starts_with('{') && s.ends_with('}')) {
        return false;
    }
    let mut depth = 0i32;
    for (i, c) in s.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 && i != s.len() - 1 {
                    return false;
                }
            }
            _ => {}
        }
    }
    true
}

// ── Institutions ─────────────────────────────────────────────────────────

/// Abbreviate an institution name: a parenthesized acronym if one is given
/// (`Los Alamos National Laboratory (LANL)` → `LANL`), otherwise the
/// initials of its capitalized words (`Massachusetts Institute of
/// Technology` → `MIT`). Text after the first comma is kept
/// (`University of California, Berkeley` → `UC Berkeley`). Short names
/// (one or two significant words) are returned unchanged.
fn acronym(name: &str) -> String {
    let name = name.trim();
    if name.is_empty() {
        return String::new();
    }
    if let Some(acr) = parenthesized_acronym(name) {
        return acr;
    }
    let (head, tail) = match name.split_once(',') {
        Some((h, t)) => (h.trim(), t.trim()),
        None => (name, ""),
    };
    let significant: Vec<&str> = head
        .split(|c: char| c.is_whitespace() || c == '-')
        .filter(|w| !w.is_empty())
        .filter(|w| !ACRONYM_STOP_WORDS.contains(&w.to_lowercase().as_str()))
        .collect();
    if significant.len() < 3 && tail.is_empty() {
        return name.to_string();
    }
    let initials: String = significant
        .iter()
        .filter(|w| w.chars().next().is_some_and(|c| c.is_uppercase()))
        .map(|w| {
            // Dotted abbreviations contribute all their letters ("U.S." → "US").
            if w.contains('.') {
                w.chars().filter(|c| c.is_alphabetic()).collect::<String>()
            } else {
                w.chars().next().map(String::from).unwrap_or_default()
            }
        })
        .collect();
    let short = if initials.chars().count() >= 2 {
        initials
    } else {
        head.to_string()
    };
    if tail.is_empty() {
        short
    } else {
        format!("{short} {tail}")
    }
}

/// A conference name: its parenthesized acronym when it has one
/// (`Proceedings of the … (HEART) Conference` → `Proc. HEART`), otherwise
/// the ISO 4 abbreviation of its main title, cut to `MAX_TITLE_WORDS`.
fn venue(name: &str, journal_overrides: &IndexMap<String, String>) -> String {
    let name = name.trim();
    if name.is_empty() {
        return String::new();
    }
    if let Some(acr) = parenthesized_acronym(name) {
        return if name.to_lowercase().starts_with("proc") {
            format!("Proc. {acr}")
        } else {
            acr
        };
    }
    let main = name.split(':').next().unwrap_or("").trim();
    shorten(&abbreviate_journal(main, journal_overrides))
}

/// The first `(TOKEN)` whose token is a plausible acronym: one word with at
/// least two uppercase letters (`HEART`, `ICML'20`, `M&C`).
fn parenthesized_acronym(s: &str) -> Option<String> {
    let mut rest = s;
    while let Some(open) = rest.find('(') {
        let after = &rest[open + 1..];
        let close = after.find(')')?;
        let inner = after[..close].trim();
        if !inner.is_empty()
            && !inner.contains(char::is_whitespace)
            && inner.chars().filter(|c| c.is_uppercase()).count() >= 2
        {
            return Some(inner.to_string());
        }
        rest = &after[close + 1..];
    }
    None
}

// ── Titles and editions ──────────────────────────────────────────────────

/// `shorttitle` if present, else the shortened `title`.
fn short_title(entry: &Entry) -> String {
    let st = field(entry, "shorttitle");
    if !st.is_empty() {
        return st;
    }
    shorten(&field(entry, "title"))
}

/// Main title before any `:` subtitle, cut to `MAX_TITLE_WORDS` words.
fn shorten(title: &str) -> String {
    let main = title.split(':').next().unwrap_or("").trim();
    let words: Vec<&str> = main.split_whitespace().collect();
    if words.len() > MAX_TITLE_WORDS {
        format!("{}…", words[..MAX_TITLE_WORDS - 1].join(" "))
    } else {
        words.join(" ")
    }
}

/// `3` → `3rd ed.`, `Third` → `Third ed.`, `2nd edition` → `2nd ed.`.
fn edition(ed: &str) -> String {
    let ed = ed.trim();
    if ed.is_empty() {
        return String::new();
    }
    if let Ok(n) = ed.parse::<u32>() {
        let suffix = match (n % 10, n % 100) {
            (1, r) if r != 11 => "st",
            (2, r) if r != 12 => "nd",
            (3, r) if r != 13 => "rd",
            _ => "th",
        };
        return format!("{n}{suffix} ed.");
    }
    let lower = ed.to_lowercase();
    for word in [" edition", " ed.", " ed"] {
        if lower.ends_with(word) {
            return format!("{} ed.", ed[..ed.len() - word.len()].trim_end());
        }
    }
    format!("{ed} ed.")
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bib::model::Entry;
    use pretty_assertions::assert_eq;

    fn make(ty: EntryType, fields: &[(&str, &str)]) -> Entry {
        let mut e = Entry {
            entry_type: ty,
            citation_key: "k".to_string(),
            fields: IndexMap::new(),
            group_memberships: Vec::new(),
            raw_index: 0,
            dirty: false,
        };
        for (k, v) in fields {
            e.fields.insert(k.to_string(), v.to_string());
        }
        e
    }

    fn fmt(e: &Entry) -> String {
        format_slidefooter(e, &IndexMap::new())
    }

    #[test]
    fn article_uses_iso4_journal_and_vol_no() {
        let e = make(
            EntryType::Article,
            &[
                ("author", "Smith, J. and Jones, A. and Brown, B."),
                ("journal", "Physical Review Letters"),
                ("volume", "12"),
                ("number", "3"),
                ("pages", "1--10"),
                ("year", "2020"),
            ],
        );
        assert_eq!(fmt(&e), "Smith et al., Phys. Rev. Lett. 12(3), 2020");
    }

    #[test]
    fn article_prefers_journal_abbrev_field_and_overrides() {
        let e = make(
            EntryType::Article,
            &[
                ("author", "Smith, J."),
                ("journal", "Journal of Widgets"),
                ("journal_abbrev", "J. Widg."),
                ("volume", "4"),
                ("year", "2001"),
            ],
        );
        assert_eq!(fmt(&e), "Smith, J. Widg. 4, 2001");

        let e = make(
            EntryType::Article,
            &[
                ("author", "Smith, J."),
                ("journal", "Journal of Widgets"),
                ("number", "7"),
                ("year", "2001"),
            ],
        );
        let mut ov = IndexMap::new();
        ov.insert("journal of widgets".to_string(), "JW".to_string());
        assert_eq!(format_slidefooter(&e, &ov), "Smith, JW no. 7, 2001");
    }

    #[test]
    fn techreport_issue_63_example() {
        let e = make(
            EntryType::TechReport,
            &[
                ("author", "Sokolowsky, S. J. and Kulesza, J. A."),
                (
                    "title",
                    "Nuclear Weapons Effects Nomogram Python Application",
                ),
                ("institution", "Los Alamos National Laboratory"),
                ("number", "LA-UR-24-28595"),
                ("year", "2024"),
            ],
        );
        assert_eq!(fmt(&e), "Sokolowsky and Kulesza, LANL LA-UR-24-28595, 2024");
    }

    #[test]
    fn techreport_without_number_uses_type_and_title() {
        let e = make(
            EntryType::TechReport,
            &[
                ("author", "Doe, J."),
                ("title", "A Report: With a Subtitle"),
                ("institution", "MIT"),
                ("type", "Memo"),
                ("year", "1999"),
            ],
        );
        assert_eq!(fmt(&e), "Doe, MIT Memo, A Report, 1999");
    }

    #[test]
    fn book_short_title_and_edition() {
        let e = make(
            EntryType::Book,
            &[
                ("author", "Donald E. Knuth"),
                (
                    "title",
                    "The Art of Computer Programming: Fundamental Algorithms",
                ),
                ("edition", "3"),
                ("publisher", "Addison-Wesley"),
                ("year", "1997"),
            ],
        );
        assert_eq!(
            fmt(&e),
            "Knuth, The Art of Computer Programming, 3rd ed., 1997"
        );
    }

    #[test]
    fn book_long_title_is_cut_and_shorttitle_wins() {
        let e = make(
            EntryType::Book,
            &[
                ("editor", "Doe, J. and Roe, R."),
                ("title", "One Two Three Four Five Six Seven Eight"),
                ("year", "2000"),
            ],
        );
        assert_eq!(
            fmt(&e),
            "Doe and Roe (eds.), One Two Three Four Five…, 2000"
        );

        let e = make(
            EntryType::Book,
            &[
                ("editor", "Doe, J."),
                ("title", "Long Title"),
                ("shorttitle", "LT"),
                ("edition", "Second edition"),
                ("year", "2000"),
            ],
        );
        assert_eq!(fmt(&e), "Doe (ed.), LT, Second ed., 2000");
    }

    #[test]
    fn edition_ordinals() {
        assert_eq!(edition("1"), "1st ed.");
        assert_eq!(edition("2"), "2nd ed.");
        assert_eq!(edition("11"), "11th ed.");
        assert_eq!(edition("12"), "12th ed.");
        assert_eq!(edition("13"), "13th ed.");
        assert_eq!(edition("22"), "22nd ed.");
        assert_eq!(edition("2nd ed"), "2nd ed.");
        assert_eq!(edition("Third"), "Third ed.");
        assert_eq!(edition(""), "");
    }

    #[test]
    fn inproceedings_incollection_inbook() {
        let e = make(
            EntryType::InProceedings,
            &[
                ("author", "Doe, J."),
                ("booktitle", "Proceedings of the International Conference"),
                ("year", "2020"),
            ],
        );
        let s = fmt(&e);
        assert!(s.starts_with("Doe, Proc"), "{s}");
        assert!(s.ends_with(", 2020"), "{s}");

        let e = make(
            EntryType::InProceedings,
            &[
                ("author", "Doe, J."),
                (
                    "booktitle",
                    "Proceedings of the Hardened Electronics and Radiation Technology (HEART) Conference",
                ),
                ("year", "2022"),
            ],
        );
        assert_eq!(fmt(&e), "Doe, Proc. HEART, 2022");

        let e = make(
            EntryType::InProceedings,
            &[
                ("author", "Doe, J."),
                ("booktitle", "Workshop (virtual) on (M&C) Methods"),
                ("year", "2022"),
            ],
        );
        assert_eq!(fmt(&e), "Doe, M&C, 2022");

        let e = make(
            EntryType::InCollection,
            &[
                ("author", "Doe, J."),
                ("booktitle", "Handbook of Things: Volume One"),
                ("year", "2019"),
            ],
        );
        assert_eq!(fmt(&e), "Doe, in Handbook of Things, 2019");

        let e = make(
            EntryType::InBook,
            &[
                ("author", "Doe, J."),
                ("title", "Big Book"),
                ("chapter", "4"),
                ("year", "2019"),
            ],
        );
        assert_eq!(fmt(&e), "Doe, Big Book, ch. 4, 2019");

        let e = make(
            EntryType::InBook,
            &[
                ("author", "Doe, J."),
                ("title", "Big Book"),
                ("chapter", "A Very Long Chapter Title"),
                ("year", "2019"),
            ],
        );
        assert_eq!(fmt(&e), "Doe, Big Book, 2019");
    }

    #[test]
    fn theses_manual_proceedings_unpublished() {
        let e = make(
            EntryType::PhdThesis,
            &[
                ("author", "Doe, J."),
                ("school", "Massachusetts Institute of Technology"),
                ("year", "2018"),
            ],
        );
        assert_eq!(fmt(&e), "Doe, Ph.D. diss., MIT, 2018");

        let e = make(
            EntryType::MastersThesis,
            &[
                ("author", "Doe, J."),
                ("school", "University of California, Berkeley"),
                ("year", "2016"),
            ],
        );
        assert_eq!(fmt(&e), "Doe, M.S. thesis, UC Berkeley, 2016");

        let e = make(
            EntryType::Manual,
            &[
                ("author", "{Los Alamos National Laboratory}"),
                ("title", "MCNP User Manual"),
                ("organization", "Los Alamos National Laboratory (LANL)"),
                ("year", "2022"),
            ],
        );
        assert_eq!(fmt(&e), "LANL, MCNP User Manual, LANL, 2022");

        let e = make(
            EntryType::Proceedings,
            &[
                ("editor", "Roe, R."),
                ("title", "Proceedings of Symposia in Applied Mathematics"),
                ("year", "2010"),
            ],
        );
        let s = fmt(&e);
        assert!(s.starts_with("Roe (ed.), Proc"), "{s}");
        assert!(!s.contains("Proceedings"), "{s}");

        let e = make(
            EntryType::Proceedings,
            &[
                ("editor", "Roe, R."),
                ("title", "Long"),
                ("shorttitle", "PSAM"),
                ("year", "2010"),
            ],
        );
        assert_eq!(fmt(&e), "Roe (ed.), PSAM, 2010");

        let e = make(
            EntryType::Unpublished,
            &[("author", "Doe, J."), ("title", "Draft"), ("year", "2025")],
        );
        assert_eq!(fmt(&e), "Doe, Draft, unpublished, 2025");
    }

    #[test]
    fn misc_falls_back_to_howpublished_then_doi() {
        let e = make(
            EntryType::Misc,
            &[
                ("author", "Doe, J."),
                ("title", "Dataset X"),
                ("date", "2021-05-01"),
            ],
        );
        assert_eq!(fmt(&e), "Doe, Dataset X, 2021");

        let e = make(
            EntryType::Misc,
            &[
                ("author", "Doe, J."),
                ("howpublished", "Online"),
                ("year", "2021"),
            ],
        );
        assert_eq!(fmt(&e), "Doe, Online, 2021");

        let e = make(
            EntryType::Other("online".to_string()),
            &[("author", "Doe, J."), ("doi", "10.1/xyz")],
        );
        assert_eq!(fmt(&e), "Doe, doi:10.1/xyz");
    }

    #[test]
    fn names_particles_others_and_latex() {
        assert_eq!(surnames("Guido van Rossum").0, "van Rossum");
        assert_eq!(surnames("Smith, J. and others").0, "Smith et al.");
        assert_eq!(surnames("G{\\\"o}del, Kurt").0, "Gödel");
        assert_eq!(surnames("{CERN}").0, "CERN");
        assert_eq!(surnames("A, B and C, D").0, "A and C");
        assert_eq!(surnames("").0, "");
    }

    #[test]
    fn acronym_rules() {
        assert_eq!(acronym("Los Alamos National Laboratory"), "LANL");
        assert_eq!(acronym("Oak Ridge National Laboratory (ORNL)"), "ORNL");
        assert_eq!(acronym("CERN"), "CERN");
        assert_eq!(acronym("Sandia Labs"), "Sandia Labs");
        assert_eq!(acronym("U.S. Nuclear Regulatory Commission"), "USNRC");
        assert_eq!(acronym(""), "");
        assert!(!is_single_brace_group("{A} {B}"));
    }

    #[test]
    fn empty_entry_is_empty() {
        let e = make(EntryType::Article, &[]);
        assert_eq!(fmt(&e), "");
    }
}
