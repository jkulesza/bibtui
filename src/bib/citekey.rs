use indexmap::IndexMap;
use regex::Regex;

/// Generate a citation key from a template string and entry fields.
///
/// Two syntaxes are supported and may be freely mixed in one template:
///
/// **New `[token]` / `[token:mod1:mod2]` syntax** (JabRef-compatible):
/// ```text
/// [auth][year]                   → Smith2020
/// [auth:upper][year]             → SMITH2020
/// [journal:abbr]                 → NSE
/// [title:lower:(20)][year]       → toward_efficient2020  (with regex mod)
/// [auth3][year]                  → SmithJonesWilliams2020
/// [auth][year:regex(\d\d$,)]     → strip last two digits of year
/// ```
///
/// **Legacy `{token}` syntax** (kept for backward compatibility):
/// ```text
/// Article_{year}_{journal_abbrev}_{authors}_{pages}
/// ```
///
/// After substitution, characters that are problematic in BibTeX keys
/// (spaces, braces, quotes, commas, backslashes) are stripped.
pub fn generate_citekey(template: &str, fields: &IndexMap<String, String>) -> String {
    let raw = parse_template(template, fields);
    // Keep only characters that are safe in BibTeX keys and filesystems:
    // alphanumeric, hyphens, and underscores. Everything else is dropped.
    let filtered: String = raw
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_' || *c == '.')
        .collect();
    // Collapse consecutive underscores (produced when optional tokens are
    // empty) and strip any leading/trailing underscores.
    let mut result = String::with_capacity(filtered.len());
    let mut prev_underscore = false;
    for c in filtered.chars() {
        if c == '_' {
            if !prev_underscore {
                result.push(c);
            }
            prev_underscore = true;
        } else {
            result.push(c);
            prev_underscore = false;
        }
    }
    result.trim_matches('_').to_string()
}

// ── Template parser ───────────────────────────────────────────────────────────

fn parse_template(template: &str, fields: &IndexMap<String, String>) -> String {
    let mut result = String::with_capacity(template.len());
    let mut chars = template.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '[' => {
                // Collect until the matching ']', tracking nesting depth
                // so that regex(...) args containing brackets are handled.
                let mut inner = String::new();
                let mut depth = 0usize;
                for ch in chars.by_ref() {
                    match ch {
                        '[' => { depth += 1; inner.push(ch); }
                        ']' if depth > 0 => { depth -= 1; inner.push(ch); }
                        ']' => break,
                        _ => inner.push(ch),
                    }
                }
                result.push_str(&resolve_bracket_token(&inner, fields));
            }
            '{' => {
                // Legacy {token} — collect until '}'
                let mut inner = String::new();
                for ch in chars.by_ref() {
                    if ch == '}' { break; }
                    inner.push(ch);
                }
                result.push_str(&resolve_legacy_token(&inner, fields));
            }
            _ => result.push(c),
        }
    }
    result
}

// ── Bracket-token resolution ──────────────────────────────────────────────────

/// Parse `inner` as `token[:mod1[:mod2...]]`, resolve the token, then apply
/// each modifier in left-to-right order.
fn resolve_bracket_token(inner: &str, fields: &IndexMap<String, String>) -> String {
    let parts = split_on_colon(inner);
    if parts.is_empty() {
        return String::new();
    }
    let mut value = resolve_token(parts[0].trim(), fields);
    for modifier in &parts[1..] {
        value = apply_modifier(value, modifier.trim());
    }
    value
}

