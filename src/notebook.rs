use std::{
    collections::BTreeSet,
    fs,
    io::{self, Write},
    path::{Component, Path, PathBuf},
};

use serde::{Deserialize, Serialize};

const VERSION: u32 = 1;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Settings {
    pub version: u32,
    pub last_file: Option<PathBuf>,
    pub expanded: BTreeSet<PathBuf>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: VERSION,
            last_file: None,
            expanded: BTreeSet::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Index {
    pub version: u32,
    pub notebooks: Vec<PathBuf>,
    pub active: Option<PathBuf>,
}

impl Default for Index {
    fn default() -> Self {
        Self {
            version: VERSION,
            notebooks: Vec::new(),
            active: None,
        }
    }
}

impl Index {
    pub fn register(&mut self, path: PathBuf) {
        if !self.notebooks.contains(&path) {
            self.notebooks.push(path.clone());
        }
        self.active = Some(path);
    }
}

pub fn index_path() -> io::Result<PathBuf> {
    std::env::var_os("APPDATA")
        .map(|base| PathBuf::from(base).join("gpui-note/notebooks.json"))
        .ok_or_else(|| io::Error::other("APPDATA is not set; notebook history cannot be stored."))
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
        if is_link(&fs::symlink_metadata(&book.root)?) {
            return Err(io::Error::other(
                "The notebook folder was replaced by a link.",
            ));
        }
        Notebook::open(&book.root, false)?;
        write_json(&settings_path(&book.root), &book.settings)?;
    }
    write_json(path, index)
}

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

