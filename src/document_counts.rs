//! Pure document statistics. Callers capture source and run this work off the UI thread.
use crate::notebook::FooterCountMode;
use html5ever::tendril::TendrilSink;
use markup5ever_rcdom::{Handle, NodeData, RcDom};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DocumentCounts {
    pub words: usize,
    pub characters: usize,
}

/// Count Unicode words and extended grapheme clusters, including whitespace.
/// Readable text only interprets Markdown documents; other files retain their source.
pub fn count(source: &str, markdown: bool, mode: FooterCountMode) -> DocumentCounts {
    let readable;
    let text = if markdown && mode == FooterCountMode::ReadableText {
        readable = readable_text(source);
        readable.as_str()
    } else {
        source
    };
    DocumentCounts {
        words: text.unicode_words().count(),
        characters: text.graphemes(true).count(),
    }
}

fn readable_text(source: &str) -> String {
    let options = markdown::Options {
        parse: markdown::ParseOptions::gfm(),
        compile: markdown::CompileOptions {
            // Only parse the result into text: no HTML is rendered or executed.
            allow_dangerous_html: true,
            gfm_tagfilter: false,
            ..Default::default()
        },
    };
    // Ordinary GFM accepts incomplete Markdown. Keep the source if a future
    // parser configuration ever introduces a fallible construct.
    let Ok(html) = markdown::to_html_with_options(source, &options) else {
        return source.to_owned();
    };
    let dom = html5ever::parse_document(RcDom::default(), Default::default()).one(html);
    let mut text = Text::default();
    walk(&dom.document, &mut text, false);
    text.value
}

/// Boundaries belong between visible blocks, never before or after the document.
#[derive(Default)]
struct Text {
    value: String,
    boundary: bool,
}

impl Text {
    fn boundary(&mut self) {
        self.boundary = !self.value.is_empty();
    }

    fn push(&mut self, value: &str) {
        if value.is_empty() {
            return;
        }
        if self.boundary && !self.value.ends_with('\n') && !value.starts_with('\n') {
            self.value.push('\n');
        }
        self.boundary = false;
        self.value.push_str(value);
    }
}

fn tag(node: &Handle) -> &str {
    match &node.data {
        NodeData::Element { name, .. } => name.local.as_ref(),
        _ => "",
    }
}

fn attribute(node: &Handle, name: &str) -> Option<String> {
    let NodeData::Element { attrs, .. } = &node.data else {
        return None;
    };
    attrs
        .borrow()
        .iter()
        .find(|attr| attr.name.local.as_ref() == name)
        .map(|attr| attr.value.to_string())
}

fn block(name: &str) -> bool {
    matches!(
        name,
        "address"
            | "article"
            | "aside"
            | "blockquote"
            | "dd"
            | "details"
            | "dialog"
            | "div"
            | "dl"
            | "dt"
            | "fieldset"
            | "figcaption"
            | "figure"
            | "footer"
            | "form"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "header"
            | "hr"
            | "li"
            | "main"
            | "nav"
            | "ol"
            | "p"
            | "pre"
            | "section"
            | "summary"
            | "table"
            | "tbody"
            | "thead"
            | "tfoot"
            | "tr"
            | "ul"
    )
}

fn skipped(node: &Handle) -> bool {
    matches!(
        tag(node),
        "head" | "script" | "style" | "template" | "input"
    ) || attribute(node, "id").as_deref() == Some("footnote-label")
        || attribute(node, "data-footnote-ref").is_some()
        || attribute(node, "data-footnote-backref").is_some()
}

fn inline_neighbor(node: &Handle) -> bool {
    match &node.data {
        NodeData::Text { contents } => !contents.borrow().trim().is_empty(),
        NodeData::Element { .. } => !block(tag(node)) && !skipped(node),
        _ => false,
    }
}

fn walk(node: &Handle, text: &mut Text, preformatted: bool) {
    let name = tag(node);
    if skipped(node) {
        return;
    }
    match &node.data {
        NodeData::Text { contents } => text.push(contents.borrow().as_ref()),
        NodeData::Document | NodeData::Element { .. } => {
            if name == "img" {
                if let Some(alt) = attribute(node, "alt") {
                    text.push(&alt);
                }
                return;
            }
            if name == "br" {
                text.push("\n");
                return;
            }
            let is_block = block(name);
            if is_block {
                text.boundary();
            }
            if name == "tr" {
                let children = node.children.borrow();
                for (index, cell) in children
                    .iter()
                    .filter(|child| matches!(tag(child), "td" | "th"))
                    .enumerate()
                {
                    if index > 0 {
                        text.push("\t");
                    }
                    let mut cell_text = Text::default();
                    walk(cell, &mut cell_text, preformatted);
                    text.push(&cell_text.value);
                }
            } else {
                walk_children(node, text, preformatted || name == "pre");
            }
            if is_block {
                text.boundary();
            }
        }
        NodeData::Comment { .. }
        | NodeData::Doctype { .. }
        | NodeData::ProcessingInstruction { .. } => {}
    }
}

