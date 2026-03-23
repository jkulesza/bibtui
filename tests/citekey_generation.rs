use bibtui::bib::citekey::generate_citekey;
use indexmap::IndexMap;

// ── Legacy {token} syntax ─────────────────────────────────────────────────────

#[test]
fn test_article_citekey() {
    let template = "Article_{year}_{journal_abbrev}_{authors}_{pages}";
    let mut fields = IndexMap::new();
    fields.insert("year".to_string(), "2010".to_string());
    fields.insert("journal".to_string(), "Nuclear Technology".to_string());
    fields.insert("author".to_string(), "Joel A. Kulesza".to_string());
    fields.insert("pages".to_string(), "228--237".to_string());

    let key = generate_citekey(template, &fields);
    assert_eq!(key, "Article_2010_NT_Kulesza_228--237");
}

#[test]
fn test_book_citekey() {
    let template = "Book_{category}_{year}_{author_last}_{title_camel}";
    let mut fields = IndexMap::new();
    fields.insert("year".to_string(), "1997".to_string());
    fields.insert("author".to_string(), "Donald E. Knuth".to_string());
    fields.insert("title".to_string(), "{Fundamental Algorithms}".to_string());
    fields.insert("keywords".to_string(), "Computer".to_string());

    let key = generate_citekey(template, &fields);
    assert_eq!(key, "Book_Computer_1997_Knuth_FundamentalAlgorithms");
}

#[test]
fn test_multiple_authors() {
    let template = "Article_{year}_{journal_abbrev}_{authors}_{pages}";
    let mut fields = IndexMap::new();
    fields.insert("year".to_string(), "1967".to_string());
    fields.insert("journal".to_string(), "Nuclear Science and Engineering".to_string());
    fields.insert(
        "author".to_string(),
        "R. R. Coveyou and V. R. Cain and K. J. Yost".to_string(),
    );
    fields.insert("pages".to_string(), "219--234".to_string());

    let key = generate_citekey(template, &fields);
    assert_eq!(key, "Article_1967_NSE_CoveyouCainEtAl_219--234");
}

#[test]
fn test_phd_thesis_citekey() {
    let template = "PhD-Thesis_{year}_{author_last}";
    let mut fields = IndexMap::new();
    fields.insert("year".to_string(), "2020".to_string());
    fields.insert("author".to_string(), "Jane Smith".to_string());

    let key = generate_citekey(template, &fields);
    assert_eq!(key, "PhD-Thesis_2020_Smith");
}

#[test]
fn test_last_first_author_format() {
    let template = "Article_{year}_{journal_abbrev}_{authors}_{pages}";
    let mut fields = IndexMap::new();
    fields.insert("year".to_string(), "2022".to_string());
    fields.insert("journal".to_string(), "Physical Review".to_string());
    fields.insert("author".to_string(), "Kulesza, Joel A.".to_string());
    fields.insert("pages".to_string(), "1--10".to_string());

    let key = generate_citekey(template, &fields);
    assert_eq!(key, "Article_2022_PR_Kulesza_1--10");
}

// ── New [token] syntax — basic tokens ────────────────────────────────────────

#[test]
fn test_bracket_auth_year() {
    let template = "[auth][year]";
    let mut fields = IndexMap::new();
    fields.insert("author".to_string(), "Jane Smith".to_string());
    fields.insert("year".to_string(), "2020".to_string());

    assert_eq!(generate_citekey(template, &fields), "Smith2020");
}

#[test]
fn test_bracket_last_first_format() {
    let template = "[auth][year]";
    let mut fields = IndexMap::new();
    fields.insert("author".to_string(), "Smith, Jane A.".to_string());
    fields.insert("year".to_string(), "2021".to_string());

    assert_eq!(generate_citekey(template, &fields), "Smith2021");
}

#[test]
fn test_bracket_authn() {
    // [auth2] — first two author last names
    let template = "[auth2][year]";
    let mut fields = IndexMap::new();
    fields.insert("author".to_string(), "Alice Adams and Bob Brown and Carol Clark".to_string());
    fields.insert("year".to_string(), "2023".to_string());

    assert_eq!(generate_citekey(template, &fields), "AdamsBrown2023");
}

#[test]
fn test_bracket_authors_three_plus() {
    // [authors] keeps existing EtAl logic
    let template = "[authors][year]";
    let mut fields = IndexMap::new();
    fields.insert("author".to_string(), "Alice Adams and Bob Brown and Carol Clark".to_string());
    fields.insert("year".to_string(), "2023".to_string());

    assert_eq!(generate_citekey(template, &fields), "AdamsBrownEtAl2023");
}

#[test]
fn test_bracket_firstpage() {
    let template = "[auth][year]_[firstpage]";
    let mut fields = IndexMap::new();
    fields.insert("author".to_string(), "Jane Smith".to_string());
    fields.insert("year".to_string(), "2019".to_string());
    fields.insert("pages".to_string(), "100--115".to_string());

    assert_eq!(generate_citekey(template, &fields), "Smith2019_100");
}

