use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub general: GeneralConfig,
    pub display: DisplayConfig,
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
}

impl Default for GeneralConfig {
    fn default() -> Self {
        GeneralConfig {
            bib_file: None,
            editor: "nvim".to_string(),
            backup_on_save: true,
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
    /// Toggle at runtime with `B`. Default true (show as-is).
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
            show_braces: true,
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
}

impl Default for TitlecaseConfig {
    fn default() -> Self {
        TitlecaseConfig {
            ignore_words: vec!["MCNP".to_string(), "OpenMC".to_string()],
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

/// A named group of extra fields shown as a separate section in the detail view.
/// Fields listed here that are present on an entry are pulled out of the generic
/// "Other" section and shown under their own header.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomFieldGroup {
    pub name: String,
    pub fields: Vec<String>,
}
