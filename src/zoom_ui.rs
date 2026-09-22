//! All zoom triggers share the guarded appearance preference boundary.
use crate::*;

/// Shared by independent form/panel entities; stores only the effective window scale.
#[derive(Clone, Copy)]
pub(super) struct ZoomScale(pub f32);
impl Global for ZoomScale {}

pub(super) fn zoom_px(value: f32, cx: &App) -> Pixels {
    px(value * cx.try_global::<ZoomScale>().map_or(1., |zoom| zoom.0))
}

#[derive(Clone, Copy)]
pub(super) enum Zoom {
    In,
    Out,
    Reset,
}

impl Notes {
    pub(super) fn change_zoom(
        &mut self,
        direction: Zoom,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.document_interaction_blocked() || window.has_active_dialog(cx) {
            return;
        }
        let mut appearance = self.appearance();
        let previous = appearance.zoom_percent;
        match direction {
            Zoom::In => appearance.zoom_in(),
            Zoom::Out => appearance.zoom_out(),
            Zoom::Reset => appearance.zoom_percent = 100,
        }
        if previous != appearance.zoom_percent {
            self.update_appearance(appearance, window, cx);
        }
    }
}

#[cfg(test)]
mod tests;
