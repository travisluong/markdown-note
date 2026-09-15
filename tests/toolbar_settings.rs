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
