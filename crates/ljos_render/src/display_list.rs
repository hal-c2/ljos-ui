//! Display lists: what an item looks like in its own local coordinates.
//!
//! Items implement [`PaintContent`] and never touch a GPU. The tree caches the
//! resulting [`DisplayList`] behind an `Arc` and only rebuilds it when the node
//! is `CONTENT`-dirty (ARCHITECTURE.md section 7.1).

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use vello_cpu::kurbo::{BezPath, Point, Rect, Shape, Size, Vec2};
use vello_cpu::peniko::FontData;

use crate::rgba::Rgba;
use crate::tree::NodeId;

/// Opaque handle to an image owned by the (future) image cache.
///
/// Placeholder: no backend resolves it yet.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct ImageKey(pub u64);

/// Opaque handle to a foreign surface (an embedded egui `Ui`, a video, ...).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct EmbedId(pub u64);

/// One colour stop of a [`LinearGradient`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GradientStop {
    /// Position along the gradient line, `0.0..=1.0`.
    pub offset: f32,
    /// Premultiplied colour at that position.
    pub color: Rgba,
}

/// A linear gradient in the node's local coordinates, padded past both ends.
#[derive(Clone, Debug, PartialEq)]
pub struct LinearGradient {
    /// Gradient line start (offset 0).
    pub start: Point,
    /// Gradient line end (offset 1).
    pub end: Point,
    /// Stops sorted by offset.
    pub stops: Vec<GradientStop>,
}

impl LinearGradient {
    /// Build a gradient; stops are sorted (stably) by offset.
    pub fn new(start: Point, end: Point, mut stops: Vec<GradientStop>) -> Self {
        stops.sort_by(|a, b| a.offset.total_cmp(&b.offset));
        Self { start, end, stops }
    }

    /// Colour at parameter `t` (clamped, i.e. `Extend::Pad`).
    pub fn color_at(&self, t: f32) -> Rgba {
        let (Some(first), Some(last)) = (self.stops.first(), self.stops.last()) else {
            return Rgba::TRANSPARENT;
        };
        if t <= first.offset {
            return first.color;
        }
        if t >= last.offset {
            return last.color;
        }
        for pair in self.stops.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            if t <= b.offset {
                let span = b.offset - a.offset;
                if span <= f32::EPSILON {
                    return b.color;
                }
                return a.color.lerp(b.color, (t - a.offset) / span);
            }
        }
        last.color
    }

    /// Projection of `p` on the gradient line, as an unclamped parameter.
    pub fn param(&self, p: Point) -> f32 {
        let d = self.end - self.start;
        let len2 = d.hypot2();
        if len2 <= f64::EPSILON {
            return 0.0;
        }
        ((p - self.start).dot(d) / len2) as f32
    }
}

/// What fills a shape.
#[derive(Clone, Debug, PartialEq)]
pub enum Paint {
    /// Flat colour.
    Solid(Rgba),
    /// Linear gradient.
    LinearGradient(Arc<LinearGradient>),
    /// Image fill (placeholder: no backend resolves images yet).
    Image(ImageKey),
}

impl Paint {
    /// Multiply the paint's alpha by `opacity`.
    #[must_use]
    pub fn with_opacity(&self, opacity: f32) -> Self {
        match self {
            Self::Solid(c) => Self::Solid(c.with_opacity(opacity)),
            Self::LinearGradient(g) => {
                let mut g = (**g).clone();
                for s in &mut g.stops {
                    s.color = s.color.with_opacity(opacity);
                }
                Self::LinearGradient(Arc::new(g))
            }
            Self::Image(k) => Self::Image(*k),
        }
    }

    /// True when the paint cannot produce visible pixels.
    pub fn is_invisible(&self) -> bool {
        match self {
            Self::Solid(c) => c.is_transparent(),
            Self::LinearGradient(g) => g.stops.iter().all(|s| s.color.is_transparent()),
            Self::Image(_) => false,
        }
    }
}

impl From<Rgba> for Paint {
    fn from(c: Rgba) -> Self {
        Self::Solid(c)
    }
}

/// Border drawn inside the rectangle's outer edge.
#[derive(Clone, Debug, PartialEq)]
pub struct Border {
    /// Width in logical pixels.
    pub width: f32,
    /// Border paint.
    pub paint: Paint,
}

