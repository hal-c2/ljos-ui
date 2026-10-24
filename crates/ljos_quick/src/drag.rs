//! The `Drag` attached type: an item that is dragged over `DropArea`s
//! (`crate::drop_area`) inside the window.
//!
//! What the areas and the item hear, and in which order, was read on qml6
//! (`features/input/drag-drop.feature`):
//!
//! - Turning `active` on tells the area under the hot spot at once
//!   (`entered`, `containsDrag`, then `Drag.target`, then `active`).
//! - Moving the item is told later and only once for many moves, as in
//!   Qt: a 0 ms timer carries it. `drop()` tells the last move first.
//! - `drop()` gives the area `dropped` and no `exited`; turning `active`
//!   off or `cancel()` gives it `exited`.
//!
//! Deviation: `Drag.Automatic` and `startDrag()` do not hand the drag to
//! the platform, so it cannot leave the window. They say `dragStarted` and
//! `dragFinished` all the same, and the drag goes on inside the window.

use std::cell::Cell;
use std::rc::Rc;

use ljos_engine::{
    Cx, Engine, Interception, LogLevel, MetaRef, ObjectId, Point, QmlObject, QmlType, Reset, Size,
    Timer, Url, Value, WriteKind, qml_impl,
};
use ljos_render::kurbo;

use crate::drop_area::{self, DragData, Offer, Tracker};
use crate::handlers::EnumTable;
use crate::item::{Item, scene_transform};
use crate::scene::SCENE_CATEGORY;

/// `Drag.DragType`.
pub mod drag_type {
    /// The item's `active` does nothing by itself.
    pub const NONE: i32 = 0;
    /// `active` starts a drag the platform carries (here: one inside the
    /// window that says `dragStarted` and `dragFinished`).
    pub const AUTOMATIC: i32 = 1;
    /// `active` starts a drag inside the window.
    pub const INTERNAL: i32 = 2;
}

fn drag_enums() -> EnumTable {
    vec![
        (
            "DragType",
            vec![
                ("None", i64::from(drag_type::NONE)),
                ("Automatic", i64::from(drag_type::AUTOMATIC)),
                ("Internal", i64::from(drag_type::INTERNAL)),
            ],
        ),
        // `XandYAxis` is the spelling Qt kept from before.
        (
            "Axis",
            vec![
                ("XAxis", 1),
                ("YAxis", 2),
                ("XAndYAxis", 3),
                ("XandYAxis", 3),
            ],
        ),
    ]
}

/// `Drag` (QtQuick): attached to an item, and the `drag` group of a
/// `MouseArea`, which moves its `target` as the pointer drags the area
/// (`crate::mouse_drag`).
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick",
    name = "Drag",
    attached = DragAttached,
    uncreatable,
    enums = drag_enums
)]
pub struct Drag {
    /// The item the area moves; none moves nothing and leaves a moving
    /// press a click.
    #[qml(notify, reset)]
    pub target: Option<ObjectId>,
    /// A `Drag.Axis`: which coordinates of the target follow.
    #[qml(notify, enumeration = "Axis")]
    pub axis: i32,
    #[qml(notify)]
    pub minimum_x: f64,
    #[qml(notify)]
    pub maximum_x: f64,
    #[qml(notify)]
    pub minimum_y: f64,
    #[qml(notify)]
    pub maximum_y: f64,
    /// The target follows the pointer.
    #[qml(notify, readonly)]
    pub active: bool,
    /// The area drags from a press that an area or control inside it took.
    #[qml(notify)]
    pub filter_children: bool,
    /// The target starts from where it is when the drag starts, instead of
    /// jumping by the distance the pointer went to start it.
    #[qml(notify)]
    pub smoothed: bool,
    /// How far the pointer goes before a press is a drag; the system's
    /// `startDragDistance` unless set.
    #[qml(notify, reset)]
    pub threshold: f64,
}

impl Default for Drag {
    fn default() -> Self {
        // Qt's limits are the largest single-precision float, as qml6 reads
        // them.
        let far = f64::from(f32::MAX);
        Self {
            target: None,
            axis: 3,
            minimum_x: -far,
            maximum_x: far,
            minimum_y: -far,
            maximum_y: far,
            active: false,
            filter_children: false,
            smoothed: true,
            threshold: crate::scene::DRAG_THRESHOLD,
        }
    }
}

