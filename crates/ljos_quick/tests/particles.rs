//! What a particle system's step costs: it reads only the emitters that
//! give birth on that step, and its painter draws every particle in one
//! display list.

mod common;

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use common::*;
use ljos_engine::{Engine, Value};
use ljos_quick::ImageStore;
use ljos_render::DrawOp;
use ljos_runtime::{Time, VirtualClock};

/// A PNG of one colour.
fn png(width: u32, height: u32, rgba: [u8; 4]) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, width, height);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().unwrap();
        let data: Vec<u8> = (0..width * height).flat_map(|_| rgba).collect();
        w.write_image_data(&data).unwrap();
    }
    out
}

#[test]
fn a_step_reads_only_the_emitters_that_give_birth() {
    let e = engine();
    let idle_emitters = (0..40)
        .map(|_| r#"Emitter { objectName: "idle"; enabled: false; width: 10; height: 10 }"#)
        .collect::<Vec<_>>()
        .join("\n");
    let root = load(
        &e,
        &format!(
            r#"
    import QtQuick
    import QtQuick.Particles
    ParticleSystem {{
      objectName: "system"
      width: 300; height: 300
      property real tick: 0
      Emitter {{ objectName: "live"; emitRate: 100; lifeSpan: 5000; width: 10; height: 10 }}
      {idle_emitters}
      ImageParticle {{ source: "dot.png" }}
    }}"#
        ),
    );
    let sys = find(&e, root, "system");
    let live = find(&e, root, "live");
    let idle: Vec<_> = e
        .children(sys)
        .into_iter()
        .filter(|o| e.read_by_name(*o, "objectName").to_js_string().as_str() == "idle")
        .collect();
    assert_eq!(idle.len(), 40);
    // Each emitter's `size` follows `tick`, so a step that reads an
    // emitter's spec pulls its binding once.
    let pulls = Rc::new(Cell::new(0u32));
    let size = e.meta_of(live).unwrap().property_index("size").unwrap();
    let tick = e.meta_of(sys).unwrap().property_index("tick").unwrap();
    for em in idle.iter().chain([&live]) {
        let p = pulls.clone();
        e.bind(*em, size, move |e: &Engine| {
            p.set(p.get() + 1);
            Value::Real(8.0 + e.read(sys, tick).to_number() * 0.0)
        })
        .unwrap();
    }
    pulls.set(0);
    let clock = Arc::new(VirtualClock::new());
    e.set_clock(Rc::new(clock.clone()));
    let mut steps = 0;
    for t in (16..=1000u32).step_by(16) {
        set(&e, sys, "tick", Value::Real(f64::from(t)));
        clock.set(Time::from_millis(u64::from(t)));
        e.advance_timers(Time::from_millis(u64::from(t)));
        steps += 1;
    }
    assert!(!get(&e, sys, "empty").to_boolean());
    // The live emitter at most once a step; the forty idle ones never.
    assert!(
        pulls.get() <= steps + 1,
        "{} pulls over {steps} steps",
        pulls.get()
    );
}

#[test]
fn a_step_costs_the_same_however_many_particles_it_draws() {
    let e = engine();
    ImageStore::of(&e).register_resource("file:///dot.png", png(8, 8, [255, 0, 0, 255]));
    let root = load(
        &e,
        r#"
    import QtQuick
    import QtQuick.Particles
    ParticleSystem {
      objectName: "system"
      width: 300; height: 300
      Emitter { emitRate: 400; lifeSpan: 5000; size: 8; endSize: 16; width: 100; height: 100 }
      ImageParticle { objectName: "painter"; source: "file:///dot.png"; alphaVariation: 0.5 }
    }"#,
    );
    let painter = find(&e, root, "painter");
    let clock = Arc::new(VirtualClock::new());
    e.set_clock(Rc::new(clock.clone()));
    let mut s = scene(&e, root, 300.0, 300.0);
    frame(&mut s, &e);
    let mut step = |t: u32| {
        clock.set(Time::from_millis(u64::from(t)));
        e.advance_timers(Time::from_millis(u64::from(t)));
        frame(&mut s, &e);
    };
    let mut t = 0;
    while t < 1000 {
        t += 16;
        step(t);
    }
    let before = e.graph().stats();
    let steps = 30;
    for _ in 0..steps {
        t += 16;
        step(t);
    }
    let after = e.graph().stats();
    let drawn = s
        .node_of(painter)
        .and_then(|n| s.render_tree().node(n))
        .and_then(|n| n.content())
        .map(|l| {
            l.ops()
                .iter()
                .filter(|op| matches!(op, DrawOp::Image { .. }))
                .count()
        })
        .unwrap_or_default();
    assert!(drawn > 300, "{drawn} particles drawn");
    assert_eq!(e.children(painter).len(), 0, "no item for each particle");
    // An item for each particle ran its own effects on every step.
    let per_step = (after.effect_runs - before.effect_runs) / steps;
    assert!(
        per_step < 20,
        "{per_step} effect runs a step for {drawn} particles"
    );
}

/// How many emitters a second of steps looks at, with `idle` emitters
/// that never emit and one that bursts once.
fn emitters_looked_at_with(idle: usize) -> u64 {
    let e = engine();
    let clock = Arc::new(VirtualClock::new());
    e.set_clock(Rc::new(clock.clone()));
    let idle_emitters = (0..idle)
        .map(|_| r#"Emitter { enabled: false; width: 10; height: 10 }"#)
        .collect::<Vec<_>>()
        .join("\n");
    let root = load(
        &e,
        &format!(
            r#"
    import QtQuick
    import QtQuick.Particles
    ParticleSystem {{
      objectName: "system"
      width: 300; height: 300
      Emitter {{ emitRate: 100; lifeSpan: 5000; width: 10; height: 10 }}
      Emitter {{ objectName: "burst"; enabled: false; lifeSpan: 5000; width: 10; height: 10 }}
      {idle_emitters}
    }}"#
        ),
    );
    let burst = find(&e, root, "burst");
    e.invoke_by_name(burst, "burst", &[Value::Int(10)]).unwrap();
    let before = ljos_quick::emitters_looked_at(&e);
    for t in (16..=1000u32).step_by(16) {
        clock.set(Time::from_millis(u64::from(t)));
        e.advance_timers(Time::from_millis(u64::from(t)));
    }
    assert!(!get(&e, find(&e, root, "system"), "empty").to_boolean());
    ljos_quick::emitters_looked_at(&e) - before
}

#[test]
fn a_step_looks_only_at_the_emitters_that_may_give_birth() {
    // A hundred blocks each with an emitter that bursts when the block
    // dies: a step costs what emits, not how many could.
    let few = emitters_looked_at_with(10);
    let many = emitters_looked_at_with(400);
    assert_eq!(few, many, "{few} looks with 10 idle, {many} with 400");
    // The rate emitter on each of the ~62 steps, the burst on its first.
    assert!(few <= 2 * 64 + 2, "{few} looks");
}

/// How many particles a second of steps and frames hands painters, with
/// ten particles in each of four groups, a painter for each group and
/// `idle` painters of groups that have none.
fn particles_painted_with(idle: usize) -> u64 {
    let e = engine();
    ImageStore::of(&e).register_resource("file:///dot.png", png(8, 8, [255, 0, 0, 255]));
    let clock = Arc::new(VirtualClock::new());
    e.set_clock(Rc::new(clock.clone()));
    let idle_painters = (0..idle)
        .map(|i| format!(r#"ImageParticle {{ source: "file:///dot.png"; groups: ["none{i}"] }}"#))
        .collect::<Vec<_>>()
        .join("\n");
    let emitters = ["a", "b", "c", "d"]
        .map(|g| {
            format!(
                r#"Emitter {{ objectName: "{g}"; group: "{g}"; enabled: false; lifeSpan: 5000; width: 10; height: 10 }}
      ImageParticle {{ source: "file:///dot.png"; groups: ["{g}"] }}"#
            )
        })
        .join("\n");
    let root = load(
        &e,
        &format!(
            r#"
    import QtQuick
    import QtQuick.Particles
    ParticleSystem {{
      width: 300; height: 300
      {emitters}
      {idle_painters}
    }}"#
        ),
    );
    for g in ["a", "b", "c", "d"] {
        e.invoke_by_name(find(&e, root, g), "burst", &[Value::Int(10)])
            .unwrap();
    }
    let mut s = scene(&e, root, 300.0, 300.0);
    frame(&mut s, &e);
    let before = ljos_quick::particles_painted(&e);
    for t in (16..=1000u32).step_by(16) {
        clock.set(Time::from_millis(u64::from(t)));
        e.advance_timers(Time::from_millis(u64::from(t)));
        frame(&mut s, &e);
    }
    ljos_quick::particles_painted(&e) - before
}

#[test]
fn a_painter_looks_only_at_the_particles_of_its_groups() {
    // samegame has a painter for each colour of block and of paint: a
    // step costs the particles, not the painters times the particles.
    let few = particles_painted_with(0);
    let many = particles_painted_with(20);
    assert_eq!(
        few, many,
        "{few} looks with no idle painters, {many} with 20"
    );
    // Forty particles on each of the ~62 steps.
    assert!(few <= 40 * 64, "{few} looks");
    assert!(few >= 40 * 60, "{few} looks");
}

#[test]
fn particles_of_many_colours_draw_one_picture() {
    let e = engine();
    ImageStore::of(&e).register_resource("file:///dot.png", png(8, 8, [255, 255, 255, 255]));
    let root = load(
        &e,
        r##"
    import QtQuick
    import QtQuick.Particles
    ParticleSystem {
      width: 300; height: 300
      Emitter { emitRate: 400; lifeSpan: 5000; size: 8; width: 100; height: 100 }
      ImageParticle {
        objectName: "painter"; source: "file:///dot.png"
        color: "#808080"; colorVariation: 0.5
      }
    }"##,
    );
    let painter = find(&e, root, "painter");
    let clock = Arc::new(VirtualClock::new());
    e.set_clock(Rc::new(clock.clone()));
    let mut s = scene(&e, root, 300.0, 300.0);
    for t in (16..=500u32).step_by(16) {
        clock.set(Time::from_millis(u64::from(t)));
        e.advance_timers(Time::from_millis(u64::from(t)));
        frame(&mut s, &e);
    }
    let list = s
        .node_of(painter)
        .and_then(|n| s.render_tree().node(n))
        .and_then(|n| n.content())
        .expect("the painter's display list");
    let mut pictures = std::collections::HashSet::new();
    let mut tints = std::collections::HashSet::new();
    for op in list.ops() {
        if let DrawOp::Image { image, tint, .. } = op {
            pictures.insert(image.id());
            tints.insert(tint.map(|t| t.to_premul_u8()));
        }
    }
    // A picture made for each colour was a texture and a batch each.
    assert!(tints.len() > 50, "{} colours", tints.len());
    assert_eq!(pictures.len(), 1, "one picture for every colour");
}

#[test]
fn a_wake_that_catches_up_on_steps_places_the_pictures_once() {
    let e = engine();
    ImageStore::of(&e).register_resource("file:///dot.png", png(8, 8, [255, 0, 0, 255]));
    let root = load(
        &e,
        r#"
    import QtQuick
    import QtQuick.Particles
    ParticleSystem {
      objectName: "system"
      width: 300; height: 300
      Emitter { objectName: "emitter"; enabled: false; lifeSpan: 5000; width: 10; height: 10 }
      ImageParticle { source: "file:///dot.png" }
    }"#,
    );
    let sys = find(&e, root, "system");
    let clock = Arc::new(VirtualClock::new());
    e.set_clock(Rc::new(clock.clone()));
    let mut s = scene(&e, root, 300.0, 300.0);
    frame(&mut s, &e);
    e.invoke_by_name(find(&e, root, "emitter"), "burst", &[Value::Int(50)])
        .unwrap();
    clock.set(Time::from_millis(16));
    e.advance_timers(Time::from_millis(16));
    frame(&mut s, &e);
    assert!(!get(&e, sys, "empty").to_boolean());
    // A host away for 160 ms: ten steps in one wake, then one frame.
    let before = ljos_quick::particles_painted(&e);
    clock.set(Time::from_millis(176));
    e.advance_timers(Time::from_millis(176));
    assert_eq!(
        ljos_quick::particles_painted(&e),
        before,
        "nothing is placed before a frame asks"
    );
    frame(&mut s, &e);
    assert_eq!(ljos_quick::particles_painted(&e) - before, 50);
}
