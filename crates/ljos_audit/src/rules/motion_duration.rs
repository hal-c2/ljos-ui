//! `motion-duration`, at runtime: an animation the application declared
//! runs longer than its kind allows, as the scene's clock measures it.
//!
//! The source cannot always say how long: a duration bound to a setting
//! or a model, a `SequentialAnimation` of parts, a transition a component
//! is given. So the audit makes the change and times what it started: it
//! closes each open popup and opens it again (`limits::motion::
//! STANDARD_MAX_MS` each way) and checks and unchecks each checkable
//! control (feedback, `limits::motion::MICRO_MAX_MS`). Motion is unreduced
//! while it looks. A run is given the one frame it may have ended before
//! it was seen to. Too short to see is the source's to say: frames of the
//! display cannot tell 20 ms from 40.

use ljos_controls::AbstractButton;
use ljos_design::limits::motion::{MICRO_MAX_MS, STANDARD_MAX_MS};
use ljos_engine::{ObjectId, Value, WriteKind};

use crate::acting::Act;
use crate::cx::{is_a, shown_items, takes_focus};
use crate::motion_watch::{
    Watched, anim_words, no_probe, own, reopen_popups, surely, watch, with_motion,
};

/// No audit flips more than this many controls.
const MOST: usize = 32;

fn set_checked(act: &Act, b: ObjectId, on: bool) {
    let e = act.engine;
    if let Ok(p) = e.property_index(b, "checked") {
        let _ = e.write(b, p, Value::Bool(on), WriteKind::Host);
    }
}

/// The runs of `w` longer than `limit`, as found: the animation, what
/// started it and how long it ran.
fn too_long(
    act: &Act,
    w: &Watched,
    what: &str,
    limit: u32,
    kind: &'static str,
    found: &mut Vec<(ObjectId, String, f64, u32, &'static str)>,
) {
    for run in w.own_runs(act.engine) {
        let Some(ms) = run.ms else { continue };
        if surely(ms) > f64::from(limit) && !found.iter().any(|f| f.0 == run.anim) {
            found.push((run.anim, what.to_string(), ms, limit, kind));
        }
    }
}

pub(crate) fn check(act: &mut Act) {
    let e = act.engine;
    let mut found = Vec::new();
    for r in reopen_popups(act) {
        let words = crate::cx::words(&act.cx(), r.popup);
        too_long(
            act,
            &r.exit,
            &format!("closing {words}"),
            STANDARD_MAX_MS,
            "a popup's way out",
            &mut found,
        );
        too_long(
            act,
            &r.enter,
            &format!("opening {words}"),
            STANDARD_MAX_MS,
            "a popup's way in",
            &mut found,
        );
    }
    let buttons: Vec<ObjectId> = shown_items(&act.cx())
        .into_iter()
        .filter(|b| {
            is_a::<AbstractButton>(e, *b)
                && own(e, *b)
                && takes_focus(e, *b)
                && e.untrack(|| e.read_by_name(*b, "checkable")).to_boolean()
        })
        .take(MOST)
        .collect();
    if !buttons.is_empty() {
        with_motion(act, false, |act| {
            for b in buttons {
                let was = e.untrack(|| e.read_by_name(b, "checked")).to_boolean();
                let words = crate::cx::words(&act.cx(), b);
                let w = watch(act, |a| set_checked(a, b, !was), &no_probe);
                let verb = if was { "unchecking" } else { "checking" };
                too_long(
                    act,
                    &w,
                    &format!("{verb} {words}"),
                    MICRO_MAX_MS,
                    "feedback on a check",
                    &mut found,
                );
                set_checked(act, b, was);
                act.finish();
            }
        });
    }
    for (anim, what, ms, limit, kind) in found {
        let message = format!(
            "{what} runs {} for about {ms:.0} ms, where {kind} takes at most {limit} ms",
            anim_words(e, anim)
        );
        let f = act.report("motion-duration", Some(anim), message);
        f.measured = Some(format!("{ms:.0} ms > {limit} ms"));
    }
}
