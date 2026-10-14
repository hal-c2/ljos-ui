//! The arithmetic of a particle system: dice, shapes, directions and how one
//! particle moves in a step. Nothing here knows an engine, so it is tested
//! on its own.
//!
//! Every system starts its dice from [`SEED`], so the same QML run the same
//! way twice puts its particles in the same places. Qt's randomness is not a
//! promise; this is only what makes Ljós's own runs repeat.

use std::rc::Rc;

use ljos_engine::ObjectId;

/// One simulation step, in seconds. The driver fires at this rate and a
/// step is always this long, so how far a particle has gone depends on how
/// many steps ran and never on when the host woke.
pub(crate) const STEP: f64 = 0.016;

/// Where every system's dice start.
pub(crate) const SEED: u64 = 0x4c6a_6f73_5061_7274;

/// splitmix64: small, fast, and good enough for scattering particles.
#[derive(Debug, Clone)]
pub(crate) struct Rng(u64);

impl Rng {
    pub(crate) fn new(seed: u64) -> Self {
        Rng(seed)
    }

    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// In `[0, 1)`.
    pub(crate) fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// In `[-range, range)`.
    pub(crate) fn signed(&mut self, range: f64) -> f64 {
        (self.unit() * 2.0 - 1.0) * range
    }
}

/// A live particle, in the coordinates of its system.
#[derive(Debug, Clone)]
pub(crate) struct Particle {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) vx: f64,
    pub(crate) vy: f64,
    pub(crate) ax: f64,
    pub(crate) ay: f64,
    /// Seconds lived and seconds to live.
    pub(crate) age: f64,
    pub(crate) life: f64,
    pub(crate) size: f64,
    pub(crate) end_size: f64,
    pub(crate) group: Rc<str>,
    pub(crate) emitter: ObjectId,
    /// Dice thrown once at birth in `[-1, 1)`; a painter scales them by its
    /// own variations, so two painters of one particle vary it alike.
    pub(crate) dice: [f64; 4],
    /// How far a `Wander` has moved the attribute it affects.
    pub(crate) wander: [f64; 2],
}

impl Particle {
    /// How far through its life, `0..=1`.
    pub(crate) fn progress(&self) -> f64 {
        if self.life <= 0.0 {
            1.0
        } else {
            (self.age / self.life).clamp(0.0, 1.0)
        }
    }

    /// Edge length now: from `size` to `end_size` over its life.
    pub(crate) fn current_size(&self) -> f64 {
        let t = self.progress();
        (self.size + (self.end_size - self.size) * t).max(0.0)
    }
}

/// What `ImageParticle.entryEffect` does at the two ends of a life: a
/// factor in `0..=1` that grows from nothing over the first tenth of the
/// life and falls back over the last.
pub(crate) fn entry_factor(age: f64, life: f64) -> f64 {
    if life <= 0.0 {
        return 0.0;
    }
    let edge = life * 0.1;
    (age / edge).min((life - age) / edge).clamp(0.0, 1.0)
}

/// Which attribute a `Wander` moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Affected {
    Position,
    Velocity,
    Acceleration,
}

impl Affected {
    pub(crate) fn from_i32(v: i32) -> Self {
        match v {
            0 => Affected::Position,
            2 => Affected::Acceleration,
            _ => Affected::Velocity,
        }
    }
}

/// One `Wander`'s numbers.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Wandering {
    pub(crate) pace: f64,
    pub(crate) x_variance: f64,
    pub(crate) y_variance: f64,
    pub(crate) affected: Affected,
}

impl Wandering {
    /// The attribute's drift this step: each axis takes a random step of at
    /// most `pace` a second and stays within its variance of where it began.
    pub(crate) fn drift(&self, p: &mut Particle, rng: &mut Rng) -> [f64; 2] {
        let mut moved = [0.0; 2];
        for (i, variance) in [self.x_variance, self.y_variance].into_iter().enumerate() {
            if variance <= 0.0 || self.pace <= 0.0 {
                continue;
            }
            let before = p.wander[i];
            p.wander[i] = (before + rng.signed(self.pace * STEP)).clamp(-variance, variance);
            moved[i] = p.wander[i] - before;
        }
        moved
    }
}

/// Advances `p` by one step; `false` when its life is over.
pub(crate) fn advance(p: &mut Particle, wander: &[Wandering], rng: &mut Rng) -> bool {
    p.age += STEP;
    if p.age >= p.life {
        return false;
    }
    let (mut wvx, mut wvy, mut wax, mut way) = (0.0, 0.0, 0.0, 0.0);
    let (mut dx, mut dy) = (0.0, 0.0);
    for w in wander {
        let moved = w.drift(p, rng);
        match w.affected {
            Affected::Position => {
                dx += moved[0];
                dy += moved[1];
            }
            Affected::Velocity => {
                wvx += p.wander[0];
                wvy += p.wander[1];
            }
            Affected::Acceleration => {
                wax += p.wander[0];
                way += p.wander[1];
            }
        }
    }
    p.vx += (p.ax + wax) * STEP;
    p.vy += (p.ay + way) * STEP;
    p.x += (p.vx + wvx) * STEP + dx;
    p.y += (p.vy + wvy) * STEP + dy;
    true
}

