//! `ParticleSystem3D`: the time particles live in, and the step that brings
//! them to it.

use std::collections::HashMap;
use std::collections::hash_map::RandomState;
use std::hash::BuildHasher;
use std::time::Instant;

use ljos::item::ljos_3d::math::{Mat4, Vec3};
use ljos::item::ljos_3d::particles::{
    Affector, Aged, Counter, Emitter, EmitterState, Follower, Lines, Look, Seen, Table,
    burst as emit_burst, hidden_in, keep_up, look as seen_at, reset_state, run_emitter, run_trail,
};
use ljos::item::{FrameAnimation, frame_time_due, frames_only_while};
use ljos::ljos_engine::{Prop, WriteKind};
use ljos::prelude::*;

use super::affectors;
use super::emitters::{self, ParticleEmitter3D, TrailEmitter3D};
use super::kinds::{self, ModelParticle3D, SpriteParticle3D};
use crate::Node;
use crate::space;

/// What a system says of itself while it is `logging`.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick3D.Particles3D",
    name = "ParticleSystem3DLogging",
    uncreatable
)]
pub struct ParticleSystem3DLogging {
    #[qml(notify)]
    pub logging_interval: i32,
    #[qml(notify, readonly)]
    pub updates: i32,
    #[qml(notify, readonly)]
    pub particles_max: i32,
    #[qml(notify, readonly)]
    pub particles_used: i32,
    #[qml(notify, readonly)]
    pub time: f64,
    #[qml(notify, readonly)]
    pub time_average: f64,
    #[qml(notify, readonly)]
    pub time_deviation: f64,
}

impl Default for ParticleSystem3DLogging {
    fn default() -> Self {
        ParticleSystem3DLogging {
            logging_interval: 1000,
            updates: 0,
            particles_max: 0,
            particles_used: 0,
            time: 0.0,
            time_average: 0.0,
            time_deviation: 0.0,
        }
    }
}

#[derive(Debug, Default)]
struct KindState {
    table: Table,
    aged: Vec<Aged>,
    lines: Lines,
}

/// What the simulation of a system keeps between steps.
#[derive(Debug, Default)]
struct Sim {
    counter: Counter,
    before: f64,
    at: Option<f64>,
    begun: bool,
    states: HashMap<ObjectId, EmitterState>,
    kinds: HashMap<ObjectId, KindState>,
    /// Since `logging` last reported.
    log: LogSince,
}

#[derive(Debug, Default)]
struct LogSince {
    updates: u32,
    longest: f64,
    whole: f64,
    squares: f64,
    last: f64,
}

/// `ParticleSystem3D`.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick3D.Particles3D", name = "ParticleSystem3D", extends = Node)]
pub struct ParticleSystem3D {
    #[qml(base)]
    pub node: Node,
    #[qml(notify)]
    pub running: bool,
    #[qml(notify)]
    pub paused: bool,
    #[qml(notify)]
    pub start_time: i32,
    #[qml(notify)]
    pub time: i32,
    #[qml(notify)]
    pub use_random_seed: bool,
    #[qml(notify)]
    pub seed: i32,
    #[qml(notify)]
    pub logging: bool,
    #[qml(notify, readonly, object = "ParticleSystem3DLogging")]
    pub logging_data: Option<ObjectId>,
    /// Taken out while a step works: an engine's write may not happen while
    /// an object is borrowed.
    #[qml(skip)]
    sim: Option<Box<Sim>>,
    #[qml(skip)]
    emitters: Vec<ObjectId>,
    #[qml(skip)]
    affectors: Vec<ObjectId>,
    #[qml(skip)]
    clock: Option<ObjectId>,
    /// The milliseconds that have gone by since it was set running.
    #[qml(skip)]
    gone: f64,
    /// The `time` it was declared with, until `time` is another: until then
    /// the system is at nought.
    #[qml(skip)]
    declared: Option<i32>,
    /// Whether it was declared with a time: at nought that counts as begun.
    #[qml(skip)]
    timed: bool,
    /// The chance it was given, for when it is told to use it.
    #[qml(skip)]
    chance: u32,
}