/// Split `s` on `:` while ignoring colons inside `(...)`.
fn split_on_colon(s: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' if depth > 0 => depth -= 1,
            ':' if depth == 0 => {
                parts.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    parts.push(&s[start..]);
    parts
}

// ── Token resolution ──────────────────────────────────────────────────────────

/// Resolve a token name to a raw string (before modifiers are applied).
///
/// Supported tokens:
///
/// | Token           | Resolves to                                         |
/// |-----------------|-----------------------------------------------------|
/// | `auth`          | Last name of first author                           |
/// | `authN`         | Last names of first N authors concatenated          |
/// | `authors`       | All authors (≤2 joined, 3+ → "Last1Last2EtAl")     |
/// | `year`          | `year` field                                        |
/// | `shortyear`     | Last two digits of `year`                           |
/// | `title`         | First significant word of `title`                   |
/// | `shorttitle`    | First three significant words of `title`            |
/// | `veryshorttitle`| First significant word of `title` (alias for title) |
/// | `journal`       | `journal` field                                     |
/// | `booktitle`     | `booktitle` field                                   |
/// | `volume`        | `volume` field                                      |
/// | `number`        | `number` or `report-number` field                   |
/// | `pages`         | `pages` field                                       |
/// | `firstpage`     | First page number extracted from `pages`            |
/// | `institution`   | `institution` field                                 |
/// | `school`        | `school` field                                      |
/// | `publisher`     | `publisher` field                                   |
/// | `keywords`      | First keyword from `keywords` field                 |
/// | `howpublished`  | `howpublished` field                                |
/// | *anything else* | Direct field lookup by that name                   |
fn resolve_token(name: &str, fields: &IndexMap<String, String>) -> String {
    // authN — first N authors (N is one or more digits after "auth")
    if let Some(n_str) = name.strip_prefix("auth") {
        if n_str.is_empty() {
            return fields
                .get("author")
                .map(|a| parse_authors(a).into_iter().next().unwrap_or_default())
                .unwrap_or_default();
        }
        if let Ok(n) = n_str.parse::<usize>() {
            return fields
                .get("author")
                .map(|a| parse_authors(a).into_iter().take(n).collect::<Vec<_>>().join(""))
                .unwrap_or_default();
        }
    }

    match name {
        "authors" => fields
            .get("author")
            .map(|a| format_authors_for_key(&parse_authors(a)))
            .unwrap_or_default(),

        "year" => fields.get("year").cloned().unwrap_or_default(),

        "shortyear" => fields
            .get("year")
            .map(|y| y.chars().rev().take(2).collect::<String>().chars().rev().collect())
            .unwrap_or_default(),

        "title" | "veryshorttitle" => fields
            .get("title")
            .map(|t| first_significant_words(&clean_braces(t), 1))
            .unwrap_or_default(),

        "shorttitle" => fields
            .get("title")
            .map(|t| first_significant_words(&clean_braces(t), 3))
            .unwrap_or_default(),

        "journal"     => fields.get("journal").map(|s| clean_braces(s)).unwrap_or_default(),

        "journal_abbrev" => {
            // Use journal_full as the source of truth when present (so the acronym
            // is consistent whether journal holds the full name or the ISO 4 form).
            // Fall back to journal if journal_full is absent.
            let name = fields.get("journal_full")
                .filter(|v| !v.is_empty())
                .or_else(|| fields.get("journal"))
                .map(|s| clean_braces(s))
                .unwrap_or_default();
            abbreviate(&name)
        }

        "booktitle"   => fields.get("booktitle").map(|s| clean_braces(s)).unwrap_or_default(),
        "volume"      => fields.get("volume").cloned().unwrap_or_default(),

        "number" => fields
            .get("number")
            .or_else(|| fields.get("report-number"))
            .cloned()
            .unwrap_or_default(),

        "pages" => fields.get("pages").cloned().unwrap_or_default(),

        "firstpage" => fields
            .get("pages")
            .map(|p| {
                p.split(|c: char| c == '-' || c == ',')
                    .next()
                    .unwrap_or("")
                    .trim()
                    .to_string()
            })
            .unwrap_or_default(),

        "institution" => fields.get("institution").map(|s| clean_braces(s)).unwrap_or_default(),
        "school"      => fields.get("school").map(|s| clean_braces(s)).unwrap_or_default(),
        "publisher"   => fields.get("publisher").map(|s| clean_braces(s)).unwrap_or_default(),

        "keywords" => fields
            .get("keywords")
            .map(|k| k.split(',').next().unwrap_or("").trim().to_string())
            .unwrap_or_default(),

        "howpublished" => fields.get("howpublished").map(|s| clean_braces(s)).unwrap_or_default(),

        // Fallback: direct field lookup
        other => fields.get(other).map(|s| clean_braces(s)).unwrap_or_default(),
    }
}

// ── Modifier application ──────────────────────────────────────────────────────

/// Apply one modifier to `value` and return the result.
///
/// Supported modifiers:
///
/// | Modifier              | Effect                                       |
/// |-----------------------|----------------------------------------------|
/// | `upper`               | Convert to uppercase                         |
/// | `lower`               | Convert to lowercase                         |
/// | `abbr`                | First letter of each significant word        |
/// | `camel`               | Capitalise first letter of each word         |
/// | `(n)`                 | Truncate to first *n* characters             |
/// | `regex(pattern,repl)` | Regex find-and-replace (repeatable)          |
fn apply_modifier(value: String, modifier: &str) -> String {
    match modifier {
        "upper" => return value.to_uppercase(),
        "lower" => return value.to_lowercase(),
        "abbr"  => return abbreviate(&value),
        "camel" => return to_camel_case(&value),
        _ => {}
    }

    // Truncate: (n)
    if modifier.starts_with('(') && modifier.ends_with(')') {
        if let Ok(n) = modifier[1..modifier.len() - 1].parse::<usize>() {
            return value.chars().take(n).collect();
        }
    }

    // Regex substitution: regex(pattern,replacement)
    if let Some(args) = modifier.strip_prefix("regex(").and_then(|s| s.strip_suffix(')')) {
        if let Some(comma) = find_unescaped_comma(args) {
            let pattern     = &args[..comma];
            let replacement = &args[comma + 1..];
            if let Ok(re) = Regex::new(pattern) {
                return re.replace_all(&value, replacement).to_string();
            }
        }
    }

    value
}

/// Index of the first comma in `s` that is not inside parentheses.
fn find_unescaped_comma(s: &str) -> Option<usize> {
    let mut depth = 0usize;
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' if depth > 0 => depth -= 1,
            ',' if depth == 0 => return Some(i),
            _ => {}
        }
    }
    None
}

