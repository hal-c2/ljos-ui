//! `Item`, the base of every visual type, its `anchors` group and anchor
//! lines.

use std::sync::OnceLock;

use ljos_engine::{
    Atom, Cx, Engine, EngineError, EnumValue, ObjectId, Point, PropIndex, QmlObject, QmlType,
    QtObject, Rect, Value, WriteKind, qml_impl,
};
use ljos_render::kurbo::{self, Affine};

use crate::enums::transform_origin;

/// `Item` (QtQuick).
///
/// `visible` and `enabled` read as the *effective* values once the item is
/// in a [`crate::QuickScene`]: a hidden ancestor makes the item read
/// `visible: false`, as Qt documents. The scene does that with an override
/// layer, so the value the item was given comes back when the ancestor is
/// shown again.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick", name = "Item", extends = QtObject, default = "data")]
pub struct Item {
    #[qml(base)]
    pub object: QtObject,
    #[qml(notify)]
    pub x: f64,
    #[qml(notify)]
    pub y: f64,
    #[qml(notify)]
    pub z: f64,
    #[qml(notify)]
    pub width: f64,
    #[qml(notify)]
    pub height: f64,
    #[qml(notify)]
    pub implicit_width: f64,
    #[qml(notify)]
    pub implicit_height: f64,
    #[qml(notify)]
    pub visible: bool,
    #[qml(notify)]
    pub opacity: f64,
    #[qml(notify)]
    pub clip: bool,
    #[qml(notify)]
    pub enabled: bool,
    #[qml(notify)]
    pub rotation: f64,
    #[qml(notify)]
    pub scale: f64,
    /// One of [`crate::enums::transform_origin`].
    #[qml(notify)]
    pub transform_origin: i32,
    #[qml(notify)]
    pub parent: Option<ObjectId>,
    /// Children and resources together, in declaration order.
    ///
    /// SHIM: a `var` list until the engine has `#[qml(list)]` (contract
    /// item 1). Use [`crate::add_data`] / [`crate::set_parent_item`].
    #[qml(notify, readonly)]
    pub data: Vec<Value>,
    #[qml(notify, readonly)]
    pub children: Vec<Value>,
    #[qml(notify, readonly)]
    pub resources: Vec<Value>,
    #[qml(notify, readonly)]
    pub children_rect: Rect,
    #[qml(notify)]
    pub baseline_offset: f64,
    #[qml(group)]
    pub anchors: Anchors,
    // Anchor lines (`parent.left`). Set in `on_complete`, when the object
    // knows its own id.
    #[qml(notify, readonly)]
    pub left: Value,
    #[qml(notify, readonly)]
    pub right: Value,
    #[qml(notify, readonly)]
    pub top: Value,
    #[qml(notify, readonly)]
    pub bottom: Value,
    #[qml(notify, readonly)]
    pub horizontal_center: Value,
    #[qml(notify, readonly)]
    pub vertical_center: Value,
    #[qml(rename = "baseline", notify, readonly)]
    pub baseline_line: Value,
    /// Static accessibility data for a later a11y pass (ARCHITECTURE §14).
    #[qml(skip)]
    pub accessible: AccessibleInfo,
}

impl Default for Item {
    fn default() -> Self {
        Self {
            object: QtObject::default(),
            x: 0.0,
            y: 0.0,
            z: 0.0,
            width: 0.0,
            height: 0.0,
            implicit_width: 0.0,
            implicit_height: 0.0,
            visible: true,
            opacity: 1.0,
            clip: false,
            enabled: true,
            rotation: 0.0,
            scale: 1.0,
            transform_origin: transform_origin::CENTER,
            parent: None,
            data: Vec::new(),
            children: Vec::new(),
            resources: Vec::new(),
            children_rect: Rect::default(),
            baseline_offset: 0.0,
            anchors: Anchors::default(),
            left: Value::Undefined,
            right: Value::Undefined,
            top: Value::Undefined,
            bottom: Value::Undefined,
            horizontal_center: Value::Undefined,
            vertical_center: Value::Undefined,
            baseline_line: Value::Undefined,
            accessible: AccessibleInfo::default(),
        }
    }
}

