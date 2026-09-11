use super::*;
use gpui_component::dialog::Dialog;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PaletteMode {
    Commands,
    Files,
    Notebooks,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Command {
    InsertDate,
    EditLink,
    ToggleTheme,
    ChangeNotebook,
    NewNote,
    SearchFile,
}

impl Command {
    const ALL: &[Self] = &[
        Self::InsertDate,
        Self::EditLink,
        Self::ToggleTheme,
        Self::ChangeNotebook,
        Self::NewNote,
        Self::SearchFile,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::InsertDate => "Templates: Insert current date",
            Self::EditLink => "Text: Insert or edit link",
            Self::ToggleTheme => "Appearance: Toggle light/dark mode",
            Self::ChangeNotebook => "Notebook: Change notebook",
            Self::NewNote => "Notebook: Create new note",
            Self::SearchFile => "Search: Search current file",
        }
    }

    fn description(self) -> &'static str {
        match self {
            Self::InsertDate => "Insert at the editor selection",
            Self::EditLink => "Insert a link or edit the selected link",
            Self::ToggleTheme => "Switch between your Light and Dark themes",
            Self::ChangeNotebook => "Choose a registered notebook",
            Self::NewNote => "Create an Untitled note in the notebook root",
            Self::SearchFile => "Find text in the current document",
        }
    }

    fn unavailable(
        self,
        document: bool,
        reading: bool,
        busy: bool,
        notebook: bool,
    ) -> Option<&'static str> {
        if busy {
            return Some("Wait for the current operation to finish");
        }
        match self {
            Self::SearchFile if !document => Some("Open a document to search"),
            Self::NewNote if !notebook => Some("Open a notebook to create a note"),
            Self::InsertDate | Self::EditLink if !document || reading => {
                Some("Open an editable document in Source or Live preview")
            }
            _ => None,
        }
    }
}

#[derive(Clone)]
enum PaletteEntry {
    Command(Command),
    File(notebook::Entry),
    Notebook { path: PathBuf, active: bool },
}

impl PaletteEntry {
    fn label(&self) -> String {
        match self {
            Self::Command(command) => command.label().into(),
            Self::Notebook { path, active } => format!(
                "{}{}",
                notebook::name(path),
                if *active { " (active)" } else { "" }
            ),
            Self::File(entry) => format!(
                "{}{}",
                if entry.directory { "▸ " } else { "" },
                notebook::name(&entry.relative)
            ),
        }
    }

    fn search_name(&self) -> String {
        match self {
            Self::Command(command) => command.label().into(),
            Self::Notebook { path, .. } => notebook::name(path),
            Self::File(entry) => notebook::name(&entry.relative),
        }
    }

    fn detail(&self) -> String {
        match self {
            Self::Command(command) => command.description().into(),
            Self::Notebook { path, .. } => path.display().to_string(),
            Self::File(entry) => entry.relative.display().to_string(),
        }
    }

    fn rank(&self, query: &str) -> Option<(u8, usize)> {
        score(&self.search_name().to_lowercase(), query).or_else(|| match self {
            Self::File(_) | Self::Notebook { .. } => {
                score(&self.detail().to_lowercase(), query).map(|(_, cost)| (4, cost))
            }
            _ => None,
        })
    }
}

pub(super) struct Palette {
    mode: PaletteMode,
    query: Entity<InputState>,
    _subscription: Subscription,
    entries: Vec<PaletteEntry>,
    results: Vec<PaletteEntry>,
    selected: usize,
    scroll: UniformListScrollHandle,
    status: Option<String>,
    focus: Option<FocusHandle>,
}

pub fn discover(root: &Path) -> io::Result<Vec<notebook::Entry>> {
    if notebook::is_link(&fs::symlink_metadata(root)?) {
        return Err(io::Error::other("Notebook links are not traversed."));
    }
    let mut pending = vec![PathBuf::new()];
    let mut found = Vec::new();
    while let Some(path) = pending.pop() {
        for entry in notebook::read_directory(root, &path)? {
            if entry.link {
                continue;
            }
            if entry.directory {
                pending.push(entry.relative.clone());
            }
            if entry.directory || notebook::markdown(&entry.relative) {
                found.push(entry);
            }
        }
    }
    Ok(found)
}

