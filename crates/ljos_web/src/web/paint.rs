//! A display list on a 2D canvas. The bindings are written out because
//! web-sys's would bring in a generated crate for these few methods.

use std::cell::RefCell;
use std::collections::HashMap;

use js_sys::{Array, Reflect, Uint8ClampedArray};
use ljos_render::kurbo::{BezPath, PathEl, Rect};
use ljos_render::{
    Brush, DrawOp, FillRule, GradientGeometry, GradientStop, ImageHandle, Paint, PathStyle, Rgba,
    ShapeStyle, TextAlign,
};
use wasm_bindgen::JsValue;
use wasm_bindgen::prelude::wasm_bindgen;

use super::store::console_warn;
use crate::pure::{Warned, css_color, decode_run};

#[wasm_bindgen]
extern "C" {
    /// A `CanvasRenderingContext2D`.
    #[derive(Clone, Debug)]
    pub type Ctx;
    #[wasm_bindgen(structural, method, setter = font)]
    pub(super) fn set_font(this: &Ctx, value: &str);
    #[wasm_bindgen(structural, method, js_name = measureText)]
    pub(super) fn measure_text(this: &Ctx, text: &str) -> JsValue;
    #[wasm_bindgen(structural, method, setter = fillStyle)]
    fn set_fill(this: &Ctx, value: &JsValue);
    #[wasm_bindgen(structural, method, setter = strokeStyle)]
    fn set_stroke(this: &Ctx, value: &JsValue);
    #[wasm_bindgen(structural, method, setter = lineWidth)]
    fn set_line_width(this: &Ctx, value: f64);
    #[wasm_bindgen(structural, method, setter = lineJoin)]
    fn set_line_join(this: &Ctx, value: &str);
    #[wasm_bindgen(structural, method, setter = lineCap)]
    fn set_line_cap(this: &Ctx, value: &str);
    #[wasm_bindgen(structural, method, setter = miterLimit)]
    fn set_miter_limit(this: &Ctx, value: f64);
    #[wasm_bindgen(structural, method, setter = globalAlpha)]
    fn set_alpha(this: &Ctx, value: f64);
    #[wasm_bindgen(structural, method, setter = imageSmoothingEnabled)]
    fn set_smoothing(this: &Ctx, value: bool);
    #[wasm_bindgen(structural, method, setter = shadowColor)]
    fn set_shadow_color(this: &Ctx, value: &str);
    #[wasm_bindgen(structural, method, setter = shadowBlur)]
    fn set_shadow_blur(this: &Ctx, value: f64);
    #[wasm_bindgen(structural, method, setter = shadowOffsetX)]
    fn set_shadow_x(this: &Ctx, value: f64);
    #[wasm_bindgen(structural, method, setter = textAlign)]
    fn set_text_align(this: &Ctx, value: &str);
    #[wasm_bindgen(structural, method, setter = textBaseline)]
    fn set_text_baseline(this: &Ctx, value: &str);
    #[wasm_bindgen(structural, method)]
    fn save(this: &Ctx);
    #[wasm_bindgen(structural, method)]
    fn restore(this: &Ctx);
    #[wasm_bindgen(structural, method, js_name = beginPath)]
    fn begin_path(this: &Ctx);
    #[wasm_bindgen(structural, method, js_name = closePath)]
    fn close_path(this: &Ctx);
    #[wasm_bindgen(structural, method, js_name = moveTo)]
    fn move_to(this: &Ctx, x: f64, y: f64);
    #[wasm_bindgen(structural, method, js_name = lineTo)]
    fn line_to(this: &Ctx, x: f64, y: f64);
    #[wasm_bindgen(structural, method, js_name = quadraticCurveTo)]
    fn quad_to(this: &Ctx, x1: f64, y1: f64, x: f64, y: f64);
    #[wasm_bindgen(structural, method, js_name = bezierCurveTo)]
    fn cubic_to(this: &Ctx, x1: f64, y1: f64, x2: f64, y2: f64, x: f64, y: f64);
    #[wasm_bindgen(structural, method, js_name = roundRect)]
    fn round_rect(this: &Ctx, x: f64, y: f64, w: f64, h: f64, radii: &JsValue);
    #[wasm_bindgen(structural, method, js_name = fillRect)]
    fn fill_rect(this: &Ctx, x: f64, y: f64, w: f64, h: f64);
    #[wasm_bindgen(structural, method)]
    fn fill(this: &Ctx, rule: &str);
    #[wasm_bindgen(structural, method)]
    fn stroke(this: &Ctx);
    #[wasm_bindgen(structural, method, js_name = fillText)]
    fn fill_text(this: &Ctx, text: &str, x: f64, y: f64);
    #[wasm_bindgen(structural, method, js_name = createLinearGradient)]
    fn linear(this: &Ctx, x0: f64, y0: f64, x1: f64, y1: f64) -> Grad;
    #[wasm_bindgen(structural, method, js_name = createRadialGradient)]
    fn radial(this: &Ctx, x0: f64, y0: f64, r0: f64, x1: f64, y1: f64, r1: f64) -> Grad;
    #[wasm_bindgen(structural, method, js_name = createConicGradient)]
    fn conic(this: &Ctx, angle: f64, x: f64, y: f64) -> Grad;
    #[wasm_bindgen(structural, method, js_name = drawImage)]
    fn draw_image(
        this: &Ctx,
        image: &JsValue,
        sx: f64,
        sy: f64,
        sw: f64,
        sh: f64,
        dx: f64,
        dy: f64,
        dw: f64,
        dh: f64,
    );
    #[wasm_bindgen(structural, method, js_name = putImageData)]
    fn put_image_data(this: &Ctx, data: &JsValue, x: f64, y: f64);
    #[wasm_bindgen(structural, method, js_name = createImageData)]
    fn create_image_data(this: &Ctx, w: f64, h: f64) -> JsValue;

    type Grad;
    #[wasm_bindgen(structural, method, js_name = addColorStop)]
    fn add_color_stop(this: &Grad, at: f64, color: &str);

    #[wasm_bindgen(js_namespace = document, js_name = createElement)]
    fn create_element(name: &str) -> JsValue;
}

