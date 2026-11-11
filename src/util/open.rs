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
///
/// Values not written by JabRef commonly hold a plain path such as
/// `file = {paper.pdf}`. When the whole value contains no unescaped `:` and
/// no unescaped `;`, it is treated as one bare path with an empty
/// description and a type derived from the file extension.
pub fn parse_file_field(s: &str) -> Vec<ParsedFile> {
    let trimmed = s.trim();
    if !trimmed.is_empty() && !has_unescaped_separator(trimmed) {
        // split_colons never splits here (no unescaped colons); it just
        // unescapes `\\`, `\;`, and `\:` in the single segment.
        let path = split_colons(trimmed).into_iter().next().unwrap_or_default();
        let file_type = Path::new(&path)
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e.to_uppercase())
            .unwrap_or_default();
        return vec![ParsedFile {
            description: String::new(),
            path,
            file_type,
        }];
    }

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
            files.push(ParsedFile {
                description,
                path,
                file_type,
            });
        }
    }

    files
}

/// True if `s` contains a `:` or `;` not escaped by a backslash.
fn has_unescaped_separator(s: &str) -> bool {
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            chars.next();
        } else if c == ':' || c == ';' {
            return true;
        }
    }
    false
}

/// Split on `;` that are not escaped by `\`.
///
/// Escape sequences (`\\`, `\;`, `\:`) are preserved verbatim so the
/// colon-splitting stage can unescape them in the right context.
fn split_semicolons(s: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            current.push(c);
            if let Some(&next) = chars.peek() {
                if next == '\\' || next == ';' || next == ':' {
                    current.push(next);
                    chars.next();
                }
            }
        } else if c == ';' {
            parts.push(std::mem::take(&mut current));
        } else {
            current.push(c);
        }
    }
    parts.push(current);
    parts
}

/// Split on `:` that are not escaped by `\`, up to 3 parts, unescaping
/// `\\`, `\;`, and `\:` in the output.
fn split_colons(s: &str) -> Vec<String> {
    let mut parts: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.peek() {
                Some(&next) if next == '\\' || next == ';' || next == ':' => {
                    current.push(next);
                    chars.next();
                }
                _ => current.push(c),
            }
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
            if fd_path.is_absolute() {
                fd_path
            } else {
                bib_dir.join(fd_path)
            }
        }
        _ => bib_dir.to_path_buf(),
    }
}

/// Compute a relative path from `base_dir` to `target`.
///
/// Both paths are canonicalized first so symlinks and `..` components are
/// resolved.  Falls back to the absolute `target` path if canonicalization
/// fails (e.g. when the file does not yet exist on disk).
pub fn make_relative(base_dir: &Path, target: &Path) -> PathBuf {
    let base = base_dir
        .canonicalize()
        .unwrap_or_else(|_| base_dir.to_path_buf());
    let tgt = target
        .canonicalize()
        .unwrap_or_else(|_| target.to_path_buf());

    let base_parts: Vec<_> = base.components().collect();
    let tgt_parts: Vec<_> = tgt.components().collect();

    let common = base_parts
        .iter()
        .zip(tgt_parts.iter())
        .take_while(|(a, b)| a == b)
        .count();

    let ups = base_parts.len() - common;
    let mut rel = PathBuf::new();
    for _ in 0..ups {
        rel.push("..");
    }
    for part in &tgt_parts[common..] {
        rel.push(part);
    }

    if rel.as_os_str().is_empty() {
        // Same path — use the filename only
        tgt.file_name().map(PathBuf::from).unwrap_or(tgt)
    } else {
        rel
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

/// Escape a single `file` field component: `\` first, then `;` and `:`,
/// mirroring what `parse_file_field` unescapes.
fn escape_file_component(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str(r"\\"),
            ';' => out.push_str(r"\;"),
            ':' => out.push_str(r"\:"),
            _ => out.push(ch),
        }
    }
    out
}

/// Serialize a list of `ParsedFile`s back into the JabRef `file` field format.
///
/// Produces `Description:Path:Type` entries joined by `;`, escaping `\`,
/// `;`, and `:` within each component so the value round-trips through
/// `parse_file_field`.
pub fn serialize_file_field(files: &[ParsedFile]) -> String {
    files
        .iter()
        .map(|f| {
            format!(
                "{}:{}:{}",
                escape_file_component(&f.description),
                escape_file_component(&f.path),
                escape_file_component(&f.file_type)
            )
        })
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
    open_target(path.as_os_str())
}

/// Open a URL with the OS-default browser.
pub fn open_url(url: &str) -> Result<()> {
    open_target(std::ffi::OsStr::new(url))
}

