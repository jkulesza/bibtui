use ratatui::layout::{Constraint, Layout};
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
        ratatui::widgets::Paragraph::new(ratatui::text::Line::from(
            ratatui::text::Span::styled(
                format!(" {}", status_text),
                app.theme.status_bar,
            ),
        )),
        vertical[1],
    );

    // Overlays
    if let Some(ref editor_state) = app.field_editor_state {
        render_field_editor(f, area, editor_state, &app.theme);
    }
    if let Some(ref mut dialog) = app.dialog_state {
        render_dialog(f, area, dialog, &app.theme);
    }
}
