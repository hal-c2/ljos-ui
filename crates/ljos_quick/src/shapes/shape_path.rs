//! `ShapePath`: a `Path` with how it is filled and outlined.
//!
//! [`PathPaint`] is what one comes to when its Shape is painted. Qt's
//! rules, each checked on qml6:
//!
//! - the fill comes first and the outline over it;
//! - a path that is not closed is filled as if it were, and not outlined
//!   along the closing line;
//! - a stroke of no width, or less, is not drawn;
//! - a dash pattern and its offset count in stroke widths, and only
//!   `ShapePath.DashLine` uses them;
//! - `fillGradient` wins over `fillColor`.

use std::sync::Arc;

use ljos_engine::{Color, Cx, Engine, EngineError, LogLevel, ObjectId, QmlObject, Value, qml_impl};
use ljos_render::kurbo::{BezPath, Cap, Join, Stroke};
use ljos_render::{Brush, FillRule};

use super::path::{Path, PathGeometry, geometry};
use crate::enums::EnumTable;
use crate::item::num;
use crate::rectangle::to_rgba;

/// `ShapePath.OddEvenFill`, the default.
pub const ODD_EVEN_FILL: i32 = 0;
/// `ShapePath.WindingFill`.
pub const WINDING_FILL: i32 = 1;

/// `ShapePath.MiterJoin`.
pub const MITER_JOIN: i32 = 0;
/// `ShapePath.BevelJoin`, the default.
pub const BEVEL_JOIN: i32 = 64;
/// `ShapePath.RoundJoin`.
pub const ROUND_JOIN: i32 = 128;

/// `ShapePath.FlatCap`.
pub const FLAT_CAP: i32 = 0;
/// `ShapePath.SquareCap`, the default.
pub const SQUARE_CAP: i32 = 16;
/// `ShapePath.RoundCap`.
pub const ROUND_CAP: i32 = 32;

/// `ShapePath.SolidLine`, the default.
pub const SOLID_LINE: i32 = 1;
/// `ShapePath.DashLine`.
pub const DASH_LINE: i32 = 2;

pub(crate) fn shape_path_enums() -> EnumTable {
    vec![
        (
            "FillRule",
            vec![
                ("OddEvenFill", i64::from(ODD_EVEN_FILL)),
                ("WindingFill", i64::from(WINDING_FILL)),
            ],
        ),
        (
            "JoinStyle",
            vec![
                ("MiterJoin", i64::from(MITER_JOIN)),
                ("BevelJoin", i64::from(BEVEL_JOIN)),
                ("RoundJoin", i64::from(ROUND_JOIN)),
            ],
        ),
        (
            "CapStyle",
            vec![
                ("FlatCap", i64::from(FLAT_CAP)),
                ("SquareCap", i64::from(SQUARE_CAP)),
                ("RoundCap", i64::from(ROUND_CAP)),
            ],
        ),
        (
            "StrokeStyle",
            vec![
                ("SolidLine", i64::from(SOLID_LINE)),
                ("DashLine", i64::from(DASH_LINE)),
            ],
        ),
    ]
}

/// `ShapePath` (QtQuick.Shapes).
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick.Shapes",
    name = "ShapePath",
    extends = Path,
    enums = shape_path_enums
)]
pub struct ShapePath {
    #[qml(base)]
    pub path: Path,
    #[qml(notify)]
    pub stroke_color: Color,
    #[qml(notify)]
    pub stroke_width: f64,
    #[qml(notify)]
    pub fill_color: Color,
    #[qml(notify, enumeration = "FillRule")]
    pub fill_rule: i32,
    #[qml(notify, enumeration = "JoinStyle")]
    pub join_style: i32,
    /// How far a miter join may reach, in stroke widths, as SVG counts it.
    #[qml(notify)]
    pub miter_limit: i32,
    #[qml(notify, enumeration = "CapStyle")]
    pub cap_style: i32,
    #[qml(notify, enumeration = "StrokeStyle")]
    pub stroke_style: i32,
    /// Where in the pattern the stroke starts, in stroke widths.
    #[qml(notify)]
    pub dash_offset: f64,
    /// Dash, gap, dash, gap, each in stroke widths.
    #[qml(notify)]
    pub dash_pattern: Vec<Value>,
    /// A `LinearGradient`, `RadialGradient` or `ConicalGradient` to fill
    /// with instead of `fillColor`.
    #[qml(notify, reset)]
    pub fill_gradient: Option<ObjectId>,
}