impl Default for ParticleSystem3D {
    fn default() -> Self {
        ParticleSystem3D {
            node: Node::default(),
            running: true,
            paused: false,
            start_time: 0,
            time: 0,
            use_random_seed: true,
            seed: 0,
            logging: false,
            logging_data: None,
            sim: None,
            emitters: Vec::new(),
            affectors: Vec::new(),
            clock: None,
            gone: 0.0,
            declared: None,
            timed: false,
            chance: 0,
        }
    }
}

#[qml_impl]
impl ParticleSystem3D {
    /// Empties the system: no particle is left, and what the time has passed
    /// comes again. What there was is still to be seen until the time moves.
    #[qml(invokable, name = "reset")]
    fn reset_all(cx: &Cx) {
        reset(cx.engine, cx.this);
    }

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, me) = (cx.engine, cx.this);
        let declared = e.get(me, ParticleSystem3D::TIME)?;
        let log = e.create_native(ParticleSystem3DLogging::default(), Some(me))?;
        e.set_internal(me, ParticleSystem3D::LOGGING_DATA, Some(log))?;
        let clock = e.create_native(FrameAnimation::default(), Some(me))?;
        // Its time is read by its step while it is shown, or by whatever
        // else reads it. Hidden and read by nobody, the clock rides along
        // with the frames others ask for and asks for none.
        frames_only_while(e, clock, move |e| {
            e.is_observed(me, ParticleSystem3D::TIME.index)
        });
        cx.with_mut(|s: &mut ParticleSystem3D| {
            s.clock = Some(clock);
            s.declared = Some(declared);
            s.timed = declared != 0;
            s.chance = (RandomState::new().hash_one(0u8) & 0x7fff_ffff) as u32;
            s.sim = Some(Box::default());
        })?;
        e.connect_by_name(clock, "triggered", move |e, _| tick(e, me))?;
        let mut ran: Option<bool> = None;
        e.effect_in(me, move |e| {
            let running = e.get(me, ParticleSystem3D::RUNNING).unwrap_or(false);
            let paused = e.get(me, ParticleSystem3D::PAUSED).unwrap_or(false);
            e.untrack(|| {
                if ran != Some(running) {
                    let again = ran.is_some();
                    ran = Some(running);
                    if again {
                        let _ = e.set_internal(me, ParticleSystem3D::PAUSED, false);
                    }
                    if running {
                        if again {
                            reset(e, me);
                        }
                        let _ = e.with_mut(me, |s: &mut ParticleSystem3D| s.gone = 0.0);
                        let _ = e.set_internal(me, ParticleSystem3D::TIME, 0);
                    }
                }
                // A clock that rode along without frames has run since its
                // last one: that time is the system's before the clock stops.
                if !(running && !paused) {
                    bank(e, me, clock);
                }
                // A write of `paused` above comes back here with it false.
                let _ = e.write(
                    clock,
                    FrameAnimation::RUNNING.index,
                    Value::Bool(running && !paused),
                    WriteKind::Host,
                );
            });
        })?;
        e.effect_in(me, move |e| step(e, me))?;
        Ok(())
    }
}

fn tick(e: &Engine, me: ObjectId) {
    let Some(clock) = e.with(me, |s: &ParticleSystem3D| s.clock).ok().flatten() else {
        return;
    };
    let delta = e.get(clock, FrameAnimation::FRAME_TIME).unwrap_or(0.0) * 1000.0;
    let Ok(gone) = e.with_mut(me, |s: &mut ParticleSystem3D| {
        s.gone += delta;
        s.gone
    }) else {
        return;
    };
    let _ = e.set_internal(me, ParticleSystem3D::TIME, gone.floor() as i32);
    if !e.is_observed(me, ParticleSystem3D::TIME.index) {
        let _ = e.make_on_read_by(me, ParticleSystem3D::TIME.index, made_time);
    }
}

