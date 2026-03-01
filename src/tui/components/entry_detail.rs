use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;

use crate::bib::entry_types::fields_for_type;
use crate::bib::model::Entry;
use crate::tui::theme::Theme;
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
}

pub struct EntryDetailState {
    pub list_state: ListState,
    /// All display rows including category headers.
    /// The list_state selection index into this vec.
    pub display_fields: Vec<DisplayItem>,
}

impl EntryDetailState {
    pub fn new(entry: &Entry) -> Self {
        let display_fields = build_display_items(entry);
        let mut state = ListState::default();
        // Start on the first selectable (non-header) item
        let first = display_fields.iter().position(|i| matches!(i, DisplayItem::Field { .. }));
        state.select(first);
        EntryDetailState {
            list_state: state,
            display_fields,
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
        self.display_fields = build_display_items(entry);
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

fn build_display_items(entry: &Entry) -> Vec<DisplayItem> {
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

    let other_fields: Vec<(String, String)> = entry
        .fields
        .iter()
        .filter(|(k, _)| {
            let kl = k.to_lowercase();
            !required.contains(&kl.as_str()) && !optional.contains(&kl.as_str())
        })
        .map(|(k, v)| (k.clone(), v.clone()))
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

pub fn render_entry_detail(
    f: &mut Frame,
    area: Rect,
    entry: &Entry,
    state: &mut EntryDetailState,
    theme: &Theme,
    show_braces: bool,
) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.border)
        .title(format!(" {} ", entry.citation_key))
        .title_bottom(
            Line::from(" [e]dit  [a]dd  [d]el  [T]itlecase  [o]pen file  [w]eb  [g]roups  [c]itekey  [Esc] back ")
                .style(theme.label),
        );

    let inner = block.inner(area);
    f.render_widget(block, area);

    // Type line at top
    let type_line = Line::from(vec![
        Span::styled("  Type: ", theme.label),
        Span::styled(entry.entry_type.display_name(), theme.value),
    ]);

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
                    FieldCategory::Optional | FieldCategory::Other => theme.label,
                };
                let value_style = if value.is_empty() && *category == FieldCategory::Required {
                    theme.search_match.add_modifier(Modifier::DIM)
                } else {
                    theme.value
                };
                let display_value: String = if value.is_empty() {
                    "·".to_string()
                } else if show_braces {
                    value.clone()
                } else {
                    strip_case_braces(value)
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

    let type_para = Paragraph::new(vec![type_line, Line::from("")]);
    f.render_widget(type_para, chunks[0]);

    let list = List::new(items).highlight_style(theme.selected);
    f.render_stateful_widget(list, chunks[1], &mut state.list_state);

    // Preview pane: show full value of selected field with wrapping
    let (preview_label, preview_text) = match state.selected_field() {
        Some((name, value)) if !value.is_empty() => {
            let text = if show_braces {
                value.to_string()
            } else {
                strip_case_braces(value)
            };
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
