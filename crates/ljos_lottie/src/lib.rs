//! The `Qt.labs.lottieqt` module for Ljós: `LottieAnimation`, an item
//! that plays a Lottie (Bodymovin) JSON animation.
//!
//! This is an opt-in module crate written against the public `ljos`
//! facade alone. The `ljos` command has it in `run`, `render` and
//! `test`; any other host registers it like any native module:
//!
//! ```ignore
//! let engine = ljos::engine();
//! ljos_lottie::register(&engine);
//! ```
//!
//! QML then says `import Qt.labs.lottieqt`:
//!
//! ```qml
//! LottieAnimation {
//!     source: "spinner.json"
//!     loops: LottieAnimation.Infinite
//!     onFinished: console.log("done")
//! }
//! ```
//!
//! ## The surface
//!
//! Matching Qt 6.11.2 as probed on `qml6` (the scenarios in
//! `features/lottie/`): `source`, `status` (`Null`, `Loading`, `Ready`,
//! `Error`), `autoPlay`, `loops` (`LottieAnimation.Infinite` is -1),
//! `frameRate`, `startFrame` and `endFrame` (read-only, from the file),
//! `direction` (`Forward`, `Reverse`) and `quality` (`LowQuality`,
//! `MediumQuality`, `HighQuality`); `start()`, `play()`, `pause()`,
//! `togglePause()`, `stop()`, `gotoAndPlay(frame or marker)`,
//! `gotoAndStop(frame or marker)` and `getDuration(inFrames)`; the
//! `finished()` signal.
//!
//! - A local file is read and ready by the time the component
//!   completes, as in Qt; one larger than [`BACKGROUND_BYTES`], a remote
//!   one and a registered resource are read off the UI thread, `status`
//!   staying `Loading` until they are.
//! - Loading sets the item's size to the file's canvas, and `frameRate`
//!   to the file's.
//! - Each tick of `1000 / frameRate` ms moves one frame; the last frame
//!   shows for a tick before a run ends ([`clock`]).
//! - `quality` is kept but changes nothing: every frame is drawn
//!   anti-aliased.
//!
//! ## Drawing
//!
//! The file is read into a model of our own ([`model`]) and each frame
//! becomes filled and stroked paths ([`render`]) in the display list
//! QtQuick.Shapes uses. What is not drawn yet is listed in
//! `docs/ARCHITECTURE.md` (Lottie).
//!
//! ## At rest
//!
//! The frame clock runs only while the animation plays and can be seen:
//! one deadline per frame. Hidden, a finite animation keeps a single
//! deadline, at the moment it finishes, so `finished` comes on time;
//! an endless one keeps none and catches up when shown. Paused, stopped
//! or finished, nothing is scheduled.

pub mod anim;
pub mod clock;
pub mod model;
pub mod render;

use std::fmt;
use std::rc::Rc;
use std::time::Duration;

use ljos::item::kurbo::{Affine, Size};
use ljos::item::{self, DisplayListBuilder, ItemPaint, TextEngine};
use ljos::prelude::*;

use crate::clock::Clock;
use crate::model::{Composition, LoadError};
use crate::render::Op;

const CATEGORY: &str = "qt.lottieqt.lottie.parser";

/// A local file larger than this is read and parsed on the compute pool,
/// so a big animation never stalls the UI thread; a smaller one loads at
/// once, as Qt loads every local file.
pub const BACKGROUND_BYTES: u64 = 256 * 1024;

mod status {
    pub const NULL: i32 = 0;
    pub const LOADING: i32 = 1;
    pub const READY: i32 = 2;
    pub const ERROR: i32 = 3;
}

