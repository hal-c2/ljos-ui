//! `SwipeDelegate` (QtQuick.Templates): a row of a list that is swiped
//! aside to show an item under it.
//!
//! The swipe is an object of its own, the delegate's `swipe`, made when
//! the delegate is or when something is first written through it
//! (`swipe.left: …`, `swipe.onOpened: …`). Its `position` runs from -1
//! (swiped left, the right item shown) to 1 (swiped right, the left item
//! shown). The item of a side is made from its component the first time
//! the position goes that way, under the delegate's background (z -2),
//! and only the item of the side shown is visible. The content item and
//! the background move by the position times the width of that item (of
//! the item behind, when there is one).
//!
//! `open()` and `close()` move the position with `swipe.transition`, then
//! say `completed` and `opened`, or `closed`; `open()` leaves a row open
//! to the other side as it is. A position written directly says nothing.
//!
//! The pointer swipes once it has gone more than the start drag distance
//! across the row; the press then ends as `canceled`, not as a click. The
//! position is the one at the press plus the way the pointer went over
//! the width of the item of the side the row would be on if it followed
//! the pointer, so it jumps across the middle where the items differ. Let
//! go, the row opens to the side it was swiped to if it went more than
//! half the way, or if the pointer went that way at more than
//! [`FLICK_SPEED`] on average since the press; otherwise it closes. A
//! click on an open row says `opened` again and is not a click either,
//! nor is one on a row partly swiped (a position written directly).
//! On an open row, the left or right item shown takes the presses on it
//! (not the item behind): its attached `SwipeDelegate.pressed` is true
//! while held, and let go on it, it says `SwipeDelegate.clicked`, after
//! which the row says it was pressed and open once more, as Qt's does.
//! Let go anywhere else, it is not clicked. A swipe that starts there lets
//! go of it, and when the row is left open, the row is pressed once more
//! and says it is open, or closed while it is still springing back. A row
//! being swiped is not `complete` until it arrives again.
//! Read off Qt 6.11.2 (qml6 and qmltestrunner) as a black box; pinned by
//! `features/controls/swipe-delegate.feature`,
//! `swipe-delegate-release.feature`, `swipe-delegate-attached.feature` and
//! `swipe-delegate-side-release.feature`.
//! Documentation: doc.qt.io/qt-6/qml-qtquick-controls-swipedelegate.html.

use std::cell::Cell;
use std::rc::Rc;

use ljos_engine::{
    Cx, Engine, EngineError, MetaRef, ObjectId, PropIndex, QmlObject, QmlType, Value, WriteKind,
    qml_impl,
};
use ljos_quick::{Item, is_item, run_transition_to, set_parent_item, stop_transition};
use ljos_render::kurbo;
use ljos_runtime::Time;

use crate::button::AbstractButton;
use crate::combobox::ItemDelegate;
use crate::util::{check, is_a, is_unset, num, object, set_computed, warn};

/// The sides, as `SwipeDelegate.Left` and `SwipeDelegate.Right` give them.
pub mod side {
    pub const LEFT: i32 = 1;
    pub const RIGHT: i32 = -1;
}

/// The speed (pixels a second, on average from the press) past which a
/// swipe let go opens to its side however little of the way it went.
const FLICK_SPEED: f64 = 300.0;

/// How far a swipe let go goes to open: more than this.
const OPEN_FROM: f64 = 0.5;

fn swipe_delegate_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![(
        "Side",
        vec![("Left", side::LEFT.into()), ("Right", side::RIGHT.into())],
    )]
}

/// `SwipeDelegate`.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtQuick.Templates",
    name = "SwipeDelegate",
    extends = ItemDelegate,
    enums = swipe_delegate_enums,
    attached = SwipeDelegateAttached
)]
pub struct SwipeDelegate {
    #[qml(base)]
    pub row: ItemDelegate,
    #[qml(readonly, object = "SwipeDelegateSwipe")]
    pub swipe: Option<ObjectId>,
}

#[qml_impl]
impl SwipeDelegate {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        make_swipe(cx.engine, cx.this);
        Ok(())
    }
}

