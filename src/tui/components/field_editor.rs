use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::tui::theme::Theme;

#[derive(Debug, Clone, PartialEq)]
pub enum EditingMode {
    Insert,
    Normal,
    Replace,
}

pub struct FieldEditorState {
    pub field_name: String,
    pub name_cursor: usize,
    pub value: String,
    pub cursor: usize,
    /// If true this is a new field and field_name is editable
    pub is_new: bool,
    /// When is_new, true means we're editing the name; false means editing the value
    pub editing_name: bool,
    /// If true, Tab key triggers filesystem path completion
    pub is_path: bool,
    /// If true, show the month selector grid
    pub is_month: bool,
    /// Completion candidates for the current prefix (field name or value).
    pub completions: Vec<String>,
    pub completion_idx: usize,
    /// Insert vs Normal mode for vim-style editing (value only; name editing is always Insert).
    pub editing_mode: EditingMode,
    /// Undo history: stack of (value, cursor) snapshots saved before mutations.
    pub undo_stack: Vec<(String, usize)>,
    /// Unnamed register: set by `x` and `dw`; read by `p`.
    pub unnamed_register: String,
    /// Per-keystroke undo for Replace mode: each entry is (byte_pos, original_text).
    /// `original_text` is the character that was overwritten (empty string if we appended).
    /// Cleared when leaving Replace mode.
    pub replace_undo_stack: Vec<(usize, String)>,
}

impl FieldEditorState {
    /// Create an editor for an existing field (value-only editing).
    pub fn new(field_name: &str, value: &str) -> Self {
        // Title fields with an empty value are pre-filled with "{}" so the
        // user types inside case-protection braces from the start.
        let is_title = field_name.eq_ignore_ascii_case("title")
            || field_name.eq_ignore_ascii_case("booktitle");
        let (init_value, editing_mode, cursor) = if value.is_empty() {
            if is_title {
                ("{}".to_string(), EditingMode::Insert, 1) // cursor between the braces
            } else {
                (String::new(), EditingMode::Insert, 0)
            }
        } else {
            let cursor = value.char_indices().last().map(|(i, _)| i).unwrap_or(0);
            (value.to_string(), EditingMode::Normal, cursor)
        };
        FieldEditorState {
            is_month: field_name.eq_ignore_ascii_case("month"),
            field_name: field_name.to_string(),
            name_cursor: field_name.len(),
            value: init_value,
            cursor,
            is_new: false,
            editing_name: false,
            is_path: false,
            completions: Vec::new(),
            completion_idx: 0,
            editing_mode,
            undo_stack: Vec::new(),
            unnamed_register: String::new(),
            replace_undo_stack: Vec::new(),
        }
    }

    /// Create a single-phase input editor (e.g. for group name entry).
    pub fn for_input(prompt: &str) -> Self {
        FieldEditorState {
            field_name: prompt.to_string(),
            name_cursor: 0,
            value: String::new(),
            cursor: 0,
            is_new: false,
            editing_name: false,
            is_path: false,
            is_month: false,
            completions: Vec::new(),
            completion_idx: 0,
            editing_mode: EditingMode::Insert,
            undo_stack: Vec::new(),
            unnamed_register: String::new(),
            replace_undo_stack: Vec::new(),
        }
    }

    /// Create an editor for a filesystem path (enables Tab completion hint).
    pub fn for_path(label: &str, default: &str) -> Self {
        let cursor = default.len();
        FieldEditorState {
            field_name: label.to_string(),
            name_cursor: label.len(),
            value: default.to_string(),
            cursor,
            is_new: false,
            editing_name: false,
            is_path: true,
            is_month: false,
            completions: Vec::new(),
            completion_idx: 0,
            editing_mode: EditingMode::Insert,
            undo_stack: Vec::new(),
            unnamed_register: String::new(),
            replace_undo_stack: Vec::new(),
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
            is_path: false,
            is_month: false,
            completions: Vec::new(),
            completion_idx: 0,
            // Name phase is always insert-like; start Insert so both phases show the indicator.
            editing_mode: EditingMode::Insert,
            undo_stack: Vec::new(),
            unnamed_register: String::new(),
            replace_undo_stack: Vec::new(),
        }
    }

    /// Returns true if we should move to value editing instead of confirming.
    pub fn advance_phase(&mut self) -> bool {
        if self.is_new && self.editing_name {
            self.editing_name = false;
            self.completions.clear();
            self.completion_idx = 0;
            // Pre-fill title fields with "{}" for case protection.
            let is_title = self.field_name.eq_ignore_ascii_case("title")
                || self.field_name.eq_ignore_ascii_case("booktitle");
            if is_title && self.value.is_empty() {
                self.value = "{}".to_string();
                self.cursor = 1;
            }
            true
        } else {
            false
        }
    }

    /// The ghost-text suffix: the part of the current best completion that
    /// hasn't been typed yet.  Only non-empty when the cursor is at the end
    /// of the active text and a completion extends it.
    pub fn ghost_text(&self) -> String {
        if self.is_path {
            return String::new();
        }
        let (text, cursor_pos) = if self.editing_name {
            (&self.field_name, self.name_cursor)
        } else {
            (&self.value, self.cursor)
        };
        // Only show when cursor is at the very end of the text.
        if cursor_pos < text.len() {
            return String::new();
        }
        let typed_chars = text.chars().count();
        match self.completions.get(self.completion_idx) {
            Some(c) if c.chars().count() > typed_chars => {
                c.chars().skip(typed_chars).collect()
            }
            _ => String::new(),
        }
    }

