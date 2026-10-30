//! `RandomInstancing` and `InstanceRange` of `QtQuick3D.Helpers`: a table
//! of copies picked at random within ranges.
//!
//! The table is made again only when what it is made from changes, so a
//! view walked for another reason keeps the copies where they were, even
//! with no seed.

use std::sync::atomic::{AtomicU64, Ordering};

use ljos::item;
use ljos::item::ljos_3d::math::{Quat, Vec3};
use ljos::prelude::*;

use crate::Object3D;
use crate::instancing::{Entry, Instancing};

/// `InstanceRange`: the bounds an attribute of a random table is picked
/// within, of the attribute's type.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick3D.Helpers", name = "InstanceRange", extends = Object3D)]
pub struct InstanceRange {
    #[qml(base)]
    pub object3d: Object3D,
    #[qml(notify)]
    pub from: Value,
    #[qml(notify)]
    pub to: Value,
    /// Every component moves by the same share of its range: a scale
    /// stays uniform. The documentation says true is the default.
    #[qml(notify)]
    pub proportional: bool,
}

impl Default for InstanceRange {
    fn default() -> Self {
        InstanceRange {
            object3d: Object3D::default(),
            from: Value::Undefined,
            to: Value::Undefined,
            proportional: true,
        }
    }
}

pub(crate) mod color_model {
    pub const RGB: i32 = 0;
    pub const HSV: i32 = 1;
    pub const HSL: i32 = 2;
}

fn random_enums() -> Vec<(&'static str, Vec<(&'static str, i64)>)> {
    vec![(
        "ColorModel",
        vec![
            ("RGB", color_model::RGB.into()),
            ("HSV", color_model::HSV.into()),
            ("HSL", color_model::HSL.into()),
        ],
    )]
}

/// `RandomInstancing`: `instanceCount` copies, each attribute picked
/// within its range; an attribute without one keeps its default.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick3D.Helpers",
    name = "RandomInstancing",
    extends = Instancing,
    enums = random_enums
)]
pub struct RandomInstancing {
    #[qml(base)]
    pub instancing: Instancing,
    #[qml(notify)]
    pub instance_count: i32,
    /// -1 picks another table each time it is made.
    #[qml(notify)]
    pub random_seed: i32,
    #[qml(notify, object = "InstanceRange")]
    pub position: Option<ObjectId>,
    #[qml(notify, object = "InstanceRange")]
    pub scale: Option<ObjectId>,
    #[qml(notify, object = "InstanceRange")]
    pub rotation: Option<ObjectId>,
    #[qml(notify, object = "InstanceRange")]
    pub color: Option<ObjectId>,
    #[qml(notify, object = "InstanceRange")]
    pub custom_data: Option<ObjectId>,
    #[qml(notify, enumeration = "ColorModel")]
    pub color_model: i32,
    #[qml(skip)]
    made: Option<(Vec<Value>, Vec<Entry>)>,
}

impl Default for RandomInstancing {
    fn default() -> Self {
        RandomInstancing {
            instancing: Instancing::default(),
            instance_count: 0,
            random_seed: -1,
            position: None,
            scale: None,
            rotation: None,
            color: None,
            custom_data: None,
            color_model: color_model::RGB,
            made: None,
        }
    }
}

#[qml_impl]
impl RandomInstancing {}

/// A range as read: its bounds and whether it is proportional.
struct Range {
    from: Value,
    to: Value,
    proportional: bool,
}

fn range(e: &Engine, obj: ObjectId, p: Prop<Option<ObjectId>>) -> Option<Range> {
    let r = item::prop(e, obj, p).filter(|r| e.is::<InstanceRange>(*r))?;
    Some(Range {
        from: e.read(r, InstanceRange::FROM.index),
        to: e.read(r, InstanceRange::TO.index),
        proportional: item::prop(e, r, InstanceRange::PROPORTIONAL),
    })
}

