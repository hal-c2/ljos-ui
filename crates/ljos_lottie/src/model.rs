//! A Lottie file read into what drawing needs.
//!
//! The JSON is walked by hand rather than derived: the format writes the
//! same property as a number, an array, a keyframe list or an object
//! depending on the exporter and its age, and an unknown or broken part
//! should drop that part, not the whole file. Only a file that is not
//! JSON, or not an animation at all, is an error.

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::{Map, Value};

use crate::anim::{Anim, Blend, Ease, Key};

/// Why a file could not be read as an animation.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    /// Not JSON; Qt's warning names the parser's complaint the same way.
    #[error("JSON parse error: \"{0}\"")]
    Json(String),
    /// JSON that lacks what every animation has.
    #[error("not a Lottie animation: no {0}")]
    NotAnimation(&'static str),
}

/// A number property: one or more components.
pub type Prop = Anim<Vec<f64>>;

/// The whole animation.
#[derive(Debug)]
pub struct Composition {
    pub width: f64,
    pub height: f64,
    pub frame_rate: f64,
    pub in_point: f64,
    pub out_point: f64,
    pub layers: Arc<[Layer]>,
    /// Compositions layers can hold, by id.
    pub assets: HashMap<String, Arc<[Layer]>>,
    pub markers: Vec<Marker>,
    /// What was read but is not drawn, for the warning a host may log.
    pub warnings: Vec<String>,
}

/// A named frame.
#[derive(Clone, Debug, PartialEq)]
pub struct Marker {
    pub name: String,
    pub frame: f64,
}

#[derive(Debug)]
pub struct Layer {
    pub kind: LayerKind,
    /// The layer's number, which `parent` names.
    pub index: Option<i64>,
    pub parent: Option<i64>,
    pub in_point: f64,
    pub out_point: f64,
    pub start_time: f64,
    pub stretch: f64,
    pub transform: Transform,
    /// Drawn only through the layer above it (a track matte), or hidden.
    pub hidden: bool,
}

#[derive(Debug)]
pub enum LayerKind {
    Shapes(Vec<Shape>),
    Precomp {
        id: String,
        /// Time remapping, in seconds.
        remap: Option<Prop>,
    },
    Solid {
        color: [f64; 4],
        width: f64,
        height: f64,
    },
    /// A null layer, or one of a kind not drawn yet (image, text).
    Empty,
}

#[derive(Debug, Default)]
pub struct Transform {
    pub anchor: Prop,
    pub position: Position,
    pub scale: Prop,
    pub rotation: Prop,
    pub opacity: Prop,
    pub skew: Prop,
    pub skew_axis: Prop,
}

#[derive(Debug)]
pub enum Position {
    Joined(Prop),
    Split(Prop, Prop),
}

impl Default for Position {
    fn default() -> Self {
        Position::Joined(Prop::default())
    }
}

/// The outline of a free path.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Bez {
    pub closed: bool,
    pub vertices: Vec<[f64; 2]>,
    pub in_tangents: Vec<[f64; 2]>,
    pub out_tangents: Vec<[f64; 2]>,
}

impl Blend for Bez {
    fn blend(&self, to: &Self, progress: &dyn Fn(usize) -> f64) -> Self {
        let u = progress(0);
        let mix = |a: &[[f64; 2]], b: &[[f64; 2]]| -> Vec<[f64; 2]> {
            a.iter()
                .enumerate()
                .map(|(i, p)| {
                    let q = b.get(i).unwrap_or(p);
                    [p[0] + (q[0] - p[0]) * u, p[1] + (q[1] - p[1]) * u]
                })
                .collect()
        };
        Bez {
            closed: self.closed,
            vertices: mix(&self.vertices, &to.vertices),
            in_tangents: mix(&self.in_tangents, &to.in_tangents),
            out_tangents: mix(&self.out_tangents, &to.out_tangents),
        }
    }
}

#[derive(Debug)]
pub struct Gradient {
    /// `true` for radial.
    pub radial: bool,
    pub start: Prop,
    pub end: Prop,
    /// The radial focus: how far towards the edge, in percent, and at
    /// what angle.
    pub highlight: Prop,
    pub angle: Prop,
    /// How many colour stops lead `colors`; opacity stops follow.
    pub count: usize,
    pub colors: Prop,
}

