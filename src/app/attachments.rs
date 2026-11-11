//! Which entries own which attachment files. Deleting or renaming a file
//! referenced by more than one entry would break the other entries, so
//! deletion, manual sync and save-time sync all consult the same ownership
//! map.

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use super::App;
use crate::util::open::{effective_file_dir, parse_file_field, resolve_file_path};

/// The identity of a file on disk. On Unix this is the device and inode, so
/// symlinks, hard links and differently spelled paths to one file compare
/// equal; elsewhere the canonical path is used.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(super) enum FileId {
    #[cfg(unix)]
    Inode(u64, u64),
    Path(PathBuf),
}

impl FileId {
    pub(super) fn of(path: &Path) -> FileId {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if let Ok(meta) = std::fs::metadata(path) {
                return FileId::Inode(meta.dev(), meta.ino());
            }
        }
        FileId::Path(path.canonicalize().unwrap_or_else(|_| path.to_path_buf()))
    }
}

impl App {
    /// Absolute paths of the files listed in an entry's `file` field.
    pub(super) fn entry_attachment_paths(&self, key: &str) -> Vec<PathBuf> {
        let Some(value) = self
            .database
            .entries
            .get(key)
            .and_then(|e| e.fields.get("file"))
        else {
            return Vec::new();
        };
        let file_dir = effective_file_dir(
            &self.bib_path,
            self.database.jabref_meta.file_directory.as_deref(),
        );
        parse_file_field(value)
            .into_iter()
            .map(|f| resolve_file_path(&f.path, &file_dir))
            .collect()
    }

    /// Every attachment in the library, mapped to the keys of the entries
    /// that reference it.
    pub(super) fn attachment_owners(&self) -> HashMap<FileId, BTreeSet<String>> {
        let mut owners: HashMap<FileId, BTreeSet<String>> = HashMap::new();
        for key in self.database.entries.keys() {
            for path in self.entry_attachment_paths(key) {
                owners
                    .entry(FileId::of(&path))
                    .or_default()
                    .insert(key.clone());
            }
        }
        owners
    }

    /// Entries other than `key` that reference the file at `path`.
    pub(super) fn other_attachment_owners(
        &self,
        owners: &HashMap<FileId, BTreeSet<String>>,
        key: &str,
        path: &Path,
    ) -> Vec<String> {
        owners
            .get(&FileId::of(path))
            .map(|keys| keys.iter().filter(|k| *k != key).cloned().collect())
            .unwrap_or_default()
    }
}

/// "B" / "B and C" / "B, C and 2 more" for a status line or dialog label.
pub(super) fn describe_owners(keys: &[String]) -> String {
    match keys {
        [] => String::new(),
        [one] => one.clone(),
        [a, b] => format!("{a} and {b}"),
        [a, b, rest @ ..] => format!("{a}, {b} and {} more", rest.len()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describe_owners_lists_up_to_two_names() {
        assert_eq!(describe_owners(&[]), "");
        assert_eq!(describe_owners(&["B".into()]), "B");
        assert_eq!(describe_owners(&["B".into(), "C".into()]), "B and C");
        assert_eq!(
            describe_owners(&["B".into(), "C".into(), "D".into(), "E".into()]),
            "B, C and 2 more"
        );
    }

    #[cfg(unix)]
    #[test]
    fn file_id_matches_links_and_spellings_of_one_file() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.pdf");
        std::fs::write(&file, "x").unwrap();
        let hard = dir.path().join("hard.pdf");
        std::fs::hard_link(&file, &hard).unwrap();
        let soft = dir.path().join("soft.pdf");
        std::os::unix::fs::symlink(&file, &soft).unwrap();
        let dotted = dir.path().join(".").join("a.pdf");
        let id = FileId::of(&file);
        for alias in [&hard, &soft, &dotted] {
            assert_eq!(FileId::of(alias), id, "{alias:?}");
        }
        let other = dir.path().join("b.pdf");
        std::fs::write(&other, "x").unwrap();
        assert_ne!(FileId::of(&other), id);
    }
}