fn score(name: &str, query: &str) -> Option<(u8, usize)> {
    if name == query {
        return Some((0, 0));
    }
    if name.starts_with(query) {
        return Some((1, name.len()));
    }
    if let Some(i) = name.find(query) {
        return Some((2, i));
    }
    let mut chars = name.chars().enumerate();
    let mut cost = 0;
    for wanted in query.chars() {
        cost += chars.find(|(_, c)| *c == wanted)?.0;
    }
    Some((3, cost))
}

fn ranked_entries(entries: &[PaletteEntry], query: &str) -> Vec<PaletteEntry> {
    let query = query.trim().to_lowercase();
    let mut ranked: Vec<_> = entries
        .iter()
        .filter_map(|entry| entry.rank(&query).map(|rank| (rank, entry.clone())))
        .collect();
    ranked.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| a.1.search_name().cmp(&b.1.search_name()))
            .then_with(|| a.1.detail().cmp(&b.1.detail()))
    });
    ranked.into_iter().map(|(_, entry)| entry).collect()
}

impl Palette {
    fn filter(&mut self, cx: &App) {
        self.results = ranked_entries(&self.entries, &self.query.read(cx).value());
        self.selected = 0;
        self.scroll.scroll_to_item(0, ScrollStrategy::Top);
    }
}

