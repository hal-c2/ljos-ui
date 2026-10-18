//! `Canvas`: an item drawn by script through a 2D context.
//!
//! The picture is pixels a [`CanvasPainter`] keeps; the scene shows them
//! as an image. Painting happens in the item's polish, the step of a frame
//! where layouts run: the first polish makes the canvas `available` and
//! asks for the first paint; after that a paint is asked for by
//! `requestPaint`, `markDirty` or a change of size, and nothing else. An
//! idle canvas has no effect that runs and asks for no frame.
//!
//! A request made while the canvas paints (a handler that animates by
//! asking again) and a `requestAnimationFrame` callback wait for the next
//! frame, which [`next_wake`] asks for, so a canvas that keeps asking
//! paints once a frame.
//!
//! Measured on qml6 (features/items/canvas*.feature): `available` is false
//! in `Component.onCompleted`; the first paint's region is the whole
//! canvas; a resize keeps the pixels that still fit and paints again with
//! the new size as the region; `getContext("2d")` is always the same
//! object, and `context` is it. `context` is `undefined` until then, or
//! until the canvas becomes available when `contextType` is `"2d"`. `loadImage` reads the image on the compute
//! pool and `imageLoaded` comes once it is there or has failed.

mod context;
mod data_url;
mod style;

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use ljos_engine::{
    Cx, Engine, EngineError, LogLevel, ObjectId, QmlObject, Rect, Size, Value, qml_impl,
};
use ljos_reactive::NodeId;
use ljos_render::kurbo;
use ljos_render::{CanvasPainter, DisplayListBuilder, ImageHandle};
use ljos_runtime::Time;

pub(crate) use context::Context2D;

use crate::enums::EnumTable;
use crate::image::{ImageKey, ImageStore, LoadError, Waiter};
use crate::item::{Item, num, write_computed};

const CATEGORY: &str = "qt.quick.canvas";

/// The refresh period a canvas that asks again waits for.
const FRAME: Duration = Duration::from_nanos(16_666_667);

pub mod render_target {
    pub const IMAGE: i32 = 0;
    pub const FRAMEBUFFER_OBJECT: i32 = 1;
}

pub mod render_strategy {
    pub const IMMEDIATE: i32 = 0;
    pub const THREADED: i32 = 1;
    pub const COOPERATIVE: i32 = 2;
}

fn canvas_enums() -> EnumTable {
    vec![
        (
            "RenderTarget",
            vec![
                ("Image", i64::from(render_target::IMAGE)),
                (
                    "FramebufferObject",
                    i64::from(render_target::FRAMEBUFFER_OBJECT),
                ),
            ],
        ),
        (
            "RenderStrategy",
            vec![
                ("Immediate", i64::from(render_strategy::IMMEDIATE)),
                ("Threaded", i64::from(render_strategy::THREADED)),
                ("Cooperative", i64::from(render_strategy::COOPERATIVE)),
            ],
        ),
    ]
}

/// An image a canvas was asked to load.
#[derive(Clone, Debug)]
enum Loaded {
    /// Read and decoded on the compute pool; [`image_arrived`] hears of it.
    Loading,
    Ready(ImageHandle),
    Error,
}

/// What a canvas keeps between frames. Shared with its context.
#[derive(Debug)]
pub(crate) struct CanvasRun {
    pub(crate) painter: CanvasPainter,
    /// The pixels the scene shows, as the last paint left them.
    shown: Option<ImageHandle>,
    /// A paint is wanted, for this part of the canvas (the whole when
    /// `None`).
    requested: bool,
    region: Option<kurbo::Rect>,
    /// The paint handler is running.
    pub(crate) in_paint: bool,
    /// Something was drawn since the picture was last taken.
    pub(crate) drawn: bool,
    /// The painter was sized once.
    sized: bool,
    /// Changes when the canvas needs polishing; the polish effect reads it.
    request_node: Option<NodeId>,
    /// Changes when [`CanvasRun::shown`] does; the content effect reads it.
    picture_node: Option<NodeId>,
    /// Images by absolute URL.
    images: HashMap<String, Loaded>,
    /// `requestAnimationFrame` callbacks, by handle.
    frames: Vec<(i32, Value)>,
    next_frame: i32,
    /// The context, once asked for: the native half and the script object.
    pub(crate) context: Option<(ObjectId, Value)>,
}

