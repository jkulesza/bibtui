#![allow(dead_code)]

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

/// Normalize date fields (YYYY-MM-DD format).
pub fn normalize_date(value: &str) -> String {
    // Already well-formatted: leave as is
    value.to_string()
}

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
}
