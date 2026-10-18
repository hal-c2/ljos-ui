//! `SpinBox` and `DoubleSpinBox` (QtQuick.Templates): a number between two
//! bounds, stepped with a button at either side, the arrow keys or the
//! wheel, and typed when the box is editable. A `SpinBox` holds a whole
//! number, a `DoubleSpinBox` one with `decimals` places; one core serves
//! both, reading each type's properties through [`Props`].
//!
//! Documentation: doc.qt.io/qt-6/qml-qtquick-controls-spinbox.html and
//! doc.qt.io/qt-6/qml-qtquick-controls-doublespinbox.html; pinned by
//! `features/controls/spin-box*.feature` and
//! `features/controls/double-spin-box.feature`, which say what running Qt
//! shows where the documentation is silent:
//!
//! - The box is a focus scope and the number in it is the stop of the Tab
//!   key. A script that gives the box the keyboard gives it to the number
//!   only when the box is editable.
//! - What is typed is `displayText` and not the value until Return, Enter
//!   or the keyboard leaving; a `live` box takes each keystroke. Either
//!   reads the text again whether or not it was changed.
//! - A step goes from the value, never from what was typed, and where
//!   there is nowhere to go nothing happens at all.
//! - A held button steps once after [`REPEAT_DELAY`] and then every
//!   [`REPEAT_INTERVAL`]; letting go of it then adds no step.
//! - A value a script gives a `DoubleSpinBox`, and its ends, are rounded
//!   to its decimals; a step is added as it is, and only its text is
//!   rounded. Other decimals read the number that was shown again.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::OnceLock;

use ljos_a11y::{NodeData, Range, Role};
use ljos_engine::{
    Atom, Cx, Engine, EngineError, MetaObject, MetaRef, ObjectId, PropIndex, QmlObject, QmlType,
    Value, WriteKind, qml_impl,
};
use ljos_i18n::LocaleInfo;
use ljos_quick::flickable::{WheelItem, register_wheel_item};
use ljos_quick::focus::{focus_reason, force_active_focus, register_focus_scope};
use ljos_quick::keys::{KeyItem, key, register_key_item};
use ljos_quick::text_input::input_method_hint;
use ljos_quick::{
    AccessibleAction, AccessibleItem, DoubleValidator, IntValidator, Item, KeyEvent, PointerEvent,
    PointerItem, TextEditor, TextInput, WheelEvent, is_item, register_accessible_item,
    register_hover_move, register_pointer_item,
};
use ljos_render::kurbo;

use crate::control::{Control, hover, is_left, takes_input};
use crate::parts::{PartKind, manage_part};
use crate::ticker;
use crate::util::{check, flag, is_a, is_unset, num, object, set_computed};

/// How long a button is held before the value begins to run, and how
/// often it steps then. Qt shows a first step between a third and half a
/// second and about ten a second after it; its exact numbers are the
/// platform's.
const REPEAT_DELAY: i32 = 400;
const REPEAT_INTERVAL: i32 = 100;

/// The most decimals a `DoubleSpinBox` has: every place a double can
/// carry (doc.qt.io gives `DBL_MAX_10_EXP + DBL_DIG`).
const MAX_DECIMALS: i32 = 323;

/// The `up` and `down` groups of a spin box: one button each.
#[derive(Clone, Debug, Default, QmlObject)]
#[qml(name = "SpinButton", uncreatable)]
pub struct SpinButton {
    #[qml(notify)]
    pub pressed: bool,
    /// The item that is the button; the style makes it.
    #[qml(notify, holds = "Item")]
    pub indicator: Option<ObjectId>,
    #[qml(notify)]
    pub hovered: bool,
    #[qml(notify, readonly)]
    pub implicit_indicator_width: f64,
    #[qml(notify, readonly)]
    pub implicit_indicator_height: f64,
}

/// The properties of one button on the box.
#[derive(Clone, Copy, Debug)]
struct ButtonProps {
    pressed: PropIndex,
    indicator: PropIndex,
    hovered: PropIndex,
    implicit: [PropIndex; 2],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Side {
    Up,
    Down,
}

const SIDES: [Side; 2] = [Side::Up, Side::Down];

/// Where a type of box keeps what the core reads and writes. The two types
/// name their properties alike; only a `SpinBox` is `live`, and only a
/// `DoubleSpinBox` has `decimals`.
#[derive(Debug)]
struct Props {
    from: PropIndex,
    to: PropIndex,
    value: PropIndex,
    step_size: PropIndex,
    editable: PropIndex,
    wrap: PropIndex,
    live: Option<PropIndex>,
    /// `Some` for a box of decimal numbers, `None` for whole ones.
    decimals: Option<PropIndex>,
    validator: PropIndex,
    text_from_value: PropIndex,
    value_from_text: PropIndex,
    display_text: PropIndex,
    input_method_composing: PropIndex,
    buttons: [ButtonProps; 2],
}

impl Props {
    fn of(meta: &'static MetaObject) -> Self {
        let p = |name: &str| {
            meta.property_index(name)
                .expect("a spin box has the property")
        };
        let button = |group: &str| ButtonProps {
            pressed: p(&format!("{group}.pressed")),
            indicator: p(&format!("{group}.indicator")),
            hovered: p(&format!("{group}.hovered")),
            implicit: [
                p(&format!("{group}.implicitIndicatorWidth")),
                p(&format!("{group}.implicitIndicatorHeight")),
            ],
        };
        Self {
            from: p("from"),
            to: p("to"),
            value: p("value"),
            step_size: p("stepSize"),
            editable: p("editable"),
            wrap: p("wrap"),
            live: meta.property_index("live"),
            decimals: meta.property_index("decimals"),
            validator: p("validator"),
            text_from_value: p("textFromValue"),
            value_from_text: p("valueFromText"),
            display_text: p("displayText"),
            input_method_composing: p("inputMethodComposing"),
            buttons: [button("up"), button("down")],
        }
    }

