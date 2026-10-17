//! `QtQuick.Particles`, as far as the examples in the gallery reach:
//! `ParticleSystem`, `Emitter`, `ImageParticle`, `Wander`, the directions
//! `PointDirection`, `AngleDirection` and `TargetDirection`, and the shapes
//! `EllipseShape` and `MaskShape`.
//!
//! A system keeps its particles and advances them in fixed steps from an
//! engine timer it owns. The timer runs only while the system is running,
//! not paused, and has something to do (a live particle, or an emitter that
//! will emit); a system with nothing to do asks for nothing, which is how an
//! idle scene stays at no work.
//!
//! An `ImageParticle` paints itself (`crate::register_paint_item`): each
//! step makes one display list with a picture for each particle, which
//! the scene draws in the painter's node. An item for each particle
//! would run its own geometry, paint and bindings on every step; a burst
//! of a thousand is then a thousand of each.

mod sim;

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use ljos_engine::{
    Atom, Color, Cx, Engine, EngineError, LogLevel, MetaRef, NodeId, ObjectId, PropIndex,
    QmlObject, QmlType, QtObject, Timer, Url, Value, qml_impl,
};
use ljos_render::{DisplayListBuilder, DrawOp, ImageHandle, Rgba, kurbo};
use rustc_hash::FxHashMap;

use self::sim::{Affected, Dir, Mask, Particle, Rng, SEED, STEP, Shape, Wandering};
use crate::enums::EnumTable;
use crate::image::{ImageKey, ImageStore};
use crate::item::{Item, is_a, write_computed};
use crate::paint_item::{ItemPaint, register_paint_item};
use ljos_text::TextEngine;

const CATEGORY: &str = "particles";

/// How many whole steps a system may simulate in one wake; a host that was
/// away longer than this has the rest skipped instead of replayed.
const WARM_STEPS_MAX: usize = 4000;

// ------------------------------------------------------------ types

/// `ParticleSystem`.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick.Particles", name = "ParticleSystem", extends = Item)]
pub struct ParticleSystem {
    #[qml(base)]
    pub item: Item,
    #[qml(notify)]
    pub running: bool,
    #[qml(notify)]
    pub paused: bool,
    /// Whether no particle is alive.
    #[qml(notify, readonly)]
    pub empty: bool,
    #[qml(skip)]
    run: Rc<RefCell<SystemRun>>,
}

impl Default for ParticleSystem {
    fn default() -> Self {
        Self {
            item: Item::default(),
            running: true,
            paused: false,
            empty: true,
            run: Rc::default(),
        }
    }
}

#[qml_impl]
impl ParticleSystem {
    #[qml(signal, name = "systemInitialized")]
    fn system_initialized();

    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, sys) = (cx.engine, cx.this);
        e.effect_in(sys, move |e| {
            if e.is_alive(sys) {
                let _ = e.read(sys, Self::RUNNING.index);
                let _ = e.read(sys, Self::PAUSED.index);
                sync_timer(e, sys);
            }
        })?;
        Ok(())
    }

    #[qml(invokable)]
    fn start(cx: &Cx) {
        let _ = cx.engine.set(cx.this, Self::RUNNING, true);
    }

    #[qml(invokable)]
    fn stop(cx: &Cx) {
        let _ = cx.engine.set(cx.this, Self::RUNNING, false);
    }

    #[qml(invokable)]
    fn restart(cx: &Cx) {
        reset(cx.engine, cx.this);
        let _ = cx.engine.set(cx.this, Self::RUNNING, true);
    }

    #[qml(invokable)]
    fn pause(cx: &Cx) {
        let _ = cx.engine.set(cx.this, Self::PAUSED, true);
    }

    #[qml(invokable)]
    fn resume(cx: &Cx) {
        let _ = cx.engine.set(cx.this, Self::PAUSED, false);
    }

    /// Every particle dies at once.
    #[qml(invokable)]
    fn reset(cx: &Cx) {
        reset(cx.engine, cx.this);
    }
}

/// The base of the shapes an `Emitter` emits from.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtQuick.Particles",
    name = "ParticleExtruder",
    extends = QtObject,
    uncreatable
)]
pub struct ParticleExtruder {
    #[qml(base)]
    pub object: QtObject,
}

#[qml_impl]
impl ParticleExtruder {}

/// `EllipseShape`.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick.Particles", name = "EllipseShape", extends = ParticleExtruder)]
pub struct EllipseShape {
    #[qml(base)]
    pub extruder: ParticleExtruder,
    #[qml(notify)]
    pub fill: bool,
}

impl Default for EllipseShape {
    fn default() -> Self {
        Self {
            extruder: ParticleExtruder::default(),
            fill: true,
        }
    }
}

#[qml_impl]
impl EllipseShape {}

/// `MaskShape`.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick.Particles", name = "MaskShape", extends = ParticleExtruder)]
pub struct MaskShape {
    #[qml(base)]
    pub extruder: ParticleExtruder,
    #[qml(notify)]
    pub source: Url,
}

#[qml_impl]
impl MaskShape {}

/// The base of the vectors an `Emitter` takes its velocity and
/// acceleration from.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtQuick.Particles",
    name = "NullVector",
    extends = QtObject,
    uncreatable
)]
pub struct Direction {
    #[qml(base)]
    pub object: QtObject,
}

#[qml_impl]
impl Direction {}

/// `PointDirection`.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick.Particles", name = "PointDirection", extends = Direction)]
pub struct PointDirection {
    #[qml(base)]
    pub direction: Direction,
    #[qml(notify)]
    pub x: f64,
    #[qml(notify)]
    pub y: f64,
    #[qml(notify)]
    pub x_variation: f64,
    #[qml(notify)]
    pub y_variation: f64,
}

#[qml_impl]
impl PointDirection {}