impl Default for CanvasRun {
    fn default() -> Self {
        Self {
            painter: CanvasPainter::new(0, 0),
            shown: None,
            requested: true,
            region: None,
            in_paint: false,
            drawn: false,
            sized: false,
            request_node: None,
            picture_node: None,
            images: HashMap::new(),
            frames: Vec::new(),
            next_frame: 1,
            context: None,
        }
    }
}

pub(crate) type SharedRun = Rc<RefCell<CanvasRun>>;

#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick",
    name = "Canvas",
    extends = Item,
    default = "data",
    enums = canvas_enums
)]
pub struct Canvas {
    #[qml(base)]
    pub item: Item,
    /// The context can be drawn on: from the first frame the canvas is in.
    #[qml(notify, readonly)]
    pub available: bool,
    /// The size of the picture; the item's size unless set.
    #[qml(notify)]
    pub canvas_size: Size,
    /// Kept; the whole canvas is always painted.
    #[qml(notify)]
    pub canvas_window: Rect,
    /// Kept; the canvas is one tile.
    #[qml(notify)]
    pub tile_size: Size,
    /// The context `getContext` made, or the one `contextType: "2d"` asks
    /// for once the canvas is available; `undefined` until then.
    #[qml(notify, readonly)]
    pub context: Value,
    #[qml(notify)]
    pub context_type: String,
    /// Accepted; the picture is always an image.
    #[qml(notify, enumeration = "RenderTarget")]
    pub render_target: i32,
    /// Accepted; painting always happens on the item's thread.
    #[qml(notify, enumeration = "RenderStrategy")]
    pub render_strategy: i32,
    #[qml(skip)]
    pub(crate) run: SharedRun,
}

impl Default for Canvas {
    fn default() -> Self {
        Self {
            item: Item::default(),
            available: false,
            canvas_size: Size::default(),
            canvas_window: Rect::default(),
            tile_size: Size::default(),
            context: Value::Undefined,
            context_type: String::new(),
            render_target: render_target::IMAGE,
            render_strategy: render_strategy::IMMEDIATE,
            run: SharedRun::default(),
        }
    }
}

#[qml_impl]
impl Canvas {
    #[qml(signal)]
    fn paint(region: Rect);

    #[qml(signal)]
    fn painted();

