use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::tui::theme::Theme;

pub struct CommandPaletteState {
    pub input: String,
    pub cursor: usize,
    /// Current tab-completion candidates (only populated for `:sort <field>`).
    pub completions: Vec<String>,
    pub completion_idx: usize,
}

impl CommandPaletteState {
    pub fn new() -> Self {
        CommandPaletteState {
            input: String::new(),
            cursor: 0,
            completions: Vec::new(),
            completion_idx: 0,
        }
    }

    pub fn clear(&mut self) {
        self.input.clear();
        self.cursor = 0;
        self.completions.clear();
        self.completion_idx = 0;
    }

    /// Returns the ghost-text suffix to display after the cursor: the part of
    /// the current completion that hasn't been typed yet.
    pub fn ghost_text(&self) -> &str {
        let partial = match self.input.strip_prefix("sort ") {
            Some(p) => p,
            None => return "",
        };
        match self.completions.get(self.completion_idx) {
            Some(c) if c.starts_with(partial) && partial.len() < c.len() => {
                &c[partial.len()..]
            }
            _ => "",
        }
    }

    pub fn push_char(&mut self, c: char) {
        self.input.insert(self.cursor, c);
        self.cursor += c.len_utf8();
    }

    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            let prev = self.input[..self.cursor]
                .char_indices()
                .last()
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.input.drain(prev..self.cursor);
            self.cursor = prev;
        }
    }
}

pub fn render_command_palette(
    f: &mut Frame,
    area: Rect,
    state: &CommandPaletteState,
    theme: &Theme,
) {
    let ghost = state.ghost_text().to_string();
    let ghost_style = Style::default().fg(Color::DarkGray).add_modifier(Modifier::DIM);
    let mut spans = vec![
        Span::styled(":", theme.search_match),
        Span::raw(state.input.clone()),
    ];
    if !ghost.is_empty() {
        spans.push(Span::styled(ghost, ghost_style));
    }
    spans.push(Span::styled("_", Style::default().add_modifier(Modifier::SLOW_BLINK)));
    let line = Line::from(spans);

    let para = Paragraph::new(line);
    f.render_widget(para, area);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let c = CommandPaletteState::new();
        assert_eq!(c.input, "");
        assert_eq!(c.cursor, 0);
    }

    #[test]
    fn test_push_char() {
        let mut c = CommandPaletteState::new();
        c.push_char('w');
        c.push_char('q');
        assert_eq!(c.input, "wq");
        assert_eq!(c.cursor, 2);
    }

    #[test]
    fn test_backspace_removes_last_char() {
        let mut c = CommandPaletteState::new();
        c.push_char('w');
        c.push_char('q');
        c.backspace();
        assert_eq!(c.input, "w");
        assert_eq!(c.cursor, 1);
    }

    #[test]
    fn test_backspace_at_start_is_noop() {
        let mut c = CommandPaletteState::new();
        c.backspace();
        assert_eq!(c.input, "");
        assert_eq!(c.cursor, 0);
    }

    #[test]
    fn test_clear() {
        let mut c = CommandPaletteState::new();
        c.push_char('s');
        c.push_char('a');
        c.push_char('v');
        c.push_char('e');
        c.clear();
        assert_eq!(c.input, "");
        assert_eq!(c.cursor, 0);
    }

    #[test]
    fn test_push_and_backspace_multibyte() {
        let mut c = CommandPaletteState::new();
        c.push_char('é'); // 2-byte UTF-8
        assert_eq!(c.cursor, 2);
        c.backspace();
        assert_eq!(c.input, "");
        assert_eq!(c.cursor, 0);
    }

    // ── ghost_text ────────────────────────────────────────────────────────────

    #[test]
    fn test_ghost_text_empty_when_no_sort_prefix() {
        let mut c = CommandPaletteState::new();
        c.completions = vec!["year".to_string()];
        c.input = "w".to_string();
        c.cursor = 1;
        assert_eq!(c.ghost_text(), "");
    }

    #[test]
    fn test_ghost_text_empty_when_no_completions() {
        let mut c = CommandPaletteState::new();
        c.input = "sort y".to_string();
        c.cursor = 6;
        assert_eq!(c.ghost_text(), "");
    }

    #[test]
    fn test_ghost_text_returns_suffix_of_completion() {
        let mut c = CommandPaletteState::new();
        c.input = "sort y".to_string();
        c.cursor = 6;
        c.completions = vec!["year".to_string()];
        c.completion_idx = 0;
        assert_eq!(c.ghost_text(), "ear");
    }

    #[test]
    fn test_ghost_text_empty_when_typed_equals_completion() {
        let mut c = CommandPaletteState::new();
        c.input = "sort year".to_string();
        c.cursor = 9;
        c.completions = vec!["year".to_string()];
        c.completion_idx = 0;
        assert_eq!(c.ghost_text(), "");
    }

    #[test]
    fn test_ghost_text_empty_when_typed_longer_than_completion() {
        let mut c = CommandPaletteState::new();
        c.input = "sort yearx".to_string();
        c.cursor = 10;
        c.completions = vec!["year".to_string()];
        c.completion_idx = 0;
        assert_eq!(c.ghost_text(), "");
    }

    #[test]
    fn test_ghost_text_partial_match_returns_suffix() {
        // command_palette ghost_text derives partial from input text, not cursor
        let mut c = CommandPaletteState::new();
        c.input = "sort ye".to_string();
        c.cursor = 5;
        c.completions = vec!["year".to_string()];
        c.completion_idx = 0;
        assert_eq!(c.ghost_text(), "ar");
    }

    #[test]
    fn test_ghost_text_uses_completion_idx() {
        let mut c = CommandPaletteState::new();
        c.input = "sort y".to_string();
        c.cursor = 6;
        c.completions = vec!["volume".to_string(), "year".to_string()];
        c.completion_idx = 1; // "year"
        assert_eq!(c.ghost_text(), "ear");
    }

    #[test]
    fn test_ghost_text_empty_when_completion_does_not_start_with_partial() {
        let mut c = CommandPaletteState::new();
        c.input = "sort au".to_string();
        c.cursor = 7;
        c.completions = vec!["year".to_string()];
        c.completion_idx = 0;
        assert_eq!(c.ghost_text(), "");
    }

    #[test]
    fn test_clear_resets_completions() {
        let mut c = CommandPaletteState::new();
        c.push_char('s');
        c.completions = vec!["year".to_string()];
        c.completion_idx = 1;
        c.clear();
        assert!(c.completions.is_empty());
        assert_eq!(c.completion_idx, 0);
    }
}
