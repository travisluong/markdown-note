use gpui_component::markdown_images;
#[test]
fn standalone_image_paragraphs_exclude_code_and_inline_images() {
    let source = "![hello](photo.png)\n\ntext ![inline](a.png)\n\n```md\n![code](b.png)\n```\n\n![nested](../a%20b.png)\n";
    let images = markdown_images::paragraphs(source);
    assert_eq!(images.len(), 2);
    assert_eq!(images.get(&0).map(String::as_str), Some("photo.png"));
    assert_eq!(
        images
            .get(&source.find("![nested]").unwrap())
            .map(String::as_str),
        Some("../a%20b.png")
    );
}
#[test]
fn resolve_relative_image_from_nested_note() {
    let directory = tempfile::tempdir().unwrap();
    let note = directory.path().join("notes").join("a.md");
    match markdown_images::resource("../a%20b%23.png", Some(&note)).unwrap() {
        gpui::Resource::Path(path) => assert_eq!(path.as_ref(), directory.path().join("a b#.png")),
        _ => panic!("local image must resolve to a filesystem path"),
    }
    assert!(markdown_images::resource("image.png", None).is_none());
    assert!(markdown_images::resource("javascript:bad", Some(&note)).is_none());
}

#[test]
fn ordinary_edits_rebase_images_without_hiding_them() {
    let original = "paragraph\n\n![a](a.png)\n";
    let mut images = markdown_images::paragraphs(original);
    markdown_images::rebase(&mut images, &(3..3), 5);
    let changed = format!("{}hello{}", &original[..3], &original[3..]);
    assert_eq!(images, markdown_images::paragraphs(&changed));
    markdown_images::rebase(&mut images, &(3..8), 0);
    assert_eq!(images, markdown_images::paragraphs(original));
}
