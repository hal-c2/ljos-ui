//! What `TextField` and `TextArea` add to the editor they are: a
//! background, a placeholder, hovering, the font and palette of the
//! controls around them, and the press, the release and the press and
//! hold they tell of.
//!
//! The press is the part that differs from a plain editor. A field keeps
//! a press to itself until it knows what it is: a press that moves or
//! ends is the editor's (it takes the keyboard, puts the cursor, selects),
//! a press that stays for `Qt.styleHints.mousePressAndHoldInterval` is a
//! press and hold and never reaches the editor. So a long press does not
//! take the keyboard, which is what `features/controls/text-field.feature` shows of Qt.
//! A move selects only once it has gone farther sideways than the drag
//! threshold: a drag straight up or down a TextArea selects nothing,
//! as `features/text-input/select-past-line-end.feature` shows of Qt.

use std::cell::Cell;

use ljos_engine::{Engine, EngineError, MetaRef, ObjectId, PropIndex, QmlType, Value};
use ljos_quick::focus::focus_reason;
use ljos_quick::{
    Accessible, AccessibleAttached, Item, MouseButtons, MouseEvent, PointerEvent, PointerItem,
    is_item, register_pointer_item, text_input,
};
use ljos_render::kurbo;

use crate::inherit::{self, Kind};
use crate::parts::{PartKind, manage_part, settle_parts};
use crate::text_area::TextArea;
use crate::text_field::TextField;
use crate::ticker;
use crate::util::{bind_default, check, flag, is_a, num, object, set_computed};

/// What a field remembers of the pointer.
#[derive(Debug, Default)]
pub(crate) struct FieldState {
    /// The press the field has told of and not yet given to its editor.
    held: Cell<Option<(PointerEvent, kurbo::Point)>>,
    /// Where the press the editor has landed, until a move has gone
    /// sideways far enough to select.
    still: Cell<Option<kurbo::Point>>,
    timer: Cell<Option<ObjectId>>,
}

/// The properties the two types have under the same names.
pub(crate) struct Props {
    pub kind: Kind,
    pub background: PropIndex,
    /// Left, top, right, bottom.
    pub insets: [PropIndex; 4],
    pub implicit_background: [PropIndex; 2],
    pub placeholder_text: PropIndex,
    pub hovered: PropIndex,
    pub hover_enabled: PropIndex,
    pub focus_reason: PropIndex,
    pub focus_policy: PropIndex,
}

fn props(engine: &Engine, obj: ObjectId) -> Option<&'static Props> {
    if is_a::<TextField>(engine, obj) {
        Some(TextField::props())
    } else if is_a::<TextArea>(engine, obj) {
        Some(TextArea::props())
    } else {
        None
    }
}

fn state(engine: &Engine, obj: ObjectId) -> Option<std::rc::Rc<FieldState>> {
    engine
        .with::<TextField, _>(obj, |f| f.state.clone())
        .or_else(|_| engine.with::<TextArea, _>(obj, |f| f.state.clone()))
        .ok()
}

/// What both types do when they complete, after the editor they are has.
pub(crate) fn completed(
    engine: &Engine,
    obj: ObjectId,
    p: &'static Props,
) -> Result<(), EngineError> {
    let implicit_of = |size: PropIndex| {
        move |e: &Engine| match object(e, obj, p.background) {
            Some(b) if is_item(e, b) => Value::Real(num(e, b, size)),
            _ => Value::Real(0.0),
        }
    };
    engine.bind_initial(
        obj,
        p.implicit_background[0],
        implicit_of(Item::IMPLICIT_WIDTH.index),
    )?;
    engine.bind_initial(
        obj,
        p.implicit_background[1],
        implicit_of(Item::IMPLICIT_HEIGHT.index),
    )?;
    settle_parts(engine, obj);
    manage_part(
        engine,
        obj,
        p.background,
        PartKind::Background(Some(p.insets)),
    )?;
    inherit::attach(engine, obj, p.kind, false)?;
    crate::focus::link_policy(engine, obj, p.focus_policy, true)?;

    // Why the keyboard came, and why it went: the scene knows both by the
    // time the flag changes.
    engine.watch(obj, Item::ACTIVE_FOCUS.index, move |e, _| {
        let reason = focus_reason(e, obj) as i32;
        let r = set_computed(e, obj, p.focus_reason, Value::Int(reason));
        check(e, "focusReason", r);
    })?;

    // The placeholder says what the field is for. Qt does not say what it
    // tells a screen reader of it; a description is what the text is.
    let attacher = MetaRef::Static(Accessible::static_meta());
    let att = engine.attached_object(obj, &attacher)?;
    bind_default(
        engine,
        att,
        AccessibleAttached::DESCRIPTION.index,
        move |e| e.read(obj, p.placeholder_text),
    )
}

fn enabled(engine: &Engine, obj: ObjectId) -> bool {
    engine.untrack(|| flag(engine, obj, Item::ENABLED.index))
}

