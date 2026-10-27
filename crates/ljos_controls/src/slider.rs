//! `Slider` and `RangeSlider` (QtQuick.Templates): a number between two
//! ends, picked by moving a handle along a track.
//!
//! A slider holds a `value` between `from` and `to` and shows it as a
//! `position`, the same as a share of the track; a range slider holds two,
//! `first` and `second`, the first never past the second. The style places
//! the handles from `visualPosition`, which runs left to right and top to
//! bottom on the screen whichever way the slider is laid out.
//!
//! Documentation: doc.qt.io/qt-6/qml-qtquick-controls-slider.html and
//! doc.qt.io/qt-6/qml-qtquick-controls-rangeslider.html. What the
//! documentation leaves open was learned by running `qml6` (Qt 6.11.2) and
//! is pinned by `features/controls/slider*.feature` and
//! `range-slider.feature`:
//!
//! - A press puts the handle's middle under the pointer at once; the
//!   middle travels the track less the handle's own length.
//! - A step is `stepSize`, or a tenth when there is none, in the units of
//!   the value, and goes towards `to` for an increase.
//! - The arrow keys along the slider step it and say it moved; Home, End
//!   and the page keys do nothing.
//! - A slider that is not `live` shows the pointer's position while it is
//!   dragged and takes the value only when it is let go.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::OnceLock;

use ljos_a11y::{NodeData, Range, Role};
use ljos_engine::{
    Atom, Cx, Engine, EngineError, MetaRef, ObjectId, OverrideContent, OverrideId, PropIndex,
    QmlObject, QmlType, RestoreMode, Value, WriteKind, qml_impl,
};
use ljos_quick::flickable::{WheelItem, register_wheel_item};
use ljos_quick::focus::{FocusReason, force_active_focus, register_focus_scope};
use ljos_quick::keys::{KeyItem, key, register_key_item};
use ljos_quick::{
    Accessible, AccessibleAction, AccessibleAttached, AccessibleItem, Item, KeyEvent, PointerEvent,
    PointerItem, WheelEvent, flickable, is_item, register_accessible_item, register_pointer_item,
};
use ljos_render::kurbo;

use crate::control::{Control, focus_policy, hover, is_left, no_key, takes_input};
use crate::parts::{PartKind, manage_part};
use crate::scroll_bar::{orientation, snap_mode};
use crate::util::{check, flag, is_a, num, object, set_derived};

/// How far `increase()` and `decrease()` go when no `stepSize` was given,
/// in the units of the value.
pub(crate) const DEFAULT_STEP: f64 = 0.1;

pub(crate) fn snap_mode_enum() -> (&'static str, Vec<(&'static str, i64)>) {
    (
        "SnapMode",
        vec![
            ("NoSnap", i64::from(snap_mode::NO_SNAP)),
            ("SnapAlways", i64::from(snap_mode::SNAP_ALWAYS)),
            ("SnapOnRelease", i64::from(snap_mode::SNAP_ON_RELEASE)),
        ],
    )
}

fn slider_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![snap_mode_enum()]
}

/// `Slider`.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick.Templates",
    name = "Slider",
    extends = Control,
    enums = slider_enums
)]
pub struct Slider {
    #[qml(base)]
    pub control: Control,
    #[qml(notify)]
    pub from: f64,
    #[qml(notify)]
    pub to: f64,
    #[qml(notify)]
    pub value: f64,
    #[qml(notify)]
    pub step_size: f64,
    /// One of `SnapMode`.
    #[qml(notify, enumeration = "SnapMode")]
    pub snap_mode: i32,
    /// Whether the value follows the handle while it is dragged, or only
    /// once it is let go.
    #[qml(notify)]
    pub live: bool,
    /// A `Qt.Orientation`.
    #[qml(notify, enumeration = "Qt.Orientation")]
    pub orientation: i32,
    #[qml(notify, readonly)]
    pub horizontal: bool,
    #[qml(notify, readonly)]
    pub vertical: bool,
    /// The value as a share of the way from `from` to `to`.
    #[qml(notify, readonly)]
    pub position: f64,
    /// The position as it shows: from the left, or from the top.
    #[qml(notify, readonly)]
    pub visual_position: f64,
    #[qml(notify)]
    pub pressed: bool,
    #[qml(notify, holds = "Item")]
    pub handle: Option<ObjectId>,
    #[qml(notify, readonly)]
    pub implicit_handle_width: f64,
    #[qml(notify, readonly)]
    pub implicit_handle_height: f64,
    /// How far a touch goes before it is a drag; below nought, the
    /// platform's distance. Kept: a touch here is a press like any other.
    #[qml(notify, reset)]
    pub touch_drag_threshold: f64,
    #[qml(skip)]
    state: Rc<Held>,
}

impl Default for Slider {
    fn default() -> Self {
        Self {
            control: Control {
                focus_policy: focus_policy::STRONG,
                ..Control::default()
            },
            from: 0.0,
            to: 1.0,
            value: 0.0,
            step_size: 0.0,
            snap_mode: snap_mode::NO_SNAP,
            live: true,
            orientation: orientation::HORIZONTAL,
            horizontal: true,
            vertical: false,
            position: 0.0,
            visual_position: 0.0,
            pressed: false,
            handle: None,
            implicit_handle_width: 0.0,
            implicit_handle_height: 0.0,
            touch_drag_threshold: -1.0,
            state: Rc::default(),
        }
    }
}

