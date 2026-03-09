use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

use crate::app::App;
use crate::tui::components::dialog::render_dialog;
use crate::tui::components::field_editor::render_field_editor;
use crate::tui::components::settings::render_settings;

pub fn render_settings_screen(f: &mut Frame, app: &mut App) {
    let area = f.area();

    let vertical = Layout::vertical([
        Constraint::Min(1),    // settings list + description
        Constraint::Length(1), // status bar
    ])
    .split(area);

    if let Some(ref mut state) = app.settings_state {
        render_settings(f, vertical[0], state, &app.theme);
    }

    // Minimal status bar
    let status_text = app
        .status_message
        .as_deref()
        .unwrap_or("Settings  (changes applied immediately to this session)");
    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            format!(" {}", status_text),
            app.theme.status_bar,
        ))),
        vertical[1],
    );

    // Overlays
    if let Some(ref editor_state) = app.field_editor_state {
        // When editing a citekey template, show the token/modifier reference
        // panel below the field editor.
        let editing_citekey = app
            .settings_state
            .as_ref()
            .map(|s| s.selected_is_citekey_template())
            .unwrap_or(false);

        if editing_citekey {
            let editor_w = (area.width.saturating_sub(4)).min(70);
            let editor_x = area.x + (area.width.saturating_sub(editor_w)) / 2;
            let editor_y = area.y + area.height / 2 - 2;
            // Help panel sits immediately below the 4-row field editor.
            let help_y = editor_y + 4;
            let help_h = area.height.saturating_sub(help_y).min(15);
            if help_h >= 4 {
                let help_area = Rect::new(editor_x, help_y, editor_w, help_h);
                render_citekey_help(f, help_area, app);
            }
        }

        render_field_editor(f, area, editor_state, &app.theme);
    }
    if let Some(ref mut dialog) = app.dialog_state {
        render_dialog(f, area, dialog, &app.theme);
    }
}

// ── Citekey template quick-reference panel ───────────────────────────────────

fn render_citekey_help(f: &mut Frame, area: Rect, app: &App) {
    f.render_widget(Clear, area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(app.theme.border)
        .title(" Citekey Template Reference ");
    let inner = block.inner(area);
    f.render_widget(block, area);

    if inner.height == 0 || inner.width == 0 {
        return;
    }

    // Split inner area into two columns: tokens (left) and modifiers (right).
    let half = inner.width / 2;
    let cols = Layout::horizontal([
        Constraint::Length(half),
        Constraint::Min(1),
    ])
    .split(inner);

    // ── Left column: tokens ──────────────────────────────────────────────────
    let kw = app.theme.header;   // keyword style
    let dim = app.theme.label;   // description style

    let token_rows: &[(&str, &str)] = &[
        ("[auth]",        "first author last name"),
        ("[authN]",       "first N authors (e.g. [auth3])"),
        ("[authors]",     "all authors (EtAl for 3+)"),
        ("[year]",        "year field"),
        ("[shortyear]",   "last 2 digits of year"),
        ("[title]",       "first significant word"),
        ("[shorttitle]",  "first 3 significant words"),
        ("[journal]",     "journal field"),
        ("[booktitle]",   "booktitle field"),
        ("[firstpage]",   "first page from pages field"),
        ("[number]",      "number / report-number field"),
        ("[institution]", "institution field"),
        ("[<field>]",     "any BibTeX field by name"),
    ];

    let token_lines: Vec<Line> = std::iter::once(
        Line::from(Span::styled(" Tokens", kw))
    )
    .chain(token_rows.iter().map(|(token, desc)| {
        let col_w = (half as usize).saturating_sub(1);
        let token_w = 15usize;
        let desc_w = col_w.saturating_sub(token_w + 1);
        let desc_trunc: String = desc.chars().take(desc_w).collect();
        Line::from(vec![
            Span::styled(format!(" {:<w$}", token, w = token_w), kw),
            Span::styled(desc_trunc, dim),
        ])
    }))
    .collect();

    f.render_widget(Paragraph::new(token_lines), cols[0]);

    // ── Right column: modifiers + examples ──────────────────────────────────
    let modifier_rows: &[(&str, &str)] = &[
        (":upper",          "uppercase"),
        (":lower",          "lowercase"),
        (":abbr",           "first letter each word"),
        (":camel",          "capitalise each word"),
        (":(n)",            "truncate to n chars"),
        (":regex(pat,repl)","regex find/replace"),
    ];

    let example_rows: &[(&str, &str)] = &[
        ("[auth][year]",           "→ Smith2020"),
        ("[auth:upper][year]",     "→ SMITH2020"),
        ("[journal:abbr]",         "→ NSE"),
        ("[auth3][year]",          "→ SmithJonesWilliams2020"),
        ("[title:lower:(8)]",      "→ toward_e"),
        ("[auth][year:regex(^\\d\\d,,)]", "→ Smith24"),
    ];

    let col2_w = (cols[1].width as usize).saturating_sub(1);
    let mod_token_w = 18usize;

    let mut right_lines: Vec<Line> = vec![Line::from(Span::styled(" Modifiers", kw))];
    for (modifier, desc) in modifier_rows {
        let desc_w = col2_w.saturating_sub(mod_token_w + 1);
        let desc_trunc: String = desc.chars().take(desc_w).collect();
        right_lines.push(Line::from(vec![
            Span::styled(format!(" {:<w$}", modifier, w = mod_token_w), kw),
            Span::styled(desc_trunc, dim),
        ]));
    }

    let remaining = (inner.height as usize).saturating_sub(right_lines.len() + 1);
    if remaining >= 2 {
        right_lines.push(Line::from(Span::raw("")));
        right_lines.push(Line::from(Span::styled(" Examples", kw)));
        for (pat, result) in example_rows.iter().take(remaining.saturating_sub(1)) {
            let pat_w = col2_w.saturating_sub(12);
            let pat_trunc: String = pat.chars().take(pat_w).collect();
            right_lines.push(Line::from(vec![
                Span::styled(format!(" {:<w$}", pat_trunc, w = pat_w), kw),
                Span::styled(format!(" {}", result), dim),
            ]));
        }
    }

    f.render_widget(
        Paragraph::new(right_lines)
            .style(Style::default()),
        cols[1],
    );
}
