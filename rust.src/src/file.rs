//! File input/output and readonly handling (RFC-0006).
//!
//! A [`Document`] couples a [`Buffer`] with its on-disk path and a readonly
//! state. Opening a missing path auto-creates an empty file (unless opened in
//! readonly mode). Saving writes the buffer back and clears the modified flag.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::buffer::Buffer;

/// Why a document cannot be modified.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadOnly {
    /// The document is writable.
    Writable,
    /// The user requested readonly mode via `--readonly`.
    Flag,
    /// The file exists but is not writable on disk.
    NoPermission,
}

impl ReadOnly {
    /// Returns whether editing/saving is blocked.
    pub fn is_readonly(self) -> bool {
        !matches!(self, ReadOnly::Writable)
    }
}

/// Error returned while opening a document; the caller exits with code 1.
#[derive(Debug)]
pub enum OpenError {
    /// The file did not exist and could not be created.
    Create { path: PathBuf, source: io::Error },
    /// The file existed but could not be read.
    Read { path: PathBuf, source: io::Error },
    /// A missing file was requested in readonly mode.
    MissingReadonly { path: PathBuf },
}

impl fmt::Display for OpenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OpenError::Create { path, source } => {
                write!(f, "cannot create '{}': {}", path.display(), source)
            }
            OpenError::Read { path, source } => {
                write!(f, "cannot read '{}': {}", path.display(), source)
            }
            OpenError::MissingReadonly { path } => {
                write!(
                    f,
                    "cannot open '{}' in readonly mode: file does not exist",
                    path.display()
                )
            }
        }
    }
}

impl std::error::Error for OpenError {}

/// Error returned while saving a document.
#[derive(Debug)]
pub enum SaveError {
    /// Saving is blocked because the document is readonly.
    ReadOnly(ReadOnly),
    /// Writing the file failed.
    Io { path: PathBuf, source: io::Error },
}

impl fmt::Display for SaveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SaveError::ReadOnly(ReadOnly::Flag) => {
                write!(f, "cannot save: opened in readonly mode")
            }
            SaveError::ReadOnly(ReadOnly::NoPermission) => {
                write!(f, "cannot save: no write permission")
            }
            SaveError::ReadOnly(ReadOnly::Writable) => {
                write!(f, "cannot save")
            }
            SaveError::Io { path, source } => {
                write!(f, "cannot save '{}': {}", path.display(), source)
            }
        }
    }
}

impl std::error::Error for SaveError {}

/// Returns whether an existing file can be opened for writing.
fn is_writable(path: &Path) -> bool {
    fs::OpenOptions::new().write(true).open(path).is_ok()
}

/// A buffer bound to a file path together with its readonly state.
#[derive(Debug)]
pub struct Document {
    buffer: Buffer,
    path: PathBuf,
    readonly: ReadOnly,
}

impl Document {
    /// Opens `path`, loading its contents into a buffer.
    ///
    /// If the file does not exist it is created as an empty file, unless
    /// `readonly_flag` is set (a missing file cannot be viewed). If creation
    /// fails an [`OpenError`] is returned and the caller should exit with 1.
    /// When the file exists but is not writable, the document is opened in
    /// [`ReadOnly::NoPermission`] state.
    pub fn open(path: impl AsRef<Path>, readonly_flag: bool) -> Result<Self, OpenError> {
        let path = path.as_ref().to_path_buf();
        let exists = path.exists();

        if !exists {
            if readonly_flag {
                return Err(OpenError::MissingReadonly { path });
            }
            fs::write(&path, "").map_err(|source| OpenError::Create {
                path: path.clone(),
                source,
            })?;
            return Ok(Self {
                buffer: Buffer::new(""),
                path,
                readonly: ReadOnly::Writable,
            });
        }

        let content = fs::read_to_string(&path).map_err(|source| OpenError::Read {
            path: path.clone(),
            source,
        })?;

        let readonly = if readonly_flag {
            ReadOnly::Flag
        } else if is_writable(&path) {
            ReadOnly::Writable
        } else {
            ReadOnly::NoPermission
        };

        Ok(Self {
            buffer: Buffer::new(&content),
            path,
            readonly,
        })
    }

    /// Returns a shared reference to the underlying buffer.
    pub fn buffer(&self) -> &Buffer {
        &self.buffer
    }

