use std::{cell::RefCell, rc::Rc, time::Duration};

use gpui::{
    Animation, AnimationExt as _, AnyElement, App, Bounds, BoxShadow, ClickEvent, Div, Edges,
    FocusHandle, Hsla, InteractiveElement, IntoElement, KeyBinding, MouseButton, ParentElement,
    Pixels, Point, RenderOnce, SharedString, Size, StyleRefinement, Styled, Window, anchored, div,
    hsla, point, prelude::FluentBuilder, px, relative,
};
use rust_i18n::t;

use crate::{
    ActiveTheme as _, IconName, Root, Sizable as _, StyledExt, WindowExt as _,
    actions::{Cancel, Confirm},
    animation::cubic_bezier,
    button::{Button, ButtonVariant, ButtonVariants as _},
    h_flex,
    scroll::ScrollableElement as _,
    v_flex,
};

const CONTEXT: &str = "Dialog";
pub(crate) fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("escape", Cancel, Some(CONTEXT)),
        KeyBinding::new("enter", Confirm { secondary: false }, Some(CONTEXT)),
    ]);
}

type RenderButtonFn = Box<dyn FnOnce(&mut Window, &mut App) -> AnyElement>;
type FooterFn =
    Box<dyn Fn(RenderButtonFn, RenderButtonFn, &mut Window, &mut App) -> Vec<AnyElement>>;

/// Per-open placement for an opt-in movable dialog. Clone the handle when rebuilding
/// the dialog; reset it only when the preferred content dimensions change.
#[derive(Clone)]
pub struct DialogPlacement(Rc<RefCell<Placement>>);

struct Placement {
    preferred: Size<Pixels>,
    margin: Pixels,
    moved: bool,
    origin: Option<Point<Pixels>>,
    resolved_size: Size<Pixels>,
    drag: Option<(Point<Pixels>, Point<Pixels>)>,
}

impl DialogPlacement {
    pub fn new(preferred: Size<Pixels>) -> Self {
        Self(Rc::new(RefCell::new(Placement {
            preferred,
            margin: px(16.),
            moved: false,
            origin: None,
            resolved_size: preferred,
            drag: None,
        })))
    }

    /// Actual dimensions from the latest dialog layout, for responsive contents.
    pub fn size(&self) -> Size<Pixels> {
        self.0.borrow().resolved_size
    }

    /// Set the minimum clearance from the window content edges (defaults to 16px).
    pub fn with_margin(self, margin: Pixels) -> Self {
        self.0.borrow_mut().margin = margin.max(px(0.));
        self
    }

    /// Resize responsive content, keeping a dragged position or centering an unmoved dialog.
    /// The next layout clamps the position and size to the available viewport.
    pub fn set_preferred_size(&self, preferred: Size<Pixels>) {
        let mut state = self.0.borrow_mut();
        if state.preferred != preferred {
            state.preferred = preferred;
            state.drag = None;
            if !state.moved {
                state.origin = None;
            }
        }
    }

    /// Center the dialog again, using the given preferred dimensions.
    pub fn reset(&self, preferred: Size<Pixels>) {
        let mut state = self.0.borrow_mut();
        state.preferred = preferred;
        state.moved = false;
        state.origin = None;
        state.drag = None;
    }

    /// Resolve and retain placement inside a viewport, respecting the configured margin.
    pub fn bounds(&self, viewport: Size<Pixels>) -> Bounds<Pixels> {
        let mut state = self.0.borrow_mut();
        let margin = state.margin;
        let size = gpui::size(
            state
                .preferred
                .width
                .min((viewport.width - margin * 2.).max(px(0.))),
            state
                .preferred
                .height
                .min((viewport.height - margin * 2.).max(px(0.))),
        );
        let origin = state.origin.unwrap_or(point(
            (viewport.width - size.width) / 2.,
            (viewport.height - size.height) / 2.,
        ));
        let origin = point(
            origin
                .x
                .max(margin)
                .min((viewport.width - size.width - margin).max(px(0.))),
            origin
                .y
                .max(margin)
                .min((viewport.height - size.height - margin).max(px(0.))),
        );
        state.origin = Some(origin);
        state.resolved_size = size;
        Bounds { origin, size }
    }
}

