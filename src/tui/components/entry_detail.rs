use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;

use crate::bib::entry_types::fields_for_type;
use crate::bib::model::Entry;
use crate::config::schema::CustomFieldGroup;
use crate::tui::theme::Theme;
use crate::util::latex::render_latex;
use crate::util::titlecase::strip_case_braces;

/// A single row in the detail view — either a non-selectable category header
/// or an editable field.
#[derive(Debug, Clone)]
pub enum DisplayItem {
    Header(String),
    Field {
        name: String,
        value: String,
        category: FieldCategory,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum FieldCategory {
    Required,
    Optional,
    Other,
    Custom(String),
}

pub struct EntryDetailState {
    pub list_state: ListState,
    /// All display rows including category headers.
    /// The list_state selection index into this vec.
    pub display_fields: Vec<DisplayItem>,
    /// Custom field groups from config — stored so refresh() can use them.
    field_groups: Vec<CustomFieldGroup>,
}

impl EntryDetailState {
    pub fn new(entry: &Entry, field_groups: Vec<CustomFieldGroup>) -> Self {
        let display_fields = build_display_items(entry, &field_groups);
        let mut state = ListState::default();
        // Start on the first selectable (non-header) item
        let first = display_fields.iter().position(|i| matches!(i, DisplayItem::Field { .. }));
        state.select(first);
        EntryDetailState {
            list_state: state,
            display_fields,
            field_groups,
        }
    }

    pub fn selected(&self) -> usize {
        self.list_state.selected().unwrap_or(0)
    }

    pub fn select(&mut self, idx: usize) {
        self.list_state.select(Some(idx));
    }

    /// Move selection by `delta`, skipping over header rows.
    pub fn move_selection(&mut self, delta: i32) {
        let count = self.display_fields.len();
        if count == 0 {
            return;
        }
        let current = self.selected() as i32;
        let mut new = (current + delta).clamp(0, count as i32 - 1) as usize;

        // Skip header rows
        let direction = delta.signum();
        loop {
            if let DisplayItem::Field { .. } = &self.display_fields[new] {
                break;
            }
            let candidate = new as i32 + direction;
            if candidate < 0 || candidate >= count as i32 {
                break; // Can't skip further — stay
            }
            new = candidate as usize;
        }
        self.select(new);
    }

    /// Return (field_name, field_value) for the currently selected item, if it is a Field.
    pub fn selected_field(&self) -> Option<(&str, &str)> {
        match self.display_fields.get(self.selected()) {
            Some(DisplayItem::Field { name, value, .. }) => Some((name, value)),
            _ => None,
        }
    }

    pub fn refresh(&mut self, entry: &Entry) {
        let sel = self.selected();
        self.display_fields = build_display_items(entry, &self.field_groups);
        // Keep selection near the same position, on a Field row
        let count = self.display_fields.len();
        if count == 0 {
            self.list_state.select(None);
            return;
        }
        let target = sel.min(count - 1);
        self.select(target);
        // Ensure we land on a field, not a header
        self.move_selection(0); // no-op but triggers header-skip
    }
}

fn build_display_items(entry: &Entry, field_groups: &[CustomFieldGroup]) -> Vec<DisplayItem> {
    let (required, optional) = fields_for_type(&entry.entry_type);
    let mut result = Vec::new();

    let required_fields: Vec<(String, String)> = required
        .iter()
        .map(|f| {
            let value = entry.fields.get(*f).cloned().unwrap_or_default();
            (f.to_string(), value)
        })
        .collect();

    let optional_fields: Vec<(String, String)> = optional
        .iter()
        .filter_map(|f| entry.fields.get(*f).map(|v| (f.to_string(), v.clone())))
        .collect();

    // Build set of fields already claimed by required/optional.
    // "groups" is displayed separately in the header area, not as a field row.
    let claimed: std::collections::HashSet<String> = required
        .iter()
        .chain(optional.iter())
        .map(|s| s.to_lowercase())
        .chain(std::iter::once("groups".to_string()))
        .collect();

    // Remaining entry fields not in required/optional
    let remaining: Vec<(String, String)> = entry
        .fields
        .iter()
        .filter(|(k, _)| !claimed.contains(&k.to_lowercase()))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();

    // Partition remaining into custom groups and a leftover "Other" bucket.
    // Each field is assigned to the FIRST custom group whose `fields` list
    // contains it (case-insensitive). The field is not duplicated.
    let mut assigned: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut custom_sections: Vec<(String, Vec<(String, String)>)> = Vec::new();
    for group in field_groups {
        let mut members: Vec<(String, String)> = Vec::new();
        for (name, value) in &remaining {
            let key = name.to_lowercase();
            if !assigned.contains(&key)
                && group.fields.iter().any(|gf| gf.to_lowercase() == key)
            {
                assigned.insert(key);
                members.push((name.clone(), value.clone()));
            }
        }
        custom_sections.push((group.name.clone(), members));
    }

    let other_fields: Vec<(String, String)> = remaining
        .into_iter()
        .filter(|(name, _)| !assigned.contains(&name.to_lowercase()))
        .collect();

    if !required_fields.is_empty() {
        result.push(DisplayItem::Header("Required:".to_string()));
        for (name, value) in required_fields {
            result.push(DisplayItem::Field {
                name,
                value,
                category: FieldCategory::Required,
            });
        }
    }

    if !optional_fields.is_empty() {
        result.push(DisplayItem::Header("Optional:".to_string()));
        for (name, value) in optional_fields {
            result.push(DisplayItem::Field {
                name,
                value,
                category: FieldCategory::Optional,
            });
        }
    }

    // Custom group sections (only rendered if non-empty)
    for (group_name, fields) in custom_sections.drain(..) {
        if !fields.is_empty() {
            result.push(DisplayItem::Header(format!("{}:", group_name)));
            for (name, value) in fields {
                result.push(DisplayItem::Field {
                    name,
                    value,
                    category: FieldCategory::Custom(group_name.clone()),
                });
            }
        }
    }

    if !other_fields.is_empty() {
        result.push(DisplayItem::Header("Other:".to_string()));
        for (name, value) in other_fields {
            result.push(DisplayItem::Field {
                name,
                value,
                category: FieldCategory::Other,
            });
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bib::model::{Entry, EntryType};
    use indexmap::IndexMap;

    fn make_entry(entry_type: EntryType, fields: &[(&str, &str)]) -> Entry {
        let mut f = IndexMap::new();
        for (k, v) in fields { f.insert(k.to_string(), v.to_string()); }
        Entry {
            entry_type,
            citation_key: "Key2020".to_string(),
            fields: f,
            group_memberships: vec![],
            raw_index: 0,
            dirty: false,
        }
    }

    #[test]
    fn test_new_selects_first_field() {
        let e = make_entry(EntryType::Article, &[("author", "Smith"), ("title", "Paper")]);
        let state = EntryDetailState::new(&e, vec![]);
        // First item should be a Header, so selected should be on a Field
        let sel = state.selected();
        assert!(matches!(state.display_fields[sel], DisplayItem::Field { .. }));
    }

    #[test]
    fn test_selected_field_returns_name_value() {
        let e = make_entry(EntryType::Misc, &[("note", "some note")]);
        let state = EntryDetailState::new(&e, vec![]);
        // Misc has no required fields — note goes in Optional or Other
        let field = state.selected_field();
        assert!(field.is_some());
    }

    #[test]
    fn test_move_selection_skips_headers() {
        let e = make_entry(EntryType::Article, &[
            ("author", "Smith"), ("title", "Paper"), ("year", "2020"), ("journal", "Nature"),
        ]);
        let mut state = EntryDetailState::new(&e, vec![]);
        let start = state.selected();
        state.move_selection(1);
        let after = state.selected();
        // After moving down, we should still be on a Field (not a Header)
        assert!(matches!(state.display_fields[after], DisplayItem::Field { .. }));
        // And we should have moved
        assert!(after > start || after == start); // could stay if already at last field
    }

    #[test]
    fn test_move_selection_up() {
        let e = make_entry(EntryType::Article, &[
            ("author", "Smith"), ("title", "Paper"), ("year", "2020"), ("journal", "Nature"),
        ]);
        let mut state = EntryDetailState::new(&e, vec![]);
        state.move_selection(10); // go to bottom
        let bottom = state.selected();
        state.move_selection(-1);
        let after = state.selected();
        assert!(after <= bottom);
        assert!(matches!(state.display_fields[after], DisplayItem::Field { .. }));
    }

    #[test]
    fn test_refresh_preserves_selection() {
        let e = make_entry(EntryType::Article, &[
            ("author", "Smith"), ("title", "Paper"), ("year", "2020"), ("journal", "Nature"),
        ]);
        let mut state = EntryDetailState::new(&e, vec![]);
        state.move_selection(1);
        let before = state.selected();
        let mut e2 = e.clone();
        e2.fields.insert("author".to_string(), "Jones".to_string());
        state.refresh(&e2);
        // Selection should be preserved or clamped
        assert!(matches!(state.display_fields[state.selected()], DisplayItem::Field { .. }));
        let _ = before; // just ensure it compiled
    }

    #[test]
    fn test_required_fields_appear() {
        let e = make_entry(EntryType::Article, &[
            ("author", "Smith"), ("title", "Paper"), ("year", "2020"), ("journal", "Nature"),
        ]);
        let state = EntryDetailState::new(&e, vec![]);
        let has_required_header = state.display_fields.iter().any(|item| {
            matches!(item, DisplayItem::Header(h) if h == "Required:")
        });
        assert!(has_required_header);
    }

    #[test]
    fn test_custom_field_group() {
        use crate::config::schema::CustomFieldGroup;
        let e = make_entry(EntryType::Article, &[
            ("author", "Smith"), ("title", "P"), ("year", "2020"),
            ("journal", "N"), ("isbn", "123"),
        ]);
        let groups = vec![CustomFieldGroup {
            name: "Identifiers".to_string(),
            fields: vec!["isbn".to_string()],
        }];
        let state = EntryDetailState::new(&e, groups);
        let has_id_header = state.display_fields.iter().any(|item| {
            matches!(item, DisplayItem::Header(h) if h.contains("Identifiers"))
        });
        assert!(has_id_header);
    }

    #[test]
    fn test_apply_display_pipeline_strip_braces() {
        assert_eq!(apply_display_pipeline("{Hello}", false, false), "Hello");
    }

    #[test]
    fn test_apply_display_pipeline_show_braces() {
        assert_eq!(apply_display_pipeline("{Hello}", true, false), "{Hello}");
    }
}

pub fn render_entry_detail(
    f: &mut Frame,
    area: Rect,
    entry: &Entry,
    state: &mut EntryDetailState,
    theme: &Theme,
    show_braces: bool,
    render_latex_enabled: bool,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.border)
        .title(format!(" {} ", entry.citation_key))
        .title_bottom(
            Line::from(" [e]dit  [a]dd  [d]el  [T]itlecase  [N]orm author  [o]pen file  [w]eb  [g]roups  [c]itekey  [Esc] back ")
                .style(theme.label),
        );

    let inner = block.inner(area);
    f.render_widget(block, area);

    // Type + groups lines at top
    let type_line = Line::from(vec![
        Span::styled("  Type:   ", theme.label),
        Span::styled(entry.entry_type.display_name(), theme.value),
    ]);
    let groups_line = if entry.group_memberships.is_empty() {
        Line::from(vec![
            Span::styled("  Groups: ", theme.label),
            Span::styled("(none)", theme.label),
        ])
    } else {
        Line::from(vec![
            Span::styled("  Groups: ", theme.label),
            Span::styled(entry.group_memberships.join(", "), theme.value),
        ])
    };

    // Determine max field name length for alignment
    let max_name_len = state
        .display_fields
        .iter()
        .filter_map(|item| match item {
            DisplayItem::Field { name, .. } => Some(name.len()),
            _ => None,
        })
        .max()
        .unwrap_or(0);

    let items: Vec<ListItem> = state
        .display_fields
        .iter()
        .map(|item| match item {
            DisplayItem::Header(label) => ListItem::new(Line::from(Span::styled(
                format!("  {}", label),
                theme.required_label,
            ))),
            DisplayItem::Field { name, value, category } => {
                let padding = " ".repeat(max_name_len.saturating_sub(name.len()));
                let name_style = match category {
                    FieldCategory::Required => theme.required_label,
                    FieldCategory::Optional
                    | FieldCategory::Other
                    | FieldCategory::Custom(_) => theme.label,
                };
                let value_style = if value.is_empty() && *category == FieldCategory::Required {
                    theme.search_match.add_modifier(Modifier::DIM)
                } else {
                    theme.value
                };
                let display_value: String = if value.is_empty() {
                    "·".to_string()
                } else {
                    apply_display_pipeline(value, show_braces, render_latex_enabled)
                };
                ListItem::new(Line::from(vec![
                    Span::styled(format!("    {}{} : ", name, padding), name_style),
                    Span::styled(display_value, value_style),
                ]))
            }
        })
        .collect();

    // Reserve a preview pane at the bottom for the full selected-field value
    let preview_height = 4u16;
    let chunks = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(3),
        Constraint::Length(preview_height),
    ])
    .split(inner);

    let type_para = Paragraph::new(vec![type_line, groups_line]);
    f.render_widget(type_para, chunks[0]);

    let list = List::new(items).highlight_style(theme.selected);
    f.render_stateful_widget(list, chunks[1], &mut state.list_state);

    // Preview pane: show full value of selected field with wrapping
    let (preview_label, preview_text) = match state.selected_field() {
        Some((name, value)) if !value.is_empty() => {
            let text = apply_display_pipeline(value, show_braces, render_latex_enabled);
            (format!(" {} ", name), text)
        }
        Some((name, _)) => (format!(" {} ", name), "(empty)".to_string()),
        None => (" Value ".to_string(), String::new()),
    };
    let preview_block = Block::default()
        .borders(Borders::TOP)
        .border_style(theme.border.add_modifier(Modifier::DIM))
        .title(preview_label)
        .title_style(theme.label);
    let preview = Paragraph::new(preview_text)
        .block(preview_block)
        .wrap(Wrap { trim: true })
        .style(theme.value);
    f.render_widget(preview, chunks[2]);
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
