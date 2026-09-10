#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

#[cfg(feature = "editor-diagnostics")]
mod benchmark;

use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

use gpui::{prelude::*, *};
use gpui_component::{
    ActiveTheme, Root, Sizable,
    button::{Button, ButtonVariants},
    input::{Input, InputEvent, InputState, Rope},
    menu::PopupMenu,
    text::TextView,
};

actions!(
    notes,
    [
        Open,
        Save,
        SourceMode,
        LivePreview,
        ReadingView,
        FileMenu,
        ViewMenu
    ]
);

#[derive(Clone, Copy, PartialEq)]
enum ViewMode {
    Source,
    LivePreview,
    Reading,
}

#[derive(Clone, Copy)]
enum Next {
    Nothing,
    Open,
    Close,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Newline {
    Lf,
    CrLf,
    Cr,
}

struct Document {
    path: PathBuf,
    saved: String,
    saved_rope: Rope,
    bom: bool,
    newline: Newline,
}

impl Document {
    fn open(path: PathBuf) -> io::Result<Self> {
        if !path
            .extension()
            .and_then(|s| s.to_str())
            .is_some_and(|s| s.eq_ignore_ascii_case("md") || s.eq_ignore_ascii_case("markdown"))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Choose a .md or .markdown file.",
            ));
        }
        let bytes = fs::read(&path)?;
        let bom = bytes.starts_with(&[0xef, 0xbb, 0xbf]);
        let raw = std::str::from_utf8(if bom { &bytes[3..] } else { &bytes }).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidData, "This file is not valid UTF-8.")
        })?;
        // ponytail: mixed endings use the first convention; track each line if exact mixed-file preservation is needed.
        let newline = match raw.bytes().position(|b| b == b'\r' || b == b'\n') {
            Some(i) if raw.as_bytes()[i..].starts_with(b"\r\n") => Newline::CrLf,
            Some(i) if raw.as_bytes()[i] == b'\r' => Newline::Cr,
            _ => Newline::Lf,
        };
        let saved = normalize_newlines(raw);
        Ok(Self {
            path,
            saved_rope: Rope::from(saved.as_str()),
            saved,
            bom,
            newline,
        })
    }

    #[cfg(test)]
    fn dirty(&self, text: &str) -> bool {
        self.saved != text
    }
}

