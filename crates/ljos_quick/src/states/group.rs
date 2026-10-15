//! The states of one object: which it is in, what that state put where,
//! and the change from one state to the next.
//!
//! A [`Group`] belongs to an item with `states`, or to a `StateGroup`. It
//! stands in front of the object's `state` (an engine interceptor) and
//! follows each state's `when`.
//!
//! Entering a state pushes every change onto the override layer of its
//! property and remembers the layer; leaving takes the layers off in the
//! opposite order. The property's own value or binding was never touched,
//! so it is what shows again.
//!
//! A change of state that one of the object's transitions is for is shown
//! by that transition. The properties are in the new state at once, as
//! far as bindings and layers go; each property that got another value is
//! then held at the value it had (one more layer, on top), the
//! transition's animations move what they name, and when the transition
//! is over the holds come off and the state's values show. A property the
//! transition does not name is not held and has its new value at once,
//! unless a `Behavior` takes it there.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use ljos_engine::{
    Atom, Engine, ForeignMember, ForeignValue, Interception, LogLevel, ObjectId, OverrideContent,
    OverrideId, PropIndex, RestoreMode, TypeTag, Value,
};
use ljos_reactive::WriteKind;
use ljos_render::kurbo::{Affine, Point};

use super::types::{
    AnchorChanges, ParentChange, PropertyChanges, State, StateChangeScript, StateGroup,
};
use crate::anchors;
use crate::animation::behavior;
use crate::animation::driver;
use crate::animation::timeline::{Change, ChangeKind, Changes, Timed, script_names};
use crate::animation::transition;
use crate::item::{AnchorProps, Item, init_item, is_a, is_item, is_unset, scene_transform};

/// The groups by the object each belongs to.
#[derive(Default)]
struct Groups {
    by_owner: RefCell<HashMap<ObjectId, Rc<Group>>>,
    /// Owners whose states wait for [`start`]: a host of content, whose
    /// declared children have no parent until it is complete.
    deferred: RefCell<Vec<ObjectId>>,
}

/// Holds back the states of `owner` until [`start`]. The states are
/// entered when the first of them is complete, which is before the items
/// declared in a host have been given their parent, and a binding of one
/// of them that reads `parent` would see none.
pub(crate) fn defer(engine: &Engine, owner: ObjectId) {
    let groups = engine.extension::<Groups>();
    let mut deferred = groups.deferred.borrow_mut();
    deferred.retain(|o| engine.is_alive(*o));
    if !deferred.contains(&owner) && !groups.by_owner.borrow().contains_key(&owner) {
        deferred.push(owner);
    }
}

/// Gives a deferred owner its states.
pub(crate) fn start(engine: &Engine, owner: ObjectId) {
    let groups = engine.extension::<Groups>();
    let was = {
        let mut deferred = groups.deferred.borrow_mut();
        let before = deferred.len();
        deferred.retain(|o| *o != owner);
        deferred.len() != before
    };
    if was {
        attach(engine, owner);
    }
}

pub(crate) fn is_deferred(engine: &Engine, owner: ObjectId) -> bool {
    engine
        .extension::<Groups>()
        .deferred
        .borrow()
        .contains(&owner)
}