    fn button(&self, side: Side) -> &ButtonProps {
        match side {
            Side::Up => &self.buttons[0],
            Side::Down => &self.buttons[1],
        }
    }
}

/// The properties of the type of `obj`.
fn props(engine: &Engine, obj: ObjectId) -> &'static Props {
    static WHOLE: OnceLock<Props> = OnceLock::new();
    static DECIMAL: OnceLock<Props> = OnceLock::new();
    if is_a::<DoubleSpinBox>(engine, obj) {
        DECIMAL.get_or_init(|| Props::of(DoubleSpinBox::static_meta()))
    } else {
        WHOLE.get_or_init(|| Props::of(SpinBox::static_meta()))
    }
}

/// What a box remembers between events. Not QML-visible.
#[derive(Debug, Default)]
pub(crate) struct SpinState {
    /// A press that landed on the box has not ended.
    held: Cell<bool>,
    /// The held button already stepped: its release adds nothing.
    repeated: Cell<bool>,
    timer: Cell<Option<ObjectId>>,
    /// The content item whose typing the box listens to.
    listening: Cell<Option<ObjectId>>,
    /// The value the box itself wrote last, which is not rounded again.
    written: Cell<Option<f64>>,
    /// The decimals the number was last shown with.
    places: Cell<i32>,
}

/// `SpinBox`.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick.Templates", name = "SpinBox", extends = Control)]
pub struct SpinBox {
    #[qml(base)]
    pub control: Control,
    #[qml(notify)]
    pub from: i32,
    #[qml(notify)]
    pub to: i32,
    #[qml(notify)]
    pub value: i32,
    #[qml(notify)]
    pub step_size: i32,
    #[qml(notify)]
    pub editable: bool,
    /// Whether a step past one end begins again at the other.
    #[qml(notify)]
    pub wrap: bool,
    /// Whether each keystroke is the value, not only what is typed when
    /// the typing is done.
    #[qml(notify)]
    pub live: bool,
    /// What may be typed. The box makes an `IntValidator` of its range
    /// unless it is given another.
    #[qml(notify)]
    pub validator: Option<ObjectId>,
    /// `function(value, locale)`: the text of a value.
    #[qml(notify)]
    pub text_from_value: Value,
    /// `function(text, locale)`: the value of a text.
    #[qml(notify)]
    pub value_from_text: Value,
    #[qml(notify, readonly)]
    pub display_text: Atom,
    #[qml(notify, flags = "Qt.InputMethodHints")]
    pub input_method_hints: i32,
    #[qml(notify, readonly)]
    pub input_method_composing: bool,
    #[qml(group)]
    pub up: SpinButton,
    #[qml(group)]
    pub down: SpinButton,
    #[qml(skip)]
    pub(crate) state: Rc<SpinState>,
}

impl Default for SpinBox {
    fn default() -> Self {
        Self {
            control: Control::default(),
            from: 0,
            to: 99,
            value: 0,
            step_size: 1,
            editable: false,
            wrap: false,
            live: false,
            validator: None,
            text_from_value: Value::Undefined,
            value_from_text: Value::Undefined,
            display_text: Atom::EMPTY,
            input_method_hints: input_method_hint::DIGITS_ONLY,
            input_method_composing: false,
            up: SpinButton::default(),
            down: SpinButton::default(),
            state: Rc::default(),
        }
    }
}

#[qml_impl]
impl SpinBox {
    /// The user changed the value: with a button, a key, the wheel, or by
    /// typing. A script that sets it does not.
    #[qml(signal, name = "valueModified")]
    fn value_modified();

    /// One step towards `to`. Not a step by the user.
    #[qml(invokable)]
    fn increase(cx: &Cx) -> Result<(), EngineError> {
        script_step(cx.engine, cx.this, Side::Up);
        Ok(())
    }

    #[qml(invokable)]
    fn decrease(cx: &Cx) -> Result<(), EngineError> {
        script_step(cx.engine, cx.this, Side::Down);
        Ok(())
    }

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        init(cx.engine, cx.this)
    }
}

/// `DoubleSpinBox` (Qt 6.11): a `SpinBox` of numbers with `decimals`
/// places.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick.Templates", name = "DoubleSpinBox", extends = Control)]
pub struct DoubleSpinBox {
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
    /// How many places the value, its ends and its text have.
    #[qml(notify)]
    pub decimals: i32,
    #[qml(notify)]
    pub editable: bool,
    #[qml(notify)]
    pub wrap: bool,
    /// What may be typed. The box makes a `DoubleValidator` of its range
    /// and decimals unless it is given another.
    #[qml(notify)]
    pub validator: Option<ObjectId>,
    /// `function(value, decimals, locale)`: the text of a value.
    #[qml(notify)]
    pub text_from_value: Value,
    /// `function(text, locale)`: the value of a text.
    #[qml(notify)]
    pub value_from_text: Value,
    #[qml(notify, readonly)]
    pub display_text: Atom,
    #[qml(notify, flags = "Qt.InputMethodHints")]
    pub input_method_hints: i32,
    #[qml(notify, readonly)]
    pub input_method_composing: bool,
    #[qml(group)]
    pub up: SpinButton,
    #[qml(group)]
    pub down: SpinButton,
    #[qml(skip)]
    pub(crate) state: Rc<SpinState>,
}

impl Default for DoubleSpinBox {
    fn default() -> Self {
        Self {
            control: Control::default(),
            from: 0.0,
            to: 99.99,
            value: 0.0,
            step_size: 1.0,
            decimals: 2,
            editable: false,
            wrap: false,
            validator: None,
            text_from_value: Value::Undefined,
            value_from_text: Value::Undefined,
            display_text: Atom::EMPTY,
            input_method_hints: input_method_hint::DIGITS_ONLY,
            input_method_composing: false,
            up: SpinButton::default(),
            down: SpinButton::default(),
            state: Rc::default(),
        }
    }
}

#[qml_impl]
impl DoubleSpinBox {
    /// The user changed the value; as `SpinBox.valueModified`.
    #[qml(signal, name = "valueModified")]
    fn value_modified();

    /// One step towards `to`. Not a step by the user.
    #[qml(invokable)]
    fn increase(cx: &Cx) -> Result<(), EngineError> {
        script_step(cx.engine, cx.this, Side::Up);
        Ok(())
    }

    #[qml(invokable)]
    fn decrease(cx: &Cx) -> Result<(), EngineError> {
        script_step(cx.engine, cx.this, Side::Down);
        Ok(())
    }

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        init(cx.engine, cx.this)
    }
}

