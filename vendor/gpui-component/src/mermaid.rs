//! Offline Mermaid rendering shared by Reading and Live preview.
//! UI callers retain a ticket only while a request is relevant. Workers never mutate documents.
use gpui::{App, Global, RenderImage, SharedString};
use merman::render::{HeadlessRenderer, RootBackgroundPostprocessor, SvgPipeline};
use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Weak},
};

pub(crate) const MAX_SOURCE: usize = 50_000;
const MAX_PIXELS: u64 = 4_000_000;
const CACHE_BYTES: usize = 64 * 1024 * 1024;
const MAX_PENDING: usize = 64;

pub(crate) fn language(lang: Option<&str>) -> bool {
    lang.is_some_and(|s| s.eq_ignore_ascii_case("mermaid"))
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub(crate) struct Key {
    pub code: SharedString,
    pub dark: bool,
    pub width: u32,
    pub scale: u32,
}
impl Key {
    pub fn new(code: SharedString, dark: bool, width: f32, scale: f32) -> Self {
        Self {
            code,
            dark,
            width: width.clamp(1., 4096.).round() as u32,
            scale: (scale.clamp(1., 4.) * 100.).round() as u32,
        }
    }
}

#[derive(Clone)]
pub(crate) struct Diagram {
    pub image: Arc<RenderImage>,
    pub width: f32,
    pub height: f32,
    bytes: usize,
}
#[derive(Clone)]
pub(crate) enum Status {
    Pending,
    Ready(Diagram),
    Error(SharedString),
}
impl Status {
    fn bytes(&self) -> usize {
        match self {
            Self::Ready(d) => d.bytes,
            Self::Error(e) => e.len(),
            Self::Pending => 0,
        }
    }
}
pub(crate) struct Ticket {
    key: Key,
    alive: Arc<()>,
}
impl std::fmt::Debug for Ticket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MermaidTicket")
    }
}
struct Request {
    key: Key,
    users: Vec<Weak<()>>,
}
impl Request {
    fn live(&self) -> bool {
        self.users.iter().any(|w| w.strong_count() > 0)
    }
}
#[derive(Default)]
struct Service {
    cache: HashMap<Key, Status>,
    lru: VecDeque<Key>,
    bytes: usize,
    pending: VecDeque<Request>,
    running: Option<Request>,
}
impl Global for Service {}
impl Service {
    fn insert(&mut self, key: Key, value: Status) {
        if let Some(old) = self.cache.remove(&key) {
            self.bytes -= old.bytes() + key.code.len();
        }
        self.lru.retain(|k| k != &key);
        self.bytes += value.bytes() + key.code.len();
        self.cache.insert(key.clone(), value);
        self.lru.push_back(key);
        while self.bytes > CACHE_BYTES || self.cache.len() > 256 {
            let Some(key) = self.lru.pop_front() else {
                break;
            };
            if let Some(old) = self.cache.remove(&key) {
                self.bytes -= old.bytes() + key.code.len();
            }
        }
    }
    fn enqueue(&mut self, ticket: &Ticket) {
        self.pending.retain(Request::live);
        let existing = self
            .running
            .iter_mut()
            .chain(self.pending.iter_mut())
            .find(|r| r.key == ticket.key);
        if let Some(request) = existing {
            request.users.retain(|w| w.strong_count() > 0);
            let weak = Arc::downgrade(&ticket.alive);
            if !request.users.iter().any(|w| w.ptr_eq(&weak)) {
                request.users.push(weak);
            }
        } else {
            if self.pending.len() == MAX_PENDING {
                self.pending.pop_front();
            }
            self.pending.push_back(Request {
                key: ticket.key.clone(),
                users: vec![Arc::downgrade(&ticket.alive)],
            });
        }
    }
}

