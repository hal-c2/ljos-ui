//! `Path` and the elements it is made of.
//!
//! The objects only hold what QML wrote. What they come to is a
//! [`PathGeometry`], worked out when somebody asks for it and kept until one
//! of the properties it was made from changes: a path that does not change
//! is the same `Arc` every time it is painted, which is what lets a render
//! backend keep what it built for it.
//!
//! How the elements are read follows Qt, each rule checked on qml6 (the
//! scenarios in `features/shapes/` say which).

use std::rc::Rc;
use std::sync::Arc;

use ljos_engine::{
    Atom, Cx, Engine, EngineError, LogLevel, ObjectId, PropIndex, QmlList, QmlObject, QmlType,
    QtObject, Size, Value, qml_impl,
};
use ljos_render::kurbo::{Affine, BezPath, Point, Rect, Vec2};

use super::builder::{Builder, bounds_of, point_at};
use super::svg::{self, SvgEl};
use crate::enums::EnumTable;
use crate::item::{is_unset, is_unset_tracked, num};

/// What a `Path` comes to.
#[derive(Debug)]
pub(crate) struct PathGeometry {
    /// The outline, `Path.scale` applied. The same `Arc` for as long as
    /// nothing it was made from changes.
    pub path: Arc<BezPath>,
    /// Where the outline lies, by where it goes.
    pub bounds: Rect,
    /// Whether the path ends where it starts.
    pub closed: bool,
}

/// `Path` (QtQuick): an outline made of elements, from `startX`, `startY`.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick", name = "Path", extends = QtObject, default = "pathElements")]
pub struct Path {
    #[qml(base)]
    pub object: QtObject,
    #[qml(list, append = element_append)]
    pub path_elements: QmlList,
    #[qml(notify)]
    pub start_x: f64,
    #[qml(notify)]
    pub start_y: f64,
    /// Whether the path ends where it starts.
    #[qml(notify, readonly)]
    pub closed: bool,
    /// Accepted; the outline is used as it was written.
    #[qml(notify)]
    pub simplify: bool,
    /// What every point of the path is multiplied by.
    #[qml(notify)]
    pub scale: Size,
    /// Accepted; the outline is worked out when it is asked for.
    #[qml(notify)]
    pub asynchronous: bool,
    /// The object whose one property is the geometry's dependency node.
    #[qml(skip)]
    cache: Option<ObjectId>,
    #[qml(skip)]
    geometry: Option<Rc<PathGeometry>>,
}

impl Default for Path {
    fn default() -> Self {
        Self {
            object: QtObject::default(),
            path_elements: QmlList::new(),
            start_x: 0.0,
            start_y: 0.0,
            closed: false,
            simplify: false,
            scale: Size {
                width: 1.0,
                height: 1.0,
            },
            asynchronous: false,
            cache: None,
            geometry: None,
        }
    }
}

#[qml_impl]
impl Path {
    /// The outline has changed.
    #[qml(signal)]
    fn changed();

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (engine, obj) = (cx.engine, cx.this);
        engine.bind_initial(obj, Path::CLOSED.index, move |e| {
            Value::Bool(geometry(e, obj).is_some_and(|g| g.closed))
        })?;
        // The first run only learns what to follow: a path that was just
        // made has not changed.
        let mut first = true;
        engine.effect_in(obj, move |e| {
            let _ = geometry(e, obj);
            if !std::mem::take(&mut first) {
                let _ = e.emit_by_name(obj, "changed", &[]);
            }
        })?;
        Ok(())
    }

    /// `pointAtPercent(t)`: the point `t` of the way along the path by
    /// length, `t` held between 0 and 1.
    #[qml(invokable, name = "pointAtPercent")]
    fn point_at_percent(cx: &Cx, t: Value) -> ljos_engine::Point {
        let at = geometry(cx.engine, cx.this)
            .map(|g| point_at(&g.path, t.to_number()))
            .unwrap_or_default();
        ljos_engine::Point { x: at.x, y: at.y }
    }
}

