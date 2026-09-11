//! Shared Markdown image recognition and document-relative resource resolution.
use markdown::{ParseOptions, mdast::Node};
use std::{collections::BTreeMap, path::Path};

pub fn paragraphs(source: &str) -> BTreeMap<usize, String> {
    let mut images = BTreeMap::new();
    if !source.contains("![") {
        return images;
    }
    if let Ok(Node::Root(root)) = markdown::to_mdast(source, &ParseOptions::gfm()) {
        for child in root.children {
            if let Node::Paragraph(p) = child {
                if p.children.len() == 1 {
                    if let Node::Image(image) = &p.children[0] {
                        if let Some(pos) = &p.position {
                            if pos.start.line == pos.end.line {
                                images.insert(pos.start.offset, image.url.clone());
                            }
                        }
                    }
                }
            }
        }
    }
    images
}

pub fn resource(destination: &str, document: Option<&Path>) -> Option<gpui::Resource> {
    if let Ok(url) = url::Url::parse(destination) {
        return match url.scheme() {
            "http" | "https" => Some(gpui::Resource::Uri(destination.to_string().into())),
            "file" => url.to_file_path().ok().map(Into::into),
            _ => None,
        };
    }
    let base = url::Url::from_file_path(document?).ok()?;
    base.join(destination)
        .ok()?
        .to_file_path()
        .ok()
        .map(Into::into)
}

/// Translate an image index across an edit already known to preserve block syntax.
pub fn rebase(
    images: &mut BTreeMap<usize, String>,
    range: &std::ops::Range<usize>,
    inserted: usize,
) {
    let delta = inserted as isize - range.len() as isize;
    *images = std::mem::take(images)
        .into_iter()
        .filter_map(|(offset, url)| {
            if offset < range.start {
                Some((offset, url))
            } else if offset >= range.end {
                Some((offset.saturating_add_signed(delta), url))
            } else {
                None
            }
        })
        .collect();
}
