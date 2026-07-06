use bibtui::bib::parser::{build_database, parse_bib_file};
use bibtui::bib::writer::write_bib_file;

// ── Parser error recovery paths ───────────────────────────────────────────────
//
// Malformed `@`-items no longer abort the whole file: they are skipped (their
// bytes preserved as Preamble for round-trip) and recorded as a warning.

/// Assert that `input` triggers recovery: parses to Ok, records at least one
/// warning, and round-trips byte-for-byte.
fn assert_recovers(input: &str) {
    let raw = parse_bib_file(input).expect("recoverable input must not hard-fail");
    assert!(!raw.warnings.is_empty(), "a warning must be recorded for {:?}", input);
    assert_eq!(write_bib_file(&raw), input, "recovered bytes must round-trip for {:?}", input);
}

#[test]
fn test_preamble_missing_brace_recovers() {
    assert_recovers("@Preamble nope");
}

#[test]
fn test_string_missing_brace_recovers() {
    assert_recovers("@String nope");
}

#[test]
fn test_entry_missing_open_brace_is_passthrough_text() {
    // No '{' after the entry type — treated as stray inter-entry text (e.g.
    // an email address in a comment line), preserved byte-for-byte.
    let input = "@Article key,\n  author = {A},\n}";
    let raw = parse_bib_file(input).unwrap();
    assert_eq!(write_bib_file(&raw), input);
    // No semantic entry is created from the stray text.
    let db = build_database(raw);
    assert!(db.entries.is_empty());
}

#[test]
fn test_stray_at_in_comment_line_roundtrips() {
    // A '%' comment containing an email address must not abort the file load.
    let input = "% maintained by jane@example.org\n@Article{k,\n  title = {T},\n}\n";
    let raw = parse_bib_file(input).unwrap();
    assert_eq!(write_bib_file(&raw), input);
    let db = build_database(raw);
    assert_eq!(db.entries.len(), 1);
    assert!(db.entries.contains_key("k"));
}

#[test]
fn test_field_missing_equals_recovers() {
    // Field name not followed by '='
    assert_recovers("@Article{key,\n  author {A}\n}");
}

#[test]
fn test_invalid_field_value_char_recovers() {
    // '!' is not a valid start character for a field value
    assert_recovers("@Article{key,\n  author = !\n}");
}

#[test]
fn test_unterminated_braced_value_recovers() {
    // Opening '{' for field value has no matching '}'
    assert_recovers("@Article{key,\n  author = {unclosed");
}

#[test]
fn test_unterminated_quoted_value_recovers() {
    // Opening '"' for field value has no matching '"'
    assert_recovers("@Article{key,\n  author = \"unclosed");
}

#[test]
fn test_unexpected_eof_in_entry_recovers() {
    // Entry body is never closed with '}'
    assert_recovers("@Article{key,\n  author = {A},");
}

// ── String and Preamble happy paths ──────────────────────────────────────────

#[test]
fn test_string_def_roundtrip() {
    let input = "@String{pub = {Some Publisher}}\n\n@Article{k,\n  publisher = pub,\n}\n";
    let raw = parse_bib_file(input).unwrap();
    let output = write_bib_file(&raw);
    assert_eq!(input, output);
}

#[test]
fn test_quoted_field_value_parsed() {
    let input = "@Article{k,\n  title = \"A Quoted Title\",\n}\n";
    let raw = parse_bib_file(input).unwrap();
    let db = build_database(raw);
    assert_eq!(db.entries["k"].fields["title"], "A Quoted Title");
}

#[test]
fn test_concatenated_field_value() {
    // '#' concatenation
    let input = "@Article{k,\n  note = {Part } # {One},\n}\n";
    let raw = parse_bib_file(input).unwrap();
    let db = build_database(raw);
    // '#' concatenation is resolved: "Part " # "One" → "Part One"
    assert_eq!(db.entries["k"].fields["note"], "Part  One");
}

#[test]
fn test_bare_comment_preserved() {
    let input = "@Comment This is a bare comment\n\n@Article{k,\n  author = {A},\n}\n";
    let raw = parse_bib_file(input).unwrap();
    let output = write_bib_file(&raw);
    assert_eq!(input, output);
}

#[test]
fn test_entry_without_trailing_comma() {
    // Last field has no trailing comma
    let input = "@Article{k,\n  author = {A}\n}\n";
    let raw = parse_bib_file(input).unwrap();
    let db = build_database(raw);
    assert_eq!(db.entries["k"].fields["author"], "A");
}

#[test]
fn test_empty_file_parses_ok() {
    let raw = parse_bib_file("").unwrap();
    assert!(raw.items.is_empty());
}

#[test]
fn test_whitespace_only_parses_ok() {
    let raw = parse_bib_file("   \n\n  ").unwrap();
    // Only a Preamble item (whitespace)
    assert_eq!(raw.items.len(), 1);
}

#[test]
fn test_roundtrip_minimal() {
    let input = std::fs::read_to_string("tests/fixtures/minimal.bib").unwrap();
    let raw = parse_bib_file(&input).unwrap();
    let output = write_bib_file(&raw);
    assert_eq!(input, output);
}

#[test]
fn test_roundtrip_complex() {
    let input = std::fs::read_to_string("tests/fixtures/complex_entries.bib").unwrap();
    let raw = parse_bib_file(&input).unwrap();
    let output = write_bib_file(&raw);
    assert_eq!(input, output);
}

#[test]
fn test_bare_month_preserved() {
    let input = "@Article{k,\n  month = apr,\n}\n";
    let raw = parse_bib_file(input).unwrap();
    let output = write_bib_file(&raw);
    assert_eq!(input, output);
}

#[test]
fn test_nested_braces_preserved() {
    let input = "@Article{k,\n  title = {{Some {Nested} Title}},\n}\n";
    let raw = parse_bib_file(input).unwrap();
    let output = write_bib_file(&raw);
    assert_eq!(input, output);
}

#[test]
fn test_entry_count_complex() {
    let input = std::fs::read_to_string("tests/fixtures/complex_entries.bib").unwrap();
    let raw = parse_bib_file(&input).unwrap();
    let db = build_database(raw);
    // 7 entries: Article (Marshak), Article (Brantley), Book, TechReport, IEEEtranBSTCTL, InBook, Misc
    assert_eq!(db.entries.len(), 7);
}

#[test]
fn test_preamble_and_comments_preserved() {
    let input = "% Encoding: UTF-8\n\n;\n\n@Article{k,\n  author = {A},\n}\n";
    let raw = parse_bib_file(input).unwrap();
    let output = write_bib_file(&raw);
    assert_eq!(input, output);
}

#[test]
fn test_multiple_authors() {
    let input = "@Article{k,\n  author = {A and B and C},\n}\n";
    let raw = parse_bib_file(input).unwrap();
    let db = build_database(raw);
    let entry = db.entries.get("k").unwrap();
    assert_eq!(entry.fields.get("author").unwrap(), "A and B and C");
}

#[test]
fn test_latex_special_chars() {
    let input = "@Article{k,\n  author = {Peir{\\'{o}}},\n}\n";
    let raw = parse_bib_file(input).unwrap();
    let output = write_bib_file(&raw);
    assert_eq!(input, output);
}

#[test]
fn test_field_alignment_preserved() {
    let input = "@Book{k,\n  author    = {A},\n  publisher = {P},\n  title     = {T},\n}\n";
    let raw = parse_bib_file(input).unwrap();
    let output = write_bib_file(&raw);
    assert_eq!(input, output);
}
