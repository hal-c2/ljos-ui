//! The native half of a canvas's 2D context.
//!
//! Script sees an object made by `context.js` whose properties and methods
//! call three methods here: `read` and `write` for the state, `op` for
//! everything else. Gradient and pattern objects live in script; what they
//! describe arrives here as a list when one becomes a style.
//!
//! The path is kept in canvas pixels: each point goes through the transform
//! current when it is added, as the HTML canvas does and as Qt measured
//! (features/items/canvas-context.feature). Strokes are drawn in the space
//! of the transform current when they are drawn.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use ljos_engine::{Color, Cx, Engine, ObjectId, QmlObject, QtObject, Value, qml_impl};
use ljos_render::kurbo::{
    self, Affine, BezPath, Cap, Ellipse, Join, PathEl, Point, Rect, Shape, Stroke, Vec2,
};
use ljos_render::peniko::BlendMode;
use ljos_render::{
    Brush, BrushGradient, CanvasPaint, CanvasShadow, DrawState, FillRule, GradientGeometry,
    GradientStop, ImageHandle, Spread,
};
use ljos_text::{LayoutParams, TextStyle};

use super::style::{blend_mode, color_string, hatch_cell, parse_color, parse_font};
use super::{Canvas, SharedRun, drawn_outside_paint, image_for, run_of};
use crate::image::ImagePaint;
use crate::item::is_a;
use crate::rectangle::to_rgba;

/// How finely arcs are cut into curves, in user units.
const ARC_TOLERANCE: f64 = 0.01;

/// What a fill or a stroke is drawn with.
#[derive(Clone, Debug)]
enum Style {
    Color(Color),
    Gradient(Arc<BrushGradient>),
    Pattern(ImageHandle, [bool; 2]),
    /// A hatching: an 8 by 8 cell in canvas pixels.
    Hatch(ImageHandle),
    /// A pattern whose image is not there: draws nothing.
    Nothing,
}

impl Style {
    fn paint(&self) -> Option<CanvasPaint> {
        Some(match self {
            Style::Color(c) => CanvasPaint::Brush(Brush::Solid(to_rgba(*c))),
            Style::Gradient(g) => CanvasPaint::Brush(Brush::Gradient(Arc::clone(g))),
            Style::Pattern(image, repeat) => CanvasPaint::Pattern {
                image: image.clone(),
                repeat: *repeat,
            },
            Style::Hatch(image) => CanvasPaint::Hatch {
                image: image.clone(),
            },
            Style::Nothing => return None,
        })
    }
}

/// The state `save` keeps and `restore` brings back.
#[derive(Clone, Debug)]
struct Gs {
    fill: Style,
    stroke: Style,
    line_width: f64,
    cap: Cap,
    join: Join,
    miter: f64,
    /// Always an even number of lengths.
    dash: Vec<f64>,
    dash_offset: f64,
    alpha: f64,
    composite: String,
    blend: BlendMode,
    font: String,
    text: TextStyle,
    align: &'static str,
    baseline: &'static str,
    shadow_blur: f64,
    shadow_color: Color,
    shadow_x: f64,
    shadow_y: f64,
    /// `Qt.OddEvenFill` (0) or `Qt.WindingFill` (1).
    fill_rule: i32,
    transform: Affine,
    clips: Vec<BezPath>,
}

const DEFAULT_FONT: &str = "10px sans-serif";

impl Default for Gs {
    fn default() -> Self {
        let black = Color::from_rgba8(0, 0, 0, 255);
        Self {
            fill: Style::Color(black),
            stroke: Style::Color(black),
            line_width: 1.0,
            cap: Cap::Butt,
            join: Join::Miter,
            miter: 10.0,
            dash: Vec::new(),
            dash_offset: 0.0,
            alpha: 1.0,
            composite: "source-over".into(),
            blend: BlendMode::default(),
            font: DEFAULT_FONT.into(),
            text: parse_font(DEFAULT_FONT).unwrap_or_default(),
            align: "start",
            baseline: "alphabetic",
            shadow_blur: 0.0,
            // Qt's default is opaque black, not the HTML canvas's
            // transparent black: an offset alone casts a shadow (measured).
            shadow_color: black,
            shadow_x: 0.0,
            shadow_y: 0.0,
            fill_rule: 1,
            transform: Affine::IDENTITY,
            clips: Vec::new(),
        }
    }
}

#[derive(Debug, Default)]
struct Ctx {
    gs: Gs,
    saved: Vec<Gs>,
    /// In canvas pixels.
    path: BezPath,
    /// Where the current subpath started and where it is now, in canvas
    /// pixels.
    start: Option<Point>,
    current: Option<Point>,
}

/// The native half of a `getContext("2d")` object.
#[derive(Debug, Default, QmlObject)]
#[qml(name = "QQuickContext2D", extends = QtObject)]
pub struct Context2D {
    #[qml(base)]
    pub object: QtObject,
    #[qml(skip)]
    canvas: Option<ObjectId>,
    #[qml(skip)]
    run: SharedRun,
    #[qml(skip)]
    ctx: Rc<RefCell<Ctx>>,
}

impl Context2D {
    pub(crate) fn new(canvas: ObjectId, run: SharedRun) -> Self {
        Self {
            object: QtObject::default(),
            canvas: Some(canvas),
            run,
            ctx: Rc::default(),
        }
    }
}

type Parts = (ObjectId, SharedRun, Rc<RefCell<Ctx>>);

fn parts(cx: &Cx) -> Option<Parts> {
    cx.engine
        .with(cx.this, |c: &Context2D| {
            Some((c.canvas?, Rc::clone(&c.run), Rc::clone(&c.ctx)))
        })
        .ok()
        .flatten()
}

