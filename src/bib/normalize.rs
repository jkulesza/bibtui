/// Normalize month fields to standard BibTeX three-letter abbreviations.
pub fn normalize_month(value: &str) -> String {
    let lower = value.to_lowercase();
    let lower = lower.trim();

    match lower {
        "january" | "jan" | "1" => "jan".to_string(),
        "february" | "feb" | "2" => "feb".to_string(),
        "march" | "mar" | "3" => "mar".to_string(),
        "april" | "apr" | "4" => "apr".to_string(),
        "may" | "5" => "may".to_string(),
        "june" | "jun" | "6" => "jun".to_string(),
        "july" | "jul" | "7" => "jul".to_string(),
        "august" | "aug" | "8" => "aug".to_string(),
        "september" | "sep" | "sept" | "9" => "sep".to_string(),
        "october" | "oct" | "10" => "oct".to_string(),
        "november" | "nov" | "11" => "nov".to_string(),
        "december" | "dec" | "12" => "dec".to_string(),
        _ => value.to_string(),
    }
}

/// Normalize page numbers: replace single hyphens with en-dashes (--).
pub fn normalize_page_numbers(value: &str) -> String {
    // Already has en-dash: leave as is
    if value.contains("--") {
        return value.to_string();
    }
    // Single hyphen between numbers: replace with en-dash
    value.replace('-', "--")
}

/// Normalize date fields to ISO 8601 yyyy-MM-dd (or yyyy-MM / yyyy) format.
///
/// Recognises:
/// - Already-ISO forms (`yyyy`, `yyyy-MM`, `yyyy-MM-dd`) — returned unchanged.
/// - `d.M.yyyy` and `dd.MM.yyyy` (e.g. European dot notation).
/// - `M/yyyy` (e.g. `3/2020` → `2020-03`).
/// - `Month dd, yyyy` or `Month dd yyyy` (e.g. `March 15 2020`).
/// - `Month yyyy` (e.g. `March 2020`).
///
/// Unrecognised formats are returned unchanged.
pub fn normalize_date(value: &str) -> String {
    let s = value.trim();

    // Already ISO: yyyy, yyyy-MM, or yyyy-MM-dd
    if is_iso_date(s) {
        return s.to_string();
    }

    // d.M.yyyy or dd.MM.yyyy
    if s.contains('.') {
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() == 3 {
            if let (Ok(d), Ok(m), Ok(y)) = (
                parts[0].trim().parse::<u32>(),
                parts[1].trim().parse::<u32>(),
                parts[2].trim().parse::<i32>(),
            ) {
                if m >= 1 && m <= 12 && d >= 1 && d <= 31 && y > 0 {
                    return format!("{:04}-{:02}-{:02}", y, m, d);
                }
            }
        }
    }

    // M/yyyy
    if s.contains('/') {
        let parts: Vec<&str> = s.split('/').collect();
        if parts.len() == 2 {
            if let (Ok(m), Ok(y)) = (
                parts[0].trim().parse::<u32>(),
                parts[1].trim().parse::<i32>(),
            ) {
                if m >= 1 && m <= 12 && y > 0 {
                    return format!("{:04}-{:02}", y, m);
                }
            }
        }
    }

    // "Month[ dd[,]] yyyy" — e.g. "March 15 2020", "March 2020"
    if let Some(result) = try_parse_month_year(s) {
        return result;
    }

    s.to_string()
}

/// Return true if `s` looks like an ISO date (yyyy, yyyy-MM, or yyyy-MM-dd).
fn is_iso_date(s: &str) -> bool {
    let parts: Vec<&str> = s.split('-').collect();
    match parts.len() {
        1 => parts[0].len() == 4 && parts[0].chars().all(|c| c.is_ascii_digit()),
        2 => {
            parts[0].len() == 4
                && parts[0].chars().all(|c| c.is_ascii_digit())
                && parts[1].len() == 2
                && parts[1].chars().all(|c| c.is_ascii_digit())
        }
        3 => {
            parts[0].len() == 4
                && parts[0].chars().all(|c| c.is_ascii_digit())
                && parts[1].len() == 2
                && parts[1].chars().all(|c| c.is_ascii_digit())
                && parts[2].len() == 2
                && parts[2].chars().all(|c| c.is_ascii_digit())
        }
        _ => false,
    }
}