/// Where an assistive-technology pass will find an item's static role,
/// name and description. Nothing reads it yet.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AccessibleInfo {
    /// `Accessible.role` as its documented name (`"Button"`), if set.
    pub role: Option<Atom>,
    pub name: Option<Atom>,
    pub description: Option<Atom>,
}

/// The `anchors` group of [`Item`].
#[derive(Debug, QmlObject)]
#[qml(name = "Anchors", uncreatable)]
pub struct Anchors {
    #[qml(notify)]
    pub left: Value,
    #[qml(notify)]
    pub right: Value,
    #[qml(notify)]
    pub top: Value,
    #[qml(notify)]
    pub bottom: Value,
    #[qml(notify)]
    pub horizontal_center: Value,
    #[qml(notify)]
    pub vertical_center: Value,
    #[qml(notify)]
    pub baseline: Value,
    #[qml(notify)]
    pub fill: Option<ObjectId>,
    #[qml(notify)]
    pub center_in: Option<ObjectId>,
    #[qml(notify)]
    pub margins: f64,
    /// Falls back to `margins` until set (see [`effective_margin`]).
    #[qml(notify)]
    pub left_margin: f64,
    #[qml(notify)]
    pub right_margin: f64,
    #[qml(notify)]
    pub top_margin: f64,
    #[qml(notify)]
    pub bottom_margin: f64,
    #[qml(notify)]
    pub horizontal_center_offset: f64,
    #[qml(notify)]
    pub vertical_center_offset: f64,
    #[qml(notify)]
    pub baseline_offset: f64,
    #[qml(notify)]
    pub align_when_centered: bool,
}

impl Default for Anchors {
    fn default() -> Self {
        Self {
            left: Value::Undefined,
            right: Value::Undefined,
            top: Value::Undefined,
            bottom: Value::Undefined,
            horizontal_center: Value::Undefined,
            vertical_center: Value::Undefined,
            baseline: Value::Undefined,
            fill: None,
            center_in: None,
            margins: 0.0,
            left_margin: 0.0,
            right_margin: 0.0,
            top_margin: 0.0,
            bottom_margin: 0.0,
            horizontal_center_offset: 0.0,
            vertical_center_offset: 0.0,
            baseline_offset: 0.0,
            align_when_centered: true,
        }
    }
}

/// Flattened indices of the `anchors.*` leaves on `Item` (and therefore on
/// every type derived from it).
#[derive(Debug)]
pub struct AnchorProps {
    pub left: PropIndex,
    pub right: PropIndex,
    pub top: PropIndex,
    pub bottom: PropIndex,
    pub horizontal_center: PropIndex,
    pub vertical_center: PropIndex,
    pub baseline: PropIndex,
    pub fill: PropIndex,
    pub center_in: PropIndex,
    pub margins: PropIndex,
    pub left_margin: PropIndex,
    pub right_margin: PropIndex,
    pub top_margin: PropIndex,
    pub bottom_margin: PropIndex,
    pub horizontal_center_offset: PropIndex,
    pub vertical_center_offset: PropIndex,
    pub baseline_offset: PropIndex,
    pub align_when_centered: PropIndex,
}

impl AnchorProps {
    pub fn get() -> &'static AnchorProps {
        static PROPS: OnceLock<AnchorProps> = OnceLock::new();
        PROPS.get_or_init(|| {
            let m = Item::static_meta();
            let p = |name: &str| {
                m.property_index(&format!("anchors.{name}"))
                    .expect("Item declares the anchors group")
            };
            AnchorProps {
                left: p("left"),
                right: p("right"),
                top: p("top"),
                bottom: p("bottom"),
                horizontal_center: p("horizontalCenter"),
                vertical_center: p("verticalCenter"),
                baseline: p("baseline"),
                fill: p("fill"),
                center_in: p("centerIn"),
                margins: p("margins"),
                left_margin: p("leftMargin"),
                right_margin: p("rightMargin"),
                top_margin: p("topMargin"),
                bottom_margin: p("bottomMargin"),
                horizontal_center_offset: p("horizontalCenterOffset"),
                vertical_center_offset: p("verticalCenterOffset"),
                baseline_offset: p("baselineOffset"),
                align_when_centered: p("alignWhenCentered"),
            }
        })
    }
}

