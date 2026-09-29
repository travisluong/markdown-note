//! Official Lucide toolbar assets layered over the component's Lucide icon bundle.
use gpui::{AssetSource, SharedString};
use std::borrow::Cow;

pub struct Assets;
const TOOLBAR: &[(&str, &[u8])] = &[
    (
        "toolbar/sort.svg",
        include_bytes!("../assets/toolbar/sort.svg"),
    ),
    (
        "toolbar/bold.svg",
        include_bytes!("../assets/toolbar/bold.svg"),
    ),
    (
        "toolbar/italic.svg",
        include_bytes!("../assets/toolbar/italic.svg"),
    ),
    (
        "toolbar/strikethrough.svg",
        include_bytes!("../assets/toolbar/strikethrough.svg"),
    ),
    (
        "toolbar/heading.svg",
        include_bytes!("../assets/toolbar/heading.svg"),
    ),
    (
        "toolbar/inlinecode.svg",
        include_bytes!("../assets/toolbar/inlinecode.svg"),
    ),
    (
        "toolbar/codeblock.svg",
        include_bytes!("../assets/toolbar/codeblock.svg"),
    ),
    (
        "toolbar/link.svg",
        include_bytes!("../assets/toolbar/link.svg"),
    ),
    (
        "toolbar/blockquote.svg",
        include_bytes!("../assets/toolbar/blockquote.svg"),
    ),
    (
        "toolbar/orderedlist.svg",
        include_bytes!("../assets/toolbar/orderedlist.svg"),
    ),
    (
        "toolbar/unorderedlist.svg",
        include_bytes!("../assets/toolbar/unorderedlist.svg"),
    ),
    (
        "toolbar/tasklist.svg",
        include_bytes!("../assets/toolbar/tasklist.svg"),
    ),
];
const BRAND: (&str, &[u8]) = (
    "brand/markdown-note.png",
    include_bytes!("../assets/brand/markdown-note.png"),
);
impl AssetSource for Assets {
    fn load(&self, path: &str) -> gpui::Result<Option<Cow<'static, [u8]>>> {
        if path == BRAND.0 {
            return Ok(Some(Cow::Borrowed(BRAND.1)));
        }
        if let Some((_, bytes)) = TOOLBAR.iter().find(|(name, _)| *name == path) {
            return Ok(Some(Cow::Borrowed(bytes)));
        }
        gpui_component_assets::Assets.load(path)
    }
    fn list(&self, path: &str) -> gpui::Result<Vec<SharedString>> {
        let mut assets = gpui_component_assets::Assets.list(path)?;
        if BRAND.0.starts_with(path) {
            assets.push(BRAND.0.into());
        }
        assets.extend(
            TOOLBAR
                .iter()
                .filter(|(name, _)| name.starts_with(path))
                .map(|(name, _)| SharedString::from(*name)),
        );
        Ok(assets)
    }
}
