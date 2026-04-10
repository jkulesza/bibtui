use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use anyhow::{Context, Result};
use crossterm::event::{Event, KeyCode, KeyEvent};
use indexmap::IndexMap;

use crate::bib::citekey::generate_citekey;
use crate::bib::normalize::{
    cleanup_url, escape_ampersands, escape_underscores, latex_cleanup,
    normalize_date, normalize_isbn, normalize_month, normalize_page_numbers,
    ordinals_to_superscript, unicode_to_latex,
};
use crate::bib::jabref::serialize_group_tree;
use crate::bib::model::*;
use crate::util::open::{effective_file_dir, parse_file_field, serialize_file_field};
use crate::tui::components::citation_preview::CitationPreviewState;
use crate::tui::components::help::HelpState;
use crate::tui::components::validate_results::{Violation, ValidateResultsState};
use crate::util::citation::format_citation;
use crate::util::export::{export_csl_json, export_ris};
use crate::bib::parser::{build_database, parse_bib_file};
use crate::bib::writer::{normalize_blank_lines, serialize_entry, write_bib_file};
use crate::config::schema::{Config, SortConfig};
use crate::search::engine::SearchEngine;
use crate::search::filter::filter_by_group;
use crate::tui::components::command_palette::CommandPaletteState;
use crate::tui::components::dialog::{DialogKind, DialogState};
use crate::tui::components::entry_detail::EntryDetailState;
use crate::tui::components::entry_list::EntryListState;
use crate::tui::components::field_editor::{EditingMode, FieldEditorState};
use crate::tui::components::group_tree::GroupTreeState;
use crate::tui::components::search_bar::SearchBarState;
use crate::tui::event::poll_event;
use crate::tui::keybindings::{build_user_bindings, map_key, InputMode};
use crate::tui::components::settings::{SettingValue, SettingsState};
use crate::tui::screens::main_screen::{render_main_screen, Focus};
use crate::tui::screens::edit_screen::render_edit_screen;
use crate::tui::screens::settings_screen::render_settings_screen;
use crate::tui::theme::Theme;
use crate::tui::Term;

mod actions;
pub use actions::Action;
use actions::{UndoItem, PendingAction, MAX_UNDO};

pub struct App {
    pub database: Database,
    pub config: Config,
    pub theme: Theme,
    pub bib_path: PathBuf,
    pub mode: InputMode,
    pub focus: Focus,
    pub show_groups: bool,
    pub show_braces: bool,
    pub render_latex: bool,
    pub dirty: bool,
    pub should_quit: bool,
    pub status_message: Option<String>,
    pub last_key: Option<char>,
    pub second_last_key: Option<char>,

    // Component states
    pub entry_list_state: EntryListState,
    pub group_tree_state: GroupTreeState,
    pub search_bar_state: SearchBarState,
    pub detail_state: Option<EntryDetailState>,
    pub detail_entry_key: Option<String>,
    pub field_editor_state: Option<FieldEditorState>,
    pub dialog_state: Option<DialogState>,
    pub command_palette_state: CommandPaletteState,
    pub citation_preview_state: Option<CitationPreviewState>,
    pub settings_state: Option<SettingsState>,
    pub last_settings_cursor: usize,
    pub validate_results_state: Option<ValidateResultsState>,
    pub help_state: Option<HelpState>,

    // Search / sort
    pub search_engine: SearchEngine,
    pub filtered_indices: Option<Vec<usize>>,
    pub sorted_keys: Vec<String>,
    /// The sort that was active when the file was loaded (or last explicitly set
    /// as the default). ESC in Normal mode restores this.
    pub default_sort: SortConfig,

    // Pending action context
    pending_action: Option<PendingAction>,
    /// Tab-completion candidates for path editors (cycles on repeated Tab)
    path_completions: Vec<String>,
    path_completion_idx: usize,
    /// Raw indices of entries deleted this session (for sync on save)
    deleted_raw_indices: Vec<usize>,

    // Undo
    undo_stack: Vec<UndoItem>,
    /// Undo-stack depth at the time of the last save.  `None` when the save
    /// point has been pushed off the end of the capped stack (i.e. it can
    /// never be reached by undoing).
    save_generation: Option<usize>,

    // Import
    /// Receives the result of a background DOI/URL import fetch.
    pending_import: Option<mpsc::Receiver<crate::util::import::ImportResult>>,
    /// Background DOI-from-metadata lookup: (entry_key, receiver of (doi, url) or error).
    pending_doi_fetch: Option<(String, mpsc::Receiver<Result<(String, String), String>>)>,

    /// Compiled user keybinding overrides from config.  Checked before
    /// built-in defaults in `handle_key`.
    user_bindings: Vec<(InputMode, KeyEvent, Action)>,
}


impl App {
    pub fn new(bib_path: PathBuf, config: Config) -> Result<Self> {
        let content = std::fs::read_to_string(&bib_path)
            .with_context(|| format!("Failed to read {}", bib_path.display()))?;
        let raw = parse_bib_file(&content)
            .with_context(|| format!("Failed to parse {}", bib_path.display()))?;
        let database = build_database(raw);

        let theme = Theme::from_config(&config.theme);
        let group_tree_state = GroupTreeState::new(&database.groups);

        // Build sorted keys
        let sorted_keys = sort_entries(&database.entries, &config);

        let show_groups = config.display.show_groups;
        let show_braces = config.display.show_braces;
        let render_latex = config.display.render_latex;
        let user_bindings = build_user_bindings(&config.keybindings);

        let default_sort = config.display.default_sort.clone();
        let app = App {
            database,
            config,
            theme,
            bib_path,
            mode: InputMode::Normal,
            focus: Focus::List,
            show_groups,
            show_braces,
            render_latex,
            dirty: false,
            should_quit: false,
            status_message: None,
            last_key: None,
            second_last_key: None,
            entry_list_state: EntryListState::new(),
            group_tree_state,
            search_bar_state: SearchBarState::new(),
            detail_state: None,
            detail_entry_key: None,
            field_editor_state: None,
            dialog_state: None,
            command_palette_state: CommandPaletteState::new(),
            citation_preview_state: None,
            settings_state: None,
            last_settings_cursor: 0,
            validate_results_state: None,
            help_state: None,
            search_engine: SearchEngine::new(),
            filtered_indices: None,
            sorted_keys,
            default_sort,
            pending_action: None,
            path_completions: Vec::new(),
            path_completion_idx: 0,
            deleted_raw_indices: Vec::new(),
            undo_stack: Vec::new(),
            save_generation: Some(0),
            pending_import: None,
            pending_doi_fetch: None,
            user_bindings,
        };

        Ok(app)
    }

    /// Create an empty app when no bib file is provided.
    /// The path prompt is shown immediately on first render.
    pub fn new_empty(config: Config) -> Result<Self> {
        let database = build_database(crate::bib::model::RawBibFile { items: vec![] });
        let theme = Theme::from_config(&config.theme);
        let group_tree_state = GroupTreeState::new(&database.groups);
        let sorted_keys = sort_entries(&database.entries, &config);
        let show_groups = config.display.show_groups;
        let show_braces = config.display.show_braces;
        let render_latex = config.display.render_latex;
        let user_bindings = build_user_bindings(&config.keybindings);
        let default_sort = config.display.default_sort.clone();

        let app = App {
            database,
            config,
            theme,
            bib_path: PathBuf::new(), // filled in when the user confirms a path
            mode: InputMode::Editing,
            focus: Focus::List,
            show_groups,
            show_braces,
            render_latex,
            dirty: false,
            should_quit: false,
            status_message: None,
            last_key: None,
            second_last_key: None,
            entry_list_state: EntryListState::new(),
            group_tree_state,
            search_bar_state: SearchBarState::new(),
            detail_state: None,
            detail_entry_key: None,
            field_editor_state: Some(FieldEditorState::for_path(
                "Save new library as",
                "",
            )),
            dialog_state: None,
            command_palette_state: CommandPaletteState::new(),
            citation_preview_state: None,
            settings_state: None,
            last_settings_cursor: 0,
            validate_results_state: None,
            help_state: None,
            search_engine: SearchEngine::new(),
            filtered_indices: None,
            sorted_keys,
            default_sort,
            pending_action: Some(PendingAction::NewFile),
            path_completions: Vec::new(),
            path_completion_idx: 0,
            deleted_raw_indices: Vec::new(),
            undo_stack: Vec::new(),
            save_generation: Some(0),
            pending_import: None,
            pending_doi_fetch: None,
            user_bindings,
        };

        Ok(app)
    }

    pub fn run(&mut self, terminal: &mut Term) -> Result<()> {
        // Always draw on the first frame, then only when something changes.
        let mut needs_redraw = true;
        while !self.should_quit {
            if needs_redraw {
                terminal.draw(|f| self.render(f))?;
                needs_redraw = false;
            }
            if let Some(event) = poll_event(Duration::from_millis(100))? {
                self.handle_event(event);
                needs_redraw = true;
            }
            // Poll background import task
            if self.pending_import.is_some() {
                match self.pending_import.as_ref().unwrap().try_recv() {
                    Ok(result) => {
                        self.pending_import = None;
                        self.handle_import_result(result);
                        needs_redraw = true;
                    }
                    Err(mpsc::TryRecvError::Empty) => {
                        // Still working; redraw to show "Fetching…" status
                        needs_redraw = true;
                    }
                    Err(mpsc::TryRecvError::Disconnected) => {
                        self.pending_import = None;
                        self.status_message =
                            Some("Import failed: fetcher thread disconnected".to_string());
                        needs_redraw = true;
                    }
                }
            }
            // Poll background DOI-from-metadata lookup
            if self.pending_doi_fetch.is_some() {
                let result = self
                    .pending_doi_fetch
                    .as_ref()
                    .unwrap()
                    .1
                    .try_recv();
                match result {
                    Ok(fetch_result) => {
                        let entry_key = self.pending_doi_fetch.take().unwrap().0;
                        self.handle_doi_fetch_result(entry_key, fetch_result);
                        needs_redraw = true;
                    }
                    Err(mpsc::TryRecvError::Empty) => {
                        needs_redraw = true;
                    }
                    Err(mpsc::TryRecvError::Disconnected) => {
                        self.pending_doi_fetch = None;
                        self.status_message =
                            Some("DOI lookup failed: thread disconnected".to_string());
                        needs_redraw = true;
                    }
                }
            }
        }
        Ok(())
    }

    fn render(&mut self, f: &mut ratatui::Frame) {
        if self.settings_state.is_some() {
            render_settings_screen(f, self);
        } else if self.detail_state.is_some() {
            render_edit_screen(f, self);
        } else {
            render_main_screen(f, self);
        }
    }

    fn handle_event(&mut self, event: Event) {
        match event {
            Event::Key(key) => self.handle_key(key),
            Event::Resize(_, _) => {} // Ratatui handles resize automatically
            _ => {}
        }
    }

    fn handle_key(&mut self, key: KeyEvent) {
        // Track last key for multi-key combos (gg, dd, yy, dt{c}, df{c}, …)
        let last = self.last_key;
        let second_last = self.second_last_key;

        // Update key-history tracking.  Only char keys advance the chain;
        // non-char keys (arrows, Esc, …) reset both slots.
        match key.code {
            KeyCode::Char(c) => {
                self.second_last_key = self.last_key;
                self.last_key = Some(c);
            }
            _ => {
                self.second_last_key = None;
                self.last_key = None;
            }
        }

        let is_message_dialog = matches!(
            self.dialog_state.as_ref().map(|d| &d.kind),
            Some(DialogKind::Message { .. })
        );
        let edit_normal = matches!(self.mode, InputMode::Editing)
            && self
                .field_editor_state
                .as_ref()
                .map(|e| e.editing_mode == EditingMode::Normal && !e.editing_name)
                .unwrap_or(false);

        // User-configured bindings override built-in defaults.
        let current_mode = self.mode.clone();
        if let Some(action) = self.user_bindings.iter()
            .find(|(m, k, _)| *m == current_mode && *k == key)
            .map(|(_, _, a)| a.clone())
        {
            self.handle_action(action);
            return;
        }

        if let Some(action) = map_key(key, &self.mode, second_last, last, is_message_dialog, edit_normal) {
            self.handle_action(action);
        }
    }

    fn handle_action(&mut self, action: Action) {
        // Clear status message on any action
        self.status_message = None;

        match action {
            Action::MoveDown => {
                if let Some(ref mut vrs) = self.validate_results_state {
                    // 24 is a safe inner-height fallback; render clamps anyway
                    let total = vrs.violations.len() * 4;
                    vrs.scroll_down(24, total);
                } else {
                    self.move_cursor(1);
                    if self.citation_preview_state.is_some() {
                        self.show_citation_preview();
                    }
                }
            }
            Action::MoveUp => {
                if let Some(ref mut vrs) = self.validate_results_state {
                    vrs.scroll_up();
                } else {
                    self.move_cursor(-1);
                    if self.citation_preview_state.is_some() {
                        self.show_citation_preview();
                    }
                }
            }
            Action::MoveToTop => self.move_to_top(),
            Action::MoveToBottom => self.move_to_bottom(),
            Action::PageDown => self.move_cursor(20),
            Action::PageUp => self.move_cursor(-20),
            Action::ResetSort => {
                if self.filtered_indices.is_some() {
                    // Active search filter: ESC clears it and returns to full list
                    self.filtered_indices = None;
                    self.search_bar_state.clear();
                    self.entry_list_state.select(0);
                    self.status_message = Some("Search cleared".to_string());
                } else {
                    let current = &self.config.display.default_sort;
                    if current.field != self.default_sort.field || current.ascending != self.default_sort.ascending {
                        self.config.display.default_sort = self.default_sort.clone();
                        self.sorted_keys = sort_entries(&self.database.entries, &self.config);
                        self.entry_list_state.select(0);
                        let dir = if self.default_sort.ascending { "↑" } else { "↓" };
                        self.status_message = Some(format!(
                            "Sort reset to default: {} {}",
                            self.default_sort.field, dir
                        ));
                    }
                }
            }
            Action::EnterSearch => {
                self.mode = InputMode::Search;
                self.search_bar_state.clear();
            }
            Action::ExitSearch => {
                self.mode = InputMode::Normal;
                self.search_bar_state.clear();
                self.filtered_indices = None;
                self.entry_list_state.select(0);
            }
            Action::ConfirmSearch => {
                self.mode = InputMode::Normal;
                // Keep filtered results
            }
            Action::SearchChar(c) => {
                self.search_bar_state.push_char(c);
                self.update_search();
            }
            Action::SearchBackspace => {
                self.search_bar_state.backspace();
                self.update_search();
            }
            Action::OpenDetail => self.open_detail(),
            Action::CloseDetail => {
                // If a search is active, Esc first clears the search; second Esc closes detail.
                let has_search = self.detail_state.as_ref()
                    .map(|d| !d.search_query.is_empty())
                    .unwrap_or(false);
                if has_search {
                    if let Some(ref mut detail) = self.detail_state {
                        detail.clear_search();
                    }
                } else {
                    self.close_detail();
                }
            }
            Action::EnterDetailSearch => {
                if let Some(ref mut detail) = self.detail_state {
                    detail.clear_search();
                }
                self.mode = InputMode::DetailSearch;
            }
            Action::ExitDetailSearch => {
                self.mode = InputMode::Detail;
            }
            Action::DetailSearchChar(c) => {
                if let Some(ref mut detail) = self.detail_state {
                    detail.push_search_char(c);
                }
            }
            Action::DetailSearchBackspace => {
                if let Some(ref mut detail) = self.detail_state {
                    detail.search_backspace();
                    if detail.search_query.is_empty() {
                        self.mode = InputMode::Detail;
                    }
                }
            }
            Action::DetailNextMatch => {
                if let Some(ref mut detail) = self.detail_state {
                    detail.next_match();
                }
            }
            Action::DetailPrevMatch => {
                if let Some(ref mut detail) = self.detail_state {
                    detail.prev_match();
                }
            }
            Action::EditField => self.start_edit_field(),
            Action::AddField => {
                self.field_editor_state = Some(FieldEditorState::new_field());
                self.mode = InputMode::Editing;
                // (completions start empty; they populate as the user types a prefix)
            }
            Action::AddFileAttachment => self.start_add_file_attachment(),
            Action::DeleteField => self.delete_field(),
            Action::EditGroups => self.start_edit_groups(),
            Action::RegenCitekey => self.regen_citekey(),
            Action::RegenAllCitekeys => self.regen_all_citekeys(),
            Action::ConfirmEdit => self.confirm_edit(),
            Action::CancelEdit => {
                let is_new_file =
                    matches!(self.pending_action, Some(PendingAction::NewFile));
                self.field_editor_state = None;
                self.pending_action = None;
                if is_new_file {
                    // No path chosen — nothing to work with; exit cleanly.
                    self.should_quit = true;
                } else {
                    self.mode = if self.settings_state.is_some() {
                        InputMode::Settings
                    } else if self.detail_state.is_some() {
                        InputMode::Detail
                    } else {
                        InputMode::Normal
                    };
                }
            }
            Action::EditChar(c) => {
                if let Some(ref mut editor) = self.field_editor_state {
                    editor.push_char(c);
                }
                self.update_field_completions();
            }
            Action::EditBackspace => {
                if let Some(ref mut editor) = self.field_editor_state {
                    editor.backspace();
                }
                self.update_field_completions();
            }
            Action::EditDelete => {
                if let Some(ref mut editor) = self.field_editor_state {
                    editor.save_undo_snapshot();
                    editor.delete();
                    // In Normal mode, clamp cursor if we deleted the last char
                    if editor.editing_mode == EditingMode::Normal
                        && !editor.value.is_empty()
                        && editor.cursor >= editor.value.len()
                    {
                        editor.cursor = editor
                            .value
                            .char_indices()
                            .last()
                            .map(|(i, _)| i)
                            .unwrap_or(0);
                    }
                }
                self.update_field_completions();
            }
            Action::EditCursorLeft => {
                if let Some(ref mut editor) = self.field_editor_state {
                    if editor.is_month {
                        editor.month_navigate(-1);
                    } else {
                        editor.cursor_left();
                    }
                }
            }
            Action::EditCursorRight => {
                if let Some(ref mut editor) = self.field_editor_state {
                    if editor.is_month {
                        editor.month_navigate(1);
                    } else {
                        editor.cursor_right();
                    }
                }
            }
            Action::EditCursorUp => {
                if let Some(ref mut editor) = self.field_editor_state {
                    if editor.is_month {
                        editor.month_navigate(-6);
                    }
                }
            }
            Action::EditCursorDown => {
                if let Some(ref mut editor) = self.field_editor_state {
                    if editor.is_month {
                        editor.month_navigate(6);
                    }
                }
            }
            Action::EditCursorHome => {
                if let Some(ref mut editor) = self.field_editor_state {
                    editor.cursor_home();
                }
            }
            Action::EditCursorEnd => {
                if let Some(ref mut editor) = self.field_editor_state {
                    editor.cursor_end();
                }
            }
            Action::AddEntry => {
                if self.focus == Focus::Groups && self.show_groups {
                    self.start_add_group();
                } else {
                    self.start_add_entry();
                }
            }
            Action::DeleteEntry => {
                if self.focus == Focus::Groups && self.show_groups {
                    self.start_delete_group();
                } else {
                    self.start_delete_entry();
                }
            }
            Action::DuplicateEntry => self.duplicate_entry(),
            Action::YankCitekey => self.yank_citekey(),
            Action::ToggleGroups => {
                self.show_groups = !self.show_groups;
            }
            Action::FocusGroups => {
                self.show_groups = true;
                self.focus = Focus::Groups;
            }
            Action::FocusList => {
                self.focus = Focus::List;
            }
            // fixes #11: Space only previews, never selects group
            Action::ShowCitationPreview => {
                self.show_citation_preview();
            }
            Action::CloseCitationPreview => {
                self.citation_preview_state = None;
                self.mode = InputMode::Normal;
            }
            Action::YankCitationPreview => {
                if let Some(state) = &self.citation_preview_state {
                    let text = state.citation.clone();
                    let key = state.entry_key.clone();
                    match crate::util::clipboard::copy_to_clipboard(&text) {
                        Ok(()) => {
                            self.status_message =
                                Some(format!("Copied citation for '{}' to clipboard", key))
                        }
                        Err(e) => {
                            self.status_message = Some(format!("Clipboard error: {}", e))
                        }
                    }
                }
            }
            Action::Validate => {
                let violations = self.compute_violations();
                let count = violations.len();
                self.validate_results_state = Some(ValidateResultsState::new(violations));
                self.mode = InputMode::ValidateResults;
                if count == 0 {
                    self.status_message = Some("All entries are valid".to_string());
                } else {
                    self.status_message = Some(format!(
                        "{} field(s) would change on save",
                        count
                    ));
                }
            }
            Action::CloseValidateResults => {
                self.validate_results_state = None;
                self.mode = InputMode::Normal;
            }
            Action::EnterCommand => {
                self.mode = InputMode::Command;
                self.command_palette_state.clear();
            }
            Action::ExitCommand => {
                self.mode = InputMode::Normal;
            }
            Action::ExecuteCommand => self.execute_command(),
            Action::CommandChar(c) => {
                self.command_palette_state.push_char(c);
                self.update_sort_completions();
            }
            Action::CommandBackspace => {
                self.command_palette_state.backspace();
                if self.command_palette_state.input.is_empty() {
                    self.mode = InputMode::Normal;
                }
                self.update_sort_completions();
            }
            Action::CommandTabComplete => self.do_sort_tab_complete(),
            Action::DialogConfirm => self.handle_dialog_confirm(),
            Action::DialogCancel => {
                self.dialog_state = None;
                self.pending_action = None;
                self.mode = if self.detail_state.is_some() {
                    InputMode::Detail
                } else {
                    InputMode::Normal
                };
            }
            Action::DialogToggle => {
                if let Some(ref mut dialog) = self.dialog_state {
                    dialog.toggle_selected();
                }
            }
            Action::DialogYank => {
                if let Some(ref dialog) = self.dialog_state {
                    if let DialogKind::Message { message, .. } = &dialog.kind {
                        let text = message.clone();
                        match crate::util::clipboard::copy_to_clipboard(&text) {
                            Ok(()) => self.status_message = Some("Error message copied to clipboard".to_string()),
                            Err(e) => self.status_message = Some(format!("Clipboard error: {}", e)),
                        }
                    }
                }
            }
            Action::ShowHelp => {
                self.help_state = Some(HelpState);
                self.mode = InputMode::Help;
            }
            Action::CloseHelp => {
                self.help_state = None;
                self.mode = InputMode::Normal;
            }

            // ── Vim modal editing ──
            Action::EditUndo
            | Action::EditPut
            | Action::EditYank
            | Action::EditEnterNormal
            | Action::EditEnterInsert
            | Action::EditEnterInsertAfter
            | Action::EditEnterInsertAtEnd
            | Action::EditEnterInsertAtHome
            | Action::EditEnterReplace
            | Action::EditMoveWordFwd
            | Action::EditMoveWordBwd
            | Action::EditMoveWordEnd
            | Action::EditMoveBigWordFwd
            | Action::EditMoveBigWordBwd
            | Action::EditMoveBigWordEnd
            | Action::EditDeleteWordFwd
            | Action::EditDeleteToEnd
            | Action::EditChangeToEnd
            | Action::EditSubstituteChar
            | Action::EditSubstituteLine
            | Action::EditToggleCase
            | Action::EditReplaceChar(_)
            | Action::EditFindCharFwd(_)
            | Action::EditFindCharBwd(_)
            | Action::EditFindToCharFwd(_)
            | Action::EditFindToCharBwd(_)
            | Action::EditDeleteToChar(_)
            | Action::EditDeleteThroughChar(_)
            | Action::EditDeleteToCharBack(_)
            | Action::EditDeleteThroughCharBack(_)
            | Action::EditDeleteCharBack
            | Action::EditDeleteWordBack
            | Action::EditDeleteToHome
            | Action::EditConfirmAndMoveDown
            | Action::EditConfirmAndMoveUp => self.handle_field_editor_action(action),

            Action::TitlecaseField => self.titlecase_selected_field(),
            Action::NormalizeNames => self.normalize_names_field(),
            Action::ChangeEntryType => self.start_change_entry_type(),
            Action::OpenFile => self.open_file(),
            Action::OpenWeb => self.open_web(),
            Action::ToggleBraces => {
                self.show_braces = !self.show_braces;
                self.status_message = Some(if self.show_braces {
                    "Braces shown".to_string()
                } else {
                    "Braces hidden".to_string()
                });
            }
            Action::ToggleLatex => {
                self.render_latex = !self.render_latex;
                self.status_message = Some(if self.render_latex {
                    "LaTeX rendering on".to_string()
                } else {
                    "LaTeX rendering off".to_string()
                });
            }
            Action::Undo => self.undo(),

            // ── Import / Export ──
            Action::ImportEntry => self.start_import_entry(),
            Action::ExportJson => {
                self.field_editor_state =
                    Some(FieldEditorState::for_path("Export path (CSL-JSON)", "export.json"));
                self.pending_action = Some(PendingAction::ExportJson);
                self.mode = InputMode::Editing;
            }
            Action::ExportRis => {
                self.field_editor_state =
                    Some(FieldEditorState::for_path("Export path (RIS)", "export.ris"));
                self.pending_action = Some(PendingAction::ExportRis);
                self.mode = InputMode::Editing;
            }

            // ── Settings ──
            Action::EnterSettings
            | Action::ExitSettings
            | Action::SettingsMoveDown
            | Action::SettingsMoveUp
            | Action::SettingsMoveToTop
            | Action::SettingsMoveToBottom
            | Action::SettingsPageDown
            | Action::SettingsPageUp
            | Action::SettingsToggle
            | Action::SettingsEdit
            | Action::SettingsAddFieldGroup
            | Action::SettingsDeleteFieldGroup
            | Action::SettingsRenameFieldGroup
            | Action::SettingsExport
            | Action::SettingsImport => self.handle_settings_action(action),
            Action::EditTabComplete => self.do_field_tab_complete(),
        }
    }

    // ── Navigation ──

    fn move_cursor(&mut self, delta: i32) {
        // When a dialog is open, navigate its list instead
        if let Some(ref mut dialog) = self.dialog_state {
            let count = dialog.option_count();
            if count == 0 {
                return;
            }
            let current = dialog.selected() as i32;
            let new = (current + delta).clamp(0, count as i32 - 1) as usize;
            dialog.select(new);
            return;
        }

        if self.focus == Focus::Groups && self.show_groups {
            let count = self.group_tree_state.flat_items.len();
            if count == 0 {
                return;
            }
            let current = self.group_tree_state.selected() as i32;
            let new = (current + delta).clamp(0, count as i32 - 1) as usize;
            self.group_tree_state.select(new);
            return;
        }

        if let Some(ref mut detail) = self.detail_state {
            detail.move_selection(delta);
            return;
        }

        let count = self.visible_entry_count();
        if count == 0 {
            return;
        }
        let current = self.entry_list_state.selected() as i32;
        let new = (current + delta).clamp(0, count as i32 - 1) as usize;
        self.entry_list_state.select(new);
    }

    fn move_to_top(&mut self) {
        if self.focus == Focus::Groups && self.show_groups {
            self.group_tree_state.select(0);
        } else if let Some(ref mut detail) = self.detail_state {
            detail.move_to_top();
        } else {
            self.entry_list_state.select(0);
        }
    }

    fn move_to_bottom(&mut self) {
        if self.focus == Focus::Groups && self.show_groups {
            let count = self.group_tree_state.flat_items.len();
            if count > 0 {
                self.group_tree_state.select(count - 1);
            }
        } else if let Some(ref mut detail) = self.detail_state {
            detail.move_to_bottom();
        } else {
            let count = self.visible_entry_count();
            if count > 0 {
                self.entry_list_state.select(count - 1);
            }
        }
    }

    // ── Search ──

