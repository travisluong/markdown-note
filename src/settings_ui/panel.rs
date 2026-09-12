//! Settings form state and events. Authoritative preferences stay with the app.
use gpui::{AppContext, Context, Entity, EventEmitter, Hsla, Subscription, Window, rgb};
use gpui_component::{
    Colorize, IndexPath,
    color_picker::{ColorPickerEvent, ColorPickerState},
    input::{InputEvent, InputState},
    select::{SearchableVec, SelectEvent, SelectState},
};
use mithril::{notebook, templates};

mod render;
#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug)]
pub(super) enum FontRole {
    Interface,
    Text,
    Monospace,
}
#[derive(Clone, Copy, Debug)]
pub(super) enum FontSizeRole {
    Content,
    Interface,
}
#[derive(Clone, Copy, Debug)]
pub(super) enum ColorRole {
    Background,
    Font,
    Primary,
}

#[derive(Clone, Debug)]
pub(super) enum SettingsIntent {
    AutosaveEnabled(bool),
    AutosaveDelay(u64),
    ShowExtensions(bool),
    HiddenExtensions(Vec<String>),
    DateFormat(String),
    Font(FontRole, Option<String>),
    FontSizeStep(FontSizeRole, i8),
    Theme(notebook::ThemeChoice),
    Color(ColorRole, u32),
    ResetPalettes,
    Close,
}
impl SettingsIntent {
    pub(super) fn is_autosave(&self) -> bool {
        matches!(self, Self::AutosaveEnabled(_) | Self::AutosaveDelay(_))
    }
}

/// A display snapshot, never written back wholesale or used as disk evidence.
#[derive(Clone, PartialEq)]
pub(super) struct SettingsValues {
    pub appearance: notebook::Appearance,
    pub autosave: notebook::Autosave,
    pub date_format: String,
    pub show_extensions: bool,
    pub hidden_extensions: Vec<String>,
    pub startup_dark: bool,
    pub busy: bool,
    pub subtitle: String,
    pub notice: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Section {
    General,
    Font,
    Templates,
    Theme,
}

type FontPicker = Entity<SelectState<SearchableVec<String>>>;

pub(super) struct SettingsPanel {
    values: SettingsValues,
    section: Section,
    hidden_extensions: Entity<InputState>,
    date: Entity<InputState>,
    delay: Entity<InputState>,
    hidden_extensions_error: Option<String>,
    date_error: Option<String>,
    delay_error: Option<String>,
    colors: [Entity<ColorPickerState>; 3],
    fonts: [FontPicker; 3],
    _subscriptions: Vec<Subscription>,
}
impl EventEmitter<SettingsIntent> for SettingsPanel {}

impl SettingsPanel {
    pub(super) fn new(
        values: SettingsValues,
        installed_fonts: &[String],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let hidden_extensions = cx.new(|cx| {
            InputState::new(window, cx).default_value(values.hidden_extensions.join(", "))
        });
        let date =
            cx.new(|cx| InputState::new(window, cx).default_value(values.date_format.clone()));
        let delay = cx.new(|cx| {
            InputState::new(window, cx).default_value(values.autosave.delay_ms.to_string())
        });
        let mut subscriptions = vec![
            cx.subscribe(&hidden_extensions, |this, input, event, cx| {
                if matches!(event, InputEvent::Change) {
                    this.edit_hidden_extensions(input.read(cx).value().as_ref(), cx);
                }
            }),
            cx.subscribe(&date, |this, input, event, cx| {
                if matches!(event, InputEvent::Change) {
                    this.edit_date(input.read(cx).value().as_ref(), cx);
                }
            }),
            cx.subscribe(&delay, |this, input, event, cx| {
                if matches!(event, InputEvent::Change) {
                    this.edit_delay(input.read(cx).value().as_ref(), cx);
                }
            }),
        ];
        let palette = values.appearance.palette(values.startup_dark);
        let colors = [
            (ColorRole::Background, palette.background),
            (ColorRole::Font, palette.font),
            (ColorRole::Primary, palette.primary),
        ]
        .map(|(role, value)| {
            let picker = cx.new(|cx| ColorPickerState::new(window, cx).default_value(rgb(value)));
            subscriptions.push(cx.subscribe(
                &picker,
                move |this, _, event: &ColorPickerEvent, cx| {
                    let ColorPickerEvent::Change(color) = event;
                    this.change_color(role, *color, cx);
                },
            ));
            picker
        });
        let fonts = [
            (FontRole::Interface, &values.appearance.interface_font),
            (FontRole::Text, &values.appearance.text_font),
            (FontRole::Monospace, &values.appearance.monospace_font),
        ]
        .map(|(role, selected)| {
            let mut fonts = installed_fonts.to_vec();
            fonts.sort_by_key(|font| font.to_lowercase());
            fonts.dedup();
            if let Some(name) = selected
                && !fonts.contains(name)
            {
                fonts.push(name.clone());
            }
            fonts.insert(0, "Default".to_owned());
            let row = selected
                .as_ref()
                .and_then(|name| fonts.iter().position(|font| font == name))
                .unwrap_or(0);
            let picker = cx.new(|cx| {
                SelectState::new(
                    SearchableVec::new(fonts),
                    Some(IndexPath::default().row(row)),
                    window,
                    cx,
                )
                .searchable(true)
            });
            subscriptions.push(cx.subscribe(
                &picker,
                move |this, _, event: &SelectEvent<SearchableVec<String>>, cx| {
                    let SelectEvent::Confirm(value) = event;
                    this.choose_font(role, value.clone().filter(|name| name != "Default"), cx);
                },
            ));
            picker
        });
        Self {
            values,
            section: Section::General,
            hidden_extensions,
            date,
            delay,
            hidden_extensions_error: None,
            date_error: None,
            delay_error: None,
            colors,
            fonts,
            _subscriptions: subscriptions,
        }
    }