/// What a state put in place, to be taken away when it is left.
enum Applied {
    Layer {
        obj: ObjectId,
        prop: PropIndex,
        id: OverrideId,
        restore: RestoreMode,
        what: Layer,
    },
    /// The parent `target` had before a `ParentChange`.
    Parent {
        target: ObjectId,
        old: Option<ObjectId>,
    },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Layer {
    /// Of a `PropertyChanges`: a transition may animate the way back.
    Property,
    /// Of an `AnchorChanges`, on an anchor line.
    Anchor,
    /// Of a `ParentChange`: the item is put back at once.
    Parent,
}

/// A transition under way.
struct Running {
    transition: ObjectId,
    /// What keeps each animated property away from the state's value.
    holds: Vec<(ObjectId, PropIndex, OverrideId, ChangeKind)>,
    /// What keeps the anchors of each item being moved out of the way.
    suspended: Vec<(ObjectId, PropIndex, OverrideId)>,
}

pub(crate) struct Group {
    owner: ObjectId,
    state: PropIndex,
    states: PropIndex,
    transitions: PropIndex,
    /// The state the applied changes are of.
    current: RefCell<String>,
    applied: RefCell<Vec<Applied>>,
    /// The state the object starts in has been entered: from here on a
    /// change of state may be animated.
    started: Cell<bool>,
    /// A change is being made. One asked for meanwhile (by a script of
    /// the state) is made when this one is done.
    changing: Cell<bool>,
    again: Cell<bool>,
    running: RefCell<Option<Running>>,
    /// Counts the transitions, so that the end of one that was cut short
    /// is not taken for the end of the present one.
    serial: Cell<u64>,
}

/// Gives `owner` its states: from here on its `state` and the `when` of
/// each state decide what its states have changed. The state it is in at
/// this moment is entered without a transition. More calls do nothing.
pub(crate) fn attach(engine: &Engine, owner: ObjectId) {
    let groups = engine.extension::<Groups>();
    if groups.by_owner.borrow().contains_key(&owner) {
        return;
    }
    let (state, states, transitions) = if is_a::<StateGroup>(engine, owner) {
        (
            StateGroup::STATE.index,
            StateGroup::STATES.index,
            StateGroup::TRANSITIONS.index,
        )
    } else if is_item(engine, owner) {
        (
            Item::STATE.index,
            Item::STATES.index,
            Item::TRANSITIONS.index,
        )
    } else {
        return;
    };
    let g = Rc::new(Group {
        owner,
        state,
        states,
        transitions,
        current: RefCell::default(),
        applied: RefCell::default(),
        started: Cell::new(false),
        changing: Cell::new(false),
        again: Cell::new(false),
        running: RefCell::new(None),
        serial: Cell::new(0),
    });
    {
        let mut by_owner = groups.by_owner.borrow_mut();
        // Now and then, so the map does not keep what is gone.
        if by_owner.len() >= 64 && by_owner.len().is_power_of_two() {
            by_owner.retain(|o, _| engine.is_alive(*o));
        }
        by_owner.insert(owner, Rc::clone(&g));
    }
    // A written `state` is stored here, so that who follows it is told
    // before the changes are made (measured on qml6), and the changes
    // are made before the write returns.
    let on_write = Rc::clone(&g);
    engine.set_interceptor(
        owner,
        state,
        Some(Rc::new(move |e, v, how| match how {
            Interception::Write => {
                let g = &on_write;
                let name = Value::string(v.to_js_string());
                // The watch below must not make the change from inside
                // the write: it is made right after it.
                let was = g.changing.replace(true);
                let _ = e.write_internal(g.owner, g.state, name, WriteKind::Imperative);
                g.changing.set(was);
                g.sync(e);
                true
            }
            Interception::Binding => false,
        })),
    );
    // A bound `state` arrives here.
    let on_change = Rc::clone(&g);
    let _ = engine.watch(owner, state, move |e, _| on_change.sync(e));
    for s in g.all_states(engine) {
        let on_when = Rc::clone(&g);
        let _ = engine.watch(s, State::WHEN.index, move |e, _| on_when.follow_when(e));
    }
    g.follow_when(engine);
    g.sync(engine);
    g.started.set(true);
}

fn objects(engine: &Engine, obj: ObjectId, prop: PropIndex) -> Vec<ObjectId> {
    match engine.untrack(|| engine.read(obj, prop)) {
        Value::List(l) => l
            .iter()
            .filter_map(Value::as_object)
            .filter(|o| engine.is_alive(*o))
            .collect(),
        _ => Vec::new(),
    }
}

fn text(engine: &Engine, obj: ObjectId, prop: PropIndex) -> String {
    engine
        .untrack(|| engine.read(obj, prop))
        .to_js_string()
        .to_string()
}

fn type_of(engine: &Engine, obj: ObjectId, prop: PropIndex) -> TypeTag {
    engine
        .meta_of(obj)
        .and_then(|m| m.property(prop).map(|p| p.ty.clone()))
        .unwrap_or(TypeTag::Var)
}

/// The members of one value (`font`) that the changes of a state give.
struct Part {
    target: ObjectId,
    prop: PropIndex,
    ty: TypeTag,
    members: Vec<(String, ForeignValue)>,
}

/// What one change of state gathers on its way.
#[derive(Default)]
struct Round {
    /// The value each property had before anything was done to it, and
    /// who may animate it from there.
    before: Vec<(ObjectId, PropIndex, Value, ChangeKind)>,
    /// The items whose anchors the old or the new state changes.
    anchored: Vec<ObjectId>,
    /// The named scripts of the state entered.
    scripts: Vec<(Atom, ObjectId, PropIndex)>,
}

impl Round {
    /// Notes what `prop` shows, unless it is noted already: the first
    /// value is the one a transition starts from. A Behavior still on its
    /// way to an older state's value is ended, so that nothing of it is
    /// in the way of the layers to come.
    fn note(&mut self, engine: &Engine, obj: ObjectId, prop: PropIndex, kind: ChangeKind) {
        if self.before.iter().any(|(o, p, ..)| *o == obj && *p == prop) {
            return;
        }
        let v = engine.untrack(|| engine.read(obj, prop));
        behavior::release(engine, obj, prop);
        self.before.push((obj, prop, v, kind));
    }

