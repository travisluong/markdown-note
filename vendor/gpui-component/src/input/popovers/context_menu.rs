use gpui::{
    App, AppContext as _, ClipboardItem, Context, Corner, DismissEvent, Entity, IntoElement,
    MouseDownEvent, ParentElement as _, Pixels, Point, Render, Styled, Subscription, Window,
    anchored, deferred, div, prelude::FluentBuilder as _, px,
};
use rust_i18n::t;

use crate::{
    ActiveTheme as _,
    input::{self, InputState, popovers::ContextMenu},
    menu::{PopupMenu, PopupMenuItem},
};

/// Context menu for mouse right clicks.
pub(crate) struct MouseContextMenu {
    editor: Entity<InputState>,
    menu: Entity<PopupMenu>,
    mouse_position: Point<Pixels>,
    open: bool,

    _subscriptions: Vec<Subscription>,
}

impl InputState {
    pub(crate) fn handle_right_click_menu(
        &mut self,
        event: &MouseDownEvent,
        offset: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Resolve the painted link before caret movement can reveal its source.
        let destination = self
            .live_preview
            .then(|| self.link_at_position(event.position))
            .flatten();
        if destination.is_some() {
            self.selecting = false;
        } else if !self.selected_range.contains(offset) {
            self.move_to(offset, None, cx);
        }

        self.context_menu = Some(ContextMenu::MouseContext(self.mouse_context_menu.clone()));

        let is_code_editor = self.mode.is_code_editor();
        if is_code_editor {
            self.handle_hover_definition(offset, window, cx);
        }

        let is_enable = !self.disabled;
        let has_goto_definition = is_enable && self.lsp.definition_provider.is_some();
        let has_code_action = is_enable && !self.lsp.code_action_providers.is_empty();
        let is_selected = !self.selected_range.is_empty();
        let has_paste = is_enable && cx.read_from_clipboard().is_some();

        let action_context = self.focus_handle.clone();
        let table_target = self
            .live_preview
            .then(|| self.text_wrapper.tables.at(offset))
            .flatten()
            .map(|(t, r, c)| (r, c, t.alignments.len(), self.text_wrapper.tables.revision));
        let editor = cx.entity().downgrade();
        self.mouse_context_menu.update(cx, |this, cx| {
            this.mouse_position = event.position;
            this.menu.update(cx, |menu, cx| {
                let mut new_menu = PopupMenu::new(cx)
                    .when_some(destination, |mut menu, destination| {
                        menu.menu_items
                            .push(PopupMenuItem::new("Copy link").on_click(move |_, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(
                                    destination.clone(),
                                ));
                            }));
                        menu.separator()
                    })
                    .when(is_code_editor, |m| {
                        m.menu_with_enable(
                            t!("Input.Go to Definition"),
                            Box::new(input::GoToDefinition),
                            has_goto_definition,
                        )
                        .menu_with_enable(
                            t!("Input.Show Code Actions"),
                            Box::new(input::ToggleCodeActions),
                            has_code_action,
                        )
                        .separator()
                    })
                    .menu_with_enable(
                        t!("Input.Cut"),
                        Box::new(input::Cut),
                        is_enable && is_selected,
                    )
                    .menu_with_enable(t!("Input.Copy"), Box::new(input::Copy), is_selected)
                    .menu_with_enable(t!("Input.Paste"), Box::new(input::Paste), has_paste)
                    .separator()
                    .menu(t!("Input.Select All"), Box::new(input::SelectAll));

                if let Some((row, _, columns, revision)) = table_target {
                    use input::table::{Alignment, Command};
                    new_menu.menu_items.push(PopupMenuItem::separator());
                    for (label, command, enabled) in [
                        ("Insert row above", Command::RowAbove, true),
                        ("Insert row below", Command::RowBelow, true),
                        ("Delete row", Command::DeleteRow, row > 0),
                        ("Insert column left", Command::ColumnLeft, true),
                        ("Insert column right", Command::ColumnRight, true),
                        ("Delete column", Command::DeleteColumn, columns > 1),
                        ("Align left", Command::Align(Alignment::Left), true),
                        ("Align center", Command::Align(Alignment::Center), true),
                        ("Align right", Command::Align(Alignment::Right), true),
                        ("Delete table", Command::DeleteTable, true),
                    ] {
                        let editor = editor.clone();
                        new_menu.menu_items.push(
                            PopupMenuItem::new(label)
                                .disabled(!is_enable || !enabled)
                                .on_click(move |_, window, cx| {
                                    let _ = editor.update(cx, |editor, cx| {
                                        editor.table_command(command, offset, revision, window, cx);
                                    });
                                }),
                        );
                    }
                }

                new_menu.focus_handle = menu.focus_handle.clone();
                *menu = new_menu.small().action_context(action_context);
                menu.focus_handle.focus(window);
                cx.notify();
            });
            this.open = true;

            cx.notify();
        });
        // Keep the input's default mouse-down focus handler from taking focus back.
        window.prevent_default();
        cx.stop_propagation();
        cx.notify();
    }
}

impl MouseContextMenu {
    #[cfg(feature = "editor-diagnostics")]
    pub(crate) fn diagnostic_menu(&self) -> Option<Entity<PopupMenu>> {
        self.open.then(|| self.menu.clone())
    }
    pub(crate) fn new(
        editor: Entity<InputState>,
        window: &mut Window,
        cx: &mut App,
    ) -> Entity<Self> {
        cx.new(|cx| {
            let menu = cx.new(|cx| PopupMenu::new(cx).small());

            let _subscriptions = vec![cx.subscribe_in(&menu, window, {
                move |this: &mut Self, _, _: &DismissEvent, window, cx| {
                    this.close(window, cx);
                }
            })];

            Self {
                editor,
                menu,
                mouse_position: Point::default(),
                open: false,
                _subscriptions,
            }
        })
    }

    #[inline]
    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    #[inline]
    pub(crate) fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open = false;
        self.editor.update(cx, |this, cx| {
            this.focus(window, cx);
        });
    }
}

impl Render for MouseContextMenu {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.open {
            return div().into_any_element();
        }

        deferred(
            anchored()
                .snap_to_window_with_margin(px(8.))
                .anchor(Corner::TopLeft)
                .position(self.mouse_position)
                .child(
                    div()
                        .font_family(cx.theme().font_family.clone())
                        .cursor_default()
                        .child(self.menu.clone()),
                ),
        )
        .into_any_element()
    }
}