    /// Acknowledges effective values without replacing raw or invalid input drafts.
    pub(super) fn sync_effective(
        &mut self,
        values: SettingsValues,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let old_palette = self.values.appearance.palette(self.values.startup_dark);
        let palette = values.appearance.palette(values.startup_dark);
        if old_palette != palette {
            for (picker, color) in
                self.colors
                    .iter()
                    .zip([palette.background, palette.font, palette.primary])
            {
                picker.update(cx, |picker, cx| picker.set_value(rgb(color), window, cx));
            }
        }
        if self.values != values {
            self.values = values;
            cx.notify();
        }
    }

    fn emit_intent(&mut self, intent: SettingsIntent, cx: &mut Context<Self>) {
        // Autosave preferences remain editable while a foreground operation runs.
        // The application independently checks the current scope and capability.
        if !self.values.busy || intent.is_autosave() || matches!(intent, SettingsIntent::Close) {
            cx.emit(intent);
        }
    }

    fn edit_hidden_extensions(&mut self, value: &str, cx: &mut Context<Self>) {
        if self.values.busy || !self.values.show_extensions {
            return;
        }
        match notebook::parse_hidden_extensions(value) {
            Ok(extensions) => {
                self.hidden_extensions_error = None;
                self.emit_intent(SettingsIntent::HiddenExtensions(extensions), cx);
            }
            Err(error) => self.hidden_extensions_error = Some(error),
        }
        cx.notify();
    }
    fn edit_date(&mut self, value: &str, cx: &mut Context<Self>) {
        match templates::current(value) {
            Ok(_) if !self.values.busy => {
                self.date_error = None;
                self.emit_intent(SettingsIntent::DateFormat(value.to_owned()), cx);
            }
            Err(error) => self.date_error = Some(error),
            _ => {}
        }
        cx.notify();
    }
    fn edit_delay(&mut self, value: &str, cx: &mut Context<Self>) {
        if let Some(delay) = notebook::Autosave::parse_delay(value) {
            self.delay_error = None;
            self.emit_intent(SettingsIntent::AutosaveDelay(delay), cx);
        } else {
            self.delay_error = Some("Enter a whole number from 100 to 60,000 ms.".into());
        }
        cx.notify();
    }
    fn change_color(&mut self, role: ColorRole, color: Option<Hsla>, cx: &mut Context<Self>) {
        if let Some(value) =
            color.and_then(|color| u32::from_str_radix(&color.to_hex()[1..7], 16).ok())
        {
            self.emit_intent(SettingsIntent::Color(role, value), cx);
        }
    }
    fn choose_font(&mut self, role: FontRole, value: Option<String>, cx: &mut Context<Self>) {
        self.emit_intent(SettingsIntent::Font(role, value), cx);
    }
    fn select_section(&mut self, section: Section, cx: &mut Context<Self>) {
        self.section = section;
        cx.notify();
    }

    #[cfg(feature = "editor-diagnostics")]
    pub(super) fn diagnostic_set_delay(
        &self,
        value: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.delay.update(cx, |input, cx| {
            input.set_value(value.to_owned(), window, cx)
        });
    }
    #[cfg(feature = "editor-diagnostics")]
    pub(super) fn diagnostic_delay_invalid(&self) -> bool {
        self.delay_error.is_some()
    }
}
