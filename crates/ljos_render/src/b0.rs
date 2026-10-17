//! B0: epaint backend (M0 onward). The only place in `ljos_render` that names egui.
//!
//! Every display list is tessellated once into a local-space [`Mesh`] (keyed by
//! [`DisplayListId`], so only a `CONTENT` rebuild invalidates it). Per node we
//! cache the world-space `Arc<Mesh>`; a transform or opacity change re-maps the
//! vertices, an unchanged node re-emits the same `Arc`.
//!
//! Approximations, each counted in [`EncodeStats::warnings`] and logged at
//! debug level:
//!
//! * rotated or rounded clips become their axis-aligned world bounding box;
//! * linear gradients are exact (triangles are split at every stop) but a
//!   gradient *border* uses its first stop colour;
//! * path fills assume convex polygons (egui's `PathShape` limit);
//! * `Embed`, `layer.*` and image fills (`Paint::Image`) are not drawn yet;
//! * a shader effect ([`DrawOp::Shader`]) becomes a paint callback
//!   ([`ShaderPaint`]) only for a host that runs them
//!   ([`EguiSink::shaders`]), and is left out otherwise;
//! * an image needs an egui `Context` (it owns the textures); without one it
//!   is skipped;
//! * an image larger than the GPU's texture limit is drawn from a smaller
//!   copy;
//! * gradient text uses its first stop colour, and colour glyphs (emoji) are
//!   not drawn;
//! * the unshaped `Text` op ignores rotation and shear;
//! * a [`DrawOp::Shape`] that should be drawn from pixels (see below) and
//!   cannot be is drawn as triangles, a radial or conical gradient then in
//!   one colour.
//!
//! Shapes ([`DrawOp::Shape`]) are triangles with a soft edge one device
//! pixel wide, or, for the brushes and sizes triangles do badly, a texture
//! that vello_cpu fills. The private `b0_shape` module has both and says
//! which is used when.
//!
//! Text ([`DrawOp::Glyphs`]) is drawn as quads that sample the glyph atlas
//! (the private `glyph_atlas` module). Upright text is placed on the device
//! pixel grid; a glyph outside the clip rectangle is neither filled nor drawn.
//!
//! Images ([`DrawOp::Image`], [`DrawOp::NinePatch`]) are textured quads. Each
//! image becomes a texture in egui's texture manager the first time it is
//! drawn, one per sampling mode it is used with. The backend holds the image
//! only weakly: the texture goes when the image does, or when it has not been
//! drawn for a while.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use egui::epaint::{
    ClippedShape, CornerRadius, Mesh, PaintCallback, PathShape, PathStroke, RectShape, Shadow,
    Shape, Stroke, StrokeKind, TessellationOptions, Tessellator, TextShape, Vertex,
};
use egui::{
    Color32, ColorImage, Context, FontId, Pos2, Rect as ERect, TextureFilter, TextureHandle,
    TextureId, TextureOptions, TextureWrapMode,
};
use rustc_hash::FxHashMap;
use slotmap::SecondaryMap;
use vello_cpu::kurbo::{Affine, PathEl, Point, Rect, Vec2};

use crate::b0_shape::{ShapeCache, ShapeCmd, ShapeSeg};
use crate::backend::{EncodeStats, FrameView, MemoryPressure, RenderBackend, RgbaImage};
use crate::display_list::{
    Decorations, DisplayList, DisplayListId, DrawOp, GlyphRun, LinearGradient, Paint, PathStyle,
    TextAlign, fill_area,
};
use crate::glyph::{baseline_snap, is_upright, snap_bar};
use crate::glyph_atlas::{BINS, GlyphAtlas, GlyphKey, Slot};
use crate::image::{ImageBlit, ImageHandle, WeakImage, image_blit, nine_patch_blits};
use crate::rgba::Rgba;
use crate::shader::{ShaderOp, Viewport};
use crate::tree::NodeId;

/// Entries unused for this many frames are dropped by [`RenderBackend::sync`].
const KEEP_FRAMES: u64 = 120;

/// Where B0 writes: a flat list of clipped shapes in paint order, plus what it
/// needs to place them.
#[derive(Debug)]
pub struct EguiSink<'a> {
    /// Needed for text: it owns the glyph atlas texture and egui's font
    /// layout. Without it, text is skipped.
    pub ctx: Option<&'a Context>,
    /// Screen position of the snapshot viewport's top-left corner, in points.
    pub origin: Pos2,
    /// Output, back to front.
    pub shapes: Vec<ClippedShape>,
    /// Let an opaque rectangle that is drawn first and covers the whole
    /// viewport be [`Self::backdrop`] instead of a shape. Off by default:
    /// only a host that clears its surface to that colour may turn it on.
    pub fold_backdrop: bool,
    /// The colour of the rectangle folded away, when there was one: what
    /// the host clears the viewport to.
    pub backdrop: Option<Color32>,
    /// Hand over an opaque rectangle with square corners and no border
    /// that lies on whole device pixels as a [`Shape::Rect`] instead of a
    /// mesh. It covers the same pixels either way; a host whose painter
    /// can fill one without blending or sampling (a scissored clear) may
    /// turn it on. Off by default.
    pub solid_rects: bool,
    /// Hand over shader effects as [`Shape::Callback`]s whose callback is
    /// a [`ShaderPaint`]. Off by default: only a host whose painter runs
    /// them may turn it on; without it they are not drawn.
    pub shaders: bool,
}

impl<'a> EguiSink<'a> {
    /// A sink that places the viewport at `origin`.
    pub fn new(ctx: Option<&'a Context>, origin: Pos2) -> Self {
        Self {
            ctx,
            origin,
            shapes: Vec::new(),
            fold_backdrop: false,
            backdrop: None,
            solid_rects: false,
            shaders: false,
        }
    }

    /// Hand the shapes to egui on the painter's layer, intersecting each clip
    /// rectangle with the painter's own.
    pub fn paint(&self, painter: &egui::Painter) {
        let outer = painter.clip_rect();
        painter.ctx().graphics_mut(|g| {
            let list = g.entry(painter.layer_id());
            for cs in &self.shapes {
                list.add(cs.clip_rect.intersect(outer), cs.shape.clone());
            }
        });
    }
}

#[derive(Clone)]
enum LocalSeg {
    Mesh(Arc<Mesh>),
    Text(TextCmd),
    Glyphs(GlyphsCmd),
    Image(ImageSeg),
    Shape(ShapeCmd),
    Shader(Rect, Arc<ShaderOp>),
}

/// Which texture an image quad samples: the image and how it is sampled.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct TextureKey {
    image: u64,
    smooth: bool,
    repeat: bool,
}

/// Quads of one image under one sampling mode. The mesh names its texture
/// only once the texture exists, which is when it is first emitted.
#[derive(Clone)]
struct ImageSeg {
    key: TextureKey,
    /// Weak, so a cached list does not keep pixels alive that no display
    /// list draws any more.
    image: WeakImage,
    mesh: Arc<Mesh>,
}

struct TextureEntry {
    handle: TextureHandle,
    owner: WeakImage,
    last_used: u64,
}

/// The most quads one tiled blit becomes. More than this is a tile of a few
/// pixels over a whole screen; the rest is left out and reported.
const MAX_TILES: usize = 4096;

fn push_rect(mesh: &mut Mesh, r: Rect, u: (f64, f64), v: (f64, f64)) {
    let p = |x: f64, y: f64| Pos2::new(x as f32, y as f32);
    push_quad(
        mesh,
        [p(r.x0, r.y0), p(r.x1, r.y0), p(r.x1, r.y1), p(r.x0, r.y1)],
        [p(u.0, v.0), p(u.1, v.0), p(u.1, v.1), p(u.0, v.1)],
        Color32::WHITE,
    );
}

