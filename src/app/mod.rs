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
use crate::util::clipboard::{Clipboard, SystemClipboard};
use crate::util::open::{effective_file_dir, parse_file_field, serialize_file_field, Opener, SystemOpener};
use crate::tui::components::citation_preview::CitationPreviewState;
use crate::tui::components::help::{HelpContext, HelpState};
use crate::tui::components::name_disambig::{NameCluster, NameDisambigState, NamePreview, NameVariant};
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
use crate::tui::components::field_editor::{collapse_newlines, EditingMode, FieldEditorState};
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
mod completions;
mod editing;
mod groups;
mod import;
mod save;
use completions::*;
use save::*;
pub use actions::Action;
use actions::{UndoItem, PendingAction, MAX_UNDO};

/// Channel for a background DOI-from-metadata lookup: Ok((doi, url)) or an error message.
type DoiFetchReceiver = mpsc::Receiver<Result<(String, String), String>>;

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
    pub name_disambig_state: Option<NameDisambigState>,
    pub help_state: Option<HelpState>,
    pub help_pre_mode: Option<InputMode>,

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
    pending_doi_fetch: Option<(String, DoiFetchReceiver)>,

    /// Compiled user keybinding overrides from config.  Checked before
    /// built-in defaults in `handle_key`.
    user_bindings: Vec<(InputMode, KeyEvent, Action)>,

    // System integrations, swappable for tests
    pub clipboard: Box<dyn Clipboard>,
    pub opener: Box<dyn Opener>,
}

impl App {
    pub fn new(bib_path: PathBuf, config: Config) -> Result<Self> {
        // A path that doesn't exist yet opens a blank library; the file is
        // created on first save.
        let (raw, is_new_file) = if bib_path.exists() {
            let content = std::fs::read_to_string(&bib_path)
                .with_context(|| format!("Failed to read {}", bib_path.display()))?;
            let raw = parse_bib_file(&content)
                .with_context(|| format!("Failed to parse {}", bib_path.display()))?;
            (raw, false)
        } else {
            (RawBibFile { items: vec![] }, true)
        };
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
        let status_message = if is_new_file {
            Some(format!(
                "New file: {} (created on first save)",
                bib_path.display()
            ))
        } else if database.duplicate_keys.is_empty() {
            None
        } else {
            Some(format!(
                "Warning: duplicate citation key(s) in file: {} — only the last copy is editable",
                database.duplicate_keys.join(", ")
            ))
        };
        let app = App {
            database,
            config,
            theme,
            clipboard: Box::new(SystemClipboard),
            opener: Box::new(SystemOpener),
            bib_path,
            mode: InputMode::Normal,
            focus: Focus::List,
            show_groups,
            show_braces,
            render_latex,
            dirty: false,
            should_quit: false,
            status_message,
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
            name_disambig_state: None,
            help_state: None,
            help_pre_mode: None,
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
            clipboard: Box::new(SystemClipboard),
            opener: Box::new(SystemOpener),
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
            name_disambig_state: None,
            help_state: None,
            help_pre_mode: None,
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
            Event::Paste(text) => self.handle_paste(&text),
            Event::Resize(_, _) => {} // Ratatui handles resize automatically
            _ => {}
        }
    }

