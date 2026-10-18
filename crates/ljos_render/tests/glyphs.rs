//! Glyph runs in both backends.
//!
//! R fills outlines; its pixels are checked directly and against a golden
//! image. B0 fills the same outlines into its atlas and emits textured quads;
//! the test below draws those quads in software and compares the result with
//! R, so the two backends are proven to put the same ink in the same place.

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use ljos_render::kurbo::{Affine, Rect, Size, Vec2};
use ljos_render::peniko::{Blob, FontData};
use ljos_render::{
    ClipSpec, Decorations, DisplayListBuilder, FrameView, GlyphRun, NodeId, PositionedGlyph,
    RasterConfig, RenderTree, Rgba, RgbaImage, TextMode, VelloCpuReference, font_tables,
    glyph_ink_bounds,
};
use skrifa::instance::{LocationRef, Size as FontSize};
use skrifa::{FontRef, MetadataProvider};

/// Liberation Sans Regular (SIL OFL 1.1), the font `ljos_text` bundles.
const FONT: &[u8] = include_bytes!("../../ljos_text/fonts/LiberationSans-Regular.ttf");

const WHITE_BG: RasterConfig = RasterConfig {
    background: Rgba::WHITE,
    text: TextMode::Skip,
};

/// One blob for every run, as a shaper hands them out: backends tell fonts
/// apart by the blob's id.
fn font() -> FontData {
    static DATA: OnceLock<FontData> = OnceLock::new();
    DATA.get_or_init(|| FontData::new(Blob::new(Arc::new(FONT)), 0))
        .clone()
}

/// Lay `text` out on one line by advance widths alone: enough for a renderer
/// test, and it keeps this crate free of the shaper.
fn run(text: &str, size: f32, baseline: f32) -> GlyphRun {
    let font_ref = FontRef::new(FONT).unwrap();
    let charmap = font_ref.charmap();
    let metrics = font_ref.glyph_metrics(FontSize::new(size), LocationRef::default());
    let mut x = 0.0;
    let mut glyphs = Vec::new();
    for c in text.chars() {
        let id = charmap.map(c).expect("the font has the character");
        glyphs.push(PositionedGlyph {
            id: id.to_u32(),
            x,
            y: baseline,
        });
        x += metrics.advance_width(id).unwrap_or(0.0);
    }
    let bounds = Rect::new(
        -f64::from(size),
        f64::from(baseline - size * 1.5),
        f64::from(x + size),
        f64::from(baseline + size),
    );
    GlyphRun::new(font(), size, glyphs, baseline, bounds)
}

struct Scene {
    tree: RenderTree,
    vp: Rect,
    frame: u64,
}

impl Scene {
    fn new(w: f64, h: f64) -> Self {
        let mut tree = RenderTree::new();
        let r = tree.root();
        tree.set_size(r, Size::new(w, h));
        Self {
            tree,
            vp: Rect::new(0.0, 0.0, w, h),
            frame: 0,
        }
    }

    fn node(&mut self, parent: Option<NodeId>, x: f64, y: f64, w: f64, h: f64) -> NodeId {
        let p = parent.unwrap_or(self.tree.root());
        let n = self.tree.add_child(p);
        self.tree.set_pos(n, x, y);
        self.tree.set_size(n, Size::new(w, h));
        n
    }

    fn paint(&mut self, id: NodeId, f: impl FnOnce(&mut DisplayListBuilder)) {
        let mut b = DisplayListBuilder::new();
        f(&mut b);
        self.tree.set_content(id, Some(b.finish_arc()));
    }

    /// One node at the origin, as large as the scene, drawing one run.
    fn with_run(w: f64, h: f64, run: GlyphRun, origin: Vec2, deco: Decorations) -> Self {
        let mut s = Self::new(w, h);
        let n = s.node(None, 0.0, 0.0, w, h);
        s.paint(n, |b| {
            b.glyphs(Arc::new(run), origin, Rgba::BLACK, deco);
        });
        s
    }