fn enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![
        (
            "Status",
            vec![
                ("Null", i64::from(status::NULL)),
                ("Loading", i64::from(status::LOADING)),
                ("Ready", i64::from(status::READY)),
                ("Error", i64::from(status::ERROR)),
            ],
        ),
        (
            "Quality",
            vec![("LowQuality", 0), ("MediumQuality", 1), ("HighQuality", 2)],
        ),
        ("Direction", vec![("Forward", 1), ("Reverse", -1)]),
        ("LoopCount", vec![("Infinite", i64::from(clock::INFINITE))]),
    ]
}

/// Registers `LottieAnimation` (`Qt.labs.lottieqt`) on `engine`: the one
/// seam a host calls.
pub fn register(engine: &Engine) {
    engine.register_module(ljos::module!("Qt.labs.lottieqt" 1.0 => [LottieAnimation]));
    item::register_paint_item::<LottieAnimation>(engine, read);
}

/// `LottieAnimation` (Qt.labs.lottieqt): a Lottie animation, played.
#[derive(QmlObject)]
#[qml(
    module = "Qt.labs.lottieqt",
    name = "LottieAnimation",
    extends = Item,
    enums = enums
)]
pub struct LottieAnimation {
    #[qml(base)]
    pub item: Item,
    /// The Lottie JSON file.
    #[qml(notify)]
    pub source: Url,
    #[qml(notify, readonly, enumeration = "Status")]
    pub status: i32,
    /// Frames a second; the file's once it loads.
    #[qml(notify)]
    pub frame_rate: i32,
    /// Whether it starts as soon as it is loaded.
    #[qml(notify)]
    pub auto_play: bool,
    /// How many times it plays; `Infinite` for ever.
    #[qml(notify)]
    pub loops: i32,
    #[qml(notify, enumeration = "Quality")]
    pub quality: i32,
    #[qml(notify, enumeration = "Direction")]
    pub direction: i32,
    #[qml(notify, readonly)]
    pub start_frame: i32,
    #[qml(notify, readonly)]
    pub end_frame: i32,
    /// Bumped by every load, so an answer to an older one is dropped.
    #[qml(skip)]
    generation: u64,
    #[qml(skip)]
    composition: Option<Rc<Composition>>,
    #[qml(skip)]
    clock: Clock,
    /// When the frame on screen began: ticks count from here.
    #[qml(skip)]
    anchor: Time,
    #[qml(skip)]
    deadline: Option<TimerId>,
    /// Whether the item can be seen, as of the last look.
    #[qml(skip)]
    shown: bool,
}

impl fmt::Debug for LottieAnimation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LottieAnimation")
            .field("source", &self.source)
            .field("status", &self.status)
            .field("clock", &self.clock)
            .finish_non_exhaustive()
    }
}

impl Default for LottieAnimation {
    fn default() -> Self {
        LottieAnimation {
            item: Item::default(),
            source: Url::default(),
            status: status::NULL,
            frame_rate: 30,
            auto_play: true,
            loops: 1,
            // Qt 6.11.2 starts at HighQuality, whatever its docs say.
            quality: 2,
            direction: 1,
            start_frame: 0,
            end_frame: 0,
            generation: 0,
            composition: None,
            clock: Clock::default(),
            anchor: Time::ZERO,
            deadline: None,
            shown: false,
        }
    }
}

