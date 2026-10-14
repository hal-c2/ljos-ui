//! `ViewTransition` and the transitions a view runs for rows that move
//! or are pushed aside (doc.qt.io/qt-6/qml-qtquick-viewtransition.html).
//!
//! A view transition runs once per item it is for. Its animations move
//! that item: an animation of `x` or `y` that names no end of its own
//! takes the item to where the view has put its row, and the attached
//! `ViewTransition` of the transition says which item that is, where it
//! is going, and which rows the change that brought this about was
//! about. Each item's run is a timeline of its own, started under the
//! item, so a hundred displaced items run the same transition at once;
//! while a run is on its way the layout pass leaves the item to it.
//! When nothing runs, nothing asks for a frame.

use std::rc::Rc;

use ljos_engine::{Engine, MetaRef, ObjectId, Point, PropIndex, QmlObject, Value};

use super::state::SharedTargets;
use super::{ItemView, state_of, warn};
use crate::animation::driver;
use crate::animation::timeline::{Change, ChangeKind, Changes, Timed};
use crate::animation::transition::{self, Transition, animations};
use crate::flickable::{X, Y};
use crate::item::{Item, is_a, write_computed, write_placed};

/// `ViewTransition` (QtQuick), attached only: what a transition a view
/// runs can ask about the item it is running on.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtQuick",
    name = "ViewTransition",
    attached = ViewTransitionAttached,
    uncreatable
)]
pub struct ViewTransition {}

/// What `ViewTransition.` attaches to a transition it is run as.
#[derive(Debug, QmlObject)]
#[qml(name = "ViewTransitionAttached")]
pub struct ViewTransitionAttached {
    #[qml(notify, readonly)]
    pub item: Option<ObjectId>,
    #[qml(notify, readonly)]
    pub index: i32,
    #[qml(notify, readonly)]
    pub destination: Point,
    #[qml(notify, readonly)]
    pub target_items: Value,
    #[qml(notify, readonly)]
    pub target_indexes: Value,
}

impl Default for ViewTransitionAttached {
    fn default() -> Self {
        Self {
            item: None,
            index: -1,
            destination: Point::default(),
            target_items: Value::list(Vec::new()),
            target_indexes: Value::list(Vec::new()),
        }
    }
}

/// Puts on `transition` the `ViewTransition` of the run of it that is
/// about to start on `item`.
pub(crate) fn tell(
    e: &Engine,
    transition: ObjectId,
    item: ObjectId,
    index: i32,
    destination: [f64; 2],
    targets: &SharedTargets,
) {
    let Ok(att) = e.attached_object(
        transition,
        &MetaRef::Static(<ViewTransition as ljos_engine::QmlType>::static_meta()),
    ) else {
        return;
    };
    let write = |p: PropIndex, v: Value| warn(e, "ViewTransition", write_computed(e, att, p, v));
    e.batch(|| {
        write(ViewTransitionAttached::ITEM.index, Value::Object(item));
        write(ViewTransitionAttached::INDEX.index, Value::Int(index));
        write(
            ViewTransitionAttached::DESTINATION.index,
            Value::point(destination[X], destination[Y]),
        );
        write(
            ViewTransitionAttached::TARGET_ITEMS.index,
            Value::list(targets.items.iter().copied().map(Value::Object).collect()),
        );
        write(
            ViewTransitionAttached::TARGET_INDEXES.index,
            Value::list(targets.indexes.iter().copied().map(Value::Int).collect()),
        );
    });
}

/// The properties the animations in `anims` name, in the order first
/// named: a transition may name the same one in every animation it holds.
fn named_properties(e: &Engine, anims: &[ObjectId], out: &mut Vec<String>) {
    for anim in anims {
        for field in ["property", "properties"] {
            let said = e.read_by_name(*anim, field).to_js_string();
            for name in said.split(',') {
                let name = name.trim();
                if !name.is_empty() && !out.iter().any(|n| n == name) {
                    out.push(name.to_owned());
                }
            }
        }
        let children: Vec<ObjectId> = match e.read_by_name(*anim, "animations") {
            Value::List(l) => l.iter().filter_map(Value::as_object).collect(),
            _ => Vec::new(),
        };
        named_properties(e, &children, out);
    }
}

fn axis_prop(axis: usize) -> PropIndex {
    if axis == X {
        Item::X.index
    } else {
        Item::Y.index
    }
}