impl Notes {
    pub(super) fn command_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || window.has_active_dialog(cx) {
            return;
        }
        self.open_palette(
            PaletteMode::Commands,
            Command::ALL
                .iter()
                .copied()
                .map(PaletteEntry::Command)
                .collect(),
            None,
            window,
            cx,
        );
    }

    fn open_palette(
        &mut self,
        mode: PaletteMode,
        entries: Vec<PaletteEntry>,
        status: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let placeholder = match mode {
            PaletteMode::Commands => "Search commands",
            PaletteMode::Files => "Search files and folders",
            PaletteMode::Notebooks => "Search notebooks",
        };
        let query = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
        let subscription = cx.subscribe(&query, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                if let Some(palette) = &mut this.palette {
                    palette.filter(cx);
                }
                cx.notify();
            }
        });
        self.palette = Some(Palette {
            mode,
            query: query.clone(),
            _subscription: subscription,
            entries,
            results: Vec::new(),
            selected: 0,
            scroll: UniformListScrollHandle::new(),
            status,
            focus: window.focused(cx),
        });
        self.palette.as_mut().unwrap().filter(cx);
        let owner = cx.weak_entity();
        window.open_dialog(cx, move |dialog, window, cx| {
            owner
                .update(cx, |this, cx| this.palette_dialog(dialog, window, cx))
                .unwrap_or_else(|_| Dialog::new(window, cx))
        });
        query.update(cx, |query, cx| query.focus(window, cx));
    }

    pub(super) fn go_to(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || window.has_active_dialog(cx) {
            return;
        }
        let Some(book) = &self.notebooks.active else {
            self.request(Next::Open, window, cx);
            return;
        };
        let root = book.root.clone();
        self.open_palette(
            PaletteMode::Files,
            Vec::new(),
            Some("Loading…".into()),
            window,
            cx,
        );
        let query = self.palette.as_ref().unwrap().query.clone();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { discover(&root) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if let Some(palette) = &mut this.palette {
                    if palette.query != query {
                        return;
                    }
                    match result {
                        Ok(entries) => {
                            palette.entries = entries.into_iter().map(PaletteEntry::File).collect();
                            palette.status = None;
                            palette.filter(cx);
                        }
                        Err(error) => {
                            palette.status = Some(format!("Could not scan notebook: {error}"))
                        }
                    }
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn close_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(palette) = self.palette.take() {
            if let Some(focus) = palette.focus {
                focus.focus(window);
            }
        }
        cx.notify();
    }

    fn activate_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let entry = self
            .palette
            .as_ref()
            .and_then(|p| p.results.get(p.selected))
            .cloned();
        let Some(entry) = entry else {
            return;
        };
        if let PaletteEntry::Command(command) = &entry {
            if self.command_unavailable(*command).is_some() {
                return;
            }
        }
        window.close_dialog(cx);
        self.close_palette(window, cx);
        match entry {
            PaletteEntry::Command(command) => self.run_command(command, window, cx),
            PaletteEntry::Notebook { path, active } => {
                if !active {
                    self.request(
                        Next::Notebook {
                            path,
                            create: false,
                        },
                        window,
                        cx,
                    );
                }
            }
            PaletteEntry::File(entry) if entry.directory => {
                self.focus_folder(entry.relative, window, cx)
            }
            PaletteEntry::File(entry) => {
                if let Some(book) = &self.notebooks.active {
                    self.request(Next::File(book.root.join(entry.relative)), window, cx);
                }
            }
        }
    }

    fn command_unavailable(&self, command: Command) -> Option<&'static str> {
        command.unavailable(
            self.document.is_some(),
            self.mode == ViewMode::Reading,
            self.busy,
            self.notebooks.active.is_some(),
        )
    }

    fn run_command(&mut self, command: Command, window: &mut Window, cx: &mut Context<Self>) {
        match command {
            Command::InsertDate => {
                if let Ok(date) = templates::current(self.date_format()) {
                    self.editor.update(cx, |editor, cx| {
                        editor.replace_selection_isolated(&date, window, cx)
                    });
                }
            }
            Command::EditLink => self.edit_link(window, cx),
            Command::SearchFile => self.search_current_file(window, cx),
            Command::NewNote => {
                if let Some(book) = &self.notebooks.active {
                    self.request(Next::NewNote(book.root.clone()), window, cx);
                }
            }
            Command::ChangeNotebook => {
                let entries = self
                    .notebooks
                    .registered_paths()
                    .iter()
                    .map(|path| PaletteEntry::Notebook {
                        path: path.clone(),
                        active: self
                            .notebooks
                            .active
                            .as_ref()
                            .is_some_and(|book| book.root == *path),
                    })
                    .collect::<Vec<_>>();
                let status = entries
                    .is_empty()
                    .then(|| "No registered notebooks. Add one from Manage notebooks.".into());
                self.open_palette(PaletteMode::Notebooks, entries, status, window, cx);
            }
            Command::ToggleTheme => {
                let mut appearance = self.appearance();
                appearance.theme = Some(match appearance.theme_choice(self.startup_dark) {
                    notebook::ThemeChoice::Light => notebook::ThemeChoice::Dark,
                    _ => notebook::ThemeChoice::Light,
                });
                self.update_appearance(appearance, window, cx);
            }
        }
    }

    fn move_palette(&mut self, down: bool, cx: &mut Context<Self>) {
        if let Some(p) = &mut self.palette {
            p.selected = if down {
                (p.selected + 1).min(p.results.len().saturating_sub(1))
            } else {
                p.selected.saturating_sub(1)
            };
            p.scroll.scroll_to_item(p.selected, ScrollStrategy::Top);
        }
        cx.notify();
    }

    fn palette_dialog(
        &mut self,
        dialog: Dialog,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Dialog {
        let owner = cx.weak_entity();
        let Some(palette) = &self.palette else {
            return dialog;
        };
        let body = div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap_2()
            .capture_action(
                cx.listener(|this, _: &gpui_component::input::MoveUp, _, cx| {
                    this.move_palette(false, cx);
                    cx.stop_propagation();
                }),
            )
            .capture_action(
                cx.listener(|this, _: &gpui_component::input::MoveDown, _, cx| {
                    this.move_palette(true, cx);
                    cx.stop_propagation();
                }),
            )
            .capture_action(
                cx.listener(|this, _: &gpui_component::input::Enter, window, cx| {
                    this.activate_palette(window, cx);
                    cx.stop_propagation();
                }),
            )
            .capture_action(
                cx.listener(|this, _: &gpui_component::input::Escape, window, cx| {
                    let back = this
                        .palette
                        .as_ref()
                        .is_some_and(|p| p.mode == PaletteMode::Notebooks);
                    window.close_dialog(cx);
                    this.close_palette(window, cx);
                    if back {
                        this.command_palette(window, cx);
                    }
                    cx.stop_propagation();
                }),
            )
            .child(Input::new(&palette.query))
            .child(if let Some(status) = &palette.status {
                div().child(status.clone()).into_any_element()
            } else if palette.results.is_empty() {
                div()
                    .child(match palette.mode {
                        PaletteMode::Commands => "No matching commands",
                        PaletteMode::Files => "No matching files or folders",
                        PaletteMode::Notebooks => "No matching notebooks",
                    })
                    .into_any_element()
            } else {
                uniform_list(
                    "goto-results",
                    palette.results.len(),
                    cx.processor(|this, range: std::ops::Range<usize>, _, cx| {
                        let p = this.palette.as_ref().unwrap();
                        range
                            .map(|i| {
                                let entry = &p.results[i];
                                let unavailable = match entry {
                                    PaletteEntry::Command(command) => {
                                        this.command_unavailable(*command)
                                    }
                                    _ => None,
                                };
                                div()
                                    .id(i)
                                    .w_full()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .h(px(48.))
                                    .px_2()
                                    .flex()
                                    .flex_col()
                                    .when(i == p.selected, |v| v.bg(cx.theme().accent))
                                    .when(unavailable.is_some(), |v| {
                                        v.text_color(cx.theme().muted_foreground)
                                    })
                                    .child(div().w_full().truncate().child(entry.label()))
                                    .child(
                                        div()
                                            .w_full()
                                            .truncate()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(
                                                unavailable
                                                    .map(str::to_owned)
                                                    .unwrap_or_else(|| entry.detail()),
                                            ),
                                    )
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        if let Some(p) = &mut this.palette {
                                            p.selected = i;
                                        }
                                        this.activate_palette(window, cx);
                                    }))
                            })
                            .collect::<Vec<_>>()
                    }),
                )
                .w_full()
                .min_w_0()
                .h(px(336.))
                .track_scroll(palette.scroll.clone())
                .into_any_element()
            });
        dialog
            .title(match palette.mode {
                PaletteMode::Commands => "Command palette",
                PaletteMode::Files => "Go to",
                PaletteMode::Notebooks => "Change notebook",
            })
            .w(px(560.).min(window.viewport_size().width - px(32.)))
            .on_close(move |_, window, cx| {
                let _ = owner.update(cx, |this, cx| this.close_palette(window, cx));
            })
            .child(body)
    }
}