/// A per-side margin: its own value once set, otherwise `anchors.margins`,
/// as Qt documents.
///
/// Deviation: reading `anchors.leftMargin` from QML before it is set gives
/// 0, not the `margins` value; only the geometry uses the fallback.
pub fn effective_margin(engine: &Engine, obj: ObjectId, side: PropIndex) -> f64 {
    let own = engine.read(obj, side).to_number();
    let margins = engine.read(obj, AnchorProps::get().margins).to_number();
    match engine.provenance(obj, side) {
        Some(ljos_engine::Provenance::Default) | None => margins,
        _ => own,
    }
}

/// An anchor edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Edge {
    Left,
    Right,
    Top,
    Bottom,
    HorizontalCenter,
    VerticalCenter,
    Baseline,
}

impl Edge {
    fn code(self) -> i32 {
        match self {
            Edge::Left => 0x01,
            Edge::Right => 0x02,
            Edge::Top => 0x04,
            Edge::Bottom => 0x08,
            Edge::HorizontalCenter => 0x10,
            Edge::VerticalCenter => 0x20,
            Edge::Baseline => 0x40,
        }
    }

    fn from_code(c: i32) -> Option<Edge> {
        Some(match c {
            0x01 => Edge::Left,
            0x02 => Edge::Right,
            0x04 => Edge::Top,
            0x08 => Edge::Bottom,
            0x10 => Edge::HorizontalCenter,
            0x20 => Edge::VerticalCenter,
            0x40 => Edge::Baseline,
            _ => return None,
        })
    }

    pub fn is_horizontal(self) -> bool {
        matches!(self, Edge::Left | Edge::Right | Edge::HorizontalCenter)
    }
}

/// Tag of the enum half of an encoded anchor line ("AL").
const ANCHOR_LINE_TY: u32 = 0x414c;

/// An anchor line: `parent.left`, `sibling.verticalCenter`.
///
/// SHIM: the engine has no `AnchorLine` value type, so a line is encoded
/// as the list `[item, enum]`. Only this crate creates or reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnchorLine {
    pub item: ObjectId,
    pub edge: Edge,
}

impl AnchorLine {
    pub fn to_value(self) -> Value {
        Value::list(vec![
            Value::Object(self.item),
            Value::Enum(EnumValue {
                ty: ANCHOR_LINE_TY,
                value: self.edge.code(),
            }),
        ])
    }

    pub fn from_value(v: &Value) -> Option<AnchorLine> {
        let Value::List(l) = v else { return None };
        match l.as_slice() {
            [Value::Object(item), Value::Enum(e)] if e.ty == ANCHOR_LINE_TY => Some(AnchorLine {
                item: *item,
                edge: Edge::from_code(e.value)?,
            }),
            _ => None,
        }
    }
}

#[qml_impl]
impl Item {
    /// Fills in what needs the object's own id: anchor lines, the lazy
    /// `childrenRect`, and the implicit-size fallback for `width`/`height`.
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        init_item(cx.engine, cx.this)
    }

    /// `mapToItem(item, x, y)`: a point in this item's coordinates, in
    /// `item`'s (the scene's when `item` is null).
    #[qml(invokable, name = "mapToItem")]
    fn map_to_item(cx: &Cx, item: Option<ObjectId>, x: f64, y: f64) -> Point {
        map_point(cx.engine, Some(cx.this), item, Point { x, y })
    }

    /// `mapFromItem(item, x, y)`: a point in `item`'s coordinates (the
    /// scene's when null), in this item's.
    #[qml(invokable, name = "mapFromItem")]
    fn map_from_item(cx: &Cx, item: Option<ObjectId>, x: f64, y: f64) -> Point {
        map_point(cx.engine, item, Some(cx.this), Point { x, y })
    }

    /// `contains(point)`: whether a point in local coordinates is inside
    /// the item's bounding rectangle.
    #[qml(invokable)]
    fn contains(&self, point: Point) -> bool {
        point.x >= 0.0 && point.y >= 0.0 && point.x < self.width && point.y < self.height
    }
}

