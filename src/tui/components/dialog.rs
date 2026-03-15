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
    /// Multi-select: choose which attached files to delete alongside an entry.
    /// Each item is (display_label, delete_this_file).
    FileDeleteSelect {
        title: String,
        files: Vec<(String, bool)>,
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

    pub fn file_delete_select(title: &str, files: Vec<(String, bool)>) -> Self {
        let mut state = ListState::default();
        if !files.is_empty() {
            state.select(Some(0));
        }
        DialogState {
            kind: DialogKind::FileDeleteSelect { title: title.to_string(), files },
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
            DialogKind::FileDeleteSelect { files, .. } => files.len(),
        }
    }

    pub fn toggle_selected(&mut self) {
        let idx = self.list_state.selected().unwrap_or(0);
        match &mut self.kind {
            DialogKind::GroupAssign { groups } => {
                if let Some((_, checked)) = groups.get_mut(idx) {
                    *checked = !*checked;
                }
            }
            DialogKind::FileDeleteSelect { files, .. } => {
                if let Some((_, checked)) = files.get_mut(idx) {
                    *checked = !*checked;
                }
            }
            _ => {}
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

    // ── file_delete_select ──

    #[test]
    fn test_file_delete_select_option_count() {
        let d = DialogState::file_delete_select(
            "Delete 'Smith2020'",
            vec![
                ("Smith2020.pdf".into(), true),
                ("Smith2020_supp.pdf".into(), true),
            ],
        );
        assert_eq!(d.option_count(), 2);
        assert_eq!(d.selected(), 0);
        assert!(matches!(d.kind, DialogKind::FileDeleteSelect { .. }));
    }

    #[test]
    fn test_file_delete_select_empty() {
        let d = DialogState::file_delete_select("Delete 'X'", vec![]);
        assert_eq!(d.option_count(), 0);
    }

    #[test]
    fn test_file_delete_select_toggle() {
        let mut d = DialogState::file_delete_select(
            "Delete 'X'",
            vec![("a.pdf".into(), true), ("b.pdf".into(), true)],
        );
        d.select(1);
        d.toggle_selected(); // uncheck second
        if let DialogKind::FileDeleteSelect { files, .. } = &d.kind {
            assert!(files[0].1, "first still checked");
            assert!(!files[1].1, "second unchecked");
        } else {
            panic!("wrong kind");
        }
    }

    #[test]
    fn test_file_delete_select_toggle_back() {
        let mut d = DialogState::file_delete_select(
            "Delete 'X'",
            vec![("a.pdf".into(), false)],
        );
        d.toggle_selected();
        if let DialogKind::FileDeleteSelect { files, .. } = &d.kind {
            assert!(files[0].1, "should toggle from false to true");
        }
    }

    #[test]
    fn test_file_delete_select_title() {
        let d = DialogState::file_delete_select("Delete 'Foo'", vec![("x.pdf".into(), true)]);
        if let DialogKind::FileDeleteSelect { title, .. } = &d.kind {
            assert_eq!(title, "Delete 'Foo'");
        } else {
            panic!("wrong kind");
        }
    }

    // ── file_sync_preview ──

    #[test]
    fn test_file_sync_preview_option_count() {
        let d = DialogState::file_sync_preview(vec![
            ("old.pdf".to_string(), "Smith2020.pdf".to_string()),
            ("fig.png".to_string(), "Smith2020_2.png".to_string()),
        ]);
        assert_eq!(d.option_count(), 2);
        assert_eq!(d.selected(), 0);
        assert!(matches!(d.kind, DialogKind::FileSyncPreview { .. }));
    }

    #[test]
    fn test_file_sync_preview_empty() {
        let d = DialogState::file_sync_preview(vec![]);
        assert_eq!(d.option_count(), 0);
    }

    #[test]
    fn test_file_sync_preview_toggle_is_noop() {
        let mut d = DialogState::file_sync_preview(vec![
            ("a.pdf".to_string(), "b.pdf".to_string()),
        ]);
        d.toggle_selected(); // should not panic
    }

    // ── toggle_selected on GroupAssign cycling ──

    #[test]
    fn test_toggle_selected_second_item() {
        let groups = vec![
            ("Physics".into(), false),
            ("Chemistry".into(), false),
        ];
        let mut d = DialogState::group_assign(groups);
        d.select(1);
        d.toggle_selected();
        if let DialogKind::GroupAssign { groups } = &d.kind {
            assert!(!groups[0].1);
            assert!(groups[1].1);
        } else {
            panic!("wrong kind");
        }
    }

    #[test]
    fn test_toggle_selected_toggles_back() {
        let groups = vec![("Physics".into(), true)];
        let mut d = DialogState::group_assign(groups);
        d.toggle_selected();
        if let DialogKind::GroupAssign { groups } = &d.kind {
            assert!(!groups[0].1, "should toggle from true to false");
        }
    }

    // ── select / selected edge cases ──

    #[test]
    fn test_selected_default_is_zero() {
        let d = DialogState::confirm("T", "M");
        assert_eq!(d.selected(), 0);
    }

    #[test]
    fn test_select_and_selected_roundtrip() {
        let mut d = DialogState::type_picker(vec!["A".into(), "B".into(), "C".into(), "D".into()]);
        for i in 0..4 {
            d.select(i);
            assert_eq!(d.selected(), i);
        }
    }

    // ── truncate_to ──

    #[test]
    fn test_truncate_to_fits() {
        assert_eq!(truncate_to("hello", 10), "hello");
        assert_eq!(truncate_to("hello", 5), "hello");
    }

    #[test]
    fn test_truncate_to_truncates_with_ellipsis() {
        let r = truncate_to("hello world", 6);
        assert!(r.ends_with('\u{2026}'));
        assert!(r.chars().count() <= 6);
    }

    #[test]
    fn test_truncate_to_zero() {
        assert_eq!(truncate_to("hello", 0), "");
    }

    #[test]
    fn test_truncate_to_one_gives_ellipsis() {
        assert_eq!(truncate_to("hello", 1), "\u{2026}");
    }

    #[test]
    fn test_truncate_to_empty_string() {
        assert_eq!(truncate_to("", 5), "");
    }

    // ── fit_rename ──

    #[test]
    fn test_fit_rename_both_fit() {
        let (old, new) = fit_rename("old.pdf", "new.pdf", 30);
        assert_eq!(old, "old.pdf");
        assert_eq!(new, "new.pdf");
    }

    #[test]
    fn test_fit_rename_truncates_long_names() {
        let long = "a".repeat(50);
        let (old, new) = fit_rename(&long, &long, 20);
        assert!(old.chars().count() <= 11); // half(10) + ellipsis
        assert!(new.chars().count() <= 11);
    }

    #[test]
    fn test_fit_rename_short_old_gives_new_more_budget() {
        // old is 5 chars; budget=20 → new should get up to 15
        let (old, new) = fit_rename("x.pdf", &"b".repeat(20), 20);
        assert_eq!(old, "x.pdf");
        assert!(new.chars().count() <= 16); // 20 - 5 + 1 for ellipsis leeway
    }

    #[test]
    fn test_fit_rename_zero_budget() {
        let (old, new) = fit_rename("old.pdf", "new.pdf", 0);
        assert!(old.is_empty());
        assert!(new.is_empty());
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
        DialogKind::FileDeleteSelect { files, .. } => {
            // Wide enough for "[x] {longest filename}" + 6 padding, min 44
            let widest = files.iter().map(|(n, _)| n.chars().count()).max().unwrap_or(0);
            ((widest + 10) as u16).max(44).min(area.width.saturating_sub(4))
        }
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
        DialogKind::FileDeleteSelect { files, .. } => {
            (files.len() as u16 + 5).min(area.height.saturating_sub(4))
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
        DialogKind::FileDeleteSelect { title, files } => {
            let block = Block::default()
                .borders(Borders::ALL)
                .border_style(theme.border)
                .title(format!(" {} ", title))
                .title_bottom(Line::from(Span::styled(
                    " Space:toggle  Enter:delete  Esc:cancel ",
                    theme.label,
                )));

            let items: Vec<ListItem> = files
                .iter()
                .map(|(name, delete)| {
                    let mark = if *delete { "[x]" } else { "[ ]" };
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
