//! Library-level settings stored inside the `.bib` file.
//!
//! A library can carry its own configuration, which takes precedence over the
//! YAML config: built-in defaults < YAML < library.
//!
//! - Capabilities JabRef shares use JabRef's own metadata keys:
//!   `keypattern_<type>` (citation-key templates), `saveOrderConfig` (entry
//!   order on save), and `saveActions` (field formatters JabRef also has).
//! - Every other setting is stored as
//!   `@Comment{jabref-meta: bibtui.<path>:<json>;}`. JabRef keeps unknown
//!   metadata keys when it saves (verified with JabRef 5.15), rewriting them
//!   in its own multi-line layout, but drops one level of backslashes. Values
//!   are therefore one-line JSON with `%`, `\`, `{` and `}` percent-encoded
//!   inside strings, and `;` escaped as `\;` the way JabRef escapes it.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::Value;

use super::schema::Config;
use crate::bib::jabref::{escape_meta, split_meta_list, unescape_meta};
use crate::bib::model::JabRefMeta;

/// Metadata key prefix for bibtui-only settings.
pub const BIBTUI_PREFIX: &str = "bibtui.";

/// Settings whose value is a user-keyed map, stored as one JSON object.
const MAP_PATHS: &[&str] = &["keybindings", "entry_types", "save.journal_abbreviations"];

/// Settings that never live in a library.
const EXCLUDED: &[&str] = &["general.bib_file"];

/// Text fields that contain natural-language prose or titles.
pub const TEXT_FIELDS: &[&str] = &[
    "abstract",
    "addendum",
    "address",
    "annote",
    "booktitle",
    "chapter",
    "edition",
    "institution",
    "journal",
    "keywords",
    "language",
    "note",
    "organization",
    "publisher",
    "school",
    "series",
    "subtitle",
    "title",
    "titleaddon",
    "type",
    "venue",
];

/// Person-name (name-list) fields.
pub const NAME_FIELDS: &[&str] = &[
    "author",
    "editor",
    "editora",
    "editorb",
    "editorc",
    "bookauthor",
    "afterword",
    "translator",
];

#[derive(Clone, Copy)]
enum Fields {
    Text,
    Names,
    TextAndNames,
    One(&'static str),
}

impl Fields {
    fn list(self) -> Vec<&'static str> {
        match self {
            Fields::Text => TEXT_FIELDS.to_vec(),
            Fields::Names => NAME_FIELDS.to_vec(),
            Fields::TextAndNames => TEXT_FIELDS.iter().chain(NAME_FIELDS).copied().collect(),
            Fields::One(field) => vec![field],
        }
    }
}

/// bibtui save actions that correspond to JabRef field formatters, with the
/// fields bibtui applies them to (see `compute_save_transforms`).
const JABREF_SAVE_ACTIONS: &[(&str, &str, Fields)] = &[
    (
        "save.save_action_unicode_to_latex",
        "unicode_to_latex",
        Fields::TextAndNames,
    ),
    (
        "save.save_action_escape_underscores",
        "escapeUnderscores",
        Fields::Text,
    ),
    (
        "save.save_action_escape_ampersands",
        "escapeAmpersands",
        Fields::TextAndNames,
    ),
    (
        "save.save_action_latex_cleanup",
        "latex_cleanup",
        Fields::Text,
    ),
    (
        "save.save_action_cleanup_url",
        "cleanup_url",
        Fields::One("url"),
    ),
    (
        "save.save_action_ordinals_to_superscript",
        "ordinals_to_superscript",
        Fields::Text,
    ),
    (
        "save.save_action_normalize_date",
        "normalize_date",
        Fields::One("date"),
    ),
    (
        "save.save_action_normalize_month",
        "normalize_month",
        Fields::One("month"),
    ),
    (
        "save.save_action_normalize_page_numbers",
        "normalize_page_numbers",
        Fields::One("pages"),
    ),
    (
        "save.save_action_normalize_names_of_persons",
        "normalize_names",
        Fields::Names,
    ),
];

const ENTRY_SORT_ORDER: &str = "save.entry_sort_order";
const CITEKEY_TEMPLATES: &str = "citekey.templates.";

/// True for settings stored in JabRef's `saveActions` rather than `bibtui.*`.
pub fn is_jabref_save_action(path: &str) -> bool {
    JABREF_SAVE_ACTIONS.iter().any(|(p, _, _)| *p == path)
}