/// Try to parse "Month [dd[,]] yyyy" or "Month yyyy".
fn try_parse_month_year(s: &str) -> Option<String> {
    // Strip trailing comma variants and split on whitespace
    let s = s.replace(',', " ");
    let tokens: Vec<&str> = s.split_whitespace().collect();
    match tokens.len() {
        2 => {
            // "Month yyyy"
            let m = month_name_to_num(tokens[0])?;
            let y: i32 = tokens[1].parse().ok()?;
            if y > 0 {
                Some(format!("{:04}-{:02}", y, m))
            } else {
                None
            }
        }
        3 => {
            // "Month dd yyyy"
            let m = month_name_to_num(tokens[0])?;
            let d: u32 = tokens[1].parse().ok()?;
            let y: i32 = tokens[2].parse().ok()?;
            if d >= 1 && d <= 31 && y > 0 {
                Some(format!("{:04}-{:02}-{:02}", y, m, d))
            } else {
                None
            }
        }
        _ => None,
    }
}

fn month_name_to_num(s: &str) -> Option<u32> {
    match s.to_lowercase().as_str() {
        "january" | "jan" => Some(1),
        "february" | "feb" => Some(2),
        "march" | "mar" => Some(3),
        "april" | "apr" => Some(4),
        "may" => Some(5),
        "june" | "jun" => Some(6),
        "july" | "jul" => Some(7),
        "august" | "aug" => Some(8),
        "september" | "sep" | "sept" => Some(9),
        "october" | "oct" => Some(10),
        "november" | "nov" => Some(11),
        "december" | "dec" => Some(12),
        _ => None,
    }
}

/// Escape bare underscores (`_`) as `\_` for LaTeX compatibility.
///
/// Already-escaped `\_` sequences and underscores inside math mode (`$...$`)
/// are left untouched.
pub fn escape_underscores(s: &str) -> String {
    if !s.contains('_') {
        return s.to_string();
    }
    let chars: Vec<char> = s.chars().collect();
    let n = chars.len();
    let mut result = String::with_capacity(s.len() + 8);
    let mut in_math = false;
    let mut i = 0;
    while i < n {
        let c = chars[i];
        if c == '$' {
            in_math = !in_math;
            result.push(c);
        } else if c == '_' && !in_math && (i == 0 || chars[i - 1] != '\\') {
            result.push_str("\\_");
        } else {
            result.push(c);
        }
        i += 1;
    }
    result
}

/// Escape bare ampersands (`&`) as `\&` for LaTeX compatibility.
///
/// Already-escaped `\&` sequences are left untouched.
pub fn escape_ampersands(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    let chars: Vec<char> = s.chars().collect();
    let n = chars.len();
    let mut result = String::with_capacity(s.len() + 8);
    let mut i = 0;
    while i < n {
        let c = chars[i];
        if c == '&' && (i == 0 || chars[i - 1] != '\\') {
            result.push_str("\\&");
        } else {
            result.push(c);
        }
        i += 1;
    }
    result
}

/// Decode percent-encoded characters in a URL string.
///
/// Only printable ASCII sequences (`%20`–`%7E`) are decoded; non-ASCII
/// percent-encoded bytes are left in their encoded form.
pub fn cleanup_url(s: &str) -> String {
    if !s.contains('%') {
        return s.to_string();
    }
    let mut result = String::with_capacity(s.len());
    let mut remaining = s;
    while !remaining.is_empty() {
        if let Some(pos) = remaining.find('%') {
            result.push_str(&remaining[..pos]);
            let after = &remaining[pos + 1..];
            if after.len() >= 2 {
                let hex = &after[..2];
                if let Ok(byte) = u8::from_str_radix(hex, 16) {
                    if byte >= 0x20 && byte < 0x7F {
                        result.push(byte as char);
                        remaining = &remaining[pos + 3..];
                        continue;
                    }
                }
            }
            result.push('%');
            remaining = &remaining[pos + 1..];
        } else {
            result.push_str(remaining);
            break;
        }
    }
    result
}