/// Hand a file path or URL to the OS: `open` on macOS, `xdg-open` on
/// Linux, and the shell's `ShellExecuteW` on Windows. The target is passed
/// as one argument, never through a shell, so characters such as `&`, `;`
/// or spaces in a URL or file name are not interpreted.
fn open_target(target: &std::ffi::OsStr) -> Result<()> {
    #[cfg(windows)]
    {
        windows::shell_open(target)
    }
    #[cfg(unix)]
    {
        run_opener(opener_command(target), OPENER_WAIT)
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = target;
        bail!("Opening files is not supported on this platform")
    }
}

/// The opener program for this platform and a hint for when it is missing.
#[cfg_attr(not(any(unix, test)), allow(dead_code))]
fn os_open_cmd() -> (&'static str, &'static str) {
    if cfg!(target_os = "macos") {
        ("open", "")
    } else {
        ("xdg-open", " (install xdg-utils)")
    }
}

#[cfg_attr(not(any(unix, test)), allow(dead_code))]
fn opener_command(target: &std::ffi::OsStr) -> std::process::Command {
    let mut cmd = std::process::Command::new(os_open_cmd().0);
    cmd.arg(target);
    cmd
}

/// How long to wait for the opener to report a failure. `open` and most
/// `xdg-open` back ends hand the target to the application and exit at
/// once; some `xdg-open` fallbacks run the application in the foreground,
/// so one still running after this is assumed to have started it.
#[cfg_attr(not(unix), allow(dead_code))]
const OPENER_WAIT: std::time::Duration = std::time::Duration::from_secs(1);

