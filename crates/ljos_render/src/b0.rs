//! B0: epaint backend (M0 onward). With its private modules (`b0_shape`,
//! `glyph_atlas`, `image_atlas`), all behind the `b0` feature, the only
//! place in `ljos_render` that names egui.
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
//! drawn for a while. Once uploaded, pixels that can be made again are let
//! go ([`ImageHandle::let_go`]).

use std::sync::{Arc, Weak};

use egui::epaint::{
    ClippedShape, CornerRadius, Mesh, PaintCallback, PathShape, PathStroke, RectShape, Shadow,
    Shape, Stroke, StrokeKind, TessellationOptions, Tessellator, TextShape, Vertex,
};
use egui::{
    Color32, ColorImage, Context, FontId, Pos2, Rect as ERect, TextureFilter, TextureHandle,
    TextureId, TextureOptions, TextureWrapMode,
};
use rustc_hash::{FxHashMap, FxHashSet};
use slotmap::SecondaryMap;
use vello_cpu::kurbo::{Affine, PathEl, Point, Rect, Vec2};

use crate::b0_shape::{ShapeCache, ShapeCmd, ShapeSeg};
use crate::backend::{EncodeStats, FrameView, MemoryPressure, RenderBackend, RgbaImage};
use crate::compose::bbox_of;
use crate::display_list::{
    Decorations, DisplayList, DisplayListId, DrawOp, GlyphRun, LinearGradient, Paint, PathStyle,
    TextAlign, fill_area,
};
use crate::glyph::{baseline_snap, is_upright, snap_bar};
use crate::glyph_atlas::{BINS, Full, GlyphAtlas, GlyphKey, Slot};
use crate::image::{ImageBlit, ImageHandle, WeakImage, image_blit, nine_patch_blits};
use crate::image_atlas::{ImageAtlas, Place};
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
    /// rectangle with the painter's own. They are moved, not copied: the
    /// sink is left without shapes.
    pub fn paint(&mut self, painter: &egui::Painter) {
        let outer = painter.clip_rect();
        painter.ctx().graphics_mut(|g| {
            let list = g.entry(painter.layer_id());
            for cs in self.shapes.drain(..) {
                list.add(cs.clip_rect.intersect(outer), cs.shape);
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
    /// With mipmaps, for a shader that samples it smaller than it is.
    mipmap: bool,
    /// A texture of its own and never a place on a shared page: a shader
    /// samples it from edge to edge.
    alone: bool,
}

impl TextureKey {
    fn may_share(&self) -> bool {
        !(self.repeat || self.mipmap || self.alone)
    }
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
    /// Where on a shared page the mesh's coordinates point; `None` while
    /// they are fractions of the image.
    placed: Option<ERect>,
}

struct TextureEntry {
    texture: Held,
    owner: WeakImage,
    last_used: u64,
}

enum Held {
    Own(TextureHandle),
    Shared(Place),
}

/// The texture a picture is sampled from, and where on it the picture is
/// when that is a shared page.
type Sampled = (TextureId, Option<ERect>);

impl TextureEntry {
    fn sampled(&self) -> Sampled {
        match &self.texture {
            Held::Own(h) => (h.id(), None),
            Held::Shared(p) => (p.texture, Some(p.uv)),
        }
    }
}

/// Points the coordinates of `mesh`, laid over the picture as `from` says,
/// at it where `to` says.
fn place_uvs(mesh: &mut Mesh, from: Option<ERect>, to: Option<ERect>) {
    for v in &mut mesh.vertices {
        let f = from.map_or(v.uv, |r| {
            Pos2::new(
                (v.uv.x - r.min.x) / r.width(),
                (v.uv.y - r.min.y) / r.height(),
            )
        });
        v.uv = to.map_or(f, |r| {
            Pos2::new(r.min.x + f.x * r.width(), r.min.y + f.y * r.height())
        });
    }
}

/// The most quads one tiled blit becomes. More than this is a tile of a few
/// pixels over a whole screen; the rest is left out and reported.
const MAX_TILES: usize = 4096;

fn push_rect(mesh: &mut Mesh, r: Rect, u: (f64, f64), v: (f64, f64), color: Color32) {
    let p = |x: f64, y: f64| Pos2::new(x as f32, y as f32);
    push_quad(
        mesh,
        [p(r.x0, r.y0), p(r.x1, r.y0), p(r.x1, r.y1), p(r.x0, r.y1)],
        [p(u.0, v.0), p(u.1, v.0), p(u.1, v.1), p(u.0, v.1)],
        color,
    );
}

/// Add the quads of `blit`, in `color`, to `clamp` (sampled with clamped
/// edges) or `repeat` (sampled with a repeating texture). Returns false
/// when the tile limit cut it short.
fn blit_quads(
    blit: &ImageBlit,
    size: (u32, u32),
    smooth: bool,
    color: Color32,
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
            color,
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
            color,
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
                color,
            );
        }
    }
    true
}

impl TextureEntry {
    /// Whether the image's pixels were let go after the upload, so that
    /// this texture is the only copy of them.
    fn only_copy(&self) -> bool {
        self.owner.upgrade().is_some_and(|i| !i.is_resident())
    }
}

/// The texture for `key`, uploading the image the first time.
fn texture_for(
    textures: &mut FxHashMap<TextureKey, TextureEntry>,
    pages: &mut ImageAtlas,
    uploads: &mut u64,
    frame: u64,
    ctx: &Context,
    key: TextureKey,
    owner: &WeakImage,
) -> Option<Sampled> {
    if let Some(t) = textures.get_mut(&key) {
        t.last_used = frame;
        return Some(t.sampled());
    }
    let image = owner.upgrade()?;
    let max_side = max_texture_side(ctx);
    let fitted = fit_texture(&image, max_side)?;
    let pixels = ColorImage::from_rgba_premultiplied(
        [fitted.width() as usize, fitted.height() as usize],
        &fitted.data_to_upload(),
    );
    // The texture has them now. A picture that can be made again is not
    // held twice; what reads it later has it made again.
    drop(fitted);
    image.let_go();
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
        mipmap_mode: key.mipmap.then_some(TextureFilter::Linear),
    };
    let shared = key
        .may_share()
        .then(|| pages.place(ctx, &pixels, options, max_side))
        .flatten();
    let texture = match shared {
        Some(place) => Held::Shared(place),
        None => Held::Own(upload(
            ctx,
            format!("ljos-image-{}", key.image),
            pixels,
            options,
        )),
    };
    *uploads += 1;
    let entry = TextureEntry {
        texture,
        owner: owner.clone(),
        last_used: frame,
    };
    let sampled = entry.sampled();
    textures.insert(key, entry);
    Some(sampled)
}