    #[qml(signal, name = "imageLoaded")]
    fn image_loaded();

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, me) = (cx.engine, cx.this);
        let size = move |e: &Engine| {
            Value::Size(Arc::new(Size {
                width: num(e, me, Item::WIDTH.index),
                height: num(e, me, Item::HEIGHT.index),
            }))
        };
        e.bind_initial(me, Self::CANVAS_SIZE.index, size)?;
        // A canvas told its kind of context after it became available has
        // the context at once; before, it waits for that.
        e.watch(me, Self::CONTEXT_TYPE.index, move |e, ty| {
            let available = e.untrack(|| e.read(me, Canvas::AVAILABLE.index).to_boolean());
            if available && matches!(ty, Value::String(t) if t.as_str() == "2d") {
                context_or_new(e, me);
            }
        })?;
        e.bind_initial(me, Self::TILE_SIZE.index, size)?;
        e.bind_initial(me, Self::CANVAS_WINDOW.index, move |e| {
            Value::Rect(Arc::new(Rect {
                x: 0.0,
                y: 0.0,
                width: num(e, me, Item::WIDTH.index),
                height: num(e, me, Item::HEIGHT.index),
            }))
        })?;
        if let Some(scope) = e.object_scope(me) {
            let request = e.graph().property(scope, Value::Int(0))?;
            let picture = e.graph().property(scope, Value::Int(0))?;
            if let Some(run) = run_of(e, me) {
                let mut r = run.borrow_mut();
                r.request_node = Some(request);
                r.picture_node = Some(picture);
            }
        }
        Ok(())
    }

    /// `getContext(contextId, ...)`: the 2D context, made the first time;
    /// `null` for any other kind.
    #[qml(invokable, name = "getContext")]
    fn get_context(cx: &Cx, context_id: Value) -> Value {
        let (e, me) = (cx.engine, cx.this);
        let id = context_id.to_js_string();
        if id.as_str() != "2d" {
            return Value::Null;
        }
        context_or_new(e, me)
    }

    /// `requestPaint()`: paint the whole canvas in a coming frame.
    #[qml(invokable, name = "requestPaint")]
    fn request_paint(cx: &Cx) {
        request(cx.engine, cx.this, None);
    }

    /// `markDirty(area)`: paint again with `area` as the region.
    #[qml(invokable, name = "markDirty")]
    fn mark_dirty(cx: &Cx, area: Value) {
        let region = match area {
            Value::Rect(r) => Some(kurbo::Rect::new(r.x, r.y, r.x + r.width, r.y + r.height)),
            _ => None,
        };
        request(cx.engine, cx.this, region);
    }

    /// `requestAnimationFrame(callback)`: call `callback` with the time
    /// before the next frame's paint. Returns a handle for
    /// `cancelRequestAnimationFrame`.
    #[qml(invokable, name = "requestAnimationFrame")]
    fn request_animation_frame(cx: &Cx, callback: Value) -> i32 {
        let (e, me) = (cx.engine, cx.this);
        let Some(run) = run_of(e, me) else { return 0 };
        let id = {
            let mut r = run.borrow_mut();
            let id = r.next_frame;
            r.next_frame = r.next_frame.wrapping_add(1).max(1);
            r.frames.push((id, callback));
            id
        };
        wait_for_frame(e, me);
        id
    }

    #[qml(invokable, name = "cancelRequestAnimationFrame")]
    fn cancel_request_animation_frame(cx: &Cx, handle: Value) {
        if let Some(run) = run_of(cx.engine, cx.this) {
            let id = handle.to_number() as i32;
            run.borrow_mut().frames.retain(|(i, _)| *i != id);
        }
    }

    /// `loadImage(url)`: starts reading the image off the engine's thread;
    /// `imageLoaded` follows once it is there or has failed. An image the
    /// engine has cached is loaded at once and says nothing, as in Qt
    /// (measured); a failed one is tried again.
    #[qml(invokable, name = "loadImage")]
    fn load_image(cx: &Cx, url: Value) {
        let (e, me) = (cx.engine, cx.this);
        let Some(run) = run_of(e, me) else { return };
        let key = image_key(e, me, &url);
        if matches!(
            run.borrow().images.get(&key.url),
            Some(Loaded::Ready(_) | Loaded::Loading)
        ) {
            return;
        }
        let store = ImageStore::of(e);
        if let Some(image) = store.cached(&key) {
            run.borrow_mut()
                .images
                .insert(key.url, Loaded::Ready(image));
            return;
        }
        run.borrow_mut()
            .images
            .insert(key.url.clone(), Loaded::Loading);
        store.request(e, key, Waiter::Canvas { item: me });
    }

    #[qml(invokable, name = "unloadImage")]
    fn unload_image(cx: &Cx, url: Value) {
        let (e, me) = (cx.engine, cx.this);
        if let Some(run) = run_of(e, me) {
            let key = image_key(e, me, &url);
            run.borrow_mut().images.remove(&key.url);
        }
    }

    #[qml(invokable, name = "isImageLoaded")]
    fn is_image_loaded(cx: &Cx, url: Value) -> bool {
        image_state(cx, &url).is_some_and(|l| matches!(l, Loaded::Ready(_)))
    }

    #[qml(invokable, name = "isImageLoading")]
    fn is_image_loading(cx: &Cx, url: Value) -> bool {
        image_state(cx, &url).is_some_and(|l| matches!(l, Loaded::Loading))
    }

    #[qml(invokable, name = "isImageError")]
    fn is_image_error(cx: &Cx, url: Value) -> bool {
        image_state(cx, &url).is_some_and(|l| matches!(l, Loaded::Error))
    }

    /// `save(filename)`: writes the picture as a PNG file.
    #[qml(invokable)]
    fn save(cx: &Cx, filename: Value) -> bool {
        let (e, me) = (cx.engine, cx.this);
        let Some(run) = run_of(e, me) else {
            return false;
        };
        let path = e.resolve_url(me, filename.to_js_string().as_str());
        let path = path.strip_prefix("file://").unwrap_or(&path).to_owned();
        let png = {
            let mut r = run.borrow_mut();
            let (w, h) = r.painter.size();
            let pixels = r.painter.image_data(0, 0, w, h);
            data_url::png(w, h, &pixels)
        };
        match png.map(|bytes| std::fs::write(&path, bytes)) {
            Some(Ok(())) => true,
            Some(Err(err)) => {
                e.log(
                    LogLevel::Warning,
                    CATEGORY,
                    format!("Canvas: cannot save {path}: {err}"),
                );
                false
            }
            None => false,
        }
    }

    /// `toDataURL(mimeType)`: the picture as a `data:` URL. Only PNG is
    /// written; another type gives `"data:,"`, as for an empty canvas.
    #[qml(invokable, name = "toDataURL")]
    fn to_data_url(cx: &Cx, mime_type: Value) -> String {
        let (e, me) = (cx.engine, cx.this);
        let mime = match mime_type {
            Value::Undefined | Value::Null => "image/png".to_owned(),
            v => v.to_js_string().as_str().to_ascii_lowercase(),
        };
        let Some(run) = run_of(e, me) else {
            return "data:,".into();
        };
        let mut r = run.borrow_mut();
        let (w, h) = r.painter.size();
        if mime != "image/png" || w == 0 || h == 0 {
            return "data:,".into();
        }
        let pixels = r.painter.image_data(0, 0, w, h);
        match data_url::png(w, h, &pixels) {
            Some(bytes) => format!("data:image/png;base64,{}", data_url::base64(&bytes)),
            None => "data:,".into(),
        }
    }
}