#[qml_impl]
impl Context2D {
    /// A state property.
    #[qml(invokable)]
    fn read(cx: &Cx, name: Value) -> Value {
        let Some((_, _, ctx)) = parts(cx) else {
            return Value::Undefined;
        };
        let c = ctx.borrow();
        let gs = &c.gs;
        let s = |v: &str| Value::String(v.into());
        match name.to_js_string().as_str() {
            "fillStyle" => style_value(&gs.fill),
            "strokeStyle" => style_value(&gs.stroke),
            "lineWidth" => Value::Real(gs.line_width),
            "lineCap" => s(match gs.cap {
                Cap::Butt => "butt",
                Cap::Round => "round",
                Cap::Square => "square",
            }),
            "lineJoin" => s(match gs.join {
                Join::Miter => "miter",
                Join::Round => "round",
                Join::Bevel => "bevel",
            }),
            "miterLimit" => Value::Real(gs.miter),
            "lineDashOffset" => Value::Real(gs.dash_offset),
            "globalAlpha" => Value::Real(gs.alpha),
            "globalCompositeOperation" => s(&gs.composite),
            "font" => s(&gs.font),
            "textAlign" => s(gs.align),
            "textBaseline" => s(gs.baseline),
            "shadowBlur" => Value::Real(gs.shadow_blur),
            "shadowColor" => s(&color_string(gs.shadow_color)),
            "shadowOffsetX" => Value::Real(gs.shadow_x),
            "shadowOffsetY" => Value::Real(gs.shadow_y),
            "fillRule" => Value::Int(gs.fill_rule),
            _ => Value::Undefined,
        }
    }

    /// Sets a state property; whether the value was taken. A value the
    /// property cannot have leaves it as it was.
    #[qml(invokable)]
    fn write(cx: &Cx, name: Value, value: Value) -> bool {
        let Some((canvas, run, ctx)) = parts(cx) else {
            return false;
        };
        let name = name.to_js_string();
        let name = name.as_str();
        if matches!(name, "fillStyle" | "strokeStyle") {
            let Some(style) = style_of(cx.engine, canvas, &run, &value) else {
                return false;
            };
            let mut c = ctx.borrow_mut();
            if name == "fillStyle" {
                c.gs.fill = style;
            } else {
                c.gs.stroke = style;
            }
            return true;
        }
        let mut c = ctx.borrow_mut();
        let gs = &mut c.gs;
        let n = value.to_number();
        let positive = n.is_finite() && n > 0.0;
        let text = value.to_js_string();
        let text = text.as_str();
        match name {
            "lineWidth" if positive => gs.line_width = n,
            "miterLimit" if positive => gs.miter = n,
            "lineDashOffset" if n.is_finite() => gs.dash_offset = n,
            "globalAlpha" if (0.0..=1.0).contains(&n) => gs.alpha = n,
            // Qt ignores a blur of zero as it does a negative one
            // (measured); only `reset()` brings it back to zero.
            "shadowBlur" if positive => gs.shadow_blur = n,
            "shadowOffsetX" if n.is_finite() => gs.shadow_x = n,
            "shadowOffsetY" if n.is_finite() => gs.shadow_y = n,
            "fillRule" if n == 0.0 || n == 1.0 => gs.fill_rule = n as i32,
            "lineCap" => match text {
                "butt" => gs.cap = Cap::Butt,
                "round" => gs.cap = Cap::Round,
                "square" => gs.cap = Cap::Square,
                _ => return false,
            },
            "lineJoin" => match text {
                "miter" => gs.join = Join::Miter,
                "round" => gs.join = Join::Round,
                "bevel" => gs.join = Join::Bevel,
                _ => return false,
            },
            "globalCompositeOperation" => match blend_mode(text) {
                Some(b) => {
                    gs.blend = b;
                    gs.composite = text.to_owned();
                }
                None => return false,
            },
            "font" => match parse_font(text) {
                Some(style) => {
                    gs.text = style;
                    gs.font = text.to_owned();
                }
                None => return false,
            },
            "textAlign" => {
                gs.align = match text {
                    "start" => "start",
                    "end" => "end",
                    "left" => "left",
                    "right" => "right",
                    "center" => "center",
                    _ => return false,
                }
            }
            "textBaseline" => {
                gs.baseline = match text {
                    "top" => "top",
                    "hanging" => "hanging",
                    "middle" => "middle",
                    "alphabetic" => "alphabetic",
                    "ideographic" => "ideographic",
                    "bottom" => "bottom",
                    _ => return false,
                }
            }
            "shadowColor" => match color_of(&value) {
                Some(c) => gs.shadow_color = c,
                None => return false,
            },
            _ => return false,
        }
        true
    }

    /// A method of the context: `name` with `args`. `undefined` for one
    /// that gives nothing back, which script turns into the context.
    #[qml(invokable)]
    fn op(cx: &Cx, name: Value, args: Value) -> Value {
        let Some((canvas, run, ctx)) = parts(cx) else {
            return Value::Undefined;
        };
        let args: Vec<Value> = match args {
            Value::List(l) => l.as_ref().clone(),
            _ => Vec::new(),
        };
        let name = name.to_js_string();
        let op = Op {
            e: cx.engine,
            canvas,
            run: &run,
            ctx: &ctx,
            args: &args,
        };
        let (out, drew) = op.run(name.as_str());
        if drew {
            let in_paint = run.borrow().in_paint;
            if in_paint {
                run.borrow_mut().drawn = true;
            } else {
                drawn_outside_paint(cx.engine, &run);
            }
        }
        out
    }
}

fn style_value(s: &Style) -> Value {
    match s {
        Style::Color(c) => Value::String(color_string(*c).into()),
        _ => Value::Null,
    }
}

fn color_of(v: &Value) -> Option<Color> {
    match v {
        Value::Color(c) => Some(*c),
        Value::String(s) | Value::Url(s) => parse_color(s.as_str()),
        _ => None,
    }
}

fn list(v: &Value) -> &[Value] {
    match v {
        Value::List(l) => l.as_slice(),
        _ => &[],
    }
}

