//! `motion-stagger`, at runtime: the items of a group start one after
//! another with a step longer than `limits::motion::STAGGER_STEP_MAX_MS`,
//! or the last waits longer than `limits::motion::STAGGER_TOTAL_MAX_MS`.
//!
//! A stagger is a delegate's `PauseAnimation` whose duration comes from
//! the row (`index * 120`): the source cannot say what the step comes to,
//! the made rows can. Every `PauseAnimation` one declaration made is
//! grouped, and with three or more distinct durations the step is the
//! smallest gap between them and the wait the largest less the smallest.

use std::collections::BTreeMap;

use ljos_design::limits::motion::{STAGGER_STEP_MAX_MS, STAGGER_TOTAL_MAX_MS};
use ljos_engine::ObjectId;
use ljos_quick::PauseAnimation;

use crate::cx::{Cx, declared_by_app, is_a};

/// No audit looks at more objects than this.
const MOST: usize = 50_000;

/// Every object the scene holds: what its items own, and their items.
fn every_object(cx: &Cx) -> Vec<ObjectId> {
    let e = cx.engine;
    let mut out = Vec::new();
    let Some(mut root) = cx.scene.root() else {
        return out;
    };
    while let Some(o) = e.owner(root) {
        root = o;
    }
    let mut seen = std::collections::HashSet::new();
    let mut stack = vec![root];
    while let Some(o) = stack.pop() {
        if out.len() >= MOST || !e.is_alive(o) || !seen.insert(o) {
            continue;
        }
        out.push(o);
        stack.extend(e.children(o));
        if ljos_quick::is_item(e, o) {
            stack.extend(ljos_quick::child_items(e, o));
        }
    }
    out
}

pub(crate) fn check(cx: &mut Cx) {
    let e = cx.engine;
    let mut groups: BTreeMap<String, Vec<(ObjectId, i64)>> = BTreeMap::new();
    for o in every_object(cx) {
        if !is_a::<PauseAnimation>(e, o) || !declared_by_app(e, o) {
            continue;
        }
        let Some(at) = crate::cx::place_of(e, o) else {
            continue;
        };
        let ms = e.untrack(|| e.read_by_name(o, "duration")).to_number();
        if ms.is_finite() && ms >= 0.0 {
            groups.entry(at).or_default().push((o, ms.round() as i64));
        }
    }
    let mut found = Vec::new();
    for (at, mut pauses) in groups {
        pauses.sort_by_key(|p| p.1);
        let mut values: Vec<i64> = pauses.iter().map(|p| p.1).collect();
        values.dedup();
        if values.len() < 3 {
            continue;
        }
        let step = values.windows(2).map(|w| w[1] - w[0]).min().unwrap_or(0);
        let total = values[values.len() - 1] - values[0];
        let longest = pauses[pauses.len() - 1].0;
        if step > i64::from(STAGGER_STEP_MAX_MS) || total > i64::from(STAGGER_TOTAL_MAX_MS) {
            found.push((longest, at, pauses.len(), step, total));
        }
    }
    for (longest, at, n, step, total) in found {
        let message = format!(
            "the {n} pauses the PauseAnimation at {at} makes start the items {step} ms apart and \
             the last {total} ms after the first, where a stagger steps at most \
             {STAGGER_STEP_MAX_MS} ms and waits at most {STAGGER_TOTAL_MAX_MS} ms in all"
        );
        let f = cx.report("motion-stagger", Some(longest), message);
        f.measured = Some(format!(
            "step {step} ms / {STAGGER_STEP_MAX_MS} ms, total {total} ms / {STAGGER_TOTAL_MAX_MS} ms"
        ));
    }
}