#[qml_impl]
impl LottieAnimation {
    #[qml(signal)]
    fn finished();

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, me) = (cx.engine, cx.this);
        load(e, me);
        e.watch(me, LottieAnimation::SOURCE.index, move |e, _| load(e, me))?;
        e.watch(me, LottieAnimation::FRAME_RATE.index, move |e, _| {
            // A new pace counts from now.
            settle(e, me);
            let now = e.now();
            update(e, me, |m| m.anchor = now);
            reschedule(e, me);
        })?;
        for prop in [
            LottieAnimation::LOOPS.index,
            LottieAnimation::DIRECTION.index,
        ] {
            e.watch(me, prop, move |e, _| {
                settle(e, me);
                let (loops, direction) = e.untrack(|| {
                    (
                        e.get(me, LottieAnimation::LOOPS).unwrap_or(1),
                        e.get(me, LottieAnimation::DIRECTION).unwrap_or(1),
                    )
                });
                update(e, me, |m| {
                    m.clock.loops = loops;
                    m.clock.direction = if direction < 0 { -1 } else { 1 };
                });
                reschedule(e, me);
            })?;
        }
        e.effect_in(me, move |e| {
            let now = shown_tracked(e, me);
            e.untrack(|| {
                let was = e.with(me, |m: &LottieAnimation| m.shown).unwrap_or(false);
                if now != was {
                    settle(e, me);
                    update(e, me, |m| m.shown = now);
                    reschedule(e, me);
                    if now {
                        item::repaint(e, me);
                    }
                }
            });
        })?;
        Ok(())
    }

    /// From the first frame (the last, in reverse), playing.
    #[qml(invokable)]
    fn start(cx: &Cx) -> Result<(), EngineError> {
        let (e, me) = (cx.engine, cx.this);
        if loaded(e, me) {
            settle(e, me);
            let now = e.now();
            update(e, me, |m| {
                m.clock.restart();
                m.anchor = now;
            });
            changed(e, me);
        }
        Ok(())
    }

    /// Plays on from the frame on screen.
    #[qml(invokable)]
    fn play(cx: &Cx) -> Result<(), EngineError> {
        play(cx.engine, cx.this);
        Ok(())
    }

    /// Holds the frame on screen.
    #[qml(invokable)]
    fn pause(cx: &Cx) -> Result<(), EngineError> {
        pause(cx.engine, cx.this);
        Ok(())
    }

    #[qml(invokable, name = "togglePause")]
    fn toggle_pause(cx: &Cx) -> Result<(), EngineError> {
        let (e, me) = (cx.engine, cx.this);
        if e.with(me, |m: &LottieAnimation| m.clock.playing)? {
            pause(e, me);
        } else {
            play(e, me);
        }
        Ok(())
    }

    /// Back to the first frame, not playing, without `finished`.
    #[qml(invokable)]
    fn stop(cx: &Cx) -> Result<(), EngineError> {
        let (e, me) = (cx.engine, cx.this);
        settle(e, me);
        update(e, me, |m| {
            m.clock.playing = false;
            m.clock.loops_done = 0;
            m.clock.frame = m.clock.start;
        });
        changed(e, me);
        Ok(())
    }

    /// Goes to a frame, or to the frame of the marker of that name, and
    /// plays from there. Says whether a marker by the name was found.
    #[qml(invokable, name = "gotoAndPlay")]
    fn goto_and_play(cx: &Cx, frame: Value) -> Result<Value, EngineError> {
        Ok(goto(cx.engine, cx.this, &frame, true))
    }

    /// Goes to a frame, or to the frame of the marker of that name, and
    /// stays there.
    #[qml(invokable, name = "gotoAndStop")]
    fn goto_and_stop(cx: &Cx, frame: Value) -> Result<Value, EngineError> {
        Ok(goto(cx.engine, cx.this, &frame, false))
    }

    /// How long one run is: in seconds, or in frames when `in_frames`.
    #[qml(invokable, name = "getDuration")]
    fn get_duration(&self, in_frames: Value) -> f64 {
        let frames = f64::from(self.end_frame - self.start_frame);
        if in_frames.to_boolean() {
            frames
        } else if self.frame_rate > 0 {
            frames / f64::from(self.frame_rate)
        } else {
            0.0
        }
    }
}

fn update(e: &Engine, me: ObjectId, f: impl FnOnce(&mut LottieAnimation)) {
    let _ = e.untrack(|| e.with_mut(me, f));
}

fn loaded(e: &Engine, me: ObjectId) -> bool {
    e.with(me, |m: &LottieAnimation| m.composition.is_some())
        .unwrap_or(false)
}

