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
use store::Picture;
pub use store::{ImageKey, ImageReply, ImageStore};
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
use crate::item::{Item, is_a, size_follows_implicit, write_computed};
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
    /// The pixels are decoded in the background, the first time and each
    /// time they are made again ([`decode_aside`]).
    aside: bool,
    /// Sharper pixels than those of `key` and what was asked for them: a
    /// drawing made at the display's scale, or a picture kept smaller for
    /// its `limit` that is drawn larger or read whole. Like `held`: for
    /// good when `pinned`, otherwise until the next paint takes them.
    sharp: Option<(Request, ImageHandle)>,
    /// The sharper pixels being made in the background, or that failed to
    /// be made: they are not asked for again.
    sharpening: Option<Request>,
    /// A graph node that changes whenever the pixels do, so that the
    /// scene's paint effect and the painted size follow.
    epoch: Option<NodeId>,
    /// The frames of an animation an `AnimatedImage` shows, and which of
    /// them it shows. Its own: the cache keeps single pictures.
    frames: Option<std::sync::Arc<decode::Frames>>,
    frame: usize,
    /// How many pages the document shown has: 1 for a picture, 0 for
    /// nothing shown.
    pages: u32,
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
    /// The page of a PDF document shown; of a still picture, always the
    /// first: there is no other.
    #[qml(notify)]
    pub current_frame: i32,
    /// The pages of a PDF document, 1 for a still picture, there or
    /// failed; 0 until one of those, and left as it was by an empty source,
    /// as in Qt. An `AnimatedImage` counts its frames.
    #[qml(notify, readonly)]
    pub frame_count: i32,
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
            current_frame: 0,
            frame_count: 0,
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
        if !is_a::<AnimatedImage>(e, me) {
            e.watch(me, Image::CURRENT_FRAME.index, move |e, v| {
                turn_to(e, me, v.to_number());
            })?;
        }
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

/// `frameCount` of an `Image` that is no `AnimatedImage`, after its
/// `status`, in Qt's order: the `pages` of a document, 1 for a picture.
/// Qt leaves it alone when the source is cleared.
fn count_still_frames(e: &Engine, me: ObjectId, status: i32, pages: u32) {
    if status != image_status::NULL && is_a::<Image>(e, me) && !is_a::<AnimatedImage>(e, me) {
        let pages = i32::try_from(pages.max(1)).unwrap_or(i32::MAX);
        set(e, me, Image::FRAME_COUNT.index, Value::Int(pages));
    }
}

