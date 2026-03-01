use ratatui::layout::{Constraint, Rect};
use ratatui::style::Modifier;
use ratatui::widgets::{Block, Borders, Cell, Row, Table, TableState};
use ratatui::Frame;

use crate::bib::model::Entry;
use crate::config::schema::{ColumnConfig, ColumnWidth};
use crate::tui::theme::Theme;
use crate::util::titlecase::strip_case_braces;
pub struct EntryListState {
    pub table_state: TableState,
}

impl EntryListState {
    pub fn new() -> Self {
        let mut state = TableState::default();
        state.select(Some(0));
        EntryListState {
            table_state: state,
        }
    }

    pub fn selected(&self) -> usize {
        self.table_state.selected().unwrap_or(0)
    }

    pub fn select(&mut self, idx: usize) {
        self.table_state.select(Some(idx));
    }
}

pub fn render_entry_list(
    f: &mut Frame,
    area: Rect,
    entries: &[&Entry],
    state: &mut EntryListState,
    columns: &[ColumnConfig],
    theme: &Theme,
    focused: bool,
    show_braces: bool,
) {
    let total_width = area.width.saturating_sub(2); // borders

    // Build constraints from column config
    let constraints: Vec<Constraint> = columns
        .iter()
        .map(|col| {
            match col.width {
                ColumnWidth::Fixed(w) => Constraint::Length(w),
                ColumnWidth::Percent(p) => {
                    let w = (total_width as u32 * p as u32 / 100) as u16;
                    if let Some(max) = col.max_width {
                        Constraint::Length(w.min(max))
                    } else {
                        Constraint::Length(w)
                    }
                }
                ColumnWidth::Flex => Constraint::Min(10),
            }
        })
        .collect();

    // Header
    let header_cells: Vec<Cell> = columns
        .iter()
        .map(|col| Cell::from(col.header.as_str()).style(theme.header))
        .collect();
    let header = Row::new(header_cells).style(theme.header).height(1);

    // Rows
    let rows: Vec<Row> = entries
        .iter()
        .map(|entry| {
            let cells: Vec<Cell> = columns
                .iter()
                .map(|col| {
                    let raw = get_field_value(entry, &col.field);
                    let value = if show_braces { raw } else { strip_case_braces(&raw) };
                    Cell::from(value)
                })
                .collect();
            Row::new(cells).height(1)
        })
        .collect();

    let border_style = if focused {
        theme.border.add_modifier(Modifier::BOLD)
    } else {
        theme.border
    };

    let table = Table::new(rows, &constraints)
        .header(header)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(border_style)
                .title(" Entries "),
        )
        .row_highlight_style(theme.selected);

    f.render_stateful_widget(table, area, &mut state.table_state);
}

fn get_field_value(entry: &Entry, field: &str) -> String {
    match field {
        "dirty" => if entry.dirty { "●".to_string() } else { " ".to_string() },
        "file_indicator" => {
            let has_file = entry.fields.get("file")
                .map(|v| !v.trim().is_empty())
                .unwrap_or(false);
            if has_file { "\u{2398}".to_string() } else { " ".to_string() }
        }
        "web_indicator" => {
            let has_doi = entry.fields.get("doi").map(|v| !v.trim().is_empty()).unwrap_or(false);
            let has_url = entry.fields.get("url").map(|v| !v.trim().is_empty()).unwrap_or(false);
            if has_doi || has_url { "\u{238B}".to_string() } else { " ".to_string() }
        }
        "entrytype" | "type" => entry.entry_type.display_name().to_string(),
        "citation_key" | "key" | "citekey" => entry.citation_key.clone(),
        "author" => entry.author_display(),
        "title" => entry.title_display(),
        "year" => entry.year_display(),
        "journal" => entry.journal_display(),
        _ => entry.fields.get(field).cloned().unwrap_or_default(),
    }
}