/// One end of a `RangeSlider`: `first` or `second`.
#[derive(Clone, Debug, Default, QmlObject)]
#[qml(name = "RangeSliderNode", uncreatable)]
pub struct RangeSliderNode {
    #[qml(notify)]
    pub value: f64,
    #[qml(notify, readonly)]
    pub position: f64,
    #[qml(notify, readonly)]
    pub visual_position: f64,
    #[qml(notify, holds = "Item")]
    pub handle: Option<ObjectId>,
    #[qml(notify)]
    pub pressed: bool,
    #[qml(notify)]
    pub hovered: bool,
    #[qml(notify, readonly)]
    pub implicit_handle_width: f64,
    #[qml(notify, readonly)]
    pub implicit_handle_height: f64,
}

/// `RangeSlider`.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick.Templates",
    name = "RangeSlider",
    extends = Control,
    enums = slider_enums
)]
pub struct RangeSlider {
    #[qml(base)]
    pub control: Control,
    #[qml(notify)]
    pub from: f64,
    #[qml(notify)]
    pub to: f64,
    #[qml(notify)]
    pub step_size: f64,
    #[qml(notify, enumeration = "SnapMode")]
    pub snap_mode: i32,
    #[qml(notify)]
    pub live: bool,
    #[qml(notify, enumeration = "Qt.Orientation")]
    pub orientation: i32,
    #[qml(notify, readonly)]
    pub horizontal: bool,
    #[qml(notify, readonly)]
    pub vertical: bool,
    #[qml(notify, reset)]
    pub touch_drag_threshold: f64,
    #[qml(group)]
    pub first: RangeSliderNode,
    #[qml(group)]
    pub second: RangeSliderNode,
    #[qml(skip)]
    state: Rc<Held>,
}

impl Default for RangeSlider {
    fn default() -> Self {
        Self {
            control: Control {
                focus_policy: focus_policy::STRONG,
                ..Control::default()
            },
            from: 0.0,
            to: 1.0,
            step_size: 0.0,
            snap_mode: snap_mode::NO_SNAP,
            live: true,
            orientation: orientation::HORIZONTAL,
            horizontal: true,
            vertical: false,
            touch_drag_threshold: -1.0,
            first: RangeSliderNode::default(),
            second: RangeSliderNode {
                value: 1.0,
                position: 1.0,
                visual_position: 1.0,
                ..RangeSliderNode::default()
            },
            state: Rc::default(),
        }
    }
}

/// What a slider remembers between pointer events. Not QML-visible.
#[derive(Debug, Default)]
struct Held {
    /// The handle the pointer has, if it has one.
    knob: Cell<Option<usize>>,
    /// The layer on the held handle's position while a slider that is not
    /// live is dragged: the value waits for the release.
    drag: Cell<Option<OverrideId>>,
    /// The layer a handle that does not snap keeps after the pointer let
    /// go short of a step: it stays where it was left, as in qml6, until
    /// its value changes.
    left: Cell<Option<(usize, OverrideId)>>,
}

/// The properties of the track, shared by the handles on it.
#[derive(Clone, Copy, Debug)]
struct Track {
    from: PropIndex,
    to: PropIndex,
    step_size: PropIndex,
    snap_mode: PropIndex,
    live: PropIndex,
    orientation: PropIndex,
    horizontal: PropIndex,
    vertical: PropIndex,
}

/// The properties of one handle.
#[derive(Clone, Copy, Debug)]
struct Knob {
    value: PropIndex,
    position: PropIndex,
    visual_position: PropIndex,
    pressed: PropIndex,
    handle: PropIndex,
    implicit: [PropIndex; 2],
    /// Where the handle says it is hovered; a slider says it of itself.
    hovered: Option<PropIndex>,
}

/// A slider or a range slider.
#[derive(Clone, Copy, Debug)]
struct Kind {
    track: Track,
    knobs: &'static [Knob],
}

const SLIDER_TRACK: Track = Track {
    from: Slider::FROM.index,
    to: Slider::TO.index,
    step_size: Slider::STEP_SIZE.index,
    snap_mode: Slider::SNAP_MODE.index,
    live: Slider::LIVE.index,
    orientation: Slider::ORIENTATION.index,
    horizontal: Slider::HORIZONTAL.index,
    vertical: Slider::VERTICAL.index,
};

const SLIDER_KNOBS: [Knob; 1] = [Knob {
    value: Slider::VALUE.index,
    position: Slider::POSITION.index,
    visual_position: Slider::VISUAL_POSITION.index,
    pressed: Slider::PRESSED.index,
    handle: Slider::HANDLE.index,
    implicit: [
        Slider::IMPLICIT_HANDLE_WIDTH.index,
        Slider::IMPLICIT_HANDLE_HEIGHT.index,
    ],
    hovered: None,
}];

