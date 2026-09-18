//! Shell policy and PTY lifetime. No GPUI or document state is owned here.
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use serde::{Deserialize, Serialize};
use std::{
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowsShell {
    #[default]
    Powershell,
    GitBash,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CursorStyle {
    #[default]
    Line,
    Box,
    Underline,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct TerminalSettings {
    pub windows_shell: WindowsShell,
    pub cursor_style: CursorStyle,
}

#[derive(Clone, Debug)]
pub struct LaunchSpec {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub cwd: PathBuf,
}

pub fn initial_directory(notebook: Option<&Path>, document: Option<&Path>) -> io::Result<PathBuf> {
    notebook
        .map(Path::to_path_buf)
        .or_else(|| document.and_then(Path::parent).map(Path::to_path_buf))
        .or_else(|| {
            std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from)
        })
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Cannot locate your home directory"))
}

#[cfg(windows)]
fn on_path(name: &str) -> Option<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
        .filter(|dir| dir.is_absolute())
        .map(|dir| dir.join(name))
        .find(|path| path.is_file())
}

#[cfg(windows)]
pub fn git_bash() -> Option<PathBuf> {
    use winreg::{
        RegKey,
        enums::{
            HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY,
        },
    };
    let mut roots = Vec::new();
    for hive in [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE] {
        for flags in [KEY_READ | KEY_WOW64_64KEY, KEY_READ | KEY_WOW64_32KEY] {
            if let Ok(key) =
                RegKey::predef(hive).open_subkey_with_flags("Software\\GitForWindows", flags)
                && let Ok(root) = key.get_value::<String, _>("InstallPath")
            {
                roots.push(PathBuf::from(root));
            }
        }
    }
    for name in ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"] {
        if let Some(base) = std::env::var_os(name) {
            let base = PathBuf::from(base);
            roots.push(if name == "LOCALAPPDATA" {
                base.join("Programs/Git")
            } else {
                base.join("Git")
            });
        }
    }
    if let Some(git) = on_path("git.exe") {
        roots.extend(git.ancestors().skip(1).take(3).map(Path::to_path_buf));
    }
    roots
        .into_iter()
        .map(|root| root.join("bin/bash.exe"))
        .find(|path| path.is_file())
}

pub fn launch_spec(settings: TerminalSettings, cwd: PathBuf) -> io::Result<LaunchSpec> {
    #[cfg(windows)]
    let (program, args) = match settings.windows_shell {
        WindowsShell::Powershell => {
            let program = on_path("pwsh.exe").or_else(|| {
                std::env::var_os("SystemRoot").map(|root| PathBuf::from(root).join("System32/WindowsPowerShell/v1.0/powershell.exe"))
                    .filter(|path| path.is_file())
            }).ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "PowerShell was not found. Install PowerShell or choose Git Bash in Settings."))?;
            (program, vec!["-NoLogo".into()])
        }
        WindowsShell::GitBash => (git_bash().ok_or_else(|| io::Error::new(io::ErrorKind::NotFound,
            "Git Bash was not found. Install Git for Windows in a standard location or choose PowerShell in Settings."))?, vec!["--login".into(), "-i".into()]),
    };
    #[cfg(not(windows))]
    let (program, args) = {
        let _ = settings;
        let program = std::env::var_os("SHELL")
            .map(PathBuf::from)
            .filter(|path| path.is_absolute() && path.is_file())
            .unwrap_or_else(|| "/bin/sh".into());
        (program, vec!["-i".into()])
    };
    Ok(LaunchSpec { program, args, cwd })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProcessExit {
    Exited(u32),
    Stopped,
    Failed(String),
}

#[derive(Clone)]
pub struct ProcessControl {
    stop: Arc<AtomicBool>,
    size: Arc<Mutex<Option<PtySize>>>,
}
impl ProcessControl {
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Release);
    }
    pub fn resize(&self, cols: usize, rows: usize) {
        if cols > 0 && rows > 0 {
            *self.size.lock().unwrap() = Some(PtySize {
                rows: rows.min(u16::MAX as usize) as u16,
                cols: cols.min(u16::MAX as usize) as u16,
                pixel_width: 0,
                pixel_height: 0,
            });
        }
    }
}

