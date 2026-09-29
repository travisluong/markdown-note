//! Notebook opening, directory mutations, and atomic JSON persistence.
//! Revalidate existing settings before replacement; write the notebook before its index.
//! Each replacement is atomic independently, not a transaction across both files.

use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

use serde::Serialize;

use super::{
    paths::{is_link, name, resolve, safe_relative, validate_name},
    settings::{Index, Settings, VERSION, normalize_sidebar_width, normalize_terminal_width},
    tree::{Entry, FileSort},
};

pub fn index_path() -> io::Result<PathBuf> {
    index_path_for(std::env::consts::OS, |key| std::env::var_os(key))
}

fn index_path_for(
    os: &str,
    env: impl Fn(&str) -> Option<std::ffi::OsString>,
) -> io::Result<PathBuf> {
    let absolute = |key| {
        env(key)
            .map(PathBuf::from)
            .filter(|path| path.is_absolute())
    };
    let base = match os {
        "windows" => absolute("APPDATA"),
        "macos" => absolute("HOME").map(|home| home.join("Library/Application Support")),
        _ => absolute("XDG_CONFIG_HOME")
            .or_else(|| absolute("HOME").map(|home| home.join(".config"))),
    };
    base.map(|base| base.join("markdown-note/notebooks.json"))
        .ok_or_else(|| io::Error::other(format!(
            "No absolute configuration directory is available for {os}; notebook history cannot be stored."
        )))
}

pub fn load_index(path: &Path) -> io::Result<Index> {
    let index: Index = read_json(path)?.unwrap_or_default();
    check_version(index.version)?;
    Ok(index)
}

fn check_version(version: u32) -> io::Result<()> {
    if version != VERSION {
        return Err(io::Error::other(
            "Unsupported notebook settings version. Existing settings were preserved.",
        ));
    }
    Ok(())
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> io::Result<Option<T>> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(io::Error::other),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

pub fn write_json(path: &Path, value: &impl Serialize) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("Missing settings parent folder"))?;
    fs::create_dir_all(parent)?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer_pretty(&mut temp, value)?;
    temp.write_all(b"\n")?;
    temp.as_file().sync_all()?;
    temp.persist(path).map_err(|error| error.error)?;
    Ok(())
}

pub fn save_state(path: &Path, index: &Index, book: Option<&Notebook>) -> io::Result<()> {
    // Revalidate before writing so externally damaged or newer settings survive.
    load_index(path)?;
    if let Some(book) = book {
        save_book(book)?;
    }
    write_json(path, index)
}

/// Replace one notebook's settings after preserving its existing path/schema guards.
pub fn save_book(book: &Notebook) -> io::Result<()> {
    if is_link(&fs::symlink_metadata(&book.root)?) {
        return Err(io::Error::other(
            "The notebook folder was replaced by a link.",
        ));
    }
    Notebook::open(&book.root, false)?;
    write_json(&settings_path(&book.root), &book.settings)
}

/// Replace the index independently, preserving corrupt or unsupported existing data.
pub fn save_index(path: &Path, index: &Index) -> io::Result<()> {
    load_index(path)?;
    write_json(path, index)
}

pub fn settings_path(root: &Path) -> PathBuf {
    root.join(".markdown-note/settings.json")
}

#[derive(Clone, Debug)]
pub struct Notebook {
    pub root: PathBuf,
    pub settings: Settings,
}

impl Notebook {
    pub fn open(path: &Path, create: bool) -> io::Result<Self> {
        if create {
            validate_name(&name(path))?;
            fs::create_dir(path)?;
        }
        let root = fs::canonicalize(path)?;
        if !root.is_dir() {
            return Err(io::Error::other("Choose an existing folder."));
        }
        let metadata_dir = root.join(".markdown-note");
        if let Ok(metadata) = fs::symlink_metadata(&metadata_dir)
            && (is_link(&metadata) || !metadata.is_dir())
        {
            return Err(io::Error::other(".markdown-note must be a regular folder."));
        }
        let path = settings_path(&root);
        if let Ok(metadata) = fs::symlink_metadata(&path)
            && is_link(&metadata)
        {
            return Err(io::Error::other("Notebook settings must not be a link."));
        }
        let existing: Option<Settings> = read_json(&path)?;
        let mut settings = existing.clone().unwrap_or_default();
        settings.sidebar_width = normalize_sidebar_width(settings.sidebar_width);
        settings.terminal_width = normalize_terminal_width(settings.terminal_width);
        check_version(settings.version)?;
        if settings
            .last_file
            .as_ref()
            .is_some_and(|p| !safe_relative(p))
            || settings.expanded.iter().any(|p| !safe_relative(p))
        {
            return Err(io::Error::other(
                "Invalid paths in notebook settings. Existing settings were preserved.",
            ));
        }
        if existing.is_none() {
            write_json(&path, &settings)?;
        }
        Ok(Self { root, settings })
    }
}