/// The state of `me`, a Canvas.
pub(crate) fn run_of(e: &Engine, me: ObjectId) -> Option<SharedRun> {
    e.with(me, |c: &Canvas| Rc::clone(&c.run)).ok()
}

fn image_key(e: &Engine, me: ObjectId, url: &Value) -> ImageKey {
    ImageKey {
        url: e.resolve_url(me, url.to_js_string().as_str()),
        request: Default::default(),
    }
}

fn image_state(cx: &Cx, url: &Value) -> Option<Loaded> {
    let run = run_of(cx.engine, cx.this)?;
    let key = image_key(cx.engine, cx.this, url);
    run.borrow().images.get(&key.url).cloned()
}

/// The pixels of an image the canvas loaded, or that is in the engine's
/// cache: what `drawImage` and `createPattern` draw for a URL.
pub(crate) fn image_for(
    e: &Engine,
    me: ObjectId,
    run: &CanvasRun,
    url: &str,
) -> Option<ImageHandle> {
    let key = image_key(e, me, &Value::String(url.into()));
    match run.images.get(&key.url) {
        Some(Loaded::Ready(i)) => Some(i.clone()),
        _ => ImageStore::of(e).cached(&key),
    }
}

/// The image store finished reading `key` for the canvas `me`: the image is
/// loaded (or failed) and `imageLoaded` says so, once for each image, as
/// Qt measured. An image unloaded in the meantime is not reported.
pub(crate) fn image_arrived(
    e: &Engine,
    me: ObjectId,
    key: &ImageKey,
    result: &Result<ImageHandle, LoadError>,
) {
    if !e.is_alive(me) {
        return;
    }
    let Some(run) = run_of(e, me) else { return };
    let wanted = {
        let mut r = run.borrow_mut();
        match r.images.get_mut(&key.url) {
            Some(l @ Loaded::Loading) => {
                *l = match result {
                    Ok(image) => Loaded::Ready(image.clone()),
                    Err(_) => Loaded::Error,
                };
                true
            }
            _ => false,
        }
    };
    if !wanted {
        return;
    }
    if let Err(err) = result {
        e.log(
            LogLevel::Warning,
            CATEGORY,
            format!("Canvas: cannot load {}: {err}", key.url),
        );
    }
    if let Err(err) = e.emit_by_name(me, "imageLoaded", &[]) {
        e.log(LogLevel::Warning, CATEGORY, format!("Canvas: {err}"));
    }
}

/// Makes the polish effect run again.
fn bump(e: &Engine, run: &SharedRun) {
    let node = run.borrow().request_node;
    if let Some(node) = node {
        bump_node(e, node);
    }
}

