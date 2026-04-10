use bibtui::bib::parser::{build_database, parse_bib_file};
use bibtui::bib::writer::write_bib_file;

// ── Parser error paths ────────────────────────────────────────────────────────

#[test]
fn test_preamble_missing_brace_errors() {
    assert!(parse_bib_file("@Preamble nope").is_err());
}

#[test]
fn test_string_missing_brace_errors() {
    assert!(parse_bib_file("@String nope").is_err());
}

#[test]
fn test_entry_missing_open_brace_errors() {
    // No '{' after the entry type
    assert!(parse_bib_file("@Article key,\n  author = {A},\n}").is_err());
}

#[test]
fn test_field_missing_equals_errors() {
    // Field name not followed by '='
    assert!(parse_bib_file("@Article{key,\n  author {A}\n}").is_err());
}

#[test]
fn test_invalid_field_value_char_errors() {
    // '!' is not a valid start character for a field value
    assert!(parse_bib_file("@Article{key,\n  author = !\n}").is_err());
}

#[test]
fn test_unterminated_braced_value_errors() {
    // Opening '{' for field value has no matching '}'
    assert!(parse_bib_file("@Article{key,\n  author = {unclosed").is_err());
}

#[test]
fn test_unterminated_quoted_value_errors() {
    // Opening '"' for field value has no matching '"'
    assert!(parse_bib_file("@Article{key,\n  author = \"unclosed").is_err());
}

#[test]
fn test_unexpected_eof_in_entry_errors() {
    // Entry body is never closed with '}'
    assert!(parse_bib_file("@Article{key,\n  author = {A},").is_err());
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
