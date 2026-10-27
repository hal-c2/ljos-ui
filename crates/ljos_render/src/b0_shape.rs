//! [`DrawOp::Shape`](crate::DrawOp::Shape) on B0.
//!
//! A shape is drawn in one of two ways, chosen by what was measured (the
//! numbers are in ARCHITECTURE.md section 7.1):
//!
//! * **Triangles**, the default. The path is flattened, a stroke is turned
//!   into its outline by kurbo (so joins, caps and dashes are the ones R
//!   draws), lyon fills the outline, and every edge on the boundary gets a
//!   strip one device pixel wide whose coverage runs from one to zero. The
//!   triangles are kept in the node's own coordinates: a move, a rotation or
//!   a fade maps them again without cutting the path again. A linear gradient
//!   is exact, since each triangle is cut where the colour bends.
//! * **Pixels**: vello_cpu fills the shape at device resolution into a
//!   texture that is drawn as one quad. This is how a radial or conical
//!   gradient is drawn (three colours per triangle cannot carry one), a
//!   linear gradient that repeats too often to cut, and a stroke with so
//!   many segments that its outline overlaps itself all over, which lyon
//!   takes ten times longer over than vello_cpu does. The texture is filled
//!   again when the shape lands differently on the pixel grid, and is not
//!   kept while the shape is out of sight.
//!
//! Neither is built before the shape is first inside its clip rectangle.

use rustc_hash::FxHashMap as HashMap;
use std::sync::Arc;

use egui::epaint::{Mesh, Vertex, WHITE_UV};
use egui::{
    Color32, ColorImage, Context, Pos2, Rect as ERect, TextureFilter, TextureHandle,
    TextureOptions, TextureWrapMode,
};
use lyon_tessellation::math::point;
use lyon_tessellation::path::Path as LyonPath;
use lyon_tessellation::{
    BuffersBuilder, FillOptions, FillRule as LyonRule, FillTessellator, FillVertex, VertexBuffers,
};
use vello_cpu::kurbo::{self, Affine, BezPath, PathEl, Point, Rect};
use vello_cpu::{
    Level, Pixmap, RasterizerSettings, RenderContext, RenderMode, RenderSettings, Resources,
};

use crate::b0::{color32, erect, pos2};
use crate::display_list::{Brush, BrushGradient, FillRule, GradientGeometry, ShapeStyle, Spread};
use crate::rgba::Rgba;
use crate::shape::{
    Resolved, draw_shape, resolve, single_color, stroke_is_visible, stroke_outline,
};

/// How far the triangles may lie from the true curve, in device pixels at
/// the top of a scale bucket (so between this and half of it).
const MESH_TOLERANCE: f64 = 0.1;

/// A stroke with more straight pieces than this is drawn from pixels.
const DENSE_SEGMENTS: usize = 1500;

/// A repeating linear gradient with more periods than this over the shape is
/// drawn from pixels: every period cuts every triangle it crosses.
const MAX_PERIODS: f32 = 16.0;

/// The longest side of a shape texture, and the most pixels in one.
const MAX_SIDE: f64 = 8192.0;
const MAX_PIXELS: f64 = 16.0 * 1024.0 * 1024.0;

/// What the shape textures of one frame may hold together (the desktop
/// budget for GPU caches in ARCHITECTURE.md section 8.5).
const TEXTURE_BUDGET: usize = 64 * 1024 * 1024;

/// Texels and device pixels coincide, so there is nothing to filter.
const TEXTURE_OPTIONS: TextureOptions = TextureOptions {
    magnification: TextureFilter::Nearest,
    minification: TextureFilter::Nearest,
    wrap_mode: TextureWrapMode::ClampToEdge,
    mipmap_mode: None,
};

/// One shape of a display list, in the node's coordinates.
#[derive(Clone)]
pub(crate) struct ShapeCmd {
    path: Arc<BezPath>,
    style: ShapeStyle,
    brush: Brush,
    /// Everything the shape can paint.
    bounds: Rect,
    /// Triangles cannot draw this brush.
    pixels_only: bool,
}

