use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use crossterm::event::{Event, KeyCode, KeyEvent};
use indexmap::IndexMap;

use crate::bib::citekey::generate_citekey;
use crate::bib::jabref::serialize_group_tree;
use crate::bib::model::*;
use crate::util::open::{effective_file_dir, parse_file_field, serialize_file_field};
use crate::tui::components::citation_preview::CitationPreviewState;
use crate::util::citation::format_citation;
use crate::bib::parser::{build_database, parse_bib_file};
use crate::bib::writer::{serialize_entry, write_bib_file};
use crate::config::schema::Config;
use crate::search::engine::SearchEngine;
use crate::search::filter::filter_by_group;
use crate::tui::components::command_palette::CommandPaletteState;
use crate::tui::components::dialog::{DialogKind, DialogState};
use crate::tui::components::entry_detail::EntryDetailState;
use crate::tui::components::entry_list::EntryListState;
use crate::tui::components::field_editor::FieldEditorState;
use crate::tui::components::group_tree::GroupTreeState;
use crate::tui::components::search_bar::SearchBarState;
use crate::tui::event::poll_event;
use crate::tui::keybindings::{map_key, InputMode};
use crate::tui::components::settings::{SettingValue, SettingsState};
use crate::tui::screens::main_screen::{render_main_screen, Focus};
use crate::tui::screens::edit_screen::render_edit_screen;
use crate::tui::screens::settings_screen::render_settings_screen;
use crate::tui::theme::Theme;
use crate::tui::Term;

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Quit,
    MoveDown,
    MoveUp,
    MoveToTop,
    MoveToBottom,
    PageDown,
    PageUp,
    EnterSearch,
    ExitSearch,
    ConfirmSearch,
    SearchChar(char),
    SearchBackspace,
    OpenDetail,
    CloseDetail,
    EditField,
    AddField,
    DeleteField,
    EditGroups,
    RegenCitekey,
    ConfirmEdit,
    CancelEdit,
    EditChar(char),
    EditBackspace,
    EditDelete,
    EditCursorLeft,
    EditCursorRight,
    EditCursorHome,
    EditCursorEnd,
    EditTabComplete,
    AddEntry,
    DeleteEntry,
    DuplicateEntry,
    YankCitekey,
    ToggleGroups,
    FocusGroups,
    FocusList,
    SelectGroup,
    EnterCommand,
    ExitCommand,
    ExecuteCommand,
    CommandChar(char),
    CommandBackspace,
    DialogConfirm,
    DialogCancel,
    DialogToggle,
    ShowHelp,
    TitlecaseField,
    ToggleBraces,
    ToggleLatex,
    NormalizeAuthor,
    OpenFile,
    OpenWeb,
    CloseCitationPreview,
    Undo,
    // Settings screen
    EnterSettings,
    ExitSettings,
    SettingsMoveDown,
    SettingsMoveUp,
    SettingsToggle,
    SettingsEdit,
    SettingsExport,
    SettingsImport,
}

/// A single reversible operation stored on the undo stack.
#[derive(Debug, Clone)]
enum UndoItem {
    /// A field value was inserted, modified, or deleted on a single entry.
    FieldChanged {
        entry_key: String,
        field_name: String,
        /// `None` means the field did not exist before (so undo removes it).
        old_value: Option<String>,
    },
    /// An entire entry was deleted.
    EntryDeleted { entry: Entry },
    /// An entry was added (new or duplicated); undo removes it.
    EntryAdded { entry_key: String },
    /// The citation key was regenerated; undo restores the old key.
    CitekeyChanged {
        old_key: String,
        new_key: String,
        entry_snapshot: Entry,
    },
    /// The group tree was changed (group added or deleted).
    GroupTreeChanged { old_tree: GroupTree },
    /// An entry's group memberships were reassigned.
    GroupMembershipChanged {
        entry_key: String,
        old_memberships: Vec<String>,
        old_groups_field: Option<String>,
    },
}

const MAX_UNDO: usize = 100;

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

    // Search
    pub search_engine: SearchEngine,
    pub filtered_indices: Option<Vec<usize>>,
    pub sorted_keys: Vec<String>,

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
}

