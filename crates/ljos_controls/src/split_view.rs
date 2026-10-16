//! `SplitView` (QtQuick.Templates): a container that lays its items out
//! side by side, with a handle between each two that the user drags to
//! give one more room and its neighbour less.
//!
//! Each item is as long as it prefers (`SplitView.preferredWidth`, or its
//! implicit size), kept between its minimum and its maximum. One item
//! fills what is left: the first that says `SplitView.fillWidth` (or
//! `fillHeight`), or else the last. A handle is made from the `handle`
//! component for every item but the last, as a child of the view, and
//! dragging it sets the preferred size of the item on the side away from
//! the one that fills.
//!
//! `saveState()` gives the preferred sizes that were set, by index, and
//! `restoreState()` puts them back. Qt's state is an `ArrayBuffer`; Ljós
//! has no such value, so it is a string here, as opaque.
//!
//! Documentation: doc.qt.io/qt-6/qml-qtquick-controls-splitview.html.
//! The sizes, the handle and what the items read of `SplitView.` were read
//! off a running Qt 6.11.2; `features/controls/split-view.feature` pins
//! them.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use ljos_engine::{
    Cx, Engine, EngineError, MetaRef, ObjectId, QmlObject, QmlType, Value, qml_impl,
};
use ljos_quick::{
    Item, PointerEvent, PointerItem, register_hover_move, register_pointer_item, set_parent_item,
};
use ljos_render::kurbo;

use crate::container::{self, Container};
use crate::control::{Control, hover, is_left, takes_input};
use crate::util::{check, flag, is_a, num, object, set_computed, warn};

/// `Qt.Horizontal`.
const HORIZONTAL: i32 = 1;
/// `Qt.Vertical`.
const VERTICAL: i32 = 2;

/// What a split view keeps that QML does not see.
#[derive(Debug, Default)]
pub(crate) struct SplitState {
    /// The handle after each item but the last, in order.
    handles: RefCell<Vec<ObjectId>>,
    /// The handle the pointer drags, if any.
    drag: Cell<Option<Drag>>,
    /// The handle the pointer is over.
    over: Cell<Option<ObjectId>>,
}

/// A drag of a handle: which item it resizes, from where.
#[derive(Clone, Copy, Debug)]
struct Drag {
    handle: ObjectId,
    item: ObjectId,
    /// Where the press was along the view.
    from: f64,
    /// How long the item was then.
    size: f64,
    /// +1 when the item is before the handle (dragging away from it makes
    /// it longer), -1 when it is after.
    sign: f64,
}

/// `SplitView`.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick.Templates",
    name = "SplitView",
    extends = Container,
    attached = SplitViewAttached
)]
pub struct SplitView {
    #[qml(base)]
    pub container: Container,
    /// `Qt.Horizontal` or `Qt.Vertical`.
    #[qml(notify, enumeration = "Qt.Orientation")]
    pub orientation: i32,
    /// Whether the user is dragging a handle.
    #[qml(notify, readonly)]
    pub resizing: bool,
    /// What each handle is made from.
    #[qml(notify, object = "Component")]
    pub handle: Option<ObjectId>,
    #[qml(skip)]
    pub(crate) state: Rc<SplitState>,
}

impl Default for SplitView {
    fn default() -> Self {
        Self {
            container: Container::default(),
            orientation: HORIZONTAL,
            resizing: false,
            handle: None,
            state: Rc::default(),
        }
    }
}

/// What `SplitView.` reads and sets on an item of a view.
#[derive(Debug, QmlObject)]
#[qml(name = "SplitViewAttached")]
pub struct SplitViewAttached {
    #[qml(notify, readonly)]
    pub view: Option<ObjectId>,
    #[qml(notify, reset)]
    pub minimum_width: f64,
    #[qml(notify, reset)]
    pub minimum_height: f64,
    /// -1: as long as the item's implicit size.
    #[qml(notify, reset)]
    pub preferred_width: f64,
    #[qml(notify, reset)]
    pub preferred_height: f64,
    #[qml(notify, reset)]
    pub maximum_width: f64,
    #[qml(notify, reset)]
    pub maximum_height: f64,
    #[qml(notify)]
    pub fill_width: bool,
    #[qml(notify)]
    pub fill_height: bool,
}