    /// Says where `prop` starts from, whatever it shows.
    fn start_at(
        &mut self,
        engine: &Engine,
        obj: ObjectId,
        prop: PropIndex,
        v: Value,
        kind: ChangeKind,
    ) {
        behavior::release(engine, obj, prop);
        self.before.retain(|(o, p, ..)| !(*o == obj && *p == prop));
        self.before.push((obj, prop, v, kind));
    }
}

impl Group {
    fn all_states(&self, engine: &Engine) -> Vec<ObjectId> {
        objects(engine, self.owner, self.states)
            .into_iter()
            .filter(|s| is_a::<State>(engine, *s))
            .collect()
    }

    fn find(&self, engine: &Engine, name: &str) -> Option<ObjectId> {
        if name.is_empty() {
            return None;
        }
        self.all_states(engine)
            .into_iter()
            .find(|s| text(engine, *s, State::NAME.index) == name)
    }

    /// One `when` changed: the object is in the first state whose `when`
    /// is true, and in the base state when the one that held it there
    /// stopped being true.
    fn follow_when(self: &Rc<Self>, engine: &Engine) {
        let conditional = |s: ObjectId| !is_unset(engine, s, State::WHEN.index);
        let holds = |s: ObjectId| {
            engine
                .untrack(|| engine.read(s, State::WHEN.index))
                .to_boolean()
        };
        let states = self.all_states(engine);
        let now = text(engine, self.owner, self.state);
        let want = match states.iter().find(|s| conditional(**s) && holds(**s)) {
            Some(s) => text(engine, *s, State::NAME.index),
            // A state entered by name is not left because a condition
            // somewhere is false.
            None if self.find(engine, &now).is_some_and(conditional) => String::new(),
            None => return,
        };
        if want == now {
            return;
        }
        let was = self.changing.replace(true);
        let _ = engine.write_internal(
            self.owner,
            self.state,
            Value::string(want),
            WriteKind::Imperative,
        );
        self.changing.set(was);
        self.sync(engine);
    }

