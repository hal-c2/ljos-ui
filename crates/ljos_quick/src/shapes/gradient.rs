//! `LinearGradient`, `RadialGradient` and `ConicalGradient`: what a
//! `ShapePath` is filled with instead of one colour.
//!
//! They are `Gradient`s, so the stops are read by the rules a Rectangle's
//! gradient has ([`GradientPaint`]): painted in the order of their
//! positions, the first written of several at one position kept. What is
//! their own, checked on qml6:
//!
//! - the coordinates are the Shape's, whatever `ShapePath.scale` is;
//! - a stop outside 0 to 1 is not painted and is warned about;
//! - a gradient without stops runs from black to white.

use std::sync::Arc;

use ljos_engine::{Color, Cx, Engine, EngineError, LogLevel, ObjectId, QmlObject, qml_impl};
use ljos_render::kurbo::Point;
use ljos_render::{Brush, BrushGradient, GradientGeometry, GradientStop as RenderStop, Spread};

use crate::enums::EnumTable;
use crate::gradient::{Gradient, GradientPaint};
use crate::item::{is_a, num};
use crate::rectangle::to_rgba;

/// `ShapeGradient.PadSpread`, the default: the end colours go on.
pub const PAD_SPREAD: i32 = 0;
/// `ShapeGradient.ReflectSpread`: the gradient runs back and forth.
pub const REFLECT_SPREAD: i32 = 1;
/// `ShapeGradient.RepeatSpread`: the gradient starts again.
pub const REPEAT_SPREAD: i32 = 2;

pub(crate) fn spread_enums() -> EnumTable {
    vec![(
        "SpreadMode",
        vec![
            ("PadSpread", i64::from(PAD_SPREAD)),
            ("ReflectSpread", i64::from(REFLECT_SPREAD)),
            ("RepeatSpread", i64::from(REPEAT_SPREAD)),
        ],
    )]
}

/// `ShapeGradient` (QtQuick.Shapes): what the three gradients share.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtQuick.Shapes",
    name = "ShapeGradient",
    extends = Gradient,
    uncreatable,
    enums = spread_enums
)]
pub struct ShapeGradient {
    #[qml(base)]
    pub gradient: Gradient,
    /// What is painted outside the gradient's own extent.
    #[qml(notify, enumeration = "SpreadMode")]
    pub spread: i32,
}

#[qml_impl]
impl ShapeGradient {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let obj = cx.this;
        // Here and not where the gradient is painted: once for each time
        // the stops change, however often they are painted.
        cx.engine.effect_in(obj, move |e| {
            let outside = GradientPaint::of(e, obj)
                .is_some_and(|p| p.stops.iter().any(|s| !(0.0..=1.0).contains(&s.0)));
            if outside {
                e.log(
                    LogLevel::Warning,
                    "qml",
                    "QGradient::setColorAt: Color position must be specified in the range 0 to 1",
                );
            }
        })?;
        Ok(())
    }
}

/// `LinearGradient` (QtQuick.Shapes): along the line from (`x1`, `y1`) to
/// (`x2`, `y2`).
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick.Shapes", name = "LinearGradient", extends = ShapeGradient)]
pub struct LinearGradient {
    #[qml(base)]
    pub gradient: ShapeGradient,
    #[qml(notify)]
    pub x1: f64,
    #[qml(notify)]
    pub y1: f64,
    #[qml(notify)]
    pub x2: f64,
    #[qml(notify)]
    pub y2: f64,
}

/// `RadialGradient` (QtQuick.Shapes): from the circle of `focalRadius`
/// round the focal point to the circle of `centerRadius` round the centre.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick.Shapes", name = "RadialGradient", extends = ShapeGradient)]
pub struct RadialGradient {
    #[qml(base)]
    pub gradient: ShapeGradient,
    #[qml(notify)]
    pub center_x: f64,
    #[qml(notify)]
    pub center_y: f64,
    #[qml(notify)]
    pub center_radius: f64,
    #[qml(notify)]
    pub focal_x: f64,
    #[qml(notify)]
    pub focal_y: f64,
    #[qml(notify)]
    pub focal_radius: f64,
}

/// `ConicalGradient` (QtQuick.Shapes): round the centre, starting `angle`
/// degrees anticlockwise from three o'clock and going on anticlockwise.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick.Shapes", name = "ConicalGradient", extends = ShapeGradient)]
pub struct ConicalGradient {
    #[qml(base)]
    pub gradient: ShapeGradient,
    #[qml(notify)]
    pub center_x: f64,
    #[qml(notify)]
    pub center_y: f64,
    #[qml(notify)]
    pub angle: f64,
}

/// What `gradient` fills a shape with (tracked). `None` for what is not
/// one of the three gradients.
pub(crate) fn brush(engine: &Engine, gradient: ObjectId) -> Option<Brush> {
    let n = |p: ljos_engine::Prop<f64>| num(engine, gradient, p.index);
    let geometry = if is_a::<LinearGradient>(engine, gradient) {
        GradientGeometry::Linear {
            start: Point::new(n(LinearGradient::X1), n(LinearGradient::Y1)),
            end: Point::new(n(LinearGradient::X2), n(LinearGradient::Y2)),
        }
    } else if is_a::<RadialGradient>(engine, gradient) {
        GradientGeometry::Radial {
            focal: Point::new(n(RadialGradient::FOCAL_X), n(RadialGradient::FOCAL_Y)),
            focal_radius: n(RadialGradient::FOCAL_RADIUS),
            center: Point::new(n(RadialGradient::CENTER_X), n(RadialGradient::CENTER_Y)),
            radius: n(RadialGradient::CENTER_RADIUS),
        }
    } else if is_a::<ConicalGradient>(engine, gradient) {
        GradientGeometry::Conical {
            center: Point::new(n(ConicalGradient::CENTER_X), n(ConicalGradient::CENTER_Y)),
            angle: n(ConicalGradient::ANGLE),
        }
    } else {
        return None;
    };
    let spread = match num(engine, gradient, ShapeGradient::SPREAD.index) as i32 {
        REFLECT_SPREAD => Spread::Reflect,
        REPEAT_SPREAD => Spread::Repeat,
        _ => Spread::Pad,
    };
    let stop = |(position, color): (f64, Color)| RenderStop {
        offset: position as f32,
        color: to_rgba(color),
    };
    let mut stops: Vec<RenderStop> = GradientPaint::of(engine, gradient)
        .map(|p| p.stops)
        .unwrap_or_default()
        .into_iter()
        .filter(|(position, _)| (0.0..=1.0).contains(position))
        .map(stop)
        .collect();
    if stops.is_empty() {
        stops = vec![stop((0.0, Color::BLACK)), stop((1.0, Color::WHITE))];
    }
    Some(Brush::Gradient(Arc::new(BrushGradient::new(
        geometry, stops, spread,
    ))))
}
