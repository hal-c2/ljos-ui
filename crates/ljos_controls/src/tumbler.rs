//! `Tumbler` (QtQuick.Templates): the rows of a model as a column that
//! turns, the current row in the middle.
//!
//! Behaviour follows doc.qt.io/qt-6/qml-qtquick-controls-tumbler.html and
//! is pinned by `features/controls/tumbler.feature`. What qml6 shows and
//! the documentation leaves open:
//!
//! - A tumbler wraps when it has at least as many rows as it shows
//!   (`visibleItemCount`, five unless set); asking for `wrap` is kept.
//! - Its content item is a `PathView` when it wraps and a `ListView` when
//!   it does not; `flickDeceleration` reads what that view has (100 and
//!   1500) until it is set.
//! - A delegate is as wide as the tumbler's content area and
//!   `availableHeight / visibleItemCount` tall, one more than it shows
//!   being made when it wraps.
//! - `Tumbler.displacement` is how many rows a delegate is above the
//!   middle (below is negative), fractions while the rows are moving.
//! - An index that is no row is not taken; a model with fewer rows than
//!   the index goes back to the first; an emptied one to none.
//!
//! The template follows the view the style gives as content item
//! (`currentIndex`, `currentItem`, `moving`), and the style binds the
//! view to the template, as a swipe view's is.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use ljos_a11y::{NodeData, Range, Role};
use ljos_engine::{
    Cx, Engine, EngineError, ItemModel, MetaRef, ModelChange, ObjectId, OverrideContent,
    OverrideId, PropIndex, QmlObject, QmlType, RestoreMode, Value, WriteKind, qml_impl,
};
use ljos_quick::flickable::{WheelItem, register_wheel_item};
use ljos_quick::keys::{KeyItem, key, register_key_item};
use ljos_quick::{
    AccessibleAction, AccessibleItem, Flickable, Item, ItemView, KeyEvent, Loader, PathView,
    WheelEvent, child_items, register_accessible_item,
};
use ljos_render::kurbo;

use crate::control::{Control, focus_policy, takes_input};
use crate::rows;
use crate::util::{bind_default, check, is_a, num, object, on_model_made, set_computed};

/// `Tumbler.PositionMode`: the same numbers as a list view's.
mod position_mode {
    pub const BEGINNING: i64 = 0;
    pub const CENTER: i64 = 1;
    pub const END: i64 = 2;
    pub const VISIBLE: i64 = 3;
    pub const CONTAIN: i64 = 4;
    pub const SNAP_POSITION: i64 = 5;
}

fn tumbler_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![(
        "PositionMode",
        vec![
            ("Beginning", position_mode::BEGINNING),
            ("Center", position_mode::CENTER),
            ("End", position_mode::END),
            ("Visible", position_mode::VISIBLE),
            ("Contain", position_mode::CONTAIN),
            ("SnapPosition", position_mode::SNAP_POSITION),
        ],
    )]
}

/// `flickDeceleration` of the view a wrapping tumbler has, and of the one
/// that does not wrap (qml6).
const WRAPPED_DECELERATION: f64 = 100.0;
const LISTED_DECELERATION: f64 = 1500.0;

/// The size a delegate was held at.
#[derive(Debug, Default)]
struct Held {
    width: Option<OverrideId>,
    height: Option<OverrideId>,
}

#[derive(Default)]
pub(crate) struct TumblerState {
    model: RefCell<Option<Rc<dyn ItemModel>>>,
    subscription: Cell<u64>,
    /// The index that was a row last: what an index that is none goes
    /// back to.
    last: Cell<i32>,
    /// Set while the template writes the index itself.
    own: Cell<bool>,
    held: RefCell<HashMap<ObjectId, Held>>,
}

impl TumblerState {
    fn rows(&self) -> usize {
        self.model.borrow().as_ref().map_or(0, |m| m.row_count())
    }
}

impl Drop for TumblerState {
    fn drop(&mut self) {
        // The model may outlive the tumbler, and would go on telling it.
        if let Some(model) = self.model.get_mut().take() {
            model.unsubscribe(self.subscription.get());
        }
    }
}

