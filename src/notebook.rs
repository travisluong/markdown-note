use std::{
    collections::BTreeSet,
    fs,
    io::{self, Write},
    path::{Component, Path, PathBuf},
};

use serde::{Deserialize, Serialize};

const VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Autosave {
    pub enabled: bool,
    pub delay_ms: u64,
}
impl Default for Autosave {
    fn default() -> Self {
        Self {
            enabled: true,
            delay_ms: 1_000,
        }
    }
}
impl Autosave {
    pub fn normalized(mut self) -> Self {
        self.delay_ms = self.delay_ms.clamp(100, 60_000);
        self
    }
    pub fn parse_delay(value: &str) -> Option<u64> {
        value
            .trim()
            .parse()
            .ok()
            .filter(|value| (100..=60_000).contains(value))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Appearance {
    pub content_font_size: u16,
    pub ui_font_size: u16,
    pub dark_mode: Option<bool>,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            content_font_size: 16,
            ui_font_size: 16,
            dark_mode: None,
        }
    }
}

impl Appearance {
    pub fn normalized(mut self) -> Self {
        self.content_font_size = self.content_font_size.clamp(10, 40);
        self.ui_font_size = self.ui_font_size.clamp(10, 24);
        self
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub entry_order: Vec<PathBuf>,
    #[serde(default)]
    pub nested_list_letters: bool,
    #[serde(default)]
    pub view_mode: crate::ViewMode,
    #[serde(default = "default_sidebar_width")]
    pub sidebar_width: f32,
    #[serde(default = "crate::templates::default_format")]
    pub date_format: String,
    pub version: u32,
    pub last_file: Option<PathBuf>,
    pub expanded: BTreeSet<PathBuf>,
    #[serde(default)]
    pub appearance: Appearance,
    #[serde(default)]
    pub autosave: Autosave,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            entry_order: Vec::new(),
            nested_list_letters: false,
            view_mode: crate::ViewMode::default(),
            sidebar_width: default_sidebar_width(),
            version: VERSION,
            last_file: None,
            date_format: crate::templates::default_format(),
            expanded: BTreeSet::new(),
            appearance: Appearance::default(),
            autosave: Autosave::default(),
        }
    }
}

pub fn default_sidebar_width() -> f32 {
    280.
}

