use super::{FileSort, Settings, parse_hidden_extensions};

#[test]
fn legacy_order_is_ignored_and_selection_persists() {
    let mut json = serde_json::to_value(Settings::default()).unwrap();
    json.as_object_mut().unwrap().remove("file_sort");
    json["entry_order"] = serde_json::json!(["z.md", "a.md"]);
    let mut settings: Settings = serde_json::from_value(json).unwrap();
    assert_eq!(settings.file_sort, FileSort::NameAscending);
    assert!(
        !serde_json::to_string(&settings)
            .unwrap()
            .contains("entry_order")
    );
    for (sort, _) in FileSort::OPTIONS {
        settings.file_sort = sort;
        let encoded = serde_json::to_string(&settings).unwrap();
        assert_eq!(
            serde_json::from_str::<Settings>(&encoded)
                .unwrap()
                .file_sort,
            sort
        );
    }
}

#[test]
fn extension_list_normalization_and_validation() {
    assert_eq!(
        parse_hidden_extensions(" .MD, txt, md, , .PNG ").unwrap(),
        vec!["md", "txt", "png"]
    );
    assert!(parse_hidden_extensions(" , , ").unwrap().is_empty());
    for invalid in [
        ".", "tar.gz", "m d", "md\ntxt", "a/b", "a\\b", "*", "?", "[md]",
    ] {
        assert!(parse_hidden_extensions(invalid).is_err(), "{invalid}");
    }
}

#[test]
fn centered_editor_defaults_round_trip_and_width_validation() {
    use super::Appearance;
    let legacy: Appearance = serde_json::from_str(r#"{"content_font_size":18}"#).unwrap();
    assert!(!legacy.centered_editor);
    assert_eq!(legacy.centered_editor_width, 700);
    let appearance = Appearance {
        centered_editor: true,
        centered_editor_width: 900,
        ..legacy
    };
    let encoded = serde_json::to_string(&appearance).unwrap();
    assert_eq!(
        serde_json::from_str::<Appearance>(&encoded).unwrap(),
        appearance
    );
    for value in ["", "199", "4001", "700px", "700.5", "-700", "65536"] {
        assert_eq!(Appearance::parse_editor_width(value), None, "{value}");
    }
    for (value, width) in [("200", 200), (" 700 ", 700), ("4000", 4000)] {
        assert_eq!(Appearance::parse_editor_width(value), Some(width));
    }
    for (value, width) in [(0, 200), (700, 700), (u16::MAX, 4000)] {
        assert_eq!(
            Appearance {
                centered_editor_width: value,
                ..Default::default()
            }
            .normalized()
            .centered_editor_width,
            width
        );
    }
}