impl std::fmt::Debug for TumblerState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TumblerState")
            .field("rows", &self.rows())
            .field("last", &self.last.get())
            .finish()
    }
}

/// `Tumbler`.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick.Templates",
    name = "Tumbler",
    extends = Control,
    attached = TumblerAttached,
    enums = tumbler_enums
)]
pub struct Tumbler {
    #[qml(base)]
    pub control: Control,
    #[qml(notify)]
    pub model: Value,
    #[qml(notify, readonly)]
    pub count: i32,
    #[qml(notify)]
    pub current_index: i32,
    #[qml(notify, readonly)]
    pub current_item: Option<ObjectId>,
    #[qml(notify, object = "Component")]
    pub delegate: Option<ObjectId>,
    #[qml(notify)]
    pub visible_item_count: i32,
    #[qml(notify, reset = ljos_engine::Reset::Follow(wraps_by_default))]
    pub wrap: bool,
    #[qml(notify, readonly)]
    pub moving: bool,
    #[qml(notify, reset = ljos_engine::Reset::Follow(deceleration_of_view))]
    pub flick_deceleration: f64,
    #[qml(skip)]
    pub(crate) state: Rc<TumblerState>,
}

impl Default for Tumbler {
    fn default() -> Self {
        Self {
            control: Control {
                focus_policy: focus_policy::TAB,
                ..Control::default()
            },
            model: Value::Undefined,
            count: 0,
            current_index: -1,
            current_item: None,
            delegate: None,
            visible_item_count: 5,
            wrap: true,
            moving: false,
            flick_deceleration: WRAPPED_DECELERATION,
            state: Rc::default(),
        }
    }
}

/// What `Tumbler.` attaches to a delegate: the tumbler it is in and how
/// far from the middle it is.
#[derive(Debug, Default, QmlObject)]
#[qml(name = "TumblerAttached")]
pub struct TumblerAttached {
    #[qml(notify, readonly)]
    pub tumbler: Option<ObjectId>,
    #[qml(notify, readonly)]
    pub displacement: f64,
}

/// A tumbler wraps when it has at least as many rows as it shows. Tracked.
fn wraps_by_default(e: &Engine, me: ObjectId, _: PropIndex) -> Value {
    let rows = e.read(me, Tumbler::COUNT.index).to_number();
    let shown = e.read(me, Tumbler::VISIBLE_ITEM_COUNT.index).to_number();
    Value::Bool(rows >= shown)
}

/// What the view that wraps, or the one that does not, flicks with.
fn deceleration_of_view(e: &Engine, me: ObjectId, _: PropIndex) -> Value {
    Value::Real(if e.read(me, Tumbler::WRAP.index).to_boolean() {
        WRAPPED_DECELERATION
    } else {
        LISTED_DECELERATION
    })
}

fn state_of(e: &Engine, me: ObjectId) -> Option<Rc<TumblerState>> {
    e.with::<Tumbler, _>(me, |t| t.state.clone()).ok()
}

fn int(e: &Engine, me: ObjectId, p: PropIndex) -> i32 {
    e.untrack(|| num(e, me, p)) as i32
}

/// The template's write: a binding on the property stays.
fn put(e: &Engine, me: ObjectId, p: PropIndex, v: Value) {
    if !e.is_alive(me) || e.untrack(|| e.read(me, p)).change_equals(&v) {
        return;
    }
    check(e, "Tumbler", e.write_derived(me, p, v));
}

/// The view that shows the rows: the content item, or what a `Loader`
/// there has loaded. Tracked.
fn view_of(e: &Engine, me: ObjectId) -> Option<ObjectId> {
    let content = object(e, me, Control::CONTENT_ITEM.index)?;
    let shown = if is_a::<Loader>(e, content) {
        object(e, content, Loader::ITEM.index)?
    } else {
        content
    };
    (is_a::<PathView>(e, shown) || is_a::<ItemView>(e, shown)).then_some(shown)
}