/// Add the quads of `blit` to `clamp` (sampled with clamped edges) or
/// `repeat` (sampled with a repeating texture). Returns false when the tile
/// limit cut it short.
fn blit_quads(
    blit: &ImageBlit,
    size: (u32, u32),
    smooth: bool,
    clamp: &mut Mesh,
    repeat: &mut Mesh,
) -> bool {
    if !blit.is_visible() {
        return true;
    }
    let (w, h) = (f64::from(size.0), f64::from(size.1));
    let whole = Rect::new(0.0, 0.0, w, h);
    let mut src = blit.src;
    if blit.isolate && smooth && src != whole {
        // Linear filtering reads half a texel past the edge; pull the edge in
        // so a border part never picks up its neighbour's colour.
        let dx = (src.width() / 2.0).min(0.5);
        let dy = (src.height() / 2.0).min(0.5);
        src = Rect::new(src.x0 + dx, src.y0 + dy, src.x1 - dx, src.y1 - dy);
    }
    // Texture coordinate at fraction `f` of one copy, along one axis.
    let coord = |f: f64, s0: f64, s1: f64, full: f64, flip: bool| {
        let f = if flip { 1.0 - f } else { f };
        (s0 + f * (s1 - s0)) / full
    };
    let u_at = |f: f64| coord(f, src.x0, src.x1, w, blit.mirror[0]);
    let v_at = |f: f64| coord(f, src.y0, src.y1, h, blit.mirror[1]);
    let Some(cover) = blit.cover else {
        push_rect(
            clamp,
            blit.dst,
            (u_at(0.0), u_at(1.0)),
            (v_at(0.0), v_at(1.0)),
        );
        return true;
    };
    let tile = blit.dst;
    let (tw, th) = (tile.width(), tile.height());
    let fx = |x: f64| (x - tile.x0) / tw;
    let fy = |y: f64| (y - tile.y0) / th;
    if blit.src == whole && blit.repeats() == [true, true] {
        // The texture repeats by itself: one quad whose coordinates run past
        // 0..1. A flipped copy is the same lattice read backwards.
        //
        // This relies on the texture's sampler. A renderer that filters in
        // its shader instead (egui_kittest's default "predictable" options)
        // shows one copy with its edges smeared outwards.
        push_rect(
            repeat,
            cover,
            (u_at(fx(cover.x0)), u_at(fx(cover.x1))),
            (v_at(fy(cover.y0)), v_at(fy(cover.y1))),
        );
        return true;
    }
    let first_col = fx(cover.x0).floor() as i64;
    let last_col = fx(cover.x1).ceil() as i64;
    let first_row = fy(cover.y0).floor() as i64;
    let last_row = fy(cover.y1).ceil() as i64;
    let mut made = 0usize;
    for row in first_row..last_row {
        for col in first_col..last_col {
            let x0 = tile.x0 + col as f64 * tw;
            let y0 = tile.y0 + row as f64 * th;
            let part = Rect::new(x0, y0, x0 + tw, y0 + th).intersect(cover);
            if part.width() <= 0.0 || part.height() <= 0.0 {
                continue;
            }
            if made == MAX_TILES {
                return false;
            }
            made += 1;
            push_rect(
                clamp,
                part,
                (u_at((part.x0 - x0) / tw), u_at((part.x1 - x0) / tw)),
                (v_at((part.y0 - y0) / th), v_at((part.y1 - y0) / th)),
            );
        }
    }
    true
}

/// The texture for `key`, uploading the image the first time.
fn texture_for(
    textures: &mut HashMap<TextureKey, TextureEntry>,
    uploads: &mut u64,
    frame: u64,
    ctx: &Context,
    key: TextureKey,
    owner: &WeakImage,
) -> Option<TextureId> {
    if let Some(t) = textures.get_mut(&key) {
        t.last_used = frame;
        return Some(t.handle.id());
    }
    let image = owner.upgrade()?;
    let max_side = ctx.input(|i| i.max_texture_side).max(1);
    let fitted = fit_texture(&image, max_side)?;
    let pixels = ColorImage::from_rgba_premultiplied(
        [fitted.width() as usize, fitted.height() as usize],
        fitted.data(),
    );
    let filter = if key.smooth {
        TextureFilter::Linear
    } else {
        TextureFilter::Nearest
    };
    let options = TextureOptions {
        magnification: filter,
        minification: filter,
        wrap_mode: if key.repeat {
            TextureWrapMode::Repeat
        } else {
            TextureWrapMode::ClampToEdge
        },
        mipmap_mode: None,
    };
    let handle = ctx.load_texture(format!("ljos-image-{}", key.image), pixels, options);
    let id = handle.id();
    *uploads += 1;
    textures.insert(
        key,
        TextureEntry {
            handle,
            owner: owner.clone(),
            last_used: frame,
        },
    );
    Some(id)
}

/// `image`, or a smaller copy when a side is past the GPU's limit. Texture
/// coordinates are fractions, so the quads do not change.
fn fit_texture(image: &ImageHandle, max_side: usize) -> Option<ImageHandle> {
    let max = u32::try_from(max_side).unwrap_or(u32::MAX);
    if image.width() <= max && image.height() <= max {
        return Some(image.clone());
    }
    let scale = f64::from(max) / f64::from(image.width().max(image.height()));
    let side = |n: u32| ((f64::from(n) * scale).round() as u32).clamp(1, max);
    image.resized(side(image.width()), side(image.height()))
}

#[derive(Clone)]
struct GlyphsCmd {
    run: Arc<GlyphRun>,
    origin: Vec2,
    color: Rgba,
    decorations: Decorations,
}

/// A run placed on the screen. Its mesh is built when it is first drawn and
/// kept until the atlas or the clip rectangle changes.
#[derive(Clone)]
struct GlyphSeg {
    cmd: GlyphsCmd,
    to_screen: Affine,
    opacity: f32,
    mesh: Option<GlyphMesh>,
}

#[derive(Clone)]
struct GlyphMesh {
    generation: u64,
    clip: ERect,
    mesh: Arc<Mesh>,
}

impl GlyphSeg {
    fn mesh_for(&self, generation: u64, clip: ERect) -> Option<&Arc<Mesh>> {
        self.mesh
            .as_ref()
            .filter(|m| m.generation == generation && m.clip == clip)
            .map(|m| &m.mesh)
    }
}

/// Where one glyph goes on the screen.
enum Placement {
    /// Upright text: the glyph origin in whole device pixels.
    Snapped { x: i64, y: i64 },
    /// Rotated, sheared or unevenly scaled text: maps bitmap pixels, relative
    /// to the glyph origin, to screen points.
    Free(Affine),
}

