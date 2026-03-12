use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

use crate::tui::theme::Theme;

pub struct CitationPreviewState {
    pub citation: String,
    pub entry_key: String,
    pub style_name: String,
}

pub fn render_citation_preview(
    f: &mut Frame,
    area: Rect,
    state: &CitationPreviewState,
    theme: &Theme,
) {
    let width = (area.width * 3 / 4).min(90).max(40);

    // Estimate how many lines the wrapped text will need so the box fits snugly.
    let inner_width = width.saturating_sub(2) as usize;
    let line_count = estimate_wrapped_lines(&state.citation, inner_width);
    let height = (line_count as u16 + 4).min(area.height.saturating_sub(4)).max(5);

    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    let popup_area = Rect::new(x, y, width, height);

    f.render_widget(Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.border)
        .title(format!(" {} ", state.entry_key))
        .title_bottom(Line::from(vec![
            Span::styled(
                format!(" {} ", state.style_name),
                theme.label,
            ),
            Span::styled(" j/k: navigate  Space/Esc: close ", theme.label),
        ]));

    let inner = block.inner(popup_area);
    f.render_widget(block, popup_area);

    let para = Paragraph::new(state.citation.as_str())
        .wrap(Wrap { trim: true })
        .style(theme.value);

    f.render_widget(para, inner);
}

/// Roughly estimate the number of terminal lines needed to display `text` when
/// word-wrapped to `width` columns.
fn estimate_wrapped_lines(text: &str, width: usize) -> usize {
    if width == 0 {
        return text.len().max(1);
    }
    text.lines()
        .map(|line| {
            let chars = line.chars().count();
            if chars == 0 { 1 } else { (chars + width - 1) / width }
        })
        .sum::<usize>()
        .max(1)
}
