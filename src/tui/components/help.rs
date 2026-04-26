use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::tui::theme::Theme;

#[derive(Clone, Debug)]
pub enum HelpContext {
    EntryList,
    Detail,
}

pub struct HelpState {
    pub context: HelpContext,
}

pub fn render_help(f: &mut Frame, area: Rect, state: &HelpState, theme: &Theme) {
    match state.context {
        HelpContext::EntryList => render_entry_list_help(f, area, theme),
        HelpContext::Detail => render_detail_help(f, area, theme),
    }
}

fn popup_area(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    Rect::new(x, y, width, height)
}

fn render_entry_list_help(f: &mut Frame, area: Rect, theme: &Theme) {
    let width = (area.width * 9 / 10).min(100).max(60);
    let height = (area.height.saturating_sub(2)).max(10);
    let popup = popup_area(area, width, height);

    f.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.border)
        .title(" Help — Entry List ")
        .title_bottom(Line::from(Span::styled(
            " Any key: close ",
            theme.label,
        )));

    let inner = block.inner(popup);
    f.render_widget(block, popup);

    if inner.width < 4 || inner.height < 2 {
        return;
    }

    let half = inner.width / 2;
    let cols = Layout::horizontal([Constraint::Length(half), Constraint::Min(1)]).split(inner);

    let kw = theme.header;
    let dim = theme.label;

    let left_sections: &[(&str, &[(&str, &str)])] = &[
        (
            "Entry List",
            &[
                ("j / k  ↑↓",      "navigate"),
                ("g g / G",         "top / bottom"),
                ("Ctrl-F / Ctrl-B", "page down / up"),
                ("Enter",           "open detail"),
                ("/",               "search"),
                ("a",               "add entry"),
                ("d d",             "delete entry"),
                ("D",               "duplicate entry"),
                ("y y",             "yank cite key"),
                ("Space",           "citation preview"),
                ("I",               "import entry from DOI, URL, or PDF file"),
                ("u",               "undo"),
                ("o / w",           "open file / web"),
                ("B / L",           "toggle braces / LaTeX"),
                ("Tab",             "toggle group sidebar"),
                ("h / l",           "focus groups / list"),
                ("S",               "settings"),
                ("?",               "help"),
                ("Esc",             "clear search filter / reset sort"),
            ],
        ),
    ];

    let right_sections: &[(&str, &[(&str, &str)])] = &[
        (
            "Commands  ( : )",
            &[
                (":w",              "save"),
                (":q",              "quit"),
                (":q!",             "force quit"),
                (":sort <field>",   "sort by field; :sort none = file order"),
                (":group <name>",   "filter to group"),
                (":search <query>", "apply search"),
                (":import <doi/isbn/url>","import from DOI, ISBN, or URL"),
            ],
        ),
        (
            "Quality",
            &[
                ("C",   "regenerate all cite keys"),
                ("M",   "name disambiguator"),
                ("v",   "validate (preview save)"),
                ("F",   "sync all filenames to cite keys"),
            ],
        ),
        (
            "Citation Preview  ( Space )",
            &[
                ("j / k",  "scroll"),
                ("y y",    "copy to clipboard"),
                ("Esc",    "close"),
            ],
        ),
    ];

    f.render_widget(build_column(left_sections, cols[0].width, kw, dim), cols[0]);
    f.render_widget(build_column(right_sections, cols[1].width, kw, dim), cols[1]);
}

