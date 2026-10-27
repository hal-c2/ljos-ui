//! `QtPositioning`, the singleton
//! (doc.qt.io/qt-6/qml-qtpositioning-qtpositioning.html): what makes the
//! value types and turns a shape back into the type it holds.

use std::sync::Arc;

use ljos::prelude::*;

use crate::geo::{self, GeoCoordinate};
use crate::values::{GeoCircle, GeoPath, GeoPolygon, GeoRectangle, GeoValue, Shape, number_or};

/// `QtPositioning` (QtPositioning).
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtPositioning", name = "QtPositioning")]
#[qml_singleton]
pub struct QtPositioning {}

/// A script's array as the coordinates in it.
fn coordinates(cx: &Cx, v: &Value) -> Vec<GeoCoordinate> {
    crate::values::coordinates_of(&cx.engine.plain(v))
}

fn coordinate(v: &Value) -> Option<GeoCoordinate> {
    GeoCoordinate::from_any(v)
}

#[qml_impl]
impl QtPositioning {
    /// Nowhere when the latitude or longitude is left out.
    #[qml(invokable)]
    fn coordinate(_cx: &Cx, latitude: Value, longitude: Value, altitude: Value) -> Value {
        GeoCoordinate::new(
            number_or(&latitude, f64::NAN),
            number_or(&longitude, f64::NAN),
            number_or(&altitude, f64::NAN),
        )
        .value()
    }

    /// From two corners, around a centre (`center, width, height`), or
    /// around a list of coordinates.
    #[qml(invokable)]
    fn rectangle(cx: &Cx, a: Value, b: Value, c: Value) -> Value {
        let list = cx.engine.plain(&a);
        if list.as_list().is_some() {
            return GeoRectangle::bounding(&crate::values::coordinates_of(&list)).value();
        }
        match (coordinate(&a), coordinate(&b)) {
            (Some(top_left), Some(bottom_right)) => {
                GeoRectangle::new(top_left, bottom_right).value()
            }
            (Some(center), None) if !matches!(b, Value::Undefined) => {
                GeoRectangle::around(center, b.to_number(), number_or(&c, 0.0)).value()
            }
            _ => GeoRectangle::default().value(),
        }
    }

    #[qml(invokable)]
    fn circle(_cx: &Cx, center: Value, radius: Value) -> Value {
        GeoCircle {
            center: coordinate(&center).unwrap_or_default(),
            radius: number_or(&radius, -1.0),
        }
        .value()
    }

    #[qml(invokable)]
    fn path(cx: &Cx, path: Value, width: Value) -> Value {
        GeoPath {
            path: coordinates(cx, &path),
            width: number_or(&width, 0.0),
        }
        .value()
    }

    /// The holes a polygon may have are not kept.
    #[qml(invokable)]
    fn polygon(cx: &Cx, perimeter: Value) -> Value {
        GeoPolygon {
            perimeter: coordinates(cx, &perimeter),
        }
        .value()
    }

    #[qml(invokable)]
    fn shape(_cx: &Cx) -> Value {
        Shape::Unknown.value()
    }

    #[qml(invokable, name = "shapeToRectangle")]
    fn shape_to_rectangle(_cx: &Cx, shape: Value) -> Value {
        match Shape::of(&shape) {
            Some(Shape::Rectangle(r)) => r.value(),
            _ => GeoRectangle::default().value(),
        }
    }

    #[qml(invokable, name = "shapeToCircle")]
    fn shape_to_circle(_cx: &Cx, shape: Value) -> Value {
        match Shape::of(&shape) {
            Some(Shape::Circle(c)) => c.value(),
            _ => GeoCircle::default().value(),
        }
    }

    #[qml(invokable, name = "shapeToPath")]
    fn shape_to_path(_cx: &Cx, shape: Value) -> Value {
        match Shape::of(&shape) {
            Some(Shape::Path(p)) => p.value(),
            _ => GeoPath::default().value(),
        }
    }

    #[qml(invokable, name = "shapeToPolygon")]
    fn shape_to_polygon(_cx: &Cx, shape: Value) -> Value {
        match Shape::of(&shape) {
            Some(Shape::Polygon(p)) => p.value(),
            _ => GeoPolygon::default().value(),
        }
    }

    /// Web Mercator, from 0 to 1 each way.
    #[qml(invokable, name = "coordToMercator")]
    fn coord_to_mercator(_cx: &Cx, coordinate: Value) -> Value {
        let (x, y) = geo::to_mercator(&crate::values::coordinate_of(&coordinate));
        Value::Point(Arc::new(Point { x, y }))
    }

    #[qml(invokable, name = "mercatorToCoord")]
    fn mercator_to_coord(_cx: &Cx, point: Value) -> Value {
        let (x, y) = match &point {
            Value::Point(p) => (p.x, p.y),
            _ => (f64::NAN, f64::NAN),
        };
        geo::from_mercator(x, y).value()
    }
}
