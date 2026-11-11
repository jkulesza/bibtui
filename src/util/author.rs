//! Utilities for parsing and formatting BibTeX author strings.
//!
//! BibTeX separates multiple authors with ` and ` (case-sensitive).
//! Individual names may be in "First Last" or canonical "Last, First" form.
//! An `and` inside braces is part of a name, not a separator, so a
//! brace-protected corporate author such as
//! `{National Aeronautics and Space Administration}` is one name; use
//! [`split_name_list`] rather than splitting on `" and "` directly.

use regex::Regex;
use std::sync::OnceLock;

/// Split a BibTeX name list into individual names.
///
/// A separator is the word `and` surrounded by whitespace (spaces, tabs or
/// newlines) at brace depth 0; an `and` inside `{...}` belongs to the name.
/// A backslash escapes the following character (matching the parser), so
/// `\{` and `\}` do not change the depth. Names are trimmed and empty
/// names are dropped.
pub fn split_name_list(s: &str) -> Vec<&str> {
    let bytes = s.as_bytes();
    let mut names = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => {
                i += 2;
                continue;
            }
            b'{' => depth += 1,
            b'}' => depth = depth.saturating_sub(1),
            b if depth == 0 && b.is_ascii_whitespace() => {
                // Whitespace run, `and`, whitespace run.
                let mut j = i;
                while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                    j += 1;
                }
                if bytes[j..].starts_with(b"and") {
                    let mut k = j + 3;
                    let after = k;
                    while k < bytes.len() && bytes[k].is_ascii_whitespace() {
                        k += 1;
                    }
                    if k > after && k < bytes.len() {
                        names.push(&s[start..i]);
                        start = k;
                        i = k;
                        continue;
                    }
                }
                i = j;
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    names.push(&s[start..]);
    names
        .into_iter()
        .map(str::trim)
        .filter(|n| !n.is_empty())
        .collect()
}

/// The text of a name written as a single brace group (a corporate or
/// otherwise literal name), without the outer braces: `{World Health
/// Organization}` → `World Health Organization`. `None` for personal names.
pub fn literal_name(name: &str) -> Option<&str> {
    let name = name.trim();
    is_single_brace_group(name).then(|| &name[1..name.len() - 1])
}

/// Extract the last name from a single BibTeX author name.
fn last_name(author: &str) -> &str {
    let author = author.trim();
    if let Some(literal) = literal_name(author) {
        return literal;
    }
    if let Some(comma) = author.find(',') {
        // "Last, First" form — last name is everything before the comma
        author[..comma].trim()
    } else {
        // "First Last" form — last name is the last whitespace-delimited token
        author.split_whitespace().next_back().unwrap_or(author)
    }
}

/// Abbreviate an author field for compact list display.
///
/// - 1 author  → last name only
/// - 2 authors → "Last1 and Last2"
/// - 3+ authors → "Last1 et al."
pub fn abbreviate_authors(s: &str) -> String {
    if s.is_empty() {
        return String::new();
    }
    let names = split_name_list(s);
    match names.len() {
        0 => String::new(),
        1 => last_name(names[0]).to_string(),
        2 => format!("{} and {}", last_name(names[0]), last_name(names[1])),
        _ => format!("{} et al.", last_name(names[0])),
    }
}

/// Normalize a BibTeX author string so every name is in "Last, First" form.
///
/// Names already in "Last, First" form are left unchanged.
/// "First Last" names are converted to "Last, First".
/// Lowercase von-particles attach to the last name per BibTeX's rule
/// (the first lowercase-starting token begins the last-name block):
/// "Guido van Rossum" → "van Rossum, Guido".
/// Names ending in a suffix token ("Jr.", "Jr", "Sr.", "Sr", "II", "III",
/// "IV") are left untouched rather than guessed at; the user should
/// brace-protect or edit such names directly.
/// A name that is entirely one brace group (e.g. a corporate author) is
/// returned unchanged.
pub fn normalize_author_names(s: &str) -> String {
    if s.is_empty() {
        return String::new();
    }
    split_name_list(s)
        .into_iter()
        .map(normalize_one)
        .collect::<Vec<_>>()
        .join(" and ")
}

/// Returns true if `name` contains a comma at brace depth 0.
fn has_comma_at_depth_zero(name: &str) -> bool {
    let mut depth = 0usize;
    for ch in name.chars() {
        match ch {
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => return true,
            _ => {}
        }
    }
    false
}

