/// Convert `input` to English title case.
///
/// - Words that case-insensitively match an entry in `ignore_words` are reproduced
///   in the canonical form given in that list (e.g. "mcnp" → "MCNP").
/// - Words that start or end with `{` / `}` are passed through unchanged (BibTeX
///   case-protection braces are respected).
/// - Words in `stop_words` are lowercased unless they are the first or last word.
/// - Hyphenated compounds are split on `-`; each part is titlecased independently
///   (after the first part, stop words remain lowercase but other words are
///   capitalized, matching standard hyphenated-title-case rules).
/// - All other words are capitalized (first letter upper, rest lower).
pub fn apply_titlecase(input: &str, ignore_words: &[String], stop_words: &[String]) -> String {
    let words: Vec<&str> = input.split_whitespace().collect();
    let n = words.len();
    words
        .iter()
        .enumerate()
        .map(|(i, &word)| {
            let is_boundary = i == 0 || i == n - 1;
            titlecase_compound_word(word, is_boundary, ignore_words, stop_words)
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Handle a single space-separated token, which may be a hyphenated compound.
fn titlecase_compound_word(
    word: &str,
    force_cap: bool,
    ignore_words: &[String],
    stop_words: &[String],
) -> String {
    if !word.contains('-') {
        return titlecase_word(word, force_cap, ignore_words, stop_words);
    }
    word.split('-')
        .enumerate()
        .map(|(j, part)| {
            // First part inherits the boundary status of the whole compound.
            // Parts after a hyphen are treated as non-boundary: stop words stay
            // lowercase, all other words are capitalized.
            titlecase_word(part, if j == 0 { force_cap } else { false }, ignore_words, stop_words)
        })
        .collect::<Vec<_>>()
        .join("-")
}

fn titlecase_word(
    word: &str,
    force_cap: bool,
    ignore_words: &[String],
    stop_words: &[String],
) -> String {
    // Pass through BibTeX case-protecting braces unchanged.
    if word.starts_with('{') || word.ends_with('}') {
        return word.to_string();
    }

    // Check ignore list — case-insensitive lookup, return canonical form.
    let lower = word.to_lowercase();
    for iw in ignore_words {
        if iw.to_lowercase() == lower {
            return iw.clone();
        }
    }

    // Lowercase stop words (unless at a boundary).
    if !force_cap && stop_words.iter().any(|sw| sw.to_lowercase() == lower) {
        return lower;
    }

    // Capitalize: first letter upper, remainder lower.
    let mut chars = word.chars();
    match chars.next() {
        None => String::new(),
        Some(first) => {
            let upper: String = first.to_uppercase().collect();
            upper + &chars.as_str().to_lowercase()
        }
    }
}

/// Strip BibTeX case-protecting inner braces from a string for display purposes.
///
/// Removes all `{` and `}` characters. The stored field value is never modified;
/// this is a display-only transformation.
///
/// Example: `"{Monte Carlo} transport in {OpenMC}"` → `"Monte Carlo transport in OpenMC"`
pub fn strip_case_braces(s: &str) -> String {
    s.chars().filter(|&c| c != '{' && c != '}').collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ignore() -> Vec<String> {
        vec!["MCNP".to_string(), "OpenMC".to_string()]
    }

    fn stops() -> Vec<String> {
        crate::config::schema::TitlecaseConfig::default()
            .stop_words
    }

    #[test]
    fn test_basic_titlecase() {
        assert_eq!(
            apply_titlecase("transport of neutrons in a reactor", &ignore(), &stops()),
            "Transport of Neutrons in a Reactor"
        );
    }

    #[test]
    fn test_ignore_words() {
        assert_eq!(
            apply_titlecase("using mcnp and openmc for simulation", &ignore(), &stops()),
            "Using MCNP and OpenMC for Simulation"
        );
    }

    #[test]
    fn test_brace_passthrough() {
        assert_eq!(
            apply_titlecase("transport in {Monte Carlo} codes", &ignore(), &stops()),
            "Transport in {Monte Carlo} Codes"
        );
    }

    #[test]
    fn test_strip_case_braces() {
        assert_eq!(
            strip_case_braces("{Monte Carlo} transport in {OpenMC}"),
            "Monte Carlo transport in OpenMC"
        );
    }

    #[test]
    fn test_hyphenated_compound_capitalizes_parts() {
        // Non-stop-word parts after a hyphen should be capitalized.
        assert_eq!(
            apply_titlecase("two-dimensional neutron transport", &[], &stops()),
            "Two-Dimensional Neutron Transport"
        );
    }

    #[test]
    fn test_hyphenated_stop_word_stays_lower() {
        // Stop words after a hyphen remain lowercase.
        assert_eq!(
            apply_titlecase("state-of-the-art reactor design", &[], &stops()),
            "State-of-the-Art Reactor Design"
        );
    }

    #[test]
    fn test_hyphenated_at_end_of_title() {
        // Last space-separated word: force_cap=true for the whole compound.
        // The first part of the hyphen gets force_cap=true; subsequent parts get false.
        assert_eq!(
            apply_titlecase("reactor design high-fidelity", &[], &stops()),
            "Reactor Design High-Fidelity"
        );
    }
}