fn normalize_newlines(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

fn save_file(path: &Path, text: &str, bom: bool, newline: Newline) -> io::Result<()> {
    // Write beside the original, then replace it only after the complete write succeeds.
    let mut temp = tempfile::NamedTempFile::new_in(path.parent().unwrap_or(Path::new(".")))?;
    if bom {
        temp.write_all(&[0xef, 0xbb, 0xbf])?;
    }
    let normalized = normalize_newlines(text);
    let encoded = match newline {
        Newline::Lf => normalized,
        Newline::CrLf => normalized.replace('\n', "\r\n"),
        Newline::Cr => normalized.replace('\n', "\r"),
    };
    temp.write_all(encoded.as_bytes())?;
    temp.as_file().sync_all()?;
    temp.persist(path).map_err(|e| e.error)?;
    Ok(())
}

struct Notes {
    document: Option<Document>,
    editor: Entity<InputState>,
    editor_subscription: Subscription,
    mode: ViewMode,
    reading_text: SharedString,
    focus: FocusHandle,
    busy: bool,
    dirty_cached: bool,
    dirty_revision: u64,
    dirty_task: Option<Task<()>>,
    menu: Option<(bool, Entity<PopupMenu>)>,
    menu_subscription: Option<Subscription>,
}

impl Notes {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let editor = cx.new(|cx| InputState::new(window, cx).multi_line(true));
        let editor_subscription = Self::subscribe_editor(&editor, window, cx);
        let weak = cx.weak_entity();
        window.on_window_should_close(cx, move |window, cx| {
            weak.update(cx, |this, cx| {
                if this.busy {
                    return false;
                }
                if this.dirty(cx) {
                    this.request(Next::Close, window, cx);
                    false
                } else {
                    true
                }
            })
            .unwrap_or(true)
        });
        let focus = cx.focus_handle();
        focus.focus(window);
        Self {
            document: None,
            editor,
            editor_subscription,
            mode: ViewMode::Source,
            reading_text: SharedString::default(),
            focus,
            busy: false,
            dirty_cached: false,
            dirty_revision: 0,
            dirty_task: None,
            menu: None,
            menu_subscription: None,
        }
    }

    fn subscribe_editor(
        editor: &Entity<InputState>,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Subscription {
        cx.subscribe_in(editor, window, |this, _, event, window, cx| {
            if matches!(event, InputEvent::Change) {
                this.check_dirty(window, cx);
            }
        })
    }

    fn dirty(&self, cx: &App) -> bool {
        self.document
            .as_ref()
            .is_some_and(|d| d.saved_rope != self.editor.read(cx).snapshot())
    }

    fn check_dirty(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(document) = self.document.as_ref() else {
            return;
        };
        let saved = document.saved_rope.clone();
        let current = self.editor.read(cx).snapshot();
        self.dirty_revision += 1;
        let revision = self.dirty_revision;
        self.dirty_cached = true;
        self.refresh(window, cx);
        self.dirty_task = Some(cx.spawn_in(window, async move |this, cx| {
            let dirty = cx
                .background_executor()
                .spawn(async move { saved != current })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.dirty_revision != revision {
                    return;
                }
                this.dirty_cached = dirty;
                this.refresh(window, cx);
            });
        }));
    }

    fn refresh(&self, window: &mut Window, cx: &mut Context<Self>) {
        let name = self
            .document
            .as_ref()
            .and_then(|d| d.path.file_name())
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "GPUI Note".into());
        window.set_window_title(&format!(
            "{}{} — GPUI Note",
            if self.dirty_cached { "* " } else { "" },
            name
        ));
        cx.notify();
    }

    fn error(
        &mut self,
        message: &str,
        error: impl std::fmt::Display,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.busy = true;
        let prompt = window.prompt(
            PromptLevel::Critical,
            message,
            Some(&error.to_string()),
            &["OK"],
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            let _ = prompt.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.busy = false;
                this.refresh(window, cx);
            });
        })
        .detach();
        cx.notify();
    }

    fn request(&mut self, next: Next, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        if !self.dirty(cx) {
            self.continue_with(next, window, cx);
            return;
        }
        self.busy = true;
        let prompt = window.prompt(
            PromptLevel::Warning,
            "Save changes?",
            Some("This document has unsaved changes."),
            &[
                PromptButton::Ok("Save".into()),
                PromptButton::Other("Discard".into()),
                PromptButton::Cancel("Cancel".into()),
            ],
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            let answer = prompt.await.ok();
            let _ = this.update_in(cx, |this, window, cx| {
                this.busy = false;
                match answer {
                    Some(0) => this.save(next, window, cx),
                    Some(1) => this.continue_with(next, window, cx),
                    _ => this.refresh(window, cx),
                }
            });
        })
        .detach();
        cx.notify();
    }

    fn continue_with(&mut self, next: Next, window: &mut Window, cx: &mut Context<Self>) {
        match next {
            Next::Nothing => self.refresh(window, cx),
            Next::Open => self.open(window, cx),
            Next::Close => window.remove_window(),
        }
    }

    fn open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.busy = true;
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Open".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = match picker.await {
                Ok(Ok(Some(paths))) => match paths.into_iter().next() {
                    Some(path) => {
                        cx.background_executor()
                            .spawn(async move { Document::open(path).map(Some) })
                            .await
                    }
                    None => Ok(None),
                },
                Ok(Ok(None)) => Ok(None),
                Ok(Err(error)) => Err(io::Error::other(error.to_string())),
                Err(error) => Err(io::Error::other(error.to_string())),
            };
            let _ = this.update_in(cx, |this, window, cx| {
                this.busy = false;
                match result {
                    Ok(Some(document)) => {
                        // A fresh editor keeps undo from restoring the previously opened document.
                        let editor = cx.new(|cx| {
                            InputState::new(window, cx)
                                .multi_line(true)
                                .default_value(document.saved.clone())
                        });
                        this.editor_subscription = Self::subscribe_editor(&editor, window, cx);
                        this.editor = editor;
                        this.document = Some(document);
                        if this.mode == ViewMode::Reading {
                            this.reading_text = this.editor.read(cx).value();
                        }
                        this.dirty_task = None;
                        this.dirty_revision += 1;
                        this.dirty_cached = false;
                        this.editor.update(cx, |editor, cx| {
                            editor.set_live_preview(this.mode == ViewMode::LivePreview, cx)
                        });
                        if this.mode != ViewMode::Reading {
                            this.editor
                                .update(cx, |editor, cx| editor.focus(window, cx));
                        }
                    }
                    Ok(None) => {}
                    Err(error) => this.error("Could not open file", error, window, cx),
                }
                this.refresh(window, cx);
            });
        })
        .detach();
        cx.notify();
    }

    fn save(&mut self, next: Next, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let Some(document) = &self.document else {
            return;
        };
        let (path, bom, newline) = (document.path.clone(), document.bom, document.newline);
        let text = self.editor.read(cx).value().to_string();
        self.busy = true;
        let task = cx
            .background_executor()
            .spawn(async move { save_file(&path, &text, bom, newline).map(|()| text) });
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.busy = false;
                match result {
                    Ok(text) => {
                        if let Some(document) = &mut this.document {
                            document.saved_rope = Rope::from(text.as_str());
                            document.saved = text;
                            this.dirty_task = None;
                            this.dirty_revision += 1;
                            this.dirty_cached = false;
                        }
                        this.continue_with(next, window, cx);
                    }
                    Err(error) => this.error("Could not save file", error, window, cx),
                }
                this.refresh(window, cx);
            });
        })
        .detach();
        cx.notify();
    }

    fn set_mode(&mut self, mode: ViewMode, window: &mut Window, cx: &mut Context<Self>) {
        self.reading_text = if mode == ViewMode::Reading {
            self.editor.read(cx).value()
        } else {
            SharedString::default()
        };
        self.mode = mode;
        self.editor.update(cx, |editor, cx| {
            editor.set_live_preview(mode == ViewMode::LivePreview, cx)
        });
        if mode != ViewMode::Reading && self.document.is_some() {
            self.editor
                .update(cx, |editor, cx| editor.focus(window, cx));
        } else {
            self.focus.focus(window);
        }
        cx.notify();
    }

    fn show_menu(&mut self, file: bool, window: &mut Window, cx: &mut Context<Self>) {
        let busy = self.busy;
        let can_save = self.document.is_some() && !busy;
        let mode = self.mode;
        let focus = self.focus.clone();
        let menu = PopupMenu::build(window, cx, |menu, _, _| {
            let menu = menu.action_context(focus);
            if file {
                menu.menu_with_disabled("Open…", Box::new(Open), busy)
                    .menu_with_disabled("Save", Box::new(Save), !can_save)
            } else {
                menu.menu_with_check(
                    "Source mode",
                    mode == ViewMode::Source,
                    Box::new(SourceMode),
                )
                .menu_with_check(
                    "Live preview",
                    mode == ViewMode::LivePreview,
                    Box::new(LivePreview),
                )
                .menu_with_check(
                    "Reading view",
                    mode == ViewMode::Reading,
                    Box::new(ReadingView),
                )
            }
        });
        self.menu_subscription =
            Some(
                cx.subscribe_in(&menu, window, |this, _, _: &DismissEvent, _, cx| {
                    this.menu = None;
                    cx.notify();
                }),
            );
        menu.focus_handle(cx).focus(window);
        self.menu = Some((file, menu));
        cx.notify();
    }
}