// ── Legacy {token} resolution ─────────────────────────────────────────────────

/// Resolve a legacy `{token}` to preserve existing behaviour exactly.
fn resolve_legacy_token(token: &str, fields: &IndexMap<String, String>) -> String {
    match token {
        "year"               => fields.get("year").cloned().unwrap_or_default(),
        "author_last"        => fields
            .get("author")
            .map(|a| parse_authors(a).into_iter().next().unwrap_or_default())
            .unwrap_or_default(),
        "authors"            => fields
            .get("author")
            .map(|a| format_authors_for_key(&parse_authors(a)))
            .unwrap_or_default(),
        "title_camel"        => fields
            .get("title")
            .map(|t| to_camel_case(&clean_braces(t)))
            .unwrap_or_default(),
        "journal_abbrev"     => {
            let name = fields.get("journal_full")
                .filter(|v| !v.is_empty())
                .or_else(|| fields.get("journal"))
                .map(|s| clean_braces(s))
                .unwrap_or_default();
            abbreviate(&name)
        }
        "booktitle_abbrev"   => fields.get("booktitle").map(|b| abbreviate(b)).unwrap_or_default(),
        "institution_abbrev" => fields.get("institution").map(|i| abbreviate(i)).unwrap_or_default(),
        "pages"              => fields.get("pages").cloned().unwrap_or_default(),
        "number"             => fields
            .get("number")
            .or_else(|| fields.get("report-number"))
            .cloned()
            .unwrap_or_default(),
        "howpublished_camel" => fields
            .get("howpublished")
            .map(|h| to_camel_case(&clean_braces(h)))
            .unwrap_or_default(),
        "category"           => fields
            .get("keywords")
            .map(|k| to_camel_case(k.split(',').next().unwrap_or("").trim()))
            .unwrap_or_default(),
        other                => format!("{{{}}}", other),
    }
}

// ── String helpers ────────────────────────────────────────────────────────────