// ---------------------------------------------------------------- values

fn state_of(engine: &Engine, obj: ObjectId) -> Option<Rc<SpinState>> {
    engine
        .with::<SpinBox, _>(obj, |s| s.state.clone())
        .or_else(|_| engine.with::<DoubleSpinBox, _>(obj, |s| s.state.clone()))
        .ok()
}

/// The decimals of a box of decimal numbers (tracked); `None` for a box of
/// whole ones.
fn places(engine: &Engine, obj: ObjectId, p: &Props) -> Option<i32> {
    p.decimals
        .map(|d| (num(engine, obj, d) as i32).clamp(0, MAX_DECIMALS))
}

/// `v` to `places` decimals, a half away from nought, as Qt writes a
/// number with that many. What is rounded is the number the double is
/// exactly: 2.675 is a little less than that, and becomes 2.67.
fn round_to(v: f64, places: i32) -> f64 {
    if !v.is_finite() {
        return v;
    }
    let places = places.clamp(0, MAX_DECIMALS) as usize;
    // Every digit the double has: its expansion ends within 1074 places.
    let exact = format!("{:.1080}", v.abs());
    let (whole, fraction) = exact.split_once('.').unwrap_or((exact.as_str(), ""));
    let kept = fraction.get(..places).unwrap_or(fraction);
    let mut digits: Vec<u8> = whole.bytes().chain(kept.bytes()).collect();
    if fraction.as_bytes().get(places).is_some_and(|d| *d >= b'5') {
        let mut carry = true;
        for d in digits.iter_mut().rev() {
            if *d == b'9' {
                *d = b'0';
            } else {
                *d += 1;
                carry = false;
                break;
            }
        }
        if carry {
            digits.insert(0, b'1');
        }
    }
    let point = digits.len() - kept.len();
    let text = format!(
        "{}.{}",
        String::from_utf8_lossy(&digits[..point]),
        String::from_utf8_lossy(&digits[point..])
    );
    let r: f64 = text.parse().unwrap_or(v.abs());
    // Nought has no sign.
    if v < 0.0 && r != 0.0 { -r } else { r }
}

/// `v` written with `places` decimals in ASCII, rounded as [`round_to`].
fn fixed(v: f64, places: i32) -> String {
    let places = places.clamp(0, MAX_DECIMALS);
    let r = round_to(v, places);
    // `r` is the double nearest a number of `places` decimals, which
    // formatting with that many writes whatever way it breaks ties.
    format!("{:.*}", places as usize, if r == 0.0 { 0.0 } else { r })
}

/// The number the box keeps for `v`: a decimal one has its decimals.
fn kept(engine: &Engine, obj: ObjectId, p: &Props, v: f64) -> f64 {
    match places(engine, obj, p) {
        Some(d) => round_to(v, d),
        None => v,
    }
}

/// The lower and the upper end of the range, whichever way round it was
/// given (tracked).
fn bounds(engine: &Engine, obj: ObjectId, p: &Props) -> (f64, f64) {
    let from = num(engine, obj, p.from);
    let to = num(engine, obj, p.to);
    (from.min(to), from.max(to))
}

fn clamped(engine: &Engine, obj: ObjectId, p: &Props, v: f64) -> f64 {
    let (lo, hi) = bounds(engine, obj, p);
    if v.is_nan() { lo } else { v.clamp(lo, hi) }
}

fn set_value(engine: &Engine, obj: ObjectId, p: &Props, v: f64) {
    if let Some(st) = state_of(engine, obj) {
        st.written.set(Some(v));
    }
    let v = match p.decimals {
        Some(_) => Value::Real(v),
        None => Value::Int(v as i32),
    };
    let r = engine.write(obj, p.value, v, WriteKind::Imperative);
    check(engine, "value", r);
}

/// The value `turns` steps from here towards `side`, or nothing when that
/// is where the value is already. A box of whole numbers takes whole
/// steps, one at least; a decimal one takes the part of a step it is given
/// (a part of a wheel's notch).
fn stepped(engine: &Engine, obj: ObjectId, p: &Props, side: Side, turns: f64) -> Option<f64> {
    engine.untrack(|| {
        let from = num(engine, obj, p.from);
        let to = num(engine, obj, p.to);
        let value = num(engine, obj, p.value);
        let wrap = flag(engine, obj, p.wrap);
        let (lo, hi) = (from.min(to), from.max(to));
        // Up is towards `to`, wherever that is.
        let towards_to = if to >= from { 1.0 } else { -1.0 };
        let mut step = num(engine, obj, p.step_size) * towards_to;
        if side == Side::Down {
            step = -step;
        }
        let count = if p.decimals.is_some() {
            step *= turns;
            1
        } else {
            (turns as i32).max(1)
        };
        let mut v = value;
        for _ in 0..count {
            let next = v + step;
            v = if next > hi {
                if wrap { lo } else { hi }
            } else if next < lo {
                if wrap { hi } else { lo }
            } else {
                next
            };
        }
        (v != value).then_some(v)
    })
}

/// `increase()` and `decrease()`.
fn script_step(engine: &Engine, obj: ObjectId, side: Side) {
    let p = props(engine, obj);
    if let Some(v) = stepped(engine, obj, p, side, 1.0) {
        set_value(engine, obj, p, v);
    }
}

/// A step by the user: the value changes and is then said to be
/// modified. What was typed is gone, since the text follows the value.
fn user_step(engine: &Engine, obj: ObjectId, side: Side, turns: f64) -> bool {
    let p = props(engine, obj);
    let Some(v) = stepped(engine, obj, p, side, turns) else {
        return false;
    };
    set_value(engine, obj, p, v);
    modified(engine, obj);
    true
}

fn modified(engine: &Engine, obj: ObjectId) {
    check(
        engine,
        "valueModified",
        engine.emit_by_name(obj, "valueModified", &[]),
    );
}

// ------------------------------------------------------------------ text

/// The number formats of the locales the boxes of an engine use.
#[derive(Default)]
struct Locales {
    by_name: RefCell<HashMap<String, Rc<LocaleInfo>>>,
}

