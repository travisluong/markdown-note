//! Right-panel geometry controls; the retained terminal owns its own lifecycle.
use super::*;
use crate::toolbar_ui::toolbar_icon_button;
use gpui_component::Disableable;

impl Notes {
    pub(super) fn terminal_settings(&self) -> mithril::terminal::TerminalSettings {
        self.notebooks
            .active
            .as_ref()
            .map_or(self.session_terminal, |book| book.settings.terminal)
    }

    fn start_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.document_interaction_blocked() || window.has_active_dialog(cx) {
            return;
        }
        let settings = self.terminal_settings();
        let directory = mithril::terminal::initial_directory(
            self.notebooks
                .active
                .as_ref()
                .map(|book| book.root.as_path()),
            self.tabs.active.path(),
        )
        .map_err(|e| e.to_string());
        let owner = cx.weak_entity();
        let panel = self
            .terminal
            .get_or_insert_with(|| cx.new(|cx| terminal_ui::TerminalPanel::new(owner, cx)));
        let panel = panel.clone();
        if self.terminal_subscription.is_none() {
            self.terminal_subscription = Some(cx.observe(&panel, |_, _, cx| cx.notify()));
        }
        panel.update(cx, |panel, cx| panel.start(settings, directory, window, cx));
    }

    pub(super) fn close_terminal_then_window(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(panel) = &self.terminal else {
            window.remove_window();
            return;
        };
        let (launch, done) = panel.update(cx, |panel, _| panel.shutdown());
        cx.spawn_in(window, async move |_, cx| {
            if let Some(launch) = launch {
                launch.await;
            }
            if let Some(done) = done {
                let _ = done.recv_async().await;
            }
            let _ = cx.update(|window, _| window.remove_window());
        })
        .detach();
    }
    pub(super) fn toggle_right_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.document_interaction_blocked() || window.has_active_dialog(cx) {
            return;
        }
        self.right_panel_open = !self.right_panel_open;
        self.reset_sidebar_resize(cx);
        if self.right_panel_open {
            if let Some(panel) = &self.terminal {
                panel.read(cx).focus(window, cx);
            } else {
                self.start_terminal(window, cx);
            }
        } else {
            if self.tabs.active.mode == ViewMode::Reading || self.document().is_none() {
                self.focus.focus(window);
            } else {
                self.tabs.active.editor.focus(window, cx);
            }
        }
        cx.notify();
    }

    pub(super) fn sync_terminal(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(panel) = &self.terminal {
            let enabled = !self.document_interaction_blocked() && !window.has_active_dialog(cx);
            panel.update(cx, |panel, cx| {
                panel.sync(
                    enabled,
                    &self.appearance(),
                    self.startup_dark,
                    self.terminal_settings(),
                    cx,
                )
            });
        }
    }

    pub(super) fn render_right_panel(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let blocked = self.document_interaction_blocked() || window.has_active_dialog(cx);
        let icon_size = self.toolbar_settings().icon_size;
        let active = self.terminal.as_ref().is_some_and(|panel| {
            matches!(
                panel.read(cx).status,
                terminal_ui::Status::Running | terminal_ui::Status::Starting
            )
        });
        div()
            .id("right-panel")
            .debug_selector(|| "right-panel".into())
            .size_full()
            .min_w_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(
                div()
                    .h(px(icon_size.button_pixels() + 9.))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .p(px(4.))
                    .border_b_1()
                    .border_color(cx.theme().border)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_sm()
                            .overflow_hidden()
                            .child("Terminal"),
                    )
                    .child(
                        Button::new("terminal-restart")
                            .debug_selector(|| "terminal-restart".into())
                            .small()
                            .ghost()
                            .label(
                                if self.terminal.as_ref().is_some_and(|p| {
                                    matches!(
                                        p.read(cx).status,
                                        terminal_ui::Status::Finished(
                                            mithril::terminal::ProcessExit::Failed(_)
                                        )
                                    )
                                }) {
                                    "Retry"
                                } else {
                                    "Restart"
                                },
                            )
                            .disabled(blocked)
                            .on_click(
                                cx.listener(|this, _, window, cx| this.start_terminal(window, cx)),
                            ),
                    )
                    .child(
                        Button::new("terminal-stop")
                            .debug_selector(|| "terminal-stop".into())
                            .small()
                            .ghost()
                            .label("Stop")
                            .disabled(blocked || !active)
                            .on_click(cx.listener(|this, _, window, cx| {
                                if !this.document_interaction_blocked()
                                    && !window.has_active_dialog(cx)
                                    && let Some(panel) = &this.terminal
                                {
                                    panel.update(cx, |panel, cx| panel.stop(cx));
                                }
                            })),
                    )
                    .child(
                        toolbar_icon_button(
                            "right-panel-close",
                            IconName::Close,
                            icon_size,
                            blocked,
                        )
                        .tooltip("Close terminal panel")
                        .on_click(
                            cx.listener(|this, _, window, cx| this.toggle_right_panel(window, cx)),
                        ),
                    ),
            )
            .children(
                self.terminal
                    .clone()
                    .map(|panel| div().flex_1().min_h_0().overflow_hidden().child(panel)),
            )
    }
}
