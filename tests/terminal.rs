//! Real PTYs, never native application windows.
use gpui::AppContext;
use gpui_terminal::{
    event::{GpuiEventProxy, TerminalEvent},
    terminal::TerminalState,
};
use markdown_note::terminal::*;
use std::{
    io::Write,
    time::{Duration, Instant},
};

fn wait_for(process: &TerminalProcess, needle: &str) {
    let deadline = Instant::now() + Duration::from_secs(20);
    let (tx, events) = std::sync::mpsc::channel();
    let mut terminal = TerminalState::new(120, 40, GpuiEventProxy::new(tx));
    let mut input = process.input.clone();
    let mut seen = String::new();
    while Instant::now() < deadline {
        if let Ok(bytes) = process.output.recv_timeout(Duration::from_millis(100)) {
            terminal.process_bytes(&bytes);
            for event in events.try_iter() {
                if let TerminalEvent::Write(reply) = event {
                    input.write_all(reply.as_bytes()).unwrap();
                }
            }
            seen = terminal.with_term(|term| {
                term.grid()
                    .display_iter()
                    .map(|indexed| indexed.cell.c)
                    .collect::<String>()
            });
            if seen.contains(needle) {
                return;
            }
        }
    }
    panic!(
        "terminal did not produce {needle:?}; exit: {:?}; screen: {seen:?}",
        process.exit.try_recv()
    );
}

fn exercise(settings: TerminalSettings) {
    let directory = tempfile::tempdir().unwrap();
    let spec = launch_spec(settings, directory.path().to_path_buf()).unwrap();
    let bash = spec
        .program
        .file_stem()
        .unwrap()
        .to_string_lossy()
        .contains("bash");
    let mut spec = spec;
    if cfg!(windows)
        && !spec
            .program
            .file_stem()
            .unwrap()
            .to_string_lossy()
            .contains("bash")
    {
        // Keep tests out of the user's profile and persistent command history.
        spec.args = [
            "-NoLogo",
            "-NoProfile",
            "-NoExit",
            "-Command",
            "Set-PSReadLineOption -HistorySaveStyle SaveNothing",
        ]
        .map(str::to_owned)
        .to_vec();
    }
    let process = TerminalProcess::spawn(spec).unwrap();
    process.control.resize(120, 40);
    let mut input = process.input.clone();
    let cwd = directory.path().to_string_lossy().replace('\'', "''");
    let command = if cfg!(windows) && !bash {
        format!(
            "if ((Get-Location).Path -eq '{cwd}' -and $Host.UI.RawUI.WindowSize.Width -eq 120 -and $Host.UI.RawUI.WindowSize.Height -eq 40) {{ Write-Output ('MARKDOWN_NOTE_' + 'READY') }}\r"
        )
    } else {
        // Git Bash represents Windows paths using /c/...; validate the directory
        // through a file created there instead of comparing platform spellings.
        std::fs::write(directory.path().join("markdown-note-cwd-probe"), "").unwrap();
        "HISTFILE=/dev/null; test -f markdown-note-cwd-probe && test \"$(stty size)\" = '40 120' && printf 'MARKDOWN_NOTE_%s\\n' READY\r".into()
    };
    input.write_all(command.as_bytes()).unwrap();
    wait_for(&process, "MARKDOWN_NOTE_READY");
    // A foreground shell builtin must be interruptible without killing the shell.
    input
        .write_all(if cfg!(windows) && !bash {
            b"Write-Output ('MARKDOWN_NOTE_' + 'BUSY'); Start-Sleep -Seconds 30\r"
        } else {
            b"printf 'MARKDOWN_NOTE_%s\\n' BUSY; sleep 30\r"
        })
        .unwrap();
    wait_for(&process, "MARKDOWN_NOTE_BUSY");
    // Let the external foreground job acquire the terminal before interrupting.
    std::thread::sleep(Duration::from_millis(300));
    input.write_all(b"\x03").unwrap();
    // The shell flushes pending console input while handling an interrupt.
    std::thread::sleep(Duration::from_millis(300));
    input
        .write_all(if cfg!(windows) && !bash {
            b"Write-Output ('MARKDOWN_NOTE_' + 'ALIVE')\r"
        } else {
            b"printf 'MARKDOWN_NOTE_%s\\n' ALIVE\r"
        })
        .unwrap();
    wait_for(&process, "MARKDOWN_NOTE_ALIVE");
    input.write_all(b"exit 7\r").unwrap();
    assert_eq!(
        process.exit.recv_timeout(Duration::from_secs(15)).unwrap(),
        ProcessExit::Exited(7)
    );
    assert!(
        process
            .done
            .recv_timeout(Duration::from_secs(5))
            .unwrap_err()
            == flume::RecvTimeoutError::Disconnected
    );
}

