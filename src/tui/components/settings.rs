use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::config::schema::{Config, CustomFieldGroup};
use crate::tui::theme::Theme;

// ── Setting value ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum SettingValue {
    Bool(bool),
    Str(String),
    /// Cycles through a fixed list of string options.
    Choice { options: &'static [&'static str], index: usize },
}

impl SettingValue {
    pub fn display(&self) -> String {
        match self {
            SettingValue::Bool(b) => if *b { "true" } else { "false" }.to_string(),
            SettingValue::Str(s) => s.clone(),
            SettingValue::Choice { options, index } => options[*index].to_string(),
        }
    }

    pub fn toggle(&mut self) {
        match self {
            SettingValue::Bool(b) => *b = !*b,
            SettingValue::Choice { options, index } => *index = (*index + 1) % options.len(),
            SettingValue::Str(_) => {}
        }
    }

    /// True if Enter/Space should cycle the value (bool or fixed-choice).
    pub fn is_cyclic(&self) -> bool {
        matches!(self, SettingValue::Bool(_) | SettingValue::Choice { .. })
    }
}

// ── Setting item ──────────────────────────────────────────────────────────────

pub struct SettingItem {
    pub id: String,
    pub label: String,
    pub description: String,
    pub value: SettingValue,
    pub default: SettingValue,
}

// ── Row (section header or setting) ──────────────────────────────────────────