const RANGE_TRACK: Track = Track {
    from: RangeSlider::FROM.index,
    to: RangeSlider::TO.index,
    step_size: RangeSlider::STEP_SIZE.index,
    snap_mode: RangeSlider::SNAP_MODE.index,
    live: RangeSlider::LIVE.index,
    orientation: RangeSlider::ORIENTATION.index,
    horizontal: RangeSlider::HORIZONTAL.index,
    vertical: RangeSlider::VERTICAL.index,
};

fn range_knobs() -> &'static [Knob; 2] {
    static KNOBS: OnceLock<[Knob; 2]> = OnceLock::new();
    KNOBS.get_or_init(|| {
        let meta = RangeSlider::static_meta();
        let of = |group: &str| {
            let p = |name: &str| {
                meta.property_index(&format!("{group}.{name}"))
                    .expect("the range slider has the group")
            };
            Knob {
                value: p("value"),
                position: p("position"),
                visual_position: p("visualPosition"),
                pressed: p("pressed"),
                handle: p("handle"),
                implicit: [p("implicitHandleWidth"), p("implicitHandleHeight")],
                hovered: Some(p("hovered")),
            }
        };
        [of("first"), of("second")]
    })
}

fn kind_of(engine: &Engine, obj: ObjectId) -> Option<Kind> {
    if is_a::<Slider>(engine, obj) {
        Some(Kind {
            track: SLIDER_TRACK,
            knobs: &SLIDER_KNOBS,
        })
    } else if is_a::<RangeSlider>(engine, obj) {
        Some(Kind {
            track: RANGE_TRACK,
            knobs: range_knobs(),
        })
    } else {
        None
    }
}

fn held_of(engine: &Engine, obj: ObjectId) -> Option<Rc<Held>> {
    if let Ok(st) = engine.with::<Slider, _>(obj, |s| s.state.clone()) {
        return Some(st);
    }
    engine.with::<RangeSlider, _>(obj, |s| s.state.clone()).ok()
}

/// A write that is the template's and not the author's: a binding on the
/// property stays, as it does when Qt's own code sets one.
fn put(engine: &Engine, obj: ObjectId, prop: PropIndex, v: Value) {
    if engine.is_alive(obj) {
        check(engine, "slider", set_derived(engine, obj, prop, v));
    }
}

// ---------------------------------------------------------------- values

/// The two ends of a value, in the order they were given.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Span {
    pub from: f64,
    pub to: f64,
}

impl Span {
    pub fn read(engine: &Engine, obj: ObjectId, from: PropIndex, to: PropIndex) -> Self {
        Self {
            from: num(engine, obj, from),
            to: num(engine, obj, to),
        }
    }

    pub fn lo(self) -> f64 {
        self.from.min(self.to)
    }

    pub fn hi(self) -> f64 {
        self.from.max(self.to)
    }

    pub fn clamp(self, v: f64) -> f64 {
        if v.is_nan() {
            return self.from;
        }
        v.clamp(self.lo(), self.hi())
    }

    /// The value as a share of the way from `from` to `to`.
    pub fn position(self, v: f64) -> f64 {
        let width = self.to - self.from;
        if width == 0.0 || !width.is_finite() {
            return 0.0;
        }
        ((v - self.from) / width).clamp(0.0, 1.0)
    }

    pub fn value_at(self, position: f64) -> f64 {
        self.from + position * (self.to - self.from)
    }

    /// `valueAt(position)` as qml6 gives it: with a step, the nearest
    /// whole number of steps from `from`, also past the ends (28 at the
    /// end of 0 to 30 in steps of 7).
    pub fn stepped_value_at(self, position: f64, step: f64) -> f64 {
        let v = self.value_at(position);
        if step > 0.0 && step.is_finite() {
            self.from + ((v - self.from) / step).round() * step
        } else {
            v
        }
    }

    /// The nearest position whose value is a whole number of steps from
    /// `from`.
    pub fn snap_position(self, position: f64, step: f64) -> f64 {
        let width = (self.to - self.from).abs();
        if step <= 0.0 || width == 0.0 {
            return position;
        }
        let share = step / width;
        ((position / share).round() * share).clamp(0.0, 1.0)
    }
}

fn span(engine: &Engine, obj: ObjectId, t: Track) -> Span {
    Span::read(engine, obj, t.from, t.to)
}

fn stepped_value_at(engine: &Engine, obj: ObjectId, t: Track, position: f64) -> f64 {
    span(engine, obj, t).stepped_value_at(position, num(engine, obj, t.step_size))
}

fn step_of(engine: &Engine, obj: ObjectId, t: Track) -> f64 {
    let step = num(engine, obj, t.step_size);
    if step > 0.0 { step } else { DEFAULT_STEP }
}

/// The least and the greatest a handle may be: the track's ends, and on a
/// range slider the other handle.
fn bounds(engine: &Engine, obj: ObjectId, kind: Kind, at: usize) -> (f64, f64) {
    let s = span(engine, obj, kind.track);
    let (mut a, mut b) = (s.from, s.to);
    if kind.knobs.len() == 2 {
        if at == 0 {
            b = num(engine, obj, kind.knobs[1].value);
        } else {
            a = num(engine, obj, kind.knobs[0].value);
        }
        // The other handle may itself be outside the track for a moment.
        a = s.clamp(a);
        b = s.clamp(b);
    }
    (a.min(b), a.max(b))
}