    /// Handle a bracketed-paste event. Newlines are collapsed into single
    /// spaces so a multi-line paste (e.g. a title copied from a PDF) lands
    /// in one field instead of the first newline confirming the edit.
    fn handle_paste(&mut self, text: &str) {
        let text = collapse_newlines(text);
        if text.is_empty() {
            return;
        }
        if let Some(ref mut editor) = self.field_editor_state {
            editor.save_undo_snapshot();
            if editor.editing_mode == EditingMode::Normal && !editor.editing_name {
                editor.put(&text);
            } else {
                for c in text.chars() {
                    editor.push_char(c);
                }
            }
            self.update_field_completions();
            return;
        }
        match self.mode {
            InputMode::Search => {
                for c in text.chars() {
                    self.handle_action(Action::SearchChar(c));
                }
            }
            InputMode::DetailSearch => {
                for c in text.chars() {
                    self.handle_action(Action::DetailSearchChar(c));
                }
            }
            InputMode::Command => {
                for c in text.chars() {
                    self.handle_action(Action::CommandChar(c));
                }
            }
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
                if let Some(ref mut nds) = self.name_disambig_state {
                    if let Some(ref mut preview) = nds.preview {
                        if preview.scroll + 1 < preview.entries.len() {
                            preview.scroll += 1;
                        }
                    } else {
                        nds.move_down();
                    }
                } else if let Some(ref mut vrs) = self.validate_results_state {
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
                if let Some(ref mut nds) = self.name_disambig_state {
                    if let Some(ref mut preview) = nds.preview {
                        preview.scroll = preview.scroll.saturating_sub(1);
                    } else {
                        nds.move_up();
                    }
                } else if let Some(ref mut vrs) = self.validate_results_state {
                    vrs.scroll_up();
                } else {
                    self.move_cursor(-1);
                    if self.citation_preview_state.is_some() {
                        self.show_citation_preview();
                    }
                }
            }
            Action::MoveToTop => {
                if let Some(ref mut nds) = self.name_disambig_state {
                    nds.cursor = 0;
                } else {
                    self.move_to_top();
                }
            }
            Action::MoveToBottom => {
                if let Some(ref mut nds) = self.name_disambig_state {
                    if !nds.clusters.is_empty() {
                        nds.cursor = nds.clusters.len() - 1;
                    }
                } else {
                    self.move_to_bottom();
                }
            }
            Action::PageDown => {
                if let Some(ref mut nds) = self.name_disambig_state {
                    nds.page_down();
                } else {
                    self.move_cursor(20);
                }
            }
            Action::PageUp => {
                if let Some(ref mut nds) = self.name_disambig_state {
                    nds.page_up();
                } else {
                    self.move_cursor(-20);
                }
            }
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
            Action::SyncFilenames => self.request_sync_filenames(),
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
                    match self.clipboard.copy(&text) {
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
            Action::DisambiguateNames => {
                let clusters = self.build_name_clusters();
                let count = clusters.len();
                self.name_disambig_state = Some(NameDisambigState::new(clusters));
                self.mode = InputMode::NameDisambig;
                if count == 0 {
                    self.status_message = Some("No similar author names found".to_string());
                } else {
                    self.status_message = Some(format!(
                        "{} cluster{} of similar names",
                        count,
                        if count == 1 { "" } else { "s" },
                    ));
                }
            }
            Action::CloseNameDisambig => {
                if let Some(ref mut nds) = self.name_disambig_state {
                    if nds.preview.is_some() {
                        nds.preview = None;
                        return;
                    }
                }
                self.name_disambig_state = None;
                self.mode = InputMode::Normal;
            }
            Action::DisambigCycleVariant => {
                if let Some(ref mut state) = self.name_disambig_state {
                    state.cycle_variant();
                }
            }
            Action::DisambigCycleVariantReverse => {
                if let Some(ref mut state) = self.name_disambig_state {
                    state.cycle_variant_reverse();
                }
            }
            Action::DisambigPreview => {
                if let Some(ref mut nds) = self.name_disambig_state {
                    if nds.preview.is_some() {
                        // Toggle off
                        nds.preview = None;
                    } else if let Some(cluster) = nds.clusters.get(nds.cursor) {
                        let variant_name = cluster.variants[cluster.selected_variant].name.clone();
                        let mut entries: Vec<String> = Vec::new();
                        for (key, entry) in &self.database.entries {
                            for &field in Self::NAME_FIELDS {
                                if let Some(val) = entry.fields.get(field) {
                                    let has_name = val.split(" and ")
                                        .any(|n| n.trim() == variant_name);
                                    if has_name {
                                        let title = entry.fields.get("title")
                                            .cloned()
                                            .unwrap_or_default();
                                        entries.push(format!("{} — {}", key, title));
                                        break;
                                    }
                                }
                            }
                        }
                        entries.sort();
                        nds.preview = Some(NamePreview {
                            variant_name,
                            entries,
                            scroll: 0,
                        });
                    }
                }
            }
            Action::DisambigRemoveVariant => {
                if let Some(ref mut state) = self.name_disambig_state {
                    state.remove_variant();
                    if state.clusters.is_empty() {
                        self.name_disambig_state = None;
                        self.mode = InputMode::Normal;
                        self.status_message = Some("All clusters removed".to_string());
                    }
                }
            }
            Action::ApplyNameDisambig => {
                self.apply_name_disambiguation();
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
            Action::CommandTabComplete => self.do_sort_tab_complete_dir(true),
            Action::CommandTabCompleteReverse => self.do_sort_tab_complete_dir(false),
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
                        match self.clipboard.copy(&text) {
                            Ok(()) => self.status_message = Some("Error message copied to clipboard".to_string()),
                            Err(e) => self.status_message = Some(format!("Clipboard error: {}", e)),
                        }
                    }
                }
            }
            Action::ShowHelp => {
                let context = match self.mode {
                    InputMode::Detail | InputMode::DetailSearch | InputMode::Editing => {
                        HelpContext::Detail
                    }
                    _ => HelpContext::EntryList,
                };
                self.help_pre_mode = Some(self.mode.clone());
                self.help_state = Some(HelpState { context });
                self.mode = InputMode::Help;
            }
            Action::CloseHelp => {
                self.help_state = None;
                self.mode = self.help_pre_mode.take().unwrap_or(InputMode::Normal);
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
            Action::SyncEntryFilename => self.sync_entry_filename(),
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
            Action::EditTabComplete => self.do_field_tab_complete_dir(true),
            Action::EditTabCompleteReverse => self.do_field_tab_complete_dir(false),
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
        let entry_type = EntryType::parse(type_name);
        let (required, _) = crate::bib::entry_types::fields_for_type(&entry_type);

        let mut fields = IndexMap::new();
        for field in required {
            fields.insert(field.to_string(), String::new());
        }

        // Never overwrite an existing entry with the same placeholder key.
        let key = self.unique_citekey(&format!("New_{}", type_name), "");
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
        let new_type = EntryType::parse(type_name);
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
                // Never overwrite an existing entry (e.g. duplicating twice).
                let new_key = self.unique_citekey(&format!("{}_copy", key), "");
                let mut new_entry = entry;
                new_entry.citation_key = new_key.clone();
                new_entry.dirty = true;
                // The copy must get its own raw slot on save — keeping the
                // original's raw_index would overwrite the original on disk.
                new_entry.raw_index = usize::MAX;
                self.database.entries.insert(new_key.clone(), new_entry);
                self.push_undo(UndoItem::EntryAdded { entry_key: new_key });
                self.sorted_keys = sort_entries(&self.database.entries, &self.config);
                self.status_message = Some("Entry duplicated".to_string());
            }
        }
    }

    fn action_entry_key(&self) -> Option<String> {
        self.detail_entry_key.clone().or_else(|| self.selected_entry_key())
    }

    fn open_file(&mut self) {
        use crate::util::open::{parse_file_field, resolve_file_path, effective_file_dir};

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
                match self.opener.open_path(&path) {
                    Ok(()) => self.status_message = Some(format!("Opening {}", path.display())),
                    Err(e) => self.status_message = Some(format!("Error: {}", e)),
                }
                return;
            }
        }

        if files.len() == 1 {
            let path = resolve_file_path(&files[0].path, &bib_dir);
            match self.opener.open_path(&path) {
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
        use crate::util::open::doi_to_url;

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
            }
            1 => {
                let url = urls.remove(0).1;
                match self.opener.open_url(&url) {
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
                self.do_yank(&key, format);
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
        match self.clipboard.copy(&text) {
            Ok(()) => self.status_message = Some(format!("Copied {} to clipboard", label)),
            Err(e) => self.status_message = Some(format!("Clipboard error: {}", e)),
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
                        match self.opener.open_path(&path) {
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
                        match self.opener.open_url(&url) {
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
            Some(PendingAction::SyncFilenamesOnly) => {
                self.sync_filenames(true);
                self.status_message = Some("Filenames synced to citation keys".to_string());
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
            UndoItem::FilenamesSynced { entry_key, old_file_value, renames } => {
                let mut errors: Vec<String> = Vec::new();
                for (new_abs, old_abs) in &renames {
                    if new_abs.exists() {
                        if let Err(e) = std::fs::rename(new_abs, old_abs) {
                            errors.push(format!("rename {}: {}", new_abs.display(), e));
                        }
                    }
                }
                if let Some(entry) = self.database.entries.get_mut(&entry_key) {
                    entry.fields.insert("file".to_string(), old_file_value);
                    entry.dirty = true;
                    if self.detail_entry_key.as_deref() == Some(entry_key.as_str()) {
                        let snapshot = entry.clone();
                        if let Some(ref mut detail) = self.detail_state {
                            detail.refresh(&snapshot);
                        }
                    }
                }
                if errors.is_empty() {
                    self.status_message = Some("Undo: filename sync".to_string());
                } else {
                    self.status_message = Some(format!("Undo errors: {}", errors.join("; ")));
                }
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

    // ── Name disambiguator ──────────────────────────────────────────────────

    /// Person-name fields that should be scanned for disambiguation.
    const NAME_FIELDS: &'static [&'static str] = &[
        "author", "editor", "editora", "editorb", "editorc",
        "bookauthor", "translator",
    ];

    /// Build clusters of similar author names across all entries.
    fn build_name_clusters(&mut self) -> Vec<NameCluster> {
        use std::collections::HashMap;

        // 1. Collect all unique person names and their usage counts.
        let mut name_counts: HashMap<String, usize> = HashMap::new();
        for entry in self.database.entries.values() {
            for &field in Self::NAME_FIELDS {
                if let Some(val) = entry.fields.get(field) {
                    for name in val.split(" and ") {
                        let name = name.trim();
                        if !name.is_empty() {
                            *name_counts.entry(name.to_string()).or_insert(0) += 1;
                        }
                    }
                }
            }
        }

        if name_counts.len() < 2 {
            return Vec::new();
        }

        // 2. Normalize each name to a canonical comparison key: lowercase last
        //    name + first-initial, so "Smith, J." and "Smith, John" compare.
        fn normalize_key(name: &str) -> String {
            let name = name.trim();
            let (last, first) = if let Some(comma) = name.find(',') {
                (name[..comma].trim(), name[comma + 1..].trim())
            } else {
                let parts: Vec<&str> = name.split_whitespace().collect();
                if parts.len() >= 2 {
                    (parts[parts.len() - 1], &name[..name.len() - parts[parts.len() - 1].len()])
                } else {
                    (name, "")
                }
            };
            // Strip braces for comparison
            let last_clean: String = last.chars().filter(|c| *c != '{' && *c != '}').collect();
            let first_clean: String = first.trim().chars().filter(|c| *c != '{' && *c != '}').collect();
            let first_initial = first_clean.chars().next().unwrap_or(' ');
            format!("{}_{}", last_clean.to_lowercase(), first_initial.to_lowercase())
        }

        // 3. Group names by normalized key.
        let mut groups: HashMap<String, Vec<String>> = HashMap::new();
        for name in name_counts.keys() {
            let key = normalize_key(name);
            groups.entry(key).or_default().push(name.clone());
        }

        // 4. Also do fuzzy matching within last-name groups to catch typos.
        //    Group by lowercase last name, then use nucleo within each group.
        let mut last_name_groups: HashMap<String, Vec<String>> = HashMap::new();
        for name in name_counts.keys() {
            let name_trimmed = name.trim();
            let last = if let Some(comma) = name_trimmed.find(',') {
                name_trimmed[..comma].trim()
            } else {
                name_trimmed.split_whitespace().next_back().unwrap_or(name_trimmed)
            };
            let last_clean: String = last.chars().filter(|c| *c != '{' && *c != '}').collect();
            last_name_groups.entry(last_clean.to_lowercase()).or_default().push(name.clone());
        }

        // Use nucleo fuzzy matching to find similar names within the same
        // last-name bucket.
        use nucleo_matcher::pattern::{AtomKind, CaseMatching, Normalization, Pattern};
        use nucleo_matcher::{Config as NucleoConfig, Matcher, Utf32Str};

        let mut matcher = Matcher::new(NucleoConfig::DEFAULT);

        // For each last-name bucket with >1 name, check pairwise fuzzy scores
        for bucket_names in last_name_groups.values() {
            if bucket_names.len() < 2 {
                continue;
            }
            for i in 0..bucket_names.len() {
                for j in (i + 1)..bucket_names.len() {
                    let a = &bucket_names[i];
                    let b = &bucket_names[j];
                    let key_a = normalize_key(a);
                    let key_b = normalize_key(b);
                    if key_a == key_b {
                        continue; // already grouped
                    }
                    let pattern = Pattern::new(
                        a,
                        CaseMatching::Ignore,
                        Normalization::Smart,
                        AtomKind::Fuzzy,
                    );
                    let mut buf = Vec::new();
                    let haystack = Utf32Str::new(b, &mut buf);
                    if let Some(score) = pattern.score(haystack, &mut matcher) {
                        if score > 80 {
                            let merged_key = key_a.clone();
                            if let Some(existing) = groups.get_mut(&key_b) {
                                let taken: Vec<String> = std::mem::take(existing);
                                groups.entry(merged_key).or_default().extend(taken);
                            }
                        }
                    }
                }
            }
        }

        // 5. Remove empty groups and groups with only one unique name.
        let mut clusters: Vec<NameCluster> = Vec::new();
        for names in groups.values() {
            let mut unique: Vec<String> = names.clone();
            unique.sort();
            unique.dedup();
            if unique.len() < 2 {
                continue;
            }
            let mut variants: Vec<NameVariant> = unique
                .iter()
                .map(|n| NameVariant {
                    name: n.clone(),
                    count: *name_counts.get(n).unwrap_or(&0),
                })
                .collect();
            variants.sort_by(|a, b| b.count.cmp(&a.count).then(a.name.cmp(&b.name)));
            let canonical_idx = variants
                .iter()
                .enumerate()
                .max_by_key(|(_, v)| v.name.len())
                .map(|(i, _)| i)
                .unwrap_or(0);
            let canonical = variants[canonical_idx].name.clone();
            clusters.push(NameCluster {
                canonical,
                variants,
                selected_variant: canonical_idx,
            });
        }
        clusters.sort_by_key(|a| a.canonical.to_lowercase());
        clusters
    }

    /// Apply name disambiguation: replace all variant names with the selected
    /// canonical form in each cluster.
    fn apply_name_disambiguation(&mut self) {
        let state = match self.name_disambig_state.take() {
            Some(s) => s,
            None => return,
        };
        if state.clusters.is_empty() {
            self.mode = InputMode::Normal;
            return;
        }

        let mut replacements: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        for cluster in &state.clusters {
            for variant in &cluster.variants {
                if variant.name != cluster.canonical {
                    replacements.insert(variant.name.clone(), cluster.canonical.clone());
                }
            }
        }

        if replacements.is_empty() {
            self.mode = InputMode::Normal;
            self.status_message = Some("No changes needed".to_string());
            return;
        }

        // Collect all mutations first (entry_key, field, old_val, new_val).
        let mut mutations: Vec<(String, String, String, String)> = Vec::new();
        for (key, entry) in &self.database.entries {
            for &field in Self::NAME_FIELDS {
                let val = match entry.fields.get(field) {
                    Some(v) => v,
                    None => continue,
                };
                let names: Vec<&str> = val.split(" and ").collect();
                let mut changed = false;
                let new_names: Vec<String> = names
                    .iter()
                    .map(|n| {
                        let trimmed = n.trim();
                        if let Some(canonical) = replacements.get(trimmed) {
                            changed = true;
                            canonical.clone()
                        } else {
                            trimmed.to_string()
                        }
                    })
                    .collect();
                if changed {
                    mutations.push((
                        key.clone(),
                        field.to_string(),
                        val.clone(),
                        new_names.join(" and "),
                    ));
                }
            }
        }

        let changed_count = mutations.len();
        for (key, field, old_val, new_val) in mutations {
            if self.undo_stack.len() >= MAX_UNDO {
                self.undo_stack.remove(0);
                self.save_generation = self.save_generation.and_then(|g: usize| g.checked_sub(1));
            }
            self.undo_stack.push(UndoItem::FieldChanged {
                entry_key: key.clone(),
                field_name: field.clone(),
                old_value: Some(old_val),
            });
            if let Some(entry) = self.database.entries.get_mut(&key) {
                entry.fields.insert(field, new_val);
                entry.dirty = true;
            }
            self.dirty = true;
        }

        self.mode = InputMode::Normal;
        self.status_message = Some(format!(
            "Disambiguated: {} field{} updated",
            changed_count,
            if changed_count == 1 { "" } else { "s" },
        ));
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

#[cfg(test)]
mod tests;
