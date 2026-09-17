use std::sync::Arc;

use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, App, Bounds, ClipboardItem, Context, Corner, DismissEvent, Element, ElementId,
    Entity, EntityId, FocusHandle, GlobalElementId, InspectorElementId, InteractiveElement,
    IntoElement, KeyBinding, LayoutId, ListState, MouseDownEvent, MouseMoveEvent, MouseUpEvent,
    ParentElement, Pixels, Point, RenderOnce, SharedString, Size, StyleRefinement, Styled,
    Subscription, Task, Window, anchored, deferred, div, px,
};

#[cfg(feature = "editor-diagnostics")]
use crate::ActiveTheme;
use crate::highlighter::HighlightTheme;
use crate::menu::{PopupMenu, PopupMenuItem};
use crate::scroll::ScrollableElement;
use crate::text::node::CodeBlock;
use crate::{StyledExt, v_flex};
use crate::{
    global_state::GlobalState,
    input::{self},
    text::{
        TextViewStyle,
        node::{self, NodeContext},
    },
};

pub(crate) type LinkOpenFn = dyn Fn(&str, &str, &mut Window, &mut App) + Send + Sync;

pub(crate) type TaskToggleFn = dyn Fn(usize, &str, &mut Window, &mut App) + Send + Sync;

const CONTEXT: &'static str = "TextView";

pub(crate) fn init(cx: &mut App) {
    cx.bind_keys(vec![
        #[cfg(target_os = "macos")]
        KeyBinding::new("cmd-c", input::Copy, Some(CONTEXT)),
        #[cfg(not(target_os = "macos"))]
        KeyBinding::new("ctrl-c", input::Copy, Some(CONTEXT)),
    ]);
}

#[derive(IntoElement, Clone)]
struct TextViewElement {
    style: TextViewStyle,
    code_block_actions: Option<Arc<CodeBlockActionsFn>>,
    image_document: Option<std::path::PathBuf>,
    link_open: Option<Arc<LinkOpenFn>>,
    task_toggle: Option<Arc<TaskToggleFn>>,
    source_highlights: Option<super::SourceHighlights>,
    source_snapshot: SharedString,
    list_state: Option<ListState>,
    state: Entity<TextViewState>,
}

impl RenderOnce for TextViewElement {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        self.state.update(cx, |state, cx| {
            if let Some(Ok(content)) = &mut state.parsed_result {
                // Presentation is live configuration; source belongs to the
                // rendered tree, even while a newer parse is pending.
                content.node_cx.style = self.style.clone();
                content.node_cx.code_block_actions = self.code_block_actions.clone();
                content.node_cx.image_document = self.image_document.clone();
                content.node_cx.link_open = self.link_open.clone();
                content.node_cx.link_menu_owner = Some(self.state.downgrade());
                content.node_cx.task_toggle = self.task_toggle.clone();
                content.node_cx.highlights = self.source_highlights.clone().filter(|search| {
                    search.matches_snapshot(&content.node_cx.source)
                        && search.matches_snapshot(&self.source_snapshot)
                });
                if let Some(search) = &content.node_cx.highlights {
                    let hidden = search
                        .active
                        .as_ref()
                        .is_some_and(|range| !content.root_node.has_visible_source(range));
                    if search.hidden_match() != hidden {
                        search.set_hidden(hidden);
                        if let Some(parent) = state.parent_entity {
                            let app: &mut App = cx;
                            app.notify(parent);
                        }
                    }
                }
            }
            v_flex()
                .size_full()
                .map(|this| match &mut state.parsed_result {
                    Some(Ok(content)) => this.child(content.root_node.render_root(
                        self.list_state.clone(),
                        &content.node_cx,
                        window,
                        cx,
                    )),
                    Some(Err(err)) => this.child(
                        v_flex()
                            .gap_1()
                            .child("Failed to parse content")
                            .child(err.to_string()),
                    ),
                    None => this,
                })
        })
    }
}

/// Type for code block actions generator function.
pub(crate) type CodeBlockActionsFn =
    dyn Fn(&CodeBlock, &mut Window, &mut App) -> AnyElement + Send + Sync;

/// A text view that can render Markdown or HTML.
///
/// ## Goals
///
/// - Provide a rich text rendering component for such as Markdown or HTML,
/// used to display rich text in GPUI application (e.g., Help messages, Release notes)
/// - Support Markdown GFM and HTML (Simple HTML like Safari Reader Mode) for showing most common used markups.
/// - Support Heading, Paragraph, Bold, Italic, StrikeThrough, Code, Link, Image, Blockquote, List, Table, HorizontalRule, CodeBlock ...
///
/// ## Not Goals
///
/// - Customization of the complex style (some simple styles will be supported)
/// - As a Markdown editor or viewer (If you want to like this, you must fork your version).
/// - As a HTML viewer, we not support CSS, we only support basic HTML tags for used to as a content reader.
///
/// See also [`MarkdownElement`], [`HtmlElement`]
#[derive(Clone)]
pub struct TextView {
    image_document: Option<std::path::PathBuf>,
    link_open: Option<Arc<LinkOpenFn>>,
    task_toggle: Option<Arc<TaskToggleFn>>,
    source_highlights: Option<super::SourceHighlights>,
    id: ElementId,
    type_: TextViewType,
    text_style: TextViewStyle,
    raw: SharedString,
    state: Entity<TextViewState>,
    style: StyleRefinement,
    selectable: bool,
    persistent_selection: bool,
    scrollable: bool,
    code_block_actions: Option<Arc<CodeBlockActionsFn>>,
    #[cfg(feature = "editor-diagnostics")]
    parse_control: Option<ReadingParseControl>,
}

