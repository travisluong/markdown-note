use super::*;

pub(super) struct Shortcut {
    key: String,
    description: &'static str,
    group: &'static str,
    pub binding: KeyBinding,
}

pub(super) fn app_shortcuts() -> Vec<Shortcut> {
    app_shortcuts_for_platform(cfg!(target_os = "macos"))
}

fn app_shortcuts_for_platform(macos: bool) -> Vec<Shortcut> {
    macro_rules! shortcut {
        ($key:literal, $action:expr, $context:literal, $group:literal, $description:literal) => {{
            let key = platform_shortcut($key, macos);
            let binding = KeyBinding::new(&key, $action, Some($context));
            Shortcut {
                key,
                description: $description,
                group: $group,
                binding,
            }
        }};
    }
    vec![
        shortcut!(
            "shift-f10",
            TreeContextMenu,
            "NotebookTree",
            "Notebook tree",
            "Open selected item context menu"
        ),
        shortcut!(
            "ctrl-up",
            MoveLineUp,
            "DocumentEditor",
            "Editor",
            "Move line or selected lines up"
        ),
        shortcut!(
            "ctrl-down",
            MoveLineDown,
            "DocumentEditor",
            "Editor",
            "Move line or selected lines down"
        ),
        shortcut!("ctrl-b", Bold, "DocumentEditor", "Editor", "Toggle bold"),
        shortcut!(
            "ctrl-i",
            Italic,
            "DocumentEditor",
            "Editor",
            "Toggle italic"
        ),
        shortcut!("ctrl-o", GoTo, "Notes", "Application", "Go to a note"),
        shortcut!(
            "ctrl-p",
            CommandPalette,
            "Notes",
            "Application",
            "Command palette"
        ),
        shortcut!("ctrl-s", Save, "Notes", "Application", "Save"),
        shortcut!(
            "alt-left",
            NavigateBack,
            "Notes",
            "Application",
            "Back in tab history"
        ),
        shortcut!(
            "alt-right",
            NavigateForward,
            "Notes",
            "Application",
            "Forward in tab history"
        ),
        shortcut!("ctrl-,", OpenSettings, "Notes", "Application", "Settings"),
        shortcut!(
            "ctrl-shift-b",
            ToggleSidebar,
            "Notes",
            "Application",
            "Toggle sidebar"
        ),
        shortcut!(
            "ctrl-shift-e",
            ShowExplorer,
            "Notes",
            "Application",
            "Show Explorer"
        ),
        shortcut!(
            "ctrl-shift-m",
            CycleViewMode,
            "Notes",
            "Application",
            "Cycle Source → Live preview → Reading"
        ),
        shortcut!(
            "ctrl-n",
            NewNote,
            "Notes",
            "Application",
            "New note in notebook root"
        ),
        shortcut!("ctrl-t", NewTab, "Notes", "Application", "New empty tab"),
        shortcut!(
            "ctrl-w",
            CloseTab,
            "Notes",
            "Application",
            "Close current tab"
        ),
        shortcut!("alt-f", FileMenu, "Notes", "Application", "File menu"),
        shortcut!("alt-v", ViewMenu, "Notes", "Application", "View menu"),
        shortcut!("alt-s", SearchMenu, "Notes", "Application", "Search menu"),
        shortcut!(
            "alt-n",
            NotebooksMenu,
            "Notes",
            "Application",
            "Notebooks menu"
        ),
        shortcut!("alt-h", HelpMenu, "Notes", "Application", "Help menu"),
        shortcut!(
            "ctrl-l",
            ToggleTasks,
            "DocumentEditor",
            "Editor",
            "Create or toggle selected tasks"
        ),
        shortcut!(
            "ctrl-k",
            EditLink,
            "DocumentEditor",
            "Editor",
            "Insert or edit a link"
        ),
        shortcut!(
            "ctrl-f",
            Search,
            "Notes",
            "Search",
            "Find in the current file"
        ),
        shortcut!(
            "ctrl-shift-f",
            SearchAllFiles,
            "Notes",
            "Search",
            "Search all notebook files"
        ),
        shortcut!(
            "ctrl-h",
            ReplaceFile,
            "Notes",
            "Search",
            "Find and replace in the current file"
        ),
        shortcut!(
            "escape",
            CloseReadingFind,
            "ReadingContent",
            "Search",
            "Close Reading find"
        ),
        shortcut!(
            "up",
            TreeUp,
            "NotebookTree",
            "Notebook tree",
            "Previous item"
        ),
        shortcut!(
            "down",
            TreeDown,
            "NotebookTree",
            "Notebook tree",
            "Next item"
        ),
        shortcut!(
            "left",
            TreeLeft,
            "NotebookTree",
            "Notebook tree",
            "Collapse folder or move to parent"
        ),
        shortcut!(
            "right",
            TreeRight,
            "NotebookTree",
            "Notebook tree",
            "Expand folder or move to child"
        ),
        shortcut!(
            "enter",
            TreeOpen,
            "NotebookTree",
            "Notebook tree",
            "Open selected item"
        ),
    ]
}