/// Where in a `w` by `h` emitter a particle is born.
#[derive(Debug, Clone)]
pub(crate) enum Shape {
    Rect,
    Ellipse { fill: bool },
    Mask(Rc<Mask>),
}

/// The part of a picture a `MaskShape` emits from.
#[derive(Debug)]
pub(crate) struct Mask {
    pub(crate) width: u32,
    pub(crate) height: u32,
    /// One alpha byte a pixel.
    pub(crate) alpha: Vec<u8>,
}

impl Shape {
    pub(crate) fn extrude(&self, rng: &mut Rng, w: f64, h: f64) -> (f64, f64) {
        match self {
            Shape::Rect => (rng.unit() * w, rng.unit() * h),
            Shape::Ellipse { fill } => {
                let angle = rng.unit() * std::f64::consts::TAU;
                let r = if *fill { rng.unit().sqrt() } else { 1.0 };
                (
                    w * 0.5 * (1.0 + r * angle.cos()),
                    h * 0.5 * (1.0 + r * angle.sin()),
                )
            }
            Shape::Mask(m) => {
                if m.width == 0 || m.height == 0 {
                    return (rng.unit() * w, rng.unit() * h);
                }
                // A picture that is mostly clear still gives a point; after
                // this many misses the last throw stands.
                let (mut x, mut y) = (rng.unit(), rng.unit());
                for _ in 0..64 {
                    let px = ((x * f64::from(m.width)) as u32).min(m.width - 1);
                    let py = ((y * f64::from(m.height)) as u32).min(m.height - 1);
                    if m.alpha[(py * m.width + px) as usize] > 0 {
                        break;
                    }
                    x = rng.unit();
                    y = rng.unit();
                }
                (x * w, y * h)
            }
        }
    }
}

/// A `Direction` as numbers, read once for a step.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) enum Dir {
    #[default]
    Null,
    Point {
        x: f64,
        y: f64,
        x_var: f64,
        y_var: f64,
    },
    Angle {
        angle: f64,
        magnitude: f64,
        angle_var: f64,
        magnitude_var: f64,
    },
    Target {
        x: f64,
        y: f64,
        target_var: f64,
        magnitude: f64,
        magnitude_var: f64,
        proportional: bool,
    },
}

