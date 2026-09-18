//! Real PTYs, never native application windows.
use gpui_terminal::{
    event::{GpuiEventProxy, TerminalEvent},
    terminal::TerminalState,
};
use mithril::terminal::*;
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
            "if ((Get-Location).Path -eq '{cwd}' -and $Host.UI.RawUI.WindowSize.Width -eq 120 -and $Host.UI.RawUI.WindowSize.Height -eq 40) {{ Write-Output ('MITHRIL_' + 'READY') }}\r"
        )
    } else {
        // Git Bash represents Windows paths using /c/...; validate the directory
        // through a file created there instead of comparing platform spellings.
        std::fs::write(directory.path().join("mithril-cwd-probe"), "").unwrap();
        "HISTFILE=/dev/null; test -f mithril-cwd-probe && test \"$(stty size)\" = '40 120' && printf 'MITHRIL_%s\\n' READY\r".into()
    };
    input.write_all(command.as_bytes()).unwrap();
    wait_for(&process, "MITHRIL_READY");
    // A foreground shell builtin must be interruptible without killing the shell.
    input
        .write_all(if cfg!(windows) && !bash {
            b"Write-Output ('MITHRIL_' + 'BUSY'); Start-Sleep -Seconds 30\r"
        } else {
            b"printf 'MITHRIL_%s\\n' BUSY; sleep 30\r"
        })
        .unwrap();
    wait_for(&process, "MITHRIL_BUSY");
    // Let the external foreground job acquire the terminal before interrupting.
    std::thread::sleep(Duration::from_millis(300));
    input.write_all(b"\x03").unwrap();
    // The shell flushes pending console input while handling an interrupt.
    std::thread::sleep(Duration::from_millis(300));
    input
        .write_all(if cfg!(windows) && !bash {
            b"Write-Output ('MITHRIL_' + 'ALIVE')\r"
        } else {
            b"printf 'MITHRIL_%s\\n' ALIVE\r"
        })
        .unwrap();
    wait_for(&process, "MITHRIL_ALIVE");
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
    let settings = mithril::notebook::Settings::default();
    let mut json = serde_json::to_value(&settings).unwrap();
    json.as_object_mut().unwrap().remove("terminal");
    let restored: mithril::notebook::Settings = serde_json::from_value(json).unwrap();
    assert_eq!(restored.terminal.windows_shell, WindowsShell::Powershell);
    let settings = TerminalSettings {
        windows_shell: WindowsShell::GitBash,
    };
    assert_eq!(
        serde_json::to_string(&settings).unwrap(),
        r#"{"windows_shell":"git_bash"}"#
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
    use gpui::AppContext;
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
