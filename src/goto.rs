use super::*;
use gpui_component::dialog::Dialog;

pub(super) struct Palette {
    query: Entity<InputState>,
    _subscription: Subscription,
    entries: Vec<notebook::Entry>,
    results: Vec<notebook::Entry>,
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

impl Palette {
    fn filter(&mut self, cx: &App) {
        let query = self.query.read(cx).value().trim().to_lowercase();
        let mut ranked: Vec<_> = self
            .entries
            .iter()
            .filter_map(|entry| {
                let name = notebook::name(&entry.relative).to_lowercase();
                let path = entry.relative.to_string_lossy().to_lowercase();
                score(&name, &query)
                    .or_else(|| score(&path, &query).map(|(_, cost)| (4, cost)))
                    .map(|rank| (rank, entry.clone()))
            })
            .collect();
        ranked.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.relative.cmp(&b.1.relative)));
        self.results = ranked.into_iter().map(|(_, entry)| entry).collect();
        self.selected = 0;
        self.scroll.scroll_to_item(0, ScrollStrategy::Top);
    }
}

impl Notes {
    pub(super) fn go_to(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || window.has_active_dialog(cx) {
            return;
        }
        let Some(book) = &self.notebooks.active else {
            self.request(Next::Open, window, cx);
            return;
        };
        let root = book.root.clone();
        let query =
            cx.new(|cx| InputState::new(window, cx).placeholder("Search files and folders"));
        let subscription = cx.subscribe(&query, |this, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                if let Some(palette) = &mut this.palette {
                    palette.filter(cx);
                }
                cx.notify();
            }
        });
        self.palette = Some(Palette {
            query: query.clone(),
            _subscription: subscription,
            entries: Vec::new(),
            results: Vec::new(),
            selected: 0,
            scroll: UniformListScrollHandle::new(),
            status: Some("Loading…".into()),
            focus: window.focused(cx),
        });
        let owner = cx.weak_entity();
        window.open_dialog(cx, move |dialog, window, cx| {
            owner
                .update(cx, |this, cx| this.palette_dialog(dialog, window, cx))
                .unwrap_or_else(|_| Dialog::new(window, cx))
        });
        query.update(cx, |query, cx| query.focus(window, cx));
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
                            palette.entries = entries;
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
        let entry = self
            .palette
            .as_ref()
            .and_then(|p| p.results.get(p.selected))
            .cloned();
        let Some(entry) = entry else {
            return;
        };
        window.close_dialog(cx);
        self.close_palette(window, cx);
        if entry.directory {
            self.focus_folder(entry.relative, window, cx);
        } else if let Some(book) = &self.notebooks.active {
            self.request(Next::File(book.root.join(entry.relative)), window, cx);
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

    fn palette_dialog(&mut self, dialog: Dialog, _: &mut Window, cx: &mut Context<Self>) -> Dialog {
        let owner = cx.weak_entity();
        let Some(palette) = &self.palette else {
            return dialog;
        };
        let body = div()
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
                    window.close_dialog(cx);
                    this.close_palette(window, cx);
                    cx.stop_propagation();
                }),
            )
            .child(Input::new(&palette.query))
            .child(if let Some(status) = &palette.status {
                div().child(status.clone()).into_any_element()
            } else if palette.results.is_empty() {
                div()
                    .child("No matching files or folders")
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
                                div()
                                    .id(i)
                                    .h(px(48.))
                                    .px_2()
                                    .flex()
                                    .flex_col()
                                    .when(i == p.selected, |v| v.bg(cx.theme().accent))
                                    .child(format!(
                                        "{}{}",
                                        if entry.directory { "▸ " } else { "" },
                                        notebook::name(&entry.relative)
                                    ))
                                    .child(
                                        div()
                                            .text_xs()
                                            .text_color(cx.theme().muted_foreground)
                                            .child(entry.relative.display().to_string()),
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
                .h(px(336.))
                .track_scroll(palette.scroll.clone())
                .into_any_element()
            });
        dialog
            .title("Go to")
            .w(px(560.))
            .on_close(move |_, window, cx| {
                let _ = owner.update(cx, |this, cx| this.close_palette(window, cx));
            })
            .child(body)
    }
}

#[cfg(test)]
mod tests {
    use super::{discover, score};
    use std::{fs, path::Path};
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