/// `AngleDirection`. The angle is in degrees, clockwise from the right on
/// the screen.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick.Particles", name = "AngleDirection", extends = Direction)]
pub struct AngleDirection {
    #[qml(base)]
    pub direction: Direction,
    #[qml(notify)]
    pub angle: f64,
    #[qml(notify)]
    pub magnitude: f64,
    #[qml(notify)]
    pub angle_variation: f64,
    #[qml(notify)]
    pub magnitude_variation: f64,
}

#[qml_impl]
impl AngleDirection {}

/// `TargetDirection`.
#[derive(Debug, Default, QmlObject)]
#[qml(module = "QtQuick.Particles", name = "TargetDirection", extends = Direction)]
pub struct TargetDirection {
    #[qml(base)]
    pub direction: Direction,
    #[qml(notify)]
    pub target_x: f64,
    #[qml(notify)]
    pub target_y: f64,
    #[qml(notify)]
    pub target_variation: f64,
    #[qml(notify)]
    pub proportional_magnitude: bool,
    #[qml(notify)]
    pub magnitude: f64,
    #[qml(notify)]
    pub magnitude_variation: f64,
}

#[qml_impl]
impl TargetDirection {}

fn emitter_enums() -> EnumTable {
    vec![("Lifetime", vec![("InfiniteLife", 1_073_741_823)])]
}

/// `Emitter`.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick.Particles",
    name = "Emitter",
    extends = Item,
    enums = emitter_enums
)]
pub struct Emitter {
    #[qml(base)]
    pub item: Item,
    #[qml(notify, object = "ParticleSystem")]
    pub system: Option<ObjectId>,
    #[qml(notify)]
    pub group: String,
    #[qml(notify, object = "ParticleExtruder")]
    pub shape: Option<ObjectId>,
    /// Milliseconds the system is run ahead of its start, so that it opens
    /// already full.
    #[qml(notify)]
    pub start_time: i32,
    #[qml(notify)]
    pub emit_rate: f64,
    #[qml(notify)]
    pub life_span: i32,
    #[qml(notify)]
    pub life_span_variation: i32,
    #[qml(notify)]
    pub maximum_emitted: i32,
    #[qml(notify)]
    pub size: f64,
    #[qml(notify)]
    pub end_size: f64,
    #[qml(notify)]
    pub size_variation: f64,
    #[qml(notify, object = "NullVector")]
    pub velocity: Option<ObjectId>,
    #[qml(notify, object = "NullVector")]
    pub acceleration: Option<ObjectId>,
    #[qml(skip)]
    run: Rc<RefCell<EmitterRun>>,
}

impl Default for Emitter {
    fn default() -> Self {
        Self {
            item: Item::default(),
            system: None,
            group: String::new(),
            shape: None,
            start_time: 0,
            emit_rate: 10.0,
            life_span: 1000,
            life_span_variation: 0,
            maximum_emitted: -1,
            size: 16.0,
            end_size: -1.0,
            size_variation: 0.0,
            velocity: None,
            acceleration: None,
            run: Rc::default(),
        }
    }
}

/// What an emitter owes beyond its rate.
#[derive(Debug, Default)]
struct EmitterRun {
    /// Fractions of a particle the rate has earned and not yet paid.
    carry: f64,
    /// Bursts asked for, each with where to emit from if it said.
    bursts: Vec<(u32, Option<(f64, f64)>)>,
    /// Seconds of `pulse()` left; the emitter emits while any are.
    pulse_left: f64,
}

#[qml_impl]
impl Emitter {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, em) = (cx.engine, cx.this);
        registry(e).emitters.borrow_mut().push(em);
        e.effect_in(em, move |e| {
            if !e.is_alive(em) {
                return;
            }
            for p in [Self::SYSTEM.index, Self::START_TIME.index] {
                let _ = e.read(em, p);
            }
            let enabled = e.read(em, Item::ENABLED.index).to_boolean();
            if enabled && e.read(em, Self::EMIT_RATE.index).to_number() > 0.0 {
                enliven(e, em);
            }
            if let Some(sys) = system_of(e, em, Self::SYSTEM.index) {
                sync_timer(e, sys);
            }
        })?;
        Ok(())
    }

    /// Emits `count` particles now, from the shape or, when a point is
    /// given, from that point of the emitter.
    #[qml(invokable)]
    fn burst(cx: &Cx, count: i32, x: Value, y: Value) {
        let at = match (x.as_number(), y.as_number()) {
            (Some(x), Some(y)) => Some((x, y)),
            _ => None,
        };
        let n = count.max(0) as u32;
        if n > 0
            && let Some(run) = emitter_run(cx.engine, cx.this)
        {
            run.borrow_mut().bursts.push((n, at));
            enliven(cx.engine, cx.this);
            wake(cx.engine, cx.this);
        }
    }

    /// Emits for `duration` milliseconds whether or not it is enabled.
    #[qml(invokable)]
    fn pulse(cx: &Cx, duration: i32) {
        if duration > 0
            && let Some(run) = emitter_run(cx.engine, cx.this)
        {
            let s = f64::from(duration) / 1000.0;
            let mut r = run.borrow_mut();
            r.pulse_left = r.pulse_left.max(s);
            drop(r);
            enliven(cx.engine, cx.this);
            wake(cx.engine, cx.this);
        }
    }
}

/// The base of what draws particles.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtQuick.Particles",
    name = "ParticlePainter",
    extends = Item,
    uncreatable
)]
pub struct ParticlePainter {
    #[qml(base)]
    pub item: Item,
    #[qml(notify, object = "ParticleSystem")]
    pub system: Option<ObjectId>,
    /// The groups it draws; none draws the particles of the group `""`.
    #[qml(notify)]
    pub groups: Vec<Value>,
}

#[qml_impl]
impl ParticlePainter {}

fn image_particle_enums() -> EnumTable {
    vec![("EntryEffect", vec![("None", 0), ("Fade", 1), ("Scale", 2)])]
}

