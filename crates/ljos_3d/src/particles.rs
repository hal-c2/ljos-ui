//! What moves particles: plain numbers in and out, no clock, no scene and no
//! QML, so that any embedding can drive it from its own frames.
//!
//! An emitter gives each particle it emits a start, a life, a place, a speed,
//! a turn, a size and a colour, and the kind of particle keeps them in a table
//! of fixed length. Where one is at a time is worked out from that and the
//! time alone ([`look`]), so a system shows the same at a time however it came
//! to it. Lines are the exception: they remember the way a particle came.
//!
//! Numbers of 32 bits are on purpose: a particle whose start is summed in them
//! may come out a hair after the moment it was emitted, and is then not there
//! until the time has moved on.

use std::sync::Arc;

use crate::math::{Mat4, Quat, Vec3};

/// What chance decides of a particle, each from a number of its own.
mod channel {
    pub const LIFE: u32 = 0;
    pub const SCALE: u32 = 1;
    pub const END_SCALE: u32 = 2;
    pub const TURN: u32 = 3;
    pub const SPIN: u32 = 6;
    pub const COLOR: u32 = 9;
    pub const VELOCITY: u32 = 13;
    pub const TARGET: u32 = 16;
    pub const MAGNITUDE: u32 = 19;
    pub const SHAPE: u32 = 20;
    pub const FRAME: u32 = 24;
    pub const LENGTH: u32 = 26;
    pub const WANDER: u32 = 27;
    pub const ATTRACT: u32 = 34;
    pub const AMOUNT: u32 = 38;
}

/// A number from nought up to one: the same whenever it is asked for with the
/// same seed, particle and purpose, so that a particle is the same particle at
/// whatever time it is looked at.
pub fn random(seed: u32, index: u32, channel: u32) -> f32 {
    let mut hash = seed
        ^ index.wrapping_add(1).wrapping_mul(0x9e37_79b1)
        ^ channel.wrapping_add(1).wrapping_mul(0x85eb_ca6b);
    hash = (hash ^ (hash >> 16)).wrapping_mul(0x7feb_352d);
    hash = (hash ^ (hash >> 15)).wrapping_mul(0x846c_a68b);
    hash ^= hash >> 16;
    (f64::from(hash) / 4_294_967_296.0) as f32
}

/// The same from minus one up to one.
pub fn spread(seed: u32, index: u32, channel: u32) -> f32 {
    random(seed, index, channel) * 2.0 - 1.0
}

fn vec3(a: [f32; 3]) -> Vec3 {
    Vec3::new(a[0], a[1], a[2])
}

/// Triangles to pick a place on, weighed by how much of the surface each is.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Surface {
    corners: Vec<[Vec3; 3]>,
    up_to: Vec<f32>,
    whole: f32,
}

impl Surface {
    /// From positions and, three to a triangle, indices into them; without
    /// indices the positions are the triangles' corners in order.
    pub fn new(positions: &[Vec3], indices: Option<&[u32]>) -> Surface {
        let mut s = Surface::default();
        let count = indices.map_or(positions.len(), <[u32]>::len) / 3;
        let at = |i: usize| match indices {
            Some(l) => positions.get(l[i] as usize).copied(),
            None => positions.get(i).copied(),
        };
        for t in 0..count {
            let (Some(a), Some(b), Some(c)) = (at(t * 3), at(t * 3 + 1), at(t * 3 + 2)) else {
                continue;
            };
            s.whole += (b - a).cross(c - a).length() / 2.0;
            s.corners.push([a, b, c]);
            s.up_to.push(s.whole);
        }
        s
    }

    fn pick(&self, seed: u32, index: u32, fill: bool) -> Option<Vec3> {
        if self.corners.is_empty() || self.whole <= 0.0 {
            return None;
        }
        let chance = |c| random(seed, index, channel::SHAPE + c);
        let wanted = chance(0) * self.whole;
        let t = self
            .up_to
            .partition_point(|u| *u < wanted)
            .min(self.corners.len() - 1);
        let root = chance(1).sqrt();
        let along = chance(2);
        let [a, b, c] = self.corners[t];
        let place = a * (1.0 - root) + b * (root * (1.0 - along)) + c * (root * along);
        Some(place * if fill { chance(3).cbrt() } else { 1.0 })
    }
}

/// Where a particle starts in its emitter or attractor.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Shape {
    /// Where the node is.
    #[default]
    Point,
    /// `extents` from its middle to its sides; `fill` is anywhere inside.
    Cube {
        extents: [f32; 3],
        fill: bool,
    },
    Sphere {
        extents: [f32; 3],
        fill: bool,
    },
    Cylinder {
        extents: [f32; 3],
        fill: bool,
    },
    /// A model's surface, `place` being where the model is in its node.
    Mesh {
        surface: std::sync::Arc<Surface>,
        place: Mat4,
        fill: bool,
    },
}

impl Shape {
    pub fn position(&self, seed: u32, index: u32) -> Vec3 {
        let chance = |c| random(seed, index, channel::SHAPE + c);
        let two_pi = std::f32::consts::TAU;
        let scaled = |p: [f32; 3], e: &[f32; 3]| Vec3::new(p[0] * e[0], p[1] * e[1], p[2] * e[2]);
        match self {
            Shape::Point => Vec3::ZERO,
            Shape::Sphere { extents, fill } => {
                let angle = chance(0) * two_pi;
                let height = chance(1) * 2.0 - 1.0;
                let round = (1.0 - height * height).max(0.0).sqrt();
                let far = if *fill { chance(2).cbrt() } else { 1.0 };
                scaled(
                    [
                        round * angle.cos() * far,
                        round * angle.sin() * far,
                        height * far,
                    ],
                    extents,
                )
            }
            Shape::Cylinder { extents, fill } => {
                let angle = chance(0) * two_pi;
                let far = if *fill { chance(2).sqrt() } else { 1.0 };
                scaled(
                    [angle.cos() * far, chance(1) * 2.0 - 1.0, angle.sin() * far],
                    extents,
                )
            }
            Shape::Cube { extents, fill } => {
                let mut p = [
                    chance(0) * 2.0 - 1.0,
                    chance(1) * 2.0 - 1.0,
                    chance(2) * 2.0 - 1.0,
                ];
                if !*fill {
                    let side = ((chance(3) * 6.0) as usize).min(5);
                    p[side >> 1] = if side & 1 == 1 { 1.0 } else { -1.0 };
                }
                scaled(p, extents)
            }
            Shape::Mesh {
                surface,
                place,
                fill,
            } => surface
                .pick(seed, index, *fill)
                .map_or(Vec3::ZERO, |p| place.transform_point(p)),
        }
    }
}

/// Which way a particle sets off, and how fast.
#[derive(Clone, Debug, PartialEq)]
pub enum Direction {
    /// The way told, give or take the variation along each axis.
    Vector {
        direction: [f32; 3],
        variation: [f32; 3],
        normalized: bool,
    },
    /// Towards a place: as fast as it is far, times the magnitude.
    Target {
        position: [f32; 3],
        variation: [f32; 3],
        normalized: bool,
        magnitude: f32,
        magnitude_variation: f32,
    },
}

impl Default for Direction {
    /// Without a direction a particle does not move.
    fn default() -> Self {
        Direction::Vector {
            direction: [0.0; 3],
            variation: [0.0; 3],
            normalized: false,
        }
    }
}