/// The entries of the random table `obj`. Tracked.
pub(crate) fn entries(e: &Engine, obj: ObjectId) -> Option<Vec<Entry>> {
    if !e.is::<RandomInstancing>(obj) {
        return None;
    }
    let count = item::prop(e, obj, RandomInstancing::INSTANCE_COUNT).max(0) as usize;
    let seed = item::prop(e, obj, RandomInstancing::RANDOM_SEED);
    let model = item::prop(e, obj, RandomInstancing::COLOR_MODEL);
    let ranges = [
        RandomInstancing::POSITION,
        RandomInstancing::SCALE,
        RandomInstancing::ROTATION,
        RandomInstancing::COLOR,
        RandomInstancing::CUSTOM_DATA,
    ]
    .map(|p| range(e, obj, p));
    let mut key = vec![
        Value::Int(count as i32),
        Value::Int(seed),
        Value::Int(model),
    ];
    for r in ranges.iter() {
        match r {
            Some(r) => key.extend([r.from.clone(), r.to.clone(), Value::Bool(r.proportional)]),
            None => key.push(Value::Null),
        }
    }
    let kept = e
        .with(obj, |r: &RandomInstancing| {
            r.made
                .as_ref()
                .filter(|(k, _)| *k == key)
                .map(|(_, entries)| entries.clone())
        })
        .ok()
        .flatten();
    if let Some(entries) = kept {
        return Some(entries);
    }
    let mut rng = Rng::new(seed);
    let [position, scale, rotation, color, custom] = &ranges;
    let entries: Vec<Entry> = (0..count)
        .map(|_| {
            let mut entry = Entry::default();
            if let Some(r) = position {
                entry.position = vec3_in(&mut rng, r, Vec3::ZERO);
            }
            if let Some(r) = scale {
                entry.scale = vec3_in(&mut rng, r, Vec3::ONE);
            }
            if let Some(r) = rotation {
                entry.rotation = Quat::from_euler_degrees(vec3_in(&mut rng, r, Vec3::ZERO));
            }
            if let Some(r) = color {
                entry.color = color_in(&mut rng, r, model);
            }
            if let Some(r) = custom {
                entry.custom = numbers_in(&mut rng, r);
            }
            entry
        })
        .collect();
    let _ = e.untrack(|| {
        e.with_mut(obj, |r: &mut RandomInstancing| {
            r.made = Some((key, entries.clone()));
        })
    });
    Some(entries)
}

/// The four numbers of a bound: a vector's parts, a colour's or a
/// number's; zeros for anything else.
fn parts(v: &Value) -> [f64; 4] {
    match v {
        Value::Vector3d(v) => [v.x, v.y, v.z, 0.0].map(f64::from),
        Value::Vector4d(v) => [v.x, v.y, v.z, v.w].map(f64::from),
        Value::Vector2d(v) => [v.x, v.y, 0.0, 0.0].map(f64::from),
        Value::Color(c) => c.to_rgba_f(),
        Value::Int(_) | Value::Real(_) => [v.to_number(); 4],
        _ => [0.0; 4],
    }
}

/// Each part picked between the bounds' parts: by one share for a
/// proportional range, by its own for another.
fn pick(rng: &mut Rng, r: &Range, from: [f64; 4], to: [f64; 4]) -> [f64; 4] {
    let one = rng.unit();
    std::array::from_fn(|i| {
        let t = if r.proportional { one } else { rng.unit() };
        from[i] + (to[i] - from[i]) * t
    })
}

fn vec3_in(rng: &mut Rng, r: &Range, otherwise: Vec3) -> Vec3 {
    if !bounded(r) {
        return otherwise;
    }
    let [x, y, z, _] = pick(rng, r, parts(&r.from), parts(&r.to));
    Vec3::new(x as f32, y as f32, z as f32)
}

fn numbers_in(rng: &mut Rng, r: &Range) -> [f32; 4] {
    if !bounded(r) {
        return [0.0; 4];
    }
    pick(rng, r, parts(&r.from), parts(&r.to)).map(|n| n as f32)
}

fn bounded(r: &Range) -> bool {
    !matches!(r.from, Value::Undefined | Value::Null)
        && !matches!(r.to, Value::Undefined | Value::Null)
}

fn color_of(v: &Value) -> Option<Color> {
    match v {
        Value::Color(c) => Some(*c),
        Value::String(s) => Color::parse(s),
        _ => None,
    }
}

