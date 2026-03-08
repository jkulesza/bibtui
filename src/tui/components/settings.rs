use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

use crate::config::schema::Config;
use crate::tui::theme::Theme;

// ── Setting value ────────────────────────────────────────────────────────────

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
            SettingValue::Choice { options, index } => {
                *index = (*index + 1) % options.len();
            }
            SettingValue::Str(_) => {}
        }
    }

    /// True if Enter/Space should cycle the value (bool or fixed-choice).
    pub fn is_cyclic(&self) -> bool {
        matches!(self, SettingValue::Bool(_) | SettingValue::Choice { .. })
    }
}

// ── Setting item ─────────────────────────────────────────────────────────────

pub struct SettingItem {
    pub id: &'static str,
    pub label: &'static str,
    pub description: &'static str,
    pub value: SettingValue,
    pub default: SettingValue,
}

// ── Row (section header or setting) ──────────────────────────────────────────

pub enum SettingRow {
    Section(&'static str),
    Item(usize), // index into SettingsState.items
}

// ── State ────────────────────────────────────────────────────────────────────

pub struct SettingsState {
    pub items: Vec<SettingItem>,
    pub rows: Vec<SettingRow>,
    /// Index into `rows`; always points at a `SettingRow::Item`.
    pub cursor: usize,
    pub scroll_offset: usize,
}

impl SettingsState {
    pub fn new(config: &Config) -> Self {
        let defaults = Config::default();

        let items: Vec<SettingItem> = vec![
            // ── General ──
            SettingItem {
                id: "general.backup_on_save",
                label: "backup_on_save",
                description: "Create a .bib.bak backup file before each save.",
                value: SettingValue::Bool(config.general.backup_on_save),
                default: SettingValue::Bool(defaults.general.backup_on_save),
            },
            SettingItem {
                id: "general.yank_format",
                label: "yank_format",
                description: "What 'yy' copies: citation_key | bibtex | formatted | prompt (prompt opens a picker each time).",
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
                id: "general.editor",
                label: "editor",
                description: "External editor command used when opening the .bib file directly.",
                value: SettingValue::Str(config.general.editor.clone()),
                default: SettingValue::Str(defaults.general.editor.clone()),
            },
            // ── Display ──
            SettingItem {
                id: "display.show_groups",
                label: "show_groups",
                description: "Show the JabRef group sidebar on startup.",
                value: SettingValue::Bool(config.display.show_groups),
                default: SettingValue::Bool(defaults.display.show_groups),
            },
            SettingItem {
                id: "display.render_latex",
                label: "render_latex",
                description: "Render LaTeX markup (accents, math, dashes) to Unicode for display. Toggle at runtime with L.",
                value: SettingValue::Bool(config.display.render_latex),
                default: SettingValue::Bool(defaults.display.render_latex),
            },
            SettingItem {
                id: "display.show_braces",
                label: "show_braces",
                description: "Show BibTeX case-protecting braces (e.g. {MCNP}) in field values. Toggle at runtime with B.",
                value: SettingValue::Bool(config.display.show_braces),
                default: SettingValue::Bool(defaults.display.show_braces),
            },
            SettingItem {
                id: "display.abbreviate_authors",
                label: "abbreviate_authors",
                description: "Abbreviate author lists in the entry list (3+ authors shown as 'Last et al.').",
                value: SettingValue::Bool(config.display.abbreviate_authors),
                default: SettingValue::Bool(defaults.display.abbreviate_authors),
            },
            // ── Save ──
            SettingItem {
                id: "save.align_fields",
                label: "align_fields",
                description: "Align field values to a common column when serialising modified entries.",
                value: SettingValue::Bool(config.save.align_fields),
                default: SettingValue::Bool(defaults.save.align_fields),
            },
            SettingItem {
                id: "save.field_order",
                label: "field_order",
                description: "Field ordering strategy on save: 'jabref' (JabRef default order) or 'alphabetical'.",
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
                id: "save.sync_filenames",
                label: "sync_filenames",
                description: "Rename attached files to match the citation key on save (single: key.ext; multiple: key_1.ext…).",
                value: SettingValue::Bool(config.save.sync_filenames),
                default: SettingValue::Bool(defaults.save.sync_filenames),
            },
            // ── Citation ──
            SettingItem {
                id: "citation.style",
                label: "style",
                description: "Citation preview format style shown when pressing Space on an entry. Currently supported: IEEEtranN.",
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
        ];

        let rows: Vec<SettingRow> = vec![
            SettingRow::Section("General"),
            SettingRow::Item(0),  // yank_format
            SettingRow::Item(1),  // backup_on_save
            SettingRow::Item(2),  // editor
            SettingRow::Section("Display"),
            SettingRow::Item(3),
            SettingRow::Item(4),
            SettingRow::Item(5),
            SettingRow::Item(6),
            SettingRow::Section("Save"),
            SettingRow::Item(7),
            SettingRow::Item(8),
            SettingRow::Item(9),
            SettingRow::Section("Citation"),
            SettingRow::Item(10),
        ];

        let cursor = rows
            .iter()
            .position(|r| matches!(r, SettingRow::Item(_)))
            .unwrap_or(0);

        SettingsState { items, rows, cursor, scroll_offset: 0 }
    }

    fn is_item_row(&self, idx: usize) -> bool {
        matches!(self.rows.get(idx), Some(SettingRow::Item(_)))
    }

    pub fn move_down(&mut self) {
        let mut i = self.cursor + 1;
        while i < self.rows.len() {
            if self.is_item_row(i) {
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
            if self.is_item_row(i) {
                self.cursor = i;
                return;
            }
            if i == 0 {
                return;
            }
            i -= 1;
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

    /// Toggle the selected bool setting; no-op for strings.
    pub fn toggle_selected(&mut self) {
        if let Some(item) = self.selected_item_mut() {
            item.value.toggle();
        }
    }

    /// Dotted ID of the currently selected setting, or `None` on a section header.
    pub fn selected_id(&self) -> Option<&'static str> {
        self.selected_item().map(|i| i.id)
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
    pub fn apply_to_config(&self, config: &mut Config) {
        for item in &self.items {
            match (item.id, &item.value) {
                ("general.yank_format", SettingValue::Choice { options, index }) => {
                    config.general.yank_format = options[*index].to_string();
                }
                ("general.backup_on_save", SettingValue::Bool(v)) => {
                    config.general.backup_on_save = *v;
                }
                ("general.editor", SettingValue::Str(v)) => {
                    config.general.editor = v.clone();
                }
                ("display.show_groups", SettingValue::Bool(v)) => {
                    config.display.show_groups = *v;
                }
                ("display.render_latex", SettingValue::Bool(v)) => {
                    config.display.render_latex = *v;
                }
                ("display.show_braces", SettingValue::Bool(v)) => {
                    config.display.show_braces = *v;
                }
                ("display.abbreviate_authors", SettingValue::Bool(v)) => {
                    config.display.abbreviate_authors = *v;
                }
                ("save.align_fields", SettingValue::Bool(v)) => {
                    config.save.align_fields = *v;
                }
                ("save.field_order", SettingValue::Choice { options, index }) => {
                    config.save.field_order = options[*index].to_string();
                }
                ("save.sync_filenames", SettingValue::Bool(v)) => {
                    config.save.sync_filenames = *v;
                }
                ("citation.style", SettingValue::Choice { options, index }) => {
                    config.citation.style = options[*index].to_string();
                }
                _ => {}
            }
        }
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

// ── Render ───────────────────────────────────────────────────────────────────

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
                // Truncate value display to VAL_W
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
        .map(|i| i.description)
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
    let action_hint = match state.selected_item().map(|i| &i.value) {
        Some(SettingValue::Bool(_)) => "Enter/Space: toggle",
        Some(SettingValue::Choice { .. }) => "Enter/Space: cycle",
        _ => "e: edit value",
    };
    let hint = format!(
        " j/k: navigate  {}  E: export config  I: import config  Esc: close",
        action_hint
    );
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(hint, theme.label))),
        hint_area,
    );
}
