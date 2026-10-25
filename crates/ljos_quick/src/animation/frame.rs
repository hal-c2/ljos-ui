//! `FrameAnimation`: a script called once a frame.
//!
//! It has no timeline. While it runs and is not paused the driver keeps
//! it in its list of frame animations ([`driver::add_frames`]) and asks
//! for every frame; each frame it is told the frame's time
//! ([`advance`]). Stopped or paused it is out of the list and asks for
//! nothing.

use std::rc::Rc;

use ljos_engine::{
    Cx, Engine, EngineError, Interception, ObjectId, QmlObject, QtObject, Value, qml_impl,
};
use ljos_reactive::WriteKind;
use ljos_runtime::Time;

use super::driver;

/// How much of the last `smoothFrameTime` the next one keeps; qml6's
/// average of steady 16 ms frames comes out at this weight.
const SMOOTHING: f64 = 0.9;

/// `FrameAnimation` (QtQuick).
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick", name = "FrameAnimation", extends = QtObject)]
pub struct FrameAnimation {
    #[qml(base)]
    pub object: QtObject,
    #[qml(notify)]
    pub running: bool,
    #[qml(notify)]
    pub paused: bool,
    #[qml(notify, readonly)]
    pub current_frame: i32,
    /// Seconds since the frame before.
    #[qml(notify, readonly)]
    pub frame_time: f64,
    #[qml(notify, readonly)]
    pub smooth_frame_time: f64,
    /// Seconds it has run since it was started, pauses included.
    #[qml(notify, readonly)]
    pub elapsed_time: f64,
    /// The frame time it was last brought to.
    #[qml(skip)]
    last: Option<Time>,
    /// Started and not stopped since: what `running` says once a write
    /// or a binding's value has been carried out.
    #[qml(skip)]
    going: bool,
}

#[qml_impl]
impl FrameAnimation {
    #[qml(signal)]
    fn triggered();

    #[qml(invokable)]
    fn start(cx: &Cx) -> Result<(), EngineError> {
        start(cx.engine, cx.this);
        Ok(())
    }

    #[qml(invokable)]
    fn stop(cx: &Cx) -> Result<(), EngineError> {
        stop(cx.engine, cx.this);
        Ok(())
    }

    #[qml(invokable)]
    fn pause(cx: &Cx) -> Result<(), EngineError> {
        set_paused(cx.engine, cx.this, true);
        Ok(())
    }

    #[qml(invokable)]
    fn resume(cx: &Cx) -> Result<(), EngineError> {
        set_paused(cx.engine, cx.this, false);
        Ok(())
    }

    #[qml(invokable)]
    fn restart(cx: &Cx) -> Result<(), EngineError> {
        stop(cx.engine, cx.this);
        start(cx.engine, cx.this);
        Ok(())
    }

    /// Back to frame 0 and no time run; what the last frame took stays.
    #[qml(invokable)]
    fn reset(cx: &Cx) -> Result<(), EngineError> {
        show(
            cx.engine,
            cx.this,
            FrameAnimation::CURRENT_FRAME.index,
            Value::Int(0),
        );
        show(
            cx.engine,
            cx.this,
            FrameAnimation::ELAPSED_TIME.index,
            Value::Real(0.0),
        );
        Ok(())
    }

    /// From here on `running` and `paused` do what the methods do,
    /// whoever sets them.
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let me = cx.this;
        let engine = cx.engine;
        engine.set_interceptor(
            me,
            FrameAnimation::RUNNING.index,
            Some(Rc::new(move |e, v, how| match how {
                Interception::Write => {
                    if v.to_boolean() {
                        start(e, me);
                    } else {
                        stop(e, me);
                    }
                    true
                }
                Interception::Binding => false,
            })),
        );
        engine.set_interceptor(
            me,
            FrameAnimation::PAUSED.index,
            Some(Rc::new(move |e, v, how| match how {
                Interception::Write => {
                    set_paused(e, me, v.to_boolean());
                    true
                }
                Interception::Binding => false,
            })),
        );
        // A binding's new value; what this file writes is the state
        // already, and changes nothing here.
        engine.watch(me, FrameAnimation::RUNNING.index, move |e, v| {
            if v.to_boolean() != is_going(e, me) {
                if v.to_boolean() {
                    start(e, me);
                } else {
                    stop(e, me);
                }
            }
        })?;
        engine.watch(me, FrameAnimation::PAUSED.index, move |e, v| {
            set_paused(e, me, v.to_boolean());
        })?;
        if cx.get(FrameAnimation::RUNNING)? {
            cx.with_mut(|f: &mut FrameAnimation| f.going = true)?;
            begin(engine, me);
        }
        Ok(())
    }
}

