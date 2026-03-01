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