#[derive(Debug)]
enum PendingAction {
    DeleteEntry(String),
    AddEntryType,
    OpenFile(Vec<crate::util::open::ParsedFile>),
    OpenWeb(Vec<String>),
    AddGroup { parent_path: Vec<usize> },
    DeleteGroup { path: Vec<usize> },
    AssignGroups { entry_key: String },
    EditSetting { setting_id: String },
    ExportSettings,
    ImportSettings,
    YankPrompt { entry_key: String },
    Save,
    SaveAndQuit,
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
            search_engine: SearchEngine::new(),
            filtered_indices: None,
            sorted_keys,
            pending_action: None,
            path_completions: Vec::new(),
            path_completion_idx: 0,
            deleted_raw_indices: Vec::new(),
            undo_stack: Vec::new(),
            save_generation: Some(0),
        };

        Ok(app)
    }

    pub fn run(&mut self, terminal: &mut Term) -> Result<()> {
        while !self.should_quit {
            terminal.draw(|f| self.render(f))?;

            if let Some(event) = poll_event(Duration::from_millis(100))? {
                self.handle_event(event);
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
        // Track last key for multi-key combos (gg, dd, yy)
        let last = self.last_key;

        // Update last_key tracking
        self.last_key = match key.code {
            KeyCode::Char(c) => Some(c),
            _ => None,
        };

        if let Some(action) = map_key(key, &self.mode, last) {
            self.handle_action(action);
        }
    }

    fn handle_action(&mut self, action: Action) {
        // Clear status message on any action
        self.status_message = None;

        match action {
            Action::Quit => {
                if self.dirty {
                    self.pending_action = None;
                    self.dialog_state =
                        Some(DialogState::confirm("Quit", "Unsaved changes. Quit anyway?"));
                    self.mode = InputMode::Dialog;
                    self.pending_action = None;
                    // We'll handle confirm => quit
                } else {
                    self.should_quit = true;
                }
            }
            Action::MoveDown => self.move_cursor(1),
            Action::MoveUp => self.move_cursor(-1),
            Action::MoveToTop => self.move_to_top(),
            Action::MoveToBottom => self.move_to_bottom(),
            Action::PageDown => self.move_cursor(20),
            Action::PageUp => self.move_cursor(-20),
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
            Action::CloseDetail => self.close_detail(),
            Action::EditField => self.start_edit_field(),
            Action::AddField => {
                self.field_editor_state = Some(FieldEditorState::new_field());
                self.mode = InputMode::Editing;
            }
            Action::DeleteField => self.delete_field(),
            Action::EditGroups => self.start_edit_groups(),
            Action::RegenCitekey => self.regen_citekey(),
            Action::ConfirmEdit => self.confirm_edit(),
            Action::CancelEdit => {
                self.field_editor_state = None;
                self.pending_action = None;
                self.mode = if self.settings_state.is_some() {
                    InputMode::Settings
                } else if self.detail_state.is_some() {
                    InputMode::Detail
                } else {
                    InputMode::Normal
                };
            }
            Action::EditChar(c) => {
                if let Some(ref mut editor) = self.field_editor_state {
                    editor.push_char(c);
                }
            }
            Action::EditBackspace => {
                if let Some(ref mut editor) = self.field_editor_state {
                    editor.backspace();
                }
            }
            Action::EditDelete => {
                if let Some(ref mut editor) = self.field_editor_state {
                    editor.delete();
                }
            }
            Action::EditCursorLeft => {
                if let Some(ref mut editor) = self.field_editor_state {
                    editor.cursor_left();
                }
            }
            Action::EditCursorRight => {
                if let Some(ref mut editor) = self.field_editor_state {
                    editor.cursor_right();
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
            Action::SelectGroup => {
                if self.focus == Focus::List {
                    self.show_citation_preview();
                } else {
                    self.select_group();
                }
            }
            Action::CloseCitationPreview => {
                self.citation_preview_state = None;
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
            }
            Action::CommandBackspace => {
                self.command_palette_state.backspace();
                if self.command_palette_state.input.is_empty() {
                    self.mode = InputMode::Normal;
                }
            }
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
            Action::ShowHelp => {
                self.status_message = Some(
                    "j/k:nav  /:search  Enter:detail  a:add  dd:del  D:dup  yy:yank  o:file  w:web  B:braces  L:latex  Tab:groups  h/l:focus  a/dd:group(grp focus)  g:assign groups(detail)  u:undo  :w save  q:quit".to_string(),
                );
            }
            Action::TitlecaseField => self.titlecase_selected_field(),
            Action::NormalizeAuthor => self.normalize_author_field(),
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

            // ── Settings ──
            Action::EnterSettings => {
                self.settings_state = Some(SettingsState::new(&self.config));
                self.mode = InputMode::Settings;
            }
            Action::ExitSettings => {
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
                    if let Some(id) = s.selected_id() {
                        // Only open the text editor for free-form string settings.
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
            Action::SettingsExport => {
                // Prompt for export path, defaulting to ./bibtui.yaml
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
            Action::EditTabComplete => self.do_path_tab_complete(),
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

        if self.focus == Focus::Groups {
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
        if self.focus == Focus::Groups {
            self.group_tree_state.select(0);
        } else if let Some(ref mut detail) = self.detail_state {
            detail.move_selection(i32::MIN / 2);
        } else {
            self.entry_list_state.select(0);
        }
    }

    fn move_to_bottom(&mut self) {
        if self.focus == Focus::Groups {
            let count = self.group_tree_state.flat_items.len();
            if count > 0 {
                self.group_tree_state.select(count - 1);
            }
        } else if let Some(ref mut detail) = self.detail_state {
            detail.move_selection(i32::MAX / 2);
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
        if let Some(ref detail) = self.detail_state {
            if let Some((field_name, field_value)) = detail.selected_field() {
                self.field_editor_state =
                    Some(FieldEditorState::new(field_name, field_value));
                self.mode = InputMode::Editing;
            }
        }
    }

    fn confirm_edit(&mut self) {
        // Export settings ─────────────────────────────────────────────────────
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
        if let Some(PendingAction::EditSetting { setting_id }) = self.pending_action.take() {
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
                // Just switched from name to value editing — stay in Editing mode
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
                if editor.value != existing_str {
                    self.push_undo(UndoItem::FieldChanged {
                        entry_key: key.clone(),
                        field_name: editor.field_name.clone(),
                        old_value: existing,
                    });
                    if let Some(entry) = self.database.entries.get_mut(key) {
                        entry.fields.insert(editor.field_name.clone(), editor.value.clone());
                        entry.dirty = true;
                        let snapshot = entry.clone();
                        if let Some(ref mut detail) = self.detail_state {
                            detail.refresh(&snapshot);
                        }
                    }
                }
            }
        }
        self.mode = InputMode::Detail;
    }

    fn delete_field(&mut self) {
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

    fn regen_citekey(&mut self) {
        if let Some(ref key) = self.detail_entry_key.clone() {
            if let Some(entry) = self.database.entries.get(key) {
                let type_name = entry.entry_type.display_name().to_lowercase();
                let template = self
                    .config
                    .citekey
                    .templates
                    .get(&type_name)
                    .cloned()
                    .unwrap_or_else(|| format!("{}_{{}}", type_name));

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

    // ── Entry CRUD ──

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

    fn start_delete_entry(&mut self) {
        if let Some(key) = self.selected_entry_key() {
            self.dialog_state = Some(DialogState::confirm(
                "Delete Entry",
                &format!("Delete '{}'?", key),
            ));
            self.pending_action = Some(PendingAction::DeleteEntry(key));
            self.mode = InputMode::Dialog;
        }
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

    fn normalize_author_field(&mut self) {
        let (field_name, is_author) = match self
            .detail_state
            .as_ref()
            .and_then(|d| d.selected_field())
            .map(|(name, _)| (name.to_string(), name == "author"))
        {
            Some(pair) => pair,
            None => return,
        };

        if !is_author {
            self.status_message = Some("N only works on the 'author' field".to_string());
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
                        Some("Author names normalized to 'Last, First' form".to_string());
                } else {
                    self.status_message =
                        Some("Author names already in 'Last, First' form".to_string());
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

        if files.len() == 1 {
            let bib_dir = effective_file_dir(
                &self.bib_path,
                self.database.jabref_meta.file_directory.as_deref(),
            );
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

        let mut urls: Vec<(String, String)> = Vec::new(); // (label, url)
        if let Some(u) = doi_url {
            urls.push((format!("DOI: {}", u), u));
        }
        if let Some(u) = raw_url {
            urls.push((format!("URL: {}", u), u.clone()));
        }

        match urls.len() {
            0 => {
                self.status_message = Some("No DOI or URL for this entry".to_string());
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
                serialize_entry(entry, self.config.save.align_fields),
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
                if field.is_empty() {
                    // Toggle ascending/descending on current sort field
                    self.config.display.default_sort.ascending =
                        !self.config.display.default_sort.ascending;
                } else if self.config.display.default_sort.field == field {
                    // Same field: toggle direction
                    self.config.display.default_sort.ascending =
                        !self.config.display.default_sort.ascending;
                } else {
                    self.config.display.default_sort.field = field.clone();
                    self.config.display.default_sort.ascending = true;
                }
                self.sorted_keys = sort_entries(&self.database.entries, &self.config);
                self.entry_list_state.select(0);
                let dir = if self.config.display.default_sort.ascending { "↑" } else { "↓" };
                self.status_message = Some(format!(
                    "Sorted by {} {}",
                    self.config.display.default_sort.field, dir
                ));
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
            Some(PendingAction::AddGroup { .. })
            | Some(PendingAction::EditSetting { .. })
            | Some(PendingAction::ExportSettings)
            | Some(PendingAction::ImportSettings) => {
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

    fn do_path_tab_complete(&mut self) {
        // Only active for path-editing pending actions.
        let is_path_edit = matches!(
            self.pending_action,
            Some(PendingAction::ExportSettings) | Some(PendingAction::ImportSettings)
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

    /// Sync all runtime fields that shadow config values, and rebuild the theme.
    /// Call this whenever the config is mutated (settings toggle, edit, or import).
    fn sync_runtime_from_config(&mut self) {
        self.render_latex = self.config.display.render_latex;
        self.show_braces  = self.config.display.show_braces;
        self.show_groups  = self.config.display.show_groups;
        self.theme        = Theme::from_config(&self.config.theme);
    }

    fn import_settings(&mut self, path: &str) {
        match std::fs::read_to_string(path) {
            Ok(contents) => match serde_yaml::from_str::<crate::config::schema::Config>(&contents) {
                Ok(cfg) => {
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
    /// Only dirty entries with a `file` field are processed.  The actual file is
    /// renamed on disk; if the rename fails the entry is left unchanged.
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
            .filter(|(_, e)| e.dirty && e.fields.contains_key("file"))
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

                let new_filename = if multi {
                    format!("{}_{}.{}", citekey, i + 1, ext)
                } else {
                    format!("{}.{}", citekey, ext)
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
            if !entry.dirty {
                continue;
            }
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
                let new_filename = if multi {
                    format!("{}_{}.{}", citekey, i + 1, ext)
                } else {
                    format!("{}.{}", citekey, ext)
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

        // Backup
        if self.config.general.backup_on_save {
            let backup_path = self.bib_path.with_extension("bib.bak");
            if let Err(e) = std::fs::copy(&self.bib_path, &backup_path) {
                self.status_message = Some(format!("Backup failed: {}", e));
                return;
            }
        }

        // Update raw file for dirty entries
        self.sync_dirty_entries();

        // Write
        let output = write_bib_file(&self.database.raw_file);
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

    fn sync_dirty_entries(&mut self) {
        let dirty_keys: Vec<String> = self
            .database
            .entries
            .iter()
            .filter(|(_, e)| e.dirty)
            .map(|(k, _)| k.clone())
            .collect();

        for key in dirty_keys {
            if let Some(entry) = self.database.entries.get(&key) {
                let serialized =
                    serialize_entry(entry, self.config.save.align_fields);

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
}

fn sort_entries(entries: &IndexMap<String, Entry>, config: &Config) -> Vec<String> {
    let mut keys: Vec<String> = entries.keys().cloned().collect();

    let field = &config.display.default_sort.field;
    let ascending = config.display.default_sort.ascending;

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
    // Truncate to a valid UTF-8 boundary.
    let s = &items[0][..len];
    let boundary = s.char_indices().map(|(i, _)| i).take_while(|&i| i <= len).last().unwrap_or(0);
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
        app.handle_action(Action::Quit);
        assert!(app.should_quit);
    }

    #[test]
    fn test_quit_when_dirty_opens_dialog() {
        let (mut app, _tmp) = make_app();
        app.dirty = true;
        app.handle_action(Action::Quit);
        assert!(!app.should_quit);
        assert!(app.dialog_state.is_some());
        assert_eq!(app.mode, InputMode::Dialog);
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
    fn test_show_help_sets_status() {
        let (mut app, _tmp) = make_app();
        app.handle_action(Action::ShowHelp);
        assert!(app.status_message.is_some());
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
        app.field_editor_state = Some(FieldEditorState::new("title", "abc"));
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
}
