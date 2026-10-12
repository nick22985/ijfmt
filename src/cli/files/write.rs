use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

struct TemporaryFile {
    path: PathBuf,
    file: fs::File,
}

impl Drop for TemporaryFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

pub(super) fn atomic_write(path: &Path, bytes: &[u8], metadata: &fs::Metadata) -> io::Result<()> {
    static NEXT_ID: AtomicU64 = AtomicU64::new(0);
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("Source has no parent directory"))?;
    let mut temporary = loop {
        let candidate = parent.join(format!(
            ".ijfmt-{}-{}.tmp",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed),
        ));
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&candidate) {
            Ok(file) => {
                break TemporaryFile {
                    path: candidate,
                    file,
                };
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    };
    temporary.file.write_all(bytes)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, chown};
        chown(&temporary.path, Some(metadata.uid()), Some(metadata.gid()))?;
    }
    temporary.file.set_permissions(metadata.permissions())?;
    temporary.file.sync_all()?;
    fs::rename(&temporary.path, path)
}