/// Changes what `node` holds, so what read it runs again.
fn bump_node(e: &Engine, node: NodeId) {
    let next = match e.graph().get_untracked(node) {
        Value::Int(n) => n.wrapping_add(1),
        _ => 1,
    };
    e.graph().set(node, Value::Int(next));
}

/// Asks for a paint of `region` (the whole canvas when `None`).
fn request(e: &Engine, me: ObjectId, region: Option<kurbo::Rect>) {
    let Some(run) = run_of(e, me) else { return };
    let in_paint = {
        let mut r = run.borrow_mut();
        r.region = match (r.requested, r.region, region) {
            (true, None, _) | (_, _, None) => None,
            (true, Some(a), Some(b)) => Some(a.union(b)),
            (false, _, Some(b)) => Some(b),
        };
        r.requested = true;
        r.in_paint
    };
    if in_paint {
        wait_for_frame(e, me);
    } else {
        bump(e, &run);
    }
}

/// Something was drawn outside a paint: show it in a coming frame.
pub(crate) fn drawn_outside_paint(e: &Engine, run: &SharedRun) {
    let first = {
        let mut r = run.borrow_mut();
        let first = !r.drawn && !r.in_paint;
        r.drawn = true;
        first
    };
    if first {
        bump(e, run);
    }
}

/// The canvases that want the next frame.
#[derive(Default)]
struct Waiting(RefCell<Vec<ObjectId>>);

fn wait_for_frame(e: &Engine, me: ObjectId) {
    let waiting = e.extension::<Waiting>();
    let mut w = waiting.0.borrow_mut();
    if !w.contains(&me) {
        w.push(me);
    }
}

/// Step 5 of a frame: the canvases that waited for it polish in it.
pub(crate) fn tick(e: &Engine) {
    let waiting = e.extension::<Waiting>();
    let list = std::mem::take(&mut *waiting.0.borrow_mut());
    for me in list {
        if e.is_alive(me)
            && let Some(run) = run_of(e, me)
        {
            bump(e, &run);
        }
    }
}

/// When a canvas that waits needs the next frame.
pub(crate) fn next_wake(e: &Engine, now: Time) -> Option<Time> {
    let waiting = e.extension::<Waiting>();
    (!waiting.0.borrow().is_empty()).then(|| now + FRAME)
}

/// What the polish of `me` follows: requests and the size. Tracked.
pub(crate) fn read_inputs(e: &Engine, me: ObjectId) {
    e.read(me, Canvas::CANVAS_SIZE.index);
    if let Some(run) = run_of(e, me) {
        let node = run.borrow().request_node;
        if let Some(node) = node {
            let _ = e.graph().get(node);
        }
    }
}

/// The context of `me`, made the first time.
fn context_or_new(e: &Engine, me: ObjectId) -> Value {
    let Some(run) = run_of(e, me) else {
        return Value::Null;
    };
    if let Some((_, js)) = &run.borrow().context {
        return js.clone();
    }
    let made = (|| -> Result<Value, EngineError> {
        let native = e.create_native_incomplete(Context2D::new(me, Rc::clone(&run)), Some(me))?;
        let make = e.eval_js("globalThis.__ljosCanvasContext")?;
        e.call_js(&make, &[Value::Object(native), Value::Object(me)])
            .inspect(|js| run.borrow_mut().context = Some((native, js.clone())))
            .ok_or_else(|| EngineError::Invoke("the 2D context could not be made".into()))
    })();
    match made {
        Ok(js) => {
            let _ = write_computed(e, me, Canvas::CONTEXT.index, js.clone());
            let _ = write_computed(
                e,
                me,
                Canvas::CONTEXT_TYPE.index,
                Value::String("2d".into()),
            );
            js
        }
        Err(err) => {
            e.log(LogLevel::Warning, CATEGORY, format!("Canvas: {err}"));
            Value::Null
        }
    }
}

