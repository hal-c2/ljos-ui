//! Contrast of text with what is painted behind it.
//!
//! Read from the last frame, so the colours are the ones shown, whoever
//! chose them: the style, a palette or the application. For each run of
//! text the fills painted before it under its middle are laid over the
//! window's colour, and the text's colour over that.

use std::collections::HashMap;

use ljos_engine::ObjectId;
use ljos_quick::Item;
use ljos_render::kurbo::{Affine, Point, Rect};
use ljos_render::{ClipId, DrawOp, FrameSnapshot, Paint, Rgba};

use super::{Code, Cx, item_words};

/// Something painted that text may lie on.
struct Fill {
    world: Affine,
    rect: Rect,
    /// `None`: a picture, whose colour here is not known.
    paint: Option<Paint>,
    opacity: f32,
    clip: Option<ClipId>,
}

/// Straight sRGB, each channel `0.0..=1.0`.
type Rgb = [f32; 3];

fn over(top: Rgba, under: Rgb) -> Rgb {
    // Premultiplied "over" in gamma space, as the renderer blends.
    let keep = 1.0 - top.a;
    [
        top.r + under[0] * keep,
        top.g + under[1] * keep,
        top.b + under[2] * keep,
    ]
}

/// WCAG 2 relative luminance.
fn luminance(c: Rgb) -> f64 {
    let lin = |v: f32| {
        let v = f64::from(v.clamp(0.0, 1.0));
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(c[0]) + 0.7152 * lin(c[1]) + 0.0722 * lin(c[2])
}

/// WCAG 2 contrast ratio, 1 to 21.
pub(super) fn ratio(a: Rgb, b: Rgb) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

fn hex(c: Rgb) -> String {
    let b = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", b(c[0]), b(c[1]), b(c[2]))
}

fn contains(r: Rect, p: Point) -> bool {
    p.x >= r.x0 && p.x < r.x1 && p.y >= r.y0 && p.y < r.y1
}

/// The colour at `p` (world) behind what is painted next. `None` when a
/// picture is the last thing there.
fn behind(snapshot: &FrameSnapshot, fills: &[Fill], backdrop: Rgb, p: Point) -> Option<Rgb> {
    let mut colour = Some(backdrop);
    for f in fills {
        let local = f.world.inverse() * p;
        if !contains(f.rect, local) || !snapshot.clip_chain_contains(f.clip, p) {
            continue;
        }
        let top = match &f.paint {
            Some(Paint::Solid(c)) => *c,
            Some(Paint::LinearGradient(g)) => g.color_at(g.param(local)),
            Some(Paint::Image(_)) | None => {
                colour = None;
                continue;
            }
        }
        .with_opacity(f.opacity);
        colour = match colour {
            Some(under) => Some(over(top, under)),
            // Opaque paint over a picture is a known colour again.
            None if top.a >= 0.999 => Some(over(top, [0.0; 3])),
            None => None,
        };
    }
    colour
}

/// The worst run of an item.
struct Worst {
    ratio: f64,
    text: Rgb,
    behind: Rgb,
    least: f64,
}

/// `a11y-contrast`.
pub(super) fn text(cx: &mut Cx) {
    let Some(snapshot) = cx.scene.snapshot().cloned() else {
        return;
    };
    let backdrop: Rgb = match cx.options.backdrop {
        Some([r, g, b]) => [r, g, b].map(|v| f32::from(v) / 255.0),
        None => match ljos_quick::system_prefs(cx.engine).color_scheme {
            ljos_a11y::ColorScheme::Dark => [0.0; 3],
            _ => [1.0; 3],
        },
    };
    let mut fills: Vec<Fill> = Vec::new();
    let mut worst: HashMap<ObjectId, Worst> = HashMap::new();
    let mut order: Vec<ObjectId> = Vec::new();
    for paint in &snapshot.items {
        let Some(list) = &paint.list else { continue };
        let fill = |rect: Rect, paint_of: Option<Paint>| Fill {
            world: paint.world,
            rect,
            paint: paint_of,
            opacity: paint.opacity,
            clip: paint.clip,
        };
        for op in list.ops() {
            let (colour, size) = match op {
                DrawOp::Rect { rect, fill: f, .. } => {
                    if !f.is_invisible() {
                        fills.push(fill(*rect, Some(f.clone())));
                    }
                    continue;
                }
                DrawOp::Image { dst, .. } | DrawOp::NinePatch { dst, .. } => {
                    fills.push(fill(*dst, None));
                    continue;
                }
                DrawOp::Glyphs {
                    run,
                    paint: Paint::Solid(c),
                    ..
                } => (*c, f64::from(run.font_size)),
                DrawOp::Text { color, size, .. } => (*color, f64::from(*size)),
                _ => continue,
            };
            let Some(item) = cx.scene.item_of(paint.node) else {
                continue;
            };
            // What is switched off is not held to the contrast (WCAG
            // 1.4.3, "inactive").
            if !cx.engine.is_alive(item) || !cx.engine.read(item, Item::ENABLED.index).to_boolean()
            {
                continue;
            }
            let b = op.bounds();
            if b.width() <= 0.0 || b.height() <= 0.0 {
                continue;
            }
            let p = paint.world * b.center();
            if !contains(snapshot.viewport, p) || !snapshot.clip_chain_contains(paint.clip, p) {
                continue;
            }
            let Some(under) = behind(&snapshot, &fills, backdrop, p) else {
                continue;
            };
            let shown = over(colour.with_opacity(paint.opacity), under);
            let least = if size >= cx.options.large_text {
                cx.options.contrast_large
            } else {
                cx.options.contrast
            };
            let r = ratio(shown, under);
            // Short of the least by more than what rounding to two places
            // hides.
            if r + 0.005 >= least {
                continue;
            }
            let found = Worst {
                ratio: r,
                text: shown,
                behind: under,
                least,
            };
            match worst.get_mut(&item) {
                Some(w) if w.ratio <= r => {}
                Some(w) => *w = found,
                None => {
                    order.push(item);
                    worst.insert(item, found);
                }
            }
        }
    }
    for item in order {
        let w = &worst[&item];
        let said = cx.engine.read_by_name(item, "text").to_js_string();
        let said: String = said.as_str().chars().take(40).collect();
        let what = if said.is_empty() {
            item_words(cx.engine, item)
        } else {
            format!("the text {said:?}")
        };
        let message = format!(
            "{what} is {} on {}, a contrast of {:.2}:1 where {}:1 is the least",
            hex(w.text),
            hex(w.behind),
            w.ratio,
            w.least,
        );
        cx.report(Code::Contrast, Some(item), message);
    }
}
