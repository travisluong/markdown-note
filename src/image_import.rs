//! Filesystem image imports are prepared on the background executor.
use markdown::{ParseOptions, mdast::Node};
use std::{
    fs,
    io::{self, Write},
    path::{Path, PathBuf},
};

pub(crate) fn supported(path: &Path) -> bool {
    path.extension().and_then(|s| s.to_str()).is_some_and(|s| {
        ["png", "jpg", "jpeg", "gif", "webp", "bmp"]
            .iter()
            .any(|ext| s.eq_ignore_ascii_case(ext))
    })
}

/// Newly created files are owned until the editor accepts the insertion.
/// Dropping a failed or canceled batch rolls back its files, never reused files.
#[derive(Default)]
pub(crate) struct Import {
    created: Vec<PathBuf>,
    pub paths: Vec<PathBuf>,
}
impl Drop for Import {
    fn drop(&mut self) {
        for path in &self.created {
            let _ = fs::remove_file(path);
        }
    }
}
impl Import {
    pub fn commit(&mut self) {
        self.created.clear();
    }
}

pub(crate) fn prepare(root: &Path, sources: &[PathBuf]) -> io::Result<Import> {
    let root = fs::canonicalize(root)?;
    if sources.is_empty() {
        return Err(io::Error::other("Drop one or more image files."));
    }
    let mut validated = Vec::new();
    // Validate the complete batch before creating any notebook files.
    for source in sources {
        let metadata = fs::symlink_metadata(source)?;
        if !metadata.is_file() || crate::notebook::is_link(&metadata) || !supported(source) {
            return Err(io::Error::other(
                "Drop only PNG, JPEG, GIF, WebP, or BMP image files.",
            ));
        }
        let source = fs::canonicalize(source)?;
        let bytes = fs::read(&source)?;
        let format = image::guess_format(&bytes).map_err(io::Error::other)?;
        if !matches!(
            format,
            image::ImageFormat::Png
                | image::ImageFormat::Jpeg
                | image::ImageFormat::Gif
                | image::ImageFormat::WebP
                | image::ImageFormat::Bmp
        ) {
            return Err(io::Error::other("Unsupported image format."));
        }
        let mut reader = image::ImageReader::new(io::Cursor::new(&bytes));
        reader.set_format(format);
        reader
            .decode()
            .map_err(|e| io::Error::other(format!("Could not decode {}: {e}", source.display())))?;
        validated.push((source, bytes));
    }
    let mut imported = Import::default();
    for (source, bytes) in validated {
        if source.parent() == Some(root.as_path()) {
            imported.paths.push(source);
            continue;
        }
        let filename = source
            .file_name()
            .ok_or_else(|| io::Error::other("Image has no filename."))?;
        let stem = source.file_stem().unwrap().to_string_lossy();
        let extension = source.extension().unwrap().to_string_lossy();
        let mut temp = tempfile::NamedTempFile::new_in(&root)?;
        temp.write_all(&bytes)?;
        temp.as_file().sync_all()?;
        let mut suffix = 0u64;
        loop {
            let target = root.join(if suffix == 0 {
                filename.to_os_string()
            } else {
                format!("{stem}-{suffix}.{extension}").into()
            });
            match temp.persist_noclobber(&target) {
                Ok(_) => {
                    imported.created.push(target.clone());
                    imported.paths.push(target);
                    break;
                }
                Err(error) if error.error.kind() == io::ErrorKind::AlreadyExists => {
                    temp = error.file;
                    suffix += 1;
                }
                Err(error) => return Err(error.error),
            }
        }
    }
    Ok(imported)
}