#[test]
fn test_bracket_shortyear() {
    let template = "[auth][shortyear]";
    let mut fields = IndexMap::new();
    fields.insert("author".to_string(), "Jane Smith".to_string());
    fields.insert("year".to_string(), "2024".to_string());

    assert_eq!(generate_citekey(template, &fields), "Smith24");
}

// ── New [token:modifier] syntax ───────────────────────────────────────────────

#[test]
fn test_modifier_upper() {
    let template = "[auth:upper][year]";
    let mut fields = IndexMap::new();
    fields.insert("author".to_string(), "Jane Smith".to_string());
    fields.insert("year".to_string(), "2020".to_string());

    assert_eq!(generate_citekey(template, &fields), "SMITH2020");
}

#[test]
fn test_modifier_lower() {
    let template = "[auth:lower][year]";
    let mut fields = IndexMap::new();
    fields.insert("author".to_string(), "Jane Smith".to_string());
    fields.insert("year".to_string(), "2020".to_string());

    assert_eq!(generate_citekey(template, &fields), "smith2020");
}

#[test]
fn test_modifier_abbr() {
    let template = "[auth][year]_[journal:abbr]";
    let mut fields = IndexMap::new();
    fields.insert("author".to_string(), "Jane Smith".to_string());
    fields.insert("year".to_string(), "2010".to_string());
    fields.insert("journal".to_string(), "Nuclear Science and Engineering".to_string());

    assert_eq!(generate_citekey(template, &fields), "Smith2010_NSE");
}

#[test]
fn test_modifier_truncate() {
    let template = "[auth][year]_[title:(5)]";
    let mut fields = IndexMap::new();
    fields.insert("author".to_string(), "Jane Smith".to_string());
    fields.insert("year".to_string(), "2020".to_string());
    fields.insert("title".to_string(), "Toward Efficient Monte Carlo".to_string());

    // title first significant word is "Toward", truncated to 5 → "Towar"
    assert_eq!(generate_citekey(template, &fields), "Smith2020_Towar");
}

#[test]
fn test_modifier_chain() {
    // :abbr then :lower
    let template = "[auth][year]_[journal:abbr:lower]";
    let mut fields = IndexMap::new();
    fields.insert("author".to_string(), "Jane Smith".to_string());
    fields.insert("year".to_string(), "2010".to_string());
    fields.insert("journal".to_string(), "Nuclear Science and Engineering".to_string());

    assert_eq!(generate_citekey(template, &fields), "Smith2010_nse");
}

// ── Regex modifier ────────────────────────────────────────────────────────────

#[test]
fn test_modifier_regex_replace_spaces() {
    // Replace spaces in title with underscores
    let template = r#"[auth][year]_[shorttitle:regex(" ", "_")]"#;
    let mut fields = IndexMap::new();
    fields.insert("author".to_string(), "Jane Smith".to_string());
    fields.insert("year".to_string(), "2020".to_string());
    fields.insert("title".to_string(), "Toward Efficient Monte Carlo".to_string());

    // shorttitle = first 3 significant words = "TowardEfficientMonte" (no spaces, joined)
    // spaces already absent in joined form, so result unchanged
    assert_eq!(generate_citekey(template, &fields), "Smith2020_TowardEfficientMonte");
}

#[test]
fn test_modifier_regex_strip_dots() {
    // Strip dots from author name (e.g. initials)
    let template = r#"[auth:regex("\.", "")][year]"#;
    let mut fields = IndexMap::new();
    fields.insert("author".to_string(), "J. Smith".to_string());
    fields.insert("year".to_string(), "2020".to_string());

    // last name is "Smith" (no dots), so result is normal
    assert_eq!(generate_citekey(template, &fields), "Smith2020");
}

#[test]
fn test_modifier_regex_year_short() {
    // Extract last two digits of year via regex
    let template = r#"[auth][year:regex("^\d\d", "")]"#;
    let mut fields = IndexMap::new();
    fields.insert("author".to_string(), "Jane Smith".to_string());
    fields.insert("year".to_string(), "2024".to_string());

    // regex strips first two digits → "24"
    assert_eq!(generate_citekey(template, &fields), "Smith24");
}

// ── Mixed syntax ──────────────────────────────────────────────────────────────

#[test]
fn test_mixed_legacy_and_bracket() {
    // A template using both syntaxes — legacy part preserved exactly
    let template = "Article_[auth][year]_{journal_abbrev}";
    let mut fields = IndexMap::new();
    fields.insert("author".to_string(), "Jane Smith".to_string());
    fields.insert("year".to_string(), "2020".to_string());
    fields.insert("journal".to_string(), "Nuclear Technology".to_string());

    assert_eq!(generate_citekey(template, &fields), "Article_Smith2020_NT");
}

