use indexmap::IndexMap;

/// Generate a citation key from a template string and entry fields.
pub fn generate_citekey(template: &str, fields: &IndexMap<String, String>) -> String {
    let mut result = template.to_string();

    // Replace template placeholders
    result = result.replace("{year}", fields.get("year").map(|s| s.as_str()).unwrap_or(""));

    if let Some(author) = fields.get("author") {
        let authors = parse_authors(author);
        result = result.replace("{author_last}", &authors.first().cloned().unwrap_or_default());
        result = result.replace("{authors}", &format_authors_for_key(&authors));
    } else {
        result = result.replace("{author_last}", "");
        result = result.replace("{authors}", "");
    }

    if let Some(title) = fields.get("title") {
        let clean = clean_braces(title);
        result = result.replace("{title_camel}", &to_camel_case(&clean));
    } else {
        result = result.replace("{title_camel}", "");
    }

    if let Some(journal) = fields.get("journal") {
        result = result.replace("{journal_abbrev}", &abbreviate(journal));
    } else {
        result = result.replace("{journal_abbrev}", "");
    }

    if let Some(booktitle) = fields.get("booktitle") {
        result = result.replace("{booktitle_abbrev}", &abbreviate(booktitle));
    } else {
        result = result.replace("{booktitle_abbrev}", "");
    }

    if let Some(pages) = fields.get("pages") {
        result = result.replace("{pages}", pages);
    } else {
        result = result.replace("{pages}", "");
    }

    if let Some(number) = fields.get("number") {
        result = result.replace("{number}", number);
    } else {
        result = result.replace("{number}", "");
    }

    if let Some(institution) = fields.get("institution") {
        result = result.replace("{institution_abbrev}", &abbreviate(institution));
    } else {
        result = result.replace("{institution_abbrev}", "");
    }

    if let Some(howpublished) = fields.get("howpublished") {
        result = result.replace("{howpublished_camel}", &to_camel_case(howpublished));
    } else {
        result = result.replace("{howpublished_camel}", "");
    }

    if let Some(category) = fields.get("keywords") {
        result = result.replace("{category}", &to_camel_case(category));
    } else {
        result = result.replace("{category}", "");
    }

    result
}

/// Parse "First Last and First2 Last2" into vec of last names.
fn parse_authors(author: &str) -> Vec<String> {
    author
        .split(" and ")
        .map(|a| {
            let a = a.trim();
            if a.contains(',') {
                // "Last, First" format
                a.split(',').next().unwrap_or("").trim().to_string()
            } else {
                // "First Last" format — last word is last name
                a.split_whitespace()
                    .last()
                    .unwrap_or("")
                    .to_string()
            }
        })
        .collect()
}

/// Format authors for citation key: "LastA" or "LastALastB" or "LastALastBEtAl"
fn format_authors_for_key(authors: &[String]) -> String {
    match authors.len() {
        0 => String::new(),
        1 => authors[0].clone(),
        2 => format!("{}{}", authors[0], authors[1]),
        _ => format!("{}{}EtAl", authors[0], authors[1]),
    }
}

/// Abbreviate a journal/institution name by taking first letter of each significant word.
fn abbreviate(name: &str) -> String {
    let skip_words = ["of", "the", "and", "for", "in", "on", "a", "an", "&"];
    name.split_whitespace()
        .filter(|w| !skip_words.contains(&w.to_lowercase().as_str()))
        .map(|w| {
            w.chars()
                .next()
                .map(|c| c.to_uppercase().to_string())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join("")
}

/// Remove braces and convert to CamelCase
fn to_camel_case(s: &str) -> String {
    let clean = clean_braces(s);
    clean
        .split_whitespace()
        .map(|w| {
            let mut chars = w.chars();
            match chars.next() {
                Some(c) => {
                    let mut s = c.to_uppercase().to_string();
                    s.extend(chars);
                    s
                }
                None => String::new(),
            }
        })
        .collect()
}

/// Remove outer braces from a value
fn clean_braces(s: &str) -> String {
    let s = s.trim();
    if s.starts_with('{') && s.ends_with('}') {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}