impl Direction {
    fn sample(&self, seed: u32, index: u32, from: Vec3) -> Vec3 {
        match self {
            Direction::Vector {
                direction,
                variation,
                normalized,
            } => {
                let way = Vec3::new(
                    direction[0] + variation[0] * spread(seed, index, channel::VELOCITY),
                    direction[1] + variation[1] * spread(seed, index, channel::VELOCITY + 1),
                    direction[2] + variation[2] * spread(seed, index, channel::VELOCITY + 2),
                );
                let long = way.length();
                if *normalized && long > 0.0 {
                    way * (vec3(*direction).length() / long)
                } else {
                    way
                }
            }
            Direction::Target {
                position,
                variation,
                normalized,
                magnitude,
                magnitude_variation,
            } => {
                let mut way = Vec3::new(
                    position[0] + variation[0] * spread(seed, index, channel::TARGET),
                    position[1] + variation[1] * spread(seed, index, channel::TARGET + 1),
                    position[2] + variation[2] * spread(seed, index, channel::TARGET + 2),
                ) - from;
                if *normalized && way.length() > 0.0 {
                    way = way.normalized();
                }
                way * (magnitude + magnitude_variation * spread(seed, index, channel::MAGNITUDE))
            }
        }
    }
}

/// How a `ScaleAffector3D` goes between its sizes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScaleCurve {
    #[default]
    Linear,
    SewSaw,
    SineWave,
    AbsSineWave,
    Step,
    SmoothStep,
}

fn smoothstep(v: f32) -> f32 {
    let w = v.clamp(0.0, 1.0);
    w * w * (3.0 - 2.0 * w)
}

/// What an application's affector runs.
pub type AffectFn = dyn Fn(u32, &Particle, &mut Current) + Send + Sync;

/// An affector an application brings: called with the seed, the particle
/// and what the affectors before it made of it. Two are the same only if they
/// are the one function, so a scene that rebuilds its list does not differ.
#[derive(Clone)]
pub struct CustomAffect(pub Arc<AffectFn>);

impl std::fmt::Debug for CustomAffect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CustomAffect")
    }
}

impl PartialEq for CustomAffect {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

/// What bends a particle's way. Each says what it does to a particle of a
/// given age; nothing is carried from one moment to the next.
#[derive(Clone, Debug, PartialEq)]
pub enum Affector {
    /// What an application wrote itself; see [`CustomAffect`].
    Custom(CustomAffect),
    /// Half the pull times the time squared, the way `direction` points.
    Gravity { direction: Vec3, magnitude: f32 },
    /// Round a point, `magnitude` degrees a second about `axis`.
    Rotator {
        axis: Vec3,
        pivot: Vec3,
        magnitude: f32,
    },
    /// Sways all together and each its own way.
    Wander {
        amount: [f32; 3],
        pace: [f32; 3],
        start: [f32; 3],
        own_amount: [f32; 3],
        own_pace: [f32; 3],
        amount_variation: f32,
        pace_variation: f32,
        fade_in: f32,
        fade_out: f32,
    },
    /// To a place in a shape, there after `duration` (negative: its life).
    Attract {
        place: Mat4,
        shape: Shape,
        variation: [f32; 3],
        duration: f32,
        duration_variation: f32,
        hide_at_end: bool,
    },
    /// Away from a point: by `strength` inside `radius`, by nothing beyond
    /// `outer`, smoothly less between.
    Repel {
        at: Vec3,
        radius: f32,
        outer: f32,
        strength: f32,
    },
    /// Between two sizes, again every `duration`.
    Scale {
        min: f32,
        max: f32,
        duration: f32,
        curve: ScaleCurve,
    },
}

/// What a particle is so far at an age.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Current {
    pub position: Vec3,
    /// Degrees.
    pub rotation: Vec3,
    pub scale: f32,
    pub alpha: f32,
    /// What it is coloured, 0 to 255.
    pub color: [u8; 4],
    /// Seconds along its way.
    pub seconds: f32,
}

impl Affector {
    pub fn affect(&self, seed: u32, p: &Particle, c: &mut Current) {
        let s = c.seconds;
        match self {
            Affector::Custom(CustomAffect(affect)) => affect(seed, p, c),
            Affector::Gravity {
                direction,
                magnitude,
            } => c.position = c.position + *direction * (0.5 * magnitude * s * s),
            Affector::Rotator {
                axis,
                pivot,
                magnitude,
            } => {
                let turn = Quat::from_axis_angle(*axis, (s * magnitude).to_radians());
                c.position = turn.rotate(c.position - *pivot) + *pivot;
            }
            Affector::Wander {
                amount,
                pace,
                start,
                own_amount,
                own_pace,
                amount_variation,
                pace_variation,
                fade_in,
                fade_out,
            } => {
                let mut smooth = 1.0_f32;
                if *fade_in > 0.0 {
                    smooth = (s * 1000.0 / fade_in).min(1.0);
                }
                if *fade_out > 0.0 {
                    smooth = smooth.min((p.life - s * 1000.0) / fade_out);
                }
                let turn = std::f32::consts::TAU;
                let mut by = [0.0_f32; 3];
                for axis in 0..3 {
                    if amount[axis] != 0.0 && pace[axis] != 0.0 {
                        by[axis] +=
                            smooth * (start[axis] + s * turn * pace[axis]).sin() * amount[axis];
                    }
                    if own_amount[axis] != 0.0 && own_pace[axis] != 0.0 {
                        let k = axis as u32;
                        let faster =
                            1.0 + pace_variation * spread(seed, p.index, channel::WANDER + k);
                        let further =
                            1.0 + amount_variation * spread(seed, p.index, channel::WANDER + 3 + k);
                        let from = random(seed, p.index, channel::WANDER + 6) * turn;
                        by[axis] += smooth
                            * (from + faster * s * turn * own_pace[axis]).sin()
                            * further
                            * own_amount[axis];
                    }
                }
                c.position = c.position + vec3(by);
            }
            Affector::Attract {
                place,
                shape,
                variation,
                duration,
                duration_variation,
                hide_at_end,
            } => {
                let mut whole = if *duration < 0.0 { p.life } else { *duration };
                if *duration_variation != 0.0 {
                    whole += duration_variation * spread(seed, p.index, channel::ATTRACT + 3);
                }
                let done = if whole > 0.0 {
                    (s * 1000.0 / whole).clamp(0.0, 1.0)
                } else {
                    1.0
                };
                if *hide_at_end && done >= 1.0 {
                    c.alpha = 0.0;
                    return;
                }
                let to = place.transform_point(shape.position(seed, p.index))
                    + Vec3::new(
                        variation[0] * spread(seed, p.index, channel::ATTRACT),
                        variation[1] * spread(seed, p.index, channel::ATTRACT + 1),
                        variation[2] * spread(seed, p.index, channel::ATTRACT + 2),
                    );
                c.position = c.position + (to - c.position) * done;
            }
            Affector::Repel {
                at,
                radius,
                outer,
                strength,
            } => {
                let away = c.position - *at;
                let far = away.length();
                if far == 0.0 {
                    return;
                }
                let inside = if *outer > *radius {
                    1.0 - (far - radius) / (outer - radius)
                } else if far <= *radius {
                    1.0
                } else {
                    0.0
                };
                c.position = c.position + away * (strength * smoothstep(inside) / far);
            }
            Affector::Scale {
                min,
                max,
                duration,
                curve,
            } => {
                if *duration <= 0.0 {
                    return;
                }
                let times = s * 1000.0 / duration;
                let through = times - times.floor();
                let turn = std::f32::consts::TAU;
                let share = match curve {
                    ScaleCurve::Linear => through,
                    ScaleCurve::SewSaw => 1.0 - (1.0 - 2.0 * through).abs(),
                    ScaleCurve::SineWave => 0.5 + 0.5 * (turn * through).sin(),
                    ScaleCurve::AbsSineWave => (turn * through).sin().abs(),
                    ScaleCurve::Step => {
                        if through < 0.5 {
                            0.0
                        } else {
                            1.0
                        }
                    }
                    ScaleCurve::SmoothStep => smoothstep(through),
                };
                c.scale *= min + (max - min) * share;
            }
        }
    }
}

