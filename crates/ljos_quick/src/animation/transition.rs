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
use super::motion;
use super::timeline::{Defaults, NO_PROPERTY, Timed, passage_of};

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
    /// The object reduced motion fades in place of a passage that would
    /// move it, and the opacity it had: given back when the run is
    /// stopped or the way out is over.
    #[qml(skip)]
    faded: Rc<Cell<Option<(ObjectId, f64)>>>,
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
            faded: Rc::default(),
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
/// none takes time (no animations, a transition that is not enabled, or
/// with reduced motion one that only moves). A run of it that is under way is replaced, and its
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
        aimed: None,
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
        aimed: None,
    };
    run(engine, transition, &defaults, Rc::new(done));
}

/// Which way an object goes when a transition is run on it as a passage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Passage {
    /// It comes into view: a popup opens, a page is pushed.
    Enter,
    /// It goes out of view.
    Exit,
}

/// Runs `transition` on `target` as [`run_transition`] does, for an object
/// coming into view or going out of it. With reduced motion a passage
/// that would move `target` (grow it, slide it) and does not fade it
/// fades it instead, where it ends ([`Passage::Enter`]: from transparent
/// to the opacity it has) or where it is ([`Passage::Exit`]: to
/// transparent, and then the movement, which is at its end at once, and
/// the opacity it had back). `done` is called when the fade is over too,
/// so that what the end of the passage tells comes when it is seen to end.
pub fn run_passage(
    engine: &Engine,
    transition: ObjectId,
    target: ObjectId,
    passage: Passage,
    done: impl Fn(&Engine) + 'static,
) {
    let Ok((run, faded)) =
        engine.with::<Transition, _>(transition, |t| (t.run.clone(), t.faded.clone()))
    else {
        return;
    };
    let enabled = engine
        .untrack(|| engine.read(transition, Transition::ENABLED.index))
        .to_boolean();
    let calm = enabled && crate::system_prefs::reduced_motion_now(engine);
    let (moves, fades) = if calm {
        passage_of(engine, &animations(engine, transition), target)
    } else {
        (false, false)
    };
    let opacity = engine
        .property_index(target, "opacity")
        .ok()
        .map(|p| engine.untrack(|| engine.read(target, p)).to_number());
    let (Some(opacity), true, false) = (opacity, moves, fades) else {
        run_transition(engine, transition, target, done);
        return;
    };
    let (from, to) = match passage {
        Passage::Enter => (0.0, opacity),
        Passage::Exit => (opacity, 0.0),
    };
    let micros = i64::from(motion::REDUCED_FADE_MAX_MS) * 1000;
    let Some(fade) = Timed::fade(engine, target, from, to, micros) else {
        run_transition(engine, transition, target, done);
        return;
    };
    let done: Rc<dyn Fn(&Engine)> = Rc::new(done);
    match passage {
        Passage::Enter => {
            // The movement is at its end at once; the passage is over when
            // the fade is too.
            let left = Rc::new(Cell::new(2u8));
            let part = {
                let (left, done) = (left.clone(), done.clone());
                Rc::new(move |e: &Engine| {
                    left.set(left.get().saturating_sub(1));
                    if left.get() == 0 {
                        show_running(e, transition, false);
                        done(e);
                    }
                })
            };
            run_transition(engine, transition, target, {
                let part = part.clone();
                move |e| part(e)
            });
            let id = run.get();
            faded.set(Some((target, opacity)));
            show_running(engine, transition, true);
            let ended: driver::Done = Rc::new(move |e| {
                if run.get() == id {
                    faded.set(None);
                    part(e);
                }
            });
            driver::start_timeline(engine, transition, fade, Some(ended));
        }
        Passage::Exit => {
            stop_transition(engine, transition);
            let id = run.get();
            faded.set(Some((target, opacity)));
            show_running(engine, transition, true);
            let ended: driver::Done = Rc::new(move |e| {
                if run.get() != id {
                    return;
                }
                // Starting the transition stops what ran before it, which
                // would give the opacity back while it is still to be
                // seen.
                let saved = faded.take();
                let over = Rc::new(Cell::new(false));
                run_transition(e, transition, target, {
                    let (faded, over, done) = (faded.clone(), over.clone(), done.clone());
                    move |e| {
                        over.set(true);
                        restore(e, faded.take().or(saved));
                        done(e);
                    }
                });
                if !over.get() {
                    faded.set(saved);
                }
            });
            driver::start_timeline(engine, transition, fade, Some(ended));
        }
    }
}

/// Gives a faded object back the opacity it had.
fn restore(engine: &Engine, faded: Option<(ObjectId, f64)>) {
    if let Some((obj, opacity)) = faded
        && engine.is_alive(obj)
        && let Ok(p) = engine.property_index(obj, "opacity")
    {
        let _ = engine.write_internal(obj, p, Value::Real(opacity), WriteKind::Animation);
    }
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
/// What reduced motion faded in place of a passage has its opacity back.
pub fn stop_transition(engine: &Engine, transition: ObjectId) {
    let Ok((run, faded)) =
        engine.with::<Transition, _>(transition, |t| (t.run.clone(), t.faded.clone()))
    else {
        return;
    };
    run.set(run.get() + 1);
    if let Some(was) = faded.take() {
        driver::forget_driven(engine, transition);
        restore(engine, Some(was));
    }
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