impl Render for Notes {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let content = if self.document.is_none() {
            div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_color(cx.theme().muted_foreground)
                .child("Open a Markdown file with File → Open… (Ctrl+O)")
                .into_any_element()
        } else if self.mode != ViewMode::Reading {
            div()
                .size_full()
                .when(self.mode == ViewMode::Source, |view| {
                    view.font_family("Consolas")
                })
                .child(
                    Input::new(&self.editor)
                        .h_full()
                        .appearance(false)
                        .disabled(self.busy),
                )
                .into_any_element()
        } else {
            div()
                .size_full()
                .p_6()
                .child(
                    TextView::markdown("reading", self.reading_text.clone(), window, cx)
                        .selectable(true)
                        .scrollable(true)
                        .size_full(),
                )
                .into_any_element()
        };
        div()
            .id("notes")
            .key_context("Notes")
            .track_focus(&self.focus)
            .on_action(
                cx.listener(|this, _: &Open, window, cx| this.request(Next::Open, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &Save, window, cx| this.save(Next::Nothing, window, cx)),
            )
            .on_action(cx.listener(|this, _: &SourceMode, window, cx| {
                this.set_mode(ViewMode::Source, window, cx)
            }))
            .on_action(cx.listener(|this, _: &LivePreview, window, cx| {
                this.set_mode(ViewMode::LivePreview, window, cx)
            }))
            .on_action(cx.listener(|this, _: &ReadingView, window, cx| {
                this.set_mode(ViewMode::Reading, window, cx)
            }))
            .on_action(
                cx.listener(|this, _: &FileMenu, window, cx| this.show_menu(true, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &ViewMenu, window, cx| this.show_menu(false, window, cx)),
            )
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .child(
                div()
                    .flex()
                    .gap_1()
                    .p_1()
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(
                        Button::new("file-menu")
                            .ghost()
                            .small()
                            .label("File")
                            .on_click(
                                cx.listener(|this, _, window, cx| this.show_menu(true, window, cx)),
                            ),
                    )
                    .child(
                        Button::new("view-menu")
                            .ghost()
                            .small()
                            .label("View")
                            .on_click(
                                cx.listener(|this, _, window, cx| {
                                    this.show_menu(false, window, cx)
                                }),
                            ),
                    ),
            )
            .child(div().flex_1().min_h_0().overflow_hidden().child(content))
            .when_some(self.menu.clone(), |this, (file, menu)| {
                this.child(deferred(
                    div()
                        .absolute()
                        .top(px(36.))
                        .left(px(if file { 4. } else { 54. }))
                        .child(menu),
                ))
            })
    }
}

fn main() {
    #[cfg(feature = "editor-diagnostics")]
    if let Some(output) = std::env::var_os("GPUI_NOTE_BENCH") {
        let output = PathBuf::from(output);
        let _ = fs::write(
            output.with_extension("progress.txt"),
            "starting application",
        );
        std::panic::set_hook(Box::new(move |info| {
            let _ = fs::write(output.with_extension("panic.txt"), info.to_string());
        }));
    }
    Application::new()
        .with_assets(gpui_component_assets::Assets)
        .run(|cx: &mut App| {
            gpui_component::init(cx);
            cx.bind_keys([
                KeyBinding::new("ctrl-o", Open, Some("Notes")),
                KeyBinding::new("ctrl-s", Save, Some("Notes")),
                KeyBinding::new("alt-f", FileMenu, Some("Notes")),
                KeyBinding::new("alt-v", ViewMenu, Some("Notes")),
            ]);
            cx.on_window_closed(|cx| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
            let bounds = Bounds::centered(None, size(px(900.), px(700.)), cx);
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    window_min_size: Some(size(px(400.), px(300.))),
                    titlebar: Some(TitlebarOptions {
                        title: Some("GPUI Note".into()),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                |window, cx| {
                    let notes = cx.new(|cx| Notes::new(window, cx));
                    #[cfg(feature = "editor-diagnostics")]
                    if let Some(output) = std::env::var_os("GPUI_NOTE_BENCH") {
                        let notes = notes.clone();
                        window.on_next_frame(move |window, cx| {
                            benchmark::start(&notes, output.into(), window, cx)
                        });
                    }
                    cx.new(|cx| Root::new(notes, window, cx))
                },
            )
            .expect("Could not create the GPUI Note window");
            cx.activate(true);
        });
}

#[cfg(test)]
mod tests {
    use super::{Document, Newline, fs, save_file};

    #[test]
    fn document_round_trip_and_dirty_state() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("note.md");
        for (bytes, newline) in [
            ("# Café\n你好\n", Newline::Lf),
            ("# Café\r\n你好\r\n", Newline::CrLf),
            ("# Café\r你好\r", Newline::Cr),
            ("", Newline::Lf),
        ] {
            for bom in [false, true] {
                let original = [
                    if bom { b"\xef\xbb\xbf".as_slice() } else { b"" },
                    bytes.as_bytes(),
                ]
                .concat();
                fs::write(&path, &original).unwrap();
                let mut document = Document::open(path.clone()).unwrap();
                assert_eq!(document.newline, newline);
                assert!(!document.dirty(&document.saved));
                save_file(&path, &document.saved, document.bom, document.newline).unwrap();
                assert_eq!(fs::read(&path).unwrap(), original);
                let edited = format!("{}edit\n", document.saved);
                assert!(document.dirty(&edited));
                save_file(&path, &edited, document.bom, document.newline).unwrap();
                document.saved = edited.clone();
                assert!(!document.dirty(&edited));
                assert_eq!(Document::open(path.clone()).unwrap().saved, edited);
            }
        }
        fs::write(&path, [0xff]).unwrap();
        assert!(Document::open(path.clone()).is_err());
        assert!(Document::open(dir.path().join("missing.md")).is_err());
        assert!(Document::open(dir.path().join("file.txt")).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn failed_save_preserves_original() {
        use std::os::windows::fs::OpenOptionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("note.md");
        fs::write(&path, "original").unwrap();
        let lock = fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(&path)
            .unwrap();
        assert!(save_file(&path, "changed", false, Newline::Lf).is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "original");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
        drop(lock);
    }
}