    /// Brings what is applied in line with `state`.
    fn sync(self: &Rc<Self>, engine: &Engine) {
        if self.changing.replace(true) {
            self.again.set(true);
            return;
        }
        // Bounded: two scripts that send each other back and forth must
        // not hang the engine.
        for _ in 0..64 {
            self.again.set(false);
            let want = text(engine, self.owner, self.state);
            let from = self.current.borrow().clone();
            if want != from {
                *self.current.borrow_mut() = want.clone();
                self.go(engine, &from, &want);
            }
            if !self.again.get() {
                break;
            }
        }
        self.changing.set(false);
    }

    /// The changes of the state called `name`, those of the states it
    /// extends first.
    fn changes_of(&self, engine: &Engine, name: &str, seen: &mut Vec<ObjectId>) -> Vec<ObjectId> {
        let Some(state) = self.find(engine, name) else {
            return Vec::new();
        };
        if seen.contains(&state) {
            return Vec::new();
        }
        seen.push(state);
        let base = text(engine, state, State::EXTEND.index);
        let mut out = self.changes_of(engine, &base, seen);
        out.extend(objects(engine, state, State::CHANGES.index));
        out
    }

    /// Ends the transition under way where it is. Its properties are
    /// noted as they show, which is where the next one starts.
    fn interrupt(&self, engine: &Engine, round: &mut Round) {
        let Some(run) = self.running.borrow_mut().take() else {
            return;
        };
        self.serial.set(self.serial.get() + 1);
        driver::forget_driven(engine, run.transition);
        for (obj, prop, id, kind) in run.holds {
            // An item on its way to a place in another parent has no
            // place to go on from once it is given back.
            if kind == ChangeKind::Property {
                round.note(engine, obj, prop, kind);
            }
            let _ = engine.remove_override(obj, prop, id, RestoreMode::BindingOrValue);
        }
        for (obj, prop, id) in run.suspended {
            if !round.anchored.contains(&obj) {
                round.anchored.push(obj);
            }
            let _ = engine.remove_override(obj, prop, id, RestoreMode::BindingOrValue);
        }
        transition::show_running(engine, run.transition, false);
    }

    /// The transition is over: the state's values show.
    fn settle(&self, engine: &Engine) {
        let Some(run) = self.running.borrow_mut().take() else {
            return;
        };
        engine.batch(|| {
            let holds = run.holds.into_iter().map(|(o, p, id, _)| (o, p, id));
            for (obj, prop, id) in holds.chain(run.suspended) {
                let _ = engine.remove_override(obj, prop, id, RestoreMode::BindingOrValue);
            }
            transition::show_running(engine, run.transition, false);
        });
    }

    /// Leaves the state called `from` for the one called `to`.
    fn go(self: &Rc<Self>, engine: &Engine, from: &str, to: &str) {
        engine.batch(|| {
            let mut round = Round::default();
            self.interrupt(engine, &mut round);
            let chosen = if self.started.get() {
                let all = objects(engine, self.owner, self.transitions);
                transition::choose(engine, &all, from, to)
            } else {
                None
            };
            let anims = chosen.map_or_else(Vec::new, |(t, _)| transition::animations(engine, t));
            let deferred = script_names(engine, &anims);

            self.leave(engine, &mut round);
            let mut seen = Vec::new();
            for change in self.changes_of(engine, to, &mut seen) {
                self.enter(engine, change, &mut round, &deferred);
            }

            let mut changes = Changes {
                list: Vec::new(),
                reversed: chosen.is_some_and(|(_, reversed)| reversed),
                scripts: std::mem::take(&mut round.scripts),
            };
            let mut starts = Vec::new();
            for (obj, prop, before, kind) in std::mem::take(&mut round.before) {
                if !engine.is_alive(obj) {
                    continue;
                }
                let after = engine.untrack(|| engine.read(obj, prop));
                if after != before {
                    changes.list.push(Change {
                        obj,
                        prop,
                        to: after,
                        kind,
                    });
                    starts.push(before);
                }
            }
            match chosen {
                Some((t, _)) => self.run(engine, t, &anims, changes, starts, &round.anchored),
                None if self.started.get() => {
                    for (c, before) in changes.list.iter().zip(&starts) {
                        if c.kind == ChangeKind::Property {
                            behavior::follow(engine, c.obj, c.prop, before, &c.to);
                        }
                    }
                }
                None => {}
            }
        });
    }