/// What `Drag.` attaches to an item.
#[derive(Debug, QmlObject)]
#[qml(name = "DragAttached", enums = drag_enums)]
pub struct DragAttached {
    /// The item is being dragged.
    #[qml(notify)]
    pub active: bool,
    /// What an area is told is dragged; the item itself unless set.
    #[qml(notify, reset = Reset::Follow(attachee))]
    pub source: Option<ObjectId>,
    /// The area that took the drag.
    #[qml(notify, readonly)]
    pub target: Option<ObjectId>,
    /// The point of the item that is over an area, in the item.
    #[qml(notify)]
    pub hot_spot: Point,
    #[qml(notify)]
    pub image_source: Url,
    #[qml(notify)]
    pub image_source_size: Size,
    /// An area with keys takes the drag only when one of them is here.
    #[qml(notify)]
    pub keys: Vec<Value>,
    /// What the drag carries, by MIME type.
    #[qml(notify)]
    pub mime_data: Value,
    #[qml(notify, flags = "Qt.DropActions")]
    pub supported_actions: i32,
    #[qml(notify, enumeration = "Qt.DropAction")]
    pub proposed_action: i32,
    #[qml(notify, enumeration = "DragType")]
    pub drag_type: i32,
    #[qml(skip)]
    state: Rc<DragState>,
}

impl Default for DragAttached {
    fn default() -> Self {
        Self {
            active: false,
            source: None,
            target: None,
            hot_spot: Point::default(),
            image_source: Url::default(),
            image_source_size: Size::default(),
            keys: Vec::new(),
            mime_data: Value::Undefined,
            supported_actions: drop_area::COPY_ACTION
                | drop_area::MOVE_ACTION
                | drop_area::LINK_ACTION,
            proposed_action: drop_area::MOVE_ACTION,
            drag_type: drag_type::INTERNAL,
            state: Rc::default(),
        }
    }
}

/// A drag in progress, per attached object.
#[derive(Debug, Default)]
struct DragState {
    tracker: Tracker,
    active: Cell<bool>,
    /// The item moved since the areas were last told.
    pending: Cell<bool>,
    /// Carries moves to the next turn of the event loop.
    timer: Cell<Option<ObjectId>>,
    /// The item's position is watched (from the first drag on).
    watching: Cell<bool>,
    /// Started as a platform drag: ends with `dragFinished`.
    automatic: Cell<bool>,
}

#[qml_impl]
impl DragAttached {
    #[qml(signal, name = "dragStarted")]
    fn drag_started();
    #[qml(signal, name = "dragFinished")]
    fn drag_finished(drop_action: i32);

    /// Starts a drag inside the window, with `supportedActions` when
    /// given.
    #[qml(invokable)]
    fn start(cx: &Cx, supported_actions: Value) {
        set_supported(cx.engine, cx.this, &supported_actions);
        begin(cx.engine, cx.this, false, WriteKind::Imperative);
    }

    /// Starts a drag that says `dragStarted` and, when it ends,
    /// `dragFinished`.
    #[qml(invokable, name = "startDrag")]
    fn start_drag(cx: &Cx, supported_actions: Value) {
        set_supported(cx.engine, cx.this, &supported_actions);
        begin(cx.engine, cx.this, true, WriteKind::Imperative);
    }

    /// Ends the drag without a drop.
    #[qml(invokable)]
    fn cancel(cx: &Cx) {
        end(cx.engine, cx.this, false, WriteKind::Imperative);
    }

    /// Drops on the area the drag is over. What that area did:
    /// `Qt.IgnoreAction` when it did not take the drop.
    #[qml(invokable)]
    fn drop(cx: &Cx) -> i32 {
        end(cx.engine, cx.this, true, WriteKind::Imperative)
    }
}

fn attacher() -> MetaRef {
    MetaRef::Static(Drag::static_meta())
}

/// The item an attached object belongs to: its owner.
fn attachee(engine: &Engine, att: ObjectId, _p: ljos_engine::PropIndex) -> Value {
    engine.owner(att).map_or(Value::Null, Value::Object)
}

fn state(engine: &Engine, att: ObjectId) -> Option<Rc<DragState>> {
    engine.with(att, |a: &DragAttached| a.state.clone()).ok()
}

