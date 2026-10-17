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
//! deadline until it can be seen again. qml6 decodes them all the same,
//! and its `videoSink` hears of each; here the sink hears of none.
//!
//! Frames reach an output through its `videoSink` ([`crate::video_sink`]),
//! and are painted turned by `orientation` (clockwise, in quarter turns),
//! then mirrored about the vertical axis.

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
use crate::video_sink;

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
    /// Degrees clockwise, a multiple of 90; another value is not used,
    /// though it reads back (qml6 keeps the last multiple of 90).
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
    #[qml(readonly, object = "VideoSink")]
    pub video_sink: Option<ObjectId>,
    #[qml(skip)]
    frame: Option<ImageHandle>,
    /// The frame turned by `turns` quarter turns, for 90 and 270.
    #[qml(skip)]
    turned: Option<(u8, ImageHandle)>,
    /// The last orientation that was a multiple of 90, in quarter turns.
    #[qml(skip)]
    turns: u8,
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
            video_sink: None,
            frame: None,
            turned: None,
            turns: 0,
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
    let sink = video_sink::make_for(e, me);
    e.set_internal(me, VideoOutput::VIDEO_SINK, sink)?;
    let first = e.get(me, VideoOutput::ORIENTATION)?;
    keep_turns(e, me, first);
    e.watch(me, VideoOutput::ORIENTATION.index, move |e, v| {
        keep_turns(e, me, v.to_number() as i32);
    })?;
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
    let turned = turns(e, me) % 2 == 1;
    let (sw, sh) = if turned {
        (source.height, source.width)
    } else {
        (source.width, source.height)
    };
    fit(w, h, sw, sh, fill)
}

/// `degrees` in quarter turns clockwise, when a multiple of 90.
fn quarter_turns(degrees: i32) -> Option<u8> {
    (degrees % 90 == 0).then(|| (degrees / 90).rem_euclid(4) as u8)
}

fn keep_turns(e: &Engine, me: ObjectId, degrees: i32) {
    if let Some(t) = quarter_turns(degrees) {
        let _ = e.with_mut::<VideoOutput, _>(me, |v| v.turns = t);
    }
}

