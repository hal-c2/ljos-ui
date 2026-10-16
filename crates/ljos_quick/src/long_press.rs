//! A TapHandler's long press: a press held for `longPressThreshold`
//! without moving beyond the drag threshold says `longPressed`, and is no
//! tap when it is let go.
//!
//! The wait is an engine `Timer`, made the first time it is needed and
//! armed only while a press is held: an idle handler has no deadline.

use std::cell::Cell;
use std::rc::Rc;

use ljos_engine::{Engine, LogLevel, ObjectId, Timer, Value};

use crate::input::TapHandler;
use crate::scene::{SCENE_CATEGORY, emit, set};

/// What a TapHandler remembers of its press that QML does not see.
#[derive(Debug, Default)]
pub(crate) struct LongPress {
    timer: Cell<Option<ObjectId>>,
    /// The press held now has been a long press.
    done: Cell<bool>,
}

fn state(engine: &Engine, h: ObjectId) -> Option<Rc<LongPress>> {
    engine
        .with::<TapHandler, _>(h, |t| t.long_press.clone())
        .ok()
}

/// The threshold in seconds. Unset or negative it is the system's
/// press-and-hold interval, as Qt's is.
fn threshold(engine: &Engine, h: ObjectId) -> f64 {
    let p = TapHandler::LONG_PRESS_THRESHOLD.index;
    let given = engine.untrack(|| engine.read(h, p)).to_number();
    if crate::item::is_unset(engine, h, p) || given < 0.0 || given.is_nan() {
        crate::system_prefs::press_and_hold_seconds(engine)
    } else {
        given
    }
}

/// The handler was pressed: the wait for a long press begins.
pub(crate) fn pressed(engine: &Engine, h: ObjectId) {
    let Some(st) = state(engine, h) else {
        return;
    };
    st.done.set(false);
    set(engine, h, TapHandler::TIME_HELD.index, Value::Real(0.0));
    let seconds = threshold(engine, h);
    if seconds > 0.0 {
        arm(engine, h, &st, (seconds * 1000.0).round() as i32);
    } else {
        stop(engine, &st);
    }
}

/// The press moved beyond the drag threshold: it is no long press.
pub(crate) fn moved_away(engine: &Engine, h: ObjectId) {
    if let Some(st) = state(engine, h) {
        stop(engine, &st);
    }
}

/// The press ended, let go or cancelled. Returns whether it had been a
/// long press.
pub(crate) fn ended(engine: &Engine, h: ObjectId) -> bool {
    let Some(st) = state(engine, h) else {
        return false;
    };
    stop(engine, &st);
    set(engine, h, TapHandler::TIME_HELD.index, Value::Real(-1.0));
    st.done.replace(false)
}

fn held(engine: &Engine, h: ObjectId) {
    let Some(st) = state(engine, h) else {
        return;
    };
    if !engine
        .untrack(|| engine.read(h, TapHandler::PRESSED.index))
        .to_boolean()
    {
        return;
    }
    st.done.set(true);
    let seconds = threshold(engine, h);
    set(engine, h, TapHandler::TIME_HELD.index, Value::Real(seconds));
    emit(engine, h, "longPressed", &[]);
}

fn arm(engine: &Engine, h: ObjectId, st: &LongPress, ms: i32) {
    let timer = match st.timer.get().filter(|t| engine.is_alive(*t)) {
        Some(t) => Ok(t),
        None => engine
            .create_native_incomplete(Timer::default(), Some(h))
            .and_then(|t| {
                engine.complete(t)?;
                engine.connect_by_name(t, "triggered", move |e, _| {
                    if e.is_alive(h) {
                        held(e, h);
                    }
                })?;
                st.timer.set(Some(t));
                Ok(t)
            }),
    };
    let r = timer.and_then(|t| {
        engine.set(t, Timer::INTERVAL, ms.max(1))?;
        engine.invoke_by_name(t, "restart", &[]).map(|_| ())
    });
    if let Err(err) = r {
        engine.log(
            LogLevel::Warning,
            SCENE_CATEGORY,
            format!("TapHandler: timer: {err}"),
        );
    }
}

fn stop(engine: &Engine, st: &LongPress) {
    if let Some(t) = st.timer.get().filter(|t| engine.is_alive(*t)) {
        let _ = engine.set(t, Timer::RUNNING, false);
    }
}