impl gpui::Focusable for TextView {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.state
            .read(cx)
            .focus_handle
            .clone()
            .expect("TextView focus handle")
    }
}

#[derive(PartialEq)]
pub(crate) struct ParsedContent {
    pub(crate) root_node: node::Node,
    pub(crate) node_cx: node::NodeContext,
}

/// The type of the text view.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TextViewType {
    /// Markdown view
    Markdown,
    /// HTML view
    Html,
}

#[derive(Clone)]
struct ParseRequest {
    generation: u64,
    type_: TextViewType,
    text: SharedString,
}

impl ParseRequest {
    fn parse(&self) -> Result<ParsedContent, SharedString> {
        // AST construction and code tokens depend only on source/type. Heading
        // callbacks, spacing, code-block styles/actions and token colors are
        // consumed during rendering, so fresh closures never schedule parsing.
        parse_content(
            self.type_,
            &self.text,
            TextViewStyle::default(),
            &HighlightTheme::default_light(),
            &None,
        )
    }
}

/// Deterministic worker gate for headless Reading lifecycle diagnostics.
#[cfg(feature = "editor-diagnostics")]
#[doc(hidden)]
#[derive(Clone)]
pub struct ReadingParseControl {
    inner: Arc<ReadingParseControlInner>,
}

#[cfg(feature = "editor-diagnostics")]
struct ReadingParseControlInner {
    started: std::sync::atomic::AtomicUsize,
    tx: smol::channel::Sender<Result<(), SharedString>>,
    rx: smol::channel::Receiver<Result<(), SharedString>>,
}

#[cfg(feature = "editor-diagnostics")]
impl Default for ReadingParseControl {
    fn default() -> Self {
        let (tx, rx) = smol::channel::bounded(1);
        Self {
            inner: Arc::new(ReadingParseControlInner {
                started: Default::default(),
                tx,
                rx,
            }),
        }
    }
}

#[cfg(feature = "editor-diagnostics")]
impl ReadingParseControl {
    pub fn started(&self) -> usize {
        self.inner
            .started
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn complete(&self, result: Result<(), SharedString>) {
        self.inner
            .tx
            .try_send(result)
            .expect("one completion per running parse");
    }
}

/// Headless observation and interaction with the actual TextView state, without retaining it.
#[cfg(feature = "editor-diagnostics")]
#[doc(hidden)]
#[derive(Clone)]
pub struct ReadingProbe(gpui::WeakEntity<TextViewState>);

#[cfg(feature = "editor-diagnostics")]
#[doc(hidden)]
pub struct ReadingSnapshot {
    pub generation: u64,
    pub running: Option<u64>,
    pub pending: Option<u64>,
    pub displayed_source: Option<SharedString>,
    pub error: Option<SharedString>,
    pub style: Option<TextViewStyle>,
    pub has_highlights: bool,
    pub search_started: bool,
    pub scroll_item: usize,
    pub item_count: usize,
}

#[cfg(feature = "editor-diagnostics")]
impl ReadingProbe {
    pub fn link_positions(&self, cx: &App) -> Vec<(String, Point<Pixels>)> {
        self.0.upgrade().unwrap().read(cx).diagnostic_links.clone()
    }

    pub fn link_menu(&self, cx: &App) -> Option<Entity<PopupMenu>> {
        self.0
            .upgrade()
            .unwrap()
            .read(cx)
            .link_menu
            .as_ref()
            .map(|(_, menu)| menu.clone())
    }

    pub fn selection(&self, cx: &App) -> (Option<Point<Pixels>>, Option<Point<Pixels>>) {
        self.0.upgrade().unwrap().read(cx).selection_positions
    }

    pub fn is_focused(&self, window: &Window, cx: &App) -> bool {
        self.0
            .upgrade()
            .unwrap()
            .read(cx)
            .focus_handle
            .as_ref()
            .unwrap()
            .is_focused(window)
    }
    pub fn snapshot(&self, cx: &App) -> Option<ReadingSnapshot> {
        let entity = self.0.upgrade()?;
        let state = entity.read(cx);
        let content = state
            .parsed_result
            .as_ref()
            .and_then(|result| result.as_ref().ok());
        Some(ReadingSnapshot {
            generation: state
                .latest_request
                .as_ref()
                .map_or(0, |request| request.generation),
            running: state.running,
            pending: state.pending.as_ref().map(|request| request.generation),
            displayed_source: content.map(|content| content.node_cx.source.clone()),
            error: state
                .parsed_result
                .as_ref()
                .and_then(|result| result.as_ref().err())
                .cloned(),
            style: content.map(|content| content.node_cx.style.clone()),
            has_highlights: content.is_some_and(|content| content.node_cx.highlights.is_some()),
            search_started: state
                .active_search
                .as_ref()
                .is_some_and(|search| search.started.load(std::sync::atomic::Ordering::Relaxed)),
            scroll_item: state.list_state.logical_scroll_top().item_ix,
            item_count: state.list_state.item_count(),
        })
    }