    fn render(&mut self, scale: f32) -> RgbaImage {
        self.frame += 1;
        let snap = self.tree.compose(self.vp, self.frame);
        VelloCpuReference::new(WHITE_BG)
            .render(&FrameView::new(&snap, scale))
            .unwrap()
    }
}

/// How dark a pixel is, 0 (white) to 255 (black).
fn ink(img: &RgbaImage, x: u32, y: u32) -> u8 {
    255 - img.pixel(x, y)[0]
}

/// Bounding box of everything darker than `threshold`: x0, y0, x1, y1, the
/// last two exclusive. `None` for a blank image.
fn ink_box(img: &RgbaImage, threshold: u8) -> Option<(u32, u32, u32, u32)> {
    let mut b: Option<(u32, u32, u32, u32)> = None;
    for y in 0..img.height {
        for x in 0..img.width {
            if ink(img, x, y) > threshold {
                b = Some(match b {
                    None => (x, y, x + 1, y + 1),
                    Some((x0, y0, x1, y1)) => (x0.min(x), y0.min(y), x1.max(x + 1), y1.max(y + 1)),
                });
            }
        }
    }
    b
}

fn total_ink(img: &RgbaImage) -> u64 {
    (0..img.height)
        .flat_map(|y| (0..img.width).map(move |x| (x, y)))
        .map(|(x, y)| u64::from(ink(img, x, y)))
        .sum()
}

fn golden_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(format!("{name}.png"))
}

fn assert_golden(name: &str, img: &RgbaImage) {
    let path = golden_path(name);
    if std::env::var_os("LJOS_BLESS").is_some() {
        std::fs::write(&path, img.to_png().unwrap()).unwrap();
    }
    let bytes = std::fs::read(&path).unwrap_or_else(|e| {
        panic!(
            "missing golden {}: {e} (run with LJOS_BLESS=1)",
            path.display()
        )
    });
    let want = RgbaImage::from_png(&bytes).unwrap();
    assert_eq!(
        (img.width, img.height),
        (want.width, want.height),
        "{name}: size"
    );
    assert!(
        img.data == want.data,
        "{name}: pixels differ from the golden"
    );
}

#[test]
fn r_draws_outlines_on_the_baseline() {
    // A capital H at 40 px: two stems and a bar, flat on the baseline.
    let mut s = Scene::with_run(
        60.0,
        60.0,
        run("H", 40.0, 0.0),
        Vec2::new(10.0, 45.0),
        Decorations::default(),
    );
    let img = s.render(1.0);
    let (x0, y0, x1, y1) = ink_box(&img, 128).expect("ink");
    // The baseline is row 45: ink ends just above it.
    assert_eq!(y1, 45);
    // Cap height of Liberation Sans is 0.729 em: 29 rows at 40 px.
    assert!((15..=17).contains(&y0), "{y0}");
    // The glyph starts a side bearing right of its origin and is 0.72 em wide.
    assert!((12..=14).contains(&x0), "{x0}");
    assert!((36..=39).contains(&x1), "{x1}");
    // A stem is solid, the counter above the bar is empty, the bar is solid.
    assert_eq!(ink(&img, x0 + 2, 20), 255);
    assert_eq!(ink(&img, (x0 + x1) / 2, 20), 0);
    assert_eq!(ink(&img, (x0 + x1) / 2, 30), 255);
    // Nothing leaks below the baseline or outside the glyph.
    assert_eq!(ink_box(&img, 0).unwrap().3, 45);
    assert_eq!(ink(&img, 5, 30), 0);
}

#[test]
fn r_snaps_the_baseline_to_a_device_pixel() {
    // Half a pixel down would blur the foot of the H over two rows.
    for y in [45.0, 45.3, 44.6] {
        let mut s = Scene::with_run(
            60.0,
            60.0,
            run("H", 40.0, 0.0),
            Vec2::new(10.0, y),
            Decorations::default(),
        );
        let img = s.render(1.0);
        assert_eq!(ink_box(&img, 0).unwrap().3, 45, "origin y {y}");
        assert_eq!(ink(&img, 15, 44), 255, "origin y {y}");
    }
}