/// What the rows are children of.
fn home_of(e: &Engine, view: ObjectId) -> ObjectId {
    if is_a::<PathView>(e, view) {
        view
    } else {
        rows::home(e, view)
    }
}

#[qml_impl]
impl Tumbler {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, me) = (cx.engine, cx.this);
        // The reset follows these too, but only once it was asked for.
        for (p, f) in [
            (
                Tumbler::WRAP.index,
                wraps_by_default as fn(&Engine, ObjectId, PropIndex) -> Value,
            ),
            (Tumbler::FLICK_DECELERATION.index, deceleration_of_view),
        ] {
            bind_default(e, me, p, move |e| f(e, me, p))?;
        }
        adopt_model(e, me);
        e.watch(me, Tumbler::MODEL.index, move |e, _| adopt_model(e, me))?;
        on_model_made(e, me, Tumbler::MODEL.index, move |e| adopt_model(e, me))?;
        e.watch(me, Tumbler::CURRENT_INDEX.index, move |e, _| {
            index_set(e, me)
        })?;
        // What the view does comes to the tumbler: the current row, its
        // item, and whether the rows are moving.
        e.effect_in(me, move |e| follow(e, me))?;
        // Each delegate is the size of a row.
        e.effect_in(me, move |e| size_delegates(e, me))?;
        crate::a11y::defaults(e, me, Role::SpinBox, None)
    }

    /// Brings the row at `index` to `mode` of the view; the middle makes
    /// it the current one.
    #[qml(invokable, name = "positionViewAtIndex")]
    fn position_view_at_index(cx: &Cx, index: Value, mode: Value) {
        let (e, me) = (cx.engine, cx.this);
        let Some(view) = e.untrack(|| view_of(e, me)) else {
            return;
        };
        let r = e.invoke_by_name(view, "positionViewAtIndex", &[index, mode]);
        check(e, "positionViewAtIndex", r.map(|_| ()));
    }
}

// ----------------------------------------------------------- the model

/// Takes the rows of `model`, and the first one as current when the index
/// is not a row of them.
fn adopt_model(e: &Engine, me: ObjectId) {
    let Some(st) = state_of(e, me) else {
        return;
    };
    let value = e.untrack(|| e.read(me, Tumbler::MODEL.index));
    let model = e.untrack(|| e.model_from_value(&value, Some(me)));
    let id = model.subscribe(Rc::new(move |e: &Engine, _: &ModelChange| {
        rows_changed(e, me);
    }));
    if let Some(old) = st.model.replace(Some(model)) {
        old.unsubscribe(st.subscription.get());
    }
    st.subscription.set(id);
    rows_changed(e, me);
}

/// The count follows the model, and the index keeps to its rows.
fn rows_changed(e: &Engine, me: ObjectId) {
    if !e.is_alive(me) {
        return;
    }
    let Some(st) = state_of(e, me) else {
        return;
    };
    let n = st.rows() as i32;
    e.batch(|| {
        check(
            e,
            "count",
            set_computed(e, me, Tumbler::COUNT.index, Value::Int(n)),
        );
        let at = int(e, me, Tumbler::CURRENT_INDEX.index);
        let kept = if n == 0 {
            -1
        } else if at < 0 || at >= n {
            0
        } else {
            at
        };
        if kept != at {
            st.own.set(true);
            put(e, me, Tumbler::CURRENT_INDEX.index, Value::Int(kept));
            st.own.set(false);
        }
        st.last.set(kept);
    });
}

/// `currentIndex` was written: one that is no row is not taken.
fn index_set(e: &Engine, me: ObjectId) {
    let Some(st) = state_of(e, me) else {
        return;
    };
    if st.own.get() {
        return;
    }
    let n = st.rows() as i32;
    let at = int(e, me, Tumbler::CURRENT_INDEX.index);
    if n == 0 || (0..n).contains(&at) {
        st.last.set(at);
        return;
    }
    st.own.set(true);
    put(
        e,
        me,
        Tumbler::CURRENT_INDEX.index,
        Value::Int(st.last.get()),
    );
    st.own.set(false);
}

