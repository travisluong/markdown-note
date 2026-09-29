//! Right-panel geometry, terminal tab selection and session cleanup coordination.
use super::*;
use crate::terminal_ui::{TerminalSession, TerminalTab, TerminalTabId};
use crate::toolbar_ui::toolbar_icon_button;
use gpui_component::{Disableable, button::ButtonCustomVariant};

impl Notes {
    pub(super) fn terminal_settings(&self) -> markdown_note::terminal::TerminalSettings {
        self.notebooks.active.as_ref().map_or_else(
            || self.session_terminal.clone(),
            |book| book.settings.terminal.clone(),
        )
    }

    fn resolved_terminal_settings(&self, cx: &App) -> markdown_note::terminal::TerminalSettings {
        let mut settings = self.terminal_settings();
        settings.font_family = Some(
            crate::settings_ui::resolve_font(
                settings.font_family.as_deref(),
                &self.installed_fonts,
                cx.theme().mono_font_family.as_ref(),
            )
            .to_string(),
        );
        settings
    }

    pub(super) fn new_terminal_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.document_interaction_blocked() || window.has_active_dialog(cx) {
            return;
        }
        let settings = self.terminal_settings();
        let directory = markdown_note::terminal::initial_directory(
            self.notebooks
                .active
                .as_ref()
                .map(|book| book.root.as_path()),
            self.tabs.active.path(),
        )
        .map_err(|e| e.to_string());
        self.terminals.serial += 1;
        let id = TerminalTabId(self.terminals.serial);
        let title = terminal_ui::shell_label(settings.clone());
        let owner = cx.weak_entity();
        let session = cx.new(|cx| TerminalSession::new(id, owner, cx));
        let subscription = cx.observe(&session, |_, _, cx| cx.notify());
        self.terminals.tabs.push(TerminalTab {
            id,
            fallback_title: title.clone(),
            title,
            session: session.clone(),
            _subscription: subscription,
        });
        self.terminals.active = Some(id);
        self.right_panel_open = true;
        session.update(cx, |session, cx| {
            session.sync(
                true,
                &self.appearance(),
                self.startup_dark,
                self.resolved_terminal_settings(cx),
                cx,
            );
            session.start(settings, directory, window, cx)
        });
        self.terminals.reveal_active();
        cx.notify();
    }

    pub(super) fn select_terminal_tab(
        &mut self,
        id: TerminalTabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.document_interaction_blocked() || window.has_active_dialog(cx) {
            return;
        }
        let Some(tab) = self.terminals.tabs.iter().find(|tab| tab.id == id) else {
            return;
        };
        self.terminals.active = Some(id);
        tab.session.read(cx).focus(window, cx);
        self.terminals.reveal_active();
        cx.notify();
    }

    pub(super) fn close_terminal_tab(
        &mut self,
        id: TerminalTabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.document_interaction_blocked() || window.has_active_dialog(cx) {
            return;
        }
        let Some(index) = self.terminals.tabs.iter().position(|tab| tab.id == id) else {
            return;
        };
        let tab = self.terminals.tabs.remove(index);
        let cleanup = tab.session.update(cx, |session, cx| session.shutdown(cx));
        // Keep the launch task alive after removing the view, including launches
        // still running on a background executor. Window close joins these tasks.
        let task = cx.spawn(async move |this, cx| {
            cleanup.wait().await;
            let _ = this.update(cx, |this, _| {
                this.terminals.cleanup.remove(&id.0);
            });
        });
        self.terminals.cleanup.insert(id.0, task);
        if self.terminals.active == Some(id) {
            self.terminals.active = self
                .terminals
                .tabs
                .get(index)
                .or_else(|| self.terminals.tabs.last())
                .map(|tab| tab.id);
            if let Some(session) = self.terminals.active_session() {
                session.read(cx).focus(window, cx);
            } else {
                self.right_panel_open = false;
                self.reset_sidebar_resize(cx);
                self.focus_after_terminal(window, cx);
            }
            self.terminals.reveal_active();
        }
        cx.notify();
    }

    pub(super) fn shutdown_terminals(&mut self, cx: &mut Context<Self>) -> Task<()> {
        // Signal every live process before waiting for any one of them.
        let cleanup: Vec<_> = self
            .terminals
            .tabs
            .iter()
            .map(|tab| tab.session.update(cx, |session, cx| session.shutdown(cx)))
            .collect();
        let closing = std::mem::take(&mut self.terminals.cleanup);
        cx.spawn(async move |_, _| {
            for session in cleanup {
                session.wait().await;
            }
            for task in closing.into_values() {
                task.await;
            }
        })
    }

    fn focus_after_terminal(&self, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs.active.mode == ViewMode::Reading || self.document().is_none() {
            self.focus.focus(window);
        } else {
            self.tabs.active.editor.focus(window, cx);
        }
    }

    pub(super) fn toggle_right_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.document_interaction_blocked() || window.has_active_dialog(cx) {
            return;
        }
        self.right_panel_open = !self.right_panel_open;
        self.reset_sidebar_resize(cx);
        if self.right_panel_open {
            if let Some(session) = self.terminals.active_session() {
                session.read(cx).focus(window, cx);
                self.terminals.reveal_active();
            } else {
                self.new_terminal_tab(window, cx);
            }
        } else {
            self.focus_after_terminal(window, cx);
        }
        cx.notify();
    }

    pub(super) fn sync_terminal(&self, window: &mut Window, cx: &mut Context<Self>) {
        let enabled = self.right_panel_open
            && !self.document_interaction_blocked()
            && !window.has_active_dialog(cx);
        for tab in &self.terminals.tabs {
            tab.session.update(cx, |session, cx| {
                session.sync(
                    enabled && self.terminals.active == Some(tab.id),
                    &self.appearance(),
                    self.startup_dark,
                    self.resolved_terminal_settings(cx),
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
        let zoom = self.appearance().zoom_factor();
        let px = |value| gpui::px(value * zoom);
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
                    .id("terminal-header")
                    .debug_selector(|| "terminal-header".into())
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
                            .id("terminal-tabs")
                            .debug_selector(|| "terminal-tabs".into())
                            .flex_shrink()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .gap_1()
                            .overflow_x_scroll()
                            .track_scroll(&self.terminals.scroll)
                            .children(self.terminals.tabs.iter().map(|tab| {
                                let id = tab.id;
                                div()
                                    .id(("terminal-tab", id.0))
                                    .debug_selector(move || format!("terminal-tab-{}", id.0))
                                    .max_w_full()
                                    .min_w_0()
                                    .flex()
                                    .items_center()
                                    .flex_shrink_0()
                                    .rounded(px(4.))
                                    .when(self.terminals.active == Some(id), |d| {
                                        d.bg(cx.theme().secondary)
                                    })
                                    .when(!blocked, |d| d.hover(|s| s.bg(cx.theme().secondary)))
                                    .child(
                                        Button::new(("terminal-tab-select", id.0))
                                            .debug_selector(move || {
                                                format!("terminal-tab-select-{}", id.0)
                                            })
                                            .custom(ButtonCustomVariant::new(cx))
                                            .small()
                                            .child(
                                                div()
                                                    .max_w(px(160.))
                                                    .truncate()
                                                    .child(tab.title.clone()),
                                            )
                                            .min_w_0()
                                            .flex_shrink()
                                            .overflow_hidden()
                                            .tooltip(tab.title.clone())
                                            .disabled(blocked)
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                this.select_terminal_tab(id, window, cx)
                                            })),
                                    )
                                    .child(
                                        Button::new(("terminal-tab-close", id.0))
                                            .debug_selector(move || {
                                                format!("terminal-tab-close-{}", id.0)
                                            })
                                            .custom(ButtonCustomVariant::new(cx))
                                            .small()
                                            .icon(IconName::Close)
                                            .tooltip("Close terminal tab")
                                            .disabled(blocked)
                                            .on_click(cx.listener(move |this, _, window, cx| {
                                                this.close_terminal_tab(id, window, cx)
                                            })),
                                    )
                            })),
                    )
                    .child(
                        div().flex_shrink_0().child(
                            toolbar_icon_button(
                                zoom,
                                "terminal-tab-add",
                                IconName::Plus,
                                icon_size,
                                blocked,
                            )
                            .tooltip("New terminal tab")
                            .on_click(
                                cx.listener(|this, _, window, cx| {
                                    this.new_terminal_tab(window, cx)
                                }),
                            ),
                        ),
                    )
                    .child(div().flex_1())
                    .child(
                        div().flex_shrink_0().child(
                            toolbar_icon_button(
                                zoom,
                                "terminal-panel-collapse",
                                IconName::PanelRight,
                                icon_size,
                                blocked,
                            )
                            .tooltip("Collapse terminal panel")
                            .on_click(cx.listener(
                                |this, _, window, cx| {
                                    this.toggle_right_panel(window, cx);
                                },
                            )),
                        ),
                    ),
            )
            .children(
                self.terminals
                    .active_session()
                    .cloned()
                    .map(|session| div().flex_1().min_h_0().overflow_hidden().child(session)),
            )
    }
}
