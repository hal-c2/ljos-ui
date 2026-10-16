//! A timer a control owns: the wait before a press is a press and hold,
//! the pace of a spin box's held button.
//!
//! It is an engine `Timer`, made the first time it is needed, so it runs
//! on the engine's clock and nothing polls: a control that is not held
//! has no deadline.

use std::cell::Cell;

use ljos_engine::{Engine, ObjectId, Timer};

use crate::util::check;

/// Arms the timer in `slot` to fire once, `ms` from now, and to call
/// `fired` with `owner` then. A timer that is already running starts
/// over.
pub(crate) fn start(
    engine: &Engine,
    owner: ObjectId,
    slot: &Cell<Option<ObjectId>>,
    ms: i32,
    fired: fn(&Engine, ObjectId),
) {
    let timer = match slot.get().filter(|t| engine.is_alive(*t)) {
        Some(t) => t,
        None => {
            let made = engine
                .create_native_incomplete(Timer::default(), Some(owner))
                .and_then(|t| {
                    engine.complete(t)?;
                    engine.connect_by_name(t, "triggered", move |e, _| {
                        if e.is_alive(owner) {
                            fired(e, owner);
                        }
                    })?;
                    Ok(t)
                });
            match made {
                Ok(t) => {
                    slot.set(Some(t));
                    t
                }
                Err(err) => {
                    check(engine, "timer", Err(err));
                    return;
                }
            }
        }
    };
    let r = engine
        .set(timer, Timer::INTERVAL, ms.max(1))
        .and_then(|()| engine.invoke_by_name(timer, "restart", &[]).map(|_| ()));
    check(engine, "timer", r);
}

pub(crate) fn stop(engine: &Engine, slot: &Cell<Option<ObjectId>>) {
    if let Some(t) = slot.get().filter(|t| engine.is_alive(*t)) {
        let _ = engine.set(t, Timer::RUNNING, false);
    }
}