/// What `on_complete` does, for trees built natively before completion or
/// when the hook did not run. Idempotent.
pub fn init_item(engine: &Engine, obj: ObjectId) -> Result<(), EngineError> {
    let line = |edge| AnchorLine { item: obj, edge }.to_value();
    let already = engine.untrack(|| engine.with(obj, |i: &Item| !i.left.is_nullish()))?;
    if already {
        return Ok(());
    }
    engine.batch(|| -> Result<(), EngineError> {
        engine.set_internal(obj, Item::LEFT, line(Edge::Left))?;
        engine.set_internal(obj, Item::RIGHT, line(Edge::Right))?;
        engine.set_internal(obj, Item::TOP, line(Edge::Top))?;
        engine.set_internal(obj, Item::BOTTOM, line(Edge::Bottom))?;
        engine.set_internal(obj, Item::HORIZONTAL_CENTER, line(Edge::HorizontalCenter))?;
        engine.set_internal(obj, Item::VERTICAL_CENTER, line(Edge::VerticalCenter))?;
        engine.set_internal(obj, Item::BASELINE_LINE, line(Edge::Baseline))?;
        // An item whose size was never given follows its implicit size.
        // Assigning `width` replaces this binding, as in Qt.
        let unset = |p| {
            matches!(
                engine.provenance(obj, p),
                Some(ljos_engine::Provenance::Default) | None
            )
        };
        if unset(Item::WIDTH.index) {
            engine.bind(obj, Item::WIDTH.index, move |e| {
                e.read(obj, Item::IMPLICIT_WIDTH.index)
            })?;
        }
        if unset(Item::HEIGHT.index) {
            engine.bind(obj, Item::HEIGHT.index, move |e| {
                e.read(obj, Item::IMPLICIT_HEIGHT.index)
            })?;
        }
        // Lazy: costs nothing until something reads `childrenRect`.
        engine.bind_initial(obj, Item::CHILDREN_RECT.index, move |e| {
            Value::Rect(std::sync::Arc::new(children_rect(e, obj)))
        })?;
        Ok(())
    })
}

/// The union of the children's geometry, in this item's coordinates.
fn children_rect(engine: &Engine, obj: ObjectId) -> Rect {
    let children = engine.read(obj, Item::CHILDREN.index);
    let Value::List(children) = children else {
        return Rect::default();
    };
    let mut acc: Option<(f64, f64, f64, f64)> = None;
    for c in children.iter().filter_map(Value::as_object) {
        let x = engine.read(c, Item::X.index).to_number();
        let y = engine.read(c, Item::Y.index).to_number();
        let w = engine.read(c, Item::WIDTH.index).to_number();
        let h = engine.read(c, Item::HEIGHT.index).to_number();
        acc = Some(match acc {
            None => (x, y, x + w, y + h),
            Some((x0, y0, x1, y1)) => (x0.min(x), y0.min(y), x1.max(x + w), y1.max(y + h)),
        });
    }
    acc.map_or_else(Rect::default, |(x0, y0, x1, y1)| Rect {
        x: x0,
        y: y0,
        width: x1 - x0,
        height: y1 - y0,
    })
}

/// Reads a number property of an item.
pub(crate) fn num(engine: &Engine, obj: ObjectId, p: PropIndex) -> f64 {
    engine.read(obj, p).to_number()
}

