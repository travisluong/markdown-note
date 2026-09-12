//! Stable command identities, labels and capability policy.
use crate::capabilities::Capabilities;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    InsertDate,
    EditLink,
    ToggleTheme,
    ChangeNotebook,
    NewNote,
    SearchFile,
    SearchAllFiles,
    ReplaceFile,
}

impl Command {
    pub const ALL: &[Self] = &[
        Self::InsertDate,
        Self::EditLink,
        Self::ToggleTheme,
        Self::ChangeNotebook,
        Self::NewNote,
        Self::SearchFile,
        Self::SearchAllFiles,
        Self::ReplaceFile,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::InsertDate => "Templates: Insert current date",
            Self::EditLink => "Text: Insert or edit link",
            Self::ToggleTheme => "Appearance: Toggle light/dark mode",
            Self::ChangeNotebook => "Notebook: Change notebook",
            Self::NewNote => "Notebook: Create new note",
            Self::SearchFile => "Search: Search current file",
            Self::SearchAllFiles => "Search: Search in all files",
            Self::ReplaceFile => "Search: Search and replace in current file",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::InsertDate => "Insert at the editor selection",
            Self::EditLink => "Insert a link or edit the selected link",
            Self::ToggleTheme => "Switch between your Light and Dark themes",
            Self::ChangeNotebook => "Choose a registered notebook",
            Self::NewNote => "Create an Untitled note in the notebook root",
            Self::SearchFile => "Find text in the current document",
            Self::SearchAllFiles => "Search Markdown files in the active notebook",
            Self::ReplaceFile => "Find and replace text in the current document",
        }
    }

    pub fn unavailable(self, context: Capabilities) -> Option<&'static str> {
        if context.busy {
            return Some("Wait for the current operation to finish");
        }
        match self {
            Self::SearchFile if !context.can_search_document() => Some("Open a document to search"),
            Self::SearchAllFiles if !context.can_search_notebook() => {
                Some("Open a notebook to search all Markdown files")
            }
            Self::NewNote if !context.can_create_note() => Some("Open a notebook to create a note"),
            Self::InsertDate | Self::EditLink | Self::ReplaceFile
                if !context.can_edit_document() =>
            {
                Some("Open an editable document in Source or Live preview")
            }
            _ => None,
        }
    }
}
