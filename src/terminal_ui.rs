//! Retained terminal tabs and generation-guarded per-session process installation.
use gpui::{prelude::*, *};
use gpui_component::{
    ActiveTheme, WindowExt,
    menu::{PopupMenu, PopupMenuItem},
};
use gpui_terminal::{ColorPalette, TerminalConfig, TerminalView};
use mithril::terminal::{ProcessExit, TerminalProcess, TerminalSettings};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct TerminalTabId(pub u64);

pub(super) struct TerminalTab {
    pub id: TerminalTabId,
    pub title: String,
    pub fallback_title: String,
    pub session: Entity<TerminalSession>,
    pub _subscription: Subscription,
}

#[derive(Default)]
pub(super) struct TerminalTabs {
    pub tabs: Vec<TerminalTab>,
    pub active: Option<TerminalTabId>,
    pub serial: u64,
    pub scroll: ScrollHandle,
    pub cleanup: std::collections::BTreeMap<u64, Task<()>>,
}

impl TerminalTabs {
    pub fn active_session(&self) -> Option<&Entity<TerminalSession>> {
        self.tabs
            .iter()
            .find(|tab| Some(tab.id) == self.active)
            .map(|tab| &tab.session)
    }

    pub fn reveal_active(&self) {
        if let Some(index) = self.tabs.iter().position(|tab| Some(tab.id) == self.active) {
            self.scroll.scroll_to_item(index);
        }
    }
}

pub(super) struct TerminalCleanup {
    launch: Option<Task<()>>,
    done: Option<flume::Receiver<()>>,
}

impl TerminalCleanup {
    pub async fn wait(self) {
        if let Some(launch) = self.launch {
            launch.await;
        }
        if let Some(done) = self.done {
            let _ = done.recv_async().await;
        }
    }
}