#[derive(Debug)]
pub struct StrokeStyle {
    pub width: Prop,
    pub cap: u8,
    pub join: u8,
    pub miter: f64,
    /// Dash lengths and gaps, alternating, then the offset last.
    pub dashes: Vec<(DashKind, Prop)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DashKind {
    Dash,
    Gap,
    Offset,
}

#[derive(Debug)]
pub enum Paint {
    Color(Prop),
    Gradient(Gradient),
}

#[derive(Debug)]
pub enum Shape {
    Group(Vec<Shape>),
    Rect {
        position: Prop,
        size: Prop,
        roundness: Prop,
    },
    Ellipse {
        position: Prop,
        size: Prop,
    },
    Path(Anim<Bez>),
    Star {
        star: bool,
        points: Prop,
        position: Prop,
        rotation: Prop,
        outer: Prop,
        inner: Prop,
    },
    Fill {
        paint: Paint,
        opacity: Prop,
        even_odd: bool,
    },
    Stroke {
        paint: Paint,
        opacity: Prop,
        style: StrokeStyle,
    },
    Transform(Transform),
    Trim {
        start: Prop,
        end: Prop,
        offset: Prop,
        /// Trimmed one after the other, as one long path.
        sequential: bool,
    },
    /// A kind not drawn yet (repeater, merge, round corners).
    Other,
}

impl Shape {
    pub fn is_geometry(&self) -> bool {
        matches!(
            self,
            Shape::Rect { .. } | Shape::Ellipse { .. } | Shape::Path(_) | Shape::Star { .. }
        )
    }
}

/// Reads `bytes` as a Lottie animation.
pub fn parse(bytes: &[u8]) -> Result<Composition, LoadError> {
    let root: Value = serde_json::from_slice(bytes).map_err(|e| LoadError::Json(e.to_string()))?;
    let root = root.as_object().ok_or(LoadError::NotAnimation("object"))?;
    let num = |k: &'static str| {
        root.get(k)
            .and_then(Value::as_f64)
            .ok_or(LoadError::NotAnimation(k))
    };
    let (width, height) = (num("w")?, num("h")?);
    let (frame_rate, in_point, out_point) = (num("fr")?, num("ip")?, num("op")?);
    let layers_json = root
        .get("layers")
        .and_then(Value::as_array)
        .ok_or(LoadError::NotAnimation("layers"))?;
    let mut warnings = Vec::new();
    let layers = layers_of(layers_json, &mut warnings);
    let mut assets = HashMap::new();
    for asset in root
        .get("assets")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let (Some(id), Some(layers)) = (
            asset.get("id").and_then(Value::as_str),
            asset.get("layers").and_then(Value::as_array),
        ) else {
            if asset.get("p").is_some() {
                warnings.push("image assets are not drawn".to_owned());
            }
            continue;
        };
        assets.insert(id.to_owned(), layers_of(layers, &mut warnings));
    }
    let markers = root
        .get("markers")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|m| {
            Some(Marker {
                name: m.get("cm")?.as_str()?.to_owned(),
                frame: m.get("tm")?.as_f64()?,
            })
        })
        .collect();
    Ok(Composition {
        width,
        height,
        frame_rate,
        in_point,
        out_point,
        layers,
        assets,
        markers,
        warnings,
    })
}

fn layers_of(list: &[Value], warnings: &mut Vec<String>) -> Arc<[Layer]> {
    list.iter()
        .filter_map(Value::as_object)
        .map(|l| layer(l, warnings))
        .collect()
}

fn f(o: &Map<String, Value>, k: &str, or: f64) -> f64 {
    o.get(k).and_then(Value::as_f64).unwrap_or(or)
}

fn flag(o: &Map<String, Value>, k: &str) -> bool {
    match o.get(k) {
        Some(Value::Bool(b)) => *b,
        Some(v) => v.as_f64().is_some_and(|n| n != 0.0),
        None => false,
    }
}