impl ShapeCmd {
    /// `None` when the shape paints nothing.
    pub(crate) fn new(path: &Arc<BezPath>, style: &ShapeStyle, brush: &Brush) -> Option<Self> {
        let gradient = match resolve(brush) {
            Resolved::Nothing => return None,
            Resolved::Solid(_) => None,
            Resolved::Gradient(g) => Some(g),
        };
        if let ShapeStyle::Stroke(s) = style
            && !stroke_is_visible(s)
        {
            return None;
        }
        if path.elements().is_empty() {
            return None;
        }
        let reach = style.reach();
        let bounds = kurbo::Shape::bounding_box(&**path).inflate(reach, reach);
        if !(bounds.x0.is_finite()
            && bounds.y0.is_finite()
            && bounds.x1.is_finite()
            && bounds.y1.is_finite())
        {
            return None;
        }
        let pixels_only = gradient.is_some_and(|g| match g.geometry {
            GradientGeometry::Radial { .. } | GradientGeometry::Conical { .. } => true,
            GradientGeometry::Linear { .. } => {
                g.spread != Spread::Pad && periods_over(g, bounds) > MAX_PERIODS
            }
        });
        Some(Self {
            path: Arc::clone(path),
            style: style.clone(),
            brush: brush.clone(),
            bounds,
            pixels_only,
        })
    }

    /// True when both draw the same thing, so what was built for one serves
    /// the other.
    pub(crate) fn same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.path, &other.path)
            && self.style == other.style
            && self.brush == other.brush
    }

    fn key(&self) -> usize {
        Arc::as_ptr(&self.path) as usize
    }
}

/// How many times a linear gradient runs over `bounds`.
fn periods_over(g: &BrushGradient, bounds: Rect) -> f32 {
    let ts = [
        Point::new(bounds.x0, bounds.y0),
        Point::new(bounds.x1, bounds.y0),
        Point::new(bounds.x1, bounds.y1),
        Point::new(bounds.x0, bounds.y1),
    ]
    .map(|p| g.param(p).unwrap_or(0.0));
    let lo = ts.iter().copied().fold(f32::INFINITY, f32::min);
    let hi = ts.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    hi - lo
}

/// What a texture was filled for.
#[derive(Clone, Copy, PartialEq)]
struct RasterKey {
    /// The device transform without its translation.
    linear: [f64; 4],
    /// Where the shape lies within a device pixel, in eighths.
    offset: [i32; 2],
    size: [usize; 2],
}

#[derive(Clone)]
struct HeldTexture {
    key: RasterKey,
    handle: TextureHandle,
    bytes: usize,
}

/// A shape placed on the screen. What egui draws is built when the shape is
/// first inside its clip rectangle.
#[derive(Clone)]
pub(crate) struct ShapeSeg {
    cmd: ShapeCmd,
    to_screen: Affine,
    opacity: f32,
    mesh: Option<Arc<Mesh>>,
    texture: Option<HeldTexture>,
    /// The shape could not be drawn the way it should be.
    approximated: bool,
    last_drawn: u64,
}

impl ShapeSeg {
    /// Its mesh, if it is one, and the bytes of its texture, if it has one.
    pub(crate) fn held(&self) -> (Option<&Arc<Mesh>>, usize) {
        (
            self.mesh.as_ref(),
            self.texture.as_ref().map_or(0, |t| t.bytes),
        )
    }

    pub(crate) fn new(cmd: ShapeCmd, to_screen: Affine, opacity: f32) -> Self {
        Self {
            cmd,
            to_screen,
            opacity,
            mesh: None,
            texture: None,
            approximated: false,
            last_drawn: 0,
        }
    }

    /// True when this draws what `old` drew, perhaps somewhere else.
    pub(crate) fn continues(&self, old: &Self) -> bool {
        self.texture.is_none() && self.cmd.same(&old.cmd)
    }

    /// Take over the texture of the segment this one replaces, so a shape
    /// that only moved by whole pixels is not filled again and one that is
    /// filled again does not allocate.
    pub(crate) fn inherit(&mut self, old: &mut Self) {
        self.texture = old.texture.take();
    }

    /// True when the shape was inside its clip rectangle in `frame`.
    pub(crate) fn drawn_in(&self, frame: u64) -> bool {
        self.last_drawn == frame
    }

    /// Let go of what was built; it is built again when the shape is next
    /// seen.
    pub(crate) fn forget(&mut self) {
        self.mesh = None;
        self.texture = None;
    }

    pub(crate) fn has_texture(&self) -> bool {
        self.texture.is_some()
    }