pub(super) fn shell_label(settings: TerminalSettings) -> String {
    #[cfg(windows)]
    {
        match settings.windows_shell {
            mithril::terminal::WindowsShell::Powershell => "PowerShell",
            mithril::terminal::WindowsShell::GitBash => "Git Bash",
        }
        .into()
    }
    #[cfg(not(windows))]
    {
        mithril::terminal::launch_spec(settings, PathBuf::new())
            .ok()
            .and_then(|spec| {
                spec.program
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| "sh".into())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Status {
    Idle,
    Starting,
    Running,
    Finished(ProcessExit),
}

pub(super) struct TerminalSession {
    id: TerminalTabId,
    pub view: Option<Entity<TerminalView>>,
    owner: WeakEntity<crate::Notes>,
    pub status: Status,
    generation: u64,
    process: Option<TerminalProcess>,
    // Latest appearance survives asynchronous process launch and view installation.
    config: TerminalConfig,
    task: Option<Task<()>>,
    title_subscription: Option<Subscription>,
    focus: FocusHandle,
    context_menu: Option<(Point<Pixels>, Entity<PopupMenu>)>,
    context_subscription: Option<Subscription>,
    #[cfg(test)]
    fixture_output: Option<flume::Sender<Vec<u8>>>,
    #[cfg(test)]
    pub fixture_input: std::sync::Arc<std::sync::Mutex<Vec<u8>>>,
    #[cfg(test)]
    pub last_launch: Option<(TerminalSettings, Result<PathBuf, String>)>,
}

impl TerminalSession {
    pub fn new(id: TerminalTabId, owner: WeakEntity<crate::Notes>, cx: &mut Context<Self>) -> Self {
        Self {
            id,
            owner,
            view: None,
            status: Status::Idle,
            generation: 0,
            process: None,
            config: TerminalConfig {
                cursor_style: cursor_style(mithril::terminal::CursorStyle::default()),
                ..Default::default()
            },
            task: None,
            title_subscription: None,
            focus: cx.focus_handle(),
            context_menu: None,
            context_subscription: None,
            #[cfg(test)]
            fixture_output: None,
            #[cfg(test)]
            fixture_input: Default::default(),
            #[cfg(test)]
            last_launch: None,
        }
    }

    fn view_config(&self) -> TerminalConfig {
        self.config.clone()
    }

    pub fn focus(&self, window: &mut Window, cx: &App) {
        self.view
            .as_ref()
            .map_or(&self.focus, |view| view.read(cx).focus_handle())
            .focus(window);
    }

    pub fn start(
        &mut self,
        settings: TerminalSettings,
        directory: Result<PathBuf, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.generation += 1;
        let generation = self.generation;
        self.config.cursor_style = cursor_style(settings.cursor_style);
        let old = self.process.take();
        let pending = self.task.take();
        if let Some(process) = &old {
            process.control.stop();
        }
        self.status = Status::Starting;
        self.focus.focus(window);
        // Each tab launches once and retains its view until closed.
        self.view = None;
        self.title_subscription = None;
        cx.notify();
        #[cfg(test)]
        {
            self.last_launch = Some((settings, directory));
            let _ = (generation, old, pending);
            let (tx, rx) = flume::bounded(64);
            self.fixture_output = Some(tx);
            let owner = self.owner.clone();
            let id = self.id;
            self.view = Some(cx.new(|cx| {
                TerminalView::from_receiver(
                    FixtureWriter(self.fixture_input.clone()),
                    rx,
                    self.view_config(),
                    cx,
                )
                .with_input_guard(move |window, cx| input_allowed(&owner, id, window, cx))
            }));
            self.status = Status::Running;
            self.subscribe_title(generation, cx);
            self.focus(window, cx);
        }
        #[cfg(not(test))]
        {
            self.task = Some(cx.spawn_in(window, async move |this, cx| {
                if let Some(pending) = pending {
                    pending.await;
                }
                // Preserve cleanup ordering if a session is started again internally.
                if let Some(old) = old {
                    let _ = old.done.recv_async().await;
                    drop(old);
                }
                let launched = cx
                    .background_executor()
                    .spawn(async move {
                        let cwd = directory?;
                        let spec = mithril::terminal::launch_spec(settings, cwd)
                            .map_err(|e| e.to_string())?;
                        TerminalProcess::spawn(spec)
                    })
                    .await;
                // Keep ownership outside the update closure so a destroyed panel
                // still stops and awaits a late launch before this task finishes.
                let mut launched = Some(launched);
                let _ = this.update_in(cx, |panel, window, cx| {
                    if panel.generation == generation {
                        panel.install(generation, launched.take().unwrap(), window, cx);
                    }
                });
                if let Some(Ok(process)) = launched {
                    process.control.stop();
                    let _ = process.done.recv_async().await;
                }
            }));
        }
    }

    fn install(
        &mut self,
        generation: u64,
        result: Result<TerminalProcess, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if generation != self.generation {
            return;
        } // dropped process stops itself
        match result {
            Ok(process) => {
                let resize = process.control.clone();
                let output = process.output.clone();
                let input = process.input.clone();
                let exit = process.exit.clone();
                let take_focus = self.focus.is_focused(window) && !window.has_active_dialog(cx);
                let owner = self.owner.clone();
                let id = self.id;
                self.view = Some(cx.new(|cx| {
                    TerminalView::from_receiver(input, output, self.view_config(), cx)
                        .with_resize_callback(move |cols, rows| resize.resize(cols, rows))
                        .with_input_guard(move |window, cx| input_allowed(&owner, id, window, cx))
                }));
                self.process = Some(process);
                self.status = Status::Running;
                self.subscribe_title(generation, cx);
                if take_focus {
                    self.focus(window, cx);
                }
                cx.spawn(async move |this, cx| {
                    let result = exit.recv_async().await.unwrap_or_else(|_| {
                        ProcessExit::Failed("Terminal worker disconnected".into())
                    });
                    let _ = this.update(cx, |panel, cx| {
                        if panel.generation == generation {
                            panel.status = Status::Finished(result);
                            if let Some(view) = &panel.view {
                                view.update(cx, |view, _| view.set_input_enabled(false));
                            }
                            panel.process = None;
                            cx.notify();
                        }
                    });
                })
                .detach();
            }
            Err(error) => self.status = Status::Finished(ProcessExit::Failed(error)),
        }
        cx.notify();
    }

    fn subscribe_title(&mut self, generation: u64, cx: &mut Context<Self>) {
        let Some(view) = &self.view else { return };
        self.title_subscription = Some(cx.subscribe(
            view,
            move |this, view, event: &gpui_terminal::TerminalTitleChanged, cx| {
                if this.generation != generation || this.view.as_ref() != Some(&view) {
                    return;
                }
                let session = cx.entity();
                let title: String = event
                    .0
                    .chars()
                    .map(|c| if c.is_control() { ' ' } else { c })
                    .collect();
                let title = title.trim();
                let _ = this.owner.update(cx, |owner, cx| {
                    let Some(tab) = owner
                        .terminals
                        .tabs
                        .iter_mut()
                        .find(|tab| tab.id == this.id && tab.session == session)
                    else {
                        return;
                    };
                    let title = if title.is_empty() {
                        tab.fallback_title.clone()
                    } else {
                        title.to_owned()
                    };
                    if tab.title != title {
                        tab.title = title;
                        owner.terminals.reveal_active();
                        cx.notify();
                    }
                });
            },
        ));
    }

    /// The accepted close barrier awaits process cleanup before removing the window.
    pub fn shutdown(&mut self, cx: &mut Context<Self>) -> TerminalCleanup {
        self.generation += 1;
        self.context_menu = None;
        self.context_subscription = None;
        self.title_subscription = None;
        self.status = Status::Finished(ProcessExit::Stopped);
        if let Some(view) = &self.view {
            view.update(cx, |view, _| view.set_input_enabled(false));
        }
        let done = self.process.as_ref().map(|process| {
            process.control.stop();
            process.done.clone()
        });
        TerminalCleanup {
            launch: self.task.take(),
            done,
        }
    }

    pub fn sync(
        &mut self,
        enabled: bool,
        appearance: &mithril::notebook::Appearance,
        startup_dark: bool,
        settings: TerminalSettings,
        cx: &mut Context<Self>,
    ) {
        let palette = appearance.palette(startup_dark);
        if !enabled {
            self.context_menu = None;
            self.context_subscription = None;
        }
        let rgb = |value: u32| ((value >> 16) as u8, (value >> 8) as u8, value as u8);
        let (br, bg, bb) = rgb(palette.background);
        let (fr, fg, fb) = rgb(palette.font);
        self.config.cursor_style = cursor_style(settings.cursor_style);
        self.config.font_family = settings
            .font_family
            .clone()
            .unwrap_or_else(|| cx.theme().mono_font_family.to_string());
        self.config.font_size = px(f32::from(
            settings.effective_font_size(appearance.content_font_size),
        ));
        self.config.padding = Edges::all(px(4.));
        self.config.colors = ColorPalette::builder()
            .background(br, bg, bb)
            .foreground(fr, fg, fb)
            .cursor(fr, fg, fb)
            .build();
        if let Some(view) = &self.view {
            view.update(cx, |view, cx| {
                view.set_input_enabled(enabled && self.status == Status::Running);
                let config = view.config();
                if config.cursor_style != self.config.cursor_style
                    || config.font_family != self.config.font_family
                    || config.font_size != self.config.font_size
                    || config.colors.background() != self.config.colors.background()
                    || config.colors.foreground() != self.config.colors.foreground()
                {
                    view.update_config(self.config.clone(), cx);
                }
            });
        }
    }

    pub fn message(&self) -> Option<String> {
        match &self.status {
            Status::Idle => None,
            Status::Starting => Some("Starting terminal…".into()),
            Status::Running => None,
            Status::Finished(ProcessExit::Stopped) => Some("Terminal stopped.".into()),
            Status::Finished(ProcessExit::Exited(code)) => {
                Some(format!("Shell exited with status {code}."))
            }
            Status::Finished(ProcessExit::Failed(error)) => Some(format!("Terminal: {error}")),
        }
    }

    fn open_context_menu(
        &mut self,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !input_allowed(&self.owner, self.id, window, cx) {
            return;
        }
        let Some(view) = self.view.clone() else {
            return;
        };
        let copy = view.read(cx).has_selection();
        let paste = self.status == Status::Running
            && cx
                .read_from_clipboard()
                .and_then(|item| item.text())
                .is_some_and(|text| !text.is_empty());
        let generation = self.generation;
        let owner = cx.weak_entity();
        let menu = PopupMenu::build(window, cx, move |mut menu, _, cx| {
            let menu_id = cx.entity_id();
            for (label, copying, enabled) in [("Copy", true, copy), ("Paste", false, paste)] {
                let owner = owner.clone();
                let view = view.clone();
                menu = menu.item(PopupMenuItem::new(label).disabled(!enabled).on_click(
                    move |_, window, cx| {
                        let _ = owner.update(cx, |this, cx| {
                            this.clipboard_action(
                                &view,
                                (generation, menu_id),
                                copying,
                                window,
                                cx,
                            );
                        });
                    },
                ));
            }
            menu
        });
        self.context_subscription =
            Some(
                cx.subscribe_in(&menu, window, |this, menu, _: &DismissEvent, window, cx| {
                    if this
                        .context_menu
                        .as_ref()
                        .is_some_and(|(_, current)| current == menu)
                    {
                        this.context_menu = None;
                        if input_allowed(&this.owner, this.id, window, cx) {
                            this.focus(window, cx);
                        }
                        cx.notify();
                    }
                }),
            );
        menu.focus_handle(cx).focus(window);
        self.context_menu = Some((position, menu));
        cx.notify();
    }

    fn clipboard_action(
        &mut self,
        view: &Entity<TerminalView>,
        target: (u64, EntityId),
        copying: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .context_menu
            .as_ref()
            .is_none_or(|(_, menu)| menu.entity_id() != target.1)
            || self.generation != target.0
            || self.view.as_ref() != Some(view)
            || !input_allowed(&self.owner, self.id, window, cx)
        {
            return;
        }
        if copying {
            view.update(cx, |view, cx| view.copy_selection(window, cx));
        } else if self.status == Status::Running {
            view.update(cx, |view, cx| view.paste_from_clipboard(window, cx));
        }
        self.context_menu = None;
        self.focus(window, cx);
        cx.notify();
    }
}

impl Render for TerminalSession {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("terminal-panel-content")
            .debug_selector(|| "terminal-panel-content".into())
            .size_full()
            .min_h_0()
            .min_w_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .track_focus(&self.focus)
            .children(self.view.clone().map(|view| {
                div()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(|this, event: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            this.open_context_menu(event.position, window, cx);
                        }),
                    )
                    .child(view)
            }))
            .children(self.context_menu.as_ref().map(|(position, menu)| {
                deferred(
                    anchored()
                        .position(*position)
                        .snap_to_window_with_margin(px(4.))
                        .child(
                            div()
                                .debug_selector(|| "terminal-context-menu".into())
                                .child(menu.clone()),
                        ),
                )
            }))
            .when_some(self.message(), |div, message| {
                div.child(div_message(message, cx))
            })
            .when_some(
                self.view
                    .as_ref()
                    .and_then(|view| view.read(cx).input_error())
                    .map(str::to_owned),
                |div, message| div.child(div_message(message, cx)),
            )
    }
}
fn cursor_style(style: mithril::terminal::CursorStyle) -> gpui_terminal::CursorStyle {
    match style {
        mithril::terminal::CursorStyle::Line => gpui_terminal::CursorStyle::Line,
        mithril::terminal::CursorStyle::Box => gpui_terminal::CursorStyle::Box,
        mithril::terminal::CursorStyle::Underline => gpui_terminal::CursorStyle::Underline,
    }
}
fn div_message(message: String, cx: &App) -> impl IntoElement {
    div()
        .p_2()
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child(message)
}

