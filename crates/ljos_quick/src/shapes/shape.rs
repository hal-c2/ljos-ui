//! `Shape`: an item that paints the `ShapePath`s declared in it.
//!
//! Qt's rules, each checked on qml6:
//!
//! - the paths are painted in the order they were written, under the
//!   Shape's child items, and are not cut off at the Shape's size;
//! - `boundingRect` is where the paths lie, each grown by what its stroke
//!   is taken to reach;
//! - the implicit size reaches from the Shape's origin to the far corner
//!   of `boundingRect`;
//! - with `Shape.FillContains`, `contains()` and the pointer take a point
//!   only where a path encloses it.

use ljos_engine::{Cx, Engine, EngineError, ObjectId, Point, QmlObject, Rect, Value, qml_impl};
use ljos_render::kurbo;
use ljos_render::{DisplayListBuilder, ShapeStyle};

use super::builder::encloses;
use super::path::geometry;
use super::shape_path::{PathPaint, ShapePath, margin};
use crate::enums::EnumTable;
use crate::item::{Item, is_a, num, write_computed};

/// `Shape.Null`: nothing to paint yet.
pub const STATUS_NULL: i32 = 0;
/// `Shape.Ready`: the paths have been worked out.
pub const STATUS_READY: i32 = 1;
/// `Shape.Processing`. Never reported: paths are worked out at once.
pub const STATUS_PROCESSING: i32 = 2;

/// `Shape.BoundingRectContains`, the default.
pub const BOUNDING_RECT_CONTAINS: i32 = 0;
/// `Shape.FillContains`.
pub const FILL_CONTAINS: i32 = 1;

/// `Shape.GeometryRenderer`.
pub const GEOMETRY_RENDERER: i32 = 1;

pub(crate) fn shape_enums() -> EnumTable {
    vec![
        (
            "RendererType",
            vec![
                ("UnknownRenderer", 0),
                ("GeometryRenderer", i64::from(GEOMETRY_RENDERER)),
                ("NvprRenderer", 2),
                ("SoftwareRenderer", 3),
                ("CurveRenderer", 4),
            ],
        ),
        (
            "Status",
            vec![
                ("Null", i64::from(STATUS_NULL)),
                ("Ready", i64::from(STATUS_READY)),
                ("Processing", i64::from(STATUS_PROCESSING)),
            ],
        ),
        (
            "ContainsMode",
            vec![
                ("BoundingRectContains", i64::from(BOUNDING_RECT_CONTAINS)),
                ("FillContains", i64::from(FILL_CONTAINS)),
            ],
        ),
    ]
}

/// `Shape` (QtQuick.Shapes).
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick.Shapes", name = "Shape", extends = Item, enums = shape_enums)]
pub struct Shape {
    #[qml(base)]
    pub item: Item,
    /// Which of Qt's renderers paints the Shape. None of them does: the
    /// render backend chooses how, and this says the one whose results are
    /// the nearest, so that code which asks does not take the Shape to be
    /// unpainted.
    #[qml(notify, readonly, enumeration = "RendererType")]
    pub renderer_type: i32,
    /// Accepted; paths are worked out when they are needed.
    #[qml(notify)]
    pub asynchronous: bool,
    /// Accepted; there are no vendor extensions.
    #[qml(notify)]
    pub vendor_extensions_enabled: bool,
    /// Accepted; the render backend chooses.
    #[qml(notify, enumeration = "RendererType")]
    pub preferred_renderer_type: i32,
    #[qml(notify, readonly, enumeration = "Status")]
    pub status: i32,
    #[qml(notify, enumeration = "ContainsMode")]
    pub contains_mode: i32,
    /// Where the paths lie, strokes counted.
    #[qml(notify, readonly)]
    pub bounding_rect: Rect,
}

impl Default for Shape {
    fn default() -> Self {
        Self {
            item: Item::default(),
            renderer_type: GEOMETRY_RENDERER,
            asynchronous: false,
            vendor_extensions_enabled: true,
            preferred_renderer_type: 0,
            status: STATUS_NULL,
            contains_mode: BOUNDING_RECT_CONTAINS,
            bounding_rect: Rect::default(),
        }
    }
}

#[qml_impl]
impl Shape {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (engine, obj) = (cx.engine, cx.this);
        engine.bind_initial(obj, Shape::BOUNDING_RECT.index, move |e| {
            let r = bounding_rect(e, obj);
            Value::rect(r.x, r.y, r.width, r.height)
        })?;
        // Runs when a path changes and at no other time: the implicit size
        // is what a layout around the Shape reads.
        engine.effect_in(obj, move |e| {
            let (right, bottom) = match e.read(obj, Shape::BOUNDING_RECT.index) {
                Value::Rect(r) => (r.x + r.width, r.y + r.height),
                _ => (0.0, 0.0),
            };
            let status = if shape_paths(e, obj).is_empty() {
                STATUS_NULL
            } else {
                STATUS_READY
            };
            let _ = write_computed(e, obj, Item::IMPLICIT_WIDTH.index, Value::Real(right));
            let _ = write_computed(e, obj, Item::IMPLICIT_HEIGHT.index, Value::Real(bottom));
            let _ = write_computed(e, obj, Shape::STATUS.index, Value::Int(status));
        })?;
        Ok(())
    }

    /// `contains(point)`: whether a point in the Shape's coordinates is
    /// inside it, by `containsMode`.
    #[qml(invokable)]
    fn contains(cx: &Cx, point: Point) -> bool {
        let (engine, obj) = (cx.engine, cx.this);
        if fills_contain(engine, obj) {
            return fill_holds(engine, obj, kurbo::Point::new(point.x, point.y));
        }
        let width = num(engine, obj, Item::WIDTH.index);
        let height = num(engine, obj, Item::HEIGHT.index);
        point.x >= 0.0 && point.y >= 0.0 && point.x < width && point.y < height
    }
}

