//! `color-text-contrast`: text stands out from what is painted behind it.
//!
//! Read from the last frame, so the colours are the ones shown, whoever
//! chose them: the style, a palette or the application. For each run of
//! text the fills painted before it are laid over the window's colour at
//! a few points along the run's middle, and the text's colour over that;
//! the worst of the points is the run's contrast. The paint is walked
//! rather than pixels sampled: it needs no renderer, is the same on
//! every machine, and keeps the glyphs themselves out of what is behind
//! them. A picture under the text is a colour nobody knows, so a point
//! on one is left out; and when a module stands in as empty Items (a 3D
//! view), so is a point where nothing is painted but the window: what
//! the stand-in would show may be there.
//!
//! A text with a `style` (an outline, raised or sunken) carries its
//! `styleColor` about its glyphs: that copy is no text of its own, and
//! the text is held against it, as WCAG counts an outline or a shadow
//! that sets text off as what lies behind it.
//!
//! WCAG 2's ratio is the gate; APCA's Lc is given as advice.

use std::collections::HashMap;

use ljos_design::color::{Srgb, Srgba, apca_lc, is_large_text, opacity, over, wcag_contrast};
use ljos_design::limits::{LARGE_TEXT_CONTRAST, TEXT_CONTRAST};
use ljos_engine::ObjectId;
use ljos_quick::{Item, Text};
use ljos_render::kurbo::{Affine, Point, Rect};
use ljos_render::{ClipId, DrawOp, FrameSnapshot, Paint};

use crate::cx::{Cx, item_words};
use crate::pixels::backdrop;
use crate::surfaces::{qcolor, straight};

/// Something painted that text may lie on.
struct Fill {
    world: Affine,
    rect: Rect,
    /// `None`: a picture, whose colour here is not known.
    paint: Option<Paint>,
    opacity: f32,
    clip: Option<ClipId>,
}

fn contains(r: Rect, p: Point) -> bool {
    p.x >= r.x0 && p.x < r.x1 && p.y >= r.y0 && p.y < r.y1
}

/// The colour at `p` (world) behind what is painted next. `None` when a
/// picture is the last thing there, or with `bare` false when nothing is.
fn behind(
    snapshot: &FrameSnapshot,
    fills: &[Fill],
    backdrop: Srgb,
    bare: bool,
    p: Point,
) -> Option<Srgb> {
    let mut colour = Some(backdrop);
    let mut painted = bare;
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
        };
        let top = opacity(straight(top), f.opacity);
        painted = true;
        colour = match colour {
            Some(under) => Some(over(top, under)),
            // Opaque paint over a picture is a known colour again.
            None if top.a >= 0.999 => Some(over(top, Srgb::BLACK)),
            None => None,
        };
    }
    colour.filter(|_| painted)
}

/// The worst run of an item.
struct Worst {
    ratio: f32,
    text: Srgb,
    behind: Srgb,
    least: f32,
}

/// The colour a styled text carries about its glyphs, or `None` for a
/// text of `Text.Normal` style, or what is no text.
fn halo(cx: &Cx, item: ObjectId) -> Option<Srgba> {
    let (style, c) = cx
        .engine
        .with::<Text, _>(item, |t| (t.style, t.style_color))
        .ok()?;
    (style != 0).then(|| qcolor(c))
}

fn same(a: Srgba, b: Srgba) -> bool {
    (a.r - b.r).abs() < 0.01
        && (a.g - b.g).abs() < 0.01
        && (a.b - b.b).abs() < 0.01
        && (a.a - b.a).abs() < 0.01
}

/// The weight of an item's font, 400 when it has none.
fn weight(cx: &Cx, item: ObjectId) -> u16 {
    cx.engine
        .with::<Text, _>(item, |t| t.font.weight)
        .ok()
        .and_then(|w| u16::try_from(w).ok())
        .unwrap_or(400)
}

pub(crate) fn check(cx: &mut Cx) {
    let Some(snapshot) = cx.scene.snapshot().cloned() else {
        return;
    };
    let backdrop = backdrop(cx.engine, cx.options);
    let bare = cx.engine.stood_in_modules().is_empty();
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
                } => (*c, run.font_size),
                DrawOp::Text { color, size, .. } => (*color, *size),
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
            let least = if is_large_text(size, weight(cx, item)) {
                LARGE_TEXT_CONTRAST
            } else {
                TEXT_CONTRAST
            };
            let halo = halo(cx, item);
            if halo.is_some_and(|h| same(h, straight(colour))) {
                continue;
            }
            let text = opacity(straight(colour), paint.opacity);
            let y = b.center().y;
            for x in [0.25, 0.5, 0.75].map(|f| b.x0 + b.width() * f) {
                let p = paint.world * Point::new(x, y);
                if !contains(snapshot.viewport, p) || !snapshot.clip_chain_contains(paint.clip, p) {
                    continue;
                }
                let under = behind(&snapshot, &fills, backdrop, bare, p);
                let under = match (halo, under) {
                    (None, Some(u)) => u,
                    (Some(h), u) => {
                        let h = opacity(h, paint.opacity);
                        match u {
                            Some(u) => over(h, u),
                            None if h.a >= 0.999 => over(h, Srgb::BLACK),
                            None => continue,
                        }
                    }
                    (None, None) => continue,
                };
                let shown = over(text, under);
                let r = wcag_contrast(shown, under);
                // Short of the least by more than what rounding to two
                // places hides.
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
        let least = trim_ratio(w.least);
        let lc = apca_lc(w.text, w.behind).abs();
        let message = format!(
            "{what} is {} on {}, a contrast of {:.2}:1 where {least}:1 is the least \
             (APCA Lc {lc:.0})",
            w.text.to_hex(),
            w.behind.to_hex(),
            w.ratio,
        );
        let f = cx.report("color-text-contrast", Some(item), message);
        f.measured = Some(format!("{:.2}:1 < {least}:1", w.ratio));
    }
}

fn trim_ratio(n: f32) -> String {
    let s = format!("{n:.1}");
    s.strip_suffix(".0").map(str::to_string).unwrap_or(s)
}