fn platform_shortcut(key: &str, macos: bool) -> String {
    if macos {
        // Preserve Option-arrow word movement and Command-bracket indentation.
        if matches!(key, "alt-left" | "alt-right") {
            return format!("cmd-{key}");
        }
        // Command-H belongs to the system Hide action.
        if key == "ctrl-h" {
            return "cmd-alt-f".into();
        }
        if let Some(rest) = key.strip_prefix("ctrl-") {
            return format!("cmd-{rest}");
        }
        // Option-letter combinations enter text on macOS.
        if let Some(rest) = key.strip_prefix("alt-") {
            return format!("ctrl-alt-{rest}");
        }
    }
    key.to_owned()
}

// Base labels audited against the vendored InputState and table handlers.
const CONTEXT_SHORTCUTS: &[(&str, &str, &str)] = &[
    ("Editor", "Ctrl+A", "Select all"),
    ("Editor", "Ctrl+C / X / V", "Copy / cut / paste"),
    ("Editor", "Ctrl+Z / Ctrl+Y", "Undo / redo"),
    ("Editor", "Arrows / Page Up / Page Down", "Move the caret"),
    ("Editor", "Home / End", "Move to line start / end"),
    ("Editor", "Ctrl+Left / Right", "Move by word"),
    ("Editor", "Shift+Arrows / Home / End", "Extend selection"),
    (
        "Editor",
        "Ctrl+Shift+Left / Right",
        "Extend selection by word",
    ),
    ("Editor", "Backspace / Delete", "Delete backward / forward"),
    (
        "Editor",
        "Ctrl+Backspace / Delete",
        "Delete previous / next word",
    ),
    (
        "Editor",
        "Enter",
        "New line; continue a list or exit an empty item",
    ),
    (
        "Editor",
        "Tab / Shift+Tab",
        "Nest / outdent list items; otherwise indent / outdent",
    ),
    ("Editor", "Ctrl+] / Ctrl+[", "Indent / outdent lines"),
    (
        "Search",
        "Up / Down (all files)",
        "Select and immediately reveal a result",
    ),
    ("Search", "Enter (all files)", "Open selected result"),
    ("Search", "Escape (all files)", "Return focus to the editor"),
    (
        "Search",
        "Enter / Shift+Enter (file find)",
        "Next / previous match",
    ),
    ("Search", "Escape (file find)", "Close find"),
    (
        "Table navigation",
        "Tab / Shift+Tab",
        "Next / previous cell; Tab at the end adds a row",
    ),
    (
        "Table navigation",
        "Enter",
        "Move to the next row; add a row at the end",
    ),
    (
        "Table navigation",
        "Arrows",
        "Move within a cell; cross cells at their boundaries",
    ),
    ("Table navigation", "Home / End", "Move to cell start / end"),
    ("Table navigation", "Escape", "Leave table navigation"),
];

fn context_shortcut(key: &str, macos: bool) -> String {
    if !macos {
        return key.to_owned();
    }
    match key {
        "Ctrl+Z / Ctrl+Y" => "Cmd+Z / Cmd+Shift+Z".into(),
        "Ctrl+Left / Right" => "Option+Left / Right".into(),
        "Ctrl+Shift+Left / Right" => "Option+Shift+Left / Right".into(),
        "Ctrl+Backspace / Delete" => "Option+Backspace / Delete".into(),
        _ => key.replace("Ctrl+", "Cmd+"),
    }
}

