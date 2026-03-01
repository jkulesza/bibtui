use std::path::PathBuf;

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