/// Split a name string on whitespace, treating `{...}` groups as atomic tokens.
fn tokenize_name(name: &str) -> Vec<String> {
    let mut tokens: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut depth = 0usize;

    for ch in name.chars() {
        match ch {
            '{' => {
                depth += 1;
                current.push(ch);
            }
            '}' => {
                depth = depth.saturating_sub(1);
                current.push(ch);
            }
            ' ' | '\t' if depth == 0 => {
                if !current.is_empty() {
                    tokens.push(current.clone());
                    current.clear();
                }
            }
            _ => current.push(ch),
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

/// Insert a space between consecutive initials: "G.H." → "G. H.", "A.B.C." → "A. B. C."
/// Only substitutes at brace depth 0; text inside `{...}` groups is untouched.
fn separate_initials(s: &str) -> String {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"([A-Z]\.)([A-Z])").expect("hardcoded regex is valid"));

    let apply = |chunk: &str| -> String {
        let mut result = chunk.to_string();
        loop {
            let next = re.replace_all(&result, "$1 $2").into_owned();
            if next == result {
                break;
            }
            result = next;
        }
        result
    };

    let mut out = String::new();
    let mut outside = String::new();
    let mut depth = 0usize;
    for ch in s.chars() {
        if depth == 0 {
            if ch == '{' {
                out.push_str(&apply(&outside));
                outside.clear();
                depth = 1;
                out.push(ch);
            } else {
                outside.push(ch);
            }
        } else {
            match ch {
                '{' => depth += 1,
                '}' => depth = depth.saturating_sub(1),
                _ => {}
            }
            out.push(ch);
        }
    }
    out.push_str(&apply(&outside));
    out
}

/// Returns true when the entire (trimmed) name is one `{...}` group.
fn is_single_brace_group(name: &str) -> bool {
    if !name.starts_with('{') || !name.ends_with('}') {
        return false;
    }
    let mut depth = 0usize;
    let last = name.len() - 1;
    for (i, ch) in name.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth = depth.saturating_sub(1);
                // The outer group must not close before the end of the string.
                if depth == 0 && i != last {
                    return false;
                }
            }
            _ => {}
        }
    }
    depth == 0
}

/// Returns true for a trailing name-suffix token like "Jr." or "III".
fn is_suffix_token(token: &str) -> bool {
    matches!(token, "Jr." | "Jr" | "Sr." | "Sr" | "II" | "III" | "IV")
}

