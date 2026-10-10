//! R: the deterministic reference renderer on vello_cpu.
//!
//! Determinism comes from a fixed SIMD level (`Level::baseline`, chosen at
//! compile time, never by runtime CPU detection), one thread
//! (the `multithreading` feature is not enabled and `num_threads` is 0) and
//! unhinted glyph outlines (shaping happens earlier, in `ljos_text`). Opacity
//! is applied per op (multiplied down), as Qt does, not by grouping a subtree
//! into a layer.

use std::collections::HashSet;

use vello_cpu::color::{AlphaColor, Srgb};
use vello_cpu::kurbo::{Affine, Rect, RoundedRect, RoundedRectRadii, Shape, Stroke, Vec2};
use vello_cpu::peniko::{
    ColorStop, ColorStops, Extend, Fill, Gradient, ImageAlphaType, LinearGradientPosition,
};
use vello_cpu::{
    Level, PaintType, Pixmap, RasterizerSettings, RenderContext, RenderMode, RenderSettings,
    Resources,
};

use crate::backend::{
    EncodeStats, FrameView, MemoryPressure, RenderBackend, RenderError, RgbaImage,
};
use crate::compose::{ClipId, FrameSnapshot};
use crate::display_list::{Decorations, DrawOp, GlyphRun, Paint, PathStyle};
use crate::glyph::{OutlineCache, baseline_snap, snap_bar};
use crate::rgba::Rgba;

/// How R draws the unshaped [`DrawOp::Text`], which only egui can lay out.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextMode {
    /// Draw nothing.
    #[default]
    Skip,
    /// Draw the layout box at 25 % of the text colour, so tests can see placement.
    Boxes,
}

/// Options for [`VelloCpuReference`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RasterConfig {
    /// Colour the frame is cleared to. Transparent by default.
    pub background: Rgba,
    /// What to draw for an unshaped [`DrawOp::Text`].
    pub text: TextMode,
}

impl Default for RasterConfig {
    fn default() -> Self {
        Self {
            background: Rgba::TRANSPARENT,
            text: TextMode::Skip,
        }
    }
}

/// R backend. Renders a [`FrameSnapshot`] to straight-alpha RGBA8.
pub struct VelloCpuReference {
    config: RasterConfig,
    ctx: Option<RenderContext>,
    resources: Resources,
    outlines: OutlineCache,
}

impl std::fmt::Debug for VelloCpuReference {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VelloCpuReference")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl Default for VelloCpuReference {
    fn default() -> Self {
        Self::new(RasterConfig::default())
    }
}

fn alpha_color(c: Rgba) -> AlphaColor<Srgb> {
    let [r, g, b, a] = c.to_straight();
    AlphaColor::new([r, g, b, a])
}

fn rounded_rect(rect: Rect, radii: [f32; 4]) -> RoundedRect {
    RoundedRect::from_rect(
        rect,
        RoundedRectRadii::new(
            f64::from(radii[0]),
            f64::from(radii[1]),
            f64::from(radii[2]),
            f64::from(radii[3]),
        ),
    )
}

fn to_paint(paint: &Paint) -> Option<PaintType> {
    match paint {
        Paint::Solid(c) => Some(PaintType::Solid(alpha_color(*c))),
        Paint::LinearGradient(g) => {
            let stops: Vec<ColorStop> = g
                .stops
                .iter()
                .map(|s| ColorStop::from((s.offset, alpha_color(s.color))))
                .collect();
            Some(PaintType::Gradient(Gradient {
                kind: LinearGradientPosition {
                    start: g.start,
                    end: g.end,
                }
                .into(),
                stops: ColorStops::from(stops.as_slice()),
                extend: Extend::Pad,
                ..Default::default()
            }))
        }
        Paint::Image(_) => None,
    }
}

impl VelloCpuReference {
    /// New renderer.
    pub fn new(config: RasterConfig) -> Self {
        Self {
            config,
            ctx: None,
            resources: Resources::new(),
            outlines: OutlineCache::default(),
        }
    }

    /// Render at `viewport.size() * scale_factor` physical pixels.
    pub fn render(&mut self, view: &FrameView<'_>) -> Result<RgbaImage, RenderError> {
        let vp = view.snapshot.viewport;
        let w = (vp.width() * f64::from(view.scale_factor)).ceil() as u32;
        let h = (vp.height() * f64::from(view.scale_factor)).ceil() as u32;
        self.render_sized(view, w, h)
    }