fn element_append(cx: &Cx, element: ObjectId) -> Result<(), EngineError> {
    if Kind::of(cx.engine, element).is_none() {
        return Err(EngineError::WrongType {
            ty: "Path.pathElements".into(),
            expected: "a path element".into(),
        });
    }
    cx.engine
        .list_push(cx.this, Path::PATH_ELEMENTS.index, element)
}

/// Holds the node a path's geometry hangs on: `revision` is a lazy binding
/// that builds the geometry, so reading it follows everything the geometry
/// was made from, and building happens once however many ask.
#[derive(Debug, Default, QmlObject)]
#[qml(name = "PathGeometryCache", extends = QtObject, uncreatable)]
pub struct GeometryCache {
    #[qml(base)]
    pub object: QtObject,
    #[qml(notify)]
    pub revision: i32,
}

fn cache_of(engine: &Engine, path: ObjectId) -> Option<ObjectId> {
    if let Some(cache) = engine.with(path, |p: &Path| p.cache).ok()? {
        return Some(cache);
    }
    // Whoever asked first must not come to depend on the making of it.
    engine.untrack(|| {
        let cache = engine
            .create_native(GeometryCache::default(), Some(path))
            .ok()?;
        engine
            .with_mut(path, |p: &mut Path| p.cache = Some(cache))
            .ok()?;
        let mut revision = 0_i32;
        engine
            .bind_initial(cache, GeometryCache::REVISION.index, move |e| {
                let built = Rc::new(build(e, path));
                let _ = e.with_mut(path, |p: &mut Path| p.geometry = Some(built));
                // Never the same twice, so that everything that read the
                // geometry reads it again.
                revision = revision.wrapping_add(1);
                Value::Int(revision)
            })
            .ok()?;
        Some(cache)
    })
}

/// The geometry of a `Path` (or `ShapePath`). Tracked: what asks from a
/// binding or an effect runs again when the path changes. `None` for what
/// is not a path, or is gone.
pub(crate) fn geometry(engine: &Engine, path: ObjectId) -> Option<Rc<PathGeometry>> {
    let cache = cache_of(engine, path)?;
    let _ = engine.read(cache, GeometryCache::REVISION.index);
    engine.with(path, |p: &Path| p.geometry.clone()).ok()?
}

/// The base of everything that can be put in a `Path`.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick", name = "PathElement", extends = QtObject, uncreatable)]
pub struct PathElement {
    #[qml(base)]
    pub object: QtObject,
}

/// What the elements that go somewhere share: where to, from the path's
/// origin (`x`, `y`) or from where the element starts (`relativeX`,
/// `relativeY`). A relative coordinate that was given wins, whatever it is.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtQuick",
    name = "PathCurveElement",
    extends = PathElement,
    uncreatable
)]
pub struct CurveElement {
    #[qml(base)]
    pub element: PathElement,
    #[qml(notify)]
    pub x: f64,
    #[qml(notify)]
    pub y: f64,
    #[qml(notify)]
    pub relative_x: f64,
    #[qml(notify)]
    pub relative_y: f64,
}

/// `PathMove`: goes to a point without drawing; what follows is a part of
/// its own.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick", name = "PathMove", extends = CurveElement)]
pub struct PathMove {
    #[qml(base)]
    pub curve: CurveElement,
}

/// `PathLine`: a straight line.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick", name = "PathLine", extends = CurveElement)]
pub struct PathLine {
    #[qml(base)]
    pub curve: CurveElement,
}

/// `PathQuad`: a curve with one control point.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick", name = "PathQuad", extends = CurveElement)]
pub struct PathQuad {
    #[qml(base)]
    pub curve: CurveElement,
    #[qml(notify)]
    pub control_x: f64,
    #[qml(notify)]
    pub control_y: f64,
    #[qml(notify)]
    pub relative_control_x: f64,
    #[qml(notify)]
    pub relative_control_y: f64,
}

/// `PathCubic`: a curve with two control points.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick", name = "PathCubic", extends = CurveElement)]
pub struct PathCubic {
    #[qml(base)]
    pub curve: CurveElement,
    #[qml(notify)]
    pub control1_x: f64,
    #[qml(notify)]
    pub control1_y: f64,
    #[qml(notify)]
    pub control2_x: f64,
    #[qml(notify)]
    pub control2_y: f64,
    #[qml(notify)]
    pub relative_control1_x: f64,
    #[qml(notify)]
    pub relative_control1_y: f64,
    #[qml(notify)]
    pub relative_control2_x: f64,
    #[qml(notify)]
    pub relative_control2_y: f64,
}