/// `ImageParticle`.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick.Particles",
    name = "ImageParticle",
    extends = ParticlePainter,
    enums = image_particle_enums
)]
pub struct ImageParticle {
    #[qml(base)]
    pub painter: ParticlePainter,
    #[qml(notify)]
    pub source: Url,
    #[qml(notify, reset)]
    pub color: Color,
    #[qml(notify, reset)]
    pub color_variation: f64,
    #[qml(notify, reset)]
    pub alpha: f64,
    #[qml(notify, reset)]
    pub alpha_variation: f64,
    #[qml(notify, enumeration = "EntryEffect")]
    pub entry_effect: i32,
}

impl Default for ImageParticle {
    fn default() -> Self {
        Self {
            painter: ParticlePainter::default(),
            source: Url::default(),
            color: Color::WHITE,
            color_variation: 0.0,
            alpha: 1.0,
            alpha_variation: 0.0,
            entry_effect: 1,
        }
    }
}

#[qml_impl]
impl ImageParticle {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        let (e, pa) = (cx.engine, cx.this);
        let reg = registry(e);
        reg.painters.borrow_mut().push(pa);
        let scope = e.object_scope(pa).unwrap_or_else(|| e.graph().root());
        let serial = e.graph().property(scope, Value::Int(0)).ok();
        let mut shown = reg.shown.borrow_mut();
        shown.retain(|p, _| e.is_alive(*p));
        shown.insert(
            pa,
            Shown {
                serial,
                ..Shown::default()
            },
        );
        drop(shown);
        // What it reads is what repainting follows: its look, and its place.
        e.effect_in(pa, move |e| {
            if !e.is_alive(pa) {
                return;
            }
            for p in [
                ParticlePainter::SYSTEM.index,
                ParticlePainter::GROUPS.index,
                Self::SOURCE.index,
                Self::COLOR.index,
                Self::COLOR_VARIATION.index,
                Self::ALPHA.index,
                Self::ALPHA_VARIATION.index,
                Self::ENTRY_EFFECT.index,
            ] {
                let _ = e.read(pa, p);
            }
            let _ = system_of(e, pa, ParticlePainter::SYSTEM.index);
            // Placed when the scene next asks, so that this follows only
            // the look and not every step.
            e.untrack(|| restale(e, pa));
        })?;
        Ok(())
    }
}

/// The base of what changes particles as they live.
#[derive(Debug, Default, QmlObject)]
#[qml(
    module = "QtQuick.Particles",
    name = "ParticleAffector",
    extends = Item,
    uncreatable
)]
pub struct ParticleAffector {
    #[qml(base)]
    pub item: Item,
    #[qml(notify, object = "ParticleSystem")]
    pub system: Option<ObjectId>,
    /// The groups it affects; none affects every group.
    #[qml(notify)]
    pub groups: Vec<Value>,
}

#[qml_impl]
impl ParticleAffector {}

/// `Age`: particles it reaches have at most `lifeLeft` milliseconds to
/// live. `advancePosition` is accepted and does not move the particle.
#[derive(Debug, QmlObject)]
#[qml(module = "QtQuick.Particles", name = "Age", extends = ParticleAffector)]
pub struct Age {
    #[qml(base)]
    pub affector: ParticleAffector,
    #[qml(notify)]
    pub life_left: i32,
    #[qml(notify)]
    pub advance_position: bool,
}

impl Default for Age {
    fn default() -> Self {
        Self {
            affector: ParticleAffector::default(),
            life_left: 0,
            advance_position: true,
        }
    }
}

#[qml_impl]
impl Age {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        registry(cx.engine).affectors.borrow_mut().push(cx.this);
        Ok(())
    }
}

fn wander_enums() -> EnumTable {
    vec![(
        "AffectableParameters",
        vec![("Position", 0), ("Velocity", 1), ("Acceleration", 2)],
    )]
}

/// `Wander`.
#[derive(Debug, QmlObject)]
#[qml(
    module = "QtQuick.Particles",
    name = "Wander",
    extends = ParticleAffector,
    enums = wander_enums
)]
pub struct Wander {
    #[qml(base)]
    pub affector: ParticleAffector,
    #[qml(notify)]
    pub pace: f64,
    #[qml(notify)]
    pub x_variance: f64,
    #[qml(notify)]
    pub y_variance: f64,
    #[qml(notify, enumeration = "AffectableParameters")]
    pub affected_parameter: i32,
}

impl Default for Wander {
    fn default() -> Self {
        Self {
            affector: ParticleAffector::default(),
            pace: 0.0,
            x_variance: 0.0,
            y_variance: 0.0,
            affected_parameter: 1,
        }
    }
}

#[qml_impl]
impl Wander {
    #[qml(on_complete)]
    fn completed(cx: &Cx) -> Result<(), EngineError> {
        registry(cx.engine).affectors.borrow_mut().push(cx.this);
        Ok(())
    }
}

// ------------------------------------------------------------ state

/// A system's particles and what it needs to carry on.
#[derive(Debug)]
struct SystemRun {
    rng: Rng,
    particles: Vec<Particle>,
    /// The timer that fires a step; made when first needed.
    timer: Option<ObjectId>,
    /// Whether the `startTime` run-ahead was done.
    warmed: bool,
    /// Where the particles of each group are, found once after they
    /// change: a painter walks only the groups it draws, where each
    /// walking all of them costs the painters times the particles.
    groups: Option<Vec<(Rc<str>, Vec<usize>)>>,
}

impl Default for SystemRun {
    fn default() -> Self {
        Self {
            rng: Rng::new(SEED),
            particles: Vec::new(),
            timer: None,
            warmed: false,
            groups: None,
        }
    }
}

impl SystemRun {
    fn grouped(&mut self) {
        if self.groups.is_some() {
            return;
        }
        let mut by_group: Vec<(Rc<str>, Vec<usize>)> = Vec::new();
        for (i, p) in self.particles.iter().enumerate() {
            match by_group.iter_mut().find(|(g, _)| *g == p.group) {
                Some((_, at)) => at.push(i),
                None => by_group.push((p.group.clone(), vec![i])),
            }
        }
        self.groups = Some(by_group);
    }
}