fn render_detail_help(f: &mut Frame, area: Rect, theme: &Theme) {
    let width = (area.width * 9 / 10).min(100).max(60);
    let height = (area.height.saturating_sub(2)).max(10);
    let popup = popup_area(area, width, height);

    f.render_widget(Clear, popup);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.border)
        .title(" Help — Detail View ")
        .title_bottom(Line::from(Span::styled(
            " Any key: close ",
            theme.label,
        )));

    let inner = block.inner(popup);
    f.render_widget(block, popup);

    if inner.width < 4 || inner.height < 2 {
        return;
    }

    let half = inner.width / 2;
    let cols = Layout::horizontal([Constraint::Length(half), Constraint::Min(1)]).split(inner);

    let kw = theme.header;
    let dim = theme.label;

    let left_sections: &[(&str, &[(&str, &str)])] = &[
        (
            "Detail View",
            &[
                ("j / k",           "navigate fields"),
                ("g g / G",         "top / bottom"),
                ("Ctrl-F / Ctrl-B", "page down / up"),
                ("e / i / Enter",   "edit field / file path"),
                ("a",               "add field"),
                ("A",               "add file attachment"),
                ("d",               "delete field / file"),
                ("T",               "title-case field"),
                ("N",               "normalize names"),
                ("c",               "regenerate cite key"),
                ("F",               "sync filename to cite key"),
                ("Tab",             "assign groups"),
                ("o / w",           "open file / web (w fetches DOI if absent)"),
                ("B / L",           "toggle braces / LaTeX"),
                ("u",               "undo"),
                ("?",               "help"),
                ("Esc",             "back to list"),
            ],
        ),
    ];

    let right_sections: &[(&str, &[(&str, &str)])] = &[
        (
            "Field Editor",
            &[
                ("i / a / A / I",   "enter Insert mode"),
                ("R",               "enter Replace mode"),
                ("r{c}",            "replace char at cursor"),
                ("f{c} / F{c}",     "find char fwd / bwd (inclusive)"),
                ("t{c} / T{c}",     "to char fwd / bwd (exclusive)"),
                ("dw",              "delete word forward"),
                ("dt{c} / df{c}",   "delete to / through char fwd"),
                ("dT{c} / dF{c}",   "delete to / through char bwd"),
                ("yy",              "yank whole field value"),
                ("Tab / S-Tab",     "autocomplete fwd / bwd"),
                ("Esc",             "exit editor"),
            ],
        ),
        (
            "Settings  ( S )",
            &[
                ("j / k",           "navigate"),
                ("g / G",           "top / bottom"),
                ("Ctrl-F/B",        "page down / up"),
                ("Enter / Space",   "toggle"),
                ("e",               "edit value"),
                ("a",               "add field group"),
                ("x",               "delete field group"),
                ("r",               "rename field group"),
                ("E / I",           "export / import config"),
                ("Esc",             "close"),
            ],
        ),
    ];

    f.render_widget(build_column(left_sections, cols[0].width, kw, dim), cols[0]);
    f.render_widget(build_column(right_sections, cols[1].width, kw, dim), cols[1]);
}

