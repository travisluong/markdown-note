use super::{
    Command, PaletteEntry, PaletteModel, discover,
    palette::{ranked_entries, score},
};
use crate::{ViewMode, capabilities::Capabilities};
use std::{fs, path::Path};

#[test]
fn view_mode_commands_match_menu_availability() {
    let entries: Vec<_> = Command::ALL
        .iter()
        .copied()
        .map(PaletteEntry::Command)
        .collect();
    for command in [
        Command::SourceMode,
        Command::LivePreview,
        Command::ReadingView,
        Command::ZoomIn,
        Command::ZoomOut,
        Command::ResetZoom,
    ] {
        assert_eq!(
            ranked_entries(&entries, command.label())[0].label(),
            command.label()
        );
        for has_document in [false, true] {
            for has_notebook in [false, true] {
                for mode in [ViewMode::Source, ViewMode::LivePreview, ViewMode::Reading] {
                    for busy in [false, true] {
                        assert_eq!(
                            command
                                .unavailable(Capabilities {
                                    source_only: false,
                                    has_back_history: false,
                                    has_forward_history: false,
                                    has_document,
                                    has_notebook,
                                    mode,
                                    busy,
                                })
                                .is_none(),
                            !busy
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn formatting_catalogue_covers_every_toolbar_operation_with_edit_guards() {
    use crate::markdown_edit::Format;
    let formats: Vec<_> = Command::ALL
        .iter()
        .filter_map(|c| c.markdown_format())
        .collect();
    let mut expected = vec![
        Format::Bold,
        Format::Italic,
        Format::Strikethrough,
        Format::InlineCode,
        Format::CodeBlock,
        Format::Blockquote,
        Format::OrderedList,
        Format::UnorderedList,
        Format::TaskList,
    ];
    expected.extend((0..=6).map(Format::Heading));
    assert_eq!(formats.len(), expected.len());
    for format in expected {
        assert!(formats.contains(&format));
    }
    for command in Command::ALL
        .iter()
        .filter(|c| c.markdown_format().is_some())
    {
        for mode in [ViewMode::Source, ViewMode::LivePreview, ViewMode::Reading] {
            for has_document in [false, true] {
                for busy in [false, true] {
                    for has_notebook in [false, true] {
                        assert_eq!(
                            command
                                .unavailable(Capabilities {
                                    source_only: false,
                                    has_back_history: false,
                                    has_forward_history: false,
                                    mode,
                                    has_document,
                                    busy,
                                    has_notebook,
                                })
                                .is_none(),
                            has_document && !busy && mode != ViewMode::Reading,
                            "{command:?}"
                        );
                    }
                }
            }
        }
        assert_eq!(
            ranked_entries(
                &Command::ALL
                    .iter()
                    .copied()
                    .map(PaletteEntry::Command)
                    .collect::<Vec<_>>(),
                command.label()
            )[0]
            .label(),
            command.label()
        );
    }
}
#[test]
fn commands_filter_by_full_label_and_check_editability() {
    let entries: Vec<_> = Command::ALL
        .iter()
        .copied()
        .map(PaletteEntry::Command)
        .collect();
    let results = ranked_entries(&entries, "  CURRENT DATE  ");
    assert_eq!(results.len(), 1);
    assert!(matches!(
        results[0],
        PaletteEntry::Command(Command::InsertDate)
    ));
    assert!(ranked_entries(&entries, "no such command xyz").is_empty());
    for command in [Command::InsertDate, Command::ToggleTasks, Command::EditLink] {
        assert!(
            command
                .unavailable(Capabilities {
                    source_only: false,
                    has_back_history: false,
                    has_forward_history: false,
                    has_document: true,
                    mode: ViewMode::Source,
                    busy: false,
                    has_notebook: false
                })
                .is_none()
        );
        assert!(
            command
                .unavailable(Capabilities {
                    source_only: false,
                    has_back_history: false,
                    has_forward_history: false,
                    has_document: false,
                    mode: ViewMode::Source,
                    busy: false,
                    has_notebook: false
                })
                .is_some()
        );
        assert!(
            command
                .unavailable(Capabilities {
                    source_only: false,
                    has_back_history: false,
                    has_forward_history: false,
                    has_document: true,
                    mode: ViewMode::Reading,
                    busy: false,
                    has_notebook: false
                })
                .is_some()
        );
        assert!(
            command
                .unavailable(Capabilities {
                    source_only: false,
                    has_back_history: false,
                    has_forward_history: false,
                    has_document: true,
                    mode: ViewMode::Source,
                    busy: true,
                    has_notebook: false
                })
                .is_some()
        );
    }
}

#[test]
fn theme_command_works_without_an_editable_document() {
    assert!(
        Command::ToggleTheme
            .unavailable(Capabilities {
                source_only: false,
                has_back_history: false,
                has_forward_history: false,
                has_document: false,
                mode: ViewMode::Reading,
                busy: false,
                has_notebook: false
            })
            .is_none()
    );
    assert!(
        Command::ToggleTheme
            .unavailable(Capabilities {
                source_only: false,
                has_back_history: false,
                has_forward_history: false,
                has_document: true,
                mode: ViewMode::Source,
                busy: true,
                has_notebook: true
            })
            .is_some()
    );
    assert_eq!(
        ranked_entries(&[PaletteEntry::Command(Command::ToggleTheme)], "light/dark").len(),
        1
    );
}

#[test]
fn notebook_results_distinguish_duplicate_names_and_search_paths() {
    let entries = vec![
        PaletteEntry::Notebook {
            path: "one/Notes".into(),
            active: false,
        },
        PaletteEntry::Notebook {
            path: "two/Notes".into(),
            active: true,
        },
    ];
    assert_eq!(ranked_entries(&entries, "notes").len(), 2);
    let results = ranked_entries(&entries, "two/notes");
    assert_eq!(results.len(), 1);
    assert!(results[0].label().contains("(active)"));
    assert_eq!(results[0].detail(), "two/Notes");
    assert_ne!(entries[0].detail(), entries[1].detail());
    assert!(
        Command::ChangeNotebook
            .unavailable(Capabilities {
                source_only: false,
                has_back_history: false,
                has_forward_history: false,
                has_document: false,
                mode: ViewMode::Reading,
                busy: false,
                has_notebook: false
            })
            .is_none()
    );
}

#[test]
fn new_note_requires_a_notebook_but_not_an_editable_document() {
    assert!(
        Command::NewNote
            .unavailable(Capabilities {
                source_only: false,
                has_back_history: false,
                has_forward_history: false,
                has_document: false,
                mode: ViewMode::Reading,
                busy: false,
                has_notebook: true
            })
            .is_none()
    );
    assert!(
        Command::NewNote
            .unavailable(Capabilities {
                source_only: false,
                has_back_history: false,
                has_forward_history: false,
                has_document: true,
                mode: ViewMode::Source,
                busy: false,
                has_notebook: false
            })
            .is_some()
    );
    assert!(
        Command::NewNote
            .unavailable(Capabilities {
                source_only: false,
                has_back_history: false,
                has_forward_history: false,
                has_document: true,
                mode: ViewMode::Source,
                busy: true,
                has_notebook: true
            })
            .is_some()
    );
}

#[test]
fn current_file_search_supports_reading_and_standalone_documents() {
    assert!(
        Command::SearchFile
            .unavailable(Capabilities {
                source_only: false,
                has_back_history: false,
                has_forward_history: false,
                has_document: true,
                mode: ViewMode::Reading,
                busy: false,
                has_notebook: false
            })
            .is_none()
    );
    assert!(
        Command::SearchFile
            .unavailable(Capabilities {
                source_only: false,
                has_back_history: false,
                has_forward_history: false,
                has_document: false,
                mode: ViewMode::Source,
                busy: false,
                has_notebook: true
            })
            .is_some()
    );
}

#[test]
fn all_files_search_requires_only_an_active_notebook() {
    assert!(
        Command::SearchAllFiles
            .unavailable(Capabilities {
                source_only: false,
                has_back_history: false,
                has_forward_history: false,
                has_document: false,
                mode: ViewMode::Reading,
                busy: false,
                has_notebook: true
            })
            .is_none()
    );
    assert!(
        Command::SearchAllFiles
            .unavailable(Capabilities {
                source_only: false,
                has_back_history: false,
                has_forward_history: false,
                has_document: true,
                mode: ViewMode::Source,
                busy: false,
                has_notebook: false
            })
            .is_some()
    );
}

#[test]
fn replacement_requires_an_editable_document() {
    assert!(
        Command::ReplaceFile
            .unavailable(Capabilities {
                source_only: false,
                has_back_history: false,
                has_forward_history: false,
                has_document: true,
                mode: ViewMode::Source,
                busy: false,
                has_notebook: false
            })
            .is_none()
    );
    assert!(
        Command::ReplaceFile
            .unavailable(Capabilities {
                source_only: false,
                has_back_history: false,
                has_forward_history: false,
                has_document: true,
                mode: ViewMode::Reading,
                busy: false,
                has_notebook: true
            })
            .is_some()
    );
    assert!(
        Command::ReplaceFile
            .unavailable(Capabilities {
                source_only: false,
                has_back_history: false,
                has_forward_history: false,
                has_document: false,
                mode: ViewMode::Source,
                busy: false,
                has_notebook: true
            })
            .is_some()
    );
    assert!(
        Command::ReplaceFile
            .unavailable(Capabilities {
                source_only: false,
                has_back_history: false,
                has_forward_history: false,
                has_document: true,
                mode: ViewMode::Source,
                busy: true,
                has_notebook: true
            })
            .is_some()
    );
}

#[test]
fn ranking() {
    assert!(score("note", "note") < score("notebook", "note"));
    assert!(score("notebook", "note") < score("a note", "note"));
    assert!(score("a note", "note") < score("n o t e", "note"));
    assert_eq!(score("note", "xyz"), None);
    assert!(score("日本語", "日語").is_some());
}
#[test]
fn recursive_duplicates_and_settings() {
    let temp = tempfile::tempdir().unwrap();
    for folder in ["a/deep", "b", ".mithril"] {
        fs::create_dir_all(temp.path().join(folder)).unwrap();
    }
    for file in [
        "a/deep/Note.md",
        "b/Note.md",
        "b/ignore.txt",
        ".mithril/private.md",
    ] {
        fs::write(temp.path().join(file), "").unwrap();
    }
    let entries = discover(temp.path(), &crate::notebook::Exclusions::default()).unwrap();
    assert_eq!(entries.iter().filter(|e| !e.directory).count(), 2);
    assert!(
        entries
            .iter()
            .any(|e| e.relative == Path::new("a/deep/Note.md"))
    );
    assert!(
        discover(
            &temp.path().join("missing"),
            &crate::notebook::Exclusions::default()
        )
        .is_err()
    );
}

#[test]
fn palette_navigation_wraps_within_filtered_results() {
    let mut model = PaletteModel::new(
        ["alpha.md", "beta.md", "gamma.md", "other.txt"]
            .into_iter()
            .map(|name| PaletteEntry::Notebook {
                path: name.into(),
                active: false,
            })
            .collect(),
    );
    model.filter(".md");
    assert_eq!(model.results().len(), 3);
    assert_eq!(model.move_selection(false), 2);
    assert_eq!(model.move_selection(true), 0);
    assert_eq!(model.move_selection(true), 1);
    assert_eq!(model.move_selection(true), 2);
    assert_eq!(model.move_selection(false), 1);
    assert_eq!(model.move_selection(false), 0);
    assert_eq!(model.move_selection(false), 2);

    model.filter("alpha");
    assert_eq!(model.results().len(), 1);
    assert_eq!(model.move_selection(false), 0);
    assert_eq!(model.move_selection(true), 0);

    model.filter("missing");
    assert!(model.results().is_empty());
    assert_eq!(model.move_selection(false), 0);
    assert_eq!(model.move_selection(true), 0);
    assert!(model.selected_entry().is_none());
}

#[test]
fn selection_and_replacement_follow_the_current_filtered_results() {
    let mut model = PaletteModel::new(
        Command::ALL
            .iter()
            .copied()
            .map(PaletteEntry::Command)
            .collect(),
    );
    model.move_selection(true);
    assert_eq!(model.selected_index(), 1);
    model.filter("current date");
    assert_eq!(model.selected_index(), 0);
    assert!(matches!(
        model.selected_entry(),
        Some(PaletteEntry::Command(Command::InsertDate))
    ));
    assert!(!model.select(1));
    assert_eq!(model.move_selection(true), 0);
    model.replace_entries(
        vec![PaletteEntry::Command(Command::SearchFile)],
        "current date",
    );
    assert!(model.results().is_empty());
    assert!(model.selected_entry().is_none());
    assert_eq!(model.move_selection(false), 0);
    model.filter("search");
    assert!(matches!(
        model.selected_entry(),
        Some(PaletteEntry::Command(Command::SearchFile))
    ));
}

#[test]
fn settings_is_discoverable_without_a_notebook_or_document_and_rechecks_busy() {
    let entries: Vec<_> = Command::ALL
        .iter()
        .copied()
        .map(PaletteEntry::Command)
        .collect();
    assert_eq!(
        ranked_entries(&entries, "settings")[0].label(),
        Command::Settings.label()
    );
    for has_document in [false, true] {
        for has_notebook in [false, true] {
            for mode in [ViewMode::Source, ViewMode::LivePreview, ViewMode::Reading] {
                for busy in [false, true] {
                    assert_eq!(
                        Command::Settings
                            .unavailable(Capabilities {
                                source_only: false,
                                has_back_history: false,
                                has_forward_history: false,
                                has_document,
                                has_notebook,
                                mode,
                                busy
                            })
                            .is_none(),
                        !busy
                    );
                }
            }
        }
    }
}

#[test]
fn history_commands_use_tab_targets_independently_of_content_and_notebook() {
    for has_document in [false, true] {
        for has_notebook in [false, true] {
            for busy in [false, true] {
                for has_back_history in [false, true] {
                    for has_forward_history in [false, true] {
                        let context = Capabilities {
                            source_only: false,
                            has_document,
                            has_notebook,
                            mode: ViewMode::Reading,
                            busy,
                            has_back_history,
                            has_forward_history,
                        };
                        assert_eq!(
                            Command::Back.unavailable(context).is_none(),
                            has_back_history && !busy
                        );
                        assert_eq!(
                            Command::Forward.unavailable(context).is_none(),
                            has_forward_history && !busy
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn sidebar_commands_require_a_notebook_and_idle_state() {
    for command in [Command::ToggleSidebar, Command::ShowExplorer] {
        assert!(Command::ALL.contains(&command));
        for has_notebook in [false, true] {
            for busy in [false, true] {
                for mode in [ViewMode::Source, ViewMode::LivePreview, ViewMode::Reading] {
                    assert_eq!(
                        command
                            .unavailable(Capabilities {
                                source_only: false,
                                has_document: false,
                                has_notebook,
                                mode,
                                busy,
                                has_back_history: false,
                                has_forward_history: false,
                            })
                            .is_none(),
                        has_notebook && !busy
                    );
                }
            }
        }
    }
}

#[test]
fn discovery_commands_are_searchable_and_save_requires_a_document_in_any_mode() {
    let entries: Vec<_> = Command::ALL
        .iter()
        .copied()
        .map(PaletteEntry::Command)
        .collect();
    for command in [
        Command::OpenFile,
        Command::Save,
        Command::ManageNotebooks,
        Command::KeyboardShortcuts,
    ] {
        assert_eq!(
            ranked_entries(&entries, command.label())[0].label(),
            command.label()
        );
        for has_document in [false, true] {
            for mode in [ViewMode::Source, ViewMode::LivePreview, ViewMode::Reading] {
                for busy in [false, true] {
                    let context = Capabilities {
                        source_only: false,
                        has_document,
                        mode,
                        busy,
                        has_notebook: false,
                        has_back_history: false,
                        has_forward_history: false,
                    };
                    assert_eq!(
                        command.unavailable(context).is_none(),
                        !busy && (command != Command::Save || has_document)
                    );
                }
            }
        }
    }
}

#[test]
fn sort_picker_uses_all_options_and_notebook_capability() {
    use crate::notebook::FileSort;
    let entries: Vec<_> = FileSort::OPTIONS
        .into_iter()
        .map(|(sort, label)| PaletteEntry::FileSort {
            sort,
            label,
            active: sort == FileSort::ModifiedNewest,
        })
        .collect();
    assert_eq!(entries.len(), 6);
    assert_eq!(
        entries
            .iter()
            .filter(|entry| entry.label().ends_with("(current)"))
            .count(),
        1
    );
    for (sort, label) in FileSort::OPTIONS {
        assert!(
            matches!(ranked_entries(&entries, &label.to_uppercase())[0], PaletteEntry::FileSort { sort: actual, .. } if actual == sort)
        );
    }
    for has_notebook in [false, true] {
        for busy in [false, true] {
            for has_document in [false, true] {
                let caps = Capabilities {
                    source_only: false,
                    has_notebook,
                    busy,
                    has_document,
                    mode: ViewMode::Reading,
                    has_back_history: false,
                    has_forward_history: false,
                };
                assert_eq!(
                    Command::SortFiles.unavailable(caps).is_none(),
                    has_notebook && !busy
                );
                assert_eq!(Command::ToggleTheme.unavailable(caps).is_none(), !busy);
            }
        }
    }
}
