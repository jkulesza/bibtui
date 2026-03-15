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
    raw.chars()
        .filter(|c| !matches!(*c, ' ' | '\t' | '\n' | '{' | '}' | '"' | ',' | '\\'))
        .collect()
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

/// Capitalise the first letter of each whitespace-separated word.
fn to_camel_case(s: &str) -> String {
    let clean = clean_braces(s);
    clean
        .split_whitespace()
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
}