    pub(crate) fn cmd(&self) -> &ShapeCmd {
        &self.cmd
    }
}

/// Triangles of one shape in the node's coordinates. A vertex ends up at
/// `pos + dir * half a device pixel`, which is what keeps the soft edge one
/// pixel wide at every scale.
struct Geom {
    pos: Vec<[f32; 2]>,
    dir: Vec<[f32; 2]>,
    color: Vec<Color32>,
    idx: Vec<u32>,
    /// The brush was replaced by one colour.
    approximated: bool,
}

impl Geom {
    fn mesh(&self, to_screen: Affine, half_pixel: f64, opacity: f32) -> Mesh {
        let mut mesh = Mesh {
            indices: self.idx.clone(),
            vertices: Vec::with_capacity(self.pos.len()),
            texture_id: egui::TextureId::default(),
        };
        let h = half_pixel as f32;
        for ((pos, dir), color) in self.pos.iter().zip(&self.dir).zip(&self.color) {
            let p = to_screen
                * Point::new(
                    f64::from(pos[0] + dir[0] * h),
                    f64::from(pos[1] + dir[1] * h),
                );
            mesh.vertices.push(Vertex {
                pos: pos2(p),
                uv: WHITE_UV,
                color: if opacity < 1.0 {
                    color.gamma_multiply(opacity)
                } else {
                    *color
                },
            });
        }
        mesh
    }
}

struct GeomEntry {
    cmd: ShapeCmd,
    /// The power of two the device scale was rounded up to.
    bucket: i32,
    /// Straight pieces in the stroked path, counted once.
    segments: Option<usize>,
    geom: Option<Arc<Geom>>,
}

/// What B0 keeps for shapes besides what each node holds.
pub(crate) struct ShapeCache {
    /// By the address of the path; the entry holds the path, so the address
    /// is not reused while the entry lives.
    geoms: HashMap<usize, Vec<GeomEntry>>,
    raster: Option<RenderContext>,
    resources: Resources,
    tessellated: u64,
    rasterised: u64,
    frame_texture_bytes: usize,
}

impl Default for ShapeCache {
    fn default() -> Self {
        Self {
            geoms: HashMap::default(),
            raster: None,
            resources: Resources::new(),
            tessellated: 0,
            rasterised: 0,
            frame_texture_bytes: 0,
        }
    }
}

impl ShapeCache {
    /// How many shapes were cut into triangles so far.
    pub(crate) fn tessellated(&self) -> u64 {
        self.tessellated
    }

    /// How many shape textures were filled so far.
    pub(crate) fn rasterised(&self) -> u64 {
        self.rasterised
    }

    /// How many sets of triangles are held.
    pub(crate) fn geometries(&self) -> usize {
        self.geoms
            .values()
            .flatten()
            .filter(|e| e.geom.is_some())
            .count()
    }

    pub(crate) fn begin_frame(&mut self) {
        self.frame_texture_bytes = 0;
    }

    pub(crate) fn clear(&mut self) {
        self.geoms.clear();
        self.raster = None;
        self.resources = Resources::new();
    }

    /// Drop what is kept for `cmd`.
    pub(crate) fn release(&mut self, cmd: &ShapeCmd) {
        if let Some(entries) = self.geoms.get_mut(&cmd.key()) {
            entries.retain(|e| !e.cmd.same(cmd));
            if entries.is_empty() {
                self.geoms.remove(&cmd.key());
            }
        }
    }

    /// Drop the triangles of paths only this cache holds. A node whose
    /// render node was replaced leaves without handing its shapes over, so
    /// nothing else would ever let them go.
    pub(crate) fn forget_orphans(&mut self) {
        self.geoms.retain(|_, entries| {
            entries.retain(|e| Arc::strong_count(&e.cmd.path) > 1);
            !entries.is_empty()
        });
    }

    /// Keep only the triangles of the shapes `keep` says yes to, and no
    /// scratch memory.
    pub(crate) fn retain(&mut self, keep: impl Fn(&ShapeCmd) -> bool) {
        self.geoms.retain(|_, entries| {
            entries.retain(|e| keep(&e.cmd));
            !entries.is_empty()
        });
        self.raster = None;
        self.resources = Resources::new();
    }