/// Visit every glyph of `seg` that can touch `clip`, with its atlas key, the
/// device size it is filled at and its place. Both the atlas pass and the mesh
/// pass go through here, so they always agree on the keys.
fn place_glyphs(
    seg: &GlyphSeg,
    ppp: f32,
    clip: ERect,
    mut f: impl FnMut(GlyphKey, f64, Placement),
) {
    let run = &*seg.cmd.run;
    let origin = seg.cmd.origin;
    let screen_bounds = erect(seg.to_screen.transform_rect_bbox(run.bounds + origin));
    if !screen_bounds.intersects(clip) {
        return;
    }
    let ppp = f64::from(ppp);
    let to_device = Affine::scale(ppp) * seg.to_screen;
    let [a, _, _, d, e, ty] = to_device.as_coeffs();
    if is_upright(to_device) && (a - d).abs() < 1e-6 * a.abs().max(1.0) {
        let size = f64::from(run.font_size) * a;
        let dy = baseline_snap(to_device, f64::from(run.baseline) + origin.y);
        // A glyph reaches at most this far from its origin; anything further
        // from the clip rectangle is skipped before it costs an atlas slot.
        let reach = size * 2.0 + 2.0;
        let (cx0, cx1) = (f64::from(clip.min.x) * ppp, f64::from(clip.max.x) * ppp);
        let (cy0, cy1) = (f64::from(clip.min.y) * ppp, f64::from(clip.max.y) * ppp);
        for g in &run.glyphs {
            let x = a * (f64::from(g.x) + origin.x) + e;
            let y = (d * (f64::from(g.y) + origin.y) + ty + dy).round();
            if x + reach < cx0 || x - reach > cx1 || y + reach < cy0 || y - reach > cy1 {
                continue;
            }
            let q = (x * BINS as f64).round() as i64;
            let (key, filled) = GlyphKey::new(run, g.id, size, q.rem_euclid(BINS));
            f(
                key,
                filled,
                Placement::Snapped {
                    x: q.div_euclid(BINS),
                    y: y as i64,
                },
            );
        }
    } else {
        let scale = to_device.determinant().abs().sqrt();
        if scale <= 0.0 {
            return;
        }
        let size = f64::from(run.font_size) * scale;
        for g in &run.glyphs {
            let (key, filled) = GlyphKey::new(run, g.id, size, 0);
            let at = origin + Vec2::new(f64::from(g.x), f64::from(g.y));
            let px_to_local = f64::from(run.font_size) / filled;
            f(
                key,
                filled,
                Placement::Free(seg.to_screen * Affine::translate(at) * Affine::scale(px_to_local)),
            );
        }
    }
}

fn push_quad(mesh: &mut Mesh, corners: [Pos2; 4], uv: [Pos2; 4], color: Color32) {
    let base = mesh.vertices.len() as u32;
    for (pos, uv) in corners.into_iter().zip(uv) {
        mesh.vertices.push(Vertex { pos, uv, color });
    }
    mesh.indices
        .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
}

fn pos2(p: Point) -> Pos2 {
    Pos2::new(p.x as f32, p.y as f32)
}

/// Quads for the glyphs of `seg` that are in the atlas, and its bars.
fn glyph_mesh(seg: &GlyphSeg, atlas: &GlyphAtlas, ppp: f32, clip: ERect) -> Mesh {
    let mut mesh = Mesh::default();
    let Some(texture) = atlas.texture_id() else {
        return mesh;
    };
    mesh.texture_id = texture;
    let color = color32(seg.cmd.color.with_opacity(seg.opacity));
    let side = atlas.side() as f32;
    let inv_ppp = 1.0 / f64::from(ppp);
    place_glyphs(seg, ppp, clip, |key, _, placement| {
        let Some(Slot {
            x,
            y,
            w,
            h,
            left,
            top,
        }) = atlas.slot(&key)
        else {
            return;
        };
        if w == 0 {
            return;
        }
        let (u0, v0) = (x as f32 / side, y as f32 / side);
        let (u1, v1) = ((x + w) as f32 / side, (y + h) as f32 / side);
        let uv = [
            Pos2::new(u0, v0),
            Pos2::new(u1, v0),
            Pos2::new(u1, v1),
            Pos2::new(u0, v1),
        ];
        let (l, t) = (f64::from(left), f64::from(top));
        let (r, b) = (l + f64::from(w), t + f64::from(h));
        let corners = match placement {
            Placement::Snapped { x, y } => {
                let (x, y) = (x as f64, y as f64);
                [(l, t), (r, t), (r, b), (l, b)]
                    .map(|(cx, cy)| pos2(Point::new((x + cx) * inv_ppp, (y + cy) * inv_ppp)))
            }
            Placement::Free(to_screen) => [(l, t), (r, t), (r, b), (l, b)]
                .map(|(cx, cy)| pos2(to_screen * Point::new(cx, cy))),
        };
        push_quad(&mut mesh, corners, uv, color);
    });
    let to_device = Affine::scale(f64::from(ppp)) * seg.to_screen;
    for bar in seg.cmd.decorations.bars() {
        let bar = bar + seg.cmd.origin;
        let corners = match snap_bar(to_device, bar) {
            Some(r) => [(r.x0, r.y0), (r.x1, r.y0), (r.x1, r.y1), (r.x0, r.y1)]
                .map(|(x, y)| pos2(Point::new(x * inv_ppp, y * inv_ppp))),
            None => [
                (bar.x0, bar.y0),
                (bar.x1, bar.y0),
                (bar.x1, bar.y1),
                (bar.x0, bar.y1),
            ]
            .map(|(x, y)| pos2(seg.to_screen * Point::new(x, y))),
        };
        push_quad(&mut mesh, corners, [atlas.white_uv(); 4], color);
    }
    mesh
}

#[derive(Clone)]
struct TextCmd {
    rect: Rect,
    text: Arc<str>,
    size: f32,
    color: Rgba,
    align: TextAlign,
}

#[derive(Clone)]
enum WorldSeg {
    Mesh(Arc<Mesh>),
    Text {
        /// World-space (screen) layout box.
        rect: Rect,
        text: Arc<str>,
        size_px: f32,
        color: Color32,
        opacity: f32,
        align: TextAlign,
    },
    Glyphs(GlyphSeg),
    Image(ImageSeg),
    Shape(ShapeSeg),
    Shader(ShaderSeg),
}

/// A shader effect placed on the screen. Its callback is made when it is
/// first drawn and kept while its textures stay, so a host sees the same
/// `Arc` for as long as nothing changed.
#[derive(Clone)]
struct ShaderSeg {
    rect: Rect,
    op: Arc<ShaderOp>,
    to_screen: Affine,
    opacity: f32,
    paint: Option<Arc<ShaderPaint>>,
}

/// What a host's GPU painter draws for a shader effect: the callback of a
/// [`Shape::Callback`] B0 emits when [`EguiSink::shaders`] is on.
#[derive(Debug)]
pub struct ShaderPaint {
    pub op: Arc<ShaderOp>,
    /// The item's rectangle in its own coordinates.
    pub rect: Rect,
    /// From the item's coordinates to the screen, in points.
    pub to_screen: Affine,
    pub opacity: f32,
    /// Device pixels per point.
    pub ppp: f32,
    /// One per [`ShaderOp::textures`]; `None` where there is no picture.
    pub textures: Vec<Option<TextureId>>,
}

impl ShaderPaint {
    /// The uniform buffer for a draw into `viewport` (device pixels).
    pub fn uniforms(&self, viewport: Viewport) -> Vec<u8> {
        self.op
            .uniforms_for(self.to_screen, self.opacity, self.ppp, viewport)
    }
}

fn shape_of(seg: &WorldSeg) -> Option<&ShapeSeg> {
    match seg {
        WorldSeg::Shape(s) => Some(s),
        _ => None,
    }
}

fn shape_of_mut(seg: &mut WorldSeg) -> Option<&mut ShapeSeg> {
    match seg {
        WorldSeg::Shape(s) => Some(s),
        _ => None,
    }
}

/// A node's segments are replaced: what the old shapes built goes to the new
/// ones that draw the same thing, and is dropped for the rest.
fn hand_over(cache: &mut ShapeCache, old: &mut [WorldSeg], new: &mut [WorldSeg]) {
    for o in old.iter_mut().filter_map(shape_of_mut) {
        match new
            .iter_mut()
            .filter_map(shape_of_mut)
            .find(|n| n.continues(o))
        {
            Some(n) => n.inherit(o),
            None => cache.release(o.cmd()),
        }
    }
}

struct ListEntry {
    segs: Arc<[LocalSeg]>,
    warnings: u32,
    last_used: u64,
}

#[derive(PartialEq)]
struct Sig {
    list: DisplayListId,
    to_screen: [f64; 6],
    opacity: f32,
}

struct NodeEntry {
    sig: Sig,
    segs: Vec<WorldSeg>,
    last_used: u64,
}