/// Moves the current row `by` rows; whether it moved.
fn step(e: &Engine, me: ObjectId, by: i32) -> bool {
    let Some(st) = state_of(e, me) else {
        return false;
    };
    let n = st.rows() as i32;
    if n == 0 {
        return false;
    }
    let at = int(e, me, Tumbler::CURRENT_INDEX.index);
    let wraps = e.untrack(|| e.read(me, Tumbler::WRAP.index).to_boolean());
    let to = if wraps {
        (at + by).rem_euclid(n)
    } else {
        (at + by).clamp(0, n - 1)
    };
    if to == at {
        return false;
    }
    put(e, me, Tumbler::CURRENT_INDEX.index, Value::Int(to));
    st.last.set(to);
    true
}

// ---------------------------------------------------------- the view

/// The view's current row, its item and its motion are the tumbler's.
///
/// A list view has no highlight range here, so the tumbler keeps the
/// current row in the middle itself: while the content is being moved the
/// row nearest the middle is the current one, and at rest the content goes
/// to the current row.
fn follow(e: &Engine, me: ObjectId) {
    let Some(view) = view_of(e, me) else {
        e.untrack(|| {
            check(
                e,
                "moving",
                set_computed(e, me, Tumbler::MOVING.index, Value::Bool(false)),
            );
        });
        return;
    };
    let listed = !is_a::<PathView>(e, view);
    let item = e.read(view, current_item_of(e, view)).as_object();
    let moving = e.read(view, moving_of(e, view)).to_boolean();
    let mut at = e.read(view, current_index_of(e, view)).to_number() as i32;
    if listed {
        let driven = moving || e.read(view, Flickable::DRAGGING.index).to_boolean();
        let content_y = num(e, view, Flickable::CONTENT_Y.index);
        let top = num(e, view, Flickable::TOP_MARGIN.index);
        let row = num(e, me, Control::AVAILABLE_HEIGHT.index)
            / e.read(me, Tumbler::VISIBLE_ITEM_COUNT.index)
                .to_number()
                .max(1.0);
        let n = e.read(me, Tumbler::COUNT.index).to_number() as i32;
        if row > 0.0 && n > 0 {
            if driven {
                at = (((content_y + top) / row).round() as i32).clamp(0, n - 1);
            } else {
                let want = e.read(me, Tumbler::CURRENT_INDEX.index).to_number() as i32;
                let target = f64::from(want) * row - top;
                if (0..n).contains(&want) && (content_y - target).abs() > 0.5 {
                    e.untrack(|| go_to(e, view, want));
                }
            }
        }
    }
    e.untrack(|| {
        e.batch(|| {
            let st = state_of(e, me);
            let n = st.as_ref().map_or(0, |s| s.rows()) as i32;
            // A view that has not the rows yet says nothing of them.
            if let Some(st) = &st
                && (0..n).contains(&at)
                && at != int(e, me, Tumbler::CURRENT_INDEX.index)
            {
                st.own.set(true);
                put(e, me, Tumbler::CURRENT_INDEX.index, Value::Int(at));
                st.own.set(false);
                st.last.set(at);
            }
            let item = item.filter(|i| e.is_alive(*i));
            check(
                e,
                "currentItem",
                set_computed(
                    e,
                    me,
                    Tumbler::CURRENT_ITEM.index,
                    item.map_or(Value::Null, Value::Object),
                ),
            );
            check(
                e,
                "moving",
                set_computed(e, me, Tumbler::MOVING.index, Value::Bool(moving)),
            );
        });
    });
}

/// Brings the row at `index` to the middle of the view.
fn go_to(e: &Engine, view: ObjectId, index: i32) {
    let r = e.invoke_by_name(
        view,
        "positionViewAtIndex",
        &[Value::Int(index), Value::Int(position_mode::CENTER as i32)],
    );
    check(e, "Tumbler", r.map(|_| ()));
}

