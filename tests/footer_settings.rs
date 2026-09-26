use mithril::notebook::{FooterCountMode, FooterSettings, Notebook, Settings, save_book};

#[test]
fn footer_defaults_preserve_legacy_and_partial_settings() {
    let legacy: Settings =
        serde_json::from_str(r#"{"version":1,"last_file":null,"expanded":[]}"#).unwrap();
    assert_eq!(legacy.footer, FooterSettings::default());
    assert!(legacy.footer.visible);
    assert!(legacy.footer.word_count);
    assert!(legacy.footer.character_count);
    assert_eq!(legacy.footer.count_mode, FooterCountMode::Source);

    let partial: Settings = serde_json::from_str(
        r#"{"version":1,"last_file":null,"expanded":[],"footer":{"word_count":false}}"#,
    )
    .unwrap();
    assert_eq!(
        partial.footer,
        FooterSettings {
            word_count: false,
            ..FooterSettings::default()
        }
    );
    assert!(serde_json::from_str::<FooterSettings>(r#"{"visible":"false"}"#).is_err());
    assert!(serde_json::from_str::<FooterSettings>(r#"{"count_mode":"unknown"}"#).is_err());
}

#[test]
fn footer_choices_round_trip_and_persist_without_changing_other_preferences() {
    let directory = tempfile::tempdir().unwrap();
    let mut book = Notebook::open(&directory.path().join("notebook"), true).unwrap();
    book.settings.date_format = "MM/DD/YYYY".into();
    for (mode, encoded_mode) in [
        (FooterCountMode::Source, "source"),
        (FooterCountMode::ReadableText, "readable_text"),
    ] {
        for mask in 0..8 {
            let footer = FooterSettings {
                visible: mask & 1 != 0,
                word_count: mask & 2 != 0,
                character_count: mask & 4 != 0,
                count_mode: mode,
            };
            let encoded = serde_json::to_value(footer).unwrap();
            assert_eq!(encoded["count_mode"], encoded_mode);
            assert_eq!(
                serde_json::from_value::<FooterSettings>(encoded).unwrap(),
                footer
            );
            book.settings.footer = footer;
            save_book(&book).unwrap();
            let saved = Notebook::open(&book.root, false).unwrap();
            assert_eq!(saved.settings.footer, footer);
            assert_eq!(saved.settings.date_format, "MM/DD/YYYY");
        }
    }
}
