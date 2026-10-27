//! `Dial` (QtQuick.Templates): a value turned like a knob.
//!
//! Behaviour follows doc.qt.io/qt-6/qml-qtquick-controls-dial.html and is
//! pinned by `features/controls/dial.feature`. What qml6 shows and the
//! documentation leaves open:
//!
//! - The angle is clockwise from twelve o'clock; one before `startAngle`
//!   is read a turn later, and one in the gap after `endAngle` belongs to
//!   the nearer end, the gap split in its middle.
//! - A dial that does not `wrap` ignores a pointer that would take it more
//!   than half the way at once, unless the short way round to the pointer,
//!   more than half a circle, is through the gap: then it goes to the end
//!   on that way. The very middle is twelve o'clock.
//! - In the `Horizontal` and `Vertical` input modes a press changes
//!   nothing, and the value follows the pointer from there, the whole way
//!   being twice the dial's width (or height).
//! - Letting go across the gap, or past an end, is a move, though the
//!   value stays.

use std::cell::Cell;
use std::rc::Rc;

use ljos_a11y::{NodeData, Range};
use ljos_engine::{
    Cx, Engine, EngineError, MetaRef, ObjectId, OverrideContent, OverrideId, PropIndex, QmlObject,
    QmlType, RestoreMode, Value, WriteKind, qml_impl,
};
use ljos_quick::flickable::{WheelItem, register_wheel_item};
use ljos_quick::focus::{FocusReason, force_active_focus};
use ljos_quick::keys::{KeyItem, key, register_key_item};
use ljos_quick::{
    AccessibleAction, AccessibleItem, Item, KeyEvent, PointerEvent, PointerItem, WheelEvent,
    flickable, register_accessible_item, register_pointer_item,
};
use ljos_render::kurbo;

use crate::control::{Control, focus_policy, hover, is_left, no_key, takes_input};
use crate::parts::{PartKind, manage_part};
use crate::scroll_bar::snap_mode;
use crate::slider::{DEFAULT_STEP, Span, snap_mode_enum};
use crate::util::{check, flag, num, set_derived};

/// `Dial.InputMode`.
pub(crate) mod input_mode {
    pub const CIRCULAR: i32 = 0;
    pub const HORIZONTAL: i32 = 1;
    pub const VERTICAL: i32 = 2;
}

fn dial_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![
        snap_mode_enum(),
        (
            "InputMode",
            vec![
                ("Circular", i64::from(input_mode::CIRCULAR)),
                ("Horizontal", i64::from(input_mode::HORIZONTAL)),
                ("Vertical", i64::from(input_mode::VERTICAL)),
            ],
        ),
    ]
}

/// `Dial`.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick.Templates",
    name = "Dial",
    extends = Control,
    enums = dial_enums
)]
pub struct Dial {
    #[qml(base)]
    pub control: Control,
    #[qml(notify)]
    pub from: f64,
    #[qml(notify)]
    pub to: f64,
    #[qml(notify)]
    pub value: f64,
    #[qml(notify, readonly)]
    pub position: f64,
    #[qml(notify, readonly)]
    pub angle: f64,
    #[qml(notify)]
    pub start_angle: f64,
    #[qml(notify)]
    pub end_angle: f64,
    #[qml(notify)]
    pub step_size: f64,
    #[qml(notify, enumeration = "SnapMode")]
    pub snap_mode: i32,
    #[qml(notify)]
    pub wrap: bool,
    #[qml(notify, readonly)]
    pub pressed: bool,
    #[qml(notify, holds = "Item")]
    pub handle: Option<ObjectId>,
    #[qml(notify)]
    pub live: bool,
    #[qml(notify, enumeration = "InputMode")]
    pub input_mode: i32,
    #[qml(skip)]
    state: Rc<Held>,
}