/// The item's transform to its parent: `x`/`y`, then `rotation` and
/// `scale` about `transformOrigin`.
pub fn local_transform(engine: &Engine, obj: ObjectId) -> Affine {
    let x = num(engine, obj, Item::X.index);
    let y = num(engine, obj, Item::Y.index);
    let w = num(engine, obj, Item::WIDTH.index);
    let h = num(engine, obj, Item::HEIGHT.index);
    let rotation = num(engine, obj, Item::ROTATION.index);
    let scale = num(engine, obj, Item::SCALE.index);
    let origin = engine.read(obj, Item::TRANSFORM_ORIGIN.index).to_number() as i32;
    compose_transform(x, y, w, h, rotation, scale, origin)
}

pub(crate) fn compose_transform(
    x: f64,
    y: f64,
    w: f64,
    h: f64,
    rotation: f64,
    scale: f64,
    origin: i32,
) -> Affine {
    let t = Affine::translate((x, y));
    if rotation == 0.0 && scale == 1.0 {
        return t;
    }
    let (ox, oy) = origin_point(origin, w, h);
    t * Affine::translate((ox, oy))
        * Affine::rotate(rotation.to_radians())
        * Affine::scale(scale)
        * Affine::translate((-ox, -oy))
}

fn origin_point(origin: i32, w: f64, h: f64) -> (f64, f64) {
    use transform_origin as o;
    let fx = match origin {
        o::TOP_LEFT | o::LEFT | o::BOTTOM_LEFT => 0.0,
        o::TOP_RIGHT | o::RIGHT | o::BOTTOM_RIGHT => 1.0,
        _ => 0.5,
    };
    let fy = match origin {
        o::TOP_LEFT | o::TOP | o::TOP_RIGHT => 0.0,
        o::BOTTOM_LEFT | o::BOTTOM | o::BOTTOM_RIGHT => 1.0,
        _ => 0.5,
    };
    (w * fx, h * fy)
}

/// The item's transform to scene coordinates (its top-level ancestor's).
pub fn scene_transform(engine: &Engine, obj: ObjectId) -> Affine {
    let mut t = Affine::IDENTITY;
    let mut cur = Some(obj);
    // Bounded so a corrupt parent cycle cannot hang.
    for _ in 0..4096 {
        let Some(o) = cur else { break };
        t = local_transform(engine, o) * t;
        cur = engine.visual_parent(o);
    }
    t
}

fn map_point(engine: &Engine, from: Option<ObjectId>, to: Option<ObjectId>, p: Point) -> Point {
    let from_t = from.map_or(Affine::IDENTITY, |o| scene_transform(engine, o));
    let to_t = to.map_or(Affine::IDENTITY, |o| scene_transform(engine, o));
    let q = to_t.inverse() * (from_t * kurbo::Point::new(p.x, p.y));
    Point { x: q.x, y: q.y }
}

/// Whether `obj`'s type is `Item` or derives from it.
pub fn is_item(engine: &Engine, obj: ObjectId) -> bool {
    engine
        .meta_of(obj)
        .is_some_and(|m| m.inherits(Item::static_meta()))
}

/// Whether the object's type derives from `T`.
pub(crate) fn is_a<T: QmlType>(engine: &Engine, obj: ObjectId) -> bool {
    engine
        .meta_of(obj)
        .is_some_and(|m| m.inherits(T::static_meta()))
}

/// Sets an item's static accessibility data.
pub fn set_accessible(
    engine: &Engine,
    obj: ObjectId,
    info: AccessibleInfo,
) -> Result<(), EngineError> {
    engine.with_mut(obj, |i: &mut Item| i.accessible = info)
}

/// An item's static accessibility data.
pub fn accessible(engine: &Engine, obj: ObjectId) -> Result<AccessibleInfo, EngineError> {
    engine.with(obj, |i: &Item| i.accessible.clone())
}

/// Writes a property from native code without breaking a binding the user
/// put on it later. Used for values Ljós computes (implicit sizes).
pub(crate) fn write_computed(
    engine: &Engine,
    obj: ObjectId,
    p: PropIndex,
    v: Value,
) -> Result<(), EngineError> {
    if engine.untrack(|| engine.read(obj, p)).change_equals(&v) {
        return Ok(());
    }
    engine.write_internal(obj, p, v, WriteKind::Imperative)
}