/// What `SwipeDelegate.pressed` and `SwipeDelegate.onClicked` attach to a
/// side item.
#[derive(Debug, Default, QmlObject)]
#[qml(name = "SwipeDelegateAttached")]
pub struct SwipeDelegateAttached {
    #[qml(notify, readonly)]
    pub pressed: bool,
}

#[qml_impl]
impl SwipeDelegateAttached {
    #[qml(signal)]
    fn clicked();
}

fn attacher() -> MetaRef {
    MetaRef::Static(SwipeDelegate::static_meta())
}

/// What a swipe remembers that QML does not see.
#[derive(Debug, Default)]
pub(crate) struct SwipeState {
    delegate: Cell<Option<ObjectId>>,
    drag: Cell<Option<Drag>>,
    /// Counts the moves the swipe was given, so that the end of one that
    /// was replaced does nothing.
    run: Cell<u64>,
}

/// A press on the row, and the swipe it became.
#[derive(Clone, Copy, Debug)]
struct Drag {
    x: f64,
    time: Time,
    /// The position when the press came.
    start: f64,
    swiping: bool,
    /// What is attached to the side item the press is on.
    side: Option<ObjectId>,
    /// The press was on a side item, which a swipe let go of.
    from_side: bool,
}

/// The `swipe` of a `SwipeDelegate`.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick.Templates", name = "SwipeDelegateSwipe", uncreatable)]
pub struct SwipeDelegateSwipe {
    #[qml(notify)]
    pub position: f64,
    #[qml(notify, readonly)]
    pub complete: bool,
    #[qml(notify, object = "Component")]
    pub left: Option<ObjectId>,
    #[qml(notify, object = "Component")]
    pub behind: Option<ObjectId>,
    #[qml(notify, object = "Component")]
    pub right: Option<ObjectId>,
    #[qml(notify, readonly, object = "Item")]
    pub left_item: Option<ObjectId>,
    #[qml(notify, readonly, object = "Item")]
    pub behind_item: Option<ObjectId>,
    #[qml(notify, readonly, object = "Item")]
    pub right_item: Option<ObjectId>,
    #[qml(notify)]
    pub enabled: bool,
    #[qml(notify, object = "Transition")]
    pub transition: Option<ObjectId>,
    #[qml(skip)]
    pub(crate) state: Rc<SwipeState>,
}

impl Default for SwipeDelegateSwipe {
    fn default() -> Self {
        Self {
            position: 0.0,
            complete: false,
            left: None,
            behind: None,
            right: None,
            left_item: None,
            behind_item: None,
            right_item: None,
            enabled: true,
            transition: None,
            state: Rc::default(),
        }
    }
}

#[qml_impl]
impl SwipeDelegateSwipe {
    #[qml(signal)]
    fn completed();
    #[qml(signal)]
    fn opened();
    #[qml(signal)]
    fn closed();

    #[qml(invokable)]
    fn open(cx: &Cx, side: Value) -> Result<(), EngineError> {
        let to = if side.to_number() as i32 == side::RIGHT {
            -1.0
        } else {
            1.0
        };
        // A row open to one side has to be closed before it opens to the
        // other, as in Qt; one only partly open goes either way.
        let at = cx
            .engine
            .untrack(|| num(cx.engine, cx.this, Swipe::POSITION.index));
        if at == -to {
            return Ok(());
        }
        go(cx.engine, cx.this, to);
        Ok(())
    }

    #[qml(invokable)]
    fn close(cx: &Cx) -> Result<(), EngineError> {
        go(cx.engine, cx.this, 0.0);
        Ok(())
    }
}

type Swipe = SwipeDelegateSwipe;

fn state_of(engine: &Engine, swipe: ObjectId) -> Option<Rc<SwipeState>> {
    engine.with::<Swipe, _>(swipe, |s| s.state.clone()).ok()
}