    /// Takes away what the state that is left put in place, last first.
    fn leave(&self, engine: &Engine, round: &mut Round) {
        let applied = std::mem::take(&mut *self.applied.borrow_mut());
        for a in &applied {
            if let Applied::Layer {
                obj, prop, what, ..
            } = a
            {
                match what {
                    Layer::Property => round.note(engine, *obj, *prop, ChangeKind::Property),
                    Layer::Anchor if !round.anchored.contains(obj) => round.anchored.push(*obj),
                    Layer::Anchor | Layer::Parent => {}
                }
            }
        }
        for a in applied.into_iter().rev() {
            match a {
                Applied::Layer {
                    obj,
                    prop,
                    id,
                    restore,
                    ..
                } => {
                    let _ = engine.remove_override(obj, prop, id, restore);
                }
                Applied::Parent { target, old } => {
                    if engine.is_alive(target) && old.is_none_or(|p| engine.is_alive(p)) {
                        let _ = crate::tree::set_parent_item(engine, target, old);
                    }
                }
            }
        }
    }

    fn push(
        &self,
        engine: &Engine,
        (obj, prop): (ObjectId, PropIndex),
        content: OverrideContent,
        restore: RestoreMode,
        what: Layer,
    ) {
        if let Ok(id) = engine.push_override(obj, prop, content) {
            self.applied.borrow_mut().push(Applied::Layer {
                obj,
                prop,
                id,
                restore,
                what,
            });
        }
    }

    /// Puts one change of the state entered in place.
    fn enter(&self, engine: &Engine, change: ObjectId, round: &mut Round, deferred: &[Atom]) {
        if is_a::<PropertyChanges>(engine, change) {
            self.enter_properties(engine, change, round);
        } else if is_a::<AnchorChanges>(engine, change) {
            self.enter_anchors(engine, change, round);
        } else if is_a::<ParentChange>(engine, change) {
            self.enter_parent(engine, change, round);
        } else if is_a::<StateChangeScript>(engine, change) {
            let prop = StateChangeScript::SCRIPT.index;
            let name = text(engine, change, StateChangeScript::NAME.index);
            if !name.is_empty() {
                round.scripts.push((Atom::from(&*name), change, prop));
                // The transition runs it, in its own time.
                if deferred.iter().any(|d| **d == *name) {
                    return;
                }
            }
            engine.run_script(change, prop);
        }
    }

