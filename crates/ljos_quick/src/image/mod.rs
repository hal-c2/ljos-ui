//! `Image` and `BorderImage`: pictures from files, shared and decoded once.
//!
//! An item here owns no pixels. It knows what it shows (a URL and the
//! size it asked for) and how large that is; the pixels live in the
//! engine's [`ImageStore`] and in the display list of an item that is on
//! screen. That is what lets the store drop the picture of a hidden item
//! under memory pressure without the item's `status` or size changing
//! (ARCHITECTURE §8.5): the next paint decodes it again.
//!
//! Loading follows Qt 6.11 as measured on qml6. A local file loads on the
//! spot unless `asynchronous` is set; then it is read and decoded on the
//! compute pool and arrives with a later frame. While a new source loads,
//! the item keeps the size of the old picture. A network source is
//! always loaded that way, whatever `asynchronous` says.
//!
//! An SVG is a drawing rather than pixels: it is made at the size asked
//! for (stretched when both sides are), and on screen at the display's
//! scale as well, so that it is sharp there. The item's size, and every
//! property that says it, stays in logical pixels.

mod animated;
mod border;
mod decode;
mod store;
mod tint;

pub use animated::AnimatedImage;
pub use border::{BorderImage, BorderImageBorder};
pub use decode::{LoadError, Request, requested_size};
pub use store::{ImageKey, ImageStore};
pub use tint::{ImageTint, register_image_tint};

use ljos_engine::{
    Cx, Engine, EngineError, LogLevel, ObjectId, PropIndex, QmlObject, Rect, Size, Url, Value,
    qml_impl,
};
use ljos_reactive::NodeId;
use ljos_render::{
    DisplayListBuilder, DrawOp, ImageAlign, ImageFit, ImageHandle, Insets, TileMode, kurbo,
    painted_size, place_image,
};

use crate::enums::{EnumTable, align};
use crate::item::{Item, is_a, write_computed};
pub(crate) use store::Waiter;

/// Log category of what the store has to say that Qt has no words for.
pub(crate) const CATEGORY: &str = "ljos.image";

/// `Image.status` and `BorderImage.status`.
pub mod image_status {
    pub const NULL: i32 = 0;
    pub const READY: i32 = 1;
    pub const LOADING: i32 = 2;
    pub const ERROR: i32 = 3;
}

/// `Image.fillMode`.
pub mod fill_mode {
    pub const STRETCH: i32 = 0;
    pub const PRESERVE_ASPECT_FIT: i32 = 1;
    pub const PRESERVE_ASPECT_CROP: i32 = 2;
    pub const TILE: i32 = 3;
    pub const TILE_VERTICALLY: i32 = 4;
    pub const TILE_HORIZONTALLY: i32 = 5;
    pub const PAD: i32 = 6;
}

/// `BorderImage.horizontalTileMode` and `verticalTileMode`.
pub mod tile_mode {
    pub const STRETCH: i32 = 0;
    pub const REPEAT: i32 = 1;
    pub const ROUND: i32 = 2;
}