/// The epaint backend.
pub struct EpaintBackend {
    ppp: f32,
    aa: Tessellator,
    no_aa: Tessellator,
    lists: FxHashMap<DisplayListId, ListEntry>,
    nodes: SecondaryMap<NodeId, NodeEntry>,
    frame: u64,
    /// The frame of the last `encode`; `frame` is also set by `sync`.
    encoded: u64,
    tessellations: u64,
    world_rebuilds: u64,
    atlas: GlyphAtlas,
    textures: HashMap<TextureKey, TextureEntry>,
    texture_uploads: u64,
    shapes: ShapeCache,
    /// The nodes that hold a shape texture, so the ones that left the frame
    /// are found without walking every node.
    textured: HashSet<NodeId>,
}

/// What the glyph atlas holds, for tests and diagnostics.
#[derive(Clone, Copy, Debug)]
pub struct GlyphAtlasInfo<'a> {
    /// Width and height of the atlas in texels.
    pub side: usize,
    /// Glyphs held, counting those without ink.
    pub glyphs: usize,
    /// Glyphs filled since the backend was created.
    pub rasterised: u64,
    /// Coverage, one byte per texel, row by row.
    pub alpha: &'a [u8],
}

impl std::fmt::Debug for EpaintBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EpaintBackend")
            .field("lists", &self.lists.len())
            .field("nodes", &self.nodes.len())
            .finish_non_exhaustive()
    }
}

impl Default for EpaintBackend {
    fn default() -> Self {
        Self::new()
    }
}

fn tessellator(ppp: f32, feathering: bool) -> Tessellator {
    // Meshes are built in local space and re-mapped later, so pixel snapping
    // relative to the local origin would be wrong; text and discs do not go
    // through here.
    let options = TessellationOptions {
        feathering,
        coarse_tessellation_culling: false,
        prerasterized_discs: false,
        round_text_to_pixels: false,
        round_line_segments_to_pixels: false,
        round_rects_to_pixels: false,
        parallel_tessellation: false,
        ..Default::default()
    };
    Tessellator::new(ppp, options, [1, 1], Vec::new())
}

fn color32(c: Rgba) -> Color32 {
    let [r, g, b, a] = c.to_premul_u8();
    Color32::from_rgba_premultiplied(r, g, b, a)
}

/// The colour of `list` when it is one opaque rectangle with square
/// corners and no border that covers all of `viewport`.
fn backdrop(
    list: &DisplayList,
    to_screen: Affine,
    opacity: f32,
    clip: ERect,
    viewport: ERect,
) -> Option<Color32> {
    let (rect, color, _) = solid(list, to_screen, opacity)?;
    (rect.contains_rect(viewport) && clip.contains_rect(viewport)).then_some(color)
}

/// Where `list` lands on the screen, its colour and whether its edges are
/// antialiased, when it is one opaque upright rectangle with square
/// corners and no border.
fn solid(list: &DisplayList, to_screen: Affine, opacity: f32) -> Option<(ERect, Color32, bool)> {
    let [
        DrawOp::Rect {
            rect,
            radii,
            fill: Paint::Solid(color),
            border: None,
            aa,
        },
    ] = list.ops()
    else {
        return None;
    };
    let [sx, ky, kx, sy, _, _] = to_screen.as_coeffs();
    let fill = color.with_opacity(opacity);
    let upright = ky == 0.0 && kx == 0.0 && sx > 0.0 && sy > 0.0;
    let square = radii.iter().all(|r| *r <= 0.0);
    (upright && square && Paint::Solid(fill).is_opaque()).then(|| {
        (
            erect(to_screen.transform_rect_bbox(*rect)),
            color32(fill),
            *aa,
        )
    })
}

/// The pixels a solid rectangle covers, as a rectangle on whole device
/// pixels, when they are the pixels of a rectangle: a hard-edged one
/// covers the pixels whose centres it holds; an antialiased one only
/// covers whole pixels when its edges lie between them.
fn pixels_of(rect: ERect, aa: bool, ppp: f32) -> Option<ERect> {
    let snapped = if aa {
        rect
    } else {
        // A pixel is in when its centre is: x0 <= i + 0.5 < x1.
        let edge = |v: f32| ((v * ppp - 0.5).ceil()) / ppp;
        ERect::from_min_max(
            Pos2::new(edge(rect.min.x), edge(rect.min.y)),
            Pos2::new(edge(rect.max.x), edge(rect.max.y)),
        )
    };
    on_pixels(snapped, ppp).then_some(snapped)
}

/// Whether every edge of `rect` (in points) lies on a device pixel's
/// edge, where a feathered edge and a hard one cover the same pixels.
fn on_pixels(rect: ERect, ppp: f32) -> bool {
    let whole = |v: f32| {
        let px = v * ppp;
        (px - px.round()).abs() < 1e-3
    };
    rect.is_positive()
        && whole(rect.min.x)
        && whole(rect.min.y)
        && whole(rect.max.x)
        && whole(rect.max.y)
}

/// What is drawn, in paint order.
enum Drawn {
    /// A node's shapes, in its clip rectangle.
    Node(NodeId, ERect),
    /// A rectangle handed over as one ([`EguiSink::solid_rects`]).
    Solid(ClippedShape),
}

fn erect(r: Rect) -> ERect {
    ERect::from_min_max(
        Pos2::new(r.x0 as f32, r.y0 as f32),
        Pos2::new(r.x1 as f32, r.y1 as f32),
    )
}

fn corner_radius(radii: [f32; 4]) -> CornerRadius {
    let q = |r: f32| r.round().clamp(0.0, 255.0) as u8;
    CornerRadius {
        nw: q(radii[0]),
        ne: q(radii[1]),
        se: q(radii[2]),
        sw: q(radii[3]),
    }
}

fn first_stop(g: &LinearGradient) -> Rgba {
    g.stops.first().map_or(Rgba::TRANSPARENT, |s| s.color)
}

fn coeffs_f32(a: Affine) -> [f64; 6] {
    a.as_coeffs()
}

/// Recolour a white-filled mesh with a linear gradient. Each triangle is cut
/// at every stop offset so the colour is linear inside every piece, which makes
/// vertex colours exact for any gradient angle.
fn apply_gradient(src: &Mesh, g: &LinearGradient, out: &mut Mesh) {
    #[derive(Clone, Copy)]
    struct V {
        pos: Pos2,
        /// Coverage carried by the tessellator's anti-aliasing feather (0..1).
        cov: f32,
        t: f32,
    }
    fn lerp(a: V, b: V, k: f32) -> V {
        V {
            pos: Pos2::new(
                a.pos.x + (b.pos.x - a.pos.x) * k,
                a.pos.y + (b.pos.y - a.pos.y) * k,
            ),
            cov: a.cov + (b.cov - a.cov) * k,
            t: a.t + (b.t - a.t) * k,
        }
    }
    /// Sutherland-Hodgman against `t >= bound` (keep_ge) or `t <= bound`.
    fn clip(poly: &[V], bound: f32, keep_ge: bool) -> Vec<V> {
        let inside = |v: &V| if keep_ge { v.t >= bound } else { v.t <= bound };
        let mut out = Vec::with_capacity(poly.len() + 2);
        for i in 0..poly.len() {
            let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
            match (inside(&a), inside(&b)) {
                (true, true) => out.push(b),
                (true, false) => out.push(lerp(a, b, (bound - a.t) / (b.t - a.t))),
                (false, true) => {
                    out.push(lerp(a, b, (bound - a.t) / (b.t - a.t)));
                    out.push(b);
                }
                (false, false) => {}
            }
        }
        out
    }

    let mut cuts: Vec<f32> = g.stops.iter().map(|s| s.offset).collect();
    cuts.dedup();
    let uv = src.vertices.first().map(|v| v.uv).unwrap_or_default();
    out.texture_id = src.texture_id;
    for tri in src.indices.as_chunks::<3>().0 {
        let vs: Vec<V> = tri
            .iter()
            .map(|i| {
                let v = &src.vertices[*i as usize];
                V {
                    pos: v.pos,
                    cov: f32::from(v.color.a()) / 255.0,
                    t: g.param(Point::new(f64::from(v.pos.x), f64::from(v.pos.y))),
                }
            })
            .collect();
        let tmin = vs.iter().map(|v| v.t).fold(f32::INFINITY, f32::min);
        let tmax = vs.iter().map(|v| v.t).fold(f32::NEG_INFINITY, f32::max);
        // Slab boundaries: -inf, each cut inside (tmin, tmax), +inf.
        let mut bounds = vec![f32::NEG_INFINITY];
        bounds.extend(cuts.iter().copied().filter(|c| *c > tmin && *c < tmax));
        bounds.push(f32::INFINITY);
        for w in bounds.windows(2) {
            let mut poly = vs.clone();
            if w[0].is_finite() {
                poly = clip(&poly, w[0], true);
            }
            if w[1].is_finite() && !poly.is_empty() {
                poly = clip(&poly, w[1], false);
            }
            if poly.len() < 3 {
                continue;
            }
            let base = out.vertices.len() as u32;
            for v in &poly {
                let c = g.color_at(v.t).with_opacity(v.cov);
                let [r, gg, b, a] = c.to_premul_u8();
                out.vertices.push(Vertex {
                    pos: v.pos,
                    uv,
                    color: Color32::from_rgba_premultiplied(r, gg, b, a),
                });
            }
            for k in 1..poly.len() as u32 - 1 {
                out.indices.extend([base, base + k, base + k + 1]);
            }
        }
    }
}

