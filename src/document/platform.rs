//! Handle-based identity and the small platform-specific storage policy.
use std::{fs, io, path::Path};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum FileIdentity {
    #[cfg(unix)]
    Unix { device: u64, inode: u64 },
    #[cfg(windows)]
    Windows128 { volume: u64, index: [u8; 16] },
    #[cfg(windows)]
    Windows64 { volume: u32, index: u64 },
}

pub(super) fn is_link(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

pub(super) fn open_file(path: &Path) -> io::Result<fs::File> {
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_DELETE, FILE_SHARE_READ,
        };
        // Deny ordinary in-place writers while comparing bytes. Another process
        // can still rename the path. Release this handle before MoveFileExW:
        // that call rejects replacement while our deny-write handle remains open.
        options
            .share_mode(FILE_SHARE_READ | FILE_SHARE_DELETE)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    options.open(path)
}

#[cfg(not(unix))]
fn open_directory(path: &Path) -> io::Result<fs::File> {
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{
            FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES,
        };
        options
            .access_mode(FILE_READ_ATTRIBUTES)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT);
    }
    options.open(path)
}

/// Directory identity does not require permission to list its entries on Unix.
/// Saving needs write+execute permission, and must not add a directory-read requirement.
pub(super) fn directory_identity(path: &Path) -> io::Result<Option<FileIdentity>> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let metadata = fs::metadata(path)?;
        Ok(Some(FileIdentity::Unix {
            device: metadata.dev(),
            inode: metadata.ino(),
        }))
    }
    #[cfg(not(unix))]
    {
        identity(&open_directory(path)?)
    }
}
#[cfg(unix)]
pub(super) fn identity(file: &fs::File) -> io::Result<Option<FileIdentity>> {
    use std::os::unix::fs::MetadataExt;
    let metadata = file.metadata()?;
    Ok(Some(FileIdentity::Unix {
        device: metadata.dev(),
        inode: metadata.ino(),
    }))
}

#[cfg(windows)]
pub(super) fn identity(file: &fs::File) -> io::Result<Option<FileIdentity>> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::{
        Foundation::{ERROR_INVALID_FUNCTION, ERROR_INVALID_PARAMETER, ERROR_NOT_SUPPORTED},
        Storage::FileSystem::{
            BY_HANDLE_FILE_INFORMATION, FILE_ID_INFO, FileIdInfo, GetFileInformationByHandle,
            GetFileInformationByHandleEx,
        },
    };
    let unsupported = |error: &io::Error| {
        matches!(
            error.raw_os_error().map(|code| code as u32),
            Some(ERROR_INVALID_FUNCTION | ERROR_INVALID_PARAMETER | ERROR_NOT_SUPPORTED)
        )
    };
    let mut extended = FILE_ID_INFO::default();
    // SAFETY: the File owns a live handle, and the correctly sized output
    // structure is initialized, aligned, and exclusively borrowed for this call.
    let succeeded = unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle(),
            FileIdInfo,
            (&mut extended as *mut FILE_ID_INFO).cast(),
            std::mem::size_of::<FILE_ID_INFO>() as u32,
        )
    };
    if succeeded != 0 {
        return Ok(Some(FileIdentity::Windows128 {
            volume: extended.VolumeSerialNumber,
            index: extended.FileId.Identifier,
        }));
    }
    let error = io::Error::last_os_error();
    if !unsupported(&error) {
        return Err(error);
    }
    let mut legacy = BY_HANDLE_FILE_INFORMATION::default();
    // SAFETY: as above, the owned handle and initialized output remain valid.
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut legacy) } != 0 {
        return Ok(Some(FileIdentity::Windows64 {
            volume: legacy.dwVolumeSerialNumber,
            index: (u64::from(legacy.nFileIndexHigh) << 32) | u64::from(legacy.nFileIndexLow),
        }));
    }
    let error = io::Error::last_os_error();
    if unsupported(&error) {
        Ok(None)
    } else {
        Err(error)
    }
}

#[cfg(not(any(unix, windows)))]
pub(super) fn identity(_file: &fs::File) -> io::Result<Option<FileIdentity>> {
    Ok(None)
}