impl Default for SplitViewAttached {
    fn default() -> Self {
        Self {
            view: None,
            minimum_width: 0.0,
            minimum_height: 0.0,
            preferred_width: -1.0,
            preferred_height: -1.0,
            maximum_width: f64::INFINITY,
            maximum_height: f64::INFINITY,
            fill_width: false,
            fill_height: false,
        }
    }
}

/// `SplitHandle`: only what it attaches to a handle.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtQuick.Templates",
    name = "SplitHandle",
    attached = SplitHandleAttached,
    uncreatable
)]
pub struct SplitHandle {}

/// What `SplitHandle.` reads on a handle.
#[derive(Debug, Default, QmlObject)]
#[qml(name = "SplitHandleAttached")]
pub struct SplitHandleAttached {
    #[qml(notify, readonly)]
    pub hovered: bool,
    #[qml(notify, readonly)]
    pub pressed: bool,
}

#[qml_impl]
impl SplitView {
    /// The preferred sizes that were set, by index.
    #[qml(invokable, name = "saveState")]
    fn save_state(cx: &Cx) -> Value {
        let (e, me) = (cx.engine, cx.this);
        let mut kept = Vec::new();
        for (i, item) in container::items(e, me).into_iter().enumerate() {
            let Some(att) = e.existing_attached_object(item, &attacher()) else {
                continue;
            };
            let (w, h) = e.untrack(|| {
                (
                    num(e, att, SplitViewAttached::PREFERRED_WIDTH.index),
                    num(e, att, SplitViewAttached::PREFERRED_HEIGHT.index),
                )
            });
            if w >= 0.0 || h >= 0.0 {
                kept.push(Kept {
                    index: i,
                    width: w,
                    height: h,
                });
            }
        }
        Value::String(encode(&kept).into())
    }

    /// Puts back the preferred sizes `saveState()` gave. False, with a
    /// warning, for anything else.
    #[qml(invokable, name = "restoreState")]
    fn restore_state(cx: &Cx, state: Value) -> Value {
        let (e, me) = (cx.engine, cx.this);
        let decoded = match &state {
            Value::String(s) => decode(s),
            _ => None,
        };
        let Some(kept) = decoded else {
            warn(
                e,
                "Error reading SplitView state: not a state saveState() gave",
            );
            return Value::Bool(false);
        };
        let items = container::items(e, me);
        for k in kept {
            let Some(item) = items.get(k.index) else {
                continue;
            };
            let r = e.attached_object(*item, &attacher()).and_then(|att| {
                set_computed(
                    e,
                    att,
                    SplitViewAttached::PREFERRED_WIDTH.index,
                    Value::Real(k.width),
                )?;
                set_computed(
                    e,
                    att,
                    SplitViewAttached::PREFERRED_HEIGHT.index,
                    Value::Real(k.height),
                )
            });
            check(e, "SplitView", r);
        }
        Value::Bool(true)
    }

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, me) = (cx.engine, cx.this);
        if e.untrack(|| object(e, me, Control::CONTENT_ITEM.index))
            .is_none()
        {
            // The template lays its items out in an item of its own.
            let ci = e.create_native_incomplete(Item::default(), Some(me))?;
            e.complete(ci)?;
            crate::popup::put(e, me, Control::CONTENT_ITEM.index, Value::Object(ci))?;
        }
        e.watch(me, SplitView::HANDLE.index, move |e, _| {
            check(e, "SplitView", remake_handles(e, me));
        })?;
        items_changed(e, me);
        e.effect_in(me, move |e| check(e, "SplitView", layout(e, me)))?;
        Ok(())
    }
}

fn state_of(engine: &Engine, me: ObjectId) -> Option<Rc<SplitState>> {
    engine.with::<SplitView, _>(me, |v| v.state.clone()).ok()
}

fn attacher() -> MetaRef {
    MetaRef::Static(SplitView::static_meta())
}

fn handle_attacher() -> MetaRef {
    MetaRef::Static(SplitHandle::static_meta())
}

fn is_vertical(engine: &Engine, me: ObjectId) -> bool {
    num(engine, me, SplitView::ORIENTATION.index) as i32 == VERTICAL
}