/// Per-side insets of a nine-patch.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Insets {
    /// Left inset.
    pub left: f32,
    /// Top inset.
    pub top: f32,
    /// Right inset.
    pub right: f32,
    /// Bottom inset.
    pub bottom: f32,
}

/// How a nine-patch edge region is filled.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TileMode {
    /// Stretch to fit.
    #[default]
    Stretch,
    /// Repeat.
    Repeat,
    /// Repeat, then scale so a whole number of tiles fits.
    Round,
}

/// One positioned glyph in a [`GlyphRun`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PositionedGlyph {
    /// Glyph id in the run's font.
    pub id: u32,
    /// Baseline-origin x, relative to the op's `origin`.
    pub x: f32,
    /// Baseline-origin y, relative to the op's `origin`.
    pub y: f32,
}

/// A shaped run of glyphs in one font at one size, produced by `ljos_text`.
///
/// Positions are relative to the `origin` of the [`DrawOp::Glyphs`] that draws
/// the run, so one cached run can be drawn at any alignment offset.
#[derive(Clone, Debug)]
pub struct GlyphRun {
    /// Font bytes and collection index.
    pub font: FontData,
    /// Font size in logical pixels.
    pub font_size: f32,
    /// Positioned glyphs.
    pub glyphs: Vec<PositionedGlyph>,
    /// Y of the run's baseline. Backends snap it to a whole device pixel, so
    /// both draw the same rows of pixels.
    pub baseline: f32,
    /// Ink bounds, supplied by the shaper (used for culling).
    pub bounds: Rect,
    /// Variation-axis position as normalised 2.14 coordinates; empty for a
    /// static font or the default instance.
    pub normalized_coords: Vec<i16>,
    /// Thicken the outlines, for a bold request the font file cannot meet.
    pub embolden: bool,
    /// Slant for an italic request the font file cannot meet: the tangent of
    /// the angle from upright, 0 for none.
    pub skew: f32,
}

impl GlyphRun {
    /// A run with no variation coordinates and no synthetic bold or italic.
    pub fn new(
        font: FontData,
        font_size: f32,
        glyphs: Vec<PositionedGlyph>,
        baseline: f32,
        bounds: Rect,
    ) -> Self {
        Self {
            font,
            font_size,
            glyphs,
            baseline,
            bounds,
            normalized_coords: Vec::new(),
            embolden: false,
            skew: 0.0,
        }
    }
}

/// Underline and strike-through bars of a [`DrawOp::Glyphs`], relative to its
/// `origin` and filled with its paint.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Decorations {
    /// The underline bar.
    pub underline: Option<Rect>,
    /// The strike-through bar.
    pub strike_through: Option<Rect>,
}

impl Decorations {
    /// The bars that are set.
    pub fn bars(&self) -> impl Iterator<Item = Rect> {
        self.underline.into_iter().chain(self.strike_through)
    }
}

/// Horizontal alignment of the stop-gap [`DrawOp::Text`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextAlign {
    /// Align to the left edge.
    #[default]
    Left,
    /// Centre.
    Center,
    /// Align to the right edge.
    Right,
}

/// Stroke or fill for [`DrawOp::Path`].
#[derive(Clone, Debug, PartialEq)]
pub enum PathStyle {
    /// Fill with the non-zero rule.
    Fill,
    /// Fill with the even-odd rule.
    FillEvenOdd,
    /// Stroke with the given width, round joins and butt caps.
    Stroke {
        /// Line width in logical pixels.
        width: f32,
    },
}