/// One particle as its emitter gave it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Particle {
    pub index: u32,
    /// Seconds.
    pub begin: f32,
    /// Seconds.
    pub end: f32,
    /// Milliseconds.
    pub life: f32,
    pub position: Vec3,
    pub velocity: Vec3,
    /// Degrees.
    pub rotation: Vec3,
    /// Degrees a second.
    pub spin: Vec3,
    pub color: [u8; 4],
    pub from: f32,
    pub to: f32,
    pub reversed: bool,
    /// The system's step this one is not to be seen in: a trail emitter wrote
    /// it into a table that step had already gone past.
    unseen: Option<u64>,
}

/// How a particle comes and goes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Fade {
    None,
    #[default]
    Opacity,
    Scale,
}

/// Which way a particle is turned.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Align {
    #[default]
    None,
    /// Facing away from a point.
    Target(Vec3),
    /// Along the way it set off.
    StartVelocity,
}

/// What a kind of particle says of how each of its particles looks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Look {
    pub max_amount: usize,
    pub color: [f32; 4],
    pub color_variation: [f32; 4],
    pub unified_color_variation: bool,
    pub fade_in: Fade,
    pub fade_out: Fade,
    pub fade_in_duration: f32,
    pub fade_out_duration: f32,
    pub align: Align,
    /// A sprite that faces the eye is turned towards nothing else.
    pub billboard: bool,
}

impl Default for Look {
    fn default() -> Self {
        Look {
            max_amount: 100,
            color: [1.0; 4],
            color_variation: [0.0; 4],
            unified_color_variation: false,
            fade_in: Fade::Opacity,
            fade_out: Fade::Opacity,
            fade_in_duration: 250.0,
            fade_out_duration: 250.0,
            align: Align::None,
            billboard: false,
        }
    }
}

/// A particle as it is at a time, ready to be drawn.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Seen {
    /// Its place in its kind's table.
    pub place: usize,
    pub index: u32,
    pub position: Vec3,
    pub rotation: Vec3,
    pub turn: Quat,
    pub aligned: bool,
    pub scale: f32,
    /// A whole number out of 255.
    pub alpha: u8,
    pub color: [u8; 4],
    pub seconds: f32,
    /// How far through its life, nought to one.
    pub age: f32,
}

/// The turn that brings the z axis round to a way by the shortest path.
fn towards(way: Vec3) -> Option<Quat> {
    if way.length() == 0.0 {
        return None;
    }
    let w = way.normalized();
    let facing = w.z + 1.0;
    if facing < 1e-6 {
        return Some(Quat {
            x: 0.0,
            y: 1.0,
            z: 0.0,
            w: 0.0,
        });
    }
    let root = (2.0 * facing).sqrt();
    let (qw, qx, qy) = (root / 2.0, -w.y / root, w.x / root);
    let whole = (qw * qw + qx * qx + qy * qy).sqrt();
    Some(Quat {
        x: qx / whole,
        y: qy / whole,
        z: 0.0,
        w: qw / whole,
    })
}

/// The turn that faces a particle away from a point: about the line across the
/// way and the system's forward, by the angle between them.
fn facing(way: Vec3) -> Option<Quat> {
    if way.length() == 0.0 {
        return None;
    }
    let w = way.normalized();
    let mut axis = Vec3::new(w.y, -w.x, 0.0);
    if axis.length() == 0.0 {
        axis = Vec3::new(0.0, 1.0, 0.0);
    }
    Some(Quat::from_axis_angle(
        axis.normalized(),
        (-w.z).clamp(-1.0, 1.0).acos(),
    ))
}

/// The particle `p` at system time `time` seconds, or `None` when it is not
/// there. One is there from the moment it starts to the moment its life is
/// over, both included.
pub fn look(
    p: &Particle,
    place: usize,
    time: f32,
    seed: u32,
    look: &Look,
    affecting: &[Affector],
) -> Option<Seen> {
    if time < p.begin || time > p.end {
        return None;
    }
    let age = (time - p.begin) * 1000.0;
    // One that goes backwards is at the end of its way when it starts.
    let seconds = (if p.reversed { p.life - age } else { age }) / 1000.0;
    let mut c = Current {
        position: p.position + p.velocity * seconds,
        rotation: p.rotation + p.spin * seconds,
        scale: 1.0,
        alpha: 1.0,
        color: p.color,
        seconds,
    };
    for a in affecting {
        a.affect(seed, p, &mut c);
    }
    let mut turn = Quat::from_euler_degrees(c.rotation);
    let aligned = if look.billboard {
        None
    } else {
        match look.align {
            Align::None => None,
            Align::StartVelocity => towards(p.velocity),
            Align::Target(aim) => facing(c.position - aim),
        }
    };
    if let Some(a) = aligned {
        turn = a * turn;
    }
    let gone = seconds * 1000.0;
    let through = if p.life > 0.0 { gone / p.life } else { 0.0 };
    let mut scale = (p.from + (p.to - p.from) * through) * c.scale;
    let mut alpha = c.alpha;
    // It comes and goes by the time of its way, so one that goes backwards
    // goes as it starts.
    if look.fade_in != Fade::None && look.fade_in_duration > 0.0 {
        let f = (gone / look.fade_in_duration).clamp(0.0, 1.0);
        match look.fade_in {
            Fade::Scale => scale *= f,
            _ => alpha *= f,
        }
    }
    if look.fade_out != Fade::None && look.fade_out_duration > 0.0 {
        let f = ((p.life - gone) / look.fade_out_duration).clamp(0.0, 1.0);
        match look.fade_out {
            Fade::Scale => scale *= f,
            _ => alpha *= f,
        }
    }
    Some(Seen {
        place,
        index: p.index,
        position: c.position,
        rotation: c.rotation,
        turn,
        aligned: aligned.is_some(),
        scale,
        alpha: (f32::from(c.color[3]) * alpha) as u8,
        color: c.color,
        seconds,
        age: through.clamp(0.0, 1.0),
    })
}

/// A burst of an emitter. One that is `dynamic` comes when the time passes
/// `time`; otherwise it is there from the start of the system.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Burst {
    /// What it is, for telling whether an emitter has other bursts than it
    /// had: anything that stays the same while the burst does.
    pub id: u64,
    pub time: f64,
    pub amount: f32,
    pub duration: f64,
    pub dynamic: bool,
    pub enabled: bool,
    pub amount_variation: f32,
    pub trigger: Trigger,
}

/// What a dynamic burst waits for. Each also comes at its `time`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Trigger {
    #[default]
    Time,
    Start,
    End,
}

