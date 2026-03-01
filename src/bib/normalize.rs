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
