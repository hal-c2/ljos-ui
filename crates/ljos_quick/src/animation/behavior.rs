//! `Behavior`: the animation a property runs whenever its value changes.
//!
//! A `Behavior on x` stands in front of `x` (an engine interceptor). A
//! value on its way in, from a JS write or from the property's binding, is
//! held back, and the Behavior's animation takes the property there from
//! the value it shows now. A new value while that is under way starts the
//! animation again from where the property is: Qt's retargeting rule.
//!
//! `Behavior on font.pixelSize` stands in front of `font` and lets
//! through what changes the other members. Its animation moves a number
//! of a [`MemberValue`] of its own, which goes into the member.
//!
//! Values written while the object is being created are not seen here
//! (the engine intercepts neither initial values nor a binding's first
//! value), so nothing animates into its initial state.

use rustc_hash::FxHashMap;
use std::cell::RefCell;
use std::rc::Rc;

use ljos_engine::{
    Atom, Engine, Interception, ObjectId, OverrideContent, OverrideId, PropIndex, QmlObject,
    QtObject, RestoreMode, TypeTag, Value,
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

/// What the animation of a Behavior on a member of a value moves: the
/// member's number, written into the value as it changes.
#[derive(Debug, Default, QmlObject)]
#[qml(name = "BehaviorMember", uncreatable)]
pub struct MemberValue {
    #[qml(base)]
    pub object: QtObject,
    #[qml(notify)]
    pub value: f64,
}

/// The Behaviors by the property each stands in front of: a change of
/// state has to find the one of a property it gave a value.
#[derive(Default)]
struct Attached {
    by_prop: RefCell<FxHashMap<(ObjectId, PropIndex), Follower>>,
}

struct Follower {
    behavior: ObjectId,
    /// What keeps the property at the animated value, over the value a
    /// state gave it, while the animation takes it there.
    hold: Option<OverrideId>,
    /// For a Behavior on a member of a value: what its animation moves.
    member: Option<Rc<Member>>,
}

/// The member of a value a Behavior is on, and the [`MemberValue`] its
/// animation moves.
struct Member {
    proxy: ObjectId,
    name: Atom,
    ty: TypeTag,
}

/// Lets the Behavior of `prop`, if it has one that animates, take the
/// property from `before` to `after`, the value a change of state just
/// gave it. True if it does. The property shows `before` at once.
///
/// A state's value never passes the Behavior on its way in (an override
/// is not intercepted), so the change of state asks here.
pub(crate) fn follow(
    engine: &Engine,
    target: ObjectId,
    prop: PropIndex,
    before: &Value,
    after: &Value,
) -> bool {
    let attached = engine.extension::<Attached>();
    let Some((behavior, member)) = attached
        .by_prop
        .borrow()
        .get(&(target, prop))
        .map(|f| (f.behavior, f.member.clone()))
    else {
        return false;
    };
    if !engine.is_alive(behavior) {
        attached.by_prop.borrow_mut().remove(&(target, prop));
        return false;
    }
    let (enabled, anim) = engine.untrack(|| {
        (
            engine.read(behavior, Behavior::ENABLED.index).to_boolean(),
            engine.read(behavior, Behavior::ANIMATION.index).as_object(),
        )
    });
    let Some(anim) = anim.filter(|a| engine.is_alive(*a)) else {
        return false;
    };
    // A Behavior on a member moves its number from the member's value
    // before to the state's; the state's other members show at once.
    let (mut defaults, held, from) = match &member {
        Some(m) => {
            let num = |v: &Value| v.member(&m.name).map(|n| n.to_number());
            let (Some(from), Some(to)) = (num(before), num(after)) else {
                return false;
            };
            if from == to {
                return false;
            }
            let Some(held) = after.with_member(&m.ty, &m.name, &Value::Real(from)) else {
                return false;
            };
            let defaults = Defaults {
                obj: m.proxy,
                prop: MemberValue::VALUE.index,
                to: Some(Value::Real(to)),
                aimed: None,
            };
            (defaults, held, Some((m.proxy, from)))
        }
        None => {
            let defaults = Defaults {
                obj: target,
                prop,
                to: Some(after.clone()),
                aimed: None,
            };
            (defaults, before.clone(), None)
        }
    };
    if !enabled || !moves(engine, anim, &mut defaults) {
        return false;
    }
    release(engine, target, prop);
    let Ok(hold) = engine.push_override(target, prop, OverrideContent::Value(held)) else {
        return false;
    };
    if let Some((proxy, from)) = from {
        let _ = engine.write_internal(
            proxy,
            MemberValue::VALUE.index,
            Value::Real(from),
            WriteKind::Animation,
        );
    }
    if let Some(f) = attached.by_prop.borrow_mut().get_mut(&(target, prop)) {
        f.hold = Some(hold);
    }
    match &defaults.to {
        Some(to) if member.is_some() => note_target(engine, behavior, to),
        _ => note_target(engine, behavior, after),
    }
    // At the end the value underneath shows again: the state's.
    let done: driver::Done = Rc::new(move |e| {
        let attached = e.extension::<Attached>();
        let mine = attached
            .by_prop
            .borrow_mut()
            .get_mut(&(target, prop))
            .and_then(|f| {
                if f.hold == Some(hold) {
                    f.hold.take()
                } else {
                    None
                }
            });
        if let Some(id) = mine {
            let _ = e.remove_override(target, prop, id, RestoreMode::BindingOrValue);
        }
    });
    driver::start_driven(engine, anim, &defaults, Some(done));
    true
}

/// Ends what [`follow`] started for `prop`, where it is: the property has
/// again the value underneath. For a change of state about to give the
/// property another value.
pub(crate) fn release(engine: &Engine, target: ObjectId, prop: PropIndex) {
    unhold(engine, target, prop, RestoreMode::BindingOrValue, true);
}

fn unhold(engine: &Engine, target: ObjectId, prop: PropIndex, mode: RestoreMode, stop: bool) {
    let attached = engine.extension::<Attached>();
    let held = attached
        .by_prop
        .borrow_mut()
        .get_mut(&(target, prop))
        .and_then(|f| f.hold.take().map(|id| (id, f.behavior)));
    let Some((id, behavior)) = held else {
        return;
    };
    if stop
        && let Some(anim) = engine
            .untrack(|| engine.read(behavior, Behavior::ANIMATION.index))
            .as_object()
    {
        driver::stop_driven(engine, anim);
    }
    let _ = engine.remove_override(target, prop, id, mode);
}

/// Puts `behavior` in front of `prop` of `target` (`Behavior on prop`).
pub(crate) fn attach(engine: &Engine, behavior: ObjectId, target: ObjectId, prop: PropIndex) {
    let info = engine
        .meta_of(target)
        .and_then(|m| m.property(prop).map(|p| (p.name.clone(), p.ty.clone())));
    let Some((name, ty)) = info else {
        return;
    };
    {
        let attached = engine.extension::<Attached>();
        let mut by_prop = attached.by_prop.borrow_mut();
        // Now and then, so the map does not keep what is gone.
        if by_prop.len() >= 64 && by_prop.len().is_power_of_two() {
            by_prop.retain(|_, f| engine.is_alive(f.behavior));
        }
        by_prop.insert(
            (target, prop),
            Follower {
                behavior,
                hold: None,
                member: None,
            },
        );
    }
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

/// Puts `behavior` in front of the member `member` of the value `prop` of
/// `target` holds (`Behavior on font.pixelSize`).
pub(crate) fn attach_member(
    engine: &Engine,
    behavior: ObjectId,
    target: ObjectId,
    prop: PropIndex,
    member: &str,
) -> bool {
    let info = engine
        .meta_of(target)
        .and_then(|m| m.property(prop).map(|p| (p.name.clone(), p.ty.clone())));
    let Some((name, ty)) = info else {
        return false;
    };
    let Ok(proxy) = engine.create_native(MemberValue::default(), Some(behavior)) else {
        return false;
    };
    let member = Atom::new(member);
    {
        let attached = engine.extension::<Attached>();
        attached.by_prop.borrow_mut().insert(
            (target, prop),
            Follower {
                behavior,
                hold: None,
                member: Some(Rc::new(Member {
                    proxy,
                    name: member.clone(),
                    ty: ty.clone(),
                })),
            },
        );
    }
    for (path, v) in [
        (
            "targetProperty.name",
            Value::string(format!("{name}.{member}")),
        ),
        ("targetProperty.object", Value::Object(target)),
    ] {
        if let Ok(p) = engine.property_index(behavior, path) {
            let _ = engine.write_internal(behavior, p, v, WriteKind::Imperative);
        }
    }
    let (into, m) = (ty.clone(), member.clone());
    let _ = engine.watch(proxy, MemberValue::VALUE.index, move |e, v| {
        let whole = e.peek(target, prop);
        if let Some(w) = whole.with_member(&into, &m, v)
            && w != whole
        {
            let _ = e.write_internal(target, prop, w, WriteKind::Animation);
        }
    });
    engine.set_interceptor(
        target,
        prop,
        Some(Rc::new(move |e, v, how| {
            intercept_member(e, (behavior, proxy), (target, prop), (&ty, &member), v, how)
        })),
    );
    true
}

/// A value on its way into a property whose member `member` has a
/// Behavior. True: held back, with the member where the animation has it.
fn intercept_member(
    e: &Engine,
    (behavior, proxy): (ObjectId, ObjectId),
    (target, prop): (ObjectId, PropIndex),
    (ty, member): (&TypeTag, &Atom),
    v: &Value,
    how: Interception,
) -> bool {
    if !e.is_alive(behavior) || !e.is_alive(proxy) {
        return false;
    }
    let (enabled, anim) = e.untrack(|| {
        (
            e.read(behavior, Behavior::ENABLED.index).to_boolean(),
            e.read(behavior, Behavior::ANIMATION.index).as_object(),
        )
    });
    let anim = anim.filter(|a| e.is_alive(*a));
    let whole = ty.coerce(v).unwrap_or_else(|_| v.clone());
    let Some(to) = whole.member(member).map(|m| m.to_number()) else {
        return false;
    };
    let mut defaults = Defaults {
        obj: proxy,
        prop: MemberValue::VALUE.index,
        to: Some(Value::Real(to)),
        aimed: None,
    };
    let through = match anim {
        Some(a) => !enabled || !moves(e, a, &mut defaults),
        None => true,
    };
    if how == Interception::Write {
        // A value written while the Behavior is on its way to a state's:
        // it goes on from where the member is, with nothing held.
        unhold(e, target, prop, RestoreMode::None, false);
    }
    let shown = e.peek(target, prop).member(member).map(|m| m.to_number());
    let running = anim.is_some_and(|a| driver::has_run(e, a));
    if through {
        // The value goes in as it is, and an animation on its way to an
        // older one must not carry on over it.
        if running && let Some(a) = anim {
            match how {
                Interception::Write => driver::stop_driven(e, a),
                Interception::Binding => {
                    if driver::forget_driven(e, a) {
                        e.after_binding(move |e| {
                            if e.is_alive(a) {
                                driver::settle_driven(e, a);
                            }
                        });
                    }
                }
            }
        }
        e.after_binding(move |e| note_target(e, behavior, &Value::Real(to)));
        return false;
    }
    let Some(anim) = anim else { return false };
    let goal = if running {
        Some(
            e.untrack(|| e.read(behavior, Behavior::TARGET_VALUE.index))
                .to_number(),
        )
    } else {
        shown
    };
    if goal == Some(to) && !running {
        // The member does not change: the others go in at once.
        return false;
    }
    let Some(held) = whole.with_member(ty, member, &Value::Real(shown.unwrap_or(to))) else {
        return false;
    };
    // A write of another member (`font.bold = false`) comes as the whole
    // value with this member as it is shown.
    // DECISION: so a value equal to the one the animation shows now does
    // not retarget it, unlike on Qt, which sees which member was written.
    let restart = goal != Some(to) && shown != Some(to);
    let start = move |e: &Engine| {
        if !e.is_alive(anim) || !e.is_alive(proxy) {
            return;
        }
        if let Some(from) = shown {
            let _ = e.write_internal(
                proxy,
                MemberValue::VALUE.index,
                Value::Real(from),
                WriteKind::Animation,
            );
        }
        retarget(e, behavior, anim, &defaults);
    };
    match how {
        Interception::Write => {
            let _ = e.write_internal(target, prop, held, WriteKind::Imperative);
            if restart {
                start(e);
            }
        }
        // The binding stays: what it gave goes in, the member held back,
        // once it has given it.
        Interception::Binding => e.after_binding(move |e| {
            let _ = e.write_internal(target, prop, held, WriteKind::Animation);
            if restart {
                start(e);
            }
        }),
    }
    true
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
    let mut defaults = Defaults {
        obj: target,
        prop,
        to: Some(to.clone()),
        aimed: None,
    };
    let through = match anim {
        Some(a) => !enabled || !moves(e, a, &mut defaults),
        None => true,
    };
    if how == Interception::Write {
        let _ = e.with_mut::<Behavior, _>(behavior, |b| b.waiting = None);
        // A value written while the Behavior is on its way to a state's:
        // it goes on from where the property is, with nothing held.
        unhold(e, target, prop, RestoreMode::None, false);
    }
    let Some(anim) = anim else {
        // `targetValue` is the value that came, animated or not.
        e.after_binding(move |e| note_target(e, behavior, &to));
        return false;
    };
    if through {
        // The value goes in as it is; an animation on its way to an
        // older value must not carry on over it.
        match how {
            // The animation has nothing to say about this property,
            // but it still runs.
            Interception::Write if enabled => retarget(e, behavior, anim, &defaults),
            Interception::Write => {
                driver::stop_driven(e, anim);
                note_target(e, behavior, &to);
            }
            // Nothing may be written from inside the binding: the run
            // is dropped now and the rest follows when the binding has
            // given its value, before whatever changed its input goes on.
            Interception::Binding => {
                let had = driver::forget_driven(e, anim);
                e.after_binding(move |e| {
                    if !e.is_alive(behavior) || !e.is_alive(anim) {
                        return;
                    }
                    if enabled {
                        retarget(e, behavior, anim, &defaults);
                        return;
                    }
                    if had {
                        driver::settle_driven(e, anim);
                    }
                    note_target(e, behavior, &to);
                });
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
            e.after_binding(move |e| {
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
