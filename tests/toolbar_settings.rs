use mithril::notebook::{Notebook, Settings, ToolbarItem, ToolbarSettings, save_book};

#[test]
fn toolbar_defaults_preserve_legacy_and_partially_configured_notebooks() {
    assert!(!ToolbarSettings::default().sidebar);
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
    for mask in (0..16).chain((4..17).map(|i| 1 << i)).chain([0x1ffff]) {
        let mut toolbar = ToolbarSettings {
            visible: mask % 2 == 0,
            ..ToolbarSettings::default()
        };
        for (index, (item, _)) in ToolbarItem::OPTIONS.into_iter().enumerate() {
            toolbar.set_enabled(item, mask & (1 << index) != 0);
        }
        toolbar.order.rotate_left(mask % 4);
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
fn toolbar_order_migrates_partial_duplicate_unknown_and_legacy_entries() {
    use ToolbarItem::*;
    let mut previous_default = ToolbarItem::DEFAULT_ORDER;
    previous_default[..3].copy_from_slice(&[Back, Forward, Sidebar]);
    for (order, prefix) in [
        (
            serde_json::json!(previous_default),
            previous_default.to_vec(),
        ),
        (serde_json::json!(null), vec![]),
        (serde_json::json!([]), vec![]),
        (serde_json::json!(false), vec![]),
        (
            serde_json::json!(["sidebar", "sidebar", "search", "sort"]),
            vec![Sidebar, Search, Sort],
        ),
        (
            serde_json::json!(["sort", "unknown", "bold", 4, "sort"]),
            vec![Sort, Bold],
        ),
        (
            serde_json::json!(["sort", "search", "explorer", "sidebar"]),
            vec![Sort, Search, Explorer, Sidebar],
        ),
    ] {
        let settings: ToolbarSettings =
            serde_json::from_value(serde_json::json!({ "order": order, "search": false })).unwrap();
        let mut expected = prefix;
        for item in ToolbarItem::DEFAULT_ORDER {
            if !expected.contains(&item) {
                expected.push(item);
            }
        }
        assert_eq!(settings.order.as_slice(), expected);
        assert!(!settings.search);
        for item in ToolbarItem::DEFAULT_ORDER.into_iter().skip(6) {
            assert!(settings.enabled(item));
        }
        let json = serde_json::to_value(settings).unwrap();
        assert_eq!(json["order"].as_array().unwrap().len(), 17);
        assert_eq!(
            serde_json::from_value::<ToolbarSettings>(json).unwrap(),
            settings
        );
    }
}

#[test]
fn toolbar_moves_match_insertion_for_all_items_and_representative_orders() {
    let items = ToolbarItem::DEFAULT_ORDER;
    for rotation in 0..items.len() {
        let mut initial = ToolbarSettings::default();
        initial.order.rotate_left(rotation);
        for (i, entry) in items.into_iter().enumerate() {
            initial.set_enabled(entry, i % 2 == rotation % 2);
        }
        for item in items {
            for target in items {
                for after in [false, true] {
                    let mut expected = initial.order.to_vec();
                    if item != target {
                        expected.retain(|entry| *entry != item);
                        let insertion = expected.iter().position(|entry| *entry == target).unwrap()
                            + usize::from(after);
                        expected.insert(insertion, item);
                    }
                    let mut actual = initial;
                    assert_eq!(
                        actual.move_item(item, target, after),
                        expected != initial.order
                    );
                    assert_eq!(actual.order.as_slice(), expected);
                    for entry in items {
                        assert_eq!(actual.enabled(entry), initial.enabled(entry));
                    }
                }
            }
        }
    }
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