pub(crate) fn request(ticket: &mut Option<Ticket>, key: Key, cx: &mut App) -> Status {
    if key.code.len() > MAX_SOURCE {
        *ticket = None;
        return Status::Error("Mermaid source exceeds the 50 KB limit".into());
    }
    if ticket.as_ref().is_none_or(|t| t.key != key) {
        *ticket = Some(Ticket {
            key: key.clone(),
            alive: Arc::new(()),
        });
    }
    if cx.try_global::<Service>().is_none() {
        cx.set_global(Service::default());
    }
    let service = cx.global_mut::<Service>();
    if let Some(value) = service.cache.get(&key).cloned() {
        service.lru.retain(|k| k != &key);
        service.lru.push_back(key);
        return value;
    }
    service.enqueue(ticket.as_ref().unwrap());
    start_next(cx);
    Status::Pending
}
fn start_next(cx: &mut App) {
    let service = cx.global_mut::<Service>();
    if service.running.is_some() {
        return;
    }
    service.pending.retain(Request::live);
    let Some(request) = service.pending.pop_front() else {
        return;
    };
    let key = request.key.clone();
    service.running = Some(request);
    let work = cx.background_executor().spawn(async move {
        // A renderer panic is an error for this diagram, not a poisoned UI/service.
        let result = std::panic::catch_unwind(|| render(&key));
        let status = match result {
            Ok(Ok(diagram)) => Status::Ready(diagram),
            Ok(Err(error)) => Status::Error(
                error
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ")
                    .chars()
                    .take(240)
                    .collect::<String>()
                    .into(),
            ),
            Err(_) => Status::Error("Mermaid renderer failed".into()),
        };
        (key, status)
    });
    cx.spawn(async move |cx| {
        let (key, result) = work.await;
        let _ = cx.update(|cx| {
            let service = cx.global_mut::<Service>();
            let relevant = service.running.take().is_some_and(|r| r.live());
            if relevant {
                service.insert(key, result);
            }
            cx.refresh_windows();
            start_next(cx);
        });
    })
    .detach();
}

fn render(key: &Key) -> Result<Diagram, String> {
    if key.code.len() > MAX_SOURCE {
        return Err("Mermaid source exceeds the 50 KB limit".into());
    }
    let renderer = HeadlessRenderer::new()
        .with_strict_parsing()
        .with_site_config(merman::MermaidConfig::from_value(serde_json::json!({
            "theme": if key.dark { "dark" } else { "default" },
            "securityLevel": "strict", "htmlLabels": false,
            "fontFamily": "Arial", "maxTextSize": MAX_SOURCE
        })))
        .with_diagram_id("mithril-mermaid");
    // Mermaid parity output can keep a white canvas even with the dark theme.
    // Let the native code-block surface supply the background in both views.
    let pipeline = SvgPipeline::resvg_safe()
        .with_postprocessor(RootBackgroundPostprocessor::new("transparent"));
    let svg = renderer
        .render_svg_with_pipeline_sync(&key.code, &pipeline)
        .map_err(|e| format!("Mermaid: {e}"))?
        .ok_or_else(|| "Unsupported or empty Mermaid diagram".to_string())?;
    rasterize(&svg, key)
}
fn rasterize(svg: &str, key: &Key) -> Result<Diagram, String> {
    use resvg::{tiny_skia, usvg};
    // No file, URL or embedded image resolver is allowed to run.
    let mut options = usvg::Options {
        image_href_resolver: usvg::ImageHrefResolver {
            resolve_string: Box::new(|_, _| None),
            resolve_data: Box::new(|_, _, _| None),
        },
        font_family: "Arial".into(),
        ..Default::default()
    };
    // Font discovery is shared, and always happens on the renderer worker.
    static FONTS: std::sync::OnceLock<Arc<usvg::fontdb::Database>> = std::sync::OnceLock::new();
    options.fontdb = FONTS
        .get_or_init(|| {
            let mut db = usvg::fontdb::Database::new();
            db.load_system_fonts();
            Arc::new(db)
        })
        .clone();
    let tree = usvg::Tree::from_str(svg, &options).map_err(|e| format!("Mermaid image: {e}"))?;
    let intrinsic = tree.size();
    let fit = (key.width as f32 / intrinsic.width()).min(1.);
    let width = intrinsic.width() * fit;
    let height = intrinsic.height() * fit;
    let scale = fit * key.scale as f32 / 100.;
    let w = (intrinsic.width() * scale).ceil().max(1.) as u32;
    let h = (intrinsic.height() * scale).ceil().max(1.) as u32;
    if u64::from(w) * u64::from(h) > MAX_PIXELS {
        return Err("Mermaid diagram exceeds the 4 megapixel limit".into());
    }
    let mut pixmap = tiny_skia::Pixmap::new(w, h).ok_or("Unable to allocate Mermaid image")?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    // GPUI expects straight-alpha BGRA; tiny-skia returns premultiplied RGBA.
    let mut pixels = pixmap.take();
    for pixel in pixels.chunks_exact_mut(4) {
        pixel.swap(0, 2);
        if pixel[3] > 0 {
            let alpha = f32::from(pixel[3]) / 255.;
            for channel in &mut pixel[..3] {
                *channel = (f32::from(*channel) / alpha).round().min(255.) as u8;
            }
        }
    }
    let bytes = pixels.len();
    let buffer = image::RgbaImage::from_raw(w, h, pixels).ok_or("Invalid Mermaid image size")?;
    Ok(Diagram {
        image: Arc::new(RenderImage::new(smallvec::smallvec![image::Frame::new(
            buffer
        )])),
        width,
        height,
        bytes,
    })
}