/// `PathArc.Clockwise`.
pub const CLOCKWISE: i32 = 0;
/// `PathArc.Counterclockwise`.
pub const COUNTERCLOCKWISE: i32 = 1;

pub(crate) fn arc_enums() -> EnumTable {
    vec![(
        "ArcDirection",
        vec![
            ("Clockwise", i64::from(CLOCKWISE)),
            ("Counterclockwise", i64::from(COUNTERCLOCKWISE)),
        ],
    )]
}

/// `PathArc`: part of an ellipse, given by where it ends, as in SVG.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick", name = "PathArc", extends = CurveElement, enums = arc_enums)]
pub struct PathArc {
    #[qml(base)]
    pub curve: CurveElement,
    #[qml(notify)]
    pub radius_x: f64,
    #[qml(notify)]
    pub radius_y: f64,
    #[qml(notify)]
    pub use_large_arc: bool,
    #[qml(notify, enumeration = "ArcDirection")]
    pub direction: i32,
    /// In degrees.
    #[qml(notify)]
    pub x_axis_rotation: f64,
}

/// `PathAngleArc`: part of an ellipse, given by its centre and angles in
/// degrees.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick", name = "PathAngleArc", extends = CurveElement)]
pub struct PathAngleArc {
    #[qml(base)]
    pub curve: CurveElement,
    #[qml(notify)]
    pub center_x: f64,
    #[qml(notify)]
    pub center_y: f64,
    #[qml(notify)]
    pub radius_x: f64,
    #[qml(notify)]
    pub radius_y: f64,
    #[qml(notify)]
    pub start_angle: f64,
    #[qml(notify)]
    pub sweep_angle: f64,
    #[qml(notify)]
    pub move_to_start: bool,
}

impl Default for PathAngleArc {
    fn default() -> Self {
        Self {
            curve: CurveElement::default(),
            center_x: 0.0,
            center_y: 0.0,
            radius_x: 0.0,
            radius_y: 0.0,
            start_angle: 0.0,
            sweep_angle: 0.0,
            move_to_start: true,
        }
    }
}

/// `PathCurve`: a point of a smooth curve through several (Catmull-Rom).
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick", name = "PathCurve", extends = CurveElement)]
pub struct PathCurve {
    #[qml(base)]
    pub curve: CurveElement,
}

/// `PathSvg`: path data as SVG writes it.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick", name = "PathSvg", extends = CurveElement)]
pub struct PathSvg {
    #[qml(base)]
    pub curve: CurveElement,
    #[qml(notify)]
    pub path: Atom,
}

/// `PathPolyline`: lines through a list of points.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick", name = "PathPolyline", extends = CurveElement)]
pub struct PathPolyline {
    #[qml(base)]
    pub curve: CurveElement,
    /// The first point.
    #[qml(notify, readonly)]
    pub start: ljos_engine::Point,
    /// The points: a list of `point`s, or of pairs of numbers.
    #[qml(notify)]
    pub path: Value,
}

impl Default for PathPolyline {
    fn default() -> Self {
        Self {
            curve: CurveElement::default(),
            start: ljos_engine::Point::default(),
            path: Value::list(Vec::new()),
        }
    }
}

#[qml_impl]
impl PathPolyline {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let obj = cx.this;
        cx.engine
            .bind_initial(obj, PathPolyline::START.index, move |e| {
                let points = points_of(e, &e.read(obj, PathPolyline::PATH.index));
                first_point(points.first())
            })
    }
}

/// `PathMultiline`: several lists of points, each a line of its own.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick", name = "PathMultiline", extends = CurveElement)]
pub struct PathMultiline {
    #[qml(base)]
    pub curve: CurveElement,
    /// The first point of the first line.
    #[qml(notify, readonly)]
    pub start: ljos_engine::Point,
    /// The lines: a list of lists of points.
    #[qml(notify)]
    pub paths: Value,
}