    /// Returns a mutable reference to the underlying buffer.
    pub fn buffer_mut(&mut self) -> &mut Buffer {
        &mut self.buffer
    }

    /// Returns the document's file path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the readonly state.
    pub fn readonly(&self) -> ReadOnly {
        self.readonly
    }

    /// Returns whether editing/saving is blocked.
    pub fn is_readonly(&self) -> bool {
        self.readonly.is_readonly()
    }

    /// Returns the file name for display in the status line.
    pub fn file_name(&self) -> String {
        self.path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.path.to_string_lossy().into_owned())
    }

    /// Writes the buffer back to its path and clears the modified flag.
    ///
    /// Returns [`SaveError::ReadOnly`] if the document is readonly, or
    /// [`SaveError::Io`] on a write failure. On I/O failure the in-memory
    /// buffer is left untouched so the user does not lose edits.
    pub fn save(&mut self) -> Result<(), SaveError> {
        if self.readonly.is_readonly() {
            return Err(SaveError::ReadOnly(self.readonly));
        }
        fs::write(&self.path, self.buffer.to_text()).map_err(|source| SaveError::Io {
            path: self.path.clone(),
            source,
        })?;
        self.buffer.mark_saved();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    /// Returns a unique, nonexistent temp path for the current test.
    fn temp_path() -> PathBuf {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("be_test_{}_{}", std::process::id(), n))
    }

    /// Removes the file at `path`, ignoring errors.
    fn cleanup(path: &Path) {
        let _ = fs::remove_file(path);
    }

    #[test]
    fn open_creates_missing_file() {
        let path = temp_path();
        let doc = Document::open(&path, false).expect("open should create file");
        assert!(path.exists());
        assert_eq!(doc.buffer().to_text(), "");
        assert_eq!(doc.readonly(), ReadOnly::Writable);
        cleanup(&path);
    }

    #[test]
    fn open_loads_existing_content() {
        let path = temp_path();
        fs::write(&path, "hello\nworld\n").unwrap();
        let doc = Document::open(&path, false).unwrap();
        assert_eq!(doc.buffer().lines(), &["hello", "world"]);
        cleanup(&path);
    }

    #[test]
    fn open_missing_in_readonly_errors() {
        let path = temp_path();
        let err = Document::open(&path, true).unwrap_err();
        assert!(matches!(err, OpenError::MissingReadonly { .. }));
        assert!(!path.exists());
    }

    #[test]
    fn open_create_failure_when_parent_missing() {
        let path = temp_path().join("nested").join("file.txt");
        let err = Document::open(&path, false).unwrap_err();
        assert!(matches!(err, OpenError::Create { .. }));
    }

    #[test]
    fn save_writes_content_and_clears_modified() {
        let path = temp_path();
        let mut doc = Document::open(&path, false).unwrap();
        doc.buffer_mut().insert_char('h');
        doc.buffer_mut().insert_char('i');
        assert!(doc.buffer().is_modified());
        doc.save().unwrap();
        assert!(!doc.buffer().is_modified());
        assert_eq!(fs::read_to_string(&path).unwrap(), "hi");
        cleanup(&path);
    }

    #[test]
    fn save_blocked_in_readonly_flag() {
        let path = temp_path();
        fs::write(&path, "data").unwrap();
        let mut doc = Document::open(&path, true).unwrap();
        assert_eq!(doc.readonly(), ReadOnly::Flag);
        let err = doc.save().unwrap_err();
        assert!(matches!(err, SaveError::ReadOnly(ReadOnly::Flag)));
        cleanup(&path);
    }

    #[cfg(unix)]
    #[test]
    fn no_write_permission_is_readonly_and_save_blocked() {
        use std::os::unix::fs::PermissionsExt;
        let path = temp_path();
        fs::write(&path, "data").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o444)).unwrap();
        let mut doc = Document::open(&path, false).unwrap();
        assert_eq!(doc.readonly(), ReadOnly::NoPermission);
        let err = doc.save().unwrap_err();
        assert!(matches!(err, SaveError::ReadOnly(ReadOnly::NoPermission)));
        // Restore permissions so cleanup can remove the file.
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        cleanup(&path);
    }

    #[test]
    fn file_name_returns_basename() {
        let path = temp_path();
        let doc = Document::open(&path, false).unwrap();
        assert_eq!(doc.file_name(), path.file_name().unwrap().to_string_lossy());
        cleanup(&path);
    }
}
