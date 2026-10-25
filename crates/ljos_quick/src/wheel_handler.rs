//! `WheelHandler`: adds up how far the wheel turned over its item, in
//! degrees, and adds the same to a property of a target.
//!
//! What it does with an event was observed on qml6
//! (`features/input/wheel-handler*.feature`). Left out: `invertible` is
//! kept (no host tells of an inverted wheel), and `acceptedDevices`,
//! `acceptedButtons`, `point`, `cursorShape` and the grab properties are
//! not there.

use std::cell::Cell;
use std::rc::Rc;

use ljos_engine::{
    Cx, Engine, EngineError, LogLevel, ObjectId, QmlObject, QtObject, Timer, Value, WriteKind,
    qml_impl,
};
use ljos_render::kurbo;

use crate::flickable::wheel::wheel_angle;
use crate::input::{MouseArea, MouseButtons, PointerSource, WheelEvent};
use crate::item::{
    Item, is_a, is_item, is_unset, local_transform, scene_transform, write_computed,
};
use crate::scene::{SCENE_CATEGORY, item_enabled, mouse_area_enabled};
use crate::tree::resources;

/// `Qt.Horizontal`.
const HORIZONTAL: i32 = 1;
/// `Qt.Vertical`.
const VERTICAL: i32 = 2;
/// `Qt.KeyboardModifierMask`: the handler takes the wheel whatever is held.
const ANY_MODIFIERS: i32 = -0x0200_0000;
/// The turn of the wheel that multiplies a scale by `targetScaleMultiplier`.
const SCALE_STEP_DEGREES: f64 = 15.0;

/// What a handler remembers between events. Not properties: nothing binds
/// to it.
#[derive(Debug, Default)]
pub(crate) struct HandlerState {
    /// All that the wheel turned, in degrees, before `rotationScale`:
    /// `rotation` is this times the scale, so a new scale applies to what
    /// was turned before it too, as in Qt.
    turned: Cell<f64>,
    /// Ends `active` when the wheel has been quiet for `activeTimeout`.
    timer: Cell<Option<ObjectId>>,
}

/// `WheelHandler` (QtQuick). Its parent item is the object that owns it,
/// and it lives in that item's `resources`.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick",
    name = "WheelHandler",
    extends = QtObject,
    enums = crate::handlers::device_enums
)]
pub struct WheelHandler {
    #[qml(base)]
    pub object: QtObject,
    #[qml(notify)]
    pub enabled: bool,
    #[qml(notify, readonly)]
    pub parent: Option<ObjectId>,
    /// The item whose property changes; the parent unless one is given.
    #[qml(notify)]
    pub target: Option<ObjectId>,
    /// The name of the target's property that the wheel changes; none
    /// when empty.
    #[qml(notify)]
    pub property: String,
    /// What the wheel turned, in degrees times `rotationScale`: a notch of
    /// a common wheel is 15.
    #[qml(notify)]
    pub rotation: f64,
    #[qml(notify)]
    pub rotation_scale: f64,
    /// Which way of turning the handler adds up.
    #[qml(notify, enumeration = "Qt.Orientation")]
    pub orientation: i32,
    /// Every wheel of Ljós is a mouse's: no host tells a touchpad's apart.
    #[qml(notify, flags = "DeviceTypes")]
    pub accepted_devices: i32,
    #[qml(notify, flags = "PointerTypes")]
    pub accepted_pointer_types: i32,
    /// The modifiers that must be held, no more and no fewer;
    /// `Qt.KeyboardModifierMask` for any.
    #[qml(notify, flags = "Qt.KeyboardModifiers")]
    pub accepted_modifiers: i32,
    /// Kept: no host of Ljós tells that the system inverts the wheel.
    #[qml(notify)]
    pub invertible: bool,
    /// What a turn of 15 degrees multiplies the target's `scale` by.
    #[qml(notify)]
    pub target_scale_multiplier: f64,
    /// Whether a `rotation` or a `scale` of the target keeps the point
    /// under the pointer where it is.
    #[qml(notify)]
    pub target_transform_around_cursor: bool,
    /// True from an event the handler took until the wheel has been quiet
    /// for `activeTimeout`.
    #[qml(notify, readonly)]
    pub active: bool,
    /// In seconds.
    #[qml(notify)]
    pub active_timeout: f64,
    /// Whether an event the handler took stays with it.
    #[qml(notify)]
    pub blocking: bool,
    /// How far outside its item the handler still takes the wheel. The
    /// item keeps its place among the others there: what is over it at
    /// the pointer is asked first, what is under it after (qml6).
    #[qml(notify)]
    pub margin: f64,
    #[qml(skip)]
    pub(crate) state: Rc<HandlerState>,
}