#[test]
fn terminal_default_shell_roundtrip_cwd_resize_interrupt_and_exit() {
    exercise(TerminalSettings::default());
}

#[cfg(windows)]
#[test]
fn terminal_git_bash_roundtrip_when_installed() {
    if git_bash().is_none() {
        eprintln!("Git Bash is not installed; its optional PTY scenario was skipped.");
        return;
    }
    exercise(TerminalSettings {
        windows_shell: WindowsShell::GitBash,
        ..TerminalSettings::default()
    });
}

#[test]
fn terminal_stop_reaps_and_broadcasts_completion() {
    let directory = tempfile::tempdir().unwrap();
    let spec = launch_spec(TerminalSettings::default(), directory.path().into()).unwrap();
    let mut spec = spec;
    if cfg!(windows)
        && !spec
            .program
            .file_stem()
            .unwrap()
            .to_string_lossy()
            .contains("bash")
    {
        // Keep tests out of the user's profile and persistent command history.
        spec.args = [
            "-NoLogo",
            "-NoProfile",
            "-NoExit",
            "-Command",
            "Set-PSReadLineOption -HistorySaveStyle SaveNothing",
        ]
        .map(str::to_owned)
        .to_vec();
    }
    let process = TerminalProcess::spawn(spec).unwrap();
    let done1 = process.done.clone();
    let done2 = process.done.clone();
    process.control.stop();
    assert_eq!(
        process.exit.recv_timeout(Duration::from_secs(15)).unwrap(),
        ProcessExit::Stopped
    );
    assert!(
        done1.recv_timeout(Duration::from_secs(5)).unwrap_err()
            == flume::RecvTimeoutError::Disconnected
    );
    assert!(
        done2.recv_timeout(Duration::from_secs(5)).unwrap_err()
            == flume::RecvTimeoutError::Disconnected
    );
}

#[test]
fn terminal_drop_stops_without_a_view_or_output_consumer() {
    let directory = tempfile::tempdir().unwrap();
    let process = TerminalProcess::spawn(
        launch_spec(TerminalSettings::default(), directory.path().into()).unwrap(),
    )
    .unwrap();
    let done = process.done.clone();
    drop(process);
    assert!(
        done.recv_timeout(Duration::from_secs(15)).unwrap_err()
            == flume::RecvTimeoutError::Disconnected
    );
}

