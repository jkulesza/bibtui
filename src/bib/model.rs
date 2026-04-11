#![allow(dead_code)]

use indexmap::IndexMap;
use std::fmt;

// ── Raw layer: preserves every byte of the original file ──

#[derive(Debug, Clone)]
pub struct RawBibFile {
    pub items: Vec<RawItem>,
}

#[derive(Debug, Clone)]
pub enum RawItem {
    /// Text between entries (whitespace, bare `%` comments, semicolons, etc.)
    Preamble(String),
    /// @Preamble{...}
    BibPreamble(String),
    /// @String{name = value}
    StringDef { name: String, raw_value: String },
    /// @Comment{...} — includes JabRef metadata blocks
    Comment { raw_text: String },
    /// A regular @Type{key, fields...}
    Entry(RawEntry),
}

#[derive(Debug, Clone)]
pub struct RawEntry {
    /// Preserved case, e.g. "Article", "TechReport"
    pub entry_type: String,
    pub citation_key: String,
    pub fields: Vec<RawField>,
    /// Column at which '=' signs are aligned (0 = no alignment)
    pub align_width: usize,
    /// Whether there's a trailing comma after the last field
    pub trailing_comma: bool,
    /// The complete raw text of this entry, used for passthrough writing
    pub raw_text: String,
}

#[derive(Debug, Clone)]
pub struct RawField {
    pub name: String,
    pub value: RawFieldValue,
    /// Leading whitespace before field name
    pub indent: String,
    /// Whitespace between field name and '='
    pub pre_eq: String,
    /// Whitespace between '=' and value
    pub post_eq: String,
    /// Trailing content after value (comma, whitespace, comment)
    pub trailing: String,
}

#[derive(Debug, Clone)]
pub enum RawFieldValue {
    Braced(String),
    Quoted(String),
    Bare(String),
    Concat(Vec<RawFieldValue>),
}

impl RawFieldValue {
    /// Extract the semantic string value
    pub fn to_string_value(&self) -> String {
        match self {
            RawFieldValue::Braced(s) => s.clone(),
            RawFieldValue::Quoted(s) => s.clone(),
            RawFieldValue::Bare(s) => s.clone(),
            RawFieldValue::Concat(parts) => {
                parts.iter().map(|p| p.to_string_value()).collect::<Vec<_>>().join(" ")
            }
        }
    }
}

// ── Semantic layer: for display, search, edit ──

#[derive(Debug, Clone)]
pub struct Database {
    pub entries: IndexMap<String, Entry>,
    pub groups: GroupTree,
    pub jabref_meta: JabRefMeta,
    pub raw_file: RawBibFile,
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub entry_type: EntryType,
    pub citation_key: String,
    pub fields: IndexMap<String, String>,
    pub group_memberships: Vec<String>,
    /// Index into RawBibFile.items for this entry's raw data
    pub raw_index: usize,
    /// Whether this entry has been modified since loading
    pub dirty: bool,
}

impl Entry {
    pub fn author_display(&self) -> String {
        self.fields.get("author").cloned().unwrap_or_default()
    }

    pub fn title_display(&self) -> String {
        self.fields.get("title").cloned().unwrap_or_default()
    }

    pub fn year_display(&self) -> String {
        self.fields.get("year").cloned().unwrap_or_default()
    }

    pub fn journal_display(&self) -> String {
        self.fields
            .get("journal")
            .or_else(|| self.fields.get("booktitle"))
            .cloned()
            .unwrap_or_default()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryType {
    Article,
    Book,
    Booklet,
    InBook,
    InCollection,
    InProceedings,
    Manual,
    MastersThesis,
    Misc,
    PhdThesis,
    Proceedings,
    TechReport,
    Unpublished,
    Other(String),
}

impl EntryType {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "article" => EntryType::Article,
            "book" => EntryType::Book,
            "booklet" => EntryType::Booklet,
            "inbook" => EntryType::InBook,
            "incollection" => EntryType::InCollection,
            "inproceedings" | "conference" => EntryType::InProceedings,
            "manual" => EntryType::Manual,
            "mastersthesis" => EntryType::MastersThesis,
            "misc" => EntryType::Misc,
            "phdthesis" => EntryType::PhdThesis,
            "proceedings" => EntryType::Proceedings,
            "techreport" => EntryType::TechReport,
            "unpublished" => EntryType::Unpublished,
            _ => EntryType::Other(s.to_string()),
        }
    }

