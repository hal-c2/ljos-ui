//! `motion-reduced-ignored`, at runtime: with the system's reduced motion
//! setting on, something the application declared keeps moving.
//!
//! The static half reads an endless animation that never reads the
//! setting. This half turns the setting on, lets what was running come to
//! rest and frames the scene for `limits::audit::ANIMATION_WAIT_MS` on its
//! clock: an item whose place, size, scale or rotation still changes moves
//! for someone who asked it not to, whatever the animation's duration or
//! curve is bound to. A `Timer` runs on the engine's clock, which the audit
//! does not move: one that moves something by hand is the static
//! `motion-timer-driven`'s. A fade or a colour may go on, and so may
//! what says the application is busy: a `BusyIndicator` or an
//! indeterminate `ProgressBar`, which reduced motion slows and keeps.

use ljos_controls::{BusyIndicator, ProgressBar};
use ljos_design::limits::audit::ANIMATION_WAIT_MS;
use ljos_engine::{Engine, ObjectId};

use crate::acting::Act;
use crate::cx::{ancestors, declared_by_app, is_a, shown_items, slack};
use crate::motion_watch::{frames_for, with_motion};

/// What is read of each item, in order.
const PROBES: [&str; 6] = ["x", "y", "width", "height", "scale", "rotation"];

/// No audit follows more items than this.
const MOST: usize = 512;

/// Whether `item` is, or is inside, what shows the application is busy.
fn busy(e: &Engine, item: ObjectId) -> bool {
    std::iter::once(item).chain(ancestors(e, item)).any(|i| {
        is_a::<BusyIndicator>(e, i)
            || (is_a::<ProgressBar>(e, i)
                && e.untrack(|| e.read_by_name(i, "indeterminate"))
                    .to_boolean())
    })
}

pub(crate) fn check(act: &mut Act) {
    let e = act.engine;
    let moved: Vec<(ObjectId, &'static str, f64)> = with_motion(act, true, |act| {
        act.finish();
        let items: Vec<ObjectId> = shown_items(&act.cx())
            .into_iter()
            .filter(|i| declared_by_app(e, *i) && !busy(e, *i))
            .take(MOST)
            .collect();
        let probe = |e: &Engine| {
            items
                .iter()
                .flat_map(|i| {
                    PROBES.iter().map(move |n| {
                        if e.is_alive(*i) {
                            e.untrack(|| e.read_by_name(*i, n)).to_number()
                        } else {
                            f64::NAN
                        }
                    })
                })
                .collect::<Vec<_>>()
        };
        let w = frames_for(act, ANIMATION_WAIT_MS, &probe);
        let mut moved = Vec::new();
        for (i, item) in items.iter().enumerate() {
            for (d, name) in PROBES.iter().enumerate() {
                let k = i * PROBES.len() + d;
                let values = w
                    .samples
                    .iter()
                    .filter_map(|(_, v)| v.get(k).copied().filter(|x| x.is_finite()));
                let (lo, hi) = values.fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), x| {
                    (lo.min(x), hi.max(x))
                });
                let least = match *name {
                    "scale" => 0.01,
                    "rotation" => 0.5,
                    _ => slack(),
                };
                if hi - lo > least {
                    moved.push((*item, *name, hi - lo));
                    break;
                }
            }
        }
        moved
    });
    // The topmost item that moves is the one to fix; what it holds moves
    // with it.
    let tops: Vec<_> = moved
        .iter()
        .filter(|(i, _, _)| !ancestors(e, *i).any(|a| moved.iter().any(|m| m.0 == a)))
        .cloned()
        .collect();
    for (item, what, by) in tops {
        let words = crate::cx::words(&act.cx(), item);
        let message = format!(
            "with reduced motion on, {words} still changes its {what} (by {by:.1} over \
             {ANIMATION_WAIT_MS} ms); stop moving it then, or fade instead"
        );
        let f = act.report("motion-reduced-ignored", Some(item), message);
        f.measured = Some(format!("{what} by {by:.1}"));
    }
}
