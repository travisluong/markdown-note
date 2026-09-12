//! Find smoke checks run with disposable notebook fixtures.
use super::*;

pub(super) async fn run(notes: &Entity<Notes>, cx: &mut AsyncWindowContext) {
    InputState::diagnostic_find_matcher();
    for step in 0..=29 {
        Timer::after(std::time::Duration::from_millis(150)).await;
        cx.update(|window, cx| {
            let key = match step {
                1 | 9 | 13 | 18 | 23 | 27 => Some("ctrl-f"),
                4 | 5 | 21 => Some("enter"),
                6 => Some("shift-enter"),
                8 | 12 | 17 | 25 | 29 => Some("escape"),
                _ => None,
            };
            if let Some(key) = key {
                window.dispatch_keystroke(Keystroke::parse(key).unwrap(), cx);
                return;
            }
            notes.update(cx, |this, cx| {
                let status = this.editor.diagnostic_input().read(cx).diagnostic_find(cx);
                match step {
                    0 => {
                        this.set_mode(ViewMode::Source, window, cx);
                        this.editor
                            .diagnostic_input()
                            .update(cx, |e, cx| e.set_value("café [.*] CAFÉ café", window, cx));
                    }
                    2 => {
                        let query = status.unwrap().0;
                        assert!(query.focus_handle(cx).is_focused(window));
                        query.update(cx, |q, cx| q.set_value("café", window, cx));
                    }
                    3 => {
                        let (_, count, index, selection) = status.unwrap();
                        assert_eq!((count, index, selection.as_str()), (2, 0, "café"));
                    }
                    7 => {
                        let (_, count, index, selection) = status.unwrap();
                        assert_eq!((count, index, selection.as_str()), (2, 1, "café"));
                        assert_eq!(this.editor.text(cx).as_str(), "café [.*] CAFÉ café");
                    }
                    10 => {
                        let query = status.unwrap().0;
                        assert_eq!(
                            query.read(cx).value().as_str(),
                            "café",
                            "selection seeds find"
                        );
                        query.update(cx, |q, cx| q.set_value("[.*]", window, cx));
                    }
                    11 => {
                        let (_, count, _, selection) = status.unwrap();
                        assert_eq!((count, selection.as_str()), (1, "[.*]"));
                        this.set_mode(ViewMode::Reading, window, cx);
                    }
                    14 => {
                        assert!(
                            this.mode == ViewMode::Reading,
                            "Reading find preserves mode"
                        );
                        let query = this.reading_search.as_ref().unwrap().query.clone();
                        assert!(query.focus_handle(cx).is_focused(window));
                        query.update(cx, |q, cx| q.set_value("missing", window, cx));
                    }
                    15 => {
                        assert_eq!(this.reading_search.as_ref().unwrap().matches.len(), 0);
                        this.editor
                            .diagnostic_input()
                            .update(cx, |e, cx| e.set_value("unsaved missing", window, cx));
                    }
                    16 => {
                        assert_eq!(
                            this.reading_search.as_ref().unwrap().matches.len(),
                            1,
                            "find refreshes after edits"
                        );
                        this.set_mode(ViewMode::LivePreview, window, cx);
                        this.editor.diagnostic_input().update(cx, |e, cx| {
                            e.set_value("# café\n\n| Name |\n| --- |\n| **café** |\n", window, cx)
                        });
                    }
                    19 => {
                        status
                            .unwrap()
                            .0
                            .update(cx, |q, cx| q.set_value("café", window, cx));
                    }
                    20 => {
                        assert_eq!(status.unwrap().1, 2, "heading and table source matches");
                    }
                    22 => {
                        let (_, count, index, selection) = status.unwrap();
                        assert_eq!(
                            (count, index, selection.as_str()),
                            (2, 1, "café"),
                            "Enter selects table text"
                        );
                    }
                    24 | 28 => {
                        assert!(status.unwrap().0.focus_handle(cx).is_focused(window));
                    }
                    26 => {
                        assert!(status.is_none());
                        assert!(this.editor.focus_handle(cx).is_focused(window));
                        this.restore_tree_cursor(window, cx);
                    }
                    _ => unreachable!(),
                }
            });
        })
        .unwrap();
    }
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            assert!(
                this.editor
                    .diagnostic_input()
                    .read(cx)
                    .diagnostic_find(cx)
                    .is_none()
            );
            assert!(this.editor.focus_handle(cx).is_focused(window));
            let document = this
                .document()
                .map(|document| Document::open(document.path.clone()).unwrap());
            this.install_document(document, window, cx);
            assert!(
                this.editor
                    .diagnostic_input()
                    .read(cx)
                    .diagnostic_find(cx)
                    .is_none(),
                "new document has fresh find state"
            );
        })
    })
    .unwrap();
    for mode in [ViewMode::Source, ViewMode::LivePreview] {
        cx.update(|window, cx| notes.update(cx, |this, cx| this.set_mode(mode, window, cx)))
            .unwrap();
        for step in 0..=3 {
            Timer::after(std::time::Duration::from_millis(150)).await;
            cx.update(|window, cx| {
                notes.update(cx, |this, cx| {
                    this.editor.diagnostic_input().update(cx, |editor, cx| {
                        editor.diagnostic_replace_step(step, window, cx)
                    });
                    if step >= 2 {
                        assert!(this.dirty(cx), "replacement updates dirty state");
                    }
                })
            })
            .unwrap();
        }
    }
}
