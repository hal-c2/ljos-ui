//! A MouseArea's press and hold: a press held for `pressAndHoldInterval`
//! milliseconds says `pressAndHold`, wherever the pointer went meanwhile.
//! When something handles the signal, the press was held: its release
//! says `wasHeld` and is no click. When nothing does, it is a click as any
//! other.
//!
//! The wait is an engine `Timer`, made the first time it is needed and
//! armed only while a press is held: an idle area has no deadline.

use std::cell::Cell;
use std::rc::Rc;

use ljos_engine::{Engine, Interception, LogLevel, ObjectId, Timer, Value, WriteKind};

use crate::input::{MouseArea, MouseEvent};
use crate::scene::{SCENE_CATEGORY, emit_mouse_event};

/// What a MouseArea remembers of its press that QML does not see.
#[derive(Debug, Default)]
pub(crate) struct Hold {
    timer: Cell<Option<ObjectId>>,
    /// The press held now said `pressAndHold`, and something heard it.
    held: Cell<bool>,
}

fn state(engine: &Engine, m: ObjectId) -> Option<Rc<Hold>> {
    engine.with::<MouseArea, _>(m, |a| a.hold.clone()).ok()
}

/// The area is complete: from now on a negative interval reads as the
/// system's, as Qt keeps it, whether QML gave it or a script writes it.
pub(crate) fn completed(engine: &Engine, m: ObjectId) {
    let p = MouseArea::PRESS_AND_HOLD_INTERVAL.index;
    let given = engine.untrack(|| engine.read(m, p)).to_number();
    if given < 0.0 {
        system(engine, m, WriteKind::Initial);
    }
    engine.set_interceptor(
        m,
        p,
        Some(Rc::new(move |e: &Engine, v: &Value, how| {
            // `undefined` is NaN, and goes on to be a reset.
            if v.to_number() >= 0.0 || v.to_number().is_nan() {
                return false;
            }
            match how {
                Interception::Write => system(e, m, WriteKind::Imperative),
                Interception::Binding => {
                    e.after_binding(move |e| system(e, m, WriteKind::Animation));
                }
            }
            true
        })),
    );
}

fn system(engine: &Engine, m: ObjectId, how: WriteKind) {
    let ms = crate::system_prefs::press_and_hold_interval(engine);
    let _ = engine.write_internal(
        m,
        MouseArea::PRESS_AND_HOLD_INTERVAL.index,
        Value::Int(ms),
        how,
    );
}

/// The area took a press: the wait begins, as long as the interval is
/// now. Changing it later does not move this wait.
pub(crate) fn pressed(engine: &Engine, m: ObjectId) {
    let Some(st) = state(engine, m) else {
        return;
    };
    st.held.set(false);
    let given = engine
        .untrack(|| engine.read(m, MouseArea::PRESS_AND_HOLD_INTERVAL.index))
        .to_number();
    let ms = if given < 0.0 || given.is_nan() {
        crate::system_prefs::press_and_hold_interval(engine)
    } else {
        given as i32
    };
    arm(engine, m, &st, ms);
}

/// The press ended, let go or cancelled. Returns whether it was held.
pub(crate) fn ended(engine: &Engine, m: ObjectId) -> bool {
    let Some(st) = state(engine, m) else {
        return false;
    };
    stop(engine, &st);
    st.held.replace(false)
}

fn fire(engine: &Engine, m: ObjectId) {
    let Some(st) = state(engine, m) else {
        return;
    };
    let read = |p| engine.untrack(|| engine.read(m, p));
    if !read(MouseArea::PRESSED.index).to_boolean() || crate::mouse_drag::dragged(engine, m) {
        return;
    }
    let listens = engine
        .signal_index(m, "pressAndHold")
        .is_ok_and(|s| engine.signal_connected(m, s));
    if !listens {
        return;
    }
    st.held.set(true);
    let buttons = read(MouseArea::PRESSED_BUTTONS.index).to_number() as i32;
    let event = MouseEvent {
        x: read(MouseArea::MOUSE_X.index).to_number(),
        y: read(MouseArea::MOUSE_Y.index).to_number(),
        // The button that is down; of several, the lowest.
        button: buttons.isolate_lowest_one(),
        buttons,
        was_held: true,
        ..MouseEvent::default()
    };
    emit_mouse_event(engine, m, "pressAndHold", event);
}

fn arm(engine: &Engine, m: ObjectId, st: &Hold, ms: i32) {
    let timer = match st.timer.get().filter(|t| engine.is_alive(*t)) {
        Some(t) => Ok(t),
        None => engine
            .create_native_incomplete(Timer::default(), Some(m))
            .and_then(|t| {
                engine.complete(t)?;
                engine.connect_by_name(t, "triggered", move |e, _| {
                    if e.is_alive(m) {
                        fire(e, m);
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
            format!("MouseArea: timer: {err}"),
        );
    }
}

fn stop(engine: &Engine, st: &Hold) {
    if let Some(t) = st.timer.get().filter(|t| engine.is_alive(*t)) {
        let _ = engine.set(t, Timer::RUNNING, false);
    }
}