/// Run an opener without letting it write over the terminal UI, and turn a
/// missing program or a quick nonzero exit into an error that carries the
/// opener's message (for example "no application knows how to open").
#[cfg_attr(not(any(unix, test)), allow(dead_code))]
fn run_opener(mut cmd: std::process::Command, wait: std::time::Duration) -> Result<()> {
    use std::io::{Read, Seek};
    use std::process::Stdio;
    let program = cmd.get_program().to_string_lossy().into_owned();
    let mut stderr_file = tempfile::tempfile()?;
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::from(stderr_file.try_clone()?))
        .spawn()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                let hint = if program == os_open_cmd().0 {
                    os_open_cmd().1
                } else {
                    ""
                };
                anyhow::anyhow!("{program} not found{hint}")
            } else {
                anyhow::anyhow!("could not run {program}: {e}")
            }
        })?;
    let deadline = std::time::Instant::now() + wait;
    loop {
        if let Some(status) = child.try_wait()? {
            if status.success() {
                return Ok(());
            }
            let mut bytes = Vec::new();
            stderr_file.rewind()?;
            let _ = stderr_file.read_to_end(&mut bytes);
            let stderr = String::from_utf8_lossy(&bytes);
            let stderr = stderr.trim();
            if stderr.is_empty() {
                bail!("{program} failed ({status})");
            }
            bail!("{program} failed ({status}): {stderr}");
        }
        if std::time::Instant::now() >= deadline {
            // Reap it whenever the application exits.
            std::thread::spawn(move || {
                let _ = child.wait();
            });
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

/// Windows: `ShellExecuteW` with the "open" verb, which is what Explorer
/// does on double-click. It opens files with their associated application
/// and URLs in the default browser, and reports failures with an error code
/// instead of an exit status.
#[cfg(windows)]
mod windows {
    use std::ffi::{c_void, OsStr};
    use std::os::windows::ffi::OsStrExt;

    #[link(name = "shell32")]
    extern "system" {
        fn ShellExecuteW(
            hwnd: *mut c_void,
            operation: *const u16,
            file: *const u16,
            parameters: *const u16,
            directory: *const u16,
            show_cmd: i32,
        ) -> *mut c_void;
    }

    #[link(name = "ole32")]
    extern "system" {
        fn CoInitializeEx(reserved: *mut c_void, co_init: u32) -> i32;
        fn CoUninitialize();
    }

    const SW_SHOWNORMAL: i32 = 1;
    const COINIT_APARTMENTTHREADED: u32 = 0x2;
    const COINIT_DISABLE_OLE1DDE: u32 = 0x4;

    fn wide(s: &OsStr) -> Vec<u16> {
        s.encode_wide().chain(std::iter::once(0)).collect()
    }

    pub(super) fn shell_open(target: &OsStr) -> anyhow::Result<()> {
        let file = wide(target);
        let verb = wide(OsStr::new("open"));
        // Shell extensions that handle some file types need COM, which
        // Microsoft says to initialize first; a thread of our own keeps
        // that apartment separate from the rest of the program.
        let code = std::thread::spawn(move || {
            // SAFETY: both strings are NUL-terminated UTF-16 buffers that
            // outlive the calls; null is allowed for the window, parameters
            // and directory; COM is uninitialized only if initialized here.
            unsafe {
                let hr = CoInitializeEx(
                    std::ptr::null_mut(),
                    COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE,
                );
                let result = ShellExecuteW(
                    std::ptr::null_mut(),
                    verb.as_ptr(),
                    file.as_ptr(),
                    std::ptr::null(),
                    std::ptr::null(),
                    SW_SHOWNORMAL,
                ) as isize;
                if hr >= 0 {
                    CoUninitialize();
                }
                result
            }
        })
        .join()
        .map_err(|_| anyhow::anyhow!("the Windows opener thread panicked"))?;
        if code > 32 {
            Ok(())
        } else {
            anyhow::bail!("{}", super::shell_execute_error(code))
        }
    }
}

/// Explain a `ShellExecuteW` failure code (any value of 32 or less).
#[cfg_attr(not(any(windows, test)), allow(dead_code))]
fn shell_execute_error(code: isize) -> String {
    let reason = match code {
        0 | 8 => "out of memory",
        2 => "file not found",
        3 => "path not found",
        5 => "access denied",
        11 => "invalid program file",
        26 => "sharing violation",
        27 => "incomplete file association",
        28..=30 => "the application did not respond",
        31 => "no application is associated with this file type",
        32 => "a required DLL was not found",
        _ => "unknown error",
    };
    format!("Windows could not open it: {reason} (error {code})")
}

/// Abstraction over "open with the OS-default application" so app logic
/// can be tested without launching real programs.
pub trait Opener {
    fn open_path(&self, path: &Path) -> Result<()>;
    fn open_url(&self, url: &str) -> Result<()>;
}

/// The real OS opener (`open` on macOS, `xdg-open` on Linux,
/// `ShellExecuteW` on Windows).
pub struct SystemOpener;

impl Opener for SystemOpener {
    fn open_path(&self, path: &Path) -> Result<()> {
        open_path(path)
    }
    fn open_url(&self, url: &str) -> Result<()> {
        open_url(url)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_opener_passes_target_as_one_argument() {
        let url = "https://example.org/a b?x=1&y=$(rm -rf ~);z";
        let cmd = opener_command(std::ffi::OsStr::new(url));
        assert_eq!(cmd.get_program(), os_open_cmd().0);
        let args: Vec<_> = cmd.get_args().collect();
        assert_eq!(args, vec![std::ffi::OsStr::new(url)]);
    }

    #[cfg(unix)]
    fn sh(script: &str) -> std::process::Command {
        let mut cmd = std::process::Command::new("sh");
        cmd.args(["-c", script]);
        cmd
    }

    #[cfg(unix)]
    #[test]
    fn test_run_opener_reports_failure_with_message() {
        let wait = std::time::Duration::from_secs(5);
        let err = run_opener(sh("echo 'no application found' >&2; exit 4"), wait)
            .unwrap_err()
            .to_string();
        assert!(err.starts_with("sh failed"), "{err}");
        assert!(err.ends_with(": no application found"), "{err}");
        assert!(run_opener(sh("exit 0"), wait).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn test_run_opener_does_not_wait_for_foreground_application() {
        let start = std::time::Instant::now();
        run_opener(sh("sleep 5"), std::time::Duration::from_millis(100)).unwrap();
        assert!(start.elapsed() < std::time::Duration::from_secs(3));
    }

    /// Native contract: the real `open` rejects a missing file with a
    /// nonzero exit, and its message reaches the caller. Nothing launches.
    #[cfg(target_os = "macos")]
    #[test]
    fn test_macos_open_failure_is_reported() {
        let target = std::ffi::OsStr::new("/nonexistent/bibtui-test/paper.pdf");
        let err = run_opener(opener_command(target), OPENER_WAIT)
            .unwrap_err()
            .to_string();
        assert!(err.starts_with("open failed"), "{err}");
        assert!(err.contains("does not exist"), "{err}");
    }

    #[test]
    fn test_run_opener_missing_program() {
        let cmd = std::process::Command::new("bibtui-no-such-opener");
        let err = run_opener(cmd, std::time::Duration::from_secs(1))
            .unwrap_err()
            .to_string();
        assert_eq!(err, "bibtui-no-such-opener not found");
    }

    #[test]
    fn test_open_path_missing_file_is_error() {
        let err = open_path(Path::new("/nonexistent/bibtui/paper.pdf"))
            .unwrap_err()
            .to_string();
        assert!(err.starts_with("File not found"), "{err}");
    }

    #[test]
    fn test_shell_execute_error_messages() {
        assert_eq!(
            shell_execute_error(31),
            "Windows could not open it: no application is associated with this file type (error 31)"
        );
        assert!(shell_execute_error(2).contains("file not found"));
        assert!(shell_execute_error(29).contains("did not respond"));
        assert!(shell_execute_error(17).contains("unknown error (error 17)"));
    }

    #[test]
    fn test_parse_single_file() {
        let files = parse_file_field(":papers/foo.pdf:PDF");
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "papers/foo.pdf");
        assert_eq!(files[0].file_type, "PDF");
        assert_eq!(files[0].description, "");
    }

    #[test]
    fn test_parse_bare_path() {
        let files = parse_file_field("paper.pdf");
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].description, "");
        assert_eq!(files[0].path, "paper.pdf");
        assert_eq!(files[0].file_type, "PDF");
    }

    #[test]
    fn test_parse_bare_path_with_directory_and_whitespace() {
        let files = parse_file_field("  papers/My Paper.PdF  ");
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].description, "");
        assert_eq!(files[0].path, "papers/My Paper.PdF");
        assert_eq!(files[0].file_type, "PDF");
    }

    #[test]
    fn test_parse_bare_path_without_extension() {
        let files = parse_file_field("papers/notes");
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "papers/notes");
        assert_eq!(files[0].file_type, "");
    }

    #[test]
    fn test_parse_bare_path_with_escaped_separators() {
        // Escaped separators do not make the value multi-segment.
        let files = parse_file_field("a\\:b\\;c.pdf");
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "a:b;c.pdf");
        assert_eq!(files[0].file_type, "PDF");
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
            ParsedFile {
                description: "A".into(),
                path: "a.pdf".into(),
                file_type: "PDF".into(),
            },
            ParsedFile {
                description: "B".into(),
                path: "b.pdf".into(),
                file_type: "PS".into(),
            },
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
        let result = effective_file_dir(Path::new("/home/user/refs.bib"), Some("/data/papers"));
        assert_eq!(result, PathBuf::from("/data/papers"));
    }

    #[test]
    fn test_effective_file_dir_relative() {
        let result = effective_file_dir(Path::new("/home/user/refs.bib"), Some("papers"));
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
        // Escaped colons stay inside the component instead of splitting it.
        let files = parse_file_field(r"Desc:path\:with\:colons.pdf:PDF");
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].description, "Desc");
        assert_eq!(files[0].path, "path:with:colons.pdf");
        assert_eq!(files[0].file_type, "PDF");
    }

    #[test]
    fn test_parse_file_field_escaped_semicolon() {
        // Escaped semicolons keep it as one segment
        let files = parse_file_field(r"Desc:path\;with\;semi.pdf:PDF");
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "path;with;semi.pdf");
    }

    // ── make_relative ─────────────────────────────────────────────────────────
    // canonicalize falls back to to_path_buf() for non-existent paths, so
    // these tests exercise the component-based logic directly.

    #[test]
    fn test_make_relative_sibling() {
        // PDF is in the same directory as the base
        let rel = make_relative(
            Path::new("/home/user/papers"),
            Path::new("/home/user/papers/foo.pdf"),
        );
        assert_eq!(rel, PathBuf::from("foo.pdf"));
    }

    #[test]
    fn test_make_relative_subdirectory() {
        let rel = make_relative(
            Path::new("/home/user/papers"),
            Path::new("/home/user/papers/2024/foo.pdf"),
        );
        assert_eq!(rel, PathBuf::from("2024/foo.pdf"));
    }

    #[test]
    fn test_make_relative_parent() {
        // PDF is one level above the base directory
        let rel = make_relative(
            Path::new("/home/user/papers"),
            Path::new("/home/user/foo.pdf"),
        );
        assert_eq!(rel, PathBuf::from("../foo.pdf"));
    }

    #[test]
    fn test_make_relative_sibling_directory() {
        // base = /home/user/bib, target = /home/user/pdfs/foo.pdf
        let rel = make_relative(
            Path::new("/home/user/bib"),
            Path::new("/home/user/pdfs/foo.pdf"),
        );
        assert_eq!(rel, PathBuf::from("../pdfs/foo.pdf"));
    }

    #[test]
    fn test_make_relative_no_common_prefix() {
        // /home/alice/bib → up 3 (bib, alice, home) → down into /data/pdfs/foo.pdf
        let rel = make_relative(
            Path::new("/home/alice/bib"),
            Path::new("/data/pdfs/foo.pdf"),
        );
        assert_eq!(rel, PathBuf::from("../../../data/pdfs/foo.pdf"));
    }

    // ── split_semicolons edge cases ─────────────────────────────────────────

    #[test]
    fn test_split_semicolons_trailing_backslash() {
        // Backslash not followed by ';' or ':' is kept as-is
        let parts = split_semicolons(r"path\to\file");
        assert_eq!(parts, vec![r"path\to\file"]);
    }

    #[test]
    fn test_split_semicolons_escaped_semicolon_keeps_one_part() {
        // The escape is preserved for the colon stage to unescape.
        let parts = split_semicolons(r"a\;b");
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0], r"a\;b");
    }

    #[test]
    fn test_split_semicolons_two_parts() {
        let parts = split_semicolons("a;b");
        assert_eq!(parts, vec!["a", "b"]);
    }

    // ── split_colons edge cases ─────────────────────────────────────────────

    #[test]
    fn test_split_colons_backslash_not_before_colon() {
        // Backslash followed by a non-colon char is kept literally
        let parts = split_colons(r"desc:path\nfile.pdf:PDF");
        assert_eq!(parts[0], "desc");
        assert_eq!(parts[1], r"path\nfile.pdf");
        assert_eq!(parts[2], "PDF");
    }

    #[test]
    fn test_split_colons_stops_at_third_part() {
        // Only first 2 colons split; rest goes into part 3
        let parts = split_colons("a:b:c:d:e");
        assert_eq!(parts, vec!["a", "b", "c:d:e"]);
    }

    #[test]
    fn test_split_colons_escaped_colon_in_path() {
        let parts = split_colons(r"desc:path\:with\:colons.pdf:PDF");
        assert_eq!(parts[0], "desc");
        assert_eq!(parts[1], "path:with:colons.pdf");
        assert_eq!(parts[2], "PDF");
    }

    // ── parse_file_field edge cases ─────────────────────────────────────────

    #[test]
    fn test_parse_file_field_whitespace_only() {
        let files = parse_file_field("   ;   ;   ");
        assert!(files.is_empty());
    }

    #[test]
    fn test_parse_file_field_only_two_parts() {
        // Segment has only 1 colon: description + path, no file_type
        let files = parse_file_field("desc:file.pdf");
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].description, "desc");
        assert_eq!(files[0].path, "file.pdf");
        assert_eq!(files[0].file_type, "");
    }

    #[test]
    fn test_parse_file_field_single_part_is_bare_path() {
        // A colon-free single segment is treated as a bare path.
        let files = parse_file_field("just-a-description");
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "just-a-description");
        assert_eq!(files[0].description, "");
        assert_eq!(files[0].file_type, "");
    }

    // ── serialize/parse round trip with separators in components ───────────

    fn assert_round_trip(files: &[ParsedFile]) {
        let serialized = serialize_file_field(files);
        let reparsed = parse_file_field(&serialized);
        assert_eq!(reparsed.len(), files.len(), "serialized: {serialized:?}");
        for (orig, back) in files.iter().zip(reparsed.iter()) {
            assert_eq!(
                back.description, orig.description,
                "serialized: {serialized:?}"
            );
            assert_eq!(back.path, orig.path, "serialized: {serialized:?}");
            assert_eq!(back.file_type, orig.file_type, "serialized: {serialized:?}");
        }
    }

    #[test]
    fn test_round_trip_colon_in_description() {
        assert_round_trip(&[ParsedFile {
            description: "Review: final".into(),
            path: "papers/foo.pdf".into(),
            file_type: "PDF".into(),
        }]);
    }

    #[test]
    fn test_round_trip_semicolon_in_description() {
        assert_round_trip(&[ParsedFile {
            description: "part one; part two".into(),
            path: "a.pdf".into(),
            file_type: "PDF".into(),
        }]);
    }

    #[test]
    fn test_round_trip_backslash_in_path() {
        assert_round_trip(&[ParsedFile {
            description: "Windows copy".into(),
            path: r"C:\papers\foo.pdf".into(),
            file_type: "PDF".into(),
        }]);
    }

    #[test]
    fn test_round_trip_multiple_files_with_separators() {
        assert_round_trip(&[
            ParsedFile {
                description: "Review: final; approved".into(),
                path: r"dir\sub:strange.pdf".into(),
                file_type: "PDF".into(),
            },
            ParsedFile {
                description: String::new(),
                path: "plain.pdf".into(),
                file_type: "PDF".into(),
            },
        ]);
    }
}