    pub fn push_char(&mut self, c: char) {
        // Convert control characters (newlines, tabs, etc.) to spaces so that
        // pasted multi-line text streams into a single line.
        let c = if c.is_control() { ' ' } else { c };
        if self.is_new && self.editing_name {
            self.field_name.insert(self.name_cursor, c);
            self.name_cursor += c.len_utf8();
        } else if self.editing_mode == EditingMode::Replace && self.cursor < self.value.len() {
            // Overwrite the character under the cursor, then advance.
            let char_len = self.value[self.cursor..]
                .chars()
                .next()
                .map(|ch| ch.len_utf8())
                .unwrap_or(0);
            // Record the original character so backspace can restore it.
            let original = self.value[self.cursor..self.cursor + char_len].to_string();
            self.replace_undo_stack.push((self.cursor, original));
            self.value.drain(self.cursor..self.cursor + char_len);
            self.value.insert(self.cursor, c);
            self.cursor += c.len_utf8();
        } else if self.editing_mode == EditingMode::Replace {
            // Appending past end of text in Replace mode — record as empty original.
            self.replace_undo_stack.push((self.cursor, String::new()));
            self.value.insert(self.cursor, c);
            self.cursor += c.len_utf8();
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
        } else if self.editing_mode == EditingMode::Replace {
            if let Some((pos, original)) = self.replace_undo_stack.pop() {
                // Remove the character we typed at `pos`.
                let typed_len = self.value[pos..]
                    .chars()
                    .next()
                    .map(|ch| ch.len_utf8())
                    .unwrap_or(0);
                self.value.drain(pos..pos + typed_len);
                // Restore the original character (empty string = it was an append, nothing to restore).
                if !original.is_empty() {
                    self.value.insert_str(pos, &original);
                }
                self.cursor = pos;
            } else if self.cursor > 0 {
                // Nothing left to restore — just move back.
                let prev = self.value[..self.cursor]
                    .char_indices()
                    .last()
                    .map(|(i, _)| i)
                    .unwrap_or(0);
                self.cursor = prev;
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
            self.unnamed_register = self.value[self.cursor..self.cursor + next_len].to_string();
            self.value.drain(self.cursor..self.cursor + next_len);
        }
    }

    /// Delete from cursor to start of next word and save to unnamed register.
    pub fn delete_word_fwd(&mut self) {
        let end = word_fwd(&self.value, self.cursor);
        if end > self.cursor {
            self.unnamed_register = self.value[self.cursor..end].to_string();
            self.value.drain(self.cursor..end);
            if self.editing_mode == EditingMode::Normal
                && !self.value.is_empty()
                && self.cursor >= self.value.len()
            {
                self.cursor = self.value.char_indices().last().map(|(i, _)| i).unwrap_or(0);
            }
        }
    }

    /// Delete from cursor to end of line (`D`); save to unnamed register.
    pub fn delete_to_end(&mut self) {
        if self.cursor < self.value.len() {
            self.unnamed_register = self.value[self.cursor..].to_string();
            self.value.truncate(self.cursor);
            if self.value.is_empty() {
                self.cursor = 0;
            } else if self.cursor >= self.value.len() {
                self.cursor = self.value.char_indices().last().map(|(i, _)| i).unwrap_or(0);
            }
        }
    }

    /// Clear entire value (`S`); save to unnamed register.
    pub fn clear_value(&mut self) {
        self.unnamed_register = self.value.clone();
        self.value.clear();
        self.cursor = 0;
    }

    /// Toggle case of char under cursor and advance one position (`~`).
    pub fn toggle_case_at_cursor(&mut self) {
        if self.cursor >= self.value.len() {
            return;
        }
        let char_len = self.value[self.cursor..]
            .chars()
            .next()
            .map(|c| c.len_utf8())
            .unwrap_or(0);
        if char_len == 0 {
            return;
        }
        let c = self.value[self.cursor..].chars().next().unwrap();
        let toggled: String = if c.is_uppercase() {
            c.to_lowercase().collect()
        } else {
            c.to_uppercase().collect()
        };
        self.value
            .replace_range(self.cursor..self.cursor + char_len, &toggled);
        let new_pos = self.cursor + toggled.len();
        self.cursor = clamp_normal(new_pos, &self.value);
    }

    /// Replace char under cursor with `c`, staying in Normal mode (`r{c}`).
    pub fn replace_char_at_cursor(&mut self, c: char) {
        if self.cursor >= self.value.len() {
            return;
        }
        let char_len = self.value[self.cursor..]
            .chars()
            .next()
            .map(|ch| ch.len_utf8())
            .unwrap_or(0);
        if char_len == 0 {
            return;
        }
        let mut replacement = String::with_capacity(c.len_utf8());
        replacement.push(c);
        self.value
            .replace_range(self.cursor..self.cursor + char_len, &replacement);
        self.cursor = clamp_normal(self.cursor, &self.value);
    }

    /// Move cursor to next occurrence of `c` to the right (`f{c}`).
    pub fn find_char_fwd(&mut self, c: char) {
        let step = self.value[self.cursor..]
            .chars()
            .next()
            .map(|ch| ch.len_utf8())
            .unwrap_or(0);
        let start = self.cursor + step;
        if let Some(pos) = find_next_char(&self.value, start, c) {
            self.cursor = pos;
        }
    }

    /// Move cursor to previous occurrence of `c` to the left (`F{c}`).
    pub fn find_char_bwd(&mut self, c: char) {
        if let Some(pos) = find_prev_char(&self.value, self.cursor, c) {
            self.cursor = pos;
        }
    }

    /// Move cursor to the char just before next occurrence of `c` (`t{c}`).
    pub fn find_to_char_fwd(&mut self, c: char) {
        let step = self.value[self.cursor..]
            .chars()
            .next()
            .map(|ch| ch.len_utf8())
            .unwrap_or(0);
        let start = self.cursor + step;
        if let Some(pos) = find_next_char(&self.value, start, c) {
            if let Some((prev_pos, _)) = self.value[..pos].char_indices().next_back() {
                if prev_pos >= self.cursor {
                    self.cursor = prev_pos;
                }
            }
        }
    }

    /// Move cursor to the char just after prev occurrence of `c` (`T{c}`).
    pub fn find_to_char_bwd(&mut self, c: char) {
        if let Some(pos) = find_prev_char(&self.value, self.cursor, c) {
            let next = pos + self.value[pos..].chars().next().map(|ch| ch.len_utf8()).unwrap_or(0);
            if next <= self.cursor {
                self.cursor = next;
            }
        }
    }

    /// Delete from cursor to (but not including) next occurrence of `c` (`dt{c}`).
    pub fn delete_to_char(&mut self, c: char) {
        let step = self.value[self.cursor..]
            .chars()
            .next()
            .map(|ch| ch.len_utf8())
            .unwrap_or(0);
        let start = self.cursor + step;
        if let Some(pos) = find_next_char(&self.value, start, c) {
            self.unnamed_register = self.value[self.cursor..pos].to_string();
            self.value.drain(self.cursor..pos);
            self.cursor = clamp_normal(self.cursor, &self.value);
        }
    }

    /// Delete from cursor through (including) next occurrence of `c` (`df{c}`).
    pub fn delete_through_char(&mut self, c: char) {
        let step = self.value[self.cursor..]
            .chars()
            .next()
            .map(|ch| ch.len_utf8())
            .unwrap_or(0);
        let start = self.cursor + step;
        if let Some(pos) = find_next_char(&self.value, start, c) {
            let end = pos + self.value[pos..].chars().next().map(|ch| ch.len_utf8()).unwrap_or(0);
            self.unnamed_register = self.value[self.cursor..end].to_string();
            self.value.drain(self.cursor..end);
            self.cursor = clamp_normal(self.cursor, &self.value);
        }
    }

    /// Delete from (but not including) prev occurrence of `c` to cursor (`dT{c}`).
    pub fn delete_to_char_back(&mut self, c: char) {
        if let Some(pos) = find_prev_char(&self.value, self.cursor, c) {
            let after = pos + self.value[pos..].chars().next().map(|ch| ch.len_utf8()).unwrap_or(0);
            if after < self.cursor {
                self.unnamed_register = self.value[after..self.cursor].to_string();
                self.value.drain(after..self.cursor);
                self.cursor = clamp_normal(after, &self.value);
            }
        }
    }

    /// Delete from (including) prev occurrence of `c` to cursor (`dF{c}`).
    pub fn delete_through_char_back(&mut self, c: char) {
        if let Some(pos) = find_prev_char(&self.value, self.cursor, c) {
            self.unnamed_register = self.value[pos..self.cursor].to_string();
            self.value.drain(pos..self.cursor);
            self.cursor = clamp_normal(pos, &self.value);
        }
    }

    /// Delete from start of previous word to cursor (Insert-mode Ctrl-W).
    pub fn delete_word_back(&mut self) {
        let start = word_bwd(&self.value, self.cursor);
        if start < self.cursor {
            self.unnamed_register = self.value[start..self.cursor].to_string();
            self.value.drain(start..self.cursor);
            self.cursor = start;
        }
    }

    /// Delete from cursor to start of line (Insert-mode Ctrl-U).
    pub fn delete_to_home(&mut self) {
        if self.cursor > 0 {
            self.unnamed_register = self.value[..self.cursor].to_string();
            self.value.drain(..self.cursor);
            self.cursor = 0;
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
            let next = self.value[self.cursor..]
                .chars()
                .next()
                .map(|c| c.len_utf8())
                .unwrap_or(0);
            let new_pos = self.cursor + next;
            // In Normal mode, stop at the last character (don't move past end)
            if self.editing_mode == EditingMode::Normal && new_pos >= self.value.len() {
                // Allow only if new_pos is a valid char start (which it is)
                self.cursor = new_pos.min(self.value.len().saturating_sub(1));
            } else {
                self.cursor = new_pos;
            }
        }
    }

    /// Move to the previous (-1) or next (+1) month in the MONTHS list.
    /// Sets value to the selected month abbreviation and moves cursor to end.
    /// Only meaningful when `is_month` is true.
    pub fn month_navigate(&mut self, delta: i32) {
        let value_lower = self.value.to_lowercase();
        let current_idx = MONTHS.iter().position(|&m| m == value_lower.as_str()).unwrap_or(0);
        let new_idx = (current_idx as i32 + delta).rem_euclid(MONTHS.len() as i32) as usize;
        self.value = MONTHS[new_idx].to_string();
        self.cursor = self.value.len();
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
        } else if self.editing_mode == EditingMode::Normal && !self.value.is_empty() {
            // In Normal mode, land on the last character, not past it
            self.cursor = self.value.char_indices().last().map(|(i, _)| i).unwrap_or(0);
        } else {
            self.cursor = self.value.len();
        }
    }

