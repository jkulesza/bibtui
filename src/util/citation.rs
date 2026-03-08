//! Citation formatting for the preview popup.
//!
//! Currently implements IEEEtranN (IEEE numbered bibliography style).
//! Add additional styles by matching on `style` in `format_citation()`.

use crate::bib::model::{Entry, EntryType};
use crate::util::latex::render_latex;
use crate::util::titlecase::strip_case_braces;

/// Format `entry` as a bibliography citation using `style`.
/// Unrecognised style names fall back to IEEEtranN.
pub fn format_citation(entry: &Entry, style: &str) -> String {
    match style.to_lowercase().replace('-', "").as_str() {
        "ieeetran" | "ieeetrann" | "ieee" | "ieeetranbst" => format_ieeetran(entry),
        _ => format_ieeetran(entry),
    }
}

// ── Display helpers ──────────────────────────────────────────────────────────

fn disp(s: &str) -> String {
    strip_case_braces(&render_latex(s))
}

fn field(entry: &Entry, name: &str) -> String {
    entry.fields.get(name).map(|v| disp(v)).unwrap_or_default()
}

// ── IEEEtranN ───────────────────────────────────────────────────────────────

fn format_ieeetran(entry: &Entry) -> String {
    let authors = format_authors_ieee(&field(entry, "author"));
    let editor  = field(entry, "editor");
    let title   = field(entry, "title");
    let year    = field(entry, "year");

    match &entry.entry_type {
        EntryType::Article => {
            let mut p: Vec<String> = Vec::new();
            push_authors(&mut p, &authors);
            push_quoted_title(&mut p, &title);
            push_nonempty(&mut p, field(entry, "journal"));
            push_vol_no(&mut p, entry);
            push_pages(&mut p, entry);
            push_year_month(&mut p, entry, &year);
            let mut s = p.join(", ");
            push_link(&mut s, entry);
            s
        }

        EntryType::Book | EntryType::Booklet => {
            let mut p: Vec<String> = Vec::new();
            if !authors.is_empty() {
                push_authors(&mut p, &authors);
            } else if !editor.is_empty() {
                p.push(format!("{}, Ed.", format_authors_ieee(&editor)));
            }
            push_nonempty(&mut p, title.clone());
            let edition = field(entry, "edition");
            if !edition.is_empty() {
                p.push(format!("{} ed.", edition));
            }
            let address   = field(entry, "address");
            let publisher = field(entry, "publisher");
            if !address.is_empty() && !publisher.is_empty() {
                p.push(format!("{}: {}", address, publisher));
            } else {
                push_nonempty(&mut p, publisher);
            }
            push_nonempty(&mut p, year.clone());
            let mut s = p.join(", ");
            push_link(&mut s, entry);
            s
        }

        EntryType::InProceedings | EntryType::InBook | EntryType::InCollection
        | EntryType::Proceedings => {
            let mut p: Vec<String> = Vec::new();
            push_authors(&mut p, &authors);
            push_quoted_title(&mut p, &title);
            let booktitle = field(entry, "booktitle");
            if !booktitle.is_empty() {
                p.push(format!("in {}", booktitle));
            }
            push_nonempty(&mut p, field(entry, "address"));
            push_nonempty(&mut p, year.clone());
            push_pages(&mut p, entry);
            let mut s = p.join(", ");
            push_link(&mut s, entry);
            s
        }

        EntryType::TechReport => {
            let mut p: Vec<String> = Vec::new();
            push_authors(&mut p, &authors);
            push_quoted_title(&mut p, &title);
            push_nonempty(&mut p, field(entry, "institution"));
            let number      = field(entry, "number");
            let report_type = field(entry, "type");
            if !number.is_empty() {
                let label = if report_type.is_empty() {
                    "Rep.".to_string()
                } else {
                    report_type
                };
                p.push(format!("{} {}", label, number));
            }
            push_nonempty(&mut p, year.clone());
            let mut s = p.join(", ");
            push_link(&mut s, entry);
            s
        }

        EntryType::PhdThesis => {
            let mut p: Vec<String> = Vec::new();
            push_authors(&mut p, &authors);
            push_quoted_title(&mut p, &title);
            p.push("Ph.D. dissertation".to_string());
            push_nonempty(&mut p, field(entry, "school"));
            push_nonempty(&mut p, field(entry, "address"));
            push_nonempty(&mut p, year.clone());
            let mut s = p.join(", ");
            push_link(&mut s, entry);
            s
        }

        EntryType::MastersThesis => {
            let mut p: Vec<String> = Vec::new();
            push_authors(&mut p, &authors);
            push_quoted_title(&mut p, &title);
            p.push("M.S. thesis".to_string());
            push_nonempty(&mut p, field(entry, "school"));
            push_nonempty(&mut p, field(entry, "address"));
            push_nonempty(&mut p, year.clone());
            let mut s = p.join(", ");
            push_link(&mut s, entry);
            s
        }

        EntryType::Unpublished => {
            let mut p: Vec<String> = Vec::new();
            push_authors(&mut p, &authors);
            push_quoted_title(&mut p, &title);
            push_nonempty(&mut p, field(entry, "note"));
            push_nonempty(&mut p, year.clone());
            let mut s = p.join(", ");
            push_link(&mut s, entry);
            s
        }

        EntryType::Manual => {
            let mut p: Vec<String> = Vec::new();
            if !authors.is_empty() {
                push_authors(&mut p, &authors);
            } else if !editor.is_empty() {
                push_authors(&mut p, &format_authors_ieee(&editor));
            }
            push_quoted_title(&mut p, &title);
            push_nonempty(&mut p, field(entry, "organization"));
            push_nonempty(&mut p, field(entry, "address"));
            push_nonempty(&mut p, year.clone());
            let mut s = p.join(", ");
            push_link(&mut s, entry);
            s
        }

        // Misc and Other(...)
        _ => {
            let mut p: Vec<String> = Vec::new();
            push_authors(&mut p, &authors);
            push_quoted_title(&mut p, &title);
            let how = field(entry, "howpublished");
            push_nonempty(&mut p, how.clone());
            push_nonempty(&mut p, year.clone());
            push_nonempty(&mut p, field(entry, "note"));
            let mut s = p.join(", ");
            push_link(&mut s, entry);
            s
        }
    }
}

