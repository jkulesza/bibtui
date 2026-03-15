use std::path::Path;

use ratatui::layout::{Alignment, Constraint, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Cell, Row, Table, TableState};
use ratatui::Frame;

use crate::bib::model::Entry;
use crate::config::schema::{ColumnConfig, ColumnWidth};
use crate::tui::theme::Theme;
use crate::util::author::abbreviate_authors;
use crate::util::journal::abbreviate_journal;
use crate::util::latex::render_latex;
use crate::util::open::{parse_file_field, resolve_file_path};
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bib::model::{Entry, EntryType};
    use indexmap::IndexMap;

    fn make_entry(key: &str, dirty: bool, fields: &[(&str, &str)]) -> Entry {
        let mut f = IndexMap::new();
        for (k, v) in fields { f.insert(k.to_string(), v.to_string()); }
        Entry {
            entry_type: EntryType::Article,
            citation_key: key.to_string(),
            fields: f,
            group_memberships: vec![],
            raw_index: 0,
            dirty,
        }
    }

    #[test]
    fn test_new_starts_at_zero() {
        let s = EntryListState::new();
        assert_eq!(s.selected(), 0);
    }

    #[test]
    fn test_select() {
        let mut s = EntryListState::new();
        s.select(5);
        assert_eq!(s.selected(), 5);
    }

    #[test]
    fn test_get_field_value_dirty() {
        let e = make_entry("k", true, &[]);
        assert_eq!(get_field_value(&e, "dirty", false, false), "●");
        let e2 = make_entry("k", false, &[]);
        assert_eq!(get_field_value(&e2, "dirty", false, false), " ");
    }

    #[test]
    fn test_get_field_value_entrytype() {
        let e = make_entry("k", false, &[]);
        assert_eq!(get_field_value(&e, "entrytype", false, false), "Article");
        assert_eq!(get_field_value(&e, "type", false, false), "Article");
    }

    #[test]
    fn test_get_field_value_citation_key() {
        let e = make_entry("Smith2020", false, &[]);
        assert_eq!(get_field_value(&e, "citation_key", false, false), "Smith2020");
        assert_eq!(get_field_value(&e, "key", false, false), "Smith2020");
        assert_eq!(get_field_value(&e, "citekey", false, false), "Smith2020");
    }

    #[test]
    fn test_get_field_value_web_indicator() {
        let e = make_entry("k", false, &[("doi", "10.1234/x")]);
        assert!(get_field_value(&e, "web_indicator", false, false) != " ");
        let e2 = make_entry("k", false, &[]);
        assert_eq!(get_field_value(&e2, "web_indicator", false, false), " ");
    }

    #[test]
    fn test_get_field_value_author_abbreviated() {
        let e = make_entry("k", false, &[("author", "Smith, J. and Doe, J. and Brown, K.")]);
        let abbr = get_field_value(&e, "author", true, false);
        assert!(abbr.contains("et al"));
    }

    #[test]
    fn test_get_field_value_author_not_abbreviated() {
        let e = make_entry("k", false, &[("author", "Smith, J.")]);
        let full = get_field_value(&e, "author", false, false);
        assert_eq!(full, "Smith, J.");
    }

    #[test]
    fn test_get_field_value_arbitrary() {
        let e = make_entry("k", false, &[("note", "important")]);
        assert_eq!(get_field_value(&e, "note", false, false), "important");
        assert_eq!(get_field_value(&e, "missing", false, false), "");
    }

    #[test]
    fn test_apply_display_pipeline_strip_braces() {
        let s = apply_display_pipeline("{Hello} {World}", false, false);
        assert_eq!(s, "Hello World");
    }

    #[test]
    fn test_apply_display_pipeline_show_braces() {
        let s = apply_display_pipeline("{Hello}", true, false);
        assert_eq!(s, "{Hello}");
    }

    #[test]
    fn test_apply_display_pipeline_latex() {
        let s = apply_display_pipeline("caf{\\'e}", false, true);
        assert!(s.contains('é') || s == "café" || !s.contains('{'));
    }

    // ── Journal column ────────────────────────────────────────────────────────

    #[test]
    fn test_get_field_value_journal_raw_when_not_abbreviated() {
        let e = make_entry("k", false, &[("journal", "Nuclear Science and Engineering")]);
        assert_eq!(
            get_field_value(&e, "journal", false, false),
            "Nuclear Science and Engineering"
        );
    }

    #[test]
    fn test_get_field_value_journal_abbreviated_on_the_fly() {
        let e = make_entry("k", false, &[("journal", "Nuclear Science and Engineering")]);
        // ISO 4 abbreviation computed on the fly; no journal_abbrev field needed
        assert_eq!(get_field_value(&e, "journal", false, true), "Nucl. Sci. Eng.");
    }

    #[test]
    fn test_get_field_value_journal_uses_journal_full_as_source() {
        // journal holds the ISO 4 form (journal_field_content = "abbreviated"),
        // but journal_full has the canonical full name.  The display must use
        // journal_full so the column is stable regardless of what journal holds.
        let e = make_entry(
            "k",
            false,
            &[
                ("journal", "Nucl. Sci. Eng."),
                ("journal_full", "Nuclear Science and Engineering"),
            ],
        );
        assert_eq!(get_field_value(&e, "journal", false, true), "Nucl. Sci. Eng.");
    }

    #[test]
    fn test_get_field_value_journal_booktitle_fallback_when_not_abbreviated() {
        // No journal field → fall back to booktitle when abbreviation is off
        let e = make_entry("k", false, &[("booktitle", "Proceedings of ICML")]);
        assert_eq!(
            get_field_value(&e, "journal", false, false),
            "Proceedings of ICML"
        );
    }

    #[test]
    fn test_get_field_value_journal_booktitle_abbreviated() {
        // No journal or journal_full → abbreviate booktitle on the fly
        let e = make_entry("k", false, &[("booktitle", "Proceedings of ICML")]);
        let result = get_field_value(&e, "journal", false, true);
        // "Proceedings" → "Proc.", "of" → dropped, "ICML" → kept
        assert_eq!(result, "Proc. ICML");
    }

    #[test]
    fn test_get_field_value_journal_empty_when_no_fields() {
        let e = make_entry("k", false, &[]);
        assert_eq!(get_field_value(&e, "journal", false, false), "");
        assert_eq!(get_field_value(&e, "journal", false, true), "");
    }

    #[test]
    fn test_get_field_value_title() {
        let e = make_entry("k", false, &[("title", "My Paper")]);
        assert_eq!(get_field_value(&e, "title", false, false), "My Paper");
    }

    #[test]
    fn test_get_field_value_year() {
        let e = make_entry("k", false, &[("year", "2024")]);
        assert_eq!(get_field_value(&e, "year", false, false), "2024");
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
    render_latex_enabled: bool,
    abbreviate_authors_enabled: bool,
    abbreviate_journal_enabled: bool,
    bib_dir: &Path,
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

    // Only compute cell content for rows that are actually visible in the
    // viewport.  Off-screen rows get cheap empty cells; ratatui still needs
    // the full row count so that selection and scrolling work correctly.
    //
    // We must predict the offset ratatui *will* use this frame rather than
    // the stale offset from the previous frame.  ratatui scrolls the table
    // so that the selected row is always visible, using these rules:
    //   • selected < offset          → new offset = selected
    //   • selected ≥ offset + height → new offset = selected - height + 1
    //   • otherwise offset is unchanged
    // Replicating that here keeps our visible window in sync.

    // Subtract 2 for the top/bottom borders and 1 for the header row.
    let viewport_rows = area.height.saturating_sub(3) as usize;
    let selected = state.selected();
    let prev_offset = state.table_state.offset();
    let offset = if selected < prev_offset {
        selected
    } else if viewport_rows > 0 && selected >= prev_offset + viewport_rows {
        selected.saturating_sub(viewport_rows - 1)
    } else {
        prev_offset
    };
    let visible_end = (offset + viewport_rows).min(entries.len());

    let rows: Vec<Row> = entries
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            if i < offset || i >= visible_end {
                // Off-screen: emit a minimal placeholder row (no string work).
                return Row::new(vec![Cell::from(""); columns.len()]).height(1);
            }
            let cells: Vec<Cell> = columns
                .iter()
                .map(|col| {
                    if col.field == "file_indicator" {
                        return file_indicator_cell(entry, bib_dir);
                    }
                    let raw = get_field_value(entry, &col.field, abbreviate_authors_enabled, abbreviate_journal_enabled);
                    let value = apply_display_pipeline(&raw, show_braces, render_latex_enabled);
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
                .title(" Entries ")
                .title(
                    Line::from(format!(
                        " {} v{} ",
                        env!("CARGO_PKG_NAME"),
                        env!("CARGO_PKG_VERSION")
                    ))
                    .alignment(Alignment::Right),
                ),
        )
        .row_highlight_style(theme.selected);

    f.render_stateful_widget(table, area, &mut state.table_state);
}

/// Return a styled Cell for the file indicator column.
/// Red if the `file` field has content but every referenced file is missing on disk;
/// normal otherwise (including when the field is absent).
fn file_indicator_cell(entry: &Entry, bib_dir: &Path) -> Cell<'static> {
    let file_val = match entry.fields.get("file") {
        Some(v) if !v.trim().is_empty() => v.trim().to_string(),
        _ => return Cell::from(" "),
    };
    let files = parse_file_field(&file_val);
    let all_missing = files.is_empty()
        || files
            .iter()
            .all(|f| !resolve_file_path(&f.path, bib_dir).exists());
    if all_missing {
        Cell::from("\u{2398}").style(Style::default().fg(Color::Red))
    } else {
        Cell::from("\u{2398}")
    }
}

