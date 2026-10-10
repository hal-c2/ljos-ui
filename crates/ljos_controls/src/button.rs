//! `AbstractButton` and the buttons made of it: `Button`, `ToolButton`,
//! `CheckBox`, `RadioButton` and `Switch` (QtQuick.Templates).
//!
//! What a press, a release and a hold do, and in which order the signals
//! come, was learned by running `qml6` (Qt 6.11.2) with a handler on every
//! signal; `features/controls/abstract-button.feature` pins it.
//! Documentation: doc.qt.io/qt-6/qml-qtquick-controls-abstractbutton.html.
//!
//! TODO(focus): Space and Return do not press a button, and a button does
//! not take focus when clicked.
//! TODO(image): `icon` is stored; nothing draws it.

use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use ljos_engine::{
    Atom, Color, ConnectionId, Cx, Engine, EngineError, MetaRef, ObjectId, OverrideContent,
    OverrideId, PropIndex, QmlObject, QmlType, RestoreMode, Timer, Url, Value, WriteKind, qml_impl,
};
use ljos_quick::{
    DOUBLE_CLICK_INTERVAL, DRAG_THRESHOLD, Item, PointerEvent, PointerItem, child_items,
    contains_point, is_item, register_pointer_item,
};
use ljos_render::kurbo;
use ljos_runtime::Time;

use crate::action::Action;
use crate::control::{Control, hover, is_left, takes_input};
use crate::group;
use crate::parts::{PartKind, manage_part};
use crate::util::{bind_default, check, flag, is_a, is_unset, num, object, set_computed};

/// How long a press is held before `pressAndHold` (Qt's platform default,
/// as the offscreen platform has it).
const PRESS_AND_HOLD: Duration = Duration::from_millis(800);

/// `Qt.CheckState`.
pub(crate) mod check_state {
    pub const UNCHECKED: i32 = 0;
    pub const PARTIALLY_CHECKED: i32 = 1;
    pub const CHECKED: i32 = 2;
}

/// The `icon` group of a button and of an action.
#[derive(Clone, Debug, QmlObject)]
#[qml(name = "ButtonIcon", uncreatable)]
pub struct Icon {
    #[qml(notify)]
    pub name: Atom,
    #[qml(notify)]
    pub source: Url,
    #[qml(notify)]
    pub width: i32,
    #[qml(notify)]
    pub height: i32,
    #[qml(notify)]
    pub color: Color,
    #[qml(notify)]
    pub cache: bool,
}

impl Default for Icon {
    fn default() -> Self {
        Self {
            name: Atom::EMPTY,
            source: Url::default(),
            // The style of a control that shows an icon gives it a size.
            width: 0,
            height: 0,
            color: Color::TRANSPARENT,
            cache: true,
        }
    }
}

/// What a button remembers between pointer events. Not QML-visible.
#[derive(Debug, Default)]
pub(crate) struct ButtonState {
    /// A press that landed on the button has not ended.
    held: Cell<bool>,
    /// When the last click ended, for telling a double click.
    last_click: Cell<Option<Time>>,
    /// The held press already did what a release would do (a double
    /// click, a hold, a repeat): its release only says `released`.
    spent: Cell<bool>,
    /// The timer of the held press: hold or repeat.
    timer: Cell<Option<ObjectId>>,
    /// Where the held press started.
    press_at: Cell<(f64, f64)>,
    /// A `Switch` whose handle is being dragged, and the layer that holds
    /// its `position` meanwhile.
    drag: Cell<Option<OverrideId>>,
    /// The `ButtonGroup` the button is in.
    pub(crate) group: Cell<Option<ObjectId>>,
    /// The action whose `triggered` the button listens to.
    action: Cell<Option<(ObjectId, ConnectionId)>>,
    /// Which of the button's properties follow its action ([`follows`]).
    follows_action: Cell<u8>,
}

mod follows {
    pub const TEXT: u8 = 1;
    pub const CHECKABLE: u8 = 2;
    pub const CHECKED: u8 = 4;
    pub const ENABLED: u8 = 8;
}

fn button_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![(
        "Display",
        vec![
            ("IconOnly", 0),
            ("TextOnly", 1),
            ("TextBesideIcon", 2),
            ("TextUnderIcon", 3),
        ],
    )]
}

