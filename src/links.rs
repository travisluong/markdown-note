use super::*;
use gpui_component::{dialog::DialogButtonProps, input::markdown_links};

#[derive(Debug, PartialEq)]
enum Destination {
    External(String),
    Note(PathBuf),
}

fn destination(raw: &str, current: &Path, root: Option<&Path>) -> Result<Destination, String> {
    if raw.is_empty() || raw.chars().any(char::is_control) {
        return Err("Enter a valid link destination.".into());
    }
    if let Ok(url) = url::Url::parse(raw) {
        return match url.scheme() {
            "http" | "https" if url.host_str().is_some() => {
                Ok(Destination::External(url.to_string()))
            }
            "mailto" if !url.path().is_empty() => Ok(Destination::External(url.to_string())),
            _ => Err(
                "Supported links use http, https, mailto, or a relative Markdown file path.".into(),
            ),
        };
    }
    if raw.starts_with(['/', '\\']) || raw.contains('\\') {
        return Err("Use a relative Markdown path with forward slashes.".into());
    }
    let base = url::Url::from_file_path(current)
        .map_err(|_| "The document path cannot resolve this link.")?;
    let url = base.join(raw).map_err(|_| "Invalid link destination.")?;
    if url.fragment().is_some() || url.query().is_some() {
        return Err("Heading anchors and file queries are not supported yet.".into());
    }
    let mut path = url.to_file_path().map_err(|_| "Invalid file link.")?;
    if !notebook::markdown(&path) {
        return Err("Local links must point to a .md or .markdown file.".into());
    }
    if let Some(root) = root.filter(|root| current.starts_with(root)) {
        // URL conversion removes Windows verbatim prefixes. Compare like forms,
        // then retain the canonical notebook root for the existing path resolver.
        let normal_root = url::Url::from_directory_path(root)
            .map_err(|_| "Invalid notebook path.")?
            .to_file_path()
            .map_err(|_| "Invalid notebook path.")?;
        let relative = path
            .strip_prefix(&normal_root)
            .map_err(|_| "This link points outside the current notebook.")?;
        if !notebook::safe_relative(relative) {
            return Err("Invalid relative note path.".into());
        }
        path = root.join(relative);
    }
    Ok(Destination::Note(path))
}

fn link_text(label: &str, target: &str, title: Option<&str>) -> Result<String, String> {
    if label.is_empty() || target.is_empty() || target.chars().any(char::is_control) {
        return Err("Enter a label and destination.".into());
    }
    let escape = |s: &str| {
        s.replace('\\', "\\\\")
            .replace('<', "\\<")
            .replace('>', "\\>")
            .replace('"', "\\\"")
    };
    let title = title
        .map(|s| format!(" \"{}\"", escape(s)))
        .unwrap_or_default();
    let value = format!("[{label}](<{}>{title})", escape(target));
    let links = markdown_links::parse(&value);
    if links.len() != 1 || links[0].source != (0..value.len()) || links[0].destination != target {
        return Err(
            "The label contains invalid Markdown. Escape literal brackets with a backslash.".into(),
        );
    }
    Ok(value)
}

impl Notes {
    pub(super) fn open_link(&mut self, raw: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.is_busy() || window.has_active_dialog(cx) {
            return;
        }
        let Some(doc) = &self.document else {
            return;
        };
        match destination(
            raw,
            &doc.path,
            self.notebooks.active.as_ref().map(|b| b.root.as_path()),
        ) {
            Ok(Destination::External(url)) => cx.open_url(&url),
            Ok(Destination::Note(path)) => {
                // Check every existing component, including links outside notebooks.
                let check = path
                    .ancestors()
                    .find_map(|p| fs::symlink_metadata(p).ok().filter(notebook::is_link));
                if check.is_some() {
                    self.error(
                        "Could not open link",
                        "Links through filesystem shortcuts are not opened.",
                        window,
                        cx,
                    );
                } else {
                    self.request(Next::File(path), window, cx);
                }
            }
            Err(error) => self.error("Could not open link", error, window, cx),
        }
    }

