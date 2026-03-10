use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use crate::tui::theme::Theme;

#[derive(Debug, Clone)]
pub enum DialogKind {
    Confirm {
        title: String,
        message: String,
    },
    TypePicker {
        title: String,
        options: Vec<String>,
    },
    GroupAssign {
        groups: Vec<(String, bool)>,
    },
    /// Scrollable preview of filename renames that sync_filenames will perform.
    /// Each entry is (old_filename, new_filename).
    FileSyncPreview {
        renames: Vec<(String, String)>,
    },
}

pub struct DialogState {
    pub kind: DialogKind,
    pub list_state: ListState,
}

impl DialogState {
    pub fn confirm(title: &str, message: &str) -> Self {
        let mut state = ListState::default();
        state.select(Some(0));
        DialogState {
            kind: DialogKind::Confirm {
                title: title.to_string(),
                message: message.to_string(),
            },
            list_state: state,
        }
    }

    pub fn type_picker(options: Vec<String>) -> Self {
        Self::type_picker_titled("Select Entry Type", options)
    }

    pub fn type_picker_titled(title: &str, options: Vec<String>) -> Self {
        let mut state = ListState::default();
        state.select(Some(0));
        DialogState {
            kind: DialogKind::TypePicker {
                title: title.to_string(),
                options,
            },
            list_state: state,
        }
    }

    pub fn file_sync_preview(renames: Vec<(String, String)>) -> Self {
        let mut state = ListState::default();
        if !renames.is_empty() {
            state.select(Some(0));
        }
        DialogState {
            kind: DialogKind::FileSyncPreview { renames },
            list_state: state,
        }
    }

    pub fn group_assign(groups: Vec<(String, bool)>) -> Self {
        let mut state = ListState::default();
        if !groups.is_empty() {
            state.select(Some(0));
        }
        DialogState {
            kind: DialogKind::GroupAssign { groups },
            list_state: state,
        }
    }

    pub fn selected(&self) -> usize {
        self.list_state.selected().unwrap_or(0)
    }

    pub fn select(&mut self, idx: usize) {
        self.list_state.select(Some(idx));
    }

    pub fn option_count(&self) -> usize {
        match &self.kind {
            DialogKind::Confirm { .. } => 2,
            DialogKind::TypePicker { options, .. } => options.len(),
            DialogKind::GroupAssign { groups } => groups.len(),
            DialogKind::FileSyncPreview { renames } => renames.len(),
        }
    }