    pub fn scroll_to(&self, item_ix: usize, cx: &App) {
        self.0
            .upgrade()
            .expect("live Reading view")
            .read(cx)
            .list_state
            .scroll_to(gpui::ListOffset {
                item_ix,
                offset_in_item: px(0.),
            });
    }

    pub fn item_is_vertically_visible(&self, item_ix: usize, cx: &App) -> bool {
        let entity = self.0.upgrade().expect("live Reading view");
        let list = &entity.read(cx).list_state;
        let viewport = list.viewport_bounds();
        list.bounds_for_item(item_ix).is_some_and(|bounds| {
            bounds.top() >= viewport.top() && bounds.bottom() <= viewport.bottom()
        })
    }

    pub fn activate_rendered_task(&self, offset: usize, window: &mut Window, cx: &mut App) {
        let (callback, source) = {
            let entity = self.0.upgrade().expect("live Reading view");
            let content = entity
                .read(cx)
                .parsed_result
                .as_ref()
                .expect("parsed Reading view")
                .as_ref()
                .expect("valid Reading content");
            (
                content.node_cx.task_toggle.clone().expect("task callback"),
                content.node_cx.source.clone(),
            )
        };
        callback(offset, &source, window, cx);
    }
}

pub(crate) struct TextViewState {
    #[cfg(feature = "editor-diagnostics")]
    pub(super) diagnostic_links: Vec<(String, Point<Pixels>)>,
    link_menu: Option<(Point<Pixels>, Entity<PopupMenu>)>,
    link_menu_subscription: Option<Subscription>,
    pub(super) source_snapshot: SharedString,
    pub(super) active_search: Option<super::SourceHighlights>,
    parent_entity: Option<EntityId>,
    latest_request: Option<ParseRequest>,
    running: Option<u64>,
    pending: Option<ParseRequest>,
    parse_task: Option<Task<()>>,
    #[cfg(feature = "editor-diagnostics")]
    parse_control: Option<ReadingParseControl>,
    parsed_result: Option<Result<ParsedContent, SharedString>>,
    focus_handle: Option<FocusHandle>,
    /// The bounds of the text view
    bounds: Bounds<Pixels>,
    /// The local (in TextView) position of the selection.
    selection_positions: (Option<Point<Pixels>>, Option<Point<Pixels>>),
    /// Is current in selection.
    is_selecting: bool,
    is_selectable: bool,
    pub(super) list_state: ListState,
}

impl TextViewState {
    fn new(cx: &mut Context<TextViewState>) -> Self {
        let focus_handle = cx.focus_handle();
        Self {
            parent_entity: None,
            #[cfg(feature = "editor-diagnostics")]
            diagnostic_links: Vec::new(),
            link_menu: None,
            link_menu_subscription: None,
            source_snapshot: SharedString::default(),
            active_search: None,
            latest_request: None,
            running: None,
            pending: None,
            parse_task: None,
            #[cfg(feature = "editor-diagnostics")]
            parse_control: None,
            parsed_result: None,
            focus_handle: Some(focus_handle),
            bounds: Bounds::default(),
            selection_positions: (None, None),
            is_selecting: false,
            is_selectable: false,
            list_state: ListState::new(0, gpui::ListAlignment::Top, px(1000.)),
        }
    }
}

impl TextViewState {
    pub(super) fn open_link_menu(
        &mut self,
        destination: SharedString,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let focus = self.focus_handle.clone().expect("Reading focus handle");
        let menu = PopupMenu::build(window, cx, |mut menu, window, _| {
            menu.menu_items
                .push(PopupMenuItem::new("Copy link").on_click(move |_, _, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(destination.to_string()));
                }));
            menu.focus_handle.focus(window);
            menu.action_context(focus).small()
        });
        self.link_menu_subscription =
            Some(
                cx.subscribe_in(&menu, window, |this, dismissed, _: &DismissEvent, _, cx| {
                    if this
                        .link_menu
                        .as_ref()
                        .is_some_and(|(_, menu)| menu == dismissed)
                    {
                        this.link_menu = None;
                        this.link_menu_subscription = None;
                        if let Some(parent) = this.parent_entity {
                            App::notify(cx, parent);
                        }
                    }
                }),
            );
        self.is_selecting = false;
        self.link_menu = Some((position, menu));
        if let Some(parent) = self.parent_entity {
            App::notify(cx, parent);
        }
    }

    fn request_parse(&mut self, type_: TextViewType, text: SharedString, cx: &mut Context<Self>) {
        if self.latest_request.as_ref().is_some_and(|latest| {
            latest.type_ == type_
                && (std::ptr::eq(latest.text.as_str(), text.as_str()) || latest.text == text)
        }) {
            return;
        }
        let generation = self.latest_request.as_ref().map_or(1, |latest| {
            latest
                .generation
                .checked_add(1)
                .expect("text view generation exhausted")
        });
        let request = ParseRequest {
            generation,
            type_,
            text,
        };
        self.latest_request = Some(request.clone());
        self.pending = Some(request);
        self.start_parse(cx);
    }

    fn start_parse(&mut self, cx: &mut Context<Self>) {
        if self.running.is_some() {
            return;
        }
        let Some(request) = self.pending.take() else {
            return;
        };
        self.running = Some(request.generation);
        #[cfg(feature = "editor-diagnostics")]
        let control = self.parse_control.clone();
        let worker = cx.background_executor().spawn(async move {
            #[cfg(feature = "editor-diagnostics")]
            if let Some(control) = control {
                control
                    .inner
                    .started
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if let Err(error) = control
                    .inner
                    .rx
                    .recv()
                    .await
                    .unwrap_or_else(|_| Err("parse control closed".into()))
                {
                    return (request.generation, Err(error));
                }
            }
            (request.generation, request.parse())
        });
        // The task owns only a weak entity reference. Dropping the view cancels
        // delivery; a parse already executing may finish on the worker.
        self.parse_task = Some(cx.spawn(async move |this, cx| {
            let (generation, result) = worker.await;
            let _ = this.update(cx, |this, cx| {
                if this.running != Some(generation) {
                    return;
                }
                this.running = None;
                if this
                    .latest_request
                    .as_ref()
                    .is_some_and(|latest| latest.generation == generation)
                {
                    this.parsed_result = Some(result);
                    this.clear_selection();
                    if let Some(parent) = this.parent_entity {
                        let app: &mut App = cx;
                        app.notify(parent);
                    }
                    cx.notify();
                }
                this.start_parse(cx);
            });
        }));
    }

    /// Save bounds and unselect if bounds changed.
    fn update_bounds(&mut self, bounds: Bounds<Pixels>) {
        if self.bounds.size != bounds.size {
            self.clear_selection();
        }
        self.bounds = bounds;
    }

    fn clear_selection(&mut self) {
        self.selection_positions = (None, None);
        self.is_selecting = false;
    }

    fn start_selection(&mut self, pos: Point<Pixels>) {
        let pos = pos - self.bounds.origin;
        self.selection_positions = (Some(pos), Some(pos));
        self.is_selecting = true;
    }

    fn update_selection(&mut self, pos: Point<Pixels>) {
        let pos = pos - self.bounds.origin;
        if let (Some(start), Some(_)) = self.selection_positions {
            self.selection_positions = (Some(start), Some(pos))
        }
    }

    fn end_selection(&mut self) {
        self.is_selecting = false;
    }

    pub(crate) fn has_selection(&self) -> bool {
        if let (Some(start), Some(end)) = self.selection_positions {
            start != end
        } else {
            false
        }
    }

    pub(crate) fn is_selectable(&self) -> bool {
        self.is_selectable
    }

    /// Return the bounds of the selection in window coordinates.
    pub(crate) fn selection_bounds(&self) -> Bounds<Pixels> {
        selection_bounds(
            self.selection_positions.0,
            self.selection_positions.1,
            self.bounds,
        )
    }

    fn selection_text(&self) -> Option<String> {
        Some(
            self.parsed_result
                .as_ref()?
                .as_ref()
                .ok()?
                .root_node
                .selected_text(),
        )
    }
}