    pub(super) fn edit_link(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.can_edit_document() || window.has_active_dialog(cx) {
            return;
        }
        let snapshot = self.editor.read(cx).value().to_string();
        let selection = self.editor.read(cx).source_selection();
        let links = markdown_links::parse(&snapshot);
        let existing = links.iter().find(|link| {
            link.source.contains(&selection.start) && selection.end <= link.source.end
        });
        let range = existing.map_or(selection.clone(), |link| link.source.clone());
        let label_value =
            existing.map_or(&snapshot[selection], |link| &snapshot[link.label.clone()]);
        let target_value = existing.map_or("https://", |link| link.destination.as_str());
        let title = existing.and_then(|link| link.title.clone());
        let label = cx.new(|cx| InputState::new(window, cx).default_value(label_value.to_owned()));
        let target =
            cx.new(|cx| InputState::new(window, cx).default_value(target_value.to_owned()));
        let focus = if label_value.is_empty() {
            label.clone()
        } else {
            target.clone()
        };
        let identity = self.saves.identity;
        let owner = cx.weak_entity();
        let error = std::rc::Rc::new(std::cell::RefCell::new(String::new()));
        window.open_dialog(cx, move |dialog, _, _| {
            let owner = owner.clone();
            let label = label.clone();
            let target = target.clone();
            let snapshot = snapshot.clone();
            let range = range.clone();
            let title = title.clone();
            let error = error.clone();
            let notice = error.borrow().clone();
            let close_owner = owner.clone();
            dialog
                .title("Insert / Edit Link")
                .confirm()
                .button_props(DialogButtonProps::default().ok_text("Apply"))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child("Label (Markdown)")
                        .child(Input::new(&label))
                        .child("Destination")
                        .child(Input::new(&target))
                        .when(!notice.is_empty(), |v| v.child(notice)),
                )
                .on_close(move |_, window, cx| {
                    let _ = close_owner.update(cx, |this, cx| {
                        this.editor.update(cx, |e, cx| e.focus(window, cx))
                    });
                })
                .on_ok(move |_, window, cx| {
                    owner
                        .update(cx, |this, cx| {
                            let result = link_text(
                                &label.read(cx).value(),
                                &target.read(cx).value(),
                                title.as_deref(),
                            );
                            if this.saves.identity != identity
                                || this.editor.read(cx).value().as_ref() != snapshot
                                || !this.can_edit_document()
                            {
                                *error.borrow_mut() =
                                    "The document changed. Close this dialog and try again.".into();
                                cx.notify();
                                return false;
                            }
                            match result {
                                Ok(text) => {
                                    this.editor.update(cx, |editor, cx| {
                                        editor.select_source_range(range.clone(), window, cx);
                                        editor.replace_selection_isolated(&text, window, cx);
                                    });
                                    true
                                }
                                Err(message) => {
                                    *error.borrow_mut() = message;
                                    cx.notify();
                                    false
                                }
                            }
                        })
                        .unwrap_or(true)
                })
        });
        focus.update(cx, |input, cx| input.focus(window, cx));
    }
}

#[cfg(test)]
mod tests {
    use super::{Destination, destination, link_text, markdown_links};
    #[test]
    fn routes_urls_and_relative_notes() {
        let root = std::env::temp_dir().join("link-notebook");
        let current = root.join("folder/current.md");
        assert_eq!(
            destination("../other%20note.md", &current, Some(&root)),
            Ok(Destination::Note(root.join("other note.md")))
        );
        for target in [
            "../../outside.md",
            "javascript:alert(1)",
            "file:///C:/secret.md",
            "//example.com/a.md",
            "other.exe",
            "other.md#heading",
            "#heading",
        ] {
            assert!(
                destination(target, &current, Some(&root)).is_err(),
                "{target}"
            );
        }
        for target in ["https://example.com/a", "mailto:hello@example.com"] {
            assert!(matches!(
                destination(target, &current, None),
                Ok(Destination::External(_))
            ));
        }
    }
    #[test]
    fn serialization_preserves_markdown_and_titles() {
        let text = link_text("**你好**", "../a b(c).md", Some("a \"title\"")).unwrap();
        let parsed = markdown_links::parse(&text);
        assert_eq!(&text[parsed[0].label.clone()], "**你好**");
        assert_eq!(parsed[0].title.as_deref(), Some("a \"title\""));
        assert!(link_text("]oops[", "https://example.com", None).is_err());
    }
}

#[cfg(all(test, windows))]
mod windows_path_tests {
    use super::{Destination, destination};
    use std::path::Path;
    #[test]
    fn canonical_drive_and_unc_notebooks_keep_their_root() {
        for root in [
            Path::new(r"\\?\C:\notes"),
            Path::new(r"\\?\UNC\server\share\notes"),
        ] {
            let current = root.join("folder/current.md");
            assert_eq!(
                destination("../other.md", &current, Some(root)),
                Ok(Destination::Note(root.join("other.md")))
            );
            assert!(destination("../../outside.md", &current, Some(root)).is_err());
        }
    }
}