impl EpaintBackend {
    /// New backend at 1 pixel per point; the real value arrives with the first frame.
    pub fn new() -> Self {
        Self {
            ppp: 1.0,
            aa: tessellator(1.0, true),
            no_aa: tessellator(1.0, false),
            lists: FxHashMap::default(),
            nodes: SecondaryMap::new(),
            frame: 0,
            encoded: 0,
            tessellations: 0,
            world_rebuilds: 0,
            atlas: GlyphAtlas::default(),
            textures: HashMap::new(),
            texture_uploads: 0,
            shapes: ShapeCache::default(),
            textured: HashSet::new(),
        }
    }

    /// How many shapes were cut into triangles so far (tests use it to prove
    /// that a shape is cut once, and not again when it only moves).
    pub fn shape_tessellations(&self) -> u64 {
        self.shapes.tessellated()
    }

    /// How many shape textures were filled so far.
    pub fn shape_rasters(&self) -> u64 {
        self.shapes.rasterised()
    }

    /// How many shapes have their triangles held now.
    pub fn shape_geometries(&self) -> usize {
        self.shapes.geometries()
    }

    /// How many shapes hold a texture now.
    pub fn shape_textures(&self) -> usize {
        self.nodes
            .values()
            .flat_map(|n| n.segs.iter().filter_map(shape_of))
            .filter(|s| s.has_texture())
            .count()
    }

    /// Drop the nodes not drawn since `keep_after`, and what their shapes
    /// built with them.
    fn retain_nodes(&mut self, keep_after: u64) {
        let shapes = &mut self.shapes;
        self.nodes.retain(|_, e| {
            let keep = e.last_used >= keep_after;
            if !keep {
                for s in e.segs.iter().filter_map(shape_of) {
                    shapes.release(s.cmd());
                }
            }
            keep
        });
    }

    /// How many image textures the backend holds now.
    pub fn image_textures(&self) -> usize {
        self.textures.len()
    }

    /// How many image textures were uploaded so far (tests use it to prove an
    /// image is uploaded once, not once per frame or per item).
    pub fn texture_uploads(&self) -> u64 {
        self.texture_uploads
    }