/// `AbstractButton`.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick.Templates",
    name = "AbstractButton",
    extends = Control,
    enums = button_enums
)]
pub struct AbstractButton {
    #[qml(base)]
    pub control: Control,
    #[qml(notify)]
    pub text: Atom,
    #[qml(notify)]
    pub display: i32,
    /// Follows `pressed` until it is given a value.
    #[qml(notify)]
    pub down: bool,
    #[qml(notify, readonly)]
    pub pressed: bool,
    #[qml(notify)]
    pub checked: bool,
    #[qml(notify)]
    pub checkable: bool,
    #[qml(notify)]
    pub auto_exclusive: bool,
    #[qml(notify)]
    pub auto_repeat: bool,
    #[qml(notify)]
    pub auto_repeat_delay: i32,
    #[qml(notify)]
    pub auto_repeat_interval: i32,
    #[qml(notify)]
    pub indicator: Option<ObjectId>,
    #[qml(group)]
    pub icon: Icon,
    #[qml(notify)]
    pub action: Option<ObjectId>,
    #[qml(notify, readonly)]
    pub press_x: f64,
    #[qml(notify, readonly)]
    pub press_y: f64,
    #[qml(notify, readonly)]
    pub implicit_indicator_width: f64,
    #[qml(notify, readonly)]
    pub implicit_indicator_height: f64,
    #[qml(skip)]
    pub(crate) state: Rc<ButtonState>,
}

impl Default for AbstractButton {
    fn default() -> Self {
        Self {
            control: Control::default(),
            text: Atom::EMPTY,
            display: 2,
            down: false,
            pressed: false,
            checked: false,
            checkable: false,
            auto_exclusive: false,
            auto_repeat: false,
            auto_repeat_delay: 300,
            auto_repeat_interval: 100,
            indicator: None,
            icon: Icon::default(),
            action: None,
            press_x: 0.0,
            press_y: 0.0,
            implicit_indicator_width: 0.0,
            implicit_indicator_height: 0.0,
            state: Rc::default(),
        }
    }
}

#[qml_impl]
impl AbstractButton {
    #[qml(signal, name = "pressed")]
    fn press_signal();
    #[qml(signal)]
    fn released();
    #[qml(signal)]
    fn canceled();
    #[qml(signal)]
    fn clicked();
    #[qml(signal)]
    fn toggled();
    #[qml(signal, name = "doubleClicked")]
    fn double_clicked();
    #[qml(signal, name = "pressAndHold")]
    fn press_and_hold();

    /// Flips `checked`, whether or not the button is checkable, and says
    /// nothing: `toggled` is for what the user did.
    #[qml(invokable)]
    fn toggle(cx: &Cx) -> Result<(), EngineError> {
        let on = cx.engine.untrack(|| cx.get(AbstractButton::CHECKED))?;
        set_checked(cx.engine, cx.this, !on)
    }

    /// What a click of the user does, without the pointer. Nothing on a
    /// button that is not enabled.
    #[qml(invokable)]
    fn click(cx: &Cx) -> Result<(), EngineError> {
        let (e, obj) = (cx.engine, cx.this);
        if !takes_input(e, obj) {
            return Ok(());
        }
        set_computed(e, obj, AbstractButton::PRESSED.index, Value::Bool(true))?;
        emit(e, obj, "pressed");
        set_computed(e, obj, AbstractButton::PRESSED.index, Value::Bool(false))?;
        trigger(e, obj);
        Ok(())
    }

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        init_button(cx.engine, cx.this)
    }
}

fn state_of(engine: &Engine, obj: ObjectId) -> Option<Rc<ButtonState>> {
    engine
        .with::<AbstractButton, _>(obj, |b| b.state.clone())
        .ok()
}

fn emit(engine: &Engine, obj: ObjectId, signal: &str) {
    if engine.is_alive(obj) {
        check(engine, signal, engine.emit_by_name(obj, signal, &[]));
    }
}

/// Sets `checked` the way a click does: a binding on it stays, as in Qt,
/// and gives the value again when what it reads changes.
pub(crate) fn set_checked(engine: &Engine, obj: ObjectId, on: bool) -> Result<(), EngineError> {
    let p = AbstractButton::CHECKED.index;
    if engine.untrack(|| flag(engine, obj, p)) == on {
        return Ok(());
    }
    if engine.is_bound(obj, p) {
        engine.write_derived(obj, p, Value::Bool(on))
    } else {
        engine.write(obj, p, Value::Bool(on), WriteKind::Imperative)
    }
}

