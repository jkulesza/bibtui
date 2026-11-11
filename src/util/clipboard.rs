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

/// The real system clipboard: pbcopy/pbpaste on macOS, wl-copy/wl-paste
/// (Wayland) or xclip/xsel (X11) on Linux, and the Win32 clipboard API on
/// Windows.
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

/// Run a clipboard helper, feed it `input` on stdin, and return its stdout.
/// Fails when the helper cannot start or exits unsuccessfully, so a broken
/// backend is reported instead of looking like a successful copy or an
/// empty paste.
///
/// Copy helpers such as xclip and wl-copy leave a background process
/// serving the selection, which inherits the helper's output handles. When
/// copying, stdout therefore goes to null and stderr to a file rather than a
/// pipe: reading a pipe to its end would wait for that background process.
#[cfg_attr(
    not(any(target_os = "macos", target_os = "linux", all(unix, test))),
    allow(dead_code)
)]
fn run_clipboard_command(
    cmd: &mut std::process::Command,
    input: Option<&[u8]>,
) -> anyhow::Result<Vec<u8>> {
    use std::io::{Read, Seek, Write};
    use std::process::Stdio;
    let program = cmd.get_program().to_string_lossy().into_owned();
    let mut stderr_file = tempfile::tempfile()?;
    let (stdin, stdout) = match input {
        Some(_) => (Stdio::piped(), Stdio::null()),
        None => (Stdio::null(), Stdio::piped()),
    };
    let mut child = cmd
        .stdin(stdin)
        .stdout(stdout)
        .stderr(Stdio::from(stderr_file.try_clone()?))
        .spawn()
        .map_err(|e| anyhow::Error::new(e).context(format!("could not run {program}")))?;
    if let (Some(data), Some(mut pipe)) = (input, child.stdin.take()) {
        pipe.write_all(data)?;
        // Dropping the pipe closes it so the helper sees end of input.
    }
    let output = child.wait_with_output()?;
    if !output.status.success() {
        let mut stderr = String::new();
        stderr_file.rewind()?;
        // Best effort: a helper's stderr need not be UTF-8.
        let mut bytes = Vec::new();
        if stderr_file.read_to_end(&mut bytes).is_ok() {
            stderr = String::from_utf8_lossy(&bytes).trim().to_string();
        }
        if stderr.is_empty() {
            anyhow::bail!("{program} failed ({})", output.status);
        }
        anyhow::bail!("{program} failed ({}): {stderr}", output.status);
    }
    Ok(output.stdout)
}

/// Try each helper in turn, moving on only when one is not installed.
/// A helper that is installed but fails stops the search with its error.
#[cfg_attr(not(any(target_os = "linux", all(unix, test))), allow(dead_code))]
fn run_first_available(
    commands: Vec<std::process::Command>,
    input: Option<&[u8]>,
    missing_hint: &str,
) -> anyhow::Result<Vec<u8>> {
    for mut cmd in commands {
        match run_clipboard_command(&mut cmd, input) {
            Err(e)
                if e.downcast_ref::<std::io::Error>()
                    .is_some_and(|io| io.kind() == std::io::ErrorKind::NotFound) => {}
            other => return other,
        }
    }
    anyhow::bail!("No clipboard tool found ({missing_hint})")
}

/// Text clipboard tools tried on Linux, for the "not found" message.
#[cfg(target_os = "linux")]
const LINUX_TEXT_TOOLS: &str = "install wl-clipboard on Wayland, or xclip or xsel on X11";

/// Whether a paste helper's error only means there is no text to paste.
/// pbpaste succeeds with empty output then, but xclip and wl-paste exit
/// unsuccessfully; that should read as an empty paste, like on macOS.
#[cfg_attr(not(any(target_os = "linux", test)), allow(dead_code))]
fn is_empty_clipboard_error(message: &str) -> bool {
    // xclip: "Error: target UTF8_STRING not available"; wl-paste:
    // "Nothing is copied" or "No selection" (wording varies by version),
    // or "No suitable type of content copied" for non-text content.
    [
        "not available",
        "Nothing is copied",
        "No selection",
        "No suitable type",
    ]
    .iter()
    .any(|m| message.contains(m))
}

/// Read text from the system clipboard.
pub fn read_from_clipboard() -> anyhow::Result<String> {
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        let out = run_clipboard_command(&mut Command::new("pbpaste"), None)?;
        Ok(String::from_utf8_lossy(&out).into_owned())
    }

    #[cfg(target_os = "linux")]
    {
        use std::process::Command;
        let mut commands = Vec::new();
        if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            let mut wl = Command::new("wl-paste");
            wl.arg("--no-newline");
            commands.push(wl);
        }
        let mut xclip = Command::new("xclip");
        xclip.args(["-selection", "clipboard", "-o"]);
        commands.push(xclip);
        let mut xsel = Command::new("xsel");
        xsel.args(["--clipboard", "--output"]);
        commands.push(xsel);
        let out = match run_first_available(commands, None, LINUX_TEXT_TOOLS) {
            Err(e) if is_empty_clipboard_error(&e.to_string()) => Vec::new(),
            other => other?,
        };
        Ok(String::from_utf8_lossy(&out).into_owned())
    }

    #[cfg(windows)]
    {
        windows::paste_text()
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    {
        anyhow::bail!("Clipboard not supported on this platform");
    }
}