/// `currentFrame` of an `Image` that is no `AnimatedImage` was set to
/// `frame`: a page of the document shown is turned to, and anything else
/// is ignored, as in Qt, which puts back the page shown (the first of a
/// picture, its only one).
fn turn_to(e: &Engine, me: ObjectId, frame: f64) {
    let Some((pages, shown)) = with_run(e, me, |r| {
        (r.pages, r.key.as_ref().map_or(0, |k| k.request.page))
    }) else {
        return;
    };
    let is_page = frame >= 0.0 && frame.fract() == 0.0 && frame < f64::from(pages.max(1));
    if !is_page {
        let back = if pages > 1 { shown } else { 0 };
        set(
            e,
            me,
            Image::CURRENT_FRAME.index,
            Value::Int(i32::try_from(back).unwrap_or(0)),
        );
    } else if frame as u32 != shown {
        reload(e, me);
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
    let animated = is_a::<AnimatedImage>(e, me);
    // Every frame of an animation is the item's own, and kept whole.
    let limit = if animated {
        0
    } else {
        ImageStore::of(e).limit()
    };
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
            animated,
            limit,
            page: if animated {
                0
            } else {
                // A page there is not shows the first, but is kept as
                // asked, as in Qt.
                let frame = e.read(me, Image::CURRENT_FRAME.index).to_number();
                if frame > 0.0 { frame as u32 } else { 0 }
            },
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
            r.aside = false;
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
        store.showing(e, me, None);
        unload(e, me, image_status::NULL);
        return;
    }
    let key = ImageKey {
        url: e.resolve_url(me, &source),
        request: request_of(e, me),
    };
    store.showing(e, me, Some(&key.url));
    // The cache keeps single pictures; the frames are the item's own.
    let cache = cache && !key.request.animated;
    // A picture another item has loaded is there at once, also for an
    // asynchronous Image.
    if cache && let Some((key, picture)) = store.cached_like(&key) {
        finish(e, me, key, picture, cache, None);
        return;
    }
    if is_local(&key.url) && !asynchronous {
        let plain = key.request.width == 0
            && key.request.height == 0
            && key.request.clip.is_none()
            && !key.request.animated;
        if cache
            && plain
            && let Some(natural) = store.header_size(&key)
            && u64::from(natural.0) * u64::from(natural.1) > ASIDE_PIXELS
        {
            decode_aside(e, me, key, natural, generation);
            return;
        }
        let loaded = if key.request.animated {
            store.decode_now(&key).map(|l| {
                (
                    Picture {
                        image: l.image,
                        natural: l.natural,
                        pages: l.pages,
                    },
                    l.frames,
                )
            })
        } else {
            store.load_picture_now(&key, cache).map(|p| (p, None))
        };
        match loaded {
            Ok((picture, frames)) => finish(e, me, key, picture, cache, frames),
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
/// background: always for a network source, which is never waited for,
/// and for a picture whose pixels came that way the first time when they
/// are painted `on_screen`. Whatever else reads them (a `Canvas`, an
/// effect) has them at once, as from a picture loaded on the spot.
fn in_background(e: &Engine, me: ObjectId, url: &str, on_screen: bool) -> bool {
    matches!(decode::scheme(url).as_deref(), Some("http" | "https"))
        || (on_screen && with_run(e, me, |r| r.aside) == Some(true))
        || e.untrack(|| e.read(me, ImageBase::ASYNCHRONOUS.index).to_boolean())
}

/// The item shows `picture` from now on, or the `frames` it is the first
/// of.
fn finish(
    e: &Engine,
    me: ObjectId,
    key: ImageKey,
    picture: Picture,
    cache: bool,
    frames: Option<std::sync::Arc<decode::Frames>>,
) {
    let Picture {
        image,
        natural,
        pages,
    } = picture;
    let asked = with_run(e, me, |r| {
        r.frames = frames;
        r.frame = 0;
        r.pages = pages;
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
    announce(e, me, natural, asked, pages);
    // After the batch: the item's size may follow the picture's.
    fit_unpainted(e, me);
}

/// The item says it is Ready with a picture of `natural` size, or a page
/// that size of a document of `pages`; `asked` is whether the author set
/// `sourceSize`, which then stays as set.
fn announce(e: &Engine, me: ObjectId, natural: (u32, u32), asked: bool, pages: u32) {
    let (w, h) = (f64::from(natural.0), f64::from(natural.1));
    // In the order Qt announces them; a handler of any of them sees the
    // others already in place.
    e.batch(|| {
        bump(e, me);
        animated::loaded(e, me);
        set(e, me, ImageBase::PROGRESS.index, Value::Real(1.0));
        let (iw, ih) = e.untrack(|| implicit_size(e, me, (w, h)));
        set(e, me, Item::IMPLICIT_WIDTH.index, Value::Real(iw));
        set(e, me, Item::IMPLICIT_HEIGHT.index, Value::Real(ih));
        set(
            e,
            me,
            ImageBase::STATUS.index,
            Value::Int(image_status::READY),
        );
        count_still_frames(e, me, image_status::READY, pages);
        if !asked {
            report_size(e, me, w, h);
        }
    });
}

/// Shows `key`, a local picture of `natural` size as its header says, as
/// a load on the spot does: Ready, with its size, at once. Its pixels are
/// decoded in the background, at the size the item draws it once its
/// bindings have followed that size, and painted when they come, so that
/// many large pictures do not hold the user interface up while they
/// decode. Qt decodes them on the spot. A host whose loads start after
/// the frame asks for it just before the frame, once the item is laid
/// out ([`request_aside`]).
fn decode_aside(e: &Engine, me: ObjectId, key: ImageKey, natural: (u32, u32), generation: u64) {
    let asked = with_run(e, me, |r| {
        r.frames = None;
        r.frame = 0;
        r.pages = 1;
        r.key = Some(key.clone());
        r.natural = natural;
        r.held = None;
        r.pinned = false;
        // Nothing to ask for when it is painted: it is on its way.
        r.reshowing = true;
        r.lost = false;
        r.sharp = None;
        r.sharpening = None;
        r.aside = true;
        r.requested.is_some()
    }) == Some(true);
    announce(e, me, natural, asked, 1);
    let store = ImageStore::of(e);
    if store.defers_asides() {
        store.defer_aside(e, me, generation);
        return;
    }
    request_aside(e, me, generation);
}

/// Asks for the pixels [`decode_aside`] put off, at the size `me` is
/// drawn now, unless it has loaded something else since.
pub(super) fn request_aside(e: &Engine, me: ObjectId, generation: u64) {
    let Some((key, natural)) = with_run(e, me, |r| {
        (r.generation == generation && r.aside)
            .then(|| r.key.clone().map(|k| (k, r.natural)))
            .flatten()
    })
    .flatten() else {
        return;
    };
    let store = ImageStore::of(e);
    let limit = e.untrack(|| {
        let sized = e.read(me, Item::WIDTH.index).to_number() > 0.0
            && e.read(me, Item::HEIGHT.index).to_number() > 0.0;
        sized.then(|| wanted_limit(e, me, natural, store.device_scale(e)))
    });
    let longest = |(w, h): (u32, u32)| w.max(h);
    let key = match limit {
        Some(limit)
            if longest(decode::kept_size(natural, limit))
                < longest(decode::kept_size(natural, key.request.limit)) =>
        {
            ImageKey {
                request: Request {
                    limit,
                    ..key.request
                },
                url: key.url,
            }
        }
        _ => key,
    };
    with_run(e, me, |r| r.key = Some(key.clone()));
    store.request_for_frame(
        e,
        key,
        Waiter::Load {
            item: me,
            generation,
            cache: true,
        },
    );
}

/// A local picture with more pixels than this has them decoded in the
/// background ([`decode_aside`]); a smaller one takes less time to decode
/// than a frame and is decoded on the spot, so that it is in the first.
const ASIDE_PIXELS: u64 = 512 * 512;

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
        r.pages = 0;
        r.requested.is_some()
    }) == Some(true);
    e.batch(|| {
        bump(e, me);
        animated::loaded(e, me);
        set(e, me, ImageBase::PROGRESS.index, Value::Real(0.0));
        set(e, me, Item::IMPLICIT_WIDTH.index, Value::Real(0.0));
        set(e, me, Item::IMPLICIT_HEIGHT.index, Value::Real(0.0));
        set(e, me, ImageBase::STATUS.index, Value::Int(status));
        count_still_frames(e, me, status, 1);
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
                    Picture {
                        image: l.image.clone(),
                        natural: l.natural,
                        pages: l.pages,
                    },
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
                let same = r.key.as_ref().is_some_and(|k| k.url == key.url);
                if !same || r.sharpening != Some(key.request) {
                    return false;
                }
                if let Ok(image) = result {
                    r.sharpening = None;
                    if r.shown || r.pinned {
                        r.sharp = Some((key.request, image.clone()));
                    }
                }
                true
            }) == Some(true);
            // Pixels that cannot be made sharper are shown as they were.
            if wanted && result.is_ok() {
                bump(e, item);
            }
        }
        Waiter::Canvas { item } => crate::canvas::image_arrived(e, item, key, result),
        // Answered by the store itself.
        Waiter::Reply { .. } => {}
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
/// background (painting nothing meanwhile) for an asynchronous one, or
/// one decoded aside that is painted `on_screen`.
fn pixels(e: &Engine, me: ObjectId, on_screen: bool) -> Option<ImageHandle> {
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
    if let Some((kept, picture)) = store.cached_like(&key) {
        if kept != key {
            with_run(e, me, |r| r.key = Some(kept));
        }
        return Some(picture.image);
    }
    if lost_before {
        return None;
    }
    if in_background(e, me, &key.url, on_screen) {
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

/// Sharper pixels than those the item loaded, when they are wanted and
/// there or can be made on the spot: a drawing made at the display's
/// scale, and a picture kept smaller for its `limit` that is drawn larger
/// than its pixels on screen (`on_screen`) or is read whole (otherwise).
/// An asynchronous item has them made in the background and gets `None`
/// meanwhile, to paint what it loaded.
fn sharp_pixels(e: &Engine, me: ObjectId, on_screen: bool) -> Option<ImageHandle> {
    let (key, natural) = with_run(e, me, |r| r.key.clone().map(|k| (k, r.natural))).flatten()?;
    let store = ImageStore::of(e);
    let request = if store.is_vector(&key.url) {
        if !on_screen {
            return None;
        }
        let request = key.request.at_scale(store.device_scale(e));
        if request.scale == 0 {
            return None;
        }
        request
    } else {
        let longest = |(w, h): (u32, u32)| w.max(h);
        let kept = longest(decode::kept_size(natural, key.request.limit));
        let sized = e.read(me, Item::WIDTH.index).to_number() > 0.0
            && e.read(me, Item::HEIGHT.index).to_number() > 0.0;
        if on_screen && !sized {
            // Drawn at no size is not drawn small: what it will be drawn
            // at is still to come.
            return None;
        }
        let limit = if on_screen {
            wanted_limit(e, me, natural, store.device_scale(e))
        } else {
            0
        };
        let wanted = longest(decode::kept_size(natural, limit));
        // Made smaller only when that saves at least half the bytes.
        if wanted * 3 <= kept * 2 {
            return shrunk(e, me, &key, natural, limit);
        }
        if wanted <= kept {
            return None;
        }
        // A limit the picture is no longer than is no limit: one key for
        // the whole picture.
        let limit = if wanted >= longest(natural) { 0 } else { limit };
        Request {
            limit,
            ..key.request
        }
    };
    let want = ImageKey {
        request,
        url: key.url,
    };
    follow(e, me);
    let (sharp, pinned, asked) = with_run(e, me, |r| {
        r.shown = true;
        let sharp = match &r.sharp {
            Some((s, image)) if *s == request => Some(image.clone()),
            _ => None,
        };
        if !r.pinned && sharp.is_some() {
            r.sharp = None;
        }
        (sharp, r.pinned, r.sharpening == Some(request))
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
    if in_background(e, me, &want.url, on_screen) {
        with_run(e, me, |r| r.sharpening = Some(request));
        store.request(e, want, Waiter::Sharpen { item: me });
        return None;
    }
    match store.load_now(&want, !pinned) {
        Ok(image) => {
            if pinned {
                with_run(e, me, |r| r.sharp = Some((request, image.clone())));
            }
            Some(image)
        }
        Err(_) => {
            with_run(e, me, |r| r.sharpening = Some(request));
            None
        }
    }
}

/// The pixels of `me` made smaller for `limit`, when it is
/// drawn that much smaller than they are: made from what is kept, with
/// nothing fetched or decoded again, and kept in their place. Only once a
/// load, and never after sharper pixels were asked for, so that an item
/// whose size goes up and down does not swing between the two.
fn shrunk(
    e: &Engine,
    me: ObjectId,
    key: &ImageKey,
    natural: (u32, u32),
    limit: u32,
) -> Option<ImageHandle> {
    let (pinned, settled) = with_run(e, me, |r| {
        (
            r.pinned,
            r.frames.is_some() || r.sharp.is_some() || r.sharpening.is_some(),
        )
    })?;
    if settled || key.request.animated {
        return None;
    }
    let source = pixels(e, me, true)?;
    // Painting may have found them kept for another limit.
    let key = with_run(e, me, |r| r.key.clone()).flatten()?;
    Some(shrink_kept(e, me, &key, natural, limit, source, pinned))
}

/// Makes `source`, the pixels of `key` the item has, smaller for `limit`
/// and keeps them in their place: the item's own when `pinned`, the
/// cache's otherwise. `source` itself when that cannot be done.
fn shrink_kept(
    e: &Engine,
    me: ObjectId,
    key: &ImageKey,
    natural: (u32, u32),
    limit: u32,
    source: ImageHandle,
    pinned: bool,
) -> ImageHandle {
    let kept = decode::kept_size(natural, limit);
    if kept.0 >= source.width() || kept.1 >= source.height() {
        return source;
    }
    let image = match decode::Raw::of(&source, &source.data()).shrunk_to(kept, &key.url) {
        Ok(image) => image,
        Err(_) => return source,
    };
    // Made again as it was made: its source made again, then shrunk.
    // Not when what that holds outweighs the smaller pixels: they could
    // never be let go, and holding it would keep the bytes for nothing.
    if let Some(again) = source
        .made_again_by()
        .filter(|again| again.holds < image.byte_len())
    {
        let (size, url) = ((source.width(), source.height()), key.url.clone());
        image.make_again_with(ljos_render::MakeAgain {
            holds: again.holds,
            make: std::sync::Arc::new(move || {
                let from = ImageHandle::from_premultiplied(size.0, size.1, (again.make)()?).ok()?;
                let smaller = decode::Raw::of(&from, &from.data())
                    .shrunk_to(kept, &url)
                    .ok()?;
                Some(smaller.into_data())
            }),
        });
    }
    let smaller = ImageKey {
        request: Request {
            limit,
            ..key.request
        },
        url: key.url.clone(),
    };
    with_run(e, me, |r| {
        if pinned {
            r.held = Some(image.clone());
        }
        r.key = Some(smaller.clone());
    });
    if !pinned {
        let store = ImageStore::of(e);
        store.keep(
            &smaller,
            &Picture {
                image: image.clone(),
                natural,
                // Only a picture is kept smaller; a document is drawn.
                pages: 1,
            },
        );
        drop(source);
        store.release(key);
    }
    image
}

/// The size in pixels of the picture kept for the image item `item`,
/// which may be smaller than the picture itself: what the item draws, or
/// the view, needs no more. None while nothing is kept for it.
pub fn kept_pixels(e: &Engine, item: ObjectId) -> Option<(u32, u32)> {
    let (key, held) = with_run(e, item, |r| (r.key.clone(), r.held.clone()))?;
    let image = held.or_else(|| ImageStore::of(e).cached(&key?))?;
    Some((image.width(), image.height()))
}

/// Whether `me` shows a picture of `url`, at whatever size it keeps it.
pub(crate) fn loads_from(e: &Engine, me: ObjectId, url: &str) -> bool {
    e.is_alive(me)
        && with_run(e, me, |r| r.key.as_ref().is_some_and(|k| k.url == url)) == Some(true)
}

/// Whether `me` shows the picture kept under `key`.
pub(crate) fn shows(e: &Engine, me: ObjectId, key: &ImageKey) -> bool {
    with_run(e, me, |r| r.key.as_ref() == Some(key)) == Some(true)
}

/// Keeps the picture of an item that is made and not painted (a list's
/// row kept beyond the view, a page not shown) at the size the item
/// draws it, as its first paint would. Without this it stays at the
/// view's size until then, for as long as the item lives. Nothing is read
/// or fetched again: the pixels are made from what is kept, and a row
/// scrolled back in at the same size paints them as they are. Only once a
/// scene has said how large it is, which is also what the device scale
/// is known from. Run when the picture arrives and, as the painted size
/// is kept, whenever the item's size or fill mode changes: a row is
/// often laid out after its picture is.
fn fit_unpainted(e: &Engine, me: ObjectId) {
    if !is_a::<Image>(e, me) {
        return;
    }
    let Some((key, natural, pinned, held)) = with_run(e, me, |r| {
        let idle = !r.shown && r.frames.is_none() && r.sharp.is_none() && r.sharpening.is_none();
        idle.then(|| {
            r.key
                .clone()
                .map(|k| (k, r.natural, r.pinned, r.held.clone()))
        })
        .flatten()
    })
    .flatten() else {
        return;
    };
    let store = ImageStore::of(e);
    if key.request.animated || store.limit() == 0 || store.is_vector(&key.url) {
        return;
    }
    let limit = e.untrack(|| {
        let sized = e.read(me, Item::WIDTH.index).to_number() > 0.0
            && e.read(me, Item::HEIGHT.index).to_number() > 0.0;
        sized.then(|| wanted_limit(e, me, natural, store.device_scale(e)))
    });
    let Some(limit) = limit else { return };
    let longest = |(w, h): (u32, u32)| w.max(h);
    let kept = longest(decode::kept_size(natural, key.request.limit));
    let wanted = longest(decode::kept_size(natural, limit));
    // As at a paint: only when that saves at least half the bytes.
    if wanted * 3 > kept * 2 {
        return;
    }
    let source = if pinned { held } else { store.cached(&key) };
    if let Some(source) = source {
        shrink_kept(e, me, &key, natural, limit, source, pinned);
    }
}

/// The `limit` a picture of `natural` size needs to be as sharp as the
/// item draws it, at `device` pixels per logical pixel (tracked: it
/// follows the item's size, fill mode and scale, and its parents' scale).
fn wanted_limit(e: &Engine, me: ObjectId, natural: (u32, u32), device: f32) -> u32 {
    let area = kurbo::Size::new(
        e.read(me, Item::WIDTH.index).to_number(),
        e.read(me, Item::HEIGHT.index).to_number(),
    );
    let picture = kurbo::Size::new(f64::from(natural.0), f64::from(natural.1));
    let shown = match fit_of(int(e, me, Image::FILL_MODE.index)) {
        fit @ (ImageFit::Stretch | ImageFit::Fit | ImageFit::Crop) => {
            painted_size(picture, area, fit)
        }
        _ => picture,
    };
    let factor = (shown.width / picture.width).max(shown.height / picture.height)
        * f64::from(device)
        * drawn_scale(e, me);
    decode::step_up(picture.width.max(picture.height) * factor)
}

/// How much larger than its own size `me` is drawn: its `scale` and that
/// of each item it is in (tracked). A picture shown in a thumbnail that
/// is a scaled-down item is drawn that much smaller.
fn drawn_scale(e: &Engine, me: ObjectId) -> f64 {
    let mut scale = 1.0;
    let mut at = Some(me);
    while let Some(item) = at.filter(|&i| is_a::<Item>(e, i)) {
        let s = e.read(item, Item::SCALE.index).to_number().abs();
        // The picture's own scale below 1 does not keep fewer pixels: it
        // is mostly on its way to 1 (a pop-in, a press), and every step
        // back up would decode the picture again. A parent's scale holds.
        let s = if item == me { s.max(1.0) } else { s };
        if s.is_finite() && s > 0.0 {
            scale *= s;
        }
        at = e.visual_parent(item);
    }
    scale
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
    let natural = with_run(e, me, |r| r.natural).unwrap_or_default();
    if natural.0 > 0 && natural.1 > 0 {
        // First: a side that follows the implicit size is then as long as
        // it will be painted, and nothing painted in between sees it 0.
        let (w, h) = implicit_size(e, me, (f64::from(natural.0), f64::from(natural.1)));
        e.batch(|| {
            set(e, me, Item::IMPLICIT_WIDTH.index, Value::Real(w));
            set(e, me, Item::IMPLICIT_HEIGHT.index, Value::Real(h));
        });
    }
    let area = kurbo::Size::new(
        e.read(me, Item::WIDTH.index).to_number(),
        e.read(me, Item::HEIGHT.index).to_number(),
    );
    let fit = fit_of(int(e, me, Image::FILL_MODE.index));
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
    e.untrack(|| fit_unpainted(e, me));
}

/// An Image's implicit size for a picture of `natural` size. Qt: a
/// picture kept in shape (`PreserveAspectFit`) that is given one side and
/// not the other is, along the other, as long as the given side leaves it,
/// and never longer than it is (observed on qml6). A row of a title beside
/// a picture given only a width is as tall as the picture is drawn, not
/// as the picture is.
fn implicit_size(e: &Engine, me: ObjectId, natural: (f64, f64)) -> (f64, f64) {
    let (w, h) = natural;
    if w <= 0.0
        || h <= 0.0
        || !is_a::<Image>(e, me)
        || int(e, me, Image::FILL_MODE.index) != fill_mode::PRESERVE_ASPECT_FIT
    {
        return natural;
    }
    let given = [
        !size_follows_implicit(e, me, true),
        !size_follows_implicit(e, me, false),
    ];
    match given {
        [true, false] => {
            let width = e.read(me, Item::WIDTH.index).to_number();
            (w, h * (width / w).clamp(0.0, 1.0))
        }
        [false, true] => {
            let height = e.read(me, Item::HEIGHT.index).to_number();
            (w * (height / h).clamp(0.0, 1.0), h)
        }
        _ => natural,
    }
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
        let sharp = is_a::<Image>(e, obj)
            .then(|| sharp_pixels(e, obj, on_screen))
            .flatten();
        let image = sharp
            .or_else(|| pixels(e, obj, on_screen))
            .map(|image| tint::tinted(e, obj, image));
        let logical = match &image {
            Some(image) => with_run(e, obj, |r| r.natural)
                .filter(|n| n.0 > 0 && n.1 > 0)
                .unwrap_or((image.width(), image.height())),
            None => (0, 0),
        };
        let smooth = flag(Item::SMOOTH.index);
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
                        opacity: 1.0,
                        tint: None,
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
