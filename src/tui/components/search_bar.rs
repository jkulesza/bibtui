use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::tui::theme::Theme;

pub struct SearchBarState {
    pub query: String,
    pub cursor: usize,
    pub result_count: usize,
}

impl SearchBarState {
    pub fn new() -> Self {
        SearchBarState {
            query: String::new(),
            cursor: 0,
            result_count: 0,
        }
    }

    pub fn clear(&mut self) {
        self.query.clear();
        self.cursor = 0;
        self.result_count = 0;
    }

    pub fn push_char(&mut self, c: char) {
        self.query.insert(self.cursor, c);
        self.cursor += c.len_utf8();
    }

    pub fn backspace(&mut self) {
        if self.cursor > 0 {
            let prev = self.query[..self.cursor]
                .char_indices()
                .last()
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.query.drain(prev..self.cursor);
            self.cursor = prev;
        }
    }
}

pub fn render_search_bar(
    f: &mut Frame,
    area: Rect,
    state: &SearchBarState,
    active: bool,
    theme: &Theme,
) {
    let line = if active {
        Line::from(vec![
            Span::styled("/", theme.search_match),
            Span::raw(&state.query),
            Span::styled("_", Style::default().add_modifier(Modifier::SLOW_BLINK)),
            Span::raw(format!("  ({} matches)", state.result_count)),
        ])
    } else if !state.query.is_empty() {
        Line::from(vec![
            Span::styled("/", theme.search_match),
            Span::raw(&state.query),
            Span::raw(format!("  ({} matches)", state.result_count)),
        ])
    } else {
        Line::from(vec![
            Span::styled("  Press ", theme.label),
            Span::styled("/", theme.search_match),
            Span::styled(" to search", theme.label),
        ])
    };

    let para = Paragraph::new(line);
    f.render_widget(para, area);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let s = SearchBarState::new();
        assert_eq!(s.query, "");
        assert_eq!(s.cursor, 0);
        assert_eq!(s.result_count, 0);
    }

    #[test]
    fn test_push_char_ascii() {
        let mut s = SearchBarState::new();
        s.push_char('h');
        s.push_char('i');
        assert_eq!(s.query, "hi");
        assert_eq!(s.cursor, 2);
    }

    #[test]
    fn test_push_char_multibyte() {
        let mut s = SearchBarState::new();
        s.push_char('é'); // 2-byte UTF-8
        assert_eq!(s.query, "é");
        assert_eq!(s.cursor, 2);
    }

    #[test]
    fn test_backspace_removes_last_char() {
        let mut s = SearchBarState::new();
        s.push_char('h');
        s.push_char('i');
        s.backspace();
        assert_eq!(s.query, "h");
        assert_eq!(s.cursor, 1);
    }

    #[test]
    fn test_backspace_multibyte() {
        let mut s = SearchBarState::new();
        s.push_char('é');
        s.backspace();
        assert_eq!(s.query, "");
        assert_eq!(s.cursor, 0);
    }

    #[test]
    fn test_backspace_at_start_is_noop() {
        let mut s = SearchBarState::new();
        s.backspace();
        assert_eq!(s.query, "");
        assert_eq!(s.cursor, 0);
    }

    #[test]
    fn test_clear() {
        let mut s = SearchBarState::new();
        s.push_char('x');
        s.result_count = 42;
        s.clear();
        assert_eq!(s.query, "");
        assert_eq!(s.cursor, 0);
        assert_eq!(s.result_count, 0);
    }
}