#[derive(IntoElement, Clone)]
pub enum Text {
    String(SharedString),
    TextView(Box<TextView>),
}

impl From<SharedString> for Text {
    fn from(s: SharedString) -> Self {
        Self::String(s)
    }
}

impl From<&str> for Text {
    fn from(s: &str) -> Self {
        Self::String(SharedString::from(s.to_string()))
    }
}

impl From<String> for Text {
    fn from(s: String) -> Self {
        Self::String(s.into())
    }
}

impl From<TextView> for Text {
    fn from(e: TextView) -> Self {
        Self::TextView(Box::new(e))
    }
}

impl Text {
    /// Set the style for [`TextView`].
    ///
    /// Do nothing if this is `String`.
    pub fn style(self, style: TextViewStyle) -> Self {
        match self {
            Self::String(s) => Self::String(s),
            Self::TextView(e) => Self::TextView(Box::new(e.style(style))),
        }
    }

    /// Get the str
    pub fn as_str(&self) -> &str {
        match self {
            Self::String(s) => s.as_str(),
            Self::TextView(view) => view.raw.as_str(),
        }
    }
}

impl RenderOnce for Text {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        match self {
            Self::String(s) => s.into_any_element(),
            Self::TextView(e) => e.into_any_element(),
        }
    }
}

impl Styled for TextView {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl TextView {
    #[cfg(feature = "editor-diagnostics")]
    #[doc(hidden)]
    pub fn diagnostic_parse_control(mut self, control: ReadingParseControl) -> Self {
        self.parse_control = Some(control);
        self
    }

