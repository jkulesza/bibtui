use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::tui::theme::Theme;

pub struct CommandPaletteState {
    pub input: String,
    pub cursor: usize,
}

impl CommandPaletteState {
    pub fn new() -> Self {
        CommandPaletteState {
            input: String::new(),
            cursor: 0,
        }
    }

    pub fn clear(&mut self) {
        self.input.clear();
        self.cursor = 0;
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
    let line = Line::from(vec![
        Span::styled(":", theme.search_match),
        Span::raw(&state.input),
        Span::styled("_", Style::default().add_modifier(Modifier::SLOW_BLINK)),
    ]);

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