// ── Part helpers ─────────────────────────────────────────────────────────────

fn push_nonempty(p: &mut Vec<String>, s: String) {
    if !s.is_empty() {
        p.push(s);
    }
}

fn push_authors(p: &mut Vec<String>, authors: &str) {
    if !authors.is_empty() {
        p.push(authors.to_string());
    }
}

fn push_quoted_title(p: &mut Vec<String>, title: &str) {
    if !title.is_empty() {
        p.push(format!("\"{}\"", title));
    }
}

fn push_vol_no(p: &mut Vec<String>, entry: &Entry) {
    let vol = field(entry, "volume");
    let num = field(entry, "number");
    if !vol.is_empty() {
        p.push(format!("vol. {}", vol));
    }
    if !num.is_empty() {
        p.push(format!("no. {}", num));
    }
}

fn push_pages(p: &mut Vec<String>, entry: &Entry) {
    let pages = field(entry, "pages");
    if !pages.is_empty() {
        p.push(format!("pp. {}", pages));
    }
}

fn push_year_month(p: &mut Vec<String>, entry: &Entry, year: &str) {
    let month = field(entry, "month");
    if !month.is_empty() && !year.is_empty() {
        p.push(format!("{} {}", abbrev_month(&month), year));
    } else if !year.is_empty() {
        p.push(year.to_string());
    }
}

/// Append a DOI hyperlink (preferred) or URL to the citation, then terminate with a period.
///
/// DOI is formatted as `https://doi.org/{doi}` and takes precedence over the `url` field.
/// The URL is wrapped in an OSC 8 terminal hyperlink so it is clickable in supporting
/// terminals while remaining readable as plain text everywhere else.
fn push_link(s: &mut String, entry: &Entry) {
    let doi = field(entry, "doi");
    let url = field(entry, "url");

    let link_url = if !doi.is_empty() {
        let raw = doi.trim();
        if raw.starts_with("http://") || raw.starts_with("https://") {
            raw.to_string()
        } else {
            format!("https://doi.org/{}", raw)
        }
    } else if !url.is_empty() {
        url
    } else {
        terminate(s);
        return;
    };

    // Ensure the body ends cleanly before appending the link.
    match s.chars().last() {
        Some(',') => { s.pop(); }
        Some('.') | Some('?') | Some('!') => {}
        _ => {}
    }

    // OSC 8 hyperlink: ESC ] 8 ;; <url> ESC \ <text> ESC ] 8 ;; ESC \
    s.push_str(&format!(
        ". \x1b]8;;{url}\x1b\\{url}\x1b]8;;\x1b\\.",
        url = link_url
    ));
}