pub enum SettingRow {
    Section(&'static str),
    Item(usize),       // index into SettingsState.items
    FieldGroup(usize), // index into SettingsState.field_groups
}

// ── State ─────────────────────────────────────────────────────────────────────

pub struct SettingsState {
    pub items: Vec<SettingItem>,
    pub rows: Vec<SettingRow>,
    /// Index into `rows`; always points at a selectable row.
    pub cursor: usize,
    pub scroll_offset: usize,
    /// Custom field groups: (name, comma-separated field list).
    pub field_groups: Vec<(String, String)>,
}

/// Standard BibTeX entry types shown in the Citekey Templates section.
const CITEKEY_TYPES: &[&str] = &[
    "article",
    "book",
    "booklet",
    "inbook",
    "incollection",
    "inproceedings",
    "manual",
    "mastersthesis",
    "misc",
    "phdthesis",
    "proceedings",
    "techreport",
    "unpublished",
];

impl SettingsState {
    pub fn new(config: &Config) -> Self {
        let defaults = Config::default();

        let mut items: Vec<SettingItem> = vec![
            // ── General ──
            SettingItem {
                id: "general.backup_on_save".into(),
                label: "backup_on_save".into(),
                description: "Create a .bib.bak backup file before each save.".into(),
                value: SettingValue::Bool(config.general.backup_on_save),
                default: SettingValue::Bool(defaults.general.backup_on_save),
            },
            SettingItem {
                id: "general.yank_format".into(),
                label: "yank_format".into(),
                description: "What 'yy' copies: citation_key | bibtex | formatted | prompt (prompt opens a picker each time).".into(),
                value: {
                    const OPTS: &[&str] = &["citation_key", "bibtex", "formatted", "prompt"];
                    let idx = OPTS.iter().position(|&o| o == config.general.yank_format.as_str()).unwrap_or(3);
                    SettingValue::Choice { options: OPTS, index: idx }
                },
                default: {
                    const OPTS: &[&str] = &["citation_key", "bibtex", "formatted", "prompt"];
                    let idx = OPTS.iter().position(|&o| o == defaults.general.yank_format.as_str()).unwrap_or(3);
                    SettingValue::Choice { options: OPTS, index: idx }
                },
            },
            SettingItem {
                id: "general.editor".into(),
                label: "editor".into(),
                description: "External editor command used when opening the .bib file directly.".into(),
                value: SettingValue::Str(config.general.editor.clone()),
                default: SettingValue::Str(defaults.general.editor.clone()),
            },
            // ── Display ──
            SettingItem {
                id: "display.show_groups".into(),
                label: "show_groups".into(),
                description: "Show the JabRef group sidebar on startup.".into(),
                value: SettingValue::Bool(config.display.show_groups),
                default: SettingValue::Bool(defaults.display.show_groups),
            },
            SettingItem {
                id: "display.render_latex".into(),
                label: "render_latex".into(),
                description: "Render LaTeX markup (accents, math, dashes) to Unicode for display. Toggle at runtime with L.".into(),
                value: SettingValue::Bool(config.display.render_latex),
                default: SettingValue::Bool(defaults.display.render_latex),
            },
            SettingItem {
                id: "display.show_braces".into(),
                label: "show_braces".into(),
                description: "Show BibTeX case-protecting braces (e.g. {MCNP}) in field values. Toggle at runtime with B.".into(),
                value: SettingValue::Bool(config.display.show_braces),
                default: SettingValue::Bool(defaults.display.show_braces),
            },
            SettingItem {
                id: "display.abbreviate_authors".into(),
                label: "abbreviate_authors".into(),
                description: "Abbreviate author lists in the entry list (3+ authors shown as 'Last et al.').".into(),
                value: SettingValue::Bool(config.display.abbreviate_authors),
                default: SettingValue::Bool(defaults.display.abbreviate_authors),
            },
            // ── Save ──
            SettingItem {
                id: "save.align_fields".into(),
                label: "align_fields".into(),
                description: "Align field values to a common column when serialising modified entries.".into(),
                value: SettingValue::Bool(config.save.align_fields),
                default: SettingValue::Bool(defaults.save.align_fields),
            },
            SettingItem {
                id: "save.field_order".into(),
                label: "field_order".into(),
                description: "Field ordering strategy on save: 'jabref' (JabRef default order) or 'alphabetical'.".into(),
                value: {
                    const OPTS: &[&str] = &["jabref", "alphabetical"];
                    let idx = OPTS.iter().position(|&o| o == config.save.field_order.as_str()).unwrap_or(0);
                    SettingValue::Choice { options: OPTS, index: idx }
                },
                default: {
                    const OPTS: &[&str] = &["jabref", "alphabetical"];
                    let idx = OPTS.iter().position(|&o| o == defaults.save.field_order.as_str()).unwrap_or(0);
                    SettingValue::Choice { options: OPTS, index: idx }
                },
            },
            SettingItem {
                id: "save.sync_filenames".into(),
                label: "sync_filenames".into(),
                description: "Rename attached files to match the citation key on save (single: key.ext; multiple: key_1.ext…).".into(),
                value: SettingValue::Bool(config.save.sync_filenames),
                default: SettingValue::Bool(defaults.save.sync_filenames),
            },
            // ── Citation ──
            SettingItem {
                id: "citation.style".into(),
                label: "style".into(),
                description: "Citation preview format style shown when pressing Space on an entry. Currently supported: IEEEtranN.".into(),
                value: {
                    const OPTS: &[&str] = &["IEEEtranN"];
                    let idx = OPTS.iter().position(|&o| o == config.citation.style.as_str()).unwrap_or(0);
                    SettingValue::Choice { options: OPTS, index: idx }
                },
                default: {
                    const OPTS: &[&str] = &["IEEEtranN"];
                    let idx = OPTS.iter().position(|&o| o == defaults.citation.style.as_str()).unwrap_or(0);
                    SettingValue::Choice { options: OPTS, index: idx }
                },
            },
            // ── General (continued) ──
            SettingItem {
                id: "general.bib_file".into(),
                label: "bib_file".into(),
                description: "Default .bib file to open when none is given on the command line. Leave empty to require a CLI argument.".into(),
                value: SettingValue::Str(config.general.bib_file.clone().unwrap_or_default()),
                default: SettingValue::Str(defaults.general.bib_file.clone().unwrap_or_default()),
            },
            // ── Display (continued) ──
            SettingItem {
                id: "display.group_sidebar_width".into(),
                label: "group_sidebar_width".into(),
                description: "Width in columns of the group sidebar.".into(),
                value: SettingValue::Str(config.display.group_sidebar_width.to_string()),
                default: SettingValue::Str(defaults.display.group_sidebar_width.to_string()),
            },
            SettingItem {
                id: "display.default_sort.field".into(),
                label: "default_sort.field".into(),
                description: "Field used to sort the entry list on startup. Any BibTeX field name, or: citation_key, entrytype.".into(),
                value: SettingValue::Str(config.display.default_sort.field.clone()),
                default: SettingValue::Str(defaults.display.default_sort.field.clone()),
            },
            SettingItem {
                id: "display.default_sort.ascending".into(),
                label: "default_sort.ascending".into(),
                description: "Sort direction: true = A→Z (ascending), false = Z→A (descending).".into(),
                value: SettingValue::Bool(config.display.default_sort.ascending),
                default: SettingValue::Bool(defaults.display.default_sort.ascending),
            },
            // ── Titlecase ──
            SettingItem {
                id: "titlecase.ignore_words".into(),
                label: "ignore_words".into(),
                description: "Words reproduced verbatim (case-insensitive) when pressing T to apply title case. Enter as comma-separated list, e.g.: MCNP, OpenMC, LaTeX.".into(),
                value: SettingValue::Str(config.titlecase.ignore_words.join(", ")),
                default: SettingValue::Str(defaults.titlecase.ignore_words.join(", ")),
            },
            SettingItem {
                id: "titlecase.stop_words".into(),
                label: "stop_words".into(),
                description: "Words lowercased in title case unless first or last. Enter as comma-separated list, e.g.: a, an, the, and, or, in, of.".into(),
                value: SettingValue::Str(config.titlecase.stop_words.join(", ")),
                default: SettingValue::Str(defaults.titlecase.stop_words.join(", ")),
            },
            // ── Theme ──
            SettingItem {
                id: "theme.selected_bg".into(),
                label: "selected_bg".into(),
                description: "Background colour of the selected row (hex, e.g. #3b4261).".into(),
                value: SettingValue::Str(config.theme.selected_bg.clone()),
                default: SettingValue::Str(defaults.theme.selected_bg.clone()),
            },
            SettingItem {
                id: "theme.selected_fg".into(),
                label: "selected_fg".into(),
                description: "Foreground colour of the selected row (hex, e.g. #c0caf5).".into(),
                value: SettingValue::Str(config.theme.selected_fg.clone()),
                default: SettingValue::Str(defaults.theme.selected_fg.clone()),
            },
            SettingItem {
                id: "theme.header_bg".into(),
                label: "header_bg".into(),
                description: "Background colour of header and status bars (hex, e.g. #1a1b26).".into(),
                value: SettingValue::Str(config.theme.header_bg.clone()),
                default: SettingValue::Str(defaults.theme.header_bg.clone()),
            },
            SettingItem {
                id: "theme.header_fg".into(),
                label: "header_fg".into(),
                description: "Foreground colour of header text (hex, e.g. #7aa2f7).".into(),
                value: SettingValue::Str(config.theme.header_fg.clone()),
                default: SettingValue::Str(defaults.theme.header_fg.clone()),
            },
            SettingItem {
                id: "theme.search_match".into(),
                label: "search_match".into(),
                description: "Colour used to highlight fuzzy-search matches (hex, e.g. #ff9e64).".into(),
                value: SettingValue::Str(config.theme.search_match.clone()),
                default: SettingValue::Str(defaults.theme.search_match.clone()),
            },
            SettingItem {
                id: "theme.border_color".into(),
                label: "border_color".into(),
                description: "Colour of panel borders (hex, e.g. #565f89).".into(),
                value: SettingValue::Str(config.theme.border_color.clone()),
                default: SettingValue::Str(defaults.theme.border_color.clone()),
            },
            // ── Save Actions ──
            SettingItem {
                id: "save_actions.escape_underscores".into(),
                label: "escape_underscores".into(),
                description: "Escape bare underscores (_) as \\_ in text fields on save (math mode is skipped).".into(),
                value: SettingValue::Bool(config.save.save_action_escape_underscores),
                default: SettingValue::Bool(defaults.save.save_action_escape_underscores),
            },
            SettingItem {
                id: "save_actions.escape_ampersands".into(),
                label: "escape_ampersands".into(),
                description: "Escape bare ampersands (&) as \\& in text and name fields on save.".into(),
                value: SettingValue::Bool(config.save.save_action_escape_ampersands),
                default: SettingValue::Bool(defaults.save.save_action_escape_ampersands),
            },
            SettingItem {
                id: "save_actions.cleanup_url".into(),
                label: "cleanup_url".into(),
                description: "Decode percent-encoded characters in the 'url' field on save (e.g. %2F → /).".into(),
                value: SettingValue::Bool(config.save.save_action_cleanup_url),
                default: SettingValue::Bool(defaults.save.save_action_cleanup_url),
            },
            SettingItem {
                id: "save_actions.latex_cleanup".into(),
                label: "latex_cleanup".into(),
                description: "Escape bare % signs as \\% and collapse multiple spaces in text fields on save.".into(),
                value: SettingValue::Bool(config.save.save_action_latex_cleanup),
                default: SettingValue::Bool(defaults.save.save_action_latex_cleanup),
            },
            SettingItem {
                id: "save_actions.normalize_date".into(),
                label: "normalize_date".into(),
                description: "Normalise the 'date' field to ISO 8601 format (yyyy-MM-dd or yyyy-MM) on save.".into(),
                value: SettingValue::Bool(config.save.save_action_normalize_date),
                default: SettingValue::Bool(defaults.save.save_action_normalize_date),
            },
            SettingItem {
                id: "save_actions.normalize_month".into(),
                label: "normalize_month".into(),
                description: "Normalise the 'month' field to a three-letter BibTeX abbreviation (jan, feb, …) on save.".into(),
                value: SettingValue::Bool(config.save.save_action_normalize_month),
                default: SettingValue::Bool(defaults.save.save_action_normalize_month),
            },
            SettingItem {
                id: "save_actions.normalize_names_of_persons".into(),
                label: "normalize_names_of_persons".into(),
                description: "Normalise author and editor names to 'Last, First' form on save.".into(),
                value: SettingValue::Bool(config.save.save_action_normalize_names_of_persons),
                default: SettingValue::Bool(defaults.save.save_action_normalize_names_of_persons),
            },
            SettingItem {
                id: "save_actions.normalize_page_numbers".into(),
                label: "normalize_page_numbers".into(),
                description: "Normalise page ranges to double-hyphen format (1-5 → 1--5) on save.".into(),
                value: SettingValue::Bool(config.save.save_action_normalize_page_numbers),
                default: SettingValue::Bool(defaults.save.save_action_normalize_page_numbers),
            },
            SettingItem {
                id: "save_actions.normalize_isbn".into(),
                label: "normalize_isbn".into(),
                description: "Strip hyphens/spaces from ISBN fields and uppercase the check digit on save.".into(),
                value: SettingValue::Bool(config.save.save_action_normalize_isbn),
                default: SettingValue::Bool(defaults.save.save_action_normalize_isbn),
            },
            SettingItem {
                id: "save_actions.ordinals_to_superscript".into(),
                label: "ordinals_to_superscript".into(),
                description: "Convert ordinal suffixes to LaTeX superscripts on save (1st → 1\\textsuperscript{st}).".into(),
                value: SettingValue::Bool(config.save.save_action_ordinals_to_superscript),
                default: SettingValue::Bool(defaults.save.save_action_ordinals_to_superscript),
            },
            SettingItem {
                id: "save_actions.unicode_to_latex".into(),
                label: "unicode_to_latex".into(),
                description: "Convert Unicode characters to LaTeX equivalents on save (é → {\\'e}, ü → {\\\"u}).".into(),
                value: SettingValue::Bool(config.save.save_action_unicode_to_latex),
                default: SettingValue::Bool(defaults.save.save_action_unicode_to_latex),
            },
            SettingItem {
                id: "save_actions.abbreviate_journal".into(),
                label: "abbreviate_journal".into(),
                description: "On save, populate journal_abbrev (ISO 4) and journal_full companion fields \
                              and rewrite journal per journal_field_content. \
                              The Journal column always displays the abbreviated form when available.".into(),
                value: SettingValue::Bool(config.save.save_action_abbreviate_journal),
                default: SettingValue::Bool(defaults.save.save_action_abbreviate_journal),
            },
            SettingItem {
                id: "save_actions.journal_field_content".into(),
                label: "journal_field_content".into(),
                description: "Controls what the journal field holds after the abbreviate_journal save action: \
                              \"full\" keeps the full name in journal; \
                              \"abbreviated\" stores the ISO 4 form instead.".into(),
                value: SettingValue::Choice {
                    options: &["full", "abbreviated"],
                    index: if config.save.journal_field_content == "abbreviated" { 1 } else { 0 },
                },
                default: SettingValue::Choice {
                    options: &["full", "abbreviated"],
                    index: if defaults.save.journal_field_content == "abbreviated" { 1 } else { 0 },
                },
            },
        ];

        // ── Citekey Templates (one item per standard entry type) ──
        let base = items.len(); // 11
        for type_name in CITEKEY_TYPES {
            let current = config.citekey.templates.get(*type_name).cloned().unwrap_or_default();
            let default_val = defaults.citekey.templates.get(*type_name).cloned().unwrap_or_default();
            items.push(SettingItem {
                id: format!("citekey.template.{}", type_name),
                label: type_name.to_string(),
                description: format!(
                    "Citation key template for @{} entries. \
                     Tokens: [auth], [year], [title], [journal:abbr], [authors], \
                     [firstpage], [number], [institution:abbr], [booktitle:abbr]. \
                     Modifiers: :upper, :lower, :abbr, :camel, :(n), :regex(pat,repl).",
                    type_name
                ),
                value: SettingValue::Str(current),
                default: SettingValue::Str(default_val),
            });
        }

        // Item index map:
        //  0  general.backup_on_save       7  save.align_fields
        //  1  general.yank_format          8  save.field_order
        //  2  general.editor               9  save.sync_filenames
        //  3  display.show_groups         10  citation.style
        //  4  display.render_latex        11  general.bib_file
        //  5  display.show_braces         12  display.group_sidebar_width
        //  6  display.abbreviate_authors  13  display.default_sort.field
        //                                14  display.default_sort.ascending
        //                                15  titlecase.ignore_words
        //                                16  titlecase.stop_words
        //                                17  theme.selected_bg
        //                                18  theme.selected_fg
        //                                19  theme.header_bg
        //                                20  theme.header_fg
        //                                21  theme.search_match
        //                                22  theme.border_color
        //  Save Actions (23–34):
        //                                23  escape_underscores
        //                                24  escape_ampersands
        //                                25  cleanup_url
        //                                26  latex_cleanup
        //                                27  normalize_date
        //                                28  normalize_month
        //                                29  normalize_names_of_persons
        //                                30  normalize_page_numbers
        //                                31  normalize_isbn
        //                                32  ordinals_to_superscript
        //                                33  unicode_to_latex
        //                                34  abbreviate_journal
        //  Display (continued):          35  journal_field_content
        //                                36+ citekey templates
        let mut rows: Vec<SettingRow> = vec![
            SettingRow::Section("General"),
            SettingRow::Item(11), // bib_file
            SettingRow::Item(0),  // backup_on_save
            SettingRow::Item(1),  // yank_format
            SettingRow::Item(2),  // editor
            SettingRow::Section("Display"),
            SettingRow::Item(3),  // show_groups
            SettingRow::Item(12), // group_sidebar_width
            SettingRow::Item(4),  // render_latex
            SettingRow::Item(5),  // show_braces
            SettingRow::Item(6),  // abbreviate_authors
            SettingRow::Item(35), // journal_field_content
            SettingRow::Item(13), // default_sort.field
            SettingRow::Item(14), // default_sort.ascending
            SettingRow::Section("Save"),
            SettingRow::Item(7),  // align_fields
            SettingRow::Item(8),  // field_order
            SettingRow::Item(9),  // sync_filenames
            SettingRow::Section("Save Actions"),
            SettingRow::Item(23), // escape_underscores
            SettingRow::Item(24), // escape_ampersands
            SettingRow::Item(25), // cleanup_url
            SettingRow::Item(26), // latex_cleanup
            SettingRow::Item(27), // normalize_date
            SettingRow::Item(28), // normalize_month
            SettingRow::Item(29), // normalize_names_of_persons
            SettingRow::Item(30), // normalize_page_numbers
            SettingRow::Item(31), // normalize_isbn
            SettingRow::Item(32), // ordinals_to_superscript
            SettingRow::Item(33), // unicode_to_latex
            SettingRow::Item(34), // abbreviate_journal
            SettingRow::Section("Citation"),
            SettingRow::Item(10), // style
            SettingRow::Section("Titlecase"),
            SettingRow::Item(15), // ignore_words
            SettingRow::Item(16), // stop_words
            SettingRow::Section("Theme"),
            SettingRow::Item(17), // selected_bg
            SettingRow::Item(18), // selected_fg
            SettingRow::Item(19), // header_bg
            SettingRow::Item(20), // header_fg
            SettingRow::Item(21), // search_match
            SettingRow::Item(22), // border_color
            SettingRow::Section("Citekey Templates"),
        ];
        for i in 0..CITEKEY_TYPES.len() {
            rows.push(SettingRow::Item(base + i));
        }

        // ── Field Groups (one row per group) ──
        rows.push(SettingRow::Section("Field Groups"));
        let fg_rows_start = rows.len();
        let _ = fg_rows_start; // used only to document intent
        let field_groups: Vec<(String, String)> = config.field_groups.iter()
            .map(|fg| (fg.name.clone(), fg.fields.join(", ")))
            .collect();
        for i in 0..field_groups.len() {
            rows.push(SettingRow::FieldGroup(i));
        }

        let cursor = rows
            .iter()
            .position(|r| matches!(r, SettingRow::Item(_) | SettingRow::FieldGroup(_)))
            .unwrap_or(0);

        SettingsState { items, rows, cursor, scroll_offset: 0, field_groups }
    }