    pub fn display_name(&self) -> &str {
        match self {
            EntryType::Article => "Article",
            EntryType::Book => "Book",
            EntryType::Booklet => "Booklet",
            EntryType::InBook => "InBook",
            EntryType::InCollection => "InCollection",
            EntryType::InProceedings => "InProceedings",
            EntryType::Manual => "Manual",
            EntryType::MastersThesis => "MastersThesis",
            EntryType::Misc => "Misc",
            EntryType::PhdThesis => "PhdThesis",
            EntryType::Proceedings => "Proceedings",
            EntryType::TechReport => "TechReport",
            EntryType::Unpublished => "Unpublished",
            EntryType::Other(s) => s.as_str(),
        }
    }
}

impl fmt::Display for EntryType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

// ── Groups ──

#[derive(Debug, Clone)]
pub struct GroupTree {
    pub root: GroupNode,
}

impl Default for GroupTree {
    fn default() -> Self {
        GroupTree {
            root: GroupNode {
                group: Group {
                    name: "All Entries".to_string(),
                    group_type: GroupType::AllEntries,
                },
                children: Vec::new(),
                expanded: true,
            },
        }
    }
}

#[derive(Debug, Clone)]
pub struct GroupNode {
    pub group: Group,
    pub children: Vec<GroupNode>,
    pub expanded: bool,
}

#[derive(Debug, Clone)]
pub struct Group {
    pub name: String,
    pub group_type: GroupType,
}

#[derive(Debug, Clone, PartialEq)]
pub enum GroupType {
    AllEntries,
    Static,
    Keyword {
        field: String,
        search_term: String,
        case_sensitive: bool,
        regex: bool,
    },
}

// ── JabRef Metadata ──

#[derive(Debug, Clone, Default)]
pub struct JabRefMeta {
    pub database_type: Option<String>,
    pub file_directory: Option<String>,
    pub save_actions: Option<String>,
    pub save_order_config: Option<String>,
    pub groups_version: Option<String>,
    pub protected_flag: Option<String>,
    /// Default citation key pattern from `jabref-meta: keypatterndefault:...`
    pub key_pattern_default: Option<String>,
    /// Per-entry-type citation key patterns from `jabref-meta: keypattern_<type>:...`
    pub key_patterns: IndexMap<String, String>,
    /// Preserve unknown metadata keys for round-trip
    pub unknown_meta: IndexMap<String, String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(fields: &[(&str, &str)]) -> Entry {
        let mut f = IndexMap::new();
        for (k, v) in fields {
            f.insert(k.to_string(), v.to_string());
        }
        Entry {
            entry_type: EntryType::Article,
            citation_key: "Key2020".to_string(),
            fields: f,
            group_memberships: vec![],
            raw_index: 0,
            dirty: false,
        }
    }

    #[test]
    fn test_entry_type_from_str_all() {
        assert_eq!(EntryType::from_str("article"), EntryType::Article);
        assert_eq!(EntryType::from_str("book"), EntryType::Book);
        assert_eq!(EntryType::from_str("booklet"), EntryType::Booklet);
        assert_eq!(EntryType::from_str("inbook"), EntryType::InBook);
        assert_eq!(EntryType::from_str("incollection"), EntryType::InCollection);
        assert_eq!(EntryType::from_str("inproceedings"), EntryType::InProceedings);
        assert_eq!(EntryType::from_str("conference"), EntryType::InProceedings);
        assert_eq!(EntryType::from_str("manual"), EntryType::Manual);
        assert_eq!(EntryType::from_str("mastersthesis"), EntryType::MastersThesis);
        assert_eq!(EntryType::from_str("misc"), EntryType::Misc);
        assert_eq!(EntryType::from_str("phdthesis"), EntryType::PhdThesis);
        assert_eq!(EntryType::from_str("proceedings"), EntryType::Proceedings);
        assert_eq!(EntryType::from_str("techreport"), EntryType::TechReport);
        assert_eq!(EntryType::from_str("unpublished"), EntryType::Unpublished);
    }