    fn update_search(&mut self) {
        let query = &self.search_bar_state.query;
        if query.is_empty() {
            self.filtered_indices = None;
            self.search_bar_state.result_count = self.sorted_keys.len();
            return;
        }

        let entries: Vec<&Entry> = self
            .sorted_keys
            .iter()
            .filter_map(|k| self.database.entries.get(k))
            .collect();

        let results = self.search_engine.search(&entries, query);
        self.search_bar_state.result_count = results.len();

        let indices: Vec<usize> = results.iter().map(|(i, _)| *i).collect();
        self.filtered_indices = Some(indices);
        self.entry_list_state.select(0);
    }

    // ── Visible entries ──

    /// True only when the field editor is open *for* a citekey template item
    /// (not for export/import path dialogs or other settings).
    pub fn is_editing_citekey_template(&self) -> bool {
        match &self.pending_action {
            Some(PendingAction::EditSetting { setting_id }) => {
                setting_id.starts_with("citekey.template.")
            }
            _ => false,
        }
    }

    #[allow(dead_code)]
    pub fn visible_entries(&self) -> Vec<&Entry> {
        if let Some(ref indices) = self.filtered_indices {
            indices
                .iter()
                .filter_map(|&i| {
                    self.sorted_keys
                        .get(i)
                        .and_then(|k| self.database.entries.get(k))
                })
                .collect()
        } else {
            self.sorted_keys
                .iter()
                .filter_map(|k| self.database.entries.get(k))
                .collect()
        }
    }

    pub fn visible_entry_count(&self) -> usize {
        if let Some(ref indices) = self.filtered_indices {
            indices.len()
        } else {
            self.sorted_keys.len()
        }
    }

    fn selected_entry_key(&self) -> Option<String> {
        let idx = self.entry_list_state.selected();
        if let Some(ref indices) = self.filtered_indices {
            indices
                .get(idx)
                .and_then(|&i| self.sorted_keys.get(i))
                .cloned()
        } else {
            self.sorted_keys.get(idx).cloned()
        }
    }

    // ── Detail view ──

    fn open_detail(&mut self) {
        if self.focus == Focus::Groups && self.show_groups {
            self.select_group();
            return;
        }
        if let Some(key) = self.selected_entry_key() {
            if let Some(entry) = self.database.entries.get(&key) {
                self.detail_state = Some(EntryDetailState::new(entry, self.config.field_groups.clone()));
                self.detail_entry_key = Some(key);
                self.mode = InputMode::Detail;
            }
        }
    }

    fn close_detail(&mut self) {
        self.detail_state = None;
        self.detail_entry_key = None;
        self.field_editor_state = None;
        self.mode = InputMode::Normal;
    }

    // ── Field editing ──

    fn start_edit_field(&mut self) {
        // When a FileEntry row is selected, edit just that file's path.
        if let Some(idx) = self.detail_state.as_ref().and_then(|d| d.selected_file_index()) {
            let key = match self.detail_entry_key.clone() { Some(k) => k, None => return };
            let file_value = self.database.entries.get(&key)
                .and_then(|e| e.fields.get("file").cloned())
                .unwrap_or_default();
            let files = parse_file_field(&file_value);
            let current_path = files.get(idx).map(|f| f.path.as_str()).unwrap_or("");
            self.field_editor_state = Some(FieldEditorState::for_path("File path", current_path));
            self.pending_action = Some(PendingAction::EditFileAttachment { entry_key: key, index: idx });
            self.mode = InputMode::Editing;
            return;
        }
        if let Some(ref detail) = self.detail_state {
            if let Some((field_name, field_value)) = detail.selected_field() {
                self.field_editor_state =
                    Some(FieldEditorState::new(field_name, field_value));
                self.mode = InputMode::Editing;
                self.update_field_completions();
            }
        }
    }

    fn start_add_file_attachment(&mut self) {
        let key = match self.detail_entry_key.clone() { Some(k) => k, None => return };
        self.field_editor_state = Some(FieldEditorState::for_path("New file path", ""));
        self.pending_action = Some(PendingAction::AddFileAttachment { entry_key: key });
        self.mode = InputMode::Editing;
    }

    fn confirm_edit(&mut self) {
        // Export settings ─────────────────────────────────────────────────────
        // New file: the user just entered a path for a brand-new library. ─────
        if matches!(self.pending_action, Some(PendingAction::NewFile)) {
            let path_str = self
                .field_editor_state
                .as_ref()
                .map(|e| e.value.trim().to_string())
                .unwrap_or_default();
            self.field_editor_state = None;
            self.pending_action = None;

            if path_str.is_empty() {
                // Re-prompt: the user must provide a path.
                self.status_message =
                    Some("Please enter a path for the new library.".to_string());
                self.field_editor_state =
                    Some(FieldEditorState::for_path("Save new library as", ""));
                self.pending_action = Some(PendingAction::NewFile);
                self.mode = InputMode::Editing;
                return;
            }

            // Append .bib if the user omitted it.
            let path_str = if path_str.ends_with(".bib") {
                path_str
            } else {
                format!("{}.bib", path_str)
            };

            self.bib_path = PathBuf::from(expand_tilde(&path_str));
            self.mode = InputMode::Normal;
            self.save(); // writes the (empty) file to disk
            return;
        }

        if matches!(self.pending_action, Some(PendingAction::ExportSettings)) {
            let path_str = self
                .field_editor_state
                .as_ref()
                .map(|e| e.value.trim().to_string())
                .unwrap_or_else(|| "bibtui.yaml".to_string());
            self.field_editor_state = None;
            self.pending_action = None;
            self.mode = InputMode::Settings;
            if !path_str.is_empty() {
                self.export_settings(&path_str);
            }
            return;
        }

        // Export as CSL-JSON ──────────────────────────────────────────────────
        if matches!(self.pending_action, Some(PendingAction::ExportJson)) {
            let path_str = self
                .field_editor_state
                .as_ref()
                .map(|e| e.value.trim().to_string())
                .unwrap_or_default();
            self.field_editor_state = None;
            self.pending_action = None;
            self.mode = InputMode::Normal;
            if !path_str.is_empty() {
                self.do_export_json(&path_str);
            }
            return;
        }

        // Export as RIS ───────────────────────────────────────────────────────
        if matches!(self.pending_action, Some(PendingAction::ExportRis)) {
            let path_str = self
                .field_editor_state
                .as_ref()
                .map(|e| e.value.trim().to_string())
                .unwrap_or_default();
            self.field_editor_state = None;
            self.pending_action = None;
            self.mode = InputMode::Normal;
            if !path_str.is_empty() {
                self.do_export_ris(&path_str);
            }
            return;
        }

        // Import entry from DOI/URL ───────────────────────────────────────────
        if matches!(self.pending_action, Some(PendingAction::ImportUrl)) {
            let doi_or_url = self
                .field_editor_state
                .as_ref()
                .map(|e| e.value.trim().to_string())
                .unwrap_or_default();
            self.field_editor_state = None;
            self.pending_action = None;
            self.mode = InputMode::Normal;
            if !doi_or_url.is_empty() {
                self.spawn_import(doi_or_url);
            }
            return;
        }

        // Import settings ─────────────────────────────────────────────────────
        if matches!(self.pending_action, Some(PendingAction::ImportSettings)) {
            let path_str = self
                .field_editor_state
                .as_ref()
                .map(|e| e.value.trim().to_string())
                .unwrap_or_default();
            self.field_editor_state = None;
            self.pending_action = None;
            self.mode = InputMode::Settings;
            if !path_str.is_empty() {
                self.import_settings(&path_str);
            }
            return;
        }

        // Edit a string setting ───────────────────────────────────────────────
        if matches!(self.pending_action, Some(PendingAction::EditSetting { .. })) {
            let setting_id = match self.pending_action.take() {
                Some(PendingAction::EditSetting { setting_id }) => setting_id,
                _ => return,
            };
            let new_val = self
                .field_editor_state
                .as_ref()
                .map(|e| e.value.clone())
                .unwrap_or_default();
            self.field_editor_state = None;
            self.mode = InputMode::Settings;
            if let Some(ref mut s) = self.settings_state {
                s.set_value(&setting_id, SettingValue::Str(new_val));
                s.apply_to_config(&mut self.config);
                self.sync_runtime_from_config();
            }
            return;
        }

        // Add a new field group ───────────────────────────────────────────────
        if matches!(self.pending_action, Some(PendingAction::AddFieldGroup)) {
            let name = self.field_editor_state.as_ref()
                .map(|e| e.value.trim().to_string()).unwrap_or_default();
            self.pending_action = None;
            self.field_editor_state = None;
            self.mode = InputMode::Settings;
            if !name.is_empty() {
                if let Some(ref mut s) = self.settings_state {
                    s.add_field_group(name);
                    s.apply_to_config(&mut self.config);
                }
                self.sync_runtime_from_config();
            }
            return;
        }

        // Edit field group fields ─────────────────────────────────────────────
        if matches!(self.pending_action, Some(PendingAction::EditFieldGroupFields { .. })) {
            let index = match self.pending_action.take() {
                Some(PendingAction::EditFieldGroupFields { index }) => index,
                _ => return,
            };
            let fields_csv = self.field_editor_state.as_ref()
                .map(|e| e.value.clone()).unwrap_or_default();
            self.field_editor_state = None;
            self.mode = InputMode::Settings;
            if let Some(ref mut s) = self.settings_state {
                s.set_field_group_fields(index, fields_csv);
                s.apply_to_config(&mut self.config);
            }
            self.sync_runtime_from_config();
            return;
        }

        // Rename a field group ────────────────────────────────────────────────
        if matches!(self.pending_action, Some(PendingAction::RenameFieldGroup { .. })) {
            let index = match self.pending_action.take() {
                Some(PendingAction::RenameFieldGroup { index }) => index,
                _ => return,
            };
            let name = self.field_editor_state.as_ref()
                .map(|e| e.value.trim().to_string()).unwrap_or_default();
            self.field_editor_state = None;
            self.mode = InputMode::Settings;
            if !name.is_empty() {
                if let Some(ref mut s) = self.settings_state {
                    s.set_field_group_name(index, name);
                    s.apply_to_config(&mut self.config);
                }
                self.sync_runtime_from_config();
            }
            return;
        }

        // Add a new display column ────────────────────────────────────────────
        if matches!(self.pending_action, Some(PendingAction::AddColumn)) {
            let input = self.field_editor_state.as_ref()
                .map(|e| e.value.trim().to_string()).unwrap_or_default();
            self.pending_action = None;
            self.field_editor_state = None;
            self.mode = InputMode::Settings;
            if !input.is_empty() {
                let (field, header) = parse_field_header(&input);
                if let Some(ref mut s) = self.settings_state {
                    s.add_column(field, header, "flex".to_string());
                    s.apply_to_config(&mut self.config);
                }
                self.sync_runtime_from_config();
            }
            return;
        }

        // Edit column width ───────────────────────────────────────────────────
        if matches!(self.pending_action, Some(PendingAction::EditColumnWidth { .. })) {
            let index = match self.pending_action.take() {
                Some(PendingAction::EditColumnWidth { index }) => index,
                _ => return,
            };
            let width_spec = self.field_editor_state.as_ref()
                .map(|e| e.value.trim().to_string()).unwrap_or_default();
            self.field_editor_state = None;
            self.mode = InputMode::Settings;
            if let Some(ref mut s) = self.settings_state {
                s.set_column_width(index, width_spec);
                s.apply_to_config(&mut self.config);
            }
            self.sync_runtime_from_config();
            return;
        }

        // Rename a display column ─────────────────────────────────────────────
        if matches!(self.pending_action, Some(PendingAction::RenameColumn { .. })) {
            let index = match self.pending_action.take() {
                Some(PendingAction::RenameColumn { index }) => index,
                _ => return,
            };
            let input = self.field_editor_state.as_ref()
                .map(|e| e.value.trim().to_string()).unwrap_or_default();
            self.field_editor_state = None;
            self.mode = InputMode::Settings;
            if !input.is_empty() {
                let (field, header) = parse_field_header(&input);
                if let Some(ref mut s) = self.settings_state {
                    s.set_column_name(index, field, header);
                    s.apply_to_config(&mut self.config);
                }
                self.sync_runtime_from_config();
            }
            return;
        }

        // Add a new file attachment ───────────────────────────────────────────
        if matches!(self.pending_action, Some(PendingAction::AddFileAttachment { .. })) {
            let entry_key = match self.pending_action.take() {
                Some(PendingAction::AddFileAttachment { entry_key }) => entry_key,
                _ => return,
            };
            let path_str = self.field_editor_state.as_ref()
                .map(|e| e.value.trim().to_string()).unwrap_or_default();
            self.field_editor_state = None;
            self.mode = InputMode::Detail;
            if !path_str.is_empty() {
                let abs_path = PathBuf::from(expand_tilde(&path_str));
                let file_type = abs_path
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|e| e.to_uppercase())
                    .unwrap_or_default();
                // Store a path relative to the JabRef fileDirectory (same convention
                // used by the import pipeline).  Falls back to absolute if the file
                // is outside that directory or canonicalization fails.
                let file_dir = effective_file_dir(
                    &self.bib_path,
                    self.database.jabref_meta.file_directory.as_deref(),
                );
                let stored_path = crate::util::open::make_relative(&file_dir, &abs_path)
                    .to_string_lossy()
                    .into_owned();
                let current = self.database.entries.get(&entry_key)
                    .and_then(|e| e.fields.get("file").cloned())
                    .unwrap_or_default();
                let mut files = parse_file_field(&current);
                files.push(crate::util::open::ParsedFile {
                    description: String::new(),
                    path: stored_path,
                    file_type,
                });
                let new_value = serialize_file_field(&files);
                self.push_undo(UndoItem::FieldChanged {
                    entry_key: entry_key.clone(),
                    field_name: "file".to_string(),
                    old_value: if current.is_empty() { None } else { Some(current) },
                });
                if let Some(entry) = self.database.entries.get_mut(&entry_key) {
                    entry.fields.insert("file".to_string(), new_value);
                    entry.dirty = true;
                    let snapshot = entry.clone();
                    if let Some(ref mut detail) = self.detail_state {
                        detail.refresh(&snapshot);
                    }
                }
            }
            return;
        }

        // Edit the path of a specific file attachment ─────────────────────────
        if matches!(self.pending_action, Some(PendingAction::EditFileAttachment { .. })) {
            let (entry_key, file_idx) = match self.pending_action.take() {
                Some(PendingAction::EditFileAttachment { entry_key, index }) => (entry_key, index),
                _ => return,
            };
            let path_str = self.field_editor_state.as_ref()
                .map(|e| e.value.trim().to_string()).unwrap_or_default();
            self.field_editor_state = None;
            self.mode = InputMode::Detail;
            if !path_str.is_empty() {
                let abs_path = PathBuf::from(expand_tilde(&path_str));
                let file_dir = effective_file_dir(
                    &self.bib_path,
                    self.database.jabref_meta.file_directory.as_deref(),
                );
                let stored_path = crate::util::open::make_relative(&file_dir, &abs_path)
                    .to_string_lossy()
                    .into_owned();
                let current = self.database.entries.get(&entry_key)
                    .and_then(|e| e.fields.get("file").cloned())
                    .unwrap_or_default();
                let mut files = parse_file_field(&current);
                if let Some(f) = files.get_mut(file_idx) {
                    let ext = abs_path
                        .extension()
                        .and_then(|e| e.to_str())
                        .map(|e| e.to_uppercase())
                        .unwrap_or_default();
                    f.path = stored_path;
                    if !ext.is_empty() && f.file_type.is_empty() {
                        f.file_type = ext;
                    }
                }
                let new_value = serialize_file_field(&files);
                if new_value != current {
                    self.push_undo(UndoItem::FieldChanged {
                        entry_key: entry_key.clone(),
                        field_name: "file".to_string(),
                        old_value: Some(current),
                    });
                    if let Some(entry) = self.database.entries.get_mut(&entry_key) {
                        entry.fields.insert("file".to_string(), new_value);
                        entry.dirty = true;
                        let snapshot = entry.clone();
                        if let Some(ref mut detail) = self.detail_state {
                            detail.refresh(&snapshot);
                        }
                    }
                }
            }
            return;
        }

        // Group name input — handled separately from field editing
        if matches!(self.pending_action, Some(PendingAction::AddGroup { .. })) {
            let name = self
                .field_editor_state
                .as_ref()
                .map(|e| e.value.trim().to_string())
                .unwrap_or_default();
            let parent_path = match self.pending_action.take() {
                Some(PendingAction::AddGroup { parent_path }) => parent_path,
                _ => vec![],
            };
            self.field_editor_state = None;
            self.mode = InputMode::Normal;
            if !name.is_empty() {
                self.finish_add_group(name, parent_path);
            }
            return;
        }

        // Two-phase for new fields: first confirm name, then enter value
        if let Some(ref mut editor) = self.field_editor_state {
            if editor.advance_phase() {
                // Now that the field name is confirmed, enable month mode if applicable.
                if editor.field_name.eq_ignore_ascii_case("month") {
                    editor.is_month = true;
                }
                // Just switched from name to value editing — seed value completions.
                self.update_field_completions();
                return;
            }
        }

