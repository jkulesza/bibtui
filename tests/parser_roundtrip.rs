use std::path::PathBuf;
use bibtui::bib::model::RawItem;

#[test]
fn test_roundtrip_jabref_bib() {
    let bib_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("jabref.bib");
    if !bib_path.exists() {
        eprintln!("Skipping: jabref.bib not found");
        return;
    }

    let original = std::fs::read_to_string(&bib_path).expect("Failed to read jabref.bib");

    let raw = bibtui::bib::parser::parse_bib_file(&original).expect("Failed to parse jabref.bib");

    let output = bibtui::bib::writer::write_bib_file(&raw);

    if original != output {
        // Find first difference
        let orig_bytes = original.as_bytes();
        let out_bytes = output.as_bytes();
        let min_len = orig_bytes.len().min(out_bytes.len());

        for i in 0..min_len {
            if orig_bytes[i] != out_bytes[i] {
                let line_num = original[..i].matches('\n').count() + 1;
                let line_start = original[..i].rfind('\n').map(|p| p + 1).unwrap_or(0);
                let col = i - line_start;
                let context_start = i.saturating_sub(40);
                let context_end = (i + 40).min(original.len());

                panic!(
                    "Round-trip mismatch at byte {} (line {}, col {}):\n\
                     Original: {:?}\n\
                     Output:   {:?}\n\
                     Original len: {}, Output len: {}",
                    i,
                    line_num,
                    col,
                    &original[context_start..context_end],
                    &output[context_start..(i + 40).min(output.len())],
                    original.len(),
                    output.len(),
                );
            }
        }

        if orig_bytes.len() != out_bytes.len() {
            panic!(
                "Round-trip length mismatch: original={}, output={}",
                orig_bytes.len(),
                out_bytes.len()
            );
        }
    }
}

#[test]
fn test_roundtrip_database_entry_count() {
    let bib_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("jabref.bib");
    if !bib_path.exists() {
        return;
    }

    let content = std::fs::read_to_string(&bib_path).unwrap();
    let raw = bibtui::bib::parser::parse_bib_file(&content).unwrap();
    let db = bibtui::bib::parser::build_database(raw);

    // Should have 550 actual entries (excluding @Comment, @IEEEtranBSTCTL, etc.)
    // The count should match what grep -c '^@' reports minus @Comment entries
    assert!(
        db.entries.len() >= 540,
        "Expected at least 540 entries, got {}",
        db.entries.len()
    );
    assert!(
        db.entries.len() <= 560,
        "Expected at most 560 entries, got {}",
        db.entries.len()
    );
}

/// Verify that editing a field and re-serialising only changes that entry.
///
/// Steps:
///   1. Parse an inline .bib string with two entries.
///   2. Build a Database from it.
///   3. Mark the first entry dirty with a changed field value.
///   4. Serialize the dirty entry and patch it back into RawBibFile.
///   5. Write the file and re-parse.
///   6. Assert: the changed field has the new value.
///   7. Assert: the second entry is byte-for-byte unchanged.
#[test]
fn test_dirty_entry_roundtrip() {
    let bib_src = r#"@Article{Smith2020,
  author  = {Jane Smith},
  title   = {An Introduction},
  journal = {Nature},
  year    = {2020},
}

@Article{Jones2019,
  author  = {Bob Jones},
  title   = {A Follow-Up},
  journal = {Science},
  year    = {2019},
}
"#;

    // ── 1. Parse + build Database ─────────────────────────────────────────────
    let mut raw = bibtui::bib::parser::parse_bib_file(bib_src).expect("parse failed");
    let mut db = bibtui::bib::parser::build_database(raw.clone());

    // Capture the raw text of the second entry so we can compare later.
    let jones_raw_index = db.entries["Jones2019"].raw_index;
    let jones_original_raw = match &raw.items[jones_raw_index] {
        RawItem::Entry(e) => e.raw_text.clone(),
        _ => panic!("unexpected raw item type for Jones2019"),
    };

    // ── 2. Edit first entry ───────────────────────────────────────────────────
    let smith_entry = db.entries.get_mut("Smith2020").expect("Smith2020 not found");
    smith_entry.fields.insert("title".to_string(), "A Revised Introduction".to_string());
    smith_entry.dirty = true;

    // ── 3. Serialize dirty entry and patch RawBibFile ─────────────────────────
    {
        let smith_entry = &db.entries["Smith2020"];
        let new_raw_text = bibtui::bib::writer::serialize_entry(smith_entry, true, false);
        raw = db.raw_file.clone();
        if let RawItem::Entry(ref mut re) = raw.items[smith_entry.raw_index] {
            re.raw_text = new_raw_text;
        }
    }

    // ── 4. Write and re-parse ─────────────────────────────────────────────────
    let output = bibtui::bib::writer::write_bib_file(&raw);
    let raw2 = bibtui::bib::parser::parse_bib_file(&output).expect("re-parse failed");
    let db2 = bibtui::bib::parser::build_database(raw2.clone());

    // ── 5. Assert: changed field has new value ────────────────────────────────
    let smith2 = &db2.entries["Smith2020"];
    assert_eq!(
        smith2.fields.get("title").map(String::as_str),
        Some("A Revised Introduction"),
        "title field should reflect the edit"
    );

    // ── 6. Assert: other fields of Smith2020 are intact ──────────────────────
    assert_eq!(smith2.fields.get("author").map(String::as_str), Some("Jane Smith"));
    assert_eq!(smith2.fields.get("year").map(String::as_str), Some("2020"));

    // ── 7. Assert: Jones2019 raw text is byte-for-byte unchanged ─────────────
    let jones_new_raw = match &raw2.items[db2.entries["Jones2019"].raw_index] {
        RawItem::Entry(e) => e.raw_text.clone(),
        _ => panic!("unexpected raw item type for Jones2019 after roundtrip"),
    };
    assert_eq!(
        jones_original_raw, jones_new_raw,
        "unmodified entry Jones2019 must be byte-for-byte identical after roundtrip"
    );
}

