//! `Behavior`: the animation a property runs whenever its value changes.
//!
//! A `Behavior on x` stands in front of `x` (an engine interceptor). A
//! value on its way in, from a JS write or from the property's binding, is
//! held back, and the Behavior's animation takes the property there from
//! the value it shows now. A new value while that is under way starts the
//! animation again from where the property is: Qt's retargeting rule.
//!
//! Values written while the object is being created are not seen here
//! (the engine intercepts neither initial values nor a binding's first
//! value), so nothing animates into its initial state.

use std::rc::Rc;

use ljos_engine::{
    Atom, Engine, Interception, ObjectId, PropIndex, QmlObject, QtObject, TypeTag, Value,
};
use ljos_reactive::WriteKind;

use super::driver;
use super::timeline::{Defaults, moves};

/// `Behavior` (QtQuick).
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick",
    name = "Behavior",
    extends = QtObject,
    default = "animation"
)]
pub struct Behavior {
    #[qml(base)]
    pub object: QtObject,
    #[qml(notify)]
    pub enabled: bool,
    #[qml(notify)]
    pub animation: Option<ObjectId>,
    /// The value the property is on its way to; `undefined` until the
    /// first one arrives.
    #[qml(notify, readonly)]
    pub target_value: Value,
    #[qml(group)]
    pub target_property: BehaviorTarget,
    /// The value a binding handed over that the animation has yet to be
    /// started for.
    #[qml(skip)]
    waiting: Option<Value>,
}

impl Default for Behavior {
    fn default() -> Self {
        Self {
            object: QtObject::default(),
            enabled: true,
            animation: None,
            target_value: Value::Undefined,
            target_property: BehaviorTarget::default(),
            waiting: None,
        }
    }
}

/// The `targetProperty` group of [`Behavior`]: what it was declared on.
#[derive(Debug, Default, QmlObject)]
#[qml(name = "BehaviorTarget", uncreatable)]
pub struct BehaviorTarget {
    #[qml(notify, readonly)]
    pub name: Atom,
    #[qml(notify, readonly)]
    pub object: Option<ObjectId>,
}

/// Puts `behavior` in front of `prop` of `target` (`Behavior on prop`).
pub(crate) fn attach(engine: &Engine, behavior: ObjectId, target: ObjectId, prop: PropIndex) {
    let info = engine
        .meta_of(target)
        .and_then(|m| m.property(prop).map(|p| (p.name.clone(), p.ty.clone())));
    let Some((name, ty)) = info else {
        return;
    };
    for (path, v) in [
        ("targetProperty.name", Value::string(name)),
        ("targetProperty.object", Value::Object(target)),
    ] {
        if let Ok(p) = engine.property_index(behavior, path) {
            let _ = engine.write_internal(behavior, p, v, WriteKind::Imperative);
        }
    }
    engine.set_interceptor(
        target,
        prop,
        Some(Rc::new(move |e, v, how| {
            intercept(e, behavior, (target, prop), &ty, v, how)
        })),
    );
}

/// A value on its way into the property. True: held back, the animation
/// takes the property there.
fn intercept(
    e: &Engine,
    behavior: ObjectId,
    (target, prop): (ObjectId, PropIndex),
    ty: &TypeTag,
    v: &Value,
    how: Interception,
) -> bool {
    if !e.is_alive(behavior) {
        return false;
    }
    let (enabled, anim) = e.untrack(|| {
        (
            e.read(behavior, Behavior::ENABLED.index).to_boolean(),
            e.read(behavior, Behavior::ANIMATION.index).as_object(),
        )
    });
    let anim = anim.filter(|a| e.is_alive(*a));
    let to = ty.coerce(v).unwrap_or_else(|_| v.clone());
    let defaults = Defaults {
        obj: target,
        prop,
        to: Some(to.clone()),
    };
    let through = match anim {
        Some(a) => !enabled || !moves(e, a, &defaults),
        None => true,
    };
    if how == Interception::Write {
        let _ = e.with_mut::<Behavior, _>(behavior, |b| b.waiting = None);
    }
    let Some(anim) = anim else {
        return false;
    };
    if through {
        // The value goes in as it is; an animation on its way to an
        // older value must not carry on over it.
        match how {
            // The animation has nothing to say about this property,
            // but it still runs.
            Interception::Write if enabled => retarget(e, behavior, anim, &defaults),
            Interception::Write => driver::stop_driven(e, anim),
            // Nothing may be written from inside the binding: the run
            // is dropped now and the rest follows.
            Interception::Binding => {
                let had = driver::forget_driven(e, anim);
                if enabled {
                    driver::defer(e, move |e| {
                        if e.is_alive(anim) {
                            retarget(e, behavior, anim, &defaults);
                        }
                    });
                } else if had {
                    driver::defer(e, move |e| driver::settle_driven(e, anim));
                }
            }
        }
        return false;
    }
    if driver::has_run(e, anim) {
        // Already on its way there.
        if e.untrack(|| e.read(behavior, Behavior::TARGET_VALUE.index)) == to {
            return true;
        }
    } else if e.peek(target, prop) == to {
        // No change, nothing to animate.
        return false;
    }
    match how {
        Interception::Write => retarget(e, behavior, anim, &defaults),
        Interception::Binding => {
            let _ = e.with_mut::<Behavior, _>(behavior, |b| b.waiting = Some(to.clone()));
            driver::defer(e, move |e| {
                // Unless a later value, or a write, came in between.
                let still = e
                    .with_mut::<Behavior, _>(behavior, |b| {
                        if b.waiting.as_ref() == Some(&to) {
                            b.waiting = None;
                            true
                        } else {
                            false
                        }
                    })
                    .unwrap_or(false);
                if still && e.is_alive(anim) {
                    retarget(e, behavior, anim, &defaults);
                }
            });
        }
    }
    true
}

fn note_target(e: &Engine, behavior: ObjectId, to: &Value) {
    let _ = e.write_internal(
        behavior,
        Behavior::TARGET_VALUE.index,
        to.clone(),
        WriteKind::Imperative,
    );
}

/// Starts the animation towards `defaults.to` from the value the property
/// shows now.
fn retarget(e: &Engine, behavior: ObjectId, anim: ObjectId, defaults: &Defaults) {
    if let Some(to) = &defaults.to {
        note_target(e, behavior, to);
    }
    driver::start_driven(e, anim, defaults, None);
}