fn locale_info(engine: &Engine, name: &str) -> Rc<LocaleInfo> {
    let all = engine.extension::<Locales>();
    if let Some(info) = all.by_name.borrow().get(name) {
        return info.clone();
    }
    let info = Rc::new(LocaleInfo::new(name));
    all.by_name
        .borrow_mut()
        .insert(name.to_owned(), info.clone());
    info
}

/// The name of the locale the box writes numbers in: its own (tracked),
/// or the application's.
fn locale_name(engine: &Engine, obj: ObjectId) -> String {
    match engine.read(obj, Control::LOCALE.index) {
        Value::String(s) if !s.is_empty() => s.to_string(),
        v @ Value::Js(_) => match engine.js_member(&v, "name") {
            Some(Value::String(s)) => s.to_string(),
            _ => engine.locale_name(),
        },
        _ => engine.locale_name(),
    }
}

/// What a script sees in `textFromValue` and `valueFromText` of a box
/// that was given neither, and the application's locale for their
/// locale argument. The box itself formats without them.
#[derive(Default)]
struct Scripts {
    made: RefCell<Option<Rc<[Value]>>>,
}

/// Where each function is in [`Scripts`].
const WHOLE_TEXT: usize = 0;
const WHOLE_VALUE: usize = 1;
const APPLICATION: usize = 2;
const DECIMAL_TEXT: usize = 3;
const DECIMAL_VALUE: usize = 4;

const TEXT_FROM_VALUE: &str = r#"(function(value, locale) {
    var n = Math.round(Number(value));
    if (isNaN(n)) n = 0;
    var digits = String(Math.abs(n));
    var sep = locale && typeof locale.groupSeparator === "string" ? locale.groupSeparator : ",";
    var out = "";
    for (var i = 0; i < digits.length; i++) {
        if (i > 0 && (digits.length - i) % 3 === 0) out += sep;
        out += digits[i];
    }
    return (n < 0 ? (locale && locale.negativeSign || "-") : "") + out;
})"#;

const VALUE_FROM_TEXT: &str = r#"(function(text, locale) {
    var s = String(text);
    if (locale && locale.groupSeparator) s = s.split(locale.groupSeparator).join("");
    if (locale && locale.negativeSign) s = s.split(locale.negativeSign).join("-");
    s = s.trim();
    return /^[-+]?[0-9]+$/.test(s) ? parseInt(s, 10) : 0;
})"#;

const APPLICATION_LOCALE: &str = "(function() { return Qt.locale(); })";

// Qt's is `Number(value).toLocaleString(locale, 'f', decimals)`, which
// writes a number given no locale as JavaScript does.
const DECIMAL_TEXT_FROM_VALUE: &str = r#"(function(value, decimals, locale) {
    var n = Number(value);
    if (!locale || typeof locale !== "object") return String(n);
    var places = Math.max(0, Math.min(100, Math.floor(Number(decimals)) || 0));
    var fixed = Math.abs(n).toFixed(places);
    var point = fixed.indexOf(".");
    var whole = point < 0 ? fixed : fixed.substring(0, point);
    var fraction = point < 0 ? "" : fixed.substring(point + 1);
    var sep = typeof locale.groupSeparator === "string" ? locale.groupSeparator : ",";
    var out = "";
    for (var i = 0; i < whole.length; i++) {
        if (i > 0 && (whole.length - i) % 3 === 0) out += sep;
        out += whole[i];
    }
    if (fraction.length > 0) out += (locale.decimalPoint || ".") + fraction;
    var negative = n < 0 && Number(fixed) !== 0;
    return (negative ? (locale.negativeSign || "-") : "") + out;
})"#;

// Qt's is `Number.fromLocaleString(locale, text)`, which throws on a text
// that is no number.
const DECIMAL_VALUE_FROM_TEXT: &str = r#"(function(text, locale) {
    var s = String(text);
    if (locale && locale.groupSeparator) s = s.split(locale.groupSeparator).join("");
    if (locale && locale.decimalPoint && locale.decimalPoint !== ".")
        s = s.split(locale.decimalPoint).join(".");
    if (locale && locale.negativeSign) s = s.split(locale.negativeSign).join("-");
    s = s.trim();
    if (!/^[-+]?([0-9]+\.?[0-9]*|\.[0-9]+)([eE][-+]?[0-9]+)?$/.test(s))
        throw new Error("Locale: Number.fromLocaleString(): Invalid format");
    return parseFloat(s);
})"#;

fn scripts(engine: &Engine) -> Option<Rc<[Value]>> {
    let all = engine.extension::<Scripts>();
    if let Some(made) = all.made.borrow().as_ref() {
        return Some(made.clone());
    }
    let mut out = Vec::new();
    for source in [
        TEXT_FROM_VALUE,
        VALUE_FROM_TEXT,
        APPLICATION_LOCALE,
        DECIMAL_TEXT_FROM_VALUE,
        DECIMAL_VALUE_FROM_TEXT,
    ] {
        match engine.eval_js(source) {
            Ok(f @ Value::Js(_)) => out.push(f),
            Ok(_) => return None,
            Err(err) => {
                check(engine, "SpinBox", Err(err));
                return None;
            }
        }
    }
    let made: Rc<[Value]> = out.into();
    *all.made.borrow_mut() = Some(made.clone());
    Some(made)
}

/// The locale a function of the author is called with.
fn locale_argument(engine: &Engine, obj: ObjectId) -> Value {
    match engine.read(obj, Control::LOCALE.index) {
        v @ Value::Js(_) => v,
        _ => scripts(engine)
            .and_then(|s| engine.call_js(&s[APPLICATION], &[]))
            .unwrap_or(Value::Undefined),
    }
}

/// Whether the author gave the box a function of their own in `p`.
fn has_own(engine: &Engine, obj: ObjectId, p: PropIndex) -> Option<Value> {
    if is_unset(engine, obj, p) {
        return None;
    }
    match engine.read(obj, p) {
        f @ Value::Js(_) => Some(f),
        _ => None,
    }
}

/// The text of `value` (tracked: it follows the locale, the decimals and
/// the function).
fn text_from_value(engine: &Engine, obj: ObjectId, p: &Props, value: f64) -> String {
    text_with(engine, obj, p, value, places(engine, obj, p))
}