    fn entry(&mut self, cmd: &ShapeCmd, bucket: i32) -> &mut GeomEntry {
        let entries = self.geoms.entry(cmd.key()).or_default();
        // One scale at a time: a shape that is zoomed does not leave its
        // triangles of every size behind.
        entries.retain(|e| !(e.cmd.same(cmd) && e.bucket != bucket));
        let at = match entries.iter().position(|e| e.cmd.same(cmd)) {
            Some(at) => at,
            None => {
                entries.push(GeomEntry {
                    cmd: cmd.clone(),
                    bucket,
                    segments: None,
                    geom: None,
                });
                entries.len() - 1
            }
        };
        &mut entries[at]
    }

    /// The mesh to hand to egui for `seg` this frame, if any of it shows
    /// inside `clip`.
    pub(crate) fn draw(
        &mut self,
        seg: &mut ShapeSeg,
        clip: ERect,
        ppp: f32,
        ctx: Option<&Context>,
        frame: u64,
        warnings: &mut u32,
    ) -> Option<Arc<Mesh>> {
        let on_screen = erect(seg.to_screen.transform_rect_bbox(seg.cmd.bounds)).expand(1.0 / ppp);
        if !on_screen.intersects(clip) {
            // Pixels nobody sees are megabytes; the triangles are small and
            // stay until memory is asked for.
            if seg.texture.take().is_some() {
                seg.mesh = None;
            }
            return None;
        }
        seg.last_drawn = frame;
        if seg.mesh.is_none() {
            self.build(seg, ppp, ctx);
        }
        if let Some(t) = &seg.texture {
            self.frame_texture_bytes += t.bytes;
        }
        if seg.approximated {
            *warnings += 1;
        }
        seg.mesh.clone().filter(|m| !m.is_empty())
    }

    fn build(&mut self, seg: &mut ShapeSeg, ppp: f32, ctx: Option<&Context>) {
        let device = Affine::scale(f64::from(ppp)) * seg.to_screen;
        let scale = device.determinant().abs().sqrt();
        if !(scale.is_finite() && scale > 1e-6) {
            seg.mesh = Some(Arc::new(Mesh::default()));
            return;
        }
        let bucket = (scale.log2().ceil() as i32).clamp(-24, 24);
        let tolerance = MESH_TOLERANCE / 2f64.powi(bucket);
        seg.approximated = false;
        if seg.cmd.pixels_only || self.is_dense(&seg.cmd, bucket, tolerance) {
            match self.texture(seg, device, scale, ppp, ctx) {
                Ok(()) => return,
                Err(why) => {
                    seg.approximated = true;
                    tracing::debug!("B0: {why}; the shape is drawn as triangles");
                }
            }
        }
        seg.texture = None;
        let geom = self.geom(&seg.cmd, bucket, tolerance);
        seg.approximated |= geom.approximated;
        seg.mesh = Some(Arc::new(geom.mesh(seg.to_screen, 0.5 / scale, seg.opacity)));
    }

    fn is_dense(&mut self, cmd: &ShapeCmd, bucket: i32, tolerance: f64) -> bool {
        let ShapeStyle::Stroke(stroke) = &cmd.style else {
            return false;
        };
        let entry = self.entry(cmd, bucket);
        let segments = *entry.segments.get_or_insert_with(|| {
            let (mut pieces, mut length) = (0usize, 0.0f64);
            let mut last = Point::ZERO;
            kurbo::flatten(
                cmd.path.elements().iter().copied(),
                tolerance,
                |el| match el {
                    PathEl::MoveTo(p) => last = p,
                    PathEl::LineTo(p) => {
                        pieces += 1;
                        length += (p - last).hypot();
                        last = p;
                    }
                    _ => {}
                },
            );
            let period: f64 = stroke.dash_pattern.iter().sum();
            if period > 0.0 && length.is_finite() {
                // Every dash is an outline of its own.
                let dashes = length / period * (stroke.dash_pattern.len() as f64 / 2.0).max(1.0);
                pieces = pieces.saturating_add(dashes.min(1e9) as usize);
            }
            pieces
        });
        segments > DENSE_SEGMENTS
    }