fn keys(list: &[(&'static str, i32)]) -> Vec<(&'static str, i64)> {
    list.iter().map(|(k, v)| (*k, i64::from(*v))).collect()
}

fn status_enums() -> EnumTable {
    use image_status as s;
    vec![(
        "Status",
        keys(&[
            ("Null", s::NULL),
            ("Ready", s::READY),
            ("Loading", s::LOADING),
            ("Error", s::ERROR),
        ]),
    )]
}

fn image_enums() -> EnumTable {
    use fill_mode as f;
    vec![
        (
            "FillMode",
            keys(&[
                ("Stretch", f::STRETCH),
                ("PreserveAspectFit", f::PRESERVE_ASPECT_FIT),
                ("PreserveAspectCrop", f::PRESERVE_ASPECT_CROP),
                ("Tile", f::TILE),
                ("TileVertically", f::TILE_VERTICALLY),
                ("TileHorizontally", f::TILE_HORIZONTALLY),
                ("Pad", f::PAD),
            ]),
        ),
        (
            "HAlignment",
            keys(&[
                ("AlignLeft", align::LEFT),
                ("AlignRight", align::RIGHT),
                ("AlignHCenter", align::H_CENTER),
            ]),
        ),
        (
            "VAlignment",
            keys(&[
                ("AlignTop", align::TOP),
                ("AlignBottom", align::BOTTOM),
                ("AlignVCenter", align::V_CENTER),
            ]),
        ),
    ]
}

/// What `Image` and `BorderImage` share: a source, its loading, and the
/// size of what was loaded. Not a QML type of its own.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick",
    name = "ImageBase",
    extends = Item,
    enums = status_enums,
    uncreatable
)]
pub struct ImageBase {
    #[qml(base)]
    pub item: Item,
    #[qml(notify)]
    pub source: Url,
    /// [`image_status`].
    #[qml(notify, readonly, enumeration = "Status")]
    pub status: i32,
    /// 0 until the picture is there, then 1: a local file has no
    /// in-between.
    #[qml(notify, readonly)]
    pub progress: f64,
    #[qml(notify)]
    pub asynchronous: bool,
    #[qml(notify)]
    pub cache: bool,
    /// What the author asked for, or the size of the loaded picture when
    /// nothing was asked.
    #[qml(notify, reset)]
    pub source_size: Size,
    #[qml(notify)]
    pub mirror: bool,
    #[qml(notify)]
    pub smooth: bool,
    #[qml(skip)]
    run: Run,
}

impl Default for ImageBase {
    fn default() -> Self {
        Self {
            item: Item::default(),
            source: Url::default(),
            status: image_status::NULL,
            progress: 0.0,
            asynchronous: false,
            cache: true,
            source_size: Size::default(),
            mirror: false,
            smooth: true,
            run: Run::default(),
        }
    }
}

/// The part of an image item that QML does not see.
#[derive(Debug, Default)]
struct Run {
    /// Nothing loads before the item is complete.
    completed: bool,
    /// Counts loads, so a decode that comes back after the source changed
    /// again is known to be stale.
    generation: u64,
    /// What is loaded, while `status` is Ready.
    key: Option<ImageKey>,
    /// The size of the loaded picture. Kept while the next one loads.
    natural: (u32, u32),
    /// The `sourceSize` the author set, if it asks for anything.
    requested: Option<(f64, f64)>,
    /// The `sourceSize` this item wrote itself, to tell its own report
    /// from the author's request when the property changes.
    reported: Option<Size>,
    /// Pixels the item keeps itself: for good with `cache: false`
    /// (`pinned`), or from a reload until the next paint takes them.
    held: Option<ImageHandle>,
    pinned: bool,
    /// The scene paints this item.
    shown: bool,
    /// A reload of dropped pixels is under way.
    reshowing: bool,
    /// The dropped pixels could not be loaded again; do not keep trying.
    lost: bool,
    /// A drawing made at the display's scale (in thousandths), like `held`:
    /// for good when `pinned`, otherwise until the next paint takes it.
    sharp: Option<(u32, ImageHandle)>,
    /// The scale a drawing is being made at in the background, or failed
    /// to be made at: it is not asked for again.
    sharpening: Option<u32>,
    /// A graph node that changes whenever the pixels do, so that the
    /// scene's paint effect and the painted size follow.
    epoch: Option<NodeId>,
    /// The frames of an animation an `AnimatedImage` shows, and which of
    /// them it shows. Its own: the cache keeps single pictures.
    frames: Option<std::sync::Arc<decode::Frames>>,
    frame: usize,
}

#[qml_impl]
impl ImageBase {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, me) = (cx.engine, cx.this);
        e.watch(me, ImageBase::SOURCE.index, move |e, _| reload(e, me))?;
        e.watch(me, ImageBase::SOURCE_SIZE.index, move |e, v| {
            let Value::Size(size) = v else { return };
            let ours = with_run(e, me, |r| r.reported == Some(**size)) == Some(true);
            if ours {
                return;
            }
            let asked = asks(size);
            with_run(e, me, |r| {
                r.requested = asked;
                r.reported = None;
            });
            reload(e, me);
        })?;
        // The watches above gave the object its scope, so the node dies
        // with the item.
        let epoch = match e.object_scope(me) {
            Some(scope) => Some(e.graph().property(scope, Value::Int(0))?),
            None => None,
        };
        let asked = match e.untrack(|| e.read(me, ImageBase::SOURCE_SIZE.index)) {
            Value::Size(size) => asks(&size),
            _ => None,
        };
        with_run(e, me, |r| {
            r.epoch = epoch;
            r.requested = asked;
            r.completed = true;
        });
        reload(e, me);
        Ok(())
    }
}