impl Default for Dial {
    fn default() -> Self {
        Self {
            control: Control {
                focus_policy: focus_policy::TAB,
                ..Control::default()
            },
            from: 0.0,
            to: 1.0,
            value: 0.0,
            position: 0.0,
            angle: -140.0,
            start_angle: -140.0,
            end_angle: 140.0,
            step_size: 0.0,
            snap_mode: snap_mode::NO_SNAP,
            wrap: false,
            pressed: false,
            handle: None,
            live: true,
            input_mode: input_mode::CIRCULAR,
            state: Rc::default(),
        }
    }
}

/// What a dial remembers while the pointer has it.
#[derive(Debug, Default)]
struct Held {
    /// Where the press was, and the position the dial had then.
    press: Cell<Option<(kurbo::Point, f64)>>,
    /// The layer on `position` while the handle is not where the value is:
    /// a dial that is not live, or one that snaps on release.
    drag: Cell<Option<OverrideId>>,
}

fn held_of(e: &Engine, obj: ObjectId) -> Option<Rc<Held>> {
    e.with::<Dial, _>(obj, |d| d.state.clone()).ok()
}

fn span(e: &Engine, obj: ObjectId) -> Span {
    Span::read(e, obj, Dial::FROM.index, Dial::TO.index)
}

/// The template's write: a binding on the property stays.
fn put(e: &Engine, obj: ObjectId, prop: PropIndex, v: Value) {
    if e.is_alive(obj) {
        check(e, "dial", set_derived(e, obj, prop, v));
    }
}

fn keep_value(e: &Engine, obj: ObjectId) {
    let (v, s) = e.untrack(|| (num(e, obj, Dial::VALUE.index), span(e, obj)));
    let k = s.clamp(v);
    if k != v {
        put(e, obj, Dial::VALUE.index, Value::Real(k));
    }
}

fn step_of(e: &Engine, obj: ObjectId) -> f64 {
    let step = num(e, obj, Dial::STEP_SIZE.index);
    if step > 0.0 { step } else { DEFAULT_STEP }
}

fn moved_signal(e: &Engine, obj: ObjectId) {
    check(e, "moved", e.emit_by_name(obj, "moved", &[]));
}

/// Sets the value; whether it changed.
fn set_value(e: &Engine, obj: ObjectId, v: f64) -> bool {
    let before = e.untrack(|| num(e, obj, Dial::VALUE.index));
    let v = e.untrack(|| span(e, obj)).clamp(v);
    put(e, obj, Dial::VALUE.index, Value::Real(v));
    v != before
}

fn step(e: &Engine, obj: ObjectId, up: bool) -> bool {
    let (v, s) = e.untrack(|| (num(e, obj, Dial::VALUE.index), step_of(e, obj)));
    set_value(e, obj, if up { v + s } else { v - s })
}

#[qml_impl]
impl Dial {
    /// The user turned the dial: with the pointer, a key or the wheel.
    #[qml(signal)]
    fn moved();

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, obj) = (cx.engine, cx.this);
        keep_value(e, obj);
        for p in [Dial::FROM.index, Dial::TO.index, Dial::VALUE.index] {
            e.watch(obj, p, move |e, _| keep_value(e, obj))?;
        }
        e.bind_initial(obj, Dial::POSITION.index, move |e| {
            Value::Real(span(e, obj).position(num(e, obj, Dial::VALUE.index)))
        })?;
        e.bind_initial(obj, Dial::ANGLE.index, move |e| {
            let start = num(e, obj, Dial::START_ANGLE.index);
            let end = num(e, obj, Dial::END_ANGLE.index);
            Value::Real(start + num(e, obj, Dial::POSITION.index) * (end - start))
        })?;
        manage_part(e, obj, Dial::HANDLE.index, PartKind::Free)?;
        for p in [Item::ENABLED.index, Item::VISIBLE.index] {
            e.watch(obj, p, move |e, v| {
                if !v.to_boolean() {
                    cancel(e, obj);
                }
            })?;
        }
        Ok(())
    }

    /// One step towards `to`. Not a move by the user, and not snapped.
    #[qml(invokable)]
    fn increase(cx: &Cx) {
        step(cx.engine, cx.this, true);
    }

    #[qml(invokable)]
    fn decrease(cx: &Cx) {
        step(cx.engine, cx.this, false);
    }
}