    fn geom(&mut self, cmd: &ShapeCmd, bucket: i32, tolerance: f64) -> Arc<Geom> {
        if let Some(g) = &self.entry(cmd, bucket).geom {
            return Arc::clone(g);
        }
        self.tessellated += 1;
        // The soft edge is half a device pixel either way; at the bottom of
        // the bucket that is this much in the node's coordinates.
        let nominal_half_pixel = 0.5 / 2f32.powi(bucket);
        let geom = Arc::new(build_geom(cmd, tolerance, nominal_half_pixel));
        self.entry(cmd, bucket).geom = Some(Arc::clone(&geom));
        geom
    }

    fn texture(
        &mut self,
        seg: &mut ShapeSeg,
        device: Affine,
        scale: f64,
        ppp: f32,
        ctx: Option<&Context>,
    ) -> Result<(), &'static str> {
        let ctx = ctx.ok_or("a shape drawn from pixels needs an egui Context")?;
        let b = device.transform_rect_bbox(seg.cmd.bounds);
        // One pixel of margin: the edge's coverage reaches past the bounds.
        let (x0, y0) = (b.x0.floor() - 1.0, b.y0.floor() - 1.0);
        let (x1, y1) = (b.x1.ceil() + 1.0, b.y1.ceil() + 1.0);
        let (w, h) = (x1 - x0, y1 - y0);
        let max_side = (crate::b0::max_texture_side(ctx) as f64).min(MAX_SIDE);
        if !(w >= 1.0 && h >= 1.0 && w <= max_side && h <= max_side && w * h <= MAX_PIXELS) {
            return Err("a shape is too large for a texture");
        }
        let (w, h) = (w as usize, h as usize);
        let bytes = w * h * 4;
        if self.frame_texture_bytes + bytes > TEXTURE_BUDGET {
            return Err("the shape textures of this frame are over their budget");
        }
        let c = device.as_coeffs();
        let (dx, dy) = (c[4] - x0, c[5] - y0);
        let key = RasterKey {
            linear: [c[0], c[1], c[2], c[3]],
            offset: [(dx * 8.0).round() as i32, (dy * 8.0).round() as i32],
            size: [w, h],
        };
        if seg.texture.as_ref().is_none_or(|t| t.key != key) {
            let to_texture = Affine::new([c[0], c[1], c[2], c[3], dx, dy]);
            let pixels = self.rasterise(&seg.cmd, to_texture, w as u16, h as u16, scale);
            let image = ColorImage::from_rgba_premultiplied([w, h], &pixels);
            match &mut seg.texture {
                Some(t) => {
                    t.handle.set(image, TEXTURE_OPTIONS);
                    t.key = key;
                    t.bytes = bytes;
                }
                None => {
                    seg.texture = Some(HeldTexture {
                        key,
                        handle: crate::b0::upload(ctx, "ljos-shape", image, TEXTURE_OPTIONS),
                        bytes,
                    });
                }
            }
            self.rasterised += 1;
        }
        let texture = seg.texture.as_ref().expect("just set");
        let inv = 1.0 / f64::from(ppp);
        let at = |x: f64, y: f64| Pos2::new((x * inv) as f32, (y * inv) as f32);
        let color = if seg.opacity < 1.0 {
            Color32::WHITE.gamma_multiply(seg.opacity)
        } else {
            Color32::WHITE
        };
        let corners = [
            (x0, y0, 0.0, 0.0),
            (x1, y0, 1.0, 0.0),
            (x1, y1, 1.0, 1.0),
            (x0, y1, 0.0, 1.0),
        ];
        seg.mesh = Some(Arc::new(Mesh {
            indices: vec![0, 1, 2, 0, 2, 3],
            vertices: corners
                .iter()
                .map(|(x, y, u, v)| Vertex {
                    pos: at(*x, *y),
                    uv: Pos2::new(*u, *v),
                    color,
                })
                .collect(),
            texture_id: texture.handle.id(),
        }));
        Ok(())
    }

    /// Premultiplied RGBA of `cmd` under `to_texture`.
    fn rasterise(
        &mut self,
        cmd: &ShapeCmd,
        to_texture: Affine,
        w: u16,
        h: u16,
        scale: f64,
    ) -> Vec<u8> {
        let mut ctx = match self.raster.take() {
            Some(mut ctx) => {
                ctx.reset_and_resize(w, h);
                ctx
            }
            None => RenderContext::new_with(
                w,
                h,
                RenderSettings {
                    // The same arithmetic as R, so a shape drawn from pixels
                    // is the reference picture.
                    level: Level::baseline(),
                    num_threads: 0,
                },
            ),
        };
        ctx.set_transform(to_texture);
        draw_shape(&mut ctx, &cmd.path, &cmd.style, &cmd.brush, 1.0, scale);
        ctx.flush();
        let mut pixmap = Pixmap::new(w, h);
        ctx.render_with(
            &mut pixmap,
            &mut self.resources,
            RasterizerSettings {
                render_mode: RenderMode::OptimizeSpeed,
                ..Default::default()
            },
        );
        self.raster = Some(ctx);
        pixmap.data_as_u8_slice().to_vec()
    }
}