    fn is_selectable_row(&self, idx: usize) -> bool {
        matches!(self.rows.get(idx), Some(SettingRow::Item(_)) | Some(SettingRow::FieldGroup(_)))
    }

    pub fn move_down(&mut self) {
        let mut i = self.cursor + 1;
        while i < self.rows.len() {
            if self.is_selectable_row(i) {
                self.cursor = i;
                return;
            }
            i += 1;
        }
    }

    pub fn move_up(&mut self) {
        if self.cursor == 0 {
            return;
        }
        let mut i = self.cursor - 1;
        loop {
            if self.is_selectable_row(i) {
                self.cursor = i;
                return;
            }
            if i == 0 {
                return;
            }
            i -= 1;
        }
    }

    pub fn move_to_top(&mut self) {
        self.cursor = 0;
        while self.cursor < self.rows.len() && !self.is_selectable_row(self.cursor) {
            self.cursor += 1;
        }
    }

    pub fn move_to_bottom(&mut self) {
        if self.rows.is_empty() {
            return;
        }
        self.cursor = self.rows.len() - 1;
        while self.cursor > 0 && !self.is_selectable_row(self.cursor) {
            self.cursor -= 1;
        }
    }

    pub fn move_page_down(&mut self) {
        for _ in 0..10 {
            self.move_down();
        }
    }