fn play(e: &Engine, me: ObjectId) {
    if !loaded(e, me) {
        return;
    }
    settle(e, me);
    let now = e.now();
    let started = e
        .untrack(|| {
            e.with_mut(me, |m: &mut LottieAnimation| {
                let was = m.clock.playing;
                m.clock.playing = true;
                if !was {
                    m.anchor = now;
                }
                !was
            })
        })
        .unwrap_or(false);
    if started {
        reschedule(e, me);
    }
}

fn pause(e: &Engine, me: ObjectId) {
    settle(e, me);
    update(e, me, |m| m.clock.playing = false);
    reschedule(e, me);
}

fn goto(e: &Engine, me: ObjectId, to: &Value, playing: bool) -> Value {
    let Ok(Some(comp)) = e.with(me, |m: &LottieAnimation| m.composition.clone()) else {
        return Value::Bool(false);
    };
    let (frame, answer) = match to {
        Value::String(name) => match comp.markers.iter().find(|m| m.name == name.as_str()) {
            Some(marker) => (marker.frame, Value::Bool(true)),
            None => return Value::Bool(false),
        },
        v => (v.to_number(), Value::Undefined),
    };
    if frame.is_nan() {
        return answer;
    }
    settle(e, me);
    let now = e.now();
    update(e, me, |m| {
        m.clock.frame = m.clock.clamp(frame.trunc());
        m.clock.loops_done = 0;
        m.clock.playing = playing;
        m.anchor = now;
    });
    changed(e, me);
    answer
}

/// After a jump: a new frame to paint and a new deadline.
fn changed(e: &Engine, me: ObjectId) {
    reschedule(e, me);
    item::repaint(e, me);
}

/// How long a tick is: `1000 / frameRate` ms, rounded down as Qt's timer.
fn tick(m: &LottieAnimation) -> Duration {
    let rate = u64::try_from(m.frame_rate.max(1)).unwrap_or(1);
    Duration::from_millis((1000 / rate).max(1))
}

/// Brings the clock up to now: the ticks that have passed since the
/// anchor move the frame, and `finished` is told if one ended the run.
fn settle(e: &Engine, me: ObjectId) {
    let now = e.now();
    let Ok((moved, finished)) = e.untrack(|| {
        e.with_mut(me, |m: &mut LottieAnimation| {
            if !m.clock.playing {
                return (false, false);
            }
            let tick = tick(m);
            let ticks = now.saturating_since(m.anchor).as_micros() / tick.as_micros().max(1);
            let ticks = u64::try_from(ticks).unwrap_or(u64::MAX);
            if ticks == 0 {
                return (false, false);
            }
            let before = m.clock.frame;
            let finished = m.clock.advance(ticks);
            let span = u32::try_from(ticks).map_or(Duration::MAX, |n| tick * n);
            m.anchor = m.anchor.saturating_add(span);
            (m.clock.frame != before, finished)
        })
    }) else {
        return;
    };
    if moved {
        item::repaint(e, me);
    }
    if finished {
        reschedule(e, me);
        let _ = e.emit_by_name(me, "finished", &[]);
    }
}

/// Arms the one deadline the animation needs now, if any: the next
/// tick while it is seen, the end of its last run while it is not.
fn reschedule(e: &Engine, me: ObjectId) {
    let Ok((old, next)) = e.untrack(|| {
        e.with_mut(me, |m: &mut LottieAnimation| {
            let old = m.deadline.take();
            let tick = tick(m);
            let next = if !m.clock.playing || m.composition.is_none() {
                None
            } else if m.shown {
                Some(m.anchor.saturating_add(tick))
            } else {
                m.clock.ticks_to_finish().map(|n| {
                    let span = u32::try_from(n).map_or(Duration::MAX, |n| tick * n);
                    m.anchor.saturating_add(span)
                })
            };
            (old, next)
        })
    }) else {
        return;
    };
    if let Some(id) = old {
        e.cancel_call(id);
    }
    if let Some(at) = next {
        let id = e.call_at(at, move |e| {
            update(e, me, |m| m.deadline = None);
            settle(e, me);
            reschedule(e, me);
        });
        update(e, me, |m| m.deadline = Some(id));
    }
}