// ---------------------------------------------------------------- layout

/// What an item asks for along the view.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Ask {
    implicit: f64,
    minimum: f64,
    /// Negative: none, the implicit size is preferred.
    preferred: f64,
    maximum: f64,
    fill: bool,
}

impl Default for Ask {
    fn default() -> Self {
        Self {
            implicit: 0.0,
            minimum: 0.0,
            preferred: -1.0,
            maximum: f64::INFINITY,
            fill: false,
        }
    }
}

impl Ask {
    fn clamp(&self, v: f64) -> f64 {
        v.min(self.maximum).max(self.minimum)
    }

    fn wanted(&self) -> f64 {
        self.clamp(if self.preferred >= 0.0 {
            self.preferred
        } else {
            self.implicit
        })
    }
}

/// The item that takes what is left: the first that fills, or the last.
fn filler(asks: &[Ask]) -> Option<usize> {
    asks.iter()
        .position(|a| a.fill)
        .or_else(|| asks.len().checked_sub(1))
}

/// Where each item starts along the view and how long it is, with `room`
/// to share and handles `handle` long between them.
fn lay_out(asks: &[Ask], room: f64, handle: f64) -> Vec<(f64, f64)> {
    let fill = filler(asks);
    let handles = handle * asks.len().saturating_sub(1) as f64;
    let others: f64 = asks
        .iter()
        .enumerate()
        .filter(|(i, _)| Some(*i) != fill)
        .map(|(_, a)| a.wanted())
        .sum();
    let mut at = 0.0;
    asks.iter()
        .enumerate()
        .map(|(i, a)| {
            let size = if Some(i) == fill {
                a.clamp((room - others - handles).max(0.0))
            } else {
                a.wanted()
            };
            let start = at;
            at += size + handle;
            (start, size)
        })
        .collect()
}

/// What `item` asks for along the view (tracked).
fn ask_of(e: &Engine, item: ObjectId, vertical: bool) -> Ask {
    let implicit = if vertical {
        Item::IMPLICIT_HEIGHT
    } else {
        Item::IMPLICIT_WIDTH
    };
    let mut ask = Ask {
        implicit: num(e, item, implicit.index),
        ..Ask::default()
    };
    if let Some(att) = e.existing_attached_object(item, &attacher()) {
        let ([min, pref, max], fill) = if vertical {
            (
                [
                    SplitViewAttached::MINIMUM_HEIGHT,
                    SplitViewAttached::PREFERRED_HEIGHT,
                    SplitViewAttached::MAXIMUM_HEIGHT,
                ]
                .map(|p| p.index),
                SplitViewAttached::FILL_HEIGHT.index,
            )
        } else {
            (
                [
                    SplitViewAttached::MINIMUM_WIDTH,
                    SplitViewAttached::PREFERRED_WIDTH,
                    SplitViewAttached::MAXIMUM_WIDTH,
                ]
                .map(|p| p.index),
                SplitViewAttached::FILL_WIDTH.index,
            )
        };
        ask.minimum = num(e, att, min);
        ask.preferred = num(e, att, pref);
        ask.maximum = num(e, att, max);
        ask.fill = flag(e, att, fill);
    }
    ask
}

/// How long a handle is along the view (tracked).
fn handle_length(e: &Engine, handle: ObjectId, vertical: bool) -> f64 {
    let p = if vertical {
        Item::IMPLICIT_HEIGHT
    } else {
        Item::IMPLICIT_WIDTH
    };
    num(e, handle, p.index)
}