    /// Enter Normal editing mode, clamping cursor off the end of the text.
    /// Saves an undo snapshot so the insert session can be undone with `u`.
    /// Has no effect when editing the field name (name editing is always Insert).
    pub fn enter_normal(&mut self) {
        if self.editing_name {
            return;
        }
        self.editing_mode = EditingMode::Normal;
        self.replace_undo_stack.clear();
        // Clamp: cursor cannot be past the last char in Normal mode
        if !self.value.is_empty() && self.cursor >= self.value.len() {
            self.cursor = self.value.char_indices().last().map(|(i, _)| i).unwrap_or(0);
        }
    }

    /// Push the current (value, cursor) onto the undo stack (capped at 50).
    pub fn save_undo_snapshot(&mut self) {
        if self.undo_stack.len() >= 50 {
            self.undo_stack.remove(0);
        }
        self.undo_stack.push((self.value.clone(), self.cursor));
    }

    /// Restore the previous snapshot from the undo stack.
    pub fn undo_edit(&mut self) {
        if let Some((value, cursor)) = self.undo_stack.pop() {
            self.value = value;
            self.cursor = cursor;
            // Re-clamp cursor for Normal mode
            if self.editing_mode == EditingMode::Normal
                && !self.value.is_empty()
                && self.cursor >= self.value.len()
            {
                self.cursor = self.value.char_indices().last().map(|(i, _)| i).unwrap_or(0);
            }
        }
    }

    /// Insert `text` after the char at the cursor (vim `p` — put after).
    pub fn put(&mut self, text: &str) {
        let insert_pos = if self.value.is_empty() {
            0
        } else {
            let char_len = self.value[self.cursor..]
                .chars()
                .next()
                .map(|c| c.len_utf8())
                .unwrap_or(0);
            self.cursor + char_len
        };
        self.value.insert_str(insert_pos, text);
        // Cursor lands on the first char of the inserted text
        self.cursor = insert_pos;
        if self.editing_mode == EditingMode::Normal && !self.value.is_empty() {
            self.cursor = self.cursor
                .min(self.value.char_indices().last().map(|(i, _)| i).unwrap_or(0));
        }
    }

    /// Move cursor to start of next word (vim `w`).
    pub fn move_word_fwd(&mut self) {
        self.cursor = clamp_normal(word_fwd(&self.value, self.cursor), &self.value);
    }

    /// Move cursor to start of current or previous word (vim `b`).
    pub fn move_word_bwd(&mut self) {
        self.cursor = word_bwd(&self.value, self.cursor);
    }

    /// Move cursor to end of current or next word (vim `e`).
    pub fn move_word_end(&mut self) {
        self.cursor = clamp_normal(word_end(&self.value, self.cursor), &self.value);
    }

    /// Move cursor to start of next WORD (vim `W`).
    pub fn move_big_word_fwd(&mut self) {
        self.cursor = clamp_normal(big_word_fwd(&self.value, self.cursor), &self.value);
    }

    /// Move cursor to start of current or previous WORD (vim `B`).
    pub fn move_big_word_bwd(&mut self) {
        self.cursor = big_word_bwd(&self.value, self.cursor);
    }

    /// Move cursor to end of current or next WORD (vim `E`).
    pub fn move_big_word_end(&mut self) {
        self.cursor = clamp_normal(big_word_end(&self.value, self.cursor), &self.value);
    }