/// `Image` (QtQuick).
///
/// `mipmap` is accepted and has no effect: both backends scale a picture
/// down in one step. A network source (`http`, `https`) is fetched with
/// the fetcher the host installed on the platform, always in the
/// background.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick",
    name = "Image",
    extends = ImageBase,
    enums = image_enums
)]
pub struct Image {
    #[qml(base)]
    pub base: ImageBase,
    /// [`fill_mode`].
    #[qml(notify, enumeration = "FillMode")]
    pub fill_mode: i32,
    #[qml(notify, enumeration = "HAlignment")]
    pub horizontal_alignment: i32,
    #[qml(notify, enumeration = "VAlignment")]
    pub vertical_alignment: i32,
    #[qml(notify)]
    pub mirror_vertically: bool,
    #[qml(notify)]
    pub mipmap: bool,
    #[qml(notify, readonly)]
    pub painted_width: f64,
    #[qml(notify, readonly)]
    pub painted_height: f64,
    /// The part of the (scaled) picture to keep; empty for all of it.
    #[qml(notify, reset)]
    pub source_clip_rect: Rect,
}

impl Default for Image {
    fn default() -> Self {
        Self {
            base: ImageBase::default(),
            fill_mode: fill_mode::STRETCH,
            horizontal_alignment: align::H_CENTER,
            vertical_alignment: align::V_CENTER,
            mirror_vertically: false,
            mipmap: false,
            painted_width: 0.0,
            painted_height: 0.0,
            source_clip_rect: Rect::default(),
        }
    }
}

#[qml_impl]
impl Image {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, me) = (cx.engine, cx.this);
        e.watch(me, Image::FILL_MODE.index, move |e, _| {
            // The fill mode only decides how a requested size is met.
            if with_run(e, me, |r| r.requested.is_some()) == Some(true) {
                reload(e, me);
            }
        })?;
        e.watch(me, Image::SOURCE_CLIP_RECT.index, move |e, _| {
            reload(e, me);
        })?;
        e.effect_in(me, move |e| update_painted(e, me))?;
        Ok(())
    }
}

fn with_run<R>(e: &Engine, me: ObjectId, f: impl FnOnce(&mut Run) -> R) -> Option<R> {
    e.with_mut(me, |b: &mut ImageBase| f(&mut b.run)).ok()
}

/// Whether a `sourceSize` asks for anything: a side that is not positive
/// is not asked for.
fn asks(size: &Size) -> Option<(f64, f64)> {
    (size.width > 0.0 || size.height > 0.0).then_some((size.width, size.height))
}

/// Writes one of the item's own properties; a change runs its handlers.
fn set(e: &Engine, me: ObjectId, p: PropIndex, v: Value) {
    if e.is_alive(me)
        && let Err(err) = write_computed(e, me, p, v)
    {
        e.log(LogLevel::Warning, CATEGORY, format!("{err}"));
    }
}

/// Qt's warning for an item: where it was declared, its type, the reason.
fn warn(e: &Engine, me: ObjectId, reason: &LoadError) {
    let at = e.declared_at(me).unwrap_or_default();
    let name = if is_a::<BorderImage>(e, me) {
        "BorderImage"
    } else {
        "Image"
    };
    e.log(
        LogLevel::Warning,
        "qml",
        format!("{at}QML {name}: {reason}"),
    );
}

/// Tells everything that reads the item's pixels that they changed.
fn bump(e: &Engine, me: ObjectId) {
    if let Some(Some(node)) = with_run(e, me, |r| r.epoch) {
        let next = match e.graph().get_untracked(node) {
            Value::Int(n) => n.wrapping_add(1),
            _ => 0,
        };
        e.graph().set(node, Value::Int(next));
    }
}