/// What an emitter says of what it emits.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Emitter {
    pub enabled: bool,
    pub emit_rate: f64,
    pub life_span: f32,
    pub life_span_variation: f32,
    pub scale: f32,
    /// Negative: the same as `scale`.
    pub end_scale: f32,
    pub scale_variation: f32,
    /// Negative: the same as `scale_variation`'s.
    pub end_scale_variation: f32,
    pub rotation: [f32; 3],
    pub rotation_variation: [f32; 3],
    pub spin: [f32; 3],
    pub spin_variation: [f32; 3],
    pub reversed: bool,
    pub shape: Shape,
    pub direction: Direction,
    /// Where it is in its system.
    pub place: Mat4,
    /// Its turn alone: no scale makes a direction longer.
    pub turn: Quat,
    /// Its own turn, without its parents', which is what turns the way the
    /// particles a trail emitter starts set off.
    pub own_turn: Quat,
    pub bursts: Vec<Burst>,
}

#[derive(Clone, Copy, Debug)]
struct Spreading {
    end: f64,
    duration: f64,
    amount: u32,
    emitted: u32,
    previous: f64,
}

/// What an emitter keeps from one time to the next.
#[derive(Clone, Debug, Default)]
pub struct EmitterState {
    previous: f64,
    burst_before: f64,
    owed: f64,
    spreading: Vec<Spreading>,
    idle: bool,
    rate: Option<f64>,
    /// The bursts it had when it emitted those that are there from the start.
    generated: Option<Vec<u64>>,
}

impl EmitterState {
    /// One that starts from the time `from`.
    pub fn starting(from: f64) -> Self {
        EmitterState {
            previous: from,
            burst_before: from,
            ..EmitterState::default()
        }
    }

    /// The same again, but those it emitted from the start stay emitted.
    fn again(&self, from: f64) -> Self {
        EmitterState {
            generated: self.generated.clone(),
            ..EmitterState::starting(from)
        }
    }
}

/// The table of a kind of particle: the next one emitted takes the place after
/// the last, and when the table is full the oldest's. Those of a burst that is
/// there from the start are at the front and keep their places.
#[derive(Clone, Debug, Default)]
pub struct Table {
    pub slots: Vec<Option<Particle>>,
    next: usize,
    kept: usize,
    /// The place written last.
    pub last: usize,
}

impl Table {
    pub fn clear(&mut self) {
        *self = Table::default();
    }

    fn take(&mut self, most: usize, kept: bool) -> Option<usize> {
        if self.slots.len() > most {
            self.slots.truncate(most);
        }
        if self.kept >= most {
            return None;
        }
        let at = if kept {
            self.next = self.next.max(self.kept + 1);
            let at = self.kept;
            self.kept += 1;
            at
        } else {
            if self.next >= most {
                self.next = self.kept;
            }
            let at = self.next;
            self.next += 1;
            at
        };
        self.last = at;
        Some(at)
    }

    fn put(&mut self, at: usize, p: Particle) {
        if self.slots.len() <= at {
            self.slots.resize(at + 1, None);
        }
        self.slots[at] = Some(p);
    }

    /// The most recently emitted first, from the place written last back.
    pub fn newest_first(&self, place: usize, most: usize) -> usize {
        (self.last + most - place % most.max(1)) % most.max(1)
    }

    /// From the place written last onwards: the newest, then the oldest.
    pub fn oldest_first(&self, place: usize, most: usize) -> usize {
        (place + most - self.last % most.max(1)) % most.max(1)
    }
}

/// Everything of a system that goes from one time to the next and is not an
/// emitter's or a kind's own.
#[derive(Clone, Copy, Debug, Default)]
pub struct Counter {
    /// How many particles were ever emitted: the number each is known by.
    pub numbered: u32,
    /// How many times the system came to a new time.
    pub updates: u64,
}

fn byte(v: f32) -> u8 {
    v.clamp(0.0, 255.0) as u8
}

/// When one of several starts that are spread over a time: `share` of the way
/// through `span` milliseconds from `from`, in seconds of 32 bits.
fn when(from: f64, share: f32, span: f64) -> f32 {
    (from / 1000.0) as f32 + share * (span / 1000.0) as f32
}

/// Emits one particle to start at `begin` seconds. `placed` says whether the
/// emitter's place puts it; `around` is how far from there it starts.
#[allow(clippy::too_many_arguments)]
fn emit(
    counter: &mut Counter,
    seed: u32,
    emitter: &Emitter,
    look: &Look,
    table: &mut Table,
    begin: f32,
    placed: bool,
    turn: Quat,
    around: Option<Vec3>,
    kept: bool,
) -> Option<usize> {
    let at = table.take(look.max_amount, kept)?;
    let index = counter.numbered;
    counter.numbered = counter.numbered.wrapping_add(1);
    let inside = emitter.shape.position(seed, index);
    let mut position = if placed {
        emitter.place.transform_point(inside)
    } else {
        inside
    };
    if let Some(a) = around {
        position = position + a;
    }
    let velocity = turn.rotate(emitter.direction.sample(seed, index, position));
    // A turn is kept as a whole number of 127ths of a full one, and how fast it
    // turns as the whole root of the degrees a second: Qt's table of particles
    // has a byte for each, which a slow turn shows.
    let mut rotation = [0.0_f32; 3];
    let mut spin = [0.0_f32; 3];
    for axis in 0..3 {
        let k = axis as u32;
        let wanted = emitter.rotation[axis]
            + emitter.rotation_variation[axis] * spread(seed, index, channel::TURN + k);
        rotation[axis] = (wanted * 127.0 / 360.0).trunc() * 360.0 / 127.0;
        let fast = emitter.spin[axis]
            + emitter.spin_variation[axis] * spread(seed, index, channel::SPIN + k);
        let root = (fast.signum() * fast.abs().sqrt())
            .trunc()
            .clamp(-127.0, 127.0);
        spin[axis] = root.abs() * root;
    }
    let together = random(seed, index, channel::COLOR + 3);
    let chance = |c: u32| {
        if look.unified_color_variation {
            together * 256.0
        } else {
            random(seed, index, channel::COLOR + c) * 256.0
        }
    };
    let mixed = |c: usize| {
        let by = look.color_variation[c];
        byte(look.color[c] * 255.0 * (1.0 - by) + chance(c as u32) * by)
    };
    let by = emitter.scale_variation * spread(seed, index, channel::SCALE);
    let end = if emitter.end_scale < 0.0 {
        emitter.scale
    } else {
        emitter.end_scale
    };
    let end_by = if emitter.end_scale_variation < 0.0 {
        by
    } else {
        emitter.end_scale_variation * spread(seed, index, channel::END_SCALE)
    };
    let life = (emitter.life_span
        + emitter.life_span_variation * spread(seed, index, channel::LIFE))
    .max(0.0);
    table.put(
        at,
        Particle {
            index,
            begin,
            end: begin + life / 1000.0,
            life,
            position,
            velocity,
            rotation: vec3(rotation),
            spin: vec3(spin),
            color: [mixed(0), mixed(1), mixed(2), mixed(3)],
            from: (emitter.scale + by).max(0.0),
            to: (end + end_by).max(0.0),
            reversed: emitter.reversed,
            unseen: None,
        },
    );
    Some(at)
}