/// Adds to the system's time what its clock has run since its last frame.
/// Nought unless the clock runs.
fn bank(e: &Engine, me: ObjectId, clock: ObjectId) {
    let due = frame_time_due(e, clock) * 1000.0;
    if due <= 0.0 {
        return;
    }
    let Ok(gone) = e.with_mut(me, |s: &mut ParticleSystem3D| {
        s.gone += due;
        s.gone
    }) else {
        return;
    };
    let _ = e.set_internal(me, ParticleSystem3D::TIME, gone.floor() as i32);
}

/// The time of a system whose clock rides along without asking for frames:
/// what it was at the clock's last frame and what has gone by since. A
/// script that reads it while nothing else does sees it go on.
fn made_time(e: &Engine, me: ObjectId, prop: PropIndex) -> Value {
    // Read again, it is made again.
    let _ = e.make_on_read_by(me, prop, made_time);
    let Ok((gone, clock)) = e.with(me, |s: &ParticleSystem3D| (s.gone, s.clock)) else {
        return Value::Int(0);
    };
    let due = clock.map_or(0.0, |c| frame_time_due(e, c)) * 1000.0;
    Value::Int((gone + due).floor() as i32)
}

fn reset(e: &Engine, me: ObjectId) {
    let _ = e.with_mut(me, |s: &mut ParticleSystem3D| {
        if let Some(sim) = &mut s.sim {
            for kind in sim.kinds.values_mut() {
                kind.table.clear();
            }
            for state in sim.states.values_mut() {
                reset_state(state);
            }
            sim.before = 0.0;
            sim.counter.numbered = 0;
        }
    });
}

/// What comes to a system.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Member {
    Emitter,
    Affector,
}

/// Has `me` come to the system `system` says, or the parent it has when that
/// is one, and go when either changes or it is destroyed.
pub(super) fn enrol(
    e: &Engine,
    me: ObjectId,
    system: Prop<Option<ObjectId>>,
    member: Member,
) -> Result<(), EngineError> {
    let mut joined: Option<ObjectId> = None;
    e.effect_in(me, move |e| {
        let told = e.get(me, system).ok().flatten();
        let now =
            told.or_else(|| space::parent_node(e, me).filter(|p| e.is::<ParticleSystem3D>(*p)));
        if now == joined {
            return;
        }
        let before = std::mem::replace(&mut joined, now);
        e.untrack(|| {
            if let Some(old) = before {
                leave(e, old, me, member);
            }
            if let Some(new) = now {
                let _ = e.with_mut(new, |s: &mut ParticleSystem3D| {
                    let list = match member {
                        Member::Emitter => &mut s.emitters,
                        Member::Affector => &mut s.affectors,
                    };
                    list.push(me);
                });
                let _ = e.notify(new, ParticleSystem3D::LOGGING_DATA.index);
            }
            if told.is_none() {
                let _ = e.set_internal(me, system, now);
            }
        });
    })?;
    e.connect_by_name(me, "destroyed", move |e, _| {
        let Some(old) = e.untrack(|| e.get(me, system).ok().flatten()) else {
            return;
        };
        leave(e, old, me, member);
    })?;
    Ok(())
}

fn leave(e: &Engine, system: ObjectId, me: ObjectId, member: Member) {
    if !e.is_alive(system) {
        return;
    }
    let _ = e.with_mut(system, |s: &mut ParticleSystem3D| {
        let list = match member {
            Member::Emitter => &mut s.emitters,
            Member::Affector => &mut s.affectors,
        };
        list.retain(|m| *m != me);
    });
    let _ = e.notify(system, ParticleSystem3D::LOGGING_DATA.index);
}

/// Whether its clock runs, so that its time is the clock's. Tracked: a
/// clock that starts again while the system is hidden runs the step again,
/// which makes the time from then on, frames or none.
fn ticking(e: &Engine, me: ObjectId) -> bool {
    let Some(clock) = e.with(me, |s: &ParticleSystem3D| s.clock).ok().flatten() else {
        return false;
    };
    e.get(clock, FrameAnimation::RUNNING).unwrap_or(false)
}