/// Nonblocking UI-side writer. Bounded input prevents stalled children retaining
/// unlimited pasted text. Callers surface WouldBlock instead of silently losing input.
#[derive(Clone)]
pub struct ProcessInput(flume::Sender<Vec<u8>>);
impl Write for ProcessInput {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > 1024 * 1024 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Paste is limited to 1 MiB",
            ));
        }
        self.0.try_send(bytes.to_vec()).map_err(|error| {
            io::Error::new(
                if matches!(error, flume::TrySendError::Full(_)) {
                    io::ErrorKind::WouldBlock
                } else {
                    io::ErrorKind::BrokenPipe
                },
                "Terminal input is busy or closed; try again",
            )
        })?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub struct TerminalProcess {
    pub input: ProcessInput,
    pub output: flume::Receiver<Vec<u8>>,
    pub exit: flume::Receiver<ProcessExit>,
    /// Disconnection broadcasts completion to every waiter.
    pub done: flume::Receiver<()>,
    pub control: ProcessControl,
}
impl Drop for TerminalProcess {
    fn drop(&mut self) {
        self.control.stop();
    }
}

impl TerminalProcess {
    /// Called on a background executor. No process or native window is created
    /// by constructing settings or rendering the panel.
    pub fn spawn(spec: LaunchSpec) -> Result<Self, String> {
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| e.to_string())?;
        let mut command = CommandBuilder::new(&spec.program);
        command.args(&spec.args);
        command.cwd(&spec.cwd);
        command.env("TERM", "xterm-256color");
        command.env("COLORTERM", "truecolor");
        // Git Bash login scripts otherwise replace the captured working directory.
        command.env("CHERE_INVOKING", "1");
        #[cfg(unix)]
        let poll_fd = unix_nonblocking_reader(&*pair.master).map_err(|e| e.to_string())?;
        let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
        let mut writer = pair.master.take_writer().map_err(|e| e.to_string())?;
        let mut child = pair
            .slave
            .spawn_command(command)
            .map_err(|e| e.to_string())?;
        drop(pair.slave);
        let control = ProcessControl {
            stop: Arc::new(AtomicBool::new(false)),
            size: Default::default(),
        };
        let (output_tx, output) = flume::bounded(64); // at most 256 KiB queued output
        let (input_tx, input_rx) = flume::bounded::<Vec<u8>>(8);
        let (exit_tx, exit) = flume::bounded(1);
        let (done_tx, done) = flume::bounded::<()>(0);
        let io_done = Arc::new(AtomicBool::new(false));
        #[cfg(unix)]
        let reader_done = io_done.clone();
        let reader_control = control.clone();
        let reader_thread = thread::spawn(move || {
            let mut buffer = [0; 4096];
            loop {
                #[cfg(unix)]
                if !unix_readable(&poll_fd) {
                    if reader_control.stop.load(Ordering::Acquire)
                        || reader_done.load(Ordering::Acquire)
                    {
                        break;
                    }
                    continue;
                }
                match reader.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(count) => {
                        let mut bytes = buffer[..count].to_vec();
                        loop {
                            match output_tx.send_timeout(bytes, Duration::from_millis(25)) {
                                Ok(()) => break,
                                Err(flume::SendTimeoutError::Timeout(returned)) => {
                                    if reader_control.stop.load(Ordering::Acquire) {
                                        break;
                                    }
                                    bytes = returned;
                                }
                                Err(flume::SendTimeoutError::Disconnected(_)) => return,
                            }
                        }
                    }
                    Err(error)
                        if matches!(
                            error.kind(),
                            io::ErrorKind::Interrupted | io::ErrorKind::WouldBlock
                        ) =>
                    {
                        continue;
                    }
                    Err(_) => break, // Unix PTYs can return EIO at EOF.
                }
            }
        });
        let writer_control = control.clone();
        let writer_done = io_done.clone();
        let writer_thread = thread::spawn(move || {
            while !writer_control.stop.load(Ordering::Acquire)
                && !writer_done.load(Ordering::Acquire)
            {
                match input_rx.recv_timeout(Duration::from_millis(25)) {
                    Ok(bytes) => {
                        let mut remaining = bytes.as_slice();
                        while !remaining.is_empty()
                            && !writer_control.stop.load(Ordering::Acquire)
                            && !writer_done.load(Ordering::Acquire)
                        {
                            match writer.write(remaining) {
                                Ok(0) => return,
                                Ok(count) => remaining = &remaining[count..],
                                Err(error) if error.kind() == io::ErrorKind::Interrupted => (),
                                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                                    thread::sleep(Duration::from_millis(10))
                                }
                                Err(_) => return,
                            }
                        }
                    }
                    Err(flume::RecvTimeoutError::Timeout) => (),
                    Err(flume::RecvTimeoutError::Disconnected) => break,
                }
            }
        });
        let lifetime = control.clone();
        thread::spawn(move || {
            let result = loop {
                if lifetime.stop.load(Ordering::Acquire) {
                    #[cfg(unix)]
                    terminate_foreground(&*pair.master);
                    let _ = child.kill();
                    let result = child.wait();
                    break result
                        .map(|_| ProcessExit::Stopped)
                        .unwrap_or_else(|e| ProcessExit::Failed(e.to_string()));
                }
                if let Some(size) = lifetime.size.lock().unwrap().take()
                    && let Err(error) = pair.master.resize(size)
                {
                    let _ = child.kill();
                    let _ = child.wait();
                    break ProcessExit::Failed(error.to_string());
                }
                match child.try_wait() {
                    Ok(Some(status)) => break ProcessExit::Exited(status.exit_code()),
                    Ok(None) => thread::sleep(Duration::from_millis(25)),
                    Err(error) => {
                        let _ = child.kill();
                        let _ = child.wait();
                        break ProcessExit::Failed(error.to_string());
                    }
                }
            };
            io_done.store(true, Ordering::Release);
            // Closing ConPTY can wait for its output reader. The reader keeps
            // draining even if a hidden/dropped view no longer consumes output.
            drop(pair.master);
            let _ = writer_thread.join();
            let _ = reader_thread.join();
            let _ = exit_tx.send(result);
            drop(done_tx);
        });
        Ok(Self {
            input: ProcessInput(input_tx),
            output,
            exit,
            done,
            control,
        })
    }
}