fn set_supported(engine: &Engine, att: ObjectId, v: &Value) {
    if matches!(v, Value::Undefined | Value::Null) {
        return;
    }
    let _ = engine.write_internal(
        att,
        DragAttached::SUPPORTED_ACTIONS.index,
        Value::Int(v.to_number() as i32),
        WriteKind::Imperative,
    );
}

fn read(engine: &Engine, att: ObjectId, p: ljos_engine::PropIndex) -> Value {
    engine.untrack(|| engine.read(att, p))
}

/// Where the drag is, from the item's position and hot spot, and what it
/// offers.
fn offer(engine: &Engine, att: ObjectId) -> Option<Offer> {
    let item = engine.owner(att).filter(|o| engine.is_alive(*o))?;
    let hot = match read(engine, att, DragAttached::HOT_SPOT.index) {
        Value::Point(p) => kurbo::Point::new(p.x, p.y),
        _ => kurbo::Point::ZERO,
    };
    let pos = engine.untrack(|| scene_transform(engine, item)) * hot;
    let keys = match read(engine, att, DragAttached::KEYS.index) {
        Value::List(l) => l
            .iter()
            .map(|v| v.to_js_string().as_str().to_string())
            .collect(),
        Value::Undefined | Value::Null => Vec::new(),
        other => vec![other.to_js_string().as_str().to_string()],
    };
    let mime = read(engine, att, DragAttached::MIME_DATA.index);
    Some(Offer {
        pos,
        // An item is dragged inside its own window.
        window: Some(engine.untrack(|| crate::focus::tree::top_of(engine, item))),
        source: read(engine, att, DragAttached::SOURCE.index).as_object(),
        keys,
        supported: read(engine, att, DragAttached::SUPPORTED_ACTIONS.index).to_number() as i32,
        proposed: read(engine, att, DragAttached::PROPOSED_ACTION.index).to_number() as i32,
        data: Rc::new(data_of(engine, &mime)),
    })
}

/// `Drag.mimeData` as what a drop can read.
fn data_of(engine: &Engine, mime: &Value) -> DragData {
    let mut data = DragData::default();
    if matches!(mime, Value::Undefined | Value::Null) {
        return data;
    }
    for (format, v) in engine.js_object_entries(mime) {
        let text = |v: &Value| v.to_js_string().as_str().to_string();
        match format.as_str() {
            "text/plain" => data.text = Some(text(&v)),
            "text/html" => data.html = Some(text(&v)),
            "text/uri-list" => {
                data.urls = match &v {
                    Value::List(l) => l.iter().map(text).collect(),
                    other => text(other)
                        .lines()
                        .map(str::trim)
                        .filter(|l| !l.is_empty() && !l.starts_with('#'))
                        .map(str::to_string)
                        .collect(),
                }
            }
            _ => data.other.push((format, text(&v))),
        }
    }
    data
}

fn show_target(engine: &Engine, att: ObjectId, st: &DragState) {
    let v = st.tracker.target().map_or(Value::Null, Value::Object);
    if !read(engine, att, DragAttached::TARGET.index).change_equals(&v) {
        let _ = engine.write_internal(att, DragAttached::TARGET.index, v, WriteKind::Imperative);
    }
}

fn show_active(engine: &Engine, att: ObjectId, on: bool, how: WriteKind) {
    if engine.peek(att, DragAttached::ACTIVE.index).to_boolean() != on {
        let _ = engine.write_internal(att, DragAttached::ACTIVE.index, Value::Bool(on), how);
    }
}

/// The areas hear where the item is now.
fn flush(engine: &Engine, att: ObjectId, st: &DragState) {
    st.pending.set(false);
    if !st.active.get() {
        return;
    }
    if let Some(o) = offer(engine, att) {
        drop_area::track(engine, &st.tracker, &o);
    }
    show_target(engine, att, st);
}

fn begin(engine: &Engine, att: ObjectId, automatic: bool, how: WriteKind) {
    let Some(st) = state(engine, att) else { return };
    if st.active.get() {
        return;
    }
    st.active.set(true);
    st.automatic.set(automatic);
    watch_moves(engine, att, &st);
    if automatic {
        let _ = engine.emit_by_name(att, "dragStarted", &[]);
    }
    flush(engine, att, &st);
    if st.active.get() {
        show_active(engine, att, true, how);
    }
}