fn init_button(engine: &Engine, obj: ObjectId) -> Result<(), EngineError> {
    bind_default(engine, obj, AbstractButton::DOWN.index, move |e| {
        Value::Bool(flag(e, obj, AbstractButton::PRESSED.index))
    })?;
    for (prop, size) in [
        (
            AbstractButton::IMPLICIT_INDICATOR_WIDTH,
            Item::IMPLICIT_WIDTH,
        ),
        (
            AbstractButton::IMPLICIT_INDICATOR_HEIGHT,
            Item::IMPLICIT_HEIGHT,
        ),
    ] {
        engine.bind_initial(obj, prop.index, move |e| {
            match object(e, obj, AbstractButton::INDICATOR.index) {
                Some(i) if is_item(e, i) => Value::Real(num(e, i, size.index)),
                _ => Value::Real(0.0),
            }
        })?;
    }
    manage_part(engine, obj, AbstractButton::INDICATOR.index, PartKind::Free)?;

    // A press does not outlive the button's being usable.
    for p in [Item::ENABLED.index, Item::VISIBLE.index] {
        engine.watch(obj, p, move |e, v| {
            if !v.to_boolean() {
                cancel(e, obj);
            }
        })?;
    }

    engine.watch(obj, AbstractButton::CHECKED.index, move |e, v| {
        checked_changed(e, obj, v.to_boolean());
    })?;
    if engine.untrack(|| flag(engine, obj, AbstractButton::CHECKED.index)) {
        checked_changed(engine, obj, true);
    }

    let action = engine.untrack(|| object(engine, obj, AbstractButton::ACTION.index));
    follow_action(engine, obj, action)?;
    engine.watch(obj, AbstractButton::ACTION.index, move |e, v| {
        let action = v.as_object().filter(|a| e.is_alive(*a));
        check(e, "action", follow_action(e, obj, action));
    })?;

    if is_a::<CheckBox>(engine, obj) {
        init_check_box(engine, obj)?;
    }
    if is_a::<Switch>(engine, obj) {
        init_switch(engine, obj)?;
    }
    crate::a11y::button_defaults(engine, obj)
}

/// `checked` changed, by whatever means: a group, or the buttons beside
/// an exclusive one, follow.
fn checked_changed(engine: &Engine, obj: ObjectId, on: bool) {
    let group = state_of(engine, obj).and_then(|s| s.group.get());
    if let Some(g) = group.filter(|g| engine.is_alive(*g)) {
        group::button_toggled(engine, g, obj, on);
        return;
    }
    if !on || !engine.untrack(|| flag(engine, obj, AbstractButton::AUTO_EXCLUSIVE.index)) {
        return;
    }
    // Exclusive among the buttons with the same parent item.
    let Some(parent) = engine.untrack(|| engine.visual_parent(obj)) else {
        return;
    };
    for other in engine.untrack(|| child_items(engine, parent)) {
        if other == obj || !is_a::<AbstractButton>(engine, other) {
            continue;
        }
        let exclusive_and_on = engine.untrack(|| {
            flag(engine, other, AbstractButton::AUTO_EXCLUSIVE.index)
                && flag(engine, other, AbstractButton::CHECKED.index)
        });
        let in_group = state_of(engine, other).is_some_and(|s| s.group.get().is_some());
        if exclusive_and_on && !in_group {
            check(engine, "autoExclusive", set_checked(engine, other, false));
        }
    }
}

/// Whether a click must leave a checked button checked: one of an
/// exclusive set is unchecked by checking another.
fn stays_checked(engine: &Engine, obj: ObjectId) -> bool {
    let group = state_of(engine, obj).and_then(|s| s.group.get());
    match group.filter(|g| engine.is_alive(*g)) {
        Some(g) => group::is_exclusive(engine, g),
        None => engine.untrack(|| flag(engine, obj, AbstractButton::AUTO_EXCLUSIVE.index)),
    }
}

