//! Yank-to-clipboard choices and the usage counts that order the `yy` picker.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// One thing `yy` can copy to the clipboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum YankChoice {
    Formatted,
    Bibtex,
    File,
    CitationKey,
}

impl YankChoice {
    /// Picker order before any usage has been recorded; also the tie-break
    /// order between choices with equal counts.
    pub const DEFAULT_ORDER: [YankChoice; 4] = [
        YankChoice::Formatted,
        YankChoice::Bibtex,
        YankChoice::File,
        YankChoice::CitationKey,
    ];

    /// The `yank_format` config value that selects this choice directly.
    pub fn format_id(self) -> &'static str {
        match self {
            YankChoice::Formatted => "formatted",
            YankChoice::Bibtex => "bibtex",
            YankChoice::File => "file",
            YankChoice::CitationKey => "citation_key",
        }
    }

    /// Key that picks this choice in the `yy` picker.
    pub fn hotkey(self) -> char {
        match self {
            YankChoice::Formatted => 'f',
            YankChoice::Bibtex => 'b',
            YankChoice::File => 'a',
            YankChoice::CitationKey => 'c',
        }
    }

    /// Picker label; `style` is the citation style name shown for `Formatted`.
    pub fn label(self, style: &str) -> String {
        match self {
            YankChoice::Formatted => format!("Formatted citation ({})", style),
            YankChoice::Bibtex => "BibTeX entry".to_string(),
            YankChoice::File => "Associated File(s)".to_string(),
            YankChoice::CitationKey => "Citation key".to_string(),
        }
    }
}

/// How many times each yank choice has been picked from the `yy` prompt.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct YankUsage {
    /// Keyed by `YankChoice::format_id`.
    pub counts: BTreeMap<String, u64>,
}

impl YankUsage {
    pub fn count(&self, choice: YankChoice) -> u64 {
        self.counts.get(choice.format_id()).copied().unwrap_or(0)
    }

    pub fn record(&mut self, choice: YankChoice) {
        *self
            .counts
            .entry(choice.format_id().to_string())
            .or_insert(0) += 1;
    }

    /// Choices ordered most-used first; ties keep `DEFAULT_ORDER`.
    pub fn ordered_choices(&self) -> Vec<YankChoice> {
        let mut choices = YankChoice::DEFAULT_ORDER.to_vec();
        // sort_by_key is stable, so equal counts keep their default order.
        choices.sort_by_key(|c| std::cmp::Reverse(self.count(*c)));
        choices
    }

    /// Load counts from `path`; a missing or unreadable file yields zero counts.
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_yaml::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, serde_yaml::to_string(self)?)?;
        Ok(())
    }
}

/// Where usage counts persist: `$XDG_STATE_HOME/bibtui/usage.yaml` on Linux,
/// the platform's local data directory elsewhere.
pub fn default_usage_path() -> Option<PathBuf> {
    dirs::state_dir()
        .or_else(dirs::data_local_dir)
        .map(|d| d.join("bibtui").join("usage.yaml"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_default_order_without_usage() {
        assert_eq!(
            YankUsage::default().ordered_choices(),
            YankChoice::DEFAULT_ORDER.to_vec()
        );
    }

    #[test]
    fn test_order_by_count_with_stable_ties() {
        let mut usage = YankUsage::default();
        usage.record(YankChoice::CitationKey);
        usage.record(YankChoice::CitationKey);
        usage.record(YankChoice::File);
        assert_eq!(
            usage.ordered_choices(),
            vec![
                YankChoice::CitationKey,
                YankChoice::File,
                YankChoice::Formatted,
                YankChoice::Bibtex,
            ]
        );
    }

    #[test]
    fn test_save_and_load_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("nested").join("usage.yaml");
        let mut usage = YankUsage::default();
        usage.record(YankChoice::Bibtex);
        usage.save(&path).unwrap();
        assert_eq!(YankUsage::load(&path), usage);
    }

    #[test]
    fn test_load_missing_or_garbage_is_empty() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(
            YankUsage::load(&tmp.path().join("nope.yaml")),
            YankUsage::default()
        );
        let bad = tmp.path().join("bad.yaml");
        std::fs::write(&bad, ": : :\n[").unwrap();
        assert_eq!(YankUsage::load(&bad), YankUsage::default());
    }

    #[test]
    fn test_hotkeys_are_unique() {
        let mut keys: Vec<char> = YankChoice::DEFAULT_ORDER
            .iter()
            .map(|c| c.hotkey())
            .collect();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), 4);
    }
}
