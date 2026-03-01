use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use crossterm::event::{Event, KeyCode, KeyEvent};
use indexmap::IndexMap;

use crate::bib::citekey::generate_citekey;
use crate::bib::jabref::serialize_group_tree;
use crate::bib::model::*;
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
use crate::tui::screens::main_screen::{render_main_screen, Focus};
use crate::tui::screens::edit_screen::render_edit_screen;
use crate::tui::theme::Theme;
use crate::tui::Term;

#[derive(Debug, Clone)]
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
}

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

    // Search
    pub search_engine: SearchEngine,
    pub filtered_indices: Option<Vec<usize>>,
    pub sorted_keys: Vec<String>,

    // Pending action context
    pending_action: Option<PendingAction>,
    /// Raw indices of entries deleted this session (for sync on save)
    deleted_raw_indices: Vec<usize>,
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

        let show_braces = config.display.show_braces;
        let render_latex = config.display.render_latex;

        let app = App {
            database,
            config,
            theme,
            bib_path,
            mode: InputMode::Normal,
            focus: Focus::List,
            show_groups: true,
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
            search_engine: SearchEngine::new(),
            filtered_indices: None,
            sorted_keys,
            pending_action: None,
            deleted_raw_indices: Vec::new(),
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
        if self.detail_state.is_some() {
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
                self.mode = if self.detail_state.is_some() {
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
                if self.show_groups {
                    self.focus = Focus::Groups;
                }
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
                    "j/k:nav  /:search  Enter:detail  a:add  dd:del  D:dup  yy:yank  o:file  w:web  B:braces  L:latex  Tab:groups  h/l:focus  a/dd:group(grp focus)  g:assign groups(detail)  :w save  q:quit".to_string(),
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
            if let Some(ref key) = self.detail_entry_key {
                if let Some(entry) = self.database.entries.get_mut(key) {
                    let existing = entry.fields.get(&editor.field_name).map(|s| s.as_str()).unwrap_or("");
                    if editor.value != existing {
                        entry
                            .fields
                            .insert(editor.field_name.clone(), editor.value.clone());
                        entry.dirty = true;
                        self.dirty = true;

                        if let Some(ref mut detail) = self.detail_state {
                            detail.refresh(entry);
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
                if let Some(entry) = self.database.entries.get_mut(key) {
                    entry.fields.shift_remove(&field_name);
                    entry.dirty = true;
                    self.dirty = true;

                    let entry_clone = entry.clone();
                    if let Some(ref mut detail) = self.detail_state {
                        detail.refresh(&entry_clone);
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
                        entry.citation_key = new_key.clone();
                        entry.dirty = true;
                        self.database.entries.insert(new_key.clone(), entry);
                        self.detail_entry_key = Some(new_key);
                        self.dirty = true;
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
        self.dirty = true;
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
        if let Some(entry) = self.database.entries.get(key) {
            if entry.raw_index != usize::MAX {
                self.deleted_raw_indices.push(entry.raw_index);
            }
        }
        self.database.entries.shift_remove(key);
        self.dirty = true;
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
                self.database.entries.insert(new_key, new_entry);
                self.dirty = true;
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
                if let Some(entry) = self.database.entries.get_mut(&key) {
                    if let Some(value) = entry.fields.get(&field_name).cloned() {
                        let converted = crate::util::titlecase::apply_titlecase(
                            &value,
                            &self.config.titlecase.ignore_words,
                        );
                        if converted != value {
                            entry.fields.insert(field_name.clone(), converted);
                            entry.dirty = true;
                            self.dirty = true;
                            let entry_clone = entry.clone();
                            if let Some(ref mut detail) = self.detail_state {
                                detail.refresh(&entry_clone);
                            }
                            self.status_message =
                                Some(format!("Title-cased '{}'", field_name));
                        } else {
                            self.status_message =
                                Some(format!("'{}' already in title case", field_name));
                        }
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
            if let Some(entry) = self.database.entries.get_mut(&key) {
                if let Some(value) = entry.fields.get(&field_name).cloned() {
                    let normalized =
                        crate::util::author::normalize_author_names(&value);
                    if normalized != value {
                        entry.fields.insert(field_name.clone(), normalized);
                        entry.dirty = true;
                        self.dirty = true;
                        let entry_clone = entry.clone();
                        if let Some(ref mut detail) = self.detail_state {
                            detail.refresh(&entry_clone);
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
    }

    fn action_entry_key(&self) -> Option<String> {
        self.detail_entry_key.clone().or_else(|| self.selected_entry_key())
    }

    fn open_file(&mut self) {
        use crate::util::open::{parse_file_field, resolve_file_path, open_path};

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
            let bib_dir = self.bib_path
                .parent()
                .unwrap_or(std::path::Path::new("."))
                .to_path_buf();
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
        if let Some(key) = self.selected_entry_key() {
            match crate::util::clipboard::copy_to_clipboard(&key) {
                Ok(()) => {
                    self.status_message = Some(format!("Copied '{}' to clipboard", key));
                }
                Err(e) => {
                    self.status_message = Some(format!("Clipboard error: {}", e));
                }
            }
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
            "w" | "write" | "save" => self.save(),
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
                self.save();
                self.should_quit = true;
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
                        let bib_dir = self.bib_path
                            .parent()
                            .unwrap_or(std::path::Path::new("."))
                            .to_path_buf();
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
            Some(PendingAction::AddGroup { .. }) => {
                // AddGroup is confirmed through confirm_edit(), not this path
            }
            None => {
                // Quit confirmation
                self.should_quit = true;
            }
        }
    }

    // ── Save ──

    fn save(&mut self) {
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
        self.dirty = true;
        self.status_message = Some(format!("Group '{}' added", name));
    }

    fn finish_delete_group(&mut self, path: Vec<usize>) {
        if path.is_empty() {
            return;
        }
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
                self.dirty = true;
                self.status_message =
                    Some(format!("Group '{}' deleted", removed.group.name));
            }
        }
    }

    fn finish_assign_groups(&mut self, entry_key: &str, selected_groups: Vec<String>) {
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
            self.dirty = true;
            let entry_clone = entry.clone();
            if let Some(ref mut detail) = self.detail_state {
                detail.refresh(&entry_clone);
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
