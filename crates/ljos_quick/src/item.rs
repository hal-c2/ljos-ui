//! `Item`, the base of every visual type, its `anchors` group and anchor
//! lines.

use std::rc::{Rc, Weak};
use std::sync::OnceLock;

use ljos_engine::{
    Atom, ConvertError, Cx, Engine, EngineError, EnumValue, LogLevel, ObjectFlags, ObjectId, Point,
    PropIndex, QmlList, QmlObject, QmlType, QmlValue, QtObject, Rect, Reset, TypeTag, Value,
    WriteKind, qml_impl,
};
use ljos_render::kurbo::{self, Affine};

use crate::enums::transform_origin;
use crate::palette::Palette;

/// `Item` (QtQuick).
///
/// `visible` and `enabled` read as the *effective* values once the item is
/// in a [`crate::QuickScene`]: a hidden ancestor makes the item read
/// `visible: false`, as Qt documents. The scene does that with an override
/// layer, so the value the item was given comes back when the ancestor is
/// shown again.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick",
    name = "Item",
    extends = QtObject,
    default = "data",
    enums = crate::enums::item_enums
)]
pub struct Item {
    #[qml(base)]
    pub object: QtObject,
    #[qml(notify)]
    pub x: f64,
    #[qml(notify)]
    pub y: f64,
    #[qml(notify)]
    pub z: f64,
    #[qml(notify, reset = SIZE_RESET)]
    pub width: f64,
    #[qml(notify, reset = SIZE_RESET)]
    pub height: f64,
    #[qml(notify)]
    pub implicit_width: f64,
    #[qml(notify)]
    pub implicit_height: f64,
    #[qml(notify)]
    pub visible: bool,
    #[qml(notify, write = Self::keep_opacity)]
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
    #[qml(notify, enumeration = "TransformOrigin")]
    pub transform_origin: i32,
    /// The visual parent. Assigning it reparents the item (see
    /// [`crate::set_parent_item`]); it follows
    /// [`Engine::set_visual_parent`] whoever calls that.
    #[qml(notify)]
    pub parent: Option<ObjectId>,
    /// Children and resources together, in the order they were added.
    /// Appending sorts: an `Item` becomes a visual child, anything else a
    /// resource, as Qt documents for the default property.
    #[qml(
        notify,
        list,
        append = crate::tree::item_data_append,
        insert = crate::tree::item_data_insert,
        remove = crate::tree::item_data_remove
    )]
    pub data: QmlList,
    /// Appending makes the item a child, as Qt's `children` does.
    #[qml(
        notify,
        list,
        append = crate::tree::item_children_append,
        insert = crate::tree::item_children_insert,
        remove = crate::tree::item_children_remove
    )]
    pub children: QmlList,
    #[qml(notify, list)]
    pub resources: QmlList,
    #[qml(notify, readonly)]
    pub children_rect: Rect,
    #[qml(notify)]
    pub baseline_offset: f64,
    /// Off for a plain item; `Text` turns it on and a `Rectangle` follows
    /// its `radius`, as Qt documents.
    #[qml(notify, reset = Reset::Follow(natural_antialiasing))]
    pub antialiasing: bool,
    /// Whether what the item draws is filtered when it is scaled or
    /// transformed (a picture; nothing else of ours changes). On, as in Qt.
    #[qml(notify)]
    pub smooth: bool,
    #[qml(group)]
    pub anchors: Anchors,
    // Anchor lines (`parent.left`). Set in `on_complete`, when the object
    // knows its own id.
    #[qml(notify, readonly)]
    pub left: LineOf,
    #[qml(notify, readonly)]
    pub right: LineOf,
    #[qml(notify, readonly)]
    pub top: LineOf,
    #[qml(notify, readonly)]
    pub bottom: LineOf,
    #[qml(notify, readonly)]
    pub horizontal_center: LineOf,
    #[qml(notify, readonly)]
    pub vertical_center: LineOf,
    #[qml(rename = "baseline", notify, readonly)]
    pub baseline_line: LineOf,
    /// Whether the item is its focus scope's focus item (see
    /// [`crate::focus`]).
    #[qml(notify)]
    pub focus: bool,
    /// Whether the item, or an item it is the focus scope of, gets the
    /// keys. Written by the focus tree only.
    #[qml(notify, readonly)]
    pub active_focus: bool,
    /// Whether Tab stops at the item.
    #[qml(notify)]
    pub active_focus_on_tab: bool,
    /// The item's states, the one it is in (`""` for none of them) and the
    /// transitions between them: see [`crate::states`].
    #[qml(notify, list)]
    pub states: QmlList,
    #[qml(notify)]
    pub state: Atom,
    #[qml(notify, list)]
    pub transitions: QmlList,
    /// `Rotation`, `Scale`, `Translate`, `Matrix4x4` and `Shear`, applied
    /// after `rotation` and `scale` (see [`crate::transform`]).
    #[qml(notify, list)]
    pub transform: QmlList,
    #[qml(group)]
    pub layer: ItemLayer,
    /// Made when a colour is first set: most items never set one.
    #[qml(group, lazy, reset)]
    pub palette: Option<Box<Palette>>,
    /// Static accessibility data for a later a11y pass (ARCHITECTURE §14).
    #[qml(skip)]
    pub accessible: AccessibleInfo,
    /// The visual parent `children`/`data` and `parent` were last brought
    /// in step with, so a change knows which parent's lists to leave.
    #[qml(skip)]
    pub(crate) synced_parent: Option<ObjectId>,
    /// Alive while `width` (0) and `height` (1) still follow the implicit
    /// size: each is held by the fallback binding [`init_item`] makes, and
    /// the engine drops that binding when the size is given.
    #[qml(skip)]
    pub(crate) follows_implicit: [Weak<()>; 2],
    /// The visual parent the focus tree last saw, so a reparent knows
    /// which focus scope the item left.
    #[qml(skip)]
    pub(crate) focus_parent: Option<ObjectId>,
    /// Why the item last gained active focus (a `Qt.FocusReason`).
    #[qml(skip)]
    pub(crate) focus_reason: i32,
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
            data: QmlList::new(),
            children: QmlList::new(),
            resources: QmlList::new(),
            children_rect: Rect::default(),
            baseline_offset: 0.0,
            antialiasing: false,
            smooth: true,
            anchors: Anchors::default(),
            left: LineOf::default(),
            right: LineOf::default(),
            top: LineOf::default(),
            bottom: LineOf::default(),
            horizontal_center: LineOf::default(),
            vertical_center: LineOf::default(),
            baseline_line: LineOf::default(),
            focus: false,
            active_focus: false,
            active_focus_on_tab: false,
            states: QmlList::new(),
            state: Atom::EMPTY,
            transitions: QmlList::new(),
            transform: QmlList::new(),
            layer: ItemLayer::default(),
            palette: None,
            accessible: AccessibleInfo::default(),
            synced_parent: None,
            follows_implicit: [Weak::new(), Weak::new()],
            focus_parent: None,
            focus_reason: crate::focus::FocusReason::Other as i32,
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