/// Everything that took part, and what each painter shows.
#[derive(Default)]
struct Registry {
    emitters: RefCell<Vec<ObjectId>>,
    /// The emitters that may give birth: enabled with a rate, or owing a
    /// burst or pulse. A step looks at these alone, where a hundred blocks
    /// that each burst once when they die would cost a hundred looks.
    lively: RefCell<Vec<ObjectId>>,
    /// How many emitters steps have looked at.
    looked: Cell<u64>,
    painters: RefCell<Vec<ObjectId>>,
    /// How many particles painters have looked at.
    painted: Cell<u64>,
    affectors: RefCell<Vec<ObjectId>>,
    shown: RefCell<FxHashMap<ObjectId, Shown>>,
    /// Each `MaskShape` picture's alpha, by URL, with the picture it was
    /// read from: a new picture under the URL is read again.
    masks: RefCell<HashMap<String, (ImageHandle, Rc<Mask>)>>,
}

fn registry(e: &Engine) -> Rc<Registry> {
    e.extension::<Registry>()
}

fn system_run(e: &Engine, sys: ObjectId) -> Option<Rc<RefCell<SystemRun>>> {
    e.with(sys, |s: &ParticleSystem| Rc::clone(&s.run)).ok()
}

fn emitter_run(e: &Engine, em: ObjectId) -> Option<Rc<RefCell<EmitterRun>>> {
    e.with(em, |s: &Emitter| Rc::clone(&s.run)).ok()
}

fn warn(e: &Engine, what: &str, r: Result<(), EngineError>) {
    if let Err(err) = r {
        e.log(LogLevel::Warning, CATEGORY, format!("{what}: {err}"));
    }
}

fn number(e: &Engine, obj: ObjectId, p: PropIndex) -> f64 {
    e.untrack(|| e.read(obj, p)).to_number()
}

/// The system an object belongs to: the one it names, or else the
/// nearest `ParticleSystem` above it.
fn system_of(e: &Engine, obj: ObjectId, system_prop: PropIndex) -> Option<ObjectId> {
    let named = e.read(obj, system_prop).as_object();
    if let Some(s) = named.filter(|s| e.is_alive(*s)) {
        return Some(s);
    }
    let mut cur = e.visual_parent(obj);
    for _ in 0..4096 {
        let p = cur?;
        if is_a::<ParticleSystem>(e, p) {
            return Some(p);
        }
        cur = e.visual_parent(p);
    }
    None
}

/// Where an item's origin is, in the scene's coordinates.
fn origin(e: &Engine, item: ObjectId) -> (f64, f64) {
    let (mut x, mut y) = (0.0, 0.0);
    let mut cur = Some(item);
    for _ in 0..4096 {
        let Some(i) = cur else { break };
        x += number(e, i, Item::X.index);
        y += number(e, i, Item::Y.index);
        cur = e.visual_parent(i);
    }
    (x, y)
}

fn groups_of(e: &Engine, obj: ObjectId, prop: PropIndex) -> Vec<String> {
    match e.untrack(|| e.read(obj, prop)) {
        Value::List(l) => l
            .iter()
            .map(|v| v.to_js_string().as_str().to_owned())
            .collect(),
        Value::String(s) if !s.as_str().is_empty() => vec![s.as_str().to_owned()],
        _ => Vec::new(),
    }
}

fn members(
    e: &Engine,
    list: &RefCell<Vec<ObjectId>>,
    sys: ObjectId,
    prop: PropIndex,
) -> Vec<ObjectId> {
    list.borrow_mut().retain(|o| e.is_alive(*o));
    let all = list.borrow().clone();
    all.into_iter()
        .filter(|o| e.untrack(|| system_of(e, *o, prop)) == Some(sys))
        .collect()
}

fn emitters_of(e: &Engine, sys: ObjectId) -> Vec<ObjectId> {
    members(e, &registry(e).emitters, sys, Emitter::SYSTEM.index)
}

/// The emitters of `sys` that may give birth.
fn lively_of(e: &Engine, sys: ObjectId) -> Vec<ObjectId> {
    let reg = registry(e);
    let lively = members(e, &reg.lively, sys, Emitter::SYSTEM.index);
    reg.looked.set(reg.looked.get() + lively.len() as u64);
    lively
}

/// `em` may give birth from now on.
fn enliven(e: &Engine, em: ObjectId) {
    let reg = registry(e);
    let mut lively = reg.lively.borrow_mut();
    if !lively.contains(&em) {
        lively.push(em);
    }
}

/// How many emitters the particle steps of this engine have looked at.
#[doc(hidden)]
pub fn emitters_looked_at(engine: &Engine) -> u64 {
    registry(engine).looked.get()
}

fn painters_of(e: &Engine, sys: ObjectId) -> Vec<ObjectId> {
    members(e, &registry(e).painters, sys, ParticlePainter::SYSTEM.index)
}

fn affectors_of(e: &Engine, sys: ObjectId) -> Vec<ObjectId> {
    members(
        e,
        &registry(e).affectors,
        sys,
        ParticleAffector::SYSTEM.index,
    )
}

// ------------------------------------------------------------ the timer

/// Whether the system has anything to step.
fn busy(e: &Engine, sys: ObjectId) -> bool {
    let Some(run) = system_run(e, sys) else {
        return false;
    };
    if !run.borrow().particles.is_empty() {
        return true;
    }
    lively_of(e, sys).into_iter().any(|em| {
        let enabled = e.read(em, Item::ENABLED.index).to_boolean();
        let owes = emitter_run(e, em).is_some_and(|r| {
            let r = r.borrow();
            !r.bursts.is_empty() || r.pulse_left > 0.0
        });
        owes || (enabled && number(e, em, Emitter::EMIT_RATE.index) > 0.0)
    })
}