/// The quarter turns `me` paints with, tracking `orientation`: one that
/// is no multiple of 90 leaves the last that was.
fn turns(e: &Engine, me: ObjectId) -> u8 {
    let degrees = e.get(me, VideoOutput::ORIENTATION).unwrap_or(0);
    quarter_turns(degrees).unwrap_or_else(|| {
        e.with::<VideoOutput, _>(me, |v| v.turns)
            .unwrap_or_default()
    })
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

/// Whether `output` can be seen, not tracked. A sink of the program's
/// always is.
pub(crate) fn shown(e: &Engine, output: Option<ObjectId>) -> bool {
    output.is_some_and(|o| {
        e.is_alive(o) && (video_sink::is_sink(e, o) || e.untrack(|| shown_tracked(e, o)))
    })
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
    if let Some(o) = output.filter(|o| !video_sink::is_sink(e, *o)) {
        let _ = e.with_mut::<VideoOutput, _>(o, |v| v.player = Some(player));
    }
}

/// Gives `image` to `target`, an output or a sink.
pub(crate) fn show(e: &Engine, target: Option<ObjectId>, image: ImageHandle) {
    if let Some(sink) = video_sink::of(e, target) {
        video_sink::show(e, sink, image);
    }
}

/// Says to `target`'s sink the size of the video about to come.
pub(crate) fn sized(e: &Engine, target: Option<ObjectId>, width: f64, height: f64) {
    if let Some(sink) = video_sink::of(e, target) {
        video_sink::sized(e, sink, ljos_engine::Size { width, height });
    }
}

/// Takes the video out of `target`, an output or a sink.
pub(crate) fn clear(e: &Engine, target: Option<ObjectId>) {
    if let Some(sink) = video_sink::of(e, target) {
        video_sink::clear(e, sink);
    }
}

/// Paints `image` in `o`, as its sink was given it.
pub(crate) fn paint(e: &Engine, o: ObjectId, image: ImageHandle) {
    if !e.is_alive(o) {
        return;
    }
    let r = image.rect();
    let rect = Rect {
        x: 0.0,
        y: 0.0,
        width: r.width(),
        height: r.height(),
    };
    let Ok(serial) = e.with_mut::<VideoOutput, _>(o, |v| {
        v.frame = Some(image);
        v.turned = None;
        v.serial
    }) else {
        return;
    };
    let _ = e.set_internal(o, VideoOutput::SOURCE_RECT, rect);
    bump(e, serial);
}

/// Takes the frame out of `o`.
pub(crate) fn unpaint(e: &Engine, o: ObjectId) {
    if !e.is_alive(o) {
        return;
    }
    let Ok((had, serial)) = e.with_mut::<VideoOutput, _>(o, |v| {
        v.turned = None;
        (v.frame.take().is_some(), v.serial)
    }) else {
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

/// Whether `output` lets go of the last frame at the end of the stream; a
/// sink of the program's does.
pub(crate) fn clears_at_end(e: &Engine, output: Option<ObjectId>) -> bool {
    output.is_none_or(|o| {
        video_sink::is_sink(e, o)
            || e.untrack(|| e.get(o, VideoOutput::END_OF_STREAM_POLICY))
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
    /// Horizontally, then vertically.
    mirror: [bool; 2],
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
            mirror: self.mirror,
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
    let turns = turns(e, me);
    // A half turn is both mirrors; a quarter turn moves the pixels, once
    // per frame and orientation.
    let half = turns == 2;
    let image = e
        .with_mut::<VideoOutput, _>(me, |v| {
            let frame = v.frame.clone()?;
            if turns.is_multiple_of(2) {
                return Some(frame);
            }
            match &v.turned {
                Some((t, image)) if *t == turns => Some(image.clone()),
                _ => {
                    let image = turn(&frame, turns).unwrap_or(frame);
                    v.turned = Some((turns, image.clone()));
                    Some(image)
                }
            }
        })
        .ok()
        .flatten();
    Rc::new(FramePaint {
        image,
        content,
        mirror: [mirrored != half, half],
    })
}

/// `image` turned clockwise by `turns` quarter turns.
pub(crate) fn turn(image: &ImageHandle, turns: u8) -> Option<ImageHandle> {
    let (w, h) = (image.width() as usize, image.height() as usize);
    let src = image.data();
    let turns = turns % 4;
    if turns == 0 {
        return Some(image.clone());
    }
    let (tw, th) = if turns % 2 == 1 { (h, w) } else { (w, h) };
    let mut out = vec![0u8; src.len()];
    for y in 0..h {
        for x in 0..w {
            let (nx, ny) = match turns {
                1 => (h - 1 - y, x),
                2 => (w - 1 - x, h - 1 - y),
                _ => (y, w - 1 - x),
            };
            let from = (y * w + x) * 4;
            let to = (ny * tw + nx) * 4;
            out[to..to + 4].copy_from_slice(&src[from..from + 4]);
        }
    }
    ImageHandle::from_premultiplied(tw as u32, th as u32, out).ok()
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

    #[test]
    fn only_multiples_of_90_turn() {
        assert_eq!(quarter_turns(90), Some(1));
        assert_eq!(quarter_turns(-90), Some(3));
        assert_eq!(quarter_turns(540), Some(2));
        assert_eq!(quarter_turns(45), None);
    }

    #[test]
    fn a_quarter_turn_is_clockwise() {
        // 2 by 1: red, then green.
        let image =
            ImageHandle::from_premultiplied(2, 1, vec![255, 0, 0, 255, 0, 255, 0, 255]).unwrap();
        let t = turn(&image, 1).unwrap();
        assert_eq!((t.width(), t.height()), (1, 2));
        // Clockwise, the left end goes to the top.
        assert_eq!(t.pixel(0, 0), [255, 0, 0, 255]);
        assert_eq!(t.pixel(0, 1), [0, 255, 0, 255]);
        let t = turn(&image, 3).unwrap();
        assert_eq!(t.pixel(0, 0), [0, 255, 0, 255]);
    }
}