#[test]
fn terminal_launch_failure_and_old_settings_defaults() {
    let directory = tempfile::tempdir().unwrap();
    assert!(
        TerminalProcess::spawn(LaunchSpec {
            program: directory.path().join("missing-shell"),
            args: vec![],
            cwd: directory.path().into()
        })
        .is_err()
    );
    let settings = markdown_note::notebook::Settings::default();
    let mut json = serde_json::to_value(&settings).unwrap();
    json.as_object_mut().unwrap().remove("terminal");
    let restored: markdown_note::notebook::Settings = serde_json::from_value(json).unwrap();
    assert_eq!(restored.terminal.windows_shell, WindowsShell::Powershell);
    assert_eq!(restored.terminal.cursor_style, CursorStyle::Line);
    let settings = TerminalSettings {
        windows_shell: WindowsShell::GitBash,
        ..TerminalSettings::default()
    };
    assert_eq!(
        serde_json::to_string(&settings).unwrap(),
        r#"{"windows_shell":"git_bash","cursor_style":"line"}"#
    );
    assert_eq!(
        serde_json::from_str::<TerminalSettings>(r#"{"windows_shell":"git_bash"}"#).unwrap(),
        settings
    );
    assert_eq!(
        initial_directory(
            Some(directory.path()),
            Some(std::path::Path::new("/other/file"))
        )
        .unwrap(),
        directory.path()
    );
}

#[test]
fn terminal_ansi_unicode_alternate_screen_and_protocol_replies() {
    let (tx, rx) = std::sync::mpsc::channel();
    let mut terminal = TerminalState::new(80, 24, GpuiEventProxy::new(tx));
    for bytes in "hello \u{8336}\x1b[31m red\x1b[0m".as_bytes().chunks(1) {
        terminal.process_bytes(bytes);
    }
    let screen = |terminal: &TerminalState| {
        terminal.with_term(|term| {
            term.grid()
                .display_iter()
                .map(|indexed| indexed.cell.c)
                .collect::<String>()
        })
    };
    assert!(screen(&terminal).contains("hello \u{8336}"));
    terminal.process_bytes(b"\x1b[?1049hother screen");
    assert!(screen(&terminal).contains("other screen"));
    terminal.process_bytes(b"\x1b[?1049l");
    assert!(screen(&terminal).contains("hello"));
    terminal.process_bytes(b"\x1b[6n");
    assert!(
        rx.try_iter()
            .any(|event| matches!(event, TerminalEvent::Write(reply) if reply.ends_with('R')))
    );
    for _ in 0..10000 {
        terminal.process_bytes(b"burst\r\n");
    }
    terminal.process_bytes(b"END_OF_BURST");
    assert!(screen(&terminal).contains("END_OF_BURST"));
}

#[derive(Clone)]
struct RecordedInput(std::sync::Arc<std::sync::Mutex<Vec<u8>>>);
impl Write for RecordedInput {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[gpui::test]
fn terminal_view_paste_modes_unicode_input_and_disabled_input(cx: &mut gpui::TestAppContext) {
    use gpui_terminal::{TerminalConfig, TerminalView};
    cx.update(gpui_component::init);
    let recorded = RecordedInput(Default::default());
    let (tx, rx) = flume::bounded(64);
    let mut view = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let terminal = cx.new(|cx| {
            TerminalView::from_receiver(recorded.clone(), rx, TerminalConfig::default(), cx)
        });
        terminal.read(cx).focus_handle().focus(window);
        view = Some(terminal.clone());
        gpui_component::Root::new(terminal, window, cx)
    });
    let view = view.unwrap();
    cx.update(|_, cx| view.update(cx, |view, cx| view.paste("a\r\nb", cx)));
    assert_eq!(*recorded.0.lock().unwrap(), b"a\rb");
    recorded.0.lock().unwrap().clear();
    tx.send(b"\x1b[?2004h".to_vec()).unwrap();
    cx.run_until_parked();
    cx.update(|_, cx| view.update(cx, |view, cx| view.paste("茶\r\nx\x1b[201~", cx)));
    assert_eq!(
        *recorded.0.lock().unwrap(),
        "\x1b[200~茶\nx[201~\x1b[201~".as_bytes()
    );
    recorded.0.lock().unwrap().clear();
    cx.update(|_, cx| {
        cx.write_to_clipboard(gpui::ClipboardItem::new_string("pasted".into()));
    });
    cx.simulate_keystrokes(if cfg!(target_os = "macos") {
        "cmd-v"
    } else {
        "ctrl-shift-v"
    });
    cx.run_until_parked();
    assert_eq!(*recorded.0.lock().unwrap(), b"\x1b[200~pasted\x1b[201~");
    recorded.0.lock().unwrap().clear();
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.set_input_enabled(false);
            view.paste("blocked", cx);
            let before = view.dimensions();
            view.resize(0, 0);
            assert_eq!(view.dimensions(), before);
        })
    });
    cx.simulate_keystrokes("x ctrl-c");
    cx.run_until_parked();
    assert!(recorded.0.lock().unwrap().is_empty());
}

#[test]
fn terminal_cursor_settings_default_and_round_trip() {
    for json in ["{}", r#"{"windows_shell":"git_bash"}"#] {
        let settings: TerminalSettings = serde_json::from_str(json).unwrap();
        assert_eq!(settings.cursor_style, CursorStyle::Line);
    }
    for (cursor_style, name) in [
        (CursorStyle::Line, "line"),
        (CursorStyle::Box, "box"),
        (CursorStyle::Underline, "underline"),
    ] {
        let settings = TerminalSettings {
            cursor_style,
            ..Default::default()
        };
        let json = serde_json::to_value(&settings).unwrap();
        assert_eq!(json["cursor_style"], name);
        assert_eq!(
            serde_json::from_value::<TerminalSettings>(json).unwrap(),
            settings
        );
    }
}