    pub fn move_page_up(&mut self) {
        for _ in 0..10 {
            self.move_up();
        }
    }

    pub fn selected_item(&self) -> Option<&SettingItem> {
        match self.rows.get(self.cursor) {
            Some(SettingRow::Item(idx)) => self.items.get(*idx),
            _ => None,
        }
    }

    pub fn selected_item_mut(&mut self) -> Option<&mut SettingItem> {
        match self.rows.get(self.cursor) {
            Some(SettingRow::Item(idx)) => {
                let idx = *idx;
                self.items.get_mut(idx)
            }
            _ => None,
        }
    }

    /// True when the currently selected row is a field group.
    pub fn selected_is_field_group(&self) -> bool {
        matches!(self.rows.get(self.cursor), Some(SettingRow::FieldGroup(_)))
    }

    /// Index into `field_groups` for the selected row, if it is a field group.
    pub fn selected_field_group_index(&self) -> Option<usize> {
        match self.rows.get(self.cursor) {
            Some(SettingRow::FieldGroup(i)) => Some(*i),
            _ => None,
        }
    }

    /// Add a new (empty-fields) field group with the given name and move cursor to it.
    pub fn add_field_group(&mut self, name: String) {
        let idx = self.field_groups.len();
        self.field_groups.push((name, String::new()));
        // Insert after the last existing FieldGroup row (or after the "Field Groups" section header).
        let insert_at = self.rows.iter().rposition(|r| matches!(r, SettingRow::FieldGroup(_)))
            .map(|i| i + 1)
            .unwrap_or_else(|| {
                self.rows.iter().rposition(|r| matches!(r, SettingRow::Section("Field Groups")))
                    .map(|i| i + 1)
                    .unwrap_or(self.rows.len())
            });
        self.rows.insert(insert_at, SettingRow::FieldGroup(idx));
        self.cursor = insert_at;
    }

    /// Delete the currently selected field group. Returns true if one was deleted.
    pub fn delete_selected_field_group(&mut self) -> bool {
        let idx = match self.rows.get(self.cursor) {
            Some(SettingRow::FieldGroup(i)) => *i,
            _ => return false,
        };
        self.field_groups.remove(idx);
        self.rows.remove(self.cursor);
        // Re-number FieldGroup indices that came after the removed one.
        for row in &mut self.rows {
            if let SettingRow::FieldGroup(i) = row {
                if *i > idx {
                    *i -= 1;
                }
            }
        }
        // Adjust cursor to stay on a selectable row.
        if self.cursor >= self.rows.len() {
            self.cursor = self.cursor.saturating_sub(1);
        }
        while self.cursor > 0 && !self.is_selectable_row(self.cursor) {
            self.cursor -= 1;
        }
        true
    }

    /// Update the name of a field group by index.
    pub fn set_field_group_name(&mut self, index: usize, name: String) {
        if let Some(fg) = self.field_groups.get_mut(index) {
            fg.0 = name;
        }
    }

    /// Update the fields CSV of a field group by index.
    pub fn set_field_group_fields(&mut self, index: usize, fields_csv: String) {
        if let Some(fg) = self.field_groups.get_mut(index) {
            fg.1 = fields_csv;
        }
    }

    /// Toggle the selected bool/choice setting; no-op for strings and field groups.
    pub fn toggle_selected(&mut self) {
        if let Some(item) = self.selected_item_mut() {
            item.value.toggle();
        }
    }

    /// ID string of the currently selected setting, or `None` on a section header or field group.
    pub fn selected_id(&self) -> Option<&str> {
        self.selected_item().map(|i| i.id.as_str())
    }


    /// Current display string of the selected value (seed for the field editor).
    pub fn selected_value_str(&self) -> String {
        self.selected_item().map(|i| i.value.display()).unwrap_or_default()
    }

    /// Update a setting by ID and return whether it was found.
    pub fn set_value(&mut self, id: &str, value: SettingValue) -> bool {
        if let Some(item) = self.items.iter_mut().find(|i| i.id == id) {
            item.value = value;
            true
        } else {
            false
        }
    }

