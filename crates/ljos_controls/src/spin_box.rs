//! `SpinBox` (QtQuick.Templates): a whole number between two bounds,
//! stepped with a button at either side, the arrow keys or the wheel, and
//! typed when the box is editable.
//!
//! Documentation: doc.qt.io/qt-6/qml-qtquick-controls-spinbox.html; pinned
//! by `features/controls/spin-box*.feature`, which say what running Qt
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

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::OnceLock;

use ljos_a11y::{NodeData, Range, Role};
use ljos_engine::{
    Atom, Cx, Engine, EngineError, MetaRef, ObjectId, PropIndex, QmlObject, QmlType, Value,
    WriteKind, qml_impl,
};
use ljos_i18n::LocaleInfo;
use ljos_quick::flickable::{WheelItem, register_wheel_item};
use ljos_quick::focus::{focus_reason, force_active_focus, register_focus_scope};
use ljos_quick::keys::{KeyItem, key, register_key_item};
use ljos_quick::text_input::input_method_hint;
use ljos_quick::{
    AccessibleAction, AccessibleItem, IntValidator, Item, KeyEvent, PointerEvent, PointerItem,
    TextEditor, TextInput, WheelEvent, is_item, register_accessible_item, register_hover_move,
    register_pointer_item,
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

/// The `up` and `down` groups of a spin box: one button each.
#[derive(Clone, Debug, Default, QmlObject)]
#[qml(name = "SpinButton", uncreatable)]
pub struct SpinButton {
    #[qml(notify)]
    pub pressed: bool,
    /// The item that is the button; the style makes it.
    #[qml(notify)]
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

fn button(side: Side) -> &'static ButtonProps {
    static PROPS: OnceLock<[ButtonProps; 2]> = OnceLock::new();
    let all = PROPS.get_or_init(|| {
        let meta = SpinBox::static_meta();
        let of = |group: &str| {
            let p = |name: &str| {
                meta.property_index(&format!("{group}.{name}"))
                    .expect("the box has the group")
            };
            ButtonProps {
                pressed: p("pressed"),
                indicator: p("indicator"),
                hovered: p("hovered"),
                implicit: [p("implicitIndicatorWidth"), p("implicitIndicatorHeight")],
            }
        };
        [of("up"), of("down")]
    });
    match side {
        Side::Up => &all[0],
        Side::Down => &all[1],
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
    #[qml(notify)]
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
        if let Some(v) = stepped(cx.engine, cx.this, Side::Up, 1) {
            set_value(cx.engine, cx.this, v);
        }
        Ok(())
    }

    #[qml(invokable)]
    fn decrease(cx: &Cx) -> Result<(), EngineError> {
        if let Some(v) = stepped(cx.engine, cx.this, Side::Down, 1) {
            set_value(cx.engine, cx.this, v);
        }
        Ok(())
    }

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        init(cx.engine, cx.this)
    }
}

// ---------------------------------------------------------------- values

fn int(engine: &Engine, obj: ObjectId, p: PropIndex) -> i32 {
    engine.read(obj, p).to_number() as i32
}

fn state_of(engine: &Engine, obj: ObjectId) -> Option<Rc<SpinState>> {
    engine.with::<SpinBox, _>(obj, |s| s.state.clone()).ok()
}

/// The lower and the upper end of the range, whichever way round it was
/// given (tracked).
fn bounds(engine: &Engine, obj: ObjectId) -> (i32, i32) {
    let from = int(engine, obj, SpinBox::FROM.index);
    let to = int(engine, obj, SpinBox::TO.index);
    (from.min(to), from.max(to))
}

fn clamped(engine: &Engine, obj: ObjectId, v: i64) -> i32 {
    let (lo, hi) = bounds(engine, obj);
    v.clamp(i64::from(lo), i64::from(hi)) as i32
}

fn set_value(engine: &Engine, obj: ObjectId, v: i32) {
    let r = engine.write(
        obj,
        SpinBox::VALUE.index,
        Value::Int(v),
        WriteKind::Imperative,
    );
    check(engine, "value", r);
}

