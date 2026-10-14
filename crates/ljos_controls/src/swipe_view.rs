//! `SwipeView` (QtQuick.Templates): a container that shows one item at a
//! time, as large as the view, and moves to the next or the one before by
//! a swipe.
//!
//! The Basic style's content item is a `ListView` on `contentModel` that
//! snaps to one item; the view's current row and the container's current
//! index are the same (`container.rs` follows the view, the style binds
//! the view). A swipe that comes to rest on an item makes it current.
//!
//! Documentation: doc.qt.io/qt-6/qml-qtquick-controls-swipeview.html.
//! What the items read of `SwipeView.` was read off a running Qt 6.11.2;
//! `features/controls/swipe-view.feature` pins it.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use ljos_a11y::Role;
use ljos_engine::{
    ConnectionId, Cx, Engine, EngineError, MetaRef, ObjectId, OverrideContent, OverrideId,
    PropIndex, QmlObject, QmlType, RestoreMode, Value, WriteKind, qml_impl,
};
use ljos_quick::{Flickable, Item, ItemView};

use crate::container::{self, Container};
use crate::control::Control;
use crate::util::{check, is_a, num, object, set_computed};

/// `Qt.Horizontal`.
const HORIZONTAL: i32 = 1;
/// `Qt.Vertical`.
const VERTICAL: i32 = 2;

#[derive(Debug, Default)]
struct Held {
    width: Option<OverrideId>,
    height: Option<OverrideId>,
}

#[derive(Debug, Default)]
pub(crate) struct SwipeViewState {
    held: RefCell<HashMap<ObjectId, Held>>,
    /// The view whose resting place is followed, and the connections.
    view: RefCell<Option<(ObjectId, Vec<ConnectionId>)>>,
    /// Where the swipe that just ended goes: the item a swipe was let go
    /// towards, once the view comes to rest.
    swiped_to: Cell<Option<i32>>,
}

/// How far a swipe has to move the content past the current item before
/// it goes to the next one rather than back (read off qml6: 30 is not
/// far enough, 31 is).
const SWIPE_THRESHOLD: f64 = 30.0;

/// `SwipeView`.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick.Templates",
    name = "SwipeView",
    extends = Container,
    attached = SwipeViewAttached
)]
pub struct SwipeView {
    #[qml(base)]
    pub container: Container,
    #[qml(notify)]
    pub interactive: bool,
    /// `Qt.Horizontal` or `Qt.Vertical`.
    #[qml(notify, enumeration = "Qt.Orientation")]
    pub orientation: i32,
    #[qml(notify, readonly)]
    pub horizontal: bool,
    #[qml(notify, readonly)]
    pub vertical: bool,
    #[qml(skip)]
    pub(crate) state: Rc<SwipeViewState>,
}

impl Default for SwipeView {
    fn default() -> Self {
        let mut container = Container::default();
        // Tab stops at a swipe view (qml6).
        container.control.focus_policy = crate::control::focus_policy::TAB;
        Self {
            container,
            interactive: true,
            orientation: HORIZONTAL,
            horizontal: true,
            vertical: false,
            state: Rc::default(),
        }
    }
}

/// What `SwipeView.index` and its neighbours read on an item of a view.
#[derive(Debug, QmlObject)]
#[qml(name = "SwipeViewAttached")]
pub struct SwipeViewAttached {
    #[qml(notify, readonly)]
    pub index: i32,
    #[qml(notify, readonly)]
    pub is_current_item: bool,
    #[qml(notify, readonly)]
    pub is_next_item: bool,
    #[qml(notify, readonly)]
    pub is_previous_item: bool,
    #[qml(notify, readonly)]
    pub view: Option<ObjectId>,
}

impl Default for SwipeViewAttached {
    fn default() -> Self {
        Self {
            index: -1,
            is_current_item: false,
            is_next_item: false,
            is_previous_item: false,
            view: None,
        }
    }
}

#[qml_impl]
impl SwipeView {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, me) = (cx.engine, cx.this);
        e.effect_in(me, move |e| {
            let o = e.read(me, SwipeView::ORIENTATION.index).to_number() as i32;
            let r = set_computed(
                e,
                me,
                SwipeView::HORIZONTAL.index,
                Value::Bool(o != VERTICAL),
            )
            .and_then(|()| {
                set_computed(e, me, SwipeView::VERTICAL.index, Value::Bool(o == VERTICAL))
            });
            check(e, "SwipeView", r);
        })?;
        e.effect_in(me, move |e| check(e, "SwipeView", layout(e, me)))?;
        e.watch(me, Control::CONTENT_ITEM.index, move |e, _| follow(e, me))?;
        follow(e, me);
        items_changed(e, me);
        current_changed(e, me);
        crate::a11y::defaults(e, me, Role::PageTabList, None)
    }
}

fn state_of(engine: &Engine, me: ObjectId) -> Option<Rc<SwipeViewState>> {
    engine.with::<SwipeView, _>(me, |v| v.state.clone()).ok()
}

fn attacher() -> MetaRef {
    MetaRef::Static(SwipeView::static_meta())
}

