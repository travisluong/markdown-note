//! A synchronous capability snapshot derived from authoritative application state.
//! No UI handles or separately mutable command availability live here.
use crate::ViewMode;

#[derive(Clone, Copy, Debug)]
pub struct Capabilities {
    pub has_document: bool,
    pub source_only: bool,
    pub mode: ViewMode,
    pub busy: bool,
    pub has_notebook: bool,
    pub has_back_history: bool,
    pub has_forward_history: bool,
}

impl Capabilities {
    pub fn can_edit_markdown(self) -> bool {
        self.can_edit_document() && !self.source_only
    }
    pub fn can_edit_document(self) -> bool {
        self.has_document && !self.busy && self.mode != ViewMode::Reading
    }
    pub fn can_save(self) -> bool {
        self.has_document && !self.busy
    }
    pub fn can_search_document(self) -> bool {
        self.has_document && !self.busy
    }
    pub fn can_search_notebook(self) -> bool {
        self.has_notebook && !self.busy
    }
    pub fn can_create_note(self) -> bool {
        self.has_notebook && !self.busy
    }
}