/// Whether the item can be seen: it has an area, and neither it nor an
/// item it is in is invisible or fully transparent. Tracked.
fn shown_tracked(e: &Engine, me: ObjectId) -> bool {
    let w = e.get(me, Item::WIDTH).unwrap_or(0.0);
    let h = e.get(me, Item::HEIGHT).unwrap_or(0.0);
    if w <= 0.0 || h <= 0.0 {
        return false;
    }
    let mut at = Some(me);
    while let Some(o) = at {
        if !e.get(o, Item::VISIBLE).unwrap_or(true) || e.get(o, Item::OPACITY).unwrap_or(1.0) <= 0.0
        {
            return false;
        }
        at = e.visual_parent(o);
    }
    true
}

/// What one load carries back.
enum Outcome {
    Bytes(Vec<u8>),
    Parsed(Result<Composition, LoadError>),
    Missing,
}

/// Starts (or restarts) loading `source`.
fn load(e: &Engine, me: ObjectId) {
    let source = e.untrack(|| e.read(me, LottieAnimation::SOURCE.index).to_js_string());
    let Ok(generation) = e.untrack(|| {
        e.with_mut(me, |m: &mut LottieAnimation| {
            m.generation += 1;
            m.composition = None;
            m.clock.playing = false;
            m.generation
        })
    }) else {
        return;
    };
    reschedule(e, me);
    item::repaint(e, me);
    if source.is_empty() {
        set_status(e, me, status::NULL);
        return;
    }
    set_status(e, me, status::LOADING);
    let url = e.resolve_url(me, &source);
    let platform = Platform::of(e);
    if let Some(bytes) = platform.resource(&url) {
        let bytes = bytes.to_vec();
        if bytes.len() as u64 > BACKGROUND_BYTES {
            parse_in_background(e, me, generation, bytes);
        } else {
            arrived(e, me, generation, Outcome::Bytes(bytes));
        }
        return;
    }
    if is_remote(&url) {
        platform.fetch_for(Some(me), &url, move |e, got| match got {
            Ok(bytes) => parse_in_background(e, me, generation, bytes),
            Err(_) => arrived(e, me, generation, Outcome::Missing),
        });
        return;
    }
    let Some(path) = url_path(&url).and_then(|p| platform.env().real(&p).ok()) else {
        arrived(e, me, generation, Outcome::Missing);
        return;
    };
    match std::fs::metadata(&path) {
        Ok(meta) if meta.is_file() && meta.len() > BACKGROUND_BYTES => {
            let spawned = ljos::task::spawn(
                e,
                move || match std::fs::read(&path) {
                    Ok(bytes) => Outcome::Parsed(model::parse(&bytes)),
                    Err(_) => Outcome::Missing,
                },
                move |e, got| arrived(e, me, generation, got.unwrap_or(Outcome::Missing)),
            );
            if spawned.is_err() {
                arrived(e, me, generation, Outcome::Missing);
            }
        }
        Ok(meta) if meta.is_file() => {
            let outcome = std::fs::read(&path).map_or(Outcome::Missing, Outcome::Bytes);
            arrived(e, me, generation, outcome);
        }
        _ => arrived(e, me, generation, Outcome::Missing),
    }
}

fn parse_in_background(e: &Engine, me: ObjectId, generation: u64, bytes: Vec<u8>) {
    let spawned = ljos::task::spawn(
        e,
        move || Outcome::Parsed(model::parse(&bytes)),
        move |e, got| arrived(e, me, generation, got.unwrap_or(Outcome::Missing)),
    );
    if spawned.is_err() {
        arrived(e, me, generation, Outcome::Missing);
    }
}

