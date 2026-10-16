//! `ToolTip` (QtQuick.Templates) and the attached `ToolTip` properties.
//!
//! A tool tip is a popup with a text, a `delay` before it shows and a
//! `timeout` after which it goes away. Both are engine timers that exist
//! and run only while a tool tip waits: nothing polls.
//!
//! `ToolTip.text`, `ToolTip.visible` and the rest attach to any item and
//! drive one tool tip that the items of a window share, so that a
//! hundred buttons with a hint cost one popup.
//!
//! Documentation: doc.qt.io/qt-6/qml-qtquick-controls-tooltip.html;
//! pinned by `features/controls/tooltip*.feature`.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use ljos_a11y::Role;
use ljos_engine::{
    Atom, Cx, Engine, EngineError, MetaRef, ObjectId, QmlObject, QmlType, Timer, Value, WriteKind,
    qml_impl,
};

use crate::containers::Page;
use crate::popup::{self, Popup, close_policy};
use crate::style::BASIC;
use crate::util::{check, flag, is_a, object, warn};

/// What a tool tip remembers that QML does not see.
#[derive(Debug, Default)]
pub(crate) struct TipState {
    /// The timers, made when first needed.
    delay: Cell<Option<ObjectId>>,
    timeout: Cell<Option<ObjectId>>,
    /// The tool tip was asked to open and waits for its delay.
    pending: Cell<bool>,
    /// The delay has passed: this opening is the one that was waited for.
    due: Cell<bool>,
    /// The item whose attached properties show this (shared) tool tip.
    user: Cell<Option<ObjectId>>,
}

/// `ToolTip`.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick.Templates",
    name = "ToolTip",
    extends = Popup,
    attached = ToolTipAttached
)]
pub struct ToolTip {
    #[qml(base)]
    pub popup: Popup,
    #[qml(notify)]
    pub text: Atom,
    /// Milliseconds between being asked to open and showing.
    #[qml(notify)]
    pub delay: i32,
    /// Milliseconds after which it closes by itself; none if not
    /// positive.
    #[qml(notify)]
    pub timeout: i32,
    #[qml(skip)]
    pub(crate) state: Rc<TipState>,
}

impl Default for ToolTip {
    fn default() -> Self {
        // A tool tip goes away with the pointer: a press on what it is
        // about does not close it.
        let popup = Popup {
            close_policy: close_policy::ON_ESCAPE
                | close_policy::ON_PRESS_OUTSIDE_PARENT
                | close_policy::ON_RELEASE_OUTSIDE_PARENT,
            ..Popup::default()
        };
        Self {
            popup,
            text: Atom::EMPTY,
            delay: 0,
            timeout: -1,
            state: Rc::default(),
        }
    }
}

/// The time a `show` was given: a number that is not negative.
fn time_arg(v: &Value) -> Option<i32> {
    match v {
        Value::Int(_) | Value::Real(_) => Some(v.to_number() as i32).filter(|ms| *ms >= 0),
        _ => None,
    }
}

fn text_arg(e: &Engine, v: &Value) -> Atom {
    match v {
        Value::String(s) => s.clone(),
        Value::Undefined | Value::Null => Atom::EMPTY,
        other => Atom::from(e.to_js_string(other).as_str()),
    }
}

#[qml_impl]
impl ToolTip {
    /// Shows `text`, for `timeout` milliseconds if that is given.
    #[qml(invokable)]
    fn show(cx: &Cx, text: Value, timeout: Value) -> Result<(), EngineError> {
        let (e, tip) = (cx.engine, cx.this);
        cx.set(ToolTip::TEXT, text_arg(e, &text))?;
        if let Some(ms) = time_arg(&timeout) {
            cx.set(ToolTip::TIMEOUT, ms)?;
        }
        popup::open(e, tip)
    }

    #[qml(invokable)]
    fn hide(cx: &Cx) -> Result<(), EngineError> {
        popup::close(cx.engine, cx.this)
    }

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        init_tooltip(cx.engine, cx.this)
    }
}

fn state_of(engine: &Engine, tip: ObjectId) -> Option<Rc<TipState>> {
    engine.with::<ToolTip, _>(tip, |t| t.state.clone()).ok()
}

fn int(engine: &Engine, obj: ObjectId, prop: ljos_engine::Prop<i32>) -> i32 {
    engine.untrack(|| engine.read(obj, prop.index)).to_number() as i32
}