impl Default for ShapePath {
    fn default() -> Self {
        Self {
            path: Path::default(),
            stroke_color: Color::WHITE,
            stroke_width: 1.0,
            fill_color: Color::WHITE,
            fill_rule: ODD_EVEN_FILL,
            join_style: BEVEL_JOIN,
            miter_limit: 2,
            cap_style: SQUARE_CAP,
            stroke_style: SOLID_LINE,
            dash_offset: 0.0,
            dash_pattern: vec![Value::Real(4.0), Value::Real(2.0)],
            fill_gradient: None,
        }
    }
}

#[qml_impl]
impl ShapePath {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let obj = cx.this;
        // Here and not where the stroke is built: once for each pattern,
        // however often it is painted.
        cx.engine.effect_in(obj, move |e| {
            let dashed = num(e, obj, ShapePath::STROKE_STYLE.index) as i32 == DASH_LINE;
            if dashed && dash_pattern(e, obj).len() % 2 == 1 {
                e.log(
                    LogLevel::Warning,
                    "qml",
                    "QPen::setDashPattern: Pattern not of even length",
                );
            }
        })?;
        Ok(())
    }
}

/// The pattern as it was written, in stroke widths (tracked).
fn dash_pattern(engine: &Engine, obj: ObjectId) -> Vec<f64> {
    match engine.plain(&engine.read(obj, ShapePath::DASH_PATTERN.index)) {
        Value::List(l) => l.iter().map(Value::to_number).collect(),
        _ => Vec::new(),
    }
}

fn color(engine: &Engine, obj: ObjectId, p: ljos_engine::Prop<Color>) -> Color {
    match engine.read(obj, p.index) {
        Value::Color(c) => c,
        _ => Color::TRANSPARENT,
    }
}

/// The dashes of a stroke `width` wide: lengths and where in them the
/// stroke starts, or `None` for a line without gaps.
fn dashes(pattern: &[f64], offset: f64, width: f64) -> Option<(Vec<f64>, f64)> {
    let mut lengths: Vec<f64> = pattern
        .iter()
        .map(|v| {
            if v.is_finite() {
                v.max(0.0) * width
            } else {
                0.0
            }
        })
        .collect();
    // Qt takes a pattern that ends on a dash to have a gap of one after it.
    if lengths.len() % 2 == 1 {
        lengths.push(width);
    }
    let period: f64 = lengths.iter().sum();
    let gaps = lengths.iter().skip(1).step_by(2).any(|gap| *gap > 0.0);
    if !gaps || !period.is_finite() || period <= 0.0 {
        return None;
    }
    let offset = if offset.is_finite() {
        (offset * width).rem_euclid(period)
    } else {
        0.0
    };
    Some((lengths, offset))
}

/// How a `ShapePath` is outlined (tracked), or `None` when it is not.
fn stroke_of(engine: &Engine, obj: ObjectId) -> Option<(Arc<Stroke>, Brush)> {
    let width = num(engine, obj, ShapePath::STROKE_WIDTH.index);
    let ink = color(engine, obj, ShapePath::STROKE_COLOR);
    if !width.is_finite() || width <= 0.0 || ink.a == 0 {
        return None;
    }
    let cap_style = num(engine, obj, ShapePath::CAP_STYLE.index) as i32;
    let cap = match cap_style {
        FLAT_CAP => Cap::Butt,
        ROUND_CAP => Cap::Round,
        _ => Cap::Square,
    };
    let limit = num(engine, obj, ShapePath::MITER_LIMIT.index);
    let join = match num(engine, obj, ShapePath::JOIN_STYLE.index) as i32 {
        // A miter that may not reach at all is a bevel.
        MITER_JOIN if limit >= 1.0 => Join::Miter,
        ROUND_JOIN => Join::Round,
        _ => Join::Bevel,
    };
    let mut stroke = Stroke::new(width)
        .with_join(join)
        .with_caps(cap)
        .with_miter_limit(limit.max(1.0));
    if num(engine, obj, ShapePath::STROKE_STYLE.index) as i32 == DASH_LINE {
        let pattern = dash_pattern(engine, obj);
        let offset = num(engine, obj, ShapePath::DASH_OFFSET.index);
        if let Some((lengths, offset)) = dashes(&pattern, offset, width) {
            // Dashes of no length are nothing unless their caps are.
            if cap_style == FLAT_CAP && lengths.iter().step_by(2).all(|dash| *dash <= 0.0) {
                return None;
            }
            stroke = stroke.with_dashes(offset, lengths);
        }
    }
    Some((Arc::new(stroke), Brush::Solid(to_rgba(ink))))
}