/// The swipe of `delegate`, made if it is not yet. None for what is no
/// swipe delegate.
pub(crate) fn make_swipe(engine: &Engine, delegate: ObjectId) -> Option<ObjectId> {
    if !is_a::<SwipeDelegate>(engine, delegate) {
        return None;
    }
    if let Some(s) = engine.untrack(|| object(engine, delegate, SwipeDelegate::SWIPE.index)) {
        return Some(s);
    }
    let made = engine
        .create_native_incomplete(Swipe::default(), Some(delegate))
        .and_then(|s| {
            engine.complete(s)?;
            Ok(s)
        });
    let swipe = match made {
        Ok(s) => s,
        Err(err) => {
            warn(engine, format!("swipe: {err}"));
            return None;
        }
    };
    if let Some(st) = state_of(engine, swipe) {
        st.delegate.set(Some(delegate));
    }
    check(
        engine,
        "swipe",
        engine.set_internal(delegate, SwipeDelegate::SWIPE, Some(swipe)),
    );
    check(
        engine,
        "swipe",
        engine
            .watch(swipe, Swipe::POSITION.index, move |e, v| {
                e.untrack(|| show_side(e, swipe, v.to_number()));
            })
            .map(|_| ()),
    );
    Some(swipe)
}

fn swipe_of(engine: &Engine, delegate: ObjectId) -> Option<ObjectId> {
    if !is_a::<SwipeDelegate>(engine, delegate) {
        return None;
    }
    object(engine, delegate, SwipeDelegate::SWIPE.index)
}

/// The component and the item property of the side `position` shows: the
/// one behind when there is one.
fn side_props(engine: &Engine, swipe: ObjectId, position: f64) -> Option<(PropIndex, PropIndex)> {
    if object(engine, swipe, Swipe::BEHIND.index).is_some() {
        Some((Swipe::BEHIND.index, Swipe::BEHIND_ITEM.index))
    } else if position > 0.0 {
        Some((Swipe::LEFT.index, Swipe::LEFT_ITEM.index))
    } else if position < 0.0 {
        Some((Swipe::RIGHT.index, Swipe::RIGHT_ITEM.index))
    } else {
        None
    }
}

/// The item of the side `position` shows, made from its component if it
/// is not yet. None where that side has no component.
fn item_for(engine: &Engine, swipe: ObjectId, position: f64) -> Option<ObjectId> {
    let (component, item_p) = engine.untrack(|| side_props(engine, swipe, position))?;
    if let Some(item) = engine.untrack(|| object(engine, swipe, item_p)) {
        return Some(item);
    }
    let component = engine.untrack(|| object(engine, swipe, component))?;
    let delegate = state_of(engine, swipe)?.delegate.get()?;
    let made = engine.component_create_owned(component, delegate, |e, root| {
        if is_item(e, root) {
            check(e, "swipe", set_parent_item(e, root, Some(delegate)));
        }
    });
    let item = match made {
        Ok(item) => item,
        Err(err) => {
            warn(engine, format!("swipe: {err}"));
            return None;
        }
    };
    if is_item(engine, item) && is_unset(engine, item, Item::Z.index) {
        let _ = engine.write_internal(item, Item::Z.index, Value::Real(-2.0), WriteKind::Initial);
    }
    check(
        engine,
        "swipe",
        engine.write_internal(swipe, item_p, Value::Object(item), WriteKind::Imperative),
    );
    Some(item)
}

/// Makes the item of the side `position` shows and shows only it. At 0
/// the items stay as they are.
fn show_side(engine: &Engine, swipe: ObjectId, position: f64) {
    if position == 0.0 {
        return;
    }
    let Some(shown) = item_for(engine, swipe, position) else {
        return;
    };
    for p in [
        Swipe::LEFT_ITEM.index,
        Swipe::BEHIND_ITEM.index,
        Swipe::RIGHT_ITEM.index,
    ] {
        if let Some(item) = object(engine, swipe, p).filter(|i| is_item(engine, *i)) {
            let r = set_computed(
                engine,
                item,
                Item::VISIBLE.index,
                Value::Bool(item == shown),
            );
            check(engine, "swipe", r);
        }
    }
}