fn current_index_of(e: &Engine, view: ObjectId) -> PropIndex {
    if is_a::<PathView>(e, view) {
        PathView::CURRENT_INDEX.index
    } else {
        ItemView::CURRENT_INDEX.index
    }
}

fn current_item_of(e: &Engine, view: ObjectId) -> PropIndex {
    if is_a::<PathView>(e, view) {
        PathView::CURRENT_ITEM.index
    } else {
        ItemView::CURRENT_ITEM.index
    }
}

fn moving_of(e: &Engine, view: ObjectId) -> PropIndex {
    if is_a::<PathView>(e, view) {
        PathView::MOVING.index
    } else {
        Flickable::MOVING.index
    }
}

// ----------------------------------------------------------- the rows

/// Every delegate is as wide as the content area and one row tall, and
/// what was held on a delegate that left goes.
fn size_delegates(e: &Engine, me: ObjectId) {
    let Some(st) = state_of(e, me) else {
        return;
    };
    let w = num(e, me, Control::AVAILABLE_WIDTH.index);
    let h = num(e, me, Control::AVAILABLE_HEIGHT.index);
    let shown = e
        .read(me, Tumbler::VISIBLE_ITEM_COUNT.index)
        .to_number()
        .max(1.0);
    let row = h / shown;
    let items = match view_of(e, me) {
        Some(view) => child_items(e, home_of(e, view)),
        None => Vec::new(),
    };
    e.untrack(|| {
        e.batch(|| {
            let gone: Vec<(ObjectId, Held)> = {
                let mut held = st.held.borrow_mut();
                let keys: Vec<ObjectId> = held.keys().copied().collect();
                keys.into_iter()
                    .filter(|k| !items.contains(k))
                    .filter_map(|k| held.remove(&k).map(|h| (k, h)))
                    .collect()
            };
            for (item, held) in gone {
                release(e, item, held);
            }
            for item in items {
                let mut table = st.held.borrow_mut();
                let held = table.entry(item).or_default();
                for (layer, prop, v) in [
                    (&mut held.width, Item::WIDTH.index, w),
                    (&mut held.height, Item::HEIGHT.index, row),
                ] {
                    let r = match *layer {
                        Some(id) => {
                            if e.read(item, prop).to_number() == v {
                                Ok(())
                            } else {
                                e.write_internal(
                                    item,
                                    prop,
                                    Value::Real(v),
                                    WriteKind::Override(id),
                                )
                            }
                        }
                        None => e
                            .push_override(item, prop, OverrideContent::Value(Value::Real(v)))
                            .map(|id| *layer = Some(id)),
                    };
                    check(e, "Tumbler", r);
                }
            }
        });
    });
}

fn release(e: &Engine, item: ObjectId, held: Held) {
    if !e.is_alive(item) {
        return;
    }
    for (prop, layer) in [
        (Item::WIDTH.index, held.width),
        (Item::HEIGHT.index, held.height),
    ] {
        if let Some(layer) = layer {
            let _ = e.remove_override(item, prop, layer, RestoreMode::None);
        }
    }
}

/// The tumbler a delegate is in, found by its parents. Tracked.
fn tumbler_above(e: &Engine, item: ObjectId) -> Option<ObjectId> {
    let mut cur = e.read(item, Item::PARENT.index).as_object()?;
    // A tree deeper than this is a cycle someone built by hand.
    for _ in 0..4096 {
        if !e.is_alive(cur) {
            return None;
        }
        if is_a::<Tumbler>(e, cur) {
            return Some(cur);
        }
        cur = e.read(cur, Item::PARENT.index).as_object()?;
    }
    None
}