/// How many the rate gives for the time since it last emitted, with what was
/// left over of the times before.
fn owing(emitter: &Emitter, state: &mut EmitterState, now: f64) -> u32 {
    if emitter.emit_rate <= 0.0 {
        return 0;
    }
    let exact = (now - state.previous) * emitter.emit_rate / 1000.0;
    let mut amount = exact.floor();
    if amount > 0.0 {
        state.owed += exact - amount;
    }
    if state.owed >= 1.0 {
        amount += 1.0;
        state.owed -= 1.0;
    }
    amount as u32
}

/// How many of a burst, give or take its variation.
fn amount_of(seed: u32, counter: &Counter, b: &Burst) -> u32 {
    let amount = b.amount.floor().max(0.0);
    if b.amount_variation == 0.0 {
        return amount as u32;
    }
    (amount + b.amount_variation * spread(seed, counter.numbered, channel::AMOUNT))
        .round()
        .max(0.0) as u32
}

/// The bursts whose time came since the emitter last looked, both ends
/// included: all at once, or a share with each step until their `duration` is
/// over. With another trigger, those that wait for it instead. `time` is the
/// system's without its `startTime`.
fn dynamic(
    seed: u32,
    counter: &Counter,
    emitter: &Emitter,
    state: &mut EmitterState,
    trigger: Trigger,
    time: f64,
) -> u32 {
    let mut amount = 0;
    let since = state.burst_before;
    for b in emitter.bursts.iter().filter(|b| b.dynamic && b.enabled) {
        let timed = trigger == Trigger::Time && since <= b.time && b.time <= time;
        if !timed && !(trigger != Trigger::Time && b.trigger == trigger) {
            continue;
        }
        let many = amount_of(seed, counter, b);
        if many == 0 {
            continue;
        }
        if timed && b.duration > 0.0 {
            state.spreading.push(Spreading {
                end: time + b.duration,
                duration: b.duration,
                amount: many,
                emitted: 0,
                previous: since,
            });
        } else {
            amount += many;
        }
    }
    state.spreading.retain_mut(|s| {
        if time >= s.end {
            amount += s.amount - s.emitted;
            return false;
        }
        let share = ((f64::from(s.amount) * (time - s.previous) / s.duration) as u32)
            .min(s.amount - s.emitted);
        if share > 0 {
            s.emitted += share;
            s.previous = time;
            amount += share;
        }
        true
    });
    state.burst_before = time;
    amount
}

/// The bursts that are there from the start: before the first thing the
/// emitter emits, and again when it has other bursts than it had. An emitter
/// with bursts of whatever sort empties its kind first.
fn generate(
    counter: &mut Counter,
    seed: u32,
    emitter: &Emitter,
    look: &Look,
    state: &mut EmitterState,
    table: &mut Table,
) {
    let ids: Vec<u64> = emitter.bursts.iter().map(|b| b.id).collect();
    if state.generated.as_ref() == Some(&ids) {
        return;
    }
    if !ids.is_empty() {
        table.clear();
        for b in emitter.bursts.iter().filter(|b| !b.dynamic) {
            let many = (b.amount.floor().max(0.0) as usize).min(look.max_amount);
            if many == 0 {
                return;
            }
            for i in 0..many {
                let begin = when(b.time, i as f32 / many as f32, b.duration);
                emit(
                    counter,
                    seed,
                    emitter,
                    look,
                    table,
                    begin,
                    true,
                    emitter.turn,
                    None,
                    true,
                );
            }
        }
    }
    state.generated = Some(ids);
}

/// Has a state follow what its emitter became since the last step: one that
/// was not enabled starts from the time it is when it is, and so does one that
/// had no rate from the time it has.
pub fn keep_up(emitter: &Emitter, state: &mut EmitterState, before: f64) {
    if !emitter.enabled {
        state.idle = true;
    } else if state.idle {
        state.idle = false;
        state.previous = before;
        state.burst_before = before;
    }
    if state.rate == Some(0.0) && emitter.emit_rate != 0.0 {
        state.previous = before;
    }
    state.rate = Some(emitter.emit_rate);
}

/// What the time up to `now` has an emitter emit; `time` is `now` without the
/// system's `startTime`, which is what dynamic bursts count by.
#[allow(clippy::too_many_arguments)]
pub fn run_emitter(
    counter: &mut Counter,
    seed: u32,
    emitter: &Emitter,
    look: &Look,
    state: &mut EmitterState,
    table: &mut Table,
    now: f64,
    time: f64,
) {
    generate(counter, seed, emitter, look, state, table);
    // Time gone back is begun from again; and of time that jumped ahead, no
    // more is made up for than a particle lives.
    if now < state.previous {
        state.previous = now;
    }
    state.previous = state
        .previous
        .max(now - f64::from(emitter.life_span + emitter.life_span_variation));
    let rate = owing(emitter, state, now);
    let bursts = dynamic(seed, counter, emitter, state, Trigger::Time, time);
    let amount = ((rate + bursts) as usize).min(look.max_amount);
    if amount == 0 {
        return;
    }
    let span = now - state.previous;
    for i in 0..amount {
        let begin = when(state.previous, (1 + i) as f32 / amount as f32, span);
        emit(
            counter,
            seed,
            emitter,
            look,
            table,
            begin,
            true,
            emitter.turn,
            None,
            false,
        );
    }
    state.previous = now;
}

/// `count` particles now, or spread over `duration` from now, `position` that
/// far from where the emitter would start them.
#[allow(clippy::too_many_arguments)]
pub fn burst(
    counter: &mut Counter,
    seed: u32,
    emitter: &Emitter,
    look: &Look,
    table: &mut Table,
    now: f64,
    count: u32,
    duration: f64,
    position: Option<Vec3>,
) {
    let many = (count as usize).min(look.max_amount);
    for i in 0..many {
        let begin = when(now, (1 + i) as f32 / many as f32, duration.max(0.0));
        emit(
            counter,
            seed,
            emitter,
            look,
            table,
            begin,
            true,
            emitter.turn,
            position,
            false,
        );
    }
}

/// A trail emitter in the step of its system.
#[derive(Debug)]
pub struct Follower<'a> {
    pub emitter: &'a Emitter,
    pub state: &'a mut EmitterState,
    /// Which table it emits into.
    pub into: usize,
    /// What was asked of it by `burst` since the last step, with when.
    pub asked: &'a [(u32, f64)],
}

/// What a kind keeps to tell a sprite's start and end for those that follow.
#[derive(Clone, Copy, Debug, Default)]
pub struct Aged {
    age: f32,
    size: f32,
}