/// The end of a click: the check state moves on, then `released` and
/// `clicked`. With an action, the action is triggered instead, and its
/// `triggered` is what makes the button say `clicked`.
fn trigger(engine: &Engine, obj: ObjectId) {
    let action = engine.untrack(|| object(engine, obj, AbstractButton::ACTION.index));
    if action.is_none() && engine.untrack(|| flag(engine, obj, AbstractButton::CHECKABLE.index)) {
        if is_a::<CheckBox>(engine, obj) {
            advance_check_box(engine, obj);
        } else {
            let on = engine.untrack(|| flag(engine, obj, AbstractButton::CHECKED.index));
            if !(on && stays_checked(engine, obj)) {
                check(engine, "checked", set_checked(engine, obj, !on));
                emit(engine, obj, "toggled");
            }
        }
    }
    emit(engine, obj, "released");
    if !engine.is_alive(obj) {
        return;
    }
    match action {
        Some(a) => {
            let r = engine.invoke_by_name(a, "trigger", &[Value::Object(obj)]);
            check(engine, "action", r.map(|_| ()));
        }
        None => emit(engine, obj, "clicked"),
    }
    let group = state_of(engine, obj).and_then(|s| s.group.get());
    if let Some(g) = group.filter(|g| engine.is_alive(*g) && engine.is_alive(obj)) {
        group::button_clicked(engine, g, obj);
    }
}

// ------------------------------------------------------------ the action

/// Makes the button follow `action` in what the button itself was not
/// told: its text, whether it can be and is checked, whether it is
/// enabled.
fn follow_action(
    engine: &Engine,
    obj: ObjectId,
    action: Option<ObjectId>,
) -> Result<(), EngineError> {
    let Some(state) = state_of(engine, obj) else {
        return Ok(());
    };
    if let Some((_, conn)) = state.action.take() {
        engine.disconnect(conn);
    }
    let Some(action) = action.filter(|a| is_a::<Action>(engine, *a)) else {
        return Ok(());
    };
    let conn = engine.connect_by_name(action, "triggered", move |e, _| {
        emit(e, obj, "clicked");
    })?;
    state.action.set(Some((action, conn)));

    let mut mine = state.follows_action.get();
    let mut follow = |bit: u8, prop: PropIndex, from: PropIndex| -> Result<(), EngineError> {
        if mine & bit != 0 || is_unset(engine, obj, prop) {
            mine |= bit;
            engine.bind(obj, prop, move |e| {
                if e.is_alive(action) {
                    e.read(action, from)
                } else {
                    e.untrack(|| e.read(obj, prop))
                }
            })?;
        }
        Ok(())
    };
    follow(
        follows::TEXT,
        AbstractButton::TEXT.index,
        Action::TEXT.index,
    )?;
    follow(
        follows::CHECKABLE,
        AbstractButton::CHECKABLE.index,
        Action::CHECKABLE.index,
    )?;
    follow(
        follows::CHECKED,
        AbstractButton::CHECKED.index,
        Action::CHECKED.index,
    )?;
    follow(follows::ENABLED, Item::ENABLED.index, Action::ENABLED.index)?;
    state.follows_action.set(mine);
    Ok(())
}

// ----------------------------------------------------------- the pointer

fn set_pressed(engine: &Engine, obj: ObjectId, on: bool) {
    let r = set_computed(engine, obj, AbstractButton::PRESSED.index, Value::Bool(on));
    check(engine, "pressed", r);
}

fn press(engine: &Engine, obj: ObjectId, ev: &PointerEvent, at: kurbo::Point) -> bool {
    if !takes_input(engine, obj) || !is_left(ev) {
        return false;
    }
    let Some(st) = state_of(engine, obj) else {
        return false;
    };
    st.held.set(true);
    st.press_at.set((at.x, at.y));
    let double = st
        .last_click
        .take()
        .is_some_and(|t| ev.time.saturating_since(t) <= DOUBLE_CLICK_INTERVAL);
    st.spent.set(double);
    let _ = set_computed(
        engine,
        obj,
        AbstractButton::PRESS_X.index,
        Value::Real(at.x),
    );
    let _ = set_computed(
        engine,
        obj,
        AbstractButton::PRESS_Y.index,
        Value::Real(at.y),
    );
    set_pressed(engine, obj, true);
    emit(engine, obj, "pressed");
    if double {
        emit(engine, obj, "doubleClicked");
    }
    if !engine.is_alive(obj) || !st.held.get() {
        // A handler destroyed or disabled the button.
        return true;
    }
    let repeats = engine.untrack(|| flag(engine, obj, AbstractButton::AUTO_REPEAT.index));
    if repeats {
        let delay = engine.untrack(|| num(engine, obj, AbstractButton::AUTO_REPEAT_DELAY.index));
        start_timer(engine, obj, &st, delay as i32);
    } else if hold_is_wanted(engine, obj) {
        start_timer(engine, obj, &st, PRESS_AND_HOLD.as_millis() as i32);
    }
    true
}