/// Starts or stops the step timer to match what the system has to do.
fn sync_timer(e: &Engine, sys: ObjectId) {
    let running = e.read(sys, ParticleSystem::RUNNING.index).to_boolean();
    let paused = e.read(sys, ParticleSystem::PAUSED.index).to_boolean();
    let wanted = running && !paused && busy(e, sys);
    let Some(run) = system_run(e, sys) else {
        return;
    };
    let existing = run.borrow().timer.filter(|t| e.is_alive(*t));
    let timer = match (existing, wanted) {
        (Some(t), _) => t,
        (None, false) => return,
        (None, true) => {
            let made = e
                .create_native_incomplete(Timer::default(), Some(sys))
                .and_then(|t| {
                    e.complete(t)?;
                    e.set(t, Timer::INTERVAL, (STEP * 1000.0).round() as i32)?;
                    e.set(t, Timer::REPEAT, true)?;
                    e.connect_by_name(t, "triggered", move |e, _| step_system(e, sys))?;
                    Ok(t)
                });
            match made {
                Ok(t) => {
                    run.borrow_mut().timer = Some(t);
                    t
                }
                Err(err) => {
                    warn(e, "timer", Err(err));
                    return;
                }
            }
        }
    };
    let on = e
        .untrack(|| e.read(timer, Timer::RUNNING.index))
        .to_boolean();
    if on != wanted {
        warn(e, "timer", e.set(timer, Timer::RUNNING, wanted));
    }
}

/// An emitter was asked to burst or pulse: make sure its system steps.
fn wake(e: &Engine, em: ObjectId) {
    if let Some(sys) = system_of(e, em, Emitter::SYSTEM.index) {
        sync_timer(e, sys);
    }
}

fn reset(e: &Engine, sys: ObjectId) {
    if let Some(run) = system_run(e, sys) {
        let mut r = run.borrow_mut();
        r.particles.clear();
        r.groups = None;
        r.rng = Rng::new(SEED);
        r.warmed = false;
    }
    finish(e, sys);
}

// ------------------------------------------------------------ stepping

/// What an emitter says for one step, read before the particles are touched.
struct EmitterSpec {
    id: ObjectId,
    group: Rc<str>,
    life: f64,
    life_var: f64,
    size: f64,
    end_size: f64,
    size_var: f64,
    max: i32,
    w: f64,
    h: f64,
    /// The emitter's origin in the system's coordinates.
    at: (f64, f64),
    shape: Shape,
    velocity: Dir,
    acceleration: Dir,
}

fn read_dir(e: &Engine, d: Option<ObjectId>) -> Dir {
    let Some(d) = d.filter(|d| e.is_alive(*d)) else {
        return Dir::Null;
    };
    if is_a::<PointDirection>(e, d) {
        Dir::Point {
            x: number(e, d, PointDirection::X.index),
            y: number(e, d, PointDirection::Y.index),
            x_var: number(e, d, PointDirection::X_VARIATION.index),
            y_var: number(e, d, PointDirection::Y_VARIATION.index),
        }
    } else if is_a::<AngleDirection>(e, d) {
        Dir::Angle {
            angle: number(e, d, AngleDirection::ANGLE.index),
            magnitude: number(e, d, AngleDirection::MAGNITUDE.index),
            angle_var: number(e, d, AngleDirection::ANGLE_VARIATION.index),
            magnitude_var: number(e, d, AngleDirection::MAGNITUDE_VARIATION.index),
        }
    } else if is_a::<TargetDirection>(e, d) {
        Dir::Target {
            x: number(e, d, TargetDirection::TARGET_X.index),
            y: number(e, d, TargetDirection::TARGET_Y.index),
            target_var: number(e, d, TargetDirection::TARGET_VARIATION.index),
            magnitude: number(e, d, TargetDirection::MAGNITUDE.index),
            magnitude_var: number(e, d, TargetDirection::MAGNITUDE_VARIATION.index),
            proportional: e
                .untrack(|| e.read(d, TargetDirection::PROPORTIONAL_MAGNITUDE.index))
                .to_boolean(),
        }
    } else {
        Dir::Null
    }
}

fn read_shape(e: &Engine, s: Option<ObjectId>) -> Shape {
    let Some(s) = s.filter(|s| e.is_alive(*s)) else {
        return Shape::Rect;
    };
    if is_a::<EllipseShape>(e, s) {
        Shape::Ellipse {
            fill: e
                .untrack(|| e.read(s, EllipseShape::FILL.index))
                .to_boolean(),
        }
    } else if is_a::<MaskShape>(e, s) {
        let raw = e
            .untrack(|| e.read(s, MaskShape::SOURCE.index))
            .to_js_string();
        if raw.as_str().is_empty() {
            return Shape::Rect;
        }
        let key = ImageKey {
            url: e.resolve_url(s, raw.as_str()),
            request: Default::default(),
        };
        match ImageStore::of(e).load_now(&key, true) {
            Ok(img) => Shape::Mask(mask_of(e, key.url, img)),
            Err(_) => Shape::Rect,
        }
    } else {
        Shape::Rect
    }
}

fn mask_of(e: &Engine, url: String, img: ImageHandle) -> Rc<Mask> {
    let reg = registry(e);
    if let Some((seen, mask)) = reg.masks.borrow().get(&url)
        && *seen == img
    {
        return Rc::clone(mask);
    }
    let mask = Rc::new(Mask {
        width: img.width(),
        height: img.height(),
        alpha: img
            .data()
            .as_chunks::<4>()
            .0
            .iter()
            .map(|px| px[3])
            .collect(),
    });
    reg.masks.borrow_mut().insert(url, (img, Rc::clone(&mask)));
    mask
}

