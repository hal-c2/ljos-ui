//! What a particle system's step costs: it reads only the emitters that
//! give birth on that step.

mod common;

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use common::*;
use ljos_engine::{Engine, Value};
use ljos_runtime::{Time, VirtualClock};

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
