use bibtui::bib::parser::{build_database, parse_bib_file};
use bibtui::bib::writer::write_bib_file;

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
    // 5 entries: Article, Book, TechReport, IEEEtranBSTCTL, Misc
    assert_eq!(db.entries.len(), 5);
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