pub fn normalize_sidebar_width(width: f32) -> f32 {
    if width.is_finite() {
        width.clamp(160., 600.)
    } else {
        default_sidebar_width()
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
    pub fn forget(&mut self, path: &Path) {
        self.notebooks.retain(|entry| entry != path);
        if self.active.as_deref() == Some(path) {
            self.active = None;
        }
    }

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
        let mut settings = existing.clone().unwrap_or_default();
        settings.sidebar_width = normalize_sidebar_width(settings.sidebar_width);
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
    fn invalid_view_mode_does_not_overwrite_settings() {
        let fixture = tempfile::tempdir().unwrap();
        let root = fixture.path().join("book");
        let mut book = Notebook::open(&root, true).unwrap();
        let path = settings_path(&root);
        let mut json = serde_json::to_value(&book.settings).unwrap();
        json["view_mode"] = "future_mode".into();
        let saved = serde_json::to_string(&json).unwrap();
        fs::write(&path, &saved).unwrap();
        book.settings.view_mode = crate::ViewMode::Source;
        assert!(
            save_state(
                &fixture.path().join("index.json"),
                &Index::default(),
                Some(&book)
            )
            .is_err()
        );
        assert_eq!(fs::read_to_string(path).unwrap(), saved);
    }

    #[test]
    fn view_modes_default_and_persist_independently() {
        let old: Settings =
            serde_json::from_str(r#"{"version":1,"last_file":null,"expanded":[]}"#).unwrap();
        assert_eq!(old.view_mode, crate::ViewMode::LivePreview);
        let fixture = tempfile::tempdir().unwrap();
        for (name, mode, serialized) in [
            ("source", crate::ViewMode::Source, "source"),
            ("live", crate::ViewMode::LivePreview, "live_preview"),
            ("reading", crate::ViewMode::Reading, "reading"),
        ] {
            let root = fixture.path().join(name);
            let mut book = Notebook::open(&root, true).unwrap();
            assert_eq!(book.settings.view_mode, crate::ViewMode::LivePreview);
            book.settings.view_mode = mode;
            write_json(&settings_path(&root), &book.settings).unwrap();
            assert_eq!(
                Notebook::open(&root, false).unwrap().settings.view_mode,
                mode
            );
            assert_eq!(
                serde_json::to_value(&book.settings).unwrap()["view_mode"],
                serialized
            );
        }
        assert_eq!(
            Notebook::open(&fixture.path().join("source"), false)
                .unwrap()
                .settings
                .view_mode,
            crate::ViewMode::Source
        );
        assert_eq!(
            Notebook::open(&fixture.path().join("reading"), false)
                .unwrap()
                .settings
                .view_mode,
            crate::ViewMode::Reading
        );
    }

    #[test]
    fn sidebar_width_compatibility_normalization_and_persistence() {
        let old: Settings =
            serde_json::from_str(r#"{"version":1,"last_file":null,"expanded":[]}"#).unwrap();
        assert_eq!(old.sidebar_width, 280.);
        assert_eq!(normalize_sidebar_width(-1.), 160.);
        assert_eq!(normalize_sidebar_width(900.), 600.);
        assert_eq!(normalize_sidebar_width(f32::NAN), 280.);
        assert_eq!(normalize_sidebar_width(f32::INFINITY), 280.);
        let fixture = tempfile::tempdir().unwrap();
        for (name, saved, expected) in [
            ("First", 375.5, 375.5),
            ("Second", 210., 210.),
            ("Small", 0., 160.),
            ("Large", 999., 600.),
        ] {
            let root = fixture.path().join(name);
            let mut book = Notebook::open(&root, true).unwrap();
            book.settings.sidebar_width = saved;
            write_json(&settings_path(&root), &book.settings).unwrap();
            assert_eq!(
                Notebook::open(&root, false).unwrap().settings.sidebar_width,
                expected
            );
        }
        assert_eq!(
            Notebook::open(&fixture.path().join("First"), false)
                .unwrap()
                .settings
                .sidebar_width,
            375.5
        );
    }

    #[test]
    fn appearance_compatibility_and_round_trip() {
        let old: Settings =
            serde_json::from_str(r#"{"version":1,"last_file":null,"expanded":[]}"#).unwrap();
        assert_eq!(old.autosave, Autosave::default());
        assert_eq!(
            serde_json::from_str::<Autosave>(r#"{"enabled":false}"#)
                .unwrap()
                .delay_ms,
            1000
        );
        for value in ["", "99", "60001", "1.5", "-100", "abc"] {
            assert_eq!(Autosave::parse_delay(value), None);
        }
        for value in [100, 1000, 60000] {
            assert_eq!(Autosave::parse_delay(&value.to_string()), Some(value));
        }
        let settings = Autosave {
            enabled: false,
            delay_ms: 2500,
        };
        assert_eq!(
            serde_json::from_str::<Autosave>(&serde_json::to_string(&settings).unwrap()).unwrap(),
            settings
        );
        let old: Settings =
            serde_json::from_str(r#"{"version":1,"last_file":null,"expanded":[]}"#).unwrap();
        assert_eq!(old.appearance, Appearance::default());
        let partial: Appearance = serde_json::from_str(r#"{"dark_mode":true}"#).unwrap();
        assert_eq!(partial.content_font_size, 16);
        assert_eq!(partial.ui_font_size, 16);
        assert_eq!(partial.dark_mode, Some(true));
        let bounds = Appearance {
            content_font_size: 0,
            ui_font_size: 500,
            dark_mode: Some(false),
        }
        .normalized();
        assert_eq!((bounds.content_font_size, bounds.ui_font_size), (10, 24));
        let temp = tempfile::tempdir().unwrap();
        let mut book = Notebook::open(temp.path(), false).unwrap();
        book.settings.appearance = Appearance {
            content_font_size: 40,
            ui_font_size: 24,
            dark_mode: Some(true),
        };
        write_json(&settings_path(temp.path()), &book.settings).unwrap();
        assert_eq!(
            Notebook::open(temp.path(), false)
                .unwrap()
                .settings
                .appearance,
            book.settings.appearance
        );
        let path = settings_path(temp.path());
        fs::write(
            &path,
            r#"{"version":1,"last_file":null,"expanded":[],"appearance":{"dark_mode":"invalid"}}"#,
        )
        .unwrap();
        let original = fs::read(&path).unwrap();
        assert!(
            save_state(
                &temp.path().join("index.json"),
                &Index::default(),
                Some(&book)
            )
            .is_err()
        );
        assert_eq!(fs::read(path).unwrap(), original);
    }

    #[test]
    fn forget_preserves_files_and_other_notebooks() {
        let temp = tempfile::tempdir().unwrap();
        let first = temp.path().join("First");
        let second = temp.path().join("Second");
        Notebook::open(&first, true).unwrap();
        fs::write(first.join("note.md"), "keep me").unwrap();
        let mut index = Index::default();
        index.register(first.clone());
        index.register(second.clone());
        index.forget(&first);
        assert_eq!(index.active, Some(second.clone()));
        assert_eq!(
            fs::read_to_string(first.join("note.md")).unwrap(),
            "keep me"
        );
        index.forget(&second);
        assert!(index.active.is_none());
        assert!(index.notebooks.is_empty());
        let path = temp.path().join("index.json");
        save_state(&path, &index, None).unwrap();
        assert!(load_index(&path).unwrap().notebooks.is_empty());
    }

    #[test]
    fn create_reopen_and_persist() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("Notes");
        let mut book = Notebook::open(&root, true).unwrap();
        assert_eq!(book.settings.date_format, "YYYY-MM-DD");
        book.settings.date_format = "MMMM Do, YYYY".into();
        assert!(Notebook::open(&root, true).is_err());
        fs::create_dir(root.join("folder")).unwrap();
        fs::write(root.join("folder/note.md"), "# Hello").unwrap();
        book.settings.last_file = Some("folder/note.md".into());
        book.settings.expanded.insert("folder".into());
        write_json(&settings_path(&root), &book.settings).unwrap();
        let reopened = Notebook::open(&root, false).unwrap();
        assert_eq!(reopened.settings.date_format, "MMMM Do, YYYY");
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

/// A display-only spelling; filesystem operations and stored identities keep the original path.
pub fn display_path(path: &Path) -> String {
    let value = path.to_string_lossy();
    if let Some(unc) = value.strip_prefix(r"\\?\UNC\") {
        if unc
            .split_once('\\')
            .is_some_and(|(server, share)| !server.is_empty() && !share.is_empty())
        {
            return format!(r"\\{unc}");
        }
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

#[cfg(test)]
mod path_display_tests {
    use super::*;
    #[test]
    fn only_normalize_verbatim_drive_and_unc_for_display() {
        for (input, expected) in [
            (r"\\?\C:\Notes\你好", r"C:\Notes\你好"),
            (r"\\?\z:\", r"z:\"),
            (r"\\?\UNC\server\share\Notes", r"\\server\share\Notes"),
            (r"\\server\share\Notes", r"\\server\share\Notes"),
            (r"C:\Notes", r"C:\Notes"),
            (r"relative\Notes", r"relative\Notes"),
            (r"\\?\Volume{abc}\Notes", r"\\?\Volume{abc}\Notes"),
            (r"\\.\C:\Notes", r"\\.\C:\Notes"),
            (r"\\?\C:relative", r"\\?\C:relative"),
            (r"\\?\UNC\server", r"\\?\UNC\server"),
            ("/home/notes", "/home/notes"),
        ] {
            let path = PathBuf::from(input);
            assert_eq!(display_path(&path), expected);
            assert_eq!(path.to_string_lossy(), input);
        }
    }
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

pub fn remap_path(path: &Path, from: &Path, to: &Path) -> PathBuf {
    path.strip_prefix(from)
        .map_or_else(|_| path.to_path_buf(), |suffix| to.join(suffix))
}

impl Settings {
    pub fn reorder_entry(
        &mut self,
        from: &Path,
        to: &Path,
        before: Option<&Path>,
        siblings: Vec<PathBuf>,
    ) {
        self.expanded = self
            .expanded
            .iter()
            .map(|p| remap_path(p, from, to))
            .collect();
        self.last_file = self.last_file.as_ref().map(|p| remap_path(p, from, to));
        self.entry_order = self
            .entry_order
            .iter()
            .map(|p| remap_path(p, from, to))
            .collect();
        self.entry_order
            .retain(|p| p != to && p.parent() != to.parent());
        let mut siblings: Vec<_> = siblings
            .into_iter()
            .filter(|p| p != from && p != to)
            .collect();
        let at = before
            .and_then(|b| siblings.iter().position(|p| p == b))
            .unwrap_or(siblings.len());
        siblings.insert(at, to.to_path_buf());
        self.entry_order.extend(siblings);
        if let Some(parent) = to.parent().filter(|p| !p.as_os_str().is_empty()) {
            self.expanded.insert(parent.to_path_buf());
        }
    }
    pub fn sort_entries(&self, entries: &mut [Entry]) {
        entries.sort_by_key(|entry| {
            self.entry_order
                .iter()
                .position(|p| p == &entry.relative)
                .unwrap_or(usize::MAX)
        });
    }
}

#[cfg(test)]
mod move_tests {
    use super::*;
    #[test]
    fn move_reorder_and_preserve_paths() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir(root.join("folder")).unwrap();
        fs::write(root.join("a.md"), "unsaved lives in the editor").unwrap();
        let destination = move_entry(root, Path::new("a.md"), Path::new("folder")).unwrap();
        assert_eq!(destination, Path::new("folder/a.md"));
        assert!(!root.join("a.md").exists());
        assert_eq!(
            move_entry(root, &destination, Path::new("")).unwrap(),
            Path::new("a.md")
        );
        assert!(move_entry(root, Path::new("folder"), Path::new("folder")).is_err());
        fs::write(root.join("folder/a.md"), "collision").unwrap();
        assert!(move_entry(root, Path::new("a.md"), Path::new("folder")).is_err());
        assert_eq!(
            fs::read_to_string(root.join("folder/a.md")).unwrap(),
            "collision"
        );
        assert!(move_entry(root, Path::new("../outside"), Path::new("")).is_err());
        let mut settings = Settings::default();
        settings.last_file = Some(PathBuf::from("folder/a.md"));
        settings.expanded.insert(PathBuf::from("folder/sub"));
        settings.reorder_entry(
            Path::new("folder"),
            Path::new("new/folder"),
            Some(Path::new("new/z.md")),
            vec!["new/z.md".into()],
        );
        assert_eq!(
            settings.last_file.unwrap(),
            PathBuf::from("new/folder/a.md")
        );
        assert!(settings.expanded.contains(Path::new("new/folder/sub")));
        assert_eq!(
            settings.entry_order,
            vec![PathBuf::from("new/folder"), PathBuf::from("new/z.md")]
        );
    }
}