    #[cfg(feature = "editor-diagnostics")]
    #[doc(hidden)]
    pub fn diagnostic_probe(&self) -> ReadingProbe {
        ReadingProbe(self.state.downgrade())
    }

    #[cfg(feature = "editor-diagnostics")]
    #[doc(hidden)]
    pub fn diagnostic_task_mappings(cx: &App) {
        fn offsets(node: &node::Node, found: &mut Vec<usize>) {
            match node {
                node::Node::ListItem {
                    task_offset,
                    children,
                    ..
                } => {
                    found.extend(task_offset.iter().copied());
                    for child in children {
                        offsets(child, found);
                    }
                }
                node::Node::Root { children }
                | node::Node::Blockquote { children }
                | node::Node::List { children, .. } => {
                    for child in children {
                        offsets(child, found);
                    }
                }
                _ => {}
            }
        }
        let source =
            "# café 你好\n\n- [ ] plain\n- [X] upper\n\n> - [x] quoted\n>   - [ ] nested\n";
        let content = parse_content(
            TextViewType::Markdown,
            source,
            TextViewStyle::default(),
            &cx.theme().highlight_theme,
            &None,
        )
        .unwrap();
        let mut actual = Vec::new();
        offsets(&content.root_node, &mut actual);
        let expected = source
            .match_indices('[')
            .map(|(i, _)| i + 1)
            .collect::<Vec<_>>();
        assert_eq!(actual, expected, "reading task UTF-8 offsets");
        assert_eq!(content.node_cx.source.as_ref(), source);
        fn lists(node: &node::Node, depth: usize, found: &mut Vec<(usize, bool)>) {
            match node {
                node::Node::List { children, ordered } => {
                    found.push((depth, *ordered));
                    for child in children {
                        lists(child, depth + 1, found);
                    }
                }
                node::Node::Root { children }
                | node::Node::Blockquote { children }
                | node::Node::ListItem { children, .. } => {
                    for child in children {
                        lists(child, depth, found);
                    }
                }
                _ => {}
            }
        }
        let source = "10. parent\n    - child\n      1. [X] 你好\n11. sibling";
        let content = parse_content(
            TextViewType::Markdown,
            source,
            TextViewStyle::default(),
            &cx.theme().highlight_theme,
            &None,
        )
        .unwrap();
        let mut hierarchy = Vec::new();
        lists(&content.root_node, 0, &mut hierarchy);
        assert_eq!(hierarchy, vec![(0, true), (1, false), (2, true)]);
        let mut actual = Vec::new();
        offsets(&content.root_node, &mut actual);
        assert_eq!(actual, vec![source.find('[').unwrap() + 1]);
    }
    #[cfg(feature = "editor-diagnostics")]
    #[doc(hidden)]
    pub fn diagnostic_reading_search() {
        node::CodeBlock::diagnostic_highlighting();
        fn collect(node: &node::Node, range: &std::ops::Range<usize>, output: &mut String) {
            fn paragraph(p: &node::Paragraph, range: &std::ops::Range<usize>, output: &mut String) {
                for child in &p.children {
                    for display in super::source_map::project(&child.source_runs, range) {
                        output.push_str(&child.text[display]);
                    }
                }
            }
            match node {
                node::Node::Paragraph(p) | node::Node::Heading { children: p, .. } => {
                    paragraph(p, range, output)
                }
                node::Node::Root { children }
                | node::Node::List { children, .. }
                | node::Node::ListItem { children, .. }
                | node::Node::Blockquote { children } => {
                    for child in children {
                        collect(child, range, output);
                    }
                }
                node::Node::CodeBlock(block) => {
                    for display in super::source_map::project(&block.source_runs, range) {
                        output.push_str(&block.code()[display]);
                    }
                }
                node::Node::Table(table) => {
                    for row in &table.children {
                        for cell in &row.children {
                            paragraph(&cell.children, range, output);
                        }
                    }
                }
                _ => {}
            }
        }
        for (source, needle, expected) in [
            ("# café **你好**", "你好", "你好"),
            (
                "a **bold *nested* word**",
                "bold *nested* word",
                "bold nested word",
            ),
            ("[label](https://example.test)", "label", "label"),
            ("[label](https://example.test)", "https://example.test", ""),
            (
                "[**label**][ref]\n\n[ref]: https://example.test",
                "label",
                "label",
            ),
            ("> - [ ] **quoted**\n>   - nested", "nested", "nested"),
            (
                "| a | b |\n| --- | --- |\n| **你好** | café |",
                "你好",
                "你好",
            ),
            ("```\n你好\nsecond\n```", "second", "second"),
            (
                "> ```rs\n> let x = \"&amp;\\*你好\";\n> ```",
                "&amp;\\*你好",
                "&amp;\\*你好",
            ),
            (
                "```rs\r\n/* café\r\n你好 */\r\n```",
                "café\r\n你好",
                "café\r\n你好",
            ),
            ("```\nfirst\nsecond\n```", "first\nsecond", "first\nsecond"),
            ("`a b`", "a b", "a b"),
            ("some &amp; &#x4f60; text", "&amp;", "&"),
            ("some &amp; &#x4f60; text", "&#x4f60;", "你"),
            ("some \\*literal\\*", "literal", "literal"),
            ("**bold**", "**", ""),
        ] {
            let content = parse_content(
                TextViewType::Markdown,
                source,
                TextViewStyle::default(),
                &HighlightTheme::default_light(),
                &None,
            )
            .unwrap();
            let start = source.find(needle).unwrap();
            let range = start..start + needle.len();
            let mut actual = String::new();
            collect(&content.root_node, &range, &mut actual);
            assert_eq!(actual, expected, "source={source:?}, needle={needle:?}");
            assert_eq!(
                content.root_node.has_visible_source(&range),
                !expected.is_empty()
            );
            assert!(super::source_map::containing_block(&content.node_cx.blocks, &range).is_some());
        }
        let source = (0..500)
            .map(|n| format!("paragraph {n}\n\n"))
            .collect::<String>();
        let content = parse_content(
            TextViewType::Markdown,
            &source,
            TextViewStyle::default(),
            &HighlightTheme::default_light(),
            &None,
        )
        .unwrap();
        let offset = source.find("paragraph 499").unwrap();
        assert_eq!(
            super::source_map::containing_block(&content.node_cx.blocks, &(offset..offset + 13)),
            Some(499)
        );
        let search = super::SourceHighlights::new(
            source.clone().into(),
            vec![offset..offset + 13],
            Some(offset..offset + 13),
        );
        assert!(search.matches_snapshot(&source));
        assert!(!search.matches_snapshot("another file"));
    }