impl Default for PathMultiline {
    fn default() -> Self {
        Self {
            curve: CurveElement::default(),
            start: ljos_engine::Point::default(),
            paths: Value::list(Vec::new()),
        }
    }
}

#[qml_impl]
impl PathMultiline {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let obj = cx.this;
        cx.engine
            .bind_initial(obj, PathMultiline::START.index, move |e| {
                let lines = lines_of(e, &e.read(obj, PathMultiline::PATHS.index));
                first_point(lines.first().and_then(|l| l.first()))
            })
    }
}

fn first_point(p: Option<&Point>) -> Value {
    let p = p.copied().unwrap_or(Point::ZERO);
    Value::point(p.x, p.y)
}

/// `PathRectangle`: a rectangle whose corners may be round or cut straight.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick", name = "PathRectangle", extends = CurveElement)]
pub struct PathRectangle {
    #[qml(base)]
    pub curve: CurveElement,
    #[qml(notify)]
    pub width: f64,
    #[qml(notify)]
    pub height: f64,
    /// How wide the stroke is that should stay inside `width` by `height`:
    /// the outline is drawn half of it further in on every side.
    #[qml(notify)]
    pub stroke_adjustment: f64,
    #[qml(notify)]
    pub radius: f64,
    #[qml(notify, reset = ljos_engine::Reset::Follow(common_radius))]
    pub top_left_radius: f64,
    #[qml(notify, reset = ljos_engine::Reset::Follow(common_radius))]
    pub top_right_radius: f64,
    #[qml(notify, reset = ljos_engine::Reset::Follow(common_radius))]
    pub bottom_left_radius: f64,
    #[qml(notify, reset = ljos_engine::Reset::Follow(common_radius))]
    pub bottom_right_radius: f64,
    #[qml(notify)]
    pub bevel: bool,
    #[qml(notify, reset = ljos_engine::Reset::Follow(common_bevel))]
    pub top_left_bevel: bool,
    #[qml(notify, reset = ljos_engine::Reset::Follow(common_bevel))]
    pub top_right_bevel: bool,
    #[qml(notify, reset = ljos_engine::Reset::Follow(common_bevel))]
    pub bottom_left_bevel: bool,
    #[qml(notify, reset = ljos_engine::Reset::Follow(common_bevel))]
    pub bottom_right_bevel: bool,
}

fn common_radius(engine: &Engine, obj: ObjectId, _corner: PropIndex) -> Value {
    engine.read(obj, PathRectangle::RADIUS.index)
}

fn common_bevel(engine: &Engine, obj: ObjectId, _corner: PropIndex) -> Value {
    engine.read(obj, PathRectangle::BEVEL.index)
}

/// Top left, top right, bottom right, bottom left: the order
/// [`Builder::rectangle`] takes the corners in.
fn corner_radii() -> [PropIndex; 4] {
    [
        PathRectangle::TOP_LEFT_RADIUS.index,
        PathRectangle::TOP_RIGHT_RADIUS.index,
        PathRectangle::BOTTOM_RIGHT_RADIUS.index,
        PathRectangle::BOTTOM_LEFT_RADIUS.index,
    ]
}

fn corner_bevels() -> [PropIndex; 4] {
    [
        PathRectangle::TOP_LEFT_BEVEL.index,
        PathRectangle::TOP_RIGHT_BEVEL.index,
        PathRectangle::BOTTOM_RIGHT_BEVEL.index,
        PathRectangle::BOTTOM_LEFT_BEVEL.index,
    ]
}

#[qml_impl]
impl PathRectangle {
    /// A corner without a value of its own reads the common one, so that
    /// QML sees what is drawn.
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (engine, obj) = (cx.engine, cx.this);
        let follow = |corners: [PropIndex; 4], common: PropIndex| -> Result<(), EngineError> {
            for corner in corners {
                if is_unset(engine, obj, corner) {
                    engine.bind(obj, corner, move |e| e.read(obj, common))?;
                }
            }
            Ok(())
        };
        follow(corner_radii(), PathRectangle::RADIUS.index)?;
        follow(corner_bevels(), PathRectangle::BEVEL.index)
    }
}

