//! `Transition`: animations that something else starts, on an object it
//! names.
//!
//! The type holds its animations; [`run_transition`] runs them all at
//! once, the way a `Behavior` runs its one: as driven runs, which show
//! `running` and emit nothing. An animation that names no target moves
//! the object the transition is run on, and only the properties it names;
//! [`run_transition_to`] gives such an animation a property and a value
//! to move it to instead, as a `Behavior` does.
//!
//! A popup runs its `enter` and `exit` transitions this way, and a view
//! the ones for its delegates.
//!
//! Between two states it is different ([`crate::states`]): one of the
//! transitions of the item is chosen by `from` and `to` ([`choose`]), and
//! its animations move what the change of state changed, as one timeline
//! under the transition.
//!
//! Documentation: doc.qt.io/qt-6/qml-qtquick-transition.html.

use std::cell::Cell;
use std::rc::Rc;

use ljos_engine::{Atom, Engine, ObjectId, PropIndex, QmlList, QmlObject, QtObject, Value};
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
    /// The states it leads from: names with commas between, `*` for any.
    #[qml(notify)]
    pub from: Atom,
    /// The states it leads to, written as `from` is.
    #[qml(notify)]
    pub to: Atom,
    /// It is also for the way back, run in the opposite order.
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

pub(crate) fn show_running(engine: &Engine, transition: ObjectId, on: bool) {
    if engine.is_alive(transition) {
        let _ = engine.write_internal(
            transition,
            Transition::RUNNING.index,
            Value::Bool(on),
            WriteKind::Host,
        );
    }
}

pub(crate) fn animations(engine: &Engine, transition: ObjectId) -> Vec<ObjectId> {
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
    let defaults = Defaults {
        obj: target,
        prop: NO_PROPERTY,
        to: None,
    };
    run(engine, transition, &defaults, Rc::new(done));
}

/// Runs `transition` as [`run_transition`] does, except that an animation
/// that names no property moves `prop` of `target` to `to`, as one in a
/// `Behavior` does; with nothing to run, `prop` is `to` at once. A control
/// whose own value a transition animates (a swipe's position) runs it this
/// way.
pub fn run_transition_to(
    engine: &Engine,
    transition: ObjectId,
    target: ObjectId,
    prop: PropIndex,
    to: Value,
    done: impl Fn(&Engine) + 'static,
) {
    let defaults = Defaults {
        obj: target,
        prop,
        to: Some(to),
    };
    run(engine, transition, &defaults, Rc::new(done));
}

fn run(engine: &Engine, transition: ObjectId, defaults: &Defaults, done: Rc<dyn Fn(&Engine)>) {
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
        // What a `Behavior` without an animation does: the value is there
        // at once.
        if let Some(to) = &defaults.to
            && defaults.prop != NO_PROPERTY
            && engine.is_alive(defaults.obj)
        {
            let _ = engine.write_internal(
                defaults.obj,
                defaults.prop,
                to.clone(),
                WriteKind::Animation,
            );
        }
        done(engine);
        return;
    }
    show_running(engine, transition, true);
    let left = Rc::new(Cell::new(anims.len()));
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
        driver::start_driven(engine, anim, defaults, Some(ended));
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

/// How well a `from` or `to` fits a state: 0 for `*`, 1 if it names the
/// state (the base state's name is the empty string), `None` if neither.
fn fit(pattern: &str, state: &str) -> Option<u8> {
    if pattern == "*" {
        Some(0)
    } else if pattern.split(',').any(|name| name.trim() == state) {
        Some(1)
    } else {
        None
    }
}

/// The transition for a change of state from `from` to `to`, and whether
/// it runs reversed: the enabled one that names the most of the two
/// states, the first of those in the list; as itself before reversed.
pub(crate) fn choose(
    engine: &Engine,
    transitions: &[ObjectId],
    from: &str,
    to: &str,
) -> Option<(ObjectId, bool)> {
    let mut best: Option<(u8, bool, ObjectId)> = None;
    for t in transitions {
        let read = |p: ljos_engine::PropIndex| engine.untrack(|| engine.read(*t, p));
        if !engine.is_alive(*t) || !read(Transition::ENABLED.index).to_boolean() {
            continue;
        }
        let pf = read(Transition::FROM.index).to_js_string();
        let pt = read(Transition::TO.index).to_js_string();
        let forward = fit(&pf, from).zip(fit(&pt, to)).map(|(a, b)| a + b);
        let back = if read(Transition::REVERSIBLE.index).to_boolean() {
            fit(&pf, to).zip(fit(&pt, from)).map(|(a, b)| a + b)
        } else {
            None
        };
        for (score, reversed) in [(forward, false), (back, true)] {
            let Some(score) = score else {
                continue;
            };
            let better = match best {
                None => true,
                Some((s, r, _)) => score > s || (score == s && r && !reversed),
            };
            if better {
                best = Some((score, reversed, *t));
            }
        }
    }
    best.map(|(_, reversed, t)| (t, reversed))
}

#[cfg(test)]
mod tests {
    use super::fit;

    #[test]
    fn a_star_fits_every_state_and_a_name_fits_better() {
        assert_eq!(fit("*", "a"), Some(0));
        assert_eq!(fit("*", ""), Some(0));
        assert_eq!(fit("a", "a"), Some(1));
        assert_eq!(fit("a", "b"), None);
    }

    #[test]
    fn a_list_fits_each_state_it_names() {
        assert_eq!(fit("b,c", "c"), Some(1));
        assert_eq!(fit("b, c", "c"), Some(1));
        assert_eq!(fit("b,c", "d"), None);
    }

    #[test]
    fn the_empty_name_is_the_base_state() {
        assert_eq!(fit("", ""), Some(1));
        assert_eq!(fit("", "a"), None);
        assert_eq!(fit("a", ""), None);
    }
}