#[test]
fn r_scales_with_the_device_pixel_ratio() {
    let make = || {
        Scene::with_run(
            60.0,
            60.0,
            run("H", 40.0, 0.0),
            Vec2::new(10.0, 45.0),
            Decorations::default(),
        )
    };
    let one = make().render(1.0);
    let two = make().render(2.0);
    assert_eq!((two.width, two.height), (120, 120));
    let a = ink_box(&one, 128).unwrap();
    let b = ink_box(&two, 128).unwrap();
    assert_eq!(b.3, 90);
    for (small, large) in [(a.0, b.0), (a.1, b.1), (a.2, b.2)] {
        assert!((i64::from(small) * 2 - i64::from(large)).abs() <= 2);
    }
    // Four times the pixels, four times the ink.
    let ratio = total_ink(&two) as f64 / total_ink(&one) as f64;
    assert!((ratio - 4.0).abs() < 0.1, "{ratio}");
}

#[test]
fn r_draws_a_line_of_text() {
    let mut s = Scene::with_run(
        150.0,
        30.0,
        run("Hello, Ljós!", 20.0, 0.0),
        Vec2::new(6.0, 21.0),
        Decorations::default(),
    );
    let img = s.render(1.0);
    assert_golden("glyph_run", &img);
    let (x0, _, x1, y1) = ink_box(&img, 64).unwrap();
    assert!(x0 >= 6 && x1 <= 120, "{x0} {x1}");
    // The comma and the j reach below the baseline, nothing else does.
    assert!(y1 > 21 && y1 <= 27, "{y1}");
}

#[test]
fn r_draws_decorations_as_whole_pixel_bars() {
    let glyphs = run("Hi", 20.0, 0.0);
    let deco = Decorations {
        underline: Some(Rect::new(0.0, 1.4, 20.0, 2.4)),
        strike_through: Some(Rect::new(0.0, -5.2, 20.0, -4.6)),
    };
    let mut s = Scene::with_run(40.0, 40.0, glyphs, Vec2::new(10.0, 25.0), deco);
    let img = s.render(1.0);
    // Underline: top 26.4 rounds to row 26, one pixel thick, sharp.
    assert_eq!(ink(&img, 29, 26), 255);
    assert_eq!(ink(&img, 29, 27), 0);
    assert_eq!(ink(&img, 29, 25), 0);
    // Strike-through: 0.6 thick, still a full row (19.8 rounds to 20).
    assert_eq!(ink(&img, 29, 20), 255);
    assert_eq!(ink(&img, 29, 19), 0);
    assert_eq!(ink(&img, 29, 21), 0);
    // Bars span the run, not the node.
    assert_eq!(ink(&img, 9, 26), 0);
    assert_eq!(ink(&img, 30, 26), 0);
}

#[test]
fn r_draws_synthetic_bold_and_italic() {
    let render = |f: fn(&mut GlyphRun)| {
        let mut r = run("l", 40.0, 0.0);
        f(&mut r);
        Scene::with_run(40.0, 60.0, r, Vec2::new(15.0, 45.0), Decorations::default()).render(1.0)
    };
    let plain = render(|_| {});
    let bold = render(|r| r.embolden = true);
    let slanted = render(|r| r.skew = 0.25);

    assert!(total_ink(&bold) as f64 > total_ink(&plain) as f64 * 1.2);
    let (p, b) = (ink_box(&plain, 128).unwrap(), ink_box(&bold, 128).unwrap());
    assert!(b.2 - b.0 > p.2 - p.0, "bold is wider: {p:?} {b:?}");

    // The top of a slanted stem moves right; its foot stays on the baseline.
    let left_edge = |img: &RgbaImage, y: u32| (0..img.width).find(|x| ink(img, *x, y) > 128);
    let top = left_edge(&slanted, 20).unwrap() as i64 - left_edge(&plain, 20).unwrap() as i64;
    let foot = left_edge(&slanted, 43).unwrap() as i64 - left_edge(&plain, 43).unwrap() as i64;
    assert!((5..=7).contains(&top), "25 rows up at 0.25: {top}");
    assert!(foot <= 1, "{foot}");
    assert_eq!(ink_box(&slanted, 0).unwrap().3, 45);
}