/// Runs `transition` on `item` towards the place the view has for its
/// row, `destination` in content coordinates. The axes the transition
/// does not move are written at once and the item is where it belongs on
/// them before this returns; the ones it does move are left where they
/// are for the animation to take from there. The result is which axes
/// the run itself is moving, empty when there was nothing to run (and
/// every axis has been written then).
pub(crate) fn start(
    e: &Engine,
    view: ObjectId,
    transition: Option<ObjectId>,
    item: ObjectId,
    index: i32,
    destination: [f64; 2],
    targets: SharedTargets,
) -> [bool; 2] {
    let place = |e: &Engine, axis: usize| {
        warn(
            e,
            "place",
            write_placed(e, item, axis_prop(axis), Value::Real(destination[axis])),
        );
    };
    let Some(t) = transition.filter(|t| e.is_alive(*t) && is_a::<Transition>(e, *t)) else {
        place(e, X);
        place(e, Y);
        return [false; 2];
    };
    let enabled = e
        .untrack(|| e.read(t, Transition::ENABLED.index))
        .to_boolean();
    let anims = if enabled {
        animations(e, t)
    } else {
        Vec::new()
    };
    if anims.is_empty() {
        place(e, X);
        place(e, Y);
        return [false; 2];
    }
    tell(e, t, item, index, destination, &targets);
    let mut list: Vec<Change> = [X, Y]
        .into_iter()
        .map(|axis| Change {
            obj: item,
            prop: axis_prop(axis),
            to: Value::Real(destination[axis]),
            kind: ChangeKind::Property,
        })
        .collect();
    // The other properties a transition animates (an opacity it fades
    // displaced items with) are changes to the values they have: the
    // timeline then takes an animation that names one to the end the
    // animation itself says.
    let mut names = Vec::new();
    named_properties(e, &anims, &mut names);
    for name in names {
        if name == "x" || name == "y" {
            continue;
        }
        if let Ok(prop) = e.property_index(item, &name) {
            list.push(Change {
                obj: item,
                prop,
                to: e.untrack(|| e.read(item, prop)),
                kind: ChangeKind::Property,
            });
        }
    }
    let changes = Changes {
        list,
        reversed: false,
        scripts: Vec::new(),
    };
    let timeline = Timed::of_transition(e, &anims, &changes);
    let mut moved: Vec<(ObjectId, PropIndex)> = Vec::new();
    timeline.moved(&mut moved);
    let going = [
        moved.contains(&(item, Item::X.index)),
        moved.contains(&(item, Item::Y.index)),
    ];
    for axis in [X, Y] {
        if !going[axis] {
            place(e, axis);
        }
    }
    if let Some(st) = state_of(e, view) {
        if going.iter().any(|g| *g) {
            st.vt_places.borrow_mut().insert(
                item,
                [
                    going[X].then_some(destination[X]),
                    going[Y].then_some(destination[Y]),
                ],
            );
        }
        let mut counts = st.vt_counts.borrow_mut();
        let n = counts.entry(t).or_insert(0);
        *n += 1;
        if *n == 1 {
            drop(counts);
            transition::show_running(e, t, true);
        }
    }
    let done: driver::Done = Rc::new(move |e| {
        if let Some(st) = state_of(e, view) {
            st.vt_places.borrow_mut().remove(&item);
            let mut counts = st.vt_counts.borrow_mut();
            if let Some(n) = counts.get_mut(&t) {
                *n = n.saturating_sub(1);
                if *n == 0 {
                    counts.remove(&t);
                    drop(counts);
                    transition::show_running(e, t, false);
                }
            }
        }
    });
    driver::start_timeline(e, item, timeline, Some(done));
    going
}

/// Ends the run a view transition has on `item`, where it is.
pub(crate) fn forget(e: &Engine, view: ObjectId, item: ObjectId) {
    if driver::forget_driven(e, item)
        && let Some(st) = state_of(e, view)
    {
        st.vt_places.borrow_mut().remove(&item);
    }
}

/// The transition of `view` for an item pushed aside by `kind`, or that
/// moved itself: the one named for the change, the plain displaced one
/// when there is none of that name.
pub(crate) fn transition_for(
    e: &Engine,
    view: ObjectId,
    kind: super::state::OpKind,
    target: bool,
) -> Option<ObjectId> {
    use super::state::OpKind;
    let get = |p: ljos_engine::Prop<Option<ObjectId>>| e.get(view, p).ok().flatten();
    let displaced = |specific: Option<ObjectId>| specific.or_else(|| get(ItemView::DISPLACED));
    match (kind, target) {
        (OpKind::Move, true) => get(ItemView::MOVE_TRANSITION),
        (OpKind::Move, false) => displaced(get(ItemView::MOVE_DISPLACED)),
        (OpKind::Add, false) => displaced(get(ItemView::ADD_DISPLACED)),
        (OpKind::Remove, false) => displaced(get(ItemView::REMOVE_DISPLACED)),
        _ => None,
    }
}