/// Reads the epoch so that the running effect follows the pixels.
fn follow(e: &Engine, me: ObjectId) {
    if let Some(Some(node)) = with_run(e, me, |r| r.epoch) {
        e.graph().get(node);
    }
}

fn fit_of(mode: i32) -> ImageFit {
    match mode {
        fill_mode::PRESERVE_ASPECT_FIT => ImageFit::Fit,
        fill_mode::PRESERVE_ASPECT_CROP => ImageFit::Crop,
        fill_mode::TILE => ImageFit::Tile,
        fill_mode::TILE_VERTICALLY => ImageFit::TileVertically,
        fill_mode::TILE_HORIZONTALLY => ImageFit::TileHorizontally,
        fill_mode::PAD => ImageFit::Pad,
        _ => ImageFit::Stretch,
    }
}

fn int(e: &Engine, me: ObjectId, p: PropIndex) -> i32 {
    e.read(me, p).to_number() as i32
}

/// What the item asks the store for besides the URL (untracked).
fn request_of(e: &Engine, me: ObjectId) -> Request {
    let side = |v: f64| if v > 0.0 { v.round() as u32 } else { 0 };
    let (width, height) = with_run(e, me, |r| r.requested)
        .flatten()
        .map_or((0, 0), |(w, h)| (side(w), side(h)));
    if !is_a::<Image>(e, me) {
        return Request {
            width,
            height,
            animated: is_a::<AnimatedImage>(e, me),
            ..Request::default()
        };
    }
    e.untrack(|| {
        let mode = int(e, me, Image::FILL_MODE.index);
        let keeps_shape = matches!(
            mode,
            fill_mode::PRESERVE_ASPECT_FIT | fill_mode::PRESERVE_ASPECT_CROP
        );
        let clip = match e.read(me, Image::SOURCE_CLIP_RECT.index) {
            Value::Rect(r) if r.width > 0.0 && r.height > 0.0 => {
                Some([side(r.x), side(r.y), side(r.width), side(r.height)])
            }
            _ => None,
        };
        Request {
            width,
            height,
            cover: keeps_shape && (width > 0 || height > 0),
            clip,
            scale: 0,
            animated: is_a::<AnimatedImage>(e, me),
        }
    })
}

/// Brings the item in step with its source and what it asks of it.
fn reload(e: &Engine, me: ObjectId) {
    if !e.is_alive(me) {
        return;
    }
    let started = with_run(e, me, |r| {
        r.completed.then(|| {
            r.generation += 1;
            r.generation
        })
    })
    .flatten();
    let Some(generation) = started else { return };
    let store = ImageStore::of(e);
    // Whatever this item was waiting for is no longer wanted.
    store.forget(me);
    let (source, asynchronous, cache) = e.untrack(|| {
        (
            e.read(me, ImageBase::SOURCE.index).to_js_string(),
            e.read(me, ImageBase::ASYNCHRONOUS.index).to_boolean(),
            e.read(me, ImageBase::CACHE.index).to_boolean(),
        )
    });
    if source.is_empty() {
        unload(e, me, image_status::NULL);
        return;
    }
    let key = ImageKey {
        url: e.resolve_url(me, &source),
        request: request_of(e, me),
    };
    // The cache keeps single pictures; the frames are the item's own.
    let cache = cache && !key.request.animated;
    // A picture another item has loaded is there at once, also for an
    // asynchronous Image.
    if cache && let Some(image) = store.cached(&key) {
        finish(e, me, key, image, cache, None);
        return;
    }
    if is_local(&key.url) && !asynchronous {
        let loaded = if key.request.animated {
            store.decode_now(&key)
        } else {
            store.load_now(&key, cache).map(|image| decode::Loaded {
                image,
                vector: false,
                frames: None,
            })
        };
        match loaded {
            Ok(loaded) => finish(e, me, key, loaded.image, cache, loaded.frames),
            Err(err) => {
                unload(e, me, image_status::ERROR);
                warn(e, me, &err);
            }
        }
        return;
    }
    with_run(e, me, |r| {
        r.key = None;
        r.held = None;
        r.reshowing = false;
        r.sharp = None;
        r.sharpening = None;
    });
    // The size of the old picture stays until the new one is there.
    e.batch(|| {
        bump(e, me);
        set(e, me, ImageBase::PROGRESS.index, Value::Real(0.0));
        set(
            e,
            me,
            ImageBase::STATUS.index,
            Value::Int(image_status::LOADING),
        );
    });
    store.request(
        e,
        key,
        Waiter::Load {
            item: me,
            generation,
            cache,
        },
    );
}

