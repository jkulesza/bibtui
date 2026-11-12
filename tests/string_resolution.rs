//! `@String` resolution and `#` concatenation (review N16), checked against
//! the values reference BibTeX produces for `fixtures/string_resolution.bib`
//! (bibtex 0.99d with a style that prints each field). Where bibtui
//! deliberately differs, the test says so and gives BibTeX's value.

use bibtui::bib::model::Database;
use bibtui::bib::parser::{build_database, parse_bib_file};
use bibtui::bib::writer::write_bib_file;

const FIXTURE: &str = include_str!("fixtures/string_resolution.bib");

fn database() -> Database {
    let raw = parse_bib_file(FIXTURE).unwrap();
    assert!(raw.warnings.is_empty(), "{:?}", raw.warnings);
    assert_eq!(write_bib_file(&raw), FIXTURE, "byte-perfect round trip");
    build_database(raw)
}

fn value<'a>(db: &'a Database, key: &str, field: &str) -> &'a str {
    &db.entries[key].fields[field]
}

#[test]
fn values_match_reference_bibtex() {
    let db = database();
    for (key, field, bibtex) in [
        // A definition using another definition, and a literal suffix.
        (
            "Nested",
            "journal",
            "Nuclear Science and Engineering, Supplement",
        ),
        // Concatenation joins exactly: no space is added between parts.
        ("Nested", "note", "prefix"),
        ("Nested", "title", "A B"),
        // Names are compared without case.
        ("Case", "journal", "Nuclear Science and Engineering"),
        ("Case", "note", "Nuclear Technologys"),
        // Numbers are literals and concatenate like strings.
        ("Numbers", "year", "2020"),
        ("Numbers", "volume", "12a"),
        // `@String(...)`, and a redefinition of a month name.
        ("Redefined", "journal", "Paren"),
        ("Redefined", "month", "Mayo"),
    ] {
        assert_eq!(value(&db, key, field), bibtex, "{key}.{field}");
    }
}

#[test]
fn documented_differences_from_bibtex() {
    let db = database();
    for (key, field, bibtui, bibtex) in [
        // A definition naming itself extends the earlier one (with a warning);
        // BibTeX treats the name as undefined there.
        (
            "Nested",
            "institution",
            "Los Alamos National Laboratory",
            "National Laboratory",
        ),
        // Text is kept as written; BibTeX also collapses runs of whitespace
        // and trims each value (bibtui's trim-whitespace save action trims).
        ("Spaces", "note", " lead  mid trail ", "lead mid trail"),
        (
            "Spaces",
            "title",
            "Two  spaces\n    and a line break",
            "Two spaces and a line break",
        ),
        // Built-in months keep their abbreviation, which bibtui's month
        // handling uses; BibTeX styles define them as full names.
        ("Months", "month", "apr", "April"),
        ("Months", "note", "jan and dec", "January and December"),
        // An undefined name keeps its name (it is often defined in another
        // file, such as IEEEabrv); BibTeX makes it empty.
        ("Undefined", "journal", "nowhere", ""),
        ("Undefined", "note", "xnowherey", "xy"),
        // A name used above its definition is resolved (with a warning);
        // BibTeX reads the file in order and leaves it undefined.
        ("Forward", "journal", "Defined Later", ""),
    ] {
        assert_eq!(
            value(&db, key, field),
            bibtui,
            "{key}.{field} (BibTeX: {bibtex:?})"
        );
    }
}

#[test]
fn problems_are_reported_with_their_lines() {
    // Each problem is reported at the line where its item begins.
    let db = database();
    let warnings: Vec<(usize, &str)> = db
        .value_warnings
        .iter()
        .map(|w| (w.line, w.message.as_str()))
        .collect();
    assert_eq!(
        warnings,
        vec![
            (
                7,
                "@String{lab} uses 'lab' in its own definition; bibtui uses the earlier definition (BibTeX would treat it as undefined)"
            ),
            (
                45,
                "Undefined: field 'journal' uses undefined @String 'nowhere' (kept as its name)"
            ),
            (
                45,
                "Undefined: field 'note' uses undefined @String 'nowhere' (kept as its name)"
            ),
            (
                50,
                "Forward: field 'journal' uses @String 'later' above its definition on line 54 (BibTeX would treat it as undefined)"
            ),
        ]
    );
}
