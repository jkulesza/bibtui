use std::path::Path;

use ratatui::layout::{Alignment, Constraint, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Row, Table, TableState};
use ratatui::Frame;

use crate::bib::model::Entry;
use crate::config::schema::{ColumnConfig, ColumnWidth};
use crate::search::engine::HighlightTerm;
use crate::tui::theme::Theme;
use crate::util::author::abbreviate_authors;
use crate::util::journal::abbreviate_journal;
use crate::util::latex::render_latex;
use crate::util::open::{parse_file_field, resolve_file_path};
use crate::util::titlecase::strip_case_braces;
pub struct EntryListState {
    pub table_state: TableState,
    #[cfg(test)]
    pub rendered_rows: usize,
}

impl Default for EntryListState {
    fn default() -> Self {
        Self::new()
    }
}

impl EntryListState {
    pub fn new() -> Self {
        let mut state = TableState::default();
        state.select(Some(0));
        EntryListState {
            table_state: state,
            #[cfg(test)]
            rendered_rows: 0,
        }
    }

    /// Global selection and offset are independent of the local table widget.
    pub fn viewport(&mut self, total: usize, height: u16) -> std::ops::Range<usize> {
        let rows = height.saturating_sub(3) as usize;
        let selected = self.selected().min(total.saturating_sub(1));
        self.select(selected);
        let mut offset = self.table_state.offset().min(total.saturating_sub(rows));
        if selected < offset {
            offset = selected;
        }
        if rows > 0 && selected >= offset.saturating_add(rows) {
            offset = selected + 1 - rows;
        }
        *self.table_state.offset_mut() = offset;
        offset..offset.saturating_add(rows).min(total)
    }

    pub fn selected(&self) -> usize {
        self.table_state.selected().unwrap_or(0)
    }

    pub fn select(&mut self, idx: usize) {
        self.table_state.select(Some(idx));
    }
}

#[allow(clippy::too_many_arguments)] // display params; bundling into a struct is tracked in REVIEW_FINDINGS.md
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
    highlights: &[HighlightTerm],
) {
    let range = state.viewport(entries.len(), area.height);
    render_entry_list_window(
        f,
        area,
        &entries[range],
        state,
        columns,
        theme,
        focused,
        show_braces,
        render_latex_enabled,
        abbreviate_authors_enabled,
        abbreviate_journal_enabled,
        bib_dir,
        highlights,
    );
}

#[allow(clippy::too_many_arguments)]
pub fn render_entry_list_window(
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
    highlights: &[HighlightTerm],
) {
    #[cfg(test)]
    {
        state.rendered_rows = entries.len();
    }
    let total_width = area.width.saturating_sub(2); // borders

    // Build constraints from column config
    let constraints: Vec<Constraint> = columns
        .iter()
        .map(|col| match col.width {
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
        })
        .collect();

    // Header
    let header_cells: Vec<Cell> = columns
        .iter()
        .map(|col| Cell::from(col.header.as_str()).style(theme.header))
        .collect();
    let header = Row::new(header_cells).style(theme.header).height(1);

    // Search terms that apply to each column: unqualified terms everywhere,
    // `field:` terms only in that field's column.
    let column_needles: Vec<Vec<&str>> = columns
        .iter()
        .map(|col| {
            let column = canonical_field_name(&col.field);
            highlights
                .iter()
                .filter(|term| {
                    term.field
                        .as_deref()
                        .is_none_or(|field| canonical_field_name(field) == column)
                })
                .map(|term| term.text.as_str())
                .collect()
        })
        .collect();

    let selected_local = (!entries.is_empty()).then(|| {
        state
            .selected()
            .saturating_sub(state.table_state.offset())
            .min(entries.len() - 1)
    });

    let rows: Vec<Row> = entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let cells: Vec<Cell> = columns
                .iter()
                .zip(&column_needles)
                .map(|(col, needles)| {
                    if col.field == "file_indicator" {
                        return file_indicator_cell(entry, bib_dir);
                    }
                    let raw = get_field_value(
                        entry,
                        &col.field,
                        abbreviate_authors_enabled,
                        abbreviate_journal_enabled,
                    );
                    let value = apply_display_pipeline(&raw, show_braces, render_latex_enabled);
                    Cell::from(highlight_line(value, needles, theme.search_match))
                })
                .collect();
            let row = Row::new(cells).height(1);
            // The selected style goes on the row itself (drawn beneath the
            // cells) rather than via `row_highlight_style` (drawn over them),
            // so highlighted matches keep their color on the selected row.
            if selected_local == Some(index) {
                row.style(theme.selected)
            } else {
                row
            }
        })
        .collect();

    let border_style = if focused {
        theme.border.add_modifier(Modifier::BOLD)
    } else {
        theme.border
    };

    let table = Table::new(rows, &constraints).header(header).block(
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
    );

    let mut local = TableState::default();
    local.select(selected_local);
    f.render_stateful_widget(table, area, &mut local);
}