/// Whether Qt reads `url` on the spot: only local files are; whatever
/// else has a scheme, a `data:` URL or a network source, arrives later
/// whatever `asynchronous` says.
fn is_local(url: &str) -> bool {
    matches!(decode::scheme(url).as_deref(), None | Some("file" | "qrc"))
}

/// Whether pixels of `url` the store dropped are made again in the
/// background: always for a network source, which is never waited for.
fn in_background(e: &Engine, me: ObjectId, url: &str) -> bool {
    matches!(decode::scheme(url).as_deref(), Some("http" | "https"))
        || e.untrack(|| e.read(me, ImageBase::ASYNCHRONOUS.index).to_boolean())
}

/// The item shows `image` from now on, or the `frames` it is the first of.
fn finish(
    e: &Engine,
    me: ObjectId,
    key: ImageKey,
    image: ImageHandle,
    cache: bool,
    frames: Option<std::sync::Arc<decode::Frames>>,
) {
    let natural = (image.width(), image.height());
    let asked = with_run(e, me, |r| {
        r.frames = frames;
        r.frame = 0;
        r.key = Some(key);
        r.natural = natural;
        // Without the cache nobody else has the pixels: the item must.
        r.held = (!cache).then_some(image);
        r.pinned = !cache;
        r.reshowing = false;
        r.lost = false;
        r.sharp = None;
        r.sharpening = None;
        r.requested.is_some()
    }) == Some(true);
    let (w, h) = (f64::from(natural.0), f64::from(natural.1));
    // In the order Qt announces them; a handler of any of them sees the
    // others already in place.
    e.batch(|| {
        bump(e, me);
        animated::loaded(e, me);
        set(e, me, ImageBase::PROGRESS.index, Value::Real(1.0));
        set(e, me, Item::IMPLICIT_WIDTH.index, Value::Real(w));
        set(e, me, Item::IMPLICIT_HEIGHT.index, Value::Real(h));
        set(
            e,
            me,
            ImageBase::STATUS.index,
            Value::Int(image_status::READY),
        );
        if !asked {
            report_size(e, me, w, h);
        }
    });
}

/// The item shows nothing: its source is empty (`Null`) or failed
/// (`Error`).
fn unload(e: &Engine, me: ObjectId, status: i32) {
    let asked = with_run(e, me, |r| {
        r.key = None;
        r.natural = (0, 0);
        r.held = None;
        r.reshowing = false;
        r.lost = false;
        r.sharp = None;
        r.sharpening = None;
        r.frames = None;
        r.frame = 0;
        r.requested.is_some()
    }) == Some(true);
    e.batch(|| {
        bump(e, me);
        animated::loaded(e, me);
        set(e, me, ImageBase::PROGRESS.index, Value::Real(0.0));
        set(e, me, Item::IMPLICIT_WIDTH.index, Value::Real(0.0));
        set(e, me, Item::IMPLICIT_HEIGHT.index, Value::Real(0.0));
        set(e, me, ImageBase::STATUS.index, Value::Int(status));
        if !asked {
            report_size(e, me, 0.0, 0.0);
        }
    });
}

/// Makes `sourceSize` read the size of the loaded picture, as it does in
/// Qt when the author asked for none.
fn report_size(e: &Engine, me: ObjectId, width: f64, height: f64) {
    with_run(e, me, |r| r.reported = Some(Size { width, height }));
    set(
        e,
        me,
        ImageBase::SOURCE_SIZE.index,
        Value::size(width, height),
    );
}

