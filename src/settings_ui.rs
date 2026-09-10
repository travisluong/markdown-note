use super::*;
use gpui_component::dialog::Dialog;

impl Notes {
    pub(super) fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy || window.has_active_dialog(cx) {
            return;
        }
        self.menu = None;
        let owner = cx.weak_entity();
        window.open_dialog(cx, move |dialog, window, cx| {
            owner
                .update(cx, |this, cx| this.settings_dialog(dialog, window, cx))
                .unwrap_or_else(|_| Dialog::new(window, cx))
        });
        cx.notify();
    }

    fn settings_dialog(&mut self, dialog: Dialog, _: &mut Window, _: &mut Context<Self>) -> Dialog {
        dialog.title("Settings").w(px(460.)).child(
            Button::new("close-settings")
                .label("Close")
                .on_click(|_, window, cx| window.close_dialog(cx)),
        )
    }
}
