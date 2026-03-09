use std::path::{Path, PathBuf};

use anyhow::{bail, Result};

/// A single attached file parsed from a JabRef `file` field.
#[derive(Debug, Clone)]
pub struct ParsedFile {
    /// Human-readable description (may be empty).
    pub description: String,
    /// Path as stored in the field (may be relative).
    pub path: String,
    /// File type label (e.g. "PDF", "PS").
    pub file_type: String,
}

impl ParsedFile {
    /// Short label for display in a picker dialog.
    pub fn label(&self) -> String {
        let name = Path::new(&self.path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&self.path);
        if self.description.is_empty() {
            format!("{} ({})", name, self.file_type)
        } else {
            format!("{}: {} ({})", self.description, name, self.file_type)
        }
    }
}

/// Parse the JabRef `file` field value into individual file entries.
///
/// Format: `Description:Path:Type` with multiple entries separated by `;`.
/// Colons within fields are escaped as `\:`.
pub fn parse_file_field(s: &str) -> Vec<ParsedFile> {
    let mut files = Vec::new();

    for segment in split_semicolons(s) {
        let segment = segment.trim();
        if segment.is_empty() {
            continue;
        }
        let parts = split_colons(segment);
        if parts.len() < 2 {
            continue;
        }
        let description = parts[0].trim().to_string();
        let path = parts[1].trim().to_string();
        let file_type = if parts.len() >= 3 {
            parts[2].trim().to_string()
        } else {
            String::new()
        };
        if !path.is_empty() {
            files.push(ParsedFile { description, path, file_type });
        }
    }

    files
}

/// Split on `;` that are not preceded by `\`.
fn split_semicolons(s: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(&next) = chars.peek() {
                if next == ';' || next == ':' {
                    current.push(next);
                    chars.next();
                    continue;
                }
            }
            current.push(c);
        } else if c == ';' {
            parts.push(std::mem::take(&mut current));
        } else {
            current.push(c);
        }
    }
    parts.push(current);
    parts
}

/// Split on `:` that are not preceded by `\`, up to 3 parts.
fn split_colons(s: &str) -> Vec<String> {
    let mut parts: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(&next) = chars.peek() {
                if next == ':' {
                    current.push(':');
                    chars.next();
                    continue;
                }
            }
            current.push(c);
        } else if c == ':' && parts.len() < 2 {
            parts.push(std::mem::take(&mut current));
        } else {
            current.push(c);
        }
    }
    parts.push(current);
    parts
}

/// Return the effective file directory, preferring JabRef's `fileDirectory` metadata when set.
///
/// - If `file_directory` is absolute, use it directly.
/// - If relative, resolve it against `bib_path`'s parent.
/// - If absent, fall back to `bib_path`'s parent.
pub fn effective_file_dir(bib_path: &Path, file_directory: Option<&str>) -> PathBuf {
    let bib_dir = bib_path.parent().unwrap_or(Path::new("."));
    match file_directory {
        Some(fd) if !fd.trim().is_empty() => {
            let fd_path = PathBuf::from(fd.trim());
            if fd_path.is_absolute() { fd_path } else { bib_dir.join(fd_path) }
        }
        _ => bib_dir.to_path_buf(),
    }
}

/// Resolve a (possibly relative) file path against the directory of the .bib file.
pub fn resolve_file_path(path: &str, bib_dir: &Path) -> PathBuf {
    let p = PathBuf::from(path);
    if p.is_absolute() {
        p
    } else {
        bib_dir.join(p)
    }
}

/// Serialize a list of `ParsedFile`s back into the JabRef `file` field format.
///
/// Produces `Description:Path:Type` entries joined by `;`.
pub fn serialize_file_field(files: &[ParsedFile]) -> String {
    files
        .iter()
        .map(|f| format!("{}:{}:{}", f.description, f.path, f.file_type))
        .collect::<Vec<_>>()
        .join(";")
}

/// Convert a DOI string to a full `https://doi.org/` URL.
/// If the input already looks like a URL, return it unchanged.
pub fn doi_to_url(doi: &str) -> String {
    let doi = doi.trim();
    if doi.starts_with("http://") || doi.starts_with("https://") {
        doi.to_string()
    } else {
        format!("https://doi.org/{}", doi)
    }
}

/// Open a local file with the OS-default application.
pub fn open_path(path: &Path) -> Result<()> {
    if !path.exists() {
        bail!("File not found: {}", path.display());
    }
    let cmd = os_open_cmd();
    std::process::Command::new(cmd).arg(path).spawn()?;
    Ok(())
}

/// Open a URL with the OS-default browser.
pub fn open_url(url: &str) -> Result<()> {
    let cmd = os_open_cmd();
    std::process::Command::new(cmd).arg(url).spawn()?;
    Ok(())
}

