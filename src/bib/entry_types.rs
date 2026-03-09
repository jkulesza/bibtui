use super::model::EntryType;

/// Returns (required_fields, optional_fields) for a given entry type.
pub fn fields_for_type(entry_type: &EntryType) -> (Vec<&'static str>, Vec<&'static str>) {
    match entry_type {
        EntryType::Article => (
            vec!["author", "journal", "title", "year"],
            vec!["doi", "month", "note", "number", "pages", "url", "volume"],
        ),
        EntryType::Book => (
            vec!["author", "publisher", "title", "year"],
            vec![
                "address", "doi", "edition", "isbn", "keywords", "month", "note", "series", "url",
                "volume",
            ],
        ),
        EntryType::Booklet => (
            vec!["title"],
            vec!["author", "address", "howpublished", "month", "note", "year"],
        ),
        EntryType::InBook => (
            vec!["author", "chapter", "pages", "publisher", "title", "year"],
            vec![
                "address", "edition", "editor", "month", "note", "series", "volume",
            ],
        ),
        EntryType::InCollection => (
            vec!["author", "booktitle", "publisher", "title", "year"],
            vec![
                "address", "chapter", "edition", "editor", "month", "note", "pages", "series",
                "volume",
            ],
        ),
        EntryType::InProceedings => (
            vec!["author", "booktitle", "title", "year"],
            vec![
                "address", "doi", "editor", "month", "note", "number", "pages", "publisher",
                "series", "url", "volume",
            ],
        ),
        EntryType::Manual => (
            vec!["title"],
            vec![
                "address",
                "author",
                "edition",
                "month",
                "note",
                "organization",
                "year",
            ],
        ),
        EntryType::MastersThesis => (
            vec!["author", "school", "title", "year"],
            vec!["address", "month", "note"],
        ),
        EntryType::Misc => (
            vec![],
            vec!["author", "howpublished", "month", "note", "title", "year"],
        ),
        EntryType::PhdThesis => (
            vec!["author", "school", "title", "year"],
            vec!["address", "month", "note"],
        ),
        EntryType::Proceedings => (
            vec!["title", "year"],
            vec![
                "address", "editor", "month", "note", "number", "organization", "publisher",
                "series", "volume",
            ],
        ),
        EntryType::TechReport => (
            vec!["author", "institution", "title", "year"],
            vec!["address", "month", "note", "number", "url"],
        ),
        EntryType::Unpublished => (
            vec!["author", "note", "title"],
            vec!["month", "year"],
        ),
        EntryType::Other(_) => (vec![], vec![]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_article_required_fields() {
        let (req, _) = fields_for_type(&EntryType::Article);
        assert!(req.contains(&"author"));
        assert!(req.contains(&"journal"));
        assert!(req.contains(&"title"));
        assert!(req.contains(&"year"));
    }

    #[test]
    fn test_book_required_fields() {
        let (req, _) = fields_for_type(&EntryType::Book);
        assert!(req.contains(&"author"));
        assert!(req.contains(&"publisher"));
        assert!(req.contains(&"title"));
        assert!(req.contains(&"year"));
    }

    #[test]
    fn test_misc_has_no_required_fields() {
        let (req, _) = fields_for_type(&EntryType::Misc);
        assert!(req.is_empty());
    }

    #[test]
    fn test_phdthesis_requires_school() {
        let (req, _) = fields_for_type(&EntryType::PhdThesis);
        assert!(req.contains(&"school"));
    }

    #[test]
    fn test_mastersthesis_requires_school() {
        let (req, _) = fields_for_type(&EntryType::MastersThesis);
        assert!(req.contains(&"school"));
    }

    #[test]
    fn test_techreport_requires_institution() {
        let (req, _) = fields_for_type(&EntryType::TechReport);
        assert!(req.contains(&"institution"));
    }

    #[test]
    fn test_inproceedings_requires_booktitle() {
        let (req, _) = fields_for_type(&EntryType::InProceedings);
        assert!(req.contains(&"booktitle"));
    }

    #[test]
    fn test_other_returns_empty() {
        let (req, opt) = fields_for_type(&EntryType::Other("custom".to_string()));
        assert!(req.is_empty());
        assert!(opt.is_empty());
    }

    #[test]
    fn test_unpublished_requires_note() {
        let (req, _) = fields_for_type(&EntryType::Unpublished);
        assert!(req.contains(&"note"));
    }
}