/// Has those that follow the kind `kind` emit what its particles give them,
/// after the other emitters have emitted: a particle that is there has them
/// emit where it is, and one whose life is over where it ended.
///
/// A start or an end is told once for a sprite; for a model an end comes for
/// as long as the particle has its place in the table, and a start only for one
/// with no life at all. That is what Qt 6.11 was found to do, and it is not what
/// its documentation says.
#[allow(clippy::too_many_arguments)]
pub fn run_trail(
    counter: &mut Counter,
    seed: u32,
    tables: &mut [Table],
    looks: &[Look],
    kind: usize,
    sprite: bool,
    aged: &mut Vec<Aged>,
    followers: &mut [Follower<'_>],
    affecting: &[Affector],
    now: f64,
    time: f64,
) {
    let seconds = (now / 1000.0) as f32;
    let step = counter.updates;
    let amounts: Vec<u32> = followers
        .iter_mut()
        .map(|f| {
            if f.emitter.enabled {
                owing(f.emitter, f.state, now)
            } else {
                0
            }
        })
        .collect();
    // Only one that has something to give takes part: the others keep what
    // they owe, and their time, for a step in which they do.
    let takes_part: Vec<bool> = followers
        .iter()
        .zip(&amounts)
        .map(|(f, amount)| {
            f.emitter.enabled
                && (*amount > 0
                    || !f.asked.is_empty()
                    || f.emitter.bursts.iter().any(|b| b.dynamic))
        })
        .collect();
    if !sprite && !takes_part.contains(&true) {
        return;
    }
    let most = looks[kind].max_amount;
    if sprite && aged.len() < most {
        aged.resize(most, Aged::default());
    }
    let filled = most.min(tables[kind].slots.len());
    let alive: Vec<(usize, Particle)> = tables[kind].slots[..filled]
        .iter()
        .enumerate()
        .filter_map(|(place, p)| p.map(|p| (place, p)))
        .collect();
    for (place, p) in alive {
        let seen = look(&p, place, seconds, seed, &looks[kind], affecting);
        let fire = |tables: &mut [Table],
                    counter: &mut Counter,
                    followers: &mut [Follower<'_>],
                    at: Vec3,
                    rate: bool,
                    trigger: Trigger| {
            for ((f, amount), takes_part) in followers.iter_mut().zip(&amounts).zip(&takes_part) {
                if !takes_part {
                    continue;
                }
                let lk = &looks[f.into];
                let extra = dynamic(seed, counter, f.emitter, f.state, trigger, time);
                let many = (if rate { *amount } else { 0 } + extra) as usize;
                let many = many.min(lk.max_amount);
                let from = (f.state.previous / 1000.0) as f32;
                let gap = ((now - f.state.previous) / 1000.0) as f32 / many.max(1) as f32;
                let mark = |table: &mut Table, written: Option<usize>| {
                    // What goes into a table that was gone past is not there
                    // to see before the next step.
                    if let Some(w) = written
                        && (f.into < kind || (f.into == kind && w < place))
                        && let Some(Some(n)) = table.slots.get_mut(w)
                    {
                        n.unseen = Some(step);
                    }
                };
                let table = &mut tables[f.into];
                for i in 0..many {
                    let written = emit(
                        counter,
                        seed,
                        f.emitter,
                        lk,
                        table,
                        from + gap * i as f32,
                        false,
                        f.emitter.own_turn,
                        Some(at),
                        false,
                    );
                    mark(table, written);
                }
                for (count, asked) in f.asked {
                    for _ in 0..(*count as usize).min(lk.max_amount) {
                        let written = emit(
                            counter,
                            seed,
                            f.emitter,
                            lk,
                            table,
                            (*asked / 1000.0) as f32,
                            false,
                            f.emitter.own_turn,
                            Some(at),
                            false,
                        );
                        mark(table, written);
                    }
                }
                f.state.previous = now;
            }
        };
        let Some(seen) = seen else {
            if seconds > p.end
                && (if sprite {
                    aged[place].age > 0.0
                } else {
                    p.life > 0.0
                })
            {
                let whole = if p.reversed { 0.0 } else { p.life / 1000.0 };
                let ended = p.position + p.velocity * whole;
                fire(tables, counter, followers, ended, false, Trigger::End);
            }
            if sprite && aged[place].size > 0.0 {
                aged[place] = Aged::default();
            }
            continue;
        };
        let started = if sprite {
            seconds < p.end && aged[place].age == 0.0
        } else {
            p.life <= 0.0
        };
        if started {
            fire(
                tables,
                counter,
                followers,
                p.position,
                false,
                Trigger::Start,
            );
        }
        fire(
            tables,
            counter,
            followers,
            seen.position,
            true,
            Trigger::Time,
        );
        if sprite {
            aged[place] = Aged {
                age: if p.life > 0.0 { seen.age } else { 0.0 },
                size: seen.scale,
            };
        }
    }
}

/// Whether a trail's writing keeps `p` from being seen in the step `step`.
pub fn hidden_in(p: &Particle, step: u64) -> bool {
    p.unseen == Some(step)
}

/// Clears what an emitter owes, as a system that is reset does.
pub fn reset_state(state: &mut EmitterState) {
    *state = state.again(0.0);
}

/// How far through the frames of a sequence.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sequence {
    pub frame_count: u32,
    pub frame_index: u32,
    /// Negative: all of them in the particle's life.
    pub duration: f32,
    pub duration_variation: f32,
    pub random_start: bool,
    pub direction: SequenceDirection,
}

/// `SpriteSequence3D.animationDirection`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SequenceDirection {
    #[default]
    Normal,
    Reverse,
    Alternate,
    AlternateReverse,
    SingleFrame,
}

impl Sequence {
    /// From nought, the beginning of the first frame, up to one, the end of the
    /// last, for a particle with this life after `seconds`.
    pub fn through(&self, seed: u32, index: u32, life: f32, seconds: f32) -> f32 {
        use SequenceDirection as D;
        let count = self.frame_count.max(1);
        let mut first = 0.0_f32;
        if self.random_start {
            first = random(seed, index, channel::FRAME);
        } else if count > 1 && self.frame_index > 0 {
            let at = self.frame_index.min(count - 1) as f32;
            first = if self.direction == D::SingleFrame {
                at / ((count - 1) as f32 + 0.0001)
            } else {
                at / count as f32
            };
        }
        let mut whole = life / 1000.0;
        if self.duration > 0.0 {
            let varied = self.duration_variation / 1000.0;
            whole = (self.duration / 1000.0 + varied
                - 2.0 * random(seed, index, channel::FRAME + 1) * varied)
                .max(0.001);
        }
        let gone = seconds / whole;
        let nearly = 0.9999_f32;
        let through = match self.direction {
            D::Normal => (first + gone) % 1.0,
            D::Reverse => (first + nearly - gone % 1.0) % 1.0,
            D::Alternate => ((1.0 + first + gone) % 2.0 - 1.0).abs(),
            D::AlternateReverse => ((1.0 + (first + nearly) % 1.0 - gone).abs() % 2.0 - 1.0).abs(),
            D::SingleFrame => first,
        };
        through.clamp(0.0, nearly)
    }
}

/// A point of a line.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Point {
    position: Vec3,
    size: f32,
    color: [f32; 4],
    tangent: Vec3,
    binormal: Vec3,
    length: f32,
}

/// How a line lays its picture.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TexcoordMode {
    #[default]
    Absolute,
    Relative,
    Fill,
}

/// What a kind of line says.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LineLook {
    pub segments: usize,
    pub width: f32,
    /// Negative: as long as `segments` pieces of `delta_min`.
    pub length: f32,
    pub length_variation: f32,
    pub delta_min: f32,
    pub eol_fade_out: f32,
    pub mode: TexcoordMode,
    pub offset: [f32; 2],
    pub scale_multiplier: f32,
    pub alpha_fade: f32,
}

/// The head of a line: where its particle is.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Head {
    position: Vec3,
    rotation: Vec3,
    color: [f32; 4],
    size: f32,
    age: f32,
}

#[derive(Clone, Debug, Default)]
struct Trail {
    begin: f32,
    index: u32,
    head: Head,
    count: usize,
    current: usize,
    limit: f32,
    shown: bool,
    points: Vec<Point>,
}

#[derive(Clone, Debug)]
struct Fading {
    trail: Trail,
    begin: f32,
    end: f32,
}