/// Dialog button props.
pub struct DialogButtonProps {
    ok_text: Option<SharedString>,
    ok_variant: ButtonVariant,
    cancel_text: Option<SharedString>,
    cancel_variant: ButtonVariant,
}

impl Default for DialogButtonProps {
    fn default() -> Self {
        Self {
            ok_text: None,
            ok_variant: ButtonVariant::Primary,
            cancel_text: None,
            cancel_variant: ButtonVariant::default(),
        }
    }
}

impl DialogButtonProps {
    /// Sets the text of the OK button. Default is `OK`.
    pub fn ok_text(mut self, ok_text: impl Into<SharedString>) -> Self {
        self.ok_text = Some(ok_text.into());
        self
    }

    /// Sets the variant of the OK button. Default is `ButtonVariant::Primary`.
    pub fn ok_variant(mut self, ok_variant: ButtonVariant) -> Self {
        self.ok_variant = ok_variant;
        self
    }

    /// Sets the text of the Cancel button. Default is `Cancel`.
    pub fn cancel_text(mut self, cancel_text: impl Into<SharedString>) -> Self {
        self.cancel_text = Some(cancel_text.into());
        self
    }

    /// Sets the variant of the Cancel button. Default is `ButtonVariant::default()`.
    pub fn cancel_variant(mut self, cancel_variant: ButtonVariant) -> Self {
        self.cancel_variant = cancel_variant;
        self
    }
}

/// A modal to display content in a dialog box.
#[derive(IntoElement)]
pub struct Dialog {
    style: StyleRefinement,
    title: Option<AnyElement>,
    footer: Option<FooterFn>,
    content: Div,
    width: Pixels,
    max_width: Option<Pixels>,
    margin_top: Option<Pixels>,
    placement: Option<DialogPlacement>,

    on_close: Rc<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>,
    on_ok: Option<Rc<dyn Fn(&ClickEvent, &mut Window, &mut App) -> bool + 'static>>,
    on_cancel: Rc<dyn Fn(&ClickEvent, &mut Window, &mut App) -> bool + 'static>,
    button_props: DialogButtonProps,
    close_button: bool,
    overlay: bool,
    overlay_color: Option<Hsla>,
    overlay_closable: bool,
    keyboard: bool,

    /// This will be change when open the dialog, the focus handle is create when open the dialog.
    pub(crate) focus_handle: FocusHandle,
    pub(crate) layer_ix: usize,
    pub(crate) overlay_visible: bool,
}

pub(crate) fn overlay_color(overlay: bool, cx: &App) -> Hsla {
    if !overlay {
        return hsla(0., 0., 0., 0.);
    }

    cx.theme().overlay
}

impl Dialog {
    /// Create a new dialog.
    pub fn new(_: &mut Window, cx: &mut App) -> Self {
        Self {
            focus_handle: cx.focus_handle(),
            style: StyleRefinement::default(),
            title: None,
            footer: None,
            content: v_flex(),
            margin_top: None,
            placement: None,
            width: px(480.),
            max_width: None,
            overlay: true,
            overlay_color: None,
            keyboard: true,
            layer_ix: 0,
            overlay_visible: false,
            on_close: Rc::new(|_, _, _| {}),
            on_ok: None,
            on_cancel: Rc::new(|_, _, _| true),
            button_props: DialogButtonProps::default(),
            close_button: true,
            overlay_closable: true,
        }
    }

    /// Sets the title of the dialog.
    pub fn title(mut self, title: impl IntoElement) -> Self {
        self.title = Some(title.into_any_element());
        self
    }

    /// Enable title-bar dragging and bounded, fixed dimensions. The caller owns
    /// the placement handle for this dialog's lifetime. Contents fill the body
    /// and must provide their own scrolling. Other dialogs retain default layout.
    pub fn movable(mut self, placement: DialogPlacement) -> Self {
        self.placement = Some(placement);
        self
    }

