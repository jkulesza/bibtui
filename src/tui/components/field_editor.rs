use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::tui::theme::Theme;

pub struct FieldEditorState {
    pub field_name: String,
    pub name_cursor: usize,
    pub value: String,
    pub cursor: usize,
    /// If true this is a new field and field_name is editable
    pub is_new: bool,
    /// When is_new, true means we're editing the name; false means editing the value
    pub editing_name: bool,
}

impl FieldEditorState {
    /// Create an editor for an existing field (value-only editing).
    pub fn new(field_name: &str, value: &str) -> Self {
        let cursor = value.len();
        FieldEditorState {
            field_name: field_name.to_string(),
            name_cursor: field_name.len(),
            value: value.to_string(),
            cursor,
            is_new: false,
            editing_name: false,
        }
    }

    /// Create an editor for a brand-new field (name then value).
    pub fn new_field() -> Self {
        FieldEditorState {
            field_name: String::new(),
            name_cursor: 0,
            value: String::new(),
            cursor: 0,
            is_new: true,
            editing_name: true,
        }
    }

    /// Returns true if we should move to value editing instead of confirming.
    pub fn advance_phase(&mut self) -> bool {
        if self.is_new && self.editing_name {
            self.editing_name = false;
            true
        } else {
            false
        }
    }

    pub fn push_char(&mut self, c: char) {
        if self.is_new && self.editing_name {
            self.field_name.insert(self.name_cursor, c);
            self.name_cursor += c.len_utf8();
        } else {
            self.value.insert(self.cursor, c);
            self.cursor += c.len_utf8();
        }
    }

    pub fn backspace(&mut self) {
        if self.is_new && self.editing_name {
            if self.name_cursor > 0 {
                let prev = self.field_name[..self.name_cursor]
                    .char_indices()
                    .last()
                    .map(|(i, _)| i)
                    .unwrap_or(0);
                self.field_name.drain(prev..self.name_cursor);
                self.name_cursor = prev;
            }
        } else if self.cursor > 0 {
            let prev = self.value[..self.cursor]
                .char_indices()
                .last()
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.value.drain(prev..self.cursor);
            self.cursor = prev;
        }
    }

    pub fn delete(&mut self) {
        if self.is_new && self.editing_name {
            if self.name_cursor < self.field_name.len() {
                let next_len = self.field_name[self.name_cursor..]
                    .chars()
                    .next()
                    .map(|c| c.len_utf8())
                    .unwrap_or(0);
                self.field_name.drain(self.name_cursor..self.name_cursor + next_len);
            }
        } else if self.cursor < self.value.len() {
            let next_len = self.value[self.cursor..]
                .chars()
                .next()
                .map(|c| c.len_utf8())
                .unwrap_or(0);
            self.value.drain(self.cursor..self.cursor + next_len);
        }
    }

    pub fn cursor_left(&mut self) {
        if self.is_new && self.editing_name {
            if self.name_cursor > 0 {
                self.name_cursor = self.field_name[..self.name_cursor]
                    .char_indices()
                    .last()
                    .map(|(i, _)| i)
                    .unwrap_or(0);
            }
        } else if self.cursor > 0 {
            self.cursor = self.value[..self.cursor]
                .char_indices()
                .last()
                .map(|(i, _)| i)
                .unwrap_or(0);
        }
    }

    pub fn cursor_right(&mut self) {
        if self.is_new && self.editing_name {
            if self.name_cursor < self.field_name.len() {
                self.name_cursor += self.field_name[self.name_cursor..]
                    .chars()
                    .next()
                    .map(|c| c.len_utf8())
                    .unwrap_or(0);
            }
        } else if self.cursor < self.value.len() {
            self.cursor += self.value[self.cursor..]
                .chars()
                .next()
                .map(|c| c.len_utf8())
                .unwrap_or(0);
        }
    }

    pub fn cursor_home(&mut self) {
        if self.is_new && self.editing_name {
            self.name_cursor = 0;
        } else {
            self.cursor = 0;
        }
    }

    pub fn cursor_end(&mut self) {
        if self.is_new && self.editing_name {
            self.name_cursor = self.field_name.len();
        } else {
            self.cursor = self.value.len();
        }
    }
}

pub fn render_field_editor(
    f: &mut Frame,
    area: Rect,
    state: &FieldEditorState,
    theme: &Theme,
) {
    let editor_width = (area.width.saturating_sub(4)).min(70);
    let x = area.x + (area.width.saturating_sub(editor_width)) / 2;
    let y = area.y + area.height / 2 - 2;
    let editor_area = Rect::new(x, y, editor_width, 4);

    f.render_widget(Clear, editor_area);

    let title = if state.is_new && state.editing_name {
        " New Field — Enter name ".to_string()
    } else if state.is_new {
        format!(" New Field '{}' — Enter value ", state.field_name)
    } else {
        format!(" Edit: {} ", state.field_name)
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.border)
        .title(title);

    let inner = block.inner(editor_area);
    f.render_widget(block, editor_area);

    let (text, cursor_pos) = if state.is_new && state.editing_name {
        (&state.field_name, state.name_cursor)
    } else {
        (&state.value, state.cursor)
    };

    let (before, after) = text.split_at(cursor_pos);
    let cursor_char = after.chars().next().unwrap_or(' ');
    let after_cursor = if after.is_empty() {
        ""
    } else {
        &after[cursor_char.len_utf8()..]
    };

    let line = Line::from(vec![
        Span::raw(before.to_string()),
        Span::styled(
            cursor_char.to_string(),
            Style::default().add_modifier(Modifier::REVERSED),
        ),
        Span::raw(after_cursor.to_string()),
    ]);

    let hint = if state.is_new && state.editing_name {
        Line::from(Span::styled(" Enter: next  Esc: cancel", theme.label))
    } else {
        Line::from(Span::styled(" Enter: save  Esc: cancel", theme.label))
    };

    let para = Paragraph::new(vec![line, hint]);
    f.render_widget(para, inner);
}
