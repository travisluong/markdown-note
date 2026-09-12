//! Optimistic replacement: byte/identity checks precede one atomic replacement.
use super::{DiskVersion, TextFormat, encode, platform};
use std::{
    error::Error,
    fmt, fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Conflict {
    Missing,
    ContentChanged,
    Replaced,
    UnsafePath,
    IdentityUnavailable,
}

#[derive(Debug)]
pub enum SaveError {
    Conflict(Conflict),
    Io(io::Error),
}
impl fmt::Display for SaveError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Conflict(conflict) => formatter.write_str(match conflict {
                Conflict::Missing => "The note no longer exists on disk",
                Conflict::ContentChanged => "The note changed on disk",
                Conflict::Replaced => "Another file replaced this note on disk",
                Conflict::UnsafePath => "The note's path changed or contains a filesystem link",
                Conflict::IdentityUnavailable => "The note's file identity could not be verified",
            }),
            Self::Io(error) => fmt::Display::fmt(error, formatter),
        }
    }
}
impl Error for SaveError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Conflict(_) => None,
        }
    }
}
impl From<io::Error> for SaveError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// Evidence for the snapshot actually committed, never a reread of the destination.
#[derive(Debug)]
pub struct WriteReceipt {
    pub version: DiskVersion,
}

pub(super) struct CapturedFile {
    // Keep the Windows sharing guard alive while comparing the captured bytes.
    _file: fs::File,
    pub version: DiskVersion,
    pub metadata: fs::Metadata,
}

pub(super) fn capture(path: &Path) -> io::Result<CapturedFile> {
    let mut file = platform::open_file(path)?;
    let before = file.metadata()?;
    if !before.is_file() || platform::is_link(&before) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "The note must be a regular file",
        ));
    }
    let mut raw = Vec::new();
    file.read_to_end(&mut raw)?;
    let metadata = file.metadata()?;
    if before.len() != metadata.len()
        || metadata.len() != raw.len() as u64
        || before.modified().ok() != metadata.modified().ok()
    {
        return Err(io::Error::other("The note changed while it was being read"));
    }
    let identity = platform::identity(&file)?;
    Ok(CapturedFile {
        _file: file,
        version: DiskVersion {
            raw: Arc::from(raw),
            identity,
        },
        metadata,
    })
}

fn missing(error: io::Error) -> SaveError {
    if error.kind() == io::ErrorKind::NotFound {
        SaveError::Conflict(Conflict::Missing)
    } else {
        SaveError::Io(error)
    }
}

/// Callers normally use the canonical path returned by open. Check ancestors as
/// well as the leaf, including Windows reparse points and Unix directory links.
fn validate_components(path: &Path, directory: bool) -> Result<(), SaveError> {
    for current in path.ancestors() {
        if current.as_os_str().is_empty() {
            continue;
        }
        let metadata = fs::symlink_metadata(current).map_err(missing)?;
        if platform::is_link(&metadata)
            || (current == path && !directory && !metadata.is_file())
            || ((current != path || directory) && !metadata.is_dir())
        {
            return Err(SaveError::Conflict(Conflict::UnsafePath));
        }
    }
    Ok(())
}

struct ParentGuard {
    path: PathBuf,
    canonical: PathBuf,
    identity: Option<platform::FileIdentity>,
}
impl ParentGuard {
    fn capture(path: &Path) -> Result<Self, SaveError> {
        validate_components(path, true)?;
        Ok(Self {
            path: path.to_path_buf(),
            canonical: fs::canonicalize(path)?,
            identity: platform::directory_identity(path)?,
        })
    }
    fn check(&self) -> Result<(), SaveError> {
        validate_components(&self.path, true)?;
        if fs::canonicalize(&self.path)? != self.canonical {
            return Err(SaveError::Conflict(Conflict::UnsafePath));
        }
        match (&self.identity, platform::directory_identity(&self.path)?) {
            (Some(expected), Some(actual)) if expected != &actual => {
                Err(SaveError::Conflict(Conflict::UnsafePath))
            }
            (Some(_), None) => Err(SaveError::Conflict(Conflict::IdentityUnavailable)),
            _ => Ok(()),
        }
    }
}

/// Do not clean up through an ancestor that has been moved or replaced. In that
/// unusual case the staged file may remain in the old directory for recovery.
struct StagedFile {
    file: Option<tempfile::NamedTempFile>,
    parent: ParentGuard,
}
impl StagedFile {
    fn persist(mut self, path: &Path) -> io::Result<fs::File> {
        match self.file.take().unwrap().persist(path) {
            Ok(file) => Ok(file),
            Err(error) => {
                self.file = Some(error.file);
                Err(error.error)
            }
        }
    }
}
impl Drop for StagedFile {
    fn drop(&mut self) {
        if let Some(file) = &mut self.file
            && self.parent.check().is_err()
        {
            file.disable_cleanup(true);
        }
    }
}