fn kept(engine: &Engine, obj: ObjectId, kind: Kind, at: usize, v: f64) -> f64 {
    let (lo, hi) = engine.untrack(|| bounds(engine, obj, kind, at));
    if v.is_nan() {
        return lo;
    }
    v.clamp(lo, hi)
}

/// Brings each value back between its bounds, the first handle first.
fn keep_in_range(engine: &Engine, obj: ObjectId, kind: Kind) {
    for (at, knob) in kind.knobs.iter().enumerate() {
        let v = engine.untrack(|| num(engine, obj, knob.value));
        let k = kept(engine, obj, kind, at, v);
        if k != v {
            put(engine, obj, knob.value, Value::Real(k));
        }
    }
}

fn set_value(engine: &Engine, obj: ObjectId, kind: Kind, at: usize, v: f64) {
    let v = kept(engine, obj, kind, at, v);
    put(engine, obj, kind.knobs[at].value, Value::Real(v));
}

/// One step towards `to` (or `from`), as `increase()` and `decrease()`
/// take it. Whether the value moved.
fn step(engine: &Engine, obj: ObjectId, kind: Kind, at: usize, up: bool) -> bool {
    let (v, s) = engine.untrack(|| {
        (
            num(engine, obj, kind.knobs[at].value),
            step_of(engine, obj, kind.track),
        )
    });
    let to = if up { v + s } else { v - s };
    set_value(engine, obj, kind, at, to);
    engine.untrack(|| num(engine, obj, kind.knobs[at].value)) != v
}

/// A step by the user, which says the handle moved.
fn user_step(engine: &Engine, obj: ObjectId, kind: Kind, at: usize, up: bool) {
    if step(engine, obj, kind, at, up) {
        moved_signal(engine, obj);
    }
}

fn moved_signal(engine: &Engine, obj: ObjectId) {
    check(engine, "moved", engine.emit_by_name(obj, "moved", &[]));
}

fn is_horizontal(engine: &Engine, obj: ObjectId, t: Track) -> bool {
    num(engine, obj, t.orientation) as i32 != orientation::VERTICAL
}

fn init(engine: &Engine, obj: ObjectId, kind: Kind) -> Result<(), EngineError> {
    let t = kind.track;
    engine.bind_initial(obj, t.horizontal, move |e| {
        Value::Bool(is_horizontal(e, obj, t))
    })?;
    engine.bind_initial(obj, t.vertical, move |e| {
        Value::Bool(!is_horizontal(e, obj, t))
    })?;
    // The declared values came in the order they were written, so the
    // range is whole only now.
    keep_in_range(engine, obj, kind);
    for p in [t.from, t.to] {
        engine.watch(obj, p, move |e, _| keep_in_range(e, obj, kind))?;
    }
    // A value that is written is the one held back by the other handle,
    // not the other way round.
    for (at, knob) in kind.knobs.iter().enumerate() {
        engine.watch(obj, knob.value, move |e, v| {
            if let Some(st) = held_of(e, obj) {
                pick_up(e, obj, kind, &st, Some(at));
            }
            let v = v.to_number();
            let k = kept(e, obj, kind, at, v);
            if k != v {
                put(e, obj, kind.knobs[at].value, Value::Real(k));
            }
        })?;
    }
    for knob in kind.knobs {
        let k = *knob;
        engine.bind_initial(obj, k.position, move |e| {
            Value::Real(span(e, obj, t).position(num(e, obj, k.value)))
        })?;
        engine.bind_initial(obj, k.visual_position, move |e| {
            let p = num(e, obj, k.position);
            let flipped = !is_horizontal(e, obj, t) || flag(e, obj, Control::MIRRORED.index);
            Value::Real(if flipped { 1.0 - p } else { p })
        })?;
        manage_part(engine, obj, k.handle, PartKind::Free)?;
        for (i, size) in [Item::IMPLICIT_WIDTH.index, Item::IMPLICIT_HEIGHT.index]
            .into_iter()
            .enumerate()
        {
            engine.bind_initial(obj, k.implicit[i], move |e| {
                match object(e, obj, k.handle) {
                    Some(part) if is_item(e, part) => Value::Real(num(e, part, size)),
                    _ => Value::Real(0.0),
                }
            })?;
        }
    }
    // A press does not outlive the slider's being usable.
    for p in [Item::ENABLED.index, Item::VISIBLE.index] {
        engine.watch(obj, p, move |e, v| {
            if !v.to_boolean() {
                cancel(e, obj);
            }
        })?;
    }
    Ok(())
}