/// Triangles with a direction per vertex: the boundary's inner copies move
/// inwards along it and the outer copies outwards.
#[derive(Default)]
struct Feathered {
    pos: Vec<[f32; 2]>,
    dir: Vec<[f32; 2]>,
    cov: Vec<f32>,
    idx: Vec<u32>,
}

fn build_geom(cmd: &ShapeCmd, tolerance: f64, nominal_half_pixel: f32) -> Geom {
    let (outline, rule) = match &cmd.style {
        ShapeStyle::Fill(rule) => (None, *rule),
        ShapeStyle::Stroke(stroke) => (
            Some(stroke_outline(&cmd.path, stroke, tolerance)),
            FillRule::NonZero,
        ),
    };
    let outline = outline.as_ref().unwrap_or(&cmd.path);
    let feathered = tessellate(outline, rule, tolerance)
        .map(|buffers| feather(&buffers))
        .unwrap_or_default();
    let mut approximated = false;
    let (pos, dir, color, idx) = match resolve(&cmd.brush) {
        Resolved::Nothing => (Vec::new(), Vec::new(), Vec::new(), Vec::new()),
        Resolved::Solid(c) => solid(feathered, c),
        Resolved::Gradient(g) => match g.geometry {
            GradientGeometry::Linear { .. } => linear(&feathered, g, nominal_half_pixel),
            GradientGeometry::Radial { .. } | GradientGeometry::Conical { .. } => {
                approximated = true;
                solid(feathered, single_color(g))
            }
        },
    };
    Geom {
        pos,
        dir,
        color,
        idx,
        approximated,
    }
}

type Colored = (Vec<[f32; 2]>, Vec<[f32; 2]>, Vec<Color32>, Vec<u32>);

fn solid(f: Feathered, c: Rgba) -> Colored {
    let full = color32(c);
    let color = f
        .cov
        .iter()
        .map(|cov| {
            if *cov >= 1.0 {
                full
            } else {
                color32(c.with_opacity(*cov))
            }
        })
        .collect();
    (f.pos, f.dir, color, f.idx)
}

/// Fill `outline` with triangles. `None` when lyon gives up on it.
fn tessellate(
    outline: &BezPath,
    rule: FillRule,
    tolerance: f64,
) -> Option<VertexBuffers<[f32; 2], u32>> {
    let mut b = LyonPath::builder();
    let mut open = false;
    let mut finite = true;
    kurbo::flatten(outline.elements().iter().copied(), tolerance, |el| {
        let mut at = |p: Point| {
            finite &= p.x.is_finite() && p.y.is_finite();
            point(p.x as f32, p.y as f32)
        };
        match el {
            PathEl::MoveTo(p) => {
                let p = at(p);
                if open {
                    b.end(true);
                }
                if finite {
                    b.begin(p);
                    open = true;
                } else {
                    open = false;
                }
            }
            PathEl::LineTo(p) => {
                let p = at(p);
                if !finite {
                    return;
                }
                if open {
                    b.line_to(p);
                } else {
                    b.begin(p);
                    open = true;
                }
            }
            PathEl::ClosePath if open => {
                b.end(true);
                open = false;
            }
            _ => {}
        }
    });
    if open {
        b.end(true);
    }
    if !finite {
        return None;
    }
    let path = b.build();
    let mut out: VertexBuffers<[f32; 2], u32> = VertexBuffers::new();
    let options = FillOptions::tolerance(tolerance as f32).with_fill_rule(match rule {
        FillRule::NonZero => LyonRule::NonZero,
        FillRule::EvenOdd => LyonRule::EvenOdd,
    });
    FillTessellator::new()
        .tessellate_path(
            &path,
            &options,
            &mut BuffersBuilder::new(&mut out, |v: FillVertex<'_>| {
                let p = v.position();
                [p.x, p.y]
            }),
        )
        .ok()?;
    Some(out)
}

