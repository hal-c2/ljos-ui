//! `DelayButton` and `RoundButton` (QtQuick.Templates).
//!
//! A delay button's `progress` runs from 0 to 1 over `delay` while it is
//! held, and back while it is not; `activated` comes when it is full, and
//! the click that ends that press checks the button. A click on a checked
//! one unchecks it at once. None of it says `toggled`. All of this was
//! learned by running `qml6` (Qt 6.11.2) and is pinned by
//! `features/controls/delay-button.feature`; documentation:
//! doc.qt.io/qt-6/qml-qtquick-controls-delaybutton.html.
//!
//! Qt lets the style give the progress a `transition`; Ljós moves it
//! natively, at the pace the Basic style asks for (the whole delay up, a
//! third of it down), with an engine `NumberAnimation` the button owns, so
//! it runs on the engine's clock and nothing polls.

use std::cell::Cell;
use std::rc::Rc;

use ljos_engine::{Atom, Cx, Engine, EngineError, ObjectId, QmlObject, Value, WriteKind, qml_impl};
use ljos_quick::{Animation, Item, NumberAnimation, PropertyAnimation};

use crate::button::{AbstractButton, Button, set_checked};
use crate::util::{bind_default, check, flag, num};

/// The share of `delay` the progress takes to go back from full.
const BACK: f64 = 0.3;

/// What a delay button remembers between a press and its release. Not
/// QML-visible.
#[derive(Debug, Default)]
pub(crate) struct Delay {
    /// The press may still activate the button: it began on one that was
    /// not checked, and the progress has not been full yet.
    armed: Cell<bool>,
    /// The press filled the progress: its click checks the button.
    activated: Cell<bool>,
    /// The animation that moves `progress`.
    anim: Cell<Option<ObjectId>>,
}

/// `DelayButton`.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick.Templates", name = "DelayButton", extends = AbstractButton)]
pub struct DelayButton {
    #[qml(base)]
    pub button: AbstractButton,
    /// Milliseconds a press has to last.
    #[qml(notify)]
    pub delay: i32,
    #[qml(notify)]
    pub progress: f64,
    #[qml(skip)]
    pub(crate) state: Rc<Delay>,
}

impl Default for DelayButton {
    fn default() -> Self {
        Self {
            button: AbstractButton {
                checkable: true,
                ..AbstractButton::default()
            },
            delay: 300,
            progress: 0.0,
            state: Rc::default(),
        }
    }
}

#[qml_impl]
impl DelayButton {
    #[qml(signal)]
    fn activated();

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        init_delay(cx.engine, cx.this)
    }
}

fn state_of(engine: &Engine, obj: ObjectId) -> Option<Rc<Delay>> {
    engine.with::<DelayButton, _>(obj, |b| b.state.clone()).ok()
}

fn init_delay(engine: &Engine, obj: ObjectId) -> Result<(), EngineError> {
    let checked_p = AbstractButton::CHECKED.index;
    if engine.untrack(|| flag(engine, obj, checked_p)) {
        put_progress(engine, obj, 1.0);
    }
    engine.watch(obj, checked_p, move |e, v| {
        stop(e, obj);
        put_progress(e, obj, if v.to_boolean() { 1.0 } else { 0.0 });
    })?;
    engine.watch(obj, AbstractButton::PRESSED.index, move |e, v| {
        pressed_changed(e, obj, v.to_boolean());
    })?;
    engine.watch(obj, DelayButton::PROGRESS.index, move |e, v| {
        let Some(st) = state_of(e, obj) else {
            return;
        };
        if v.to_number() >= 1.0 && st.armed.replace(false) {
            st.activated.set(true);
            check(e, "activated", e.emit_by_name(obj, "activated", &[]));
        }
    })?;
    // A press that ends without a click (the pointer left, the button
    // went away) lets the progress fall back as a short press does.
    engine.connect_by_name(obj, "canceled", move |e, _| {
        if let Some(st) = state_of(e, obj) {
            st.armed.set(false);
            st.activated.set(false);
        }
        if !e.untrack(|| flag(e, obj, AbstractButton::CHECKED.index)) {
            run_to(e, obj, 0.0);
        }
    })?;
    Ok(())
}

