//! Display lists: what an item looks like in its own local coordinates.
//!
//! Items implement [`PaintContent`] and never touch a GPU. The tree caches the
//! resulting [`DisplayList`] behind an `Arc` and only rebuilds it when the node
//! is `CONTENT`-dirty (ARCHITECTURE.md section 7.1).

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use vello_cpu::kurbo::{BezPath, Cap, Join, Point, Rect, Shape, Size, Stroke, Vec2};
use vello_cpu::peniko::FontData;

use crate::image::ImageHandle;
use crate::rgba::Rgba;
use crate::tree::NodeId;

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
    /// Image fill. No backend draws it yet; [`DrawOp::Image`] is how
    /// pictures are drawn.
    Image(ImageHandle),
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
            Self::Image(k) => Self::Image(k.clone()),
        }
    }

    /// True when the paint hides what is under it wherever it is painted.
    pub fn is_opaque(&self) -> bool {
        // What rounds to 255 of 255 is opaque in an 8-bit picture.
        let opaque = |c: &Rgba| c.a >= 254.5 / 255.0;
        match self {
            Self::Solid(c) => opaque(c),
            Self::LinearGradient(g) => {
                !g.stops.is_empty() && g.stops.iter().all(|s| opaque(&s.color))
            }
            Self::Image(_) => false,
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

/// How a [`BrushGradient`] goes on outside its first and last stop.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Spread {
    /// The end colours go on for ever.
    #[default]
    Pad,
    /// The gradient runs back and forth.
    Reflect,
    /// The gradient starts again.
    Repeat,
}

impl Spread {
    /// Where the unbounded parameter `t` falls in `0..=1`.
    pub fn fold(self, t: f32) -> f32 {
        match self {
            Self::Pad => t.clamp(0.0, 1.0),
            Self::Repeat => t - t.floor(),
            Self::Reflect => {
                let u = t.rem_euclid(2.0);
                if u > 1.0 { 2.0 - u } else { u }
            }
        }
    }
}

/// Where the colours of a [`BrushGradient`] lie, in the node's coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GradientGeometry {
    /// Along the line from `start` (0) to `end` (1).
    Linear {
        /// Where the gradient is 0.
        start: Point,
        /// Where the gradient is 1.
        end: Point,
    },
    /// Between two circles: 0 on the focal circle, 1 on the outer one.
    Radial {
        /// Centre of the circle where the gradient is 0.
        focal: Point,
        /// Its radius.
        focal_radius: f64,
        /// Centre of the circle where the gradient is 1.
        center: Point,
        /// Its radius.
        radius: f64,
    },
    /// Around `center`: 0 at `angle` degrees from the positive x axis, going
    /// anticlockwise on a screen whose y axis points down, 1 after a turn.
    Conical {
        /// The point the colours turn around.
        center: Point,
        /// Where the gradient starts, in degrees.
        angle: f64,
    },
}

/// A gradient for [`Brush::Gradient`].
#[derive(Clone, Debug, PartialEq)]
pub struct BrushGradient {
    /// Where the colours lie.
    pub geometry: GradientGeometry,
    /// Stops sorted by offset, each within `0.0..=1.0`.
    pub stops: Vec<GradientStop>,
    /// What happens outside the stops. A conical gradient has no outside.
    pub spread: Spread,
}

impl BrushGradient {
    /// Build a gradient; stops are sorted (stably) by offset.
    pub fn new(geometry: GradientGeometry, mut stops: Vec<GradientStop>, spread: Spread) -> Self {
        stops.sort_by(|a, b| a.offset.total_cmp(&b.offset));
        Self {
            geometry,
            stops,
            spread,
        }
    }

    /// Colour at `t` once folded into `0..=1`.
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

