//! Bibliography persistence. Each write owns its temporary file, and callers
//! can replace the I/O boundary to exercise failures without OS permissions.

use std::fs;
use std::io::{self, Write};
use std::path::Path;

pub(crate) trait SaveIo {
    fn backup(&self, source: &Path, destination: &Path) -> io::Result<()> {
        fs::copy(source, destination).map(|_| ())
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
        let parent = path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
        let mut temporary = tempfile::Builder::new().prefix(".bibtui-").tempfile_in(parent)?;
        temporary.write_all(contents)?;
        temporary.flush()?;
        if let Ok(metadata) = fs::metadata(path) {
            temporary.as_file().set_permissions(metadata.permissions())?;
        }
        temporary.as_file().sync_all()?;
        temporary.persist(path).map_err(|error| error.error)?;
        Ok(())
    }
}
