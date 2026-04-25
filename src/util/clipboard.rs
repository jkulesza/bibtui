/// Read text from the system clipboard.
pub fn read_from_clipboard() -> anyhow::Result<String> {
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        let output = Command::new("pbpaste").output()?;
        return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
    }

    #[cfg(target_os = "linux")]
    {
        use std::process::Command;
        let result = Command::new("xclip")
            .args(["-selection", "clipboard", "-o"])
            .output();
        let output = match result {
            Ok(o) => o,
            Err(_) => Command::new("xsel").args(["--clipboard", "--output"]).output()?,
        };
        return Ok(String::from_utf8_lossy(&output.stdout).into_owned());
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
        return Ok(());
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
        return Ok(());
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    {
        let _ = text;
        anyhow::bail!("Clipboard not supported on this platform");
    }
}