/// The `layer` group of [`Item`]: a ShaderEffect that names an item whose
/// layer is enabled samples a picture of it, taken as a
/// `ShaderEffectSource` would take it (see [`crate::shader_effect`]).
/// `effect`, `samplerName`, `format` and `samples` are kept and do nothing.
#[derive(Debug, QmlObject)]
#[qml(name = "ItemLayer", uncreatable)]
pub struct ItemLayer {
    #[qml(notify)]
    pub enabled: bool,
    #[qml(notify)]
    pub smooth: bool,
    #[qml(notify)]
    pub mipmap: bool,
    #[qml(notify)]
    pub wrap_mode: i32,
    #[qml(notify)]
    pub texture_mirroring: i32,
    #[qml(notify)]
    pub sampler_name: Atom,
    #[qml(notify)]
    pub format: i32,
    #[qml(notify)]
    pub samples: i32,
    #[qml(notify)]
    pub live: bool,
    #[qml(notify)]
    pub effect: Option<ObjectId>,
    #[qml(notify)]
    pub texture_size: ljos_engine::Size,
    #[qml(notify)]
    pub source_rect: Rect,
}

impl Default for ItemLayer {
    fn default() -> Self {
        Self {
            enabled: false,
            smooth: false,
            mipmap: false,
            wrap_mode: 0,
            texture_mirroring: 2,
            // Every item has a layer; one shared name, not one per item.
            sampler_name: {
                static SOURCE: std::sync::LazyLock<Atom> =
                    std::sync::LazyLock::new(|| Atom::from("source"));
                SOURCE.clone()
            },
            format: 1,
            samples: 0,
            live: true,
            effect: None,
            texture_size: ljos_engine::Size {
                width: -1.0,
                height: -1.0,
            },
            source_rect: Rect::default(),
        }
    }
}