    /// Resolve local images relative to this Markdown document.
    pub fn image_document(mut self, path: impl Into<std::path::PathBuf>) -> Self {
        self.image_document = Some(path.into());
        self
    }

    /// Handle link activation with its destination and exact rendered source snapshot.
    pub fn on_link_open(
        mut self,
        callback: impl Fn(&str, &str, &mut Window, &mut App) + Send + Sync + 'static,
    ) -> Self {
        self.link_open = Some(Arc::new(callback));
        self
    }

    /// Opt in to task checkbox clicks. Offset and source belong to the rendered parse.
    pub fn on_task_toggle(
        mut self,
        callback: impl Fn(usize, &str, &mut Window, &mut App) + Send + Sync + 'static,
    ) -> Self {
        self.task_toggle = Some(Arc::new(callback));
        self
    }
    /// Create a new markdown text view.
    pub fn markdown(
        id: impl Into<ElementId>,
        markdown: impl Into<SharedString>,
        window: &mut Window,
        cx: &mut App,
    ) -> Self {
        let id: ElementId = id.into();
        let markdown = markdown.into();
        let state =
            window.use_keyed_state(SharedString::from(format!("{}/state", id)), cx, |_, cx| {
                TextViewState::new(cx)
            });
        state.update(cx, |state, _| state.source_snapshot = markdown.clone());
        Self {
            id,
            type_: TextViewType::Markdown,
            text_style: TextViewStyle::default(),
            raw: markdown.clone(),
            style: StyleRefinement::default(),
            state,
            selectable: false,
            persistent_selection: false,
            scrollable: false,
            code_block_actions: None,
            image_document: None,
            link_open: None,
            task_toggle: None,
            source_highlights: None,
            #[cfg(feature = "editor-diagnostics")]
            parse_control: None,
        }
    }

    /// Create a new html text view.
    pub fn html(
        id: impl Into<ElementId>,
        html: impl Into<SharedString>,
        window: &mut Window,
        cx: &mut App,
    ) -> Self {
        let id: ElementId = id.into();
        let html = html.into();
        let state =
            window.use_keyed_state(SharedString::from(format!("{}/state", id)), cx, |_, cx| {
                TextViewState::new(cx)
            });
        state.update(cx, |state, _| state.source_snapshot = html.clone());
        Self {
            id,
            type_: TextViewType::Html,
            text_style: TextViewStyle::default(),
            style: StyleRefinement::default(),
            state,
            raw: html,
            selectable: false,
            persistent_selection: false,
            scrollable: false,
            code_block_actions: None,
            image_document: None,
            link_open: None,
            task_toggle: None,
            source_highlights: None,
            #[cfg(feature = "editor-diagnostics")]
            parse_control: None,
        }
    }

    /// Set the source text of the text view.
    pub fn text(mut self, raw: impl Into<SharedString>) -> Self {
        self.raw = raw.into();
        self
    }

    /// Set [`TextViewStyle`].
    pub fn style(mut self, style: TextViewStyle) -> Self {
        self.text_style = style;
        self
    }

    /// Highlight and reveal source ranges only when their snapshot matches this view.
    pub fn source_highlights(mut self, highlights: Option<super::SourceHighlights>) -> Self {
        self.source_highlights = highlights;
        self
    }

    /// Set the text view to be selectable, default is false.
    pub fn selectable(mut self, selectable: bool) -> Self {
        self.selectable = selectable;
        self
    }

    /// Retain the selection when clicking outside this view, for application menus.
    /// Defaults to false; clicking within the view still starts a new selection.
    pub fn persistent_selection(mut self, persistent: bool) -> Self {
        self.persistent_selection = persistent;
        self
    }