pub fn settings_path(root: &Path) -> PathBuf {
    root.join(".gpui-note/settings.json")
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
        let metadata_dir = root.join(".gpui-note");
        if let Ok(metadata) = fs::symlink_metadata(&metadata_dir) {
            if is_link(&metadata) || !metadata.is_dir() {
                return Err(io::Error::other(".gpui-note must be a regular folder."));
            }
        }
        let path = settings_path(&root);
        if let Ok(metadata) = fs::symlink_metadata(&path) {
            if is_link(&metadata) {
                return Err(io::Error::other("Notebook settings must not be a link."));
            }
        }
        let existing: Option<Settings> = read_json(&path)?;
        let settings = existing.clone().unwrap_or_default();
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

#[derive(Clone, Debug)]
pub struct Entry {
    pub relative: PathBuf,
    pub directory: bool,
    pub link: bool,
}

pub fn read_directory(root: &Path, relative: &Path) -> io::Result<Vec<Entry>> {
    let directory = if relative.as_os_str().is_empty() {
        root.to_path_buf()
    } else {
        resolve(root, relative)?
    };
    let mut entries = Vec::new();
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        if entry.file_name().eq_ignore_ascii_case(".gpui-note") {
            continue;
        }
        let metadata = fs::symlink_metadata(entry.path())?;
        entries.push(Entry {
            relative: relative.join(entry.file_name()),
            directory: metadata.is_dir(),
            link: is_link(&metadata),
        });
    }
    entries.sort_by(|a, b| {
        b.directory
            .cmp(&a.directory)
            .then_with(|| {
                name(&a.relative)
                    .to_lowercase()
                    .cmp(&name(&b.relative).to_lowercase())
            })
            .then_with(|| a.relative.cmp(&b.relative))
    });
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_names() {
        for value in [
            "",
            " ",
            ".",
            "..",
            "a/b",
            "a\\b",
            "a:",
            "nul",
            "COM1.md",
            "LPT²",
            "trailing.",
            "trailing ",
        ] {
            assert!(validate_name(value).is_err(), "{value}");
        }
        for value in ["Work", "My notes", "日本語", "COM10", "notes.v2"] {
            validate_name(value).unwrap();
        }
    }

    #[test]
    fn create_reopen_and_persist() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("Notes");
        let mut book = Notebook::open(&root, true).unwrap();
        assert!(Notebook::open(&root, true).is_err());
        fs::create_dir(root.join("folder")).unwrap();
        fs::write(root.join("folder/note.md"), "# Hello").unwrap();
        book.settings.last_file = Some("folder/note.md".into());
        book.settings.expanded.insert("folder".into());
        write_json(&settings_path(&root), &book.settings).unwrap();
        let reopened = Notebook::open(&root, false).unwrap();
        assert_eq!(reopened.settings.last_file, book.settings.last_file);
        assert_eq!(reopened.settings.expanded, book.settings.expanded);
        let mut index = Index::default();
        index.register(book.root.clone());
        index.register(Notebook::open(&root.join("."), false).unwrap().root);
        assert_eq!(index.notebooks.len(), 1);
        let index_file = temp.path().join("app/notebooks.json");
        write_json(&index_file, &index).unwrap();
        assert_eq!(load_index(&index_file).unwrap().active, Some(book.root));
    }

    #[test]
    fn corrupt_and_future_settings_are_preserved() {
        let temp = tempfile::tempdir().unwrap();
        Notebook::open(temp.path(), false).unwrap();
        let path = settings_path(temp.path());
        for bytes in [
            "broken json",
            r#"{"version":999,"last_file":null,"expanded":[]}"#,
            r#"{"version":1,"last_file":"../outside.md","expanded":[]}"#,
        ] {
            fs::write(&path, bytes).unwrap();
            assert!(Notebook::open(temp.path(), false).is_err());
            assert_eq!(fs::read_to_string(&path).unwrap(), bytes);
        }
        assert!(load_index(&path).is_err());
        assert!(
            load_index(&temp.path().join("missing.json"))
                .unwrap()
                .notebooks
                .is_empty()
        );
    }

    #[test]
    fn directory_listing_and_safe_paths() {
        let temp = tempfile::tempdir().unwrap();
        Notebook::open(temp.path(), false).unwrap();
        fs::create_dir_all(temp.path().join("z-folder/empty")).unwrap();
        fs::write(temp.path().join("b.txt"), "text").unwrap();
        fs::write(temp.path().join("A.MD"), "# Note").unwrap();
        let entries = read_directory(temp.path(), Path::new("")).unwrap();
        assert_eq!(
            entries
                .iter()
                .map(|e| name(&e.relative))
                .collect::<Vec<_>>(),
            ["z-folder", "A.MD", "b.txt"]
        );
        assert!(
            read_directory(temp.path(), Path::new("z-folder/empty"))
                .unwrap()
                .is_empty()
        );
        assert!(read_directory(temp.path(), Path::new("missing")).is_err());
        assert!(resolve(temp.path(), Path::new("../outside.md")).is_err());
        assert!(resolve(temp.path(), Path::new(".gpui-note/settings.json")).is_err());
        assert!(markdown(Path::new("A.MD")));
        assert!(!markdown(Path::new("b.txt")));
    }

    #[test]
    fn persistence_does_not_recreate_missing_notebook_or_overwrite_corruption() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("Notebook");
        let book = Notebook::open(&root, true).unwrap();
        let index_path = temp.path().join("index.json");
        let mut index = Index::default();
        index.register(book.root.clone());
        save_state(&index_path, &index, Some(&book)).unwrap();
        fs::write(settings_path(&root), "damaged").unwrap();
        assert!(save_state(&index_path, &index, Some(&book)).is_err());
        assert_eq!(fs::read_to_string(settings_path(&root)).unwrap(), "damaged");
        fs::rename(&root, temp.path().join("Moved")).unwrap();
        assert!(save_state(&index_path, &index, Some(&book)).is_err());
        assert!(!root.exists());
        fs::write(&index_path, "damaged index").unwrap();
        assert!(save_state(&index_path, &index, None).is_err());
        assert_eq!(fs::read_to_string(index_path).unwrap(), "damaged index");
    }

    #[cfg(windows)]
    #[test]
    fn junctions_are_listed_but_never_traversed() {
        use std::process::Command;
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("Notebook");
        let book = Notebook::open(&root, true).unwrap();
        let link = root.join("loop");
        // Junction creation does not require Windows developer mode or elevation.
        let script = "New-Item -ItemType Junction -Path $env:GPUI_TEST_LINK -Target $env:GPUI_TEST_TARGET | Out-Null";
        let result = Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", script])
            .env("GPUI_TEST_LINK", &link)
            .env("GPUI_TEST_TARGET", &root)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let entries = read_directory(&book.root, Path::new("")).unwrap();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].link);
        assert!(read_directory(&book.root, Path::new("loop")).is_err());
        assert!(resolve(&book.root, Path::new("loop/note.md")).is_err());
        // Remove the junction itself, never its target.
        fs::remove_dir(link).unwrap();
        assert!(root.exists());
    }
}