// ── Value encoding ──────────────────────────────────────────────────────────

/// Encode a setting value for a `bibtui.*` metadata comment.
pub fn encode_value(value: &Value) -> String {
    let json = value.to_string();
    let mut out = String::with_capacity(json.len());
    let mut in_string = false;
    let mut escaped = false;
    for c in json.chars() {
        if !in_string {
            in_string = c == '"';
            out.push(c);
            continue;
        }
        if !escaped && c == '"' {
            in_string = false;
            out.push(c);
            continue;
        }
        escaped = !escaped && c == '\\';
        match c {
            '%' => out.push_str("%25"),
            '\\' => out.push_str("%5C"),
            '{' => out.push_str("%7B"),
            '}' => out.push_str("%7D"),
            ';' => out.push_str("\\;"),
            _ => out.push(c),
        }
    }
    out
}

/// Decode a `bibtui.*` metadata value written by [`encode_value`], including
/// after JabRef has re-escaped it. A hand-written bare word (`alphabetical`)
/// is accepted as a string; malformed JSON strings, lists, or objects are
/// errors.
pub fn decode_value(text: &str) -> Result<Value, String> {
    let unescaped = unescape_meta(text.trim());
    let mut decoded = String::with_capacity(unescaped.len());
    let mut rest = unescaped.as_str();
    while let Some(index) = rest.find('%') {
        decoded.push_str(&rest[..index]);
        let code = rest.get(index..index + 3);
        let replacement = match code {
            Some("%25") => Some('%'),
            Some("%5C") | Some("%5c") => Some('\\'),
            Some("%7B") | Some("%7b") => Some('{'),
            Some("%7D") | Some("%7d") => Some('}'),
            _ => None,
        };
        match replacement {
            Some(c) => {
                decoded.push(c);
                rest = &rest[index + 3..];
            }
            None => {
                decoded.push('%');
                rest = &rest[index + 1..];
            }
        }
    }
    decoded.push_str(rest);
    match serde_json::from_str(&decoded) {
        Ok(value) => Ok(value),
        Err(_) if !decoded.starts_with(['"', '[', '{']) && !decoded.is_empty() => {
            Ok(Value::String(decoded))
        }
        Err(error) => Err(error.to_string()),
    }
}

// ── Setting paths ───────────────────────────────────────────────────────────

/// The configuration as JSON, the common form for comparing and merging.
pub fn config_value(config: &Config) -> Value {
    serde_json::to_value(config).unwrap_or(Value::Null)
}

/// Flatten a configuration into `path → value` leaves. Struct fields are
/// expanded; lists and user-keyed maps (keybindings, entry types, journal
/// abbreviations) are single leaves. Citation-key templates are expanded per
/// entry type because each one maps to its own JabRef `keypattern_<type>`.
pub fn flatten(config: &Config) -> BTreeMap<String, Value> {
    fn walk(prefix: &str, value: &Value, out: &mut BTreeMap<String, Value>) {
        match value {
            Value::Object(map) if !MAP_PATHS.contains(&prefix) => {
                for (key, child) in map {
                    let path = if prefix.is_empty() {
                        key.clone()
                    } else {
                        format!("{prefix}.{key}")
                    };
                    walk(&path, child, out);
                }
            }
            _ => {
                out.insert(prefix.to_string(), value.clone());
            }
        }
    }
    let mut out = BTreeMap::new();
    walk("", &config_value(config), &mut out);
    out
}

/// Look up a dotted path in a JSON value.
pub fn get_path<'a>(root: &'a Value, path: &str) -> Option<&'a Value> {
    path.split('.').try_fold(root, |value, key| value.get(key))
}

/// Set a dotted path in a JSON value, creating intermediate objects.
pub fn set_path(root: &mut Value, path: &str, value: Value) {
    let mut current = root;
    let mut keys = path.split('.').peekable();
    while let Some(key) = keys.next() {
        if !current.is_object() {
            *current = Value::Object(Default::default());
        }
        let Value::Object(map) = current else { return };
        if keys.peek().is_none() {
            map.insert(key.to_string(), value);
            return;
        }
        current = map
            .entry(key.to_string())
            .or_insert_with(|| Value::Object(Default::default()));
    }
}