impl Default for WheelHandler {
    fn default() -> Self {
        Self {
            object: QtObject::default(),
            enabled: true,
            parent: None,
            target: None,
            property: String::new(),
            rotation: 0.0,
            rotation_scale: 1.0,
            orientation: VERTICAL,
            accepted_devices: PointerSource::MOUSE.device,
            accepted_pointer_types: crate::handlers::ALL_POINTER_TYPES,
            accepted_modifiers: ANY_MODIFIERS,
            invertible: true,
            target_scale_multiplier: 2f64.powf(1.0 / 3.0),
            target_transform_around_cursor: true,
            active: false,
            active_timeout: 0.1,
            blocking: true,
            margin: 0.0,
            state: Rc::default(),
        }
    }
}

#[qml_impl]
impl WheelHandler {
    /// `wheel(event)`: the handler took the event. Refusing it there
    /// changes nothing, as in Qt.
    #[qml(signal)]
    fn wheel(event: Option<ObjectId>);

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, h) = (cx.engine, cx.this);
        let owner = e.owner(h);
        if cx.get(WheelHandler::PARENT)?.is_none() {
            e.set_internal(h, WheelHandler::PARENT, owner)?;
        }
        // `target: null` is a target of none, so only a target nobody
        // gave is the parent.
        if is_unset(e, h, WheelHandler::TARGET.index) {
            let parent = cx.get(WheelHandler::PARENT)?;
            e.set_internal(h, WheelHandler::TARGET, parent)?;
        }
        rotation_assigned(e, h);
        noted_margin(e, h);
        e.watch(h, WheelHandler::ROTATION.index, move |e, _| {
            rotation_assigned(e, h);
        })?;
        e.watch(h, WheelHandler::ROTATION_SCALE.index, move |e, _| {
            show_rotation(e, h);
        })?;
        e.watch(h, WheelHandler::MARGIN.index, move |e, _| {
            noted_margin(e, h);
        })?;
        Ok(())
    }
}

#[derive(Default)]
struct Margins(std::cell::RefCell<Vec<ObjectId>>);

fn noted_margin(engine: &Engine, h: ObjectId) {
    let m = engine.extension::<Margins>();
    let mut list = m.0.borrow_mut();
    list.retain(|o| engine.is_alive(*o) && *o != h);
    if real(engine, h, WheelHandler::MARGIN) > 0.0 {
        list.push(h);
    }
}

fn handler_item(engine: &Engine, h: ObjectId) -> Option<ObjectId> {
    engine
        .untrack(|| engine.read(h, WheelHandler::PARENT.index))
        .as_object()
        .or_else(|| engine.owner(h))
        .filter(|o| is_item(engine, *o))
}

fn item_on(engine: &Engine, item: ObjectId) -> bool {
    let on = if is_a::<MouseArea>(engine, item) {
        mouse_area_enabled(engine, item)
    } else {
        item_enabled(engine, item)
    };
    if !on {
        return false;
    }
    let mut cur = Some(item);
    while let Some(o) = cur {
        if is_item(engine, o) {
            let vis = engine
                .untrack(|| engine.read(o, crate::item::Item::VISIBLE.index))
                .to_boolean();
            let op = engine
                .untrack(|| engine.read(o, crate::item::Item::OPACITY.index))
                .to_number();
            if !vis || op <= 0.0 {
                return false;
            }
        }
        cur = engine.untrack(|| engine.visual_parent(o));
    }
    true
}

fn state_of(engine: &Engine, h: ObjectId) -> Option<Rc<HandlerState>> {
    engine.with(h, |w: &WheelHandler| w.state.clone()).ok()
}

fn real(engine: &Engine, h: ObjectId, p: ljos_engine::Prop<f64>) -> f64 {
    engine.untrack(|| engine.read(h, p.index)).to_number()
}

/// `rotation` got a value: the next turn goes on from it.
fn rotation_assigned(engine: &Engine, h: ObjectId) {
    let Some(st) = state_of(engine, h) else {
        return;
    };
    let rotation = real(engine, h, WheelHandler::ROTATION);
    let scale = real(engine, h, WheelHandler::ROTATION_SCALE);
    // What the handler wrote itself is left as it is, so that dividing
    // does not wear the sum down.
    if scale != 0.0 && rotation != st.turned.get() * scale {
        st.turned.set(rotation / scale);
    }
}

/// Makes `rotation` what was turned times the scale.
fn show_rotation(engine: &Engine, h: ObjectId) {
    let Some(st) = state_of(engine, h) else {
        return;
    };
    let scale = real(engine, h, WheelHandler::ROTATION_SCALE);
    let _ = write_computed(
        engine,
        h,
        WheelHandler::ROTATION.index,
        Value::Real(st.turned.get() * scale),
    );
}

