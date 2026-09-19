//! Fixed-order visibility controls for the toolbar preference list.
use super::{SettingsIntent, SettingsPanel};
use gpui::{Context, IntoElement, div, prelude::*};
use gpui_component::{Disableable, switch::Switch};
use mithril::notebook::ToolbarItem;

impl SettingsPanel {
    pub(super) fn render_toolbar_item(
        &self,
        item: ToolbarItem,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let id = item as usize;
        div()
            .id(("toolbar-row", id))
            .debug_selector(move || format!("toolbar-row-{id}"))
            .w_full()
            .h_10()
            .flex()
            .items_center()
            .gap_2()
            .child(
                div()
                    .debug_selector(move || format!("toolbar-switch-{id}"))
                    .child(
                        Switch::new(("toolbar-item", id))
                            .label(item.label())
                            .checked(self.values.toolbar.enabled(item))
                            .disabled(self.values.busy)
                            .on_click(cx.listener(move |this, enabled: &bool, _, cx| {
                                this.emit_intent(
                                    SettingsIntent::ToolbarItemEnabled(item, *enabled),
                                    cx,
                                )
                            })),
                    ),
            )
    }
}
