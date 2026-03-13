use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub general: GeneralConfig,
    pub display: DisplayConfig,
    pub citation: CitationConfig,
    pub citekey: CitekeyConfig,
    pub entry_types: IndexMap<String, EntryTypeConfig>,
    pub save: SaveConfig,
    pub theme: ThemeConfig,
    pub titlecase: TitlecaseConfig,
    pub field_groups: Vec<CustomFieldGroup>,
}

impl Default for Config {
    fn default() -> Self {
        super::defaults::default_config()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GeneralConfig {
    pub bib_file: Option<String>,
    pub editor: String,
    pub backup_on_save: bool,
    /// What `yy` copies to the clipboard.
    ///
    /// Values: `citation_key` | `bibtex` | `formatted` | `prompt`
    ///
    /// `prompt` opens a picker dialog each time so the user can choose.
    pub yank_format: String,
}

impl Default for GeneralConfig {
    fn default() -> Self {
        GeneralConfig {
            bib_file: None,
            editor: "nvim".to_string(),
            backup_on_save: true,
            yank_format: "prompt".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct DisplayConfig {
    pub show_groups: bool,
    pub group_sidebar_width: u16,
    pub columns: Vec<ColumnConfig>,
    pub default_sort: SortConfig,
    /// Show BibTeX case-protecting braces (e.g. `{MCNP}`) in field values.
    /// Toggle at runtime with `B`. Default false (braces hidden).
    pub show_braces: bool,
    /// Render LaTeX markup (accents, math, dashes) to Unicode for display.
    /// Toggle at runtime with `L`. Default true.
    pub render_latex: bool,
    /// Abbreviate author lists in the entry list (1 author → last name;
    /// 2 → "Last1 and Last2"; 3+ → "Last1 et al."). Default true.
    pub abbreviate_authors: bool,
}

impl Default for DisplayConfig {
    fn default() -> Self {
        DisplayConfig {
            show_groups: true,
            group_sidebar_width: 30,
            columns: super::defaults::default_columns(),
            default_sort: SortConfig {
                field: "citation_key".to_string(),
                ascending: true,
            },
            show_braces: false,
            render_latex: true,
            abbreviate_authors: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ColumnConfig {
    pub field: String,
    pub header: String,
    pub width: ColumnWidth,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_width: Option<u16>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ColumnWidth {
    Fixed(u16),
    Percent(u16),
    Flex,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SortConfig {
    pub field: String,
    pub ascending: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CitekeyConfig {
    pub templates: IndexMap<String, String>,
}

impl Default for CitekeyConfig {
    fn default() -> Self {
        CitekeyConfig {
            templates: super::defaults::default_citekey_templates(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntryTypeConfig {
    pub required: Vec<String>,
    pub optional: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SaveConfig {
    pub align_fields: bool,
    pub field_order: String,
    pub actions: Vec<SaveAction>,
    /// Rename attached files to match the citation key on save.
    /// Single file: `citekey.ext`. Multiple files: `citekey_1.ext`, `citekey_2.ext`, …
    pub sync_filenames: bool,
}

impl Default for SaveConfig {
    fn default() -> Self {
        SaveConfig {
            align_fields: true,
            field_order: "jabref".to_string(),
            actions: vec![
                SaveAction {
                    field: "month".to_string(),
                    action: "normalize_month".to_string(),
                },
                SaveAction {
                    field: "pages".to_string(),
                    action: "normalize_page_numbers".to_string(),
                },
            ],
            sync_filenames: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveAction {
    pub field: String,
    pub action: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TitlecaseConfig {
    /// Words to reproduce verbatim regardless of position (case-insensitive match).
    /// Defaults to ["MCNP", "OpenMC"].
    pub ignore_words: Vec<String>,
    /// Words lowercased in title case unless they are the first or last word.
    /// Defaults to standard English articles, conjunctions, and short prepositions.
    pub stop_words: Vec<String>,
}

impl Default for TitlecaseConfig {
    fn default() -> Self {
        TitlecaseConfig {
            ignore_words: vec!["MCNP".to_string(), "OpenMC".to_string()],
            stop_words: vec![
                "a", "an", "and", "as", "at",
                "but", "by",
                "for",
                "in",
                "nor",
                "of", "off", "on", "or", "out",
                "per",
                "so",
                "the", "to",
                "up",
                "via", "vs",
                "yet",
            ]
            .into_iter()
            .map(str::to_string)
            .collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ThemeConfig {
    pub selected_bg: String,
    pub selected_fg: String,
    pub header_bg: String,
    pub header_fg: String,
    pub search_match: String,
    pub border_color: String,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        ThemeConfig {
            selected_bg: "#3b4261".to_string(),
            selected_fg: "#c0caf5".to_string(),
            header_bg: "#1a1b26".to_string(),
            header_fg: "#7aa2f7".to_string(),
            search_match: "#ff9e64".to_string(),
            border_color: "#565f89".to_string(),
        }
    }
}

/// Configuration for the citation preview popup (Space in entry list).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CitationConfig {
    /// Bibliography style used to format the preview.
    /// Currently supported: IEEEtranN (default).
    pub style: String,
}

impl Default for CitationConfig {
    fn default() -> Self {
        CitationConfig { style: "IEEEtranN".to_string() }
    }
}

/// A named group of extra fields shown as a separate section in the detail view.
/// Fields listed here that are present on an entry are pulled out of the generic
/// "Other" section and shown under their own header.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomFieldGroup {
    pub name: String,
    pub fields: Vec<String>,
}