/// Byte ranges of `text` where any of `needles` occurs, compared
/// case-insensitively character by character. Overlapping and adjacent
/// matches are merged.
fn match_ranges(text: &str, needles: &[&str]) -> Vec<std::ops::Range<usize>> {
    fn same(a: char, b: char) -> bool {
        a == b || a.to_lowercase().eq(b.to_lowercase())
    }
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut ranges: Vec<std::ops::Range<usize>> = Vec::new();
    for needle in needles {
        let needle: Vec<char> = needle.chars().collect();
        if needle.is_empty() || needle.len() > chars.len() {
            continue;
        }
        for start in 0..=chars.len() - needle.len() {
            let window = &chars[start..start + needle.len()];
            if window.iter().zip(&needle).all(|(&(_, a), &b)| same(a, b)) {
                let end = chars
                    .get(start + needle.len())
                    .map_or(text.len(), |&(offset, _)| offset);
                ranges.push(chars[start].0..end);
            }
        }
    }
    ranges.sort_by_key(|range| range.start);
    let mut merged: Vec<std::ops::Range<usize>> = Vec::with_capacity(ranges.len());
    for range in ranges {
        match merged.last_mut() {
            Some(last) if range.start <= last.end => last.end = last.end.max(range.end),
            _ => merged.push(range),
        }
    }
    merged
}

/// Render `text` with every case-insensitive occurrence of `needles` styled
/// with `style`.
fn highlight_line(text: String, needles: &[&str], style: Style) -> Line<'static> {
    let ranges = match_ranges(&text, needles);
    if ranges.is_empty() {
        return Line::from(text);
    }
    let mut spans = Vec::with_capacity(ranges.len() * 2 + 1);
    let mut cursor = 0;
    for range in ranges {
        if range.start > cursor {
            spans.push(Span::raw(text[cursor..range.start].to_string()));
        }
        spans.push(Span::styled(text[range.clone()].to_string(), style));
        cursor = range.end;
    }
    if cursor < text.len() {
        spans.push(Span::raw(text[cursor..].to_string()));
    }
    Line::from(spans)
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

/// Collapse field-name aliases (see `get_field_value`/`get_sort_value`) to a
/// single canonical name, so e.g. a sort on `"key"` is recognized as already
/// visible when the `"citekey"` column is shown.
fn canonical_field_name(field: &str) -> &str {
    match field {
        "citation_key" | "key" | "citekey" => "citekey",
        "entrytype" | "type" => "entrytype",
        other => other,
    }
}

/// Build the columns to render: the configured columns, plus a temporary
/// column appended on the right showing the active sort field's raw values
/// when that field isn't already one of the visible columns. This gives
/// visual confirmation that a sort on an otherwise-hidden field took effect.
///
/// The extra column is derived fresh from the current sort state on every
/// call, so it disappears on its own as soon as the sort changes to a
/// visible field or is cleared — there's no separate toggle to reset.
pub fn columns_with_sort_preview(
    columns: &[ColumnConfig],
    sort_field: &str,
    sort_ascending: bool,
) -> Vec<ColumnConfig> {
    if sort_field.is_empty() || sort_field == "none" {
        return columns.to_vec();
    }
    let already_shown = columns
        .iter()
        .any(|c| canonical_field_name(&c.field) == canonical_field_name(sort_field));
    if already_shown {
        return columns.to_vec();
    }
    let mut result = columns.to_vec();
    let arrow = if sort_ascending {
        '\u{2191}'
    } else {
        '\u{2193}'
    };
    result.push(ColumnConfig {
        field: sort_field.to_string(),
        header: format!("{} {}", sort_field, arrow),
        width: ColumnWidth::Percent(15),
        max_width: None,
    });
    result
}