/// The text of `value` with `places` decimals, or as a whole number.
fn text_with(engine: &Engine, obj: ObjectId, p: &Props, value: f64, places: Option<i32>) -> String {
    if let Some(f) = has_own(engine, obj, p.text_from_value) {
        let locale = locale_argument(engine, obj);
        let args = match places {
            Some(d) => vec![Value::Real(value), Value::Int(d), locale],
            None => vec![Value::Int(value as i32), locale],
        };
        return match engine.call_js(&f, &args) {
            Some(Value::String(s)) => s.to_string(),
            Some(Value::Undefined | Value::Null) | None => String::new(),
            Some(other) => other.to_js_string().to_string(),
        };
    }
    let info = locale_info(engine, &locale_name(engine, obj));
    match places {
        Some(d) => info.localize_number(&fixed(value, d)),
        None => info.format_integer(value as i64),
    }
}

/// The number a text stands for; nought for a text that is none.
fn value_from_text(engine: &Engine, obj: ObjectId, p: &Props, text: &str) -> f64 {
    if let Some(f) = has_own(engine, obj, p.value_from_text) {
        let locale = locale_argument(engine, obj);
        let n = engine
            .call_js(&f, &[Value::String(Atom::from(text)), locale])
            .map_or(f64::NAN, |v| v.to_number());
        return match (n.is_finite(), p.decimals) {
            (false, _) => 0.0,
            (true, Some(_)) => n,
            (true, None) => n.trunc(),
        };
    }
    let info = locale_info(engine, &locale_name(engine, obj));
    match p.decimals {
        Some(_) => parse_decimal(&info, text),
        None => parse_integer(&info, text) as f64,
    }
}

/// A number the way the locale writes it, in ASCII: its digits and sign,
/// and with `point` its decimal separator, are made ASCII ones, and its
/// grouping, which is not checked, is dropped.
fn ascii_number(info: &LocaleInfo, text: &str, point: bool) -> String {
    let group = info.group_separator();
    let minus = info.negative_sign();
    let zero = info.zero_digit().chars().next().unwrap_or('0');
    let mut s = text.trim().to_owned();
    if !group.is_empty() {
        s = s.replace(&group, "");
    }
    if !minus.is_empty() {
        s = s.replace(&minus, "-");
    }
    if point {
        let decimal = info.decimal_point();
        if decimal != "." {
            s = s.replace(&decimal, ".");
        }
    }
    s.chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| {
            let d = (c as u32).wrapping_sub(zero as u32);
            match char::from_digit(d, 10) {
                Some(digit) if d < 10 => digit,
                _ => c,
            }
        })
        .collect()
}

/// Reads a whole number the way the locale writes one.
fn parse_integer(info: &LocaleInfo, text: &str) -> i64 {
    ascii_number(info, text, false).parse::<i64>().unwrap_or(0)
}

/// Reads a number with decimals the way the locale writes one.
fn parse_decimal(info: &LocaleInfo, text: &str) -> f64 {
    ascii_number(info, text, true)
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .unwrap_or(0.0)
}

/// The content item when it is a text the box can read.
fn editor(engine: &Engine, obj: ObjectId) -> Option<ObjectId> {
    engine
        .untrack(|| object(engine, obj, Control::CONTENT_ITEM.index))
        .filter(|c| is_a::<TextInput>(engine, *c))
}

fn show(engine: &Engine, obj: ObjectId, p: &Props, text: &str) {
    // Derived: the text goes on following the value.
    let r = engine.write_derived(obj, p.display_text, Value::String(Atom::from(text)));
    check(engine, "displayText", r);
}

/// Makes what the number reads the value, as Return and the keyboard
/// leaving do. The text is read whether or not it was typed: a box whose
/// two functions do not undo each other moves each time (checked on
/// qml6).
fn commit(engine: &Engine, obj: ObjectId, by_user: bool) {
    let Some(content) = editor(engine, obj) else {
        return;
    };
    let p = props(engine, obj);
    engine.untrack(|| {
        let text = engine.read(content, TextEditor::TEXT.index);
        let text = match &text {
            Value::String(s) => s.to_string(),
            _ => String::new(),
        };
        let before = num(engine, obj, p.value);
        let read = value_from_text(engine, obj, p, &text);
        let v = clamped(engine, obj, p, kept(engine, obj, p, read));
        if v != before {
            set_value(engine, obj, p, v);
        }
        // Also when the value is what it was: "007" is shown as "7".
        let now = num(engine, obj, p.value);
        show(engine, obj, p, &text_from_value(engine, obj, p, now));
        if by_user && v != before {
            modified(engine, obj);
        }
    });
}

/// The number was typed in.
fn typed(engine: &Engine, obj: ObjectId, content: ObjectId) {
    if editor(engine, obj) != Some(content) {
        return;
    }
    let p = props(engine, obj);
    if engine.untrack(|| p.live.is_some_and(|live| flag(engine, obj, live))) {
        commit(engine, obj, false);
        return;
    }
    let text = engine.untrack(|| engine.read(content, TextEditor::TEXT.index));
    if let Value::String(s) = text {
        show(engine, obj, p, &s);
    }
}

/// Listens to the typing in the content item, once for each item.
fn listen(engine: &Engine, obj: ObjectId) {
    let Some(st) = state_of(engine, obj) else {
        return;
    };
    let Some(content) = editor(engine, obj) else {
        return;
    };
    if st.listening.replace(Some(content)) == Some(content) {
        return;
    }
    let r = engine
        .connect_by_name(content, "textEdited", move |e, _| {
            if e.is_alive(obj) {
                typed(e, obj, content);
            }
        })
        .map(|_| ());
    check(engine, "contentItem", r);
    // The number is where the Tab key stops; the box is not.
    if is_unset(engine, content, Item::ACTIVE_FOCUS_ON_TAB.index) {
        let r = engine.write_derived(content, Item::ACTIVE_FOCUS_ON_TAB.index, Value::Bool(true));
        check(engine, "contentItem", r);
    }
}

// -------------------------------------------------------------- complete