        if let Some(editor) = self.field_editor_state.take() {
            // Skip if field name is empty (aborted new-field)
            if editor.field_name.is_empty() {
                self.mode = InputMode::Detail;
                return;
            }
            if let Some(ref key) = self.detail_entry_key.clone() {
                let existing = self.database.entries.get(key)
                    .and_then(|e| e.fields.get(&editor.field_name).cloned());
                let existing_str = existing.clone().unwrap_or_default();
                // Normalize month values to standard 3-letter abbreviations.
                let save_value = if editor.is_month {
                    normalize_month(&editor.value)
                } else {
                    editor.value.clone()
                };
                if save_value != existing_str {
                    self.push_undo(UndoItem::FieldChanged {
                        entry_key: key.clone(),
                        field_name: editor.field_name.clone(),
                        old_value: existing,
                    });
                    if let Some(entry) = self.database.entries.get_mut(key) {
                        entry.fields.insert(editor.field_name.clone(), save_value);
                        entry.dirty = true;
                        let snapshot = entry.clone();
                        if let Some(ref mut detail) = self.detail_state {
                            detail.refresh(&snapshot);
                        }
                    }
                    self.regen_citekey();
                    let current_key = self.detail_entry_key.clone().unwrap_or_else(|| key.clone());
                    self.recheck_dirty(&current_key);
                }
            }
        }
        self.mode = InputMode::Detail;
    }

    fn delete_field(&mut self) {
        // When a FileEntry row is selected, remove just that file from the field.
        if let Some(file_idx) = self.detail_state.as_ref().and_then(|d| d.selected_file_index()) {
            self.delete_file_attachment(file_idx);
            return;
        }

        let field_name_opt = self
            .detail_state
            .as_ref()
            .and_then(|d| d.selected_field())
            .map(|(name, _)| name.to_string());

        if let Some(field_name) = field_name_opt {
            if let Some(ref key) = self.detail_entry_key.clone() {
                let old_value = self.database.entries.get(key)
                    .and_then(|e| e.fields.get(&field_name).cloned());
                if let Some(old_value) = old_value {
                    self.push_undo(UndoItem::FieldChanged {
                        entry_key: key.clone(),
                        field_name: field_name.clone(),
                        old_value: Some(old_value),
                    });
                    if let Some(entry) = self.database.entries.get_mut(key) {
                        entry.fields.shift_remove(&field_name);
                        entry.dirty = true;
                        let entry_clone = entry.clone();
                        if let Some(ref mut detail) = self.detail_state {
                            detail.refresh(&entry_clone);
                        }
                    }
                }
            }
        }
    }

    fn delete_file_attachment(&mut self, index: usize) {
        let key = match self.detail_entry_key.clone() { Some(k) => k, None => return };
        let file_value = match self.database.entries.get(&key)
            .and_then(|e| e.fields.get("file")).cloned()
        {
            Some(v) if !v.is_empty() => v,
            _ => return,
        };
        let mut files = parse_file_field(&file_value);
        if index >= files.len() {
            return;
        }
        files.remove(index);
        let new_value = serialize_file_field(&files);
        self.push_undo(UndoItem::FieldChanged {
            entry_key: key.clone(),
            field_name: "file".to_string(),
            old_value: Some(file_value),
        });
        if let Some(entry) = self.database.entries.get_mut(&key) {
            if new_value.is_empty() {
                entry.fields.shift_remove("file");
            } else {
                entry.fields.insert("file".to_string(), new_value);
            }
            entry.dirty = true;
            let snapshot = entry.clone();
            if let Some(ref mut detail) = self.detail_state {
                detail.refresh(&snapshot);
            }
        }
    }

    /// After a field edit, re-evaluate whether the entry is truly dirty by
    /// comparing its current fields and citation key against the original raw
    /// representation in the file.  Clears `dirty` when the entry has been
    /// fully reverted to its on-disk state.
    fn recheck_dirty(&mut self, entry_key: &str) {
        use crate::bib::model::RawItem;

        let Some(entry) = self.database.entries.get_mut(entry_key) else { return };

        // A changed citation key is always dirty.
        let original_raw = match self.database.raw_file.items.get(entry.raw_index) {
            Some(RawItem::Entry(re)) => re,
            _ => return,
        };
        if entry.citation_key != original_raw.citation_key {
            return;
        }

        // Build a map of original field values (inner content, no delimiters).
        let original: std::collections::HashMap<String, String> = original_raw
            .fields
            .iter()
            .map(|f| (f.name.to_lowercase(), f.value.to_string_value()))
            .collect();

        // Current non-empty fields must match the original exactly.
        let all_match = entry
            .fields
            .iter()
            .filter(|(_, v)| !v.is_empty())
            .all(|(k, v)| original.get(&k.to_lowercase()).map(|ov| ov == v).unwrap_or(false))
            && original
                .iter()
                .filter(|(_, v)| !v.is_empty())
                .all(|(k, v)| entry.fields.get(k.as_str()).map(|ev| ev == v).unwrap_or(false));

        if all_match {
            entry.dirty = false;
        }
    }

    fn regen_citekey(&mut self) {
        if let Some(ref key) = self.detail_entry_key.clone() {
            if let Some(entry) = self.database.entries.get(key) {
                let display_name = entry.entry_type.display_name();
                let type_name = display_name.to_lowercase();
                let template = self
                    .config
                    .citekey
                    .templates
                    .get(&type_name)
                    .cloned()
                    .unwrap_or_else(|| format!("{}_[year]_[auth]", display_name));

                let new_key = generate_citekey(&template, &entry.fields);

                if new_key != *key {
                    // Re-key the entry
                    if let Some(mut entry) = self.database.entries.shift_remove(key) {
                        self.push_undo(UndoItem::CitekeyChanged {
                            old_key: key.clone(),
                            new_key: new_key.clone(),
                            entry_snapshot: entry.clone(),
                        });
                        entry.citation_key = new_key.clone();
                        entry.dirty = true;
                        self.database.entries.insert(new_key.clone(), entry);
                        self.detail_entry_key = Some(new_key);
                        self.sorted_keys = sort_entries(&self.database.entries, &self.config);

                        if let Some(ref mut detail) = self.detail_state {
                            if let Some(entry) = self.database.entries.get(self.detail_entry_key.as_ref().unwrap()) {
                                detail.refresh(entry);
                            }
                        }
                        self.status_message = Some("Citation key regenerated".to_string());
                    }
                }
            }
        }
    }

    /// Regenerate citation keys for all entries using their configured templates.
    /// Returns the number of keys changed.  When `push_undo` is true each rename
    /// is pushed to the undo stack.
    fn regen_all_citekeys_impl(&mut self, push_undo: bool) -> usize {
        let keys: Vec<String> = self.database.entries.keys().cloned().collect();
        let mut renamed = 0usize;

        for key in keys {
            let (base_new_key, skip) = {
                let Some(entry) = self.database.entries.get(&key) else { continue };
                let display_name = entry.entry_type.display_name();
                let type_name = display_name.to_lowercase();
                let template = self.config.citekey.templates.get(&type_name)
                    .cloned()
                    .unwrap_or_else(|| format!("{}_[year]_[auth]", display_name));
                let base = generate_citekey(&template, &entry.fields);
                let skip = base == key;
                (base, skip)
            };

            if skip {
                continue;
            }

            // Resolve collisions with a numeric suffix.
            let new_key = if !self.database.entries.contains_key(&base_new_key) {
                base_new_key.clone()
            } else {
                let mut n = 2usize;
                loop {
                    let candidate = format!("{}_{}", base_new_key, n);
                    if !self.database.entries.contains_key(&candidate) {
                        break candidate;
                    }
                    n += 1;
                }
            };

            if new_key == key {
                continue;
            }

            if let Some(mut entry) = self.database.entries.shift_remove(&key) {
                if push_undo {
                    self.push_undo(UndoItem::CitekeyChanged {
                        old_key: key.clone(),
                        new_key: new_key.clone(),
                        entry_snapshot: entry.clone(),
                    });
                }
                entry.citation_key = new_key.clone();
                entry.dirty = true;
                self.database.entries.insert(new_key.clone(), entry);
                renamed += 1;

                if self.detail_entry_key.as_deref() == Some(key.as_str()) {
                    self.detail_entry_key = Some(new_key);
                }
            }
        }

        renamed
    }

    fn regen_all_citekeys(&mut self) {
        let renamed = self.regen_all_citekeys_impl(true);

        if renamed > 0 {
            self.sorted_keys = sort_entries(&self.database.entries, &self.config);
            let new_key = self.detail_entry_key.clone();
            if let (Some(ref key), Some(ref mut detail)) = (new_key, self.detail_state.as_mut()) {
                if let Some(entry) = self.database.entries.get(key) {
                    detail.refresh(entry);
                }
            }
            self.status_message = Some(format!(
                "{} citation key{} regenerated",
                renamed,
                if renamed == 1 { "" } else { "s" }
            ));
        } else {
            self.status_message = Some("All citation keys are up to date".to_string());
        }
    }

    // ── Entry CRUD ──

    fn start_change_entry_type(&mut self) {
        let Some(entry_key) = self.detail_entry_key.clone() else { return };
        let types = vec![
            "Article".to_string(),
            "Book".to_string(),
            "InProceedings".to_string(),
            "TechReport".to_string(),
            "PhdThesis".to_string(),
            "MastersThesis".to_string(),
            "Misc".to_string(),
            "InBook".to_string(),
            "InCollection".to_string(),
            "Proceedings".to_string(),
            "Unpublished".to_string(),
            "Booklet".to_string(),
            "Manual".to_string(),
        ];
        // Pre-select the entry's current type in the picker
        let current = self
            .database
            .entries
            .get(&entry_key)
            .map(|e| e.entry_type.display_name().to_string());
        let selected = current
            .as_deref()
            .and_then(|name| types.iter().position(|t| t == name))
            .unwrap_or(0);
        let mut dialog = DialogState::type_picker_titled("Change Entry Type", types);
        dialog.select(selected);
        self.dialog_state = Some(dialog);
        self.pending_action = Some(PendingAction::ChangeEntryType { entry_key });
        self.mode = InputMode::Dialog;
    }

    fn start_add_entry(&mut self) {
        let types = vec![
            "Article".to_string(),
            "Book".to_string(),
            "InProceedings".to_string(),
            "TechReport".to_string(),
            "PhdThesis".to_string(),
            "MastersThesis".to_string(),
            "Misc".to_string(),
            "InBook".to_string(),
            "InCollection".to_string(),
            "Proceedings".to_string(),
            "Unpublished".to_string(),
            "Booklet".to_string(),
            "Manual".to_string(),
        ];
        self.dialog_state = Some(DialogState::type_picker(types));
        self.pending_action = Some(PendingAction::AddEntryType);
        self.mode = InputMode::Dialog;
    }

    fn add_entry_of_type(&mut self, type_name: &str) {
        let entry_type = EntryType::from_str(type_name);
        let (required, _) = crate::bib::entry_types::fields_for_type(&entry_type);

        let mut fields = IndexMap::new();
        for field in required {
            fields.insert(field.to_string(), String::new());
        }

        let key = format!("New_{}", type_name);
        let entry = Entry {
            entry_type,
            citation_key: key.clone(),
            fields,
            group_memberships: Vec::new(),
            raw_index: usize::MAX,
            dirty: true,
        };

        self.database.entries.insert(key.clone(), entry);
        self.push_undo(UndoItem::EntryAdded { entry_key: key.clone() });
        self.sorted_keys = sort_entries(&self.database.entries, &self.config);

        // Open detail view for the new entry
        self.detail_entry_key = Some(key.clone());
        if let Some(entry) = self.database.entries.get(&key) {
            self.detail_state = Some(EntryDetailState::new(entry, self.config.field_groups.clone()));
        }
        self.mode = InputMode::Detail;
        self.status_message = Some(format!("Added new {} entry", type_name));
    }

    fn apply_entry_type_change(&mut self, entry_key: &str, type_name: &str) {
        let new_type = EntryType::from_str(type_name);
        if let Some(entry) = self.database.entries.get_mut(entry_key) {
            if entry.entry_type == new_type {
                self.mode = InputMode::Detail;
                return;
            }
            let old_type = entry.entry_type.clone();
            self.push_undo(UndoItem::EntryTypeChanged {
                entry_key: entry_key.to_string(),
                old_type,
            });
            let entry = self.database.entries.get_mut(entry_key).unwrap();
            entry.entry_type = new_type;
            entry.dirty = true;
        }
        // Rebuild detail state to reflect new required/optional categorisation
        if self.detail_entry_key.as_deref() == Some(entry_key) {
            if let Some(entry) = self.database.entries.get(entry_key) {
                let snapshot = entry.clone();
                if let Some(ref mut detail) = self.detail_state {
                    detail.refresh(&snapshot);
                }
            }
        }
        self.mode = InputMode::Detail;
        self.status_message = Some(format!("Entry type changed to {}", type_name));
    }

    fn start_delete_entry(&mut self) {
        let Some(key) = self.selected_entry_key() else { return };

        let local_files = self.resolve_entry_local_files(&key);

        match local_files.len() {
            0 => {
                // No local files — simple yes/no confirm (existing behaviour).
                self.dialog_state = Some(DialogState::confirm(
                    "Delete Entry",
                    &format!("Delete '{}'?", key),
                ));
                self.pending_action = Some(PendingAction::DeleteEntry(key));
            }
            1 => {
                // One local file — TypePicker with three clear choices.
                let file = local_files.into_iter().next().unwrap();
                let fname = file
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("file")
                    .to_string();
                self.dialog_state = Some(DialogState::type_picker_titled(
                    &format!("Delete '{}'", key),
                    vec![
                        format!("Delete entry + {}", fname),
                        "Delete entry only".to_string(),
                        "Cancel".to_string(),
                    ],
                ));
                self.pending_action =
                    Some(PendingAction::DeleteEntryWithFile { entry_key: key, file });
            }
            _ => {
                // Multiple local files — checkbox multi-select (default: all checked).
                let labels: Vec<(String, bool)> = local_files
                    .iter()
                    .map(|p| {
                        let name = p
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("?")
                            .to_string();
                        (name, true)
                    })
                    .collect();
                self.dialog_state = Some(DialogState::file_delete_select(
                    &format!("Delete '{}'", key),
                    labels,
                ));
                self.pending_action = Some(PendingAction::DeleteEntryWithFileSelect {
                    entry_key: key,
                    files: local_files,
                });
            }
        }
        self.mode = InputMode::Dialog;
    }

    /// Collect paths of locally-existing files referenced in the entry's `file` field.
    fn resolve_entry_local_files(&self, key: &str) -> Vec<std::path::PathBuf> {
        use crate::util::open::{effective_file_dir, parse_file_field, resolve_file_path};
        let entry = match self.database.entries.get(key) {
            Some(e) => e,
            None => return vec![],
        };
        let file_value = match entry.fields.get("file") {
            Some(v) if !v.trim().is_empty() => v.clone(),
            _ => return vec![],
        };
        let bib_dir = effective_file_dir(
            &self.bib_path,
            self.database.jabref_meta.file_directory.as_deref(),
        );
        parse_file_field(&file_value)
            .into_iter()
            .map(|f| resolve_file_path(&f.path, &bib_dir))
            .filter(|p| p.exists())
            .collect()
    }

    fn delete_entry(&mut self, key: &str) {
        if let Some(entry) = self.database.entries.get(key).cloned() {
            self.push_undo(UndoItem::EntryDeleted { entry: entry.clone() });
            if entry.raw_index != usize::MAX {
                self.deleted_raw_indices.push(entry.raw_index);
            }
        }
        self.database.entries.shift_remove(key);
        self.sorted_keys = sort_entries(&self.database.entries, &self.config);

        let count = self.visible_entry_count();
        if self.entry_list_state.selected() >= count && count > 0 {
            self.entry_list_state.select(count - 1);
        }
        self.status_message = Some(format!("Deleted '{}'", key));
    }

    fn duplicate_entry(&mut self) {
        if let Some(key) = self.selected_entry_key() {
            if let Some(entry) = self.database.entries.get(&key).cloned() {
                let new_key = format!("{}_copy", key);
                let mut new_entry = entry;
                new_entry.citation_key = new_key.clone();
                new_entry.dirty = true;
                self.database.entries.insert(new_key.clone(), new_entry);
                self.push_undo(UndoItem::EntryAdded { entry_key: new_key });
                self.sorted_keys = sort_entries(&self.database.entries, &self.config);
                self.status_message = Some("Entry duplicated".to_string());
            }
        }
    }

    fn titlecase_selected_field(&mut self) {
        let field_name = self
            .detail_state
            .as_ref()
            .and_then(|d| d.selected_field())
            .map(|(name, _)| name.to_string());

        if let Some(field_name) = field_name {
            if let Some(key) = self.detail_entry_key.clone() {
                let value = self.database.entries.get(&key)
                    .and_then(|e| e.fields.get(&field_name).cloned());
                if let Some(value) = value {
                    let converted = crate::util::titlecase::apply_titlecase(
                        &value,
                        &self.config.titlecase.ignore_words,
                        &self.config.titlecase.stop_words,
                    );
                    if converted != value {
                        self.push_undo(UndoItem::FieldChanged {
                            entry_key: key.clone(),
                            field_name: field_name.clone(),
                            old_value: Some(value),
                        });
                        if let Some(entry) = self.database.entries.get_mut(&key) {
                            entry.fields.insert(field_name.clone(), converted);
                            entry.dirty = true;
                            let entry_clone = entry.clone();
                            if let Some(ref mut detail) = self.detail_state {
                                detail.refresh(&entry_clone);
                            }
                        }
                        self.status_message = Some(format!("Title-cased '{}'", field_name));
                    } else {
                        self.status_message =
                            Some(format!("'{}' already in title case", field_name));
                    }
                }
            }
        }
    }

    fn normalize_names_field(&mut self) {
        const NAME_FIELDS: &[&str] = &[
            "author", "editor", "editora", "editorb", "editorc",
            "bookauthor", "afterword", "translator",
        ];
        let field_name = match self
            .detail_state
            .as_ref()
            .and_then(|d| d.selected_field())
            .map(|(name, _)| name.to_string())
        {
            Some(n) => n,
            None => return,
        };

        if !NAME_FIELDS.iter().any(|&f| f.eq_ignore_ascii_case(&field_name)) {
            self.status_message = Some(format!(
                "'{}' is not a person-name field",
                field_name
            ));
            return;
        }

        if let Some(key) = self.detail_entry_key.clone() {
            let value = self.database.entries.get(&key)
                .and_then(|e| e.fields.get(&field_name).cloned());
            if let Some(value) = value {
                let normalized = crate::util::author::normalize_author_names(&value);
                if normalized != value {
                    self.push_undo(UndoItem::FieldChanged {
                        entry_key: key.clone(),
                        field_name: field_name.clone(),
                        old_value: Some(value),
                    });
                    if let Some(entry) = self.database.entries.get_mut(&key) {
                        entry.fields.insert(field_name.clone(), normalized);
                        entry.dirty = true;
                        let entry_clone = entry.clone();
                        if let Some(ref mut detail) = self.detail_state {
                            detail.refresh(&entry_clone);
                        }
                    }
                    self.status_message =
                        Some(format!("'{}' normalized to 'Last, First' form", field_name));
                } else {
                    self.status_message =
                        Some(format!("'{}' already in 'Last, First' form", field_name));
                }
            }
        }
    }

    fn action_entry_key(&self) -> Option<String> {
        self.detail_entry_key.clone().or_else(|| self.selected_entry_key())
    }

    fn open_file(&mut self) {
        use crate::util::open::{parse_file_field, resolve_file_path, effective_file_dir, open_path};

        let key = match self.action_entry_key() {
            Some(k) => k,
            None => return,
        };
        let file_value = match self.database.entries.get(&key)
            .and_then(|e| e.fields.get("file")).cloned()
        {
            Some(v) if !v.trim().is_empty() => v,
            _ => {
                self.status_message = Some("No file attached to this entry".to_string());
                return;
            }
        };

        let files = parse_file_field(&file_value);
        if files.is_empty() {
            self.status_message = Some("No file attached to this entry".to_string());
            return;
        }

        // If in detail mode with a specific FileEntry row selected, open that file directly.
        let selected_idx = self.detail_state.as_ref().and_then(|d| d.selected_file_index());

        let bib_dir = effective_file_dir(
            &self.bib_path,
            self.database.jabref_meta.file_directory.as_deref(),
        );

        if let Some(idx) = selected_idx {
            if let Some(f) = files.get(idx) {
                let path = resolve_file_path(&f.path, &bib_dir);
                match open_path(&path) {
                    Ok(()) => self.status_message = Some(format!("Opening {}", path.display())),
                    Err(e) => self.status_message = Some(format!("Error: {}", e)),
                }
                return;
            }
        }

        if files.len() == 1 {
            let path = resolve_file_path(&files[0].path, &bib_dir);
            match open_path(&path) {
                Ok(()) => self.status_message = Some(format!("Opening {}", path.display())),
                Err(e) => self.status_message = Some(format!("Error: {}", e)),
            }
        } else {
            let options: Vec<String> = files.iter().map(|f| f.label()).collect();
            self.dialog_state = Some(DialogState::type_picker_titled(
                "Open File",
                options,
            ));
            self.pending_action = Some(PendingAction::OpenFile(files));
            self.mode = InputMode::Dialog;
        }
    }

    fn open_web(&mut self) {
        use crate::util::open::{doi_to_url, open_url};

        let key = match self.action_entry_key() {
            Some(k) => k,
            None => return,
        };
        let entry = match self.database.entries.get(&key) {
            Some(e) => e,
            None => return,
        };

        let doi_url = entry.fields.get("doi")
            .filter(|v| !v.trim().is_empty())
            .map(|v| doi_to_url(v.trim()));
        let raw_url = entry.fields.get("url")
            .filter(|v| !v.trim().is_empty())
            .map(|v| v.trim().to_string());

        let isbn_url = entry.fields.get("isbn")
            .filter(|v| !v.trim().is_empty())
            .map(|v| {
                // Strip spaces and hyphens to get a clean ISBN for the URL
                let clean: String = v.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
                format!("https://openlibrary.org/search?isbn={}", clean)
            });

        let mut urls: Vec<(String, String)> = Vec::new(); // (label, url)
        if let Some(u) = doi_url {
            urls.push((format!("DOI: {}", u), u));
        }
        if let Some(u) = raw_url {
            urls.push((format!("URL: {}", u), u.clone()));
        }
        if let Some(u) = isbn_url {
            urls.push(("ISBN (openlibrary.org)".to_string(), u));
        }

        match urls.len() {
            0 => {
                // No DOI/URL/ISBN — fetch one from metadata instead
                self.start_fetch_doi();
                return;
            }
            1 => {
                let url = urls.remove(0).1;
                match open_url(&url) {
                    Ok(()) => self.status_message = Some(format!("Opening {}", url)),
                    Err(e) => self.status_message = Some(format!("Error: {}", e)),
                }
            }
            _ => {
                let labels: Vec<String> = urls.iter().map(|(l, _)| l.clone()).collect();
                let raw_urls: Vec<String> = urls.into_iter().map(|(_, u)| u).collect();
                self.dialog_state = Some(DialogState::type_picker_titled(
                    "Open Web Link",
                    labels,
                ));
                self.pending_action = Some(PendingAction::OpenWeb(raw_urls));
                self.mode = InputMode::Dialog;
            }
        }
    }

    fn yank_citekey(&mut self) {
        let key = match self.selected_entry_key() {
            Some(k) => k,
            None => return,
        };
        let yank_format = self.config.general.yank_format.clone();
        match yank_format.as_str() {
            "prompt" => {
                let style = self.config.citation.style.clone();
                self.dialog_state = Some(DialogState::type_picker_titled(
                    "Yank to clipboard",
                    vec![
                        "Citation key".to_string(),
                        "BibTeX entry".to_string(),
                        format!("Formatted citation ({})", style),
                    ],
                ));
                self.pending_action = Some(PendingAction::YankPrompt { entry_key: key });
                self.mode = InputMode::Dialog;
            }
            format => {
                self.do_yank(&key, &format);
            }
        }
    }

    /// Copy `entry_key` to clipboard in the given format string.
    fn do_yank(&mut self, entry_key: &str, format: &str) {
        let entry = match self.database.entries.get(entry_key) {
            Some(e) => e,
            None => return,
        };
        let (text, label) = match format {
            "citation_key" => (
                entry.citation_key.clone(),
                format!("key '{}'", entry.citation_key),
            ),
            "bibtex" => (
                serialize_entry(entry, self.config.save.align_fields, self.config.save.field_order == "alphabetical"),
                format!("BibTeX entry for '{}'", entry.citation_key),
            ),
            _ => (
                format_citation(entry, &self.config.citation.style),
                format!("citation for '{}'", entry.citation_key),
            ),
        };
        match crate::util::clipboard::copy_to_clipboard(&text) {
            Ok(()) => self.status_message = Some(format!("Copied {} to clipboard", label)),
            Err(e) => self.status_message = Some(format!("Clipboard error: {}", e)),
        }
    }

    // ── Groups ──

    fn select_group(&mut self) {
        if let Some(item) = self.group_tree_state.selected_item() {
            let name = item.name.clone();

            if self.group_tree_state.active_group.as_ref() == Some(&name) {
                // Deselect
                self.group_tree_state.active_group = None;
                self.filtered_indices = None;
            } else {
                // Find the group node and filter
                if let Some(node) = find_group_node(&self.database.groups.root, &name) {
                    let entries: Vec<&Entry> = self
                        .sorted_keys
                        .iter()
                        .filter_map(|k| self.database.entries.get(k))
                        .collect();
                    let indices = filter_by_group(&entries, node);
                    self.search_bar_state.result_count = indices.len();
                    self.filtered_indices = Some(indices);
                    self.group_tree_state.active_group = Some(name);
                    self.entry_list_state.select(0);
                }
            }
            self.focus = Focus::List;
        }
    }

    // ── Citation preview ──

    fn show_citation_preview(&mut self) {
        let key = match self.current_entry_key() {
            Some(k) => k,
            None => return,
        };
        let citation = match self.database.entries.get(&key) {
            Some(entry) => format_citation(entry, &self.config.citation.style),
            None => return,
        };
        self.citation_preview_state = Some(CitationPreviewState {
            citation,
            entry_key: key,
            style_name: self.config.citation.style.clone(),
        });
        self.mode = InputMode::CitationPreview;
    }

    /// Return the citation key of the currently selected entry list row.
    fn current_entry_key(&self) -> Option<String> {
        let idx = self.entry_list_state.selected();
        let visible: Vec<&String> = if let Some(ref indices) = self.filtered_indices {
            indices.iter().filter_map(|&i| self.sorted_keys.get(i)).collect()
        } else {
            self.sorted_keys.iter().collect()
        };
        visible.get(idx).map(|k| (*k).clone())
    }

    // ── Commands ──

    fn execute_command(&mut self) {
        let cmd = self.command_palette_state.input.trim().to_string();
        self.mode = InputMode::Normal;

        match cmd.as_str() {
            "w" | "write" | "save" => self.request_save(false),
            "q" | "quit" => {
                if self.dirty {
                    self.status_message = Some("Unsaved changes. Use :q! to force quit".to_string());
                } else {
                    self.should_quit = true;
                }
            }
            "q!" => {
                self.should_quit = true;
            }
            "wq" => {
                self.request_save(true);
            }
            _ if cmd.starts_with("sort ") || cmd == "sort" => {
                let field = cmd.trim_start_matches("sort").trim().to_string();
                let msg = if field == "none" {
                    self.config.display.default_sort.field = "none".to_string();
                    "Sort cleared (file order)".to_string()
                } else if field.is_empty() {
                    // Toggle ascending/descending on current sort field
                    self.config.display.default_sort.ascending =
                        !self.config.display.default_sort.ascending;
                    let dir = if self.config.display.default_sort.ascending { "↑" } else { "↓" };
                    format!("Sorted by {} {}", self.config.display.default_sort.field, dir)
                } else if self.config.display.default_sort.field == field {
                    // Same field: toggle direction
                    self.config.display.default_sort.ascending =
                        !self.config.display.default_sort.ascending;
                    let dir = if self.config.display.default_sort.ascending { "↑" } else { "↓" };
                    format!("Sorted by {} {}", self.config.display.default_sort.field, dir)
                } else {
                    self.config.display.default_sort.field = field.clone();
                    self.config.display.default_sort.ascending = true;
                    let dir = if self.config.display.default_sort.ascending { "↑" } else { "↓" };
                    format!("Sorted by {} {}", self.config.display.default_sort.field, dir)
                };
                self.sorted_keys = sort_entries(&self.database.entries, &self.config);
                // Re-run search so filtered_indices stays consistent with new sorted_keys
                self.update_search();
                self.entry_list_state.select(0);
                self.status_message = Some(msg);
            }
            _ if cmd.starts_with("import ") => {
                let doi_or_url = cmd["import ".len()..].trim().to_string();
                if !doi_or_url.is_empty() {
                    self.spawn_import(doi_or_url);
                }
            }
            _ if cmd.starts_with("export-json") => {
                let path = cmd["export-json".len()..].trim().to_string();
                if path.is_empty() {
                    self.handle_action(Action::ExportJson);
                } else {
                    self.do_export_json(&path);
                }
            }
            _ if cmd.starts_with("export-ris") => {
                let path = cmd["export-ris".len()..].trim().to_string();
                if path.is_empty() {
                    self.handle_action(Action::ExportRis);
                } else {
                    self.do_export_ris(&path);
                }
            }
            _ if cmd.starts_with("group ") => {
                let group_name = cmd["group ".len()..].trim().to_string();
                if !group_name.is_empty() {
                    self.apply_group_filter(&group_name);
                }
            }
            _ if cmd.starts_with("search ") => {
                let query = cmd["search ".len()..].trim().to_string();
                if !query.is_empty() {
                    self.search_bar_state.query = query.clone();
                    self.update_search();
                    self.status_message = Some(format!("Search: {}", query));
                }
            }
            _ => {
                self.status_message = Some(format!("Unknown command: {}", cmd));
            }
        }
    }

    // ── Dialog handling ──

    fn handle_dialog_confirm(&mut self) {
        let action = self.pending_action.take();
        let dialog = self.dialog_state.take();
        self.mode = InputMode::Normal;

        match action {
            Some(PendingAction::DeleteEntry(key)) => {
                self.delete_entry(&key);
            }
            Some(PendingAction::DeleteEntryWithFile { entry_key, file }) => {
                let selected = dialog.as_ref().map(|d| d.selected()).unwrap_or(2);
                match selected {
                    0 => {
                        // Delete entry and the one attached file.
                        let _ = std::fs::remove_file(&file);
                        self.delete_entry(&entry_key);
                    }
                    1 => {
                        // Delete entry, keep file.
                        self.delete_entry(&entry_key);
                    }
                    _ => {
                        // Cancel — do nothing; mode already reset to Normal above.
                    }
                }
            }
            Some(PendingAction::DeleteEntryWithFileSelect { entry_key, files }) => {
                // Delete the entry unconditionally; delete only the checked files.
                if let Some(ref d) = dialog {
                    if let DialogKind::FileDeleteSelect { files: ref labels, .. } = d.kind {
                        for (path, (_, delete)) in files.iter().zip(labels.iter()) {
                            if *delete {
                                let _ = std::fs::remove_file(path);
                            }
                        }
                    }
                }
                self.delete_entry(&entry_key);
            }
            Some(PendingAction::AddEntryType) => {
                if let Some(dialog) = dialog {
                    if let DialogKind::TypePicker { options, .. } = &dialog.kind {
                        let selected = dialog.selected();
                        if let Some(type_name) = options.get(selected) {
                            self.add_entry_of_type(&type_name.clone());
                        }
                    }
                }
            }
            Some(PendingAction::ChangeEntryType { entry_key }) => {
                if let Some(dialog) = dialog {
                    if let DialogKind::TypePicker { options, .. } = &dialog.kind {
                        let selected = dialog.selected();
                        if let Some(type_name) = options.get(selected).cloned() {
                            self.apply_entry_type_change(&entry_key, &type_name);
                        }
                    }
                }
            }
            Some(PendingAction::OpenFile(files)) => {
                if let Some(dialog) = dialog {
                    let selected = dialog.selected();
                    if let Some(file) = files.get(selected) {
                        let bib_dir = crate::util::open::effective_file_dir(
                            &self.bib_path,
                            self.database.jabref_meta.file_directory.as_deref(),
                        );
                        let path = crate::util::open::resolve_file_path(&file.path, &bib_dir);
                        match crate::util::open::open_path(&path) {
                            Ok(()) => self.status_message =
                                Some(format!("Opening {}", path.display())),
                            Err(e) => self.status_message = Some(format!("Error: {}", e)),
                        }
                    }
                }
            }
            Some(PendingAction::OpenWeb(urls)) => {
                if let Some(dialog) = dialog {
                    let selected = dialog.selected();
                    if let Some(url) = urls.get(selected) {
                        let url = url.clone();
                        match crate::util::open::open_url(&url) {
                            Ok(()) => self.status_message = Some(format!("Opening {}", url)),
                            Err(e) => self.status_message = Some(format!("Error: {}", e)),
                        }
                    }
                }
            }
            Some(PendingAction::DeleteGroup { path }) => {
                self.finish_delete_group(path);
            }
            Some(PendingAction::AssignGroups { entry_key }) => {
                if let Some(dialog) = dialog {
                    if let DialogKind::GroupAssign { groups } = &dialog.kind {
                        let selected: Vec<String> = groups
                            .iter()
                            .filter(|(_, checked)| *checked)
                            .map(|(name, _)| name.clone())
                            .collect();
                        self.finish_assign_groups(&entry_key.clone(), selected);
                    }
                }
                self.mode = InputMode::Detail;
            }
            Some(PendingAction::YankPrompt { entry_key }) => {
                let format = match dialog.as_ref().map(|d| d.selected()) {
                    Some(0) => "citation_key",
                    Some(1) => "bibtex",
                    _ => "formatted",
                };
                self.do_yank(&entry_key.clone(), format);
            }
            Some(PendingAction::Save) => {
                self.save();
            }
            Some(PendingAction::SaveAndQuit) => {
                self.save();
                self.should_quit = true;
            }
            Some(PendingAction::DismissMessage) => {
                // Message popup dismissed — nothing to do; mode already reset above.
            }
            Some(PendingAction::AddGroup { .. })
            | Some(PendingAction::EditSetting { .. })
            | Some(PendingAction::ExportSettings)
            | Some(PendingAction::ImportSettings)
            | Some(PendingAction::AddFieldGroup)
            | Some(PendingAction::EditFieldGroupFields { .. })
            | Some(PendingAction::RenameFieldGroup { .. })
            | Some(PendingAction::AddColumn)
            | Some(PendingAction::EditColumnWidth { .. })
            | Some(PendingAction::RenameColumn { .. })
            | Some(PendingAction::NewFile)
            | Some(PendingAction::ImportUrl)
            | Some(PendingAction::AddFileAttachment { .. })
            | Some(PendingAction::EditFileAttachment { .. })
            | Some(PendingAction::ExportJson)
            | Some(PendingAction::ExportRis) => {
                // These are confirmed through confirm_edit(), not this path
            }
            None => {
                // Quit confirmation
                self.should_quit = true;
            }
        }
    }

    // ── Save ──

    // ── Tab completion for path editors ──

    /// Recompute tab-completion candidates for the field editor based on the
    /// current field name and value prefix.  Called on every char/backspace.
    fn update_field_completions(&mut self) {
        let editor = match &self.field_editor_state {
            Some(e) if !e.is_path => e,
            _ => return,
        };

        // Month field: completions are always the 12 standard abbreviations,
        // filtered by whatever the user has typed so far.
        if editor.is_month {
            const MONTHS: [&str; 12] = [
                "jan", "feb", "mar", "apr", "may", "jun",
                "jul", "aug", "sep", "oct", "nov", "dec",
            ];
            let prefix = editor.value.to_lowercase();
            let completions: Vec<String> = MONTHS
                .iter()
                .filter(|&&m| m.starts_with(prefix.as_str()))
                .map(|&m| m.to_string())
                .collect();
            let e = self.field_editor_state.as_mut().unwrap();
            e.completions = completions;
            e.completion_idx = 0;
            return;
        }

        let current_key = self.detail_entry_key.clone();

        if editor.editing_name {
            let prefix = editor.field_name.to_lowercase();
            let candidates = field_name_candidates(&self.database);
            let completions: Vec<String> = candidates
                .into_iter()
                .filter(|c| c.to_lowercase().starts_with(&prefix) && !prefix.is_empty())
                .collect();
            let e = self.field_editor_state.as_mut().unwrap();
            e.completions = completions;
            e.completion_idx = 0;
        } else {
            let field = editor.field_name.clone();
            let value_lower = editor.value.to_lowercase();
            let all = field_value_candidates(&self.database, &field, current_key.as_deref());
            let completions: Vec<String> = all
                .into_iter()
                .filter(|c| c.to_lowercase().starts_with(&value_lower))
                .collect();
            let e = self.field_editor_state.as_mut().unwrap();
            e.completions = completions;
            e.completion_idx = 0;
        }
    }

    /// Tab-complete for the field editor (name phase or value phase).
    /// Delegates to path completion when `editor.is_path` is set.
    fn do_field_tab_complete(&mut self) {
        if self.field_editor_state.as_ref().map(|e| e.is_path).unwrap_or(false) {
            self.do_path_tab_complete();
            return;
        }

        let editor = match &self.field_editor_state {
            Some(e) => e,
            None => return,
        };
        let completions = editor.completions.clone();
        if completions.is_empty() {
            return;
        }
        let idx = editor.completion_idx;
        let editing_name = editor.editing_name;
        let current = if editing_name {
            editor.field_name.clone()
        } else {
            editor.value.clone()
        };

        // If the active text already equals completions[idx], cycle to next.
        if current.to_lowercase() == completions[idx].to_lowercase() {
            let next_idx = (idx + 1) % completions.len();
            let e = self.field_editor_state.as_mut().unwrap();
            e.completion_idx = next_idx;
            if editing_name {
                e.field_name = completions[next_idx].clone();
                e.name_cursor = e.field_name.len();
            } else {
                e.value = completions[next_idx].clone();
                e.cursor = e.value.len();
            }
            return; // Preserve the completion set for continued cycling.
        }

        // First Tab on a partial: fill common prefix, then first match.
        match completions.len() {
            1 => {
                let e = self.field_editor_state.as_mut().unwrap();
                if editing_name {
                    e.field_name = completions[0].clone();
                    e.name_cursor = e.field_name.len();
                } else {
                    e.value = completions[0].clone();
                    e.cursor = e.value.len();
                }
                e.completion_idx = 0;
            }
            _ => {
                let common = longest_common_prefix(&completions);
                let e = self.field_editor_state.as_mut().unwrap();
                let active_lower = if editing_name {
                    e.field_name.to_lowercase()
                } else {
                    e.value.to_lowercase()
                };
                if active_lower != common.to_lowercase() {
                    // Advance to the common prefix.
                    if editing_name {
                        e.field_name = common;
                        e.name_cursor = e.field_name.len();
                    } else {
                        e.value = common;
                        e.cursor = e.value.len();
                    }
                    e.completion_idx = 0;
                } else {
                    // Already at common prefix — fill in first completion.
                    if editing_name {
                        e.field_name = completions[0].clone();
                        e.name_cursor = e.field_name.len();
                    } else {
                        e.value = completions[0].clone();
                        e.cursor = e.value.len();
                    }
                    e.completion_idx = 0;
                }
            }
        }
        // DON'T update completions here — preserve the set for cycling.
    }

    fn update_sort_completions(&mut self) {
        let input = self.command_palette_state.input.clone();
        let partial = match input.strip_prefix("sort ") {
            Some(p) => p.to_string(),
            None => {
                self.command_palette_state.completions.clear();
                self.command_palette_state.completion_idx = 0;
                return;
            }
        };
        let candidates = sort_field_candidates(&self.database);
        self.command_palette_state.completions = candidates
            .into_iter()
            .filter(|c| c.starts_with(partial.as_str()))
            .collect();
        self.command_palette_state.completion_idx = 0;
    }

    fn do_sort_tab_complete(&mut self) {
        let completions = self.command_palette_state.completions.clone();
        if completions.is_empty() {
            return;
        }
        let input = self.command_palette_state.input.clone();
        let partial = match input.strip_prefix("sort ") {
            Some(p) => p.to_string(),
            None => return,
        };
        let idx = self.command_palette_state.completion_idx;

        // Already filled this completion — cycle to the next one.
        if partial == completions[idx].as_str() {
            let next_idx = (idx + 1) % completions.len();
            self.command_palette_state.completion_idx = next_idx;
            let new_input = format!("sort {}", completions[next_idx]);
            self.command_palette_state.cursor = new_input.len();
            self.command_palette_state.input = new_input;
            return; // Keep the same completion set for continued cycling.
        }

        // First Tab on a partial: complete to common prefix, then first match.
        match completions.len() {
            1 => {
                let new_input = format!("sort {}", completions[0]);
                self.command_palette_state.cursor = new_input.len();
                self.command_palette_state.input = new_input;
                self.command_palette_state.completion_idx = 0;
            }
            _ => {
                let common = longest_common_prefix(&completions);
                if partial.as_str() != common.as_str() {
                    // Advance to the longest common prefix without cycling yet.
                    let new_input = format!("sort {}", common);
                    self.command_palette_state.cursor = new_input.len();
                    self.command_palette_state.input = new_input;
                    self.command_palette_state.completion_idx = 0;
                } else {
                    // Already at common prefix — start cycling from the first entry.
                    let new_input = format!("sort {}", completions[0]);
                    self.command_palette_state.cursor = new_input.len();
                    self.command_palette_state.input = new_input;
                    self.command_palette_state.completion_idx = 0;
                }
            }
        }
        // DON'T update completions here — preserve the full set for cycling.
    }

    fn do_path_tab_complete(&mut self) {
        // Only active for path-editing pending actions.
        let is_path_edit = matches!(
            self.pending_action,
            Some(PendingAction::ExportSettings)
                | Some(PendingAction::ImportSettings)
                | Some(PendingAction::NewFile)
                | Some(PendingAction::ImportUrl)
                | Some(PendingAction::AddFileAttachment { .. })
                | Some(PendingAction::EditFileAttachment { .. })
                | Some(PendingAction::ExportJson)
                | Some(PendingAction::ExportRis)
        );
        if !is_path_edit {
            return;
        }
        let editor = match self.field_editor_state.as_mut() {
            Some(e) => e,
            None => return,
        };

        // If we already have completions and the current value matches the
        // last-inserted candidate, cycle to the next one.
        if !self.path_completions.is_empty()
            && self.path_completion_idx < self.path_completions.len()
            && editor.value == self.path_completions[self.path_completion_idx]
        {
            self.path_completion_idx =
                (self.path_completion_idx + 1) % self.path_completions.len();
            let next = self.path_completions[self.path_completion_idx].clone();
            editor.value = next;
            editor.cursor = editor.value.len();
            return;
        }

        // Compute fresh completions from the current value.
        self.path_completions = path_completions(&editor.value);
        self.path_completion_idx = 0;

        match self.path_completions.len() {
            0 => {
                self.status_message = Some("No completions".to_string());
            }
            1 => {
                editor.value = self.path_completions[0].clone();
                editor.cursor = editor.value.len();
            }
            _ => {
                // Complete to the longest common prefix.
                let common = longest_common_prefix(&self.path_completions);
                if common != editor.value {
                    // Advance to the common prefix without cycling yet.
                    editor.value = common;
                    editor.cursor = editor.value.len();
                    // Reset completions so the next Tab starts cycling.
                    self.path_completion_idx = 0;
                } else {
                    // Already at the common prefix — start cycling.
                    let first = self.path_completions[0].clone();
                    editor.value = first;
                    editor.cursor = editor.value.len();
                }
            }
        }
    }

    // ── Settings import / export ──

    fn export_settings(&mut self, path: &str) {
        match serde_yaml::to_string(&self.config) {
            Ok(yaml) => match std::fs::write(path, yaml) {
                Ok(()) => {
                    self.status_message =
                        Some(format!("Settings exported to {}", path));
                }
                Err(e) => {
                    self.status_message = Some(format!("Export failed: {}", e));
                }
            },
            Err(e) => {
                self.status_message = Some(format!("Serialise failed: {}", e));
            }
        }
    }

    fn do_export_json(&mut self, path: &str) {
        let path = expand_tilde(path);
        match export_csl_json(&self.database) {
            Ok(json) => match std::fs::write(&path, json) {
                Ok(()) => {
                    self.status_message = Some(format!("Exported {} entries as CSL-JSON to {}", self.database.entries.len(), path));
                }
                Err(e) => {
                    self.status_message = Some(format!("Export failed: {}", e));
                }
            },
            Err(e) => {
                self.status_message = Some(format!("CSL-JSON serialisation failed: {}", e));
            }
        }
    }

    fn do_export_ris(&mut self, path: &str) {
        let path = expand_tilde(path);
        let ris = export_ris(&self.database);
        match std::fs::write(&path, ris) {
            Ok(()) => {
                self.status_message = Some(format!("Exported {} entries as RIS to {}", self.database.entries.len(), path));
            }
            Err(e) => {
                self.status_message = Some(format!("Export failed: {}", e));
            }
        }
    }

    /// Sync all runtime fields that shadow config values, and rebuild the theme.
    /// Call this whenever the config is mutated (settings toggle, edit, or import).
    fn sync_runtime_from_config(&mut self) {
        self.render_latex = self.config.display.render_latex;
        self.show_braces  = self.config.display.show_braces;
        self.show_groups  = self.config.display.show_groups;
        self.theme        = Theme::from_config(&self.config.theme);
        // If the detail view is open, rebuild display items with current field groups.
        if let Some(key) = self.detail_entry_key.clone() {
            if let Some(entry) = self.database.entries.get(&key) {
                let entry_clone = entry.clone();
                let groups = self.config.field_groups.clone();
                if let Some(ref mut detail) = self.detail_state {
                    detail.refresh_with_groups(&entry_clone, groups);
                }
            }
        }
    }

    fn import_settings(&mut self, path: &str) {
        match std::fs::read_to_string(path) {
            Ok(contents) => match serde_yaml::from_str::<crate::config::schema::Config>(&contents) {
                Ok(mut cfg) => {
                    crate::config::defaults::normalize_citekey_templates(&mut cfg);
                    self.config = cfg;
                    self.sync_runtime_from_config();
                    // Refresh settings panel to reflect imported values
                    self.settings_state = Some(SettingsState::new(&self.config));
                    self.status_message =
                        Some(format!("Settings imported from {}", path));
                }
                Err(e) => {
                    self.status_message = Some(format!("Parse failed: {}", e));
                }
            },
            Err(e) => {
                self.status_message = Some(format!("Read failed: {}", e));
            }
        }
    }

    /// Rename attached files to match the citation key, updating the `file` field in place.
    ///
    /// - One file  →  `citekey.ext`
    /// - N files   →  `citekey_1.ext`, `citekey_2.ext`, …
    ///
    /// All entries with a `file` field are processed, regardless of dirty state.
    /// The actual file is renamed on disk; if the rename fails the entry is left unchanged.
    fn sync_filenames(&mut self) {
        if !self.config.save.sync_filenames {
            return;
        }

        let file_dir = effective_file_dir(
            &self.bib_path,
            self.database.jabref_meta.file_directory.as_deref(),
        );

        let keys: Vec<String> = self
            .database
            .entries
            .iter()
            .filter(|(_, e)| e.fields.contains_key("file"))
            .map(|(k, _)| k.clone())
            .collect();

        let mut rename_msgs: Vec<String> = Vec::new();

        for key in keys {
            let (citekey, file_val) = {
                let entry = &self.database.entries[&key];
                (entry.citation_key.clone(), entry.fields["file"].clone())
            };

            let mut parsed = parse_file_field(&file_val);
            if parsed.is_empty() {
                continue;
            }

            let multi = parsed.len() > 1;
            let mut changed = false;

            for (i, pf) in parsed.iter_mut().enumerate() {
                let old_rel = PathBuf::from(&pf.path);
                let ext = old_rel
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("pdf")
                    .to_string();

                let safe_stem = crate::util::import::sanitize_filename_stem(&citekey);
                let new_filename = if multi {
                    format!("{}_{}.{}", safe_stem, i + 1, ext)
                } else {
                    format!("{}.{}", safe_stem, ext)
                };

                // Already correctly named?
                if old_rel.file_name().and_then(|n| n.to_str()) == Some(&new_filename) {
                    continue;
                }

                // Resolve to absolute paths.
                let old_abs = if old_rel.is_absolute() {
                    old_rel.clone()
                } else {
                    file_dir.join(&old_rel)
                };
                let new_abs = old_abs
                    .parent()
                    .map(|p| p.join(&new_filename))
                    .unwrap_or_else(|| file_dir.join(&new_filename));

                if old_abs.exists() {
                    if let Err(e) = std::fs::rename(&old_abs, &new_abs) {
                        rename_msgs.push(format!("rename {}: {}", old_abs.display(), e));
                        continue;
                    }
                }

                // Update path in the parsed struct, preserving relative vs absolute.
                pf.path = if old_rel.is_absolute() {
                    new_abs.to_string_lossy().into_owned()
                } else {
                    old_rel
                        .parent()
                        .map(|p| p.join(&new_filename))
                        .unwrap_or_else(|| PathBuf::from(&new_filename))
                        .to_string_lossy()
                        .into_owned()
                };
                changed = true;
            }

            if changed {
                let new_file_val = serialize_file_field(&parsed);
                if let Some(entry) = self.database.entries.get_mut(&key) {
                    entry.fields.insert("file".to_string(), new_file_val);
                    entry.dirty = true;
                }
            }
        }

        if !rename_msgs.is_empty() {
            self.status_message = Some(format!("File rename errors: {}", rename_msgs.join("; ")));
        }
    }

    /// Compute the (old_filename, new_filename) pairs that `sync_filenames`
    /// would rename, without touching the filesystem.  Returns an empty vec
    /// when sync is disabled or nothing would change.
    fn compute_sync_renames(&self) -> Vec<(String, String)> {
        if !self.config.save.sync_filenames {
            return Vec::new();
        }

        let file_dir = effective_file_dir(
            &self.bib_path,
            self.database.jabref_meta.file_directory.as_deref(),
        );

        let mut renames = Vec::new();

        for (_, entry) in &self.database.entries {
            let file_val = match entry.fields.get("file") {
                Some(v) => v,
                None => continue,
            };
            let citekey = &entry.citation_key;
            let parsed = parse_file_field(file_val);
            if parsed.is_empty() {
                continue;
            }
            let multi = parsed.len() > 1;
            for (i, pf) in parsed.iter().enumerate() {
                let old_rel = PathBuf::from(&pf.path);
                let ext = old_rel
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("pdf")
                    .to_string();
                let safe_stem = crate::util::import::sanitize_filename_stem(&citekey);
                let new_filename = if multi {
                    format!("{}_{}.{}", safe_stem, i + 1, ext)
                } else {
                    format!("{}.{}", safe_stem, ext)
                };
                if old_rel.file_name().and_then(|n| n.to_str()) == Some(&new_filename) {
                    continue; // Already correctly named.
                }
                let old_display = old_rel
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(&pf.path)
                    .to_string();
                // Show the directory-relative context for absolute paths.
                let old_display = if old_rel.is_absolute() {
                    let rel = old_rel.strip_prefix(&file_dir).unwrap_or(&old_rel);
                    rel.to_string_lossy().into_owned()
                } else {
                    old_display
                };
                renames.push((old_display, new_filename));
            }
        }

        renames.sort();
        renames
    }

    /// Begin a save, showing a filename-sync preview dialog first if any files
    /// would be renamed.  `and_quit` causes the app to exit after saving.
    fn request_save(&mut self, and_quit: bool) {
        let renames = self.compute_sync_renames();
        if renames.is_empty() {
            self.save();
            if and_quit {
                self.should_quit = true;
            }
        } else {
            self.dialog_state = Some(DialogState::file_sync_preview(renames));
            self.pending_action = Some(if and_quit {
                PendingAction::SaveAndQuit
            } else {
                PendingAction::Save
            });
            self.mode = InputMode::Dialog;
        }
    }

    fn save(&mut self) {
        // Rename attached files to match citation keys before serialising.
        self.sync_filenames();

        // Apply save actions (field normalisations) to all entries.
        self.apply_save_actions();

        // Regenerate all citation keys from templates (after field normalisations
        // so the keys are based on the final, normalised field values).
        if self.config.save.save_action_regenerate_citekeys {
            let n = self.regen_all_citekeys_impl(false);
            if n > 0 {
                self.sorted_keys = sort_entries(&self.database.entries, &self.config);
            }
        }

        // Backup — only when the file already exists (skip for brand-new libraries).
        if self.config.general.backup_on_save && self.bib_path.exists() {
            let backup_path = self.bib_path.with_extension("bib.bak");
            if let Err(e) = std::fs::copy(&self.bib_path, &backup_path) {
                self.status_message = Some(format!("Backup failed: {}", e));
                return;
            }
        }

        // Update raw file for dirty entries
        self.sync_dirty_entries();

        // Re-order entries in the raw file if configured.
        self.sort_entries_for_save();

        // Write (normalise blank lines so no more than one blank line appears anywhere)
        let output = normalize_blank_lines(write_bib_file(&self.database.raw_file));
        match std::fs::write(&self.bib_path, &output) {
            Ok(()) => {
                self.save_generation = Some(self.undo_stack.len());
                self.dirty = false;
                // Mark all entries clean
                for entry in self.database.entries.values_mut() {
                    entry.dirty = false;
                }
                self.status_message = Some(format!("Saved to {}", self.bib_path.display()));
            }
            Err(e) => {
                self.status_message = Some(format!("Save failed: {}", e));
            }
        }
    }

    /// Dry-run of [`apply_save_actions`]: returns every field that *would* change
    /// without mutating the database.  Violations are listed in the same stable
    /// order as the real save actions.
    fn compute_violations(&self) -> Vec<Violation> {
        const TEXT: &[&str] = &[
            "abstract", "addendum", "address", "afterword", "annote",
            "booktitle", "chapter", "edition", "institution", "journal",
            "keywords", "language", "note", "organization", "publisher",
            "school", "series", "subtitle", "title", "titleaddon", "type",
            "venue",
        ];
        const NAMES: &[&str] = &[
            "author", "editor", "editora", "editorb", "editorc",
            "bookauthor", "afterword", "translator",
        ];

        let cfg = &self.config.save;
        let mut violations: Vec<Violation> = Vec::new();

        for (key, entry) in &self.database.entries {
            // Simulate each save action on a per-field current value, accumulating
            // transforms in the same order as apply_save_actions so that the
            // final (new_value, old_value) pair reflects the net change.
            // We only report a violation when the final value differs from the
            // original stored value.

            // Collect the fields we care about and simulate sequentially.
            let mut field_state: IndexMap<&str, String> = IndexMap::new();
            let relevant: Vec<&str> = TEXT
                .iter()
                .copied()
                .chain(NAMES.iter().copied())
                .chain(["url", "date", "month", "pages", "isbn"])
                .collect();

            for &f in &relevant {
                if let Some(v) = entry.fields.get(f) {
                    field_state.insert(f, v.clone());
                }
            }

            macro_rules! transform {
                ($field:expr, $fn:expr) => {{
                    if let Some(val) = field_state.get_mut($field) {
                        *val = $fn(val.as_str());
                    }
                }};
            }

            // 1. Unicode → LaTeX
            if cfg.save_action_unicode_to_latex {
                for f in TEXT.iter().copied().chain(NAMES.iter().copied()) {
                    transform!(f, unicode_to_latex);
                }
            }
            // 2. Escape underscores
            if cfg.save_action_escape_underscores {
                for f in TEXT.iter().copied() {
                    transform!(f, escape_underscores);
                }
            }
            // 3. Escape ampersands
            if cfg.save_action_escape_ampersands {
                for f in TEXT.iter().copied().chain(NAMES.iter().copied()) {
                    transform!(f, escape_ampersands);
                }
            }
            // 4. LaTeX cleanup
            if cfg.save_action_latex_cleanup {
                for f in TEXT.iter().copied() {
                    transform!(f, latex_cleanup);
                }
            }
            // 5. URL cleanup
            if cfg.save_action_cleanup_url {
                transform!("url", cleanup_url);
            }
            // 6. Ordinals to superscript
            if cfg.save_action_ordinals_to_superscript {
                for f in TEXT.iter().copied() {
                    transform!(f, ordinals_to_superscript);
                }
            }
            // 7. Normalise date
            if cfg.save_action_normalize_date {
                transform!("date", normalize_date);
            }
            // 8. Normalise month
            if cfg.save_action_normalize_month {
                transform!("month", normalize_month);
            }
            // 9. Normalise page numbers
            if cfg.save_action_normalize_page_numbers {
                transform!("pages", normalize_page_numbers);
            }
            // 10. Normalise ISBN
            if cfg.save_action_normalize_isbn {
                transform!("isbn", normalize_isbn);
            }
            // 11. Normalise person names
            if cfg.save_action_normalize_names_of_persons {
                for f in NAMES.iter().copied() {
                    transform!(f, crate::util::author::normalize_author_names);
                }
            }

            // Emit one violation per field whose net value changed.
            for (field, new_val) in &field_state {
                if let Some(orig) = entry.fields.get(*field) {
                    if new_val != orig {
                        violations.push(Violation {
                            entry_key: key.clone(),
                            field: field.to_string(),
                            old_value: orig.clone(),
                            new_value: new_val.clone(),
                            action_name: action_label_for_field(field, cfg),
                        });
                    }
                }
            }

            // 12. Abbreviate journal — simulate sync of journal, journal_full, journal_abbrev
            if cfg.save_action_abbreviate_journal {
                let journal_current = field_state.get("journal").cloned()
                    .or_else(|| entry.fields.get("journal").cloned())
                    .unwrap_or_default();

                if !journal_current.is_empty() {
                    let full = entry.fields
                        .get("journal_full")
                        .filter(|v| !v.is_empty())
                        .cloned()
                        .unwrap_or_else(|| journal_current.clone());

                    let abbrev = crate::util::journal::abbreviate_journal(
                        &full, &cfg.journal_abbreviations,
                    );
                    let preferred = if cfg.journal_field_content == "abbreviated" {
                        abbrev.clone()
                    } else {
                        full.clone()
                    };

                    for (field_name, new_val, orig_key) in [
                        ("journal_full",   &full,      "journal_full"),
                        ("journal_abbrev", &abbrev,    "journal_abbrev"),
                        ("journal",        &preferred, "journal"),
                    ] {
                        let orig = entry.fields.get(orig_key).cloned().unwrap_or_default();
                        if *new_val != orig {
                            violations.push(Violation {
                                entry_key: key.clone(),
                                field: field_name.to_string(),
                                old_value: orig,
                                new_value: new_val.clone(),
                                action_name: "abbreviate_journal",
                            });
                        }
                    }
                }
            }
        }

        violations
    }

    /// Apply the enabled save actions to every entry in the database.
    ///
    /// Entries whose fields change are marked dirty so they are re-serialised.
    /// Actions are applied in a stable order: unicode conversion first (so that
    /// subsequent escaping acts on LaTeX sequences), then escaping, cleanup,
    /// normalisation, and finally ordinal superscripts.
    fn apply_save_actions(&mut self) {
        // Text fields that contain natural-language prose / titles.
        const TEXT: &[&str] = &[
            "abstract", "addendum", "address", "afterword", "annote",
            "booktitle", "chapter", "edition", "institution", "journal",
            "keywords", "language", "note", "organization", "publisher",
            "school", "series", "subtitle", "title", "titleaddon", "type",
            "venue",
        ];
        // Person-name fields.
        const NAMES: &[&str] = &[
            "author", "editor", "editora", "editorb", "editorc",
            "bookauthor", "afterword", "translator",
        ];

        let cfg = self.config.save.clone();
        let keys: Vec<String> = self.database.entries.keys().cloned().collect();

        for key in &keys {
            let entry = match self.database.entries.get_mut(key) {
                Some(e) => e,
                None => continue,
            };
            let mut changed = false;

            // Helper: apply a &str → String function to a named field.
            macro_rules! apply {
                ($field:expr, $fn:expr) => {{
                    if let Some(val) = entry.fields.get($field) {
                        let new_val = $fn(val.as_str());
                        if new_val != *val {
                            entry.fields.insert($field.to_string(), new_val);
                            changed = true;
                        }
                    }
                }};
            }

            // 1. Unicode → LaTeX (run first so later escaping sees LaTeX text)
            if cfg.save_action_unicode_to_latex {
                for f in TEXT.iter().copied().chain(NAMES.iter().copied()) {
                    apply!(f, unicode_to_latex);
                }
            }

            // 2. Escape underscores
            if cfg.save_action_escape_underscores {
                for f in TEXT.iter().copied() {
                    apply!(f, escape_underscores);
                }
            }

            // 3. Escape ampersands
            if cfg.save_action_escape_ampersands {
                for f in TEXT.iter().copied().chain(NAMES.iter().copied()) {
                    apply!(f, escape_ampersands);
                }
            }

            // 4. LaTeX cleanup (% escaping, space collapsing)
            if cfg.save_action_latex_cleanup {
                for f in TEXT.iter().copied() {
                    apply!(f, latex_cleanup);
                }
            }

            // 5. URL cleanup
            if cfg.save_action_cleanup_url {
                apply!("url", cleanup_url);
            }

            // 6. Ordinals to superscript
            if cfg.save_action_ordinals_to_superscript {
                for f in TEXT.iter().copied() {
                    apply!(f, ordinals_to_superscript);
                }
            }

            // 7. Normalise date
            if cfg.save_action_normalize_date {
                apply!("date", normalize_date);
            }

            // 8. Normalise month
            if cfg.save_action_normalize_month {
                apply!("month", normalize_month);
            }

            // 9. Normalise page numbers
            if cfg.save_action_normalize_page_numbers {
                apply!("pages", normalize_page_numbers);
            }

            // 10. Normalise ISBN
            if cfg.save_action_normalize_isbn {
                apply!("isbn", normalize_isbn);
            }

            // 11. Normalise person names
            if cfg.save_action_normalize_names_of_persons {
                for f in NAMES.iter().copied() {
                    apply!(f, crate::util::author::normalize_author_names);
                }
            }

            // 11. Abbreviate journal name — sync journal, journal_full, journal_abbrev
            if cfg.save_action_abbreviate_journal {
                if let Some(journal_val) = entry.fields.get("journal").cloned() {
                    if !journal_val.is_empty() {
                        // journal_full is the source of truth; fall back to journal on first run
                        let full = entry.fields
                            .get("journal_full")
                            .filter(|v| !v.is_empty())
                            .cloned()
                            .unwrap_or_else(|| journal_val.clone());

                        let abbrev = crate::util::journal::abbreviate_journal(
                            &full, &cfg.journal_abbreviations,
                        );
                        let preferred = if cfg.journal_field_content == "abbreviated" {
                            abbrev.clone()
                        } else {
                            full.clone()
                        };

                        let needs_update =
                            entry.fields.get("journal_full").map_or(true, |v| v != &full)
                            || entry.fields.get("journal_abbrev").map_or(true, |v| v != &abbrev)
                            || entry.fields.get("journal").map_or(true, |v| v != &preferred);

                        if needs_update {
                            entry.fields.insert("journal_full".to_string(), full);
                            entry.fields.insert("journal_abbrev".to_string(), abbrev);
                            entry.fields.insert("journal".to_string(), preferred);
                            changed = true;
                        }
                    }
                }
            }

            if changed {
                entry.dirty = true;
            }
        }
    }

    fn sync_dirty_entries(&mut self) {
        let sort_fields = self.config.save.field_order == "alphabetical";
        // When alphabetical ordering is enabled, re-serialize all entries (not just
        // dirty ones) so that existing entries also get their fields reordered.
        let keys_to_sync: Vec<String> = self
            .database
            .entries
            .iter()
            .filter(|(_, e)| e.dirty || sort_fields)
            .map(|(k, _)| k.clone())
            .collect();

        for key in keys_to_sync {
            if let Some(entry) = self.database.entries.get(&key) {
                let serialized =
                    serialize_entry(entry, self.config.save.align_fields, sort_fields);

                if entry.raw_index < self.database.raw_file.items.len() {
                    // Update existing raw item in-place (no length change)
                    self.database.raw_file.items[entry.raw_index] =
                        RawItem::Entry(RawEntry {
                            entry_type: entry.entry_type.display_name().to_string(),
                            citation_key: entry.citation_key.clone(),
                            fields: Vec::new(), // Not used for passthrough
                            align_width: 0,
                            trailing_comma: true,
                            raw_text: serialized,
                        });
                } else {
                    // New entry — insert before the JabRef @Comment blocks
                    let insert_pos = self
                        .database
                        .raw_file
                        .items
                        .iter()
                        .position(|item| matches!(item, RawItem::Comment { .. }))
                        .unwrap_or(self.database.raw_file.items.len());

                    // Insert: blank line, entry, blank line (before @Comment).
                    // normalize_blank_lines will collapse any excess, ensuring exactly
                    // one blank line on each side of the new entry.
                    self.database.raw_file.items.insert(
                        insert_pos,
                        RawItem::Preamble("\n".to_string()),
                    );
                    self.database.raw_file.items.insert(
                        insert_pos + 1,
                        RawItem::Entry(RawEntry {
                            entry_type: entry.entry_type.display_name().to_string(),
                            citation_key: entry.citation_key.clone(),
                            fields: Vec::new(),
                            align_width: 0,
                            trailing_comma: true,
                            raw_text: serialized,
                        }),
                    );
                    self.database.raw_file.items.insert(
                        insert_pos + 2,
                        RawItem::Preamble("\n".to_string()),
                    );
                }
            }
        }

        // Remove raw items for deleted entries. Process in reverse index order
        // so that earlier removals don't shift indices of later ones.
        if !self.deleted_raw_indices.is_empty() {
            let mut to_remove = self.deleted_raw_indices.drain(..).collect::<Vec<_>>();
            to_remove.sort_unstable_by(|a, b| b.cmp(a)); // descending
            to_remove.dedup();
            for idx in to_remove {
                if idx < self.database.raw_file.items.len() {
                    self.database.raw_file.items.remove(idx);
                    // Also remove the preceding blank Preamble separator if present
                    if idx > 0 {
                        if let Some(RawItem::Preamble(s)) =
                            self.database.raw_file.items.get(idx - 1)
                        {
                            if s.trim().is_empty() {
                                self.database.raw_file.items.remove(idx - 1);
                            }
                        }
                    }
                }
            }
        }
    }

    /// Reorder `Entry` slots in `raw_file.items` according to `config.save.entry_sort_order`,
    /// then rebuild `raw_index` in all database entries so future saves remain correct.
    fn sort_entries_for_save(&mut self) {
        if self.config.save.entry_sort_order == "none" {
            return;
        }

        // Positions of every Entry item in the raw list.
        let entry_indices: Vec<usize> = self
            .database
            .raw_file
            .items
            .iter()
            .enumerate()
            .filter_map(|(i, item)| if matches!(item, RawItem::Entry(_)) { Some(i) } else { None })
            .collect();

        if entry_indices.len() < 2 {
            return;
        }

        // Extract those items, sort them by citation key (case-insensitive).
        let mut sorted: Vec<RawItem> = entry_indices
            .iter()
            .map(|&i| self.database.raw_file.items[i].clone())
            .collect();
        sorted.sort_by(|a, b| {
            let ka = if let RawItem::Entry(e) = a { e.citation_key.to_lowercase() } else { String::new() };
            let kb = if let RawItem::Entry(e) = b { e.citation_key.to_lowercase() } else { String::new() };
            ka.cmp(&kb)
        });

        // Write them back into the same index slots (non-entry items stay put).
        for (&idx, item) in entry_indices.iter().zip(sorted) {
            self.database.raw_file.items[idx] = item;
        }

        // Rebuild raw_index for every database entry so future saves are correct.
        for (i, item) in self.database.raw_file.items.iter().enumerate() {
            if let RawItem::Entry(re) = item {
                if let Some(db_entry) = self.database.entries.get_mut(&re.citation_key) {
                    db_entry.raw_index = i;
                }
            }
        }
    }

    // ── Group management ──

    fn start_add_group(&mut self) {
        let parent_path = self
            .group_tree_state
            .selected_item()
            .map(|item| item.path.clone())
            .unwrap_or_default();
        self.field_editor_state = Some(FieldEditorState::for_input("Group name"));
        self.pending_action = Some(PendingAction::AddGroup { parent_path });
        self.mode = InputMode::Editing;
    }

    fn start_delete_group(&mut self) {
        let item = match self.group_tree_state.selected_item() {
            Some(item) => item.clone(),
            None => return,
        };
        if item.depth == 0 {
            self.status_message = Some("Cannot delete root group".to_string());
            return;
        }
        let name = item.name.clone();
        let path = item.path.clone();
        self.dialog_state = Some(DialogState::confirm(
            "Delete Group",
            &format!("Delete group '{}'?", name),
        ));
        self.pending_action = Some(PendingAction::DeleteGroup { path });
        self.mode = InputMode::Dialog;
    }

    fn start_edit_groups(&mut self) {
        let entry_key = match self.detail_entry_key.clone() {
            Some(k) => k,
            None => return,
        };
        let entry = match self.database.entries.get(&entry_key) {
            Some(e) => e,
            None => return,
        };
        let memberships = entry.group_memberships.clone();
        let mut group_names = Vec::new();
        collect_group_names(&self.database.groups.root, &mut group_names);
        if group_names.is_empty() {
            self.status_message = Some("No groups defined".to_string());
            return;
        }
        let groups: Vec<(String, bool)> = group_names
            .into_iter()
            .map(|name| {
                let checked = memberships.contains(&name);
                (name, checked)
            })
            .collect();
        self.dialog_state = Some(DialogState::group_assign(groups));
        self.pending_action = Some(PendingAction::AssignGroups { entry_key });
        self.mode = InputMode::Dialog;
    }

    fn finish_add_group(&mut self, name: String, parent_path: Vec<usize>) {
        self.push_undo(UndoItem::GroupTreeChanged { old_tree: self.database.groups.clone() });
        let new_node = GroupNode {
            group: Group {
                name: name.clone(),
                group_type: GroupType::Static,
            },
            children: Vec::new(),
            expanded: true,
        };
        if let Some(parent) =
            find_group_node_mut(&mut self.database.groups.root, &parent_path)
        {
            parent.children.push(new_node);
        }
        self.sync_groups_to_raw();
        self.group_tree_state.refresh(&self.database.groups);
        self.status_message = Some(format!("Group '{}' added", name));
    }

    fn finish_delete_group(&mut self, path: Vec<usize>) {
        if path.is_empty() {
            return;
        }
        self.push_undo(UndoItem::GroupTreeChanged { old_tree: self.database.groups.clone() });
        let (parent_path, child_idx) = path.split_at(path.len() - 1);
        let child_idx = child_idx[0];
        if let Some(parent) =
            find_group_node_mut(&mut self.database.groups.root, parent_path)
        {
            if child_idx < parent.children.len() {
                let removed = parent.children.remove(child_idx);
                self.sync_groups_to_raw();
                self.group_tree_state.refresh(&self.database.groups);
                // Clear active group filter if the deleted group was active
                if self.group_tree_state.active_group.as_deref()
                    == Some(removed.group.name.as_str())
                {
                    self.group_tree_state.active_group = None;
                    self.filtered_indices = None;
                }
                self.status_message =
                    Some(format!("Group '{}' deleted", removed.group.name));
            }
        }
    }

    fn finish_assign_groups(&mut self, entry_key: &str, selected_groups: Vec<String>) {
        // Snapshot before mutating (avoid holding a mutable borrow while calling push_undo)
        let undo_item = self.database.entries.get(entry_key).map(|entry| {
            UndoItem::GroupMembershipChanged {
                entry_key: entry_key.to_string(),
                old_memberships: entry.group_memberships.clone(),
                old_groups_field: entry.fields.get("groups").cloned(),
            }
        });
        if let Some(item) = undo_item {
            self.push_undo(item);
        }
        if let Some(entry) = self.database.entries.get_mut(entry_key) {
            if selected_groups.is_empty() {
                entry.fields.shift_remove("groups");
            } else {
                entry
                    .fields
                    .insert("groups".to_string(), selected_groups.join(","));
            }
            entry.group_memberships = selected_groups;
            entry.dirty = true;
            let entry_clone = entry.clone();
            if let Some(ref mut detail) = self.detail_state {
                detail.refresh(&entry_clone);
            }
        }
    }

    // ── Undo ──

    fn push_undo(&mut self, item: UndoItem) {
        if self.undo_stack.len() >= MAX_UNDO {
            self.undo_stack.remove(0);
            // Shift the save-generation marker; if it was already at 0 the
            // save point has been evicted and can never be reached again.
            self.save_generation = self.save_generation.and_then(|g| g.checked_sub(1));
        }
        self.undo_stack.push(item);
        self.dirty = self.save_generation != Some(self.undo_stack.len());
    }

    fn undo(&mut self) {
        let Some(item) = self.undo_stack.pop() else {
            self.status_message = Some("Nothing to undo".to_string());
            return;
        };

        match item {
            UndoItem::FieldChanged { entry_key, field_name, old_value } => {
                if let Some(entry) = self.database.entries.get_mut(&entry_key) {
                    match old_value {
                        Some(v) => { entry.fields.insert(field_name.clone(), v); }
                        None    => { entry.fields.shift_remove(&field_name); }
                    }
                    entry.dirty = true;
                    if self.detail_entry_key.as_deref() == Some(entry_key.as_str()) {
                        let snapshot = entry.clone();
                        if let Some(ref mut detail) = self.detail_state {
                            detail.refresh(&snapshot);
                        }
                    }
                }
                self.status_message = Some(format!("Undo: field '{}'", field_name));
            }
            UndoItem::EntryDeleted { entry } => {
                let key = entry.citation_key.clone();
                // If the raw_index was queued for removal, cancel that
                if let Some(pos) = self.deleted_raw_indices.iter().position(|&i| i == entry.raw_index) {
                    self.deleted_raw_indices.remove(pos);
                }
                self.database.entries.insert(key.clone(), entry);
                self.sorted_keys = sort_entries(&self.database.entries, &self.config);
                self.status_message = Some(format!("Undo: restored '{}'", key));
            }
            UndoItem::EntryAdded { entry_key } => {
                if let Some(entry) = self.database.entries.get(&entry_key) {
                    if entry.raw_index != usize::MAX {
                        self.deleted_raw_indices.push(entry.raw_index);
                    }
                }
                self.database.entries.shift_remove(&entry_key);
                self.sorted_keys = sort_entries(&self.database.entries, &self.config);
                if self.detail_entry_key.as_deref() == Some(entry_key.as_str()) {
                    self.close_detail();
                }
                self.status_message = Some(format!("Undo: removed '{}'", entry_key));
            }
            UndoItem::EntryTypeChanged { entry_key, old_type } => {
                let old_name = old_type.display_name().to_string();
                if let Some(entry) = self.database.entries.get_mut(&entry_key) {
                    entry.entry_type = old_type;
                    entry.dirty = true;
                    if self.detail_entry_key.as_deref() == Some(entry_key.as_str()) {
                        let snapshot = entry.clone();
                        if let Some(ref mut detail) = self.detail_state {
                            detail.refresh(&snapshot);
                        }
                    }
                }
                self.status_message = Some(format!("Undo: type reverted to {}", old_name));
            }
            UndoItem::CitekeyChanged { old_key, new_key, entry_snapshot } => {
                self.database.entries.shift_remove(&new_key);
                let mut entry = entry_snapshot;
                entry.citation_key = old_key.clone();
                self.database.entries.insert(old_key.clone(), entry);
                if self.detail_entry_key.as_deref() == Some(new_key.as_str()) {
                    self.detail_entry_key = Some(old_key.clone());
                    if let Some(e) = self.database.entries.get(&old_key) {
                        let snapshot = e.clone();
                        if let Some(ref mut detail) = self.detail_state {
                            detail.refresh(&snapshot);
                        }
                    }
                }
                self.sorted_keys = sort_entries(&self.database.entries, &self.config);
                self.status_message = Some(format!("Undo: key reverted to '{}'", old_key));
            }
            UndoItem::GroupTreeChanged { old_tree } => {
                self.database.groups = old_tree;
                self.sync_groups_to_raw();
                self.group_tree_state.refresh(&self.database.groups);
                self.status_message = Some("Undo: group change".to_string());
            }
            UndoItem::GroupMembershipChanged { entry_key, old_memberships, old_groups_field } => {
                if let Some(entry) = self.database.entries.get_mut(&entry_key) {
                    entry.group_memberships = old_memberships;
                    match old_groups_field {
                        Some(v) => { entry.fields.insert("groups".to_string(), v); }
                        None    => { entry.fields.shift_remove("groups"); }
                    }
                    entry.dirty = true;
                    if self.detail_entry_key.as_deref() == Some(entry_key.as_str()) {
                        let snapshot = entry.clone();
                        if let Some(ref mut detail) = self.detail_state {
                            detail.refresh(&snapshot);
                        }
                    }
                }
                self.status_message = Some("Undo: group membership".to_string());
            }
        }

        // Recompute dirty from the save-generation marker now that the stack shrank.
        self.dirty = self.save_generation != Some(self.undo_stack.len());

        // If we've returned to the exact saved state, clear per-entry dirty flags
        // too so the entry-list indicator disappears.
        if !self.dirty {
            for entry in self.database.entries.values_mut() {
                entry.dirty = false;
            }
        }
    }

    fn sync_groups_to_raw(&mut self) {
        let serialized = serialize_group_tree(&self.database.groups);
        let new_raw = format!("@Comment{{jabref-meta: grouping:\n{};}}", serialized);
        for item in &mut self.database.raw_file.items {
            if let RawItem::Comment { raw_text } = item {
                if raw_text.contains("jabref-meta: grouping:") {
                    *raw_text = new_raw;
                    self.database
                        .jabref_meta
                        .unknown_meta
                        .insert("grouping".to_string(), serialized);
                    return;
                }
            }
        }
        // No existing grouping comment — append one
        self.database
            .raw_file
            .items
            .push(RawItem::Comment { raw_text: new_raw });
        self.database
            .jabref_meta
            .unknown_meta
            .insert("grouping".to_string(), serialized);
    }

    // ── Import ────────────────────────────────────────────────────────────────

    /// Open a field-editor prompt asking the user to enter a DOI, URL, or local PDF path.
    fn start_import_entry(&mut self) {
        self.field_editor_state = Some(
            crate::tui::components::field_editor::FieldEditorState::for_path(
                "DOI, ISBN, URL, or PDF file path",
                "",
            ),
        );
        self.pending_action = Some(PendingAction::ImportUrl);
        self.mode = InputMode::Editing;
    }

    /// Spawn a background thread to fetch the entry, storing the receiver for polling.
    fn spawn_import(&mut self, doi_or_url: String) {
        if self.pending_import.is_some() {
            self.status_message =
                Some("Import already in progress — please wait".to_string());
            return;
        }
        let bib_dir = effective_file_dir(
            &self.bib_path,
            self.database.jabref_meta.file_directory.as_deref(),
        );
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let mut result = crate::util::import::fetch(&doi_or_url);
            // If the fetcher already resolved a local PDF (e.g. PdfFetcher), skip download.
            // Otherwise try each URL candidate in order; stop on first successful download.
            if let Ok(ref mut entry) = result {
                if entry.pdf_path.is_none() && !entry.pdf_urls.is_empty() {
                    let doi = entry.fields.get("doi").cloned().unwrap_or_else(|| "import".to_string());
                    let mut last_err: Option<String> = None;
                    for pdf_url in &entry.pdf_urls.clone() {
                        match crate::util::import::download_pdf(pdf_url, &bib_dir, &doi) {
                            Ok(path) => {
                                entry.pdf_path = Some(path);
                                last_err = None;
                                break;
                            }
                            Err(e) => {
                                last_err = Some(e.to_string());
                            }
                        }
                    }
                    entry.pdf_error = last_err;
                }
            }
            let _ = tx.send(result);
        });
        self.pending_import = Some(rx);
        self.status_message = Some("Fetching…".to_string());
    }

    /// Handle the result of a completed background import fetch.
    fn handle_import_result(&mut self, result: crate::util::import::ImportResult) {
        match result {
            Ok(imported) => {
                let entry_type = EntryType::from_str(&imported.entry_type);

                // Generate a citation key from the configured template.
                let display_name = entry_type.display_name();
                let type_name = display_name.to_lowercase();
                let template = self
                    .config
                    .citekey
                    .templates
                    .get(&type_name)
                    .cloned()
                    .unwrap_or_else(|| format!("{}_[year]_[auth]", display_name));
                let temp_key = {
                    let key = generate_citekey(&template, &imported.fields);
                    if key.is_empty() { "imported_entry".to_string() } else { key }
                };

                let mut fields = imported.fields;

                // Titlecase the title and wrap in braces to protect case in BibTeX
                if let Some(raw_title) = fields.get("title").cloned() {
                    let titled = crate::util::titlecase::apply_titlecase(
                        &raw_title,
                        &self.config.titlecase.ignore_words,
                        &self.config.titlecase.stop_words,
                    );
                    fields.insert("title".to_string(), format!("{{{}}}", titled));
                }

                // Normalise imported URL fields (percent-decode, strip trailing slash)
                for url_field in &["url", "doi"] {
                    if let Some(v) = fields.get(*url_field).cloned() {
                        let cleaned = cleanup_url(&v);
                        if cleaned != v {
                            fields.insert(url_field.to_string(), cleaned);
                        }
                    }
                }

                // Set the `file` field using a path relative to the effective file
                // directory (JabRef fileDirectory if set, otherwise the bib parent).
                if let Some(ref pdf_path) = imported.pdf_path {
                    let file_dir = effective_file_dir(
                        &self.bib_path,
                        self.database.jabref_meta.file_directory.as_deref(),
                    );
                    let rel = crate::util::open::make_relative(&file_dir, pdf_path);
                    fields.insert(
                        "file".to_string(),
                        format!(":{}:PDF", rel.to_string_lossy()),
                    );
                }

                let entry = Entry {
                    entry_type,
                    citation_key: temp_key.clone(),
                    fields,
                    group_memberships: Vec::new(),
                    raw_index: usize::MAX,
                    dirty: true,
                };

                // If the key already exists, make it unique
                let key = if self.database.entries.contains_key(&temp_key) {
                    let mut n = 2;
                    loop {
                        let k = format!("{}_{}", temp_key, n);
                        if !self.database.entries.contains_key(&k) {
                            break k;
                        }
                        n += 1;
                    }
                } else {
                    temp_key
                };

                let mut entry = entry;
                entry.citation_key = key.clone();

                self.database.entries.insert(key.clone(), entry);
                self.push_undo(UndoItem::EntryAdded { entry_key: key.clone() });
                self.sorted_keys = sort_entries(&self.database.entries, &self.config);
                self.dirty = true;

                // Open detail view
                self.detail_entry_key = Some(key.clone());
                if let Some(e) = self.database.entries.get(&key) {
                    self.detail_state =
                        Some(EntryDetailState::new(e, self.config.field_groups.clone()));
                }
                self.mode = InputMode::Detail;
                self.status_message = if let Some(ref pdf_err) = imported.pdf_error {
                    Some(format!(
                        "Imported entry (PDF download failed: {}) — press 'c' to regenerate citation key",
                        pdf_err
                    ))
                } else if imported.pdf_path.is_some() {
                    Some("Imported entry with PDF — press 'c' to regenerate citation key".to_string())
                } else {
                    Some("Imported entry — press 'c' to regenerate citation key".to_string())
                };
            }
            Err(e) => {
                self.dialog_state = Some(DialogState::message(
                    "Import Error",
                    &format!("Import failed: {}", e),
                ));
                self.pending_action = Some(PendingAction::DismissMessage);
                self.mode = InputMode::Dialog;
            }
        }
    }

    /// Start a background metadata→DOI lookup for the current entry (detail view or list selection).
    fn start_fetch_doi(&mut self) {
        let entry_key = match self.action_entry_key() {
            Some(k) => k,
            None => {
                self.status_message = Some("No entry selected".to_string());
                return;
            }
        };

        if self.pending_doi_fetch.is_some() {
            self.status_message = Some("DOI lookup already in progress".to_string());
            return;
        }

        let entry = match self.database.entries.get(&entry_key) {
            Some(e) => e,
            None => return,
        };

        let title  = entry.fields.get("title").cloned().unwrap_or_default();
        let author = entry.fields.get("author").cloned().unwrap_or_default();
        let year   = entry.fields.get("year").cloned().unwrap_or_default();

        if title.trim().is_empty() && author.trim().is_empty() {
            self.status_message =
                Some("Entry needs at least a title or author to search".to_string());
            return;
        }

        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let result = crate::util::import::crossref::search_by_metadata(
                &title, &author, &year,
            );
            let _ = tx.send(result);
        });

        self.pending_doi_fetch = Some((entry_key, rx));
        self.status_message = Some("Searching for DOI…".to_string());
    }

    /// Apply the result of a metadata→DOI lookup to the entry.
    fn handle_doi_fetch_result(
        &mut self,
        entry_key: String,
        result: Result<(String, String), String>,
    ) {
        match result {
            Ok((doi, url)) => {
                let mut changed = false;

                // Only set url if it carries information beyond the DOI itself
                // (Crossref often returns https://doi.org/<doi> as the URL, which is redundant).
                let url_is_distinct = !url.is_empty()
                    && crate::util::import::crossref::CrossrefFetcher::extract_doi(&url)
                        .map_or(true, |extracted| extracted != doi);
                let effective_url = if url_is_distinct { url.as_str() } else { "" };

                // Update doi and url fields, recording undo for each changed field.
                for (field, new_val) in [("doi", doi.as_str()), ("url", effective_url)] {
                    if new_val.is_empty() {
                        continue;
                    }
                    let old = self.database.entries.get(&entry_key)
                        .and_then(|e| e.fields.get(field).cloned());
                    if old.as_deref() == Some(new_val) {
                        continue; // No change
                    }
                    self.push_undo(UndoItem::FieldChanged {
                        entry_key: entry_key.clone(),
                        field_name: field.to_string(),
                        old_value: old,
                    });
                    if let Some(entry) = self.database.entries.get_mut(&entry_key) {
                        entry.fields.insert(field.to_string(), new_val.to_string());
                        entry.dirty = true;
                    }
                    changed = true;
                }

                if changed {
                    // Refresh the detail view if this entry is still open
                    if self.detail_entry_key.as_deref() == Some(entry_key.as_str()) {
                        if let Some(entry) = self.database.entries.get(&entry_key) {
                            let entry_clone = entry.clone();
                            if let Some(ref mut detail) = self.detail_state {
                                detail.refresh(&entry_clone);
                            }
                        }
                    }
                    self.dirty = true;
                    self.status_message = Some(format!("Found DOI: {}", doi));
                } else {
                    self.status_message =
                        Some(format!("DOI already up-to-date: {}", doi));
                }
            }
            Err(e) => {
                self.status_message = Some(format!("DOI lookup failed: {}", e));
            }
        }
    }

    /// Filter the entry list to a named group (used by `:group <name>` command).
    fn apply_group_filter(&mut self, group_name: &str) {
        if let Some(node) = find_group_node(&self.database.groups.root, group_name) {
            let entries: Vec<&Entry> = self
                .sorted_keys
                .iter()
                .filter_map(|k| self.database.entries.get(k))
                .collect();
            let indices = filter_by_group(&entries, node);
            self.search_bar_state.result_count = indices.len();
            self.filtered_indices = Some(indices);
            self.group_tree_state.active_group = Some(group_name.to_string());
            self.entry_list_state.select(0);
            self.status_message = Some(format!("Group: {}", group_name));
        } else {
            self.status_message = Some(format!("Group not found: {}", group_name));
        }
    }

    // ── Field editor (vim modal) action handler ───────────────────────────

    fn handle_field_editor_action(&mut self, action: Action) {
        match action {
            Action::EditUndo => {
                if let Some(ref mut editor) = self.field_editor_state {
                    editor.undo_edit();
                }
                self.update_field_completions();
            }
            Action::EditPut => {
                // Use unnamed register (set by x/dw/yy) if non-empty, else fall back to clipboard.
                let text_result = self.field_editor_state.as_ref().map(|editor| {
                    if !editor.unnamed_register.is_empty() {
                        Ok(editor.unnamed_register.clone())
                    } else {
                        crate::util::clipboard::read_from_clipboard()
                            .map_err(|e| format!("Clipboard error: {e}"))
                    }
                });
                match text_result {
                    Some(Ok(text)) if !text.is_empty() => {
                        if let Some(ref mut editor) = self.field_editor_state {
                            editor.save_undo_snapshot();
                            editor.put(&text);
                        }
                        self.update_field_completions();
                    }
                    Some(Err(e)) => self.status_message = Some(e),
                    _ => {}
                }
            }
            Action::EditYank => {
                if let Some(ref mut editor) = self.field_editor_state {
                    let text = editor.value.clone();
                    editor.unnamed_register = text.clone();
                    match crate::util::clipboard::copy_to_clipboard(&text) {
                        Ok(()) => self.status_message = Some(format!("Yanked: {text}")),
                        Err(e) => self.status_message = Some(format!("Clipboard error: {e}")),
                    }
                }
            }
            Action::EditEnterNormal => {
                if let Some(ref mut editor) = self.field_editor_state {
                    editor.enter_normal();
                }
            }
            Action::EditEnterInsert => {
                if let Some(ref mut editor) = self.field_editor_state {
                    editor.save_undo_snapshot();
                    editor.editing_mode = EditingMode::Insert;
                }
            }
            Action::EditEnterInsertAfter => {
                if let Some(ref mut editor) = self.field_editor_state {
                    editor.save_undo_snapshot();
                    // Advance cursor one char past current position (like vim `a`)
                    if editor.cursor < editor.value.len() {
                        let next = editor.value[editor.cursor..]
                            .chars()
                            .next()
                            .map(|c| c.len_utf8())
                            .unwrap_or(0);
                        editor.cursor += next;
                    }
                    editor.editing_mode = EditingMode::Insert;
                }
            }
            Action::EditEnterInsertAtEnd => {
                if let Some(ref mut editor) = self.field_editor_state {
                    editor.save_undo_snapshot();
                    editor.cursor = editor.value.len();
                    editor.editing_mode = EditingMode::Insert;
                }
            }
            Action::EditEnterInsertAtHome => {
                if let Some(ref mut editor) = self.field_editor_state {
                    editor.save_undo_snapshot();
                    editor.cursor = 0;
                    editor.editing_mode = EditingMode::Insert;
                }
            }
            Action::EditEnterReplace => {
                if let Some(ref mut editor) = self.field_editor_state {
                    editor.save_undo_snapshot();
                    editor.replace_undo_stack.clear();
                    editor.editing_mode = EditingMode::Replace;
                }
            }
            Action::EditMoveWordFwd => {
                if let Some(ref mut editor) = self.field_editor_state {
                    editor.move_word_fwd();
                }
            }
            Action::EditMoveWordBwd => {
                if let Some(ref mut editor) = self.field_editor_state {
                    editor.move_word_bwd();
                }
            }
            Action::EditMoveWordEnd => {
                if let Some(ref mut editor) = self.field_editor_state {
                    editor.move_word_end();
                }
            }
            Action::EditMoveBigWordFwd => {
                if let Some(ref mut editor) = self.field_editor_state {
                    editor.move_big_word_fwd();
                }
            }
            Action::EditMoveBigWordBwd => {
                if let Some(ref mut editor) = self.field_editor_state {
                    editor.move_big_word_bwd();
                }
            }
            Action::EditMoveBigWordEnd => {
                if let Some(ref mut editor) = self.field_editor_state {
                    editor.move_big_word_end();
                }
            }
            Action::EditDeleteWordFwd => {
                if let Some(ref mut editor) = self.field_editor_state {
                    editor.save_undo_snapshot();
                    editor.delete_word_fwd();
                }
                self.update_field_completions();
            }
            Action::EditDeleteToEnd => {
                if let Some(ref mut editor) = self.field_editor_state {
                    editor.save_undo_snapshot();
                    editor.delete_to_end();
                }
                self.update_field_completions();
            }
            Action::EditChangeToEnd => {
                if let Some(ref mut editor) = self.field_editor_state {
                    editor.save_undo_snapshot();
                    editor.delete_to_end();
                    editor.editing_mode = EditingMode::Insert;
                }
                self.update_field_completions();
            }
            Action::EditSubstituteChar => {
                if let Some(ref mut editor) = self.field_editor_state {
                    editor.save_undo_snapshot();
                    editor.delete();
                    editor.editing_mode = EditingMode::Insert;
                }
                self.update_field_completions();
            }
            Action::EditSubstituteLine => {
                if let Some(ref mut editor) = self.field_editor_state {
                    editor.save_undo_snapshot();
                    editor.clear_value();
                    editor.editing_mode = EditingMode::Insert;
                }
                self.update_field_completions();
            }
            Action::EditToggleCase => {
                if let Some(ref mut editor) = self.field_editor_state {
                    editor.save_undo_snapshot();
                    editor.toggle_case_at_cursor();
                }
                self.update_field_completions();
            }
            Action::EditReplaceChar(c) => {
                if let Some(ref mut editor) = self.field_editor_state {
                    editor.save_undo_snapshot();
                    editor.replace_char_at_cursor(c);
                }
            }
            Action::EditFindCharFwd(c) => {
                if let Some(ref mut editor) = self.field_editor_state {
                    editor.find_char_fwd(c);
                }
            }
            Action::EditFindCharBwd(c) => {
                if let Some(ref mut editor) = self.field_editor_state {
                    editor.find_char_bwd(c);
                }
            }
            Action::EditFindToCharFwd(c) => {
                if let Some(ref mut editor) = self.field_editor_state {
                    editor.find_to_char_fwd(c);
                }
            }
            Action::EditFindToCharBwd(c) => {
                if let Some(ref mut editor) = self.field_editor_state {
                    editor.find_to_char_bwd(c);
                }
            }
            Action::EditDeleteToChar(c) => {
                if let Some(ref mut editor) = self.field_editor_state {
                    editor.save_undo_snapshot();
                    editor.delete_to_char(c);
                }
                self.update_field_completions();
            }
            Action::EditDeleteThroughChar(c) => {
                if let Some(ref mut editor) = self.field_editor_state {
                    editor.save_undo_snapshot();
                    editor.delete_through_char(c);
                }
                self.update_field_completions();
            }
            Action::EditDeleteToCharBack(c) => {
                if let Some(ref mut editor) = self.field_editor_state {
                    editor.save_undo_snapshot();
                    editor.delete_to_char_back(c);
                }
                self.update_field_completions();
            }
            Action::EditDeleteThroughCharBack(c) => {
                if let Some(ref mut editor) = self.field_editor_state {
                    editor.save_undo_snapshot();
                    editor.delete_through_char_back(c);
                }
                self.update_field_completions();
            }
            Action::EditDeleteCharBack => {
                if let Some(ref mut editor) = self.field_editor_state {
                    editor.save_undo_snapshot();
                    editor.backspace();
                    if editor.editing_mode == EditingMode::Normal
                        && !editor.value.is_empty()
                        && editor.cursor >= editor.value.len()
                    {
                        editor.cursor = editor
                            .value
                            .char_indices()
                            .last()
                            .map(|(i, _)| i)
                            .unwrap_or(0);
                    }
                }
                self.update_field_completions();
            }
            Action::EditDeleteWordBack => {
                if let Some(ref mut editor) = self.field_editor_state {
                    editor.save_undo_snapshot();
                    editor.delete_word_back();
                }
                self.update_field_completions();
            }
            Action::EditDeleteToHome => {
                if let Some(ref mut editor) = self.field_editor_state {
                    editor.save_undo_snapshot();
                    editor.delete_to_home();
                }
                self.update_field_completions();
            }
            Action::EditConfirmAndMoveDown => {
                self.confirm_edit();
                self.move_cursor(1);
                self.start_edit_field();
            }
            Action::EditConfirmAndMoveUp => {
                self.confirm_edit();
                self.move_cursor(-1);
                self.start_edit_field();
            }
            _ => {}
        }
    }

    // ── Settings action handler ───────────────────────────────────────────

    fn handle_settings_action(&mut self, action: Action) {
        match action {
            Action::EnterSettings => {
                let mut s = SettingsState::new(&self.config);
                let row_count = s.rows.len();
                if self.last_settings_cursor < row_count {
                    s.cursor = self.last_settings_cursor;
                }
                self.settings_state = Some(s);
                self.mode = InputMode::Settings;
            }
            Action::ExitSettings => {
                if let Some(ref s) = self.settings_state {
                    self.last_settings_cursor = s.cursor;
                }
                self.settings_state = None;
                self.mode = InputMode::Normal;
            }
            Action::SettingsMoveDown => {
                if let Some(ref mut s) = self.settings_state {
                    s.move_down();
                }
            }
            Action::SettingsMoveUp => {
                if let Some(ref mut s) = self.settings_state {
                    s.move_up();
                }
            }
            Action::SettingsMoveToTop => {
                if let Some(ref mut s) = self.settings_state {
                    s.move_to_top();
                }
            }
            Action::SettingsMoveToBottom => {
                if let Some(ref mut s) = self.settings_state {
                    s.move_to_bottom();
                }
            }
            Action::SettingsPageDown => {
                if let Some(ref mut s) = self.settings_state {
                    s.move_page_down();
                }
            }
            Action::SettingsPageUp => {
                if let Some(ref mut s) = self.settings_state {
                    s.move_page_up();
                }
            }
            Action::SettingsToggle => {
                if let Some(ref mut s) = self.settings_state {
                    if s.selected_item().map(|i| i.value.is_cyclic()).unwrap_or(false) {
                        s.toggle_selected();
                        s.apply_to_config(&mut self.config);
                        self.sync_runtime_from_config();
                    }
                }
            }
            Action::SettingsEdit => {
                if let Some(ref s) = self.settings_state {
                    if s.selected_is_column() {
                        if let Some(idx) = s.selected_column_index() {
                            let width_spec = s.columns.get(idx)
                                .map(|(_, _, w)| w.clone()).unwrap_or_default();
                            self.field_editor_state =
                                Some(FieldEditorState::new("Width (fixed:N / percent:N / flex [max:N])", &width_spec));
                            self.pending_action =
                                Some(PendingAction::EditColumnWidth { index: idx });
                            self.mode = InputMode::Editing;
                        }
                    } else if s.selected_is_field_group() {
                        if let Some(idx) = s.selected_field_group_index() {
                            let fields_csv = s.field_groups.get(idx)
                                .map(|(_, f)| f.clone()).unwrap_or_default();
                            self.field_editor_state =
                                Some(FieldEditorState::new("Fields (comma-separated)", &fields_csv));
                            self.pending_action =
                                Some(PendingAction::EditFieldGroupFields { index: idx });
                            self.mode = InputMode::Editing;
                        }
                    } else if let Some(id) = s.selected_id() {
                        let is_str = s.selected_item()
                            .map(|i| matches!(i.value, SettingValue::Str(_)))
                            .unwrap_or(false);
                        if is_str {
                            let current = s.selected_value_str();
                            let label = s.selected_item().map(|i| i.label.clone()).unwrap_or_else(|| id.to_string());
                            let setting_id = id.to_string();
                            self.field_editor_state =
                                Some(FieldEditorState::new(&label, &current));
                            self.pending_action =
                                Some(PendingAction::EditSetting { setting_id });
                            self.mode = InputMode::Editing;
                        }
                    }
                }
            }
            Action::SettingsAddFieldGroup => {
                if self.settings_state.is_some() {
                    let in_columns = self.settings_state.as_ref()
                        .map(|s| s.current_section() == Some("Columns"))
                        .unwrap_or(false);
                    if in_columns {
                        self.field_editor_state =
                            Some(FieldEditorState::new("Column field name (field or field|header)", ""));
                        self.pending_action = Some(PendingAction::AddColumn);
                    } else {
                        self.field_editor_state =
                            Some(FieldEditorState::new("New field group name", ""));
                        self.pending_action = Some(PendingAction::AddFieldGroup);
                    }
                    self.mode = InputMode::Editing;
                }
            }
            Action::SettingsDeleteFieldGroup => {
                if let Some(ref mut s) = self.settings_state {
                    let deleted = if s.selected_is_column() {
                        s.delete_selected_column()
                    } else {
                        s.delete_selected_field_group()
                    };
                    if deleted {
                        s.apply_to_config(&mut self.config);
                        self.sync_runtime_from_config();
                    }
                }
            }
            Action::SettingsRenameFieldGroup => {
                if let Some(ref s) = self.settings_state {
                    if s.selected_is_column() {
                        if let Some(idx) = s.selected_column_index() {
                            let current = s.columns.get(idx)
                                .map(|(f, h, _)| if f == h { f.clone() } else { format!("{}|{}", f, h) })
                                .unwrap_or_default();
                            self.field_editor_state =
                                Some(FieldEditorState::new("field or field|header", &current));
                            self.pending_action =
                                Some(PendingAction::RenameColumn { index: idx });
                            self.mode = InputMode::Editing;
                        }
                    } else if let Some(idx) = s.selected_field_group_index() {
                        let name = s.field_groups.get(idx)
                            .map(|(n, _)| n.clone()).unwrap_or_default();
                        self.field_editor_state =
                            Some(FieldEditorState::new("Group name", &name));
                        self.pending_action =
                            Some(PendingAction::RenameFieldGroup { index: idx });
                        self.mode = InputMode::Editing;
                    }
                }
            }
            Action::SettingsExport => {
                self.field_editor_state =
                    Some(FieldEditorState::for_path("Export path", "bibtui.yaml"));
                self.path_completions.clear();
                self.pending_action = Some(PendingAction::ExportSettings);
                self.mode = InputMode::Editing;
            }
            Action::SettingsImport => {
                self.field_editor_state =
                    Some(FieldEditorState::for_path("Import path", ""));
                self.path_completions.clear();
                self.pending_action = Some(PendingAction::ImportSettings);
                self.mode = InputMode::Editing;
            }
            _ => {}
        }
    }
}