fn read_emitter(e: &Engine, sys: ObjectId, em: ObjectId) -> EmitterSpec {
    let (so, eo) = (origin(e, sys), origin(e, em));
    let num = |p: PropIndex| number(e, em, p);
    let size = num(Emitter::SIZE.index);
    let end = num(Emitter::END_SIZE.index);
    let (shape, velocity, acceleration) = (
        e.untrack(|| e.read(em, Emitter::SHAPE.index)).as_object(),
        e.untrack(|| e.read(em, Emitter::VELOCITY.index))
            .as_object(),
        e.untrack(|| e.read(em, Emitter::ACCELERATION.index))
            .as_object(),
    );
    EmitterSpec {
        id: em,
        group: Rc::from(
            e.untrack(|| e.read(em, Emitter::GROUP.index))
                .to_js_string()
                .as_str(),
        ),
        life: num(Emitter::LIFE_SPAN.index) / 1000.0,
        life_var: num(Emitter::LIFE_SPAN_VARIATION.index) / 1000.0,
        size,
        end_size: if end < 0.0 { size } else { end },
        size_var: num(Emitter::SIZE_VARIATION.index),
        max: num(Emitter::MAXIMUM_EMITTED.index) as i32,
        w: num(Item::WIDTH.index),
        h: num(Item::HEIGHT.index),
        at: (eo.0 - so.0, eo.1 - so.1),
        shape: read_shape(e, shape),
        velocity: read_dir(e, velocity),
        acceleration: read_dir(e, acceleration),
    }
}

/// One affector's reach: whole system when it has no size.
struct Reach {
    wander: Option<Wandering>,
    /// Seconds an `Age` leaves a particle at most.
    life_left: Option<f64>,
    groups: Vec<String>,
    /// `x, y, w, h` in system coordinates; none for the whole system.
    area: Option<(f64, f64, f64, f64)>,
}

fn read_reaches(e: &Engine, sys: ObjectId) -> Vec<Reach> {
    let so = origin(e, sys);
    affectors_of(e, sys)
        .into_iter()
        .filter(|a| is_a::<Wander>(e, *a) || is_a::<Age>(e, *a))
        .filter(|a| e.untrack(|| e.read(*a, Item::ENABLED.index)).to_boolean())
        .map(|a| {
            let (w, h) = (
                number(e, a, Item::WIDTH.index),
                number(e, a, Item::HEIGHT.index),
            );
            let ao = origin(e, a);
            let aging = is_a::<Age>(e, a);
            Reach {
                wander: (!aging).then(|| Wandering {
                    pace: number(e, a, Wander::PACE.index),
                    x_variance: number(e, a, Wander::X_VARIANCE.index),
                    y_variance: number(e, a, Wander::Y_VARIANCE.index),
                    affected: Affected::from_i32(
                        number(e, a, Wander::AFFECTED_PARAMETER.index) as i32
                    ),
                }),
                life_left: aging.then(|| (number(e, a, Age::LIFE_LEFT.index) / 1000.0).max(0.0)),
                groups: groups_of(e, a, ParticleAffector::GROUPS.index),
                area: (w > 0.0 || h > 0.0).then_some((ao.0 - so.0, ao.1 - so.1, w, h)),
            }
        })
        .collect()
}

/// What an emitter owes this step: asked-for bursts, and the whole
/// particles its rate has earned.
type Burst = (u32, Option<(f64, f64)>);

/// Advances the live particles one step, then lets the emitters add theirs.
fn step_particles(run: &mut SystemRun, reaches: &[Reach], emitters: &[(EmitterSpec, Vec<Burst>)]) {
    run.groups = None;
    let SystemRun { rng, particles, .. } = run;
    particles.retain_mut(|p| {
        let reached: Vec<&Reach> = reaches
            .iter()
            .filter(|r| r.groups.is_empty() || r.groups.iter().any(|g| **g == *p.group))
            .filter(|r| {
                r.area
                    .is_none_or(|(x, y, w, h)| p.x >= x && p.x <= x + w && p.y >= y && p.y <= y + h)
            })
            .collect();
        for left in reached.iter().filter_map(|r| r.life_left) {
            p.life = p.life.min(p.age + left);
        }
        let here: Vec<Wandering> = reached.iter().filter_map(|r| r.wander).collect();
        sim::advance(p, &here, rng)
    });
    for (spec, owed) in emitters {
        let live = particles.iter().filter(|p| p.emitter == spec.id).count() as i64;
        let mut room = if spec.max < 0 {
            i64::MAX
        } else {
            i64::from(spec.max) - live
        };
        for &(n, at) in owed {
            for _ in 0..n {
                if room <= 0 {
                    break;
                }
                room -= 1;
                particles.push(birth(spec, at, rng));
            }
        }
    }
}

fn birth(spec: &EmitterSpec, at: Option<(f64, f64)>, rng: &mut Rng) -> Particle {
    let local = at.unwrap_or_else(|| spec.shape.extrude(rng, spec.w, spec.h));
    let (vx, vy) = spec.velocity.vector(rng, local);
    let (ax, ay) = spec.acceleration.vector(rng, local);
    let jitter = rng.signed(spec.size_var);
    let life = (spec.life + rng.signed(spec.life_var)).max(0.0);
    Particle {
        x: spec.at.0 + local.0,
        y: spec.at.1 + local.1,
        vx,
        vy,
        ax,
        ay,
        age: 0.0,
        life,
        size: (spec.size + jitter).max(0.0),
        end_size: (spec.end_size + jitter).max(0.0),
        group: Rc::clone(&spec.group),
        emitter: spec.id,
        dice: [
            rng.signed(1.0),
            rng.signed(1.0),
            rng.signed(1.0),
            rng.signed(1.0),
        ],
        wander: [0.0; 2],
    }
}