/// Basic LaTeX cleanup: escape bare `%` signs and collapse multiple spaces.
///
/// - `%` not preceded by `\` is replaced with `\%`.
/// - Runs of two or more consecutive spaces are collapsed to a single space.
pub fn latex_cleanup(s: &str) -> String {
    // Escape bare percent signs
    let mut percent_escaped = String::with_capacity(s.len());
    let mut prev = '\0';
    for c in s.chars() {
        if c == '%' && prev != '\\' {
            percent_escaped.push_str("\\%");
        } else {
            percent_escaped.push(c);
        }
        prev = c;
    }
    // Collapse multiple spaces
    let mut result = String::with_capacity(percent_escaped.len());
    let mut prev_space = false;
    for c in percent_escaped.chars() {
        if c == ' ' {
            if !prev_space {
                result.push(c);
            }
            prev_space = true;
        } else {
            result.push(c);
            prev_space = false;
        }
    }
    result
}

/// Convert ordinal numbers to LaTeX superscript form.
///
/// Examples: `1st` → `1\textsuperscript{st}`, `2nd` → `2\textsuperscript{nd}`.
/// The suffix must be followed by a non-alphabetic character (or end of string)
/// to avoid matching inside words like "mast" or "standard".
pub fn ordinals_to_superscript(s: &str) -> String {
    if !s.chars().any(|c| c.is_ascii_digit()) {
        return s.to_string();
    }
    let mut result = String::with_capacity(s.len() + 32);
    let mut remaining = s;
    while !remaining.is_empty() {
        match remaining.find(|c: char| c.is_ascii_digit()) {
            None => {
                result.push_str(remaining);
                break;
            }
            Some(pos) => {
                result.push_str(&remaining[..pos]);
                remaining = &remaining[pos..];
                // Collect the digit run
                let digit_end = remaining
                    .find(|c: char| !c.is_ascii_digit())
                    .unwrap_or(remaining.len());
                let digits = &remaining[..digit_end];
                remaining = &remaining[digit_end..];
                // Check for ordinal suffix
                if let Some((suf, suf_len)) = ordinal_suffix(remaining) {
                    result.push_str(digits);
                    result.push_str("\\textsuperscript{");
                    result.push_str(suf);
                    result.push('}');
                    remaining = &remaining[suf_len..];
                } else {
                    result.push_str(digits);
                }
            }
        }
    }
    result
}

/// If `s` starts with an ordinal suffix (`st`, `nd`, `rd`, or `th`, case-insensitive)
/// and the character after the suffix is not alphabetic (or `s` ends there),
/// return `(canonical_suffix, byte_length)`.
fn ordinal_suffix(s: &str) -> Option<(&'static str, usize)> {
    if s.len() < 2 {
        return None;
    }
    let candidate = &s[..2];
    let after = &s[2..];
    // Word-boundary check: next char must not be alphabetic
    if after.starts_with(|c: char| c.is_alphabetic()) {
        return None;
    }
    for &(pat, canonical) in &[("st", "st"), ("nd", "nd"), ("rd", "rd"), ("th", "th")] {
        if candidate.eq_ignore_ascii_case(pat) {
            return Some((canonical, 2));
        }
    }
    None
}

/// Convert Unicode characters to their LaTeX equivalents.
///
/// Covers accented Latin letters (acute, grave, circumflex, diaeresis, tilde,
/// cedilla, caron, breve, macron, double acute, ogonek), special letters
/// (ß, æ, œ, å, ø, ł, …), typographic dashes, curly quotes, and common
/// symbols (©, ®, ™, °, ×, …).
pub fn unicode_to_latex(s: &str) -> String {
    if s.is_ascii() {
        return s.to_string();
    }
    let mut result = String::with_capacity(s.len() * 2);
    for c in s.chars() {
        if let Some(latex) = char_to_latex(c) {
            result.push_str(latex);
        } else {
            result.push(c);
        }
    }
    result
}

fn char_to_latex(c: char) -> Option<&'static str> {
    // Binary search would be faster but the table is small enough for linear scan.
    UNICODE_TO_LATEX.iter().find(|(uc, _)| *uc == c).map(|(_, latex)| *latex)
}

