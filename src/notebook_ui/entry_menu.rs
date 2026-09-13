//! Sidebar menus capture a notebook generation and path, never a reusable row number.
use super::*;
use gpui::{ClipboardItem, Pixels, Point};
use gpui_component::{WindowExt, dialog::DialogButtonProps, input::Input};
use mithril::explorer::Generation;
use std::path::Path;

#[derive(Clone)]
pub(crate) struct EntryTarget {
    pub root: PathBuf,
    pub path: PathBuf,
    pub generation: Generation,
    pub directory: bool,
}
#[derive(Clone)]
pub(crate) enum EntryAction {
    Open,
    Rename,
    NewNote,
    NewFolder,
    Duplicate,
    CopyPath,
    Reveal,
    Trash,
    Refresh,
    CreateFolder(String),
}
fn actions(directory: bool, background: bool) -> Vec<(&'static str, EntryAction)> {
    if background {
        return vec![
            ("New note", EntryAction::NewNote),
            ("New folder", EntryAction::NewFolder),
            ("Refresh", EntryAction::Refresh),
        ];
    }
    let mut items = if directory {
        vec![
            ("New note", EntryAction::NewNote),
            ("New folder", EntryAction::NewFolder),
            ("Rename", EntryAction::Rename),
        ]
    } else {
        vec![
            ("Open", EntryAction::Open),
            ("Rename", EntryAction::Rename),
            ("Duplicate", EntryAction::Duplicate),
        ]
    };
    items.push(("Copy relative path", EntryAction::CopyPath));
    items.push((
        if cfg!(target_os = "windows") {
            "Reveal in Explorer"
        } else if cfg!(target_os = "macos") {
            "Reveal in Finder"
        } else {
            "Reveal in file manager"
        },
        EntryAction::Reveal,
    ));
    items.push(("Move to Trash", EntryAction::Trash));
    items
}
impl Notes {
    fn entry_target_current(&self, target: &EntryTarget) -> bool {
        self.notebooks
            .active
            .as_ref()
            .is_some_and(|book| book.root == target.root)
            && self.notebooks.explorer.is_current(target.generation)
    }
    pub(super) fn open_entry_menu(
        &mut self,
        path: Option<PathBuf>,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if window.has_active_dialog(cx) {
            return;
        }
        let Some(book) = &self.notebooks.active else {
            return;
        };
        let background = path.is_none();
        let directory = path.as_ref().is_none_or(|path| {
            self.notebooks
                .explorer
                .rows()
                .iter()
                .any(|row| row.path == *path && row.directory)
        });
        let target = EntryTarget {
            root: book.root.clone(),
            path: path.unwrap_or_default(),
            generation: self.notebooks.explorer.generation(),
            directory,
        };
        let busy = self.is_busy();
        let writing = self.session.write().is_some();
        let owner = cx.weak_entity();
        let menu = PopupMenu::build(window, cx, move |mut menu, _, _| {
            for (label, action) in actions(directory, background) {
                if matches!(action, EntryAction::CopyPath | EntryAction::Trash) {
                    menu = menu.separator();
                }
                let disabled = busy || (writing && matches!(action, EntryAction::Rename));
                let target = target.clone();
                let owner = owner.clone();
                menu = menu.item(PopupMenuItem::new(label).disabled(disabled).on_click(
                    move |_, window, cx| {
                        let _ = owner.update(cx, |this, cx| {
                            this.entry_action(target.clone(), action.clone(), window, cx)
                        });
                    },
                ));
            }
            menu
        });
        self.menu = None;
        self.notebooks.context_subscription =
            Some(
                cx.subscribe_in(&menu, window, |this, _, _: &DismissEvent, window, cx| {
                    this.notebooks.context_menu = None;
                    if !window.has_active_dialog(cx) && !this.editor.is_focused(window, cx) {
                        this.notebooks.tree_focus.focus(window);
                    }
                    cx.notify();
                }),
            );
        menu.focus_handle(cx).focus(window);
        self.notebooks.context_menu = Some((position, menu));
        cx.notify();
    }
    fn entry_action(
        &mut self,
        target: EntryTarget,
        action: EntryAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.notebooks.context_menu = None;
        if !self.entry_target_current(&target) || self.is_busy() {
            return;
        }
        let validated = if target.path.as_os_str().is_empty() {
            notebook::entry_directory(&target.root, &target.path)
        } else {
            notebook::entry_path(&target.root, &target.path)
        };
        if let Err(error) = validated {
            self.notebooks.notice = Some(error.to_string());
            cx.notify();
            return;
        }
        match action {
            EntryAction::Open => {
                self.request(Next::File(target.root.join(&target.path)), window, cx)
            }
            EntryAction::Refresh => self.refresh_sorted_tree(window, cx),
            EntryAction::CopyPath => cx.write_to_clipboard(ClipboardItem::new_string(
                target.path.to_string_lossy().replace('\\', "/"),
            )),
            EntryAction::Reveal => match notebook::entry_path(&target.root, &target.path) {
                Ok(path) => {
                    #[cfg(not(test))]
                    cx.reveal_path(&path);
                    #[cfg(test)]
                    {
                        let _ = path;
                    }
                }
                Err(error) => self.notebooks.notice = Some(error.to_string()),
            },
            EntryAction::Rename | EntryAction::NewFolder => {
                self.entry_name_dialog(target, matches!(action, EntryAction::Rename), window, cx)
            }
            EntryAction::Trash => {
                let owner = cx.weak_entity();
                let detail = if target.directory {
                    format!(
                        "Move '{}' and all its contents to Trash?",
                        notebook::name(&target.path)
                    )
                } else {
                    format!("Move '{}' to Trash?", notebook::name(&target.path))
                };
                window.open_dialog(cx, move |dialog, _, _| {
                    let owner = owner.clone();
                    let target = target.clone();
                    dialog
                        .title("Move to Trash")
                        .confirm()
                        .button_props(DialogButtonProps::default().ok_text("Move to Trash"))
                        .child(detail.clone())
                        .on_ok(move |_, window, cx| {
                            let _ = owner.update(cx, |this, cx| {
                                this.request_entry(target.clone(), EntryAction::Trash, window, cx)
                            });
                            true
                        })
                });
            }
            action => self.request_entry(target, action, window, cx),
        }
        cx.notify();
    }
    fn entry_name_dialog(
        &mut self,
        target: EntryTarget,
        rename: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Folder name")
                .default_value(if rename {
                    notebook::name(&target.path)
                } else {
                    String::new()
                })
        });
        let focus = input.clone();
        let owner = cx.weak_entity();
        let error = std::rc::Rc::new(std::cell::RefCell::new(String::new()));
        window.open_dialog(cx, move |dialog, _, _| {
            let input = input.clone();
            let target = target.clone();
            let owner = owner.clone();
            let error = error.clone();
            let notice = error.borrow().clone();
            dialog
                .title(if rename { "Rename" } else { "New folder" })
                .confirm()
                .child(Input::new(&input).w_full())
                .child(notice)
                .on_ok(move |_, window, cx| {
                    let name = input.read(cx).value().to_string();
                    let validation = if rename {
                        notebook::rename_destination(&target.path, &name, target.directory)
                            .map(|_| ())
                    } else {
                        notebook::validate_name(&name).and_then(|_| {
                            if name.eq_ignore_ascii_case(".mithril") {
                                Err(io::Error::other("This name is reserved."))
                            } else {
                                Ok(())
                            }
                        })
                    };
                    if let Err(message) = validation {
                        *error.borrow_mut() = message.to_string();
                        cx.refresh_windows();
                        return false;
                    }
                    owner
                        .update(cx, |this, cx| {
                            if !this.entry_target_current(&target) || this.is_busy() {
                                return true;
                            }
                            if rename {
                                this.move_sidebar_entry(
                                    &EntryDrag {
                                        root: target.root.clone(),
                                        path: target.path.clone(),
                                    },
                                    target.path.parent().unwrap_or(Path::new("")).to_path_buf(),
                                    Some(name),
                                    window,
                                    cx,
                                );
                            } else {
                                this.request_entry(
                                    target.clone(),
                                    EntryAction::CreateFolder(name),
                                    window,
                                    cx,
                                );
                            }
                            true
                        })
                        .unwrap_or(true)
                })
        });
        focus.update(cx, |input, cx| input.focus(window, cx));
    }
    fn request_entry(
        &mut self,
        target: EntryTarget,
        action: EntryAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.entry_target_current(&target) || self.is_busy() {
            return;
        }
        let absolute = target.root.join(&target.path);
        let affects_content = matches!(action, EntryAction::NewNote)
            || (matches!(action, EntryAction::Trash)
                && self
                    .document()
                    .map(|doc| doc.path.as_path())
                    .or(self.image_path())
                    .is_some_and(|path| path.starts_with(&absolute)));
        let observed = self.observed_document(cx);
        let next = Next::Entry { target, action };
        let effects = if affects_content {
            self.session.request(next, observed)
        } else {
            self.session.request_retaining(next, observed)
        };
        self.apply_session_effects(effects, window, cx);
    }
    pub(crate) fn entry_with_operation(
        &mut self,
        operation: OperationId,
        target: EntryTarget,
        action: EntryAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.entry_target_current(&target) {
            let observed = self.observed_document(cx);
            let effects = self.session.continued(operation, observed);
            self.apply_session_effects(effects, window, cx);
            return;
        }
        let absolute = target.root.join(&target.path);
        let contains_active = self
            .document()
            .map(|doc| doc.path.as_path())
            .or(self.image_path())
            .is_some_and(|path| path.starts_with(&absolute));
        let overlay = self
            .document()
            .filter(|doc| doc.path == absolute)
            .map(|doc| (self.editor.snapshot(cx), doc.format));
        let deleting = matches!(action, EntryAction::Trash);
        let effects = self.session.begin_entry_operation(operation);
        if effects.is_empty() {
            return;
        }
        self.apply_session_effects(effects, window, cx);
        #[cfg(test)]
        let trash_backend = self.notebooks.trash_backend;
        cx.spawn_in(window, async move |this, cx| {
            let worker = target.clone();
            let result = cx
                .background_executor()
                .spawn(async move {
                    match action {
                        EntryAction::NewNote => {
                            let directory = notebook::entry_directory(&worker.root, &worker.path)?;
                            let path = notebook::create_untitled(&directory)?;
                            let relative = path
                                .strip_prefix(&worker.root)
                                .map_err(io::Error::other)?
                                .to_path_buf();
                            Document::open(path).map(|doc| (Some(relative), Some(doc)))
                        }
                        EntryAction::CreateFolder(name) => {
                            notebook::create_folder(&worker.root, &worker.path, &name)
                                .map(|path| (Some(path), None))
                        }
                        EntryAction::Duplicate => {
                            let bytes = overlay
                                .map(|(text, format)| document::encode(&text.to_string(), format));
                            notebook::duplicate_entry(&worker.root, &worker.path, bytes.as_deref())
                                .map(|path| (Some(path), None))
                        }
                        EntryAction::Trash => {
                            #[cfg(test)]
                            trash_backend(&worker.root, &worker.path)?;
                            #[cfg(not(test))]
                            notebook::trash_entry(&worker.root, &worker.path)?;
                            Ok((None, None))
                        }
                        _ => Err(io::Error::other("Invalid entry operation.")),
                    }
                })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.session.foreground()
                    != Some((operation, app::session::PhaseKind::EntryOperation))
                {
                    return;
                }
                let previous = this
                    .document()
                    .zip(this.document_id())
                    .map(|(doc, id)| (id, doc.path.clone()));
                let observed = this.observed_document(cx);
                let success = result.is_ok();
                let (created, document) = match result {
                    Ok(result) => {
                        this.notebooks.notice = None;
                        result
                    }
                    Err(error) => {
                        this.notebooks.notice = Some(format!("Could not update entry: {error}"));
                        (None, None)
                    }
                };
                let effects = if deleting {
                    this.session
                        .entry_deleted(operation, success, contains_active, observed)
                } else {
                    this.session.entry_operation_finished(
                        operation,
                        document.map(app::session::Loaded::Document),
                        observed,
                    )
                };
                for effect in effects {
                    if let app::session::Effect::Installed {
                        discarded_document, ..
                    } = effect
                    {
                        if !deleting && this.mode == ViewMode::Reading {
                            this.set_mode(ViewMode::Source, window, cx);
                        }
                        this.install_session_editor(window, cx);
                        this.search_document_installed(
                            previous
                                .as_ref()
                                .filter(|(id, _)| Some(*id) == discarded_document)
                                .map(|(_, path)| path.as_path()),
                            window,
                            cx,
                        );
                        if !deleting {
                            this.remember_file(cx);
                        }
                    } else {
                        this.apply_session_effects(vec![effect], window, cx);
                    }
                }
                if success {
                    if let Some(book) = &mut this.notebooks.active
                        && book.root == target.root
                    {
                        if deleting {
                            book.settings
                                .expanded
                                .retain(|path| !path.starts_with(&target.path));
                            if book
                                .settings
                                .last_file
                                .as_ref()
                                .is_some_and(|path| path.starts_with(&target.path))
                            {
                                book.settings.last_file = None;
                            }
                        }
                        if let Some(path) = &created {
                            for parent in path
                                .ancestors()
                                .skip(1)
                                .filter(|p| !p.as_os_str().is_empty())
                            {
                                book.settings.expanded.insert(parent.to_path_buf());
                            }
                        }
                    }
                    this.notebooks.select_after_refresh = created;
                    this.persist_notebooks(cx);
                    this.refresh_sorted_tree(window, cx);
                    this.schedule_search(window, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::super::persistence_tests::fixture;
    use super::{Document, EntryAction, EntryDrag, EntryTarget, Next, Notes, actions, notebook};
    use gpui::TestAppContext;
    use std::fs;
    use std::{
        io,
        path::{Path, PathBuf},
    };
    fn target(notes: &Notes, path: &str, directory: bool) -> EntryTarget {
        EntryTarget {
            root: notes.notebooks.active.as_ref().unwrap().root.clone(),
            path: path.into(),
            generation: notes.notebooks.explorer.generation(),
            directory,
        }
    }
    fn fake_trash(root: &Path, relative: &Path) -> io::Result<()> {
        let from = notebook::entry_path(root, relative)?;
        fs::rename(from, root.join("trashed-fixture"))
    }
    #[::core::prelude::v1::test]
    fn menus_have_the_requested_order_and_folder_specific_actions() {
        let file = actions(false, false);
        assert_eq!(
            file.iter()
                .map(|(name, _)| *name)
                .take(4)
                .collect::<Vec<_>>(),
            ["Open", "Rename", "Duplicate", "Copy relative path"]
        );
        assert_eq!(file.last().unwrap().0, "Move to Trash");
        assert_eq!(actions(true, false)[0].0, "New note");
        assert_eq!(actions(true, true).len(), 3);
    }
    #[gpui::test]
    fn right_click_selects_without_opening_and_keyboard_menu_uses_selection(
        cx: &mut TestAppContext,
    ) {
        let (fixture, notes, cx) = fixture(cx);
        fs::write(fixture.root.join("other.md"), "other").unwrap();
        cx.update(|window, cx| notes.update(cx, |this, cx| this.refresh_tree(window, cx)));
        cx.run_until_parked();
        cx.refresh().unwrap();
        cx.run_until_parked();
        let bounds = cx.debug_bounds("notebook-row-other.md").unwrap();
        cx.simulate_mouse_down(
            bounds.center(),
            gpui::MouseButton::Right,
            gpui::Modifiers::default(),
        );
        cx.simulate_mouse_up(
            bounds.center(),
            gpui::MouseButton::Right,
            gpui::Modifiers::default(),
        );
        cx.run_until_parked();
        cx.update(|window, cx| {
            notes.update(cx, |this, _cx| {
                assert_eq!(this.document().unwrap().path, fixture.note);
                assert_eq!(
                    this.notebooks.explorer.selected().unwrap().path,
                    Path::new("other.md")
                );
                assert!(this.notebooks.context_menu.is_some());
                this.notebooks.context_menu = None;
                this.notebooks.tree_focus.focus(window);
            })
        });
        cx.dispatch_action(crate::TreeContextMenu);
        cx.run_until_parked();
        cx.update(|_, cx| assert!(notes.read(cx).notebooks.context_menu.is_some()));
    }
    #[gpui::test]
    fn duplicate_copies_unsaved_source_and_selects_copy_without_navigation(
        cx: &mut TestAppContext,
    ) {
        let (fixture, notes, cx) = fixture(cx);
        let id = cx.update(|window, cx| {
            notes.update(cx, |this, cx| {
                this.editor
                    .select(0..this.editor.snapshot(cx).len(), window, cx);
                this.editor.replace_selection("unsaved café\n", window, cx);
                let id = this.editor.id();
                this.request_entry(
                    target(this, "note.md", false),
                    EntryAction::Duplicate,
                    window,
                    cx,
                );
                id
            })
        });
        cx.run_until_parked();
        assert_eq!(
            fs::read_to_string(fixture.root.join("note copy.md")).unwrap(),
            "unsaved café\n"
        );
        assert_eq!(fs::read_to_string(&fixture.note).unwrap(), "original A");
        cx.update(|_, cx| {
            let this = notes.read(cx);
            assert_eq!(this.editor.id(), id);
            assert!(this.dirty(cx));
            assert_eq!(
                this.notebooks.explorer.selected().unwrap().path,
                Path::new("note copy.md")
            );
        });
        assert!(!cx.has_pending_prompt());
    }
    #[gpui::test]
    fn create_in_folder_and_stale_targets_are_guarded(cx: &mut TestAppContext) {
        let (fixture, notes, cx) = fixture(cx);
        fs::create_dir(fixture.root.join("folder")).unwrap();
        cx.update(|window, cx| {
            notes.update(cx, |this, cx| {
                let old = target(this, "folder", true);
                this.notebooks.explorer.begin_refresh();
                this.request_entry(old, EntryAction::CreateFolder("stale".into()), window, cx);
                this.request_entry(
                    target(this, "folder", true),
                    EntryAction::NewNote,
                    window,
                    cx,
                );
            })
        });
        cx.run_until_parked();
        assert!(!fixture.root.join("folder/stale").exists());
        assert!(fixture.root.join("folder/Untitled.md").exists());
        cx.update(|_, cx| {
            assert_eq!(
                notes.read(cx).document().unwrap().path,
                fixture.root.join("folder/Untitled.md")
            )
        });
    }
    #[gpui::test]
    fn trash_failure_retains_draft_and_success_clears_content_and_settings(
        cx: &mut TestAppContext,
    ) {
        let (fixture, notes, cx) = fixture(cx);
        let id = cx.update(|window, cx| {
            notes.update(cx, |this, cx| {
                this.editor.replace_selection("draft", window, cx);
                let id = this.editor.id();
                let target = target(this, "note.md", false);
                this.diagnostic_answer_request(
                    Some(1),
                    Next::Entry {
                        target,
                        action: EntryAction::Trash,
                    },
                    window,
                    cx,
                );
                id
            })
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            notes.update(cx, |this, cx| {
                assert_eq!(this.editor.id(), id);
                assert!(this.dirty(cx));
                assert!(
                    this.notebooks
                        .notice
                        .as_ref()
                        .unwrap()
                        .contains("OS Trash is disabled")
                );
                this.notebooks.trash_backend = fake_trash;
                let target = target(this, "note.md", false);
                this.diagnostic_answer_request(
                    Some(1),
                    Next::Entry {
                        target,
                        action: EntryAction::Trash,
                    },
                    window,
                    cx,
                );
            })
        });
        cx.run_until_parked();
        assert!(!fixture.note.exists());
        assert!(fixture.root.join("trashed-fixture").exists());
        cx.update(|_, cx| {
            let this = notes.read(cx);
            assert!(this.document().is_none());
            assert!(
                this.notebooks
                    .active
                    .as_ref()
                    .unwrap()
                    .settings
                    .last_file
                    .is_none()
            );
        });
    }
    #[gpui::test]
    fn rename_active_ancestor_preserves_buffer_history_and_expansion(cx: &mut TestAppContext) {
        let (fixture, notes, cx) = fixture(cx);
        fs::create_dir(fixture.root.join("folder")).unwrap();
        fs::write(fixture.root.join("folder/note.md"), "original").unwrap();
        cx.update(|window, cx| {
            notes.update(cx, |this, cx| {
                this.install_document(
                    Some(Document::open(fixture.root.join("folder/note.md")).unwrap()),
                    window,
                    cx,
                );
                this.notebooks
                    .active
                    .as_mut()
                    .unwrap()
                    .settings
                    .expanded
                    .insert("folder".into());
                this.editor.select(0..8, window, cx);
                this.editor.replace_selection("draft", window, cx);
                this.move_sidebar_entry(
                    &EntryDrag {
                        root: fixture.root.clone(),
                        path: "folder".into(),
                    },
                    PathBuf::new(),
                    Some("renamed".into()),
                    window,
                    cx,
                );
            })
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            notes.update(cx, |this, cx| {
                assert_eq!(
                    this.document().unwrap().path,
                    fixture.root.join("renamed/note.md")
                );
                assert_eq!(this.editor.text(cx).as_ref(), "draft");
                assert!(
                    this.notebooks
                        .active
                        .as_ref()
                        .unwrap()
                        .settings
                        .expanded
                        .contains(Path::new("renamed"))
                );
                this.editor.focus(window, cx);
            })
        });
        cx.dispatch_action(gpui_component::input::Undo);
        cx.run_until_parked();
        cx.update(|_, cx| assert_eq!(notes.read(cx).editor.text(cx).as_ref(), "original"));
    }
}
