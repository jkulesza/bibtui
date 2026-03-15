/// Integration tests for ISO 4 journal abbreviation.
///
/// These tests cover:
/// - `abbreviate_journal()` utility: known journals, LTWA fallback, overrides
/// - Citekey token `[journal_abbrev]` (new syntax) and `{journal_abbrev}` (legacy)
///   both stable whether or not the `journal_abbrev` field is already stored
use bibtui::bib::citekey::generate_citekey;
use bibtui::util::journal::abbreviate_journal;
use indexmap::IndexMap;

// ── abbreviate_journal unit-level integration ─────────────────────────────────

#[test]
fn known_journal_physical_review_letters() {
    let result = abbreviate_journal("Physical Review Letters", &IndexMap::new());
    assert_eq!(result, "Phys. Rev. Lett.");
}

#[test]
fn known_journal_nuclear_science_and_engineering() {
    let result = abbreviate_journal("Nuclear Science and Engineering", &IndexMap::new());
    assert_eq!(result, "Nucl. Sci. Eng.");
}

#[test]
fn known_journal_annals_of_nuclear_energy() {
    let result = abbreviate_journal("Annals of Nuclear Energy", &IndexMap::new());
    assert_eq!(result, "Ann. Nucl. Energy");
}

#[test]
fn known_journal_journal_of_computational_physics() {
    let result = abbreviate_journal("Journal of Computational Physics", &IndexMap::new());
    assert_eq!(result, "J. Comput. Phys.");
}

#[test]
fn known_journal_case_insensitive() {
    let result = abbreviate_journal("NUCLEAR SCIENCE AND ENGINEERING", &IndexMap::new());
    assert_eq!(result, "Nucl. Sci. Eng.");
}

#[test]
fn known_journal_brace_stripped() {
    let result = abbreviate_journal("{Nuclear Science and Engineering}", &IndexMap::new());
    assert_eq!(result, "Nucl. Sci. Eng.");
}

#[test]
fn unknown_journal_ltwa_fallback() {
    // Not in the built-in table — LTWA word-level abbreviation kicks in
    let result = abbreviate_journal("Journal of Applied Widgets", &IndexMap::new());
    // "Journal" → "J.", "of" → dropped, "Applied" → "Appl.", "Widgets" → kept
    assert_eq!(result, "J. Appl. Widgets");
}

#[test]
fn ltwa_stop_word_only_returns_original() {
    // All words are stop words → return original stripped name unchanged
    let result = abbreviate_journal("In and of the", &IndexMap::new());
    assert_eq!(result, "In and of the");
}

#[test]
fn empty_input_returns_empty() {
    let result = abbreviate_journal("", &IndexMap::new());
    assert_eq!(result, "");
}

#[test]
fn user_override_takes_precedence_over_builtin_table() {
    let mut overrides = IndexMap::new();
    overrides.insert("Nuclear Science and Engineering".to_string(), "NSE".to_string());
    let result = abbreviate_journal("Nuclear Science and Engineering", &overrides);
    assert_eq!(result, "NSE");
}

#[test]
fn user_override_case_insensitive() {
    let mut overrides = IndexMap::new();
    overrides.insert("nuclear science and engineering".to_string(), "NSE".to_string());
    let result = abbreviate_journal("Nuclear Science and Engineering", &overrides);
    assert_eq!(result, "NSE");
}

#[test]
fn user_override_for_unknown_journal() {
    let mut overrides = IndexMap::new();
    overrides.insert("Exotic Journal of Widgets".to_string(), "EJW".to_string());
    let result = abbreviate_journal("Exotic Journal of Widgets", &overrides);
    assert_eq!(result, "EJW");
}

// ── Citekey token [journal_abbrev] — new bracket syntax ──────────────────────

#[test]
fn bracket_token_journal_abbrev_from_journal_field() {
    // No journal_full: abbreviate from journal
    let mut fields = IndexMap::new();
    fields.insert("author".to_string(), "Smith, Jane".to_string());
    fields.insert("year".to_string(), "2020".to_string());
    fields.insert("journal".to_string(), "Nuclear Science and Engineering".to_string());

    let key = generate_citekey("[auth][year]_[journal_abbrev]", &fields);
    assert_eq!(key, "Smith2020_NSE");
}

#[test]
fn bracket_token_journal_abbrev_stable_when_journal_holds_abbreviated_form() {
    // When journal holds the ISO 4 abbreviation (journal_field_content = "abbreviated")
    // but journal_full records the canonical full name, the citekey must be the same
    // as when journal holds the full name.
    let mut fields_full = IndexMap::new();
    fields_full.insert("author".to_string(), "Smith, Jane".to_string());
    fields_full.insert("year".to_string(), "2020".to_string());
    fields_full.insert("journal".to_string(), "Nuclear Science and Engineering".to_string());

    let mut fields_abbrev = IndexMap::new();
    fields_abbrev.insert("author".to_string(), "Smith, Jane".to_string());
    fields_abbrev.insert("year".to_string(), "2020".to_string());
    // journal holds the ISO 4 form; journal_full records the original full name
    fields_abbrev.insert("journal".to_string(), "Nucl. Sci. Eng.".to_string());
    fields_abbrev.insert("journal_full".to_string(), "Nuclear Science and Engineering".to_string());

    let key_full   = generate_citekey("[auth][year]_[journal_abbrev]", &fields_full);
    let key_abbrev = generate_citekey("[auth][year]_[journal_abbrev]", &fields_abbrev);
    assert_eq!(key_full, key_abbrev, "citekey must be stable regardless of journal_field_content");
    assert_eq!(key_full, "Smith2020_NSE");
}

// ── Citekey token {journal_abbrev} — legacy syntax ───────────────────────────

#[test]
fn legacy_token_journal_abbrev_from_journal_field() {
    let mut fields = IndexMap::new();
    fields.insert("year".to_string(), "2010".to_string());
    fields.insert("journal".to_string(), "Nuclear Technology".to_string());
    fields.insert("author".to_string(), "Joel A. Kulesza".to_string());
    fields.insert("pages".to_string(), "228--237".to_string());

    let key = generate_citekey("Article_{year}_{journal_abbrev}_{authors}_{pages}", &fields);
    assert_eq!(key, "Article_2010_NT_Kulesza_228--237");
}

#[test]
fn legacy_token_journal_abbrev_uses_journal_full_when_present() {
    // journal_full is the source of truth; journal may hold ISO 4 form
    let mut fields = IndexMap::new();
    fields.insert("year".to_string(), "1967".to_string());
    fields.insert("journal".to_string(), "Nucl. Sci. Eng.".to_string());
    fields.insert("journal_full".to_string(), "Nuclear Science and Engineering".to_string());
    fields.insert(
        "author".to_string(),
        "R. R. Coveyou and V. R. Cain and K. J. Yost".to_string(),
    );
    fields.insert("pages".to_string(), "219--234".to_string());

    let key = generate_citekey("Article_{year}_{journal_abbrev}_{authors}_{pages}", &fields);
    assert_eq!(key, "Article_1967_NSE_CoveyouCainEtAl_219--234");
}

#[test]
fn legacy_token_journal_abbrev_produces_short_acronym() {
    // Verify the token produces the short first-letter acronym style
    let mut fields = IndexMap::new();
    fields.insert("journal".to_string(), "Nuclear Science and Engineering".to_string());

    let key = generate_citekey("{journal_abbrev}", &fields);
    assert_eq!(key, "NSE");
}