    /// The glyph atlas as it is now.
    pub fn glyph_atlas(&self) -> GlyphAtlasInfo<'_> {
        GlyphAtlasInfo {
            side: self.atlas.side(),
            glyphs: self.atlas.len(),
            rasterised: self.atlas.rasterised(),
            alpha: self.atlas.alpha(),
        }
    }

    /// How many display lists were tessellated so far (tests use it to prove
    /// that only `CONTENT` changes rebuild geometry).
    pub fn tessellations(&self) -> u64 {
        self.tessellations
    }

    /// How many per-node world meshes were re-mapped so far.
    pub fn world_rebuilds(&self) -> u64 {
        self.world_rebuilds
    }

    fn set_ppp(&mut self, ppp: f32) {
        if (ppp - self.ppp).abs() > f32::EPSILON {
            self.ppp = ppp;
            self.aa = tessellator(ppp, true);
            self.no_aa = tessellator(ppp, false);
            self.lists.clear();
            self.nodes.clear();
            self.shapes.clear();
            // Glyphs are filled per device size; none of the old ones match.
            self.atlas.clear();
        }
    }

    fn build_list(&mut self, list: &DisplayList) -> ListEntry {
        self.tessellations += 1;
        let mut segs: Vec<LocalSeg> = Vec::new();
        let mut cur = Mesh::default();
        let mut warnings = 0u32;
        let mut warn = |what: &str| {
            warnings += 1;
            tracing::debug!("B0: {what}");
        };
        for op in list.ops() {
            match op {
                DrawOp::Rect {
                    rect,
                    radii,
                    fill,
                    border,
                    aa,
                } => {
                    let tess = if *aa { &mut self.aa } else { &mut self.no_aa };
                    let (r, cr) = (erect(*rect), corner_radius(*radii));
                    // The mesh is built before the node's opacity is
                    // known, so a border that hides the fill only while
                    // the node is not faded still has it underneath.
                    let area = fill_area(*rect, *radii, border.as_ref(), 1.0)
                        .map(|(rect, radii)| (erect(rect), corner_radius(radii)));
                    match (fill, area) {
                        (Paint::Solid(c), Some((r, cr))) if !c.is_transparent() => {
                            tess.tessellate_rect(&RectShape::filled(r, cr, color32(*c)), &mut cur);
                        }
                        (Paint::LinearGradient(g), Some((r, cr))) if !fill.is_invisible() => {
                            let mut tmp = Mesh::default();
                            tess.tessellate_rect(
                                &RectShape::filled(r, cr, Color32::WHITE),
                                &mut tmp,
                            );
                            apply_gradient(&tmp, g, &mut cur);
                        }
                        (Paint::Image(_), _) => warn("image fills are not drawn"),
                        _ => {}
                    }
                    if let Some(b) = border
                        && b.width > 0.0
                    {
                        let color = match &b.paint {
                            Paint::Solid(c) => *c,
                            Paint::LinearGradient(g) => {
                                warn("gradient borders use their first stop colour");
                                first_stop(g)
                            }
                            Paint::Image(_) => {
                                warn("image borders are not drawn");
                                Rgba::TRANSPARENT
                            }
                        };
                        if !color.is_transparent() {
                            let stroke = Stroke::new(b.width, color32(color));
                            tess.tessellate_rect(
                                &RectShape::stroke(r, cr, stroke, StrokeKind::Inside),
                                &mut cur,
                            );
                        }
                    }
                }
                DrawOp::Shadow {
                    rect,
                    radii,
                    blur,
                    spread,
                    color,
                } => {
                    if color.is_transparent() {
                        continue;
                    }
                    let shadow = Shadow {
                        offset: [0, 0],
                        blur: blur.round().clamp(0.0, 255.0) as u8,
                        spread: spread.round().clamp(0.0, 255.0) as u8,
                        color: color32(*color),
                    };
                    let shape = shadow.as_shape(erect(*rect), corner_radius(*radii));
                    self.aa.tessellate_rect(&shape, &mut cur);
                }
                DrawOp::Path { path, style, paint } => {
                    let color = match paint {
                        Paint::Solid(c) => *c,
                        Paint::LinearGradient(g) => {
                            warn("gradient paths use their first stop colour");
                            first_stop(g)
                        }
                        Paint::Image(_) => {
                            warn("image paths are not drawn");
                            continue;
                        }
                    };
                    let mut subpaths: Vec<(Vec<Pos2>, bool)> = Vec::new();
                    path.flatten_into(0.1, &mut subpaths);
                    for (pts, closed) in subpaths {
                        if pts.len() < 2 {
                            continue;
                        }
                        let shape = match style {
                            PathStyle::Stroke { width } => {
                                let s = PathStroke::new(*width, color32(color));
                                if closed {
                                    PathShape::closed_line(pts, s)
                                } else {
                                    PathShape::line(pts, s)
                                }
                            }
                            PathStyle::Fill | PathStyle::FillEvenOdd => {
                                warn("path fills assume convex polygons");
                                PathShape::convex_polygon(pts, color32(color), PathStroke::NONE)
                            }
                        };
                        self.aa.tessellate_shape(Shape::Path(shape), &mut cur);
                    }
                }
                DrawOp::Shape { path, style, brush } => {
                    // Nothing is built here: a shape that is never inside
                    // its clip rectangle costs nothing.
                    if let Some(cmd) = ShapeCmd::new(path, style, brush) {
                        if !cur.is_empty() {
                            segs.push(LocalSeg::Mesh(Arc::new(std::mem::take(&mut cur))));
                        }
                        segs.push(LocalSeg::Shape(cmd));
                    }
                }
                DrawOp::Text {
                    rect,
                    text,
                    size,
                    color,
                    align,
                } => {
                    if !cur.is_empty() {
                        segs.push(LocalSeg::Mesh(Arc::new(std::mem::take(&mut cur))));
                    }
                    segs.push(LocalSeg::Text(TextCmd {
                        rect: *rect,
                        text: Arc::clone(text),
                        size: *size,
                        color: *color,
                        align: *align,
                    }));
                }
                DrawOp::Image {
                    image,
                    src,
                    dst,
                    tile,
                    mirror,
                    smooth,
                    opacity,
                    tint,
                } => {
                    if !cur.is_empty() {
                        segs.push(LocalSeg::Mesh(Arc::new(std::mem::take(&mut cur))));
                    }
                    let blit = image_blit(*src, *dst, *tile, *mirror);
                    if !image_segs(&mut segs, image, &[blit], *smooth, *opacity, *tint) {
                        warn("an image is tiled too finely; some tiles are not drawn");
                    }
                }
                DrawOp::NinePatch {
                    image,
                    insets,
                    dst,
                    tile,
                    mirror,
                    smooth,
                } => {
                    if !cur.is_empty() {
                        segs.push(LocalSeg::Mesh(Arc::new(std::mem::take(&mut cur))));
                    }
                    let size = (image.width(), image.height());
                    let blits = nine_patch_blits(size, *insets, *dst, *tile, *mirror);
                    if !image_segs(&mut segs, image, &blits, *smooth, 1.0, None) {
                        warn("a border image is tiled too finely; some tiles are not drawn");
                    }
                }
                DrawOp::Glyphs {
                    run,
                    origin,
                    paint,
                    decorations,
                } => {
                    let color = match paint {
                        Paint::Solid(c) => *c,
                        Paint::LinearGradient(g) => {
                            warn("gradient text uses its first stop colour");
                            first_stop(g)
                        }
                        Paint::Image(_) => {
                            warn("image-filled text is not drawn");
                            continue;
                        }
                    };
                    if color.is_transparent() {
                        continue;
                    }
                    if !cur.is_empty() {
                        segs.push(LocalSeg::Mesh(Arc::new(std::mem::take(&mut cur))));
                    }
                    segs.push(LocalSeg::Glyphs(GlyphsCmd {
                        run: Arc::clone(run),
                        origin: *origin,
                        color,
                        decorations: *decorations,
                    }));
                }
                DrawOp::Shader { rect, op } => {
                    if !cur.is_empty() {
                        segs.push(LocalSeg::Mesh(Arc::new(std::mem::take(&mut cur))));
                    }
                    segs.push(LocalSeg::Shader(*rect, Arc::clone(op)));
                }
                DrawOp::Embed { .. } => warn("Embed surfaces are not drawn by the shape encoder"),
            }
        }
        if !cur.is_empty() {
            segs.push(LocalSeg::Mesh(Arc::new(cur)));
        }
        ListEntry {
            segs: segs.into(),
            warnings,
            last_used: self.frame,
        }
    }

    /// Put every glyph this frame draws into the atlas, then upload it.
    fn fill_atlas(&mut self, ctx: &Context, order: &[Drawn], stats: &mut EncodeStats) {
        let max_side = GlyphAtlas::max_side(ctx);
        for attempt in 0..2 {
            let mut full = false;
            for drawn in order {
                let Drawn::Node(node, clip) = drawn else {
                    continue;
                };
                let Some(ne) = self.nodes.get(*node) else {
                    continue;
                };
                for seg in &ne.segs {
                    let WorldSeg::Glyphs(g) = seg else { continue };
                    // A valid mesh means its glyphs are already in the atlas.
                    if g.mesh_for(self.atlas.generation(), *clip).is_some() {
                        continue;
                    }
                    place_glyphs(g, self.ppp, *clip, |key, size, _| {
                        full |= self.atlas.ensure(&key, &g.cmd.run, size, max_side).is_err();
                    });
                }
            }
            if !full {
                break;
            }
            if attempt == 0 {
                // Largest size reached: drop the glyphs of earlier frames and
                // fill this frame's again.
                self.atlas.clear();
            } else {
                stats.warnings += 1;
                tracing::debug!("B0: the glyph atlas is full; some glyphs are not drawn");
            }
        }
        self.atlas.upload(ctx);
    }

    fn world_segs(local: &[LocalSeg], to_screen: Affine, opacity: f32) -> Vec<WorldSeg> {
        let scale = to_screen.determinant().abs().sqrt();
        local
            .iter()
            .map(|seg| match seg {
                LocalSeg::Mesh(m) => WorldSeg::Mesh(Arc::new(map_mesh(m, to_screen, opacity))),
                LocalSeg::Text(t) => WorldSeg::Text {
                    rect: to_screen.transform_rect_bbox(t.rect),
                    text: Arc::clone(&t.text),
                    size_px: t.size * scale as f32,
                    color: color32(t.color),
                    opacity,
                    align: t.align,
                },
                LocalSeg::Glyphs(cmd) => WorldSeg::Glyphs(GlyphSeg {
                    cmd: cmd.clone(),
                    to_screen,
                    opacity,
                    mesh: None,
                }),
                LocalSeg::Image(seg) => WorldSeg::Image(ImageSeg {
                    key: seg.key,
                    image: seg.image.clone(),
                    mesh: Arc::new(map_mesh(&seg.mesh, to_screen, opacity)),
                }),
                LocalSeg::Shape(cmd) => {
                    WorldSeg::Shape(ShapeSeg::new(cmd.clone(), to_screen, opacity))
                }
                LocalSeg::Shader(rect, op) => WorldSeg::Shader(ShaderSeg {
                    rect: *rect,
                    op: Arc::clone(op),
                    to_screen,
                    opacity,
                    paint: None,
                }),
            })
            .collect()
    }
}