/// Return a short label describing which save action is responsible for
/// the change in `field`.  Used in the Validate results popup.
fn action_label_for_field(field: &str, cfg: &crate::config::schema::SaveConfig) -> &'static str {
    match field {
        "url" if cfg.save_action_cleanup_url => "cleanup_url",
        "date" if cfg.save_action_normalize_date => "normalize_date",
        "month" if cfg.save_action_normalize_month => "normalize_month",
        "pages" if cfg.save_action_normalize_page_numbers => "normalize_pages",
        "isbn" if cfg.save_action_normalize_isbn => "normalize_isbn",
        "author" | "editor" | "editora" | "editorb" | "editorc"
        | "bookauthor" | "translator"
            if cfg.save_action_normalize_names_of_persons =>
        {
            "normalize_names"
        }
        "journal_full" | "journal_abbrev" if cfg.save_action_abbreviate_journal => {
            "abbreviate_journal"
        }
        "journal" if cfg.save_action_abbreviate_journal => "abbreviate_journal",
        _ => {
            // For text fields the first applicable action wins (same order as apply_save_actions)
            if cfg.save_action_unicode_to_latex {
                "unicode→latex"
            } else if cfg.save_action_escape_underscores {
                "esc_underscores"
            } else if cfg.save_action_escape_ampersands {
                "esc_ampersands"
            } else if cfg.save_action_latex_cleanup {
                "latex_cleanup"
            } else if cfg.save_action_ordinals_to_superscript {
                "ordinals"
            } else {
                "save_action"
            }
        }
    }
}