/// The ends of a decimal box have its decimals. The rounded end is
/// written past any binding the author gave it, which stays.
fn round_end(engine: &Engine, obj: ObjectId, p: &Props, end: PropIndex) {
    let Some(d) = engine.untrack(|| places(engine, obj, p)) else {
        return;
    };
    let v = engine.untrack(|| num(engine, obj, end));
    let r = round_to(v, d);
    if r != v && !r.is_nan() {
        let w = engine.write(obj, end, Value::Real(r), WriteKind::Host);
        check(engine, "DoubleSpinBox", w);
    }
}

/// Other decimals: the number that was shown is read again with the
/// box's own `valueFromText`, and the ends are rounded. Qt reads the text
/// rather than rounding the value, which an author's pair of functions
/// that do not undo each other shows (checked on qml6).
fn decimals_changed(engine: &Engine, obj: ObjectId, p: &Props) {
    let Some(st) = state_of(engine, obj) else {
        return;
    };
    let Some(now) = engine.untrack(|| places(engine, obj, p)) else {
        return;
    };
    let before = st.places.replace(now);
    for end in [p.from, p.to] {
        round_end(engine, obj, p, end);
    }
    engine.untrack(|| {
        let value = num(engine, obj, p.value);
        let shown = text_with(engine, obj, p, value, Some(before));
        let read = value_from_text(engine, obj, p, &shown);
        let v = clamped(engine, obj, p, round_to(read, now));
        if v != value {
            set_value(engine, obj, p, v);
        }
    });
}

fn init(engine: &Engine, obj: ObjectId) -> Result<(), EngineError> {
    let p = props(engine, obj);
    let st = state_of(engine, obj);

    // Before the value is kept in the range: the range is rounded first.
    if let Some(dp) = p.decimals {
        if let (Some(st), Some(d)) = (&st, places(engine, obj, p)) {
            st.places.set(d);
        }
        for end in [p.from, p.to] {
            round_end(engine, obj, p, end);
            engine.watch(obj, end, move |e, _| round_end(e, obj, p, end))?;
        }
        engine.watch(obj, dp, move |e, _| decimals_changed(e, obj, p))?;
    }

    // The declared values came in the order they were written, so the
    // range is whole only now.
    let keep_in_range = move |e: &Engine| {
        let v = e.untrack(|| num(e, obj, p.value));
        // A value from a script has the decimals; one the box stepped to
        // is kept as it is (three steps of 0.1 are 0.30000000000000004 on
        // qml6).
        let ours = state_of(e, obj).is_some_and(|st| st.written.get() == Some(v));
        let rounded = if ours {
            v
        } else {
            e.untrack(|| kept(e, obj, p, v))
        };
        let keep = e.untrack(|| clamped(e, obj, p, rounded));
        if keep != v {
            set_value(e, obj, p, keep);
        }
    };
    keep_in_range(engine);
    for prop in [p.from, p.to, p.value] {
        engine.watch(obj, prop, move |e, _| keep_in_range(e))?;
    }

    for side in SIDES {
        let b = *p.button(side);
        manage_part(engine, obj, b.indicator, PartKind::Free)?;
        for (i, size) in [Item::IMPLICIT_WIDTH.index, Item::IMPLICIT_HEIGHT.index]
            .into_iter()
            .enumerate()
        {
            engine.bind_initial(obj, b.implicit[i], move |e| {
                match object(e, obj, b.indicator) {
                    Some(part) if is_item(e, part) => Value::Real(num(e, part, size)),
                    _ => Value::Real(0.0),
                }
            })?;
        }
        // A button with nowhere to go is not enabled.
        engine.effect_in(obj, move |e| {
            let Some(part) = object(e, obj, b.indicator).filter(|p| is_item(e, *p)) else {
                return;
            };
            let end = match side {
                Side::Up => p.to,
                Side::Down => p.from,
            };
            let on = flag(e, obj, p.wrap) || num(e, obj, p.value) != num(e, obj, end);
            if e.untrack(|| is_unset(e, part, Item::ENABLED.index)) {
                let r = e.write_derived(part, Item::ENABLED.index, Value::Bool(on));
                check(e, "indicator", r);
            }
        })?;
    }

    // A validator of the box's own, unless the author has one.
    if is_unset(engine, obj, p.validator) {
        let made = match p.decimals {
            Some(_) => engine.create_native_incomplete(DoubleValidator::default(), Some(obj))?,
            None => engine.create_native_incomplete(IntValidator::default(), Some(obj))?,
        };
        engine.complete(made)?;
        engine.write_derived(obj, p.validator, Value::Object(made))?;
        engine.effect_in(obj, move |e| {
            if !e.is_alive(made) {
                return;
            }
            let (lo, hi) = bounds(e, obj, p);
            let name = locale_name(e, obj);
            let r = match places(e, obj, p) {
                Some(d) => e
                    .set(made, DoubleValidator::BOTTOM, lo)
                    .and_then(|()| e.set(made, DoubleValidator::TOP, hi))
                    .and_then(|()| e.set(made, DoubleValidator::DECIMALS, d))
                    .and_then(|()| e.set(made, DoubleValidator::LOCALE, name)),
                None => e
                    .set(made, IntValidator::BOTTOM, lo as i32)
                    .and_then(|()| e.set(made, IntValidator::TOP, hi as i32))
                    .and_then(|()| e.set(made, IntValidator::LOCALE, name)),
            };
            check(e, "validator", r);
        })?;
    }

    // A script may call the two functions of a box that was given none.
    if let Some(made) = scripts(engine) {
        let (text, value) = match p.decimals {
            Some(_) => (DECIMAL_TEXT, DECIMAL_VALUE),
            None => (WHOLE_TEXT, WHOLE_VALUE),
        };
        for (prop, f) in [(p.text_from_value, text), (p.value_from_text, value)] {
            if is_unset(engine, obj, prop) {
                engine.write_derived(obj, prop, made[f].clone())?;
            }
        }
    }

    engine.bind_initial(obj, p.display_text, move |e| {
        let v = num(e, obj, p.value);
        Value::String(Atom::from(text_from_value(e, obj, p, v).as_str()))
    })?;
    engine.bind_initial(obj, p.input_method_composing, move |e| {
        let composing = object(e, obj, Control::CONTENT_ITEM.index)
            .filter(|c| is_a::<TextInput>(e, *c))
            .is_some_and(|c| flag(e, c, TextEditor::INPUT_METHOD_COMPOSING.index));
        Value::Bool(composing)
    })?;

    listen(engine, obj);
    engine.watch(obj, Control::CONTENT_ITEM.index, move |e, _| listen(e, obj))?;

    engine.watch(obj, Item::ACTIVE_FOCUS.index, move |e, v| {
        if !e.untrack(|| flag(e, obj, p.editable)) {
            return;
        }
        if !v.to_boolean() {
            // The typing is done.
            commit(e, obj, true);
        } else if let Some(content) = editor(e, obj)
            && !e.untrack(|| flag(e, content, Item::ACTIVE_FOCUS.index))
        {
            // The keyboard came to the box: it is the number that is
            // typed in.
            force_active_focus(e, content, focus_reason(e, obj));
        }
    })?;

    crate::a11y::defaults(engine, obj, Role::SpinBox, None)
}