fn is_visible(engine: &Engine, tip: ObjectId) -> bool {
    engine.untrack(|| flag(engine, tip, Popup::VISIBLE.index))
}

fn init_tooltip(e: &Engine, tip: ObjectId) -> Result<(), EngineError> {
    e.watch(tip, ToolTip::TIMEOUT.index, move |e, _| {
        // A new timeout counts from when it is given.
        if popup::is_shown(e, tip) {
            arm_timeout(e, tip);
        }
    })?;
    e.connect_by_name(tip, "opened", move |e, _| {
        arm_timeout(e, tip);
        tell_user(e, tip, true);
    })?;
    e.connect_by_name(tip, "closed", move |e, _| {
        if let Some(st) = state_of(e, tip) {
            stop(e, &st.timeout);
        }
        tell_user(e, tip, false);
    })?;
    if let Some(item) = popup::item_of(e, tip) {
        // The popup's item is a page, which a title names: a screen
        // reader reads the text.
        popup::follow(e, item, Page::TITLE.index, tip, ToolTip::TEXT.index)?;
    }
    popup::set_role(e, tip, Role::ToolTip)
}

// ---------------------------------------------------------------- timers

/// An engine timer owned by `tip` that calls `fired` once, `ms` from
/// now. It runs on the engine's clock; nothing polls.
fn arm(
    e: &Engine,
    tip: ObjectId,
    slot: &Cell<Option<ObjectId>>,
    ms: i32,
    fired: fn(&Engine, ObjectId),
) {
    let timer = match slot.get().filter(|t| e.is_alive(*t)) {
        Some(t) => Ok(t),
        None => e
            .create_native_incomplete(Timer::default(), Some(tip))
            .and_then(|t| {
                e.complete(t)?;
                e.connect_by_name(t, "triggered", move |e, _| fired(e, tip))?;
                slot.set(Some(t));
                Ok(t)
            }),
    };
    let r = timer.and_then(|t| {
        e.set(t, Timer::INTERVAL, ms.max(1))?;
        e.invoke_by_name(t, "restart", &[]).map(|_| ())
    });
    check(e, "tool tip", r);
}

fn stop(e: &Engine, slot: &Cell<Option<ObjectId>>) {
    if let Some(t) = slot.get().filter(|t| e.is_alive(*t)) {
        let _ = e.set(t, Timer::RUNNING, false);
    }
}

fn arm_timeout(e: &Engine, tip: ObjectId) {
    let Some(st) = state_of(e, tip) else {
        return;
    };
    let ms = int(e, tip, ToolTip::TIMEOUT);
    if ms > 0 {
        arm(e, tip, &st.timeout, ms, timed_out);
    } else {
        stop(e, &st.timeout);
    }
}

fn timed_out(e: &Engine, tip: ObjectId) {
    check(e, "tool tip", popup::close(e, tip));
}

fn delay_passed(e: &Engine, tip: ObjectId) {
    let Some(st) = state_of(e, tip) else {
        return;
    };
    if !st.pending.replace(false) {
        return;
    }
    st.due.set(true);
    // `visible` is already true if it was written and not `open()` that
    // asked.
    let r = if is_visible(e, tip) {
        popup::visible_written(e, tip, true)
    } else {
        popup::open(e, tip)
    };
    st.due.set(false);
    check(e, "tool tip", r);
}

/// Called when a popup is asked to open. True if it is a tool tip that
/// waits for its delay first: it opens when that has passed.
pub(crate) fn defer(e: &Engine, popup: ObjectId) -> bool {
    let Some(st) = state_of(e, popup) else {
        return false;
    };
    if st.due.get() {
        return false;
    }
    let ms = int(e, popup, ToolTip::DELAY);
    if ms <= 0 {
        return false;
    }
    if !st.pending.replace(true) {
        arm(e, popup, &st.delay, ms, delay_passed);
    }
    true
}

/// Called when a popup is asked to close. True if it is a tool tip that
/// was waiting for its delay: there is nothing to hide then.
pub(crate) fn cancel(e: &Engine, popup: ObjectId) -> bool {
    let Some(st) = state_of(e, popup) else {
        return false;
    };
    if !st.pending.replace(false) {
        return false;
    }
    stop(e, &st.delay);
    true
}

