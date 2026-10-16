//! `AnimatedImage`: an `Image` that plays the frames of an animated GIF.
//!
//! The frames are decoded with the picture, as the rest of the item is
//! (on the spot for a local file, in the background otherwise), and kept
//! by the item: the store's cache holds single pictures only. Playing is
//! an engine timer owned by the item, armed for the delay of the frame on
//! screen, so a still or paused animation costs nothing (ARCHITECTURE
//! principle 6).
//!
//! Behaviour follows Qt 6.11.2 as measured on qml6: the first frame is
//! there with the picture, `playing` stays true while paused, stopping
//! keeps the frame on screen and lets go of `paused`, starting again moves
//! on a frame at once, and a picture of one frame stops playing.

use std::cell::Cell;

use ljos_engine::{Cx, Engine, EngineError, LogLevel, ObjectId, QmlObject, Timer, Value, qml_impl};

use super::{CATEGORY, Image, bump, is_a, with_run, write_computed};

/// `AnimatedImage` (QtQuick).
///
/// `loops`, `finishBehavior` and `finished()` (Qt 6.12) are not there yet:
/// an animation repeats for ever.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick", name = "AnimatedImage", extends = Image)]
pub struct AnimatedImage {
    #[qml(base)]
    pub image: Image,
    #[qml(notify)]
    pub playing: bool,
    #[qml(notify)]
    pub paused: bool,
    #[qml(notify)]
    pub current_frame: i32,
    #[qml(notify, readonly)]
    pub frame_count: i32,
    /// How fast the frames go, as a share of their own pace.
    #[qml(notify)]
    pub speed: f64,
    #[qml(skip)]
    timer: Cell<Option<ObjectId>>,
}

impl Default for AnimatedImage {
    fn default() -> Self {
        Self {
            image: Image::default(),
            playing: true,
            paused: false,
            current_frame: 0,
            frame_count: 0,
            speed: 1.0,
            timer: Cell::new(None),
        }
    }
}

#[qml_impl]
impl AnimatedImage {
    /// `currentFrame` changed, whoever changed it.
    #[qml(signal, name = "frameChanged")]
    fn frame_changed();

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, me) = (cx.engine, cx.this);
        e.watch(me, AnimatedImage::PLAYING.index, move |e, v| {
            if v.to_boolean() {
                // Qt moves on a frame as soon as it is told to play again.
                if count(e, me) > 1 {
                    show(e, me, (frame(e, me) + 1) % count(e, me));
                }
            } else {
                set(e, me, AnimatedImage::PAUSED.index, Value::Bool(false));
            }
            schedule(e, me);
        })?;
        e.watch(me, AnimatedImage::PAUSED.index, move |e, _| schedule(e, me))?;
        e.watch(me, AnimatedImage::SPEED.index, move |e, _| schedule(e, me))?;
        e.watch(me, AnimatedImage::CURRENT_FRAME.index, move |e, v| {
            let n = count(e, me);
            let wanted = v.to_number();
            if n == 0 || !wanted.is_finite() || wanted < 0.0 {
                return;
            }
            let wanted = (wanted as usize).min(n - 1);
            if wanted != frame(e, me) {
                show(e, me, wanted);
                schedule(e, me);
            }
        })?;
        schedule(e, me);
        Ok(())
    }
}

/// The item's picture changed (it loaded, failed or was cleared): the
/// frames start again from the first.
pub(super) fn loaded(e: &Engine, me: ObjectId) {
    if !is_a::<AnimatedImage>(e, me) {
        return;
    }
    let n = count(e, me);
    set(
        e,
        me,
        AnimatedImage::FRAME_COUNT.index,
        Value::Int(i32::try_from(n).unwrap_or(i32::MAX)),
    );
    show(e, me, 0);
    if n == 1 {
        set(e, me, AnimatedImage::PLAYING.index, Value::Bool(false));
    }
    schedule(e, me);
}

fn count(e: &Engine, me: ObjectId) -> usize {
    with_run(e, me, |r| r.frames.as_ref().map_or(0, |f| f.images.len())).unwrap_or(0)
}

fn frame(e: &Engine, me: ObjectId) -> usize {
    with_run(e, me, |r| r.frame).unwrap_or(0)
}

/// Puts frame `n` on screen and says so.
fn show(e: &Engine, me: ObjectId, n: usize) {
    with_run(e, me, |r| r.frame = n);
    bump(e, me);
    let value = Value::Int(i32::try_from(n).unwrap_or(i32::MAX));
    let changed = e.untrack(|| e.read(me, AnimatedImage::CURRENT_FRAME.index)) != value;
    set(e, me, AnimatedImage::CURRENT_FRAME.index, value);
    // Qt says so for the first frame of every picture, too.
    if changed || n == 0 {
        let _ = e.emit_by_name(me, "frameChanged", &[]);
    }
}

/// Arms the timer for the frame on screen, or stops it when the item is
/// not to move on.
fn schedule(e: &Engine, me: ObjectId) {
    let Ok(timer) = e.with(me, |a: &AnimatedImage| a.timer.get()) else {
        return;
    };
    let timer = timer.filter(|t| e.is_alive(*t));
    let (playing, paused, speed) = e.untrack(|| {
        (
            e.read(me, AnimatedImage::PLAYING.index).to_boolean(),
            e.read(me, AnimatedImage::PAUSED.index).to_boolean(),
            e.read(me, AnimatedImage::SPEED.index).to_number(),
        )
    });
    let delay = with_run(e, me, |r| {
        r.frames
            .as_ref()
            .filter(|f| f.images.len() > 1)
            .and_then(|f| f.delays.get(r.frame).copied())
    })
    .flatten();
    let due = match delay {
        Some(ms) if playing && !paused && speed > 0.0 && speed.is_finite() => Some(
            (f64::from(ms) / speed)
                .round()
                .clamp(1.0, f64::from(i32::MAX)) as i32,
        ),
        _ => None,
    };
    let Some(ms) = due else {
        if let Some(t) = timer {
            check(e, e.set(t, Timer::RUNNING, false));
        }
        return;
    };
    let timer = match timer {
        Some(t) => Ok(t),
        None => e
            .create_native_incomplete(Timer::default(), Some(me))
            .and_then(|t| {
                e.complete(t)?;
                e.connect_by_name(t, "triggered", move |e, _| advance(e, me))?;
                let _ = e.with(me, |a: &AnimatedImage| a.timer.set(Some(t)));
                Ok(t)
            }),
    };
    let armed = timer.and_then(|t| {
        e.set(t, Timer::INTERVAL, ms)?;
        e.invoke_by_name(t, "restart", &[]).map(|_| ())
    });
    check(e, armed);
}

fn advance(e: &Engine, me: ObjectId) {
    if !e.is_alive(me) {
        return;
    }
    let n = count(e, me);
    if n > 1 {
        show(e, me, (frame(e, me) + 1) % n);
    }
    schedule(e, me);
}

fn set(e: &Engine, me: ObjectId, p: ljos_engine::PropIndex, v: Value) {
    if let Err(err) = write_computed(e, me, p, v) {
        e.log(LogLevel::Warning, CATEGORY, format!("{err}"));
    }
}

fn check(e: &Engine, r: Result<(), EngineError>) {
    if let Err(err) = r {
        e.log(
            LogLevel::Warning,
            CATEGORY,
            format!("AnimatedImage timer: {err}"),
        );
    }
}
