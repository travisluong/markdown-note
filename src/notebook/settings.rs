//! Persisted preference and index schemas, compatibility defaults, and pure updates.
//! This module does not read or write files; keep serialized field names stable.

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use super::{
    paths::remap_path,
    tree::{Entry, FileSort},
};

pub(super) const VERSION: u32 = 1;

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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Appearance {
    pub theme: Option<ThemeChoice>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub light: Option<ThemePalette>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dark: Option<ThemePalette>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub background_color: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub font_color: Option<u32>,
    pub interface_font: Option<String>,
    pub text_font: Option<String>,
    pub monospace_font: Option<String>,
    pub content_font_size: u16,
    pub ui_font_size: u16,
    pub centered_editor: bool,
    pub centered_editor_width: u16,
    pub dark_mode: Option<bool>,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            theme: None,
            light: None,
            dark: None,
            background_color: None,
            font_color: None,
            interface_font: None,
            text_font: None,
            monospace_font: None,
            content_font_size: 16,
            ui_font_size: 16,
            centered_editor: false,
            centered_editor_width: 700,
            dark_mode: None,
        }
    }
}

impl Appearance {
    pub fn parse_editor_width(value: &str) -> Option<u16> {
        value
            .trim()
            .parse()
            .ok()
            .filter(|width| (200..=4000).contains(width))
    }

    pub fn resolved(mut self, startup_dark: bool) -> Self {
        self = self.normalized();
        let choice = self.theme_choice(startup_dark);
        let legacy = self.theme == Some(ThemeChoice::DarkGray)
            || self.background_color.is_some()
            || self.font_color.is_some();
        if legacy {
            let mut palette = if self.theme == Some(ThemeChoice::DarkGray) {
                ThemePalette {
                    background: 0x303030,
                    ..ThemePalette::DARK
                }
            } else if choice == ThemeChoice::Light {
                ThemePalette::LIGHT
            } else {
                ThemePalette::DARK
            };
            palette.background = self.background_color.unwrap_or(palette.background);
            palette.font = self.font_color.unwrap_or(palette.font);
            palette.primary = palette.font;
            let target = if choice == ThemeChoice::Light {
                &mut self.light
            } else {
                &mut self.dark
            };
            target.get_or_insert(palette);
        }
        self.light = Some(
            self.light
                .unwrap_or(ThemePalette::LIGHT)
                .normalized(ThemePalette::LIGHT),
        );
        self.dark = Some(
            self.dark
                .unwrap_or(ThemePalette::DARK)
                .normalized(ThemePalette::DARK),
        );
        if self.theme == Some(ThemeChoice::DarkGray) {
            self.theme = Some(ThemeChoice::Dark);
        }
        self.background_color = None;
        self.font_color = None;
        self
    }

    pub fn palette(&self, startup_dark: bool) -> ThemePalette {
        let resolved = self.clone().resolved(startup_dark);
        if resolved.theme_choice(startup_dark) == ThemeChoice::Light {
            resolved.light.unwrap()
        } else {
            resolved.dark.unwrap()
        }
    }

    pub fn set_palette(&mut self, startup_dark: bool, palette: ThemePalette) {
        *self = self.clone().resolved(startup_dark);
        if self.theme_choice(startup_dark) == ThemeChoice::Light {
            self.light = Some(palette);
        } else {
            self.dark = Some(palette);
        }
    }

    pub fn reset_palettes(&mut self) {
        self.light = Some(ThemePalette::LIGHT);
        self.dark = Some(ThemePalette::DARK);
        self.background_color = None;
        self.font_color = None;
        if self.theme == Some(ThemeChoice::DarkGray) {
            self.theme = Some(ThemeChoice::Dark);
        }
    }