    #[test]
    fn test_entry_type_from_str_case_insensitive() {
        assert_eq!(EntryType::from_str("Article"), EntryType::Article);
        assert_eq!(EntryType::from_str("TECHREPORT"), EntryType::TechReport);
    }

    #[test]
    fn test_entry_type_from_str_unknown() {
        assert_eq!(
            EntryType::from_str("IEEEtranBSTCTL"),
            EntryType::Other("IEEEtranBSTCTL".to_string())
        );
    }

    #[test]
    fn test_entry_type_display_name() {
        assert_eq!(EntryType::Article.display_name(), "Article");
        assert_eq!(EntryType::TechReport.display_name(), "TechReport");
        assert_eq!(EntryType::Other("Foo".to_string()).display_name(), "Foo");
    }

    #[test]
    fn test_entry_type_display_trait() {
        assert_eq!(format!("{}", EntryType::Book), "Book");
    }

    #[test]
    fn test_entry_author_display() {
        let e = make_entry(&[("author", "Smith, John")]);
        assert_eq!(e.author_display(), "Smith, John");
    }

    #[test]
    fn test_entry_author_display_missing() {
        let e = make_entry(&[]);
        assert_eq!(e.author_display(), "");
    }

    #[test]
    fn test_entry_title_display() {
        let e = make_entry(&[("title", "My Paper")]);
        assert_eq!(e.title_display(), "My Paper");
    }

    #[test]
    fn test_entry_year_display() {
        let e = make_entry(&[("year", "2024")]);
        assert_eq!(e.year_display(), "2024");
    }

    #[test]
    fn test_entry_journal_display_journal() {
        let e = make_entry(&[("journal", "Nature")]);
        assert_eq!(e.journal_display(), "Nature");
    }

    #[test]
    fn test_entry_journal_display_booktitle_fallback() {
        let e = make_entry(&[("booktitle", "ICML 2024")]);
        assert_eq!(e.journal_display(), "ICML 2024");
    }

    #[test]
    fn test_entry_journal_display_empty_when_neither() {
        let e = make_entry(&[]);
        assert_eq!(e.journal_display(), "");
    }

    #[test]
    fn test_entry_journal_display_prefers_journal_over_booktitle() {
        let e = make_entry(&[("journal", "Nature"), ("booktitle", "Some Book")]);
        assert_eq!(e.journal_display(), "Nature");
    }

    #[test]
    fn test_raw_field_value_braced() {
        assert_eq!(RawFieldValue::Braced("hello".to_string()).to_string_value(), "hello");
    }

    #[test]
    fn test_raw_field_value_quoted() {
        assert_eq!(RawFieldValue::Quoted("world".to_string()).to_string_value(), "world");
    }

    #[test]
    fn test_raw_field_value_bare() {
        assert_eq!(RawFieldValue::Bare("2024".to_string()).to_string_value(), "2024");
    }

    #[test]
    fn test_raw_field_value_concat() {
        let v = RawFieldValue::Concat(vec![
            RawFieldValue::Braced("foo".to_string()),
            RawFieldValue::Bare("bar".to_string()),
        ]);
        assert_eq!(v.to_string_value(), "foo bar");
    }

    #[test]
    fn test_group_tree_default() {
        let tree = GroupTree::default();
        assert_eq!(tree.root.group.name, "All Entries");
        assert!(tree.root.children.is_empty());
        assert!(tree.root.expanded);
    }
}