fn walk_children(node: &Handle, text: &mut Text, preformatted: bool) {
    let children = node.children.borrow();
    let has_blocks = children.iter().any(|child| block(tag(child)));
    let structural = matches!(
        tag(node),
        "html" | "body" | "ul" | "ol" | "table" | "thead" | "tbody" | "tfoot"
    );
    let mut remove_space = false;
    let mut remove_newline = false;
    for (index, child) in children.iter().enumerate() {
        if let NodeData::Text { contents } = &child.data {
            let contents = contents.borrow();
            let mut value: &str = contents.as_ref();
            if remove_space {
                value = value.strip_prefix(' ').unwrap_or(value);
            }
            if remove_newline {
                value = value.strip_prefix('\n').unwrap_or(value);
            }
            // Containers may mix blocks with inline runs. A space between
            // inline siblings remains authored text even in such a container.
            let between_inline = index > 0
                && index + 1 < children.len()
                && inline_neighbor(&children[index - 1])
                && inline_neighbor(&children[index + 1]);
            if preformatted
                || !((has_blocks || structural) && value.trim().is_empty() && !between_inline)
            {
                text.push(value);
            }
        } else {
            // markdown-rs inserts one space before each generated backlink.
            // Remove that adornment without trimming authored text whitespace.
            if attribute(child, "data-footnote-backref").is_some() && text.value.ends_with(' ') {
                text.value.pop();
            }
            walk(child, text, preformatted);
        }
        remove_space =
            tag(child) == "input" && attribute(child, "type").as_deref() == Some("checkbox");
        remove_newline = tag(child) == "br";
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_counts_unicode_words_and_visible_characters_including_whitespace() {
        for (source, words, characters) in [
            ("", 0, 0),
            (" \t\n", 0, 3),
            ("one  two\n", 2, 9),
            ("cafe\u{301}", 1, 4),
            ("👩‍👩‍👧‍👦 🇺🇸", 0, 3),
            ("hello—world!", 2, 12),
            ("a\r\nb", 2, 3),
            ("can't", 1, 5),
        ] {
            assert_eq!(
                count(source, true, FooterCountMode::Source),
                DocumentCounts { words, characters },
                "{source:?}"
            );
        }
    }

    #[test]
    fn source_and_non_markdown_readable_modes_preserve_the_exact_text() {
        let source = "# heading\n\n**bold** [link](https://example.com)";
        assert_eq!(
            count(source, false, FooterCountMode::ReadableText),
            count(source, true, FooterCountMode::Source)
        );
        assert_ne!(
            count(source, true, FooterCountMode::ReadableText),
            count(source, true, FooterCountMode::Source)
        );
    }

    #[test]
    fn readable_text_removes_markup_without_splitting_inline_words() {
        assert_eq!(
            readable_text(
                "# Heading\n\nfoo**bar** and ~~old~~ [label](https://example.com \"title\")\n\n![image alt](image.png)"
            ),
            "Heading\nfoobar and old label\nimage alt"
        );
        assert_eq!(readable_text("a &amp; b \\* c `x  y`"), "a & b * c x  y");
        assert_eq!(
            readable_text(
                "[label][id] ![alt][image]\n\n[id]: https://example.com\n[image]: photo.png"
            ),
            "label alt"
        );
    }

    #[test]
    fn readable_text_has_one_boundary_between_blocks_and_tabs_between_cells() {
        assert_eq!(
            readable_text("> # A\n>\n> one\n>\n> two\n\n- first\n- second\n\n---\n\nlast"),
            "A\none\ntwo\nfirst\nsecond\nlast"
        );
        assert_eq!(
            readable_text("| A | B |\n| - | - |\n| C | D |"),
            "A\tB\nC\tD"
        );
    }

    #[test]
    fn readable_text_preserves_text_and_code_whitespace_and_hard_breaks() {
        assert_eq!(
            readable_text("one  two\nthree  \nfour\\\nfive"),
            "one  two\nthree\nfour\nfive"
        );
        assert_eq!(
            readable_text("```rust\n  a\t b\n\n c\n```"),
            "  a\t b\n\n c\n"
        );
        assert_eq!(readable_text("- [ ] task\n- [x] done"), "task\ndone");
    }

    #[test]
    fn readable_html_keeps_text_and_alt_but_excludes_hidden_markup() {
        let source = "<div>Hello <strong>world</strong><br>again <img src=\"x\" alt=\"picture\"></div>\n\n<script>secret()</script>\n\n<style>hidden {}</style>\n\n<!-- comment -->\n\n<template>inert</template>";
        assert_eq!(readable_text(source), "Hello world\nagain picture");
        assert_eq!(
            readable_text("start <script>hidden</script> end"),
            "start  end"
        );
        assert_eq!(
            readable_text("<div><span>a</span> <span>b</span><p>c</p></div>"),
            "a b\nc"
        );
    }

    #[test]
    fn readable_footnotes_include_bodies_without_generated_labels_or_padding() {
        assert_eq!(
            readable_text("hello[^a] again[^a]\n\n[^a]: footnote body\n\n[^unused]: omitted"),
            "hello again\nfootnote body"
        );
    }

    #[test]
    fn incomplete_markdown_is_countable_and_readable_graphemes_include_spaces() {
        assert_eq!(readable_text("[unfinished **bold"), "[unfinished **bold");
        assert_eq!(
            count("**cafe\u{301}** 👩‍👩‍👧‍👦", true, FooterCountMode::ReadableText),
            DocumentCounts {
                words: 1,
                characters: 6
            }
        );
    }
}