impl Notes {
    pub(super) fn open_help(
        &mut self,
        shortcuts: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if window.has_active_dialog(cx) {
            return;
        }
        let previous = self
            .menu_previous_focus
            .take()
            .or_else(|| window.focused(cx));
        self.menu = None;
        window.open_dialog(cx, move |dialog, window, _cx| {
            let mut body = div().flex().flex_col().gap_2();
            if shortcuts {
                let app = app_shortcuts();
                for group in [
                    "Application",
                    "Editor",
                    "Search",
                    "Notebook tree",
                    "Table navigation",
                ] {
                    body = body.child(div().mt_2().font_weight(FontWeight::BOLD).child(group));
                    for shortcut in app.iter().filter(|s| s.group == group) {
                        body = body.child(shortcut_row(&shortcut.key, shortcut.description));
                    }
                    for (_, key, description) in CONTEXT_SHORTCUTS.iter().filter(|s| s.0 == group) {
                        body = body.child(shortcut_row(
                            &context_shortcut(key, cfg!(target_os = "macos")),
                            description,
                        ));
                    }
                }
            } else {
                body = body
                    .child("Mithril")
                    .child(format!("Version {}", env!("CARGO_PKG_VERSION")))
                    .child("A Markdown notebook built with GPUI. More information coming soon.");
            }
            let restore = previous.clone();
            let close_focus = previous.clone();
            dialog
                .title(if shortcuts {
                    "Keyboard shortcuts"
                } else {
                    "About Mithril"
                })
                .w(px(700.).min(window.viewport_size().width - px(32.)))
                // Reserve both window margins and the dialog's 30px slide animation.
                .margin_top(px(16.))
                .max_h((window.viewport_size().height - px(62.)).max(px(0.)))
                .child(body)
                .footer(move |_, _, _, _| {
                    let close_focus = close_focus.clone();
                    vec![
                        Button::new("close-help")
                            .label("Close")
                            .debug_selector(|| "close-help".into())
                            .on_click(move |_, window, cx| {
                                window.close_dialog(cx);
                                if let Some(focus) = &close_focus {
                                    focus.focus(window);
                                }
                            }),
                    ]
                })
                .on_close(move |_, window, _| {
                    if let Some(focus) = &restore {
                        focus.focus(window);
                    }
                })
        });
        cx.notify();
    }
}

