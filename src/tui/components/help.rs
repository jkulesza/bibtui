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
                ("C",               "regenerate all cite keys"),
                ("M",               "name disambiguator"),
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
