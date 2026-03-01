use ratatui::layout::{Constraint, Layout};
use ratatui::Frame;

use crate::app::App;
use crate::bib::model::Entry;
use crate::tui::keybindings::InputMode;

pub fn render_main_screen(f: &mut Frame, app: &mut App) {
    let area = f.area();

    // Layout: [groups | entries] / [search_or_command] / [status]
    let vertical = Layout::vertical([
        Constraint::Min(1),    // main content
        Constraint::Length(1), // search bar or command
        Constraint::Length(1), // status bar
    ])
    .split(area);

    let main_area = vertical[0];
    let search_area = vertical[1];
    let status_area = vertical[2];

    // Collect entry keys to avoid borrow conflicts
    let visible_keys: Vec<String> = if let Some(ref indices) = app.filtered_indices {
        indices
            .iter()
            .filter_map(|&i| app.sorted_keys.get(i).cloned())
            .collect()
    } else {
        app.sorted_keys.clone()
    };

    // Main area: optional groups sidebar + entry list
    if app.config.display.show_groups && app.show_groups {
        let sidebar_width = app.config.display.group_sidebar_width;
        let horizontal = Layout::horizontal([
            Constraint::Length(sidebar_width),
            Constraint::Min(1),
        ])
        .split(main_area);

        let total_entries = app.database.entries.len();
        let is_groups_focused = app.focus == Focus::Groups;
        let is_list_focused = app.focus == Focus::List;

        crate::tui::components::group_tree::render_group_tree(
            f,
            horizontal[0],
            &mut app.group_tree_state,
            &app.theme,
            is_groups_focused,
            total_entries,
        );

        let entries: Vec<&Entry> = visible_keys
            .iter()
            .filter_map(|k| app.database.entries.get(k))
            .collect();

        crate::tui::components::entry_list::render_entry_list(
            f,
            horizontal[1],
            &entries,
            &mut app.entry_list_state,
            &app.config.display.columns,
            &app.theme,
            is_list_focused,
            app.show_braces,
        );
    } else {
        let entries: Vec<&Entry> = visible_keys
            .iter()
            .filter_map(|k| app.database.entries.get(k))
            .collect();

        crate::tui::components::entry_list::render_entry_list(
            f,
            main_area,
            &entries,
            &mut app.entry_list_state,
            &app.config.display.columns,
            &app.theme,
            true,
            app.show_braces,
        );
    }

    // Search bar or command palette
    match &app.mode {
        InputMode::Search => {
            crate::tui::components::search_bar::render_search_bar(
                f,
                search_area,
                &app.search_bar_state,
                true,
                &app.theme,
            );
        }
        InputMode::Command => {
            crate::tui::components::command_palette::render_command_palette(
                f,
                search_area,
                &app.command_palette_state,
                &app.theme,
            );
        }
        _ => {
            crate::tui::components::search_bar::render_search_bar(
                f,
                search_area,
                &app.search_bar_state,
                false,
                &app.theme,
            );
        }
    }

    // Status bar
    let entry_count = visible_keys.len();
    crate::tui::components::status_bar::render_status_bar(
        f,
        status_area,
        entry_count,
        app.group_tree_state.active_group.as_deref(),
        &app.config.display.default_sort.field,
        app.config.display.default_sort.ascending,
        app.dirty,
        app.status_message.as_deref(),
        &app.theme,
    );

    // Dialog overlay
    if let Some(ref mut dialog) = app.dialog_state {
        crate::tui::components::dialog::render_dialog(f, area, dialog, &app.theme);
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Focus {
    List,
    Groups,
}