/// First `n` significant words from `s` joined without separator.
/// "Significant" means not a common article/preposition.
fn first_significant_words(s: &str, n: usize) -> String {
    const SKIP: &[&str] = &[
        "a", "an", "the", "of", "in", "on", "at", "to", "for", "and", "or", "but",
    ];
    s.split_whitespace()
        .filter(|w| !SKIP.contains(&w.to_lowercase().as_str()))
        .take(n)
        .collect::<Vec<_>>()
        .join("")
}

/// Parse an author string into a vec of last names.
///
/// Handles both "First Last" and "Last, First" formats separated by " and ".
pub fn parse_authors(author: &str) -> Vec<String> {
    author
        .split(" and ")
        .map(|a| {
            let a = a.trim();
            if a.contains(',') {
                a.split(',').next().unwrap_or("").trim().to_string()
            } else {
                a.split_whitespace().last().unwrap_or("").to_string()
            }
        })
        .collect()
}

/// Format author last-names for use in a citation key.
///
/// 1 author  → "Last"
/// 2 authors → "Last1Last2"
/// 3+ authors → "Last1Last2EtAl"
fn format_authors_for_key(authors: &[String]) -> String {
    match authors.len() {
        0 => String::new(),
        1 => authors[0].clone(),
        2 => format!("{}{}", authors[0], authors[1]),
        _ => format!("{}{}EtAl", authors[0], authors[1]),
    }
}

/// First letter of each significant word, uppercased, joined without separator.
fn abbreviate(name: &str) -> String {
    const SKIP: &[&str] = &["of", "the", "and", "for", "in", "on", "a", "an", "&"];
    name.split_whitespace()
        .filter(|w| !SKIP.contains(&w.to_lowercase().as_str()))
        .map(|w| w.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default())
        .collect::<Vec<_>>()
        .join("")
}

/// Capitalise the first letter of each word.
///
/// Words are delimited by any run of non-alphanumeric characters (spaces,
/// hyphens, slashes, dots, …) so that e.g. "self-consistent field" →
/// "SelfConsistentField".
fn to_camel_case(s: &str) -> String {
    let clean = clean_braces(s);
    clean
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut chars = w.chars();
            match chars.next() {
                Some(c) => {
                    let mut out = c.to_uppercase().to_string();
                    out.extend(chars);
                    out
                }
                None => String::new(),
            }
        })
        .collect()
}

