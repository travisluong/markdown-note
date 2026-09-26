//! One retained runtime per canonical document; tabs only own views and navigation.
use crate::*;
use app::session::{DocumentId, SharedSavePolicy};
use gpui_component::input::InputBuffer;

pub(crate) struct DocumentRuntime {
    pub document: Document,
    pub buffer: Entity<InputBuffer>,
    pub _subscription: Subscription,
    pub policy: SharedSavePolicy,
    pub identity: DocumentId,
    pub saves: autosave::SaveState,
    pub dirty_revision: u64,
    pub dirty_task: Option<Task<()>>,
    pub counts: crate::footer_ui::CountState,
}

impl Notes {
    pub(crate) fn document_edited(
        &mut self,
        document: u64,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let ids = self.document_tabs(document);
        let Some(&first) = ids.first() else {
            return;
        };
        let id = if ids.contains(&self.tabs.active.id) {
            self.tabs.active.id
        } else {
            first
        };
        self.check_tab_dirty(id, window, cx);
        let snapshot = self.tabs.get(id).unwrap().editor.snapshot(cx);
        self.document_counts_edited(document, snapshot, window, cx);
        for &peer in &ids {
            let tab = self.tabs.get_mut(peer).unwrap();
            if tab.mode == ViewMode::Reading {
                tab.reading_text = tab.editor.text(cx);
            }
        }
        let tab = self.tabs.get_mut(id).unwrap();
        if let Some(identity) = tab.session.content().document_id() {
            let effects = tab.session.edit_notice(identity);
            self.apply_tab_effects(id, effects, window, cx);
        }
        self.schedule_search(window, cx);
        if id == self.tabs.active.id && self.tabs.active.mode == ViewMode::Reading {
            self.update_reading_search(cx);
        }
    }

    pub(crate) fn retained_document(&self, path: &Path) -> Option<Document> {
        let canonical = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        self.documents
            .get(&canonical)
            .map(|runtime| runtime.document.clone())
    }

    pub(crate) fn document_tabs(&self, document: u64) -> Vec<tabs::TabId> {
        self.tabs
            .iter()
            .filter(|tab| tab.document().is_some_and(|d| d.id == document))
            .map(|tab| tab.id)
            .collect()
    }

    pub(crate) fn document_write_blocked(&self, id: tabs::TabId) -> bool {
        self.tabs
            .get(id)
            .and_then(|tab| tab.document())
            .is_some_and(|doc| {
                self.tabs.iter().any(|tab| {
                    tab.document().is_some_and(|peer| peer.id == doc.id) && tab.session.is_busy()
                })
            })
    }

    pub(crate) fn retains_active_document(&self) -> bool {
        self.document()
            .is_some_and(|doc| self.document_tabs(doc.id).len() > 1)
    }

    pub(crate) fn save_state(&self, id: tabs::TabId) -> &autosave::SaveState {
        let tab = self.tabs.get(id).expect("live tab");
        tab.document()
            .and_then(|doc| {
                self.documents
                    .values()
                    .find(|runtime| runtime.document.id == doc.id)
            })
            .map_or(&tab.saves, |runtime| &runtime.saves)
    }

    pub(crate) fn save_state_mut(&mut self, id: tabs::TabId) -> &mut autosave::SaveState {
        let path = self
            .tabs
            .get(id)
            .and_then(|tab| tab.document())
            .and_then(|doc| {
                self.documents
                    .iter()
                    .find(|(_, runtime)| runtime.document.id == doc.id)
                    .map(|(path, _)| path.clone())
            });
        if let Some(path) = path.filter(|path| self.documents.contains_key(path)) {
            &mut self.documents.get_mut(&path).unwrap().saves
        } else {
            &mut self.tabs.get_mut(id).expect("live tab").saves
        }
    }

    #[cfg(any(test, feature = "editor-diagnostics"))]
    pub(crate) fn active_saves(&self) -> &autosave::SaveState {
        self.save_state(self.tabs.active.id)
    }
    #[cfg(test)]
    pub(crate) fn active_saves_mut(&mut self) -> &mut autosave::SaveState {
        self.save_state_mut(self.tabs.active.id)
    }

    pub(crate) fn document_editor(
        &mut self,
        id: tabs::TabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Editor {
        let Some(document) = self.tabs.get(id).and_then(|tab| tab.document()).cloned() else {
            return Editor::new(SharedString::default(), window, cx);
        };
        if let Some(runtime) = self.documents.get(&document.path) {
            let buffer = runtime.buffer.clone();
            let policy = runtime.policy.clone();
            let identity = runtime.identity;
            self.tabs
                .get_mut(id)
                .unwrap()
                .session
                .attach_saving(policy, identity);
            let editor = Editor::from_shared_buffer(buffer, window, cx);
            editor.break_undo_group(cx);
            return editor;
        }
        let editor = Editor::new(document.saved(), window, cx);
        let buffer = editor.shared_buffer(cx);
        let document_id = document.id;
        let subscription = cx.subscribe_in(&buffer, window, move |this, _, _, window, cx| {
            this.document_edited(document_id, window, cx)
        });
        let tab = self.tabs.get(id).unwrap();
        self.documents.insert(
            document.path.clone(),
            DocumentRuntime {
                document,
                buffer,
                _subscription: subscription,
                policy: tab.session.saving_policy(),
                identity: tab.session.content().document_id().unwrap(),
                saves: Default::default(),
                dirty_revision: 0,
                dirty_task: None,
                counts: Default::default(),
            },
        );
        self.queue_document_counts(document_id, editor.snapshot(cx), false, window, cx);
        editor
    }

    /// Drop only unreferenced documents; navigation cannot remove the final view
    /// while a write is pending. Reindex after moves using accepted tab metadata.
    pub(crate) fn reconcile_documents(&mut self) {
        let mut old = std::mem::take(&mut self.documents)
            .into_values()
            .map(|runtime| (runtime.document.id, runtime))
            .collect::<std::collections::HashMap<_, _>>();
        for tab in self.tabs.iter() {
            if let Some(doc) = tab.document()
                && let Some(mut runtime) = old.remove(&doc.id)
            {
                runtime.document.path = doc.path.clone();
                self.documents.insert(doc.path.clone(), runtime);
            }
        }
    }
}
