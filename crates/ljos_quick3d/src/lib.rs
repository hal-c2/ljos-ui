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
//! M2 to M4: `Object3D`, `Node`, `Model` (`#Cube`, `#Rectangle`,
//! `#Sphere`, `#Cylinder` and `#Cone`), `PerspectiveCamera` (vertical or
//! horizontal field of view), `OrthographicCamera`, a camera's
//! `frustumCullingEnabled`, `DirectionalLight`, `PointLight` and
//! `SpotLight` (with every light's `color`, `ambientColor` and
//! `brightness`; up to 16 a scene), `Material.cullMode`,
//! `DefaultMaterial` (`diffuseColor`, `diffuseMap`, `lighting`,
//! `specularAmount`, `specularRoughness`), `PrincipledMaterial`
//! (`baseColor`, `metalness`, `roughness`, `emissiveFactor`, `opacity`,
//! `alphaMode`, `alphaCutoff`, `lighting`, and the base colour,
//! metalness, roughness, normal, emissive, occlusion and opacity maps with
//! their channels), `Texture` (every property but `sourceItem`),
//! `SceneEnvironment` (`clearColor`, `backgroundMode` Transparent, Color
//! and SkyBox, `lightProbe` with `probeExposure`, `probeHorizon`,
//! `probeOrientation` and `skyboxBlurAmount`, `tonemapMode`,
//! `antialiasingMode` MSAA with `antialiasingQuality`), `View3D` and
//! `RuntimeLoader` (glTF 2.0 files read on the compute pool; without the
//! default `gltf` feature every source is an error that says so).
//! `Node.pivot` and `Node.opacity` are kept but do not change the drawing
//! yet.
//!
//! A `Texture` reads its picture through the image cache the first time a
//! view draws it: a small local picture at once, others in the background,
//! the views that wanted it painted again when it arrives.
//!
//! ## Enum values
//!
//! Qt's documentation names the values of its enums but not their
//! numbers. They are numbered here in the order the documentation lists
//! them, from 0, and checked against Qt once qml6 has QtQuick3D.

mod items;
mod node;
mod pick;
mod repeater;
mod runtime_loader;
mod scene;
mod space;
mod texture;
mod view;

#[cfg(test)]
mod tests;

pub use ljos::item::ljos_3d::{ComponentType, PrimitiveType, Semantic, math::Vec3};
pub use node::{
    Camera, DefaultMaterial, DirectionalLight, Geometry, Light, Material, Model, Node, Object3D,
    OrthographicCamera, PerspectiveCamera, PointLight, PrincipledMaterial, SceneEnvironment,
    SpotLight,
};
pub use pick::PickResult;
pub use repeater::Repeater3D;
pub use runtime_loader::RuntimeLoader;
pub use texture::Texture;
pub use view::View3D;

use ljos::item;
use ljos::prelude::*;

/// Registers `QtQuick3D` on `engine`: the one seam a host calls.
pub fn register(engine: &Engine) {
    engine.register_module(ljos::module!("QtQuick3D" 6.0 => [
        Object3D,
        Node,
        Model,
        Geometry,
        Camera,
        PerspectiveCamera,
        OrthographicCamera,
        Light,
        DirectionalLight,
        PointLight,
        SpotLight,
        Material,
        DefaultMaterial,
        PrincipledMaterial,
        SceneEnvironment,
        Texture,
        View3D,
        PickResult,
        Repeater3D,
        RuntimeLoader,
        item::QuaternionAnimation,
    ]));
    item::register_paint_item::<View3D>(engine, view::read);
}