fn layer(o: &Map<String, Value>, warnings: &mut Vec<String>) -> Layer {
    let ty = o.get("ty").and_then(Value::as_i64).unwrap_or(-1);
    let kind = match ty {
        4 => LayerKind::Shapes(shapes(o.get("shapes"), warnings)),
        0 => LayerKind::Precomp {
            id: o
                .get("refId")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_owned(),
            remap: o.get("tm").map(prop),
        },
        1 => LayerKind::Solid {
            color: o
                .get("sc")
                .and_then(Value::as_str)
                .and_then(hex_color)
                .unwrap_or([0.0, 0.0, 0.0, 1.0]),
            width: f(o, "sw", 0.0),
            height: f(o, "sh", 0.0),
        },
        3 => LayerKind::Empty,
        2 => {
            warnings.push("image layers are not drawn".to_owned());
            LayerKind::Empty
        }
        5 => {
            warnings.push("text layers are not drawn".to_owned());
            LayerKind::Empty
        }
        _ => LayerKind::Empty,
    };
    if o.get("masksProperties")
        .is_some_and(|m| m.as_array().is_some_and(|a| !a.is_empty()))
    {
        warnings.push("masks are not applied".to_owned());
    }
    if o.get("tt").is_some() {
        warnings.push("track mattes are not applied".to_owned());
    }
    Layer {
        kind,
        index: o.get("ind").and_then(Value::as_i64),
        parent: o.get("parent").and_then(Value::as_i64),
        in_point: f(o, "ip", 0.0),
        out_point: f(o, "op", f64::INFINITY),
        start_time: f(o, "st", 0.0),
        stretch: f(o, "sr", 1.0).max(1e-6),
        transform: o
            .get("ks")
            .and_then(Value::as_object)
            .map(transform)
            .unwrap_or_default(),
        // A matte source is drawn only through the layer it masks.
        hidden: flag(o, "hd") || flag(o, "td"),
    }
}

fn hex_color(s: &str) -> Option<[f64; 4]> {
    let s = s.strip_prefix('#')?;
    let v = u32::from_str_radix(s.get(..6)?, 16).ok()?;
    let c = |shift: u32| f64::from((v >> shift) & 0xff) / 255.0;
    Some([c(16), c(8), c(0), 1.0])
}

fn transform(o: &Map<String, Value>) -> Transform {
    let get = |k: &str, or: &[f64]| o.get(k).map(prop).unwrap_or(Anim::Static(or.to_vec()));
    let position = match o.get("p").and_then(Value::as_object) {
        Some(p) if flag(p, "s") => Position::Split(
            p.get("x").map(prop).unwrap_or_default(),
            p.get("y").map(prop).unwrap_or_default(),
        ),
        _ => Position::Joined(get("p", &[0.0, 0.0])),
    };
    Transform {
        anchor: get("a", &[0.0, 0.0]),
        position,
        scale: get("s", &[100.0, 100.0]),
        // A 3D layer turns about z with `rz`.
        rotation: o
            .get("r")
            .or_else(|| o.get("rz"))
            .map(prop)
            .unwrap_or(Anim::Static(vec![0.0])),
        opacity: get("o", &[100.0]),
        skew: get("sk", &[0.0]),
        skew_axis: get("sa", &[0.0]),
    }
}

fn shapes(list: Option<&Value>, warnings: &mut Vec<String>) -> Vec<Shape> {
    list.and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_object)
        .filter(|s| !flag(s, "hd"))
        .map(|s| shape(s, warnings))
        .collect()
}