/// The `anchors` group of [`Item`].
#[derive(Debug, QmlObject)]
#[qml(name = "Anchors", uncreatable)]
pub struct Anchors {
    #[qml(notify, reset)]
    pub left: Value,
    #[qml(notify, reset)]
    pub right: Value,
    #[qml(notify, reset)]
    pub top: Value,
    #[qml(notify, reset)]
    pub bottom: Value,
    #[qml(notify, reset)]
    pub horizontal_center: Value,
    #[qml(notify, reset)]
    pub vertical_center: Value,
    #[qml(notify, reset)]
    pub baseline: Value,
    #[qml(notify, reset)]
    pub fill: Option<ObjectId>,
    #[qml(notify, reset)]
    pub center_in: Option<ObjectId>,
    #[qml(notify, write = margins_written)]
    pub margins: f64,
    /// The four side margins follow `margins` until they are set.
    #[qml(notify, reset = MARGIN_RESET)]
    pub left_margin: f64,
    #[qml(notify, reset = MARGIN_RESET)]
    pub right_margin: f64,
    #[qml(notify, reset = MARGIN_RESET)]
    pub top_margin: f64,
    #[qml(notify, reset = MARGIN_RESET)]
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
/// A completed item has its unset sides bound to `margins`
/// ([`init_item`]), so QML reads the same number. The check here covers an
/// item that is read before it completes.
pub fn effective_margin(engine: &Engine, obj: ObjectId, side: PropIndex) -> f64 {
    let own = engine.read(obj, side).to_number();
    let margins = engine.read(obj, AnchorProps::get().margins).to_number();
    if is_unset(engine, obj, side) {
        margins
    } else {
        own
    }
}

/// Whether nothing has given the item a size along one axis: it still
/// follows its implicit size and no pair of anchors stretches it. Tracks
/// the anchors, so an effect asking re-runs when they change.
pub(crate) fn size_follows_implicit(engine: &Engine, obj: ObjectId, horizontal: bool) -> bool {
    let axis = usize::from(!horizontal);
    let follows = engine
        .with(obj, |i: &Item| i.follows_implicit[axis].strong_count() > 0)
        .unwrap_or(false);
    if !follows {
        return false;
    }
    let a = AnchorProps::get();
    let set = |p| !engine.read(obj, p).is_nullish();
    let (near, far) = if horizontal {
        (a.left, a.right)
    } else {
        (a.top, a.bottom)
    };
    !(set(a.fill) || (set(near) && set(far)))
}

/// A control that lays a part out gives it its size: from now on the part
/// is a given size on both axes, so a text in it wraps to that width
/// where it would have wrapped to nothing while it followed its own
/// implicit size. The value stays whatever the control's layer says.
pub(crate) fn give_size(engine: &Engine, obj: ObjectId) {
    let _ = engine.with_mut(obj, |i: &mut Item| {
        i.follows_implicit = [Weak::new(), Weak::new()];
    });
}

/// `width = undefined` and `height = undefined`: the size follows the
/// implicit size again, as Qt documents.
const SIZE_RESET: Reset = Reset::Custom {
    apply: follow_implicit_size,
    value: implicit_size,
};

fn implicit_of(size: PropIndex) -> (usize, PropIndex) {
    if size == Item::WIDTH.index {
        (0, Item::IMPLICIT_WIDTH.index)
    } else {
        (1, Item::IMPLICIT_HEIGHT.index)
    }
}

fn implicit_size(engine: &Engine, obj: ObjectId, size: PropIndex) -> Value {
    engine.read(obj, implicit_of(size).1)
}

/// Makes `width` or `height` read the implicit size until it is given a
/// value. Not a value the author wrote: a Behavior on the size does not
/// animate it, as it does not in Qt.
fn follow_implicit_size(
    engine: &Engine,
    obj: ObjectId,
    size: PropIndex,
) -> Result<(), EngineError> {
    let (axis, implicit) = implicit_of(size);
    let editor = axis == 0 && is_a::<crate::text_input::TextEditor>(engine, obj);
    let token = Rc::new(());
    engine.with_mut(obj, |i: &mut Item| {
        i.follows_implicit[axis] = Rc::downgrade(&token);
    })?;
    engine.bind_computed(obj, size, move |e| {
        // Held, not used: see `Item::follows_implicit`.
        let _alive = &token;
        if editor {
            return crate::text_input::followed_width(e, obj);
        }
        e.read(obj, implicit)
    })
}

/// What a side margin is while it has no value of its own.
fn all_margins(engine: &Engine, obj: ObjectId, _side: PropIndex) -> Value {
    engine.read(obj, AnchorProps::get().margins)
}

/// A side margin taken back is as a new item has it: bound to `margins`
/// once that has a value, and otherwise its default with nothing bound,
/// to be bound when `margins` is first written ([`margins_written`]).
const MARGIN_RESET: Reset = Reset::Custom {
    apply: take_back_margin,
    value: all_margins,
};

fn take_back_margin(engine: &Engine, obj: ObjectId, side: PropIndex) -> Result<(), EngineError> {
    if is_unset(engine, obj, AnchorProps::get().margins) {
        engine.reset_to_default(obj, side)
    } else {
        engine.bind_computed(obj, side, move |e| all_margins(e, obj, side))
    }
}

/// What `antialiasing` is until it is given: on for a rounded rectangle,
/// otherwise what the type starts with (on for text), as Qt documents.
fn natural_antialiasing(engine: &Engine, obj: ObjectId, p: PropIndex) -> Value {
    use crate::rectangle::Rectangle;
    if is_a::<Rectangle>(engine, obj) {
        return Value::Bool(
            [
                Rectangle::RADIUS.index,
                Rectangle::TOP_LEFT_RADIUS.index,
                Rectangle::TOP_RIGHT_RADIUS.index,
                Rectangle::BOTTOM_LEFT_RADIUS.index,
                Rectangle::BOTTOM_RIGHT_RADIUS.index,
            ]
            .into_iter()
            .any(|r| engine.read(obj, r).to_number() > 0.0),
        );
    }
    engine.default_value(obj, p)
}

/// Whether nothing has given the property a value yet.
pub(crate) fn is_unset(engine: &Engine, obj: ObjectId, p: PropIndex) -> bool {
    matches!(
        engine.provenance(obj, p),
        Some(ljos_engine::Provenance::Default) | None
    )
}

/// [`is_unset`] as a tracked read, for where the default means something
/// else than the same value given on purpose: the effect that asks runs
/// again when the property gets its first value, whatever the value.
pub(crate) fn is_unset_tracked(engine: &Engine, obj: ObjectId, p: PropIndex) -> bool {
    matches!(
        engine.provenance_tracked(obj, p),
        Some(ljos_engine::Provenance::Default) | None
    )
}

/// `anchors.margins` written to a complete item: the sides start to follow
/// it. A filter rather than a watcher, which would be a node and an effect
/// on every item, for a property nearly none sets. What it binds hears this
/// write when it is stored.
fn margins_written(cx: &Cx, m: f64) -> Option<f64> {
    let (engine, obj) = (cx.engine, cx.this);
    let complete = engine
        .flags(obj)
        .is_some_and(|f| f.contains(ObjectFlags::COMPLETED));
    if complete && let Err(err) = follow_margins(engine, obj) {
        engine.log(LogLevel::Warning, "qml", format!("anchors.margins: {err}"));
    }
    Some(m)
}

/// Makes every side margin that has no value of its own read
/// `anchors.margins`.
fn follow_margins(engine: &Engine, obj: ObjectId) -> Result<(), EngineError> {
    let a = AnchorProps::get();
    for side in [a.left_margin, a.right_margin, a.top_margin, a.bottom_margin] {
        if is_unset(engine, obj, side) {
            engine.bind(obj, side, move |e| e.read(obj, AnchorProps::get().margins))?;
        }
    }
    Ok(())
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

/// One of an item's own anchor lines, as its field holds it: the line
/// becomes a value only when it is read, so an item that nobody anchors to
/// makes none of its seven.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LineOf(pub Option<AnchorLine>);

impl QmlValue for LineOf {
    fn type_tag() -> TypeTag {
        TypeTag::Var
    }
    fn to_value(&self) -> Value {
        self.0.map_or(Value::Undefined, AnchorLine::to_value)
    }
    fn from_value(v: &Value) -> Result<Self, ConvertError> {
        Ok(LineOf(AnchorLine::from_value(v)))
    }
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
            // A line a script handed on (`cond ? undefined : parent.right`)
            // comes back with the edge as the plain number a script sees.
            [Value::Object(item), edge @ (Value::Int(_) | Value::Real(_))] => {
                let code = edge.to_number();
                let edge = Edge::from_code(code as i32).filter(|e| f64::from(e.code()) == code)?;
                Some(AnchorLine { item: *item, edge })
            }
            _ => None,
        }
    }
}