impl Dir {
    /// The vector for a particle born at `at`, in the emitter's coordinates.
    pub(crate) fn vector(&self, rng: &mut Rng, at: (f64, f64)) -> (f64, f64) {
        match *self {
            Dir::Null => (0.0, 0.0),
            Dir::Point { x, y, x_var, y_var } => (x + rng.signed(x_var), y + rng.signed(y_var)),
            Dir::Angle {
                angle,
                magnitude,
                angle_var,
                magnitude_var,
            } => {
                let a = (angle + rng.signed(angle_var)).to_radians();
                let m = magnitude + rng.signed(magnitude_var);
                (a.cos() * m, a.sin() * m)
            }
            Dir::Target {
                x,
                y,
                target_var,
                magnitude,
                magnitude_var,
                proportional,
            } => {
                let tx = x + rng.signed(target_var);
                let ty = y + rng.signed(target_var);
                let (dx, dy) = (tx - at.0, ty - at.1);
                let len = dx.hypot(dy);
                let m = magnitude + rng.signed(magnitude_var);
                if len == 0.0 {
                    (0.0, 0.0)
                } else if proportional {
                    (dx * m, dy * m)
                } else {
                    (dx / len * m, dy / len * m)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at_rest(life: f64) -> Particle {
        Particle {
            x: 0.0,
            y: 0.0,
            vx: 0.0,
            vy: 0.0,
            ax: 0.0,
            ay: 0.0,
            age: 0.0,
            life,
            size: 10.0,
            end_size: 10.0,
            group: Rc::from(""),
            emitter: ObjectId::default(),
            dice: [0.0; 4],
            wander: [0.0; 2],
        }
    }

    #[test]
    fn the_same_seed_throws_the_same_dice() {
        let (mut a, mut b) = (Rng::new(SEED), Rng::new(SEED));
        for _ in 0..16 {
            assert_eq!(a.unit().to_bits(), b.unit().to_bits());
        }
        let mut c = Rng::new(SEED + 1);
        assert_ne!(Rng::new(SEED).unit().to_bits(), c.unit().to_bits());
    }

    #[test]
    fn dice_stay_in_range() {
        let mut r = Rng::new(7);
        for _ in 0..1000 {
            let u = r.unit();
            assert!((0.0..1.0).contains(&u));
            assert!((-3.0..3.0).contains(&r.signed(3.0)));
        }
    }

    #[test]
    fn a_particle_moves_by_velocity_and_dies_with_its_life() {
        let mut rng = Rng::new(1);
        let mut p = at_rest(1.0);
        p.vx = 100.0;
        let mut steps = 0;
        while advance(&mut p, &[], &mut rng) {
            steps += 1;
        }
        assert!((60..=63).contains(&steps));
        assert!((p.x - 100.0 * STEP * f64::from(steps)).abs() < 1e-9);
    }

    #[test]
    fn acceleration_changes_velocity() {
        let mut rng = Rng::new(1);
        let mut p = at_rest(10.0);
        p.ay = 100.0;
        for _ in 0..10 {
            advance(&mut p, &[], &mut rng);
        }
        assert!((p.vy - 100.0 * STEP * 10.0).abs() < 1e-9);
        assert!(p.y > 0.0);
    }

    #[test]
    fn a_wander_stays_within_its_variance() {
        let mut rng = Rng::new(3);
        let w = Wandering {
            pace: 500.0,
            x_variance: 5.0,
            y_variance: 0.0,
            affected: Affected::Position,
        };
        let mut p = at_rest(100.0);
        for _ in 0..500 {
            advance(&mut p, &[w], &mut rng);
            assert!(p.wander[0].abs() <= 5.0 + 1e-9);
            assert_eq!(p.wander[1], 0.0);
        }
    }

    #[test]
    fn an_ellipse_stays_inside_its_box_and_a_filled_one_fills_it() {
        let mut rng = Rng::new(9);
        let mut near_centre = false;
        for _ in 0..500 {
            let (x, y) = (Shape::Ellipse { fill: true }).extrude(&mut rng, 100.0, 50.0);
            assert!((0.0..=100.0).contains(&x) && (0.0..=50.0).contains(&y));
            let (dx, dy) = ((x - 50.0) / 50.0, (y - 25.0) / 25.0);
            assert!(dx * dx + dy * dy <= 1.0 + 1e-9);
            near_centre |= dx * dx + dy * dy < 0.25;
            let (x, y) = (Shape::Ellipse { fill: false }).extrude(&mut rng, 100.0, 50.0);
            let (dx, dy) = ((x - 50.0) / 50.0, (y - 25.0) / 25.0);
            assert!((dx * dx + dy * dy - 1.0).abs() < 1e-9);
        }
        assert!(near_centre);
    }

    #[test]
    fn a_mask_emits_from_what_is_not_clear() {
        let mut alpha = vec![0u8; 4 * 4];
        for y in 0..2 {
            for x in 0..2 {
                alpha[y * 4 + x] = 255;
            }
        }
        let m = Shape::Mask(Rc::new(Mask {
            width: 4,
            height: 4,
            alpha,
        }));
        let mut rng = Rng::new(5);
        for _ in 0..200 {
            let (x, y) = m.extrude(&mut rng, 40.0, 40.0);
            assert!(x < 20.0 && y < 20.0, "({x}, {y})");
        }
    }

    #[test]
    fn directions_point_where_they_say() {
        let mut rng = Rng::new(2);
        let (x, y) = (Dir::Point {
            x: 3.0,
            y: -4.0,
            x_var: 0.0,
            y_var: 0.0,
        })
        .vector(&mut rng, (0.0, 0.0));
        assert_eq!((x, y), (3.0, -4.0));
        let (x, y) = (Dir::Angle {
            angle: 90.0,
            magnitude: 10.0,
            angle_var: 0.0,
            magnitude_var: 0.0,
        })
        .vector(&mut rng, (0.0, 0.0));
        assert!(x.abs() < 1e-9 && (y - 10.0).abs() < 1e-9);
        let (x, y) = (Dir::Target {
            x: 10.0,
            y: 0.0,
            target_var: 0.0,
            magnitude: 5.0,
            magnitude_var: 0.0,
            proportional: false,
        })
        .vector(&mut rng, (0.0, 0.0));
        assert!((x - 5.0).abs() < 1e-9 && y.abs() < 1e-9);
    }

    #[test]
    fn the_entry_effect_is_nothing_at_the_ends_and_whole_between() {
        assert_eq!(entry_factor(0.0, 1.0), 0.0);
        assert_eq!(entry_factor(0.5, 1.0), 1.0);
        assert_eq!(entry_factor(1.0, 1.0), 0.0);
    }
}
