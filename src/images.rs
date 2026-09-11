use super::*;

pub(super) fn supported(path: &Path) -> bool {
    path.extension().and_then(|s| s.to_str()).is_some_and(|s|
        ["png", "jpg", "jpeg", "gif", "webp", "bmp"].iter().any(|ext| s.eq_ignore_ascii_case(ext)))
}

pub(super) fn viewer(path: &Path) -> impl IntoElement {
    div().size_full().p_6().flex().items_center().justify_center().overflow_hidden()
        .child(img(path.to_path_buf()).max_w(relative(1.)).max_h(relative(1.))
            .object_fit(ObjectFit::ScaleDown)
            .with_loading(|| div().child("Loading image…").into_any_element())
            .with_fallback(|| div().child("Could not decode or load this image.").into_any_element()))
}

impl Notes {
    pub(super) fn open_image(&mut self, path: PathBuf, tree_focus: bool, window: &mut Window, cx: &mut Context<Self>) {
        let result = self.notebooks.active.as_ref().ok_or_else(|| io::Error::other("Open a notebook first."))
            .and_then(|book| path.strip_prefix(&book.root).map_err(io::Error::other)
                .and_then(|relative| notebook::resolve(&book.root, relative)));
        match result {
            Ok(path) => {
                self.install_document(None, window, cx);
                self.image_path = Some(path);
                if tree_focus { self.notebooks.tree_focus.focus(window); }
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
        for name in ["a.PNG", "a.jpg", "a.jpeg", "a.gif", "a.webp", "a.bmp"] { assert!(supported(Path::new(name))); }
        for name in ["a.md", "a.svg", "a.png.exe", "png"] { assert!(!supported(Path::new(name))); }
    }
}