#[derive(Default, Debug)]
pub(crate) struct Reading {
    ticket: Option<Ticket>,
    width: Option<f32>,
    #[cfg(feature = "editor-diagnostics")]
    pub(crate) ready: bool,
}

pub(crate) fn reading(
    state: Arc<std::sync::Mutex<Reading>>,
    code: SharedString,
    reveal: bool,
    window: &mut gpui::Window,
    cx: &mut App,
) -> (
    Option<gpui::AnyElement>,
    Option<SharedString>,
    gpui::AnyElement,
) {
    use crate::ActiveTheme;
    use gpui::{InteractiveElement, IntoElement, ParentElement, Styled, StyledImage};
    let width = state.lock().unwrap().width.unwrap_or(640.);
    let status = if reveal {
        state.lock().unwrap().ticket = None;
        None
    } else {
        let key = Key::new(code, cx.theme().is_dark(), width, window.scale_factor());
        Some(request(&mut state.lock().unwrap().ticket, key, cx))
    };
    #[cfg(feature = "editor-diagnostics")]
    {
        state.lock().unwrap().ready = matches!(status, Some(Status::Ready(_)));
    }
    let probe = gpui::canvas(
        move |bounds, _, cx| {
            let width = f32::from(bounds.size.width).max(1.);
            let mut state = state.lock().unwrap();
            if state.width.is_none_or(|old| (old - width).abs() > 1.) {
                state.width = Some(width);
                cx.refresh_windows();
            }
        },
        |_, _, _, _| {},
    )
    .w_full()
    .h(gpui::px(0.))
    .into_any_element();
    let (image, message) = match status {
        Some(Status::Ready(d)) => (
            Some(
                gpui::div()
                    .id("mermaid-diagram")
                    .debug_selector(|| "mermaid-diagram".into())
                    .child(
                        gpui::img(d.image)
                            .w(gpui::px(d.width))
                            .h(gpui::px(d.height))
                            .object_fit(gpui::ObjectFit::Contain),
                    )
                    .into_any_element(),
            ),
            None,
        ),
        Some(Status::Error(e)) => (None, Some(e)),
        Some(Status::Pending) => (None, Some("Rendering Mermaid…".into())),
        None => (None, None),
    };
    (image, message, probe)
}