fn get_field_value(entry: &Entry, field: &str, abbreviate_authors_enabled: bool, abbreviate_journal_enabled: bool) -> String {
    match field {
        "dirty" => if entry.dirty { "●".to_string() } else { " ".to_string() },
        "web_indicator" => {
            let has_doi = entry.fields.get("doi").map(|v| !v.trim().is_empty()).unwrap_or(false);
            let has_url = entry.fields.get("url").map(|v| !v.trim().is_empty()).unwrap_or(false);
            if has_doi || has_url { "\u{238B}".to_string() } else { " ".to_string() }
        }
        "entrytype" | "type" => entry.entry_type.display_name().to_string(),
        "citation_key" | "key" | "citekey" => entry.citation_key.clone(),
        "author" => {
            let raw = entry.author_display();
            if abbreviate_authors_enabled { abbreviate_authors(&raw) } else { raw }
        }
        "title" => entry.title_display(),
        "year" => entry.year_display(),
        "journal" => {
            if abbreviate_journal_enabled {
                // Use journal_full as the canonical source (present after a save),
                // fall back to journal, then booktitle. Abbreviate on the fly so
                // the result is correct even without a stored journal_abbrev field.
                let full = entry.fields.get("journal_full")
                    .filter(|v| !v.is_empty())
                    .or_else(|| entry.fields.get("journal"))
                    .or_else(|| entry.fields.get("booktitle"))
                    .cloned()
                    .unwrap_or_default();
                abbreviate_journal(&full, &indexmap::IndexMap::new())
            } else {
                entry.journal_display()
            }
        }
        _ => entry.fields.get(field).cloned().unwrap_or_default(),
    }
}

/// Apply the display pipeline: optionally render LaTeX, then optionally strip braces.
/// LaTeX must run first because it needs the `{...}` accent patterns.
fn apply_display_pipeline(value: &str, show_braces: bool, render_latex_enabled: bool) -> String {
    let s = if render_latex_enabled {
        render_latex(value)
    } else {
        value.to_string()
    };
    if show_braces {
        s
    } else {
        strip_case_braces(&s)
    }
}