/// How far the content and the background of `delegate` are moved
/// (tracked): 0 for what is no swipe delegate.
pub(crate) fn offset(engine: &Engine, delegate: ObjectId) -> f64 {
    let Some(swipe) = swipe_of(engine, delegate) else {
        return 0.0;
    };
    let position = num(engine, swipe, Swipe::POSITION.index);
    let Some((_, item_p)) = side_props(engine, swipe, position) else {
        return 0.0;
    };
    object(engine, swipe, item_p)
        .map_or(0.0, |item| position * num(engine, item, Item::WIDTH.index))
}

// ------------------------------------------------------------- the moves

/// Moves the position to `to` with the transition, then says where it
/// ended. A move under way is replaced.
fn go(engine: &Engine, swipe: ObjectId, to: f64) {
    let Some(st) = state_of(engine, swipe) else {
        return;
    };
    let id = st.run.get() + 1;
    st.run.set(id);
    let transition = engine.untrack(|| object(engine, swipe, Swipe::TRANSITION.index));
    let from = engine.untrack(|| num(engine, swipe, Swipe::POSITION.index));
    let ended = {
        let st = st.clone();
        move |e: &Engine| {
            if st.run.get() == id {
                arrived(e, swipe, to);
            }
        }
    };
    match transition {
        Some(t) if from != to => run_transition_to(
            engine,
            t,
            swipe,
            Swipe::POSITION.index,
            Value::Real(to),
            ended,
        ),
        _ => ended(engine),
    }
}

/// Stops a move under way, leaving the position where it is.
fn halt(engine: &Engine, swipe: ObjectId) {
    if let Some(st) = state_of(engine, swipe) {
        st.run.set(st.run.get() + 1);
    }
    if let Some(t) = engine.untrack(|| object(engine, swipe, Swipe::TRANSITION.index)) {
        stop_transition(engine, t);
    }
}

fn arrived(engine: &Engine, swipe: ObjectId, to: f64) {
    if !engine.is_alive(swipe) {
        return;
    }
    check(
        engine,
        "swipe",
        set_computed(engine, swipe, Swipe::POSITION.index, Value::Real(to)),
    );
    let open = to != 0.0;
    check(
        engine,
        "swipe",
        set_computed(engine, swipe, Swipe::COMPLETE.index, Value::Bool(open)),
    );
    if open {
        emit(engine, swipe, "completed");
        emit(engine, swipe, "opened");
    } else {
        emit(engine, swipe, "closed");
    }
}

fn emit(engine: &Engine, obj: ObjectId, signal: &str) {
    if engine.is_alive(obj) {
        check(engine, signal, engine.emit_by_name(obj, signal, &[]));
    }
}

// ----------------------------------------------------------- the pointer

/// A press on `delegate` at `x` (its own coordinates): it may become a
/// swipe. A move under way stops where it is.
pub(crate) fn pressed(engine: &Engine, delegate: ObjectId, x: f64, time: Time) {
    let Some(swipe) = engine.untrack(|| swipe_of(engine, delegate)) else {
        return;
    };
    halt(engine, swipe);
    let Some(st) = state_of(engine, swipe) else {
        return;
    };
    let start = engine.untrack(|| num(engine, swipe, Swipe::POSITION.index));
    st.drag.set(Some(Drag {
        x,
        time,
        start,
        swiping: false,
        side: None,
        from_side: false,
    }));
}

/// The press `pressed` was told of took the row, at `at`: on an open row
/// the side item shown under it takes it too.
pub(crate) fn press_taken(engine: &Engine, delegate: ObjectId, at: kurbo::Point) {
    engine.untrack(|| {
        let Some(swipe) = swipe_of(engine, delegate) else {
            return;
        };
        let Some(st) = state_of(engine, swipe) else {
            return;
        };
        let Some(mut d) = st.drag.get() else {
            return;
        };
        let position = num(engine, swipe, Swipe::POSITION.index);
        let open = engine.read(swipe, Swipe::COMPLETE.index).to_boolean();
        if !open || object(engine, swipe, Swipe::BEHIND.index).is_some() {
            return;
        }
        let item_p = if position > 0.0 {
            Swipe::LEFT_ITEM.index
        } else if position < 0.0 {
            Swipe::RIGHT_ITEM.index
        } else {
            return;
        };
        let Some(item) = object(engine, swipe, item_p).filter(|i| holds(engine, *i, at)) else {
            return;
        };
        let Some(att) = engine.existing_attached_object(item, &attacher()) else {
            return;
        };
        d.side = Some(att);
        st.drag.set(Some(d));
        side_pressed(engine, att, true);
    });
}

