//! Palette UI adapter. Command policy, ranking and selection belong to `commands`.
use crate::{Next, Notes};
use gpui::{
    App, Context, Entity, FocusHandle, ScrollStrategy, Subscription, UniformListScrollHandle,
    Window, div, prelude::*, px, uniform_list,
};
#[cfg(feature = "editor-diagnostics")]
use gpui::{AsyncWindowContext, Keystroke, Timer};
use gpui_component::{
    ActiveTheme, WindowExt,
    dialog::Dialog,
    input::{Input, InputEvent, InputState},
};
#[cfg(feature = "editor-diagnostics")]
use mithril::ViewMode;
use mithril::{
    commands::{Command, PaletteEntry, PaletteModel, discover},
    notebook, templates,
};
#[cfg(feature = "editor-diagnostics")]
use std::{fs, path::Path};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PaletteMode {
    Commands,
    Files,
    Notebooks,
}

pub(super) struct Palette {
    mode: PaletteMode,
    query: Entity<InputState>,
    _subscription: Subscription,
    model: PaletteModel,
    scroll: UniformListScrollHandle,
    status: Option<String>,
    focus: Option<FocusHandle>,
}

impl Palette {
    fn filter(&mut self, cx: &App) {
        self.model.filter(&self.query.read(cx).value());
        self.scroll.scroll_to_item(0, ScrollStrategy::Top);
    }
}

impl Notes {
    pub(super) fn command_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_busy() || window.has_active_dialog(cx) {
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
            model: PaletteModel::new(entries),
            scroll: UniformListScrollHandle::new(),
            status,
            focus: window.focused(cx),
        });
        let owner = cx.weak_entity();
        window.open_dialog(cx, move |dialog, window, cx| {
            owner
                .update(cx, |this, cx| this.palette_dialog(dialog, window, cx))
                .unwrap_or_else(|_| Dialog::new(window, cx))
        });
        query.update(cx, |query, cx| query.focus(window, cx));
    }

    pub(super) fn go_to(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_busy() || window.has_active_dialog(cx) {
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
                            palette.model.replace_entries(
                                entries.into_iter().map(PaletteEntry::File).collect(),
                                &palette.query.read(cx).value(),
                            );
                            palette.status = None;
                            palette.scroll.scroll_to_item(0, ScrollStrategy::Top);
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
        if let Some(palette) = self.palette.take()
            && let Some(focus) = palette.focus
        {
            focus.focus(window);
        }
        cx.notify();
    }

    fn activate_palette(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_busy() {
            return;
        }
        let entry = self
            .palette
            .as_ref()
            .and_then(|p| p.model.selected_entry())
            .cloned();
        let Some(entry) = entry else {
            return;
        };
        if let PaletteEntry::Command(command) = &entry
            && self.command_unavailable(*command).is_some()
        {
            return;
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
        command.unavailable(self.capabilities())
    }

    fn run_command(&mut self, command: Command, window: &mut Window, cx: &mut Context<Self>) {
        match command {
            Command::MoveLineUp => self.editor.move_lines(false, window, cx),
            Command::MoveLineDown => self.editor.move_lines(true, window, cx),
            Command::InsertDate => {
                if let Ok(date) = templates::current(self.date_format()) {
                    self.editor.replace_selection(&date, window, cx);
                }
            }
            Command::EditLink => self.edit_link(window, cx),
            Command::SearchFile => self.search_current_file(window, cx),
            Command::SearchAllFiles => self.open_search_all(window, cx),
            Command::ReplaceFile => self.editor.find_and_replace(window, cx),
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
            let selected = p.model.move_selection(down);
            p.scroll.scroll_to_item(selected, ScrollStrategy::Top);
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
            } else if palette.model.results().is_empty() {
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
                    palette.model.results().len(),
                    cx.processor(|this, range: std::ops::Range<usize>, _, cx| {
                        let p = this.palette.as_ref().unwrap();
                        range
                            .map(|i| {
                                let entry = &p.model.results()[i];
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
                                    .when(i == p.model.selected_index(), |v| {
                                        v.bg(cx.theme().accent)
                                    })
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
                                        if let Some(p) = &mut this.palette
                                            && p.model.select(i)
                                        {
                                            this.activate_palette(window, cx);
                                        }
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

#[cfg(feature = "editor-diagnostics")]
pub(super) async fn diagnostic_palette(notes: &Entity<Notes>, cx: &mut AsyncWindowContext) {
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.set_mode(ViewMode::Source, window, cx);
            let before = this.editor.text(cx);
            if let Some(book) = &this.notebooks.active {
                let root = book.root.clone();
                let count = fs::read_dir(&root).unwrap().count();
                let previous_path = this.document().map(|doc| doc.path.clone());
                this.diagnostic_answer_request(None, Next::NewNote(root.clone()), window, cx);
                assert!(!this.is_busy());
                assert_eq!(
                    fs::read_dir(root).unwrap().count(),
                    count,
                    "cancel creates no note"
                );
                assert_eq!(this.document().map(|doc| doc.path.clone()), previous_path);
                assert_eq!(this.editor.text(cx), before);
            }
            this.command_palette(window, cx);
            assert_eq!(this.palette.as_ref().unwrap().mode, PaletteMode::Commands);
            this.palette
                .as_ref()
                .unwrap()
                .query
                .update(cx, |q, cx| q.set_value("current date", window, cx));
            assert_eq!(this.palette.as_ref().unwrap().model.results().len(), 1);
            window.close_dialog(cx);
            this.close_palette(window, cx);
            assert!(this.editor.focus_handle(cx).is_focused(window));
            assert_eq!(this.editor.text(cx), before);
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
                    .diagnostic_input()
                    .read(cx)
                    .value()
                    .contains(&templates::current(this.date_format()).unwrap())
            );
            this.editor
                .diagnostic_input()
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
                        this.palette
                            .as_ref()
                            .is_some_and(|p| p.model.results().len() >= 3),
                        "Ctrl+O opens recursive palette"
                    );
                }
                4 => {
                    assert_eq!(
                        this.editor.text(cx).as_str(),
                        "# Root",
                        "arrow keys do not open results"
                    );
                    let query = this.palette.as_ref().unwrap().query.clone();
                    query.update(cx, |q, cx| q.set_value("nested", window, cx));
                }
                6 => {
                    assert_eq!(
                        this.editor.text(cx).as_str(),
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
                        .diagnostic_input()
                        .update(cx, |e, cx| e.set_value("- [ ] task", window, cx));
                }
                2 => {
                    assert_eq!(this.editor.text(cx).as_str(), "- [x] task");
                    this.set_mode(ViewMode::Reading, window, cx);
                }
                3 => {}
                5 => {
                    assert_eq!(
                        this.editor.text(cx).as_str(),
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
                    assert_eq!(this.editor.text(cx).as_str(), "- [x] task");
                    window.close_dialog(cx);
                    this.close_palette(window, cx);
                }
                _ => unreachable!(),
            }),
        })
        .unwrap();
    }
}

#[cfg(test)]
mod tests;