/// Emits `signal` with a `MouseEvent` at `at`, as a `MouseArea` does.
fn tell(
    engine: &Engine,
    obj: ObjectId,
    signal: &str,
    ev: &PointerEvent,
    at: kurbo::Point,
    held: bool,
) {
    let native = MouseEvent {
        x: at.x,
        y: at.y,
        button: ev.button.map_or(0, |b| b.bits() as i32),
        buttons: ev.buttons.bits() as i32,
        modifiers: ev.modifiers.bits() as i32,
        was_held: held,
        ..MouseEvent::default()
    };
    let Ok(me) = engine.create_native_incomplete(native, Some(obj)) else {
        return;
    };
    check(
        engine,
        signal,
        engine.emit_by_name(obj, signal, &[Value::Object(me)]),
    );
    engine.destroy_now(me);
}

/// Gives the editor the press the field kept, as it was when it landed:
/// the editor tells a double click by its time.
fn hand_over(engine: &Engine, obj: ObjectId, st: &FieldState) -> Option<kurbo::Point> {
    let (ev, at) = st.held.take()?;
    ticker::stop(engine, &st.timer);
    (text_input::pointer_hooks().press)(engine, obj, &ev, at);
    Some(at)
}

fn press(engine: &Engine, obj: ObjectId, ev: &PointerEvent, at: kurbo::Point) -> bool {
    if ev.button != Some(MouseButtons::LEFT) || !enabled(engine, obj) {
        return false;
    }
    let Some(st) = state(engine, obj) else {
        return false;
    };
    st.held.set(Some((*ev, at)));
    st.still.set(Some(at));
    ticker::start(
        engine,
        obj,
        &st.timer,
        ljos_quick::press_and_hold_interval(engine),
        held_long,
    );
    tell(engine, obj, "pressed", ev, at, false);
    true
}

/// The press stayed: it is a press and hold, and the editor never hears
/// of it.
fn held_long(engine: &Engine, obj: ObjectId) {
    let Some(st) = state(engine, obj) else {
        return;
    };
    ticker::stop(engine, &st.timer);
    if let Some((ev, at)) = st.held.take() {
        tell(engine, obj, "pressAndHold", &ev, at, true);
    }
}

/// The pointer has not yet gone farther sideways from the press than the
/// drag threshold since the press landed; past it, it never is again.
fn still(st: &FieldState, at: kurbo::Point) -> bool {
    let near = st
        .still
        .get()
        .is_some_and(|from| (at.x - from.x).abs() <= ljos_quick::DRAG_THRESHOLD);
    if !near {
        st.still.set(None);
    }
    near
}

fn moved(engine: &Engine, obj: ObjectId, ev: &PointerEvent, at: kurbo::Point) {
    let Some(st) = state(engine, obj) else {
        return;
    };
    hand_over(engine, obj, &st);
    if still(&st, at) {
        return;
    }
    (text_input::pointer_hooks().moved)(engine, obj, ev, at);
}

fn release(engine: &Engine, obj: ObjectId, ev: &PointerEvent, at: kurbo::Point) {
    let Some(st) = state(engine, obj) else {
        return;
    };
    let hooks = text_input::pointer_hooks();
    if hand_over(engine, obj, &st).is_some() && !still(&st, at) {
        // Released somewhere else with no move in between: it selected
        // on its way all the same.
        (hooks.moved)(engine, obj, ev, at);
    }
    ticker::stop(engine, &st.timer);
    (hooks.release)(engine, obj, ev, at);
    tell(engine, obj, "released", ev, at, false);
}

fn cancel(engine: &Engine, obj: ObjectId) {
    if let Some(st) = state(engine, obj) {
        st.held.set(None);
        st.still.set(None);
        ticker::stop(engine, &st.timer);
    }
    (text_input::pointer_hooks().cancel)(engine, obj);
}

fn hover(engine: &Engine, obj: ObjectId, over: bool) {
    let Some(p) = props(engine, obj) else {
        return;
    };
    // A field that is not enabled is hovered too (checked on qml6).
    let on = over && engine.untrack(|| flag(engine, obj, p.hover_enabled));
    let r = set_computed(engine, obj, p.hovered, Value::Bool(on));
    check(engine, "hovered", r);
}

pub(crate) fn register(engine: &Engine) {
    // A field shows its text only inside its padding (checked on qml6);
    // a bare TextInput draws past its edges.
    text_input::register_clip_to_padding(engine, MetaRef::Static(TextField::static_meta()));
    for meta in [TextField::static_meta(), TextArea::static_meta()] {
        // The style says how large a field would like to be: its
        // background and its placeholder count, not only its text.
        text_input::register_own_implicit_size(engine, MetaRef::Static(meta));
        register_pointer_item(
            engine,
            MetaRef::Static(meta),
            PointerItem {
                press,
                moved,
                release,
                cancel,
                hover,
            },
        );
    }
}