/// Which element an object is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Move,
    Line,
    Quad,
    Cubic,
    Arc,
    AngleArc,
    Curve,
    Svg,
    Polyline,
    Multiline,
    Rectangle,
}

impl Kind {
    fn of(engine: &Engine, obj: ObjectId) -> Option<Kind> {
        let meta = engine.meta_of(obj)?;
        let is = |m: &'static ljos_engine::MetaObject| meta.inherits(m);
        Some(if is(PathLine::static_meta()) {
            Kind::Line
        } else if is(PathMove::static_meta()) {
            Kind::Move
        } else if is(PathQuad::static_meta()) {
            Kind::Quad
        } else if is(PathCubic::static_meta()) {
            Kind::Cubic
        } else if is(PathArc::static_meta()) {
            Kind::Arc
        } else if is(PathAngleArc::static_meta()) {
            Kind::AngleArc
        } else if is(PathCurve::static_meta()) {
            Kind::Curve
        } else if is(PathSvg::static_meta()) {
            Kind::Svg
        } else if is(PathPolyline::static_meta()) {
            Kind::Polyline
        } else if is(PathMultiline::static_meta()) {
            Kind::Multiline
        } else if is(PathRectangle::static_meta()) {
            Kind::Rectangle
        } else {
            return None;
        })
    }
}

/// A coordinate of where an element goes: counted from `from` when the
/// relative one was given a value, whatever the value.
fn along(engine: &Engine, el: ObjectId, abs: PropIndex, rel: PropIndex, from: f64) -> f64 {
    if is_unset_tracked(engine, el, rel) {
        num(engine, el, abs)
    } else {
        from + num(engine, el, rel)
    }
}

fn pair(engine: &Engine, el: ObjectId, x: [PropIndex; 2], y: [PropIndex; 2], from: Point) -> Point {
    Point::new(
        along(engine, el, x[0], x[1], from.x),
        along(engine, el, y[0], y[1], from.y),
    )
}

/// Where an element that starts at `from` ends.
fn target(engine: &Engine, el: ObjectId, from: Point) -> Point {
    pair(
        engine,
        el,
        [CurveElement::X.index, CurveElement::RELATIVE_X.index],
        [CurveElement::Y.index, CurveElement::RELATIVE_Y.index],
        from,
    )
}

/// A point as QML may write it: a `point`, or two numbers.
fn point_of(engine: &Engine, v: &Value) -> Option<Point> {
    match engine.plain(v) {
        Value::Point(p) => Some(Point::new(p.x, p.y)),
        Value::List(l) if l.len() == 2 => Some(Point::new(l[0].as_number()?, l[1].as_number()?)),
        _ => None,
    }
}

fn points_of(engine: &Engine, v: &Value) -> Vec<Point> {
    match engine.plain(v) {
        Value::List(l) => l.iter().filter_map(|p| point_of(engine, p)).collect(),
        _ => Vec::new(),
    }
}

/// The lines of a `PathMultiline`: a list of lists of points. A list of
/// points alone is one line.
fn lines_of(engine: &Engine, v: &Value) -> Vec<Vec<Point>> {
    let Value::List(entries) = engine.plain(v) else {
        return Vec::new();
    };
    if !entries.is_empty() && entries.iter().all(|e| point_of(engine, e).is_some()) {
        return vec![points_of(engine, v)];
    }
    entries.iter().map(|e| points_of(engine, e)).collect()
}

/// How far a `PathCurve`'s control points lie from its ends, as a part of
/// the way between the points on either side. A sixth makes the curve a
/// Catmull-Rom spline; Qt's figure is this one, measured on qml6 from where
/// its curves go (a sixth is off by four thousandths over fifty pixels).
const CURVE_PULL: f64 = 0.167;

/// A path laid out: the outline before `Path.scale`, where every element
/// started and ended, and what there was to complain about.
struct Laid {
    builder: Builder,
    spans: Vec<(Point, Point)>,
    warnings: Vec<String>,
}