    fn enter_properties(&self, engine: &Engine, change: ObjectId, round: &mut Round) {
        let Ok(members) = engine.with::<PropertyChanges, _>(change, |pc| pc.members.clone()) else {
            return;
        };
        let aimed = engine
            .untrack(|| engine.read(change, PropertyChanges::TARGET.index))
            .as_object();
        let flag = |p: PropIndex| engine.untrack(|| engine.read(change, p)).to_boolean();
        let explicit = flag(PropertyChanges::EXPLICIT.index);
        let restore = if flag(PropertyChanges::RESTORE_ENTRY_VALUES.index) {
            RestoreMode::BindingOrValue
        } else {
            RestoreMode::None
        };
        let mut parts: Vec<Part> = Vec::new();
        for ForeignMember {
            object,
            name,
            attached,
            value,
            at,
        } in members
        {
            let Some(mut target) = object.or(aimed).filter(|t| engine.is_alive(*t)) else {
                continue;
            };
            if let Some(meta) = &attached {
                match engine.attached_object(target, meta) {
                    Ok(a) => target = a,
                    Err(_) => continue,
                }
            }
            let written = name;
            let mut name: &str = &written;
            // `button.opacity` where `button` holds an object (an alias of
            // a child, say): the change is to that object's property.
            while engine.property_index(target, name).is_err()
                && let Some((head, rest)) = name.split_once('.')
                && let Ok(p) = engine.property_index(target, head)
                && let Some(o) = engine
                    .untrack(|| engine.read(target, p))
                    .as_object()
                    .filter(|o| engine.is_alive(*o))
            {
                target = o;
                name = rest;
            }
            let Ok(prop) = engine.property_index(target, name) else {
                // `font.pixelSize`, `sourceSize.height`: one member of a
                // value. Members of one value go in together, below.
                if let Some((head, member)) = name.rsplit_once('.')
                    && let Ok(whole) = engine.property_index(target, head)
                {
                    let ty = type_of(engine, target, whole);
                    if Value::Undefined
                        .with_member(&ty, member, &Value::Undefined)
                        .is_some()
                    {
                        let slot = match parts
                            .iter()
                            .position(|p| p.target == target && p.prop == whole)
                        {
                            Some(i) => i,
                            None => {
                                parts.push(Part {
                                    target,
                                    prop: whole,
                                    ty,
                                    members: Vec::new(),
                                });
                                parts.len() - 1
                            }
                        };
                        parts[slot].members.push((member.to_owned(), value));
                        continue;
                    }
                }
                engine.log(
                    LogLevel::Warning,
                    "qml",
                    format!(
                        "{at}: QML PropertyChanges: Cannot assign to non-existent property \"{written}\""
                    ),
                );
                continue;
            };
            if is_item(engine, target) {
                let _ = init_item(engine, target);
            }
            let ty = type_of(engine, target, prop);
            let content = match value {
                // `undefined` takes the property back to what it is
                // without a value, which only a binding's value does.
                ForeignValue::Value(Value::Undefined) => {
                    OverrideContent::Binding(Box::new(|_| Value::Undefined))
                }
                ForeignValue::Value(v) => OverrideContent::Value(v),
                ForeignValue::Binding(f) if explicit => match engine.untrack(|| f(engine, &ty)) {
                    Some(v) => OverrideContent::Value(v),
                    None => continue,
                },
                ForeignValue::Binding(f) => {
                    // A run that failed keeps the value of the last that
                    // did not.
                    let mut last = engine.untrack(|| engine.read(target, prop));
                    OverrideContent::Binding(Box::new(move |e| {
                        if let Some(v) = f(e, &ty) {
                            last = v;
                        }
                        last.clone()
                    }))
                }
            };
            round.note(engine, target, prop, ChangeKind::Property);
            self.push(engine, (target, prop), content, restore, Layer::Property);
        }
        for Part {
            target,
            prop,
            ty,
            members,
        } in parts
        {
            if is_item(engine, target) {
                let _ = init_item(engine, target);
            }
            let base = engine.untrack(|| engine.read(target, prop));
            let given = move |e: &Engine, value: &ForeignValue| match value {
                ForeignValue::Value(v) => Some(v.clone()),
                ForeignValue::Binding(f) => f(e, &TypeTag::Var),
            };
            let content = if explicit
                || members
                    .iter()
                    .all(|(_, v)| matches!(v, ForeignValue::Value(_)))
            {
                let mut now = base;
                for (member, value) in &members {
                    let v = engine.untrack(|| given(engine, value));
                    if let Some(n) = v.and_then(|v| now.with_member(&ty, member, &v)) {
                        now = n;
                    }
                }
                OverrideContent::Value(now)
            } else {
                // One layer for the whole value: what the members read
                // is followed, and the other members are what they were.
                OverrideContent::Binding(Box::new(move |e| {
                    let mut now = base.clone();
                    for (member, value) in &members {
                        if let Some(v) = given(e, value)
                            && let Some(n) = now.with_member(&ty, member, &v)
                        {
                            now = n;
                        }
                    }
                    now
                }))
            };
            round.note(engine, target, prop, ChangeKind::Property);
            self.push(engine, (target, prop), content, restore, Layer::Property);
        }
    }