/// Puts each item and each handle where the asks say. Everything it reads
/// is a dependency.
fn layout(e: &Engine, me: ObjectId) -> Result<(), EngineError> {
    let Some(st) = state_of(e, me) else {
        return Ok(());
    };
    let _ = e.read(me, Container::COUNT.index);
    let vertical = is_vertical(e, me);
    let items = container::items(e, me);
    let handles = st.handles.borrow().clone();
    let asks: Vec<Ask> = items.iter().map(|i| ask_of(e, *i, vertical)).collect();
    let handle = handles
        .first()
        .map_or(0.0, |h| handle_length(e, *h, vertical));
    let (w, h) = (
        num(e, me, Control::AVAILABLE_WIDTH.index),
        num(e, me, Control::AVAILABLE_HEIGHT.index),
    );
    let (left, top) = (
        num(e, me, Control::LEFT_PADDING.index),
        num(e, me, Control::TOP_PADDING.index),
    );
    let (room, across) = if vertical { (h, w) } else { (w, h) };
    let placed = lay_out(&asks, room, handle);
    e.untrack(|| {
        e.batch(|| {
            let put = |obj: ObjectId, start: f64, size: f64, across: f64, at: (f64, f64)| {
                let (x, y, w, h) = if vertical {
                    (at.0, at.1 + start, across, size)
                } else {
                    (at.0 + start, at.1, size, across)
                };
                set_computed(e, obj, Item::X.index, Value::Real(x))?;
                set_computed(e, obj, Item::Y.index, Value::Real(y))?;
                set_computed(e, obj, Item::WIDTH.index, Value::Real(w))?;
                set_computed(e, obj, Item::HEIGHT.index, Value::Real(h))
            };
            for (item, (start, size)) in items.iter().zip(&placed) {
                put(*item, *start, *size, across, (0.0, 0.0))?;
            }
            // A handle is a child of the view, past the padding.
            for (handle_item, (start, size)) in handles.iter().zip(&placed) {
                put(*handle_item, start + size, handle, across, (left, top))?;
            }
            Ok(())
        })
    })
}

// ----------------------------------------------------------- the items

/// The items changed: each knows its view, and there is a handle after
/// every one but the last.
pub(crate) fn items_changed(e: &Engine, me: ObjectId) {
    if !is_a::<SplitView>(e, me) {
        return;
    }
    let Some(st) = state_of(e, me) else {
        return;
    };
    let items = container::items(e, me);
    for item in &items {
        let r = e
            .attached_object(*item, &attacher())
            .and_then(|att| set_computed(e, att, SplitViewAttached::VIEW.index, Value::Object(me)));
        check(e, "SplitView", r);
    }
    let want = items.len().saturating_sub(1);
    if st.handles.borrow().len() != want {
        check(e, "SplitView", fit_handles(e, me, &st, want));
    }
}

/// An item that left the view no longer reads it.
pub(crate) fn left(e: &Engine, me: ObjectId, item: ObjectId) {
    if !is_a::<SplitView>(e, me) {
        return;
    }
    if let Some(att) = e.existing_attached_object(item, &attacher()) {
        check(
            e,
            "SplitView",
            set_computed(e, att, SplitViewAttached::VIEW.index, Value::Null),
        );
    }
}

/// Makes or destroys handles until there are `want`.
fn fit_handles(e: &Engine, me: ObjectId, st: &SplitState, want: usize) -> Result<(), EngineError> {
    let component = e.untrack(|| object(e, me, SplitView::HANDLE.index));
    loop {
        let have = st.handles.borrow().len();
        if have > want {
            if let Some(h) = st.handles.borrow_mut().pop() {
                e.destroy(h);
            }
            continue;
        }
        if have == want {
            return Ok(());
        }
        let Some(c) = component else {
            return Ok(());
        };
        // The view owns it, but it is none of the view's items.
        let h = e.component_create_owned(c, me, |e, h| {
            if ljos_quick::is_item(e, h) {
                check(e, "SplitView", set_parent_item(e, h, Some(me)));
            }
        })?;
        if !ljos_quick::is_item(e, h) {
            e.destroy(h);
            return Ok(());
        }
        st.handles.borrow_mut().push(h);
    }
}

/// The handle component was replaced: every handle is made again.
fn remake_handles(e: &Engine, me: ObjectId) -> Result<(), EngineError> {
    let Some(st) = state_of(e, me) else {
        return Ok(());
    };
    st.drag.set(None);
    st.over.set(None);
    for h in st.handles.borrow_mut().drain(..) {
        e.destroy(h);
    }
    let want = container::items(e, me).len().saturating_sub(1);
    fit_handles(e, me, &st, want)
}

// ----------------------------------------------------------- the pointer

