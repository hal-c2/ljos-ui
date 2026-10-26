//! `motion-scale-from-zero`, at runtime: a popup the application declared
//! grows from a scale below `limits::motion::ENTER_SCALE_MIN`, as its way
//! in shows it frame by frame.
//!
//! Each open popup is closed and opened again with motion unreduced, and
//! the scale of what it is drawn as is read at every frame: whatever the
//! source binds `from` to, the smallest scale it was shown at is what the
//! user saw. Only a way in that an animation the application declared
//! runs is reported.

use ljos_design::limits::motion::{ENTER_SCALE_MIN, ENTER_SCALE_RECOMMENDED};

use crate::acting::Act;
use crate::motion_watch::{ENTER_PROBES, anim_words, reopened};

pub(crate) fn check(act: &mut Act) {
    let e = act.engine;
    let d = ENTER_PROBES
        .iter()
        .position(|p| *p == "scale")
        .expect("scale is read");
    let mut found = Vec::new();
    for r in reopened(act).iter() {
        let Some(anim) = r.enter.own_runs(e).next().map(|run| run.anim) else {
            continue;
        };
        let scales: Vec<f64> = r
            .enter
            .samples
            .iter()
            .filter_map(|(_, v)| v.get(d).copied().filter(|x| x.is_finite()))
            .collect();
        let (Some(least), Some(end)) = (
            scales.iter().copied().reduce(f64::min),
            scales.last().copied(),
        ) else {
            continue;
        };
        if least < f64::from(ENTER_SCALE_MIN) && end >= f64::from(ENTER_SCALE_MIN) {
            found.push((r.popup, anim, least));
        }
    }
    for (popup, anim, least) in found {
        let words = crate::cx::words(&act.cx(), popup);
        let message = format!(
            "opening {words}, {} grows it from scale {least:.2}, so it seems to come from \
             nowhere; start at {ENTER_SCALE_RECOMMENDED} with a fade",
            anim_words(e, anim)
        );
        let f = act.report("motion-scale-from-zero", Some(anim), message);
        f.measured = Some(format!("scale {least:.2} < {ENTER_SCALE_MIN}"));
    }
}