/// Whether `popup` is a tool tip that waits for its delay.
pub(crate) fn waiting(e: &Engine, popup: ObjectId) -> bool {
    state_of(e, popup).is_some_and(|s| s.pending.get())
}

// -------------------------------------------------------------- attached

/// What `ToolTip.text: …` attaches to an item.
#[derive(Debug, QmlObject)]
#[qml(name = "ToolTipAttached")]
pub struct ToolTipAttached {
    #[qml(notify)]
    pub text: Atom,
    #[qml(notify)]
    pub delay: i32,
    #[qml(notify)]
    pub timeout: i32,
    /// Whether the shared tool tip is shown for this item.
    #[qml(notify)]
    pub visible: bool,
    /// The tool tip the items of the window share.
    #[qml(notify, readonly)]
    pub tool_tip: Option<ObjectId>,
}

impl Default for ToolTipAttached {
    fn default() -> Self {
        Self {
            text: Atom::EMPTY,
            delay: 0,
            timeout: -1,
            visible: false,
            tool_tip: None,
        }
    }
}

#[qml_impl]
impl ToolTipAttached {
    /// Shows `text` in the shared tool tip, at this item, for `timeout`
    /// milliseconds if that is given.
    #[qml(invokable)]
    fn show(cx: &Cx, text: Value, timeout: Value) -> Result<(), EngineError> {
        let e = cx.engine;
        let Some(owner) = e.owner(cx.this) else {
            return Ok(());
        };
        show_for(
            e,
            owner,
            cx.this,
            Some(text_arg(e, &text)),
            time_arg(&timeout),
        )
    }

    /// Hides the shared tool tip if this item shows it.
    #[qml(invokable)]
    fn hide(cx: &Cx) -> Result<(), EngineError> {
        let e = cx.engine;
        match e.owner(cx.this) {
            Some(owner) => hide_for(e, owner, cx.this),
            None => Ok(()),
        }
    }
}

fn attacher() -> MetaRef {
    MetaRef::Static(ToolTip::static_meta())
}

/// The shared tool tips of an engine, by the window they are for. One
/// that was made before its users were in a window has no window yet.
#[derive(Default)]
struct SharedTips(RefCell<Vec<(Option<ObjectId>, ObjectId)>>);

/// The tool tip that `owner` shares with the other items of its window,
/// made if there is none. In a binding it follows the item to another
/// window.
fn shared_for(e: &Engine, owner: ObjectId) -> Option<ObjectId> {
    let top = ljos_quick::window_of(e, owner);
    e.untrack(|| {
        let shared = e.extension::<SharedTips>();
        {
            let mut tips = shared.0.borrow_mut();
            tips.retain(|(window, tip)| e.is_alive(*tip) && window.is_none_or(|w| e.is_alive(w)));
            if let Some((_, tip)) = tips.iter().find(|(window, _)| *window == top) {
                return Some(*tip);
            }
            // The one made before there was a window is the first
            // window's; an item outside every window uses what there is.
            if let Some(entry) = tips.iter_mut().find(|(window, _)| window.is_none()) {
                entry.0 = top;
                return Some(entry.1);
            }
            if top.is_none()
                && let Some((_, tip)) = tips.first()
            {
                return Some(*tip);
            }
        }
        // The style's tool tip; the bare template if there is no style.
        let made = e
            .create_from_url(&format!("{BASIC}/ToolTip"), None, Vec::new())
            .or_else(|_| {
                let tip = e.create_native_incomplete(ToolTip::default(), None)?;
                e.complete(tip)?;
                Ok::<_, EngineError>(tip)
            });
        match made {
            Ok(tip) if is_a::<ToolTip>(e, tip) => {
                shared.0.borrow_mut().push((top, tip));
                Some(tip)
            }
            Ok(other) => {
                e.destroy(other);
                None
            }
            Err(err) => {
                warn(e, format!("ToolTip: no shared tool tip: {err}"));
                None
            }
        }
    })
}

/// The shared tool tip `att` knows of, if it is showing, or waiting to
/// show, for `owner`.
fn showing_for(e: &Engine, owner: ObjectId, att: ObjectId) -> Option<ObjectId> {
    let tip = e.untrack(|| object(e, att, ToolTipAttached::TOOL_TIP.index))?;
    let st = state_of(e, tip)?;
    (st.user.get() == Some(owner) && (is_visible(e, tip) || st.pending.get())).then_some(tip)
}