#[cfg(test)]
mod tests {
    use super::{Command, PaletteEntry, discover, ranked_entries, score};
    use std::{fs, path::Path};
    #[test]
    fn commands_filter_by_full_label_and_check_editability() {
        let entries: Vec<_> = Command::ALL
            .iter()
            .copied()
            .map(PaletteEntry::Command)
            .collect();
        let results = ranked_entries(&entries, "  CURRENT DATE  ");
        assert_eq!(results.len(), 1);
        assert!(matches!(
            results[0],
            PaletteEntry::Command(Command::InsertDate)
        ));
        assert!(ranked_entries(&entries, "no such command xyz").is_empty());
        for command in [Command::InsertDate, Command::EditLink] {
            assert!(command.unavailable(true, false, false, false).is_none());
            assert!(command.unavailable(false, false, false, false).is_some());
            assert!(command.unavailable(true, true, false, false).is_some());
            assert!(command.unavailable(true, false, true, false).is_some());
        }
    }

    #[test]
    fn theme_command_works_without_an_editable_document() {
        assert!(
            Command::ToggleTheme
                .unavailable(false, true, false, false)
                .is_none()
        );
        assert!(
            Command::ToggleTheme
                .unavailable(true, false, true, true)
                .is_some()
        );
        assert_eq!(
            ranked_entries(&[PaletteEntry::Command(Command::ToggleTheme)], "light/dark").len(),
            1
        );
    }

    #[test]
    fn notebook_results_distinguish_duplicate_names_and_search_paths() {
        let entries = vec![
            PaletteEntry::Notebook {
                path: "one/Notes".into(),
                active: false,
            },
            PaletteEntry::Notebook {
                path: "two/Notes".into(),
                active: true,
            },
        ];
        assert_eq!(ranked_entries(&entries, "notes").len(), 2);
        let results = ranked_entries(&entries, "two/notes");
        assert_eq!(results.len(), 1);
        assert!(results[0].label().contains("(active)"));
        assert_eq!(results[0].detail(), "two/Notes");
        assert_ne!(entries[0].detail(), entries[1].detail());
        assert!(
            Command::ChangeNotebook
                .unavailable(false, true, false, false)
                .is_none()
        );
    }