/// `m` moved to the screen and faded by `opacity`.
fn map_mesh(m: &Mesh, to_screen: Affine, opacity: f32) -> Mesh {
    let mut out = Mesh {
        indices: m.indices.clone(),
        vertices: Vec::with_capacity(m.vertices.len()),
        texture_id: m.texture_id,
    };
    for v in &m.vertices {
        let p = to_screen * Point::new(f64::from(v.pos.x), f64::from(v.pos.y));
        out.vertices.push(Vertex {
            pos: Pos2::new(p.x as f32, p.y as f32),
            uv: v.uv,
            color: if opacity < 1.0 {
                v.color.gamma_multiply(opacity)
            } else {
                v.color
            },
        });
    }
    out
}

/// Append the quads of `blits`, faded by `opacity`, at most one segment
/// per sampling mode. Quads of the texture the last segment draws join
/// it, so many pictures of one image (a particle system's) are one mesh.
/// Returns false when a blit hit the tile limit.
fn image_segs(
    segs: &mut Vec<LocalSeg>,
    image: &ImageHandle,
    blits: &[ImageBlit],
    smooth: bool,
    opacity: f32,
    tint: Option<Rgba>,
) -> bool {
    let size = (image.width(), image.height());
    // egui multiplies the texture by the vertex colour, so a tint costs no
    // texture of its own and the quads still join the batch of the picture.
    let tint = tint.map(|t| {
        let [r, g, b, _] = t.to_straight_u8();
        Color32::from_rgb(r, g, b)
    });
    let (mut clamp, mut repeat) = (Mesh::default(), Mesh::default());
    let mut complete = true;
    for blit in blits {
        complete &= blit_quads(blit, size, smooth, &mut clamp, &mut repeat);
    }
    for (mut mesh, repeat) in [(clamp, false), (repeat, true)] {
        if mesh.is_empty() {
            continue;
        }
        if let Some(tint) = tint {
            for v in &mut mesh.vertices {
                v.color = tint;
            }
        }
        if opacity < 1.0 {
            for v in &mut mesh.vertices {
                v.color = v.color.gamma_multiply(opacity);
            }
        }
        let key = TextureKey {
            image: image.id(),
            smooth,
            repeat,
        };
        if let Some(LocalSeg::Image(last)) = segs.last_mut()
            && last.key == key
            && let Some(joined) = Arc::get_mut(&mut last.mesh)
        {
            joined.append(mesh);
            continue;
        }
        segs.push(LocalSeg::Image(ImageSeg {
            key,
            image: image.downgrade(),
            mesh: Arc::new(mesh),
        }));
    }
    complete
}

trait FlattenInto {
    fn flatten_into(&self, tolerance: f64, out: &mut Vec<(Vec<Pos2>, bool)>);
}

impl FlattenInto for vello_cpu::kurbo::BezPath {
    fn flatten_into(&self, tolerance: f64, out: &mut Vec<(Vec<Pos2>, bool)>) {
        let p = |p: Point| Pos2::new(p.x as f32, p.y as f32);
        let mut cur: Vec<Pos2> = Vec::new();
        vello_cpu::kurbo::flatten(self.elements().iter().copied(), tolerance, |el| match el {
            PathEl::MoveTo(pt) => {
                if !cur.is_empty() {
                    out.push((std::mem::take(&mut cur), false));
                }
                cur.push(p(pt));
            }
            PathEl::LineTo(pt) => cur.push(p(pt)),
            PathEl::ClosePath if !cur.is_empty() => {
                out.push((std::mem::take(&mut cur), true));
            }
            _ => {}
        });
        if !cur.is_empty() {
            out.push((cur, false));
        }
    }
}

impl RenderBackend for EpaintBackend {
    type Sink<'a> = EguiSink<'a>;

    fn sync(&mut self, view: &FrameView<'_>) {
        self.set_ppp(view.scale_factor);
        self.frame = view.snapshot.frame;
        let keep_after = self.frame.saturating_sub(KEEP_FRAMES);
        self.lists.retain(|_, e| e.last_used >= keep_after);
        self.retain_nodes(keep_after);
        // A texture outlives neither its image nor a long stretch unseen.
        self.textures
            .retain(|_, t| t.owner.is_alive() && t.last_used >= keep_after);
    }