/// Ensure the string ends with a period (adds one if missing, replaces trailing comma).
fn terminate(s: &mut String) {
    match s.chars().last() {
        Some('.') | Some('?') | Some('!') => {}
        Some(',') => { s.pop(); s.push('.'); }
        _ => { s.push('.'); }
    }
}

// ── Author formatting ────────────────────────────────────────────────────────

fn format_authors_ieee(raw: &str) -> String {
    let raw = disp(raw);
    if raw.trim().is_empty() {
        return String::new();
    }

    let authors: Vec<&str> = raw.split(" and ").map(str::trim).collect();

    if authors.len() > 6 {
        return format!("{} et al.", format_single_ieee(authors[0]));
    }

    let fmt: Vec<String> = authors.iter().map(|a| format_single_ieee(a)).collect();

    match fmt.len() {
        0 => String::new(),
        1 => fmt[0].clone(),
        2 => format!("{} and {}", fmt[0], fmt[1]),
        _ => {
            let head = fmt[..fmt.len() - 1].join(", ");
            format!("{}, and {}", head, fmt.last().unwrap())
        }
    }
}

/// Format one author name as "F. M. Last".
fn format_single_ieee(author: &str) -> String {
    let author = disp(author.trim());
    if let Some(comma) = author.find(',') {
        let last      = author[..comma].trim();
        let given     = author[comma + 1..].trim();
        let initials  = build_initials(given);
        if initials.is_empty() { last.to_string() } else { format!("{} {}", initials, last) }
    } else {
        let words: Vec<&str> = author.split_whitespace().collect();
        match words.len() {
            0 => String::new(),
            1 => words[0].to_string(),
            _ => {
                let last     = *words.last().unwrap();
                let given    = words[..words.len() - 1].join(" ");
                let initials = build_initials(&given);
                if initials.is_empty() { last.to_string() } else { format!("{} {}", initials, last) }
            }
        }
    }
}

/// Turn "John Andrew" → "J. A.".  Lowercase particles (von, van, de…) are skipped.
fn build_initials(given: &str) -> String {
    given
        .split_whitespace()
        .filter_map(|w| {
            let c = w.chars().next()?;
            if c.is_lowercase() { None } else { Some(format!("{}.", c)) }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn abbrev_month(month: &str) -> &'static str {
    match month.to_lowercase().trim() {
        "jan" | "january"   => "Jan.",
        "feb" | "february"  => "Feb.",
        "mar" | "march"     => "Mar.",
        "apr" | "april"     => "Apr.",
        "may"               => "May",
        "jun" | "june"      => "Jun.",
        "jul" | "july"      => "Jul.",
        "aug" | "august"    => "Aug.",
        "sep" | "sept" | "september" => "Sep.",
        "oct" | "october"   => "Oct.",
        "nov" | "november"  => "Nov.",
        "dec" | "december"  => "Dec.",
        _                   => "??.",
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_author_last_first() {
        assert_eq!(format_authors_ieee("Smith, John A."), "J. A. Smith");
    }

    #[test]
    fn test_author_first_last() {
        assert_eq!(format_authors_ieee("John Smith"), "J. Smith");
    }

    #[test]
    fn test_two_authors() {
        assert_eq!(
            format_authors_ieee("Smith, Alice and Jones, Bob"),
            "A. Smith and B. Jones"
        );
    }

    #[test]
    fn test_three_authors() {
        assert_eq!(
            format_authors_ieee("Smith, Alice and Jones, Bob and Brown, Carol"),
            "A. Smith, B. Jones, and C. Brown"
        );
    }

    #[test]
    fn test_abbrev_month() {
        assert_eq!(abbrev_month("jan"), "Jan.");
        assert_eq!(abbrev_month("december"), "Dec.");
        assert_eq!(abbrev_month("may"), "May");
    }
}
