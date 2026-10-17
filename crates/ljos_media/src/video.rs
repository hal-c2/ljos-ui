//! `VideoOutput`, where a player's frames are shown, and `Video`, an
//! output with its own player.
//!
//! A frame reaches the screen as an image in the item's display list,
//! like an `Image`'s: the decode thread converts it, the output keeps the
//! newest, and the scene repaints the item because the output's frame
//! counter, which its paint reads, moved.
//!
//! An output that cannot be seen (invisible, transparent, or of no size,
//! itself or an ancestor) asks for no frames: its player arms no frame
//! deadline until it can be seen again.

use std::fmt;
use std::rc::Rc;
use std::sync::Arc;

use ljos_engine::{
    Atom, Cx, Engine, EngineError, MetaRef, NodeId, ObjectId, QmlObject, QmlType, Rect, Url, Value,
    qml_impl,
};
use ljos_quick::{Item, ItemPaint, TextEngine};
use ljos_render::{DisplayListBuilder, DrawOp, ImageHandle, kurbo};

use crate::devices::AudioOutput;
use crate::player::{self, MediaPlayer};

fn video_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![
        (
            "FillMode",
            vec![
                ("Stretch", 0),
                ("PreserveAspectFit", 1),
                ("PreserveAspectCrop", 2),
            ],
        ),
        (
            "EndOfStreamPolicy",
            vec![("ClearOutput", 0), ("KeepLastFrame", 1)],
        ),
    ]
}

const STRETCH: i32 = 0;
const CROP: i32 = 2;
const CLEAR_OUTPUT: i32 = 0;

/// `VideoOutput` (QtMultimedia).
#[derive(QmlObject)]
#[qml(
    module = "QtMultimedia",
    name = "VideoOutput",
    extends = Item,
    default = "data",
    enums = video_enums
)]
pub struct VideoOutput {
    #[qml(base)]
    pub item: Item,
    #[qml(notify, enumeration = "FillMode")]
    pub fill_mode: i32,
    #[qml(notify, enumeration = "EndOfStreamPolicy")]
    pub end_of_stream_policy: i32,
    /// Degrees, a multiple of 90.
    #[qml(notify)]
    pub orientation: i32,
    #[qml(notify)]
    pub mirrored: bool,
    /// The frame's own rectangle; empty with no frame.
    #[qml(notify, readonly)]
    pub source_rect: Rect,
    /// Where the frame is shown in the item.
    #[qml(notify, readonly)]
    pub content_rect: Rect,
    #[qml(skip)]
    frame: Option<ImageHandle>,
    #[qml(skip)]
    serial: Option<NodeId>,
    #[qml(skip)]
    player: Option<ObjectId>,
}

impl fmt::Debug for VideoOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VideoOutput")
            .field("fill_mode", &self.fill_mode)
            .field("source_rect", &self.source_rect)
            .field("frame", &self.frame.is_some())
            .finish_non_exhaustive()
    }
}

impl Default for VideoOutput {
    fn default() -> Self {
        VideoOutput {
            item: Item::default(),
            fill_mode: 1,
            end_of_stream_policy: CLEAR_OUTPUT,
            orientation: 0,
            mirrored: false,
            source_rect: Rect::default(),
            content_rect: Rect::default(),
            frame: None,
            serial: None,
            player: None,
        }
    }
}

#[qml_impl]
impl VideoOutput {
    #[qml(signal, name = "sourceChanged")]
    fn source_changed();

    /// Takes the frame shown away.
    #[qml(invokable, name = "clearOutput")]
    fn clear_output(cx: &Cx) -> Result<(), EngineError> {
        clear(cx.engine, Some(cx.this));
        Ok(())
    }

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        output_completed(cx.engine, cx.this)
    }
}

fn output_completed(e: &Engine, me: ObjectId) -> Result<(), EngineError> {
    e.bind_initial(me, VideoOutput::CONTENT_RECT.index, move |e| {
        Value::Rect(Arc::new(content_rect(e, me)))
    })?;
    // Seen or not: the player arms frames only for an output that is.
    let mut was = false;
    e.effect_in(me, move |e| {
        let now = shown_tracked(e, me);
        if now != was {
            was = now;
            if let Some(p) = e
                .untrack(|| e.with::<VideoOutput, _>(me, |v| v.player))
                .ok()
                .flatten()
            {
                e.untrack(|| player::output_shown(e, p));
            }
        }
    })?;
    let scope = e.object_scope(me).unwrap_or_else(|| e.graph().root());
    let serial = e.graph().property(scope, Value::Int(0)).ok();
    e.with_mut::<VideoOutput, _>(me, |v| v.serial = serial)?;
    Ok(())
}

