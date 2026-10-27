//! `motion-frequent`, at runtime: something done many times a day runs
//! an animation the application declared for longer than
//! `limits::motion::FREQUENT_MAX_MS`, as the scene's clock measures it.
//!
//! The audit does what a user does many times a day and times what it
//! started: it gives each Tab stop keyboard focus, and moves each tab bar
//! to another tab and back. A `Behavior` on a colour that focus changes,
//! or a page that slides in on every tab switch, is found whatever the
//! source binds its duration to. Motion is unreduced while it looks, and
//! focus and the tabs are put back.

use ljos_controls::TabBar;
use ljos_design::limits::motion::FREQUENT_MAX_MS;
use ljos_engine::{ObjectId, Value, WriteKind};
use ljos_quick::focus::{FocusReason, force_active_focus};

use crate::acting::{Act, hold_focus, restore_focus};
use crate::cx::{declared_by_app, is_a, shown_items};
use crate::focus_walk::rounds;
use crate::motion_watch::{Watched, anim_words, no_probe, surely, watch, with_motion};

/// No audit focuses more than this many stops.
const MOST: usize = 64;

fn too_long(act: &Act, w: &Watched, what: String, found: &mut Vec<(ObjectId, String, f64)>) {
    for run in w.own_runs(act.engine) {
        let Some(ms) = run.ms else { continue };
        if surely(ms) > f64::from(FREQUENT_MAX_MS) && !found.iter().any(|f| f.0 == run.anim) {
            found.push((run.anim, what.clone(), ms));
        }
    }
}

fn set_index(act: &Act, bar: ObjectId, i: f64) {
    let e = act.engine;
    if let Ok(p) = e.property_index(bar, "currentIndex") {
        let _ = e.write(bar, p, Value::Real(i), WriteKind::Host);
    }
}

pub(crate) fn check(act: &mut Act) {
    act.rest();
    let e = act.engine;
    let Some(root) = act.scene.root() else { return };
    let (stops, bars): (Vec<(ObjectId, ObjectId)>, Vec<ObjectId>) = {
        let cx = act.cx();
        let stops = rounds(&cx)
            .into_iter()
            .flat_map(|(scope, round)| round.into_iter().map(move |s| (scope.unwrap_or(root), s)))
            .take(MOST)
            .collect();
        let bars = shown_items(&cx)
            .into_iter()
            .filter(|b| is_a::<TabBar>(e, *b) && declared_by_app(e, *b))
            .filter(|b| e.untrack(|| e.read_by_name(*b, "count")).to_number() >= 2.0)
            .collect();
        (stops, bars)
    };
    if stops.is_empty() && bars.is_empty() {
        return;
    }
    let held = hold_focus(act);
    let mut found = Vec::new();
    with_motion(act, false, |act| {
        for (away, stop) in stops {
            if !e.is_alive(stop) {
                continue;
            }
            force_active_focus(e, away, FocusReason::Other);
            act.rest();
            let words = crate::cx::words(&act.cx(), stop);
            let w = watch(
                act,
                |a| force_active_focus(a.engine, stop, FocusReason::Tab),
                &no_probe,
            );
            too_long(
                act,
                &w,
                format!("giving {words} keyboard focus"),
                &mut found,
            );
        }
        for bar in bars {
            let was = e
                .untrack(|| e.read_by_name(bar, "currentIndex"))
                .to_number();
            let to = if was == 0.0 { 1.0 } else { 0.0 };
            let words = crate::cx::words(&act.cx(), bar);
            let w = watch(act, |a| set_index(a, bar, to), &no_probe);
            too_long(act, &w, format!("switching tabs in {words}"), &mut found);
            set_index(act, bar, was);
            act.finish();
        }
    });
    restore_focus(act, &held);
    for (anim, what, ms) in found {
        let message = format!(
            "{what} runs {} for about {ms:.0} ms; something done many times a day is instant or \
             takes at most {FREQUENT_MAX_MS} ms",
            anim_words(e, anim)
        );
        let f = act.report("motion-frequent", Some(anim), message);
        f.measured = Some(format!("{ms:.0} ms > {FREQUENT_MAX_MS} ms"));
    }
}