/// The `ShapePath`s of a Shape in the order they are painted (tracked).
fn shape_paths(engine: &Engine, shape: ObjectId) -> Vec<ObjectId> {
    crate::tree::resources(engine, shape)
        .into_iter()
        .filter(|o| is_a::<ShapePath>(engine, *o))
        .collect()
}

/// Two rectangles as one, the way Qt joins them: one of no size at all is
/// left out, wherever it is, unless there is nothing else.
fn united(a: Rect, b: Rect) -> Rect {
    let null = |r: &Rect| r.width == 0.0 && r.height == 0.0;
    if null(&a) {
        return b;
    }
    if null(&b) {
        return a;
    }
    let span = |at: f64, len: f64| (at.min(at + len), at.max(at + len));
    let (ax, bx) = (span(a.x, a.width), span(b.x, b.width));
    let (ay, by) = (span(a.y, a.height), span(b.y, b.height));
    let (x0, x1) = (ax.0.min(bx.0), ax.1.max(bx.1));
    let (y0, y1) = (ay.0.min(by.0), ay.1.max(by.1));
    Rect {
        x: x0,
        y: y0,
        width: x1 - x0,
        height: y1 - y0,
    }
}

fn bounding_rect(engine: &Engine, shape: ObjectId) -> Rect {
    let mut all = Rect::default();
    for path in shape_paths(engine, shape) {
        let Some(g) = geometry(engine, path) else {
            continue;
        };
        let m = margin(engine, path);
        all = united(
            all,
            Rect {
                x: g.bounds.x0 - m,
                y: g.bounds.y0 - m,
                width: g.bounds.width() + 2.0 * m,
                height: g.bounds.height() + 2.0 * m,
            },
        );
    }
    all
}

fn fills_contain(engine: &Engine, shape: ObjectId) -> bool {
    num(engine, shape, Shape::CONTAINS_MODE.index) as i32 == FILL_CONTAINS
}

/// Whether one of the Shape's paths encloses `p`, whatever it is filled
/// with (tracked).
fn fill_holds(engine: &Engine, shape: ObjectId, p: kurbo::Point) -> bool {
    shape_paths(engine, shape)
        .into_iter()
        .filter_map(|path| geometry(engine, path))
        .any(|g| encloses(&g.path, p))
}

/// Whether the pointer at `local` (in `obj`'s coordinates, asked for only
/// when it matters) is for `obj`: not when `obj` is a Shape that takes
/// points by its fill and no path encloses this one. What lies behind the
/// Shape gets the pointer then.
pub(crate) fn takes_pointer(
    engine: &Engine,
    obj: ObjectId,
    local: impl FnOnce() -> kurbo::Point,
) -> bool {
    if !is_a::<Shape>(engine, obj) {
        return true;
    }
    engine.untrack(|| !fills_contain(engine, obj) || fill_holds(engine, obj, local()))
}

/// What a Shape paints. Built by the scene's content effect.
#[derive(Clone, Debug, Default)]
pub struct ShapePaint {
    paths: Vec<PathPaint>,
}

impl ShapePaint {
    /// Reads the paths and how each is filled and outlined (tracked).
    pub fn read(engine: &Engine, obj: ObjectId) -> Self {
        Self {
            paths: shape_paths(engine, obj)
                .into_iter()
                .filter_map(|path| PathPaint::read(engine, path))
                .collect(),
        }
    }

    /// The paths in the Shape's coordinates. They are not cut off at the
    /// Shape's `size`.
    pub fn paint(&self, _size: kurbo::Size, out: &mut DisplayListBuilder) {
        for p in &self.paths {
            if let Some((rule, brush)) = &p.fill {
                out.shape(p.path.clone(), ShapeStyle::Fill(*rule), brush.clone());
            }
            if let Some((stroke, brush)) = &p.stroke {
                out.shape(
                    p.path.clone(),
                    ShapeStyle::Stroke(stroke.clone()),
                    brush.clone(),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(x: f64, y: f64, width: f64, height: f64) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn a_rectangle_of_no_size_is_left_out() {
        let a = r(10.0, 20.0, 100.0, 50.0);
        assert_eq!(united(a, r(500.0, 500.0, 0.0, 0.0)), a);
        assert_eq!(united(r(0.0, 0.0, 0.0, 0.0), a), a);
    }

    #[test]
    fn a_point_alone_is_where_it_is() {
        let point = r(3.0, 4.0, 0.0, 0.0);
        assert_eq!(united(Rect::default(), point), point);
    }

    #[test]
    fn rectangles_are_joined_by_their_far_sides() {
        assert_eq!(
            united(r(10.0, 20.0, 100.0, 50.0), r(0.0, 60.0, 20.0, 40.0)),
            r(0.0, 20.0, 110.0, 80.0)
        );
        // A line has no height and still counts.
        assert_eq!(
            united(r(10.0, 20.0, 100.0, 50.0), r(0.0, 0.0, 200.0, 0.0)),
            r(0.0, 0.0, 200.0, 70.0)
        );
    }

    #[test]
    fn a_rectangle_turned_inside_out_is_read_the_right_way_round() {
        assert_eq!(
            united(r(10.0, 10.0, 10.0, 10.0), r(40.0, 40.0, -5.0, -5.0)),
            r(10.0, 10.0, 30.0, 30.0)
        );
    }
}
