//! The `QtPositioning` module for Ljós: places on the Earth, and where the
//! system is.
//!
//! This is an opt-in module crate written against the public `ljos`
//! facade alone. The `ljos` command has it in `run`, `render` and
//! `test`; any other host registers it like any native module, and
//! installs the system's position service if it has one:
//!
//! ```ignore
//! let engine = ljos::engine();
//! ljos_positioning::register(&engine);
//! ljos_positioning::set_service(&engine, Rc::new(MyGnss::new()));
//! ```
//!
//! ## The surface
//!
//! Matching Qt 6.11.2 as probed on `qml6` (the scenarios in
//! `features/positioning/`):
//!
//! - the value types `geoCoordinate`, `geoRectangle`, `geoCircle`,
//!   `geoPath`, `geoPolygon` and `geoShape` ([`values`]): their members,
//!   their texts, `distanceTo`, `azimuthTo`, `atDistanceAndAzimuth`,
//!   `contains`, `boundingGeoRectangle`, `translated`, a path's `length`
//!   and the rest. Distances are on a sphere of the Earth's mean radius,
//!   as Qt measures them.
//! - the `QtPositioning` singleton ([`helpers`]), which makes each, turns
//!   a shape back into the type it holds and converts to and from Web
//!   Mercator.
//! - `PositionSource` and its `Position` ([`source`]), fed by a
//!   [`PositionService`]. Without one a source is not valid and starting
//!   it does nothing.
//!
//! A value is frozen, as every value type is in Ljós: a member is written
//! (`r.width = 4`), but the methods that change a value in place in Qt
//! (`addCoordinate`, `extendRectangle`) are not there.

pub mod geo;
pub mod helpers;
pub mod source;
pub mod values;

pub use geo::GeoCoordinate;
pub use source::{
    FixedPosition, NoPosition, Position, PositionFix, PositionService, PositionSink,
    PositionSource, service, set_service,
};
pub use values::{GeoCircle, GeoPath, GeoPolygon, GeoRectangle, Shape};

/// Registers `QtPositioning` on `engine`: the one seam a host calls.
pub fn register(engine: &ljos::Engine) {
    for ty in values::all() {
        engine.register_value_type(ty);
    }
    engine.register_module(ljos::module!("QtPositioning" 6.0 => [PositionSource, Position]));
    engine.register_singleton::<helpers::QtPositioning>(
        "QtPositioning",
        ljos::prelude::Version::new(6, 0),
    );
}