#[test]
fn r_clips_and_skips_what_is_off_the_canvas() {
    // A run far outside draws nothing and costs nothing.
    let mut s = Scene::with_run(
        40.0,
        40.0,
        run("H", 20.0, 0.0),
        Vec2::new(500.0, 500.0),
        Decorations::default(),
    );
    assert_eq!(ink_box(&s.render(1.0), 0), None);

    // A clip cuts a glyph in half.
    let mut s = Scene::new(60.0, 60.0);
    let clip = s.node(None, 0.0, 0.0, 24.0, 60.0);
    s.tree
        .set_clip(clip, Some(ClipSpec::bounds(Size::new(24.0, 60.0))));
    let text = s.node(Some(clip), 0.0, 0.0, 60.0, 60.0);
    s.paint(text, |b| {
        b.glyphs(
            Arc::new(run("H", 40.0, 0.0)),
            Vec2::new(10.0, 45.0),
            Rgba::BLACK,
            Decorations::default(),
        );
    });
    let img = s.render(1.0);
    let (x0, _, x1, _) = ink_box(&img, 0).unwrap();
    assert!(x0 >= 12 && x1 == 24, "{x0} {x1}");
}

#[test]
fn r_draws_rotated_text() {
    let mut s = Scene::new(60.0, 60.0);
    let n = s.node(None, 0.0, 0.0, 60.0, 60.0);
    s.tree.set_local(
        n,
        Affine::rotate_about(std::f64::consts::FRAC_PI_2, (30.0, 30.0)),
    );
    s.paint(n, |b| {
        b.glyphs(
            Arc::new(run("l", 40.0, 0.0)),
            Vec2::new(26.0, 45.0),
            Rgba::BLACK,
            Decorations::default(),
        );
    });
    let img = s.render(1.0);
    let (x0, y0, x1, y1) = ink_box(&img, 128).unwrap();
    // An upright stem became a lying one.
    assert!(x1 - x0 > 20 && y1 - y0 < 8, "{:?}", (x0, y0, x1, y1));
}

#[cfg(feature = "b0")]
mod b0 {
    use egui::epaint::{ClippedShape, Shape};
    use egui::{Context, Pos2, TextureId};
    use ljos_render::b0::{EguiSink, EpaintBackend};
    use ljos_render::{EncodeStats, MemoryPressure, RenderBackend};

    use super::*;

    fn pass(ctx: &Context, f: impl FnMut(&mut egui::Ui)) {
        ctx.run_ui(Default::default(), f).textures_delta.clear();
    }

    fn encode(
        s: &mut Scene,
        be: &mut EpaintBackend,
        ctx: Option<&Context>,
        scale: f32,
    ) -> (Vec<ClippedShape>, EncodeStats) {
        s.frame += 1;
        let snap = s.tree.compose(s.vp, s.frame);
        let view = FrameView::new(&snap, scale);
        be.sync(&view);
        let mut sink = EguiSink::new(ctx, Pos2::ZERO);
        let stats = be.encode(&view, &mut sink);
        (sink.shapes, stats)
    }

    /// One frame inside an egui pass, the way the view runs it.
    fn frame(
        s: &mut Scene,
        be: &mut EpaintBackend,
        ctx: &Context,
        scale: f32,
    ) -> (Vec<ClippedShape>, EncodeStats) {
        let mut out = None;
        pass(ctx, |_ui| out = Some(encode(s, be, Some(ctx), scale)));
        out.unwrap()
    }

