use super::Transport;
use crate::{HelpMenu, Notes, NotesWindow, SendFeedback};
use gpui::{Entity, prelude::*};
use gpui::{TestAppContext, VisualTestContext};
use gpui_component::{Root, WindowExt};
use markdown_note::feedback::{Failure, Payload};
use std::sync::Arc;
use std::sync::Mutex;

struct Fake {
    requests: Mutex<Vec<Payload>>,
    result: Mutex<Result<(), Failure>>,
}
impl Transport for Fake {
    fn send(&self, payload: &Payload) -> Result<(), Failure> {
        self.requests.lock().unwrap().push(payload.clone());
        *self.result.lock().unwrap()
    }
}
fn fixture(cx: &mut TestAppContext) -> (Entity<Notes>, Arc<Fake>, &mut VisualTestContext) {
    cx.update(gpui_component::init);
    let transport = Arc::new(Fake {
        requests: Mutex::new(Vec::new()),
        result: Mutex::new(Err(Failure::Offline)),
    });
    let mut notes = None;
    let (_, view) = cx.add_window_view(|window, cx| {
        let entity = cx.new(|cx| Notes::new(window, cx));
        entity.update(cx, |this, _| this.feedback.transport = transport.clone());
        notes = Some(entity.clone());
        let root = cx.new(|cx| NotesWindow {
            _subscription: cx.observe(&entity, |_, _, cx| cx.notify()),
            notes: entity,
        });
        Root::new(root, window, cx)
    });
    view.update(|window, _| window.activate_window());
    view.run_until_parked();
    (notes.unwrap(), transport, view)
}
fn settle(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(300));
    cx.refresh().unwrap();
    cx.run_until_parked();
    cx.refresh().unwrap();
    cx.run_until_parked();
}
fn draft(notes: &Entity<Notes>, message: &str, email: &str, cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.feedback
                .message
                .as_ref()
                .unwrap()
                .update(cx, |input, cx| {
                    input.set_value(message.to_string(), window, cx)
                });
            this.feedback
                .email
                .as_ref()
                .unwrap()
                .update(cx, |input, cx| {
                    input.set_value(email.to_string(), window, cx)
                });
        })
    });
    settle(cx);
}
#[gpui::test]
fn feedback_help_modal_retains_draft_restores_focus_and_respects_busy_guards(
    cx: &mut TestAppContext,
) {
    let (notes, _, cx) = fixture(cx);
    cx.update(|window, cx| notes.read(cx).focus.focus(window));
    settle(cx);
    cx.update(|window, cx| notes.read(cx).focus.focus(window));
    settle(cx);
    cx.dispatch_action(HelpMenu);
    settle(cx);
    // Activate the second Help-menu item through TestPlatform keyboard navigation.
    cx.simulate_keystrokes("down down enter");
    settle(cx);
    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
    assert!(cx.debug_bounds("feedback-message").is_some());
    draft(&notes, "draft survives", "reply@example.com", cx);
    cx.update(|window, cx| window.close_dialog(cx));
    settle(cx);
    cx.update(|window, cx| assert!(notes.read(cx).focus.is_focused(window)));
    cx.dispatch_action(SendFeedback);
    settle(cx);
    cx.update(|_, cx| {
        assert_eq!(
            notes
                .read(cx)
                .feedback
                .message
                .as_ref()
                .unwrap()
                .read(cx)
                .value(),
            "draft survives"
        )
    });
    cx.update(|window, cx| window.close_dialog(cx));
    cx.update(|_, cx| {
        notes.update(cx, |this, _| {
            let _ = this.session.pick();
        })
    });
    cx.dispatch_action(SendFeedback);
    settle(cx);
    assert!(!cx.update(|window, cx| window.has_active_dialog(cx)));
}
#[gpui::test]
fn feedback_send_close_reopen_retry_success_and_document_state_are_independent(
    cx: &mut TestAppContext,
) {
    let (notes, transport, cx) = fixture(cx);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("note.md");
    std::fs::write(&path, "private document text").unwrap();
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.install_document(Some(crate::Document::open(path).unwrap()), window, cx);
            this.editor.select(0..7, window, cx);
            this.editor.replace_selection("changed", window, cx);
        })
    });
    settle(cx);
    cx.update(|window, cx| notes.update(cx, |this, cx| this.editor.focus(window, cx)));
    cx.dispatch_action(SendFeedback);
    settle(cx);
    draft(&notes, "anonymous", "", cx);
    let before = cx.update(|_, cx| {
        (
            notes.read(cx).editor.id(),
            notes.read(cx).editor.text(cx),
            notes.read(cx).editor.selection(cx),
            notes.read(cx).dirty(cx),
        )
    });
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.send_feedback(window, cx);
            assert!(this.feedback.session.sending);
            this.send_feedback(window, cx); // Duplicate dispatch cannot enqueue a second request.
            window.close_dialog(cx);
            this.open_feedback(window, cx);
            assert!(this.feedback.session.sending);
            window.close_dialog(cx);
        })
    });
    settle(cx);
    assert_eq!(transport.requests.lock().unwrap().len(), 1);
    assert!(!cx.update(|window, cx| window.has_active_dialog(cx)));
    cx.update(|window, cx| {
        assert!(notes.read(cx).editor.is_focused(window, cx));
        assert_eq!(
            (
                notes.read(cx).editor.id(),
                notes.read(cx).editor.text(cx),
                notes.read(cx).editor.selection(cx),
                notes.read(cx).dirty(cx)
            ),
            before
        );
        assert_eq!(
            notes.read(cx).feedback.session.result,
            Some(Err(Failure::Offline))
        );
    });
    cx.dispatch_action(SendFeedback);
    settle(cx);
    for failure in [Failure::Timeout, Failure::RateLimited, Failure::Server] {
        *transport.result.lock().unwrap() = Err(failure);
        cx.update(|window, cx| notes.update(cx, |this, cx| this.send_feedback(window, cx)));
        settle(cx);
        cx.update(|_, cx| {
            assert_eq!(
                notes
                    .read(cx)
                    .feedback
                    .message
                    .as_ref()
                    .unwrap()
                    .read(cx)
                    .value(),
                "anonymous"
            )
        });
    }
    *transport.result.lock().unwrap() = Ok(());
    cx.update(|window, cx| notes.update(cx, |this, cx| this.send_feedback(window, cx)));
    settle(cx);
    let requests = transport.requests.lock().unwrap();
    assert!(
        requests
            .iter()
            .all(|p| p.submission_id == requests[0].submission_id && p.email.is_none())
    );
    drop(requests);
    cx.update(|_, cx| {
        assert_eq!(notes.read(cx).feedback.session.result, Some(Ok(())));
        assert_eq!(
            notes
                .read(cx)
                .feedback
                .message
                .as_ref()
                .unwrap()
                .read(cx)
                .value(),
            ""
        );
    });
    draft(&notes, "with contact", "reply@example.com", cx);
    cx.update(|window, cx| notes.update(cx, |this, cx| this.send_feedback(window, cx)));
    settle(cx);
    assert_eq!(
        transport
            .requests
            .lock()
            .unwrap()
            .last()
            .unwrap()
            .email
            .as_deref(),
        Some("reply@example.com")
    );
}

