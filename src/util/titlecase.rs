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
    // A value enclosed in a single balanced brace pair (e.g. a field created by
    // pasting into an empty editor, which pre-fills protective braces) is
    // unwrapped, titlecased, and re-wrapped — otherwise the outer braces would
    // shield the first and last words from titlecasing.
    if let Some(inner) = strip_outer_braces(input) {
        return format!("{{{}}}", apply_titlecase(inner, ignore_words, stop_words));
    }
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

/// Return the contents of `s` when the whole string is enclosed in a single
/// balanced pair of braces (`{...}`); `None` otherwise. `{Monte Carlo} codes`
/// is not enclosed — its opening brace closes before the end of the string.
fn strip_outer_braces(s: &str) -> Option<&str> {
    let inner = s.strip_prefix('{')?.strip_suffix('}')?;
    let mut depth = 1usize; // depth inside the outer brace
    for c in inner.chars() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth = depth.checked_sub(1)?; // None: unbalanced
                if depth == 0 {
                    return None; // outer brace closes before the end
                }
            }
            _ => {}
        }
    }
    // depth must return to exactly the outer level for the pair to be balanced
    if depth == 1 {
        Some(inner)
    } else {
        None
    }
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

    #[test]
    fn test_fully_wrapped_value_titlecased_inside_braces() {
        // Pasting into an empty field wraps the whole value in protective
        // braces; titlecase must still reach the first and last words.
        assert_eq!(
            apply_titlecase(
                "{Discrimination between gamma and mixed gamma-neutron fields by X-ray diffraction changes in alanine}",
                &[],
                &stops()
            ),
            "{Discrimination Between Gamma and Mixed Gamma-Neutron Fields by X-Ray Diffraction Changes in Alanine}"
        );
    }

    #[test]
    fn test_wrapped_value_preserves_inner_protected_groups() {
        assert_eq!(
            apply_titlecase("{transport in {Monte Carlo} codes}", &ignore(), &stops()),
            "{Transport in {Monte Carlo} Codes}"
        );
    }

    #[test]
    fn test_double_wrapped_value_unwraps_recursively() {
        assert_eq!(
            apply_titlecase("{{neutron transport}}", &[], &stops()),
            "{{Neutron Transport}}"
        );
    }

    #[test]
    fn test_adjacent_brace_groups_are_not_treated_as_wrapped() {
        // Starts with { and ends with } but the outer brace closes early —
        // these are two separate protection groups, both passed through.
        assert_eq!(
            apply_titlecase("{MCNP} and {OpenMC}", &[], &stops()),
            "{MCNP} and {OpenMC}"
        );
    }

    #[test]
    fn test_unbalanced_braces_fall_back_to_word_rules() {
        assert_eq!(
            apply_titlecase("{unclosed brace title", &[], &stops()),
            "{unclosed Brace Title"
        );
    }
}