/// A style from a colour, or from what script made of a gradient or a
/// pattern: `[kind, [numbers], [[offset, colour], ...]]`, `["pattern",
/// image, repetition]` or `["solid", colour, brushStyle]`.
fn style_of(e: &Engine, canvas: ObjectId, run: &SharedRun, v: &Value) -> Option<Style> {
    if let Some(c) = color_of(v) {
        return Some(Style::Color(c));
    }
    let desc = list(v);
    let kind = desc.first()?.to_js_string();
    let nums: Vec<f64> = desc
        .get(1)
        .map(|a| list(a).iter().map(Value::to_number).collect())
        .unwrap_or_default();
    let at = |i: usize| nums.get(i).copied().unwrap_or(0.0);
    let geometry = match kind.as_str() {
        "linear" => GradientGeometry::Linear {
            start: Point::new(at(0), at(1)),
            end: Point::new(at(2), at(3)),
        },
        "radial" => GradientGeometry::Radial {
            focal: Point::new(at(0), at(1)),
            focal_radius: at(2).max(0.0),
            center: Point::new(at(3), at(4)),
            radius: at(5).max(0.0),
        },
        "conical" => GradientGeometry::Conical {
            center: Point::new(at(0), at(1)),
            angle: at(2).to_degrees(),
        },
        "solid" => {
            // Qt's createPattern(colour, brushStyle): NoBrush draws
            // nothing, a hatching its cell, any other style the colour.
            let c = desc.get(1).and_then(color_of)?;
            let style = desc.get(2).map_or(1.0, Value::to_number);
            if style == 0.0 {
                return Some(Style::Nothing);
            }
            let cell = hatch_cell(style as i32).and_then(|cell| hatch(c, cell));
            return Some(cell.map_or(Style::Color(c), Style::Hatch));
        }
        "pattern" => {
            let image = desc.get(1).and_then(|s| image_of(e, canvas, run, s));
            let repeat = match desc.get(2).map(Value::to_js_string).as_deref() {
                Some("repeat-x") => [true, false],
                Some("repeat-y") => [false, true],
                Some("no-repeat") => [false, false],
                _ => [true, true],
            };
            return Some(match image {
                Some(i) => Style::Pattern(i, repeat),
                None => Style::Nothing,
            });
        }
        _ => return None,
    };
    if nums.iter().any(|n| !n.is_finite()) {
        return None;
    }
    let stops: Vec<GradientStop> = desc
        .get(2)
        .map(|s| {
            list(s)
                .iter()
                .filter_map(|stop| {
                    let stop = list(stop);
                    let offset = stop.first()?.to_number();
                    let color = color_of(stop.get(1)?)?;
                    (0.0..=1.0).contains(&offset).then(|| GradientStop {
                        offset: offset as f32,
                        color: to_rgba(color),
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    if stops.is_empty() {
        // A gradient with no stops is transparent black.
        return Some(Style::Color(Color::from_rgba8(0, 0, 0, 0)));
    }
    Some(Style::Gradient(Arc::new(BrushGradient::new(
        geometry,
        stops,
        Spread::Pad,
    ))))
}

/// The pixels an image argument names: a URL, an `Image` or `Canvas`
/// item, or image data as `["data", width, height, [bytes]]`.
fn image_of(e: &Engine, canvas: ObjectId, run: &SharedRun, v: &Value) -> Option<ImageHandle> {
    match v {
        Value::String(s) | Value::Url(s) => image_for(e, canvas, &run.borrow(), s.as_str()),
        Value::Object(obj) if *obj == canvas => run.borrow_mut().painter.picture(),
        Value::Object(obj) if is_a::<Canvas>(e, *obj) => {
            run_of(e, *obj)?.borrow_mut().painter.picture()
        }
        Value::Object(obj) => e.untrack(|| ImagePaint::read(e, *obj).image().cloned()),
        Value::List(l)
            if l.first()
                .is_some_and(|k| k.to_js_string().as_str() == "data") =>
        {
            let (w, h, bytes) = image_data_of(l)?;
            ImageHandle::from_straight(w, h, bytes).ok()
        }
        _ => None,
    }
}

/// `["data", width, height, [bytes]]` as its parts.
fn image_data_of(l: &[Value]) -> Option<(u32, u32, Vec<u8>)> {
    let w = l.get(1)?.to_number();
    let h = l.get(2)?.to_number();
    if !(w >= 1.0 && h >= 1.0 && w.is_finite() && h.is_finite()) {
        return None;
    }
    let (w, h) = (w as u32, h as u32);
    let bytes: Vec<u8> = list(l.get(3)?)
        .iter()
        .map(|b| b.to_number().clamp(0.0, 255.0) as u8)
        .collect();
    (bytes.len() >= w as usize * h as usize * 4).then_some((w, h, bytes))
}

/// One call of `op`.
struct Op<'a> {
    e: &'a Engine,
    canvas: ObjectId,
    run: &'a SharedRun,
    ctx: &'a Rc<RefCell<Ctx>>,
    args: &'a [Value],
}

impl Op<'_> {
    fn n(&self, i: usize) -> f64 {
        self.args.get(i).map_or(f64::NAN, Value::to_number)
    }

    /// The first `count` arguments as numbers, when they are all finite.
    fn nums<const N: usize>(&self) -> Option<[f64; N]> {
        let mut out = [0.0; N];
        for (i, o) in out.iter_mut().enumerate() {
            *o = self.n(i);
            if !o.is_finite() {
                return None;
            }
        }
        Some(out)
    }

    /// Runs `name`: what it gives back and whether it drew.
    fn run(&self, name: &str) -> (Value, bool) {
        let mut drew = false;
        let out = match name {
            "save" => {
                let mut c = self.ctx.borrow_mut();
                let gs = c.gs.clone();
                c.saved.push(gs);
                Value::Undefined
            }
            "restore" => {
                let mut c = self.ctx.borrow_mut();
                if let Some(gs) = c.saved.pop() {
                    c.gs = gs;
                }
                Value::Undefined
            }
            "reset" => {
                *self.ctx.borrow_mut() = Ctx::default();
                Value::Undefined
            }
            "scale" | "rotate" | "translate" | "transform" | "setTransform" | "shear"
            | "resetTransform" => {
                self.transform(name);
                Value::Undefined
            }
            "beginPath" => {
                let mut c = self.ctx.borrow_mut();
                c.path = BezPath::new();
                c.start = None;
                c.current = None;
                Value::Undefined
            }
            "closePath" => {
                let mut c = self.ctx.borrow_mut();
                if c.current.is_some() {
                    c.path.close_path();
                    c.current = c.start;
                }
                Value::Undefined
            }
            "moveTo" | "lineTo" | "quadraticCurveTo" | "bezierCurveTo" | "arc" | "arcTo"
            | "rect" | "roundedRect" | "ellipse" | "text" => {
                self.path_op(name);
                Value::Undefined
            }
            "fill" | "stroke" | "clip" => {
                drew = name != "clip";
                self.use_path(name);
                Value::Undefined
            }
            "fillRect" | "strokeRect" | "clearRect" => {
                if let Some([x, y, w, h]) = self.nums() {
                    drew = true;
                    self.rect_op(name, Rect::new(x, y, x + w, y + h));
                }
                Value::Undefined
            }
            "fillText" | "strokeText" => {
                drew = self.text_op(name == "fillText");
                Value::Undefined
            }
            "measureText" => {
                let text = self
                    .args
                    .first()
                    .map(Value::to_js_string)
                    .unwrap_or_default();
                let style = self.ctx.borrow().gs.text.clone();
                let layout = crate::text::text_engine().borrow_mut().layout(
                    text.as_str(),
                    &style,
                    &LayoutParams::default(),
                );
                Value::Real(layout.natural_width)
            }
            "isPointInPath" => {
                let Some([x, y]) = self.nums() else {
                    return (Value::Bool(false), false);
                };
                let c = self.ctx.borrow();
                let w = c.path.winding(Point::new(x, y));
                Value::Bool(if c.gs.fill_rule == 0 {
                    w % 2 != 0
                } else {
                    w != 0
                })
            }
            "setLineDash" => {
                let dash: Vec<f64> = self
                    .args
                    .first()
                    .map(|a| list(a).iter().map(Value::to_number).collect())
                    .unwrap_or_default();
                if dash.iter().all(|d| d.is_finite() && *d >= 0.0) {
                    let mut dash = dash;
                    if dash.len() % 2 == 1 {
                        dash.extend_from_within(..);
                    }
                    self.ctx.borrow_mut().gs.dash = dash;
                }
                Value::Undefined
            }
            "getLineDash" => Value::list(
                self.ctx
                    .borrow()
                    .gs
                    .dash
                    .iter()
                    .map(|d| Value::Real(*d))
                    .collect(),
            ),
            "drawImage" => {
                drew = self.draw_image();
                Value::Undefined
            }
            "getImageData" => self.get_image_data(),
            "putImageData" => {
                drew = self.put_image_data();
                Value::Undefined
            }
            _ => Value::Undefined,
        };
        (out, drew)
    }

    fn transform(&self, name: &str) {
        let mut c = self.ctx.borrow_mut();
        let t = c.gs.transform;
        c.gs.transform = match name {
            "resetTransform" => Affine::IDENTITY,
            "rotate" => match self.nums() {
                Some([a]) => t * Affine::rotate(a),
                None => return,
            },
            "scale" => match self.nums() {
                Some([x, y]) => t * Affine::scale_non_uniform(x, y),
                None => return,
            },
            "translate" => match self.nums() {
                Some([x, y]) => t * Affine::translate((x, y)),
                None => return,
            },
            // Qt's shear(sh, sv): x moves by sh·y, y by sv·x.
            "shear" => match self.nums() {
                Some([sh, sv]) => t * Affine::new([1.0, sv, sh, 1.0, 0.0, 0.0]),
                None => return,
            },
            "transform" => match self.nums() {
                Some(m) => t * Affine::new(m),
                None => return,
            },
            "setTransform" => match self.nums() {
                Some(m) => Affine::new(m),
                None => return,
            },
            _ => return,
        };
    }

    fn path_op(&self, name: &str) {
        let mut c = self.ctx.borrow_mut();
        let t = c.gs.transform;
        match name {
            "moveTo" => {
                if let Some([x, y]) = self.nums() {
                    c.move_to(t * Point::new(x, y));
                }
            }
            "lineTo" => {
                if let Some([x, y]) = self.nums() {
                    c.line_to(t * Point::new(x, y));
                }
            }
            "quadraticCurveTo" => {
                if let Some([cx, cy, x, y]) = self.nums() {
                    let p1 = t * Point::new(cx, cy);
                    c.ensure(p1);
                    let p2 = t * Point::new(x, y);
                    c.path.quad_to(p1, p2);
                    c.current = Some(p2);
                }
            }
            "bezierCurveTo" => {
                if let Some([x1, y1, x2, y2, x, y]) = self.nums() {
                    let p1 = t * Point::new(x1, y1);
                    c.ensure(p1);
                    let p3 = t * Point::new(x, y);
                    c.path.curve_to(p1, t * Point::new(x2, y2), p3);
                    c.current = Some(p3);
                }
            }
            "arc" => {
                if let Some([x, y, r, a0, a1]) = self.nums()
                    && r >= 0.0
                {
                    let ccw = self.args.get(5).is_some_and(Value::to_boolean);
                    let arc = kurbo::Arc::new(
                        Point::new(x, y),
                        Vec2::new(r, r),
                        a0,
                        sweep(a0, a1, ccw),
                        0.0,
                    );
                    c.add_arc(&arc, t, true);
                }
            }
            "arcTo" => {
                if let Some([x1, y1, x2, y2, r]) = self.nums()
                    && r >= 0.0
                {
                    c.arc_to(Point::new(x1, y1), Point::new(x2, y2), r, t);
                }
            }
            "rect" => {
                if let Some([x, y, w, h]) = self.nums() {
                    let p = t * Point::new(x, y);
                    c.path.move_to(p);
                    c.path.line_to(t * Point::new(x + w, y));
                    c.path.line_to(t * Point::new(x + w, y + h));
                    c.path.line_to(t * Point::new(x, y + h));
                    c.path.close_path();
                    c.start = Some(p);
                    c.current = Some(p);
                }
            }
            "roundedRect" => {
                if let Some([x, y, w, h, rx, ry]) = self.nums() {
                    let shape = rounded_rect(Rect::new(x, y, x + w, y + h), rx, ry);
                    c.add_closed(&shape, t);
                }
            }
            "ellipse" => {
                if let Some([x, y, w, h]) = self.nums() {
                    let shape =
                        Ellipse::from_rect(Rect::new(x, y, x + w, y + h)).to_path(ARC_TOLERANCE);
                    c.add_closed(&shape, t);
                }
            }
            "text" => {
                if let (Some(text), Some([_, x, y])) = (self.args.first(), self.nums_from::<3>(0)) {
                    let style = c.gs.text.clone();
                    let (align, baseline) = (c.gs.align, c.gs.baseline);
                    drop(c);
                    let shape =
                        self.glyphs(&text.to_js_string(), &style, align, baseline, x, y, None);
                    self.ctx.borrow_mut().add_closed(&shape, t);
                }
            }
            _ => {}
        }
    }

    /// Like [`Op::nums`] but leaves non-numbers at `skip` alone.
    fn nums_from<const N: usize>(&self, skip: usize) -> Option<[f64; N]> {
        let mut out = [0.0; N];
        for (i, o) in out.iter_mut().enumerate() {
            if i == skip {
                continue;
            }
            *o = self.n(i);
            if !o.is_finite() {
                return None;
            }
        }
        Some(out)
    }

    fn fill_rule(&self) -> FillRule {
        let explicit = self.args.first().map(Value::to_js_string);
        match explicit.as_deref() {
            Some("evenodd") => FillRule::EvenOdd,
            Some("nonzero") => FillRule::NonZero,
            _ if self.ctx.borrow().gs.fill_rule == 0 => FillRule::EvenOdd,
            _ => FillRule::NonZero,
        }
    }

    fn use_path(&self, name: &str) {
        let rule = self.fill_rule();
        let c = self.ctx.borrow();
        match name {
            "fill" => {
                if let Some((paint, st)) = with_style(&c.gs, &c.gs.fill) {
                    self.run
                        .borrow_mut()
                        .painter
                        .fill(&c.path, rule, &paint, &st);
                }
            }
            "stroke" => {
                if let Some((paint, st)) = with_style(&c.gs, &c.gs.stroke) {
                    let stroke = stroke_of(&c.gs);
                    self.run
                        .borrow_mut()
                        .painter
                        .stroke(&c.path, &stroke, &paint, &st);
                }
            }
            "clip" => {
                let mut clip = c.path.clone();
                drop(c);
                if rule == FillRule::EvenOdd {
                    // A clip is wound non-zero; the even-odd one keeps the
                    // parts an odd number of edges in.
                    clip = even_odd_as_non_zero(&clip);
                }
                self.ctx.borrow_mut().gs.clips.push(clip);
            }
            _ => {}
        }
    }

    fn rect_op(&self, name: &str, rect: Rect) {
        let c = self.ctx.borrow();
        let st = state(&c.gs);
        let mut r = self.run.borrow_mut();
        let painter = &mut r.painter;
        match name {
            "clearRect" => painter.clear_rect(rect, &st),
            "fillRect" => {
                if let Some((paint, st)) = with_style(&c.gs, &c.gs.fill) {
                    let mut area = rect.abs().to_path(0.1);
                    area.apply_affine(c.gs.transform);
                    painter.fill(&area, FillRule::NonZero, &paint, &st);
                }
            }
            _ => {
                if let Some((paint, st)) = with_style(&c.gs, &c.gs.stroke) {
                    let mut area = rect.abs().to_path(0.1);
                    area.apply_affine(c.gs.transform);
                    let stroke = stroke_of(&c.gs);
                    painter.stroke(&area, &stroke, &paint, &st);
                }
            }
        }
    }

    /// The outlines of `text` placed at `x`, `y` in user space.
    #[allow(clippy::too_many_arguments)]
    fn glyphs(
        &self,
        text: &str,
        style: &TextStyle,
        align: &str,
        baseline: &str,
        x: f64,
        y: f64,
        max_width: Option<f64>,
    ) -> BezPath {
        let (layout, metrics) = {
            let engine = crate::text::text_engine();
            let mut engine = engine.borrow_mut();
            let layout = engine.layout(text, style, &LayoutParams::default());
            let metrics = engine.font_metrics(style);
            (layout, metrics)
        };
        let width = layout.natural_width;
        let dx = match align {
            "right" | "end" => -width,
            "center" => -width / 2.0,
            _ => 0.0,
        };
        let shift = match baseline {
            "top" | "hanging" => metrics.ascent,
            "middle" => (metrics.ascent - metrics.descent) / 2.0,
            "bottom" | "ideographic" => -metrics.descent,
            _ => 0.0,
        };
        let first = layout.lines.first().map_or(0.0, |l| l.baseline);
        let origin = Point::new(x + dx, y + shift - first);
        let mut out = BezPath::new();
        {
            let mut r = self.run.borrow_mut();
            for line in &layout.lines {
                for run in &line.runs {
                    let p = r.painter.glyph_path(&run.glyphs, origin);
                    out.extend(p);
                }
            }
        }
        if let Some(max) = max_width.filter(|m| *m >= 0.0 && *m < width && width > 0.0) {
            let anchor = x;
            out.apply_affine(
                Affine::translate((anchor, 0.0))
                    * Affine::scale_non_uniform(max / width, 1.0)
                    * Affine::translate((-anchor, 0.0)),
            );
        }
        out
    }

    fn text_op(&self, fill: bool) -> bool {
        let Some(text) = self.args.first().map(Value::to_js_string) else {
            return false;
        };
        let Some([_, x, y]) = self.nums_from::<3>(0) else {
            return false;
        };
        let max_width = self
            .args
            .get(3)
            .map(Value::to_number)
            .filter(|m| m.is_finite());
        let (style, align, baseline) = {
            let c = self.ctx.borrow();
            (c.gs.text.clone(), c.gs.align, c.gs.baseline)
        };
        let user = self.glyphs(text.as_str(), &style, align, baseline, x, y, max_width);
        let c = self.ctx.borrow();
        let mut device = user;
        device.apply_affine(c.gs.transform);
        let mut r = self.run.borrow_mut();
        if fill {
            if let Some((paint, st)) = with_style(&c.gs, &c.gs.fill) {
                r.painter.fill(&device, FillRule::NonZero, &paint, &st);
            }
        } else if let Some((paint, st)) = with_style(&c.gs, &c.gs.stroke) {
            let stroke = stroke_of(&c.gs);
            r.painter.stroke(&device, &stroke, &paint, &st);
        }
        true
    }

    fn draw_image(&self) -> bool {
        let Some(source) = self.args.first() else {
            return false;
        };
        let Some(image) = image_of(self.e, self.canvas, self.run, source) else {
            return false;
        };
        let (iw, ih) = (f64::from(image.width()), f64::from(image.height()));
        let n = |i: usize| self.n(i + 1);
        let (src, dst) = match self.args.len() {
            3 => (
                Rect::new(0.0, 0.0, iw, ih),
                Rect::new(n(0), n(1), n(0) + iw, n(1) + ih),
            ),
            5 => (
                Rect::new(0.0, 0.0, iw, ih),
                Rect::new(n(0), n(1), n(0) + n(2), n(1) + n(3)),
            ),
            9 => (
                Rect::new(n(0), n(1), n(0) + n(2), n(1) + n(3)),
                Rect::new(n(4), n(5), n(4) + n(6), n(5) + n(7)),
            ),
            _ => return false,
        };
        let finite = |r: Rect| [r.x0, r.y0, r.x1, r.y1].iter().all(|v| v.is_finite());
        if !finite(src) || !finite(dst) {
            return false;
        }
        let (src, dst) = (src.abs(), dst.abs());
        // The part of the source outside the image draws nothing.
        let inside = src.intersect(Rect::new(0.0, 0.0, iw, ih));
        if inside.area() <= 0.0 {
            return false;
        }
        let sx = dst.width() / src.width();
        let sy = dst.height() / src.height();
        let dst = Rect::new(
            dst.x0 + (inside.x0 - src.x0) * sx,
            dst.y0 + (inside.y0 - src.y0) * sy,
            dst.x0 + (inside.x1 - src.x0) * sx,
            dst.y0 + (inside.y1 - src.y0) * sy,
        );
        let c = self.ctx.borrow();
        let st = state(&c.gs);
        self.run
            .borrow_mut()
            .painter
            .draw_image(&image, inside, dst, &st);
        true
    }

    /// `[width, height, [bytes]]`.
    fn get_image_data(&self) -> Value {
        let Some([x, y, w, h]) = self.nums() else {
            return Value::Undefined;
        };
        let (x, w) = if w < 0.0 { (x + w, -w) } else { (x, w) };
        let (y, h) = if h < 0.0 { (y + h, -h) } else { (y, h) };
        let (w, h) = (w.round().max(1.0) as u32, h.round().max(1.0) as u32);
        let bytes =
            self.run
                .borrow_mut()
                .painter
                .image_data(x.floor() as i64, y.floor() as i64, w, h);
        Value::list(vec![
            Value::Int(w as i32),
            Value::Int(h as i32),
            Value::list(
                bytes
                    .into_iter()
                    .map(|b| Value::Int(i32::from(b)))
                    .collect(),
            ),
        ])
    }

    fn put_image_data(&self) -> bool {
        let Some((w, h, bytes)) = self.args.first().and_then(|d| image_data_of(list(d))) else {
            return false;
        };
        let (dx, dy) = (self.n(1), self.n(2));
        if !dx.is_finite() || !dy.is_finite() {
            return false;
        }
        let dirty = if self.args.len() >= 7 {
            let [x, y, dw, dh] = [self.n(3), self.n(4), self.n(5), self.n(6)];
            if ![x, y, dw, dh].iter().all(|v| v.is_finite()) {
                return false;
            }
            Rect::new(x, y, x + dw, y + dh).abs()
        } else {
            Rect::new(0.0, 0.0, f64::from(w), f64::from(h))
        };
        self.run.borrow_mut().painter.put_image_data(
            &bytes,
            w,
            h,
            dx.floor() as i64,
            dy.floor() as i64,
            dirty,
        );
        true
    }
}

impl Ctx {
    fn move_to(&mut self, p: Point) {
        self.path.move_to(p);
        self.start = Some(p);
        self.current = Some(p);
    }

    fn line_to(&mut self, p: Point) {
        if self.current.is_none() {
            self.move_to(p);
        } else {
            self.path.line_to(p);
            self.current = Some(p);
        }
    }

    /// Starts a subpath at `p` when there is none.
    fn ensure(&mut self, p: Point) {
        if self.current.is_none() {
            self.move_to(p);
        }
    }

    /// Adds `arc` (in user space), joined by a line to what came before
    /// when `join`.
    fn add_arc(&mut self, arc: &kurbo::Arc, t: Affine, join: bool) {
        let start = t
            * (arc.center
                + Vec2::new(
                    arc.radii.x * arc.start_angle.cos(),
                    arc.radii.y * arc.start_angle.sin(),
                ));
        if join {
            self.line_to(start);
        } else {
            self.ensure(start);
        }
        let mut last = start;
        arc.append_iter(ARC_TOLERANCE).for_each(|el| {
            let el = t * el;
            if let Some(p) = el.end_point() {
                last = p;
            }
            self.path.push(el);
        });
        self.current = Some(last);
    }

    fn arc_to(&mut self, p1: Point, p2: Point, r: f64, t: Affine) {
        let Some(current) = self.current else {
            self.move_to(t * p1);
            return;
        };
        if t.determinant().abs() < 1e-12 {
            return;
        }
        let p0 = t.inverse() * current;
        let v1 = p0 - p1;
        let v2 = p2 - p1;
        let cross = v1.cross(v2);
        if p0 == p1 || p1 == p2 || r == 0.0 || cross.abs() < 1e-9 {
            self.line_to(t * p1);
            return;
        }
        let (u1, u2) = (v1.normalize(), v2.normalize());
        let angle = u1.dot(u2).clamp(-1.0, 1.0).acos();
        let along = r / (angle / 2.0).tan();
        let t1 = p1 + u1 * along;
        let t2 = p1 + u2 * along;
        let centre = p1 + (u1 + u2).normalize() * (r / (angle / 2.0).sin());
        let a0 = (t1 - centre).atan2();
        let a1 = (t2 - centre).atan2();
        let mut sweep = a1 - a0;
        while sweep > std::f64::consts::PI {
            sweep -= std::f64::consts::TAU;
        }
        while sweep < -std::f64::consts::PI {
            sweep += std::f64::consts::TAU;
        }
        let arc = kurbo::Arc::new(centre, Vec2::new(r, r), a0, sweep, 0.0);
        self.add_arc(&arc, t, true);
    }

    /// Adds a closed shape (in user space) as subpaths of its own; the
    /// next point starts a new one where the shape started.
    fn add_closed(&mut self, shape: &BezPath, t: Affine) {
        let mut first = None;
        for el in shape.elements() {
            let el = t * *el;
            if first.is_none()
                && let PathEl::MoveTo(p) = el
            {
                first = Some(p);
            }
            self.path.push(el);
        }
        if let Some(p) = first {
            self.start = Some(p);
            self.current = Some(p);
        }
    }
}

/// The sweep of an arc from `a0` to `a1`, as the HTML canvas works it out.
fn sweep(a0: f64, a1: f64, ccw: bool) -> f64 {
    use std::f64::consts::TAU;
    if !ccw && a1 - a0 >= TAU {
        TAU
    } else if ccw && a0 - a1 >= TAU {
        -TAU
    } else if ccw {
        let s = (a0 - a1).rem_euclid(TAU);
        -s
    } else {
        (a1 - a0).rem_euclid(TAU)
    }
}

/// A rectangle with elliptical corners `rx` by `ry`.
fn rounded_rect(rect: Rect, rx: f64, ry: f64) -> BezPath {
    use std::f64::consts::FRAC_PI_2;
    let r = rect.abs();
    let rx = rx.abs().min(r.width() / 2.0);
    let ry = ry.abs().min(r.height() / 2.0);
    if rx <= 0.0 || ry <= 0.0 {
        return r.to_path(0.1);
    }
    let mut p = BezPath::new();
    p.move_to((r.x0 + rx, r.y0));
    let corners = [
        (Point::new(r.x1 - rx, r.y0 + ry), -FRAC_PI_2),
        (Point::new(r.x1 - rx, r.y1 - ry), 0.0),
        (Point::new(r.x0 + rx, r.y1 - ry), FRAC_PI_2),
        (Point::new(r.x0 + rx, r.y0 + ry), FRAC_PI_2 * 2.0),
    ];
    for (centre, start) in corners {
        let arc = kurbo::Arc::new(centre, Vec2::new(rx, ry), start, FRAC_PI_2, 0.0);
        let s = centre + Vec2::new(rx * start.cos(), ry * start.sin());
        p.line_to(s);
        arc.append_iter(ARC_TOLERANCE).for_each(|el| p.push(el));
    }
    p.close_path();
    p
}

/// An even-odd shape as one a non-zero rule fills the same: its flattened
/// subpaths, each turned so that nested ones alternate.
fn even_odd_as_non_zero(path: &BezPath) -> BezPath {
    // Simple subpaths that do not cross each other are enough for the
    // clips scripts draw (rings, holes).
    let mut subpaths: Vec<BezPath> = Vec::new();
    let mut current = BezPath::new();
    kurbo::flatten(path.elements().iter().copied(), 0.1, |el| {
        if matches!(el, PathEl::MoveTo(_)) && !current.elements().is_empty() {
            subpaths.push(std::mem::take(&mut current));
        }
        current.push(el);
    });
    if !current.elements().is_empty() {
        subpaths.push(current);
    }
    let mut out = BezPath::new();
    for (i, sp) in subpaths.iter().enumerate() {
        let inside_of = subpaths
            .iter()
            .enumerate()
            .filter(|(j, other)| {
                *j != i
                    && sp
                        .elements()
                        .first()
                        .and_then(|e| e.end_point())
                        .is_some_and(|p| other.winding(p) != 0)
            })
            .count();
        let want_positive = inside_of % 2 == 0;
        let positive = sp.area() >= 0.0;
        if want_positive == positive {
            out.extend(sp.elements().iter().copied());
        } else {
            out.extend(sp.reverse_subpaths().elements().iter().copied());
        }
    }
    out
}

/// The pixels of a hatching cell in `color`.
fn hatch(color: Color, cell: [&str; 8]) -> Option<ImageHandle> {
    let ink = color.to_rgba8();
    let bytes: Vec<u8> = cell
        .iter()
        .flat_map(|row| row.bytes())
        .flat_map(|b| if b == b'#' { ink } else { [0; 4] })
        .collect();
    ImageHandle::from_straight(8, 8, bytes).ok()
}

fn state(gs: &Gs) -> DrawState<'_> {
    DrawState {
        transform: gs.transform,
        alpha: gs.alpha as f32,
        blend: gs.blend,
        clips: &gs.clips,
        shadow: shadow_of(gs),
    }
}

/// The shadow the state casts, if any. As measured on qml6, and unlike
/// the HTML canvas, the offsets and the blur go through the transform: a
/// scaled drawing casts its shadow further and blurrier.
fn shadow_of(gs: &Gs) -> Option<CanvasShadow> {
    let color = to_rgba(gs.shadow_color);
    let offset = Vec2::new(gs.shadow_x, gs.shadow_y);
    if color.a <= 0.0 || (gs.shadow_blur <= 0.0 && offset == Vec2::ZERO) {
        return None;
    }
    let [a, b, c, d, _, _] = gs.transform.as_coeffs();
    let offset = Vec2::new(a * offset.x + c * offset.y, b * offset.x + d * offset.y);
    let scale = gs.transform.determinant().abs().sqrt();
    Some(CanvasShadow {
        color,
        offset,
        sigma: blur_sigma(gs.shadow_blur) * scale,
    })
}

/// The standard deviation of the shadow's blur for a `shadowBlur` of
/// `blur`, fitted to qml6's pixels: it grows in steps of two, by a little
/// more than a pixel a step (2 gives 1.5, 4 gives 2.6, 10 gives 5.7, 30
/// gives 16.2).
fn blur_sigma(blur: f64) -> f64 {
    if blur <= 0.0 {
        return 0.0;
    }
    1.04 * (blur / 2.0).ceil() + 0.5
}

/// What `style` draws with in the state `gs`. A colour takes
/// `globalAlpha` in 256ths as Qt's painter does (an opaque colour at 0.5
/// is 127, measured), so it is folded into the colour.
fn with_style<'a>(gs: &'a Gs, style: &Style) -> Option<(CanvasPaint, DrawState<'a>)> {
    let mut st = state(gs);
    if let Style::Color(c) = style {
        let [r, g, b, a] = c.to_rgba8();
        let alpha = (u32::from(a) * (gs.alpha * 256.0).round() as u32) >> 8;
        let c = Color::from_rgba8(r, g, b, alpha.min(255) as u8);
        st.alpha = 1.0;
        return Some((CanvasPaint::Brush(Brush::Solid(to_rgba(c))), st));
    }
    if let Style::Hatch(_) = style {
        // A hatching is a brush of Qt's painter too (0.5 is 127, measured).
        let alpha = (255 * (gs.alpha * 256.0).round() as u32) >> 8;
        st.alpha = alpha as f32 / 255.0;
    }
    Some((style.paint()?, st))
}

/// The pen: dash lengths and their offset are in line widths, as in Qt
/// (measured: `[10, 10]` at width 2 is 20 on, 20 off).
fn stroke_of(gs: &Gs) -> Stroke {
    let mut s = Stroke::new(gs.line_width)
        .with_caps(gs.cap)
        .with_join(gs.join)
        .with_miter_limit(gs.miter);
    if !gs.dash.is_empty() && gs.dash.iter().any(|d| *d > 0.0) {
        let w = gs.line_width;
        s = s.with_dashes(gs.dash_offset * w, gs.dash.iter().map(|d| d * w));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sweeps_follow_the_html_canvas() {
        use std::f64::consts::{FRAC_PI_2, PI, TAU};
        assert!((sweep(0.0, FRAC_PI_2, false) - FRAC_PI_2).abs() < 1e-9);
        assert!((sweep(0.0, FRAC_PI_2, true) + 3.0 * FRAC_PI_2).abs() < 1e-9);
        assert!((sweep(0.0, 3.0 * PI, false) - TAU).abs() < 1e-9);
        assert!((sweep(0.0, -3.0 * PI, true) + TAU).abs() < 1e-9);
    }

    #[test]
    fn rounded_corners_leave_the_corner_empty() {
        let p = rounded_rect(Rect::new(0.0, 0.0, 40.0, 40.0), 10.0, 10.0);
        assert_eq!(p.winding(Point::new(1.0, 1.0)), 0);
        assert_ne!(p.winding(Point::new(5.0, 5.0)), 0);
        assert_ne!(p.winding(Point::new(20.0, 1.0)), 0);
    }

    #[test]
    fn a_shadow_takes_the_transform() {
        let mut gs = Gs::default();
        assert!(shadow_of(&gs).is_none());
        gs.shadow_x = 10.0;
        gs.shadow_blur = 4.0;
        gs.transform = Affine::translate((50.0, 0.0)) * Affine::rotate(std::f64::consts::FRAC_PI_2);
        let s = shadow_of(&gs).expect("a shadow");
        assert!(s.offset.x.abs() < 1e-9 && (s.offset.y - 10.0).abs() < 1e-9);
        assert!((s.sigma - blur_sigma(4.0)).abs() < 1e-9);
        gs.transform = Affine::scale(2.0);
        let s = shadow_of(&gs).expect("a shadow");
        assert_eq!(s.offset, Vec2::new(20.0, 0.0));
        assert!((s.sigma - 2.0 * blur_sigma(4.0)).abs() < 1e-9);
        gs.shadow_color = Color::from_rgba8(0, 0, 0, 0);
        assert!(shadow_of(&gs).is_none());
    }

    #[test]
    fn a_hatch_is_its_cell_in_the_colour() {
        let cell = hatch_cell(9).expect("HorPattern");
        let image = hatch(Color::from_rgba8(0, 255, 0, 255), cell).expect("an image");
        assert_eq!((image.width(), image.height()), (8, 8));
        assert!(hatch_cell(1).is_none() && hatch_cell(15).is_none());
    }
}