    #[test]
    fn new_note_requires_a_notebook_but_not_an_editable_document() {
        assert!(
            Command::NewNote
                .unavailable(false, true, false, true)
                .is_none()
        );
        assert!(
            Command::NewNote
                .unavailable(true, false, false, false)
                .is_some()
        );
        assert!(
            Command::NewNote
                .unavailable(true, false, true, true)
                .is_some()
        );
    }

    #[test]
    fn current_file_search_supports_reading_and_standalone_documents() {
        assert!(
            Command::SearchFile
                .unavailable(true, true, false, false)
                .is_none()
        );
        assert!(
            Command::SearchFile
                .unavailable(false, false, false, true)
                .is_some()
        );
    }

    #[test]
    fn ranking() {
        assert!(score("note", "note") < score("notebook", "note"));
        assert!(score("notebook", "note") < score("a note", "note"));
        assert!(score("a note", "note") < score("n o t e", "note"));
        assert_eq!(score("note", "xyz"), None);
        assert!(score("日本語", "日語").is_some());
    }
    #[test]
    fn recursive_duplicates_and_settings() {
        let temp = tempfile::tempdir().unwrap();
        for folder in ["a/deep", "b", ".gpui-note"] {
            fs::create_dir_all(temp.path().join(folder)).unwrap();
        }
        for file in [
            "a/deep/Note.md",
            "b/Note.md",
            "b/ignore.txt",
            ".gpui-note/private.md",
        ] {
            fs::write(temp.path().join(file), "").unwrap();
        }
        let entries = discover(temp.path()).unwrap();
        assert_eq!(entries.iter().filter(|e| !e.directory).count(), 2);
        assert!(
            entries
                .iter()
                .any(|e| e.relative == Path::new("a/deep/Note.md"))
        );
        assert!(discover(&temp.path().join("missing")).is_err());
    }
}