/// A background decode came back for `waiter`.
pub(crate) fn delivered(
    e: &Engine,
    waiter: Waiter,
    key: &ImageKey,
    result: &Result<decode::Loaded, LoadError>,
) {
    let loaded = result;
    let result = &loaded
        .as_ref()
        .map(|l| l.image.clone())
        .map_err(Clone::clone);
    match waiter {
        Waiter::Load {
            item,
            generation,
            cache,
        } => {
            let current = with_run(e, item, |r| r.generation == generation) == Some(true);
            if !e.is_alive(item) || !current {
                return;
            }
            match loaded {
                Ok(l) => finish(
                    e,
                    item,
                    key.clone(),
                    l.image.clone(),
                    cache,
                    l.frames.clone(),
                ),
                Err(err) => {
                    unload(e, item, image_status::ERROR);
                    warn(e, item, err);
                }
            }
        }
        Waiter::Reshow { item } => {
            if !e.is_alive(item) {
                return;
            }
            let wanted = with_run(e, item, |r| {
                if r.key.as_ref() != Some(key) {
                    return false;
                }
                r.reshowing = false;
                match result {
                    // Held until the paint takes it, so that the next
                    // picture into a full cache cannot push it out first.
                    Ok(image) if r.shown => r.held = Some(image.clone()),
                    Ok(_) => {}
                    Err(_) => r.lost = true,
                }
                true
            }) == Some(true);
            if !wanted {
                return;
            }
            if let Err(err) = result {
                lost(e, err);
            }
            bump(e, item);
        }
        Waiter::Sharpen { item } => {
            if !e.is_alive(item) {
                return;
            }
            let wanted = with_run(e, item, |r| {
                let same = r.key.as_ref().is_some_and(|k| {
                    k.url == key.url && k.request.at_scale(1.0) == key.request.at_scale(1.0)
                });
                if !same || r.sharpening != Some(key.request.scale) {
                    return false;
                }
                if let Ok(image) = result {
                    r.sharpening = None;
                    if r.shown || r.pinned {
                        r.sharp = Some((key.request.scale, image.clone()));
                    }
                }
                true
            }) == Some(true);
            // A drawing that cannot be made sharper is shown as it was.
            if wanted && result.is_ok() {
                bump(e, item);
            }
        }
        Waiter::Canvas { item } => crate::canvas::image_arrived(e, item, key, result),
    }
}

/// A picture that was dropped to save memory cannot be read again (its
/// file changed or went). Qt never drops one, so it has no words for it.
fn lost(e: &Engine, err: &LoadError) {
    e.log(
        LogLevel::Warning,
        CATEGORY,
        format!("a picture dropped to save memory did not load again: {err}"),
    );
}

/// The pixels the item shows, for the scene's paint effect: reading them
/// makes the effect run again when they change. Pixels the store dropped
/// are decoded again, on the spot for a synchronous item and in the
/// background (painting nothing meanwhile) for an asynchronous one.
fn pixels(e: &Engine, me: ObjectId) -> Option<ImageHandle> {
    follow(e, me);
    let frame = with_run(e, me, |r| {
        let frames = r.frames.as_ref()?;
        r.shown = true;
        frames
            .images
            .get(r.frame)
            .or(frames.images.first())
            .cloned()
    })
    .flatten();
    if frame.is_some() {
        return frame;
    }
    let (held, key, lost_before) = with_run(e, me, |r| {
        r.shown = true;
        let held = if r.pinned {
            r.held.clone()
        } else {
            r.held.take()
        };
        (held, r.key.clone(), r.lost)
    })?;
    if held.is_some() {
        return held;
    }
    let key = key?;
    let store = ImageStore::of(e);
    if let Some(image) = store.cached(&key) {
        return Some(image);
    }
    if lost_before {
        return None;
    }
    if in_background(e, me, &key.url) {
        let ask = with_run(e, me, |r| !std::mem::replace(&mut r.reshowing, true)) == Some(true);
        if ask {
            store.request(e, key, Waiter::Reshow { item: me });
        }
        return None;
    }
    match store.load_now(&key, true) {
        Ok(image) => Some(image),
        Err(err) => {
            with_run(e, me, |r| r.lost = true);
            lost(e, &err);
            None
        }
    }
}

