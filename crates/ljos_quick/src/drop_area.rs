//! `DropArea`, `DragEvent`, and what carries a drag from area to area:
//! an item's `Drag` (`crate::drag`) inside the window, or a drag that
//! comes from outside it, which the shell tells through [`PlatformDrag`].
//!
//! What an area hears, and in which order, was read on qml6
//! (`features/input/drag-drop.feature`): `entered` comes before
//! `containsDrag` turns true; an area that sets `drag.accepted = false` in
//! `entered` hears nothing more of that drag; a drop is told with
//! `dropped` and no `exited`.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use ljos_engine::{
    Atom, Cx, Engine, EngineError, ObjectId, QmlObject, QtObject, Url, Value, WriteKind, qml_impl,
};
use ljos_render::kurbo;

use crate::item::{Item, is_a, is_item};
use crate::scene::item_enabled;

/// `Qt.IgnoreAction`: nothing was done with the drop.
pub const IGNORE_ACTION: i32 = 0;
/// `Qt.CopyAction`.
pub const COPY_ACTION: i32 = 1;
/// `Qt.MoveAction`.
pub const MOVE_ACTION: i32 = 2;
/// `Qt.LinkAction`.
pub const LINK_ACTION: i32 = 4;

/// What a drag carries, by MIME type.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DragData {
    /// Files and other addresses (`text/uri-list`).
    pub urls: Vec<String>,
    /// `text/plain`.
    pub text: Option<String>,
    /// `text/html`.
    pub html: Option<String>,
    /// Anything else, as text, by MIME type.
    pub other: Vec<(String, String)>,
}

impl DragData {
    /// Files, as a file manager drags them.
    pub fn files<I: IntoIterator<Item = std::path::PathBuf>>(paths: I) -> Self {
        Self {
            urls: paths
                .into_iter()
                .map(|p| ljos_engine::file_url(&p))
                .collect(),
            ..Self::default()
        }
    }

    /// The MIME types it has, as `DragEvent.formats` and `keys` tell
    /// them.
    pub fn formats(&self) -> Vec<String> {
        let mut out = Vec::new();
        if !self.urls.is_empty() {
            out.push("text/uri-list".to_string());
        }
        if self.text.is_some() {
            out.push("text/plain".to_string());
        }
        if self.html.is_some() {
            out.push("text/html".to_string());
        }
        out.extend(self.other.iter().map(|(k, _)| k.clone()));
        out
    }

    fn get(&self, format: &str) -> Option<String> {
        match format {
            // The list leaves the process, so each address in it is written
            // the strict way a reader can parse without guessing.
            "text/uri-list" if !self.urls.is_empty() => Some(
                self.urls
                    .iter()
                    .map(|u| ljos_engine::encoded_url(u))
                    .collect::<Vec<_>>()
                    .join("\r\n"),
            ),
            "text/plain" => self.text.clone(),
            "text/html" => self.html.clone(),
            _ => self
                .other
                .iter()
                .find(|(k, _)| k == format)
                .map(|(_, v)| v.clone()),
        }
    }
}

/// The `drag` group of a `DropArea`: the drag over it.
#[derive(Debug, Default, QmlObject)]
#[qml(name = "DropAreaDrag", uncreatable)]
pub struct DropAreaDrag {
    /// Where the drag is, in the area.
    #[qml(notify, readonly)]
    pub x: f64,
    #[qml(notify, readonly)]
    pub y: f64,
    /// What is dragged; none from outside the window.
    #[qml(notify, readonly)]
    pub source: Option<ObjectId>,
}

/// `DropArea` (QtQuick).
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick", name = "DropArea", extends = Item)]
pub struct DropArea {
    #[qml(base)]
    pub item: Item,
    /// A drag the area took is over it.
    #[qml(notify, readonly)]
    pub contains_drag: bool,
    /// The area takes only drags with one of these keys; any when empty.
    #[qml(notify)]
    pub keys: Vec<Value>,
    #[qml(group)]
    pub drag: DropAreaDrag,
}

