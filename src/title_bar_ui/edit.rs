//! Edit menu presentation; document commands retain their existing owners.
use super::*;
use gpui::{Action, App, EntityId, FocusHandle};
use gpui_component::input::{Copy, Cut, Paste, Redo, Rope, SelectAll, Undo};
use mithril::commands::Command;
use std::ops::Range;

#[derive(Clone)]
struct Target {
    editor: EntityId,
    document: Option<crate::app::session::DocumentId>,
    source: Rope,
    selection: Range<usize>,
    mode: ViewMode,
}

impl Target {
    fn capture(notes: &Notes, cx: &App) -> Self {
        Self {
            editor: notes.tabs.active.editor.id(),
            document: notes.document_id(),
            source: notes.tabs.active.editor.snapshot(cx),
            selection: notes.tabs.active.editor.selection(cx),
            mode: notes.tabs.active.mode,
        }
    }

    fn is_current(&self, notes: &Notes, window: &mut Window, cx: &mut App) -> bool {
        !notes.document_interaction_blocked()
            && !window.has_active_dialog(cx)
            && notes.document_id() == self.document
            && notes.tabs.active.editor.id() == self.editor
            && notes.tabs.active.mode == self.mode
            && notes.tabs.active.editor.snapshot(cx) == self.source
            && notes.tabs.active.editor.selection(cx) == self.selection
    }
}

impl Notes {
    pub(super) fn edit_menu(
        &self,
        mut menu: PopupMenu,
        owner: WeakEntity<Self>,
        window: &mut Window,
        cx: &mut Context<PopupMenu>,
    ) -> PopupMenu {
        let target = Target::capture(self, cx);
        let editable = self.can_edit_document();
        let editor_focus = self.tabs.active.editor.focus_handle(cx);
        // Reading Copy targets the rendered view, even if a sidebar had focus.
        // Source/Live commands always act on the remembered document selection.
        let focus = if self.document().is_none() {
            self.menu_previous_focus
                .clone()
                .unwrap_or_else(|| self.focus.clone())
        } else if self.tabs.active.mode == ViewMode::Reading {
            self.tabs
                .active
                .reading_view
                .as_ref()
                .map(|view| view.focus_handle(cx))
                .unwrap_or_else(|| self.focus.clone())
        } else {
            editor_focus.clone()
        };
        let item =
            |label, command| command_item(label, command, editable, &owner, &target, &editor_focus);
        menu = menu.action_context(focus.clone());
        for (label, action, enabled) in [
            ("Undo", Box::new(Undo) as Box<dyn Action>, editable),
            ("Redo", Box::new(Redo), editable),
            ("Cut", Box::new(Cut), editable),
            (
                "Copy",
                Box::new(Copy),
                self.document().is_some() && !self.document_interaction_blocked(),
            ),
            ("Paste", Box::new(Paste), editable),
            ("Select all", Box::new(SelectAll), editable),
        ] {
            if label == "Cut" {
                menu = menu.separator();
            }
            let owner = owner.clone();
            let target = target.clone();
            let focus = focus.clone();
            let read_only = action.as_ref().as_any().is::<Copy>();
            let dispatch = action.boxed_clone();
            menu = menu.item(
                PopupMenuItem::new(label)
                    .action(action)
                    .disabled(!enabled)
                    .on_click(move |_, window, cx| {
                        let owner = owner.clone();
                        let target = target.clone();
                        let focus = focus.clone();
                        let dispatch = dispatch.boxed_clone();
                        window.defer(cx, move |window, cx| {
                            let _ = owner.update(cx, |this, cx| {
                                if enabled
                                    && target.is_current(this, window, cx)
                                    && (read_only || this.can_edit_document())
                                {
                                    focus.focus(window);
                                    window.dispatch_action(dispatch, cx);
                                }
                            });
                        });
                    }),
            );
        }
        menu.separator()
            .submenu("Format", window, cx, |menu, window, cx| {
                let mut menu = menu.action_context(editor_focus.clone());
                for (label, command) in [
                    ("Bold", Command::Bold),
                    ("Italic", Command::Italic),
                    ("Strikethrough", Command::Strikethrough),
                    ("Inline code", Command::InlineCode),
                ] {
                    menu = menu.item(item(label, command));
                }
                menu.separator()
                    .submenu("Headings", window, cx, |mut menu, _, _| {
                        menu = menu.action_context(editor_focus.clone());
                        for (label, command) in [
                            ("Heading 1", Command::Heading1),
                            ("Heading 2", Command::Heading2),
                            ("Heading 3", Command::Heading3),
                            ("Heading 4", Command::Heading4),
                            ("Heading 5", Command::Heading5),
                            ("Heading 6", Command::Heading6),
                            ("Paragraph", Command::Paragraph),
                        ] {
                            menu = menu.item(item(label, command));
                        }
                        menu
                    })
                    .submenu("Lists", window, cx, |mut menu, _, _| {
                        menu = menu.action_context(editor_focus.clone());
                        for (label, command) in [
                            ("Ordered list", Command::OrderedList),
                            ("Unordered list", Command::UnorderedList),
                            ("Task list", Command::TaskList),
                        ] {
                            menu = menu.item(item(label, command));
                        }
                        menu
                    })
                    .item(item("Blockquote", Command::Blockquote))
                    .item(item("Code block", Command::CodeBlock))
                    .separator()
                    .item(item("Link…", Command::EditLink))
            })
            .separator()
            .item(item("Insert date", Command::InsertDate))
            .item(item("Toggle task completion", Command::ToggleTasks))
    }
}

fn command_item(
    label: &'static str,
    command: Command,
    enabled: bool,
    owner: &WeakEntity<Notes>,
    target: &Target,
    focus: &FocusHandle,
) -> PopupMenuItem {
    let owner = owner.clone();
    let target = target.clone();
    let focus = focus.clone();
    let mut item = PopupMenuItem::new(label).disabled(!enabled);
    if let Some((action, _)) = crate::help_ui::command_action(command) {
        item = item.action(action);
    }
    item.on_click(move |_, window, cx| {
        let owner = owner.clone();
        let target = target.clone();
        let focus = focus.clone();
        // Run after every ancestor popup restores focus, including More > Edit.
        window.defer(cx, move |window, cx| {
            let _ = owner.update(cx, |this, cx| {
                if target.is_current(this, window, cx) && this.can_edit_document() {
                    focus.focus(window);
                    this.run_command(command, window, cx);
                }
            });
        });
    })
}