/// Whether the system and each node it is in are visible. Tracked: shown
/// again, the step runs.
fn shown(e: &Engine, me: ObjectId) -> bool {
    let mut at = Some(me);
    while let Some(o) = at {
        if !e.get(o, Node::VISIBLE).unwrap_or(true) {
            return false;
        }
        at = space::parent_node(e, o);
    }
    true
}

/// The time without `startTime`: nought until `time` is another than it was
/// declared with.
fn time_of(e: &Engine, me: ObjectId) -> i32 {
    let time = e.get(me, ParticleSystem3D::TIME).unwrap_or(0);
    let declared = e.with(me, |s: &ParticleSystem3D| s.declared).ok().flatten();
    match declared {
        Some(d) if d == time => 0,
        Some(_) => {
            let _ = e.with_mut(me, |s: &mut ParticleSystem3D| s.declared = None);
            time
        }
        None => time,
    }
}

fn seed_of(e: &Engine, me: ObjectId) -> u32 {
    if e.get(me, ParticleSystem3D::USE_RANDOM_SEED).unwrap_or(true) {
        e.with(me, |s: &ParticleSystem3D| s.chance).unwrap_or(0)
    } else {
        e.get(me, ParticleSystem3D::SEED).unwrap_or(0) as u32
    }
}

/// How `node` lies seen from its system.
fn from_system(e: &Engine, me: ObjectId) -> (Mat4, ljos::item::ljos_3d::math::Quat) {
    let place = space::place(e, me);
    (
        place.world.inverse().unwrap_or(Mat4::IDENTITY),
        place.rotation,
    )
}

/// An emitter as it is read for one step.
struct Read {
    id: ObjectId,
    emitter: Emitter,
    kind: Option<ObjectId>,
    /// What it follows, for a trail emitter.
    follow: Option<Option<ObjectId>>,
}

/// A kind as it is read for one step.
struct KindRead {
    id: ObjectId,
    look: Look,
    sprite: bool,
    affecting: Vec<Affector>,
}

