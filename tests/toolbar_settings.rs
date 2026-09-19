use mithril::notebook::{Notebook, Settings, ToolbarItem, ToolbarSettings, save_book};

#[test]
fn toolbar_defaults_preserve_legacy_and_partially_configured_notebooks() {
    assert!(ToolbarSettings::default().sidebar);
    let legacy: Settings =
        serde_json::from_str(r#"{"version":1,"last_file":null,"expanded":[]}"#).unwrap();
    assert_eq!(legacy.toolbar, ToolbarSettings::default());
    assert!(legacy.toolbar.visible);
    let partial: Settings = serde_json::from_str(
        r#"{"version":1,"last_file":null,"expanded":[],"toolbar":{"search":false}}"#,
    )
    .unwrap();
    assert_eq!(
        partial.toolbar,
        ToolbarSettings {
            search: false,
            ..ToolbarSettings::default()
        }
    );
    assert!(serde_json::from_str::<ToolbarSettings>(r#"{"search":"false"}"#).is_err());
    for enabled in [false, true] {
        let settings: ToolbarSettings =
            serde_json::from_str(&format!(r#"{{"sidebar":{enabled}}}"#)).unwrap();
        assert_eq!(settings.sidebar, enabled);
        let saved = serde_json::to_string(&settings).unwrap();
        assert_eq!(
            serde_json::from_str::<ToolbarSettings>(&saved)
                .unwrap()
                .sidebar,
            enabled
        );
    }
}

#[test]
fn toolbar_combinations_round_trip_and_persist_without_changing_other_preferences() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("notebook");
    let mut book = Notebook::open(&root, true).unwrap();
    book.settings.date_format = "MM/DD/YYYY".into();
    for mask in (0..16).chain((4..18).map(|i| 1 << i)).chain([0x3ffff]) {
        let mut toolbar = ToolbarSettings {
            visible: mask % 2 == 0,
            ..ToolbarSettings::default()
        };
        for (index, (item, _)) in ToolbarItem::OPTIONS.into_iter().enumerate() {
            toolbar.set_enabled(item, mask & (1 << index) != 0);
        }
        assert_eq!(toolbar.any_enabled(), mask != 0);
        book.settings.toolbar = toolbar;
        save_book(&book).unwrap();
        let loaded = Notebook::open(&root, false).unwrap();
        assert_eq!(loaded.settings.toolbar, toolbar);
        assert_eq!(loaded.settings.date_format, "MM/DD/YYYY");
        for (index, (item, _)) in ToolbarItem::OPTIONS.into_iter().enumerate() {
            assert_eq!(
                loaded.settings.toolbar.enabled(item),
                mask & (1 << index) != 0
            );
        }
    }
}

#[test]
fn legacy_toolbar_orders_are_ignored_and_omitted_on_save() {
    let mut reversed = ToolbarItem::DEFAULT_ORDER;
    reversed.reverse();
    for order in [
        serde_json::json!(reversed),
        serde_json::json!(null),
        serde_json::json!([]),
        serde_json::json!(false),
        serde_json::json!(["sidebar", "sidebar", "search", "sort"]),
        serde_json::json!(["sort", "unknown", "bold", 4, "sort"]),
        serde_json::json!(["sort", "search", "explorer", "sidebar"]),
    ] {
        let settings: ToolbarSettings = serde_json::from_value(serde_json::json!({
            "order": order, "search": false, "icon_size": "large", "alignment": "right"
        }))
        .unwrap();
        assert_eq!(
            settings,
            ToolbarSettings {
                search: false,
                icon_size: mithril::notebook::ToolbarIconSize::Large,
                alignment: mithril::notebook::ToolbarAlignment::Right,
                ..ToolbarSettings::default()
            }
        );
        let json = serde_json::to_value(settings).unwrap();
        assert!(json.get("order").is_none());
        assert_eq!(
            serde_json::from_value::<ToolbarSettings>(json).unwrap(),
            settings
        );
    }

    let directory = tempfile::tempdir().unwrap();
    let book = Notebook::open(&directory.path().join("book"), true).unwrap();
    let path = book.root.join(".mithril/settings.json");
    let mut legacy = serde_json::to_value(&book.settings).unwrap();
    legacy["toolbar"]["order"] = serde_json::json!(reversed);
    legacy["toolbar"]["bold"] = serde_json::json!(false);
    let bytes = serde_json::to_vec_pretty(&legacy).unwrap();
    std::fs::write(&path, &bytes).unwrap();
    let loaded = Notebook::open(&book.root, false).unwrap();
    assert!(!loaded.settings.toolbar.bold);
    assert_eq!(
        std::fs::read(&path).unwrap(),
        bytes,
        "loading must not force a migration write"
    );
    save_book(&loaded).unwrap();
    let saved: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert!(saved["toolbar"].get("order").is_none());
    assert_eq!(
        Notebook::open(&book.root, false).unwrap().settings.toolbar,
        loaded.settings.toolbar
    );
}

#[test]
fn toolbar_icon_sizes_default_normalize_and_persist() {
    use mithril::notebook::ToolbarIconSize;
    assert_eq!(
        ToolbarSettings::default().icon_size,
        ToolbarIconSize::Medium
    );
    assert_eq!(
        serde_json::from_str::<ToolbarSettings>("{}")
            .unwrap()
            .icon_size,
        ToolbarIconSize::Medium
    );
    for invalid in [
        serde_json::json!(null),
        serde_json::json!("huge"),
        serde_json::json!(20),
        serde_json::json!([]),
        serde_json::json!(false),
    ] {
        let settings: ToolbarSettings =
            serde_json::from_value(serde_json::json!({ "icon_size": invalid, "search": false }))
                .unwrap();
        assert_eq!(settings.icon_size, ToolbarIconSize::Medium);
        assert!(!settings.search);
    }
    let directory = tempfile::tempdir().unwrap();
    let mut book = Notebook::open(&directory.path().join("book"), true).unwrap();
    for (size, label) in ToolbarIconSize::OPTIONS {
        book.settings.toolbar.icon_size = size;
        save_book(&book).unwrap();
        let saved = Notebook::open(&book.root, false).unwrap();
        assert_eq!(saved.settings.toolbar.icon_size, size);
        assert_eq!(
            serde_json::to_value(saved.settings.toolbar).unwrap()["icon_size"],
            label.to_lowercase()
        );
    }
}

#[test]
fn toolbar_alignments_default_normalize_and_persist() {
    use mithril::notebook::ToolbarAlignment;
    assert_eq!(ToolbarSettings::default().alignment, ToolbarAlignment::Left);
    assert_eq!(
        serde_json::from_str::<ToolbarSettings>("{}")
            .unwrap()
            .alignment,
        ToolbarAlignment::Left
    );
    for invalid in [
        serde_json::json!(null),
        serde_json::json!("justify"),
        serde_json::json!(42),
        serde_json::json!([]),
        serde_json::json!(false),
        serde_json::json!({}),
    ] {
        let settings: ToolbarSettings =
            serde_json::from_value(serde_json::json!({ "alignment": invalid, "search": false }))
                .unwrap();
        assert_eq!(settings.alignment, ToolbarAlignment::Left);
        assert!(!settings.search);
    }
    let directory = tempfile::tempdir().unwrap();
    let mut book = Notebook::open(&directory.path().join("book"), true).unwrap();
    book.settings.toolbar.search = false;
    for (alignment, label) in ToolbarAlignment::OPTIONS {
        book.settings.toolbar.alignment = alignment;
        save_book(&book).unwrap();
        let saved = Notebook::open(&book.root, false).unwrap();
        assert_eq!(saved.settings.toolbar, book.settings.toolbar);
        assert_eq!(
            serde_json::to_value(saved.settings.toolbar).unwrap()["alignment"],
            label.to_lowercase()
        );
    }
}

#[test]
fn legacy_toolbar_gains_terminal_without_changing_saved_choices() {
    use mithril::notebook::ToolbarRegion;
    let settings: ToolbarSettings = serde_json::from_str(
        r#"{"order":["italic","bold","forward","back"],"bold":false,"alignment":"right"}"#,
    )
    .unwrap();
    assert!(settings.terminal);
    assert!(!settings.bold);
    assert_eq!(
        settings.alignment,
        mithril::notebook::ToolbarAlignment::Right
    );
    assert_eq!(ToolbarItem::Terminal.region(), ToolbarRegion::ContentRight);
}

#[test]
fn top_bar_visibility_defaults_and_saved_preferences_are_independent() {
    let legacy: ToolbarSettings =
        serde_json::from_str(r#"{"visible":false,"terminal":false}"#).unwrap();
    assert!(!legacy.title_bar_sidebar);
    assert!(!legacy.title_bar_terminal);
    assert!(!legacy.terminal);
    let directory = tempfile::tempdir().unwrap();
    let mut book = Notebook::open(&directory.path().join("book"), true).unwrap();
    for sidebar in [false, true] {
        for terminal in [false, true] {
            book.settings.toolbar = ToolbarSettings {
                visible: false,
                title_bar_sidebar: sidebar,
                title_bar_terminal: terminal,
                terminal,
                ..ToolbarSettings::default()
            };
            save_book(&book).unwrap();
            assert_eq!(
                Notebook::open(&book.root, false).unwrap().settings.toolbar,
                book.settings.toolbar
            );
        }
    }
}