// ----------------------------------------------------------- the pointer

/// The position the pointer at `p` turns the dial to.
fn position_at(e: &Engine, obj: ObjectId, held: (kurbo::Point, f64), p: kurbo::Point) -> f64 {
    e.untrack(|| {
        let (w, h) = (
            num(e, obj, Item::WIDTH.index),
            num(e, obj, Item::HEIGHT.index),
        );
        let (pressed_at, from) = held;
        match num(e, obj, Dial::INPUT_MODE.index) as i32 {
            input_mode::HORIZONTAL if w > 0.0 => {
                (from + (p.x - pressed_at.x) / (2.0 * w)).clamp(0.0, 1.0)
            }
            input_mode::VERTICAL if h > 0.0 => {
                (from - (p.y - pressed_at.y) / (2.0 * h)).clamp(0.0, 1.0)
            }
            input_mode::HORIZONTAL | input_mode::VERTICAL => from,
            _ => {
                let start = num(e, obj, Dial::START_ANGLE.index);
                let end = num(e, obj, Dial::END_ANGLE.index);
                let (dx, up) = (p.x - w / 2.0, h / 2.0 - p.y);
                // The very middle is twelve o'clock to qml6 (atan2 of +0, +0).
                let mut angle = (dx + 0.0).atan2(up + 0.0).to_degrees();
                if angle < start {
                    angle += 360.0;
                }
                // In the gap, the pointer is at the nearer end: past the
                // middle of the gap, before the start.
                if angle > end && angle - end > start + 360.0 - angle {
                    angle -= 360.0;
                }
                // Not held to the ends: a release past one is a move.
                if end == start {
                    0.0
                } else {
                    (angle - start) / (end - start)
                }
            }
        }
    })
}

fn snaps(e: &Engine, obj: ObjectId, mode: i32) -> bool {
    e.untrack(|| num(e, obj, Dial::SNAP_MODE.index)) as i32 == mode
}

/// Whether `from`, `to` and a step that is not 0 are all whole numbers.
fn whole_steps(s: Span, step: f64) -> bool {
    step != 0.0
        && [s.from, s.to, step]
            .iter()
            .all(|n| n.is_finite() && n.fract() == 0.0)
}