/// The pixels of the drawing the item shows made at the display's scale,
/// when that is not one and they are there or can be made on the spot. An
/// asynchronous item has them made in the background and gets `None`
/// meanwhile, to paint the drawing at one device pixel per logical pixel.
/// Pictures that are not drawings are never made again.
fn sharp_pixels(e: &Engine, me: ObjectId) -> Option<ImageHandle> {
    let key = with_run(e, me, |r| r.key.clone()).flatten()?;
    let store = ImageStore::of(e);
    if !store.is_vector(&key.url) {
        return None;
    }
    let want = ImageKey {
        request: key.request.at_scale(store.device_scale(e)),
        url: key.url,
    };
    let scale = want.request.scale;
    if scale == 0 {
        return None;
    }
    follow(e, me);
    let (sharp, pinned, asked) = with_run(e, me, |r| {
        r.shown = true;
        let sharp = match &r.sharp {
            Some((s, image)) if *s == scale => Some(image.clone()),
            _ => None,
        };
        if !r.pinned && sharp.is_some() {
            r.sharp = None;
        }
        (sharp, r.pinned, r.sharpening == Some(scale))
    })?;
    if sharp.is_some() {
        return sharp;
    }
    if !pinned && let Some(image) = store.cached(&want) {
        return Some(image);
    }
    if asked {
        return None;
    }
    if in_background(e, me, &want.url) {
        with_run(e, me, |r| r.sharpening = Some(scale));
        store.request(e, want, Waiter::Sharpen { item: me });
        return None;
    }
    match store.load_now(&want, !pinned) {
        Ok(image) => {
            if pinned {
                with_run(e, me, |r| r.sharp = Some((scale, image.clone())));
            }
            Some(image)
        }
        Err(_) => {
            with_run(e, me, |r| r.sharpening = Some(scale));
            None
        }
    }
}

/// The scene stopped painting `me` (hidden, scrolled away): pixels kept
/// for its next paint are let go.
pub(crate) fn hidden(e: &Engine, me: ObjectId) {
    with_run(e, me, |r| {
        r.shown = false;
        if !r.pinned {
            r.held = None;
            r.sharp = None;
        }
    });
}

/// Keeps `paintedWidth` and `paintedHeight` in step (an effect).
fn update_painted(e: &Engine, me: ObjectId) {
    if !e.is_alive(me) {
        return;
    }
    follow(e, me);
    let area = kurbo::Size::new(
        e.read(me, Item::WIDTH.index).to_number(),
        e.read(me, Item::HEIGHT.index).to_number(),
    );
    let fit = fit_of(int(e, me, Image::FILL_MODE.index));
    let natural = with_run(e, me, |r| r.natural).unwrap_or_default();
    let painted = if natural.0 == 0 || natural.1 == 0 {
        // Qt, with no picture: the stretching and tiling modes still
        // report the item's size.
        match fit {
            ImageFit::Fit | ImageFit::Crop | ImageFit::Pad => kurbo::Size::ZERO,
            _ => area,
        }
    } else {
        let natural = kurbo::Size::new(f64::from(natural.0), f64::from(natural.1));
        painted_size(natural, area, fit)
    };
    e.batch(|| {
        set(
            e,
            me,
            Image::PAINTED_WIDTH.index,
            Value::Real(painted.width),
        );
        set(
            e,
            me,
            Image::PAINTED_HEIGHT.index,
            Value::Real(painted.height),
        );
    });
}

/// What an `Image` or `BorderImage` paints. Built by the scene's paint
/// effect; holding it (and the display list made from it) is what keeps
/// the pixels of a visible item from being dropped.
#[derive(Clone, Debug)]
pub struct ImagePaint {
    image: Option<ImageHandle>,
    /// The size of the picture in logical pixels, which is the size of
    /// `image` unless it is a drawing made at the display's scale.
    logical: (u32, u32),
    smooth: bool,
    shape: Shape,
}

#[derive(Clone, Debug)]
enum Shape {
    Picture {
        fit: ImageFit,
        align: [ImageAlign; 2],
        mirror: [bool; 2],
    },
    Border {
        insets: Insets,
        tile: [TileMode; 2],
        mirror: bool,
    },
}

