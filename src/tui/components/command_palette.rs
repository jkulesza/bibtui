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
