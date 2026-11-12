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
    assert!(
        !raw.warnings.is_empty(),
        "a warning must be recorded for {:?}",
        input
    );
    assert_eq!(
        write_bib_file(&raw),
        input,
        "recovered bytes must round-trip for {:?}",
        input
    );
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
    // It starts a line, so it looks like an entry: say why it was not read.
    assert_eq!(raw.warnings.len(), 1);
    assert!(
        raw.warnings[0]
            .message
            .contains("not followed by '{' or '('"),
        "{:?}",
        raw.warnings
    );
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
    // '#' joins exactly, as BibTeX does: "Part " # "One" → "Part One"
    assert_eq!(db.entries["k"].fields["note"], "Part One");
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

// ── BibTeX syntax accepted by reference BibTeX (review N10) ─────────────────

/// Parse `input`, check it round-trips byte-for-byte without warnings, and
/// return each entry's key with its fields.
fn entries_of(input: &str) -> Vec<(String, Vec<(String, String)>)> {
    let raw = parse_bib_file(input).unwrap();
    assert!(
        raw.warnings.is_empty(),
        "unexpected warnings: {:?}",
        raw.warnings
    );
    assert_eq!(write_bib_file(&raw), input);
    build_database(raw)
        .entries
        .into_values()
        .map(|e| {
            let fields = e.fields.into_iter().collect();
            (e.citation_key, fields)
        })
        .collect()
}

fn entry(key: &str, fields: &[(&str, &str)]) -> (String, Vec<(String, String)>) {
    (
        key.to_string(),
        fields
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
    )
}

#[test]
fn parenthesized_items_are_entries() {
    let input = "@Misc(A, title={Title})\n\n@String(venue = \"Nature\")\n\
@Preamble(\"\\newcommand{\\x}{y}\")\n@Book(B,\n  title = {With (parens) inside},\n  year = 2020\n)\n";
    assert_eq!(
        entries_of(input),
        vec![
            entry("A", &[("title", "Title")]),
            entry("B", &[("title", "With (parens) inside"), ("year", "2020")]),
        ]
    );
}

#[test]
fn line_breaks_around_equals_hash_and_commas_are_whitespace() {
    let input = "@Misc{B,\n  title =\n    {A title},\n  note\n  = {N},\n  \
subtitle = {X}\r\n    # {Y},\r\n  year = 2020\n  ,\n  month = apr\n}\n";
    let entries = entries_of(input);
    assert_eq!(entries.len(), 1);
    let fields = &entries[0].1;
    assert_eq!(fields[0], ("title".to_string(), "A title".to_string()));
    assert_eq!(fields[1], ("note".to_string(), "N".to_string()));
    assert_eq!(fields[2].0, "subtitle");
    assert_eq!(fields[3], ("year".to_string(), "2020".to_string()));
    assert_eq!(fields[4], ("month".to_string(), "apr".to_string()));
}

#[test]
fn quotes_inside_braces_do_not_end_a_quoted_value() {
    let input = "@Misc{Q,\n  title = \"The {\"}Quoted{\"} Word\",\n  note = \"{a \"b\" c}\"\n}\n";
    assert_eq!(
        entries_of(input),
        vec![entry(
            "Q",
            &[
                ("title", "The {\"}Quoted{\"} Word"),
                ("note", "{a \"b\" c}")
            ]
        )]
    );
}

#[test]
fn recovery_resumes_at_an_indented_entry() {
    let input = "@Misc{Bad,\n  title {missing equals},\n}\n  @Misc{Good, title = {Kept}}\n\t@Misc{Also, title = {Kept too}}\n";
    let raw = parse_bib_file(input).unwrap();
    assert_eq!(raw.warnings.len(), 1, "{:?}", raw.warnings);
    assert_eq!(raw.warnings[0].line, 1);
    assert_eq!(write_bib_file(&raw), input);
    let db = build_database(raw);
    let keys: Vec<&str> = db.entries.keys().map(String::as_str).collect();
    assert_eq!(keys, ["Good", "Also"]);
    assert_eq!(db.entries["Good"].fields["title"], "Kept");
}

#[test]
fn mid_line_at_signs_stay_silent_prose() {
    let input = "Contact jane@example.org about @misc usage.\n@Misc{K, title = {T}}\n";
    assert_eq!(entries_of(input), vec![entry("K", &[("title", "T")])]);
}