/// One drawing command in a node's local coordinates.
#[derive(Clone, Debug)]
pub enum DrawOp {
    /// Rounded rectangle. `radii` are top-left, top-right, bottom-right, bottom-left.
    Rect {
        /// Outer rectangle.
        rect: Rect,
        /// Corner radii (tl, tr, br, bl).
        radii: [f32; 4],
        /// Fill.
        fill: Paint,
        /// Optional inside border, painted over the fill.
        border: Option<Border>,
        /// Anti-aliasing (Qt's `antialiasing`).
        aa: bool,
    },
    /// Blurred box shadow, drawn around `rect`.
    Shadow {
        /// Caster rectangle.
        rect: Rect,
        /// Corner radii (tl, tr, br, bl).
        radii: [f32; 4],
        /// Blur width.
        blur: f32,
        /// Grow the caster by this much on every side.
        spread: f32,
        /// Shadow colour at full strength.
        color: Rgba,
    },
    /// Image blit (placeholder: no backend resolves images yet).
    Image {
        /// Image handle.
        image: ImageKey,
        /// Source rectangle in image pixels.
        src: Rect,
        /// Destination rectangle.
        dst: Rect,
        /// Smooth (bilinear) sampling.
        smooth: bool,
    },
    /// Nine-patch (placeholder).
    NinePatch {
        /// Image handle.
        image: ImageKey,
        /// Fixed border sizes.
        insets: Insets,
        /// Destination rectangle.
        dst: Rect,
        /// Horizontal and vertical edge handling.
        tile: [TileMode; 2],
    },
    /// Shaped glyphs: how text is drawn. Both backends fill the same outlines
    /// (R directly, B0 through its glyph atlas).
    Glyphs {
        /// The run.
        run: Arc<GlyphRun>,
        /// Where the run's coordinate origin lies in the node's local space.
        origin: Vec2,
        /// Fill. Backends draw a gradient or image paint in its first colour.
        paint: Paint,
        /// Decorations.
        decorations: Decorations,
    },
    /// A plain string laid out by egui's own text code, without shaping.
    ///
    /// `ljos_text` produces [`DrawOp::Glyphs`] instead; this stays for callers
    /// that have no text engine. Backends other than B0 skip it (R can draw a
    /// box with `TextMode::Boxes`).
    Text {
        /// Layout box; also the culling bounds.
        rect: Rect,
        /// The string, single line.
        text: Arc<str>,
        /// Font size in logical pixels.
        size: f32,
        /// Colour.
        color: Rgba,
        /// Horizontal alignment inside `rect`.
        align: TextAlign,
    },
    /// Vector path.
    Path {
        /// Geometry.
        path: Arc<BezPath>,
        /// Fill or stroke.
        style: PathStyle,
        /// Paint.
        paint: Paint,
    },
    /// Foreign surface (embedded egui widgets, video, ...). Splits batches.
    Embed {
        /// Surface id.
        surface: EmbedId,
        /// Placement rectangle.
        rect: Rect,
    },
}

impl DrawOp {
    /// Conservative local-space bounds of the ink this op produces.
    pub fn bounds(&self) -> Rect {
        match self {
            // The border sits inside the rect, so the rect already bounds it.
            Self::Rect { rect, .. } => *rect,
            Self::Shadow {
                rect, blur, spread, ..
            } => rect.inflate(f64::from(*spread + *blur), f64::from(*spread + *blur)),
            Self::Image { dst, .. } | Self::NinePatch { dst, .. } => *dst,
            Self::Glyphs {
                run,
                origin,
                decorations,
                ..
            } => {
                decorations
                    .bars()
                    .fold(run.bounds, |acc, bar| acc.union(bar))
                    + *origin
            }
            Self::Text { rect, .. } | Self::Embed { rect, .. } => *rect,
            Self::Path { path, style, .. } => {
                let b = path.bounding_box();
                match style {
                    PathStyle::Stroke { width } => {
                        let h = f64::from(*width) / 2.0;
                        b.inflate(h, h)
                    }
                    _ => b,
                }
            }
        }
    }
}

static NEXT_LIST_ID: AtomicU64 = AtomicU64::new(1);

/// Unique identity of a built [`DisplayList`]. Backends key their caches on it,
/// so a new list (a `CONTENT` rebuild) is the only thing that invalidates them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DisplayListId(u64);

/// An immutable list of draw ops in local coordinates.
#[derive(Debug)]
pub struct DisplayList {
    id: DisplayListId,
    ops: Box<[DrawOp]>,
    bounds: Rect,
}

impl DisplayList {
    /// Unique id of this build.
    pub fn id(&self) -> DisplayListId {
        self.id
    }

    /// The ops in paint order.
    pub fn ops(&self) -> &[DrawOp] {
        &self.ops
    }

