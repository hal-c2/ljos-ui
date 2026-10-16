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