fn input_allowed(
    owner: &WeakEntity<crate::Notes>,
    id: TerminalTabId,
    window: &mut Window,
    cx: &mut App,
) -> bool {
    !window.has_active_dialog(cx)
        && owner.upgrade().is_some_and(|owner| {
            let notes = owner.read(cx);
            notes.right_panel_open
                && notes.terminals.active == Some(id)
                && !notes.document_interaction_blocked()
        })
}
#[cfg(test)]
struct FixtureWriter(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);
#[cfg(test)]
impl std::io::Write for FixtureWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{ProcessExit, Status};
    use gpui::{AppContext, TestAppContext};
    #[gpui::test]
    fn terminal_typography_sync_preserves_output_and_pending_launch_configuration(
        cx: &mut TestAppContext,
    ) {
        use mithril::terminal::{CursorStyle, TerminalSettings};
        cx.update(gpui_component::init);
        let mut owner = None;
        let (_, cx) = cx.add_window_view(|window, cx| {
            let notes = cx.new(|cx| crate::Notes::new(window, cx));
            owner = Some(notes.clone());
            gpui_component::Root::new(notes, window, cx)
        });
        let owner = owner.unwrap();
        let (panel, view, tx, input, generation) = cx.update(|window, cx| {
            owner.update(cx, |notes, cx| notes.toggle_right_panel(window, cx));
            let panel = owner.read(cx).terminals.active_session().cloned().unwrap();
            let state = panel.read(cx);
            (
                panel.clone(),
                state.view.clone().unwrap(),
                state.fixture_output.clone().unwrap(),
                state.fixture_input.clone(),
                state.generation,
            )
        });
        tx.send(b"\x1b[2J\x1b[Hretained".to_vec()).unwrap();
        cx.run_until_parked();
        for style in [CursorStyle::Box, CursorStyle::Underline, CursorStyle::Line] {
            cx.update(|_, cx| {
                panel.update(cx, |panel, cx| {
                    panel.sync(
                        true,
                        &Default::default(),
                        false,
                        TerminalSettings {
                            cursor_style: style,
                            font_family: Some("Test terminal font".into()),
                            font_size: Some(22),
                            ..Default::default()
                        },
                        cx,
                    );
                    assert_eq!(view.read(cx).config().font_family, "Test terminal font");
                    assert_eq!(view.read(cx).config().font_size, gpui::px(22.));
                    assert_eq!(panel.view.as_ref(), Some(&view));
                    assert_eq!(panel.generation, generation);
                    assert_eq!(
                        view.read(cx).config().cursor_style,
                        super::cursor_style(style)
                    );
                });
            });
            input.lock().unwrap().clear();
            tx.send(b"\x1b[6n".to_vec()).unwrap();
            cx.executor()
                .advance_clock(std::time::Duration::from_millis(10));
            cx.run_until_parked();
            assert_eq!(*input.lock().unwrap(), b"\x1b[1;9R");
        }
        cx.update(|_, cx| {
            panel.update(cx, |panel, cx| {
                panel.view = None;
                panel.status = Status::Starting;
                panel.sync(
                    false,
                    &Default::default(),
                    false,
                    TerminalSettings {
                        cursor_style: CursorStyle::Underline,
                        font_family: Some("Latest pending font".into()),
                        font_size: Some(25),
                        ..Default::default()
                    },
                    cx,
                );
                assert_eq!(panel.view_config().font_family, "Latest pending font");
                assert_eq!(panel.view_config().font_size, gpui::px(25.));
                // Async installation and fixture construction use this same current config.
                assert_eq!(
                    panel.view_config().cursor_style,
                    gpui_terminal::CursorStyle::Underline
                );
                assert_eq!(panel.generation, generation);
            })
        });
    }

    #[gpui::test]
    fn stale_launch_failure_cannot_replace_a_new_session(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let mut owner = None;
        let (_, cx) = cx.add_window_view(|window, cx| {
            let notes = cx.new(|cx| crate::Notes::new(window, cx));
            owner = Some(notes.clone());
            gpui_component::Root::new(notes, window, cx)
        });
        let owner = owner.unwrap();
        cx.update(|window, cx| {
            owner.update(cx, |notes, cx| notes.toggle_right_panel(window, cx));
            let panel = owner.read(cx).terminals.active_session().cloned().unwrap();
            panel.update(cx, |panel, cx| {
                let generation = panel.generation;
                let view = panel.view.clone().unwrap();
                panel.install(generation - 1, Err("stale".into()), window, cx);
                assert_eq!(panel.status, Status::Running);
                assert_eq!(panel.view, Some(view));
                panel.install(generation, Err("missing shell".into()), window, cx);
                assert_eq!(
                    panel.status,
                    Status::Finished(ProcessExit::Failed("missing shell".into()))
                );
            });
        });
    }
}