/// Qt only waits for a hold when something listens for it; otherwise a
/// long press is a click like any other.
fn hold_is_wanted(engine: &Engine, obj: ObjectId) -> bool {
    engine
        .signal_index(obj, "pressAndHold")
        .is_ok_and(|s| engine.signal_connected(obj, s))
}

fn moved(engine: &Engine, obj: ObjectId, _ev: &PointerEvent, at: kurbo::Point) {
    let Some(st) = state_of(engine, obj) else {
        return;
    };
    if !st.held.get() {
        return;
    }
    if is_a::<Switch>(engine, obj) && drag_switch(engine, obj, &st, at) {
        return;
    }
    // Leaving the button lets go of it; coming back takes it again.
    set_pressed(engine, obj, contains_point(engine, obj, at));
}

fn release(engine: &Engine, obj: ObjectId, ev: &PointerEvent, _at: kurbo::Point) {
    let Some(st) = state_of(engine, obj) else {
        return;
    };
    if !st.held.replace(false) {
        return;
    }
    stop_timer(engine, &st);
    let was_pressed = engine.untrack(|| flag(engine, obj, AbstractButton::PRESSED.index));
    set_pressed(engine, obj, false);
    if let Some(layer) = st.drag.take() {
        drop_switch(engine, obj, layer);
        return;
    }
    if !was_pressed {
        emit(engine, obj, "canceled");
        return;
    }
    if st.spent.get() {
        emit(engine, obj, "released");
        return;
    }
    st.last_click.set(Some(ev.time));
    trigger(engine, obj);
}

fn cancel(engine: &Engine, obj: ObjectId) {
    let Some(st) = state_of(engine, obj) else {
        return;
    };
    if !st.held.replace(false) {
        return;
    }
    stop_timer(engine, &st);
    if let Some(layer) = st.drag.take() {
        let p = Switch::POSITION.index;
        let _ = engine.remove_override(obj, p, layer, RestoreMode::BindingOrValue);
    }
    if engine.untrack(|| flag(engine, obj, AbstractButton::PRESSED.index)) {
        set_pressed(engine, obj, false);
        emit(engine, obj, "canceled");
    }
}

/// Arms the button's timer to fire once, `ms` from now. The timer is an
/// engine `Timer` the button owns, so it runs on the engine's clock and
/// nothing polls.
fn start_timer(engine: &Engine, obj: ObjectId, st: &ButtonState, ms: i32) {
    let timer = match st.timer.get().filter(|t| engine.is_alive(*t)) {
        Some(t) => t,
        None => {
            let made = engine
                .create_native(Timer::default(), Some(obj))
                .and_then(|t| {
                    engine.complete(t)?;
                    engine.connect_by_name(t, "triggered", move |e, _| timer_fired(e, obj))?;
                    Ok(t)
                });
            match made {
                Ok(t) => {
                    st.timer.set(Some(t));
                    t
                }
                Err(err) => {
                    check(engine, "timer", Err(err));
                    return;
                }
            }
        }
    };
    let r = engine
        .set(timer, Timer::INTERVAL, ms.max(1))
        .and_then(|()| engine.invoke_by_name(timer, "restart", &[]).map(|_| ()));
    check(engine, "timer", r);
}

fn stop_timer(engine: &Engine, st: &ButtonState) {
    if let Some(t) = st.timer.get().filter(|t| engine.is_alive(*t)) {
        let _ = engine.set(t, Timer::RUNNING, false);
    }
}