    fn encode(&mut self, view: &FrameView<'_>, sink: &mut EguiSink<'_>) -> EncodeStats {
        self.set_ppp(view.scale_factor);
        let snap = view.snapshot;
        self.frame = snap.frame;
        self.encoded = snap.frame;
        self.shapes.begin_frame();
        let mut stats = EncodeStats::default();
        let mut warned_clips = HashSet::new();
        let shift = Affine::translate((
            f64::from(sink.origin.x) - snap.viewport.x0,
            f64::from(sink.origin.y) - snap.viewport.y0,
        ));
        let screen_viewport = ERect::from_min_size(
            sink.origin,
            egui::Vec2::new(snap.viewport.width() as f32, snap.viewport.height() as f32),
        );
        sink.shapes.clear();
        sink.shapes.reserve(snap.items.len());
        sink.backdrop = None;
        // Nodes to draw, in paint order, each with its clip rectangle. Shapes
        // are emitted after the loop, once the glyph atlas is complete: adding
        // a glyph can grow the atlas, which moves every texture coordinate.
        let mut order: Vec<Drawn> = Vec::with_capacity(snap.items.len());
        let mut has_glyphs = false;

        for item in &snap.items {
            let Some(list) = &item.list else { continue };
            if list.is_empty() {
                continue;
            }
            stats.items += 1;

            // Clip: world bbox shifted to the screen, never larger than the viewport.
            let mut clip = screen_viewport;
            {
                let mut cur = item.clip;
                while let Some(id) = cur {
                    let c = snap.clip(id);
                    if (!c.axis_aligned || c.is_rounded()) && warned_clips.insert(id) {
                        stats.warnings += 1;
                        tracing::debug!(
                            "B0: {} clip approximated by its bounding box",
                            if c.axis_aligned { "rounded" } else { "rotated" }
                        );
                    }
                    cur = c.parent;
                }
                if let Some(id) = item.clip {
                    clip = clip.intersect(erect(shift.transform_rect_bbox(snap.clip(id).bbox)));
                }
            }
            if !(clip.width() > 0.0 && clip.height() > 0.0) {
                continue;
            }

            let to_screen = shift * item.world;
            if sink.fold_backdrop
                && order.is_empty()
                && sink.backdrop.is_none()
                && let Some(color) = backdrop(list, to_screen, item.opacity, clip, screen_viewport)
            {
                // Clearing to the colour draws the same pixels, without
                // one more pass over all of them.
                sink.backdrop = Some(color);
                continue;
            }
            if sink.solid_rects
                && let Some((rect, color, aa)) = solid(list, to_screen, item.opacity)
                && let Some(rect) = pixels_of(rect, aa, self.ppp)
            {
                order.push(Drawn::Solid(ClippedShape {
                    clip_rect: clip,
                    shape: Shape::Rect(RectShape::filled(rect, CornerRadius::ZERO, color)),
                }));
                continue;
            }
            let sig = Sig {
                list: list.id(),
                to_screen: coeffs_f32(to_screen),
                opacity: item.opacity,
            };

            // Local geometry, tessellated at most once per display list.
            if !self.lists.contains_key(&list.id()) {
                let entry = self.build_list(list);
                self.lists.insert(list.id(), entry);
                stats.caches_built += 1;
            }
            let le = self.lists.get_mut(&list.id()).expect("just inserted");
            le.last_used = self.frame;
            stats.warnings += le.warnings;
            let local = Arc::clone(&le.segs);

            let reuse = self.nodes.get(item.node).is_some_and(|n| n.sig == sig);
            if !reuse {
                let mut segs = Self::world_segs(&local, to_screen, item.opacity);
                if let Some(old) = self.nodes.get_mut(item.node) {
                    hand_over(&mut self.shapes, &mut old.segs, &mut segs);
                }
                self.world_rebuilds += 1;
                self.nodes.insert(
                    item.node,
                    NodeEntry {
                        sig,
                        segs,
                        last_used: self.frame,
                    },
                );
            } else {
                stats.caches_reused += 1;
            }
            let ne = self.nodes.get_mut(item.node).expect("present");
            ne.last_used = self.frame;
            has_glyphs |= ne.segs.iter().any(|s| matches!(s, WorldSeg::Glyphs(_)));
            order.push(Drawn::Node(item.node, clip));
        }

        if has_glyphs {
            match sink.ctx {
                Some(ctx) => self.fill_atlas(ctx, &order, &mut stats),
                None => {
                    stats.warnings += 1;
                    tracing::debug!("B0: text needs an egui Context; skipped");
                }
            }
        }

        for drawn in order {
            let (node, clip) = match drawn {
                Drawn::Node(node, clip) => (node, clip),
                Drawn::Solid(shape) => {
                    sink.shapes.push(shape);
                    continue;
                }
            };
            let ne = self.nodes.get_mut(node).expect("present");
            for seg in &mut ne.segs {
                match seg {
                    WorldSeg::Mesh(m) => sink.shapes.push(ClippedShape {
                        clip_rect: clip,
                        shape: Shape::Mesh(Arc::clone(m)),
                    }),
                    WorldSeg::Image(seg) => {
                        let Some(ctx) = sink.ctx else {
                            stats.warnings += 1;
                            tracing::debug!("B0: an image needs an egui Context; skipped");
                            continue;
                        };
                        let Some(texture) = texture_for(
                            &mut self.textures,
                            &mut self.texture_uploads,
                            self.frame,
                            ctx,
                            seg.key,
                            &seg.image,
                        ) else {
                            continue;
                        };
                        if seg.mesh.texture_id != texture {
                            Arc::make_mut(&mut seg.mesh).texture_id = texture;
                        }
                        sink.shapes.push(ClippedShape {
                            clip_rect: clip,
                            shape: Shape::Mesh(Arc::clone(&seg.mesh)),
                        });
                    }
                    WorldSeg::Shape(s) => {
                        let mesh = self.shapes.draw(
                            s,
                            clip,
                            self.ppp,
                            sink.ctx,
                            self.frame,
                            &mut stats.warnings,
                        );
                        if s.has_texture() {
                            self.textured.insert(node);
                        }
                        if let Some(mesh) = mesh {
                            sink.shapes.push(ClippedShape {
                                clip_rect: clip,
                                shape: Shape::Mesh(mesh),
                            });
                        }
                    }
                    WorldSeg::Shader(seg) => {
                        let Some(ctx) = sink.ctx.filter(|_| sink.shaders) else {
                            continue;
                        };
                        let textures: Vec<Option<TextureId>> = seg
                            .op
                            .textures
                            .iter()
                            .map(|t| {
                                let image = t.image.as_ref()?;
                                let key = TextureKey {
                                    image: image.id(),
                                    smooth: t.smooth,
                                    repeat: false,
                                };
                                texture_for(
                                    &mut self.textures,
                                    &mut self.texture_uploads,
                                    self.frame,
                                    ctx,
                                    key,
                                    &image.downgrade(),
                                )
                            })
                            .collect();
                        if seg.paint.as_ref().is_none_or(|p| p.textures != textures) {
                            seg.paint = Some(Arc::new(ShaderPaint {
                                op: Arc::clone(&seg.op),
                                rect: seg.rect,
                                to_screen: seg.to_screen,
                                opacity: seg.opacity,
                                ppp: self.ppp,
                                textures,
                            }));
                        }
                        let Some(paint) = &seg.paint else { continue };
                        sink.shapes.push(ClippedShape {
                            clip_rect: clip,
                            shape: Shape::Callback(PaintCallback {
                                rect: erect(seg.to_screen.transform_rect_bbox(seg.rect)),
                                callback: Arc::clone(paint) as _,
                            }),
                        });
                    }
                    WorldSeg::Glyphs(g) => {
                        if sink.ctx.is_none() {
                            continue;
                        }
                        let generation = self.atlas.generation();
                        if g.mesh_for(generation, clip).is_none() {
                            g.mesh = Some(GlyphMesh {
                                generation,
                                clip,
                                mesh: Arc::new(glyph_mesh(g, &self.atlas, self.ppp, clip)),
                            });
                        }
                        if let Some(m) = g.mesh_for(generation, clip)
                            && !m.is_empty()
                        {
                            sink.shapes.push(ClippedShape {
                                clip_rect: clip,
                                shape: Shape::Mesh(Arc::clone(m)),
                            });
                        }
                    }
                    WorldSeg::Text {
                        rect,
                        text,
                        size_px,
                        color,
                        opacity,
                        align,
                    } => {
                        let Some(ctx) = sink.ctx else {
                            stats.warnings += 1;
                            tracing::debug!("B0: unshaped text needs an egui Context; skipped");
                            continue;
                        };
                        let galley = ctx.fonts_mut(|f| {
                            f.layout_no_wrap(
                                text.to_string(),
                                FontId::proportional(*size_px),
                                *color,
                            )
                        });
                        let w = f64::from(galley.size().x);
                        let x = match align {
                            TextAlign::Left => rect.x0,
                            TextAlign::Center => (rect.x0 + rect.x1 - w) / 2.0,
                            TextAlign::Right => rect.x1 - w,
                        };
                        let shape =
                            TextShape::new(Pos2::new(x as f32, rect.y0 as f32), galley, *color)
                                .with_opacity_factor(*opacity);
                        sink.shapes.push(ClippedShape {
                            clip_rect: clip,
                            shape: Shape::Text(shape),
                        });
                    }
                }
            }
        }
        // A node that is not in this frame keeps no pixels: megabytes for
        // something nobody sees, and the next frame may be a long way off.
        let (nodes, frame) = (&mut self.nodes, self.frame);
        self.textured.retain(|id| {
            let Some(e) = nodes.get_mut(*id) else {
                return false;
            };
            let shapes = e.segs.iter_mut().filter_map(shape_of_mut);
            if e.last_used == frame {
                shapes.fold(false, |any, s| any | s.has_texture())
            } else {
                shapes
                    .filter(|s| s.has_texture())
                    .for_each(ShapeSeg::forget);
                false
            }
        });
        stats.shapes = sink.shapes.len() as u32;
        stats
    }

    fn trim(&mut self, pressure: MemoryPressure) {
        match pressure {
            MemoryPressure::Critical => {
                self.lists.clear();
                self.nodes.clear();
                self.shapes.clear();
                self.atlas.reset();
                self.textures.clear();
            }
            MemoryPressure::Moderate => {
                let keep_after = self.frame.saturating_sub(10);
                self.lists.retain(|_, e| e.last_used >= keep_after);
                self.retain_nodes(keep_after);
                // A shape that was out of sight in the last frame gives up
                // its triangles and its pixels; both are built again when
                // it is next seen.
                let last = self.encoded;
                let mut seen: Vec<ShapeCmd> = Vec::new();
                for e in self.nodes.values_mut() {
                    let in_frame = e.last_used >= last;
                    for s in e.segs.iter_mut().filter_map(shape_of_mut) {
                        if in_frame && s.drawn_in(last) {
                            seen.push(s.cmd().clone());
                        } else {
                            s.forget();
                        }
                    }
                }
                self.shapes.retain(|cmd| seen.iter().any(|s| s.same(cmd)));
                self.textures
                    .retain(|_, t| t.owner.is_alive() && t.last_used >= keep_after);
            }
        }
    }

    /// B0 draws through egui's own renderer, so there are no pixels here; use R
    /// (`VelloCpuReference`) or egui_kittest's wgpu renderer.
    fn snapshot_rgba(&mut self, _view: &FrameView<'_>) -> Option<RgbaImage> {
        None
    }
}