#[test]
fn terminal_cursor_shapes_preserve_visibility_and_cell_contents() {
    use gpui::{Edges, point, px, size};
    use gpui_terminal::{ColorPalette, CursorStyle, TerminalRenderer};
    let (tx, _) = std::sync::mpsc::channel();
    let mut terminal = TerminalState::new(80, 24, GpuiEventProxy::new(tx));
    terminal.process_bytes(b"hello\x1b[2;3H");
    let mut renderer =
        TerminalRenderer::new("monospace".into(), px(14.), 1.2, ColorPalette::default());
    renderer.cell_width = px(10.);
    renderer.cell_height = px(20.);
    for (style, origin, dimensions, border) in [
        (
            CursorStyle::Line,
            point(px(24.), px(26.)),
            size(px(1.), px(20.)),
            px(0.),
        ),
        (
            CursorStyle::Box,
            point(px(24.), px(26.)),
            size(px(10.), px(20.)),
            px(1.),
        ),
        (
            CursorStyle::Underline,
            point(px(24.), px(45.)),
            size(px(10.), px(1.)),
            px(0.),
        ),
    ] {
        renderer.cursor_style = style;
        // Application shape requests do not override the user's selection.
        terminal.process_bytes(b"\x1b[2 q\x1b[?25h");
        terminal.with_term(|term| {
            let cursor = renderer.cursor_quad(point(px(4.), px(6.)), term).unwrap();
            assert_eq!(cursor.bounds.origin, origin);
            assert_eq!(cursor.bounds.size, dimensions);
            assert_eq!(cursor.border_widths, Edges::all(border));
            let text: String = term.grid().display_iter().map(|cell| cell.cell.c).collect();
            assert!(text.contains("hello"));
        });
        terminal.process_bytes(b"\x1b[?25l");
        terminal
            .with_term(|term| assert!(renderer.cursor_quad(point(px(4.), px(6.)), term).is_none()));
    }
}

#[test]
fn terminal_sessions_are_independent_and_closing_one_preserves_the_other() {
    let directory = tempfile::tempdir().unwrap();
    let mut spec = launch_spec(TerminalSettings::default(), directory.path().into()).unwrap();
    let powershell = cfg!(windows);
    if powershell {
        spec.args = [
            "-NoLogo",
            "-NoProfile",
            "-NoExit",
            "-Command",
            "Set-PSReadLineOption -HistorySaveStyle SaveNothing",
        ]
        .map(str::to_owned)
        .to_vec();
    }
    let first = TerminalProcess::spawn(spec.clone()).unwrap();
    let second = TerminalProcess::spawn(spec).unwrap();
    first
        .input
        .clone()
        .write_all(if powershell {
            b"$markdown_note_tab = 'FIRST'; Write-Output ('SESSION_' + $markdown_note_tab)\r"
        } else {
            b"HISTFILE=/dev/null; markdown_note_tab=FIRST; printf 'SESSION_%s\\n' $markdown_note_tab\r"
        })
        .unwrap();
    second
        .input
        .clone()
        .write_all(if powershell {
            b"$markdown_note_tab = 'SECOND'; Write-Output ('SESSION_' + $markdown_note_tab)\r"
        } else {
            b"HISTFILE=/dev/null; markdown_note_tab=SECOND; printf 'SESSION_%s\\n' $markdown_note_tab\r"
        })
        .unwrap();
    wait_for(&first, "SESSION_FIRST");
    wait_for(&second, "SESSION_SECOND");
    first.control.stop();
    assert_eq!(
        first.exit.recv_timeout(Duration::from_secs(15)).unwrap(),
        ProcessExit::Stopped
    );
    assert_eq!(
        first.done.recv_timeout(Duration::from_secs(5)),
        Err(flume::RecvTimeoutError::Disconnected)
    );
    second
        .input
        .clone()
        .write_all(if powershell {
            b"Write-Output ('SURVIVING_' + $markdown_note_tab)\r"
        } else {
            b"printf 'SURVIVING_%s\\n' $markdown_note_tab\r"
        })
        .unwrap();
    wait_for(&second, "SURVIVING_SECOND");
    let done = second.done.clone();
    drop(second);
    assert_eq!(
        done.recv_timeout(Duration::from_secs(15)),
        Err(flume::RecvTimeoutError::Disconnected)
    );
}

