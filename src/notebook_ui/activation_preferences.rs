//! Explicit preference choices made while one notebook activation is pending.
//! Replay only onto the same canonical notebook and operation; externally loaded
//! fields without a user choice retain the loaded value.

use std::path::{Path, PathBuf};

use crate::{ViewMode, app::operation::OperationId, notebook};

#[derive(Clone, Copy, Debug)]
pub(crate) enum PreferenceIntent {
    ViewMode(ViewMode),
    SidebarWidth(f32),
    Autosave(notebook::Autosave),
}

pub(super) struct ActivationPreferences {
    operation: OperationId,
    root: PathBuf,
    view_mode: Option<ViewMode>,
    sidebar_width: Option<f32>,
    autosave: Option<notebook::Autosave>,
}

impl ActivationPreferences {
    pub(super) fn new(operation: OperationId, root: PathBuf) -> Self {
        Self {
            operation,
            root,
            view_mode: None,
            sidebar_width: None,
            autosave: None,
        }
    }

    pub(super) fn operation(&self) -> OperationId {
        self.operation
    }

    pub(super) fn record(&mut self, operation: OperationId, root: &Path, intent: PreferenceIntent) {
        if self.operation != operation || self.root != root {
            return;
        }
        match intent {
            PreferenceIntent::ViewMode(mode) => self.view_mode = Some(mode),
            PreferenceIntent::SidebarWidth(width) => self.sidebar_width = Some(width),
            PreferenceIntent::Autosave(autosave) => self.autosave = Some(autosave),
        }
    }

    pub(super) fn replay(self, operation: OperationId, book: &mut notebook::Notebook) {
        if self.operation != operation || self.root != book.root {
            return;
        }
        if let Some(mode) = self.view_mode {
            book.settings.view_mode = mode;
        }
        if let Some(width) = self.sidebar_width {
            book.settings.sidebar_width = width;
        }
        if let Some(autosave) = self.autosave {
            book.settings.autosave = autosave;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::{ActivationPreferences, PreferenceIntent};
    use crate::{ViewMode, app::operation::ForegroundOperation, notebook};

    #[test]
    fn choices_are_scoped_to_the_matching_operation_and_notebook() {
        let mut gate = ForegroundOperation::default();
        let operation = gate.begin().unwrap();
        assert!(gate.finish(operation));
        let stale = operation;
        let operation = gate.begin().unwrap();
        let root = PathBuf::from("A");
        let mut choices = ActivationPreferences::new(operation, root.clone());
        choices.record(stale, &root, PreferenceIntent::ViewMode(ViewMode::Reading));
        choices.record(
            operation,
            &PathBuf::from("B"),
            PreferenceIntent::SidebarWidth(400.),
        );
        let mut book = notebook::Notebook {
            root,
            settings: notebook::Settings::default(),
        };
        choices.replay(operation, &mut book);
        assert_eq!(book.settings.view_mode, ViewMode::LivePreview);
        assert_eq!(book.settings.sidebar_width, 280.);
    }

    #[test]
    fn latest_explicit_fields_replay_without_replacing_other_loaded_settings() {
        let operation = ForegroundOperation::default().begin().unwrap();
        let root = PathBuf::from("A");
        let mut choices = ActivationPreferences::new(operation, root.clone());
        choices.record(
            operation,
            &root,
            PreferenceIntent::ViewMode(ViewMode::Source),
        );
        choices.record(
            operation,
            &root,
            PreferenceIntent::ViewMode(ViewMode::Reading),
        );
        choices.record(operation, &root, PreferenceIntent::SidebarWidth(375.));
        choices.record(
            operation,
            &root,
            PreferenceIntent::Autosave(notebook::Autosave {
                enabled: false,
                delay_ms: 500,
            }),
        );
        let mut book = notebook::Notebook {
            root,
            settings: notebook::Settings {
                date_format: "MMMM Do, YYYY".into(),
                file_sort: notebook::FileSort::ModifiedNewest,
                ..Default::default()
            },
        };
        choices.replay(operation, &mut book);
        assert_eq!(book.settings.view_mode, ViewMode::Reading);
        assert_eq!(book.settings.sidebar_width, 375.);
        assert_eq!(
            book.settings.autosave,
            notebook::Autosave {
                enabled: false,
                delay_ms: 500
            }
        );
        assert_eq!(book.settings.date_format, "MMMM Do, YYYY");
        assert_eq!(book.settings.file_sort, notebook::FileSort::ModifiedNewest);
    }
}