/// Brings the system to its time and fills the tables. Run by its effect when
/// anything it reads changes: the time above all.
fn step(e: &Engine, me: ObjectId) {
    let started = Instant::now();
    let logging = e.get(me, ParticleSystem3D::LOGGING).unwrap_or(false);
    // Hidden, it shows nothing: what it would have done by its time is
    // worked out once it is shown. Its time, unread, asks for no frames,
    // and is worked out when something reads it. Only a clock that runs
    // owns the time: a stopped or paused system keeps what it was given.
    if !logging && !shown(e, me) {
        if ticking(e, me) {
            let _ = e.make_on_read_by(me, ParticleSystem3D::TIME.index, made_time);
        }
        return;
    }
    // Told when what is in the system changes.
    let _ = e.get(me, ParticleSystem3D::LOGGING_DATA);
    let t = time_of(e, me);
    let now = f64::from(t) + f64::from(e.get(me, ParticleSystem3D::START_TIME).unwrap_or(0));
    let running = e.get(me, ParticleSystem3D::RUNNING).unwrap_or(false);
    let seed = seed_of(e, me);
    let Ok((members, others, timed)) = e.with(me, |s: &ParticleSystem3D| {
        (s.emitters.clone(), s.affectors.clone(), s.timed)
    }) else {
        return;
    };
    // The last that came goes first: each comes when it is complete, and
    // Qt's work in the order the last made is complete first.
    let (from, turn) = from_system(e, me);
    let mut reads: Vec<Read> = members
        .iter()
        .rev()
        .filter(|m| e.is_alive(**m))
        .map(|&id| Read {
            id,
            emitter: emitters::read(e, id, from, turn),
            kind: e.get(id, ParticleEmitter3D::PARTICLE).ok().flatten(),
            follow: e
                .is::<TrailEmitter3D>(id)
                .then(|| e.get(id, TrailEmitter3D::FOLLOW).ok().flatten()),
        })
        .collect();
    reads.retain(|r| r.kind.is_some());
    let mut kinds_read: Vec<KindRead> = Vec::new();
    for kind in reads.iter().filter_map(|r| r.kind) {
        if kinds_read.iter().any(|k| k.id == kind) {
            continue;
        }
        e.untrack(|| kinds::attach(e, kind, me));
        let affecting = others
            .iter()
            .rev()
            .filter(|a| e.is_alive(**a) && affectors::affects(e, **a, kind))
            .filter_map(|a| affectors::read(e, *a, from))
            .collect();
        kinds_read.push(KindRead {
            id: kind,
            look: kinds::look(e, kind),
            sprite: e.is::<SpriteParticle3D>(kind),
            affecting,
        });
    }
    let asked: HashMap<ObjectId, Vec<(u32, f64)>> = reads
        .iter()
        .filter(|r| r.follow.is_some())
        .filter_map(|r| {
            e.with(r.id, |t: &TrailEmitter3D| t.asked.clone())
                .ok()
                .map(|a| (r.id, a))
        })
        .collect();
    // Colours and fades may be read and written by what is told below, so
    // nothing of the engine is touched while the simulation is taken out.
    let Some(mut sim) = e
        .with_mut(me, |s: &mut ParticleSystem3D| s.sim.take())
        .ok()
        .flatten()
    else {
        return;
    };
    for r in &reads {
        let state = sim
            .states
            .entry(r.id)
            .or_insert_with(|| EmitterState::starting(sim.before));
        keep_up(&r.emitter, state, sim.before);
    }
    let not_yet = !sim.begun && now == 0.0 && !running && !timed && others.is_empty();
    let mut emitted = false;
    // A system's members come to it as they complete: nothing is emitted or
    // counted as done before there is one.
    if !not_yet && !reads.is_empty() {
        sim.begun = true;
        if sim.at != Some(now) {
            emit_all(
                &mut sim,
                &reads,
                &kinds_read,
                &asked,
                seed,
                now,
                f64::from(t),
            );
            sim.before = now;
            sim.at = Some(now);
            sim.log.updates += 1;
            emitted = true;
        }
    }
    // What is seen of each kind, sorted as it says.
    let time = (now / 1000.0) as f32;
    let step_number = sim.counter.updates;
    let mut seen_of: Vec<(
        ObjectId,
        Vec<Seen>,
        Vec<ljos::item::ljos_3d::particles::Particle>,
    )> = Vec::new();
    for k in &kinds_read {
        let state = sim.kinds.entry(k.id).or_default();
        let mode = e.untrack(|| kinds::sort_mode(e, k.id));
        let most = k.look.max_amount.min(state.table.slots.len());
        let mut seen = Vec::with_capacity(most);
        let mut particles = Vec::with_capacity(most);
        for i in 0..most {
            let place = match mode {
                1 => state.table.newest_first(i, most),
                2 => state.table.oldest_first(i, most),
                _ => i,
            };
            let Some(Some(p)) = state.table.slots.get(place) else {
                continue;
            };
            if hidden_in(p, step_number) {
                continue;
            }
            if let Some(s) = seen_at(p, place, time, seed, &k.look, &k.affecting) {
                seen.push(s);
                particles.push(*p);
            }
        }
        seen_of.push((k.id, seen, particles));
    }
    let mut rows = Vec::with_capacity(seen_of.len());
    for (id, seen, particles) in seen_of {
        let state = sim.kinds.entry(id).or_default();
        if let Some((look, stretch)) = kinds::line_look(e, id) {
            state.lines.trace(seed, &state.table, &seen, now, &look);
            rows.push((id, kinds::fill_lines(&state.lines, now, &look, stretch)));
        } else if e.is::<SpriteParticle3D>(id) {
            let sequence = kinds::sequence(e, id);
            let framed: Vec<(Seen, f32)> = seen
                .iter()
                .zip(&particles)
                .map(|(s, p)| {
                    let frame = sequence
                        .as_ref()
                        .map_or(0.0, |q| q.through(seed, p.index, p.life, s.seconds));
                    (*s, frame)
                })
                .collect();
            rows.push((id, kinds::fill_sprites(e, id, &framed)));
        } else if e.is::<ModelParticle3D>(id) {
            let plain: Vec<(Seen, f32)> = seen.iter().map(|s| (*s, 0.0)).collect();
            rows.push((id, kinds::fill_models(&plain)));
        }
    }
    let used: i32 = rows.iter().map(|(_, r)| r.1.len() as i32).sum();
    let wanted: i32 = kinds_read.iter().map(|k| k.look.max_amount as i32).sum();
    let took = started.elapsed().as_secs_f64() * 1000.0;
    let report = logging.then(|| log_step(&mut sim, now, took, e, me, used, wanted));
    let _ = e.with_mut(me, |s: &mut ParticleSystem3D| s.sim = Some(sim));
    e.untrack(|| {
        e.batch(|| {
            if emitted {
                for id in asked.keys() {
                    let _ = e.with_mut(*id, |t: &mut TrailEmitter3D| t.asked.clear());
                }
            }
            for (id, r) in rows {
                kinds::show(e, id, r);
            }
            if let Some(Some(write)) = report {
                write_log(e, me, write);
            }
        });
    });
}