fn shape(o: &Map<String, Value>, warnings: &mut Vec<String>) -> Shape {
    let get = |k: &str| o.get(k).map(prop).unwrap_or_default();
    let int = |k: &str, or: u8| {
        o.get(k)
            .and_then(Value::as_u64)
            .and_then(|v| u8::try_from(v).ok())
            .unwrap_or(or)
    };
    match o.get("ty").and_then(Value::as_str).unwrap_or_default() {
        "gr" => Shape::Group(shapes(o.get("it"), warnings)),
        "rc" => Shape::Rect {
            position: get("p"),
            size: get("s"),
            roundness: get("r"),
        },
        "el" => Shape::Ellipse {
            position: get("p"),
            size: get("s"),
        },
        "sh" => Shape::Path(o.get("ks").map(bez_prop).unwrap_or_default()),
        "sr" => Shape::Star {
            star: int("sy", 1) == 1,
            points: get("pt"),
            position: get("p"),
            rotation: get("r"),
            outer: get("or"),
            inner: get("ir"),
        },
        "fl" => Shape::Fill {
            paint: Paint::Color(get("c")),
            opacity: o.get("o").map(prop).unwrap_or(Anim::Static(vec![100.0])),
            even_odd: int("r", 1) == 2,
        },
        "gf" => Shape::Fill {
            paint: Paint::Gradient(gradient(o)),
            opacity: o.get("o").map(prop).unwrap_or(Anim::Static(vec![100.0])),
            even_odd: int("r", 1) == 2,
        },
        "st" | "gs" => Shape::Stroke {
            paint: if o.get("ty").and_then(Value::as_str) == Some("gs") {
                Paint::Gradient(gradient(o))
            } else {
                Paint::Color(get("c"))
            },
            opacity: o.get("o").map(prop).unwrap_or(Anim::Static(vec![100.0])),
            style: StrokeStyle {
                width: get("w"),
                cap: int("lc", 2),
                join: int("lj", 2),
                miter: f(o, "ml", 4.0),
                dashes: o
                    .get("d")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(|d| {
                        let kind = match d.get("n")?.as_str()? {
                            "d" => DashKind::Dash,
                            "g" => DashKind::Gap,
                            "o" => DashKind::Offset,
                            _ => return None,
                        };
                        Some((kind, prop(d.get("v")?)))
                    })
                    .collect(),
            },
        },
        "tr" => Shape::Transform(transform(o)),
        "tm" => Shape::Trim {
            start: get("s"),
            end: o.get("e").map(prop).unwrap_or(Anim::Static(vec![100.0])),
            offset: get("o"),
            sequential: int("m", 1) == 2,
        },
        other => {
            warnings.push(format!("shapes of type {other:?} are not drawn"));
            Shape::Other
        }
    }
}

fn gradient(o: &Map<String, Value>) -> Gradient {
    let get = |k: &str| o.get(k).map(prop).unwrap_or_default();
    let g = o.get("g").and_then(Value::as_object);
    Gradient {
        radial: o.get("t").and_then(Value::as_i64) == Some(2),
        start: get("s"),
        end: get("e"),
        highlight: get("h"),
        angle: get("a"),
        count: g
            .and_then(|g| g.get("p"))
            .and_then(Value::as_u64)
            .map_or(0, |n| n as usize),
        colors: g.and_then(|g| g.get("k")).map(prop).unwrap_or_default(),
    }
}

/// Numbers as a list: a number is one component.
fn numbers(v: &Value) -> Option<Vec<f64>> {
    match v {
        Value::Number(n) => Some(vec![n.as_f64()?]),
        Value::Array(a) => a.iter().map(Value::as_f64).collect(),
        _ => None,
    }
}

fn ease(v: Option<&Value>) -> Ease {
    let Some(o) = v.and_then(Value::as_object) else {
        return Ease::default();
    };
    Ease {
        x: o.get("x").and_then(numbers).unwrap_or_default(),
        y: o.get("y").and_then(numbers).unwrap_or_default(),
    }
}

fn pair(v: Option<&Value>) -> Option<[f64; 2]> {
    let n = numbers(v?)?;
    Some([*n.first()?, *n.get(1)?])
}

/// Whether `k` is a keyframe list rather than a value.
fn keyframes(o: &Map<String, Value>) -> Option<&Vec<Value>> {
    let k = o.get("k")?.as_array()?;
    let keyed = match o.get("a") {
        Some(a) => flag_value(a),
        None => k.first().is_some_and(|f| f.get("t").is_some()),
    };
    keyed.then_some(k).filter(|k| !k.is_empty())
}

fn flag_value(v: &Value) -> bool {
    match v {
        Value::Bool(b) => *b,
        v => v.as_f64().is_some_and(|n| n != 0.0),
    }
}

