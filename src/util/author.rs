/// Utilities for parsing and formatting BibTeX author strings.
///
/// BibTeX separates multiple authors with ` and ` (case-sensitive).
/// Individual names may be in "First Last" or canonical "Last, First" form.

/// Extract the last name from a single BibTeX author name.
fn last_name(author: &str) -> &str {
    let author = author.trim();
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
    let names: Vec<&str> = s.split(" and ").collect();
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
/// Handles "Jr.", "III" and similar suffixes: "John Smith Jr." → "Smith Jr., John"
/// is NOT attempted — only the last whitespace token is treated as the last name.
/// For accurate results on complex names, the user should edit the field directly.
pub fn normalize_author_names(s: &str) -> String {
    if s.is_empty() {
        return String::new();
    }
    s.split(" and ")
        .map(|name| normalize_one(name.trim()))
        .collect::<Vec<_>>()
        .join(" and ")
}

fn normalize_one(name: &str) -> String {
    if name.contains(',') {
        // Already in "Last, First" form
        name.to_string()
    } else {
        let parts: Vec<&str> = name.split_whitespace().collect();
        match parts.len() {
            0 => String::new(),
            1 => name.to_string(),
            _ => {
                let last = *parts.last().unwrap();
                let first = parts[..parts.len() - 1].join(" ");
                format!("{}, {}", last, first)
            }
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_abbreviate_single() {
        assert_eq!(abbreviate_authors("Smith, John"), "Smith");
        assert_eq!(abbreviate_authors("John Smith"), "Smith");
    }

    #[test]
    fn test_abbreviate_two() {
        assert_eq!(abbreviate_authors("Smith, John and Jones, Alice"), "Smith and Jones");
        assert_eq!(abbreviate_authors("John Smith and Alice Jones"), "Smith and Jones");
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
}