#[qml_impl]
impl DropArea {
    #[qml(signal)]
    fn entered(drag: Option<ObjectId>);
    #[qml(signal)]
    fn exited();
    #[qml(signal, name = "positionChanged")]
    fn position_changed(drag: Option<ObjectId>);
    #[qml(signal)]
    fn dropped(drop: Option<ObjectId>);
    #[qml(signal, name = "sourceChanged")]
    fn source_changed();

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let reg = cx.engine.extension::<Areas>();
        let mut areas = reg.0.borrow_mut();
        areas.retain(|a| cx.engine.is_alive(*a));
        areas.push(cx.this);
        Ok(())
    }
}

/// The live DropAreas. Per engine.
#[derive(Default)]
struct Areas(RefCell<Vec<ObjectId>>);

/// What `action` goes back to when it is reset.
fn proposed_action(engine: &Engine, ev: ObjectId, _: ljos_engine::PropIndex) -> Value {
    engine.read(ev, DragEvent::PROPOSED_ACTION.index)
}

/// The `drag` and `drop` parameters of a DropArea's signals
/// (`DragEvent` in Qt). Valid while the signal runs.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick", name = "DragEvent", extends = QtObject, uncreatable)]
pub struct DragEvent {
    #[qml(base)]
    pub object: QtObject,
    /// In the area.
    #[qml(readonly)]
    pub x: f64,
    #[qml(readonly)]
    pub y: f64,
    #[qml(readonly)]
    pub source: Option<ObjectId>,
    #[qml(readonly)]
    pub keys: Vec<Value>,
    #[qml(readonly, flags = "Qt.DropActions")]
    pub supported_actions: i32,
    #[qml(readonly, flags = "Qt.DropActions")]
    pub proposed_action: i32,
    /// What the area does with the drop; the proposed action unless it
    /// says otherwise.
    #[qml(
        enumeration = "Qt.DropAction",
        reset = ljos_engine::Reset::Follow(proposed_action)
    )]
    pub action: i32,
    pub accepted: bool,
    #[qml(readonly)]
    pub has_color: bool,
    #[qml(readonly)]
    pub has_html: bool,
    #[qml(readonly)]
    pub has_text: bool,
    #[qml(readonly)]
    pub has_urls: bool,
    #[qml(readonly)]
    pub color_data: Value,
    #[qml(readonly)]
    pub html: String,
    #[qml(readonly)]
    pub text: String,
    #[qml(readonly)]
    pub urls: Vec<Value>,
    #[qml(readonly)]
    pub formats: Vec<Value>,
    #[qml(skip)]
    data: Rc<DragData>,
}

impl Default for DragEvent {
    fn default() -> Self {
        Self {
            object: QtObject::default(),
            x: 0.0,
            y: 0.0,
            source: None,
            keys: Vec::new(),
            supported_actions: COPY_ACTION | MOVE_ACTION | LINK_ACTION,
            proposed_action: MOVE_ACTION,
            action: MOVE_ACTION,
            accepted: true,
            has_color: false,
            has_html: false,
            has_text: false,
            has_urls: false,
            color_data: Value::Undefined,
            html: String::new(),
            text: String::new(),
            urls: Vec::new(),
            formats: Vec::new(),
            data: Rc::default(),
        }
    }
}

#[qml_impl]
impl DragEvent {
    /// The data of MIME type `format`, as text.
    #[qml(invokable, name = "getDataAsString")]
    fn get_data_as_string(cx: &Cx, format: Value) -> String {
        let format = format.to_js_string().as_str().to_string();
        cx.engine
            .with(cx.this, |e: &DragEvent| e.data.get(&format))
            .ok()
            .flatten()
            .unwrap_or_default()
    }

    /// Deviation: the data as text, not an `ArrayBuffer`.
    #[qml(invokable, name = "getDataAsArrayBuffer")]
    fn get_data_as_array_buffer(cx: &Cx, format: Value) -> Value {
        let format = format.to_js_string().as_str().to_string();
        cx.engine
            .with(cx.this, |e: &DragEvent| e.data.get(&format))
            .ok()
            .flatten()
            .map_or(Value::Undefined, |s| Value::String(Atom::new(&s)))
    }