    /// The gradient's parameter at `p`, before the spread folds it. `None`
    /// where a radial gradient between two circles has no colour.
    pub fn param(&self, p: Point) -> Option<f32> {
        match self.geometry {
            GradientGeometry::Linear { start, end } => {
                let d = end - start;
                let len2 = d.hypot2();
                if len2 <= f64::EPSILON {
                    return Some(0.0);
                }
                Some(((p - start).dot(d) / len2) as f32)
            }
            GradientGeometry::Radial {
                focal,
                focal_radius,
                center,
                radius,
            } => {
                // The circle through `p` with centre f + t (c - f) and
                // radius r0 + t (r1 - r0): the largest t whose radius is
                // not negative.
                let cd = center - focal;
                let pd = p - focal;
                let dr = radius - focal_radius;
                let a = cd.hypot2() - dr * dr;
                let b = pd.dot(cd) + focal_radius * dr;
                let c = pd.hypot2() - focal_radius * focal_radius;
                let t = if a.abs() < 1e-9 {
                    if b.abs() < 1e-12 {
                        return None;
                    }
                    c / (2.0 * b)
                } else {
                    let disc = b * b - a * c;
                    if disc < 0.0 {
                        return None;
                    }
                    let root = disc.sqrt();
                    let (t0, t1) = ((b + root) / a, (b - root) / a);
                    let ok = |t: f64| focal_radius + t * dr >= 0.0;
                    match (ok(t0.max(t1)), ok(t0.min(t1))) {
                        (true, _) => t0.max(t1),
                        (false, true) => t0.min(t1),
                        _ => return None,
                    }
                };
                (focal_radius + t * dr >= 0.0).then_some(t as f32)
            }
            GradientGeometry::Conical { center, angle } => {
                let d = p - center;
                let turns = (-d.y).atan2(d.x) / std::f64::consts::TAU - angle / 360.0;
                Some((turns - turns.floor()) as f32)
            }
        }
    }

    fn map_colors(&self, f: impl Fn(Rgba) -> Rgba) -> Self {
        let mut g = self.clone();
        for s in &mut g.stops {
            s.color = f(s.color);
        }
        g
    }
}

/// What a [`DrawOp::Shape`] is painted with.
#[derive(Clone, Debug, PartialEq)]
pub enum Brush {
    /// One colour.
    Solid(Rgba),
    /// A linear, radial or conical gradient.
    Gradient(Arc<BrushGradient>),
}

impl Brush {
    /// Multiply the brush's alpha by `opacity`.
    #[must_use]
    pub fn with_opacity(&self, opacity: f32) -> Self {
        match self {
            Self::Solid(c) => Self::Solid(c.with_opacity(opacity)),
            Self::Gradient(g) => {
                Self::Gradient(Arc::new(g.map_colors(|c| c.with_opacity(opacity))))
            }
        }
    }

    /// True when the brush cannot produce visible pixels.
    pub fn is_invisible(&self) -> bool {
        match self {
            Self::Solid(c) => c.is_transparent(),
            Self::Gradient(g) => g.stops.iter().all(|s| s.color.is_transparent()),
        }
    }
}

impl From<Rgba> for Brush {
    fn from(c: Rgba) -> Self {
        Self::Solid(c)
    }
}

/// Which points a path encloses.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum FillRule {
    /// A point is inside when the path winds around it.
    #[default]
    NonZero,
    /// A point is inside when a ray from it crosses the path an odd number
    /// of times.
    EvenOdd,
}

/// Fill or outline for [`DrawOp::Shape`].
#[derive(Clone, Debug, PartialEq)]
pub enum ShapeStyle {
    /// Fill what the path encloses. An open subpath is closed first.
    Fill(FillRule),
    /// Draw the outline: width, joins, caps, miter limit and dashes as
    /// kurbo's [`Stroke`] has them (the miter limit is the SVG one).
    Stroke(Arc<Stroke>),
}