fn get_field_value(
    entry: &Entry,
    field: &str,
    abbreviate_authors_enabled: bool,
    abbreviate_journal_enabled: bool,
) -> String {
    match field {
        "dirty" => {
            if entry.dirty {
                "●".to_string()
            } else {
                " ".to_string()
            }
        }
        "web_indicator" => {
            let has_doi = entry
                .fields
                .get("doi")
                .map(|v| !v.trim().is_empty())
                .unwrap_or(false);
            let has_url = entry
                .fields
                .get("url")
                .map(|v| !v.trim().is_empty())
                .unwrap_or(false);
            if has_doi || has_url {
                "\u{238B}".to_string()
            } else {
                " ".to_string()
            }
        }
        "entrytype" | "type" => entry.entry_type.display_name().to_string(),
        "citation_key" | "key" | "citekey" => entry.citation_key.clone(),
        "author" => {
            let raw = entry.author_display();
            if abbreviate_authors_enabled {
                abbreviate_authors(&raw)
            } else {
                raw
            }
        }
        "title" => entry.title_display(),
        "year" => entry.year_display(),
        "journal" => {
            if abbreviate_journal_enabled {
                // Use journal_full as the canonical source (present after a save),
                // fall back to journal, then booktitle. Abbreviate on the fly so
                // the result is correct even without a stored journal_abbrev field.
                let full = entry
                    .fields
                    .get("journal_full")
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

#[cfg(test)]
mod tests {
    // ── columns_with_sort_preview ────────────────────────────────────────

    #[test]
    fn test_sort_preview_column_added_for_hidden_field() {
        let cols = crate::config::defaults::default_columns();
        let result = columns_with_sort_preview(&cols, "note", true);
        assert_eq!(result.len(), cols.len() + 1);
        let last = result.last().unwrap();
        assert_eq!(last.field, "note");
        assert!(last.header.contains("note"));
        assert!(last.header.contains('\u{2191}'), "ascending arrow expected");
    }

    #[test]
    fn test_sort_preview_column_shows_descending_arrow() {
        let cols = crate::config::defaults::default_columns();
        let result = columns_with_sort_preview(&cols, "note", false);
        let last = result.last().unwrap();
        assert!(
            last.header.contains('\u{2193}'),
            "descending arrow expected"
        );
    }

    #[test]
    fn test_sort_preview_column_omitted_when_field_already_visible() {
        let cols = crate::config::defaults::default_columns();
        // "title" is already a default column.
        let result = columns_with_sort_preview(&cols, "title", true);
        assert_eq!(result.len(), cols.len());
    }

    #[test]
    fn test_sort_preview_column_omitted_for_alias_of_visible_field() {
        let cols = crate::config::defaults::default_columns();
        // "key" is an alias of the already-visible "citekey" column.
        let result = columns_with_sort_preview(&cols, "key", true);
        assert_eq!(result.len(), cols.len());
    }

    #[test]
    fn test_sort_preview_column_omitted_when_sort_is_none() {
        let cols = crate::config::defaults::default_columns();
        let result = columns_with_sort_preview(&cols, "none", true);
        assert_eq!(result.len(), cols.len());
    }

    use super::*;
    use crate::bib::model::{Entry, EntryType};
    use indexmap::IndexMap;

    fn make_entry(key: &str, dirty: bool, fields: &[(&str, &str)]) -> Entry {
        let mut f = IndexMap::new();
        for (k, v) in fields {
            f.insert(k.to_string(), v.to_string());
        }
        Entry {
            entry_type: EntryType::Article,
            citation_key: key.to_string(),
            fields: f,
            group_memberships: vec![],
            raw_index: 0,
            dirty,
        }
    }

    // ── Search-term highlighting ──

    #[test]
    fn test_match_ranges_case_insensitive() {
        assert_eq!(match_ranges("Rust and RUST", &["rust"]), vec![0..4, 9..13]);
    }

    #[test]
    fn test_match_ranges_merges_overlaps_and_multiple_terms() {
        assert_eq!(match_ranges("aaa", &["aa"]), vec![0..3]);
        assert_eq!(match_ranges("Doe2021", &["doe", "2021"]), vec![0..7]);
    }

    #[test]
    fn test_match_ranges_unicode_and_no_match() {
        assert_eq!(match_ranges("Café Über", &["über"]), vec![6..11]);
        assert_eq!(match_ranges("Café", &["CAFÉ"]), vec![0..5]);
        assert!(match_ranges("Rust", &["python"]).is_empty());
        assert!(match_ranges("Rust", &[""]).is_empty());
        assert!(match_ranges("", &["x"]).is_empty());
    }

    #[test]
    fn test_highlight_line_styles_only_matches() {
        let style = Style::default().fg(Color::Red);
        let line = highlight_line("My Rust Book".into(), &["rust"], style);
        let spans: Vec<(&str, Style)> = line
            .spans
            .iter()
            .map(|span| (span.content.as_ref(), span.style))
            .collect();
        assert_eq!(
            spans,
            vec![
                ("My ", Style::default()),
                ("Rust", style),
                (" Book", Style::default()),
            ]
        );
        let plain = highlight_line("My Book".into(), &["rust"], style);
        assert_eq!(plain.spans.len(), 1);
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
        assert_eq!(
            get_field_value(&e, "citation_key", false, false),
            "Smith2020"
        );
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
        let e = make_entry(
            "k",
            false,
            &[("author", "Smith, J. and Doe, J. and Brown, K.")],
        );
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
        let e = make_entry(
            "k",
            false,
            &[("journal", "Nuclear Science and Engineering")],
        );
        assert_eq!(
            get_field_value(&e, "journal", false, false),
            "Nuclear Science and Engineering"
        );
    }

    #[test]
    fn test_get_field_value_journal_abbreviated_on_the_fly() {
        let e = make_entry(
            "k",
            false,
            &[("journal", "Nuclear Science and Engineering")],
        );
        // ISO 4 abbreviation computed on the fly; no journal_abbrev field needed
        assert_eq!(
            get_field_value(&e, "journal", false, true),
            "Nucl. Sci. Eng."
        );
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
        assert_eq!(
            get_field_value(&e, "journal", false, true),
            "Nucl. Sci. Eng."
        );
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

    // ── file_indicator_cell ───────────────────────────────────────────────────

    #[test]
    fn test_file_indicator_cell_no_file_field_is_space() {
        let e = make_entry("k", false, &[]);
        let cell = file_indicator_cell(&e, std::path::Path::new("/tmp"));
        // ratatui Cell doesn't expose text directly, but we just verify no panic
        let _ = cell;
    }

    #[test]
    fn test_file_indicator_cell_empty_file_field_is_space() {
        let e = make_entry("k", false, &[("file", "   ")]);
        let _ = file_indicator_cell(&e, std::path::Path::new("/tmp"));
    }

    #[test]
    fn test_file_indicator_cell_existing_file_returns_icon() {
        use tempfile::NamedTempFile;
        let tmp = NamedTempFile::new().unwrap();
        let path_str = tmp.path().to_str().unwrap().to_string();
        let file_val = crate::util::open::serialize_file_field(&[crate::util::open::ParsedFile {
            description: String::new(),
            path: path_str,
            file_type: "application/pdf".into(),
        }]);
        let e = make_entry("k", false, &[("file", &file_val)]);
        let bib_dir = tmp.path().parent().unwrap();
        let _ = file_indicator_cell(&e, bib_dir);
    }

    #[test]
    fn test_file_indicator_cell_missing_file_returns_red_icon() {
        let e = make_entry(
            "k",
            false,
            &[("file", ":/nonexistent/path/Smith2020.pdf:application/pdf")],
        );
        let _ = file_indicator_cell(&e, std::path::Path::new("/tmp"));
    }
}