fn content_rect(e: &Engine, me: ObjectId) -> Rect {
    let w = e.get(me, Item::WIDTH).unwrap_or(0.0);
    let h = e.get(me, Item::HEIGHT).unwrap_or(0.0);
    let source = e.get(me, VideoOutput::SOURCE_RECT).unwrap_or_default();
    let fill = e.get(me, VideoOutput::FILL_MODE).unwrap_or(1);
    let turned = e
        .get(me, VideoOutput::ORIENTATION)
        .unwrap_or(0)
        .rem_euclid(180)
        == 90;
    let (sw, sh) = if turned {
        (source.height, source.width)
    } else {
        (source.width, source.height)
    };
    fit(w, h, sw, sh, fill)
}

/// Where a frame of `sw` by `sh` goes in an item of `w` by `h`.
fn fit(w: f64, h: f64, sw: f64, sh: f64, fill: i32) -> Rect {
    let full = Rect {
        x: 0.0,
        y: 0.0,
        width: w,
        height: h,
    };
    if sw <= 0.0 || sh <= 0.0 || fill == STRETCH {
        return full;
    }
    let (sx, sy) = (w / sw, h / sh);
    let scale = if fill == CROP { sx.max(sy) } else { sx.min(sy) };
    let (cw, ch) = (sw * scale, sh * scale);
    Rect {
        x: (w - cw) / 2.0,
        y: (h - ch) / 2.0,
        width: cw,
        height: ch,
    }
}

/// Whether `output` can be seen, not tracked.
pub(crate) fn shown(e: &Engine, output: Option<ObjectId>) -> bool {
    output.is_some_and(|o| e.is_alive(o) && e.untrack(|| shown_tracked(e, o)))
}

fn shown_tracked(e: &Engine, me: ObjectId) -> bool {
    let w = e.get(me, Item::WIDTH).unwrap_or(0.0);
    let h = e.get(me, Item::HEIGHT).unwrap_or(0.0);
    if w <= 0.0 || h <= 0.0 {
        return false;
    }
    let mut at = Some(me);
    while let Some(o) = at {
        if ljos_quick::is_item(e, o)
            && (!e.get(o, Item::VISIBLE).unwrap_or(true)
                || e.get(o, Item::OPACITY).unwrap_or(1.0) <= 0.0)
        {
            return false;
        }
        at = e.visual_parent(o);
    }
    true
}

/// Makes `player` the one that shows frames in `output`.
pub(crate) fn attach(e: &Engine, output: Option<ObjectId>, player: ObjectId) {
    if let Some(o) = output {
        let _ = e.with_mut::<VideoOutput, _>(o, |v| v.player = Some(player));
    }
}

/// Shows `image` in `output`.
pub(crate) fn show(e: &Engine, output: Option<ObjectId>, image: ImageHandle) {
    let Some(o) = output.filter(|o| e.is_alive(*o)) else {
        return;
    };
    let r = image.rect();
    let rect = Rect {
        x: 0.0,
        y: 0.0,
        width: r.width(),
        height: r.height(),
    };
    let Ok(serial) = e.with_mut::<VideoOutput, _>(o, |v| {
        v.frame = Some(image);
        v.serial
    }) else {
        return;
    };
    let _ = e.set_internal(o, VideoOutput::SOURCE_RECT, rect);
    bump(e, serial);
}

/// Takes the frame out of `output`.
pub(crate) fn clear(e: &Engine, output: Option<ObjectId>) {
    let Some(o) = output.filter(|o| e.is_alive(*o)) else {
        return;
    };
    let Ok((had, serial)) =
        e.with_mut::<VideoOutput, _>(o, |v| (v.frame.take().is_some(), v.serial))
    else {
        return;
    };
    let _ = e.set_internal(o, VideoOutput::SOURCE_RECT, Rect::default());
    if had {
        bump(e, serial);
    }
}

fn bump(e: &Engine, serial: Option<NodeId>) {
    if let Some(n) = serial {
        let next = e.graph().get_untracked(n).to_number() as i32 + 1;
        e.graph().set(n, Value::Int(next));
    }
}

/// Whether `output` lets go of the last frame at the end of the stream.
pub(crate) fn clears_at_end(e: &Engine, output: Option<ObjectId>) -> bool {
    output.is_none_or(|o| {
        e.untrack(|| e.get(o, VideoOutput::END_OF_STREAM_POLICY))
            .unwrap_or(CLEAR_OUTPUT)
            == CLEAR_OUTPUT
    })
}

/// What an output paints: its frame where `contentRect` says, cut to
/// the item.
#[derive(Debug)]
struct FramePaint {
    image: Option<ImageHandle>,
    content: Rect,
    mirrored: bool,
}

