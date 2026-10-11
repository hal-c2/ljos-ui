//! `Transition`: animations that something else starts, on an object it
//! names.
//!
//! The type holds its animations; [`run_transition`] runs them all at
//! once, the way a `Behavior` runs its one: as driven runs, which show
//! `running` and emit nothing. An animation that names no target moves
//! the object the transition is run on, and only the properties it names.
//!
//! A popup runs its `enter` and `exit` transitions this way. States (M2)
//! will choose a transition by `from` and `to`, which are only stored
//! until then.
//!
//! Documentation: doc.qt.io/qt-6/qml-qtquick-transition.html.

use std::cell::Cell;
use std::rc::Rc;

use ljos_engine::{Atom, Engine, ObjectId, QmlList, QmlObject, QtObject, Value};
use ljos_reactive::WriteKind;

use super::driver;
use super::timeline::{Defaults, NO_PROPERTY};

/// `Transition` (QtQuick).
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick",
    name = "Transition",
    extends = QtObject,
    default = "animations"
)]
pub struct Transition {
    #[qml(base)]
    pub object: QtObject,
    #[qml(list)]
    pub animations: QmlList,
    /// A transition that is not enabled is over as soon as it is run.
    #[qml(notify)]
    pub enabled: bool,
    /// Stored: the state the transition leaves.
    #[qml(notify)]
    pub from: Atom,
    /// Stored: the state the transition goes to.
    #[qml(notify)]
    pub to: Atom,
    /// Stored.
    #[qml(notify)]
    pub reversible: bool,
    #[qml(notify, readonly)]
    pub running: bool,
    /// Counts the runs, so that the end of an animation of a run that was
    /// stopped or replaced is not taken for the end of the present one.
    #[qml(skip)]
    run: Rc<Cell<u64>>,
}

impl Default for Transition {
    fn default() -> Self {
        Self {
            object: QtObject::default(),
            animations: QmlList::new(),
            enabled: true,
            from: Atom::from("*"),
            to: Atom::from("*"),
            reversible: false,
            running: false,
            run: Rc::default(),
        }
    }
}

fn show_running(engine: &Engine, transition: ObjectId, on: bool) {
    if engine.is_alive(transition) {
        let _ = engine.write_internal(
            transition,
            Transition::RUNNING.index,
            Value::Bool(on),
            WriteKind::Animation,
        );
    }
}

fn animations(engine: &Engine, transition: ObjectId) -> Vec<ObjectId> {
    match engine.untrack(|| engine.read(transition, Transition::ANIMATIONS.index)) {
        Value::List(l) => l
            .iter()
            .filter_map(Value::as_object)
            .filter(|a| engine.is_alive(*a))
            .collect(),
        _ => Vec::new(),
    }
}

/// Runs the animations of `transition` on `target`, all at once, and
/// calls `done` when the last of them has ended: before this returns if
/// none takes time (no animations, a transition that is not enabled,
/// reduced motion). A run of it that is under way is replaced, and its
/// `done` is never called. Nothing if `transition` is no `Transition`.
pub fn run_transition(
    engine: &Engine,
    transition: ObjectId,
    target: ObjectId,
    done: impl Fn(&Engine) + 'static,
) {
    let Ok(run) = engine.with::<Transition, _>(transition, |t| t.run.clone()) else {
        return;
    };
    stop_transition(engine, transition);
    let id = run.get();
    let enabled = engine
        .untrack(|| engine.read(transition, Transition::ENABLED.index))
        .to_boolean();
    let anims = if enabled {
        animations(engine, transition)
    } else {
        Vec::new()
    };
    if anims.is_empty() {
        done(engine);
        return;
    }
    show_running(engine, transition, true);
    let left = Rc::new(Cell::new(anims.len()));
    let done: Rc<dyn Fn(&Engine)> = Rc::new(done);
    let defaults = Defaults {
        obj: target,
        prop: NO_PROPERTY,
        to: None,
    };
    for anim in anims {
        let (now, left, done) = (run.clone(), left.clone(), done.clone());
        let ended: driver::Done = Rc::new(move |e| {
            if now.get() != id {
                return;
            }
            left.set(left.get().saturating_sub(1));
            if left.get() == 0 {
                show_running(e, transition, false);
                done(e);
            }
        });
        driver::start_driven(engine, anim, &defaults, Some(ended));
        if run.get() != id {
            // Something an animation did stopped the transition.
            return;
        }
    }
}

/// Ends a run of `transition` where it is: the properties stay as they
/// are, and what was to be called at the end of the run is not called.
pub fn stop_transition(engine: &Engine, transition: ObjectId) {
    let Ok(run) = engine.with::<Transition, _>(transition, |t| t.run.clone()) else {
        return;
    };
    run.set(run.get() + 1);
    for anim in animations(engine, transition) {
        driver::stop_driven(engine, anim);
    }
    show_running(engine, transition, false);
}