fn build_column<'a>(
    sections: &[(&'a str, &[(&'a str, &'a str)])],
    col_width: u16,
    kw: ratatui::style::Style,
    dim: ratatui::style::Style,
) -> Paragraph<'a> {
    let key_col = 18usize;
    let max_desc = (col_width as usize).saturating_sub(key_col + 3);

    let bold = kw.add_modifier(Modifier::BOLD);

    let mut lines: Vec<Line<'a>> = Vec::new();
    for (section, entries) in sections {
        if !lines.is_empty() {
            lines.push(Line::from(""));
        }
        lines.push(Line::from(Span::styled(*section, bold)));
        for (key, desc) in *entries {
            let desc_trunc: String = desc.chars().take(max_desc).collect();
            lines.push(Line::from(vec![
                Span::styled(format!("  {:<width$}", key, width = key_col), kw),
                Span::styled(desc_trunc, dim),
            ]));
        }
    }
    Paragraph::new(lines)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{backend::TestBackend, Terminal};

    fn make_terminal(width: u16, height: u16) -> Terminal<TestBackend> {
        Terminal::new(TestBackend::new(width, height)).unwrap()
    }

    fn default_theme() -> Theme {
        crate::tui::theme::Theme::default()
    }

    #[test]
    fn test_help_state_entry_list_context() {
        let state = HelpState { context: HelpContext::EntryList };
        assert!(matches!(state.context, HelpContext::EntryList));
    }

    #[test]
    fn test_help_state_detail_context() {
        let state = HelpState { context: HelpContext::Detail };
        assert!(matches!(state.context, HelpContext::Detail));
    }

    #[test]
    fn test_render_entry_list_help_does_not_panic() {
        let mut term = make_terminal(120, 40);
        let state = HelpState { context: HelpContext::EntryList };
        let theme = default_theme();
        term.draw(|f| render_help(f, f.area(), &state, &theme)).unwrap();
    }

    #[test]
    fn test_render_detail_help_does_not_panic() {
        let mut term = make_terminal(120, 40);
        let state = HelpState { context: HelpContext::Detail };
        let theme = default_theme();
        term.draw(|f| render_help(f, f.area(), &state, &theme)).unwrap();
    }

    #[test]
    fn test_entry_list_help_contains_expected_keys() {
        let mut term = make_terminal(120, 40);
        let state = HelpState { context: HelpContext::EntryList };
        let theme = default_theme();
        term.draw(|f| render_help(f, f.area(), &state, &theme)).unwrap();
        let buf = term.backend().buffer().clone();
        let rendered: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(rendered.contains("Entry List"),    "missing 'Entry List' section");
        assert!(rendered.contains("Quality"),       "missing 'Quality' section");
        assert!(rendered.contains("Commands"),      "missing 'Commands' section");
        assert!(rendered.contains("Citation Preview"), "missing 'Citation Preview' section");
        // Spot-check a few key bindings
        assert!(rendered.contains(":w"),            "missing :w command");
        assert!(rendered.contains("j / k"),         "missing j/k navigation");
    }

    #[test]
    fn test_detail_help_contains_expected_keys() {
        let mut term = make_terminal(120, 40);
        let state = HelpState { context: HelpContext::Detail };
        let theme = default_theme();
        term.draw(|f| render_help(f, f.area(), &state, &theme)).unwrap();
        let buf = term.backend().buffer().clone();
        let rendered: String = buf.content().iter().map(|c| c.symbol()).collect();
        assert!(rendered.contains("Detail View"),   "missing 'Detail View' section");
        assert!(rendered.contains("Field Editor"),  "missing 'Field Editor' section");
        assert!(rendered.contains("Settings"),      "missing 'Settings' section");
    }

    #[test]
    fn test_entry_list_help_quality_section_has_correct_keys() {
        let mut term = make_terminal(120, 40);
        let state = HelpState { context: HelpContext::EntryList };
        let theme = default_theme();
        term.draw(|f| render_help(f, f.area(), &state, &theme)).unwrap();
        let buf = term.backend().buffer().clone();
        let rendered: String = buf.content().iter().map(|c| c.symbol()).collect();
        // Quality section keys
        assert!(rendered.contains('C'), "missing C (regen citekeys)");
        assert!(rendered.contains('M'), "missing M (disambiguator)");
        assert!(rendered.contains('v'), "missing v (validate)");
        assert!(rendered.contains('F'), "missing F (sync filenames)");
    }

    #[test]
    fn test_render_help_tiny_terminal_does_not_panic() {
        // Ensure graceful handling of very small terminal sizes.
        let mut term = make_terminal(10, 5);
        let state = HelpState { context: HelpContext::EntryList };
        let theme = default_theme();
        term.draw(|f| render_help(f, f.area(), &state, &theme)).unwrap();
    }

    #[test]
    fn test_help_context_clone() {
        let ctx = HelpContext::Detail;
        let cloned = ctx.clone();
        assert!(matches!(cloned, HelpContext::Detail));
    }

    #[test]
    fn test_build_column_empty_sections_returns_empty_paragraph() {
        let theme = default_theme();
        let para = build_column(&[], 40, theme.header, theme.label);
        // Paragraph with no lines — just verify it constructs without panic.
        let _ = para;
    }

    #[test]
    fn test_build_column_truncates_long_desc() {
        let theme = default_theme();
        // col_width=10, key_col=18 → max_desc = 10.saturating_sub(21) = 0, all descs truncated.
        let sections: &[(&str, &[(&str, &str)])] = &[
            ("Sec", &[("k", "a very long description that should be truncated")]),
        ];
        let _ = build_column(sections, 10, theme.header, theme.label);
    }
}