/// One step of the whole system.
fn advance_system(e: &Engine, sys: ObjectId) {
    let Some(run) = system_run(e, sys) else {
        return;
    };
    let reaches = read_reaches(e, sys);
    // Only an emitter that gives birth has the rest of what it says read.
    let mut spent = Vec::new();
    let emitters: Vec<(EmitterSpec, Vec<Burst>)> = lively_of(e, sys)
        .into_iter()
        .filter_map(|em| {
            let run = emitter_run(e, em)?;
            let enabled = e.untrack(|| e.read(em, Item::ENABLED.index)).to_boolean();
            let rate = number(e, em, Emitter::EMIT_RATE.index);
            let owed = {
                let mut r = run.borrow_mut();
                let emits = r.pulse_left > 0.0 || enabled;
                let mut owed = std::mem::take(&mut r.bursts);
                r.pulse_left = (r.pulse_left - STEP).max(0.0);
                if emits && rate > 0.0 {
                    // The rate's carry lives with the emitter, across steps.
                    r.carry += rate * STEP;
                    let whole = r.carry.floor();
                    r.carry -= whole;
                    if whole >= 1.0 {
                        owed.push((whole as u32, None));
                    }
                }
                if !(enabled && rate > 0.0) && r.pulse_left <= 0.0 && r.bursts.is_empty() {
                    spent.push(em);
                }
                owed
            };
            (!owed.is_empty()).then(|| (read_emitter(e, sys, em), owed))
        })
        .collect();
    if !spent.is_empty() {
        registry(e)
            .lively
            .borrow_mut()
            .retain(|em| !spent.contains(em));
    }
    step_particles(&mut run.borrow_mut(), &reaches, &emitters);
}

/// The timer fired: simulate, draw, and say whether the system is empty.
fn step_system(e: &Engine, sys: ObjectId) {
    if !e.is_alive(sys) {
        return;
    }
    let Some(run) = system_run(e, sys) else {
        return;
    };
    let ahead = !run.borrow().warmed;
    if ahead {
        run.borrow_mut().warmed = true;
        let ms = emitters_of(e, sys)
            .into_iter()
            .map(|em| number(e, em, Emitter::START_TIME.index))
            .fold(0.0, f64::max);
        let steps = ((ms / 1000.0 / STEP) as usize).min(WARM_STEPS_MAX);
        for _ in 0..steps {
            advance_system(e, sys);
        }
    }
    advance_system(e, sys);
    finish(e, sys);
}

/// Asks for a repaint, reports `empty`, and stops the timer when there is
/// nothing left.
fn finish(e: &Engine, sys: ObjectId) {
    mark(e, sys);
    let empty = system_run(e, sys).is_none_or(|r| r.borrow().particles.is_empty());
    write_internal_bool(e, sys, ParticleSystem::EMPTY.index, empty);
    sync_timer(e, sys);
}

fn write_internal_bool(e: &Engine, obj: ObjectId, p: PropIndex, v: bool) {
    let _ = write_computed(e, obj, p, Value::Bool(v));
}

// ------------------------------------------------------------ painting

/// The particles changed: every painter that shows some, or did, is drawn
/// again. Its pictures are placed when the scene draws, so a wake that
/// catches up on several steps places them once, and a window that shows
/// no frame places none.
fn mark(e: &Engine, sys: ObjectId) {
    let Some(run) = system_run(e, sys) else {
        return;
    };
    if run.borrow().groups.is_none() {
        run.borrow_mut().grouped();
    }
    let groups: Vec<Rc<str>> = run
        .borrow()
        .groups
        .iter()
        .flatten()
        .map(|(g, _)| g.clone())
        .collect();
    let groups: Vec<&str> = groups.iter().map(|g| &**g).collect();
    let reg = registry(e);
    for pa in painters_of(e, sys).into_iter().filter(|p| e.is_alive(*p)) {
        let mine = groups_of(e, pa, ParticlePainter::GROUPS.index);
        let has = if mine.is_empty() {
            groups.contains(&"")
        } else {
            mine.iter().any(|g| groups.contains(&g.as_str()))
        };
        let serial = {
            let mut shown = reg.shown.borrow_mut();
            let Some(s) = shown.get_mut(&pa) else {
                continue;
            };
            // Nothing shown before or now: no repaint.
            let again = has || s.has;
            s.has = has;
            s.serial.filter(|_| again)
        };
        if let Some(n) = serial {
            bump(e, n);
        }
    }
}

/// The look of `pa` changed: it is drawn again.
fn restale(e: &Engine, pa: ObjectId) {
    let serial = registry(e).shown.borrow().get(&pa).and_then(|s| s.serial);
    if let Some(n) = serial {
        bump(e, n);
    }
}

fn bump(e: &Engine, n: NodeId) {
    let next = e.graph().get_untracked(n).to_number() as i32 + 1;
    e.graph().set(n, Value::Int(next));
}

/// How many particles painters have looked at.
#[doc(hidden)]
pub fn particles_painted(engine: &Engine) -> u64 {
    registry(engine).painted.get()
}

/// What a painter shows, and the serial its paint follows.
#[derive(Default)]
struct Shown {
    serial: Option<NodeId>,
    /// The `source` as written, and the picture of where it resolves to;
    /// none when it could not be read.
    source: Option<String>,
    image: Option<ImageHandle>,
    /// Whether the painter had particles to draw when last looked at.
    has: bool,
}

/// Every particle of a painter, drawn into the painter's own node: one
/// display list, where an item for each would cost its own geometry,
/// paint and bindings on every step.
///
/// Every particle draws the one picture and carries its colour as a tint,
/// as Qt colours a particle: a picture for each colour cost its pixels and
/// a texture, and drew each particle as a batch of its own.
///
/// The particles are placed when the scene draws, not when it reads this:
/// a timer that catches up fires several steps before one frame.
struct Pictures {
    image: ImageHandle,
    run: Rc<RefCell<SystemRun>>,
    reg: Rc<Registry>,
    /// The groups drawn; none draws the default group.
    groups: Vec<String>,
    /// The painter's place in the system's coordinates.
    off: (f64, f64),
    color: [f64; 4],
    color_variation: f64,
    alpha: f64,
    alpha_variation: f64,
    entry_effect: i32,
}