    /// Apply all current values to a `Config` in place.
    ///
    /// Some fields (e.g. `titlecase.stop_words`) are normalised (sorted) during
    /// application; the displayed `SettingValue` is updated to match.
    pub fn apply_to_config(&mut self, config: &mut Config) {
        for item in &self.items {
            match item.id.as_str() {
                "general.backup_on_save" => {
                    if let SettingValue::Bool(v) = item.value { config.general.backup_on_save = v; }
                }
                "general.yank_format" => {
                    if let SettingValue::Choice { options, index } = &item.value {
                        config.general.yank_format = options[*index].to_string();
                    }
                }
                "general.editor" => {
                    if let SettingValue::Str(v) = &item.value { config.general.editor = v.clone(); }
                }
                "display.show_groups" => {
                    if let SettingValue::Bool(v) = item.value { config.display.show_groups = v; }
                }
                "display.render_latex" => {
                    if let SettingValue::Bool(v) = item.value { config.display.render_latex = v; }
                }
                "display.show_braces" => {
                    if let SettingValue::Bool(v) = item.value { config.display.show_braces = v; }
                }
                "display.abbreviate_authors" => {
                    if let SettingValue::Bool(v) = item.value { config.display.abbreviate_authors = v; }
                }
                "save.align_fields" => {
                    if let SettingValue::Bool(v) = item.value { config.save.align_fields = v; }
                }
                "save.field_order" => {
                    if let SettingValue::Choice { options, index } = &item.value {
                        config.save.field_order = options[*index].to_string();
                    }
                }
                "save.sync_filenames" => {
                    if let SettingValue::Bool(v) = item.value { config.save.sync_filenames = v; }
                }
                "citation.style" => {
                    if let SettingValue::Choice { options, index } = &item.value {
                        config.citation.style = options[*index].to_string();
                    }
                }
                "general.bib_file" => {
                    if let SettingValue::Str(v) = &item.value {
                        config.general.bib_file = if v.trim().is_empty() { None } else { Some(v.trim().to_string()) };
                    }
                }
                "display.group_sidebar_width" => {
                    if let SettingValue::Str(v) = &item.value {
                        if let Ok(n) = v.trim().parse::<u16>() {
                            config.display.group_sidebar_width = n;
                        }
                    }
                }
                "display.default_sort.field" => {
                    if let SettingValue::Str(v) = &item.value {
                        config.display.default_sort.field = v.trim().to_string();
                    }
                }
                "display.default_sort.ascending" => {
                    if let SettingValue::Bool(v) = item.value { config.display.default_sort.ascending = v; }
                }
                "titlecase.ignore_words" => {
                    if let SettingValue::Str(v) = &item.value {
                        config.titlecase.ignore_words = v
                            .split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect();
                    }
                }
                "titlecase.stop_words" => {
                    if let SettingValue::Str(v) = &item.value {
                        let mut words: Vec<String> = v
                            .split(',')
                            .map(|s| s.trim().to_string())
                            .filter(|s| !s.is_empty())
                            .collect();
                        words.sort_unstable();
                        config.titlecase.stop_words = words;
                    }
                }
                "save_actions.escape_underscores" => {
                    if let SettingValue::Bool(v) = item.value { config.save.save_action_escape_underscores = v; }
                }
                "save_actions.escape_ampersands" => {
                    if let SettingValue::Bool(v) = item.value { config.save.save_action_escape_ampersands = v; }
                }
                "save_actions.cleanup_url" => {
                    if let SettingValue::Bool(v) = item.value { config.save.save_action_cleanup_url = v; }
                }
                "save_actions.latex_cleanup" => {
                    if let SettingValue::Bool(v) = item.value { config.save.save_action_latex_cleanup = v; }
                }
                "save_actions.normalize_date" => {
                    if let SettingValue::Bool(v) = item.value { config.save.save_action_normalize_date = v; }
                }
                "save_actions.normalize_month" => {
                    if let SettingValue::Bool(v) = item.value { config.save.save_action_normalize_month = v; }
                }
                "save_actions.normalize_names_of_persons" => {
                    if let SettingValue::Bool(v) = item.value { config.save.save_action_normalize_names_of_persons = v; }
                }
                "save_actions.normalize_page_numbers" => {
                    if let SettingValue::Bool(v) = item.value { config.save.save_action_normalize_page_numbers = v; }
                }
                "save_actions.normalize_isbn" => {
                    if let SettingValue::Bool(v) = item.value { config.save.save_action_normalize_isbn = v; }
                }
                "save_actions.ordinals_to_superscript" => {
                    if let SettingValue::Bool(v) = item.value { config.save.save_action_ordinals_to_superscript = v; }
                }
                "save_actions.unicode_to_latex" => {
                    if let SettingValue::Bool(v) = item.value { config.save.save_action_unicode_to_latex = v; }
                }
                "save_actions.abbreviate_journal" => {
                    if let SettingValue::Bool(v) = item.value { config.save.save_action_abbreviate_journal = v; }
                }
                "save_actions.journal_field_content" => {
                    if let SettingValue::Choice { options, index } = &item.value {
                        config.save.journal_field_content = options[*index].to_string();
                    }
                }
                "theme.selected_bg"  => { if let SettingValue::Str(v) = &item.value { config.theme.selected_bg  = v.clone(); } }
                "theme.selected_fg"  => { if let SettingValue::Str(v) = &item.value { config.theme.selected_fg  = v.clone(); } }
                "theme.header_bg"    => { if let SettingValue::Str(v) = &item.value { config.theme.header_bg    = v.clone(); } }
                "theme.header_fg"    => { if let SettingValue::Str(v) = &item.value { config.theme.header_fg    = v.clone(); } }
                "theme.search_match" => { if let SettingValue::Str(v) = &item.value { config.theme.search_match = v.clone(); } }
                "theme.border_color" => { if let SettingValue::Str(v) = &item.value { config.theme.border_color = v.clone(); } }
                id if id.starts_with("citekey.template.") => {
                    if let SettingValue::Str(v) = &item.value {
                        let type_name = &id["citekey.template.".len()..];
                        config.citekey.templates.insert(type_name.to_string(), v.clone());
                    }
                }
                _ => {}
            }
        }
        // Sync sorted stop_words back to the displayed SettingValue so the UI
        // reflects the canonical order without requiring the screen to be reopened.
        if let Some(item) = self.items.iter_mut().find(|i| i.id == "titlecase.stop_words") {
            item.value = SettingValue::Str(config.titlecase.stop_words.join(", "));
        }
        // Apply field groups
        config.field_groups = self.field_groups.iter()
            .filter(|(name, _)| !name.trim().is_empty())
            .map(|(name, fields_csv)| CustomFieldGroup {
                name: name.clone(),
                fields: fields_csv.split(',')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect(),
            })
            .collect();
    }

