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

/// JXA script that puts each argv path on the general pasteboard the way
/// Finder's Copy does: Finder's item types plus a security-scope token, so
/// sandboxed apps such as Apple Mail and Outlook can attach the file.
#[cfg(target_os = "macos")]
const MACOS_COPY_FILES_JXA: &str = r#"
ObjC.import('AppKit');

// The file's Finder icon as .icns data, or null. ImageIO cannot encode
// icns from JXA directly, so render the 512px icon to PNG and let sips
// convert it (its icns writer rejects 1024px@72dpi, hence 512).
function icnsFor(path, tmpDir, i) {
    const reps = $.NSWorkspace.sharedWorkspace.iconForFile(path).representations;
    let cg = null;
    for (let r = 0; r < reps.count && !cg; r++) {
        const rep = reps.objectAtIndex(r);
        if (rep.pixelsWide == 512) cg = rep.CGImageForProposedRectContextHints(null, $(), $());
    }
    if (!cg) return null;
    const png = $.NSBitmapImageRep.alloc.initWithCGImage(cg)
        .representationUsingTypeProperties($.NSBitmapImageFileTypePNG, $());
    const pngPath = tmpDir + '/icon' + i + '.png';
    const icnsPath = tmpDir + '/icon' + i + '.icns';
    if (!png.writeToFileAtomically(pngPath, true)) return null;
    const task = $.NSTask.alloc.init;
    task.launchPath = '/usr/bin/sips';
    task.arguments = ['-s', 'format', 'icns', pngPath, '--out', icnsPath];
    task.standardOutput = $.NSFileHandle.fileHandleWithNullDevice;
    task.standardError = $.NSFileHandle.fileHandleWithNullDevice;
    task.launch;
    task.waitUntilExit;
    if (task.terminationStatus != 0) return null;
    const data = $.NSData.dataWithContentsOfFile(icnsPath);
    return data.isNil() ? null : data;
}

// One pasteboard item per file, with the same types Finder's Copy writes:
// file URL, file name as UTF-8 and BOM-prefixed UTF-16 text, and the icon.
function run(argv) {
    const fm = $.NSFileManager.defaultManager;
    const tmpDir = ObjC.unwrap($.NSTemporaryDirectory()) + 'bibtui-yank-' + $.NSProcessInfo.processInfo.processIdentifier;
    fm.createDirectoryAtPathWithIntermediateDirectoriesAttributesError(tmpDir, true, $(), null);
    const items = $.NSMutableArray.alloc.init;
    argv.forEach((p, i) => {
        const url = $.NSURL.fileURLWithPath(p);
        const name = url.lastPathComponent;
        const item = $.NSPasteboardItem.alloc.init;
        item.setStringForType(url.absoluteString, 'public.file-url');
        item.setDataForType(name.dataUsingEncoding($.NSUTF16StringEncoding), 'public.utf16-external-plain-text');
        item.setStringForType(name, 'public.utf8-plain-text');
        const icns = icnsFor(p, tmpDir, i);
        if (icns) item.setDataForType(icns, 'com.apple.icns');
        items.addObject(item);
    });
    fm.removeItemAtPathError(tmpDir, null);
    const pb = $.NSPasteboard.generalPasteboard;
    pb.clearContents;
    if (!pb.writeObjects(items)) { throw new Error('pasteboard write failed'); }
    // Sandboxed apps (Mail, Outlook) may only open a pasted file when the
    // item carries a security-scope token, which Finder attaches with this
    // AppKit SPI. Without it they receive the URL but are denied the file.
    const sel = '_attachSecurityScopeToURL:index:';
    if (pb.respondsToSelector(sel)) {
        argv.forEach((p, i) => pb._attachSecurityScopeToURLIndex($.NSURL.fileURLWithPath(p), i));
    }
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

    /// Overwrites the real clipboard, so run on demand:
    /// `cargo test finder_layout -- --ignored`
    #[cfg(target_os = "macos")]
    #[test]
    #[ignore]
    fn test_copy_files_matches_finder_layout() {
        use std::process::Command;
        let dir = tempfile::tempdir().unwrap();
        let paths: Vec<PathBuf> = ["a b.pdf", "c.pdf"]
            .iter()
            .map(|n| {
                let p = dir.path().join(n);
                std::fs::write(&p, b"%PDF").unwrap();
                p
            })
            .collect();
        copy_files_to_clipboard(&paths).unwrap();

        let dump = r#"ObjC.import("AppKit");
            const items = $.NSPasteboard.generalPasteboard.pasteboardItems;
            let out = [];
            for (let i = 0; i < items.count; i++) {
                const it = items.objectAtIndex(i);
                out.push(ObjC.deepUnwrap(it.types).join(",") + " " + ObjC.unwrap(it.stringForType("public.utf8-plain-text")));
            }
            out.join("\n")"#;
        let out = Command::new("osascript")
            .args(["-l", "JavaScript", "-e", dump])
            .output()
            .unwrap();
        let types = "public.file-url,public.utf16-external-plain-text,public.utf8-plain-text,com.apple.icns";
        assert_eq!(
            String::from_utf8_lossy(&out.stdout).trim(),
            format!("{types} a b.pdf\n{types} c.pdf")
        );
    }

    #[test]
    fn test_copy_files_empty_is_error() {
        assert!(copy_files_to_clipboard(&[]).is_err());
    }
}