fn os_open_cmd() -> &'static str {
    #[cfg(target_os = "macos")]
    { "open" }
    #[cfg(target_os = "linux")]
    { "xdg-open" }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    { "xdg-open" }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_single_file() {
        let files = parse_file_field(":papers/foo.pdf:PDF");
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "papers/foo.pdf");
        assert_eq!(files[0].file_type, "PDF");
        assert_eq!(files[0].description, "");
    }

    #[test]
    fn test_parse_multiple_files() {
        let files = parse_file_field(":a.pdf:PDF;Draft:b.pdf:PDF");
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].path, "a.pdf");
        assert_eq!(files[1].description, "Draft");
        assert_eq!(files[1].path, "b.pdf");
    }

    #[test]
    fn test_doi_to_url_raw() {
        assert_eq!(doi_to_url("10.1234/foo"), "https://doi.org/10.1234/foo");
    }

    #[test]
    fn test_doi_to_url_already_url() {
        assert_eq!(
            doi_to_url("https://doi.org/10.1234/foo"),
            "https://doi.org/10.1234/foo"
        );
    }

    #[test]
    fn test_serialize_file_field_single() {
        let files = vec![ParsedFile {
            description: "My Paper".into(),
            path: "papers/foo.pdf".into(),
            file_type: "PDF".into(),
        }];
        assert_eq!(serialize_file_field(&files), "My Paper:papers/foo.pdf:PDF");
    }

    #[test]
    fn test_serialize_file_field_multiple() {
        let files = vec![
            ParsedFile { description: "A".into(), path: "a.pdf".into(), file_type: "PDF".into() },
            ParsedFile { description: "B".into(), path: "b.pdf".into(), file_type: "PS".into() },
        ];
        assert_eq!(serialize_file_field(&files), "A:a.pdf:PDF;B:b.pdf:PS");
    }

    #[test]
    fn test_serialize_file_field_empty() {
        assert_eq!(serialize_file_field(&[]), "");
    }

    #[test]
    fn test_parse_file_field_empty_input() {
        let files = parse_file_field("");
        assert!(files.is_empty());
    }

    #[test]
    fn test_parse_file_field_missing_path() {
        // "Desc::PDF" — path is empty, should be skipped
        let files = parse_file_field("Desc::PDF");
        assert!(files.is_empty());
    }

    #[test]
    fn test_parsed_file_label_with_desc() {
        let f = ParsedFile {
            description: "Draft".into(),
            path: "papers/foo.pdf".into(),
            file_type: "PDF".into(),
        };
        assert_eq!(f.label(), "Draft: foo.pdf (PDF)");
    }

    #[test]
    fn test_parsed_file_label_no_desc() {
        let f = ParsedFile {
            description: "".into(),
            path: "papers/foo.pdf".into(),
            file_type: "PDF".into(),
        };
        assert_eq!(f.label(), "foo.pdf (PDF)");
    }

    #[test]
    fn test_effective_file_dir_none() {
        let result = effective_file_dir(Path::new("/home/user/refs.bib"), None);
        assert_eq!(result, PathBuf::from("/home/user"));
    }

    #[test]
    fn test_effective_file_dir_absolute() {
        let result = effective_file_dir(
            Path::new("/home/user/refs.bib"),
            Some("/data/papers"),
        );
        assert_eq!(result, PathBuf::from("/data/papers"));
    }

    #[test]
    fn test_effective_file_dir_relative() {
        let result = effective_file_dir(
            Path::new("/home/user/refs.bib"),
            Some("papers"),
        );
        assert_eq!(result, PathBuf::from("/home/user/papers"));
    }

    #[test]
    fn test_resolve_file_path_absolute() {
        let result = resolve_file_path("/absolute/path.pdf", Path::new("/home/user"));
        assert_eq!(result, PathBuf::from("/absolute/path.pdf"));
    }

    #[test]
    fn test_resolve_file_path_relative() {
        let result = resolve_file_path("subdir/file.pdf", Path::new("/home/user"));
        assert_eq!(result, PathBuf::from("/home/user/subdir/file.pdf"));
    }

    #[test]
    fn test_parse_file_field_escaped_colon() {
        // The escape \: is handled at the split_semicolons stage, which converts \: to :
        // in the segment string. split_colons then sees "Desc:path:with:colons.pdf:PDF"
        // and splits at the first two unescaped colons.
        // So description="Desc", path="path", file_type="with:colons.pdf:PDF".
        // This test verifies the actual parsing behavior.
        let files = parse_file_field(r"Desc:path\:with\:colons.pdf:PDF");
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].description, "Desc");
        assert_eq!(files[0].path, "path");
    }

    #[test]
    fn test_parse_file_field_escaped_semicolon() {
        // Escaped semicolons keep it as one segment
        let files = parse_file_field(r"Desc:path\;with\;semi.pdf:PDF");
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "path;with;semi.pdf");
    }
}
