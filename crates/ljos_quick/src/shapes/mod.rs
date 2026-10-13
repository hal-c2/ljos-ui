//! QtQuick.Shapes: `Shape`, `ShapePath` and the gradients a path is filled
//! with, and `Path` with its elements (which Qt keeps in QtQuick).
//!
//! The objects hold what QML wrote. A path's outline is worked out when it
//! is asked for and kept until something it was made from changes
//! ([`path`]); a Shape hands the outlines to the render backends as they
//! are ([`ShapePaint`]), and how they are drawn is the backend's to choose.
//! A Shape that does not change costs nothing: nothing here runs unless a
//! property it read did change.
//!
//! Behaviour follows the documentation at doc.qt.io and what qml6 does
//! (`features/shapes/`).

mod builder;
mod gradient;
mod path;
mod shape;
mod shape_path;
mod svg;

pub use gradient::{ConicalGradient, LinearGradient, RadialGradient, ShapeGradient};
pub(crate) use path::geometry as path_geometry;
pub use path::{
    Path, PathAngleArc, PathArc, PathAttribute, PathCubic, PathCurve, PathLine, PathMove,
    PathMultiline, PathPercent, PathPolyline, PathQuad, PathRectangle, PathSvg,
};
pub(crate) use shape::takes_pointer;
pub use shape::{Shape, ShapePaint};
pub use shape_path::ShapePath;

use ljos_engine::Engine;

/// Registers module `QtQuick.Shapes`, and the path types in `QtQuick`.
pub(crate) fn register(engine: &Engine) {
    engine.register_module(ljos_engine::module!("QtQuick.Shapes" 1.15 => [
        Shape, ShapePath, ShapeGradient, LinearGradient, RadialGradient, ConicalGradient,
    ]));
    // Its own list, merged into the module by the registry.
    engine.register_module(ljos_engine::module!("QtQuick" 2.15 => [
        Path, PathMove, PathLine, PathQuad, PathCubic, PathArc, PathAngleArc,
        PathCurve, PathSvg, PathPolyline, PathMultiline, PathRectangle,
        PathAttribute, PathPercent,
    ]));
}
