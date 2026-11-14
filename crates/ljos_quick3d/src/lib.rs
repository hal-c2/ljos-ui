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
//! their channels), `Texture` (every property; `sourceItem` drawn live
//! and taking presses on a pickable model), Items declared in a `Node`
//! (drawn in its plane, taking presses),
//! `SceneEnvironment` (`clearColor`, `backgroundMode` Transparent, Color
//! and SkyBox, `lightProbe` with `probeExposure`, `probeHorizon`,
//! `probeOrientation` and `skyboxBlurAmount`, `tonemapMode`,
//! `antialiasingMode` MSAA with `antialiasingQuality`), `View3D` and
//! `RuntimeLoader` (glTF 2.0 files read on the compute pool; without the
//! default `gltf` feature every source is an error that says so; each of
//! a file's meshes is a pickable `Model` named after its node). A model's
//! `instancing` draws it once a row of an `InstanceList`, a
//! `FileInstancing` (XML) or a `RandomInstancing` (`QtQuick3D.Helpers`,
//! with `InstanceRange`, the only types of that module built), and
//! `Model.skin` or `Model.skeleton` with `inverseBindPoses` (`Skin`,
//! `Skeleton`, `Joint`) bends a mesh that has joints.
//! `Node.opacity` multiplies down the tree and blends a model below 1.
//! `Node.pivot` is kept but does not change the drawing yet.
//!
//! `CustomMaterial` (unshaded and shaded, with light processors and
//! `TextureInput` samplers) and `SceneEnvironment.effects` (`Effect`,
//! `Pass`, `Shader`, `Buffer`, `BufferInput` and `SetUniformValue`) run
//! the author's GLSL snippets, translated to WGSL when first drawn; the
//! snippet files are read at that point, so only local and resource
//! files are read. `SceneEnvironment.ao*` draws screen-space ambient
//! occlusion. `QtQuick3D.Helpers` has `ExtendedSceneEnvironment`, with
//! exposure, white point, glow, depth of field, colour adjustments, a
//! vignette, a look-up table, FXAA and dithering.
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

mod arrival;
mod custom;
mod effects;
mod fetch;
mod helpers;
mod instancing;
mod items;
pub mod mesh_source;
mod node;
mod particles;
mod pick;
mod quaternion;
mod random_instancing;
mod repeater;
mod runtime_loader;
mod scene;
mod skin;
mod source;
mod space;
mod texture;
mod texture_data;
mod view;

#[cfg(test)]
mod sync_tests;
#[cfg(test)]
mod tests;

pub use custom::{
    Buffer, BufferInput, Command, CustomMaterial, Effect, Pass, SetUniformValue, Shader,
    TextureInput,
};
pub use helpers::ExtendedSceneEnvironment;
pub use instancing::{FileInstancing, InstanceList, InstanceListEntry, Instancing};
pub use ljos::item::ljos_3d::{ComponentType, PrimitiveType, Semantic, math::Vec3};
pub use node::{
    Camera, DefaultMaterial, DirectionalLight, Fog, Geometry, Light, Material, Model, MorphTarget,
    Node, Object3D, OrthographicCamera, PerspectiveCamera, PointLight, PrincipledMaterial,
    ReflectionProbe, SceneEnvironment, SpotLight, host_write,
};
pub use particles::{
    Affector3D, Attractor3D, Direction3D, DynamicBurst3D, EmitBurst3D, Gravity3D, LineParticle3D,
    ModelBlendParticle3D, ModelParticle3D, Particle3D, ParticleAbstractShape3D,
    ParticleCustomShape3D, ParticleEmitter3D, ParticleModelShape3D, ParticleSceneShape3D,
    ParticleShape3D, ParticleSystem3D, ParticleSystem3DLogging, ParticleTable, PointRotator3D,
    Repeller3D, ScaleAffector3D, SpriteParticle3D, SpriteSequence3D, TargetDirection3D,
    TrailEmitter3D, VectorDirection3D, Wander3D,
};
pub use pick::PickResult;
pub use random_instancing::{InstanceRange, RandomInstancing};
pub use repeater::Repeater3D;
pub use runtime_loader::RuntimeLoader;
pub use skin::{Joint, Skeleton, Skin};
pub use source::EngineSource;
pub use space::{Place, in_scene, parent_node, place};
pub use texture::Texture;
pub use texture_data::TextureData;
pub use view::View3D;

use ljos::item;
use ljos::prelude::*;

/// Registers `QtQuick3D` on `engine`: the one seam a host calls.
pub fn register(engine: &Engine) {
    ljos::item::register_state_owner::<Object3D>(
        engine,
        Object3D::STATE.index,
        Object3D::STATES.index,
        Object3D::TRANSITIONS.index,
    );
    engine.register_singleton::<quaternion::Quaternion>(
        "QtQuick3D",
        ljos::prelude::Version::new(6, 0),
    );
    engine.register_module(ljos::module!("QtQuick3D" 6.0 => [
        Object3D,
        Node,
        Model,
        Geometry,
        TextureData,
        Camera,
        PerspectiveCamera,
        OrthographicCamera,
        Light,
        DirectionalLight,
        PointLight,
        SpotLight,
        Fog,
        MorphTarget,
        ReflectionProbe,
        Material,
        DefaultMaterial,
        PrincipledMaterial,
        CustomMaterial,
        TextureInput,
        SceneEnvironment,
        Effect,
        Pass,
        Shader,
        Buffer,
        Command,
        BufferInput,
        SetUniformValue,
        Texture,
        View3D,
        helpers::RenderStats,
        PickResult,
        Repeater3D,
        RuntimeLoader,
        Instancing,
        InstanceList,
        InstanceListEntry,
        FileInstancing,
        Skin,
        Skeleton,
        Joint,
        item::QuaternionAnimation,
    ]));
    helpers::register(engine);
    particles::register(engine);
    item::register_paint_item::<View3D>(engine, view::read);
    item::register_pointer_portal::<View3D>(engine, pick::item_under);
}