fn context_of(canvas: &JsValue) -> Ctx {
    let get: js_sys::Function = Reflect::get(canvas, &"getContext".into())
        .expect("a canvas has getContext")
        .into();
    get.call1(canvas, &"2d".into())
        .expect("a 2d context")
        .into()
}

/// A canvas of no element's, and its context.
pub(super) fn document_canvas(width: u32, height: u32) -> Ctx {
    context_of(&new_canvas(width, height))
}

fn new_canvas(width: u32, height: u32) -> JsValue {
    let canvas = create_element("canvas");
    let _ = Reflect::set(&canvas, &"width".into(), &width.into());
    let _ = Reflect::set(&canvas, &"height".into(), &height.into());
    canvas
}

thread_local! {
    // By the handle's id: an image is turned into a canvas once, not per paint.
    static IMAGES: RefCell<HashMap<u64, JsValue>> = RefCell::default();
    static WARNED: RefCell<Warned> = RefCell::default();
}

fn unsupported(name: &'static str) {
    if WARNED.with_borrow_mut(|w| w.first(name)) {
        console_warn(&format!(
            "ljos: a {name} in a paint hook is not drawn on the web"
        ));
    }
}

fn image_canvas(image: &ImageHandle) -> JsValue {
    if let Some(canvas) = IMAGES.with_borrow(|m| m.get(&image.id()).cloned()) {
        return canvas;
    }
    let (w, h) = (image.width(), image.height());
    let canvas = new_canvas(w, h);
    let ctx = context_of(&canvas);
    let data = ctx.create_image_data(f64::from(w), f64::from(h));
    let pixels: Uint8ClampedArray = Reflect::get(&data, &"data".into())
        .expect("ImageData has data")
        .into();
    let mut straight = image.data().to_vec();
    for px in straight.as_chunks_mut::<4>().0 {
        let a = u32::from(px[3]);
        if a != 0 && a != 255 {
            for c in &mut px[..3] {
                *c = ((u32::from(*c) * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
    pixels.copy_from(&straight);
    ctx.put_image_data(&data, 0.0, 0.0);
    IMAGES.with_borrow_mut(|m| m.insert(image.id(), canvas.clone()));
    canvas
}

fn css(c: Rgba) -> String {
    css_color(c.r, c.g, c.b, c.a)
}

fn stops(grad: Grad, stops: &[GradientStop]) -> JsValue {
    for stop in stops {
        grad.add_color_stop(f64::from(stop.offset), &css(stop.color));
    }
    grad.into()
}

fn style_of(ctx: &Ctx, paint: &Paint) -> Option<JsValue> {
    match paint {
        Paint::Solid(c) => Some(JsValue::from_str(&css(*c))),
        Paint::LinearGradient(g) => {
            let grad = ctx.linear(g.start.x, g.start.y, g.end.x, g.end.y);
            Some(stops(grad, &g.stops))
        }
        Paint::Image(_) => {
            unsupported("Paint::Image");
            None
        }
    }
}

fn brush_of(ctx: &Ctx, brush: &Brush) -> JsValue {
    match brush {
        Brush::Solid(c) => JsValue::from_str(&css(*c)),
        Brush::Gradient(g) => {
            let grad = match g.geometry {
                GradientGeometry::Linear { start, end } => {
                    ctx.linear(start.x, start.y, end.x, end.y)
                }
                GradientGeometry::Radial {
                    focal,
                    focal_radius,
                    center,
                    radius,
                } => ctx.radial(focal.x, focal.y, focal_radius, center.x, center.y, radius),
                // Native turns anticlockwise from x, a canvas clockwise: only
                // the start maps directly, so the stops are walked backwards.
                GradientGeometry::Conical { center, angle } => {
                    let grad = ctx.conic(-angle.to_radians(), center.x, center.y);
                    for stop in &g.stops {
                        grad.add_color_stop(1.0 - f64::from(stop.offset), &css(stop.color));
                    }
                    return grad.into();
                }
            };
            stops(grad, &g.stops)
        }
    }
}

fn trace(ctx: &Ctx, path: &BezPath) {
    ctx.begin_path();
    for el in path.elements() {
        match *el {
            PathEl::MoveTo(p) => ctx.move_to(p.x, p.y),
            PathEl::LineTo(p) => ctx.line_to(p.x, p.y),
            PathEl::QuadTo(a, p) => ctx.quad_to(a.x, a.y, p.x, p.y),
            PathEl::CurveTo(a, b, p) => ctx.cubic_to(a.x, a.y, b.x, b.y, p.x, p.y),
            PathEl::ClosePath => ctx.close_path(),
        }
    }
}

fn rounded(ctx: &Ctx, r: Rect, radii: [f32; 4]) {
    ctx.begin_path();
    let list: Array = radii
        .iter()
        .map(|c| JsValue::from_f64(f64::from(*c)))
        .collect();
    ctx.round_rect(r.x0, r.y0, r.width(), r.height(), &list);
}

fn bar(ctx: &Ctx, r: Rect, x: f64, y: f64) {
    ctx.fill_rect(r.x0 + x, r.y0 + y, r.width(), r.height());
}

/// Each operation in order. The state a hook's list cannot have asked for
/// (a transform, a clip) is left where the face set it.
pub(super) fn replay(ctx: &Ctx, ops: &[DrawOp]) {
    for op in ops {
        match op {
            DrawOp::Rect {
                rect,
                radii,
                fill,
                border,
                aa: _,
            } => {
                if let Some(style) = style_of(ctx, fill) {
                    ctx.set_fill(&style);
                    if radii.iter().all(|r| *r == 0.0) {
                        ctx.fill_rect(rect.x0, rect.y0, rect.width(), rect.height());
                    } else {
                        rounded(ctx, *rect, *radii);
                        ctx.fill("nonzero");
                    }
                }
                if let Some(border) = border
                    && let Some(style) = style_of(ctx, &border.paint)
                {
                    // Inside the rectangle, as natively.
                    let half = f64::from(border.width) / 2.0;
                    let inner = rect.inflate(-half, -half);
                    ctx.set_stroke(&style);
                    ctx.set_line_width(f64::from(border.width));
                    ctx.set_line_join("miter");
                    let radii = radii.map(|r| (r - border.width / 2.0).max(0.0));
                    rounded(ctx, inner, radii);
                    ctx.stroke();
                }
            }
            DrawOp::Shadow {
                rect,
                radii,
                blur,
                spread,
                color,
            } => {
                // The caster is drawn off the canvas so only its shadow lands.
                const AWAY: f64 = 100_000.0;
                ctx.save();
                ctx.set_shadow_color(&css(*color));
                ctx.set_shadow_blur(f64::from(*blur));
                ctx.set_shadow_x(AWAY * device_scale(ctx));
                ctx.set_fill(&"black".into());
                let grown = rect.inflate(f64::from(*spread), f64::from(*spread));
                rounded(
                    ctx,
                    grown + ljos_render::kurbo::Vec2::new(-AWAY, 0.0),
                    *radii,
                );
                ctx.fill("nonzero");
                ctx.restore();
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
                if tile.is_some() || mirror[0] || mirror[1] {
                    unsupported("tiled or mirrored image");
                }
                if tint.is_some() {
                    unsupported("tinted image");
                }
                ctx.save();
                ctx.set_alpha(f64::from(*opacity));
                ctx.set_smoothing(*smooth);
                ctx.draw_image(
                    &image_canvas(image),
                    src.x0,
                    src.y0,
                    src.width(),
                    src.height(),
                    dst.x0,
                    dst.y0,
                    dst.width(),
                    dst.height(),
                );
                ctx.restore();
            }
            DrawOp::Glyphs {
                run,
                origin,
                paint,
                decorations,
            } => {
                let Some(style) = style_of(ctx, paint) else {
                    continue;
                };
                ctx.set_fill(&style);
                if let Some((font, text)) = decode_run(run.font.data.data()) {
                    ctx.set_font(font);
                    ctx.set_text_align("left");
                    ctx.set_text_baseline("alphabetic");
                    ctx.fill_text(text, origin.x, origin.y + f64::from(run.baseline));
                }
                if let Some(r) = decorations.underline {
                    bar(ctx, r, origin.x, origin.y);
                }
                if let Some(r) = decorations.strike_through {
                    bar(ctx, r, origin.x, origin.y);
                }
            }
            DrawOp::Text {
                rect,
                text,
                size,
                color,
                align,
            } => {
                ctx.set_fill(&JsValue::from_str(&css(*color)));
                ctx.set_font(&crate::pure::css_font("", *size, 400.0, false));
                ctx.set_text_baseline("top");
                let (name, x) = match align {
                    TextAlign::Left => ("left", rect.x0),
                    TextAlign::Center => ("center", rect.center().x),
                    TextAlign::Right => ("right", rect.x1),
                };
                ctx.set_text_align(name);
                ctx.fill_text(text, x, rect.y0);
            }
            DrawOp::Path { path, style, paint } => {
                let Some(fill) = style_of(ctx, paint) else {
                    continue;
                };
                trace(ctx, path);
                match style {
                    PathStyle::Fill => {
                        ctx.set_fill(&fill);
                        ctx.fill("nonzero");
                    }
                    PathStyle::FillEvenOdd => {
                        ctx.set_fill(&fill);
                        ctx.fill("evenodd");
                    }
                    PathStyle::Stroke { width } => {
                        ctx.set_stroke(&fill);
                        ctx.set_line_width(f64::from(*width));
                        ctx.set_line_join("round");
                        ctx.set_line_cap("butt");
                        ctx.stroke();
                    }
                }
            }
            DrawOp::Shape { path, style, brush } => {
                let paint = brush_of(ctx, brush);
                trace(ctx, path);
                match style {
                    ShapeStyle::Fill(rule) => {
                        ctx.set_fill(&paint);
                        ctx.fill(match rule {
                            FillRule::EvenOdd => "evenodd",
                            _ => "nonzero",
                        });
                    }
                    ShapeStyle::Stroke(s) => {
                        use ljos_render::kurbo::{Cap, Join};
                        ctx.set_stroke(&paint);
                        ctx.set_line_width(s.width);
                        ctx.set_miter_limit(s.miter_limit);
                        ctx.set_line_join(match s.join {
                            Join::Bevel => "bevel",
                            Join::Miter => "miter",
                            Join::Round => "round",
                        });
                        ctx.set_line_cap(match s.start_cap {
                            Cap::Butt => "butt",
                            Cap::Round => "round",
                            Cap::Square => "square",
                        });
                        if !s.dash_pattern.is_empty() {
                            unsupported("dashed stroke");
                        }
                        ctx.stroke();
                    }
                }
            }
            DrawOp::NinePatch { .. } => unsupported("NinePatch"),
            #[allow(unreachable_patterns)]
            _ => unsupported("Shader, View3d or Embed"),
        }
    }
}

// A shadow's offset is in device pixels whatever the transform is.
fn device_scale(ctx: &Ctx) -> f64 {
    let get = |name: &str| {
        Reflect::get(ctx, &name.into())
            .ok()
            .and_then(|f| js_sys::Function::from(f).call0(ctx).ok())
            .and_then(|m| Reflect::get(&m, &"a".into()).ok())
            .and_then(|a| a.as_f64())
    };
    get("getTransform").unwrap_or(1.0)
}