pub fn read_directory(root: &Path, relative: &Path) -> io::Result<Vec<Entry>> {
    read_directory_filtered(root, relative, &super::Exclusions::default())
}

/// Discovery-only filtering; explicit file access retains its usual path guards.
pub fn read_directory_filtered(
    root: &Path,
    relative: &Path,
    exclusions: &super::Exclusions,
) -> io::Result<Vec<Entry>> {
    if exclusions.excludes(relative) {
        return Ok(Vec::new());
    }
    let directory = if relative.as_os_str().is_empty() {
        root.to_path_buf()
    } else {
        resolve(root, relative)?
    };
    let mut entries = Vec::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        if entry.file_name().eq_ignore_ascii_case(".markdown-note") {
            continue;
        }
        let relative = relative.join(entry.file_name());
        if exclusions.excludes(&relative) {
            continue;
        }
        let metadata = fs::symlink_metadata(entry.path())?;
        entries.push(Entry {
            modified: metadata.modified().ok(),
            created: metadata.created().ok(),
            relative,
            directory: metadata.is_dir(),
            link: is_link(&metadata),
        });
    }
    FileSort::default().sort_entries(&mut entries);
    Ok(entries)
}

/// Move within the notebook without traversing links or replacing another entry.
pub fn move_entry(root: &Path, source: &Path, parent: &Path) -> io::Result<PathBuf> {
    if is_link(&fs::symlink_metadata(root)?) {
        return Err(io::Error::other("Notebook root is a link."));
    }
    let from = resolve(root, source)?;
    let directory = if parent.as_os_str().is_empty() {
        root.to_path_buf()
    } else {
        resolve(root, parent)?
    };
    if !directory.is_dir() {
        return Err(io::Error::other("Drop onto a folder."));
    }
    let canonical_from = fs::canonicalize(&from)?;
    let canonical_parent = fs::canonicalize(&directory)?;
    if canonical_parent.starts_with(&canonical_from) {
        return Err(io::Error::other("A folder cannot be moved into itself."));
    }
    let destination = parent.join(
        source
            .file_name()
            .ok_or_else(|| io::Error::other("Missing entry name."))?,
    );
    if destination == source {
        return Ok(destination);
    }
    let to = directory.join(source.file_name().unwrap());
    match fs::symlink_metadata(&to) {
        Ok(_) => {
            return Err(io::Error::other(
                "An entry with this name already exists in the destination.",
            ));
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    fs::rename(from, to)?;
    Ok(destination)
}

// Reserve a fresh name atomically; never truncate an existing file or follow a link.
pub fn create_untitled(root: &Path) -> io::Result<PathBuf> {
    let metadata = fs::symlink_metadata(root)?;
    if is_link(&metadata) || !metadata.is_dir() {
        return Err(io::Error::other(
            "Choose a notebook directory that is not a link.",
        ));
    }
    for number in 0u64.. {
        let name = if number == 0 {
            "Untitled.md".into()
        } else {
            format!("Untitled {number}.md")
        };
        let path = root.join(name);
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(file) => {
                drop(file);
                return Ok(path);
            }
            Err(error)
                if error.kind() == io::ErrorKind::AlreadyExists
                    || fs::symlink_metadata(&path).is_ok() =>
            {
                continue;
            }
            Err(error) => return Err(error),
        }
    }
    Err(io::Error::other(
        "No unused Untitled filename is available.",
    ))
}

#[cfg(test)]
mod tests;
