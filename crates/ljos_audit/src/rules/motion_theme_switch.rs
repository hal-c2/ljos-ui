//! `motion-theme-switch`: switching between light and dark animates a
//! colour for longer than `limits::motion::THEME_SWITCH_MAX_MS`, or shows
//! a colour on the way that is neither the old nor the new.
//!
//! The audit switches the system's colour scheme, as a user does from the
//! desktop, with motion unreduced, and follows what the switch started on
//! the scene's clock; then it switches back. A whole window cross-fading
//! for half a second is a long wait for something done every evening, and
//! a background that goes through white on its way from one dark to
//! another flashes. A flash is a rectangle the application declared whose
//! lightness (OKLab L) goes past both ends by more than
//! `limits::motion::THEME_FLASH_MIN_L`.

use ljos_a11y::ColorScheme;
use ljos_design::color::{Oklab, Srgb};
use ljos_design::limits::motion::{THEME_FLASH_MIN_L, THEME_SWITCH_MAX_MS};
use ljos_engine::{Engine, ObjectId, Value};
use ljos_quick::Rectangle;

use crate::acting::Act;
use crate::cx::{is_a, shown_items};
use crate::motion_watch::{anim_words, own, surely, watch, with_motion};

/// No audit follows more rectangles than this.
const MOST: usize = 256;

fn lightness(e: &Engine, r: ObjectId) -> f64 {
    if !e.is_alive(r) {
        return f64::NAN;
    }
    match e.untrack(|| e.read_by_name(r, "color")) {
        Value::Color(c) if c.a > 0 => {
            let ch = |v: u16| f32::from(v) / 65535.0;
            f64::from(Oklab::from_srgb(Srgb::new(ch(c.r), ch(c.g), ch(c.b))).l)
        }
        _ => f64::NAN,
    }
}

pub(crate) fn check(act: &mut Act) {
    act.rest();
    let e = act.engine;
    let rects: Vec<ObjectId> = shown_items(&act.cx())
        .into_iter()
        .filter(|r| is_a::<Rectangle>(e, *r) && own(e, *r))
        .take(MOST)
        .collect();
    let probe = |e: &Engine| rects.iter().map(|r| lightness(e, *r)).collect::<Vec<_>>();
    let mut long: Vec<(ObjectId, Option<f64>, &'static str)> = Vec::new();
    let mut flashes: Vec<(ObjectId, f64, f64, f64, &'static str)> = Vec::new();
    with_motion(act, false, |act| {
        let held = ljos_quick::hold_system_prefs(e);
        let first = ljos_quick::system_prefs(e).color_scheme;
        let other = match first {
            ColorScheme::Dark => ColorScheme::Light,
            _ => ColorScheme::Dark,
        };
        for (to, words) in [(other, scheme_words(other)), (first, scheme_words(first))] {
            let before = probe(e);
            let w = watch(
                act,
                |a| {
                    let mut prefs = ljos_quick::system_prefs(e);
                    prefs.color_scheme = to;
                    ljos_quick::set_system_prefs(e, prefs);
                    // The controls are given the new scheme once the change
                    // has gone through, still at the time of the switch.
                    a.settle();
                },
                &probe,
            );
            for run in w.own_runs(e) {
                let over = run
                    .ms
                    .is_none_or(|ms| surely(ms) > f64::from(THEME_SWITCH_MAX_MS));
                if over && !long.iter().any(|l| l.0 == run.anim) {
                    long.push((run.anim, run.ms, words));
                }
            }
            let Some((_, end)) = w.samples.last() else {
                continue;
            };
            for (i, r) in rects.iter().enumerate() {
                let (a, b) = (before[i], end[i]);
                if !a.is_finite() || !b.is_finite() || flashes.iter().any(|f| f.0 == *r) {
                    continue;
                }
                let (lo, hi) = (a.min(b), a.max(b));
                let slack = f64::from(THEME_FLASH_MIN_L);
                let worst = w
                    .samples
                    .iter()
                    .filter_map(|(_, v)| v.get(i).copied().filter(|x| x.is_finite()))
                    .map(|x| (x, (lo - x).max(x - hi)))
                    .max_by(|p, q| p.1.total_cmp(&q.1));
                if let Some((x, by)) = worst
                    && by > slack
                {
                    flashes.push((*r, a, x, b, words));
                }
            }
        }
        ljos_quick::restore_system_prefs(e, held);
        act.finish();
    });
    for (anim, ms, to) in long {
        let how = match ms {
            Some(ms) => format!("for about {ms:.0} ms"),
            None => "without end".to_string(),
        };
        let message = format!(
            "switching to {to}, {} runs {how}; a theme switch is instant or takes at most \
             {THEME_SWITCH_MAX_MS} ms",
            anim_words(e, anim)
        );
        let f = act.report("motion-theme-switch", Some(anim), message);
        f.measured = Some(match ms {
            Some(ms) => format!("{ms:.0} ms > {THEME_SWITCH_MAX_MS} ms"),
            None => "endless".into(),
        });
    }
    for (rect, a, x, b, to) in flashes {
        let words = crate::cx::words(&act.cx(), rect);
        let message = format!(
            "switching to {to}, {words} goes from lightness {a:.2} through {x:.2} to {b:.2}: a \
             flash of a colour that is neither theme's; change straight to the new colour"
        );
        let f = act.report("motion-theme-switch", Some(rect), message);
        f.measured = Some(format!("L {x:.2} outside {:.2}..{:.2}", a.min(b), a.max(b)));
    }
}

fn scheme_words(s: ColorScheme) -> &'static str {
    match s {
        ColorScheme::Dark => "dark",
        _ => "light",
    }
}