fn shortcut_row(key: &str, description: &str) -> impl IntoElement {
    div()
        .flex()
        .flex_wrap()
        .gap_2()
        .child(div().w(px(250.)).child(key.to_owned()))
        .child(div().flex_1().child(description.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::app_shortcuts;

    #[test]
    fn sidebar_bindings_document_both_platforms_without_app_conflicts() {
        for (macos, modifier) in [(false, "ctrl"), (true, "cmd")] {
            let shortcuts = super::app_shortcuts_for_platform(macos);
            let toggle = format!("{modifier}-shift-b");
            let explorer = format!("{modifier}-shift-e");
            let toggles: Vec<_> = shortcuts.iter().filter(|s| s.key == toggle).collect();
            let explorers: Vec<_> = shortcuts.iter().filter(|s| s.key == explorer).collect();
            assert_eq!(toggles.len(), 1);
            assert_eq!(explorers.len(), 1);
            assert!(
                toggles[0]
                    .binding
                    .action()
                    .as_any()
                    .is::<crate::ToggleSidebar>()
            );
            assert!(
                explorers[0]
                    .binding
                    .action()
                    .as_any()
                    .is::<crate::ShowExplorer>()
            );
            assert_eq!(toggles[0].description, "Toggle sidebar");
            assert_eq!(explorers[0].description, "Show Explorer");
        }
    }

    #[test]
    fn history_bindings_document_exact_platform_keys_and_actions() {
        for (macos, back, forward) in [
            (false, "alt-left", "alt-right"),
            (true, "cmd-alt-left", "cmd-alt-right"),
        ] {
            let shortcuts = super::app_shortcuts_for_platform(macos);
            let back_binding = shortcuts.iter().find(|s| s.key == back).unwrap();
            let forward_binding = shortcuts.iter().find(|s| s.key == forward).unwrap();
            assert!(
                back_binding
                    .binding
                    .action()
                    .as_any()
                    .is::<crate::NavigateBack>()
            );
            assert!(
                forward_binding
                    .binding
                    .action()
                    .as_any()
                    .is::<crate::NavigateForward>()
            );
            assert_eq!(back_binding.description, "Back in tab history");
            assert_eq!(forward_binding.description, "Forward in tab history");
            if macos {
                assert!(!shortcuts.iter().any(|s| matches!(
                    s.key.as_str(),
                    "alt-left" | "alt-right" | "cmd-[" | "cmd-]"
                )));
            }
        }
    }
    #[test]
    fn settings_and_replace_bindings_preserve_mac_hide_and_document_both_platforms() {
        for (macos, settings, replace) in
            [(false, "ctrl-,", "ctrl-h"), (true, "cmd-,", "cmd-alt-f")]
        {
            let shortcuts = super::app_shortcuts_for_platform(macos);
            let mut keys = std::collections::BTreeSet::new();
            for shortcut in &shortcuts {
                gpui::Keystroke::parse(&shortcut.key).unwrap();
                assert!(keys.insert(shortcut.key.as_str()));
            }
            let settings_binding = shortcuts.iter().find(|s| s.key == settings).unwrap();
            assert!(
                settings_binding
                    .binding
                    .action()
                    .as_any()
                    .is::<crate::OpenSettings>()
            );
            assert_eq!(settings_binding.description, "Settings");
            let replace_binding = shortcuts.iter().find(|s| s.key == replace).unwrap();
            assert!(
                replace_binding
                    .binding
                    .action()
                    .as_any()
                    .is::<crate::ReplaceFile>()
            );
            assert_eq!(
                replace_binding.description,
                "Find and replace in the current file"
            );
            if macos {
                assert!(!keys.contains("cmd-h"));
            }
        }
    }

    #[gpui::test]
    fn shortcuts_keep_close_button_inside_window_when_resized(cx: &mut gpui::TestAppContext) {
        use crate::{Notes, NotesWindow};
        use gpui::{AppContext, px, size};
        use gpui_component::Root;

        cx.update(gpui_component::init);
        let mut notes_view = None;
        let (_, cx) = cx.add_window_view(|window, cx| {
            let notes = cx.new(|cx| Notes::new(window, cx));
            notes_view = Some(notes.clone());
            let view = cx.new(|cx| NotesWindow {
                _subscription: cx.observe(&notes, |_, _, cx| cx.notify()),
                notes,
            });
            Root::new(view, window, cx)
        });
        cx.update(|window, cx| {
            notes_view
                .unwrap()
                .update(cx, |notes, cx| notes.open_help(true, window, cx));
        });

        for (width, height) in [(1000., 900.), (800., 600.), (480., 320.), (1000., 1200.)] {
            cx.simulate_resize(size(px(width), px(height)));
            cx.refresh().unwrap();
            cx.run_until_parked();
            cx.executor()
                .advance_clock(std::time::Duration::from_millis(300));
            cx.refresh().unwrap();
            cx.run_until_parked();

            let close = cx.debug_bounds("close-help").unwrap();
            assert!(close.size.height > px(0.), "{close:?}");
            assert!(close.top() >= px(16.), "{close:?}");
            assert!(close.bottom() <= px(height - 16.), "{close:?}");
            assert!(close.right() <= px(width - 16.), "{close:?}");
        }
    }

    #[test]
    fn app_bindings_have_unique_documented_shortcuts() {
        let shortcuts = app_shortcuts();
        let mut keys = std::collections::BTreeSet::new();
        for shortcut in shortcuts {
            assert!(!shortcut.description.is_empty());
            assert!(keys.insert(shortcut.key));
        }
        assert!(keys.contains(&super::platform_shortcut(
            "alt-h",
            cfg!(target_os = "macos")
        )));
        assert!(keys.contains(&super::platform_shortcut(
            "ctrl-shift-f",
            cfg!(target_os = "macos")
        )));
    }

    #[test]
    fn mac_shortcuts_use_command_and_preserve_option_text_entry() {
        for shortcut in app_shortcuts() {
            let mac_key = super::platform_shortcut(&shortcut.key, true);
            gpui::Keystroke::parse(&mac_key).unwrap();
        }
        assert_eq!(super::platform_shortcut("ctrl-s", true), "cmd-s");
        assert_eq!(
            super::platform_shortcut("ctrl-shift-f", true),
            "cmd-shift-f"
        );
        assert_eq!(super::platform_shortcut("alt-n", true), "ctrl-alt-n");
        assert_eq!(super::platform_shortcut("ctrl-s", false), "ctrl-s");
        assert_eq!(
            super::context_shortcut("Ctrl+Z / Ctrl+Y", true),
            "Cmd+Z / Cmd+Shift+Z"
        );
        assert_eq!(
            super::context_shortcut("Ctrl+Left / Right", true),
            "Option+Left / Right"
        );
    }
}