// --------------------------------------------------------------- pointer

/// Where the item is in its parent, and whether the point is in it.
fn in_item(engine: &Engine, item: ObjectId, at: kurbo::Point) -> bool {
    let x = num(engine, item, Item::X.index);
    let y = num(engine, item, Item::Y.index);
    let w = num(engine, item, Item::WIDTH.index);
    let h = num(engine, item, Item::HEIGHT.index);
    at.x >= x && at.x < x + w && at.y >= y && at.y < y + h
}

/// The button the point is on, with whether it is enabled.
fn button_at(engine: &Engine, obj: ObjectId, at: kurbo::Point) -> Option<(Side, bool)> {
    let p = props(engine, obj);
    engine.untrack(|| {
        SIDES.into_iter().find_map(|side| {
            let part = object(engine, obj, p.button(side).indicator)?;
            let shown = is_item(engine, part) && flag(engine, part, Item::VISIBLE.index);
            (shown && in_item(engine, part, at))
                .then(|| (side, flag(engine, part, Item::ENABLED.index)))
        })
    })
}

fn pressed_side(engine: &Engine, obj: ObjectId) -> Option<Side> {
    let p = props(engine, obj);
    engine.untrack(|| {
        SIDES
            .into_iter()
            .find(|s| flag(engine, obj, p.button(*s).pressed))
    })
}

fn set_flag(engine: &Engine, obj: ObjectId, p: PropIndex, on: bool) {
    let r = set_computed(engine, obj, p, Value::Bool(on));
    check(engine, "SpinBox", r);
}

/// Sets one flag of each button: on for `on`'s, off for the other.
fn set_buttons(
    engine: &Engine,
    obj: ObjectId,
    flag_of: fn(&ButtonProps) -> PropIndex,
    on: Option<Side>,
) {
    let p = props(engine, obj);
    for side in SIDES {
        set_flag(engine, obj, flag_of(p.button(side)), on == Some(side));
    }
}

/// Presses the button under the held pointer and lets go of the other.
/// A button that is pressed stays so when it runs out of places to go.
fn track(engine: &Engine, obj: ObjectId, st: &SpinState, at: kurbo::Point) {
    let was = pressed_side(engine, obj);
    let now = button_at(engine, obj, at)
        .filter(|(side, enabled)| *enabled || was == Some(*side))
        .map(|(side, _)| side);
    if now == was {
        return;
    }
    set_buttons(engine, obj, |b| b.pressed, now);
    if now.is_some() {
        ticker::start(engine, obj, &st.timer, REPEAT_DELAY, repeat);
    } else {
        ticker::stop(engine, &st.timer);
    }
}

/// The held button steps again.
fn repeat(engine: &Engine, obj: ObjectId) {
    let Some(st) = state_of(engine, obj) else {
        return;
    };
    let side = pressed_side(engine, obj).filter(|_| st.held.get());
    if side.is_some_and(|side| user_step(engine, obj, side, 1.0)) {
        st.repeated.set(true);
        ticker::start(engine, obj, &st.timer, REPEAT_INTERVAL, repeat);
    } else {
        ticker::stop(engine, &st.timer);
    }
}

fn press(engine: &Engine, obj: ObjectId, ev: &PointerEvent, at: kurbo::Point) -> bool {
    if !takes_input(engine, obj) || !is_left(ev) {
        return false;
    }
    if let Some(st) = state_of(engine, obj) {
        st.held.set(true);
        st.repeated.set(false);
        track(engine, obj, &st, at);
    }
    // Opaque, as every control is.
    true
}

fn moved(engine: &Engine, obj: ObjectId, _ev: &PointerEvent, at: kurbo::Point) {
    if let Some(st) = state_of(engine, obj).filter(|st| st.held.get()) {
        track(engine, obj, &st, at);
    }
}

fn let_go(engine: &Engine, obj: ObjectId, st: &SpinState) {
    st.held.set(false);
    ticker::stop(engine, &st.timer);
    set_buttons(engine, obj, |b| b.pressed, None);
}

fn release(engine: &Engine, obj: ObjectId, _ev: &PointerEvent, at: kurbo::Point) {
    let Some(st) = state_of(engine, obj).filter(|st| st.held.get()) else {
        return;
    };
    track(engine, obj, &st, at);
    let side = pressed_side(engine, obj);
    let ran = st.repeated.get();
    let_go(engine, obj, &st);
    if let Some(side) = side
        && !ran
    {
        user_step(engine, obj, side, 1.0);
    }
}

fn cancel(engine: &Engine, obj: ObjectId) {
    if let Some(st) = state_of(engine, obj) {
        let_go(engine, obj, &st);
    }
}

fn hover_box(engine: &Engine, obj: ObjectId, over: bool) {
    hover(engine, obj, over);
    if !over {
        set_buttons(engine, obj, |b| b.hovered, None);
    }
}

/// The pointer moved over the box: the enabled button it is on is
/// hovered.
fn hover_moved(engine: &Engine, obj: ObjectId, at: kurbo::Point) {
    let wanted = engine.untrack(|| flag(engine, obj, Control::HOVER_ENABLED.index));
    let on = button_at(engine, obj, at)
        .filter(|(_, enabled)| wanted && *enabled)
        .map(|(side, _)| side);
    set_buttons(engine, obj, |b| b.hovered, on);
}

// ------------------------------------------------------- keys and wheel

