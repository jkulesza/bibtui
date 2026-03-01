/// English stop words that are lowercased in title case (except at position 0 or last).
const STOP_WORDS: &[&str] = &[
    "a", "an", "the", "and", "but", "or", "nor", "for", "so", "yet", "at", "by", "in", "of",
    "off", "on", "out", "per", "to", "up", "via", "as", "vs",
];

/// Convert `input` to English title case.
///
/// - Words that case-insensitively match an entry in `ignore_words` are reproduced
///   in the canonical form given in that list (e.g. "mcnp" → "MCNP").
/// - Words that start or end with `{` / `}` are passed through unchanged (BibTeX
///   case-protection braces are respected).
/// - Standard English stop words are lowercased unless they are the first or last word.
/// - All other words are capitalized (first letter upper, rest lower).
pub fn apply_titlecase(input: &str, ignore_words: &[String]) -> String {
    let words: Vec<&str> = input.split_whitespace().collect();
    let n = words.len();
    words
        .iter()
        .enumerate()
        .map(|(i, &word)| {
            let is_boundary = i == 0 || i == n - 1;
            titlecase_word(word, is_boundary, ignore_words)
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn titlecase_word(word: &str, force_cap: bool, ignore_words: &[String]) -> String {
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
    if !force_cap && STOP_WORDS.contains(&lower.as_str()) {
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

    #[test]
    fn test_basic_titlecase() {
        let ignore: Vec<String> = vec!["MCNP".to_string(), "OpenMC".to_string()];
        assert_eq!(
            apply_titlecase("transport of neutrons in a reactor", &ignore),
            "Transport of Neutrons in a Reactor"
        );
    }

    #[test]
    fn test_ignore_words() {
        let ignore: Vec<String> = vec!["MCNP".to_string(), "OpenMC".to_string()];
        assert_eq!(
            apply_titlecase("using mcnp and openmc for simulation", &ignore),
            "Using MCNP and OpenMC for Simulation"
        );
    }

    #[test]
    fn test_brace_passthrough() {
        let ignore: Vec<String> = vec![];
        assert_eq!(
            apply_titlecase("transport in {Monte Carlo} codes", &ignore),
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
}
