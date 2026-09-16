//! Windows caption commands are posted so native move/resize loops run after
//! GPUI releases the current input callback and its application/window borrows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CaptionCommand {
    Move,
    Minimize,
    Maximize,
    Restore,
}

impl CaptionCommand {
    pub(super) fn zoom(maximized: bool) -> Self {
        if maximized {
            Self::Restore
        } else {
            Self::Maximize
        }
    }
}

#[cfg(not(test))]
pub(super) fn post_command(command: CaptionCommand, window: &gpui::Window) -> Result<(), String> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows_sys::Win32::UI::{
        Input::KeyboardAndMouse::ReleaseCapture,
        WindowsAndMessaging::{
            HTCAPTION, PostMessageW, SC_MAXIMIZE, SC_MINIMIZE, SC_MOVE, SC_RESTORE, WM_SYSCOMMAND,
        },
    };

    let handle = HasWindowHandle::window_handle(window).map_err(|error| error.to_string())?;
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        return Err("expected a Windows window handle".into());
    };
    let code = match command {
        CaptionCommand::Move => SC_MOVE | HTCAPTION,
        CaptionCommand::Minimize => SC_MINIMIZE,
        CaptionCommand::Maximize => SC_MAXIMIZE,
        CaptionCommand::Restore => SC_RESTORE,
    };
    // SAFETY: This is the live GPUI window's HWND on its UI thread. The posted
    // message contains only integer parameters, never borrowed data. Releasing
    // GPUI's client mouse capture lets Windows take over the caption drag.
    unsafe {
        if command == CaptionCommand::Move {
            ReleaseCapture();
        }
        if PostMessageW(handle.hwnd.get() as _, WM_SYSCOMMAND, code as usize, 0) == 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
    }
    Ok(())
}
