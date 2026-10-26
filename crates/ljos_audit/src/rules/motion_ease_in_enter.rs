//! `motion-ease-in-enter`, at runtime: a popup the application declared
//! comes in on an ease-in curve, as its way in measures on the scene's
//! clock.
//!
//! Each open popup is closed and opened again with motion unreduced, and
//! its opacity, scale and place are read at every frame of the way in.
//! Halfway through the time, a linear or an in-out curve is halfway there
//! and an ease-out curve further; an ease-in curve is less than
//! `limits::motion::EASE_IN_HALFWAY_MAX` of the way (a quadratic one a
//! quarter). That holds whatever the source binds `easing.type` to, and
//! for a transition a component is given. Only a way in that an animation
//! the application declared runs is reported.

use ljos_design::limits::motion::EASE_IN_HALFWAY_MAX;

use crate::acting::Act;
use crate::motion_watch::{ENTER_PROBES, Watched, anim_words, period_ms, reopened};

/// How far along `d` the probe is at half the time it took, with what it
/// changed by, or `None` when it did not change or took too few frames to
/// tell.
pub(crate) fn halfway(w: &Watched, d: usize) -> Option<(f64, f64)> {
    let values: Vec<(f64, f64)> = w
        .samples
        .iter()
        .filter_map(|(t, v)| v.get(d).copied().filter(|x| x.is_finite()).map(|x| (*t, x)))
        .collect();
    let (&(_, v0), &(_, end)) = (values.first()?, values.last()?);
    let change = end - v0;
    let least = if d < 2 { 0.05 } else { 2.0 };
    if change.abs() < least {
        return None;
    }
    let near = change.abs() * 1e-3;
    // When it got there: the first frame from which it stays.
    let mut t_end = values.last()?.0;
    for (t, v) in values.iter().rev() {
        if (v - end).abs() > near {
            break;
        }
        t_end = *t;
    }
    if t_end < 3.0 * period_ms() {
        return None;
    }
    let mid = t_end / 2.0;
    let at = values.windows(2).find_map(|p| {
        let ((ta, va), (tb, vb)) = (p[0], p[1]);
        (ta <= mid && mid <= tb).then(|| {
            if tb > ta {
                va + (vb - va) * (mid - ta) / (tb - ta)
            } else {
                va
            }
        })
    })?;
    Some(((at - v0) / change, change))
}

pub(crate) fn check(act: &mut Act) {
    let e = act.engine;
    let mut found = Vec::new();
    for r in reopened(act).iter() {
        let Some(anim) = r.enter.own_runs(e).next().map(|run| run.anim) else {
            continue;
        };
        let slowest = (0..ENTER_PROBES.len())
            .filter_map(|d| halfway(&r.enter, d).map(|(p, _)| (d, p)))
            .min_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((d, p)) = slowest
            && p < f64::from(EASE_IN_HALFWAY_MAX)
        {
            found.push((r.popup, anim, ENTER_PROBES[d], p));
        }
    }
    for (popup, anim, what, p) in found {
        let words = crate::cx::words(&act.cx(), popup);
        let message = format!(
            "opening {words}, {} has its {what} {:.0} % of the way at half the time: an ease-in \
             curve, which starts slow just as the user looks for an answer; ease out to enter",
            anim_words(e, anim),
            p * 100.0
        );
        let f = act.report("motion-ease-in-enter", Some(anim), message);
        f.measured = Some(format!(
            "{:.0} % at half time < {:.0} %",
            p * 100.0,
            EASE_IN_HALFWAY_MAX * 100.0
        ));
    }
}