/// Merge library overrides into a base configuration. Each override is
/// applied on its own; one that is unknown or does not fit the setting's type
/// is skipped with a warning instead of invalidating the rest.
pub fn apply_overrides(
    base: &Config,
    overrides: &BTreeMap<String, Value>,
) -> (Config, Vec<String>) {
    let mut root = config_value(base);
    let mut warnings = Vec::new();
    for (path, value) in overrides {
        if EXCLUDED.contains(&path.as_str()) {
            warnings.push(format!("{path} cannot be set by a library; ignored"));
            continue;
        }
        let mut candidate = root.clone();
        set_path(&mut candidate, path, value.clone());
        match serde_json::from_value::<Config>(candidate) {
            Ok(config) => {
                let applied = config_value(&config);
                if get_path(&applied, path) == Some(value) {
                    root = applied;
                } else {
                    warnings.push(format!("unknown library setting {path}; ignored"));
                }
            }
            Err(error) => warnings.push(format!("library setting {path}: {error}; ignored")),
        }
    }
    let mut config = serde_json::from_value(root).unwrap_or_else(|_| base.clone());
    super::defaults::normalize_citekey_templates(&mut config);
    (config, warnings)
}

/// Short names of save settings (`save.*`) whose effective value differs
/// from the YAML layer, e.g. `latex_cleanup off`, `entry_sort_order=none`.
pub fn overridden_save_settings(base: &Config, effective: &Config) -> Vec<String> {
    let base = flatten(base);
    flatten(effective)
        .into_iter()
        .filter(|(path, value)| path.starts_with("save.") && base.get(path) != Some(value))
        .map(|(path, value)| {
            let name = path
                .trim_start_matches("save.")
                .trim_start_matches("save_action_")
                .to_string();
            match value {
                Value::Bool(true) => format!("{name} on"),
                Value::Bool(false) => format!("{name} off"),
                Value::String(s) => format!("{name}={s}"),
                _ => name,
            }
        })
        .collect()
}

// ── Reading library settings ────────────────────────────────────────────────

/// JabRef `saveActions` as bibtui understands them.
#[derive(Debug, Clone, PartialEq)]
pub struct SaveActionsState {
    /// Value of each bibtui save action listed in [`JABREF_SAVE_ACTIONS`].
    pub flags: BTreeMap<String, bool>,
    /// False when the JabRef configuration contains formatters or field
    /// combinations bibtui cannot represent; bibtui then never rewrites it.
    pub exact: bool,
}

/// Settings read from a library's metadata.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LibrarySettings {
    /// Decoded `bibtui.*` values by setting path (without the prefix).
    pub values: BTreeMap<String, Value>,
    /// Raw metadata keys of every `bibtui.*` comment, including invalid ones.
    pub bibtui_keys: BTreeSet<String>,
    /// Citation-key templates from `keypattern_<type>`.
    pub key_patterns: BTreeMap<String, String>,
    /// `save.entry_sort_order` from `saveOrderConfig`, when representable.
    pub entry_sort_order: Option<String>,
    /// True when a `saveOrderConfig` exists but cannot be represented.
    pub save_order_unrepresentable: bool,
    /// Save actions from JabRef's `saveActions`.
    pub save_actions: Option<SaveActionsState>,
    /// Problems found while reading; shown at startup.
    pub warnings: Vec<String>,
}

impl LibrarySettings {
    pub fn from_meta(meta: &JabRefMeta) -> Self {
        let mut settings = LibrarySettings::default();
        for (key, raw) in &meta.unknown_meta {
            let Some(path) = key.strip_prefix(BIBTUI_PREFIX) else {
                continue;
            };
            settings.bibtui_keys.insert(key.clone());
            match decode_value(raw) {
                Ok(value) => {
                    settings.values.insert(path.to_string(), value);
                }
                Err(error) => settings.warnings.push(format!(
                    "library setting {path} is not valid JSON ({error}); ignored"
                )),
            }
        }
        settings.key_patterns = meta
            .key_patterns
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        if let Some(raw) = &meta.save_order_config {
            match parse_save_order(raw) {
                Some(order) => settings.entry_sort_order = Some(order),
                None => {
                    settings.save_order_unrepresentable = true;
                    settings.warnings.push(format!(
                        "JabRef saveOrderConfig '{}' has no bibtui equivalent (bibtui sorts by citation key or keeps file order); ignored",
                        raw.trim()
                    ));
                }
            }
        }
        if let Some(raw) = &meta.save_actions {
            let state = parse_save_actions(raw);
            if !state.exact {
                settings.warnings.push(
                    "JabRef saveActions use formatters bibtui does not model; bibtui reads the ones it knows and will not rewrite them"
                        .to_string(),
                );
            }
            settings.save_actions = Some(state);
        }
        settings
    }