impl ItemPaint for FramePaint {
    fn paint(&self, size: kurbo::Size, _text: &mut TextEngine, out: &mut DisplayListBuilder) {
        let Some(image) = &self.image else {
            return;
        };
        let c = self.content;
        if c.width <= 0.0 || c.height <= 0.0 {
            return;
        }
        let item = kurbo::Rect::new(0.0, 0.0, size.width, size.height);
        let content = kurbo::Rect::new(c.x, c.y, c.x + c.width, c.y + c.height);
        let dst = content.intersect(item);
        if dst.width() <= 0.0 || dst.height() <= 0.0 {
            return;
        }
        // What of the frame falls inside the item (all of it, unless
        // cropped).
        let px = image.rect();
        let sx = px.width() / content.width();
        let sy = px.height() / content.height();
        let src = kurbo::Rect::new(
            (dst.x0 - content.x0) * sx,
            (dst.y0 - content.y0) * sy,
            (dst.x1 - content.x0) * sx,
            (dst.y1 - content.y0) * sy,
        );
        out.push(DrawOp::Image {
            image: image.clone(),
            src,
            dst,
            tile: None,
            mirror: [self.mirrored, false],
            smooth: true,
            opacity: 1.0,
            tint: None,
        });
    }
}

fn read_paint(e: &Engine, me: ObjectId) -> Rc<dyn ItemPaint> {
    let serial = e.with::<VideoOutput, _>(me, |v| v.serial).ok().flatten();
    if let Some(n) = serial {
        let _ = e.graph().get(n);
    }
    let content = e.get(me, VideoOutput::CONTENT_RECT).unwrap_or_default();
    let mirrored = e.get(me, VideoOutput::MIRRORED).unwrap_or(false);
    let image = e
        .with::<VideoOutput, _>(me, |v| v.frame.clone())
        .ok()
        .flatten();
    Rc::new(FramePaint {
        image,
        content,
        mirrored,
    })
}

pub(crate) fn register_paint(engine: &Engine) {
    ljos_quick::register_paint_item(
        engine,
        MetaRef::Static(<VideoOutput as QmlType>::static_meta()),
        read_paint,
    );
}

/// `Video` (QtMultimedia): a `VideoOutput` that plays its own `source`.
#[derive(Debug, QmlObject)]
#[qml(module = "QtMultimedia", name = "Video", extends = VideoOutput)]
pub struct Video {
    #[qml(base)]
    pub output: VideoOutput,
    #[qml(notify)]
    pub source: Url,
    #[qml(notify)]
    pub auto_play: bool,
    #[qml(notify)]
    pub loops: i32,
    #[qml(notify)]
    pub playback_rate: f64,
    #[qml(notify)]
    pub volume: f64,
    #[qml(notify)]
    pub muted: bool,
    #[qml(notify)]
    pub position: f64,
    #[qml(notify, readonly)]
    pub duration: f64,
    #[qml(notify, readonly)]
    pub seekable: bool,
    #[qml(notify, readonly)]
    pub has_audio: bool,
    #[qml(notify, readonly)]
    pub has_video: bool,
    #[qml(notify, readonly)]
    pub playback_state: i32,
    #[qml(notify, readonly)]
    pub buffer_progress: f64,
    #[qml(readonly, object = "MediaMetaData")]
    pub meta_data: Option<ObjectId>,
    #[qml(readonly)]
    pub error: i32,
    #[qml(readonly)]
    pub error_string: Atom,
    #[qml(skip)]
    player: Option<ObjectId>,
}

impl Default for Video {
    fn default() -> Self {
        Video {
            output: VideoOutput::default(),
            source: Url(Atom::EMPTY),
            auto_play: false,
            loops: 1,
            playback_rate: 1.0,
            volume: 1.0,
            muted: false,
            position: 0.0,
            duration: 0.0,
            seekable: false,
            has_audio: false,
            has_video: false,
            playback_state: 0,
            buffer_progress: 0.0,
            meta_data: None,
            error: 0,
            error_string: Atom::EMPTY,
            player: None,
        }
    }
}

#[qml_impl]
impl Video {
    #[qml(signal)]
    fn paused();
    #[qml(signal)]
    fn playing();
    #[qml(signal)]
    fn stopped();
    #[qml(signal, name = "errorOccurred")]
    fn error_occurred(error: i32, error_string: Atom);

    #[qml(invokable)]
    fn play(cx: &Cx) -> Result<(), EngineError> {
        if let Some(p) = inner(cx) {
            player::play(cx.engine, p);
        }
        Ok(())
    }

    #[qml(invokable)]
    fn pause(cx: &Cx) -> Result<(), EngineError> {
        if let Some(p) = inner(cx) {
            player::pause(cx.engine, p);
        }
        Ok(())
    }

    #[qml(invokable)]
    fn stop(cx: &Cx) -> Result<(), EngineError> {
        if let Some(p) = inner(cx) {
            player::stop(cx.engine, p);
        }
        Ok(())
    }