/// Turns the dial to where the pointer has it; `release` says the pointer
/// let go there.
fn turn_to(e: &Engine, obj: ObjectId, st: &Held, p: kurbo::Point, release: bool) {
    let Some(held) = st.press.get() else {
        return;
    };
    let (s, step, live, wrap, shown_now) = e.untrack(|| {
        (
            span(e, obj),
            num(e, obj, Dial::STEP_SIZE.index),
            flag(e, obj, Dial::LIVE.index),
            flag(e, obj, Dial::WRAP.index),
            num(e, obj, Dial::POSITION.index),
        )
    });
    let raw = position_at(e, obj, held, p);
    let mut free = raw.clamp(0.0, 1.0);
    if !wrap && (free - shown_now).abs() > 0.5 {
        let circular =
            e.untrack(|| num(e, obj, Dial::INPUT_MODE.index)) as i32 == input_mode::CIRCULAR;
        let turn =
            e.untrack(|| num(e, obj, Dial::END_ANGLE.index) - num(e, obj, Dial::START_ANGLE.index));
        if circular && (free - shown_now).abs() * turn.abs() > 180.0 {
            // The short way round is through the gap: to the end on that way.
            free = if free < shown_now { 1.0 } else { 0.0 };
        } else {
            // Too far to be a turn: the dial stays. Letting go there is
            // still a move to qml6.
            if release && raw != shown_now {
                moved_signal(e, obj);
            }
            return;
        }
    }
    // Unlike a slider's, the value of a dial that snaps on release is
    // free until then too (qml6).
    let snap = if release {
        !snaps(e, obj, snap_mode::NO_SNAP)
    } else {
        snaps(e, obj, snap_mode::SNAP_ALWAYS)
    };
    let shown_at = if snap {
        s.snap_position(free, step)
    } else if whole_steps(s, step) {
        // A dial whose ends and step are whole numbers is turned in whole
        // numbers, also when it does not snap (qml6).
        s.position(s.value_at(free).round())
    } else {
        free
    };
    if live || release {
        set_value(e, obj, s.value_at(shown_at));
    }
    let settled = e.untrack(|| s.position(num(e, obj, Dial::VALUE.index)));
    if (live || release) && shown_at == settled {
        if let Some(layer) = st.drag.take() {
            let _ = e.remove_override(
                obj,
                Dial::POSITION.index,
                layer,
                RestoreMode::BindingOrValue,
            );
        }
    } else {
        let shown = Value::Real(shown_at);
        let r = match st.drag.get() {
            Some(layer) => {
                e.write_internal(obj, Dial::POSITION.index, shown, WriteKind::Override(layer))
            }
            None => e
                .push_override(obj, Dial::POSITION.index, OverrideContent::Value(shown))
                .map(|id| st.drag.set(Some(id))),
        };
        check(e, "position", r);
    }
    // qml6 counts a release past an end as a move, though the dial stays.
    let after = e.untrack(|| num(e, obj, Dial::POSITION.index));
    if after != shown_now || (release && raw != free) {
        moved_signal(e, obj);
    }
}

fn press(e: &Engine, obj: ObjectId, ev: &PointerEvent, p: kurbo::Point) -> bool {
    if !takes_input(e, obj) || !is_left(ev) {
        return false;
    }
    let Some(st) = held_of(e, obj) else {
        return false;
    };
    let policy = e.untrack(|| num(e, obj, Control::FOCUS_POLICY.index)) as i32;
    if policy & focus_policy::CLICK != 0 {
        force_active_focus(e, obj, FocusReason::Mouse);
    }
    flickable::keep_pointer_grab(e, obj, true);
    let from = e.untrack(|| num(e, obj, Dial::POSITION.index));
    st.press.set(Some((p, from)));
    put(e, obj, Dial::PRESSED.index, Value::Bool(true));
    let circular = e.untrack(|| num(e, obj, Dial::INPUT_MODE.index)) as i32 == input_mode::CIRCULAR;
    if circular {
        turn_to(e, obj, &st, p, false);
    }
    true
}

fn moved(e: &Engine, obj: ObjectId, _ev: &PointerEvent, p: kurbo::Point) {
    if let Some(st) = held_of(e, obj) {
        turn_to(e, obj, &st, p, false);
    }
}

fn let_go(e: &Engine, obj: ObjectId, st: &Held) {
    if st.press.take().is_none() {
        return;
    }
    if let Some(layer) = st.drag.take() {
        let _ = e.remove_override(
            obj,
            Dial::POSITION.index,
            layer,
            RestoreMode::BindingOrValue,
        );
    }
    flickable::keep_pointer_grab(e, obj, false);
    put(e, obj, Dial::PRESSED.index, Value::Bool(false));
}

fn release(e: &Engine, obj: ObjectId, _ev: &PointerEvent, p: kurbo::Point) {
    let Some(st) = held_of(e, obj) else {
        return;
    };
    if st.press.get().is_none() {
        return;
    }
    // A press that has not moved in a linear mode changes nothing.
    let circular = e.untrack(|| num(e, obj, Dial::INPUT_MODE.index)) as i32 == input_mode::CIRCULAR;
    if circular || st.press.get().is_some_and(|(at, _)| at != p) || st.drag.get().is_some() {
        turn_to(e, obj, &st, p, true);
    }
    let_go(e, obj, &st);
}