/// The polish of a canvas: becomes available, reports images that have
/// arrived, runs animation callbacks, follows its size and paints when a
/// paint was asked for.
pub(crate) fn polish(e: &Engine, me: ObjectId) -> Result<(), EngineError> {
    let Some(run) = run_of(e, me) else {
        return Ok(());
    };
    if !e.untrack(|| e.read(me, Canvas::AVAILABLE.index).to_boolean()) {
        write_computed(e, me, Canvas::AVAILABLE.index, Value::Bool(true))?;
        // After `availableChanged`, as on qml6: its handler still reads no
        // context.
        let ty = e.untrack(|| e.read(me, Canvas::CONTEXT_TYPE.index));
        if matches!(&ty, Value::String(t) if t.as_str() == "2d") {
            context_or_new(e, me);
        }
    }
    let frames = std::mem::take(&mut run.borrow_mut().frames);
    if !frames.is_empty() {
        let at = Value::Real(e.now().as_micros() as f64 / 1000.0);
        for (_, f) in frames {
            e.call_js(&f, std::slice::from_ref(&at));
        }
    }
    let size = match e.untrack(|| e.read(me, Canvas::CANVAS_SIZE.index)) {
        Value::Size(s) => *s,
        _ => Size::default(),
    };
    let (w, h) = (pixels(size.width), pixels(size.height));
    {
        let mut r = run.borrow_mut();
        if !r.sized || r.painter.size() != (w, h) {
            r.sized = true;
            r.painter.resize(w, h);
            r.requested = true;
            r.region = None;
        }
    }
    let region = {
        let mut r = run.borrow_mut();
        if r.requested && w > 0 && h > 0 {
            r.requested = false;
            r.in_paint = true;
            Some(
                r.region
                    .take()
                    .unwrap_or(kurbo::Rect::new(0.0, 0.0, f64::from(w), f64::from(h))),
            )
        } else {
            None
        }
    };
    let painted = region.is_some();
    if let Some(region) = region {
        let arg = Value::Rect(Arc::new(Rect {
            x: region.x0,
            y: region.y0,
            width: region.width(),
            height: region.height(),
        }));
        let r = e.emit_by_name(me, "paint", &[arg]);
        run.borrow_mut().in_paint = false;
        r?;
    }
    let changed = {
        let mut r = run.borrow_mut();
        let drawn = std::mem::take(&mut r.drawn);
        if painted || drawn {
            let picture = r.painter.picture();
            let changed = picture != r.shown;
            r.shown = picture;
            changed
        } else {
            false
        }
    };
    let picture_node = run.borrow().picture_node;
    if changed && let Some(node) = picture_node {
        bump_node(e, node);
    }
    if painted {
        e.emit_by_name(me, "painted", &[])?;
    }
    Ok(())
}

/// A side in whole pixels.
fn pixels(v: f64) -> u32 {
    if v.is_finite() && v > 0.0 {
        v.ceil().min(f64::from(u16::MAX)) as u32
    } else {
        0
    }
}

/// Drops what the canvases of `e` can make again (copies of images and
/// glyph outlines); their pictures stay.
pub fn trim(e: &Engine, canvases: impl IntoIterator<Item = ObjectId>) {
    for me in canvases {
        if let Some(run) = run_of(e, me) {
            run.borrow_mut().painter.trim();
        }
    }
}

/// What the scene draws for a canvas: its picture at its own size.
#[derive(Clone, Debug)]
pub(crate) struct CanvasPicture {
    image: Option<ImageHandle>,
}

impl CanvasPicture {
    /// Reads the picture; tracked, so the content effect follows it.
    pub(crate) fn read(e: &Engine, me: ObjectId) -> Self {
        let Some(run) = run_of(e, me) else {
            return Self { image: None };
        };
        let node = run.borrow().picture_node;
        if let Some(node) = node {
            let _ = e.graph().get(node);
        }
        let image = run.borrow().shown.clone();
        Self { image }
    }

    pub(crate) fn paint(&self, _size: kurbo::Size, out: &mut DisplayListBuilder) {
        if let Some(image) = &self.image {
            let dst = image.rect();
            out.image(image.clone(), dst);
        }
    }
}

const INSTALL: &str = include_str!("context.js");

pub(crate) fn register(engine: &Engine) {
    engine.register_module(ljos_engine::module!("QtQuick" 2.15 => [Canvas]));
    engine.on_js_started(|e| {
        let r = e.eval_js(INSTALL).map(|f| e.call_js(&f, &[]));
        if let Err(err) = r {
            e.log(
                LogLevel::Warning,
                CATEGORY,
                format!("the Canvas 2D context is not available: {err}"),
            );
        }
    });
}