    #[qml(invokable, name = "acceptProposedAction")]
    fn accept_proposed_action(cx: &Cx) {
        let (e, me) = (cx.engine, cx.this);
        let proposed = e.read(me, DragEvent::PROPOSED_ACTION.index);
        let _ = e.write_internal(me, DragEvent::ACTION.index, proposed, WriteKind::Imperative);
        let _ = e.write_internal(
            me,
            DragEvent::ACCEPTED.index,
            Value::Bool(true),
            WriteKind::Imperative,
        );
    }

    /// `accept()` or `accept(action)`.
    #[qml(invokable)]
    fn accept(cx: &Cx, action: Value) {
        let (e, me) = (cx.engine, cx.this);
        if !matches!(action, Value::Undefined | Value::Null) {
            let a = Value::Int(action.to_number() as i32);
            let _ = e.write_internal(me, DragEvent::ACTION.index, a, WriteKind::Imperative);
        }
        let _ = e.write_internal(
            me,
            DragEvent::ACCEPTED.index,
            Value::Bool(true),
            WriteKind::Imperative,
        );
    }
}

/// A drag at one place: what an area is told of it.
#[derive(Clone, Debug)]
pub(crate) struct Offer {
    /// In the scene.
    pub pos: kurbo::Point,
    /// The top item of the window the drag is over: a scene position means
    /// nothing in another window's tree. `None` looks in every tree.
    pub window: Option<ObjectId>,
    pub source: Option<ObjectId>,
    pub keys: Vec<String>,
    pub supported: i32,
    pub proposed: i32,
    pub data: Rc<DragData>,
}

/// Which area a drag is over. One per drag.
#[derive(Debug, Default)]
pub(crate) struct Tracker {
    /// The area that took the drag in `entered`.
    over: Cell<Option<ObjectId>>,
    /// The area that turned it down: it hears no more of the drag while
    /// the drag stays over it.
    refused: Cell<Option<ObjectId>>,
}

impl Tracker {
    /// The area that took the drag, if any.
    pub(crate) fn target(&self) -> Option<ObjectId> {
        self.over.get()
    }
}

fn shown(engine: &Engine, item: ObjectId) -> bool {
    let mut cur = Some(item);
    for _ in 0..4096 {
        let Some(o) = cur.filter(|o| is_item(engine, *o)) else {
            return true;
        };
        if !engine
            .untrack(|| engine.read(o, Item::VISIBLE.index))
            .to_boolean()
        {
            return false;
        }
        cur = engine.untrack(|| engine.visual_parent(o));
    }
    true
}

fn strings(v: &Value) -> Vec<String> {
    match v {
        Value::List(l) => l
            .iter()
            .map(|v| v.to_js_string().as_str().to_string())
            .collect(),
        Value::Undefined | Value::Null => Vec::new(),
        other => vec![other.to_js_string().as_str().to_string()],
    }
}

fn takes_keys(engine: &Engine, area: ObjectId, keys: &[String]) -> bool {
    let wanted = strings(&engine.untrack(|| engine.read(area, DropArea::KEYS.index)));
    wanted.is_empty() || wanted.iter().any(|k| keys.contains(k))
}