    /// Render the floating editor overlay into `area`.
    pub fn render(&self, f: &mut Frame, area: Rect, theme: &Theme) {
        let editor_width = (area.width.saturating_sub(4)).min(70);
        let x = area.x + (area.width.saturating_sub(editor_width)) / 2;
        // Month mode needs 4 inner rows (text + grid row 1 + grid row 2 + hint).
        let editor_height: u16 = if self.is_month { 6 } else { 3 };
        let y = area.y + area.height / 2 - editor_height / 2;
        let editor_area = Rect::new(x, y, editor_width, editor_height);

        f.render_widget(Clear, editor_area);

        // Append a mode indicator to the title.
        let mode_suffix = match self.editing_mode {
            EditingMode::Insert => " \u{2014} INSERT",
            EditingMode::Replace => " \u{2014} REPLACE",
            EditingMode::Normal => "",
        };
        let title = if self.is_new && self.editing_name {
            format!(" New Field \u{2014} Enter name{} ", mode_suffix)
        } else if self.is_new {
            format!(" New Field '{}' \u{2014} Enter value{} ", self.field_name, mode_suffix)
        } else {
            format!(" Edit: {}{} ", self.field_name, mode_suffix)
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(theme.border)
            .title(title);

        let inner = block.inner(editor_area);
        f.render_widget(block, editor_area);

        let (text, cursor_pos) = if self.is_new && self.editing_name {
            (&self.field_name, self.name_cursor)
        } else {
            (&self.value, self.cursor)
        };

        let inner_w = inner.width as usize;
        let cursor_char_idx = text[..cursor_pos].chars().count();
        let total_chars = text.chars().count();
        let tentative_center = inner_w / 2;
        let tentative_scroll = cursor_char_idx
            .saturating_sub(tentative_center)
            .min(total_chars.saturating_sub(inner_w.saturating_sub(1)));
        let has_left = tentative_scroll > 0;

        let text_w = if has_left { inner_w - 1 } else { inner_w };
        let center = text_w / 2;
        let scroll_chars = cursor_char_idx
            .saturating_sub(center)
            .min(total_chars.saturating_sub(text_w.saturating_sub(1)));
        let scroll_byte = text
            .char_indices()
            .nth(scroll_chars)
            .map(|(i, _)| i)
            .unwrap_or(text.len());

        let visible_text = &text[scroll_byte..];
        let visible_cursor_pos = cursor_pos - scroll_byte;

        let (before, after) = visible_text.split_at(visible_cursor_pos);
        let cursor_char = after.chars().next().unwrap_or(' ');
        let after_cursor = if after.is_empty() {
            ""
        } else {
            &after[cursor_char.len_utf8()..]
        };

        let before_char_count = before.chars().count();
        let after_max_full = text_w.saturating_sub(before_char_count + 1);
        let has_right = after_cursor.chars().count() > after_max_full;
        let after_max = if has_right {
            after_max_full.saturating_sub(1)
        } else {
            after_max_full
        };
        let after_visible: String = after_cursor.chars().take(after_max).collect();

        let ghost = if !has_right && after_cursor.is_empty() {
            self.ghost_text()
        } else {
            String::new()
        };
        let chars_used = before_char_count + 1 + after_visible.chars().count();
        let ghost_max = text_w.saturating_sub(chars_used);
        let ghost_display: String = ghost.chars().take(ghost_max).collect();

        let indicator_style = theme.label;
        let ghost_style = Style::default().fg(Color::DarkGray).add_modifier(Modifier::DIM);
        let mut spans: Vec<Span> = Vec::new();
        if has_left {
            spans.push(Span::styled("<", indicator_style));
        }
        spans.push(Span::raw(before.to_string()));
        spans.push(Span::styled(
            cursor_char.to_string(),
            Style::default().add_modifier(Modifier::REVERSED),
        ));
        spans.push(Span::raw(after_visible));
        if has_right {
            spans.push(Span::styled(">", indicator_style));
        } else if !ghost_display.is_empty() {
            spans.push(Span::styled(ghost_display, ghost_style));
        }
        let line = Line::from(spans);

        if self.is_month {
            let value_lower = self.value.to_lowercase();
            let highlighted: Option<&str> = if MONTHS.contains(&value_lower.as_str()) {
                Some(value_lower.as_str())
            } else {
                self.completions.get(self.completion_idx).map(|s| s.as_str())
            };

            let month_row = |months: &[&str]| -> Line {
                let mut spans: Vec<Span> = Vec::new();
                for (i, &m) in months.iter().enumerate() {
                    if i > 0 {
                        spans.push(Span::raw(" "));
                    }
                    let is_sel = highlighted == Some(m);
                    let style = if is_sel {
                        theme.border.add_modifier(Modifier::REVERSED)
                    } else {
                        theme.label
                    };
                    spans.push(Span::styled(format!(" {} ", m), style));
                }
                Line::from(spans)
            };

            let hint = Line::from(Span::styled(" Tab: cycle month  Enter: save  Esc: cancel", theme.label));
            let para = Paragraph::new(vec![
                line,
                month_row(&MONTHS[..6]),
                month_row(&MONTHS[6..]),
                hint,
            ]);
            f.render_widget(para, inner);
        } else {
            let para = Paragraph::new(vec![line]);
            f.render_widget(para, inner);
        }
    }
}

// ── Word-motion helpers ────────────────────────────────────────────────────────

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Clamp a byte position to the last character start (for Normal mode).
fn clamp_normal(pos: usize, text: &str) -> usize {
    if text.is_empty() {
        return 0;
    }
    if pos >= text.len() {
        text.char_indices().last().map(|(i, _)| i).unwrap_or(0)
    } else {
        pos
    }
}

/// Advance to the start of the next word (vim `w`).
fn word_fwd(text: &str, pos: usize) -> usize {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let n = chars.len();
    let ci = match chars.iter().position(|(i, _)| *i >= pos) {
        Some(i) => i,
        None => return pos,
    };
    if ci >= n {
        return pos;
    }
    let mut i = ci;
    if is_word_char(chars[i].1) {
        while i < n && is_word_char(chars[i].1) {
            i += 1;
        }
    } else if !chars[i].1.is_whitespace() {
        while i < n && !is_word_char(chars[i].1) && !chars[i].1.is_whitespace() {
            i += 1;
        }
    }
    while i < n && chars[i].1.is_whitespace() {
        i += 1;
    }
    if i >= n { text.len() } else { chars[i].0 }
}

/// Move to the start of the current or previous word (vim `b`).
fn word_bwd(text: &str, pos: usize) -> usize {
    if pos == 0 {
        return 0;
    }
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let ci = match chars.iter().rposition(|(i, _)| *i < pos) {
        Some(i) => i,
        None => return 0,
    };
    let mut i = ci as isize;
    while i >= 0 && chars[i as usize].1.is_whitespace() {
        i -= 1;
    }
    if i < 0 {
        return 0;
    }
    if is_word_char(chars[i as usize].1) {
        while i > 0 && is_word_char(chars[(i - 1) as usize].1) {
            i -= 1;
        }
    } else {
        while i > 0
            && !is_word_char(chars[(i - 1) as usize].1)
            && !chars[(i - 1) as usize].1.is_whitespace()
        {
            i -= 1;
        }
    }
    chars[i as usize].0
}

/// Move to the end of the current or next word (vim `e`).
fn word_end(text: &str, pos: usize) -> usize {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let n = chars.len();
    if n == 0 {
        return 0;
    }
    let ci = match chars.iter().position(|(i, _)| *i >= pos) {
        Some(i) => i,
        None => return pos,
    };
    let mut i = ci + 1;
    if i >= n {
        return chars[n - 1].0;
    }
    while i < n && chars[i].1.is_whitespace() {
        i += 1;
    }
    if i >= n {
        return chars[n - 1].0;
    }
    if is_word_char(chars[i].1) {
        while i + 1 < n && is_word_char(chars[i + 1].1) {
            i += 1;
        }
    } else {
        while i + 1 < n && !is_word_char(chars[i + 1].1) && !chars[i + 1].1.is_whitespace() {
            i += 1;
        }
    }
    chars[i].0
}

/// Move to the start of the next WORD (vim `W`).
fn big_word_fwd(text: &str, pos: usize) -> usize {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let n = chars.len();
    let ci = match chars.iter().position(|(i, _)| *i >= pos) {
        Some(i) => i,
        None => return pos,
    };
    let mut i = ci;
    while i < n && !chars[i].1.is_whitespace() {
        i += 1;
    }
    while i < n && chars[i].1.is_whitespace() {
        i += 1;
    }
    if i >= n { text.len() } else { chars[i].0 }
}

/// Move to the start of the current or previous WORD (vim `B`).
fn big_word_bwd(text: &str, pos: usize) -> usize {
    if pos == 0 {
        return 0;
    }
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let ci = match chars.iter().rposition(|(i, _)| *i < pos) {
        Some(i) => i,
        None => return 0,
    };
    let mut i = ci as isize;
    while i >= 0 && chars[i as usize].1.is_whitespace() {
        i -= 1;
    }
    if i < 0 {
        return 0;
    }
    while i > 0 && !chars[(i - 1) as usize].1.is_whitespace() {
        i -= 1;
    }
    chars[i as usize].0
}

/// Move to the end of the current or next WORD (vim `E`).
fn big_word_end(text: &str, pos: usize) -> usize {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let n = chars.len();
    if n == 0 {
        return 0;
    }
    let ci = match chars.iter().position(|(i, _)| *i >= pos) {
        Some(i) => i,
        None => return pos,
    };
    let mut i = ci + 1;
    if i >= n {
        return chars[n - 1].0;
    }
    while i < n && chars[i].1.is_whitespace() {
        i += 1;
    }
    if i >= n {
        return chars[n - 1].0;
    }
    while i + 1 < n && !chars[i + 1].1.is_whitespace() {
        i += 1;
    }
    chars[i].0
}

/// Find the byte position of the next occurrence of `c` at or after `from`.
fn find_next_char(text: &str, from: usize, c: char) -> Option<usize> {
    text[from..]
        .char_indices()
        .find(|(_, ch)| *ch == c)
        .map(|(offset, _)| from + offset)
}

/// Find the byte position of the last occurrence of `c` strictly before `before`.
fn find_prev_char(text: &str, before: usize, c: char) -> Option<usize> {
    text[..before]
        .char_indices()
        .filter(|(_, ch)| *ch == c)
        .last()
        .map(|(offset, _)| offset)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_existing_field() {
        let e = FieldEditorState::new("title", "hello");
        assert_eq!(e.field_name, "title");
        assert_eq!(e.value, "hello");
        assert_eq!(e.cursor, 4); // Normal mode: cursor on last char 'o' at byte 4
        assert_eq!(e.editing_mode, EditingMode::Normal);
        assert!(!e.is_new);
        assert!(!e.editing_name);
    }

    #[test]
    fn test_for_input() {
        let e = FieldEditorState::for_input("Group name");
        assert_eq!(e.field_name, "Group name");
        assert_eq!(e.value, "");
        assert!(!e.is_new);
    }

    #[test]
    fn test_new_field() {
        let e = FieldEditorState::new_field();
        assert!(e.is_new);
        assert!(e.editing_name);
        assert_eq!(e.field_name, "");
        assert_eq!(e.value, "");
    }

    #[test]
    fn test_advance_phase_transitions() {
        let mut e = FieldEditorState::new_field();
        assert!(e.advance_phase()); // name → value
        assert!(!e.editing_name);
        assert!(!e.advance_phase()); // already on value
    }

    #[test]
    fn test_push_char_value() {
        let mut e = FieldEditorState::new("title", "ab");
        e.cursor = e.value.len(); // position at end as if in insert mode
        e.push_char('c');
        assert_eq!(e.value, "abc");
        assert_eq!(e.cursor, 3);
    }

    #[test]
    fn test_push_char_name_editing() {
        let mut e = FieldEditorState::new_field();
        e.push_char('x');
        assert_eq!(e.field_name, "x");
        assert_eq!(e.name_cursor, 1);
    }

    #[test]
    fn test_backspace_value() {
        let mut e = FieldEditorState::new("title", "abc");
        e.cursor = e.value.len(); // position at end
        e.backspace();
        assert_eq!(e.value, "ab");
        assert_eq!(e.cursor, 2);
    }

    #[test]
    fn test_backspace_at_start_is_noop() {
        // Use a non-title field so the value starts empty (title gets pre-filled).
        let mut e = FieldEditorState::new("author", "");
        e.backspace();
        assert_eq!(e.value, "");
        assert_eq!(e.cursor, 0);
    }

    #[test]
    fn test_backspace_name() {
        let mut e = FieldEditorState::new_field();
        e.push_char('a');
        e.push_char('b');
        e.backspace();
        assert_eq!(e.field_name, "a");
    }

    #[test]
    fn test_delete_value() {
        let mut e = FieldEditorState::new("title", "abc");
        e.cursor = 0;
        e.delete();
        assert_eq!(e.value, "bc");
        assert_eq!(e.cursor, 0);
    }

    #[test]
    fn test_delete_at_end_is_noop() {
        let mut e = FieldEditorState::new("title", "abc");
        e.cursor = e.value.len(); // position past end — delete is a noop
        e.delete();
        assert_eq!(e.value, "abc");
    }

    #[test]
    fn test_cursor_left_right() {
        let mut e = FieldEditorState::new("title", "abc");
        assert_eq!(e.cursor, 2); // Normal mode: last char 'c' at byte 2
        e.cursor_left();
        assert_eq!(e.cursor, 1);
        e.cursor_right();
        assert_eq!(e.cursor, 2); // Normal mode: stops at last char
    }

    #[test]
    fn test_cursor_left_clamps() {
        let mut e = FieldEditorState::new("title", "abc");
        e.cursor = 0;
        e.cursor_left();
        assert_eq!(e.cursor, 0);
    }

    #[test]
    fn test_cursor_right_clamps() {
        let mut e = FieldEditorState::new("title", "abc");
        // Normal mode: cursor starts on last char (byte 2); cursor_right stays there
        e.cursor_right();
        assert_eq!(e.cursor, 2);
    }

    #[test]
    fn test_cursor_home_end() {
        let mut e = FieldEditorState::new("title", "abc");
        e.cursor_home();
        assert_eq!(e.cursor, 0);
        e.cursor_end();
        assert_eq!(e.cursor, 2); // Normal mode: end lands on last char 'c' at byte 2
    }

    #[test]
    fn test_cursor_home_end_name() {
        let mut e = FieldEditorState::new_field();
        e.push_char('a');
        e.push_char('b');
        e.cursor_home();
        assert_eq!(e.name_cursor, 0);
        e.cursor_end();
        assert_eq!(e.name_cursor, 2);
    }

    #[test]
    fn test_delete_name_editing() {
        let mut e = FieldEditorState::new_field();
        e.push_char('a');
        e.push_char('b');
        e.name_cursor = 0;
        e.delete();
        assert_eq!(e.field_name, "b");
    }

    // ── is_month flag ────────────────────────────────────────────────────────

    #[test]
    fn test_is_month_true_for_month_field() {
        let e = FieldEditorState::new("month", "jan");
        assert!(e.is_month, "is_month should be true when field_name is 'month'");
    }

    #[test]
    fn test_is_month_true_case_insensitive() {
        let e = FieldEditorState::new("Month", "jan");
        assert!(e.is_month, "is_month should be case-insensitive");
        let e2 = FieldEditorState::new("MONTH", "jan");
        assert!(e2.is_month, "is_month should be case-insensitive (uppercase)");
    }

    #[test]
    fn test_is_month_false_for_title() {
        let e = FieldEditorState::new("title", "My Paper");
        assert!(!e.is_month, "is_month should be false for non-month fields");
    }

    #[test]
    fn test_is_month_false_for_year() {
        let e = FieldEditorState::new("year", "2020");
        assert!(!e.is_month, "is_month should be false for 'year' field");
    }

    #[test]
    fn test_for_input_is_not_month() {
        let e = FieldEditorState::for_input("month");
        assert!(!e.is_month, "for_input should never set is_month");
    }

    #[test]
    fn test_for_path_is_not_month() {
        let e = FieldEditorState::for_path("month", "");
        assert!(!e.is_month, "for_path should never set is_month");
    }

    #[test]
    fn test_new_field_is_not_month() {
        let e = FieldEditorState::new_field();
        assert!(!e.is_month, "new_field should never set is_month");
    }

    // ── month_navigate ───────────────────────────────────────────────────────

    #[test]
    fn test_month_navigate_forward() {
        let mut e = FieldEditorState::new("month", "jan");
        e.month_navigate(1);
        assert_eq!(e.value, "feb");
        e.month_navigate(1);
        assert_eq!(e.value, "mar");
    }

    #[test]
    fn test_month_navigate_backward() {
        let mut e = FieldEditorState::new("month", "mar");
        e.month_navigate(-1);
        assert_eq!(e.value, "feb");
        e.month_navigate(-1);
        assert_eq!(e.value, "jan");
    }

    #[test]
    fn test_month_navigate_wraps_dec_to_jan() {
        let mut e = FieldEditorState::new("month", "dec");
        e.month_navigate(1);
        assert_eq!(e.value, "jan");
    }

    #[test]
    fn test_month_navigate_wraps_jan_to_dec() {
        let mut e = FieldEditorState::new("month", "jan");
        e.month_navigate(-1);
        assert_eq!(e.value, "dec");
    }

    #[test]
    fn test_month_navigate_row_down_6() {
        // jan is index 0; +6 should give jul (index 6)
        let mut e = FieldEditorState::new("month", "jan");
        e.month_navigate(6);
        assert_eq!(e.value, "jul");
    }

    #[test]
    fn test_month_navigate_row_up_6() {
        // jul is index 6; -6 should give jan (index 0)
        let mut e = FieldEditorState::new("month", "jul");
        e.month_navigate(-6);
        assert_eq!(e.value, "jan");
    }

    #[test]
    fn test_month_navigate_unknown_value_starts_from_jan() {
        // Unrecognised value → unwrap_or(0) → jan, then +1 → feb
        let mut e = FieldEditorState::new("month", "spring");
        e.month_navigate(1);
        assert_eq!(e.value, "feb");
    }

    #[test]
    fn test_month_navigate_sets_cursor_to_end() {
        let mut e = FieldEditorState::new("month", "jan");
        e.cursor = 0;
        e.month_navigate(1);
        assert_eq!(e.cursor, e.value.len());
    }

    #[test]
    fn test_month_navigate_full_cycle() {
        let mut e = FieldEditorState::new("month", "jan");
        for _ in 0..12 {
            e.month_navigate(1);
        }
        // After 12 forward steps from jan we should be back at jan
        assert_eq!(e.value, "jan");
    }

    // ── ghost_text ───────────────────────────────────────────────────────────

    #[test]
    fn test_ghost_text_empty_for_path_editor() {
        let mut e = FieldEditorState::for_path("file", "");
        e.completions = vec!["some/path.bib".to_string()];
        assert_eq!(e.ghost_text(), "", "ghost_text should be empty for path editors");
    }

    #[test]
    fn test_ghost_text_empty_when_cursor_not_at_end() {
        let mut e = FieldEditorState::new("title", "Smi");
        e.cursor = 1; // cursor in middle
        e.completions = vec!["Smith, John".to_string()];
        assert_eq!(e.ghost_text(), "");
    }

    #[test]
    fn test_ghost_text_empty_when_no_completions() {
        let e = FieldEditorState::new("title", "abc");
        // completions is empty
        assert_eq!(e.ghost_text(), "");
    }

    #[test]
    fn test_ghost_text_empty_when_completion_not_longer() {
        let mut e = FieldEditorState::new("title", "Smith, John");
        e.completions = vec!["Smith, John".to_string()]; // same length — no suffix
        assert_eq!(e.ghost_text(), "");
    }

    #[test]
    fn test_ghost_text_name_editing_phase() {
        let mut e = FieldEditorState::new_field();
        e.push_char('a');
        e.push_char('u');
        // name_cursor now = 2
        e.completions = vec!["author".to_string()];
        assert_eq!(e.ghost_text(), "thor");
    }

    #[test]
    fn test_ghost_text_name_editing_cursor_not_at_end_is_empty() {
        let mut e = FieldEditorState::new_field();
        e.push_char('a');
        e.push_char('u');
        e.push_char('t');
        e.name_cursor = 1; // move cursor back
        e.completions = vec!["author".to_string()];
        assert_eq!(e.ghost_text(), "");
    }

    // ── delete on name-editing path ──────────────────────────────────────────

    #[test]
    fn test_delete_name_mid_cursor() {
        let mut e = FieldEditorState::new_field();
        // field_name = "ab", cursor at 1 → delete 'b'
        e.push_char('a');
        e.push_char('b');
        e.name_cursor = 1;
        e.delete();
        assert_eq!(e.field_name, "a");
        assert_eq!(e.name_cursor, 1);
    }

    #[test]
    fn test_delete_name_at_end_is_noop() {
        let mut e = FieldEditorState::new_field();
        e.push_char('a');
        // name_cursor = 1 = field_name.len() → nothing to delete
        e.delete();
        assert_eq!(e.field_name, "a");
    }

    // ── cursor_left / cursor_right on name-editing path ──────────────────────

    #[test]
    fn test_cursor_left_name_editing() {
        let mut e = FieldEditorState::new_field();
        e.push_char('a');
        e.push_char('b');
        e.cursor_left();
        assert_eq!(e.name_cursor, 1);
    }

    #[test]
    fn test_cursor_left_name_editing_at_start_clamps() {
        let mut e = FieldEditorState::new_field();
        e.push_char('a');
        e.name_cursor = 0;
        e.cursor_left();
        assert_eq!(e.name_cursor, 0);
    }

    #[test]
    fn test_cursor_right_name_editing() {
        let mut e = FieldEditorState::new_field();
        e.push_char('a');
        e.push_char('b');
        e.name_cursor = 0;
        e.cursor_right();
        assert_eq!(e.name_cursor, 1);
    }

    #[test]
    fn test_cursor_right_name_editing_at_end_clamps() {
        let mut e = FieldEditorState::new_field();
        e.push_char('a');
        // name_cursor already at end
        e.cursor_right();
        assert_eq!(e.name_cursor, 1);
    }

    // ── cursor_home / cursor_end on name-editing path (already tested above in
    //    test_cursor_home_end_name, but add explicit coverage for the method ──

    #[test]
    fn test_for_path_sets_is_path_and_cursor() {
        let e = FieldEditorState::for_path("Select file", "/tmp/default.pdf");
        assert_eq!(e.field_name, "Select file");
        assert_eq!(e.value, "/tmp/default.pdf");
        assert_eq!(e.cursor, "/tmp/default.pdf".len());
        assert!(e.is_path);
        assert!(!e.is_new);
        assert!(!e.editing_name);
    }

    #[test]
    fn test_for_path_empty_default() {
        let e = FieldEditorState::for_path("Pick file", "");
        assert_eq!(e.value, "");
        assert_eq!(e.cursor, 0);
        assert!(e.is_path);
    }

    // ── EditingMode and vim motions ──────────────────────────────────────────

    #[test]
    fn test_default_editing_mode_is_normal() {
        let e = FieldEditorState::new("title", "hello");
        assert_eq!(e.editing_mode, EditingMode::Normal);
    }

    #[test]
    fn test_enter_normal_clamps_cursor() {
        let mut e = FieldEditorState::new("title", "abc");
        // cursor starts at 3 (past end); enter_normal should clamp to 2
        e.enter_normal();
        assert_eq!(e.editing_mode, EditingMode::Normal);
        assert_eq!(e.cursor, 2); // last char 'c' is at byte 2
    }

    #[test]
    fn test_enter_normal_empty_text() {
        // Use a non-title field so the value starts empty (title gets pre-filled).
        let mut e = FieldEditorState::new("author", "");
        e.enter_normal();
        assert_eq!(e.editing_mode, EditingMode::Normal);
        assert_eq!(e.cursor, 0);
    }

    #[test]
    fn test_enter_normal_noop_on_editing_name() {
        let mut e = FieldEditorState::new_field();
        // new_field starts in Normal; force Insert to verify enter_normal is truly a noop
        e.editing_mode = EditingMode::Insert;
        e.enter_normal(); // should have no effect during name editing
        assert_eq!(e.editing_mode, EditingMode::Insert);
    }

    #[test]
    fn test_cursor_right_normal_clamps_at_last_char() {
        let mut e = FieldEditorState::new("title", "abc");
        e.editing_mode = EditingMode::Normal;
        e.cursor = 1; // on 'b'
        e.cursor_right(); // advances to 'c' at byte 2
        assert_eq!(e.cursor, 2);
        e.cursor_right(); // already at last char; should stay
        assert_eq!(e.cursor, 2);
    }

    #[test]
    fn test_cursor_end_normal_lands_on_last_char() {
        let mut e = FieldEditorState::new("title", "hello");
        e.editing_mode = EditingMode::Normal;
        e.cursor_end();
        assert_eq!(e.cursor, 4); // 'o' is at byte 4
    }

    #[test]
    fn test_cursor_end_insert_past_last_char() {
        let mut e = FieldEditorState::new("title", "hello");
        e.editing_mode = EditingMode::Insert;
        e.cursor = e.value.len();
        // Insert mode: cursor_end goes to len
        e.cursor_end();
        assert_eq!(e.cursor, 5);
    }

    // ── word_fwd ─────────────────────────────────────────────────────────────

    #[test]
    fn test_word_fwd_basic() {
        assert_eq!(word_fwd("hello world", 0), 6); // start of 'world'
        assert_eq!(word_fwd("hello world", 4), 6); // from 'o', skip to 'world'
    }

    #[test]
    fn test_word_fwd_from_space() {
        assert_eq!(word_fwd("hello  world", 5), 7); // from first space to 'world'
    }

    #[test]
    fn test_word_fwd_at_end() {
        // At last char with no next word: raw function returns text.len();
        // move_word_fwd() clamps this to the last char via clamp_normal.
        let text = "hello";
        assert_eq!(word_fwd(text, 4), text.len()); // no next word → past end
        // method-level clamping is tested in test_move_word_fwd
    }

    #[test]
    fn test_word_fwd_empty() {
        assert_eq!(word_fwd("", 0), 0);
    }

    // ── word_bwd ─────────────────────────────────────────────────────────────

    #[test]
    fn test_word_bwd_basic() {
        assert_eq!(word_bwd("hello world", 8), 6); // from 'r' to start of 'world'
        assert_eq!(word_bwd("hello world", 6), 0); // from 'w' to start of 'hello'
    }

    #[test]
    fn test_word_bwd_from_space() {
        assert_eq!(word_bwd("hello world", 5), 0); // from space to start of 'hello'
    }

    #[test]
    fn test_word_bwd_at_start() {
        assert_eq!(word_bwd("hello", 0), 0);
    }

    // ── word_end ─────────────────────────────────────────────────────────────

    #[test]
    fn test_word_end_basic() {
        assert_eq!(word_end("hello world", 0), 4); // end of 'hello'
        assert_eq!(word_end("hello world", 4), 10); // from end of 'hello' to end of 'world'
    }

    #[test]
    fn test_word_end_at_last_char() {
        assert_eq!(word_end("hello", 4), 4); // already at last char
    }

    // ── big_word_fwd ─────────────────────────────────────────────────────────

    #[test]
    fn test_big_word_fwd_basic() {
        assert_eq!(big_word_fwd("hello world", 0), 6); // to 'world'
        assert_eq!(big_word_fwd("foo.bar baz", 0), 8); // foo.bar is one WORD
    }

    #[test]
    fn test_big_word_fwd_at_end() {
        // Same as word_fwd: raw function returns text.len() when no next WORD.
        let text = "hello";
        assert_eq!(big_word_fwd(text, 4), text.len());
    }

    // ── big_word_bwd ─────────────────────────────────────────────────────────

    #[test]
    fn test_big_word_bwd_basic() {
        assert_eq!(big_word_bwd("hello world", 8), 6); // from 'r' to start of 'world'
        assert_eq!(big_word_bwd("foo.bar baz", 9), 8); // from 'a' in 'baz' to 'b'
    }

    // ── big_word_end ─────────────────────────────────────────────────────────

    #[test]
    fn test_big_word_end_basic() {
        assert_eq!(big_word_end("foo.bar baz", 0), 6); // end of 'foo.bar' WORD
    }

    // ── method wrappers ──────────────────────────────────────────────────────

    #[test]
    fn test_move_word_fwd() {
        let mut e = FieldEditorState::new("title", "hello world");
        e.editing_mode = EditingMode::Normal;
        e.cursor = 0;
        e.move_word_fwd();
        assert_eq!(e.cursor, 6); // start of 'world'
    }

    #[test]
    fn test_move_word_bwd() {
        let mut e = FieldEditorState::new("title", "hello world");
        e.editing_mode = EditingMode::Normal;
        e.cursor = 8; // in 'world'
        e.move_word_bwd();
        assert_eq!(e.cursor, 6); // start of 'world'
    }

    #[test]
    fn test_move_word_end() {
        let mut e = FieldEditorState::new("title", "hello world");
        e.editing_mode = EditingMode::Normal;
        e.cursor = 0;
        e.move_word_end();
        assert_eq!(e.cursor, 4); // end of 'hello'
    }

    #[test]
    fn test_move_big_word_fwd() {
        let mut e = FieldEditorState::new("title", "foo.bar baz");
        e.editing_mode = EditingMode::Normal;
        e.cursor = 0;
        e.move_big_word_fwd();
        assert_eq!(e.cursor, 8); // start of 'baz'
    }

    #[test]
    fn test_move_big_word_bwd() {
        let mut e = FieldEditorState::new("title", "foo.bar baz");
        e.editing_mode = EditingMode::Normal;
        e.cursor = 8;
        e.move_big_word_bwd();
        assert_eq!(e.cursor, 0); // start of 'foo.bar'
    }

    #[test]
    fn test_move_big_word_end() {
        let mut e = FieldEditorState::new("title", "foo.bar baz");
        e.editing_mode = EditingMode::Normal;
        e.cursor = 0;
        e.move_big_word_end();
        assert_eq!(e.cursor, 6); // end of 'foo.bar'
    }

    #[test]
    fn test_advance_phase_sets_editing_name_false() {
        let mut e = FieldEditorState::new_field();
        e.push_char('m');
        e.push_char('o');
        e.push_char('n');
        e.push_char('t');
        e.push_char('h');
        assert!(e.advance_phase());
        assert!(!e.editing_name);
        // Note: advance_phase itself does not set is_month — that is done
        // by App::confirm_edit after the phase transition.
    }

    // ── delete_to_end ─────────────────────────────────────────────────────────

    #[test]
    fn test_delete_to_end_from_middle() {
        let mut e = FieldEditorState::new("f", "hello world");
        e.editing_mode = EditingMode::Normal;
        e.cursor = 5; // space before "world"
        e.delete_to_end();
        assert_eq!(e.value, "hello");
        assert_eq!(e.unnamed_register, " world");
        assert_eq!(e.cursor, 4); // clamped to last char 'o'
    }

    #[test]
    fn test_delete_to_end_from_last_char() {
        let mut e = FieldEditorState::new("f", "abc");
        e.editing_mode = EditingMode::Normal;
        e.cursor = 2; // 'c'
        e.delete_to_end();
        assert_eq!(e.value, "ab");
        assert_eq!(e.unnamed_register, "c");
        assert_eq!(e.cursor, 1); // clamped to 'b'
    }

    #[test]
    fn test_delete_to_end_from_start() {
        let mut e = FieldEditorState::new("f", "abc");
        e.editing_mode = EditingMode::Normal;
        e.cursor = 0;
        e.delete_to_end();
        assert_eq!(e.value, "");
        assert_eq!(e.unnamed_register, "abc");
        assert_eq!(e.cursor, 0);
    }

    // ── clear_value ───────────────────────────────────────────────────────────

    #[test]
    fn test_clear_value() {
        let mut e = FieldEditorState::new("f", "hello");
        e.cursor = 3;
        e.clear_value();
        assert_eq!(e.value, "");
        assert_eq!(e.unnamed_register, "hello");
        assert_eq!(e.cursor, 0);
    }

    // ── toggle_case_at_cursor ─────────────────────────────────────────────────

    #[test]
    fn test_toggle_case_lowercase_to_upper() {
        let mut e = FieldEditorState::new("f", "hello");
        e.editing_mode = EditingMode::Normal;
        e.cursor = 0;
        e.toggle_case_at_cursor();
        assert_eq!(e.value, "Hello");
        assert_eq!(e.cursor, 1); // advanced past 'H'
    }

    #[test]
    fn test_toggle_case_uppercase_to_lower() {
        let mut e = FieldEditorState::new("f", "Hello");
        e.editing_mode = EditingMode::Normal;
        e.cursor = 0;
        e.toggle_case_at_cursor();
        assert_eq!(e.value, "hello");
        assert_eq!(e.cursor, 1);
    }

    #[test]
    fn test_toggle_case_at_last_char_clamps() {
        let mut e = FieldEditorState::new("f", "ab");
        e.editing_mode = EditingMode::Normal;
        e.cursor = 1; // 'b' — last char
        e.toggle_case_at_cursor();
        assert_eq!(e.value, "aB");
        assert_eq!(e.cursor, 1); // clamped at last char
    }

    #[test]
    fn test_toggle_case_noop_on_empty() {
        let mut e = FieldEditorState::new("f", "");
        e.editing_mode = EditingMode::Normal;
        e.cursor = 0;
        e.toggle_case_at_cursor(); // should not panic
        assert_eq!(e.value, "");
    }

    // ── replace_char_at_cursor ────────────────────────────────────────────────

    #[test]
    fn test_replace_char_at_cursor() {
        let mut e = FieldEditorState::new("f", "abc");
        e.editing_mode = EditingMode::Normal;
        e.cursor = 1; // 'b'
        e.replace_char_at_cursor('X');
        assert_eq!(e.value, "aXc");
        assert_eq!(e.cursor, 1); // stays on replaced char
    }

    #[test]
    fn test_replace_char_at_last_char() {
        let mut e = FieldEditorState::new("f", "abc");
        e.editing_mode = EditingMode::Normal;
        e.cursor = 2; // 'c'
        e.replace_char_at_cursor('Z');
        assert_eq!(e.value, "abZ");
        assert_eq!(e.cursor, 2);
    }

    #[test]
    fn test_replace_char_noop_on_empty() {
        let mut e = FieldEditorState::new("f", "");
        e.editing_mode = EditingMode::Normal;
        e.cursor = 0;
        e.replace_char_at_cursor('X'); // should not panic
        assert_eq!(e.value, "");
    }

    // ── find_char_fwd / find_char_bwd ─────────────────────────────────────────

    #[test]
    fn test_find_char_fwd_basic() {
        let mut e = FieldEditorState::new("f", "abcabc");
        e.editing_mode = EditingMode::Normal;
        e.cursor = 0; // 'a'
        e.find_char_fwd('b');
        assert_eq!(e.cursor, 1); // first 'b'
    }

    #[test]
    fn test_find_char_fwd_skips_current() {
        let mut e = FieldEditorState::new("f", "abab");
        e.editing_mode = EditingMode::Normal;
        e.cursor = 0; // first 'a'
        e.find_char_fwd('a');
        assert_eq!(e.cursor, 2); // second 'a'
    }

    #[test]
    fn test_find_char_fwd_no_match_stays() {
        let mut e = FieldEditorState::new("f", "abc");
        e.editing_mode = EditingMode::Normal;
        e.cursor = 0;
        e.find_char_fwd('z');
        assert_eq!(e.cursor, 0); // unchanged
    }

    #[test]
    fn test_find_char_bwd_basic() {
        let mut e = FieldEditorState::new("f", "abcabc");
        e.editing_mode = EditingMode::Normal;
        e.cursor = 4; // second 'b'
        e.find_char_bwd('a');
        assert_eq!(e.cursor, 3); // second 'a'
    }

    #[test]
    fn test_find_char_bwd_no_match_stays() {
        let mut e = FieldEditorState::new("f", "abc");
        e.editing_mode = EditingMode::Normal;
        e.cursor = 2;
        e.find_char_bwd('z');
        assert_eq!(e.cursor, 2); // unchanged
    }

    // ── delete_word_back ──────────────────────────────────────────────────────

    #[test]
    fn test_delete_word_back_basic() {
        let mut e = FieldEditorState::new("f", "hello world");
        e.editing_mode = EditingMode::Insert;
        e.cursor = 11; // end of "world"
        e.delete_word_back();
        assert_eq!(e.value, "hello ");
        assert_eq!(e.unnamed_register, "world");
        assert_eq!(e.cursor, 6);
    }

    #[test]
    fn test_delete_word_back_at_start_is_noop() {
        let mut e = FieldEditorState::new("f", "hello");
        e.editing_mode = EditingMode::Insert;
        e.cursor = 0;
        e.delete_word_back();
        assert_eq!(e.value, "hello");
        assert_eq!(e.cursor, 0);
    }

    // ── delete_to_home ────────────────────────────────────────────────────────

    #[test]
    fn test_delete_to_home_basic() {
        let mut e = FieldEditorState::new("f", "hello world");
        e.editing_mode = EditingMode::Insert;
        e.cursor = 5;
        e.delete_to_home();
        assert_eq!(e.value, " world");
        assert_eq!(e.unnamed_register, "hello");
        assert_eq!(e.cursor, 0);
    }

    #[test]
    fn test_delete_to_home_at_start_is_noop() {
        let mut e = FieldEditorState::new("f", "hello");
        e.editing_mode = EditingMode::Insert;
        e.cursor = 0;
        e.delete_to_home();
        assert_eq!(e.value, "hello");
        assert_eq!(e.cursor, 0);
    }

    // ── find_next_char / find_prev_char helpers ───────────────────────────────

    #[test]
    fn test_find_next_char_basic() {
        assert_eq!(find_next_char("hello", 0, 'l'), Some(2));
    }

    #[test]
    fn test_find_next_char_from_mid() {
        assert_eq!(find_next_char("hello", 3, 'l'), Some(3));
    }

    #[test]
    fn test_find_next_char_no_match() {
        assert_eq!(find_next_char("hello", 0, 'z'), None);
    }

    #[test]
    fn test_find_prev_char_basic() {
        assert_eq!(find_prev_char("hello", 5, 'l'), Some(3));
    }

    #[test]
    fn test_find_prev_char_excludes_pos() {
        // strictly before pos=3 — should find position 2
        assert_eq!(find_prev_char("hello", 3, 'l'), Some(2));
    }

    #[test]
    fn test_find_prev_char_no_match() {
        assert_eq!(find_prev_char("hello", 5, 'z'), None);
    }

    #[test]
    fn test_push_char_converts_newline_to_space() {
        let mut editor = FieldEditorState::new("title", "hello");
        editor.editing_mode = EditingMode::Insert;
        editor.cursor = 5;
        editor.push_char('\n');
        assert_eq!(editor.value, "hello ");
    }

    #[test]
    fn test_push_char_converts_tab_to_space() {
        let mut editor = FieldEditorState::new("title", "hello");
        editor.editing_mode = EditingMode::Insert;
        editor.cursor = 5;
        editor.push_char('\t');
        assert_eq!(editor.value, "hello ");
    }

    #[test]
    fn test_push_char_converts_carriage_return_to_space() {
        let mut editor = FieldEditorState::new("title", "hello");
        editor.editing_mode = EditingMode::Insert;
        editor.cursor = 5;
        editor.push_char('\r');
        assert_eq!(editor.value, "hello ");
    }

    #[test]
    fn test_push_char_preserves_normal_chars() {
        let mut editor = FieldEditorState::new("title", "hello");
        editor.editing_mode = EditingMode::Insert;
        editor.cursor = 5;
        editor.push_char('!');
        assert_eq!(editor.value, "hello!");
    }
}

const MONTHS: [&str; 12] = [
    "jan", "feb", "mar", "apr", "may", "jun",
    "jul", "aug", "sep", "oct", "nov", "dec",
];