fn emit_all(
    sim: &mut Sim,
    reads: &[Read],
    kinds_read: &[KindRead],
    asked: &HashMap<ObjectId, Vec<(u32, f64)>>,
    seed: u32,
    now: f64,
    time: f64,
) {
    let Sim {
        counter,
        states,
        kinds,
        ..
    } = sim;
    counter.updates += 1;
    for r in reads
        .iter()
        .filter(|r| r.follow.is_none() && r.emitter.enabled)
    {
        let (Some(kind), Some(state)) = (r.kind, states.get_mut(&r.id)) else {
            continue;
        };
        let Some(k) = kinds_read.iter().find(|k| k.id == kind) else {
            continue;
        };
        let table = &mut kinds.entry(kind).or_default().table;
        run_emitter(counter, seed, &r.emitter, &k.look, state, table, now, time);
    }
    if !reads.iter().any(|r| r.follow.is_some()) {
        return;
    }
    // Those that follow particles come after those that make them, a kind at
    // a time, with all the tables at hand as they may write in any of them.
    let looks: Vec<Look> = kinds_read.iter().map(|k| k.look).collect();
    let mut tables: Vec<Table> = kinds_read
        .iter()
        .map(|k| std::mem::take(&mut kinds.entry(k.id).or_default().table))
        .collect();
    for (number, k) in kinds_read.iter().enumerate() {
        let followers: Vec<&Read> = reads
            .iter()
            .filter(|r| r.follow == Some(Some(k.id)))
            .collect();
        if followers.is_empty() {
            continue;
        }
        let mut taken: Vec<(ObjectId, EmitterState)> = followers
            .iter()
            .filter_map(|r| states.remove(&r.id).map(|s| (r.id, s)))
            .collect();
        let nothing = Vec::new();
        let mut list: Vec<Follower<'_>> = taken
            .iter_mut()
            .filter_map(|(id, state)| {
                let r = followers.iter().find(|r| r.id == *id)?;
                let into = kinds_read.iter().position(|k| Some(k.id) == r.kind)?;
                Some(Follower {
                    emitter: &r.emitter,
                    state,
                    into,
                    asked: asked.get(id).unwrap_or(&nothing),
                })
            })
            .collect();
        let mut aged = std::mem::take(&mut kinds.entry(k.id).or_default().aged);
        run_trail(
            counter,
            seed,
            &mut tables,
            &looks,
            number,
            k.sprite,
            &mut aged,
            &mut list,
            &k.affecting,
            now,
            time,
        );
        drop(list);
        kinds.entry(k.id).or_default().aged = aged;
        states.extend(taken);
    }
    for (k, table) in kinds_read.iter().zip(tables) {
        kinds.entry(k.id).or_default().table = table;
    }
}