/// A colour picked between two, part by part in the colour model.
fn color_in(rng: &mut Rng, r: &Range, model: i32) -> Color {
    let (Some(from), Some(to)) = (color_of(&r.from), color_of(&r.to)) else {
        return Entry::default().color;
    };
    let (into, back): (fn([f64; 4]) -> [f64; 4], fn([f64; 4]) -> [f64; 4]) = match model {
        color_model::HSV => (to_hsv, from_hsv),
        color_model::HSL => (to_hsl, from_hsl),
        _ => (|c| c, |c| c),
    };
    let [r_, g, b, a] = back(pick(rng, r, into(from.to_rgba_f()), into(to.to_rgba_f())));
    Color::from_rgba_f(
        r_.clamp(0.0, 1.0),
        g.clamp(0.0, 1.0),
        b.clamp(0.0, 1.0),
        a.clamp(0.0, 1.0),
    )
}

/// Hue in turns, then the rest from 0 to 1.
fn to_hsv([r, g, b, a]: [f64; 4]) -> [f64; 4] {
    let max = r.max(g).max(b);
    let d = max - r.min(g).min(b);
    let s = if max > 0.0 { d / max } else { 0.0 };
    [hue(r, g, b, max, d), s, max, a]
}

fn from_hsv([h, s, v, a]: [f64; 4]) -> [f64; 4] {
    let c = v * s;
    let [r, g, b] = from_hue(h, c, v - c);
    [r, g, b, a]
}

fn to_hsl([r, g, b, a]: [f64; 4]) -> [f64; 4] {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let l = (max + min) / 2.0;
    let s = if d == 0.0 {
        0.0
    } else {
        d / (1.0 - (2.0 * l - 1.0).abs())
    };
    [hue(r, g, b, max, d), s, l, a]
}

fn from_hsl([h, s, l, a]: [f64; 4]) -> [f64; 4] {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let [r, g, b] = from_hue(h, c, l - c / 2.0);
    [r, g, b, a]
}

fn hue(r: f64, g: f64, b: f64, max: f64, d: f64) -> f64 {
    if d == 0.0 {
        return 0.0;
    }
    let sixths = if max == r {
        ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    sixths / 6.0
}

fn from_hue(h: f64, c: f64, m: f64) -> [f64; 3] {
    let h6 = h.rem_euclid(1.0) * 6.0;
    let x = c * (1.0 - (h6 % 2.0 - 1.0).abs());
    let [r, g, b] = match h6 as u32 {
        0 => [c, x, 0.0],
        1 => [x, c, 0.0],
        2 => [0.0, c, x],
        3 => [0.0, x, c],
        4 => [x, 0.0, c],
        _ => [c, 0.0, x],
    };
    [r + m, g + m, b + m]
}

/// SplitMix64: small, and the same numbers for a seed everywhere.
struct Rng(u64);

impl Rng {
    fn new(seed: i32) -> Rng {
        if seed >= 0 {
            return Rng(seed as u64);
        }
        static NEXT: AtomicU64 = AtomicU64::new(0x2545_f491_4f6c_dd1d);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos() as u64);
        Rng(now ^ NEXT.fetch_add(0x9e37_79b9_7f4a_7c15, Ordering::Relaxed))
    }

    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// From 0 to 1.
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hsv_and_hsl_come_back_to_the_colour() {
        let c = [0.2, 0.6, 0.9, 0.5];
        for (into, back) in [
            (to_hsv as fn(_) -> _, from_hsv as fn(_) -> _),
            (to_hsl, from_hsl),
        ] {
            let got: [f64; 4] = back(into(c));
            for (a, b) in got.iter().zip(c) {
                assert!((a - b).abs() < 1e-9, "{got:?}");
            }
        }
    }

    #[test]
    fn a_seed_gives_the_same_numbers() {
        let (mut a, mut b) = (Rng::new(7), Rng::new(7));
        for _ in 0..10 {
            let n = a.unit();
            assert_eq!(n, b.unit());
            assert!((0.0..1.0).contains(&n));
        }
    }
}
