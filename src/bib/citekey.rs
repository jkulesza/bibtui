use indexmap::IndexMap;
use regex::Regex;

/// Generate a citation key from a template string and entry fields.
///
/// Two syntaxes are supported and may be freely mixed in one template:
///
/// **New `[token]` / `[token:mod1:mod2]` syntax** (JabRef-compatible):
/// ```text
/// [auth][year]                        → Smith2020
/// [auth:upper][year]                  → SMITH2020
/// [journal:abbr]                      → NSE
/// [title:lower:(20)][year]            → toward_efficient2020  (with regex mod)
/// [auth3][year]                       → Smi2020 (first 3 chars of first author)
/// [auth][year:regex("\d\d$", "")]    → strip last two digits of year
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

/// JabRef function words — excluded from significant-word title tokens.
const FUNCTION_WORDS: &[&str] = &[
    "a", "about", "above", "across", "against", "along", "among", "an",
    "and", "around", "at", "before", "behind", "below", "beneath",
    "beside", "between", "beyond", "but", "by", "down", "during",
    "except", "for", "from", "in", "inside", "into", "like", "near",
    "nor", "of", "off", "on", "onto", "or", "since", "so", "the",
    "through", "to", "toward", "under", "until", "up", "upon", "with",
    "within", "without", "yet",
];

fn is_function_word(w: &str) -> bool {
    FUNCTION_WORDS.contains(&w.to_lowercase().as_str())
}

/// Return the author or editor field (author preferred, editor as fallback).
fn get_author_or_editor<'a>(fields: &'a IndexMap<String, String>) -> Option<&'a String> {
    fields.get("author")
        .filter(|v| !v.is_empty())
        .or_else(|| fields.get("editor").filter(|v| !v.is_empty()))
}