    /// Render at an explicit pixel size; `scale_factor` still maps logical to
    /// physical pixels, the viewport origin maps to pixel (0, 0).
    pub fn render_sized(
        &mut self,
        view: &FrameView<'_>,
        width: u32,
        height: u32,
    ) -> Result<RgbaImage, RenderError> {
        let (Ok(w16), Ok(h16)) = (u16::try_from(width), u16::try_from(height)) else {
            return Err(RenderError::BadSize { width, height });
        };
        if w16 == 0 || h16 == 0 {
            return Err(RenderError::BadSize { width, height });
        }
        let settings = RenderSettings {
            level: Level::baseline(),
            num_threads: 0,
        };
        let mut ctx = match self.ctx.take() {
            Some(mut c) => {
                c.reset_and_resize(w16, h16);
                c
            }
            None => RenderContext::new_with(w16, h16, settings),
        };
        self.draw(&mut ctx, view);
        ctx.flush();
        let mut pixmap = Pixmap::new(w16, h16);
        ctx.render_with(
            &mut pixmap,
            &mut self.resources,
            RasterizerSettings {
                render_mode: RenderMode::OptimizeSpeed,
                ..Default::default()
            },
        );
        self.ctx = Some(ctx);
        Ok(RgbaImage {
            width,
            height,
            data: pixmap.take_rgba8(ImageAlphaType::Alpha),
        })
    }

    fn draw(&mut self, ctx: &mut RenderContext, view: &FrameView<'_>) {
        let snap = view.snapshot;
        let base = Affine::scale(f64::from(view.scale_factor))
            * Affine::translate((-snap.viewport.x0, -snap.viewport.y0));

        if !self.config.background.is_transparent() {
            ctx.set_transform(Affine::IDENTITY);
            ctx.set_paint(alpha_color(self.config.background));
            ctx.fill_rect(&Rect::new(
                0.0,
                0.0,
                f64::from(ctx.width()),
                f64::from(ctx.height()),
            ));
        }

        let mut pushed: Vec<ClipId> = Vec::new();
        let mut warned: HashSet<&'static str> = HashSet::new();
        for item in &snap.items {
            let Some(list) = &item.list else { continue };
            self.sync_clips(ctx, snap, &mut pushed, item.clip, base);
            let to_device = base * item.world;
            ctx.set_transform(to_device);
            for op in list.ops() {
                self.draw_op(ctx, op, item.opacity, to_device, &mut warned);
            }
        }
        for _ in pushed.drain(..) {
            ctx.pop_clip();
        }
    }

    /// Make the context's clip stack equal the chain ending at `target`, popping
    /// and pushing only the differing tail.
    fn sync_clips(
        &self,
        ctx: &mut RenderContext,
        snap: &FrameSnapshot,
        pushed: &mut Vec<ClipId>,
        target: Option<ClipId>,
        base: Affine,
    ) {
        let mut chain = Vec::new();
        let mut cur = target;
        while let Some(id) = cur {
            chain.push(id);
            cur = snap.clip(id).parent;
        }
        chain.reverse();
        let common = pushed
            .iter()
            .zip(&chain)
            .take_while(|(a, b)| a == b)
            .count();
        while pushed.len() > common {
            pushed.pop();
            ctx.pop_clip();
        }
        for id in &chain[common..] {
            let c = snap.clip(*id);
            ctx.set_transform(base * c.transform);
            if c.is_rounded() {
                ctx.push_clip_path(&rounded_rect(c.rect, c.radii).to_path(0.1));
            } else {
                ctx.push_clip_rect(&c.rect);
            }
            pushed.push(*id);
        }
    }

    /// Fill a run's outlines. Runs wholly outside the canvas are skipped, so
    /// text scrolled out of view costs nothing.
    fn draw_glyphs(
        &mut self,
        ctx: &mut RenderContext,
        run: &GlyphRun,
        origin: Vec2,
        decorations: &Decorations,
        color: Rgba,
        to_device: Affine,
    ) {
        if color.is_transparent() {
            return;
        }
        let canvas = Rect::new(0.0, 0.0, f64::from(ctx.width()), f64::from(ctx.height()));
        ctx.set_paint(alpha_color(color));
        ctx.set_fill_rule(Fill::NonZero);
        let snapped = Affine::translate((
            0.0,
            baseline_snap(to_device, f64::from(run.baseline) + origin.y),
        )) * to_device;
        let ink = snapped.transform_rect_bbox(run.bounds + origin);
        if ink.overlaps(canvas) {
            let size = f64::from(run.font_size);
            for g in &run.glyphs {
                let Some(outline) = self.outlines.get(run, g.id) else {
                    continue;
                };
                if outline.path.is_empty() {
                    continue;
                }
                let at = origin + Vec2::new(f64::from(g.x), f64::from(g.y));
                ctx.set_transform(snapped * Affine::translate(at) * outline.to_size(run, size));
                ctx.fill_path(&outline.path);
                if let Some(width) = outline.embolden_width(run) {
                    ctx.set_stroke(Stroke::new(width));
                    ctx.stroke_path(&outline.path);
                }
            }
        }
        for bar in decorations.bars() {
            match snap_bar(to_device, bar + origin) {
                Some(device) => {
                    ctx.set_transform(Affine::IDENTITY);
                    ctx.fill_rect(&device);
                }
                None => {
                    ctx.set_transform(to_device);
                    ctx.fill_rect(&(bar + origin));
                }
            }
        }
        ctx.set_transform(to_device);
    }