    /// Set the text view to be scrollable, default is false.
    ///
    /// ## If true for `scrollable`
    ///
    /// The `scrollable` mode used for large content,
    /// will show scrollbar, but requires the parent to have a fixed height,
    /// and use [`gpui::list`] to render the content in a virtualized way.
    ///
    /// ## If false to fit content
    ///
    /// The TextView will expand to fit all content, no scrollbar.
    /// This mode is suitable for small content, such as a few lines of text, a label, etc.
    pub fn scrollable(mut self, scrollable: bool) -> Self {
        self.scrollable = scrollable;
        self
    }

    fn on_action_copy(state: &Entity<TextViewState>, cx: &mut App) {
        let Some(selected_text) = state.read(cx).selection_text() else {
            return;
        };

        cx.write_to_clipboard(ClipboardItem::new_string(selected_text.trim().to_string()));
    }

    /// Set custom block actions for code blocks.
    ///
    /// The closure receives the [`CodeBlock`],
    /// and returns an element to display.
    pub fn code_block_actions<F, E>(mut self, f: F) -> Self
    where
        F: Fn(&CodeBlock, &mut Window, &mut App) -> E + Send + Sync + 'static,
        E: IntoElement,
    {
        self.code_block_actions = Some(Arc::new(move |code_block, window, cx| {
            f(&code_block, window, cx).into_any_element()
        }));
        self
    }
}

impl IntoElement for TextView {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for TextView {
    type RequestLayoutState = AnyElement;
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        Some(self.id.clone())
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let parent_entity = window.current_view();
        self.state.update(cx, |state, cx| {
            // Register the notification owner before starting the cold parse.
            state.parent_entity = Some(parent_entity);
            state.source_snapshot = self.raw.clone();
            if let Some(previous) = &state.active_search {
                if !self
                    .source_highlights
                    .as_ref()
                    .is_some_and(|current| current.same_request(previous))
                {
                    previous
                        .cancelled
                        .store(true, std::sync::atomic::Ordering::Relaxed);
                }
            }
            state.active_search = self.source_highlights.clone();
            #[cfg(feature = "editor-diagnostics")]
            {
                state.parse_control = self.parse_control.clone();
            }
            state.request_parse(self.type_, self.raw.clone(), cx);
        });
        let list_state = &self.state.read(cx).list_state;

        let focus_handle = self
            .state
            .read(cx)
            .focus_handle
            .as_ref()
            .expect("focus_handle should init by TextViewState::new");

        let mut el = div()
            .key_context(CONTEXT)
            .track_focus(focus_handle)
            .size_full()
            .relative()
            .on_action({
                let state = self.state.clone();
                move |_: &input::Copy, _, cx| {
                    Self::on_action_copy(&state, cx);
                }
            })
            .child(TextViewElement {
                style: self.text_style.clone(),
                code_block_actions: self.code_block_actions.clone(),
                image_document: self.image_document.clone(),
                link_open: self.link_open.clone(),
                task_toggle: self.task_toggle.clone(),
                source_highlights: self.source_highlights.clone(),
                source_snapshot: self.raw.clone(),
                list_state: if self.scrollable {
                    Some(list_state.clone())
                } else {
                    None
                },
                state: self.state.clone(),
            })
            .refine_style(&self.style)
            .vertical_scrollbar(list_state)
            .children(
                self.state
                    .read(cx)
                    .link_menu
                    .clone()
                    .map(|(position, menu)| {
                        deferred(
                            anchored()
                                .position(position)
                                .anchor(Corner::TopLeft)
                                .snap_to_window_with_margin(px(8.))
                                .child(menu),
                        )
                    }),
            )
            .into_any_element();
        let layout_id = el.request_layout(window, cx);
        (layout_id, el)
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        request_layout: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        request_layout.prepaint(window, cx);
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        request_layout: &mut Self::RequestLayoutState,
        _: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        let entity_id = window.current_view();
        let is_selectable = self.selectable;

        self.state.update(cx, |state, _| {
            state.parent_entity = Some(entity_id);
            state.update_bounds(bounds);
            #[cfg(feature = "editor-diagnostics")]
            state.diagnostic_links.clear();
            state.is_selectable = is_selectable;
        });

        GlobalState::global_mut(cx)
            .text_view_state_stack
            .push(self.state.clone());
        request_layout.paint(window, cx);
        GlobalState::global_mut(cx).text_view_state_stack.pop();

