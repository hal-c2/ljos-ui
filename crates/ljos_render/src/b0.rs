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
//! * `Image`, `NinePatch`, `Glyphs`, `Embed` and `layer.*` are not drawn yet;
//! * the text stop-gap ignores rotation and shear.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use egui::epaint::{
    ClippedShape, CornerRadius, Mesh, PathShape, PathStroke, RectShape, Shadow, Shape, Stroke,
    StrokeKind, TessellationOptions, Tessellator, TextShape, Vertex,
};
use egui::{Color32, Context, FontId, Pos2, Rect as ERect};
use vello_cpu::kurbo::{Affine, PathEl, Point, Rect};

use crate::backend::{EncodeStats, FrameView, MemoryPressure, RenderBackend, RgbaImage};
use crate::display_list::{
    DisplayList, DisplayListId, DrawOp, LinearGradient, Paint, PathStyle, TextAlign,
};
use crate::rgba::Rgba;
use crate::tree::NodeId;

/// Entries unused for this many frames are dropped by [`RenderBackend::sync`].
const KEEP_FRAMES: u64 = 120;

/// Where B0 writes: a flat list of clipped shapes in paint order, plus what it
/// needs to place them.
#[derive(Debug)]
pub struct EguiSink<'a> {
    /// Needed for the text stop-gap (egui's font layout). Without it, text is skipped.
    pub ctx: Option<&'a Context>,
    /// Screen position of the snapshot viewport's top-left corner, in points.
    pub origin: Pos2,
    /// Output, back to front.
    pub shapes: Vec<ClippedShape>,
}

impl<'a> EguiSink<'a> {
    /// A sink that places the viewport at `origin`.
    pub fn new(ctx: Option<&'a Context>, origin: Pos2) -> Self {
        Self {
            ctx,
            origin,
            shapes: Vec::new(),
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
    lists: HashMap<DisplayListId, ListEntry>,
    nodes: HashMap<NodeId, NodeEntry>,
    frame: u64,
    tessellations: u64,
    world_rebuilds: u64,
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
            lists: HashMap::new(),
            nodes: HashMap::new(),
            frame: 0,
            tessellations: 0,
            world_rebuilds: 0,
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
                    match fill {
                        Paint::Solid(c) if !c.is_transparent() => {
                            tess.tessellate_rect(&RectShape::filled(r, cr, color32(*c)), &mut cur);
                        }
                        Paint::LinearGradient(g) if !fill.is_invisible() => {
                            let mut tmp = Mesh::default();
                            tess.tessellate_rect(
                                &RectShape::filled(r, cr, Color32::WHITE),
                                &mut tmp,
                            );
                            apply_gradient(&tmp, g, &mut cur);
                        }
                        Paint::Image(_) => warn("image fills are not drawn"),
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
                DrawOp::Image { .. } | DrawOp::NinePatch { .. } => {
                    warn("image ops are not drawn (no texture cache in M0)");
                }
                DrawOp::Glyphs { .. } => warn("Glyphs are not drawn; B0 uses the Text stop-gap"),
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

    fn world_segs(local: &[LocalSeg], to_screen: Affine, opacity: f32) -> Vec<WorldSeg> {
        let scale = to_screen.determinant().abs().sqrt();
        local
            .iter()
            .map(|seg| match seg {
                LocalSeg::Mesh(m) => {
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
                    WorldSeg::Mesh(Arc::new(out))
                }
                LocalSeg::Text(t) => WorldSeg::Text {
                    rect: to_screen.transform_rect_bbox(t.rect),
                    text: Arc::clone(&t.text),
                    size_px: t.size * scale as f32,
                    color: color32(t.color),
                    opacity,
                    align: t.align,
                },
            })
            .collect()
    }
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
        self.nodes.retain(|_, e| e.last_used >= keep_after);
    }

    fn encode(&mut self, view: &FrameView<'_>, sink: &mut EguiSink<'_>) -> EncodeStats {
        self.set_ppp(view.scale_factor);
        let snap = view.snapshot;
        self.frame = snap.frame;
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

            let reuse = self.nodes.get(&item.node).is_some_and(|n| n.sig == sig);
            if !reuse {
                let segs = Self::world_segs(&local, to_screen, item.opacity);
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
            let ne = self.nodes.get_mut(&item.node).expect("present");
            ne.last_used = self.frame;

            for seg in &ne.segs {
                match seg {
                    WorldSeg::Mesh(m) => sink.shapes.push(ClippedShape {
                        clip_rect: clip,
                        shape: Shape::Mesh(Arc::clone(m)),
                    }),
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
                            tracing::debug!("B0: text stop-gap needs an egui Context; skipped");
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
        stats.shapes = sink.shapes.len() as u32;
        stats
    }

    fn trim(&mut self, pressure: MemoryPressure) {
        match pressure {
            MemoryPressure::Critical => {
                self.lists.clear();
                self.nodes.clear();
            }
            MemoryPressure::Moderate => {
                let keep_after = self.frame.saturating_sub(10);
                self.lists.retain(|_, e| e.last_used >= keep_after);
                self.nodes.retain(|_, e| e.last_used >= keep_after);
            }
        }
    }

    /// B0 draws through egui's own renderer, so there are no pixels here; use R
    /// (`VelloCpuReference`) or egui_kittest's wgpu renderer.
    fn snapshot_rgba(&mut self, _view: &FrameView<'_>) -> Option<RgbaImage> {
        None
    }
}