    pub fn normalized(mut self) -> Self {
        self.background_color = self.background_color.filter(|c| *c <= 0xffffff);
        self.font_color = self.font_color.filter(|c| *c <= 0xffffff);
        self.content_font_size = self.content_font_size.clamp(10, 40);
        self.ui_font_size = self.ui_font_size.clamp(10, 24);
        self.centered_editor_width = self.centered_editor_width.clamp(200, 4000);
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThemePalette {
    pub background: u32,
    pub font: u32,
    pub primary: u32,
    #[serde(default = "missing_palette_color")]
    pub border: u32,
    #[serde(default = "missing_palette_color")]
    pub icon: u32,
    #[serde(default = "missing_palette_color")]
    pub secondary: u32,
}

// Resolve older palettes against their own mode, rather than assuming Light.
fn missing_palette_color() -> u32 {
    u32::MAX
}

impl ThemePalette {
    pub const LIGHT: Self = Self {
        background: 0xffffff,
        font: 0x202020,
        primary: 0x202020,
        border: 0xe5e5e5,
        icon: 0x202020,
        // 12% built-in primary over 88% built-in background, rounded per channel.
        secondary: 0xe4e4e4,
    };
    pub const DARK: Self = Self {
        background: 0x202020,
        font: 0xe6e6e6,
        primary: 0xe6e6e6,
        border: 0x404040,
        icon: 0xe6e6e6,
        secondary: 0x383838,
    };

    fn normalized(self, defaults: Self) -> Self {
        Self {
            background: if self.background <= 0xffffff {
                self.background
            } else {
                defaults.background
            },
            font: if self.font <= 0xffffff {
                self.font
            } else {
                defaults.font
            },
            primary: if self.primary <= 0xffffff {
                self.primary
            } else {
                defaults.primary
            },
            border: if self.border <= 0xffffff {
                self.border
            } else {
                defaults.border
            },
            icon: if self.icon <= 0xffffff {
                self.icon
            } else {
                defaults.icon
            },
            secondary: if self.secondary <= 0xffffff {
                self.secondary
            } else {
                defaults.secondary
            },
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolbarAlignment {
    #[default]
    Left,
    Center,
    Right,
}

impl ToolbarAlignment {
    pub const OPTIONS: [(Self, &'static str); 3] = [
        (Self::Left, "Left"),
        (Self::Center, "Center"),
        (Self::Right, "Right"),
    ];
}

fn deserialize_toolbar_alignment<'de, D>(deserializer: D) -> Result<ToolbarAlignment, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(serde_json::from_value(value).unwrap_or_default())
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolbarIconSize {
    Small,
    #[default]
    Medium,
    Large,
}

impl ToolbarIconSize {
    pub const OPTIONS: [(Self, &'static str); 3] = [
        (Self::Small, "Small"),
        (Self::Medium, "Medium"),
        (Self::Large, "Large"),
    ];

    pub fn icon_pixels(self) -> f32 {
        match self {
            Self::Small => 14.,
            Self::Medium => 20.,
            Self::Large => 28.,
        }
    }

    pub fn button_pixels(self) -> f32 {
        match self {
            Self::Small => 24.,
            Self::Medium => 32.,
            Self::Large => 40.,
        }
    }
}

fn deserialize_toolbar_icon_size<'de, D>(deserializer: D) -> Result<ToolbarIconSize, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(serde_json::from_value(value).unwrap_or_default())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolbarItem {
    Sidebar,
    Explorer,
    Search,
    Sort,
    Bold,
    Italic,
    Strikethrough,
    Heading,
    InlineCode,
    CodeBlock,
    Link,
    Blockquote,
    OrderedList,
    UnorderedList,
    TaskList,
    Back,
    Forward,
    Terminal,
}

impl ToolbarItem {
    pub fn region(self) -> ToolbarRegion {
        match self {
            Self::Sidebar | Self::Explorer | Self::Search | Self::Sort => ToolbarRegion::Sidebar,
            Self::Back | Self::Forward => ToolbarRegion::ContentLeft,
            Self::Terminal => ToolbarRegion::ContentRight,
            _ => ToolbarRegion::ContentCenter,
        }
    }

    pub const DEFAULT_ORDER: [Self; 18] = [
        Self::Sidebar,
        Self::Back,
        Self::Forward,
        Self::Explorer,
        Self::Search,
        Self::Sort,
        Self::Bold,
        Self::Italic,
        Self::Strikethrough,
        Self::Heading,
        Self::InlineCode,
        Self::CodeBlock,
        Self::Link,
        Self::Blockquote,
        Self::OrderedList,
        Self::UnorderedList,
        Self::TaskList,
        Self::Terminal,
    ];

    pub fn label(self) -> &'static str {
        Self::OPTIONS
            .into_iter()
            .find(|(item, _)| *item == self)
            .unwrap()
            .1
    }

    pub const OPTIONS: [(Self, &'static str); 18] = [
        (Self::Sidebar, "Toggle sidebar"),
        (Self::Explorer, "Explorer"),
        (Self::Search, "Search"),
        (Self::Sort, "Sort files"),
        (Self::Bold, "Bold"),
        (Self::Italic, "Italic"),
        (Self::Strikethrough, "Strikethrough"),
        (Self::Heading, "Heading"),
        (Self::InlineCode, "Inline code"),
        (Self::CodeBlock, "Code block"),
        (Self::Link, "Insert / Edit Link"),
        (Self::Blockquote, "Blockquote"),
        (Self::OrderedList, "Ordered list"),
        (Self::UnorderedList, "Unordered list"),
        (Self::TaskList, "Task list"),
        (Self::Back, "Back"),
        (Self::Forward, "Forward"),
        (Self::Terminal, "Terminal"),
    ];
}

/// Placement is fixed; customization only changes order inside a region.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolbarRegion {
    Sidebar,
    ContentLeft,
    ContentCenter,
    ContentRight,
}

impl ToolbarRegion {
    pub const OPTIONS: [(Self, &'static str); 4] = [
        (Self::Sidebar, "Sidebar"),
        (Self::ContentLeft, "Content left"),
        (Self::ContentCenter, "Content center"),
        (Self::ContentRight, "Content right"),
    ];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ToolbarSettings {
    pub visible: bool,
    /// Top-bar controls are independent of workspace toolbar visibility and sizing.
    pub title_bar_sidebar: bool,
    pub title_bar_terminal: bool,
    #[serde(deserialize_with = "deserialize_toolbar_alignment")]
    pub alignment: ToolbarAlignment,
    #[serde(deserialize_with = "deserialize_toolbar_icon_size")]
    pub icon_size: ToolbarIconSize,
    #[serde(deserialize_with = "deserialize_toolbar_order")]
    pub order: [ToolbarItem; 18],
    pub back: bool,
    pub forward: bool,
    pub sidebar: bool,
    pub explorer: bool,
    pub search: bool,
    pub sort: bool,
    pub bold: bool,
    pub italic: bool,
    pub strikethrough: bool,
    pub heading: bool,
    pub inline_code: bool,
    pub code_block: bool,
    pub link: bool,
    pub blockquote: bool,
    pub ordered_list: bool,
    pub unordered_list: bool,
    pub task_list: bool,
    pub terminal: bool,
}

impl Default for ToolbarSettings {
    fn default() -> Self {
        Self {
            visible: true,
            title_bar_sidebar: false,
            title_bar_terminal: false,
            alignment: ToolbarAlignment::default(),
            icon_size: ToolbarIconSize::default(),
            order: ToolbarItem::DEFAULT_ORDER,
            back: true,
            forward: true,
            sidebar: true,
            explorer: true,
            search: true,
            sort: true,
            bold: true,
            italic: true,
            strikethrough: true,
            heading: true,
            inline_code: true,
            code_block: true,
            link: true,
            blockquote: true,
            ordered_list: true,
            unordered_list: true,
            task_list: true,
            terminal: true,
        }
    }
}

impl ToolbarSettings {
    /// Insert an item on either side of a target without changing visibility.
    /// Returns false when the item already occupies that insertion position.
    pub fn move_item(&mut self, item: ToolbarItem, target: ToolbarItem, after: bool) -> bool {
        if item == target || item.region() != target.region() {
            return false;
        }
        let Some(from) = self.order.iter().position(|entry| *entry == item) else {
            return false;
        };
        let Some(target) = self.order.iter().position(|entry| *entry == target) else {
            return false;
        };
        let insertion = target + usize::from(after);
        let to = insertion - usize::from(from < insertion);
        if from == to {
            return false;
        }
        if from < to {
            self.order[from..=to].rotate_left(1);
        } else {
            self.order[to..=from].rotate_right(1);
        }
        true
    }

    pub fn enabled(self, item: ToolbarItem) -> bool {
        match item {
            ToolbarItem::Back => self.back,
            ToolbarItem::Forward => self.forward,
            ToolbarItem::Sidebar => self.sidebar,
            ToolbarItem::Explorer => self.explorer,
            ToolbarItem::Search => self.search,
            ToolbarItem::Sort => self.sort,
            ToolbarItem::Bold => self.bold,
            ToolbarItem::Italic => self.italic,
            ToolbarItem::Strikethrough => self.strikethrough,
            ToolbarItem::Heading => self.heading,
            ToolbarItem::InlineCode => self.inline_code,
            ToolbarItem::CodeBlock => self.code_block,
            ToolbarItem::Link => self.link,
            ToolbarItem::Blockquote => self.blockquote,
            ToolbarItem::OrderedList => self.ordered_list,
            ToolbarItem::UnorderedList => self.unordered_list,
            ToolbarItem::TaskList => self.task_list,
            ToolbarItem::Terminal => self.terminal,
        }
    }

    pub fn set_enabled(&mut self, item: ToolbarItem, enabled: bool) {
        *match item {
            ToolbarItem::Back => &mut self.back,
            ToolbarItem::Forward => &mut self.forward,
            ToolbarItem::Sidebar => &mut self.sidebar,
            ToolbarItem::Explorer => &mut self.explorer,
            ToolbarItem::Search => &mut self.search,
            ToolbarItem::Sort => &mut self.sort,
            ToolbarItem::Bold => &mut self.bold,
            ToolbarItem::Italic => &mut self.italic,
            ToolbarItem::Strikethrough => &mut self.strikethrough,
            ToolbarItem::Heading => &mut self.heading,
            ToolbarItem::InlineCode => &mut self.inline_code,
            ToolbarItem::CodeBlock => &mut self.code_block,
            ToolbarItem::Link => &mut self.link,
            ToolbarItem::Blockquote => &mut self.blockquote,
            ToolbarItem::OrderedList => &mut self.ordered_list,
            ToolbarItem::UnorderedList => &mut self.unordered_list,
            ToolbarItem::TaskList => &mut self.task_list,
            ToolbarItem::Terminal => &mut self.terminal,
        } = enabled;
    }

    pub fn any_enabled(self) -> bool {
        ToolbarItem::DEFAULT_ORDER
            .into_iter()
            .any(|item| self.enabled(item))
    }
}

fn deserialize_toolbar_order<'de, D>(deserializer: D) -> Result<[ToolbarItem; 18], D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    let mut order = Vec::new();
    if let Some(items) = value.as_array() {
        for value in items {
            if let Ok(item) = serde_json::from_value::<ToolbarItem>(value.clone())
                && !order.contains(&item)
            {
                order.push(item);
            }
        }
    }
    for item in ToolbarItem::DEFAULT_ORDER {
        if !order.contains(&item) {
            order.push(item);
        }
    }
    Ok(order.try_into().unwrap())
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub terminal: crate::terminal::TerminalSettings,
    #[serde(default)]
    pub autosave_external_files: bool,
    #[serde(default)]
    pub toolbar: ToolbarSettings,
    #[serde(default)]
    pub excluded_patterns: Vec<String>,
    #[serde(default = "default_show_file_extensions")]
    pub show_file_extensions: bool,
    #[serde(default = "default_always_hide_extensions")]
    pub always_hide_extensions: Vec<String>,
    #[serde(default)]
    pub file_sort: FileSort,
    #[serde(default)]
    pub view_mode: crate::ViewMode,
    #[serde(default = "default_sidebar_width")]
    pub sidebar_width: f32,
    #[serde(default = "default_terminal_width")]
    pub terminal_width: f32,
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
            terminal: Default::default(),
            toolbar: ToolbarSettings::default(),
            autosave_external_files: false,
            excluded_patterns: Vec::new(),
            file_sort: FileSort::default(),
            show_file_extensions: default_show_file_extensions(),
            always_hide_extensions: default_always_hide_extensions(),
            view_mode: crate::ViewMode::default(),
            sidebar_width: default_sidebar_width(),
            terminal_width: default_terminal_width(),
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

pub fn default_terminal_width() -> f32 {
    280.
}

pub fn normalize_terminal_width(width: f32) -> f32 {
    if width.is_finite() {
        width.max(160.)
    } else {
        default_terminal_width()
    }
}

pub fn default_show_file_extensions() -> bool {
    true
}

pub fn default_always_hide_extensions() -> Vec<String> {
    vec!["md".into()]
}

pub fn parse_hidden_extensions(value: &str) -> Result<Vec<String>, String> {
    let mut extensions = Vec::new();
    for token in value
        .split(',')
        .map(str::trim)
        .filter(|token| !token.is_empty())
    {
        let extension = token.strip_prefix('.').unwrap_or(token);
        if extension.is_empty()
            || extension
                .chars()
                .any(|c| c.is_whitespace() || matches!(c, '.' | '/' | '\\' | '*' | '?' | '[' | ']'))
        {
            return Err("Enter extensions separated by commas, without paths, spaces, wildcards, or compound suffixes.".into());
        }
        let extension = extension.to_lowercase();
        if !extensions.contains(&extension) {
            extensions.push(extension);
        }
    }
    Ok(extensions)
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

impl Settings {
    pub fn remap_entry(&mut self, from: &Path, to: &Path) {
        self.expanded = self
            .expanded
            .iter()
            .map(|p| remap_path(p, from, to))
            .collect();
        self.last_file = self.last_file.as_ref().map(|p| remap_path(p, from, to));
        if let Some(parent) = to.parent().filter(|p| !p.as_os_str().is_empty()) {
            self.expanded.insert(parent.to_path_buf());
        }
    }
    pub fn sort_entries(&self, entries: &mut [Entry]) {
        self.file_sort.sort_entries(entries);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemeChoice {
    Light,
    Dark,
    DarkGray,
}
impl Appearance {
    pub fn theme_choice(&self, startup_dark: bool) -> ThemeChoice {
        self.theme
            .map(|choice| {
                if choice == ThemeChoice::DarkGray {
                    ThemeChoice::Dark
                } else {
                    choice
                }
            })
            .unwrap_or(if self.dark_mode.unwrap_or(startup_dark) {
                ThemeChoice::Dark
            } else {
                ThemeChoice::Light
            })
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod palette_tests {
    use super::{Appearance, ThemeChoice, ThemePalette};

    #[test]
    fn old_and_invalid_colors_use_their_palette_mode_defaults() {
        for border in [None, Some(0x1000000u32), Some(u32::MAX)] {
            let mut palette =
                serde_json::json!({"background": 0x123456, "font": 0x234567, "primary": 0x345678});
            if let Some(border) = border {
                for field in ["border", "icon", "secondary"] {
                    palette[field] = border.into();
                }
            }
            let json = serde_json::json!({"light": palette, "dark": palette});
            for startup in [false, true] {
                let appearance: Appearance = serde_json::from_value(json.clone()).unwrap();
                let resolved = appearance.resolved(startup);
                assert_eq!(resolved.light.unwrap().border, ThemePalette::LIGHT.border);
                assert_eq!(resolved.dark.unwrap().border, ThemePalette::DARK.border);
                assert_eq!(resolved.light.unwrap().icon, ThemePalette::LIGHT.icon);
                assert_eq!(resolved.dark.unwrap().icon, ThemePalette::DARK.icon);
                assert_eq!(
                    resolved.light.unwrap().secondary,
                    ThemePalette::LIGHT.secondary
                );
                assert_eq!(
                    resolved.dark.unwrap().secondary,
                    ThemePalette::DARK.secondary
                );
                assert_eq!(resolved.light.unwrap().background, 0x123456);
                assert_eq!(resolved.dark.unwrap().primary, 0x345678);
            }
        }
    }

    #[test]
    fn custom_colors_round_trip_independently_and_reset_with_palettes() {
        let mut appearance = Appearance::default().resolved(false);
        for (mode, border) in [(ThemeChoice::Light, 0), (ThemeChoice::Dark, 0xffffff)] {
            appearance.theme = Some(mode);
            let mut palette = appearance.palette(false);
            palette.border = border;
            palette.icon = border;
            palette.secondary = border;
            appearance.set_palette(false, palette);
        }
        let json = serde_json::to_string(&appearance).unwrap();
        let loaded: Appearance = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded, appearance);
        assert_eq!(loaded.light.unwrap().border, 0);
        assert_eq!(loaded.dark.unwrap().border, 0xffffff);
        assert_eq!(loaded.light.unwrap().icon, 0);
        assert_eq!(loaded.dark.unwrap().icon, 0xffffff);
        assert_eq!(loaded.light.unwrap().secondary, 0);
        assert_eq!(loaded.dark.unwrap().secondary, 0xffffff);
        appearance.reset_palettes();
        assert_eq!(appearance.light, Some(ThemePalette::LIGHT));
        assert_eq!(appearance.dark, Some(ThemePalette::DARK));
    }
}

#[cfg(test)]
mod additional_color_defaults_tests {
    use super::{Appearance, ThemeChoice, ThemePalette};

    #[test]
    fn muted_defaults_are_independent_of_font_primary_and_background_edits() {
        for (mode, defaults) in [
            (ThemeChoice::Light, ThemePalette::LIGHT),
            (ThemeChoice::Dark, ThemePalette::DARK),
        ] {
            for shift in [0, 8, 16] {
                let primary = (defaults.primary >> shift) & 255;
                let background = (defaults.background >> shift) & 255;
                assert_eq!(
                    (defaults.secondary >> shift) & 255,
                    (12 * primary + 88 * background + 50) / 100
                );
            }
            assert_eq!(defaults.icon, defaults.font);
            let mut appearance = Appearance {
                theme: Some(mode),
                ..Default::default()
            };
            appearance.set_palette(
                false,
                ThemePalette {
                    primary: 0xff0000,
                    font: 0x00ff00,
                    background: 0x123456,
                    ..defaults
                },
            );
            assert_eq!(appearance.palette(false).icon, defaults.icon);
            assert_eq!(appearance.palette(false).secondary, defaults.secondary);
        }
    }
}