    /// Set the footer of the dialog.
    ///
    /// The `footer` is a function that takes two `RenderButtonFn` and a `WindowContext` and returns a list of `AnyElement`.
    ///
    /// - First `RenderButtonFn` is the render function for the OK button.
    /// - Second `RenderButtonFn` is the render function for the CANCEL button.
    ///
    /// When you set the footer, the footer will be placed default footer buttons.
    pub fn footer<E, F>(mut self, footer: F) -> Self
    where
        E: IntoElement,
        F: Fn(RenderButtonFn, RenderButtonFn, &mut Window, &mut App) -> Vec<E> + 'static,
    {
        self.footer = Some(Box::new(move |ok, cancel, window, cx| {
            footer(ok, cancel, window, cx)
                .into_iter()
                .map(|e| e.into_any_element())
                .collect()
        }));
        self
    }

    /// Set to use confirm dialog, with OK and Cancel buttons.
    ///
    /// See also [`Self::alert`]
    pub fn confirm(self) -> Self {
        self.footer(|ok, cancel, window, cx| vec![cancel(window, cx), ok(window, cx)])
            .overlay_closable(false)
            .close_button(false)
    }

    /// Set to as a alter dialog, with OK button.
    ///
    /// See also [`Self::confirm`]
    pub fn alert(self) -> Self {
        self.footer(|ok, _, window, cx| vec![ok(window, cx)])
            .overlay_closable(false)
            .close_button(false)
    }

    /// Set the button props of the dialog.
    pub fn button_props(mut self, button_props: DialogButtonProps) -> Self {
        self.button_props = button_props;
        self
    }

    /// Sets the callback for when the dialog is closed.
    ///
    /// Called after [`Self::on_ok`] or [`Self::on_cancel`] callback.
    pub fn on_close(
        mut self,
        on_close: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_close = Rc::new(on_close);
        self
    }

    /// Sets the callback for when the dialog is has been confirmed.
    ///
    /// The callback should return `true` to close the dialog, if return `false` the dialog will not be closed.
    pub fn on_ok(
        mut self,
        on_ok: impl Fn(&ClickEvent, &mut Window, &mut App) -> bool + 'static,
    ) -> Self {
        self.on_ok = Some(Rc::new(on_ok));
        self
    }

    /// Sets the callback for when the dialog is has been canceled.
    ///
    /// The callback should return `true` to close the dialog, if return `false` the dialog will not be closed.
    pub fn on_cancel(
        mut self,
        on_cancel: impl Fn(&ClickEvent, &mut Window, &mut App) -> bool + 'static,
    ) -> Self {
        self.on_cancel = Rc::new(on_cancel);
        self
    }

    /// Sets the false to hide close icon, default: true
    pub fn close_button(mut self, close_button: bool) -> Self {
        self.close_button = close_button;
        self
    }

    /// Set the top offset of the dialog, defaults to None, will use the 1/10 of the viewport height.
    pub fn margin_top(mut self, margin_top: Pixels) -> Self {
        self.margin_top = Some(margin_top);
        self
    }

    /// Sets the width of the dialog, defaults to 480px.
    ///
    /// See also [`Self::width`]
    pub fn w(mut self, width: Pixels) -> Self {
        self.width = width;
        self
    }

    /// Sets the width of the dialog, defaults to 480px.
    pub fn width(mut self, width: Pixels) -> Self {
        self.width = width;
        self
    }

    /// Set the maximum width of the dialog, defaults to `None`.
    pub fn max_w(mut self, max_width: Pixels) -> Self {
        self.max_width = Some(max_width);
        self
    }

    /// Set the overlay of the dialog, defaults to `true`.
    pub fn overlay(mut self, overlay: bool) -> Self {
        self.overlay = overlay;
        self
    }

    /// Override the theme's overlay color without changing modal mouse handling.
    pub fn overlay_color(mut self, color: impl Into<Hsla>) -> Self {
        self.overlay_color = Some(color.into());
        self
    }

    fn resolved_overlay_color(&self, cx: &App) -> Hsla {
        if self.overlay {
            self.overlay_color.unwrap_or_else(|| overlay_color(true, cx))
        } else {
            overlay_color(false, cx)
        }
    }

    /// Return the color used to paint this dialog's overlay in headless tests.
    #[cfg(feature = "editor-diagnostics")]
    pub fn diagnostic_overlay_color(&self, cx: &App) -> Hsla {
        self.resolved_overlay_color(cx)
    }