/// Draws the elements from `start`. `wrap` is for a ring of `PathCurve`s:
/// where the last element starts and where the first ends, which the ends
/// of the curve bend towards.
fn lay(
    engine: &Engine,
    start: Point,
    elements: &[(ObjectId, Kind)],
    wrap: Option<(Point, Point)>,
) -> Laid {
    let mut b = Builder::new(start);
    let mut spans: Vec<(Point, Point)> = Vec::with_capacity(elements.len());
    let mut warnings = Vec::new();
    let n = |el, p: ljos_engine::Prop<f64>| num(engine, el, p.index);
    for (i, &(el, kind)) in elements.iter().enumerate() {
        let from = b.cur;
        match kind {
            Kind::Move => b.move_to(target(engine, el, from)),
            Kind::Line => b.line_to(target(engine, el, from)),
            Kind::Quad => {
                let control = pair(
                    engine,
                    el,
                    [
                        PathQuad::CONTROL_X.index,
                        PathQuad::RELATIVE_CONTROL_X.index,
                    ],
                    [
                        PathQuad::CONTROL_Y.index,
                        PathQuad::RELATIVE_CONTROL_Y.index,
                    ],
                    from,
                );
                b.quad_to(control, target(engine, el, from));
            }
            Kind::Cubic => {
                let c1 = pair(
                    engine,
                    el,
                    [
                        PathCubic::CONTROL1_X.index,
                        PathCubic::RELATIVE_CONTROL1_X.index,
                    ],
                    [
                        PathCubic::CONTROL1_Y.index,
                        PathCubic::RELATIVE_CONTROL1_Y.index,
                    ],
                    from,
                );
                let c2 = pair(
                    engine,
                    el,
                    [
                        PathCubic::CONTROL2_X.index,
                        PathCubic::RELATIVE_CONTROL2_X.index,
                    ],
                    [
                        PathCubic::CONTROL2_Y.index,
                        PathCubic::RELATIVE_CONTROL2_Y.index,
                    ],
                    from,
                );
                b.curve_to(c1, c2, target(engine, el, from));
            }
            Kind::Arc => {
                let radii = Vec2::new(n(el, PathArc::RADIUS_X), n(el, PathArc::RADIUS_Y));
                let large = engine.read(el, PathArc::USE_LARGE_ARC.index).to_boolean();
                let clockwise =
                    engine.read(el, PathArc::DIRECTION.index).to_number() as i32 == CLOCKWISE;
                b.arc_to(
                    radii,
                    n(el, PathArc::X_AXIS_ROTATION),
                    large,
                    clockwise,
                    target(engine, el, from),
                );
            }
            Kind::AngleArc => b.angle_arc(
                Point::new(n(el, PathAngleArc::CENTER_X), n(el, PathAngleArc::CENTER_Y)),
                Vec2::new(n(el, PathAngleArc::RADIUS_X), n(el, PathAngleArc::RADIUS_Y)),
                n(el, PathAngleArc::START_ANGLE),
                n(el, PathAngleArc::SWEEP_ANGLE),
                engine
                    .read(el, PathAngleArc::MOVE_TO_START.index)
                    .to_boolean(),
            ),
            Kind::Curve => {
                let to = target(engine, el, from);
                let curve_at = |at: usize| elements.get(at).is_some_and(|e| e.1 == Kind::Curve);
                // The point before: where the curve came from.
                let before = if i > 0 {
                    if curve_at(i - 1) {
                        spans[i - 1].0
                    } else {
                        from
                    }
                } else {
                    wrap.map_or(from, |w| w.0)
                };
                // The point after: where the curve goes on to.
                let after = if i + 1 < elements.len() {
                    if curve_at(i + 1) {
                        target(engine, elements[i + 1].0, to)
                    } else {
                        to
                    }
                } else {
                    wrap.map_or(to, |w| w.1)
                };
                b.curve_to(
                    from + (to - before) * CURVE_PULL,
                    to - (after - from) * CURVE_PULL,
                    to,
                );
            }
            Kind::Svg => {
                let data = engine.read(el, PathSvg::PATH.index).to_js_string();
                let (drawn, mistake) = svg::parse(data.as_str());
                for part in drawn {
                    match part {
                        SvgEl::Move(p) => b.move_to(p),
                        SvgEl::Line(p) => b.line_to(p),
                        SvgEl::Quad(c, p) => b.quad_to(c, p),
                        SvgEl::Cubic(c1, c2, p) => b.curve_to(c1, c2, p),
                        SvgEl::Arc {
                            radii,
                            rotation,
                            large,
                            sweep,
                            to,
                        } => b.arc_to(radii, rotation, large, sweep, to),
                        SvgEl::Close => b.close(),
                    }
                }
                if let Some(mistake) = mistake {
                    warnings.push(format!("PathSvg: {}", mistake.message));
                }
            }
            Kind::Polyline => {
                b.polyline(&points_of(
                    engine,
                    &engine.read(el, PathPolyline::PATH.index),
                ));
            }
            Kind::Multiline => {
                for line in lines_of(engine, &engine.read(el, PathMultiline::PATHS.index)) {
                    b.polyline(&line);
                }
            }
            Kind::Rectangle => {
                let at = target(engine, el, from);
                let inset = n(el, PathRectangle::STROKE_ADJUSTMENT) / 2.0;
                let rect = Rect::new(
                    at.x + inset,
                    at.y + inset,
                    at.x + n(el, PathRectangle::WIDTH) - inset,
                    at.y + n(el, PathRectangle::HEIGHT) - inset,
                );
                // A corner's own value once it has one, else the common
                // one: an element that was not completed has no binding
                // that makes them the same.
                let common_radius = n(el, PathRectangle::RADIUS);
                let radii = corner_radii().map(|corner| {
                    let own = num(engine, el, corner);
                    if is_unset_tracked(engine, el, corner) {
                        common_radius
                    } else {
                        own
                    }
                });
                let common_bevel = engine.read(el, PathRectangle::BEVEL.index).to_boolean();
                let bevels = corner_bevels().map(|corner| {
                    let own = engine.read(el, corner).to_boolean();
                    if is_unset_tracked(engine, el, corner) {
                        common_bevel
                    } else {
                        own
                    }
                });
                // `Rect::new` puts the sides in order, which would turn a
                // rectangle of less than no size inside out instead of
                // leaving it out.
                if n(el, PathRectangle::WIDTH) - 2.0 * inset > 0.0
                    && n(el, PathRectangle::HEIGHT) - 2.0 * inset > 0.0
                {
                    b.rectangle(rect, radii, bevels);
                }
            }
        }
        spans.push((from, b.cur));
    }
    Laid {
        builder: b,
        spans,
        warnings,
    }
}