fn put_visible(e: &Engine, owner: ObjectId, on: bool) {
    if !e.is_alive(owner) {
        return;
    }
    if let Some(att) = e.existing_attached_object(owner, &attacher()) {
        let r = popup::put(e, att, ToolTipAttached::VISIBLE.index, Value::Bool(on));
        check(e, "ToolTip.visible", r);
    }
}

/// Makes `ToolTip.visible` of the item that uses a shared tool tip say
/// whether the tool tip is shown.
fn tell_user(e: &Engine, tip: ObjectId, on: bool) {
    if let Some(user) = state_of(e, tip).and_then(|s| s.user.get()) {
        put_visible(e, user, on);
    }
}

/// Shows the shared tool tip at `owner`, with the text and the times of
/// its attached properties unless others are given.
fn show_for(
    e: &Engine,
    owner: ObjectId,
    att: ObjectId,
    text: Option<Atom>,
    timeout: Option<i32>,
) -> Result<(), EngineError> {
    let Some(tip) = shared_for(e, owner) else {
        return Ok(());
    };
    let Some(st) = state_of(e, tip) else {
        return Ok(());
    };
    let before = st.user.replace(Some(owner));
    if let Some(other) = before.filter(|b| *b != owner) {
        // The tool tip has moved on.
        put_visible(e, other, false);
        cancel(e, tip);
    }
    let read = |prop| e.untrack(|| e.read(att, prop));
    e.write(
        tip,
        Popup::PARENT.index,
        Value::Object(owner),
        WriteKind::Imperative,
    )?;
    let text = text.map_or_else(|| read(ToolTipAttached::TEXT.index), Value::String);
    popup::put(e, tip, ToolTip::TEXT.index, text)?;
    popup::put(
        e,
        tip,
        ToolTip::DELAY.index,
        read(ToolTipAttached::DELAY.index),
    )?;
    let timeout = timeout.map_or_else(|| read(ToolTipAttached::TIMEOUT.index), Value::Int);
    popup::put(e, tip, ToolTip::TIMEOUT.index, timeout)?;
    if is_visible(e, tip) && !st.pending.get() {
        // Already shown for another item: it stays, for as long as this
        // one says.
        arm_timeout(e, tip);
        put_visible(e, owner, true);
        return Ok(());
    }
    popup::open(e, tip)
}

fn hide_for(e: &Engine, owner: ObjectId, att: ObjectId) -> Result<(), EngineError> {
    let Some(tip) = showing_for(e, owner, att) else {
        return Ok(());
    };
    popup::close(e, tip)?;
    // One that was still waiting was never shown and says nothing.
    put_visible(e, owner, false);
    Ok(())
}

fn attached_created(e: &Engine, owner: ObjectId, att: ObjectId) -> Result<(), EngineError> {
    e.bind_initial(att, ToolTipAttached::TOOL_TIP.index, move |e| {
        shared_for(e, owner).map_or(Value::Null, Value::Object)
    })?;
    e.watch(att, ToolTipAttached::VISIBLE.index, move |e, v| {
        let r = match (v.to_boolean(), showing_for(e, owner, att)) {
            (true, None) => show_for(e, owner, att, None, None),
            (false, Some(_)) => hide_for(e, owner, att),
            _ => Ok(()),
        };
        check(e, "ToolTip.visible", r);
    })?;
    // What the item says while its tool tip is up is what is shown.
    for (from, to) in [
        (ToolTipAttached::TEXT.index, ToolTip::TEXT.index),
        (ToolTipAttached::DELAY.index, ToolTip::DELAY.index),
        (ToolTipAttached::TIMEOUT.index, ToolTip::TIMEOUT.index),
    ] {
        e.watch(att, from, move |e, v| {
            if let Some(tip) = showing_for(e, owner, att) {
                check(e, "ToolTip", popup::put(e, tip, to, v.clone()));
            }
        })?;
    }
    Ok(())
}

pub(crate) fn register(engine: &Engine) {
    engine.on_attached_created(|e, owner, att| {
        if is_a::<ToolTipAttached>(e, att) {
            check(e, "ToolTip", attached_created(e, owner, att));
        }
        true
    });
}
