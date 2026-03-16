use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
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
    /// If true, Tab key triggers filesystem path completion
    pub is_path: bool,
    /// If true, show the month selector grid
    pub is_month: bool,
    /// Completion candidates for the current prefix (field name or value).
    pub completions: Vec<String>,
    pub completion_idx: usize,
}

impl FieldEditorState {
    /// Create an editor for an existing field (value-only editing).
    pub fn new(field_name: &str, value: &str) -> Self {
        let cursor = value.len();
        FieldEditorState {
            is_month: field_name.eq_ignore_ascii_case("month"),
            field_name: field_name.to_string(),
            name_cursor: field_name.len(),
            value: value.to_string(),
            cursor,
            is_new: false,
            editing_name: false,
            is_path: false,
            completions: Vec::new(),
            completion_idx: 0,
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
        }
    }

    /// Returns true if we should move to value editing instead of confirming.
    pub fn advance_phase(&mut self) -> bool {
        if self.is_new && self.editing_name {
            self.editing_name = false;
            self.completions.clear();
            self.completion_idx = 0;
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
        } else {
            self.cursor = self.value.len();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_existing_field() {
        let e = FieldEditorState::new("title", "hello");
        assert_eq!(e.field_name, "title");
        assert_eq!(e.value, "hello");
        assert_eq!(e.cursor, 5);
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
        e.backspace();
        assert_eq!(e.value, "ab");
        assert_eq!(e.cursor, 2);
    }

    #[test]
    fn test_backspace_at_start_is_noop() {
        let mut e = FieldEditorState::new("title", "");
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
        e.delete(); // cursor at end
        assert_eq!(e.value, "abc");
    }

    #[test]
    fn test_cursor_left_right() {
        let mut e = FieldEditorState::new("title", "abc");
        assert_eq!(e.cursor, 3);
        e.cursor_left();
        assert_eq!(e.cursor, 2);
        e.cursor_right();
        assert_eq!(e.cursor, 3);
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
        e.cursor_right(); // already at end
        assert_eq!(e.cursor, 3);
    }

    #[test]
    fn test_cursor_home_end() {
        let mut e = FieldEditorState::new("title", "abc");
        e.cursor_home();
        assert_eq!(e.cursor, 0);
        e.cursor_end();
        assert_eq!(e.cursor, 3);
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
}

const MONTHS: [&str; 12] = [
    "jan", "feb", "mar", "apr", "may", "jun",
    "jul", "aug", "sep", "oct", "nov", "dec",
];

pub fn render_field_editor(
    f: &mut Frame,
    area: Rect,
    state: &FieldEditorState,
    theme: &Theme,
) {
    let editor_width = (area.width.saturating_sub(4)).min(70);
    let x = area.x + (area.width.saturating_sub(editor_width)) / 2;
    // Month mode needs 2 extra rows for the month grid.
    let editor_height: u16 = if state.is_month { 6 } else { 4 };
    let y = area.y + area.height / 2 - editor_height / 2;
    let editor_area = Rect::new(x, y, editor_width, editor_height);

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

    // Horizontal scrolling: keep cursor visible within the inner width.
    // Step 1: tentative scroll assuming full width (to detect left overflow).
    let inner_w = inner.width as usize;
    let cursor_char_idx = text[..cursor_pos].chars().count();
    let tentative_scroll = if cursor_char_idx + 1 > inner_w {
        cursor_char_idx + 1 - inner_w
    } else {
        0
    };
    let has_left = tentative_scroll > 0;

    // Step 2: reserve 1 col for left indicator when scrolled, then recompute scroll.
    let text_w = if has_left { inner_w - 1 } else { inner_w };
    let scroll_chars = if cursor_char_idx + 1 > text_w {
        cursor_char_idx + 1 - text_w
    } else {
        0
    };
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

    // Step 3: check right overflow, reserve 1 col for right indicator if needed.
    let before_char_count = before.chars().count();
    let after_max_full = text_w.saturating_sub(before_char_count + 1);
    let has_right = after_cursor.chars().count() > after_max_full;
    let after_max = if has_right {
        after_max_full.saturating_sub(1)
    } else {
        after_max_full
    };
    let after_visible: String = after_cursor.chars().take(after_max).collect();

    // Ghost text: the portion of the best completion not yet typed, shown
    // only when the cursor is at the end of the active text.
    let ghost = if !has_right && after_cursor.is_empty() {
        state.ghost_text()
    } else {
        String::new()
    };
    let chars_used = before_char_count + 1 + after_visible.chars().count();
    let ghost_max = text_w.saturating_sub(chars_used);
    let ghost_display: String = ghost.chars().take(ghost_max).collect();

    // Build the line with optional scroll indicators at each end.
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

    let has_completions = !state.completions.is_empty() && !state.is_path;
    let hint = if state.is_month {
        Line::from(Span::styled(" Tab: cycle month  Enter: save  Esc: cancel", theme.label))
    } else if state.is_new && state.editing_name {
        if has_completions {
            Line::from(Span::styled(" Tab: complete  Enter: next  Esc: cancel", theme.label))
        } else {
            Line::from(Span::styled(" Enter: next  Esc: cancel", theme.label))
        }
    } else if state.is_path {
        Line::from(Span::styled(" Tab: complete  Enter: save  Esc: cancel", theme.label))
    } else if has_completions {
        Line::from(Span::styled(" Tab: complete  Enter: save  Esc: cancel", theme.label))
    } else {
        Line::from(Span::styled(" Enter: save  Esc: cancel", theme.label))
    };

    if state.is_month {
        // Determine which month to highlight: exact value match takes priority,
        // then the current completion selection.
        let value_lower = state.value.to_lowercase();
        let highlighted: Option<&str> = if MONTHS.contains(&value_lower.as_str()) {
            Some(value_lower.as_str())
        } else {
            state.completions.get(state.completion_idx).map(|s| s.as_str())
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

        let para = Paragraph::new(vec![
            line,
            month_row(&MONTHS[..6]),
            month_row(&MONTHS[6..]),
            hint,
        ]);
        f.render_widget(para, inner);
    } else {
        let para = Paragraph::new(vec![line, hint]);
        f.render_widget(para, inner);
    }
}