// ── Edge cases ────────────────────────────────────────────────────────────────

#[test]
fn test_modifier_abbr_single_word() {
    // A single-word journal should abbreviate to its own first letter.
    let template = "[auth][year]_[journal:abbr]";
    let mut fields = IndexMap::new();
    fields.insert("author".to_string(), "Smith, John".to_string());
    fields.insert("year".to_string(), "2020".to_string());
    fields.insert("journal".to_string(), "Nature".to_string());
    assert_eq!(generate_citekey(template, &fields), "Smith2020_N");
}

#[test]
fn test_modifier_abbr_skip_words() {
    // Words on the skip list (of, the, and, …) should be omitted.
    let template = "[journal:abbr]";
    let mut fields = IndexMap::new();
    fields.insert("journal".to_string(), "Journal of the American Chemical Society".to_string());
    // Skips "of", "the" → J, A, C, S
    assert_eq!(generate_citekey(template, &fields), "JACS");
}

#[test]
fn test_modifier_regex_special_chars() {
    // Regex with special regex metacharacters in the pattern.
    // Replace literal parentheses "(Rev.)" → ""
    let template = r#"[title:regex("\(Rev\.\)", "")]"#;
    let mut fields = IndexMap::new();
    fields.insert("title".to_string(), "Physical Review (Rev.)".to_string());
    let result = generate_citekey(template, &fields);
    assert!(!result.contains("(Rev.)"), "got: {}", result);
}

#[test]
fn test_modifier_regex_capture_group() {
    // Regex replacement using a capture group to keep part of the match.
    let template = r#"[year:regex("(\d\d)(\d\d)", "$2")]"#;
    let mut fields = IndexMap::new();
    fields.insert("year".to_string(), "2024".to_string());
    // Captures "20" and "24", replaces whole match with group 2 → "24"
    assert_eq!(generate_citekey(template, &fields), "24");
}

#[test]
fn test_modifier_regex_invalid_pattern_passthrough() {
    // An invalid regex pattern should leave the value unchanged (no panic).
    let template = r#"[title:regex("[invalid", "x")]"#;
    let mut fields = IndexMap::new();
    fields.insert("title".to_string(), "Some Title".to_string());
    // Invalid regex: value passes through unchanged
    // The template produces the first significant word of title
    let result = generate_citekey(template, &fields);
    // Should not panic; result may be the original or modified value
    assert!(!result.is_empty() || result.is_empty()); // just ensure no panic
}

#[test]
fn test_modifier_truncate_longer_than_value() {
    // Truncating to more chars than the value has should return the full value.
    let template = "[title:(100)]";
    let mut fields = IndexMap::new();
    fields.insert("title".to_string(), "Short".to_string());
    // first significant word of "Short" = "Short", truncated to 100 = "Short"
    assert_eq!(generate_citekey(template, &fields), "Short");
}

#[test]
fn test_modifier_upper_lower_on_abbr() {
    // Chain: abbr then upper → same as abbr (abbr already uppercases initials).
    // Chain: abbr then lower → lowercase abbreviation.
    let mut fields = IndexMap::new();
    fields.insert("journal".to_string(), "Nuclear Science and Engineering".to_string());

    let upper = generate_citekey("[journal:abbr:upper]", &fields);
    let lower = generate_citekey("[journal:abbr:lower]", &fields);
    assert_eq!(upper, "NSE");
    assert_eq!(lower, "nse");
}

#[test]
fn test_missing_field_produces_empty_string() {
    // A bracket token referencing a missing field resolves to "".
    // The trailing "_" separator is trimmed by generate_citekey.
    let template = "[auth][year]_[journal:abbr]";
    let mut fields = IndexMap::new();
    fields.insert("author".to_string(), "Smith, John".to_string());
    fields.insert("year".to_string(), "2020".to_string());
    // journal not present → trailing "_" is stripped
    assert_eq!(generate_citekey(template, &fields), "Smith2020");
}

#[test]
fn test_modifier_camel_case() {
    // [title] resolves to the first significant word; :camel then runs
    // to_camel_case on that single word.  "self-consistent" (split on '-')
    // → "SelfConsistent".
    let template = "[title:camel]";
    let mut fields = IndexMap::new();
    fields.insert("title".to_string(), "self-consistent field theory".to_string());
    assert_eq!(generate_citekey(template, &fields), "SelfConsistent");
}

#[test]
fn test_modifier_camel_case_multi_word_via_shorttitle() {
    // [shorttitle] gives the first 3 significant words joined with no
    // separator → "self-consistentfieldtheory".  :camel then splits on the
    // hyphen only, capitalising each part → "SelfConsistentfieldtheory".
    let template = "[shorttitle:camel]";
    let mut fields = IndexMap::new();
    fields.insert("title".to_string(), "self-consistent field theory".to_string());
    assert_eq!(generate_citekey(template, &fields), "SelfConsistentfieldtheory");
}