#[cfg(feature = "editor-diagnostics")]
#[doc(hidden)]
pub fn diagnostic_checks() -> Vec<(&'static str, bool, Arc<RenderImage>)> {
    let mut examples = Vec::new();
    let fixtures = [
        "flowchart TD\nA[Start] --> B[End]",
        r#"sequenceDiagram
    participant U as User
    participant A as App
    participant C as Cache
    participant S as Server
    U->>A: Open a mysterious note
    A->>C: Look up note
    alt Note is cached
        C-->>A: Return cached note
    else Cache miss
        C-->>A: Not found
        A->>S: Fetch note
        S-->>A: Secret pancake recipe
        A->>C: Cache recipe
    end
    A-->>U: Display note
    Note over U,S: Breakfast has been successfully planned
"#,
        "classDiagram\nAnimal <|-- Duck",
        "stateDiagram-v2\n[*] --> Idle\nIdle --> [*]",
        "erDiagram\nCUSTOMER ||--o{ ORDER : places",
        "gantt\n title Plan\n dateFormat YYYY-MM-DD\n section Work\n Task :a, 2026-01-01, 2d",
        "pie\n\"A\" : 40\n\"B\" : 60",
        include_str!("mermaid/spaced-sequence.mmd"),
    ];
    for dark in [false, true] {
        for source in fixtures {
            let key = Key::new(source.into(), dark, 320., 2.);
            let d = render(&key).unwrap_or_else(|e| panic!("{source}: {e}"));
            if source == fixtures[0] {
                examples.push(("flowchart", dark, d.image.clone()));
            } else if source == fixtures[1] {
                examples.push(("sequence", dark, d.image.clone()));
            }
            assert!(d.width > 0. && d.width <= 320. && d.height > 0.);
            assert!(d.bytes <= MAX_PIXELS as usize * 4);
            assert!(
                d.image
                    .as_bytes(0)
                    .unwrap()
                    .chunks_exact(4)
                    .any(|p| p[3] != 0)
            );
        }
    }
    for source in [
        "",
        "unsupported diagram",
        "flowchart TD\nA[",
        &"x".repeat(MAX_SOURCE + 1),
    ] {
        assert!(
            render(&Key::new(source.to_owned().into(), false, 320., 1.)).is_err(),
            "{source}"
        );
    }
    let key = Key::new(fixtures[0].into(), false, 320., 1.);
    assert!(
        rasterize(
            "<svg xmlns='http://www.w3.org/2000/svg' width='320' height='20000'/>",
            &key
        )
        .is_err()
    );
    // Even valid external and embedded image references are never resolved.
    let d = rasterize("<svg xmlns='http://www.w3.org/2000/svg' width='20' height='20'><image href='file:///nonexistent.png' width='20' height='20'/><image href='https://example.com/a.png' width='20' height='20'/></svg>", &key).unwrap();
    assert!(
        d.image
            .as_bytes(0)
            .unwrap()
            .chunks_exact(4)
            .all(|p| p[3] == 0)
    );
    let translucent = rasterize("<svg xmlns='http://www.w3.org/2000/svg' width='2' height='2'><rect width='2' height='2' fill='red' opacity='0.5'/></svg>", &key).unwrap();
    let pixel = &translucent.image.as_bytes(0).unwrap()[..4];
    assert!(pixel[2] > 250 && pixel[3] >= 127 && pixel[3] <= 128);
    let mut service = Service::default();
    let ticket = Ticket {
        key: key.clone(),
        alive: Arc::new(()),
    };
    service.enqueue(&ticket);
    service.enqueue(&ticket);
    assert_eq!(service.pending.len(), 1);
    assert_eq!(service.pending[0].users.len(), 1);
    let peer = Ticket {
        key: key.clone(),
        alive: Arc::new(()),
    };
    service.enqueue(&peer);
    drop(ticket);
    assert!(service.pending[0].live());
    drop(peer);
    assert!(!service.pending[0].live());
    let mut tickets = Vec::new();
    for n in 0..100 {
        let mut key = key.clone();
        key.width += n;
        tickets.push(Ticket {
            key,
            alive: Arc::new(()),
        });
        service.enqueue(tickets.last().unwrap());
    }
    assert_eq!(service.pending.len(), MAX_PENDING);
    // Cache budget includes code and error strings, and eviction is deterministic.
    for n in 0..300 {
        let mut key = key.clone();
        key.width += n;
        service.insert(key, Status::Error("expected".into()));
    }
    assert_eq!(service.cache.len(), 256);
    let diagram = render(&key).unwrap();
    for n in 0..8 {
        let mut key = key.clone();
        key.width += n;
        let mut d = diagram.clone();
        d.bytes = 16 * 1024 * 1024;
        service.insert(key, Status::Ready(d));
    }
    assert!(service.bytes <= CACHE_BYTES);
    examples
}