/// Copy text to system clipboard.
pub fn copy_to_clipboard(text: &str) -> anyhow::Result<()> {
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        run_clipboard_command(&mut Command::new("pbcopy"), Some(text.as_bytes()))?;
        Ok(())
    }

    #[cfg(target_os = "linux")]
    {
        use std::process::Command;
        let mut commands = Vec::new();
        if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            commands.push(Command::new("wl-copy"));
        }
        let mut xclip = Command::new("xclip");
        xclip.args(["-selection", "clipboard"]);
        commands.push(xclip);
        let mut xsel = Command::new("xsel");
        xsel.arg("--clipboard").arg("--input");
        commands.push(xsel);
        run_first_available(commands, Some(text.as_bytes()), LINUX_TEXT_TOOLS)?;
        Ok(())
    }

    #[cfg(windows)]
    {
        windows::copy_text(text)
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    {
        let _ = text;
        anyhow::bail!("Clipboard not supported on this platform");
    }
}

/// Windows clipboard access through the Win32 API (CF_UNICODETEXT for
/// text, CF_HDROP for files, the format Explorer's Copy uses).
#[cfg(windows)]
mod windows {
    use clipboard_win::{formats, options::DoClear, raw, Clipboard};
    use std::path::PathBuf;

    /// Another program may hold the clipboard open briefly; retry like
    /// browsers do before giving up.
    const OPEN_ATTEMPTS: usize = 10;

    fn open() -> anyhow::Result<Clipboard> {
        Clipboard::new_attempts(OPEN_ATTEMPTS)
            .map_err(|e| anyhow::anyhow!("could not open the Windows clipboard: {e}"))
    }

    pub(super) fn copy_text(text: &str) -> anyhow::Result<()> {
        let _clipboard = open()?;
        raw::set_string_with(text, DoClear)
            .map_err(|e| anyhow::anyhow!("could not set clipboard text: {e}"))
    }

    pub(super) fn paste_text() -> anyhow::Result<String> {
        let _clipboard = open()?;
        // No text on the clipboard reads as empty, as with pbpaste/xclip.
        if !raw::is_format_avail(formats::CF_UNICODETEXT) {
            return Ok(String::new());
        }
        let mut out = Vec::new();
        raw::get_string(&mut out)
            .map_err(|e| anyhow::anyhow!("could not read clipboard text: {e}"))?;
        Ok(String::from_utf8_lossy(&out).into_owned())
    }

    pub(super) fn copy_files(paths: &[PathBuf]) -> anyhow::Result<()> {
        let list = super::windows_file_list(paths)?;
        let _clipboard = open()?;
        raw::set_file_list_with(&list, DoClear)
            .map_err(|e| anyhow::anyhow!("could not put files on the clipboard: {e}"))
    }
}