    fn enter_anchors(&self, engine: &Engine, change: ObjectId, round: &mut Round) {
        let target = engine
            .untrack(|| engine.read(change, AnchorChanges::TARGET.index))
            .as_object()
            .filter(|t| engine.is_alive(*t) && is_item(engine, *t));
        let Some(target) = target else {
            return;
        };
        let _ = init_item(engine, target);
        if !round.anchored.contains(&target) {
            round.anchored.push(target);
        }
        let a = AnchorProps::get();
        for (line, to) in [
            ("anchors.left", a.left),
            ("anchors.right", a.right),
            ("anchors.top", a.top),
            ("anchors.bottom", a.bottom),
            ("anchors.horizontalCenter", a.horizontal_center),
            ("anchors.verticalCenter", a.vertical_center),
            ("anchors.baseline", a.baseline),
        ] {
            let Ok(from) = engine.property_index(change, line) else {
                continue;
            };
            let taken = move |e: &Engine| {
                e.with::<AnchorChanges, _>(change, |c| c.takes_off(from))
                    .unwrap_or(false)
            };
            // A line that was taken back holds `undefined` as one nobody
            // named does, and is the opposite: the anchor goes.
            let off = taken(engine)
                && matches!(
                    engine.untrack(|| engine.read(change, from)),
                    Value::Undefined
                );
            if !off && is_unset(engine, change, from) {
                continue;
            }
            // Followed: the line may be an expression. `undefined` takes
            // the anchor off, as it does written in the item.
            let mut kept = None;
            let content = OverrideContent::Binding(Box::new(move |e| {
                if off {
                    return Value::Undefined;
                }
                // Taken back while the state lasts: Qt goes on with the
                // anchor the state gave until the state is entered again.
                if let Some(kept) = &kept
                    && taken(e)
                {
                    return Value::clone(kept);
                }
                let now = e.read(change, from);
                kept = Some(now.clone());
                now
            }));
            self.push(
                engine,
                (target, to),
                content,
                RestoreMode::BindingOrValue,
                Layer::Anchor,
            );
        }
    }

    fn enter_parent(&self, engine: &Engine, change: ObjectId, round: &mut Round) {
        let read = |p: PropIndex| engine.untrack(|| engine.read(change, p));
        let target = read(ParentChange::TARGET.index)
            .as_object()
            .filter(|t| engine.is_alive(*t) && is_item(engine, *t));
        let parent = read(ParentChange::PARENT.index)
            .as_object()
            .filter(|p| engine.is_alive(*p) && is_item(engine, *p));
        let (Some(target), Some(parent)) = (target, parent) else {
            return;
        };
        let _ = init_item(engine, target);
        let old = engine.untrack(|| engine.visual_parent(target));
        // Where the item is now, as its new parent sees it: there it
        // stays, or from there it is moved.
        let place = engine.untrack(|| {
            let here = Point::new(
                engine.read(target, Item::X.index).to_number(),
                engine.read(target, Item::Y.index).to_number(),
            );
            let out = old.map_or(Affine::IDENTITY, |o| scene_transform(engine, o));
            scene_transform(engine, parent).inverse() * (out * here)
        });
        if crate::tree::set_parent_item(engine, target, Some(parent)).is_err() {
            return;
        }
        self.applied
            .borrow_mut()
            .push(Applied::Parent { target, old });
        for (given, prop, kept) in [
            (ParentChange::X.index, Item::X.index, Some(place.x)),
            (ParentChange::Y.index, Item::Y.index, Some(place.y)),
            (ParentChange::WIDTH.index, Item::WIDTH.index, None),
            (ParentChange::HEIGHT.index, Item::HEIGHT.index, None),
            (ParentChange::SCALE.index, Item::SCALE.index, None),
            (ParentChange::ROTATION.index, Item::ROTATION.index, None),
        ] {
            let set = !is_unset(engine, change, given);
            let content = match kept {
                _ if set => OverrideContent::Binding(Box::new(move |e| e.read(change, given))),
                Some(v) => OverrideContent::Value(Value::Real(v)),
                None => continue,
            };
            match kept {
                Some(v) => {
                    round.start_at(engine, target, prop, Value::Real(v), ChangeKind::Parent);
                }
                None => round.note(engine, target, prop, ChangeKind::Parent),
            }
            self.push(
                engine,
                (target, prop),
                content,
                RestoreMode::BindingOrValue,
                Layer::Parent,
            );
        }
    }

