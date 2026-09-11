use super::*;
use gpui_component::scroll::ScrollableElement;

pub(super) struct Shortcut {
    key: &'static str,
    description: &'static str,
    group: &'static str,
    pub binding: KeyBinding,
}

pub(super) fn app_shortcuts() -> Vec<Shortcut> {
    macro_rules! shortcut {
        ($key:literal, $action:expr, $context:literal, $group:literal, $description:literal) => {
            Shortcut { key: $key, description: $description, group: $group, binding: KeyBinding::new($key, $action, Some($context)) }
        };
    }
    vec![
        shortcut!("ctrl-o", GoTo, "Notes", "Application", "Go to a note"),
        shortcut!("ctrl-p", CommandPalette, "Notes", "Application", "Command palette"),
        shortcut!("ctrl-s", Save, "Notes", "Application", "Save"),
        shortcut!("alt-f", FileMenu, "Notes", "Application", "File menu"),
        shortcut!("alt-v", ViewMenu, "Notes", "Application", "View menu"),
        shortcut!("alt-n", NotebooksMenu, "Notes", "Application", "Notebooks menu"),
        shortcut!("alt-h", HelpMenu, "Notes", "Application", "Help menu"),
        shortcut!("ctrl-l", ToggleTasks, "DocumentEditor", "Editor", "Create or toggle selected tasks"),
        shortcut!("ctrl-k", EditLink, "DocumentEditor", "Editor", "Insert or edit a link"),
        shortcut!("ctrl-f", Search, "Notes", "Search", "Find in the current file"),
        shortcut!("ctrl-shift-f", SearchAllFiles, "Notes", "Search", "Search all notebook files"),
        shortcut!("escape", CloseReadingFind, "ReadingContent", "Search", "Close Reading find"),
        shortcut!("up", TreeUp, "NotebookTree", "Notebook tree", "Previous item"),
        shortcut!("down", TreeDown, "NotebookTree", "Notebook tree", "Next item"),
        shortcut!("left", TreeLeft, "NotebookTree", "Notebook tree", "Collapse folder or move to parent"),
        shortcut!("right", TreeRight, "NotebookTree", "Notebook tree", "Expand folder or move to child"),
        shortcut!("enter", TreeOpen, "NotebookTree", "Notebook tree", "Open selected item"),
    ]
}

// Windows bindings audited against the vendored InputState and table handlers.
const CONTEXT_SHORTCUTS: &[(&str, &str, &str)] = &[
    ("Editor", "Ctrl+A", "Select all"),
    ("Editor", "Ctrl+C / X / V", "Copy / cut / paste"),
    ("Editor", "Ctrl+Z / Ctrl+Y", "Undo / redo"),
    ("Editor", "Arrows / Page Up / Page Down", "Move the caret"),
    ("Editor", "Home / End", "Move to line start / end"),
    ("Editor", "Ctrl+Left / Right", "Move by word"),
    ("Editor", "Shift+Arrows / Home / End", "Extend selection"),
    ("Editor", "Ctrl+Shift+Left / Right", "Extend selection by word"),
    ("Editor", "Backspace / Delete", "Delete backward / forward"),
    ("Editor", "Ctrl+Backspace / Delete", "Delete previous / next word"),
    ("Editor", "Enter", "New line; continue a list or exit an empty item"),
    ("Editor", "Tab / Shift+Tab", "Nest / outdent list items; otherwise indent / outdent"),
    ("Editor", "Ctrl+] / Ctrl+[", "Indent / outdent lines"),
    ("Search", "Up / Down (all files)", "Select and immediately reveal a result"),
    ("Search", "Enter (all files)", "Open selected result"),
    ("Search", "Escape (all files)", "Return focus to the editor"),
    ("Search", "Enter / Shift+Enter (file find)", "Next / previous match"),
    ("Search", "Escape (file find)", "Close find"),
    ("Table navigation", "Tab / Shift+Tab", "Next / previous cell; Tab at the end adds a row"),
    ("Table navigation", "Enter", "Move to the next row; add a row at the end"),
    ("Table navigation", "Arrows", "Move within a cell; cross cells at their boundaries"),
    ("Table navigation", "Home / End", "Move to cell start / end"),
    ("Table navigation", "Escape", "Leave table navigation"),
];

impl Notes {
    pub(super) fn open_help(&mut self, shortcuts: bool, window: &mut Window, cx: &mut Context<Self>) {
        if window.has_active_dialog(cx) { return; }
        let previous = self.menu_previous_focus.take().or_else(|| window.focused(cx));
        self.menu = None;
        window.open_dialog(cx, move |dialog, window, _cx| {
            let mut body = div().flex().flex_col().gap_2();
            if shortcuts {
                let app = app_shortcuts();
                for group in ["Application", "Editor", "Search", "Notebook tree", "Table navigation"] {
                    body = body.child(div().mt_2().font_weight(FontWeight::BOLD).child(group));
                    for shortcut in app.iter().filter(|s| s.group == group) {
                        body = body.child(shortcut_row(shortcut.key, shortcut.description));
                    }
                    for (_, key, description) in CONTEXT_SHORTCUTS.iter().filter(|s| s.0 == group) {
                        body = body.child(shortcut_row(key, description));
                    }
                }
            } else {
                body = body.child("GPUI Note")
                    .child(format!("Version {}", env!("CARGO_PKG_VERSION")))
                    .child("A Markdown notebook built with GPUI. More information coming soon.");
            }
            let restore = previous.clone();
            let close_focus = previous.clone();
            dialog.title(if shortcuts { "Keyboard shortcuts" } else { "About GPUI Note" })
                .w(px(700.).min(window.viewport_size().width - px(32.)))
                .max_h((window.viewport_size().height - px(80.)).max(px(120.)))
                .child(div().id("help-content").max_h((window.viewport_size().height - px(220.)).max(px(80.))).overflow_y_scrollbar().child(body))
                .child(Button::new("close-help").label("Close").on_click(move |_, window, cx| {
                    window.close_dialog(cx);
                    if let Some(focus) = &close_focus { focus.focus(window); }
                }))
                .on_close(move |_, window, _| {
                    if let Some(focus) = &restore { focus.focus(window); }
                })
        });
        cx.notify();
    }
}

fn shortcut_row(key: &str, description: &str) -> impl IntoElement {
    div().flex().flex_wrap().gap_2()
        .child(div().w(px(250.)).child(key.to_owned()))
        .child(div().flex_1().child(description.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::app_shortcuts;
    #[test]
    fn app_bindings_have_unique_documented_shortcuts() {
        let shortcuts = app_shortcuts();
        let mut keys = std::collections::BTreeSet::new();
        for shortcut in shortcuts {
            assert!(!shortcut.description.is_empty());
            assert!(keys.insert(shortcut.key));
        }
        assert!(keys.contains("alt-h"));
        assert!(keys.contains("ctrl-shift-f"));
    }
}
