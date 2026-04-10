use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::tui::theme::Theme;

pub struct HelpState;

pub fn render_help(f: &mut Frame, area: Rect, _state: &HelpState, theme: &Theme) {
    let width = (area.width * 9 / 10).min(100).max(60);
    let height = (area.height.saturating_sub(2)).max(10);

    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;
    let popup_area = Rect::new(x, y, width, height);

    f.render_widget(Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.border)
        .title(" Help ")
        .title_bottom(Line::from(Span::styled(
            " Any key: close ",
            theme.label,
        )));

    let inner = block.inner(popup_area);
    f.render_widget(block, popup_area);

    if inner.width < 4 || inner.height < 2 {
        return;
    }

    // Split into two columns
    let half = inner.width / 2;
    let cols = Layout::horizontal([Constraint::Length(half), Constraint::Min(1)]).split(inner);

    let kw = theme.header;
    let dim = theme.label;

    // ── Left column: Entry list (Normal mode) ────────────────────────────────

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
                ("C",               "regenerate all cite keys"),
                ("v",               "validate (preview save)"),
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
    ];

    // ── Right column: Detail view ────────────────────────────────────────────

    let right_sections: &[(&str, &[(&str, &str)])] = &[
        (
            "Detail View",
            &[
                ("j / k",        "navigate fields"),
                ("g g / G",      "top / bottom"),
                ("Ctrl-F / Ctrl-B", "page down / up"),
                ("e / i / Enter","edit field / file path"),
                ("a",            "add field"),
                ("A",            "add file attachment"),
                ("d",            "delete field / file"),
                ("T",            "title-case field"),
                ("a",            "normalize names"),
                ("c",            "regenerate cite key"),
                ("Tab",          "assign groups"),
                ("o / w",        "open file / web (w fetches DOI if absent)"),
                ("B / L",        "toggle braces / LaTeX"),
                ("u",            "undo"),
                ("Esc",          "back to list"),
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
        (
            "Settings  ( S )",
            &[
                ("j / k",    "navigate"),
                ("g / G",    "top / bottom"),
                ("Ctrl-F/B", "page down / up"),
                ("Enter / Space", "toggle"),
                ("e",        "edit value"),
                ("a",        "add field group"),
                ("x",        "delete field group"),
                ("r",        "rename field group"),
                ("E / I",    "export / import config"),
                ("Esc",      "close"),
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
