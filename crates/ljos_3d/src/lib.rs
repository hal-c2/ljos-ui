//! Ljós's 3D: the scene a `View3D` shows and the renderer that draws it.
//!
//! The crate knows neither egui nor the engine (ARCHITECTURE.md §7.8). The
//! QtQuick3D types write a [`Scene`]; a host draws it with a [`Renderer`]
//! on a wgpu device of its own choosing: egui-wgpu's in the desktop shell,
//! a browser's behind a `<canvas>` on the web. Without the `gpu` feature
//! the crate is the scene alone and links no wgpu.

pub mod custom;
pub mod fetch;
pub mod geometry;
#[cfg(feature = "gltf")]
pub mod gltf;
pub mod instancing;
pub mod math;
pub mod mesh;
#[cfg(feature = "gltf")]
pub mod obj;
pub mod pick;
pub mod qml;
pub mod scene;
pub mod sky;
pub mod slot;
#[cfg(feature = "snippets")]
pub mod snippet;
pub mod stats;
pub mod texture;
pub mod texture_data;

#[cfg(feature = "gpu")]
pub mod gpu;
#[cfg(feature = "gpu")]
mod gpu_extra;
#[cfg(feature = "gpu")]
mod ibl;
#[cfg(feature = "gpu")]
mod maps;
#[cfg(all(feature = "gpu", not(target_arch = "wasm32")))]
pub mod offscreen;
#[cfg(feature = "gpu")]
mod post;
#[cfg(feature = "gpu")]
mod programs;
#[cfg(feature = "gpu")]
mod screen;
#[cfg(feature = "gpu")]
mod shadow;
#[cfg(feature = "gpu")]
pub mod surface;
#[cfg(feature = "gpu")]
mod timing;

pub use custom::{
    Adjustments, AmbientOcclusion, Blend, BufferFormat, CustomMaterial, DepthDraw, DepthOfField,
    Effect, EffectBuffer, Glow, GlowBlend, Lut, Pass, PassTexture, PostProcess, Program,
    ProgramKind, Screen, UniformKind, UniformValue, Vignette,
};
pub use fetch::Fetch;
pub use geometry::{
    Attribute, ComponentType, CustomGeometry, GeometryError, PrimitiveType, Semantic,
};
pub use instancing::{Instance, InstanceTable, Instancing, Skin};
pub use scene::{
    AlphaMode, Background, Camera, Cascades, Color, CullMode, DefaultMaterial, Environment, Fade,
    Light, LightProbe, Lighting, Material, MeshSource, Model, Node, NodeId, NodeKind, Primitive,
    PrincipledMaps, PrincipledMaterial, Scene, Shadow, Tonemap, Transform,
};
pub use slot::SceneSlot;
pub use texture::{
    Channel, Filter, PixelEncoding, Sampling, Texture, TextureImage, TexturePixels, Tiling,
    UvTransform, fresh_pixels_id,
};

#[cfg(feature = "gpu")]
pub use gpu::{Renderer, Target, TextureBytes};
#[cfg(feature = "gpu")]
pub use surface::{Gpu, SurfaceView, SurfaceViewError};
#[cfg(feature = "gpu")]
pub use timing::timing_features;
