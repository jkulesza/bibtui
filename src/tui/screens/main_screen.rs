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

    // Build the visible entry list without cloning the full key vec.
    // Using references into sorted_keys avoids per-frame String allocations.
    let entries: Vec<&Entry> = if let Some(ref indices) = app.filtered_indices {
        indices
            .iter()
            .filter_map(|&i| app.sorted_keys.get(i))
            .filter_map(|k| app.database.entries.get(k))
            .collect()
    } else {
        app.sorted_keys
            .iter()
            .filter_map(|k| app.database.entries.get(k))
            .collect()
    };
    let entry_count = entries.len();

    let bib_dir = crate::util::open::effective_file_dir(
        &app.bib_path,
        app.database.jabref_meta.file_directory.as_deref(),
    );

    // Main area: optional groups sidebar + entry list
    if app.show_groups {
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

        crate::tui::components::entry_list::render_entry_list(
            f,
            horizontal[1],
            &entries,
            &mut app.entry_list_state,
            &app.config.display.columns,
            &app.theme,
            is_list_focused,
            app.show_braces,
            app.render_latex,
            app.config.display.abbreviate_authors,
            &bib_dir,
        );
    } else {
        crate::tui::components::entry_list::render_entry_list(
            f,
            main_area,
            &entries,
            &mut app.entry_list_state,
            &app.config.display.columns,
            &app.theme,
            true,
            app.show_braces,
            app.render_latex,
            app.config.display.abbreviate_authors,
            &bib_dir,
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

    // Field editor overlay (e.g. group name input)
    if let Some(ref editor_state) = app.field_editor_state {
        crate::tui::components::field_editor::render_field_editor(f, area, editor_state, &app.theme);
    }

    // Citation preview overlay
    if let Some(ref preview_state) = app.citation_preview_state {
        crate::tui::components::citation_preview::render_citation_preview(
            f, area, preview_state, &app.theme,
        );
    }

    // Validate results overlay
    if let Some(ref mut vrs) = app.validate_results_state {
        crate::tui::components::validate_results::render_validate_results(
            f, area, vrs, &app.theme,
        );
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Focus {
    List,
    Groups,
}
