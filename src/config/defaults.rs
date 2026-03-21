use indexmap::IndexMap;

use super::schema::*;

pub fn default_config() -> Config {
    Config {
        general: GeneralConfig::default(),
        display: DisplayConfig::default(),
        citation: super::schema::CitationConfig::default(),
        citekey: CitekeyConfig::default(),
        entry_types: IndexMap::new(),
        save: SaveConfig::default(),
        theme: ThemeConfig::default(),
        titlecase: TitlecaseConfig::default(),
        field_groups: default_field_groups(),
    }
}

pub fn default_field_groups() -> Vec<CustomFieldGroup> {
    vec![
        CustomFieldGroup {
            name: "Identifiers".to_string(),
            fields: vec![
                "isbn".to_string(),
                "issn".to_string(),
                "lccn".to_string(),
                "eprint".to_string(),
                "archiveprefix".to_string(),
                "primaryclass".to_string(),
                "pmid".to_string(),
                "arxivid".to_string(),
            ],
        },
    ]
}

pub fn default_columns() -> Vec<ColumnConfig> {
    vec![
        ColumnConfig {
            field: "dirty".to_string(),
            header: " ".to_string(),
            width: ColumnWidth::Fixed(2),
            max_width: None,
        },
        ColumnConfig {
            field: "file_indicator".to_string(),
            header: "\u{2398}".to_string(),
            width: ColumnWidth::Fixed(2),
            max_width: None,
        },
        ColumnConfig {
            field: "web_indicator".to_string(),
            header: "\u{238B}".to_string(),
            width: ColumnWidth::Fixed(2),
            max_width: None,
        },
        ColumnConfig {
            field: "entrytype".to_string(),
            header: "Type".to_string(),
            width: ColumnWidth::Fixed(12),
            max_width: None,
        },
        ColumnConfig {
            field: "year".to_string(),
            header: "Year".to_string(),
            width: ColumnWidth::Fixed(6),
            max_width: None,
        },
        ColumnConfig {
            field: "author".to_string(),
            header: "Author".to_string(),
            width: ColumnWidth::Percent(20),
            max_width: Some(20),
        },
        ColumnConfig {
            field: "title".to_string(),
            header: "Title".to_string(),
            width: ColumnWidth::Flex,
            max_width: None,
        },
        ColumnConfig {
            field: "journal".to_string(),
            header: "Journal".to_string(),
            width: ColumnWidth::Percent(10),
            max_width: Some(22),
        },
    ]
}

pub fn default_citekey_templates() -> IndexMap<String, String> {
    let mut m = IndexMap::new();
    m.insert(
        "article".to_string(),
        "Article_[year]_[journal_abbrev]_[authors]_[pages]".to_string(),
    );
    m.insert(
        "book".to_string(),
        "Book_[keywords:camel]_[year]_[auth]_[shorttitle:camel]".to_string(),
    );
    m.insert(
        "techreport".to_string(),
        "TechReport_[year]_[institution:abbr]_[number]_[authors]".to_string(),
    );
    m.insert(
        "inproceedings".to_string(),
        "Proceedings_[year]_[booktitle:abbr]_[authors]_[pages]".to_string(),
    );
    m.insert(
        "phdthesis".to_string(),
        "PhD-Thesis_[year]_[auth]".to_string(),
    );
    m.insert(
        "mastersthesis".to_string(),
        "MS-Thesis_[year]_[auth]".to_string(),
    );
    m.insert(
        "misc".to_string(),
        "Misc_[year]_[howpublished:camel]_[authors]_[title:camel]".to_string(),
    );
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_field_groups_has_identifiers() {
        let groups = default_field_groups();
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].name, "Identifiers");
        assert!(groups[0].fields.contains(&"isbn".to_string()));
        assert!(groups[0].fields.contains(&"eprint".to_string()));
    }

    #[test]
    fn test_default_columns_has_expected_fields() {
        let cols = default_columns();
        let fields: Vec<&str> = cols.iter().map(|c| c.field.as_str()).collect();
        assert!(fields.contains(&"author"));
        assert!(fields.contains(&"title"));
        assert!(fields.contains(&"year"));
        assert!(fields.contains(&"entrytype"));
    }

    #[test]
    fn test_default_citekey_templates_has_article() {
        let templates = default_citekey_templates();
        assert!(templates.contains_key("article"));
        assert!(templates.contains_key("book"));
        assert!(templates.contains_key("techreport"));
        assert!(templates.contains_key("phdthesis"));
    }

    #[test]
    fn test_default_config_is_valid() {
        let config = default_config();
        // Should not panic and have expected defaults
        assert!(!config.field_groups.is_empty());
        assert!(!config.display.columns.is_empty());
    }
}