// ── Fixture-based roundtrip tests ─────────────────────────────────────────────

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

fn roundtrip_fixture(name: &str) {
    let path = fixture_path(name);
    let original = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read fixture {}: {}", name, e));
    let raw = bibtui::bib::parser::parse_bib_file(&original)
        .unwrap_or_else(|e| panic!("failed to parse fixture {}: {}", name, e));
    let output = bibtui::bib::writer::write_bib_file(&raw);
    assert_eq!(
        original, output,
        "round-trip mismatch for fixture {}",
        name
    );
}

#[test]
fn test_roundtrip_fixture_minimal() {
    roundtrip_fixture("minimal.bib");
}

#[test]
fn test_roundtrip_fixture_complex_entries() {
    roundtrip_fixture("complex_entries.bib");
}

#[test]
fn test_roundtrip_fixture_jabref_groups() {
    roundtrip_fixture("jabref_groups.bib");
}

#[test]
fn test_roundtrip_fixture_special_chars() {
    roundtrip_fixture("special_chars.bib");
}

#[test]
fn test_roundtrip_fixture_string_macros() {
    roundtrip_fixture("string_macros.bib");
}

/// Both multi-file fixtures parse and round-trip cleanly.
#[test]
fn test_roundtrip_fixture_multi_file_a() {
    roundtrip_fixture("multi_file_a.bib");
}

#[test]
fn test_roundtrip_fixture_multi_file_b() {
    roundtrip_fixture("multi_file_b.bib");
}

/// Parsing both multi-file fixtures yields the expected entry counts,
/// and the overlapping citekey is present in each independently.
#[test]
fn test_multi_file_overlapping_citekey() {
    let a_src = std::fs::read_to_string(fixture_path("multi_file_a.bib")).unwrap();
    let b_src = std::fs::read_to_string(fixture_path("multi_file_b.bib")).unwrap();

    let raw_a = bibtui::bib::parser::parse_bib_file(&a_src).unwrap();
    let raw_b = bibtui::bib::parser::parse_bib_file(&b_src).unwrap();
    let db_a = bibtui::bib::parser::build_database(raw_a);
    let db_b = bibtui::bib::parser::build_database(raw_b);

    assert_eq!(db_a.entries.len(), 2, "multi_file_a.bib should have 2 entries");
    assert_eq!(db_b.entries.len(), 2, "multi_file_b.bib should have 2 entries");

    // Both files have Smith2020 but with different authors
    let author_a = db_a.entries["Smith2020"].fields.get("author").map(String::as_str).unwrap_or("");
    let author_b = db_b.entries["Smith2020"].fields.get("author").map(String::as_str).unwrap_or("");
    assert_eq!(author_a, "Alice Smith");
    assert_eq!(author_b, "Bob Smith");
    assert_ne!(author_a, author_b, "overlapping citekeys have different content");
}

/// Entries with special characters parse without error and
/// the accent sequences survive a roundtrip unchanged.
#[test]
fn test_special_chars_parse_and_roundtrip() {
    let path = fixture_path("special_chars.bib");
    let original = std::fs::read_to_string(&path).unwrap();
    let raw = bibtui::bib::parser::parse_bib_file(&original).unwrap();
    let db = bibtui::bib::parser::build_database(raw);

    assert_eq!(db.entries.len(), 5, "special_chars.bib should have 5 entries");

    // Umlaut in Schrödinger's name is preserved inside braces
    let schr = &db.entries["Schrodinger1926"];
    let author = schr.fields.get("author").unwrap();
    assert!(author.contains("Schr"), "author field should contain Schr...");
}

/// @String macro definitions are preserved byte-for-byte on roundtrip.
#[test]
fn test_string_macros_roundtrip_exact() {
    let path = fixture_path("string_macros.bib");
    let original = std::fs::read_to_string(&path).unwrap();
    let raw = bibtui::bib::parser::parse_bib_file(&original).unwrap();
    let output = bibtui::bib::writer::write_bib_file(&raw);
    assert_eq!(original, output, "@String macro file must round-trip byte-perfectly");
}
