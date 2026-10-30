//! The `QtQuick3D` module for Ljós: a `View3D` item that shows a 3D scene
//! of `Node`s, `Model`s, cameras and lights.
//!
//! This is an opt-in module crate written against the public `ljos`
//! facade alone, as `ljos_lottie` is. A host registers it like any native
//! module:
//!
//! ```ignore
//! let engine = ljos::engine();
//! ljos_quick3d::register(&engine);
//! ```
//!
//! QML then says `import QtQuick3D`:
//!
//! ```qml
//! View3D {
//!     anchors.fill: parent
//!     PerspectiveCamera { z: 300 }
//!     DirectionalLight {}
//!     Model {
//!         source: "#Cube"
//!         eulerRotation.y: 30
//!         materials: DefaultMaterial { diffuseColor: "red" }
//!     }
//! }
//! ```
//!
//! ## Drawing
//!
//! A `View3D` is a paint item. Its `read` walks the nodes under it into a
//! [`Scene`](ljos::item::ljos_3d::Scene) and paints one
//! [`DrawOp::View3d`](ljos::item::DrawOp) with it; the painter or the
//! headless backend draws that with `ljos_3d` (ARCHITECTURE.md §7.8). The
//! walk is tracked, so the view is drawn again only when something it read
//! changed, and a walk that comes out the same keeps the scene it had so
//! the picture is not drawn again. A still scene schedules nothing.
//!
//! ## What is built
//!
//! `docs/quick3d.md` lists the milestones. This crate has the types of
//! M2: `Object3D`, `Node`, `Model` (`#Cube` and `#Rectangle`),
//! `PerspectiveCamera`, `OrthographicCamera`, `DirectionalLight`,
//! `DefaultMaterial` (`diffuseColor`, `lighting`), `SceneEnvironment`
//! (`clearColor`, `backgroundMode` Transparent and Color) and `View3D`.
//! `Node.pivot` and `Node.opacity` are kept but do not change the drawing
//! yet.
//!
//! ## Enum values
//!
//! Qt's documentation names the values of its enums but not their
//! numbers. They are numbered here in the order the documentation lists
//! them, from 0, and checked against Qt once qml6 has QtQuick3D.

mod node;
mod scene;
mod view;

#[cfg(test)]
mod tests;

pub use node::{
    Camera, DefaultMaterial, DirectionalLight, Light, Material, Model, Node, Object3D,
    OrthographicCamera, PerspectiveCamera, SceneEnvironment,
};
pub use view::View3D;

use ljos::item;
use ljos::prelude::*;

/// Registers `QtQuick3D` on `engine`: the one seam a host calls.
pub fn register(engine: &Engine) {
    engine.register_module(ljos::module!("QtQuick3D" 6.0 => [
        Object3D,
        Node,
        Model,
        Camera,
        PerspectiveCamera,
        OrthographicCamera,
        Light,
        DirectionalLight,
        Material,
        DefaultMaterial,
        SceneEnvironment,
        View3D,
    ]));
    item::register_paint_item::<View3D>(engine, view::read);
}