/// Whether `at`, in the delegate's coordinates, is on its child `item`.
fn holds(engine: &Engine, item: ObjectId, at: kurbo::Point) -> bool {
    let n = |p: PropIndex| num(engine, item, p);
    let (x, y) = (n(Item::X.index), n(Item::Y.index));
    (x..x + n(Item::WIDTH.index)).contains(&at.x) && (y..y + n(Item::HEIGHT.index)).contains(&at.y)
}

fn side_pressed(engine: &Engine, att: ObjectId, on: bool) {
    if engine.is_alive(att) {
        let r = set_computed(
            engine,
            att,
            SwipeDelegateAttached::PRESSED.index,
            Value::Bool(on),
        );
        check(engine, "SwipeDelegate.pressed", r);
    }
}

/// The position the pointer `dx` across from the press puts the row at,
/// making the item of the side it goes to.
fn position_at(engine: &Engine, swipe: ObjectId, start: f64, dx: f64) -> f64 {
    let width_of = |position: f64| {
        item_for(engine, swipe, position).map_or(0.0, |i| num(engine, i, Item::WIDTH.index))
    };
    // The side is the one the row would be on if it followed the pointer,
    // but the way the pointer went is counted in that side's width from
    // the position at the press, so the position jumps as the side
    // changes when the items differ in width: Qt's measure.
    let moved = start * width_of(start) + dx;
    if moved == 0.0 {
        return 0.0;
    }
    let w = width_of(moved);
    if w <= 0.0 {
        return 0.0;
    }
    (start + dx / w).clamp(-1.0, 1.0)
}

/// A move with the press held. True while the row is swiped: the press
/// then stays with it wherever the pointer goes.
pub(crate) fn moved(engine: &Engine, delegate: ObjectId, at: kurbo::Point) -> bool {
    engine.untrack(|| {
        let Some(swipe) = swipe_of(engine, delegate) else {
            return false;
        };
        let Some(st) = state_of(engine, swipe) else {
            return false;
        };
        let Some(mut d) = st.drag.get() else {
            return false;
        };
        let dx = at.x - d.x;
        if !d.swiping {
            let can = engine.read(swipe, Swipe::ENABLED.index).to_boolean()
                && [Swipe::LEFT, Swipe::BEHIND, Swipe::RIGHT]
                    .iter()
                    .any(|p| object(engine, swipe, p.index).is_some());
            if !can || dx.abs() <= ljos_quick::start_drag_distance(engine) {
                return false;
            }
            d.swiping = true;
            // A row being swiped is not complete, even before it moves.
            check(
                engine,
                "swipe",
                set_computed(engine, swipe, Swipe::COMPLETE.index, Value::Bool(false)),
            );
            if let Some(att) = d.side.take() {
                side_pressed(engine, att, false);
                d.from_side = true;
            }
            st.drag.set(Some(d));
        }
        let p = position_at(engine, swipe, d.start, dx);
        check(
            engine,
            "swipe",
            set_computed(engine, swipe, Swipe::POSITION.index, Value::Real(p)),
        );
        true
    })
}

/// Where a swipe let go at `position`, the pointer having gone across at
/// `speed` on average since the press, goes: -1, 0 or 1.
fn goes_to(position: f64, speed: f64) -> f64 {
    if speed > FLICK_SPEED && position > 0.0 {
        1.0
    } else if speed < -FLICK_SPEED && position < 0.0 {
        -1.0
    } else if position > OPEN_FROM {
        1.0
    } else if position < -OPEN_FROM {
        -1.0
    } else {
        0.0
    }
}

