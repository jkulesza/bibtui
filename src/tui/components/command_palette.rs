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
}
