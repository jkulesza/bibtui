//! Bibliography persistence. Each write owns its temporary file, and callers
//! can replace the I/O boundary to exercise failures without OS permissions.

use std::fs;
use std::io::{self, Write};
use std::path::Path;

pub(crate) trait SaveIo {
    fn read(&self, path: &Path) -> io::Result<Option<Vec<u8>>> {
        match fs::read(path) {
            Ok(contents) => Ok(Some(contents)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error),
        }
    }

    fn backup(&self, source: &Path, destination: &Path) -> io::Result<()> {
        fs::copy(source, destination).map(|_| ())
    }

    fn rename_attachment(&self, source: &Path, destination: &Path) -> io::Result<()> {
        // Never overwrite an existing destination. A destination that is the
        // same file as the source is a case-only rename on a case-insensitive
        // filesystem (e.g. default APFS), which a plain rename handles safely.
        if source == destination {
            return Ok(());
        }
        if fs::symlink_metadata(destination).is_ok() {
            if same_file(source, destination) {
                return fs::rename(source, destination);
            }
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("{} already exists", destination.display()),
            ));
        }
        rename_no_replace(source, destination)
    }

    fn persist(&self, path: &Path, contents: &[u8]) -> io::Result<()>;
}

pub(crate) struct FileSaveIo;

impl SaveIo for FileSaveIo {
    fn persist(&self, path: &Path, contents: &[u8]) -> io::Result<()> {
        // Follow an existing symlink rather than replacing the link itself.
        // A dangling symlink is an error: never silently retarget it.
        let resolved;
        let path = match fs::symlink_metadata(path) {
            Ok(meta) if meta.file_type().is_symlink() => {
                resolved = fs::canonicalize(path)?;
                resolved.as_path()
            }
            Ok(_) => path,
            Err(error) if error.kind() == io::ErrorKind::NotFound => path,
            Err(error) => return Err(error),
        };
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let mut temporary = new_file_builder(".bibtui-").tempfile_in(parent)?;
        temporary.write_all(contents)?;
        temporary.flush()?;
        if let Ok(metadata) = fs::metadata(path) {
            temporary
                .as_file()
                .set_permissions(metadata.permissions())?;
        }
        temporary.as_file().sync_all()?;
        temporary.persist(path).map_err(|error| error.error)?;
        Ok(())
    }
}

/// A tempfile builder whose files get the same permissions as an ordinary new
/// file (0666 filtered by the umask) instead of tempfile's private 0600.
pub(crate) fn new_file_builder(prefix: &str) -> tempfile::Builder<'_, '_> {
    let mut builder = tempfile::Builder::new();
    builder.prefix(prefix);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        builder.permissions(fs::Permissions::from_mode(0o666));
    }
    builder
}

/// True when both paths name the same existing file.
pub(crate) fn same_file(a: &Path, b: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        match (fs::metadata(a), fs::metadata(b)) {
            (Ok(a), Ok(b)) => a.dev() == b.dev() && a.ino() == b.ino(),
            _ => false,
        }
    }
    #[cfg(not(unix))]
    {
        match (fs::canonicalize(a), fs::canonicalize(b)) {
            (Ok(a), Ok(b)) => a == b,
            _ => false,
        }
    }
}

/// Rename `source` to `destination`, failing if the destination exists.
///
/// Uses the kernel's atomic no-replace rename where available, then a hard
/// link, and finally a check-then-rename for filesystems that support neither
/// (exFAT/FAT, many network shares). Only the last step has a race window.
fn rename_no_replace(source: &Path, destination: &Path) -> io::Result<()> {
    let fatal = |error: &io::Error| {
        matches!(
            error.kind(),
            io::ErrorKind::AlreadyExists | io::ErrorKind::NotFound
        )
    };
    match native_rename_no_replace(source, destination) {
        Ok(()) => return Ok(()),
        Err(error) if fatal(&error) => return Err(error),
        Err(_) => {}
    }
    match fs::hard_link(source, destination) {
        Ok(()) => {
            if let Err(error) = fs::remove_file(source) {
                let _ = fs::remove_file(destination);
                return Err(error);
            }
            return Ok(());
        }
        Err(error) if fatal(&error) => return Err(error),
        Err(_) => {}
    }
    if fs::symlink_metadata(destination).is_ok() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("{} already exists", destination.display()),
        ));
    }
    fs::rename(source, destination)
}