    /// Moves by `offset` milliseconds from where it is.
    #[qml(invokable)]
    fn seek(cx: &Cx, offset: f64) -> Result<(), EngineError> {
        if let Some(p) = inner(cx) {
            let at = cx.engine.get(p, MediaPlayer::POSITION)?;
            cx.engine.set(p, MediaPlayer::POSITION, at + offset)?;
        }
        Ok(())
    }

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        // `VideoOutput`'s hook has run: the base's come first.
        let e = cx.engine;
        let me = cx.this;
        let out = e.create_native_incomplete(AudioOutput::default(), Some(me))?;
        e.complete(out)?;
        let p = e.create_native_incomplete(MediaPlayer::default(), Some(me))?;
        cx.with_mut::<Video, _>(|v| v.player = Some(p))?;
        e.set(p, MediaPlayer::VIDEO_OUTPUT, Some(me))?;
        e.set(p, MediaPlayer::AUDIO_OUTPUT, Some(out))?;
        // What is written on the Video goes to its player and output.
        macro_rules! down {
            ($from:expr, $to_obj:expr, $to:expr) => {{
                let to_obj = $to_obj;
                e.set(to_obj, $to, cx.get($from)?)?;
                e.watch(me, $from.index, move |e, _| {
                    if let Ok(v) = e.get(me, $from) {
                        let _ = e.set(to_obj, $to, v);
                    }
                })?;
            }};
        }
        down!(Video::AUTO_PLAY, p, MediaPlayer::AUTO_PLAY);
        down!(Video::LOOPS, p, MediaPlayer::LOOPS);
        down!(Video::PLAYBACK_RATE, p, MediaPlayer::PLAYBACK_RATE);
        down!(Video::VOLUME, out, AudioOutput::VOLUME);
        down!(Video::MUTED, out, AudioOutput::MUTED);
        down!(Video::SOURCE, p, MediaPlayer::SOURCE);
        // What the player reports shows on the Video.
        macro_rules! up {
            ($from:expr, $to:expr) => {{
                let _ = e.set_internal(me, $to, e.get(p, $from)?);
                e.watch(p, $from.index, move |e, _| {
                    if let Ok(v) = e.get(p, $from) {
                        let _ = e.set_internal(me, $to, v);
                    }
                })?;
            }};
        }
        up!(MediaPlayer::DURATION, Video::DURATION);
        up!(MediaPlayer::SEEKABLE, Video::SEEKABLE);
        up!(MediaPlayer::HAS_AUDIO, Video::HAS_AUDIO);
        up!(MediaPlayer::HAS_VIDEO, Video::HAS_VIDEO);
        up!(MediaPlayer::BUFFER_PROGRESS, Video::BUFFER_PROGRESS);
        up!(MediaPlayer::META_DATA, Video::META_DATA);
        up!(MediaPlayer::POSITION, Video::POSITION);
        e.watch(me, Video::POSITION.index, move |e, v| {
            let to = v.to_number();
            if e.get(p, MediaPlayer::POSITION).ok() != Some(to) {
                let _ = e.set(p, MediaPlayer::POSITION, to);
            }
        })?;
        e.watch(p, MediaPlayer::PLAYBACK_STATE.index, move |e, v| {
            let s = v.to_number() as i32;
            let _ = e.set_internal(me, Video::PLAYBACK_STATE, s);
            let signal = match s {
                1 => "playing",
                2 => "paused",
                _ => "stopped",
            };
            let _ = e.emit_by_name(me, signal, &[]);
        })?;
        e.connect_by_name(p, "errorOccurred", move |e, args| {
            let error = args.first().cloned().unwrap_or(Value::Int(0));
            let text = args.get(1).cloned().unwrap_or(Value::from(""));
            let _ = e.set_internal(me, Video::ERROR, error.to_number() as i32);
            let _ = e.set_internal(me, Video::ERROR_STRING, text.to_js_string());
            let _ = e.emit_by_name(me, "errorOccurred", &[error, text]);
        })?;
        e.complete(p)?;
        Ok(())
    }
}

fn inner(cx: &Cx) -> Option<ObjectId> {
    cx.with::<Video, _>(|v| v.player).ok().flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frame_fits_as_qt_fits_it() {
        let r = fit(100.0, 100.0, 64.0, 48.0, 1);
        assert_eq!((r.x, r.y, r.width, r.height), (0.0, 12.5, 100.0, 75.0));
        let r = fit(100.0, 50.0, 64.0, 48.0, CROP);
        assert_eq!((r.x, r.y, r.width, r.height), (0.0, -12.5, 100.0, 75.0));
        let r = fit(100.0, 50.0, 64.0, 48.0, STRETCH);
        assert_eq!((r.x, r.y, r.width, r.height), (0.0, 0.0, 100.0, 50.0));
        let r = fit(100.0, 50.0, 0.0, 0.0, 1);
        assert_eq!((r.width, r.height), (100.0, 50.0));
    }
}
