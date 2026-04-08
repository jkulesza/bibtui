use crate::bib::model::Entry;
use crate::bib::model::GroupTree;

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    MoveDown,
    MoveUp,
    MoveToTop,
    MoveToBottom,
    PageDown,
    PageUp,
    EnterSearch,
    ExitSearch,
    ResetSort,
    ConfirmSearch,
    SearchChar(char),
    SearchBackspace,
    OpenDetail,
    CloseDetail,
    EnterDetailSearch,
    ExitDetailSearch,
    DetailSearchChar(char),
    DetailSearchBackspace,
    DetailNextMatch,
    DetailPrevMatch,
    EditField,
    AddField,
    AddFileAttachment,
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
    EditCursorUp,
    EditCursorDown,
    EditCursorHome,
    EditCursorEnd,
    EditTabComplete,
    AddEntry,
    ChangeEntryType,
    DeleteEntry,
    DuplicateEntry,
    YankCitekey,
    ToggleGroups,
    FocusGroups,
    FocusList,
    ShowCitationPreview,
    EnterCommand,
    ExitCommand,
    ExecuteCommand,
    CommandChar(char),
    CommandBackspace,
    CommandTabComplete,
    DialogConfirm,
    DialogCancel,
    DialogToggle,
    DialogYank,
    ShowHelp,
    TitlecaseField,
    ToggleBraces,
    ToggleLatex,
    NormalizeNames,
    EditEnterReplace,
    OpenFile,
    OpenWeb,
    CloseCitationPreview,
    YankCitationPreview,
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
    SettingsAddFieldGroup,
    SettingsDeleteFieldGroup,
    SettingsRenameFieldGroup,
    SettingsMoveToTop,
    SettingsMoveToBottom,
    SettingsPageDown,
    SettingsPageUp,
    // Validate
    Validate,
    CloseValidateResults,
    // Import / Export
    ImportEntry,
    /// Export all entries as CSL-JSON to a user-specified path.
    ExportJson,
    /// Export all entries in RIS format to a user-specified path.
    ExportRis,
    // Help
    CloseHelp,
    // Vim modal editing
    EditUndo,
    EditPut,
    EditYank,
    EditEnterNormal,
    EditEnterInsert,
    EditEnterInsertAfter,
    EditEnterInsertAtEnd,
    EditEnterInsertAtHome,
    EditMoveWordFwd,
    EditMoveWordBwd,
    EditMoveWordEnd,
    EditMoveBigWordFwd,
    EditMoveBigWordBwd,
    EditMoveBigWordEnd,
    EditDeleteWordFwd,
    EditDeleteToEnd,
    EditChangeToEnd,
    EditSubstituteChar,
    EditSubstituteLine,
    EditToggleCase,
    EditReplaceChar(char),
    EditFindCharFwd(char),
    EditFindCharBwd(char),
    EditDeleteCharBack,
    EditDeleteWordBack,
    EditDeleteToHome,
    EditConfirmAndMoveDown,
    EditConfirmAndMoveUp,
}

/// A single reversible operation stored on the undo stack.
#[derive(Debug, Clone)]
pub(super) enum UndoItem {
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
    /// An entry's type was changed; undo restores the old type.
    EntryTypeChanged {
        entry_key: String,
        old_type: crate::bib::model::EntryType,
    },
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

pub(super) const MAX_UNDO: usize = 100;

#[derive(Debug)]
pub(super) enum PendingAction {
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
    AddFieldGroup,
    EditFieldGroupFields { index: usize },
    RenameFieldGroup { index: usize },
    /// No bib file was given on the command line; waiting for the user to
    /// supply a path for the new library before we can do anything else.
    NewFile,
    /// Waiting for the user to type a DOI or URL to import.
    ImportUrl,
    /// Deleting an entry that has exactly one local file; TypePicker offers
    /// "delete entry+file", "delete entry only", or "cancel".
    DeleteEntryWithFile { entry_key: String, file: std::path::PathBuf },
    /// Deleting an entry that has multiple local files; FileDeleteSelect lets
    /// the user choose which files to also remove.
    DeleteEntryWithFileSelect { entry_key: String, files: Vec<std::path::PathBuf> },
    /// Waiting for the user to provide a path for a new file attachment.
    AddFileAttachment { entry_key: String },
    /// Dismiss a non-interactive message popup (no side effect on confirm).
    DismissMessage,
    /// Waiting for the user to edit the path of the file at `index` in the `file` field.
    EditFileAttachment { entry_key: String, index: usize },
    /// Waiting for the user to type a path for CSL-JSON export.
    ExportJson,
    /// Waiting for the user to type a path for RIS export.
    ExportRis,
    /// Waiting for the user to pick a new type for an existing entry.
    ChangeEntryType { entry_key: String },
    /// Waiting for the user to type a field name for a new display column.
    AddColumn,
    /// Waiting for the user to edit the width spec of a column.
    EditColumnWidth { index: usize },
    /// Waiting for the user to rename a column (field|header string).
    RenameColumn { index: usize },
}