fn cancel(e: &Engine, obj: ObjectId) {
    if let Some(st) = held_of(e, obj) {
        let_go(e, obj, &st);
    }
}

// ------------------------------------------------------- keys and wheel

fn key_press(e: &Engine, obj: ObjectId, ev: &KeyEvent) -> bool {
    if !takes_input(e, obj) {
        return false;
    }
    let changed = match ev.key {
        key::RIGHT | key::UP => step(e, obj, true),
        key::LEFT | key::DOWN => step(e, obj, false),
        key::HOME => set_value(e, obj, e.untrack(|| num(e, obj, Dial::FROM.index))),
        key::END => set_value(e, obj, e.untrack(|| num(e, obj, Dial::TO.index))),
        _ => return false,
    };
    if changed {
        moved_signal(e, obj);
    }
    true
}

fn wheel(e: &Engine, obj: ObjectId, ev: &WheelEvent, _p: kurbo::Point) -> bool {
    let wanted = e.untrack(|| flag(e, obj, Control::WHEEL_ENABLED.index));
    if !wanted || !takes_input(e, obj) {
        return false;
    }
    let turned = if ev.angle_delta.y != 0.0 {
        ev.angle_delta.y
    } else {
        ev.angle_delta.x
    };
    if turned == 0.0 {
        return true;
    }
    let notches = ((turned.abs() / WheelEvent::NOTCH).trunc() as i32).max(1);
    let mut any = false;
    for _ in 0..notches {
        any |= step(e, obj, turned > 0.0);
    }
    if any {
        moved_signal(e, obj);
    }
    true
}

// ------------------------------------------------- assistive technology

fn describe(e: &Engine, obj: ObjectId, data: &mut NodeData) {
    let s = span(e, obj);
    data.range = Some(Range {
        value: num(e, obj, Dial::VALUE.index),
        min: s.lo(),
        max: s.hi(),
        step: step_of(e, obj),
    });
}

fn act(e: &Engine, obj: ObjectId, action: AccessibleAction) -> bool {
    if !takes_input(e, obj) {
        return false;
    }
    let rising = e.untrack(|| {
        let s = span(e, obj);
        s.to >= s.from
    });
    let changed = match action {
        AccessibleAction::Increase => step(e, obj, rising),
        AccessibleAction::Decrease => step(e, obj, !rising),
        AccessibleAction::SetValue(v) if v.is_finite() => set_value(e, obj, v),
        _ => return false,
    };
    if changed {
        moved_signal(e, obj);
    }
    changed
}

pub(crate) fn register(engine: &Engine) {
    let meta = || MetaRef::Static(Dial::static_meta());
    register_pointer_item(
        engine,
        meta(),
        PointerItem {
            press,
            moved,
            release,
            cancel,
            hover,
        },
    );
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whole_ends_and_a_whole_step_turn_in_whole_numbers() {
        let s = |from, to| Span { from, to };
        assert!(whole_steps(s(0.0, 30.0), 1.0));
        assert!(whole_steps(s(30.0, 0.0), -1.0));
        assert!(!whole_steps(s(0.0, 30.0), 0.0));
        assert!(!whole_steps(s(0.5, 30.0), 1.0));
        assert!(!whole_steps(s(0.0, 20.5), 1.0));
        assert!(!whole_steps(s(0.0, 30.0), 1.5));
    }

    #[test]
    fn the_angle_is_clockwise_from_twelve() {
        let angle = |dx: f64, dy: f64| dx.atan2(-dy).to_degrees();
        assert_eq!(angle(0.0, -1.0), 0.0);
        assert_eq!(angle(1.0, 0.0), 90.0);
        assert_eq!(angle(0.0, 1.0), 180.0);
        assert_eq!(angle(-1.0, 0.0), -90.0);
    }
}