    fn draw_op(
        &mut self,
        ctx: &mut RenderContext,
        op: &DrawOp,
        opacity: f32,
        to_device: Affine,
        warned: &mut HashSet<&'static str>,
    ) {
        match op {
            DrawOp::Rect {
                rect,
                radii,
                fill,
                border,
                aa,
            } => {
                ctx.set_aliasing_threshold(if *aa { None } else { Some(128) });
                let outer = rounded_rect(*rect, *radii);
                if !fill.is_invisible()
                    && let Some(p) = to_paint(&fill.with_opacity(opacity))
                {
                    ctx.set_paint(p);
                    ctx.set_fill_rule(Fill::NonZero);
                    ctx.fill_path(&outer.to_path(0.1));
                }
                if let Some(b) = border
                    && b.width > 0.0
                    && !b.paint.is_invisible()
                    && let Some(p) = to_paint(&b.paint.with_opacity(opacity))
                {
                    let bw = f64::from(b.width);
                    let inner_rect = rect.inset(-bw);
                    let mut path = outer.to_path(0.1);
                    // A border thicker than half the rect fills it completely.
                    if inner_rect.width() > 0.0 && inner_rect.height() > 0.0 {
                        let inner_radii = radii.map(|r| (r - b.width).max(0.0));
                        path.extend(rounded_rect(inner_rect, inner_radii).to_path(0.1));
                        ctx.set_fill_rule(Fill::EvenOdd);
                    } else {
                        ctx.set_fill_rule(Fill::NonZero);
                    }
                    ctx.set_paint(p);
                    ctx.fill_path(&path);
                    ctx.set_fill_rule(Fill::NonZero);
                }
                ctx.set_aliasing_threshold(None);
            }
            DrawOp::Shadow {
                rect,
                radii,
                blur,
                spread,
                color,
            } => {
                let c = color.with_opacity(opacity);
                if c.is_transparent() {
                    return;
                }
                let s = f64::from(*spread);
                let grown = rect.inset(-s);
                let radius = radii.iter().copied().fold(0.0_f32, f32::max) + *spread;
                ctx.set_paint(alpha_color(c));
                // Gaussian sigma is half the CSS-style blur width.
                ctx.fill_blurred_rounded_rect(&grown, radius, (*blur / 2.0).max(0.0), false);
            }
            DrawOp::Path { path, style, paint } => {
                let Some(p) = to_paint(&paint.with_opacity(opacity)) else {
                    return;
                };
                ctx.set_paint(p);
                match style {
                    PathStyle::Fill => {
                        ctx.set_fill_rule(Fill::NonZero);
                        ctx.fill_path(path);
                    }
                    PathStyle::FillEvenOdd => {
                        ctx.set_fill_rule(Fill::EvenOdd);
                        ctx.fill_path(path);
                        ctx.set_fill_rule(Fill::NonZero);
                    }
                    PathStyle::Stroke { width } => {
                        ctx.set_stroke(Stroke::new(f64::from(*width)));
                        ctx.stroke_path(path);
                    }
                }
            }
            DrawOp::Text {
                rect, color, size, ..
            } => {
                if self.config.text == TextMode::Boxes {
                    let _ = size;
                    ctx.set_paint(alpha_color(color.with_opacity(opacity * 0.25)));
                    ctx.fill_rect(rect);
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
                        if warned.insert("glyph-gradient") {
                            tracing::debug!("R: gradient text uses its first stop colour");
                        }
                        g.stops.first().map_or(Rgba::TRANSPARENT, |s| s.color)
                    }
                    Paint::Image(_) => Rgba::TRANSPARENT,
                };
                self.draw_glyphs(
                    ctx,
                    run,
                    *origin,
                    decorations,
                    color.with_opacity(opacity),
                    to_device,
                );
            }
            DrawOp::Image { .. } | DrawOp::NinePatch { .. } => {
                if warned.insert("image") {
                    tracing::debug!("R: image ops are not rendered yet (no image cache in M0)");
                }
            }
            DrawOp::Embed { .. } => {
                if warned.insert("embed") {
                    tracing::debug!("R: Embed surfaces are not rendered");
                }
            }
        }
    }
}

impl RenderBackend for VelloCpuReference {
    type Sink<'a> = RgbaImage;

    fn sync(&mut self, _view: &FrameView<'_>) {}

    fn encode(&mut self, view: &FrameView<'_>, sink: &mut RgbaImage) -> EncodeStats {
        let mut stats = EncodeStats {
            items: view
                .snapshot
                .items
                .iter()
                .filter(|i| i.list.is_some())
                .count() as u32,
            ..Default::default()
        };
        match self.render_sized(view, sink.width, sink.height) {
            Ok(img) => {
                *sink = img;
                stats.shapes = view.snapshot.op_count() as u32;
            }
            Err(e) => {
                tracing::warn!("R: encode failed: {e}");
                stats.warnings += 1;
            }
        }
        stats
    }

    fn trim(&mut self, _pressure: MemoryPressure) {
        self.ctx = None;
        self.resources = Resources::new();
        self.outlines.clear();
    }

    fn snapshot_rgba(&mut self, view: &FrameView<'_>) -> Option<RgbaImage> {
        self.render(view).ok()
    }
}
