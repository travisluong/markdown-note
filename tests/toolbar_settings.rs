use mithril::notebook::{Notebook, Settings, ToolbarItem, ToolbarSettings, save_book};

#[test]
fn toolbar_defaults_preserve_legacy_and_partially_configured_notebooks() {
    let legacy: Settings =
        serde_json::from_str(r#"{"version":1,"last_file":null,"expanded":[]}"#).unwrap();
    assert_eq!(legacy.toolbar, ToolbarSettings::default());
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
}

#[test]
fn toolbar_combinations_round_trip_and_persist_without_changing_other_preferences() {
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path().join("notebook");
    let mut book = Notebook::open(&root, true).unwrap();
    book.settings.date_format = "MM/DD/YYYY".into();
    for mask in 0..16 {
        let mut toolbar = ToolbarSettings::default();
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
fn toolbar_order_defaults_on_invalid_data_and_serializes_item_names() {
    use ToolbarItem::*;
    for order in [
        serde_json::json!(null),
        serde_json::json!([]),
        serde_json::json!(["sidebar", "explorer", "search"]),
        serde_json::json!(["sidebar", "sidebar", "search", "sort"]),
        serde_json::json!(["sidebar", "explorer", "search", "unknown"]),
        serde_json::json!(["sidebar", "explorer", "search", "sort", "sidebar"]),
        serde_json::json!(false),
    ] {
        let settings: ToolbarSettings = serde_json::from_value(serde_json::json!({
            "order": order, "search": false
        }))
        .unwrap();
        assert_eq!(settings.order, ToolbarItem::DEFAULT_ORDER);
        assert!(!settings.search);
    }
    let settings = ToolbarSettings {
        order: [Sort, Search, Explorer, Sidebar],
        ..Default::default()
    };
    let json = serde_json::to_value(settings).unwrap();
    assert_eq!(
        json["order"],
        serde_json::json!(["sort", "search", "explorer", "sidebar"])
    );
    assert_eq!(
        serde_json::from_value::<ToolbarSettings>(json).unwrap(),
        settings
    );
}

#[test]
fn toolbar_moves_match_insertion_for_every_permutation_and_visibility_mask() {
    let items = ToolbarItem::DEFAULT_ORDER;
    for a in items {
        for b in items.into_iter().filter(|item| *item != a) {
            for c in items.into_iter().filter(|item| *item != a && *item != b) {
                let d = *items
                    .iter()
                    .find(|item| **item != a && **item != b && **item != c)
                    .unwrap();
                for mask in 0..16 {
                    let mut initial = ToolbarSettings {
                        order: [a, b, c, d],
                        ..Default::default()
                    };
                    for (index, item) in items.into_iter().enumerate() {
                        initial.set_enabled(item, mask & (1 << index) != 0);
                    }
                    for item in items {
                        for target in items {
                            for after in [false, true] {
                                let mut expected = initial.order.to_vec();
                                if item != target {
                                    expected.retain(|entry| *entry != item);
                                    let insertion =
                                        expected.iter().position(|entry| *entry == target).unwrap()
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
        }
    }
}

#[test]
fn toolbar_icon_sizes_default_normalize_and_persist() {
    use mithril::notebook::ToolbarIconSize;
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
        assert_eq!(settings.icon_size, ToolbarIconSize::Small);
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