fn normalize_one(name: &str) -> String {
    // A fully brace-protected name (corporate author) is returned verbatim,
    // before any processing including initial separation.
    if is_single_brace_group(name) {
        return name.to_string();
    }

    let converted = if has_comma_at_depth_zero(name) {
        // Already in "Last, First" form
        name.to_string()
    } else {
        let parts = tokenize_name(name);
        match parts.len() {
            0 => String::new(),
            1 => name.to_string(),
            _ => {
                // A trailing suffix token would be misparsed as the last name;
                // leave the whole name untouched rather than mangle it.
                if parts.last().map(|t| is_suffix_token(t)).unwrap_or(false) {
                    return name.to_string();
                }
                // BibTeX von-particle rule: the first token starting with a
                // lowercase letter begins the last-name block.
                let von_start = parts
                    .iter()
                    .position(|t| t.chars().next().is_some_and(|c| c.is_lowercase()));
                match von_start {
                    // The whole name is the last-name block; nothing to move.
                    Some(0) => name.to_string(),
                    Some(i) => {
                        let last = parts[i..].join(" ");
                        let first = parts[..i].join(" ");
                        format!("{}, {}", last, first)
                    }
                    None => {
                        let last = parts.last().expect("parts.len() >= 2 in this arm").clone();
                        let first = parts[..parts.len() - 1].join(" ");
                        format!("{}, {}", last, first)
                    }
                }
            }
        }
    };
    separate_initials(&converted)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── split_name_list / literal_name ──────────────────────────────────

    #[test]
    fn split_name_list_keeps_braced_and_inside_one_name() {
        assert_eq!(
            split_name_list("{National Aeronautics and Space Administration}"),
            vec!["{National Aeronautics and Space Administration}"]
        );
    }

    #[test]
    fn split_name_list_mixes_organizations_and_people() {
        assert_eq!(
            split_name_list("Smith, Jane and {Bell and Howell} and John Doe"),
            vec!["Smith, Jane", "{Bell and Howell}", "John Doe"]
        );
    }

    #[test]
    fn split_name_list_handles_nested_groups_and_escapes() {
        assert_eq!(
            split_name_list("{A {and} B and C} and D"),
            vec!["{A {and} B and C}", "D"]
        );
        // An escaped brace does not open a group.
        assert_eq!(split_name_list(r"A\{ and B"), vec![r"A\{", "B"]);
    }

    #[test]
    fn split_name_list_accepts_any_whitespace_around_and() {
        assert_eq!(
            split_name_list("Smith, J.\n  and\tDoe, J."),
            vec!["Smith, J.", "Doe, J."]
        );
        // "and" inside a word, or at the edges, is not a separator.
        assert_eq!(
            split_name_list("Anderson and Sandberg"),
            vec!["Anderson", "Sandberg"]
        );
        assert_eq!(split_name_list("Smith and"), vec!["Smith and"]);
        assert_eq!(split_name_list(""), Vec::<&str>::new());
    }

    #[test]
    fn literal_name_detects_single_brace_group() {
        assert_eq!(
            literal_name("{World Health Organization}"),
            Some("World Health Organization")
        );
        assert_eq!(literal_name(" {CERN} "), Some("CERN"));
        assert_eq!(literal_name("{McConn Jr.}, R. J."), None);
        assert_eq!(literal_name("{A} {B}"), None);
        assert_eq!(literal_name("Jane Smith"), None);
    }

    #[test]
    fn normalize_preserves_corporate_author_with_and() {
        let org = "{National Aeronautics and Space Administration}";
        assert_eq!(normalize_author_names(org), org);
        let mixed = "Jane Smith and {Bell and Howell} and van Rossum, Guido";
        let once = normalize_author_names(mixed);
        assert_eq!(
            once,
            "Smith, Jane and {Bell and Howell} and van Rossum, Guido"
        );
        assert_eq!(normalize_author_names(&once), once, "idempotent");
    }

    #[test]
    fn normalize_keeps_suffix_names_and_braced_surnames() {
        let s = "{Rockwell, III}, Theodore and John Doe Jr. and {Williams III}, R. G.";
        assert_eq!(normalize_author_names(s), s);
    }

    #[test]
    fn abbreviate_corporate_author_is_whole_name() {
        assert_eq!(
            abbreviate_authors("{National Aeronautics and Space Administration}"),
            "National Aeronautics and Space Administration"
        );
        assert_eq!(abbreviate_authors("{CERN} and Smith, J."), "CERN and Smith");
    }

    #[test]
    fn test_abbreviate_single() {
        assert_eq!(abbreviate_authors("Smith, John"), "Smith");
        assert_eq!(abbreviate_authors("John Smith"), "Smith");
    }

    #[test]
    fn test_abbreviate_two() {
        assert_eq!(
            abbreviate_authors("Smith, John and Jones, Alice"),
            "Smith and Jones"
        );
        assert_eq!(
            abbreviate_authors("John Smith and Alice Jones"),
            "Smith and Jones"
        );
    }

    #[test]
    fn test_abbreviate_many() {
        assert_eq!(
            abbreviate_authors("Smith, John and Jones, Alice and Brown, Bob"),
            "Smith et al."
        );
    }

    #[test]
    fn test_normalize_already_normalized() {
        assert_eq!(
            normalize_author_names("Smith, John and Jones, Alice"),
            "Smith, John and Jones, Alice"
        );
    }

    #[test]
    fn test_normalize_first_last() {
        assert_eq!(normalize_author_names("John Smith"), "Smith, John");
        assert_eq!(
            normalize_author_names("John Smith and Alice Jones"),
            "Smith, John and Jones, Alice"
        );
    }

    #[test]
    fn test_normalize_mixed() {
        // One already normalized, one not
        assert_eq!(
            normalize_author_names("Smith, John and Alice Jones"),
            "Smith, John and Jones, Alice"
        );
    }

    #[test]
    fn test_normalize_brace_protected_name() {
        // Names with brace-protected suffixes must not be split on spaces inside {}
        assert_eq!(
            normalize_author_names("R. J. {McConn Jr.}"),
            "{McConn Jr.}, R. J."
        );
        assert_eq!(
            normalize_author_names("R. G. {Williams III}"),
            "{Williams III}, R. G."
        );
        // Multiple authors, some with braces
        assert_eq!(
            normalize_author_names("R. J. {McConn Jr.} and C. J. Gesh and R. G. {Williams III}"),
            "{McConn Jr.}, R. J. and Gesh, C. J. and {Williams III}, R. G."
        );
        // Already normalized with brace — leave unchanged
        assert_eq!(
            normalize_author_names("{McConn Jr.}, R. J."),
            "{McConn Jr.}, R. J."
        );
    }

    #[test]
    fn test_separate_initials() {
        // Two initials run together
        assert_eq!(normalize_author_names("G.H. Smith"), "Smith, G. H.");
        // Three initials run together
        assert_eq!(normalize_author_names("A.B.C. Jones"), "Jones, A. B. C.");
        // Already spaced — unchanged
        assert_eq!(normalize_author_names("G. H. Smith"), "Smith, G. H.");
        // Last, First form with run-together initials
        assert_eq!(normalize_author_names("Smith, G.H."), "Smith, G. H.");
    }

    #[test]
    fn test_abbreviate_empty() {
        assert_eq!(abbreviate_authors(""), "");
    }

    #[test]
    fn test_normalize_empty() {
        assert_eq!(normalize_author_names(""), "");
    }

    #[test]
    fn test_normalize_single_token() {
        // A bare last name with no spaces stays as-is
        assert_eq!(normalize_author_names("Smith"), "Smith");
    }

    #[test]
    fn test_normalize_von_particle() {
        assert_eq!(
            normalize_author_names("Guido van Rossum"),
            "van Rossum, Guido"
        );
        assert_eq!(
            normalize_author_names("Ludwig van der Waals"),
            "van der Waals, Ludwig"
        );
        assert_eq!(
            normalize_author_names("Jean de la Fontaine"),
            "de la Fontaine, Jean"
        );
        // Already normalized von form is left unchanged.
        assert_eq!(
            normalize_author_names("van Rossum, Guido"),
            "van Rossum, Guido"
        );
        // Name that starts with a lowercase particle has no first-name block.
        assert_eq!(normalize_author_names("van Beethoven"), "van Beethoven");
    }

    #[test]
    fn test_normalize_suffix_left_untouched() {
        assert_eq!(normalize_author_names("John Smith Jr."), "John Smith Jr.");
        assert_eq!(normalize_author_names("John Smith Jr"), "John Smith Jr");
        assert_eq!(
            normalize_author_names("Robert Downey Sr."),
            "Robert Downey Sr."
        );
        assert_eq!(normalize_author_names("Henry Ford II"), "Henry Ford II");
        assert_eq!(
            normalize_author_names("William Gates III"),
            "William Gates III"
        );
        assert_eq!(
            normalize_author_names("Thurston Howell IV"),
            "Thurston Howell IV"
        );
    }

    #[test]
    fn test_normalize_corporate_brace_group_unchanged() {
        // A name that is entirely one brace group is returned verbatim —
        // in particular separate_initials must not touch "U.S.".
        assert_eq!(
            normalize_author_names("{U.S. Department of Energy}"),
            "{U.S. Department of Energy}"
        );
        assert_eq!(
            normalize_author_names("{Steering Committee}"),
            "{Steering Committee}"
        );
    }

    #[test]
    fn test_separate_initials_brace_aware() {
        // Initials inside a brace group are untouched; outside they separate.
        assert_eq!(
            normalize_author_names("G.H. {von M.K. Institute}"),
            "{von M.K. Institute}, G. H."
        );
    }

    #[test]
    fn test_normalize_others_unchanged() {
        assert_eq!(
            normalize_author_names("Smith, John and others"),
            "Smith, John and others"
        );
        assert_eq!(
            normalize_author_names("John Smith and others"),
            "Smith, John and others"
        );
    }

    #[test]
    fn test_normalize_idempotent() {
        let inputs = [
            "Guido van Rossum",
            "John Smith Jr.",
            "{U.S. Department of Energy}",
            "G.H. Smith and Alice Jones and others",
            "R. J. {McConn Jr.} and C. J. Gesh",
            "Jean de la Fontaine and Henry Ford II",
        ];
        for input in inputs {
            let once = normalize_author_names(input);
            let twice = normalize_author_names(&once);
            assert_eq!(twice, once, "not idempotent for input: {input}");
        }
    }
}