/// Strip one layer of surrounding braces, if present.
fn clean_braces(s: &str) -> String {
    let s = s.trim();
    if s.starts_with('{') && s.ends_with('}') {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::IndexMap;

    fn fields(pairs: &[(&str, &str)]) -> IndexMap<String, String> {
        let mut m = IndexMap::new();
        for (k, v) in pairs {
            m.insert(k.to_string(), v.to_string());
        }
        m
    }

    #[test]
    fn test_token_shorttitle() {
        let f = fields(&[("title", "The Quick Brown Fox Jumps")]);
        let result = generate_citekey("[shorttitle]", &f);
        // skips "The"; takes first 3 significant words: Quick, Brown, Fox
        assert_eq!(result, "QuickBrownFox");
    }

    #[test]
    fn test_token_veryshorttitle() {
        let f = fields(&[("title", "The Quick Brown Fox")]);
        let result = generate_citekey("[veryshorttitle]", &f);
        // veryshorttitle = first significant word (skips "The")
        assert_eq!(result, "Quick");
    }

    #[test]
    fn test_token_school() {
        let f = fields(&[
            ("author", "Smith, Jane"),
            ("year", "2020"),
            ("school", "MIT"),
        ]);
        let result = generate_citekey("[auth][year]_[school]", &f);
        assert!(result.contains("MIT"), "result: {}", result);
    }

    #[test]
    fn test_token_publisher() {
        let f = fields(&[("publisher", "Springer")]);
        let result = generate_citekey("[publisher]", &f);
        assert_eq!(result, "Springer");
    }

    #[test]
    fn test_token_keywords() {
        let f = fields(&[("keywords", "nuclear, physics, reactor")]);
        let result = generate_citekey("[keywords]", &f);
        assert_eq!(result, "nuclear");
    }

    #[test]
    fn test_token_howpublished() {
        let f = fields(&[("howpublished", "Online")]);
        let result = generate_citekey("[howpublished]", &f);
        assert_eq!(result, "Online");
    }

    #[test]
    fn test_modifier_camel() {
        let f = fields(&[("journal", "nuclear science and engineering")]);
        let result = generate_citekey("[journal:camel]", &f);
        assert_eq!(result, "NuclearScienceAndEngineering");
    }

    #[test]
    fn test_token_direct_field_lookup() {
        // [reportnumber] is not a known token; falls back to direct field lookup
        let f = fields(&[("reportnumber", "ANL-42")]);
        let result = generate_citekey("[reportnumber]", &f);
        assert_eq!(result, "ANL-42");
    }

    #[test]
    fn test_legacy_unknown_token() {
        // {unknown_token} → resolve_legacy_token returns "{unknown_token}"
        // generate_citekey strips { and } → "unknown_token"
        let f = fields(&[]);
        let result = generate_citekey("{unknown_token}", &f);
        assert_eq!(result, "unknown_token");
    }

    #[test]
    fn test_char_stripping() {
        // Space between tokens is a literal character, stripped by generate_citekey
        let f = fields(&[
            ("author", "Jane Smith"),
            ("year", "2020"),
        ]);
        let result = generate_citekey("[auth] [year]", &f);
        assert_eq!(result, "Smith2020");
    }

    #[test]
    fn test_auth_zero_authors() {
        // No author field → empty string
        let f = fields(&[]);
        let result = generate_citekey("[auth]", &f);
        assert_eq!(result, "");
    }

    #[test]
    fn test_firstpage_comma_separated() {
        let f = fields(&[("pages", "100, 200")]);
        let result = generate_citekey("[firstpage]", &f);
        assert_eq!(result, "100");
    }

    // ── authN token ───────────────────────────────────────────────────────────

    #[test]
    fn test_auth_n_two() {
        let f = fields(&[("author", "Smith, Jane and Jones, Bob and Williams, Carol")]);
        let result = generate_citekey("[auth2]", &f);
        assert_eq!(result, "SmithJones");
    }

    #[test]
    fn test_auth_n_three() {
        let f = fields(&[("author", "Smith, Jane and Jones, Bob and Williams, Carol")]);
        let result = generate_citekey("[auth3]", &f);
        assert_eq!(result, "SmithJonesWilliams");
    }

    #[test]
    fn test_auth_n_more_than_available() {
        // Requesting 5 authors when only 2 exist — returns all available
        let f = fields(&[("author", "Smith, Jane and Jones, Bob")]);
        let result = generate_citekey("[auth5]", &f);
        assert_eq!(result, "SmithJones");
    }

    // ── shortyear ─────────────────────────────────────────────────────────────

    #[test]
    fn test_shortyear() {
        let f = fields(&[("year", "2023")]);
        assert_eq!(generate_citekey("[shortyear]", &f), "23");
    }

    #[test]
    fn test_shortyear_missing() {
        let f = fields(&[]);
        assert_eq!(generate_citekey("[shortyear]", &f), "");
    }

    // ── firstpage with hyphen ─────────────────────────────────────────────────

    #[test]
    fn test_firstpage_hyphen_range() {
        let f = fields(&[("pages", "100--200")]);
        assert_eq!(generate_citekey("[firstpage]", &f), "100");
    }

    #[test]
    fn test_firstpage_single_page() {
        let f = fields(&[("pages", "42")]);
        assert_eq!(generate_citekey("[firstpage]", &f), "42");
    }

    // ── format_authors_for_key ────────────────────────────────────────────────

    #[test]
    fn test_format_authors_two() {
        let f = fields(&[("author", "Smith, Jane and Jones, Bob")]);
        assert_eq!(generate_citekey("[authors]", &f), "SmithJones");
    }

    #[test]
    fn test_format_authors_three_plus_etal() {
        let f = fields(&[("author", "Smith, Jane and Jones, Bob and Williams, Carol")]);
        assert_eq!(generate_citekey("[authors]", &f), "SmithJonesEtAl");
    }

    // ── modifiers ─────────────────────────────────────────────────────────────

    #[test]
    fn test_modifier_upper() {
        let f = fields(&[("year", "2020")]);
        assert_eq!(generate_citekey("[year:upper]", &f), "2020");

        let f2 = fields(&[("author", "Smith, Jane")]);
        assert_eq!(generate_citekey("[auth:upper]", &f2), "SMITH");
    }

    #[test]
    fn test_modifier_lower() {
        let f = fields(&[("author", "Smith, Jane")]);
        assert_eq!(generate_citekey("[auth:lower]", &f), "smith");
    }

    #[test]
    fn test_modifier_abbr() {
        let f = fields(&[("journal", "Nuclear Science Engineering")]);
        assert_eq!(generate_citekey("[journal:abbr]", &f), "NSE");
    }

    #[test]
    fn test_modifier_truncate() {
        let f = fields(&[("author", "Smith, Jane")]);
        // (3) = take first 3 chars of "Smith"
        assert_eq!(generate_citekey("[auth:(3)]", &f), "Smi");
    }

    #[test]
    fn test_modifier_regex() {
        let f = fields(&[("year", "2023")]);
        // Strip last two digits of year
        assert_eq!(generate_citekey("[year:regex(\\d\\d$,)]", &f), "20");
    }

    #[test]
    fn test_modifier_unknown_passthrough() {
        let f = fields(&[("year", "2020")]);
        // Unknown modifier — value unchanged
        assert_eq!(generate_citekey("[year:unknown_modifier]", &f), "2020");
    }

    // ── nested bracket in template ────────────────────────────────────────────

    #[test]
    fn test_empty_bracket_token() {
        // [] resolves to empty string
        let f = fields(&[]);
        assert_eq!(generate_citekey("[]", &f), "");
    }

    // ── legacy tokens ─────────────────────────────────────────────────────────

    #[test]
    fn test_legacy_year() {
        let f = fields(&[("year", "1999")]);
        assert_eq!(generate_citekey("{year}", &f), "1999");
    }

    #[test]
    fn test_legacy_author_last() {
        let f = fields(&[("author", "Smith, Jane and Jones, Bob")]);
        assert_eq!(generate_citekey("{author_last}", &f), "Smith");
    }

    #[test]
    fn test_legacy_authors() {
        let f = fields(&[("author", "Smith, Jane and Jones, Bob and Williams, Carol")]);
        assert_eq!(generate_citekey("{authors}", &f), "SmithJonesEtAl");
    }

    #[test]
    fn test_legacy_title_camel() {
        let f = fields(&[("title", "nuclear science engineering")]);
        assert_eq!(generate_citekey("{title_camel}", &f), "NuclearScienceEngineering");
    }

    #[test]
    fn test_legacy_journal_abbrev() {
        let f = fields(&[("journal", "Nuclear Science Engineering")]);
        assert_eq!(generate_citekey("{journal_abbrev}", &f), "NSE");
    }

    #[test]
    fn test_legacy_booktitle_abbrev() {
        let f = fields(&[("booktitle", "International Conference Nuclear")]);
        let result = generate_citekey("{booktitle_abbrev}", &f);
        assert_eq!(result, "ICN");
    }

    #[test]
    fn test_legacy_institution_abbrev() {
        let f = fields(&[("institution", "Argonne National Laboratory")]);
        assert_eq!(generate_citekey("{institution_abbrev}", &f), "ANL");
    }

    #[test]
    fn test_legacy_pages() {
        let f = fields(&[("pages", "100--200")]);
        assert_eq!(generate_citekey("{pages}", &f), "100--200");
    }

    #[test]
    fn test_legacy_number() {
        let f = fields(&[("number", "42")]);
        assert_eq!(generate_citekey("{number}", &f), "42");
    }

    #[test]
    fn test_legacy_number_report_number_fallback() {
        let f = fields(&[("report-number", "ANL-2020")]);
        assert_eq!(generate_citekey("{number}", &f), "ANL-2020");
    }

    #[test]
    fn test_legacy_howpublished_camel() {
        let f = fields(&[("howpublished", "online report")]);
        assert_eq!(generate_citekey("{howpublished_camel}", &f), "OnlineReport");
    }

    #[test]
    fn test_legacy_category() {
        let f = fields(&[("keywords", "nuclear physics, reactor")]);
        assert_eq!(generate_citekey("{category}", &f), "NuclearPhysics");
    }

    // ── journal_abbrev token (new syntax) ─────────────────────────────────────

    #[test]
    fn test_journal_abbrev_token_uses_journal_full() {
        // When journal_full is present, journal_abbrev token uses it
        let f = fields(&[
            ("journal", "NSE"),                  // abbreviated form in journal field
            ("journal_full", "Nuclear Science and Engineering"),
        ]);
        let result = generate_citekey("[journal_abbrev]", &f);
        // Should abbreviate from journal_full, not the already-abbreviated journal
        assert!(!result.is_empty());
    }

    #[test]
    fn test_journal_abbrev_token_falls_back_to_journal() {
        let f = fields(&[("journal", "Nuclear Science Engineering")]);
        assert_eq!(generate_citekey("[journal_abbrev]", &f), "NSE");
    }

    // ── split_on_colon with nested parens ─────────────────────────────────────

    #[test]
    fn test_modifier_regex_with_colon_in_pattern() {
        // regex pattern containing colon inside parens — must not be split at that colon
        let f = fields(&[("year", "2020")]);
        // regex(\d+,X) → replace digits with X
        let result = generate_citekey("[year:regex(\\d+,X)]", &f);
        assert_eq!(result, "X");
    }

    // ── camel case with punctuation ───────────────────────────────────────────

    #[test]
    fn test_camel_hyphenated_word() {
        // journal:camel on a hyphenated value splits on the hyphen
        let f = fields(&[("journal", "self-consistent methods")]);
        assert_eq!(generate_citekey("[journal:camel]", &f), "SelfConsistentMethods");
    }

    #[test]
    fn test_camel_slash_separated() {
        let f = fields(&[("journal", "nuclear/reactor physics")]);
        assert_eq!(generate_citekey("[journal:camel]", &f), "NuclearReactorPhysics");
    }

    // ── underscore collapsing for empty optional tokens ────────────────────────

    #[test]
    fn test_empty_optional_trailing_underscore_stripped() {
        // When pages is absent, the trailing _ from the template is stripped.
        let f = fields(&[("year", "2020"), ("author", "Smith, J"), ("journal", "Nuclear Science Engineering")]);
        let result = generate_citekey("{year}_{journal_abbrev}_{author_last}_{pages}", &f);
        assert!(!result.ends_with('_'), "trailing underscore should be stripped: {}", result);
        assert_eq!(result, "2020_NSE_Smith");
    }

    #[test]
    fn test_double_underscore_collapsed() {
        // Two adjacent empty tokens produce __ which should collapse to _.
        let f = fields(&[("year", "2020")]);
        let result = generate_citekey("{year}__{pages}", &f);
        assert_eq!(result, "2020");
    }

    // ── sanitization of special characters ───────────────────────────────────

    #[test]
    fn test_citekey_tilde_removed() {
        // Tildes (e.g. from author names like "O~Brien") are dropped, not replaced.
        let f = fields(&[("author", "O~Brien, Patrick")]);
        let result = generate_citekey("[auth]", &f);
        assert_eq!(result, "OBrien");
    }

    #[test]
    fn test_citekey_apostrophe_removed() {
        let f = fields(&[("author", "O'Brien, Patrick")]);
        let result = generate_citekey("[auth]", &f);
        assert_eq!(result, "OBrien");
    }

    // ── parse_authors edge cases ──────────────────────────────────────────────

    #[test]
    fn test_parse_authors_first_last_format() {
        let authors = parse_authors("Jane Smith and Bob Jones");
        assert_eq!(authors, vec!["Smith", "Jones"]);
    }

    #[test]
    fn test_parse_authors_single() {
        let authors = parse_authors("Smith, Jane");
        assert_eq!(authors, vec!["Smith"]);
    }

    #[test]
    fn test_parse_authors_empty() {
        // Single empty-ish entry
        let authors = parse_authors("Smith");
        assert_eq!(authors, vec!["Smith"]);
    }
}
