//! A running `Timer` an application makes from Rust runs, with no
//! completion step of its own: the object is complete when it is made.

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use ljos::prelude::*;
use ljos_runtime::{Clock as _, VirtualClock};

/// Steps the clock ten intervals of 10 ms and says how often `timer` fired.
fn fired_in_ten_steps(engine: &Engine, clock: &VirtualClock, timer: ObjectId) -> u32 {
    let fired = Rc::new(Cell::new(0));
    let count = Rc::clone(&fired);
    engine
        .connect_by_name(timer, "triggered", move |_, _| count.set(count.get() + 1))
        .unwrap();
    for _ in 0..10 {
        clock.set(clock.now() + Duration::from_millis(10));
        engine.advance_timers(clock.now());
    }
    fired.get()
}

fn engine_with_clock() -> (Engine, Arc<VirtualClock>) {
    let engine = ljos::engine();
    let clock = Arc::new(VirtualClock::new());
    engine.set_clock(Rc::new(Arc::clone(&clock)));
    (engine, clock)
}

#[test]
fn a_running_native_timer_fires() {
    let (engine, clock) = engine_with_clock();
    let timer = Timer {
        interval: 10,
        repeat: true,
        running: true,
        ..Default::default()
    };
    let timer = engine.create_native(timer, None).unwrap();
    assert_eq!(fired_in_ten_steps(&engine, &clock, timer), 10);
}

#[test]
fn a_timer_made_by_name_and_started_later_fires() {
    let (engine, clock) = engine_with_clock();
    let timer = engine.create_by_name("QtQml", "Timer", None).unwrap();
    engine.set(timer, Timer::INTERVAL, 10).unwrap();
    engine.set(timer, Timer::REPEAT, true).unwrap();
    engine.set(timer, Timer::RUNNING, true).unwrap();
    assert_eq!(fired_in_ten_steps(&engine, &clock, timer), 10);
    // Completing again changes nothing.
    engine.complete(timer).unwrap();
    assert_eq!(fired_in_ten_steps(&engine, &clock, timer), 10);
}

#[test]
fn an_incomplete_timer_waits_for_complete() {
    let (engine, clock) = engine_with_clock();
    let timer = Timer {
        interval: 10,
        repeat: true,
        running: true,
        ..Default::default()
    };
    let timer = engine.create_native_incomplete(timer, None).unwrap();
    assert_eq!(fired_in_ten_steps(&engine, &clock, timer), 0);
    engine.complete(timer).unwrap();
    assert_eq!(fired_in_ten_steps(&engine, &clock, timer), 10);
}