#[test]
fn terminal_font_settings_inherit_round_trip_and_clamp() {
    for json in [
        "{}",
        r#"{"windows_shell":"git_bash"}"#,
        r#"{"font_family":null,"font_size":null}"#,
    ] {
        let settings: TerminalSettings = serde_json::from_str(json).unwrap();
        assert_eq!(settings.font_family, None);
        assert_eq!(settings.font_size, None);
        assert_eq!(settings.effective_font_size(23), 23);
    }
    let settings = TerminalSettings {
        font_family: Some("Missing font kept for another computer".into()),
        font_size: Some(24),
        ..Default::default()
    };
    assert_eq!(settings.effective_font_size(16), 24);
    assert_eq!(
        serde_json::from_str::<TerminalSettings>(&serde_json::to_string(&settings).unwrap())
            .unwrap(),
        settings
    );
    for (size, expected) in [(0, 10), (9, 10), (16, 16), (40, 40), (65535, 40)] {
        let settings: TerminalSettings =
            serde_json::from_value(serde_json::json!({"font_size": size})).unwrap();
        assert_eq!(settings.font_size, Some(expected));
        assert_eq!(settings.effective_font_size(16), expected);
    }
}

#[gpui::test]
fn terminal_font_resize_updates_grid_and_pty_callback(cx: &mut gpui::TestAppContext) {
    use gpui_terminal::{TerminalConfig, TerminalView};
    cx.update(gpui_component::init);
    let recorded = RecordedInput(Default::default());
    let (_tx, rx) = flume::bounded(64);
    let sizes = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let mut view = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let terminal = cx.new(|cx| {
            let sizes = sizes.clone();
            TerminalView::from_receiver(
                recorded.clone(),
                rx,
                TerminalConfig {
                    font_size: gpui::px(12.),
                    ..Default::default()
                },
                cx,
            )
            .with_resize_callback(move |cols, rows| sizes.lock().unwrap().push((cols, rows)))
        });
        view = Some(terminal.clone());
        gpui_component::Root::new(terminal, window, cx)
    });
    let view = view.unwrap();
    cx.refresh().unwrap();
    let before = cx.update(|_, cx| view.read(cx).dimensions());
    sizes.lock().unwrap().clear();
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let mut config = view.config().clone();
            config.font_size = gpui::px(24.);
            view.update_config(config, cx);
        })
    });
    cx.refresh().unwrap();
    let after = cx.update(|_, cx| view.read(cx).dimensions());
    assert!(
        after.0 < before.0 && after.1 < before.1,
        "{before:?} -> {after:?}"
    );
    assert!(
        sizes
            .lock()
            .unwrap()
            .iter()
            .any(|&(cols, rows)| (cols, rows) == after)
    );
}

fn clipboard_fixture(
    cx: &mut gpui::TestAppContext,
) -> (
    gpui::Entity<gpui_terminal::TerminalView>,
    flume::Sender<Vec<u8>>,
    RecordedInput,
    &mut gpui::VisualTestContext,
) {
    cx.update(gpui_component::init);
    let recorded = RecordedInput(Default::default());
    let (tx, rx) = flume::bounded(64);
    let mut view = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let terminal = cx.new(|cx| {
            gpui_terminal::TerminalView::from_receiver(
                recorded.clone(),
                rx,
                gpui_terminal::TerminalConfig::default(),
                cx,
            )
        });
        terminal.read(cx).focus_handle().focus(window);
        view = Some(terminal.clone());
        gpui_component::Root::new(terminal, window, cx)
    });
    cx.simulate_resize(gpui::size(gpui::px(600.), gpui::px(300.)));
    cx.refresh().unwrap();
    (view.unwrap(), tx, recorded, cx)
}

