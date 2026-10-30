//! `QtQuick3D.Helpers`: shape geometries, grids and the controllers that
//! make a scene explorable.
//!
//! One file per type or small group, each exporting its types; `register`
//! lists them, so a type added later touches its own file and one line here.

mod extended_environment;
mod geometry;
mod height_field;
mod library;
mod look_at;
mod mesh;
mod shapes;

use geometry::{
    CapsuleGeometry, ConeGeometry, CuboidGeometry, CylinderGeometry, GridGeometry, PlaneGeometry,
    SphereGeometry, TorusGeometry,
};
use height_field::HeightFieldGeometry;
use ljos::prelude::*;
use look_at::LookAtNode;

use crate::random_instancing::{InstanceRange, RandomInstancing};
pub use extended_environment::ExtendedSceneEnvironment;

pub(crate) fn register(engine: &Engine) {
    engine.register_module(ljos::module!("QtQuick3D.Helpers" 6.0 => [
        CuboidGeometry,
        SphereGeometry,
        CylinderGeometry,
        ConeGeometry,
        PlaneGeometry,
        TorusGeometry,
        CapsuleGeometry,
        GridGeometry,
        ExtendedSceneEnvironment,
        InstanceRange,
        RandomInstancing,
        HeightFieldGeometry,
        LookAtNode,
    ]));
    library::register(engine);
}