    /// Draw B0's glyph quads the way a GPU would for upright text: every quad
    /// copies a rectangle of the atlas, texel on pixel, inside its clip.
    fn blit(shapes: &[ClippedShape], be: &EpaintBackend, w: u32, h: u32, scale: f32) -> RgbaImage {
        let atlas = be.glyph_atlas();
        let side = atlas.side as f32;
        let mut cover = vec![0u8; (w * h) as usize];
        for clipped in shapes {
            let Shape::Mesh(mesh) = &clipped.shape else {
                panic!("glyphs are meshes")
            };
            assert_ne!(mesh.texture_id, TextureId::default(), "atlas texture");
            let clip = clipped.clip_rect;
            for quad in mesh.vertices.as_chunks::<4>().0 {
                let (tl, br) = (quad[0], quad[2]);
                let px = |v: f32| (v * scale).round() as i64;
                let (x0, y0, x1, y1) = (px(tl.pos.x), px(tl.pos.y), px(br.pos.x), px(br.pos.y));
                let solid = tl.uv == br.uv;
                let (u0, v0) = (
                    (tl.uv.x * side).round() as i64,
                    (tl.uv.y * side).round() as i64,
                );
                if !solid {
                    // Texel on pixel: the quad is as large as its atlas slot.
                    assert_eq!(((br.uv.x * side).round() as i64 - u0, x1 - x0).0, x1 - x0);
                    assert_eq!((br.uv.y * side).round() as i64 - v0, y1 - y0);
                }
                for y in y0.max(0)..y1.min(i64::from(h)) {
                    for x in x0.max(0)..x1.min(i64::from(w)) {
                        let (cx, cy) = ((x as f32 + 0.5) / scale, (y as f32 + 0.5) / scale);
                        if !clip.contains(Pos2::new(cx, cy)) {
                            continue;
                        }
                        let a = if solid {
                            atlas.alpha
                                [(tl.uv.y * side) as usize * atlas.side + (tl.uv.x * side) as usize]
                        } else {
                            atlas.alpha
                                [(v0 + y - y0) as usize * atlas.side + (u0 + x - x0) as usize]
                        };
                        let a = (u32::from(a) * u32::from(tl.color.a()) / 255) as u8;
                        let at = &mut cover[(y as u32 * w + x as u32) as usize];
                        *at = (*at).max(a);
                    }
                }
            }
        }
        let data = cover
            .iter()
            .flat_map(|c| [255 - c, 255 - c, 255 - c, 255])
            .collect();
        RgbaImage {
            width: w,
            height: h,
            data,
        }
    }

    /// Largest and mean difference between two images.
    fn difference(a: &RgbaImage, b: &RgbaImage) -> (u8, f64) {
        assert_eq!((a.width, a.height), (b.width, b.height));
        let (mut max, mut sum) = (0u8, 0u64);
        for (p, q) in a
            .data
            .as_chunks::<4>()
            .0
            .iter()
            .zip(b.data.as_chunks::<4>().0)
        {
            let d = p[0].abs_diff(q[0]);
            max = max.max(d);
            sum += u64::from(d);
        }
        (max, sum as f64 / f64::from(a.width * a.height))
    }

    fn line() -> Scene {
        let deco = Decorations {
            underline: Some(Rect::new(0.0, 1.4, 100.0, 2.4)),
            strike_through: None,
        };
        Scene::with_run(
            150.0,
            30.0,
            run("Hello, Ljós!", 20.0, 0.0),
            Vec2::new(6.3, 21.0),
            deco,
        )
    }

    #[test]
    fn b0_puts_the_same_ink_where_r_does() {
        for scale in [1.0, 2.0] {
            let mut s = line();
            let reference = s.render(scale);
            let ctx = Context::default();
            let mut be = EpaintBackend::new();
            let (shapes, stats) = frame(&mut s, &mut be, &ctx, scale);
            assert_eq!(stats.warnings, 0);
            assert_eq!(shapes.len(), 1, "one mesh for the run and its bar");
            let img = blit(&shapes, &be, reference.width, reference.height, scale);
            let (max, mean) = difference(&img, &reference);
            // The atlas fills a glyph at one of four sub-pixel positions, so an
            // edge may sit an eighth of a pixel off: 32 of 255 on a sharp edge.
            assert!(max <= 40, "scale {scale}: max {max}");
            assert!(mean < 2.0, "scale {scale}: mean {mean}");
            // And it is text, not a blank image.
            assert!(total_ink(&img) > 100_000 * (scale * scale) as u64 / 2);
            let ratio = total_ink(&img) as f64 / total_ink(&reference) as f64;
            assert!((ratio - 1.0).abs() < 0.02, "scale {scale}: {ratio}");
        }
    }