impl ShapeStyle {
    /// How far the ink can reach outside the path's own bounds.
    pub fn reach(&self) -> f64 {
        match self {
            Self::Fill(_) => 0.0,
            Self::Stroke(s) => {
                let mut k: f64 = 1.0;
                if s.join == Join::Miter {
                    k = k.max(s.miter_limit);
                }
                if s.start_cap == Cap::Square || s.end_cap == Cap::Square {
                    k = k.max(std::f64::consts::SQRT_2);
                }
                s.width.abs() / 2.0 * k
            }
        }
    }
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
        /// Optional inside border. The fill and the border are painted side
        /// by side, as Qt paints them: a border that lets light through
        /// shows what is behind the rectangle, not the fill (see
        /// [`fill_area`]). A gradient fill still runs over the whole
        /// `rect`.
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
    /// A rectangle of an image drawn into a rectangle of the node.
    Image {
        /// The pixels.
        image: ImageHandle,
        /// Source rectangle in image pixels.
        src: Rect,
        /// The area painted. Without `tile`, `src` is scaled to fill it.
        dst: Rect,
        /// Where one copy of `src` lies. Copies repeat at its width and
        /// height in both directions and are cut off at `dst`.
        tile: Option<Rect>,
        /// Flip the content of every copy left-right and top-bottom. The
        /// copies stay where they are.
        mirror: [bool; 2],
        /// Smooth (bilinear) sampling; otherwise nearest neighbour.
        smooth: bool,
        /// How opaque the picture is drawn, within the node's own opacity:
        /// 1 shows it as it is. One node can draw many pictures, each as
        /// faded as it should be (a particle system's).
        opacity: f32,
    },
    /// An image cut into nine parts: the corners keep their size, the edges
    /// and the middle stretch or repeat to fill `dst`.
    NinePatch {
        /// The pixels.
        image: ImageHandle,
        /// Border sizes in image pixels.
        insets: Insets,
        /// The area painted.
        dst: Rect,
        /// How the middle column fills sideways and the middle row downwards.
        tile: [TileMode; 2],
        /// Flip the whole picture left-right.
        mirror: bool,
        /// Smooth (bilinear) sampling; otherwise nearest neighbour.
        smooth: bool,
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
    /// A vector shape with the whole of a stroke's style and any [`Brush`].
    /// Both backends draw it anti-aliased; [`DrawOp::Path`] stays for the
    /// plain cases and means what it did.
    Shape {
        /// Geometry.
        path: Arc<BezPath>,
        /// Fill or outline.
        style: ShapeStyle,
        /// What it is painted with.
        brush: Brush,
    },
    /// A `ShaderEffect`'s programs over `rect`. Only a GPU painter draws
    /// it; the others draw nothing (see [`crate::ShaderOp`]).
    Shader {
        /// The item's rectangle, where the grid lies.
        rect: Rect,
        op: Arc<crate::shader::ShaderOp>,
    },
    /// Foreign surface (embedded egui widgets, video, ...). Splits batches.
    Embed {
        /// Surface id.
        surface: EmbedId,
        /// Placement rectangle.
        rect: Rect,
    },
}

/// Where the fill of a [`DrawOp::Rect`] is painted, as a rectangle and its
/// radii: inside the border. `None` when the border leaves no room.
///
/// Under a border that hides it the fill is painted all the same, over the
/// whole rectangle: nobody can see it, and two anti-aliased edges that meet
/// would let the backdrop through where one shape painted over the other
/// does not. `opacity` is what the node is faded by; a backend that fades
/// after it has built its shapes passes 1.
pub fn fill_area(
    rect: Rect,
    radii: [f32; 4],
    border: Option<&Border>,
    opacity: f32,
) -> Option<(Rect, [f32; 4])> {
    let drawn = border.filter(|b| {
        b.width > 0.0 && !b.paint.is_invisible() && !matches!(b.paint, Paint::Image(_))
    });
    let Some(b) = drawn else {
        return Some((rect, radii));
    };
    if b.paint.is_opaque() && opacity >= 1.0 {
        return Some((rect, radii));
    }
    let inner = rect.inset(-f64::from(b.width));
    (inner.width() > 0.0 && inner.height() > 0.0)
        .then(|| (inner, radii.map(|r| (r - b.width).max(0.0))))
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
            Self::Text { rect, .. } | Self::Embed { rect, .. } | Self::Shader { rect, .. } => *rect,
            Self::Shape { path, style, .. } => {
                let r = style.reach();
                path.bounding_box().inflate(r, r)
            }
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

    /// A shape filled or outlined with a brush (see [`DrawOp::Shape`]).
    pub fn shape(&mut self, path: Arc<BezPath>, style: ShapeStyle, brush: Brush) -> &mut Self {
        self.push(DrawOp::Shape { path, style, brush })
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

    /// The whole of `image` scaled into `dst`, smoothly.
    pub fn image(&mut self, image: ImageHandle, dst: Rect) -> &mut Self {
        let src = image.rect();
        self.push(DrawOp::Image {
            image,
            src,
            dst,
            tile: None,
            mirror: [false, false],
            smooth: true,
            opacity: 1.0,
        })
    }

    /// A shader effect over `rect`.
    pub fn shader(&mut self, rect: Rect, op: Arc<crate::shader::ShaderOp>) -> &mut Self {
        self.push(DrawOp::Shader { rect, op })
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
