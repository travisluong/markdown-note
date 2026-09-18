//! Retained terminal view and generation-guarded process installation.
use gpui::{prelude::*, *};
use gpui_component::{ActiveTheme, WindowExt};
use gpui_terminal::{ColorPalette, TerminalConfig, TerminalView};
use mithril::terminal::{ProcessExit, TerminalProcess, TerminalSettings};
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Status {
    Idle,
    Starting,
    Running,
    Stopping,
    Finished(ProcessExit),
}

pub(super) struct TerminalPanel {
    pub view: Option<Entity<TerminalView>>,
    owner: WeakEntity<crate::Notes>,
    pub status: Status,
    generation: u64,
    process: Option<TerminalProcess>,
    task: Option<Task<()>>,
    focus: FocusHandle,
    #[cfg(test)]
    fixture_output: Option<flume::Sender<Vec<u8>>>,
    #[cfg(test)]
    pub fixture_input: std::sync::Arc<std::sync::Mutex<Vec<u8>>>,
    #[cfg(test)]
    pub last_launch: Option<(TerminalSettings, Result<PathBuf, String>)>,
}

impl TerminalPanel {
    pub fn new(owner: WeakEntity<crate::Notes>, cx: &mut Context<Self>) -> Self {
        Self {
            owner,
            view: None,
            status: Status::Idle,
            generation: 0,
            process: None,
            task: None,
            focus: cx.focus_handle(),
            #[cfg(test)]
            fixture_output: None,
            #[cfg(test)]
            fixture_input: Default::default(),
            #[cfg(test)]
            last_launch: None,
        }
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
        let old = self.process.take();
        let pending = self.task.take();
        if let Some(process) = &old {
            process.control.stop();
        }
        self.status = Status::Starting;
        self.focus.focus(window);
        // Drop the old view only when replacing its output; Stop retains it.
        self.view = None;
        cx.notify();
        #[cfg(test)]
        {
            self.last_launch = Some((settings, directory));
            let _ = (generation, old, pending);
            let (tx, rx) = flume::bounded(64);
            self.fixture_output = Some(tx);
            let owner = self.owner.clone();
            self.view = Some(cx.new(|cx| {
                TerminalView::from_receiver(
                    FixtureWriter(self.fixture_input.clone()),
                    rx,
                    TerminalConfig::default(),
                    cx,
                )
                .with_input_guard(move |window, cx| input_allowed(&owner, window, cx))
            }));
            self.status = Status::Running;
            self.focus(window, cx);
        }
        #[cfg(not(test))]
        {
            self.task = Some(cx.spawn_in(window, async move |this, cx| {
                if let Some(pending) = pending {
                    pending.await;
                }
                // A restart never leaves two shells alive.
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
                self.view = Some(cx.new(|cx| {
                    TerminalView::from_receiver(input, output, TerminalConfig::default(), cx)
                        .with_resize_callback(move |cols, rows| resize.resize(cols, rows))
                        .with_input_guard(move |window, cx| input_allowed(&owner, window, cx))
                }));
                self.process = Some(process);
                self.status = Status::Running;
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

    pub fn stop(&mut self, cx: &mut Context<Self>) {
        if let Some(view) = &self.view {
            view.update(cx, |view, _| view.set_input_enabled(false));
        }
        if let Some(process) = &self.process {
            self.status = Status::Stopping;
            process.control.stop();
        } else {
            // Invalidate pending launches; their returned process is dropped.
            self.generation += 1;
            self.status = Status::Finished(ProcessExit::Stopped);
        }
        cx.notify();
    }

    /// The accepted close barrier awaits process cleanup before removing the window.
    pub fn shutdown(&mut self) -> (Option<Task<()>>, Option<flume::Receiver<()>>) {
        self.generation += 1;
        let done = self.process.as_ref().map(|process| {
            process.control.stop();
            process.done.clone()
        });
        (self.task.take(), done)
    }

    pub fn sync(
        &mut self,
        enabled: bool,
        appearance: &mithril::notebook::Appearance,
        startup_dark: bool,
        cx: &mut Context<Self>,
    ) {
        if let Some(view) = &self.view {
            let palette = appearance.palette(startup_dark);
            let rgb = |value: u32| ((value >> 16) as u8, (value >> 8) as u8, value as u8);
            let (br, bg, bb) = rgb(palette.background);
            let (fr, fg, fb) = rgb(palette.font);
            let font = appearance
                .monospace_font
                .clone()
                .unwrap_or_else(|| cx.theme().mono_font_family.to_string());
            view.update(cx, |view, cx| {
                view.set_input_enabled(enabled && self.status == Status::Running);
                let config = view.config();
                if config.font_family != font
                    || config.font_size != px(f32::from(appearance.content_font_size))
                    || config.colors.background() != gpui::rgb(palette.background).into()
                    || config.colors.foreground() != gpui::rgb(palette.font).into()
                {
                    let mut config = config.clone();
                    config.font_family = font;
                    config.font_size = px(f32::from(appearance.content_font_size));
                    config.padding = Edges::all(px(4.));
                    config.colors = ColorPalette::builder()
                        .background(br, bg, bb)
                        .foreground(fr, fg, fb)
                        .cursor(fr, fg, fb)
                        .build();
                    view.update_config(config, cx);
                }
            });
        }
    }

    pub fn message(&self) -> Option<String> {
        match &self.status {
            Status::Idle => None,
            Status::Starting => Some("Starting terminal…".into()),
            Status::Running => None,
            Status::Stopping => Some("Stopping terminal…".into()),
            Status::Finished(ProcessExit::Stopped) => Some("Terminal stopped.".into()),
            Status::Finished(ProcessExit::Exited(code)) => {
                Some(format!("Shell exited with status {code}."))
            }
            Status::Finished(ProcessExit::Failed(error)) => Some(format!("Terminal: {error}")),
        }
    }
}

impl Render for TerminalPanel {
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
            .children(
                self.view
                    .clone()
                    .map(|view| div().flex_1().min_h_0().overflow_hidden().child(view)),
            )
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
fn div_message(message: String, cx: &App) -> impl IntoElement {
    div()
        .p_2()
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child(message)
}

fn input_allowed(owner: &WeakEntity<crate::Notes>, window: &mut Window, cx: &mut App) -> bool {
    !window.has_active_dialog(cx)
        && owner.upgrade().is_some_and(|owner| {
            let notes = owner.read(cx);
            notes.right_panel_open && !notes.document_interaction_blocked()
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
            let panel = owner.read(cx).terminal.clone().unwrap();
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
