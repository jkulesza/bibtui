use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::tui::theme::Theme;

pub fn render_status_bar(
    f: &mut Frame,
    area: Rect,
    entry_count: usize,
    active_group: Option<&str>,
    sort_field: &str,
    sort_asc: bool,
    dirty: bool,
    message: Option<&str>,
    theme: &Theme,
) {
    let sort_dir = if sort_asc { "asc" } else { "desc" };
    let group_name = active_group.unwrap_or("All Entries");
    let dirty_indicator = if dirty { " [modified]" } else { "" };

    let left = format!(
        " {} entries | {} | Sort: {} {}{}",
        entry_count, group_name, sort_field, sort_dir, dirty_indicator
    );

    let right = message.unwrap_or(":q: quit  /: search  ?: help");

    // Pad to fill width
    let pad_len = (area.width as usize).saturating_sub(left.len() + right.len());
    let padding = " ".repeat(pad_len);

    let line = Line::from(vec![
        Span::styled(&left, theme.status_bar),
        Span::styled(padding, theme.status_bar),
        Span::styled(right, theme.status_bar),
    ]);

    let para = Paragraph::new(line);
    f.render_widget(para, area);
}