fn timer_fired(engine: &Engine, obj: ObjectId) {
    let Some(st) = state_of(engine, obj) else {
        return;
    };
    if !st.held.get() {
        return;
    }
    let pressed = engine.untrack(|| flag(engine, obj, AbstractButton::PRESSED.index));
    if engine.untrack(|| flag(engine, obj, AbstractButton::AUTO_REPEAT.index)) {
        if pressed {
            // Each repeat is a whole click and the start of the next.
            st.spent.set(true);
            trigger(engine, obj);
            emit(engine, obj, "pressed");
        }
        if engine.is_alive(obj) && st.held.get() {
            let every =
                engine.untrack(|| num(engine, obj, AbstractButton::AUTO_REPEAT_INTERVAL.index));
            start_timer(engine, obj, &st, every as i32);
        }
    } else if pressed {
        st.spent.set(true);
        emit(engine, obj, "pressAndHold");
    }
}

pub(crate) fn register(engine: &Engine) {
    register_pointer_item(
        engine,
        MetaRef::Static(AbstractButton::static_meta()),
        PointerItem {
            press,
            moved,
            release,
            cancel,
            hover,
        },
    );
}

// --------------------------------------------------------------- Button

/// `Button`.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick.Templates", name = "Button", extends = AbstractButton)]
pub struct Button {
    #[qml(base)]
    pub button: AbstractButton,
    #[qml(notify)]
    pub highlighted: bool,
    #[qml(notify)]
    pub flat: bool,
}

/// `ToolButton`.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick.Templates", name = "ToolButton", extends = Button)]
pub struct ToolButton {
    #[qml(base)]
    pub button: Button,
}

/// `RadioButton`: checkable, and exclusive among the radio buttons beside
/// it.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick.Templates", name = "RadioButton", extends = AbstractButton)]
pub struct RadioButton {
    #[qml(base)]
    pub button: AbstractButton,
}

impl Default for RadioButton {
    fn default() -> Self {
        Self {
            button: AbstractButton {
                checkable: true,
                auto_exclusive: true,
                ..AbstractButton::default()
            },
        }
    }
}

// ------------------------------------------------------------- CheckBox

/// `CheckBox`.
///
/// TODO(js): `nextCheckState` is stored and not called; the engine has no
/// way yet to call a function a property holds from native code.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick.Templates", name = "CheckBox", extends = AbstractButton)]
pub struct CheckBox {
    #[qml(base)]
    pub button: AbstractButton,
    #[qml(notify)]
    pub tristate: bool,
    /// A `Qt.CheckState`.
    #[qml(notify)]
    pub check_state: i32,
    #[qml(notify)]
    pub next_check_state: Value,
}

impl Default for CheckBox {
    fn default() -> Self {
        Self {
            button: AbstractButton {
                checkable: true,
                ..AbstractButton::default()
            },
            tristate: false,
            check_state: check_state::UNCHECKED,
            next_check_state: Value::Undefined,
        }
    }
}

fn init_check_box(engine: &Engine, obj: ObjectId) -> Result<(), EngineError> {
    let state_p = CheckBox::CHECK_STATE.index;
    let checked_p = AbstractButton::CHECKED.index;
    // `checked` and `checkState` say the same thing twice; whichever the
    // author gave decides the other.
    if !is_unset(engine, obj, state_p) {
        let s = engine.untrack(|| num(engine, obj, state_p)) as i32;
        set_checked(engine, obj, s == check_state::CHECKED)?;
    } else if engine.untrack(|| flag(engine, obj, checked_p)) {
        engine.write_derived(obj, state_p, Value::Int(check_state::CHECKED))?;
    }
    engine.watch(obj, state_p, move |e, v| {
        let on = v.to_number() as i32 == check_state::CHECKED;
        check(e, "checkState", set_checked(e, obj, on));
    })?;
    engine.watch(obj, checked_p, move |e, v| {
        let on = v.to_boolean();
        let s = e.untrack(|| num(e, obj, state_p)) as i32;
        if (s == check_state::CHECKED) != on {
            let s = if on {
                check_state::CHECKED
            } else {
                check_state::UNCHECKED
            };
            check(e, "checked", set_check_state(e, obj, s));
        }
    })?;
    Ok(())
}

fn set_check_state(engine: &Engine, obj: ObjectId, s: i32) -> Result<(), EngineError> {
    let p = CheckBox::CHECK_STATE.index;
    if engine.is_bound(obj, p) {
        engine.write_derived(obj, p, Value::Int(s))
    } else {
        engine.write(obj, p, Value::Int(s), WriteKind::Imperative)
    }
}