fn drag_terminal(
    cx: &mut gpui::VisualTestContext,
    from: gpui::Point<gpui::Pixels>,
    to: gpui::Point<gpui::Pixels>,
) {
    cx.simulate_mouse_move(from, None, gpui::Modifiers::default());
    cx.simulate_mouse_down(from, gpui::MouseButton::Left, gpui::Modifiers::default());
    cx.simulate_mouse_move(
        to,
        Some(gpui::MouseButton::Left),
        gpui::Modifiers::default(),
    );
    cx.simulate_mouse_up(to, gpui::MouseButton::Left, gpui::Modifiers::default());
    cx.run_until_parked();
}

#[gpui::test]
fn terminal_clipboard_selection_shortcuts_and_exited_output(cx: &mut gpui::TestAppContext) {
    let (view, tx, input, cx) = clipboard_fixture(cx);
    tx.send(b"abc\r\ndef".to_vec()).unwrap();
    cx.run_until_parked();
    cx.refresh().unwrap();
    let geometry = cx.update(|_, cx| view.read(cx).grid_geometry().unwrap());
    let from = geometry.origin + gpui::point(geometry.cell_width * 0.1, geometry.cell_height * 0.5);
    let to = geometry.origin + gpui::point(geometry.cell_width * 2.9, geometry.cell_height * 1.5);
    for (start, end) in [(from, to), (to, from)] {
        drag_terminal(cx, start, end);
        cx.update(|_, cx| assert_eq!(view.read(cx).selected_text().as_deref(), Some("abc\ndef")));
        cx.simulate_keystrokes(if cfg!(target_os = "macos") {
            "cmd-c"
        } else {
            "ctrl-c"
        });
        cx.update(|_, cx| {
            assert_eq!(
                cx.read_from_clipboard().unwrap().text().as_deref(),
                Some("abc\ndef")
            );
            assert!(!view.read(cx).has_selection());
        });
        assert!(input.0.lock().unwrap().is_empty());
    }
    cx.simulate_keystrokes("ctrl-c");
    assert_eq!(*input.0.lock().unwrap(), b"\x03");
    input.0.lock().unwrap().clear();
    cx.simulate_keystrokes(if cfg!(target_os = "macos") {
        "cmd-c"
    } else {
        "ctrl-shift-c"
    });
    assert!(input.0.lock().unwrap().is_empty());
    cx.update(|_, cx| {
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("abc\ndef")
        )
    });
    drag_terminal(cx, from, to);
    drag_terminal(cx, from, from);
    cx.update(|_, cx| assert!(!view.read(cx).has_selection()));
    // Repeated host synchronization after exit must not cancel a new selection.
    cx.update(|_, cx| {
        view.update(cx, |view, _| view.set_input_enabled(false));
        cx.write_to_clipboard(gpui::ClipboardItem::new_string("before exited copy".into()));
    });
    cx.simulate_mouse_down(from, gpui::MouseButton::Left, gpui::Modifiers::default());
    cx.update(|_, cx| view.update(cx, |view, _| view.set_input_enabled(false)));
    cx.simulate_mouse_move(
        to,
        Some(gpui::MouseButton::Left),
        gpui::Modifiers::default(),
    );
    cx.simulate_mouse_up(to, gpui::MouseButton::Left, gpui::Modifiers::default());
    cx.update(|_, cx| assert_eq!(view.read(cx).selected_text().as_deref(), Some("abc\ndef")));
    cx.simulate_keystrokes(if cfg!(target_os = "macos") {
        "cmd-c cmd-v"
    } else {
        "ctrl-shift-c ctrl-v"
    });
    cx.update(|_, cx| {
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("abc\ndef")
        )
    });
    assert!(input.0.lock().unwrap().is_empty());
}

