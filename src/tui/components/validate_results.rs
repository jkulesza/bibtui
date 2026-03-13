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

fn unique_entry_count(violations: &[Violation]) -> usize {
    let mut seen = std::collections::HashSet::new();
    for v in violations {
        seen.insert(&v.entry_key);
    }
    seen.len()
}

fn truncate(s: &str, max_chars: usize) -> String {
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