/// Give every vertex on the boundary an inner and an outer copy, joined by a
/// strip that carries the coverage from one to zero.
fn feather(src: &VertexBuffers<[f32; 2], u32>) -> Feathered {
    let n = src.vertices.len();
    let mut out = Feathered {
        pos: src.vertices.clone(),
        dir: vec![[0.0; 2]; n],
        cov: vec![1.0; n],
        idx: src.indices.clone(),
    };
    // An edge that belongs to one triangle only lies on the boundary. Kept
    // per edge: its two ends in order, the triangle's third corner, and how
    // many triangles use it.
    let mut edges: HashMap<(u32, u32), (u32, u32, u32, u8)> =
        HashMap::with_capacity_and_hasher(src.indices.len(), Default::default());
    for t in src.indices.as_chunks::<3>().0 {
        for k in 0..3 {
            let (a, b, c) = (t[k], t[(k + 1) % 3], t[(k + 2) % 3]);
            edges
                .entry((a.min(b), a.max(b)))
                .and_modify(|e| e.3 = e.3.saturating_add(1))
                .or_insert((a, b, c, 1));
        }
    }
    let mut normal_sum = vec![[0.0f32; 2]; n];
    let mut first = vec![[0.0f32; 2]; n];
    let mut count = vec![0u8; n];
    let mut boundary: Vec<(u32, u32)> = Vec::new();
    for (a, b, c, uses) in edges.into_values() {
        if uses != 1 {
            continue;
        }
        let (pa, pb, pc) = (
            src.vertices[a as usize],
            src.vertices[b as usize],
            src.vertices[c as usize],
        );
        let (ex, ey) = (pb[0] - pa[0], pb[1] - pa[1]);
        let len = (ex * ex + ey * ey).sqrt();
        if len <= 0.0 {
            continue;
        }
        let (mut nx, mut ny) = (ey / len, -ex / len);
        // Away from the triangle's third corner is out of the shape.
        if nx * (pc[0] - pa[0]) + ny * (pc[1] - pa[1]) > 0.0 {
            nx = -nx;
            ny = -ny;
        }
        for v in [a, b] {
            let v = v as usize;
            if count[v] == 0 {
                first[v] = [nx, ny];
            }
            normal_sum[v][0] += nx;
            normal_sum[v][1] += ny;
            count[v] = count[v].saturating_add(1);
        }
        boundary.push((a, b));
    }
    // HashMap order is not stable between runs; the picture should be.
    boundary.sort_unstable();
    let mut outer = vec![u32::MAX; n];
    for v in 0..n {
        if count[v] == 0 {
            continue;
        }
        let s = normal_sum[v];
        let len = (s[0] * s[0] + s[1] * s[1]).sqrt();
        let d = if len < 1e-4 {
            [0.0, 0.0]
        } else {
            let unit = [s[0] / len, s[1] / len];
            // A miter, so the strip is a pixel wide across each edge and not
            // across the corner. A sharp corner is cut short.
            let cos_half = (unit[0] * first[v][0] + unit[1] * first[v][1])
                .abs()
                .max(0.4);
            [unit[0] / cos_half, unit[1] / cos_half]
        };
        out.dir[v] = [-d[0], -d[1]];
        outer[v] = out.pos.len() as u32;
        out.pos.push(src.vertices[v]);
        out.dir.push(d);
        out.cov.push(0.0);
    }
    for (a, b) in boundary {
        let (ao, bo) = (outer[a as usize], outer[b as usize]);
        out.idx.extend([a, b, bo, a, bo, ao]);
    }
    out
}

#[derive(Clone, Copy)]
struct Cut {
    pos: [f32; 2],
    dir: [f32; 2],
    cov: f32,
    t: f32,
}

fn mix(a: Cut, b: Cut, k: f32) -> Cut {
    let l = |x: f32, y: f32| x + (y - x) * k;
    Cut {
        pos: [l(a.pos[0], b.pos[0]), l(a.pos[1], b.pos[1])],
        dir: [l(a.dir[0], b.dir[0]), l(a.dir[1], b.dir[1])],
        cov: l(a.cov, b.cov),
        t: l(a.t, b.t),
    }
}