    /// Every setting the library overrides, by setting path. JabRef-native
    /// keys win over a `bibtui.*` value for the same setting.
    pub fn overrides(&self) -> BTreeMap<String, Value> {
        let mut out: BTreeMap<String, Value> = self
            .values
            .iter()
            .filter(|(path, _)| {
                !path.starts_with(CITEKEY_TEMPLATES)
                    && path.as_str() != ENTRY_SORT_ORDER
                    && !(self.save_actions.is_some() && is_jabref_save_action(path))
            })
            .map(|(path, value)| (path.clone(), value.clone()))
            .collect();
        for (type_name, pattern) in &self.key_patterns {
            out.insert(
                format!("{CITEKEY_TEMPLATES}{type_name}"),
                Value::String(pattern.clone()),
            );
        }
        if let Some(order) = &self.entry_sort_order {
            out.insert(ENTRY_SORT_ORDER.into(), Value::String(order.clone()));
        }
        if let Some(actions) = &self.save_actions {
            for (path, on) in &actions.flags {
                out.insert(path.clone(), Value::Bool(*on));
            }
        }
        out
    }
}

/// Map a JabRef `saveOrderConfig` to `save.entry_sort_order`.
fn parse_save_order(raw: &str) -> Option<String> {
    let items = split_meta_list(raw);
    let items: Vec<&str> = items.iter().map(|s| s.trim()).collect();
    match items.as_slice() {
        ["original", ..] => Some("none".into()),
        ["specified", "citationkey" | "bibtexkey", "false", ..] => Some("citation_key".into()),
        _ => None,
    }
}

/// The JabRef `saveOrderConfig` value for an entry sort order.
fn save_order_value(order: &str) -> &'static str {
    if order == "none" {
        "original;"
    } else {
        "specified;citationkey;false;"
    }
}

/// Read JabRef `saveActions` (`enabled;` then one `field[formatter,…]` per line).
fn parse_save_actions(raw: &str) -> SaveActionsState {
    let items = split_meta_list(raw);
    let enabled = items.first().map(|s| s.trim()) == Some("enabled");
    let mut pairs: BTreeSet<(String, String)> = BTreeSet::new();
    for line in items.iter().skip(1).flat_map(|item| item.lines()) {
        let line = line.trim();
        let Some((field, rest)) = line.split_once('[') else {
            continue;
        };
        for formatter in rest.trim_end_matches(']').split(',') {
            let formatter = formatter.trim();
            // `identity` is JabRef's no-op placeholder (e.g. the default
            // `all-text-fields[identity]`); it changes nothing.
            if !formatter.is_empty() && formatter != "identity" {
                pairs.insert((field.trim().to_lowercase(), formatter.to_string()));
            }
        }
    }
    let mut flags = BTreeMap::new();
    for (path, formatter, fields) in JABREF_SAVE_ACTIONS {
        let on = enabled
            && fields
                .list()
                .iter()
                .all(|field| pairs.contains(&(field.to_string(), formatter.to_string())));
        flags.insert(path.to_string(), on);
    }
    let exact = if enabled {
        save_action_pairs(&flags) == pairs
    } else {
        pairs.is_empty()
    };
    SaveActionsState { flags, exact }
}

/// The (field, formatter) pairs a set of bibtui save actions corresponds to.
fn save_action_pairs(flags: &BTreeMap<String, bool>) -> BTreeSet<(String, String)> {
    let mut pairs = BTreeSet::new();
    for (path, formatter, fields) in JABREF_SAVE_ACTIONS {
        if flags.get(*path).copied().unwrap_or(false) {
            for field in fields.list() {
                pairs.insert((field.to_string(), formatter.to_string()));
            }
        }
    }
    pairs
}

// ── Writing library settings ────────────────────────────────────────────────