#[cfg(test)]
mod tab_lifecycle_tests {
    use super::{ProcessExit, Status, TerminalTabId, input_allowed};
    use gpui::{AppContext, Entity, TestAppContext, VisualTestContext};

    fn fixture(cx: &mut TestAppContext) -> (Entity<crate::Notes>, &mut VisualTestContext) {
        cx.update(gpui_component::init);
        let mut owner = None;
        let (_, cx) = cx.add_window_view(|window, cx| {
            let notes = cx.new(|cx| crate::Notes::new(window, cx));
            owner = Some(notes.clone());
            gpui_component::Root::new(notes, window, cx)
        });
        let owner = owner.unwrap();
        cx.update(|window, cx| {
            owner.update(cx, |notes, cx| {
                notes.new_terminal_tab(window, cx);
                notes.new_terminal_tab(window, cx);
            })
        });
        (owner, cx)
    }

    #[gpui::test]
    fn reported_titles_update_hidden_tabs_reset_and_reject_stale_views(cx: &mut TestAppContext) {
        let (owner, cx) = fixture(cx);
        let sessions = cx.update(|_, cx| {
            owner
                .read(cx)
                .terminals
                .tabs
                .iter()
                .map(|tab| tab.session.clone())
                .collect::<Vec<_>>()
        });
        let send = |index: usize, bytes: &[u8], cx: &mut VisualTestContext| {
            cx.update(|_, cx| {
                sessions[index]
                    .read(cx)
                    .fixture_output
                    .as_ref()
                    .unwrap()
                    .send(bytes.to_vec())
                    .unwrap()
            });
            cx.run_until_parked();
            cx.background_executor
                .advance_clock(std::time::Duration::from_millis(10));
            cx.run_until_parked();
        };
        send(0, b"\x1b]0;npm run", cx);
        cx.update(|_, cx| {
            assert_eq!(
                owner.read(cx).terminals.tabs[0].title,
                super::shell_label(Default::default())
            )
        });
        send(0, b" dev\x07", cx);
        cx.update(|window, cx| {
            let notes = owner.read(cx);
            assert_eq!(notes.terminals.tabs[0].title, "npm run dev");
            assert_eq!(notes.terminals.active, Some(TerminalTabId(2)));
            assert!(
                sessions[1]
                    .read(cx)
                    .view
                    .as_ref()
                    .unwrap()
                    .read(cx)
                    .focus_handle()
                    .is_focused(window)
            );
            owner.update(cx, |notes, cx| notes.toggle_right_panel(window, cx));
        });
        send(1, b"\x1b]2;Claude session\x1b\\", cx);
        cx.update(|_, cx| assert_eq!(owner.read(cx).terminals.tabs[1].title, "Claude session"));
        send(0, b"\x1b]2;\x07", cx);
        cx.update(|_, cx| {
            assert_eq!(
                owner.read(cx).terminals.tabs[0].title,
                super::shell_label(Default::default())
            )
        });
        let old_view = cx.update(|_, cx| sessions[0].read(cx).view.clone().unwrap());
        cx.update(|window, cx| {
            sessions[0].update(cx, |session, cx| {
                session.start(Default::default(), Ok(std::env::temp_dir()), window, cx)
            })
        });
        cx.update(|_, cx| {
            old_view.update(cx, |_, cx| {
                cx.emit(gpui_terminal::TerminalTitleChanged("stale".into()))
            })
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            assert_ne!(owner.read(cx).terminals.tabs[0].title, "stale");
            owner.update(cx, |notes, cx| {
                notes.close_terminal_tab(TerminalTabId(1), window, cx)
            });
        });
        cx.update(|_, cx| {
            old_view.update(cx, |_, cx| {
                cx.emit(gpui_terminal::TerminalTitleChanged("closed".into()))
            })
        });
        cx.run_until_parked();
        cx.update(|_, cx| {
            let notes = owner.read(cx);
            assert_eq!(notes.terminals.tabs.len(), 1);
            assert_eq!(notes.terminals.tabs[0].title, "Claude session");
        });
    }