impl Item {
    /// Qt keeps `opacity` in 0..1 as it is set, so a write past either end
    /// stores the end and, if the item is already there, tells nobody.
    fn keep_opacity(&self, opacity: f64) -> Option<f64> {
        Some(opacity.clamp(0.0, 1.0))
    }
}

#[qml_impl]
impl Item {
    /// Fills in what needs the object's own id: anchor lines, the lazy
    /// `childrenRect`, and the implicit-size fallback for `width`/`height`.
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        init_item(cx.engine, cx.this)?;
        crate::palette::item_palette_set(cx.engine, cx.this, false);
        Ok(())
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

    /// `mapToGlobal(x, y)` or `mapToGlobal(point)`: a point in this item's
    /// coordinates, on the desktop.
    #[qml(invokable, name = "mapToGlobal")]
    fn map_to_global(cx: &Cx, x: Value, y: Value) -> Point {
        let (e, me) = (cx.engine, cx.this);
        let at = map_point(e, Some(me), None, point_of(&x, &y));
        let origin = window_position(e, me);
        Point {
            x: at.x + origin.x,
            y: at.y + origin.y,
        }
    }

    /// `mapFromGlobal(x, y)` or `mapFromGlobal(point)`: a point on the
    /// desktop, in this item's coordinates.
    #[qml(invokable, name = "mapFromGlobal")]
    fn map_from_global(cx: &Cx, x: Value, y: Value) -> Point {
        let (e, me) = (cx.engine, cx.this);
        let at = point_of(&x, &y);
        let origin = window_position(e, me);
        let scene = Point {
            x: at.x - origin.x,
            y: at.y - origin.y,
        };
        map_point(e, None, Some(me), scene)
    }