/// One metadata comment to add, replace, or (with `comment: None`) remove.
#[derive(Debug, Clone, PartialEq)]
pub struct MetaChange {
    /// Metadata key, e.g. `bibtui.save.sync_filenames` or `keypattern_article`.
    pub key: String,
    /// Complete `@Comment{…}` text, or `None` to remove the key.
    pub comment: Option<String>,
    /// One-line description for previews.
    pub summary: String,
}

/// The `@Comment` for a `bibtui.*` setting, in the layout JabRef itself uses
/// for unknown metadata so files saved by either program stay identical.
pub fn bibtui_comment(path: &str, value: &Value) -> String {
    format!(
        "@Comment{{jabref-meta: {BIBTUI_PREFIX}{path}:\n{};\n}}",
        encode_value(value)
    )
}

fn keypattern_comment(type_name: &str, pattern: &str) -> String {
    format!(
        "@Comment{{jabref-meta: keypattern_{type_name}:{};}}",
        escape_meta(pattern)
    )
}

fn save_order_comment(order: &str) -> String {
    format!(
        "@Comment{{jabref-meta: saveOrderConfig:{}}}",
        save_order_value(order)
    )
}

fn save_actions_comment(flags: &BTreeMap<String, bool>) -> String {
    let pairs = save_action_pairs(flags);
    if pairs.is_empty() {
        return "@Comment{jabref-meta: saveActions:disabled;\n;}".to_string();
    }
    // One line per field, formatters in bibtui's application order.
    let mut by_field: BTreeMap<String, Vec<&str>> = BTreeMap::new();
    for (path, formatter, fields) in JABREF_SAVE_ACTIONS {
        if flags.get(*path).copied().unwrap_or(false) {
            for field in fields.list() {
                by_field
                    .entry(field.to_string())
                    .or_default()
                    .push(formatter);
            }
        }
    }
    let lines: String = by_field
        .iter()
        .map(|(field, formatters)| format!("{field}[{}]\n", formatters.join(",")))
        .collect();
    format!("@Comment{{jabref-meta: saveActions:enabled;\n{lines};}}")
}