/// Resolve a token name to a raw string (before modifiers are applied).
///
/// JabRef-compatible tokens — see <https://docs.jabref.org/setup/citationkeypatterns>.
fn resolve_token(name: &str, fields: &IndexMap<String, String>) -> String {
    // ── pureauth* — author only, no editor fallback ──────────────────────
    if let Some(rest) = name.strip_prefix("pureauth") {
        let inner = if rest.is_empty() { "auth".to_string() } else { format!("auth{}", rest) };
        return resolve_auth_token(&inner, fields.get("author").filter(|v| !v.is_empty()));
    }

    // ── edtr* / editor* — editor field only ──────────────────────────────
    if name.starts_with("edtr") || name.starts_with("editor") {
        let mapped = remap_editor_token(name);
        return resolve_auth_token(&mapped, fields.get("editor").filter(|v| !v.is_empty()));
    }

    // ── auth* / author* — author with editor fallback ────────────────────
    if name.starts_with("auth") || name.starts_with("author") {
        return resolve_auth_token(name, get_author_or_editor(fields));
    }

    // ── Non-author tokens ────────────────────────────────────────────────
    match name {
        "authors" => get_author_or_editor(fields)
            .map(|a| format_authors_for_key(&parse_authors(a)))
            .unwrap_or_default(),

        "editors" => fields.get("editor")
            .map(|a| format_authors_for_key(&parse_authors(a)))
            .unwrap_or_default(),

        "year" => fields.get("year").cloned().unwrap_or_default(),

        "shortyear" => fields
            .get("year")
            .map(|y| {
                let digits: String = y.chars().filter(|c| c.is_ascii_digit()).collect();
                if digits.len() >= 2 { digits[digits.len()-2..].to_string() } else { digits }
            })
            .unwrap_or_default(),

        "title" => fields
            .get("title")
            .map(|t| capitalize_significant_words(&clean_braces(t)))
            .unwrap_or_default(),

        "fulltitle" => fields
            .get("title")
            .map(|t| clean_braces(t))
            .unwrap_or_default(),

        "veryshorttitle" => fields
            .get("title")
            .map(|t| first_significant_words(&clean_braces(t), 1))
            .unwrap_or_default(),

        "shorttitle" => fields
            .get("title")
            .map(|t| first_significant_words(&clean_braces(t), 3))
            .unwrap_or_default(),

        "entrytype" => fields.get("entrytype").cloned().unwrap_or_default(),

        "journal"     => fields.get("journal").map(|s| clean_braces(s)).unwrap_or_default(),

        "journal_abbrev" => {
            let jname = fields.get("journal_full")
                .filter(|v| !v.is_empty())
                .or_else(|| fields.get("journal"))
                .map(|s| clean_braces(s))
                .unwrap_or_default();
            abbreviate(&jname)
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
            .map(|p| extract_first_page(p))
            .unwrap_or_default(),

        "lastpage" => fields
            .get("pages")
            .map(|p| extract_last_page(p))
            .unwrap_or_default(),

        "pageprefix" => fields
            .get("pages")
            .map(|p| p.chars().take_while(|c| !c.is_ascii_digit()).collect())
            .unwrap_or_default(),

        "institution" => fields.get("institution").map(|s| clean_braces(s)).unwrap_or_default(),
        "school"      => fields.get("school").map(|s| clean_braces(s)).unwrap_or_default(),
        "publisher"   => fields.get("publisher").map(|s| clean_braces(s)).unwrap_or_default(),

        "keywords" => fields
            .get("keywords")
            .map(|k| split_keywords(k).into_iter().next().unwrap_or_default())
            .unwrap_or_default(),

        "howpublished" => fields.get("howpublished").map(|s| clean_braces(s)).unwrap_or_default(),

        other => {
            // camelN — first N words of title camelized
            if let Some(n_str) = other.strip_prefix("camel") {
                if let Ok(n) = n_str.parse::<usize>() {
                    return fields.get("title")
                        .map(|t| to_camel_case_n(&clean_braces(t), n))
                        .unwrap_or_default();
                }
            }
            // keywordN — Nth keyword (1-indexed)
            if let Some(n_str) = other.strip_prefix("keyword") {
                if !n_str.starts_with('s') {
                    if let Ok(n) = n_str.parse::<usize>() {
                        return fields.get("keywords")
                            .and_then(|k| split_keywords(k).into_iter().nth(n.saturating_sub(1)))
                            .unwrap_or_default();
                    }
                }
            }
            // keywordsN — first N keywords joined
            if let Some(n_str) = other.strip_prefix("keywords") {
                if let Ok(n) = n_str.parse::<usize>() {
                    return fields.get("keywords")
                        .map(|k| split_keywords(k).into_iter().take(n).collect::<Vec<_>>().join(""))
                        .unwrap_or_default();
                }
            }
            // [ALLCAPS] → raw field value
            if !other.is_empty() && other.chars().all(|c| c.is_uppercase() || !c.is_alphabetic()) {
                let lower = other.to_lowercase();
                return fields.get(&lower).map(|s| clean_braces(s)).unwrap_or_default();
            }
            fields.get(other).map(|s| clean_braces(s)).unwrap_or_default()
        }
    }
}

// ── Author/editor token resolution ───────────────────────────────────────────

/// Map an `edtr*`/`editor*` token to its `auth*`/`author*` equivalent so the
/// same resolution logic handles both.
fn remap_editor_token(name: &str) -> String {
    if let Some(rest) = name.strip_prefix("editorLast") {
        return format!("authorLast{}", rest);
    }
    if let Some(rest) = name.strip_prefix("editorIni") {
        return format!("authorIni{}", rest);
    }
    if let Some(rest) = name.strip_prefix("editor") {
        return format!("author{}", rest);
    }
    if let Some(rest) = name.strip_prefix("edtr") {
        return format!("auth{}", rest);
    }
    name.to_string()
}

/// Resolve an auth-family token against a given name-list field value.
fn resolve_auth_token(name: &str, author_field: Option<&String>) -> String {
    let author_str = match author_field {
        Some(s) if !s.is_empty() => s,
        _ => return String::new(),
    };

    // Exact-match compound tokens (before strip_prefix catches them)
    match name {
        "auth" => return parse_authors(author_str).into_iter().next().unwrap_or_default(),

        "auth.etal" => {
            let authors = parse_authors(author_str);
            return match authors.len() {
                0 => String::new(),
                1 | 2 => authors[0].clone(),
                _ => format!("{}.etal", authors[0]),
            };
        }
        "authEtAl" => {
            let authors = parse_authors(author_str);
            return match authors.len() {
                0 => String::new(),
                1 | 2 => authors[0].clone(),
                _ => format!("{}EtAl", authors[0]),
            };
        }
        "auth.auth.ea" => {
            let authors = parse_authors(author_str);
            return match authors.len() {
                0 => String::new(),
                1 => authors[0].clone(),
                2 => format!("{}.{}", authors[0], authors[1]),
                _ => format!("{}.{}.ea", authors[0], authors[1]),
            };
        }
        "authshort" => {
            let authors = parse_authors(author_str);
            return match authors.len() {
                0 => String::new(),
                1 => authors[0].clone(),
                n => {
                    let mut s: String = authors.iter()
                        .take(3)
                        .filter_map(|a| a.chars().next())
                        .collect();
                    if n > 3 { s.push('+'); }
                    s
                }
            };
        }
        "authorLast" => {
            return parse_authors(author_str).into_iter().last().unwrap_or_default();
        }
        "authForeIni" => {
            return parse_forename_initial(author_str, 0);
        }
        "authorLastForeIni" => {
            let parts: Vec<&str> = author_str.split(" and ").collect();
            if let Some(last) = parts.last() {
                return forename_initial_of(last.trim());
            }
            return String::new();
        }
        "authorIni" => {
            let authors = parse_authors(author_str);
            if authors.is_empty() { return String::new(); }
            let first: String = authors[0].chars().take(5).collect();
            let rest: String = authors[1..].iter()
                .filter_map(|a| a.chars().next())
                .collect();
            return format!("{}{}", first, rest);
        }
        "authors" => {
            return format_authors_for_key(&parse_authors(author_str));
        }
        _ => {}
    }

    // authIniN — beginning of each author's surname, max N total chars
    if let Some(n_str) = name.strip_prefix("authIni") {
        if let Ok(n) = n_str.parse::<usize>() {
            let authors = parse_authors(author_str);
            if authors.is_empty() { return String::new(); }
            let base = (n / authors.len()).max(1);
            let extra = n - base * authors.len();
            let mut result = String::new();
            for (i, a) in authors.iter().enumerate() {
                let take = base + if i < extra { 1 } else { 0 };
                let chunk: String = a.chars().take(take).collect();
                result.push_str(&chunk);
                if result.len() >= n { break; }
            }
            return result.chars().take(n).collect();
        }
    }

    // authN_M — first N chars of Mth author's last name
    if let Some(rest) = name.strip_prefix("auth") {
        if let Some((n_str, m_str)) = rest.split_once('_') {
            if let (Ok(n), Ok(m)) = (n_str.parse::<usize>(), m_str.parse::<usize>()) {
                let authors = parse_authors(author_str);
                return authors.get(m.saturating_sub(1))
                    .map(|a| a.chars().take(n).collect())
                    .unwrap_or_default();
            }
        }
    }

    // authorsN — first N authors' last names + EtAl if more
    if let Some(n_str) = name.strip_prefix("authors") {
        if let Ok(n) = n_str.parse::<usize>() {
            let authors = parse_authors(author_str);
            let has_more = authors.len() > n;
            let mut result: String = authors.iter().take(n).cloned().collect::<Vec<_>>().join("");
            if has_more { result.push_str("EtAl"); }
            return result;
        }
    }

    // authN — first N chars of first author's last name (JabRef semantics)
    if let Some(n_str) = name.strip_prefix("auth") {
        if let Ok(n) = n_str.parse::<usize>() {
            return parse_authors(author_str)
                .into_iter()
                .next()
                .map(|a| a.chars().take(n).collect())
                .unwrap_or_default();
        }
    }

    String::new()
}

// ── Modifier application ──────────────────────────────────────────────────────

/// Apply one modifier to `value` and return the result.
///
/// Supported modifiers:
///
/// | Modifier              | Effect                                        |
/// |-----------------------|-----------------------------------------------|
/// | `upper`               | Convert to uppercase                          |
/// | `lower`               | Convert to lowercase                          |
/// | `abbr`                | First letter of each significant word         |
/// | `camel`               | Capitalise first letter of each word          |
/// | `capitalize`          | Uppercase first char of each word, rest lower |
/// | `titlecase`           | Like capitalize but lowercase function words  |
/// | `sentencecase`        | All lowercase, then uppercase first char      |
/// | `(n)`                 | Truncate to first *n* characters              |
/// | `(text)`              | Fallback: use text if value is empty          |
/// | `truncateN`           | Truncate to first N characters                |
/// | `regex("pat","repl")` | Regex find-and-replace (repeatable)           |
fn apply_modifier(value: String, modifier: &str) -> String {
    match modifier {
        "upper" => return value.to_uppercase(),
        "lower" => return value.to_lowercase(),
        "abbr"  => return abbreviate(&value),
        "camel" => return to_camel_case(&value),
        "capitalize" => return capitalize_all_words(&value),
        "titlecase" => return titlecase(&value),
        "sentencecase" => return sentencecase(&value),
        _ => {}
    }

    // truncateN
    if let Some(n_str) = modifier.strip_prefix("truncate") {
        if let Ok(n) = n_str.parse::<usize>() {
            return value.chars().take(n).collect::<String>().trim_end().to_string();
        }
    }

    // (n) truncate or (text) fallback
    if modifier.starts_with('(') && modifier.ends_with(')') {
        let inner = &modifier[1..modifier.len() - 1];
        if let Ok(n) = inner.parse::<usize>() {
            return value.chars().take(n).collect();
        }
        // Fallback: if value is empty, use the inner text
        if value.is_empty() {
            return inner.to_string();
        }
        return value;
    }

    // Regex substitution: regex("pattern","replacement")
    if let Some(args) = modifier.strip_prefix("regex(").and_then(|s| s.strip_suffix(')')) {
        if let Some((pattern, replacement)) = parse_regex_args(args) {
            if let Ok(re) = Regex::new(&pattern) {
                return re.replace_all(&value, replacement.as_str()).to_string();
            }
        }
    }

    value
}

/// Parse one double-quoted (or single-quoted) string from the start of `s`,
/// honouring backslash-escaped quotes within the string.
///
/// Returns `(unescaped_content, remainder_after_closing_quote)` or `None` if
/// `s` does not begin with a quote character.
fn parse_quoted_arg(s: &str) -> Option<(String, &str)> {
    let s = s.trim_start();
    let mut chars = s.char_indices();
    let (_, quote) = chars.next()?;
    if quote != '"' && quote != '\'' {
        return None;
    }
    let mut result = String::new();
    while let Some((i, c)) = chars.next() {
        if c == '\\' {
            if let Some((_, next)) = chars.next() {
                if next == quote {
                    result.push(next);
                } else {
                    result.push('\\');
                    result.push(next);
                }
            }
        } else if c == quote {
            return Some((result, &s[i + 1..]));
        } else {
            result.push(c);
        }
    }
    None
}

/// Parse the two quoted arguments inside `regex(...)`.
///
/// Expected form (after the outer `regex(` / `)` have been stripped):
/// `"pattern", "replacement"` — whitespace around the comma is allowed.
fn parse_regex_args(args: &str) -> Option<(String, String)> {
    let (pattern, rest) = parse_quoted_arg(args)?;
    let rest = rest.trim_start().strip_prefix(',')?.trim_start();
    let (replacement, _) = parse_quoted_arg(rest)?;
    Some((pattern, replacement))
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

/// Extract the forename initial of the Nth author (0-indexed).
fn parse_forename_initial(author_str: &str, idx: usize) -> String {
    let parts: Vec<&str> = author_str.split(" and ").collect();
    parts.get(idx)
        .map(|a| forename_initial_of(a.trim()))
        .unwrap_or_default()
}

/// Extract the forename initial from a single author name.
fn forename_initial_of(name: &str) -> String {
    if name.contains(',') {
        // "Last, First Middle" → first char of "First"
        name.split(',')
            .nth(1)
            .and_then(|f| f.trim().chars().next())
            .map(|c| c.to_uppercase().to_string())
            .unwrap_or_default()
    } else {
        // "First Last" → first char of "First"
        name.chars().next()
            .map(|c| c.to_uppercase().to_string())
            .unwrap_or_default()
    }
}

fn extract_first_page(pages: &str) -> String {
    pages.split(|c: char| c == '-' || c == ',')
        .next()
        .unwrap_or("")
        .trim()
        .to_string()
}

fn extract_last_page(pages: &str) -> String {
    pages.split(|c: char| c == '-' || c == ',')
        .last()
        .unwrap_or("")
        .trim()
        .to_string()
}

fn split_keywords(k: &str) -> Vec<String> {
    k.split(|c: char| c == ',' || c == ';')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// First `n` significant words from `s` joined without separator.
/// "Significant" means not a JabRef function word.
fn first_significant_words(s: &str, n: usize) -> String {
    s.split_whitespace()
        .filter(|w| !is_function_word(w))
        .take(n)
        .collect::<Vec<_>>()
        .join("")
}

/// Capitalize all significant words in the title and concatenate.
/// Function words are lowercased. JabRef `[title]` behavior.
fn capitalize_significant_words(s: &str) -> String {
    s.split_whitespace()
        .filter(|w| !is_function_word(w))
        .map(|w| capitalize_word(w))
        .collect()
}

/// Capitalize first char, preserve rest.
fn capitalize_word(w: &str) -> String {
    let mut chars = w.chars();
    match chars.next() {
        Some(c) => {
            let mut out = c.to_uppercase().to_string();
            out.extend(chars);
            out
        }
        None => String::new(),
    }
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
    name.split_whitespace()
        .filter(|w| !is_function_word(w))
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
        .map(|w| capitalize_word(w))
        .collect()
}

/// Camel-case the first N words of a string.
fn to_camel_case_n(s: &str, n: usize) -> String {
    let clean = clean_braces(s);
    clean
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .take(n)
        .map(|w| capitalize_word(w))
        .collect()
}

/// Capitalize first char of each word, rest lowercase.
fn capitalize_all_words(s: &str) -> String {
    s.split_whitespace()
        .map(|w| {
            let mut chars = w.chars();
            match chars.next() {
                Some(c) => {
                    let mut out = c.to_uppercase().to_string();
                    for ch in chars { out.extend(ch.to_lowercase()); }
                    out
                }
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Titlecase: capitalize normal words, lowercase function words (except first/last).
fn titlecase(s: &str) -> String {
    let words: Vec<&str> = s.split_whitespace().collect();
    let last_idx = words.len().saturating_sub(1);
    words.iter().enumerate()
        .map(|(i, w)| {
            if i == 0 || i == last_idx || !is_function_word(w) {
                capitalize_word(w)
            } else {
                w.to_lowercase()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Sentence case: all lowercase, then uppercase first char.
fn sentencecase(s: &str) -> String {
    let lower = s.to_lowercase();
    let mut chars = lower.chars();
    match chars.next() {
        Some(c) => {
            let mut out = c.to_uppercase().to_string();
            out.extend(chars);
            out
        }
        None => String::new(),
    }
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

    // ── title tokens (JabRef semantics) ──────────────────────────────────

    #[test]
    fn test_title_capitalizes_significant_words() {
        let f = fields(&[("title", "The Quick Brown Fox on a Hill")]);
        // [title] skips function words, capitalizes and concatenates
        assert_eq!(generate_citekey("[title]", &f), "QuickBrownFoxHill");
    }

    #[test]
    fn test_token_shorttitle() {
        let f = fields(&[("title", "The Quick Brown Fox Jumps")]);
        let result = generate_citekey("[shorttitle]", &f);
        assert_eq!(result, "QuickBrownFox");
    }

    #[test]
    fn test_token_veryshorttitle() {
        let f = fields(&[("title", "The Quick Brown Fox")]);
        let result = generate_citekey("[veryshorttitle]", &f);
        assert_eq!(result, "Quick");
    }

    #[test]
    fn test_token_fulltitle() {
        let f = fields(&[("title", "{The Quick Brown Fox}")]);
        assert_eq!(generate_citekey("[fulltitle]", &f), "TheQuickBrownFox");
    }

    #[test]
    fn test_token_camel_n() {
        let f = fields(&[("title", "the quick brown fox jumps")]);
        assert_eq!(generate_citekey("[camel3]", &f), "TheQuickBrown");
    }

    // ── auth tokens (JabRef semantics) ───────────────────────────────────

    #[test]
    fn test_auth_returns_first_author_lastname() {
        let f = fields(&[("author", "Smith, Jane and Jones, Bob")]);
        assert_eq!(generate_citekey("[auth]", &f), "Smith");
    }

    #[test]
    fn test_auth_n_first_n_chars() {
        // JabRef: [auth3] = first 3 chars of first author's last name
        let f = fields(&[("author", "Smith, Jane and Jones, Bob")]);
        assert_eq!(generate_citekey("[auth3]", &f), "Smi");
    }

    #[test]
    fn test_authors_n_first_n_authors() {
        // JabRef: [authors2] = first 2 authors' last names + EtAl if more
        let f = fields(&[("author", "Smith, Jane and Jones, Bob and Williams, Carol")]);
        assert_eq!(generate_citekey("[authors2]", &f), "SmithJonesEtAl");
    }

    #[test]
    fn test_authors_n_no_etal_when_exact() {
        let f = fields(&[("author", "Smith, Jane and Jones, Bob")]);
        assert_eq!(generate_citekey("[authors2]", &f), "SmithJones");
    }

    #[test]
    fn test_auth_dot_etal() {
        let f = fields(&[("author", "Smith, Jane and Jones, Bob and Williams, Carol")]);
        assert_eq!(generate_citekey("[auth.etal]", &f), "Smith.etal");
        let f2 = fields(&[("author", "Smith, Jane")]);
        assert_eq!(generate_citekey("[auth.etal]", &f2), "Smith");
    }

    #[test]
    fn test_auth_et_al_no_dots() {
        let f = fields(&[("author", "Smith, Jane and Jones, Bob and Williams, Carol")]);
        assert_eq!(generate_citekey("[authEtAl]", &f), "SmithEtAl");
        let f2 = fields(&[("author", "Smith, Jane and Jones, Bob")]);
        assert_eq!(generate_citekey("[authEtAl]", &f2), "Smith");
    }

    #[test]
    fn test_auth_auth_ea() {
        let f = fields(&[("author", "Smith, Jane and Jones, Bob and Williams, Carol")]);
        assert_eq!(generate_citekey("[auth.auth.ea]", &f), "Smith.Jones.ea");
        let f2 = fields(&[("author", "Smith, Jane and Jones, Bob")]);
        assert_eq!(generate_citekey("[auth.auth.ea]", &f2), "Smith.Jones");
        let f1 = fields(&[("author", "Smith, Jane")]);
        assert_eq!(generate_citekey("[auth.auth.ea]", &f1), "Smith");
    }

    #[test]
    fn test_authshort() {
        let f1 = fields(&[("author", "Smith, Jane")]);
        assert_eq!(generate_citekey("[authshort]", &f1), "Smith");
        let f3 = fields(&[("author", "Smith, Jane and Jones, Bob and Williams, Carol")]);
        assert_eq!(generate_citekey("[authshort]", &f3), "SJW");
        let f4 = fields(&[("author", "Smith, Jane and Jones, Bob and Williams, Carol and Adams, Dave")]);
        assert_eq!(generate_citekey("[authshort]", &f4), "SJW");
    }

    #[test]
    fn test_author_last() {
        let f = fields(&[("author", "Smith, Jane and Jones, Bob and Williams, Carol")]);
        assert_eq!(generate_citekey("[authorLast]", &f), "Williams");
    }

    #[test]
    fn test_auth_fore_ini() {
        let f = fields(&[("author", "Smith, Jane Marie")]);
        assert_eq!(generate_citekey("[authForeIni]", &f), "J");
        let f2 = fields(&[("author", "Jane Smith")]);
        assert_eq!(generate_citekey("[authForeIni]", &f2), "J");
    }

    #[test]
    fn test_author_ini() {
        let f = fields(&[("author", "Smith, Jane and Jones, Bob and Williams, Carol")]);
        // First 5 chars of "Smith" + initials of Jones, Williams
        assert_eq!(generate_citekey("[authorIni]", &f), "SmithJW");
    }

    #[test]
    fn test_auth_ini_n() {
        let f = fields(&[("author", "Smith, Jane and Jones, Bob and Williams, Carol")]);
        // authIni4 — 4 total chars distributed among 3 authors (1 each, then truncate)
        let result = generate_citekey("[authIni4]", &f);
        assert_eq!(result.len(), 4);
    }

    #[test]
    fn test_auth_n_m() {
        let f = fields(&[("author", "Smith, Jane and Jones, Bob and Williams, Carol")]);
        // auth3_2 = first 3 chars of 2nd author "Jones" = "Jon"
        assert_eq!(generate_citekey("[auth3_2]", &f), "Jon");
    }

    // ── editor fallback ──────────────────────────────────────────────────

    #[test]
    fn test_auth_falls_back_to_editor() {
        let f = fields(&[("editor", "Jones, Bob")]);
        assert_eq!(generate_citekey("[auth]", &f), "Jones");
    }

    #[test]
    fn test_pureauth_no_editor_fallback() {
        let f = fields(&[("editor", "Jones, Bob")]);
        assert_eq!(generate_citekey("[pureauth]", &f), "");
    }

    // ── editor tokens ────────────────────────────────────────────────────

    #[test]
    fn test_edtr_token() {
        let f = fields(&[("editor", "Jones, Bob and Williams, Carol")]);
        assert_eq!(generate_citekey("[edtr]", &f), "Jones");
    }

    #[test]
    fn test_editors_token() {
        let f = fields(&[("editor", "Jones, Bob and Williams, Carol")]);
        assert_eq!(generate_citekey("[editors]", &f), "JonesWilliams");
    }

    #[test]
    fn test_editor_last_token() {
        let f = fields(&[("editor", "Jones, Bob and Williams, Carol")]);
        assert_eq!(generate_citekey("[editorLast]", &f), "Williams");
    }

    // ── entrytype token ──────────────────────────────────────────────────

    #[test]
    fn test_entrytype_token() {
        let f = fields(&[("entrytype", "Article")]);
        assert_eq!(generate_citekey("[entrytype]", &f), "Article");
    }

    // ── page tokens ──────────────────────────────────────────────────────

    #[test]
    fn test_lastpage() {
        let f = fields(&[("pages", "100--200")]);
        assert_eq!(generate_citekey("[lastpage]", &f), "200");
    }

    #[test]
    fn test_pageprefix() {
        let f = fields(&[("pages", "S100--S200")]);
        assert_eq!(generate_citekey("[pageprefix]", &f), "S");
        let f2 = fields(&[("pages", "100--200")]);
        assert_eq!(generate_citekey("[pageprefix]", &f2), "");
    }

    // ── keyword tokens ───────────────────────────────────────────────────

    #[test]
    fn test_keyword_n() {
        let f = fields(&[("keywords", "nuclear, physics, reactor")]);
        assert_eq!(generate_citekey("[keyword1]", &f), "nuclear");
        assert_eq!(generate_citekey("[keyword3]", &f), "reactor");
    }

    #[test]
    fn test_keywords_n() {
        let f = fields(&[("keywords", "nuclear, physics, reactor")]);
        assert_eq!(generate_citekey("[keywords2]", &f), "nuclearphysics");
    }

    // ── ALLCAPS raw field access ─────────────────────────────────────────

    #[test]
    fn test_allcaps_raw_field() {
        let f = fields(&[("author", "Smith, Jane and Jones, Bob")]);
        // [AUTHOR] returns the raw author string (braces cleaned)
        let result = generate_citekey("[AUTHOR]", &f);
        assert!(result.contains("Smith"));
        assert!(result.contains("Jones"));
    }

    // ── modifier tests ───────────────────────────────────────────────────

    #[test]
    fn test_modifier_capitalize() {
        let f = fields(&[("title", "hello WORLD test")]);
        assert_eq!(generate_citekey("[fulltitle:capitalize]", &f), "HelloWorldTest");
    }

    #[test]
    fn test_modifier_sentencecase() {
        let f = fields(&[("title", "HELLO WORLD")]);
        assert_eq!(generate_citekey("[fulltitle:sentencecase]", &f), "Helloworld");
    }

    #[test]
    fn test_modifier_fallback() {
        // Empty value → fallback text
        let f = fields(&[]);
        assert_eq!(generate_citekey("[year:(unknown)]", &f), "unknown");
        // Non-empty value → original
        let f2 = fields(&[("year", "2020")]);
        assert_eq!(generate_citekey("[year:(unknown)]", &f2), "2020");
    }

    #[test]
    fn test_modifier_truncate_n() {
        let f = fields(&[("author", "Smith, Jane")]);
        assert_eq!(generate_citekey("[auth:truncate3]", &f), "Smi");
    }

    // ── existing tests (updated for JabRef semantics) ────────────────────

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
        let f = fields(&[("reportnumber", "ANL-42")]);
        let result = generate_citekey("[reportnumber]", &f);
        assert_eq!(result, "ANL-42");
    }

    #[test]
    fn test_legacy_unknown_token() {
        let f = fields(&[]);
        let result = generate_citekey("{unknown_token}", &f);
        assert_eq!(result, "unknown_token");
    }

    #[test]
    fn test_char_stripping() {
        let f = fields(&[
            ("author", "Jane Smith"),
            ("year", "2020"),
        ]);
        let result = generate_citekey("[auth] [year]", &f);
        assert_eq!(result, "Smith2020");
    }

    #[test]
    fn test_auth_zero_authors() {
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

    // ── shortyear ─────────────────────────────────────────────────────────

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

    // ── firstpage with hyphen ─────────────────────────────────────────────

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

    // ── format_authors_for_key ────────────────────────────────────────────

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

    // ── modifiers ─────────────────────────────────────────────────────────

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
        assert_eq!(generate_citekey("[auth:(3)]", &f), "Smi");
    }

    #[test]
    fn test_modifier_regex() {
        let f = fields(&[("year", "2023")]);
        assert_eq!(generate_citekey(r#"[year:regex("\d\d$", "")]"#, &f), "20");
    }

    #[test]
    fn test_modifier_regex_quoted_args() {
        let f = fields(&[("loc_call_number", "QA76.9.U83R43 1994")]);
        assert_eq!(
            generate_citekey(r#"[loc_call_number:regex(" \d+$", "")]"#, &f),
            "QA76.9.U83R43"
        );
    }

    #[test]
    fn test_modifier_unknown_passthrough() {
        let f = fields(&[("year", "2020")]);
        assert_eq!(generate_citekey("[year:unknown_modifier]", &f), "2020");
    }

    #[test]
    fn test_empty_bracket_token() {
        let f = fields(&[]);
        assert_eq!(generate_citekey("[]", &f), "");
    }

    // ── legacy tokens ─────────────────────────────────────────────────────

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
        assert_eq!(generate_citekey("{booktitle_abbrev}", &f), "ICN");
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

    #[test]
    fn test_journal_abbrev_token_uses_journal_full() {
        let f = fields(&[
            ("journal", "NSE"),
            ("journal_full", "Nuclear Science and Engineering"),
        ]);
        let result = generate_citekey("[journal_abbrev]", &f);
        assert!(!result.is_empty());
    }

    #[test]
    fn test_journal_abbrev_token_falls_back_to_journal() {
        let f = fields(&[("journal", "Nuclear Science Engineering")]);
        assert_eq!(generate_citekey("[journal_abbrev]", &f), "NSE");
    }

    #[test]
    fn test_modifier_regex_with_colon_in_pattern() {
        let f = fields(&[("year", "2020")]);
        let result = generate_citekey(r#"[year:regex("\d+", "X")]"#, &f);
        assert_eq!(result, "X");
    }

    #[test]
    fn test_camel_hyphenated_word() {
        let f = fields(&[("journal", "self-consistent methods")]);
        assert_eq!(generate_citekey("[journal:camel]", &f), "SelfConsistentMethods");
    }

    #[test]
    fn test_camel_slash_separated() {
        let f = fields(&[("journal", "nuclear/reactor physics")]);
        assert_eq!(generate_citekey("[journal:camel]", &f), "NuclearReactorPhysics");
    }

    #[test]
    fn test_empty_optional_trailing_underscore_stripped() {
        let f = fields(&[("year", "2020"), ("author", "Smith, J"), ("journal", "Nuclear Science Engineering")]);
        let result = generate_citekey("{year}_{journal_abbrev}_{author_last}_{pages}", &f);
        assert!(!result.ends_with('_'), "trailing underscore should be stripped: {}", result);
        assert_eq!(result, "2020_NSE_Smith");
    }

    #[test]
    fn test_double_underscore_collapsed() {
        let f = fields(&[("year", "2020")]);
        let result = generate_citekey("{year}__{pages}", &f);
        assert_eq!(result, "2020");
    }

    #[test]
    fn test_citekey_tilde_removed() {
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
        let authors = parse_authors("Smith");
        assert_eq!(authors, vec!["Smith"]);
    }

    // ── Additional coverage tests ────────────────────────────────────────

    #[test]
    fn test_editors_token_direct() {
        // The `editors` match arm in resolve_token (non-auth-family path)
        let f = fields(&[("editor", "Smith, Jane and Jones, Bob and Clark, Carol")]);
        assert_eq!(generate_citekey("[editors]", &f), "SmithJonesEtAl");
    }

    #[test]
    fn test_number_falls_back_to_report_number() {
        let f = fields(&[("report-number", "TR-42")]);
        assert_eq!(generate_citekey("[number]", &f), "TR-42");
    }

    #[test]
    fn test_booktitle_token() {
        let f = fields(&[("booktitle", "{ICML 2024}")]);
        // Space is stripped by sanitizer → "ICML2024"
        assert_eq!(generate_citekey("[booktitle]", &f), "ICML2024");
    }

    #[test]
    fn test_volume_token() {
        let f = fields(&[("volume", "12")]);
        assert_eq!(generate_citekey("[volume]", &f), "12");
    }

    #[test]
    fn test_remap_editor_token_editor_last() {
        // editorLast → authorLast remapping
        let f = fields(&[("editor", "Smith, Jane and Jones, Bob")]);
        assert_eq!(generate_citekey("[editorLast]", &f), "Jones");
    }

    #[test]
    fn test_remap_editor_token_editor_ini() {
        // editorIni → authorIni remapping
        let f = fields(&[("editor", "Smith, Jane and Jones, Bob")]);
        assert_eq!(generate_citekey("[editorIni]", &f), "SmithJ");
    }

    #[test]
    fn test_remap_editor_token_editor_prefix() {
        // editor → author remapping (e.g. editorForeIni → authForeIni doesn't match,
        // but generic editor prefix should still work)
        let f = fields(&[("editor", "Smith, Jane and Jones, Bob and Clark, Carol")]);
        // [editorLast] already tested; test [editors] via editor path
        let result = generate_citekey("[editors]", &f);
        assert_eq!(result, "SmithJonesEtAl");
    }

    #[test]
    fn test_auth_dot_etal_two_authors() {
        let f = fields(&[("author", "Smith, Jane and Jones, Bob")]);
        // 2 authors: no ".etal" suffix
        assert_eq!(generate_citekey("[auth.etal]", &f), "Smith");
    }

    #[test]
    fn test_auth_et_al_two_authors() {
        let f = fields(&[("author", "Smith, Jane and Jones, Bob")]);
        assert_eq!(generate_citekey("[authEtAl]", &f), "Smith");
    }

    #[test]
    fn test_auth_auth_ea_single_author() {
        let f = fields(&[("author", "Smith, Jane")]);
        assert_eq!(generate_citekey("[auth.auth.ea]", &f), "Smith");
    }

    #[test]
    fn test_auth_auth_ea_two_authors() {
        let f = fields(&[("author", "Smith, Jane and Jones, Bob")]);
        assert_eq!(generate_citekey("[auth.auth.ea]", &f), "Smith.Jones");
    }

    #[test]
    fn test_authshort_two_authors() {
        let f = fields(&[("author", "Smith, Jane and Jones, Bob")]);
        // 2 authors: first letter of each
        assert_eq!(generate_citekey("[authshort]", &f), "SJ");
    }

    #[test]
    fn test_authshort_four_authors() {
        let f = fields(&[("author", "Smith, Jane and Jones, Bob and Clark, Carol and Davis, Dan")]);
        // 4+ authors: first 3 letters + "+" but "+" stripped by sanitizer
        assert_eq!(generate_citekey("[authshort]", &f), "SJC");
    }

    #[test]
    fn test_author_last_fore_ini() {
        let f = fields(&[("author", "Smith, Jane and Jones, Bob")]);
        assert_eq!(generate_citekey("[authorLastForeIni]", &f), "B");
    }

    #[test]
    fn test_truncate_n_modifier() {
        let f = fields(&[("title", "Fundamental Algorithms")]);
        assert_eq!(generate_citekey("[title:truncate5]", &f), "Funda");
    }

    #[test]
    fn test_titlecase_modifier() {
        let f = fields(&[("title", "the art of computer programming")]);
        // titlecase produces spaces → sanitizer strips them
        assert_eq!(generate_citekey("[fulltitle:titlecase]", &f), "TheArtofComputerProgramming");
    }

    #[test]
    fn test_sentencecase_modifier() {
        let f = fields(&[("title", "MONTE CARLO METHODS")]);
        // sentencecase produces spaces → sanitizer strips them
        assert_eq!(generate_citekey("[fulltitle:sentencecase]", &f), "Montecarlomethods");
    }

    #[test]
    fn test_capitalize_modifier() {
        let f = fields(&[("title", "monte carlo methods")]);
        // capitalize produces spaces → sanitizer strips them
        assert_eq!(generate_citekey("[fulltitle:capitalize]", &f), "MonteCarloMethods");
    }

    #[test]
    fn test_fallback_modifier_empty_value() {
        // (text) fallback when field is missing → use the text
        let f = fields(&[]);
        assert_eq!(generate_citekey("[year:(unknown)]", &f), "unknown");
    }

    #[test]
    fn test_fallback_modifier_nonempty_value() {
        // (text) fallback when field is present → keep original value
        let f = fields(&[("year", "2024")]);
        assert_eq!(generate_citekey("[year:(unknown)]", &f), "2024");
    }

    #[test]
    fn test_keyword_n_out_of_range() {
        let f = fields(&[("keywords", "foo, bar")]);
        // keyword5 — only 2 keywords, should be empty
        assert_eq!(generate_citekey("[keyword5]", &f), "");
    }

    #[test]
    fn test_camel_n_token() {
        let f = fields(&[("title", "self-consistent field theory")]);
        assert_eq!(generate_citekey("[camel2]", &f), "SelfConsistent");
    }

    #[test]
    fn test_auth_n_m_out_of_range() {
        let f = fields(&[("author", "Smith, Jane")]);
        // auth3_5 = 3 chars of 5th author (doesn't exist)
        assert_eq!(generate_citekey("[auth3_5]", &f), "");
    }

    #[test]
    fn test_format_authors_for_key_zero() {
        let authors: Vec<String> = vec![];
        assert_eq!(format_authors_for_key(&authors), "");
    }

    #[test]
    fn test_pureauth_n_no_editor_fallback() {
        let f = fields(&[("editor", "Jones, Bob")]);
        // pureauth3 should NOT fall back to editor
        assert_eq!(generate_citekey("[pureauth3]", &f), "");
    }

    #[test]
    fn test_regex_modifier_invalid_args() {
        // Malformed regex args (no quotes) → value passes through
        let f = fields(&[("year", "2024")]);
        assert_eq!(generate_citekey("[year:regex(bad)]", &f), "2024");
    }

    #[test]
    fn test_empty_bracket_token_returns_empty() {
        // An empty bracket token `[]` resolves to empty; double underscore collapsed
        assert_eq!(generate_citekey("prefix_[]_suffix", &fields(&[])), "prefix_suffix");
    }

    #[test]
    fn test_authors_token_editor_fallback() {
        // [authors] in resolve_token uses get_author_or_editor → falls back to editor
        let f = fields(&[("editor", "Smith, Jane and Jones, Bob and Clark, Carol")]);
        assert_eq!(generate_citekey("[authors]", &f), "SmithJonesEtAl");
    }

    #[test]
    fn test_parse_quoted_arg_escaped_quote() {
        // regex with escaped quote inside pattern
        let f = fields(&[("title", r#"He said "hello" world"#)]);
        let result = generate_citekey(r#"[fulltitle:regex("\"", "")]"#, &f);
        assert!(!result.contains('"'));
    }
}
