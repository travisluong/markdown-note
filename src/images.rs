use super::*;

pub(super) fn supported(path: &Path) -> bool {
    crate::image_import::supported(path)
}

pub(super) fn viewer(path: &Path) -> impl IntoElement {
    div()
        .size_full()
        .p_6()
        .flex()
        .items_center()
        .justify_center()
        .overflow_hidden()
        .child(
            img(path.to_path_buf())
                .max_w(relative(1.))
                .max_h(relative(1.))
                .object_fit(ObjectFit::ScaleDown)
                .with_loading(|| div().child("Loading image…").into_any_element())
                .with_fallback(|| {
                    div()
                        .child("Could not decode or load this image.")
                        .into_any_element()
                }),
        )
}

impl Notes {
    pub(super) fn open_image(
        &mut self,
        path: PathBuf,
        tree_focus: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let result = self
            .notebooks
            .active
            .as_ref()
            .ok_or_else(|| io::Error::other("Open a notebook first."))
            .and_then(|book| {
                path.strip_prefix(&book.root)
                    .map_err(io::Error::other)
                    .and_then(|relative| notebook::resolve(&book.root, relative))
            });
        match result {
            Ok(path) => {
                self.install_document(None, window, cx);
                self.image_path = Some(path);
                if tree_focus {
                    self.notebooks.tree_focus.focus(window);
                }
                self.refresh(window, cx);
            }
            Err(error) => self.error("Could not open image", error, window, cx),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[::core::prelude::v1::test]
    fn supported_extensions() {
        for name in ["a.PNG", "a.jpg", "a.jpeg", "a.gif", "a.webp", "a.bmp"] {
            assert!(supported(Path::new(name)));
        }
        for name in ["a.md", "a.svg", "a.png.exe", "png"] {
            assert!(!supported(Path::new(name)));
        }
    }
}

fn drop_allowed(mode: ViewMode, busy: bool, note: Option<&Path>, root: Option<&Path>) -> bool {
    !busy
        && mode != ViewMode::Reading
        && note
            .zip(root)
            .is_some_and(|(note, root)| note.starts_with(root))
}

impl Notes {
    fn can_drop_images(&self) -> bool {
        drop_allowed(
            self.mode,
            self.is_busy(),
            self.document.as_ref().map(|d| d.path.as_path()),
            self.notebooks
                .active
                .as_ref()
                .map(|book| book.root.as_path()),
        )
    }

    pub(super) fn drop_images(
        &mut self,
        paths: Vec<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.can_drop_images() || window.has_active_dialog(cx) {
            return;
        }
        let Some(offset) = self.editor.read(cx).drop_offset(window.mouse_position()) else {
            return;
        };
        let root = self.notebooks.active.as_ref().unwrap().root.clone();
        let note = self.document.as_ref().unwrap().path.clone();
        let expected_note = note.clone();
        let identity = self.saves.identity;
        let revision = self.dirty_revision;
        let snapshot = self.editor.read(cx).snapshot();
        let worker_snapshot = snapshot.clone();
        cx.spawn_in(window, async move |this, cx| {
            let result = cx.background_executor().spawn(async move {
                let source = worker_snapshot.to_string();
                // Check structural insertion rules before touching the filesystem.
                crate::image_import::insertion(&source, offset, "")?;
                let imported = crate::image_import::prepare(&root, &paths)?;
                let markdown = crate::image_import::markdown(&note, &imported.paths)?;
                let (offset, text) = crate::image_import::insertion(&source, offset, &markdown)?;
                Ok::<_, io::Error>((imported, offset, text))
            }).await;
            match result {
                Ok((mut imported, offset, text)) => {
                    let _ = this.update_in(cx, |this, window, cx| {
                        if !this.can_drop_images() || this.saves.identity != identity || this.dirty_revision != revision
                            || this.document.as_ref().is_none_or(|doc| doc.path != expected_note) {
                            this.notebooks.notice = Some("Image drop canceled because the document changed. Drop the image again.".into());
                            cx.notify();
                            return;
                        }
                        let inserted = this.editor.update(cx, |editor, cx| {
                            let inserted = editor.insert_at_snapshot(offset, &snapshot, &text, window, cx);
                            if inserted { editor.focus(window, cx); }
                            inserted
                        });
                        if inserted {
                            imported.commit();
                            this.notebooks.notice = None;
                            this.refresh_sorted_tree(window, cx);
                        } else {
                            this.notebooks.notice = Some("Image drop canceled because the document changed. Drop the image again.".into());
                            cx.notify();
                        }
                    });
                    // Roll back on the worker, including when the Notes entity was closed.
                    cx.background_executor().spawn(async move { drop(imported); }).await;
                }
                Err(error) => {
                    let _ = this.update_in(cx, |this, _, cx| {
                        this.notebooks.notice = Some(format!("Could not import images: {error}"));
                        cx.notify();
                    });
                }
            }
        }).detach();
    }
}

#[cfg(test)]
mod drop_tests {
    use super::*;
    #[::core::prelude::v1::test]
    fn drops_require_an_editable_notebook_note() {
        let root = Path::new("notebook");
        let note = Path::new("notebook/nested/a.md");
        for mode in [ViewMode::Source, ViewMode::LivePreview] {
            assert!(drop_allowed(mode, false, Some(note), Some(root)));
            assert!(!drop_allowed(mode, true, Some(note), Some(root)));
            assert!(!drop_allowed(mode, false, None, Some(root)));
            assert!(!drop_allowed(mode, false, Some(note), None));
            assert!(!drop_allowed(
                mode,
                false,
                Some(Path::new("outside/a.md")),
                Some(root)
            ));
        }
        assert!(!drop_allowed(
            ViewMode::Reading,
            false,
            Some(note),
            Some(root)
        ));
    }
}
