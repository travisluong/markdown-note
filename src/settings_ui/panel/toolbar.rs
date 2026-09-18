//! Drag handles and insertion targets for the toolbar preference list.
use super::{Section, SettingsIntent, SettingsPanel};
use gpui::{AppContext, Context, EntityId, IntoElement, Render, Window, div, prelude::*, px};
use gpui_component::{ActiveTheme, Disableable, Icon, IconName, switch::Switch};
use mithril::notebook::ToolbarItem;

#[derive(Clone)]
pub(super) struct ToolbarDrag {
    pub panel: EntityId,
    pub item: ToolbarItem,
}

impl Render for ToolbarDrag {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_3()
            .py_1()
            .rounded_md()
            .bg(cx.theme().accent)
            .text_color(cx.theme().foreground)
            .child(self.item.label())
    }
}

impl SettingsPanel {
    fn drop_toolbar_item(
        &mut self,
        drag: &ToolbarDrag,
        target: ToolbarItem,
        after: bool,
        cx: &mut Context<Self>,
    ) {
        if drag.panel != cx.entity_id()
            || self.section != Section::Toolbar
            || self.values.busy
            || drag.item.region() != target.region()
        {
            return;
        }
        self.emit_intent(
            SettingsIntent::ToolbarItemMoved {
                item: drag.item,
                target,
                after,
            },
            cx,
        );
    }

    pub(super) fn render_toolbar_item(
        &self,
        item: ToolbarItem,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let panel = cx.entity_id();
        let id = item as usize;
        div()
            .id(("toolbar-row", id))
            .debug_selector(move || format!("toolbar-row-{id}"))
            .relative()
            .w_full()
            .h_10()
            .flex()
            .items_center()
            .gap_2()
            .child(
                div()
                    .id(("toolbar-grip", id))
                    .debug_selector(move || format!("toolbar-grip-{id}"))
                    .w_6()
                    .h_8()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_color(cx.theme().muted_foreground)
                    .child(
                        Icon::new(IconName::EllipsisVertical)
                            .themed_color(!self.values.busy)
                            .size(px(16.)),
                    )
                    .when(!self.values.busy, |view| {
                        view.cursor_grab()
                            .on_drag(ToolbarDrag { panel, item }, |drag, _, _, cx| {
                                cx.stop_propagation();
                                cx.new(|_| drag.clone())
                            })
                    }),
            )
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
            // Overlay insertion targets only during a drag, leaving ordinary switch
            // presses independent. Absolute halves keep row geometry stable on hover.
            .when(cx.has_active_drag() && !self.values.busy, |view| {
                view.children([false, true].map(|after| {
                    div()
                        .id(("toolbar-drop", id * 2 + usize::from(after)))
                        .debug_selector(move || format!("toolbar-drop-{id}-{after}"))
                        .absolute()
                        .left_0()
                        .right_0()
                        .h_1_2()
                        .when(after, |view| view.bottom_0())
                        .when(!after, |view| view.top_0())
                        .can_drop(move |drag, _, _| {
                            drag.downcast_ref::<ToolbarDrag>().is_some_and(|drag| {
                                drag.panel == panel && drag.item.region() == item.region()
                            })
                        })
                        .drag_over::<ToolbarDrag>(move |style, drag, _, cx| {
                            if drag.panel != panel || drag.item.region() != item.region() {
                                return style;
                            }
                            let style = style.border_color(cx.theme().primary);
                            if after {
                                style.border_b_2()
                            } else {
                                style.border_t_2()
                            }
                        })
                        .on_drop(cx.listener(move |this, drag: &ToolbarDrag, _, cx| {
                            cx.stop_propagation();
                            this.drop_toolbar_item(drag, item, after, cx);
                        }))
                }))
            })
    }
}
