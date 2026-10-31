//! A QtQuick3D object graph read into a [`Scene`]: the mapping both hosts
//! share.
//!
//! The native QML engine and the web's object graph hold the same objects
//! (`Model`, `PerspectiveCamera`, `PrincipledMaterial`, ...) in different
//! stores. Each implements [`SceneSource`] over its own store and calls
//! [`build`]; what a member means, how an enumeration's number maps and
//! which default a member left out takes are written once, here.
//!
//! No GPU and no engine: this builds for `wasm32-unknown-unknown` without
//! the `gpu` feature.

use std::hash::Hash;
use std::sync::Arc;

use crate::custom::{CustomMaterial, Effect};
use crate::geometry::CustomGeometry;
use crate::instancing::Instancing;
use crate::math::{Mat4, Quat, Vec3};
use crate::scene::Node;
use crate::texture::TextureImage;

mod build;
pub mod defaults;
mod map_graph;
mod names;

pub use build::{Built, build, camera, srgb8, texture, tiling_mode, uv_transform};
pub use map_graph::{MapGraph, MapValue};
pub use names::{Prop3d, Type3d};

/// What a member's enumeration numbers are, as QML gives them: the values
/// of `DefaultMaterial.NoLighting`, `Texture.Repeat` and the rest. A
/// graph that holds enumerations by name gives their number here.
pub mod enums {
    /// `Light.shadowMapQuality`.
    pub mod shadow_map {
        pub const LOW: i32 = 0;
        pub const MEDIUM: i32 = 1;
        pub const HIGH: i32 = 2;
        pub const VERY_HIGH: i32 = 3;
        pub const ULTRA: i32 = 4;
    }

    /// `Light.softShadowQuality`.
    pub mod soft_shadow {
        pub const HARD: i32 = 0;
        pub const PCF4: i32 = 1;
        pub const PCF8: i32 = 2;
        pub const PCF16: i32 = 3;
        pub const PCF32: i32 = 4;
        pub const PCF64: i32 = 5;
    }

    /// `PerspectiveCamera.fieldOfViewOrientation`.
    pub mod fov_orientation {
        pub const VERTICAL: i32 = 0;
        pub const HORIZONTAL: i32 = 1;
    }

    /// `Material.cullMode`.
    pub mod cull {
        pub const BACK: i32 = 0;
        pub const FRONT: i32 = 1;
        pub const NONE: i32 = 2;
    }

    /// `Material.depthDrawMode`.
    pub mod depth_draw {
        pub const OPAQUE_ONLY: i32 = 0;
        pub const ALWAYS: i32 = 1;
        pub const NEVER: i32 = 2;
        pub const OPAQUE_PRE_PASS: i32 = 3;
    }

    /// `Material.TextureChannelMapping`.
    pub mod channel {
        pub const R: i32 = 0;
        pub const G: i32 = 1;
        pub const B: i32 = 2;
        pub const A: i32 = 3;
    }

    /// `DefaultMaterial.lighting` and `PrincipledMaterial.lighting`.
    pub mod lighting {
        pub const NO_LIGHTING: i32 = 0;
        pub const FRAGMENT_LIGHTING: i32 = 1;
    }

    /// `PrincipledMaterial.alphaMode`.
    pub mod alpha {
        pub const DEFAULT: i32 = 0;
        pub const BLEND: i32 = 1;
        pub const OPAQUE: i32 = 2;
        pub const MASK: i32 = 3;
    }

    /// `SceneEnvironment.antialiasingMode` (`MSAA`) and
    /// `antialiasingQuality` (the rest).
    pub mod antialiasing {
        pub const NO_AA: i32 = 0;
        pub const SSAA: i32 = 1;
        pub const MSAA: i32 = 2;
        pub const PROGRESSIVE_AA: i32 = 3;
        pub const MEDIUM: i32 = 0;
        pub const HIGH: i32 = 1;
        pub const VERY_HIGH: i32 = 2;
    }

    /// `SceneEnvironment.backgroundMode`.
    pub mod background {
        pub const TRANSPARENT: i32 = 0;
        pub const COLOR: i32 = 1;
        pub const SKY_BOX: i32 = 2;
        pub const SKY_BOX_CUBE_MAP: i32 = 3;
        pub const SKY_MATERIAL: i32 = 4;
    }

    /// `SceneEnvironment.tonemapMode`.
    pub mod tonemap {
        pub const NONE: i32 = 0;
        pub const LINEAR: i32 = 1;
        pub const ACES: i32 = 2;
        pub const HEJL_DAWSON: i32 = 3;
        pub const FILMIC: i32 = 4;
        pub const CUSTOM: i32 = 5;
    }

    /// `Texture.tilingModeHorizontal` and the rest.
    pub mod tiling {
        pub const CLAMP_TO_EDGE: i32 = 0;
        pub const MIRRORED_REPEAT: i32 = 1;
        pub const REPEAT: i32 = 2;
    }

    /// `Texture.minFilter`, `magFilter` and `mipFilter`.
    pub mod filter {
        pub const NONE: i32 = 0;
        pub const NEAREST: i32 = 1;
        pub const LINEAR: i32 = 2;
    }

    /// `ExtendedSceneEnvironment.glowBlendMode`.
    pub mod glow_blend {
        pub const ADDITIVE: i32 = 0;
        pub const SCREEN: i32 = 1;
        pub const SOFT_LIGHT: i32 = 2;
        pub const REPLACE: i32 = 3;
    }
}