/// Unicode character → canonical BibTeX/LaTeX representation.
///
/// Em-dash and en-dash come first so that they are found before any character
/// that shares a prefix in `--` / `---` expansions (not relevant here, but
/// kept for consistency with the render_latex convention of longer-first).
static UNICODE_TO_LATEX: &[(char, &str)] = &[
    // ── Dashes & non-breaking space ───────────────────────────────────────────
    ('\u{2014}', "---"),   // em-dash  —
    ('\u{2013}', "--"),    // en-dash  –
    ('\u{00A0}', "~"),     // non-breaking space
    // ── Curly quotes ──────────────────────────────────────────────────────────
    ('\u{201C}', "``"),    // left  double "
    ('\u{201D}', "''"),    // right double "
    ('\u{2018}', "`"),     // left  single '
    ('\u{2019}', "'"),     // right single '
    // ── Ellipsis ──────────────────────────────────────────────────────────────
    ('\u{2026}', "\\ldots{}"), // …
    // ── Common symbols ────────────────────────────────────────────────────────
    ('\u{00A9}', "\\textcopyright{}"),   // ©
    ('\u{00AE}', "\\textregistered{}"),  // ®
    ('\u{2122}', "\\texttrademark{}"),   // ™
    ('\u{00B0}', "\\textdegree{}"),      // °
    ('\u{00D7}', "\\times{}"),           // ×  (multiplication sign)
    ('\u{00B1}', "\\pm{}"),              // ±
    // ── Special letters ───────────────────────────────────────────────────────
    ('ß', "{\\ss}"),
    ('æ', "{\\ae}"), ('Æ', "{\\AE}"),
    ('œ', "{\\oe}"), ('Œ', "{\\OE}"),
    ('å', "{\\aa}"), ('Å', "{\\AA}"),
    ('ø', "{\\o}"),  ('Ø', "{\\O}"),
    ('ł', "{\\l}"),  ('Ł', "{\\L}"),
    // ── Acute accent ──────────────────────────────────────────────────────────
    ('á', "{\\'a}"), ('Á', "{\\'A}"),
    ('é', "{\\'e}"), ('É', "{\\'E}"),
    ('í', "{\\'i}"), ('Í', "{\\'I}"),
    ('ó', "{\\'o}"), ('Ó', "{\\'O}"),
    ('ú', "{\\'u}"), ('Ú', "{\\'U}"),
    ('ý', "{\\'y}"), ('Ý', "{\\'Y}"),
    ('ć', "{\\'c}"), ('Ć', "{\\'C}"),
    ('ń', "{\\'n}"), ('Ń', "{\\'N}"),
    ('ś', "{\\'s}"), ('Ś', "{\\'S}"),
    ('ź', "{\\'z}"), ('Ź', "{\\'Z}"),
    // ── Grave accent ──────────────────────────────────────────────────────────
    ('à', "{\\`a}"), ('À', "{\\`A}"),
    ('è', "{\\`e}"), ('È', "{\\`E}"),
    ('ì', "{\\`i}"), ('Ì', "{\\`I}"),
    ('ò', "{\\`o}"), ('Ò', "{\\`O}"),
    ('ù', "{\\`u}"), ('Ù', "{\\`U}"),
    // ── Circumflex ────────────────────────────────────────────────────────────
    ('â', "{\\^a}"), ('Â', "{\\^A}"),
    ('ê', "{\\^e}"), ('Ê', "{\\^E}"),
    ('î', "{\\^i}"), ('Î', "{\\^I}"),
    ('ô', "{\\^o}"), ('Ô', "{\\^O}"),
    ('û', "{\\^u}"), ('Û', "{\\^U}"),
    // ── Diaeresis / Umlaut ────────────────────────────────────────────────────
    ('ä', "{\\\"a}"), ('Ä', "{\\\"A}"),
    ('ë', "{\\\"e}"), ('Ë', "{\\\"E}"),
    ('ï', "{\\\"i}"), ('Ï', "{\\\"I}"),
    ('ö', "{\\\"o}"), ('Ö', "{\\\"O}"),
    ('ü', "{\\\"u}"), ('Ü', "{\\\"U}"),
    ('ÿ', "{\\\"y}"), ('Ÿ', "{\\\"Y}"),
    // ── Tilde ─────────────────────────────────────────────────────────────────
    ('ã', "{\\~a}"), ('Ã', "{\\~A}"),
    ('ñ', "{\\~n}"), ('Ñ', "{\\~N}"),
    ('õ', "{\\~o}"), ('Õ', "{\\~O}"),
    // ── Cedilla ───────────────────────────────────────────────────────────────
    ('ç', "{\\c{c}}"), ('Ç', "{\\c{C}}"),
    // ── Caron ─────────────────────────────────────────────────────────────────
    ('č', "{\\v{c}}"), ('Č', "{\\v{C}}"),
    ('š', "{\\v{s}}"), ('Š', "{\\v{S}}"),
    ('ž', "{\\v{z}}"), ('Ž', "{\\v{Z}}"),
    ('ř', "{\\v{r}}"), ('Ř', "{\\v{R}}"),
    ('ň', "{\\v{n}}"), ('Ň', "{\\v{N}}"),
    ('ě', "{\\v{e}}"), ('Ě', "{\\v{E}}"),
    // ── Double acute (Hungarian) ──────────────────────────────────────────────
    ('ő', "{\\H{o}}"), ('Ő', "{\\H{O}}"),
    ('ű', "{\\H{u}}"), ('Ű', "{\\H{U}}"),
    // ── Ogonek ────────────────────────────────────────────────────────────────
    ('ą', "{\\k{a}}"), ('Ą', "{\\k{A}}"),
    ('ę', "{\\k{e}}"), ('Ę', "{\\k{E}}"),
    // ── Breve ─────────────────────────────────────────────────────────────────
    ('ă', "{\\u{a}}"), ('Ă', "{\\u{A}}"),
    ('ğ', "{\\u{g}}"), ('Ğ', "{\\u{G}}"),
    // ── Macron ────────────────────────────────────────────────────────────────
    ('ā', "{\\=a}"), ('Ā', "{\\=A}"),
    ('ē', "{\\=e}"), ('Ē', "{\\=E}"),
    ('ī', "{\\=i}"), ('Ī', "{\\=I}"),
    ('ō', "{\\=o}"), ('Ō', "{\\=O}"),
    ('ū', "{\\=u}"), ('Ū', "{\\=U}"),
    // ── Dot above ─────────────────────────────────────────────────────────────
    ('ċ', "{\\.c}"), ('Ċ', "{\\.C}"),
    ('ġ', "{\\.g}"), ('Ġ', "{\\.G}"),
    ('ż', "{\\.z}"), ('Ż', "{\\.Z}"),
];

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── normalize_month ───────────────────────────────────────────────────────

    #[test]
    fn test_normalize_month_full_names() {
        assert_eq!(normalize_month("january"), "jan");
        assert_eq!(normalize_month("february"), "feb");
        assert_eq!(normalize_month("march"), "mar");
        assert_eq!(normalize_month("april"), "apr");
        assert_eq!(normalize_month("may"), "may");
        assert_eq!(normalize_month("june"), "jun");
        assert_eq!(normalize_month("july"), "jul");
        assert_eq!(normalize_month("august"), "aug");
        assert_eq!(normalize_month("september"), "sep");
        assert_eq!(normalize_month("october"), "oct");
        assert_eq!(normalize_month("november"), "nov");
        assert_eq!(normalize_month("december"), "dec");
    }

    #[test]
    fn test_normalize_month_abbreviations() {
        assert_eq!(normalize_month("jan"), "jan");
        assert_eq!(normalize_month("feb"), "feb");
        assert_eq!(normalize_month("mar"), "mar");
        assert_eq!(normalize_month("apr"), "apr");
        assert_eq!(normalize_month("jun"), "jun");
        assert_eq!(normalize_month("jul"), "jul");
        assert_eq!(normalize_month("aug"), "aug");
        assert_eq!(normalize_month("sep"), "sep");
        assert_eq!(normalize_month("sept"), "sep");
        assert_eq!(normalize_month("oct"), "oct");
        assert_eq!(normalize_month("nov"), "nov");
        assert_eq!(normalize_month("dec"), "dec");
    }

    #[test]
    fn test_normalize_month_numeric() {
        assert_eq!(normalize_month("1"), "jan");
        assert_eq!(normalize_month("2"), "feb");
        assert_eq!(normalize_month("3"), "mar");
        assert_eq!(normalize_month("4"), "apr");
        assert_eq!(normalize_month("5"), "may");
        assert_eq!(normalize_month("6"), "jun");
        assert_eq!(normalize_month("7"), "jul");
        assert_eq!(normalize_month("8"), "aug");
        assert_eq!(normalize_month("9"), "sep");
        assert_eq!(normalize_month("10"), "oct");
        assert_eq!(normalize_month("11"), "nov");
        assert_eq!(normalize_month("12"), "dec");
    }

    #[test]
    fn test_normalize_month_unknown() {
        assert_eq!(normalize_month("unknown"), "unknown");
        assert_eq!(normalize_month("13"), "13");
        assert_eq!(normalize_month("0"), "0");
        assert_eq!(normalize_month("spring"), "spring");
    }

    #[test]
    fn test_normalize_month_case_insensitive() {
        assert_eq!(normalize_month("JANUARY"), "jan");
        assert_eq!(normalize_month("January"), "jan");
        assert_eq!(normalize_month("JAN"), "jan");
    }

    // ── normalize_page_numbers ────────────────────────────────────────────────

    #[test]
    fn test_normalize_page_numbers_already_en_dash() {
        assert_eq!(normalize_page_numbers("100--110"), "100--110");
        assert_eq!(normalize_page_numbers("1--5"), "1--5");
    }

    #[test]
    fn test_normalize_page_numbers_single_hyphen() {
        assert_eq!(normalize_page_numbers("100-110"), "100--110");
        assert_eq!(normalize_page_numbers("1-5"), "1--5");
    }

    #[test]
    fn test_normalize_page_numbers_multiple_hyphens() {
        // Multiple single hyphens (e.g., "100-110-120") each get doubled
        assert_eq!(normalize_page_numbers("100-110-120"), "100--110--120");
    }

    #[test]
    fn test_normalize_page_numbers_no_hyphen() {
        assert_eq!(normalize_page_numbers("100"), "100");
        assert_eq!(normalize_page_numbers("xii"), "xii");
    }

    // ── normalize_date ────────────────────────────────────────────────────────

    #[test]
    fn test_normalize_date_already_iso() {
        assert_eq!(normalize_date("2020-03-15"), "2020-03-15");
        assert_eq!(normalize_date("2020-03"), "2020-03");
        assert_eq!(normalize_date("2020"), "2020");
    }

    #[test]
    fn test_normalize_date_dot_notation() {
        assert_eq!(normalize_date("15.3.2020"), "2020-03-15");
        assert_eq!(normalize_date("1.12.2019"), "2019-12-01");
    }

    #[test]
    fn test_normalize_date_slash_month_year() {
        assert_eq!(normalize_date("3/2020"), "2020-03");
        assert_eq!(normalize_date("12/2019"), "2019-12");
    }

    #[test]
    fn test_normalize_date_month_name() {
        assert_eq!(normalize_date("March 15 2020"), "2020-03-15");
        assert_eq!(normalize_date("March 2020"), "2020-03");
        assert_eq!(normalize_date("march 2020"), "2020-03");
    }

    #[test]
    fn test_normalize_date_month_name_with_comma() {
        assert_eq!(normalize_date("March 15, 2020"), "2020-03-15");
    }

    #[test]
    fn test_normalize_date_unknown() {
        assert_eq!(normalize_date("Spring 2020"), "Spring 2020");
        assert_eq!(normalize_date(""), "");
    }

    // ── escape_underscores ────────────────────────────────────────────────────

    #[test]
    fn test_escape_underscores_basic() {
        assert_eq!(escape_underscores("foo_bar"), "foo\\_bar");
    }

    #[test]
    fn test_escape_underscores_already_escaped() {
        assert_eq!(escape_underscores("foo\\_bar"), "foo\\_bar");
    }

    #[test]
    fn test_escape_underscores_in_math_mode() {
        assert_eq!(escape_underscores("$x_i$"), "$x_i$");
    }

    #[test]
    fn test_escape_underscores_no_underscore() {
        assert_eq!(escape_underscores("no underscore here"), "no underscore here");
    }

    // ── escape_ampersands ─────────────────────────────────────────────────────

    #[test]
    fn test_escape_ampersands_basic() {
        assert_eq!(escape_ampersands("Tom & Jerry"), "Tom \\& Jerry");
    }

    #[test]
    fn test_escape_ampersands_already_escaped() {
        assert_eq!(escape_ampersands("Tom \\& Jerry"), "Tom \\& Jerry");
    }

    #[test]
    fn test_escape_ampersands_no_ampersand() {
        assert_eq!(escape_ampersands("no ampersand"), "no ampersand");
    }

    // ── cleanup_url ───────────────────────────────────────────────────────────

    #[test]
    fn test_cleanup_url_basic() {
        assert_eq!(
            cleanup_url("http%3A%2F%2Fexample.com%2Fpage"),
            "http://example.com/page"
        );
    }

    #[test]
    fn test_cleanup_url_no_encoding() {
        assert_eq!(cleanup_url("https://example.com"), "https://example.com");
    }

    #[test]
    fn test_cleanup_url_space_encoding() {
        assert_eq!(cleanup_url("foo%20bar"), "foo bar");
    }

    // ── latex_cleanup ─────────────────────────────────────────────────────────

    #[test]
    fn test_latex_cleanup_percent() {
        assert_eq!(latex_cleanup("50% yield"), "50\\% yield");
    }

    #[test]
    fn test_latex_cleanup_already_escaped_percent() {
        assert_eq!(latex_cleanup("50\\% yield"), "50\\% yield");
    }

    #[test]
    fn test_latex_cleanup_multiple_spaces() {
        assert_eq!(latex_cleanup("foo  bar   baz"), "foo bar baz");
    }

    // ── ordinals_to_superscript ───────────────────────────────────────────────

    #[test]
    fn test_ordinals_basic() {
        assert_eq!(
            ordinals_to_superscript("1st Conference"),
            "1\\textsuperscript{st} Conference"
        );
        assert_eq!(
            ordinals_to_superscript("the 2nd Annual Meeting"),
            "the 2\\textsuperscript{nd} Annual Meeting"
        );
        assert_eq!(
            ordinals_to_superscript("3rd Edition"),
            "3\\textsuperscript{rd} Edition"
        );
        assert_eq!(
            ordinals_to_superscript("4th Symposium"),
            "4\\textsuperscript{th} Symposium"
        );
    }

    #[test]
    fn test_ordinals_not_matched_inside_word() {
        // "mast" contains "st" but not as an ordinal suffix after digits
        assert_eq!(ordinals_to_superscript("mastermind"), "mastermind");
    }

    #[test]
    fn test_ordinals_no_digits() {
        assert_eq!(ordinals_to_superscript("no digits here"), "no digits here");
    }

    #[test]
    fn test_ordinals_number_not_ordinal() {
        // Plain number with no suffix → unchanged
        assert_eq!(ordinals_to_superscript("2020 Conference"), "2020 Conference");
    }

    // ── unicode_to_latex ──────────────────────────────────────────────────────

    #[test]
    fn test_unicode_to_latex_acute() {
        assert_eq!(unicode_to_latex("é"), "{\\'e}");
        assert_eq!(unicode_to_latex("Ångström"), "{\\AA}ngstr{\\\"o}m");
    }

    #[test]
    fn test_unicode_to_latex_diaeresis() {
        assert_eq!(unicode_to_latex("ö"), "{\\\"o}");
        assert_eq!(unicode_to_latex("Müller"), "M{\\\"u}ller");
    }

    #[test]
    fn test_unicode_to_latex_special() {
        assert_eq!(unicode_to_latex("ß"), "{\\ss}");
        assert_eq!(unicode_to_latex("ñ"), "{\\~n}");
    }

    #[test]
    fn test_unicode_to_latex_dashes() {
        assert_eq!(unicode_to_latex("pp. 1\u{2013}10"), "pp. 1--10");
        assert_eq!(unicode_to_latex("foo\u{2014}bar"), "foo---bar");
    }

    #[test]
    fn test_unicode_to_latex_ascii_passthrough() {
        assert_eq!(unicode_to_latex("plain ASCII text"), "plain ASCII text");
    }
}