#[cfg(feature = "editor-diagnostics")]
pub(super) async fn diagnostic_palette(notes: &Entity<Notes>, cx: &mut AsyncWindowContext) {
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.set_mode(ViewMode::Source, window, cx);
            let before = this.editor.read(cx).value();
            if let Some(book) = &this.notebooks.active {
                let root = book.root.clone();
                let count = fs::read_dir(&root).unwrap().count();
                let previous_path = this.document.as_ref().map(|doc| doc.path.clone());
                this.busy = true;
                this.answer_request(None, Next::NewNote(root.clone()), window, cx);
                assert!(!this.busy);
                assert_eq!(
                    fs::read_dir(root).unwrap().count(),
                    count,
                    "cancel creates no note"
                );
                assert_eq!(
                    this.document.as_ref().map(|doc| doc.path.clone()),
                    previous_path
                );
                assert_eq!(this.editor.read(cx).value(), before);
            }
            this.command_palette(window, cx);
            assert_eq!(this.palette.as_ref().unwrap().mode, PaletteMode::Commands);
            this.palette
                .as_ref()
                .unwrap()
                .query
                .update(cx, |q, cx| q.set_value("current date", window, cx));
            assert_eq!(this.palette.as_ref().unwrap().results.len(), 1);
            window.close_dialog(cx);
            this.close_palette(window, cx);
            assert!(this.editor.focus_handle(cx).is_focused(window));
            assert_eq!(this.editor.read(cx).value(), before);
            this.command_palette(window, cx);
            this.palette
                .as_ref()
                .unwrap()
                .query
                .update(cx, |q, cx| q.set_value("current date", window, cx));
            this.activate_palette(window, cx);
            assert!(this.palette.is_none());
            assert!(this.editor.focus_handle(cx).is_focused(window));
            assert!(
                this.editor
                    .read(cx)
                    .value()
                    .contains(&templates::current(this.date_format()).unwrap())
            );
            this.editor
                .update(cx, |editor, cx| editor.set_value(before, window, cx));
            this.set_mode(ViewMode::Reading, window, cx);
            this.command_palette(window, cx);
            this.palette
                .as_ref()
                .unwrap()
                .query
                .update(cx, |q, cx| q.set_value("current date", window, cx));
            this.activate_palette(window, cx);
            assert!(
                this.palette.is_some(),
                "Reading mode disables date insertion"
            );
            window.close_dialog(cx);
            this.close_palette(window, cx);
        });
    })
    .unwrap();
    for step in 0..=13 {
        Timer::after(std::time::Duration::from_millis(150)).await;
        cx.update(|window, cx| match step {
            1 | 7 | 10 => {
                window.dispatch_keystroke(Keystroke::parse("ctrl-o").unwrap(), cx);
            }
            3 => {
                window.dispatch_keystroke(Keystroke::parse("down").unwrap(), cx);
                window.dispatch_keystroke(Keystroke::parse("up").unwrap(), cx);
            }
            5 | 12 => {
                window.dispatch_keystroke(Keystroke::parse("enter").unwrap(), cx);
            }
            8 => {
                window.dispatch_keystroke(Keystroke::parse("escape").unwrap(), cx);
            }
            _ => notes.update(cx, |this, cx| match step {
                0 => this.set_mode(ViewMode::Source, window, cx),
                2 => {
                    assert!(
                        this.palette.as_ref().is_some_and(|p| p.results.len() >= 3),
                        "Ctrl+O opens recursive palette"
                    );
                }
                4 => {
                    assert_eq!(
                        this.editor.read(cx).value().as_str(),
                        "# Root",
                        "arrow keys do not open results"
                    );
                    let query = this.palette.as_ref().unwrap().query.clone();
                    query.update(cx, |q, cx| q.set_value("nested", window, cx));
                }
                6 => {
                    assert_eq!(
                        this.editor.read(cx).value().as_str(),
                        "# Saved",
                        "Enter opens result"
                    );
                    assert!(this.editor.focus_handle(cx).is_focused(window));
                }
                9 => {
                    assert!(this.palette.is_none(), "Escape closes palette");
                    assert!(
                        this.editor.focus_handle(cx).is_focused(window),
                        "Escape restores focus"
                    );
                }
                11 => {
                    let query = this.palette.as_ref().unwrap().query.clone();
                    query.update(cx, |q, cx| q.set_value("folder", window, cx));
                }
                13 => {
                    assert!(this.palette.is_none());
                    assert!(
                        this.notebooks
                            .active
                            .as_ref()
                            .unwrap()
                            .settings
                            .expanded
                            .contains(Path::new("folder"))
                    );
                    assert!(
                        !this.editor.focus_handle(cx).is_focused(window),
                        "folder activation focuses tree"
                    );
                }
                _ => unreachable!(),
            }),
        })
        .unwrap();
    }
}

#[cfg(feature = "editor-diagnostics")]
pub(super) async fn diagnostic_task_shortcut(notes: &Entity<Notes>, cx: &mut AsyncWindowContext) {
    for step in 0..=8 {
        Timer::after(std::time::Duration::from_millis(150)).await;
        cx.update(|window, cx| match step {
            1 | 4 | 7 => {
                window.dispatch_keystroke(Keystroke::parse("ctrl-l").unwrap(), cx);
            }
            _ => notes.update(cx, |this, cx| match step {
                0 => {
                    this.set_mode(ViewMode::Source, window, cx);
                    this.editor
                        .update(cx, |e, cx| e.set_value("- [ ] task", window, cx));
                }
                2 => {
                    assert_eq!(this.editor.read(cx).value().as_str(), "- [x] task");
                    this.set_mode(ViewMode::Reading, window, cx);
                }
                3 => {}
                5 => {
                    assert_eq!(
                        this.editor.read(cx).value().as_str(),
                        "- [x] task",
                        "Reading has no task shortcut"
                    );
                    this.set_mode(ViewMode::LivePreview, window, cx);
                    this.command_palette(window, cx);
                }
                6 => {}
                8 => {
                    assert_eq!(
                        this.palette
                            .as_ref()
                            .unwrap()
                            .query
                            .read(cx)
                            .value()
                            .as_str(),
                        "",
                        "Palette input has no task shortcut"
                    );
                    assert_eq!(this.editor.read(cx).value().as_str(), "- [x] task");
                    window.close_dialog(cx);
                    this.close_palette(window, cx);
                }
                _ => unreachable!(),
            }),
        })
        .unwrap();
    }
}