/// A keyed or static property, its values read by `value`.
fn anim<T: Blend>(v: &Value, value: impl Fn(&Value) -> Option<T>, or: T) -> Anim<T> {
    let Some(o) = v.as_object() else {
        return Anim::Static(value(v).unwrap_or(or));
    };
    let Some(list) = keyframes(o) else {
        return Anim::Static(o.get("k").and_then(&value).unwrap_or(or));
    };
    let mut keys: Vec<Key<T>> = Vec::with_capacity(list.len());
    for k in list {
        let t = k.get("t").and_then(Value::as_f64).unwrap_or(0.0);
        let start = k.get("s").and_then(&value);
        let end = k.get("e").and_then(&value);
        // A key without a value starts where the one before it ended.
        let Some(start) = start.or_else(|| {
            keys.last()
                .map(|p| p.end.clone().unwrap_or_else(|| p.start.clone()))
        }) else {
            continue;
        };
        keys.push(Key {
            t,
            start,
            end,
            hold: k.get("h").is_some_and(flag_value),
            out_ease: ease(k.get("o")),
            in_ease: ease(k.get("i")),
            spatial: pair(k.get("to")).zip(pair(k.get("ti"))),
        });
    }
    if keys.is_empty() {
        return Anim::Static(or);
    }
    Anim::Keyed(keys)
}

/// A number property.
pub fn prop(v: &Value) -> Prop {
    anim(v, numbers, Vec::new())
}

fn bez_prop(v: &Value) -> Anim<Bez> {
    // A keyframe's value is the outline wrapped in an array of one.
    anim(
        v,
        |v| bez(v.as_array().and_then(|a| a.first()).unwrap_or(v)),
        Bez::default(),
    )
}

fn bez(v: &Value) -> Option<Bez> {
    let o = v.as_object()?;
    let points = |k: &str| -> Vec<[f64; 2]> {
        o.get(k)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|p| pair(Some(p)).unwrap_or([0.0, 0.0]))
            .collect()
    };
    Some(Bez {
        closed: o.get("c").is_some_and(flag_value),
        vertices: points("v"),
        in_tangents: points("i"),
        out_tangents: points("o"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn not_json_is_a_parse_error() {
        let err = parse(br#"{"v":"5.7.0","fr":10,"#).unwrap_err();
        assert!(matches!(err, LoadError::Json(_)));
        assert!(err.to_string().starts_with("JSON parse error"));
    }

    #[test]
    fn json_that_is_no_animation_is_refused() {
        assert!(matches!(parse(b"{}"), Err(LoadError::NotAnimation(_))));
        assert!(matches!(parse(b"[1, 2]"), Err(LoadError::NotAnimation(_))));
    }

    #[test]
    fn an_animation_without_layers_is_still_one() {
        let c = parse(br#"{"fr":30,"ip":0,"op":60,"w":10,"h":20,"layers":[]}"#).unwrap();
        assert_eq!((c.width, c.height, c.frame_rate), (10.0, 20.0, 30.0));
        assert!(c.layers.is_empty());
    }

    #[test]
    fn values_are_read_in_every_shape_the_format_allows() {
        let num = prop(&serde_json::json!({"a": 0, "k": 5}));
        assert_eq!(num, Anim::Static(vec![5.0]));
        let bare = prop(&serde_json::json!([1, 2]));
        assert_eq!(bare, Anim::Static(vec![1.0, 2.0]));
        let keyed = prop(&serde_json::json!({"k": [{"t": 0, "s": [0]}, {"t": 10, "s": [10]}]}));
        assert_eq!(keyed.num(5.0, 0, -1.0), 5.0);
        let old = prop(&serde_json::json!({"a": 1, "k": [{"t": 0, "s": [0], "e": [4]}, {"t": 4}]}));
        assert_eq!(old.num(2.0, 0, -1.0), 2.0);
        assert_eq!(old.num(9.0, 0, -1.0), 4.0);
    }

    #[test]
    fn markers_and_assets_are_kept() {
        let c = parse(
            br#"{"fr":10,"ip":0,"op":20,"w":100,"h":100,
                "markers":[{"tm":5,"cm":"middle","dr":0}],
                "assets":[{"id":"inner","layers":[]}],"layers":[]}"#,
        )
        .unwrap();
        assert_eq!(c.markers[0].name, "middle");
        assert!(c.assets.contains_key("inner"));
    }
}