/// How many rows `item` is above the middle of the tumbler's content.
/// Tracked: it follows the item as it moves.
fn displacement(e: &Engine, item: ObjectId, tumbler: ObjectId) -> f64 {
    let Some(content) = object(e, tumbler, Control::CONTENT_ITEM.index) else {
        return 0.0;
    };
    // Where the item is in the content item, through what it is in.
    let mut y = num(e, item, Item::Y.index) + num(e, item, Item::HEIGHT.index) / 2.0;
    let mut cur = e.read(item, Item::PARENT.index).as_object();
    for _ in 0..4096 {
        let Some(p) = cur.filter(|p| e.is_alive(*p)) else {
            return 0.0;
        };
        if p == content {
            break;
        }
        y += num(e, p, Item::Y.index);
        cur = e.read(p, Item::PARENT.index).as_object();
    }
    let height = num(e, item, Item::HEIGHT.index);
    if height <= 0.0 {
        return 0.0;
    }
    (num(e, content, Item::HEIGHT.index) / 2.0 - y) / height
}

// ------------------------------------------------------- keys and wheel

fn key_press(e: &Engine, me: ObjectId, ev: &KeyEvent) -> bool {
    if !takes_input(e, me) {
        return false;
    }
    match ev.key {
        key::UP => {
            step(e, me, -1);
            true
        }
        key::DOWN => {
            step(e, me, 1);
            true
        }
        _ => false,
    }
}

fn no_key(_: &Engine, _: ObjectId, _: &KeyEvent) -> bool {
    false
}

fn wheel(e: &Engine, me: ObjectId, ev: &WheelEvent, _p: kurbo::Point) -> bool {
    if !takes_input(e, me) {
        return false;
    }
    let turned = ev.angle_delta.y;
    if turned == 0.0 {
        return false;
    }
    let notches = ((turned.abs() / WheelEvent::NOTCH).trunc() as i32).max(1);
    // Down, away from the user, is the next row, as the keys are.
    step(e, me, if turned < 0.0 { notches } else { -notches });
    true
}

// ------------------------------------------------- assistive technology

fn describe(e: &Engine, me: ObjectId, data: &mut NodeData) {
    // Tracked: the node follows the index and the rows.
    let n = num(e, me, Tumbler::COUNT.index);
    data.range = Some(Range {
        value: num(e, me, Tumbler::CURRENT_INDEX.index),
        min: 0.0,
        max: (n - 1.0).max(0.0),
        step: 1.0,
    });
}

fn act(e: &Engine, me: ObjectId, action: AccessibleAction) -> bool {
    if !takes_input(e, me) {
        return false;
    }
    match action {
        AccessibleAction::Increase => step(e, me, 1),
        AccessibleAction::Decrease => step(e, me, -1),
        AccessibleAction::SetValue(v) if v.is_finite() => {
            let at = int(e, me, Tumbler::CURRENT_INDEX.index);
            step(e, me, v.round() as i32 - at)
        }
        _ => false,
    }
}

pub(crate) fn register(engine: &Engine) {
    let meta = || MetaRef::Static(Tumbler::static_meta());
    register_key_item(
        engine,
        meta(),
        KeyItem {
            shortcut_override: no_key,
            press: key_press,
            release: no_key,
        },
    );
    register_wheel_item(engine, meta(), WheelItem { wheel });
    register_accessible_item(engine, meta(), AccessibleItem { describe, act });
    // A delegate that reads `Tumbler.` is told what it is in at once, and
    // follows the rows after.
    engine.on_attached_created(|e, item, att| {
        if !is_a::<TumblerAttached>(e, att) {
            return true;
        }
        let found = move |e: &Engine| tumbler_above(e, item);
        let r = e
            .bind_initial(att, TumblerAttached::TUMBLER.index, move |e| {
                found(e).map_or(Value::Null, Value::Object)
            })
            .and_then(|()| {
                e.bind_initial(att, TumblerAttached::DISPLACEMENT.index, move |e| {
                    Value::Real(found(e).map_or(0.0, |t| displacement(e, item, t)))
                })
            });
        check(e, "Tumbler", r);
        true
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_modes_are_those_of_a_list() {
        let modes = tumbler_enums().remove(0).1;
        let by = |n: &str| modes.iter().find(|(m, _)| *m == n).map(|(_, v)| *v);
        assert_eq!(by("Beginning"), Some(0));
        assert_eq!(by("Center"), Some(1));
        assert_eq!(by("SnapPosition"), Some(5));
    }
}