impl std::fmt::Debug for Pictures {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pictures")
            .field("groups", &self.groups)
            .field("off", &self.off)
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
struct NoPictures;

impl ItemPaint for NoPictures {
    fn paint(&self, _size: kurbo::Size, _text: &mut TextEngine, _out: &mut DisplayListBuilder) {}
}

impl ItemPaint for Pictures {
    fn paint(&self, _size: kurbo::Size, _text: &mut TextEngine, out: &mut DisplayListBuilder) {
        if self.run.borrow().groups.is_none() {
            self.run.borrow_mut().grouped();
        }
        let run = self.run.borrow();
        let src = self.image.rect();
        let [cr, cg, cb, ca] = self.color;
        let mut looked = 0;
        let mine = run.groups.iter().flatten().filter(|(g, _)| {
            if self.groups.is_empty() {
                g.is_empty()
            } else {
                self.groups.iter().any(|x| **x == **g)
            }
        });
        for i in mine.flat_map(|(_, at)| at.iter().copied()) {
            looked += 1;
            let p = &run.particles[i];
            let fade = sim::entry_factor(p.age, p.life);
            let size = p.current_size() * if self.entry_effect == 2 { fade } else { 1.0 };
            let a = ((ca * self.alpha + p.dice[3] * self.alpha_variation).clamp(0.0, 1.0))
                * if self.entry_effect == 1 { fade } else { 1.0 };
            if a <= 0.0 || size <= 0.0 {
                continue;
            }
            let shift = p.dice[0] * self.color_variation;
            let (r, g, b) = (
                (cr + shift).clamp(0.0, 1.0),
                (cg + shift).clamp(0.0, 1.0),
                (cb + shift).clamp(0.0, 1.0),
            );
            let white = r >= 0.999 && g >= 0.999 && b >= 0.999;
            let (x, y) = (p.x - self.off.0 - size / 2.0, p.y - self.off.1 - size / 2.0);
            out.push(DrawOp::Image {
                image: self.image.clone(),
                src,
                dst: kurbo::Rect::new(x, y, x + size, y + size),
                tile: None,
                mirror: [false; 2],
                smooth: true,
                opacity: a as f32,
                tint: (!white).then(|| Rgba::from_straight(r as f32, g as f32, b as f32, 1.0)),
            });
        }
        self.reg.painted.set(self.reg.painted.get() + looked);
    }
}

/// What `pa` shows. Only its serial is followed: the look effect and the
/// steps bump it, and what is read here is read untracked.
fn read_pictures(e: &Engine, pa: ObjectId) -> Rc<dyn ItemPaint> {
    let reg = registry(e);
    let serial = reg.shown.borrow().get(&pa).map(|s| s.serial);
    let Some(serial) = serial else {
        return Rc::new(NoPictures);
    };
    if let Some(n) = serial {
        let _ = e.graph().get(n);
    }
    match e.untrack(|| look_of(e, pa)) {
        Some(p) => Rc::new(p),
        None => Rc::new(NoPictures),
    }
}

fn look_of(e: &Engine, pa: ObjectId) -> Option<Pictures> {
    if !e.is_alive(pa) {
        return None;
    }
    let sys = system_of(e, pa, ParticlePainter::SYSTEM.index)?;
    let run = system_run(e, sys)?;
    let reg = registry(e);
    let raw = e.read(pa, ImageParticle::SOURCE.index).to_js_string();
    let image = {
        let mut shown = reg.shown.borrow_mut();
        let s = shown.get_mut(&pa)?;
        if s.source.as_deref() != Some(raw.as_str()) {
            let url = if raw.is_empty() {
                Atom::from("")
            } else {
                Atom::from(e.resolve_url(pa, raw.as_str()).as_str())
            };
            s.source = Some(raw.as_str().to_owned());
            s.image = load_picture(e, &url);
        }
        s.image.clone()?
    };
    let (so, po) = (origin(e, sys), origin(e, pa));
    let color = match e.read(pa, ImageParticle::COLOR.index) {
        Value::Color(c) => c,
        _ => Color::WHITE,
    };
    Some(Pictures {
        image,
        run,
        groups: groups_of(e, pa, ParticlePainter::GROUPS.index),
        off: (po.0 - so.0, po.1 - so.1),
        color: color.to_rgba_f(),
        color_variation: number(e, pa, ImageParticle::COLOR_VARIATION.index),
        alpha: number(e, pa, ImageParticle::ALPHA.index),
        alpha_variation: number(e, pa, ImageParticle::ALPHA_VARIATION.index),
        entry_effect: number(e, pa, ImageParticle::ENTRY_EFFECT.index) as i32,
        reg,
    })
}

/// The picture of `url`, read on this thread as `Image` reads a local
/// file. None, with a warning, when it cannot be read.
fn load_picture(e: &Engine, url: &Atom) -> Option<ImageHandle> {
    if url.as_str().is_empty() {
        return None;
    }
    let key = ImageKey {
        url: url.as_str().to_owned(),
        request: Default::default(),
    };
    match ImageStore::of(e).load_now(&key, true) {
        Ok(image) => Some(image),
        Err(err) => {
            e.log(
                LogLevel::Warning,
                CATEGORY,
                format!("cannot read the particle picture {url}: {err}"),
            );
            None
        }
    }
}

// ------------------------------------------------------------ registration

pub(crate) fn register(engine: &Engine) {
    engine.register_module(ljos_engine::module!("QtQuick.Particles" 2.0 => [
        ParticleSystem, ParticleExtruder, EllipseShape, MaskShape,
        Direction, PointDirection, AngleDirection, TargetDirection,
        Emitter, ParticlePainter, ImageParticle, ParticleAffector, Wander, Age,
    ]));
    register_paint_item(
        engine,
        MetaRef::Static(ImageParticle::static_meta()),
        read_pictures,
    );
}