#[cfg(any(target_os = "macos", target_os = "ios"))]
fn native_rename_no_replace(source: &Path, destination: &Path) -> io::Result<()> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    let from = CString::new(source.as_os_str().as_bytes())?;
    let to = CString::new(destination.as_os_str().as_bytes())?;
    // SAFETY: both arguments are valid NUL-terminated paths for the call.
    if unsafe { libc::renamex_np(from.as_ptr(), to.as_ptr(), libc::RENAME_EXCL) } == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(any(target_os = "linux", target_os = "android"))]
fn native_rename_no_replace(source: &Path, destination: &Path) -> io::Result<()> {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;
    const RENAME_NOREPLACE: libc::c_uint = 1;
    let from = CString::new(source.as_os_str().as_bytes())?;
    let to = CString::new(destination.as_os_str().as_bytes())?;
    // SAFETY: renameat2 takes two directory descriptors (AT_FDCWD here), two
    // valid NUL-terminated paths, and a flags word. The raw syscall is used
    // because older glibc and musl lack a renameat2 wrapper.
    let result = unsafe {
        libc::syscall(
            libc::SYS_renameat2,
            libc::AT_FDCWD,
            from.as_ptr(),
            libc::AT_FDCWD,
            to.as_ptr(),
            RENAME_NOREPLACE,
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(not(any(
    target_os = "macos",
    target_os = "ios",
    target_os = "linux",
    target_os = "android"
)))]
fn native_rename_no_replace(_: &Path, _: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "no atomic no-replace rename on this platform",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rename_no_replace_moves_file_and_refuses_existing_destination() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.pdf");
        let b = dir.path().join("b.pdf");
        fs::write(&a, b"A").unwrap();
        rename_no_replace(&a, &b).unwrap();
        assert!(!a.exists());
        assert_eq!(fs::read(&b).unwrap(), b"A");
        fs::write(&a, b"NEW").unwrap();
        let error = rename_no_replace(&a, &b).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read(&a).unwrap(), b"NEW");
        assert_eq!(fs::read(&b).unwrap(), b"A");
    }

    #[test]
    fn rename_attachment_allows_case_only_rename_of_the_same_file() {
        let dir = tempfile::tempdir().unwrap();
        let lower = dir.path().join("smith2020.pdf");
        let upper = dir.path().join("Smith2020.pdf");
        fs::write(&lower, b"PDF").unwrap();
        FileSaveIo.rename_attachment(&lower, &upper).unwrap();
        let names: Vec<_> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        assert_eq!(names, ["Smith2020.pdf"]);
        assert_eq!(fs::read(&upper).unwrap(), b"PDF");
    }

    #[test]
    fn rename_attachment_refuses_distinct_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.pdf");
        let b = dir.path().join("b.pdf");
        fs::write(&a, b"A").unwrap();
        fs::write(&b, b"B").unwrap();
        assert!(FileSaveIo.rename_attachment(&a, &b).is_err());
        assert_eq!(fs::read(&a).unwrap(), b"A");
        assert_eq!(fs::read(&b).unwrap(), b"B");
    }

    #[cfg(unix)]
    #[test]
    fn new_files_follow_the_umask_instead_of_private_tempfile_mode() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("new.bib");
        FileSaveIo.persist(&path, b"@Misc{A}\n").unwrap();
        let reference = dir.path().join("reference");
        fs::write(&reference, b"").unwrap();
        let mode = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
        // Same permissions as an ordinarily created file under the current umask.
        assert_eq!(mode(&path), mode(&reference));
    }
}