/// What the handlers of an item did with an event.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Taken {
    /// None took it.
    No,
    /// One took it and lets the others have it too (`blocking: false`).
    Shared,
    /// One took it and keeps it from the item and from what is beneath.
    Kept,
}

/// Where `item` is in the paint order: compared, the greater is drawn
/// above. The same key `drop_area` builds, so a margin takes the place
/// the item itself would take at the pointer.
pub(crate) fn stack_key(engine: &Engine, item: ObjectId) -> Vec<(f64, usize)> {
    let mut key = Vec::new();
    let mut cur = item;
    for _ in 0..4096 {
        let Some(parent) = engine
            .untrack(|| engine.visual_parent(cur))
            .filter(|p| is_item(engine, *p))
        else {
            break;
        };
        let z = engine
            .untrack(|| engine.read(cur, crate::item::Item::Z.index))
            .to_number();
        let index = crate::tree::child_items(engine, parent)
            .iter()
            .position(|c| *c == cur)
            .unwrap_or(0);
        key.push((z, index));
        cur = parent;
    }
    key.reverse();
    key
}

/// The items whose WheelHandler takes the wheel at `pos` only by its
/// `margin`: the pointer is outside the item, no further than the
/// margin. Topmost first, as [`crate::wheel::deliver`] asks them among
/// the items that are under the pointer.
pub(crate) fn margin_items(engine: &Engine, pos: kurbo::Point) -> Vec<ObjectId> {
    let margins: Vec<ObjectId> = engine.extension::<Margins>().0.borrow().clone();
    let mut items: Vec<ObjectId> = Vec::new();
    for h in margins {
        if !engine.is_alive(h) {
            continue;
        }
        let Some(item) = handler_item(engine, h) else {
            continue;
        };
        let margin = real(engine, h, WheelHandler::MARGIN);
        if margin <= 0.0
            || items.contains(&item)
            || !item_on(engine, item)
            || crate::handlers::reaches(engine, item, pos, 0.0)
            || !crate::handlers::reaches(engine, item, pos, margin)
        {
            continue;
        }
        items.push(item);
    }
    items.sort_by(|a, b| {
        stack_key(engine, *b)
            .partial_cmp(&stack_key(engine, *a))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    items
}

/// Offers `ev` to the WheelHandlers of `item`, the last declared first.
/// Every one that the event fits gets it.
pub(crate) fn offer(
    engine: &Engine,
    item: ObjectId,
    ev: &WheelEvent,
    buttons: MouseButtons,
) -> Taken {
    let handlers: Vec<ObjectId> = resources(engine, item)
        .into_iter()
        .filter(|h| is_a::<WheelHandler>(engine, *h))
        .rev()
        .collect();
    if handlers.is_empty() || !item_on(engine, item) {
        return Taken::No;
    }
    let mut taken = Taken::No;
    for h in handlers {
        if !engine.is_alive(h) || !engine.is_alive(item) || !takes(engine, h, ev) {
            continue;
        }
        turned(engine, h, item, ev, buttons);
        let blocks =
            engine.is_alive(h) && engine.read(h, WheelHandler::BLOCKING.index).to_boolean();
        taken = match (taken, blocks) {
            (_, true) | (Taken::Kept, _) => Taken::Kept,
            _ => Taken::Shared,
        };
    }
    taken
}

/// How far `ev` turns the way the handler adds up, in eighths of a degree.
fn along(engine: &Engine, h: ObjectId, ev: &WheelEvent) -> f64 {
    let angle = wheel_angle(ev);
    match engine.read(h, WheelHandler::ORIENTATION.index).to_number() as i32 {
        HORIZONTAL => angle.x,
        _ => angle.y,
    }
}

/// Whether the handler takes the event: it is enabled, it takes a
/// mouse's wheel and the modifiers are its own, and the wheel turned its way at all. While it is active it
/// takes a turn the other way as well, as Qt's does.
fn takes(engine: &Engine, h: ObjectId, ev: &WheelEvent) -> bool {
    if !engine.read(h, WheelHandler::ENABLED.index).to_boolean() {
        return false;
    }
    let accepted = crate::handlers::Accepted {
        devices: WheelHandler::ACCEPTED_DEVICES.index,
        pointer_types: WheelHandler::ACCEPTED_POINTER_TYPES.index,
        modifiers: WheelHandler::ACCEPTED_MODIFIERS.index,
    };
    if !accepted.fit(engine, h, PointerSource::MOUSE, ev.modifiers) {
        return false;
    }
    along(engine, h, ev) != 0.0 || engine.read(h, WheelHandler::ACTIVE.index).to_boolean()
}

/// The handler takes `ev`. Qt's order: it is active, its rotation
/// changes, it signals, and then the target's property changes.
fn turned(engine: &Engine, h: ObjectId, item: ObjectId, ev: &WheelEvent, buttons: MouseButtons) {
    let Some(st) = state_of(engine, h) else {
        return;
    };
    let degrees = along(engine, h, ev) / 8.0;
    let _ = write_computed(engine, h, WheelHandler::ACTIVE.index, Value::Bool(true));
    rest_after_timeout(engine, h, &st);
    if degrees != 0.0 {
        st.turned.set(st.turned.get() + degrees);
        show_rotation(engine, h);
    }
    crate::wheel::emit_wheel(engine, h, item, ev, buttons);
    if degrees == 0.0 || !engine.is_alive(h) {
        return;
    }
    let scaled = degrees * real(engine, h, WheelHandler::ROTATION_SCALE);
    let name = engine
        .read(h, WheelHandler::PROPERTY.index)
        .to_js_string()
        .to_string();
    let target = engine.read(h, WheelHandler::TARGET.index).as_object();
    if let Some(target) = target.filter(|t| engine.is_alive(*t))
        && !name.is_empty()
    {
        turn_target(engine, h, target, &name, scaled, ev.pos);
    }
}

/// Adds `degrees` to the target's property. A `scale` is multiplied
/// instead, and it and a `rotation` keep the point under the pointer
/// where it is unless the handler says otherwise.
fn turn_target(
    engine: &Engine,
    h: ObjectId,
    target: ObjectId,
    name: &str,
    degrees: f64,
    scene_pos: kurbo::Point,
) {
    let Ok(p) = engine.property_index(target, name) else {
        engine.log(
            LogLevel::Warning,
            SCENE_CATEGORY,
            format!("WheelHandler: the target has no property \"{name}\""),
        );
        return;
    };
    let old = engine.untrack(|| engine.read(target, p)).to_number();
    let new = match name {
        "scale" => {
            let by = real(engine, h, WheelHandler::TARGET_SCALE_MULTIPLIER);
            old * by.powf(degrees / SCALE_STEP_DEGREES)
        }
        _ => old + degrees,
    };
    let around = matches!(name, "rotation" | "scale")
        && is_item(engine, target)
        && engine
            .read(h, WheelHandler::TARGET_TRANSFORM_AROUND_CURSOR.index)
            .to_boolean();
    // Where the point under the pointer is in the target, and where that
    // puts it in the target's parent.
    let held = around.then(|| {
        engine.untrack(|| {
            let local = scene_transform(engine, target).inverse() * scene_pos;
            (local, local_transform(engine, target) * local)
        })
    });
    let write = |p, v: f64| {
        if let Err(err) = engine.write(target, p, Value::Real(v), WriteKind::Imperative) {
            engine.log(
                LogLevel::Warning,
                SCENE_CATEGORY,
                format!("WheelHandler: cannot change \"{name}\": {err}"),
            );
        }
    };
    write(p, new);
    let Some((local, was)) = held else { return };
    if !engine.is_alive(target) {
        return;
    }
    let is = engine.untrack(|| local_transform(engine, target) * local);
    for (p, moved) in [(Item::X.index, was.x - is.x), (Item::Y.index, was.y - is.y)] {
        if moved != 0.0 {
            let at = engine.untrack(|| engine.read(target, p)).to_number();
            write(p, at + moved);
        }
    }
}

/// (Re)starts the timer that ends `active`. A timer and not a check each
/// frame: an idle handler costs nothing.
fn rest_after_timeout(engine: &Engine, h: ObjectId, st: &HandlerState) {
    let timer = match st.timer.get().filter(|t| engine.is_alive(*t)) {
        Some(t) => t,
        None => {
            let made = engine
                .create_native_incomplete(Timer::default(), Some(h))
                .and_then(|t| {
                    engine.complete(t)?;
                    engine.connect_by_name(t, "triggered", move |e, _| {
                        if e.is_alive(h) {
                            let _ = write_computed(
                                e,
                                h,
                                WheelHandler::ACTIVE.index,
                                Value::Bool(false),
                            );
                        }
                    })?;
                    Ok(t)
                });
            match made {
                Ok(t) => {
                    st.timer.set(Some(t));
                    t
                }
                Err(err) => {
                    engine.log(
                        LogLevel::Warning,
                        SCENE_CATEGORY,
                        format!("WheelHandler: timer: {err}"),
                    );
                    return;
                }
            }
        }
    };
    let seconds = real(engine, h, WheelHandler::ACTIVE_TIMEOUT);
    let ms = (seconds * 1000.0).round().clamp(1.0, f64::from(i32::MAX)) as i32;
    let _ = engine
        .set(timer, Timer::INTERVAL, ms)
        .and_then(|()| engine.invoke_by_name(timer, "restart", &[]).map(|_| ()));
}