/// The GPU's limit as the host said it ([`set_max_texture_side`]).
#[derive(Clone, Copy)]
struct GpuTextureSide(usize);

/// Tells the backends drawing with `ctx` the largest texture side the GPU
/// takes. A host that says it can give egui a smaller limit of its own:
/// egui makes its text atlas as wide as that limit, 2 MiB at 16384 and as
/// much again once uploaded, though Ljós draws its text with an atlas of
/// its own.
pub fn set_max_texture_side(ctx: &Context, side: usize) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::NULL, GpuTextureSide(side.max(1))));
}

/// The largest texture side the GPU takes: what the host said with
/// [`set_max_texture_side`], or else egui's limit.
pub fn max_texture_side(ctx: &Context) -> usize {
    ctx.data(|d| d.get_temp::<GpuTextureSide>(egui::Id::NULL))
        .map_or_else(|| ctx.input(|i| i.max_texture_side), |s| s.0)
        .max(1)
}

/// `ctx.load_texture`, which would check the size against egui's limit,
/// not the GPU's ([`max_texture_side`]).
pub(crate) fn upload(
    ctx: &Context,
    name: impl Into<String>,
    image: impl Into<egui::ImageData>,
    options: TextureOptions,
) -> TextureHandle {
    let manager = ctx.tex_manager();
    let id = manager.write().alloc(name.into(), image.into(), options);
    TextureHandle::new(manager, id)
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
/// kept until the atlas or the clip rectangle changes; one that left out
/// glyphs the atlas had no room for is built again in each frame that
/// draws it, until it has them all.
#[derive(Clone)]
struct GlyphSeg {
    cmd: GlyphsCmd,
    to_screen: Affine,
    opacity: f32,
    mesh: Option<GlyphMesh>,
    /// Its mesh left glyphs out, and a frame was asked for to try again:
    /// one is asked for when they go missing, not each frame they stay so.
    missed: bool,
}

#[derive(Clone)]
struct GlyphMesh {
    generation: u64,
    clip: ERect,
    /// Every glyph it places was in the atlas. One that is not is drawn as
    /// it is, but not kept as finished.
    complete: bool,
    /// Where the glyph origins are, when none was left out for being far
    /// from the clip rectangle.
    span: Option<GlyphSpan>,
    mesh: Arc<Mesh>,
}

/// The device pixels the glyph origins of upright text span, the snapped
/// rows of them: what [`place_glyphs`] tests each against the clip. A move
/// by whole pixels moves each by as much ([`glyph_shift`]), so the outermost
/// tell a moved run's test without its glyphs.
#[derive(Clone, Copy, Debug)]
struct GlyphSpan {
    x0: f64,
    x1: f64,
    y0: f64,
    y1: f64,
}

impl GlyphSpan {
    const EMPTY: Self = Self {
        x0: f64::INFINITY,
        x1: f64::NEG_INFINITY,
        y0: f64::INFINITY,
        y1: f64::NEG_INFINITY,
    };

    fn add(&mut self, x: f64, y: f64) {
        self.x0 = self.x0.min(x);
        self.x1 = self.x1.max(x);
        self.y0 = self.y0.min(y);
        self.y1 = self.y1.max(y);
    }

    fn moved(self, [dx, dy]: [f64; 2]) -> Self {
        Self {
            x0: self.x0 + dx,
            x1: self.x1 + dx,
            y0: self.y0 + dy,
            y1: self.y1 + dy,
        }
    }
}

impl GlyphSeg {
    fn mesh_for(&self, generation: u64, clip: ERect) -> Option<&Arc<Mesh>> {
        self.mesh
            .as_ref()
            .filter(|m| m.generation == generation && m.clip == clip)
            .map(|m| &m.mesh)
    }

    /// Whether its mesh needs nothing more from the atlas.
    fn finished(&self, generation: u64, clip: ERect) -> bool {
        self.mesh
            .as_ref()
            .is_some_and(|m| m.complete && m.generation == generation && m.clip == clip)
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
/// pass go through here, so they always agree on the keys. The span of the
/// glyphs of upright text none of which was left out, which is what a moved
/// run is tested by ([`places_all`]).
fn place_glyphs(
    seg: &GlyphSeg,
    ppp: f32,
    clip: ERect,
    mut f: impl FnMut(GlyphKey, f64, Placement),
) -> Option<GlyphSpan> {
    let run = &*seg.cmd.run;
    let origin = seg.cmd.origin;
    let screen_bounds = erect(bbox_of(seg.to_screen, run.bounds + origin));
    if !screen_bounds.intersects(clip) {
        return None;
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
        let mut all = true;
        let mut span = GlyphSpan::EMPTY;
        for g in &run.glyphs {
            let x = a * (f64::from(g.x) + origin.x) + e;
            let y = (d * (f64::from(g.y) + origin.y) + ty + dy).round();
            if x + reach < cx0 || x - reach > cx1 || y + reach < cy0 || y - reach > cy1 {
                all = false;
                continue;
            }
            span.add(x, y);
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
        all.then_some(span)
    } else {
        let scale = to_device.determinant().abs().sqrt();
        if scale <= 0.0 {
            return None;
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
        None
    }
}

/// Whether [`place_glyphs`] visits every glyph of `seg` in `clip`, when
/// they span `span` on the device: the test it makes of each glyph, made of
/// the outermost. `seg` is upright, as it was when `span` was taken.
fn places_all(seg: &GlyphSeg, span: GlyphSpan, ppp: f32, clip: ERect) -> bool {
    let run = &*seg.cmd.run;
    let origin = seg.cmd.origin;
    let screen_bounds = erect(bbox_of(seg.to_screen, run.bounds + origin));
    if !screen_bounds.intersects(clip) {
        return false;
    }
    let ppp = f64::from(ppp);
    let a = (Affine::scale(ppp) * seg.to_screen).as_coeffs()[0];
    let reach = f64::from(run.font_size) * a * 2.0 + 2.0;
    let (cx0, cx1) = (f64::from(clip.min.x) * ppp, f64::from(clip.max.x) * ppp);
    let (cy0, cy1) = (f64::from(clip.min.y) * ppp, f64::from(clip.max.y) * ppp);
    !(span.x0 + reach < cx0
        || span.x1 - reach > cx1
        || span.y0 + reach < cy0
        || span.y1 - reach > cy1)
}

/// How far, in points and in device pixels, every glyph of `new` is placed
/// from where it is in `old`, the same run under another transform: `None`
/// unless that is whole device pixels for each glyph, so each keeps its
/// atlas key and bin.
fn glyph_shift(
    cmd: &GlyphsCmd,
    from: Affine,
    to: Affine,
    ppp: f32,
) -> Option<(egui::Vec2, [f64; 2])> {
    if cmd.decorations.bars().next().is_some() {
        return None;
    }
    let ppp = f64::from(ppp);
    let from = Affine::scale(ppp) * from;
    let to = Affine::scale(ppp) * to;
    let [a, b, c, d, e0, f0] = from.as_coeffs();
    let [a1, b1, c1, d1, e1, f1] = to.as_coeffs();
    if [a, b, c, d] != [a1, b1, c1, d1]
        || !is_upright(from)
        || (a - d).abs() >= 1e-6 * a.abs().max(1.0)
    {
        return None;
    }
    let dx = e1 - e0;
    if dx != dx.round() {
        return None;
    }
    // Each glyph is rounded from the snapped baseline: a move of it is a
    // move of every glyph.
    let baseline = d * (f64::from(cmd.run.baseline) + cmd.origin.y);
    let dy = (baseline + f1).round() - (baseline + f0).round();
    Some((egui::vec2((dx / ppp) as f32, (dy / ppp) as f32), [dx, dy]))
}

/// A node drawn again under a transform that only moved it: a run whose
/// glyphs all keep their keys keeps its quads, moved, and asks the atlas
/// for none of them. `old` and `new` are the segments of the same list.
fn carry_glyphs(old: &[WorldSeg], new: &mut [WorldSeg], clip: ERect, ppp: f32) {
    for (o, n) in old.iter().zip(new) {
        let (WorldSeg::Glyphs(o), WorldSeg::Glyphs(n)) = (o, n) else {
            continue;
        };
        let Some((m, span)) = o.mesh.as_ref().and_then(|m| Some((m, m.span?))) else {
            continue;
        };
        let Some((by, px)) = glyph_shift(&n.cmd, o.to_screen, n.to_screen, ppp) else {
            continue;
        };
        let span = span.moved(px);
        if !places_all(n, span, ppp, clip) {
            continue;
        }
        let mut mesh = Mesh::clone(&m.mesh);
        mesh.translate(by);
        n.mesh = Some(GlyphMesh {
            generation: m.generation,
            clip,
            complete: m.complete,
            span: Some(span),
            mesh: Arc::new(mesh),
        });
        n.missed = o.missed;
    }
}

/// The segments of a node drawn again under a transform that only moved it
/// by `by` points, moved where they are: a scrolled list's rows keep their
/// meshes rather than map each again into a new one. `false`, with nothing
/// changed, where a segment keeps more than its vertices (a shape's cache, a
/// shader's callback) and is built again.
fn move_segs(segs: &mut [WorldSeg], by: Vec2, to_screen: Affine, clip: ERect, ppp: f32) -> bool {
    if segs
        .iter()
        .any(|s| matches!(s, WorldSeg::Shape(_) | WorldSeg::Shader(_)))
    {
        return false;
    }
    let shift = egui::vec2(by.x as f32, by.y as f32);
    for seg in segs {
        match seg {
            WorldSeg::Mesh(m) => Arc::make_mut(m).translate(shift),
            WorldSeg::Image(i) => Arc::make_mut(&mut i.mesh).translate(shift),
            WorldSeg::Text { rect, .. } => *rect = *rect + by,
            WorldSeg::Glyphs(g) => {
                let from = std::mem::replace(&mut g.to_screen, to_screen);
                let mesh = g.mesh.take();
                // Only a whole-pixel move keeps each glyph's atlas key; any
                // other is built from the atlas when it is drawn.
                if let Some(mut m) = mesh
                    && let Some(span) = m.span
                    && let Some((by, px)) = glyph_shift(&g.cmd, from, to_screen, ppp)
                    && places_all(g, span.moved(px), ppp, clip)
                {
                    Arc::make_mut(&mut m.mesh).translate(by);
                    m.clip = clip;
                    m.span = Some(span.moved(px));
                    g.mesh = Some(m);
                }
            }
            WorldSeg::Shape(_) | WorldSeg::Shader(_) => unreachable!("checked above"),
        }
    }
    true
}

fn push_quad(mesh: &mut Mesh, corners: [Pos2; 4], uv: [Pos2; 4], color: Color32) {
    let base = mesh.vertices.len() as u32;
    // Whole arrays: a particle system pushes hundreds of quads a frame, and
    // a vertex at a time paid a capacity check for each.
    let v = |i: usize| Vertex {
        pos: corners[i],
        uv: uv[i],
        color,
    };
    mesh.vertices.extend_from_slice(&[v(0), v(1), v(2), v(3)]);
    mesh.indices
        .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
}

fn pos2(p: Point) -> Pos2 {
    Pos2::new(p.x as f32, p.y as f32)
}

/// What a mesh of glyphs takes from the atlas besides their slots, while it
/// has a texture.
#[derive(Clone, Copy)]
struct AtlasView {
    texture: egui::TextureId,
    side: f32,
    white_uv: Pos2,
}

impl AtlasView {
    fn of(atlas: &GlyphAtlas) -> Option<Self> {
        Some(Self {
            texture: atlas.texture_id()?,
            side: atlas.side() as f32,
            white_uv: atlas.white_uv(),
        })
    }
}

/// Quads for the glyphs of `seg` that `slot` finds in the atlas, and its
/// bars, with the span [`place_glyphs`] gives and whether `slot` found every
/// glyph. `slot` is given each glyph's key and the device size it is filled
/// at, so the pass that fills the atlas builds the mesh too and makes and
/// looks up each key once.
fn glyph_mesh(
    seg: &GlyphSeg,
    atlas: AtlasView,
    ppp: f32,
    clip: ERect,
    mut slot: impl FnMut(&GlyphKey, f64) -> Option<Slot>,
) -> (Mesh, Option<GlyphSpan>, bool) {
    let mut mesh = Mesh::with_texture(atlas.texture);
    let quads = seg.cmd.run.glyphs.len() + 2;
    mesh.vertices.reserve(quads * 4);
    mesh.indices.reserve(quads * 6);
    let color = color32(seg.cmd.color.with_opacity(seg.opacity));
    let side = atlas.side;
    let inv_ppp = 1.0 / f64::from(ppp);
    let mut complete = true;
    let span = place_glyphs(seg, ppp, clip, |key, size, placement| {
        let Some(Slot {
            x,
            y,
            w,
            h,
            left,
            top,
        }) = slot(&key, size)
        else {
            complete = false;
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
        push_quad(&mut mesh, corners, [atlas.white_uv; 4], color);
    }
    (mesh, span, complete)
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
    /// The list, while the tree has it: one it let go of (its node gone or
    /// painted again) is drawn by no one, and an idle window draws too few
    /// frames for the age of an entry to say so soon.
    owner: Weak<DisplayList>,
}

#[derive(Clone, Copy, PartialEq)]
struct Sig {
    list: DisplayListId,
    to_screen: [f64; 6],
    opacity: f32,
}

/// How far `new` places a list from where `old` did, when that is all that
/// changed.
fn moved_by(old: &Sig, new: &Sig) -> Option<Vec2> {
    let (a, b) = (old.to_screen, new.to_screen);
    (old.list == new.list && old.opacity == new.opacity && a[..4] == b[..4])
        .then(|| Vec2::new(b[4] - a[4], b[5] - a[5]))
}

/// Moves in a row before a node's meshes are mapped again from its list:
/// each adds the rounding of an `f32` sum, and that must not add up.
const MAX_MOVES: u8 = 32;

struct NodeEntry {
    sig: Sig,
    segs: Vec<WorldSeg>,
    last_used: u64,
    /// Moves since the meshes were last mapped from the list.
    moves: u8,
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
    world_moves: u64,
    glyph_meshes_built: u64,
    /// [`EncodeStats::warnings`] of the last `encode`.
    warnings: u32,
    atlas: GlyphAtlas,
    textures: FxHashMap<TextureKey, TextureEntry>,
    /// The shared pages small pictures are put on.
    pages: ImageAtlas,
    texture_uploads: u64,
    shapes: ShapeCache,
    /// The nodes that hold a shape texture, so the ones that left the frame
    /// are found without walking every node.
    textured: FxHashSet<NodeId>,
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
    let [sx, ky, kx, sy, tx, ty] = to_screen.as_coeffs();
    let upright = ky == 0.0 && kx == 0.0 && sx > 0.0 && sy > 0.0;
    if !upright || radii.iter().any(|r| *r > 0.0) {
        return None;
    }
    // Every frame of an animation asks this of each rectangle again.
    let fill = if opacity >= 1.0 {
        *color
    } else {
        color.with_opacity(opacity)
    };
    if !Paint::Solid(fill).is_opaque() {
        return None;
    }
    // Upright and growing on both axes: its corners map to corners, in
    // order, unless the rectangle is turned inside out.
    let screen = if rect.x0 <= rect.x1 && rect.y0 <= rect.y1 {
        Rect::new(
            sx * rect.x0 + tx,
            sy * rect.y0 + ty,
            sx * rect.x1 + tx,
            sy * rect.y1 + ty,
        )
    } else {
        to_screen.transform_rect_bbox(*rect)
    };
    Some((erect(screen), color32(fill), *aa))
}

/// The pixels a solid rectangle covers, as a rectangle on whole device
/// pixels, when they are the pixels of a rectangle: a hard-edged one
/// covers the pixels whose centres it holds; an antialiased one only
/// covers whole pixels when its edges lie between them.
fn pixels_of(rect: ERect, aa: bool, ppp: f32) -> Option<ERect> {
    if aa {
        return on_pixels(rect, ppp).then_some(rect);
    }
    // A pixel is in when its centre is: x0 <= i + 0.5 < x1. The edges are
    // then whole pixels by making, within what `near_whole` allows, so
    // they are not checked again for each rectangle of every frame.
    let edge = |v: f32| ceil(v * ppp - 0.5) / ppp;
    let snapped = ERect::from_min_max(
        Pos2::new(edge(rect.min.x), edge(rect.min.y)),
        Pos2::new(edge(rect.max.x), edge(rect.max.y)),
    );
    (snapped.is_positive() && snapped.is_finite()).then_some(snapped)
}

/// Whether every edge of `rect` (in points) lies on a device pixel's
/// edge, where a feathered edge and a hard one cover the same pixels.
fn on_pixels(rect: ERect, ppp: f32) -> bool {
    rect.is_positive()
        && all_near_whole([rect.min.x, rect.min.y, rect.max.x, rect.max.y].map(|v| v * ppp))
}

/// Whether `px` lies within a thousandth of a whole number:
/// `(px - px.round()).abs() < 1e-3`, without the call to libm that
/// `round` is where the CPU has no rounding instruction, four times for
/// each rectangle of every frame.
pub fn near_whole(px: f32) -> bool {
    // Below 2^22, adding 1.5 × 2^23 leaves only whole numbers, so the sum
    // holds `px` rounded to the nearest, and taking it away again is exact.
    const ROUND: f32 = 12_582_912.0;
    if px.abs() < 4_194_304.0 {
        return (px - ((px + ROUND) - ROUND)).abs() < 1e-3;
    }
    // From 2^23 on every `f32` is whole; below it, halves are not.
    px.is_finite() && (px.abs() >= 8_388_608.0 || px == (px as i32) as f32)
}

/// [`near_whole`] of each of `px`, the four edges of a rectangle: asked of
/// every rectangle of every frame, and with no branch for each edge.
pub fn all_near_whole(px: [f32; 4]) -> bool {
    const ROUND: f32 = 12_582_912.0;
    let [a, b, c, d] = px;
    // `&`, not `&&`: four comparisons and one branch. NaN is not small.
    let small = |v: f32| v.abs() < 4_194_304.0;
    if small(a) & small(b) & small(c) & small(d) {
        let near = |v: f32| (v - ((v + ROUND) - ROUND)).abs() < 1e-3;
        return near(a) & near(b) & near(c) & near(d);
    }
    px.iter().all(|&v| near_whole(v))
}

/// `v.ceil()` without the call to libm (see [`near_whole`]).
fn ceil(v: f32) -> f32 {
    // As in `near_whole`: `r` is `v` rounded to the nearest, so within a
    // half of it, and one up from it when below. Four of these for every
    // rectangle of every frame: going by way of `i32` was twice the work.
    const ROUND: f32 = 12_582_912.0;
    // NaN, the infinities and the large go to `ceil` itself.
    if v.abs() < 4_194_304.0 {
        let r = (v + ROUND) - ROUND;
        // The sign stays, so `-0.3` gives `-0`, as `ceil` does.
        (if r < v { r + 1.0 } else { r }).copysign(v)
    } else {
        v.ceil()
    }
}

/// What is drawn, in paint order.
enum Drawn {
    /// A node's shapes, in its clip rectangle.
    Node(NodeId, ERect),
    /// A rectangle handed over as one ([`EguiSink::solid_rects`]), in its
    /// clip rectangle; made a shape only as it is handed over, as the
    /// shape is four times its size.
    Solid(ERect, ERect, Color32),
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
            world_moves: 0,
            glyph_meshes_built: 0,
            warnings: 0,
            atlas: GlyphAtlas::default(),
            textures: FxHashMap::default(),
            pages: ImageAtlas::default(),
            texture_uploads: 0,
            shapes: ShapeCache::default(),
            textured: FxHashSet::default(),
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

    /// How many shared pages small pictures are on.
    pub fn image_pages(&self) -> usize {
        self.pages.pages()
    }

    /// How many image textures were uploaded so far (tests use it to prove an
    /// image is uploaded once, not once per frame or per item).
    pub fn texture_uploads(&self) -> u64 {
        self.texture_uploads
    }

    /// What it keeps from frame to frame: one pass over its lists and
    /// nodes. A mesh two of them share is counted once.
    pub fn memory(&self) -> crate::BackendMemory {
        fn mesh_bytes(m: &Mesh) -> usize {
            m.indices.capacity() * size_of::<u32>()
                + m.vertices.capacity() * size_of::<egui::epaint::Vertex>()
        }
        let mut seen: FxHashSet<*const Mesh> = FxHashSet::default();
        let mut m = crate::BackendMemory::default();
        let mut mesh = |arc: &Arc<Mesh>, m: &mut crate::BackendMemory| {
            if seen.insert(Arc::as_ptr(arc)) {
                m.meshes += 1;
                m.mesh_bytes += mesh_bytes(arc);
            }
        };
        for entry in self.lists.values() {
            for seg in entry.segs.iter() {
                match seg {
                    LocalSeg::Mesh(arc) => mesh(arc, &mut m),
                    LocalSeg::Image(i) => mesh(&i.mesh, &mut m),
                    _ => {}
                }
            }
        }
        for entry in self.nodes.values() {
            for seg in &entry.segs {
                match seg {
                    WorldSeg::Mesh(arc) => mesh(arc, &mut m),
                    WorldSeg::Image(i) => mesh(&i.mesh, &mut m),
                    WorldSeg::Glyphs(g) => {
                        if let Some(g) = &g.mesh {
                            mesh(&g.mesh, &mut m);
                            m.glyph_runs_incomplete += usize::from(!g.complete);
                        }
                    }
                    WorldSeg::Shape(s) => {
                        let (shape_mesh, texture) = s.held();
                        if let Some(arc) = shape_mesh {
                            mesh(arc, &mut m);
                        }
                        if texture > 0 {
                            m.shape_textures += 1;
                            m.shape_texture_bytes += texture;
                        }
                    }
                    _ => {}
                }
            }
        }
        let side = self.atlas.side();
        m.glyphs = self.atlas.len();
        m.glyph_atlas_bytes = self.atlas.alpha().len()
            + if self.atlas.texture_id().is_some() {
                side * side * 4
            } else {
                0
            };
        for entry in self.textures.values() {
            if let Held::Own(handle) = &entry.texture {
                let [w, h] = handle.size();
                m.textures += 1;
                m.texture_bytes += w * h * 4;
            }
        }
        m.pages = self.pages.pages();
        let page = crate::image_atlas::PAGE as usize;
        m.page_bytes = m.pages * page * page * 4;
        m.glyph_meshes_built = self.glyph_meshes_built;
        m.warnings = self.warnings;
        m
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

    /// How many display lists are held now.
    pub fn display_lists(&self) -> usize {
        self.lists.len()
    }

    /// How many per-node world meshes were re-mapped so far.
    pub fn world_rebuilds(&self) -> u64 {
        self.world_rebuilds
    }

    /// Of [`Self::world_rebuilds`], those that moved the meshes a node
    /// already had, as a translation alone does.
    pub fn world_moves(&self) -> u64 {
        self.world_moves
    }

    /// Glyph runs whose quads were built from the atlas, not moved.
    pub fn glyph_meshes_built(&self) -> u64 {
        self.glyph_meshes_built
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

    fn build_list(&mut self, list: &Arc<DisplayList>) -> ListEntry {
        self.tessellations += 1;
        let mut segs: Vec<LocalSeg> = Vec::new();
        let mut cur = Mesh::default();
        let mut warnings = 0u32;
        let mut warn = |what: &str| {
            warnings += 1;
            tracing::debug!("B0: {what}");
        };
        let ops = list.ops();
        for (at, op) in ops.iter().enumerate() {
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
                    // The untiled pictures of this image that follow, a
                    // particle system's, each one quad of the same mesh.
                    let run = || {
                        ops[at..]
                            .iter()
                            .take_while(|o| {
                                matches!(o, DrawOp::Image { image: i, tile: None, smooth: s, .. }
                                    if i.id() == image.id() && s == smooth)
                            })
                            .count()
                    };
                    if !image_segs(&mut segs, image, &[blit], *smooth, *opacity, *tint, run) {
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
                    if !image_segs(&mut segs, image, &blits, *smooth, 1.0, None, || 0) {
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
            owner: Arc::downgrade(list),
        }
    }

    /// Put every glyph this frame draws into the atlas, then upload it. A
    /// run is given its mesh as its glyphs go in, unless the atlas moved
    /// them (it grew or was emptied) after the first of them went in: that
    /// one is built when it is drawn.
    fn fill_atlas(&mut self, ctx: &Context, order: &[Drawn], stats: &mut EncodeStats) {
        let max_side = GlyphAtlas::max_side(ctx);
        let ppp = self.ppp;
        for attempt in 0..2 {
            let mut full = false;
            for drawn in order {
                let Drawn::Node(node, clip) = drawn else {
                    continue;
                };
                let Some(ne) = self.nodes.get_mut(*node) else {
                    continue;
                };
                for seg in &mut ne.segs {
                    let WorldSeg::Glyphs(g) = seg else { continue };
                    let generation = self.atlas.generation();
                    // A finished mesh means its glyphs are already in the
                    // atlas; one that left some out looks for them again.
                    if g.finished(generation, *clip) {
                        continue;
                    }
                    let view = AtlasView::of(&self.atlas);
                    let atlas = &mut self.atlas;
                    let run = &g.cmd.run;
                    let mut ensure =
                        |key: &GlyphKey, size| match atlas.ensure(key, run, size, max_side) {
                            Ok(slot) => Some(slot),
                            Err(Full) => {
                                full = true;
                                None
                            }
                        };
                    let Some(view) = view else {
                        place_glyphs(g, ppp, *clip, |key, size, _| {
                            ensure(&key, size);
                        });
                        continue;
                    };
                    let (mesh, span, complete) = glyph_mesh(g, view, ppp, *clip, ensure);
                    if self.atlas.generation() == generation {
                        self.glyph_meshes_built += 1;
                        g.mesh = Some(GlyphMesh {
                            generation,
                            clip: *clip,
                            complete,
                            span,
                            mesh: Arc::new(mesh),
                        });
                    }
                }
            }
            if !full {
                break;
            }
            if attempt == 0 {
                // No room without letting go of the glyphs of earlier
                // frames: keep this frame's, copied, and grow only for them.
                self.atlas.compact();
            } else {
                stats.warnings += 1;
                tracing::debug!("B0: the glyph atlas is full; some glyphs are not drawn");
            }
        }
        self.atlas.end_frame(max_side);
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
                    missed: false,
                }),
                LocalSeg::Image(seg) => WorldSeg::Image(ImageSeg {
                    key: seg.key,
                    image: seg.image.clone(),
                    mesh: Arc::new(map_mesh(&seg.mesh, to_screen, opacity)),
                    placed: None,
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
    let fade = |c: Color32| {
        if opacity < 1.0 {
            c.gamma_multiply(opacity)
        } else {
            c
        }
    };
    let [sx, ky, kx, sy, tx, ty] = to_screen.as_coeffs();
    if ky == 0.0 && kx == 0.0 {
        // Upright, as nearly every item is: each axis on its own, the same
        // sums the whole map makes less its terms of nothing.
        let along = |s: f64, t: f64, p: f32| (s * f64::from(p) + t) as f32;
        out.vertices.extend(m.vertices.iter().map(|v| Vertex {
            pos: Pos2::new(along(sx, tx, v.pos.x), along(sy, ty, v.pos.y)),
            uv: v.uv,
            color: fade(v.color),
        }));
        return out;
    }
    for v in &m.vertices {
        let p = to_screen * Point::new(f64::from(v.pos.x), f64::from(v.pos.y));
        out.vertices.push(Vertex {
            pos: Pos2::new(p.x as f32, p.y as f32),
            uv: v.uv,
            color: fade(v.color),
        });
    }
    out
}

/// Append the quads of `blits`, faded by `opacity`, at most one segment
/// per sampling mode. `quads_ahead` says how many quads a new segment is
/// likely to be given. Quads of the texture the last segment draws join
/// it, so many pictures of one image (a particle system's) are one mesh.
/// Returns false when a blit hit the tile limit.
fn image_segs(
    segs: &mut Vec<LocalSeg>,
    image: &ImageHandle,
    blits: &[ImageBlit],
    smooth: bool,
    opacity: f32,
    tint: Option<Rgba>,
    quads_ahead: impl FnOnce() -> usize,
) -> bool {
    let size = (image.width(), image.height());
    // egui multiplies the texture by the vertex colour, so a tint costs no
    // texture of its own and the quads still join the batch of the picture.
    // egui blends premultiplied, so channels above the alpha add light.
    let tint = tint.map_or(Color32::WHITE, |t| {
        let [r, g, b, a] = t.to_premul_u8();
        Color32::from_rgba_premultiplied(r, g, b, a)
    });
    // One colour for all the quads, worked out once and not for each corner
    // of each of a particle system's pictures.
    let tint = if opacity < 1.0 {
        tint.gamma_multiply(opacity)
    } else {
        tint
    };
    let key = |repeat| TextureKey {
        image: image.id(),
        smooth,
        repeat,
        mipmap: false,
        alone: false,
    };
    // Clamped quads are made right in the mesh of the segment they would
    // join: a particle system draws each of hundreds of pictures alone,
    // and a mesh of its own for each, appended and freed, was most of
    // what drawing them cost. Taking that mesh out and putting it back
    // for each picture cost nearly as much as making its quad.
    let joins = match segs.last_mut() {
        Some(LocalSeg::Image(last)) => {
            last.key == key(false) && Arc::get_mut(&mut last.mesh).is_some()
        }
        _ => false,
    };
    if !joins {
        // Grown once for the run, not doubled and copied as it goes.
        let quads = quads_ahead();
        let mesh = Mesh {
            vertices: Vec::with_capacity(4 * quads),
            indices: Vec::with_capacity(6 * quads),
            ..Mesh::default()
        };
        segs.push(LocalSeg::Image(ImageSeg {
            key: key(false),
            image: image.downgrade(),
            mesh: Arc::new(mesh),
            placed: None,
        }));
    }
    let Some(LocalSeg::Image(last)) = segs.last_mut() else {
        unreachable!("an image segment is last")
    };
    let clamp = Arc::get_mut(&mut last.mesh).expect("the last mesh is this call's own");
    let mut repeat = Mesh::default();
    let mut complete = true;
    for blit in blits {
        complete &= blit_quads(blit, size, smooth, tint, clamp, &mut repeat);
    }
    if !joins && clamp.is_empty() {
        segs.pop();
    }
    if !repeat.is_empty() {
        if let Some(LocalSeg::Image(last)) = segs.last_mut()
            && last.key == key(true)
            && let Some(joined) = Arc::get_mut(&mut last.mesh)
        {
            joined.append(repeat);
        } else {
            segs.push(LocalSeg::Image(ImageSeg {
                key: key(true),
                image: image.downgrade(),
                mesh: Arc::new(repeat),
                placed: None,
            }));
        }
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
        self.lists
            .retain(|_, e| e.owner.strong_count() > 0 && e.last_used >= keep_after);
        self.retain_nodes(keep_after);
        // A texture outlives neither its image nor a long stretch unseen,
        // unless it is all there is of the pixels: dropping it would cost
        // a decode when the picture is seen again. Pressure drops it all
        // the same.
        self.textures
            .retain(|_, t| t.owner.is_alive() && (t.last_used >= keep_after || t.only_copy()));
        self.pages.sweep();
    }

    fn encode(&mut self, view: &FrameView<'_>, sink: &mut EguiSink<'_>) -> EncodeStats {
        self.set_ppp(view.scale_factor);
        let snap = view.snapshot;
        self.frame = snap.frame;
        self.encoded = snap.frame;
        self.shapes.begin_frame();
        let mut stats = EncodeStats::default();
        let mut warned_clips = FxHashSet::default();
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
                order.push(Drawn::Solid(clip, rect, color));
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
            let moved = !reuse
                && self.nodes.get_mut(item.node).is_some_and(|n| {
                    let Some(by) = moved_by(&n.sig, &sig).filter(|_| n.moves < MAX_MOVES) else {
                        return false;
                    };
                    move_segs(&mut n.segs, by, to_screen, clip, self.ppp) && {
                        n.sig = sig;
                        n.moves += 1;
                        true
                    }
                });
            if moved {
                self.world_rebuilds += 1;
                self.world_moves += 1;
            } else if !reuse {
                let mut segs = Self::world_segs(&local, to_screen, item.opacity);
                if let Some(old) = self.nodes.get_mut(item.node) {
                    hand_over(&mut self.shapes, &mut old.segs, &mut segs);
                    if old.sig.list == sig.list && old.sig.opacity == sig.opacity {
                        carry_glyphs(&old.segs, &mut segs, clip, self.ppp);
                    }
                }
                self.world_rebuilds += 1;
                self.nodes.insert(
                    item.node,
                    NodeEntry {
                        sig,
                        segs,
                        last_used: self.frame,
                        moves: 0,
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
                Drawn::Solid(clip_rect, rect, color) => {
                    sink.shapes.push(ClippedShape::new(
                        clip_rect,
                        Shape::Rect(RectShape::filled(rect, CornerRadius::ZERO, color)),
                    ));
                    continue;
                }
            };
            let ne = self.nodes.get_mut(node).expect("present");
            for seg in &mut ne.segs {
                match seg {
                    WorldSeg::Mesh(m) => sink
                        .shapes
                        .push(ClippedShape::new(clip, Shape::Mesh(Arc::clone(m)))),
                    WorldSeg::Image(seg) => {
                        let Some(ctx) = sink.ctx else {
                            stats.warnings += 1;
                            tracing::debug!("B0: an image needs an egui Context; skipped");
                            continue;
                        };
                        let Some((texture, uv)) = texture_for(
                            &mut self.textures,
                            &mut self.pages,
                            &mut self.texture_uploads,
                            self.frame,
                            ctx,
                            seg.key,
                            &seg.image,
                        ) else {
                            continue;
                        };
                        if seg.mesh.texture_id != texture || seg.placed != uv {
                            let mesh = Arc::make_mut(&mut seg.mesh);
                            mesh.texture_id = texture;
                            place_uvs(mesh, seg.placed, uv);
                            seg.placed = uv;
                        }
                        sink.shapes
                            .push(ClippedShape::new(clip, Shape::Mesh(Arc::clone(&seg.mesh))));
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
                            sink.shapes.push(ClippedShape::new(clip, Shape::Mesh(mesh)));
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
                                    // The painter's sampler wraps.
                                    repeat: false,
                                    mipmap: t.mipmap,
                                    alone: true,
                                };
                                texture_for(
                                    &mut self.textures,
                                    &mut self.pages,
                                    &mut self.texture_uploads,
                                    self.frame,
                                    ctx,
                                    key,
                                    &image.downgrade(),
                                )
                                .map(|(id, _)| id)
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
                        sink.shapes.push(ClippedShape::new(
                            clip,
                            Shape::Callback(PaintCallback {
                                rect: erect(seg.to_screen.transform_rect_bbox(seg.rect)),
                                callback: Arc::clone(paint) as _,
                            }),
                        ));
                    }
                    WorldSeg::Glyphs(g) => {
                        if sink.ctx.is_none() {
                            continue;
                        }
                        let generation = self.atlas.generation();
                        if g.mesh_for(generation, clip).is_none() {
                            self.glyph_meshes_built += 1;
                            // The atlas was uploaded before this, so it has
                            // its texture.
                            let (mesh, span, complete) = match AtlasView::of(&self.atlas) {
                                Some(view) => glyph_mesh(g, view, self.ppp, clip, |key, _| {
                                    self.atlas.slot(key)
                                }),
                                None => (Mesh::default(), None, false),
                            };
                            g.mesh = Some(GlyphMesh {
                                generation,
                                clip,
                                complete,
                                span,
                                mesh: Arc::new(mesh),
                            });
                        }
                        // A run with glyphs missing is filled again by the
                        // next frame, which may have room for them: ask for
                        // that frame once, so a shortage that lasts does
                        // not keep an idle window drawing.
                        let missing = !g.finished(generation, clip);
                        stats.again |= missing && !g.missed;
                        g.missed = missing;
                        if let Some(m) = g.mesh_for(generation, clip)
                            && !m.is_empty()
                        {
                            sink.shapes
                                .push(ClippedShape::new(clip, Shape::Mesh(Arc::clone(m))));
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
                        sink.shapes
                            .push(ClippedShape::new(clip, Shape::Text(shape)));
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
        // A node the tree made again in the slot of one it removed has
        // just replaced that one's entry: what only it used goes now, not
        // frames later.
        self.lists.retain(|_, e| e.owner.strong_count() > 0);
        self.shapes.forget_orphans();
        stats.shapes = sink.shapes.len() as u32;
        self.warnings = stats.warnings;
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
                self.pages.sweep();
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
                self.pages.sweep();
            }
        }
    }

    /// B0 draws through egui's own renderer, so there are no pixels here; use R
    /// (`VelloCpuReference`) or egui_kittest's wgpu renderer.
    fn snapshot_rgba(&mut self, _view: &FrameView<'_>) -> Option<RgbaImage> {
        None
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use egui::{Rect as ERect, pos2};
    use vello_cpu::kurbo::{Affine, Rect, Vec2};
    use vello_cpu::peniko::{Blob, FontData};

    use super::{
        GlyphSeg, GlyphsCmd, all_near_whole, ceil, glyph_shift, near_whole, place_glyphs,
        places_all,
    };
    use crate::display_list::{Decorations, GlyphRun, PositionedGlyph};
    use crate::rgba::Rgba;

    /// A run moved by whole pixels is told near its clip from its span, as
    /// placing each of its glyphs again tells it.
    #[test]
    fn a_moved_run_is_near_its_clip_as_its_glyphs_are() {
        let font = FontData::new(Blob::new(Arc::new([0u8; 16])), 0);
        let glyphs = (0..12)
            .map(|i| PositionedGlyph {
                id: i,
                x: i as f32 * 7.5,
                y: 12.0 + (i % 3) as f32 * 0.4,
            })
            .collect();
        let bounds = Rect::new(-13.0, -8.0, 103.0, 26.0);
        let run = Arc::new(GlyphRun::new(font, 13.0, glyphs, 12.0, bounds));
        let seg = |to_screen| GlyphSeg {
            cmd: GlyphsCmd {
                run: Arc::clone(&run),
                origin: Vec2::new(3.0, 1.25),
                color: Rgba::BLACK,
                decorations: Decorations::default(),
            },
            to_screen,
            opacity: 1.0,
            mesh: None,
            missed: false,
        };
        let clip = ERect::from_min_max(pos2(40.0, 30.0), pos2(160.0, 90.0));
        let (mut near, mut far) = (0, 0);
        for ppp in [1.0, 1.5, 2.0] {
            let from = Affine::translate((0.0, 0.6));
            let span = place_glyphs(&seg(from), ppp, ERect::EVERYTHING, |_, _, _| {})
                .expect("upright, and nothing is far from everything");
            for k in (-90..150).step_by(7) {
                for j in (-200..300).step_by(9) {
                    let by = (f64::from(k) / f64::from(ppp), f64::from(j) * 0.37);
                    let to = Affine::translate(by) * from;
                    let Some((_, px)) = glyph_shift(&seg(to).cmd, from, to, ppp) else {
                        continue;
                    };
                    let placed = place_glyphs(&seg(to), ppp, clip, |_, _, _| {}).is_some();
                    assert_eq!(
                        places_all(&seg(to), span.moved(px), ppp, clip),
                        placed,
                        "ppp {ppp}, moved by {by:?}"
                    );
                    if placed { near += 1 } else { far += 1 }
                }
            }
        }
        assert!(near > 100 && far > 100, "{near} near, {far} far");
    }

    #[test]
    fn the_rounding_without_libm_says_what_libm_does() {
        let mut values = vec![
            0.0,
            -0.0,
            0.5,
            -0.5,
            0.0005,
            0.001,
            0.999,
            0.9995,
            -0.9995,
            2.9995,
            8_388_607.5,
            8_388_608.0,
            -8_388_609.0,
            1e30,
            f32::MAX,
            f32::MIN_POSITIVE,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::NAN,
        ];
        // Fine steps across a few pixels either side of 0, and small
        // values near the threshold.
        values.extend((-40_000..40_000).map(|i| i as f32 / 10_000.0 * 3.7));
        values.extend((0..2000).map(|i| f32::from_bits(0x3a80_0000 + i * 977)));
        // Either side of 2^22 and 2^23, where halves stop and start.
        values.extend([
            4_194_303.5,
            4_194_304.0,
            4_194_304.5,
            6_000_000.5,
            8_388_607.5,
        ]);
        values.extend([-4_194_304.5, -6_000_001.5, 8_388_608.0, 8_388_610.0]);
        for v in values {
            assert_eq!(near_whole(v), (v - v.round()).abs() < 1e-3, "{v}");
            for w in [0.0, 1.5, 3.0004, f32::NAN, 5_000_000.0] {
                assert_eq!(
                    all_near_whole([w, v, 2.0, w]),
                    near_whole(w) && near_whole(v),
                    "{w} {v}"
                );
            }
            let (ours, theirs) = (ceil(v), v.ceil());
            assert!(
                ours.to_bits() == theirs.to_bits() || (ours.is_nan() && theirs.is_nan()),
                "{v}: {ours} != {theirs}"
            );
        }
    }
}