    /// Adjust scroll so the cursor row is visible within `viewport_height` lines.
    pub fn ensure_visible(&mut self, viewport_height: usize) {
        if viewport_height == 0 {
            return;
        }
        if self.cursor < self.scroll_offset {
            self.scroll_offset = self.cursor;
        } else if self.cursor >= self.scroll_offset + viewport_height {
            self.scroll_offset = self.cursor - viewport_height + 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::defaults::default_config;

    // ── SettingValue ──

    #[test]
    fn test_bool_display() {
        assert_eq!(SettingValue::Bool(true).display(), "true");
        assert_eq!(SettingValue::Bool(false).display(), "false");
    }

    #[test]
    fn test_str_display() {
        assert_eq!(SettingValue::Str("hello".into()).display(), "hello");
    }

    #[test]
    fn test_choice_display() {
        let v = SettingValue::Choice { options: &["a", "b", "c"], index: 1 };
        assert_eq!(v.display(), "b");
    }

    #[test]
    fn test_bool_toggle() {
        let mut v = SettingValue::Bool(false);
        v.toggle();
        assert_eq!(v, SettingValue::Bool(true));
        v.toggle();
        assert_eq!(v, SettingValue::Bool(false));
    }

    #[test]
    fn test_choice_toggle_cycles() {
        let mut v = SettingValue::Choice { options: &["x", "y", "z"], index: 0 };
        v.toggle();
        assert_eq!(v, SettingValue::Choice { options: &["x", "y", "z"], index: 1 });
        v.toggle();
        assert_eq!(v, SettingValue::Choice { options: &["x", "y", "z"], index: 2 });
        v.toggle(); // wraps
        assert_eq!(v, SettingValue::Choice { options: &["x", "y", "z"], index: 0 });
    }

    #[test]
    fn test_str_toggle_is_noop() {
        let mut v = SettingValue::Str("hello".into());
        v.toggle();
        assert_eq!(v, SettingValue::Str("hello".into()));
    }

    #[test]
    fn test_is_cyclic() {
        assert!(SettingValue::Bool(true).is_cyclic());
        assert!(SettingValue::Choice { options: &["a"], index: 0 }.is_cyclic());
        assert!(!SettingValue::Str("x".into()).is_cyclic());
    }

    // ── SettingsState ──

    #[test]
    fn test_new_creates_items() {
        let cfg = default_config();
        let state = SettingsState::new(&cfg);
        assert!(!state.items.is_empty());
        assert!(!state.rows.is_empty());
    }

    #[test]
    fn test_cursor_starts_on_item() {
        let cfg = default_config();
        let state = SettingsState::new(&cfg);
        assert!(matches!(state.rows[state.cursor], SettingRow::Item(_)));
    }

    #[test]
    fn test_move_down_skips_sections() {
        let cfg = default_config();
        let mut state = SettingsState::new(&cfg);
        let start = state.cursor;
        state.move_down();
        assert!(state.cursor > start);
        assert!(matches!(state.rows[state.cursor], SettingRow::Item(_)));
    }

    #[test]
    fn test_move_up_skips_sections() {
        let cfg = default_config();
        let mut state = SettingsState::new(&cfg);
        state.move_down();
        state.move_down();
        let after_down = state.cursor;
        state.move_up();
        assert!(state.cursor < after_down);
        assert!(matches!(state.rows[state.cursor], SettingRow::Item(_)));
    }

    #[test]
    fn test_move_up_at_top_is_noop() {
        let cfg = default_config();
        let mut state = SettingsState::new(&cfg);
        // cursor starts on the first Item row (which has a Section above it, not an Item)
        let top = state.cursor;
        state.move_up(); // no item row above — should be a no-op
        assert_eq!(state.cursor, top);
    }

    #[test]
    fn test_selected_item() {
        let cfg = default_config();
        let state = SettingsState::new(&cfg);
        assert!(state.selected_item().is_some());
    }

    #[test]
    fn test_selected_id() {
        let cfg = default_config();
        let state = SettingsState::new(&cfg);
        assert!(state.selected_id().is_some());
    }

    #[test]
    fn test_toggle_selected_bool() {
        let cfg = default_config();
        let mut state = SettingsState::new(&cfg);
        // Find a bool item
        while !matches!(state.selected_item().map(|i| &i.value), Some(SettingValue::Bool(_))) {
            state.move_down();
        }
        let before = match state.selected_item().unwrap().value.clone() {
            SettingValue::Bool(b) => b,
            _ => panic!(),
        };
        state.toggle_selected();
        let after = match state.selected_item().unwrap().value.clone() {
            SettingValue::Bool(b) => b,
            _ => panic!(),
        };
        assert_eq!(after, !before);
    }

    #[test]
    fn test_set_value() {
        let cfg = default_config();
        let mut state = SettingsState::new(&cfg);
        let found = state.set_value("general.editor", SettingValue::Str("vim".into()));
        assert!(found);
        let item = state.items.iter().find(|i| i.id == "general.editor").unwrap();
        assert_eq!(item.value, SettingValue::Str("vim".into()));
    }

    #[test]
    fn test_set_value_unknown_id() {
        let cfg = default_config();
        let mut state = SettingsState::new(&cfg);
        assert!(!state.set_value("does.not.exist", SettingValue::Bool(true)));
    }

    #[test]
    fn test_apply_to_config_bool() {
        let cfg = default_config();
        let mut state = SettingsState::new(&cfg);
        state.set_value("general.backup_on_save", SettingValue::Bool(false));
        let mut new_cfg = cfg.clone();
        state.apply_to_config(&mut new_cfg);
        assert!(!new_cfg.general.backup_on_save);
    }

    #[test]
    fn test_apply_to_config_choice() {
        let cfg = default_config();
        let mut state = SettingsState::new(&cfg);
        state.set_value("save.field_order", SettingValue::Choice {
            options: &["jabref", "alphabetical"],
            index: 1,
        });
        let mut new_cfg = cfg.clone();
        state.apply_to_config(&mut new_cfg);
        assert_eq!(new_cfg.save.field_order, "alphabetical");
    }

    #[test]
    fn test_apply_to_config_str() {
        let cfg = default_config();
        let mut state = SettingsState::new(&cfg);
        state.set_value("general.editor", SettingValue::Str("nano".into()));
        let mut new_cfg = cfg.clone();
        state.apply_to_config(&mut new_cfg);
        assert_eq!(new_cfg.general.editor, "nano");
    }

    #[test]
    fn test_ensure_visible_scrolls_down() {
        let cfg = default_config();
        let mut state = SettingsState::new(&cfg);
        state.cursor = 20;
        state.scroll_offset = 0;
        state.ensure_visible(10);
        assert!(state.scroll_offset > 0);
    }

    #[test]
    fn test_ensure_visible_scrolls_up() {
        let cfg = default_config();
        let mut state = SettingsState::new(&cfg);
        state.cursor = 2;
        state.scroll_offset = 10;
        state.ensure_visible(10);
        assert_eq!(state.scroll_offset, 2);
    }

    // ── Field Groups ──

    #[test]
    fn test_new_loads_field_groups_from_config() {
        let cfg = default_config();
        let state = SettingsState::new(&cfg);
        // Default config has one field group ("Identifiers")
        assert!(!state.field_groups.is_empty());
        let (name, _) = &state.field_groups[0];
        assert_eq!(name, "Identifiers");
    }

    #[test]
    fn test_field_groups_have_rows() {
        let cfg = default_config();
        let state = SettingsState::new(&cfg);
        let fg_rows = state.rows.iter().filter(|r| matches!(r, SettingRow::FieldGroup(_))).count();
        assert_eq!(fg_rows, cfg.field_groups.len());
    }

    #[test]
    fn test_add_field_group_appends_and_moves_cursor() {
        let cfg = default_config();
        let mut state = SettingsState::new(&cfg);
        let before_count = state.field_groups.len();
        state.add_field_group("My Group".to_string());
        assert_eq!(state.field_groups.len(), before_count + 1);
        assert_eq!(state.field_groups.last().unwrap().0, "My Group");
        assert_eq!(state.field_groups.last().unwrap().1, "");
        // cursor should be on the new FieldGroup row
        assert!(matches!(state.rows[state.cursor], SettingRow::FieldGroup(_)));
    }

    #[test]
    fn test_delete_field_group_removes_it() {
        let cfg = default_config();
        let mut state = SettingsState::new(&cfg);
        // Navigate to a field group row
        while !state.selected_is_field_group() {
            let before = state.cursor;
            state.move_down();
            if state.cursor == before { break; }
        }
        assert!(state.selected_is_field_group());
        let before_count = state.field_groups.len();
        let deleted = state.delete_selected_field_group();
        assert!(deleted);
        assert_eq!(state.field_groups.len(), before_count - 1);
        // cursor should be on a selectable row after deletion
        assert!(state.is_selectable_row(state.cursor));
    }

    #[test]
    fn test_delete_non_field_group_returns_false() {
        let cfg = default_config();
        let mut state = SettingsState::new(&cfg);
        // cursor starts on an Item row
        assert!(!state.selected_is_field_group());
        assert!(!state.delete_selected_field_group());
    }

    #[test]
    fn test_set_field_group_name() {
        let cfg = default_config();
        let mut state = SettingsState::new(&cfg);
        state.set_field_group_name(0, "Renamed".to_string());
        assert_eq!(state.field_groups[0].0, "Renamed");
    }

    #[test]
    fn test_set_field_group_fields() {
        let cfg = default_config();
        let mut state = SettingsState::new(&cfg);
        state.set_field_group_fields(0, "doi, url".to_string());
        assert_eq!(state.field_groups[0].1, "doi, url");
    }

    #[test]
    fn test_selected_field_group_index() {
        let cfg = default_config();
        let mut state = SettingsState::new(&cfg);
        while !state.selected_is_field_group() {
            let before = state.cursor;
            state.move_down();
            if state.cursor == before { break; }
        }
        assert!(state.selected_field_group_index().is_some());
    }

    #[test]
    fn test_apply_to_config_field_groups() {
        let cfg = default_config();
        let mut state = SettingsState::new(&cfg);
        state.field_groups = vec![
            ("Physics".to_string(), "doi, url".to_string()),
        ];
        let mut new_cfg = cfg.clone();
        state.apply_to_config(&mut new_cfg);
        assert_eq!(new_cfg.field_groups.len(), 1);
        assert_eq!(new_cfg.field_groups[0].name, "Physics");
        assert_eq!(new_cfg.field_groups[0].fields, vec!["doi", "url"]);
    }

    #[test]
    fn test_apply_to_config_skips_empty_group_names() {
        let cfg = default_config();
        let mut state = SettingsState::new(&cfg);
        state.field_groups = vec![
            ("".to_string(), "doi".to_string()),
            ("Identifiers".to_string(), "isbn".to_string()),
        ];
        let mut new_cfg = cfg.clone();
        state.apply_to_config(&mut new_cfg);
        assert_eq!(new_cfg.field_groups.len(), 1);
        assert_eq!(new_cfg.field_groups[0].name, "Identifiers");
    }

    #[test]
    fn test_delete_and_renumber_field_groups() {
        let cfg = default_config();
        let mut state = SettingsState::new(&cfg);
        // Add a second group so we have two
        state.add_field_group("Second".to_string());
        assert_eq!(state.field_groups.len(), 2);
        // Place cursor on the first FieldGroup row (index 0)
        let first_fg_row = state.rows.iter().position(|r| matches!(r, SettingRow::FieldGroup(0))).unwrap();
        state.cursor = first_fg_row;
        assert_eq!(state.selected_field_group_index(), Some(0));
        state.delete_selected_field_group();
        // Remaining group should now be at index 0
        assert_eq!(state.field_groups.len(), 1);
        // All FieldGroup rows should reference valid indices
        for row in &state.rows {
            if let SettingRow::FieldGroup(i) = row {
                assert!(*i < state.field_groups.len(), "stale FieldGroup index {}", i);
            }
        }
    }

    #[test]
    fn test_move_down_reaches_field_group_rows() {
        let cfg = default_config();
        let mut state = SettingsState::new(&cfg);
        let mut found = false;
        for _ in 0..state.rows.len() {
            if state.selected_is_field_group() {
                found = true;
                break;
            }
            let before = state.cursor;
            state.move_down();
            if state.cursor == before { break; }
        }
        assert!(found, "move_down should reach the Field Groups section");
    }

    // ── Extended navigation ───────────────────────────────────────────────────

    #[test]
    fn test_move_to_top_returns_to_first_item() {
        let cfg = default_config();
        let mut state = SettingsState::new(&cfg);
        let top = state.cursor;
        // Move away from the top
        state.move_down();
        state.move_down();
        state.move_down();
        assert!(state.cursor > top);
        state.move_to_top();
        assert_eq!(state.cursor, top);
        assert!(state.is_selectable_row(state.cursor));
    }

    #[test]
    fn test_move_to_bottom_reaches_last_selectable() {
        let cfg = default_config();
        let mut state = SettingsState::new(&cfg);
        let top = state.cursor;
        state.move_to_bottom();
        assert!(state.cursor > top);
        assert!(state.is_selectable_row(state.cursor));
        // A second call should be idempotent
        let bottom = state.cursor;
        state.move_to_bottom();
        assert_eq!(state.cursor, bottom);
    }

    #[test]
    fn test_move_page_down_advances_multiple_items() {
        let cfg = default_config();
        let mut state = SettingsState::new(&cfg);
        let start = state.cursor;
        state.move_page_down();
        assert!(state.cursor > start);
        assert!(state.is_selectable_row(state.cursor));
    }

    #[test]
    fn test_move_page_up_retreats_multiple_items() {
        let cfg = default_config();
        let mut state = SettingsState::new(&cfg);
        // First get to the bottom so there is room to page up
        state.move_to_bottom();
        let bottom = state.cursor;
        state.move_page_up();
        assert!(state.cursor < bottom);
        assert!(state.is_selectable_row(state.cursor));
    }

    #[test]
    fn test_move_page_up_at_top_is_noop() {
        let cfg = default_config();
        let mut state = SettingsState::new(&cfg);
        let top = state.cursor;
        state.move_page_up();
        assert_eq!(state.cursor, top);
    }

    #[test]
    fn test_move_page_down_at_bottom_is_noop() {
        let cfg = default_config();
        let mut state = SettingsState::new(&cfg);
        state.move_to_bottom();
        let bottom = state.cursor;
        state.move_page_down();
        assert_eq!(state.cursor, bottom);
    }

    #[test]
    fn test_selected_value_str_returns_display() {
        let cfg = default_config();
        let state = SettingsState::new(&cfg);
        // Just verify no panic — the value may be empty (Str) or non-empty (Bool/Choice)
        let _val = state.selected_value_str();
    }

    #[test]
    fn test_selected_item_mut_returns_some() {
        let cfg = default_config();
        let mut state = SettingsState::new(&cfg);
        assert!(state.selected_item_mut().is_some());
    }

    #[test]
    fn test_toggle_selected_bool_via_selected_item_mut() {
        let cfg = default_config();
        let mut state = SettingsState::new(&cfg);
        // Navigate to a Bool item (backup_on_save = Item(0), which is at rows[2])
        state.cursor = state.rows.iter()
            .position(|r| matches!(r, SettingRow::Item(0)))
            .expect("backup_on_save row must exist");
        let before = state.selected_value_str();
        assert!(!before.is_empty(), "Bool display should be non-empty");
        state.toggle_selected();
        let after = state.selected_value_str();
        assert_ne!(before, after);
    }
}

// ── Render ────────────────────────────────────────────────────────────────────

const LABEL_W: usize = 26;
const VAL_W: usize = 18;

pub fn render_settings(f: &mut Frame, area: Rect, state: &mut SettingsState, theme: &Theme) {
    // Split into list + description + hint
    let v = Layout::vertical([
        Constraint::Min(3),
        Constraint::Length(3),
        Constraint::Length(1),
    ])
    .split(area);

    let list_area = v[0];
    let desc_area = v[1];
    let hint_area = v[2];

    // ── List block ──────────────────────────────────────────────────────────
    let list_block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.border)
        .title(" Settings ");
    let list_inner = list_block.inner(list_area);
    f.render_widget(list_block, list_area);

    let viewport_h = list_inner.height as usize;
    state.ensure_visible(viewport_h);

    let modified_style = Style::default()
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD);
    let section_sep_style = theme.border;