fn set_status(e: &Engine, me: ObjectId, s: i32) {
    let _ = e.set_internal(me, LottieAnimation::STATUS, s);
}

/// A load's answer: the animation, ready, or an error.
fn arrived(e: &Engine, me: ObjectId, generation: u64, outcome: Outcome) {
    let current = e.is_alive(me)
        && e.with(me, |m: &LottieAnimation| m.generation == generation)
            .unwrap_or(false);
    if !current {
        return;
    }
    let parsed = match outcome {
        Outcome::Bytes(bytes) => model::parse(&bytes),
        Outcome::Parsed(parsed) => parsed,
        Outcome::Missing => {
            set_status(e, me, status::ERROR);
            return;
        }
    };
    let comp = match parsed {
        Ok(comp) => comp,
        Err(err) => {
            if matches!(err, LoadError::Json(_)) {
                e.log(LogLevel::Warning, CATEGORY, err.to_string());
            }
            set_status(e, me, status::ERROR);
            return;
        }
    };
    if comp.layers.is_empty() {
        e.log(LogLevel::Warning, CATEGORY, "Layers are empty".to_owned());
    }
    for warning in &comp.warnings {
        e.log(LogLevel::Info, CATEGORY, warning.clone());
    }
    let start = comp.in_point.round() as i32;
    let end = comp.out_point.round() as i32;
    let rate = comp.frame_rate.round() as i32;
    let (w, h) = (comp.width, comp.height);
    let comp = Rc::new(comp);
    let (loops, direction) = e.untrack(|| {
        (
            e.get(me, LottieAnimation::LOOPS).unwrap_or(1),
            e.get(me, LottieAnimation::DIRECTION).unwrap_or(1),
        )
    });
    update(e, me, |m| {
        m.composition = Some(comp);
        m.clock = Clock {
            start: f64::from(start),
            end: f64::from(end),
            direction: if direction < 0 { -1 } else { 1 },
            loops,
            frame: f64::from(start),
            loops_done: 0,
            playing: false,
        };
    });
    let _ = e.set_internal(me, LottieAnimation::START_FRAME, start);
    let _ = e.set_internal(me, LottieAnimation::END_FRAME, end);
    let _ = e.set(me, LottieAnimation::FRAME_RATE, rate);
    let _ = e.set(me, Item::WIDTH, w);
    let _ = e.set(me, Item::HEIGHT, h);
    item::repaint(e, me);
    set_status(e, me, status::READY);
    let auto = e.untrack(|| e.get(me, LottieAnimation::AUTO_PLAY).unwrap_or(true));
    if auto && e.is_alive(me) {
        let now = e.now();
        update(e, me, |m| {
            m.clock.restart();
            m.anchor = now;
        });
        changed(e, me);
    }
}

/// One frame, drawn.
#[derive(Debug)]
struct Frame(Vec<Op>);

impl ItemPaint for Frame {
    fn paint(&self, _: Size, _: &mut TextEngine, out: &mut DisplayListBuilder) {
        out.reserve(self.0.len());
        for op in &self.0 {
            out.shape(op.path.clone(), op.style.clone(), op.brush.clone());
        }
    }
}

fn read(e: &Engine, me: ObjectId) -> Rc<dyn ItemPaint> {
    let w = item::prop(e, me, Item::WIDTH);
    let h = item::prop(e, me, Item::HEIGHT);
    let Ok(Some((comp, frame))) = e.with(me, |m: &LottieAnimation| {
        m.composition.clone().map(|c| (c, m.clock.frame))
    }) else {
        return Rc::new(Frame(Vec::new()));
    };
    let view = if comp.width > 0.0 && comp.height > 0.0 {
        Affine::scale_non_uniform(w / comp.width, h / comp.height)
    } else {
        Affine::IDENTITY
    };
    Rc::new(Frame(render::render(&comp, frame, view)))
}
