//! App-wide placement, independent of notebooks. Only accepted close writes it.
use crate::{Notes, notebook};
use gpui::{App, Bounds, Context, DisplayId, Pixels, Window, WindowBounds, point, px, size};
use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
struct Placement {
    version: u32,
    maximized: bool,
    normal: Geometry,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
struct Geometry {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

impl Geometry {
    fn valid(self) -> bool {
        [self.x, self.y, self.width, self.height]
            .into_iter()
            .all(|v| v.is_finite() && v.abs() <= 1_000_000.)
            && self.width > 0.
            && self.height > 0.
    }

    fn bounds(self) -> Bounds<Pixels> {
        Bounds::new(
            point(px(self.x), px(self.y)),
            size(px(self.width), px(self.height)),
        )
    }

    fn overlap(self, other: Self) -> f32 {
        ((self.x + self.width).min(other.x + other.width) - self.x.max(other.x)).max(0.)
            * ((self.y + self.height).min(other.y + other.height) - self.y.max(other.y)).max(0.)
    }

    fn display(self, displays: &[Self]) -> Option<usize> {
        // Prefer the display with most overlap; the primary display is first
        // and wins ties, including a completely disconnected former monitor.
        let mut best = None;
        let mut overlap = -1.;
        for (index, display) in displays
            .iter()
            .copied()
            .enumerate()
            .filter(|(_, d)| d.valid())
        {
            let area = self.overlap(display);
            if area > overlap {
                best = Some(index);
                overlap = area;
            }
        }
        best
    }

    fn fit(mut self, displays: &[Self]) -> Self {
        self.width = self.width.max(400.);
        self.height = self.height.max(300.);
        if let Some(index) = self.display(displays) {
            let display = displays[index];
            // Keep the application minimum even on unusually small displays.
            self.width = self.width.min(display.width.max(400.));
            self.height = self.height.min(display.height.max(300.));
            self.x = self
                .x
                .clamp(display.x, display.x + (display.width - self.width).max(0.));
            self.y = self.y.clamp(
                display.y,
                display.y + (display.height - self.height).max(0.),
            );
        }
        self
    }
}

impl From<Bounds<Pixels>> for Geometry {
    fn from(bounds: Bounds<Pixels>) -> Self {
        Self {
            x: bounds.origin.x.into(),
            y: bounds.origin.y.into(),
            width: bounds.size.width.into(),
            height: bounds.size.height.into(),
        }
    }
}

impl Placement {
    fn capture(bounds: WindowBounds, minimized_maximized: bool) -> Self {
        Self {
            version: 1,
            maximized: matches!(bounds, WindowBounds::Maximized(_))
                || (matches!(bounds, WindowBounds::Windowed(_)) && minimized_maximized),
            normal: bounds.get_bounds().into(),
        }
    }

    fn restore(self, displays: &[Geometry]) -> WindowBounds {
        let bounds = self.normal.fit(displays).bounds();
        if self.maximized {
            WindowBounds::Maximized(bounds)
        } else {
            WindowBounds::Windowed(bounds)
        }
    }

    fn validate(self) -> io::Result<Self> {
        if self.version == 1 && self.normal.valid() {
            Ok(self)
        } else {
            Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid window placement",
            ))
        }
    }
}

fn load(path: &Path) -> io::Result<Option<Placement>> {
    match fs::read(path) {
        Ok(bytes) => serde_json::from_slice::<Placement>(&bytes)
            .map_err(io::Error::other)?
            .validate()
            .map(Some),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn save(path: &Path, placement: Placement) -> io::Result<()> {
    notebook::write_json(path, &placement.validate()?)
}

pub(super) fn state_path(diagnostic: bool) -> Option<PathBuf> {
    if diagnostic {
        return None;
    }
    match notebook::index_path() {
        Ok(path) => Some(path.with_file_name("window-state.json")),
        Err(error) => {
            eprintln!("Window placement path: {error}");
            None
        }
    }
}

pub(super) fn startup(path: Option<&Path>, cx: &App) -> WindowBounds {
    let fallback = || WindowBounds::Windowed(Bounds::centered(None, size(px(900.), px(700.)), cx));
    let Some(path) = path else {
        return fallback();
    };
    match load(path) {
        Ok(Some(placement)) => {
            let mut displays = cx.displays();
            if let Some(primary) = cx.primary_display() {
                displays.sort_by_key(|display| display.id() != primary.id());
            }
            placement.restore(
                &displays
                    .iter()
                    .map(|display| display.bounds().into())
                    .collect::<Vec<_>>(),
            )
        }
        Ok(None) => fallback(),
        Err(error) => {
            eprintln!("Loading window placement: {error}");
            fallback()
        }
    }
}

pub(super) fn display_for(bounds: WindowBounds, cx: &App) -> Option<DisplayId> {
    let mut displays = cx.displays();
    if let Some(primary) = cx.primary_display() {
        displays.sort_by_key(|display| display.id() != primary.id());
    }
    let geometry: Vec<_> = displays
        .iter()
        .map(|display| display.bounds().into())
        .collect();
    // Windows validates initial bounds against the requested display, falling
    // back to its default rectangle otherwise. Supply the restored monitor too.
    Geometry::from(bounds.get_bounds())
        .display(&geometry)
        .map(|index| displays[index].id())
}

impl Notes {
    pub(super) fn close_window(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let placement = self.window_state_path.clone().map(|path| {
            (
                path,
                Placement::capture(window.window_bounds(), minimized_restore_maximized(window)),
            )
        });
        let write = cx.background_executor().spawn(async move {
            if let Some((path, placement)) = placement
                && let Err(error) = save(&path, placement)
            {
                eprintln!("Saving window placement: {error}");
            }
        });
        let cleanup = self.shutdown_terminals(cx);
        cx.spawn_in(window, async move |_, cx| {
            // Both tasks have already started; neither failure delays the other.
            write.await;
            cleanup.await;
            let _ = cx.update(|window, _| window.remove_window());
        })
        .detach();
    }
}

#[cfg(any(not(target_os = "windows"), test))]
fn minimized_restore_maximized(_: &Window) -> bool {
    false
}

#[cfg(all(target_os = "windows", not(test)))]
fn minimized_restore_maximized(window: &Window) -> bool {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetWindowPlacement, IsIconic, WINDOWPLACEMENT, WPF_RESTORETOMAXIMIZED,
    };
    let Ok(handle) = HasWindowHandle::window_handle(window) else {
        return false;
    };
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        return false;
    };
    let mut placement = WINDOWPLACEMENT {
        length: std::mem::size_of::<WINDOWPLACEMENT>() as u32,
        ..Default::default()
    };
    // SAFETY: read-only queries of this live window's HWND on its UI thread;
    // placement has the required size and remains valid throughout the call.
    unsafe {
        let hwnd = handle.hwnd.get() as _;
        if IsIconic(hwnd) == 0 {
            return false;
        }
        if GetWindowPlacement(hwnd, &mut placement) == 0 {
            eprintln!(
                "Reading minimized window placement: {}",
                io::Error::last_os_error()
            );
            return false;
        }
    }
    placement.flags & WPF_RESTORETOMAXIMIZED != 0
}

#[cfg(test)]
mod tests;