#[gpui::test]
fn terminal_clipboard_paste_aliases_guard_and_measured_geometry(cx: &mut gpui::TestAppContext) {
    let (view, tx, input, cx) = clipboard_fixture(cx);
    tx.send(b"\x1b[?2004hhello".to_vec()).unwrap();
    cx.run_until_parked();
    cx.update(|_, cx| {
        cx.write_to_clipboard(gpui::ClipboardItem::new_string("茶\r\nx\x1b[201~".into()))
    });
    for key in if cfg!(target_os = "macos") {
        vec!["cmd-v"]
    } else {
        vec!["ctrl-v", "ctrl-shift-v"]
    } {
        cx.simulate_keystrokes(key);
        assert_eq!(
            *input.0.lock().unwrap(),
            "\x1b[200~茶\nx[201~\x1b[201~".as_bytes()
        );
        input.0.lock().unwrap().clear();
    }
    for size in [14., 24.] {
        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                let mut config = view.config().clone();
                config.font_size = gpui::px(size);
                config.padding = gpui::Edges::all(gpui::px(7.));
                view.update_config(config, cx);
            })
        });
        cx.simulate_resize(gpui::size(gpui::px(600. + size), gpui::px(300.)));
        cx.refresh().unwrap();
        let geometry = cx.update(|_, cx| view.read(cx).grid_geometry().unwrap());
        let from =
            geometry.origin + gpui::point(geometry.cell_width * 0.1, geometry.cell_height * 0.5);
        let to =
            geometry.origin + gpui::point(geometry.cell_width * 4.9, geometry.cell_height * 0.5);
        drag_terminal(cx, from, to);
        cx.update(|_, cx| assert_eq!(view.read(cx).selected_text().as_deref(), Some("hello")));
        // Releasing outside the grid must end dragging and clamp to the final cell.
        drag_terminal(cx, from, gpui::point(gpui::px(900.), gpui::px(900.)));
        cx.update(|_, cx| assert!(view.read(cx).selected_text().unwrap().starts_with("hello")));
        cx.simulate_mouse_move(from, None, gpui::Modifiers::default());
    }
    // Host authority is checked by both explicit clipboard entry points.
    cx.update(|window, cx| {
        let blocked = cx.new(|cx| {
            gpui_terminal::TerminalView::from_receiver(
                input.clone(),
                flume::bounded(1).1,
                Default::default(),
                cx,
            )
            .with_input_guard(|_, _| false)
        });
        blocked.update(cx, |view, cx| {
            assert!(!view.interaction_allowed(window, cx));
            view.paste_from_clipboard(window, cx);
            view.copy_selection(window, cx);
        });
    });
    assert!(input.0.lock().unwrap().is_empty());
}

#[test]
fn terminal_selection_extracts_wrapped_unicode_and_highlight_bounds() {
    use alacritty_terminal::{
        index::{Column, Line, Point, Side},
        selection::{Selection, SelectionType},
    };
    use gpui_terminal::render::{GridGeometry, TerminalRenderer};
    let (tx, _) = std::sync::mpsc::channel();
    let mut state = TerminalState::new(5, 3, GpuiEventProxy::new(tx));
    state.process_bytes("abcde茶e\u{301}".as_bytes());
    state.with_term_mut(|term| {
        let mut selection = Selection::new(
            SelectionType::Simple,
            Point::new(Line(0), Column(0)),
            Side::Left,
        );
        selection.update(Point::new(Line(1), Column(2)), Side::Right);
        term.selection = Some(selection);
        assert_eq!(
            term.selection_to_string().as_deref(),
            Some("abcde茶e\u{301}")
        );
        let mut renderer =
            TerminalRenderer::new("monospace".into(), gpui::px(14.), 1.2, Default::default());
        for cell_size in [10., 20.] {
            renderer.cell_width = gpui::px(cell_size);
            renderer.cell_height = gpui::px(cell_size * 2.);
            let origin = gpui::point(gpui::px(7.), gpui::px(9.));
            let rects = renderer.selection_bounds(origin, term);
            assert_eq!(rects.len(), 2);
            assert_eq!(rects[0].origin, origin);
            assert_eq!(rects[0].size.width, gpui::px(cell_size * 5.));
            assert_eq!(rects[1].origin.y, origin.y + renderer.cell_height);
            assert_eq!(rects[1].size.width, gpui::px(cell_size * 3.));
            let geometry = GridGeometry {
                origin,
                cell_width: renderer.cell_width,
                cell_height: renderer.cell_height,
                columns: 5,
                rows: 3,
            };
            assert_eq!(
                geometry.hit_test(gpui::point(gpui::px(-100.), gpui::px(-100.))),
                (Point::new(Line(0), Column(0)), Side::Left)
            );
            assert_eq!(
                geometry.hit_test(gpui::point(gpui::px(1000.), gpui::px(1000.))),
                (Point::new(Line(2), Column(4)), Side::Right)
            );
        }
    });
}
