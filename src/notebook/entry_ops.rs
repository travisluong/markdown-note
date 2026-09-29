//! Guarded sidebar filesystem operations. No application state or dialogs.
use super::{is_link, resolve, safe_relative, validate_name};
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

pub fn entry_directory(root: &Path, parent: &Path) -> io::Result<PathBuf> {
    let metadata = fs::symlink_metadata(root)?;
    if is_link(&metadata) || !metadata.is_dir() {
        return Err(io::Error::other(
            "Notebook root must be a regular directory.",
        ));
    }
    let path = if parent.as_os_str().is_empty() {
        root.to_path_buf()
    } else {
        resolve(root, parent)?
    };
    if !path.is_dir() {
        return Err(io::Error::other("Choose a folder."));
    }
    Ok(path)
}
pub fn entry_path(root: &Path, relative: &Path) -> io::Result<PathBuf> {
    entry_directory(root, Path::new(""))?;
    resolve(root, relative)
}
fn child(parent: &Path, name: &str) -> io::Result<PathBuf> {
    validate_name(name)?;
    let path = parent.join(name);
    if !safe_relative(&path) {
        return Err(io::Error::other(
            "This name is reserved for notebook settings.",
        ));
    }
    Ok(path)
}
pub fn rename_destination(source: &Path, name: &str, directory: bool) -> io::Result<PathBuf> {
    // Validate the requested name before preserving a file extension.
    validate_name(name)?;
    let mut name = name.to_owned();
    if !directory
        && Path::new(&name).extension().is_none()
        && let Some(ext) = source.extension()
    {
        name.push('.');
        name.push_str(&ext.to_string_lossy());
    }
    child(source.parent().unwrap_or(Path::new("")), &name)
}
pub fn rename_entry(root: &Path, source: &Path, name: &str) -> io::Result<PathBuf> {
    let from = entry_path(root, source)?;
    let destination = rename_destination(source, name, from.is_dir())?;
    if destination == source {
        return Ok(destination);
    }
    let to = entry_directory(root, destination.parent().unwrap())?
        .join(destination.file_name().unwrap());
    match fs::symlink_metadata(&to) {
        Ok(_) => {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "An entry with that name already exists.",
            ));
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    fs::rename(from, to)?;
    Ok(destination)
}
pub fn create_folder(root: &Path, parent: &Path, name: &str) -> io::Result<PathBuf> {
    let relative = child(parent, name)?;
    fs::create_dir(entry_directory(root, parent)?.join(name))?;
    Ok(relative)
}
pub fn duplicate_entry(root: &Path, source: &Path, bytes: Option<&[u8]>) -> io::Result<PathBuf> {
    let from = entry_path(root, source)?;
    if !from.is_file() {
        return Err(io::Error::other("Only files can be duplicated."));
    }
    let parent = source.parent().unwrap();
    let directory = entry_directory(root, parent)?;
    let disk;
    let bytes = if let Some(bytes) = bytes {
        bytes
    } else {
        disk = fs::read(&from)?;
        &disk
    };
    let stem = source.file_stem().unwrap().to_string_lossy();
    let extension = source
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    let mut staged = tempfile::NamedTempFile::new_in(&directory)?;
    staged.write_all(bytes)?;
    staged.flush()?;
    for number in 1u64.. {
        let suffix = if number == 1 {
            String::new()
        } else {
            format!(" {number}")
        };
        let name = format!("{stem} copy{suffix}{extension}");
        let relative = child(parent, &name)?;
        match staged.persist_noclobber(directory.join(&name)) {
            Ok(_) => return Ok(relative),
            Err(error) if error.error.kind() == io::ErrorKind::AlreadyExists => staged = error.file,
            Err(error) => return Err(error.error),
        }
    }
    Err(io::Error::other("No available copy name."))
}
pub fn trash_entry(root: &Path, relative: &Path) -> io::Result<()> {
    let path = entry_path(root, relative)?;
    #[cfg(windows)]
    return run_trash_operation(move || trash::delete(path).map_err(io::Error::other));
    #[cfg(not(windows))]
    trash::delete(path).map_err(io::Error::other)
}