/// The handle under `at`, a point in the view, and its index.
fn handle_at(e: &Engine, st: &SplitState, at: kurbo::Point) -> Option<(usize, ObjectId)> {
    e.untrack(|| {
        st.handles
            .borrow()
            .iter()
            .copied()
            .enumerate()
            .find(|(_, h)| {
                let x = num(e, *h, Item::X.index);
                let y = num(e, *h, Item::Y.index);
                let w = num(e, *h, Item::WIDTH.index);
                let ht = num(e, *h, Item::HEIGHT.index);
                at.x >= x && at.x < x + w && at.y >= y && at.y < y + ht
            })
    })
}

fn mark(e: &Engine, handle: ObjectId, prop: ljos_engine::PropIndex, on: bool) {
    if !e.is_alive(handle) {
        return;
    }
    let r = e
        .attached_object(handle, &handle_attacher())
        .and_then(|att| set_computed(e, att, prop, Value::Bool(on)));
    check(e, "SplitHandle", r);
}

fn along(e: &Engine, me: ObjectId, at: kurbo::Point) -> f64 {
    if e.untrack(|| is_vertical(e, me)) {
        at.y
    } else {
        at.x
    }
}

fn press(e: &Engine, me: ObjectId, ev: &PointerEvent, at: kurbo::Point) -> bool {
    if !(takes_input(e, me) && is_left(ev)) {
        return false;
    }
    let Some(st) = state_of(e, me) else {
        return false;
    };
    let Some((i, handle)) = handle_at(e, &st, at) else {
        // A view is opaque to the pointer, as any control.
        return true;
    };
    let items = container::items(e, me);
    let vertical = e.untrack(|| is_vertical(e, me));
    let asks: Vec<Ask> = e.untrack(|| items.iter().map(|it| ask_of(e, *it, vertical)).collect());
    // The handle resizes the item on its side away from the one that fills.
    let (index, sign) = if filler(&asks).is_some_and(|f| i < f) {
        (i, 1.0)
    } else {
        (i + 1, -1.0)
    };
    let Some(item) = items.get(index).copied() else {
        return true;
    };
    let size = e.untrack(|| {
        let p = if vertical { Item::HEIGHT } else { Item::WIDTH };
        num(e, item, p.index)
    });
    st.drag.set(Some(Drag {
        handle,
        item,
        from: along(e, me, at),
        size,
        sign,
    }));
    mark(e, handle, SplitHandleAttached::PRESSED.index, true);
    check(
        e,
        "SplitView",
        set_computed(e, me, SplitView::RESIZING.index, Value::Bool(true)),
    );
    true
}

fn moved(e: &Engine, me: ObjectId, _ev: &PointerEvent, at: kurbo::Point) {
    let Some(st) = state_of(e, me) else {
        return;
    };
    let Some(d) = st.drag.get() else {
        return;
    };
    if !e.is_alive(d.item) {
        return;
    }
    let vertical = e.untrack(|| is_vertical(e, me));
    let ask = e.untrack(|| ask_of(e, d.item, vertical));
    let to = ask.clamp(d.size + d.sign * (along(e, me, at) - d.from));
    let pref = if vertical {
        SplitViewAttached::PREFERRED_HEIGHT
    } else {
        SplitViewAttached::PREFERRED_WIDTH
    };
    let r = e
        .attached_object(d.item, &attacher())
        .and_then(|att| set_computed(e, att, pref.index, Value::Real(to)));
    check(e, "SplitView", r);
}

fn end_drag(e: &Engine, me: ObjectId) {
    let Some(st) = state_of(e, me) else {
        return;
    };
    if let Some(d) = st.drag.take() {
        mark(e, d.handle, SplitHandleAttached::PRESSED.index, false);
        check(
            e,
            "SplitView",
            set_computed(e, me, SplitView::RESIZING.index, Value::Bool(false)),
        );
    }
}

fn release(e: &Engine, me: ObjectId, ev: &PointerEvent, at: kurbo::Point) {
    moved(e, me, ev, at);
    end_drag(e, me);
}

fn cancel(e: &Engine, me: ObjectId) {
    end_drag(e, me);
}