fn pressed_changed(engine: &Engine, obj: ObjectId, on: bool) {
    let Some(st) = state_of(engine, obj) else {
        return;
    };
    let checked = engine.untrack(|| flag(engine, obj, AbstractButton::CHECKED.index));
    if on {
        // A checked button is full and stays so until the click.
        st.activated.set(false);
        st.armed.set(!checked);
        if !checked {
            run_to(engine, obj, 1.0);
        }
    } else if !checked && !st.activated.get() {
        st.armed.set(false);
        run_to(engine, obj, 0.0);
    }
}

/// What the click that ends a press does to a delay button, in place of
/// the toggle of other buttons: a full press checks it, any click
/// unchecks a checked one.
pub(crate) fn click(engine: &Engine, obj: ObjectId) {
    let Some(st) = state_of(engine, obj) else {
        return;
    };
    let activated = st.activated.replace(false);
    st.armed.set(false);
    let checked = engine.untrack(|| flag(engine, obj, AbstractButton::CHECKED.index));
    if checked {
        check(engine, "checked", set_checked(engine, obj, false));
    } else if activated {
        check(engine, "checked", set_checked(engine, obj, true));
    }
}

fn put_progress(engine: &Engine, obj: ObjectId, p: f64) {
    let r = engine.write(
        obj,
        DelayButton::PROGRESS.index,
        Value::Real(p),
        WriteKind::Imperative,
    );
    check(engine, "progress", r);
}

fn stop(engine: &Engine, obj: ObjectId) {
    let anim = state_of(engine, obj).and_then(|st| st.anim.get());
    if let Some(a) = anim.filter(|a| engine.is_alive(*a)) {
        check(engine, "progress", engine.set(a, Animation::RUNNING, false));
    }
}

/// Moves `progress` to `to`, from where it is, at the pace the Basic style
/// gives: the whole delay for the whole way up, `BACK` of it down.
fn run_to(engine: &Engine, obj: ObjectId, to: f64) {
    let Some(st) = state_of(engine, obj) else {
        return;
    };
    stop(engine, obj);
    let (now, delay) = engine.untrack(|| {
        (
            num(engine, obj, DelayButton::PROGRESS.index),
            num(engine, obj, DelayButton::DELAY.index),
        )
    });
    let way = (to - now).abs();
    if way <= 0.0 {
        return;
    }
    let ms = if to > now {
        delay * way
    } else {
        BACK * delay * way
    };
    if ms < 1.0 {
        put_progress(engine, obj, to);
        return;
    }
    let anim = match st.anim.get().filter(|a| engine.is_alive(*a)) {
        Some(a) => a,
        None => {
            let made = engine
                .create_native(NumberAnimation::default(), Some(obj))
                .and_then(|a| {
                    engine.complete(a)?;
                    engine.set(a, PropertyAnimation::TARGET, Some(obj))?;
                    engine.set(a, PropertyAnimation::PROPERTY, Atom::from("progress"))?;
                    Ok(a)
                });
            match made {
                Ok(a) => {
                    st.anim.set(Some(a));
                    a
                }
                Err(err) => {
                    check(engine, "progress", Err(err));
                    return;
                }
            }
        }
    };
    let r = engine
        .set(anim, PropertyAnimation::FROM, Value::Real(now))
        .and_then(|()| engine.set(anim, PropertyAnimation::TO, Value::Real(to)))
        .and_then(|()| engine.set(anim, PropertyAnimation::DURATION, ms.round() as i32))
        .and_then(|()| engine.set(anim, Animation::RUNNING, true));
    check(engine, "progress", r);
}

// ---------------------------------------------------------- RoundButton

/// `RoundButton`: a button whose `radius` is half its shorter side until
/// it is given one.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick.Templates", name = "RoundButton", extends = Button)]
pub struct RoundButton {
    #[qml(base)]
    pub button: Button,
    #[qml(notify, reset = HALF_SIDE)]
    pub radius: f64,
}

/// `radius = undefined` gives the button back its round corners.
const HALF_SIDE: ljos_engine::Reset = ljos_engine::Reset::Follow(half_side);

fn half_side(e: &Engine, obj: ObjectId, _: ljos_engine::PropIndex) -> Value {
    Value::Real(num(e, obj, Item::WIDTH.index).min(num(e, obj, Item::HEIGHT.index)) / 2.0)
}

#[qml_impl]
impl RoundButton {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let obj = cx.this;
        bind_default(cx.engine, obj, RoundButton::RADIUS.index, move |e| {
            half_side(e, obj, RoundButton::RADIUS.index)
        })
    }
}