fn compare(expected: &DiskVersion, actual: &DiskVersion) -> Result<(), SaveError> {
    if expected.raw != actual.raw {
        return Err(SaveError::Conflict(Conflict::ContentChanged));
    }
    match (&expected.identity, &actual.identity) {
        (Some(expected), Some(actual)) if expected != actual => {
            Err(SaveError::Conflict(Conflict::Replaced))
        }
        (Some(_), None) => Err(SaveError::Conflict(Conflict::IdentityUnavailable)),
        _ => Ok(()),
    }
}

fn writable(captured: &CapturedFile) -> Result<(), SaveError> {
    if captured.metadata.permissions().readonly() {
        return Err(
            io::Error::new(io::ErrorKind::PermissionDenied, "The note is read-only").into(),
        );
    }
    Ok(())
}

/// Revalidate an app-owned move without treating a filesystem's changed-on-rename
/// ID as an external replacement. Call only after that move succeeds. This never
/// adopts changed contents as a new saved baseline.
pub fn version_after_move(path: &Path, expected: &DiskVersion) -> Result<DiskVersion, SaveError> {
    let path = std::path::absolute(path)?;
    validate_components(&path, false)?;
    let actual = capture(&path).map_err(missing)?;
    if expected.raw != actual.version.raw {
        return Err(SaveError::Conflict(Conflict::ContentChanged));
    }
    Ok(DiskVersion {
        raw: expected.raw.clone(),
        identity: actual.version.identity,
    })
}

/// Save only when the current raw bytes and available file identity match the
/// expected version. Checks are optimistic: an external rename can still race the
/// final path-based replacement. No automatic retry or implicit file recreation.
///
/// Unix permission bits are preserved; ownership, ACLs and extended attributes
/// are outside this policy. Windows retains tempfile's MoveFileExW replacement
/// semantics; destination ACLs, named streams and other attributes are not copied.
/// The staged file is synced before replacement; parent-directory crash durability
/// and arbitrary remote-filesystem failure guarantees are not added here.
pub fn save_file(
    path: &Path,
    text: &str,
    format: TextFormat,
    expected: &DiskVersion,
) -> Result<WriteReceipt, SaveError> {
    save_with_hooks(path, text, format, expected, || Ok(()), platform::identity)
}

fn save_with_hooks(
    path: &Path,
    text: &str,
    format: TextFormat,
    expected: &DiskVersion,
    before_commit: impl FnOnce() -> io::Result<()>,
    receipt_identity: impl FnOnce(&fs::File) -> io::Result<Option<platform::FileIdentity>>,
) -> Result<WriteReceipt, SaveError> {
    let path = std::path::absolute(path)?;
    validate_components(&path, false)?;
    let initial = capture(&path).map_err(missing)?;
    compare(expected, &initial.version)?;
    writable(&initial)?;
    #[cfg(unix)]
    let initial_permissions = initial.metadata.permissions();
    drop(initial);
    let parent = ParentGuard::capture(
        path.parent()
            .ok_or_else(|| io::Error::other("The note has no parent directory"))?,
    )?;
    let temporary = tempfile::NamedTempFile::new_in(&parent.path)?;
    let mut staged = StagedFile {
        file: Some(temporary),
        parent,
    };
    let encoded: Arc<[u8]> = Arc::from(encode(text, format));
    let file = staged.file.as_mut().unwrap();
    file.write_all(&encoded)?;
    // Keep staging private until the contents are complete.
    #[cfg(unix)]
    file.as_file().set_permissions(initial_permissions)?;
    file.as_file().sync_all()?;
    before_commit()?;
    staged.parent.check()?;
    validate_components(&path, false)?;
    let current = capture(&path).map_err(missing)?;
    compare(expected, &current.version)?;
    writable(&current)?;
    // Capture permission changes made during staging as well as their read-only state.
    #[cfg(unix)]
    {
        let file = staged.file.as_ref().unwrap().as_file();
        file.set_permissions(current.metadata.permissions())?;
        file.sync_all()?;
    }
    staged.parent.check()?;
    validate_components(&path, false)?;
    // MoveFileExW rejects replacement while our comparison handle is
    // open. Release it only after validation; the remaining rename race is part
    // of this store's explicit optimistic contract.
    drop(current);
    let committed = staged.persist(&path)?;
    // A committed write remains successful if identity querying fails. Its bytes
    // are still known exactly. Never reread the destination to mint this receipt.
    let identity = receipt_identity(&committed).ok().flatten();
    Ok(WriteReceipt {
        version: DiskVersion {
            raw: encoded,
            identity,
        },
    })
}

#[cfg(test)]
mod tests;