    #[gpui::test]
    fn terminal_background_output_and_preferences_survive_switching_and_hiding(
        cx: &mut TestAppContext,
    ) {
        let (owner, cx) = fixture(cx);
        let sessions = cx.update(|_, cx| {
            owner
                .read(cx)
                .terminals
                .tabs
                .iter()
                .map(|tab| tab.session.clone())
                .collect::<Vec<_>>()
        });
        let views = cx.update(|_, cx| {
            sessions
                .iter()
                .map(|session| session.read(cx).view.clone().unwrap())
                .collect::<Vec<_>>()
        });
        cx.update(|window, cx| {
            owner.update(cx, |notes, cx| {
                notes.toggle_right_panel(window, cx);
                notes.session_terminal.cursor_style = mithril::terminal::CursorStyle::Underline;
                notes.sync_terminal(window, cx);
            })
        });
        for (index, session) in sessions.iter().enumerate() {
            cx.update(|_, cx| {
                let session = session.read(cx);
                session
                    .fixture_output
                    .as_ref()
                    .unwrap()
                    .send(if index == 0 {
                        b"\x1b[2J\x1b[Hfirst\x1b[6n".to_vec()
                    } else {
                        b"\x1b[2J\x1b[Hsecond\x1b[6n".to_vec()
                    })
                    .unwrap();
            });
        }
        cx.run_until_parked();
        cx.update(|window, cx| {
            for (index, session) in sessions.iter().enumerate() {
                let session = session.read(cx);
                assert_eq!(session.view.as_ref(), Some(&views[index]));
                assert_eq!(
                    views[index].read(cx).config().cursor_style,
                    gpui_terminal::CursorStyle::Underline
                );
                assert_eq!(
                    *session.fixture_input.lock().unwrap(),
                    if index == 0 {
                        b"\x1b[1;6R"
                    } else {
                        b"\x1b[1;7R"
                    }
                );
                assert!(!input_allowed(
                    &owner.downgrade(),
                    TerminalTabId(index as u64 + 1),
                    window,
                    cx
                ));
            }
            owner.update(cx, |notes, cx| notes.toggle_right_panel(window, cx));
            assert!(input_allowed(
                &owner.downgrade(),
                TerminalTabId(2),
                window,
                cx
            ));
            assert!(!input_allowed(
                &owner.downgrade(),
                TerminalTabId(1),
                window,
                cx
            ));
            owner.update(cx, |notes, cx| {
                notes.select_terminal_tab(TerminalTabId(1), window, cx)
            });
            assert!(input_allowed(
                &owner.downgrade(),
                TerminalTabId(1),
                window,
                cx
            ));
            assert!(!input_allowed(
                &owner.downgrade(),
                TerminalTabId(2),
                window,
                cx
            ));
            assert!(views[0].read(cx).focus_handle().is_focused(window));
        });
    }