/// Where `item` is in the paint order: compared, the greater is drawn
/// above.
fn stack_key(engine: &Engine, item: ObjectId) -> Vec<(f64, usize)> {
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
            .untrack(|| engine.read(cur, Item::Z.index))
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

/// The topmost area under `pos` that would take a drag with `keys`.
fn area_at(
    engine: &Engine,
    pos: kurbo::Point,
    window: Option<ObjectId>,
    keys: &[String],
) -> Option<ObjectId> {
    let reg = engine.extension::<Areas>();
    let areas: Vec<ObjectId> = {
        let mut a = reg.0.borrow_mut();
        a.retain(|o| engine.is_alive(*o));
        a.clone()
    };
    areas
        .into_iter()
        .filter(|&a| {
            is_a::<DropArea>(engine, a)
                && window
                    .is_none_or(|w| engine.untrack(|| crate::focus::tree::top_of(engine, a)) == w)
                && engine.untrack(|| item_enabled(engine, a))
                && shown(engine, a)
                && crate::handlers::reaches(engine, a, pos, 0.0)
                && takes_keys(engine, a, keys)
        })
        .map(|a| (stack_key(engine, a), a))
        .max_by(|(x, _), (y, _)| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(_, a)| a)
}

fn put(engine: &Engine, obj: ObjectId, p: ljos_engine::PropIndex, v: Value) {
    if !engine.untrack(|| engine.read(obj, p)).change_equals(&v) {
        let _ = engine.write_internal(obj, p, v, WriteKind::Imperative);
    }
}

fn group_index(engine: &Engine, area: ObjectId, name: &str) -> Option<ljos_engine::PropIndex> {
    engine.property_index(area, &format!("drag.{name}")).ok()
}

fn show_position(engine: &Engine, area: ObjectId, at: kurbo::Point) {
    engine.batch(|| {
        if let Some(p) = group_index(engine, area, "x") {
            put(engine, area, p, Value::Real(at.x));
        }
        if let Some(p) = group_index(engine, area, "y") {
            put(engine, area, p, Value::Real(at.y));
        }
    });
}

fn show_source(engine: &Engine, area: ObjectId, source: Option<ObjectId>) {
    if let Some(p) = group_index(engine, area, "source")
        && !engine
            .untrack(|| engine.read(area, p))
            .change_equals(&source.map_or(Value::Null, Value::Object))
    {
        put(engine, area, p, source.map_or(Value::Null, Value::Object));
        if engine.is_alive(area) {
            let _ = engine.emit_by_name(area, "sourceChanged", &[]);
        }
    }
}

/// Emits `signal` on `area` with a new DragEvent. What the handler left
/// in it: (accepted, action).
fn emit_event(engine: &Engine, area: ObjectId, signal: &str, offer: &Offer) -> (bool, i32) {
    let at = crate::handlers::local(engine, area, offer.pos);
    let data = offer.data.clone();
    let native = DragEvent {
        x: at.x,
        y: at.y,
        source: offer.source,
        keys: offer
            .keys
            .iter()
            .map(|k| Value::String(Atom::new(k)))
            .collect(),
        supported_actions: offer.supported,
        proposed_action: offer.proposed,
        action: offer.proposed,
        // A drop is turned down unless the area takes it.
        accepted: signal != "dropped",
        has_html: data.html.is_some(),
        has_text: data.text.is_some(),
        has_urls: !data.urls.is_empty(),
        html: data.html.clone().unwrap_or_default(),
        text: data.text.clone().unwrap_or_default(),
        urls: data
            .urls
            .iter()
            .map(|u| Url(Atom::new(u)))
            .map(|u| Value::Url(u.0))
            .collect(),
        formats: data
            .formats()
            .iter()
            .map(|f| Value::String(Atom::new(f)))
            .collect(),
        data,
        ..DragEvent::default()
    };
    let Ok(ev) = engine.create_native_incomplete(native, Some(area)) else {
        return (false, IGNORE_ACTION);
    };
    let _ = engine.emit_by_name(area, signal, &[Value::Object(ev)]);
    let accepted = engine.read(ev, DragEvent::ACCEPTED.index).to_boolean();
    let action = engine.read(ev, DragEvent::ACTION.index).to_number() as i32;
    engine.destroy_now(ev);
    (accepted, action)
}

/// The drag left the area it was over.
fn exit(engine: &Engine, area: ObjectId) {
    if !engine.is_alive(area) {
        return;
    }
    let _ = engine.emit_by_name(area, "exited", &[]);
    if engine.is_alive(area) {
        put(
            engine,
            area,
            DropArea::CONTAINS_DRAG.index,
            Value::Bool(false),
        );
        show_source(engine, area, None);
    }
}

/// The drag is at `offer.pos`: the areas it left and entered are told,
/// and the one it is over hears where it is.
pub(crate) fn track(engine: &Engine, t: &Tracker, offer: &Offer) {
    let now = area_at(engine, offer.pos, offer.window, &offer.keys);
    let over = t.over.get().filter(|a| engine.is_alive(*a));
    if let Some(area) = now.filter(|a| Some(*a) == over) {
        show_position(
            engine,
            area,
            crate::handlers::local(engine, area, offer.pos),
        );
        emit_event(engine, area, "positionChanged", offer);
        return;
    }
    if now.is_some() && now == t.refused.get() {
        return;
    }
    t.refused.set(None);
    if let Some(area) = over {
        t.over.set(None);
        exit(engine, area);
    }
    t.over.set(None);
    let Some(area) = now else { return };
    show_position(
        engine,
        area,
        crate::handlers::local(engine, area, offer.pos),
    );
    show_source(engine, area, offer.source);
    let (accepted, _) = emit_event(engine, area, "entered", offer);
    if !engine.is_alive(area) {
        return;
    }
    if accepted {
        t.over.set(Some(area));
        put(
            engine,
            area,
            DropArea::CONTAINS_DRAG.index,
            Value::Bool(true),
        );
    } else {
        t.refused.set(Some(area));
        show_source(engine, area, None);
    }
}

/// The drag ended without a drop: the area it was over hears `exited`.
pub(crate) fn leave(engine: &Engine, t: &Tracker) {
    t.refused.set(None);
    if let Some(area) = t.over.take() {
        exit(engine, area);
    }
}

/// The drag was let go at `offer.pos`. What the area that took it did with
/// it; `Qt.IgnoreAction` when none did.
pub(crate) fn drop(engine: &Engine, t: &Tracker, offer: &Offer) -> i32 {
    t.refused.set(None);
    let Some(area) = t.over.take().filter(|a| engine.is_alive(*a)) else {
        return IGNORE_ACTION;
    };
    let (accepted, action) = emit_event(engine, area, "dropped", offer);
    if engine.is_alive(area) {
        put(
            engine,
            area,
            DropArea::CONTAINS_DRAG.index,
            Value::Bool(false),
        );
        show_source(engine, area, None);
    }
    if accepted { action } else { IGNORE_ACTION }
}

// ------------------------------------------------------- platform drags

/// What happened to a drag that comes from outside the window, as the
/// shell tells it.
#[derive(Clone, Debug, PartialEq)]
pub enum PlatformDragKind {
    /// The drag is over the window at this scene position (it came in, or
    /// moved).
    Move(kurbo::Point),
    /// It left the window, or was given up.
    Leave,
    /// It was let go at this scene position.
    Drop(kurbo::Point),
}

/// A drag from outside the window: the shell's side of the seam. The
/// shell keeps one per drag and sends every event of the drag to
/// [`platform_drag`].
#[derive(Clone, Debug)]
pub struct PlatformDrag {
    pub data: Rc<DragData>,
    /// `Qt.DropActions` the dragging application allows.
    pub supported_actions: i32,
    /// The `Qt.DropAction` it would like.
    pub proposed_action: i32,
    /// The window it is over, as the top item of its tree: an engine with
    /// several windows has DropAreas at the same scene position in each.
    /// `None` looks in every window.
    pub window: Option<ObjectId>,
}

impl PlatformDrag {
    pub fn new(data: DragData) -> Self {
        Self {
            data: Rc::new(data),
            supported_actions: COPY_ACTION | MOVE_ACTION | LINK_ACTION,
            proposed_action: COPY_ACTION,
            window: None,
        }
    }

    /// The drag over the window that shows `item`'s tree.
    pub fn over(mut self, engine: &Engine, item: ObjectId) -> Self {
        self.window = Some(engine.untrack(|| crate::focus::tree::top_of(engine, item)));
        self
    }
}

/// The platform drag in progress. Per engine: a window has one pointer.
#[derive(Default)]
struct Platform {
    tracker: Tracker,
    last: Cell<Option<kurbo::Point>>,
}

/// Tells the window's DropAreas of a drag from outside it. For a drop,
/// returns the `Qt.DropAction` the area that took it chose
/// (`Qt.IgnoreAction` when none did); `Qt.IgnoreAction` otherwise.
///
/// Platform drags have no source item, and their keys are their MIME
/// types (`text/uri-list` for files), as in Qt.
pub fn platform_drag(engine: &Engine, drag: &PlatformDrag, kind: &PlatformDragKind) -> i32 {
    let p = engine.extension::<Platform>();
    let offer = |pos| Offer {
        pos,
        window: drag.window,
        source: None,
        keys: drag.data.formats(),
        supported: drag.supported_actions,
        proposed: drag.proposed_action,
        data: drag.data.clone(),
    };
    match kind {
        PlatformDragKind::Move(pos) => {
            p.last.set(Some(*pos));
            track(engine, &p.tracker, &offer(*pos));
            IGNORE_ACTION
        }
        PlatformDragKind::Leave => {
            p.last.set(None);
            leave(engine, &p.tracker);
            IGNORE_ACTION
        }
        PlatformDragKind::Drop(pos) => {
            // A drop where the drag was never seen still enters first; one
            // where it was last seen is not a move.
            if p.last.take() != Some(*pos) {
                track(engine, &p.tracker, &offer(*pos));
            }
            drop(engine, &p.tracker, &offer(*pos))
        }
    }
}

/// The DropArea that took the drag from outside the window when it came
/// over it, if one did: where a drop now would go.
pub fn platform_drag_target(engine: &Engine) -> Option<ObjectId> {
    engine
        .extension::<Platform>()
        .tracker
        .over
        .get()
        .filter(|a| engine.is_alive(*a))
}

/// Registers `DropArea` and `DragEvent` with module `QtQuick`, and
/// installs the drag-and-drop of the items as the platform's handler of
/// drags from other applications (`ljos_platform::drop`): a host turns its
/// window system's file events into `ExternalDrag`s, and they reach the
/// `DropArea`s as a [`platform_drag`], the same seam the egui view feeds
/// from its own file events. Each of the two paths is the only one for
/// its host; neither calls the other.
pub(crate) fn register(engine: &Engine) {
    engine.register_module(ljos_engine::module!("QtQuick" 2.15 => [DropArea, DragEvent]));

    // One drag at a time, as the egui view keeps one: the files it
    // carries are said once (when it comes in, or only when it is let
    // go), so the drag and where it was last seen are kept here for the
    // events that have neither.
    let current: RefCell<Option<PlatformDrag>> = RefCell::new(None);
    let last: Cell<Option<kurbo::Point>> = Cell::new(None);
    ljos_platform::Platform::of(engine).set_drop_handler(move |engine, drag| {
        use ljos_platform::ExternalDrag;
        let point = |at: (f64, f64)| kurbo::Point::new(at.0, at.1);
        match drag {
            ExternalDrag::Entered { files, at } => {
                // Cloned out: no borrow is held while the areas are told.
                let drag = current
                    .borrow_mut()
                    .get_or_insert_with(|| {
                        PlatformDrag::new(DragData::files(files.iter().cloned()))
                    })
                    .clone();
                // Where it is is not always known when it comes in; the
                // areas hear of it when a position is.
                if let Some(at) = at {
                    let pos = point(*at);
                    last.set(Some(pos));
                    platform_drag(engine, &drag, &PlatformDragKind::Move(pos));
                }
                false
            }
            ExternalDrag::Moved { at } => {
                let Some(drag) = current.borrow().clone() else {
                    return false; // A move of a drag that never came in.
                };
                let pos = point(*at);
                last.set(Some(pos));
                platform_drag(engine, &drag, &PlatformDragKind::Move(pos));
                false
            }
            ExternalDrag::Left => {
                last.set(None);
                if let Some(drag) = current.borrow_mut().take() {
                    platform_drag(engine, &drag, &PlatformDragKind::Leave);
                }
                false
            }
            ExternalDrag::Dropped { files, at } => {
                // Let go where it was last seen when the drop itself has
                // no position, as the egui view drops them.
                let drag = current
                    .borrow_mut()
                    .take()
                    .unwrap_or_else(|| PlatformDrag::new(DragData::files(files.iter().cloned())));
                let at = at.map(point).or_else(|| last.take());
                last.set(None);
                match at {
                    Some(pos) => {
                        platform_drag(engine, &drag, &PlatformDragKind::Drop(pos)) != IGNORE_ACTION
                    }
                    None => {
                        platform_drag(engine, &drag, &PlatformDragKind::Leave);
                        false
                    }
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::DragData;

    #[test]
    fn a_uri_list_is_written_strictly() {
        let d = DragData::files([
            std::path::PathBuf::from("/data/b c.png"),
            std::path::PathBuf::from("/data/ljós#1.txt"),
        ]);
        assert_eq!(
            d.get("text/uri-list").as_deref(),
            Some("file:///data/b%20c.png\r\nfile:///data/lj%C3%B3s%231.txt")
        );
    }
}
