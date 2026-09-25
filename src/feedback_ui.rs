//! Application-scoped feedback inputs and request lifetime; modal is only an adapter.
use super::*;
use gpui_component::Disableable;
use mithril::feedback::{self, HttpTransport, Transport};
use std::sync::Arc;

pub(super) struct FeedbackState {
    session: feedback::Session,
    message: Option<Entity<InputState>>,
    email: Option<Entity<InputState>>,
    task: Option<Task<()>>,
    inputs: Vec<Subscription>,
    transport: Arc<dyn Transport>,
}
impl Default for FeedbackState {
    fn default() -> Self {
        Self {
            session: Default::default(),
            message: None,
            email: None,
            task: None,
            inputs: Vec::new(),
            transport: Arc::new(HttpTransport),
        }
    }
}
impl Notes {
    pub(super) fn open_feedback(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.document_interaction_blocked() || window.has_active_dialog(cx) {
            return;
        }
        let previous = self
            .menu_previous_focus
            .take()
            .or_else(|| window.focused(cx));
        self.menu = None;
        let message = self
            .feedback
            .message
            .get_or_insert_with(|| {
                cx.new(|cx| {
                    InputState::new(window, cx)
                        .multi_line(true)
                        .rows(8)
                        .placeholder("What worked, or what got in your way?")
                })
            })
            .clone();
        self.feedback.email.get_or_insert_with(|| {
            cx.new(|cx| InputState::new(window, cx).placeholder("Optional reply email"))
        });
        if self.feedback.inputs.is_empty() {
            for input in [&self.feedback.message, &self.feedback.email]
                .into_iter()
                .flatten()
            {
                self.feedback.inputs.push(cx.subscribe(
                    input,
                    |this, _, event: &InputEvent, cx| {
                        if matches!(event, InputEvent::Change) && !this.feedback.session.sending {
                            let has_text = [&this.feedback.message, &this.feedback.email]
                                .into_iter()
                                .flatten()
                                .any(|input| !input.read(cx).value().is_empty());
                            if has_text || this.feedback.session.result.is_some_and(|r| r.is_err())
                            {
                                this.feedback.session.result = None;
                                cx.notify();
                            }
                        }
                    },
                ));
            }
        }
        let owner = cx.entity();
        let panel = cx.new(|cx| FeedbackPanel {
            owner: owner.downgrade(),
            _subscription: cx.observe(&owner, |_, _, cx| cx.notify()),
        });
        // Root captures focus here, including for Cancel/programmatic dismissal.
        if let Some(focus) = &previous {
            focus.focus(window);
        }
        window.open_dialog(cx, move |dialog, window, cx| {
            let previous = previous.clone();
            dialog
                .title("Send feedback")
                .w(crate::zoom_ui::zoom_px(580., cx).min(window.viewport_size().width - px(32.)))
                .max_h((window.viewport_size().height - px(62.)).max(px(0.)))
                .child(panel.clone())
                .on_close(move |_, window, _| {
                    if let Some(focus) = &previous {
                        focus.focus(window);
                    }
                })
        });
        message.focus_handle(cx).focus(window);
        cx.notify();
    }
    fn send_feedback(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(message), Some(email)) = (&self.feedback.message, &self.feedback.email) else {
            return;
        };
        let payload = match self
            .feedback
            .session
            .begin(&message.read(cx).value(), &email.read(cx).value())
        {
            Ok(Some(payload)) => payload,
            Ok(None) => return,
            Err(error) => {
                self.feedback.session.result = Some(Err(error));
                cx.notify();
                return;
            }
        };
        let transport = self.feedback.transport.clone();
        let work = cx
            .background_executor()
            .spawn(async move { transport.send(&payload) });
        self.feedback.task = Some(cx.spawn_in(window, async move |this, cx| {
            let result = work.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.feedback.session.finish(result);
                if result.is_ok() {
                    for input in [&this.feedback.message, &this.feedback.email]
                        .into_iter()
                        .flatten()
                    {
                        input.update(cx, |input, cx| input.set_value("", window, cx));
                    }
                }
                // Completion only updates retained state. Never opens or focuses a modal.
                cx.notify();
            });
        }));
        cx.notify();
    }
}
struct FeedbackPanel {
    owner: WeakEntity<Notes>,
    _subscription: Subscription,
}
impl Render for FeedbackPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(owner) = self.owner.upgrade() else {
            return div().into_any_element();
        };
        let state = &owner.read(cx).feedback;
        let sending = state.session.sending;
        let status = if sending {
            "Sending feedback…"
        } else {
            match state.session.result {
                Some(Ok(())) => "Thank you. Your feedback has been saved.",
                Some(Err(error)) => error.message(),
                None => "",
            }
        };
        let retry = state.session.result.is_some_and(|r| r.is_err());
        let owner = owner.downgrade();
        div()
            .flex()
            .flex_col()
            .gap_3()
            .child("Message")
            .child(
                div()
                    .id("feedback-message")
                    .debug_selector(|| "feedback-message".into())
                    .flex_shrink_0()
                    .child(
                        Input::new(state.message.as_ref().unwrap())
                            .disabled(sending)
                            .w_full()
                            .h(gpui::rems(12.)),
                    ),
            )
            .child("Reply email (optional)")
            .child(
                div()
                    .id("feedback-email")
                    .debug_selector(|| "feedback-email".into())
                    .child(
                        Input::new(state.email.as_ref().unwrap())
                            .disabled(sending)
                            .w_full(),
                    ),
            )
            .child(
                div()
                    .id("feedback-status")
                    .debug_selector(|| "feedback-status".into())
                    .child(status),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("send-feedback")
                            .debug_selector(|| "send-feedback".into())
                            .primary()
                            .label(if sending {
                                "Sending…"
                            } else if retry {
                                "Retry feedback"
                            } else {
                                "Send feedback"
                            })
                            .disabled(sending)
                            .on_click(move |_, window, cx| {
                                let _ =
                                    owner.update(cx, |owner, cx| owner.send_feedback(window, cx));
                            }),
                    )
                    .child(
                        Button::new("cancel-feedback")
                            .debug_selector(|| "cancel-feedback".into())
                            .label("Cancel")
                            .on_click(|_, window, cx| window.close_dialog(cx)),
                    ),
            )
            .into_any_element()
    }
}
#[cfg(test)]
mod tests;