pub(crate) fn markdown(note: &Path, paths: &[PathBuf]) -> io::Result<String> {
    let base =
        url::Url::from_file_path(note).map_err(|_| io::Error::other("Invalid note path."))?;
    let mut paragraphs = Vec::new();
    for path in paths {
        let target =
            url::Url::from_file_path(path).map_err(|_| io::Error::other("Invalid image path."))?;
        let relative = base
            .make_relative(&target)
            .ok_or_else(|| io::Error::other("Image must belong to this notebook."))?;
        let name = path.file_name().unwrap().to_string_lossy();
        if name.chars().any(char::is_control) {
            return Err(io::Error::other(
                "Image filename contains control characters.",
            ));
        }
        let mut alt = String::new();
        for ch in name.chars() {
            if matches!(ch, '\\' | '[' | ']' | '<' | '>' | '&' | '*' | '_' | '`') {
                alt.push('\\');
            }
            alt.push(ch);
        }
        paragraphs.push(format!("![{alt}](<{relative}>)"));
    }
    Ok(paragraphs.join("\n\n"))
}

/// Protect structural blocks and inline syntax when choosing the insertion point.
pub(crate) fn insertion(source: &str, offset: usize, images: &str) -> io::Result<(usize, String)> {
    if !source.is_char_boundary(offset) {
        return Err(io::Error::other("Invalid drop position."));
    }
    let mut offset = offset;
    let root = markdown::to_mdast(source, &ParseOptions::gfm())
        .map_err(|e| io::Error::other(e.to_string()))?;
    if let Node::Root(root) = root {
        for node in root.children {
            if let Some(pos) = node.position() {
                if pos.start.offset <= offset && offset <= pos.end.offset {
                    if let Node::Paragraph(paragraph) = &node {
                        // Keep ordinary text offsets, but move protected spans
                        // after the paragraph. Splitting at a span's end could
                        // reinterpret its remaining text as a new Markdown block.
                        for child in &paragraph.children {
                            if !matches!(child, Node::Text(_))
                                && let Some(position) = child.position()
                                && position.start.offset < offset
                                && offset < position.end.offset
                            {
                                offset = pos.end.offset;
                                break;
                            }
                        }
                    } else {
                        // A separate paragraph cannot safely split a structural block.
                        offset = pos.end.offset;
                        if source[offset..].starts_with('\n') {
                            offset += 1;
                        }
                        if matches!(node, Node::Code(_)) && pos.end.offset == source.len() {
                            // An unclosed fence would swallow the image; refuse instead of changing code.
                            let raw = &source[pos.start.offset..pos.end.offset];
                            let first = raw.lines().next().unwrap_or("").trim_start();
                            if first.starts_with("```") || first.starts_with("~~~") {
                                let marker = first.as_bytes()[0] as char;
                                let count = first.chars().take_while(|c| *c == marker).count();
                                let closed = raw.lines().skip(1).any(|line| {
                                    let line = line.trim();
                                    line.len() >= count && line.chars().all(|c| c == marker)
                                });
                                if !closed {
                                    return Err(io::Error::other(
                                        "Close the code fence before dropping an image here.",
                                    ));
                                }
                            }
                        }
                    }
                    break;
                }
            }
        }
    }
    let before = &source[..offset];
    let after = &source[offset..];
    let prefix = if before.is_empty() || before.ends_with("\n\n") {
        ""
    } else if before.ends_with('\n') {
        "\n"
    } else {
        "\n\n"
    };
    let suffix = if after.starts_with("\n\n") {
        ""
    } else if after.starts_with('\n') {
        "\n"
    } else {
        "\n\n"
    };
    Ok((offset, format!("{prefix}{images}{suffix}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn png(path: &Path) {
        image::RgbaImage::from_pixel(2, 3, image::Rgba([255, 0, 0, 255]))
            .save(path)
            .unwrap();
    }
    #[test]
    fn import_collisions_reuse_subfolders_and_rollback() {
        let fixture = tempfile::tempdir().unwrap();
        let root = fixture.path().join("notebook");
        let outside = fixture.path().join("outside");
        fs::create_dir(&root).unwrap();
        fs::create_dir(&outside).unwrap();
        let image = outside.join("photo.png");
        png(&image);
        let existing = root.join("photo.png");
        png(&existing);
        let existing_bytes = fs::read(&existing).unwrap();
        let batch = prepare(&root, &[image.clone(), existing.clone(), image.clone()]).unwrap();
        assert_eq!(
            batch
                .paths
                .iter()
                .map(|p| p.file_name().unwrap().to_str().unwrap())
                .collect::<Vec<_>>(),
            ["photo-1.png", "photo.png", "photo-2.png"]
        );
        assert_eq!(fs::read(&existing).unwrap(), existing_bytes);
        drop(batch);
        assert!(!root.join("photo-1.png").exists());
        assert!(!root.join("photo-2.png").exists());
        assert!(existing.exists());
        let folder = root.join("nested");
        fs::create_dir(&folder).unwrap();
        let nested = folder.join("inner.png");
        png(&nested);
        let mut batch = prepare(&root, &[nested.clone()]).unwrap();
        assert!(root.join("inner.png").exists());
        batch.commit();
        drop(batch);
        assert!(root.join("inner.png").exists());
        assert!(nested.exists());
    }
    #[test]
    fn invalid_batches_create_nothing() {
        let fixture = tempfile::tempdir().unwrap();
        let root = fixture.path().join("root");
        fs::create_dir(&root).unwrap();
        let good = fixture.path().join("good.png");
        png(&good);
        let bad = fixture.path().join("bad.png");
        fs::write(&bad, b"invalid").unwrap();
        assert!(prepare(&root, &[good.clone(), bad]).is_err());
        let mixed = fixture.path().join("file.txt");
        fs::write(&mixed, "text").unwrap();
        assert!(prepare(&root, &[good.clone(), mixed]).is_err());
        assert!(prepare(&root, &[good.clone(), fixture.path().to_path_buf()]).is_err());
        assert!(prepare(&root, &[]).is_err());
        assert_eq!(fs::read_dir(&root).unwrap().count(), 0);
        assert!(prepare(&fixture.path().join("missing"), &[good]).is_err());
    }
    #[test]
    fn escaped_unicode_paths_parse_and_resolve() {
        let fixture = tempfile::tempdir().unwrap();
        let note = fixture.path().join("notes").join("deep").join("note.md");
        let image = fixture.path().join("写真 [a] #100%.png");
        let text = markdown(&note, &[image.clone()]).unwrap();
        let parsed = gpui_component::markdown_images::paragraphs(&text);
        assert_eq!(parsed.len(), 1, "{text}");
        let url = parsed.values().next().unwrap();
        assert!(url.starts_with("../../"));
        assert!(url.contains("%23"));
        assert!(url.contains("%25"));
        match gpui_component::markdown_images::resource(url, Some(&note)).unwrap() {
            gpui::Resource::Path(path) => assert_eq!(path.as_ref(), image),
            _ => panic!("expected file"),
        }
    }

    fn without_positions(mut node: Node) -> Node {
        node.position_set(None);
        if let Some(children) = node.children_mut() {
            for child in children {
                *child = without_positions(child.clone());
            }
        }
        node
    }

    fn inline_syntax(source: &str) -> Vec<Node> {
        let Node::Root(root) = markdown::to_mdast(source, &ParseOptions::gfm()).unwrap() else {
            panic!("expected document root");
        };
        root.children
            .into_iter()
            .filter_map(|node| match node {
                Node::Paragraph(paragraph) => Some(paragraph.children),
                _ => None,
            })
            .flatten()
            .filter(|node| !matches!(node, Node::Text(_) | Node::Image(_)))
            .map(without_positions)
            .collect()
    }

    #[test]
    fn insertion_preserves_inline_syntax_and_nested_spans() {
        let images = "![a](a.png)";
        let prefix = "hello ";
        for span in [
            "**world**",
            "*world*",
            "~~world~~",
            "`world`",
            "``a ` b``",
            "[world](target.md)",
            "[**world**](target.md)",
            "**outer *inner* text**",
            "***world***",
            "<https://example.com>",
            "**世🙂界**",
        ] {
            let source = format!("{prefix}{span} tail");
            let expected = inline_syntax(&source);
            assert_eq!(expected.len(), 1, "{source}");
            // Exercise label/content and delimiter positions, including a link's
            // destination and nested spans, without bisecting UTF-8 code points.
            for (inside, _) in span.char_indices().skip(1) {
                let (offset, inserted) = insertion(&source, prefix.len() + inside, images).unwrap();
                assert_eq!(offset, source.len(), "{source} at {inside}");
                let result = format!("{}{}{}", &source[..offset], inserted, &source[offset..]);
                assert_eq!(inline_syntax(&result), expected, "{source} at {inside}");
                assert_eq!(gpui_component::markdown_images::paragraphs(&result).len(), 1);
            }
            for boundary in [prefix.len(), prefix.len() + span.len()] {
                let (offset, inserted) = insertion(&source, boundary, images).unwrap();
                assert_eq!(offset, boundary, "{source}");
                let result = format!("{}{}{}", &source[..offset], inserted, &source[offset..]);
                assert_eq!(inline_syntax(&result), expected, "{source}");
            }
        }
    }

    #[test]
    fn insertion_preserves_paragraph_suffixes_that_look_like_blocks() {
        let images = "![a](a.png)";
        for suffix in [" - tail", " # tail", "    tail"] {
            let paragraph = format!("hello **world**{suffix}");
            let source = format!("before\n\n{paragraph}\n\nfollowing");
            let paragraph_start = source.find("hello").unwrap();
            let (offset, inserted) = insertion(&source, paragraph_start + 10, images).unwrap();
            assert_eq!(offset, paragraph_start + paragraph.len(), "{source}");
            let result = format!("{}{}{}", &source[..offset], inserted, &source[offset..]);

            // The complete AST must retain every original block and inline node;
            // only the standalone image paragraph is added after the target.
            let mut expected = markdown::to_mdast(&source, &ParseOptions::gfm()).unwrap();
            let mut image = markdown::to_mdast(images, &ParseOptions::gfm()).unwrap();
            expected
                .children_mut()
                .unwrap()
                .insert(2, image.children_mut().unwrap().remove(0));
            let actual = markdown::to_mdast(&result, &ParseOptions::gfm()).unwrap();
            assert_eq!(
                without_positions(actual),
                without_positions(expected),
                "{source}"
            );
        }
    }

    #[test]
    fn insertion_keeps_plain_text_offsets_and_rejects_invalid_boundaries() {
        let source = "hello 世🙂界";
        for offset in source
            .char_indices()
            .map(|(offset, _)| offset)
            .chain([source.len()])
        {
            assert_eq!(insertion(source, offset, "![a](a.png)").unwrap().0, offset);
        }
        for offset in [7, source.len() + 1, usize::MAX] {
            assert!(insertion(source, offset, "![a](a.png)").is_err());
        }
    }

    #[test]
    fn insertion_preserves_utf8_and_structural_blocks() {
        let images = "![a](a.png)";
        let source = "hello 世界";
        let (offset, inserted) = insertion(source, 6, images).unwrap();
        assert_eq!(offset, 6);
        let result = format!("{}{}{}", &source[..offset], inserted, &source[offset..]);
        assert_eq!(
            gpui_component::markdown_images::paragraphs(&result).len(),
            1
        );
        assert!(result.ends_with("世界"));
        assert!(insertion(source, 7, images).is_err());
        for (source, inside) in [
            ("```rust\ncode\n```\nnext", 9),
            ("| a | b |\n| --- | --- |\n| c | d |\n\nnext", 3),
        ] {
            let (offset, inserted) = insertion(source, inside, images).unwrap();
            assert_eq!(source[offset..].trim_start_matches('\n'), "next");
            let result = format!("{}{}{}", &source[..offset], inserted, &source[offset..]);
            assert_eq!(
                gpui_component::markdown_images::paragraphs(&result).len(),
                1
            );
        }
        assert!(insertion("```\ncode", 5, images).is_err());
        let (offset, inserted) = insertion("", 0, images).unwrap();
        assert_eq!(offset, 0);
        assert!(inserted.starts_with(images));
        assert_eq!(
            gpui_component::markdown_images::paragraphs(&inserted).len(),
            1
        );
    }
}