    #[gpui::test]
    fn terminal_close_during_launch_and_window_shutdown_join_all_cleanup(cx: &mut TestAppContext) {
        let (owner, cx) = fixture(cx);
        let sessions = cx.update(|_, cx| {
            owner
                .read(cx)
                .terminals
                .tabs
                .iter()
                .map(|tab| tab.session.clone())
                .collect::<Vec<_>>()
        });
        let (release_first, first) = flume::bounded::<()>(1);
        let (release_second, second) = flume::bounded::<()>(1);
        let (finished_tx, finished_rx) = flume::bounded::<()>(1);
        cx.update(|window, cx| {
            for (session, release) in sessions.iter().zip([first, second]) {
                session.update(cx, |session, cx| {
                    session.status = Status::Starting;
                    let generation = session.generation;
                    session.task = Some(cx.spawn_in(window, async move |this, cx| {
                        release.recv_async().await.unwrap();
                        let _ = this.update_in(cx, |session, window, cx| {
                            // A late result must not replace shutdown status or take focus.
                            session.install(generation, Err("late launch".into()), window, cx);
                        });
                    }));
                });
            }
            owner.update(cx, |notes, cx| {
                notes.close_terminal_tab(TerminalTabId(1), window, cx)
            });
            assert_eq!(owner.read(cx).terminals.tabs.len(), 1);
            assert_eq!(owner.read(cx).terminals.cleanup.len(), 1);
            let barrier = owner.update(cx, |notes, cx| notes.shutdown_terminals(cx));
            cx.spawn(async move |_| {
                barrier.await;
                finished_tx.send(()).unwrap();
            })
            .detach();
        });
        cx.run_until_parked();
        assert!(finished_rx.try_recv().is_err());
        // The still-open tab completing does not release the already-closed tab's barrier.
        release_second.send(()).unwrap();
        cx.run_until_parked();
        assert!(finished_rx.try_recv().is_err());
        release_first.send(()).unwrap();
        cx.run_until_parked();
        assert_eq!(finished_rx.try_recv(), Ok(()));
        cx.update(|_, cx| {
            assert!(owner.read(cx).terminals.cleanup.is_empty());
            for session in sessions {
                assert_eq!(
                    session.read(cx).status,
                    Status::Finished(ProcessExit::Stopped)
                );
            }
        });
    }