    /// `contains(point)`: whether a point in local coordinates is inside
    /// the item's bounding rectangle.
    #[qml(invokable)]
    fn contains(&self, point: Point) -> bool {
        point.x >= 0.0 && point.y >= 0.0 && point.x < self.width && point.y < self.height
    }

    /// `childAt(x, y)`: the visible child under a point in this item's
    /// coordinates, mapped through the child's transform, or null. Like
    /// Qt, the last child in `children` wins whatever the `z`, and opacity
    /// does not matter.
    #[qml(invokable, name = "childAt")]
    fn child_at(cx: &Cx, x: f64, y: f64) -> Option<ObjectId> {
        let (e, me) = (cx.engine, cx.this);
        let under = |c: ObjectId| {
            if !e.read(c, Item::VISIBLE.index).to_boolean() {
                return false;
            }
            let p = map_point(e, Some(me), Some(c), Point { x, y });
            let w = e.read(c, Item::WIDTH.index).to_number();
            let h = e.read(c, Item::HEIGHT.index).to_number();
            p.x >= 0.0 && p.y >= 0.0 && p.x < w && p.y < h
        };
        crate::tree::child_items(e, me)
            .into_iter()
            .rev()
            .find(|c| under(*c))
    }

    /// `forceActiveFocus(reason)`: focus for the item and for every focus
    /// scope around it. Without a reason, `Qt.OtherFocusReason`.
    #[qml(invokable, name = "forceActiveFocus")]
    fn force_active_focus(cx: &Cx, reason: Value) {
        let reason = match reason.as_number() {
            Some(n) => crate::focus::FocusReason::from_qt(n as i32),
            None => crate::focus::FocusReason::Other,
        };
        crate::focus::force_active_focus(cx.engine, cx.this, reason);
    }