/// Every item is as large as the view's content area, unless its author
/// gave it a size.
fn layout(e: &Engine, me: ObjectId) -> Result<(), EngineError> {
    let Some(st) = state_of(e, me) else {
        return Ok(());
    };
    let _ = e.read(me, Container::COUNT.index);
    let w = num(e, me, Control::AVAILABLE_WIDTH.index);
    let h = num(e, me, Control::AVAILABLE_HEIGHT.index);
    e.untrack(|| {
        e.batch(|| {
            for item in container::items(e, me) {
                let (lw, lh) = {
                    let held = st.held.borrow();
                    let Some(x) = held.get(&item) else { continue };
                    (x.width, x.height)
                };
                for (layer, prop, v) in [(lw, Item::WIDTH.index, w), (lh, Item::HEIGHT.index, h)] {
                    if let Some(layer) = layer
                        && e.untrack(|| e.read(item, prop)).to_number() != v
                    {
                        e.write_internal(item, prop, Value::Real(v), WriteKind::Override(layer))?;
                    }
                }
            }
            Ok(())
        })
    })
}

/// Every page is the size of the view, even one the author gave a size
/// (qml6 does the same; the documentation says not to).
fn hold(
    e: &Engine,
    me: ObjectId,
    item: ObjectId,
    prop: PropIndex,
) -> Result<Option<OverrideId>, EngineError> {
    let room = if prop == Item::WIDTH.index {
        Control::AVAILABLE_WIDTH.index
    } else {
        Control::AVAILABLE_HEIGHT.index
    };
    let now = e.untrack(|| num(e, me, room));
    e.push_override(item, prop, OverrideContent::Value(Value::Real(now)))
        .map(Some)
}

/// The items changed: each knows its place, new ones are sized, and what
/// was held on the ones that left goes.
pub(crate) fn items_changed(e: &Engine, me: ObjectId) {
    let Some(st) = state_of(e, me) else {
        return;
    };
    let items = container::items(e, me);
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
    for item in &items {
        if st.held.borrow().contains_key(item) {
            continue;
        }
        let r = (|| -> Result<Held, EngineError> {
            Ok(Held {
                width: hold(e, me, *item, Item::WIDTH.index)?,
                height: hold(e, me, *item, Item::HEIGHT.index)?,
            })
        })();
        match r {
            Ok(held) => {
                st.held.borrow_mut().insert(*item, held);
            }
            Err(err) => check(e, "SwipeView", Err(err)),
        }
    }
    current_changed(e, me);
}

fn release(e: &Engine, item: ObjectId, held: Held) {
    if !e.is_alive(item) {
        return;
    }
    check(e, "SwipeView", e.set_frozen(item, false));
    for (prop, layer) in [
        (Item::WIDTH.index, held.width),
        (Item::HEIGHT.index, held.height),
    ] {
        if let Some(layer) = layer {
            // A page taken out keeps the size it had in the view (qml6).
            let _ = e.remove_override(item, prop, layer, RestoreMode::None);
        }
    }
    if let Some(att) = e.existing_attached_object(item, &attacher()) {
        let r = (|| {
            set_computed(e, att, SwipeViewAttached::INDEX.index, Value::Int(-1))?;
            set_computed(e, att, SwipeViewAttached::VIEW.index, Value::Null)?;
            for p in [
                SwipeViewAttached::IS_CURRENT_ITEM.index,
                SwipeViewAttached::IS_NEXT_ITEM.index,
                SwipeViewAttached::IS_PREVIOUS_ITEM.index,
            ] {
                set_computed(e, att, p, Value::Bool(false))?;
            }
            Ok(())
        })();
        check(e, "SwipeView", r);
    }
}

/// Whether the content view is being dragged or moving between items.
fn is_moving(e: &Engine, me: ObjectId) -> bool {
    let Some(ci) = e
        .untrack(|| object(e, me, Control::CONTENT_ITEM.index))
        .filter(|c| is_a::<ItemView>(e, *c))
    else {
        return false;
    };
    e.untrack(|| {
        e.read(ci, Flickable::MOVING.index).to_boolean()
            || e.read(ci, Flickable::DRAGGING.index).to_boolean()
    })
}

/// Freezes pages that are not the current one while the view is at rest,
/// and thaws them while the view is moving (ARCHITECTURE.md §8.3).
fn sync_frozen(e: &Engine, me: ObjectId) {
    if !is_a::<SwipeView>(e, me) {
        return;
    }
    let cur = container::current_index(e, me);
    let moving = is_moving(e, me);
    let items = container::items(e, me);
    for (i, &item) in items.iter().enumerate() {
        if moving || i as i32 == cur {
            check(e, "SwipeView", e.set_frozen(item, false));
        }
    }
    if !moving {
        for (i, item) in items.into_iter().enumerate() {
            if i as i32 != cur {
                check(e, "SwipeView", e.set_frozen(item, true));
            }
        }
    }
}