    /// Union of all op bounds, in local space. Zero rect when empty.
    pub fn bounds(&self) -> Rect {
        self.bounds
    }

    /// True when there is nothing to draw.
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }
}

/// Collects [`DrawOp`]s for one node.
#[derive(Debug, Default)]
pub struct DisplayListBuilder {
    ops: Vec<DrawOp>,
}

impl DisplayListBuilder {
    /// Empty builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Append any op.
    pub fn push(&mut self, op: DrawOp) -> &mut Self {
        self.ops.push(op);
        self
    }

    /// Flat-coloured rectangle.
    pub fn fill_rect(&mut self, rect: Rect, color: Rgba) -> &mut Self {
        self.rounded_rect(rect, [0.0; 4], Paint::Solid(color), None)
    }

    /// Rounded rectangle with optional border, anti-aliased.
    pub fn rounded_rect(
        &mut self,
        rect: Rect,
        radii: [f32; 4],
        fill: Paint,
        border: Option<Border>,
    ) -> &mut Self {
        self.push(DrawOp::Rect {
            rect,
            radii,
            fill,
            border,
            aa: true,
        })
    }

    /// Blurred shadow.
    pub fn shadow(
        &mut self,
        rect: Rect,
        radii: [f32; 4],
        blur: f32,
        spread: f32,
        color: Rgba,
    ) -> &mut Self {
        self.push(DrawOp::Shadow {
            rect,
            radii,
            blur,
            spread,
            color,
        })
    }

    /// Path fill or stroke.
    pub fn path(&mut self, path: BezPath, style: PathStyle, paint: Paint) -> &mut Self {
        self.push(DrawOp::Path {
            path: Arc::new(path),
            style,
            paint,
        })
    }

    /// A shaped run in one colour.
    pub fn glyphs(
        &mut self,
        run: Arc<GlyphRun>,
        origin: Vec2,
        color: Rgba,
        decorations: Decorations,
    ) -> &mut Self {
        self.push(DrawOp::Glyphs {
            run,
            origin,
            paint: Paint::Solid(color),
            decorations,
        })
    }

    /// Unshaped plain text (see [`DrawOp::Text`]).
    pub fn text_stopgap(
        &mut self,
        rect: Rect,
        text: impl Into<Arc<str>>,
        size: f32,
        color: Rgba,
        align: TextAlign,
    ) -> &mut Self {
        self.push(DrawOp::Text {
            rect,
            text: text.into(),
            size,
            color,
            align,
        })
    }

    /// Embedded surface placeholder.
    pub fn embed(&mut self, surface: EmbedId, rect: Rect) -> &mut Self {
        self.push(DrawOp::Embed { surface, rect })
    }

    /// Number of ops so far.
    pub fn len(&self) -> usize {
        self.ops.len()
    }

    /// True when no op was added.
    pub fn is_empty(&self) -> bool {
        self.ops.is_empty()
    }

    /// Freeze into an immutable list with a fresh id.
    pub fn finish(self) -> DisplayList {
        let mut bounds: Option<Rect> = None;
        for op in &self.ops {
            let b = op.bounds();
            bounds = Some(bounds.map_or(b, |acc| acc.union(b)));
        }
        DisplayList {
            id: DisplayListId(NEXT_LIST_ID.fetch_add(1, Ordering::Relaxed)),
            ops: self.ops.into_boxed_slice(),
            bounds: bounds.unwrap_or(Rect::ZERO),
        }
    }

    /// [`Self::finish`] wrapped in an `Arc`, ready for `RenderTree::set_content`.
    pub fn finish_arc(self) -> Arc<DisplayList> {
        Arc::new(self.finish())
    }
}

/// Read-only context handed to [`PaintContent::paint`].
#[derive(Clone, Copy, Debug)]
pub struct PaintCx {
    /// The node being painted.
    pub node: NodeId,
    /// The node's size.
    pub size: Size,
    /// Physical pixels per logical pixel, for items that snap to pixels.
    pub scale_factor: f32,
}

/// Implemented by items that draw. Never touches the GPU.
pub trait PaintContent {
    /// Append this item's ops, in local coordinates, to `out`.
    fn paint(&self, cx: &PaintCx, out: &mut DisplayListBuilder);
}