/// The part of `poly` with `t >= bound` (`keep_ge`) or `t <= bound`.
fn clip_poly(poly: &[Cut], bound: f32, keep_ge: bool) -> Vec<Cut> {
    let inside = |v: &Cut| if keep_ge { v.t >= bound } else { v.t <= bound };
    let mut out = Vec::with_capacity(poly.len() + 2);
    for i in 0..poly.len() {
        let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
        match (inside(&a), inside(&b)) {
            (true, true) => out.push(b),
            (true, false) => out.push(mix(a, b, (bound - a.t) / (b.t - a.t))),
            (false, true) => {
                out.push(mix(a, b, (bound - a.t) / (b.t - a.t)));
                out.push(b);
            }
            (false, false) => {}
        }
    }
    out
}

/// Where the colour of `g` bends or jumps strictly between `lo` and `hi`.
fn bends(g: &BrushGradient, lo: f32, hi: f32, out: &mut Vec<f32>) {
    out.clear();
    let mut push = |t: f32| {
        if t > lo && t < hi {
            out.push(t);
        }
    };
    match g.spread {
        Spread::Pad => g.stops.iter().for_each(|s| push(s.offset)),
        // More periods than this in one triangle is past what the eye
        // resolves, and the shape should have been drawn from pixels.
        Spread::Repeat | Spread::Reflect if hi - lo <= 64.0 => {
            for k in lo.floor() as i32..=hi.floor() as i32 {
                let base = k as f32;
                push(base);
                let back = g.spread == Spread::Reflect && k.rem_euclid(2) == 1;
                for s in &g.stops {
                    push(base + if back { 1.0 - s.offset } else { s.offset });
                }
            }
        }
        Spread::Repeat | Spread::Reflect => {}
    }
    out.sort_by(f32::total_cmp);
    out.dedup();
}

/// Colour the triangles with a linear gradient. Each is cut where the colour
/// bends, so the colour is linear inside every piece and three vertex colours
/// carry it exactly.
fn linear(f: &Feathered, g: &BrushGradient, nominal_half_pixel: f32) -> Colored {
    let (mut pos, mut dir, mut color, mut idx) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut cuts = Vec::new();
    for tri in f.idx.as_chunks::<3>().0 {
        let corners = tri.map(|i| {
            let i = i as usize;
            // Measured where the vertex ends up, or the two sides of a soft
            // edge that lies on a jump of the gradient cannot be told apart.
            let at = Point::new(
                f64::from(f.pos[i][0] + f.dir[i][0] * nominal_half_pixel),
                f64::from(f.pos[i][1] + f.dir[i][1] * nominal_half_pixel),
            );
            Cut {
                pos: f.pos[i],
                dir: f.dir[i],
                cov: f.cov[i],
                t: g.param(at).unwrap_or(0.0),
            }
        });
        let lo = corners.iter().map(|v| v.t).fold(f32::INFINITY, f32::min);
        let hi = corners
            .iter()
            .map(|v| v.t)
            .fold(f32::NEG_INFINITY, f32::max);
        if !(lo.is_finite() && hi.is_finite()) {
            continue;
        }
        bends(g, lo, hi, &mut cuts);
        let mut from = lo;
        for k in 0..=cuts.len() {
            let to = cuts.get(k).copied().unwrap_or(hi);
            let mut poly = corners.to_vec();
            if k > 0 {
                poly = clip_poly(&poly, from, true);
            }
            if k < cuts.len() && !poly.is_empty() {
                poly = clip_poly(&poly, to, false);
            }
            if poly.len() >= 3 {
                // A vertex on a jump takes the colour of the side this
                // piece is on.
                let nudge = (to - from) * 1e-3;
                let base = pos.len() as u32;
                for v in &poly {
                    let t = if to - from > 0.0 {
                        v.t.clamp(from + nudge, to - nudge)
                    } else {
                        v.t
                    };
                    pos.push(v.pos);
                    dir.push(v.dir);
                    color.push(color32(g.color_at(g.spread.fold(t)).with_opacity(v.cov)));
                }
                for k in 1..poly.len() as u32 - 1 {
                    idx.extend([base, base + k, base + k + 1]);
                }
            }
            from = to;
        }
    }
    (pos, dir, color, idx)
}