/// The handle under the pointer is hovered.
fn hover_moved(e: &Engine, me: ObjectId, at: kurbo::Point) {
    let Some(st) = state_of(e, me) else {
        return;
    };
    let now = handle_at(e, &st, at).map(|(_, h)| h);
    let was = st.over.replace(now);
    if was != now {
        if let Some(h) = was {
            mark(e, h, SplitHandleAttached::HOVERED.index, false);
        }
        if let Some(h) = now {
            mark(e, h, SplitHandleAttached::HOVERED.index, true);
        }
    }
}

fn view_hover(e: &Engine, me: ObjectId, over: bool) {
    hover(e, me, over);
    if !over
        && let Some(st) = state_of(e, me)
        && let Some(h) = st.over.take()
    {
        mark(e, h, SplitHandleAttached::HOVERED.index, false);
    }
}

pub(crate) fn register(engine: &Engine) {
    let meta = MetaRef::Static(SplitView::static_meta());
    register_pointer_item(
        engine,
        meta.clone(),
        PointerItem {
            press,
            moved,
            release,
            cancel,
            hover: view_hover,
        },
    );
    register_hover_move(engine, meta, hover_moved);
}

// ------------------------------------------------------------ the state

/// A preferred size that was set, as `saveState()` keeps it.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Kept {
    index: usize,
    width: f64,
    height: f64,
}

/// What a saved state starts with, so that a string that is not one is
/// told apart.
const STATE_TAG: &str = "SplitView/1";

fn encode(kept: &[Kept]) -> String {
    let mut s = String::from(STATE_TAG);
    for k in kept {
        s.push_str(&format!(" {}:{}:{}", k.index, k.width, k.height));
    }
    s
}

fn decode(s: &str) -> Option<Vec<Kept>> {
    let mut words = s.split_whitespace();
    if words.next()? != STATE_TAG {
        return None;
    }
    words
        .map(|w| {
            let mut parts = w.split(':');
            let k = Kept {
                index: parts.next()?.parse().ok()?,
                width: parts.next()?.parse().ok()?,
                height: parts.next()?.parse().ok()?,
            };
            parts.next().is_none().then_some(k)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{Ask, Kept, decode, encode, lay_out};

    fn ask(implicit: f64) -> Ask {
        Ask {
            implicit,
            ..Ask::default()
        }
    }

    #[test]
    fn the_item_that_fills_takes_what_the_others_leave() {
        let a = Ask {
            minimum: 50.0,
            ..ask(100.0)
        };
        let b = Ask {
            fill: true,
            ..ask(80.0)
        };
        let c = Ask {
            preferred: 70.0,
            maximum: 120.0,
            ..ask(0.0)
        };
        assert_eq!(
            lay_out(&[a, b, c], 400.0, 6.0),
            vec![(0.0, 100.0), (106.0, 218.0), (330.0, 70.0)]
        );
    }

    #[test]
    fn without_one_that_fills_the_last_does_within_its_maximum() {
        let a = ask(30.0);
        let b = ask(20.0);
        assert_eq!(
            lay_out(&[a, b], 100.0, 6.0),
            vec![(0.0, 30.0), (36.0, 64.0)]
        );
        let c = Ask {
            maximum: 120.0,
            ..ask(70.0)
        };
        assert_eq!(
            lay_out(&[ask(50.0), ask(80.0), c], 400.0, 6.0),
            vec![(0.0, 50.0), (56.0, 80.0), (142.0, 120.0)]
        );
    }

    #[test]
    fn a_preferred_size_is_kept_between_the_minimum_and_the_maximum() {
        let a = Ask {
            minimum: 50.0,
            preferred: 10.0,
            ..ask(100.0)
        };
        assert_eq!(a.wanted(), 50.0);
        let c = Ask {
            preferred: 300.0,
            maximum: 120.0,
            ..ask(0.0)
        };
        assert_eq!(c.wanted(), 120.0);
    }

    #[test]
    fn a_saved_state_reads_back() {
        let kept = vec![Kept {
            index: 2,
            width: 70.0,
            height: -1.0,
        }];
        assert_eq!(decode(&encode(&kept)), Some(kept));
        assert_eq!(decode(&encode(&[])), Some(vec![]));
        assert_eq!(decode("junk"), None);
        assert_eq!(decode("SplitView/1 2:x:1"), None);
    }
}