/// What a click does to a check box. With three states it goes round
/// them, and does not say `toggled` (checked on qml6).
fn advance_check_box(engine: &Engine, obj: ObjectId) {
    let s = engine.untrack(|| num(engine, obj, CheckBox::CHECK_STATE.index)) as i32;
    let tristate = engine.untrack(|| flag(engine, obj, CheckBox::TRISTATE.index));
    if tristate {
        check(
            engine,
            "checkState",
            set_check_state(engine, obj, (s + 1) % 3),
        );
        return;
    }
    let next = if s == check_state::CHECKED {
        check_state::UNCHECKED
    } else {
        check_state::CHECKED
    };
    if next == check_state::UNCHECKED && stays_checked(engine, obj) {
        return;
    }
    check(engine, "checkState", set_check_state(engine, obj, next));
    emit(engine, obj, "toggled");
}

// --------------------------------------------------------------- Switch

/// `Switch`.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick.Templates", name = "Switch", extends = AbstractButton)]
pub struct Switch {
    #[qml(base)]
    pub button: AbstractButton,
    /// 0 off, 1 on, between while the handle is dragged.
    #[qml(notify)]
    pub position: f64,
    /// `position`, from the left: the other way round when mirrored.
    #[qml(notify, readonly)]
    pub visual_position: f64,
}

impl Default for Switch {
    fn default() -> Self {
        Self {
            button: AbstractButton {
                checkable: true,
                ..AbstractButton::default()
            },
            position: 0.0,
            visual_position: 0.0,
        }
    }
}

fn init_switch(engine: &Engine, obj: ObjectId) -> Result<(), EngineError> {
    bind_default(engine, obj, Switch::POSITION.index, move |e| {
        Value::Real(f64::from(u8::from(flag(
            e,
            obj,
            AbstractButton::CHECKED.index,
        ))))
    })?;
    engine.bind_initial(obj, Switch::VISUAL_POSITION.index, move |e| {
        let p = num(e, obj, Switch::POSITION.index);
        Value::Real(if flag(e, obj, Control::MIRRORED.index) {
            1.0 - p
        } else {
            p
        })
    })
}

/// Moves the handle with the pointer once it has gone far enough to be a
/// drag. True while dragging: the press then stays with the switch
/// wherever the pointer is.
fn drag_switch(engine: &Engine, obj: ObjectId, st: &ButtonState, at: kurbo::Point) -> bool {
    let (x0, _) = st.press_at.get();
    if st.drag.get().is_none() && (at.x - x0).abs() <= DRAG_THRESHOLD {
        return false;
    }
    let Some(indicator) = engine.untrack(|| object(engine, obj, AbstractButton::INDICATOR.index))
    else {
        return false;
    };
    let (ix, iw, mirrored) = engine.untrack(|| {
        (
            num(engine, indicator, Item::X.index),
            num(engine, indicator, Item::WIDTH.index),
            flag(engine, obj, Control::MIRRORED.index),
        )
    });
    if iw <= 0.0 {
        return false;
    }
    let visual = ((at.x - ix) / iw).clamp(0.0, 1.0);
    let pos = Value::Real(if mirrored { 1.0 - visual } else { visual });
    let p = Switch::POSITION.index;
    let r = match st.drag.get() {
        Some(layer) => engine.write_internal(obj, p, pos, WriteKind::Override(layer)),
        None => engine
            .push_override(obj, p, OverrideContent::Value(pos))
            .map(|id| st.drag.set(Some(id))),
    };
    check(engine, "position", r);
    // A drag cannot become a hold or a double click.
    stop_timer(engine, st);
    true
}

/// The end of a drag: the switch is on if the handle is past the middle.
fn drop_switch(engine: &Engine, obj: ObjectId, layer: OverrideId) {
    let p = Switch::POSITION.index;
    let on = engine.untrack(|| num(engine, obj, p)) >= 0.5;
    let was = engine.untrack(|| flag(engine, obj, AbstractButton::CHECKED.index));
    let _ = engine.remove_override(obj, p, layer, RestoreMode::BindingOrValue);
    if on != was {
        check(engine, "checked", set_checked(engine, obj, on));
        emit(engine, obj, "toggled");
    }
    emit(engine, obj, "released");
    emit(engine, obj, "clicked");
}