#[cfg(windows)]
fn run_trash_operation(
    operation: impl FnOnce() -> io::Result<()> + Send + 'static,
) -> io::Result<()> {
    // GPUI uses Windows Runtime MTA workers, but trash initializes COM as STA.
    // A fresh thread lets trash own its apartment and keeps any dependency
    // panic out of the executor. Callers already run this blocking I/O off UI.
    std::thread::Builder::new()
        .name("markdown-note-trash".into())
        .spawn(operation)?
        .join()
        .map_err(|_| io::Error::other("Trash operation failed unexpectedly."))?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(windows)]
    #[test]
    fn trash_operation_can_initialize_sta_from_an_mta_caller() {
        use windows_sys::Win32::System::Com::{
            COINIT_APARTMENTTHREADED, COINIT_MULTITHREADED, CoInitializeEx, CoUninitialize,
        };

        // Match GPUI's Windows Runtime worker apartment without launching GPUI
        // or touching the real Recycle Bin. Only probe COM initialization.
        std::thread::spawn(|| {
            // SAFETY: null reserved pointer and a valid apartment flag. Each
            // successful initialization is balanced on the same thread below.
            let initialized =
                unsafe { CoInitializeEx(std::ptr::null(), COINIT_MULTITHREADED as u32) };
            assert!(initialized >= 0);
            let result = run_trash_operation(|| {
                // SAFETY: as above; this mirrors trash's default STA setup.
                let initialized =
                    unsafe { CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32) };
                if initialized < 0 {
                    return Err(io::Error::other(format!(
                        "Trash STA initialization failed: {initialized:#x}"
                    )));
                }
                // SAFETY: balance this thread's successful initialization.
                unsafe { CoUninitialize() };
                Ok(())
            });
            // SAFETY: balance the caller's successful MTA initialization.
            unsafe { CoUninitialize() };
            result.unwrap();
        })
        .join()
        .unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn trash_operation_returns_errors_and_contains_backend_panics() {
        let error = run_trash_operation(|| {
            Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "fixture denial",
            ))
        })
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(error.to_string(), "fixture denial");
        let error = run_trash_operation(|| panic!("fixture shell failure")).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("Trash operation failed unexpectedly")
        );
        run_trash_operation(|| Ok(())).unwrap();
    }

    #[test]
    fn creation_rename_and_copy_guard_names_and_preserve_bytes() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        create_folder(root, Path::new(""), "notes").unwrap();
        assert!(create_folder(root, Path::new(""), "notes").is_err());
        for name in ["../escape", ".markdown-note", "CON", "bad/child", "bad."] {
            assert!(create_folder(root, Path::new(""), name).is_err());
        }
        fs::write(root.join("notes/a.md"), b"\xef\xbb\xbfhello\r\n").unwrap();
        let source = Path::new("notes/a.md");
        let copy = duplicate_entry(root, source, None).unwrap();
        assert_eq!(copy, Path::new("notes/a copy.md"));
        assert_eq!(fs::read(root.join(copy)).unwrap(), b"\xef\xbb\xbfhello\r\n");
        let copy = duplicate_entry(root, source, Some(b"unsaved")).unwrap();
        assert_eq!(copy, Path::new("notes/a copy 2.md"));
        assert_eq!(fs::read(root.join(copy)).unwrap(), b"unsaved");
        assert!(rename_entry(root, source, "a copy").is_err());
        assert_eq!(
            rename_entry(root, source, "renamed").unwrap(),
            Path::new("notes/renamed.md")
        );
        assert!(entry_path(root, Path::new("../outside")).is_err());
        assert!(entry_path(root, Path::new("")).is_err());
    }
}