/// The lines there are of a kind, kept from one time the system comes to the
/// next: what a line is depends on the times the system went through.
#[derive(Clone, Debug, Default)]
pub struct Lines {
    segments: usize,
    trails: Vec<Option<Trail>>,
    fading: Vec<Fading>,
    traced: Option<f64>,
}

/// One piece of a line, from `a` to `b`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Piece {
    pub a: Vec3,
    pub b: Vec3,
    pub across: Vec3,
    pub width_a: f32,
    pub width_b: f32,
    pub color: [f32; 4],
    pub length_a: f32,
    pub length_b: f32,
}

/// The way a line faces from how its particle is turned: across that and the
/// way the line goes is the way it is wide. One not turned faces the other way
/// from one turned by nothing to speak of, as Qt has it.
fn faced(rotation: Vec3) -> Vec3 {
    let (x, y) = (rotation.x.to_radians(), rotation.y.to_radians());
    if x.abs() <= 1e-5 && y.abs() <= 1e-5 {
        return Vec3::new(0.0, 0.0, -1.0);
    }
    Vec3::new(y.sin(), -x.sin() * y.cos(), x.cos() * y.cos())
}

fn along(way: Vec3) -> Vec3 {
    if way.length() > 1e-6 {
        way.normalized()
    } else {
        Vec3::ZERO
    }
}

impl Trail {
    /// Puts down where the particle is, if it has come far enough from the
    /// last: `apart`, or as far as leaves the line its length over its pieces.
    fn put(&mut self, segments: usize, apart: f32) {
        let head = self.head;
        let mut at = self.current;
        let before = (self.count > 0).then(|| self.points[at]);
        if let Some(b) = before
            && segments > 1
        {
            let far = (b.position - head.position).length();
            let wanted = if self.limit >= 0.0 {
                self.limit / (segments - 1) as f32
            } else {
                apart
            };
            if far < wanted {
                return;
            }
        }
        if self.count < segments {
            self.count += 1;
        }
        if before.is_some() {
            at = (at + 1) % segments;
        }
        self.current = at;
        let normal = faced(head.rotation);
        let mut here = self.points[at];
        here.color = head.color;
        here.size = head.size;
        match before {
            Some(b) if segments == 1 => {
                let way = b.position - head.position;
                let back = along(way);
                let behind = if self.limit >= 0.0 { self.limit } else { apart };
                here.position = head.position + back * behind;
                here.length += way.length();
                here.tangent = back;
                here.binormal = normal.cross(back);
            }
            _ => {
                here.position = head.position;
                here.length = 0.0;
            }
        }
        if let Some(b) = before
            && segments > 1
        {
            let way = b.position - head.position;
            let far = way.length();
            let tangent = if far > 0.0 {
                way * (1.0 / far)
            } else {
                Vec3::ZERO
            };
            here.length = far + b.length;
            here.binormal = normal.cross(tangent);
            let old = at.checked_sub(1).unwrap_or(segments - 1);
            self.points[old].tangent = tangent;
            self.points[old].binormal = if self.count == 1 {
                here.binormal
            } else {
                along(b.binormal + here.binormal)
            };
        }
        self.points[at] = here;
    }

    /// The pieces of the line, from the particle back.
    fn pieces(&self, alpha: f32, look: &LineLook, out: &mut Vec<Piece>) {
        let segments = look.segments;
        if self.count == 0 {
            return;
        }
        let mut at = self.current;
        let partial = (self.points[at].position - self.head.position).length();
        let mut from = self.points[at].length + partial;
        let mut scale = -1.0_f32;
        let mut first_length = 0.0;
        match look.mode {
            TexcoordMode::Absolute => {
                first_length = from;
                from = 0.0;
            }
            TexcoordMode::Fill => {
                if self.limit > 0.0 {
                    scale = -1.0 / self.limit;
                } else {
                    let oldest = (at + 1 + segments - self.count) % segments;
                    let mut whole = self.points[at].length - self.points[oldest].length;
                    if self.count < segments {
                        whole += partial;
                    }
                    if whole.abs() > 1e-5 {
                        scale = -1.0 / whole;
                    }
                }
            }
            TexcoordMode::Relative => {}
        }
        let mut last = Point {
            position: self.head.position,
            size: self.head.size * look.width,
            color: self.head.color,
            binormal: faced(self.head.rotation)
                .cross(along(self.points[at].position - self.head.position)),
            length: first_length,
            ..Point::default()
        };
        let (mut grown, mut faded) = (1.0_f32, 1.0_f32);
        let grow = look.scale_multiplier.clamp(0.0, 2.0);
        let fade = 1.0 - look.alpha_fade.clamp(0.0, 1.0);
        let mut whole = 0.0;
        let mut before = self.points[at].length + partial;
        let mut index = 0;
        while index < self.count && (self.limit < 0.0 || whole < self.limit) {
            let one = self.points[at];
            if one.size * look.width <= 0.0 {
                break;
            }
            let mut point = Point {
                size: one.size * look.width,
                length: (from - one.length) * scale,
                ..one
            };
            if self.limit >= 0.0 {
                // No further than the line may be long: its last piece is cut.
                let mut piece = before - one.length;
                before = one.length;
                if whole + piece > self.limit {
                    let over = whole + piece - self.limit;
                    point.position = point.position - one.tangent * over;
                    point.length -= over * scale;
                    piece -= over;
                }
                whole += piece;
            }
            // A line with all its pieces and no `length` ends as far short of
            // its first point as the particle is past its last, so that it is
            // as long from one moment to the next.
            if index + 1 == segments && self.limit < 0.0 && segments > 1 {
                point.position = point.position - one.tangent * partial;
                if look.mode != TexcoordMode::Fill {
                    point.length -= partial * scale;
                }
            }
            let next_grown = grown * grow;
            let next_faded = faded * fade;
            out.push(Piece {
                a: last.position,
                b: point.position,
                across: along(last.binormal + point.binormal),
                width_a: last.size * grown,
                width_b: point.size * next_grown,
                color: [
                    point.color[0],
                    point.color[1],
                    point.color[2],
                    point.color[3] * next_faded * alpha,
                ],
                length_a: last.length,
                length_b: point.length,
            });
            grown = next_grown;
            faded = next_faded;
            last = point;
            at = if at == 0 { segments - 1 } else { at - 1 };
            index += 1;
        }
    }
}