/// The value `count` steps from here towards `side`, or nothing when
/// that is where the value is already.
fn stepped(engine: &Engine, obj: ObjectId, side: Side, count: i32) -> Option<i32> {
    engine.untrack(|| {
        let from = int(engine, obj, SpinBox::FROM.index);
        let to = int(engine, obj, SpinBox::TO.index);
        let value = int(engine, obj, SpinBox::VALUE.index);
        let wrap = flag(engine, obj, SpinBox::WRAP.index);
        let (lo, hi) = (i64::from(from.min(to)), i64::from(from.max(to)));
        // Up is towards `to`, wherever that is.
        let towards_to = if to >= from { 1 } else { -1 };
        let mut step = i64::from(int(engine, obj, SpinBox::STEP_SIZE.index)) * towards_to;
        if side == Side::Down {
            step = -step;
        }
        let mut v = i64::from(value);
        for _ in 0..count.max(1) {
            let next = v + step;
            v = if next > hi {
                if wrap { lo } else { hi }
            } else if next < lo {
                if wrap { hi } else { lo }
            } else {
                next
            };
        }
        let v = v as i32;
        (v != value).then_some(v)
    })
}

/// A step by the user: the value changes and is then said to be
/// modified. What was typed is gone, since the text follows the value.
fn user_step(engine: &Engine, obj: ObjectId, side: Side, count: i32) -> bool {
    let Some(v) = stepped(engine, obj, side, count) else {
        return false;
    };
    set_value(engine, obj, v);
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
/// that was given neither, and the application's locale for their second
/// argument. The box itself formats without them.
#[derive(Default)]
struct Scripts {
    made: RefCell<Option<[Value; 3]>>,
}

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

fn scripts(engine: &Engine) -> Option<[Value; 3]> {
    let all = engine.extension::<Scripts>();
    if let Some(made) = all.made.borrow().as_ref() {
        return Some(made.clone());
    }
    let mut out = Vec::new();
    for source in [TEXT_FROM_VALUE, VALUE_FROM_TEXT, APPLICATION_LOCALE] {
        match engine.eval_js(source) {
            Ok(f @ Value::Js(_)) => out.push(f),
            Ok(_) => return None,
            Err(err) => {
                check(engine, "SpinBox", Err(err));
                return None;
            }
        }
    }
    let made: [Value; 3] = out.try_into().ok()?;
    *all.made.borrow_mut() = Some(made.clone());
    Some(made)
}

/// The locale a function of the author is called with.
fn locale_argument(engine: &Engine, obj: ObjectId) -> Value {
    match engine.read(obj, Control::LOCALE.index) {
        v @ Value::Js(_) => v,
        _ => scripts(engine)
            .and_then(|s| engine.call_js(&s[2], &[]))
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

/// The text of `value` (tracked: it follows the locale and the function).
fn text_from_value(engine: &Engine, obj: ObjectId, value: i32) -> String {
    if let Some(f) = has_own(engine, obj, SpinBox::TEXT_FROM_VALUE.index) {
        let locale = locale_argument(engine, obj);
        return match engine.call_js(&f, &[Value::Int(value), locale]) {
            Some(Value::String(s)) => s.to_string(),
            Some(Value::Undefined | Value::Null) | None => String::new(),
            Some(other) => other.to_js_string().to_string(),
        };
    }
    let name = locale_name(engine, obj);
    locale_info(engine, &name).format_integer(i64::from(value))
}

/// The number a text stands for; nought for a text that is none.
fn value_from_text(engine: &Engine, obj: ObjectId, text: &str) -> i64 {
    if let Some(f) = has_own(engine, obj, SpinBox::VALUE_FROM_TEXT.index) {
        let locale = locale_argument(engine, obj);
        let n = engine
            .call_js(&f, &[Value::String(Atom::from(text)), locale])
            .map_or(f64::NAN, |v| v.to_number());
        return if n.is_finite() { n as i64 } else { 0 };
    }
    let name = locale_name(engine, obj);
    parse_integer(&locale_info(engine, &name), text)
}

/// Reads a whole number the way the locale writes one: with its digits,
/// its sign, and its grouping, which is not checked.
fn parse_integer(info: &LocaleInfo, text: &str) -> i64 {
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
    let ascii: String = s
        .chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| {
            let d = (c as u32).wrapping_sub(zero as u32);
            match char::from_digit(d, 10) {
                Some(digit) if d < 10 => digit,
                _ => c,
            }
        })
        .collect();
    ascii.parse::<i64>().unwrap_or(0)
}

/// The content item when it is a text the box can read.
fn editor(engine: &Engine, obj: ObjectId) -> Option<ObjectId> {
    engine
        .untrack(|| object(engine, obj, Control::CONTENT_ITEM.index))
        .filter(|c| is_a::<TextInput>(engine, *c))
}

fn show(engine: &Engine, obj: ObjectId, text: &str) {
    // Derived: the text goes on following the value.
    let r = engine.write_derived(
        obj,
        SpinBox::DISPLAY_TEXT.index,
        Value::String(Atom::from(text)),
    );
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
    engine.untrack(|| {
        let text = engine.read(content, TextEditor::TEXT.index);
        let text = match &text {
            Value::String(s) => s.to_string(),
            _ => String::new(),
        };
        let before = int(engine, obj, SpinBox::VALUE.index);
        let v = clamped(engine, obj, value_from_text(engine, obj, &text));
        if v != before {
            set_value(engine, obj, v);
        }
        // Also when the value is what it was: "007" is shown as "7".
        let now = int(engine, obj, SpinBox::VALUE.index);
        show(engine, obj, &text_from_value(engine, obj, now));
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
    if engine.untrack(|| flag(engine, obj, SpinBox::LIVE.index)) {
        commit(engine, obj, false);
        return;
    }
    let text = engine.untrack(|| engine.read(content, TextEditor::TEXT.index));
    if let Value::String(s) = text {
        show(engine, obj, &s);
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

fn init(engine: &Engine, obj: ObjectId) -> Result<(), EngineError> {
    // The declared values came in the order they were written, so the
    // range is whole only now.
    let keep_in_range = move |e: &Engine| {
        let v = e.untrack(|| int(e, obj, SpinBox::VALUE.index));
        let kept = e.untrack(|| clamped(e, obj, i64::from(v)));
        if kept != v {
            set_value(e, obj, kept);
        }
    };
    keep_in_range(engine);
    for p in [SpinBox::FROM.index, SpinBox::TO.index, SpinBox::VALUE.index] {
        engine.watch(obj, p, move |e, _| keep_in_range(e))?;
    }

    for side in SIDES {
        let b = button(side);
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
                Side::Up => SpinBox::TO.index,
                Side::Down => SpinBox::FROM.index,
            };
            let on = flag(e, obj, SpinBox::WRAP.index)
                || int(e, obj, SpinBox::VALUE.index) != int(e, obj, end);
            if e.untrack(|| is_unset(e, part, Item::ENABLED.index)) {
                let r = e.write_derived(part, Item::ENABLED.index, Value::Bool(on));
                check(e, "indicator", r);
            }
        })?;
    }

    // A validator of the box's own, unless the author has one.
    if is_unset(engine, obj, SpinBox::VALIDATOR.index) {
        let made = engine.create_native(IntValidator::default(), Some(obj))?;
        engine.complete(made)?;
        engine.write_derived(obj, SpinBox::VALIDATOR.index, Value::Object(made))?;
        engine.effect_in(obj, move |e| {
            if !e.is_alive(made) {
                return;
            }
            let (lo, hi) = bounds(e, obj);
            let name = locale_name(e, obj);
            let r = e
                .set(made, IntValidator::BOTTOM, lo)
                .and_then(|()| e.set(made, IntValidator::TOP, hi))
                .and_then(|()| e.set(made, IntValidator::LOCALE, name));
            check(e, "validator", r);
        })?;
    }

    // A script may call the two functions of a box that was given none.
    if let Some([text, value, _]) = scripts(engine) {
        for (p, f) in [
            (SpinBox::TEXT_FROM_VALUE.index, text),
            (SpinBox::VALUE_FROM_TEXT.index, value),
        ] {
            if is_unset(engine, obj, p) {
                engine.write_derived(obj, p, f)?;
            }
        }
    }

    engine.bind_initial(obj, SpinBox::DISPLAY_TEXT.index, move |e| {
        let v = int(e, obj, SpinBox::VALUE.index);
        Value::String(Atom::from(text_from_value(e, obj, v).as_str()))
    })?;
    engine.bind_initial(obj, SpinBox::INPUT_METHOD_COMPOSING.index, move |e| {
        let composing = object(e, obj, Control::CONTENT_ITEM.index)
            .filter(|c| is_a::<TextInput>(e, *c))
            .is_some_and(|c| flag(e, c, TextEditor::INPUT_METHOD_COMPOSING.index));
        Value::Bool(composing)
    })?;

    listen(engine, obj);
    engine.watch(obj, Control::CONTENT_ITEM.index, move |e, _| listen(e, obj))?;

    engine.watch(obj, Item::ACTIVE_FOCUS.index, move |e, v| {
        if !e.untrack(|| flag(e, obj, SpinBox::EDITABLE.index)) {
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
    engine.untrack(|| {
        SIDES.into_iter().find_map(|side| {
            let part = object(engine, obj, button(side).indicator)?;
            let shown = is_item(engine, part) && flag(engine, part, Item::VISIBLE.index);
            (shown && in_item(engine, part, at))
                .then(|| (side, flag(engine, part, Item::ENABLED.index)))
        })
    })
}

fn pressed_side(engine: &Engine, obj: ObjectId) -> Option<Side> {
    engine.untrack(|| {
        SIDES
            .into_iter()
            .find(|s| flag(engine, obj, button(*s).pressed))
    })
}

fn set_flag(engine: &Engine, obj: ObjectId, p: PropIndex, on: bool) {
    let r = set_computed(engine, obj, p, Value::Bool(on));
    check(engine, "SpinBox", r);
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
    for side in SIDES {
        set_flag(engine, obj, button(side).pressed, now == Some(side));
    }
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
    if side.is_some_and(|side| user_step(engine, obj, side, 1)) {
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
    for side in SIDES {
        set_flag(engine, obj, button(side).pressed, false);
    }
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
        user_step(engine, obj, side, 1);
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
        for side in SIDES {
            set_flag(engine, obj, button(side).hovered, false);
        }
    }
}

/// The pointer moved over the box: the enabled button it is on is
/// hovered.
fn hover_moved(engine: &Engine, obj: ObjectId, at: kurbo::Point) {
    let wanted = engine.untrack(|| flag(engine, obj, Control::HOVER_ENABLED.index));
    let on = button_at(engine, obj, at)
        .filter(|(_, enabled)| wanted && *enabled)
        .map(|(side, _)| side);
    for side in SIDES {
        set_flag(engine, obj, button(side).hovered, on == Some(side));
    }
}

// ------------------------------------------------------- keys and wheel

fn key_press(engine: &Engine, obj: ObjectId, ev: &KeyEvent) -> bool {
    if !takes_input(engine, obj) {
        return false;
    }
    match ev.key {
        // With any modifier, and again for each repeat of a held key.
        key::UP => {
            user_step(engine, obj, Side::Up, 1);
            true
        }
        key::DOWN => {
            user_step(engine, obj, Side::Down, 1);
            true
        }
        key::RETURN | key::ENTER => {
            if engine.untrack(|| flag(engine, obj, SpinBox::EDITABLE.index)) {
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
    // Part of a notch is a step too, so that a fine wheel is not dead.
    let notches = ((turned.abs() / WheelEvent::NOTCH).trunc() as i32).max(1);
    // Away from the user is up.
    let side = if turned > 0.0 { Side::Up } else { Side::Down };
    user_step(engine, obj, side, notches);
    true
}

// --------------------------------------------------------- accessibility

fn describe(engine: &Engine, obj: ObjectId, node: &mut NodeData) {
    let (lo, hi) = bounds(engine, obj);
    node.range = Some(Range {
        value: f64::from(int(engine, obj, SpinBox::VALUE.index)),
        min: f64::from(lo),
        max: f64::from(hi),
        step: f64::from(int(engine, obj, SpinBox::STEP_SIZE.index)).abs(),
    });
}

fn act(engine: &Engine, obj: ObjectId, action: AccessibleAction) -> bool {
    if !takes_input(engine, obj) {
        return false;
    }
    match action {
        AccessibleAction::Increase => user_step(engine, obj, Side::Up, 1),
        AccessibleAction::Decrease => user_step(engine, obj, Side::Down, 1),
        AccessibleAction::SetValue(v) if v.is_finite() => {
            let v = engine.untrack(|| clamped(engine, obj, v.round() as i64));
            if v != engine.untrack(|| int(engine, obj, SpinBox::VALUE.index)) {
                set_value(engine, obj, v);
                modified(engine, obj);
            }
            true
        }
        _ => false,
    }
}

pub(crate) fn register(engine: &Engine) {
    let meta = || MetaRef::Static(SpinBox::static_meta());
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
}
