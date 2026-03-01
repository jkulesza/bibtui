use std::path::PathBuf;

use bibtui::bib::parser::{build_database, parse_bib_file};
use bibtui::util::latex::render_latex;
use bibtui::util::titlecase::strip_case_braces;



#[test]
fn test_jabref_groups_parsed() {
    let input = std::fs::read_to_string("tests/fixtures/jabref_groups.bib").unwrap();
    let raw = parse_bib_file(&input).unwrap();
    let db = build_database(raw);

    // Root should be AllEntries
    assert_eq!(db.groups.root.group.name, "All Entries");

    // Should have children
    assert!(!db.groups.root.children.is_empty());
}

#[test]
fn test_jabref_groups_roundtrip() {
    let input = std::fs::read_to_string("tests/fixtures/jabref_groups.bib").unwrap();
    let raw = parse_bib_file(&input).unwrap();
    let output = bibtui::bib::writer::write_bib_file(&raw);
    assert_eq!(input, output);
}

#[test]
fn test_jabref_group_memberships() {
    let input = std::fs::read_to_string("tests/fixtures/jabref_groups.bib").unwrap();
    let raw = parse_bib_file(&input).unwrap();
    let db = build_database(raw);

    // The Article entry has groups = {Mine: Refereed Journal Article}
    let article = db
        .entries
        .get("Article_2010_NT_Kulesza_228--237")
        .unwrap();
    assert!(article
        .group_memberships
        .contains(&"Mine: Refereed Journal Article".to_string()));
}

#[test]
fn test_jabref_database_type() {
    let input = std::fs::read_to_string("tests/fixtures/jabref_groups.bib").unwrap();
    let raw = parse_bib_file(&input).unwrap();
    let db = build_database(raw);
    assert_eq!(
        db.jabref_meta.database_type.as_deref(),
        Some("bibtex")
    );
}

/// Brantley & Larsen 2000: title = {{The Simplified $P_3$ Approximation}}
/// The double-braced field stores the inner brace-pair as the field value.
/// With LaTeX rendering enabled, $P_3$ must display as P₃.
/// Uses the embedded fixture so this test does not depend on jabref.bib.
#[test]
fn test_brantley_p3_latex_display() {
    let bib_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/complex_entries.bib");
    let input = std::fs::read_to_string(&bib_path).unwrap();
    let raw = parse_bib_file(&input).unwrap();
    let db = build_database(raw);

    let entry = db
        .entries
        .get("Article_2000_NSaE_BrantleyLarsen_1--21")
        .expect("Brantley entry not found in fixture");

    let raw_title = entry.fields.get("title").expect("title field missing");

    // The parser strips the outer field-delimiter braces; the inner
    // case-protecting brace pair becomes the stored value.
    assert_eq!(raw_title, "{The Simplified $P_3$ Approximation}");

    // Full display pipeline (render_latex default is now true):
    // render LaTeX first, then strip case-protecting braces.
    let displayed = strip_case_braces(&render_latex(raw_title));
    assert_eq!(displayed, "The Simplified P₃ Approximation");
}

#[test]
fn test_keyword_group_matching() {
    let input = std::fs::read_to_string("tests/fixtures/jabref_groups.bib").unwrap();
    let raw = parse_bib_file(&input).unwrap();
    let db = build_database(raw);

    // The Book entry has keywords = {Nuclear}
    let book = db
        .entries
        .get("Book_Nuclear_2005_Lewis_FundamentalsofNuclearReactorPhysics")
        .unwrap();
    assert_eq!(
        book.fields.get("keywords").map(|s| s.as_str()),
        Some("Nuclear")
    );
}