fn sort_entries(entries: &IndexMap<String, Entry>, config: &Config) -> Vec<String> {
    let keys: Vec<String> = entries.keys().cloned().collect();

    let field = &config.display.default_sort.field;
    if field == "none" {
        return keys;
    }

    let ascending = config.display.default_sort.ascending;
    let mut keys = keys;
    keys.sort_by(|a, b| {
        let ea = entries.get(a);
        let eb = entries.get(b);

        let va = ea.map(|e| get_sort_value(e, field)).unwrap_or_default();
        let vb = eb.map(|e| get_sort_value(e, field)).unwrap_or_default();

        if ascending {
            va.cmp(&vb)
        } else {
            vb.cmp(&va)
        }
    });

    keys
}

fn get_sort_value(entry: &Entry, field: &str) -> String {
    match field {
        "citation_key" | "key" | "citekey" => entry.citation_key.clone(),
        "entrytype" | "type" => entry.entry_type.display_name().to_string(),
        _ => entry.fields.get(field).cloned().unwrap_or_default(),
    }
}

fn find_group_node<'a>(node: &'a GroupNode, name: &str) -> Option<&'a GroupNode> {
    if node.group.name == name {
        return Some(node);
    }
    for child in &node.children {
        if let Some(found) = find_group_node(child, name) {
            return Some(found);
        }
    }
    None
}