impl Lines {
    /// Comes to the time `now`, with those there are of the kind; `table`
    /// is the kind's. Puts down what there is to put down for each particle.
    /// One whose life is over stays for `eol_fade_out`, where it was, going.
    pub fn trace(&mut self, seed: u32, table: &Table, seen: &[Seen], now: f64, look: &LineLook) {
        let segments = look.segments.max(1);
        if self.segments != segments {
            *self = Lines {
                segments,
                ..Lines::default()
            };
        }
        if self.traced == Some(now) {
            return;
        }
        self.traced = Some(now);
        let time = (now / 1000.0) as f32;
        if self.trails.len() < table.slots.len() {
            self.trails.resize(table.slots.len(), None);
        }
        let mut here = vec![None; table.slots.len()];
        for s in seen {
            here[s.place] = Some(s);
        }
        for (place, p) in table.slots.iter().enumerate() {
            let Some(p) = p else { continue };
            let slot = &mut self.trails[place];
            // A particle that has taken the place of another begins a line.
            if slot
                .as_ref()
                .is_none_or(|t| t.index != p.index || t.begin != p.begin)
            {
                let head = slot.as_ref().map_or(Head::default(), |t| t.head);
                let limit = if look.length > 0.0 {
                    (look.length
                        + look.length_variation * (random(seed, p.index, channel::LENGTH) - 0.5))
                        .max(0.0)
                } else {
                    -1.0
                };
                *slot = Some(Trail {
                    begin: p.begin,
                    index: p.index,
                    head,
                    count: 0,
                    current: 0,
                    limit,
                    shown: true,
                    points: vec![Point::default(); segments],
                });
            }
            let Some(trail) = slot else { continue };
            let Some(one) = here[place] else {
                if time > p.end
                    && trail.head.age > 0.0
                    && look.eol_fade_out > 0.0
                    && trail.count > 0
                {
                    self.fading.push(Fading {
                        trail: trail.clone(),
                        begin: time,
                        end: time + look.eol_fade_out * 0.001,
                    });
                    trail.points = vec![Point::default(); segments];
                    trail.limit = -1.0;
                    trail.shown = false;
                }
                trail.count = 0;
                trail.current = 0;
                if trail.head.size > 0.0 {
                    trail.head = Head::default();
                }
                continue;
            };
            let head = Head {
                position: one.position
                    + Vec3::new(look.offset[0] * one.scale, look.offset[1] * one.scale, 0.0),
                rotation: if one.aligned {
                    one.turn.to_euler_degrees()
                } else {
                    one.rotation
                },
                color: [
                    f32::from(one.color[0]) / 255.0,
                    f32::from(one.color[1]) / 255.0,
                    f32::from(one.color[2]) / 255.0,
                    f32::from(one.alpha) / 255.0,
                ],
                size: one.scale,
                age: one.age,
            };
            let moved = head.size > 0.0 || trail.head.size > 0.0;
            trail.head = head;
            if moved {
                trail.put(segments, look.delta_min.max(0.0));
            }
        }
        self.fading.retain(|g| time >= g.begin && time < g.end);
    }

    /// Every piece of every line at `now`.
    pub fn pieces(&self, now: f64, look: &LineLook) -> Vec<Piece> {
        let time = (now / 1000.0) as f32;
        let mut out = Vec::new();
        for t in self.trails.iter().flatten().filter(|t| t.shown) {
            t.pieces(1.0, look, &mut out);
        }
        for g in &self.fading {
            let alpha = 1.0 - (time - g.begin) * 1000.0 / look.eol_fade_out;
            g.trail.pieces(alpha, look, &mut out);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn emitter() -> Emitter {
        Emitter {
            enabled: true,
            emit_rate: 10.0,
            life_span: 1000.0,
            scale: 1.0,
            end_scale: -1.0,
            end_scale_variation: -1.0,
            place: Mat4::IDENTITY,
            turn: Quat::IDENTITY,
            own_turn: Quat::IDENTITY,
            direction: Direction::Vector {
                direction: [0.0, 100.0, 0.0],
                variation: [0.0; 3],
                normalized: false,
            },
            ..Emitter::default()
        }
    }

    #[test]
    fn chance_is_the_same_whenever_it_is_asked() {
        assert_eq!(random(1, 2, 3), random(1, 2, 3));
        assert_ne!(random(1, 2, 3), random(1, 3, 3));
        assert!((0.0..1.0).contains(&random(7, 8, 9)));
    }

    #[test]
    fn a_rate_gives_so_many_a_second_and_carries_what_is_left_over() {
        let (e, l) = (emitter(), Look::default());
        let mut c = Counter::default();
        let (mut s, mut t) = (EmitterState::starting(0.0), Table::default());
        for now in (0..=10_000).step_by(16) {
            let now = f64::from(now);
            run_emitter(&mut c, 1, &e, &l, &mut s, &mut t, now, now);
        }
        // What has not made a whole particle waits where it was, so the
        // steps' lengths do not lose any.
        assert!(c.numbered.abs_diff(100) <= 2, "{}", c.numbered);
    }

    #[test]
    fn a_full_table_takes_the_place_of_the_oldest() {
        let l = Look {
            max_amount: 2,
            ..Look::default()
        };
        let e = Emitter {
            emit_rate: 3.0,
            ..emitter()
        };
        let mut c = Counter::default();
        let (mut s, mut t) = (EmitterState::starting(0.0), Table::default());
        run_emitter(&mut c, 1, &e, &l, &mut s, &mut t, 1000.0, 1000.0);
        assert_eq!(t.slots.iter().flatten().count(), 2);
    }

    #[test]
    fn a_particle_is_where_its_speed_and_affectors_put_it() {
        let (e, l) = (emitter(), Look::default());
        let mut c = Counter::default();
        let mut t = Table::default();
        burst(&mut c, 1, &e, &l, &mut t, 0.0, 1, 0.0, None);
        let p = t.slots[0].unwrap();
        let gravity = [Affector::Gravity {
            direction: Vec3::new(0.0, -1.0, 0.0),
            magnitude: 100.0,
        }];
        let seen = look(&p, 0, 0.5, 1, &l, &gravity).unwrap();
        assert!((seen.position.y - (50.0 - 12.5)).abs() < 1e-3);
        assert!(look(&p, 0, 1.5, 1, &l, &gravity).is_none());
    }

    #[test]
    fn a_shape_keeps_inside_its_extents() {
        let s = Shape::Sphere {
            extents: [10.0, 10.0, 10.0],
            fill: false,
        };
        for i in 0..100 {
            assert!((s.position(1, i).length() - 10.0).abs() < 1e-3);
        }
    }

    #[test]
    fn a_surface_is_picked_on_its_triangles() {
        let surface = Surface::new(
            &[
                Vec3::ZERO,
                Vec3::new(1.0, 0.0, 0.0),
                Vec3::new(0.0, 1.0, 0.0),
            ],
            None,
        );
        for i in 0..100 {
            let p = surface.pick(1, i, false).unwrap();
            assert!(p.z == 0.0 && p.x >= 0.0 && p.y >= 0.0 && p.x + p.y <= 1.0001);
        }
    }

    #[test]
    fn a_line_stays_as_long_as_it_is_told() {
        let lines_look = LineLook {
            segments: 3,
            width: 1.0,
            length: 30.0,
            length_variation: 0.0,
            delta_min: 10.0,
            eol_fade_out: 0.0,
            mode: TexcoordMode::Absolute,
            offset: [0.0; 2],
            scale_multiplier: 1.0,
            alpha_fade: 0.0,
        };
        let (e, l) = (
            Emitter {
                life_span: 10_000.0,
                ..emitter()
            },
            Look {
                fade_in: Fade::None,
                ..Look::default()
            },
        );
        let mut c = Counter::default();
        let mut t = Table::default();
        burst(&mut c, 1, &e, &l, &mut t, 0.0, 1, 0.0, None);
        let mut lines = Lines::default();
        let p = t.slots[0].unwrap();
        for ms in (0..=1000).step_by(50) {
            let now = f64::from(ms);
            let seen: Vec<Seen> = look(&p, 0, (now / 1000.0) as f32, 1, &l, &[])
                .into_iter()
                .collect();
            lines.trace(1, &t, &seen, now, &lines_look);
        }
        let long: f32 = lines
            .pieces(1000.0, &lines_look)
            .iter()
            .map(|p| (p.a - p.b).length())
            .sum();
        assert!((long - 30.0).abs() < 1.0, "{long}");
    }
}