/// What a `ShapePath` is filled with (tracked), or `None` when it is not.
fn fill_of(engine: &Engine, obj: ObjectId) -> Option<(FillRule, Brush)> {
    let rule = match num(engine, obj, ShapePath::FILL_RULE.index) as i32 {
        WINDING_FILL => FillRule::NonZero,
        _ => FillRule::EvenOdd,
    };
    let gradient = engine
        .read(obj, ShapePath::FILL_GRADIENT.index)
        .as_object()
        .filter(|g| engine.is_alive(*g))
        .and_then(|g| super::gradient::brush(engine, g));
    let brush = match gradient {
        Some(brush) => brush,
        None => {
            let ink = color(engine, obj, ShapePath::FILL_COLOR);
            if ink.a == 0 {
                return None;
            }
            Brush::Solid(to_rgba(ink))
        }
    };
    Some((rule, brush))
}

/// How far the ink of a `ShapePath` is taken to reach beyond its path, for
/// `Shape.boundingRect` (tracked): half the stroke, and for square caps the
/// diagonal of that. Qt counts a width below nothing too, which pulls the
/// rectangle in.
pub(crate) fn margin(engine: &Engine, obj: ObjectId) -> f64 {
    if color(engine, obj, ShapePath::STROKE_COLOR).a == 0 {
        return 0.0;
    }
    let half = num(engine, obj, ShapePath::STROKE_WIDTH.index) / 2.0;
    if num(engine, obj, ShapePath::CAP_STYLE.index) as i32 == SQUARE_CAP {
        half * std::f64::consts::SQRT_2
    } else {
        half
    }
}

/// What one `ShapePath` paints.
#[derive(Clone, Debug)]
pub struct PathPaint {
    pub(crate) path: Arc<BezPath>,
    pub(crate) fill: Option<(FillRule, Brush)>,
    pub(crate) stroke: Option<(Arc<Stroke>, Brush)>,
}

impl PathPaint {
    /// Reads a `ShapePath` (tracked). `None` when nothing of it shows.
    pub(crate) fn read(engine: &Engine, obj: ObjectId) -> Option<Self> {
        let fill = fill_of(engine, obj);
        let stroke = stroke_of(engine, obj);
        if fill.is_none() && stroke.is_none() {
            return None;
        }
        let PathGeometry { path, .. } = &*geometry(engine, obj)?;
        Some(Self {
            path: Arc::clone(path),
            fill,
            stroke,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::dashes;

    #[test]
    fn dashes_count_in_stroke_widths() {
        assert_eq!(dashes(&[4.0, 2.0], 0.0, 4.0), Some((vec![16.0, 8.0], 0.0)));
        assert_eq!(dashes(&[4.0, 2.0], 1.0, 4.0), Some((vec![16.0, 8.0], 4.0)));
    }

    #[test]
    fn an_offset_is_brought_into_the_pattern() {
        assert_eq!(
            dashes(&[4.0, 2.0], -1.0, 4.0),
            Some((vec![16.0, 8.0], 20.0))
        );
        assert_eq!(dashes(&[4.0, 2.0], 7.0, 4.0), Some((vec![16.0, 8.0], 4.0)));
    }

    #[test]
    fn a_pattern_that_ends_on_a_dash_gets_a_gap_of_one() {
        assert_eq!(dashes(&[3.0], 0.0, 4.0), Some((vec![12.0, 4.0], 0.0)));
    }

    #[test]
    fn a_pattern_without_gaps_is_a_solid_line() {
        assert_eq!(dashes(&[], 0.0, 4.0), None);
        assert_eq!(dashes(&[2.0, 0.0], 0.0, 4.0), None);
        assert_eq!(dashes(&[0.0, 0.0], 0.0, 4.0), None);
    }

    #[test]
    fn dashes_of_no_length_keep_their_gaps() {
        assert_eq!(dashes(&[0.0, 2.0], 0.0, 4.0), Some((vec![0.0, 8.0], 0.0)));
    }
}