fn display(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Metadata changes that store `paths` of `effective` in the library.
/// Returns the changes and notes about settings that could not be written.
pub fn changes_for_paths(
    effective: &Config,
    library: &LibrarySettings,
    paths: &BTreeSet<String>,
) -> (Vec<MetaChange>, Vec<String>) {
    let values = flatten(effective);
    let mut changes = Vec::new();
    let mut notes = Vec::new();
    let mut save_actions = false;
    for path in paths {
        let Some(value) = values.get(path) else {
            continue;
        };
        if EXCLUDED.contains(&path.as_str()) {
            continue;
        }
        if let Some(type_name) = path.strip_prefix(CITEKEY_TEMPLATES) {
            let pattern = display(value);
            if library.key_patterns.get(type_name) != Some(&pattern) {
                changes.push(MetaChange {
                    key: format!("keypattern_{type_name}"),
                    comment: Some(keypattern_comment(type_name, &pattern)),
                    summary: format!("keypattern_{type_name} = {pattern}"),
                });
            }
        } else if path == ENTRY_SORT_ORDER {
            if library.save_order_unrepresentable {
                notes.push(
                    "saveOrderConfig left unchanged: JabRef's current order has no bibtui equivalent"
                        .into(),
                );
                continue;
            }
            let order = display(value);
            if library.entry_sort_order.as_deref() != Some(order.as_str()) {
                changes.push(MetaChange {
                    key: "saveOrderConfig".into(),
                    comment: Some(save_order_comment(&order)),
                    summary: format!("saveOrderConfig = {}", save_order_value(&order)),
                });
            }
        } else if is_jabref_save_action(path) {
            save_actions = true;
        } else if library.values.get(path) != Some(value) {
            changes.push(MetaChange {
                key: format!("{BIBTUI_PREFIX}{path}"),
                comment: Some(bibtui_comment(path, value)),
                summary: format!("{BIBTUI_PREFIX}{path} = {}", display(value)),
            });
        }
    }
    if save_actions {
        let flags: BTreeMap<String, bool> = JABREF_SAVE_ACTIONS
            .iter()
            .map(|(path, _, _)| {
                let on = values.get(*path).and_then(Value::as_bool).unwrap_or(false);
                (path.to_string(), on)
            })
            .collect();
        match &library.save_actions {
            Some(current) if !current.exact => notes.push(
                "saveActions left unchanged: JabRef's configuration uses formatters bibtui does not model"
                    .into(),
            ),
            Some(current) if current.flags == flags => {}
            _ => {
                let enabled: Vec<&str> = JABREF_SAVE_ACTIONS
                    .iter()
                    .filter(|(path, _, _)| flags[*path])
                    .map(|(_, formatter, _)| *formatter)
                    .collect();
                changes.push(MetaChange {
                    key: "saveActions".into(),
                    comment: Some(save_actions_comment(&flags)),
                    summary: if enabled.is_empty() {
                        "saveActions = disabled".into()
                    } else {
                        format!("saveActions = {}", enabled.join(", "))
                    },
                });
            }
        }
    }
    (changes, notes)
}

/// Plan writing the effective settings into the library. A setting is
/// written when it differs from bibtui's built-in default, or when the
/// library already overrides it; `bibtui.*` keys that are no longer needed
/// are removed. JabRef's own keys are updated but never removed.
pub fn plan_library_export(
    effective: &Config,
    base: &Config,
    library: &LibrarySettings,
) -> (Vec<MetaChange>, Vec<String>) {
    let values = flatten(effective);
    let base_values = flatten(base);
    let defaults = flatten(&Config::default());
    let overridden = library.overrides();
    let wanted: BTreeSet<String> = values
        .iter()
        .filter(|(path, value)| {
            !EXCLUDED.contains(&path.as_str())
                && (defaults.get(*path) != Some(*value)
                    || base_values.get(*path) != Some(*value)
                    || overridden.contains_key(*path))
        })
        .map(|(path, _)| path.clone())
        .collect();
    let (mut changes, notes) = changes_for_paths(effective, library, &wanted);
    for key in &library.bibtui_keys {
        let path = &key[BIBTUI_PREFIX.len()..];
        let stored_natively = path.starts_with(CITEKEY_TEMPLATES)
            || path == ENTRY_SORT_ORDER
            || is_jabref_save_action(path);
        if !wanted.contains(path) || stored_natively || !library.values.contains_key(path) {
            changes.push(MetaChange {
                key: key.clone(),
                comment: None,
                summary: format!("remove {key}"),
            });
        }
    }
    (changes, notes)
}

/// Remove every `bibtui.*` key. JabRef's own keys are left alone.
pub fn plan_library_clear(library: &LibrarySettings) -> Vec<MetaChange> {
    library
        .bibtui_keys
        .iter()
        .map(|key| MetaChange {
            key: key.clone(),
            comment: None,
            summary: format!("remove {key}"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bib::jabref::parse_jabref_comment;
    use pretty_assertions::assert_eq;
    use serde_json::json;

    fn meta_from(comments: &[&str]) -> JabRefMeta {
        let mut meta = JabRefMeta::default();
        for comment in comments {
            parse_jabref_comment(comment, &mut meta);
        }
        meta
    }

    #[test]
    fn encoded_values_round_trip_including_awkward_characters() {
        for value in [
            json!(true),
            json!(42),
            json!("IEEEtranN"),
            json!("a;b"),
            json!("C:\\papers\\x"),
            json!("100% {braced} \"quoted\"\nline"),
            json!(["a", "an", "the"]),
            json!([{"name": "Identifiers", "fields": ["isbn", "issn"]}]),
            json!({"normal": {"ctrl-d": "DeleteEntry"}}),
        ] {
            let encoded = encode_value(&value);
            assert!(!encoded.contains('\n'), "{encoded}");
            // No backslash except JabRef's own `\;` separator escape.
            assert!(!encoded.replace("\\;", "").contains('\\'), "{encoded}");
            // Braces only appear structurally, so the @Comment stays balanced.
            assert_eq!(
                encoded.matches('{').count(),
                encoded.matches('}').count(),
                "{encoded}"
            );
            assert_eq!(decode_value(&encoded).unwrap(), value, "{encoded}");
        }
    }

    #[test]
    fn values_survive_jabref_rewriting_its_unknown_metadata() {
        // JabRef 5.15 rewrote these comments in this layout (see fixtures).
        let comment = "@Comment{jabref-meta: bibtui.test.semicolon:\n\"a\\;b\";\n}";
        let meta = meta_from(&[comment]);
        let settings = LibrarySettings::from_meta(&meta);
        assert_eq!(settings.values["test.semicolon"], json!("a;b"));
        let written = bibtui_comment("test.semicolon", &json!("a;b"));
        assert_eq!(written, comment);
    }

    #[test]
    fn invalid_values_warn_and_are_still_tracked_for_removal() {
        let meta = meta_from(&[
            "@Comment{jabref-meta: bibtui.save.sync_filenames:yes please;}",
            "@Comment{jabref-meta: bibtui.titlecase.stop_words:[\"a\", ;}",
            "@Comment{jabref-meta: bibtui.save.field_order:alphabetical;}",
        ]);
        let settings = LibrarySettings::from_meta(&meta);
        // Malformed JSON is rejected when read; a bare word is a string.
        assert_eq!(settings.warnings.len(), 1, "{:?}", settings.warnings);
        assert!(settings.warnings[0].contains("titlecase.stop_words"));
        assert_eq!(settings.values["save.field_order"], json!("alphabetical"));
        // A value of the wrong type is rejected when applied.
        let (config, warnings) = apply_overrides(&Config::default(), &settings.overrides());
        assert_eq!(config.save.field_order, "alphabetical");
        assert!(!config.save.sync_filenames);
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(warnings[0].contains("save.sync_filenames"));
        assert_eq!(plan_library_clear(&settings).len(), 3);
    }

    #[test]
    fn overrides_apply_individually_and_report_bad_ones() {
        let base = Config::default();
        let overrides = BTreeMap::from([
            ("save.sync_filenames".to_string(), json!(true)),
            ("save.align_fields".to_string(), json!("not a bool")),
            ("save.no_such_setting".to_string(), json!(1)),
            ("general.bib_file".to_string(), json!("elsewhere.bib")),
            (
                "citekey.templates.article".to_string(),
                json!("[auth][year]"),
            ),
        ]);
        let (config, warnings) = apply_overrides(&base, &overrides);
        assert!(config.save.sync_filenames);
        assert_eq!(config.save.align_fields, base.save.align_fields);
        assert_eq!(config.general.bib_file, base.general.bib_file);
        assert_eq!(config.citekey.templates["article"], "[auth][year]");
        assert_eq!(warnings.len(), 3, "{warnings:?}");
    }

    #[test]
    fn jabref_keys_map_to_settings() {
        let meta = meta_from(&[
            "@Comment{jabref-meta: keypattern_article:[auth]\\;[year];}",
            "@Comment{jabref-meta: saveOrderConfig:original;}",
            "@Comment{jabref-meta: saveActions:enabled;\ndate[normalize_date]\npages[normalize_page_numbers]\n;}",
        ]);
        let settings = LibrarySettings::from_meta(&meta);
        let overrides = settings.overrides();
        assert_eq!(
            overrides["citekey.templates.article"],
            json!("[auth];[year]")
        );
        assert_eq!(overrides["save.entry_sort_order"], json!("none"));
        assert_eq!(overrides["save.save_action_normalize_date"], json!(true));
        assert_eq!(
            overrides["save.save_action_normalize_page_numbers"],
            json!(true)
        );
        assert_eq!(overrides["save.save_action_latex_cleanup"], json!(false));
        assert!(settings.save_actions.as_ref().unwrap().exact);
        assert!(settings.warnings.is_empty(), "{:?}", settings.warnings);
    }

    #[test]
    fn jabref_identity_placeholder_does_not_block_rewriting() {
        // JabRef's default save actions for a new library.
        let meta = meta_from(&[
            "@Comment{jabref-meta: saveActions:enabled;\nall-text-fields[identity]\ndate[normalize_date]\nmonth[normalize_month]\npages[normalize_page_numbers]\n;}",
            "@Comment{jabref-meta: saveOrderConfig:original;citationkey;false;citationkey;false;citationkey;false;}",
        ]);
        let settings = LibrarySettings::from_meta(&meta);
        assert!(settings.warnings.is_empty(), "{:?}", settings.warnings);
        let state = settings.save_actions.unwrap();
        assert!(state.exact);
        assert!(state.flags["save.save_action_normalize_month"]);
        assert!(!state.flags["save.save_action_latex_cleanup"]);
        assert_eq!(settings.entry_sort_order.as_deref(), Some("none"));
    }

    #[test]
    fn unrepresentable_jabref_settings_are_reported_and_never_rewritten() {
        let meta = meta_from(&[
            "@Comment{jabref-meta: saveOrderConfig:specified;author;false;year;true;}",
            "@Comment{jabref-meta: saveActions:enabled;\ntitle[latex_cleanup,title_case]\n;}",
        ]);
        let settings = LibrarySettings::from_meta(&meta);
        assert!(settings.save_order_unrepresentable);
        assert!(!settings.save_actions.as_ref().unwrap().exact);
        assert_eq!(settings.warnings.len(), 2);
        let mut effective = Config::default();
        effective.save.entry_sort_order = "none".into();
        effective.save.save_action_latex_cleanup = false;
        let paths = BTreeSet::from([
            "save.entry_sort_order".to_string(),
            "save.save_action_latex_cleanup".to_string(),
        ]);
        let (changes, notes) = changes_for_paths(&effective, &settings, &paths);
        assert!(changes.is_empty(), "{changes:?}");
        assert_eq!(notes.len(), 2);
    }

    #[test]
    fn written_save_actions_read_back_identically() {
        let mut effective = Config::default();
        effective.save.save_action_latex_cleanup = false;
        effective.save.save_action_normalize_month = false;
        let paths = BTreeSet::from(["save.save_action_latex_cleanup".to_string()]);
        let (changes, _) = changes_for_paths(&effective, &LibrarySettings::default(), &paths);
        assert_eq!(changes.len(), 1);
        let comment = changes[0].comment.clone().unwrap();
        assert!(comment.starts_with("@Comment{jabref-meta: saveActions:enabled;\n"));
        assert!(comment.contains("date[normalize_date]\n"));
        assert!(!comment.contains("month["));
        let settings = LibrarySettings::from_meta(&meta_from(&[&comment]));
        let state = settings.save_actions.unwrap();
        assert!(state.exact);
        assert!(!state.flags["save.save_action_latex_cleanup"]);
        assert!(!state.flags["save.save_action_normalize_month"]);
        assert!(state.flags["save.save_action_normalize_date"]);
    }

    #[test]
    fn export_writes_differences_keeps_overrides_and_drops_invalid_keys() {
        let library = LibrarySettings::from_meta(&meta_from(&[
            "@Comment{jabref-meta: bibtui.save.field_order:\"alphabetical\";}",
            "@Comment{jabref-meta: bibtui.save.align_fields:[1,;}",
        ]));
        let (mut effective, _) = apply_overrides(&Config::default(), &library.overrides());
        effective.save.sync_filenames = true;
        effective
            .citekey
            .templates
            .insert("article".into(), "[auth][year]".into());
        effective.general.bib_file = Some("x.bib".into());
        let (changes, _) = plan_library_export(&effective, &Config::default(), &library);
        let keys: Vec<_> = changes
            .iter()
            .map(|c| (c.key.as_str(), c.comment.is_some()))
            .collect();
        assert_eq!(
            keys,
            [
                ("keypattern_article", true),
                ("bibtui.save.sync_filenames", true),
                ("bibtui.save.align_fields", false),
            ]
        );
    }

    #[test]
    fn export_round_trips_to_the_same_effective_config() {
        let mut effective = Config::default();
        effective.save.sync_filenames = true;
        effective.save.save_action_latex_cleanup = false;
        effective.save.entry_sort_order = "none".into();
        effective.titlecase.stop_words = vec!["a".into(), "of".into()];
        effective
            .citekey
            .templates
            .insert("book".into(), "Book_[auth]".into());
        let (changes, _) =
            plan_library_export(&effective, &Config::default(), &LibrarySettings::default());
        let comments: Vec<String> = changes.iter().filter_map(|c| c.comment.clone()).collect();
        let refs: Vec<&str> = comments.iter().map(String::as_str).collect();
        let library = LibrarySettings::from_meta(&meta_from(&refs));
        assert!(library.warnings.is_empty(), "{:?}", library.warnings);
        let (reloaded, warnings) = apply_overrides(&Config::default(), &library.overrides());
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(config_value(&reloaded), config_value(&effective));
        // Exporting again is a no-op.
        let (again, _) = plan_library_export(&reloaded, &Config::default(), &library);
        assert!(again.is_empty(), "{again:?}");
    }
}