    pub fn toggle_selected(&mut self) {
        if let DialogKind::GroupAssign { groups } = &mut self.kind {
            let idx = self.list_state.selected().unwrap_or(0);
            if let Some((_, checked)) = groups.get_mut(idx) {
                *checked = !*checked;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_confirm_option_count() {
        let d = DialogState::confirm("Title", "Are you sure?");
        assert_eq!(d.option_count(), 2);
        assert_eq!(d.selected(), 0);
    }

    #[test]
    fn test_type_picker_option_count() {
        let d = DialogState::type_picker(vec!["Article".into(), "Book".into(), "Misc".into()]);
        assert_eq!(d.option_count(), 3);
        assert_eq!(d.selected(), 0);
    }

    #[test]
    fn test_type_picker_titled() {
        let d = DialogState::type_picker_titled("Pick Type", vec!["A".into(), "B".into()]);
        assert_eq!(d.option_count(), 2);
        if let DialogKind::TypePicker { title, .. } = &d.kind {
            assert_eq!(title, "Pick Type");
        } else {
            panic!("wrong kind");
        }
    }

    #[test]
    fn test_group_assign_option_count() {
        let groups = vec![
            ("Physics".into(), false),
            ("Chemistry".into(), true),
        ];
        let d = DialogState::group_assign(groups);
        assert_eq!(d.option_count(), 2);
        assert_eq!(d.selected(), 0);
    }

    #[test]
    fn test_group_assign_empty() {
        let d = DialogState::group_assign(vec![]);
        assert_eq!(d.option_count(), 0);
    }

    #[test]
    fn test_select() {
        let mut d = DialogState::type_picker(vec!["A".into(), "B".into(), "C".into()]);
        d.select(2);
        assert_eq!(d.selected(), 2);
    }

    #[test]
    fn test_toggle_selected_group_assign() {
        let groups = vec![("Physics".into(), false), ("Chemistry".into(), false)];
        let mut d = DialogState::group_assign(groups);
        d.toggle_selected();
        if let DialogKind::GroupAssign { groups } = &d.kind {
            assert!(groups[0].1); // first toggled to true
            assert!(!groups[1].1);
        }
    }

    #[test]
    fn test_toggle_selected_confirm_is_noop() {
        let mut d = DialogState::confirm("T", "M");
        d.toggle_selected(); // should not panic
    }

    #[test]
    fn test_toggle_selected_type_picker_is_noop() {
        let mut d = DialogState::type_picker(vec!["A".into()]);
        d.toggle_selected(); // should not panic
    }
}

/// Truncate a string to at most `max` display columns, appending `…` when cut.
fn truncate_to(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    if max == 0 {
        return String::new();
    }
    let truncated: String = s.chars().take(max.saturating_sub(1)).collect();
    format!("{}\u{2026}", truncated) // …
}

/// Fit (old, new) filename strings into `budget` total display columns.
/// Each name gets up to half the budget; if one is shorter the other gets
/// the remainder.
fn fit_rename(old: &str, new: &str, budget: usize) -> (String, String) {
    let old_len = old.chars().count();
    let new_len = new.chars().count();
    if old_len + new_len <= budget {
        return (old.to_string(), new.to_string());
    }
    let half = budget / 2;
    // Allocate: old gets up to half, new takes the rest.
    let old_alloc = half.min(old_len);
    let new_alloc = budget.saturating_sub(old_alloc).min(new_len);
    // If new is short, give unused space back to old.
    let old_alloc = budget.saturating_sub(new_alloc).min(old_len);
    (truncate_to(old, old_alloc), truncate_to(new, new_alloc))
}

pub fn render_dialog(f: &mut Frame, area: Rect, state: &mut DialogState, theme: &Theme) {
    let dialog_width = match &state.kind {
        DialogKind::FileSyncPreview { renames } => {
            // Grow to fit the widest "  old → new" line, then cap at terminal width.
            // "  " (2) + old + " → " (3) + new + 2 borders = content + 7
            let widest = renames
                .iter()
                .map(|(old, new)| old.chars().count() + new.chars().count() + 7)
                .max()
                .unwrap_or(30);
            (widest as u16).max(44).min(area.width.saturating_sub(4))
        }
        DialogKind::GroupAssign { .. } => 50u16.min(area.width.saturating_sub(4)),
        _ => 40u16.min(area.width.saturating_sub(4)),
    };
    let dialog_height = match &state.kind {
        DialogKind::Confirm { .. } => 5,
        DialogKind::TypePicker { options, .. } => {
            (options.len() as u16 + 4).min(area.height.saturating_sub(4))
        }
        DialogKind::GroupAssign { groups } => {
            (groups.len() as u16 + 5).min(area.height.saturating_sub(4))
        }
        DialogKind::FileSyncPreview { renames } => {
            (renames.len() as u16 + 4).min(area.height.saturating_sub(4))
        }
    };

    let x = area.x + (area.width.saturating_sub(dialog_width)) / 2;
    let y = area.y + (area.height.saturating_sub(dialog_height)) / 2;
    let dialog_area = Rect::new(x, y, dialog_width, dialog_height);

    f.render_widget(Clear, dialog_area);

    match &state.kind {
        DialogKind::Confirm { title, message } => {
            let block = Block::default()
                .borders(Borders::ALL)
                .border_style(theme.border)
                .title(format!(" {} ", title));

            let inner = block.inner(dialog_area);
            f.render_widget(block, dialog_area);

            let lines = vec![
                Line::from(message.as_str()),
                Line::from(""),
                Line::from(vec![
                    Span::styled("[y]es", theme.search_match),
                    Span::raw("  "),
                    Span::styled("[n]o", theme.label),
                ]),
            ];
            let para = Paragraph::new(lines);
            f.render_widget(para, inner);
        }
        DialogKind::TypePicker { title, options } => {
            let block = Block::default()
                .borders(Borders::ALL)
                .border_style(theme.border)
                .title(format!(" {} ", title));

            let items: Vec<ListItem> = options
                .iter()
                .map(|opt| ListItem::new(Line::from(format!("  {}", opt))))
                .collect();

            let list = List::new(items)
                .block(block)
                .highlight_style(theme.selected);

            f.render_stateful_widget(list, dialog_area, &mut state.list_state);
        }
        DialogKind::GroupAssign { groups } => {
            let block = Block::default()
                .borders(Borders::ALL)
                .border_style(theme.border)
                .title(" Assign Groups ")
                .title_bottom(Line::from(Span::styled(
                    " Space:toggle  Enter:confirm  Esc:cancel ",
                    theme.label,
                )));

            let items: Vec<ListItem> = groups
                .iter()
                .map(|(name, checked)| {
                    let mark = if *checked { "[x]" } else { "[ ]" };
                    ListItem::new(Line::from(format!("  {} {}", mark, name)))
                })
                .collect();

            let list = List::new(items)
                .block(block)
                .highlight_style(theme.selected);

            f.render_stateful_widget(list, dialog_area, &mut state.list_state);
        }
        DialogKind::FileSyncPreview { renames } => {
            let block = Block::default()
                .borders(Borders::ALL)
                .border_style(theme.border)
                .title(" Filename Sync — files to be renamed ")
                .title_bottom(Line::from(vec![
                    Span::raw(" "),
                    Span::styled("[y]es", theme.search_match),
                    Span::raw(" — proceed  "),
                    Span::styled("[n]o", theme.label),
                    Span::raw(" — cancel "),
                ]));

            // inner_w = dialog width minus the two border columns.
            // Each line is "  old → new"; overhead = 2 spaces + " → " = 5 chars.
            let inner_w = dialog_area.width.saturating_sub(2) as usize;
            let name_budget = inner_w.saturating_sub(5);

            let items: Vec<ListItem> = renames
                .iter()
                .map(|(old, new)| {
                    let (old_s, new_s) = fit_rename(old, new, name_budget);
                    ListItem::new(Line::from(format!("  {} \u{2192} {}", old_s, new_s)))
                })
                .collect();

            let list = List::new(items)
                .block(block)
                .highlight_style(theme.selected);

            f.render_stateful_widget(list, dialog_area, &mut state.list_state);
        }
    }
}
