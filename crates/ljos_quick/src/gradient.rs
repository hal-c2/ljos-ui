//! `Gradient` and `GradientStop`: what a Rectangle is filled with instead
//! of one colour.
//!
//! The objects only hold what QML wrote. [`GradientPaint`] is what they
//! come to when a Rectangle is painted, and the one place that knows how
//! Qt reads the stops (all of it checked on qml6):
//!
//! - the stops are painted in the order of their positions, whatever
//!   order they were written in;
//! - of several stops at one position the first written is painted and
//!   the others are not;
//! - a position outside 0 to 1 is where the stop is, off the rectangle;
//! - before the first stop its colour is painted, and after the last, its;
//! - a gradient without stops paints nothing: the rectangle's own colour
//!   shows.

use ljos_engine::{
    Color, Cx, Engine, EngineError, ObjectId, QmlList, QmlObject, QtObject, Value, qml_impl,
};
use ljos_render::{GradientStop as RenderStop, LinearGradient, Paint, kurbo};

use crate::enums::EnumTable;
use crate::item::is_a;
use crate::rectangle::to_rgba;

/// `Gradient.Vertical`, the default: from the top to the bottom.
pub const VERTICAL: i32 = 2;
/// `Gradient.Horizontal`: from the left to the right.
pub const HORIZONTAL: i32 = 1;

pub(crate) fn gradient_enums() -> EnumTable {
    vec![(
        "Orientation",
        vec![
            ("Vertical", i64::from(VERTICAL)),
            ("Horizontal", i64::from(HORIZONTAL)),
        ],
    )]
}

/// `GradientStop` (QtQuick): a colour at a position along a gradient.
///
/// Neither property has a change signal in Qt; what changes when they do
/// is told by the gradient's `updated`.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick", name = "GradientStop", extends = QtObject)]
pub struct GradientStop {
    #[qml(base)]
    pub object: QtObject,
    pub position: f64,
    pub color: Color,
}

impl Default for GradientStop {
    fn default() -> Self {
        Self {
            object: QtObject::default(),
            position: 0.0,
            color: Color::BLACK,
        }
    }
}

/// `Gradient` (QtQuick): a list of stops and the direction they run in.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick",
    name = "Gradient",
    extends = QtObject,
    default = "stops",
    enums = gradient_enums
)]
pub struct Gradient {
    #[qml(base)]
    pub object: QtObject,
    #[qml(list, append = stop_append)]
    pub stops: QmlList,
    #[qml(notify, enumeration = "Orientation")]
    pub orientation: i32,
}

impl Default for Gradient {
    fn default() -> Self {
        Self {
            object: QtObject::default(),
            stops: QmlList::new(),
            orientation: VERTICAL,
        }
    }
}

#[qml_impl]
impl Gradient {
    /// What the gradient paints has changed: a stop's position or colour,
    /// the stops themselves, or the orientation.
    #[qml(signal)]
    fn updated();

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let obj = cx.this;
        // The first run only learns what to follow: a gradient that was
        // just made has not been updated.
        let mut first = true;
        cx.engine.effect_in(obj, move |e| {
            let _ = GradientPaint::of(e, obj);
            if !std::mem::take(&mut first) {
                let _ = e.emit_by_name(obj, "updated", &[]);
            }
        })?;
        Ok(())
    }
}

fn stop_append(cx: &Cx, stop: ObjectId) -> Result<(), EngineError> {
    if !is_a::<GradientStop>(cx.engine, stop) {
        return Err(EngineError::WrongType {
            ty: "Gradient.stops".into(),
            expected: "a GradientStop".into(),
        });
    }
    cx.engine.list_push(cx.this, Gradient::STOPS.index, stop)
}

/// Whether `v` holds a `Gradient` that is still there.
pub(crate) fn as_gradient(engine: &Engine, v: &Value) -> Option<ObjectId> {
    v.as_object().filter(|g| is_a::<Gradient>(engine, *g))
}

/// What a gradient paints.
#[derive(Clone, Debug, PartialEq)]
pub struct GradientPaint {
    pub horizontal: bool,
    /// Position and colour, in the order of the positions; no two stops
    /// share a position, and there is at least one.
    pub stops: Vec<(f64, Color)>,
}

impl GradientPaint {
    /// Reads a `Gradient` (tracked: the stops, their positions and colours
    /// and the orientation). `None` for a gradient without stops, which
    /// paints nothing.
    pub fn of(engine: &Engine, gradient: ObjectId) -> Option<Self> {
        let horizontal = engine
            .read(gradient, Gradient::ORIENTATION.index)
            .to_number()
            == f64::from(HORIZONTAL);
        let Value::List(list) = engine.read(gradient, Gradient::STOPS.index) else {
            return None;
        };
        let mut stops: Vec<(f64, Color)> = list
            .iter()
            .filter_map(Value::as_object)
            .filter(|s| is_a::<GradientStop>(engine, *s))
            .filter_map(|s| {
                let position = engine.read(s, GradientStop::POSITION.index).to_number();
                let color = match engine.read(s, GradientStop::COLOR.index) {
                    Value::Color(c) => c,
                    _ => Color::BLACK,
                };
                // A stop that is nowhere cannot be put in order.
                position.is_finite().then_some((position, color))
            })
            .collect();
        // Stable, so that the first written of several at one position
        // stays first and is the one kept.
        stops.sort_by(|a, b| a.0.total_cmp(&b.0));
        stops.dedup_by(|later, kept| later.0 == kept.0);
        (!stops.is_empty()).then_some(Self { horizontal, stops })
    }

    /// The paint that fills `rect`.
    pub fn paint(&self, rect: kurbo::Rect) -> Paint {
        let (first, last) = (self.stops[0], self.stops[self.stops.len() - 1]);
        let span = last.0 - first.0;
        if self.stops.len() == 1 || span <= 0.0 {
            return Paint::Solid(to_rgba(first.1));
        }
        // The line runs from the first stop to the last, wherever on or off
        // the rectangle they are; the renderer keeps the end colours going
        // beyond them, which is what Qt paints there.
        let at = |position: f64| {
            if self.horizontal {
                kurbo::Point::new(rect.x0 + position * rect.width(), rect.y0)
            } else {
                kurbo::Point::new(rect.x0, rect.y0 + position * rect.height())
            }
        };
        let stops = self
            .stops
            .iter()
            .map(|&(position, color)| RenderStop {
                offset: ((position - first.0) / span) as f32,
                color: to_rgba(color),
            })
            .collect();
        Paint::LinearGradient(std::sync::Arc::new(LinearGradient::new(
            at(first.0),
            at(last.0),
            stops,
        )))
    }
}