/// An object graph that holds a View3D and its 3D objects: what
/// [`build`] reads.
///
/// A read returns `None` for a member the graph does not hold or the
/// object does not have; the mapping then takes Qt's default for it
/// ([`defaults`]). A graph that tracks what was read (the QML engine, the
/// web graph's versions) tracks these reads: a later change to any of
/// them is what makes the scene worth building again.
///
/// The hooks are the parts that are not plain members: pixels, committed
/// geometry, shader programs, loaded files, instance tables and a node's
/// 2D items. Each has a default of `None`, which leaves the part out as
/// the native host does while it is not ready.
pub trait SceneSource {
    /// An object of the graph: a handle, compared and hashed to name the
    /// scene node it became.
    type Object: Copy + Eq + Hash;

    /// Whether `o` is a `ty`, or derives from one: a `PerspectiveCamera`
    /// is a `Camera`, a `Node` and an `Object3D`.
    fn is(&self, o: Self::Object, ty: Type3d) -> bool;

    fn number(&self, o: Self::Object, p: Prop3d) -> Option<f64>;
    /// An integer member, enumerations among them, as their number.
    fn int(&self, o: Self::Object, p: Prop3d) -> Option<i32>;
    fn boolean(&self, o: Self::Object, p: Prop3d) -> Option<bool>;
    /// sRGB, not premultiplied, each channel 0 to 1, as `[r, g, b, a]`.
    fn color(&self, o: Self::Object, p: Prop3d) -> Option<[f32; 4]>;
    fn vector3d(&self, o: Self::Object, p: Prop3d) -> Option<Vec3>;
    /// Not necessarily of unit length.
    fn quaternion(&self, o: Self::Object, p: Prop3d) -> Option<Quat>;
    /// As written, or resolved against the file that wrote it: only the
    /// fragment of a built-in mesh's `#Cube` is read.
    fn url(&self, o: Self::Object, p: Prop3d) -> Option<String>;
    fn object(&self, o: Self::Object, p: Prop3d) -> Option<Self::Object>;
    /// A list member's live objects, in order. `data` of a View3D is its
    /// 3D nodes.
    fn objects(&self, o: Self::Object, p: Prop3d) -> Vec<Self::Object>;
    /// Whether the application gave `p` a value, by a binding or an
    /// assignment, rather than leaving it at its default. Code written
    /// before a switch existed sets only what the switch now gates.
    fn authored(&self, o: Self::Object, p: Prop3d) -> bool;

    /// A list member with its holes: a `Skin`'s joints keep their indices
    /// when one of them is gone.
    fn object_slots(&self, o: Self::Object, p: Prop3d) -> Vec<Option<Self::Object>> {
        self.objects(o, p).into_iter().map(Some).collect()
    }

    /// A list of `Qt.matrix4x4` values (`inverseBindPoses`); a missing or
    /// unreadable one is the identity.
    fn matrices(&self, o: Self::Object, p: Prop3d) -> Vec<Mat4> {
        let _ = (o, p);
        Vec::new()
    }

    /// The pixels the `Texture` `texture` shows in `view`: its
    /// `sourceItem`'s picture or its decoded `source`. `None` while they
    /// are not ready; the host builds the scene again when they are.
    fn texture_image(&self, view: Self::Object, texture: Self::Object) -> Option<TextureImage> {
        let _ = (view, texture);
        None
    }

    /// The vertices the `Geometry` `geometry` last committed.
    fn geometry(&self, geometry: Self::Object) -> Option<Arc<CustomGeometry>> {
        let _ = geometry;
        None
    }

    /// The program the `CustomMaterial` `material` makes of its snippets;
    /// `None` leaves its model out, as Qt does with snippets that make no
    /// program.
    fn custom_material(
        &self,
        view: Self::Object,
        material: Self::Object,
    ) -> Option<CustomMaterial> {
        let _ = (view, material);
        None
    }

    /// The passes of the `Effect` `effect`; `None` leaves it out and the
    /// others still run.
    fn effect(&self, view: Self::Object, effect: Self::Object) -> Option<Effect> {
        let _ = (view, effect);
        None
    }

    /// The table of the `Instancing` object `table`: its entries, read or
    /// loaded.
    fn instancing(&self, table: Self::Object) -> Option<Instancing> {
        let _ = table;
        None
    }

    /// The meshes a `RuntimeLoader` loaded, and the `Model` objects it
    /// made for them, by the index of the mesh in the file.
    #[cfg(feature = "gltf")]
    fn loaded(&self, loader: Self::Object) -> Option<Loaded<Self::Object>> {
        let _ = loader;
        None
    }

    /// The 2D items declared in the node `node`, drawn as one textured
    /// quad, and the object a pick on the quad names.
    fn items_quad(&self, view: Self::Object, node: Self::Object) -> Option<(Node, Self::Object)> {
        let _ = (view, node);
        None
    }

    /// The lines of an `InfiniteGrid` among the environment's data.
    fn infinite_grid(&self, grid: Self::Object) -> Option<GridLines> {
        let _ = grid;
        None
    }
}

/// What a `RuntimeLoader` loaded: [`SceneSource::loaded`].
#[cfg(feature = "gltf")]
#[derive(Clone, Debug)]
pub struct Loaded<O> {
    pub asset: Arc<crate::gltf::Asset>,
    /// The index of a mesh primitive in the file, and the object a pick
    /// that hits it names.
    pub models: Vec<(usize, O)>,
}

/// The meshes of an `InfiniteGrid`: [`SceneSource::infinite_grid`].
#[derive(Clone, Debug)]
pub struct GridLines {
    pub grid: Arc<CustomGeometry>,
    pub x_axis: Arc<CustomGeometry>,
    pub z_axis: Arc<CustomGeometry>,
    /// `gridAxes`: the axes drawn in red and blue rather than grey.
    pub axes: bool,
}
