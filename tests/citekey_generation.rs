use bibtui::bib::citekey::generate_citekey;
use indexmap::IndexMap;

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
    fields.insert(
        "title".to_string(),
        "{Fundamental Algorithms}".to_string(),
    );
    fields.insert("keywords".to_string(), "Computer".to_string());

    let key = generate_citekey(template, &fields);
    assert_eq!(key, "Book_Computer_1997_Knuth_FundamentalAlgorithms");
}

#[test]
fn test_multiple_authors() {
    let template = "Article_{year}_{journal_abbrev}_{authors}_{pages}";
    let mut fields = IndexMap::new();
    fields.insert("year".to_string(), "1967".to_string());
    fields.insert(
        "journal".to_string(),
        "Nuclear Science and Engineering".to_string(),
    );
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
    fields.insert(
        "journal".to_string(),
        "Physical Review".to_string(),
    );
    fields.insert(
        "author".to_string(),
        "Kulesza, Joel A.".to_string(),
    );
    fields.insert("pages".to_string(), "1--10".to_string());

    let key = generate_citekey(template, &fields);
    assert_eq!(key, "Article_2022_PR_Kulesza_1--10");
}