    #[test]
    fn b0_fills_each_glyph_once() {
        let mut s = line();
        let ctx = Context::default();
        let mut be = EpaintBackend::new();
        let (first, _) = frame(&mut s, &mut be, &ctx, 1.0);
        let atlas = be.glyph_atlas();
        // "Hello, Ljós!" has two l: eleven different glyphs at most (two glyphs
        // at different sub-pixel offsets count twice), never twelve fills of
        // the same one.
        assert!(atlas.glyphs >= 10 && atlas.glyphs <= 12, "{}", atlas.glyphs);
        let filled = atlas.rasterised;
        assert!(filled >= 10);

        // A second frame fills nothing and reuses the mesh.
        let (second, stats) = frame(&mut s, &mut be, &ctx, 1.0);
        assert_eq!(be.glyph_atlas().rasterised, filled);
        assert_eq!(stats.caches_reused, 1);
        let (Shape::Mesh(a), Shape::Mesh(b)) = (&first[0].shape, &second[0].shape) else {
            panic!("meshes")
        };
        assert!(Arc::ptr_eq(a, b));

        // The same text again in another node costs no fill either.
        let n = s.node(None, 0.0, 0.0, 150.0, 30.0);
        s.paint(n, |b| {
            b.glyphs(
                Arc::new(run("Hello, Ljós!", 20.0, 0.0)),
                Vec2::new(6.3, 21.0),
                Rgba::BLACK,
                Decorations::default(),
            );
        });
        frame(&mut s, &mut be, &ctx, 1.0);
        assert_eq!(be.glyph_atlas().rasterised, filled);
    }

    #[test]
    fn b0_hands_over_only_the_glyphs_it_fills() {
        let mut s = line();
        let ctx = Context::default();
        let mut be = EpaintBackend::new();
        let (first, _) = frame(&mut s, &mut be, &ctx, 1.0);
        let Shape::Mesh(shown) = &first[0].shape else {
            panic!("glyphs are meshes")
        };
        let side = be.glyph_atlas().side as f32;
        // The texels of the glyphs already shown.
        let boxes: Vec<_> = shown
            .vertices
            .as_chunks::<4>()
            .0
            .iter()
            .map(|q| (q[0].uv.to_vec2() * side, q[2].uv.to_vec2() * side))
            .collect();

        let n = s.node(None, 0.0, 30.0, 150.0, 30.0);
        s.paint(n, |b| {
            b.glyphs(
                Arc::new(run("xyz", 20.0, 0.0)),
                Vec2::new(6.0, 21.0),
                Rgba::BLACK,
                Decorations::default(),
            );
        });
        let mut out = ctx.run_ui(Default::default(), |_| {
            encode(&mut s, &mut be, Some(&ctx), 1.0);
        });
        let delta = std::mem::take(&mut out.textures_delta.set).remove(&shown.texture_id);
        let delta = delta.expect("the new glyphs are handed over");
        for d in &delta {
            let [x, y] = d.pos.expect("a part of the atlas, not all of it");
            let [w, h] = d.image.size();
            assert!(w < side as usize, "{w} texels wide: whole rows");
            let (x0, y0, x1, y1) = (x as f32, y as f32, (x + w) as f32, (y + h) as f32);
            for (min, max) in &boxes {
                let apart = max.x <= x0 || x1 <= min.x || max.y <= y0 || y1 <= min.y;
                assert!(apart, "{:?} handed over again", (min, max));
            }
        }
    }