/// Shows a state; a binding on the property stays in place.
fn show(engine: &Engine, me: ObjectId, prop: ljos_engine::PropIndex, v: Value) {
    let _ = engine.write_internal(me, prop, v, WriteKind::Host);
}

fn read(engine: &Engine, me: ObjectId, prop: ljos_engine::PropIndex) -> Value {
    engine.untrack(|| engine.read(me, prop))
}

fn is_paused(engine: &Engine, me: ObjectId) -> bool {
    read(engine, me, FrameAnimation::PAUSED.index).to_boolean()
}

fn is_going(engine: &Engine, me: ObjectId) -> bool {
    engine
        .with::<FrameAnimation, _>(me, |f| f.going)
        .unwrap_or(false)
}

fn set_going(engine: &Engine, me: ObjectId, going: bool) {
    let _ = engine.with_mut::<FrameAnimation, _>(me, |f| f.going = going);
}

/// Its time begins again; the frame count goes on, as on qml6.
fn begin(engine: &Engine, me: ObjectId) {
    show(
        engine,
        me,
        FrameAnimation::ELAPSED_TIME.index,
        Value::Real(0.0),
    );
    if !is_paused(engine, me) {
        follow(engine, me);
    }
}

/// Puts it in the driver's list from the frame clock's time now.
fn follow(engine: &Engine, me: ObjectId) {
    let now = driver::frame_now(engine);
    let _ = engine.with_mut::<FrameAnimation, _>(me, |f| f.last = now);
    driver::add_frames(engine, me);
}

fn start(engine: &Engine, me: ObjectId) {
    if is_going(engine, me) {
        return;
    }
    set_going(engine, me, true);
    show(engine, me, FrameAnimation::RUNNING.index, Value::Bool(true));
    begin(engine, me);
}

/// The frame and the time it got to stay; `paused` goes back to false.
fn stop(engine: &Engine, me: ObjectId) {
    set_going(engine, me, false);
    driver::remove_frames(engine, me);
    show(engine, me, FrameAnimation::PAUSED.index, Value::Bool(false));
    show(
        engine,
        me,
        FrameAnimation::RUNNING.index,
        Value::Bool(false),
    );
}

/// Paused while stopped, it only says so.
fn set_paused(engine: &Engine, me: ObjectId, paused: bool) {
    show(
        engine,
        me,
        FrameAnimation::PAUSED.index,
        Value::Bool(paused),
    );
    if paused {
        driver::remove_frames(engine, me);
    } else if is_going(engine, me) && !driver::has_frames(engine, me) {
        follow(engine, me);
    }
}

/// One frame, at `now`: the counts go on and `triggered` is emitted.
pub(crate) fn advance(engine: &Engine, me: ObjectId, now: Time) {
    let Ok(last) = engine.with_mut::<FrameAnimation, _>(me, |f| f.last.replace(now)) else {
        return;
    };
    let dt = last.map_or(0.0, |last| now.saturating_since(last).as_secs_f64());
    let frame = read(engine, me, FrameAnimation::CURRENT_FRAME.index).to_number() as i32;
    let elapsed = read(engine, me, FrameAnimation::ELAPSED_TIME.index).to_number();
    let smooth = read(engine, me, FrameAnimation::SMOOTH_FRAME_TIME.index).to_number();
    show(
        engine,
        me,
        FrameAnimation::CURRENT_FRAME.index,
        Value::Int(frame.saturating_add(1)),
    );
    show(
        engine,
        me,
        FrameAnimation::FRAME_TIME.index,
        Value::Real(dt),
    );
    show(
        engine,
        me,
        FrameAnimation::SMOOTH_FRAME_TIME.index,
        Value::Real(SMOOTHING * smooth + (1.0 - SMOOTHING) * dt),
    );
    show(
        engine,
        me,
        FrameAnimation::ELAPSED_TIME.index,
        Value::Real(elapsed + dt),
    );
    let _ = engine.emit_by_name(me, "triggered", &[]);
}