#[gpui::test]
fn feedback_validation_and_open_modal_do_not_pause_autosave(cx: &mut TestAppContext) {
    let (notes, transport, cx) = fixture(cx);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("autosaved.md");
    std::fs::write(&path, "before").unwrap();
    cx.update(|window, cx| {
        notes.update(cx, |this, cx| {
            this.update_external_autosave(true, window, cx);
            this.session_autosave = crate::notebook::Autosave {
                enabled: true,
                delay_ms: 100,
            };
            this.install_document(
                Some(crate::Document::open(path.clone()).unwrap()),
                window,
                cx,
            );
            this.editor.select(0..6, window, cx);
            this.editor
                .replace_selection("retained and autosaved", window, cx);
            this.editor.focus(window, cx);
        })
    });
    cx.run_until_parked();
    cx.update(|window, cx| notes.update(cx, |this, cx| this.open_feedback(window, cx)));
    settle(cx);
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "retained and autosaved"
    );
    assert!(cx.update(|window, cx| window.has_active_dialog(cx)));
    for (message, email) in [(" \n ", ""), ("feedback", "bad address")] {
        draft(&notes, message, email, cx);
        cx.update(|window, cx| notes.update(cx, |this, cx| this.send_feedback(window, cx)));
        settle(cx);
        cx.update(|_, cx| {
            assert_eq!(
                notes.read(cx).feedback.session.result,
                Some(Err(Failure::Invalid))
            )
        });
    }
    assert!(transport.requests.lock().unwrap().is_empty());
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "retained and autosaved"
    );
    cx.update(|window, cx| window.close_dialog(cx));
    settle(cx);
    cx.update(|window, cx| assert!(notes.read(cx).editor.is_focused(window, cx)));
}
