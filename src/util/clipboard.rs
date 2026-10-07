use std::path::{Path, PathBuf};

/// Abstraction over the system clipboard so app logic can be tested
/// without touching the real clipboard.
pub trait Clipboard {
    fn copy(&self, text: &str) -> anyhow::Result<()>;
    fn paste(&self) -> anyhow::Result<String>;
    /// Put the files themselves on the clipboard, so pasting into a mail
    /// client or file manager attaches/copies them.
    fn copy_files(&self, paths: &[PathBuf]) -> anyhow::Result<()>;
}

/// The real system clipboard (pbcopy/pbpaste on macOS, xclip/xsel on Linux).
pub struct SystemClipboard;

impl Clipboard for SystemClipboard {
    fn copy(&self, text: &str) -> anyhow::Result<()> {
        copy_to_clipboard(text)
    }
    fn paste(&self) -> anyhow::Result<String> {
        read_from_clipboard()
    }
    fn copy_files(&self, paths: &[PathBuf]) -> anyhow::Result<()> {
        copy_files_to_clipboard(paths)
    }
}

/// Read text from the system clipboard.
pub fn read_from_clipboard() -> anyhow::Result<String> {
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        let output = Command::new("pbpaste").output()?;
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    #[cfg(target_os = "linux")]
    {
        use std::process::Command;
        let result = Command::new("xclip")
            .args(["-selection", "clipboard", "-o"])
            .output();
        let output = match result {
            Ok(o) => o,
            Err(_) => Command::new("xsel")
                .args(["--clipboard", "--output"])
                .output()?,
        };
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        anyhow::bail!("Clipboard not supported on this platform");
    }
}

/// Copy text to system clipboard.
pub fn copy_to_clipboard(text: &str) -> anyhow::Result<()> {
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        let mut child = Command::new("pbcopy")
            .stdin(std::process::Stdio::piped())
            .spawn()?;
        if let Some(stdin) = child.stdin.as_mut() {
            use std::io::Write;
            stdin.write_all(text.as_bytes())?;
        }
        child.wait()?;
        Ok(())
    }

    #[cfg(target_os = "linux")]
    {
        use std::process::Command;
        // Try xclip first, then xsel
        let result = Command::new("xclip")
            .args(["-selection", "clipboard"])
            .stdin(std::process::Stdio::piped())
            .spawn();

        let mut child = match result {
            Ok(child) => child,
            Err(_) => Command::new("xsel")
                .arg("--clipboard")
                .stdin(std::process::Stdio::piped())
                .spawn()?,
        };

        if let Some(stdin) = child.stdin.as_mut() {
            use std::io::Write;
            stdin.write_all(text.as_bytes())?;
        }
        child.wait()?;
        Ok(())
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = text;
        anyhow::bail!("Clipboard not supported on this platform");
    }
}

/// JXA script that writes each argv path to the general pasteboard as a file
/// URL — the same thing Finder's Copy produces.
#[cfg(target_os = "macos")]
const MACOS_COPY_FILES_JXA: &str = r#"
ObjC.import('AppKit');
function run(argv) {
    const urls = $.NSMutableArray.alloc.init;
    argv.forEach(p => urls.addObject($.NSURL.fileURLWithPath(p)));
    const pb = $.NSPasteboard.generalPasteboard;
    pb.clearContents;
    if (!pb.writeObjects(urls)) { throw new Error('pasteboard write failed'); }
}
"#;

/// Copy files (not their contents or paths as text) to the system clipboard.
pub fn copy_files_to_clipboard(paths: &[PathBuf]) -> anyhow::Result<()> {
    if paths.is_empty() {
        anyhow::bail!("No files to copy");
    }

    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        let output = Command::new("osascript")
            .args(["-l", "JavaScript", "-e", MACOS_COPY_FILES_JXA])
            .args(paths)
            .output()?;
        if !output.status.success() {
            anyhow::bail!(
                "osascript failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        Ok(())
    }

    #[cfg(target_os = "linux")]
    {
        use std::io::Write;
        use std::process::{Command, Stdio};
        let uris = file_uri_list(paths);
        // Wayland first when available, then X11. xsel cannot set a MIME
        // target, so it is not usable here.
        let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();
        let spawn = || -> std::io::Result<std::process::Child> {
            if wayland {
                if let Ok(child) = Command::new("wl-copy")
                    .args(["--type", "text/uri-list"])
                    .stdin(Stdio::piped())
                    .spawn()
                {
                    return Ok(child);
                }
            }
            Command::new("xclip")
                .args(["-selection", "clipboard", "-t", "text/uri-list"])
                .stdin(Stdio::piped())
                .spawn()
        };
        let mut child = spawn()?;
        if let Some(stdin) = child.stdin.as_mut() {
            stdin.write_all(uris.as_bytes())?;
        }
        child.wait()?;
        Ok(())
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        anyhow::bail!("Copying files to the clipboard is not supported on this platform");
    }
}

/// `text/uri-list` body (RFC 2483): one percent-encoded `file://` URI per
/// line, CRLF-terminated.
#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
fn file_uri_list(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|p| format!("{}\r\n", file_uri(p)))
        .collect()
}

#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
fn file_uri(path: &Path) -> String {
    let mut out = String::from("file://");
    for &b in path.to_string_lossy().as_bytes() {
        if b.is_ascii_alphanumeric() || b"/-_.~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_uri_percent_encodes() {
        assert_eq!(
            file_uri(Path::new("/home/me/My Paper (2020).pdf")),
            "file:///home/me/My%20Paper%20%282020%29.pdf"
        );
        assert_eq!(file_uri(Path::new("/tmp/é.pdf")), "file:///tmp/%C3%A9.pdf");
    }

    #[test]
    fn test_file_uri_list_crlf() {
        let list = file_uri_list(&[PathBuf::from("/a.pdf"), PathBuf::from("/b.pdf")]);
        assert_eq!(list, "file:///a.pdf\r\nfile:///b.pdf\r\n");
    }

    #[test]
    fn test_copy_files_empty_is_error() {
        assert!(copy_files_to_clipboard(&[]).is_err());
    }
}