fn build(engine: &Engine, path: ObjectId) -> PathGeometry {
    let start = Point::new(
        num(engine, path, Path::START_X.index),
        num(engine, path, Path::START_Y.index),
    );
    let elements: Vec<(ObjectId, Kind)> =
        crate::tree::list(engine, path, Path::PATH_ELEMENTS.index)
            .into_iter()
            .filter_map(|el| Some((el, Kind::of(engine, el)?)))
            .collect();
    let mut laid = lay(engine, start, &elements, None);
    // Curves that come back to where the path started are a ring: the
    // first bends as if the last came before it, and the last as if the
    // first came after.
    let ring = elements.len() > 1
        && elements.first().is_some_and(|e| e.1 == Kind::Curve)
        && elements.last().is_some_and(|e| e.1 == Kind::Curve)
        && laid.builder.cur == start;
    if ring {
        let wrap = (laid.spans[laid.spans.len() - 1].0, laid.spans[0].1);
        laid = lay(engine, start, &elements, Some(wrap));
    }
    for warning in &laid.warnings {
        engine.log(LogLevel::Warning, "qml", warning.clone());
    }
    let closed = !elements.is_empty() && laid.builder.cur == start;
    let mut outline = laid.builder.finish();
    let scale = match engine.read(path, Path::SCALE.index) {
        Value::Size(s) => (s.width, s.height),
        _ => (1.0, 1.0),
    };
    if scale != (1.0, 1.0) && scale.0.is_finite() && scale.1.is_finite() {
        outline.apply_affine(Affine::scale_non_uniform(scale.0, scale.1));
    }
    PathGeometry {
        bounds: bounds_of(&outline),
        path: Arc::new(outline),
        closed,
    }
}