/// The press on `delegate` was let go at `at`, after `pressed` went false.
/// True when the swipe has dealt with it, and it is no click: a swipe
/// goes to where it is let go to, an open row says it is open, and a row
/// that is not closed is canceled.
pub(crate) fn released(engine: &Engine, delegate: ObjectId, at: kurbo::Point, time: Time) -> bool {
    let Some(swipe) = engine.untrack(|| swipe_of(engine, delegate)) else {
        return false;
    };
    let Some(d) = state_of(engine, swipe).and_then(|st| st.drag.take()) else {
        return false;
    };
    if d.swiping {
        let position = engine.untrack(|| position_at(engine, swipe, d.start, at.x - d.x));
        let secs = time.saturating_since(d.time).as_secs_f64();
        // Moves that all come at once tell nothing of speed.
        let speed = if secs > 0.0 { (at.x - d.x) / secs } else { 0.0 };
        emit(engine, delegate, "canceled");
        let to = goes_to(position, speed);
        go(engine, swipe, to);
        if d.from_side && to != 0.0 {
            pressed_again(engine, delegate, swipe);
        }
        return true;
    }
    // A row that is not closed is no click; one that is open says so.
    if engine.untrack(|| num(engine, swipe, Swipe::POSITION.index)) == 0.0 {
        return false;
    }
    if engine.untrack(|| engine.read(swipe, Swipe::COMPLETE.index).to_boolean()) {
        emit(engine, swipe, "opened");
    }
    let on_side = d.side.filter(|att| {
        engine.untrack(|| {
            engine
                .owner(*att)
                .is_some_and(|item| holds(engine, item, at))
        })
    });
    if let Some(att) = on_side {
        side_pressed(engine, att, false);
        emit(engine, att, "clicked");
    }
    emit(engine, delegate, "canceled");
    if let Some(att) = d.side {
        pressed_again(engine, delegate, swipe);
        side_pressed(engine, att, false);
    }
    true
}

/// After a press on a side item that leaves the row open, Qt's row is
/// pressed once more and says it is open, or, while it is still on its
/// way back there, closed.
fn pressed_again(engine: &Engine, delegate: ObjectId, swipe: ObjectId) {
    let complete = engine.untrack(|| engine.read(swipe, Swipe::COMPLETE.index).to_boolean());
    let pressed = |on| {
        let r = set_computed(
            engine,
            delegate,
            AbstractButton::PRESSED.index,
            Value::Bool(on),
        );
        check(engine, "pressed", r);
    };
    pressed(true);
    emit(engine, swipe, if complete { "opened" } else { "closed" });
    pressed(false);
}

/// The press was taken away: a swipe goes back to where it started.
pub(crate) fn canceled(engine: &Engine, delegate: ObjectId) {
    let Some(swipe) = engine.untrack(|| swipe_of(engine, delegate)) else {
        return;
    };
    let Some(d) = state_of(engine, swipe).and_then(|st| st.drag.take()) else {
        return;
    };
    if let Some(att) = d.side {
        side_pressed(engine, att, false);
    }
    if d.swiping {
        go(
            engine,
            swipe,
            d.start.signum() * f64::from(u8::from(d.start != 0.0)),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::goes_to;

    #[test]
    fn let_go_slowly_it_opens_past_half_the_way() {
        assert_eq!(goes_to(0.41, 0.0), 0.0);
        assert_eq!(goes_to(0.5, 0.0), 0.0);
        assert_eq!(goes_to(0.61, 0.0), 1.0);
        assert_eq!(goes_to(-0.61, 0.0), -1.0);
        assert_eq!(goes_to(-0.5, 0.0), 0.0);
    }

    #[test]
    fn flicked_it_opens_to_the_side_it_went() {
        // 20 pixels in 30 ms, and the same held 50 ms more (qmltestrunner).
        assert_eq!(goes_to(0.2, 20.0 / 0.030), 1.0);
        assert_eq!(goes_to(0.2, 20.0 / 0.080), 0.0);
        assert_eq!(goes_to(-0.275, -22.0 / 0.030), -1.0);
    }

    #[test]
    fn a_flick_the_other_way_does_not_close_a_row_past_half_the_way() {
        // Open, flicked back 20 pixels: it stays open; flicked back 70 it
        // closes, as its position says.
        assert_eq!(goes_to(0.8, -20.0 / 0.020), 1.0);
        assert_eq!(goes_to(0.3, -70.0 / 0.020), 0.0);
    }
}