/// Absolute, UTF-8 paths for a Windows CF_HDROP file list. Explorer and
/// mail clients resolve dropped files without a working directory, so a
/// relative path would point nowhere.
#[cfg_attr(not(any(windows, test)), allow(dead_code))]
fn windows_file_list(paths: &[PathBuf]) -> anyhow::Result<Vec<String>> {
    paths
        .iter()
        .map(|p| {
            let abs = std::path::absolute(p)
                .map_err(|e| anyhow::anyhow!("could not resolve {}: {e}", p.display()))?;
            abs.into_os_string().into_string().map_err(|raw| {
                anyhow::anyhow!(
                    "{} is not valid Unicode and cannot be copied",
                    std::path::Path::new(&raw).display()
                )
            })
        })
        .collect()
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
        use std::process::Command;
        let uris = file_uri_list(paths);
        // Wayland first when available, then X11. xsel cannot set a MIME
        // target, so it is not usable here.
        let mut commands = Vec::new();
        if std::env::var_os("WAYLAND_DISPLAY").is_some() {
            let mut wl = Command::new("wl-copy");
            wl.args(["--type", "text/uri-list"]);
            commands.push(wl);
        }
        let mut xclip = Command::new("xclip");
        xclip.args(["-selection", "clipboard", "-t", "text/uri-list"]);
        commands.push(xclip);
        run_first_available(commands, Some(uris.as_bytes()), "install wl-copy or xclip")?;
        Ok(())
    }

    #[cfg(windows)]
    {
        windows::copy_files(paths)
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
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

    #[cfg(unix)]
    fn sh(script: &str) -> std::process::Command {
        let mut cmd = std::process::Command::new("sh");
        cmd.args(["-c", script]);
        cmd
    }

    #[cfg(unix)]
    #[test]
    fn test_clipboard_command_returns_stdout_on_success() {
        let out = run_clipboard_command(&mut sh("printf 'pasted é'"), None).unwrap();
        assert_eq!(String::from_utf8(out).unwrap(), "pasted é");
    }

    #[cfg(unix)]
    #[test]
    fn test_clipboard_command_delivers_input() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("got");
        let mut cmd = sh("cat > \"$0\"");
        cmd.arg(&dest);
        run_clipboard_command(&mut cmd, Some("copied é\n".as_bytes())).unwrap();
        assert_eq!(std::fs::read_to_string(&dest).unwrap(), "copied é\n");
    }

    #[cfg(unix)]
    #[test]
    fn test_clipboard_command_nonzero_exit_reports_stderr() {
        let err = run_clipboard_command(&mut sh("echo 'no display' >&2; exit 3"), None)
            .unwrap_err()
            .to_string();
        assert!(err.starts_with("sh failed"), "{err}");
        assert!(err.ends_with(": no display"), "{err}");

        let err = run_clipboard_command(&mut sh("exit 1"), Some(b"x"))
            .unwrap_err()
            .to_string();
        assert!(err.starts_with("sh failed ("), "{err}");
    }

    #[cfg(unix)]
    #[test]
    fn test_clipboard_command_does_not_wait_for_background_server() {
        // Like xclip: the helper exits at once but leaves a child running
        // that still holds the inherited output handles.
        let start = std::time::Instant::now();
        run_clipboard_command(&mut sh("cat >/dev/null; (sleep 5 &) ; exit 0"), Some(b"x")).unwrap();
        assert!(start.elapsed() < std::time::Duration::from_secs(3));
    }

    #[cfg(unix)]
    #[test]
    fn test_first_available_skips_missing_tools_only() {
        let missing = std::process::Command::new("bibtui-no-such-clipboard-tool");
        let out = run_first_available(vec![missing, sh("printf second")], None, "hint").unwrap();
        assert_eq!(out, b"second");

        // An installed tool that fails is reported, not skipped.
        let err = run_first_available(vec![sh("exit 2"), sh("printf second")], None, "hint")
            .unwrap_err()
            .to_string();
        assert!(err.starts_with("sh failed"), "{err}");

        let none = vec![std::process::Command::new("bibtui-no-such-clipboard-tool")];
        let err = run_first_available(none, None, "install xclip")
            .unwrap_err()
            .to_string();
        assert_eq!(err, "No clipboard tool found (install xclip)");
    }

    #[test]
    fn test_empty_clipboard_errors_read_as_empty() {
        for msg in [
            "xclip failed (exit status: 1): Error: target UTF8_STRING not available",
            "wl-paste failed (exit status: 1): Nothing is copied",
            "wl-paste failed (exit status: 1): No selection",
            "wl-paste failed (exit status: 1): No suitable type of content copied",
        ] {
            assert!(is_empty_clipboard_error(msg), "{msg}");
        }
        assert!(!is_empty_clipboard_error(
            "xclip failed (exit status: 1): Error: Can't open display: (null)"
        ));
        assert!(!is_empty_clipboard_error(
            "wl-paste failed (exit status: 1): Failed to connect to a Wayland server"
        ));
    }

    #[test]
    fn test_windows_file_list_is_absolute() {
        let list = windows_file_list(&[PathBuf::from("PDF/a b.pdf")]).unwrap();
        let expected = std::env::current_dir().unwrap().join("PDF/a b.pdf");
        assert_eq!(list, vec![expected.to_string_lossy().into_owned()]);
        assert!(std::path::Path::new(&list[0]).is_absolute());
    }

    #[cfg(unix)]
    #[test]
    fn test_windows_file_list_rejects_non_unicode_path() {
        use std::os::unix::ffi::OsStrExt;
        let bad = PathBuf::from(std::ffi::OsStr::from_bytes(b"/tmp/\xff.pdf"));
        let err = windows_file_list(&[bad]).unwrap_err().to_string();
        assert!(err.contains("not valid Unicode"), "{err}");
    }

    /// Overwrites the real clipboard, so run on demand:
    /// `cargo test text_round_trip -- --ignored`
    #[test]
    #[ignore]
    fn test_system_clipboard_text_round_trip() {
        let text = "bibtui clipboard test — Gödel {A and B}";
        copy_to_clipboard(text).unwrap();
        assert_eq!(read_from_clipboard().unwrap(), text);
    }

    #[test]
    fn test_copy_files_empty_is_error() {
        assert!(copy_files_to_clipboard(&[]).is_err());
    }
}