#[qml_impl]
impl Slider {
    /// The user moved the handle: with the pointer, a key or the wheel.
    #[qml(signal)]
    fn moved();

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let kind = kind_of(cx.engine, cx.this).expect("a slider");
        init(cx.engine, cx.this, kind)
    }

    /// `increase()`: one step towards `to`. Not a move by the user.
    #[qml(invokable)]
    fn increase(cx: &Cx) {
        if let Some(kind) = kind_of(cx.engine, cx.this) {
            step(cx.engine, cx.this, kind, 0, true);
        }
    }

    #[qml(invokable)]
    fn decrease(cx: &Cx) {
        if let Some(kind) = kind_of(cx.engine, cx.this) {
            step(cx.engine, cx.this, kind, 0, false);
        }
    }

    /// `valueAt(position)`: the value at a share of the track.
    #[qml(invokable, name = "valueAt")]
    fn value_at(cx: &Cx, position: f64) -> f64 {
        stepped_value_at(cx.engine, cx.this, SLIDER_TRACK, position)
    }
}

#[qml_impl]
impl RangeSlider {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, obj) = (cx.engine, cx.this);
        let kind = kind_of(e, obj).expect("a range slider");
        init(e, obj, kind)?;
        for (at, knob) in range_knobs().iter().enumerate() {
            let k = *knob;
            // Tab goes from one handle to the other.
            let tab_stop = move |e: &Engine| {
                if let Some(h) = e
                    .untrack(|| object(e, obj, k.handle))
                    .filter(|h| is_item(e, *h))
                {
                    if crate::util::is_unset(e, h, Item::ACTIVE_FOCUS_ON_TAB.index) {
                        let r =
                            e.write_derived(h, Item::ACTIVE_FOCUS_ON_TAB.index, Value::Bool(true));
                        check(e, "handle", r);
                    }
                    check(e, "handle", expose_handle(e, obj, at, h));
                }
            };
            tab_stop(e);
            e.watch(obj, k.handle, move |e, _| tab_stop(e))?;
        }
        // The keyboard that comes to the slider goes to a handle.
        e.watch(obj, Item::ACTIVE_FOCUS.index, move |e, v| {
            if !v.to_boolean() {
                return;
            }
            let knobs = range_knobs();
            let handles: Vec<ObjectId> = knobs
                .iter()
                .filter_map(|k| e.untrack(|| object(e, obj, k.handle)))
                .collect();
            if handles
                .iter()
                .any(|h| e.untrack(|| flag(e, *h, Item::ACTIVE_FOCUS.index)))
            {
                return;
            }
            if let Some(first) = handles.first() {
                force_active_focus(e, *first, ljos_quick::focus::focus_reason(e, obj));
            }
        })?;
        Ok(())
    }

    /// `setValues(firstValue, secondValue)`: both at once, so that neither
    /// is held back by where the other was.
    #[qml(invokable, name = "setValues")]
    fn set_values(cx: &Cx, first: f64, second: f64) {
        let (e, obj) = (cx.engine, cx.this);
        let knobs = range_knobs();
        let s = e.untrack(|| span(e, obj, RANGE_TRACK));
        let a = s.clamp(first);
        let b = s.clamp(second);
        let (a, b) = if (s.to >= s.from) == (a <= b) {
            (a, b)
        } else {
            (a, a)
        };
        put(e, obj, knobs[0].value, Value::Real(a));
        put(e, obj, knobs[1].value, Value::Real(b));
    }

    #[qml(invokable, name = "valueAt")]
    fn value_at(cx: &Cx, position: f64) -> f64 {
        stepped_value_at(cx.engine, cx.this, RANGE_TRACK, position)
    }
}

// ----------------------------------------------------------- the pointer

