use ratatui::layout::{Constraint, Layout};
use ratatui::Frame;

use crate::app::App;
use crate::tui::components::entry_detail::render_entry_detail;
use crate::tui::components::field_editor::render_field_editor;
use crate::tui::components::status_bar::render_status_bar;

pub fn render_edit_screen(f: &mut Frame, app: &mut App) {
    let area = f.area();

    let vertical = Layout::vertical([
        Constraint::Min(1),    // detail view
        Constraint::Length(1), // status bar
    ])
    .split(area);

    if let Some(key) = &app.detail_entry_key {
        if let Some(entry) = app.database.entries.get(key) {
            let entry = entry.clone();
            if let Some(ref mut detail_state) = app.detail_state {
                render_entry_detail(f, vertical[0], &entry, detail_state, &app.theme, app.show_braces);
            }
        }
    }

    // Status bar
    render_status_bar(
        f,
        vertical[1],
        app.database.entries.len(),
        None,
        &app.config.display.default_sort.field,
        app.config.display.default_sort.ascending,
        app.dirty,
        app.status_message.as_deref(),
        &app.theme,
    );

    // Field editor overlay
    if let Some(ref editor_state) = app.field_editor_state {
        render_field_editor(f, area, editor_state, &app.theme);
    }

    // Dialog overlay
    if let Some(ref mut dialog) = app.dialog_state {
        crate::tui::components::dialog::render_dialog(f, area, dialog, &app.theme);
    }
}
