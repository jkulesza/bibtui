use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::tui::theme::Theme;

/// One field that would be modified by a save action.
pub struct Violation {
    pub entry_key: String,
    pub field: String,
    pub old_value: String,
    pub new_value: String,
    /// Short label for the save action responsible for this change.
    pub action_name: &'static str,
}

pub struct ValidateResultsState {
    pub violations: Vec<Violation>,
    pub scroll: usize,
}

impl ValidateResultsState {
    pub fn new(violations: Vec<Violation>) -> Self {
        ValidateResultsState { violations, scroll: 0 }
    }

    pub fn scroll_down(&mut self, inner_height: u16, total_lines: usize) {
        let max = total_lines.saturating_sub(inner_height as usize);
        self.scroll = (self.scroll + 1).min(max);
    }

    pub fn scroll_up(&mut self) {
        self.scroll = self.scroll.saturating_sub(1);
    }
}

pub fn render_validate_results(
    f: &mut Frame,
    area: Rect,
    state: &mut ValidateResultsState,
    theme: &Theme,
) {
    let width = (area.width * 4 / 5).min(110).max(50);
    let height = (area.height.saturating_sub(4)).max(8).min(area.height);

    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    let popup_area = Rect::new(x, y, width, height);

    f.render_widget(Clear, popup_area);

    let title = if state.violations.is_empty() {
        " Validate: all entries are valid ".to_string()
    } else {
        format!(
            " Validate: {} field(s) in {} entr{} would change on save ",
            state.violations.len(),
            unique_entry_count(&state.violations),
            if unique_entry_count(&state.violations) == 1 { "y" } else { "ies" },
        )
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.border)
        .title(title)
        .title_bottom(Line::from(Span::styled(
            " j/k: scroll  Esc: close ",
            theme.label,
        )));

    let inner = block.inner(popup_area);
    f.render_widget(block, popup_area);

    if state.violations.is_empty() {
        let para = Paragraph::new(
            "No entries would be modified by the current save actions.",
        )
        .style(theme.value);
        f.render_widget(para, inner);
        return;
    }

    let key_style = Style::default().add_modifier(Modifier::BOLD);
    let field_style = theme.label;
    let action_style = theme.search_match;
    let minus_style = Style::default().fg(Color::Red);
    let plus_style = Style::default().fg(Color::Green);

    let max_w = inner.width.saturating_sub(4) as usize;

    let mut lines: Vec<Line> = Vec::new();
    for v in &state.violations {
        // Entry key + field + action on one line
        lines.push(Line::from(vec![
            Span::styled(truncate(&v.entry_key, 40), key_style),
            Span::styled("  ", Style::default()),
            Span::styled(v.field.clone(), field_style),
            Span::styled("  ", Style::default()),
            Span::styled(format!("[{}]", v.action_name), action_style),
        ]));
        // Old value
        let old = truncate(&v.old_value, max_w);
        lines.push(Line::from(vec![
            Span::styled("- ", minus_style),
            Span::styled(old, theme.value),
        ]));
        // New value
        let new = truncate(&v.new_value, max_w);
        lines.push(Line::from(vec![
            Span::styled("+ ", plus_style),
            Span::styled(new, theme.value),
        ]));
        lines.push(Line::from(""));
    }

    let total_lines = lines.len();

    // Clamp scroll
    let max_scroll = total_lines.saturating_sub(inner.height as usize);
    state.scroll = state.scroll.min(max_scroll);

    let para = Paragraph::new(lines).scroll((state.scroll as u16, 0));
    f.render_widget(para, inner);
}

pub(crate) fn unique_entry_count(violations: &[Violation]) -> usize {
    let mut seen = std::collections::HashSet::new();
    for v in violations {
        seen.insert(&v.entry_key);
    }
    seen.len()
}

pub(crate) fn truncate(s: &str, max_chars: usize) -> String {
    if max_chars == 0 {
        return String::new();
    }
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max_chars {
        s.to_string()
    } else {
        let t: String = chars[..max_chars.saturating_sub(1)].iter().collect();
        format!("{}…", t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_violation(key: &str, field: &str) -> Violation {
        Violation {
            entry_key: key.to_string(),
            field: field.to_string(),
            old_value: "old".to_string(),
            new_value: "new".to_string(),
            action_name: "test_action",
        }
    }

    // ── ValidateResultsState ──────────────────────────────────────────────────

    #[test]
    fn test_new_empty() {
        let s = ValidateResultsState::new(vec![]);
        assert!(s.violations.is_empty());
        assert_eq!(s.scroll, 0);
    }

    #[test]
    fn test_new_with_violations() {
        let v = vec![make_violation("k1", "title"), make_violation("k2", "author")];
        let s = ValidateResultsState::new(v);
        assert_eq!(s.violations.len(), 2);
    }

    #[test]
    fn test_scroll_up_at_zero_is_noop() {
        let mut s = ValidateResultsState::new(vec![make_violation("k", "f")]);
        s.scroll_up();
        assert_eq!(s.scroll, 0);
    }

    #[test]
    fn test_scroll_down_bounded_by_total_lines() {
        let mut s = ValidateResultsState::new(vec![make_violation("k", "f")]);
        // total_lines = 4 (one violation × 4 lines), inner_height = 24
        // max_scroll = 4.saturating_sub(24) = 0 → clamped to 0
        s.scroll_down(24, 4);
        assert_eq!(s.scroll, 0);
    }

    #[test]
    fn test_scroll_down_advances_when_room() {
        let mut s = ValidateResultsState::new(vec![make_violation("k", "f")]);
        // total_lines = 100, inner_height = 10 → max_scroll = 90
        s.scroll_down(10, 100);
        assert_eq!(s.scroll, 1);
    }

    #[test]
    fn test_scroll_up_decrements() {
        let mut s = ValidateResultsState::new(vec![make_violation("k", "f")]);
        s.scroll_down(10, 100);
        s.scroll_down(10, 100);
        assert_eq!(s.scroll, 2);
        s.scroll_up();
        assert_eq!(s.scroll, 1);
    }

    // ── unique_entry_count ────────────────────────────────────────────────────

    #[test]
    fn test_unique_entry_count_empty() {
        assert_eq!(unique_entry_count(&[]), 0);
    }

    #[test]
    fn test_unique_entry_count_deduplicates() {
        let v = vec![
            make_violation("k1", "title"),
            make_violation("k1", "author"),
            make_violation("k2", "title"),
        ];
        assert_eq!(unique_entry_count(&v), 2);
    }

    // ── truncate ─────────────────────────────────────────────────────────────

    #[test]
    fn test_truncate_short_string_unchanged() {
        assert_eq!(truncate("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_exact_length_unchanged() {
        assert_eq!(truncate("hello", 5), "hello");
    }

    #[test]
    fn test_truncate_long_string_ellipsis() {
        let result = truncate("hello world", 6);
        assert!(result.ends_with('…'));
        assert!(result.len() < "hello world".len());
    }

    #[test]
    fn test_truncate_zero_max_returns_empty() {
        assert_eq!(truncate("hello", 0), "");
    }
}