/// Where the handle's middle would be with the pointer at `at`, as a
/// position on the track (not the screen's way round).
fn position_at(engine: &Engine, obj: ObjectId, kind: Kind, at: usize, p: kurbo::Point) -> f64 {
    engine.untrack(|| {
        let horizontal = is_horizontal(engine, obj, kind.track);
        let handle = object(engine, obj, kind.knobs[at].handle).filter(|h| is_item(engine, *h));
        let length = |size: PropIndex| handle.map_or(0.0, |h| num(engine, h, size));
        let (lead, room, along, handle_len) = if horizontal {
            (
                num(engine, obj, Control::LEFT_PADDING.index),
                num(engine, obj, Control::AVAILABLE_WIDTH.index),
                p.x,
                length(Item::WIDTH.index),
            )
        } else {
            (
                num(engine, obj, Control::TOP_PADDING.index),
                num(engine, obj, Control::AVAILABLE_HEIGHT.index),
                p.y,
                length(Item::HEIGHT.index),
            )
        };
        let travel = room - handle_len;
        let visual = if travel > 0.0 {
            ((along - lead - handle_len / 2.0) / travel).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let flipped = !horizontal || flag(engine, obj, Control::MIRRORED.index);
        if flipped { 1.0 - visual } else { visual }
    })
}

/// The handle nearer the pointer; between two at one place, the one on
/// the pointer's side.
fn nearest(engine: &Engine, obj: ObjectId, kind: Kind, p: kurbo::Point) -> usize {
    if kind.knobs.len() < 2 {
        return 0;
    }
    let at = position_at(engine, obj, kind, 0, p);
    let [a, b] = [0, 1].map(|i| engine.untrack(|| num(engine, obj, kind.knobs[i].position)));
    let (da, db) = ((at - a).abs(), (at - b).abs());
    if da < db || (da == db && at < a) {
        0
    } else {
        1
    }
}

fn snaps(engine: &Engine, obj: ObjectId, t: Track, mode: i32) -> bool {
    engine.untrack(|| num(engine, obj, t.snap_mode)) as i32 == mode
}

/// Puts the held handle where the pointer has it. `release` says the
/// pointer let go there.
fn drag_to(engine: &Engine, obj: ObjectId, kind: Kind, st: &Held, p: kurbo::Point, release: bool) {
    let Some(at) = st.knob.get() else {
        return;
    };
    let t = kind.track;
    let knob = kind.knobs[at];
    let (s, step, live) = engine.untrack(|| {
        (
            span(engine, obj, t),
            num(engine, obj, t.step_size),
            flag(engine, obj, t.live),
        )
    });
    let free = position_at(engine, obj, kind, at, p);
    let snapped = if snaps(engine, obj, t, snap_mode::NO_SNAP) {
        free
    } else {
        s.snap_position(free, step)
    };
    // Snapping on release snaps the value all along (qml6 shows it), and
    // leaves only the handle free until the pointer lets go.
    let shown_at = if release || snaps(engine, obj, t, snap_mode::SNAP_ALWAYS) {
        snapped
    } else {
        free
    };
    let before = engine.untrack(|| num(engine, obj, knob.position));
    // A slider that does not snap still goes in steps, as its valueAt()
    // does; only its handle is free (qml6).
    let value = if snaps(engine, obj, t, snap_mode::NO_SNAP) {
        s.stepped_value_at(free, step)
    } else {
        s.value_at(snapped)
    };
    if live || release {
        set_value(engine, obj, kind, at, value);
    }
    // The handle shows where it is, short of the other one.
    let shown = s.position(kept(engine, obj, kind, at, s.value_at(shown_at)));
    let settled = engine.untrack(|| s.position(num(engine, obj, knob.value)));
    if (live || release) && shown == settled {
        if let Some(layer) = st.drag.take() {
            let _ = engine.remove_override(obj, knob.position, layer, RestoreMode::BindingOrValue);
        }
    } else {
        let shown = Value::Real(shown);
        let r = match st.drag.get() {
            Some(layer) => {
                engine.write_internal(obj, knob.position, shown, WriteKind::Override(layer))
            }
            None => engine
                .push_override(obj, knob.position, OverrideContent::Value(shown))
                .map(|id| st.drag.set(Some(id))),
        };
        check(engine, "position", r);
    }
    if engine.untrack(|| num(engine, obj, knob.position)) != before {
        moved_signal(engine, obj);
    }
}

fn press(engine: &Engine, obj: ObjectId, ev: &PointerEvent, p: kurbo::Point) -> bool {
    if !takes_input(engine, obj) || !is_left(ev) {
        return false;
    }
    let (Some(kind), Some(st)) = (kind_of(engine, obj), held_of(engine, obj)) else {
        return false;
    };
    let at = nearest(engine, obj, kind, p);
    pick_up(engine, obj, kind, &st, None);
    st.knob.set(Some(at));
    let policy = engine.untrack(|| num(engine, obj, Control::FOCUS_POLICY.index)) as i32;
    if policy & focus_policy::CLICK != 0 {
        // On a range slider it is the handle that has the keyboard.
        let target = if kind.knobs.len() > 1 {
            engine.untrack(|| object(engine, obj, kind.knobs[at].handle))
        } else {
            None
        };
        force_active_focus(engine, target.unwrap_or(obj), FocusReason::Mouse);
    }
    // The Flickable the slider is on would take a press that moves this
    // far for a drag of its content.
    flickable::keep_pointer_grab(engine, obj, true);
    put(engine, obj, kind.knobs[at].pressed, Value::Bool(true));
    drag_to(engine, obj, kind, &st, p, false);
    true
}

fn moved(engine: &Engine, obj: ObjectId, _ev: &PointerEvent, p: kurbo::Point) {
    if let (Some(kind), Some(st)) = (kind_of(engine, obj), held_of(engine, obj))
        && st.knob.get().is_some()
    {
        drag_to(engine, obj, kind, &st, p, false);
    }
}

/// Gives a handle left short of a step back to its value: on the next
/// press, or once the value of that handle (`Some(at)`) changes.
fn pick_up(engine: &Engine, obj: ObjectId, kind: Kind, st: &Held, at: Option<usize>) {
    let Some((left, layer)) = st.left.get() else {
        return;
    };
    if at.is_some_and(|at| at != left) {
        return;
    }
    st.left.set(None);
    let p = kind.knobs[left].position;
    let _ = engine.remove_override(obj, p, layer, RestoreMode::BindingOrValue);
}

fn let_go(engine: &Engine, obj: ObjectId, kind: Kind, st: &Held) {
    let Some(at) = st.knob.take() else {
        return;
    };
    if let Some(layer) = st.drag.take() {
        if snaps(engine, obj, kind.track, snap_mode::NO_SNAP) {
            st.left.set(Some((at, layer)));
        } else {
            let p = kind.knobs[at].position;
            let _ = engine.remove_override(obj, p, layer, RestoreMode::BindingOrValue);
        }
    }
    flickable::keep_pointer_grab(engine, obj, false);
    put(engine, obj, kind.knobs[at].pressed, Value::Bool(false));
}

fn release(engine: &Engine, obj: ObjectId, _ev: &PointerEvent, p: kurbo::Point) {
    let (Some(kind), Some(st)) = (kind_of(engine, obj), held_of(engine, obj)) else {
        return;
    };
    if st.knob.get().is_none() {
        return;
    }
    drag_to(engine, obj, kind, &st, p, true);
    let_go(engine, obj, kind, &st);
}

fn cancel(engine: &Engine, obj: ObjectId) {
    if let (Some(kind), Some(st)) = (kind_of(engine, obj), held_of(engine, obj)) {
        let_go(engine, obj, kind, &st);
    }
}

fn slider_hover(engine: &Engine, obj: ObjectId, over: bool) {
    hover(engine, obj, over);
    if !over && let Some(kind) = kind_of(engine, obj) {
        for k in kind.knobs {
            if let Some(h) = k.hovered {
                put(engine, obj, h, Value::Bool(false));
            }
        }
    }
}

/// The pointer moved over a range slider: the handle it is on is hovered.
fn hover_moved(engine: &Engine, obj: ObjectId, p: kurbo::Point) {
    let Some(kind) = kind_of(engine, obj) else {
        return;
    };
    let wanted = engine.untrack(|| flag(engine, obj, Control::HOVER_ENABLED.index));
    for k in kind.knobs {
        let Some(hovered) = k.hovered else { continue };
        let on = wanted
            && engine.untrack(|| {
                object(engine, obj, k.handle).is_some_and(|h| {
                    let x = num(engine, h, Item::X.index);
                    let y = num(engine, h, Item::Y.index);
                    let w = num(engine, h, Item::WIDTH.index);
                    let ht = num(engine, h, Item::HEIGHT.index);
                    p.x >= x && p.x < x + w && p.y >= y && p.y < y + ht
                })
            });
        put(engine, obj, hovered, Value::Bool(on));
    }
}

// ------------------------------------------------------ keys and wheel

/// The handle the keyboard is on: on a range slider the one that has it.
fn focused_knob(engine: &Engine, obj: ObjectId, kind: Kind) -> usize {
    if kind.knobs.len() < 2 {
        return 0;
    }
    engine.untrack(|| {
        kind.knobs
            .iter()
            .position(|k| {
                object(engine, obj, k.handle)
                    .is_some_and(|h| flag(engine, h, Item::ACTIVE_FOCUS.index))
            })
            .unwrap_or(0)
    })
}

fn key_press(engine: &Engine, obj: ObjectId, ev: &KeyEvent) -> bool {
    let Some(kind) = kind_of(engine, obj) else {
        return false;
    };
    if !takes_input(engine, obj) {
        return false;
    }
    let (horizontal, mirrored) = engine.untrack(|| {
        (
            is_horizontal(engine, obj, kind.track),
            flag(engine, obj, Control::MIRRORED.index),
        )
    });
    let up = match ev.key {
        key::RIGHT if horizontal => !mirrored,
        key::LEFT if horizontal => mirrored,
        key::UP if !horizontal => true,
        key::DOWN if !horizontal => false,
        _ => return false,
    };
    let at = focused_knob(engine, obj, kind);
    user_step(engine, obj, kind, at, up);
    true
}

fn wheel(engine: &Engine, obj: ObjectId, ev: &WheelEvent, _p: kurbo::Point) -> bool {
    let wanted = engine.untrack(|| flag(engine, obj, Control::WHEEL_ENABLED.index));
    let Some(kind) = kind_of(engine, obj) else {
        return false;
    };
    if !wanted || !takes_input(engine, obj) {
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
    // Part of a notch is a step too, so that a fine wheel is not dead.
    let notches = ((turned.abs() / WheelEvent::NOTCH).trunc() as i32).max(1);
    let at = focused_knob(engine, obj, kind);
    let mut any = false;
    for _ in 0..notches {
        any |= step(engine, obj, kind, at, turned > 0.0);
    }
    if any {
        moved_signal(engine, obj);
    }
    true
}

// ------------------------------------------------- assistive technology

fn describe(engine: &Engine, obj: ObjectId, data: &mut NodeData) {
    let Some(kind) = kind_of(engine, obj) else {
        return;
    };
    let s = span(engine, obj, kind.track);
    // A range slider tells the first handle, or the one with the keyboard.
    let at = focused_knob(engine, obj, kind);
    data.range = Some(Range {
        value: num(engine, obj, kind.knobs[at].value),
        min: s.lo(),
        max: s.hi(),
        step: step_of(engine, obj, kind.track),
    });
}

fn act(engine: &Engine, obj: ObjectId, action: AccessibleAction) -> bool {
    let Some(kind) = kind_of(engine, obj) else {
        return false;
    };
    if !takes_input(engine, obj) {
        return false;
    }
    let at = focused_knob(engine, obj, kind);
    let before = engine.untrack(|| num(engine, obj, kind.knobs[at].value));
    // Towards the greater number, whichever end that is.
    let rising = engine.untrack(|| {
        let s = span(engine, obj, kind.track);
        s.to >= s.from
    });
    match action {
        AccessibleAction::Increase => step(engine, obj, kind, at, rising),
        AccessibleAction::Decrease => step(engine, obj, kind, at, !rising),
        AccessibleAction::SetValue(v) if v.is_finite() => {
            set_value(engine, obj, kind, at, v);
            true
        }
        _ => return false,
    };
    let moved = engine.untrack(|| num(engine, obj, kind.knobs[at].value)) != before;
    if moved {
        moved_signal(engine, obj);
    }
    moved
}

/// Makes handle `at` of a range slider a slider of its own to an
/// assistive technology, as it takes the keyboard of its own: named after
/// the range slider, and stepped through its `Accessible` signals, since
/// a handle is any item the style makes and has no type to hook. What it
/// tells of its value is the style's to give, in the handle's own
/// `value`, `minimumValue`, `maximumValue` and `stepSize`, as the
/// `Accessible` documentation has a slider do. The author's own role,
/// name or handlers on the handle win.
fn expose_handle(e: &Engine, obj: ObjectId, at: usize, h: ObjectId) -> Result<(), EngineError> {
    let attacher = MetaRef::Static(Accessible::static_meta());
    let att = e.attached_object(h, &attacher)?;
    let role = AccessibleAttached::ROLE.index;
    if crate::util::is_unset(e, att, role) {
        e.write_derived(att, role, Value::Int(Role::Slider.qt_value()))?;
    }
    let control = e.attached_object(obj, &attacher)?;
    let end = if at == 0 { "start" } else { "end" };
    crate::util::bind_default(e, att, AccessibleAttached::NAME.index, move |e| {
        let name = e
            .read(control, AccessibleAttached::NAME.index)
            .to_js_string();
        let name = if name.is_empty() {
            end.to_string()
        } else {
            format!("{name} {end}")
        };
        Value::String(Atom::from(name.as_str()))
    })?;
    for (signal, up) in [("increaseAction", true), ("decreaseAction", false)] {
        // An author who handles the request on the handle does it instead.
        let taken = e
            .signal_index(att, signal)
            .is_ok_and(|s| e.signal_connected(att, s));
        if taken {
            continue;
        }
        e.connect_by_name(att, signal, move |e, _| {
            let Some(kind) = kind_of(e, obj).filter(|_| takes_input(e, obj)) else {
                return;
            };
            // Towards the greater number, whichever end that is.
            let rising = e.untrack(|| {
                let s = span(e, obj, kind.track);
                s.to >= s.from
            });
            user_step(e, obj, kind, at, if up { rising } else { !rising });
        })?;
    }
    Ok(())
}

pub(crate) fn register(engine: &Engine) {
    for meta in [Slider::static_meta(), RangeSlider::static_meta()] {
        let meta = || MetaRef::Static(meta);
        register_pointer_item(
            engine,
            meta(),
            PointerItem {
                press,
                moved,
                release,
                cancel,
                hover: slider_hover,
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
    let range = || MetaRef::Static(RangeSlider::static_meta());
    // The handles have the keyboard on the slider's behalf.
    register_focus_scope(engine, range());
    ljos_quick::register_hover_move(engine, range(), hover_moved);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_position_is_a_share_of_the_way_from_from_to_to() {
        let s = Span {
            from: 10.0,
            to: 0.0,
        };
        assert!((s.position(2.0) - 0.8).abs() < 1e-12);
        assert_eq!(s.value_at(0.5), 5.0);
        assert_eq!(s.clamp(-3.0), 0.0);
        assert_eq!(Span { from: 1.0, to: 1.0 }.position(1.0), 0.0);
    }

    #[test]
    fn a_snapped_position_is_a_whole_number_of_steps() {
        let s = Span { from: 0.0, to: 1.0 };
        assert_eq!(s.snap_position(0.3125, 0.25), 0.25);
        assert_eq!(s.snap_position(0.6, 0.25), 0.5);
        assert_eq!(s.snap_position(0.6, 0.0), 0.6);
        let tens = Span {
            from: 0.0,
            to: 100.0,
        };
        assert!((tens.snap_position(0.33, 10.0) - 0.3).abs() < 1e-12);
    }

    #[test]
    fn a_stepped_value_counts_steps_from_from_past_the_ends() {
        let s = Span {
            from: 0.0,
            to: 30.0,
        };
        assert_eq!(s.stepped_value_at(1.0, 7.0), 28.0);
        assert_eq!(s.stepped_value_at(-0.5, 7.0), -14.0);
        assert_eq!(s.stepped_value_at(0.6875, 2.0), 20.0);
        assert_eq!(s.stepped_value_at(0.6875, 0.0), 20.625);
        let back = Span {
            from: 30.0,
            to: 0.0,
        };
        assert_eq!(back.stepped_value_at(0.5, 7.0), 16.0);
        let half = Span {
            from: 0.5,
            to: 30.5,
        };
        assert_eq!(half.stepped_value_at(0.6875, 1.0), 21.5);
    }
}
