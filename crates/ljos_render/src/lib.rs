//! Retained render tree, display lists, compose, hit testing and backends.
//!
//! See ARCHITECTURE.md sections 7.1 and 7.2. The crate has two backends:
//!
//! * **R** ([`VelloCpuReference`]): deterministic vello_cpu renderer for tests,
//!   Qt comparison and GPU-less fallback.
//! * **B0** (`b0` module, feature `b0`, default on): epaint shapes with cached
//!   meshes. It is the only place in this crate that names egui.

mod backend;
mod canvas;
mod compose;
mod display_list;
mod glyph;
mod image;
mod raster;
mod rgba;
mod shape;
mod tree;

#[cfg(feature = "b0")]
pub mod b0;
#[cfg(feature = "b0")]
mod b0_shape;
#[cfg(feature = "b0")]
mod glyph_atlas;

pub use backend::{EncodeStats, FrameView, MemoryPressure, RenderBackend, RenderError, RgbaImage};
pub use canvas::{CanvasPaint, CanvasPainter, CanvasShadow, DrawState};
pub use compose::{
    ClipEntry, ClipId, ComposeStats, FrameSnapshot, PaintItem, hit_test, is_axis_aligned,
};
pub use display_list::{
    Border, Brush, BrushGradient, Decorations, DisplayList, DisplayListBuilder, DisplayListId,
    DrawOp, EmbedId, FillRule, GlyphRun, GradientGeometry, GradientStop, Insets, LinearGradient,
    Paint, PaintContent, PaintCx, PathStyle, PositionedGlyph, ShapeStyle, Spread, TextAlign,
    TileMode, fill_area,
};
pub use glyph::{FontTables, Os2Table, font_tables, glyph_advances, glyph_ink_bounds};
pub use image::{
    ImageAlign, ImageBlit, ImageError, ImageFit, ImageHandle, ImagePlacement, WeakImage,
    image_blit, nine_patch_blits, painted_size, place_image,
};
pub use raster::{RasterConfig, TextMode, VelloCpuReference};
pub use rgba::Rgba;
pub use tree::{ClipSpec, Dirty, LayerSpec, NodeId, RenderNode, RenderTree, TreeError};

/// Geometry types. Re-exported from vello_cpu so the whole workspace sees one
/// version of kurbo and peniko.
pub use vello_cpu::{kurbo, peniko};