    #[gpui::test]
    fn terminal_exit_and_failure_remain_until_closed_and_cannot_affect_other_tabs(
        cx: &mut TestAppContext,
    ) {
        let (owner, cx) = fixture(cx);
        let sessions = cx.update(|_, cx| {
            owner
                .read(cx)
                .terminals
                .tabs
                .iter()
                .map(|tab| tab.session.clone())
                .collect::<Vec<_>>()
        });
        cx.update(|window, cx| {
            sessions[0].update(cx, |session, cx| {
                session.status = Status::Finished(ProcessExit::Exited(7));
                cx.notify();
            });
            sessions[1].update(cx, |session, cx| {
                session.install(session.generation, Err("missing shell".into()), window, cx)
            });
            owner.update(cx, |notes, cx| notes.toggle_right_panel(window, cx));
            owner.update(cx, |notes, cx| notes.toggle_right_panel(window, cx));
            assert_eq!(owner.read(cx).terminals.tabs.len(), 2);
            assert_eq!(
                sessions[0].read(cx).message().as_deref(),
                Some("Shell exited with status 7.")
            );
            assert_eq!(
                sessions[1].read(cx).message().as_deref(),
                Some("Terminal: missing shell")
            );
            assert!(sessions[0].read(cx).view.is_some());
            owner.update(cx, |notes, cx| notes.new_terminal_tab(window, cx));
            assert_eq!(owner.read(cx).terminals.active, Some(TerminalTabId(3)));
            assert_eq!(
                sessions[0].read(cx).status,
                Status::Finished(ProcessExit::Exited(7))
            );
        });
    }
}

#[cfg(test)]
mod clipboard_tests {
    use super::{ProcessExit, Status};
    use gpui::{
        AppContext, Modifiers, MouseButton, TestAppContext, VisualTestContext, point, px, size,
    };
    use gpui_component::WindowExt;