impl ImagePaint {
    /// Reads the paint properties and the pixels (tracked), one device
    /// pixel to the logical pixel: what a `Canvas` or an effect draws from.
    pub fn read(e: &Engine, obj: ObjectId) -> Self {
        Self::read_at(e, obj, false)
    }

    /// As [`ImagePaint::read`], with a drawing made at the scale of the
    /// display the scene paints on, so that it is as sharp there as it is
    /// at one device pixel per logical pixel.
    pub fn read_on_screen(e: &Engine, obj: ObjectId) -> Self {
        Self::read_at(e, obj, true)
    }

    fn read_at(e: &Engine, obj: ObjectId, on_screen: bool) -> Self {
        let flag = |p: PropIndex| e.read(obj, p).to_boolean();
        let sharp = (on_screen && is_a::<Image>(e, obj))
            .then(|| sharp_pixels(e, obj))
            .flatten();
        let image = sharp
            .or_else(|| pixels(e, obj))
            .map(|image| tint::tinted(e, obj, image));
        let logical = match &image {
            Some(image) => with_run(e, obj, |r| r.natural)
                .filter(|n| n.0 > 0 && n.1 > 0)
                .unwrap_or((image.width(), image.height())),
            None => (0, 0),
        };
        let smooth = flag(ImageBase::SMOOTH.index);
        let mirror = flag(ImageBase::MIRROR.index);
        let shape = if is_a::<BorderImage>(e, obj) {
            border::shape(e, obj, mirror)
        } else if is_a::<Image>(e, obj) {
            let h = match int(e, obj, Image::HORIZONTAL_ALIGNMENT.index) {
                align::LEFT => ImageAlign::Start,
                align::RIGHT => ImageAlign::End,
                _ => ImageAlign::Center,
            };
            let v = match int(e, obj, Image::VERTICAL_ALIGNMENT.index) {
                align::TOP => ImageAlign::Start,
                align::BOTTOM => ImageAlign::End,
                _ => ImageAlign::Center,
            };
            Shape::Picture {
                fit: fit_of(int(e, obj, Image::FILL_MODE.index)),
                align: [h, v],
                mirror: [mirror, flag(Image::MIRROR_VERTICALLY.index)],
            }
        } else {
            Shape::Picture {
                fit: ImageFit::Stretch,
                align: [ImageAlign::Center; 2],
                mirror: [mirror, false],
            }
        };
        Self {
            image,
            logical,
            smooth,
            shape,
        }
    }

    /// The pixels painted, if they are there.
    pub fn image(&self) -> Option<&ImageHandle> {
        self.image.as_ref()
    }

    pub fn paint(&self, size: kurbo::Size, out: &mut DisplayListBuilder) {
        let Some(image) = &self.image else { return };
        if size.width <= 0.0 || size.height <= 0.0 {
            return;
        }
        match &self.shape {
            Shape::Picture { fit, align, mirror } => {
                let natural =
                    kurbo::Size::new(f64::from(self.logical.0), f64::from(self.logical.1));
                if let Some(p) = place_image(natural, size, *fit, *align, *mirror) {
                    // From logical pixels of the picture to its pixels.
                    let kx = f64::from(image.width()) / natural.width;
                    let ky = f64::from(image.height()) / natural.height;
                    let src = kurbo::Rect::new(
                        p.src.x0 * kx,
                        p.src.y0 * ky,
                        p.src.x1 * kx,
                        p.src.y1 * ky,
                    );
                    out.push(DrawOp::Image {
                        image: image.clone(),
                        src,
                        dst: p.dst,
                        tile: p.tile,
                        mirror: *mirror,
                        smooth: self.smooth,
                    });
                }
            }
            Shape::Border {
                insets,
                tile,
                mirror,
            } => {
                out.push(DrawOp::NinePatch {
                    image: image.clone(),
                    insets: *insets,
                    dst: kurbo::Rect::from_origin_size((0.0, 0.0), size),
                    tile: *tile,
                    mirror: *mirror,
                    smooth: self.smooth,
                });
            }
        }
    }
}