    let default_field_groups: Vec<(String, String)> = Config::default().field_groups.iter()
        .map(|fg| (fg.name.clone(), fg.fields.join(", ")))
        .collect();

    let lines: Vec<Line> = state
        .rows
        .iter()
        .enumerate()
        .skip(state.scroll_offset)
        .take(viewport_h)
        .map(|(row_idx, row)| match row {
            SettingRow::Section(name) => {
                let label = format!(" ── {} ", name);
                let fill_w = (list_inner.width as usize).saturating_sub(label.len());
                let fill = "─".repeat(fill_w);
                Line::from(vec![
                    Span::styled(label, theme.header),
                    Span::styled(fill, section_sep_style),
                ])
            }
            SettingRow::Item(item_idx) => {
                let item = &state.items[*item_idx];
                let is_selected = row_idx == state.cursor;
                let is_modified = item.value != item.default;

                let base_style =
                    if is_selected { theme.selected } else { Style::default() };

                let cursor_ch = if is_selected { "▶" } else { " " };
                let type_ch = match &item.value {
                    SettingValue::Bool(true) => "[✓]",
                    SettingValue::Bool(false) => "[ ]",
                    SettingValue::Choice { .. } => "[⇄]",
                    SettingValue::Str(_) => "[-]",
                };

                let label = format!(" {:<w$}", item.label, w = LABEL_W);
                let val_str = item.value.display();
                let val_trunc: String = val_str.chars().take(VAL_W).collect();
                let val_padded = format!("{:<w$}", val_trunc, w = VAL_W);

                let default_str = item.default.display();
                let default_trunc: String = default_str.chars().take(VAL_W).collect();

                let mod_marker = if is_modified { "● " } else { "  " };
                let default_hint = format!("default: {}", default_trunc);

                let val_style = if is_modified { modified_style } else { base_style };
                let hint_style = if is_modified {
                    Style::default().fg(Color::DarkGray)
                } else {
                    theme.label
                };
                let mod_style = if is_modified {
                    Style::default().fg(Color::Yellow)
                } else {
                    base_style
                };

                Line::from(vec![
                    Span::styled(format!(" {} {} ", cursor_ch, type_ch), base_style),
                    Span::styled(label, base_style),
                    Span::styled(val_padded, val_style),
                    Span::styled(mod_marker, mod_style),
                    Span::styled(default_hint, hint_style),
                ])
            }
            SettingRow::FieldGroup(fg_idx) => {
                let (name, fields_csv) = match state.field_groups.get(*fg_idx) {
                    Some(fg) => fg,
                    None => return Line::from(""),
                };
                let is_selected = row_idx == state.cursor;
                let is_modified = !default_field_groups.iter()
                    .any(|(dn, df)| dn == name && df == fields_csv);

                let base_style = if is_selected { theme.selected } else { Style::default() };
                let cursor_ch = if is_selected { "▶" } else { " " };

                let label = format!(" {:<w$}", name, w = LABEL_W);
                let val_trunc: String = fields_csv.chars().take(VAL_W).collect();
                let val_padded = format!("{:<w$}", val_trunc, w = VAL_W);
                let mod_marker = if is_modified { "● " } else { "  " };
                let val_style = if is_modified { modified_style } else { base_style };
                let mod_style = if is_modified {
                    Style::default().fg(Color::Yellow)
                } else {
                    base_style
                };

                Line::from(vec![
                    Span::styled(format!(" {} [G] ", cursor_ch), base_style),
                    Span::styled(label, base_style),
                    Span::styled(val_padded, val_style),
                    Span::styled(mod_marker, mod_style),
                    Span::styled("fields (comma-separated)", theme.label),
                ])
            }
        })
        .collect();

    f.render_widget(Paragraph::new(lines), list_inner);

    // ── Description block ───────────────────────────────────────────────────
    let desc_block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.border)
        .title(" Description ");
    let desc_inner = desc_block.inner(desc_area);
    f.render_widget(desc_block, desc_area);

    let desc_text = state
        .selected_item()
        .map(|i| i.description.as_str())
        .unwrap_or("");
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!(" {}", desc_text),
            theme.value,
        )))
        .wrap(ratatui::widgets::Wrap { trim: false }),
        desc_inner,
    );

    // ── Hint bar ────────────────────────────────────────────────────────────
    let action_hint = if state.selected_is_field_group() {
        "e: edit fields  r: rename  a: add group  x: delete group"
    } else {
        match state.selected_item().map(|i| &i.value) {
            Some(SettingValue::Bool(_)) => "Enter/Space: toggle  a: add field group",
            Some(SettingValue::Choice { .. }) => "Enter/Space: cycle  a: add field group",
            _ => "e: edit value  a: add field group",
        }
    };
    let hint = format!(
        " j/k: navigate  {}  E: export  I: import  Esc: close",
        action_hint
    );
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(hint, theme.label))),
        hint_area,
    );
}
