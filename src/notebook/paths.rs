//! Notebook path validation, link-aware resolution, and display-only spellings.
//! Stored identities retain their original paths; display normalization never changes I/O.

use std::{
    fs, io,
    path::{Component, Path, PathBuf},
};

pub fn validate_name(name: &str) -> io::Result<()> {
    let stem = name.split('.').next().unwrap_or("").to_ascii_uppercase();
    let reserved = ["CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$"].contains(&stem.as_str())
        || ["COM", "LPT"].iter().any(|prefix| {
            stem.strip_prefix(prefix).is_some_and(|s| {
                matches!(
                    s,
                    "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
                )
            })
        });
    if name.is_empty()
        || name.trim().is_empty()
        || name.ends_with([' ', '.'])
        || name
            .chars()
            .any(|c| c.is_control() || "<>:\"/\\|?*".contains(c))
        || name.encode_utf16().count() > 255
        || reserved
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Enter a valid folder name without slashes, reserved names, or trailing spaces/dots.",
        ));
    }
    Ok(())
}

pub fn safe_relative(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && path.components().all(|c| matches!(c, Component::Normal(_)))
        && !path
            .components()
            .any(|c| c.as_os_str().eq_ignore_ascii_case(".gpui-note"))
}

pub fn markdown(path: &Path) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .is_some_and(|s| s.eq_ignore_ascii_case("md") || s.eq_ignore_ascii_case("markdown"))
}

pub fn name(root: &Path) -> String {
    root.file_name()
        .unwrap_or(root.as_os_str())
        .to_string_lossy()
        .into_owned()
}

pub fn is_link(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

/// Resolve persisted paths one component at a time, without following links.
pub fn resolve(root: &Path, relative: &Path) -> io::Result<PathBuf> {
    if !safe_relative(relative) {
        return Err(io::Error::other("Invalid notebook-relative path"));
    }
    let mut path = root.to_path_buf();
    for component in relative.components() {
        path.push(component);
        if is_link(&fs::symlink_metadata(&path)?) {
            return Err(io::Error::other(
                "Notebook links are not opened or traversed.",
            ));
        }
    }
    Ok(path)
}

/// A display-only spelling; filesystem operations and stored identities keep the original path.
pub fn display_path(path: &Path) -> String {
    let value = path.to_string_lossy();
    if let Some(unc) = value.strip_prefix(r"\\?\UNC\")
        && unc
            .split_once('\\')
            .is_some_and(|(server, share)| !server.is_empty() && !share.is_empty())
    {
        return format!(r"\\{unc}");
    }
    if let Some(drive) = value.strip_prefix(r"\\?\") {
        let bytes = drive.as_bytes();
        if bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && bytes[2] == b'\\'
        {
            return drive.into();
        }
    }
    value.into_owned()
}

pub fn remap_path(path: &Path, from: &Path, to: &Path) -> PathBuf {
    path.strip_prefix(from)
        .map_or_else(|_| path.to_path_buf(), |suffix| to.join(suffix))
}

#[cfg(test)]
mod tests;