/// What each item reads of `SwipeView.` follows the current index.
pub(crate) fn current_changed(e: &Engine, me: ObjectId) {
    if !is_a::<SwipeView>(e, me) {
        return;
    }
    let cur = container::current_index(e, me);
    let moving = is_moving(e, me);
    let items = container::items(e, me);
    for (i, &item) in items.iter().enumerate() {
        if moving || i as i32 == cur {
            check(e, "SwipeView", e.set_frozen(item, false));
        }
    }
    for (i, item) in items.into_iter().enumerate() {
        let i = i as i32;
        let r = e.attached_object(item, &attacher()).and_then(|att| {
            set_computed(e, att, SwipeViewAttached::INDEX.index, Value::Int(i))?;
            set_computed(e, att, SwipeViewAttached::VIEW.index, Value::Object(me))?;
            set_computed(
                e,
                att,
                SwipeViewAttached::IS_CURRENT_ITEM.index,
                Value::Bool(i == cur),
            )?;
            set_computed(
                e,
                att,
                SwipeViewAttached::IS_NEXT_ITEM.index,
                Value::Bool(cur >= 0 && i == cur + 1),
            )?;
            set_computed(
                e,
                att,
                SwipeViewAttached::IS_PREVIOUS_ITEM.index,
                Value::Bool(i == cur - 1),
            )
        });
        check(e, "SwipeView", r);
        if !moving && i != cur {
            check(e, "SwipeView", e.set_frozen(item, true));
        }
    }
}

/// A swipe that comes to rest makes the item it rests on current: the
/// view keeps its current row where it is shown.
fn follow(e: &Engine, me: ObjectId) {
    let Some(st) = state_of(e, me) else {
        return;
    };
    if let Some((_, conns)) = st.view.borrow_mut().take() {
        for conn in conns {
            e.disconnect(conn);
        }
    }
    let Some(ci) = e
        .untrack(|| object(e, me, Control::CONTENT_ITEM.index))
        .filter(|c| is_a::<ItemView>(e, *c))
    else {
        return;
    };
    let r = (|| -> Result<Vec<ConnectionId>, EngineError> {
        let a = e.connect_by_name(ci, "movementStarted", move |e, _| sync_frozen(e, me))?;
        let b = e.connect_by_name(ci, "dragEnded", move |e, _| let_go(e, me, ci))?;
        let c = e.connect_by_name(ci, "movementEnded", move |e, _| rested(e, me, ci))?;
        Ok(vec![a, b, c])
    })();
    match r {
        Ok(conns) => *st.view.borrow_mut() = Some((ci, conns)),
        Err(err) => check(e, "SwipeView", Err(err)),
    }
}

/// Where the content is along the view's axis, and how far apart the
/// items start.
fn along(e: &Engine, me: ObjectId, view: ObjectId) -> (f64, f64) {
    e.untrack(|| {
        let vertical = num(e, me, SwipeView::ORIENTATION.index) as i32 == VERTICAL;
        let (pos, size) = if vertical {
            (
                num(e, view, Flickable::CONTENT_Y.index),
                num(e, view, Item::HEIGHT.index),
            )
        } else {
            (
                num(e, view, Flickable::CONTENT_X.index),
                num(e, view, Item::WIDTH.index),
            )
        };
        (pos, size + num(e, me, Control::SPACING.index))
    })
}

/// A swipe was let go: one that moved the content far enough from the
/// current item goes one item that way, whatever the speed or the
/// distance; one that did not goes back.
fn let_go(e: &Engine, me: ObjectId, view: ObjectId) {
    let Some(st) = state_of(e, me) else {
        return;
    };
    let n = container::items(e, me).len() as i32;
    let (pos, step) = along(e, me, view);
    if n == 0 || step <= 0.0 {
        return;
    }
    let cur = container::current_index(e, me).max(0);
    let moved = pos - f64::from(cur) * step;
    let to = if moved > SWIPE_THRESHOLD {
        cur + 1
    } else if moved < -SWIPE_THRESHOLD {
        cur - 1
    } else {
        cur
    };
    st.swiped_to.set(Some(to.clamp(0, n - 1)));
}

fn rested(e: &Engine, me: ObjectId, view: ObjectId) {
    if !e.is_alive(me) || !e.is_alive(view) {
        return;
    }
    let n = container::items(e, me).len();
    if n == 0 {
        return;
    }
    let swiped = state_of(e, me).and_then(|st| st.swiped_to.take());
    let at = swiped.unwrap_or_else(|| {
        let (pos, step) = along(e, me, view);
        if step <= 0.0 {
            return container::current_index(e, me);
        }
        ((pos / step).round().max(0.0) as usize).min(n - 1) as i32
    });
    if at != container::current_index(e, me) {
        container::set_current(e, me, at);
    } else if swiped.is_some() {
        // The view snapped to the nearest item, which need not be this one.
        go_to(e, view, at);
    }
    sync_frozen(e, me);
}

/// Moves the view to the item at `index`.
fn go_to(e: &Engine, view: ObjectId, index: i32) {
    let r = e.invoke_by_name(
        view,
        "positionViewAtIndex",
        &[Value::Int(index), Value::Int(0)],
    );
    check(e, "SwipeView", r.map(|_| ()));
}