    #[test]
    fn b0_does_not_fill_clipped_glyphs() {
        // A long line in a narrow clip: only the glyphs near the clip are filled.
        let text = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
        let mut s = Scene::new(400.0, 40.0);
        let clip = s.node(None, 0.0, 0.0, 30.0, 40.0);
        s.tree
            .set_clip(clip, Some(ClipSpec::bounds(Size::new(30.0, 40.0))));
        let node = s.node(Some(clip), 0.0, 0.0, 400.0, 40.0);
        s.paint(node, |b| {
            b.glyphs(
                Arc::new(run(text, 10.0, 0.0)),
                Vec2::new(2.0, 20.0),
                Rgba::BLACK,
                Decorations::default(),
            );
        });
        let ctx = Context::default();
        let mut be = EpaintBackend::new();
        let (shapes, _) = frame(&mut s, &mut be, &ctx, 1.0);
        assert_eq!(shapes.len(), 1);
        let filled = be.glyph_atlas().rasterised;
        assert!((4..=12).contains(&filled), "{filled} of 52 glyphs");

        // A run wholly outside its clip: nothing filled, nothing emitted.
        let mut s = Scene::new(400.0, 40.0);
        let clip = s.node(None, 0.0, 0.0, 30.0, 40.0);
        s.tree
            .set_clip(clip, Some(ClipSpec::bounds(Size::new(30.0, 40.0))));
        let node = s.node(Some(clip), 200.0, 0.0, 200.0, 40.0);
        s.paint(node, |b| {
            b.glyphs(
                Arc::new(run(text, 10.0, 0.0)),
                Vec2::new(2.0, 20.0),
                Rgba::BLACK,
                Decorations::default(),
            );
        });
        let mut be = EpaintBackend::new();
        let (shapes, _) = frame(&mut s, &mut be, &ctx, 1.0);
        assert_eq!(be.glyph_atlas().rasterised, 0);
        assert!(shapes.is_empty(), "{}", shapes.len());
    }

    #[test]
    fn b0_needs_a_context_for_text() {
        let mut s = line();
        let mut be = EpaintBackend::new();
        let (shapes, stats) = encode(&mut s, &mut be, None, 1.0);
        assert!(shapes.is_empty());
        assert_eq!(stats.warnings, 1);
    }

    #[test]
    fn b0_refills_after_a_scale_change_or_a_trim() {
        let mut s = line();
        let ctx = Context::default();
        let mut be = EpaintBackend::new();
        frame(&mut s, &mut be, &ctx, 1.0);
        let at_one = be.glyph_atlas().rasterised;

        // Other pixels per point: other glyph sizes, the old ones are gone.
        let (shapes, _) = frame(&mut s, &mut be, &ctx, 2.0);
        let at_two = be.glyph_atlas().rasterised;
        assert!(at_two > at_one);
        assert!(be.glyph_atlas().glyphs <= 12);
        let reference = s.render(2.0);
        let img = blit(&shapes, &be, reference.width, reference.height, 2.0);
        assert!(difference(&img, &reference).0 <= 40);

        be.trim(MemoryPressure::Critical);
        assert_eq!(be.glyph_atlas().glyphs, 0);
        let (shapes, _) = frame(&mut s, &mut be, &ctx, 2.0);
        assert!(be.glyph_atlas().rasterised > at_two);
        let img = blit(&shapes, &be, reference.width, reference.height, 2.0);
        assert!(difference(&img, &reference).0 <= 40);
    }