    /// Shows the change with `transition`.
    fn run(
        self: &Rc<Self>,
        engine: &Engine,
        transition: ObjectId,
        anims: &[ObjectId],
        mut changes: Changes,
        starts: Vec<Value>,
        anchored: &[ObjectId],
    ) {
        // Where the new anchors put each item, worked out while they are
        // in force; then they are taken out of the way, so that the item
        // stays where it is until an AnchorAnimation moves it or the
        // transition is over.
        let mut suspended = Vec::new();
        let a = AnchorProps::get();
        for item in anchored.iter().filter(|i| engine.is_alive(**i)) {
            let geometry = [
                Item::X.index,
                Item::WIDTH.index,
                Item::Y.index,
                Item::HEIGHT.index,
            ];
            for (prop, to) in geometry.into_iter().zip(anchors::resolve(engine, *item)) {
                if let Some(to) = to {
                    changes.list.push(Change {
                        obj: *item,
                        prop,
                        to: Value::Real(to),
                        kind: ChangeKind::Anchor,
                    });
                }
            }
            let lines = [
                a.left,
                a.right,
                a.top,
                a.bottom,
                a.horizontal_center,
                a.vertical_center,
                a.baseline,
            ];
            for prop in lines {
                let none = OverrideContent::Value(Value::Undefined);
                if let Ok(id) = engine.push_override(*item, prop, none) {
                    suspended.push((*item, prop, id));
                }
            }
            for prop in [a.fill, a.center_in] {
                let none = OverrideContent::Value(Value::Null);
                if let Ok(id) = engine.push_override(*item, prop, none) {
                    suspended.push((*item, prop, id));
                }
            }
        }
        // Every changed property is held at the value it had, so that an
        // animation that begins later still begins there.
        let mut holds = Vec::new();
        for (c, before) in changes.list.iter().zip(&starts) {
            let hold = OverrideContent::Value(before.clone());
            if let Ok(id) = engine.push_override(c.obj, c.prop, hold) {
                holds.push((c.obj, c.prop, id, c.kind));
            }
        }
        let timeline = Timed::of_transition(engine, anims, &changes);
        let mut moved = Vec::new();
        timeline.moved(&mut moved);
        // What the transition does not name has its new value at once,
        // or is its Behavior's to take there.
        holds.retain(|(obj, prop, id, _)| {
            if moved.contains(&(*obj, *prop)) {
                return true;
            }
            let _ = engine.remove_override(*obj, *prop, *id, RestoreMode::BindingOrValue);
            false
        });
        for (c, before) in changes.list.iter().zip(&starts) {
            if c.kind == ChangeKind::Property && !moved.contains(&(c.obj, c.prop)) {
                behavior::follow(engine, c.obj, c.prop, before, &c.to);
            }
        }
        let serial = self.serial.get() + 1;
        self.serial.set(serial);
        *self.running.borrow_mut() = Some(Running {
            transition,
            holds,
            suspended,
        });
        transition::show_running(engine, transition, true);
        let me = Rc::clone(self);
        let done: driver::Done = Rc::new(move |e| {
            if me.serial.get() == serial {
                me.settle(e);
            }
        });
        driver::start_timeline(engine, transition, timeline, Some(done));
    }
}