    #[gpui::test]
    fn terminal_context_menu_copy_paste_and_stale_authority(cx: &mut TestAppContext) {
        cx.update(gpui_component::init);
        let mut owner = None;
        let (_, cx) = cx.add_window_view(|window, cx| {
            let notes = cx.new(|cx| crate::Notes::new(window, cx));
            owner = Some(notes.clone());
            gpui_component::Root::new(notes, window, cx)
        });
        let owner = owner.unwrap();
        cx.simulate_resize(size(px(1200.), px(800.)));
        let (session, view, tx, input, document) = cx.update(|window, cx| {
            owner.update(cx, |notes, cx| notes.toggle_right_panel(window, cx));
            let session = owner.read(cx).terminals.active_session().unwrap().clone();
            let panel = session.read(cx);
            (
                session.clone(),
                panel.view.clone().unwrap(),
                panel.fixture_output.clone().unwrap(),
                panel.fixture_input.clone(),
                owner.read(cx).tabs.active.editor.text(cx),
            )
        });
        tx.send(b"terminal text".to_vec()).unwrap();
        cx.run_until_parked();
        cx.refresh().unwrap();
        let geometry = cx.update(|_, cx| view.read(cx).grid_geometry().unwrap());
        let start = geometry.origin + point(geometry.cell_width * 0.1, geometry.cell_height * 0.5);
        let end = geometry.origin + point(geometry.cell_width * 7.9, geometry.cell_height * 0.5);
        cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_move(end, Some(MouseButton::Left), Modifiers::default());
        cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::default());
        cx.update(|_, cx| assert_eq!(view.read(cx).selected_text().as_deref(), Some("terminal")));
        // Open through the actual pointer handler and invoke Copy through menu navigation.
        cx.simulate_mouse_down(end, MouseButton::Right, Modifiers::default());
        cx.simulate_mouse_up(end, MouseButton::Right, Modifiers::default());
        cx.refresh().unwrap();
        assert!(cx.debug_bounds("terminal-context-menu").is_some());
        cx.simulate_keystrokes("down enter");
        cx.run_until_parked();
        cx.update(|window, cx| {
            assert_eq!(
                cx.read_from_clipboard().unwrap().text().as_deref(),
                Some("terminal")
            );
            assert!(!view.read(cx).has_selection());
            assert!(view.read(cx).focus_handle().is_focused(window));
        });
        assert!(input.lock().unwrap().is_empty());
        // Copy is now disabled; selecting the final menu item pastes into this session.
        cx.simulate_mouse_down(end, MouseButton::Right, Modifiers::default());
        cx.simulate_mouse_up(end, MouseButton::Right, Modifiers::default());
        cx.simulate_keystrokes("up enter");
        cx.run_until_parked();
        assert_eq!(*input.lock().unwrap(), b"terminal");
        input.lock().unwrap().clear();

        let open = |cx: &mut VisualTestContext| {
            cx.update(|window, cx| {
                session.update(cx, |panel, cx| {
                    panel.open_context_menu(end, window, cx);
                    (
                        panel.generation,
                        panel.context_menu.as_ref().unwrap().1.entity_id(),
                    )
                })
            })
        };
        let first = open(cx);
        let current = open(cx);
        // A callback from a replaced menu cannot act through the newer popup.
        cx.update(|window, cx| {
            session.update(cx, |panel, cx| {
                panel.clipboard_action(&view, first, false, window, cx)
            })
        });
        assert!(input.lock().unwrap().is_empty());
        // Switching tabs invalidates the old target immediately, before repaint.
        cx.update(|window, cx| owner.update(cx, |notes, cx| notes.new_terminal_tab(window, cx)));
        cx.update(|window, cx| {
            session.update(cx, |panel, cx| {
                panel.clipboard_action(&view, current, false, window, cx)
            })
        });
        assert!(input.lock().unwrap().is_empty());
        let id = cx.update(|_, cx| session.read(cx).id);
        cx.update(|window, cx| {
            owner.update(cx, |notes, cx| notes.select_terminal_tab(id, window, cx))
        });
        let current = open(cx);
        cx.update(|window, cx| owner.update(cx, |notes, cx| notes.open_settings(window, cx)));
        cx.update(|window, cx| {
            session.update(cx, |panel, cx| {
                panel.clipboard_action(&view, current, false, window, cx);
                view.update(cx, |view, cx| {
                    view.copy_selection(window, cx);
                    view.paste_from_clipboard(window, cx);
                });
            })
        });
        assert!(input.lock().unwrap().is_empty());
        cx.update(|window, cx| window.close_dialog(cx));
        cx.run_until_parked();
        let current = open(cx);
        cx.update(|_, cx| {
            session.update(cx, |panel, cx| {
                panel.status = Status::Finished(ProcessExit::Stopped);
                view.update(cx, |view, _| view.set_input_enabled(false));
            })
        });
        cx.update(|window, cx| {
            session.update(cx, |panel, cx| {
                panel.clipboard_action(&view, current, false, window, cx)
            })
        });
        assert!(input.lock().unwrap().is_empty());
        let current = open(cx);
        cx.update(|window, cx| {
            owner.update(cx, |notes, cx| notes.close_terminal_tab(id, window, cx))
        });
        cx.update(|window, cx| {
            session.update(cx, |panel, cx| {
                panel.clipboard_action(&view, current, false, window, cx)
            })
        });
        assert!(input.lock().unwrap().is_empty());
        cx.update(|_, cx| assert_eq!(owner.read(cx).tabs.active.editor.text(cx), document));
    }
}