    #[test]
    fn b0_applies_opacity_and_colour() {
        let mut s = Scene::new(60.0, 60.0);
        let n = s.node(None, 0.0, 0.0, 60.0, 60.0);
        s.tree.set_opacity(n, 0.5);
        s.paint(n, |b| {
            b.glyphs(
                Arc::new(run("H", 40.0, 0.0)),
                Vec2::new(10.0, 45.0),
                Rgba::rgb(255, 0, 0),
                Decorations::default(),
            );
        });
        let ctx = Context::default();
        let mut be = EpaintBackend::new();
        let (shapes, _) = frame(&mut s, &mut be, &ctx, 1.0);
        let Shape::Mesh(mesh) = &shapes[0].shape else {
            panic!("mesh")
        };
        // Premultiplied half-transparent red on every vertex.
        for v in &mesh.vertices {
            assert!((126..=129).contains(&v.color.a()), "{:?}", v.color);
            assert!((126..=129).contains(&v.color.r()), "{:?}", v.color);
            assert_eq!((v.color.g(), v.color.b()), (0, 0));
        }
    }
}

#[test]
fn font_tables_are_read_in_font_units() {
    let t = font_tables(&font()).expect("the bundled font has the tables");
    assert_eq!(t.units_per_em, 2048);
    assert_eq!((t.ascender, t.descender, t.line_gap), (1854, -434, 67));
    assert_eq!(t.advance_width_max, 2740);
    assert!(t.min_left_side_bearing < 0 && t.min_right_side_bearing < 0);
    let os2 = t.os2.expect("the bundled font has an OS/2 table");
    assert_eq!((os2.win_ascent, os2.win_descent), (1854, 434));
    assert_eq!(
        (os2.typo_ascender, os2.typo_descender, os2.typo_line_gap),
        (1491, -431, 307)
    );
    assert!(!os2.use_typo_metrics);
    assert_eq!(os2.x_avg_char_width, 1187);

    let not_a_font = FontData::new(Blob::new(Arc::new([0u8; 16])), 0);
    assert_eq!(font_tables(&not_a_font), None);
}

#[test]
fn ink_bounds_follow_the_glyphs() {
    let r = run("x g", 20.0, 30.0);
    let boxes = glyph_ink_bounds(&r);
    assert_eq!(boxes.len(), 3);
    let (x, g) = (boxes[0].unwrap(), boxes[2].unwrap());
    // A space has no ink.
    assert_eq!(boxes[1], None);
    // x stands on the baseline and is as tall as the font's x-height.
    assert!((x.y1 - 30.0).abs() < 0.01, "{x:?}");
    assert!((x.height() - 1082.0 * 20.0 / 2048.0).abs() < 0.01, "{x:?}");
    assert!(x.x0 >= 0.0 && x.x1 <= f64::from(r.glyphs[1].x));
    // g hangs below it, and sits where its glyph is.
    assert!(g.y1 > 33.0, "{g:?}");
    assert!(g.x0 >= f64::from(r.glyphs[2].x), "{g:?}");

    // A synthetic slant leans ink above the baseline to the right.
    let mut slanted = run("l", 20.0, 30.0);
    slanted.skew = 0.25;
    let upright = glyph_ink_bounds(&run("l", 20.0, 30.0))[0].unwrap();
    let leaning = glyph_ink_bounds(&slanted)[0].unwrap();
    assert!(leaning.x1 > upright.x1 + 2.0, "{leaning:?} {upright:?}");
    assert_eq!((leaning.y0, leaning.y1), (upright.y0, upright.y1));
}

#[test]
fn glyph_advances_are_the_fonts_own() {
    let r = run("Hx ", 20.0, 30.0);
    let ids: Vec<u32> = r.glyphs.iter().map(|g| g.id).collect();
    let (units_per_em, advances) =
        ljos_render::glyph_advances(&font(), &[], &ids).expect("the bundled font");
    assert_eq!(units_per_em, 2048);
    // Liberation Sans: H, x and the space.
    assert_eq!(advances, [1479.0, 1024.0, 569.0]);
    // A glyph the font does not have takes no room.
    let (_, missing) = ljos_render::glyph_advances(&font(), &[], &[0x00FF_FFFF]).unwrap();
    assert_eq!(missing, [0.0]);

    let not_a_font = FontData::new(Blob::new(Arc::new([0u8; 16])), 0);
    assert_eq!(ljos_render::glyph_advances(&not_a_font, &[], &ids), None);
}