    /// `nextItemInFocusChain(forward)`: where Tab (or, with `false`,
    /// Shift+Tab) goes from the item.
    #[qml(invokable, name = "nextItemInFocusChain")]
    fn next_item_in_focus_chain(cx: &Cx, forward: Value) -> Option<ObjectId> {
        let forward = forward.is_nullish() || forward.to_boolean();
        Some(crate::focus::next_item_in_focus_chain(
            cx.engine, cx.this, forward,
        ))
    }
}

/// What `on_complete` does, for trees built natively before completion or
/// when the hook did not run. Idempotent.
pub fn init_item(engine: &Engine, obj: ObjectId) -> Result<(), EngineError> {
    let line = |edge| LineOf(Some(AnchorLine { item: obj, edge }));
    let already = engine.with_mut(obj, |i: &mut Item| {
        if i.left.0.is_some() {
            return true;
        }
        i.left = line(Edge::Left);
        i.right = line(Edge::Right);
        i.top = line(Edge::Top);
        i.bottom = line(Edge::Bottom);
        i.horizontal_center = line(Edge::HorizontalCenter);
        i.vertical_center = line(Edge::VerticalCenter);
        i.baseline_line = line(Edge::Baseline);
        false
    })?;
    if already {
        return Ok(());
    }
    engine.batch(|| -> Result<(), EngineError> {
        // Only a line read before the item was complete has observers.
        for p in [
            Item::LEFT.index,
            Item::RIGHT.index,
            Item::TOP.index,
            Item::BOTTOM.index,
            Item::HORIZONTAL_CENTER.index,
            Item::VERTICAL_CENTER.index,
            Item::BASELINE_LINE.index,
        ] {
            engine.notify(obj, p)?;
        }
        // An item whose size was never given follows its implicit size.
        // Assigning `width` replaces this binding, as in Qt.
        let unset = |p| is_unset(engine, obj, p);
        // Most items never set `margins`, so the four side bindings are
        // only made once it has a value: here, or when it is first written
        // ([`margins_written`]).
        if !unset(AnchorProps::get().margins) {
            follow_margins(engine, obj)?;
        }
        for size in [Item::WIDTH.index, Item::HEIGHT.index] {
            if !unset(size) {
                continue;
            }
            follow_implicit_size(engine, obj, size)?;
        }
        // Made when something first reads `childrenRect`: nearly nothing
        // does, and a binding is a node on every item.
        engine.bind_on_read(obj, Item::CHILDREN_RECT.index, |e, obj, p| {
            e.bind_initial(obj, p, move |e| {
                Value::Rect(std::sync::Arc::new(children_rect(e, obj)))
            })
        })?;
        // Assigning `parent` reparents the item. A layer's `parent` names
        // the item it lies above, which it is drawn over and not in.
        engine.watch(obj, Item::PARENT.index, move |e, v| {
            let p = v.as_object();
            if e.visual_parent(obj) != p
                && crate::layer::layer_host(e, obj).is_none()
                && let Err(err) = crate::tree::set_parent_item(e, obj, p)
            {
                e.log(
                    LogLevel::Warning,
                    "qml",
                    format!("Cannot set parent: {err}"),
                );
            }
        })?;
        let declared = engine.untrack(|| engine.read(obj, Item::PARENT.index).as_object());
        let actual = engine.untrack(|| engine.visual_parent(obj));
        if declared != actual {
            if unset(Item::PARENT.index) {
                crate::tree::sync_parent(engine, obj, actual)?;
            } else {
                crate::tree::set_parent_item(engine, obj, declared)?;
            }
        }
        // After the parent is settled: `focus` is claimed in a scope.
        crate::focus::init(engine, obj)?;
        Ok(())
    })
}

