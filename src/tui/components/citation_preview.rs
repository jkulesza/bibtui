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
    let width = (area.width * 3 / 4).min(90).max(40_u16.min(area.width));

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
            Span::styled(" j/k: navigate  yy: copy  Space/Esc: close ", theme.label),
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

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{backend::TestBackend, Terminal};

    fn make_terminal(width: u16, height: u16) -> Terminal<TestBackend> {
        Terminal::new(TestBackend::new(width, height)).unwrap()
    }

    fn default_theme() -> crate::tui::theme::Theme {
        crate::tui::theme::Theme::default()
    }

    // ── estimate_wrapped_lines ────────────────────────────────────────────────

    #[test]
    fn test_estimate_single_short_line() {
        assert_eq!(estimate_wrapped_lines("hello", 20), 1);
    }

    #[test]
    fn test_estimate_exact_width() {
        assert_eq!(estimate_wrapped_lines("abcde", 5), 1);
    }

    #[test]
    fn test_estimate_wraps_to_two_lines() {
        // 10 chars, width 5 → ceil(10/5) = 2
        assert_eq!(estimate_wrapped_lines("0123456789", 5), 2);
    }

    #[test]
    fn test_estimate_wraps_partial_line() {
        // 11 chars, width 5 → ceil(11/5) = 3
        assert_eq!(estimate_wrapped_lines("01234567890", 5), 3);
    }

    #[test]
    fn test_estimate_multiline_input() {
        // Two physical lines, each fits in width
        assert_eq!(estimate_wrapped_lines("hello\nworld", 20), 2);
    }

    #[test]
    fn test_estimate_multiline_with_empty_line() {
        // Empty line counts as 1
        assert_eq!(estimate_wrapped_lines("abc\n\ndef", 20), 3);
    }

    #[test]
    fn test_estimate_empty_string_returns_one() {
        assert_eq!(estimate_wrapped_lines("", 20), 1);
    }

    #[test]
    fn test_estimate_width_zero_returns_len() {
        assert_eq!(estimate_wrapped_lines("abc", 0), 3);
    }

    #[test]
    fn test_estimate_width_zero_empty_string_returns_one() {
        assert_eq!(estimate_wrapped_lines("", 0), 1);
    }

    // ── render_citation_preview ───────────────────────────────────────────────

    #[test]
    fn test_render_does_not_panic() {
        let mut term = make_terminal(120, 40);
        let state = CitationPreviewState {
            citation: "J. Smith, \"A Title,\" Journal, vol. 1, pp. 1--10, 2020.".to_string(),
            entry_key: "Smith2020".to_string(),
            style_name: "IEEEtranN".to_string(),
        };
        let theme = default_theme();
        term.draw(|f| render_citation_preview(f, f.area(), &state, &theme)).unwrap();
    }

    #[test]
    fn test_render_tiny_terminal_does_not_panic() {
        let mut term = make_terminal(15, 8);
        let state = CitationPreviewState {
            citation: "Short.".to_string(),
            entry_key: "K".to_string(),
            style_name: "S".to_string(),
        };
        let theme = default_theme();
        term.draw(|f| render_citation_preview(f, f.area(), &state, &theme)).unwrap();
    }

    #[test]
    fn test_render_long_citation_does_not_panic() {
        let mut term = make_terminal(80, 24);
        let state = CitationPreviewState {
            citation: "A".repeat(500),
            entry_key: "LongKey".to_string(),
            style_name: "IEEE".to_string(),
        };
        let theme = default_theme();
        term.draw(|f| render_citation_preview(f, f.area(), &state, &theme)).unwrap();
    }

    #[test]
    fn test_render_multiline_citation_does_not_panic() {
        let mut term = make_terminal(100, 30);
        let state = CitationPreviewState {
            citation: "Line one.\nLine two.\nLine three.".to_string(),
            entry_key: "Multi2024".to_string(),
            style_name: "Chicago".to_string(),
        };
        let theme = default_theme();
        term.draw(|f| render_citation_preview(f, f.area(), &state, &theme)).unwrap();
    }
}