// Unix readers/writers must be cancellable even when a child job retains the
// slave descriptor after its shell exits. All duplicates share O_NONBLOCK.
#[cfg(unix)]
fn unix_nonblocking_reader(master: &dyn portable_pty::MasterPty) -> io::Result<std::fs::File> {
    use std::os::fd::FromRawFd;
    let fd = master
        .as_raw_fd()
        .ok_or_else(|| io::Error::other("PTY has no file descriptor"))?;
    // SAFETY: fd belongs to the live PTY; fcntl takes scalar flags.
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags == -1 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } == -1 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: F_DUPFD_CLOEXEC creates an independently owned descriptor; it is
    // transferred exactly once to File and never inherited by another child.
    let duplicate = unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 0) };
    if duplicate == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { std::fs::File::from_raw_fd(duplicate) })
}

#[cfg(unix)]
fn unix_readable(fd: &std::fs::File) -> bool {
    use std::os::fd::AsRawFd;
    let mut poll = libc::pollfd {
        fd: fd.as_raw_fd(),
        events: libc::POLLIN,
        revents: 0,
    };
    // SAFETY: poll points to one valid pollfd, whose File remains alive.
    unsafe { libc::poll(&mut poll, 1, 25) > 0 }
}

#[cfg(unix)]
fn terminate_foreground(master: &dyn portable_pty::MasterPty) {
    // The PTY reports only its own foreground process group. Never signal our
    // parent's group or an unchecked numeric PID.
    if let Some(group) = master.process_group_leader()
        && group > 0
        && group != unsafe { libc::getpgrp() }
    {
        // SAFETY: a negative PID addresses the checked PTY foreground group.
        unsafe {
            libc::kill(-group, libc::SIGKILL);
        }
    }
}