fn find_group_node_mut<'a>(
    node: &'a mut GroupNode,
    path: &[usize],
) -> Option<&'a mut GroupNode> {
    if path.is_empty() {
        return Some(node);
    }
    let idx = path[0];
    node.children
        .get_mut(idx)
        .and_then(|child| find_group_node_mut(child, &path[1..]))
}

fn collect_group_names(node: &GroupNode, names: &mut Vec<String>) {
    if !matches!(node.group.group_type, GroupType::AllEntries) {
        names.push(node.group.name.clone());
    }
    for child in &node.children {
        collect_group_names(child, names);
    }
}

/// Return sorted filesystem completions for `prefix`.
///
/// The prefix is split into a directory part and a name stem.  All entries
/// in that directory whose names start with the stem are returned.
/// Directory entries are returned with a trailing `/`.
/// Expand a leading `~` to the user's home directory (Unix/macOS).
/// Returns the input unchanged if `~` cannot be resolved or is not present.
/// Parse `"field"` or `"field|header"` into `(field, header)`.
/// When no `|` separator is present, the header defaults to the field name.
fn parse_field_header(s: &str) -> (String, String) {
    if let Some(pos) = s.find('|') {
        let field = s[..pos].trim().to_string();
        let header = s[pos + 1..].trim().to_string();
        let header = if header.is_empty() { field.clone() } else { header };
        (field, header)
    } else {
        let field = s.trim().to_string();
        (field.clone(), field)
    }
}

fn expand_tilde(s: &str) -> String {
    if s == "~" || s.starts_with("~/") {
        if let Some(home) = std::env::var_os("HOME") {
            let home = home.to_string_lossy();
            return format!("{}{}", home, &s[1..]);
        }
    }
    s.to_string()
}

/// Contract an absolute path back to a `~`-prefixed form when the path falls
/// under the user's home directory.  Returns the input unchanged otherwise.
fn contract_tilde(s: &str) -> String {
    if let Some(home) = std::env::var_os("HOME") {
        let home = home.to_string_lossy();
        let home_slash = format!("{}/", home);
        if s == home.as_ref() {
            return "~".to_string();
        }
        if let Some(rest) = s.strip_prefix(home_slash.as_str()) {
            return format!("~/{}", rest);
        }
    }
    s.to_string()
}

/// All BibTeX field names the user might want to type (for new-field name completion).
fn field_name_candidates(database: &Database) -> Vec<String> {
    let mut names: std::collections::BTreeSet<String> = [
        "abstract", "address", "annote", "author", "booktitle", "chapter",
        "crossref", "doi", "edition", "editor", "howpublished", "institution",
        "isbn", "issn", "journal", "keywords", "language", "lccn", "month",
        "note", "number", "organization", "pages", "publisher", "school",
        "series", "title", "type", "url", "volume", "year",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    for entry in database.entries.values() {
        for key in entry.fields.keys() {
            names.insert(key.clone());
        }
    }
    names.into_iter().collect()
}

/// Values used for `field` across the database, sorted by frequency then
/// alphabetically.  Skips `current_entry_key` so the entry being edited
/// doesn't seed its own suggestions.  Returns an empty vec for fields where
/// completion would be unhelpful (identifiers, page ranges, etc.).
fn field_value_candidates(
    database: &Database,
    field: &str,
    current_entry_key: Option<&str>,
) -> Vec<String> {
    // These fields hold unique identifiers or numeric ranges — skip them.
    if matches!(
        field,
        "doi" | "eprint" | "isbn" | "issn" | "lccn" | "pages" | "url"
            | "volume" | "number"
    ) {
        return Vec::new();
    }
    let mut freq: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for (key, entry) in &database.entries {
        if current_entry_key.map(|k| k == key.as_str()).unwrap_or(false) {
            continue;
        }
        if let Some(v) = entry.fields.get(field) {
            let v = v.trim().to_string();
            if !v.is_empty() {
                *freq.entry(v).or_insert(0) += 1;
            }
        }
    }
    let mut candidates: Vec<String> = freq.keys().cloned().collect();
    candidates.sort_by(|a, b| freq[b].cmp(&freq[a]).then(a.cmp(b)));
    candidates.truncate(50);
    candidates
}

/// All field names that are valid `:sort` targets.
/// Includes the standard virtual fields plus every field key present in the database.
fn sort_field_candidates(database: &Database) -> Vec<String> {
    let mut fields: std::collections::BTreeSet<String> = [
        "author", "citation_key", "entrytype", "journal", "title", "year",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    for entry in database.entries.values() {
        for key in entry.fields.keys() {
            fields.insert(key.clone());
        }
    }
    fields.into_iter().collect()
}

fn path_completions(prefix: &str) -> Vec<String> {
    use std::path::Path;

    let tilde = prefix.starts_with('~');
    // Work with the expanded form for all filesystem operations.
    let expanded = expand_tilde(prefix);
    let expanded = expanded.as_str();

    let path = Path::new(expanded);
    let (dir, stem) = if expanded.ends_with('/') || expanded.ends_with(std::path::MAIN_SEPARATOR) {
        (path, "")
    } else {
        let stem = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
        let parent = path.parent().unwrap_or(Path::new("."));
        let parent = if parent == Path::new("") { Path::new(".") } else { parent };
        (parent, stem)
    };

    let mut matches = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if !name_str.starts_with(stem) {
                continue;
            }
            let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
            let candidate = if dir == std::path::Path::new(".") && !expanded.contains('/') {
                if is_dir {
                    format!("{}/", name_str)
                } else {
                    name_str.to_string()
                }
            } else {
                let base = dir.display().to_string();
                let sep = if base.ends_with('/') { "" } else { "/" };
                if is_dir {
                    format!("{}{}{}/", base, sep, name_str)
                } else {
                    format!("{}{}{}", base, sep, name_str)
                }
            };
            // Re-apply `~` contraction so the editor shows the tilde form.
            let candidate = if tilde { contract_tilde(&candidate) } else { candidate };
            matches.push(candidate);
        }
    }
    matches.sort();
    matches
}

/// Return the longest common byte prefix shared by all strings in `items`.
fn longest_common_prefix(items: &[String]) -> String {
    if items.is_empty() {
        return String::new();
    }
    let first = items[0].as_bytes();
    let mut len = first.len();
    for s in &items[1..] {
        let s = s.as_bytes();
        len = len.min(s.len());
        for i in 0..len {
            if first[i] != s[i] {
                len = i;
                break;
            }
        }
    }
    // Walk the first string's chars to find the largest valid UTF-8 boundary <= len.
    let mut boundary = 0;
    for (i, c) in items[0].char_indices() {
        if i >= len {
            break;
        }
        boundary = i + c.len_utf8();
    }
    items[0][..boundary].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;
    use crate::config::defaults::default_config;
    use crate::tui::keybindings::InputMode;

    /// Two-entry bib used by most tests. Sorted by citation_key: Doe2021, Smith2020.
    const TEST_BIB: &str = r#"@Article{Smith2020,
  author  = {Smith, John},
  title   = {My Paper},
  year    = {2020},
  journal = {Nature},
}

