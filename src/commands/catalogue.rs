//! Stable command identities, labels and capability policy.
use crate::{capabilities::Capabilities, markdown_edit::Format};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Bold,
    Italic,
    Strikethrough,
    InlineCode,
    CodeBlock,
    Blockquote,
    OrderedList,
    UnorderedList,
    TaskList,
    Heading1,
    Heading2,
    Heading3,
    Heading4,
    Heading5,
    Heading6,
    Paragraph,

    InsertDate,
    MoveLineUp,
    MoveLineDown,
    EditLink,
    ToggleTheme,
    Settings,
    SourceMode,
    LivePreview,
    ReadingView,
    ChangeNotebook,
    NewNote,
    NewTab,
    CloseTab,
    SearchFile,
    SearchAllFiles,
    ReplaceFile,
}

impl Command {
    pub const ALL: &[Self] = &[
        Self::Bold,
        Self::Italic,
        Self::Strikethrough,
        Self::InlineCode,
        Self::CodeBlock,
        Self::Blockquote,
        Self::OrderedList,
        Self::UnorderedList,
        Self::TaskList,
        Self::Heading1,
        Self::Heading2,
        Self::Heading3,
        Self::Heading4,
        Self::Heading5,
        Self::Heading6,
        Self::Paragraph,
        Self::InsertDate,
        Self::MoveLineUp,
        Self::MoveLineDown,
        Self::EditLink,
        Self::ToggleTheme,
        Self::Settings,
        Self::SourceMode,
        Self::LivePreview,
        Self::ReadingView,
        Self::ChangeNotebook,
        Self::NewNote,
        Self::NewTab,
        Self::CloseTab,
        Self::SearchFile,
        Self::SearchAllFiles,
        Self::ReplaceFile,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Bold => "Format: Bold",
            Self::Italic => "Format: Italic",
            Self::Strikethrough => "Format: Strikethrough",
            Self::InlineCode => "Format: Inline code",
            Self::CodeBlock => "Format: Code block",
            Self::Blockquote => "Format: Blockquote",
            Self::OrderedList => "Format: Ordered list",
            Self::UnorderedList => "Format: Unordered list",
            Self::TaskList => "Format: Task list",
            Self::Heading1 => "Format: Heading 1",
            Self::Heading2 => "Format: Heading 2",
            Self::Heading3 => "Format: Heading 3",
            Self::Heading4 => "Format: Heading 4",
            Self::Heading5 => "Format: Heading 5",
            Self::Heading6 => "Format: Heading 6",
            Self::Paragraph => "Format: Paragraph",
            Self::InsertDate => "Templates: Insert current date",
            Self::MoveLineUp => "Text: Move line up",
            Self::MoveLineDown => "Text: Move line down",
            Self::EditLink => "Text: Insert or edit link",
            Self::ToggleTheme => "Appearance: Toggle light/dark mode",
            Self::Settings => "Application: Settings",
            Self::SourceMode => "View: Source mode",
            Self::LivePreview => "View: Live preview",
            Self::ReadingView => "View: Reading view",
            Self::ChangeNotebook => "Notebook: Change notebook",
            Self::NewNote => "Notebook: Create new note",
            Self::NewTab => "Tabs: New tab",
            Self::CloseTab => "Tabs: Close current tab",
            Self::SearchFile => "Search: Search current file",
            Self::SearchAllFiles => "Search: Search in all files",
            Self::ReplaceFile => "Search: Search and replace in current file",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Self::Bold => "Toggle bold on the selection",
            Self::Italic => "Toggle italic on the selection",
            Self::Strikethrough => "Toggle strikethrough on the selection",
            Self::InlineCode => "Toggle inline code on the selection",
            Self::CodeBlock => "Toggle a fenced code block",
            Self::Blockquote => "Toggle blockquote formatting",
            Self::OrderedList => "Toggle numbered list formatting",
            Self::UnorderedList => "Toggle bulleted list formatting",
            Self::TaskList => "Apply or remove checkbox list formatting",
            Self::Heading1 => "Set selected lines to heading level 1",
            Self::Heading2 => "Set selected lines to heading level 2",
            Self::Heading3 => "Set selected lines to heading level 3",
            Self::Heading4 => "Set selected lines to heading level 4",
            Self::Heading5 => "Set selected lines to heading level 5",
            Self::Heading6 => "Set selected lines to heading level 6",
            Self::Paragraph => "Remove heading formatting from selected lines",
            Self::InsertDate => "Insert at the editor selection",
            Self::MoveLineUp => "Move the current or selected lines up",
            Self::MoveLineDown => "Move the current or selected lines down",
            Self::EditLink => "Insert a link or edit the selected link",
            Self::ToggleTheme => "Switch between your Light and Dark themes",
            Self::Settings => "Open application and notebook preferences",
            Self::SourceMode => "Show editable Markdown source",
            Self::LivePreview => "Edit Markdown with inline previews",
            Self::ReadingView => "Read the rendered document",
            Self::ChangeNotebook => "Choose a registered notebook",
            Self::NewNote => "Create an Untitled note in the notebook root",
            Self::NewTab => "Open an empty tab without creating a file",
            Self::CloseTab => "Close the active tab, asking to save unsaved changes when needed",
            Self::SearchFile => "Find text in the current document",
            Self::SearchAllFiles => "Search Markdown files in the active notebook",
            Self::ReplaceFile => "Find and replace text in the current document",
        }
    }

    /// Semantic formatting shared by palette, toolbar and document shortcuts.
    pub fn markdown_format(self) -> Option<Format> {
        Some(match self {
            Self::Bold => Format::Bold,
            Self::Italic => Format::Italic,
            Self::Strikethrough => Format::Strikethrough,
            Self::InlineCode => Format::InlineCode,
            Self::CodeBlock => Format::CodeBlock,
            Self::Blockquote => Format::Blockquote,
            Self::OrderedList => Format::OrderedList,
            Self::UnorderedList => Format::UnorderedList,
            Self::TaskList => Format::TaskList,
            Self::Heading1 => Format::Heading(1),
            Self::Heading2 => Format::Heading(2),
            Self::Heading3 => Format::Heading(3),
            Self::Heading4 => Format::Heading(4),
            Self::Heading5 => Format::Heading(5),
            Self::Heading6 => Format::Heading(6),
            Self::Paragraph => Format::Heading(0),
            _ => return None,
        })
    }

    pub fn unavailable(self, context: Capabilities) -> Option<&'static str> {
        if context.busy {
            return Some("Wait for the current operation to finish");
        }
        if self.markdown_format().is_some() && !context.can_edit_document() {
            return Some("Open an editable document in Source or Live preview");
        }
        match self {
            Self::SearchFile if !context.can_search_document() => Some("Open a document to search"),
            Self::SearchAllFiles if !context.can_search_notebook() => {
                Some("Open a notebook to search all Markdown files")
            }
            Self::NewNote if !context.can_create_note() => Some("Open a notebook to create a note"),
            Self::InsertDate
            | Self::EditLink
            | Self::ReplaceFile
            | Self::MoveLineUp
            | Self::MoveLineDown
                if !context.can_edit_document() =>
            {
                Some("Open an editable document in Source or Live preview")
            }
            _ => None,
        }
    }
}