        if self.selectable {
            let is_selecting = self.state.read(cx).is_selecting;
            let has_selection = self.state.read(cx).has_selection();

            window.on_mouse_event({
                let state = self.state.clone();
                move |event: &MouseDownEvent, phase, _, cx| {
                    if event.button != gpui::MouseButton::Left
                        || !bounds.contains(&event.position)
                        || !phase.bubble()
                    {
                        return;
                    }

                    state.update(cx, |state, _| {
                        state.start_selection(event.position);
                    });
                    cx.notify(entity_id);
                }
            });

            if is_selecting {
                // move to update end position.
                window.on_mouse_event({
                    let state = self.state.clone();
                    move |event: &MouseMoveEvent, phase, _, cx| {
                        if !phase.bubble() {
                            return;
                        }

                        state.update(cx, |state, _| {
                            state.update_selection(event.position);
                        });
                        cx.notify(entity_id);
                    }
                });

                // up to end selection
                window.on_mouse_event({
                    let state = self.state.clone();
                    move |_: &MouseUpEvent, phase, _, cx| {
                        if !phase.bubble() {
                            return;
                        }

                        state.update(cx, |state, _| {
                            state.end_selection();
                        });
                        cx.notify(entity_id);
                    }
                });
            }

            if has_selection && !self.persistent_selection {
                // down outside to clear selection
                window.on_mouse_event({
                    let state = self.state.clone();
                    move |event: &MouseDownEvent, _, _, cx| {
                        if bounds.contains(&event.position) || state.read(cx).link_menu.is_some() {
                            return;
                        }

                        state.update(cx, |state, _| {
                            state.clear_selection();
                        });
                        cx.notify(entity_id);
                    }
                });
            }
        }
    }
}

fn parse_content(
    type_: TextViewType,
    text: &str,
    style: TextViewStyle,
    highlight_theme: &HighlightTheme,
    code_block_actions: &Option<Arc<CodeBlockActionsFn>>,
) -> Result<ParsedContent, SharedString> {
    let mut node_cx = NodeContext {
        // Markdown sets this itself; HTML also needs the rendered snapshot for
        // callbacks and lifecycle identity even though it has no Markdown map.
        source: if type_ == TextViewType::Html {
            text.to_owned().into()
        } else {
            SharedString::default()
        },
        style: style.clone(),
        code_block_actions: code_block_actions.clone(),
        ..NodeContext::default()
    };

    let res = match type_ {
        TextViewType::Markdown => {
            super::format::markdown::parse(text, &style, &mut node_cx, highlight_theme)
        }
        TextViewType::Html => super::format::html::parse(text, &mut node_cx),
    };
    res.map(move |root_node| ParsedContent { root_node, node_cx })
}

fn selection_bounds(
    start: Option<Point<Pixels>>,
    end: Option<Point<Pixels>>,
    bounds: Bounds<Pixels>,
) -> Bounds<Pixels> {
    if let (Some(start), Some(end)) = (start, end) {
        let start = start + bounds.origin;
        let end = end + bounds.origin;

        let origin = Point {
            x: start.x.min(end.x),
            y: start.y.min(end.y),
        };
        let size = Size {
            width: (start.x - end.x).abs(),
            height: (start.y - end.y).abs(),
        };

        return Bounds { origin, size };
    }

    Bounds::default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Bounds, point, px, size};

    #[test]
    fn test_text_view_state_selection_bounds() {
        assert_eq!(
            selection_bounds(None, None, Default::default()),
            Bounds::default()
        );
        assert_eq!(
            selection_bounds(None, Some(point(px(10.), px(20.))), Default::default()),
            Bounds::default()
        );
        assert_eq!(
            selection_bounds(Some(point(px(10.), px(20.))), None, Default::default()),
            Bounds::default()
        );

        // 10,10 start
        //   |------|
        //   |      |
        //   |------|
        //         50,50
        assert_eq!(
            selection_bounds(
                Some(point(px(10.), px(10.))),
                Some(point(px(50.), px(50.))),
                Default::default()
            ),
            Bounds {
                origin: point(px(10.), px(10.)),
                size: size(px(40.), px(40.))
            }
        );
        // 10,10
        //   |------|
        //   |      |
        //   |------|
        //         50,50 start
        assert_eq!(
            selection_bounds(
                Some(point(px(50.), px(50.))),
                Some(point(px(10.), px(10.))),
                Default::default()
            ),
            Bounds {
                origin: point(px(10.), px(10.)),
                size: size(px(40.), px(40.))
            }
        );
        //        50,10 start
        //   |------|
        //   |      |
        //   |------|
        // 10,50
        assert_eq!(
            selection_bounds(
                Some(point(px(50.), px(10.))),
                Some(point(px(10.), px(50.))),
                Default::default()
            ),
            Bounds {
                origin: point(px(10.), px(10.)),
                size: size(px(40.), px(40.))
            }
        );
        //        50,10
        //   |------|
        //   |      |
        //   |------|
        // 10,50 start
        assert_eq!(
            selection_bounds(
                Some(point(px(10.), px(50.))),
                Some(point(px(50.), px(10.))),
                Default::default()
            ),
            Bounds {
                origin: point(px(10.), px(10.)),
                size: size(px(40.), px(40.))
            }
        );
    }
}

#[cfg(all(feature = "mermaid", feature = "editor-diagnostics"))]
impl ReadingProbe {
    pub fn mermaid_states(&self, cx: &App) -> Vec<bool> {
        let entity = self.0.upgrade().unwrap();
        let mut states = Vec::new();
        if let Some(Ok(content)) = &entity.read(cx).parsed_result {
            content.root_node.diagnostic_mermaid_states(&mut states);
        }
        states
    }
}