fn key_press(engine: &Engine, obj: ObjectId, ev: &KeyEvent) -> bool {
    if !takes_input(engine, obj) {
        return false;
    }
    match ev.key {
        // With any modifier, and again for each repeat of a held key.
        key::UP => {
            user_step(engine, obj, Side::Up, 1.0);
            true
        }
        key::DOWN => {
            user_step(engine, obj, Side::Down, 1.0);
            true
        }
        key::RETURN | key::ENTER => {
            let p = props(engine, obj);
            if engine.untrack(|| flag(engine, obj, p.editable)) {
                commit(engine, obj, true);
            }
            // The key is not the box's: a dialog's default button may
            // still want it.
            false
        }
        _ => false,
    }
}

fn no_key(_: &Engine, _: ObjectId, _: &KeyEvent) -> bool {
    false
}

fn wheel(engine: &Engine, obj: ObjectId, ev: &WheelEvent, _at: kurbo::Point) -> bool {
    let wanted = engine.untrack(|| flag(engine, obj, Control::WHEEL_ENABLED.index));
    if !wanted || !takes_input(engine, obj) {
        return false;
    }
    let turned = ev.angle_delta.y;
    if turned == 0.0 {
        return true;
    }
    let notches = turned.abs() / WheelEvent::NOTCH;
    // Part of a notch is a step too, so that a fine wheel is not dead: a
    // whole one for whole numbers, that part of one for decimal numbers
    // (checked on qml6), and the notches of one turn are one step.
    let turns = match props(engine, obj).decimals {
        Some(_) => notches,
        None => notches.trunc().max(1.0),
    };
    // Away from the user is up.
    let side = if turned > 0.0 { Side::Up } else { Side::Down };
    user_step(engine, obj, side, turns);
    true
}

// --------------------------------------------------------- accessibility

fn describe(engine: &Engine, obj: ObjectId, node: &mut NodeData) {
    let p = props(engine, obj);
    let (lo, hi) = bounds(engine, obj, p);
    node.range = Some(Range {
        value: num(engine, obj, p.value),
        min: lo,
        max: hi,
        step: num(engine, obj, p.step_size).abs(),
    });
    // AccessKit makes a spin button editable text; only an editable box is.
    node.read_only |= !flag(engine, obj, p.editable);
}

fn act(engine: &Engine, obj: ObjectId, action: AccessibleAction) -> bool {
    if !takes_input(engine, obj) {
        return false;
    }
    match action {
        AccessibleAction::Increase => user_step(engine, obj, Side::Up, 1.0),
        AccessibleAction::Decrease => user_step(engine, obj, Side::Down, 1.0),
        AccessibleAction::SetValue(v) if v.is_finite() => {
            let p = props(engine, obj);
            let v = engine.untrack(|| {
                let v = if p.decimals.is_some() { v } else { v.round() };
                clamped(engine, obj, p, kept(engine, obj, p, v))
            });
            if v != engine.untrack(|| num(engine, obj, p.value)) {
                set_value(engine, obj, p, v);
                modified(engine, obj);
            }
            true
        }
        _ => false,
    }
}

pub(crate) fn register(engine: &Engine) {
    for meta in [SpinBox::static_meta(), DoubleSpinBox::static_meta()] {
        let meta = || MetaRef::Static(meta);
        // The number has the keyboard on the box's behalf.
        register_focus_scope(engine, meta());
        register_pointer_item(
            engine,
            meta(),
            PointerItem {
                press,
                moved,
                release,
                cancel,
                hover: hover_box,
            },
        );
        register_hover_move(engine, meta(), hover_moved);
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_number_is_read_as_its_locale_writes_it() {
        let en = LocaleInfo::new("en_US");
        assert_eq!(parse_integer(&en, "1,234,567"), 1_234_567);
        assert_eq!(parse_integer(&en, " -42 "), -42);
        assert_eq!(parse_integer(&en, ""), 0);
        assert_eq!(parse_integer(&en, "12 kg"), 0);
        let de = LocaleInfo::new("de_DE");
        assert_eq!(parse_integer(&de, "1.234.567"), 1_234_567);
        assert_eq!(parse_integer(&de, &de.format_integer(-9_876)), -9_876);
    }

    #[test]
    fn a_decimal_number_is_read_as_its_locale_writes_it() {
        let en = LocaleInfo::new("en_US");
        assert_eq!(parse_decimal(&en, "1,234.5"), 1234.5);
        assert_eq!(parse_decimal(&en, "-2.25"), -2.25);
        assert_eq!(parse_decimal(&en, ""), 0.0);
        assert_eq!(parse_decimal(&en, "inf"), 0.0);
        let de = LocaleInfo::new("de_DE");
        assert_eq!(parse_decimal(&de, "1.234,5"), 1234.5);
        assert_eq!(parse_decimal(&de, "3,75"), 3.75);
    }

    #[test]
    fn rounding_is_of_the_exact_number_and_a_half_goes_away_from_nought() {
        assert_eq!(round_to(1.234, 2), 1.23);
        assert_eq!(round_to(0.125, 2), 0.13);
        assert_eq!(round_to(-0.125, 2), -0.13);
        // 2.675 is 2.67499999999999982236431605997495353221893310546875.
        assert_eq!(round_to(2.675, 2), 2.67);
        assert_eq!(round_to(2.5, 0), 3.0);
        assert_eq!(round_to(9.999, 2), 10.0);
        assert_eq!(round_to(99.995, 0), 100.0);
        assert_eq!(round_to(-0.001, 2), 0.0);
        assert_eq!(round_to(1.23456789, 4), 1.2346);
        assert_eq!(round_to(1e300, 2), 1e300);
    }

    #[test]
    fn a_decimal_number_is_written_with_its_places() {
        assert_eq!(fixed(0.30000000000000004, 2), "0.30");
        assert_eq!(fixed(-0.001, 2), "0.00");
        assert_eq!(fixed(2.5, 0), "3");
        let en = LocaleInfo::new("en_US");
        assert_eq!(en.localize_number(&fixed(1234.5, 2)), "1,234.50");
        let de = LocaleInfo::new("de_DE");
        assert_eq!(de.localize_number(&fixed(1234.5, 2)), "1.234,50");
        assert_eq!(de.localize_number(&fixed(-0.5, 3)), "-0,500");
    }
}
