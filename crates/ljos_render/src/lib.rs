//! Retained render tree, display lists, compose, hit testing and backends.
//!
//! See ARCHITECTURE.md sections 7.1 and 7.2. The crate has two backends:
//!
//! * **R** ([`VelloCpuReference`]): deterministic vello_cpu renderer for tests,
//!   Qt comparison and GPU-less fallback.
//! * **B0** (`b0` module, feature `b0`, default on): epaint shapes with cached
//!   meshes. It is the only place in this crate that names egui.

mod backend;
mod compose;
mod display_list;
mod raster;
mod rgba;
mod tree;

#[cfg(feature = "b0")]
pub mod b0;

pub use backend::{EncodeStats, FrameView, MemoryPressure, RenderBackend, RenderError, RgbaImage};
pub use compose::{
    ClipEntry, ClipId, ComposeStats, FrameSnapshot, PaintItem, hit_test, is_axis_aligned,
};
pub use display_list::{
    Border, Decorations, DisplayList, DisplayListBuilder, DisplayListId, DrawOp, EmbedId, GlyphRun,
    GradientStop, ImageKey, Insets, LinearGradient, Paint, PaintContent, PaintCx, PathStyle,
    PositionedGlyph, TextAlign, TileMode,
};
pub use raster::{RasterConfig, TextMode, VelloCpuReference};
pub use rgba::Rgba;
pub use tree::{ClipSpec, Dirty, LayerSpec, NodeId, RenderNode, RenderTree, TreeError};

/// Geometry types. Re-exported from vello_cpu so the whole workspace sees one
/// version of kurbo and peniko.
pub use vello_cpu::{kurbo, peniko};