/// The union of the children's geometry, in this item's coordinates.
fn children_rect(engine: &Engine, obj: ObjectId) -> Rect {
    let mut acc: Option<(f64, f64, f64, f64)> = None;
    for c in crate::tree::child_items(engine, obj) {
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
    engine.read_number(obj, p)
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
    let list = crate::transform::list_map(engine, obj);
    crate::transform::compose(x, y, w, h, rotation, scale, origin, &list)
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

/// Where `p`, in the item's coordinates, lies in the scene's, mapped
/// exactly (perspective and all), as `mapToItem(null, p)`. An item in a
/// layer maps to the scene of the tree the layer lies above.
pub fn map_to_scene(engine: &Engine, obj: ObjectId, p: kurbo::Point) -> kurbo::Point {
    let q = map_point(engine, Some(obj), None, Point { x: p.x, y: p.y });
    kurbo::Point::new(q.x, q.y)
}

fn map_point(engine: &Engine, from: Option<ObjectId>, to: Option<ObjectId>, p: Point) -> Point {
    // Exact, perspective and all, as Qt maps: the render tree's affine
    // fit is only for the picture.
    use crate::transform::{Projective, scene_map};
    let from_t = from.map_or(Projective::IDENTITY, |o| scene_map(engine, o));
    let to_t = to.map_or(Projective::IDENTITY, |o| scene_map(engine, o));
    let q = to_t
        .inverse()
        .and_then(|inv| inv.apply(from_t.apply(kurbo::Point::new(p.x, p.y))?))
        .unwrap_or(kurbo::Point::new(p.x, p.y));
    Point { x: q.x, y: q.y }
}

/// The point a script gives as `(x, y)` or as one `point`.
fn point_of(x: &Value, y: &Value) -> Point {
    match x {
        Value::Point(p) => **p,
        Value::IntPoint(p) => Point {
            x: f64::from(p.x),
            y: f64::from(p.y),
        },
        _ => Point {
            x: x.to_number(),
            y: y.to_number(),
        },
    }
}

/// Where the window an item is shown in is on the desktop, as a
/// [`WindowPosition`] hook gives it.
pub type WindowPosition = fn(&Engine, ObjectId) -> Option<Point>;

#[derive(Default)]
struct WindowPositions(std::cell::Cell<Option<WindowPosition>>);

/// Says where an item's window is on the desktop, for `mapToGlobal` and
/// `mapFromGlobal`. The window types live above this crate; without
/// one, a window is at the desktop's origin, as an off-screen one is.
pub fn register_window_position(engine: &Engine, position: WindowPosition) {
    engine.extension::<WindowPositions>().0.set(Some(position));
}

fn window_position(engine: &Engine, item: ObjectId) -> Point {
    let hook = engine.extension::<WindowPositions>().0.get();
    hook.and_then(|f| f(engine, item)).unwrap_or_default()
}

/// Whether `obj`'s type is `Item` or derives from it.
pub fn is_item(engine: &Engine, obj: ObjectId) -> bool {
    engine
        .meta_of(obj)
        .is_some_and(|m| m.inherits(Item::static_meta()))
}

/// Whether the object's type derives from `T`.
pub(crate) fn is_a<T: QmlType>(engine: &Engine, obj: ObjectId) -> bool {
    engine.inherits(obj, T::static_meta())
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

/// Writes what a layout or a positioner chose for a child: its position,
/// its size, or whether a stack shows it. As in Qt this is not an
/// assignment: a binding the user put on the property stays and gives the
/// value again when its inputs change, and no `Behavior` animates it.
pub(crate) fn write_placed(
    engine: &Engine,
    obj: ObjectId,
    p: PropIndex,
    v: Value,
) -> Result<(), EngineError> {
    // Reading first also runs a binding that was due, so that it does not
    // run after the write and take the place back.
    if engine.untrack(|| engine.read(obj, p)).change_equals(&v) {
        return Ok(());
    }
    // The binding that makes a size follow the implicit size is ours, and
    // it goes: a size a layout gave is a given size from then on.
    let axis = [Item::WIDTH.index, Item::HEIGHT.index]
        .iter()
        .position(|size| *size == p);
    let ours = axis.is_some_and(|axis| {
        engine
            .with(obj, |i: &Item| i.follows_implicit[axis].strong_count() > 0)
            .unwrap_or(false)
    });
    let how = if ours {
        WriteKind::Imperative
    } else {
        WriteKind::Layout
    };
    engine.write_internal(obj, p, v, how)
}