#[derive(Debug)]
struct LogWrite {
    log: ObjectId,
    updates: i32,
    max: i32,
    used: i32,
    longest: f64,
    average: f64,
    deviation: f64,
}

fn log_step(
    sim: &mut Sim,
    now: f64,
    took: f64,
    e: &Engine,
    me: ObjectId,
    used: i32,
    wanted: i32,
) -> Option<LogWrite> {
    let since = &mut sim.log;
    since.longest = since.longest.max(took);
    since.whole += took;
    since.squares += took * took;
    let log = e.untrack(|| e.get(me, ParticleSystem3D::LOGGING_DATA).ok().flatten())?;
    let every = f64::from(
        e.untrack(|| {
            e.get(log, ParticleSystem3DLogging::LOGGING_INTERVAL)
                .unwrap_or(1000)
        })
        .max(1),
    );
    if (now / every).floor() == (since.last / every).floor() && now >= since.last {
        return None;
    }
    since.last = now;
    let count = f64::from(since.updates);
    let average = if count > 0.0 {
        since.whole / count
    } else {
        0.0
    };
    let write = LogWrite {
        log,
        updates: since.updates as i32,
        max: wanted,
        used,
        longest: since.longest,
        average,
        deviation: if count > 0.0 {
            (since.squares / count - average * average).max(0.0).sqrt()
        } else {
            0.0
        },
    };
    *since = LogSince {
        last: now,
        ..LogSince::default()
    };
    Some(write)
}

fn write_log(e: &Engine, me: ObjectId, w: LogWrite) {
    use ParticleSystem3DLogging as L;
    let _ = me;
    let _ = e.set_internal(w.log, L::UPDATES, w.updates);
    let _ = e.set_internal(w.log, L::PARTICLES_MAX, w.max);
    let _ = e.set_internal(w.log, L::PARTICLES_USED, w.used);
    let _ = e.set_internal(w.log, L::TIME, w.longest);
    let _ = e.set_internal(w.log, L::TIME_AVERAGE, w.average);
    let _ = e.set_internal(w.log, L::TIME_DEVIATION, w.deviation);
}

/// The system an emitter is in, with the system brought up to date.
fn synced(e: &Engine, emitter: ObjectId) -> Option<(ObjectId, f64)> {
    let system = e.get(emitter, ParticleEmitter3D::SYSTEM).ok().flatten()?;
    step(e, system);
    let now = e.with(system, |s: &ParticleSystem3D| {
        s.sim.as_ref().and_then(|m| m.at)
    });
    Some((system, now.ok().flatten().unwrap_or(0.0)))
}

/// `ParticleEmitter3D.burst`.
pub(super) fn burst(e: &Engine, me: ObjectId, count: u32, duration: f64, position: Option<Vec3>) {
    e.untrack(|| {
        if !e.get(me, ParticleEmitter3D::ENABLED).unwrap_or(false) {
            return;
        }
        let Some(kind) = e.get(me, ParticleEmitter3D::PARTICLE).ok().flatten() else {
            return;
        };
        let Some((system, now)) = synced(e, me) else {
            return;
        };
        let (from, turn) = from_system(e, system);
        let emitter = emitters::read(e, me, from, turn);
        let look = kinds::look(e, kind);
        let seed = seed_of(e, system);
        let _ = e.with_mut(system, |s: &mut ParticleSystem3D| {
            if let Some(sim) = &mut s.sim {
                let Sim { counter, kinds, .. } = &mut **sim;
                let table = &mut kinds.entry(kind).or_default().table;
                emit_burst(
                    counter, seed, &emitter, &look, table, now, count, duration, position,
                );
            }
        });
    });
}

/// `TrailEmitter3D.burst`.
pub(super) fn ask(e: &Engine, me: ObjectId, count: u32) {
    e.untrack(|| {
        if !e.get(me, ParticleEmitter3D::ENABLED).unwrap_or(false) {
            return;
        }
        let Some((_, now)) = synced(e, me) else {
            return;
        };
        let _ = e.with_mut(me, |t: &mut TrailEmitter3D| t.asked.push((count, now)));
    });
}
