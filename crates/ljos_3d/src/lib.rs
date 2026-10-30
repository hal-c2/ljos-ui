//! Ljós's 3D: the scene a `View3D` shows and the renderer that draws it.
//!
//! The crate knows neither egui nor the engine (ARCHITECTURE.md §7.8). The
//! QtQuick3D types write a [`Scene`]; a host draws it with a [`Renderer`]
//! on a wgpu device of its own choosing: egui-wgpu's in the desktop shell,
//! a browser's behind a `<canvas>` on the web. Without the `gpu` feature
//! the crate is the scene alone and links no wgpu.

pub mod geometry;
pub mod math;
pub mod mesh;
pub mod pick;
pub mod scene;
pub mod texture;

#[cfg(feature = "gpu")]
pub mod gpu;
#[cfg(feature = "gpu")]
mod ibl;
#[cfg(feature = "gpu")]
mod maps;
#[cfg(all(feature = "gpu", not(target_arch = "wasm32")))]
pub mod offscreen;
#[cfg(feature = "gpu")]
mod shadow;
#[cfg(feature = "gpu")]
pub mod surface;

pub use geometry::{
    Attribute, ComponentType, CustomGeometry, GeometryError, PrimitiveType, Semantic,
};
pub use scene::{
    AlphaMode, Background, Camera, Cascades, Color, CullMode, DefaultMaterial, Environment, Fade,
    Light, LightProbe, Lighting, Material, MeshSource, Model, Node, NodeId, NodeKind, Primitive,
    PrincipledMaps, PrincipledMaterial, Scene, Shadow, Tonemap, Transform,
};
pub use texture::{
    Channel, Filter, PixelEncoding, Sampling, Texture, TextureImage, TexturePixels, Tiling,
    UvTransform,
};

#[cfg(feature = "gpu")]
pub use gpu::{Renderer, Target, TextureBytes};
#[cfg(feature = "gpu")]
pub use surface::{SurfaceView, SurfaceViewError};