    /// Set the overlay closable of the dialog, defaults to `true`.
    ///
    /// When the overlay is clicked, the dialog will be closed.
    pub fn overlay_closable(mut self, overlay_closable: bool) -> Self {
        self.overlay_closable = overlay_closable;
        self
    }

    /// Set whether to support keyboard esc to close the dialog, defaults to `true`.
    pub fn keyboard(mut self, keyboard: bool) -> Self {
        self.keyboard = keyboard;
        self
    }

    pub(crate) fn has_overlay(&self) -> bool {
        self.overlay
    }
}

impl ParentElement for Dialog {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.content.extend(elements);
    }
}

impl Styled for Dialog {
    fn style(&mut self) -> &mut gpui::StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Dialog {
    fn render(self, window: &mut Window, cx: &mut App) -> impl gpui::IntoElement {
        let overlay_color = self.resolved_overlay_color(cx);
        let layer_ix = self.layer_ix;
        let on_close = self.on_close.clone();
        let on_ok = self.on_ok.clone();
        let on_cancel = self.on_cancel.clone();

        let render_ok: RenderButtonFn = Box::new({
            let on_ok = on_ok.clone();
            let on_close = on_close.clone();
            let ok_text = self
                .button_props
                .ok_text
                .unwrap_or_else(|| t!("Dialog.ok").into());
            let ok_variant = self.button_props.ok_variant;
            move |_, _| {
                Button::new("ok")
                    .label(ok_text)
                    .with_variant(ok_variant)
                    .on_click({
                        let on_ok = on_ok.clone();
                        let on_close = on_close.clone();

                        move |_, window, cx| {
                            if let Some(on_ok) = &on_ok {
                                if !on_ok(&ClickEvent::default(), window, cx) {
                                    return;
                                }
                            }

                            on_close(&ClickEvent::default(), window, cx);
                            window.close_dialog(cx);
                        }
                    })
                    .into_any_element()
            }
        });
        let render_cancel: RenderButtonFn = Box::new({
            let on_cancel = on_cancel.clone();
            let on_close = on_close.clone();
            let cancel_text = self
                .button_props
                .cancel_text
                .unwrap_or_else(|| t!("Dialog.cancel").into());
            let cancel_variant = self.button_props.cancel_variant;
            move |_, _| {
                Button::new("cancel")
                    .label(cancel_text)
                    .with_variant(cancel_variant)
                    .on_click({
                        let on_cancel = on_cancel.clone();
                        let on_close = on_close.clone();
                        move |_, window, cx| {
                            if !on_cancel(&ClickEvent::default(), window, cx) {
                                return;
                            }

                            on_close(&ClickEvent::default(), window, cx);
                            window.close_dialog(cx);
                        }
                    })
                    .into_any_element()
            }
        });

        let window_paddings = crate::window_border::window_paddings(window);
        let view_size = window.viewport_size()
            - gpui::size(
                window_paddings.left + window_paddings.right,
                window_paddings.top + window_paddings.bottom,
            );
        let bounds = Bounds {
            origin: Point::default(),
            size: view_size,
        };
        let offset_top = px(layer_ix as f32 * 16.);
        let y = self.margin_top.unwrap_or(view_size.height / 10.) + offset_top;
        let x = bounds.center().x - self.width / 2.;
        let placement = self.placement.clone();
        let movable_bounds = placement.as_ref().map(|state| state.bounds(view_size));
        let (x, y) = movable_bounds.map_or((x, y), |bounds| (bounds.origin.x, bounds.origin.y));

        let base_size = window.text_style().font_size;
        let rem_size = window.rem_size();

        let mut paddings = Edges::all(px(24.));
        if let Some(pl) = self.style.padding.left {
            paddings.left = pl.to_pixels(base_size, rem_size);
        }
        if let Some(pr) = self.style.padding.right {
            paddings.right = pr.to_pixels(base_size, rem_size);
        }
        if let Some(pt) = self.style.padding.top {
            paddings.top = pt.to_pixels(base_size, rem_size);
        }
        if let Some(pb) = self.style.padding.bottom {
            paddings.bottom = pb.to_pixels(base_size, rem_size);
        }

        let animation = Animation::new(Duration::from_secs_f64(0.25))
            .with_easing(cubic_bezier(0.32, 0.72, 0., 1.));

        anchored()
            .position(point(window_paddings.left, window_paddings.top))
            .snap_to_window()
            .child(
                div()
                    .id("dialog")
                    .occlude()
                    .w(view_size.width)
                    .h(view_size.height)
                    .when_some(placement.clone(), |this, placement| {
                        this.child(
                            gpui::canvas(
                                |_, _, _| (),
                                move |_, _, window, _| {
                                    let move_state = placement.clone();
                                    window.on_mouse_event(
                                        move |event: &gpui::MouseMoveEvent, phase, window, cx| {
                                            if !phase.capture() {
                                                return;
                                            }
                                            let mut state = move_state.0.borrow_mut();
                                            if let Some((pointer, origin)) = state.drag {
                                                if event.pressed_button == Some(MouseButton::Left) {
                                                    state.moved = true;
                                                    state.origin =
                                                        Some(origin + event.position - pointer);
                                                    window.refresh();
                                                    cx.stop_propagation();
                                                } else {
                                                    state.drag = None;
                                                }
                                            }
                                        },
                                    );
                                    let release_state = placement.clone();
                                    window.on_mouse_event(
                                        move |event: &gpui::MouseUpEvent, phase, _, cx| {
                                            if phase.capture()
                                                && event.button == MouseButton::Left
                                                && release_state
                                                    .0
                                                    .borrow_mut()
                                                    .drag
                                                    .take()
                                                    .is_some()
                                            {
                                                cx.stop_propagation();
                                            }
                                        },
                                    );
                                },
                            )
                            .absolute()
                            .size_full(),
                        )
                    })
                    .when(self.overlay_visible, |this| {
                        this.bg(overlay_color)
                    })
                    .when(self.overlay, |this| {
                        // Only the last dialog owns the `mouse down - close dialog` event.
                        if (self.layer_ix + 1) != Root::read(window, cx).active_dialogs.len() {
                            return this;
                        }

                        this.on_any_mouse_down({
                            let on_cancel = on_cancel.clone();
                            let on_close = on_close.clone();
                            move |event, window, cx| {
                                cx.stop_propagation();

                                if self.overlay_closable && event.button == MouseButton::Left {
                                    on_cancel(&ClickEvent::default(), window, cx);
                                    on_close(&ClickEvent::default(), window, cx);
                                    window.close_dialog(cx);
                                }
                            }
                        })
                    })
                    .child(
                        v_flex()
                            .id(layer_ix)
                            .debug_selector(|| "dialog-surface".into())
                            .bg(cx.theme().background)
                            .border_1()
                            .border_color(cx.theme().border)
                            .rounded(cx.theme().radius_lg)
                            .min_h_24()
                            .pt(paddings.top)
                            .pb(paddings.bottom)
                            .gap(paddings.top.min(px(16.)))
                            .refine_style(&self.style)
                            .px_0()
                            .key_context(CONTEXT)
                            .track_focus(&self.focus_handle)
                            .tab_group()
                            .when(self.keyboard, |this| {
                                this.on_action({
                                    let on_cancel = on_cancel.clone();
                                    let on_close = on_close.clone();
                                    move |_: &Cancel, window, cx| {
                                        // FIXME:
                                        //
                                        // Here some Dialog have no focus_handle, so it will not work will Escape key.
                                        // But by now, we `cx.close_dialog()` going to close the last active model, so the Escape is unexpected to work.
                                        on_cancel(&ClickEvent::default(), window, cx);
                                        on_close(&ClickEvent::default(), window, cx);
                                        window.close_dialog(cx);
                                    }
                                })
                                .on_action({
                                    let on_ok = on_ok.clone();
                                    let on_close = on_close.clone();
                                    let has_footer = self.footer.is_some();
                                    move |_: &Confirm, window, cx| {
                                        if let Some(on_ok) = &on_ok {
                                            if on_ok(&ClickEvent::default(), window, cx) {
                                                on_close(&ClickEvent::default(), window, cx);
                                                window.close_dialog(cx);
                                            }
                                        } else if has_footer {
                                            window.close_dialog(cx);
                                        }
                                    }
                                })
                            })
                            // There style is high priority, can't be overridden.
                            .absolute()
                            .occlude()
                            .relative()
                            .left(x)
                            .top(y)
                            .w(self.width)
                            .when_some(movable_bounds, |this, bounds| {
                                this.w(bounds.size.width)
                                    .h(bounds.size.height)
                                    .min_h_0()
                                    .overflow_hidden()
                            })
                            .when_some(self.max_width, |this, w| this.max_w(w))
                            .when_some(self.title, |this, title| {
                                this.child(
                                    div()
                                        .id("dialog-title")
                                        .debug_selector(|| "dialog-title".into())
                                        .flex_shrink_0()
                                        .when_some(placement.clone(), |this, placement| {
                                            this.cursor_move().on_mouse_down(
                                                MouseButton::Left,
                                                move |event, _, cx| {
                                                    let mut state = placement.0.borrow_mut();
                                                    if let Some(origin) = state.origin {
                                                        state.drag = Some((event.position, origin));
                                                        cx.stop_propagation();
                                                    }
                                                },
                                            )
                                        })
                                        .pl(paddings.left)
                                        .pr(paddings.right)
                                        .line_height(relative(1.))
                                        .font_semibold()
                                        .child(title),
                                )
                            })
                            .children(self.close_button.then(|| {
                                let top = (paddings.top - px(10.)).max(px(8.));
                                let right = (paddings.right - px(10.)).max(px(8.));

                                Button::new("close")
                                    .debug_selector(|| "dialog-close".into())
                                    .absolute()
                                    .top(top)
                                    .right(right)
                                    .small()
                                    .ghost()
                                    .icon(IconName::Close)
                                    // The title overlaps this control; do not start a drag
                                    // that would capture the release before our click.
                                    .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                        cx.stop_propagation();
                                    })
                                    .on_click({
                                        let on_cancel = self.on_cancel.clone();
                                        let on_close = self.on_close.clone();
                                        move |_, window, cx| {
                                            on_cancel(&ClickEvent::default(), window, cx);
                                            on_close(&ClickEvent::default(), window, cx);
                                            window.close_dialog(cx);
                                        }
                                    })
                            }))
                            .child(
                                div().w_full().flex_1().min_h_0().overflow_hidden().child(
                                    v_flex()
                                        .id("contents")
                                        .pl(paddings.left)
                                        .pr(paddings.right)
                                        .when(placement.is_some(), |this| this.h_full().min_h_0())
                                        .child(self.content.when(placement.is_some(), |this| {
                                            this.h_full().min_h_0()
                                        }))
                                        .map(|this| {
                                            if placement.is_none() {
                                                this.overflow_y_scrollbar().into_any_element()
                                            } else {
                                                this.into_any_element()
                                            }
                                        }),
                                ),
                            )
                            .when_some(self.footer, |this, footer| {
                                this.child(
                                    h_flex()
                                        .gap_2()
                                        .pl(paddings.left)
                                        .pr(paddings.right)
                                        .line_height(relative(1.))
                                        .justify_end()
                                        .children(footer(render_ok, render_cancel, window, cx)),
                                )
                            })
                            .map(|this| {
                                if placement.is_some() {
                                    this.shadow_xl().into_any_element()
                                } else {
                                    this.with_animation(
                                        "slide-down",
                                        animation.clone(),
                                        move |this, delta| {
                                            let y_offset = px(0.) + delta * px(30.);
                                            // This is equivalent to `shadow_xl` with an extra opacity.
                                            let shadow = vec![
                                                BoxShadow {
                                                    color: hsla(0., 0., 0., 0.1 * delta),
                                                    offset: point(px(0.), px(20.)),
                                                    blur_radius: px(25.),
                                                    spread_radius: px(-5.),
                                                },
                                                BoxShadow {
                                                    color: hsla(0., 0., 0., 0.1 * delta),
                                                    offset: point(px(0.), px(8.)),
                                                    blur_radius: px(10.),
                                                    spread_radius: px(-6.),
                                                },
                                            ];
                                            this.top(y + y_offset).shadow(shadow)
                                        },
                                    )
                                    .into_any_element()
                                }
                            }),
                    )
                    .with_animation("fade-in", animation, move |this, delta| this.opacity(delta)),
            )
    }
}