@Book{Doe2021,
  author    = {Doe, Jane},
  title     = {Rust Programming},
  year      = {2021},
  publisher = {ACM Press},
}
"#;

    /// Build an App from the TEST_BIB string. Returns (App, NamedTempFile);
    /// the caller must keep the NamedTempFile alive to prevent deletion.
    fn make_app() -> (App, NamedTempFile) {
        let mut tmp = NamedTempFile::new().unwrap();
        write!(tmp, "{}", TEST_BIB).unwrap();
        tmp.flush().unwrap();
        let path = tmp.path().to_path_buf();
        let app = App::new(path, default_config()).unwrap();
        (app, tmp)
    }

    // ── Sanity ──────────────────────────────────────────────────────────────

    #[test]
    fn test_app_loads_entries() {
        let (app, _tmp) = make_app();
        assert_eq!(app.database.entries.len(), 2);
    }

    #[test]
    fn test_initial_mode_is_normal() {
        let (app, _tmp) = make_app();
        assert_eq!(app.mode, InputMode::Normal);
    }

    // ── Navigation ──────────────────────────────────────────────────────────

    #[test]
    fn test_move_down() {
        let (mut app, _tmp) = make_app();
        assert_eq!(app.entry_list_state.selected(), 0);
        app.handle_action(Action::MoveDown);
        assert_eq!(app.entry_list_state.selected(), 1);
    }

    #[test]
    fn test_move_down_clamps_at_bottom() {
        let (mut app, _tmp) = make_app();
        app.handle_action(Action::MoveToBottom);
        let bottom = app.entry_list_state.selected();
        app.handle_action(Action::MoveDown);
        assert_eq!(app.entry_list_state.selected(), bottom);
    }

    #[test]
    fn test_move_up_clamps_at_top() {
        let (mut app, _tmp) = make_app();
        app.handle_action(Action::MoveUp);
        assert_eq!(app.entry_list_state.selected(), 0);
    }

    #[test]
    fn test_move_to_top() {
        let (mut app, _tmp) = make_app();
        app.handle_action(Action::MoveDown);
        app.handle_action(Action::MoveToTop);
        assert_eq!(app.entry_list_state.selected(), 0);
    }

    #[test]
    fn test_move_to_bottom() {
        let (mut app, _tmp) = make_app();
        app.handle_action(Action::MoveToBottom);
        assert_eq!(app.entry_list_state.selected(), 1); // 2 entries, index 1
    }

    #[test]
    fn test_page_down_clamps() {
        let (mut app, _tmp) = make_app();
        app.handle_action(Action::PageDown);
        assert_eq!(app.entry_list_state.selected(), 1); // only 2 entries
    }

    #[test]
    fn test_page_up_from_top_stays_at_zero() {
        let (mut app, _tmp) = make_app();
        app.handle_action(Action::PageUp);
        assert_eq!(app.entry_list_state.selected(), 0);
    }

    // ── Focus ────────────────────────────────────────────────────────────────

    #[test]
    fn test_focus_groups() {
        let (mut app, _tmp) = make_app();
        app.handle_action(Action::FocusGroups);
        assert_eq!(app.focus, Focus::Groups);
    }

    #[test]
    fn test_focus_list() {
        let (mut app, _tmp) = make_app();
        app.handle_action(Action::FocusGroups);
        app.handle_action(Action::FocusList);
        assert_eq!(app.focus, Focus::List);
    }

    #[test]
    fn test_toggle_groups() {
        let (mut app, _tmp) = make_app();
        let initial = app.show_groups;
        app.handle_action(Action::ToggleGroups);
        assert_eq!(app.show_groups, !initial);
        app.handle_action(Action::ToggleGroups);
        assert_eq!(app.show_groups, initial);
    }

    // ── Mode transitions ─────────────────────────────────────────────────────

    #[test]
    fn test_enter_exit_search() {
        let (mut app, _tmp) = make_app();
        app.handle_action(Action::EnterSearch);
        assert_eq!(app.mode, InputMode::Search);
        app.handle_action(Action::ExitSearch);
        assert_eq!(app.mode, InputMode::Normal);
        assert!(app.filtered_indices.is_none());
    }

    #[test]
    fn test_confirm_search_stays_normal() {
        let (mut app, _tmp) = make_app();
        app.handle_action(Action::EnterSearch);
        app.handle_action(Action::ConfirmSearch);
        assert_eq!(app.mode, InputMode::Normal);
    }

    #[test]
    fn test_enter_exit_command() {
        let (mut app, _tmp) = make_app();
        app.handle_action(Action::EnterCommand);
        assert_eq!(app.mode, InputMode::Command);
        app.handle_action(Action::ExitCommand);
        assert_eq!(app.mode, InputMode::Normal);
    }

    #[test]
    fn test_enter_exit_settings() {
        let (mut app, _tmp) = make_app();
        app.handle_action(Action::EnterSettings);
        assert_eq!(app.mode, InputMode::Settings);
        assert!(app.settings_state.is_some());
        app.handle_action(Action::ExitSettings);
        assert_eq!(app.mode, InputMode::Normal);
        assert!(app.settings_state.is_none());
    }

    #[test]
    fn test_open_close_detail() {
        let (mut app, _tmp) = make_app();
        app.handle_action(Action::OpenDetail);
        assert_eq!(app.mode, InputMode::Detail);
        assert!(app.detail_state.is_some());
        app.handle_action(Action::CloseDetail);
        assert_eq!(app.mode, InputMode::Normal);
        assert!(app.detail_state.is_none());
    }

    // ── Toggles ──────────────────────────────────────────────────────────────

    #[test]
    fn test_toggle_braces() {
        let (mut app, _tmp) = make_app();
        let initial = app.show_braces;
        app.handle_action(Action::ToggleBraces);
        assert_eq!(app.show_braces, !initial);
        assert!(app.status_message.is_some());
    }

    #[test]
    fn test_toggle_latex() {
        let (mut app, _tmp) = make_app();
        let initial = app.render_latex;
        app.handle_action(Action::ToggleLatex);
        assert_eq!(app.render_latex, !initial);
        assert!(app.status_message.is_some());
    }

    // ── Quit ─────────────────────────────────────────────────────────────────

    #[test]
    fn test_quit_when_clean() {
        let (mut app, _tmp) = make_app();
        app.command_palette_state.input = "q".to_string();
        app.handle_action(Action::ExecuteCommand);
        assert!(app.should_quit);
    }

    #[test]
    fn test_quit_when_dirty_shows_message() {
        let (mut app, _tmp) = make_app();
        app.dirty = true;
        app.command_palette_state.input = "q".to_string();
        app.handle_action(Action::ExecuteCommand);
        assert!(!app.should_quit);
        assert!(app.status_message.is_some());
    }

    // ── Search ────────────────────────────────────────────────────────────────

    #[test]
    fn test_search_char_updates_query() {
        let (mut app, _tmp) = make_app();
        app.handle_action(Action::EnterSearch);
        app.handle_action(Action::SearchChar('s'));
        app.handle_action(Action::SearchChar('m'));
        assert_eq!(app.search_bar_state.query, "sm");
    }

    #[test]
    fn test_search_backspace() {
        let (mut app, _tmp) = make_app();
        app.handle_action(Action::EnterSearch);
        app.handle_action(Action::SearchChar('s'));
        app.handle_action(Action::SearchChar('m'));
        app.handle_action(Action::SearchBackspace);
        assert_eq!(app.search_bar_state.query, "s");
    }

    #[test]
    fn test_search_filters_entries() {
        let (mut app, _tmp) = make_app();
        app.handle_action(Action::EnterSearch);
        app.handle_action(Action::SearchChar('S')); // "Smith2020"
        app.handle_action(Action::SearchChar('m'));
        app.handle_action(Action::SearchChar('i'));
        app.handle_action(Action::SearchChar('t'));
        app.handle_action(Action::SearchChar('h'));
        // filtered_indices should now have 1 match
        assert!(app.filtered_indices.is_some());
        assert_eq!(app.filtered_indices.as_ref().unwrap().len(), 1);
    }

    // ── Command palette ───────────────────────────────────────────────────────

    #[test]
    fn test_command_char_updates_input() {
        let (mut app, _tmp) = make_app();
        app.handle_action(Action::EnterCommand);
        app.handle_action(Action::CommandChar('w'));
        assert_eq!(app.command_palette_state.input, "w");
    }

    #[test]
    fn test_command_backspace() {
        let (mut app, _tmp) = make_app();
        app.handle_action(Action::EnterCommand);
        app.handle_action(Action::CommandChar('w'));
        app.handle_action(Action::CommandBackspace);
        assert_eq!(app.command_palette_state.input, "");
        assert_eq!(app.mode, InputMode::Normal);
    }

    #[test]
    fn test_execute_command_sort() {
        let (mut app, _tmp) = make_app();
        app.handle_action(Action::EnterCommand);
        for c in "sort year".chars() {
            app.handle_action(Action::CommandChar(c));
        }
        app.handle_action(Action::ExecuteCommand);
        assert_eq!(app.config.display.default_sort.field, "year");
        assert!(app.status_message.is_some());
    }

    #[test]
    fn test_execute_command_sort_toggle_direction() {
        let (mut app, _tmp) = make_app();
        app.handle_action(Action::EnterCommand);
        for c in "sort year".chars() { app.handle_action(Action::CommandChar(c)); }
        app.handle_action(Action::ExecuteCommand);
        let asc = app.config.display.default_sort.ascending;
        // Same field again: toggle direction
        app.handle_action(Action::EnterCommand);
        for c in "sort year".chars() { app.handle_action(Action::CommandChar(c)); }
        app.handle_action(Action::ExecuteCommand);
        assert_eq!(app.config.display.default_sort.ascending, !asc);
    }

    #[test]
    fn test_execute_command_unknown() {
        let (mut app, _tmp) = make_app();
        app.handle_action(Action::EnterCommand);
        for c in "foobar".chars() { app.handle_action(Action::CommandChar(c)); }
        app.handle_action(Action::ExecuteCommand);
        let msg = app.status_message.as_deref().unwrap_or("");
        assert!(msg.contains("Unknown command"));
    }

    #[test]
    fn test_execute_command_quit_with_dirty() {
        let (mut app, _tmp) = make_app();
        app.dirty = true;
        app.handle_action(Action::EnterCommand);
        for c in "q".chars() { app.handle_action(Action::CommandChar(c)); }
        app.handle_action(Action::ExecuteCommand);
        assert!(!app.should_quit);
        assert!(app.status_message.is_some());
    }

    #[test]
    fn test_execute_command_force_quit() {
        let (mut app, _tmp) = make_app();
        app.dirty = true;
        app.handle_action(Action::EnterCommand);
        for c in "q!".chars() { app.handle_action(Action::CommandChar(c)); }
        app.handle_action(Action::ExecuteCommand);
        assert!(app.should_quit);
    }

    // ── Field editor tab completion ───────────────────────────────────────────

    #[test]
    fn test_field_value_completion_on_open() {
        // Opening an existing field editor seeds completions from the database.
        let (mut app, _tmp) = make_app();
        app.handle_action(Action::OpenDetail); // enter detail view
        app.handle_action(Action::EditField);
        let editor = app.field_editor_state.as_ref().unwrap();
        // Completions may be empty if the field has no other values — just
        // verify the state is initialised (no panic and completions is a Vec).
        let _ = &editor.completions;
    }

    #[test]
    fn test_field_value_completion_filters_by_prefix() {
        let (mut app, _tmp) = make_app();
        // Manually open editor for "author" with a known prefix.
        app.field_editor_state = Some(FieldEditorState::new("author", "S"));
        app.update_field_completions();
        let editor = app.field_editor_state.as_ref().unwrap();
        // Every completion must start with "s" (case-insensitive).
        for c in &editor.completions {
            assert!(c.to_lowercase().starts_with('s'), "unexpected: {}", c);
        }
    }

    #[test]
    fn test_field_tab_complete_fills_single_match() {
        let (mut app, _tmp) = make_app();
        // Use a prefix that uniquely matches one author in the test bib file.
        // We inject a known completion directly to avoid bib-content dependency.
        app.field_editor_state = Some(FieldEditorState::new("author", "Smi"));
        let e = app.field_editor_state.as_mut().unwrap();
        e.completions = vec!["Smith, John".to_string()];
        app.handle_action(Action::EditTabComplete);
        let editor = app.field_editor_state.as_ref().unwrap();
        assert_eq!(editor.value, "Smith, John");
        assert_eq!(editor.cursor, "Smith, John".len());
    }

    #[test]
    fn test_field_tab_complete_cycles() {
        let (mut app, _tmp) = make_app();
        app.field_editor_state = Some(FieldEditorState::new("author", "S"));
        let e = app.field_editor_state.as_mut().unwrap();
        e.completions = vec!["Smith, John".to_string(), "Stone, Alice".to_string()];
        // First Tab: common prefix "S" → already there, so fill first match.
        app.handle_action(Action::EditTabComplete);
        assert_eq!(app.field_editor_state.as_ref().unwrap().value, "Smith, John");
        // Second Tab: cycle to next.
        app.handle_action(Action::EditTabComplete);
        assert_eq!(app.field_editor_state.as_ref().unwrap().value, "Stone, Alice");
        // Third Tab: wrap back.
        app.handle_action(Action::EditTabComplete);
        assert_eq!(app.field_editor_state.as_ref().unwrap().value, "Smith, John");
    }

    #[test]
    fn test_field_tab_complete_name_phase() {
        let (mut app, _tmp) = make_app();
        app.field_editor_state = Some(FieldEditorState::new_field());
        let e = app.field_editor_state.as_mut().unwrap();
        e.field_name = "auth".to_string();
        e.name_cursor = 4;
        e.completions = vec!["author".to_string()];
        app.handle_action(Action::EditTabComplete);
        let editor = app.field_editor_state.as_ref().unwrap();
        assert_eq!(editor.field_name, "author");
    }

    #[test]
    fn test_ghost_text_shows_suffix() {
        let mut e = FieldEditorState::new("author", "Smi");
        e.cursor = e.value.len(); // ghost text only shows when cursor is at end
        e.completions = vec!["Smith, John".to_string()];
        assert_eq!(e.ghost_text(), "th, John");
    }

    #[test]
    fn test_ghost_text_empty_when_cursor_not_at_end() {
        let mut e = FieldEditorState::new("author", "Smith");
        e.cursor = 2; // cursor in the middle
        e.completions = vec!["Smith, John".to_string()];
        assert_eq!(e.ghost_text(), "");
    }

    #[test]
    fn test_ghost_text_empty_for_exact_match() {
        let mut e = FieldEditorState::new("author", "Smith, John");
        e.completions = vec!["Smith, John".to_string()];
        assert_eq!(e.ghost_text(), "");
    }

    #[test]
    fn test_field_name_candidates_contains_standard_fields() {
        let (app, _tmp) = make_app();
        let names = field_name_candidates(&app.database);
        assert!(names.contains(&"author".to_string()));
        assert!(names.contains(&"title".to_string()));
        assert!(names.contains(&"journal".to_string()));
    }

    #[test]
    fn test_field_value_candidates_skips_doi() {
        let (app, _tmp) = make_app();
        let candidates = field_value_candidates(&app.database, "doi", None);
        assert!(candidates.is_empty());
    }

    // ── Sort tab completion ───────────────────────────────────────────────────

    #[test]
    fn test_sort_tab_complete_single_match() {
        let (mut app, _tmp) = make_app();
        app.handle_action(Action::EnterCommand);
        for c in "sort yea".chars() { app.handle_action(Action::CommandChar(c)); }
        // completions should contain "year"
        assert!(app.command_palette_state.completions.contains(&"year".to_string()));
        app.handle_action(Action::CommandTabComplete);
        assert_eq!(app.command_palette_state.input, "sort year");
    }

    #[test]
    fn test_sort_tab_complete_ghost_text() {
        let (mut app, _tmp) = make_app();
        app.handle_action(Action::EnterCommand);
        for c in "sort yea".chars() { app.handle_action(Action::CommandChar(c)); }
        assert_eq!(app.command_palette_state.ghost_text(), "r");
    }

    #[test]
    fn test_sort_tab_complete_cycles() {
        let (mut app, _tmp) = make_app();
        app.handle_action(Action::EnterCommand);
        for c in "sort yea".chars() { app.handle_action(Action::CommandChar(c)); }
        app.handle_action(Action::CommandTabComplete); // fills "year"
        // Cycling: only one match for "year", so it wraps back
        app.handle_action(Action::CommandTabComplete);
        assert_eq!(app.command_palette_state.input, "sort year");
    }

    #[test]
    fn test_sort_tab_no_completions_outside_sort() {
        let (mut app, _tmp) = make_app();
        app.handle_action(Action::EnterCommand);
        for c in "write".chars() { app.handle_action(Action::CommandChar(c)); }
        assert!(app.command_palette_state.completions.is_empty());
        // Tab should be a no-op
        app.handle_action(Action::CommandTabComplete);
        assert_eq!(app.command_palette_state.input, "write");
    }

    #[test]
    fn test_sort_completions_cleared_on_clear() {
        let (mut app, _tmp) = make_app();
        app.handle_action(Action::EnterCommand);
        for c in "sort yea".chars() { app.handle_action(Action::CommandChar(c)); }
        assert!(!app.command_palette_state.completions.is_empty());
        app.handle_action(Action::EnterCommand); // clears state
        assert!(app.command_palette_state.completions.is_empty());
    }

    // ── Entry operations ──────────────────────────────────────────────────────

    #[test]
    fn test_add_entry_opens_type_picker() {
        let (mut app, _tmp) = make_app();
        app.handle_action(Action::AddEntry);
        assert!(app.dialog_state.is_some());
        assert_eq!(app.mode, InputMode::Dialog);
    }

    #[test]
    fn test_delete_entry_opens_confirm_dialog() {
        let (mut app, _tmp) = make_app();
        app.handle_action(Action::DeleteEntry);
        assert!(app.dialog_state.is_some());
        assert_eq!(app.mode, InputMode::Dialog);
        // No local files → simple Confirm dialog
        assert!(matches!(
            app.dialog_state.as_ref().unwrap().kind,
            crate::tui::components::dialog::DialogKind::Confirm { .. }
        ));
    }

    #[test]
    fn test_delete_entry_with_one_local_file_shows_type_picker() {
        use tempfile::NamedTempFile;
        let (mut app, _tmp) = make_app();

        // Attach a real temporary file to the first entry.
        let pdf = NamedTempFile::new().unwrap();
        let pdf_path = pdf.path().to_path_buf();
        let fname = pdf_path.file_name().unwrap().to_str().unwrap().to_string();

        let key = app.sorted_keys.first().cloned().unwrap();
        app.database
            .entries
            .get_mut(&key)
            .unwrap()
            .fields
            .insert("file".to_string(), format!(":{}:PDF", pdf_path.display()));

        app.handle_action(Action::DeleteEntry);
        assert_eq!(app.mode, InputMode::Dialog);
        // Should be a TypePicker (not Confirm) because there is one local file.
        let dialog = app.dialog_state.as_ref().unwrap();
        assert!(matches!(
            dialog.kind,
            crate::tui::components::dialog::DialogKind::TypePicker { .. }
        ));
        // "Delete entry + {fname}" should be the first option.
        if let crate::tui::components::dialog::DialogKind::TypePicker { options, .. } = &dialog.kind {
            assert!(options[0].contains(&fname));
            assert_eq!(options.len(), 3); // entry+file, entry only, cancel
        }
    }

    #[test]
    fn test_delete_entry_with_multiple_local_files_shows_file_delete_select() {
        use tempfile::NamedTempFile;
        let (mut app, _tmp) = make_app();

        let pdf1 = NamedTempFile::new().unwrap();
        let pdf2 = NamedTempFile::new().unwrap();
        let p1 = pdf1.path();
        let p2 = pdf2.path();
        let file_val = format!(":{}:PDF;:{}:PDF", p1.display(), p2.display());

        let key = app.sorted_keys.first().cloned().unwrap();
        app.database
            .entries
            .get_mut(&key)
            .unwrap()
            .fields
            .insert("file".to_string(), file_val);

        app.handle_action(Action::DeleteEntry);
        assert_eq!(app.mode, InputMode::Dialog);
        let dialog = app.dialog_state.as_ref().unwrap();
        assert!(matches!(
            dialog.kind,
            crate::tui::components::dialog::DialogKind::FileDeleteSelect { .. }
        ));
        assert_eq!(dialog.option_count(), 2);
    }

    #[test]
    fn test_delete_entry_with_file_option0_deletes_both() {
        use tempfile::NamedTempFile;
        let (mut app, _tmp) = make_app();

        let pdf = NamedTempFile::new().unwrap();
        let pdf_path = pdf.path().to_path_buf();

        let key = app.sorted_keys.first().cloned().unwrap();
        app.database
            .entries
            .get_mut(&key)
            .unwrap()
            .fields
            .insert("file".to_string(), format!(":{}:PDF", pdf_path.display()));

        // Trigger delete → TypePicker
        app.handle_action(Action::DeleteEntry);
        // Select option 0 (delete entry + file) and confirm
        app.dialog_state.as_mut().unwrap().select(0);
        app.handle_action(Action::DialogConfirm);

        assert!(!app.database.entries.contains_key(&key));
        assert!(!pdf_path.exists(), "file should have been deleted");
    }

    #[test]
    fn test_delete_entry_with_file_option1_keeps_file() {
        use tempfile::NamedTempFile;
        let (mut app, _tmp) = make_app();

        let pdf = NamedTempFile::new().unwrap();
        let pdf_path = pdf.path().to_path_buf();

        let key = app.sorted_keys.first().cloned().unwrap();
        app.database
            .entries
            .get_mut(&key)
            .unwrap()
            .fields
            .insert("file".to_string(), format!(":{}:PDF", pdf_path.display()));

        app.handle_action(Action::DeleteEntry);
        app.dialog_state.as_mut().unwrap().select(1); // "Delete entry only"
        app.handle_action(Action::DialogConfirm);

        assert!(!app.database.entries.contains_key(&key));
        assert!(pdf_path.exists(), "file should have been kept");
    }

    #[test]
    fn test_delete_entry_with_file_option2_cancels() {
        use tempfile::NamedTempFile;
        let (mut app, _tmp) = make_app();

        let pdf = NamedTempFile::new().unwrap();
        let pdf_path = pdf.path().to_path_buf();
        let key = app.sorted_keys.first().cloned().unwrap();
        app.database
            .entries
            .get_mut(&key)
            .unwrap()
            .fields
            .insert("file".to_string(), format!(":{}:PDF", pdf_path.display()));

        app.handle_action(Action::DeleteEntry);
        app.dialog_state.as_mut().unwrap().select(2); // "Cancel"
        app.handle_action(Action::DialogConfirm);

        assert!(app.database.entries.contains_key(&key), "entry should survive cancel");
        assert!(pdf_path.exists(), "file should survive cancel");
    }

    #[test]
    fn test_delete_entry_multi_file_select_partial() {
        use tempfile::NamedTempFile;
        let (mut app, _tmp) = make_app();

        let pdf1 = NamedTempFile::new().unwrap();
        let pdf2 = NamedTempFile::new().unwrap();
        let p1 = pdf1.path().to_path_buf();
        let p2 = pdf2.path().to_path_buf();
        let file_val = format!(":{}:PDF;:{}:PDF", p1.display(), p2.display());

        let key = app.sorted_keys.first().cloned().unwrap();
        app.database
            .entries
            .get_mut(&key)
            .unwrap()
            .fields
            .insert("file".to_string(), file_val);

        app.handle_action(Action::DeleteEntry);
        // Uncheck the second file (keep it)
        app.dialog_state.as_mut().unwrap().select(1);
        app.handle_action(Action::DialogToggle); // uncheck second file
        app.handle_action(Action::DialogConfirm);

        assert!(!app.database.entries.contains_key(&key));
        assert!(!p1.exists(), "first file should be deleted");
        assert!(p2.exists(), "second file should be kept");
    }

    #[test]
    fn test_duplicate_entry() {
        let (mut app, _tmp) = make_app();
        let initial_count = app.database.entries.len();
        app.handle_action(Action::DuplicateEntry);
        assert_eq!(app.database.entries.len(), initial_count + 1);
        assert!(app.status_message.as_deref().unwrap().contains("duplicated"));
    }

    #[test]
    fn test_add_entry_of_type() {
        let (mut app, _tmp) = make_app();
        let before = app.database.entries.len();
        app.add_entry_of_type("Article");
        assert_eq!(app.database.entries.len(), before + 1);
        assert_eq!(app.mode, InputMode::Detail);
    }

    #[test]
    fn test_delete_entry() {
        let (mut app, _tmp) = make_app();
        let key = app.sorted_keys[0].clone();
        let before = app.database.entries.len();
        app.delete_entry(&key);
        assert_eq!(app.database.entries.len(), before - 1);
        assert!(!app.database.entries.contains_key(&key));
    }

    // ── Undo ─────────────────────────────────────────────────────────────────

    #[test]
    fn test_undo_empty_stack() {
        let (mut app, _tmp) = make_app();
        app.handle_action(Action::Undo);
        assert_eq!(app.status_message.as_deref(), Some("Nothing to undo"));
    }

    #[test]
    fn test_undo_after_duplicate() {
        let (mut app, _tmp) = make_app();
        let before = app.database.entries.len();
        app.handle_action(Action::DuplicateEntry);
        assert_eq!(app.database.entries.len(), before + 1);
        app.handle_action(Action::Undo);
        assert_eq!(app.database.entries.len(), before);
    }

    #[test]
    fn test_undo_after_delete() {
        let (mut app, _tmp) = make_app();
        let key = app.sorted_keys[0].clone();
        let before = app.database.entries.len();
        app.delete_entry(&key);
        app.undo();
        assert_eq!(app.database.entries.len(), before);
        assert!(app.database.entries.contains_key(&key));
    }

    // ── Dialog ───────────────────────────────────────────────────────────────

    #[test]
    fn test_dialog_cancel_clears_state() {
        let (mut app, _tmp) = make_app();
        app.handle_action(Action::DeleteEntry);
        app.handle_action(Action::DialogCancel);
        assert!(app.dialog_state.is_none());
        assert_eq!(app.mode, InputMode::Normal);
    }

    #[test]
    fn test_dialog_toggle() {
        let (mut app, _tmp) = make_app();
        app.handle_action(Action::AddEntry); // opens type picker
        // DialogToggle should not panic even on a type-picker dialog
        app.handle_action(Action::DialogToggle);
    }

    // ── ShowHelp ─────────────────────────────────────────────────────────────

    #[test]
    fn test_show_help_opens_modal() {
        let (mut app, _tmp) = make_app();
        app.handle_action(Action::ShowHelp);
        assert!(app.help_state.is_some());
        assert_eq!(app.mode, InputMode::Help);
        app.handle_action(Action::CloseHelp);
        assert!(app.help_state.is_none());
        assert_eq!(app.mode, InputMode::Normal);
    }

    // ── Validate ─────────────────────────────────────────────────────────────

    #[test]
    fn test_validate_opens_results_panel() {
        let (mut app, _tmp) = make_app();
        app.handle_action(Action::Validate);
        assert!(app.validate_results_state.is_some());
        assert_eq!(app.mode, InputMode::ValidateResults);
    }

    #[test]
    fn test_close_validate_results_returns_to_normal() {
        let (mut app, _tmp) = make_app();
        app.handle_action(Action::Validate);
        app.handle_action(Action::CloseValidateResults);
        assert!(app.validate_results_state.is_none());
        assert_eq!(app.mode, InputMode::Normal);
    }

    #[test]
    fn test_validate_move_down_scrolls_results() {
        let (mut app, _tmp) = make_app();
        app.handle_action(Action::Validate);
        // Build a large enough violation list that scroll > 0 is possible
        if let Some(ref mut vrs) = app.validate_results_state {
            // Manually push enough violations so total_lines > inner_height fallback (24)
            for i in 0..10 {
                vrs.violations.push(
                    crate::tui::components::validate_results::Violation {
                        entry_key: format!("k{}", i),
                        field: "title".to_string(),
                        old_value: "old".to_string(),
                        new_value: "new".to_string(),
                        action_name: "test",
                    },
                );
            }
        }
        // In ValidateResults mode, MoveDown scrolls the panel
        app.handle_action(Action::MoveDown);
        assert_eq!(app.validate_results_state.as_ref().unwrap().scroll, 1);
    }

    #[test]
    fn test_validate_move_up_scrolls_results() {
        let (mut app, _tmp) = make_app();
        app.handle_action(Action::Validate);
        if let Some(ref mut vrs) = app.validate_results_state {
            for i in 0..10 {
                vrs.violations.push(
                    crate::tui::components::validate_results::Violation {
                        entry_key: format!("k{}", i),
                        field: "title".to_string(),
                        old_value: "old".to_string(),
                        new_value: "new".to_string(),
                        action_name: "test",
                    },
                );
            }
        }
        app.handle_action(Action::MoveDown);
        assert_eq!(app.validate_results_state.as_ref().unwrap().scroll, 1);
        app.handle_action(Action::MoveUp);
        assert_eq!(app.validate_results_state.as_ref().unwrap().scroll, 0);
    }

    // ── Settings extended navigation ──────────────────────────────────────────

    #[test]
    fn test_settings_move_to_top() {
        let (mut app, _tmp) = make_app();
        app.handle_action(Action::EnterSettings);
        // Move down a few rows
        app.handle_action(Action::SettingsMoveDown);
        app.handle_action(Action::SettingsMoveDown);
        let after_down = app.settings_state.as_ref().unwrap().cursor;
        app.handle_action(Action::SettingsMoveToTop);
        let after_top = app.settings_state.as_ref().unwrap().cursor;
        assert!(after_top < after_down);
    }

    #[test]
    fn test_settings_move_to_bottom() {
        let (mut app, _tmp) = make_app();
        app.handle_action(Action::EnterSettings);
        let start = app.settings_state.as_ref().unwrap().cursor;
        app.handle_action(Action::SettingsMoveToBottom);
        let bottom = app.settings_state.as_ref().unwrap().cursor;
        assert!(bottom > start);
    }

    #[test]
    fn test_settings_page_down() {
        let (mut app, _tmp) = make_app();
        app.handle_action(Action::EnterSettings);
        let start = app.settings_state.as_ref().unwrap().cursor;
        app.handle_action(Action::SettingsPageDown);
        let after = app.settings_state.as_ref().unwrap().cursor;
        assert!(after > start);
    }

    #[test]
    fn test_settings_page_up_at_top_is_noop() {
        let (mut app, _tmp) = make_app();
        app.handle_action(Action::EnterSettings);
        let top = app.settings_state.as_ref().unwrap().cursor;
        app.handle_action(Action::SettingsPageUp);
        assert_eq!(app.settings_state.as_ref().unwrap().cursor, top);
    }

    #[test]
    fn test_settings_cursor_restored_on_reopen() {
        let (mut app, _tmp) = make_app();
        app.handle_action(Action::EnterSettings);
        app.handle_action(Action::SettingsMoveDown);
        app.handle_action(Action::SettingsMoveDown);
        let cursor_before = app.settings_state.as_ref().unwrap().cursor;
        app.handle_action(Action::ExitSettings);
        assert!(app.settings_state.is_none());
        app.handle_action(Action::EnterSettings);
        let cursor_after = app.settings_state.as_ref().unwrap().cursor;
        assert_eq!(cursor_after, cursor_before, "cursor should be restored on reopen");
    }

    // ── Dirty-flag recheck after field edit ──────────────────────────────────

    #[test]
    fn test_dirty_cleared_when_field_reverted_to_original() {
        // Use an entry whose citation key already matches the template output,
        // so that reverting the field also reverts the auto-generated key.
        // Template: Article_[year]_[auth]
        // For year=2020, author=Smith → Article_2020_Smith
        let mut tmp = NamedTempFile::new().unwrap();
        write!(tmp, "@Article{{Article_2020_Smith,\n  author  = {{Smith, John}},\n  title   = {{My Paper}},\n  year    = {{2020}},\n  journal = {{Nature}},\n}}\n").unwrap();
        tmp.flush().unwrap();
        let app_result = App::new(tmp.path().to_path_buf(), default_config());
        let mut app = app_result.unwrap();

        app.handle_action(Action::OpenDetail);
        app.detail_entry_key = Some("Article_2020_Smith".to_string());

        use crate::tui::components::field_editor::FieldEditorState;

        // Modify the year field — key auto-regens to Article_2099_Smith.
        app.field_editor_state = Some(FieldEditorState::new("year", "2099"));
        app.handle_action(Action::ConfirmEdit);
        assert!(app.database.entries.values().any(|e| e.dirty), "should be dirty after change");

        // Revert the year field to its original value — key regens back.
        app.field_editor_state = Some(FieldEditorState::new("year", "2020"));
        app.handle_action(Action::ConfirmEdit);

        // After reverting, the entry should no longer be dirty.
        let reverted_key = app.detail_entry_key.clone().unwrap();
        let entry = app.database.entries.get(&reverted_key).expect("entry must exist");
        assert!(!entry.dirty, "entry should not be dirty after reverting to original value; key={}", reverted_key);
        let _tmp = tmp;
    }

    // ── Auto-regen citekey on field edit ─────────────────────────────────────

    #[test]
    fn test_citekey_auto_updated_on_field_edit() {
        let (mut app, _tmp) = make_app();
        app.handle_action(Action::OpenDetail);
        app.detail_entry_key = Some("Smith2020".to_string());

        use crate::tui::components::field_editor::FieldEditorState;
        app.field_editor_state = Some(FieldEditorState::new("year", "2023"));
        app.handle_action(Action::ConfirmEdit);

        // Key should have been auto-regenerated to reflect the new year.
        // Template: Article_[year]_[auth]  →  Article_2023_Smith
        assert!(
            app.database.entries.contains_key("Article_2023_Smith"),
            "expected auto-regenerated key Article_2023_Smith; keys: {:?}",
            app.database.entries.keys().collect::<Vec<_>>()
        );
        assert!(!app.database.entries.contains_key("Smith2020"), "old key should be gone");
    }

    // ── Close citation preview ────────────────────────────────────────────────

    #[test]
    fn test_close_citation_preview() {
        let (mut app, _tmp) = make_app();
        app.mode = InputMode::CitationPreview;
        app.citation_preview_state = Some(CitationPreviewState {
            citation: "cite".to_string(),
            entry_key: "Smith2020".to_string(),
            style_name: "ieeetran".to_string(),
        });
        app.handle_action(Action::CloseCitationPreview);
        assert!(app.citation_preview_state.is_none());
        assert_eq!(app.mode, InputMode::Normal);
    }

    // ── Field editor ─────────────────────────────────────────────────────────

    #[test]
    fn test_edit_char_updates_editor() {
        let (mut app, _tmp) = make_app();
        app.field_editor_state = Some(FieldEditorState::new("title", "old"));
        app.handle_action(Action::EditChar('X'));
        let val = app.field_editor_state.as_ref().unwrap().value.clone();
        assert!(val.contains('X'));
    }

    #[test]
    fn test_edit_backspace() {
        let (mut app, _tmp) = make_app();
        let mut editor = FieldEditorState::new("title", "abc");
        editor.cursor = editor.value.len(); // position at end for backspace test
        app.field_editor_state = Some(editor);
        app.handle_action(Action::EditBackspace);
        let val = app.field_editor_state.as_ref().unwrap().value.clone();
        assert_eq!(val, "ab");
    }

    #[test]
    fn test_cancel_edit() {
        let (mut app, _tmp) = make_app();
        app.field_editor_state = Some(FieldEditorState::new("title", "abc"));
        app.mode = InputMode::Editing;
        app.handle_action(Action::CancelEdit);
        assert!(app.field_editor_state.is_none());
        assert_eq!(app.mode, InputMode::Normal);
    }

    // ── sort_entries (module-level fn) ────────────────────────────────────────

    #[test]
    fn test_sort_entries_ascending() {
        let (app, _tmp) = make_app();
        // Default config sorts by citation_key ascending → Doe2021, Smith2020
        assert_eq!(app.sorted_keys[0], "Doe2021");
        assert_eq!(app.sorted_keys[1], "Smith2020");
    }

    #[test]
    fn test_sort_entries_descending() {
        let (app, _tmp) = make_app();
        let mut cfg = default_config();
        cfg.display.default_sort.ascending = false;
        let keys = sort_entries(&app.database.entries, &cfg);
        assert_eq!(keys[0], "Smith2020");
        assert_eq!(keys[1], "Doe2021");
    }

    #[test]
    fn test_sort_by_year() {
        let (app, _tmp) = make_app();
        let mut cfg = default_config();
        cfg.display.default_sort.field = "year".to_string();
        let keys = sort_entries(&app.database.entries, &cfg);
        // Smith2020 (2020) before Doe2021 (2021)
        assert_eq!(keys[0], "Smith2020");
        assert_eq!(keys[1], "Doe2021");
    }

    // ── get_sort_value ────────────────────────────────────────────────────────

    #[test]
    fn test_get_sort_value_citation_key() {
        let (app, _tmp) = make_app();
        let entry = app.database.entries.get("Smith2020").unwrap();
        assert_eq!(get_sort_value(entry, "citation_key"), "Smith2020");
        assert_eq!(get_sort_value(entry, "key"), "Smith2020");
        assert_eq!(get_sort_value(entry, "citekey"), "Smith2020");
    }

    #[test]
    fn test_get_sort_value_entrytype() {
        let (app, _tmp) = make_app();
        let entry = app.database.entries.get("Smith2020").unwrap();
        assert_eq!(get_sort_value(entry, "entrytype"), "Article");
        assert_eq!(get_sort_value(entry, "type"), "Article");
    }

    #[test]
    fn test_get_sort_value_field() {
        let (app, _tmp) = make_app();
        let entry = app.database.entries.get("Smith2020").unwrap();
        assert_eq!(get_sort_value(entry, "year"), "2020");
    }

    #[test]
    fn test_get_sort_value_missing_field() {
        let (app, _tmp) = make_app();
        let entry = app.database.entries.get("Smith2020").unwrap();
        assert_eq!(get_sort_value(entry, "nonexistent"), "");
    }

    #[test]
    fn test_sort_none_returns_file_order() {
        // When field is "none", keys come back in IndexMap insertion order
        // (Smith2020 first, Doe2021 second — file order).
        let (app, _tmp) = make_app();
        let mut cfg = default_config();
        cfg.display.default_sort.field = "none".to_string();
        let keys = sort_entries(&app.database.entries, &cfg);
        assert_eq!(keys[0], "Smith2020");
        assert_eq!(keys[1], "Doe2021");
    }

    #[test]
    fn test_sort_command_none_sets_file_order() {
        // `:sort none` must set the sort field to "none" and rebuild sorted_keys
        // in file (insertion) order regardless of any prior sort.
        let (mut app, _tmp) = make_app();
        // First sort by citation_key (default) — Doe2021 first.
        assert_eq!(app.sorted_keys[0], "Doe2021");
        // Now reset to file order via command.
        app.handle_action(Action::EnterCommand);
        for c in "sort none".chars() { app.handle_action(Action::CommandChar(c)); }
        app.handle_action(Action::ExecuteCommand);
        assert_eq!(app.config.display.default_sort.field, "none");
        assert_eq!(app.sorted_keys[0], "Smith2020",
            "sorted_keys[0] should be Smith2020 (file order) after :sort none");
        let msg = app.status_message.as_deref().unwrap_or("");
        assert!(msg.contains("file order"), "status should mention file order");
    }

    #[test]
    fn test_sort_command_none_reruns_active_search() {
        // After a search is confirmed, `:sort none` must re-run the search so
        // filtered_indices stays valid against the new sorted_keys.
        let (mut app, _tmp) = make_app();
        // Search for "Smith" — matches Smith2020 only.
        app.handle_action(Action::EnterSearch);
        for c in "Smith".chars() { app.handle_action(Action::SearchChar(c)); }
        app.handle_action(Action::ConfirmSearch);
        assert!(app.filtered_indices.is_some());
        // Now change sort order.
        app.handle_action(Action::EnterCommand);
        for c in "sort none".chars() { app.handle_action(Action::CommandChar(c)); }
        app.handle_action(Action::ExecuteCommand);
        // filtered_indices should still be Some (search is still active)
        // and its single entry should index Smith2020 in the new sorted_keys.
        let indices = app.filtered_indices.as_ref().expect("filter still active");
        assert_eq!(indices.len(), 1);
        let matched_key = app.sorted_keys.get(indices[0]).expect("valid index");
        assert_eq!(matched_key, "Smith2020");
    }

    #[test]
    fn test_reset_sort_clears_active_search_filter() {
        // ESC (ResetSort) while filtered_indices is set clears the filter
        // instead of touching the sort config.
        let (mut app, _tmp) = make_app();
        app.handle_action(Action::EnterSearch);
        for c in "Smith".chars() { app.handle_action(Action::SearchChar(c)); }
        app.handle_action(Action::ConfirmSearch);
        assert!(app.filtered_indices.is_some(), "filter should be active");
        // ESC in Normal mode.
        app.handle_action(Action::ResetSort);
        assert!(app.filtered_indices.is_none(), "ESC should clear the filter");
        assert!(app.search_bar_state.query.is_empty(), "search query should be cleared");
        let msg = app.status_message.as_deref().unwrap_or("");
        assert!(msg.contains("cleared"), "status should say search was cleared");
    }

    #[test]
    fn test_reset_sort_falls_through_to_sort_reset_when_no_filter() {
        // ESC (ResetSort) with no active filter and a non-default sort still
        // restores the configured default sort.
        let (mut app, _tmp) = make_app();
        // Change the sort away from the default.
        app.handle_action(Action::EnterCommand);
        for c in "sort year".chars() { app.handle_action(Action::CommandChar(c)); }
        app.handle_action(Action::ExecuteCommand);
        assert_eq!(app.config.display.default_sort.field, "year");
        // No search filter active.
        assert!(app.filtered_indices.is_none());
        // ESC should restore the default sort (citation_key).
        app.handle_action(Action::ResetSort);
        assert_eq!(app.config.display.default_sort.field, "citation_key");
    }

    #[test]
    fn test_sort_command_reruns_search_after_sort_change() {
        // Any :sort command while a search is active must recompute filtered_indices
        // against the new sorted_keys order.
        let (mut app, _tmp) = make_app();
        app.handle_action(Action::EnterSearch);
        for c in "Doe".chars() { app.handle_action(Action::SearchChar(c)); }
        app.handle_action(Action::ConfirmSearch);
        let before = app.filtered_indices.clone().unwrap();
        // Change sort — sorted_keys order changes.
        app.handle_action(Action::EnterCommand);
        for c in "sort year".chars() { app.handle_action(Action::CommandChar(c)); }
        app.handle_action(Action::ExecuteCommand);
        let after = app.filtered_indices.as_ref().expect("filter stays active");
        // The matched key must still be Doe2021 regardless of index position.
        let matched_key = app.sorted_keys.get(after[0]).expect("valid index");
        assert_eq!(matched_key, "Doe2021");
        // The index into the new sorted_keys may differ from before.
        let _ = before; // suppress unused warning
    }

    // ── find_group_node ───────────────────────────────────────────────────────

    #[test]
    fn test_find_group_node_root() {
        let (app, _tmp) = make_app();
        let found = find_group_node(&app.database.groups.root, "All Entries");
        assert!(found.is_some());
    }

    #[test]
    fn test_find_group_node_missing() {
        let (app, _tmp) = make_app();
        let found = find_group_node(&app.database.groups.root, "NoSuchGroup");
        assert!(found.is_none());
    }

    // ── collect_group_names ───────────────────────────────────────────────────

    #[test]
    fn test_collect_group_names_excludes_all_entries() {
        let (app, _tmp) = make_app();
        let mut names = Vec::new();
        collect_group_names(&app.database.groups.root, &mut names);
        assert!(!names.contains(&"All Entries".to_string()));
    }

    // ── visible_entry_count ───────────────────────────────────────────────────

    #[test]
    fn test_visible_entry_count_unfiltered() {
        let (app, _tmp) = make_app();
        assert_eq!(app.visible_entry_count(), 2);
    }

    #[test]
    fn test_visible_entry_count_filtered() {
        let (mut app, _tmp) = make_app();
        app.filtered_indices = Some(vec![0]);
        assert_eq!(app.visible_entry_count(), 1);
    }

    // ── dirty tracking ────────────────────────────────────────────────────────

    #[test]
    fn test_dirty_after_duplicate() {
        let (mut app, _tmp) = make_app();
        assert!(!app.dirty);
        app.handle_action(Action::DuplicateEntry);
        assert!(app.dirty);
    }

    #[test]
    fn test_not_dirty_after_undo_to_clean_state() {
        let (mut app, _tmp) = make_app();
        app.handle_action(Action::DuplicateEntry);
        assert!(app.dirty);
        app.handle_action(Action::Undo);
        assert!(!app.dirty);
    }

    // ── longest_common_prefix ────────────────────────────────────────────────

    #[test]
    fn test_lcp_empty() {
        assert_eq!(longest_common_prefix(&[]), "");
    }

    #[test]
    fn test_lcp_single() {
        assert_eq!(longest_common_prefix(&["bibtui.yaml".to_string()]), "bibtui.yaml");
    }

    #[test]
    fn test_lcp_shared_prefix() {
        let items = vec!["bibtui.yaml".to_string(), "bibtui.yml".to_string()];
        assert_eq!(longest_common_prefix(&items), "bibtui.y");
    }

    #[test]
    fn test_lcp_no_common() {
        let items = vec!["abc".to_string(), "xyz".to_string()];
        assert_eq!(longest_common_prefix(&items), "");
    }

    #[test]
    fn test_lcp_identical() {
        let items = vec!["foo.yaml".to_string(), "foo.yaml".to_string()];
        assert_eq!(longest_common_prefix(&items), "foo.yaml");
    }

    #[test]
    fn test_lcp_one_is_prefix_of_other() {
        let items = vec!["foo".to_string(), "foobar".to_string()];
        assert_eq!(longest_common_prefix(&items), "foo");
    }

    // ── expand_tilde / contract_tilde ────────────────────────────────────────

    #[test]
    fn test_expand_tilde_non_tilde_path_unchanged() {
        assert_eq!(expand_tilde("/tmp/foo.yaml"), "/tmp/foo.yaml");
        assert_eq!(expand_tilde("relative/path"), "relative/path");
    }

    #[test]
    fn test_contract_tilde_non_home_path_unchanged() {
        assert_eq!(contract_tilde("/tmp/foo.yaml"), "/tmp/foo.yaml");
    }

    #[test]
    fn test_expand_contract_roundtrip() {
        if let Ok(home) = std::env::var("HOME") {
            let abs = format!("{}/documents/file.yaml", home);
            let contracted = contract_tilde(&abs);
            assert!(contracted.starts_with("~/"));
            let re_expanded = expand_tilde(&contracted);
            assert_eq!(re_expanded, abs);
        }
    }

    #[test]
    fn test_contract_tilde_exact_home() {
        if let Ok(home) = std::env::var("HOME") {
            assert_eq!(contract_tilde(&home), "~");
        }
    }

    // ── compute_sync_renames ─────────────────────────────────────────────────

    #[test]
    fn test_compute_sync_renames_disabled_returns_empty() {
        let (mut app, _tmp) = make_app();
        app.config.save.sync_filenames = false;
        assert!(app.compute_sync_renames().is_empty());
    }

    #[test]
    fn test_compute_sync_renames_no_dirty_entries_returns_empty() {
        let (mut app, _tmp) = make_app();
        app.config.save.sync_filenames = true;
        // Fresh app has no dirty entries.
        assert!(app.compute_sync_renames().is_empty());
    }

    // ── FocusGroups reveals panel ────────────────────────────────────────────

    #[test]
    fn test_focus_groups_shows_hidden_panel() {
        let (mut app, _tmp) = make_app();
        app.show_groups = false;
        app.handle_action(Action::FocusGroups);
        assert!(app.show_groups, "FocusGroups should reveal the panel when hidden");
        assert_eq!(app.focus, Focus::Groups);
    }

    #[test]
    fn test_focus_list_after_focus_groups() {
        let (mut app, _tmp) = make_app();
        app.handle_action(Action::FocusGroups);
        app.handle_action(Action::FocusList);
        assert_eq!(app.focus, Focus::List);
    }

    // ── Settings path editors ────────────────────────────────────────────────

    #[test]
    fn test_settings_export_enters_path_editing() {
        let (mut app, _tmp) = make_app();
        app.handle_action(Action::EnterSettings);
        app.handle_action(Action::SettingsExport);
        assert_eq!(app.mode, InputMode::Editing);
        let editor = app.field_editor_state.as_ref().expect("editor should be set");
        assert!(editor.is_path);
        assert_eq!(editor.value, "bibtui.yaml");
        assert!(matches!(app.pending_action, Some(PendingAction::ExportSettings)));
    }

    #[test]
    fn test_settings_import_enters_path_editing() {
        let (mut app, _tmp) = make_app();
        app.handle_action(Action::EnterSettings);
        app.handle_action(Action::SettingsImport);
        assert_eq!(app.mode, InputMode::Editing);
        let editor = app.field_editor_state.as_ref().expect("editor should be set");
        assert!(editor.is_path);
        assert!(matches!(app.pending_action, Some(PendingAction::ImportSettings)));
    }

    #[test]
    fn test_cancel_edit_from_settings_returns_to_settings_mode() {
        let (mut app, _tmp) = make_app();
        app.handle_action(Action::EnterSettings);
        app.handle_action(Action::SettingsExport);
        assert_eq!(app.mode, InputMode::Editing);
        app.handle_action(Action::CancelEdit);
        assert_eq!(app.mode, InputMode::Settings);
        assert!(app.field_editor_state.is_none());
        assert!(app.pending_action.is_none());
    }

    // ── new_empty constructor ─────────────────────────────────────────────────

    #[test]
    fn test_new_empty_app_starts_in_editing_mode() {
        let app = App::new_empty(default_config()).unwrap();
        assert_eq!(app.mode, InputMode::Editing);
    }

    #[test]
    fn test_new_empty_app_has_no_entries() {
        let app = App::new_empty(default_config()).unwrap();
        assert_eq!(app.database.entries.len(), 0);
    }

    #[test]
    fn test_new_empty_app_has_path_editor() {
        let app = App::new_empty(default_config()).unwrap();
        let editor = app.field_editor_state.as_ref().expect("editor should be set");
        assert!(editor.is_path, "new_empty should open a path editor");
    }

    #[test]
    fn test_new_empty_app_has_new_file_pending_action() {
        let app = App::new_empty(default_config()).unwrap();
        assert!(matches!(app.pending_action, Some(PendingAction::NewFile)));
    }

    #[test]
    fn test_new_empty_app_cancel_sets_should_quit() {
        let mut app = App::new_empty(default_config()).unwrap();
        app.handle_action(Action::CancelEdit);
        assert!(app.should_quit, "cancelling on a NewFile prompt should quit");
    }

    // ── Month mode in the field editor ───────────────────────────────────────

    /// A bib with a month field for month-editor tests.
    const MONTH_BIB: &str = r#"@Article{Smith2020,
  author  = {Smith, John},
  title   = {My Paper},
  year    = {2020},
  journal = {Nature},
  month   = {jan},
}
"#;

    fn make_app_with_month() -> (App, NamedTempFile) {
        let mut tmp = NamedTempFile::new().unwrap();
        write!(tmp, "{}", MONTH_BIB).unwrap();
        tmp.flush().unwrap();
        let path = tmp.path().to_path_buf();
        let app = App::new(path, default_config()).unwrap();
        (app, tmp)
    }

    /// Open detail view, navigate to the month field, and start editing it.
    /// Returns the app with a month field editor open.
    fn open_month_editor() -> (App, NamedTempFile) {
        let (mut app, tmp) = make_app_with_month();
        app.handle_action(Action::OpenDetail);
        // Manually inject the month field editor so we don't depend on
        // the detail-view navigation (which can vary by field order).
        use crate::tui::components::field_editor::FieldEditorState;
        app.field_editor_state = Some(FieldEditorState::new("month", "jan"));
        app.mode = InputMode::Editing;
        (app, tmp)
    }

    #[test]
    fn test_is_month_flag_on_month_field() {
        use crate::tui::components::field_editor::FieldEditorState;
        let e = FieldEditorState::new("month", "jan");
        assert!(e.is_month);
    }

    #[test]
    fn test_is_month_flag_false_for_title() {
        use crate::tui::components::field_editor::FieldEditorState;
        let e = FieldEditorState::new("title", "x");
        assert!(!e.is_month);
    }

    #[test]
    fn test_edit_cursor_left_in_month_mode_navigates_backward() {
        let (mut app, _tmp) = open_month_editor();
        // value starts at "jan"
        app.handle_action(Action::EditCursorLeft);
        let val = app.field_editor_state.as_ref().unwrap().value.clone();
        // jan backward → dec
        assert_eq!(val, "dec", "EditCursorLeft in month mode should go to dec from jan");
    }

    #[test]
    fn test_edit_cursor_right_in_month_mode_navigates_forward() {
        let (mut app, _tmp) = open_month_editor();
        // value starts at "jan"
        app.handle_action(Action::EditCursorRight);
        let val = app.field_editor_state.as_ref().unwrap().value.clone();
        // jan forward → feb
        assert_eq!(val, "feb", "EditCursorRight in month mode should go to feb from jan");
    }

    #[test]
    fn test_edit_cursor_up_in_month_mode_navigates_minus_6() {
        let (mut app, _tmp) = open_month_editor();
        // Set value to "jul" (index 6) so -6 → jan
        app.field_editor_state.as_mut().unwrap().value = "jul".to_string();
        app.field_editor_state.as_mut().unwrap().cursor = 3;
        app.handle_action(Action::EditCursorUp);
        let val = app.field_editor_state.as_ref().unwrap().value.clone();
        assert_eq!(val, "jan", "EditCursorUp in month mode should navigate -6");
    }

    #[test]
    fn test_edit_cursor_down_in_month_mode_navigates_plus_6() {
        let (mut app, _tmp) = open_month_editor();
        // value starts at "jan" (index 0); +6 → jul (index 6)
        app.handle_action(Action::EditCursorDown);
        let val = app.field_editor_state.as_ref().unwrap().value.clone();
        assert_eq!(val, "jul", "EditCursorDown in month mode should navigate +6");
    }

    #[test]
    fn test_edit_cursor_up_down_noop_for_non_month() {
        let (mut app, _tmp) = make_app();
        // Plain text editor — EditCursorUp/Down should be no-ops.
        use crate::tui::components::field_editor::FieldEditorState;
        app.field_editor_state = Some(FieldEditorState::new("title", "hello"));
        app.mode = InputMode::Editing;
        app.handle_action(Action::EditCursorUp);
        app.handle_action(Action::EditCursorDown);
        // Value must be unchanged
        let val = app.field_editor_state.as_ref().unwrap().value.clone();
        assert_eq!(val, "hello");
    }

    #[test]
    fn test_month_completions_filtered_by_prefix() {
        let (mut app, _tmp) = make_app_with_month();
        use crate::tui::components::field_editor::FieldEditorState;
        // Set up a month editor with prefix "j" — should match jan, jul
        app.field_editor_state = Some(FieldEditorState::new("month", "j"));
        app.field_editor_state.as_mut().unwrap().is_month = true;
        app.update_field_completions();
        let completions = app.field_editor_state.as_ref().unwrap().completions.clone();
        assert!(completions.contains(&"jan".to_string()), "should contain jan");
        assert!(completions.contains(&"jun".to_string()), "should contain jun");
        assert!(completions.contains(&"jul".to_string()), "should contain jul");
        for c in &completions {
            assert!(c.starts_with('j'), "all completions should start with 'j': {}", c);
        }
    }

    #[test]
    fn test_month_completions_all_when_empty_prefix() {
        let (mut app, _tmp) = make_app_with_month();
        use crate::tui::components::field_editor::FieldEditorState;
        // Empty value → all 12 months
        app.field_editor_state = Some(FieldEditorState::new("month", ""));
        app.field_editor_state.as_mut().unwrap().is_month = true;
        app.update_field_completions();
        let completions = app.field_editor_state.as_ref().unwrap().completions.clone();
        assert_eq!(completions.len(), 12);
    }

    #[test]
    fn test_confirm_edit_normalizes_month() {
        let (mut app, _tmp) = make_app_with_month();
        // Open detail view on Smith2020
        app.handle_action(Action::OpenDetail);
        // Manually inject a month editor with the full word "january"
        use crate::tui::components::field_editor::FieldEditorState;
        app.field_editor_state = Some(FieldEditorState::new("month", "january"));
        app.field_editor_state.as_mut().unwrap().is_month = true;
        app.detail_entry_key = Some("Smith2020".to_string());
        app.handle_action(Action::ConfirmEdit);
        // The saved value should be normalized to "jan"
        let saved = app.database.entries.get("Smith2020")
            .and_then(|e| e.fields.get("month"))
            .cloned()
            .unwrap_or_default();
        assert_eq!(saved, "jan", "month should be normalized to 3-letter abbreviation");
    }

    #[test]
    fn test_advance_phase_sets_is_month_when_field_name_is_month() {
        // When a new-field editor advances from name→value phase
        // and field_name == "month", App should set is_month.
        let (mut app, _tmp) = make_app();
        app.handle_action(Action::OpenDetail); // enter detail
        // Inject a new-field editor with "month" as the name
        use crate::tui::components::field_editor::FieldEditorState;
        let mut editor = FieldEditorState::new_field();
        editor.field_name = "month".to_string();
        editor.name_cursor = 5;
        app.field_editor_state = Some(editor);
        app.mode = InputMode::Editing;
        // ConfirmEdit while in name phase → advance to value phase
        app.handle_action(Action::ConfirmEdit);
        // After advancing, is_month should be true
        let is_month = app.field_editor_state.as_ref().map(|e| e.is_month).unwrap_or(false);
        assert!(is_month, "is_month should be set after phase transition to 'month' field");
    }

    // ── Toggle / show_groups init ────────────────────────────────────────────

    #[test]
    fn test_show_groups_respects_config() {
        let mut tmp = NamedTempFile::new().unwrap();
        write!(tmp, "{}", TEST_BIB).unwrap();
        tmp.flush().unwrap();
        let path = tmp.path().to_path_buf();
        let mut cfg = default_config();
        cfg.display.show_groups = false;
        let app = App::new(path, cfg).unwrap();
        assert!(!app.show_groups, "App::new should honour config.display.show_groups");
    }

    // ── handle_doi_fetch_result ──────────────────────────────────────────────

    #[test]
    fn test_handle_doi_fetch_result_sets_doi_field() {
        let (mut app, _tmp) = make_app();
        // Open detail on Smith2020
        app.handle_action(Action::OpenDetail);
        let key = app.detail_entry_key.clone().unwrap();

        app.handle_doi_fetch_result(
            key.clone(),
            Ok(("10.1234/test".to_string(), "https://doi.org/10.1234/test".to_string())),
        );

        let entry = app.database.entries.get(&key).unwrap();
        assert_eq!(entry.fields.get("doi").map(String::as_str), Some("10.1234/test"));
        // URL is redundant (same DOI), so it should NOT be set
        assert!(entry.fields.get("url").is_none() || entry.fields["url"].is_empty(),
            "redundant DOI URL should not be stored in url field");
    }

    #[test]
    fn test_handle_doi_fetch_result_sets_distinct_url() {
        let (mut app, _tmp) = make_app();
        app.handle_action(Action::OpenDetail);
        let key = app.detail_entry_key.clone().unwrap();

        app.handle_doi_fetch_result(
            key.clone(),
            Ok((
                "10.1234/test".to_string(),
                "https://publisher.example.com/article/42".to_string(),
            )),
        );

        let entry = app.database.entries.get(&key).unwrap();
        assert_eq!(entry.fields.get("doi").map(String::as_str), Some("10.1234/test"));
        assert_eq!(
            entry.fields.get("url").map(String::as_str),
            Some("https://publisher.example.com/article/42")
        );
    }

    #[test]
    fn test_handle_doi_fetch_result_marks_entry_dirty() {
        let (mut app, _tmp) = make_app();
        app.handle_action(Action::OpenDetail);
        let key = app.detail_entry_key.clone().unwrap();
        assert!(!app.database.entries[&key].dirty);

        app.handle_doi_fetch_result(key.clone(), Ok(("10.5555/x".to_string(), String::new())));

        assert!(app.database.entries[&key].dirty);
        assert!(app.dirty);
    }

    #[test]
    fn test_handle_doi_fetch_result_already_up_to_date() {
        let (mut app, _tmp) = make_app();
        app.handle_action(Action::OpenDetail);
        let key = app.detail_entry_key.clone().unwrap();

        // Set doi first
        app.database.entries.get_mut(&key).unwrap()
            .fields.insert("doi".to_string(), "10.1234/test".to_string());

        app.handle_doi_fetch_result(
            key.clone(),
            Ok(("10.1234/test".to_string(), String::new())),
        );

        // Should report already-up-to-date, not set dirty
        let msg = app.status_message.as_deref().unwrap_or("");
        assert!(msg.contains("already") || msg.contains("up-to-date"),
            "expected already-up-to-date message, got: {}", msg);
    }

    #[test]
    fn test_handle_doi_fetch_result_error_sets_status() {
        let (mut app, _tmp) = make_app();
        app.handle_action(Action::OpenDetail);
        let key = app.detail_entry_key.clone().unwrap();

        app.handle_doi_fetch_result(key, Err("Network error: timeout".to_string()));

        let msg = app.status_message.as_deref().unwrap_or("");
        assert!(msg.contains("failed") || msg.contains("Network"), "msg: {}", msg);
    }

    #[test]
    fn test_handle_doi_fetch_result_pushes_undo() {
        let (mut app, _tmp) = make_app();
        app.handle_action(Action::OpenDetail);
        let key = app.detail_entry_key.clone().unwrap();
        let before = app.undo_stack.len();

        app.handle_doi_fetch_result(key, Ok(("10.9999/z".to_string(), String::new())));

        assert!(app.undo_stack.len() > before, "undo record should have been pushed");
    }

    #[test]
    fn test_start_fetch_doi_no_entry_selected() {
        let (mut app, _tmp) = make_app();
        // No entry selected in list (deselect by going to empty state) — use a
        // fresh app with no selection driven via filtered_indices
        app.filtered_indices = Some(vec![]); // empty filtered list → no selection
        app.start_fetch_doi();
        // Should set a status message and not panic
        assert!(app.status_message.is_some());
        assert!(app.pending_doi_fetch.is_none());
    }

    #[test]
    fn test_start_fetch_doi_spawns_background_task() {
        let (mut app, _tmp) = make_app();
        // Select first entry which has title+author
        app.handle_action(Action::OpenDetail);
        assert!(app.pending_doi_fetch.is_none());
        app.start_fetch_doi();
        assert!(app.pending_doi_fetch.is_some(), "background fetch should be pending");
        assert!(app.status_message.as_deref().unwrap_or("").contains("earch") ||
                app.status_message.as_deref().unwrap_or("").contains("etch"),
                "status: {:?}", app.status_message);
    }

    // ── open_web / ISBN ──────────────────────────────────────────────────────

    const ISBN_BIB: &str = r#"@Book{Gottschling2016,
  author    = {Gottschling, Peter},
  publisher = {Addison-Wesley},
  title     = {{Discovering Modern C++}},
  year      = {2016},
  isbn      = {978-0-13-679847-7},
  url       = {https://www.oreilly.com/library/view/discovering-modern-c/9780136798477},
}
"#;

    fn make_isbn_app() -> (App, NamedTempFile) {
        let mut tmp = NamedTempFile::new().unwrap();
        write!(tmp, "{}", ISBN_BIB).unwrap();
        tmp.flush().unwrap();
        let path = tmp.path().to_path_buf();
        let app = App::new(path, default_config()).unwrap();
        (app, tmp)
    }

    #[test]
    fn test_open_web_isbn_and_url_shows_picker() {
        // Entry has both isbn and url — expect a picker dialog with 2 options.
        let (mut app, _tmp) = make_isbn_app();
        app.handle_action(Action::OpenWeb);
        assert_eq!(app.mode, InputMode::Dialog, "should enter dialog mode for picker");
        match &app.pending_action {
            Some(PendingAction::OpenWeb(urls)) => {
                assert_eq!(urls.len(), 2, "should have URL and ISBN options");
                let isbn_url = urls.iter().find(|u| u.contains("openlibrary.org"));
                assert!(isbn_url.is_some(), "isbn openlibrary.org URL should be in list");
                // ISBN digits stripped of hyphens/spaces, using search endpoint
                assert!(isbn_url.unwrap().contains("search?isbn=9780136798477"),
                    "openlibrary URL should use search endpoint with clean ISBN: {}", isbn_url.unwrap());
            }
            other => panic!("expected PendingAction::OpenWeb, got {:?}", other),
        }
    }

    #[test]
    fn test_open_web_isbn_hyphenated_strips_cleanly() {
        // Verify the ISBN "978-0-13-679847-7" is normalized to "9780136798477".
        let (mut app, _tmp) = make_isbn_app();
        app.handle_action(Action::OpenWeb);
        if let Some(PendingAction::OpenWeb(urls)) = &app.pending_action {
            let isbn_url = urls.iter().find(|u| u.contains("openlibrary.org")).unwrap();
            // Should not contain hyphens in the URL
            assert!(!isbn_url.contains('-'), "hyphens should be stripped from ISBN URL: {}", isbn_url);
        }
    }

    #[test]
    fn test_open_web_isbn_with_doi_shows_picker_not_fetch() {
        // Entry has isbn + doi — 2 URL candidates → picker dialog, no browser opened,
        // no DOI fetch started. This verifies isbn generates a URL candidate without
        // triggering the browser-opening single-URL path.
        const ISBN_DOI_BIB: &str = r#"@Book{IsbnDoi2020,
  author    = {Author, Test},
  title     = {{Test Book}},
  year      = {2020},
  isbn      = {9781234567890},
  doi       = {10.1234/test},
}
"#;
        let mut tmp = NamedTempFile::new().unwrap();
        write!(tmp, "{}", ISBN_DOI_BIB).unwrap();
        tmp.flush().unwrap();
        let path = tmp.path().to_path_buf();
        let mut app = App::new(path, default_config()).unwrap();
        app.handle_action(Action::OpenWeb);
        // Picker shown (2 URLs: doi + isbn) — no browser opened, no DOI fetch
        assert_eq!(app.mode, InputMode::Dialog, "should show picker for doi + isbn");
        assert!(app.pending_doi_fetch.is_none(),
            "should not start DOI fetch when isbn/doi already provide URLs");
        if let Some(PendingAction::OpenWeb(urls)) = &app.pending_action {
            assert_eq!(urls.len(), 2);
            assert!(urls.iter().any(|u| u.contains("openlibrary.org")),
                "isbn openlibrary URL should be present");
        } else {
            panic!("expected PendingAction::OpenWeb");
        }
    }
}
