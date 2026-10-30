//! Retained render tree, display lists, compose, hit testing and backends.
//!
//! See ARCHITECTURE.md sections 7.1 and 7.2. The crate has two backends:
//!
//! * **R** ([`VelloCpuReference`]): deterministic vello_cpu renderer for tests,
//!   Qt comparison and GPU-less fallback.
//! * **B0** (`b0` module, feature `b0`, default on): epaint shapes with cached
//!   meshes. It and its private modules (`b0_shape`, `glyph_atlas`,
//!   `image_atlas`) are the only places in this crate that name egui.

mod backend;
mod canvas;
mod compose;
mod damage;
mod display_list;
mod glyph;
mod image;
mod raster;
mod rgba;
pub mod scene3d;
mod shader;
mod shape;
mod tree;

#[cfg(feature = "b0")]
pub mod b0;
#[cfg(feature = "b0")]
mod b0_shape;
#[cfg(feature = "b0")]
mod glyph_atlas;
#[cfg(feature = "b0")]
mod image_atlas;

pub use backend::{
    BackendMemory, EncodeStats, FrameView, MemoryPressure, RenderBackend, RenderError, RgbaImage,
};
pub use canvas::{CanvasPaint, CanvasPainter, CanvasShadow, DrawState};
pub use compose::{
    ClipEntry, ClipId, ComposeStats, FrameSnapshot, PaintItem, hit_test, is_axis_aligned,
};
pub use damage::{PixelRect, frame_damage, merge as merge_damage};
pub use display_list::{
    Border, Brush, BrushGradient, Decorations, DisplayList, DisplayListBuilder, DisplayListId,
    DrawOp, EmbedId, FillRule, GlyphRun, GradientGeometry, GradientStop, Insets, LinearGradient,
    Paint, PaintContent, PaintCx, PathStyle, PositionedGlyph, ShapeStyle, Spread, TextAlign,
    TileMode, fill_area,
};
pub use glyph::{FontTables, Os2Table, font_tables, glyph_advances, glyph_ink_bounds};
pub use image::{
    ImageAlign, ImageBlit, ImageEncoding, ImageError, ImageFit, ImageHandle, ImagePlacement,
    MakeAgain, PixelData, WeakImage, image_blit, nine_patch_blits, painted_size, place_image,
};
pub use ljos_3d;
/// The programs a [`ShaderOp`] runs.
pub use ljos_shader;
pub use raster::{RasterConfig, TextMode, VelloCpuReference};
pub use rgba::Rgba;
pub use shader::{Cull, ShaderOp, ShaderTexture, Viewport};
pub use tree::{ClipSpec, Dirty, LayerSpec, NodeId, RenderNode, RenderTree, TreeError, TreeMemory};

/// Geometry types. Re-exported from vello_cpu so the whole workspace sees one
/// version of kurbo and peniko.
pub use vello_cpu::{kurbo, peniko};