/// Ends the drag: a drop when `dropping`. What the area did with a drop.
fn end(engine: &Engine, att: ObjectId, dropping: bool, how: WriteKind) -> i32 {
    let Some(st) = state(engine, att) else {
        return drop_area::IGNORE_ACTION;
    };
    if !st.active.get() {
        return drop_area::IGNORE_ACTION;
    }
    let action = if dropping {
        // Qt tells the last move before the drop.
        if st.pending.get() {
            flush(engine, att, &st);
        }
        match offer(engine, att) {
            Some(o) => drop_area::drop(engine, &st.tracker, &o),
            None => {
                drop_area::leave(engine, &st.tracker);
                drop_area::IGNORE_ACTION
            }
        }
    } else {
        drop_area::leave(engine, &st.tracker);
        drop_area::IGNORE_ACTION
    };
    st.active.set(false);
    st.pending.set(false);
    show_target(engine, att, &st);
    show_active(engine, att, false, how);
    if st.automatic.replace(false) {
        let _ = engine.emit_by_name(att, "dragFinished", &[Value::Int(action)]);
    }
    action
}

/// Moves of the item are told on the next turn, all at once.
fn watch_moves(engine: &Engine, att: ObjectId, st: &Rc<DragState>) {
    if st.watching.replace(true) {
        return;
    }
    let Some(item) = engine.owner(att) else {
        return;
    };
    let moved = move |e: &Engine, _: &Value| {
        if let Some(st) = state(e, att)
            && st.active.get()
            && !st.pending.replace(true)
        {
            schedule(e, att, &st);
        }
    };
    for p in [Item::X.index, Item::Y.index] {
        let _ = engine.watch(item, p, moved);
    }
    let _ = engine.watch(att, DragAttached::HOT_SPOT.index, moved);
}

fn schedule(engine: &Engine, att: ObjectId, st: &DragState) {
    let timer = match st.timer.get().filter(|t| engine.is_alive(*t)) {
        Some(t) => t,
        None => {
            let made = engine
                .create_native_incomplete(Timer::default(), Some(att))
                .and_then(|t| {
                    engine.complete(t)?;
                    engine.set(t, Timer::INTERVAL, 0)?;
                    engine.connect_by_name(t, "triggered", move |e, _| {
                        if let Some(st) = state(e, att)
                            && st.pending.get()
                        {
                            flush(e, att, &st);
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
                        format!("Drag: timer: {err}"),
                    );
                    return;
                }
            }
        }
    };
    let _ = engine.invoke_by_name(timer, "start", &[]);
}

/// From the first `Drag.` on an item on, its `active` starts and ends
/// drags, whether a script or a binding sets it.
pub(crate) fn register(engine: &Engine) {
    engine.register_module(ljos_engine::module!("QtQuick" 2.15 => [Drag]));
    engine.on_attached_created_of::<DragAttached>(|e, owner, att| {
        let _ = e.write_internal(
            att,
            DragAttached::SOURCE.index,
            Value::Object(owner),
            WriteKind::Initial,
        );
        e.set_interceptor(
            att,
            DragAttached::ACTIVE.index,
            Some(Rc::new(move |e, v, how| {
                let on = v.to_boolean();
                match how {
                    Interception::Write => {
                        toggle(e, att, on, WriteKind::Imperative);
                    }
                    // The property shows the drag once the areas were told,
                    // and keeps its binding.
                    Interception::Binding => {
                        e.after_binding(move |e| toggle(e, att, on, WriteKind::Host));
                    }
                }
                true
            })),
        );
        true
    });
}

fn toggle(engine: &Engine, att: ObjectId, on: bool, how: WriteKind) {
    let kind = read(engine, att, DragAttached::DRAG_TYPE.index).to_number() as i32;
    if on {
        if kind == drag_type::NONE {
            // Qt keeps the value and does nothing with it.
            show_active(engine, att, true, how);
            return;
        }
        begin(engine, att, kind == drag_type::AUTOMATIC, how);
    } else if state(engine, att).is_some_and(|st| st.active.get()) {
        end(engine, att, false, how);
    } else {
        show_active(engine, att, false, how);
    }
}

/// The item's `Drag` attached object, if it has one.
pub fn drag_attached(engine: &Engine, item: ObjectId) -> Option<ObjectId> {
    engine.existing_attached_object(item, &attacher())
}
