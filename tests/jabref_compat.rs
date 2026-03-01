use bibtui::bib::parser::{build_database, parse_bib_file};

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
